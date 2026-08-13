use super::*;

fn stage(stage_id: &'static str, kind: &'static str) -> HarnessPipelineStage<'static> {
    HarnessPipelineStage {
        stage_id,
        kind,
        path: "docs/HARNESS-PROGRAMMABLE-PIPELINES.md",
        contents: "programmable pipelines\ntyped work over governed primitives",
        query: "pipelines",
        items: &["a", "b", "a"],
        allowed_roots: &["docs/"],
        max_output_bytes: 256,
    }
}

fn pipeline_request<'a>(
    stages: &'a [HarnessPipelineStage<'a>],
    profiles: &'a [HarnessProviderProfile<'a>],
    packs: &'a [HarnessEnterprisePack<'a>],
) -> HarnessPipelineRequest<'a> {
    HarnessPipelineRequest {
        pipeline_id: "pipeline_local_test",
        intent: "prove the governed pipeline runtime locally",
        provider: HarnessProviderGatewayRequest {
            requested_profile_id: "deterministic_stub",
            requested_mode: "plan_only",
            profiles,
            enterprise_packs: packs,
        },
        stages,
        max_stages: 8,
        max_tool_calls: 6,
        max_output_bytes: 4096,
        max_model_calls: 1,
        quality_sensors: &["contract_battery", "no_fake_green"],
    }
}

fn pipeline_manifest() -> HarnessRunManifest<'static> {
    let mut manifest = valid_harness_run_manifest();
    manifest.budget_policy.max_tool_calls = 6;
    manifest
}

#[test]
fn admitted_pipeline_mediates_runnable_stages_and_records_the_verdict() {
    let mut kernel = MdxKernel::boot_local();
    let stages = [
        stage("s1", "virtual_read"),
        stage("s2", "virtual_search"),
        stage("s3", "dedupe_rank_select"),
        stage("s4", "memory_recall"),
        stage("s5", "model_stage"),
    ];
    let profiles = [deterministic_harness_provider_profile()];
    let packs = [deterministic_harness_enterprise_pack()];
    let request = pipeline_request(&stages, &profiles, &packs);
    let verdict = LocalHarnessPipelineRuntime
        .run(&mut kernel, &pipeline_manifest(), &request)
        .expect("pipeline verdict");

    assert!(verdict.admitted);
    assert_eq!(verdict.status, "PIPELINE_COMPLETED_LOCAL");
    assert_eq!(verdict.stages_run, 5);
    assert_eq!(verdict.stages_denied, 0);
    assert_eq!(
        verdict.provider_profile_id.as_deref(),
        Some("deterministic_stub")
    );
    assert_eq!(verdict.model_id.as_deref(), Some("deterministic_local_v1"));
    assert_eq!(
        verdict.quality_sensors_requested,
        vec!["contract_battery", "no_fake_green"]
    );
    // The honest run passes no_fake_green and skips the external battery,
    // never silently - the skip is named, and nothing is blocked.
    assert!(!verdict.quality_blocked);
    assert_eq!(verdict.sensors_passed, 1);
    assert_eq!(verdict.sensors_failed, 0);
    assert_eq!(verdict.sensors_skipped, 1);
    let fake_green = verdict
        .sensor_outcomes
        .iter()
        .find(|sensor| sensor.sensor_id == "no_fake_green")
        .expect("no_fake_green sensor");
    assert_eq!(fake_green.status, "PASSED");
    assert_eq!(fake_green.authority, "never_advisory");
    let battery = verdict
        .sensor_outcomes
        .iter()
        .find(|sensor| sensor.sensor_id == "contract_battery")
        .expect("contract_battery sensor");
    assert_eq!(battery.status, "SKIPPED");
    assert!(battery.reason.contains("external check runner"));
    assert!(!verdict.live_provider_call_allowed);
    assert!(!verdict.patch_application_allowed);
    assert!(!verdict.shell_execution_allowed);
    // The verdict receipt is real and carries the closed authority flags.
    let receipt = kernel
        .ledger()
        .query()
        .by_id(&verdict.verdict_receipt_id)
        .cloned()
        .expect("verdict receipt");
    assert_eq!(receipt.kind, "harness.pipeline.verdict.recorded");
    assert_eq!(
        receipt
            .payload
            .get("shell_execution_allowed")
            .map(String::as_str),
        Some("false")
    );
    // The model stage names no prompt or output text - identity only.
    let model = verdict
        .outcomes
        .iter()
        .find(|outcome| outcome.kind == "model_stage")
        .expect("model outcome");
    assert!(model.output.contains("deterministic local model run"));
    assert!(model.executed);
    assert!(kernel.ledger().verify().is_ok());
}

