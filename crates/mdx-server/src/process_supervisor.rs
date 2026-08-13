//! Bounded native supervision for model-influenced subprocesses.
//!
//! A Forge command is not complete merely because its direct child exits. The
//! process may have left descendants holding pipes, may ignore a polite stop,
//! or may produce enough output to exhaust the server. This module gives every
//! adopted call site the same lifecycle: a hard deadline, optional cooperative
//! cancellation, process-group termination on Unix, bounded output capture,
//! and a bounded drain after exit.

use std::collections::{HashMap, HashSet};
use std::io::{self, Read};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::time::{Duration, Instant};

const READER_COUNT: usize = 2;
static ACTIVE_RUN_CANCELLATIONS: OnceLock<Mutex<HashMap<String, ActiveRunCancellation>>> =
    OnceLock::new();
static NEXT_RUN_REGISTRATION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Default)]
pub(crate) struct ProcessCancellation {
    cancelled: Arc<AtomicBool>,
}

impl ProcessCancellation {
    pub(crate) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

#[derive(Debug)]
struct ActiveRunCancellation {
    cancellation: ProcessCancellation,
    registrations: HashSet<u64>,
}

/// Keeps one run addressable by operator controls for the guard's lifetime.
/// Concurrent registrations for the same run deliberately share a token so
/// an accidental overlapping worker cannot escape a stop. The final guard to
/// leave removes the token, allowing a later resume to start fresh.
#[derive(Debug)]
pub(crate) struct RunCancellationGuard {
    run_id: String,
    registration: u64,
}

impl Drop for RunCancellationGuard {
    fn drop(&mut self) {
        let mut active = active_run_cancellations()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let remove = active.get_mut(&self.run_id).is_some_and(|entry| {
            entry.registrations.remove(&self.registration);
            entry.registrations.is_empty()
        });
        if remove {
            active.remove(&self.run_id);
        }
    }
}

pub(crate) fn register_run_cancellation(run_id: &str) -> RunCancellationGuard {
    let run_id = run_id.trim().to_string();
    let registration = NEXT_RUN_REGISTRATION.fetch_add(1, Ordering::Relaxed);
    let mut active = active_run_cancellations()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let entry = active
        .entry(run_id.clone())
        .or_insert_with(|| ActiveRunCancellation {
            cancellation: ProcessCancellation::default(),
            registrations: HashSet::new(),
        });
    entry.registrations.insert(registration);
    RunCancellationGuard {
        run_id,
        registration,
    }
}

pub(crate) fn cancellation_for_run(run_id: &str) -> Option<ProcessCancellation> {
    active_run_cancellations()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(run_id.trim())
        .map(|entry| entry.cancellation.clone())
}

/// Signals every supervised process currently owned by this run. Returns
/// false when the durable control was recorded before or after a local worker
/// lifetime and therefore had no in-memory process to interrupt.
pub(crate) fn cancel_run(run_id: &str) -> bool {
    let active = active_run_cancellations()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(entry) = active.get(run_id.trim()) {
        entry.cancellation.cancel();
        true
    } else {
        false
    }
}

fn active_run_cancellations() -> &'static Mutex<HashMap<String, ActiveRunCancellation>> {
    ACTIVE_RUN_CANCELLATIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ProcessLimits {
    pub(crate) timeout: Duration,
    pub(crate) terminate_grace: Duration,
    pub(crate) drain_timeout: Duration,
    pub(crate) output_tail_bytes: usize,
    pub(crate) poll_interval: Duration,
}

impl ProcessLimits {
    pub(crate) fn bounded(timeout: Duration, output_tail_bytes: usize) -> Self {
        Self {
            timeout,
            terminate_grace: Duration::from_secs(1),
            drain_timeout: Duration::from_secs(2),
            output_tail_bytes,
            poll_interval: Duration::from_millis(10),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProcessReport {
    pub(crate) exit_code: i32,
    pub(crate) duration_ms: u64,
    pub(crate) timed_out: bool,
    pub(crate) cancelled: bool,
    pub(crate) stdout_bytes: u64,
    pub(crate) stderr_bytes: u64,
    pub(crate) output_truncated: bool,
    pub(crate) output_tail: String,
    pub(crate) drain_timed_out: bool,
}

impl ProcessReport {
    pub(crate) fn passed(&self) -> bool {
        !self.timed_out && !self.cancelled && !self.drain_timed_out && self.exit_code == 0
    }

    pub(crate) fn effective_exit_code(&self) -> i32 {
        if self.timed_out {
            124
        } else if self.cancelled {
            130
        } else if self.drain_timed_out {
            125
        } else {
            self.exit_code
        }
    }

    pub(crate) fn output_bytes(&self) -> u64 {
        self.stdout_bytes.saturating_add(self.stderr_bytes)
    }
}

#[derive(Clone, Copy)]
enum StreamKind {
    Stdout,
    Stderr,
}

#[derive(Debug)]
struct CaptureState {
    tail: Vec<u8>,
    tail_limit: usize,
    stdout_bytes: u64,
    stderr_bytes: u64,
}

impl CaptureState {
    fn new(tail_limit: usize) -> Self {
        Self {
            tail: Vec::with_capacity(tail_limit.min(64 * 1024)),
            tail_limit,
            stdout_bytes: 0,
            stderr_bytes: 0,
        }
    }

    fn append(&mut self, kind: StreamKind, bytes: &[u8]) {
        let count = bytes.len() as u64;
        match kind {
            StreamKind::Stdout => self.stdout_bytes = self.stdout_bytes.saturating_add(count),
            StreamKind::Stderr => self.stderr_bytes = self.stderr_bytes.saturating_add(count),
        }
        if self.tail_limit == 0 || bytes.is_empty() {
            return;
        }
        if bytes.len() >= self.tail_limit {
            self.tail.clear();
            self.tail
                .extend_from_slice(&bytes[bytes.len() - self.tail_limit..]);
            return;
        }
        let overflow = self
            .tail
            .len()
            .saturating_add(bytes.len())
            .saturating_sub(self.tail_limit);
        if overflow > 0 {
            self.tail.drain(..overflow);
        }
        self.tail.extend_from_slice(bytes);
    }
}

pub(crate) fn run_supervised(
    command: &mut Command,
    limits: ProcessLimits,
    cancellation: Option<&ProcessCancellation>,
) -> io::Result<ProcessReport> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    configure_process_group(command);

    let started = Instant::now();
    let mut child = command.spawn()?;
    let process_group_id = child.id();
    let capture = Arc::new(Mutex::new(CaptureState::new(limits.output_tail_bytes)));
    let (reader_done_tx, reader_done_rx) = mpsc::channel();
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("supervised child stdout was not piped"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| io::Error::other("supervised child stderr was not piped"))?;
    spawn_reader(
        stdout,
        StreamKind::Stdout,
        capture.clone(),
        reader_done_tx.clone(),
    );
    spawn_reader(stderr, StreamKind::Stderr, capture.clone(), reader_done_tx);

    let deadline = started + limits.timeout;
    let mut timed_out = false;
    let mut cancelled = false;
    let status = loop {
        if cancellation.is_some_and(ProcessCancellation::is_cancelled) {
            cancelled = true;
            break terminate_process_tree(
                &mut child,
                process_group_id,
                limits.terminate_grace,
                limits.poll_interval,
            )?;
        }
        if Instant::now() >= deadline {
            timed_out = true;
            break terminate_process_tree(
                &mut child,
                process_group_id,
                limits.terminate_grace,
                limits.poll_interval,
            )?;
        }
        if let Some(status) = child.try_wait()? {
            cleanup_descendants(process_group_id);
            break status;
        }
        std::thread::sleep(limits.poll_interval);
    };

    let drain_timed_out = !wait_for_readers(&reader_done_rx, limits.drain_timeout);
    let duration_ms = started.elapsed().as_millis() as u64;
    let capture = capture
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let total_bytes = capture.stdout_bytes.saturating_add(capture.stderr_bytes);
    let output_truncated = total_bytes > limits.output_tail_bytes as u64;
    let tail = String::from_utf8_lossy(&capture.tail).into_owned();
    let output_tail = if output_truncated {
        format!(
            "[...truncated to the last {} bytes; {} bytes total...]\n{}",
            limits.output_tail_bytes, total_bytes, tail
        )
    } else {
        tail
    };
    Ok(ProcessReport {
        exit_code: exit_status_code(status),
        duration_ms,
        timed_out,
        cancelled,
        stdout_bytes: capture.stdout_bytes,
        stderr_bytes: capture.stderr_bytes,
        output_truncated,
        output_tail,
        drain_timed_out,
    })
}

fn spawn_reader<R: Read + Send + 'static>(
    mut reader: R,
    kind: StreamKind,
    capture: Arc<Mutex<CaptureState>>,
    done: mpsc::Sender<()>,
) {
    std::thread::spawn(move || {
        let mut buffer = [0u8; 8192];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => capture
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .append(kind, &buffer[..count]),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
        let _ = done.send(());
    });
}

fn wait_for_readers(done: &mpsc::Receiver<()>, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    let mut completed = 0;
    while completed < READER_COUNT {
        let now = Instant::now();
        if now >= deadline {
            return false;
        }
        match done.recv_timeout(deadline.saturating_duration_since(now)) {
            Ok(()) => completed += 1,
            Err(_) => return false,
        }
    }
    true
}

fn terminate_process_tree(
    child: &mut std::process::Child,
    process_group_id: u32,
    grace: Duration,
    poll_interval: Duration,
) -> io::Result<ExitStatus> {
    signal_process_tree(child, process_group_id, TerminationSignal::Polite);
    let deadline = Instant::now() + grace;
    let mut observed = None;
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait()? {
            observed = Some(status);
            break;
        }
        std::thread::sleep(poll_interval);
    }
    signal_process_tree(child, process_group_id, TerminationSignal::Force);
    match observed {
        Some(status) => Ok(status),
        None => child.wait(),
    }
}

#[derive(Clone, Copy)]
enum TerminationSignal {
    Polite,
    Force,
}

#[cfg(unix)]
fn configure_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(not(unix))]
fn configure_process_group(_command: &mut Command) {}

