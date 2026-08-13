use super::*;

fn receipt_by_id<'a>(kernel: &'a MdxKernel, receipt_id: &str) -> &'a Receipt {
    kernel.ledger().query().by_id(receipt_id).expect("receipt")
}

fn provider_request<'a>(
    profile_id: &'a str,
    profiles: &'a [HarnessProviderProfile<'a>],
    enterprise_packs: &'a [HarnessEnterprisePack<'a>],
) -> HarnessProviderGatewayRequest<'a> {
    HarnessProviderGatewayRequest {
        requested_profile_id: profile_id,
        requested_mode: "plan_only",
        profiles,
        enterprise_packs,
    }
}

#[test]
fn local_harness_provider_gateway_selects_deterministic_profile_without_live_call() {
    let mut kernel = MdxKernel::boot_local();
    let manifest = valid_harness_run_manifest();
    let profiles = [deterministic_harness_provider_profile()];
    let enterprise_packs = [deterministic_harness_enterprise_pack()];
    let request = provider_request("deterministic_stub", &profiles, &enterprise_packs);

    let report = LocalHarnessProviderGateway
        .select_profile(&mut kernel, &manifest, &request)
        .expect("provider gateway report");

    assert_eq!(report.status, "PROFILE_SELECTED");
    assert_eq!(
        report.selected_profile_id.as_deref(),
        Some("deterministic_stub")
    );
    assert_eq!(report.provider_kind.as_deref(), Some("deterministic_stub"));
    assert_eq!(report.model_id.as_deref(), Some("deterministic_local_v1"));
    assert_eq!(report.policy_profile.as_deref(), Some("core"));
    assert_eq!(report.enterprise_pack.as_deref(), Some("none"));
    assert!(!report.live_provider_call_allowed);
    assert_eq!(report.denied_reason, None);
    assert_eq!(
        report.transcript.items()[0].kind,
        HarnessRunItemKind::ProviderProfile
    );
    let receipt = receipt_by_id(&kernel, &report.receipt_id);
    assert_eq!(receipt.kind, "harness.provider.profile.selected");
    assert_eq!(
        receipt
            .payload
            .get("live_provider_call_allowed")
            .map(String::as_str),
        Some("false")
    );
    assert!(kernel.ledger().verify().is_ok());
}

#[test]
fn local_harness_provider_gateway_denies_missing_profile_with_receipt() {
    let mut kernel = MdxKernel::boot_local();
    let manifest = valid_harness_run_manifest();
    let profiles = [deterministic_harness_provider_profile()];
    let enterprise_packs = [deterministic_harness_enterprise_pack()];
    let request = provider_request("missing_profile", &profiles, &enterprise_packs);

    let report = LocalHarnessProviderGateway
        .select_profile(&mut kernel, &manifest, &request)
        .expect("provider gateway denial report");

    assert_eq!(report.status, "PROFILE_DENIED");
    assert_eq!(report.selected_profile_id, None);
    assert_eq!(report.denied_reason.as_deref(), Some("missing_profile"));
    assert!(!report.live_provider_call_allowed);
    assert_eq!(
        report.transcript.items()[0].kind,
        HarnessRunItemKind::ProviderDenied
    );
    let receipt = receipt_by_id(&kernel, &report.receipt_id);
    assert_eq!(receipt.kind, "harness.provider.profile.denied");
    assert!(kernel.ledger().verify().is_ok());
}

#[test]
fn local_harness_provider_gateway_denies_enterprise_pack_mismatch_with_receipt() {
    let mut kernel = MdxKernel::boot_local();
    let mut manifest = valid_harness_run_manifest();
    manifest.enterprise_pack = Some("regulated_enterprise_pack");
    let profiles = [deterministic_harness_provider_profile()];
    let enterprise_packs = [deterministic_harness_enterprise_pack()];
    let request = provider_request("deterministic_stub", &profiles, &enterprise_packs);

    let report = LocalHarnessProviderGateway
        .select_profile(&mut kernel, &manifest, &request)
        .expect("provider gateway enterprise pack denial report");

    assert_eq!(report.status, "PROFILE_DENIED");
    assert_eq!(
        report.denied_reason.as_deref(),
        Some("enterprise_pack_mismatch")
    );
    assert_eq!(report.enterprise_pack.as_deref(), Some("none"));
    assert!(!report.live_provider_call_allowed);
    let receipt = receipt_by_id(&kernel, &report.receipt_id);
    assert_eq!(receipt.kind, "harness.provider.profile.denied");
    assert_eq!(
        receipt.payload.get("reason").map(String::as_str),
        Some("enterprise_pack_mismatch")
    );
    assert_eq!(
        receipt
            .payload
            .get("manifest_enterprise_pack")
            .map(String::as_str),
        Some("regulated_enterprise_pack")
    );
    assert!(kernel.ledger().verify().is_ok());
}

