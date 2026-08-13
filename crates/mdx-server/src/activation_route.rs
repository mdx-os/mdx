use crate::RouteResponse;
use crate::forge_turn_client::TurnClient;
use crate::secret_store::global;
use mdx_core::{
    ACTIVATION_FIRST_MISSION_BRIEF_SHAPED, ACTIVATION_FIRST_MISSION_COMPLETED,
    ACTIVATION_FIRST_MISSION_RESULT_RECORDED, ACTIVATION_FIRST_MISSION_RUN_ADMITTED,
    ACTIVATION_FIRST_MISSION_STARTED, ACTIVATION_FIRST_PROOF_RECORDED,
    ACTIVATION_FORGE_WORKSPACE_SAVED, ACTIVATION_MODEL_SETUP_RECORDED, ACTIVATION_PROFILE_SAVED,
    ACTIVATION_STARTER_WORKSPACE_SEEDED, ActivationEvent, ClaimInstallOwner, ForgeOutcomeSignal,
    ForgeRepoConnect, ForgeRepoIndex, ForgeRunEvent, GovernedWriteIdentity,
    LearningJudgmentDecision, LearningMemoryActivation, LearningMemoryPromotion, MarketplaceAct,
    MdxKernel, MessageActionRequest, PagesPublication, Receipt, json_string_literal,
};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::sync::{Arc, RwLock, mpsc};
use std::time::Duration;

const PROJECTION_ROUTE: &str = "/activation/projection.json";
const PROFILE_ROUTE: &str = "/activation/profile.json";
const MODEL_SETUP_ROUTE: &str = "/activation/model-setup.json";
const FORGE_WORKSPACE_ROUTE: &str = "/activation/forge-workspace.json";
const STARTER_WORKSPACE_ROUTE: &str = "/activation/starter-workspace.json";
const FIRST_PROOF_ROUTE: &str = "/activation/first-proof.json";
const FIRST_MISSION_PROJECTION_ROUTE: &str = "/activation/first-mission/projection.json";
const FIRST_MISSION_START_ROUTE: &str = "/activation/first-mission/start.json";
const MDX_SELF_REPO_ID: &str = "mdx-self";
const MDX_SELF_REPO_LABEL: &str = "MDx (this app, strict scope)";
const DEFAULT_FIRST_MISSION_ADMISSION_TIMEOUT_SECS: u64 = 30;

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        PROJECTION_ROUTE => Some(handle_projection(method, kernel)),
        PROFILE_ROUTE => Some(handle_profile(method, body, kernel)),
        MODEL_SETUP_ROUTE => Some(handle_model_setup(method, body, kernel)),
        FORGE_WORKSPACE_ROUTE => Some(handle_forge_workspace(method, body, kernel)),
        STARTER_WORKSPACE_ROUTE => Some(handle_starter_workspace(method, body, kernel)),
        FIRST_PROOF_ROUTE => Some(handle_first_proof(method, body, kernel)),
        FIRST_MISSION_PROJECTION_ROUTE => Some(handle_first_mission_projection(method, kernel)),
        FIRST_MISSION_START_ROUTE => Some(handle_first_mission_start(method, body, kernel)),
        _ => None,
    }
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
    Ok(RouteResponse::json(
        "200 OK",
        render_projection_json(&kernel),
    ))
}

fn handle_profile(
    method: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if method.eq_ignore_ascii_case("GET") {
        let kernel = kernel
            .read()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        return Ok(RouteResponse::json("200 OK", render_profile_json(&kernel)));
    }
    if let Some(response) = crate::reject_unless_method(method, "POST") {
        return Ok(response);
    }
    let value = json_value(body);
    let identity = activation_identity(body);
    let name = string_field(&value, "name")
        .or_else(|| string_field(&value, "display_name"))
        .unwrap_or_default();
    let role = string_field(&value, "role").unwrap_or_default();
    let style = string_field(&value, "style").unwrap_or_default();
    let style_register = nested_string_field(&value, "style", "register").unwrap_or_default();
    let style_grounding = nested_string_field(&value, "style", "grounding").unwrap_or_default();
    let style_guidance = nested_string_field(&value, "style", "guidance").unwrap_or_default();
    let voice = string_field(&value, "voice").unwrap_or_default();
    let principles = compact_value_field(&value, "principles");
    let tone = compact_value_field(&value, "toneKeywords");
    let avoid = compact_value_field(&value, "avoidPhrases");
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let install_owner = kernel
        .claim_install_owner_with_identity(
            ClaimInstallOwner {
                tenant_id: &identity.tenant_id,
                actor_id: &identity.actor_id,
                owner_name: &name,
            },
            &identity.governed,
        )
        .ok();
    let report = match kernel.record_activation_event_with_identity(
        ActivationEvent {
            tenant_id: &identity.tenant_id,
            actor_id: &identity.actor_id,
            kind: ACTIVATION_PROFILE_SAVED,
            step: "profile",
            surface: "twin",
            detail: "Saved the operator profile that Twin can cite.",
            starter_workspace_id: "",
            activation_seed: false,
            fields: &[
                ("profile_name", &name),
                ("profile_role", &role),
                ("profile_style", &style),
                ("profile_style_register", &style_register),
                ("profile_style_grounding", &style_grounding),
                ("profile_style_guidance", &style_guidance),
                ("profile_voice", &voice),
                ("profile_principles", &principles),
                ("profile_tone_keywords", &tone),
                ("profile_avoid_phrases", &avoid),
            ],
        },
        &identity.governed,
    ) {
        Ok(report) => report,
        Err(error) => return Ok(refusal_json(PROFILE_ROUTE, &error.message())),
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-activation-profile-local-post","status":{},"recorded":true,"step":"profile","install_owner":{{"claimed":{},"already_claimed":{},"owner_name":{},"receipt_id":{}}},"receipt_id":{},"policy_decision_id":{},"projection_route":{},"secret_values_recorded":false,"production_write_allowed":false}}"#,
            json_string_literal(report.status),
            install_owner
                .as_ref()
                .map(|report| report.claimed)
                .unwrap_or(false),
            install_owner
                .as_ref()
                .map(|report| report.already_claimed)
                .unwrap_or(false),
            json_string_literal(
                install_owner
                    .as_ref()
                    .map(|report| report.owner_name.as_str())
                    .unwrap_or("")
            ),
            json_string_literal(
                install_owner
                    .as_ref()
                    .map(|report| report.receipt_id.as_str())
                    .unwrap_or("")
            ),
            json_string_literal(&report.receipt_id),
            json_string_literal(&report.policy_decision_id),
            json_string_literal(PROJECTION_ROUTE),
        ),
    ))
}

fn handle_model_setup(
    method: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "POST") {
        return Ok(response);
    }
    let value = json_value(body);
    let identity = activation_identity(body);
    let providers = provider_inputs(&value);
    let remember_on_machine = bool_field(&value, "remember_on_machine")
        || bool_field(&value, "persist_on_machine")
        || bool_field(&value, "save_to_keychain");
    let test_mode = bool_field(&value, "test_mode");
    let mut stored_families = Vec::new();
    for provider in providers {
        if provider.api_key.trim().is_empty() || provider.family.trim().is_empty() {
            continue;
        }
        let provider_body = format!(
            r#"{{"provider_id":{},"api_key":{},"model_id":{},"actor_id":{},"persist_key":{},"test_mode":{}}}"#,
            json_string_literal(&provider.family),
            json_string_literal(&provider.api_key),
            json_string_literal(&provider.model_id),
            json_string_literal(&identity.actor_id),
            remember_on_machine,
            test_mode,
        );
        let response = crate::model_fabric_route::connect_model(&provider_body, kernel)?;
        if response.body.contains("MODEL_CONNECTED") {
            stored_families.push(provider.family);
        }
    }
    stored_families.sort();
    stored_families.dedup();
    let ollama_ready = ollama_runtime_ready();
    let private_twin_model = string_field(&value, "private_twin_model")
        .or_else(|| nested_string_field(&value, "local_models", "twin"))
        .unwrap_or_else(|| "qwen3".to_string());
    let local_coding_model = string_field(&value, "local_coding_model")
        .or_else(|| nested_string_field(&value, "local_models", "forge"))
        .unwrap_or_else(|| "qwen2.5-coder".to_string());
    if ollama_ready && value.get("private_twin_model").is_some() {
        let local_body = format!(
            r#"{{"provider_id":"ollama","model_id":{},"actor_id":{}}}"#,
            json_string_literal(&private_twin_model),
            json_string_literal(&identity.actor_id),
        );
        let response = crate::model_fabric_route::connect_model(&local_body, kernel)?;
        if response.body.contains("MODEL_CONNECTED") {
            stored_families.push("ollama".to_string());
            stored_families.sort();
            stored_families.dedup();
        }
    }
    let provider_count = stored_families.len().to_string();
    let ollama_ready_text = bool_text(ollama_ready);
    let families = stored_families.join(",");
    let mut guard = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let report = match guard.record_activation_event_with_identity(
        ActivationEvent {
            tenant_id: &identity.tenant_id,
            actor_id: &identity.actor_id,
            kind: ACTIVATION_MODEL_SETUP_RECORDED,
            step: "model_setup",
            surface: "models",
            detail: "Recorded model lanes for Twin and Forge without recording secret values.",
            starter_workspace_id: "",
            activation_seed: false,
            fields: &[
                ("stored_provider_count", &provider_count),
                ("stored_provider_families", &families),
                ("ollama_runtime_detected", ollama_ready_text),
                ("private_twin_model_recommendation", &private_twin_model),
                ("local_coding_model_recommendation", &local_coding_model),
                ("local_model_setup_source", "ollama"),
            ],
        },
        &identity.governed,
    ) {
        Ok(report) => report,
        Err(error) => return Ok(refusal_json(MODEL_SETUP_ROUTE, &error.message())),
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-activation-model-setup-local-post","status":{},"recorded":true,"stored_provider_count":{},"stored_provider_families":[{}],"local_models":{{"runtime":"ollama","runtime_detected":{},"private_twin_model":{},"forge_coding_model":{},"secret_values_recorded":false}},"receipt_id":{},"policy_decision_id":{},"projection_route":{},"production_write_allowed":false}}"#,
            json_string_literal(report.status),
            provider_count,
            quoted_strings(&stored_families),
            ollama_ready,
            json_string_literal(&private_twin_model),
            json_string_literal(&local_coding_model),
            json_string_literal(&report.receipt_id),
            json_string_literal(&report.policy_decision_id),
            json_string_literal(PROJECTION_ROUTE),
        ),
    ))
}

fn handle_forge_workspace(
    method: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "POST") {
        return Ok(response);
    }
    let value = json_value(body);
    let identity = activation_identity(body);
    let mut repo_ids = value_list(&value, "repo_ids")
        .or_else(|| value_list(&value, "repos"))
        .unwrap_or_default();
    let use_mdx_self = bool_field(&value, "use_mdx_self");
    let prefs = compact_value_field(&value, "preferences");
    let fleet_width =
        number_or_string_field(&value, "default_fleet_width").unwrap_or_else(|| "4".to_string());
    let mut guard = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let connected_repo_json = if use_mdx_self {
        if let Err(error) = connect_mdx_self_repo_if_needed(&mut guard, &identity) {
            return Ok(refusal_json(FORGE_WORKSPACE_ROUTE, &error));
        }
        if !repo_ids.iter().any(|repo_id| repo_id == MDX_SELF_REPO_ID) {
            repo_ids.push(MDX_SELF_REPO_ID.to_string());
        }
        format!(
            r#"{{"repo_id":{},"label":{},"kind":"local","strict_scope":true}}"#,
            json_string_literal(MDX_SELF_REPO_ID),
            json_string_literal(MDX_SELF_REPO_LABEL),
        )
    } else {
        "null".to_string()
    };
    let repo_count = repo_ids.len().to_string();
    let repo_ids_joined = repo_ids.join(",");
    let use_mdx_self_text = if use_mdx_self { "true" } else { "false" };
    let mdx_self_repo_id = if use_mdx_self { MDX_SELF_REPO_ID } else { "" };
    let mdx_self_repo_label = if use_mdx_self {
        MDX_SELF_REPO_LABEL
    } else {
        ""
    };
    let report = match guard.record_activation_event_with_identity(
        ActivationEvent {
            tenant_id: &identity.tenant_id,
            actor_id: &identity.actor_id,
            kind: ACTIVATION_FORGE_WORKSPACE_SAVED,
            step: "forge_workspace",
            surface: "forge",
            detail: "Recorded the Forge workspace preferences for first work.",
            starter_workspace_id: "",
            activation_seed: false,
            fields: &[
                ("repo_count", &repo_count),
                ("repo_ids", &repo_ids_joined),
                ("use_mdx_self", use_mdx_self_text),
                ("mdx_self_repo_id", mdx_self_repo_id),
                ("mdx_self_repo_label", mdx_self_repo_label),
                ("default_fleet_width", &fleet_width),
                ("preferences", &prefs),
            ],
        },
        &identity.governed,
    ) {
        Ok(report) => report,
        Err(error) => return Ok(refusal_json(FORGE_WORKSPACE_ROUTE, &error.message())),
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-activation-forge-workspace-local-post","status":{},"recorded":true,"repo_count":{},"use_mdx_self":{},"connected_repo":{},"default_fleet_width":{},"receipt_id":{},"policy_decision_id":{},"projection_route":{},"production_write_allowed":false}}"#,
            json_string_literal(report.status),
            repo_count,
            use_mdx_self,
            connected_repo_json,
            json_string_literal(&fleet_width),
            json_string_literal(&report.receipt_id),
            json_string_literal(&report.policy_decision_id),
            json_string_literal(PROJECTION_ROUTE),
        ),
    ))
}

fn handle_starter_workspace(
    method: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "POST") {
        return Ok(response);
    }
    let value = json_value(body);
    let identity = activation_identity(body);
    let starter_workspace_id = string_field(&value, "starter_workspace_id")
        .unwrap_or_else(|| "starter_workspace_local_001".to_string());
    let mut guard = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let starter = match guard.record_activation_event_with_identity(
        ActivationEvent {
            tenant_id: &identity.tenant_id,
            actor_id: &identity.actor_id,
            kind: ACTIVATION_STARTER_WORKSPACE_SEEDED,
            step: "starter_workspace",
            surface: "welcome",
            detail: "Seeded a small starter workspace so the user can see MDx's cross-surface loop.",
            starter_workspace_id: &starter_workspace_id,
            activation_seed: true,
            fields: &[
                ("starter_build_title", "Add a TDD standard and cite it from Forge"),
                ("message_channel_seeded", "true"),
                ("pages_seeded", "2"),
                ("marketplace_pack_seeded", "true"),
                ("action_card_seeded", "true"),
            ],
        },
        &identity.governed,
    ) {
        Ok(report) => report,
        Err(error) => return Ok(refusal_json(STARTER_WORKSPACE_ROUTE, &error.message())),
    };
    let seeded = seed_flywheel_substrate(
        &mut guard,
        &identity,
        &starter_workspace_id,
        &starter.receipt_id,
    )?;
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-activation-starter-workspace-local-post","status":{},"recorded":true,"starter_workspace_id":{},"receipt_id":{},"policy_decision_id":{},"seeded_receipt_ids":[{}],"activation_seed":true,"model_calls_performed":0,"honesty_note":"Starter workspace receipts are labeled activation_seed:true. Your real work replaces them.","projection_route":{},"production_write_allowed":false}}"#,
            json_string_literal(starter.status),
            json_string_literal(&starter_workspace_id),
            json_string_literal(&starter.receipt_id),
            json_string_literal(&starter.policy_decision_id),
            quoted_strings(&seeded),
            json_string_literal(PROJECTION_ROUTE),
        ),
    ))
}

fn handle_first_proof(
    method: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "POST") {
        return Ok(response);
    }
    let value = json_value(body);
    let identity = activation_identity(body);
    let starter_workspace_id = string_field(&value, "starter_workspace_id")
        .unwrap_or_else(|| latest_starter_workspace_id(kernel).unwrap_or_default());
    let mut guard = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let report = match guard.record_activation_event_with_identity(
        ActivationEvent {
            tenant_id: &identity.tenant_id,
            actor_id: &identity.actor_id,
            kind: ACTIVATION_FIRST_PROOF_RECORDED,
            step: "first_proof",
            surface: "welcome",
            detail: "Recorded the user's first proof that MDx is alive across surfaces.",
            starter_workspace_id: &starter_workspace_id,
            activation_seed: true,
            fields: &[
                ("twin_personalized", "true"),
                ("model_lane_ready", "true"),
                ("forge_workspace_ready", "true"),
                ("message_action_card_works", "true"),
                ("pages_source_available", "true"),
                ("marketplace_install_recorded", "true"),
                ("first_lesson_available", "true"),
            ],
        },
        &identity.governed,
    ) {
        Ok(report) => report,
        Err(error) => return Ok(refusal_json(FIRST_PROOF_ROUTE, &error.message())),
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-activation-first-proof-local-post","status":{},"recorded":true,"starter_workspace_id":{},"receipt_id":{},"policy_decision_id":{},"projection_route":{},"production_write_allowed":false}}"#,
            json_string_literal(report.status),
            json_string_literal(&starter_workspace_id),
            json_string_literal(&report.receipt_id),
            json_string_literal(&report.policy_decision_id),
            json_string_literal(PROJECTION_ROUTE),
        ),
    ))
}

