use crate::RouteResponse;
use mdx_core::{
    MdxKernel, Receipt, V1ReadShadowApprovalDecision, V1ReadShadowApprovalRequest,
    V1WriteMirrorApprovalDecision, V1WriteMirrorApprovalRequest, json_string_literal,
};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path == "/v1/read-shadow-approval-requests.json" {
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
            render_local_post_json(body, &mut kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/v1/read-shadow-approval-requests/projection.json" {
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
            render_local_projection_json(&kernel).map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/v1/read-shadow-approval-decisions.json" {
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
            render_local_decision_post_json(body, &mut kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/v1/read-shadow-approval-decisions/projection.json" {
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
            render_local_decision_projection_json(&kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/v1/write-mirror-approval-requests.json" {
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
            render_write_mirror_request_post_json(body, &mut kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/v1/write-mirror-approval-requests/projection.json" {
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
            render_write_mirror_request_projection_json(&kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/v1/write-mirror-approval-decisions.json" {
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
            render_write_mirror_decision_post_json(body, &mut kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/v1/write-mirror-approval-decisions/projection.json" {
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
            render_write_mirror_decision_projection_json(&kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    None
}

fn render_local_post_json(body: &str, kernel: &mut MdxKernel) -> Result<String, String> {
    let approval_request_id = json_string_field(body, "approval_request_id")
        .unwrap_or_else(|| "v1_read_shadow_approval_request_local_001".to_string());
    let preflight_name = json_string_field(body, "preflight_name")
        .unwrap_or_else(|| "v1_production_read_shadow_preflight".to_string());
    let requested_scope = json_string_field(body, "requested_scope")
        .unwrap_or_else(|| "message_shadow_replay".to_string());
    let requested_window = json_string_field(body, "requested_window")
        .unwrap_or_else(|| "2026-06-02T00:00:00Z/2026-06-02T01:00:00Z".to_string());
    let evidence_plan = json_string_field(body, "evidence_plan")
        .unwrap_or_else(|| "counts only with no row payload export".to_string());
    let approver_id =
        json_string_field(body, "approver_id").unwrap_or_else(|| "human:local_owner".to_string());
    let approver_role =
        json_string_field(body, "approver_role").unwrap_or_else(|| "owner".to_string());
    let expires_at =
        json_string_field(body, "expires_at").unwrap_or_else(|| "2030-01-01T00:00:00Z".to_string());
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    let report = kernel
        .save_v1_read_shadow_approval_request_local_with_identity(
            V1ReadShadowApprovalRequest {
                tenant_id: &resolved.tenant_id,
                actor_id: &resolved.actor_id,
                actor_role: &resolved.actor_role,
                approval_request_id: &approval_request_id,
                preflight_name: &preflight_name,
                requested_scope: &requested_scope,
                requested_window: &requested_window,
                evidence_plan: &evidence_plan,
                approver_id: &approver_id,
                approver_role: &approver_role,
                expires_at: &expires_at,
            },
            &resolved.identity,
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-v1-read-shadow-approval-request-local-post","status":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","auth_session_tenant_id":{},"auth_session_user_id":{},"auth_session_role":{},"writes_route":"/v1/read-shadow-approval-requests.json","projection_route":"/v1/read-shadow-approval-requests/projection.json","approval_request_id":{},"approval_request_receipt_id":{},"policy_decision_id":{},"preflight_name":{},"requested_scope":{},"requested_window":{},"approver_id":{},"approver_role":{},"terminal_state":{},"approval_request_shape_valid":{},"human_approval_granted":{},"live_read_allowed":{},"row_payload_allowed":{},"service_role_runtime_allowed":{},"credential_export_allowed":{},"live_substrate_required":false,"production_cutover_allowed":{},"production_write_allowed":{}}}"#,
        json_string_literal(report.status),
        json_string_literal(resolved.auth_session_status),
        json_string_literal(&resolved.tenant_id),
        json_string_literal(&resolved.actor_id),
        json_string_literal(&resolved.actor_role),
        json_string_literal(&report.approval_request_id),
        json_string_literal(&report.approval_request_receipt_id),
        json_string_literal(&report.policy_decision_id),
        json_string_literal(&report.preflight_name),
        json_string_literal(&report.requested_scope),
        json_string_literal(&report.requested_window),
        json_string_literal(&report.approver_id),
        json_string_literal(&report.approver_role),
        json_string_literal(report.terminal_state),
        report.approval_request_shape_valid,
        report.human_approval_granted,
        report.live_read_allowed,
        report.row_payload_allowed,
        report.service_role_runtime_allowed,
        report.credential_export_allowed,
        report.production_cutover_allowed,
        report.production_write_allowed
    ))
}

pub(crate) fn render_local_projection_json(kernel: &MdxKernel) -> Result<String, String> {
    let requests = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| is_v1_read_shadow_approval_request_receipt(receipt))
        .map(|receipt| {
            format!(
                r#"{{"approval_request_id":{},"approval_request_receipt_id":{},"policy_decision_id":{},"preflight_name":{},"requested_scope":{},"requested_window":{},"evidence_plan":{},"approver_id":{},"approver_role":{},"expires_at":{},"terminal_state":{},"approval_request_shape_valid":true,"human_approval_granted":false,"live_read_allowed":false,"row_payload_allowed":false,"service_role_runtime_allowed":false,"credential_export_allowed":false,"production_cutover_allowed":false,"production_write_allowed":false}}"#,
                json_string_literal(payload_value(receipt, "approval_request_id")),
                json_string_literal(&receipt.receipt_id),
                json_string_literal(receipt.policy_decision_id.as_deref().unwrap_or("")),
                json_string_literal(payload_value(receipt, "preflight_name")),
                json_string_literal(payload_value(receipt, "requested_scope")),
                json_string_literal(payload_value(receipt, "requested_window")),
                json_string_literal(payload_value(receipt, "evidence_plan")),
                json_string_literal(payload_value(receipt, "approver_id")),
                json_string_literal(payload_value(receipt, "approver_role")),
                json_string_literal(payload_value(receipt, "expires_at")),
                json_string_literal(payload_value(receipt, "terminal_state"))
            )
        })
        .collect::<Vec<_>>();
    Ok(format!(
        r#"{{"name":"mdx-v1-read-shadow-approval-request-local-projection","status":"OK","auth_session_route":"/local/auth-session.json","writes_route":"/v1/read-shadow-approval-requests.json","approval_request_count":{},"approval_request_shape_valid":true,"human_approval_granted":false,"live_read_allowed":false,"row_payload_allowed":false,"service_role_runtime_allowed":false,"credential_export_allowed":false,"live_substrate_required":false,"production_cutover_allowed":false,"production_write_allowed":false,"requests":[{}]}}"#,
        requests.len(),
        requests.join(",")
    ))
}

fn render_local_decision_post_json(body: &str, kernel: &mut MdxKernel) -> Result<String, String> {
    let approval_decision_id = json_string_field(body, "approval_decision_id")
        .unwrap_or_else(|| "v1_read_shadow_approval_decision_local_001".to_string());
    let approval_request_receipt_id =
        json_string_field(body, "approval_request_receipt_id").unwrap_or_default();
    let decision_scope = json_string_field(body, "decision_scope")
        .unwrap_or_else(|| "message_shadow_replay".to_string());
    let decision_outcome = json_string_field(body, "decision_outcome")
        .unwrap_or_else(|| "hold_for_live_turn_on".to_string());
    let decision_rationale = json_string_field(body, "decision_rationale")
        .unwrap_or_else(|| "record the human decision shape before live reads".to_string());
    let decided_by =
        json_string_field(body, "decided_by").unwrap_or_else(|| "human:local_owner".to_string());
    let decided_role =
        json_string_field(body, "decided_role").unwrap_or_else(|| "owner".to_string());
    let expires_at =
        json_string_field(body, "expires_at").unwrap_or_else(|| "2030-01-01T00:00:00Z".to_string());
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    let report = kernel
        .save_v1_read_shadow_approval_decision_local_with_identity(
            V1ReadShadowApprovalDecision {
                tenant_id: &resolved.tenant_id,
                actor_id: &resolved.actor_id,
                actor_role: &resolved.actor_role,
                approval_decision_id: &approval_decision_id,
                approval_request_receipt_id: &approval_request_receipt_id,
                decision_scope: &decision_scope,
                decision_outcome: &decision_outcome,
                decision_rationale: &decision_rationale,
                decided_by: &decided_by,
                decided_role: &decided_role,
                expires_at: &expires_at,
            },
            &resolved.identity,
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-v1-read-shadow-approval-decision-local-post","status":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","auth_session_tenant_id":{},"auth_session_user_id":{},"auth_session_role":{},"writes_route":"/v1/read-shadow-approval-decisions.json","projection_route":"/v1/read-shadow-approval-decisions/projection.json","approval_decision_id":{},"approval_decision_receipt_id":{},"policy_decision_id":{},"approval_request_receipt_id":{},"decision_scope":{},"decision_outcome":{},"terminal_state":{},"approval_decision_shape_valid":{},"human_decision_recorded":{},"human_approval_granted":{},"live_read_allowed":{},"row_payload_allowed":{},"service_role_runtime_allowed":{},"credential_export_allowed":{},"live_substrate_required":false,"production_cutover_allowed":{},"production_write_allowed":{}}}"#,
        json_string_literal(report.status),
        json_string_literal(resolved.auth_session_status),
        json_string_literal(&resolved.tenant_id),
        json_string_literal(&resolved.actor_id),
        json_string_literal(&resolved.actor_role),
        json_string_literal(&report.approval_decision_id),
        json_string_literal(&report.approval_decision_receipt_id),
        json_string_literal(&report.policy_decision_id),
        json_string_literal(&report.approval_request_receipt_id),
        json_string_literal(&report.decision_scope),
        json_string_literal(&report.decision_outcome),
        json_string_literal(report.terminal_state),
        report.approval_decision_shape_valid,
        report.human_decision_recorded,
        report.human_approval_granted,
        report.live_read_allowed,
        report.row_payload_allowed,
        report.service_role_runtime_allowed,
        report.credential_export_allowed,
        report.production_cutover_allowed,
        report.production_write_allowed
    ))
}