#[test]
fn local_harness_provider_gateway_denies_profile_outside_enterprise_pack_allowlist() {
    let mut kernel = MdxKernel::boot_local();
    let manifest = valid_harness_run_manifest();
    let profiles = [deterministic_harness_provider_profile()];
    let enterprise_packs = [HarnessEnterprisePack {
        enterprise_pack_id: "none",
        allowed_policy_profiles: &["core"],
        provider_profile_allowlist: &["another_profile"],
        ..deterministic_harness_enterprise_pack()
    }];
    let request = provider_request("deterministic_stub", &profiles, &enterprise_packs);

    let report = LocalHarnessProviderGateway
        .select_profile(&mut kernel, &manifest, &request)
        .expect("provider gateway enterprise pack allowlist denial report");

    assert_eq!(report.status, "PROFILE_DENIED");
    assert_eq!(
        report.denied_reason.as_deref(),
        Some("enterprise_pack_profile_not_allowed")
    );
    assert!(!report.live_provider_call_allowed);
    let receipt = receipt_by_id(&kernel, &report.receipt_id);
    assert_eq!(receipt.kind, "harness.provider.profile.denied");
    assert_eq!(
        receipt.payload.get("reason").map(String::as_str),
        Some("enterprise_pack_profile_not_allowed")
    );
    assert!(kernel.ledger().verify().is_ok());
}

#[test]
fn local_harness_provider_gateway_denies_profile_budget_above_enterprise_pack() {
    let mut kernel = MdxKernel::boot_local();
    let manifest = valid_harness_run_manifest();
    let profiles = [deterministic_harness_provider_profile()];
    let enterprise_packs = [HarnessEnterprisePack {
        max_context_tokens: 2048,
        ..deterministic_harness_enterprise_pack()
    }];
    let request = provider_request("deterministic_stub", &profiles, &enterprise_packs);

    let report = LocalHarnessProviderGateway
        .select_profile(&mut kernel, &manifest, &request)
        .expect("provider gateway enterprise pack budget denial report");

    assert_eq!(report.status, "PROFILE_DENIED");
    assert_eq!(
        report.denied_reason.as_deref(),
        Some("enterprise_pack_context_budget_exceeded")
    );
    assert!(!report.live_provider_call_allowed);
    let receipt = receipt_by_id(&kernel, &report.receipt_id);
    assert_eq!(receipt.kind, "harness.provider.profile.denied");
    assert_eq!(
        receipt.payload.get("reason").map(String::as_str),
        Some("enterprise_pack_context_budget_exceeded")
    );
    assert!(kernel.ledger().verify().is_ok());
}

#[test]
fn local_harness_provider_gateway_denies_audit_export_enabled_enterprise_pack() {
    let mut kernel = MdxKernel::boot_local();
    let manifest = valid_harness_run_manifest();
    let profiles = [deterministic_harness_provider_profile()];
    let enterprise_packs = [HarnessEnterprisePack {
        audit_export_allowed: true,
        ..deterministic_harness_enterprise_pack()
    }];
    let request = provider_request("deterministic_stub", &profiles, &enterprise_packs);

    let report = LocalHarnessProviderGateway
        .select_profile(&mut kernel, &manifest, &request)
        .expect("provider gateway audit export denial report");

    assert_eq!(report.status, "PROFILE_DENIED");
    assert_eq!(
        report.denied_reason.as_deref(),
        Some("enterprise_pack_audit_export_not_local")
    );
    assert!(!report.live_provider_call_allowed);
    let receipt = receipt_by_id(&kernel, &report.receipt_id);
    assert_eq!(receipt.kind, "harness.provider.profile.denied");
    assert_eq!(
        receipt.payload.get("reason").map(String::as_str),
        Some("enterprise_pack_audit_export_not_local")
    );
    assert!(kernel.ledger().verify().is_ok());
}

#[test]
fn local_harness_provider_gateway_denies_live_profiles_without_provider_call() {
    let mut kernel = MdxKernel::boot_local();
    let mut manifest = valid_harness_run_manifest();
    manifest.allowed_model_profiles = &["tensorzero_live"];
    let profiles = [live_harness_provider_profile()];
    let enterprise_packs = [deterministic_harness_enterprise_pack()];
    let request = provider_request("tensorzero_live", &profiles, &enterprise_packs);

    let report = LocalHarnessProviderGateway
        .select_profile(&mut kernel, &manifest, &request)
        .expect("provider gateway live denial report");

    assert_eq!(report.status, "PROFILE_DENIED");
    assert_eq!(
        report.provider_kind.as_deref(),
        Some("tensorzero_model_gateway")
    );
    assert_eq!(
        report.denied_reason.as_deref(),
        Some("live_provider_turn_on_evidence_missing")
    );
    assert!(!report.live_provider_call_allowed);
    let receipt = receipt_by_id(&kernel, &report.receipt_id);
    assert_eq!(receipt.kind, "harness.provider.profile.denied");
    assert_eq!(
        receipt
            .payload
            .get("live_provider_call_allowed")
            .map(String::as_str),
        Some("false")
    );
    assert!(kernel.ledger().verify().is_ok());
}