#[test]
fn declared_not_runnable_stages_deny_naming_the_missing_authority() {
    let mut kernel = MdxKernel::boot_local();
    let stages = [
        stage("s1", "virtual_read"),
        stage("s2", "test_execution"),
        stage("s3", "ci_triage"),
    ];
    let profiles = [deterministic_harness_provider_profile()];
    let packs = [deterministic_harness_enterprise_pack()];
    let request = pipeline_request(&stages, &profiles, &packs);
    let verdict = LocalHarnessPipelineRuntime
        .run(&mut kernel, &pipeline_manifest(), &request)
        .expect("pipeline verdict");

    assert_eq!(verdict.status, "PIPELINE_COMPLETED_WITH_DENIALS");
    assert_eq!(verdict.stages_run, 1);
    assert_eq!(verdict.stages_denied, 2);
    let denied: Vec<&HarnessPipelineStageOutcome> = verdict
        .outcomes
        .iter()
        .filter(|outcome| outcome.status == "DENIED")
        .collect();
    assert!(
        denied[0]
            .denied_reason
            .as_deref()
            .unwrap()
            .contains("command execution authority is not proven")
    );
    assert!(
        denied[1]
            .denied_reason
            .as_deref()
            .unwrap()
            .contains("CI evidence ingestion is not proven")
    );
    // Denied stages are receipts too.
    let receipt = kernel
        .ledger()
        .query()
        .by_id(&denied[0].receipt_id)
        .cloned()
        .expect("denied receipt");
    assert_eq!(receipt.kind, "harness.pipeline.stage.denied");
}

#[test]
fn unknown_stage_kind_denies_the_whole_pipeline() {
    let mut kernel = MdxKernel::boot_local();
    let stages = [stage("s1", "python_sandbox")];
    let profiles = [deterministic_harness_provider_profile()];
    let packs = [deterministic_harness_enterprise_pack()];
    let request = pipeline_request(&stages, &profiles, &packs);
    let verdict = LocalHarnessPipelineRuntime
        .run(&mut kernel, &pipeline_manifest(), &request)
        .expect("pipeline verdict");

    assert!(!verdict.admitted);
    assert_eq!(verdict.status, "PIPELINE_DENIED");
    assert_eq!(verdict.stages_run, 0);
    assert_eq!(
        verdict.denied_reason.as_deref(),
        Some("unknown_stage_kind:python_sandbox")
    );
}

#[test]
fn tool_call_budget_denies_explicitly_never_silently() {
    let mut kernel = MdxKernel::boot_local();
    let stages = [
        stage("s1", "virtual_read"),
        stage("s2", "virtual_read"),
        stage("s3", "virtual_read"),
    ];
    let profiles = [deterministic_harness_provider_profile()];
    let packs = [deterministic_harness_enterprise_pack()];
    let mut request = pipeline_request(&stages, &profiles, &packs);
    request.max_tool_calls = 2;
    let verdict = LocalHarnessPipelineRuntime
        .run(&mut kernel, &pipeline_manifest(), &request)
        .expect("pipeline verdict");

    assert_eq!(verdict.stages_run, 2);
    assert_eq!(verdict.stages_denied, 1);
    let last = verdict.outcomes.last().expect("last outcome");
    assert_eq!(
        last.denied_reason.as_deref(),
        Some("tool_call_budget_exhausted")
    );
}

