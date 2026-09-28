//! Run a child process with a hard bound on how long zforge waits for it —
//! including every process it spawns.
//!
//! Both the agent runner and the test runner used to `kill()` only their
//! direct child on timeout, then `join()` the stdout/stderr reader threads.
//! Those threads end at EOF, and EOF only arrives when *every* process
//! holding the pipe's write end has exited. A shell that backgrounded a
//! grandchild (`sh -c 'sleep 300 & wait'`), a test harness that forked a
//! server, or an agent CLI that spawned helpers all kept the pipe open, so
//! a one-second timeout could wait for as long as the grandchild lived and
//! left it running afterwards (FIX-006). The same happened on a *normal*
//! exit that left a background process behind.
//!
//! Here each child is started as the leader of its own process group, so
//! the whole tree can be signalled at once:
//!
//! 1. Wait up to `timeout` for the direct child.
//! 2. On timeout: SIGTERM the group, give it [`KILL_GRACE`], SIGKILL.
//! 3. Drain the pipes for at most [`DRAIN_GRACE`]. If something still holds
//!    them, SIGKILL the group and drain once more. If a process escaped the
//!    group (`setsid`) and still holds a pipe, stop waiting and report the
//!    output as incomplete rather than block.
//! 4. Kill anything left in the group, so no descendant outlives the call.
//!
//! Worst-case wall time: `timeout + KILL_GRACE + 2 × DRAIN_GRACE`.
//!
//! Moving children out of zforge's own process group changes who receives
//! group-directed signals: Ctrl-C at the terminal and `zforge run cancel`
//! (which signals the worker's group) would no longer reach them. Two
//! mechanisms restore that:
//!
//! - A handler for SIGINT/SIGTERM/SIGHUP forwards the signal to every child
//!   group currently running, then restores the default action and
//!   re-raises, so zforge itself still terminates exactly as before.
//! - When [`CHILD_PGIDS_FILE_ENV`] is set (a background run's worker sets it
//!   to a file in the run directory), the active child groups are recorded
//!   there, and `run cancel` escalates to SIGKILL on them too.

use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use wait_timeout::ChildExt;

/// How long a timed-out process group gets between SIGTERM and SIGKILL.
pub const KILL_GRACE: Duration = Duration::from_secs(2);
/// How long to keep draining output after the direct child has exited
/// before treating a still-open pipe as held by a leftover descendant.
pub const DRAIN_GRACE: Duration = Duration::from_secs(1);

/// When set, the process groups of running children are written to this
/// file (one pgid per line) so an external canceller can kill them.
pub const CHILD_PGIDS_FILE_ENV: &str = "ZFORGE_CHILD_PGIDS_FILE";

#[derive(Debug)]
pub struct BoundedOutput {
    /// `None` only if the child could not be reaped at all.
    pub status: Option<ExitStatus>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    /// The direct child outlived `timeout` and was killed.
    pub timed_out: bool,
    /// A process outside the child's group still held a pipe after the
    /// drain grace; the captured output may be missing its tail.
    pub output_incomplete: bool,
}

impl BoundedOutput {
    /// Exit code, or -1 when the child was terminated by a signal.
    pub fn exit_code(&self) -> i32 {
        self.status.and_then(|s| s.code()).unwrap_or(-1)
    }
}