#[test]
fn local_harness_provider_gateway_denies_each_live_adapter_profile_without_provider_call() {
    for (profile_id, provider_kind, expected_kind) in [
        (
            "openai_live",
            HarnessProviderKind::OpenAiModelGateway,
            "openai_model_gateway",
        ),
        (
            "anthropic_live",
            HarnessProviderKind::AnthropicModelGateway,
            "anthropic_model_gateway",
        ),
        (
            "xai_live",
            HarnessProviderKind::XaiModelGateway,
            "xai_model_gateway",
        ),
        (
            "gemini_live",
            HarnessProviderKind::GeminiModelGateway,
            "gemini_model_gateway",
        ),
        (
            "mistral_live",
            HarnessProviderKind::MistralModelGateway,
            "mistral_model_gateway",
        ),
        (
            "tensorzero_live",
            HarnessProviderKind::TensorZeroModelGateway,
            "tensorzero_model_gateway",
        ),
    ] {
        let mut kernel = MdxKernel::boot_local();
        let mut manifest = valid_harness_run_manifest();
        let allowed_profiles = [profile_id];
        manifest.allowed_model_profiles = &allowed_profiles;
        let profile = HarnessProviderProfile {
            profile_id,
            provider_kind,
            model_id: "policy_selected",
            allowed_modes: &["plan_only"],
            max_context_tokens: 4096,
            max_output_tokens: 1024,
            max_cost_cents: 0,
            data_retention: "provider_ephemeral",
            context_redaction_required: true,
            telemetry_redaction_required: true,
            provider_fail_closed: true,
            policy_profile: "core",
            enterprise_pack: "none",
            enabled: true,
        };
        let profiles = [profile];
        let provider_profile_allowlist = [profile_id];
        let enterprise_packs = [HarnessEnterprisePack {
            provider_profile_allowlist: &provider_profile_allowlist,
            data_retention: "provider_ephemeral",
            ..deterministic_harness_enterprise_pack()
        }];
        let request = provider_request(profile_id, &profiles, &enterprise_packs);

        let report = LocalHarnessProviderGateway
            .select_profile(&mut kernel, &manifest, &request)
            .expect("provider gateway live adapter denial report");

        assert_eq!(report.status, "PROFILE_DENIED");
        assert_eq!(report.provider_kind.as_deref(), Some(expected_kind));
        assert_eq!(
            report.denied_reason.as_deref(),
            Some("live_provider_turn_on_evidence_missing")
        );
        assert!(!report.live_provider_call_allowed);
        let receipt = receipt_by_id(&kernel, &report.receipt_id);
        assert_eq!(receipt.kind, "harness.provider.profile.denied");
        assert_eq!(
            receipt.payload.get("provider_kind").map(String::as_str),
            Some(expected_kind)
        );
        assert_eq!(
            receipt
                .payload
                .get("live_provider_call_allowed")
                .map(String::as_str),
            Some("false")
        );
        assert!(kernel.ledger().verify().is_ok());
    }
}

// The turn-on bridge: an ACCEPTED observation through the governed rail
// plus an explicit manifest network grant makes a live profile
// selectable - and selection still places no call.
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
            model_id: "grok-4-fast-non-reasoning",
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

fn xai_live_profile() -> HarnessProviderProfile<'static> {
    HarnessProviderProfile {
        profile_id: "xai_live",
        provider_kind: HarnessProviderKind::XaiModelGateway,
        model_id: "grok-4-fast-non-reasoning",
        ..live_harness_provider_profile()
    }
}

#[test]
fn observation_backed_live_profile_selects_without_call_authority() {
    let mut kernel = MdxKernel::boot_local();
    record_accepted_xai_observation(&mut kernel);
    let mut manifest = valid_harness_run_manifest();
    manifest.allowed_model_profiles = &["xai_live"];
    let profiles = [xai_live_profile()];
    let provider_profile_allowlist = ["xai_live"];
    let enterprise_packs = [HarnessEnterprisePack {
        provider_profile_allowlist: &provider_profile_allowlist,
        data_retention: "provider_ephemeral",
        ..deterministic_harness_enterprise_pack()
    }];
    let request = provider_request("xai_live", &profiles, &enterprise_packs);

    let report = LocalHarnessProviderGateway
        .select_profile(&mut kernel, &manifest, &request)
        .expect("observation-backed selection");

    assert_eq!(report.status, "PROFILE_SELECTED");
    assert_eq!(report.provider_kind.as_deref(), Some("xai_model_gateway"));
    assert_eq!(
        report.model_id.as_deref(),
        Some("grok-4-fast-non-reasoning")
    );
    // Selection is readiness, never call authority.
    assert!(!report.live_provider_call_allowed);
    let receipt = receipt_by_id(&kernel, &report.receipt_id);
    assert_eq!(receipt.kind, "harness.provider.profile.selected");
    assert_eq!(
        receipt
            .payload
            .get("provider_connected_observed")
            .map(String::as_str),
        Some("true")
    );
    assert!(
        receipt
            .payload
            .get("turn_on_observation_receipt_id")
            .is_some_and(|id| !id.is_empty())
    );
    assert_eq!(
        receipt
            .payload
            .get("live_provider_call_allowed")
            .map(String::as_str),
        Some("false")
    );
    assert!(kernel.ledger().verify().is_ok());
}

