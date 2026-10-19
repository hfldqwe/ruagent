//! Boot-time orphan sweep (issue #44): a daemon that dies — crash,
//! kill, power loss, even a clean exit — leaves its agent child
//! processes running with nobody reading their output, burning tokens
//! under run rows already marked interrupted. The SDK's child guard
//! only fires on an in-process drop; process death never runs it, and
//! Windows does not reparent or terminate children of a dead parent.
//!
//! Fix: each daemon records its pid + start time under the data root.
//! The next boot reads the prior record and kills its surviving
//! children — agent processes and orphaned stdio MCP servers alike.
//!
//! PID-reuse safety: if the recorded pid is no longer alive, every
//! process parented to it is a true orphan (the parent is genuinely
//! gone), so killing them cannot hit a stranger. If the pid IS alive,
//! either the prior daemon is still running on this root (another
//! instance) or the pid was reused by an unrelated process — in both
//! cases the sweep is skipped and orphans are accepted rather than
//! killing someone else's children.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// The recorded prior daemon: `<pid> <started_at_unix_ms>`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PriorDaemon {
    pub pid: u32,
    pub started_at_ms: i64,
}

/// Where the current daemon's identity lives.
pub fn record_path(root: &Path) -> PathBuf {
    root.join("data").join("daemon.pid")
}

/// Read the prior record (if any) and whether that daemon is verifiably
/// still alive. `None` = first boot on this root.
pub fn read_prior(root: &Path) -> Result<Option<PriorDaemon>> {
    let path = record_path(root);
    let Some(text) = std::fs::read_to_string(&path).ok() else {
        return Ok(None);
    };
    let mut it = text.split_whitespace();
    let (Some(pid), Some(started)) = (it.next(), it.next()) else {
        // Unreadable garbage (a torn write): treat as absent.
        return Ok(None);
    };
    let (Ok(pid), Ok(started)) = (pid.parse::<u32>(), started.parse::<i64>()) else {
        return Ok(None);
    };
    Ok(Some(PriorDaemon {
        pid,
        started_at_ms: started,
    }))
}

/// Record this daemon for the next boot.
pub fn write_current(root: &Path) -> Result<()> {
    let path = record_path(root);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    let pid = std::process::id();
    let started = process_started_at_ms(pid).unwrap_or(0);
    // temp + rename, same discipline as agents.toml: a torn write must
    // never corrupt the record.
    let tmp = path.with_extension("pid.tmp");
    std::fs::write(&tmp, format!("{pid} {started}"))
        .with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}

/// The boot sequence: join the kill-on-death job (Windows), sweep the
/// prior daemon's orphans, then record this one. Errors are logged,
/// never fatal — a sweep problem must not stop the daemon from
/// starting.
pub fn boot(root: &Path) {
    #[cfg(windows)]
    if !join_kill_on_death_job() {
        tracing::warn!("kill-on-death job unavailable — falling back to the boot sweep only");
    }
    match read_prior(root) {
        Ok(Some(prior)) if prior.pid != std::process::id() => {
            if let Some(start) = process_started_at_ms(prior.pid) {
                if start == prior.started_at_ms {
                    tracing::warn!(
                        pid = prior.pid,
                        "previous daemon still running on this root — skipping orphan sweep"
                    );
                } else {
                    tracing::warn!(
                        pid = prior.pid,
                        "recorded pid reused by another process — skipping orphan sweep"
                    );
                }
            } else {
                let pids = child_pids_of(prior.pid);
                if !pids.is_empty() {
                    // The boot sweep is a reclamation like any other, so it
                    // reports the same reading (issue #60): how many it had to
                    // kill, how many were already gone, and -- the one that
                    // matters -- how many SURVIVED.
                    let out = reclaim(&pids, "boot sweep of the previous daemon orphans");
                    tracing::info!(
                        parent = prior.pid,
                        killed = out.killed,
                        already_gone = out.already_gone,
                        failed = out.failed,
                        "orphaned agent processes from the previous daemon swept"
                    );
                }
            }
        }
        _ => {}
    }
    if let Err(e) = write_current(root) {
        tracing::warn!(error = %e, "recording daemon pid failed");
    }
}