#[test]
fn blocked_paths_deny_through_the_same_tool_plane_walls() {
    let mut kernel = MdxKernel::boot_local();
    let mut secret = stage("s1", "virtual_read");
    secret.path = ".env";
    secret.allowed_roots = &[".env"];
    let stages = [secret];
    let profiles = [deterministic_harness_provider_profile()];
    let packs = [deterministic_harness_enterprise_pack()];
    let request = pipeline_request(&stages, &profiles, &packs);
    let verdict = LocalHarnessPipelineRuntime
        .run(&mut kernel, &pipeline_manifest(), &request)
        .expect("pipeline verdict");

    assert_eq!(verdict.stages_denied, 1);
    assert!(verdict.outcomes[0].output.is_empty());
}

#[test]
fn product_copy_quiet_fails_in_process_when_the_pipelines_own_copy_is_loud() {
    let mut kernel = MdxKernel::boot_local();
    let stages = [stage("s1", "virtual_read")];
    let profiles = [deterministic_harness_provider_profile()];
    let packs = [deterministic_harness_enterprise_pack()];
    let mut request = pipeline_request(&stages, &profiles, &packs);
    // The intent rides into the run's human-facing copy; a first-read
    // banned phrase there must trip the in-process screen.
    request.intent = "surface the receipt id to the operator on first read";
    request.quality_sensors = &["product_copy_quiet"];
    let verdict = LocalHarnessPipelineRuntime
        .run(&mut kernel, &pipeline_manifest(), &request)
        .expect("pipeline verdict");

    assert_eq!(verdict.sensors_failed, 1);
    assert!(!verdict.quality_blocked); // product_copy is deterministic, not never-advisory
    assert_eq!(verdict.status, "PIPELINE_COMPLETED_WITH_DENIALS");
    let copy = verdict
        .sensor_outcomes
        .iter()
        .find(|sensor| sensor.sensor_id == "product_copy_quiet")
        .expect("product_copy_quiet sensor");
    assert_eq!(copy.status, "FAILED");
    assert!(copy.reason.contains("receipt id"));
}

#[test]
fn never_advisory_external_sensor_defers_visibly_never_silently() {
    let mut kernel = MdxKernel::boot_local();
    let stages = [stage("s1", "virtual_read")];
    let profiles = [deterministic_harness_provider_profile()];
    let packs = [deterministic_harness_enterprise_pack()];
    let mut request = pipeline_request(&stages, &profiles, &packs);
    request.quality_sensors = &["security_invariants", "made_up_sensor"];
    let verdict = LocalHarnessPipelineRuntime
        .run(&mut kernel, &pipeline_manifest(), &request)
        .expect("pipeline verdict");

    assert_eq!(verdict.sensors_skipped, 2);
    assert_eq!(verdict.never_advisory_sensors_deferred, 1);
    assert!(!verdict.quality_blocked);
    let security = verdict
        .sensor_outcomes
        .iter()
        .find(|sensor| sensor.sensor_id == "security_invariants")
        .expect("security sensor");
    assert_eq!(security.status, "SKIPPED");
    assert_eq!(security.authority, "never_advisory");
    let unknown = verdict
        .sensor_outcomes
        .iter()
        .find(|sensor| sensor.sensor_id == "made_up_sensor")
        .expect("unknown sensor");
    assert!(
        unknown
            .reason
            .contains("not in the quality sensor registry")
    );
}

// A fake executor stands in for the server-layer live call so the kernel
// path is provable without real keys: it returns presence-only evidence,
// exactly the shape the real executor returns.
struct FakeLiveExecutor;
impl PipelineModelExecutor for FakeLiveExecutor {
    fn execute(&self, context: &PipelineModelStageContext<'_>) -> Option<PipelineModelExecution> {
        assert_eq!(context.provider_kind, "xai_model_gateway");
        Some(PipelineModelExecution {
            provider: "xai".to_string(),
            adapter: "XaiChatModelGateway".to_string(),
            model_id: context.model_id.to_string(),
            inference_id: "inf_fake_live".to_string(),
            input_tokens: 11,
            output_tokens: 7,
        })
    }
}

