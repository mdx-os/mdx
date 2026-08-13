use crate::RouteResponse;
use crate::secret_store::global;
use crate::studio::json_string_field;
use mdx_core::{ChooseSetupTrack, MdxKernel, RecordFirstRunProfile, json_string_literal};
use std::sync::{Arc, RwLock};

// HTTP for the guided "Try MDx" start. POST records the first-run profile (role,
// goal, working mode, local-posture acknowledgement) as a governed write; GET
// reports it so the flow knows whether the guided start has been done. This is
// NOT account/auth: it records a few facts to seed the first Twin conversation,
// nothing more. The rich personalization lives in the "You" surface.
pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path == "/install/first-run-profiles.json" {
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
        return Some(record(body, &mut kernel).map(|body| RouteResponse::json("200 OK", body)));
    }
    if path == "/install/first-run-profile.json" {
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
        return Some(Ok(RouteResponse::json("200 OK", projection(&kernel))));
    }
    None
}

fn identity(body: &str) -> (String, String) {
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    if resolved.auth_session_status == "VERIFIED_TRUSTED_SESSION" {
        (resolved.tenant_id.clone(), resolved.actor_id.clone())
    } else {
        (
            json_string_field(body, "tenant_id").unwrap_or_else(|| "local_tenant".to_string()),
            json_string_field(body, "actor_id").unwrap_or_else(|| "human:local_user".to_string()),
        )
    }
}

// A truthy acknowledgement, accepting either a JSON bool or the string "true"
// (the compact JSON the client sends carries a real bool).
fn json_truthy_field(body: &str, key: &str) -> bool {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return false;
    };
    match value.get(key) {
        Some(serde_json::Value::Bool(true)) => true,
        Some(serde_json::Value::String(value)) => value == "true",
        _ => false,
    }
}

fn record(body: &str, kernel: &mut MdxKernel) -> Result<String, String> {
    let (tenant_id, actor_id) = identity(body);
    let role = json_string_field(body, "role").unwrap_or_default();
    let goal = json_string_field(body, "goal").unwrap_or_default();
    let org_context = json_string_field(body, "org_context").unwrap_or_default();
    let working_mode = json_string_field(body, "working_mode").unwrap_or_default();
    let privacy_ack = json_truthy_field(body, "privacy_ack");
    let (setup_complete, owner_claimed, model_connected) =
        crate::install_setup_track::setup_activation_state(kernel, global());
    if !setup_complete {
        let report = kernel
            .refuse_setup_track_pre_activation(
                ChooseSetupTrack {
                    tenant_id: &tenant_id,
                    actor_id: &actor_id,
                    track: "try",
                },
                "owner and usable model must be established before the guided start",
                owner_claimed,
                model_connected,
            )
            .map_err(|error| error.message())?;
        return Ok(format!(
            r#"{{"name":"mdx-install-first-run-profile-local-post","status":"FIRST_RUN_PROFILE_REFUSED","recorded":false,"reason":"owner and usable model must be established before the guided start","receipt_id":{},"production_write_allowed":false}}"#,
            json_string_literal(&report.receipt_id)
        ));
    }
    // A bad start (missing fields, unknown role/mode, no acknowledgement, e.g. an
    // empty smoke body) is refused gracefully as a 200 with a reason.
    let report = match kernel.record_first_run_profile(RecordFirstRunProfile {
        tenant_id: &tenant_id,
        actor_id: &actor_id,
        role: &role,
        goal: &goal,
        org_context: &org_context,
        working_mode: &working_mode,
        privacy_ack,
    }) {
        Ok(report) => report,
        Err(error) => {
            return Ok(format!(
                r#"{{"name":"mdx-install-first-run-profile-local-post","status":"FIRST_RUN_PROFILE_REFUSED","recorded":false,"reason":{},"production_write_allowed":false}}"#,
                json_string_literal(&error.message())
            ));
        }
    };
    let chosen = kernel.chosen_setup_track_local();
    if chosen.track != "try" {
        kernel
            .choose_setup_track(ChooseSetupTrack {
                tenant_id: &tenant_id,
                actor_id: &actor_id,
                track: "try",
            })
            .map_err(|error| error.message())?;
    }
    Ok(format!(
        r#"{{"name":"mdx-install-first-run-profile-local-post","status":{},"recorded":true,"role":{},"receipt_id":{},"projection_route":"/install/first-run-profile.json","production_write_allowed":false}}"#,
        json_string_literal(report.status),
        json_string_literal(&report.role),
        json_string_literal(&report.receipt_id),
    ))
}