pub(crate) fn render_local_decision_projection_json(kernel: &MdxKernel) -> Result<String, String> {
    let decisions = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| is_v1_read_shadow_approval_decision_receipt(receipt))
        .map(|receipt| {
            format!(
                r#"{{"approval_decision_id":{},"approval_decision_receipt_id":{},"policy_decision_id":{},"approval_request_receipt_id":{},"decision_scope":{},"decision_outcome":{},"decision_rationale":{},"decided_by":{},"decided_role":{},"expires_at":{},"terminal_state":{},"approval_decision_shape_valid":true,"human_decision_recorded":true,"human_approval_granted":false,"live_read_allowed":false,"row_payload_allowed":false,"service_role_runtime_allowed":false,"credential_export_allowed":false,"production_cutover_allowed":false,"production_write_allowed":false}}"#,
                json_string_literal(payload_value(receipt, "approval_decision_id")),
                json_string_literal(&receipt.receipt_id),
                json_string_literal(receipt.policy_decision_id.as_deref().unwrap_or("")),
                json_string_literal(payload_value(receipt, "approval_request_receipt_id")),
                json_string_literal(payload_value(receipt, "decision_scope")),
                json_string_literal(payload_value(receipt, "decision_outcome")),
                json_string_literal(payload_value(receipt, "decision_rationale")),
                json_string_literal(payload_value(receipt, "decided_by")),
                json_string_literal(payload_value(receipt, "decided_role")),
                json_string_literal(payload_value(receipt, "expires_at")),
                json_string_literal(payload_value(receipt, "terminal_state"))
            )
        })
        .collect::<Vec<_>>();
    Ok(format!(
        r#"{{"name":"mdx-v1-read-shadow-approval-decision-local-projection","status":"OK","auth_session_route":"/local/auth-session.json","writes_route":"/v1/read-shadow-approval-decisions.json","approval_decision_count":{},"approval_decision_shape_valid":true,"human_decision_recorded":{},"human_approval_granted":false,"live_read_allowed":false,"row_payload_allowed":false,"service_role_runtime_allowed":false,"credential_export_allowed":false,"live_substrate_required":false,"production_cutover_allowed":false,"production_write_allowed":false,"decisions":[{}]}}"#,
        decisions.len(),
        !decisions.is_empty(),
        decisions.join(",")
    ))
}