struct DecliningExecutor;
impl PipelineModelExecutor for DecliningExecutor {
    fn execute(&self, _context: &PipelineModelStageContext<'_>) -> Option<PipelineModelExecution> {
        None
    }
}

fn record_accepted_xai_observation(kernel: &mut MdxKernel) {
    let report = kernel
        .save_twin_model_gateway_provider_observation_local(TwinModelGatewayProviderObservation {
            tenant_id: "local_tenant",
            actor_id: "human:local_user",
            provider_id: "xai",
            adapter: "XaiChatModelGateway",
            receipt_kind: "xai.chat.observed",
            approval_receipt_id: "provider_turn_on_approval_001",
            evidence_file: ".mdx-local/provider-turn-on/xai-chat-observed.json",
            model_id: "grok-4.3",
            response_id: "resp_local_turn_on",
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
            total_tokens: 42,
        })
        .expect("observation accepted");
    assert!(report.accepted);
}

fn live_pipeline_request<'a>(
    stages: &'a [HarnessPipelineStage<'a>],
    profiles: &'a [HarnessProviderProfile<'a>],
    packs: &'a [HarnessEnterprisePack<'a>],
) -> HarnessPipelineRequest<'a> {
    HarnessPipelineRequest {
        pipeline_id: "pipeline_live_test",
        intent: "summarize the governed pipeline shape",
        provider: HarnessProviderGatewayRequest {
            requested_profile_id: "xai_live",
            requested_mode: "plan_only",
            profiles,
            enterprise_packs: packs,
        },
        stages,
        max_stages: 8,
        max_tool_calls: 6,
        max_output_bytes: 4096,
        max_model_calls: 1,
        quality_sensors: &["no_fake_green", "product_copy_quiet"],
    }
}

fn xai_live_profile() -> HarnessProviderProfile<'static> {
    HarnessProviderProfile {
        profile_id: "xai_live",
        provider_kind: HarnessProviderKind::XaiModelGateway,
        model_id: "grok-4.3",
        ..live_harness_provider_profile()
    }
}

#[test]
fn live_model_stage_executes_through_the_injected_executor_presence_only() {
    let mut kernel = MdxKernel::boot_local();
    record_accepted_xai_observation(&mut kernel);
    let profiles = [xai_live_profile()];
    let allowlist = ["xai_live"];
    let packs = [HarnessEnterprisePack {
        provider_profile_allowlist: &allowlist,
        data_retention: "provider_ephemeral",
        ..deterministic_harness_enterprise_pack()
    }];
    let stages = [stage("s1", "virtual_read"), stage("s2", "model_stage")];
    let request = live_pipeline_request(&stages, &profiles, &packs);
    let mut manifest = pipeline_manifest();
    manifest.allowed_model_profiles = &["xai_live"];

    let verdict = LocalHarnessPipelineRuntime
        .run_with_executor(&mut kernel, &manifest, &request, &FakeLiveExecutor)
        .expect("live pipeline verdict");

    assert!(verdict.admitted);
    assert!(verdict.live_provider_call_executed);
    // The kernel itself never opens call authority - the executor did, gated.
    assert!(!verdict.live_provider_call_allowed);
    let model = verdict
        .outcomes
        .iter()
        .find(|outcome| outcome.kind == "model_stage")
        .expect("model outcome");
    assert_eq!(model.status, "MEDIATED");
    assert!(model.live_provider_call);
    assert!(model.output.contains("live model answered"));
    assert!(model.output.contains("grok-4.3"));
    // The verdict still passes no_fake_green: the five forbidden flags stay
    // closed; a real call through the governed executor is not fake green.
    assert!(!verdict.quality_blocked);
    let fake_green = verdict
        .sensor_outcomes
        .iter()
        .find(|sensor| sensor.sensor_id == "no_fake_green")
        .expect("no_fake_green sensor");
    assert_eq!(fake_green.status, "PASSED");
    // The receipt is presence-only: provider and usage, never output text.
    let receipt = kernel
        .ledger()
        .query()
        .by_id(&model.receipt_id)
        .cloned()
        .expect("live model receipt");
    assert_eq!(
        receipt
            .payload
            .get("output_text_recorded")
            .map(String::as_str),
        Some("false")
    );
    assert_eq!(
        receipt.payload.get("provider").map(String::as_str),
        Some("xai")
    );
    assert_eq!(
        receipt
            .payload
            .get("live_provider_call_executed")
            .map(String::as_str),
        Some("true")
    );
    assert!(kernel.ledger().verify().is_ok());
}

