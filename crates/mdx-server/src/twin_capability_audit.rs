// Capability changes leave a record. The config file stays the hot-read
// truth the guards consult; this directory is the trail that explains it:
// which switch moved, from what, to what, on whose say-so, and when.
// Same durable-evidence posture as provider turn-on - write-once files
// under .mdx-local, listable through a declared projection route - and an
// unchanged POST writes nothing, so the trail only ever records real moves.
use std::path::Path;

pub(crate) const CHANGES_DIR: &str = ".mdx-local/twin-capability-changes";

pub(crate) struct CapabilityChange {
    pub key: &'static str,
    pub previous: bool,
    pub next: bool,
}

// Record one real change set. Returns the record id, or None when there is
// nothing to record or the record could not land (the caller reports that
// honestly rather than pretending the trail has it).
pub(crate) fn record_change(changes: &[CapabilityChange], actor_id: &str) -> Option<String> {
    record_change_in(Path::new(CHANGES_DIR), changes, actor_id)
}

pub(crate) fn record_change_in(
    dir: &Path,
    changes: &[CapabilityChange],
    actor_id: &str,
) -> Option<String> {
    if changes.is_empty() {
        return None;
    }
    let sequence = next_sequence(dir);
    let record_id = format!("twin_capability_change_{sequence:06}");
    let recorded_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let changed: Vec<serde_json::Value> = changes
        .iter()
        .map(|change| {
            serde_json::json!({
                "key": change.key,
                "previous": change.previous,
                "next": change.next,
            })
        })
        .collect();
    let record = serde_json::json!({
        "record_id": record_id,
        "source_route": "/twin/capabilities.json",
        "actor_id": actor_id,
        "recorded_at_epoch_seconds": recorded_at,
        "changed": changed,
    });
    std::fs::create_dir_all(dir).ok()?;
    let body = serde_json::to_string_pretty(&record).ok()?;
    std::fs::write(dir.join(format!("{record_id}.json")), body).ok()?;
    Some(record_id)
}

// The next sequence continues past every record already on disk, so the
// trail stays strictly ordered even across server restarts.
fn next_sequence(dir: &Path) -> usize {
    list_records_in(dir)
        .iter()
        .filter_map(|record| record["record_id"].as_str())
        .filter_map(|id| id.rsplit('_').next())
        .filter_map(|tail| tail.parse::<usize>().ok())
        .max()
        .unwrap_or(0)
        + 1
}

pub(crate) fn list_records() -> Vec<serde_json::Value> {
    list_records_in(Path::new(CHANGES_DIR))
}

// Every well-formed record in the trail, oldest first. Unreadable files are
// skipped rather than failing the whole projection - the trail shows what it
// can prove.
pub(crate) fn list_records_in(dir: &Path) -> Vec<serde_json::Value> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.starts_with("twin_capability_change_") && name.ends_with(".json"))
        .collect();
    names.sort();
    names
        .iter()
        .filter_map(|name| std::fs::read_to_string(dir.join(name)).ok())
        .filter_map(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .filter(|record| record["record_id"].is_string() && record["changed"].is_array())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mdx-twin-capability-audit-{}-{}",
            name,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn empty_change_set_records_nothing() {
        let dir = scratch_dir("noop");
        assert!(record_change_in(&dir, &[], "local_user").is_none());
        assert!(!dir.exists());
    }

    #[test]
    fn changes_land_ordered_with_actor_and_values() {
        let dir = scratch_dir("ordered");
        let first = record_change_in(
            &dir,
            &[CapabilityChange {
                key: "skill_record_decision",
                previous: true,
                next: false,
            }],
            "you_v1_engineering_direct",
        )
        .expect("first record lands");
        let second = record_change_in(
            &dir,
            &[CapabilityChange {
                key: "skill_record_decision",
                previous: false,
                next: true,
            }],
            "local_user",
        )
        .expect("second record lands");
        assert_eq!(first, "twin_capability_change_000001");
        assert_eq!(second, "twin_capability_change_000002");
        let records = list_records_in(&dir);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0]["actor_id"], "you_v1_engineering_direct");
        assert_eq!(records[0]["changed"][0]["key"], "skill_record_decision");
        assert_eq!(records[0]["changed"][0]["previous"], true);
        assert_eq!(records[0]["changed"][0]["next"], false);
        assert_eq!(records[1]["record_id"], "twin_capability_change_000002");
        assert!(
            records[1]["recorded_at_epoch_seconds"]
                .as_u64()
                .unwrap_or(0)
                > 0
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
