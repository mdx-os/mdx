use crate::RouteResponse;
use crate::studio::{json_string_field, source_receipt_id};
use mdx_core::{
    MdxKernel, StudioControl, StudioLanding, StudioOpenRun, StudioPresenceMark, StudioSteering,
    json_string_literal,
};
use std::sync::{Arc, RwLock};

// HTTP control plane for mid-flight Studio steering, routing
// docs/STUDIO-RUNTIME-STEERING-CONTRACT.md. The contract declares pause,
// steering, and resume per run; open and land complete the lifecycle so the UI
// can create a held run and finish it. These ride the parameterized
// governed-write pattern (run_id in the path), like /dxr/forge-runs/{run_id}.
pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    let tail = path.strip_prefix("/studio/runs/")?;
    let (run_id, leaf) = tail.split_once('/')?;
    if run_id.is_empty()
        || !matches!(
            leaf,
            "open.json"
                | "pause.json"
                | "steering.json"
                | "resume.json"
                | "land.json"
                | "presence.json"
        )
    {
        return None;
    }
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
    let result = match leaf {
        "open.json" => open(run_id, body, &mut kernel),
        "pause.json" => pause(run_id, body, &mut kernel),
        "steering.json" => steer(run_id, body, &mut kernel),
        "resume.json" => resume(run_id, body, &mut kernel),
        "land.json" => land(run_id, body, &mut kernel),
        "presence.json" => presence(run_id, body, &mut kernel),
        _ => return None,
    };
    Some(result.map(|body| RouteResponse::json("200 OK", body)))
}

fn identity(body: &str) -> (String, String, String) {
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    let verified = resolved.auth_session_status == "VERIFIED_TRUSTED_SESSION";
    let tenant_id = if verified {
        resolved.tenant_id.clone()
    } else {
        json_string_field(body, "tenant_id").unwrap_or_else(|| "local_tenant".to_string())
    };
    let actor_id = if verified {
        resolved.actor_id.clone()
    } else {
        json_string_field(body, "actor_id").unwrap_or_else(|| "human:local_user".to_string())
    };
    let operator_id = json_string_field(body, "operator_id").unwrap_or_else(|| actor_id.clone());
    (tenant_id, actor_id, operator_id)
}

fn open(run_id: &str, body: &str, kernel: &mut MdxKernel) -> Result<String, String> {
    let source = match json_string_field(body, "source_receipt_id") {
        Some(value) if !value.trim().is_empty() => value,
        _ => source_receipt_id(kernel)?,
    };
    let goal = json_string_field(body, "goal")
        .unwrap_or_else(|| "Draft a document from grounded evidence".to_string());
    let artifact_kind = json_string_field(body, "artifact_kind").unwrap_or_default();
    let source_kind = json_string_field(body, "source_kind").unwrap_or_default();
    let source_title = json_string_field(body, "source_title").unwrap_or_default();
    let (tenant_id, actor_id, _) = identity(body);
    let report = kernel
        .open_studio_run(StudioOpenRun {
            tenant_id: &tenant_id,
            actor_id: &actor_id,
            run_id,
            goal: &goal,
            artifact_kind: &artifact_kind,
            source_receipt_id: &source,
            source_kind: &source_kind,
            source_title: &source_title,
        })
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-studio-run-open-local-post","status":{},"run_id":{},"state":{},"artifact_kind":{},"source_kind":{},"opened_receipt_id":{},"kind_selected_receipt_id":{},"writes_route":"/studio/runs/{{run_id}}/open.json","standalone_document_store_allowed":false,"live_co_edit_allowed":false,"production_write_allowed":false}}"#,
        json_string_literal(report.status),
        json_string_literal(&report.run_id),
        json_string_literal(report.state),
        json_string_literal(report.artifact_kind),
        json_string_literal(report.source_kind),
        json_string_literal(&report.opened_receipt_id),
        json_string_literal(&report.kind_selected_receipt_id),
    ))
}

fn pause(run_id: &str, body: &str, kernel: &mut MdxKernel) -> Result<String, String> {
    let (tenant_id, actor_id, operator_id) = identity(body);
    let key =
        json_string_field(body, "idempotency_key").unwrap_or_else(|| format!("pause_{run_id}"));
    let report = kernel
        .pause_studio_run(StudioControl {
            tenant_id: &tenant_id,
            actor_id: &actor_id,
            run_id,
            operator_id: &operator_id,
            idempotency_key: &key,
        })
        .map_err(|error| error.message())?;
    Ok(render_control("/studio/runs/{run_id}/pause.json", &report))
}

fn resume(run_id: &str, body: &str, kernel: &mut MdxKernel) -> Result<String, String> {
    let (tenant_id, actor_id, operator_id) = identity(body);
    let key =
        json_string_field(body, "idempotency_key").unwrap_or_else(|| format!("resume_{run_id}"));
    let report = kernel
        .resume_studio_run(StudioControl {
            tenant_id: &tenant_id,
            actor_id: &actor_id,
            run_id,
            operator_id: &operator_id,
            idempotency_key: &key,
        })
        .map_err(|error| error.message())?;
    Ok(render_control("/studio/runs/{run_id}/resume.json", &report))
}

