// Fixture graduation drafting turns a triaged finding into a proposed golden
// fixture for the Machine League regression floor. This governed write is the
// DRAFT step only: it verifies the source finding receipt in the ledger,
// refuses a duplicate draft of the same fixture id, and records the proposed
// fixture id, scenario, expected behavior, and family. It writes no registry
// entry and opens no allowlist. GRADUATION - the human-curated write that adds
// the fixture to the versioned registry and makes it a regression floor - is
// the sim program owner's lane and is not served here.
use crate::RouteResponse;
use mdx_core::{FixtureGraduationDraft, MdxKernel, json_string_literal};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/learning/fixture-graduations.json" => Some(handle_post(method, body, kernel)),
        "/learning/fixture-graduations/projection.json" => Some(handle_projection(method, kernel)),
        _ => None,
    }
}

fn handle_post(
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
    let report = match kernel.draft_fixture_graduation_with_identity(
        FixtureGraduationDraft {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            source_receipt_id: &field("source_receipt_id"),
            fixture_id: &field("fixture_id"),
            scenario_description: &field("scenario_description"),
            expected_behavior: &field("expected_behavior"),
            proposed_fixture_family: &field("proposed_fixture_family"),
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => return Ok(refusal(&error.message())),
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-fixture-graduation-draft-local-post","status":"DRAFTED","auth_session_status":{},"fixture_graduation_id":{},"fixture_id":{},"source_receipt_id":{},"lane_state":{},"fixture_graduation_receipt_id":{},"policy_decision_id":{},"projection_route":"/learning/fixture-graduations/projection.json","reserved_graduated_kind":"fixture.graduation.graduated","graduation_write_allowed":false,"registry_write_allowed":false,"allowlist_change_allowed":false,"memory_write_allowed":false,"adaptation_allowed":false,"runtime_behavior_change_allowed":false,"production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&report.fixture_graduation_id),
            json_string_literal(&report.fixture_id),
            json_string_literal(&report.source_receipt_id),
            json_string_literal(&report.lane_state),
            json_string_literal(&report.receipt_id),
            json_string_literal(&report.policy_decision_id),
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
    let drafts: Vec<String> = kernel
        .ledger()
        .query()
        .by_kind("fixture.graduation.drafted")
        .iter()
        .map(|receipt| {
            let value = |key: &str| {
                json_string_literal(receipt.payload.get(key).map(String::as_str).unwrap_or(""))
            };
            format!(
                r#"{{"fixture_graduation_receipt_id":{},"fixture_graduation_id":{},"fixture_id":{},"source_receipt_id":{},"source_receipt_kind":{},"scenario_description":{},"expected_behavior":{},"proposed_fixture_family":{},"proposed_origin":{},"actor_id":{},"lane_state":{},"reserved_graduated_kind":{},"graduation_write_allowed":false,"registry_write_allowed":false,"allowlist_change_allowed":false,"memory_write_allowed":false,"adaptation_allowed":false,"runtime_behavior_change_allowed":false}}"#,
                json_string_literal(&receipt.receipt_id),
                value("fixture_graduation_id"),
                value("fixture_id"),
                value("source_receipt_id"),
                value("source_receipt_kind"),
                value("scenario_description"),
                value("expected_behavior"),
                value("proposed_fixture_family"),
                value("proposed_origin"),
                json_string_literal(receipt.actor_id.as_str()),
                value("lane_state"),
                value("reserved_graduated_kind"),
            )
        })
        .collect();
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-fixture-graduation-draft-local-projection","receipt_kind":"fixture.graduation.drafted","reserved_graduated_kind":"fixture.graduation.graduated","writes_route":"/learning/fixture-graduations.json","registry_path":"generated/eval-suites/machine-league-fixture-registry.json","draft_count":{},"drafts":[{}],"graduation_write_allowed":false,"registry_write_allowed":false,"allowlist_change_allowed":false,"memory_write_allowed":false,"adaptation_allowed":false,"runtime_behavior_change_allowed":false,"production_write_allowed":false}}"#,
            drafts.len(),
            drafts.join(","),
        ),
    ))
}

fn refusal(reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-fixture-graduation-draft-local-post","status":"REFUSED","reason":{},"fixture_graduation_receipt_id":"","graduation_write_allowed":false,"registry_write_allowed":false,"allowlist_change_allowed":false,"memory_write_allowed":false,"adaptation_allowed":false,"runtime_behavior_change_allowed":false,"production_write_allowed":false}}"#,
            json_string_literal(reason)
        ),
    )
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