fn handle_first_mission_projection(
    method: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "GET") {
        return Ok(response);
    }
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    Ok(RouteResponse::json(
        "200 OK",
        render_first_mission_projection_json(&kernel),
    ))
}

fn handle_first_mission_start(
    method: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "POST") {
        return Ok(response);
    }
    let value = json_value(body);
    let identity = activation_identity(body);
    let starter_id =
        string_field(&value, "starter_id").unwrap_or_else(|| "add_small_test".to_string());
    let starter = {
        let guard = kernel
            .read()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        first_mission_starter(&guard, &starter_id)
    };
    let ask = string_field(&value, "ask").unwrap_or_else(|| starter.ask.clone());
    let repo_target =
        string_field(&value, "repo_target").unwrap_or_else(|| starter.repo_target.clone());
    let repo_id = string_field(&value, "repo_id").unwrap_or_else(|| {
        if repo_target == "mdx_self" {
            MDX_SELF_REPO_ID.to_string()
        } else {
            starter.repo_id.clone()
        }
    });
    let force_fallback = bool_field(&value, "force_fallback") || bool_field(&value, "preview_only");
    let run_available = {
        let guard = kernel
            .read()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        !force_fallback
            && TurnClient::any_builder_configured_for_tenant(&guard, &identity.tenant_id)
    };
    if run_available {
        let (mission_id, started_receipt_id) = {
            let mut guard = kernel
                .write()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            if repo_target == "mdx_self"
                && let Err(error) = connect_mdx_self_repo_if_needed(&mut guard, &identity)
            {
                return Ok(refusal_json(FIRST_MISSION_START_ROUTE, &error));
            }
            let mission_id = guard.mint_id("activation_first_mission");
            let started = match guard.record_activation_event_with_identity(
                ActivationEvent {
                    tenant_id: &identity.tenant_id,
                    actor_id: &identity.actor_id,
                    kind: ACTIVATION_FIRST_MISSION_STARTED,
                    step: "first_mission",
                    surface: "welcome",
                    detail: "Started a real first mission from the welcome flow.",
                    starter_workspace_id: "",
                    activation_seed: false,
                    fields: &[
                        ("mission_id", &mission_id),
                        ("ask", &ask),
                        ("starter_id", starter.id.as_str()),
                        ("repo_target", &repo_target),
                        ("repo_id", &repo_id),
                        ("curated_safe_task", "true"),
                        ("shaping_mode", "live_personas"),
                    ],
                },
                &identity.governed,
            ) {
                Ok(report) => report,
                Err(error) => {
                    return Ok(refusal_json(FIRST_MISSION_START_ROUTE, &error.message()));
                }
            };
            (mission_id, started.receipt_id)
        };
        spawn_first_mission_live_shaping(
            Arc::clone(kernel),
            identity.tenant_id.clone(),
            identity.actor_id.clone(),
            mission_id.clone(),
            started_receipt_id.clone(),
            ask.clone(),
            repo_id.clone(),
            starter.clone(),
        );
        return Ok(RouteResponse::json(
            "200 OK",
            format!(
                r#"{{"name":"mdx-activation-first-mission-local-post","status":"FIRST_MISSION_SHAPING","mission_id":{},"started_receipt_id":{},"brief_receipt_id":"","shaping":{},"page":null,"run":{{"run_id":"","status":"not_started","run_started_receipt_id":"","run_admitted_receipt_id":"","projection_route":"/forge/runs/projection.json","fallback_reason":""}},"message":{{"channel_id":"activation_first_mission","action_request_id":"","receipt_id":"","projection_route":"/messages/action-requests/projection.json"}},"completed_receipt_id":"","projection_route":{},"activation_seed":false,"production_write_allowed":false}}"#,
                json_string_literal(&mission_id),
                json_string_literal(&started_receipt_id),
                first_mission_shaping_json(None, &mission_id, true),
                json_string_literal(FIRST_MISSION_PROJECTION_ROUTE),
            ),
        ));
    }
    let (mission_id, started_receipt_id, brief_receipt_id, page_receipt_id, page_id) = {
        let mut guard = kernel
            .write()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        if repo_target == "mdx_self"
            && let Err(error) = connect_mdx_self_repo_if_needed(&mut guard, &identity)
        {
            return Ok(refusal_json(FIRST_MISSION_START_ROUTE, &error));
        }
        let mission_id = guard.mint_id("activation_first_mission");
        let started = match guard.record_activation_event_with_identity(
            ActivationEvent {
                tenant_id: &identity.tenant_id,
                actor_id: &identity.actor_id,
                kind: ACTIVATION_FIRST_MISSION_STARTED,
                step: "first_mission",
                surface: "welcome",
                detail: "Started a real first mission from the welcome flow.",
                starter_workspace_id: "",
                activation_seed: false,
                fields: &[
                    ("mission_id", &mission_id),
                    ("ask", &ask),
                    ("starter_id", starter.id.as_str()),
                    ("repo_target", &repo_target),
                    ("repo_id", &repo_id),
                    ("curated_safe_task", "true"),
                ],
            },
            &identity.governed,
        ) {
            Ok(report) => report,
            Err(error) => return Ok(refusal_json(FIRST_MISSION_START_ROUTE, &error.message())),
        };
        let brief = match guard.record_activation_event_with_identity(
            ActivationEvent {
                tenant_id: &identity.tenant_id,
                actor_id: &identity.actor_id,
                kind: ACTIVATION_FIRST_MISSION_BRIEF_SHAPED,
                step: "first_mission_brief",
                surface: "twin",
                detail: "Twin shaped the first mission brief into goal, approach, acceptance, and proof.",
                starter_workspace_id: "",
                activation_seed: false,
                fields: &[
                    ("mission_id", &mission_id),
                    ("goal", starter.goal.as_str()),
                    ("approach", starter.approach.as_str()),
                    ("acceptance", starter.acceptance.as_str()),
                    ("proof_command", starter.proof_command.as_str()),
                    ("persona_ids", "twin_coder,twin_architect,twin_advisor"),
                    ("coder_contribution", starter.coder_contribution.as_str()),
                    ("architect_contribution", starter.architect_contribution.as_str()),
                    ("advisor_contribution", starter.advisor_contribution.as_str()),
                    ("shaping_mode", "curated_tier0_no_live_search"),
                ],
            },
            &identity.governed,
        ) {
            Ok(report) => report,
            Err(error) => return Ok(refusal_json(FIRST_MISSION_START_ROUTE, &error.message())),
        };
        let page_id = format!("{mission_id}_brief");
        let revision_id = format!("{page_id}_rev_001");
        let body_ref = store_first_mission_brief_body(
            &mission_id,
            &page_id,
            &revision_id,
            starter.goal.as_str(),
            starter.approach.as_str(),
            starter.acceptance.as_str(),
            starter.proof_command.as_str(),
        );
        let page = match guard.save_pages_publication_local_with_identity(
            PagesPublication {
                tenant_id: &identity.tenant_id,
                actor_id: &identity.actor_id,
                document_id: &page_id,
                title: starter.page_title.as_str(),
                body_ref: &body_ref,
                source_receipt_id: &brief.receipt_id,
                revision_id: &revision_id,
                page_type: "spec",
            },
            &identity.governed,
        ) {
            Ok(report) => report,
            Err(error) => return Ok(refusal_json(FIRST_MISSION_START_ROUTE, &error.message())),
        };
        (
            mission_id,
            started.receipt_id,
            brief.receipt_id,
            page.publication_receipt_id,
            page_id,
        )
    };
    let run_id = if run_available {
        let mut guard = kernel
            .write()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        guard.mint_id("forge_run")
    } else {
        String::new()
    };
    let mut run_status = if run_available {
        "running".to_string()
    } else {
        "not_started".to_string()
    };
    let run_receipt_id = String::new();
    let mut fallback_reason = String::new();
    if run_available {
        let run_body = format!(
            r#"{{"intent":{},"repo_id":{},"work_item_id":{},"allowed_commands":[{}],"max_turns":{},"fleet_width":1,"actor_id":{},"tenant_id":{},"source_receipt_id":{},"activation_first_mission_id":{},"requested_run_id":{}}}"#,
            json_string_literal(&starter.forge_intent),
            json_string_literal(&repo_id),
            json_string_literal(starter.work_item_id.as_str()),
            json_string_literal(starter.proof_command.as_str()),
            starter.max_turns,
            json_string_literal(&identity.actor_id),
            json_string_literal(&identity.tenant_id),
            json_string_literal(&page_receipt_id),
            json_string_literal(&mission_id),
            json_string_literal(&run_id),
        );
        spawn_first_mission_forge_admission(
            Arc::clone(kernel),
            identity.tenant_id.clone(),
            identity.actor_id.clone(),
            mission_id.clone(),
            started_receipt_id.clone(),
            brief_receipt_id.clone(),
            page_receipt_id.clone(),
            page_id.clone(),
            run_id.clone(),
            run_body,
        );
    } else {
        run_status = "fallback".to_string();
        fallback_reason = if force_fallback {
            "Live Forge execution was skipped for preview or smoke proof.".to_string()
        } else {
            "Connect a runnable Forge model to turn this first mission into a live build."
                .to_string()
        };
    }
    let mut message_receipt_id = String::new();
    let mut action_request_id = String::new();
    let mut completed_receipt_id = String::new();
    let mut run_admitted_receipt_id = String::new();
    {
        let mut guard = kernel
            .write()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        let run_available_text = bool_text(run_available);
        if run_status == "running" {
            let admitted = match guard.record_activation_event_with_identity(
                ActivationEvent {
                    tenant_id: &identity.tenant_id,
                    actor_id: &identity.actor_id,
                    kind: ACTIVATION_FIRST_MISSION_RUN_ADMITTED,
                    step: "first_mission_run",
                    surface: "forge",
                    detail:
                        "Forge accepted the first mission run and is executing it asynchronously.",
                    starter_workspace_id: "",
                    activation_seed: false,
                    fields: &[
                        ("mission_id", &mission_id),
                        ("started_receipt_id", &started_receipt_id),
                        ("brief_receipt_id", &brief_receipt_id),
                        ("page_receipt_id", &page_receipt_id),
                        ("run_id", &run_id),
                        ("run_receipt_id", &run_receipt_id),
                        ("run_status", &run_status),
                        ("run_available", run_available_text),
                    ],
                },
                &identity.governed,
            ) {
                Ok(report) => report,
                Err(error) => return Ok(refusal_json(FIRST_MISSION_START_ROUTE, &error.message())),
            };
            run_admitted_receipt_id = admitted.receipt_id;
        } else {
            let source_receipt_id = page_receipt_id.as_str();
            action_request_id = format!("{mission_id}_review_action");
            let action_payload = format!(
                r#"{{"kind":"first_mission_review","mission_id":{},"run_id":{},"page_id":{},"run_status":{},"fallback_reason":{},"production_write_allowed":false}}"#,
                json_string_literal(&mission_id),
                json_string_literal(&run_id),
                json_string_literal(&page_id),
                json_string_literal(&run_status),
                json_string_literal(&fallback_reason),
            );
            let message = match guard.save_message_action_request_local_with_identity(
                MessageActionRequest {
                    tenant_id: &identity.tenant_id,
                    actor_id: &identity.actor_id,
                    thread_id: "activation_first_mission_thread",
                    channel_id: "activation_first_mission",
                    action_request_id: &action_request_id,
                    source_receipt_id,
                    title: "Review your first mission",
                    summary: "MDx shaped the first mission and saved the brief. Connect a runnable model to let Forge build it live.",
                    proposed_action: "Connect a model or continue with the seeded warm-up preview.",
                    action_payload: &action_payload,
                },
                &identity.governed,
            ) {
                Ok(report) => report,
                Err(error) => return Ok(refusal_json(FIRST_MISSION_START_ROUTE, &error.message())),
            };
            message_receipt_id = message.action_request_receipt_id;
            let completed = match guard.record_activation_event_with_identity(
                ActivationEvent {
                    tenant_id: &identity.tenant_id,
                    actor_id: &identity.actor_id,
                    kind: ACTIVATION_FIRST_MISSION_COMPLETED,
                    step: "first_mission_payoff",
                    surface: "welcome",
                    detail: "Recorded the first-mission fallback chain through Twin, Context, and Message.",
                    starter_workspace_id: "",
                    activation_seed: false,
                    fields: &[
                        ("mission_id", &mission_id),
                        ("started_receipt_id", &started_receipt_id),
                        ("brief_receipt_id", &brief_receipt_id),
                        ("page_receipt_id", &page_receipt_id),
                        ("run_id", &run_id),
                        ("run_receipt_id", &run_receipt_id),
                        ("run_status", &run_status),
                        ("run_available", run_available_text),
                        ("fallback_reason", &fallback_reason),
                        ("message_action_request_id", &action_request_id),
                        ("message_receipt_id", &message_receipt_id),
                    ],
                },
                &identity.governed,
            ) {
                Ok(report) => report,
                Err(error) => return Ok(refusal_json(FIRST_MISSION_START_ROUTE, &error.message())),
            };
            completed_receipt_id = completed.receipt_id;
            record_first_mission_activation_completion(
                &mut guard,
                &identity.governed,
                FirstMissionActivationCompletion {
                    tenant_id: &identity.tenant_id,
                    actor_id: &identity.actor_id,
                    mission_id: &mission_id,
                    started_receipt_id: &started_receipt_id,
                    brief_receipt_id: &brief_receipt_id,
                    page_receipt_id: &page_receipt_id,
                    run_id: &run_id,
                    run_status: &run_status,
                    completed_receipt_id: &completed_receipt_id,
                    result_receipt_id: "",
                },
            );
        }
    }
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-activation-first-mission-local-post","status":{},"mission_id":{},"started_receipt_id":{},"brief_receipt_id":{},"page":{{"page_id":{},"receipt_id":{},"url":{}}},"run":{{"run_id":{},"status":{},"run_started_receipt_id":{},"run_admitted_receipt_id":{},"projection_route":"/forge/runs/projection.json","fallback_reason":{}}},"message":{{"channel_id":"activation_first_mission","action_request_id":{},"receipt_id":{},"projection_route":"/messages/action-requests/projection.json"}},"completed_receipt_id":{},"projection_route":{},"activation_seed":false,"production_write_allowed":false}}"#,
            json_string_literal(if run_status == "running" {
                "FIRST_MISSION_RUNNING"
            } else {
                "FIRST_MISSION_FALLBACK"
            }),
            json_string_literal(&mission_id),
            json_string_literal(&started_receipt_id),
            json_string_literal(&brief_receipt_id),
            json_string_literal(&page_id),
            json_string_literal(&page_receipt_id),
            json_string_literal(&format!("/pages?document_id={page_id}")),
            json_string_literal(&run_id),
            json_string_literal(&run_status),
            json_string_literal(&run_receipt_id),
            json_string_literal(&run_admitted_receipt_id),
            json_string_literal(&fallback_reason),
            json_string_literal(&action_request_id),
            json_string_literal(&message_receipt_id),
            json_string_literal(&completed_receipt_id),
            json_string_literal(FIRST_MISSION_PROJECTION_ROUTE),
        ),
    ))
}

#[derive(Clone)]
struct FirstMissionBriefShape {
    goal: String,
    approach: String,
    acceptance: String,
    proof_command: String,
    coder_contribution: String,
    architect_contribution: String,
    advisor_contribution: String,
    shaping_mode: String,
}

