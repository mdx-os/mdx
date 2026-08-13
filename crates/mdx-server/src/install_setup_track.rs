use crate::RouteResponse;
use crate::secret_store::{SecretStore, global};
use crate::studio::json_string_field;
use mdx_core::{
    ChooseSetupTrack, MdxKernel, json_string_literal, twin_model_gateway_provider_slots,
};
use std::sync::{Arc, RwLock};

// HTTP for the activation router. POST records the track the operator chose
// (a governed write); GET serves the lean router projection the chooser screen
// renders. Activation owns the final setup-complete answer. This router exposes
// the narrower path-choice gate, track state, and one honest destination per
// track.
pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path == "/install/track-choices.json" {
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
            choose(body, &mut kernel, global()).map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/install/setup-router.json" {
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
            router_projection(&kernel, global()),
        )));
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

pub(crate) fn model_connected_for_setup(kernel: &MdxKernel, store: &SecretStore) -> bool {
    let tenant_id = crate::request_security::current_verified_identity()
        .map(|identity| identity.tenant_id)
        .unwrap_or_else(|| "local_tenant".to_string());
    if let Ok(snapshot) = crate::model_fabric_registry::global().snapshot_for_tenant(&tenant_id)
        && snapshot.deployments.iter().any(|deployment| {
            deployment.enabled
                && snapshot.connections.iter().any(|connection| {
                    connection.connection_id == deployment.connection_id
                        && connection.live_call_allowed
                        && matches!(connection.health.as_str(), "healthy" | "observed")
                        && mdx_core::model_provider(&connection.provider_id).is_some_and(
                            |provider| {
                                matches!(provider.provider_id, "ollama" | "vllm" | "tensorzero")
                                    || store.has_credential_for_connection(
                                        &tenant_id,
                                        &connection.credential_ref,
                                        provider.credential_env,
                                    )
                            },
                        )
                })
        })
    {
        return true;
    }
    let state = kernel.connected_model_local();
    let env_key = twin_model_gateway_provider_slots()
        .iter()
        .find(|slot| slot.provider_id == state.provider_id)
        .map(|slot| slot.env_key)
        .unwrap_or("");
    !env_key.is_empty() && state.connected && store.has_provider_key_for_tenant(&tenant_id, env_key)
}

pub(crate) fn setup_activation_state(
    kernel: &MdxKernel,
    store: &SecretStore,
) -> (bool, bool, bool) {
    let owner_claimed = kernel.install_owner_state().claimed;
    let model_connected = model_connected_for_setup(kernel, store);
    (
        owner_claimed && model_connected,
        owner_claimed,
        model_connected,
    )
}

fn choose(body: &str, kernel: &mut MdxKernel, store: &SecretStore) -> Result<String, String> {
    let (tenant_id, actor_id) = identity(body);
    let track = json_string_field(body, "track").unwrap_or_default();
    let (setup_complete, owner_claimed, model_connected) = setup_activation_state(kernel, store);
    if !setup_complete {
        let report = match kernel.refuse_setup_track_pre_activation(
            ChooseSetupTrack {
                tenant_id: &tenant_id,
                actor_id: &actor_id,
                track: &track,
            },
            "owner and usable model must be established before choosing a path",
            owner_claimed,
            model_connected,
        ) {
            Ok(report) => report,
            Err(error) => {
                return Ok(format!(
                    r#"{{"name":"mdx-install-track-choice-local-post","status":"SETUP_TRACK_REFUSED","chosen":false,"reason":{},"production_write_allowed":false}}"#,
                    json_string_literal(&error.message())
                ));
            }
        };
        return Ok(format!(
            r#"{{"name":"mdx-install-track-choice-local-post","status":{},"chosen":false,"track":{},"reason":"owner and usable model must be established before choosing a path","receipt_id":{},"projection_route":"/install/setup-router.json","production_write_allowed":false}}"#,
            json_string_literal(report.status),
            json_string_literal(&report.track),
            json_string_literal(&report.receipt_id),
        ));
    }
    // A bad choice (blank or unknown track, e.g. an empty smoke body) is refused
    // gracefully as a 200 with a reason, never a hard error.
    let report = match kernel.choose_setup_track(ChooseSetupTrack {
        tenant_id: &tenant_id,
        actor_id: &actor_id,
        track: &track,
    }) {
        Ok(report) => report,
        Err(error) => {
            return Ok(format!(
                r#"{{"name":"mdx-install-track-choice-local-post","status":"SETUP_TRACK_REFUSED","chosen":false,"reason":{},"production_write_allowed":false}}"#,
                json_string_literal(&error.message())
            ));
        }
    };
    Ok(format!(
        r#"{{"name":"mdx-install-track-choice-local-post","status":{},"chosen":true,"track":{},"destination":{},"receipt_id":{},"projection_route":"/install/setup-router.json","production_write_allowed":false}}"#,
        json_string_literal(report.status),
        json_string_literal(&report.track),
        json_string_literal(track_destination(&report.track)),
        json_string_literal(&report.receipt_id),
    ))
}

// Where each track sends the operator, and the honest state of that door today.
// "ready" = a real surface exists now; "needs_action" = the door is real and
// points at an operator checklist, but the work behind it is not complete.
// This is the only place destinations are declared, so the GUI cannot invent
// a route.
fn track_destination(track: &str) -> &'static str {
    match track {
        "try" => "/welcome/try",
        "build" => "/welcome/forge",
        "prepare" => "/welcome/production",
        _ => "/twin",
    }
}

