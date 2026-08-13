use crate::RouteResponse;
use mdx_core::{
    GovernedWriteIdentity, MESSAGE_ACTION_VERDICTS, MdxKernel, MessageActionRequest,
    MessageActionVerdict, MessageThreadMessage, PagesApprovalDecision, PagesApprovalRequest,
    PagesEditDraft, json_string_literal, message_receipt_kind_for_actor_id,
};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path == "/messages/action-requests.json" {
        if !method.eq_ignore_ascii_case("POST") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let mut kernel = match kernel.write() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(
            render_action_request_post_json(body, &mut kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/messages/action-requests/projection.json" {
        if !method.eq_ignore_ascii_case("GET") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let kernel = match kernel.read() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(
            render_action_request_projection_json(&kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/messages/action-verdicts.json" {
        if !method.eq_ignore_ascii_case("POST") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let mut kernel = match kernel.write() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(
            render_action_verdict_post_json(body, &mut kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/messages/action-verdicts/projection.json" {
        if !method.eq_ignore_ascii_case("GET") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let kernel = match kernel.read() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(
            render_action_verdict_projection_json(&kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    None
}

fn render_action_request_post_json(body: &str, kernel: &mut MdxKernel) -> Result<String, String> {
    let action_request_id = json_string_field(body, "action_request_id")
        .unwrap_or_else(|| "message_action_request_local_001".to_string());
    let thread_id = json_string_field(body, "thread_id").unwrap_or_else(|| "local".to_string());
    let channel_id = json_string_field(body, "channel_id").unwrap_or_else(|| "forge".to_string());
    let actor_id =
        json_string_field(body, "actor_id").unwrap_or_else(|| "agent:assistant".to_string());
    let title = json_string_field(body, "title")
        .unwrap_or_else(|| "Review the proposed action".to_string());
    let summary = json_string_field(body, "summary").unwrap_or_else(|| {
        "An agent is asking for a governed human verdict before anything can run.".to_string()
    });
    let proposed_action = json_string_field(body, "proposed_action").unwrap_or_else(|| {
        "Review the plan and choose approve, edit, reject, or respond.".to_string()
    });
    let action_payload = json_string_field(body, "action_payload").unwrap_or_else(|| {
        r#"{"kind":"local_message_action","execution_allowed":false}"#.to_string()
    });
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    let source_receipt_id = match json_string_field(body, "source_receipt_id") {
        Some(id) if !id.trim().is_empty() => id,
        _ => seed_action_source_receipt(
            kernel,
            &resolved.tenant_id,
            &resolved.actor_id,
            &resolved.actor_role,
        )?,
    };
    let identity = GovernedWriteIdentity {
        identity_source: "local_message_action_gate".to_string(),
        actor_kind: actor_id.split(':').next().unwrap_or("agent").to_string(),
        subject_actor_id: actor_id.clone(),
        delegation_id: json_string_field(body, "delegation_id").unwrap_or_default(),
        authority_scope: vec!["message:action:request".to_string()],
    };
    let report = kernel
        .save_message_action_request_local_with_identity(
            MessageActionRequest {
                tenant_id: &resolved.tenant_id,
                actor_id: &actor_id,
                thread_id: &thread_id,
                channel_id: &channel_id,
                action_request_id: &action_request_id,
                source_receipt_id: &source_receipt_id,
                title: &title,
                summary: &summary,
                proposed_action: &proposed_action,
                action_payload: &action_payload,
            },
            &identity,
        )
        .map_err(|error| error.message())?;
    let message_id = format!("{action_request_id}_message");
    let message_body = json_string_field(body, "body").unwrap_or_else(|| proposed_action.clone());
    let content_payload = action_request_payload(&report);
    let message_report = kernel
        .save_message_thread_message_local_with_identity(
            MessageThreadMessage {
                tenant_id: &resolved.tenant_id,
                actor_id: &actor_id,
                thread_id: &thread_id,
                channel_id: &channel_id,
                message_id: &message_id,
                content_type: "action_request",
                body: &message_body,
                content_payload: &content_payload,
                source_receipt_id: &report.action_request_receipt_id,
                outbox_topic: message_receipt_kind_for_actor_id(&actor_id),
            },
            &identity,
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-message-action-request-local-post","status":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","auth_session_tenant_id":{},"auth_session_user_id":{},"auth_session_role":{},"writes_route":"/messages/action-requests.json","projection_route":"/messages/action-requests/projection.json","verdicts_route":"/messages/action-verdicts.json","action_request_id":{},"action_request_receipt_id":{},"message_receipt_id":{},"policy_decision_id":{},"thread_id":{},"channel_id":{},"source_receipt_id":{},"source_receipt_kind":{},"title":{},"summary":{},"proposed_action":{},"allowed_verdicts":{},"gate_state":{},"execution_allowed":{},"live_substrate_required":false,"production_write_allowed":{}}}"#,
        json_string_literal(report.status),
        json_string_literal(resolved.auth_session_status),
        json_string_literal(&resolved.tenant_id),
        json_string_literal(&resolved.actor_id),
        json_string_literal(&resolved.actor_role),
        json_string_literal(&report.action_request_id),
        json_string_literal(&report.action_request_receipt_id),
        json_string_literal(&message_report.message_receipt_id),
        json_string_literal(&report.policy_decision_id),
        json_string_literal(&report.thread_id),
        json_string_literal(&report.channel_id),
        json_string_literal(&report.source_receipt_id),
        json_string_literal(&report.source_receipt_kind),
        json_string_literal(&report.title),
        json_string_literal(&report.summary),
        json_string_literal(&report.proposed_action),
        json_string_literal(&report.allowed_verdicts),
        json_string_literal(report.gate_state),
        report.execution_allowed,
        report.production_write_allowed
    ))
}

fn render_action_verdict_post_json(body: &str, kernel: &mut MdxKernel) -> Result<String, String> {
    let action_verdict_id = json_string_field(body, "action_verdict_id")
        .unwrap_or_else(|| "message_action_verdict_local_001".to_string());
    let verdict = json_string_field(body, "verdict").unwrap_or_else(|| "respond".to_string());
    let decision_note = json_string_field(body, "decision_note").unwrap_or_else(|| {
        "Human verdict recorded. Execution remains blocked until the action gate lands.".to_string()
    });
    let edited_action_payload =
        json_string_field(body, "edited_action_payload").unwrap_or_default();
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    let action_request_receipt_id = match json_string_field(body, "action_request_receipt_id") {
        Some(id) if !id.trim().is_empty() => id,
        _ => latest_or_seed_action_request_receipt_id(kernel, &resolved.tenant_id)?,
    };
    let report = kernel
        .save_message_action_verdict_local_with_identity(
            MessageActionVerdict {
                tenant_id: &resolved.tenant_id,
                actor_id: &resolved.actor_id,
                actor_role: &resolved.actor_role,
                action_verdict_id: &action_verdict_id,
                action_request_receipt_id: &action_request_receipt_id,
                verdict: &verdict,
                decision_note: &decision_note,
                edited_action_payload: &edited_action_payload,
            },
            &resolved.identity,
        )
        .map_err(|error| error.message())?;
    let gate = apply_deterministic_action_gate(
        kernel,
        &report,
        &resolved.tenant_id,
        &resolved.actor_id,
        &resolved.actor_role,
        &resolved.identity,
    )?;
    let message_id = format!("{action_verdict_id}_message");
    let body = format!(
        "{} recorded. {}{}",
        human_token(&report.verdict),
        report.decision_note,
        gate.message_suffix()
    );
    let content_payload = action_verdict_payload(&report, &gate);
    let message_report = kernel
        .save_message_thread_message_local_with_identity(
            MessageThreadMessage {
                tenant_id: &resolved.tenant_id,
                actor_id: &resolved.actor_id,
                thread_id: &report.thread_id,
                channel_id: &report.channel_id,
                message_id: &message_id,
                content_type: "action_response",
                body: &body,
                content_payload: &content_payload,
                source_receipt_id: &report.action_verdict_receipt_id,
                outbox_topic: message_receipt_kind_for_actor_id(&resolved.actor_id),
            },
            &resolved.identity,
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-message-action-verdict-local-post","status":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","auth_session_tenant_id":{},"auth_session_user_id":{},"auth_session_role":{},"writes_route":"/messages/action-verdicts.json","projection_route":"/messages/action-verdicts/projection.json","action_verdict_id":{},"action_verdict_receipt_id":{},"message_receipt_id":{},"policy_decision_id":{},"action_request_receipt_id":{},"action_request_id":{},"thread_id":{},"channel_id":{},"title":{},"verdict":{},"decision_note":{},"gate_state":{},"applied_receipt_kind":{},"applied_receipt_id":{},"human_decision_recorded":{},"human_approval_granted":{},"execution_allowed":{},"live_substrate_required":false,"production_write_allowed":{}}}"#,
        json_string_literal(report.status),
        json_string_literal(resolved.auth_session_status),
        json_string_literal(&resolved.tenant_id),
        json_string_literal(&resolved.actor_id),
        json_string_literal(&resolved.actor_role),
        json_string_literal(&report.action_verdict_id),
        json_string_literal(&report.action_verdict_receipt_id),
        json_string_literal(&message_report.message_receipt_id),
        json_string_literal(&report.policy_decision_id),
        json_string_literal(&report.action_request_receipt_id),
        json_string_literal(&report.action_request_id),
        json_string_literal(&report.thread_id),
        json_string_literal(&report.channel_id),
        json_string_literal(&report.title),
        json_string_literal(&report.verdict),
        json_string_literal(&report.decision_note),
        json_string_literal(gate.state),
        json_string_literal(&gate.applied_receipt_kind),
        json_string_literal(&gate.applied_receipt_id),
        report.human_decision_recorded,
        report.human_approval_granted,
        report.execution_allowed,
        report.production_write_allowed
    ))
}

fn render_action_request_projection_json(kernel: &MdxKernel) -> Result<String, String> {
    let requests = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == "message.action.requested")
        .map(|receipt| {
            format!(
                r#"{{"action_request_id":{},"action_request_receipt_id":{},"policy_decision_id":{},"thread_id":{},"channel_id":{},"source_receipt_id":{},"source_receipt_kind":{},"title":{},"summary":{},"proposed_action":{},"allowed_verdicts":{},"gate_state":{},"execution_allowed":false,"production_write_allowed":false}}"#,
                json_string_literal(payload_value(receipt, "action_request_id")),
                json_string_literal(&receipt.receipt_id),
                json_string_literal(receipt.policy_decision_id.as_deref().unwrap_or("")),
                json_string_literal(payload_value(receipt, "thread_id")),
                json_string_literal(payload_value(receipt, "channel_id")),
                json_string_literal(payload_value(receipt, "source_receipt_id")),
                json_string_literal(payload_value(receipt, "source_receipt_kind")),
                json_string_literal(payload_value(receipt, "title")),
                json_string_literal(payload_value(receipt, "summary")),
                json_string_literal(payload_value(receipt, "proposed_action")),
                json_string_literal(payload_value(receipt, "allowed_verdicts")),
                json_string_literal(payload_value(receipt, "gate_state"))
            )
        })
        .collect::<Vec<_>>();
    Ok(format!(
        r#"{{"name":"mdx-message-action-request-local-projection","status":"OK","auth_session_route":"/local/auth-session.json","writes_route":"/messages/action-requests.json","verdicts_route":"/messages/action-verdicts.json","action_request_count":{},"allowed_verdicts":{},"execution_allowed":false,"live_substrate_required":false,"production_write_allowed":false,"requests":[{}]}}"#,
        requests.len(),
        json_string_literal(&MESSAGE_ACTION_VERDICTS.join(",")),
        requests.join(",")
    ))
}

fn render_action_verdict_projection_json(kernel: &MdxKernel) -> Result<String, String> {
    let verdicts = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == "message.action.verdict.recorded")
        .map(|receipt| {
            let gate = gate_outcome_for_verdict_projection(kernel, receipt);
            format!(
                r#"{{"action_verdict_id":{},"action_verdict_receipt_id":{},"policy_decision_id":{},"action_request_receipt_id":{},"action_request_id":{},"thread_id":{},"channel_id":{},"title":{},"verdict":{},"decision_note":{},"gate_state":{},"applied_receipt_kind":{},"applied_receipt_id":{},"human_decision_recorded":true,"human_approval_granted":{},"execution_allowed":false,"production_write_allowed":false}}"#,
                json_string_literal(payload_value(receipt, "action_verdict_id")),
                json_string_literal(&receipt.receipt_id),
                json_string_literal(receipt.policy_decision_id.as_deref().unwrap_or("")),
                json_string_literal(payload_value(receipt, "action_request_receipt_id")),
                json_string_literal(payload_value(receipt, "action_request_id")),
                json_string_literal(payload_value(receipt, "thread_id")),
                json_string_literal(payload_value(receipt, "channel_id")),
                json_string_literal(payload_value(receipt, "title")),
                json_string_literal(payload_value(receipt, "verdict")),
                json_string_literal(payload_value(receipt, "decision_note")),
                json_string_literal(gate.state),
                json_string_literal(&gate.applied_receipt_kind),
                json_string_literal(&gate.applied_receipt_id),
                payload_value(receipt, "human_approval_granted")
            )
        })
        .collect::<Vec<_>>();
    Ok(format!(
        r#"{{"name":"mdx-message-action-verdict-local-projection","status":"OK","auth_session_route":"/local/auth-session.json","writes_route":"/messages/action-verdicts.json","action_verdict_count":{},"execution_allowed":false,"live_substrate_required":false,"production_write_allowed":false,"verdicts":[{}]}}"#,
        verdicts.len(),
        verdicts.join(",")
    ))
}

