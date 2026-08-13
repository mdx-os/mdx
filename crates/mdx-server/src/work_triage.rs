// The triage stream over HTTP: entries recorded, verdicts called, and
// one projection that reads the whole inbox - manual entries, flagged
// messages, AND the feedback rail's captures, whose note text already
// passed the safety boundary at capture time. The watchtower stays
// metadata-only; this is the surface where reading belongs. Entries
// fold with their latest verdict; no verdict means the entry is open.
use crate::RouteResponse;
use mdx_core::{
    MdxKernel, TRIAGE_ACCEPT_TARGETS, TRIAGE_VERDICTS, TriageEntry, TriageVerdict,
    json_string_literal,
};
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/product/triage-entries.json" => Some(handle_entry_post(method, body, kernel)),
        "/product/triage-verdicts.json" => Some(handle_verdict_post(method, body, kernel)),
        "/product/triage/projection.json" => Some(handle_projection(method, kernel)),
        _ => None,
    }
}

fn refusal(name: &str, reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":{},"status":"REFUSED","reason":{},"production_write_allowed":false}}"#,
            json_string_literal(name),
            json_string_literal(reason)
        ),
    )
}

fn handle_entry_post(
    method: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "POST") {
        return Ok(response);
    }
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    let field = |name: &str| json_string_field(body, name).unwrap_or_default();
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let report = match kernel.save_triage_entry_local_with_identity(
        TriageEntry {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            text: &field("text"),
            source: &field("source"),
            source_ref: &field("source_ref"),
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => {
            return Ok(refusal("mdx-triage-entry-local-post", &error.message()));
        }
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-triage-entry-local-post","status":"RECORDED","auth_session_status":{},"entry_id":{},"entry_receipt_id":{},"policy_decision_id":{},"terminal_state":{},"verdict_route":"/product/triage-verdicts.json","projection_route":"/product/triage/projection.json","authority_opened":"none","production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&report.record_id),
            json_string_literal(&report.receipt_id),
            json_string_literal(&report.policy_decision_id),
            json_string_literal(report.terminal_state),
        ),
    ))
}

fn handle_verdict_post(
    method: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "POST") {
        return Ok(response);
    }
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    let field = |name: &str| json_string_field(body, name).unwrap_or_default();
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let entry_ref = field("entry_ref");
    let verdict = field("verdict");
    let report = match kernel.save_triage_verdict_local_with_identity(
        TriageVerdict {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            entry_ref: &entry_ref,
            verdict: &verdict,
            note: &field("note"),
            accept_as: &field("accept_as"),
            accept_title: &field("accept_title"),
            bet_id: &field("bet_id"),
            duplicate_of: &field("duplicate_of"),
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => {
            return Ok(refusal("mdx-triage-verdict-local-post", &error.message()));
        }
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-triage-verdict-local-post","status":"RECORDED","auth_session_status":{},"verdict_id":{},"entry_ref":{},"verdict":{},"accepted_record_id":{},"accepted_receipt_id":{},"verdict_receipt_id":{},"policy_decision_id":{},"terminal_state":{},"allowed_verdicts":[{}],"accept_targets":[{}],"authority_opened":"none","production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&report.record_id),
            json_string_literal(&entry_ref),
            json_string_literal(&verdict),
            json_string_literal(report.accepted_record_id.as_deref().unwrap_or("")),
            json_string_literal(report.accepted_receipt_id.as_deref().unwrap_or("")),
            json_string_literal(&report.receipt_id),
            json_string_literal(&report.policy_decision_id),
            json_string_literal(report.terminal_state),
            TRIAGE_VERDICTS
                .iter()
                .map(|s| json_string_literal(s))
                .collect::<Vec<_>>()
                .join(","),
            TRIAGE_ACCEPT_TARGETS
                .iter()
                .map(|s| json_string_literal(s))
                .collect::<Vec<_>>()
                .join(","),
        ),
    ))
}

