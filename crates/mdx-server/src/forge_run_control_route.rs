// Operator controls for a running Forge run over HTTP: stop it cleanly, or
// steer it with a note the agent picks up on its next turn. Governed: a
// control is a person's verb, recorded with their identity. The running
// loop reads these receipts between turns. A stop also signals the local
// process supervisor as a best-effort fast path; the receipt remains the
// durable authority when no worker is live in this server process.
use crate::RouteResponse;
use mdx_core::{ForgeRunControl, MdxKernel, json_string_literal};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/forge/run-controls.json" => Some(handle_post(method, body, kernel)),
        "/forge/run-controls/projection.json" => Some(handle_projection(method, kernel)),
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
    let run_id = field("run_id");
    let control = field("control");
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let report = match kernel.record_forge_run_control_with_identity(
        ForgeRunControl {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            run_id: &run_id,
            control: &control,
            note: &field("note"),
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => return Ok(refusal(&error.message())),
    };
    drop(kernel);
    if report.control == "stop" {
        crate::process_supervisor::cancel_run(&report.run_id);
    }
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-run-control-local-post","status":"RECORDED","auth_session_status":{},"run_id":{},"control":{},"control_receipt_id":{},"policy_decision_id":{},"projection_route":"/forge/run-controls/projection.json","production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&report.run_id),
            json_string_literal(&report.control),
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
    let controls: Vec<String> = kernel
        .ledger()
        .query()
        .by_kind("forge.run.control")
        .iter()
        .map(|receipt| {
            let value = |key: &str| {
                json_string_literal(receipt.payload.get(key).map(String::as_str).unwrap_or(""))
            };
            format!(
                r#"{{"control_receipt_id":{},"run_id":{},"control":{},"note":{},"actor_id":{}}}"#,
                json_string_literal(&receipt.receipt_id),
                value("run_id"),
                value("control"),
                value("note"),
                json_string_literal(receipt.actor_id.as_str()),
            )
        })
        .collect();
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-run-control-local-projection","receipt_kind":"forge.run.control","control_count":{},"controls":[{}],"production_write_allowed":false}}"#,
            controls.len(),
            controls.join(","),
        ),
    ))
}

fn refusal(reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-run-control-local-post","status":"REFUSED","reason":{},"control_receipt_id":"","production_write_allowed":false}}"#,
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

    #[test]
    fn recorded_stop_signals_an_active_native_run() {
        let run_id = "forge_route_active_stop";
        let _guard = crate::process_supervisor::register_run_cancellation(run_id);
        let cancellation =
            crate::process_supervisor::cancellation_for_run(run_id).expect("active run token");
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let response = handle_post(
            "POST",
            &format!(
                r#"{{"run_id":"{run_id}","control":"stop","note":"operator requested stop"}}"#
            ),
            &kernel,
        )
        .expect("route response");

        assert!(response.body.contains(r#""status":"RECORDED""#));
        assert!(cancellation.is_cancelled());
        assert_eq!(kernel.read().unwrap().forge_run_controls(run_id).len(), 1);
    }
}
