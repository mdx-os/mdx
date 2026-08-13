// Stewardship: who owns a page, who reviews it, how often, and when it
// was last confirmed. The same audited-config posture as the Twin and
// Message controls - a plain hot-read file, every REAL change a
// write-once record with its actor. Freshness is DERIVED from these
// facts by the lifecycle projection: a page past its review interval is
// stale out loud, and a stale page is not citable by agents.
use crate::RouteResponse;
use std::path::Path;

pub(crate) const STEWARDSHIP_PATH: &str = ".mdx-local/pages-stewardship.json";
pub(crate) const CHANGES_DIR: &str = ".mdx-local/pages-stewardship-changes";

const INTERVAL_FLOOR_DAYS: u64 = 7;
const INTERVAL_CEILING_DAYS: u64 = 730;
pub(crate) const INTERVAL_DEFAULT_DAYS: u64 = 90;

pub(crate) struct Stewardship {
    raw: serde_json::Value,
}

impl Stewardship {
    pub(crate) fn load() -> Self {
        let raw = std::fs::read_to_string(Path::new(STEWARDSHIP_PATH))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or(serde_json::Value::Null);
        Self { raw }
    }

    pub(crate) fn entry(&self, document_id: &str) -> serde_json::Value {
        self.raw[document_id].clone()
    }

