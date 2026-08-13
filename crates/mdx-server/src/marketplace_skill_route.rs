// The Marketplace skill library routes: draft a distilled skill from a
// verified Forge run, ratify a draft with a distinct actor, and read the
// drafted and ratified distilled skills. Drafting and ratifying are governed
// writes; the projections fold the ledger. No route here opens execution,
// secrets, inherited permissions, adaptation, or a production write.
use crate::RouteResponse;
use mdx_core::{
    LearningMemoryApplicability, MarketplaceSkillDraft, MarketplaceSkillRatification, MdxKernel,
    json_string_literal,
};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/marketplace/skill-drafts.json" => Some(handle_draft(method, body, kernel)),
        "/marketplace/skill-drafts/projection.json" => {
            Some(handle_drafts_projection(method, kernel))
        }
        "/marketplace/skill-ratifications.json" => Some(handle_ratify(method, body, kernel)),
        "/marketplace/distilled-skills/projection.json" => {
            Some(handle_distilled_projection(method, kernel))
        }
        _ => None,
    }
}

fn handle_draft(
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
    let work_tiers =
        json_string_or_string_array_field(body, "applicability_work_tiers").unwrap_or_default();
    let language_packs =
        json_string_or_string_array_field(body, "applicability_language_packs").unwrap_or_default();
    let notes = field("applicability_notes");
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let report = match kernel.draft_marketplace_skill_with_applicability(
        MarketplaceSkillDraft {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            source_run_receipt_id: &field("source_run_receipt_id"),
            skill_id: &field("skill_id"),
            skill_name: &field("skill_name"),
            summary: &field("summary"),
            helps_with: &field("helps_with"),
            model_procedure: &field("model_procedure"),
            procedure_source: &field("procedure_source"),
            sensitivity: &field("sensitivity"),
        },
        LearningMemoryApplicability {
            work_tiers: &work_tiers,
            language_packs: &language_packs,
            notes: &notes,
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => return Ok(refusal("skill-draft", &error.message())),
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-marketplace-skill-draft-local-post","status":"DRAFTED","auth_session_status":{},"skill_id":{},"skill_state":{},"source_run_id":{},"procedure_source":{},"procedure_step_count":{},"skill_draft_receipt_id":{},"policy_decision_id":{},"projection_route":"/marketplace/skill-drafts/projection.json","capability_execution_allowed":false,"adaptation_allowed":false,"production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&report.skill_id),
            json_string_literal(&report.skill_state),
            json_string_literal(&report.source_run_id),
            json_string_literal(&report.procedure_source),
            report.procedure_step_count,
            json_string_literal(&report.receipt_id),
            json_string_literal(&report.policy_decision_id),
        ),
    ))
}

fn handle_ratify(
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
    let report = match kernel.ratify_marketplace_skill_with_identity(
        MarketplaceSkillRatification {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            skill_id: &field("skill_id"),
            draft_receipt_id: &field("draft_receipt_id"),
            decision: &field("decision"),
            note: &field("note"),
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => return Ok(refusal("skill-ratification", &error.message())),
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-marketplace-skill-ratification-local-post","status":"RATIFIED","auth_session_status":{},"skill_id":{},"capability_state":{},"drafted_by":{},"ratified_by":{},"skill_ratification_receipt_id":{},"policy_decision_id":{},"projection_route":"/marketplace/distilled-skills/projection.json","capability_execution_allowed":false,"adaptation_allowed":false,"production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&report.skill_id),
            json_string_literal(&report.capability_state),
            json_string_literal(&report.drafted_by),
            json_string_literal(&report.ratified_by),
            json_string_literal(&report.receipt_id),
            json_string_literal(&report.policy_decision_id),
        ),
    ))
}

fn handle_drafts_projection(
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
        .distilled_skill_views()
        .into_iter()
        .filter(|view| view.state == "drafted")
        .map(|view| render_skill_view(&view))
        .collect();
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-marketplace-skill-drafts-local-projection","receipt_kind":"marketplace.skill.drafted","writes_route":"/marketplace/skill-drafts.json","ratify_route":"/marketplace/skill-ratifications.json","draft_count":{},"drafts":[{}],"capability_execution_allowed":false,"adaptation_allowed":false,"production_write_allowed":false}}"#,
            drafts.len(),
            drafts.join(","),
        ),
    ))
}