/// Kill one process and its children. Agents run behind wrapper
/// launchers (`npx → node`), so this must reach the whole tree.
#[cfg(unix)]
pub fn kill_tree(pid: u32) -> bool {
    // Agents run as their own process-group leaders (the SDK spawns
    // them that way): kill the group to reach the grandchildren.
    // Fallback to the pid itself for non-leaders.
    (unsafe { libc::kill(-(pid as i32), libc::SIGKILL) == 0 })
        || (unsafe { libc::kill(pid as i32, libc::SIGKILL) == 0 })
}

#[cfg(windows)]
pub fn kill_tree(pid: u32) -> bool {
    // /T: taskkill walks the tree (wrappers + grandchildren).
    std::process::Command::new("taskkill")
        .args(["/F", "/T", "/PID", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(not(any(unix, windows)))]
pub fn kill_tree(pid: u32) -> bool {
    let _ = pid;
    false
}

// ---------------------------------------------------------------------------
// Reclamation: a COUNTED teardown (issue #60)
// ---------------------------------------------------------------------------
//
// WHY THIS IS NOT A RENAME OF `kill_tree`. `kill_tree` answers "did the kill
// CALL succeed"; a teardown has to answer "is the child GONE". Those are
// different questions, and the difference is the defect issue #60 names: the
// SDK's ChildGuard fires on an in-process drop, NOTHING waits for it, and a
// child that survived one teardown left no reading anywhere. t7 measured 10
// leftover `ruagent-mock-agent.exe` after one e2e run, 1 leaked in each of 2 runs
// out of 3 -- and "no failure was logged" is perfectly consistent with that.
//
// So a reclamation REPORTS, per pid, which of three things happened:
//
//   * `killed`       -- it was ALIVE, we killed it, and it is gone now.
//   * `already_gone` -- it was already gone when we looked: the healthy case
//                       (the agent honoured the shutdown and exited by itself).
//                       NOT a failure, and NOT a kill -- folding it into
//                       `killed` would make the reading lie in the common case.
//   * `failed`       -- it is STILL ALIVE after the kill attempt: it refused to
//                       die. Reclaim::clean() is false exactly here, and this is
//                       the count a test must be able to assert as zero.
//
// Windows has no process-group kill: termination goes through `kill_tree`
// (taskkill /F /T), so a wrapper launcher's descendants go with it.

/// What one reclamation concluded about a set of pids.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Reclaim {
    /// Alive when we looked, killed by us, verified gone.
    pub killed: usize,
    /// Already exited when we looked -- the healthy case, not a failure.
    pub already_gone: usize,
    /// STILL ALIVE after the kill attempt. It refused to die.
    pub failed: usize,
}

impl Reclaim {
    /// Pids this pass looked at.
    pub fn total(&self) -> usize {
        self.killed + self.already_gone + self.failed
    }

    /// True when nothing survived. A caller that must not leak asserts THIS,
    /// not total() == 0: a pass that killed three children is clean.
    pub fn clean(&self) -> bool {
        self.failed == 0
    }
}

impl std::fmt::Display for Reclaim {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "killed={} already_gone={} failed={}",
            self.killed, self.already_gone, self.failed
        )
    }
}

/// How long a killed process may take to leave the process table before it
/// counts as failed. taskkill returns once the kernel has terminated the
/// process, but its table entry can outlive the call -- so a single check
/// immediately after the kill would report healthy kills as failures.
const EXIT_GRACE: std::time::Duration = std::time::Duration::from_millis(1000);
const EXIT_POLL: std::time::Duration = std::time::Duration::from_millis(25);

