use crate::RouteResponse;
use mdx_core::{
    AuthInviteRedemption, AuthInviteRequest, AuthRoleAssignment, AuthSessionControl,
    AuthTenantPolicyPreflight, BetaEnrollment, MdxKernel, json_string_literal,
};
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path == "/auth/readiness.json" {
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
            render_auth_readiness_json(&kernel).map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/auth/tenant-policy-preflights.json" {
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
    if path == "/auth/tenant-policy-preflights/projection.json" {
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
    if path == "/auth/invite-requests.json" {
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
            render_invite_post_json(body, &mut kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/auth/invite-requests/projection.json" {
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
        return Some(Ok(RouteResponse::json(
            "200 OK",
            render_invite_projection_json(&kernel),
        )));
    }
    if path == "/auth/invite-redemptions.json" {
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
            render_invite_redemption_post_json(body, &mut kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/auth/invite-redemptions/projection.json" {
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
        return Some(Ok(RouteResponse::json(
            "200 OK",
            render_invite_redemption_projection_json(&kernel),
        )));
    }
    if path == "/auth/role-assignments.json" {
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
            render_role_assignment_post_json(body, &mut kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/auth/role-assignments/projection.json" {
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
        return Some(Ok(RouteResponse::json(
            "200 OK",
            render_role_assignment_projection_json(&kernel),
        )));
    }
    if path == "/auth/session-controls.json" {
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
            render_session_control_post_json(body, &mut kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/auth/session-controls/projection.json" {
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
        return Some(Ok(RouteResponse::json(
            "200 OK",
            render_session_control_projection_json(&kernel),
        )));
    }
    None
}

fn render_auth_readiness_json(kernel: &MdxKernel) -> Result<String, String> {
    let preflight_count = receipt_count(kernel, "auth.tenant_policy.preflighted");
    let invite_count = receipt_count(kernel, "auth.invite.requested");
    let role_assignment_count = receipt_count(kernel, "auth.role_assignment.recorded");
    let session_activation_count = receipt_count(kernel, "auth.session.activation.recorded");
    let session_revocation_count = receipt_count(kernel, "auth.session.revocation.recorded");
    let tenant_switch_refusal_count = receipt_count(kernel, "auth.tenant_switch.refused");
    let role_escalation_refusal_count = receipt_count(kernel, "auth.role_escalation.refused");
    let access_ready = invite_count > 0 && role_assignment_count > 0;
    let session_controls_ready = session_activation_count > 0
        && session_revocation_count > 0
        && tenant_switch_refusal_count > 0
        && role_escalation_refusal_count > 0;
    let local_ready = preflight_count > 0 && access_ready && session_controls_ready;
    let status = if preflight_count == 0 {
        "LOCAL-AUTH-MULTIUSER-MISSING-GOVERNED-EVIDENCE"
    } else if !access_ready {
        "LOCAL-AUTH-MULTIUSER-MISSING-ACCESS-EVIDENCE"
    } else if !session_controls_ready {
        "LOCAL-AUTH-MULTIUSER-MISSING-SESSION-CONTROLS"
    } else {
        "LIVE-LOCAL-AUTH-MULTIUSER-READY-PRODUCTION-BLOCKED"
    };
    Ok(format!(
        r#"{{"name":"mdx-auth-multiuser-readiness","status":{},"route":"/auth/readiness.json","read_only":true,"receipt_backed":true,"local_ready":{},"auth_session":{{"route":"/local/auth-session.json","status":"deterministic-local-stub","tenant_id":"local_tenant","user_id":"local_user","role":"owner","roles":["owner","operator","auditor","viewer"],"expires_at":"2027-01-01T00:00:00Z","session_cookie_write_allowed":false,"oauth_login_allowed":false,"password_login_allowed":false}},"tenant_policy":{{"preflight_route":"/auth/tenant-policy-preflights.json","projection_route":"/auth/tenant-policy-preflights/projection.json","preflight_count":{},"required_receipt_kind":"auth.tenant_policy.preflighted","terminal_state":"AUTH_TENANT_POLICY_PREFLIGHT_RECORDED_PRODUCTION_AUTH_BLOCKED","v1_sources_bound":["tenant_orgs","tenant_org_members","user_profiles","mdx_roles","invites","tenant_approved_models","get_user_tenant_id","get_user_access_level","get_user_role"],"tenant_membership_policy_recorded":{},"role_mapping_policy_recorded":{},"invite_policy_recorded":{},"approved_model_policy_recorded":{},"source_receipt_ids":[{}]}},"access_management":{{"invite_route":"/auth/invite-requests.json","invite_projection_route":"/auth/invite-requests/projection.json","role_assignment_route":"/auth/role-assignments.json","role_assignment_projection_route":"/auth/role-assignments/projection.json","invite_count":{},"role_assignment_count":{},"invite_receipt_kind":"auth.invite.requested","role_assignment_receipt_kind":"auth.role_assignment.recorded","access_management_ready":{},"email_delivery_allowed":false,"role_assignment_allowed":false,"session_activation_allowed":false,"session_cookie_write_allowed":false,"production_role_write_allowed":false,"production_auth_provider_allowed":false}},"session_controls":{{"write_route":"/auth/session-controls.json","projection_route":"/auth/session-controls/projection.json","activation_receipt_kind":"auth.session.activation.recorded","revocation_receipt_kind":"auth.session.revocation.recorded","tenant_switch_refusal_receipt_kind":"auth.tenant_switch.refused","role_escalation_refusal_receipt_kind":"auth.role_escalation.refused","session_activation_count":{},"session_revocation_count":{},"tenant_switch_refusal_count":{},"role_escalation_refusal_count":{},"session_controls_ready":{},"session_cookie_write_allowed":false,"session_revocation_provider_allowed":false,"tenant_switch_without_policy_allowed":false,"role_escalation_without_policy_allowed":false,"source_receipt_ids":[{}]}},"actor_admission":{{"governed_write_route_count":40,"actor_admission_required_for_mutation":true,"policy_required_for_mutation":true,"receipt_required_for_mutation":true,"source_route_required_for_mutation":true,"hidden_privilege_bypass_allowed":false,"tenant_switch_without_policy_allowed":false,"role_escalation_without_policy_allowed":false}},"blocked_authority":{{"service_role_shortcut_allowed":false,"user_metadata_authorization_allowed":false,"implicit_tenant_inference_allowed":false,"production_role_write_allowed":false,"production_auth_provider_allowed":false,"supabase_auth_claim_authority_allowed":false,"session_cookie_write_allowed":false,"session_revocation_provider_allowed":false,"oauth_login_allowed":false,"password_login_allowed":false,"cutover_allowed":false,"production_write_allowed":false}},"source_routes":["/local/auth-session.json","/auth/tenant-policy-preflights.json","/auth/tenant-policy-preflights/projection.json","/auth/invite-requests.json","/auth/invite-requests/projection.json","/auth/invite-redemptions.json","/auth/invite-redemptions/projection.json","/auth/role-assignments.json","/auth/role-assignments/projection.json","/auth/session-controls.json","/auth/session-controls/projection.json"],"ui_handoff":{{"target_owner":"claude","target_surface":"apps/mdx Console Access and Session module","safe_to_show_local_auth_ready":{},"safe_to_show_invite_and_role_receipts":true,"safe_to_show_session_control_receipts":true,"must_show_production_auth_blocked":true,"must_show_email_delivery_blocked":true,"must_show_session_activation_blocked":true,"must_show_session_revocation_provider_blocked":true,"must_show_tenant_switch_refusal":true,"must_show_role_escalation_refusal":true,"must_not_enable_service_role_shortcut":true,"must_not_claim_supabase_auth_cutover":true,"must_not_allow_tenant_switch_without_policy":true,"must_not_allow_role_escalation_without_policy":true,"must_keep_local_stub_label_visible":true}},"checks":["make auth-multiuser-readiness-check","make auth-session-control-check","make auth-tenant-policy-preflight-check","make auth-session-check","make v1-auth-tenant-policy-control-check"]}}"#,
        json_string_literal(status),
        local_ready,
        preflight_count,
        preflight_count > 0,
        preflight_count > 0,
        preflight_count > 0,
        preflight_count > 0,
        source_receipt_ids_json(kernel, "auth.tenant_policy.preflighted"),
        invite_count,
        role_assignment_count,
        access_ready,
        session_activation_count,
        session_revocation_count,
        tenant_switch_refusal_count,
        role_escalation_refusal_count,
        session_controls_ready,
        auth_session_control_source_receipt_ids_json(kernel),
        local_ready
    ))
}

fn render_local_post_json(body: &str, kernel: &mut MdxKernel) -> Result<String, String> {
    let preflight_id = json_string_field(body, "preflight_id")
        .unwrap_or_else(|| "auth_tenant_policy_preflight_local_001".to_string());
    let source_auth_session_evidence_id =
        json_string_field(body, "source_auth_session_evidence_id")
            .unwrap_or_else(|| "auth.session.local.stub".to_string());
    let tenant_membership_scope = json_string_field(body, "tenant_membership_scope")
        .unwrap_or_else(|| "tenant_org_members".to_string());
    let role_mapping_scope =
        json_string_field(body, "role_mapping_scope").unwrap_or_else(|| "mdx_roles".to_string());
    let invite_state_scope =
        json_string_field(body, "invite_state_scope").unwrap_or_else(|| "invites".to_string());
    let approved_model_scope = json_string_field(body, "approved_model_scope")
        .unwrap_or_else(|| "tenant_approved_models".to_string());
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    let report = kernel
        .save_auth_tenant_policy_preflight_local_with_identity(
            AuthTenantPolicyPreflight {
                tenant_id: &resolved.tenant_id,
                actor_id: &resolved.actor_id,
                preflight_id: &preflight_id,
                source_auth_session_evidence_id: &source_auth_session_evidence_id,
                tenant_membership_scope: &tenant_membership_scope,
                role_mapping_scope: &role_mapping_scope,
                invite_state_scope: &invite_state_scope,
                approved_model_scope: &approved_model_scope,
            },
            &resolved.identity,
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-auth-tenant-policy-preflight-local-post","status":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","writes_route":"/auth/tenant-policy-preflights.json","projection_route":"/auth/tenant-policy-preflights/projection.json","preflight_id":{},"preflight_receipt_id":{},"policy_decision_id":{},"source_auth_session_evidence_id":{},"tenant_membership_scope":{},"role_mapping_scope":{},"invite_state_scope":{},"approved_model_scope":{},"v1_helper_mapping_recorded":{},"tenant_membership_policy_recorded":{},"role_mapping_policy_recorded":{},"invite_policy_recorded":{},"approved_model_policy_recorded":{},"service_role_shortcut_allowed":{},"user_metadata_authorization_allowed":{},"implicit_tenant_inference_allowed":{},"production_role_write_allowed":{},"production_auth_provider_allowed":{},"supabase_auth_claim_authority_allowed":{},"cutover_allowed":{},"live_substrate_required":false,"production_write_allowed":{}}}"#,
        json_string_literal(report.status),
        json_string_literal(resolved.auth_session_status),
        json_string_literal(&report.preflight_id),
        json_string_literal(&report.preflight_receipt_id),
        json_string_literal(&report.policy_decision_id),
        json_string_literal(&report.source_auth_session_evidence_id),
        json_string_literal(&report.tenant_membership_scope),
        json_string_literal(&report.role_mapping_scope),
        json_string_literal(&report.invite_state_scope),
        json_string_literal(&report.approved_model_scope),
        report.v1_helper_mapping_recorded,
        report.tenant_membership_policy_recorded,
        report.role_mapping_policy_recorded,
        report.invite_policy_recorded,
        report.approved_model_policy_recorded,
        report.service_role_shortcut_allowed,
        report.user_metadata_authorization_allowed,
        report.implicit_tenant_inference_allowed,
        report.production_role_write_allowed,
        report.production_auth_provider_allowed,
        report.supabase_auth_claim_authority_allowed,
        report.cutover_allowed,
        report.production_write_allowed
    ))
}

fn render_invite_post_json(body: &str, kernel: &mut MdxKernel) -> Result<String, String> {
    let invite_id =
        json_string_field(body, "invite_id").unwrap_or_else(|| "auth_invite_local_001".to_string());
    let source_tenant_policy_preflight_receipt_id =
        json_string_field(body, "source_tenant_policy_preflight_receipt_id").unwrap_or_default();
    let invited_actor_id = json_string_field(body, "invited_actor_id")
        .unwrap_or_else(|| "human:invited_engineer".to_string());
    let invited_email_hash = json_string_field(body, "invited_email_hash")
        .unwrap_or_else(|| "sha256:local-invite".to_string());
    let requested_role =
        json_string_field(body, "requested_role").unwrap_or_else(|| "operator".to_string());
    let expires_at =
        json_string_field(body, "expires_at").unwrap_or_else(|| "2026-01-02T00:00:00Z".to_string());
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    let report = kernel
        .save_auth_invite_request_local_with_identity(
            AuthInviteRequest {
                tenant_id: &resolved.tenant_id,
                actor_id: &resolved.actor_id,
                invite_id: &invite_id,
                invited_actor_id: &invited_actor_id,
                invited_email_hash: &invited_email_hash,
                requested_role: &requested_role,
                source_tenant_policy_preflight_receipt_id:
                    &source_tenant_policy_preflight_receipt_id,
                expires_at: &expires_at,
            },
            &resolved.identity,
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-auth-invite-request-local-post","status":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","writes_route":"/auth/invite-requests.json","projection_route":"/auth/invite-requests/projection.json","invite_id":{},"invite_request_receipt_id":{},"policy_decision_id":{},"source_tenant_policy_preflight_receipt_id":{},"invited_actor_id":{},"invited_email_hash":{},"requested_role":{},"expires_at":{},"email_delivery_allowed":{},"role_assignment_allowed":{},"session_activation_allowed":{},"service_role_shortcut_allowed":{},"production_auth_provider_allowed":{},"live_substrate_required":false,"production_write_allowed":{}}}"#,
        json_string_literal(report.status),
        json_string_literal(resolved.auth_session_status),
        json_string_literal(&report.invite_id),
        json_string_literal(&report.invite_request_receipt_id),
        json_string_literal(&report.policy_decision_id),
        json_string_literal(&report.source_tenant_policy_preflight_receipt_id),
        json_string_literal(&report.invited_actor_id),
        json_string_literal(&report.invited_email_hash),
        json_string_literal(&report.requested_role),
        json_string_literal(&report.expires_at),
        report.email_delivery_allowed,
        report.role_assignment_allowed,
        report.session_activation_allowed,
        report.service_role_shortcut_allowed,
        report.production_auth_provider_allowed,
        report.production_write_allowed
    ))
}

