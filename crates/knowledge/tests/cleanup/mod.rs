//! Test-root cleanup for the knowledge crate's test binaries (t31).
//!
//! THE SHAPE is the one t27 fixed for the daemon's t16 tests and t30 spread to four more suites.
//! Three parts, and a suite needs all three:
//!   1. a BOUNDED retry that RETURNS the reason, so a failure can be reported or asserted;
//!   2. a REPORTING call site for a root whose holder is the test process itself (the in-test
//!      Knowledge handle keeps a LanceDB/SQLite file open inside the root, so the removal CANNOT
//!      succeed from inside the test);
//!   3. a PRUNER that removes, from the NEXT process, what the previous one could not.
//!
//! WHY THIS MODULE IS A COPY of the daemon's crate-internal one: an integration-test binary
//! compiles against the public API of the crate under test only, and ruagent-knowledge does not
//! depend on ruagent-daemon (the direction is the other way around), so the daemon's
//! process_started_at_ms primitive is not available here. The liveness question is answered
//! locally instead: /proc on unix, ONE parsed tasklist call on Windows. One copy for the whole
//! directory rather than one per test file, because a subdirectory of tests/ is not itself a test
//! binary.
#![allow(dead_code)] // each test binary uses a different subset

use std::path::Path;
use std::time::Duration;

/// Remove a root with a bounded retry, RETURNING the reason (t31).
///
/// It returns instead of printing so a caller can assert on it, and it polls instead of sleeping
/// once because what it waits for is another task dropping a handle.
pub fn try_remove_test_root(root: &Path, budget: Duration) -> Result<(), String> {
    let started = std::time::Instant::now();
    loop {
        match std::fs::remove_dir_all(root) {
            Ok(()) => {
                if started.elapsed() > Duration::from_millis(250) {
                    println!(
                        "READING t31 cleanup of {} waited {:.1} s for the handles to be released",
                        root.display(),
                        started.elapsed().as_secs_f64()
                    );
                }
                return Ok(());
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => {
                if started.elapsed() >= budget {
                    return Err(format!(
                        "{} could NOT be removed within {:.2} s: {e}",
                        root.display(),
                        budget.as_secs_f64()
                    ));
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        }
    }
}

/// Remove a root test code OWNS, or PANIC naming it (t31).
pub fn remove_test_root(root: &Path, what: &str, budget: Duration) {
    if let Err(why) = try_remove_test_root(root, budget) {
        panic!("{what}: {why}");
    }
}

/// Try once, then REPORT -- never silently drop -- a root that is still held (t31).
///
/// WHY NOT AN ASSERTION HERE. The holder is the test process itself: the Knowledge handle lives
/// for the whole test, so the removal cannot succeed however long it waits, and a panic would red
/// a test for a fact that is not its own. The failure is made VISIBLE instead, and the pile is
/// bounded by the pruner below -- which this call also guarantees has run.
pub fn report_test_root_left_behind(root: &Path) {
    prune_stale_test_roots(TEST_ROOT_PREFIXES, "knowledge");
    if let Err(why) = try_remove_test_root(root, Duration::from_millis(500)) {
        eprintln!(
            "READING t31 cleanup: {why} -- the root is held by THIS test process, so the failure \
             is REPORTED rather than swallowed; the next run's pruner removes it"
        );
    }
}

/// Every temp-root prefix the knowledge test binaries create, in ONE place.
pub const TEST_ROOT_PREFIXES: &[&str] = &[
    "ruagent-t250-legs-",
    "ruagent-t5-kb-weight-",
    "ruagent-t5-kb-off-",
    "ruagent-t5-kb-alloff-",
    "ruagent-t7-window-",
    "ruagent-t7-cjk-",
    "ruagent-t7-resid-",
    "ruagent-t245-",
    "ruagent-t293-",
    "ruagent-kbfiles-",
    "ruagent-kbcorpus-",
    "ra-t29-",
];

/// Remove the roots of DEAD test processes for the given prefixes, once per label.
///
/// THE SAFETY RULE, and why it is deliberately conservative: a root belongs to a run whose process
/// may still be read while it lives, so a root is pruned only when it has at least one numeric
/// field AND none of its numeric fields belongs to a live process. Root names put the pid in
/// different positions (tag first, or pid first), so the question is asked of every number in the
/// name; a number that happens to be a live process keeps the root, which costs disk and never a
/// run. Files are left alone: only directories are candidates.
pub fn prune_stale_test_roots(prefixes: &[&str], label: &str) {
    static DONE: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    {
        let mut done = DONE.lock().expect("prune labels");
        if done.iter().any(|l| l == label) {
            return;
        }
        done.push(label.to_string());
    }
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    let me = std::process::id();
    let (mut pruned, mut failed) = (0usize, 0usize);
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if !prefixes.iter().any(|p| name.starts_with(p)) {
            continue;
        }
        let path = e.path();
        if !path.is_dir() || !all_numeric_fields_are_dead(&name, me) {
            continue;
        }
        match std::fs::remove_dir_all(&path) {
            Ok(()) => pruned += 1,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                failed += 1;
                eprintln!(
                    "READING t31 stale root {} could NOT be removed: {err}",
                    path.display()
                );
            }
        }
    }
    if pruned > 0 || failed > 0 {
        eprintln!(
            "READING t31 pruned {pruned} stale {label} root(s) of dead test processes; {failed} could not be removed"
        );
    }
}

/// True when every number in a root name is safe to prune on, and there is at least one.
pub fn all_numeric_fields_are_dead(name: &str, me: u32) -> bool {
    all_numeric_fields_are_dead_with(name, me, pid_is_alive)
}

/// The rule with the liveness question passed IN, so the test can answer it
/// deterministically instead of depending on which pids this machine happens to have
/// live: Windows REUSES pids, and a reused one made the dead-pid case vacuous-red.
fn all_numeric_fields_are_dead_with(name: &str, me: u32, alive: impl Fn(u32) -> bool) -> bool {
    let mut seen = false;
    for field in name.split(|c: char| !c.is_ascii_digit()) {
        if field.is_empty() {
            continue;
        }
        let Ok(n) = field.parse::<u32>() else {
            continue;
        };
        // A number outside this range is NOT evidence about a run: below 1000 it is
        // almost always part of a NAME (the 2 in ruagent-e2e-mock-bins, the 329 in a
        // tag), and above 100_000_000 a nanosecond stamp cannot be a pid. A name with no
        // pid in it is never pruned, however many of these numbers it carries.
        if !(1000..=100_000_000).contains(&n) {
            continue;
        }
        seen = true;
        if n == me || alive(n) {
            return false;
        }
    }
    seen
}

/// Is a process with this pid alive? /proc on unix, one parsed tasklist call on Windows.
pub fn pid_is_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        Path::new(&format!("/proc/{pid}")).exists()
    }
    #[cfg(windows)]
    {
        LIVE_PIDS.get_or_init(live_pids).contains(&pid)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        true
    }
}