impl FirstMissionBriefShape {
    fn curated(starter: &FirstMissionStarter) -> Self {
        Self {
            goal: starter.goal.clone(),
            approach: starter.approach.clone(),
            acceptance: starter.acceptance.clone(),
            proof_command: starter.proof_command.clone(),
            coder_contribution: starter.coder_contribution.clone(),
            architect_contribution: starter.architect_contribution.clone(),
            advisor_contribution: starter.advisor_contribution.clone(),
            shaping_mode: "curated_tier0_no_live_search".to_string(),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_first_mission_live_shaping(
    kernel: Arc<RwLock<MdxKernel>>,
    tenant_id: String,
    actor_id: String,
    mission_id: String,
    started_receipt_id: String,
    ask: String,
    repo_id: String,
    starter: FirstMissionStarter,
) {
    let thread_name = format!("activation-first-mission-shape-{mission_id}");
    let thread_kernel = Arc::clone(&kernel);
    let thread_tenant_id = tenant_id.clone();
    let thread_actor_id = actor_id.clone();
    let thread_mission_id = mission_id.clone();
    let thread_started_receipt_id = started_receipt_id.clone();
    let thread_ask = ask.clone();
    let thread_repo_id = repo_id.clone();
    let thread_starter = starter.clone();
    if let Err(error) = std::thread::Builder::new()
        .name(thread_name)
        .spawn(move || {
            let shaped = match shape_first_mission_live(
                &thread_kernel,
                &thread_tenant_id,
                &thread_mission_id,
                &thread_ask,
                &thread_repo_id,
                &thread_starter,
            ) {
                Ok(shaped) => shaped,
                Err(error) => {
                    crate::activation_deliberation_stream::publish_terminal(
                        &thread_mission_id,
                        "fallback",
                        unknown_deliberation_model(),
                    );
                    let shaped = FirstMissionBriefShape::curated(&thread_starter);
                    FirstMissionBriefShape {
                        advisor_contribution: format!(
                            "Live shaping fell back to the prepared starter. Reason: {error}"
                        ),
                        ..shaped
                    }
                }
            };
            record_first_mission_brief_page_and_run(
                &thread_kernel,
                &thread_tenant_id,
                &thread_actor_id,
                &thread_mission_id,
                &thread_started_receipt_id,
                &thread_repo_id,
                &thread_starter,
                shaped,
            );
            crate::kernel_snapshot::snapshot_at_boundary(&thread_kernel);
        })
    {
        crate::activation_deliberation_stream::publish_terminal(
            &mission_id,
            "fallback",
            unknown_deliberation_model(),
        );
        let shaped = FirstMissionBriefShape::curated(&starter);
        record_first_mission_brief_page_and_run(
            &kernel,
            &tenant_id,
            &actor_id,
            &mission_id,
            &started_receipt_id,
            &repo_id,
            &starter,
            FirstMissionBriefShape {
                advisor_contribution: format!(
                    "Live shaping could not start, so MDx used the prepared starter. Reason: {error}"
                ),
                ..shaped
            },
        );
    }
}

fn unknown_deliberation_model() -> crate::activation_deliberation_stream::DeliberationModel<'static>
{
    crate::activation_deliberation_stream::DeliberationModel {
        provider_family: "unknown",
        model_id: "unknown",
        model_class: "unknown",
    }
}

fn shape_first_mission_live(
    kernel: &Arc<RwLock<MdxKernel>>,
    tenant_id: &str,
    mission_id: &str,
    ask: &str,
    repo_id: &str,
    starter: &FirstMissionStarter,
) -> Result<FirstMissionBriefShape, String> {
    let client = {
        let guard = kernel
            .read()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        TurnClient::for_role(&guard, tenant_id, "planner")
            .or_else(|| TurnClient::builder_for_tenant(&guard, tenant_id, ""))
            .ok_or_else(|| "no runnable model for first mission shaping".to_string())?
    };
    let model_id = client.model_id().to_string();
    let provider_family = client.provider_family().to_string();
    let model_class =
        crate::forge_run_stream::model_class(&provider_family, "planner", &model_id).to_string();
    let model = || crate::activation_deliberation_stream::DeliberationModel {
        provider_family: &provider_family,
        model_id: &model_id,
        model_class: &model_class,
    };
    let architect = complete_persona_line(
        &client,
        "Architect",
        "structure, boundaries, and the safest shape of the work",
        ask,
        repo_id,
        starter,
    )?;
    publish_first_mission_deliberation(
        mission_id,
        "twin_architect",
        "Architect",
        &architect,
        "voice",
        model(),
    );
    let coder = complete_persona_line(
        &client,
        "Coder",
        "the smallest useful implementation path and likely files",
        ask,
        repo_id,
        starter,
    )?;
    publish_first_mission_deliberation(mission_id, "twin_coder", "Coder", &coder, "voice", model());
    let advisor = complete_persona_line(
        &client,
        "Advisor",
        "risks, acceptance, and what would make the result trustworthy",
        ask,
        repo_id,
        starter,
    )?;
    publish_first_mission_deliberation(
        mission_id,
        "twin_advisor",
        "Advisor",
        &advisor,
        "voice",
        model(),
    );
    let synthesis = complete_first_mission_synthesis(
        &client, ask, repo_id, starter, &architect, &coder, &advisor,
    )?;
    publish_first_mission_deliberation(
        mission_id,
        "twin_architect",
        "Architect",
        &synthesis,
        "synthesis",
        model(),
    );
    let (goal, approach, acceptance) = parse_first_mission_synthesis(&synthesis, starter);
    crate::activation_deliberation_stream::publish_terminal(mission_id, "ready", model());
    Ok(FirstMissionBriefShape {
        goal,
        approach,
        acceptance,
        proof_command: starter.proof_command.clone(),
        coder_contribution: cap_chars(coder.trim(), 500),
        architect_contribution: cap_chars(architect.trim(), 500),
        advisor_contribution: cap_chars(advisor.trim(), 500),
        shaping_mode: "live_personas".to_string(),
    })
}

fn complete_persona_line(
    client: &TurnClient,
    label: &str,
    lens: &str,
    ask: &str,
    repo_id: &str,
    starter: &FirstMissionStarter,
) -> Result<String, String> {
    let messages = vec![
        serde_json::json!({
            "role": "system",
            "content": format!("You are the MDx Twin {label}. Give one concise, concrete sentence from this lens: {lens}. Do not claim work has run. Do not propose files outside the starter scope.")
        }),
        serde_json::json!({
            "role": "user",
            "content": format!("User ask: {ask}\nRepo: {repo_id}\nStarter work item: {}\nAllowed proof command: {}\nSafe intent: {}\nReturn one useful sentence only.", starter.work_item_id, starter.proof_command, starter.forge_intent)
        }),
    ];
    client
        .complete(&messages)
        .map(|turn| cap_chars(turn.text.trim(), 500))
}

fn complete_first_mission_synthesis(
    client: &TurnClient,
    ask: &str,
    repo_id: &str,
    starter: &FirstMissionStarter,
    architect: &str,
    coder: &str,
    advisor: &str,
) -> Result<String, String> {
    let messages = vec![
        serde_json::json!({
            "role": "system",
            "content": "You are the MDx Twin Architect. Synthesize the first mission brief. Return exactly three lines: Goal: ..., Approach: ..., Acceptance: .... Keep it narrow and compatible with the provided starter proof command."
        }),
        serde_json::json!({
            "role": "user",
            "content": format!("User ask: {ask}\nRepo: {repo_id}\nStarter title: {}\nWork item: {}\nProof command, fixed by the harness: {}\nArchitect: {architect}\nCoder: {coder}\nAdvisor: {advisor}", starter.title, starter.work_item_id, starter.proof_command)
        }),
    ];
    client
        .complete(&messages)
        .map(|turn| cap_chars(turn.text.trim(), 1200))
}

fn parse_first_mission_synthesis(
    synthesis: &str,
    starter: &FirstMissionStarter,
) -> (String, String, String) {
    let field = |label: &str| -> Option<String> {
        synthesis.lines().find_map(|line| {
            let (prefix, value) = line.split_once(':')?;
            prefix
                .trim()
                .eq_ignore_ascii_case(label)
                .then(|| cap_chars(value.trim(), 500))
                .filter(|value| !value.is_empty())
        })
    };
    (
        field("Goal").unwrap_or_else(|| starter.goal.clone()),
        field("Approach").unwrap_or_else(|| starter.approach.clone()),
        field("Acceptance").unwrap_or_else(|| starter.acceptance.clone()),
    )
}

fn publish_first_mission_deliberation(
    mission_id: &str,
    companion_id: &str,
    label: &str,
    text: &str,
    role: &str,
    model: crate::activation_deliberation_stream::DeliberationModel<'_>,
) {
    crate::activation_deliberation_stream::publish_deliberation(
        crate::activation_deliberation_stream::DeliberationFrame {
            mission_id,
            companion_id,
            label,
            text,
            role,
            final_chunk: true,
            model,
        },
    );
}

pub(crate) fn first_mission_brief_body(
    goal: &str,
    approach: &str,
    acceptance: &str,
    proof_command: &str,
) -> String {
    format!(
        "# First Mission Brief\n\n## Goal\n{}\n\n## Approach\n{}\n\n## Acceptance\n{}\n\n## Proof\n{}\n",
        goal.trim(),
        approach.trim(),
        acceptance.trim(),
        proof_command.trim()
    )
}

fn store_first_mission_brief_body(
    mission_id: &str,
    page_id: &str,
    revision_id: &str,
    goal: &str,
    approach: &str,
    acceptance: &str,
    proof_command: &str,
) -> String {
    let body = first_mission_brief_body(goal, approach, acceptance, proof_command);
    crate::pages_body_store::store_published_body(page_id, revision_id, &body)
        .unwrap_or_else(|| format!("activation://first-mission/{mission_id}/brief"))
}

#[allow(clippy::too_many_arguments)]
fn record_first_mission_brief_page_and_run(
    kernel: &Arc<RwLock<MdxKernel>>,
    tenant_id: &str,
    actor_id: &str,
    mission_id: &str,
    started_receipt_id: &str,
    repo_id: &str,
    starter: &FirstMissionStarter,
    shaped: FirstMissionBriefShape,
) {
    let (brief_receipt_id, page_receipt_id, page_id, run_id) = {
        let Ok(mut guard) = kernel.write() else {
            return;
        };
        let identity = GovernedWriteIdentity::local_demo(actor_id);
        let Ok(brief) = guard.record_activation_event_with_identity(
            ActivationEvent {
                tenant_id,
                actor_id,
                kind: ACTIVATION_FIRST_MISSION_BRIEF_SHAPED,
                step: "first_mission_brief",
                surface: "twin",
                detail: "Twin shaped the first mission brief into goal, approach, acceptance, and proof.",
                starter_workspace_id: "",
                activation_seed: false,
                fields: &[
                    ("mission_id", mission_id),
                    ("goal", shaped.goal.as_str()),
                    ("approach", shaped.approach.as_str()),
                    ("acceptance", shaped.acceptance.as_str()),
                    ("proof_command", shaped.proof_command.as_str()),
                    ("persona_ids", "twin_coder,twin_architect,twin_advisor"),
                    ("coder_contribution", shaped.coder_contribution.as_str()),
                    ("architect_contribution", shaped.architect_contribution.as_str()),
                    ("advisor_contribution", shaped.advisor_contribution.as_str()),
                    ("shaping_mode", shaped.shaping_mode.as_str()),
                ],
            },
            &identity,
        ) else {
            return;
        };
        let page_id = format!("{mission_id}_brief");
        let revision_id = format!("{page_id}_rev_001");
        let body_ref = store_first_mission_brief_body(
            mission_id,
            &page_id,
            &revision_id,
            shaped.goal.as_str(),
            shaped.approach.as_str(),
            shaped.acceptance.as_str(),
            shaped.proof_command.as_str(),
        );
        let Ok(page) = guard.save_pages_publication_local_with_identity(
            PagesPublication {
                tenant_id,
                actor_id,
                document_id: &page_id,
                title: starter.page_title.as_str(),
                body_ref: &body_ref,
                source_receipt_id: &brief.receipt_id,
                revision_id: &revision_id,
                page_type: "spec",
            },
            &identity,
        ) else {
            return;
        };
        let run_id = guard.mint_id("forge_run");
        (
            brief.receipt_id,
            page.publication_receipt_id,
            page_id,
            run_id,
        )
    };
    let run_body = format!(
        r#"{{"intent":{},"repo_id":{},"work_item_id":{},"allowed_commands":[{}],"max_turns":{},"fleet_width":1,"actor_id":{},"tenant_id":{},"source_receipt_id":{},"activation_first_mission_id":{},"requested_run_id":{}}}"#,
        json_string_literal(&starter.forge_intent),
        json_string_literal(repo_id),
        json_string_literal(starter.work_item_id.as_str()),
        json_string_literal(starter.proof_command.as_str()),
        starter.max_turns,
        json_string_literal(actor_id),
        json_string_literal(tenant_id),
        json_string_literal(&page_receipt_id),
        json_string_literal(mission_id),
        json_string_literal(&run_id),
    );
    spawn_first_mission_forge_admission(
        Arc::clone(kernel),
        tenant_id.to_string(),
        actor_id.to_string(),
        mission_id.to_string(),
        started_receipt_id.to_string(),
        brief_receipt_id.clone(),
        page_receipt_id.clone(),
        page_id.clone(),
        run_id.clone(),
        run_body,
    );
    let Ok(mut guard) = kernel.write() else {
        return;
    };
    let _ = guard.record_activation_event_with_identity(
        ActivationEvent {
            tenant_id,
            actor_id,
            kind: ACTIVATION_FIRST_MISSION_RUN_ADMITTED,
            step: "first_mission_run",
            surface: "forge",
            detail: "Forge is admitting the first mission run.",
            starter_workspace_id: "",
            activation_seed: false,
            fields: &[
                ("mission_id", mission_id),
                ("started_receipt_id", started_receipt_id),
                ("brief_receipt_id", &brief_receipt_id),
                ("page_receipt_id", &page_receipt_id),
                ("run_id", &run_id),
                ("run_receipt_id", ""),
                ("run_status", "admitting"),
                ("run_available", "true"),
            ],
        },
        &GovernedWriteIdentity::local_demo(actor_id),
    );
}

#[allow(clippy::too_many_arguments)]
fn spawn_first_mission_forge_admission(
    kernel: Arc<RwLock<MdxKernel>>,
    tenant_id: String,
    actor_id: String,
    mission_id: String,
    started_receipt_id: String,
    brief_receipt_id: String,
    page_receipt_id: String,
    page_id: String,
    run_id: String,
    run_body: String,
) {
    let thread_run_id = run_id.clone();
    let thread_kernel = Arc::clone(&kernel);
    let thread_tenant_id = tenant_id.clone();
    let thread_actor_id = actor_id.clone();
    let thread_mission_id = mission_id.clone();
    let thread_started_receipt_id = started_receipt_id.clone();
    let thread_brief_receipt_id = brief_receipt_id.clone();
    let thread_page_receipt_id = page_receipt_id.clone();
    let thread_page_id = page_id.clone();
    let thread_run_id_for_body = run_id.clone();
    if let Err(error) = std::thread::Builder::new()
        .name(format!("activation-first-mission-admit-{thread_run_id}"))
        .spawn(move || {
            let admission_kernel = Arc::clone(&thread_kernel);
            let (tx, rx) = mpsc::channel();
            if let Err(error) = std::thread::Builder::new()
                .name(format!(
                    "activation-first-mission-admit-route-{thread_run_id_for_body}"
                ))
                .spawn(move || {
                    let result = crate::forge_run_route::route_response(
                        "POST",
                        "/forge/runs.json",
                        &run_body,
                        &admission_kernel,
                    );
                    let _ = tx.send(result);
                })
            {
                record_first_mission_run_start_failure(
                    &thread_kernel,
                    &thread_tenant_id,
                    &thread_actor_id,
                    &thread_mission_id,
                    &thread_started_receipt_id,
                    &thread_brief_receipt_id,
                    &thread_page_receipt_id,
                    &thread_page_id,
                    &thread_run_id_for_body,
                    &format!("could not start first mission Forge admission call: {error}"),
                );
                crate::kernel_snapshot::snapshot_at_boundary(&thread_kernel);
                return;
            }
            let failure_reason = match rx.recv_timeout(first_mission_admission_timeout()) {
                Ok(Some(Ok(response))) if response.body.contains("\"status\":\"RUN_STARTED\"") => {
                    let value = json_value(&response.body);
                    let admitted_run_id = string_field(&value, "run_id").unwrap_or_default();
                    if admitted_run_id == thread_run_id_for_body {
                        let run_receipt_id =
                            string_field(&value, "run_started_receipt_id").unwrap_or_default();
                        record_first_mission_run_admitted(
                            &thread_kernel,
                            &thread_tenant_id,
                            &thread_actor_id,
                            &thread_mission_id,
                            &thread_started_receipt_id,
                            &thread_brief_receipt_id,
                            &thread_page_receipt_id,
                            &thread_run_id_for_body,
                            &run_receipt_id,
                            "running",
                            "Forge accepted the first mission run and is executing it asynchronously.",
                        );
                        return;
                    }
                    format!(
                        "Forge admitted a different run id ({admitted_run_id}) than the first mission reserved ({thread_run_id_for_body})."
                    )
                }
                Ok(Some(Ok(response))) => {
                    let value = json_value(&response.body);
                    string_field(&value, "reason").unwrap_or_else(|| {
                        "Forge could not start this first mission run yet.".to_string()
                    })
                }
                Ok(Some(Err(error))) => error,
                Ok(None) => "Forge run route was unavailable for the first mission.".to_string(),
                Err(mpsc::RecvTimeoutError::Timeout) => format!(
                    "Forge run admission did not return within {} seconds. The run was not marked running because Forge did not record RUN_STARTED.",
                    first_mission_admission_timeout().as_secs()
                ),
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    "Forge run admission worker disconnected before returning.".to_string()
                }
            };
            record_first_mission_run_start_failure(
                &thread_kernel,
                &thread_tenant_id,
                &thread_actor_id,
                &thread_mission_id,
                &thread_started_receipt_id,
                &thread_brief_receipt_id,
                &thread_page_receipt_id,
                &thread_page_id,
                &thread_run_id_for_body,
                &failure_reason,
            );
            crate::kernel_snapshot::snapshot_at_boundary(&thread_kernel);
        })
    {
        record_first_mission_run_start_failure(
            &kernel,
            &tenant_id,
            &actor_id,
            &mission_id,
            &started_receipt_id,
            &brief_receipt_id,
            &page_receipt_id,
            &page_id,
            &run_id,
            &format!("could not start the first mission Forge admission thread: {error}"),
        );
    }
}

fn first_mission_admission_timeout() -> Duration {
    std::env::var("MDX_FIRST_MISSION_ADMISSION_TIMEOUT_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|seconds| *seconds > 0)
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_secs(DEFAULT_FIRST_MISSION_ADMISSION_TIMEOUT_SECS))
}

#[allow(clippy::too_many_arguments)]
fn record_first_mission_run_admitted(
    kernel: &Arc<RwLock<MdxKernel>>,
    tenant_id: &str,
    actor_id: &str,
    mission_id: &str,
    started_receipt_id: &str,
    brief_receipt_id: &str,
    page_receipt_id: &str,
    run_id: &str,
    run_receipt_id: &str,
    run_status: &str,
    detail: &str,
) {
    let Ok(mut guard) = kernel.write() else {
        return;
    };
    if latest_for_mission(&guard, ACTIVATION_FIRST_MISSION_RESULT_RECORDED, mission_id).is_some()
        || latest_for_mission(&guard, ACTIVATION_FIRST_MISSION_COMPLETED, mission_id).is_some()
    {
        return;
    }
    let _ = guard.record_activation_event_with_identity(
        ActivationEvent {
            tenant_id,
            actor_id,
            kind: ACTIVATION_FIRST_MISSION_RUN_ADMITTED,
            step: "first_mission_run",
            surface: "forge",
            detail,
            starter_workspace_id: "",
            activation_seed: false,
            fields: &[
                ("mission_id", mission_id),
                ("started_receipt_id", started_receipt_id),
                ("brief_receipt_id", brief_receipt_id),
                ("page_receipt_id", page_receipt_id),
                ("run_id", run_id),
                ("run_receipt_id", run_receipt_id),
                ("run_status", run_status),
                ("run_available", "true"),
            ],
        },
        &GovernedWriteIdentity::local_demo(actor_id),
    );
}

struct FirstMissionActivationCompletion<'a> {
    tenant_id: &'a str,
    actor_id: &'a str,
    mission_id: &'a str,
    started_receipt_id: &'a str,
    brief_receipt_id: &'a str,
    page_receipt_id: &'a str,
    run_id: &'a str,
    run_status: &'a str,
    completed_receipt_id: &'a str,
    result_receipt_id: &'a str,
}

fn record_first_mission_activation_completion(
    guard: &mut MdxKernel,
    identity: &GovernedWriteIdentity,
    completion: FirstMissionActivationCompletion<'_>,
) {
    let starter_workspace_id = format!("{}_workspace", completion.mission_id);
    let starter_receipt_id = if let Some(receipt) = latest_for_mission(
        guard,
        ACTIVATION_STARTER_WORKSPACE_SEEDED,
        completion.mission_id,
    ) {
        receipt.receipt_id.clone()
    } else {
        let Ok(report) = guard.record_activation_event_with_identity(
            ActivationEvent {
                tenant_id: completion.tenant_id,
                actor_id: completion.actor_id,
                kind: ACTIVATION_STARTER_WORKSPACE_SEEDED,
                step: "starter_workspace",
                surface: "welcome",
                detail: "Recorded the completed first mission as the real starter workspace proof.",
                starter_workspace_id: &starter_workspace_id,
                activation_seed: false,
                fields: &[
                    ("mission_id", completion.mission_id),
                    ("started_receipt_id", completion.started_receipt_id),
                    ("brief_receipt_id", completion.brief_receipt_id),
                    ("page_receipt_id", completion.page_receipt_id),
                    ("run_id", completion.run_id),
                    ("run_status", completion.run_status),
                    ("completed_receipt_id", completion.completed_receipt_id),
                    ("result_receipt_id", completion.result_receipt_id),
                    ("completion_source", "first_mission"),
                ],
            },
            identity,
        ) else {
            return;
        };
        report.receipt_id
    };
    if latest_for_mission(
        guard,
        ACTIVATION_FIRST_PROOF_RECORDED,
        completion.mission_id,
    )
    .is_some()
    {
        return;
    }
    let _ = guard.record_activation_event_with_identity(
        ActivationEvent {
            tenant_id: completion.tenant_id,
            actor_id: completion.actor_id,
            kind: ACTIVATION_FIRST_PROOF_RECORDED,
            step: "first_proof",
            surface: "welcome",
            detail: "Recorded the first mission terminal evidence as the user's first proof.",
            starter_workspace_id: &starter_workspace_id,
            activation_seed: false,
            fields: &[
                ("mission_id", completion.mission_id),
                ("starter_workspace_receipt_id", &starter_receipt_id),
                ("started_receipt_id", completion.started_receipt_id),
                ("brief_receipt_id", completion.brief_receipt_id),
                ("page_receipt_id", completion.page_receipt_id),
                ("run_id", completion.run_id),
                ("run_status", completion.run_status),
                ("completed_receipt_id", completion.completed_receipt_id),
                ("result_receipt_id", completion.result_receipt_id),
                ("proof_source", "first_mission"),
            ],
        },
        identity,
    );
}

#[allow(clippy::too_many_arguments)]
fn record_first_mission_run_start_failure(
    kernel: &Arc<RwLock<MdxKernel>>,
    tenant_id: &str,
    actor_id: &str,
    mission_id: &str,
    started_receipt_id: &str,
    brief_receipt_id: &str,
    page_receipt_id: &str,
    page_id: &str,
    run_id: &str,
    fallback_reason: &str,
) {
    let Ok(mut guard) = kernel.write() else {
        return;
    };
    if latest_for_mission(&guard, ACTIVATION_FIRST_MISSION_RESULT_RECORDED, mission_id).is_some()
        || latest_for_mission(&guard, ACTIVATION_FIRST_MISSION_COMPLETED, mission_id).is_some()
    {
        return;
    }
    let action_request_id = format!("{mission_id}_review_action");
    let action_payload = format!(
        r#"{{"kind":"first_mission_review","mission_id":{},"run_id":{},"page_id":{},"run_status":"fallback","fallback_reason":{},"production_write_allowed":false}}"#,
        json_string_literal(mission_id),
        json_string_literal(run_id),
        json_string_literal(page_id),
        json_string_literal(fallback_reason),
    );
    let Ok(message) = guard.save_message_action_request_local_with_identity(
        MessageActionRequest {
            tenant_id,
            actor_id,
            thread_id: "activation_first_mission_thread",
            channel_id: "activation_first_mission",
            action_request_id: &action_request_id,
            source_receipt_id: page_receipt_id,
            title: "First mission needs attention",
            summary: "MDx shaped the first mission and saved the brief, but Forge could not start the live build.",
            proposed_action: "Open the run setup, fix the blocker, or continue with the starter workspace.",
            action_payload: &action_payload,
        },
        &GovernedWriteIdentity::local_demo(actor_id),
    ) else {
        return;
    };
    let Ok(completed) = guard.record_activation_event(ActivationEvent {
        tenant_id,
        actor_id,
        kind: ACTIVATION_FIRST_MISSION_COMPLETED,
        step: "first_mission_payoff",
        surface: "welcome",
        detail: "Recorded the first-mission fallback chain after Forge run admission failed.",
        starter_workspace_id: "",
        activation_seed: false,
        fields: &[
            ("mission_id", mission_id),
            ("started_receipt_id", started_receipt_id),
            ("brief_receipt_id", brief_receipt_id),
            ("page_receipt_id", page_receipt_id),
            ("run_id", run_id),
            ("run_receipt_id", ""),
            ("run_status", "fallback"),
            ("run_available", "true"),
            ("fallback_reason", fallback_reason),
            ("message_action_request_id", &action_request_id),
            ("message_receipt_id", &message.action_request_receipt_id),
        ],
    }) else {
        return;
    };
    let identity = GovernedWriteIdentity::local_demo(actor_id);
    record_first_mission_activation_completion(
        &mut guard,
        &identity,
        FirstMissionActivationCompletion {
            tenant_id,
            actor_id,
            mission_id,
            started_receipt_id,
            brief_receipt_id,
            page_receipt_id,
            run_id,
            run_status: "fallback",
            completed_receipt_id: &completed.receipt_id,
            result_receipt_id: "",
        },
    );
}

pub(crate) struct ActivationSetupProjectionStatus {
    pub(crate) status: &'static str,
    pub(crate) setup_complete: bool,
    pub(crate) complete_steps: usize,
    pub(crate) total_steps: usize,
}

pub(crate) fn activation_setup_projection_status(
    kernel: &MdxKernel,
) -> ActivationSetupProjectionStatus {
    let model_ready = crate::install_setup_track::model_connected_for_setup(kernel, global());
    let complete_steps = [
        latest(kernel, ACTIVATION_PROFILE_SAVED).is_some(),
        model_ready,
        latest(kernel, ACTIVATION_FORGE_WORKSPACE_SAVED).is_some(),
        latest(kernel, ACTIVATION_STARTER_WORKSPACE_SEEDED).is_some(),
        latest(kernel, ACTIVATION_FIRST_PROOF_RECORDED).is_some(),
    ]
    .into_iter()
    .filter(|done| *done)
    .count();
    let status = if complete_steps == 5 {
        "READY"
    } else if complete_steps > 0 {
        "IN_PROGRESS"
    } else {
        "NOT_STARTED"
    };
    ActivationSetupProjectionStatus {
        status,
        setup_complete: complete_steps == 5,
        complete_steps,
        total_steps: 5,
    }
}

fn render_projection_json(kernel: &MdxKernel) -> String {
    let provider_projection =
        crate::forge_model_provider_routing::provider_projection(kernel, global());
    let provider_value = serde_json::from_str::<serde_json::Value>(&provider_projection)
        .unwrap_or(serde_json::Value::Null);
    let connected_provider_count = provider_value
        .get("ready_provider_count")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let roles_runnable = provider_value
        .get("roles")
        .and_then(serde_json::Value::as_array)
        .map(|roles| {
            roles
                .iter()
                .filter(|role| {
                    role.get("credential_available")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false)
                })
                .count()
        })
        .unwrap_or(0);
    let profile = latest(kernel, ACTIVATION_PROFILE_SAVED);
    let owner = latest(kernel, mdx_core::INSTALL_OWNER_RECEIPT_KIND);
    let model_setup = latest(kernel, ACTIVATION_MODEL_SETUP_RECORDED);
    let workspace = latest(kernel, ACTIVATION_FORGE_WORKSPACE_SAVED);
    let starter = latest(kernel, ACTIVATION_STARTER_WORKSPACE_SEEDED);
    let first_proof = latest(kernel, ACTIVATION_FIRST_PROOF_RECORDED);
    let model_ready = crate::install_setup_track::model_connected_for_setup(kernel, global());
    let steps = [
        step_json("profile", "Twin knows you", profile, false),
        step_json("model_setup", "Models can run", model_setup, model_ready),
        step_json("forge_workspace", "Forge workspace", workspace, false),
        step_json("starter_workspace", "Starter workspace", starter, false),
        step_json("first_proof", "First proof", first_proof, false),
    ]
    .join(",");
    let activation_status = activation_setup_projection_status(kernel);
    let starter_workspace_id = starter
        .and_then(|receipt| receipt.payload.get("starter_workspace_id"))
        .cloned()
        .unwrap_or_default();
    let starter_activation_seed = starter
        .map(|receipt| payload_value(receipt, "activation_seed") == "true")
        .unwrap_or(false);
    let starter_honesty_note = if starter_activation_seed {
        "Starter data is labeled. Your real work replaces it."
    } else {
        "First mission completion is recorded as the real starter workspace proof."
    };
    let profile_json = profile_object(profile);
    let install_owner_json = install_owner_json(owner);
    let local_model_recommendations = local_model_recommendations_json(model_setup);
    let proofs = [
        proof_json("install_claimed", "Install claimed", owner, None),
        proof_json("twin_personalized", "Twin personalized", profile, None),
        proof_json(
            "model_lane_ready",
            "A model lane is runnable",
            model_setup,
            Some(model_ready),
        ),
        proof_json(
            "forge_workspace_ready",
            "Forge workspace ready",
            workspace,
            None,
        ),
        proof_json(
            "starter_workspace_seeded",
            "Starter workspace seeded",
            starter,
            None,
        ),
        proof_json(
            "pages_source_available",
            "Pages source available",
            latest(kernel, "pages.document.published"),
            None,
        ),
        proof_json(
            "message_action_card_works",
            "Message action card works",
            latest(kernel, "message.action.requested"),
            None,
        ),
        proof_json(
            "first_lesson_available",
            "First lesson available",
            latest(kernel, "learning.memory.activated"),
            None,
        ),
        proof_json(
            "marketplace_install_recorded",
            "Starter pack installed",
            latest(kernel, "marketplace.act.recorded"),
            None,
        ),
        proof_json("first_proof_seen", "First proof seen", first_proof, None),
    ]
    .join(",");
    let safe_next_move = match activation_status.status {
        "READY" => "Start with Twin or hand Forge real work.",
        "IN_PROGRESS" => "Finish the next unchecked setup step.",
        _ => "Personalize Twin, connect a model, then prepare Forge.",
    };
    format!(
        r#"{{"name":"mdx-activation-projection","status":{},"route":{},"writes_routes":[{},{},{},{},{}],"read_only":true,"receipt_backed":true,"tenant_id":"local_tenant","profile":{},"install_owner":{},"progress":{{"complete_steps":{},"total_steps":5,"steps":[{}]}},"model_lanes":{{"connected_provider_count":{},"roles_runnable":{},"ollama_runtime_detected":{},"private_by_default_supported":true,"secret_values_recorded":false,"provider_projection_route":"/forge/model-providers.json","recommendations":[{}]}},"starter_workspace":{{"starter_workspace_id":{},"activation_seed":{},"honesty_note":{}}},"proofs":[{}],"safe_next_move":{},"authority_granted":false,"adaptation_allowed":false,"production_write_allowed":false}}"#,
        json_string_literal(activation_status.status),
        json_string_literal(PROJECTION_ROUTE),
        json_string_literal(PROFILE_ROUTE),
        json_string_literal(MODEL_SETUP_ROUTE),
        json_string_literal(FORGE_WORKSPACE_ROUTE),
        json_string_literal(STARTER_WORKSPACE_ROUTE),
        json_string_literal(FIRST_PROOF_ROUTE),
        profile_json,
        install_owner_json,
        activation_status.complete_steps,
        steps,
        connected_provider_count,
        roles_runnable,
        ollama_runtime_ready(),
        local_model_recommendations,
        json_string_literal(&starter_workspace_id),
        starter_activation_seed,
        json_string_literal(starter_honesty_note),
        proofs,
        json_string_literal(safe_next_move),
    )
}