fn render_invite_redemption_post_json(
    body: &str,
    kernel: &mut MdxKernel,
) -> Result<String, String> {
    let redemption_id = json_string_field(body, "redemption_id")
        .unwrap_or_else(|| "auth_invite_redemption_local_001".to_string());
    let source_invite_request_receipt_id = invite_request_receipt_id(body, kernel)?;
    let cohort_id =
        json_string_field(body, "cohort_id").unwrap_or_else(|| "cohort_local".to_string());
    let risk_tier = json_string_field(body, "risk_tier").unwrap_or_else(|| "standard".to_string());
    let expected_first_task = json_string_field(body, "expected_first_task")
        .unwrap_or_else(|| "request_forge_recipe".to_string());
    let use_case = json_string_field(body, "use_case").unwrap_or_else(|| "forge".to_string());
    let support_owner =
        json_string_field(body, "support_owner").unwrap_or_else(|| "beta_lead".to_string());
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    let authenticated_actor_id = json_string_field(body, "authenticated_actor_id")
        .unwrap_or_else(|| resolved.actor_id.clone());
    let redemption = kernel
        .save_auth_invite_redemption_local_with_identity(
            AuthInviteRedemption {
                tenant_id: &resolved.tenant_id,
                actor_id: &resolved.actor_id,
                redemption_id: &redemption_id,
                source_invite_request_receipt_id: &source_invite_request_receipt_id,
                authenticated_actor_id: &authenticated_actor_id,
                cohort_id: &cohort_id,
                risk_tier: &risk_tier,
            },
            &resolved.identity,
        )
        .map_err(|error| error.message())?;
    let enrollment = kernel
        .record_beta_enrollment_with_identity(
            &BetaEnrollment {
                participant_id: &authenticated_actor_id,
                cohort_id: &cohort_id,
                role: &redemption.requested_role,
                use_case: &use_case,
                expected_first_task: &expected_first_task,
                support_owner: &support_owner,
                risk_tier: &risk_tier,
                status: "active",
                product_analytics_consent: false,
                follow_up_contact_consent: false,
            },
            &resolved.identity,
            &resolved.tenant_id,
            &authenticated_actor_id,
        )
        .map_err(|error| format!("beta enrollment after invite redemption failed: {error:?}"))?;
    Ok(format!(
        r#"{{"name":"mdx-auth-invite-redemption-local-post","status":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","writes_route":"/auth/invite-redemptions.json","projection_route":"/auth/invite-redemptions/projection.json","first_login_transition_route":"/auth/invite-redemptions.json","first_login_enrollment_route":"/beta/enrollments.json","first_login_writes_enrollment":true,"redemption_id":{},"invite_redemption_receipt_id":{},"policy_decision_id":{},"source_invite_request_receipt_id":{},"invite_id":{},"authenticated_actor_id":{},"requested_role":{},"cohort_id":{},"risk_tier":{},"enrollment_receipt_id":{},"enrollment_record_allowed":{},"session_cookie_write_allowed":{},"production_auth_provider_allowed":{},"live_substrate_required":false,"production_write_allowed":{}}}"#,
        json_string_literal(redemption.status),
        json_string_literal(resolved.auth_session_status),
        json_string_literal(&redemption.redemption_id),
        json_string_literal(&redemption.invite_redemption_receipt_id),
        json_string_literal(&redemption.policy_decision_id),
        json_string_literal(&redemption.source_invite_request_receipt_id),
        json_string_literal(&redemption.invite_id),
        json_string_literal(&redemption.authenticated_actor_id),
        json_string_literal(&redemption.requested_role),
        json_string_literal(&redemption.cohort_id),
        json_string_literal(&redemption.risk_tier),
        json_string_literal(&enrollment.enrollment_receipt_id),
        redemption.enrollment_record_allowed,
        redemption.session_cookie_write_allowed,
        redemption.production_auth_provider_allowed,
        redemption.production_write_allowed
    ))
}

