//! Durable ledger and memory snapshots for the serving path.
//!
//! The kernel ledger lives in process memory, so before this module a restart
//! erased every governed write since boot - receipts included, which breaks
//! the one promise this system makes. The serving path now snapshots the
//! ledger and the id counter to disk after every successful POST and restores
//! them on boot.
//!
//! Memory rides inside the authoritative ledger snapshot and in a derived
//! sibling file: the restored ledger attests to consolidation receipts, so
//! the memory records those receipts minted must survive the same atomic
//! commit or the chain attests to memory that no longer exists. The embedded
//! snapshot and sibling carry only the durable memory core - records, graph,
//! lifecycle events, recall rankings, and the surface access matrix. The
//! eval/comparator/benchmark/topology ceremony rows are deliberately not
//! durable (see the 2026-07 memory audit): they restore empty and reseed on
//! demand rather than entrenching placeholder-grade measurements.
//!
//! Posture by mode: trusted-session modes (local-secure, production) snapshot
//! by default, because that is where real operators live. local-demo stays
//! off by default - the deterministic proofs rely on a fresh world per boot -
//! and opts in with `MDX_KERNEL_SNAPSHOT=1`. `MDX_KERNEL_SNAPSHOT=0` forces
//! it off anywhere.
//!
//! Restore fails closed: a snapshot that does not parse or whose hash chain
//! does not verify refuses boot with a clear message instead of silently
//! starting empty and overwriting the evidence on the next write. The memory
//! sibling holds the same bar: a file that does not parse, or whose records
//! cite receipts absent from the restored ledger, refuses boot.

use mdx_core::{
    ConsolidationDecision, DeploymentMode, MdxKernel, MemoryBrainSnapshot, MemoryGraphEdge,
    MemoryGraphNode, MemoryLifecycleEvent, MemoryProvenance, MemoryRecallRanking, MemoryRecord,
    MemorySurfaceAccess, Receipt,
};
use serde::ser::{SerializeSeq, Serializer};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{BufReader, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};

pub(crate) const SNAPSHOT_PATH: &str = ".mdx-local/kernel-ledger-snapshot.json";
const SNAPSHOT_VERSION: u64 = 1;
const SNAPSHOT_BUNDLE_VERSION: u64 = 1;
pub(crate) const MEMORY_SNAPSHOT_PATH: &str = ".mdx-local/memory-brain-snapshot.json";
const MEMORY_SNAPSHOT_VERSION: u64 = 1;

/// Default coalescing window for the background flusher. Every write inside a
/// window folds into a single snapshot, so the serialize+fsync cost is paid at
/// most once per window instead of once per write. A hard, unclean kill can
/// lose only the writes committed in the last window that no `flush_now`
/// covered.
const DEFAULT_SNAPSHOT_INTERVAL_MS: u64 = 250;

/// Coalescing window from the environment. `0` keeps the previous
/// write-synchronous behavior (no background flusher).
fn interval_from_env() -> Duration {
    let ms = std::env::var("MDX_KERNEL_SNAPSHOT_INTERVAL_MS")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .unwrap_or(DEFAULT_SNAPSHOT_INTERVAL_MS);
    Duration::from_millis(ms)
}

/// Whether snapshots are active for this deployment mode.
pub(crate) fn enabled(mode: DeploymentMode) -> bool {
    match std::env::var("MDX_KERNEL_SNAPSHOT").ok().as_deref() {
        Some("1") => true,
        Some("0") => false,
        _ => mode.requires_trusted_session(),
    }
}

/// Snapshot from a worker thread at a run boundary - a run, stream, or
/// fleet reaching a terminal receipt. The founder's A/B evidence
/// evaporated on a kernel restart because run-thread receipts never
/// triggered the POST-path snapshot; terminal-boundary snapshots close
/// that gap without serializing the ledger on every mid-run receipt.
/// Gated on the explicit env switch; the POST path covers mode defaults.
pub(crate) fn snapshot_at_boundary(kernel: &std::sync::Arc<RwLock<MdxKernel>>) {
    if std::env::var("MDX_KERNEL_SNAPSHOT").ok().as_deref() != Some("1") {
        return;
    }
    // Recover a poisoned lock rather than skip: the receipt hash-chain is the
    // real integrity guarantee (verified on every write and at boot), so a
    // prior panic must never silently stop a run's terminal receipts from
    // being durably snapshotted.
    let rendered = {
        let kernel = kernel
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        render_snapshots(&kernel)
    };
    if let Err(error) = rendered.and_then(|rendered| write_snapshots(&rendered)) {
        eprintln!("mdx-server snapshot write failed: {error}");
    }
}

/// The authoritative ledger bundle and its derived Memory sibling, rendered
/// under one kernel read so both copies are a consistent view: every Memory
/// record cites a receipt present in the ledger rendering.
pub(crate) struct RenderedSnapshots {
    pub(crate) ledger: String,
    pub(crate) memory: String,
}

pub(crate) fn render_snapshots(kernel: &MdxKernel) -> Result<RenderedSnapshots, String> {
    let memory = render_memory_snapshot(&kernel.memory_brain_snapshot());
    Ok(RenderedSnapshots {
        ledger: render_snapshot_bundle(kernel.ledger().entries(), kernel.ids_counter(), &memory)?,
        memory,
    })
}

/// Commit the authoritative bundle first, then refresh the derived sibling.
/// The ledger file embeds the matching Memory snapshot, so its one atomic
/// rename is the commit point for both. A crash before that rename keeps the
/// previous pair; a crash after it restores Memory from the embedded copy and
/// ignores any older sibling left by the interrupted compatibility write.
pub(crate) fn write_snapshots(rendered: &RenderedSnapshots) -> Result<(), String> {
    write_snapshot(&rendered.ledger)?;
    write_memory_snapshot(&rendered.memory)
}

fn render_snapshot_bundle(
    entries: &[Receipt],
    ids_counter: u64,
    memory: &str,
) -> Result<String, String> {
    let memory_snapshot = serde_json::from_str::<serde_json::Value>(memory)
        .map_err(|error| format!("bundle rendered Memory snapshot: {error}"))?;
    serde_json::to_string(&SnapshotBundleRef {
        name: "mdx-kernel-ledger-snapshot",
        version: SNAPSHOT_VERSION,
        ids_counter,
        receipt_count: entries.len(),
        receipts: ReceiptSlice(entries),
        bundle_version: SNAPSHOT_BUNDLE_VERSION,
        memory_snapshot,
    })
    .map_err(|error| format!("serialize bundled kernel snapshot: {error}"))
}

#[derive(Serialize)]
struct LedgerSnapshotRef<'a> {
    name: &'static str,
    version: u64,
    ids_counter: u64,
    receipt_count: usize,
    receipts: ReceiptSlice<'a>,
}

#[derive(Serialize)]
struct SnapshotBundleRef<'a> {
    name: &'static str,
    version: u64,
    ids_counter: u64,
    receipt_count: usize,
    receipts: ReceiptSlice<'a>,
    bundle_version: u64,
    memory_snapshot: serde_json::Value,
}

struct ReceiptSlice<'a>(&'a [Receipt]);

impl Serialize for ReceiptSlice<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for receipt in self.0 {
            sequence.serialize_element(&ReceiptSnapshotRef::from(receipt))?;
        }
        sequence.end()
    }
}

#[derive(Serialize)]
struct ReceiptSnapshotRef<'a> {
    receipt_id: &'a str,
    tenant_id: &'a str,
    trace_id: &'a str,
    actor_id: &'a str,
    loop_id: &'a str,
    workflow_id: &'a str,
    kind: &'a str,
    policy_decision_id: &'a Option<String>,
    payload: &'a BTreeMap<String, String>,
    previous_hash: &'a Option<String>,
    receipt_timestamp: &'a str,
    hash_version: u64,
    hash: &'a str,
}

impl<'a> From<&'a Receipt> for ReceiptSnapshotRef<'a> {
    fn from(receipt: &'a Receipt) -> Self {
        Self {
            receipt_id: &receipt.receipt_id,
            tenant_id: receipt.tenant_id.as_str(),
            trace_id: receipt.trace_id.as_str(),
            actor_id: receipt.actor_id.as_str(),
            loop_id: receipt.loop_id.as_str(),
            workflow_id: receipt.workflow_id.as_str(),
            kind: &receipt.kind,
            policy_decision_id: &receipt.policy_decision_id,
            payload: &receipt.payload,
            previous_hash: &receipt.previous_hash,
            receipt_timestamp: &receipt.receipt_timestamp,
            hash_version: receipt.hash_version,
            hash: &receipt.hash,
        }
    }
}

/// Serialize the ledger and id counter. Called with the kernel lock held;
/// the caller writes the returned bytes after releasing the lock.
pub(crate) fn render_snapshot(entries: &[Receipt], ids_counter: u64) -> String {
    serde_json::to_string(&LedgerSnapshotRef {
        name: "mdx-kernel-ledger-snapshot",
        version: SNAPSHOT_VERSION,
        ids_counter,
        receipt_count: entries.len(),
        receipts: ReceiptSlice(entries),
    })
    .expect("serializing the typed kernel snapshot cannot fail")
}

static SNAPSHOT_SEQ: AtomicU64 = AtomicU64::new(0);
/// Serializes snapshot writes so a slow write cannot interleave with a
/// rename from another request thread.
static SNAPSHOT_WRITE: Mutex<()> = Mutex::new(());

/// Write a rendered snapshot atomically: temp file, then rename. The rename
/// is the commit point, so a crash mid-write leaves the previous snapshot
/// intact. Failure is surfaced to the caller; the serving path logs it
/// loudly rather than pretending durability it does not have.
pub(crate) fn write_snapshot(rendered: &str) -> Result<(), String> {
    write_snapshot_to(SNAPSHOT_PATH, rendered)
}