fn gate_outcome_for_verdict_projection(
    kernel: &MdxKernel,
    verdict_receipt: &mdx_core::Receipt,
) -> GateOutcome {
    let action_request_receipt_id = payload_value(verdict_receipt, "action_request_receipt_id");
    let Some(request_receipt) = kernel.ledger().query().by_id(action_request_receipt_id) else {
        return GateOutcome::held("ACTION_REQUEST_MISSING");
    };
    let action_payload = payload_value(request_receipt, "action_payload");
    let kind = json_string_field(action_payload, "kind").unwrap_or_default();
    if kind == "pages_approval_decision" {
        let verdict = payload_value(verdict_receipt, "verdict");
        if verdict != "approve" && verdict != "reject" {
            return GateOutcome::held("VERDICT_RECORDED_NO_ACTION_APPLIED");
        }
        let approval_decision_id = pages_approval_decision_id(
            action_payload,
            payload_value(verdict_receipt, "action_verdict_id"),
        );
        let Some(decision) = kernel.ledger().entries().iter().find(|receipt| {
            receipt.kind == "pages.approval.decision.recorded"
                && payload_value(receipt, "approval_decision_id") == approval_decision_id
        }) else {
            return GateOutcome::held("GOVERNED_ACTION_APPLIED_RECEIPT_MISSING");
        };
        return GateOutcome::applied("pages.approval.decision.recorded", &decision.receipt_id);
    }
    if payload_value(verdict_receipt, "verdict") != "approve" {
        return GateOutcome::held("VERDICT_RECORDED_NO_ACTION_APPLIED");
    }
    if kind == "pages_draft_approval_request" {
        let approval_request_id = pages_approval_request_id(
            action_payload,
            payload_value(verdict_receipt, "action_verdict_id"),
        );
        let Some(approval_request) = kernel.ledger().entries().iter().find(|receipt| {
            receipt.kind == "pages.approval.requested"
                && payload_value(receipt, "approval_request_id") == approval_request_id
        }) else {
            return GateOutcome::held("GOVERNED_ACTION_APPLIED_RECEIPT_MISSING");
        };
        return GateOutcome::applied("pages.approval.requested", &approval_request.receipt_id);
    }
    // The forge_build_approval and forge_workflow_plan_proof action
    // kinds belonged to the deleted intake/build lane; their cards can
    // no longer resolve to a governed write, so they hold honestly.
    GateOutcome::held("NO_SUPPORTED_ACTION_BOUND")
}