fn render_profile_json(kernel: &MdxKernel) -> String {
    let profile = latest(kernel, ACTIVATION_PROFILE_SAVED);
    let profile_json = profile_object(profile);
    let receipt_id = profile
        .map(|receipt| receipt.receipt_id.as_str())
        .unwrap_or("");
    format!(
        r#"{{"name":"mdx-activation-profile","status":{},"route":{},"writes_route":{},"read_only":true,"receipt_backed":true,"recorded":{},"profile":{},"receipt_id":{},"secret_values_recorded":false,"adaptation_allowed":false,"production_write_allowed":false}}"#,
        json_string_literal(if profile.is_some() {
            "PROFILE_RECORDED"
        } else {
            "NOT_RECORDED"
        }),
        json_string_literal(PROFILE_ROUTE),
        json_string_literal(PROFILE_ROUTE),
        profile.is_some(),
        profile_json,
        json_string_literal(receipt_id),
    )
}

fn render_first_mission_projection_json(kernel: &MdxKernel) -> String {
    let starters = first_mission_starters_json(kernel);
    let Some(started) = latest(kernel, ACTIVATION_FIRST_MISSION_STARTED) else {
        return format!(
            r#"{{"name":"mdx-activation-first-mission-projection","status":"not_started","route":{},"writes_route":{},"starters":[{}],"capabilities":{},"mission":null,"receipt_backed":true,"receipts":[],"activation_seed":false,"fallback_available":true,"production_write_allowed":false}}"#,
            json_string_literal(FIRST_MISSION_PROJECTION_ROUTE),
            json_string_literal(FIRST_MISSION_START_ROUTE),
            starters,
            first_mission_capabilities_json(kernel),
        );
    };
    let mission_id = payload_value(started, "mission_id");
    let brief = latest_for_mission(kernel, ACTIVATION_FIRST_MISSION_BRIEF_SHAPED, mission_id);
    let run_admitted =
        latest_for_mission(kernel, ACTIVATION_FIRST_MISSION_RUN_ADMITTED, mission_id);
    let completed = latest_for_mission(kernel, ACTIVATION_FIRST_MISSION_COMPLETED, mission_id);
    let result = latest_for_mission(kernel, ACTIVATION_FIRST_MISSION_RESULT_RECORDED, mission_id);
    let run_id = result
        .map(|receipt| payload_value(receipt, "run_id"))
        .filter(|value| !value.trim().is_empty())
        .or_else(|| run_admitted.map(|receipt| payload_value(receipt, "run_id")))
        .filter(|value| !value.trim().is_empty())
        .or_else(|| completed.map(|receipt| payload_value(receipt, "run_id")))
        .unwrap_or("");
    let run_events = forge_run_events_for(kernel, run_id);
    let latest_run_event = run_events
        .last()
        .map(|receipt| payload_value(receipt, "event"))
        .unwrap_or("");
    let completed_run_status = completed
        .map(|receipt| payload_value(receipt, "run_status"))
        .unwrap_or("");
    let admitted_run_status = run_admitted
        .map(|receipt| payload_value(receipt, "run_status"))
        .unwrap_or("");
    let result_run_status = result
        .map(|receipt| payload_value(receipt, "run_status"))
        .unwrap_or("");
    let result_proof_passed = result
        .map(|receipt| payload_value(receipt, "proof_passed") == "true")
        .unwrap_or(false);
    let status = if result_run_status == "RUN_FINISHED_DONE" && result_proof_passed {
        "done"
    } else if result.is_some() {
        "failed"
    } else if completed_run_status == "fallback" {
        "fallback"
    } else if latest_run_event == "run_finished" {
        "finalizing"
    } else if admitted_run_status == "admitting" && latest_run_event.is_empty() {
        "admitting"
    } else if !run_id.is_empty() {
        "running"
    } else if brief.is_some() {
        "landed"
    } else {
        "shaping"
    };
    let page_receipt_id =
        first_non_empty_payload(&[result, run_admitted, completed], "page_receipt_id")
            .or_else(|| first_mission_page_receipt_id(kernel, mission_id))
            .unwrap_or_default();
    let page_receipt = kernel.ledger().query().by_id(&page_receipt_id);
    let page = if let Some(page_receipt) = page_receipt {
        format!(
            r#"{{"page_id":{},"title":{},"url":{},"receipt_id":{}}}"#,
            json_string_literal(payload_value(page_receipt, "document_id")),
            json_string_literal(payload_value(page_receipt, "title")),
            json_string_literal(&format!(
                "/pages?document_id={}",
                payload_value(page_receipt, "document_id")
            )),
            json_string_literal(&page_receipt.receipt_id),
        )
    } else {
        "null".to_string()
    };
    let shaping_json =
        first_mission_shaping_json(brief, mission_id, started_is_live_shaping(started));
    let run_json = first_mission_run_json(run_admitted, completed, result, run_id, &run_events);
    let message_json = if let Some(result) = result {
        format!(
            r#"{{"channel_id":"activation_first_mission","action_request_id":{},"card_id":{},"verbs":["approve","respond","reject"],"receipt_id":{},"terminal":true,"run_status":{},"url":"/message?channel_id=activation_first_mission"}}"#,
            json_string_literal(payload_value(result, "message_action_request_id")),
            json_string_literal(payload_value(result, "message_action_request_id")),
            json_string_literal(payload_value(result, "message_receipt_id")),
            json_string_literal(payload_value(result, "run_status")),
        )
    } else if let Some(completed) = completed.filter(|receipt| {
        payload_value(receipt, "run_status") == "fallback"
            || payload_value(receipt, "run_status") == "not_started"
    }) {
        format!(
            r#"{{"channel_id":"activation_first_mission","action_request_id":{},"card_id":{},"verbs":["approve","respond","reject"],"receipt_id":{},"terminal":false,"run_status":{},"url":"/message?channel_id=activation_first_mission"}}"#,
            json_string_literal(payload_value(completed, "message_action_request_id")),
            json_string_literal(payload_value(completed, "message_action_request_id")),
            json_string_literal(payload_value(completed, "message_receipt_id")),
            json_string_literal(payload_value(completed, "run_status")),
        )
    } else {
        "null".to_string()
    };
    let receipts = first_mission_receipts_json(
        started,
        brief,
        page_receipt,
        run_admitted,
        completed,
        result,
        &run_events,
    );
    format!(
        r#"{{"name":"mdx-activation-first-mission-projection","status":{},"route":{},"writes_route":{},"starters":[{}],"capabilities":{},"mission":{{"mission_id":{},"ask":{},"starter_id":{},"repo_target":{},"repo_id":{},"shaping":{},"brief":{},"persona_contributions":[{}],"page":{},"run":{},"message":{},"receipts":[{}]}},"activation_seed":false,"fallback_available":true,"production_write_allowed":false}}"#,
        json_string_literal(status),
        json_string_literal(FIRST_MISSION_PROJECTION_ROUTE),
        json_string_literal(FIRST_MISSION_START_ROUTE),
        starters,
        first_mission_capabilities_json(kernel),
        json_string_literal(mission_id),
        json_string_literal(payload_value(started, "ask")),
        json_string_literal(payload_value(started, "starter_id")),
        json_string_literal(payload_value(started, "repo_target")),
        json_string_literal(payload_value(started, "repo_id")),
        shaping_json,
        first_mission_brief_json(brief),
        first_mission_personas_json(brief),
        page,
        run_json,
        message_json,
        receipts,
    )
}