fn render_write_mirror_request_post_json(
    body: &str,
    kernel: &mut MdxKernel,
) -> Result<String, String> {
    let approval_request_id = json_string_field(body, "approval_request_id")
        .unwrap_or_else(|| "v1_write_mirror_approval_request_local_001".to_string());
    let policy_control_name = json_string_field(body, "policy_control_name")
        .unwrap_or_else(|| "v1_write_shadow_mirror_policy_control".to_string());
    let requested_scope = json_string_field(body, "requested_scope")
        .unwrap_or_else(|| "message_shadow_replay".to_string());
    let requested_window = json_string_field(body, "requested_window")
        .unwrap_or_else(|| "2026-06-02T00:00:00Z/2026-06-09T00:00:00Z".to_string());
    let reconciliation_plan = json_string_field(body, "reconciliation_plan")
        .unwrap_or_else(|| "receipt comparison only with production writes blocked".to_string());
    let rollback_owner = json_string_field(body, "rollback_owner")
        .unwrap_or_else(|| "human:local_owner".to_string());
    let approver_id =
        json_string_field(body, "approver_id").unwrap_or_else(|| "human:local_owner".to_string());
    let approver_role =
        json_string_field(body, "approver_role").unwrap_or_else(|| "owner".to_string());
    let expires_at =
        json_string_field(body, "expires_at").unwrap_or_else(|| "2030-01-01T00:00:00Z".to_string());
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    let report = kernel
        .save_v1_write_mirror_approval_request_local_with_identity(
            V1WriteMirrorApprovalRequest {
                tenant_id: &resolved.tenant_id,
                actor_id: &resolved.actor_id,
                actor_role: &resolved.actor_role,
                approval_request_id: &approval_request_id,
                policy_control_name: &policy_control_name,
                requested_scope: &requested_scope,
                requested_window: &requested_window,
                reconciliation_plan: &reconciliation_plan,
                rollback_owner: &rollback_owner,
                approver_id: &approver_id,
                approver_role: &approver_role,
                expires_at: &expires_at,
            },
            &resolved.identity,
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-v1-write-mirror-approval-request-local-post","status":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","auth_session_tenant_id":{},"auth_session_user_id":{},"auth_session_role":{},"writes_route":"/v1/write-mirror-approval-requests.json","projection_route":"/v1/write-mirror-approval-requests/projection.json","approval_request_id":{},"approval_request_receipt_id":{},"policy_decision_id":{},"policy_control_name":{},"requested_scope":{},"requested_window":{},"rollback_owner":{},"approver_id":{},"approver_role":{},"terminal_state":{},"approval_request_shape_valid":{},"human_approval_granted":{},"write_mirror_allowed":{},"production_write_allowed":{},"service_role_runtime_allowed":{},"live_substrate_required":false,"production_cutover_allowed":{}}}"#,
        json_string_literal(report.status),
        json_string_literal(resolved.auth_session_status),
        json_string_literal(&resolved.tenant_id),
        json_string_literal(&resolved.actor_id),
        json_string_literal(&resolved.actor_role),
        json_string_literal(&report.approval_request_id),
        json_string_literal(&report.approval_request_receipt_id),
        json_string_literal(&report.policy_decision_id),
        json_string_literal(&report.policy_control_name),
        json_string_literal(&report.requested_scope),
        json_string_literal(&report.requested_window),
        json_string_literal(&report.rollback_owner),
        json_string_literal(&report.approver_id),
        json_string_literal(&report.approver_role),
        json_string_literal(report.terminal_state),
        report.approval_request_shape_valid,
        report.human_approval_granted,
        report.write_mirror_allowed,
        report.production_write_allowed,
        report.service_role_runtime_allowed,
        report.production_cutover_allowed
    ))
}

