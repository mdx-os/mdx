use mdx_core::{
    BETA_COHORT_HALT_RECEIPT_KIND, BETA_ENROLLMENT_PROJECTION_ROUTE, BETA_ENROLLMENT_RECEIPT_KIND,
    BETA_ENROLLMENT_ROUTE, BETA_OPERATIONAL_TELEMETRY_KINDS, BETA_TELEMETRY_EVENT_KINDS,
    BETA_TELEMETRY_PROJECTION_ROUTE, BETA_TELEMETRY_RECEIPT_KIND,
    BETA_TELEMETRY_REJECTION_RECEIPT_KIND, BETA_TELEMETRY_ROUTE, BETA_WAITLIST_PROJECTION_ROUTE,
    BETA_WAITLIST_RECEIPT_KIND, BETA_WAITLIST_ROUTE, BetaCohortHalt, BetaEnrollment,
    BetaProgramError, BetaTelemetryEvent, BetaWaitlistRequest, GovernedWriteIdentity, MdxKernel,
    beta_route_path_only_reason, json_string_literal,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, RwLock};

use crate::RouteResponse;

const ENROLLMENT_TOP_LEVEL: &[&str] = &[
    "participant_id",
    "cohort_id",
    "role",
    "use_case",
    "expected_first_task",
    "support_owner",
    "risk_tier",
    "status",
    "product_analytics_consent",
    "follow_up_contact_consent",
    "tenant_id",
    "actor_id",
    "actor_role",
];

const WAITLIST_TOP_LEVEL: &[&str] = &[
    "applicant_id",
    "email_hash",
    "role",
    "repo_ecosystem",
    "build_goal",
    "requested_cohort",
    "referral_source",
    "follow_up_contact_consent",
];

const TELEMETRY_TOP_LEVEL: &[&str] = &[
    "type",
    "route",
    "ts",
    "step",
    "surface",
    "flow",
    "last_step",
    "action",
    "reason_category",
    "interaction_kind",
    "completed",
    "duration_ms",
    "budget_ms",
    "value",
    "app_version",
    "tenant_id",
    "actor_id",
    "actor_role",
];

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        BETA_WAITLIST_ROUTE => Some(post_waitlist(method, body, kernel)),
        BETA_WAITLIST_PROJECTION_ROUTE => Some(get_waitlist_projection(method, kernel)),
        BETA_ENROLLMENT_ROUTE => Some(post_enrollment(method, body, kernel)),
        BETA_ENROLLMENT_PROJECTION_ROUTE => Some(get_enrollment_projection(method, kernel)),
        BETA_TELEMETRY_ROUTE => Some(post_telemetry(method, body, kernel)),
        BETA_TELEMETRY_PROJECTION_ROUTE => Some(get_telemetry_projection(method, kernel)),
        _ => None,
    }
}

fn post_waitlist(
    method: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if method != "POST" {
        return Ok(RouteResponse::json(
            "405 Method Not Allowed",
            "{\"error\":\"method_not_allowed\"}".to_string(),
        ));
    }
    if let Some(reason) = reject_top_level(body, WAITLIST_TOP_LEVEL) {
        return Ok(RouteResponse::json(
            "200 OK",
            waitlist_refusal_json(reason, "body"),
        ));
    }
    let value = match serde_json::from_str::<Value>(body) {
        Ok(value) => value,
        Err(_) => {
            return Ok(RouteResponse::json(
                "200 OK",
                waitlist_refusal_json("invalid_json", "body"),
            ));
        }
    };
    let request = BetaWaitlistRequest {
        applicant_id: string_field(&value, "applicant_id").unwrap_or("waitlist_applicant"),
        email_hash: string_field(&value, "email_hash").unwrap_or("sha256:missing"),
        role: string_field(&value, "role").unwrap_or("engineer"),
        repo_ecosystem: string_field(&value, "repo_ecosystem").unwrap_or("unknown"),
        build_goal: string_field(&value, "build_goal").unwrap_or("evaluate_mdx"),
        requested_cohort: string_field(&value, "requested_cohort").unwrap_or("waitlist"),
        referral_source: string_field(&value, "referral_source").unwrap_or("direct"),
        follow_up_contact_consent: bool_field(&value, "follow_up_contact_consent").unwrap_or(false),
    };
    let identity = GovernedWriteIdentity::local_demo("public_waitlist");
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let body = match kernel.record_beta_waitlist_request_with_identity(
        &request,
        &identity,
        "public_waitlist",
        "public_waitlist",
    ) {
        Ok(report) => format!(
            concat!(
                "{{\"name\":\"mdx-beta-waitlist-local-post\",",
                "\"status\":{},",
                "\"identity_required\":false,",
                "\"applicant_id\":{},",
                "\"email_hash\":{},",
                "\"role\":{},",
                "\"repo_ecosystem\":{},",
                "\"build_goal\":{},",
                "\"requested_cohort\":{},",
                "\"referral_source\":{},",
                "\"follow_up_contact_consent\":{},",
                "\"waitlist_receipt_id\":{},",
                "\"policy_decision_id\":{},",
                "\"projection_route\":\"/beta/waitlist/projection.json\",",
                "\"production_write_allowed\":{}}}"
            ),
            json_string_literal(report.status),
            json_string_literal(&report.applicant_id),
            json_string_literal(&report.email_hash),
            json_string_literal(&report.role),
            json_string_literal(&report.repo_ecosystem),
            json_string_literal(&report.build_goal),
            json_string_literal(&report.requested_cohort),
            json_string_literal(&report.referral_source),
            report.follow_up_contact_consent,
            json_string_literal(&report.waitlist_receipt_id),
            json_string_literal(&report.policy_decision_id),
            report.production_write_allowed,
        ),
        Err(error) => {
            let (reason, field) = classify_error(&error);
            waitlist_refusal_json(reason, &field)
        }
    };
    Ok(RouteResponse::json("200 OK", body))
}