fn render_role_assignment_post_json(body: &str, kernel: &mut MdxKernel) -> Result<String, String> {
    let role_assignment_id = json_string_field(body, "role_assignment_id")
        .unwrap_or_else(|| "auth_role_assignment_local_001".to_string());
    let source_invite_request_receipt_id = invite_request_receipt_id(body, kernel)?;
    let target_actor_id = json_string_field(body, "target_actor_id")
        .unwrap_or_else(|| "human:invited_engineer".to_string());
    let assigned_role =
        json_string_field(body, "assigned_role").unwrap_or_else(|| "operator".to_string());
    let assignment_note = json_string_field(body, "assignment_note")
        .unwrap_or_else(|| "Local role assignment proof only.".to_string());
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    let report = kernel
        .save_auth_role_assignment_local_with_identity(
            AuthRoleAssignment {
                tenant_id: &resolved.tenant_id,
                actor_id: &resolved.actor_id,
                role_assignment_id: &role_assignment_id,
                source_invite_request_receipt_id: &source_invite_request_receipt_id,
                target_actor_id: &target_actor_id,
                assigned_role: &assigned_role,
                assignment_note: &assignment_note,
            },
            &resolved.identity,
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-auth-role-assignment-local-post","status":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","writes_route":"/auth/role-assignments.json","projection_route":"/auth/role-assignments/projection.json","role_assignment_id":{},"role_assignment_receipt_id":{},"policy_decision_id":{},"source_invite_request_receipt_id":{},"invite_id":{},"target_actor_id":{},"assigned_role":{},"assignment_note":{},"membership_state":{},"role_mapping_recorded":{},"tenant_membership_recorded":{},"session_cookie_write_allowed":{},"role_escalation_allowed":{},"production_auth_provider_allowed":{},"production_role_write_allowed":{},"live_substrate_required":false,"production_write_allowed":{}}}"#,
        json_string_literal(report.status),
        json_string_literal(resolved.auth_session_status),
        json_string_literal(&report.role_assignment_id),
        json_string_literal(&report.role_assignment_receipt_id),
        json_string_literal(&report.policy_decision_id),
        json_string_literal(&report.source_invite_request_receipt_id),
        json_string_literal(&report.invite_id),
        json_string_literal(&report.target_actor_id),
        json_string_literal(&report.assigned_role),
        json_string_literal(&report.assignment_note),
        json_string_literal(&report.membership_state),
        report.role_mapping_recorded,
        report.tenant_membership_recorded,
        report.session_cookie_write_allowed,
        report.role_escalation_allowed,
        report.production_auth_provider_allowed,
        report.production_role_write_allowed,
        report.production_write_allowed
    ))
}