pub(crate) fn render_write_mirror_request_projection_json(
    kernel: &MdxKernel,
) -> Result<String, String> {
    let requests = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| is_v1_write_mirror_approval_request_receipt(receipt))
        .map(|receipt| {
            format!(
                r#"{{"approval_request_id":{},"approval_request_receipt_id":{},"policy_decision_id":{},"policy_control_name":{},"requested_scope":{},"requested_window":{},"reconciliation_plan":{},"rollback_owner":{},"approver_id":{},"approver_role":{},"expires_at":{},"terminal_state":{},"approval_request_shape_valid":true,"human_approval_granted":false,"write_mirror_allowed":false,"production_write_allowed":false,"service_role_runtime_allowed":false,"production_cutover_allowed":false}}"#,
                json_string_literal(payload_value(receipt, "approval_request_id")),
                json_string_literal(&receipt.receipt_id),
                json_string_literal(receipt.policy_decision_id.as_deref().unwrap_or("")),
                json_string_literal(payload_value(receipt, "policy_control_name")),
                json_string_literal(payload_value(receipt, "requested_scope")),
                json_string_literal(payload_value(receipt, "requested_window")),
                json_string_literal(payload_value(receipt, "reconciliation_plan")),
                json_string_literal(payload_value(receipt, "rollback_owner")),
                json_string_literal(payload_value(receipt, "approver_id")),
                json_string_literal(payload_value(receipt, "approver_role")),
                json_string_literal(payload_value(receipt, "expires_at")),
                json_string_literal(payload_value(receipt, "terminal_state"))
            )
        })
        .collect::<Vec<_>>();
    Ok(format!(
        r#"{{"name":"mdx-v1-write-mirror-approval-request-local-projection","status":"OK","auth_session_route":"/local/auth-session.json","writes_route":"/v1/write-mirror-approval-requests.json","approval_request_count":{},"approval_request_shape_valid":true,"human_approval_granted":false,"write_mirror_allowed":false,"production_write_allowed":false,"service_role_runtime_allowed":false,"live_substrate_required":false,"production_cutover_allowed":false,"requests":[{}]}}"#,
        requests.len(),
        requests.join(",")
    ))
}