#[test]
fn observation_evidence_is_per_provider_never_global() {
    let mut kernel = MdxKernel::boot_local();
    // xAI is observed; the TensorZero live profile must stay locked.
    record_accepted_xai_observation(&mut kernel);
    let mut manifest = valid_harness_run_manifest();
    manifest.allowed_model_profiles = &["tensorzero_live"];
    let profiles = [live_harness_provider_profile()];
    let provider_profile_allowlist = ["tensorzero_live"];
    let enterprise_packs = [HarnessEnterprisePack {
        provider_profile_allowlist: &provider_profile_allowlist,
        data_retention: "provider_ephemeral",
        ..deterministic_harness_enterprise_pack()
    }];
    let request = provider_request("tensorzero_live", &profiles, &enterprise_packs);

    let report = LocalHarnessProviderGateway
        .select_profile(&mut kernel, &manifest, &request)
        .expect("denial report");

    assert_eq!(report.status, "PROFILE_DENIED");
    assert_eq!(
        report.denied_reason.as_deref(),
        Some("live_provider_turn_on_evidence_missing")
    );
    assert!(!report.live_provider_call_allowed);
}

#[test]
fn rejected_observation_never_unlocks_a_live_profile() {
    let mut kernel = MdxKernel::boot_local();
    // A rejected observation: output text recorded fails the safety posture.
    let report = kernel
        .save_twin_model_gateway_provider_observation_local(TwinModelGatewayProviderObservation {
            tenant_id: "local_tenant",
            actor_id: "human:local_user",
            provider_id: "xai",
            adapter: "XaiChatModelGateway",
            receipt_kind: "xai.chat.observed",
            approval_receipt_id: "provider_turn_on_approval_002",
            evidence_file: ".mdx-local/provider-turn-on/xai-chat-observed.json",
            model_id: "grok-4-fast-non-reasoning",
            response_id: "resp_unsafe",
            response_status: "completed",
            observed: true,
            provider_call_attempted: true,
            network_call_attempted: true,
            credential_presence_only: true,
            credential_values_recorded: false,
            provider_secret_values_recorded: false,
            requested_secret_values_recorded: false,
            output_text_recorded: true,
            production_write_allowed: false,
            total_tokens: 42,
        })
        .expect("observation recorded");
    assert!(!report.accepted);
    assert_eq!(report.denial_reason, "output_text_recording_rejected");

    let mut manifest = valid_harness_run_manifest();
    manifest.allowed_model_profiles = &["xai_live"];
    let profiles = [xai_live_profile()];
    let provider_profile_allowlist = ["xai_live"];
    let enterprise_packs = [HarnessEnterprisePack {
        provider_profile_allowlist: &provider_profile_allowlist,
        data_retention: "provider_ephemeral",
        ..deterministic_harness_enterprise_pack()
    }];
    let request = provider_request("xai_live", &profiles, &enterprise_packs);

    let gateway_report = LocalHarnessProviderGateway
        .select_profile(&mut kernel, &manifest, &request)
        .expect("denial report");
    assert_eq!(gateway_report.status, "PROFILE_DENIED");
    assert_eq!(
        gateway_report.denied_reason.as_deref(),
        Some("live_provider_turn_on_evidence_missing")
    );
}

#[test]
fn deterministic_fallback_survives_the_bridge() {
    let mut kernel = MdxKernel::boot_local();
    record_accepted_xai_observation(&mut kernel);
    let manifest = valid_harness_run_manifest();
    let profiles = [deterministic_harness_provider_profile()];
    let enterprise_packs = [deterministic_harness_enterprise_pack()];
    let request = provider_request(profiles[0].profile_id, &profiles, &enterprise_packs);

    let report = LocalHarnessProviderGateway
        .select_profile(&mut kernel, &manifest, &request)
        .expect("deterministic selection");
    assert_eq!(report.status, "PROFILE_SELECTED");
    assert!(!report.live_provider_call_allowed);
}