pub(crate) fn write_memory_snapshot(rendered: &str) -> Result<(), String> {
    write_snapshot_to(MEMORY_SNAPSHOT_PATH, rendered)
}

/// Acquire the snapshot write serializer. A prior panic can poison it, but the
/// receipt hash-chain (verified on every write and at boot) is the real
/// integrity guarantee, so durability must never be skipped over a poison
/// flag: recover the guard and keep writing rather than drop it. Recovering is
/// also what actually holds the lock - mapping the poison to an error would
/// discard the `PoisonError` that carries the guard and release it at once,
/// letting two writers interleave their temp/rename.
fn snapshot_write_guard() -> std::sync::MutexGuard<'static, ()> {
    SNAPSHOT_WRITE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Atomic snapshot write to an explicit path. `write_snapshot` targets the
/// serving-path const; tests target a temp path to exercise durability without
/// touching the real snapshot.
fn write_snapshot_to(path: &str, rendered: &str) -> Result<(), String> {
    let _guard = snapshot_write_guard();
    write_snapshot_locked(path, rendered)
}

/// Write rendered bytes to `path` atomically: temp file, fsync, then rename.
/// The rename is the commit point, so a crash mid-write leaves the previous
/// snapshot intact. Assumes the caller already holds `SNAPSHOT_WRITE` so the
/// temp/rename of two writers cannot interleave; `write_snapshot_to` is the
/// standalone entry that takes the guard, and `SnapshotFlusher::write_target`
/// holds the guard across the render as well so the watermark cannot advance
/// past what is on disk.
fn write_snapshot_locked(path: &str, rendered: &str) -> Result<(), String> {
    let seq = SNAPSHOT_SEQ.fetch_add(1, Ordering::SeqCst);
    let tmp_path = format!("{path}.tmp.{}.{seq}", std::process::id());
    if let Some(parent) = std::path::Path::new(path).parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    let mut file = std::fs::File::create(&tmp_path)
        .map_err(|error| format!("create snapshot temp {tmp_path}: {error}"))?;
    file.write_all(rendered.as_bytes())
        .map_err(|error| format!("write snapshot temp: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("sync snapshot temp: {error}"))?;
    std::fs::rename(&tmp_path, path).map_err(|error| format!("commit snapshot rename: {error}"))?;
    sync_snapshot_parent(path)?;
    Ok(())
}

#[cfg(unix)]
fn sync_snapshot_parent(path: &str) -> Result<(), String> {
    let parent = std::path::Path::new(path)
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    std::fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("sync snapshot directory {}: {error}", parent.display()))
}

#[cfg(not(unix))]
fn sync_snapshot_parent(_path: &str) -> Result<(), String> {
    Ok(())
}

/// Bounded-interval coalescing snapshot flusher.
///
/// Before this, `snapshot_after_write` serialized the whole ledger and fsynced
/// the whole file - synchronously, on the request thread, behind one global
/// write lock - after every governed write. Under concurrency that is
/// O(writes x ledger_size) of serialized disk work on the hot path, so p95
/// latency climbed superlinearly as the ledger grew. The flusher moves that
/// work off the request thread: a write marks the ledger dirty in O(1), and a
/// background thread renders and atomically writes the current ledger at most
/// once per `interval`, folding a burst of writes into one snapshot.
///
/// Durability: the in-memory ledger is the source of truth during a run; the
/// snapshot is the restart-durability copy. Every committed write lands on
/// disk within `interval` even if traffic then stops, and `flush_now` forces a
/// synchronous write on demand (explicit checkpoint, shutdown). The only thing
/// a hard, unclean kill can lose from this local snapshot is writes committed
/// in the last `interval` that no `flush_now` covered; the postgres export
/// plane still runs per write and is unaffected.
pub(crate) struct SnapshotFlusher {
    kernel: Arc<RwLock<MdxKernel>>,
    interval: Duration,
    ledger_path: String,
    memory_path: Option<String>,
    state: Mutex<FlushState>,
    wake: Condvar,
}

#[derive(Default)]
struct FlushState {
    /// Monotonic count of write requests seen.
    requested: u64,
    /// Highest `requested` value durably written.
    flushed: u64,
    last_flush: Option<Instant>,
}

impl SnapshotFlusher {
    #[cfg(test)]
    fn new(kernel: Arc<RwLock<MdxKernel>>, interval: Duration, path: String) -> Arc<Self> {
        Arc::new(Self {
            kernel,
            interval,
            ledger_path: path,
            memory_path: None,
            state: Mutex::new(FlushState::default()),
            wake: Condvar::new(),
        })
    }

    fn new_with_memory(
        kernel: Arc<RwLock<MdxKernel>>,
        interval: Duration,
        ledger_path: String,
        memory_path: String,
    ) -> Arc<Self> {
        Arc::new(Self {
            kernel,
            interval,
            ledger_path,
            memory_path: Some(memory_path),
            state: Mutex::new(FlushState::default()),
            wake: Condvar::new(),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, FlushState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Mark the ledger dirty. O(1): bumps the request counter and wakes the
    /// flusher. Never serializes the ledger, never touches disk, never blocks
    /// on IO - this is what runs on the governed-write hot path.
    pub(crate) fn request(&self) {
        {
            let mut state = self.lock();
            state.requested = state.requested.saturating_add(1);
        }
        self.wake.notify_one();
    }

    /// Render and atomically write the current ledger and embedded Memory now,
    /// on the calling thread, advancing the flushed watermark to whatever was
    /// requested at entry. Used for explicit checkpoints and shutdown, where
    /// the caller wants the durability guarantee before returning.
    pub(crate) fn flush_now(&self) -> Result<(), String> {
        let target = self.lock().requested;
        self.write_target(target)
    }

    /// Render the current ledger, atomically write it, then advance the
    /// flushed watermark - all under the one `SNAPSHOT_WRITE` guard.
    ///
    /// The render is held under the guard, not only the file write, and that
    /// is the whole point. A `flush_now` on a request thread can race the
    /// background flusher. If only the file write were serialized, the two
    /// could interleave so an older/smaller render renames last while the
    /// watermark was advanced to the newer target - leaving the on-disk
    /// snapshot behind the watermark, and because `requested == flushed` the
    /// flusher would not self-heal until the next write. If traffic then
    /// stopped, those receipts would never reach disk.
    ///
    /// Serializing render+write together removes the interleave: whichever
    /// writer holds the guard last renders the newest ledger and writes it, so
    /// the file on disk is always at least as new as any watermark. `requested`
    /// only grows and every increment trails a committed write, so a render
    /// taken while holding the guard contains at least `requested`-many writes,
    /// which is `>= target`. `max(flushed, target)` therefore never claims more
    /// than the last render actually put on disk. The render stays off the
    /// request hot path (that only marks the ledger dirty in O(1)); the only
    /// added contention is between the rare `flush_now` and the once-per-window
    /// background render, so the common single-writer path is unchanged.
    fn write_target(&self, target: u64) -> Result<(), String> {
        let _guard = snapshot_write_guard();
        let rendered = {
            let kernel = self
                .kernel
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if self.memory_path.is_some() {
                render_snapshots(&kernel)
            } else {
                Ok(RenderedSnapshots {
                    ledger: render_snapshot(kernel.ledger().entries(), kernel.ids_counter()),
                    memory: String::new(),
                })
            }
        }?;
        // Test-only seam: force the render-then-rename interleave a racing
        // writer would otherwise hit only by luck. Compiled out of the server
        // binary; the guard above is held across it, which is exactly what the
        // fix relies on.
        #[cfg(test)]
        run_write_target_render_hook();
        let result = write_snapshot_locked(&self.ledger_path, &rendered.ledger).and_then(|_| {
            if let Some(memory_path) = &self.memory_path {
                write_snapshot_locked(memory_path, &rendered.memory)
            } else {
                Ok(())
            }
        });
        let mut state = self.lock();
        state.last_flush = Some(Instant::now());
        if result.is_ok() {
            state.flushed = state.flushed.max(target);
        }
        result
    }

    fn spawn(self: &Arc<Self>) {
        let this = Arc::clone(self);
        std::thread::Builder::new()
            .name("mdx-snapshot-flusher".to_string())
            .spawn(move || this.run())
            .expect("spawn snapshot flusher thread");
    }

    fn run(self: Arc<Self>) {
        loop {
            let target = self.wait_for_work();
            if let Err(error) = self.write_target(target) {
                eprintln!("mdx-server snapshot flush failed: {error}");
                // Back off a window before retrying so a persistent disk error
                // cannot spin the thread. The watermark stayed behind, so the
                // next pass retries the same target.
                std::thread::sleep(self.interval);
            }
        }
    }

    /// Block until there is unflushed work, honoring the coalescing interval,
    /// then return the request watermark to flush up to.
    fn wait_for_work(&self) -> u64 {
        let mut state = self.lock();
        loop {
            if state.requested > state.flushed {
                // Hold until at least `interval` since the last flush so more
                // writes fold into this one snapshot.
                let remaining = state
                    .last_flush
                    .and_then(|last| self.interval.checked_sub(last.elapsed()));
                match remaining {
                    Some(wait) if !wait.is_zero() => {
                        state = self
                            .wake
                            .wait_timeout(state, wait)
                            .map(|(guard, _)| guard)
                            .unwrap_or_else(|poisoned| poisoned.into_inner().0);
                    }
                    _ => return state.requested,
                }
            } else {
                // Idle: wait for a request, with a cap so a missed notify
                // cannot wedge the thread.
                state = self
                    .wake
                    .wait_timeout(state, Duration::from_millis(500))
                    .map(|(guard, _)| guard)
                    .unwrap_or_else(|poisoned| poisoned.into_inner().0);
            }
        }
    }
}

static FLUSHER: OnceLock<Arc<SnapshotFlusher>> = OnceLock::new();

/// Start the background flusher for a snapshotting mode. A zero interval keeps
/// the previous synchronous-per-write behavior (no flusher installed), as does
/// a mode with snapshots disabled. Call once, at boot, after restore.
pub(crate) fn init_flusher(kernel: Arc<RwLock<MdxKernel>>, mode: DeploymentMode) {
    if !enabled(mode) {
        return;
    }
    let interval = interval_from_env();
    if interval.is_zero() {
        return;
    }
    let flusher = SnapshotFlusher::new_with_memory(
        kernel,
        interval,
        SNAPSHOT_PATH.to_string(),
        MEMORY_SNAPSHOT_PATH.to_string(),
    );
    flusher.spawn();
    let _ = FLUSHER.set(flusher);
}

/// The installed background flusher, if any. `None` in synchronous mode.
pub(crate) fn flusher() -> Option<&'static Arc<SnapshotFlusher>> {
    FLUSHER.get()
}

/// Force a synchronous durable flush of everything committed so far. A no-op
/// in synchronous mode, where the per-write path already wrote.
pub(crate) fn flush_now() -> Result<(), String> {
    shutdown_flush(FLUSHER.get())
}

/// The flush the shutdown handler performs, factored out from the global
/// `FLUSHER` so it is unit testable against a locally built flusher. A `None`
/// flusher (snapshots off, or interval 0) is a no-op: the per-write path
/// already wrote, so nothing is buffered to lose.
fn shutdown_flush(flusher: Option<&Arc<SnapshotFlusher>>) -> Result<(), String> {
    match flusher {
        Some(flusher) => flusher.flush_now(),
        None => Ok(()),
    }
}

/// Perform the final durable flush on the way out. Safe to call once; logs and
/// swallows any error, because a best-effort last write on a graceful stop is
/// strictly better than the up-to-one-window loss the coalescing flusher would
/// otherwise leave.
pub(crate) fn run_shutdown_flush() {
    if let Err(error) = shutdown_flush(FLUSHER.get()) {
        eprintln!("mdx-server shutdown flush failed: {error}");
    }
}

/// Install the shutdown flush: block SIGTERM and SIGINT process-wide, then a
/// dedicated thread waits for either and performs one final synchronous durable
/// flush before the process exits.
///
/// Without this, `serve` is an infinite accept loop with no signal handler, so
/// a container stop / deploy / `systemctl` (SIGTERM), a Ctrl-C (SIGINT), or a
/// dogfood-stack teardown terminates the process with up to one coalescing
/// window of committed receipts never written to the snapshot - a regression
/// from the old per-write fsync that lost nothing on a graceful stop. With this
/// installed, the only remaining loss window is a genuine SIGKILL or power
/// loss.
///
/// Blocking the signals on the calling (main) thread before any worker thread
/// spawns means every later thread inherits the block, so only the dedicated
/// waiter receives them. `sigwait` returns in an ordinary thread context (not
/// an async-signal handler), so the flush it triggers runs with no
/// async-signal-safety constraints. Must be called before `init_flusher` and
/// the accept loop so the flusher and connection threads inherit the block. It
/// is a no-op when snapshots are off or the interval is 0, because `flush_now`
/// (which the waiter calls) is a no-op there.
#[cfg(unix)]
pub(crate) fn install_shutdown_flush() {
    use std::mem::MaybeUninit;
    // SAFETY: standard POSIX signal-set construction. `sigemptyset` fully
    // initializes the set before any read, and the mask is applied to the
    // current thread before any worker thread exists.
    let set = unsafe {
        let mut set = MaybeUninit::<libc::sigset_t>::uninit();
        libc::sigemptyset(set.as_mut_ptr());
        libc::sigaddset(set.as_mut_ptr(), libc::SIGTERM);
        libc::sigaddset(set.as_mut_ptr(), libc::SIGINT);
        let set = set.assume_init();
        if libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut()) != 0 {
            eprintln!(
                "mdx-server could not block shutdown signals; final durable flush not installed"
            );
            return;
        }
        set
    };
    std::thread::Builder::new()
        .name("mdx-shutdown-flush".to_string())
        .spawn(move || {
            let mut received: libc::c_int = 0;
            // sigwait consumes one blocked signal. A nonzero return is EINTR or
            // a spurious wake: keep waiting. A real SIGTERM/SIGINT falls through
            // to the final flush, then the process exits.
            loop {
                // SAFETY: `set` is a fully initialized signal set owned by this
                // thread; `received` is a valid out-pointer.
                let rc = unsafe { libc::sigwait(&set, &mut received) };
                if rc == 0 {
                    break;
                }
            }
            run_shutdown_flush();
            std::process::exit(0);
        })
        .expect("spawn shutdown flush thread");
}

/// No POSIX signals to install against here. Explicit checkpoints still flush
/// through `flush_now`; there is simply no graceful-stop signal to hook.
#[cfg(not(unix))]
pub(crate) fn install_shutdown_flush() {}

/// Restore the ledger from disk at boot. Returns the restored receipt count,
/// or None when no snapshot exists. A snapshot that exists but does not
/// parse or verify is a hard error - the operator decides, the server never
/// silently boots empty over real evidence.
pub(crate) fn restore_into(kernel: &mut MdxKernel) -> Result<Option<usize>, String> {
    restore_from(SNAPSHOT_PATH, kernel)
}

/// Restore from an explicit snapshot path. `restore_into` targets the
/// serving-path const; tests target a temp path.
fn restore_from(path: &str, kernel: &mut MdxKernel) -> Result<Option<usize>, String> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("read {path}: {error}")),
    };
    let parsed: LedgerSnapshotOwned = serde_json::from_reader(BufReader::new(file))
        .map_err(|error| format!("parse {path}: {error}"))?;
    if parsed.version != SNAPSHOT_VERSION {
        return Err(format!(
            "{path}: unsupported snapshot version {}",
            parsed.version
        ));
    }
    let ids_counter = parsed.ids_counter;
    let entries = parsed
        .receipts
        .into_iter()
        .map(Receipt::from)
        .collect::<Vec<_>>();
    let count = kernel
        .restore_ledger_entries(entries)
        .map_err(|error| format!("{path}: {error}"))?;
    kernel.restore_ids_counter(ids_counter);
    Ok(Some(count))
}