fn first_mission_brief_json(brief: Option<&Receipt>) -> String {
    let Some(brief) = brief else {
        return "null".to_string();
    };
    format!(
        r#"{{"goal":{},"approach":{},"acceptance":{},"proof_command":{},"shaping_mode":{},"receipt_id":{}}}"#,
        json_string_literal(payload_value(brief, "goal")),
        json_string_literal(payload_value(brief, "approach")),
        json_string_literal(payload_value(brief, "acceptance")),
        json_string_literal(payload_value(brief, "proof_command")),
        json_string_literal(payload_value(brief, "shaping_mode")),
        json_string_literal(&brief.receipt_id),
    )
}

fn first_mission_shaping_json(
    brief: Option<&Receipt>,
    mission_id: &str,
    live_requested: bool,
) -> String {
    let mode = brief
        .map(|receipt| payload_value(receipt, "shaping_mode"))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(if live_requested {
            "live_personas"
        } else {
            "curated_tier0_no_live_search"
        });
    let status = if brief.is_some() { "ready" } else { "shaping" };
    format!(
        r#"{{"status":{},"mode":{},"stream_route":{}}}"#,
        json_string_literal(status),
        json_string_literal(mode),
        json_string_literal(&first_mission_shaping_stream_route(mission_id)),
    )
}

fn first_mission_shaping_stream_route(mission_id: &str) -> String {
    if mission_id.trim().is_empty() {
        "/activation/first-mission/shaping/stream".to_string()
    } else {
        format!("/activation/first-mission/shaping/stream?mission_id={mission_id}")
    }
}

fn started_is_live_shaping(started: &Receipt) -> bool {
    payload_value(started, "shaping_mode") == "live_personas"
}

fn first_mission_personas_json(brief: Option<&Receipt>) -> String {
    let Some(brief) = brief else {
        return String::new();
    };
    [
        ("twin_architect", "Architect", "architect_contribution"),
        ("twin_coder", "Coder", "coder_contribution"),
        ("twin_advisor", "Advisor", "advisor_contribution"),
    ]
    .iter()
    .map(|(id, label, key)| {
        format!(
            r#"{{"companion_id":{},"label":{},"line":{},"receipt_id":{}}}"#,
            json_string_literal(id),
            json_string_literal(label),
            json_string_literal(payload_value(brief, key)),
            json_string_literal(&brief.receipt_id),
        )
    })
    .collect::<Vec<_>>()
    .join(",")
}

fn first_mission_run_json(
    run_admitted: Option<&Receipt>,
    completed: Option<&Receipt>,
    result: Option<&Receipt>,
    run_id: &str,
    run_events: &[&Receipt],
) -> String {
    let fallback_reason = completed
        .map(|receipt| payload_value(receipt, "fallback_reason"))
        .unwrap_or("");
    let result_status = result
        .map(|receipt| payload_value(receipt, "run_status"))
        .unwrap_or("");
    let admitted_status = run_admitted
        .map(|receipt| payload_value(receipt, "run_status"))
        .unwrap_or("");
    let latest_event = run_events
        .last()
        .map(|receipt| payload_value(receipt, "event"))
        .unwrap_or("");
    let result_proof_passed = result
        .map(|receipt| payload_value(receipt, "proof_passed") == "true")
        .unwrap_or(false);
    let status = if result_status == "RUN_FINISHED_DONE" && result_proof_passed {
        "done"
    } else if result.is_some() {
        "failed"
    } else if !fallback_reason.is_empty() {
        "fallback"
    } else if latest_event == "run_finished" {
        "finalizing"
    } else if admitted_status == "admitting" && latest_event.is_empty() {
        "admitting"
    } else if !run_id.is_empty() {
        "running"
    } else {
        "not_started"
    };
    let branch = result
        .map(|receipt| payload_value(receipt, "branch"))
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .or_else(|| first_mission_branch_from_run_events(run_events))
        .unwrap_or_default();
    let files_changed = result
        .map(|receipt| payload_value(receipt, "files_changed"))
        .filter(|value| !value.trim().is_empty())
        .or_else(|| first_mission_files_changed_from_run_events(run_events))
        .unwrap_or("0");
    let proof_passed = run_events
        .iter()
        .any(|receipt| payload_value(receipt, "event") == "check_passed");
    let model = first_mission_run_model_json(run_events);
    let diff_summary = if branch.is_empty() {
        "Diff appears after the Forge worker writes a branch.".to_string()
    } else if files_changed == "1" {
        "Branch ready for review with 1 changed file.".to_string()
    } else {
        format!("Branch ready for review with {files_changed} changed files.")
    };
    format!(
        r#"{{"run_id":{},"status":{},"stage":{},"stages":[{}],"model":{},"branch":{},"diff_summary":{},"proof_passed":{},"terminal_message_recorded":{},"fallback_reason":{},"projection_route":"/forge/runs/projection.json","stream_route":{}}}"#,
        json_string_literal(run_id),
        json_string_literal(status),
        json_string_literal(first_mission_stage(latest_event, status)),
        first_mission_stages_json(run_events, status),
        model,
        json_string_literal(&branch),
        json_string_literal(&diff_summary),
        proof_passed,
        result.is_some(),
        json_string_literal(fallback_reason),
        json_string_literal(&forge_run_stream_route(run_id)),
    )
}

fn first_mission_run_model_json(run_events: &[&Receipt]) -> String {
    let Some(run_started) = run_events
        .iter()
        .rev()
        .find(|receipt| payload_value(receipt, "event") == "run_started")
    else {
        return "null".to_string();
    };
    let provider_family = payload_value(run_started, "builder_casting_selected_provider_family");
    let model_id = payload_value(run_started, "builder_casting_selected_model_id");
    if provider_family.is_empty() && model_id.is_empty() {
        return "null".to_string();
    }
    let slot = payload_value(run_started, "builder_casting_selected_slot");
    let model_class = payload_value(run_started, "builder_casting_selected_model_class");
    format!(
        r#"{{"provider_family":{},"model_id":{},"slot":{},"model_class":{},"source":"run_started_receipt","grants_execution_authority":false}}"#,
        json_string_literal(provider_family),
        json_string_literal(model_id),
        json_string_literal(slot),
        json_string_literal(model_class),
    )
}

fn forge_run_stream_route(run_id: &str) -> String {
    if run_id.trim().is_empty() {
        "/forge/runs/stream".to_string()
    } else {
        format!("/forge/runs/stream?run_id={run_id}")
    }
}

fn first_mission_stage(latest_event: &str, status: &str) -> &'static str {
    if status == "fallback" {
        "waiting_for_model"
    } else if status == "admitting" {
        "reading"
    } else if status == "finalizing" {
        "done"
    } else {
        match latest_event {
            "run_started" => "reading",
            "model_called" | "turn_executed" => "planning",
            "tool_executed" => "writing",
            "check_passed" | "check_failed" => "proof",
            "run_finished" => "done",
            _ => "ready",
        }
    }
}

fn first_mission_stages_json(run_events: &[&Receipt], status: &str) -> String {
    let latest = run_events
        .last()
        .map(|receipt| payload_value(receipt, "event"))
        .unwrap_or("");
    let succeeded = terminal_run_succeeded(run_events);
    let terminal = latest == "run_finished";
    let stage_status = |stage: &str| -> &'static str {
        if status == "fallback" {
            return if stage == "reading" {
                "blocked"
            } else {
                "pending"
            };
        }
        if status == "admitting" {
            return if stage == "reading" {
                "active"
            } else {
                "pending"
            };
        }
        match stage {
            "reading" if !latest.is_empty() => "done",
            "planning" if matches!(latest, "model_called" | "turn_executed") => "active",
            "planning"
                if matches!(
                    latest,
                    "tool_executed" | "check_passed" | "check_failed" | "run_finished"
                ) =>
            {
                "done"
            }
            "writing" if latest == "tool_executed" => "active",
            "writing" if matches!(latest, "check_passed" | "check_failed" | "run_finished") => {
                "done"
            }
            "proof" if matches!(latest, "check_passed" | "check_failed") => "active",
            "proof" if terminal && succeeded => "done",
            "done" if status == "finalizing" => "active",
            "proof" if terminal => "failed",
            "done" if terminal && succeeded => "done",
            "done" if terminal => "failed",
            "reading" if latest.is_empty() => "pending",
            "reading" => "active",
            _ => "pending",
        }
    };
    [
        ("reading", "Reading the repo"),
        ("planning", "Planning"),
        ("writing", "Writing the change"),
        ("proof", "Running the proof"),
        ("done", "Done"),
    ]
    .iter()
    .map(|(key, label)| {
        let state = stage_status(key);
        format!(
            r#"{{"key":{},"label":{},"state":{},"status":{},"detail":{}}}"#,
            json_string_literal(key),
            json_string_literal(label),
            json_string_literal(state),
            json_string_literal(state),
            json_string_literal(first_mission_stage_detail(key, latest, status)),
        )
    })
    .collect::<Vec<_>>()
    .join(",")
}

fn first_mission_stage_detail(stage: &str, latest_event: &str, status: &str) -> &'static str {
    if status == "fallback" {
        return "Connect a runnable Forge model to start the live worker.";
    }
    if status == "admitting" && stage == "reading" {
        return "Forge is admitting the run before the worker starts.";
    }
    if status == "finalizing" && stage == "done" && latest_event == "run_finished" {
        return "Forge reached a terminal run receipt. Recording the result for Message.";
    }
    if status == "failed" && stage == "done" && latest_event == "run_finished" {
        return "The run reached a terminal receipt, but it did not prove a successful change.";
    }
    match (stage, latest_event) {
        ("reading", "run_started") => "Forge accepted the work in an isolated copy.",
        ("planning", "model_called" | "turn_executed") => {
            "The builder is planning against the scoped brief."
        }
        ("writing", "tool_executed") => "The builder is editing in a throwaway worktree.",
        ("proof", "check_passed") => "A selected proof check passed.",
        ("proof", "check_failed") => "A selected proof check failed.",
        ("done", "run_finished") => "The run reached a terminal receipt.",
        _ => "",
    }
}