fn projection(kernel: &MdxKernel) -> String {
    let state = kernel.first_run_profile_local();
    format!(
        r#"{{"name":"mdx-install-first-run-profile","status":"OK","read_only":true,"writes_route":"/install/first-run-profiles.json","recorded":{},"role":{},"goal":{},"working_mode":{},"receipt_id":{},"production_write_allowed":false}}"#,
        state.recorded,
        json_string_literal(&state.role),
        json_string_literal(&state.goal),
        json_string_literal(&state.working_mode),
        json_string_literal(&state.receipt_id),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdx_core::{
        ClaimInstallOwner, SETUP_TRACK_CHOSEN_RECEIPT_KIND, TwinModelGatewayProviderObservation,
        twin_model_gateway_provider_slots,
    };

    fn activate(kernel: &mut MdxKernel) {
        kernel
            .claim_install_owner(ClaimInstallOwner {
                tenant_id: "tenant_local",
                actor_id: "human:test",
                owner_name: "Dana",
            })
            .expect("owner");
        let slot = twin_model_gateway_provider_slots()
            .iter()
            .find(|slot| slot.provider_id == "xai")
            .expect("xai slot");
        kernel
            .save_twin_model_gateway_provider_observation_local(
                TwinModelGatewayProviderObservation {
                    tenant_id: "tenant_local",
                    actor_id: "human:test",
                    provider_id: slot.provider_id,
                    adapter: slot.adapter,
                    receipt_kind: slot.required_receipt_kind,
                    approval_receipt_id: "approval_test",
                    evidence_file: "test",
                    model_id: "grok-test",
                    response_id: "response_test",
                    response_status: "completed",
                    observed: true,
                    provider_call_attempted: true,
                    network_call_attempted: true,
                    credential_presence_only: true,
                    credential_values_recorded: false,
                    provider_secret_values_recorded: false,
                    requested_secret_values_recorded: false,
                    output_text_recorded: false,
                    production_write_allowed: false,
                    total_tokens: 7,
                },
            )
            .expect("observation");
        global().set_for_tenant("local_tenant", slot.env_key, "sk-test");
    }

    #[test]
    fn direct_try_profile_records_the_try_track_too() {
        let _guard = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let mut kernel = MdxKernel::boot_local();
        activate(&mut kernel);
        let response = record(
            r#"{"tenant_id":"tenant_local","role":"leadership","goal":"Help me evaluate MDx.","working_mode":"deciding","privacy_ack":true,"actor_id":"human:test"}"#,
            &mut kernel,
        )
        .expect("profile");
        assert!(response.contains("\"recorded\":true"), "{response}");
        assert_eq!(kernel.chosen_setup_track_local().track, "try");
        assert!(
            kernel
                .ledger()
                .entries()
                .iter()
                .any(|receipt| receipt.kind == SETUP_TRACK_CHOSEN_RECEIPT_KIND)
        );
        global().clear_tenant("local_tenant");
    }

    #[test]
    fn invalid_try_profile_does_not_choose_a_track() {
        let _guard = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let mut kernel = MdxKernel::boot_local();
        activate(&mut kernel);
        let response = record(
            r#"{"tenant_id":"tenant_local","role":"leadership","goal":"Help me evaluate MDx.","working_mode":"deciding","privacy_ack":false,"actor_id":"human:test"}"#,
            &mut kernel,
        )
        .expect("profile");
        assert!(response.contains("\"recorded\":false"), "{response}");
        assert!(!kernel.chosen_setup_track_local().chosen);
        global().clear_tenant("local_tenant");
    }
}