fn render_write_mirror_decision_post_json(
    body: &str,
    kernel: &mut MdxKernel,
) -> Result<String, String> {
    let approval_decision_id = json_string_field(body, "approval_decision_id")
        .unwrap_or_else(|| "v1_write_mirror_approval_decision_local_001".to_string());
    let approval_request_receipt_id =
        json_string_field(body, "approval_request_receipt_id").unwrap_or_default();
    let decision_scope = json_string_field(body, "decision_scope")
        .unwrap_or_else(|| "message_shadow_replay".to_string());
    let decision_outcome = json_string_field(body, "decision_outcome")
        .unwrap_or_else(|| "hold_for_mirror_turn_on".to_string());
    let decision_rationale = json_string_field(body, "decision_rationale")
        .unwrap_or_else(|| "record the decision shape before write mirror approval".to_string());
    let decided_by =
        json_string_field(body, "decided_by").unwrap_or_else(|| "human:local_owner".to_string());
    let decided_role =
        json_string_field(body, "decided_role").unwrap_or_else(|| "owner".to_string());
    let expires_at =
        json_string_field(body, "expires_at").unwrap_or_else(|| "2030-01-01T00:00:00Z".to_string());
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    let report = kernel
        .save_v1_write_mirror_approval_decision_local_with_identity(
            V1WriteMirrorApprovalDecision {
                tenant_id: &resolved.tenant_id,
                actor_id: &resolved.actor_id,
                actor_role: &resolved.actor_role,
                approval_decision_id: &approval_decision_id,
                approval_request_receipt_id: &approval_request_receipt_id,
                decision_scope: &decision_scope,
                decision_outcome: &decision_outcome,
                decision_rationale: &decision_rationale,
                decided_by: &decided_by,
                decided_role: &decided_role,
                expires_at: &expires_at,
            },
            &resolved.identity,
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-v1-write-mirror-approval-decision-local-post","status":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","auth_session_tenant_id":{},"auth_session_user_id":{},"auth_session_role":{},"writes_route":"/v1/write-mirror-approval-decisions.json","projection_route":"/v1/write-mirror-approval-decisions/projection.json","approval_decision_id":{},"approval_decision_receipt_id":{},"policy_decision_id":{},"approval_request_receipt_id":{},"decision_scope":{},"decision_outcome":{},"terminal_state":{},"approval_decision_shape_valid":{},"human_decision_recorded":{},"human_approval_granted":{},"write_mirror_allowed":{},"production_write_allowed":{},"service_role_runtime_allowed":{},"live_substrate_required":false,"production_cutover_allowed":{}}}"#,
        json_string_literal(report.status),
        json_string_literal(resolved.auth_session_status),
        json_string_literal(&resolved.tenant_id),
        json_string_literal(&resolved.actor_id),
        json_string_literal(&resolved.actor_role),
        json_string_literal(&report.approval_decision_id),
        json_string_literal(&report.approval_decision_receipt_id),
        json_string_literal(&report.policy_decision_id),
        json_string_literal(&report.approval_request_receipt_id),
        json_string_literal(&report.decision_scope),
        json_string_literal(&report.decision_outcome),
        json_string_literal(report.terminal_state),
        report.approval_decision_shape_valid,
        report.human_decision_recorded,
        report.human_approval_granted,
        report.write_mirror_allowed,
        report.production_write_allowed,
        report.service_role_runtime_allowed,
        report.production_cutover_allowed
    ))
}

