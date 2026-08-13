use std::collections::VecDeque;
use std::io::{self, Read};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use mdx_core::hex;

const MAX_CACHE_ENTRIES: usize = 16;

#[derive(Clone, Debug)]
pub(super) struct MachineRuntimeStaticObservation {
    pub(super) binary_path: String,
    pub(super) binary_present: bool,
    pub(super) version_command: String,
    pub(super) version_observed: String,
    pub(super) version_raw_output: String,
    pub(super) checksum_sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MachineRuntimeCacheKey {
    adapter_kind: String,
    canonical_path: String,
    file_len: u64,
    modified_nanos: u128,
    change_marker: String,
}

#[derive(Clone, Debug)]
struct CachedMachineRuntimeObservation {
    key: MachineRuntimeCacheKey,
    observation: Arc<OnceLock<MachineRuntimeStaticObservation>>,
}

pub(super) fn cached_machine_runtime_static_observation(
    adapter_kind: &str,
    binary_path: &str,
    observe: impl FnOnce() -> MachineRuntimeStaticObservation,
) -> MachineRuntimeStaticObservation {
    let Some(key) = machine_runtime_cache_key(adapter_kind, binary_path) else {
        return observe();
    };
    let observation_slot = {
        let mut cache = machine_runtime_static_cache()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(entry) = cache.iter().find(|entry| entry.key == key) {
            entry.observation.clone()
        } else {
            cache.retain(|entry| entry.key.adapter_kind != key.adapter_kind);
            while cache.len() >= MAX_CACHE_ENTRIES {
                cache.pop_front();
            }
            let observation = Arc::new(OnceLock::new());
            cache.push_back(CachedMachineRuntimeObservation {
                key: key.clone(),
                observation: observation.clone(),
            });
            observation
        }
    };

    let observation = observation_slot.get_or_init(observe).clone();
    let identity_is_current =
        machine_runtime_cache_key(adapter_kind, binary_path).as_ref() == Some(&key);
    if observation.checksum_sha256.is_empty()
        || observation
            .version_raw_output
            .starts_with("version_probe_failed:")
        || !identity_is_current
    {
        remove_machine_runtime_static_cache_entry(&key, &observation_slot);
    }
    observation
}

pub(super) fn sha256_file_hex(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    sha256_reader_hex(file).ok()
}

fn sha256_reader_hex(mut reader: impl Read) -> io::Result<String> {
    let mut digest = ring::digest::Context::new(&ring::digest::SHA256);
    let mut buffer = [0u8; 64 * 1024];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => return Ok(hex(digest.finish().as_ref())),
            Ok(read) => digest.update(&buffer[..read]),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
}

fn machine_runtime_static_cache() -> &'static Mutex<VecDeque<CachedMachineRuntimeObservation>> {
    static CACHE: OnceLock<Mutex<VecDeque<CachedMachineRuntimeObservation>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(VecDeque::new()))
}

fn remove_machine_runtime_static_cache_entry(
    key: &MachineRuntimeCacheKey,
    observation: &Arc<OnceLock<MachineRuntimeStaticObservation>>,
) {
    let mut cache = machine_runtime_static_cache()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    cache.retain(|entry| entry.key != *key || !Arc::ptr_eq(&entry.observation, observation));
}

fn machine_runtime_cache_key(
    adapter_kind: &str,
    binary_path: &str,
) -> Option<MachineRuntimeCacheKey> {
    let canonical_path = std::fs::canonicalize(binary_path).ok()?;
    let metadata = std::fs::metadata(&canonical_path).ok()?;
    let modified_nanos = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    Some(MachineRuntimeCacheKey {
        adapter_kind: adapter_kind.to_string(),
        canonical_path: canonical_path.display().to_string(),
        file_len: metadata.len(),
        modified_nanos,
        change_marker: machine_runtime_metadata_change_marker(&metadata),
    })
}