#[cfg(unix)]
fn signal_process_tree(
    _child: &mut std::process::Child,
    process_group_id: u32,
    signal: TerminationSignal,
) {
    let signal = match signal {
        TerminationSignal::Polite => libc::SIGTERM,
        TerminationSignal::Force => libc::SIGKILL,
    };
    // Negative pid targets the process group created for this command.
    unsafe {
        libc::kill(-(process_group_id as i32), signal);
    }
}

#[cfg(not(unix))]
fn signal_process_tree(
    child: &mut std::process::Child,
    _process_group_id: u32,
    _signal: TerminationSignal,
) {
    let _ = child.kill();
}

#[cfg(unix)]
fn cleanup_descendants(process_group_id: u32) {
    unsafe {
        libc::kill(-(process_group_id as i32), libc::SIGTERM);
        libc::kill(-(process_group_id as i32), libc::SIGKILL);
    }
}

#[cfg(not(unix))]
fn cleanup_descendants(_process_group_id: u32) {}

#[cfg(unix)]
fn exit_status_code(status: ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt;
    status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(0))
}

#[cfg(not(unix))]
fn exit_status_code(status: ExitStatus) -> i32 {
    status.code().unwrap_or(-1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn shell(script: &str) -> Command {
        let mut command = Command::new("sh");
        command.arg("-c").arg(script);
        command
    }

    #[cfg(unix)]
    #[test]
    fn output_is_counted_and_only_the_bounded_tail_is_retained() {
        let mut command = shell(
            "i=0; while [ $i -lt 200 ]; do printf 'line%03d-abcdefghij\\n' $i; i=$((i+1)); done",
        );
        let report = run_supervised(
            &mut command,
            ProcessLimits::bounded(Duration::from_secs(2), 128),
            None,
        )
        .expect("supervised");
        assert!(report.passed());
        assert!(report.output_bytes() > 128);
        assert!(report.output_truncated);
        assert!(report.output_tail.contains("line199"));
        assert!(!report.output_tail.contains("line000"));
    }

    #[cfg(unix)]
    #[test]
    fn timeout_terminates_the_process_group_and_returns_promptly() {
        let mut command = shell("sleep 30 & echo child:$!; wait");
        let mut limits = ProcessLimits::bounded(Duration::from_millis(100), 1024);
        limits.terminate_grace = Duration::from_millis(100);
        let report = run_supervised(&mut command, limits, None).expect("supervised");
        assert!(report.timed_out);
        assert_eq!(report.effective_exit_code(), 124);
        assert!(report.duration_ms < 2_000);
        let child_pid = report
            .output_tail
            .lines()
            .find_map(|line| line.strip_prefix("child:"))
            .and_then(|pid| pid.trim().parse::<i32>().ok())
            .expect("child pid was captured");
        std::thread::sleep(Duration::from_millis(20));
        let alive = unsafe { libc::kill(child_pid, 0) } == 0;
        assert!(!alive, "the descendant must not survive the timeout");
    }

    #[cfg(unix)]
    #[test]
    fn cooperative_cancellation_uses_the_same_bounded_cleanup() {
        let cancellation = ProcessCancellation::default();
        let trigger = cancellation.clone();
        let canceller = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            trigger.cancel();
        });
        let mut command = shell("sleep 30");
        let mut limits = ProcessLimits::bounded(Duration::from_secs(30), 256);
        limits.terminate_grace = Duration::from_millis(100);
        let report = run_supervised(&mut command, limits, Some(&cancellation)).expect("supervised");
        canceller.join().expect("canceller");
        assert!(report.cancelled);
        assert!(!report.timed_out);
        assert_eq!(report.effective_exit_code(), 130);
        assert!(report.duration_ms < 2_000);
    }

    #[test]
    fn run_registration_is_shared_until_the_last_overlapping_worker_leaves() {
        let run_id = "forge_registry_shared_workers";
        let first = register_run_cancellation(run_id);
        let first_token = cancellation_for_run(run_id).expect("first token");
        let second = register_run_cancellation(run_id);
        let second_token = cancellation_for_run(run_id).expect("second token");

        assert!(!first_token.is_cancelled());
        assert!(cancel_run(run_id));
        assert!(first_token.is_cancelled());
        assert!(second_token.is_cancelled());

        drop(first);
        assert!(cancellation_for_run(run_id).is_some());
        drop(second);
        assert!(cancellation_for_run(run_id).is_none());

        let resumed = register_run_cancellation(run_id);
        assert!(
            !cancellation_for_run(run_id)
                .expect("fresh resume token")
                .is_cancelled()
        );
        drop(resumed);
    }
}