fn post_enrollment(
    method: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if method != "POST" {
        return Ok(RouteResponse::json(
            "405 Method Not Allowed",
            "{\"error\":\"method_not_allowed\"}".to_string(),
        ));
    }
    if let Some(reason) = reject_top_level(body, ENROLLMENT_TOP_LEVEL) {
        return Ok(RouteResponse::json(
            "200 OK",
            enrollment_refusal_json("ACCEPTED_LOCAL_STUB", reason, "body"),
        ));
    }
    let value = match serde_json::from_str::<Value>(body) {
        Ok(value) => value,
        Err(_) => {
            return Ok(RouteResponse::json(
                "200 OK",
                enrollment_refusal_json("ACCEPTED_LOCAL_STUB", "invalid_json", "body"),
            ));
        }
    };
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "tenant_local",
        "local_user",
        "viewer",
    );
    let enrollment = BetaEnrollment {
        participant_id: string_field(&value, "participant_id")
            .unwrap_or(resolved.actor_id.as_str()),
        cohort_id: string_field(&value, "cohort_id").unwrap_or("cohort_local"),
        role: string_field(&value, "role").unwrap_or(resolved.actor_role.as_str()),
        use_case: string_field(&value, "use_case").unwrap_or("forge"),
        expected_first_task: string_field(&value, "expected_first_task")
            .unwrap_or("request_forge_recipe"),
        support_owner: string_field(&value, "support_owner").unwrap_or("beta_lead"),
        risk_tier: string_field(&value, "risk_tier").unwrap_or("standard"),
        status: string_field(&value, "status").unwrap_or("active"),
        product_analytics_consent: bool_field(&value, "product_analytics_consent").unwrap_or(false),
        follow_up_contact_consent: bool_field(&value, "follow_up_contact_consent").unwrap_or(false),
    };
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    if (enrollment.status.trim().is_empty() || enrollment.status.trim() == "active")
        && let Some(cap) = cohort_cap_for(enrollment.cohort_id)
    {
        let active = kernel
            .beta_enrollment_projection()
            .latest_active
            .iter()
            .filter(|record| {
                record.tenant_id == resolved.tenant_id && record.cohort_id == enrollment.cohort_id
            })
            .count();
        if active >= cap {
            let halt = kernel
                .record_beta_cohort_halt_with_identity(
                    &BetaCohortHalt {
                        cohort_id: enrollment.cohort_id,
                        reason: "cohort_cap_reached",
                        active_count: active,
                        cap,
                    },
                    &resolved.identity,
                    &resolved.tenant_id,
                    &resolved.actor_id,
                )
                .map_err(|error| format!("beta cohort halt record failed: {error:?}"))?;
            return Ok(RouteResponse::json(
                "200 OK",
                enrollment_cohort_halt_refusal_json(
                    resolved.auth_session_status,
                    &halt.cohort_id,
                    &halt.cohort_halt_receipt_id,
                    &halt.policy_decision_id,
                    halt.active_count,
                    halt.cap,
                ),
            ));
        }
    }
    let result = kernel.record_beta_enrollment_with_identity(
        &enrollment,
        &resolved.identity,
        &resolved.tenant_id,
        &resolved.actor_id,
    );
    let body = match result {
        Ok(report) => format!(
            concat!(
                "{{\"name\":\"mdx-beta-enrollment-local-post\",",
                "\"status\":{},",
                "\"auth_session_required\":true,",
                "\"auth_session_status\":{},",
                "\"tenant_id\":{},",
                "\"participant_actor_id\":{},",
                "\"participant_id\":{},",
                "\"cohort_id\":{},",
                "\"enrollment_status\":{},",
                "\"product_analytics_consent\":{},",
                "\"follow_up_contact_consent\":{},",
                "\"enrollment_receipt_id\":{},",
                "\"policy_decision_id\":{},",
                "\"projection_route\":\"/beta/enrollments/projection.json\",",
                "\"production_write_allowed\":{}}}"
            ),
            json_string_literal(report.status),
            json_string_literal(&report.auth_session_status),
            json_string_literal(&resolved.tenant_id),
            json_string_literal(&report.participant_actor_id),
            json_string_literal(&report.participant_id),
            json_string_literal(&report.cohort_id),
            json_string_literal(&report.enrollment_status),
            report.product_analytics_consent,
            report.follow_up_contact_consent,
            json_string_literal(&report.enrollment_receipt_id),
            json_string_literal(&report.policy_decision_id),
            report.production_write_allowed,
        ),
        Err(error) => {
            let (reason, field) = classify_error(&error);
            enrollment_refusal_json(resolved.auth_session_status, reason, &field)
        }
    };
    Ok(RouteResponse::json("200 OK", body))
}