fn render_session_control_post_json(body: &str, kernel: &mut MdxKernel) -> Result<String, String> {
    let control_kind =
        json_string_field(body, "control_kind").unwrap_or_else(|| "session_activation".to_string());
    let control_id = json_string_field(body, "control_id")
        .unwrap_or_else(|| format!("auth_session_control_{control_kind}_local_001"));
    let source_role_assignment_receipt_id =
        json_string_field(body, "source_role_assignment_receipt_id").unwrap_or_default();
    let source_session_activation_receipt_id =
        json_string_field(body, "source_session_activation_receipt_id").unwrap_or_default();
    let target_actor_id = json_string_field(body, "target_actor_id")
        .unwrap_or_else(|| "human:session_control_user".to_string());
    let current_role =
        json_string_field(body, "current_role").unwrap_or_else(|| "operator".to_string());
    let requested_role =
        json_string_field(body, "requested_role").unwrap_or_else(|| "owner".to_string());
    let requested_tenant_id = json_string_field(body, "requested_tenant_id")
        .unwrap_or_else(|| "other_tenant".to_string());
    let expires_at =
        json_string_field(body, "expires_at").unwrap_or_else(|| "2026-01-02T00:00:00Z".to_string());
    let reason = json_string_field(body, "reason")
        .unwrap_or_else(|| "Local governed session control proof.".to_string());
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    let report = kernel
        .save_auth_session_control_local_with_identity(
            AuthSessionControl {
                tenant_id: &resolved.tenant_id,
                actor_id: &resolved.actor_id,
                control_id: &control_id,
                control_kind: &control_kind,
                source_role_assignment_receipt_id: &source_role_assignment_receipt_id,
                source_session_activation_receipt_id: &source_session_activation_receipt_id,
                target_actor_id: &target_actor_id,
                current_role: &current_role,
                requested_role: &requested_role,
                requested_tenant_id: &requested_tenant_id,
                expires_at: &expires_at,
                reason: &reason,
            },
            &resolved.identity,
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-auth-session-control-local-post","status":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","writes_route":"/auth/session-controls.json","projection_route":"/auth/session-controls/projection.json","control_id":{},"control_kind":{},"control_receipt_kind":{},"control_receipt_id":{},"policy_decision_id":{},"source_role_assignment_receipt_id":{},"source_session_activation_receipt_id":{},"target_actor_id":{},"current_role":{},"requested_role":{},"requested_tenant_id":{},"expires_at":{},"reason":{},"actor_admission_status":{},"session_activation_recorded":{},"session_revocation_recorded":{},"tenant_switch_refused":{},"role_escalation_refused":{},"session_cookie_write_allowed":{},"production_auth_provider_allowed":{},"production_role_write_allowed":{},"service_role_shortcut_allowed":{},"live_substrate_required":false,"production_write_allowed":{}}}"#,
        json_string_literal(report.status),
        json_string_literal(resolved.auth_session_status),
        json_string_literal(&report.control_id),
        json_string_literal(&report.control_kind),
        json_string_literal(report.control_receipt_kind),
        json_string_literal(&report.control_receipt_id),
        json_string_literal(&report.policy_decision_id),
        json_string_literal(&report.source_role_assignment_receipt_id),
        json_string_literal(&report.source_session_activation_receipt_id),
        json_string_literal(&report.target_actor_id),
        json_string_literal(&report.current_role),
        json_string_literal(&report.requested_role),
        json_string_literal(&report.requested_tenant_id),
        json_string_literal(&report.expires_at),
        json_string_literal(&report.reason),
        json_string_literal(&report.actor_admission_status),
        report.session_activation_recorded,
        report.session_revocation_recorded,
        report.tenant_switch_refused,
        report.role_escalation_refused,
        report.session_cookie_write_allowed,
        report.production_auth_provider_allowed,
        report.production_role_write_allowed,
        report.service_role_shortcut_allowed,
        report.production_write_allowed
    ))
}

fn render_local_projection_json(kernel: &MdxKernel) -> Result<String, String> {
    let preflights = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == "auth.tenant_policy.preflighted")
        .map(|receipt| {
            format!(
                r#"{{"preflight_id":{},"preflight_receipt_id":{},"policy_decision_id":{},"source_auth_session_evidence_id":{},"tenant_membership_scope":{},"role_mapping_scope":{},"invite_state_scope":{},"approved_model_scope":{},"terminal_state":{},"v1_helper_mapping_recorded":true,"tenant_membership_policy_recorded":true,"role_mapping_policy_recorded":true,"invite_policy_recorded":true,"approved_model_policy_recorded":true,"service_role_shortcut_allowed":false,"user_metadata_authorization_allowed":false,"implicit_tenant_inference_allowed":false,"production_role_write_allowed":false,"production_auth_provider_allowed":false,"supabase_auth_claim_authority_allowed":false,"cutover_allowed":false,"production_write_allowed":false}}"#,
                json_string_literal(payload_value(receipt, "preflight_id")),
                json_string_literal(&receipt.receipt_id),
                json_string_literal(receipt.policy_decision_id.as_deref().unwrap_or("")),
                json_string_literal(payload_value(receipt, "source_auth_session_evidence_id")),
                json_string_literal(payload_value(receipt, "tenant_membership_scope")),
                json_string_literal(payload_value(receipt, "role_mapping_scope")),
                json_string_literal(payload_value(receipt, "invite_state_scope")),
                json_string_literal(payload_value(receipt, "approved_model_scope")),
                json_string_literal(payload_value(receipt, "terminal_state"))
            )
        })
        .collect::<Vec<_>>();
    Ok(format!(
        r#"{{"name":"mdx-auth-tenant-policy-preflight-local-projection","status":"OK","auth_session_route":"/local/auth-session.json","writes_route":"/auth/tenant-policy-preflights.json","preflight_count":{},"v1_helper_mapping_recorded":true,"tenant_membership_policy_recorded":true,"role_mapping_policy_recorded":true,"invite_policy_recorded":true,"approved_model_policy_recorded":true,"service_role_shortcut_allowed":false,"user_metadata_authorization_allowed":false,"implicit_tenant_inference_allowed":false,"production_role_write_allowed":false,"production_auth_provider_allowed":false,"supabase_auth_claim_authority_allowed":false,"cutover_allowed":false,"live_substrate_required":false,"production_write_allowed":false,"preflights":[{}]}}"#,
        preflights.len(),
        preflights.join(",")
    ))
}

