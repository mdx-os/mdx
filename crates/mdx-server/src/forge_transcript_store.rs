// The durable run transcript. A Forge run's model conversation lives in a
// thread-local Vec that dies when the OS thread exits, so a crashed or
// restarted run has only one recovery: redo from turn 0. This store gives
// the conversation a home on disk - the full messages array is written to a
// write file under .mdx-local per run, refreshed each turn, and the kernel
// records only the REFERENCE (the run keeps no receipt copy of prompts or
// tool bodies; the file IS the recorded transcript). Reads are confined to
// the store prefix with a sanitized name, so a transcript_ref can never walk
// the filesystem.
//
// The snapshot is the whole conversation each turn, not an event log to
// replay - the same shape the model call already re-sends every turn, so
// persisting it costs no more than a turn already does, and resume is a
// direct read rather than a reconstruction.
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) const TRANSCRIPTS_DIR: &str = ".mdx-local/forge-transcripts";
static SNAPSHOT_NONCE: AtomicU64 = AtomicU64::new(0);

// Run ids are agent/machine-minted (`forge_run_000123`,
// `forge_machine_league_native_trial_000001`), so they are already lowercase
// ASCII with digits, underscores, and dashes. A longer ceiling than the pages
// store leaves room for the longest run-kind prefixes; anything outside the
// alphabet or over the ceiling refuses to name a file.
fn sanitized(run_id: &str) -> Option<String> {
    if run_id.is_empty() || run_id.len() > 160 {
        return None;
    }
    if run_id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        Some(run_id.to_string())
    } else {
        None
    }
}

fn transcript_path(run_id: &str) -> Option<String> {
    let name = sanitized(run_id)?;
    Some(format!("{TRANSCRIPTS_DIR}/{name}.json"))
}

/// Persist the full conversation for a run, replacing any prior snapshot.
/// Returns the transcript_ref to record on the receipt, or None when the
/// run id cannot name a file or the messages do not serialize.
pub(crate) fn store_transcript(run_id: &str, messages: &[serde_json::Value]) -> Option<String> {
    let path = transcript_path(run_id)?;
    let body = serde_json::to_string(messages).ok()?;
    std::fs::create_dir_all(TRANSCRIPTS_DIR).ok()?;
    write_snapshot_atomically(&path, body.as_bytes())?;
    Some(path)
}

