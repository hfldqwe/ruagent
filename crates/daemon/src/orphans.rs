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
                let killed = child_pids_of(prior.pid)
                    .into_iter()
                    .filter(|pid| kill_tree(*pid))
                    .count();
                if killed > 0 {
                    tracing::info!(
                        parent = prior.pid,
                        killed,
                        "orphaned agent processes from the previous daemon cleaned up"
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
