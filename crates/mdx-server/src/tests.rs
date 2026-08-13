// The serve-layer test suite, split from main.rs to honor its line
// budget. Same crate, same coverage - main.rs declares `mod tests`.

use super::*;
use mdx_core::{
    ActorId, LoopId, PagesEditDraft, PagesPublication, Receipt, TenantId, TraceId,
    TwinModelGatewayProviderObservation, WorkflowId, local_http_routes,
    twin_model_gateway_provider_slots,
};
use std::collections::BTreeMap;

fn local_model_test_guard() -> std::sync::MutexGuard<'static, ()> {
    crate::forge_turn_client::ENV_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}

fn seed_pages_edit_draft_for_test(
    kernel: &Arc<RwLock<MdxKernel>>,
    tenant_id: &str,
    actor_id: &str,
    document_id: &str,
    draft_id: &str,
    content: (&str, &str, &str),
) -> String {
    let (title, body_ref, revision_id) = content;
    let mut kernel = kernel.write().expect("kernel lock");
    let seed = kernel.run_evals_runner_agent().expect("seed evidence");
    let source_receipt_id = seed.receipts[0].clone();
    let publication = kernel
        .save_pages_publication_local(PagesPublication {
            tenant_id,
            actor_id,
            document_id,
            title: "Existing page",
            body_ref: "world_model://pages/existing/body/v1",
            source_receipt_id: &source_receipt_id,
            revision_id: "rev_existing",
            page_type: "knowledge",
        })
        .expect("seed Pages publication");
    kernel
        .save_pages_edit_draft_local(PagesEditDraft {
            tenant_id,
            actor_id,
            draft_id,
            document_id,
            title,
            body_ref,
            source_publication_receipt_id: &publication.publication_receipt_id,
            origin_receipt_id: "",
            origin_surface: "",
            revision_id,
        })
        .expect("seed Pages edit draft")
        .edit_draft_receipt_id
}

fn request_pages_approval_for_test(
    kernel: &Arc<RwLock<MdxKernel>>,
    document_id: &str,
    draft_id: &str,
    source_edit_draft_receipt_id: &str,
    approval_request_id: &str,
) -> String {
    let body = format!(
        r#"{{"approval_request_id":"{approval_request_id}","source_edit_draft_receipt_id":"{source_edit_draft_receipt_id}","document_id":"{document_id}","draft_id":"{draft_id}","requested_visibility":"tenant_review"}}"#
    );
    let response = route_request_with_body("POST", "/pages/approval-requests.json", &body, kernel)
        .expect("Pages approval request");
    assert!(
        response
            .body
            .contains("\"status\":\"PAGE_APPROVAL_REQUEST_RECORDED_PUBLICATION_BLOCKED\""),
        "approval request body: {}",
        response.body
    );
    json_string_field_for_test(&response.body, "approval_request_receipt_id")
}

fn decide_pages_approval_for_test(
    kernel: &Arc<RwLock<MdxKernel>>,
    approval_request_receipt_id: &str,
    approval_decision_id: &str,
    outcome: &str,
) -> String {
    let path = if outcome == "approved" {
        "/pages/approval-decisions/approve.json"
    } else {
        "/pages/approval-decisions/reject.json"
    };
    let body = format!(
        r#"{{"approval_decision_id":"{approval_decision_id}","approval_request_receipt_id":"{approval_request_receipt_id}","decision_note":"Test review decision."}}"#
    );
    let response =
        route_request_with_body("POST", path, &body, kernel).expect("Pages approval decision");
    assert!(
        response.body.contains("\"human_decision_recorded\":true"),
        "approval decision body: {}",
        response.body
    );
    json_string_field_for_test(&response.body, "approval_decision_receipt_id")
}

fn record_runtime_model_connected(kernel: &Arc<RwLock<MdxKernel>>) {
    let slot = twin_model_gateway_provider_slots()
        .iter()
        .find(|slot| slot.provider_id == "xai")
        .expect("xai slot");
    let tenant_id = crate::request_security::current_verified_identity()
        .map(|identity| identity.tenant_id)
        .unwrap_or_else(|| "local_tenant".to_string());
    crate::secret_store::global().set_for_tenant(&tenant_id, slot.env_key, "sk-test");
    let mut guard = kernel.write().expect("kernel lock");
    guard
        .save_twin_model_gateway_provider_observation_local(TwinModelGatewayProviderObservation {
            tenant_id: &tenant_id,
            actor_id: "human:activation_test",
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
        })
        .expect("runtime observation");
}

fn activation_test_identity(
    tenant_id: &str,
    actor_id: &str,
) -> crate::request_security::VerifiedIdentityGuard {
    crate::request_security::set_verified_identity(Some(mdx_core::AdmittedIdentity {
        deployment_mode: "local-secure",
        tenant_id: tenant_id.to_string(),
        actor_id: actor_id.to_string(),
        actor_role: "owner".to_string(),
        actor_kind: "human".to_string(),
        subject_actor_id: actor_id.to_string(),
        authority_scope: Vec::new(),
        delegation_id: None,
        identity_source: "trusted_session",
        production_write_allowed: false,
    }))
}

#[test]
fn activation_spine_records_profile_models_workspace_seed_and_first_proof() {
    let _identity = activation_test_identity("activation_spine_tenant", "human:activation_test");
    crate::secret_store::global().clear_tenant("activation_spine_tenant");
    let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
    let initial =
        route_request("GET", "/activation/projection.json", &kernel).expect("projection route");
    assert_eq!(initial.status, "200 OK");
    assert!(
        initial
            .body
            .contains("\"name\":\"mdx-activation-projection\"")
    );
    assert!(initial.body.contains("\"total_steps\":5"));
    assert!(initial.body.contains("\"complete_steps\":"));

    route_request_with_body(
        "POST",
        "/activation/profile.json",
        r#"{"name":"Dana","role":"engineering","style":{"register":"direct","grounding":"data_first","guidance":"recommendation"},"voice":"concise","actor_id":"human:activation_test"}"#,
        &kernel,
    )
    .expect("profile route");
    let profile = route_request("GET", "/activation/profile.json", &kernel).expect("profile get");
    assert!(profile.body.contains("\"recorded\":true"));
    assert!(profile.body.contains("\"register\":\"direct\""));
    assert!(profile.body.contains("\"grounding\":\"data_first\""));
    route_request_with_body(
        "POST",
        "/activation/model-setup.json",
        r#"{"providers":[{"provider":"xai","api_key":"xai-test-key","model_id":"grok-4.3"}],"private_twin_model":"qwen3","local_coding_model":"qwen2.5-coder","actor_id":"human:activation_test","test_mode":true}"#,
        &kernel,
    )
    .expect("model setup route");
    record_runtime_model_connected(&kernel);
    route_request_with_body(
        "POST",
        "/activation/forge-workspace.json",
        r#"{"repo_ids":["external_repo"],"use_mdx_self":true,"default_fleet_width":8,"actor_id":"human:activation_test"}"#,
        &kernel,
    )
    .expect("forge workspace route");
    let repos =
        route_request("GET", "/forge/repos/projection.json", &kernel).expect("forge repo list");
    assert!(repos.body.contains("\"repo_id\":\"mdx-self\""));
    assert!(
        repos
            .body
            .contains("\"label\":\"MDx (this app, strict scope)\"")
    );
    route_request_with_body(
        "POST",
        "/activation/starter-workspace.json",
        r#"{"starter_workspace_id":"starter_workspace_test","actor_id":"human:activation_test"}"#,
        &kernel,
    )
    .expect("starter workspace route");
    route_request_with_body(
        "POST",
        "/activation/first-proof.json",
        r#"{"starter_workspace_id":"starter_workspace_test","actor_id":"human:activation_test"}"#,
        &kernel,
    )
    .expect("first proof route");

    let projection =
        route_request("GET", "/activation/projection.json", &kernel).expect("projection route");
    assert_eq!(projection.status, "200 OK");
    for expected in [
        "\"status\":\"READY\"",
        "\"complete_steps\":5",
        "\"name\":\"Dana\"",
        "\"install_owner\":{\"claimed\":true",
        "\"owner_name\":\"Dana\"",
        "\"install_claimed\"",
        "\"roles_runnable\":8",
        "\"lane\":\"private_twin\"",
        "\"model_id\":\"qwen3\"",
        "\"lane\":\"forge_coding\"",
        "\"model_id\":\"qwen2.5-coder\"",
        "\"starter_workspace_id\":\"starter_workspace_test\"",
        "\"pages_source_available\"",
        "\"message_action_card_works\"",
        "\"first_lesson_available\"",
        "\"marketplace_install_recorded\"",
        "\"adaptation_allowed\":false",
        "\"production_write_allowed\":false",
    ] {
        assert!(projection.body.contains(expected), "{expected}");
    }
    let flywheel =
        route_request("GET", "/forge/flywheel-proof.json", &kernel).expect("flywheel proof");
    assert!(flywheel.body.contains("\"outcome_signal_count\":1"));
    assert!(flywheel.body.contains("\"active_memory_count\":1"));
    assert!(flywheel.body.contains("\"installed_capability_count\":1"));
    assert!(flywheel.body.contains("\"lesson_citation_event_count\":1"));
    let pages = route_request("GET", "/pages/publications/projection.json", &kernel)
        .expect("pages publications projection");
    assert!(
        pages
            .body
            .contains("\"document_id\":\"activation_seed_tdd_standard\"")
    );
    assert!(pages.body.contains("\"page_type\":\"standard\""));
    let message = route_request("GET", "/messages/action-requests/projection.json", &kernel)
        .expect("message action projection");
    assert!(
        message
            .body
            .contains("\"action_request_id\":\"activation_seed_message_action\"")
    );
    assert!(message.body.contains("\"execution_allowed\":false"));
}

#[test]
fn activation_projection_reaches_ready_after_real_first_mission_completion() {
    let _identity = activation_test_identity("activation_real_tenant", "human:activation_real");
    crate::secret_store::global().clear_tenant("activation_real_tenant");
    let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
    route_request_with_body(
        "POST",
        "/activation/profile.json",
        r#"{"name":"Dana","role":"engineering","style":{"register":"direct","grounding":"data_first","guidance":"recommendation"},"voice":"concise","actor_id":"human:activation_real"}"#,
        &kernel,
    )
    .expect("profile route");
    route_request_with_body(
        "POST",
        "/activation/model-setup.json",
        r#"{"providers":[{"provider":"xai","api_key":"xai-test-key","model_id":"grok-4.3"}],"private_twin_model":"qwen3","local_coding_model":"qwen2.5-coder","actor_id":"human:activation_real","test_mode":true}"#,
        &kernel,
    )
    .expect("model setup route");
    record_runtime_model_connected(&kernel);
    route_request_with_body(
        "POST",
        "/activation/forge-workspace.json",
        r#"{"repo_ids":["external_repo"],"default_fleet_width":4,"actor_id":"human:activation_real"}"#,
        &kernel,
    )
    .expect("forge workspace route");
    let first_mission = route_request_with_body(
        "POST",
        "/activation/first-mission/start.json",
        r#"{"starter_id":"add_small_test","repo_target":"mdx_self","force_fallback":true,"actor_id":"human:activation_real"}"#,
        &kernel,
    )
    .expect("first mission start");
    assert!(first_mission.body.contains("\"activation_seed\":false"));

    let projection =
        route_request("GET", "/activation/projection.json", &kernel).expect("projection route");
    for expected in [
        "\"status\":\"READY\"",
        "\"complete_steps\":5",
        "\"starter_workspace_id\":\"activation_first_mission_",
        "\"activation_seed\":false",
        "First mission completion is recorded as the real starter workspace proof.",
        "\"starter_workspace_seeded\"",
        "\"source_kind\":\"activation.starter_workspace.seeded\"",
        "\"first_proof_seen\"",
        "\"source_kind\":\"activation.first_proof.recorded\"",
    ] {
        assert!(
            projection.body.contains(expected),
            "{expected}: {}",
            projection.body
        );
    }
    let setup_router =
        route_request("GET", "/install/setup-router.json", &kernel).expect("setup router");
    for expected in [
        "\"setup_complete\":true",
        "\"setup_authority_route\":\"/activation/projection.json\"",
        "\"activation_status\":\"READY\"",
        "\"activation_complete_steps\":5",
        "\"path_choice_unlocked\":true",
    ] {
        assert!(
            setup_router.body.contains(expected),
            "{expected}: {}",
            setup_router.body
        );
    }
}

#[test]
fn activation_model_step_does_not_complete_from_stored_forge_key_only() {
    let _identity =
        activation_test_identity("activation_keychain_tenant", "human:activation_keychain");
    crate::secret_store::global().clear_tenant("activation_keychain_tenant");
    let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
    route_request_with_body(
        "POST",
        "/activation/profile.json",
        r#"{"name":"Dana","role":"engineering","style":{"register":"direct","grounding":"data_first","guidance":"recommendation"},"voice":"concise","actor_id":"human:activation_keychain"}"#,
        &kernel,
    )
    .expect("profile route");
    crate::secret_store::global().set_for_tenant(
        "activation_keychain_tenant",
        "ANTHROPIC_API_KEY",
        "sk-ant-test-key",
    );

    let projection =
        route_request("GET", "/activation/projection.json", &kernel).expect("projection route");
    let projection_json: serde_json::Value =
        serde_json::from_str(&projection.body).expect("projection json");
    assert_eq!(
        projection_json["progress"]["complete_steps"].as_u64(),
        Some(1),
        "{}",
        projection.body
    );
    let model_step = projection_json["progress"]["steps"]
        .as_array()
        .expect("steps")
        .iter()
        .find(|step| step["key"].as_str() == Some("model_setup"))
        .expect("model setup step");
    assert_eq!(
        model_step["status"].as_str(),
        Some("waiting"),
        "{}",
        projection.body
    );
    assert_eq!(
        model_step["done"].as_bool(),
        Some(false),
        "{}",
        projection.body
    );
    let model_proof = projection_json["proofs"]
        .as_array()
        .expect("proof items")
        .iter()
        .find(|item| item["key"].as_str() == Some("model_lane_ready"))
        .expect("model proof");
    assert_eq!(
        model_proof["satisfied"].as_bool(),
        Some(false),
        "{}",
        projection.body
    );
    let setup_router =
        route_request("GET", "/install/setup-router.json", &kernel).expect("setup router");
    assert!(
        setup_router.body.contains("\"model_connected\":false"),
        "{}",
        setup_router.body
    );
    assert!(
        setup_router.body.contains("\"path_choice_unlocked\":false"),
        "{}",
        setup_router.body
    );
}

#[test]
fn activation_first_mission_records_real_chain_and_honest_fallback_without_model() {
    let _guard = local_model_test_guard();
    crate::secret_store::global().clear_tenant("local_tenant");
    let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
    let initial = route_request("GET", "/activation/first-mission/projection.json", &kernel)
        .expect("first mission projection");
    assert_eq!(initial.status, "200 OK");
    assert!(initial.body.contains("\"status\":\"not_started\""));
    assert!(initial.body.contains("\"receipt_backed\":true"));
    assert!(initial.body.contains("\"receipts\":[]"));
    assert!(initial.body.contains("\"starter_id\":\"add_small_test\""));

    let started = route_request_with_body(
        "POST",
        "/activation/first-mission/start.json",
        r#"{"starter_id":"add_small_test","repo_target":"mdx_self","force_fallback":true,"actor_id":"human:first_mission_test"}"#,
        &kernel,
    )
    .expect("first mission start");
    assert_eq!(started.status, "200 OK");
    for expected in [
        "\"name\":\"mdx-activation-first-mission-local-post\"",
        "\"status\":\"FIRST_MISSION_FALLBACK\"",
        "\"activation_seed\":false",
        "\"page_id\":\"activation_first_mission_",
        "\"fallback_reason\":\"Live Forge execution was skipped",
        "\"action_request_id\":\"activation_first_mission_",
    ] {
        assert!(
            started.body.contains(expected),
            "{expected}: {}",
            started.body
        );
    }
    let projection = route_request("GET", "/activation/first-mission/projection.json", &kernel)
        .expect("first mission projection after start");
    for expected in [
        "\"status\":\"fallback\"",
        "\"endpoints\":[",
        "\"brief\":{\"goal\":\"Add a tiny note",
        "\"companion_id\":\"twin_coder\"",
        "\"page\":{\"page_id\":\"activation_first_mission_",
        "\"run\":{\"run_id\":\"\",\"status\":\"fallback\"",
        "\"state\":\"blocked\"",
        "\"message\":{\"channel_id\":\"activation_first_mission\"",
        "\"activation_seed\":false",
    ] {
        assert!(
            projection.body.contains(expected),
            "{expected}: {}",
            projection.body
        );
    }
    let pages = route_request("GET", "/pages/publications/projection.json", &kernel)
        .expect("pages projection");
    assert!(pages.body.contains("\"page_type\":\"spec\""));
    let started_json: serde_json::Value = serde_json::from_str(&started.body).expect("start json");
    let page_id = started_json["page"]["page_id"].as_str().expect("page id");
    let body_route = format!("/pages/{page_id}/body");
    let page_body = route_request("GET", &body_route, &kernel).expect("first mission page body");
    for expected in [
        "\"document_id\": \"activation_first_mission_",
        "# First Mission Brief",
        "Add a tiny note",
        "make agent-working-contract-check",
    ] {
        assert!(
            page_body.body.contains(expected),
            "{expected}: {}",
            page_body.body
        );
    }
    let actions = route_request("GET", "/messages/action-requests/projection.json", &kernel)
        .expect("message action projection");
    assert!(
        actions
            .body
            .contains("\"channel_id\":\"activation_first_mission\"")
    );
}

#[test]
fn activation_first_mission_projection_exposes_live_shaping_before_brief() {
    let _guard = local_model_test_guard();
    crate::secret_store::global().clear_tenant("local_tenant");
    let mut kernel = MdxKernel::boot_local();
    let identity = GovernedWriteIdentity::local_demo("human:first_mission_test");
    let mission_id = "activation_first_mission_live_shaping";
    kernel
        .record_activation_event_with_identity(
            mdx_core::ActivationEvent {
                tenant_id: "local_tenant",
                actor_id: "human:first_mission_test",
                kind: mdx_core::ACTIVATION_FIRST_MISSION_STARTED,
                step: "first_mission",
                surface: "welcome",
                detail: "Started a real first mission from the welcome flow.",
                starter_workspace_id: "",
                activation_seed: false,
                fields: &[
                    ("mission_id", mission_id),
                    ("ask", "shape this with the team"),
                    ("starter_id", "add_small_test"),
                    ("repo_target", "mdx_self"),
                    ("repo_id", "mdx-self"),
                    ("shaping_mode", "live_personas"),
                ],
            },
            &identity,
        )
        .expect("started");
    let kernel = Arc::new(RwLock::new(kernel));
    let projection = route_request("GET", "/activation/first-mission/projection.json", &kernel)
        .expect("first mission projection");
    for expected in [
        "\"status\":\"shaping\"",
        "\"shaping\":{\"status\":\"shaping\",\"mode\":\"live_personas\"",
        "\"stream_route\":\"/activation/first-mission/shaping/stream?mission_id=activation_first_mission_live_shaping\"",
        "\"brief\":null",
        "\"page\":null",
        "\"run\":{\"run_id\":\"\",\"status\":\"not_started\"",
        "\"message\":null",
    ] {
        assert!(
            projection.body.contains(expected),
            "{expected}: {}",
            projection.body
        );
    }
}

#[test]
fn activation_first_mission_projection_streams_after_run_admission_before_terminal_result() {
    let _guard = local_model_test_guard();
    crate::secret_store::global().clear_tenant("local_tenant");
    let mut kernel = MdxKernel::boot_local();
    let identity = GovernedWriteIdentity::local_demo("human:first_mission_test");
    let mission_id = "activation_first_mission_streaming";
    let run_id = "forge_run_streaming_first_mission";
    let started = kernel
        .record_activation_event_with_identity(
            mdx_core::ActivationEvent {
                tenant_id: "local_tenant",
                actor_id: "human:first_mission_test",
                kind: mdx_core::ACTIVATION_FIRST_MISSION_STARTED,
                step: "first_mission",
                surface: "welcome",
                detail: "Started a real first mission from the welcome flow.",
                starter_workspace_id: "",
                activation_seed: false,
                fields: &[
                    ("mission_id", mission_id),
                    ("ask", "stream the first mission"),
                    ("starter_id", "add_small_test"),
                    ("repo_target", "mdx_self"),
                    ("repo_id", "mdx-self"),
                ],
            },
            &identity,
        )
        .expect("started");
    let brief = kernel
        .record_activation_event_with_identity(
            mdx_core::ActivationEvent {
                tenant_id: "local_tenant",
                actor_id: "human:first_mission_test",
                kind: mdx_core::ACTIVATION_FIRST_MISSION_BRIEF_SHAPED,
                step: "first_mission_brief",
                surface: "twin",
                detail: "Twin shaped the first mission brief.",
                starter_workspace_id: "",
                activation_seed: false,
                fields: &[
                    ("mission_id", mission_id),
                    ("goal", "Stream the first mission"),
                    ("approach", "Expose early run state"),
                    (
                        "acceptance",
                        "Projection shows running before terminal result",
                    ),
                    ("proof_command", "make agent-working-contract-check"),
                    ("persona_ids", "twin_coder,twin_architect,twin_advisor"),
                    ("coder_contribution", "Run it narrowly."),
                    ("architect_contribution", "Keep receipts incremental."),
                    ("advisor_contribution", "Show progress honestly."),
                    ("shaping_mode", "curated_tier0_no_live_search"),
                ],
            },
            &identity,
        )
        .expect("brief");
    let page = kernel
        .save_pages_publication_local_with_identity(
            mdx_core::PagesPublication {
                tenant_id: "local_tenant",
                actor_id: "human:first_mission_test",
                document_id: "activation_first_mission_streaming_brief",
                title: "First Mission Brief - Streaming",
                body_ref: "activation://first-mission/activation_first_mission_streaming/brief",
                source_receipt_id: &brief.receipt_id,
                revision_id: "activation_first_mission_streaming_brief_rev_001",
                page_type: "spec",
            },
            &identity,
        )
        .expect("page");
    kernel
        .record_activation_event_with_identity(
            mdx_core::ActivationEvent {
                tenant_id: "local_tenant",
                actor_id: "human:first_mission_test",
                kind: mdx_core::ACTIVATION_FIRST_MISSION_RUN_ADMITTED,
                step: "first_mission_run",
                surface: "forge",
                detail: "Forge accepted the first mission run and is executing it asynchronously.",
                starter_workspace_id: "",
                activation_seed: false,
                fields: &[
                    ("mission_id", mission_id),
                    ("started_receipt_id", &started.receipt_id),
                    ("brief_receipt_id", &brief.receipt_id),
                    ("page_receipt_id", &page.publication_receipt_id),
                    ("run_id", run_id),
                    ("run_receipt_id", ""),
                    ("run_status", "running"),
                    ("run_available", "true"),
                ],
            },
            &identity,
        )
        .expect("run admitted");
    let kernel = Arc::new(RwLock::new(kernel));
    let admitted_projection =
        route_request("GET", "/activation/first-mission/projection.json", &kernel)
            .expect("first mission projection");
    for expected in [
        "\"status\":\"running\"",
        "\"page\":{\"page_id\":\"activation_first_mission_streaming_brief\"",
        "\"run\":{\"run_id\":\"forge_run_streaming_first_mission\",\"status\":\"running\"",
        "\"state\":\"pending\"",
        "\"message\":null",
        "\"kind\":\"activation.first_mission.run_admitted\"",
    ] {
        assert!(
            admitted_projection.body.contains(expected),
            "{expected}: {}",
            admitted_projection.body
        );
    }
    let legacy_body = route_request(
        "GET",
        "/pages/activation_first_mission_streaming_brief/body",
        &kernel,
    )
    .expect("legacy first mission page body");
    for expected in [
        "\"document_id\": \"activation_first_mission_streaming_brief\"",
        "# First Mission Brief",
        "Stream the first mission",
        "make agent-working-contract-check",
    ] {
        assert!(
            legacy_body.body.contains(expected),
            "{expected}: {}",
            legacy_body.body
        );
    }
    {
        let mut kernel = kernel.write().expect("kernel");
        kernel
            .record_forge_run_event_with_evidence_fields(
                mdx_core::ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "human:first_mission_test",
                    run_id,
                    event: "run_started",
                    work_item_id: "tier0_dev_101",
                    detail: "accepted first mission",
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &identity,
                &[
                    ("activation_first_mission_id", mission_id),
                    ("builder_casting_selected_provider_family", "anthropic"),
                    ("builder_casting_selected_model_id", "claude-opus-4-8"),
                    ("builder_casting_selected_slot", "reviewer"),
                    ("builder_casting_selected_model_class", "frontier"),
                ],
            )
            .expect("run started")
    };
    let projection = route_request("GET", "/activation/first-mission/projection.json", &kernel)
        .expect("first mission projection");
    for expected in [
        "\"status\":\"running\"",
        "\"page\":{\"page_id\":\"activation_first_mission_streaming_brief\"",
        "\"run\":{\"run_id\":\"forge_run_streaming_first_mission\",\"status\":\"running\"",
        "\"model\":{\"provider_family\":\"anthropic\",\"model_id\":\"claude-opus-4-8\",\"slot\":\"reviewer\",\"model_class\":\"frontier\"",
        "\"state\":\"done\"",
        "\"message\":null",
        "\"kind\":\"activation.first_mission.run_admitted\"",
    ] {
        assert!(
            projection.body.contains(expected),
            "{expected}: {}",
            projection.body
        );
    }
}