/// Reclaim these pids (each with its tree) and LOG the reading.
///
/// BLOCKING: it waits (up to EXIT_GRACE) for a killed process to disappear, so
/// an async caller must wrap it in spawn_blocking.
pub fn reclaim(pids: &[u32], what: &str) -> Reclaim {
    let out = reclaim_with(
        pids,
        EXIT_GRACE,
        &|pid| process_started_at_ms(pid).is_some(),
        &kill_tree,
    );
    if out.total() > 0 {
        if out.clean() {
            tracing::info!(
                what,
                killed = out.killed,
                already_gone = out.already_gone,
                failed = out.failed,
                "child processes reclaimed"
            );
        } else {
            tracing::warn!(
                what,
                killed = out.killed,
                already_gone = out.already_gone,
                failed = out.failed,
                "child processes reclaimed WITH SURVIVORS -- these are still running"
            );
        }
    } else {
        tracing::debug!(what, "nothing to reclaim");
    }
    out
}

/// The reclamation rule, with liveness and the kill injected so all three
/// outcomes are testable without a process that genuinely refuses to die.
///
/// A pid that is gone when we look is already_gone. Otherwise we kill it and
/// wait up to grace for it to leave the table: gone means killed, still there
/// means failed. A kill that returns false for a process that IS gone is still
/// killed -- the question is whether it died, not whether the call succeeded.
fn reclaim_with(
    pids: &[u32],
    grace: std::time::Duration,
    alive: &dyn Fn(u32) -> bool,
    kill: &dyn Fn(u32) -> bool,
) -> Reclaim {
    let mut out = Reclaim::default();
    for pid in pids {
        if !alive(*pid) {
            out.already_gone += 1;
            continue;
        }
        let _ = kill(*pid);
        let deadline = std::time::Instant::now() + grace;
        let mut gone = !alive(*pid);
        while !gone && std::time::Instant::now() < deadline {
            std::thread::sleep(EXIT_POLL);
            gone = !alive(*pid);
        }
        if gone {
            out.killed += 1;
        } else {
            out.failed += 1;
            tracing::warn!(pid, "child process survived its kill and is STILL alive");
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Attribution: whose child is that? (issue #60)
// ---------------------------------------------------------------------------

/// Our own LIVE direct children, right now. The snapshot primitive: take one
/// before a session spawns and one after, and the difference is what that
/// session (or anything else that ran concurrently) created.
pub fn live_children_of_self() -> Vec<u32> {
    live_children_of(std::process::id())
}

/// Live direct children of parent.
pub fn live_children_of(parent: u32) -> Vec<u32> {
    child_pids_of(parent)
        .into_iter()
        .filter(|pid| process_started_at_ms(*pid).is_some())
        .collect()
}

/// The live children that appeared since the snapshot before.
pub fn children_since(before: &[u32]) -> Vec<u32> {
    live_children_of_self()
        .into_iter()
        .filter(|pid| !before.contains(pid))
        .collect()
}

/// The image (executable) name of a live process -- the SECOND attribution
/// tool, and the one that still works when the window is not the only thing that
/// has spawned.
///
/// WHY A WINDOW ALONE IS NOT ENOUGH. Closing a chat must never kill a child that
/// another live session -- a run, another chat -- started in the same window,
/// and "it appeared after my snapshot" cannot tell those apart. The image name
/// can, when the program is distinctive: the child this session started carries
/// ITS program's file name. Attribution therefore uses the narrow tool when the
/// wide one is ambiguous, and GIVES UP (rather than guessing) when both leave
/// more than one candidate.
pub fn image_name(pid: u32) -> Option<String> {
    #[cfg(windows)]
    return windows_image_name(pid);
    #[cfg(unix)]
    return unix_image_name(pid);
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        None
    }
}

/// Those of the given pids whose image name matches the FILE NAME of the given
/// program. Case-insensitive on Windows, exact elsewhere; a program written with
/// arguments (as an agent card may write it) is compared by its first word.
pub fn matching_image(pids: &[u32], program: &str) -> Vec<u32> {
    let first = program.split_whitespace().next().unwrap_or(program);
    let want = std::path::Path::new(first)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| first.to_string());
    if want.is_empty() {
        return Vec::new();
    }
    pids.iter()
        .copied()
        .filter(|pid| match image_name(*pid) {
            Some(name) => {
                #[cfg(windows)]
                {
                    name.eq_ignore_ascii_case(&want)
                }
                #[cfg(not(windows))]
                {
                    name == want
                }
            }
            None => false,
        })
        .collect()
}

#[cfg(windows)]
fn windows_image_name(pid: u32) -> Option<String> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };

    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap.is_null() || snap as isize == -1 {
            return None;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut found = None;
        if Process32FirstW(snap, &mut entry) != 0 {
            loop {
                if entry.th32ProcessID == pid {
                    let end = entry
                        .szExeFile
                        .iter()
                        .position(|c| *c == 0)
                        .unwrap_or(entry.szExeFile.len());
                    found = Some(String::from_utf16_lossy(&entry.szExeFile[..end]));
                    break;
                }
                if Process32NextW(snap, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snap);
        found
    }
}

#[cfg(unix)]
fn unix_image_name(pid: u32) -> Option<String> {
    let exe = std::fs::read_link(format!("/proc/{pid}/exe")).ok()?;
    exe.file_name().map(|n| n.to_string_lossy().to_string())
}

/// Put this daemon into a job object with KILL_ON_JOB_CLOSE. Children
/// of a job member join the job automatically — so every agent,
/// wrapper launcher (`npx → node`) and MCP server the daemon ever
/// spawns lands in it, and the OS terminates them all when the daemon
/// process dies, whatever the death mode (crash, kill, exit). This is
/// the primary Windows mechanism; the boot sweep below is the fallback
/// for the job-unavailable case. The handle intentionally leaks: it
/// must stay open for the daemon's whole life — process death closes
/// it, which is exactly the trigger.
#[cfg(windows)]
pub fn join_kill_on_death_job() -> bool {
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            return false;
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &mut info as *mut _ as *mut core::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        ) == 0
        {
            return false;
        }
        // GetCurrentProcess is a pseudo-handle; assignment is enough.
        AssignProcessToJobObject(job, GetCurrentProcess()) != 0
    }
}

