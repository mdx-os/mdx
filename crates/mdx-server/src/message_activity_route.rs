// The activity substrate. GET /messages/activity/projection.json turns the
// receipt ledger into a live feed of what the stack is doing, routed into
// per-area channels (#forge, #deploys, #strategy, #product, #evals, #pages).
// Every Forge run, ship, deploy, decision, ratification, and eval already
// emits a receipt; this projection reads them and renders each as a human
// one-liner with its provenance, in ledger order. Human messages posted into
// an area channel ride the same feed (so people can talk about what the
// agents are doing, in context), ordered with the activity by ledger
// position. Nothing here writes - it is a read over receipts already on the
// record. Integration: mod + dispatch in main.rs, one GET HttpRouteDeclaration
// in mdx-core http_routes.rs (+1 count), regenerate route/openapi manifests.
use crate::RouteResponse;
use mdx_core::{MdxKernel, json_string_literal};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    let query = activity_query(path)?;
    Some(handle(method, query, kernel))
}

// The per-area channels and the label each carries. Order here is the order
// the areas surface in.
const AREAS: &[(&str, &str)] = &[
    ("forge", "Forge"),
    ("deploys", "Deploys"),
    ("strategy", "Strategy"),
    ("product", "Product"),
    ("evals", "Evals"),
    ("pages", "Pages"),
];

// Which area a receipt kind belongs to, and the verb phrase that reads as a
// human one-liner. Returns None for kinds that are not stack activity.
fn route_kind(kind: &str) -> Option<(&'static str, &'static str)> {
    let entry = match kind {
        "forge.outcome.signal.recorded" => ("forge", "recorded a build outcome"),
        "forge.run.event" => ("forge", "moved a build forward"),
        "forge.run.ship.decided" => ("forge", "decided on a ship"),
        "forge.review.panel.recorded" => ("forge", "ran a review panel"),
        "forge.run.control" => ("forge", "steered a run"),
        "forge.repo.connected" => ("forge", "connected a repo"),
        "fleet.plan.drafted" => ("forge", "drafted a fleet plan"),
        "fleet.plan.ratified" => ("forge", "ratified a fleet plan"),
        "fleet.run.event" => ("forge", "ran a fleet stream"),
        "forge.production.deployed" => ("deploys", "shipped to production"),
        "render.deployment.observed" => ("deploys", "observed a deployment"),
        "forge.ship.ratified" => ("deploys", "ratified a ship"),
        "forge.ship.escalated" => ("deploys", "escalated a ship for a call"),
        "forge.ship.rejected" => ("deploys", "held a ship back"),
        "forge.ship.revision_requested" => ("deploys", "asked for a revision"),
        "strategy.condition.stated" => ("strategy", "stated a condition"),
        "strategy.condition.updated" => ("strategy", "updated a condition"),
        "strategy.direction.recorded" => ("strategy", "set the direction"),
        "strategy.proposal.resolved" => ("strategy", "resolved a proposal"),
        "strategy.ratification.recorded" => ("strategy", "ratified a direction"),
        "product.bet.shaped" => ("product", "shaped a bet"),
        "product.bet.resolved" => ("product", "resolved a bet"),
        "product.condition.stated" => ("product", "stated a condition"),
        "product.condition.updated" => ("product", "updated a condition"),
        "product.ratification.recorded" => ("product", "ratified a product call"),
        "work.item.moved" => ("product", "moved a work item"),
        "triage.entry.recorded" => ("product", "took in something to triage"),
        "triage.verdict.recorded" => ("product", "called a triage verdict"),
        "eval.suite.ran" => ("evals", "ran the evals"),
        "harness.final.verdict.recorded" => ("evals", "recorded a final verdict"),
        "harness.quality.gate.recorded" => ("evals", "recorded a quality gate"),
        "harness.plan.hash.approved" => ("evals", "approved a plan hash"),
        "pages.approval.decision.recorded" => ("pages", "decided a page approval"),
        "changelog.entry.recorded" => ("pages", "logged a change"),
        _ => return None,
    };
    Some(entry)
}