fn action_request_payload(report: &mdx_core::MessageActionRequestReport) -> String {
    format!(
        r#"{{"kind":"message_action_request","title":{},"summary":{},"action_request_receipt_id":{},"source_receipt_id":{},"allowed_verdicts":{},"state":"ready_for_review","execution_allowed":false}}"#,
        json_string_literal(&report.title),
        json_string_literal(&report.summary),
        json_string_literal(&report.action_request_receipt_id),
        json_string_literal(&report.source_receipt_id),
        json_string_literal(&report.allowed_verdicts)
    )
}

fn action_verdict_payload(
    report: &mdx_core::MessageActionVerdictReport,
    gate: &GateOutcome,
) -> String {
    format!(
        r#"{{"kind":"message_action_verdict","title":{},"action_request_receipt_id":{},"action_verdict_receipt_id":{},"verdict":{},"state":{},"gate_state":{},"applied_receipt_kind":{},"applied_receipt_id":{},"execution_allowed":false}}"#,
        json_string_literal(&report.title),
        json_string_literal(&report.action_request_receipt_id),
        json_string_literal(&report.action_verdict_receipt_id),
        json_string_literal(&report.verdict),
        json_string_literal(gate.state),
        json_string_literal(gate.state),
        json_string_literal(&gate.applied_receipt_kind),
        json_string_literal(&gate.applied_receipt_id)
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GateOutcome {
    state: &'static str,
    applied_receipt_kind: String,
    applied_receipt_id: String,
}

impl GateOutcome {
    fn held(state: &'static str) -> Self {
        Self {
            state,
            applied_receipt_kind: String::new(),
            applied_receipt_id: String::new(),
        }
    }

    fn applied(kind: &str, receipt_id: &str) -> Self {
        Self {
            state: "GOVERNED_ACTION_APPLIED_EXECUTION_BLOCKED",
            applied_receipt_kind: kind.to_string(),
            applied_receipt_id: receipt_id.to_string(),
        }
    }

    fn message_suffix(&self) -> &'static str {
        if self.applied_receipt_id.is_empty() {
            " Execution remains blocked."
        } else {
            " The governed action was applied and still stops before execution."
        }
    }
}

fn apply_deterministic_action_gate(
    kernel: &mut MdxKernel,
    report: &mdx_core::MessageActionVerdictReport,
    tenant_id: &str,
    actor_id: &str,
    _actor_role: &str,
    identity: &GovernedWriteIdentity,
) -> Result<GateOutcome, String> {
    let Some(request_receipt) = kernel
        .ledger()
        .query()
        .by_id(&report.action_request_receipt_id)
        .cloned()
    else {
        return Ok(GateOutcome::held("ACTION_REQUEST_MISSING"));
    };
    let action_payload = payload_value(&request_receipt, "action_payload");
    let kind = json_string_field(action_payload, "kind").unwrap_or_default();
    if kind == "pages_approval_decision" {
        return apply_pages_approval_decision_gate(
            kernel,
            report,
            tenant_id,
            actor_id,
            identity,
            action_payload,
        );
    }
    if report.verdict != "approve" {
        return Ok(GateOutcome::held("VERDICT_RECORDED_NO_ACTION_APPLIED"));
    }
    if kind == "pages_draft_approval_request" {
        return apply_pages_draft_approval_request_gate(
            kernel,
            report,
            tenant_id,
            actor_id,
            identity,
            action_payload,
        );
    }
    // The forge_build_approval and forge_workflow_plan_proof action kinds
    // belonged to the deleted intake/build lane; approving such a card no
    // longer applies a governed write.
    Ok(GateOutcome::held("NO_SUPPORTED_ACTION_BOUND"))
}

fn apply_pages_draft_approval_request_gate(
    kernel: &mut MdxKernel,
    report: &mdx_core::MessageActionVerdictReport,
    tenant_id: &str,
    actor_id: &str,
    identity: &GovernedWriteIdentity,
    action_payload: &str,
) -> Result<GateOutcome, String> {
    let approval_request_id = pages_approval_request_id(action_payload, &report.action_verdict_id);
    if let Some(existing) = kernel.ledger().entries().iter().find(|receipt| {
        receipt.kind == "pages.approval.requested"
            && payload_value(receipt, "approval_request_id") == approval_request_id
    }) {
        return Ok(GateOutcome::applied(
            "pages.approval.requested",
            &existing.receipt_id,
        ));
    }
    let slug = message_action_slug(&report.action_verdict_id);
    let draft_id = json_string_field(action_payload, "draft_id")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("message_page_draft_{slug}"));
    let document_id = json_string_field(action_payload, "document_id")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("page_message_{slug}"));
    let title = json_string_field(action_payload, "title")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| report.title.clone());
    let body_text = json_string_field(action_payload, "body_text")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            format!(
                "# {}\n\n{}\n\nSource Message action: {}",
                report.title, report.decision_note, report.action_request_receipt_id
            )
        });
    let body_ref = crate::pages_body_store::store_draft_body(&draft_id, &body_text)
        .ok_or_else(|| "message action Pages draft body could not be stored".to_string())?;
    let source_publication_receipt_id =
        json_string_field(action_payload, "source_publication_receipt_id").unwrap_or_default();
    let revision_id = json_string_field(action_payload, "revision_id")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("rev_message_{slug}"));
    let draft = kernel
        .save_pages_edit_draft_local_with_identity(
            PagesEditDraft {
                tenant_id,
                actor_id,
                draft_id: &draft_id,
                document_id: &document_id,
                title: &title,
                body_ref: &body_ref,
                source_publication_receipt_id: &source_publication_receipt_id,
                origin_receipt_id: "",
                origin_surface: "",
                revision_id: &revision_id,
            },
            identity,
        )
        .map_err(|error| error.message())?;
    let requested_visibility = json_string_field(action_payload, "requested_visibility")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "tenant_review".to_string());
    let approval = kernel
        .save_pages_approval_request_local_with_identity(
            PagesApprovalRequest {
                tenant_id,
                actor_id,
                approval_request_id: &approval_request_id,
                source_edit_draft_receipt_id: &draft.edit_draft_receipt_id,
                document_id: &document_id,
                draft_id: &draft_id,
                requested_visibility: &requested_visibility,
            },
            identity,
        )
        .map_err(|error| error.message())?;
    Ok(GateOutcome::applied(
        "pages.approval.requested",
        &approval.approval_request_receipt_id,
    ))
}