#[cfg(not(windows))]
#[allow(dead_code)]
fn join_kill_on_death_job() -> bool {
    false
}

// ---------------------------------------------------------------------------
// Platform: process table
// ---------------------------------------------------------------------------

/// Start time of a pid in unix milliseconds, if it is alive.
pub fn process_started_at_ms(pid: u32) -> Option<i64> {
    #[cfg(windows)]
    return windows_process_start(pid);
    #[cfg(unix)]
    return unix_process_start(pid);
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        None
    }
}

/// Pids whose parent is `parent`. Only meaningful when `parent` is
/// dead (see the module doc for the reuse-safety argument).
pub fn child_pids_of(parent: u32) -> Vec<u32> {
    #[cfg(windows)]
    return windows_children(parent);
    #[cfg(unix)]
    return unix_children(parent);
    #[cfg(not(any(unix, windows)))]
    {
        let _ = parent;
        Vec::new()
    }
}

#[cfg(windows)]
fn windows_children(parent: u32) -> Vec<u32> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };

    let mut out = Vec::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap.is_null() || snap as isize == -1 {
            return out;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        if Process32FirstW(snap, &mut entry) != 0 {
            loop {
                if entry.th32ParentProcessID == parent && entry.th32ProcessID != parent {
                    out.push(entry.th32ProcessID);
                }
                if Process32NextW(snap, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snap);
    }
    out
}

#[cfg(windows)]
fn windows_process_start(pid: u32) -> Option<i64> {
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        // A terminated process keeps its pid until every handle closes:
        // query the exit code so "alive" means alive, not zombie.
        let mut exit: u32 = 0;
        let alive = GetExitCodeProcess(handle, &mut exit) != 0 && exit == 259; // STILL_ACTIVE
        let (mut created, mut exit_t, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        let ok = GetProcessTimes(handle, &mut created, &mut exit_t, &mut kernel, &mut user);
        CloseHandle(handle);
        if ok == 0 || !alive {
            return None;
        }
        // FILETIME: 100ns ticks since 1601-01-01 → unix ms.
        let ticks = ((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64;
        Some((ticks / 10_000) as i64 - 11_644_473_600_000)
    }
}

#[cfg(unix)]
fn unix_children(parent: u32) -> Vec<u32> {
    let mut out = Vec::new();
    let Ok(dir) = std::fs::read_dir("/proc") else {
        return out;
    };
    for entry in dir.flatten() {
        let Some(name) = entry
            .file_name()
            .to_str()
            .and_then(|s| s.parse::<u32>().ok())
        else {
            continue;
        };
        if read_stat_field(name, 4).and_then(|f| f.parse::<u32>().ok()) == Some(parent) {
            out.push(name);
        }
    }
    out
}

#[cfg(unix)]
fn unix_process_start(pid: u32) -> Option<i64> {
    // stat field 22 (starttime): clock ticks since boot.
    let ticks: i64 = read_stat_field(pid, 22)?.parse().ok()?;
    let hz: i64 = 100; // USER_HZ on every mainstream linux
    let boot_secs = std::fs::read_to_string("/proc/stat")
        .ok()?
        .lines()
        .find(|l| l.starts_with("btime "))
        .and_then(|l| {
            l.split_whitespace()
                .nth(1)
                .and_then(|v| v.parse::<i64>().ok())
        })?;
    Some(boot_secs * 1000 + ticks * 1000 / hz)
}

#[cfg(unix)]
fn read_stat_field(pid: u32, field: usize) -> Option<String> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // The comm field can contain spaces/parens — fields are counted
    // AFTER the closing paren of comm.
    let after = stat.rsplit(')').next()?;
    after.split_whitespace().nth(field - 3).map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// All THREE outcomes through the rule itself. Liveness and the kill are
    /// injected precisely BECAUSE the third one (a process that refuses to die)
    /// cannot be summoned portably: taskkill /F on Windows and SIGKILL on unix
    /// both win against an ordinary process, so a genuinely unkillable child is
    /// not something a test can arrange -- and an untested branch is how "we
    /// tried" turns back into a claim nobody can check.
    ///
    /// Note what the injection also pins: with liveness constant, the pid whose
    /// kill CALL returned true still survives, and is counted as a SURVIVOR --
    /// the reading follows the process table, not the killer's exit code.
    #[test]
    fn reclaim_classifies_killed_already_gone_and_refused() {
        let calls: std::sync::Mutex<Vec<u32>> = Default::default();
        let alive = |pid: u32| pid == 10 || pid == 30;
        let out = reclaim_with(&[10, 20, 30], std::time::Duration::ZERO, &alive, &|pid| {
            calls.lock().unwrap().push(pid);
            pid == 10
        });
        println!("READING t16 injected three ways: {out}");
        assert_eq!(
            (out.killed, out.already_gone, out.failed),
            (0, 1, 2),
            "20 was already gone; 10 and 30 never left the table: {out}"
        );
        assert!(!out.clean(), "survivors make the pass unclean");
        assert_eq!(calls.lock().unwrap().len(), 2, "the dead pid is not killed");
    }

    /// The same rule with death actually happening on the kill.
    #[test]
    fn reclaim_counts_a_process_that_dies_as_killed() {
        let dead: std::sync::Mutex<Vec<u32>> = Default::default();
        let out = reclaim_with(
            &[10, 20],
            std::time::Duration::from_millis(200),
            &|pid| pid == 10 && !dead.lock().unwrap().contains(&10),
            &|pid| {
                dead.lock().unwrap().push(pid);
                true
            },
        );
        println!("READING t16 injected kill works: {out}");
        assert_eq!(
            (out.killed, out.already_gone, out.failed),
            (1, 1, 0),
            "{out}"
        );
        assert!(out.clean(), "a kill plus a natural exit is clean");
        assert_eq!(out.total(), 2, "both pids are accounted for");
    }

    /// The NARROW attribution tool: an image name identifies a child even when
    /// the window contains other sessions' spawns, and it does not match a
    /// process that is something else -- so a close reclaims the agent THIS
    /// session started without reaching a concurrent run's child.
    #[test]
    fn the_image_name_narrows_attribution_to_the_programs_own_children() {
        let mut child = sleeper();
        let pid = child.id();
        #[cfg(windows)]
        let (mine, other) = ("ping.exe", "explorer.exe");
        #[cfg(unix)]
        let (mine, other) = ("sleep", "init");
        // Case-INSENSITIVE: the process table reports the image in the case the
        // executable was launched with (PING.EXE here), and the comparison that
        // matters downstream is the case-insensitive one.
        assert!(
            image_name(pid)
                .map(|n| n.eq_ignore_ascii_case(mine))
                .unwrap_or(false),
            "the snapshot reports the image of a live child: {:?}",
            image_name(pid)
        );
        assert_eq!(
            matching_image(&[pid], mine),
            vec![pid],
            "the program's own file name matches"
        );
        assert_eq!(
            matching_image(&[pid], &format!("/somewhere/else/{mine} --arg 1")),
            vec![pid],
            "a card's command line is compared by its program's FILE NAME"
        );
        assert!(
            matching_image(&[pid], other).is_empty(),
            "another program's name does NOT match it"
        );
        assert!(
            matching_image(&[pid], "").is_empty(),
            "an empty program matches nothing, rather than everything"
        );
        let out = reclaim(&[pid], "test: image name helper");
        assert_eq!(out.killed, 1, "{out}");
        let _ = child.wait();
    }

    /// The real thing, against the real process table: a child we spawn is seen,
    /// killed, VERIFIED gone, and only then reported as already gone.
    #[test]
    fn a_real_child_is_reclaimed_and_verified_gone() {
        let mut child = sleeper();
        let pid = child.id();
        assert!(
            live_children_of_self().contains(&pid),
            "the freshly spawned child {pid} is one of our live children"
        );

        let out = reclaim(&[pid], "test: one real child");
        println!("READING t16 reclaim one real child: {out}");
        assert_eq!(
            (out.killed, out.already_gone, out.failed),
            (1, 0, 0),
            "an alive child is killed, and only a VERIFIED kill counts: {out}"
        );
        assert!(out.clean());
        assert!(
            process_started_at_ms(pid).is_none(),
            "reclaim verified it is gone, so this must agree"
        );
        assert!(
            !live_children_of_self().contains(&pid),
            "and it is no longer among our live children"
        );
        let _ = child.wait();

        let again = reclaim(&[pid], "test: the same pid twice");
        println!("READING t16 reclaim the same pid twice: {again}");
        assert_eq!(
            (again.killed, again.already_gone, again.failed),
            (0, 1, 0),
            "a child that already exited is NOT counted as a kill: {again}"
        );
    }

    /// A long-running child this process owns. Portable and needs no build
    /// artefact: ping and sleep both just wait.
    fn sleeper() -> std::process::Child {
        #[cfg(windows)]
        let mut cmd = {
            let mut c = std::process::Command::new("ping");
            c.args(["-n", "30", "127.0.0.1"]);
            c
        };
        #[cfg(unix)]
        let mut cmd = {
            let mut c = std::process::Command::new("sleep");
            c.arg("30");
            c
        };
        cmd.stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawning a sleeper child")
    }
}