#[test]
fn activation_first_mission_observer_posts_terminal_result_to_message() {
    let _guard = local_model_test_guard();
    crate::secret_store::global().clear_tenant("local_tenant");
    let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
    let started = route_request_with_body(
        "POST",
        "/activation/first-mission/start.json",
        r#"{"starter_id":"add_small_test","repo_target":"mdx_self","force_fallback":true,"actor_id":"human:first_mission_test"}"#,
        &kernel,
    )
    .expect("first mission start");
    let value: serde_json::Value = serde_json::from_str(&started.body).expect("start json");
    let mission_id = value["mission_id"].as_str().expect("mission id");
    let run_id = "forge_run_terminal_first_mission";
    {
        let mut guard = kernel.write().expect("kernel");
        let identity = GovernedWriteIdentity::local_demo("human:first_mission_test");
        guard
            .record_forge_run_event_with_evidence_fields(
                mdx_core::ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "human:first_mission_test",
                    run_id,
                    event: "run_started",
                    work_item_id: "tier0_dev_101",
                    detail: "accepted first mission",
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &identity,
                &[("activation_first_mission_id", mission_id)],
            )
            .expect("run started");
        guard
            .record_forge_run_event(mdx_core::ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "human:first_mission_test",
                run_id,
                event: "check_passed",
                work_item_id: "tier0_dev_101",
                detail: "make agent-working-contract-check passed",
                turn: 2,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("check passed");
        guard
            .record_forge_run_event(mdx_core::ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "human:first_mission_test",
                run_id,
                event: "evidence_appended",
                work_item_id: "tier0_dev_101",
                detail: "branch=forge/run-terminal-first-mission sha=abcdef123456",
                turn: 3,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("branch evidence");
        guard
            .record_forge_run_event(mdx_core::ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "human:first_mission_test",
                run_id,
                event: "run_finished",
                work_item_id: "tier0_dev_101",
                detail: "status=RUN_FINISHED_DONE turns=3 files_changed=1",
                turn: 3,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("run finished");
    }
    let request = crate::forge_loop_runner::ForgeRunRequest {
        run_id: run_id.to_string(),
        tenant_id: "local_tenant".to_string(),
        actor_id: "human:first_mission_test".to_string(),
        work_item_id: "tier0_dev_101".to_string(),
        intent: "test first mission observer".to_string(),
        allowed_commands: vec!["make agent-working-contract-check".to_string()],
        max_turns: 3,
        revise_branch: None,
        resume: false,
        write_scope: vec!["docs/DEV-101.md".to_string()],
        check_target_dir: None,
        builder_slot: String::new(),
        work_complexity_tier: "small".to_string(),
        semantic_policy_required_operations: Vec::new(),
        semantic_policy_source: String::new(),
        execution_geometry_requested_workers: 1,
        execution_geometry_effective_workers: 1,
        execution_geometry_lane: "single".to_string(),
        execution_geometry_route: "/forge/runs.json".to_string(),
        mission_id: String::new(),
        mission_milestone_id: String::new(),
        max_cost_cents: 0,
        max_runtime_ms: 0,
        envelope_id: String::new(),
        plan_only: false,
        reasoning_effort: String::new(),
    };
    let outcome = crate::forge_loop_runner::ForgeRunOutcome {
        run_id: run_id.to_string(),
        status: "RUN_FINISHED_DONE",
        turns_used: 3,
        files_changed: 1,
        check_runs: 1,
        check_duration_ms: 12,
        branch: Some("forge/run-terminal-first-mission".to_string()),
        commit_sha: Some("abcdef123456".to_string()),
        finish_summary: "Added the note and passed the proof.".to_string(),
        last_check_passed: true,
    };
    crate::activation_route::record_first_mission_result_from_run_outcome(
        &kernel, &request, &outcome,
    );

    let projection = route_request("GET", "/activation/first-mission/projection.json", &kernel)
        .expect("first mission projection after observer");
    for expected in [
        "\"status\":\"done\"",
        "\"run_id\":\"forge_run_terminal_first_mission\"",
        "\"proof_passed\":true",
        "\"terminal_message_recorded\":true",
        "\"terminal\":true",
        "\"run_status\":\"RUN_FINISHED_DONE\"",
        "\"kind\":\"activation.first_mission.result_recorded\"",
        "\"branch\":\"forge/run-terminal-first-mission\"",
        "\"diff_summary\":\"Branch ready for review with 1 changed file.\"",
    ] {
        assert!(
            projection.body.contains(expected),
            "{expected}: {}",
            projection.body
        );
    }
    let actions = route_request("GET", "/messages/action-requests/projection.json", &kernel)
        .expect("message actions after observer");
    assert!(actions.body.contains("_result_action"), "{actions:?}");
}

#[test]
fn activation_first_mission_result_requires_observed_check_receipt() {
    let _guard = local_model_test_guard();
    crate::secret_store::global().clear_tenant("local_tenant");
    let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
    let started = route_request_with_body(
        "POST",
        "/activation/first-mission/start.json",
        r#"{"starter_id":"add_small_test","repo_target":"mdx_self","force_fallback":true,"actor_id":"human:first_mission_test"}"#,
        &kernel,
    )
    .expect("first mission start");
    let value: serde_json::Value = serde_json::from_str(&started.body).expect("start json");
    let mission_id = value["mission_id"].as_str().expect("mission id");
    let run_id = "forge_run_terminal_without_check";
    {
        let mut guard = kernel.write().expect("kernel");
        let identity = GovernedWriteIdentity::local_demo("human:first_mission_test");
        guard
            .record_forge_run_event_with_evidence_fields(
                mdx_core::ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "human:first_mission_test",
                    run_id,
                    event: "run_started",
                    work_item_id: "tier0_dev_101",
                    detail: "accepted first mission",
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &identity,
                &[("activation_first_mission_id", mission_id)],
            )
            .expect("run started");
        guard
            .record_forge_run_event(mdx_core::ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "human:first_mission_test",
                run_id,
                event: "run_finished",
                work_item_id: "tier0_dev_101",
                detail: "status=RUN_FINISHED_DONE turns=3 files_changed=1",
                turn: 3,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("run finished");
    }
    let request = crate::forge_loop_runner::ForgeRunRequest {
        run_id: run_id.to_string(),
        tenant_id: "local_tenant".to_string(),
        actor_id: "human:first_mission_test".to_string(),
        work_item_id: "tier0_dev_101".to_string(),
        intent: "test first mission observer without proof".to_string(),
        allowed_commands: vec!["make agent-working-contract-check".to_string()],
        max_turns: 3,
        revise_branch: None,
        resume: false,
        write_scope: vec!["docs/DEV-101.md".to_string()],
        check_target_dir: None,
        builder_slot: String::new(),
        work_complexity_tier: "small".to_string(),
        semantic_policy_required_operations: Vec::new(),
        semantic_policy_source: String::new(),
        execution_geometry_requested_workers: 1,
        execution_geometry_effective_workers: 1,
        execution_geometry_lane: "single".to_string(),
        execution_geometry_route: "/forge/runs.json".to_string(),
        mission_id: String::new(),
        mission_milestone_id: String::new(),
        max_cost_cents: 0,
        max_runtime_ms: 0,
        envelope_id: String::new(),
        plan_only: false,
        reasoning_effort: String::new(),
    };
    let outcome = crate::forge_loop_runner::ForgeRunOutcome {
        run_id: run_id.to_string(),
        status: "RUN_FINISHED_DONE",
        turns_used: 3,
        files_changed: 1,
        check_runs: 0,
        check_duration_ms: 0,
        branch: Some("forge/run-terminal-without-check".to_string()),
        commit_sha: Some("abcdef123456".to_string()),
        finish_summary: "Claimed done without observed proof.".to_string(),
        last_check_passed: true,
    };
    crate::activation_route::record_first_mission_result_from_run_outcome(
        &kernel, &request, &outcome,
    );

    let projection = route_request("GET", "/activation/first-mission/projection.json", &kernel)
        .expect("first mission projection after observer");
    for expected in [
        "\"status\":\"failed\"",
        "\"run_id\":\"forge_run_terminal_without_check\"",
        "\"proof_passed\":false",
        "\"terminal_message_recorded\":true",
        "\"terminal\":true",
        "\"run_status\":\"RUN_FINISHED_DONE\"",
    ] {
        assert!(
            projection.body.contains(expected),
            "{expected}: {}",
            projection.body
        );
    }
}

#[test]
fn activation_first_mission_projects_external_repo_scout_starters() {
    let root = std::env::temp_dir().join(format!("mdx-first-mission-scout-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("repo dir");
    std::fs::write(
        root.join("package.json"),
        r#"{"scripts":{"test":"node index.js"}}"#,
    )
    .expect("package");
    std::fs::write(
        root.join("index.js"),
        "export function answer() {\n  // TODO: replace this placeholder\n  return 42;\n}\n",
    )
    .expect("source");
    assert!(
        std::process::Command::new("git")
            .arg("init")
            .arg(&root)
            .status()
            .expect("git init")
            .success()
    );
    let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
    let body = format!(
        r#"{{"repo_id":"node-first-mission","label":"Node First Mission","root":{},"kind":"local","actor_id":"human:first_mission_test"}}"#,
        mdx_core::json_string_literal(&root.to_string_lossy()),
    );
    let connected = route_request_with_body("POST", "/forge/repos.json", &body, &kernel)
        .expect("connect external repo");
    assert!(
        connected.body.contains("\"status\":\"CONNECTED\""),
        "{connected:?}"
    );

    let projection = route_request("GET", "/activation/first-mission/projection.json", &kernel)
        .expect("first mission projection");
    assert!(
        projection.body.contains("repo_scout::node-first-mission::"),
        "{}",
        projection.body
    );
    assert!(projection.body.contains("\"source\":\"repo_task_scout\""));
    assert!(
        projection
            .body
            .contains("\"repo_id\":\"node-first-mission\"")
    );

    let starter_id = projection
        .body
        .split("\"starter_id\":\"")
        .find_map(|part| {
            let candidate = part.split('"').next().unwrap_or("");
            candidate
                .starts_with("repo_scout::node-first-mission::")
                .then(|| candidate.to_string())
        })
        .expect("external starter id");
    let start_body = format!(
        r#"{{"starter_id":{},"force_fallback":true,"actor_id":"human:first_mission_test"}}"#,
        mdx_core::json_string_literal(&starter_id),
    );
    let started = route_request_with_body(
        "POST",
        "/activation/first-mission/start.json",
        &start_body,
        &kernel,
    )
    .expect("start external first mission");
    assert!(
        started
            .body
            .contains("\"status\":\"FIRST_MISSION_FALLBACK\"")
    );
    let after = route_request("GET", "/activation/first-mission/projection.json", &kernel)
        .expect("projection after external start");
    assert!(
        after.body.contains("\"repo_target\":\"repo_id\""),
        "{after:?}"
    );
    assert!(after.body.contains("\"repo_id\":\"node-first-mission\""));
}

#[test]
fn status_routes_report_text_and_json() {
    let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
    let text = route_request("GET", "/status", &kernel).expect("status route");
    assert_eq!(text.status, "200 OK");
    assert_eq!(text.content_type, TEXT_CONTENT_TYPE);
    assert!(text.body.contains("mdx-native local"));
    assert!(text.body.contains("postgres: LIVE-LOCAL"));
    let json = route_request("GET", "/status.json", &kernel).expect("status json route");
    assert_eq!(json.status, "200 OK");
    assert_eq!(json.content_type, JSON_CONTENT_TYPE);
    assert!(json.body.contains("\"mode\": \"deterministic-local\""));
    assert!(json.body.contains("\"substrate\": \"opentelemetry\""));
    assert_eq!(
        json.body
            .matches("\"status\": \"PENDING-LIVE-RUN\"")
            .count(),
        5
    );
    assert!(json.body.contains("\"status\": \"LIVE-LOCAL\""));
    for (path, name, file_field) in [
        (
            "/local/index.json",
            "mdx-local-api-index",
            "\"path\": \"/local/operational-handoff.json\"",
        ),
        (
            "/local/evidence-index.json",
            "mdx-local-evidence-index",
            "\"evidence_file\": \".mdx-local/evidence-index.json\"",
        ),
        (
            "/local/activation-summary.json",
            "mdx-local-activation-summary",
            "\"summary_file\": \".mdx-local/activation-summary.json\"",
        ),
        (
            "/local/activation-report.json",
            "mdx-local-activation-report",
            "\"evidence_file\": \".mdx-local/activation-report.json\"",
        ),
        (
            "/local/dogfood-proof.json",
            "mdx-local-dogfood-proof",
            "\"evidence_file\": \".mdx-local/dogfood/local-dogfood-proof.json\"",
        ),
        (
            "/local/operational-handoff.json",
            "mdx-local-operational-handoff",
            "\"evidence_file\": \".mdx-local/handoff/local-operational-handoff.json\"",
        ),
        (
            "/local/auth-session.json",
            "mdx-auth-session-boundary",
            "\"tenant_id\":\"local_tenant\"",
        ),
    ] {
        let response = route_request("GET", path, &kernel).expect("local evidence route");
        assert_eq!(response.status, "200 OK");
        assert_eq!(response.content_type, JSON_CONTENT_TYPE);
        assert!(
            response.body.contains(&format!("\"name\": \"{name}\""))
                || response.body.contains(&format!("\"name\":\"{name}\""))
        );
        assert!(response.body.contains(file_field));
    }
    route_request("POST", "/run-loop/evals_runner_agent", &kernel).expect("run loop route");
    let observatory_json =
        route_request("GET", "/observatory.json", &kernel).expect("observatory json route");
    assert_eq!(observatory_json.status, "200 OK");
    assert_eq!(observatory_json.content_type, JSON_CONTENT_TYPE);
    assert!(
        observatory_json
            .body
            .contains("\"surface\": \"observatory\"")
    );
    assert!(observatory_json.body.contains("\"receipt_count\": 14"));
    let concierge_json =
        route_request("GET", "/concierge.json", &kernel).expect("concierge json route");
    assert_eq!(concierge_json.status, "200 OK");
    assert_eq!(concierge_json.content_type, JSON_CONTENT_TYPE);
    assert!(
        concierge_json
            .body
            .contains("\"credential_status\": \"MINTED\"")
    );
    assert!(concierge_json.body.contains("\"receipt_ids\""));
    let twin_json = route_request("GET", "/twin.json", &kernel).expect("twin json route");
    assert_eq!(twin_json.status, "200 OK");
    assert_eq!(twin_json.content_type, JSON_CONTENT_TYPE);
    assert!(
        twin_json
            .body
            .contains("\"companion_id\": \"twin_advisor\"")
    );
    assert!(
        twin_json
            .body
            .contains("\"runtime_status\": \"PENDING-LIVE-RUN\"")
    );
    assert!(twin_json.body.contains("\"receipt_ids\""));
    let architect_json =
        route_request("GET", "/twin/twin_architect.json", &kernel).expect("twin architect");
    assert_eq!(architect_json.status, "200 OK");
    assert_eq!(architect_json.content_type, JSON_CONTENT_TYPE);
    assert!(
        architect_json
            .body
            .contains("\"companion_id\": \"twin_architect\"")
    );
    assert!(architect_json.body.contains("\"role\": \"architect\""));
    assert!(architect_json.body.contains("\"receipt_ids\""));
    let coder_json = route_request("GET", "/twin/twin_coder.json", &kernel).expect("twin coder");
    assert_eq!(coder_json.status, "200 OK");
    assert_eq!(coder_json.content_type, JSON_CONTENT_TYPE);
    assert!(coder_json.body.contains("\"companion_id\": \"twin_coder\""));
    assert!(coder_json.body.contains("\"role\": \"coder\""));
    assert!(coder_json.body.contains("\"receipt_ids\""));
    let strategy_json =
        route_request("GET", "/strategy.json", &kernel).expect("strategy json route");
    assert_eq!(strategy_json.status, "200 OK");
    assert_eq!(strategy_json.content_type, JSON_CONTENT_TYPE);
    let strategy_body = strategy_json.body.as_str();
    assert!(strategy_body.contains("\"proposal_id\": \"strategy_local_ratification_001\""));
    assert!(strategy_body.contains("\"runtime_status\": \"DECLARED-LOCAL-READ-SURFACE\""));
    assert!(strategy_body.contains("\"ratification_required\": true"));
    assert!(strategy_body.contains("\"receipt_ids\""));
    let strategy_text = route_request("GET", "/strategy", &kernel).expect("strategy route");
    assert_eq!(strategy_text.status, "200 OK");
    assert_eq!(strategy_text.content_type, TEXT_CONTENT_TYPE);
    assert!(strategy_text.body.contains("Ratification required: true"));
    assert!(strategy_text.body.contains("Blocked actions:"));
    let product_json = route_request("GET", "/product.json", &kernel).expect("product json route");
    assert_eq!(product_json.status, "200 OK");
    assert_eq!(product_json.content_type, JSON_CONTENT_TYPE);
    for expected in [
        "\"bet_id\": \"product_bet_local_stub\"",
        "\"runtime_status\": \"LOCAL-BLOCKED-AT-HUMAN-RATIFICATION\"",
        "\"ratification_required\": true",
        "\"receipt_ids\"",
    ] {
        assert!(product_json.body.contains(expected), "{expected}");
    }
    let product_text = route_request("GET", "/product", &kernel).expect("product route");
    assert_eq!(product_text.status, "200 OK");
    assert_eq!(product_text.content_type, TEXT_CONTENT_TYPE);
    assert!(product_text.body.contains("Ratification required: true"));
    assert!(product_text.body.contains("Blocked actions:"));
    let pages_list = route_request("GET", "/pages.json", &kernel).expect("pages list");
    assert_eq!(pages_list.status, "200 OK");
    assert_eq!(pages_list.content_type, JSON_CONTENT_TYPE);
    let pages_list_body = pages_list.body.as_str();
    assert!(pages_list_body.contains("\"surface\": \"pages\""));
    assert!(pages_list_body.contains("\"projection\": \"document_list\""));
    assert!(pages_list_body.contains("\"document_id\": \"page_evidence_evals_runner\""));
    assert!(pages_list_body.contains("\"document_id\": \"page_developer_start_here\""));
    assert!(pages_list_body.contains("\"document_id\": \"page_local_proof_runway\""));
    assert!(pages_list_body.contains("\"source_receipt_ids\""));
    assert!(pages_list_body.contains("\"visibility\": \"tenant_only\""));
    assert!(pages_list_body.contains("\"writes_allowed\": false"));
    let pages_document =
        route_request("GET", "/pages/page_evidence_evals_runner", &kernel).expect("pages document");
    assert_eq!(pages_document.status, "200 OK");
    assert_eq!(pages_document.content_type, JSON_CONTENT_TYPE);
    let pages_document_body = pages_document.body.as_str();
    assert!(pages_document_body.contains("\"title\": \"Evals Runner Evidence Summary\""));
    assert!(pages_document_body.contains("\"receipt_backed\": true"));
    let developer_page =
        route_request("GET", "/pages/page_developer_start_here", &kernel).expect("developer page");
    assert_eq!(developer_page.status, "200 OK");
    assert_eq!(developer_page.content_type, JSON_CONTENT_TYPE);
    let developer_page_body = developer_page.body.as_str();
    assert!(developer_page_body.contains("\"title\": \"Developer Start Here\""));
    assert!(developer_page_body.contains("\"body_ref\": \"docs/ONBOARDING.md\""));
    for (path, expected_body_ref) in [
        ("/pages/page_local_proof_runway", "docs/LOCAL-RUNBOOK.md"),
        ("/pages/page_activation_runway", "docs/QUICKSTART.md"),
        (
            "/pages/page_harness_safe_tool_plane",
            "docs/HARNESS-SAFE-TOOL-PLANE.md",
        ),
    ] {
        let page = route_request("GET", path, &kernel).expect("pages document");
        assert_eq!(page.status, "200 OK");
        assert_eq!(page.content_type, JSON_CONTENT_TYPE);
        assert!(page.body.contains(expected_body_ref), "{expected_body_ref}");
    }
    let developer_body = route_request("GET", "/pages/page_developer_start_here/body", &kernel)
        .expect("developer body");
    assert_eq!(developer_body.status, "200 OK");
    assert_eq!(developer_body.content_type, JSON_CONTENT_TYPE);
    assert!(
        developer_body
            .body
            .contains("\"body_ref\": \"docs/ONBOARDING.md\"")
    );
    assert!(developer_body.body.contains("\"writes_allowed\": false"));
    assert!(developer_body.body.contains("MDx"));
    let pages_body_missing =
        route_request("GET", "/pages/missing/body", &kernel).expect("missing page body");
    assert_eq!(pages_body_missing.status, "404 Not Found");
    let pages_missing = route_request("GET", "/pages/missing", &kernel).expect("missing page");
    assert_eq!(pages_missing.status, "404 Not Found");
    let pages_post = route_request("POST", "/pages.json", &kernel).expect("pages post");
    assert_eq!(pages_post.status, "405 Method Not Allowed");
    let message_threads =
        route_request("GET", "/messages/threads.json", &kernel).expect("message threads");
    assert_eq!(message_threads.status, "200 OK");
    assert_eq!(message_threads.content_type, JSON_CONTENT_TYPE);
    assert!(message_threads.body.contains("\"surface\": \"message\""));
    assert!(
        message_threads
            .body
            .contains("\"projection\": \"thread_summary\"")
    );
    assert!(
        message_threads
            .body
            .contains("\"thread_id\": \"thread_local_receipts\"")
    );
    assert!(message_threads.body.contains("\"messages\": ["));
    assert!(message_threads.body.contains("\"actor_id\""));
    assert!(message_threads.body.contains("\"body\""));
    assert!(message_threads.body.contains("\"message_count\": 14"));
    assert!(
        message_threads
            .body
            .contains("\"sequence_high_watermark\": 14")
    );
    assert!(message_threads.body.contains("\"receipt_backed\": true"));
    assert!(message_threads.body.contains("\"writes_allowed\": false"));
    assert!(
        message_threads
            .body
            .contains("\"llm_allowed_on_hot_path\": false")
    );
    let message_channel = route_request("GET", "/messages/channels/local-ops.json", &kernel)
        .expect("message channel");
    assert_eq!(message_channel.status, "200 OK");
    assert_eq!(message_channel.content_type, JSON_CONTENT_TYPE);
    assert!(
        message_channel
            .body
            .contains("\"projection\": \"channel_timeline\"")
    );
    assert!(
        message_channel
            .body
            .contains("\"channel_id\": \"local-ops\"")
    );
    assert!(message_channel.body.contains("\"name\": \"Local Ops\""));
    assert!(message_channel.body.contains("\"message_count\": 14"));
    assert!(message_channel.body.contains("\"messages\": ["));
    assert!(message_channel.body.contains("\"source_event_ids\""));
    let message_post =
        route_request("POST", "/messages/threads.json", &kernel).expect("message post");
    assert_eq!(message_post.status, "405 Method Not Allowed");
}
#[test]
fn receipt_routes_list_and_lookup_loop_receipts() {
    let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
    route_request("POST", "/run-loop/evals_runner_agent", &kernel).expect("run loop route");
    let list = route_request("GET", "/receipts.json", &kernel).expect("receipts route");
    assert_eq!(list.status, "200 OK");
    assert_eq!(list.content_type, JSON_CONTENT_TYPE);
    assert!(list.body.contains("\"count\": 14"));
    assert!(list.body.contains("evals_runner_agent_receipt_000005"));
    let receipt_id = {
        let kernel = kernel.read().expect("kernel lock");
        kernel
            .ledger()
            .query()
            .receipt_ids()
            .first()
            .expect("first receipt")
            .to_string()
    };
    let receipt =
        route_request("GET", &format!("/receipts/{receipt_id}"), &kernel).expect("receipt lookup");
    assert_eq!(receipt.status, "200 OK");
    assert_eq!(receipt.content_type, JSON_CONTENT_TYPE);
    assert!(
        receipt
            .body
            .contains(&format!("\"receipt_id\": \"{receipt_id}\""))
    );
    assert!(receipt.body.contains("\"tenant_id\": \"tenant_local\""));
    let missing =
        route_request("GET", "/receipts/missing", &kernel).expect("missing receipt lookup");
    assert_eq!(missing.status, "404 Not Found");
    assert_eq!(missing.content_type, TEXT_CONTENT_TYPE);
    assert_eq!(missing.body, "receipt not found\n");
}
#[test]
fn receipt_json_rendering_escapes_strings() {
    let receipt = Receipt {
        receipt_id: "receipt\"1".to_string(),
        tenant_id: TenantId::new("tenant\\local"),
        trace_id: TraceId::new("trace\nlocal"),
        actor_id: ActorId::new("actor\tlocal"),
        loop_id: LoopId::new("loop\rlocal"),
        workflow_id: WorkflowId::new("workflow\"local"),
        kind: "kind\\local".to_string(),
        policy_decision_id: None,
        payload: BTreeMap::new(),
        previous_hash: None,
        receipt_timestamp: "2026-01-01T00:00:00.000000000Z".to_string(),
        hash_version: mdx_core::RECEIPT_HASH_VERSION_TRUSTED_TIME,
        hash: "hash\"local".to_string(),
    };
    let json = render_receipt_json(&receipt);
    assert!(json.contains(r#"receipt\"1"#));
    assert!(json.contains(r#"tenant\\local"#));
    assert!(json.contains(r#"trace\nlocal"#));
    assert!(json.contains(r#"actor\tlocal"#));
    assert!(json.contains(r#"loop\rlocal"#));
    assert!(json.contains(r#"workflow\"local"#));
    assert!(json.contains(r#"kind\\local"#));
    assert!(json.contains(r#"hash\"local"#));
}
#[test]
fn generated_route_contract_matches_local_handler() {
    let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
    route_request("POST", "/run-loop/evals_runner_agent", &kernel).expect("seed receipts");
    let receipt_id = {
        let kernel = kernel.read().expect("kernel lock");
        kernel
            .ledger()
            .query()
            .receipt_ids()
            .first()
            .expect("first receipt")
            .to_string()
    };
    let route_contract = include_str!("../../../generated/routes/mdx-local-routes.json");
    let route_smoke_matrix = route_contract_smoke_matrix();
    let trace_route_timings =
        std::env::var("MDX_ROUTE_CONTRACT_TIMINGS").ok().as_deref() == Some("1");
    for route in local_http_routes() {
        assert!(route_contract.contains(&format!("\"method\": \"{}\"", route.method)));
        assert!(route_contract.contains(&format!("\"local_path\": \"{}\"", route.local_path)));
        assert!(route_contract.contains(&format!("\"openapi_path\": \"{}\"", route.openapi_path)));
        assert!(route_contract.contains(&format!("\"operation_id\": \"{}\"", route.operation_id)));
        assert!(route_contract.contains(&format!(
            "\"response_content_type\": \"{}\"",
            route.response_content_type
        )));
        match route.response_schema_path {
            Some(schema_path) => {
                assert!(
                    route_contract
                        .contains(&format!("\"response_schema_path\": \"{schema_path}\""))
                );
            }
            None => {
                assert!(route_contract.contains("\"response_schema_path\": null"));
            }
        }
        if route.response_content_type == "text/event-stream" {
            // Streaming routes own their TcpStream in handle_connection
            // and never pass through route_request_with_body; the real
            // HTTP smoke (scripts/local-http-smoke.sh) exercises them.
            continue;
        }
        let key = format!("{} {}", route.method, route.local_path);
        let smoke = route_smoke_matrix.get(&key);
        let smoke_path = smoke.map(|(path, _)| path.to_string()).unwrap_or_else(|| {
            route
                .local_path
                .replace("{receipt_id}", &receipt_id)
                .replace("{companion_id}", "twin_architect")
                .replace("{channel_id}", "local-ops")
                .replace("{document_id}", "page_evidence_evals_runner")
        });
        let smoke_body = smoke.map(|(_, body)| body.as_str()).unwrap_or("");
        let route_started_at = std::time::Instant::now();
        let response = route_request_with_body(route.method, &smoke_path, smoke_body, &kernel)
            .expect("route contract smoke");
        if trace_route_timings {
            eprintln!(
                "route_contract_timing ms={} {} {}",
                route_started_at.elapsed().as_millis(),
                route.method,
                smoke_path
            );
        }
        assert_eq!(
            response.status, "200 OK",
            "route {} {}",
            route.method, route.local_path
        );
        assert_eq!(response.content_type, route.response_content_type);
    }
}

fn route_contract_smoke_matrix() -> BTreeMap<String, (String, String)> {
    let matrix = include_str!("../../../generated/routes/mdx-runtime-route-smoke-matrix.json");
    let parsed: serde_json::Value = serde_json::from_str(matrix).expect("route smoke matrix json");
    let mut out = BTreeMap::new();
    for route in parsed["routes"].as_array().cloned().unwrap_or_default() {
        let Some(method) = route["method"].as_str() else {
            continue;
        };
        let Some(declared_path) = route["declared_path"].as_str() else {
            continue;
        };
        let smoke_path = route["smoke_path"].as_str().unwrap_or(declared_path);
        let smoke_body = route["smoke_body"].as_str().unwrap_or("");
        out.insert(
            format!("{method} {declared_path}"),
            (smoke_path.to_string(), smoke_body.to_string()),
        );
    }
    out
}

#[test]
fn forge_control_plane_projection_folds_gates_capacity_and_results() {
    use mdx_core::{
        FleetPlanDraft, FleetRunEvent, FleetStream, ForgeRunEvent, ForgeRunShipDecision,
        GovernedWriteIdentity,
    };

    let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
    {
        let mut kernel = kernel.write().expect("kernel");
        let stream = FleetStream {
            stream_id: "lane_api".to_string(),
            objective: "Add the API seam".to_string(),
            write_scope: vec!["src/api.rs".to_string()],
            interface_contract: "public API stays stable".to_string(),
            depends_on: vec![],
            checks: vec!["cargo test -p api".to_string()],
            max_turns: 20,
            builder_slot: "GROK".to_string(),
            data_sensitivity: "internal".to_string(),
        };
        kernel
            .record_fleet_plan_draft(
                FleetPlanDraft {
                    tenant_id: "local_tenant",
                    actor_id: "human:md",
                    fleet_id: "fleet_control_plane",
                    spec: "ship one governed lane",
                    goal: "ship one governed lane",
                    checks: &["cargo test".to_string()],
                    integration_owned_paths: &[],
                    full_suite_checks: &["cargo test".to_string()],
                    justification: "one stream is enough for this proof",
                    requested_width: 1,
                    planner_model: "planner-test",
                    bet_id: "",
                    repo_id: "",
                    repo_primary_language: "rust",
                    language_pack_id: "rust-cargo",
                    repo_profile_suggested_checks: "cargo test",
                    repo_profile_artifact_patterns: "target/**",
                    repo_profile_semantic_intelligence: "rust-analyzer",
                    repo_profile_semantic_tool_readiness: "semantic_query:available",
                    repo_profile_toolchain_readiness: "cargo:ready",
                    repo_profile_proof_plan_status: "ready",
                    repo_profile_proof_plan_summary: "cargo test available",
                    repo_profile_source: "test",
                    wide_plan_review_required: false,
                    wide_plan_review_status: "not_required",
                    wide_plan_review_reviewer_model: "",
                    wide_plan_review_verdict: "not_required",
                    wide_plan_review_confidence: "none",
                    wide_plan_review_concerns: "below threshold",
                },
                &[stream],
            )
            .expect("plan");
        kernel
            .ratify_fleet_plan_with_identity(
                "local_tenant",
                "human:md",
                "fleet_control_plane",
                "approved for projection test",
                &GovernedWriteIdentity::local_demo("human:md"),
            )
            .expect("ratify");
        kernel
            .record_fleet_run_event(FleetRunEvent {
                tenant_id: "local_tenant",
                actor_id: "human:md",
                fleet_id: "fleet_control_plane",
                event: "run_started",
                stream_id: "",
                forge_run_id: "",
                detail: "streams=1 requested_width=1 repo_id=mdx",
            })
            .expect("fleet started");
        kernel
            .record_fleet_run_event(FleetRunEvent {
                tenant_id: "local_tenant",
                actor_id: "agent:lane",
                fleet_id: "fleet_control_plane",
                event: "stream_finished",
                stream_id: "lane_api",
                forge_run_id: "forge_run_lane_api",
                detail: "status=RUN_FINISHED_DONE",
            })
            .expect("stream finished");
        kernel
            .record_forge_run_event(ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "agent:lane",
                run_id: "forge_run_lane_api",
                event: "run_started",
                work_item_id: "",
                detail: "intent=lane",
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("run started");
        kernel
            .record_forge_run_event(ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "agent:lane",
                run_id: "forge_run_lane_api",
                event: "check_failed",
                work_item_id: "",
                detail: "baseline cargo test -p api",
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("red baseline");
        kernel
            .record_forge_run_event(ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "agent:lane",
                run_id: "forge_run_lane_api",
                event: "check_passed",
                work_item_id: "",
                detail: "cargo test -p api",
                turn: 4,
                input_tokens: 10,
                output_tokens: 20,
            })
            .expect("check");
        kernel
            .record_forge_run_event(ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "agent:lane",
                run_id: "forge_run_lane_api",
                event: "evidence_appended",
                work_item_id: "",
                detail: "branch=forge/run-forge_run_lane_api",
                turn: 5,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("branch");
        kernel
            .record_forge_run_event(ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "agent:lane",
                run_id: "forge_run_lane_api",
                event: "run_finished",
                work_item_id: "",
                detail: "status=RUN_FINISHED_DONE branch=forge/run-forge_run_lane_api",
                turn: 5,
                input_tokens: 30,
                output_tokens: 40,
            })
            .expect("finish");
    }

    let response = route_request("GET", "/forge/control-plane/projection.json", &kernel)
        .expect("control plane projection");
    assert_eq!(response.status, "200 OK");
    assert_eq!(response.content_type, JSON_CONTENT_TYPE);
    for expected in [
        "\"name\":\"mdx-forge-control-plane-local-projection\"",
        "\"needs_you\"",
        "\"kind\":\"ship_call\"",
        "\"id\":\"ship_call:run:forge_run_lane_api\"",
        "\"subject_ref\":{\"type\":\"run\",\"id\":\"forge_run_lane_api\"}",
        "\"priority\":40",
        "\"primary_action\":{\"kind\":\"ship_call\"",
        "\"state\":\"review_required\"",
        "\"evidence_receipt_ids\":[\"forge_harness_run_receipt_",
        "\"grants_execution_authority\":false",
        "\"pipeline_counts\"",
        "\"capacity_pulse\"",
        "\"fleet_results\"",
        "\"label\":\"Fleet result\"",
        "\"status\":\"running\",\"lanes_clean\":1",
        "\"lane_results\"",
        "\"label\":\"Lane result\"",
        "\"label\":\"Lane result\",\"status\":\"clean\"",
        "\"fleet_id\":\"fleet_control_plane\"",
        "\"stream_id\":\"lane_api\"",
        "\"checks_passed\":1",
        "\"writes_allowed\":false",
        "\"authority_granted\":false",
        "\"production_write_allowed\":false",
    ] {
        assert!(
            response.body.contains(expected),
            "{expected}: {}",
            response.body
        );
    }
    let needs_you =
        route_request("GET", "/product/needs-you/projection.json", &kernel).expect("needs you");
    for expected in [
        "\"reason\":\"forge_ship_call\"",
        "\"route\":\"/forge/review\"",
        "run_id=forge_run_lane_api",
        "\"forge_ship_call\":1",
    ] {
        assert!(
            needs_you.body.contains(expected),
            "{expected}: {}",
            needs_you.body
        );
    }

    {
        let mut kernel = kernel.write().expect("kernel");
        kernel
            .record_forge_run_ship_decision_with_identity(
                ForgeRunShipDecision {
                    tenant_id: "local_tenant",
                    human_actor_id: "human:md",
                    run_id: "forge_run_lane_api",
                    commit_sha: "lane_tip_sha",
                    observed_branch_tip: "lane_tip_sha",
                    reason: "reviewed the lane and proof",
                },
                &GovernedWriteIdentity::local_demo("human:md"),
            )
            .expect("ship decision");
    }

    let after_ship = route_request("GET", "/forge/control-plane/projection.json", &kernel)
        .expect("control plane projection after ship");
    assert!(
        !after_ship.body.contains("\"kind\":\"ship_call\""),
        "{}",
        after_ship.body
    );
    let needs_you_after_ship = route_request("GET", "/product/needs-you/projection.json", &kernel)
        .expect("needs you after ship");
    assert!(
        !needs_you_after_ship
            .body
            .contains("\"reason\":\"forge_ship_call\""),
        "{}",
        needs_you_after_ship.body
    );
}

#[test]
fn forge_control_plane_projection_keeps_failed_attention_lane_dirty() {
    use mdx_core::{
        FleetPlanDraft, FleetRunEvent, FleetStream, ForgeRunEvent, GovernedWriteIdentity,
    };

    let mut kernel = MdxKernel::boot_local();
    kernel
        .record_fleet_plan_draft(
            FleetPlanDraft {
                tenant_id: "local_tenant",
                actor_id: "human:md",
                fleet_id: "fleet_attention_invariant",
                spec: "exercise a failed lane",
                goal: "keep terminal attention visible",
                checks: &["cargo test".to_string()],
                integration_owned_paths: &[],
                full_suite_checks: &["cargo test".to_string()],
                justification: "one lane pins the projection invariant",
                requested_width: 1,
                planner_model: "planner-test",
                bet_id: "",
                repo_id: "",
                repo_primary_language: "rust",
                language_pack_id: "rust-cargo",
                repo_profile_suggested_checks: "cargo test",
                repo_profile_artifact_patterns: "target/**",
                repo_profile_semantic_intelligence: "rust-analyzer",
                repo_profile_semantic_tool_readiness: "semantic_query:available",
                repo_profile_toolchain_readiness: "cargo:ready",
                repo_profile_proof_plan_status: "ready",
                repo_profile_proof_plan_summary: "cargo test available",
                repo_profile_source: "test",
                wide_plan_review_required: false,
                wide_plan_review_status: "not_required",
                wide_plan_review_reviewer_model: "",
                wide_plan_review_verdict: "not_required",
                wide_plan_review_confidence: "none",
                wide_plan_review_concerns: "below threshold",
            },
            &[FleetStream {
                stream_id: "lane_api".to_string(),
                objective: "Exercise the failed lane".to_string(),
                write_scope: vec!["src/api.rs".to_string()],
                interface_contract: "public API stays stable".to_string(),
                depends_on: vec![],
                checks: vec!["cargo test -p api".to_string()],
                max_turns: 20,
                builder_slot: "GROK".to_string(),
                data_sensitivity: "internal".to_string(),
            }],
        )
        .expect("plan");
    kernel
        .ratify_fleet_plan_with_identity(
            "local_tenant",
            "human:md",
            "fleet_attention_invariant",
            "approved for projection invariant test",
            &GovernedWriteIdentity::local_demo("human:md"),
        )
        .expect("ratify");
    kernel
        .record_fleet_run_event(FleetRunEvent {
            tenant_id: "local_tenant",
            actor_id: "human:md",
            fleet_id: "fleet_attention_invariant",
            event: "run_started",
            stream_id: "",
            forge_run_id: "",
            detail: "streams=1 requested_width=1 repo_id=mdx",
        })
        .expect("fleet started");
    kernel
        .record_forge_run_event(ForgeRunEvent {
            tenant_id: "local_tenant",
            actor_id: "agent:lane",
            run_id: "forge_run_failed_lane",
            event: "check_failed",
            work_item_id: "",
            detail: "cargo test -p api",
            turn: 3,
            input_tokens: 10,
            output_tokens: 20,
        })
        .expect("failed check");
    kernel
        .record_fleet_run_event(FleetRunEvent {
            tenant_id: "local_tenant",
            actor_id: "agent:lane",
            fleet_id: "fleet_attention_invariant",
            event: "stream_needs_attention",
            stream_id: "lane_api",
            forge_run_id: "forge_run_failed_lane",
            detail: "status=RUN_FINISHED_CANNOT_PROCEED",
        })
        .expect("lane needs attention");

    let kernel = Arc::new(RwLock::new(kernel));
    let response = route_request("GET", "/forge/control-plane/projection.json", &kernel)
        .expect("control plane projection");
    for expected in [
        "\"kind\":\"fleet_repair_escalation\"",
        "\"fleet_id\":\"fleet_attention_invariant\"",
        "\"stream_id\":\"lane_api\"",
        "\"label\":\"Lane result\",\"status\":\"needs_attention\"",
        "\"checks_failed\":1",
    ] {
        assert!(
            response.body.contains(expected),
            "{expected}: {}",
            response.body
        );
    }
}

#[test]
#[rustfmt::skip]
fn forge_run_projection_hides_builder_ticks_and_labels_system_runs() { let mut kernel = MdxKernel::boot_local(); let identity = GovernedWriteIdentity::local_demo("agent:forge_eval"); kernel.record_forge_run_event_with_evidence_fields(mdx_core::ForgeRunEvent { tenant_id: "local_tenant", actor_id: "agent:forge_eval", run_id: "native_improvement_loop_scheduled", event: "evidence_appended", work_item_id: "native_improvement_work_item", detail: "native_improvement_loop_scheduled note=distillation_note_policy_edge_001 builder_loop_tick=builder_loop_tick_070645", turn: 0, input_tokens: 0, output_tokens: 0 }, &identity, &[("builder_loop_tick_id", "builder_loop_tick_070645"), ("builder_loop_tick_receipt_id", "receipt_builder_loop_tick_070645")]).expect("builder tick event"); kernel.record_forge_run_event_with_evidence_fields(mdx_core::ForgeRunEvent { tenant_id: "local_tenant", actor_id: "agent:forge_eval", run_id: "forge_run_system_without_operator_title", event: "run_started", work_item_id: "system_work_item", detail: "system scheduled repo intake", turn: 0, input_tokens: 0, output_tokens: 0 }, &identity, &[]).expect("system run started"); let kernel = Arc::new(RwLock::new(kernel)); let projection = route_request("GET", "/forge/runs/projection.json", &kernel).expect("runs projection"); assert!(!projection.body.contains("\"run_id\":\"native_improvement_loop_scheduled\""), "{}", projection.body); for expected in ["\"run_id\":\"forge_run_system_without_operator_title\"", "\"origin\":\"system\"", "\"system_origin\":\"forge_system\"", "\"run_title\":\"System Forge run\""] { assert!(projection.body.contains(expected), "{expected}: {}", projection.body); } }

#[test]
#[rustfmt::skip]
fn twin_session_draft_route_exposes_admission_shape() { let kernel = Arc::new(RwLock::new(MdxKernel::boot_local())); let response = route_request("GET", "/twin/session-draft.json", &kernel).expect("twin session draft route"); assert_eq!(response.status, "200 OK"); assert_eq!(response.content_type, JSON_CONTENT_TYPE); for expected in ["\"name\": \"mdx-twin-session-persona-contract\"", "\"admission_result\": \"ADMITTED_LOCAL_DRAFT\"", "\"provider_call_allowed\": false", "\"memory_write_allowed\": false", "\"worker_spawn_allowed\": false", "\"refusal\": \"missing_actor\""] { assert!(response.body.contains(expected)); } }
#[test]
#[rustfmt::skip]
fn twin_session_drafts_post_route_accepts_local_auth_stub_and_records_memory() { let kernel = Arc::new(RwLock::new(MdxKernel::boot_local())); let response = route_request_with_body("POST", "/twin/session-drafts.json", r#"{"session_id":"twin_session_post_test","draft_text":"Test local Twin draft"}"#, &kernel).expect("twin session draft post route"); assert_eq!(response.status, "200 OK"); assert_eq!(response.content_type, JSON_CONTENT_TYPE); for expected in ["\"name\":\"mdx-twin-session-draft-local-post\"", "\"status\":\"SAVED_LOCAL_DRAFT\"", "\"auth_session_status\":\"ACCEPTED_LOCAL_STUB\"", "\"auth_session_user_id\":\"local_user\"", "\"draft_text\":\"Test local Twin draft\"", "\"memory_retrieval_receipt_id\"", "\"memory_scoring_receipt_id\"", "\"brain_recall_receipt_id\"", "\"recall_packet_id\":\"brain_recall_packet_", "\"brain_recall_scope\":\"private_user_session\"", "\"brain_recall_policy\":\"local_brain_recall_packet_v1\"", "\"brain_recall_source_count\":0", "\"brain_recall_token_budget\":2400", "\"conversation_summary_receipt_id\"", "\"conversation_summary\"", "\"memory_driver\":\"local_memory_store\"", "\"model_gateway_driver\":\"local_model_gateway\"", "\"model_gateway_model_id\":\"deterministic_local_v1\"", "\"production_write_allowed\":false"] { assert!(response.body.contains(expected), "{expected}"); } let kernel = kernel.read().expect("kernel lock"); assert!(kernel.memory_records().iter().any(|record| record.provenance.driver_id == "local_memory_store")); assert!(kernel.ledger().entries().iter().any(|receipt| receipt.kind == "twin.session.brain_recall.preflighted" && receipt.payload.get("private_memory_export_allowed").map(String::as_str) == Some("false"))); }
#[test]
#[rustfmt::skip]
fn feedback_capture_post_route_records_safe_feedback() { let kernel = Arc::new(RwLock::new(MdxKernel::boot_local())); let body = r#"{"surface":"forge","route":"/forge","session_ref":"sess_local","occurred_at":"2026-06-09T00:00:00Z","category":"blocked","surface_state":"error","blocked_reason":"production_missing_trusted_session","note":"could not request a build"}"#; let response = route_request_with_body("POST", "/feedback/captures.json", body, &kernel).expect("feedback post route"); assert_eq!(response.status, "200 OK"); assert_eq!(response.content_type, JSON_CONTENT_TYPE); for expected in ["\"name\":\"mdx-beta-feedback-capture-local-post\"", "\"status\":\"BETA_FEEDBACK_RECORDED\"", "\"auth_session_status\":\"ACCEPTED_LOCAL_STUB\"", "\"surface\":\"forge\"", "\"category\":\"blocked\"", "\"feedback_receipt_id\"", "\"context_count\":2", "\"has_note\":true", "\"production_write_allowed\":false"] { assert!(response.body.contains(expected), "{expected}"); } let kernel = kernel.read().expect("kernel lock"); assert!(kernel.ledger().entries().iter().any(|r| r.kind == "beta.feedback.captured")); }
#[test]
#[rustfmt::skip]
fn feedback_autonomy_route_closes_screenshot_feedback_loop() { let kernel = Arc::new(RwLock::new(MdxKernel::boot_local())); let body = r#"{"surface":"twin","route":"/","session_ref":"sess_local","occurred_at":"2026-06-12T00:00:00Z","category":"idea","feature_area":"feedback","note":"Right now there is no way to submit a screenshot as part of feedback."}"#; let capture = route_request_with_body("POST", "/feedback/captures.json", body, &kernel).expect("feedback post route"); assert!(capture.body.contains("BETA_FEEDBACK_RECORDED"), "feedback records: {}", capture.body); let run = route_request_with_body("POST", "/feedback/autonomy-runs.json", "{}", &kernel).expect("feedback autonomy run"); assert_eq!(run.status, "200 OK"); for expected in ["\"name\":\"mdx-feedback-autonomy-local-post\"", "\"status\":\"PROCESSED\"", "\"processed_count\":1", "\"skipped_count\":0", "\"lane\":\"autopilot_with_review\"", "\"lifecycle\":\"closed_loop_replied\"", "\"assessment_page_ref\":\"feedback_assessment_beta_feedback_receipt_", "\"closeout_page_ref\":\"feedback_closeout_beta_feedback_receipt_", "\"work_item_ref\":\"work_item_feedback_beta_feedback_receipt_", "\"forge_detail\":\"held for brief review before Forge because the change touches boundary or product scope\""] { assert!(run.body.contains(expected), "{expected}: {}", run.body); } let projection = route_request("GET", "/feedback/autonomy-runs/projection.json", &kernel).expect("feedback autonomy projection"); for expected in ["\"name\":\"mdx-feedback-autonomy-local-projection\"", "\"total_feedback\":1", "\"processed_count\":1", "\"open_count\":0", "\"channel_id\":\"beta-feedback\"", "\"acknowledged\":true", "\"assessed\":true", "\"assessment_published\":true", "\"work_shaped\":true", "\"forge_requested\":true", "\"evidence_attached\":true", "\"closeout_published\":true", "\"closed_loop_replied\":true"] { assert!(projection.body.contains(expected), "{expected}: {}", projection.body); } let rerun = route_request_with_body("POST", "/feedback/autonomy-runs.json", "{}", &kernel).expect("feedback autonomy rerun"); assert!(rerun.body.contains("\"processed_count\":0"), "idempotent rerun: {}", rerun.body); assert!(rerun.body.contains("\"skipped_count\":1"), "idempotent rerun: {}", rerun.body); let kernel = kernel.read().expect("kernel lock"); for kind in ["beta.feedback.captured", "message.feedback.acknowledged", "product.feedback.assessed", "pages.feedback.assessment_published", "work.item.shaped", "forge.work.requested", "forge.evidence.attached", "pages.change.closeout_published", "message.feedback.closed_loop_replied"] { assert!(kernel.ledger().entries().iter().any(|r| r.kind == kind), "missing receipt kind {kind}"); } }
#[test]
#[rustfmt::skip]
fn feedback_autonomy_route_starts_scoped_forge_for_safe_copy_feedback() { let kernel = Arc::new(RwLock::new(MdxKernel::boot_local())); let body = r#"{"surface":"twin","route":"/","session_ref":"sess_local","occurred_at":"2026-06-12T00:00:00Z","category":"confusing","feature_area":"feedback","note":"The feedback button label copy is confusing."}"#; let capture = route_request_with_body("POST", "/feedback/captures.json", body, &kernel).expect("feedback post route"); assert!(capture.body.contains("BETA_FEEDBACK_RECORDED"), "feedback records: {}", capture.body); let run = route_request_with_body("POST", "/feedback/autonomy-runs.json", "{}", &kernel).expect("feedback autonomy run"); assert_eq!(run.status, "200 OK"); for expected in ["\"lane\":\"autopilot_allowed\"", "\"scope_status\":\"scope_resolved\"", "\"write_scope\":[\"packages/ui/components/FeedbackButton.svelte\"]", "\"forge_run_ref\":\"forge_run_", "\"forge_detail\":\"scoped Forge run started with write scope: packages/ui/components/FeedbackButton.svelte\""] { assert!(run.body.contains(expected), "{expected}: {}", run.body); } let projection = route_request("GET", "/feedback/autonomy-runs/projection.json", &kernel).expect("feedback autonomy projection"); for expected in ["\"forge_run_ref\":\"forge_run_", "\"lane\":\"autopilot_allowed\"", "\"closed_loop_replied\":true"] { assert!(projection.body.contains(expected), "{expected}: {}", projection.body); } let kernel = kernel.read().expect("kernel lock"); assert!(kernel.ledger().entries().iter().any(|r| r.kind == "forge.run.event" && r.payload.get("event").map(String::as_str) == Some("run_started")), "scoped Forge run_started receipt exists"); }
#[test]
#[rustfmt::skip]
fn feedback_capture_post_route_refuses_unsafe_note_and_records_nothing() { let kernel = Arc::new(RwLock::new(MdxKernel::boot_local())); let body = r#"{"surface":"twin","route":"/","session_ref":"sess_local","occurred_at":"2026-06-09T00:00:00Z","category":"bug","note":"here is the model output: my password is hunter2"}"#; let response = route_request_with_body("POST", "/feedback/captures.json", body, &kernel).expect("feedback post route"); assert_eq!(response.status, "200 OK"); for expected in ["\"status\":\"BETA_FEEDBACK_REFUSED\"", "\"recorded\":false", "\"refusal_code\":\"unsafe_content\"", "MDx will not record"] { assert!(response.body.contains(expected), "{expected}"); } let kernel = kernel.read().expect("kernel lock"); assert!(!kernel.ledger().entries().iter().any(|r| r.kind == "beta.feedback.captured"), "no receipt for refused feedback"); }
#[test]
#[rustfmt::skip]
fn feedback_capture_route_rejects_non_post() { let kernel = Arc::new(RwLock::new(MdxKernel::boot_local())); let response = route_request("GET", "/feedback/captures.json", &kernel).expect("feedback get"); assert_eq!(response.status, "405 Method Not Allowed"); }
#[test]
#[rustfmt::skip]
fn watchtower_route_is_read_only_and_summarizes_safely() { let kernel = Arc::new(RwLock::new(MdxKernel::boot_local())); let response = route_request("GET", "/runtime/watchtower.json", &kernel).expect("watchtower route"); assert_eq!(response.status, "200 OK"); assert_eq!(response.content_type, JSON_CONTENT_TYPE); for expected in ["\"name\": \"mdx-runtime-watchtower\"", "\"health\":", "\"is_beta_healthy\"", "\"what_is_blocked\"", "\"queue_pressure\"", "\"feedback\"", "\"security_posture\"", "\"what_changed_recently\"", "\"look_at_next\"", "/admin/platform", "/admin/aegis", "\"status\": \"NOT_OBSERVED\"", "\"read_only\": true", "\"production_write_allowed\": false"] { assert!(response.body.contains(expected), "{expected}"); } let parsed: serde_json::Value = serde_json::from_str(&response.body).expect("watchtower body is valid json"); assert_eq!(parsed["read_only"], serde_json::Value::Bool(true)); }
#[test]
#[rustfmt::skip]
fn watchtower_counts_feedback_without_leaking_the_note() { let kernel = Arc::new(RwLock::new(MdxKernel::boot_local())); let secret_note = "DISTINCTIVE_OPERATOR_NOTE_ZZ9"; let body = format!(r#"{{"surface":"forge","route":"/forge","session_ref":"sess_w","occurred_at":"2026-06-09T00:00:00Z","category":"blocked","note":"{secret_note}"}}"#); let post = route_request_with_body("POST", "/feedback/captures.json", &body, &kernel).expect("feedback post"); assert!(post.body.contains("BETA_FEEDBACK_RECORDED"), "feedback should record: {}", post.body); let response = route_request("GET", "/runtime/watchtower.json", &kernel).expect("watchtower route"); assert_eq!(response.status, "200 OK"); let parsed: serde_json::Value = serde_json::from_str(&response.body).expect("valid json"); assert_eq!(parsed["feedback"]["total"], serde_json::json!(1)); assert!(response.body.contains("\"surface\": \"forge\""), "feedback by_surface should include forge: {}", response.body); assert!(!response.body.contains(secret_note), "watchtower must never echo a feedback note: {}", response.body); }
#[test]
fn install_owner_claim_records_owner_and_refuses_a_second_claim() {
    let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
    let fresh = route_request("GET", "/install/owner.json", &kernel).expect("owner projection");
    assert!(
        fresh.body.contains("\"claimed\":false"),
        "fresh install has no owner: {}",
        fresh.body
    );

    let claim = route_request_with_body(
        "POST",
        "/install/owner-claims.json",
        r#"{"owner_name":"Dana"}"#,
        &kernel,
    )
    .expect("claim");
    assert_eq!(claim.status, "200 OK");
    for expected in [
        "\"status\":\"INSTALL_OWNER_CLAIMED\"",
        "\"claimed\":true",
        "\"already_claimed\":false",
        "\"owner_name\":\"Dana\"",
    ] {
        assert!(claim.body.contains(expected), "{expected}: {}", claim.body);
    }

    let after =
        route_request("GET", "/install/owner.json", &kernel).expect("owner projection after");
    assert!(
        after.body.contains("\"claimed\":true") && after.body.contains("\"owner_name\":\"Dana\""),
        "projection shows owner: {}",
        after.body
    );

    let second = route_request_with_body(
        "POST",
        "/install/owner-claims.json",
        r#"{"owner_name":"Imposter"}"#,
        &kernel,
    )
    .expect("second claim");
    assert!(
        second
            .body
            .contains("\"status\":\"INSTALL_OWNER_ALREADY_CLAIMED\""),
        "second claim refused: {}",
        second.body
    );
    assert!(
        second.body.contains("\"owner_name\":\"Dana\""),
        "ownership not overwritten: {}",
        second.body
    );

    let kernel = kernel.read().expect("kernel lock");
    assert_eq!(
        kernel
            .ledger()
            .entries()
            .iter()
            .filter(|r| r.kind == "install.owner.claimed")
            .count(),
        1,
        "exactly one owner receipt"
    );
}
#[test]
#[rustfmt::skip]
fn feedback_capture_post_route_refuses_forbidden_top_level_field_and_records_nothing() { for bad_field in ["prompt", "output", "answer", "secret", "token", "api_key", "password", "message_body", "page_body"] { let kernel = Arc::new(RwLock::new(MdxKernel::boot_local())); let body = format!(r#"{{"surface":"twin","route":"/","category":"bug","occurred_at":"2026-06-09T00:00:00Z","note":"hi","{bad_field}":"leaked value"}}"#); let response = route_request_with_body("POST", "/feedback/captures.json", &body, &kernel).expect("feedback post route"); assert_eq!(response.status, "200 OK"); for expected in ["\"status\":\"BETA_FEEDBACK_REFUSED\"", "\"recorded\":false", "\"refusal_code\":\"unsafe_content\""] { assert!(response.body.contains(expected), "{bad_field}: {expected}"); } let kernel = kernel.read().expect("kernel lock"); assert!(!kernel.ledger().entries().iter().any(|r| r.kind == "beta.feedback.captured"), "forbidden top-level field {bad_field} must record nothing"); } }
#[test]
#[rustfmt::skip]
fn feedback_capture_post_route_refuses_unknown_top_level_field_and_records_nothing() { let kernel = Arc::new(RwLock::new(MdxKernel::boot_local())); let body = r#"{"surface":"twin","route":"/","category":"bug","occurred_at":"2026-06-09T00:00:00Z","note":"hi","evil_extra_field":"x"}"#; let response = route_request_with_body("POST", "/feedback/captures.json", body, &kernel).expect("feedback post route"); assert_eq!(response.status, "200 OK"); for expected in ["\"status\":\"BETA_FEEDBACK_REFUSED\"", "\"recorded\":false", "\"refusal_code\":\"unknown_field\""] { assert!(response.body.contains(expected), "{expected}"); } let kernel = kernel.read().expect("kernel lock"); assert!(!kernel.ledger().entries().iter().any(|r| r.kind == "beta.feedback.captured"), "unknown top-level field must record nothing"); }
#[test]
#[rustfmt::skip]
fn twin_session_draft_projection_reads_saved_local_drafts() { let kernel = Arc::new(RwLock::new(MdxKernel::boot_local())); route_request_with_body("POST", "/twin/session-drafts.json", r#"{"session_id":"twin_session_projection_test","draft_text":"Projection test draft"}"#, &kernel).expect("post draft"); let response = route_request("GET", "/twin/session-drafts/projection.json", &kernel).expect("draft projection"); assert_eq!(response.status, "200 OK"); assert_eq!(response.content_type, JSON_CONTENT_TYPE); for expected in ["\"name\":\"mdx-twin-session-draft-local-projection\"", "\"draft_count\":1", "\"memory_record_count\":1", "\"writes_route\":\"/twin/session-drafts.json\"", "\"session_id\":\"twin_session_projection_test\"", "\"draft_text\":\"Projection test draft\"", "\"memory_retrieval_receipt_id\"", "\"brain_recall_receipt_id\"", "\"recall_packet_id\":\"brain_recall_packet_", "\"brain_recall_scope\":\"private_user_session\"", "\"brain_recall_policy\":\"local_brain_recall_packet_v1\"", "\"brain_recall_source_count\":0", "\"brain_recall_token_budget\":2400", "\"conversation_summary_receipt_id\"", "\"retrieval_driver\":\"local_memory_store\"", "\"summary_state\":\"LOCAL_ONLY\"", "\"message_count\":2", "\"memory_driver\":\"local_memory_store\"", "\"model_gateway_driver\":\"local_model_gateway\"", "\"grounded_answer\"", "\"production_write_allowed\":false"] { assert!(response.body.contains(expected), "{expected}"); } let memory = route_request("GET", "/memory/records.json", &kernel).expect("memory records route"); assert_eq!(memory.status, "200 OK"); for expected in ["\"name\":\"mdx-memory-store-records\"", "\"read_only\":true", "\"writes_allowed\":false", "\"vendor_memory_driver\":\"mem0_memory_store\"", "\"vendor_status\":\"PENDING-LIVE-RUN\"", "\"episode_id\":\"episode_memory_", "\"memory_scope\":\"private_user_memory\"", "\"memory_tier\":\"episodic\"", "\"decay_policy\":\"local_recent_session_decay_v1\"", "\"importance_score\":70", "\"content\":\"Projection test draft\""] { assert!(memory.body.contains(expected), "{expected}"); } }

#[test]
fn memory_brain_map_route_projects_read_phase_one() {
    let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
    route_request_with_body(
        "POST",
        "/twin/session-drafts.json",
        r#"{"session_id":"brain_map_seed","draft_text":"Brain map seed memory"}"#,
        &kernel,
    )
    .expect("seed memory through governed Twin draft");
    let response =
        route_request("GET", "/memory/brain-map.json", &kernel).expect("brain map route");
    assert_eq!(response.status, "200 OK");
    assert_eq!(response.content_type, JSON_CONTENT_TYPE);
    for expected in [
        "\"name\":\"mdx-memory-brain-map\"",
        "\"status\":\"READ_PHASE_1\"",
        "\"route\":\"/memory/brain-map.json\"",
        "\"writes_allowed\":false",
        "\"memory_driver\":\"local_memory_store\"",
        "\"memory_provider\":\"InMemoryProvider\"",
        "\"durable_driver\":\"postgres_memory_records\"",
        "\"vendor_memory_driver\":\"mem0_memory_store\"",
        "\"vendor_status\":\"PENDING-LIVE-RUN\"",
        "\"source_contract\":\"docs/MDX-MEMORY-BRAIN-RUNWAY.md\"",
        "\"runtime_influence_allowed\":true",
        "\"runtime_influence_scope\":\"prompt_context_only\"",
        "\"adaptation_allowed\":false",
        "\"provider_call_allowed\":false",
        "\"production_write_allowed\":false",
        "\"memory_record_count\":",
        "\"scale_fixture_targets\":[1,10,50,100,500,1000,5000]",
        "\"LoCoMo\"",
        "\"LongMemEval-V2\"",
        "\"BEAM\"",
        "\"MemoryEpisode\"",
        "\"RecallPacket\"",
        "\"private_user_memory\"",
        "\"shared_project_memory\"",
        "\"kernel_memory_records\"",
        "\"twin_brain_recall_preflight\"",
    ] {
        assert!(response.body.contains(expected), "{expected}");
    }
}

#[test]
#[rustfmt::skip]
fn memory_brain_substrate_route_projects_durable_graph_eval_and_topology() { let kernel = Arc::new(RwLock::new(MdxKernel::boot_local())); route_request_with_body("POST", "/twin/session-drafts.json", r#"{"session_id":"brain_substrate_session","draft_text":"Brain substrate memory seed"}"#, &kernel).expect("seed memory"); let response = route_request("GET", "/memory/brain-substrate.json", &kernel).expect("brain substrate route"); assert_eq!(response.status, "200 OK"); assert_eq!(response.content_type, JSON_CONTENT_TYPE); for expected in ["\"name\":\"mdx-memory-brain-substrate\"", "\"status\":\"LOCAL_SUBSTRATE_READY\"", "\"route\":\"/memory/brain-substrate.json\"", "\"brain_map_route\":\"/memory/brain-map.json\"", "\"memory_records_route\":\"/memory/records.json\"", "\"memory_record_count\":", "\"episode_count\":", "\"driver\":\"postgres_memory_records\"", "\"restart_replay_proof\":\"deterministic_sql_export\"", "\"live_database_write_allowed\":false", "\"policy\":\"local_session_summary_v1\"", "\"human_review_required\":true", "\"promotion_allowed\":false", "\"graph_id\":\"local_memory_graph_v1\"", "\"MemoryEpisode\"", "\"MemoryAtom\"", "\"RETRIEVAL_TRAVERSES\"", "\"write_allowed\":true", "\"score_name\":\"MDx Brain Score\"", "\"self_retrieval_smoke\"", "\"target_sessions_supported\":1000", "\"latency_budget_ms\":250", "\"brain-api\"", "\"consolidation-worker\"", "\"valkey-hot-cache\"", "\"shared_memory_allowed\":true", "\"private_memory_export_allowed\":false", "\"vendor_swap_allowed\":false"] { assert!(response.body.contains(expected), "{expected}"); } }

#[test]
#[rustfmt::skip]
fn memory_brain_runtime_route_projects_live_consolidation_graph_eval_governance() { let kernel = Arc::new(RwLock::new(MdxKernel::boot_local())); let seeded = route_request_with_body("POST", "/twin/session-drafts.json", r#"{"session_id":"brain_runtime_session","draft_text":"Runtime memory seed"}"#, &kernel).expect("seed memory"); assert_eq!(seeded.status, "200 OK", "seed body: {}", seeded.body); route_request_with_body("POST", "/memory/brain-eval-runs.json", r#"{"reason":"runtime projection eval seed"}"#, &kernel).expect("eval run seed"); route_request_with_body("POST", "/memory/topology-validations.json", r#"{"reason":"runtime projection topology seed"}"#, &kernel).expect("topology seed"); let response = route_request("GET", "/memory/brain-runtime.json", &kernel).expect("brain runtime route"); assert_eq!(response.status, "200 OK"); assert_eq!(response.content_type, JSON_CONTENT_TYPE); for expected in ["\"name\":\"mdx-memory-brain-runtime\"", "\"status\":\"LOCAL_RUNTIME_READY\"", "\"route\":\"/memory/brain-runtime.json\"", "\"graph_route\":\"/memory/graph.json\"", "\"ranking_route\":\"/memory/recall-rankings.json\"", "\"proposal_count\":1", "\"review_count\":1", "\"approved_review_count\":1", "\"retained_memory_count\":", "\"proposal_receipt_kind\":\"memory.consolidation.proposed\"", "\"review_receipt_kind\":\"memory.consolidation.reviewed\"", "\"graph_id\":\"local_memory_graph_v1\"", "\"origin\":\"asserted\"", "\"temporal_truth_requires_trusted_time\":true", "\"score_name\":\"MDx Brain Score\"", "\"target_sessions_supported\":1000", "\"ranking_count\":", "\"share_requires_review\":true", "\"private_memory_export_allowed\":false", "\"mem0\"", "\"zep\"", "\"graphiti\"", "\"brain-worker\"", "\"valkey-hot-cache\"", "\"production_write_allowed\":false"] { assert!(response.body.contains(expected), "{expected}; body: {}", response.body); } }

#[test]
#[rustfmt::skip]
fn memory_brain_routes_expose_real_graph_lifecycle_ranking_eval_governance_and_topology() { let kernel = Arc::new(RwLock::new(MdxKernel::boot_local())); route_request_with_body("POST", "/twin/session-drafts.json", r#"{"session_id":"brain_real_routes_session","draft_text":"Twin should recall durable project context"}"#, &kernel).expect("seed memory"); for (route, expected) in [("/memory/graph.json", "\"status\":\"LOCAL_GRAPH_STORAGE_READY\""), ("/memory/lifecycle.json", "\"status\":\"LOCAL_LIFECYCLE_READY\""), ("/memory/recall-rankings.json", "\"status\":\"LOCAL_RANKING_READY\""), ("/memory/brain-evals.json", "\"status\":\"LOCAL_EVAL_HARNESS_READY\""), ("/memory/governance.json", "\"status\":\"LOCAL_SCOPE_GOVERNANCE_READY\""), ("/memory/vendor-comparators.json", "\"status\":\"LOCAL_COMPARATOR_READY\""), ("/memory/topology.json", "\"status\":\"LOCAL_TOPOLOGY_READY\"")] { let response = route_request("GET", route, &kernel).expect(route); assert_eq!(response.status, "200 OK"); assert_eq!(response.content_type, JSON_CONTENT_TYPE); assert!(response.body.contains(expected), "{route}: {expected}; body: {}", response.body); } let graph = route_request("GET", "/memory/graph.json", &kernel).expect("graph"); for expected in ["\"node_storage\":\"memory_graph_nodes\"", "\"edge_storage\":\"memory_graph_edges\"", "\"MemoryAtom\"", "\"DERIVED_FROM\"", "\"RETRIEVAL_TRAVERSES\""] { assert!(graph.body.contains(expected), "{expected}"); } let rankings = route_request("GET", "/memory/recall-rankings.json", &kernel).expect("rankings"); for expected in ["\"lexical_score\"", "\"content_checksum_score\"", "\"graph_score\"", "\"source_authority_score\"", "\"rank\":1"] { assert!(rankings.body.contains(expected), "{expected}"); } let action = route_request_with_body("POST", "/memory/lifecycle-actions.json", r#"{"action":"suppress","reason":"test suppresses stale memory"}"#, &kernel).expect("lifecycle action"); assert_eq!(action.status, "200 OK"); assert!(action.body.contains("\"lifecycle_state\":\"suppressed\"")); let lifecycle = route_request("GET", "/memory/lifecycle.json", &kernel).expect("lifecycle"); assert!(lifecycle.body.contains("\"action\":\"suppress\"")); assert!(lifecycle.body.contains("\"trusted_time_required\":true")); }

#[test]
fn pages_and_messages_write_through_shared_memory_substrate() {
    let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
    let draft_receipt_id = seed_pages_edit_draft_for_test(
        &kernel,
        "local_tenant",
        "human:local_user",
        "page_memory_surface",
        "draft_memory_surface",
        (
            "Memory Surface Page",
            "world_model://pages/page_memory_surface/body/v1",
            "rev_memory_surface",
        ),
    );
    let request_receipt_id = request_pages_approval_for_test(
        &kernel,
        "page_memory_surface",
        "draft_memory_surface",
        &draft_receipt_id,
        "approval_memory_surface",
    );
    let decision_receipt_id = decide_pages_approval_for_test(
        &kernel,
        &request_receipt_id,
        "decision_memory_surface",
        "approved",
    );
    let pages_body = format!(
        r#"{{"document_id":"page_memory_surface","approval_decision_receipt_id":"{decision_receipt_id}","page_type":"spec"}}"#
    );
    let pages = route_request_with_body("POST", "/pages/publications.json", &pages_body, &kernel)
        .expect("pages post");
    assert!(pages.body.contains("\"human_approval_granted\":true"));
    let message = route_request_with_body(
        "POST",
        "/messages/thread-messages.json",
        r#"{"message_id":"message_memory_surface","body":"Remember this team decision"}"#,
        &kernel,
    )
    .expect("message post");
    assert_eq!(message.status, "200 OK");
    let memory = route_request("GET", "/memory/records.json", &kernel).expect("memory records");
    for expected in [
        "\"memory_scope\":\"company_memory\"",
        "\"memory_scope\":\"team_memory\"",
        "\"content\":\"Published page page_memory_surface titled Memory Surface Page\"",
        "\"content\":\"Remember this team decision\"",
    ] {
        assert!(
            memory.body.contains(expected),
            "{expected}; body: {}",
            memory.body
        );
    }
    let graph = route_request("GET", "/memory/graph.json", &kernel).expect("memory graph");
    for expected in ["\"label\":\"pages\"", "\"label\":\"messages\""] {
        assert!(
            graph.body.contains(expected),
            "{expected}; body: {}",
            graph.body
        );
    }
}

#[test]
fn memory_brain_routes_run_lifecycle_evaluation_and_eval_harness() {
    let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
    route_request_with_body(
        "POST",
        "/twin/session-drafts.json",
        r#"{"session_id":"brain_eval_route_session","draft_text":"Twin should remember eval route proof"}"#,
        &kernel,
    )
    .expect("seed memory");

    let eval = route_request_with_body(
        "POST",
        "/memory/brain-eval-runs.json",
        r#"{"reason":"route measured eval"}"#,
        &kernel,
    )
    .expect("eval route");
    assert_eq!(eval.status, "200 OK");
    assert!(eval.body.contains("\"status\":\"MEASURED\""));
    assert!(eval.body.contains("\"mdx_brain_score\""));

    let lifecycle = route_request_with_body(
        "POST",
        "/memory/lifecycle-evaluations.json",
        r#"{"reason":"route lifecycle eval"}"#,
        &kernel,
    )
    .expect("lifecycle eval route");
    assert_eq!(lifecycle.status, "200 OK");
    assert!(lifecycle.body.contains("\"status\":\"EVALUATED\""));
    assert!(lifecycle.body.contains("\"trusted_time_required\":true"));

    let evals = route_request("GET", "/memory/brain-evals.json", &kernel).expect("evals");
    assert!(evals.body.contains("\"fixture_result_count\""));
    assert!(evals.body.contains("\"fixture_results\""));
    let lifecycle_projection =
        route_request("GET", "/memory/lifecycle.json", &kernel).expect("lifecycle");
    assert!(lifecycle_projection.body.contains("\"evaluation_count\""));
    assert!(
        lifecycle_projection
            .body
            .contains("\"evaluation_storage\":\"memory_lifecycle_evaluations\"")
    );
}

#[test]
fn memory_brain_routes_run_topology_validation() {
    let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
    route_request_with_body(
        "POST",
        "/twin/session-drafts.json",
        r#"{"session_id":"brain_topology_route_session","draft_text":"Twin should seed topology proof"}"#,
        &kernel,
    )
    .expect("seed memory");

    let response = route_request_with_body(
        "POST",
        "/memory/topology-validations.json",
        r#"{"reason":"route topology proof"}"#,
        &kernel,
    )
    .expect("topology validation");
    assert_eq!(response.status, "200 OK");
    assert!(response.body.contains("\"status\":\"VALIDATED\""));
    assert!(response.body.contains("\"runtime_event_count\":4"));

    let topology = route_request("GET", "/memory/topology.json", &kernel).expect("topology");
    assert!(
        topology
            .body
            .contains("\"runtime_event_storage\":\"memory_topology_runtime_events\"")
    );
    assert!(
        topology
            .body
            .contains("\"event_kind\":\"cache_hit_recall_packet\"")
    );
    assert!(
        topology
            .body
            .contains("\"event_kind\":\"enqueue_consolidation_job\"")
    );
}

#[test]
fn memory_brain_routes_run_beta_readiness_gate() {
    let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

    let response = route_request_with_body(
        "POST",
        "/memory/beta-readiness-runs.json",
        r#"{"synthetic_sessions":"1000"}"#,
        &kernel,
    )
    .expect("beta readiness");
    assert_eq!(response.status, "200 OK");
    // The drill is synthetic and the CLI status says so; readiness for a real
    // beta is never claimed from this run.
    assert!(
        response
            .body
            .contains("\"status\":\"LOCAL_SYNTHETIC_DRILL_COMPLETE\"")
    );
    assert!(response.body.contains("\"synthetic_session_count\":1000"));
    assert!(response.body.contains("\"memory_record_count\":1000"));
    assert!(
        response
            .body
            .contains("\"vendor_dependency_required\":false")
    );

    let projection = route_request("GET", "/memory/beta-readiness.json", &kernel)
        .expect("beta readiness projection");
    assert!(
        projection
            .body
            .contains("\"status\":\"LOCAL_SYNTHETIC_DRILL_RECORDED\"")
    );
    assert!(
        projection
            .body
            .contains("\"ready_for_cloud_turn_on\":false")
    );
    assert!(
        projection
            .body
            .contains("\"fixture_family\":\"synthetic_placeholder\"")
    );
    assert!(
        projection
            .body
            .contains("\"source_kind\":\"synthetic_placeholder\"")
    );
    assert!(projection.body.contains("\"check_kind\":\"cloud_turn_on\""));
    assert!(projection.body.contains("\"status\":\"NOT_IMPORTED\""));
}

#[test]
#[rustfmt::skip]
fn message_thread_message_post_route_records_message_and_projection() { let kernel = Arc::new(RwLock::new(MdxKernel::boot_local())); let response = route_request_with_body("POST", "/messages/thread-messages.json", r#"{"message_id":"message_post_test","body":"Message route test"}"#, &kernel).expect("message thread message post route"); assert_eq!(response.status, "200 OK"); assert_eq!(response.content_type, JSON_CONTENT_TYPE); for expected in ["\"name\":\"mdx-message-thread-message-local-post\"", "\"status\":\"MESSAGE_APPENDED_FANOUT_BLOCKED\"", "\"auth_session_status\":\"ACCEPTED_LOCAL_STUB\"", "\"message_id\":\"message_post_test\"", "\"trusted_context_citation\":false", "\"trusted_context_projection_route\":\"/pages/context-sources/projection.json\"", "\"llm_allowed_on_hot_path\":false", "\"realtime_fanout_allowed\":false", "\"production_write_allowed\":false"] { assert!(response.body.contains(expected), "{expected}"); } let projection = route_request("GET", "/messages/thread-messages/projection.json", &kernel).expect("message projection route"); assert_eq!(projection.status, "200 OK"); for expected in ["\"name\":\"mdx-message-thread-message-local-projection\"", "\"message_count\":1", "\"writes_route\":\"/messages/thread-messages.json\"", "\"message_id\":\"message_post_test\"", "\"body\":\"Message route test\"", "\"trusted_context_citation\":false"] { assert!(projection.body.contains(expected), "{expected}"); } }
#[test]
fn message_thread_message_route_records_agent_event_for_bridge_consumers() {
    let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
    let response = route_request_with_body(
        "POST",
        "/messages/thread-messages.json",
        r#"{"message_id":"forge_agent_bridge_post","actor_id":"agent:forge","channel_id":"forge","thread_id":"thread_forge","content_type":"card","body":"Forge prepared the governed plan.","content_payload":"{\"title\":\"Governed plan\",\"state\":\"ready_for_review\"}"}"#,
        &kernel,
    )
    .expect("agent message thread message post route");
    assert_eq!(response.status, "200 OK");
    for expected in [
        "\"message_receipt_kind\":\"message.agent.posted\"",
        "\"actor_type\":\"agent\"",
        "\"actor_role\":\"agent\"",
        "\"content_type\":\"card\"",
        "\"outbox_topic\":\"message.agent.posted\"",
    ] {
        assert!(
            response.body.contains(expected),
            "{expected}: {}",
            response.body
        );
    }

    let projection = route_request(
        "GET",
        "/messages/thread-messages/projection.json?channel_id=forge",
        &kernel,
    )
    .expect("message projection route");
    for expected in [
        "\"message_id\":\"forge_agent_bridge_post\"",
        "\"message_receipt_kind\":\"message.agent.posted\"",
        "\"actor_id\":\"agent:forge\"",
        "\"actor_type\":\"agent\"",
        "\"content_type\":\"card\"",
        "Governed plan",
    ] {
        assert!(
            projection.body.contains(expected),
            "{expected}: {}",
            projection.body
        );
    }
}
#[test]
#[rustfmt::skip]
fn message_presence_request_post_route_records_request_and_projection() { let kernel = Arc::new(RwLock::new(MdxKernel::boot_local())); let response = route_request_with_body("POST", "/messages/presence-requests.json", r#"{"presence_request_id":"presence_post_test","thread_id":"thread_local_receipts","channel_id":"local-ops","presence_scope":"local_thread"}"#, &kernel).expect("message presence request post route"); assert_eq!(response.status, "200 OK"); assert_eq!(response.content_type, JSON_CONTENT_TYPE); for expected in ["\"name\":\"mdx-message-presence-request-local-post\"", "\"status\":\"PRESENCE_REQUEST_RECORDED_REALTIME_BLOCKED\"", "\"auth_session_status\":\"ACCEPTED_LOCAL_STUB\"", "\"presence_request_id\":\"presence_post_test\"", "\"realtime_presence_allowed\":false", "\"websocket_fanout_allowed\":false", "\"typing_indicator_allowed\":false", "\"provider_call_allowed\":false", "\"production_write_allowed\":false"] { assert!(response.body.contains(expected), "{expected}"); } let projection = route_request("GET", "/messages/presence-requests/projection.json", &kernel).expect("message presence projection route"); assert_eq!(projection.status, "200 OK"); for expected in ["\"name\":\"mdx-message-presence-request-local-projection\"", "\"presence_request_count\":1", "\"writes_route\":\"/messages/presence-requests.json\"", "\"presence_request_id\":\"presence_post_test\"", "\"terminal_state\":\"PRESENCE_REQUEST_RECORDED_REALTIME_BLOCKED\""] { assert!(projection.body.contains(expected), "{expected}"); } }
#[test]
fn message_action_routes_record_verdicts_and_append_typed_thread_events() {
    let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
    let request = route_request_with_body(
        "POST",
        "/messages/action-requests.json",
        r#"{"action_request_id":"message_action_route_test","actor_id":"agent:forge","thread_id":"local","channel_id":"forge","title":"Approve the Forge plan","summary":"A governed action needs your call.","proposed_action":"Approve the plan boundary before any worker can run.","action_payload":"{\"plan_hash\":\"plan-action-route-001\"}"}"#,
        &kernel,
    )
    .expect("message action request route");
    assert_eq!(request.status, "200 OK");
    for expected in [
        "\"name\":\"mdx-message-action-request-local-post\"",
        "\"status\":\"MESSAGE_ACTION_REQUEST_RECORDED_EXECUTION_BLOCKED\"",
        "\"action_request_id\":\"message_action_route_test\"",
        "\"message_receipt_id\":",
        "\"allowed_verdicts\":\"approve,edit,reject,respond\"",
        "\"gate_state\":\"WAITING_FOR_HUMAN_VERDICT\"",
        "\"execution_allowed\":false",
        "\"production_write_allowed\":false",
    ] {
        assert!(
            request.body.contains(expected),
            "{expected}: {}",
            request.body
        );
    }
    let action_request_receipt_id =
        json_string_field_for_test(&request.body, "action_request_receipt_id");
    let needs_you_before_verdict =
        route_request("GET", "/product/needs-you/projection.json", &kernel).expect("needs you");
    for expected in [
        "\"surface\":\"message\"",
        "\"reason\":\"message_action\"",
        "\"href\":\"/message\"",
        "\"message_action\":1",
        "\"message\":1",
        &format!("\"receipt_id\":\"{action_request_receipt_id}\""),
    ] {
        assert!(
            needs_you_before_verdict.body.contains(expected),
            "{expected}: {}",
            needs_you_before_verdict.body
        );
    }
    let verdict_body = format!(
        r#"{{"action_verdict_id":"message_action_verdict_route_test","action_request_receipt_id":"{}","verdict":"approve","decision_note":"Approve the boundary. Execution stays blocked."}}"#,
        action_request_receipt_id
    );
    let verdict = route_request_with_body(
        "POST",
        "/messages/action-verdicts.json",
        &verdict_body,
        &kernel,
    )
    .expect("message action verdict route");
    for expected in [
        "\"name\":\"mdx-message-action-verdict-local-post\"",
        "\"status\":\"MESSAGE_ACTION_VERDICT_RECORDED_EXECUTION_BLOCKED\"",
        "\"verdict\":\"approve\"",
        "\"gate_state\":\"NO_SUPPORTED_ACTION_BOUND\"",
        "\"applied_receipt_kind\":\"\"",
        "\"applied_receipt_id\":\"\"",
        "\"human_decision_recorded\":true",
        "\"human_approval_granted\":true",
        "\"execution_allowed\":false",
        "\"production_write_allowed\":false",
    ] {
        assert!(
            verdict.body.contains(expected),
            "{expected}: {}",
            verdict.body
        );
    }
    let actions = route_request("GET", "/messages/action-requests/projection.json", &kernel)
        .expect("action request projection");
    for expected in [
        "\"name\":\"mdx-message-action-request-local-projection\"",
        "\"action_request_count\":1",
        "\"title\":\"Approve the Forge plan\"",
    ] {
        assert!(
            actions.body.contains(expected),
            "{expected}: {}",
            actions.body
        );
    }
    let verdicts = route_request("GET", "/messages/action-verdicts/projection.json", &kernel)
        .expect("action verdict projection");
    for expected in [
        "\"name\":\"mdx-message-action-verdict-local-projection\"",
        "\"action_verdict_count\":1",
        "\"gate_state\":\"NO_SUPPORTED_ACTION_BOUND\"",
        "\"applied_receipt_kind\":\"\"",
        "\"applied_receipt_id\":\"\"",
    ] {
        assert!(
            verdicts.body.contains(expected),
            "{expected}: {}",
            verdicts.body
        );
    }
    let needs_you_after_verdict =
        route_request("GET", "/product/needs-you/projection.json", &kernel).expect("needs you");
    assert!(
        !needs_you_after_verdict
            .body
            .contains("\"reason\":\"message_action\""),
        "{}",
        needs_you_after_verdict.body
    );
    let thread = route_request(
        "GET",
        "/messages/thread-messages/projection.json?channel_id=forge",
        &kernel,
    )
    .expect("thread projection");
    for expected in [
        "\"actor_id\":\"agent:forge\"",
        "\"content_type\":\"action_request\"",
        "\"content_type\":\"action_response\"",
        "\"source_receipt_kind\":\"message.action.requested\"",
        "\"source_receipt_kind\":\"message.action.verdict.recorded\"",
        "\"message_receipt_kind\":\"message.agent.posted\"",
        "\"message_receipt_kind\":\"message.human.posted\"",
        "\\\"gate_state\\\":\\\"NO_SUPPORTED_ACTION_BOUND\\\"",
        "Approve the plan boundary before any worker can run.",
    ] {
        assert!(
            thread.body.contains(expected),
            "{expected}: {}",
            thread.body
        );
    }
}

#[test]
fn message_action_approval_can_create_pages_draft_approval_request_without_publish() {
    let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
    let source = route_request_with_body(
        "POST",
        "/messages/thread-messages.json",
        r#"{"message_id":"message_pages_crystallize_source","thread_id":"thread_pages","channel_id":"strategy","body":"Decision: ship the trust rail first."}"#,
        &kernel,
    )
    .expect("message source route");
    assert_eq!(source.status, "200 OK");
    let source_receipt_id = json_string_field_for_test(&source.body, "message_receipt_id");
    let payload = r#"{\"kind\":\"pages_draft_approval_request\",\"document_id\":\"page_message_crystallize_test\",\"draft_id\":\"message_page_draft_crystallize_test\",\"approval_request_id\":\"message_pages_approval_crystallize_test\",\"title\":\"Message Crystallize Test\",\"body_text\":\"Decision: ship the trust rail first.\",\"requested_visibility\":\"tenant_review\",\"revision_id\":\"rev_message_crystallize_test\"}"#;
    let request_body = format!(
        r#"{{"action_request_id":"message_pages_crystallize_request","actor_id":"agent:assistant","thread_id":"thread_pages","channel_id":"strategy","source_receipt_id":"{}","title":"Create a Pages review draft","summary":"Message should crystallize this decision into a Pages draft for review.","proposed_action":"Create a draft and request Pages approval without publishing.","action_payload":"{}"}}"#,
        source_receipt_id, payload
    );
    let request = route_request_with_body(
        "POST",
        "/messages/action-requests.json",
        &request_body,
        &kernel,
    )
    .expect("pages crystallize action request route");
    assert_eq!(request.status, "200 OK");
    let action_request_receipt_id =
        json_string_field_for_test(&request.body, "action_request_receipt_id");
    let verdict_body = format!(
        r#"{{"action_verdict_id":"message_pages_crystallize_verdict","action_request_receipt_id":"{}","verdict":"approve","decision_note":"Create the Pages draft for review only."}}"#,
        action_request_receipt_id
    );
    let verdict = route_request_with_body(
        "POST",
        "/messages/action-verdicts.json",
        &verdict_body,
        &kernel,
    )
    .expect("pages crystallize action verdict route");
    for expected in [
        "\"gate_state\":\"GOVERNED_ACTION_APPLIED_EXECUTION_BLOCKED\"",
        "\"applied_receipt_kind\":\"pages.approval.requested\"",
        "\"human_approval_granted\":true",
        "\"execution_allowed\":false",
        "\"production_write_allowed\":false",
    ] {
        assert!(
            verdict.body.contains(expected),
            "{expected}: {}",
            verdict.body
        );
    }
    let approval_receipt_id = json_string_field_for_test(&verdict.body, "applied_receipt_id");
    assert!(
        !approval_receipt_id.is_empty(),
        "pages approval request receipt should be linked: {}",
        verdict.body
    );

    let drafts = route_request("GET", "/pages/edit-drafts/projection.json", &kernel)
        .expect("pages draft projection");
    for expected in [
        "\"draft_id\":\"message_page_draft_crystallize_test\"",
        "\"document_id\":\"page_message_crystallize_test\"",
        "\"title\":\"Message Crystallize Test\"",
        "\"approval_rail_allowed\":false",
        "\"production_publish_allowed\":false",
        "\"production_write_allowed\":false",
    ] {
        assert!(
            drafts.body.contains(expected),
            "{expected}: {}",
            drafts.body
        );
    }
    let approvals = route_request("GET", "/pages/approval-requests/projection.json", &kernel)
        .expect("pages approval request projection");
    for expected in [
        "\"approval_request_id\":\"message_pages_approval_crystallize_test\"".to_string(),
        "\"document_id\":\"page_message_crystallize_test\"".to_string(),
        "\"draft_id\":\"message_page_draft_crystallize_test\"".to_string(),
        "\"decision_state\":\"PENDING_DECISION\"".to_string(),
        "\"production_publish_allowed\":false".to_string(),
        "\"production_write_allowed\":false".to_string(),
        format!("\"approval_request_receipt_id\":\"{approval_receipt_id}\""),
    ] {
        assert!(
            approvals.body.contains(&expected),
            "{expected}: {}",
            approvals.body
        );
    }
    let verdicts = route_request("GET", "/messages/action-verdicts/projection.json", &kernel)
        .expect("message verdict projection");
    for expected in [
        "\"gate_state\":\"GOVERNED_ACTION_APPLIED_EXECUTION_BLOCKED\"".to_string(),
        "\"applied_receipt_kind\":\"pages.approval.requested\"".to_string(),
        format!("\"applied_receipt_id\":\"{approval_receipt_id}\""),
    ] {
        assert!(
            verdicts.body.contains(&expected),
            "{expected}: {}",
            verdicts.body
        );
    }
}

#[test]
fn message_action_rejection_can_record_pages_approval_decision_without_publish() {
    let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
    let draft_receipt_id = seed_pages_edit_draft_for_test(
        &kernel,
        "local_tenant",
        "human:local_user",
        "page_message_decision_test",
        "draft_message_decision_test",
        (
            "Message decision test",
            "world_model://pages/page_message_decision_test/body/draft/v1",
            "rev_message_decision_test",
        ),
    );
    let approval_body = format!(
        r#"{{"approval_request_id":"message_pages_decision_request","source_edit_draft_receipt_id":"{draft_receipt_id}","document_id":"page_message_decision_test","draft_id":"draft_message_decision_test","requested_visibility":"tenant_review"}}"#
    );
    let approval_request = route_request_with_body(
        "POST",
        "/pages/approval-requests.json",
        &approval_body,
        &kernel,
    )
    .expect("pages approval request route");
    assert_eq!(approval_request.status, "200 OK");
    let approval_request_receipt_id =
        json_string_field_for_test(&approval_request.body, "approval_request_receipt_id");
    let payload = format!(
        r#"{{\"kind\":\"pages_approval_decision\",\"approval_request_receipt_id\":\"{}\",\"approval_decision_id\":\"message_pages_decision_reject\",\"decision_note\":\"Needs more evidence before approval.\"}}"#,
        approval_request_receipt_id
    );
    let request_body = format!(
        r#"{{"action_request_id":"message_pages_decision_action","actor_id":"agent:assistant","thread_id":"thread_pages","channel_id":"strategy","source_receipt_id":"{}","title":"Decide the Pages review","summary":"Pages needs a human approval decision from Message.","proposed_action":"Record the Pages review outcome without publishing.","action_payload":"{}"}}"#,
        approval_request_receipt_id, payload
    );
    let request = route_request_with_body(
        "POST",
        "/messages/action-requests.json",
        &request_body,
        &kernel,
    )
    .expect("pages decision action request route");
    assert_eq!(request.status, "200 OK");
    let action_request_receipt_id =
        json_string_field_for_test(&request.body, "action_request_receipt_id");
    let verdict_body = format!(
        r#"{{"action_verdict_id":"message_pages_decision_verdict","action_request_receipt_id":"{}","verdict":"reject","decision_note":"Reject from Message until the evidence is stronger."}}"#,
        action_request_receipt_id
    );
    let verdict = route_request_with_body(
        "POST",
        "/messages/action-verdicts.json",
        &verdict_body,
        &kernel,
    )
    .expect("pages decision action verdict route");
    for expected in [
        "\"gate_state\":\"GOVERNED_ACTION_APPLIED_EXECUTION_BLOCKED\"",
        "\"applied_receipt_kind\":\"pages.approval.decision.recorded\"",
        "\"human_approval_granted\":false",
        "\"execution_allowed\":false",
        "\"production_write_allowed\":false",
    ] {
        assert!(
            verdict.body.contains(expected),
            "{expected}: {}",
            verdict.body
        );
    }
    let decision_receipt_id = json_string_field_for_test(&verdict.body, "applied_receipt_id");
    assert!(
        !decision_receipt_id.is_empty(),
        "pages decision receipt should be linked: {}",
        verdict.body
    );

    let decisions = route_request("GET", "/pages/approval-decisions/projection.json", &kernel)
        .expect("pages approval decision projection");
    for expected in [
        "\"approval_decision_id\":\"message_pages_decision_reject\"".to_string(),
        "\"decision_outcome\":\"rejected\"".to_string(),
        "\"human_approval_granted\":false".to_string(),
        "\"production_publish_allowed\":false".to_string(),
        "\"production_write_allowed\":false".to_string(),
        format!("\"approval_decision_receipt_id\":\"{decision_receipt_id}\""),
    ] {
        assert!(
            decisions.body.contains(&expected),
            "{expected}: {}",
            decisions.body
        );
    }
    let verdicts = route_request("GET", "/messages/action-verdicts/projection.json", &kernel)
        .expect("message verdict projection");
    for expected in [
        "\"gate_state\":\"GOVERNED_ACTION_APPLIED_EXECUTION_BLOCKED\"".to_string(),
        "\"applied_receipt_kind\":\"pages.approval.decision.recorded\"".to_string(),
        format!("\"applied_receipt_id\":\"{decision_receipt_id}\""),
    ] {
        assert!(
            verdicts.body.contains(&expected),
            "{expected}: {}",
            verdicts.body
        );
    }
}

#[test]
fn pages_publication_post_route_requires_and_consumes_exact_approval() {
    let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
    let refused = route_request_with_body(
        "POST",
        "/pages/publications.json",
        r#"{"document_id":"page_post_test","page_type":"spec"}"#,
        &kernel,
    )
    .expect("unapproved publication refusal");
    assert!(refused.body.contains("\"status\":\"REFUSED\""));
    assert!(refused.body.contains("required_approved_draft_content"));

    let draft_receipt_id = seed_pages_edit_draft_for_test(
        &kernel,
        "local_tenant",
        "human:local_user",
        "page_post_test",
        "draft_post_test",
        (
            "Page Post Test",
            "world_model://pages/page_post_test/body/approved",
            "rev_post_test",
        ),
    );
    let request_receipt_id = request_pages_approval_for_test(
        &kernel,
        "page_post_test",
        "draft_post_test",
        &draft_receipt_id,
        "approval_post_test",
    );
    let decision_receipt_id = decide_pages_approval_for_test(
        &kernel,
        &request_receipt_id,
        "decision_post_test",
        "approved",
    );
    let body = format!(
        r#"{{"document_id":"page_post_test","approval_decision_receipt_id":"{decision_receipt_id}","title":"Injected title","body_ref":"world_model://attacker/substitute","revision_id":"rev_injected","page_type":"spec"}}"#
    );
    let response = route_request_with_body("POST", "/pages/publications.json", &body, &kernel)
        .expect("approved Pages publication");
    for expected in [
        "\"status\":\"PAGE_PUBLISHED_EDITOR_BLOCKED\"",
        "\"title\":\"Page Post Test\"",
        "\"body_ref\":\"world_model://pages/page_post_test/body/approved\"",
        "\"revision_id\":\"rev_post_test\"",
        "\"approval_binding\":\"approved_draft_content_bound\"",
        "\"human_approval_granted\":true",
        "\"page_type\":\"spec\"",
    ] {
        assert!(
            response.body.contains(expected),
            "{expected}: {}",
            response.body
        );
    }
    assert!(!response.body.contains("Injected title"));
    assert!(!response.body.contains("attacker/substitute"));

    let replay = route_request_with_body("POST", "/pages/publications.json", &body, &kernel)
        .expect("approval replay refusal");
    assert!(replay.body.contains("\"status\":\"REFUSED\""));
    assert!(replay.body.contains("already published"));

    let projection = route_request("GET", "/pages/publications/projection.json", &kernel)
        .expect("pages publication projection route");
    for expected in [
        "\"name\":\"mdx-pages-publication-local-projection\"",
        "\"document_id\":\"page_post_test\"",
        "\"title\":\"Page Post Test\"",
        "\"approval_binding\":\"approved_draft_content_bound\"",
        "\"human_approval_granted\":true",
    ] {
        assert!(
            projection.body.contains(expected),
            "{expected}: {}",
            projection.body
        );
    }
}

#[test]
fn pages_publication_refuses_approval_after_the_draft_changes() {
    let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
    let first_draft = seed_pages_edit_draft_for_test(
        &kernel,
        "local_tenant",
        "human:local_user",
        "page_changed_after_review",
        "draft_changed_after_review_v1",
        (
            "Reviewed title",
            "world_model://pages/page_changed_after_review/body/v1",
            "rev_changed_v1",
        ),
    );
    let request = request_pages_approval_for_test(
        &kernel,
        "page_changed_after_review",
        "draft_changed_after_review_v1",
        &first_draft,
        "approval_changed_after_review_v1",
    );
    let decision = decide_pages_approval_for_test(
        &kernel,
        &request,
        "decision_changed_after_review_v1",
        "approved",
    );
    seed_pages_edit_draft_for_test(
        &kernel,
        "local_tenant",
        "human:local_user",
        "page_changed_after_review",
        "draft_changed_after_review_v2",
        (
            "Edited title",
            "world_model://pages/page_changed_after_review/body/v2",
            "rev_changed_v2",
        ),
    );
    let body = format!(
        r#"{{"document_id":"page_changed_after_review","approval_decision_receipt_id":"{decision}","page_type":"knowledge"}}"#
    );
    let response = route_request_with_body("POST", "/pages/publications.json", &body, &kernel)
        .expect("superseded approval refusal");
    assert!(response.body.contains("\"status\":\"REFUSED\""));
    assert!(response.body.contains("no longer covers the latest draft"));
    let kernel = kernel.read().expect("kernel lock");
    assert!(!kernel.ledger().entries().iter().any(|receipt| {
        receipt.kind == "pages.document.published"
            && receipt
                .payload
                .get("approval_decision_receipt_id")
                .map(String::as_str)
                == Some(decision.as_str())
    }));
}

#[test]
fn pages_publication_history_reads_only_the_named_recorded_body() {
    let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
    let body_ref = crate::pages_body_store::store_draft_body(
        "draft_history_read_test",
        "The exact historical words.",
    )
    .expect("historical body stored");
    let draft = seed_pages_edit_draft_for_test(
        &kernel,
        "local_tenant",
        "human:local_user",
        "page_history_read_test",
        "draft_history_read_test",
        ("Historical title", &body_ref, "rev_history_read_test"),
    );
    let request = request_pages_approval_for_test(
        &kernel,
        "page_history_read_test",
        "draft_history_read_test",
        &draft,
        "approval_history_read_test",
    );
    let decision =
        decide_pages_approval_for_test(&kernel, &request, "decision_history_read_test", "approved");
    let published = route_request_with_body(
        "POST",
        "/pages/publications.json",
        &format!(
            r#"{{"document_id":"page_history_read_test","approval_decision_receipt_id":"{decision}","page_type":"knowledge"}}"#
        ),
        &kernel,
    )
    .expect("publish historical body");
    let publication_receipt_id =
        json_string_field_for_test(&published.body, "publication_receipt_id");
    let historical = route_request(
        "GET",
        &format!("/pages/publications/{publication_receipt_id}/body.json"),
        &kernel,
    )
    .expect("historical body route");
    for expected in [
        "\"status\":\"OK\"",
        "\"title\":\"Historical title\"",
        "\"revision_id\":\"rev_history_read_test\"",
        "\"body\":\"The exact historical words.\"",
        "\"read_only\":true",
        "\"publication_allowed\":false",
    ] {
        assert!(
            historical.body.contains(expected),
            "{expected}: {}",
            historical.body
        );
    }
    let unknown = route_request(
        "GET",
        "/pages/publications/missing_receipt/body.json",
        &kernel,
    )
    .expect("unknown historical body route");
    assert!(unknown.body.contains("\"status\":\"REFUSED\""));
    let _ = std::fs::remove_file(body_ref);
}
#[test]
#[rustfmt::skip]
fn studio_run_post_route_lands_document_and_shows_in_both_projections() { let kernel = Arc::new(RwLock::new(MdxKernel::boot_local())); let response = route_request_with_body("POST", "/studio/runs.json", r#"{"run_id":"studio_run_post_test","goal":"Draft the board memo","artifact_kind":"document","document_id":"page_studio_post_test","title":"Studio Post Test","body_ref":"world_model://pages/page_studio_post_test/body/v1","revision_id":"rev_studio_post_test"}"#, &kernel).expect("studio run post route"); assert_eq!(response.status, "200 OK"); assert_eq!(response.content_type, JSON_CONTENT_TYPE); for expected in ["\"name\":\"mdx-studio-run-local-post\"", "\"status\":\"STUDIO_DOCUMENT_RUN_LANDED\"", "\"auth_session_status\":\"ACCEPTED_LOCAL_STUB\"", "\"artifact_kind\":\"document\"", "\"run_id\":\"studio_run_post_test\"", "\"kind_selection_mode\":\"propose_and_confirm\"", "\"standalone_document_store_allowed\":false", "\"live_co_edit_allowed\":false", "\"production_write_allowed\":false"] { assert!(response.body.contains(expected), "{expected}"); } let projection = route_request("GET", "/studio/runs/projection.json", &kernel).expect("studio projection route"); assert_eq!(projection.status, "200 OK"); for expected in ["\"name\":\"mdx-studio-run-local-projection\"", "\"run_count\":1", "\"run_id\":\"studio_run_post_test\"", "\"document_id\":\"page_studio_post_test\"", "\"title\":\"Studio Post Test\"", "\"origin\":\"studio\""] { assert!(projection.body.contains(expected), "{expected}"); } let pages = route_request("GET", "/pages/publications/projection.json", &kernel).expect("pages projection sees the studio landing"); for expected in ["\"publication_count\":1", "\"document_id\":\"page_studio_post_test\"", "\"title\":\"Studio Post Test\""] { assert!(pages.body.contains(expected), "one body of truth, studio landing in pages: {expected}"); } }
#[test]
#[rustfmt::skip]
fn studio_run_post_route_refuses_code_kind_into_forge() { let kernel = Arc::new(RwLock::new(MdxKernel::boot_local())); let response = route_request_with_body("POST", "/studio/runs.json", r#"{"run_id":"studio_run_code","goal":"build a thing","artifact_kind":"code","document_id":"x_code","title":"Code","revision_id":"rev_code"}"#, &kernel).expect("studio code refusal route"); assert_eq!(response.status, "200 OK"); for expected in ["\"status\":\"REFUSED\"", "code work opens in Forge", "\"open_in\":\"forge\""] { assert!(response.body.contains(expected), "{expected}"); } }
#[test]
#[rustfmt::skip]
fn studio_steering_routes_drive_a_run_through_pause_steer_resume_land() { let kernel = Arc::new(RwLock::new(MdxKernel::boot_local())); let opened = route_request_with_body("POST", "/studio/runs/run_http_1/open.json", r#"{"goal":"Draft the board memo","artifact_kind":"document"}"#, &kernel).expect("open"); assert_eq!(opened.status, "200 OK"); for e in ["\"name\":\"mdx-studio-run-open-local-post\"", "\"status\":\"STUDIO_RUN_DRAFTING\"", "\"state\":\"drafting\""] { assert!(opened.body.contains(e), "{e}"); } let paused = route_request_with_body("POST", "/studio/runs/run_http_1/pause.json", r#"{"idempotency_key":"k_pause"}"#, &kernel).expect("pause"); for e in ["\"status\":\"STUDIO_RUN_PAUSED\"", "\"state\":\"paused\"", "\"refused\":false"] { assert!(paused.body.contains(e), "{e}"); } let steered = route_request_with_body("POST", "/studio/runs/run_http_1/steering.json", r#"{"idempotency_key":"k_steer","instruction":"Lead with the risk","reason":"the board reads risk first"}"#, &kernel).expect("steer"); assert!(steered.body.contains("\"status\":\"STUDIO_STEERING_RECORDED\""), "steering recorded"); let resumed = route_request_with_body("POST", "/studio/runs/run_http_1/resume.json", r#"{"idempotency_key":"k_resume"}"#, &kernel).expect("resume"); for e in ["\"status\":\"STUDIO_RUN_RESUMED\"", "\"state\":\"drafting\""] { assert!(resumed.body.contains(e), "{e}"); } let landed = route_request_with_body("POST", "/studio/runs/run_http_1/land.json", r#"{"idempotency_key":"k_land","document_id":"page_http_memo","title":"HTTP Memo","body_text":"the memo body","revision_id":"rev_http_1"}"#, &kernel).expect("land"); for e in ["\"status\":\"STUDIO_DOCUMENT_RUN_LANDED\"", "\"state\":\"landed\""] { assert!(landed.body.contains(e), "{e}"); } let projection = route_request("GET", "/studio/runs/projection.json", &kernel).expect("projection"); for e in ["\"run_id\":\"run_http_1\"", "\"state\":\"landed\"", "\"document_id\":\"page_http_memo\"", "\"allowed_controls\":[]"] { assert!(projection.body.contains(e), "{e}"); } let pages = route_request("GET", "/pages/publications/projection.json", &kernel).expect("pages projection"); assert!(pages.body.contains("\"publication_count\":1"), "one body of truth"); assert!(pages.body.contains("page_http_memo"), "studio landing in pages"); }
#[test]
#[rustfmt::skip]
fn studio_steering_route_refuses_after_terminal_as_evidence() { let kernel = Arc::new(RwLock::new(MdxKernel::boot_local())); route_request_with_body("POST", "/studio/runs/run_http_2/open.json", r#"{"artifact_kind":"document"}"#, &kernel).expect("open"); route_request_with_body("POST", "/studio/runs/run_http_2/land.json", r#"{"idempotency_key":"l","document_id":"page_http_two","title":"T","body_text":"b","revision_id":"r"}"#, &kernel).expect("land"); let late = route_request_with_body("POST", "/studio/runs/run_http_2/pause.json", r#"{"idempotency_key":"late"}"#, &kernel).expect("late pause"); assert_eq!(late.status, "200 OK"); for e in ["\"status\":\"STUDIO_STEERING_REFUSED\"", "\"refused\":true", "run already landed"] { assert!(late.body.contains(e), "{e}"); } }
#[test]
#[rustfmt::skip]
fn a_private_channel_and_its_messages_hide_from_a_non_member_reader() { let kernel = Arc::new(RwLock::new(MdxKernel::boot_local())); route_request_with_body("POST", "/messages/channels.json", r#"{"channel_id":"warroom","name":"War room","visibility":"private","actor_id":"agent:scout"}"#, &kernel).expect("create private channel as agent"); route_request_with_body("POST", "/messages/thread-messages.json", r#"{"body":"eyes only","channel_id":"warroom","message_id":"m_secret_1","actor_id":"agent:scout"}"#, &kernel).expect("post into private channel"); let channels = route_request("GET", "/messages/channels/projection.json", &kernel).expect("channels projection"); assert!(channels.body.contains("\"hidden_private_count\":1"), "one private channel hidden"); assert!(!channels.body.contains("warroom"), "private channel must not appear to a non-member"); let timeline = route_request("GET", "/messages/thread-messages/projection.json", &kernel).expect("timeline projection"); assert!(!timeline.body.contains("eyes only"), "private channel messages must not leak"); let added = route_request_with_body("POST", "/messages/channel-members.json", r#"{"channel_id":"warroom","member_actor_id":"human:local_user","actor_id":"agent:scout"}"#, &kernel).expect("add local reader"); assert!(added.body.contains("\"status\":\"ADDED\"")); let after = route_request("GET", "/messages/channels/projection.json", &kernel).expect("channels after join"); assert!(after.body.contains("warroom"), "now a member, the channel appears"); assert!(after.body.contains("\"hidden_private_count\":0"), "nothing hidden now"); let timeline2 = route_request("GET", "/messages/thread-messages/projection.json", &kernel).expect("timeline after join"); assert!(timeline2.body.contains("eyes only"), "now a member, messages are readable"); }
#[test]
#[rustfmt::skip]
fn message_channel_member_routes_add_remove_and_fold_into_the_channel() { let kernel = Arc::new(RwLock::new(MdxKernel::boot_local())); route_request_with_body("POST", "/messages/channels.json", r#"{"channel_id":"forge","name":"Forge","actor_id":"human:md"}"#, &kernel).expect("create channel"); let added = route_request_with_body("POST", "/messages/channel-members.json", r#"{"channel_id":"forge","member_actor_id":"agent:scout","member_role":"member","actor_id":"human:md"}"#, &kernel).expect("add member route"); for expected in ["\"status\":\"ADDED\"", "\"member_actor_id\":\"agent:scout\"", "\"member_role\":\"member\""] { assert!(added.body.contains(expected), "{expected}"); } let missing = route_request_with_body("POST", "/messages/channel-members.json", r#"{"channel_id":"ghost","member_actor_id":"agent:scout","actor_id":"human:md"}"#, &kernel).expect("add to missing channel"); assert!(missing.body.contains("\"status\":\"REFUSED\""), "adding to a missing channel refused"); let projection = route_request("GET", "/messages/channels/projection.json", &kernel).expect("projection"); for expected in ["\"member_count\":2", "\"actor_id\":\"human:md\"", "\"role\":\"owner\"", "\"actor_id\":\"agent:scout\"", "\"role\":\"member\""] { assert!(projection.body.contains(expected), "{expected}"); } let removed = route_request_with_body("POST", "/messages/channel-member-removals.json", r#"{"channel_id":"forge","member_actor_id":"agent:scout","actor_id":"human:md"}"#, &kernel).expect("remove member route"); assert!(removed.body.contains("\"status\":\"REMOVED\""), "remove accepted"); let after = route_request("GET", "/messages/channels/projection.json", &kernel).expect("projection after remove"); assert!(after.body.contains("\"member_count\":1"), "member dropped"); assert!(!after.body.contains("agent:scout"), "removed member gone"); }
#[test]
#[rustfmt::skip]
fn message_bridge_queues_outbound_and_rolls_up_inbound_connector_items() { let kernel = Arc::new(RwLock::new(MdxKernel::boot_local())); route_request_with_body("POST", "/connectors/items.json", r#"{"source_id":"slack:acme","source_kind":"slack","external_ref":"https://acme.slack.com/p1","title":"From #general: ship it","summary":"a Slack message","actor_id":"human:md"}"#, &kernel).expect("ingest slack inbound"); let outbound = route_request_with_body("POST", "/messages/bridge-posts.json", r#"{"target_kind":"slack","target_channel":"slack:C123","body":"posting from MDx","actor_id":"human:md"}"#, &kernel).expect("queue outbound"); for expected in ["\"status\":\"QUEUED\"", "\"target_kind\":\"slack\"", "\"production_delivery_allowed\":false"] { assert!(outbound.body.contains(expected), "{expected}"); } let bad = route_request_with_body("POST", "/messages/bridge-posts.json", r#"{"target_kind":"carrier_pigeon","target_channel":"x","body":"x","actor_id":"human:md"}"#, &kernel).expect("bad target"); assert!(bad.body.contains("\"status\":\"REFUSED\""), "unknown bridge target refused"); let projection = route_request("GET", "/messages/bridge/projection.json", &kernel).expect("bridge projection"); for expected in ["\"name\":\"mdx-message-bridge-local-projection\"", "\"system\":\"slack\"", "\"inbound_count\":1", "\"outbound_queued_count\":1", "\"inbound_total\":1", "\"outbound_total\":1", "posting from MDx", "\"production_delivery_allowed\":false", "live transport is gated"] { assert!(projection.body.contains(expected), "{expected}"); } }
#[test]
#[rustfmt::skip]
fn message_presence_projection_rosters_recent_actors() { let kernel = Arc::new(RwLock::new(MdxKernel::boot_local())); { let mut k = kernel.write().unwrap(); k.run_evals_runner_agent().expect("seed receipts"); } route_request_with_body("POST", "/messages/thread-messages.json", r#"{"body":"hello team","channel_id":"local-ops","message_id":"m_presence_1","actor_id":"human:md"}"#, &kernel).expect("post a message"); let presence = route_request("GET", "/messages/presence/projection.json", &kernel).expect("presence projection"); assert_eq!(presence.status, "200 OK"); for expected in ["\"name\":\"mdx-message-presence-local-projection\"", "\"active_now_count\":", "\"roster\":[", "\"actor_id\":\"human:md\"", "\"active\":true", "\"last_action\":\"said something\"", "\"production_write_allowed\":false"] { assert!(presence.body.contains(expected), "{expected}"); } }
#[test]
#[rustfmt::skip]
fn message_activity_projection_routes_receipts_into_per_area_channels() { let kernel = Arc::new(RwLock::new(MdxKernel::boot_local())); { let mut k = kernel.write().unwrap(); k.run_evals_runner_agent().expect("seed an eval receipt"); } let posted = route_request_with_body("POST", "/messages/thread-messages.json", r#"{"body":"nice work on that run","channel_id":"forge","message_id":"msg_forge_1","actor_id":"human:md"}"#, &kernel).expect("post into forge channel"); assert_eq!(posted.status, "200 OK"); let projection = route_request("GET", "/messages/activity/projection.json", &kernel).expect("activity projection route"); assert_eq!(projection.status, "200 OK"); for expected in ["\"name\":\"mdx-message-activity-local-projection\"", "\"channel_id\":\"forge\"", "\"label\":\"Forge\"", "\"channel_id\":\"deploys\"", "\"channel_id\":\"evals\"", "\"feed_kind\":\"message\"", "\"area\":\"forge\"", "nice work on that run", "\"production_write_allowed\":false"] { assert!(projection.body.contains(expected), "{expected}"); } }

#[test]
fn message_activity_projection_supports_limit_cursor_and_run_links() {
    let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
    {
        let mut k = kernel.write().unwrap();
        k.record_forge_run_event(mdx_core::ForgeRunEvent {
            tenant_id: "local_tenant",
            actor_id: "agent:forge",
            run_id: "forge_run_activity_link",
            event: "run_started",
            work_item_id: "wi_activity_link",
            detail: "accepted activity-linked run",
            turn: 0,
            input_tokens: 0,
            output_tokens: 0,
        })
        .expect("run event");
        k.record_forge_run_event(mdx_core::ForgeRunEvent {
            tenant_id: "local_tenant",
            actor_id: "agent:forge",
            run_id: "forge_run_activity_link",
            event: "check_passed",
            work_item_id: "wi_activity_link",
            detail: "run_command cargo test exit=0",
            turn: 1,
            input_tokens: 0,
            output_tokens: 0,
        })
        .expect("check event");
    }
    let first = route_request("GET", "/messages/activity/projection.json?limit=1", &kernel)
        .expect("activity projection route");
    assert_eq!(first.status, "200 OK");
    let parsed: serde_json::Value = serde_json::from_str(&first.body).expect("json");
    assert_eq!(parsed["item_count"].as_u64(), Some(1));
    assert_eq!(parsed["total_item_count"].as_u64(), Some(2));
    assert_eq!(parsed["has_more"].as_bool(), Some(true));
    assert_eq!(
        parsed["items"][0]["run_id"].as_str(),
        Some("forge_run_activity_link")
    );
    assert_eq!(
        parsed["items"][0]["href"].as_str(),
        Some("/forge/runs?run_id=forge_run_activity_link")
    );
    let next = parsed["next_cursor"].as_u64().unwrap_or(0);
    let second = route_request(
        "GET",
        &format!("/messages/activity/projection.json?limit=1&cursor={next}"),
        &kernel,
    )
    .expect("activity projection route");
    let parsed_second: serde_json::Value = serde_json::from_str(&second.body).expect("json");
    assert_eq!(parsed_second["item_count"].as_u64(), Some(1));
    assert_eq!(parsed_second["has_more"].as_bool(), Some(false));
}
#[test]
#[rustfmt::skip]
fn message_channel_routes_create_edit_and_fold_into_a_projection() { let kernel = Arc::new(RwLock::new(MdxKernel::boot_local())); let created = route_request_with_body("POST", "/messages/channels.json", r#"{"channel_id":"forge","name":"Forge","description":"Where the factory talks","actor_id":"human:md"}"#, &kernel).expect("channel create route"); assert_eq!(created.status, "200 OK"); for expected in ["\"status\":\"CREATED\"", "\"channel_id\":\"forge\"", "\"channel_kind\":\"team\"", "\"visibility\":\"public\"", "\"production_write_allowed\":false"] { assert!(created.body.contains(expected), "{expected}"); } let bad = route_request_with_body("POST", "/messages/channels.json", r#"{"channel_id":"Forge Ops","name":"x","actor_id":"human:md"}"#, &kernel).expect("bad slug route"); assert!(bad.body.contains("\"status\":\"REFUSED\""), "bad slug refused"); let renamed = route_request_with_body("POST", "/messages/channel-updates.json", r#"{"channel_id":"forge","topic":"ship daily","status":"archived","actor_id":"human:md"}"#, &kernel).expect("channel update route"); assert!(renamed.body.contains("\"status\":\"UPDATED\""), "update accepted"); let missing = route_request_with_body("POST", "/messages/channel-updates.json", r#"{"channel_id":"ghost","name":"Nope","actor_id":"human:md"}"#, &kernel).expect("update missing route"); assert!(missing.body.contains("\"status\":\"REFUSED\""), "editing a missing channel refused"); let projection = route_request("GET", "/messages/channels/projection.json", &kernel).expect("channels projection route"); assert_eq!(projection.status, "200 OK"); for expected in ["\"name\":\"mdx-message-channels-local-projection\"", "\"channel_count\":1", "\"archived_count\":1", "\"channel_id\":\"forge\"", "\"topic\":\"ship daily\"", "\"status\":\"archived\""] { assert!(projection.body.contains(expected), "{expected}"); } }
#[test]
#[rustfmt::skip]
fn connector_item_post_route_ingests_external_and_refuses_sensitive_on_frontier() { let kernel = Arc::new(RwLock::new(MdxKernel::boot_local())); let ingested = route_request_with_body("POST", "/connectors/items.json", r#"{"source_id":"github:mdx-stack","source_kind":"github","external_ref":"https://github.com/mdx-stack/doc","title":"Architecture note","summary":"a take","grade":"signal","scope":"tenant"}"#, &kernel).expect("connector ingest post route"); assert_eq!(ingested.status, "200 OK"); assert_eq!(ingested.content_type, JSON_CONTENT_TYPE); for expected in ["\"status\":\"INGESTED\"", "\"origin\":\"external\"", "\"source_id\":\"github:mdx-stack\"", "\"data_sensitivity\":\"normal\"", "\"handling\":\"frontier\"", "\"grade\":\"signal\"", "\"scope\":\"tenant\"", "\"production_write_allowed\":false"] { assert!(ingested.body.contains(expected), "{expected}"); } let personal = route_request_with_body("POST", "/connectors/items.json", r#"{"source_id":"notion:md-private","source_kind":"notion","external_ref":"https://notion/x","title":"My private notes","scope":"personal"}"#, &kernel).expect("personal ingest route"); assert!(personal.body.contains("\"scope\":\"personal\""), "personal ingested"); let refused = route_request_with_body("POST", "/connectors/items.json", r#"{"source_id":"confluence:eng","source_kind":"confluence","external_ref":"https://eng/secret","title":"Regulated","data_sensitivity":"sensitive","handling":"frontier"}"#, &kernel).expect("connector refusal route"); assert_eq!(refused.status, "200 OK"); assert!(refused.body.contains("\"status\":\"REFUSED\""), "sensitive on frontier refused"); let projection = route_request("GET", "/connectors/projection.json", &kernel).expect("connector projection route"); assert_eq!(projection.status, "200 OK"); for expected in ["\"name\":\"mdx-connector-local-projection\"", "\"feed_scope\":\"tenant\"", "\"item_count\":1", "\"source_count\":1", "\"personal_item_count_withheld\":1", "\"source_id\":\"github:mdx-stack\""] { assert!(projection.body.contains(expected), "{expected}"); } assert!(!projection.body.contains("notion:md-private"), "personal source must not leak into the shared feed"); assert!(!projection.body.contains("My private notes"), "personal content must not leak"); }
#[test]
fn pages_approval_request_post_route_records_exact_draft_and_projection() {
    let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
    let missing = route_request_with_body(
        "POST",
        "/pages/approval-requests.json",
        r#"{"approval_request_id":"missing_source","document_id":"page_local_operator_note","draft_id":"draft_for_approval_request","requested_visibility":"tenant_review"}"#,
        &kernel,
    )
    .expect("missing source refusal");
    assert!(missing.body.contains("\"status\":\"REFUSED\""));
    let draft_receipt_id = seed_pages_edit_draft_for_test(
        &kernel,
        "local_tenant",
        "human:local_user",
        "page_local_operator_note",
        "draft_for_approval_request",
        (
            "Local operator note",
            "world_model://pages/page_local_operator_note/body/draft/v1",
            "rev_local_approval_test",
        ),
    );
    let body = format!(
        r#"{{"approval_request_id":"pages_approval_post_test","source_edit_draft_receipt_id":"{draft_receipt_id}","document_id":"page_local_operator_note","draft_id":"draft_for_approval_request","requested_visibility":"tenant_review"}}"#
    );
    let response = route_request_with_body("POST", "/pages/approval-requests.json", &body, &kernel)
        .expect("pages approval request post route");
    for expected in [
        "\"status\":\"PAGE_APPROVAL_REQUEST_RECORDED_PUBLICATION_BLOCKED\"",
        "\"approval_request_id\":\"pages_approval_post_test\"",
        "\"source_edit_draft_receipt_id\":",
        "\"production_publish_allowed\":false",
    ] {
        assert!(
            response.body.contains(expected),
            "{expected}: {}",
            response.body
        );
    }
    let projection = route_request("GET", "/pages/approval-requests/projection.json", &kernel)
        .expect("pages approval request projection route");
    for expected in [
        "\"approval_request_count\":1",
        "\"approval_request_id\":\"pages_approval_post_test\"",
        "\"decision_state\":\"PENDING_DECISION\"",
    ] {
        assert!(
            projection.body.contains(expected),
            "{expected}: {}",
            projection.body
        );
    }
}
#[test]
fn pages_approval_decision_routes_record_approve_reject_and_projection() {
    let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
    let empty_projection =
        route_request("GET", "/pages/approval-decisions/projection.json", &kernel)
            .expect("empty pages decision projection");
    for expected in [
        "\"receipt_kind\":\"pages.approval.decision.recorded\"",
        "\"decision_count\":0",
        "\"human_decision_recorded\":false",
    ] {
        assert!(empty_projection.body.contains(expected), "{expected}");
    }
    let approved_draft = seed_pages_edit_draft_for_test(
        &kernel,
        "local_tenant",
        "human:local_user",
        "page_decision_approve_test",
        "draft_decision_approve_test",
        (
            "Approve test",
            "world_model://pages/page_decision_approve_test/body/v1",
            "rev_decision_approve_test",
        ),
    );
    let receipt_id = request_pages_approval_for_test(
        &kernel,
        "page_decision_approve_test",
        "draft_decision_approve_test",
        &approved_draft,
        "pages_decision_request_approve_test",
    );
    let approve_body = format!(
        r#"{{"approval_decision_id":"pages_decision_approve_test","approval_request_receipt_id":"{}","decision_note":"Approve local review."}}"#,
        receipt_id
    );
    let approve = route_request_with_body(
        "POST",
        "/pages/approval-decisions/approve.json",
        &approve_body,
        &kernel,
    )
    .expect("pages approve decision");
    assert_eq!(approve.status, "200 OK");
    for expected in [
        "\"name\":\"mdx-pages-approval-decision-local-post\"",
        "\"status\":\"PAGE_APPROVAL_APPROVED_PUBLICATION_BLOCKED\"",
        "\"decision_outcome\":\"approved\"",
        "\"human_decision_recorded\":true",
        "\"human_approval_granted\":true",
        "\"reviewer_independence\":\"self_review_permitted\"",
        "\"self_review_permitted\":true",
        "\"production_publish_allowed\":false",
        "\"production_write_allowed\":false",
    ] {
        assert!(approve.body.contains(expected), "{expected}");
    }
    let duplicate = route_request_with_body(
        "POST",
        "/pages/approval-decisions/reject.json",
        &format!(
            r#"{{"approval_decision_id":"pages_decision_duplicate_test","approval_request_receipt_id":"{receipt_id}","decision_note":"Duplicate review."}}"#
        ),
        &kernel,
    )
    .expect("duplicate decision refusal");
    assert!(duplicate.body.contains("\"status\":\"REFUSED\""));
    assert!(duplicate.body.contains("already has a decision"));

    let rejected_draft = seed_pages_edit_draft_for_test(
        &kernel,
        "local_tenant",
        "human:local_user",
        "page_decision_reject_test",
        "draft_decision_reject_test",
        (
            "Reject test",
            "world_model://pages/page_decision_reject_test/body/v1",
            "rev_decision_reject_test",
        ),
    );
    let rejected_request = request_pages_approval_for_test(
        &kernel,
        "page_decision_reject_test",
        "draft_decision_reject_test",
        &rejected_draft,
        "pages_decision_request_reject_test",
    );
    let reject_body = format!(
        r#"{{"approval_decision_id":"pages_decision_reject_test","approval_request_receipt_id":"{rejected_request}","decision_note":"Reject local review."}}"#
    );
    let reject = route_request_with_body(
        "POST",
        "/pages/approval-decisions/reject.json",
        &reject_body,
        &kernel,
    )
    .expect("pages reject decision");
    assert_eq!(reject.status, "200 OK");
    for expected in [
        "\"status\":\"PAGE_APPROVAL_REJECTED_PUBLICATION_BLOCKED\"",
        "\"decision_outcome\":\"rejected\"",
        "\"human_approval_granted\":false",
        "\"production_publish_allowed\":false",
        "\"production_write_allowed\":false",
    ] {
        assert!(reject.body.contains(expected), "{expected}");
    }
    let decisions = route_request("GET", "/pages/approval-decisions/projection.json", &kernel)
        .expect("pages decision projection");
    assert_eq!(decisions.status, "200 OK");
    for expected in [
        "\"name\":\"mdx-pages-approval-decision-local-projection\"",
        "\"decision_count\":2",
        "\"approval_decision_id\":\"pages_decision_approve_test\"",
        "\"approval_decision_id\":\"pages_decision_reject_test\"",
    ] {
        assert!(decisions.body.contains(expected), "{expected}");
    }
    let requests = route_request("GET", "/pages/approval-requests/projection.json", &kernel)
        .expect("pages request projection");
    assert!(
        requests
            .body
            .contains("\"decision_state\":\"APPROVED_PUBLICATION_BLOCKED\"")
    );
    assert!(
        requests
            .body
            .contains("\"decision_state\":\"REJECTED_PUBLICATION_BLOCKED\"")
    );
    assert!(requests.body.contains("\"human_decision_recorded\":true"));
}
#[test]
fn runtime_projection_routes_are_read_only_and_honest() {
    let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

    // Every runtime projection route is a read: it grants nothing, writes
    // nothing, executes nothing, and opens no deployment or production door.
    let read_only_routes = [
        "/runtime/queue-projection.json",
        "/runtime/metrics.json",
        "/runtime/operator-packet.json",
        "/autonomy/envelopes/projection.json",
        "/runtime/load-proof-summary.json",
        "/forge/runtime-projection.json",
    ];
    for path in read_only_routes {
        let response = route_request("GET", path, &kernel).expect("runtime projection route");
        assert_eq!(response.status, "200 OK", "{path}");
        assert_eq!(response.content_type, JSON_CONTENT_TYPE, "{path}");
        for expected in [
            "\"read_only\": true",
            "\"writes_allowed\": false",
            "\"execution_allowed\": false",
            "\"deployment_authority_granted\": false",
            "\"production_write_allowed\": false",
        ] {
            assert!(response.body.contains(expected), "{path}: {expected}");
        }
        // A read route never accepts a write.
        let post = route_request("POST", path, &kernel).expect("runtime projection post");
        assert_eq!(post.status, "405 Method Not Allowed", "{path}");
    }

    // The queue is empty: no job in flight.
    let queue = route_request("GET", "/runtime/queue-projection.json", &kernel).expect("queue");
    for expected in [
        "\"name\": \"mdx-runtime-queue-projection\"",
        "\"queued_job_count\": 0",
        "\"jobs\": []",
        "\"human_summary\": \"No job in flight.\"",
        "\"live_durability_observed\": false",
    ] {
        assert!(queue.body.contains(expected), "{expected}");
    }

    // Metrics are observed-local zeros alongside the real standing targets.
    let metrics = route_request("GET", "/runtime/metrics.json", &kernel).expect("metrics");
    for expected in [
        "\"name\": \"mdx-runtime-metrics\"",
        "\"measurement_source\": \"observed_local\"",
        "\"admitted\": 0",
        "\"engineers\": 1000",
        "\"peak_forge_builds\": 5000",
        "\"top_failure_reasons\": []",
    ] {
        assert!(metrics.body.contains(expected), "{expected}");
    }

    // No build at the ship door; the packet is absent and asks for no human.
    let packet =
        route_request("GET", "/runtime/operator-packet.json", &kernel).expect("operator packet");
    for expected in [
        "\"name\": \"mdx-runtime-operator-packet\"",
        "\"packet_present\": false",
        "\"needs_human\": false",
        "\"terminal_build_count\": 0",
        "\"operator_can_ratify_here\": false",
        "\"human_summary\": \"No build at the ship door yet.\"",
    ] {
        assert!(packet.body.contains(expected), "{expected}");
    }

    // No standing authorization; the safe default keeps a human on every task.
    let envelopes =
        route_request("GET", "/autonomy/envelopes/projection.json", &kernel).expect("envelopes");
    for expected in [
        "\"name\": \"mdx-autonomy-envelope-projection\"",
        "\"envelope_count\": 0",
        "\"active_authorization\": false",
        "\"envelopes\": []",
        "no standing authorization active; every task reviews with a human",
    ] {
        assert!(envelopes.body.contains(expected), "{expected}");
    }

    // The load proof has not been run in this test environment; never faked.
    let load = route_request("GET", "/runtime/load-proof-summary.json", &kernel).expect("load");
    assert!(
        load.body
            .contains("\"name\": \"mdx-runtime-load-proof-summary\"")
    );
    assert!(
        load.body
            .contains("\"source_file\": \".mdx-local/load-proof/evidence.json\"")
    );
    // Either the proof file is present (proof_run true with evidence) or absent
    // (proof_run false, evidence null). Both are honest; nothing is invented.
    assert!(
        load.body.contains("\"proof_run\": false") || load.body.contains("\"proof_run\": true"),
        "{}",
        load.body
    );
}

#[test]
fn runtime_metrics_route_surfaces_timed_forge_receipts() {
    let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
    {
        let mut kernel = kernel.write().expect("kernel lock");
        for (event, detail, duration_ms, input_tokens, output_tokens) in [
            (
                "evidence_appended",
                "phase=intake context_chars=10 standards=0 outcomes=0 active_memories=0 installed_capabilities=0",
                2,
                0,
                0,
            ),
            (
                "model_called",
                "model=qwen finish_reason=tool_calls tool_calls=1",
                30,
                100,
                20,
            ),
            (
                "tool_executed",
                "read_file crates/mdx-core/src/lib.rs",
                4,
                0,
                0,
            ),
            (
                "run_finished",
                "status=RUN_FINISHED_DONE turns=1 files_changed=0",
                50,
                0,
                0,
            ),
        ] {
            kernel
                .record_forge_run_event_with_duration(
                    mdx_core::ForgeRunEvent {
                        tenant_id: "tenant",
                        actor_id: "agent:forge",
                        run_id: "forge_run_timed_route",
                        event,
                        work_item_id: "work_1",
                        detail,
                        turn: 1,
                        input_tokens,
                        output_tokens,
                    },
                    duration_ms,
                )
                .expect("timed forge receipt");
        }
    }
    let metrics = route_request("GET", "/runtime/metrics.json", &kernel).expect("metrics");
    for expected in [
        "\"forge_phase_duration_ms\"",
        "\"forge_total_run_duration_ms\": { \"count\": 1, \"min_ms\": 50, \"mean_ms\": 50, \"max_ms\": 50 }",
        "\"forge_model_tokens\": { \"input\": 100, \"output\": 20 }",
        "\"model\":\"qwen\"",
        "\"tool\":\"read_file\"",
    ] {
        assert!(
            metrics.body.contains(expected),
            "{expected}: {}",
            metrics.body
        );
    }
    assert!(!metrics.body.contains("\"model_latency_ms\""));
    assert!(metrics.body.contains("\"cost_budget_use\""));
}

#[test]
#[rustfmt::skip]
fn local_http_routes_enforce_declared_methods() { let kernel = Arc::new(RwLock::new(MdxKernel::boot_local())); let get_loop = route_request("GET", "/run-loop/evals_runner_agent", &kernel).unwrap(); assert_eq!(get_loop.status, "405 Method Not Allowed"); assert_eq!(get_loop.body, "method not allowed\n"); let post_status = route_request("POST", "/status", &kernel).expect("wrong method"); assert_eq!(post_status.status, "405 Method Not Allowed"); for path in ["/twin/session-drafts.json", "/twin/model-gateway-provider-observations.json", "/twin/provider-call-refusals.json", "/twin/production-memory-refusals.json", "/twin/tool-execution-refusals.json", "/forge/intake-plan-requests.json", "/messages/thread-messages.json", "/forge/build-approvals.json", "/forge/workflow-plan-proofs.json", "/forge/worker-authority-requests.json", "/forge/talent-authorizations.json", "/forge/worker-credential-checks.json", "/forge/worker-spawn-preflights.json", "/forge/ci-evidence-preflights.json", "/forge/human-ratification-preflights.json", "/forge/deployment-preflights.json", "/messages/thread-messages.json", "/messages/fanout-requests.json", "/messages/presence-requests.json", "/messages/realtime-cutover-preflights.json", "/pages/publications.json", "/pages/edit-drafts.json", "/pages/approval-requests.json", "/pages/approval-decisions/approve.json", "/pages/approval-decisions/reject.json", "/pages/search-preflights.json", "/pages/revision-citation-comparisons.json", "/pages/publication-visibility-checks.json", "/pages/embedding-provider-refusals.json", "/auth/tenant-policy-preflights.json", "/v1/read-shadow-approval-requests.json", "/v1/read-shadow-approval-decisions.json", "/v1/write-mirror-approval-requests.json", "/v1/write-mirror-approval-decisions.json"] { let preflight = route_request("OPTIONS", path, &kernel).expect("governed write preflight"); assert_eq!(preflight.status, "204 No Content"); assert_eq!(preflight.body, ""); } let loop_preflight = route_request("OPTIONS", "/run-loop/evals_runner_agent", &kernel).expect("loop preflight"); assert_eq!(loop_preflight.status, "204 No Content"); }

#[test]
fn leakage_matrix_primary_surfaces_map_to_real_served_routes() {
    // The leakage matrix's direct-DB dimension is grounded in mdx-core against
    // the real RLS tables. This grounds its API dimension: each primary
    // user-data surface in the matrix corresponds to a real served route surface,
    // so the authorization decisions the matrix proves are decisions on routes the
    // kernel actually serves, not abstract ones.
    use mdx_core::{LEAKAGE_MATRIX, Surface};
    let routes = local_http_routes();
    let route_surface_for = |surface: Surface| -> &'static str {
        match surface {
            Surface::Twin => "twin_session",
            Surface::Message => "message",
            Surface::Pages => "pages",
            Surface::Forge => "forge",
            _ => "",
        }
    };
    for surface in [
        Surface::Twin,
        Surface::Message,
        Surface::Pages,
        Surface::Forge,
    ] {
        let token = route_surface_for(surface);
        assert!(
            routes.iter().any(|r| r.surface == token),
            "matrix surface {} has no served route surface {}",
            surface.as_str(),
            token
        );
        assert!(
            LEAKAGE_MATRIX.iter().any(|c| c.surface == surface),
            "served surface {} is missing from the leakage matrix",
            surface.as_str()
        );
    }
}

// Phase 8 hotfix: the production HTTP boundary. These exercise the real secured
// serving entry (route_request_secured, what handle_connection calls) in
// production mode and prove governed writes fail closed without a verified
// session, that no local stub or body-supplied identity leaks, that CORS never
// wildcards, and that local-demo is unchanged.
fn production_security() -> crate::request_security::RequestSecurity {
    crate::request_security::RequestSecurity::production_unverified()
}

#[test]
fn production_denies_unauthenticated_forge_build_request() {
    let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
    let response = route_request_secured(
        "POST",
        "/messages/thread-messages.json",
        r#"{"request_id":"hotfix","requested_change":"should be refused"}"#,
        &kernel,
        &production_security(),
    )
    .expect("route");
    assert_eq!(response.status, "401 Unauthorized");
    assert!(
        response
            .body
            .contains("production_governed_write_requires_verified_session")
    );
    // No local stub and no body-defaulted identity may leak in production.
    assert!(!response.body.contains("ACCEPTED_LOCAL_STUB"));
    assert!(!response.body.contains("local_tenant"));
    assert!(!response.body.contains("local_user"));
}

#[test]
fn production_denies_body_supplied_tenant_and_actor() {
    let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
    let response = route_request_secured(
        "POST",
        "/messages/thread-messages.json",
        r#"{"tenant_id":"attacker_tenant","actor_id":"attacker","actor_role":"owner","requested_change":"escalate"}"#,
        &kernel,
        &production_security(),
    )
    .expect("route");
    assert_eq!(response.status, "401 Unauthorized");
    // The body-supplied identity is never honored or echoed as an accepted write.
    assert!(!response.body.contains("attacker_tenant"));
    assert!(!response.body.contains("ACCEPTED_LOCAL_STUB"));
}

#[test]
fn production_denies_governed_writes_across_surfaces() {
    let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
    for path in [
        "/messages/thread-messages.json",
        "/twin/session-drafts.json",
        "/messages/thread-messages.json",
        "/pages/publications.json",
        "/studio/runs.json",
        "/auth/role-assignments.json",
    ] {
        let response = route_request_secured("POST", path, "{}", &kernel, &production_security())
            .expect("route");
        assert_eq!(
            response.status, "401 Unauthorized",
            "{path} must be refused"
        );
        assert!(
            !response.body.contains("ACCEPTED_LOCAL_STUB"),
            "{path} must not emit a local stub in production"
        );
    }
}

#[test]
fn local_demo_governed_write_still_accepts_the_fixture() {
    let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
    let response = route_request_secured(
        "POST",
        "/messages/thread-messages.json",
        r#"{"request_id":"local_ok","requested_change":"record only"}"#,
        &kernel,
        &crate::request_security::RequestSecurity::local_demo(),
    )
    .expect("route");
    assert_eq!(response.status, "200 OK");
    assert!(response.body.contains("ACCEPTED_LOCAL_STUB"));
}

#[test]
fn production_responses_never_carry_wildcard_cors() {
    // Production never emits a wildcard origin regardless of any ambient
    // MDX_CORS_ALLOW_ORIGINS value, so this holds without mutating process env
    // (which would race other tests). The "no header when unset" case is proven
    // by the pure unit tests in request_security.
    let headers = crate::request_security::cors_headers(
        mdx_core::DeploymentMode::Production,
        "GET /status.json HTTP/1.1\r\nOrigin: https://evil.example\r\n\r\n",
    );
    assert!(
        !headers.contains("Access-Control-Allow-Origin: *"),
        "production CORS must never wildcard: {headers}"
    );
}

#[test]
fn local_demo_responses_keep_permissive_cors() {
    let headers = crate::request_security::cors_headers(
        mdx_core::DeploymentMode::LocalDemo,
        "GET /status.json HTTP/1.1\r\n\r\n",
    );
    assert!(headers.contains("Access-Control-Allow-Origin: *"));
}

// Auth verifier slice 2: local-secure signed test-token verifier, end to end
// through the real secured serving path with a minted HS256 token and no process
// environment (profile and key are injected).
#[cfg(test)]
mod auth_verifier_http {
    use super::*;
    use crate::auth_verifier::{ClaimMapping, VerifierConfig, mint_local_secure_token};
    use crate::request_security::RequestSecurity;
    use mdx_core::{AuthProfile, DeploymentMode};

    const NOW: &str = "2026-06-08T00:00:00Z";
    const KEY: &[u8] = b"local-secure-test-key-v1";
    // 2030-01-01 and 2020-01-01 as numeric JWT exp.
    const FUTURE_EXP: i64 = 1_893_456_000;
    const PAST_EXP: i64 = 1_577_836_800;

    fn profile() -> AuthProfile {
        AuthProfile::new("mdx-local-issuer", "mdx")
    }

    fn valid_claims() -> serde_json::Value {
        serde_json::json!({
            "iss": "mdx-local-issuer", "aud": "mdx", "exp": FUTURE_EXP,
            "tenant": "acme", "actor": "user_alice", "role": "member",
            "kind": "human", "subject": "user_alice"
        })
    }

    fn request_with_token(token: &str) -> String {
        format!(
            "POST /messages/thread-messages.json HTTP/1.1\r\nAuthorization: Bearer {token}\r\n\r\n"
        )
    }

    fn request_for_path_with_token(method: &str, path: &str, token: &str) -> String {
        format!("{method} {path} HTTP/1.1\r\nAuthorization: Bearer {token}\r\n\r\n")
    }

    fn read_request_with_token(token: &str, path: &str) -> String {
        format!("GET {path} HTTP/1.1\r\nAuthorization: Bearer {token}\r\n\r\n")
    }

    fn local_secure_config(key: Option<&[u8]>) -> VerifierConfig {
        VerifierConfig {
            profile: profile(),
            local_secure_key: key.map(|k| k.to_vec()),
            jwks: None,
            jwks_url: None,
            mapping: ClaimMapping::default(),
        }
    }

    fn secure_security(request: &str, key: Option<&[u8]>) -> RequestSecurity {
        RequestSecurity::for_connection_verified_test(
            DeploymentMode::LocalSecure,
            request,
            NOW,
            &local_secure_config(key),
        )
    }

    fn post_build(security: &RequestSecurity, body: &str) -> RouteResponse {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        route_request_secured(
            "POST",
            "/messages/thread-messages.json",
            body,
            &kernel,
            security,
        )
        .expect("route")
    }

    #[test]
    fn valid_signed_token_admits_the_governed_write() {
        let token = mint_local_secure_token(KEY, &valid_claims());
        let security = secure_security(&request_with_token(&token), Some(KEY));
        let response = post_build(&security, r#"{"message_id":"m1","body":"x"}"#);
        assert_eq!(response.status_for_test(), "200 OK");
        assert!(
            response
                .body_text()
                .contains("mdx-message-thread-message-local-post")
        );
    }

    #[test]
    fn verified_auth_session_read_projects_the_trusted_identity() {
        let token = mint_local_secure_token(KEY, &valid_claims());
        let request = read_request_with_token(&token, "/local/auth-session.json");
        let security = secure_security(&request, Some(KEY));
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response =
            route_request_secured("GET", "/local/auth-session.json", "", &kernel, &security)
                .expect("verified auth session route");

        assert_eq!(response.status_for_test(), "200 OK");
        let body: serde_json::Value =
            serde_json::from_str(response.body_text()).expect("auth session JSON");
        assert_eq!(body["status"], "VERIFIED_TRUSTED_SESSION");
        assert_eq!(body["verified_session"]["tenant_id"], "acme");
        assert_eq!(body["verified_session"]["user_id"], "user_alice");
        assert_eq!(body["verified_session"]["role"], "member");
        assert_eq!(body["verified_session"]["actor_kind"], "human");
        assert!(!response.body_text().contains("local_user"));
        assert!(!response.body_text().contains("local_tenant"));
    }

    #[test]
    fn verified_forge_classification_uses_server_identity_and_refuses_a_conflict() {
        let token = mint_local_secure_token(KEY, &valid_claims());
        let request =
            request_for_path_with_token("POST", "/forge/work-classifications.json", &token);
        let security = secure_security(&request, Some(KEY));
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let accepted = route_request_secured(
            "POST",
            "/forge/work-classifications.json",
            r#"{"classification_id":"canary_verified","ask":"Inspect the repository","repo":"mdx-rust"}"#,
            &kernel,
            &security,
        )
        .expect("verified Forge classification");

        assert_eq!(accepted.status_for_test(), "200 OK");
        assert!(
            accepted
                .body_text()
                .contains("\"auth_session_status\":\"VERIFIED_TRUSTED_SESSION\"")
        );

        let refused = route_request_secured(
            "POST",
            "/forge/work-classifications.json",
            r#"{"classification_id":"canary_conflict","actor_id":"local_user","ask":"Inspect the repository","repo":"mdx-rust"}"#,
            &kernel,
            &security,
        )
        .expect("conflicting Forge classification");
        assert_eq!(refused.status_for_test(), "403 Forbidden");
        assert!(
            refused
                .body_text()
                .contains("production_body_supplied_actor")
        );

        let projection = route_request(
            "GET",
            "/forge/work-classifications/projection.json",
            &kernel,
        )
        .expect("Forge classification projection");
        assert!(
            projection
                .body_text()
                .contains("\"classification_count\":1")
        );
        assert!(projection.body_text().contains("canary_verified"));
        assert!(!projection.body_text().contains("canary_conflict"));
    }

    #[test]
    fn missing_token_fails_closed() {
        let request = "POST /messages/thread-messages.json HTTP/1.1\r\n\r\n";
        let security = secure_security(request, Some(KEY));
        let response = post_build(&security, "{}");
        assert_eq!(response.status_for_test(), "401 Unauthorized");
        assert!(response.body_text().contains("verifier_missing_token"));
    }

    #[test]
    fn token_signed_with_the_wrong_key_fails_closed() {
        let token = mint_local_secure_token(b"some-other-key", &valid_claims());
        let security = secure_security(&request_with_token(&token), Some(KEY));
        let response = post_build(&security, "{}");
        assert_eq!(response.status_for_test(), "401 Unauthorized");
        assert!(response.body_text().contains("verifier_malformed_token"));
    }

    #[test]
    fn expired_token_fails_closed() {
        let mut claims = valid_claims();
        claims["exp"] = serde_json::json!(PAST_EXP);
        let token = mint_local_secure_token(KEY, &claims);
        let security = secure_security(&request_with_token(&token), Some(KEY));
        let response = post_build(&security, "{}");
        assert_eq!(response.status_for_test(), "401 Unauthorized");
        assert!(response.body_text().contains("verifier_expired_token"));
    }

    #[test]
    fn wrong_issuer_fails_closed() {
        let mut claims = valid_claims();
        claims["iss"] = serde_json::json!("evil-issuer");
        let token = mint_local_secure_token(KEY, &claims);
        let security = secure_security(&request_with_token(&token), Some(KEY));
        let response = post_build(&security, "{}");
        assert_eq!(response.status_for_test(), "401 Unauthorized");
        assert!(response.body_text().contains("verifier_issuer_mismatch"));
    }

    #[test]
    fn wrong_audience_fails_closed() {
        let mut claims = valid_claims();
        claims["aud"] = serde_json::json!("some-other-app");
        let token = mint_local_secure_token(KEY, &claims);
        let security = secure_security(&request_with_token(&token), Some(KEY));
        let response = post_build(&security, "{}");
        assert_eq!(response.status_for_test(), "401 Unauthorized");
        assert!(response.body_text().contains("verifier_audience_mismatch"));
    }

    #[test]
    fn body_supplied_tenant_escalation_is_refused() {
        let token = mint_local_secure_token(KEY, &valid_claims());
        let security = secure_security(&request_with_token(&token), Some(KEY));
        // Valid token for tenant=acme, but the body tries to write as another tenant.
        let response = post_build(
            &security,
            r#"{"tenant_id":"evil_tenant","request_id":"r1","requested_change":"x"}"#,
        );
        assert_eq!(response.status_for_test(), "403 Forbidden");
        assert!(
            response
                .body_text()
                .contains("production_body_supplied_tenant")
        );
        assert!(!response.body_text().contains("evil_tenant"));
    }

    #[test]
    fn body_supplied_actor_escalation_is_refused() {
        let token = mint_local_secure_token(KEY, &valid_claims());
        let security = secure_security(&request_with_token(&token), Some(KEY));
        let response = post_build(
            &security,
            r#"{"actor_id":"user_mallory","request_id":"r1"}"#,
        );
        assert_eq!(response.status_for_test(), "403 Forbidden");
        assert!(
            response
                .body_text()
                .contains("production_body_supplied_actor")
        );
    }

    #[test]
    fn body_echoing_the_session_is_allowed() {
        let token = mint_local_secure_token(KEY, &valid_claims());
        let security = secure_security(&request_with_token(&token), Some(KEY));
        // Body fields that merely echo the verified session carry no authority.
        let response = post_build(
            &security,
            r#"{"tenant_id":"acme","actor_id":"user_alice","actor_role":"member","request_id":"r1"}"#,
        );
        assert_eq!(response.status_for_test(), "200 OK");
    }

    #[test]
    fn local_secure_without_a_signing_key_fails_closed() {
        let token = mint_local_secure_token(KEY, &valid_claims());
        let security = secure_security(&request_with_token(&token), None);
        let response = post_build(&security, "{}");
        assert_eq!(response.status_for_test(), "401 Unauthorized");
        assert!(
            response
                .body_text()
                .contains("verifier_unconfigured_auth_profile")
        );
    }

    #[test]
    fn production_without_configured_jwks_fails_closed() {
        let token = mint_local_secure_token(KEY, &valid_claims());
        // Production mode but no JWKS material configured: fail closed.
        let security = RequestSecurity::for_connection_verified_test(
            DeploymentMode::Production,
            &request_with_token(&token),
            NOW,
            &local_secure_config(Some(KEY)),
        );
        let response = post_build(&security, "{}");
        assert_eq!(response.status_for_test(), "401 Unauthorized");
    }

    #[test]
    fn valid_token_records_verified_identity_not_local_stub() {
        let token = mint_local_secure_token(KEY, &valid_claims());
        let security = secure_security(&request_with_token(&token), Some(KEY));
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = route_request_secured(
            "POST",
            "/messages/thread-messages.json",
            r#"{"message_id":"m1","body":"x"}"#,
            &kernel,
            &security,
        )
        .expect("route");
        assert_eq!(response.status_for_test(), "200 OK");
        // The verified session is the identity: no local stub, the verified tenant,
        // actor, and role from the token.
        assert!(!response.body_text().contains("ACCEPTED_LOCAL_STUB"));
        assert!(
            response
                .body_text()
                .contains("\"auth_session_status\":\"VERIFIED_TRUSTED_SESSION\"")
        );
        assert!(
            response
                .body_text()
                .contains("\"auth_session_tenant_id\":\"acme\"")
        );
        assert!(
            response
                .body_text()
                .contains("\"auth_session_user_id\":\"user_alice\"")
        );
        assert!(
            response
                .body_text()
                .contains("\"auth_session_role\":\"member\"")
        );
        assert!(!response.body_text().contains("local_user"));
        // The receipt preserves the verified identity.
        let kernel = kernel.read().expect("kernel lock");
        let receipt = kernel
            .ledger()
            .entries()
            .iter()
            .find(|r| r.kind == "message.human.posted")
            .expect("message receipt");
        assert_eq!(receipt.tenant_id.as_str(), "acme");
        assert_eq!(receipt.actor_id.as_str(), "user_alice");
        assert_eq!(receipt.payload["identity_source"], "trusted_session");
        assert_eq!(receipt.payload["identity_actor_kind"], "human");
        assert_eq!(receipt.payload["identity_subject_actor_id"], "user_alice");
    }

    fn request_for(path: &str, token: &str) -> String {
        format!("POST {path} HTTP/1.1\r\nAuthorization: Bearer {token}\r\n\r\n")
    }

    #[test]
    fn valid_token_records_verified_identity_across_twin_writes() {
        // The Twin governed writes: the draft compose path plus the boundary
        // refusal and runtime-control recorders. Each, posted with a valid
        // trusted-session token, records the verified identity instead of the
        // local stub. The refusal/control responses do not surface tenant/actor,
        // so those are asserted on the receipt; the draft response surfaces all.
        let routes = [
            ("/twin/session-drafts.json", "twin.session.draft.admitted"),
            (
                "/twin/provider-call-refusals.json",
                "twin.session.provider_call.refused",
            ),
            (
                "/twin/production-memory-refusals.json",
                "twin.session.production_memory.refused",
            ),
            (
                "/twin/tool-execution-refusals.json",
                "twin.session.tool_execution.refused",
            ),
            (
                "/twin/agent-mode-preflights.json",
                "twin.agent_mode.preflighted",
            ),
            ("/twin/hook-preflights.json", "twin.hook.preflighted"),
            ("/twin/skill-preflights.json", "twin.skill.preflighted"),
            (
                "/twin/connector-preflights.json",
                "twin.connector.preflighted",
            ),
            (
                "/twin/agent-handoff-preflights.json",
                "twin.agent_handoff.preflighted",
            ),
        ];
        for (path, kind) in routes {
            let token = mint_local_secure_token(KEY, &valid_claims());
            let security = secure_security(&request_for(path, &token), Some(KEY));
            let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
            let response = route_request_secured("POST", path, "{}", &kernel, &security)
                .unwrap_or_else(|error| panic!("route {path} failed: {error}"));
            assert_eq!(response.status_for_test(), "200 OK", "{path}");
            let body = response.body_text().to_string();
            assert!(
                !body.contains("ACCEPTED_LOCAL_STUB"),
                "{path} still emits the local stub"
            );
            assert!(
                body.contains("\"auth_session_status\":\"VERIFIED_TRUSTED_SESSION\""),
                "{path}"
            );
            let kernel = kernel.read().expect("kernel lock");
            let receipt = kernel
                .ledger()
                .entries()
                .iter()
                .find(|receipt| receipt.kind == kind)
                .unwrap_or_else(|| panic!("{path}: no {kind} receipt"));
            assert_eq!(receipt.tenant_id.as_str(), "acme", "{path}");
            assert_eq!(receipt.actor_id.as_str(), "user_alice", "{path}");
            assert_eq!(
                receipt.payload["identity_source"], "trusted_session",
                "{path}"
            );
            assert_eq!(receipt.payload["identity_actor_kind"], "human", "{path}");
            assert_eq!(
                receipt.payload["identity_subject_actor_id"], "user_alice",
                "{path}"
            );
        }
    }

    #[test]
    fn valid_token_records_verified_identity_across_message_writes() {
        // The Message governed writes: thread post, presence, fanout, relay
        // observation, and the realtime cutover/replay/isolation/refusal
        // recorders. Each, posted with a valid trusted-session token, records the
        // verified identity instead of the local stub. Some responses surface
        // only auth_session_status, so the verified identity is asserted on the
        // receipt.
        let routes = [
            ("/messages/thread-messages.json", "message.human.posted"),
            (
                "/messages/presence-requests.json",
                "message.presence.requested",
            ),
            ("/messages/fanout-requests.json", "message.fanout.requested"),
            (
                "/messages/relay-observations.json",
                "message.relay.observed",
            ),
            (
                "/messages/realtime-cutover-preflights.json",
                "message.realtime.cutover.preflighted",
            ),
            (
                "/messages/delivery-replay-batches.json",
                "message.delivery.replay.recorded",
            ),
            (
                "/messages/subscription-isolation-checks.json",
                "message.subscription.isolation.checked",
            ),
            (
                "/messages/service-role-fanout-refusals.json",
                "message.service_role.fanout.refused",
            ),
        ];
        for (path, kind) in routes {
            let token = mint_local_secure_token(KEY, &valid_claims());
            let security = secure_security(&request_for(path, &token), Some(KEY));
            let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
            let response = route_request_secured("POST", path, "{}", &kernel, &security)
                .unwrap_or_else(|error| panic!("route {path} failed: {error}"));
            assert_eq!(response.status_for_test(), "200 OK", "{path}");
            let body = response.body_text();
            assert!(
                !body.contains("ACCEPTED_LOCAL_STUB"),
                "{path} still emits the local stub"
            );
            assert!(
                body.contains("\"auth_session_status\":\"VERIFIED_TRUSTED_SESSION\""),
                "{path}"
            );
            let kernel = kernel.read().expect("kernel lock");
            let receipt = kernel
                .ledger()
                .entries()
                .iter()
                .find(|receipt| receipt.kind == kind)
                .unwrap_or_else(|| panic!("{path}: no {kind} receipt"));
            assert_eq!(receipt.tenant_id.as_str(), "acme", "{path}");
            assert_eq!(receipt.actor_id.as_str(), "user_alice", "{path}");
            assert_eq!(
                receipt.payload["identity_source"], "trusted_session",
                "{path}"
            );
            assert_eq!(receipt.payload["identity_actor_kind"], "human", "{path}");
            assert_eq!(
                receipt.payload["identity_subject_actor_id"], "user_alice",
                "{path}"
            );
        }
    }

    #[test]
    fn valid_token_records_verified_identity_across_pages_writes() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let seed_draft = seed_pages_edit_draft_for_test(
            &kernel,
            "acme",
            "user_alice",
            "page_auth_identity",
            "draft_auth_seed",
            (
                "Auth identity seed",
                "world_model://pages/page_auth_identity/body/seed",
                "rev_auth_seed",
            ),
        );
        let source_publication_receipt_id = {
            let kernel = kernel.read().expect("kernel lock");
            kernel
                .ledger()
                .query()
                .by_id(&seed_draft)
                .and_then(|receipt| receipt.payload.get("source_publication_receipt_id"))
                .cloned()
                .expect("seed publication receipt")
        };
        let post = |path: &str, body: &str| {
            let token = mint_local_secure_token(KEY, &valid_claims());
            let security = secure_security(&request_for(path, &token), Some(KEY));
            let response = route_request_secured("POST", path, body, &kernel, &security)
                .unwrap_or_else(|error| panic!("route {path} failed: {error}"));
            assert_eq!(response.status_for_test(), "200 OK", "{path}");
            let body = response.body_text().to_string();
            assert!(
                body.contains("\"auth_session_status\":\"VERIFIED_TRUSTED_SESSION\""),
                "{path}: {body}"
            );
            body
        };

        let edit_body = format!(
            r#"{{"draft_id":"draft_auth_identity","document_id":"page_auth_identity","title":"Auth identity","body_ref":"world_model://pages/page_auth_identity/body/approved","source_publication_receipt_id":"{source_publication_receipt_id}","revision_id":"rev_auth_identity"}}"#
        );
        let edit = post("/pages/edit-drafts.json", &edit_body);
        let draft_receipt_id = json_string_field_for_test(&edit, "edit_draft_receipt_id");
        let request = post(
            "/pages/approval-requests.json",
            &format!(
                r#"{{"approval_request_id":"approval_auth_identity","source_edit_draft_receipt_id":"{draft_receipt_id}","document_id":"page_auth_identity","draft_id":"draft_auth_identity","requested_visibility":"tenant_review"}}"#
            ),
        );
        let request_receipt_id =
            json_string_field_for_test(&request, "approval_request_receipt_id");
        let decision = post(
            "/pages/approval-decisions/approve.json",
            &format!(
                r#"{{"approval_decision_id":"decision_auth_identity","approval_request_receipt_id":"{request_receipt_id}","decision_note":"Approve trusted-session identity proof."}}"#
            ),
        );
        let decision_receipt_id =
            json_string_field_for_test(&decision, "approval_decision_receipt_id");
        post(
            "/pages/publications.json",
            &format!(
                r#"{{"document_id":"page_auth_identity","approval_decision_receipt_id":"{decision_receipt_id}","page_type":"knowledge"}}"#
            ),
        );
        let search = post(
            "/pages/search-preflights.json",
            &format!(
                r#"{{"preflight_id":"search_auth_identity","source_approval_request_receipt_id":"{request_receipt_id}","document_id":"page_auth_identity","revision_id":"rev_auth_identity","citation_handle":"pages/auth-identity#rev","attachment_policy_id":"attachment-policy-auth","requested_index_scope":"tenant operating memory"}}"#
            ),
        );
        let search_receipt_id = json_string_field_for_test(&search, "preflight_receipt_id");
        let comparison = post(
            "/pages/revision-citation-comparisons.json",
            &format!(
                r#"{{"comparison_id":"comparison_auth_identity","source_search_preflight_receipt_id":"{search_receipt_id}","document_id":"page_auth_identity","revision_id":"rev_auth_identity","citation_handle":"pages/auth-identity#rev","v1_revision_key":"rev_auth_identity"}}"#
            ),
        );
        let comparison_receipt_id =
            json_string_field_for_test(&comparison, "comparison_receipt_id");
        let visibility = post(
            "/pages/publication-visibility-checks.json",
            &format!(
                r#"{{"visibility_check_id":"visibility_auth_identity","source_revision_citation_receipt_id":"{comparison_receipt_id}","document_id":"page_auth_identity","revision_id":"rev_auth_identity","requested_visibility":"tenant_review"}}"#
            ),
        );
        let visibility_receipt_id =
            json_string_field_for_test(&visibility, "visibility_check_receipt_id");
        post(
            "/pages/embedding-provider-refusals.json",
            &format!(
                r#"{{"refusal_id":"embedding_auth_identity","source_visibility_check_receipt_id":"{visibility_receipt_id}","document_id":"page_auth_identity","revision_id":"rev_auth_identity","provider_profile":"embedding-provider-local-blocked"}}"#
            ),
        );

        let routes = [
            ("/pages/publications.json", "pages.document.published"),
            ("/pages/edit-drafts.json", "pages.edit.draft.saved"),
            ("/pages/approval-requests.json", "pages.approval.requested"),
            (
                "/pages/approval-decisions/approve.json",
                "pages.approval.decision.recorded",
            ),
            ("/pages/search-preflights.json", "pages.search.preflighted"),
            (
                "/pages/revision-citation-comparisons.json",
                "pages.revision_citation.compared",
            ),
            (
                "/pages/publication-visibility-checks.json",
                "pages.publication.visibility.checked",
            ),
            (
                "/pages/embedding-provider-refusals.json",
                "pages.embedding_provider.refused",
            ),
        ];
        for (path, kind) in routes {
            let kernel = kernel.read().expect("kernel lock");
            let receipt = kernel
                .ledger()
                .entries()
                .iter()
                .rfind(|receipt| receipt.kind == kind)
                .unwrap_or_else(|| panic!("{path}: no {kind} receipt"));
            assert_eq!(receipt.tenant_id.as_str(), "acme", "{path}");
            assert_eq!(receipt.actor_id.as_str(), "user_alice", "{path}");
            assert_eq!(
                receipt.payload["identity_source"], "trusted_session",
                "{path}"
            );
            assert_eq!(receipt.payload["identity_actor_kind"], "human", "{path}");
            assert_eq!(
                receipt.payload["identity_subject_actor_id"], "user_alice",
                "{path}"
            );
        }
    }

    #[test]
    fn valid_token_records_verified_identity_across_auth_writes() {
        // The auth and v1-cutover governed writes: tenant-policy preflight,
        // invite, role assignment, session control, and the read-shadow and
        // write-mirror approval request/decision recorders. Each, posted with a
        // valid trusted-session token, records the verified identity instead of
        // the local stub.
        let routes = [
            (
                "/auth/tenant-policy-preflights.json",
                "auth.tenant_policy.preflighted",
            ),
            ("/auth/invite-requests.json", "auth.invite.requested"),
            (
                "/auth/role-assignments.json",
                "auth.role_assignment.recorded",
            ),
            (
                "/auth/session-controls.json",
                "auth.session.activation.recorded",
            ),
            (
                "/v1/read-shadow-approval-requests.json",
                "v1.read_shadow.approval.requested",
            ),
            (
                "/v1/read-shadow-approval-decisions.json",
                "v1.read_shadow.approval.decision.recorded",
            ),
            (
                "/v1/write-mirror-approval-requests.json",
                "v1.write_mirror.approval.requested",
            ),
            (
                "/v1/write-mirror-approval-decisions.json",
                "v1.write_mirror.approval.decision.recorded",
            ),
        ];
        // These routes form dependency chains (a role assignment needs an
        // invite; a decision needs its request), so they share one kernel and are
        // posted in order. Each route's own receipt kind is unique.
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        for (path, kind) in routes {
            let token = mint_local_secure_token(KEY, &valid_claims());
            let security = secure_security(&request_for(path, &token), Some(KEY));
            let response = route_request_secured("POST", path, "{}", &kernel, &security)
                .unwrap_or_else(|error| panic!("route {path} failed: {error}"));
            assert_eq!(response.status_for_test(), "200 OK", "{path}");
            let body = response.body_text();
            assert!(
                !body.contains("ACCEPTED_LOCAL_STUB"),
                "{path} still emits the local stub"
            );
            assert!(
                body.contains("\"auth_session_status\":\"VERIFIED_TRUSTED_SESSION\""),
                "{path}"
            );
            let guard = kernel.read().expect("kernel lock");
            let receipt = guard
                .ledger()
                .entries()
                .iter()
                .find(|receipt| receipt.kind == kind)
                .unwrap_or_else(|| panic!("{path}: no {kind} receipt"));
            assert_eq!(receipt.tenant_id.as_str(), "acme", "{path}");
            assert_eq!(receipt.actor_id.as_str(), "user_alice", "{path}");
            assert_eq!(
                receipt.payload["identity_source"], "trusted_session",
                "{path}"
            );
            assert_eq!(receipt.payload["identity_actor_kind"], "human", "{path}");
            assert_eq!(
                receipt.payload["identity_subject_actor_id"], "user_alice",
                "{path}"
            );
        }
    }

    #[test]
    fn valid_token_records_verified_identity_on_the_ship_decision() {
        // The human ship decision. An empty body lacks the binding evidence, so
        // the kernel records a first-class blocked decision - which is still a
        // governed write that must carry the verified identity, not the stub.
        let path = "/forge/ship-decisions.json";
        let token = mint_local_secure_token(KEY, &valid_claims());
        let security = secure_security(&request_for(path, &token), Some(KEY));
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response =
            route_request_secured("POST", path, "{}", &kernel, &security).expect("route");
        assert_eq!(response.status_for_test(), "200 OK");
        let body = response.body_text();
        assert!(!body.contains("ACCEPTED_LOCAL_STUB"));
        assert!(body.contains("\"auth_session_status\":\"VERIFIED_TRUSTED_SESSION\""));
        assert!(body.contains("\"auth_session_tenant_id\":\"acme\""));
        assert!(body.contains("\"auth_session_user_id\":\"user_alice\""));
        assert!(body.contains("\"auth_session_role\":\"member\""));
        let kernel = kernel.read().expect("kernel lock");
        let receipt = kernel
            .ledger()
            .entries()
            .iter()
            .find(|receipt| receipt.kind == "forge.ship.decision_blocked")
            .expect("blocked ship decision receipt");
        assert_eq!(receipt.tenant_id.as_str(), "acme");
        assert_eq!(receipt.actor_id.as_str(), "user_alice");
        assert_eq!(receipt.payload["identity_source"], "trusted_session");
        assert_eq!(receipt.payload["identity_actor_kind"], "human");
        assert_eq!(receipt.payload["identity_subject_actor_id"], "user_alice");
    }

    #[test]
    fn local_demo_still_accepts_the_fixture_without_a_token() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let security = RequestSecurity::local_demo();
        let response = route_request_secured(
            "POST",
            "/messages/thread-messages.json",
            r#"{"request_id":"r1"}"#,
            &kernel,
            &security,
        )
        .expect("route");
        assert_eq!(response.status_for_test(), "200 OK");
        assert!(response.body_text().contains("ACCEPTED_LOCAL_STUB"));
    }
}

// Auth verifier slice 3: production OIDC/JWT RS256 verifier against a static JWKS
// fixture. The RSA keypair was generated offline; only the PUBLIC JWKS and a
// pre-signed token are embedded here - no private key, no secret.
#[cfg(test)]
mod auth_verifier_oidc {
    use super::*;
    use crate::auth_verifier::{
        ClaimMapping, JwksFetchError, JwksResolver, RateLimitedJwks, VerifierConfig,
        build_jwks_resolver,
    };
    use crate::request_security::RequestSecurity;
    use jsonwebtoken::jwk::JwkSet;
    use mdx_core::{AuthProfile, DeploymentMode, VerifierRefusal};
    use std::time::{Duration, Instant};

    // A real RS256 OIDC token (iss=https://idp.example.com/, aud=mdx-production,
    // exp=2100-01-01, human user_alice@acme), signed by an offline test RSA key.
    const RS256_TOKEN_PARTS: [&str; 3] = [
        "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6Im1keC10ZXN0LWtleS0xIn0",
        "eyJpc3MiOiJodHRwczovL2lkcC5leGFtcGxlLmNvbS8iLCJhdWQiOiJtZHgtcHJvZHVjdGlvbiIsImV4cCI6NDEwMjQ0NDgwMCwidGVuYW50IjoiYWNtZSIsImFjdG9yIjoidXNlcl9hbGljZSIsInJvbGUiOiJtZW1iZXIiLCJraW5kIjoiaHVtYW4iLCJzdWJqZWN0IjoidXNlcl9hbGljZSJ9",
        "dlit22w8x3udDVmw0hH2Udvhc2Zkq-ZkTIDLTj4ZIdFCHlim79dvPFctH77F54zLMcUpnAc18HIh8rZP3Cu0lmVzGVTdH6uwFYvkdQPAmlO_BEpoOF2MR98AztMiirJewTOvV-06MhgPno1SBZvRnvSiFKmcI-6UBLn5_eJn7zBNLR2L8UuRBGqh5dNhKCTFdhP47hjmEHDT6jn2_LjOXGN-P3M7_1-6g3ySjDAcDj0I9UJ5swHJLnvNTNtMBOC24kvO5cab4SEr1KWN4W_WeQTRDZs5buWsl1FZf3oqMql2j_WmJggFQX-n-8mbgPW6iVppxdDKZok6sTgyW38CRw",
    ];
    const JWKS: &str = r#"{"keys":[{"kty":"RSA","use":"sig","alg":"RS256","kid":"mdx-test-key-1","n":"uWyEi8FKFQn0N36T6ovMRlgN-bBMi0qsuM7e10Vku0DeFMUb9XiP6w_HIYajwbpsmqJo2HotehKKVfhvv28Zl7ABkYYO0bLYOy_vnsOEnGZcnPU86sxdLLCnoav2xjwTsAOj71fPoaMS2w3-Fnr7hbOdtZxapjqc-ggcBp1gQDay2s3dKS0fH-6KE7pr8nwY4Gw7OsAMiIgIZcwnld-8_zPH19uo31l7e69mhB02u6LMEMWjWsV-P49pBjtr9M-Pj09eJ7hWjP8F7zpr4m59AVIzk2mhNBCDKr8SVA5-37itEASLbWzj0hVTuokP4bGxuGNrcU4Czw8-0jxQSKSRMw","e":"AQAB"}]}"#;

    // A Supabase-shaped ES256 token and public JWKS generated offline for this
    // fixture. Only public verification material and the signed token are
    // committed; no private key or provider credential is present.
    const SUPABASE_ES256_TOKEN: &str = "eyJhbGciOiJFUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6Im1keC1zdXBhYmFzZS1lczI1Ni10ZXN0In0.eyJpc3MiOiJodHRwczovL3Byb2plY3QtcmVmLnN1cGFiYXNlLmNvL2F1dGgvdjEiLCJhdWQiOiJhdXRoZW50aWNhdGVkIiwiZXhwIjo0MTAyNDQ0ODAwLCJpYXQiOjE3ODQzMzI4MDAsInN1YiI6IjExMTExMTExLTIyMjItMzMzMy00NDQ0LTU1NTU1NTU1NTU1NSIsInJvbGUiOiJhdXRoZW50aWNhdGVkIiwibWR4X3RlbmFudF9pZCI6InRlbmFudF9iZXRhIiwibWR4X3JvbGUiOiJvcGVyYXRvciIsIm1keF9hY3Rvcl9raW5kIjoiaHVtYW4ifQ.Orr3l4oXIlrjWzimG5-Y1xk-SNmJdVWlOhZl-kNTAoo9N4ST4ZAdC5BcP59LcgHlJWTtKswsHbaRfubo-eUAsQ";
    const SUPABASE_ES256_JWKS: &str = r#"{"keys":[{"kty":"EC","use":"sig","alg":"ES256","kid":"mdx-supabase-es256-test","crv":"P-256","x":"KLSpdzLXw701xFsOHJb2B4YwtON_seqwgRGFcAGsxqo","y":"5uZfAu_GGdY1wR2nFKf6p5yjREh4APArmaDvWSOEc78"}]}"#;

    const NOW: &str = "2026-06-08T00:00:00Z";
    // After the token's 2100-01-01 expiry.
    const NOW_AFTER_EXP: &str = "2200-01-01T00:00:00Z";

    fn jwks() -> JwkSet {
        serde_json::from_str(JWKS).expect("parse jwks fixture")
    }

    fn rs256_token() -> String {
        RS256_TOKEN_PARTS.join(".")
    }

    fn config(profile: AuthProfile, jwks: Option<JwkSet>) -> VerifierConfig {
        VerifierConfig {
            profile,
            local_secure_key: None,
            jwks,
            jwks_url: None,
            mapping: ClaimMapping::default(),
        }
    }

    fn correct_profile() -> AuthProfile {
        AuthProfile::new("https://idp.example.com/", "mdx-production")
    }

    fn supabase_config() -> VerifierConfig {
        let mapping = ClaimMapping {
            tenant: "mdx_tenant_id".to_string(),
            actor: "sub".to_string(),
            role: "mdx_role".to_string(),
            kind: "mdx_actor_kind".to_string(),
            subject: "sub".to_string(),
            ..ClaimMapping::default()
        };
        VerifierConfig {
            profile: AuthProfile::new("https://project-ref.supabase.co/auth/v1", "authenticated"),
            local_secure_key: None,
            jwks: Some(
                serde_json::from_str(SUPABASE_ES256_JWKS).expect("parse Supabase ES256 JWKS"),
            ),
            jwks_url: None,
            mapping,
        }
    }

    fn request() -> String {
        let token = rs256_token();
        format!(
            "POST /messages/thread-messages.json HTTP/1.1\r\nAuthorization: Bearer {token}\r\n\r\n"
        )
    }

    fn run(config: &VerifierConfig, now: &str, body: &str) -> RouteResponse {
        let security = RequestSecurity::for_connection_verified_test(
            DeploymentMode::Production,
            &request(),
            now,
            config,
        );
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        route_request_secured(
            "POST",
            "/messages/thread-messages.json",
            body,
            &kernel,
            &security,
        )
        .expect("route")
    }

    #[test]
    fn valid_oidc_token_admits_a_production_governed_write() {
        let response = run(
            &config(correct_profile(), Some(jwks())),
            NOW,
            r#"{"message_id":"m1","body":"x"}"#,
        );
        assert_eq!(response.status_for_test(), "200 OK");
        assert!(
            response
                .body_text()
                .contains("mdx-message-thread-message-local-post")
        );
    }

    #[test]
    fn supabase_es256_token_admits_a_production_governed_write() {
        let request = format!(
            "POST /messages/thread-messages.json HTTP/1.1\r\nAuthorization: Bearer {SUPABASE_ES256_TOKEN}\r\n\r\n"
        );
        let security = RequestSecurity::for_connection_verified_test(
            DeploymentMode::Production,
            &request,
            NOW,
            &supabase_config(),
        );
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = route_request_secured(
            "POST",
            "/messages/thread-messages.json",
            r#"{"message_id":"m-es256","body":"verified"}"#,
            &kernel,
            &security,
        )
        .expect("route");
        assert_eq!(response.status_for_test(), "200 OK");
        let receipt = kernel
            .read()
            .expect("kernel lock")
            .ledger()
            .entries()
            .last()
            .cloned()
            .expect("receipt");
        assert_eq!(receipt.tenant_id.as_str(), "tenant_beta");
        assert_eq!(
            receipt.actor_id.as_str(),
            "11111111-2222-3333-4444-555555555555"
        );
    }

    #[test]
    fn production_oidc_refuses_a_symmetric_algorithm_before_verification() {
        const HS256_TOKEN_WITH_KID: &str =
            "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6Im1keC10ZXN0LWtleS0xIn0.e30.invalid";
        let request = format!(
            "POST /messages/thread-messages.json HTTP/1.1\r\nAuthorization: Bearer {HS256_TOKEN_WITH_KID}\r\n\r\n"
        );
        let security = RequestSecurity::for_connection_verified_test(
            DeploymentMode::Production,
            &request,
            NOW,
            &config(correct_profile(), Some(jwks())),
        );
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = route_request_secured(
            "POST",
            "/messages/thread-messages.json",
            "{}",
            &kernel,
            &security,
        )
        .expect("route");
        assert_eq!(response.status_for_test(), "401 Unauthorized");
        assert!(response.body_text().contains("verifier_malformed_token"));
    }

    #[test]
    fn expired_oidc_token_fails_closed() {
        let response = run(
            &config(correct_profile(), Some(jwks())),
            NOW_AFTER_EXP,
            "{}",
        );
        assert_eq!(response.status_for_test(), "401 Unauthorized");
        assert!(response.body_text().contains("verifier_expired_token"));
    }

    #[test]
    fn wrong_issuer_fails_closed() {
        let profile = AuthProfile::new("https://evil.example.com/", "mdx-production");
        let response = run(&config(profile, Some(jwks())), NOW, "{}");
        assert_eq!(response.status_for_test(), "401 Unauthorized");
        assert!(response.body_text().contains("verifier_issuer_mismatch"));
    }

    #[test]
    fn wrong_audience_fails_closed() {
        let profile = AuthProfile::new("https://idp.example.com/", "some-other-app");
        let response = run(&config(profile, Some(jwks())), NOW, "{}");
        assert_eq!(response.status_for_test(), "401 Unauthorized");
        assert!(response.body_text().contains("verifier_audience_mismatch"));
    }

    #[test]
    fn tampered_signature_fails_closed() {
        // Flip the last character of the signature.
        let mut tampered = rs256_token();
        let last = tampered.pop().unwrap();
        tampered.push(if last == 'A' { 'B' } else { 'A' });
        let request = format!(
            "POST /messages/thread-messages.json HTTP/1.1\r\nAuthorization: Bearer {tampered}\r\n\r\n"
        );
        let security = RequestSecurity::for_connection_verified_test(
            DeploymentMode::Production,
            &request,
            NOW,
            &config(correct_profile(), Some(jwks())),
        );
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = route_request_secured(
            "POST",
            "/messages/thread-messages.json",
            "{}",
            &kernel,
            &security,
        )
        .expect("route");
        assert_eq!(response.status_for_test(), "401 Unauthorized");
        assert!(response.body_text().contains("verifier_malformed_token"));
    }

    #[test]
    fn unknown_signing_key_fails_closed() {
        // A JWKS that does not contain the token's kid, and a static source so the
        // rotation refresh returns the same empty set: the key stays unknown.
        let empty: JwkSet = serde_json::from_str(r#"{"keys":[]}"#).unwrap();
        let response = run(&config(correct_profile(), Some(empty)), NOW, "{}");
        assert_eq!(response.status_for_test(), "401 Unauthorized");
        assert!(
            response
                .body_text()
                .contains("verifier_signing_key_unknown")
        );
    }

    #[test]
    fn production_without_jwks_material_fails_closed() {
        let response = run(&config(correct_profile(), None), NOW, "{}");
        assert_eq!(response.status_for_test(), "401 Unauthorized");
        assert!(
            response
                .body_text()
                .contains("verifier_unconfigured_auth_profile")
        );
    }

    // JWKS rotation cache, proven against fixture fetches (no live endpoint).
    fn empty_jwks() -> JwkSet {
        serde_json::from_str(r#"{"keys":[]}"#).expect("empty jwks")
    }
    const MIN: Duration = Duration::from_secs(60);

    #[test]
    fn endpoint_cache_refreshes_on_an_unknown_kid() {
        // The cache starts empty; the rotated-in JWKS holds the kid.
        let mut cache = RateLimitedJwks::empty();
        let t0 = Instant::now();
        assert!(
            cache
                .signing_key("mdx-test-key-1", t0, MIN, || Ok(jwks()))
                .is_ok()
        );
        // The now-current key is served without another fetch.
        assert!(
            cache
                .signing_key("mdx-test-key-1", t0, MIN, || panic!("must not fetch"))
                .is_ok()
        );
    }

    #[test]
    fn endpoint_cache_fails_closed_on_a_fetch_failure() {
        let mut cache = RateLimitedJwks::empty();
        let result = cache.signing_key("mdx-test-key-1", Instant::now(), MIN, || {
            Err(JwksFetchError)
        });
        assert!(matches!(result, Err(VerifierRefusal::JwksFetchFailed)));
    }

    #[test]
    fn endpoint_cache_fails_closed_on_an_unknown_kid_after_refresh() {
        let mut cache = RateLimitedJwks::empty();
        let result = cache.signing_key("a-kid-the-idp-never-issued", Instant::now(), MIN, || {
            Ok(jwks())
        });
        assert!(matches!(result, Err(VerifierRefusal::SigningKeyUnknown)));
    }

    #[test]
    fn endpoint_cache_rate_limits_fetches_across_calls() {
        use std::cell::Cell;
        let fetches = Cell::new(0u32);
        let mut cache = RateLimitedJwks::empty();
        let t0 = Instant::now();
        // The fetch never returns the requested kid, so each *due* lookup fetches.
        // First unknown-kid lookup is due, so it fetches once.
        let _ = cache.signing_key("bogus", t0, MIN, || {
            fetches.set(fetches.get() + 1);
            Ok(empty_jwks())
        });
        assert_eq!(fetches.get(), 1);
        // A second lookup 10s later is inside the 60s window: it must NOT fetch.
        let _ = cache.signing_key("bogus", t0 + Duration::from_secs(10), MIN, || {
            fetches.set(fetches.get() + 1);
            Ok(empty_jwks())
        });
        assert_eq!(
            fetches.get(),
            1,
            "the rate limit must prevent a second fetch within the window"
        );
        // After the window elapses, a refresh is due again.
        let _ = cache.signing_key("bogus", t0 + Duration::from_secs(61), MIN, || {
            fetches.set(fetches.get() + 1);
            Ok(empty_jwks())
        });
        assert_eq!(fetches.get(), 2);
    }

    #[test]
    fn endpoint_cache_backs_off_after_a_failed_fetch() {
        use std::cell::Cell;
        let fetches = Cell::new(0u32);
        let mut cache = RateLimitedJwks::empty();
        let t0 = Instant::now();
        // A failing endpoint: the attempt is timestamped, so it backs off too.
        let _ = cache.signing_key("bogus", t0, MIN, || {
            fetches.set(fetches.get() + 1);
            Err(JwksFetchError)
        });
        assert_eq!(fetches.get(), 1);
        let _ = cache.signing_key("bogus", t0 + Duration::from_secs(5), MIN, || {
            fetches.set(fetches.get() + 1);
            Err(JwksFetchError)
        });
        assert_eq!(fetches.get(), 1, "a failing endpoint must not be hammered");
    }

    #[test]
    fn inline_resolver_pins_a_key_and_fails_closed_on_an_unknown_kid() {
        let resolver = JwksResolver::Inline(jwks());
        assert!(resolver.signing_key("mdx-test-key-1").is_ok());
        assert!(matches!(
            resolver.signing_key("nope"),
            Err(VerifierRefusal::SigningKeyUnknown)
        ));
    }

    #[test]
    fn inline_jwks_takes_precedence_over_a_configured_endpoint() {
        // Both inline and a URL configured: inline wins (emergency pinning).
        let mut cfg = config(correct_profile(), Some(jwks()));
        cfg.jwks_url = Some("https://idp.example.com/.well-known/jwks.json".to_string());
        assert!(matches!(
            build_jwks_resolver(&cfg),
            Some(JwksResolver::Inline(_))
        ));
    }

    #[test]
    fn an_https_endpoint_with_no_inline_builds_an_endpoint_resolver() {
        let mut cfg = config(correct_profile(), None);
        cfg.jwks_url = Some("https://idp.example.com/.well-known/jwks.json".to_string());
        assert!(matches!(
            build_jwks_resolver(&cfg),
            Some(JwksResolver::Endpoint(_))
        ));
    }

    #[test]
    fn a_non_https_endpoint_is_refused_and_fails_closed() {
        let mut cfg = config(correct_profile(), None);
        cfg.jwks_url = Some("http://idp.example.com/.well-known/jwks.json".to_string());
        assert!(
            build_jwks_resolver(&cfg).is_none(),
            "a plaintext JWKS endpoint must be refused"
        );
    }

    #[test]
    fn body_supplied_tenant_escalation_is_refused_in_production() {
        let response = run(
            &config(correct_profile(), Some(jwks())),
            NOW,
            r#"{"tenant_id":"evil_tenant","request_id":"r1"}"#,
        );
        assert_eq!(response.status_for_test(), "403 Forbidden");
        assert!(
            response
                .body_text()
                .contains("production_body_supplied_tenant")
        );
    }
}

// Auth verifier slice 4: agent/control-plane delegation binding. An agent token
// is validated through the delegation runtime: separate agent and subject, scope
// within the sponsor, valid expiry, not revoked, no self-sponsorship.
#[cfg(test)]
mod auth_verifier_agent {
    use super::*;
    use crate::auth_verifier::{ClaimMapping, VerifierConfig, mint_local_secure_token};
    use crate::request_security::RequestSecurity;
    use mdx_core::{AuthProfile, DeploymentMode};

    const NOW: &str = "2026-06-08T00:00:00Z";
    const KEY: &[u8] = b"local-secure-test-key-v1";
    const FUTURE_EXP: i64 = 1_893_456_000;
    const IAT: i64 = 1_700_000_000;

    fn config() -> VerifierConfig {
        VerifierConfig {
            profile: AuthProfile::new("mdx-local-issuer", "mdx"),
            local_secure_key: Some(KEY.to_vec()),
            jwks: None,
            jwks_url: None,
            mapping: ClaimMapping::default(),
        }
    }

    fn agent_claims() -> serde_json::Value {
        serde_json::json!({
            "iss": "mdx-local-issuer", "aud": "mdx", "exp": FUTURE_EXP, "iat": IAT,
            "tenant": "acme", "actor": "agent_worker", "role": "agent", "kind": "agent",
            "subject": "user_alice", "delegation_id": "lease_1",
            "scope": "forge:run:update",
            "sponsor_scope": "forge:run:read,forge:run:update",
            "policy_decision_id": "policy_1"
        })
    }

    fn run(claims: &serde_json::Value, body: &str) -> RouteResponse {
        let token = mint_local_secure_token(KEY, claims);
        let request = format!(
            "POST /forge/build-requests.json HTTP/1.1\r\nAuthorization: Bearer {token}\r\n\r\n"
        );
        let security = RequestSecurity::for_connection_verified_test(
            DeploymentMode::LocalSecure,
            &request,
            NOW,
            &config(),
        );
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        route_request_secured("POST", "/forge/runs.json", body, &kernel, &security).expect("route")
    }

    #[test]
    fn valid_agent_delegation_admits() {
        let response = run(&agent_claims(), r#"{"message_id":"m1","body":"x"}"#);
        assert_eq!(response.status_for_test(), "200 OK");
    }

    #[test]
    fn read_scoped_agent_cannot_post_governed_write() {
        let mut claims = agent_claims();
        claims["scope"] = serde_json::json!("forge:run:read");
        let response = run(&claims, r#"{"message_id":"m1","body":"x"}"#);
        assert_eq!(response.status_for_test(), "403 Forbidden");
        assert!(
            response
                .body_text()
                .contains("production_actor_scope_forbidden")
        );
    }

    #[test]
    fn agent_scope_exceeding_sponsor_is_denied() {
        let mut claims = agent_claims();
        claims["scope"] = serde_json::json!("forge:run:delete");
        let response = run(&claims, "{}");
        assert_eq!(response.status_for_test(), "401 Unauthorized");
        assert!(
            response
                .body_text()
                .contains("delegation_scope_exceeds_sponsor")
        );
    }

    #[test]
    fn agent_acting_as_its_own_sponsor_is_denied() {
        let mut claims = agent_claims();
        claims["subject"] = serde_json::json!("agent_worker");
        let response = run(&claims, "{}");
        assert_eq!(response.status_for_test(), "401 Unauthorized");
        assert!(response.body_text().contains("delegation_self_sponsorship"));
    }

    #[test]
    fn revoked_agent_delegation_is_denied() {
        let mut claims = agent_claims();
        claims["revoked"] = serde_json::json!(true);
        let response = run(&claims, "{}");
        assert_eq!(response.status_for_test(), "401 Unauthorized");
        assert!(response.body_text().contains("delegation_revoked"));
    }

    #[test]
    fn agent_without_delegation_id_is_denied() {
        let mut claims = agent_claims();
        claims.as_object_mut().unwrap().remove("delegation_id");
        let response = run(&claims, "{}");
        assert_eq!(response.status_for_test(), "401 Unauthorized");
        assert!(
            response
                .body_text()
                .contains("verifier_delegation_missing_for_agent")
        );
    }

    #[test]
    fn agent_body_supplied_actor_escalation_is_refused() {
        // A valid agent session cannot be overridden by a body-supplied actor.
        let response = run(
            &agent_claims(),
            r#"{"actor_id":"user_mallory","request_id":"r1"}"#,
        );
        assert_eq!(response.status_for_test(), "403 Forbidden");
        assert!(
            response
                .body_text()
                .contains("production_body_supplied_actor")
        );
    }
}