fn apply_pages_approval_decision_gate(
    kernel: &mut MdxKernel,
    report: &mdx_core::MessageActionVerdictReport,
    tenant_id: &str,
    actor_id: &str,
    identity: &GovernedWriteIdentity,
    action_payload: &str,
) -> Result<GateOutcome, String> {
    let decision_outcome = match report.verdict.as_str() {
        "approve" => "approved",
        "reject" => "rejected",
        _ => return Ok(GateOutcome::held("VERDICT_RECORDED_NO_ACTION_APPLIED")),
    };
    let approval_request_receipt_id =
        json_string_field(action_payload, "approval_request_receipt_id").unwrap_or_default();
    if approval_request_receipt_id.is_empty() {
        return Ok(GateOutcome::held(
            "ACTION_PAYLOAD_MISSING_PAGES_APPROVAL_REQUEST",
        ));
    }
    let approval_decision_id =
        pages_approval_decision_id(action_payload, &report.action_verdict_id);
    if let Some(existing) = kernel.ledger().entries().iter().find(|receipt| {
        receipt.kind == "pages.approval.decision.recorded"
            && payload_value(receipt, "approval_decision_id") == approval_decision_id
    }) {
        return Ok(GateOutcome::applied(
            "pages.approval.decision.recorded",
            &existing.receipt_id,
        ));
    }
    let decision_note = json_string_field(action_payload, "decision_note")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| report.decision_note.clone());
    let decision = kernel
        .save_pages_approval_decision_local_with_identity(
            PagesApprovalDecision {
                tenant_id,
                actor_id,
                approval_decision_id: &approval_decision_id,
                approval_request_receipt_id: &approval_request_receipt_id,
                decision_outcome,
                decision_note: &decision_note,
                source_route: "/messages/action-verdicts.json",
            },
            identity,
        )
        .map_err(|error| error.message())?;
    Ok(GateOutcome::applied(
        "pages.approval.decision.recorded",
        &decision.approval_decision_receipt_id,
    ))
}