fn post_telemetry(
    method: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if method != "POST" {
        return Ok(RouteResponse::json(
            "405 Method Not Allowed",
            "{\"error\":\"method_not_allowed\"}".to_string(),
        ));
    }
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "tenant_local",
        "local_user",
        "viewer",
    );
    let top_level_rejection = reject_top_level(body, TELEMETRY_TOP_LEVEL);
    let parsed = serde_json::from_str::<Value>(body);
    if top_level_rejection.is_some() || parsed.is_err() {
        let attempted = parsed
            .as_ref()
            .ok()
            .and_then(|value| string_field_owned(value, "type"))
            .unwrap_or_else(|| "unknown".to_string());
        let route = parsed
            .as_ref()
            .ok()
            .and_then(|value| string_field_owned(value, "route"))
            .unwrap_or_default();
        let reason = top_level_rejection.unwrap_or("invalid_json");
        let mut kernel = kernel
            .write()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        let report = kernel.record_beta_telemetry_rejection_with_identity(
            &attempted,
            reason,
            &route,
            &resolved.identity,
            &resolved.tenant_id,
            &resolved.actor_id,
        );
        return Ok(RouteResponse::json(
            "200 OK",
            telemetry_rejection_json(&report),
        ));
    }
    let value = parsed.unwrap();
    let event_kind = string_field(&value, "type").unwrap_or_default().to_string();
    let route = string_field(&value, "route")
        .unwrap_or_default()
        .to_string();
    let occurred_at = string_field(&value, "ts").unwrap_or_default().to_string();
    let mut fields = BTreeMap::new();
    if let Some(object) = value.as_object() {
        for (key, value) in object {
            if matches!(key.as_str(), "tenant_id" | "actor_id" | "actor_role") {
                continue;
            }
            if let Some(text) = scalar_to_string(value) {
                fields.insert(key.clone(), text);
            }
        }
    }
    let event = BetaTelemetryEvent {
        event_kind: event_kind.clone(),
        route: route.clone(),
        occurred_at,
        fields,
    };
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let body = match kernel.record_beta_telemetry_with_identity(
        &event,
        &resolved.identity,
        &resolved.tenant_id,
        &resolved.actor_id,
    ) {
        Ok(report) => format!(
            concat!(
                "{{\"name\":\"mdx-beta-telemetry-local-post\",",
                "\"status\":{},",
                "\"auth_session_required\":true,",
                "\"auth_session_status\":{},",
                "\"tenant_id\":{},",
                "\"participant_actor_id\":{},",
                "\"cohort_id\":{},",
                "\"event_kind\":{},",
                "\"product_analytics_consent_observed\":{},",
                "\"operational_security_telemetry\":{},",
                "\"telemetry_receipt_id\":{},",
                "\"policy_decision_id\":{},",
                "\"projection_route\":\"/beta/telemetry-events/projection.json\",",
                "\"production_write_allowed\":{}}}"
            ),
            json_string_literal(report.status),
            json_string_literal(&report.auth_session_status),
            json_string_literal(&resolved.tenant_id),
            json_string_literal(&report.participant_actor_id),
            json_string_literal(&report.cohort_id),
            json_string_literal(&report.event_kind),
            report.product_analytics_consent_observed,
            report.operational_security_telemetry,
            json_string_literal(&report.telemetry_receipt_id),
            json_string_literal(&report.policy_decision_id),
            report.production_write_allowed,
        ),
        Err(error) => {
            let (reason, _) = classify_error(&error);
            let report = kernel.record_beta_telemetry_rejection_with_identity(
                &event_kind,
                reason,
                &route,
                &resolved.identity,
                &resolved.tenant_id,
                &resolved.actor_id,
            );
            telemetry_rejection_json(&report)
        }
    };
    Ok(RouteResponse::json("200 OK", body))
}

fn get_waitlist_projection(
    method: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if method != "GET" {
        return Ok(RouteResponse::json(
            "405 Method Not Allowed",
            "{\"error\":\"method_not_allowed\"}".to_string(),
        ));
    }
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let projection = kernel.beta_waitlist_projection();
    let records = projection
        .records
        .iter()
        .map(|record| {
            format!(
                concat!(
                    "{{\"applicant_id\":{},\"email_hash\":{},",
                    "\"role\":{},\"repo_ecosystem\":{},\"build_goal\":{},",
                    "\"requested_cohort\":{},\"referral_source\":{},",
                    "\"receipt_id\":{},\"receipt_timestamp\":{},",
                    "\"follow_up_contact_consent\":{}}}"
                ),
                json_string_literal(&record.applicant_id),
                json_string_literal(&record.email_hash),
                json_string_literal(&record.role),
                json_string_literal(&record.repo_ecosystem),
                json_string_literal(&record.build_goal),
                json_string_literal(&record.requested_cohort),
                json_string_literal(&record.referral_source),
                json_string_literal(&record.receipt_id),
                json_string_literal(&record.receipt_timestamp),
                record.follow_up_contact_consent,
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            concat!(
                "{{\"name\":\"mdx-beta-waitlist-projection\",",
                "\"status\":\"OK\",",
                "\"route\":\"/beta/waitlist/projection.json\",",
                "\"source_receipt_kind\":{},",
                "\"record_count\":{},",
                "\"identity_required\":false,",
                "\"records\":[{}],",
                "\"receipt_backed\":true,",
                "\"production_write_allowed\":false}}"
            ),
            json_string_literal(BETA_WAITLIST_RECEIPT_KIND),
            projection.records.len(),
            records,
        ),
    ))
}