fn render_invite_projection_json(kernel: &MdxKernel) -> String {
    let invites = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == "auth.invite.requested")
        .map(|receipt| {
            format!(
                r#"{{"invite_id":{},"invite_request_receipt_id":{},"policy_decision_id":{},"source_tenant_policy_preflight_receipt_id":{},"invited_actor_id":{},"invited_email_hash":{},"requested_role":{},"expires_at":{},"terminal_state":{},"email_delivery_allowed":false,"role_assignment_allowed":false,"session_activation_allowed":false,"service_role_shortcut_allowed":false,"production_auth_provider_allowed":false,"production_write_allowed":false}}"#,
                json_string_literal(payload_value(receipt, "invite_id")),
                json_string_literal(&receipt.receipt_id),
                json_string_literal(receipt.policy_decision_id.as_deref().unwrap_or("")),
                json_string_literal(payload_value(receipt, "source_tenant_policy_preflight_receipt_id")),
                json_string_literal(payload_value(receipt, "invited_actor_id")),
                json_string_literal(payload_value(receipt, "invited_email_hash")),
                json_string_literal(payload_value(receipt, "requested_role")),
                json_string_literal(payload_value(receipt, "expires_at")),
                json_string_literal(payload_value(receipt, "terminal_state"))
            )
        })
        .collect::<Vec<_>>();
    format!(
        r#"{{"name":"mdx-auth-invite-request-local-projection","status":"OK","auth_session_route":"/local/auth-session.json","writes_route":"/auth/invite-requests.json","invite_count":{},"email_delivery_allowed":false,"role_assignment_allowed":false,"session_activation_allowed":false,"service_role_shortcut_allowed":false,"production_auth_provider_allowed":false,"live_substrate_required":false,"production_write_allowed":false,"invites":[{}]}}"#,
        invites.len(),
        invites.join(",")
    )
}

fn render_invite_redemption_projection_json(kernel: &MdxKernel) -> String {
    let enrollment_receipts = beta_enrollment_receipts_by_actor_and_cohort(kernel);
    let redemptions = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == "auth.invite.redeemed")
        .map(|receipt| {
            let authenticated_actor_id = payload_value(receipt, "authenticated_actor_id");
            let cohort_id = payload_value(receipt, "cohort_id");
            let enrollment_receipt_id = enrollment_receipts
                .get(&(authenticated_actor_id.to_string(), cohort_id.to_string()))
                .map(String::as_str)
                .unwrap_or("");
            let first_login_status = if enrollment_receipt_id.is_empty() {
                "redemption_recorded_enrollment_missing"
            } else {
                "enrollment_recorded"
            };
            format!(
                r#"{{"redemption_id":{},"invite_redemption_receipt_id":{},"policy_decision_id":{},"source_invite_request_receipt_id":{},"invite_id":{},"authenticated_actor_id":{},"requested_role":{},"cohort_id":{},"risk_tier":{},"terminal_state":{},"first_login_status":{},"enrollment_receipt_id":{},"enrollment_record_allowed":{},"session_cookie_write_allowed":false,"production_auth_provider_allowed":false,"production_write_allowed":false}}"#,
                json_string_literal(payload_value(receipt, "redemption_id")),
                json_string_literal(&receipt.receipt_id),
                json_string_literal(receipt.policy_decision_id.as_deref().unwrap_or("")),
                json_string_literal(payload_value(
                    receipt,
                    "source_invite_request_receipt_id"
                )),
                json_string_literal(payload_value(receipt, "invite_id")),
                json_string_literal(authenticated_actor_id),
                json_string_literal(payload_value(receipt, "requested_role")),
                json_string_literal(cohort_id),
                json_string_literal(payload_value(receipt, "risk_tier")),
                json_string_literal(payload_value(receipt, "terminal_state")),
                json_string_literal(first_login_status),
                json_string_literal(enrollment_receipt_id),
                payload_bool(receipt, "enrollment_record_allowed"),
            )
        })
        .collect::<Vec<_>>();
    format!(
        r#"{{"name":"mdx-auth-invite-redemption-local-projection","status":"OK","auth_session_route":"/local/auth-session.json","writes_route":"/auth/invite-redemptions.json","first_login_transition_route":"/auth/invite-redemptions.json","first_login_enrollment_route":"/beta/enrollments.json","first_login_writes_enrollment":true,"redemption_count":{},"invite_redemption_receipt_kind":"auth.invite.redeemed","enrollment_receipt_kind":"beta.enrollment.recorded","enrollment_record_allowed":true,"session_cookie_write_allowed":false,"production_auth_provider_allowed":false,"live_substrate_required":false,"production_write_allowed":false,"redemptions":[{}]}}"#,
        redemptions.len(),
        redemptions.join(",")
    )
}

fn beta_enrollment_receipts_by_actor_and_cohort(
    kernel: &MdxKernel,
) -> BTreeMap<(String, String), String> {
    let mut out = BTreeMap::new();
    for receipt in kernel
        .ledger()
        .query()
        .by_kind("beta.enrollment.recorded")
        .iter()
    {
        let actor_id = payload_value(receipt, "participant_actor_id");
        let cohort_id = payload_value(receipt, "cohort_id");
        if actor_id.is_empty() || cohort_id.is_empty() {
            continue;
        }
        out.insert(
            (actor_id.to_string(), cohort_id.to_string()),
            receipt.receipt_id.clone(),
        );
    }
    out
}