/// Spawn `cmd`, optionally feed `stdin_bytes`, and wait under the bounds
/// described in the [module docs](self). `cmd`'s stdout and stderr are
/// always captured; its stdin is piped when `stdin_bytes` is `Some` and
/// connected to `/dev/null` otherwise (a child in a background process
/// group that reads the terminal would be stopped by SIGTTIN).
pub fn run_bounded(
    mut cmd: Command,
    stdin_bytes: Option<Vec<u8>>,
    timeout: Duration,
) -> io::Result<BoundedOutput> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd.stdin(if stdin_bytes.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    group::isolate(&mut cmd);

    let mut child = cmd.spawn()?;
    let registration = group::register(child.id());

    if let (Some(bytes), Some(mut stdin)) = (stdin_bytes, child.stdin.take()) {
        // Background thread so a child that writes before it finishes
        // reading cannot deadlock against us. Dropping `stdin` sends EOF.
        thread::spawn(move || {
            let _ = stdin.write_all(&bytes);
        });
    }

    let stdout = Drain::start(child.stdout.take().expect("stdout piped"));
    let stderr = Drain::start(child.stderr.take().expect("stderr piped"));

    let (status, timed_out) = wait_or_terminate(&mut child, timeout)?;

    let mut incomplete = false;
    let deadline = Instant::now() + DRAIN_GRACE;
    if !(stdout.wait_until(deadline) && stderr.wait_until(deadline)) {
        // The direct child is gone but its pipes are still open: a
        // descendant is holding them. Kill the group and try once more.
        group::kill(child.id(), group::Signal::Kill);
        let deadline = Instant::now() + DRAIN_GRACE;
        incomplete = !(stdout.wait_until(deadline) && stderr.wait_until(deadline));
    }
    // Nothing in the group may outlive the call, whether it still held a
    // pipe or had already redirected its output elsewhere.
    group::kill_if_alive(child.id());
    drop(registration);

    Ok(BoundedOutput {
        status,
        stdout: stdout.take(),
        stderr: stderr.take(),
        timed_out,
        output_incomplete: incomplete,
    })
}

/// Wait for `child`; on timeout escalate SIGTERM → SIGKILL on its group.
/// Signals go out while the leader is still unreaped, so its pid — and with
/// it the group id — cannot have been reused by an unrelated process.
fn wait_or_terminate(
    child: &mut Child,
    timeout: Duration,
) -> io::Result<(Option<ExitStatus>, bool)> {
    if let Some(status) = child.wait_timeout(timeout)? {
        return Ok((Some(status), false));
    }
    group::kill(child.id(), group::Signal::Term);
    if let Some(status) = child.wait_timeout(KILL_GRACE)? {
        return Ok((Some(status), true));
    }
    group::kill(child.id(), group::Signal::Kill);
    let _ = child.kill(); // non-unix fallback; harmless after a group kill
    Ok((child.wait().ok(), true))
}

/// Reads a pipe to EOF on a background thread, keeping what it has read so
/// far available even if EOF never comes.
struct Drain {
    buf: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
    done: mpsc::Receiver<()>,
    finished: std::cell::Cell<bool>,
}

impl Drain {
    fn start<R: Read + Send + 'static>(mut pipe: R) -> Self {
        let buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let (tx, rx) = mpsc::channel();
        let sink = buf.clone();
        thread::spawn(move || {
            let mut chunk = [0u8; 8192];
            loop {
                match pipe.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => sink
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .extend_from_slice(&chunk[..n]),
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
            let _ = tx.send(());
        });
        Self {
            buf,
            done: rx,
            finished: std::cell::Cell::new(false),
        }
    }

    /// True once the pipe reached EOF; waits no later than `deadline`.
    fn wait_until(&self, deadline: Instant) -> bool {
        if self.finished.get() {
            return true;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        let done = !matches!(
            self.done.recv_timeout(remaining),
            Err(mpsc::RecvTimeoutError::Timeout)
        );
        self.finished.set(done);
        done
    }

    /// Everything read so far. If the reader is still blocked (output
    /// incomplete) it is left behind; it ends when the pipe finally closes.
    fn take(self) -> Vec<u8> {
        std::mem::take(&mut *self.buf.lock().unwrap_or_else(|p| p.into_inner()))
    }
}

#[cfg(unix)]
mod group {
    //! Process-group control and signal forwarding (unix).

    use super::CHILD_PGIDS_FILE_ENV;
    use std::os::unix::process::CommandExt;
    use std::process::Command;
    use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
    use std::sync::{Mutex, Once};

    // Set by `catch_interrupts`: the handler records the signal instead of
    // re-raising it, so the caller can record the interruption and exit.
    static CATCH: AtomicBool = AtomicBool::new(false);
    static CAUGHT: AtomicI32 = AtomicI32::new(0);

    pub fn catch_interrupts() {
        CATCH.store(true, Ordering::SeqCst);
        INSTALL.call_once(install_forwarding);
    }

    pub fn take_interrupt() -> Option<i32> {
        match CAUGHT.swap(0, Ordering::SeqCst) {
            0 => None,
            sig => Some(sig),
        }
    }

    pub enum Signal {
        Term,
        Kill,
    }