fn get_enrollment_projection(
    method: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if method != "GET" {
        return Ok(RouteResponse::json(
            "405 Method Not Allowed",
            "{\"error\":\"method_not_allowed\"}".to_string(),
        ));
    }
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let mut projection = kernel.beta_enrollment_projection();
    let read_identity = crate::request_security::current_verified_identity();
    if let Some(identity) = read_identity.as_ref() {
        let operator = is_beta_operator_role(&identity.actor_role);
        projection.records.retain(|record| {
            record.tenant_id == identity.tenant_id
                && (operator || record.actor_id == identity.actor_id)
        });
        projection.latest_active.retain(|record| {
            record.tenant_id == identity.tenant_id
                && (operator || record.actor_id == identity.actor_id)
        });
    }
    let cohort_halts = cohort_halts_json(
        &kernel,
        read_identity
            .as_ref()
            .map(|identity| identity.tenant_id.as_str()),
    );
    let latest = projection
        .latest_active
        .iter()
        .map(|record| {
            format!(
                concat!(
                    "{{\"tenant_id\":{},\"participant_actor_id\":{},",
                    "\"participant_id\":{},\"cohort_id\":{},\"role\":{},",
                    "\"use_case\":{},\"support_owner\":{},\"risk_tier\":{},",
                    "\"receipt_id\":{},\"receipt_timestamp\":{},",
                    "\"product_analytics_consent\":{},",
                    "\"follow_up_contact_consent\":{}}}"
                ),
                json_string_literal(&record.tenant_id),
                json_string_literal(&record.actor_id),
                json_string_literal(&record.participant_id),
                json_string_literal(&record.cohort_id),
                json_string_literal(&record.role),
                json_string_literal(&record.use_case),
                json_string_literal(&record.support_owner),
                json_string_literal(&record.risk_tier),
                json_string_literal(&record.receipt_id),
                json_string_literal(&record.receipt_timestamp),
                record.product_analytics_consent,
                record.follow_up_contact_consent,
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            concat!(
                "{{\"name\":\"mdx-beta-enrollments-projection\",",
                "\"status\":\"OK\",",
                "\"route\":\"/beta/enrollments/projection.json\",",
                "\"source_receipt_kind\":{},",
                "\"cohort_halt_receipt_kind\":{},",
                "\"record_count\":{},",
                "\"active_count\":{},",
                "\"cohort_halt_count\":{},",
                "\"latest_active\":[{}],",
                "\"cohort_halts\":[{}],",
                "\"receipt_backed\":true,",
                "\"production_write_allowed\":false}}"
            ),
            json_string_literal(BETA_ENROLLMENT_RECEIPT_KIND),
            json_string_literal(BETA_COHORT_HALT_RECEIPT_KIND),
            projection.records.len(),
            projection.latest_active.len(),
            cohort_halts.count,
            latest,
            cohort_halts.items,
        ),
    ))
}

