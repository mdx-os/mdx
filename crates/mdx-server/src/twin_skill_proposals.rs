// The skill proposal ledger: every offer the model makes leaves a record,
// and so does what the person did with it - approved, declined, executed
// (with the governed receipt it produced), or failed. Same write-once
// evidence posture as capability changes. PRIVACY LINE, deliberate: the
// ledger never stores proposal CONTENT - only the skill, sizes, and ids.
// Approved content becomes company record through the governed message
// write (that receipt holds it, because the person said yes); declined
// content vanishes with the decline, which is exactly what declining means.
use crate::RouteResponse;
use std::path::Path;

pub(crate) const LEDGER_DIR: &str = ".mdx-local/twin-skill-proposals";

const EVENT_KINDS: &[&str] = &["proposed", "approved", "declined", "executed", "failed"];
// Which event wins as the proposal's current state when several exist.
const STATE_ORDER: &[&str] = &["failed", "executed", "declined", "approved", "proposed"];

pub(crate) struct ProposalEvent<'a> {
    pub event: &'a str,
    pub proposal_id: &'a str,
    pub skill: &'a str,
    pub session_id: &'a str,
    pub actor_id: &'a str,
    pub args_chars: usize,
    pub executed_receipt_id: &'a str,
}

// Mint the next proposal id - called server-side when the model proposes.
pub(crate) fn mint_proposal_id() -> String {
    mint_proposal_id_in(Path::new(LEDGER_DIR))
}

pub(crate) fn mint_proposal_id_in(dir: &Path) -> String {
    let next = list_records_in(dir)
        .iter()
        .filter_map(|record| record["proposal_id"].as_str())
        .filter_map(|id| id.rsplit('_').next())
        .filter_map(|tail| tail.parse::<usize>().ok())
        .max()
        .unwrap_or(0)
        + 1;
    format!("twin_skill_proposal_{next:06}")
}

pub(crate) fn record_event(event: &ProposalEvent<'_>) -> Option<String> {
    record_event_in(Path::new(LEDGER_DIR), event)
}

pub(crate) fn record_event_in(dir: &Path, event: &ProposalEvent<'_>) -> Option<String> {
    if !EVENT_KINDS.contains(&event.event) {
        return None;
    }
    let sequence = list_records_in(dir).len() + 1;
    let record_id = format!("twin_skill_proposal_event_{sequence:06}");
    let recorded_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let record = serde_json::json!({
        "record_id": record_id,
        "event": event.event,
        "proposal_id": event.proposal_id,
        "skill": event.skill,
        "session_id": event.session_id,
        "actor_id": event.actor_id,
        "args_chars": event.args_chars,
        "executed_receipt_id": event.executed_receipt_id,
        "recorded_at_epoch_seconds": recorded_at,
    });
    std::fs::create_dir_all(dir).ok()?;
    let body = serde_json::to_string_pretty(&record).ok()?;
    std::fs::write(dir.join(format!("{record_id}.json")), body).ok()?;
    Some(record_id)
}

pub(crate) fn list_records_in(dir: &Path) -> Vec<serde_json::Value> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.starts_with("twin_skill_proposal_event_") && name.ends_with(".json"))
        .collect();
    names.sort();
    names
        .iter()
        .filter_map(|name| std::fs::read_to_string(dir.join(name)).ok())
        .filter_map(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .filter(|record| record["proposal_id"].is_string() && record["event"].is_string())
        .collect()
}

pub(crate) fn route_response(method: &str, path: &str, body: &str) -> Option<RouteResponse> {
    if path == "/twin/skill-proposals/projection.json" {
        if !method.eq_ignore_ascii_case("GET") {
            return Some(method_not_allowed());
        }
        return Some(RouteResponse::json("200 OK", render_projection_json()));
    }
    if path != "/twin/skill-proposal-events.json" {
        return None;
    }
    if method.eq_ignore_ascii_case("GET") {
        return Some(RouteResponse::json("200 OK", render_projection_json()));
    }
    if !method.eq_ignore_ascii_case("POST") {
        return Some(method_not_allowed());
    }
    Some(apply_event(body))
}