fn terminal_run_succeeded(run_events: &[&Receipt]) -> bool {
    run_events.iter().rev().any(|receipt| {
        payload_value(receipt, "event") == "run_finished"
            && payload_value(receipt, "detail").contains("status=RUN_FINISHED_DONE")
    })
}

fn first_mission_branch_from_run_events(run_events: &[&Receipt]) -> Option<String> {
    run_events.iter().rev().find_map(|receipt| {
        if payload_value(receipt, "event") != "evidence_appended" {
            return None;
        }
        let detail = payload_value(receipt, "detail");
        let branch = detail.strip_prefix("branch=")?.split_whitespace().next()?;
        (!branch.trim().is_empty()).then(|| branch.to_string())
    })
}

fn first_mission_files_changed_from_run_events<'a>(run_events: &'a [&Receipt]) -> Option<&'a str> {
    run_events.iter().rev().find_map(|receipt| {
        if payload_value(receipt, "event") != "run_finished" {
            return None;
        }
        let detail = payload_value(receipt, "detail");
        detail
            .split_whitespace()
            .find_map(|part| part.strip_prefix("files_changed="))
    })
}

fn first_mission_page_receipt_id(kernel: &MdxKernel, mission_id: &str) -> Option<String> {
    let document_id = format!("{mission_id}_brief");
    kernel.ledger().entries().iter().rev().find_map(|receipt| {
        (receipt.kind == "pages.document.published"
            && payload_value(receipt, "document_id") == document_id)
            .then(|| receipt.receipt_id.clone())
    })
}

fn first_non_empty_payload(receipts: &[Option<&Receipt>], key: &str) -> Option<String> {
    receipts.iter().find_map(|receipt| {
        let value = payload_value((*receipt)?, key);
        (!value.trim().is_empty()).then(|| value.to_string())
    })
}

fn first_mission_receipts_json(
    started: &Receipt,
    brief: Option<&Receipt>,
    page: Option<&Receipt>,
    run_admitted: Option<&Receipt>,
    completed: Option<&Receipt>,
    result: Option<&Receipt>,
    run_events: &[&Receipt],
) -> String {
    let mut receipts = vec![receipt_pointer_json(started)];
    if let Some(brief) = brief {
        receipts.push(receipt_pointer_json(brief));
    }
    if let Some(page) = page {
        receipts.push(receipt_pointer_json(page));
    }
    if let Some(run_admitted) = run_admitted {
        receipts.push(receipt_pointer_json(run_admitted));
    }
    for event in run_events.iter().take(8) {
        receipts.push(receipt_pointer_json(event));
    }
    if let Some(completed) = completed {
        receipts.push(receipt_pointer_json(completed));
    }
    if let Some(result) = result {
        receipts.push(receipt_pointer_json(result));
    }
    receipts.join(",")
}

fn receipt_pointer_json(receipt: &Receipt) -> String {
    format!(
        r#"{{"receipt_id":{},"kind":{}}}"#,
        json_string_literal(&receipt.receipt_id),
        json_string_literal(&receipt.kind),
    )
}

pub(crate) fn record_first_mission_result_from_run_outcome(
    kernel: &Arc<RwLock<MdxKernel>>,
    request: &crate::forge_loop_runner::ForgeRunRequest,
    outcome: &crate::forge_loop_runner::ForgeRunOutcome,
) {
    if let Err(error) = record_first_mission_result_from_run_outcome_inner(kernel, request, outcome)
    {
        eprintln!(
            "forge run {} first-mission observer skipped: {error}",
            outcome.run_id
        );
    }
}

fn record_first_mission_result_from_run_outcome_inner(
    kernel: &Arc<RwLock<MdxKernel>>,
    request: &crate::forge_loop_runner::ForgeRunRequest,
    outcome: &crate::forge_loop_runner::ForgeRunOutcome,
) -> Result<(), String> {
    let (mission_id, terminal_receipt_id, observed_check_passed) = {
        let guard = kernel
            .read()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        let Some(started) = guard.ledger().entries().iter().rev().find(|receipt| {
            receipt.kind == "forge.run.event"
                && payload_value(receipt, "run_id") == outcome.run_id
                && payload_value(receipt, "event") == "run_started"
        }) else {
            return Ok(());
        };
        let mission_id = payload_value(started, "activation_first_mission_id");
        if mission_id.trim().is_empty() {
            return Ok(());
        }
        if latest_for_mission(&guard, ACTIVATION_FIRST_MISSION_RESULT_RECORDED, mission_id)
            .is_some()
        {
            return Ok(());
        }
        let terminal_receipt = guard.ledger().entries().iter().rev().find(|receipt| {
            receipt.kind == "forge.run.event"
                && payload_value(receipt, "run_id") == outcome.run_id
                && payload_value(receipt, "event") == "run_finished"
        });
        let Some(terminal_receipt) = terminal_receipt else {
            return Ok(());
        };
        let observed_check_passed = guard.ledger().entries().iter().rev().any(|receipt| {
            receipt.kind == "forge.run.event"
                && payload_value(receipt, "run_id") == outcome.run_id
                && payload_value(receipt, "event") == "check_passed"
        });
        (
            mission_id.to_string(),
            terminal_receipt.receipt_id.clone(),
            observed_check_passed,
        )
    };
    let mut guard = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    if latest_for_mission(
        &guard,
        ACTIVATION_FIRST_MISSION_RESULT_RECORDED,
        &mission_id,
    )
    .is_some()
    {
        return Ok(());
    }
    let action_request_id = format!("{mission_id}_result_action");
    let branch = outcome.branch.clone().unwrap_or_default();
    let commit_sha = outcome.commit_sha.clone().unwrap_or_default();
    let proof_passed = bool_text(observed_check_passed);
    let files_changed = outcome.files_changed.to_string();
    let turns_used = outcome.turns_used.to_string();
    let summary = if outcome.finish_summary.trim().is_empty() {
        format!("Forge ended the first mission with {}.", outcome.status)
    } else {
        cap_chars(&outcome.finish_summary, 700)
    };
    let successful_terminal = outcome.status == "RUN_FINISHED_DONE" && observed_check_passed;
    let action_payload = format!(
        r#"{{"kind":"first_mission_terminal_result","mission_id":{},"run_id":{},"run_status":{},"branch":{},"files_changed":{},"proof_passed":{},"production_write_allowed":false}}"#,
        json_string_literal(&mission_id),
        json_string_literal(&outcome.run_id),
        json_string_literal(outcome.status),
        json_string_literal(&branch),
        outcome.files_changed,
        observed_check_passed,
    );
    let message = guard
        .save_message_action_request_local_with_identity(
            MessageActionRequest {
                tenant_id: &request.tenant_id,
                actor_id: &request.actor_id,
                thread_id: "activation_first_mission_thread",
                channel_id: "activation_first_mission",
                action_request_id: &action_request_id,
                source_receipt_id: &terminal_receipt_id,
                title: if successful_terminal {
                    "First mission result is ready"
                } else {
                    "First mission needs attention"
                },
                summary: &summary,
                proposed_action: if successful_terminal {
                    "Review the branch and decide what to keep."
                } else {
                    "Open the run, read the terminal reason, and decide whether to retry or continue with the starter workspace."
                },
                action_payload: &action_payload,
            },
            &GovernedWriteIdentity::local_demo(&request.actor_id),
        )
        .map_err(|error| error.message())?;
    let mut completed_receipt_id =
        latest_for_mission(&guard, ACTIVATION_FIRST_MISSION_COMPLETED, &mission_id)
            .map(|receipt| receipt.receipt_id.clone())
            .unwrap_or_default();
    let page_receipt_id =
        latest_for_mission(&guard, ACTIVATION_FIRST_MISSION_RUN_ADMITTED, &mission_id)
            .map(|receipt| payload_value(receipt, "page_receipt_id"))
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
            .or_else(|| first_mission_page_receipt_id(&guard, &mission_id))
            .unwrap_or_default();
    if latest_for_mission(&guard, ACTIVATION_FIRST_MISSION_COMPLETED, &mission_id).is_none() {
        let started_receipt_id =
            latest_for_mission(&guard, ACTIVATION_FIRST_MISSION_STARTED, &mission_id)
                .map(|receipt| receipt.receipt_id.as_str())
                .unwrap_or("")
                .to_string();
        let brief_receipt_id =
            latest_for_mission(&guard, ACTIVATION_FIRST_MISSION_BRIEF_SHAPED, &mission_id)
                .map(|receipt| receipt.receipt_id.as_str())
                .unwrap_or("")
                .to_string();
        let run_receipt_id =
            latest_for_mission(&guard, ACTIVATION_FIRST_MISSION_RUN_ADMITTED, &mission_id)
                .map(|receipt| payload_value(receipt, "run_receipt_id"))
                .filter(|value| !value.trim().is_empty())
                .unwrap_or(terminal_receipt_id.as_str())
                .to_string();
        let completed = guard
            .record_activation_event(
                ActivationEvent {
                    tenant_id: &request.tenant_id,
                    actor_id: &request.actor_id,
                    kind: ACTIVATION_FIRST_MISSION_COMPLETED,
                    step: "first_mission_payoff",
                    surface: "welcome",
                    detail: "Recorded the first-mission terminal chain through Twin, Context, Forge, and Message.",
                    starter_workspace_id: "",
                    activation_seed: false,
                    fields: &[
                        ("mission_id", &mission_id),
                        ("started_receipt_id", &started_receipt_id),
                        ("brief_receipt_id", &brief_receipt_id),
                        ("page_receipt_id", &page_receipt_id),
                        ("run_id", &outcome.run_id),
                        ("run_receipt_id", &run_receipt_id),
                        ("run_status", outcome.status),
                        ("run_available", "true"),
                        ("fallback_reason", ""),
                        ("message_action_request_id", &action_request_id),
                        ("message_receipt_id", &message.action_request_receipt_id),
                    ],
                },
            )
            .map_err(|error| error.message())?;
        completed_receipt_id = completed.receipt_id;
    }
    let result = guard
        .record_activation_event(
            ActivationEvent {
                tenant_id: &request.tenant_id,
                actor_id: &request.actor_id,
                kind: ACTIVATION_FIRST_MISSION_RESULT_RECORDED,
                step: "first_mission_result",
                surface: "message",
                detail: "Recorded the terminal Forge result for the first mission and posted it to Message.",
                starter_workspace_id: "",
                activation_seed: false,
                fields: &[
                    ("mission_id", &mission_id),
                    ("run_id", &outcome.run_id),
                    ("run_status", outcome.status),
                    ("run_finished_receipt_id", &terminal_receipt_id),
                    ("completed_receipt_id", &completed_receipt_id),
                    ("page_receipt_id", &page_receipt_id),
                    ("branch", &branch),
                    ("commit_sha", &commit_sha),
                    ("files_changed", &files_changed),
                    ("turns_used", &turns_used),
                    ("proof_passed", proof_passed),
                    ("message_action_request_id", &action_request_id),
                    ("message_receipt_id", &message.action_request_receipt_id),
                ],
            },
        )
        .map_err(|error| error.message())?;
    let started_receipt_id =
        latest_for_mission(&guard, ACTIVATION_FIRST_MISSION_STARTED, &mission_id)
            .map(|receipt| receipt.receipt_id.clone())
            .unwrap_or_default();
    let brief_receipt_id =
        latest_for_mission(&guard, ACTIVATION_FIRST_MISSION_BRIEF_SHAPED, &mission_id)
            .map(|receipt| receipt.receipt_id.clone())
            .unwrap_or_default();
    let identity = GovernedWriteIdentity::local_demo(&request.actor_id);
    record_first_mission_activation_completion(
        &mut guard,
        &identity,
        FirstMissionActivationCompletion {
            tenant_id: &request.tenant_id,
            actor_id: &request.actor_id,
            mission_id: &mission_id,
            started_receipt_id: &started_receipt_id,
            brief_receipt_id: &brief_receipt_id,
            page_receipt_id: &page_receipt_id,
            run_id: &outcome.run_id,
            run_status: outcome.status,
            completed_receipt_id: &completed_receipt_id,
            result_receipt_id: &result.receipt_id,
        },
    );
    Ok(())
}

fn latest_for_mission<'a>(
    kernel: &'a MdxKernel,
    kind: &str,
    mission_id: &str,
) -> Option<&'a Receipt> {
    kernel.ledger().entries().iter().rev().find(|receipt| {
        receipt.kind == kind
            && receipt.payload.get("mission_id").map(String::as_str) == Some(mission_id)
    })
}

fn cap_chars(value: &str, max: usize) -> String {
    let mut out = String::new();
    for ch in value.chars().take(max) {
        out.push(ch);
    }
    out
}

fn forge_run_events_for<'a>(kernel: &'a MdxKernel, run_id: &str) -> Vec<&'a Receipt> {
    if run_id.trim().is_empty() {
        return Vec::new();
    }
    kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| {
            receipt.kind == "forge.run.event"
                && receipt.payload.get("run_id").map(String::as_str) == Some(run_id)
        })
        .collect()
}

fn first_mission_capabilities_json(kernel: &MdxKernel) -> String {
    let provider_projection =
        crate::forge_model_provider_routing::provider_projection(kernel, global());
    let provider_value = serde_json::from_str::<serde_json::Value>(&provider_projection)
        .unwrap_or(serde_json::Value::Null);
    let connected_provider_count = provider_value
        .get("ready_provider_count")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let roles_runnable = provider_value
        .get("roles")
        .and_then(serde_json::Value::as_array)
        .map(|roles| {
            roles
                .iter()
                .filter(|role| {
                    role.get("credential_available")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false)
                })
                .count()
        })
        .unwrap_or(0);
    let tenant_id = "local_tenant";
    let endpoints_json =
        crate::forge_model_provider_routing::endpoint_capabilities_json(kernel, tenant_id);
    let endpoint_values = serde_json::from_str::<serde_json::Value>(&format!("[{endpoints_json}]"))
        .unwrap_or(serde_json::Value::Array(Vec::new()));
    let endpoints = endpoint_values.as_array().cloned().unwrap_or_default();
    let connected_endpoint_count = endpoints.len();
    let coding_endpoint_count = endpoints
        .iter()
        .filter(|endpoint| {
            endpoint
                .get("coding_model")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
        })
        .count();
    let live_search_endpoint_count = endpoints
        .iter()
        .filter(|endpoint| {
            endpoint
                .get("web_search")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
                || endpoint
                    .get("google_search")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false)
        })
        .count();
    let x_search_endpoint_count = endpoints
        .iter()
        .filter(|endpoint| {
            endpoint
                .get("x_search")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
        })
        .count();
    let forge_builder_ready = TurnClient::any_builder_configured_for_tenant(kernel, tenant_id);
    let tier = if forge_builder_ready {
        "tier0_real_loop"
    } else {
        "fallback_seed_preview"
    };
    format!(
        r#"{{"tier":{},"runnable_model":{},"forge_builder_ready":{},"connected_provider_count":{},"connected_endpoint_count":{},"coding_endpoint_count":{},"roles_runnable":{},"live_search_available":{},"live_search_endpoint_count":{},"x_search_available":{},"x_search_endpoint_count":{},"local_model_supported":{},"endpoints":[{}],"honesty_note":"Tier 0 uses model and repo capability only. Live web and X search are detected for later tiers, never claimed unless a connected endpoint can serve them."}}"#,
        json_string_literal(tier),
        forge_builder_ready,
        forge_builder_ready,
        connected_provider_count,
        connected_endpoint_count,
        coding_endpoint_count,
        roles_runnable,
        live_search_endpoint_count > 0,
        live_search_endpoint_count,
        x_search_endpoint_count > 0,
        x_search_endpoint_count,
        ollama_runtime_ready(),
        endpoints_json,
    )
}

#[derive(Clone)]
struct FirstMissionStarter {
    id: String,
    title: String,
    ask: String,
    repo_target: String,
    repo_id: String,
    work_item_id: String,
    page_title: String,
    goal: String,
    approach: String,
    acceptance: String,
    proof_command: String,
    forge_intent: String,
    max_turns: u32,
    coder_contribution: String,
    architect_contribution: String,
    advisor_contribution: String,
}

fn first_mission_starter(kernel: &MdxKernel, id: &str) -> FirstMissionStarter {
    if let Some(starter) = external_first_mission_starter(kernel, id) {
        return starter;
    }
    curated_first_mission_starter(id)
}