#[derive(Deserialize)]
struct LedgerSnapshotOwned {
    version: u64,
    #[serde(default)]
    ids_counter: u64,
    receipts: Vec<ReceiptSnapshotOwned>,
}

#[derive(Deserialize)]
struct ReceiptSnapshotOwned {
    receipt_id: String,
    tenant_id: String,
    trace_id: String,
    actor_id: String,
    loop_id: String,
    workflow_id: String,
    kind: String,
    policy_decision_id: Option<String>,
    payload: BTreeMap<String, String>,
    previous_hash: Option<String>,
    #[serde(default)]
    receipt_timestamp: String,
    #[serde(default = "timeless_receipt_hash_version")]
    hash_version: u64,
    hash: String,
}

fn timeless_receipt_hash_version() -> u64 {
    mdx_core::RECEIPT_HASH_VERSION_TIMELESS
}

impl From<ReceiptSnapshotOwned> for Receipt {
    fn from(receipt: ReceiptSnapshotOwned) -> Self {
        Self {
            receipt_id: receipt.receipt_id,
            tenant_id: mdx_core::TenantId::new(receipt.tenant_id),
            trace_id: mdx_core::TraceId::new(receipt.trace_id),
            actor_id: mdx_core::ActorId::new(receipt.actor_id),
            loop_id: mdx_core::LoopId::new(receipt.loop_id),
            workflow_id: mdx_core::WorkflowId::new(receipt.workflow_id),
            kind: receipt.kind,
            policy_decision_id: receipt.policy_decision_id,
            payload: receipt.payload,
            previous_hash: receipt.previous_hash,
            receipt_timestamp: receipt.receipt_timestamp,
            hash_version: receipt.hash_version,
            hash: receipt.hash,
        }
    }
}