#[cfg(test)]
mod tests {
    use super::*;
    use mdx_core::BetaFeedbackCapture;

    fn finding_receipt_id(kernel: &Arc<RwLock<MdxKernel>>) -> String {
        let mut kernel = kernel.write().expect("kernel");
        kernel
            .record_beta_feedback_local(&BetaFeedbackCapture {
                surface: "forge",
                route: "/forge",
                tenant_id: "local_tenant",
                actor_id: "human:beta",
                session_ref: "session_1",
                occurred_at: "2026-07-03T00:00:00Z",
                category: "bug",
                context: &[],
                note: "A no_change run inherited a misdiagnosed fix location.",
            })
            .expect("feedback")
            .feedback_receipt_id
    }

    #[test]
    fn post_drafts_graduation_and_projection_lists_it() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let source_receipt_id = finding_receipt_id(&kernel);
        let post = route_response(
            "POST",
            "/learning/fixture-graduations.json",
            &format!(
                r#"{{"source_receipt_id":"{source_receipt_id}","fixture_id":"rust_compose_seam_premise","scenario_description":"A self-fix run inherited a wrong fix location.","expected_behavior":"The compose seam verifies the fix location first.","proposed_fixture_family":"rust_compose_seam"}}"#
            ),
            &kernel,
        )
        .expect("fixture graduation route")
        .expect("fixture graduation response");
        assert_eq!(post.status, "200 OK");
        assert!(post.body.contains("\"status\":\"DRAFTED\""));
        assert!(post.body.contains("\"lane_state\":\"drafted\""));
        assert!(post.body.contains("\"graduation_write_allowed\":false"));
        assert!(post.body.contains("\"registry_write_allowed\":false"));
        assert!(
            post.body
                .contains("\"reserved_graduated_kind\":\"fixture.graduation.graduated\"")
        );

        let projection = route_response(
            "GET",
            "/learning/fixture-graduations/projection.json",
            "",
            &kernel,
        )
        .expect("fixture graduation projection route")
        .expect("fixture graduation projection response");
        assert!(projection.body.contains("\"draft_count\":1"));
        assert!(
            projection
                .body
                .contains("\"receipt_kind\":\"fixture.graduation.drafted\"")
        );
        assert!(projection.body.contains("rust_compose_seam_premise"));
        assert!(
            projection
                .body
                .contains(&format!("\"source_receipt_id\":\"{source_receipt_id}\""))
        );
    }

    #[test]
    fn post_refuses_unknown_and_wrong_kind_source() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let unknown = route_response(
            "POST",
            "/learning/fixture-graduations.json",
            r#"{"source_receipt_id":"receipt_that_does_not_exist","fixture_id":"rust_missing","scenario_description":"No source.","expected_behavior":"Refuse.","proposed_fixture_family":"rust_compose_seam"}"#,
            &kernel,
        )
        .expect("fixture graduation route")
        .expect("fixture graduation response");
        assert!(unknown.body.contains("\"status\":\"REFUSED\""));
        assert!(unknown.body.contains("is unknown"));

        // The draft receipt itself is not a valid finding source: a second
        // draft that cites it must refuse on the wrong-kind check.
        let source_receipt_id = finding_receipt_id(&kernel);
        let first = route_response(
            "POST",
            "/learning/fixture-graduations.json",
            &format!(
                r#"{{"source_receipt_id":"{source_receipt_id}","fixture_id":"rust_first","scenario_description":"A first scenario.","expected_behavior":"Keeps passing.","proposed_fixture_family":"rust_compose_seam"}}"#
            ),
            &kernel,
        )
        .expect("fixture graduation route")
        .expect("fixture graduation response");
        assert!(first.body.contains("\"status\":\"DRAFTED\""));
        let draft_receipt_id = {
            let kernel = kernel.read().expect("kernel");
            kernel
                .ledger()
                .query()
                .by_kind("fixture.graduation.drafted")
                .iter()
                .last()
                .expect("draft receipt")
                .receipt_id
                .clone()
        };
        let wrong_kind = route_response(
            "POST",
            "/learning/fixture-graduations.json",
            &format!(
                r#"{{"source_receipt_id":"{draft_receipt_id}","fixture_id":"rust_second","scenario_description":"A draft receipt is not a finding.","expected_behavior":"Refuse.","proposed_fixture_family":"rust_compose_seam"}}"#
            ),
            &kernel,
        )
        .expect("fixture graduation route")
        .expect("fixture graduation response");
        assert!(wrong_kind.body.contains("\"status\":\"REFUSED\""));
        assert!(wrong_kind.body.contains("expected one of"));
    }
}