// The client records the human side of a proposal's life: approved,
// declined, executed (with its receipt), failed. The model's own
// "proposed" is recorded server-side at proposal time and never accepted
// here. An empty body (the smoke probe) is a no-op.
fn apply_event(body: &str) -> RouteResponse {
    let incoming: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
    let event = incoming["event"].as_str().unwrap_or("");
    if event.is_empty() {
        return RouteResponse::json(
            "200 OK",
            r#"{"name":"mdx-twin-skill-proposal-events","status":"OK","event_recorded":false,"event_record_id":""}"#.to_string(),
        );
    }
    let allowed = ["approved", "declined", "executed", "failed"];
    let proposal_id = incoming["proposal_id"].as_str().unwrap_or("");
    let skill = incoming["skill"].as_str().unwrap_or("");
    if !allowed.contains(&event)
        || !proposal_id.starts_with("twin_skill_proposal_")
        || !crate::twin_live_gateway::known_skill(skill)
    {
        return RouteResponse::json(
            "200 OK",
            r#"{"name":"mdx-twin-skill-proposal-events","status":"REJECTED","reason":"unknown event, proposal id, or skill","event_recorded":false,"event_record_id":""}"#.to_string(),
        );
    }
    let recorded = record_event(&ProposalEvent {
        event,
        proposal_id,
        skill,
        session_id: incoming["session_id"].as_str().unwrap_or(""),
        actor_id: incoming["actor_id"].as_str().unwrap_or("local_user"),
        args_chars: incoming["args_chars"].as_u64().unwrap_or(0) as usize,
        executed_receipt_id: incoming["executed_receipt_id"].as_str().unwrap_or(""),
    });
    match recorded {
        Some(record_id) => RouteResponse::json(
            "200 OK",
            format!(
                r#"{{"name":"mdx-twin-skill-proposal-events","status":"OK","event_recorded":true,"event_record_id":"{record_id}"}}"#
            ),
        ),
        None => RouteResponse::json(
            "200 OK",
            r#"{"name":"mdx-twin-skill-proposal-events","status":"OK","event_recorded":false,"event_record_id":"","event_record_failed":true}"#.to_string(),
        ),
    }
}

fn method_not_allowed() -> RouteResponse {
    RouteResponse::text("405 Method Not Allowed", "method not allowed\n".to_string())
}

// The whole ledger grouped by proposal: every event in order, plus the
// proposal's current state (the strongest event wins - failed and executed
// outrank approved, which outranks the original offer).
fn render_projection_json() -> String {
    let records = list_records_in(Path::new(LEDGER_DIR));
    let mut order: Vec<&str> = Vec::new();
    for record in &records {
        if let Some(id) = record["proposal_id"].as_str()
            && !order.contains(&id)
        {
            order.push(id);
        }
    }
    let proposals: Vec<serde_json::Value> = order
        .iter()
        .rev()
        .map(|id| {
            let events: Vec<&serde_json::Value> = records
                .iter()
                .filter(|record| record["proposal_id"].as_str() == Some(id))
                .collect();
            let state = STATE_ORDER
                .iter()
                .find(|kind| {
                    events
                        .iter()
                        .any(|event| event["event"].as_str() == Some(kind))
                })
                .copied()
                .unwrap_or("proposed");
            serde_json::json!({
                "proposal_id": id,
                "skill": events.first().and_then(|event| event["skill"].as_str()).unwrap_or(""),
                "state": state,
                "events": events,
            })
        })
        .collect();
    serde_json::json!({
        "name": "mdx-twin-skill-proposals",
        "status": "OK",
        "ledger_dir": LEDGER_DIR,
        "proposal_count": proposals.len(),
        "proposals": proposals,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mdx-twin-skill-proposals-{}-{}",
            name,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn unknown_event_kind_records_nothing() {
        let dir = scratch_dir("unknown");
        let event = ProposalEvent {
            event: "sneaky",
            proposal_id: "twin_skill_proposal_000001",
            skill: "record_decision",
            session_id: "sess_x",
            actor_id: "local_user",
            args_chars: 10,
            executed_receipt_id: "",
        };
        assert!(record_event_in(&dir, &event).is_none());
        assert!(!dir.exists());
    }

    #[test]
    fn proposal_life_records_in_order_and_without_content() {
        let dir = scratch_dir("life");
        let proposal_id = mint_proposal_id_in(&dir);
        assert_eq!(proposal_id, "twin_skill_proposal_000001");
        for (event, receipt) in [
            ("proposed", ""),
            ("approved", ""),
            ("executed", "message_thread_message_receipt_000009"),
        ] {
            record_event_in(
                &dir,
                &ProposalEvent {
                    event,
                    proposal_id: &proposal_id,
                    skill: "record_decision",
                    session_id: "sess_x",
                    actor_id: "you_v1_engineering_direct",
                    args_chars: 142,
                    executed_receipt_id: receipt,
                },
            )
            .expect("event lands");
        }
        let records = list_records_in(&dir);
        assert_eq!(records.len(), 3);
        assert_eq!(records[0]["event"], "proposed");
        assert_eq!(
            records[2]["executed_receipt_id"],
            "message_thread_message_receipt_000009"
        );
        // The ledger knows sizes and ids, never text.
        for record in &records {
            let raw = serde_json::to_string(record).expect("serialize");
            assert!(!raw.contains("decision text"));
            assert!(record["args_chars"].as_u64().unwrap_or(0) > 0);
            assert!(record.get("args").is_none());
            assert!(record.get("body").is_none());
        }
        // A second proposal mints the next id even after events landed.
        assert_eq!(mint_proposal_id_in(&dir), "twin_skill_proposal_000002");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