    /// Start the child as the leader of a new process group (pgid = pid).
    pub fn isolate(cmd: &mut Command) {
        cmd.process_group(0);
    }

    pub fn kill(pgid: u32, sig: Signal) {
        let sig = match sig {
            Signal::Term => libc::SIGTERM,
            Signal::Kill => libc::SIGKILL,
        };
        // SAFETY: negative pid addresses the process group, per kill(2).
        // ESRCH (group already empty) is harmless.
        unsafe {
            libc::kill(-(pgid as i32), sig);
        }
    }

    /// SIGKILL the group if any member is still alive. The leader has been
    /// reaped by now; a group id stays reserved while any member remains,
    /// so the probe and the kill both address this group.
    pub fn kill_if_alive(pgid: u32) {
        // SAFETY: signal 0 only checks for existence.
        let alive = unsafe { libc::kill(-(pgid as i32), 0) } == 0;
        if alive {
            kill(pgid, Signal::Kill);
        }
    }

    // ── active child registry ────────────────────────────────────────────
    //
    // Fixed-size array of atomics so the signal handler can read it without
    // taking a lock (which would not be async-signal-safe). 0 = free slot.
    const SLOTS: usize = 64;
    // A `const` item (not an inline `const {}` block) so this builds on the
    // crate's declared MSRV; each array element is a fresh atomic.
    #[allow(clippy::declare_interior_mutable_const)]
    const FREE: AtomicI32 = AtomicI32::new(0);
    static ACTIVE: [AtomicI32; SLOTS] = [FREE; SLOTS];
    // Serializes rewrites of the pgid file; never touched by the handler.
    static FILE_LOCK: Mutex<()> = Mutex::new(());
    static INSTALL: Once = Once::new();

    /// Removes the child from the registry when dropped.
    pub struct Registration {
        slot: Option<usize>,
    }

    impl Drop for Registration {
        fn drop(&mut self) {
            if let Some(i) = self.slot {
                ACTIVE[i].store(0, Ordering::SeqCst);
                persist();
            }
        }
    }

    pub fn register(pgid: u32) -> Registration {
        INSTALL.call_once(install_forwarding);
        let pgid = pgid as i32;
        let slot = (0..SLOTS).find(|&i| {
            ACTIVE[i]
                .compare_exchange(0, pgid, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
        });
        if slot.is_none() {
            eprintln!(
                "zforge: more than {SLOTS} concurrent child processes; \
                 pgid {pgid} will not receive forwarded signals"
            );
        }
        persist();
        Registration { slot }
    }

    fn active_pgids() -> Vec<i32> {
        ACTIVE
            .iter()
            .map(|a| a.load(Ordering::SeqCst))
            .filter(|&p| p > 0)
            .collect()
    }

    /// Mirror the active set into `$ZFORGE_CHILD_PGIDS_FILE`, if set.
    /// Best-effort: failing to record only weakens cancel escalation.
    fn persist() {
        let Some(path) = std::env::var_os(CHILD_PGIDS_FILE_ENV) else {
            return;
        };
        let _guard = FILE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let body: String = active_pgids().iter().map(|p| format!("{p}\n")).collect();
        let path = std::path::PathBuf::from(path);
        if let Err(e) = crate::fs::write_atomic(&path, body.as_bytes()) {
            eprintln!("zforge: could not record child process groups in {path:?}: {e:#}");
        }
    }

    const FORWARDED: [libc::c_int; 3] = [libc::SIGINT, libc::SIGTERM, libc::SIGHUP];

    /// `sigaction` rather than `signal`: `sa_mask` blocks all forwarded
    /// signals while the handler runs, so a second one (Ctrl-C, then the
    /// terminal closing) waits instead of starting a second forwarding pass
    /// on top of the first.
    fn install_forwarding() {
        // SAFETY: plain libc calls on locally owned, zero-initialized
        // `sigaction` structs; the handler only calls async-signal-safe
        // functions (kill, nanosleep, sigaction, raise) and reads lock-free
        // atomics.
        unsafe {
            let mut mask: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut mask);
            for sig in FORWARDED {
                libc::sigaddset(&mut mask, sig);
            }
            for sig in FORWARDED {
                let mut prev: libc::sigaction = std::mem::zeroed();
                libc::sigaction(sig, std::ptr::null(), &mut prev);
                // Respect an inherited "ignore" (e.g. nohup'd SIGHUP).
                if prev.sa_sigaction == libc::SIG_IGN {
                    continue;
                }
                let handler: extern "C" fn(libc::c_int) = forward;
                let mut action: libc::sigaction = std::mem::zeroed();
                action.sa_sigaction = handler as libc::sighandler_t;
                action.sa_mask = mask;
                action.sa_flags = libc::SA_RESTART;
                libc::sigaction(sig, &action, std::ptr::null_mut());
            }
        }
    }