/// Serialize the durable memory core. mdx-core carries no serde, so the
/// nested structs are hand-rendered here, mirroring the receipt rendering
/// above. Only records, graph nodes/edges, lifecycle events, recall
/// rankings, and the surface access matrix are durable; the
/// eval/comparator/benchmark/topology ceremony vectors are deliberately
/// omitted so placeholder-grade measurement rows never survive a restart.
pub(crate) fn render_memory_snapshot(snapshot: &MemoryBrainSnapshot) -> String {
    let records: Vec<serde_json::Value> = snapshot
        .records
        .iter()
        .map(|record| {
            serde_json::json!({
                "memory_id": record.memory_id,
                "episode_id": record.episode_id,
                "tenant_id": record.tenant_id.as_str(),
                "source_receipt_id": record.source_receipt_id,
                "atom_origin": record.atom_origin,
                "valid_from_receipt_timestamp": record.valid_from_receipt_timestamp,
                "consolidation_decision": record.consolidation_decision.as_str(),
                "provenance": {
                    "driver_id": record.provenance.driver_id,
                    "provider": record.provenance.provider,
                    "durable_driver": record.provenance.durable_driver,
                    "durable_table": record.provenance.durable_table,
                    "consolidation_gate": record.provenance.consolidation_gate,
                    "gate_receipt_id": record.provenance.gate_receipt_id,
                    "source_receipt_kind": record.provenance.source_receipt_kind,
                    "temporal_status": record.provenance.temporal_status,
                },
                "memory_scope": record.memory_scope,
                "memory_tier": record.memory_tier,
                "decay_policy": record.decay_policy,
                "importance_score": record.importance_score,
                "content": record.content,
                "valid_until_receipt_timestamp": record.valid_until_receipt_timestamp,
                "invalidated_by_receipt_id": record.invalidated_by_receipt_id,
                "consolidation_state": record.consolidation_state,
                "embedding": record.embedding,
            })
        })
        .collect();
    let graph_nodes: Vec<serde_json::Value> = snapshot
        .graph_nodes
        .iter()
        .map(|node| {
            serde_json::json!({
                "node_id": node.node_id,
                "tenant_id": node.tenant_id.as_str(),
                "node_kind": node.node_kind,
                "label": node.label,
                "memory_id": node.memory_id,
                "source_receipt_id": node.source_receipt_id,
                "atom_origin": node.atom_origin,
                "valid_from_receipt_timestamp": node.valid_from_receipt_timestamp,
                "lifecycle_state": node.lifecycle_state,
            })
        })
        .collect();
    let graph_edges: Vec<serde_json::Value> = snapshot
        .graph_edges
        .iter()
        .map(|edge| {
            serde_json::json!({
                "edge_id": edge.edge_id,
                "tenant_id": edge.tenant_id.as_str(),
                "from_node_id": edge.from_node_id,
                "to_node_id": edge.to_node_id,
                "edge_kind": edge.edge_kind,
                "source_receipt_id": edge.source_receipt_id,
                "weight": edge.weight,
                "valid_from_receipt_timestamp": edge.valid_from_receipt_timestamp,
            })
        })
        .collect();
    let lifecycle_events: Vec<serde_json::Value> = snapshot
        .lifecycle_events
        .iter()
        .map(|event| {
            serde_json::json!({
                "event_id": event.event_id,
                "tenant_id": event.tenant_id.as_str(),
                "memory_id": event.memory_id,
                "action": event.action,
                "lifecycle_state": event.lifecycle_state,
                "reason": event.reason,
                "source_receipt_id": event.source_receipt_id,
                "valid_from_receipt_timestamp": event.valid_from_receipt_timestamp,
                "receipt_id": event.receipt_id,
            })
        })
        .collect();
    let recall_rankings: Vec<serde_json::Value> = snapshot
        .recall_rankings
        .iter()
        .map(|ranking| {
            serde_json::json!({
                "ranking_id": ranking.ranking_id,
                "tenant_id": ranking.tenant_id.as_str(),
                "surface": ranking.surface,
                "query": ranking.query,
                "memory_id": ranking.memory_id,
                "lexical_score": ranking.lexical_score,
                "content_checksum_score": ranking.content_checksum_score,
                "graph_score": ranking.graph_score,
                "recency_score": ranking.recency_score,
                "importance_score": ranking.importance_score,
                "scope_score": ranking.scope_score,
                "source_authority_score": ranking.source_authority_score,
                "final_score": ranking.final_score,
                "rank": ranking.rank,
                "source_receipt_id": ranking.source_receipt_id,
                "receipt_id": ranking.receipt_id,
            })
        })
        .collect();
    let surface_access: Vec<serde_json::Value> = snapshot
        .surface_access
        .iter()
        .map(|access| {
            serde_json::json!({
                "access_id": access.access_id,
                "tenant_id": access.tenant_id.as_str(),
                "surface": access.surface,
                "scope": access.scope,
                "can_read": access.can_read,
                "can_write": access.can_write,
                "review_required": access.review_required,
                "receipt_id": access.receipt_id,
            })
        })
        .collect();
    serde_json::json!({
        "name": "mdx-memory-brain-snapshot",
        "version": MEMORY_SNAPSHOT_VERSION,
        "record_count": records.len(),
        "records": records,
        "graph_nodes": graph_nodes,
        "graph_edges": graph_edges,
        "lifecycle_events": lifecycle_events,
        "recall_rankings": recall_rankings,
        "surface_access": surface_access,
    })
    .to_string()
}

/// Restore the memory brain from the authoritative ledger bundle at boot,
/// falling back to the sibling for snapshots written before bundling. Must
/// run after the ledger restore: the kernel refuses any record citing a
/// receipt absent from the restored ledger, so Memory can never resurrect
/// content the evidence spine does not back. An absent legacy sibling boots
/// with empty Memory; malformed or unverifiable persisted state is a hard
/// error for the operator.
pub(crate) fn restore_memory_into(kernel: &mut MdxKernel) -> Result<Option<usize>, String> {
    match restore_memory_from_bundle_path(SNAPSHOT_PATH, kernel)? {
        Some(count) => Ok(Some(count)),
        None => restore_memory_from_path(MEMORY_SNAPSHOT_PATH, kernel),
    }
}

fn restore_memory_from_bundle_path(
    ledger_path: &str,
    kernel: &mut MdxKernel,
) -> Result<Option<usize>, String> {
    let file = match std::fs::File::open(ledger_path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("read {ledger_path}: {error}")),
    };
    let parsed: MemorySnapshotEnvelope = serde_json::from_reader(BufReader::new(file))
        .map_err(|error| format!("parse {ledger_path}: {error}"))?;
    let Some(memory) = parsed.memory_snapshot else {
        return Ok(None);
    };
    if parsed.bundle_version != Some(SNAPSHOT_BUNDLE_VERSION) {
        return Err(format!(
            "{ledger_path}: unsupported snapshot bundle version {:?}",
            parsed.bundle_version
        ));
    }
    let snapshot = parse_memory_snapshot_value(&memory)
        .map_err(|error| format!("{ledger_path}: embedded Memory snapshot: {error}"))?;
    let count = kernel
        .restore_memory_brain_snapshot(snapshot)
        .map_err(|error| format!("{ledger_path}: embedded Memory snapshot: {error}"))?;
    Ok(Some(count))
}

#[derive(Deserialize)]
struct MemorySnapshotEnvelope {
    #[serde(default)]
    bundle_version: Option<u64>,
    #[serde(default)]
    memory_snapshot: Option<serde_json::Value>,
}

fn restore_memory_from_path(path: &str, kernel: &mut MdxKernel) -> Result<Option<usize>, String> {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("read {path}: {error}")),
    };
    let snapshot = parse_memory_snapshot(&raw).map_err(|error| format!("{path}: {error}"))?;
    let count = kernel
        .restore_memory_brain_snapshot(snapshot)
        .map_err(|error| format!("{path}: {error}"))?;
    Ok(Some(count))
}

fn parse_memory_snapshot(raw: &str) -> Result<MemoryBrainSnapshot, String> {
    let parsed: serde_json::Value =
        serde_json::from_str(raw).map_err(|error| format!("parse memory snapshot: {error}"))?;
    parse_memory_snapshot_value(&parsed)
}

fn parse_memory_snapshot_value(parsed: &serde_json::Value) -> Result<MemoryBrainSnapshot, String> {
    if parsed["version"].as_u64() != Some(MEMORY_SNAPSHOT_VERSION) {
        return Err(format!(
            "unsupported memory snapshot version {}",
            parsed["version"]
        ));
    }
    Ok(MemoryBrainSnapshot {
        records: parse_memory_array(parsed, "records", memory_record_from_value)?,
        graph_nodes: parse_memory_array(parsed, "graph_nodes", graph_node_from_value)?,
        graph_edges: parse_memory_array(parsed, "graph_edges", graph_edge_from_value)?,
        lifecycle_events: parse_memory_array(
            parsed,
            "lifecycle_events",
            lifecycle_event_from_value,
        )?,
        recall_rankings: parse_memory_array(parsed, "recall_rankings", recall_ranking_from_value)?,
        surface_access: parse_memory_array(parsed, "surface_access", surface_access_from_value)?,
        // The ceremony vectors are intentionally not durable; they restore
        // empty even if a hand-edited file smuggles rows in.
        ..MemoryBrainSnapshot::default()
    })
}

fn parse_memory_array<T>(
    parsed: &serde_json::Value,
    name: &str,
    from_value: impl Fn(&serde_json::Value) -> Result<T, String>,
) -> Result<Vec<T>, String> {
    parsed[name]
        .as_array()
        .ok_or_else(|| format!("{name} is not an array"))?
        .iter()
        .map(|value| from_value(value).map_err(|error| format!("{name}: {error}")))
        .collect()
}

fn memory_string(value: &serde_json::Value, name: &str) -> Result<String, String> {
    value[name]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| format!("missing {name}"))
}

/// The memory structs carry `&'static str` fields for their closed
/// vocabularies (origins, tiers, scopes, kinds, states). A restored value is
/// leaked once to satisfy that shape: the leak is bounded to a single
/// boot-time restore, and `&str` equality is by content, so restored values
/// behave exactly like their compile-time originals. Interning against a
/// hand-maintained constant table was rejected - it would refuse valid
/// snapshots every time a vocabulary grows.
fn memory_static(value: &serde_json::Value, name: &str) -> Result<&'static str, String> {
    memory_string(value, name).map(|owned| &*Box::leak(owned.into_boxed_str()))
}

fn memory_u8(value: &serde_json::Value, name: &str) -> Result<u8, String> {
    value[name]
        .as_u64()
        .and_then(|number| u8::try_from(number).ok())
        .ok_or_else(|| format!("{name} is not a u8"))
}

fn memory_u16(value: &serde_json::Value, name: &str) -> Result<u16, String> {
    value[name]
        .as_u64()
        .and_then(|number| u16::try_from(number).ok())
        .ok_or_else(|| format!("{name} is not a u16"))
}

fn memory_bool(value: &serde_json::Value, name: &str) -> Result<bool, String> {
    value[name]
        .as_bool()
        .ok_or_else(|| format!("{name} is not a bool"))
}