fn curated_first_mission_starter(id: &str) -> FirstMissionStarter {
    match id {
        "tidy_doc" => FirstMissionStarter {
            id: "tidy_doc".to_string(),
            title: "Tidy a doc".to_string(),
            ask: "Tidy one onboarding doc so the first Forge run guidance is easier to follow.".to_string(),
            repo_target: "mdx_self".to_string(),
            repo_id: MDX_SELF_REPO_ID.to_string(),
            work_item_id: "tier0_dev_101".to_string(),
            page_title: "First Mission Brief - Tidy a Doc".to_string(),
            goal: "Make a tiny documentation improvement in the authorized Dev 101 onboarding path.".to_string(),
            approach: "Keep the change doc-only, touch one scoped file, and preserve existing guidance.".to_string(),
            acceptance: "The diff is small, readable, and the agent-facing contract check still passes.".to_string(),
            proof_command: "make agent-working-contract-check".to_string(),
            forge_intent: "In docs/DEV-101.md, make one tiny clarity improvement about naming a proof command before a Forge run starts. Keep it doc-only, minimal, and inside the tier0_dev_101 work item.".to_string(),
            max_turns: 12,
            coder_contribution: "Use a minimal one-file doc patch and avoid touching generated artifacts.".to_string(),
            architect_contribution: "Keep this inside the existing tier0_dev_101 write scope so MDx dogfood rules stay intact.".to_string(),
            advisor_contribution: "The user should see a safe real branch, not a broad rewrite.".to_string(),
        },
        "fix_lint_nit" => FirstMissionStarter {
            id: "fix_lint_nit".to_string(),
            title: "Fix a small wording nit".to_string(),
            ask: "Fix a small wording nit in the agent onboarding docs.".to_string(),
            repo_target: "mdx_self".to_string(),
            repo_id: MDX_SELF_REPO_ID.to_string(),
            work_item_id: "tier0_dev_101".to_string(),
            page_title: "First Mission Brief - Fix a Wording Nit".to_string(),
            goal: "Improve one sentence in the maintainer onboarding docs without expanding scope.".to_string(),
            approach: "Find a small clarity edit in docs/DEV-101.md and keep the result easy to review.".to_string(),
            acceptance: "The changed sentence is clearer and agent-working-contract validation passes.".to_string(),
            proof_command: "make agent-working-contract-check".to_string(),
            forge_intent: "In docs/DEV-101.md, improve one small awkward sentence related to scoped agent work. Keep the change tiny, doc-only, and inside the tier0_dev_101 work item.".to_string(),
            max_turns: 12,
            coder_contribution: "Prefer a one-sentence patch with no formatting churn.".to_string(),
            architect_contribution: "Avoid touching runtime, apps, or generated files.".to_string(),
            advisor_contribution: "This first mission should teach the review loop, not show off a large change.".to_string(),
        },
        _ => FirstMissionStarter {
            id: "add_small_test".to_string(),
            title: "Add a small test-minded note".to_string(),
            ask: "Add a small test-minded note to the maintainer onboarding docs.".to_string(),
            repo_target: "mdx_self".to_string(),
            repo_id: MDX_SELF_REPO_ID.to_string(),
            work_item_id: "tier0_dev_101".to_string(),
            page_title: "First Mission Brief - Add a Small Test-Minded Note".to_string(),
            goal: "Add a tiny note that reminds future Forge runs to name their proof before editing.".to_string(),
            approach: "Make a minimal doc-only change in docs/DEV-101.md under the authorized work item.".to_string(),
            acceptance: "The note is present, the diff is small, and the agent working contract check passes.".to_string(),
            proof_command: "make agent-working-contract-check".to_string(),
            forge_intent: "In docs/DEV-101.md, add one short note reminding maintainers to name the proof command before a Forge run starts editing. Keep the diff tiny, doc-only, and inside the tier0_dev_101 work item.".to_string(),
            max_turns: 12,
            coder_contribution: "One short note is enough. No generated files, no broad rewrite.".to_string(),
            architect_contribution: "This is authorized by tier0_dev_101 and has a cheap proof command.".to_string(),
            advisor_contribution: "The first mission should complete quickly and leave a reviewable branch.".to_string(),
        },
    }
}

fn external_first_mission_starter(
    kernel: &MdxKernel,
    starter_id: &str,
) -> Option<FirstMissionStarter> {
    let (repo_id, task_id) = parse_external_starter_id(starter_id)?;
    let root = kernel.forge_repo_root(repo_id)?;
    let root_path = Path::new(&root);
    if !root_path.is_dir() {
        return None;
    }
    let profile = crate::forge_repo_profile::profile_repo(root_path);
    let suggested_checks = profile
        .suggested_checks
        .iter()
        .map(|check| (*check).to_string())
        .collect::<Vec<_>>();
    let artifact_patterns = profile
        .artifact_patterns
        .iter()
        .map(|pattern| (*pattern).to_string())
        .collect::<Vec<_>>();
    let scout = crate::forge_repo_task_scout_route::scout_repo(
        root_path,
        &suggested_checks,
        &artifact_patterns,
    );
    scout
        .candidates
        .iter()
        .find(|candidate| candidate["task_id"].as_str() == Some(task_id))
        .and_then(|candidate| external_starter_from_candidate(repo_id, starter_id, candidate))
}

fn external_first_mission_starters_json(kernel: &MdxKernel) -> Vec<String> {
    let mut latest = std::collections::BTreeMap::<String, String>::new();
    for receipt in kernel.ledger().query().by_kind("forge.repo.connected") {
        let repo_id = payload_value(receipt, "repo_id");
        let root = payload_value(receipt, "root");
        if repo_id.trim().is_empty() || repo_id == MDX_SELF_REPO_ID || root.trim().is_empty() {
            continue;
        }
        latest.insert(repo_id.to_string(), root.to_string());
    }
    let mut starters = Vec::new();
    for (repo_id, root) in latest {
        if starters.len() >= 6 {
            break;
        }
        let root_path = Path::new(&root);
        if !root_path.is_dir() {
            continue;
        }
        let profile = crate::forge_repo_profile::profile_repo(root_path);
        let suggested_checks = profile
            .suggested_checks
            .iter()
            .map(|check| (*check).to_string())
            .collect::<Vec<_>>();
        let artifact_patterns = profile
            .artifact_patterns
            .iter()
            .map(|pattern| (*pattern).to_string())
            .collect::<Vec<_>>();
        let scout = crate::forge_repo_task_scout_route::scout_repo(
            root_path,
            &suggested_checks,
            &artifact_patterns,
        );
        for candidate in scout.candidates.iter().take(2) {
            let Some(task_id) = candidate["task_id"].as_str() else {
                continue;
            };
            let starter_id = format!("repo_scout::{repo_id}::{task_id}");
            if let Some(starter) = external_starter_from_candidate(&repo_id, &starter_id, candidate)
            {
                starters.push(first_mission_starter_json(&starter, "repo_task_scout"));
            }
            if starters.len() >= 6 {
                break;
            }
        }
    }
    starters
}

fn external_starter_from_candidate(
    repo_id: &str,
    starter_id: &str,
    candidate: &serde_json::Value,
) -> Option<FirstMissionStarter> {
    let title = candidate["title"].as_str()?.trim();
    let prompt = candidate["prompt_template"].as_str()?.trim();
    let path = candidate["path"].as_str().unwrap_or("").trim();
    let proof_command = candidate["suggested_checks"]
        .as_array()
        .and_then(|checks| {
            checks
                .iter()
                .filter_map(serde_json::Value::as_str)
                .find(|check| {
                    let check = check.trim();
                    !check.is_empty() && check != "operator-selected check"
                })
        })?
        .trim()
        .to_string();
    let path_line = if path.is_empty() {
        "the selected repo scout finding".to_string()
    } else {
        format!("{path}:{}", candidate["line"].as_u64().unwrap_or(1))
    };
    Some(FirstMissionStarter {
        id: starter_id.to_string(),
        title: title.to_string(),
        ask: title.to_string(),
        repo_target: "repo_id".to_string(),
        repo_id: repo_id.to_string(),
        work_item_id: String::new(),
        page_title: format!("First Mission Brief - {title}"),
        goal: title.to_string(),
        approach: format!(
            "Use the deterministic repo scout finding at {path_line}, keep the patch contained, and prove it with the repo's selected check."
        ),
        acceptance: format!(
            "The change addresses {path_line}, avoids artifact noise, and `{proof_command}` passes."
        ),
        proof_command: proof_command.clone(),
        forge_intent: format!(
            "{prompt}\n\nKeep the change contained to the finding and closely related tests. Prove it with `{proof_command}`."
        ),
        max_turns: 24,
        coder_contribution: "Start from the file-anchored scout finding and keep the diff narrow.".to_string(),
        architect_contribution: "Use the repo's detected proof command and avoid broad redesign during the first mission.".to_string(),
        advisor_contribution: "This should feel like useful real work in the user's repo, not a generic demo.".to_string(),
    })
}

fn parse_external_starter_id(starter_id: &str) -> Option<(&str, &str)> {
    let rest = starter_id.strip_prefix("repo_scout::")?;
    let (repo_id, task_id) = rest.split_once("::")?;
    (!repo_id.trim().is_empty() && !task_id.trim().is_empty()).then_some((repo_id, task_id))
}

fn first_mission_starters_json(kernel: &MdxKernel) -> String {
    let mut starters = ["add_small_test", "tidy_doc", "fix_lint_nit"]
        .iter()
        .map(|id| {
            let starter = curated_first_mission_starter(id);
            first_mission_starter_json(&starter, "curated_mdx_self")
        })
        .collect::<Vec<_>>();
    starters.extend(external_first_mission_starters_json(kernel));
    starters.join(",")
}

fn first_mission_starter_json(starter: &FirstMissionStarter, source: &str) -> String {
    format!(
        r#"{{"starter_id":{},"title":{},"ask":{},"repo_target":{},"repo_id":{},"work_item_id":{},"proof_command":{},"source":{},"safe":true}}"#,
        json_string_literal(&starter.id),
        json_string_literal(&starter.title),
        json_string_literal(&starter.ask),
        json_string_literal(&starter.repo_target),
        json_string_literal(&starter.repo_id),
        json_string_literal(&starter.work_item_id),
        json_string_literal(&starter.proof_command),
        json_string_literal(source),
    )
}

fn seed_flywheel_substrate(
    kernel: &mut MdxKernel,
    identity: &ActivationIdentity,
    starter_workspace_id: &str,
    starter_receipt_id: &str,
) -> Result<Vec<String>, String> {
    let mut receipts = Vec::new();
    let tenant_id = &identity.tenant_id;
    let actor_id = &identity.actor_id;
    let run_id = format!(
        "activation_seed_run_{}",
        kernel.ledger().entries().len() + 1
    );
    let started = kernel
        .record_forge_run_event_with_evidence_fields(
            ForgeRunEvent {
                tenant_id,
                actor_id,
                run_id: &run_id,
                event: "run_started",
                work_item_id: "activation_seed",
                detail: "starter workspace seeded",
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            },
            &identity.governed,
            &[
                ("activation_seed", "true"),
                ("starter_workspace_id", starter_workspace_id),
            ],
        )
        .map_err(|error| error.message())?;
    receipts.push(started.receipt_id.clone());
    let evidence = kernel
        .record_forge_run_event_with_evidence_fields(
            ForgeRunEvent {
                tenant_id,
                actor_id,
                run_id: &run_id,
                event: "evidence_appended",
                work_item_id: "activation_seed",
                detail: "active learning memory cited by starter Forge plan",
                turn: 1,
                input_tokens: 0,
                output_tokens: 0,
            },
            &identity.governed,
            &[
                ("activation_seed", "true"),
                ("starter_workspace_id", starter_workspace_id),
            ],
        )
        .map_err(|error| error.message())?;
    receipts.push(evidence.receipt_id.clone());
    let tdd_page = kernel
        .save_pages_publication_local_with_identity(
            PagesPublication {
                tenant_id,
                actor_id,
                document_id: "activation_seed_tdd_standard",
                title: "Starter TDD Standard",
                body_ref: "activation://starter-workspace/pages/tdd-standard",
                source_receipt_id: starter_receipt_id,
                revision_id: "activation_seed_tdd_standard_rev_001",
                page_type: "standard",
            },
            &identity.governed,
        )
        .map_err(|error| error.message())?;
    receipts.push(tdd_page.publication_receipt_id.clone());
    let workspace_page = kernel
        .save_pages_publication_local_with_identity(
            PagesPublication {
                tenant_id,
                actor_id,
                document_id: "activation_seed_workspace_note",
                title: "Starter Workspace Note",
                body_ref: "activation://starter-workspace/pages/workspace-note",
                source_receipt_id: starter_receipt_id,
                revision_id: "activation_seed_workspace_note_rev_001",
                page_type: "knowledge",
            },
            &identity.governed,
        )
        .map_err(|error| error.message())?;
    receipts.push(workspace_page.publication_receipt_id.clone());
    let action_payload = format!(
        r#"{{"kind":"starter_workspace_review","starter_workspace_id":{},"production_write_allowed":false}}"#,
        json_string_literal(starter_workspace_id)
    );
    let message_action = kernel
        .save_message_action_request_local_with_identity(
            MessageActionRequest {
                tenant_id,
                actor_id,
                thread_id: "activation_starter_thread",
                channel_id: "activation_starter_channel",
                action_request_id: "activation_seed_message_action",
                source_receipt_id: starter_receipt_id,
                title: "Review the starter workspace",
                summary: "MDx set up a starter workspace so you can see the loop before real work replaces it.",
                proposed_action: "Open the starter workspace payoff and decide what to try first.",
                action_payload: &action_payload,
            },
            &identity.governed,
        )
        .map_err(|error| error.message())?;
    receipts.push(message_action.action_request_receipt_id.clone());
    let outcome = kernel
        .record_forge_outcome_signal_with_identity(
            ForgeOutcomeSignal {
                tenant_id,
                actor_id,
                run_id: &run_id,
                source_receipt_id: &evidence.receipt_id,
                source_receipt_kind: "forge.run.event",
                disposition: "completed",
                summary: "Starter Forge run cited the seeded TDD standard.",
                capability_ids: "forge,context,pages",
                model_or_worker: "activation-seed",
                lesson_candidate: "When a repo has a TDD standard in Pages, Forge should cite it before planning tests.",
                lesson_source: "",
                message_channel_id: "activation_starter_channel",
            },
            &identity.governed,
        )
        .map_err(|error| error.message())?;
    receipts.push(outcome.receipt_id.clone());
    let judgment = kernel
        .record_learning_judgment_decision_with_identity(
            LearningJudgmentDecision {
                tenant_id,
                actor_id,
                judgment_id: "activation_seed_judgment",
                promotion_id: "activation_seed_memory_promotion",
                decision: "promote_candidate",
                rationale: "Starter workspace seed promotes the TDD-standard lesson for first-run demonstration.",
                evidence_refs: &outcome.receipt_id,
            },
            &identity.governed,
        )
        .map_err(|error| error.message())?;
    receipts.push(judgment.receipt_id.clone());
    let promotion = kernel
        .request_learning_memory_promotion_with_identity(
            LearningMemoryPromotion {
                tenant_id,
                actor_id,
                judgment_decision_id: &judgment.judgment_decision_id,
                judgment_decision_receipt_id: &judgment.receipt_id,
                judgment_id: "activation_seed_judgment",
                promotion_id: "activation_seed_memory_promotion",
                target_type: "page",
                target_path: "pages/engineering/tdd-standard",
                lesson_summary: "Forge plans should cite the team's TDD standard before selecting proof checks.",
                evidence_refs: &outcome.receipt_id,
                review_cadence: "review before activation",
                expiry_rule: "supersede with real work receipts",
            },
            &identity.governed,
        )
        .map_err(|error| error.message())?;
    receipts.push(promotion.receipt_id.clone());
    let memory = kernel
        .activate_learning_memory_with_identity(
            LearningMemoryActivation {
                tenant_id,
                actor_id,
                memory_promotion_id: &promotion.memory_promotion_id,
                memory_promotion_receipt_id: &promotion.receipt_id,
                judgment_decision_receipt_id: &judgment.receipt_id,
                target_type: "page",
                target_path: "pages/engineering/tdd-standard",
                lesson_summary: "Forge plans should cite the team's TDD standard before selecting proof checks.",
                evidence_refs: &outcome.receipt_id,
                activation_basis: "starter workspace seed for first-run demonstration",
                rollback_plan: "clear starter workspace seed or supersede with real work receipts",
                local_checks: "starter workspace proof only; no live model calls",
                approval_refs: starter_receipt_id,
                review_owner: actor_id,
            },
            &identity.governed,
        )
        .map_err(|error| error.message())?;
    receipts.push(memory.receipt_id.clone());
    let marketplace = kernel
        .save_marketplace_act(MarketplaceAct {
            tenant_id,
            actor_id,
            actor_role: "owner",
            act: "pack_install",
            source_route: "/marketplace/installs.json",
            capability_id: "activation_starter_pack",
            source_record_id: starter_receipt_id,
            scope: "local_starter_workspace",
            decision: "installed",
            read_only: true,
            pack_id: "mdx-starter-workspace",
            pack_version: "1.0.0",
            previous_version: "",
            application_targets: "twin,forge,pages,message",
            return_to: "/welcome/beta",
            origin_surface: "welcome",
            origin_object_id: starter_receipt_id,
            readiness_state: "ready",
            config_status: "complete",
            outcome: "starter_workspace_seeded",
            task_class: "onboarding",
            change_summary: "Install the local starter workspace pack.",
            permission_diff: "read-only local starter context",
            source_lane: "mdx",
            url: "",
            note: "Starter workspace pack installed for day-1 demonstration.",
            reason: "Light up the cross-surface loop without claiming real user work.",
            scan_status: "starter_seed_trusted_local",
            prompt_injection_scan: "not_applicable",
            signature_scan: "not_applicable",
            sbom_scan: "not_applicable",
            checksum: "",
            scan_findings: "",
            items_added: "2 pages, 1 action card, 1 Forge lesson",
            items_held: "",
            item_record_ids: "",
        })
        .map_err(|error| error.message())?;
    receipts.push(marketplace.act_receipt_id);
    Ok(receipts)
}