fn handle_distilled_projection(
    method: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "GET") {
        return Ok(response);
    }
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let views = kernel.distilled_skill_views();
    let ratified: Vec<String> = views
        .iter()
        .filter(|view| view.state == "ratified")
        .map(render_skill_view)
        .collect();
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-marketplace-distilled-skills-local-projection","receipt_kind":"marketplace.skill.ratified","origin":"distilled_skill","provenance_rule":"a ratified skill cites a drafted skill cites a proven-green source run","ratified_count":{},"distilled_skills":[{}],"capability_execution_allowed":false,"adaptation_allowed":false,"production_write_allowed":false}}"#,
            ratified.len(),
            ratified.join(","),
        ),
    ))
}

fn render_skill_view(view: &mdx_core::DistilledSkillView) -> String {
    format!(
        r#"{{"skill_id":{},"skill_name":{},"summary":{},"helps_with":{},"procedure":{},"procedure_source":{},"procedure_step_count":{},"sensitivity":{},"boundary_locked":{},"applicability_work_tiers":{},"applicability_language_packs":{},"origin":"distilled_skill","state":{},"source_run_id":{},"source_run_receipt_id":{},"source_check_passed_receipt_id":{},"draft_receipt_id":{},"ratification_receipt_id":{},"drafted_by":{},"ratified_by":{}}}"#,
        json_string_literal(&view.skill_id),
        json_string_literal(&view.skill_name),
        json_string_literal(&view.summary),
        json_string_literal(&view.helps_with),
        json_string_literal(&view.procedure),
        json_string_literal(&view.procedure_source),
        view.procedure_step_count,
        json_string_literal(&view.sensitivity),
        view.boundary_locked,
        json_string_literal(&view.applicability_work_tiers),
        json_string_literal(&view.applicability_language_packs),
        json_string_literal(&view.state),
        json_string_literal(&view.source_run_id),
        json_string_literal(&view.source_run_receipt_id),
        json_string_literal(&view.source_check_passed_receipt_id),
        json_string_literal(&view.draft_receipt_id),
        json_string_literal(&view.ratification_receipt_id),
        json_string_literal(&view.drafted_by),
        json_string_literal(&view.ratified_by),
    )
}