fn memory_record_from_value(value: &serde_json::Value) -> Result<MemoryRecord, String> {
    let consolidation_decision = match memory_string(value, "consolidation_decision")?.as_str() {
        "RETAIN" => ConsolidationDecision::Retain,
        "SKIP" => ConsolidationDecision::Skip,
        "ADD" => ConsolidationDecision::Add,
        "UPDATE" => ConsolidationDecision::Update,
        "SUPERSEDE" => ConsolidationDecision::Supersede,
        "NOOP" => ConsolidationDecision::Noop,
        other => return Err(format!("unknown consolidation decision {other}")),
    };
    let provenance = &value["provenance"];
    if !provenance.is_object() {
        return Err("missing provenance".to_string());
    }
    Ok(MemoryRecord {
        memory_id: memory_string(value, "memory_id")?,
        episode_id: memory_string(value, "episode_id")?,
        tenant_id: mdx_core::TenantId::new(memory_string(value, "tenant_id")?),
        source_receipt_id: memory_string(value, "source_receipt_id")?,
        atom_origin: memory_static(value, "atom_origin")?,
        valid_from_receipt_timestamp: memory_string(value, "valid_from_receipt_timestamp")?,
        consolidation_decision,
        provenance: MemoryProvenance {
            driver_id: memory_static(provenance, "driver_id")?,
            provider: memory_static(provenance, "provider")?,
            durable_driver: memory_static(provenance, "durable_driver")?,
            durable_table: memory_static(provenance, "durable_table")?,
            consolidation_gate: memory_static(provenance, "consolidation_gate")?,
            gate_receipt_id: memory_string(provenance, "gate_receipt_id")?,
            source_receipt_kind: memory_string(provenance, "source_receipt_kind")?,
            temporal_status: memory_static(provenance, "temporal_status")?,
        },
        memory_scope: memory_static(value, "memory_scope")?,
        memory_tier: memory_static(value, "memory_tier")?,
        decay_policy: memory_static(value, "decay_policy")?,
        importance_score: memory_u8(value, "importance_score")?,
        content: memory_string(value, "content")?,
        // Absent in pre-adjudication snapshots: an open validity window and
        // an active consolidation are exactly what those records were.
        valid_until_receipt_timestamp: value["valid_until_receipt_timestamp"]
            .as_str()
            .unwrap_or("")
            .to_string(),
        invalidated_by_receipt_id: value["invalidated_by_receipt_id"]
            .as_str()
            .unwrap_or("")
            .to_string(),
        consolidation_state: match value["consolidation_state"].as_str() {
            None => mdx_core::MEMORY_CONSOLIDATION_ACTIVE,
            // Leak-once interning, same rationale as memory_static.
            Some(state) => &*Box::leak(state.to_string().into_boxed_str()),
        },
        // Absent in pre-embedding snapshots: no local embedder had run, so an
        // empty string is exactly what those records were.
        embedding: value["embedding"].as_str().unwrap_or("").to_string(),
    })
}

fn graph_node_from_value(value: &serde_json::Value) -> Result<MemoryGraphNode, String> {
    Ok(MemoryGraphNode {
        node_id: memory_string(value, "node_id")?,
        tenant_id: mdx_core::TenantId::new(memory_string(value, "tenant_id")?),
        node_kind: memory_static(value, "node_kind")?,
        label: memory_string(value, "label")?,
        memory_id: value["memory_id"].as_str().map(str::to_string),
        source_receipt_id: memory_string(value, "source_receipt_id")?,
        atom_origin: memory_static(value, "atom_origin")?,
        valid_from_receipt_timestamp: memory_string(value, "valid_from_receipt_timestamp")?,
        lifecycle_state: memory_static(value, "lifecycle_state")?,
    })
}

fn graph_edge_from_value(value: &serde_json::Value) -> Result<MemoryGraphEdge, String> {
    Ok(MemoryGraphEdge {
        edge_id: memory_string(value, "edge_id")?,
        tenant_id: mdx_core::TenantId::new(memory_string(value, "tenant_id")?),
        from_node_id: memory_string(value, "from_node_id")?,
        to_node_id: memory_string(value, "to_node_id")?,
        edge_kind: memory_static(value, "edge_kind")?,
        source_receipt_id: memory_string(value, "source_receipt_id")?,
        weight: memory_u8(value, "weight")?,
        valid_from_receipt_timestamp: memory_string(value, "valid_from_receipt_timestamp")?,
    })
}

fn lifecycle_event_from_value(value: &serde_json::Value) -> Result<MemoryLifecycleEvent, String> {
    Ok(MemoryLifecycleEvent {
        event_id: memory_string(value, "event_id")?,
        tenant_id: mdx_core::TenantId::new(memory_string(value, "tenant_id")?),
        memory_id: memory_string(value, "memory_id")?,
        action: memory_static(value, "action")?,
        lifecycle_state: memory_static(value, "lifecycle_state")?,
        reason: memory_string(value, "reason")?,
        source_receipt_id: memory_string(value, "source_receipt_id")?,
        valid_from_receipt_timestamp: memory_string(value, "valid_from_receipt_timestamp")?,
        receipt_id: memory_string(value, "receipt_id")?,
    })
}

fn recall_ranking_from_value(value: &serde_json::Value) -> Result<MemoryRecallRanking, String> {
    Ok(MemoryRecallRanking {
        ranking_id: memory_string(value, "ranking_id")?,
        tenant_id: mdx_core::TenantId::new(memory_string(value, "tenant_id")?),
        surface: memory_static(value, "surface")?,
        query: memory_string(value, "query")?,
        memory_id: memory_string(value, "memory_id")?,
        lexical_score: memory_u8(value, "lexical_score")?,
        content_checksum_score: memory_u8(value, "content_checksum_score")?,
        graph_score: memory_u8(value, "graph_score")?,
        recency_score: memory_u8(value, "recency_score")?,
        importance_score: memory_u8(value, "importance_score")?,
        scope_score: memory_u8(value, "scope_score")?,
        source_authority_score: memory_u8(value, "source_authority_score")?,
        final_score: memory_u16(value, "final_score")?,
        rank: memory_u16(value, "rank")?,
        source_receipt_id: memory_string(value, "source_receipt_id")?,
        receipt_id: memory_string(value, "receipt_id")?,
    })
}

fn surface_access_from_value(value: &serde_json::Value) -> Result<MemorySurfaceAccess, String> {
    Ok(MemorySurfaceAccess {
        access_id: memory_string(value, "access_id")?,
        tenant_id: mdx_core::TenantId::new(memory_string(value, "tenant_id")?),
        surface: memory_static(value, "surface")?,
        scope: memory_static(value, "scope")?,
        can_read: memory_bool(value, "can_read")?,
        can_write: memory_bool(value, "can_write")?,
        review_required: memory_bool(value, "review_required")?,
        receipt_id: memory_string(value, "receipt_id")?,
    })
}

/// Test-only seam invoked inside `write_target`, after the render and (with the
/// fix) while holding `SNAPSHOT_WRITE`. It lets a test pause one writer between
/// render and rename to force the stale-rename interleave deterministically.
/// None in normal runs, so it is a no-op even in the test binary until a test
/// arms it.
#[cfg(test)]
static WRITE_TARGET_RENDER_HOOK: Mutex<Option<Arc<dyn Fn() + Send + Sync>>> = Mutex::new(None);

#[cfg(test)]
fn set_write_target_render_hook(hook: Option<Arc<dyn Fn() + Send + Sync>>) {
    *WRITE_TARGET_RENDER_HOOK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = hook;
}