fn pages_approval_request_id(action_payload: &str, action_verdict_id: &str) -> String {
    json_string_field(action_payload, "approval_request_id")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            format!(
                "message_pages_approval_{}",
                message_action_slug(action_verdict_id)
            )
        })
}

fn pages_approval_decision_id(action_payload: &str, action_verdict_id: &str) -> String {
    json_string_field(action_payload, "approval_decision_id")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            format!(
                "message_pages_decision_{}",
                message_action_slug(action_verdict_id)
            )
        })
}

fn message_action_slug(value: &str) -> String {
    let mut out = String::new();
    let mut previous_separator = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            out.push(character.to_ascii_lowercase());
            previous_separator = false;
        } else if (character == '_' || character == '-') && !previous_separator {
            out.push(character);
            previous_separator = true;
        } else if !previous_separator {
            out.push('_');
            previous_separator = true;
        }
    }
    let trimmed = out.trim_matches(['_', '-']).to_string();
    if trimmed.is_empty() {
        "local".to_string()
    } else {
        trimmed
    }
}

fn latest_action_request_receipt_id(kernel: &MdxKernel) -> String {
    kernel
        .ledger()
        .entries()
        .iter()
        .rev()
        .find(|receipt| receipt.kind == "message.action.requested")
        .map(|receipt| receipt.receipt_id.clone())
        .unwrap_or_default()
}