    pub(crate) fn owner(&self, document_id: &str) -> String {
        self.raw[document_id]["owner"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    }

    pub(crate) fn review_interval_days(&self, document_id: &str) -> u64 {
        self.raw[document_id]["review_interval_days"]
            .as_u64()
            .map(|days| days.clamp(INTERVAL_FLOOR_DAYS, INTERVAL_CEILING_DAYS))
            .unwrap_or(INTERVAL_DEFAULT_DAYS)
    }

    pub(crate) fn last_reviewed_epoch(&self, document_id: &str) -> u64 {
        self.raw[document_id]["last_reviewed_epoch_seconds"]
            .as_u64()
            .unwrap_or(0)
    }
}

pub(crate) fn route_response(method: &str, path: &str, body: &str) -> Option<RouteResponse> {
    if path == "/pages/stewardship-changes/projection.json" {
        if !method.eq_ignore_ascii_case("GET") {
            return Some(method_not_allowed());
        }
        return Some(RouteResponse::json("200 OK", render_changes_json()));
    }
    if path != "/pages/stewardship.json" {
        return None;
    }
    if method.eq_ignore_ascii_case("POST") {
        return Some(apply_update(body));
    }
    if !method.eq_ignore_ascii_case("GET") {
        return Some(method_not_allowed());
    }
    Some(RouteResponse::json("200 OK", render_json("")))
}

fn now_epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

fn apply_update(body: &str) -> RouteResponse {
    let incoming: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
    let Some(document_id) = incoming["document_id"].as_str().filter(|id| !id.is_empty()) else {
        // The empty smoke probe: a no-op answer, nothing recorded.
        return RouteResponse::json("200 OK", render_json(""));
    };
    let actor_id = incoming["actor_id"].as_str().unwrap_or("local_user");
    let current = Stewardship::load();
    let mut entry = match current.entry(document_id) {
        serde_json::Value::Object(map) => map,
        _ => serde_json::Map::new(),
    };
    let mut changed: Vec<serde_json::Value> = Vec::new();
    for key in ["owner", "reviewer"] {
        if let Some(value) = incoming[key].as_str() {
            let previous = entry
                .get(key)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            if previous != value {
                changed
                    .push(serde_json::json!({ "key": key, "previous": previous, "next": value }));
                entry.insert(key.to_string(), serde_json::Value::from(value));
            }
        }
    }
    if let Some(days) = incoming["review_interval_days"].as_u64() {
        let next = days.clamp(INTERVAL_FLOOR_DAYS, INTERVAL_CEILING_DAYS);
        let previous = current.review_interval_days(document_id);
        if previous != next {
            changed.push(
                serde_json::json!({ "key": "review_interval_days", "previous": previous, "next": next }),
            );
            entry.insert(
                "review_interval_days".to_string(),
                serde_json::Value::from(next),
            );
        }
    }
    // mark_reviewed stamps now; last_reviewed_epoch_seconds allows an
    // honest backfill ("we reviewed this in March") - both recorded.
    let reviewed_at = if incoming["mark_reviewed"].as_bool() == Some(true) {
        Some(now_epoch())
    } else {
        incoming["last_reviewed_epoch_seconds"].as_u64()
    };
    if let Some(stamp) = reviewed_at {
        let previous = current.last_reviewed_epoch(document_id);
        if previous != stamp {
            changed.push(
                serde_json::json!({ "key": "last_reviewed_epoch_seconds", "previous": previous, "next": stamp }),
            );
            entry.insert(
                "last_reviewed_epoch_seconds".to_string(),
                serde_json::Value::from(stamp),
            );
        }
    }
    let mut root = match Stewardship::load().raw {
        serde_json::Value::Object(map) => map,
        _ => serde_json::Map::new(),
    };
    root.insert(document_id.to_string(), serde_json::Value::Object(entry));
    let _ = std::fs::create_dir_all(".mdx-local");
    if std::fs::write(
        STEWARDSHIP_PATH,
        serde_json::to_string_pretty(&serde_json::Value::Object(root)).unwrap_or_default(),
    )
    .is_err()
    {
        return RouteResponse::json(
            "200 OK",
            r#"{"name":"mdx-pages-stewardship","status":"WRITE_FAILED"}"#.to_string(),
        );
    }
    let record_id = record_change(document_id, &changed, actor_id);
    let note = match &record_id {
        Some(id) => format!(r#","change_recorded":true,"change_record_id":"{id}""#),
        None if changed.is_empty() => r#","change_recorded":false,"change_record_id":"""#.into(),
        None => r#","change_recorded":false,"change_record_id":"","change_record_failed":true"#
            .to_string(),
    };
    RouteResponse::json("200 OK", render_json(&note))
}

fn record_change(
    document_id: &str,
    changed: &[serde_json::Value],
    actor_id: &str,
) -> Option<String> {
    if changed.is_empty() {
        return None;
    }
    let dir = Path::new(CHANGES_DIR);
    let sequence = list_changes().len() + 1;
    let record_id = format!("pages_stewardship_change_{sequence:06}");
    let record = serde_json::json!({
        "record_id": record_id,
        "source_route": "/pages/stewardship.json",
        "document_id": document_id,
        "actor_id": actor_id,
        "recorded_at_epoch_seconds": now_epoch(),
        "changed": changed,
    });
    std::fs::create_dir_all(dir).ok()?;
    std::fs::write(
        dir.join(format!("{record_id}.json")),
        serde_json::to_string_pretty(&record).ok()?,
    )
    .ok()?;
    Some(record_id)
}

fn list_changes() -> Vec<serde_json::Value> {
    let Ok(entries) = std::fs::read_dir(CHANGES_DIR) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.starts_with("pages_stewardship_change_") && name.ends_with(".json"))
        .collect();
    names.sort();
    names
        .iter()
        .filter_map(|name| std::fs::read_to_string(Path::new(CHANGES_DIR).join(name)).ok())
        .filter_map(|text| serde_json::from_str(&text).ok())
        .collect()
}

fn method_not_allowed() -> RouteResponse {
    RouteResponse::text("405 Method Not Allowed", "method not allowed\n".to_string())
}

fn render_json(note: &str) -> String {
    let stewardship = Stewardship::load();
    let body = match &stewardship.raw {
        serde_json::Value::Object(map) => serde_json::Value::Object(map.clone()),
        _ => serde_json::json!({}),
    };
    format!(
        r#"{{"name":"mdx-pages-stewardship","status":"OK","config_path":"{STEWARDSHIP_PATH}","review_interval_default_days":{INTERVAL_DEFAULT_DAYS},"changes_route":"/pages/stewardship-changes/projection.json","documents":{}{note}}}"#,
        serde_json::to_string(&body).unwrap_or_else(|_| "{}".to_string())
    )
}

fn render_changes_json() -> String {
    let mut records = list_changes();
    records.reverse();
    serde_json::json!({
        "name": "mdx-pages-stewardship-changes",
        "status": "OK",
        "change_count": records.len(),
        "changes": records,
    })
    .to_string()
}