#[test]
fn live_profile_holds_when_the_executor_declines() {
    let mut kernel = MdxKernel::boot_local();
    record_accepted_xai_observation(&mut kernel);
    let profiles = [xai_live_profile()];
    let allowlist = ["xai_live"];
    let packs = [HarnessEnterprisePack {
        provider_profile_allowlist: &allowlist,
        data_retention: "provider_ephemeral",
        ..deterministic_harness_enterprise_pack()
    }];
    let stages = [stage("s1", "model_stage")];
    let request = live_pipeline_request(&stages, &profiles, &packs);
    let mut manifest = pipeline_manifest();
    manifest.allowed_model_profiles = &["xai_live"];

    let verdict = LocalHarnessPipelineRuntime
        .run_with_executor(&mut kernel, &manifest, &request, &DecliningExecutor)
        .expect("held pipeline verdict");

    assert!(!verdict.live_provider_call_executed);
    let model = verdict
        .outcomes
        .iter()
        .find(|outcome| outcome.kind == "model_stage")
        .expect("model outcome");
    assert_eq!(model.status, "HELD");
    assert!(!model.live_provider_call);
}

#[test]
fn empty_pipeline_and_empty_budgets_deny_at_admission() {
    let mut kernel = MdxKernel::boot_local();
    let profiles = [deterministic_harness_provider_profile()];
    let packs = [deterministic_harness_enterprise_pack()];
    let request = pipeline_request(&[], &profiles, &packs);
    let verdict = LocalHarnessPipelineRuntime
        .run(&mut kernel, &pipeline_manifest(), &request)
        .expect("pipeline verdict");
    assert_eq!(verdict.denied_reason.as_deref(), Some("empty_pipeline"));

    let stages = [stage("s1", "virtual_read")];
    let mut starved = pipeline_request(&stages, &profiles, &packs);
    starved.max_tool_calls = 0;
    let verdict = LocalHarnessPipelineRuntime
        .run(&mut kernel, &pipeline_manifest(), &starved)
        .expect("pipeline verdict");
    assert_eq!(
        verdict.denied_reason.as_deref(),
        Some("empty_pipeline_budget")
    );
}