fn handle_projection(
    method: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "GET") {
        return Ok(response);
    }
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let ledger = kernel.ledger();
    // Latest verdict per entry: a re-verdict (snoozed, then accepted)
    // overrides, and the full call history stays in the ledger.
    let mut verdicts: BTreeMap<String, (String, BTreeMap<String, String>, String)> =
        BTreeMap::new();
    for receipt in ledger.query().by_kind("triage.verdict.recorded").iter() {
        let entry_ref = receipt
            .payload
            .get("entry_ref")
            .cloned()
            .unwrap_or_default();
        if entry_ref.is_empty() {
            continue;
        }
        verdicts.insert(
            entry_ref,
            (
                receipt.receipt_id.clone(),
                receipt.payload.clone(),
                receipt.actor_id.as_str().to_string(),
            ),
        );
    }
    // One pass over the chain keeps the stream in arrival order across
    // both kinds: posted entries and feedback captures.
    let mut entries: Vec<String> = Vec::new();
    let mut open_count = 0usize;
    for receipt in ledger.entries() {
        let (entry_ref, text, source, source_ref) = match receipt.kind.as_str() {
            "triage.entry.recorded" => {
                let value = |key: &str| receipt.payload.get(key).map(String::as_str).unwrap_or("");
                (
                    value("entry_id").to_string(),
                    value("text").to_string(),
                    value("source").to_string(),
                    value("source_ref").to_string(),
                )
            }
            "beta.feedback.captured" => {
                let value = |key: &str| receipt.payload.get(key).map(String::as_str).unwrap_or("");
                (
                    receipt.receipt_id.clone(),
                    value("note").to_string(),
                    "feedback".to_string(),
                    format!("{} {}", value("surface"), value("route"))
                        .trim()
                        .to_string(),
                )
            }
            _ => continue,
        };
        if entry_ref.is_empty() {
            continue;
        }
        let verdict_json = match verdicts.get(&entry_ref) {
            Some((verdict_receipt_id, fields, actor)) => {
                let value = |key: &str| {
                    json_string_literal(fields.get(key).map(String::as_str).unwrap_or(""))
                };
                format!(
                    r#"{{"verdict":{},"note":{},"duplicate_of":{},"accepted_as":{},"accepted_record_id":{},"verdict_receipt_id":{},"actor_id":{}}}"#,
                    value("verdict"),
                    value("note"),
                    value("duplicate_of"),
                    value("accepted_as"),
                    value("accepted_record_id"),
                    json_string_literal(verdict_receipt_id),
                    json_string_literal(actor),
                )
            }
            None => {
                open_count += 1;
                "null".to_string()
            }
        };
        entries.push(format!(
            r#"{{"entry_ref":{},"text":{},"source":{},"source_ref":{},"entry_receipt_id":{},"actor_id":{},"verdict":{}}}"#,
            json_string_literal(&entry_ref),
            json_string_literal(&text),
            json_string_literal(&source),
            json_string_literal(&source_ref),
            json_string_literal(&receipt.receipt_id),
            json_string_literal(receipt.actor_id.as_str()),
            verdict_json,
        ));
    }
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-triage-local-projection","receipt_kind":"triage.entry.recorded","entry_count":{},"open_count":{},"entries":[{}],"allowed_verdicts":[{}],"accept_targets":[{}],"authority_opened":"none","production_write_allowed":false}}"#,
            entries.len(),
            open_count,
            entries.join(","),
            TRIAGE_VERDICTS
                .iter()
                .map(|s| json_string_literal(s))
                .collect::<Vec<_>>()
                .join(","),
            TRIAGE_ACCEPT_TARGETS
                .iter()
                .map(|s| json_string_literal(s))
                .collect::<Vec<_>>()
                .join(","),
        ),
    ))
}

fn json_string_field(body: &str, key: &str) -> Option<String> {
    let marker = format!("\"{key}\":");
    let after = body.split(&marker).nth(1)?;
    let after = after.trim_start();
    let rest = after.strip_prefix('"')?;
    let mut value = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(escaped) = chars.next() {
                    value.push(escaped);
                }
            }
            '"' => return Some(value),
            other => value.push(other),
        }
    }
    None
}
