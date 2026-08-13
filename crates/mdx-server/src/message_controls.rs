// Message operator controls: the same posture as Twin's capabilities -
// a plain hot-read file under .mdx-local, served and updated through
// declared routes, with every REAL change leaving a write-once record.
// The write path enforces these server-side: an agent posting ban is the
// kernel's refusal, never a hidden client filter.
use crate::RouteResponse;
use std::path::Path;

pub(crate) const CONTROLS_PATH: &str = ".mdx-local/message-controls.json";
pub(crate) const CHANGES_DIR: &str = ".mdx-local/message-control-changes";

// Switches (absent file or key = ON, fail-safe) and one bounded number.
pub(crate) const SWITCH_KEYS: &[&str] = &[
    "agent_posting_allowed",
    "reactions_enabled",
    "slash_commands_enabled",
];
pub(crate) const MAX_BODY_KEY: &str = "max_message_body_chars";
const MAX_BODY_DEFAULT: usize = 4000;
const MAX_BODY_FLOOR: usize = 100;
const MAX_BODY_CEILING: usize = 20000;

pub(crate) struct MessageControls {
    raw: serde_json::Value,
}

impl MessageControls {
    pub(crate) fn load() -> Self {
        let raw = std::fs::read_to_string(Path::new(CONTROLS_PATH))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or(serde_json::Value::Null);
        Self { raw }
    }

    pub(crate) fn enabled(&self, key: &str) -> bool {
        self.raw[key].as_bool().unwrap_or(true)
    }

    pub(crate) fn max_body_chars(&self) -> usize {
        self.raw[MAX_BODY_KEY]
            .as_u64()
            .map(|value| (value as usize).clamp(MAX_BODY_FLOOR, MAX_BODY_CEILING))
            .unwrap_or(MAX_BODY_DEFAULT)
    }
}

pub(crate) fn route_response(method: &str, path: &str, body: &str) -> Option<RouteResponse> {
    if path == "/messages/control-changes/projection.json" {
        if !method.eq_ignore_ascii_case("GET") {
            return Some(method_not_allowed());
        }
        return Some(RouteResponse::json("200 OK", render_changes_json()));
    }
    if path != "/messages/controls.json" {
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

fn apply_update(body: &str) -> RouteResponse {
    let current = MessageControls::load();
    let incoming: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
    let actor_id = incoming["actor_id"].as_str().unwrap_or("local_user");
    let mut next = serde_json::Map::new();
    let mut changed: Vec<serde_json::Value> = Vec::new();
    for key in SWITCH_KEYS {
        let previous = current.enabled(key);
        let value = incoming[*key].as_bool().unwrap_or(previous);
        if value != previous {
            changed.push(serde_json::json!({ "key": key, "previous": previous, "next": value }));
        }
        next.insert((*key).to_string(), serde_json::Value::Bool(value));
    }
    let previous_max = current.max_body_chars();
    let next_max = incoming[MAX_BODY_KEY]
        .as_u64()
        .map(|value| (value as usize).clamp(MAX_BODY_FLOOR, MAX_BODY_CEILING))
        .unwrap_or(previous_max);
    if next_max != previous_max {
        changed.push(
            serde_json::json!({ "key": MAX_BODY_KEY, "previous": previous_max, "next": next_max }),
        );
    }
    next.insert(
        MAX_BODY_KEY.to_string(),
        serde_json::Value::from(next_max as u64),
    );
    let _ = std::fs::create_dir_all(".mdx-local");
    if std::fs::write(
        CONTROLS_PATH,
        serde_json::to_string_pretty(&serde_json::Value::Object(next)).unwrap_or_default(),
    )
    .is_err()
    {
        return RouteResponse::json(
            "200 OK",
            r#"{"name":"mdx-message-controls","status":"WRITE_FAILED"}"#.to_string(),
        );
    }
    let record_id = record_change(&changed, actor_id);
    let note = match &record_id {
        Some(id) => format!(r#","change_recorded":true,"change_record_id":"{id}""#),
        None if changed.is_empty() => r#","change_recorded":false,"change_record_id":"""#.into(),
        None => r#","change_recorded":false,"change_record_id":"","change_record_failed":true"#
            .to_string(),
    };
    RouteResponse::json("200 OK", render_json(&note))
}

fn record_change(changed: &[serde_json::Value], actor_id: &str) -> Option<String> {
    if changed.is_empty() {
        return None;
    }
    let dir = Path::new(CHANGES_DIR);
    let sequence = list_changes().len() + 1;
    let record_id = format!("message_control_change_{sequence:06}");
    let recorded_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let record = serde_json::json!({
        "record_id": record_id,
        "source_route": "/messages/controls.json",
        "actor_id": actor_id,
        "recorded_at_epoch_seconds": recorded_at,
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
        .filter(|name| name.starts_with("message_control_change_") && name.ends_with(".json"))
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
    let controls = MessageControls::load();
    let switches = SWITCH_KEYS
        .iter()
        .map(|key| format!(r#""{key}":{}"#, controls.enabled(key)))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"{{"name":"mdx-message-controls","status":"OK","config_path":"{CONTROLS_PATH}","absent_means":"everything on","changes_route":"/messages/control-changes/projection.json","keys":{{{switches},"{MAX_BODY_KEY}":{}}}{note}}}"#,
        controls.max_body_chars()
    )
}

fn render_changes_json() -> String {
    let mut records = list_changes();
    records.reverse();
    serde_json::json!({
        "name": "mdx-message-control-changes",
        "status": "OK",
        "config_path": CONTROLS_PATH,
        "change_count": records.len(),
        "changes": records,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_fail_safe_and_bounds_hold() {
        let controls = MessageControls {
            raw: serde_json::Value::Null,
        };
        assert!(controls.enabled("agent_posting_allowed"));
        assert_eq!(controls.max_body_chars(), 4000);
        let bounded = MessageControls {
            raw: serde_json::json!({ "max_message_body_chars": 999999 }),
        };
        assert_eq!(bounded.max_body_chars(), 20000);
        let floored = MessageControls {
            raw: serde_json::json!({ "max_message_body_chars": 1 }),
        };
        assert_eq!(floored.max_body_chars(), 100);
    }
}