fn track_status(track: &str) -> &'static str {
    match track {
        "try" | "build" => "ready",
        "prepare" => "needs_action",
        _ => "later",
    }
}

fn track_card(id: &str, label: &str, who: &str, note: &str) -> String {
    format!(
        r#"{{"id":{},"label":{},"who":{},"status":{},"destination":{},"note":{}}}"#,
        json_string_literal(id),
        json_string_literal(label),
        json_string_literal(who),
        json_string_literal(track_status(id)),
        json_string_literal(track_destination(id)),
        json_string_literal(note),
    )
}

fn router_projection(kernel: &MdxKernel, store: &SecretStore) -> String {
    let owner = kernel.install_owner_state();
    let observed_model = kernel.connected_model_local();
    let model_connected = model_connected_for_setup(kernel, store);
    let chosen = kernel.chosen_setup_track_local();
    let activation_status = crate::activation_route::activation_setup_projection_status(kernel);
    let path_choice_unlocked =
        activation_status.setup_complete || (owner.claimed && model_connected);
    let setup_complete = activation_status.setup_complete;
    let tracks = [
        track_card(
            "try",
            "Try MDx",
            "For leaders, product, design, and anyone evaluating MDx.",
            "A short guided start, then your first real conversation with Twin.",
        ),
        track_card(
            "build",
            "Build with MDx",
            "For engineers and technical operators working locally.",
            "A short Forge orientation, then the governed builder workspace.",
        ),
        track_card(
            "prepare",
            "Prepare for Production",
            "For cloud, infra, security, and admins.",
            "A short operator orientation, then the production readiness checklist.",
        ),
    ]
    .join(",");
    format!(
        r#"{{"name":"mdx-install-setup-router","status":"OK","read_only":true,"setup_complete":{},"setup_authority_route":"/activation/projection.json","activation_status":{},"activation_complete_steps":{},"activation_total_steps":{},"path_choice_unlocked":{},"owner_claimed":{},"owner_name":{},"model_connected":{},"model_id":{},"track_chosen":{},"chosen_track":{},"chosen_track_receipt_id":{},"writes_route":"/install/track-choices.json","tracks":[{}],"home_after_choice":"/twin","production_write_allowed":false}}"#,
        setup_complete,
        json_string_literal(activation_status.status),
        activation_status.complete_steps,
        activation_status.total_steps,
        path_choice_unlocked,
        owner.claimed,
        json_string_literal(&owner.owner_name),
        model_connected,
        json_string_literal(&observed_model.model_id),
        chosen.chosen,
        json_string_literal(&chosen.track),
        json_string_literal(&chosen.receipt_id),
        tracks,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdx_core::{
        ClaimInstallOwner, SETUP_TRACK_CHOSEN_RECEIPT_KIND, SETUP_TRACK_REFUSED_RECEIPT_KIND,
        TwinModelGatewayProviderObservation,
    };

    fn activate(kernel: &mut MdxKernel, store: &SecretStore) {
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
        store.set_for_tenant("local_tenant", slot.env_key, "sk-test");
    }

    #[test]
    fn track_choice_refuses_before_activation_without_choosing() {
        let mut kernel = MdxKernel::boot_local();
        let store = SecretStore::default();
        let response = choose(
            r#"{"track":"try","actor_id":"human:test"}"#,
            &mut kernel,
            &store,
        )
        .expect("choice");
        assert!(
            response.contains("\"status\":\"SETUP_TRACK_REFUSED_PRE_ACTIVATION\""),
            "{response}"
        );
        assert!(response.contains("\"chosen\":false"), "{response}");
        assert!(response.contains("\"receipt_id\""), "{response}");
        assert!(!kernel.chosen_setup_track_local().chosen);
        assert!(
            kernel
                .ledger()
                .entries()
                .iter()
                .any(|receipt| receipt.kind == SETUP_TRACK_REFUSED_RECEIPT_KIND)
        );
        assert!(
            !kernel
                .ledger()
                .entries()
                .iter()
                .any(|receipt| receipt.kind == SETUP_TRACK_CHOSEN_RECEIPT_KIND)
        );
    }

    #[test]
    fn setup_router_requires_a_usable_credential_not_just_an_observation() {
        let _env = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        unsafe {
            std::env::remove_var("XAI_API_KEY");
        }
        let mut kernel = MdxKernel::boot_local();
        let store = SecretStore::default();
        store.disable_keychain_lookup_for_tenant("local_tenant", "XAI_API_KEY");
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

        let projection = router_projection(&kernel, &store);
        assert!(
            projection.contains("\"setup_complete\":false"),
            "{projection}"
        );
        assert!(
            projection.contains("\"model_connected\":false"),
            "{projection}"
        );
    }

    #[test]
    fn activated_track_choice_records_the_chosen_track() {
        let mut kernel = MdxKernel::boot_local();
        let store = SecretStore::default();
        activate(&mut kernel, &store);
        let response = choose(
            r#"{"track":"try","actor_id":"human:test"}"#,
            &mut kernel,
            &store,
        )
        .expect("choice");
        assert!(response.contains("\"chosen\":true"), "{response}");
        assert!(response.contains("\"track\":\"try\""), "{response}");
        assert!(kernel.chosen_setup_track_local().chosen);
    }

    #[test]
    fn build_and_prepare_tracks_route_through_first_run_orientation() {
        assert_eq!(track_destination("build"), "/welcome/forge");
        assert_eq!(track_destination("prepare"), "/welcome/production");
    }
}
