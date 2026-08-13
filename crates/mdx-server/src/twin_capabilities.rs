// The Twin capabilities routes: what the admin surface reads and
// writes. Same durable-evidence pattern as provider turn-on: a plain
// file under .mdx-local, served and updated through declared routes.
// POST merges only KNOWN keys with boolean values - an empty body is a
// no-op, unknown keys never land, absent keys stay at their current
// value (and absent file means everything on). Every REAL change writes
// an audit record - who flipped what, from what, to what - and the
// trail is served whole at /twin/capability-changes/projection.json.
use crate::RouteResponse;
use crate::twin_capability_audit::{self, CapabilityChange};
use crate::twin_guards::{CAPABILITIES_PATH, CAPABILITY_KEYS, Capabilities};

pub(crate) fn route_response(method: &str, path: &str, body: &str) -> Option<RouteResponse> {
    if path == "/twin/capability-changes/projection.json" {
        if !method.eq_ignore_ascii_case("GET") {
            return Some(method_not_allowed());
        }
        return Some(RouteResponse::json("200 OK", render_projection_json()));
    }
    if path != "/twin/capabilities.json" {
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
    let current = Capabilities::load();
    let incoming: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
    let actor_id = incoming["actor_id"].as_str().unwrap_or("local_user");
    let mut next = serde_json::Map::new();
    let mut changes: Vec<CapabilityChange> = Vec::new();
    for key in CAPABILITY_KEYS {
        let previous = current.enabled(key);
        let value = incoming[*key].as_bool().unwrap_or(previous);
        if value != previous {
            changes.push(CapabilityChange {
                key,
                previous,
                next: value,
            });
        }
        next.insert((*key).to_string(), serde_json::Value::Bool(value));
    }
    let _ = std::fs::create_dir_all(".mdx-local");
    if std::fs::write(
        CAPABILITIES_PATH,
        serde_json::to_string_pretty(&serde_json::Value::Object(next)).unwrap_or_default(),
    )
    .is_err()
    {
        return RouteResponse::json(
            "200 OK",
            r#"{"name":"mdx-twin-capabilities","status":"WRITE_FAILED"}"#.to_string(),
        );
    }
    // The trail records only real moves: an unchanged POST (including the
    // empty-body smoke probe) leaves no record because nothing happened.
    let record_id = twin_capability_audit::record_change(&changes, actor_id);
    let change_note = match &record_id {
        Some(id) => format!(r#","change_recorded":true,"change_record_id":"{id}""#),
        None if changes.is_empty() => r#","change_recorded":false,"change_record_id":"""#.into(),
        // The switch moved but the record did not land - say so rather than
        // pretending the trail has it.
        None => r#","change_recorded":false,"change_record_id":"","change_record_failed":true"#
            .to_string(),
    };
    RouteResponse::json("200 OK", render_json(&change_note))
}

fn method_not_allowed() -> RouteResponse {
    RouteResponse::text("405 Method Not Allowed", "method not allowed\n".to_string())
}

fn render_json(change_note: &str) -> String {
    let capabilities = Capabilities::load();
    let keys = CAPABILITY_KEYS
        .iter()
        .map(|key| format!(r#""{key}":{}"#, capabilities.enabled(key)))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"{{"name":"mdx-twin-capabilities","status":"OK","config_path":"{CAPABILITIES_PATH}","absent_means":"everything on","changes_route":"/twin/capability-changes/projection.json","keys":{{{keys}}}{change_note}}}"#
    )
}

// The whole trail, newest first, plus the current switch values so one
// read answers both "what is set" and "how did it get this way".
fn render_projection_json() -> String {
    let mut records = twin_capability_audit::list_records();
    records.reverse();
    let capabilities = Capabilities::load();
    let mut current = serde_json::Map::new();
    for key in CAPABILITY_KEYS {
        current.insert(
            (*key).to_string(),
            serde_json::Value::Bool(capabilities.enabled(key)),
        );
    }
    serde_json::json!({
        "name": "mdx-twin-capability-changes",
        "status": "OK",
        "config_path": CAPABILITIES_PATH,
        "change_count": records.len(),
        "changes": records,
        "current": current,
    })
    .to_string()
}