/// Write one complete snapshot without sharing a temporary name with another
/// writer. Exclusive creation prevents a pre-existing path from being followed,
/// and syncing before rename keeps an acknowledged snapshot from referring to
/// bytes that only existed in the process cache.
fn write_snapshot_atomically(path: &str, body: &[u8]) -> Option<()> {
    let nonce = SNAPSHOT_NONCE.fetch_add(1, Ordering::Relaxed);
    let temp = format!("{path}.{}.{nonce}.tmp", std::process::id());
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    let result = (|| -> std::io::Result<()> {
        let mut file = options.open(&temp)?;
        file.write_all(body)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp, path)?;
        sync_transcript_directory()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result.ok()
}

#[cfg(unix)]
fn sync_transcript_directory() -> std::io::Result<()> {
    std::fs::File::open(TRANSCRIPTS_DIR)?.sync_all()
}

#[cfg(not(unix))]
fn sync_transcript_directory() -> std::io::Result<()> {
    Ok(())
}

// Serve a stored transcript ONLY when the recorded reference sits inside the
// store directory and names a sanitized file.
fn read_confined(transcript_ref: &str) -> Option<String> {
    let rest = transcript_ref.strip_prefix(&format!("{TRANSCRIPTS_DIR}/"))?;
    let stem = rest.strip_suffix(".json")?;
    let valid = stem
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-');
    if !valid {
        return None;
    }
    std::fs::read_to_string(transcript_ref).ok()
}

/// Load a persisted conversation by transcript_ref (the value recorded on the
/// last transcript receipt). Returns the messages array when the reference is
/// confined to the store and parses as a JSON array; None otherwise.
pub(crate) fn load_transcript_by_ref(transcript_ref: &str) -> Option<Vec<serde_json::Value>> {
    let body = read_confined(transcript_ref)?;
    match serde_json::from_str::<serde_json::Value>(&body).ok()? {
        serde_json::Value::Array(messages) => Some(messages),
        _ => None,
    }
}

/// Load a persisted conversation by run id, for a resume that has the run id
/// but not the receipt in hand. Same confinement as the ref path.
pub(crate) fn load_transcript_by_run(run_id: &str) -> Option<Vec<serde_json::Value>> {
    let path = transcript_path(run_id)?;
    if !Path::new(&path).exists() {
        return None;
    }
    load_transcript_by_ref(&path)
}

/// Remove interrupted transcript publications only after their encoded owner
/// PID is dead. Complete snapshots and temp files owned by another live server
/// are preserved.
pub(crate) fn reconcile_stale_temps() -> Result<usize, String> {
    let directory = Path::new(TRANSCRIPTS_DIR);
    if !directory.exists() {
        return Ok(0);
    }
    let entries = std::fs::read_dir(directory).map_err(|error| error.to_string())?;
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.ends_with(".tmp") {
            continue;
        }
        let mut parts = name.rsplit('.');
        let _suffix = parts.next();
        let _nonce = parts.next();
        let Some(pid) = parts.next().and_then(|value| value.parse::<u32>().ok()) else {
            continue;
        };
        if crate::forge_workspace_checkpoint::process_is_alive(pid) {
            continue;
        }
        std::fs::remove_file(&path).map_err(|error| {
            format!(
                "could not remove stale transcript temp {}: {error}",
                path.display()
            )
        })?;
        removed += 1;
    }
    if removed > 0 {
        sync_transcript_directory().map_err(|error| error.to_string())?;
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refs_outside_the_store_never_read() {
        assert!(load_transcript_by_ref("docs/ONBOARDING.md").is_none());
        assert!(load_transcript_by_ref(".mdx-local/forge-transcripts/../secrets.json").is_none());
        assert!(load_transcript_by_ref(".mdx-local/forge-transcripts/UPPER.json").is_none());
    }

    #[test]
    fn bad_run_ids_refuse_to_store() {
        assert!(store_transcript("../evil", &[]).is_none());
        assert!(store_transcript("", &[]).is_none());
        assert!(store_transcript("Forge_Run_Upper", &[]).is_none());
    }

    #[test]
    fn store_and_load_round_trip_by_ref_and_by_run() {
        let run_id = "forge_transcript_store_test_run";
        let messages = vec![
            serde_json::json!({ "role": "system", "content": "You are a builder." }),
            serde_json::json!({ "role": "user", "content": "Fix the failing test." }),
            serde_json::json!({ "role": "assistant", "content": "Reading the file now." }),
        ];
        let transcript_ref = store_transcript(run_id, &messages).expect("stored");
        assert_eq!(transcript_ref, format!("{TRANSCRIPTS_DIR}/{run_id}.json"));
        let by_ref = load_transcript_by_ref(&transcript_ref).expect("loaded by ref");
        assert_eq!(by_ref, messages);
        let by_run = load_transcript_by_run(run_id).expect("loaded by run");
        assert_eq!(by_run, messages);
        let _ = std::fs::remove_file(&transcript_ref);
    }

    #[test]
    fn latest_snapshot_replaces_the_prior_one() {
        let run_id = "forge_transcript_store_replace_test";
        let first = vec![serde_json::json!({ "role": "user", "content": "turn one" })];
        store_transcript(run_id, &first).expect("first stored");
        let second = vec![
            serde_json::json!({ "role": "user", "content": "turn one" }),
            serde_json::json!({ "role": "assistant", "content": "turn two" }),
        ];
        let transcript_ref = store_transcript(run_id, &second).expect("second stored");
        assert_eq!(
            load_transcript_by_run(run_id).expect("loaded").len(),
            2,
            "resume reads the latest full conversation, not the first"
        );
        let _ = std::fs::remove_file(&transcript_ref);
    }

    #[test]
    fn concurrent_snapshots_never_share_a_temp_file_or_publish_partial_json() {
        let run_id = "forge_transcript_store_concurrent_test";
        let writers = 16;
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(writers));
        let handles: Vec<_> = (0..writers)
            .map(|writer| {
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let messages = vec![serde_json::json!({
                        "role": "assistant",
                        "content": format!("complete snapshot from writer {writer}"),
                        "writer": writer,
                    })];
                    barrier.wait();
                    store_transcript(run_id, &messages)
                })
            })
            .collect();

        for handle in handles {
            assert!(handle.join().expect("writer thread").is_some());
        }
        let published = load_transcript_by_run(run_id).expect("complete published snapshot");
        assert_eq!(published.len(), 1);
        assert_eq!(published[0]["role"], "assistant");
        assert!(published[0]["writer"].as_u64().is_some());

        let temp_prefix = format!("{run_id}.json.");
        let temp_files = std::fs::read_dir(TRANSCRIPTS_DIR)
            .expect("transcript directory")
            .filter_map(Result::ok)
            .filter(|entry| {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                name.starts_with(&temp_prefix) && name.ends_with(".tmp")
            })
            .count();
        assert_eq!(temp_files, 0, "successful writers clean every temp file");

        let transcript_ref = transcript_path(run_id).expect("valid run id");
        let _ = std::fs::remove_file(transcript_ref);
    }

    #[cfg(unix)]
    #[test]
    fn stored_transcripts_are_private_to_the_local_user() {
        use std::os::unix::fs::PermissionsExt;

        let run_id = "forge_transcript_store_mode_test";
        let transcript_ref = store_transcript(
            run_id,
            &[serde_json::json!({ "role": "user", "content": "private" })],
        )
        .expect("stored");
        let mode = std::fs::metadata(&transcript_ref)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
        let _ = std::fs::remove_file(transcript_ref);
    }

    #[test]
    fn missing_run_loads_nothing() {
        assert!(load_transcript_by_run("forge_run_that_never_persisted").is_none());
    }

    #[test]
    fn startup_reconciliation_removes_only_dead_writer_temps() {
        std::fs::create_dir_all(TRANSCRIPTS_DIR).expect("transcript directory");
        let nonce = SNAPSHOT_NONCE.fetch_add(1, Ordering::Relaxed);
        let dead = format!("{TRANSCRIPTS_DIR}/stale.json.99999999.{nonce}.tmp");
        let live = format!(
            "{TRANSCRIPTS_DIR}/live.json.{}.{nonce}.tmp",
            std::process::id()
        );
        std::fs::write(&dead, b"partial").expect("dead temp");
        std::fs::write(&live, b"active").expect("live temp");
        assert_eq!(reconcile_stale_temps().expect("reconciled"), 1);
        assert!(!Path::new(&dead).exists());
        assert!(Path::new(&live).exists());
        let _ = std::fs::remove_file(live);
    }
}