fn latest_or_seed_action_request_receipt_id(
    kernel: &mut MdxKernel,
    tenant_id: &str,
) -> Result<String, String> {
    let latest = latest_action_request_receipt_id(kernel);
    if !latest.is_empty() {
        return Ok(latest);
    }
    let source_receipt_id =
        seed_action_source_receipt(kernel, tenant_id, "human:local_user", "owner")?;
    let identity = GovernedWriteIdentity {
        identity_source: "local_message_action_gate".to_string(),
        actor_kind: "agent".to_string(),
        subject_actor_id: "agent:assistant".to_string(),
        delegation_id: String::new(),
        authority_scope: vec!["message:action:request".to_string()],
    };
    kernel
        .save_message_action_request_local_with_identity(
            MessageActionRequest {
                tenant_id,
                actor_id: "agent:assistant",
                thread_id: "local",
                channel_id: "forge",
                action_request_id: "message_action_verdict_seed_request",
                source_receipt_id: &source_receipt_id,
                title: "Review the proposed action",
                summary: "Seeded local action request for route smoke.",
                proposed_action: "Choose approve, edit, reject, or respond.",
                action_payload: r#"{"kind":"local_message_action","execution_allowed":false}"#,
            },
            &identity,
        )
        .map(|report| report.action_request_receipt_id)
        .map_err(|error| error.message())
}