#[cfg(windows)]
static LIVE_PIDS: std::sync::OnceLock<std::collections::HashSet<u32>> = std::sync::OnceLock::new();

/// One tasklist call, parsed. An EMPTY set means the tool could not be read, and the caller then
/// refuses to prune at all (an empty oracle makes every root look dead).
#[cfg(windows)]
fn live_pids() -> std::collections::HashSet<u32> {
    use std::collections::HashSet;
    let mut out = HashSet::new();
    let Ok(text) = std::process::Command::new("tasklist")
        .args(["/FO", "CSV", "/NH"])
        .output()
    else {
        return out;
    };
    for line in String::from_utf8_lossy(&text.stdout).lines() {
        // "name","pid","session","#","mem" -- the pid is the second field, and a comma inside the
        // LAST field cannot reach it.
        let mut fields = line.split(',');
        let _name = fields.next();
        if let Some(pid) = fields
            .next()
            .and_then(|f| f.trim_matches('"').parse::<u32>().ok())
        {
            out.insert(pid);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pruner's rule, tested without touching %TEMP%: a name is prunable only when every
    /// number in it is dead, and a name with no number is never prunable.
    #[test]
    fn only_names_whose_numbers_are_all_dead_are_prunable() {
        let me = std::process::id();
        let never = |_pid: u32| false;
        let pid_4242 = |pid: u32| pid == 4242;
        // NO pid in the name: nothing identifies its run, so it is never pruned -- even though
        // it carries digits. MEASURED REGRESSION: the first version of this rule treated ANY
        // number as a pid candidate, so the 2 of "e2e" made the mock-agent copies directory
        // prunable, and the pruner deleted a live suite's agent binaries.
        assert!(!all_numeric_fields_are_dead_with(
            "ruagent-e2e-mock-bins",
            me,
            never
        ));
        assert!(!all_numeric_fields_are_dead_with(
            "ruagent-knowledge-probe",
            me,
            never
        ));
        assert!(!all_numeric_fields_are_dead_with(
            "ruagent-kbfiles-4294967280",
            me,
            never
        ));
        // A plausible pid that is GONE: prunable (the reading the pruner acts on) ...
        assert!(all_numeric_fields_are_dead_with(
            "ruagent-kbfiles-4242",
            me,
            never
        ));
        // ... and the same pid ALIVE keeps the root, wherever it sits in the name.
        assert!(!all_numeric_fields_are_dead_with(
            "ruagent-kbfiles-4242",
            me,
            pid_4242
        ));
        assert!(!all_numeric_fields_are_dead_with(
            "ruagent-t250-legs-warm-4242-3",
            me,
            pid_4242
        ));
        assert!(!all_numeric_fields_are_dead_with(
            &format!("ruagent-t293-{me}"),
            me,
            never
        ));
    }
}