pub(crate) fn render_write_mirror_decision_projection_json(
    kernel: &MdxKernel,
) -> Result<String, String> {
    let decisions = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| is_v1_write_mirror_approval_decision_receipt(receipt))
        .map(|receipt| {
            format!(
                r#"{{"approval_decision_id":{},"approval_decision_receipt_id":{},"policy_decision_id":{},"approval_request_receipt_id":{},"decision_scope":{},"decision_outcome":{},"decision_rationale":{},"decided_by":{},"decided_role":{},"expires_at":{},"terminal_state":{},"approval_decision_shape_valid":true,"human_decision_recorded":true,"human_approval_granted":false,"write_mirror_allowed":false,"production_write_allowed":false,"service_role_runtime_allowed":false,"production_cutover_allowed":false}}"#,
                json_string_literal(payload_value(receipt, "approval_decision_id")),
                json_string_literal(&receipt.receipt_id),
                json_string_literal(receipt.policy_decision_id.as_deref().unwrap_or("")),
                json_string_literal(payload_value(receipt, "approval_request_receipt_id")),
                json_string_literal(payload_value(receipt, "decision_scope")),
                json_string_literal(payload_value(receipt, "decision_outcome")),
                json_string_literal(payload_value(receipt, "decision_rationale")),
                json_string_literal(payload_value(receipt, "decided_by")),
                json_string_literal(payload_value(receipt, "decided_role")),
                json_string_literal(payload_value(receipt, "expires_at")),
                json_string_literal(payload_value(receipt, "terminal_state"))
            )
        })
        .collect::<Vec<_>>();
    Ok(format!(
        r#"{{"name":"mdx-v1-write-mirror-approval-decision-local-projection","status":"OK","auth_session_route":"/local/auth-session.json","writes_route":"/v1/write-mirror-approval-decisions.json","approval_decision_count":{},"approval_decision_shape_valid":true,"human_decision_recorded":{},"human_approval_granted":false,"write_mirror_allowed":false,"production_write_allowed":false,"service_role_runtime_allowed":false,"live_substrate_required":false,"production_cutover_allowed":false,"decisions":[{}]}}"#,
        decisions.len(),
        !decisions.is_empty(),
        decisions.join(",")
    ))
}