fn get_telemetry_projection(
    method: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if method != "GET" {
        return Ok(RouteResponse::json(
            "405 Method Not Allowed",
            "{\"error\":\"method_not_allowed\"}".to_string(),
        ));
    }
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let event_counts = json_counts(kernel.beta_telemetry_counts());
    let rejection_counts = json_counts(kernel.beta_telemetry_rejection_counts());
    let cohort_counts = json_counts(kernel.beta_telemetry_cohort_counts());
    let mut journey_projection = kernel.beta_participant_journey_projection();
    if let Some(identity) = crate::request_security::current_verified_identity() {
        let operator = is_beta_operator_role(&identity.actor_role);
        journey_projection.journeys.retain(|journey| {
            journey.tenant_id == identity.tenant_id
                && (operator || journey.participant_actor_id == identity.actor_id)
        });
    }
    let active_enrollments = journey_projection.journeys.len();
    let activated_count = journey_projection
        .journeys
        .iter()
        .filter(|journey| journey.first_value_observed)
        .count();
    let returned_count = journey_projection
        .journeys
        .iter()
        .filter(|journey| journey.return_session_observed)
        .count();
    let participant_journeys = journey_projection
        .journeys
        .iter()
        .map(|journey| {
            let activation_steps = journey
                .activation_steps
                .iter()
                .map(|step| json_string_literal(step))
                .collect::<Vec<_>>()
                .join(",");
            format!(
                concat!(
                    "{{\"tenant_id\":{},\"participant_actor_id\":{},",
                    "\"participant_id\":{},\"cohort_id\":{},\"role\":{},",
                    "\"expected_first_task\":{},\"support_owner\":{},",
                    "\"product_analytics_consent\":{},",
                    "\"first_seen_at\":{},\"last_seen_at\":{},",
                    "\"event_count\":{},\"session_start_count\":{},",
                    "\"session_end_count\":{},\"distinct_session_day_count\":{},",
                    "\"activation_steps\":[{}],",
                    "\"surface_visit_counts\":{},\"feedback_opened_count\":{},",
                    "\"first_value_observed\":{},\"return_session_observed\":{},",
                    "\"last_app_version\":{}}}"
                ),
                json_string_literal(&journey.tenant_id),
                json_string_literal(&journey.participant_actor_id),
                json_string_literal(&journey.participant_id),
                json_string_literal(&journey.cohort_id),
                json_string_literal(&journey.role),
                json_string_literal(&journey.expected_first_task),
                json_string_literal(&journey.support_owner),
                journey.product_analytics_consent,
                json_string_literal(&journey.first_seen_at),
                json_string_literal(&journey.last_seen_at),
                journey.event_count,
                journey.session_start_count,
                journey.session_end_count,
                journey.distinct_session_day_count,
                activation_steps,
                json_counts(journey.surface_visit_counts.clone()),
                journey.feedback_opened_count,
                journey.first_value_observed,
                journey.return_session_observed,
                json_string_literal(&journey.last_app_version),
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            concat!(
                "{{\"name\":\"mdx-beta-telemetry-projection\",",
                "\"status\":\"OK\",",
                "\"route\":\"/beta/telemetry-events/projection.json\",",
                "\"source_receipt_kinds\":[{},{},{}],",
                "\"accepted_event_kinds\":{},",
                "\"operational_security_event_kinds\":{},",
                "\"active_enrollment_count\":{},",
                "\"journey_count\":{},",
                "\"first_value_count\":{},",
                "\"return_session_count\":{},",
                "\"event_counts\":{},",
                "\"rejection_counts\":{},",
                "\"cohort_counts\":{},",
                "\"participant_journeys\":[{}],",
                "\"unsafe_payload_persisted\":false,",
                "\"raw_event_ttl_days\":14,",
                "\"receipt_backed\":true,",
                "\"production_write_allowed\":false}}"
            ),
            json_string_literal(BETA_ENROLLMENT_RECEIPT_KIND),
            json_string_literal(BETA_TELEMETRY_RECEIPT_KIND),
            json_string_literal(BETA_TELEMETRY_REJECTION_RECEIPT_KIND),
            json_str_array(BETA_TELEMETRY_EVENT_KINDS),
            json_str_array(BETA_OPERATIONAL_TELEMETRY_KINDS),
            active_enrollments,
            journey_projection.journeys.len(),
            activated_count,
            returned_count,
            event_counts,
            rejection_counts,
            cohort_counts,
            participant_journeys,
        ),
    ))
}

fn is_beta_operator_role(role: &str) -> bool {
    matches!(
        role.trim().to_ascii_lowercase().as_str(),
        "owner" | "admin" | "tenant_admin" | "operator" | "security" | "cloud" | "infra"
    )
}

fn telemetry_rejection_json(report: &mdx_core::BetaTelemetryRejectionReport) -> String {
    format!(
        concat!(
            "{{\"name\":\"mdx-beta-telemetry-local-post\",",
            "\"status\":{},",
            "\"auth_session_required\":true,",
            "\"auth_session_status\":{},",
            "\"attempted_event_kind\":{},",
            "\"rejection_reason\":{},",
            "\"route\":{},",
            "\"participant_actor_id\":{},",
            "\"cohort_id\":{},",
            "\"rejection_receipt_id\":{},",
            "\"policy_decision_id\":{},",
            "\"unsafe_payload_persisted\":{},",
            "\"recorded\":false,",
            "\"projection_route\":\"/beta/telemetry-events/projection.json\",",
            "\"production_write_allowed\":{}}}"
        ),
        json_string_literal(report.status),
        json_string_literal(&report.auth_session_status),
        json_string_literal(&report.attempted_event_kind),
        json_string_literal(&report.rejection_reason),
        json_string_literal(&report.route),
        json_string_literal(&report.participant_actor_id),
        json_string_literal(&report.cohort_id),
        json_string_literal(&report.rejection_receipt_id),
        json_string_literal(&report.policy_decision_id),
        report.unsafe_payload_persisted,
        report.production_write_allowed,
    )
}

fn enrollment_refusal_json(auth_session_status: &str, reason: &str, field: &str) -> String {
    format!(
        concat!(
            "{{\"name\":\"mdx-beta-enrollment-local-post\",",
            "\"status\":\"BETA_ENROLLMENT_REFUSED\",",
            "\"auth_session_required\":true,",
            "\"auth_session_status\":{},",
            "\"recorded\":false,",
            "\"refusal_code\":{},",
            "\"refused_field\":{},",
            "\"message\":\"That beta enrollment could not be recorded with the safe program shape.\",",
            "\"production_write_allowed\":false}}"
        ),
        json_string_literal(auth_session_status),
        json_string_literal(reason),
        json_string_literal(field),
    )
}