fn render_role_assignment_projection_json(kernel: &MdxKernel) -> String {
    let assignments = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == "auth.role_assignment.recorded")
        .map(|receipt| {
            format!(
                r#"{{"role_assignment_id":{},"role_assignment_receipt_id":{},"policy_decision_id":{},"source_invite_request_receipt_id":{},"invite_id":{},"target_actor_id":{},"assigned_role":{},"assignment_note":{},"membership_state":{},"terminal_state":{},"role_mapping_recorded":true,"tenant_membership_recorded":true,"session_cookie_write_allowed":false,"role_escalation_allowed":false,"production_auth_provider_allowed":false,"production_role_write_allowed":false,"production_write_allowed":false}}"#,
                json_string_literal(payload_value(receipt, "role_assignment_id")),
                json_string_literal(&receipt.receipt_id),
                json_string_literal(receipt.policy_decision_id.as_deref().unwrap_or("")),
                json_string_literal(payload_value(receipt, "source_invite_request_receipt_id")),
                json_string_literal(payload_value(receipt, "invite_id")),
                json_string_literal(payload_value(receipt, "target_actor_id")),
                json_string_literal(payload_value(receipt, "assigned_role")),
                json_string_literal(payload_value(receipt, "assignment_note")),
                json_string_literal(payload_value(receipt, "membership_state")),
                json_string_literal(payload_value(receipt, "terminal_state"))
            )
        })
        .collect::<Vec<_>>();
    format!(
        r#"{{"name":"mdx-auth-role-assignment-local-projection","status":"OK","auth_session_route":"/local/auth-session.json","writes_route":"/auth/role-assignments.json","assignment_count":{},"role_mapping_recorded":true,"tenant_membership_recorded":true,"session_cookie_write_allowed":false,"role_escalation_allowed":false,"production_auth_provider_allowed":false,"production_role_write_allowed":false,"live_substrate_required":false,"production_write_allowed":false,"assignments":[{}]}}"#,
        assignments.len(),
        assignments.join(",")
    )
}

fn render_session_control_projection_json(kernel: &MdxKernel) -> String {
    let controls = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| is_auth_session_control_kind(&receipt.kind))
        .map(|receipt| {
            format!(
                r#"{{"control_id":{},"control_kind":{},"control_receipt_kind":{},"control_receipt_id":{},"policy_decision_id":{},"source_role_assignment_receipt_id":{},"source_session_activation_receipt_id":{},"target_actor_id":{},"current_role":{},"requested_role":{},"requested_tenant_id":{},"expires_at":{},"reason":{},"actor_admission_status":{},"terminal_state":{},"session_activation_recorded":{},"session_revocation_recorded":{},"tenant_switch_refused":{},"role_escalation_refused":{},"session_cookie_write_allowed":false,"production_auth_provider_allowed":false,"production_role_write_allowed":false,"service_role_shortcut_allowed":false,"production_write_allowed":false}}"#,
                json_string_literal(payload_value(receipt, "control_id")),
                json_string_literal(payload_value(receipt, "control_kind")),
                json_string_literal(&receipt.kind),
                json_string_literal(&receipt.receipt_id),
                json_string_literal(receipt.policy_decision_id.as_deref().unwrap_or("")),
                json_string_literal(payload_value(receipt, "source_role_assignment_receipt_id")),
                json_string_literal(payload_value(receipt, "source_session_activation_receipt_id")),
                json_string_literal(payload_value(receipt, "target_actor_id")),
                json_string_literal(payload_value(receipt, "current_role")),
                json_string_literal(payload_value(receipt, "requested_role")),
                json_string_literal(payload_value(receipt, "requested_tenant_id")),
                json_string_literal(payload_value(receipt, "expires_at")),
                json_string_literal(payload_value(receipt, "reason")),
                json_string_literal(payload_value(receipt, "actor_admission_status")),
                json_string_literal(payload_value(receipt, "terminal_state")),
                payload_bool(receipt, "session_activation_recorded"),
                payload_bool(receipt, "session_revocation_recorded"),
                payload_bool(receipt, "tenant_switch_refused"),
                payload_bool(receipt, "role_escalation_refused")
            )
        })
        .collect::<Vec<_>>();
    format!(
        r#"{{"name":"mdx-auth-session-control-local-projection","status":"OK","auth_session_route":"/local/auth-session.json","writes_route":"/auth/session-controls.json","control_count":{},"session_activation_count":{},"session_revocation_count":{},"tenant_switch_refusal_count":{},"role_escalation_refusal_count":{},"session_cookie_write_allowed":false,"production_auth_provider_allowed":false,"production_role_write_allowed":false,"service_role_shortcut_allowed":false,"live_substrate_required":false,"production_write_allowed":false,"controls":[{}]}}"#,
        controls.len(),
        receipt_count(kernel, "auth.session.activation.recorded"),
        receipt_count(kernel, "auth.session.revocation.recorded"),
        receipt_count(kernel, "auth.tenant_switch.refused"),
        receipt_count(kernel, "auth.role_escalation.refused"),
        controls.join(",")
    )
}

fn invite_request_receipt_id(body: &str, kernel: &MdxKernel) -> Result<String, String> {
    if let Some(receipt_id) = json_string_field(body, "source_invite_request_receipt_id") {
        return Ok(receipt_id);
    }
    kernel
        .ledger()
        .entries()
        .iter()
        .find(|receipt| receipt.kind == "auth.invite.requested")
        .map(|receipt| receipt.receipt_id.clone())
        .ok_or_else(|| "auth role assignment missing source_invite_request_receipt_id".to_string())
}

fn payload_value<'a>(receipt: &'a mdx_core::Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}

fn payload_bool(receipt: &mdx_core::Receipt, key: &str) -> bool {
    receipt.payload.get(key).map(String::as_str) == Some("true")
}

fn receipt_count(kernel: &MdxKernel, kind: &str) -> usize {
    kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == kind)
        .count()
}

fn is_auth_session_control_kind(kind: &str) -> bool {
    matches!(
        kind,
        "auth.session.activation.recorded"
            | "auth.session.revocation.recorded"
            | "auth.tenant_switch.refused"
            | "auth.role_escalation.refused"
    )
}

fn auth_session_control_source_receipt_ids_json(kernel: &MdxKernel) -> String {
    kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| is_auth_session_control_kind(&receipt.kind))
        .map(|receipt| json_string_literal(&receipt.receipt_id))
        .collect::<Vec<_>>()
        .join(",")
}