fn is_v1_read_shadow_approval_request_receipt(receipt: &Receipt) -> bool {
    receipt.kind == "v1.read_shadow.approval.requested"
        && payload_value(receipt, "source_route") == "/v1/read-shadow-approval-requests.json"
}

fn is_v1_read_shadow_approval_decision_receipt(receipt: &Receipt) -> bool {
    receipt.kind == "v1.read_shadow.approval.decision.recorded"
        && payload_value(receipt, "source_route") == "/v1/read-shadow-approval-decisions.json"
}

fn is_v1_write_mirror_approval_request_receipt(receipt: &Receipt) -> bool {
    receipt.kind == "v1.write_mirror.approval.requested"
        && payload_value(receipt, "source_route") == "/v1/write-mirror-approval-requests.json"
}

fn is_v1_write_mirror_approval_decision_receipt(receipt: &Receipt) -> bool {
    receipt.kind == "v1.write_mirror.approval.decision.recorded"
        && payload_value(receipt, "source_route") == "/v1/write-mirror-approval-decisions.json"
}

fn payload_value<'a>(receipt: &'a Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_read_shadow_approval_request_route_records_blocked_request_and_projection() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = route_response(
            "POST",
            "/v1/read-shadow-approval-requests.json",
            r#"{"approval_request_id":"v1_read_shadow_approval_route_test","requested_scope":"message_shadow_replay"}"#,
            &kernel,
        )
        .expect("route")
        .expect("response");
        assert_eq!(response.status, "200 OK");
        for expected in [
            "\"name\":\"mdx-v1-read-shadow-approval-request-local-post\"",
            "\"status\":\"READ_SHADOW_APPROVAL_REQUEST_RECORDED_LIVE_READ_BLOCKED\"",
            "\"approval_request_shape_valid\":true",
            "\"human_approval_granted\":false",
            "\"live_read_allowed\":false",
            "\"row_payload_allowed\":false",
            "\"service_role_runtime_allowed\":false",
            "\"credential_export_allowed\":false",
            "\"production_cutover_allowed\":false",
            "\"production_write_allowed\":false",
        ] {
            assert!(response.body.contains(expected), "{expected}");
        }
        let projection = route_response(
            "GET",
            "/v1/read-shadow-approval-requests/projection.json",
            "",
            &kernel,
        )
        .expect("projection route")
        .expect("projection response");
        assert_eq!(projection.status, "200 OK");
        for expected in [
            "\"name\":\"mdx-v1-read-shadow-approval-request-local-projection\"",
            "\"approval_request_count\":1",
            "\"writes_route\":\"/v1/read-shadow-approval-requests.json\"",
            "\"approval_request_id\":\"v1_read_shadow_approval_route_test\"",
        ] {
            assert!(projection.body.contains(expected), "{expected}");
        }
    }

    #[test]
    fn v1_read_shadow_approval_decision_route_records_blocked_decision_and_projection() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = route_response(
            "POST",
            "/v1/read-shadow-approval-decisions.json",
            r#"{"approval_decision_id":"v1_read_shadow_approval_decision_route_test","decision_scope":"message_shadow_replay"}"#,
            &kernel,
        )
        .expect("route")
        .expect("response");
        assert_eq!(response.status, "200 OK");
        for expected in [
            "\"name\":\"mdx-v1-read-shadow-approval-decision-local-post\"",
            "\"status\":\"READ_SHADOW_APPROVAL_DECISION_RECORDED_LIVE_READ_BLOCKED\"",
            "\"approval_decision_shape_valid\":true",
            "\"human_decision_recorded\":true",
            "\"human_approval_granted\":false",
            "\"live_read_allowed\":false",
            "\"row_payload_allowed\":false",
            "\"service_role_runtime_allowed\":false",
            "\"credential_export_allowed\":false",
            "\"production_cutover_allowed\":false",
            "\"production_write_allowed\":false",
        ] {
            assert!(response.body.contains(expected), "{expected}");
        }
        let projection = route_response(
            "GET",
            "/v1/read-shadow-approval-decisions/projection.json",
            "",
            &kernel,
        )
        .expect("projection route")
        .expect("projection response");
        assert_eq!(projection.status, "200 OK");
        for expected in [
            "\"name\":\"mdx-v1-read-shadow-approval-decision-local-projection\"",
            "\"approval_decision_count\":1",
            "\"writes_route\":\"/v1/read-shadow-approval-decisions.json\"",
            "\"approval_decision_id\":\"v1_read_shadow_approval_decision_route_test\"",
        ] {
            assert!(projection.body.contains(expected), "{expected}");
        }
    }

    #[test]
    fn v1_write_mirror_approval_request_route_records_blocked_request_and_projection() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = route_response(
            "POST",
            "/v1/write-mirror-approval-requests.json",
            r#"{"approval_request_id":"v1_write_mirror_approval_route_test","requested_scope":"message_shadow_replay"}"#,
            &kernel,
        )
        .expect("route")
        .expect("response");
        assert_eq!(response.status, "200 OK");
        for expected in [
            "\"name\":\"mdx-v1-write-mirror-approval-request-local-post\"",
            "\"status\":\"WRITE_MIRROR_APPROVAL_REQUEST_RECORDED_MIRROR_BLOCKED\"",
            "\"approval_request_shape_valid\":true",
            "\"human_approval_granted\":false",
            "\"write_mirror_allowed\":false",
            "\"production_write_allowed\":false",
            "\"service_role_runtime_allowed\":false",
            "\"production_cutover_allowed\":false",
        ] {
            assert!(response.body.contains(expected), "{expected}");
        }
        let projection = route_response(
            "GET",
            "/v1/write-mirror-approval-requests/projection.json",
            "",
            &kernel,
        )
        .expect("projection route")
        .expect("projection response");
        assert_eq!(projection.status, "200 OK");
        for expected in [
            "\"name\":\"mdx-v1-write-mirror-approval-request-local-projection\"",
            "\"approval_request_count\":1",
            "\"writes_route\":\"/v1/write-mirror-approval-requests.json\"",
            "\"approval_request_id\":\"v1_write_mirror_approval_route_test\"",
        ] {
            assert!(projection.body.contains(expected), "{expected}");
        }
    }

    #[test]
    fn v1_write_mirror_approval_decision_route_records_blocked_decision_and_projection() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = route_response(
            "POST",
            "/v1/write-mirror-approval-decisions.json",
            r#"{"approval_decision_id":"v1_write_mirror_approval_decision_route_test","decision_scope":"message_shadow_replay"}"#,
            &kernel,
        )
        .expect("route")
        .expect("response");
        assert_eq!(response.status, "200 OK");
        for expected in [
            "\"name\":\"mdx-v1-write-mirror-approval-decision-local-post\"",
            "\"status\":\"WRITE_MIRROR_APPROVAL_DECISION_RECORDED_MIRROR_BLOCKED\"",
            "\"approval_decision_shape_valid\":true",
            "\"human_decision_recorded\":true",
            "\"human_approval_granted\":false",
            "\"write_mirror_allowed\":false",
            "\"production_write_allowed\":false",
            "\"service_role_runtime_allowed\":false",
            "\"production_cutover_allowed\":false",
        ] {
            assert!(response.body.contains(expected), "{expected}");
        }
        let projection = route_response(
            "GET",
            "/v1/write-mirror-approval-decisions/projection.json",
            "",
            &kernel,
        )
        .expect("projection route")
        .expect("projection response");
        assert_eq!(projection.status, "200 OK");
        for expected in [
            "\"name\":\"mdx-v1-write-mirror-approval-decision-local-projection\"",
            "\"approval_decision_count\":1",
            "\"writes_route\":\"/v1/write-mirror-approval-decisions.json\"",
            "\"approval_decision_id\":\"v1_write_mirror_approval_decision_route_test\"",
        ] {
            assert!(projection.body.contains(expected), "{expected}");
        }
    }
}