fn refusal(surface: &str, reason: &str) -> RouteResponse {
    // A refused write cites no receipt because none was recorded; the empty
    // receipt field says so honestly and keeps the receipt-backed route
    // contract truthful (a success cites a real one, a refusal an empty one),
    // matching the learning governed-write refusals.
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-marketplace-{}-local-post","status":"REFUSED","reason":{},"skill_receipt_id":"","capability_execution_allowed":false,"adaptation_allowed":false,"production_write_allowed":false}}"#,
            surface,
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

fn json_string_or_string_array_field(body: &str, key: &str) -> Option<String> {
    if let Some(value) = json_string_field(body, key) {
        return Some(value);
    }
    let marker = format!("\"{key}\":");
    let after = body.split(&marker).nth(1)?;
    let after = after.trim_start();
    let rest = after.strip_prefix('[')?;
    let mut values = Vec::new();
    let mut chars = rest.chars().peekable();
    loop {
        while matches!(chars.peek(), Some(c) if c.is_whitespace() || *c == ',') {
            chars.next();
        }
        match chars.peek() {
            Some(']') => return Some(values.join(", ")),
            Some('"') => {
                chars.next();
                let mut value = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => {
                            if let Some(escaped) = chars.next() {
                                value.push(escaped);
                            }
                        }
                        '"' => {
                            values.push(value);
                            break;
                        }
                        other => value.push(other),
                    }
                }
            }
            _ => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdx_core::ForgeRunEvent;

    fn green_run(kernel: &Arc<RwLock<MdxKernel>>, run_id: &str) -> String {
        let mut guard = kernel.write().expect("kernel");
        for (event, detail) in [
            ("run_started", "run started"),
            ("tool_executed", "edit_file src/lib.rs"),
            ("check_passed", "run_command cargo test exit=0 tail=ok"),
        ] {
            guard
                .record_forge_run_event(ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "human:builder",
                    run_id,
                    event,
                    work_item_id: "",
                    detail,
                    turn: 1,
                    input_tokens: 0,
                    output_tokens: 0,
                })
                .expect("event");
        }
        guard
            .record_forge_run_event(ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "human:builder",
                run_id,
                event: "run_finished",
                work_item_id: "",
                detail: "status=RUN_FINISHED_DONE turns=1 files_changed=1 check_runs=1",
                turn: 1,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("finished")
            .receipt_id
    }

    #[test]
    fn draft_ratify_and_projection_round_trip() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let run_receipt = green_run(&kernel, "run_route_alpha");
        let draft_body = format!(
            r#"{{"actor_id":"human:builder","source_run_receipt_id":"{run_receipt}","skill_id":"rust_repair_v1","skill_name":"Rust repair","summary":"Repair a failing test.","helps_with":"backend runs","applicability_work_tiers":["medium"],"applicability_language_packs":["rust-cargo"]}}"#
        );
        let draft = route_response(
            "POST",
            "/marketplace/skill-drafts.json",
            &draft_body,
            &kernel,
        )
        .expect("route")
        .expect("response");
        assert!(draft.body.contains("\"status\":\"DRAFTED\""));

        let drafts = route_response(
            "GET",
            "/marketplace/skill-drafts/projection.json",
            "",
            &kernel,
        )
        .expect("route")
        .expect("response");
        assert!(drafts.body.contains("\"draft_count\":1"));
        let draft_receipt = json_string_field(&draft.body, "skill_draft_receipt_id").unwrap();

        // A distinct actor ratifies.
        let ratify_body = format!(
            r#"{{"actor_id":"human:reviewer","skill_id":"rust_repair_v1","draft_receipt_id":"{draft_receipt}","decision":"ratify","note":"matches a real repair"}}"#
        );
        let ratify = route_response(
            "POST",
            "/marketplace/skill-ratifications.json",
            &ratify_body,
            &kernel,
        )
        .expect("route")
        .expect("response");
        assert!(ratify.body.contains("\"status\":\"RATIFIED\""));
        assert!(ratify.body.contains("\"capability_state\":\"ratified\""));

        let distilled = route_response(
            "GET",
            "/marketplace/distilled-skills/projection.json",
            "",
            &kernel,
        )
        .expect("route")
        .expect("response");
        assert!(distilled.body.contains("\"ratified_count\":1"));
        assert!(distilled.body.contains("\"origin\":\"distilled_skill\""));
        assert!(distilled.body.contains("run_route_alpha"));
    }

    #[test]
    fn draft_refuses_a_run_that_is_not_proven_green() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let not_green = {
            let mut guard = kernel.write().expect("kernel");
            guard
                .record_forge_run_event(ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "human:builder",
                    run_id: "run_route_red",
                    event: "run_finished",
                    work_item_id: "",
                    detail: "status=RUN_FINISHED_CANNOT_PROCEED turns=3 files_changed=0",
                    turn: 3,
                    input_tokens: 0,
                    output_tokens: 0,
                })
                .expect("finished")
                .receipt_id
        };
        let body = format!(
            r#"{{"actor_id":"human:builder","source_run_receipt_id":"{not_green}","skill_id":"bad","skill_name":"Bad","summary":"nope","helps_with":"nope"}}"#
        );
        let draft = route_response("POST", "/marketplace/skill-drafts.json", &body, &kernel)
            .expect("route")
            .expect("response");
        assert!(draft.body.contains("\"status\":\"REFUSED\""));
        assert!(draft.body.contains("not a proven-green"));
    }

    #[test]
    fn ratify_refuses_self_ratification() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let run_receipt = green_run(&kernel, "run_route_self");
        let draft_body = format!(
            r#"{{"actor_id":"human:builder","source_run_receipt_id":"{run_receipt}","skill_id":"self_v1","skill_name":"Self","summary":"x","helps_with":"y"}}"#
        );
        let draft = route_response(
            "POST",
            "/marketplace/skill-drafts.json",
            &draft_body,
            &kernel,
        )
        .expect("route")
        .expect("response");
        let draft_receipt = json_string_field(&draft.body, "skill_draft_receipt_id").unwrap();
        let ratify_body = format!(
            r#"{{"actor_id":"human:builder","skill_id":"self_v1","draft_receipt_id":"{draft_receipt}","decision":"ratify","note":""}}"#
        );
        let ratify = route_response(
            "POST",
            "/marketplace/skill-ratifications.json",
            &ratify_body,
            &kernel,
        )
        .expect("route")
        .expect("response");
        assert!(ratify.body.contains("\"status\":\"REFUSED\""));
        assert!(ratify.body.contains("distinct actor"));
    }
}