fn steer(run_id: &str, body: &str, kernel: &mut MdxKernel) -> Result<String, String> {
    let (tenant_id, actor_id, operator_id) = identity(body);
    let key =
        json_string_field(body, "idempotency_key").unwrap_or_else(|| format!("steer_{run_id}"));
    let instruction =
        json_string_field(body, "instruction").unwrap_or_else(|| "Adjust the draft".to_string());
    let target_scope =
        json_string_field(body, "target_scope").unwrap_or_else(|| "document".to_string());
    let reason = json_string_field(body, "reason").unwrap_or_default();
    let source_refs = json_string_field(body, "source_refs").unwrap_or_default();
    let report = kernel
        .steer_studio_run(StudioSteering {
            tenant_id: &tenant_id,
            actor_id: &actor_id,
            run_id,
            operator_id: &operator_id,
            idempotency_key: &key,
            instruction: &instruction,
            target_scope: &target_scope,
            reason: &reason,
            source_refs: &source_refs,
        })
        .map_err(|error| error.message())?;
    Ok(render_control(
        "/studio/runs/{run_id}/steering.json",
        &report,
    ))
}

fn land(run_id: &str, body: &str, kernel: &mut MdxKernel) -> Result<String, String> {
    let (tenant_id, actor_id, operator_id) = identity(body);
    let key =
        json_string_field(body, "idempotency_key").unwrap_or_else(|| format!("land_{run_id}"));
    let document_id =
        json_string_field(body, "document_id").unwrap_or_else(|| "page_studio_steered".to_string());
    let title =
        json_string_field(body, "title").unwrap_or_else(|| "Studio Steered Draft".to_string());
    let revision_id =
        json_string_field(body, "revision_id").unwrap_or_else(|| "rev_studio_steered".to_string());
    let body_ref = match json_string_field(body, "body_text") {
        Some(text) => {
            crate::pages_body_store::store_published_body(&document_id, &revision_id, &text)
                .unwrap_or_else(|| format!("world_model://pages/{document_id}/body/{revision_id}"))
        }
        None => json_string_field(body, "body_ref")
            .unwrap_or_else(|| format!("world_model://pages/{document_id}/body/{revision_id}")),
    };
    let report = kernel
        .land_studio_run(StudioLanding {
            tenant_id: &tenant_id,
            actor_id: &actor_id,
            run_id,
            operator_id: &operator_id,
            idempotency_key: &key,
            document_id: &document_id,
            title: &title,
            body_ref: &body_ref,
            revision_id: &revision_id,
        })
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-studio-run-land-local-post","status":{},"run_id":{},"state":{},"document_id":{},"publication_receipt_id":{},"run_landed_receipt_id":{},"idempotent_replay":{},"refused":{},"refusal_reason":{},"pages_projection_route":"/pages/publications/projection.json","standalone_document_store_allowed":false,"live_co_edit_allowed":false,"production_write_allowed":false}}"#,
        json_string_literal(report.status),
        json_string_literal(&report.run_id),
        json_string_literal(report.state),
        json_string_literal(&report.document_id),
        json_string_literal(&report.publication_receipt_id),
        json_string_literal(&report.run_landed_receipt_id),
        report.idempotent_replay,
        report.refused,
        json_string_literal(&report.refusal_reason),
    ))
}

fn presence(run_id: &str, body: &str, kernel: &mut MdxKernel) -> Result<String, String> {
    let (tenant_id, actor_id, operator_id) = identity(body);
    let presence_scope = json_string_field(body, "presence_scope").unwrap_or_default();
    let report = kernel
        .mark_studio_presence(StudioPresenceMark {
            tenant_id: &tenant_id,
            actor_id: &actor_id,
            run_id,
            operator_id: &operator_id,
            presence_scope: &presence_scope,
        })
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-studio-run-presence-local-post","status":{},"run_id":{},"receipt_id":{},"present_count":{},"writes_route":"/studio/runs/{{run_id}}/presence.json","live_co_edit_allowed":false,"production_write_allowed":false}}"#,
        json_string_literal(report.status),
        json_string_literal(&report.run_id),
        json_string_literal(&report.receipt_id),
        report.present_count,
    ))
}

fn render_control(route: &str, report: &mdx_core::StudioControlReport) -> String {
    format!(
        r#"{{"name":"mdx-studio-run-control-local-post","status":{},"run_id":{},"state":{},"receipt_id":{},"idempotency_key":{},"idempotent_replay":{},"refused":{},"refusal_reason":{},"writes_route":{},"standalone_document_store_allowed":false,"live_co_edit_allowed":false,"production_write_allowed":false}}"#,
        json_string_literal(report.status),
        json_string_literal(&report.run_id),
        json_string_literal(report.state),
        json_string_literal(&report.receipt_id),
        json_string_literal(&report.idempotency_key),
        report.idempotent_replay,
        report.refused,
        json_string_literal(&report.refusal_reason),
        json_string_literal(route),
    )
}