fn enrollment_cohort_halt_refusal_json(
    auth_session_status: &str,
    cohort_id: &str,
    cohort_halt_receipt_id: &str,
    policy_decision_id: &str,
    active_count: usize,
    cap: usize,
) -> String {
    format!(
        concat!(
            "{{\"name\":\"mdx-beta-enrollment-local-post\",",
            "\"status\":\"BETA_ENROLLMENT_REFUSED\",",
            "\"auth_session_required\":true,",
            "\"auth_session_status\":{},",
            "\"recorded\":false,",
            "\"refusal_code\":\"cohort_cap_reached\",",
            "\"refused_field\":\"cohort_id\",",
            "\"cohort_id\":{},",
            "\"active_count\":{},",
            "\"cohort_cap\":{},",
            "\"cohort_halt_receipt_id\":{},",
            "\"policy_decision_id\":{},",
            "\"projection_route\":\"/beta/enrollments/projection.json\",",
            "\"message\":\"That beta cohort is full, so the enrollment was halted on the record.\",",
            "\"production_write_allowed\":false}}"
        ),
        json_string_literal(auth_session_status),
        json_string_literal(cohort_id),
        active_count,
        cap,
        json_string_literal(cohort_halt_receipt_id),
        json_string_literal(policy_decision_id),
    )
}

fn waitlist_refusal_json(reason: &str, field: &str) -> String {
    format!(
        concat!(
            "{{\"name\":\"mdx-beta-waitlist-local-post\",",
            "\"status\":\"BETA_WAITLIST_REFUSED\",",
            "\"identity_required\":false,",
            "\"recorded\":false,",
            "\"refusal_code\":{},",
            "\"refused_field\":{},",
            "\"message\":\"That waitlist request could not be recorded with the safe program shape.\",",
            "\"production_write_allowed\":false}}"
        ),
        json_string_literal(reason),
        json_string_literal(field),
    )
}

struct CohortHaltsJson {
    count: usize,
    items: String,
}

fn cohort_halts_json(kernel: &MdxKernel, tenant_id: Option<&str>) -> CohortHaltsJson {
    let items = kernel
        .ledger()
        .query()
        .by_kind(BETA_COHORT_HALT_RECEIPT_KIND)
        .iter()
        .filter(|receipt| tenant_id.is_none_or(|tenant| receipt.tenant_id.as_str() == tenant))
        .map(|receipt| {
            let value = |key: &str| receipt.payload.get(key).map(String::as_str).unwrap_or("");
            format!(
                concat!(
                    "{{\"cohort_id\":{},",
                    "\"reason\":{},",
                    "\"active_count\":{},",
                    "\"cohort_cap\":{},",
                    "\"cohort_halt_receipt_id\":{},",
                    "\"policy_decision_id\":{},",
                    "\"receipt_timestamp\":{},",
                    "\"terminal_state\":{}}}"
                ),
                json_string_literal(value("cohort_id")),
                json_string_literal(value("reason")),
                value("active_count").parse::<usize>().unwrap_or(0),
                value("cap").parse::<usize>().unwrap_or(0),
                json_string_literal(&receipt.receipt_id),
                json_string_literal(receipt.policy_decision_id.as_deref().unwrap_or("")),
                json_string_literal(&receipt.receipt_timestamp),
                json_string_literal(value("terminal_state")),
            )
        })
        .collect::<Vec<_>>();
    CohortHaltsJson {
        count: items.len(),
        items: items.join(","),
    }
}

fn classify_error(error: &BetaProgramError) -> (&'static str, String) {
    match error {
        BetaProgramError::Missing(field) => ("missing_field", (*field).to_string()),
        BetaProgramError::UnknownStatus(_) => ("invalid_value", "status".to_string()),
        BetaProgramError::UnknownRiskTier(_) => ("invalid_value", "risk_tier".to_string()),
        BetaProgramError::UnknownTelemetryKind(_) => ("unknown_event_kind", "type".to_string()),
        BetaProgramError::InvalidRoute(_) => ("invalid_route", "route".to_string()),
        BetaProgramError::InvalidValue { field, reason } => {
            if *reason == "unknown_field" {
                ("unknown_field", field.clone())
            } else {
                ("invalid_value", field.clone())
            }
        }
        BetaProgramError::UnsafeValue { field, .. } => ("unsafe_value", field.clone()),
        BetaProgramError::MissingEnrollment => ("missing_enrollment", "enrollment".to_string()),
        BetaProgramError::ProductAnalyticsOptOut => {
            ("product_analytics_opt_out", "consent".to_string())
        }
    }
}

fn cohort_cap_for(cohort_id: &str) -> Option<usize> {
    let scoped = format!(
        "MDX_BETA_COHORT_CAP_{}",
        cohort_id
            .trim()
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_uppercase()
                } else {
                    '_'
                }
            })
            .collect::<String>()
    );
    env_usize(&scoped).or_else(|| env_usize("MDX_BETA_COHORT_CAP"))
}

fn env_usize(name: &str) -> Option<usize> {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
}

fn reject_top_level(body: &str, allowed: &[&str]) -> Option<&'static str> {
    let value = serde_json::from_str::<Value>(body).ok()?;
    let object = value.as_object()?;
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Some("unknown_field");
        }
    }
    None
}

fn string_field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.as_object()?.get(key)?.as_str()
}