fn source_receipt_ids_json(kernel: &MdxKernel, kind: &str) -> String {
    kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == kind)
        .map(|receipt| json_string_literal(&receipt.receipt_id))
        .collect::<Vec<_>>()
        .join(",")
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
    fn auth_tenant_policy_preflight_route_records_preflight_and_projection() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = route_response(
            "POST",
            "/auth/tenant-policy-preflights.json",
            r#"{"preflight_id":"auth_tenant_policy_preflight_route_test","source_auth_session_evidence_id":"auth.session.local.stub","tenant_membership_scope":"tenant_org_members","role_mapping_scope":"mdx_roles","invite_state_scope":"invites","approved_model_scope":"tenant_approved_models"}"#,
            &kernel,
        )
        .expect("route handled")
        .expect("post response");
        assert_eq!(response.status, "200 OK");
        for expected in [
            "\"name\":\"mdx-auth-tenant-policy-preflight-local-post\"",
            "\"status\":\"AUTH_TENANT_POLICY_PREFLIGHT_RECORDED_PRODUCTION_AUTH_BLOCKED\"",
            "\"preflight_id\":\"auth_tenant_policy_preflight_route_test\"",
            "\"v1_helper_mapping_recorded\":true",
            "\"tenant_membership_policy_recorded\":true",
            "\"service_role_shortcut_allowed\":false",
            "\"production_auth_provider_allowed\":false",
            "\"cutover_allowed\":false",
            "\"production_write_allowed\":false",
        ] {
            assert!(response.body.contains(expected), "{expected}");
        }
        let projection = route_response(
            "GET",
            "/auth/tenant-policy-preflights/projection.json",
            "",
            &kernel,
        )
        .expect("route handled")
        .expect("projection response");
        assert_eq!(projection.status, "200 OK");
        for expected in [
            "\"name\":\"mdx-auth-tenant-policy-preflight-local-projection\"",
            "\"preflight_count\":1",
            "\"writes_route\":\"/auth/tenant-policy-preflights.json\"",
            "\"preflight_id\":\"auth_tenant_policy_preflight_route_test\"",
            "\"terminal_state\":\"AUTH_TENANT_POLICY_PREFLIGHT_RECORDED_PRODUCTION_AUTH_BLOCKED\"",
        ] {
            assert!(projection.body.contains(expected), "{expected}");
        }
    }

    #[test]
    fn auth_readiness_reports_missing_then_ready_with_production_blocked() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let initial = route_response("GET", "/auth/readiness.json", "", &kernel)
            .expect("readiness route")
            .expect("readiness response");
        for expected in [
            "\"status\":\"LOCAL-AUTH-MULTIUSER-MISSING-GOVERNED-EVIDENCE\"",
            "\"local_ready\":false",
            "\"safe_to_show_local_auth_ready\":false",
            "\"production_auth_provider_allowed\":false",
            "\"supabase_auth_claim_authority_allowed\":false",
        ] {
            assert!(initial.body.contains(expected), "{expected}");
        }
        route_response(
            "POST",
            "/auth/tenant-policy-preflights.json",
            r#"{"preflight_id":"auth_readiness_test","source_auth_session_evidence_id":"auth.session.local.stub","tenant_membership_scope":"tenant_org_members","role_mapping_scope":"mdx_roles","invite_state_scope":"invites","approved_model_scope":"tenant_approved_models"}"#,
            &kernel,
        )
        .expect("preflight route")
        .expect("preflight response");
        let missing_access = route_response("GET", "/auth/readiness.json", "", &kernel)
            .expect("readiness route")
            .expect("readiness response");
        for expected in [
            "\"status\":\"LOCAL-AUTH-MULTIUSER-MISSING-ACCESS-EVIDENCE\"",
            "\"local_ready\":false",
            "\"preflight_count\":1",
            "\"invite_count\":0",
            "\"role_assignment_count\":0",
            "\"safe_to_show_local_auth_ready\":false",
        ] {
            assert!(missing_access.body.contains(expected), "{expected}");
        }
        route_response(
            "POST",
            "/auth/invite-requests.json",
            r#"{"invite_id":"auth_readiness_invite","invited_actor_id":"human:new_engineer","invited_email_hash":"sha256:new-engineer","requested_role":"operator","expires_at":"2026-01-02T00:00:00Z"}"#,
            &kernel,
        )
        .expect("invite route")
        .expect("invite response");
        route_response(
            "POST",
            "/auth/role-assignments.json",
            r#"{"role_assignment_id":"auth_readiness_role","target_actor_id":"human:new_engineer","assigned_role":"operator","assignment_note":"Local role assignment proof."}"#,
            &kernel,
        )
        .expect("role route")
        .expect("role response");
        let missing_controls = route_response("GET", "/auth/readiness.json", "", &kernel)
            .expect("readiness route")
            .expect("readiness response");
        for expected in [
            "\"status\":\"LOCAL-AUTH-MULTIUSER-MISSING-SESSION-CONTROLS\"",
            "\"access_management_ready\":true",
            "\"session_controls_ready\":false",
            "\"safe_to_show_local_auth_ready\":false",
        ] {
            assert!(missing_controls.body.contains(expected), "{expected}");
        }
        for kind in [
            "session_activation",
            "session_revocation",
            "tenant_switch_refusal",
            "role_escalation_refusal",
        ] {
            route_response(
                "POST",
                "/auth/session-controls.json",
                &format!(
                    r#"{{"control_id":"auth_readiness_{kind}","control_kind":"{kind}","target_actor_id":"human:new_engineer","current_role":"operator","requested_role":"owner","requested_tenant_id":"other_tenant","expires_at":"2026-01-02T00:00:00Z","reason":"Local governed session proof."}}"#
                ),
                &kernel,
            )
            .expect("session control route")
            .expect("session control response");
        }
        let ready = route_response("GET", "/auth/readiness.json", "", &kernel)
            .expect("readiness route")
            .expect("readiness response");
        for expected in [
            "\"status\":\"LIVE-LOCAL-AUTH-MULTIUSER-READY-PRODUCTION-BLOCKED\"",
            "\"local_ready\":true",
            "\"preflight_count\":1",
            "\"invite_count\":1",
            "\"role_assignment_count\":1",
            "\"invite_route\":\"/auth/invite-requests.json\"",
            "\"role_assignment_route\":\"/auth/role-assignments.json\"",
            "\"write_route\":\"/auth/session-controls.json\"",
            "\"session_activation_count\":1",
            "\"session_revocation_count\":1",
            "\"tenant_switch_refusal_count\":1",
            "\"role_escalation_refusal_count\":1",
            "\"session_controls_ready\":true",
            "\"tenant_membership_policy_recorded\":true",
            "\"invite_policy_recorded\":true",
            "\"actor_admission_required_for_mutation\":true",
            "\"service_role_shortcut_allowed\":false",
            "\"session_cookie_write_allowed\":false",
            "\"session_revocation_provider_allowed\":false",
            "\"must_not_claim_supabase_auth_cutover\":true",
            "\"safe_to_show_local_auth_ready\":true",
        ] {
            assert!(ready.body.contains(expected), "{expected}");
        }
    }

    #[test]
    fn auth_access_management_routes_record_invite_and_role_assignment() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let invite = route_response(
            "POST",
            "/auth/invite-requests.json",
            r#"{"invite_id":"auth_invite_route_test","invited_actor_id":"human:new_engineer","invited_email_hash":"sha256:new-engineer","requested_role":"operator","expires_at":"2026-01-02T00:00:00Z"}"#,
            &kernel,
        )
        .expect("invite route")
        .expect("invite response");
        assert_eq!(invite.status, "200 OK");
        for expected in [
            "\"name\":\"mdx-auth-invite-request-local-post\"",
            "\"status\":\"AUTH_INVITE_RECORDED_DELIVERY_BLOCKED\"",
            "\"invite_id\":\"auth_invite_route_test\"",
            "\"email_delivery_allowed\":false",
            "\"role_assignment_allowed\":false",
            "\"session_activation_allowed\":false",
            "\"production_auth_provider_allowed\":false",
        ] {
            assert!(invite.body.contains(expected), "{expected}");
        }
        let redemption = route_response(
            "POST",
            "/auth/invite-redemptions.json",
            r#"{"redemption_id":"auth_invite_redemption_route_test","authenticated_actor_id":"human:new_engineer","cohort_id":"cohort_a","risk_tier":"trusted","expected_first_task":"first_forge_request"}"#,
            &kernel,
        )
        .expect("redemption route")
        .expect("redemption response");
        assert_eq!(redemption.status, "200 OK");
        for expected in [
            "\"name\":\"mdx-auth-invite-redemption-local-post\"",
            "\"status\":\"AUTH_INVITE_REDEEMED_ENROLLMENT_READY\"",
            "\"invite_id\":\"auth_invite_route_test\"",
            "\"authenticated_actor_id\":\"human:new_engineer\"",
            "\"cohort_id\":\"cohort_a\"",
            "\"risk_tier\":\"trusted\"",
            "\"first_login_transition_route\":\"/auth/invite-redemptions.json\"",
            "\"first_login_enrollment_route\":\"/beta/enrollments.json\"",
            "\"first_login_writes_enrollment\":true",
            "\"enrollment_record_allowed\":true",
            "\"session_cookie_write_allowed\":false",
        ] {
            assert!(redemption.body.contains(expected), "{expected}");
        }
        let role = route_response(
            "POST",
            "/auth/role-assignments.json",
            r#"{"role_assignment_id":"auth_role_assignment_route_test","target_actor_id":"human:new_engineer","assigned_role":"operator","assignment_note":"Local role assignment proof."}"#,
            &kernel,
        )
        .expect("role route")
        .expect("role response");
        assert_eq!(role.status, "200 OK");
        for expected in [
            "\"name\":\"mdx-auth-role-assignment-local-post\"",
            "\"status\":\"AUTH_ROLE_ASSIGNMENT_RECORDED_SESSION_BLOCKED\"",
            "\"role_mapping_recorded\":true",
            "\"tenant_membership_recorded\":true",
            "\"session_cookie_write_allowed\":false",
            "\"production_role_write_allowed\":false",
        ] {
            assert!(role.body.contains(expected), "{expected}");
        }
        let invites = route_response("GET", "/auth/invite-requests/projection.json", "", &kernel)
            .expect("invite projection route")
            .expect("invite projection");
        assert!(invites.body.contains("\"invite_count\":1"));
        let redemptions = route_response(
            "GET",
            "/auth/invite-redemptions/projection.json",
            "",
            &kernel,
        )
        .expect("redemption projection route")
        .expect("redemption projection");
        assert!(redemptions.body.contains("\"redemption_count\":1"));
        assert!(redemptions.body.contains("\"auth.invite.redeemed\""));
        assert!(
            redemptions
                .body
                .contains("\"first_login_writes_enrollment\":true")
        );
        assert!(
            redemptions
                .body
                .contains("\"first_login_status\":\"enrollment_recorded\"")
        );
        let redemptions_json: serde_json::Value =
            serde_json::from_str(&redemptions.body).expect("redemption projection json");
        assert!(
            redemptions_json["redemptions"][0]["enrollment_receipt_id"]
                .as_str()
                .is_some_and(|id| !id.is_empty())
        );
        let enrollments = crate::beta_program_route::route_response(
            "GET",
            mdx_core::BETA_ENROLLMENT_PROJECTION_ROUTE,
            "",
            &kernel,
        )
        .expect("enrollment projection route")
        .expect("enrollment projection");
        assert!(enrollments.body.contains("\"active_count\":1"));
        assert!(enrollments.body.contains("cohort_a"));
        let assignments =
            route_response("GET", "/auth/role-assignments/projection.json", "", &kernel)
                .expect("assignment projection route")
                .expect("assignment projection");
        assert!(assignments.body.contains("\"assignment_count\":1"));
        let ready = route_response("GET", "/auth/readiness.json", "", &kernel)
            .expect("readiness route")
            .expect("readiness response");
        assert!(ready.body.contains("\"invite_count\":1"));
        assert!(ready.body.contains("\"role_assignment_count\":1"));
        assert!(
            ready
                .body
                .contains("\"safe_to_show_invite_and_role_receipts\":true")
        );
    }

    #[test]
    fn auth_session_control_routes_record_lifecycle_and_denials() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        for (kind, status) in [
            (
                "session_activation",
                "AUTH_SESSION_ACTIVATION_RECORDED_COOKIE_BLOCKED",
            ),
            (
                "session_revocation",
                "AUTH_SESSION_REVOCATION_RECORDED_PROVIDER_BLOCKED",
            ),
            (
                "tenant_switch_refusal",
                "AUTH_TENANT_SWITCH_REFUSED_POLICY_REQUIRED",
            ),
            (
                "role_escalation_refusal",
                "AUTH_ROLE_ESCALATION_REFUSED_POLICY_REQUIRED",
            ),
        ] {
            let response = route_response(
                "POST",
                "/auth/session-controls.json",
                &format!(
                    r#"{{"control_id":"auth_session_control_route_{kind}","control_kind":"{kind}","target_actor_id":"human:new_engineer","current_role":"operator","requested_role":"owner","requested_tenant_id":"other_tenant","expires_at":"2026-01-02T00:00:00Z","reason":"Local governed session proof."}}"#
                ),
                &kernel,
            )
            .expect("session control route")
            .expect("session control response");
            assert_eq!(response.status, "200 OK");
            for expected in [
                "\"name\":\"mdx-auth-session-control-local-post\"",
                &format!("\"status\":\"{status}\""),
                "\"writes_route\":\"/auth/session-controls.json\"",
                "\"session_cookie_write_allowed\":false",
                "\"production_auth_provider_allowed\":false",
                "\"service_role_shortcut_allowed\":false",
            ] {
                assert!(response.body.contains(expected), "{expected}");
            }
        }
        let projection =
            route_response("GET", "/auth/session-controls/projection.json", "", &kernel)
                .expect("session control projection route")
                .expect("session control projection");
        for expected in [
            "\"name\":\"mdx-auth-session-control-local-projection\"",
            "\"control_count\":4",
            "\"session_activation_count\":1",
            "\"session_revocation_count\":1",
            "\"tenant_switch_refusal_count\":1",
            "\"role_escalation_refusal_count\":1",
            "\"production_write_allowed\":false",
        ] {
            assert!(projection.body.contains(expected), "{expected}");
        }
    }
}