#[test]
fn a_live_model_stage_suspends_instead_of_calling_under_the_lock() {
    // The suspend-for-external-call contract, driven explicitly: begin
    // yields a prepared call (no network, no executor), the caller makes
    // the call on its own authority, resume records the result and the
    // verdict matches the inline era exactly.
    let mut kernel = MdxKernel::boot_local();
    record_accepted_xai_observation(&mut kernel);
    let profiles = [xai_live_profile()];
    let allowlist = ["xai_live"];
    let packs = [HarnessEnterprisePack {
        provider_profile_allowlist: &allowlist,
        data_retention: "provider_ephemeral",
        ..deterministic_harness_enterprise_pack()
    }];
    let stages = [stage("s1", "virtual_read"), stage("s2", "model_stage")];
    let request = live_pipeline_request(&stages, &profiles, &packs);
    let mut manifest = pipeline_manifest();
    manifest.allowed_model_profiles = &["xai_live"];

    let step = kernel
        .begin_harness_pipeline(&manifest, &request)
        .expect("pipeline begins");
    let HarnessPipelineStep::NeedsModelCall { state, prepared } = step else {
        panic!("a live model stage must suspend the run");
    };
    assert_eq!(prepared.provider_kind, "xai_model_gateway");
    assert_eq!(prepared.model_id, "grok-4.3");
    assert!(!prepared.profile_id.is_empty());
    assert!(!prepared.gateway_receipt_id.is_empty());

    // The caller's call, made with no kernel access at all.
    let execution = PipelineModelExecution {
        provider: "xai".to_string(),
        adapter: "openai_compatible".to_string(),
        model_id: prepared.model_id.clone(),
        inference_id: "inf_suspend_proof".to_string(),
        input_tokens: 11,
        output_tokens: 7,
    };
    let step = kernel
        .resume_harness_pipeline(state, &manifest, &request, Some(execution))
        .expect("pipeline resumes");
    let HarnessPipelineStep::Complete(verdict) = step else {
        panic!("one model stage means one suspension");
    };
    assert!(verdict.live_provider_call_executed);
    let model = verdict
        .outcomes
        .iter()
        .find(|outcome| outcome.kind == "model_stage")
        .expect("model outcome");
    assert_eq!(model.status, "MEDIATED");
    assert!(model.output.contains("inf_suspend_proof"));
    assert!(kernel.ledger().verify().is_ok());
}

#[test]
fn resuming_with_a_mismatched_request_is_refused() {
    let mut kernel = MdxKernel::boot_local();
    record_accepted_xai_observation(&mut kernel);
    let profiles = [xai_live_profile()];
    let allowlist = ["xai_live"];
    let packs = [HarnessEnterprisePack {
        provider_profile_allowlist: &allowlist,
        data_retention: "provider_ephemeral",
        ..deterministic_harness_enterprise_pack()
    }];
    let stages = [stage("s2", "model_stage")];
    let request = live_pipeline_request(&stages, &profiles, &packs);
    let mut manifest = pipeline_manifest();
    manifest.allowed_model_profiles = &["xai_live"];

    let step = kernel
        .begin_harness_pipeline(&manifest, &request)
        .expect("pipeline begins");
    let HarnessPipelineStep::NeedsModelCall { state, .. } = step else {
        panic!("the live stage must suspend");
    };
    // A different request shape must refuse the resume - a suspended run
    // belongs to the request it began with.
    let other_stages = [stage("s1", "virtual_read"), stage("s2", "model_stage")];
    let other = live_pipeline_request(&other_stages, &profiles, &packs);
    let error = kernel
        .resume_harness_pipeline(state, &manifest, &other, None)
        .expect_err("mismatched resume must refuse");
    assert!(error.message().contains("resume request mismatch"));
}

#[test]
fn resuming_with_none_holds_the_stage_honestly() {
    let mut kernel = MdxKernel::boot_local();
    record_accepted_xai_observation(&mut kernel);
    let profiles = [xai_live_profile()];
    let allowlist = ["xai_live"];
    let packs = [HarnessEnterprisePack {
        provider_profile_allowlist: &allowlist,
        data_retention: "provider_ephemeral",
        ..deterministic_harness_enterprise_pack()
    }];
    let stages = [stage("s2", "model_stage")];
    let request = live_pipeline_request(&stages, &profiles, &packs);
    let mut manifest = pipeline_manifest();
    manifest.allowed_model_profiles = &["xai_live"];

    let step = kernel
        .begin_harness_pipeline(&manifest, &request)
        .expect("pipeline begins");
    let HarnessPipelineStep::NeedsModelCall { state, .. } = step else {
        panic!("the live stage must suspend");
    };
    let step = kernel
        .resume_harness_pipeline(state, &manifest, &request, None)
        .expect("pipeline resumes held");
    let HarnessPipelineStep::Complete(verdict) = step else {
        panic!("the run must complete");
    };
    assert!(!verdict.live_provider_call_executed);
    assert_eq!(verdict.stages_held, 1);
}