#[derive(Clone, Debug)]
struct ProviderInput {
    family: String,
    api_key: String,
    model_id: String,
}

fn provider_inputs(value: &serde_json::Value) -> Vec<ProviderInput> {
    let mut providers = Vec::new();
    if let Some(array) = value.get("providers").and_then(serde_json::Value::as_array) {
        for item in array {
            providers.push(provider_input_from_object(item));
        }
    }
    if let Some(array) = value
        .get("provider_keys")
        .and_then(serde_json::Value::as_array)
    {
        for item in array {
            providers.push(provider_input_from_object(item));
        }
    }
    if let Some(object) = value
        .get("provider_keys")
        .and_then(serde_json::Value::as_object)
    {
        for (family, key) in object {
            providers.push(ProviderInput {
                family: normalize_provider(family, key.as_str().unwrap_or_default()),
                api_key: key.as_str().unwrap_or_default().to_string(),
                model_id: String::new(),
            });
        }
    }
    if let Some(api_key) = string_field(value, "api_key") {
        let provider = string_field(value, "provider").unwrap_or_default();
        providers.push(ProviderInput {
            family: normalize_provider(&provider, &api_key),
            api_key,
            model_id: string_field(value, "model_id").unwrap_or_default(),
        });
    }
    providers
        .into_iter()
        .filter(|provider| {
            !provider.family.trim().is_empty() && !provider.api_key.trim().is_empty()
        })
        .collect()
}

fn provider_input_from_object(value: &serde_json::Value) -> ProviderInput {
    let api_key = string_field(value, "api_key")
        .or_else(|| string_field(value, "key"))
        .unwrap_or_default();
    let provider = string_field(value, "provider")
        .or_else(|| string_field(value, "family"))
        .unwrap_or_default();
    ProviderInput {
        family: normalize_provider(&provider, &api_key),
        api_key,
        model_id: string_field(value, "model_id")
            .or_else(|| string_field(value, "model"))
            .unwrap_or_default(),
    }
}

fn normalize_provider(provider: &str, key: &str) -> String {
    let provider = provider.trim().to_ascii_lowercase();
    if matches!(
        provider.as_str(),
        "openai" | "anthropic" | "xai" | "gemini" | "bedrock" | "ollama"
    ) {
        return provider;
    }
    let key = key.trim();
    if key.starts_with("sk-ant-") {
        "anthropic".to_string()
    } else if key.starts_with("xai-") {
        "xai".to_string()
    } else if key.starts_with("AIza") {
        "gemini".to_string()
    } else if key.starts_with("sk-") || key.starts_with("sk-proj-") {
        "openai".to_string()
    } else {
        provider
    }
}

struct ActivationIdentity {
    tenant_id: String,
    actor_id: String,
    governed: GovernedWriteIdentity,
}

fn activation_identity(body: &str) -> ActivationIdentity {
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    if resolved.auth_session_status == "VERIFIED_TRUSTED_SESSION" {
        ActivationIdentity {
            tenant_id: resolved.tenant_id.clone(),
            actor_id: resolved.actor_id.clone(),
            governed: GovernedWriteIdentity::local_demo(&resolved.actor_id),
        }
    } else {
        let value = json_value(body);
        let actor_id =
            string_field(&value, "actor_id").unwrap_or_else(|| "human:local_user".to_string());
        let tenant_id =
            string_field(&value, "tenant_id").unwrap_or_else(|| "local_tenant".to_string());
        ActivationIdentity {
            tenant_id,
            governed: GovernedWriteIdentity::local_demo(&actor_id),
            actor_id,
        }
    }
}

fn latest<'a>(kernel: &'a MdxKernel, kind: &str) -> Option<&'a Receipt> {
    kernel
        .ledger()
        .entries()
        .iter()
        .rev()
        .find(|receipt| receipt.kind == kind)
}

fn latest_starter_workspace_id(kernel: &Arc<RwLock<MdxKernel>>) -> Option<String> {
    kernel
        .read()
        .ok()
        .and_then(|kernel| latest(&kernel, ACTIVATION_STARTER_WORKSPACE_SEEDED).cloned())
        .and_then(|receipt| receipt.payload.get("starter_workspace_id").cloned())
}

fn step_json(key: &str, label: &str, receipt: Option<&Receipt>, externally_done: bool) -> String {
    let done = if key == "model_setup" {
        externally_done
    } else {
        receipt.is_some() || externally_done
    };
    let receipt_id = receipt
        .map(|receipt| receipt.receipt_id.as_str())
        .unwrap_or("");
    format!(
        r#"{{"key":{},"label":{},"status":{},"done":{},"receipt_id":{}}}"#,
        json_string_literal(key),
        json_string_literal(label),
        json_string_literal(if done { "done" } else { "waiting" }),
        done,
        json_string_literal(receipt_id),
    )
}

fn proof_json(
    key: &str,
    label: &str,
    receipt: Option<&Receipt>,
    externally_done: Option<bool>,
) -> String {
    let satisfied = externally_done.unwrap_or_else(|| receipt.is_some());
    let receipt_id = receipt
        .map(|receipt| receipt.receipt_id.as_str())
        .unwrap_or("");
    let source_kind = receipt.map(|receipt| receipt.kind.as_str()).unwrap_or("");
    format!(
        r#"{{"key":{},"label":{},"satisfied":{},"receipt_id":{},"source_kind":{}}}"#,
        json_string_literal(key),
        json_string_literal(label),
        satisfied,
        json_string_literal(receipt_id),
        json_string_literal(source_kind),
    )
}

fn profile_object(receipt: Option<&Receipt>) -> String {
    let Some(receipt) = receipt else {
        return r#"{"recorded":false}"#.to_string();
    };
    format!(
        r#"{{"recorded":true,"name":{},"role":{},"style":{{"register":{},"grounding":{},"guidance":{}}},"voice":{},"principles":{},"toneKeywords":{},"avoidPhrases":{},"receipt_id":{}}}"#,
        json_string_literal(payload_value(receipt, "profile_name")),
        json_string_literal(payload_value(receipt, "profile_role")),
        json_string_literal(payload_value(receipt, "profile_style_register")),
        json_string_literal(payload_value(receipt, "profile_style_grounding")),
        json_string_literal(payload_value(receipt, "profile_style_guidance")),
        json_string_literal(payload_value(receipt, "profile_voice")),
        json_string_literal(payload_value(receipt, "profile_principles")),
        json_string_literal(payload_value(receipt, "profile_tone_keywords")),
        json_string_literal(payload_value(receipt, "profile_avoid_phrases")),
        json_string_literal(&receipt.receipt_id),
    )
}

fn install_owner_json(receipt: Option<&Receipt>) -> String {
    let Some(receipt) = receipt else {
        return r#"{"claimed":false}"#.to_string();
    };
    format!(
        r#"{{"claimed":true,"owner_name":{},"owner_actor_id":{},"receipt_id":{}}}"#,
        json_string_literal(payload_value(receipt, "owner_name")),
        json_string_literal(payload_value(receipt, "owner_actor_id")),
        json_string_literal(&receipt.receipt_id),
    )
}

fn local_model_recommendations_json(model_setup: Option<&Receipt>) -> String {
    let private_twin = model_setup
        .map(|receipt| payload_value(receipt, "private_twin_model_recommendation"))
        .filter(|value| !value.is_empty())
        .unwrap_or("qwen3");
    let forge_coding = model_setup
        .map(|receipt| payload_value(receipt, "local_coding_model_recommendation"))
        .filter(|value| !value.is_empty())
        .unwrap_or("qwen2.5-coder");
    format!(
        r#"{{"lane":"private_twin","provider":"ollama","model_id":{},"purpose":"Sensitive Twin conversations and private document work","runtime_detected":{}}},{{"lane":"forge_coding","provider":"ollama","model_id":{},"purpose":"Local coding-grade Forge builder when frontier keys are unavailable or disallowed","runtime_detected":{}}}"#,
        json_string_literal(private_twin),
        ollama_runtime_ready(),
        json_string_literal(forge_coding),
        ollama_runtime_ready(),
    )
}

fn payload_value<'a>(receipt: &'a Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}

fn latest_repo_receipt_id(kernel: &MdxKernel, repo_id: &str) -> Option<String> {
    kernel
        .ledger()
        .query()
        .by_kind("forge.repo.connected")
        .iter()
        .rev()
        .find(|receipt| receipt.payload.get("repo_id").map(String::as_str) == Some(repo_id))
        .map(|receipt| receipt.receipt_id.clone())
}

fn mdx_self_repo_profile_json() -> String {
    r#"{"primary_language":"rust","language_pack_id":"rust-cargo","detected_language_packs":["rust-cargo","node"],"detected_files":["Cargo.toml","Makefile","package.json","apps/mdx-host"],"quality_signals":["workspace checks","generated contract checks","ui product surface checks"],"standards_sources":["AGENTS.md","docs/FORGE-OPERATOR-EXPERIENCE.md","docs/UI-PRODUCT-SURFACE-CONTRACT.md"],"standards_source_fingerprints":[],"standards_source_summaries":["MDx itself uses strict work-item scope and generated-contract checks; deeper repo intelligence can be refreshed after setup."],"review_axes":["correctness","tests","security","maintainability","generated-contract-drift"],"principal_review_gates":["behavior:changed_behavior_is_explicit","tests:checks_cover_changed_behavior","security:no_secret_or_authority_expansion","maintainability:generated_churn_justified"],"language_pack_guidance":["Prefer small scoped Rust changes with cargo fmt and targeted cargo tests.","For host UI changes run the UI product checks."],"semantic_intelligence":["rust-analyzer","ripgrep","generated-contract-index"],"semantic_tool_readiness":["rust-analyzer-ready","typescript-server-available"],"toolchain_readiness":["cargo","make","npm"],"proof_plan":{"status":"PROOF_PLAN_READY","next_action":"Run the checks named by the selected work item.","summary":"MDx self-work is available for onboarding, but stricter than external repos because agent work-queue scope still applies."},"suggested_checks":["cargo fmt --all --check","cargo check -p mdx-core -p mdx-server","make ui-product-surface-check"],"artifact_patterns":["target/**","node_modules/**","apps/mdx-host/.svelte-kit/**","apps/mdx-host/build/**"]}"#.to_string()
}

fn connect_mdx_self_repo_if_needed(
    kernel: &mut MdxKernel,
    identity: &ActivationIdentity,
) -> Result<String, String> {
    let root = mdx_self_repo_root()?;
    let repo_receipt_id = if kernel.forge_repo_root(MDX_SELF_REPO_ID).is_none() {
        kernel
            .connect_forge_repo_with_identity(
                ForgeRepoConnect {
                    tenant_id: &identity.tenant_id,
                    actor_id: &identity.actor_id,
                    repo_id: MDX_SELF_REPO_ID,
                    label: MDX_SELF_REPO_LABEL,
                    root: &root,
                    kind: "local",
                    origin: "",
                },
                &identity.governed,
            )
            .map(|report| report.receipt_id)
            .map_err(|error| error.message())?
    } else {
        latest_repo_receipt_id(kernel, MDX_SELF_REPO_ID).unwrap_or_default()
    };
    if kernel
        .forge_repo_index_receipt_id(MDX_SELF_REPO_ID)
        .is_none()
    {
        let profile_json = mdx_self_repo_profile_json();
        kernel
            .index_forge_repo_with_identity(
                ForgeRepoIndex {
                    tenant_id: &identity.tenant_id,
                    actor_id: &identity.actor_id,
                    repo_id: MDX_SELF_REPO_ID,
                    repo_receipt_id: &repo_receipt_id,
                    profile_json: &profile_json,
                    profile_fingerprint: "mdx-self-built-in-profile-v1",
                    primary_language: "rust",
                    language_pack_id: "rust-cargo",
                    detected_language_packs: "rust-cargo,node",
                    semantic_tool_readiness: "rust-analyzer-ready,typescript-server-available",
                    toolchain_readiness: "cargo,make,npm",
                    proof_plan_status: "PROOF_PLAN_READY",
                    standards_source_summaries: "MDx repository standards are governed by the agent working contract and Forge operator experience docs.",
                },
                &identity.governed,
            )
            .map_err(|error| error.message())?;
    }
    Ok(repo_receipt_id)
}

fn json_value(body: &str) -> serde_json::Value {
    serde_json::from_str(body).unwrap_or(serde_json::Value::Null)
}

fn string_field(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn bool_field(value: &serde_json::Value, key: &str) -> bool {
    value
        .get(key)
        .and_then(|field| match field {
            serde_json::Value::Bool(value) => Some(*value),
            serde_json::Value::String(value) => {
                let lowered = value.trim().to_ascii_lowercase();
                Some(matches!(lowered.as_str(), "1" | "true" | "yes" | "on"))
            }
            _ => None,
        })
        .unwrap_or(false)
}

fn mdx_self_repo_root() -> Result<String, String> {
    let cwd = std::env::current_dir()
        .map_err(|error| format!("cannot resolve the running MDx repo root: {error}"))?;
    for candidate in cwd.ancestors() {
        if candidate.join(".git").exists() && candidate.join("Cargo.toml").exists() {
            return canonical_path(candidate);
        }
    }
    Err("cannot connect MDx itself: no git repo root with Cargo.toml was found above the running server".to_string())
}

fn canonical_path(path: &Path) -> Result<String, String> {
    std::fs::canonicalize(path)
        .map(|path| path.to_string_lossy().to_string())
        .map_err(|error| format!("cannot resolve repo path {}: {error}", path.display()))
}

fn nested_string_field(value: &serde_json::Value, key: &str, nested_key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(|nested| string_field(nested, nested_key))
}

fn number_or_string_field(value: &serde_json::Value, key: &str) -> Option<String> {
    string_field(value, key).or_else(|| {
        value
            .get(key)
            .and_then(serde_json::Value::as_u64)
            .map(|number| number.to_string())
    })
}

fn value_list(value: &serde_json::Value, key: &str) -> Option<Vec<String>> {
    value.get(key).and_then(|list| match list {
        serde_json::Value::Array(items) => Some(
            items
                .iter()
                .filter_map(|item| {
                    item.as_str()
                        .map(str::to_string)
                        .or_else(|| string_field(item, "repo_id"))
                        .or_else(|| string_field(item, "id"))
                })
                .collect::<Vec<_>>(),
        ),
        serde_json::Value::String(value) if !value.trim().is_empty() => {
            Some(vec![value.trim().to_string()])
        }
        _ => None,
    })
}

fn compact_value_field(value: &serde_json::Value, key: &str) -> String {
    match value.get(key) {
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .filter_map(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join("; "),
        Some(serde_json::Value::String(value)) => value.trim().to_string(),
        Some(other) if !other.is_null() => other.to_string(),
        _ => String::new(),
    }
}

fn quoted_strings(values: &[String]) -> String {
    values
        .iter()
        .map(|value| json_string_literal(value))
        .collect::<Vec<_>>()
        .join(",")
}

fn bool_text(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

fn ollama_runtime_ready() -> bool {
    let Ok(address) = "127.0.0.1:11434".parse::<SocketAddr>() else {
        return false;
    };
    TcpStream::connect_timeout(&address, Duration::from_millis(150)).is_ok()
}

fn refusal_json(route: &str, reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-activation-write","status":"REFUSED","route":{},"recorded":false,"reason":{},"production_write_allowed":false}}"#,
            json_string_literal(route),
            json_string_literal(reason),
        ),
    )
}