    /// zforge is about to die, and the child groups it started would be left
    /// as orphans nobody manages. Same escalation as a timeout: pass the
    /// signal on, give them [`super::KILL_GRACE`] to exit, SIGKILL the rest.
    ///
    /// Forwarding alone is not enough. A non-interactive shell starts its
    /// background jobs (`cmd &`) with SIGINT *ignored*, so a Ctrl-C passed
    /// along leaves exactly those processes running — the same leak a
    /// shared process group had before, now closed.
    ///
    /// Only async-signal-safe calls: kill, nanosleep, signal, raise, and
    /// lock-free atomic loads. Runs with every forwarded signal blocked
    /// (see [`install_forwarding`]), so it never re-enters.
    extern "C" fn forward(sig: libc::c_int) {
        signal_active(sig);

        const STEP_MS: u64 = 50;
        let mut waited_ms = 0u64;
        while any_active_alive() && waited_ms < super::KILL_GRACE.as_millis() as u64 {
            let step = libc::timespec {
                tv_sec: 0,
                tv_nsec: (STEP_MS * 1_000_000) as libc::c_long,
            };
            // SAFETY: nanosleep(2) is async-signal-safe.
            unsafe {
                libc::nanosleep(&step, std::ptr::null_mut());
            }
            waited_ms += STEP_MS;
        }
        signal_active(libc::SIGKILL);

        // A caller that catches interrupts records its own end; the children
        // are already stopped. (Atomic store: async-signal-safe.)
        if CATCH.load(Ordering::SeqCst) {
            CAUGHT.store(sig, Ordering::SeqCst);
            return;
        }

        // Then die the way we would have without the handler, of *this*
        // signal. Forwarded signals that arrived meanwhile are pending
        // (blocked by `sa_mask`); ignoring them discards them, so none can
        // win the race to be delivered first once the handler returns.
        // SAFETY: signal(2) and raise(3) are async-signal-safe.
        unsafe {
            for other in FORWARDED {
                if other != sig {
                    libc::signal(other, libc::SIG_IGN);
                }
            }
            libc::signal(sig, libc::SIG_DFL);
            libc::raise(sig);
        }
    }

    fn signal_active(sig: libc::c_int) {
        for slot in &ACTIVE {
            let pgid = slot.load(Ordering::SeqCst);
            if pgid > 0 {
                // SAFETY: kill(2) is async-signal-safe; ESRCH is harmless.
                unsafe {
                    libc::kill(-pgid, sig);
                }
            }
        }
    }

    fn any_active_alive() -> bool {
        ACTIVE.iter().any(|slot| {
            let pgid = slot.load(Ordering::SeqCst);
            // SAFETY: signal 0 only probes for existence.
            pgid > 0 && unsafe { libc::kill(-pgid, 0) } == 0
        })
    }
}

#[cfg(not(unix))]
mod group {
    //! No process groups: only the direct child is killed on timeout, and
    //! the drain bound still guarantees zforge stops waiting.
    use std::process::Command;