#[cfg(unix)]
fn machine_runtime_metadata_change_marker(metadata: &std::fs::Metadata) -> String {
    use std::os::unix::fs::MetadataExt;
    format!(
        "dev={}:ino={}:ctime={}:ctime_nsec={}",
        metadata.dev(),
        metadata.ino(),
        metadata.ctime(),
        metadata.ctime_nsec()
    )
}

#[cfg(not(unix))]
fn machine_runtime_metadata_change_marker(_metadata: &std::fs::Metadata) -> String {
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn runtime_fixture(name: &str, body: &[u8]) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "mdx-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("current time")
                .as_nanos()
        ));
        std::fs::write(&path, body).expect("write runtime fixture");
        path
    }

    fn observation(path: &Path, call: usize) -> MachineRuntimeStaticObservation {
        MachineRuntimeStaticObservation {
            binary_path: path.display().to_string(),
            binary_present: true,
            version_command: "runtime --version".to_string(),
            version_observed: format!("version-{call}"),
            version_raw_output: format!("version-{call}"),
            checksum_sha256: format!("sha256:{call}"),
        }
    }

    #[test]
    fn ring_stream_hash_matches_sha256_vector() {
        assert_eq!(
            sha256_reader_hex(std::io::Cursor::new(b"abc")).expect("hash"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn static_cache_reuses_identity_invalidates_changes_and_retries_failures() {
        let binary = runtime_fixture("runtime-cache", b"first runtime");
        let calls = AtomicUsize::new(0);
        let observe = || {
            let call = calls.fetch_add(1, Ordering::SeqCst) + 1;
            observation(&binary, call)
        };

        let first = cached_machine_runtime_static_observation(
            "runtime_cache_fixture",
            &binary.display().to_string(),
            observe,
        );
        let second = cached_machine_runtime_static_observation(
            "runtime_cache_fixture",
            &binary.display().to_string(),
            observe,
        );
        assert_eq!(first.version_observed, "version-1");
        assert_eq!(second.version_observed, "version-1");
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        std::fs::write(&binary, b"second runtime with a different length")
            .expect("replace runtime");
        let replacement = cached_machine_runtime_static_observation(
            "runtime_cache_fixture",
            &binary.display().to_string(),
            observe,
        );
        assert_eq!(replacement.version_observed, "version-2");

        let failed_calls = AtomicUsize::new(0);
        let failed_observe = || {
            failed_calls.fetch_add(1, Ordering::SeqCst);
            MachineRuntimeStaticObservation {
                binary_path: binary.display().to_string(),
                binary_present: true,
                version_command: "runtime --version".to_string(),
                version_observed: String::new(),
                version_raw_output: "version_probe_failed: timeout".to_string(),
                checksum_sha256: String::new(),
            }
        };
        for _ in 0..2 {
            cached_machine_runtime_static_observation(
                "runtime_cache_failure_fixture",
                &binary.display().to_string(),
                failed_observe,
            );
        }
        assert_eq!(failed_calls.load(Ordering::SeqCst), 2);
        std::fs::remove_file(binary).expect("remove runtime fixture");
    }

    #[test]
    fn static_cache_single_flights_concurrent_misses() {
        let binary = runtime_fixture("runtime-cache-singleflight", b"single flight runtime");
        let calls = AtomicUsize::new(0);
        let barrier = std::sync::Barrier::new(8);

        std::thread::scope(|scope| {
            let handles = (0..8)
                .map(|_| {
                    let binary = &binary;
                    let calls = &calls;
                    let barrier = &barrier;
                    scope.spawn(move || {
                        barrier.wait();
                        cached_machine_runtime_static_observation(
                            "runtime_cache_singleflight_fixture",
                            &binary.display().to_string(),
                            || {
                                calls.fetch_add(1, Ordering::SeqCst);
                                std::thread::sleep(Duration::from_millis(20));
                                observation(binary, 1)
                            },
                        )
                    })
                })
                .collect::<Vec<_>>();
            for handle in handles {
                assert_eq!(
                    handle.join().expect("probe thread").version_observed,
                    "version-1"
                );
            }
        });

        assert_eq!(calls.load(Ordering::SeqCst), 1);
        std::fs::remove_file(binary).expect("remove runtime fixture");
    }
}
