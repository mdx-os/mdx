use super::*;
use crate::harness_provider_policy::evaluate_harness_enterprise_pack;

impl<S: StorageProvider> MdxKernel<S> {
    pub fn run_harness_provider_gateway(
        &mut self,
        manifest: &HarnessRunManifest<'_>,
        request: &HarnessProviderGatewayRequest<'_>,
    ) -> Result<HarnessProviderGatewayReport, HarnessProviderGatewayError> {
        admit_harness_run_manifest(manifest).map_err(|rejection| {
            HarnessProviderGatewayError::ManifestRejected(rejection.message())
        })?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(manifest.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(manifest.actor_id),
            loop_id: LoopId::new("harness_provider_gateway"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let harness_run_id = self.ids.next("harness_run");
        self.storage.push_loop_run(LoopRun {
            run_id: harness_run_id.clone(),
            loop_id: correlation.loop_id.clone(),
            agent_id: correlation.actor_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            status: "RUNNING".to_string(),
        });

        let requested_profile_id = request.requested_profile_id.trim();
        let requested_mode = request.requested_mode.trim();
        // The turn-on bridge: an accepted provider observation (recorded
        // through the governed Twin observation rail with full secret and
        // output safety) is the ONLY thing that can make a live profile
        // selectable here - and selection still places no call.
        let turn_on_observation = request
            .profiles
            .iter()
            .find(|profile| profile.profile_id == requested_profile_id)
            .and_then(|profile| self.accepted_turn_on_observation(&profile.provider_kind));
        let profile_result =
            evaluate_harness_provider_profile(manifest, request, turn_on_observation.is_some());

        let (
            status,
            denied_reason,
            selected_profile_id,
            provider_kind,
            model_id,
            policy_profile,
            enterprise_pack,
            receipt,
            policy_decision,
        ) = match profile_result {
            Ok(profile) => self.select_harness_provider_profile(
                manifest,
                requested_mode,
                &correlation,
                &harness_run_id,
                profile,
                turn_on_observation.as_deref(),
            ),
            Err(reason) => self.deny_harness_provider_profile(
                manifest,
                request,
                &correlation,
                &harness_run_id,
                reason,
            ),
        };

        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == harness_run_id)
        {
            run.status = status.clone();
        }
        self.storage
            .ledger()
            .verify()
            .map_err(HarnessProviderGatewayError::ManifestRejected)?;

        let mut transcript = HarnessRunTranscript::new(manifest.run_id, &harness_run_id);
        transcript.append_item(
            if denied_reason.is_some() {
                HarnessRunItemKind::ProviderDenied
            } else {
                HarnessRunItemKind::ProviderProfile
            },
            receipt.receipt_id.clone(),
            if denied_reason.is_some() {
                "provider profile selection failed closed"
            } else {
                "deterministic provider profile selected without live call"
            },
        );

        Ok(HarnessProviderGatewayReport {
            manifest_run_id: manifest.run_id.to_string(),
            harness_run_id,
            status,
            requested_profile_id: requested_profile_id.to_string(),
            requested_mode: requested_mode.to_string(),
            selected_profile_id,
            provider_kind,
            model_id,
            policy_profile,
            enterprise_pack,
            live_provider_call_allowed: false,
            denied_reason,
            receipt_id: receipt.receipt_id,
            policy_decision_id: policy_decision.policy_decision_id,
            transcript,
        })
    }

    // The accepted turn-on observation for a provider kind, if one exists:
    // a twin.model_gateway.provider.observed receipt that the governed rail
    // accepted (provider_connected_local_allowed=true) for this provider.
    fn accepted_turn_on_observation(&self, kind: &HarnessProviderKind) -> Option<String> {
        let provider_id = turn_on_provider_id(kind)?;
        self.storage
            .ledger()
            .entries()
            .iter()
            .rev()
            .find(|receipt| {
                receipt.kind == "twin.model_gateway.provider.observed"
                    && receipt.payload.get("provider_id").map(String::as_str) == Some(provider_id)
                    && receipt
                        .payload
                        .get("provider_connected_local_allowed")
                        .map(String::as_str)
                        == Some("true")
            })
            .map(|receipt| receipt.receipt_id.clone())
    }

    fn select_harness_provider_profile(
        &mut self,
        manifest: &HarnessRunManifest<'_>,
        requested_mode: &str,
        correlation: &CorrelationIds,
        harness_run_id: &str,
        profile: &HarnessProviderProfile<'_>,
        turn_on_observation: Option<&str>,
    ) -> HarnessProviderGatewayTransition {
        let decision =
            self.decide_with_receipt(correlation, ActionKind::SelectHarnessProviderProfile);
        let max_context_tokens = profile.max_context_tokens.to_string();
        let max_output_tokens = profile.max_output_tokens.to_string();
        let max_cost_cents = profile.max_cost_cents.to_string();
        let observation_backed = profile.provider_kind.is_live() && turn_on_observation.is_some();
        let receipt = self.transition_receipt(
            harness_run_id,
            "PROVIDER_PROFILE_SELECTED",
            correlation,
            &decision,
            "harness.provider.profile.selected",
            payload(&[
                ("manifest_run_id", manifest.run_id),
                ("profile_id", profile.profile_id),
                ("requested_mode", requested_mode),
                ("provider_kind", profile.provider_kind.as_str()),
                ("model_id", profile.model_id),
                ("max_context_tokens", &max_context_tokens),
                ("max_output_tokens", &max_output_tokens),
                ("max_cost_cents", &max_cost_cents),
                ("data_retention", profile.data_retention),
                ("policy_profile", profile.policy_profile),
                ("enterprise_pack", profile.enterprise_pack),
                ("provider_fail_closed", "true"),
                // Selection never places a call - call authority is the
                // plan-execute loop's own later slice, even for an
                // observation-backed live profile.
                ("live_provider_call_allowed", "false"),
                (
                    "provider_connected_observed",
                    if observation_backed { "true" } else { "false" },
                ),
                (
                    "turn_on_observation_receipt_id",
                    turn_on_observation.unwrap_or(""),
                ),
            ]),
        );
        (
            "PROFILE_SELECTED".to_string(),
            None,
            Some(profile.profile_id.to_string()),
            Some(profile.provider_kind.as_str().to_string()),
            Some(profile.model_id.to_string()),
            Some(profile.policy_profile.to_string()),
            Some(profile.enterprise_pack.to_string()),
            receipt,
            decision,
        )
    }

    fn deny_harness_provider_profile(
        &mut self,
        manifest: &HarnessRunManifest<'_>,
        request: &HarnessProviderGatewayRequest<'_>,
        correlation: &CorrelationIds,
        harness_run_id: &str,
        reason: &'static str,
    ) -> HarnessProviderGatewayTransition {
        let decision =
            self.decide_with_receipt(correlation, ActionKind::DenyHarnessProviderProfile);
        let requested_profile_id = request.requested_profile_id.trim();
        let requested_mode = request.requested_mode.trim();
        let profile = request
            .profiles
            .iter()
            .find(|profile| profile.profile_id == requested_profile_id);
        let provider_kind = profile.map(|profile| profile.provider_kind.as_str());
        let model_id = profile.map(|profile| profile.model_id);
        let policy_profile = profile.map(|profile| profile.policy_profile);
        let enterprise_pack = profile.map(|profile| profile.enterprise_pack);
        let provider_fail_closed = manifest.provider_fail_closed.to_string();
        let manifest_policy_profile = manifest.policy_profile.unwrap_or("unspecified");
        let manifest_enterprise_pack = manifest.enterprise_pack.unwrap_or("unspecified");
        let receipt = self.transition_receipt(
            harness_run_id,
            "PROVIDER_PROFILE_DENIED",
            correlation,
            &decision,
            "harness.provider.profile.denied",
            payload(&[
                ("manifest_run_id", manifest.run_id),
                ("requested_profile_id", requested_profile_id),
                ("requested_mode", requested_mode),
                ("reason", reason),
                ("provider_kind", provider_kind.unwrap_or("unknown")),
                ("model_id", model_id.unwrap_or("unknown")),
                ("policy_profile", policy_profile.unwrap_or("unknown")),
                ("enterprise_pack", enterprise_pack.unwrap_or("unknown")),
                ("manifest_policy_profile", manifest_policy_profile),
                ("manifest_enterprise_pack", manifest_enterprise_pack),
                ("manifest_provider_fail_closed", &provider_fail_closed),
                ("live_provider_call_allowed", "false"),
            ]),
        );
        (
            "PROFILE_DENIED".to_string(),
            Some(reason.to_string()),
            None,
            provider_kind.map(str::to_string),
            model_id.map(str::to_string),
            policy_profile.map(str::to_string),
            enterprise_pack.map(str::to_string),
            receipt,
            decision,
        )
    }
}

type HarnessProviderGatewayTransition = (
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Receipt,
    PolicyDecision,
);

// Map a harness provider kind to its turn-on slot id on the observation
// rail. The deterministic stub has no slot - it never needs evidence.
fn turn_on_provider_id(kind: &HarnessProviderKind) -> Option<&'static str> {
    match kind {
        HarnessProviderKind::DeterministicStub => None,
        HarnessProviderKind::OpenAiModelGateway => Some("openai"),
        HarnessProviderKind::AnthropicModelGateway => Some("anthropic"),
        HarnessProviderKind::XaiModelGateway => Some("xai"),
        HarnessProviderKind::GeminiModelGateway => Some("gemini"),
        HarnessProviderKind::MistralModelGateway => Some("mistral"),
        HarnessProviderKind::TensorZeroModelGateway => Some("tensorzero"),
    }
}