    pub enum Signal {
        Term,
        Kill,
    }
    pub fn isolate(_cmd: &mut Command) {}
    pub fn kill(_pgid: u32, _sig: Signal) {}
    pub fn kill_if_alive(_pgid: u32) {}
    pub struct Registration;
    pub fn register(_pgid: u32) -> Registration {
        Registration
    }
    pub fn catch_interrupts() {}
    pub fn take_interrupt() -> Option<i32> {
        None
    }
}

/// From now on SIGINT/SIGTERM/SIGHUP stop every running child tree as
/// before, but do not end the process: the signal is recorded for
/// [`take_interrupt`], so a long-running command can record how it ended
/// (a Mốc B run writes `cancelled`) and exit on its own.
pub fn catch_interrupts() {
    group::catch_interrupts();
}

/// The signal caught since the last call, if any (see [`catch_interrupts`]).
pub fn take_interrupt() -> Option<i32> {
    group::take_interrupt()
}

/// Process groups recorded in a `CHILD_PGIDS_FILE_ENV` file.
pub fn read_child_pgids(path: &std::path::Path) -> Vec<i32> {
    std::fs::read_to_string(path)
        .map(|s| s.lines().filter_map(|l| l.trim().parse().ok()).collect())
        .unwrap_or_default()
}

/// Path of the child-pgid file inside a run directory.
pub fn child_pgids_file(run_dir: &std::path::Path) -> PathBuf {
    run_dir.join("child-pgids")
}

/// `kill(pid, 0)` returns Ok if signal could be delivered (process exists +
/// we have permission). ESRCH = dead; EPERM = alive but ours-or-theirs.
/// Anything other than ESRCH is treated as alive — conservative.
pub fn pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        let pid_i = pid as i32;
        // SAFETY: `kill(pid, 0)` is a query, not a signal send. No state
        // mutation; thread-safe per POSIX.
        let rc = unsafe { libc::kill(pid_i, 0) };
        if rc == 0 {
            return true;
        }
        let err = std::io::Error::last_os_error();
        // ESRCH (3) → not alive. Other errors (EPERM etc.) → alive.
        !matches!(err.raw_os_error(), Some(3))
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        true // unsupported platform; assume alive to avoid false negatives
    }
}

/// How long a cancelled worker and its children get between SIGTERM and
/// SIGKILL.
pub const CANCEL_GRACE: std::time::Duration = std::time::Duration::from_secs(5);