fn handle(
    method: &str,
    query: Option<&str>,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "GET") {
        return Ok(response);
    }
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;

    let area_channels: std::collections::HashSet<&str> =
        AREAS.iter().map(|(channel, _)| *channel).collect();

    let page = ActivityPage::from_query(query);
    let mut items: Vec<String> = Vec::new();
    let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    let mut total_count = 0usize;
    let mut next_cursor = page.cursor.unwrap_or(0);
    let mut has_more = false;
    for (index, receipt) in kernel.ledger().entries().iter().enumerate() {
        let actor = receipt.actor_id.as_str();
        let actor_type = actor_kind(actor);
        let actor_name = display_name(actor);
        if let Some((area, verb)) = route_kind(&receipt.kind) {
            *counts.entry(area).or_insert(0) += 1;
            total_count += 1;
            if page.includes(index) {
                if items.len() < page.limit {
                    let run_id = activity_run_id(receipt);
                    items.push(format!(
                        r#"{{"feed_kind":"activity","area":{},"channel_id":{},"receipt_kind":{},"actor":{},"actor_type":{},"headline":{},"detail":{},"receipt_id":{},"ledger_seq":{},"run_id":{},"href":{}}}"#,
                        json_string_literal(area),
                        json_string_literal(area),
                        json_string_literal(&receipt.kind),
                        json_string_literal(&actor_name),
                        json_string_literal(actor_type),
                        json_string_literal(&format!("{actor_name} {verb}")),
                        json_string_literal(&detail_for(receipt)),
                        json_string_literal(&receipt.receipt_id),
                        index,
                        json_string_literal(&run_id),
                        json_string_literal(&activity_href(area, &run_id)),
                    ));
                    next_cursor = index;
                } else {
                    has_more = true;
                }
            }
            continue;
        }
        // A real Message event posted into an area channel joins that area's
        // feed. This remains compatibility scaffolding until bridges narrate
        // through the normal thread projection directly.
        if mdx_core::is_message_thread_message_receipt_kind(&receipt.kind) {
            let channel = pv(receipt, "channel_id");
            if area_channels.contains(channel) {
                *counts.entry(area_static(channel)).or_insert(0) += 1;
                total_count += 1;
                if page.includes(index) {
                    if items.len() < page.limit {
                        items.push(format!(
                            r#"{{"feed_kind":"message","area":{},"channel_id":{},"receipt_kind":{},"actor":{},"actor_type":{},"headline":{},"detail":{},"receipt_id":{},"ledger_seq":{},"run_id":"","href":""}}"#,
                            json_string_literal(channel),
                            json_string_literal(channel),
                            json_string_literal(&receipt.kind),
                            json_string_literal(&display_name(actor)),
                            json_string_literal(pv_or(receipt, "actor_type", actor_type)),
                            json_string_literal(&display_name(actor)),
                            json_string_literal(pv(receipt, "body")),
                            json_string_literal(&receipt.receipt_id),
                            index,
                        ));
                        next_cursor = index;
                    } else {
                        has_more = true;
                    }
                }
            }
        }
    }

    let areas: Vec<String> = AREAS
        .iter()
        .map(|(channel, label)| {
            format!(
                r#"{{"channel_id":{},"label":{},"activity":true,"item_count":{}}}"#,
                json_string_literal(channel),
                json_string_literal(label),
                counts.get(*channel).copied().unwrap_or(0),
            )
        })
        .collect();

    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-message-activity-local-projection","status":"OK","derivation":"the receipt ledger rendered as a live feed of stack activity, routed into per-area channels; human messages in an area channel ride the same feed, ordered by ledger position - every card traces to a receipt, nothing is invented","areas":[{}],"item_count":{},"total_item_count":{},"limit":{},"cursor":{},"next_cursor":{},"has_more":{},"items":[{}],"production_write_allowed":false}}"#,
            areas.join(","),
            items.len(),
            total_count,
            page.limit,
            page.cursor.unwrap_or(0),
            next_cursor,
            has_more,
            items.join(","),
        ),
    ))
}

fn activity_query(path: &str) -> Option<Option<&str>> {
    const ROUTE: &str = "/messages/activity/projection.json";
    if path == ROUTE {
        return Some(None);
    }
    path.strip_prefix(ROUTE)
        .and_then(|suffix| suffix.strip_prefix('?'))
        .map(Some)
}

struct ActivityPage {
    limit: usize,
    cursor: Option<usize>,
}

impl ActivityPage {
    fn from_query(query: Option<&str>) -> Self {
        let mut limit = 500usize;
        let mut cursor = None;
        if let Some(query) = query {
            for pair in query.split('&') {
                let mut parts = pair.splitn(2, '=');
                let key = parts.next().unwrap_or("");
                let value = parts.next().unwrap_or("");
                match key {
                    "limit" => {
                        limit = value.parse::<usize>().unwrap_or(limit).clamp(1, 500);
                    }
                    "cursor" => {
                        cursor = value.parse::<usize>().ok();
                    }
                    _ => {}
                }
            }
        }
        Self { limit, cursor }
    }

    fn includes(&self, ledger_seq: usize) -> bool {
        self.cursor.is_none_or(|cursor| ledger_seq > cursor)
    }
}

fn activity_run_id(receipt: &mdx_core::Receipt) -> String {
    for key in ["run_id", "forge_run_id", "related_run_id"] {
        let value = pv(receipt, key);
        if !value.trim().is_empty() {
            return value.to_string();
        }
    }
    String::new()
}

fn activity_href(area: &str, run_id: &str) -> String {
    if run_id.trim().is_empty() {
        return String::new();
    }
    match area {
        "forge" => format!("/forge/runs?run_id={run_id}"),
        _ => String::new(),
    }
}

fn area_static(channel: &str) -> &'static str {
    AREAS
        .iter()
        .find(|(area, _)| *area == channel)
        .map(|(area, _)| *area)
        .unwrap_or("forge")
}

// A short, human detail line from whatever the receipt carries - the most
// telling field, capped so the card stays calm.
fn detail_for(receipt: &mdx_core::Receipt) -> String {
    for key in [
        "title",
        "summary",
        "name",
        "objective",
        "headline",
        "spec",
        "transition",
    ] {
        let value = pv(receipt, key);
        if !value.is_empty() {
            return cap(value, 140);
        }
    }
    if !receipt.loop_id.as_str().is_empty() {
        return format!("On the record in {}.", receipt.loop_id.as_str());
    }
    "On the record.".to_string()
}

fn cap(value: &str, max: usize) -> String {
    if value.chars().count() > max {
        format!("{}…", value.chars().take(max - 1).collect::<String>())
    } else {
        value.to_string()
    }
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

fn pv<'a>(receipt: &'a mdx_core::Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}

fn pv_or<'a>(receipt: &'a mdx_core::Receipt, key: &str, fallback: &'a str) -> &'a str {
    let value = pv(receipt, key);
    if value.is_empty() { fallback } else { value }
}