fn seed_action_source_receipt(
    kernel: &mut MdxKernel,
    tenant_id: &str,
    actor_id: &str,
    actor_role: &str,
) -> Result<String, String> {
    let _ = actor_role;
    kernel
        .record_forge_run_event(mdx_core::ForgeRunEvent {
            tenant_id,
            actor_id,
            run_id: "run_message_action_source_local_001",
            event: "run_started",
            work_item_id: "w_message_action_source",
            detail: "accepted",
            turn: 0,
            input_tokens: 0,
            output_tokens: 0,
        })
        .map(|report| report.receipt_id)
        .map_err(|error| error.message())
}

fn payload_value<'a>(receipt: &'a mdx_core::Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}

fn human_token(value: &str) -> String {
    let mut out = String::new();
    let mut capitalize_next = true;
    for character in value.replace(['_', '-'], " ").chars() {
        if capitalize_next {
            out.extend(character.to_uppercase());
            capitalize_next = false;
        } else {
            out.push(character);
        }
        if character == ' ' {
            capitalize_next = true;
        }
    }
    out
}

fn json_string_field(body: &str, key: &str) -> Option<String> {
    let after_key = body.split(&format!("\"{key}\"")).nth(1)?;
    let after_colon = after_key.split_once(':')?.1.trim_start();
    let value = after_colon.strip_prefix('"')?;
    let mut out = String::new();
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            out.push(match character {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                other => other,
            });
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '"' {
            return Some(out);
        } else {
            out.push(character);
        }
    }
    None
}
