// Presence, honestly. GET /messages/presence/projection.json derives, from the
// receipt ledger, a roster of who is active in the stack right now - people and
// agents alike - by how recently they have acted, with their last action and a
// running count. It does NOT fake "online now": there is no websocket presence
// broadcast here (that is the gated production path), and receipts carry no
// wall-clock yet, so recency is ledger order, not minutes. The most recent
// actors are marked active; the rest are simply "around". Each actor now also
// carries last_active_at, the trusted receipt timestamp of their most recent
// action, so "last seen" reads a real signed time, not a placeholder. Live
// typing (the relay's job) stays the real-time signal on top of this roster.
// Integration: mod + dispatch in main.rs, one GET HttpRouteDeclaration in
// mdx-core http_routes.rs (+1 count), regenerate route/openapi manifests.
use crate::RouteResponse;
use mdx_core::{MdxKernel, json_string_literal};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path != "/messages/presence/projection.json" {
        return None;
    }
    Some(handle(method, kernel))
}

// How many of the most-recent actors read as "active now"; the rest are
// "around". A small number keeps the live signal meaningful.
const ACTIVE_RECENT: usize = 5;
// The roster is bounded - the company can be large; this is the recent face of
// it, not a directory.
const ROSTER_LIMIT: usize = 16;

struct ActorPresence {
    actor_id: String,
    actor_type: &'static str,
    display_name: String,
    last_action: String,
    last_active_at: String,
    last_seq: usize,
    action_count: usize,
}

fn handle(method: &str, kernel: &Arc<RwLock<MdxKernel>>) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "GET") {
        return Ok(response);
    }
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;

    // Fold each actor's most recent action and how often they have acted.
    let mut order: Vec<String> = Vec::new();
    let mut by_actor: std::collections::HashMap<String, ActorPresence> =
        std::collections::HashMap::new();
    for (index, receipt) in kernel.ledger().entries().iter().enumerate() {
        let actor_id = receipt.actor_id.as_str();
        if actor_id.is_empty() {
            continue;
        }
        if receipt.kind.starts_with("memory.")
            || receipt.kind == mdx_core::POLICY_DECISION_RECEIPT_KIND
        {
            continue;
        }
        let entry = by_actor.entry(actor_id.to_string()).or_insert_with(|| {
            order.push(actor_id.to_string());
            ActorPresence {
                actor_id: actor_id.to_string(),
                actor_type: actor_kind(actor_id),
                display_name: display_name(actor_id),
                last_action: humanize_kind(&receipt.kind),
                last_active_at: receipt.receipt_timestamp.clone(),
                last_seq: index,
                action_count: 0,
            }
        });
        entry.action_count += 1;
        if index >= entry.last_seq {
            entry.last_seq = index;
            entry.last_action = humanize_kind(&receipt.kind);
            entry.last_active_at = receipt.receipt_timestamp.clone();
        }
    }

    // Most recent first; the top few are "active now".
    let mut roster: Vec<&ActorPresence> = by_actor.values().collect();
    roster.sort_by_key(|presence| std::cmp::Reverse(presence.last_seq));
    let active_total = roster.len().min(ACTIVE_RECENT);
    let people = roster
        .iter()
        .filter(|presence| presence.actor_type == "human")
        .count();
    let agents = roster
        .iter()
        .filter(|presence| presence.actor_type == "agent")
        .count();

    let entries: Vec<String> = roster
        .iter()
        .take(ROSTER_LIMIT)
        .enumerate()
        .map(|(rank, presence)| {
            format!(
                r#"{{"actor_id":{},"actor_type":{},"display_name":{},"last_action":{},"last_active_at":{},"action_count":{},"active":{}}}"#,
                json_string_literal(&presence.actor_id),
                json_string_literal(presence.actor_type),
                json_string_literal(&presence.display_name),
                json_string_literal(&presence.last_action),
                json_string_literal(&presence.last_active_at),
                presence.action_count,
                rank < ACTIVE_RECENT,
            )
        })
        .collect();

    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-message-presence-local-projection","status":"OK","derivation":"who has acted, most recent first, derived from the receipt ledger; the top {} read as active now, the rest are around. each actor carries last_active_at from the trusted receipt timestamp of their most recent action; real websocket presence broadcast stays the gated production path","active_now_count":{},"people_count":{},"agent_count":{},"roster_count":{},"roster":[{}],"production_write_allowed":false}}"#,
            ACTIVE_RECENT,
            active_total,
            people,
            agents,
            roster.len(),
            entries.join(","),
        ),
    ))
}

fn actor_kind(actor_id: &str) -> &'static str {
    if actor_id.starts_with("human:") {
        "human"
    } else if actor_id.starts_with("agent:") || actor_id.contains("_agent") {
        "agent"
    } else {
        "system"
    }
}

fn display_name(actor_id: &str) -> String {
    let trimmed = actor_id
        .trim_start_matches("agent:")
        .trim_start_matches("human:")
        .trim_start_matches("system:")
        .replace(['_', ':'], " ");
    trimmed
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// A receipt kind as a short human phrase, so a roster line reads "last: shipped
// a build" rather than a dotted kind. Falls back to a tidied form.
fn humanize_kind(kind: &str) -> String {
    match kind {
        "message.human.posted" => "said something".to_string(),
        "forge.run.event" => "moved a build".to_string(),
        "forge.run.ship.decided" => "decided a ship".to_string(),
        "forge.production.deployed" => "shipped to production".to_string(),
        "forge.review.panel.recorded" => "ran a review".to_string(),
        "fleet.run.event" => "ran a fleet stream".to_string(),
        "strategy.ratification.recorded" => "ratified a direction".to_string(),
        "product.ratification.recorded" => "ratified a call".to_string(),
        "eval.suite.ran" => "ran the evals".to_string(),
        "message.channel.created" => "made a channel".to_string(),
        other => other.replace(['.', '_'], " "),
    }
}