#[cfg(test)]
fn run_write_target_render_hook() {
    let hook = WRITE_TARGET_RENDER_HOOK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    if let Some(hook) = hook {
        hook();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdx_core::{
        ActorId, CorrelationIds, IdFactory, Ledger, LoopId, TenantId, TraceId, WorkflowId,
    };

    fn chained_entries(count: usize) -> Vec<Receipt> {
        let mut ledger = Ledger::default();
        let mut ids = IdFactory::default();
        let correlation = CorrelationIds {
            tenant_id: TenantId::new("tenant_local"),
            trace_id: TraceId::new("trace_snapshot_test"),
            actor_id: ActorId::new("human:snapshot_test"),
            loop_id: LoopId::new("snapshot_test_loop"),
            workflow_id: WorkflowId::new("wf_snapshot_test"),
        };
        for index in 0..count {
            let mut payload = BTreeMap::new();
            payload.insert("index".to_string(), index.to_string());
            ledger.append(
                &mut ids,
                &correlation,
                "snapshot.test.recorded",
                Some(format!("policy_{index}")),
                payload,
            );
        }
        ledger.entries().to_vec()
    }

    #[test]
    fn snapshot_round_trips_through_render_and_parse() {
        let entries = chained_entries(3);
        let rendered = render_snapshot(&entries, 7);

        let parsed: LedgerSnapshotOwned = serde_json::from_str(&rendered).expect("parse");
        let ids_counter = parsed.ids_counter;
        let restored_entries = parsed.receipts.into_iter().map(Receipt::from).collect();

        let mut restored = MdxKernel::boot_local();
        let count = restored
            .restore_ledger_entries(restored_entries)
            .expect("verified chain restores");
        restored.restore_ids_counter(ids_counter);
        assert_eq!(count, 3);
        assert_eq!(restored.ids_counter(), 7);
        assert_eq!(
            restored.ledger().entries().last().map(|r| r.hash.clone()),
            entries.last().map(|r| r.hash.clone())
        );
    }

    #[test]
    fn empty_ledger_bundle_is_structurally_serialized() {
        let kernel = MdxKernel::boot_local();
        let rendered = render_snapshots(&kernel).expect("render snapshots");
        let parsed: serde_json::Value = serde_json::from_str(&rendered.ledger).expect("parse");

        assert_eq!(parsed["name"], "mdx-kernel-ledger-snapshot");
        assert_eq!(parsed["version"], SNAPSHOT_VERSION);
        assert_eq!(parsed["receipt_count"], 0);
        assert_eq!(parsed["receipts"], serde_json::json!([]));
        assert_eq!(parsed["bundle_version"], SNAPSHOT_BUNDLE_VERSION);
        assert_eq!(
            parsed["memory_snapshot"]["version"],
            MEMORY_SNAPSHOT_VERSION
        );
    }

    #[test]
    fn tampered_chain_is_refused_whole() {
        let mut entries = chained_entries(3);
        entries[1].kind = "tampered.kind".to_string();
        let mut restored = MdxKernel::boot_local();
        assert!(restored.restore_ledger_entries(entries).is_err());
        assert!(restored.ledger().entries().is_empty());
    }

    #[test]
    fn restore_counter_never_moves_backwards() {
        let mut kernel = MdxKernel::boot_local();
        kernel.restore_ids_counter(9);
        assert_eq!(kernel.ids_counter(), 9);
        kernel.restore_ids_counter(2);
        assert_eq!(kernel.ids_counter(), 9);
    }

    /// A kernel with real governed memory: seeded receipts plus one surface
    /// memory write per (tenant, content) pair, in order.
    fn kernel_with_memory(writes: &[(&str, &str)]) -> MdxKernel {
        let mut kernel = MdxKernel::boot_local();
        kernel.run_evals_runner_agent().expect("loop run");
        for (tenant_id, content) in writes {
            let source_receipt_id = kernel
                .ledger()
                .entries()
                .last()
                .expect("source receipt")
                .receipt_id
                .clone();
            kernel
                .record_surface_memory_local(
                    tenant_id,
                    "human:snapshot_test",
                    "twin",
                    "private_user_memory",
                    &source_receipt_id,
                    content,
                )
                .expect("surface memory write");
        }
        kernel
    }

    /// A fresh kernel whose ledger matches the given kernel's rendered
    /// ledger snapshot - the boot-time restore sequence, without disk.
    fn kernel_with_restored_ledger(rendered_ledger: &str) -> MdxKernel {
        let parsed: LedgerSnapshotOwned = serde_json::from_str(rendered_ledger).expect("parse");
        let ids_counter = parsed.ids_counter;
        let entries = parsed.receipts.into_iter().map(Receipt::from).collect();
        let mut restored = MdxKernel::boot_local();
        restored
            .restore_ledger_entries(entries)
            .expect("ledger restores");
        restored.restore_ids_counter(ids_counter);
        restored
    }

    fn scratch_path(name: &str) -> String {
        std::env::temp_dir()
            .join(format!(
                "mdx-memory-snapshot-{}-{name}.json",
                std::process::id()
            ))
            .to_string_lossy()
            .into_owned()
    }

    fn temp_snapshot_path(tag: &str) -> String {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let seq = SEQ.fetch_add(1, Ordering::SeqCst);
        std::env::temp_dir()
            .join(format!(
                "mdx-snapshot-test-{}-{tag}-{seq}/kernel-ledger-snapshot.json",
                std::process::id()
            ))
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn memory_snapshot_round_trips_through_disk_into_a_fresh_kernel() {
        let kernel = kernel_with_memory(&[("tenant_local", "Durable memory survives a restart")]);
        let rendered = render_snapshots(&kernel).expect("render snapshots");
        let path = scratch_path("round-trip");
        std::fs::write(&path, &rendered.memory).expect("write memory snapshot");

        let mut restored = kernel_with_restored_ledger(&rendered.ledger);
        let count = restore_memory_from_path(&path, &mut restored)
            .expect("memory restores")
            .expect("memory snapshot present");
        std::fs::remove_file(&path).ok();

        // The seeded loop run writes one agent_operational_memory record of
        // its own, so the world carries that plus the governed write above.
        assert_eq!(count, kernel.memory_records().len());
        assert!(count >= 2);
        assert_eq!(restored.memory_records(), kernel.memory_records());
        assert_eq!(restored.memory_graph_nodes(), kernel.memory_graph_nodes());
        assert_eq!(restored.memory_graph_edges(), kernel.memory_graph_edges());
        assert_eq!(
            restored.memory_lifecycle_events(),
            kernel.memory_lifecycle_events()
        );
        assert_eq!(
            restored.memory_recall_rankings(),
            kernel.memory_recall_rankings()
        );
        assert_eq!(
            restored.memory_surface_access(),
            kernel.memory_surface_access()
        );
        // The projection a user actually reads survives the restart too.
        let projection = crate::memory_store::render_records_json(&restored);
        assert!(projection.contains("Durable memory survives a restart"));
        assert!(projection.contains(&format!("\"memory_record_count\":{count}")));
    }

    #[test]
    fn missing_memory_snapshot_boots_with_empty_memory() {
        let mut kernel = MdxKernel::boot_local();
        let restored = restore_memory_from_path(&scratch_path("never-written"), &mut kernel)
            .expect("absent file is not an error");
        assert_eq!(restored, None);
        assert!(kernel.memory_records().is_empty());
    }

    #[test]
    fn tampered_memory_snapshot_refuses_restore_whole() {
        let kernel = kernel_with_memory(&[("tenant_local", "Tamper evidence proof")]);
        let rendered = render_snapshots(&kernel).expect("render snapshots");
        let source_receipt_id = kernel.memory_records()[0].source_receipt_id.clone();

        // A record edited to cite a receipt the ledger never wrote refuses.
        let forged = rendered
            .memory
            .replace(&source_receipt_id, "receipt_forged_000001");
        let path = scratch_path("tampered");
        std::fs::write(&path, forged).expect("write tampered snapshot");
        let mut restored = kernel_with_restored_ledger(&rendered.ledger);
        let error = restore_memory_from_path(&path, &mut restored)
            .expect_err("record citing a missing receipt must refuse");
        assert!(error.contains("receipt_forged_000001"), "{error}");
        assert!(
            restored.memory_records().is_empty(),
            "refused restore must not partially apply"
        );

        // A file that no longer parses refuses too.
        let truncated = &rendered.memory[..rendered.memory.len() - 1];
        std::fs::write(&path, truncated).expect("write truncated snapshot");
        let error = restore_memory_from_path(&path, &mut restored)
            .expect_err("unparseable snapshot must refuse");
        assert!(error.contains("parse memory snapshot"), "{error}");
        assert!(restored.memory_records().is_empty());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn memory_snapshot_never_makes_ceremony_measurement_rows_durable() {
        let mut kernel = kernel_with_memory(&[("tenant_local", "Ceremony rows stay volatile")]);
        let correlation = CorrelationIds {
            tenant_id: TenantId::new("tenant_local"),
            trace_id: TraceId::new("trace_ceremony_test"),
            actor_id: ActorId::new("human:snapshot_test"),
            loop_id: LoopId::new("snapshot_test_loop"),
            workflow_id: WorkflowId::new("wf_snapshot_test"),
        };
        kernel
            .run_memory_brain_eval_harness(&correlation, "ceremony durability proof")
            .expect("eval harness");
        kernel
            .evaluate_memory_lifecycle(&correlation, "ceremony durability proof")
            .expect("lifecycle evaluation");
        kernel
            .run_memory_topology_validation(&correlation, "ceremony durability proof")
            .expect("topology validation");
        let live = kernel.memory_brain_snapshot();
        assert!(!live.eval_runs.is_empty(), "harness populated eval rows");

        let rendered = render_memory_snapshot(&live);
        let parsed = parse_memory_snapshot(&rendered).expect("parse");
        assert!(!parsed.records.is_empty());
        assert!(parsed.eval_runs.is_empty());
        assert!(parsed.vendor_comparator_runs.is_empty());
        assert!(parsed.production_topology_checks.is_empty());
        assert!(parsed.lifecycle_evaluations.is_empty());
        assert!(parsed.eval_fixture_results.is_empty());
        assert!(parsed.topology_runtime_events.is_empty());
        assert!(parsed.benchmark_imports.is_empty());
        assert!(parsed.scale_load_runs.is_empty());
        assert!(parsed.cloud_turn_on_checks.is_empty());
        assert!(!rendered.contains("eval_runs"));
        assert!(!rendered.contains("vendor_comparator_runs"));
    }

    #[test]
    fn pending_consolidations_and_validity_windows_survive_restart() {
        let mut kernel = kernel_with_memory(&[("tenant_local", "User prefers dark mode themes")]);
        // A shared-scope write lands pending ratification.
        let source_receipt_id = kernel
            .ledger()
            .entries()
            .last()
            .expect("source receipt")
            .receipt_id
            .clone();
        let pending = kernel
            .consolidate_surface_memory(
                "tenant_local",
                "human:snapshot_test",
                "messages",
                "team_memory",
                &source_receipt_id,
                "Team decided the beta gate ships first",
                "unadjudicated_verbatim",
            )
            .expect("pending consolidation");
        assert_eq!(
            pending.consolidation_state,
            mdx_core::MEMORY_CONSOLIDATION_PENDING
        );
        // A newer adjudicated fact supersedes the private record, closing
        // its validity window.
        let superseding = kernel
            .consolidate_surface_memory(
                "tenant_local",
                "human:snapshot_test",
                "twin",
                "private_user_memory",
                &source_receipt_id,
                "User prefers light mode themes",
                "adjudicated_fact_atom",
            )
            .expect("superseding consolidation");
        assert_eq!(superseding.decision, "SUPERSEDE");

        let rendered = render_snapshots(&kernel).expect("render snapshots");
        let path = scratch_path("pending-round-trip");
        std::fs::write(&path, &rendered.memory).expect("write memory snapshot");
        let mut restored = kernel_with_restored_ledger(&rendered.ledger);
        restore_memory_from_path(&path, &mut restored)
            .expect("memory restores")
            .expect("memory snapshot present");
        std::fs::remove_file(&path).ok();

        assert_eq!(restored.memory_records(), kernel.memory_records());
        let restored_pending = restored
            .pending_memory_consolidations()
            .into_iter()
            .find(|record| record.memory_id == pending.memory_id)
            .expect("pending record survives restart pending");
        assert_eq!(
            restored_pending.consolidation_state,
            mdx_core::MEMORY_CONSOLIDATION_PENDING
        );
        let superseded = restored
            .memory_records()
            .iter()
            .find(|record| record.memory_id == superseding.cited_memory_id)
            .expect("superseded record survives");
        assert!(!superseded.valid_until_receipt_timestamp.is_empty());
        assert_eq!(
            superseded.invalidated_by_receipt_id,
            superseding.gate_receipt_id
        );
    }

    fn read_identity_for(tenant_id: &str) -> mdx_core::AdmittedIdentity {
        mdx_core::AdmittedIdentity {
            deployment_mode: "local-secure",
            tenant_id: tenant_id.to_string(),
            actor_id: "human:isolation_test".to_string(),
            actor_role: "owner".to_string(),
            actor_kind: "human".to_string(),
            subject_actor_id: "human:isolation_test".to_string(),
            authority_scope: Vec::new(),
            delegation_id: None,
            identity_source: "trusted_session",
            production_write_allowed: false,
        }
    }

    #[test]
    fn restored_memory_stays_tenant_scoped_in_the_records_projection() {
        let kernel = kernel_with_memory(&[
            ("tenant_alpha", "Alpha private planning preference"),
            ("tenant_beta", "Beta private planning preference"),
        ]);
        let rendered = render_snapshots(&kernel).expect("render snapshots");
        let path = scratch_path("tenant-isolation");
        std::fs::write(&path, &rendered.memory).expect("write memory snapshot");
        let mut restored = kernel_with_restored_ledger(&rendered.ledger);
        restore_memory_from_path(&path, &mut restored)
            .expect("memory restores")
            .expect("memory snapshot present");
        std::fs::remove_file(&path).ok();

        // Restore preserves each record's tenant.
        assert!(
            restored
                .memory_records()
                .iter()
                .any(|record| record.tenant_id.as_str() == "tenant_alpha")
        );
        assert!(
            restored
                .memory_records()
                .iter()
                .any(|record| record.tenant_id.as_str() == "tenant_beta")
        );

        // A verified read session sees only its own tenant's memory.
        {
            let _guard = crate::request_security::set_verified_identity(Some(read_identity_for(
                "tenant_alpha",
            )));
            let body = crate::memory_store::render_records_json(&restored);
            assert!(body.contains("Alpha private planning preference"));
            assert!(!body.contains("Beta private planning preference"));
            let graph = crate::memory_store::render_graph_json(&restored);
            assert!(graph.contains("Alpha private planning preference"));
            assert!(!graph.contains("Beta private planning preference"));
        }
        {
            let _guard = crate::request_security::set_verified_identity(Some(read_identity_for(
                "tenant_beta",
            )));
            let body = crate::memory_store::render_records_json(&restored);
            assert!(body.contains("Beta private planning preference"));
            assert!(!body.contains("Alpha private planning preference"));
        }
        // local-demo reads carry no verified identity and keep the shared
        // local world.
        let body = crate::memory_store::render_records_json(&restored);
        assert!(body.contains("Alpha private planning preference"));
        assert!(body.contains("Beta private planning preference"));
    }
    fn kernel_with_entries(count: usize) -> Arc<RwLock<MdxKernel>> {
        let mut kernel = MdxKernel::boot_local();
        kernel
            .restore_ledger_entries(chained_entries(count))
            .expect("verified chain restores");
        Arc::new(RwLock::new(kernel))
    }

    // Durability semantic under the coalescing flusher: an explicit flush
    // persists every committed receipt, and a fresh boot restores them whole.
    // This is the checkpoint/shutdown guarantee the request-path debounce is
    // allowed to trade a small recency window against.
    #[test]
    fn flush_now_persists_every_committed_receipt() {
        let path = temp_snapshot_path("flush-now");
        let kernel = kernel_with_entries(64);
        let flusher = SnapshotFlusher::new(
            Arc::clone(&kernel),
            Duration::from_millis(250),
            path.clone(),
        );

        // A request alone must not be assumed durable, but an explicit flush is.
        flusher.request();
        flusher
            .flush_now()
            .expect("explicit flush writes the snapshot");

        let mut restored = MdxKernel::boot_local();
        let count = restore_from(&path, &mut restored)
            .expect("restore parses and verifies")
            .expect("snapshot exists after flush");
        assert_eq!(count, 64);
        assert_eq!(
            restored.ledger().entries().last().map(|r| r.hash.clone()),
            kernel
                .read()
                .unwrap()
                .ledger()
                .entries()
                .last()
                .map(|r| r.hash.clone()),
        );
        if let Some(parent) = std::path::Path::new(&path).parent() {
            let _ = std::fs::remove_dir_all(parent);
        }
    }

    #[test]
    fn flush_now_persists_memory_atomically_with_the_ledger() {
        let ledger_path = temp_snapshot_path("flush-now-memory-ledger");
        let memory_path = std::path::Path::new(&ledger_path)
            .with_file_name("memory-brain-snapshot.json")
            .to_string_lossy()
            .to_string();
        let kernel = Arc::new(RwLock::new(kernel_with_memory(&[(
            "tenant_local",
            "Durable memory follows its receipt",
        )])));
        let flusher = SnapshotFlusher::new_with_memory(
            Arc::clone(&kernel),
            Duration::from_millis(250),
            ledger_path.clone(),
            memory_path.clone(),
        );

        flusher.request();
        flusher
            .flush_now()
            .expect("explicit flush writes both snapshots");

        let mut restored = MdxKernel::boot_local();
        restore_from(&ledger_path, &mut restored)
            .expect("ledger restore parses and verifies")
            .expect("ledger snapshot exists");
        let memory_count = restore_memory_from_bundle_path(&ledger_path, &mut restored)
            .expect("embedded Memory restore parses and verifies")
            .expect("embedded Memory snapshot exists");
        assert_eq!(
            memory_count,
            kernel.read().unwrap().memory_records().len(),
            "the paired flush must not omit any durable memory records"
        );
        assert!(
            restored
                .memory_records()
                .iter()
                .any(|record| record.content == "Durable memory follows its receipt")
        );

        if let Some(parent) = std::path::Path::new(&ledger_path).parent() {
            let _ = std::fs::remove_dir_all(parent);
        }
    }

    #[test]
    fn embedded_memory_survives_a_crash_before_sibling_refresh() {
        let ledger_path = temp_snapshot_path("crash-before-memory-sibling");
        let memory_path = std::path::Path::new(&ledger_path)
            .with_file_name("memory-brain-snapshot.json")
            .to_string_lossy()
            .to_string();

        let previous = render_snapshots(&kernel_with_memory(&[(
            "tenant_local",
            "Previous durable memory",
        )]))
        .expect("render previous snapshots");
        write_snapshot_to(&ledger_path, &previous.ledger).expect("write previous ledger bundle");
        write_snapshot_to(&memory_path, &previous.memory).expect("write previous sibling");

        let current_kernel = kernel_with_memory(&[("tenant_local", "Current durable memory")]);
        let current = render_snapshots(&current_kernel).expect("render current snapshots");
        // This is the exact crash point under review: the authoritative ledger
        // bundle committed, but the derived sibling refresh never happened.
        write_snapshot_to(&ledger_path, &current.ledger).expect("commit current ledger bundle");
        let stale_sibling = std::fs::read_to_string(&memory_path).expect("read stale sibling");
        assert!(stale_sibling.contains("Previous durable memory"));
        assert!(!stale_sibling.contains("Current durable memory"));

        let mut restored = MdxKernel::boot_local();
        restore_from(&ledger_path, &mut restored)
            .expect("ledger restore parses and verifies")
            .expect("ledger bundle exists");
        restore_memory_from_bundle_path(&ledger_path, &mut restored)
            .expect("embedded Memory restore parses and verifies")
            .expect("embedded Memory snapshot exists");
        assert!(
            restored
                .memory_records()
                .iter()
                .any(|record| record.content == "Current durable memory")
        );
        assert!(
            restored
                .memory_records()
                .iter()
                .all(|record| record.content != "Previous durable memory")
        );

        if let Some(parent) = std::path::Path::new(&ledger_path).parent() {
            let _ = std::fs::remove_dir_all(parent);
        }
    }

    // Trailing writes must land within the window even if traffic then stops:
    // the background flusher guarantees no committed receipt is left unwritten
    // past one interval, which bounds what a hard kill can lose.
    #[test]
    fn background_flusher_persists_trailing_writes_within_the_window() {
        let path = temp_snapshot_path("background");
        let kernel = kernel_with_entries(32);
        let flusher =
            SnapshotFlusher::new(Arc::clone(&kernel), Duration::from_millis(20), path.clone());
        flusher.spawn();

        // One dirty mark, then silence - the flusher must still write it.
        flusher.request();

        let deadline = Instant::now() + Duration::from_secs(5);
        let restored_count = loop {
            let mut restored = MdxKernel::boot_local();
            match restore_from(&path, &mut restored) {
                Ok(Some(count)) => break count,
                Ok(None) => {}
                Err(error) => panic!("restore refused a written snapshot: {error}"),
            }
            assert!(
                Instant::now() < deadline,
                "background flusher never wrote the trailing snapshot"
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(restored_count, 32);
        if let Some(parent) = std::path::Path::new(&path).parent() {
            let _ = std::fs::remove_dir_all(parent);
        }
    }

    // FIX 1: the shutdown handler's flush action must land the trailing
    // coalescing window synchronously before the process exits. A dirty mark
    // with no background thread and no explicit flush leaves nothing on disk;
    // the shutdown flush is what a SIGTERM/SIGINT would run, and it must make
    // every committed receipt durable. Testing the factored `shutdown_flush`
    // against a local flusher exercises exactly what the signal waiter calls,
    // without needing to raise a real signal in the test process.
    #[test]
    fn shutdown_flush_persists_the_trailing_window() {
        let path = temp_snapshot_path("shutdown-flush");
        let kernel = kernel_with_entries(48);
        let flusher = SnapshotFlusher::new(
            Arc::clone(&kernel),
            Duration::from_millis(250),
            path.clone(),
        );

        // A dirty mark inside the window, no background flusher spawned, no
        // explicit flush yet: nothing is on disk, which is the exact loss the
        // old accept loop had on a graceful stop.
        flusher.request();
        let mut before = MdxKernel::boot_local();
        assert!(
            matches!(restore_from(&path, &mut before), Ok(None)),
            "nothing durable before the shutdown flush"
        );

        // The shutdown flush the signal waiter runs.
        shutdown_flush(Some(&flusher)).expect("shutdown flush writes the snapshot");

        let mut restored = MdxKernel::boot_local();
        let count = restore_from(&path, &mut restored)
            .expect("restore parses and verifies")
            .expect("snapshot exists after the shutdown flush");
        assert_eq!(count, 48);
        assert_eq!(
            restored.ledger().entries().last().map(|r| r.hash.clone()),
            kernel
                .read()
                .unwrap()
                .ledger()
                .entries()
                .last()
                .map(|r| r.hash.clone()),
        );
        if let Some(parent) = std::path::Path::new(&path).parent() {
            let _ = std::fs::remove_dir_all(parent);
        }
    }

    // FIX 1: in synchronous mode (no background flusher installed) the per-write
    // path already wrote, so the shutdown flush has nothing buffered and must be
    // a clean no-op rather than an error.
    #[test]
    fn shutdown_flush_without_a_flusher_is_a_noop() {
        assert!(shutdown_flush(None).is_ok());
    }

    // FIX 2: a `flush_now` on a request thread racing the background flusher
    // must never advance the watermark past what is on disk. The ledger grows
    // one receipt at a time (each with a dirty mark, like a governed write);
    // whenever the flusher reports `flushed == k`, the first k marks must be
    // durable, so the on-disk snapshot must hold at least BASE + k receipts. The
    // pre-fix code rendered outside the write mutex, so an older render could
    // rename last while the watermark advanced to a newer target - leaving the
    // file behind the watermark with `requested == flushed` and no pending work
    // to heal it. Serializing render+write under one guard makes that
    // impossible; this stresses the race and asserts the invariant throughout.
    #[test]
    fn concurrent_flush_now_and_background_never_leave_disk_behind_the_watermark() {
        use std::sync::atomic::AtomicBool;
        const BASE: usize = 8;
        const STEPS: usize = 200;
        let path = temp_snapshot_path("race-watermark");
        let full = chained_entries(BASE + STEPS);
        let kernel = kernel_with_entries(BASE);
        let flusher =
            SnapshotFlusher::new(Arc::clone(&kernel), Duration::from_millis(1), path.clone());
        flusher.spawn();

        let done = Arc::new(AtomicBool::new(false));

        // Grow the ledger one receipt at a time, marking dirty after each
        // commit, exactly as a governed write does.
        let writer = {
            let kernel = Arc::clone(&kernel);
            let flusher = Arc::clone(&flusher);
            let full = full.clone();
            let done = Arc::clone(&done);
            std::thread::spawn(move || {
                for i in 1..=STEPS {
                    {
                        let mut booted = kernel.write().unwrap();
                        booted
                            .restore_ledger_entries(full[..BASE + i].to_vec())
                            .expect("prefix chain restores");
                    }
                    flusher.request();
                    std::thread::yield_now();
                }
                done.store(true, Ordering::SeqCst);
            })
        };

        // Race flush_now against the background render+write.
        let hammer = {
            let flusher = Arc::clone(&flusher);
            let done = Arc::clone(&done);
            std::thread::spawn(move || {
                while !done.load(Ordering::SeqCst) {
                    flusher.flush_now().expect("flush_now writes");
                }
            })
        };

        // The watermark must never claim more than what is on disk. Read the
        // watermark first, then the file: `flushed == k` means the k-th mark is
        // durable, so the atomically renamed snapshot must already hold at least
        // BASE + k receipts. Sample in a tight loop (cheap count parse, no chain
        // verify) so the assertion actually lands inside the microsecond window
        // a stale rename would open.
        let on_disk_count = |path: &str| -> Option<u64> {
            let raw = std::fs::read_to_string(path).ok()?;
            let parsed: serde_json::Value = serde_json::from_str(&raw).ok()?;
            parsed["receipt_count"].as_u64()
        };
        let deadline = Instant::now() + Duration::from_secs(30);
        while !done.load(Ordering::SeqCst) {
            let flushed = flusher.lock().flushed;
            if flushed > 0
                && let Some(count) = on_disk_count(&path)
            {
                assert!(
                    count >= BASE as u64 + flushed,
                    "watermark {flushed} claims more than the {count} receipts on disk"
                );
            }
            assert!(Instant::now() < deadline, "race test did not converge");
        }

        writer.join().unwrap();
        hammer.join().unwrap();

        // After quiescence the disk must equal the full grown ledger.
        flusher.flush_now().expect("final flush");
        let mut restored = MdxKernel::boot_local();
        let count = restore_from(&path, &mut restored)
            .expect("restore parses and verifies")
            .expect("snapshot exists");
        assert_eq!(count, BASE + STEPS);
        assert_eq!(
            restored.ledger().entries().last().map(|r| r.hash.clone()),
            kernel
                .read()
                .unwrap()
                .ledger()
                .entries()
                .last()
                .map(|r| r.hash.clone()),
        );
        if let Some(parent) = std::path::Path::new(&path).parent() {
            let _ = std::fs::remove_dir_all(parent);
        }
    }

    // FIX 2, deterministic: force the exact stale-rename interleave the stress
    // test can only hit by luck. Writer A renders a SMALL ledger and is paused
    // between render and rename; the ledger then grows; writer B renders and
    // writes the LARGE ledger; A resumes and renames. The snapshot on disk must
    // reflect the newest committed ledger, not A's stale render.
    //
    // Because the fix holds SNAPSHOT_WRITE across the render, A holds the guard
    // while paused, so B blocks at the guard and can only render+write AFTER A
    // releases - B renders the current (large) ledger and its rename lands last.
    // On the pre-fix code A rendered outside the guard, so B would complete
    // first and A's later rename would clobber disk with the small ledger while
    // the watermark stayed high; this test fails there.
    #[test]
    fn stale_render_cannot_clobber_a_newer_snapshot() {
        use std::sync::atomic::AtomicBool;
        let path = temp_snapshot_path("stale-render");
        let full = chained_entries(64);
        // A starts with a small ledger.
        let kernel = kernel_with_entries(8);
        let flusher = SnapshotFlusher::new(
            Arc::clone(&kernel),
            Duration::from_millis(250),
            path.clone(),
        );

        let armed = Arc::new(AtomicBool::new(true));
        let paused = Arc::new((Mutex::new(false), Condvar::new()));
        let resume = Arc::new((Mutex::new(false), Condvar::new()));
        {
            let armed = Arc::clone(&armed);
            let paused = Arc::clone(&paused);
            let resume = Arc::clone(&resume);
            set_write_target_render_hook(Some(Arc::new(move || {
                // Only the first writer (A) pauses; B and any later call pass.
                if armed.swap(false, Ordering::SeqCst) {
                    let (lock, cv) = &*paused;
                    *lock.lock().unwrap() = true;
                    cv.notify_all();
                    let (lock, cv) = &*resume;
                    let mut go = lock.lock().unwrap();
                    while !*go {
                        go = cv.wait(go).unwrap();
                    }
                }
            })));
        }

        // A: render the small ledger, then pause holding the write guard.
        let writer_a = {
            let flusher = Arc::clone(&flusher);
            std::thread::spawn(move || flusher.flush_now().expect("A writes"))
        };
        {
            let (lock, cv) = &*paused;
            let mut is_paused = lock.lock().unwrap();
            while !*is_paused {
                is_paused = cv.wait(is_paused).unwrap();
            }
        }

        // The ledger grows while A is parked mid-write.
        kernel
            .write()
            .unwrap()
            .restore_ledger_entries(full.clone())
            .expect("grow to the large ledger");

        // B: race in a second writer for the large ledger. Under the fix it
        // blocks on the guard A holds; give it time to reach that state.
        let writer_b = {
            let flusher = Arc::clone(&flusher);
            std::thread::spawn(move || flusher.flush_now().expect("B writes"))
        };
        std::thread::sleep(Duration::from_millis(50));

        // Release A. Its stale rename must not win.
        {
            let (lock, cv) = &*resume;
            *lock.lock().unwrap() = true;
            cv.notify_all();
        }
        writer_a.join().unwrap();
        writer_b.join().unwrap();
        set_write_target_render_hook(None);

        // The durable snapshot must be the newest ledger, never A's stale eight.
        let mut restored = MdxKernel::boot_local();
        let count = restore_from(&path, &mut restored)
            .expect("restore parses and verifies")
            .expect("snapshot exists");
        assert_eq!(count, 64, "a stale render clobbered a newer snapshot");
        assert_eq!(
            restored.ledger().entries().last().map(|r| r.hash.clone()),
            full.last().map(|r| r.hash.clone()),
        );
        // And the watermark never claims more than what is on disk.
        assert!(count as u64 >= flusher.lock().flushed);
        if let Some(parent) = std::path::Path::new(&path).parent() {
            let _ = std::fs::remove_dir_all(parent);
        }
    }
}