/// SIGTERM the worker's process group and every child group recorded in
/// `pgids_file`, wait up to [`CANCEL_GRACE`], SIGKILL what is left. Shared
/// by `zforge run cancel` and a feature loop's cancel.
#[cfg(unix)]
pub fn terminate_process_groups(worker_pid: u32, pgids_file: &std::path::Path) {
    let mut groups: Vec<i32> = vec![worker_pid as i32];
    groups.extend(read_child_pgids(pgids_file));
    groups.sort_unstable();
    groups.dedup();

    let signal_all = |sig: libc::c_int| {
        for g in &groups {
            // SAFETY: negative pid addresses the process group, per kill(2).
            // ESRCH (group already gone) is harmless.
            unsafe {
                libc::kill(-g, sig);
            }
        }
    };
    let any_alive = || {
        groups
            .iter()
            // SAFETY: signal 0 only probes for existence.
            .any(|g| unsafe { libc::kill(-g, 0) } == 0)
    };

    signal_all(libc::SIGTERM);
    let start = std::time::Instant::now();
    while any_alive() && start.elapsed() < CANCEL_GRACE {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    if any_alive() {
        signal_all(libc::SIGKILL);
    }
}

#[cfg(not(unix))]
pub fn terminate_process_groups(_worker_pid: u32, _pgids_file: &std::path::Path) {
    eprintln!("warning: cancel on Windows does not yet terminate the worker — mark-only.");
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::path::Path;

    fn sh(script: &str) -> Command {
        let mut c = Command::new("sh");
        c.arg("-c").arg(script);
        c
    }

    fn pid_alive(pid: i32) -> bool {
        // SAFETY: signal 0 only checks for existence.
        unsafe { libc::kill(pid, 0) == 0 }
    }

    /// Orphans are reaped by init/launchd asynchronously; allow a moment.
    fn assert_dies(pid: i32) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while pid_alive(pid) {
            assert!(Instant::now() < deadline, "pid {pid} still alive");
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn read_pid(path: &Path) -> i32 {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Ok(s) = std::fs::read_to_string(path) {
                if let Ok(p) = s.trim().parse() {
                    return p;
                }
            }
            assert!(Instant::now() < deadline, "pid file never written");
            thread::sleep(Duration::from_millis(10));
        }
    }

    const BOUND_SLACK: Duration = Duration::from_millis(1500);

    // The FIX-006 repro: timeout 1s, a grandchild holding the pipes for
    // longer. Previously took as long as the grandchild lived.
    #[test]
    fn timeout_bounds_the_wait_when_a_grandchild_holds_the_pipes() {
        let dir = tempfile::tempdir().unwrap();
        let pidfile = dir.path().join("gc.pid");
        let script = format!("sleep 30 & echo $! > '{}'; wait", pidfile.display());

        let started = Instant::now();
        let out = run_bounded(sh(&script), None, Duration::from_secs(1)).unwrap();
        let elapsed = started.elapsed();

        assert!(out.timed_out);
        assert!(
            elapsed < Duration::from_secs(1) + KILL_GRACE + BOUND_SLACK,
            "waited {elapsed:?}"
        );
        assert_dies(read_pid(&pidfile));
    }

    // Same leak on a normal exit: the direct child finishes, a background
    // grandchild keeps the pipes open and would keep running.
    #[test]
    fn normal_exit_does_not_wait_for_or_leave_a_background_grandchild() {
        let dir = tempfile::tempdir().unwrap();
        let pidfile = dir.path().join("gc.pid");
        let script = format!("sleep 30 & echo $! > '{}'; echo done", pidfile.display());

        let started = Instant::now();
        let out = run_bounded(sh(&script), None, Duration::from_secs(60)).unwrap();
        let elapsed = started.elapsed();

        assert!(!out.timed_out);
        assert_eq!(out.exit_code(), 0);
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "done");
        assert!(
            elapsed < 2 * DRAIN_GRACE + BOUND_SLACK,
            "waited {elapsed:?} for a leftover grandchild"
        );
        assert_dies(read_pid(&pidfile));
    }

    // A descendant that redirected its output away never holds our pipes,
    // but must still not outlive the call.
    #[test]
    fn detached_output_grandchild_is_still_killed() {
        let dir = tempfile::tempdir().unwrap();
        let pidfile = dir.path().join("gc.pid");
        let script = format!(
            "sleep 30 >/dev/null 2>&1 & echo $! > '{}'",
            pidfile.display()
        );

        let out = run_bounded(sh(&script), None, Duration::from_secs(60)).unwrap();
        assert_eq!(out.exit_code(), 0);
        assert_dies(read_pid(&pidfile));
    }

    #[test]
    fn direct_child_timeout_is_prompt() {
        let started = Instant::now();
        let out = run_bounded(sh("exec sleep 30"), None, Duration::from_secs(1)).unwrap();
        assert!(out.timed_out);
        assert!(started.elapsed() < Duration::from_secs(1) + KILL_GRACE + BOUND_SLACK);
    }

    // A child that ignores SIGTERM is still stopped by the SIGKILL step.
    #[test]
    fn sigterm_resistant_child_is_killed_after_the_grace() {
        let started = Instant::now();
        let out = run_bounded(
            sh("trap '' TERM; while :; do sleep 1; done"),
            None,
            Duration::from_secs(1),
        )
        .unwrap();
        assert!(out.timed_out);
        assert!(started.elapsed() < Duration::from_secs(1) + KILL_GRACE + BOUND_SLACK);
    }

    #[test]
    fn captures_stdout_stderr_exit_code_and_stdin() {
        let out = run_bounded(
            sh("cat; echo err >&2; exit 3"),
            Some(b"from stdin".to_vec()),
            Duration::from_secs(10),
        )
        .unwrap();
        assert!(!out.timed_out && !out.output_incomplete);
        assert_eq!(out.exit_code(), 3);
        assert_eq!(out.stdout, b"from stdin");
        assert_eq!(String::from_utf8_lossy(&out.stderr).trim(), "err");
    }

    #[test]
    fn large_output_is_drained_without_deadlock() {
        let out = run_bounded(sh("yes | head -c 300000"), None, Duration::from_secs(10)).unwrap();
        assert!(!out.timed_out);
        assert_eq!(out.stdout.len(), 300_000);
    }

    #[test]
    fn stdin_is_not_the_terminal_when_not_provided() {
        // Reads /dev/null → immediate EOF, instead of blocking or SIGTTIN.
        let out = run_bounded(sh("cat; echo ok"), None, Duration::from_secs(5)).unwrap();
        assert!(!out.timed_out);
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "ok");
    }

    #[test]
    fn pid_alive_for_self_process() {
        assert!(crate::process::pid_alive(std::process::id()));
    }

    #[test]
    fn pid_alive_false_for_unallocated_pid() {
        assert!(!crate::process::pid_alive(999_999_999));
    }
}