fn evaluate_harness_provider_profile<'request, 'profile>(
    manifest: &HarnessRunManifest<'_>,
    request: &'request HarnessProviderGatewayRequest<'profile>,
    turn_on_observed: bool,
) -> Result<&'request HarnessProviderProfile<'profile>, &'static str> {
    let requested_profile_id = request.requested_profile_id.trim();
    let requested_mode = request.requested_mode.trim();
    if requested_profile_id.is_empty() {
        return Err("missing_requested_profile");
    }
    if requested_mode.is_empty() {
        return Err("missing_requested_mode");
    }
    if requested_mode != manifest.mode.as_str() {
        return Err("manifest_mode_mismatch");
    }
    let profile = request
        .profiles
        .iter()
        .find(|profile| profile.profile_id == requested_profile_id)
        .ok_or("missing_profile")?;
    if !profile.enabled {
        return Err("profile_disabled");
    }
    if profile.profile_id.trim().is_empty() {
        return Err("missing_profile_id");
    }
    if profile.model_id.trim().is_empty() {
        return Err("missing_model_id");
    }
    if profile.allowed_modes.is_empty()
        || profile
            .allowed_modes
            .iter()
            .any(|mode| mode.trim().is_empty())
    {
        return Err("missing_allowed_modes");
    }
    if !profile.allowed_modes.contains(&requested_mode) {
        return Err("mode_not_allowed");
    }
    if !manifest
        .allowed_model_profiles
        .contains(&profile.profile_id)
    {
        return Err("profile_not_allowed_by_manifest");
    }
    // The turn-on bridge: a live profile is selectable only when the
    // governed observation rail has ACCEPTED a turn-on observation for
    // EXACTLY this provider - evidence for one provider never unlocks
    // another. Selection is readiness, not call authority: it places no
    // call, so the manifest's network and secret walls stay closed here
    // exactly as the run admission requires. Live call authority is the
    // plan-execute loop's own later slice.
    if manifest.trust_boundary.network_allowed || manifest.trust_boundary.secrets_available {
        return Err("live_provider_access_blocked");
    }
    if profile.provider_kind.is_live() && !turn_on_observed {
        return Err("live_provider_turn_on_evidence_missing");
    }
    if !manifest.provider_fail_closed || !profile.provider_fail_closed {
        return Err("provider_fail_closed_required");
    }
    if !manifest.context_redaction_required || !profile.context_redaction_required {
        return Err("context_redaction_required");
    }
    if !manifest.telemetry_redaction_required || !profile.telemetry_redaction_required {
        return Err("telemetry_redaction_required");
    }
    if profile.max_context_tokens == 0 || profile.max_output_tokens == 0 {
        return Err("empty_profile_budget");
    }
    if profile.max_context_tokens > manifest.budget_policy.max_context_tokens {
        return Err("context_budget_exceeded");
    }
    if profile.max_output_tokens > manifest.budget_policy.max_output_tokens {
        return Err("output_budget_exceeded");
    }
    if profile.max_cost_cents > manifest.budget_policy.max_cost_cents {
        return Err("cost_budget_exceeded");
    }
    if profile.data_retention.trim().is_empty() {
        return Err("missing_data_retention");
    }
    if profile.policy_profile.trim().is_empty() {
        return Err("missing_policy_profile");
    }
    if profile.enterprise_pack.trim().is_empty() {
        return Err("missing_enterprise_pack");
    }
    if let Some(policy_profile) = manifest.policy_profile
        && policy_profile != profile.policy_profile
    {
        return Err("policy_profile_mismatch");
    }
    if let Some(enterprise_pack) = manifest.enterprise_pack
        && enterprise_pack != profile.enterprise_pack
    {
        return Err("enterprise_pack_mismatch");
    }
    evaluate_harness_enterprise_pack(manifest, request, profile)?;
    Ok(profile)
}