fn string_field_owned(value: &Value, key: &str) -> Option<String> {
    string_field(value, key).map(|value| value.to_string())
}

fn bool_field(value: &Value, key: &str) -> Option<bool> {
    let value = value.as_object()?.get(key)?;
    match value {
        Value::Bool(value) => Some(*value),
        Value::String(value) => match value.trim().to_ascii_lowercase().as_str() {
            "true" | "yes" | "1" => Some(true),
            "false" | "no" | "0" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn scalar_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

fn json_counts(counts: BTreeMap<String, usize>) -> String {
    let mut seen = BTreeSet::new();
    let pairs = counts
        .into_iter()
        .filter(|(key, _)| seen.insert(key.clone()))
        .map(|(key, count)| format!("{}:{}", json_string_literal(&key), count))
        .collect::<Vec<_>>()
        .join(",");
    format!("{{{pairs}}}")
}

fn json_str_array(values: &[&str]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| json_string_literal(value))
            .collect::<Vec<_>>()
            .join(",")
    )
}

#[allow(dead_code)]
fn _route_is_safe_for_rejection(route: &str) -> bool {
    beta_route_path_only_reason(route).is_none()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdx_core::{AdmittedIdentity, MdxKernel};

    static ENV_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn kernel() -> Arc<RwLock<MdxKernel>> {
        Arc::new(RwLock::new(MdxKernel::boot_local()))
    }

    fn read_identity(actor_id: &str, actor_role: &str) -> AdmittedIdentity {
        AdmittedIdentity {
            deployment_mode: "local-secure",
            tenant_id: "tenant_local".to_string(),
            actor_id: actor_id.to_string(),
            actor_role: actor_role.to_string(),
            actor_kind: "human".to_string(),
            subject_actor_id: actor_id.to_string(),
            authority_scope: Vec::new(),
            delegation_id: None,
            identity_source: "trusted_session",
            production_write_allowed: false,
        }
    }

    #[test]
    fn route_records_waitlist_request_without_identity() {
        let kernel = kernel();
        let body = r#"{"applicant_id":"applicant_1","email_hash":"sha256:applicant","role":"backend","repo_ecosystem":"rust","build_goal":"ship governed app","requested_cohort":"cohort_a","referral_source":"founder","follow_up_contact_consent":true}"#;
        let response = route_response("POST", BETA_WAITLIST_ROUTE, body, &kernel)
            .unwrap()
            .unwrap();
        assert_eq!(response.status_for_test(), "200 OK");
        assert!(
            response
                .body_text()
                .contains("BETA_WAITLIST_REQUEST_RECORDED")
        );
        assert!(response.body_text().contains("\"identity_required\":false"));

        let projection = route_response("GET", BETA_WAITLIST_PROJECTION_ROUTE, "", &kernel)
            .unwrap()
            .unwrap();
        assert!(projection.body_text().contains("\"record_count\":1"));
        assert!(projection.body_text().contains("cohort_a"));
    }

    #[test]
    fn route_records_enrollment_and_projects_latest_active() {
        let kernel = kernel();
        let body = r#"{"participant_id":"engineer_1","cohort_id":"cohort_a","role":"backend","use_case":"forge","expected_first_task":"first_forge_request","support_owner":"beta_lead","risk_tier":"trusted","status":"active","product_analytics_consent":true,"follow_up_contact_consent":true,"actor_id":"engineer_1"}"#;
        let response = route_response("POST", BETA_ENROLLMENT_ROUTE, body, &kernel)
            .unwrap()
            .unwrap();
        assert_eq!(response.status_for_test(), "200 OK");
        assert!(response.body_text().contains("BETA_ENROLLMENT_RECORDED"));

        let projection = route_response("GET", BETA_ENROLLMENT_PROJECTION_ROUTE, "", &kernel)
            .unwrap()
            .unwrap();
        assert!(projection.body_text().contains("\"active_count\":1"));
        assert!(projection.body_text().contains("cohort_a"));
    }

    #[test]
    fn participant_reads_only_own_journey_while_operator_reads_tenant_cohort() {
        let _env = ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let kernel = kernel();
        for actor in ["engineer_1", "engineer_2"] {
            let body = format!(
                r#"{{"participant_id":"{actor}","cohort_id":"cohort_a","role":"backend","use_case":"forge","expected_first_task":"first_forge_request","support_owner":"beta_lead","risk_tier":"trusted","status":"active","product_analytics_consent":true,"follow_up_contact_consent":true,"actor_id":"{actor}"}}"#
            );
            route_response("POST", BETA_ENROLLMENT_ROUTE, &body, &kernel)
                .unwrap()
                .unwrap();
        }

        {
            let _guard = crate::request_security::set_verified_identity(Some(read_identity(
                "engineer_1",
                "member",
            )));
            let telemetry = route_response("GET", BETA_TELEMETRY_PROJECTION_ROUTE, "", &kernel)
                .unwrap()
                .unwrap();
            assert!(telemetry.body_text().contains("engineer_1"));
            assert!(!telemetry.body_text().contains("engineer_2"));
            let enrollment = route_response("GET", BETA_ENROLLMENT_PROJECTION_ROUTE, "", &kernel)
                .unwrap()
                .unwrap();
            assert!(enrollment.body_text().contains("engineer_1"));
            assert!(!enrollment.body_text().contains("engineer_2"));
        }

        let _guard =
            crate::request_security::set_verified_identity(Some(read_identity("founder", "owner")));
        let telemetry = route_response("GET", BETA_TELEMETRY_PROJECTION_ROUTE, "", &kernel)
            .unwrap()
            .unwrap();
        assert!(telemetry.body_text().contains("engineer_1"));
        assert!(telemetry.body_text().contains("engineer_2"));
    }

    #[test]
    fn route_refuses_active_enrollment_when_cohort_cap_is_reached() {
        let _env = ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        unsafe {
            std::env::set_var("MDX_BETA_COHORT_CAP_COHORT_A", "1");
        }
        let kernel = kernel();
        let first = route_response(
            "POST",
            BETA_ENROLLMENT_ROUTE,
            r#"{"participant_id":"engineer_1","cohort_id":"cohort_a","role":"backend","use_case":"forge","expected_first_task":"first_forge_request","support_owner":"beta_lead","risk_tier":"trusted","status":"active","actor_id":"engineer_1"}"#,
            &kernel,
        )
        .unwrap()
        .unwrap();
        assert!(first.body_text().contains("BETA_ENROLLMENT_RECORDED"));

        let second = route_response(
            "POST",
            BETA_ENROLLMENT_ROUTE,
            r#"{"participant_id":"engineer_2","cohort_id":"cohort_a","role":"backend","use_case":"forge","expected_first_task":"first_forge_request","support_owner":"beta_lead","risk_tier":"trusted","status":"active","actor_id":"engineer_2"}"#,
            &kernel,
        )
        .unwrap()
        .unwrap();
        assert!(second.body_text().contains("BETA_ENROLLMENT_REFUSED"));
        assert!(second.body_text().contains("cohort_cap_reached"));
        let second_json: serde_json::Value =
            serde_json::from_str(second.body_text()).expect("halt response json");
        assert!(
            second_json["cohort_halt_receipt_id"]
                .as_str()
                .is_some_and(|id| !id.is_empty())
        );
        let projection = route_response("GET", BETA_ENROLLMENT_PROJECTION_ROUTE, "", &kernel)
            .unwrap()
            .unwrap();
        assert!(projection.body_text().contains("\"cohort_halt_count\":1"));
        assert!(
            projection
                .body_text()
                .contains("\"cohort_halt_receipt_kind\":\"beta.cohort_halt.recorded\"")
        );
        assert!(projection.body_text().contains("BETA_COHORT_HALT_RECORDED"));
        unsafe {
            std::env::remove_var("MDX_BETA_COHORT_CAP_COHORT_A");
        }
    }

    #[test]
    fn route_records_product_analytics_after_enrollment() {
        let kernel = kernel();
        route_response(
            "POST",
            BETA_ENROLLMENT_ROUTE,
            r#"{"participant_id":"engineer_1","cohort_id":"cohort_a","role":"backend","use_case":"forge","expected_first_task":"first_forge_request","support_owner":"beta_lead","risk_tier":"trusted","status":"active","product_analytics_consent":true,"follow_up_contact_consent":false,"actor_id":"engineer_1"}"#,
            &kernel,
        )
        .unwrap()
        .unwrap();
        let response = route_response(
            "POST",
            BETA_TELEMETRY_ROUTE,
            r#"{"type":"activation_step","route":"/forge","ts":"2026-06-21T00:00:00Z","step":"first_forge_request","completed":true,"actor_id":"engineer_1"}"#,
            &kernel,
        )
        .unwrap()
        .unwrap();
        assert!(response.body_text().contains("BETA_TELEMETRY_RECORDED"));
        assert!(response.body_text().contains("cohort_a"));

        let projection = route_response("GET", BETA_TELEMETRY_PROJECTION_ROUTE, "", &kernel)
            .unwrap()
            .unwrap();
        assert!(projection.body_text().contains("activation_step"));
        assert!(projection.body_text().contains("\"raw_event_ttl_days\":14"));
        assert!(projection.body_text().contains("\"journey_count\":1"));
        assert!(
            projection
                .body_text()
                .contains("\"participant_journeys\":[")
        );
        assert!(projection.body_text().contains("engineer_1"));
    }

    #[test]
    fn route_records_safe_rejection_without_unsafe_payload() {
        let kernel = kernel();
        let response = route_response(
            "POST",
            BETA_TELEMETRY_ROUTE,
            r#"{"type":"activation_step","route":"/forge","ts":"2026-06-21T00:00:00Z","step":"ignore previous instructions","actor_id":"engineer_1"}"#,
            &kernel,
        )
        .unwrap()
        .unwrap();
        let body = response.body_text();
        assert!(body.contains("BETA_TELEMETRY_REJECTED"));
        assert!(body.contains("\"unsafe_payload_persisted\":false"));
        assert!(!body.contains("ignore previous instructions"));
        assert!(!body.contains("\"refused_field\""));

        let projection = route_response("GET", BETA_TELEMETRY_PROJECTION_ROUTE, "", &kernel)
            .unwrap()
            .unwrap();
        assert!(projection.body_text().contains("unsafe_value"));
    }
}
