// The programmable pipeline runtime: typed work over governed
// primitives, local and fail-closed. The model may propose the IR; this
// runtime owns admission, policy, provider profile selection, budgets,
// caps, receipts, and the verdict. Runnable stages mediate through the
// PROVEN primitives (the safe tool plane, the deterministic local model
// driver, pure transforms); declared-not-runnable stages deny with the
// missing authority named. A live model stage delegates to an injected
// PipelineModelExecutor - the kernel never opens the network itself; the
// executor holds the gated authority and may decline, holding the stage.
// No stage here opens shell, git, patch application, browser, MCP, worker,
// sandbox, deployment, or production-write authority.
use crate::*;

pub const PIPELINE_RUNNABLE_STAGE_KINDS: &[&str] = &[
    "virtual_read",
    "virtual_search",
    "virtual_list",
    "patch_preview",
    "memory_recall",
    "dedupe_rank_select",
    "model_stage",
];

pub const PIPELINE_NOT_RUNNABLE_STAGE_KINDS: &[(&str, &str)] = &[
    (
        "test_execution",
        "command execution authority is not proven - denied until a runtime slice proves it",
    ),
    (
        "ci_triage",
        "CI evidence ingestion is not proven - denied until observed CI evidence exists",
    ),
    (
        "failure_triage",
        "execution transcripts from real runs do not exist - denied until live execution exists",
    ),
    (
        "security_scan_execution",
        "command execution authority is not proven - discovery via read and search stays allowed",
    ),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HarnessPipelineStage<'a> {
    pub stage_id: &'a str,
    pub kind: &'a str,
    pub path: &'a str,
    pub contents: &'a str,
    pub query: &'a str,
    pub items: &'a [&'a str],
    pub allowed_roots: &'a [&'a str],
    pub max_output_bytes: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HarnessPipelineRequest<'a> {
    pub pipeline_id: &'a str,
    pub intent: &'a str,
    pub provider: HarnessProviderGatewayRequest<'a>,
    pub stages: &'a [HarnessPipelineStage<'a>],
    pub max_stages: usize,
    pub max_tool_calls: usize,
    pub max_output_bytes: usize,
    pub max_model_calls: usize,
    pub quality_sensors: &'a [&'a str],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HarnessPipelineStageOutcome {
    pub stage_id: String,
    pub kind: String,
    pub status: &'static str,
    pub summary: String,
    pub output: String,
    pub denied_reason: Option<String>,
    pub executed: bool,
    pub receipt_id: String,
    pub inner_receipt_id: String,
    /// True only when a real live provider call was made for this stage
    /// through the injected, separately-gated executor. The kernel itself
    /// never opens the network; this records that the governed executor did.
    pub live_provider_call: bool,
}

/// The context a model stage hands the injected executor. The intent is the
/// task to answer; it is the prompt and is never recorded in a receipt.
pub struct PipelineModelStageContext<'a> {
    pub profile_id: &'a str,
    pub provider_kind: &'a str,
    pub model_id: &'a str,
    pub intent: &'a str,
    pub max_output_tokens: u32,
}

/// What a live model call returns: presence-only evidence. No prompt text,
/// no output text, no secret value - identity and usage only, exactly like
/// the provider turn-on observation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PipelineModelExecution {
    pub provider: String,
    pub adapter: String,
    pub model_id: String,
    pub inference_id: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
}

/// The port the kernel delegates a live model stage to. The kernel never
/// makes the call itself (it stays network and secret closed); a server-
/// layer implementation holds the gated authority. Returning None falls the
/// stage back to held, honestly - never deterministic theater for a live ask.
pub trait PipelineModelExecutor {
    fn execute(&self, context: &PipelineModelStageContext<'_>) -> Option<PipelineModelExecution>;
}

/// The suspend-for-external-call contract (ADR 0481, the trigger ADR 0479
/// named): the kernel PREPARES a live model call under its lock, YIELDS
/// with everything the caller needs, and RESUMES with the result. The
/// kernel never opens the network and never waits on it - one slow
/// provider answer must never freeze every other surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedPipelineModelCall {
    pub profile_id: String,
    pub provider_kind: String,
    pub model_id: String,
    pub intent: String,
    pub max_output_tokens: u32,
    pub gateway_receipt_id: String,
}

/// Where a suspended run stands: it either needs the caller to make one
/// external call, or it is done. The run state moves by value through
/// the protocol, so a run cannot be resumed twice.
#[derive(Debug)]
pub enum HarnessPipelineStep {
    NeedsModelCall {
        state: Box<HarnessPipelineRunState>,
        prepared: PreparedPipelineModelCall,
    },
    Complete(Box<HarnessPipelineVerdict>),
}

/// Everything a suspended pipeline run owns. Opaque outside this module:
/// the caller holds it between prepare and resume and hands it back
/// untouched, with the SAME manifest and request (the fingerprint
/// refuses a mismatched resume).
#[derive(Debug)]
pub struct HarnessPipelineRunState {
    correlation: CorrelationIds,
    harness_run_id: String,
    receipt_ids: Vec<String>,
    outcomes: Vec<HarnessPipelineStageOutcome>,
    tool_calls_used: usize,
    model_calls_used: usize,
    prior_outputs: Vec<String>,
    next_stage: usize,
    gateway: Option<HarnessProviderGatewayReport>,
    admitted: bool,
    denial: Option<String>,
    proposed_receipt_id: String,
    admission_receipt_id: String,
    fingerprint: String,
    pending_model_stage: Option<usize>,
}

fn pipeline_fingerprint(request: &HarnessPipelineRequest<'_>) -> String {
    format!("{}:{}", request.pipeline_id, request.stages.len())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HarnessPipelineVerdict {
    pub pipeline_id: String,
    pub harness_run_id: String,
    pub status: &'static str,
    pub admitted: bool,
    pub denied_reason: Option<String>,
    pub stages_run: usize,
    pub stages_denied: usize,
    pub stages_held: usize,
    pub provider_profile_id: Option<String>,
    pub provider_kind: Option<String>,
    pub model_id: Option<String>,
    pub provider_connected_observed: bool,
    pub live_provider_call_executed: bool,
    pub quality_sensors_requested: Vec<String>,
    pub sensor_outcomes: Vec<HarnessSensorOutcome>,
    pub sensors_passed: usize,
    pub sensors_failed: usize,
    pub sensors_skipped: usize,
    pub quality_blocked: bool,
    pub never_advisory_sensors_deferred: usize,
    pub outcomes: Vec<HarnessPipelineStageOutcome>,
    pub proposed_receipt_id: String,
    pub admission_receipt_id: String,
    pub verdict_receipt_id: String,
    pub receipt_ids: Vec<String>,
    pub live_provider_call_allowed: bool,
    pub patch_application_allowed: bool,
    pub shell_execution_allowed: bool,
    pub worker_spawn_allowed: bool,
    pub production_write_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HarnessPipelineError {
    ManifestRejected(String),
    Internal(String),
}

impl HarnessPipelineError {
    pub fn message(&self) -> String {
        match self {
            Self::ManifestRejected(message) => message.clone(),
            Self::Internal(message) => message.clone(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LocalHarnessPipelineRuntime;

impl LocalHarnessPipelineRuntime {
    pub fn run<S: StorageProvider>(
        &self,
        kernel: &mut MdxKernel<S>,
        manifest: &HarnessRunManifest<'_>,
        request: &HarnessPipelineRequest<'_>,
    ) -> Result<HarnessPipelineVerdict, HarnessPipelineError> {
        kernel.run_harness_pipeline_with_executor(manifest, request, None)
    }

    /// Run with a live model executor injected. The kernel stays fail-closed;
    /// the executor holds the gated authority and may decline (None) to keep
    /// the model stage held.
    pub fn run_with_executor<S: StorageProvider>(
        &self,
        kernel: &mut MdxKernel<S>,
        manifest: &HarnessRunManifest<'_>,
        request: &HarnessPipelineRequest<'_>,
        executor: &dyn PipelineModelExecutor,
    ) -> Result<HarnessPipelineVerdict, HarnessPipelineError> {
        kernel.run_harness_pipeline_with_executor(manifest, request, Some(executor))
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn run_harness_pipeline(
        &mut self,
        manifest: &HarnessRunManifest<'_>,
        request: &HarnessPipelineRequest<'_>,
    ) -> Result<HarnessPipelineVerdict, HarnessPipelineError> {
        self.run_harness_pipeline_with_executor(manifest, request, None)
    }

    /// The legacy single-call shape, now a driver over the suspend
    /// protocol: the executor (when present) runs BETWEEN kernel calls,
    /// never inside one. Receipt order is unchanged from the inline era.
    pub fn run_harness_pipeline_with_executor(
        &mut self,
        manifest: &HarnessRunManifest<'_>,
        request: &HarnessPipelineRequest<'_>,
        executor: Option<&dyn PipelineModelExecutor>,
    ) -> Result<HarnessPipelineVerdict, HarnessPipelineError> {
        let mut step = self.begin_harness_pipeline(manifest, request)?;
        loop {
            match step {
                HarnessPipelineStep::Complete(verdict) => return Ok(*verdict),
                HarnessPipelineStep::NeedsModelCall { state, prepared } => {
                    let execution = executor.and_then(|executor| {
                        executor.execute(&PipelineModelStageContext {
                            profile_id: &prepared.profile_id,
                            provider_kind: &prepared.provider_kind,
                            model_id: &prepared.model_id,
                            intent: &prepared.intent,
                            max_output_tokens: prepared.max_output_tokens,
                        })
                    });
                    step = self.resume_harness_pipeline(state, manifest, request, execution)?;
                }
            }
        }
    }

    /// Phase one of the contract: everything up to (and excluding) the
    /// first live model call runs under the caller's lock - admission,
    /// gateway selection, every mediated stage. A live model stage
    /// suspends the run instead of touching the network.
    pub fn begin_harness_pipeline(
        &mut self,
        manifest: &HarnessRunManifest<'_>,
        request: &HarnessPipelineRequest<'_>,
    ) -> Result<HarnessPipelineStep, HarnessPipelineError> {
        admit_harness_run_manifest(manifest)
            .map_err(|rejection| HarnessPipelineError::ManifestRejected(rejection.message()))?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(manifest.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(manifest.actor_id),
            loop_id: LoopId::new("harness_pipeline"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let harness_run_id = self.ids.next("harness_run");
        let mut receipt_ids: Vec<String> = Vec::new();

        // 1. The proposal is recorded before anything is judged.
        let decision = self.decide_with_receipt(&correlation, ActionKind::ProposeHarnessPipeline);
        let stage_count = request.stages.len().to_string();
        let sensor_list = request.quality_sensors.join(",");
        let proposed = self.transition_receipt(
            &harness_run_id,
            "PIPELINE_PROPOSED",
            &correlation,
            &decision,
            "harness.pipeline.proposed",
            payload(&[
                ("pipeline_id", request.pipeline_id),
                ("intent", request.intent),
                ("stage_count", &stage_count),
                ("quality_sensors", &sensor_list),
                ("manifest_run_id", manifest.run_id),
            ]),
        );
        receipt_ids.push(proposed.receipt_id.clone());

        // 2. Admission: typed validation, budgets, stage vocabulary.
        let admission_denial = self.judge_pipeline_admission(request);

        // 3. Provider profile selection through the governed gateway -
        // the pipeline never picks a model on its own.
        let gateway = if admission_denial.is_none() {
            Some(
                self.run_harness_provider_gateway(manifest, &request.provider)
                    .map_err(|error| HarnessPipelineError::Internal(error.message()))?,
            )
        } else {
            None
        };
        let provider_denied = gateway
            .as_ref()
            .and_then(|report| report.denied_reason.clone())
            .map(|reason| format!("provider_profile_denied:{reason}"));
        let denial = admission_denial.or(provider_denied);
        if let Some(report) = &gateway {
            receipt_ids.push(report.receipt_id.clone());
        }

        let admitted = denial.is_none();
        let admission_decision =
            self.decide_with_receipt(&correlation, ActionKind::AdmitHarnessPipeline);
        let admission = self.transition_receipt(
            &harness_run_id,
            if admitted {
                "PIPELINE_ADMITTED"
            } else {
                "PIPELINE_DENIED"
            },
            &correlation,
            &admission_decision,
            if admitted {
                "harness.pipeline.admitted"
            } else {
                "harness.pipeline.denied"
            },
            payload(&[
                ("pipeline_id", request.pipeline_id),
                ("manifest_run_id", manifest.run_id),
                ("denied_reason", denial.as_deref().unwrap_or("")),
                (
                    "proposed_receipt_id",
                    &receipt_ids.first().cloned().unwrap_or_default(),
                ),
            ]),
        );
        receipt_ids.push(admission.receipt_id.clone());

        // 4. Stage mediation - only an admitted pipeline runs anything.
        // The loop lives in advance_pipeline so a live model stage can
        // suspend the run instead of calling the network under the lock.
        let mut state = Box::new(HarnessPipelineRunState {
            correlation,
            harness_run_id,
            receipt_ids,
            outcomes: Vec::new(),
            tool_calls_used: 0,
            model_calls_used: 0,
            prior_outputs: Vec::new(),
            next_stage: 0,
            gateway,
            admitted,
            denial,
            proposed_receipt_id: proposed.receipt_id,
            admission_receipt_id: admission.receipt_id,
            fingerprint: pipeline_fingerprint(request),
            pending_model_stage: None,
        });
        if let Some(prepared) = self.advance_pipeline(&mut state, manifest, request) {
            return Ok(HarnessPipelineStep::NeedsModelCall { state, prepared });
        }
        Ok(HarnessPipelineStep::Complete(Box::new(
            self.conclude_pipeline(*state, manifest, request)?,
        )))
    }

    /// Phase two: the caller made (or declined) the external call with no
    /// kernel lock held; the result lands here as a record. Some(execution)
    /// records the live stage presence-only; None holds it honestly. The
    /// run then continues - it may suspend again for the next model stage.
    pub fn resume_harness_pipeline(
        &mut self,
        mut state: Box<HarnessPipelineRunState>,
        manifest: &HarnessRunManifest<'_>,
        request: &HarnessPipelineRequest<'_>,
        execution: Option<PipelineModelExecution>,
    ) -> Result<HarnessPipelineStep, HarnessPipelineError> {
        if state.fingerprint != pipeline_fingerprint(request) {
            return Err(HarnessPipelineError::Internal(
                "pipeline resume request mismatch - a suspended run resumes with the same request it began with".to_string(),
            ));
        }
        let Some(index) = state.pending_model_stage.take() else {
            return Err(HarnessPipelineError::Internal(
                "pipeline resume without a suspended model stage".to_string(),
            ));
        };
        let stage = &request.stages[index];
        let gateway_receipt_id = state
            .gateway
            .as_ref()
            .map(|report| report.receipt_id.clone())
            .unwrap_or_default();
        let cap = stage.max_output_bytes.min(request.max_output_bytes).max(1);
        let outcome = match execution {
            Some(execution) => {
                // A real call happened through the caller's gated authority.
                // Presence-only: provider, model, inference id, token usage -
                // never prompt or output text.
                let mut output = format!(
                    "live model answered: provider={} adapter={} model={} inference={} tokens={}in/{}out",
                    execution.provider,
                    execution.adapter,
                    execution.model_id,
                    execution.inference_id,
                    execution.input_tokens,
                    execution.output_tokens
                );
                output.truncate(cap);
                let mut outcome = self.record_live_model_stage(
                    &state.correlation,
                    &state.harness_run_id,
                    stage,
                    &execution,
                    output,
                    gateway_receipt_id,
                );
                outcome.live_provider_call = true;
                outcome
            }
            None => self.record_stage(
                &state.correlation,
                &state.harness_run_id,
                stage,
                "HELD",
                "live profile selected with turn-on evidence - execution held: no live executor admitted the call; the deterministic fallback stays available",
                String::new(),
                None,
                false,
                gateway_receipt_id,
            ),
        };
        state.receipt_ids.push(outcome.receipt_id.clone());
        if outcome.status == "MEDIATED" {
            state.prior_outputs.push(outcome.output.clone());
        }
        state.outcomes.push(outcome);
        state.next_stage = index + 1;
        if let Some(prepared) = self.advance_pipeline(&mut state, manifest, request) {
            return Ok(HarnessPipelineStep::NeedsModelCall { state, prepared });
        }
        Ok(HarnessPipelineStep::Complete(Box::new(
            self.conclude_pipeline(*state, manifest, request)?,
        )))
    }

    /// Drive stages from where the run stands. Returns Some(prepared) when
    /// a live model stage needs the caller to make the external call.
    fn advance_pipeline(
        &mut self,
        state: &mut HarnessPipelineRunState,
        manifest: &HarnessRunManifest<'_>,
        request: &HarnessPipelineRequest<'_>,
    ) -> Option<PreparedPipelineModelCall> {
        if !state.admitted {
            return None;
        }
        while state.next_stage < request.stages.len() {
            let index = state.next_stage;
            let stage = &request.stages[index];
            // A live model stage with budget left suspends here - the
            // budget is spent at prepare time, exactly as the inline era
            // spent it before the call.
            if stage.kind == "model_stage"
                && let Some(report) = state.gateway.as_ref()
            {
                let kind_is_live = report
                    .provider_kind
                    .as_deref()
                    .map(|kind| kind != "deterministic_stub")
                    .unwrap_or(false);
                if kind_is_live && state.model_calls_used < request.max_model_calls {
                    state.model_calls_used += 1;
                    state.pending_model_stage = Some(index);
                    return Some(PreparedPipelineModelCall {
                        profile_id: report.selected_profile_id.clone().unwrap_or_default(),
                        provider_kind: report.provider_kind.clone().unwrap_or_default(),
                        model_id: report.model_id.clone().unwrap_or_default(),
                        intent: stage.query.to_string(),
                        max_output_tokens: 700,
                        gateway_receipt_id: report.receipt_id.clone(),
                    });
                }
            }
            let outcome = self.mediate_pipeline_stage(
                manifest,
                &state.correlation,
                &state.harness_run_id,
                request,
                stage,
                state.gateway.as_ref(),
                &mut state.tool_calls_used,
                &mut state.model_calls_used,
                &state.prior_outputs,
            );
            state.receipt_ids.push(outcome.receipt_id.clone());
            if outcome.status == "MEDIATED" {
                state.prior_outputs.push(outcome.output.clone());
            }
            state.outcomes.push(outcome);
            state.next_stage = index + 1;
        }
        None
    }

    /// The shared tail: sensors, the verdict receipt, the verdict itself.
    fn conclude_pipeline(
        &mut self,
        state: HarnessPipelineRunState,
        manifest: &HarnessRunManifest<'_>,
        request: &HarnessPipelineRequest<'_>,
    ) -> Result<HarnessPipelineVerdict, HarnessPipelineError> {
        let HarnessPipelineRunState {
            correlation,
            harness_run_id,
            mut receipt_ids,
            outcomes,
            gateway,
            admitted,
            denial,
            proposed_receipt_id,
            admission_receipt_id,
            ..
        } = state;
        let sensor_list = request.quality_sensors.join(",");

        // 5. The compact verdict, recorded.
        let stages_run = outcomes
            .iter()
            .filter(|outcome| outcome.status == "MEDIATED")
            .count();
        let stages_denied = outcomes
            .iter()
            .filter(|outcome| outcome.status == "DENIED")
            .count();
        let stages_held = outcomes
            .iter()
            .filter(|outcome| outcome.status == "HELD")
            .count();
        let live_provider_call_executed = outcomes.iter().any(|outcome| outcome.live_provider_call);

        // 4b. Quality sensors evaluated against the run's own evidence.
        // The verdict authority flags are closed by construction below, so
        // the honesty facts the sensors judge come straight from outcomes.
        let mut human_copy: Vec<&str> = vec![request.intent];
        for outcome in &outcomes {
            human_copy.push(outcome.summary.as_str());
        }
        let evidence = SensorRunEvidence {
            all_executed_stages_have_receipts: outcomes
                .iter()
                .filter(|outcome| outcome.executed)
                .all(|outcome| !outcome.receipt_id.is_empty()),
            executed_only_when_mediated: outcomes
                .iter()
                .all(|outcome| !outcome.executed || outcome.status == "MEDIATED"),
            all_authority_flags_closed: true,
            admitted,
            status_claims_completion: admitted,
            human_copy: &human_copy,
        };
        let sensor_outcomes = evaluate_pipeline_sensors(request.quality_sensors, &evidence);
        let sensor_summary = summarize_sensors(&sensor_outcomes);

        let status = if !admitted {
            "PIPELINE_DENIED"
        } else if sensor_summary.blocked {
            "PIPELINE_BLOCKED_BY_SENSOR"
        } else if stages_denied > 0 || sensor_summary.failed > 0 {
            "PIPELINE_COMPLETED_WITH_DENIALS"
        } else {
            "PIPELINE_COMPLETED_LOCAL"
        };
        let verdict_decision =
            self.decide_with_receipt(&correlation, ActionKind::RecordHarnessPipelineVerdict);
        let run_count = stages_run.to_string();
        let denied_count = stages_denied.to_string();
        let held_count = stages_held.to_string();
        let sensors_passed_count = sensor_summary.passed.to_string();
        let sensors_failed_count = sensor_summary.failed.to_string();
        let sensors_skipped_count = sensor_summary.skipped.to_string();
        let never_advisory_deferred_count = sensor_summary.never_advisory_deferred.to_string();
        let failed_sensor_ids = sensor_outcomes
            .iter()
            .filter(|outcome| outcome.status == "FAILED")
            .map(|outcome| outcome.sensor_id.as_str())
            .collect::<Vec<_>>()
            .join(",");
        let provider_connected_observed = gateway
            .as_ref()
            .and_then(|report| {
                self.storage
                    .ledger()
                    .query()
                    .by_id(&report.receipt_id)
                    .map(|receipt| {
                        receipt
                            .payload
                            .get("provider_connected_observed")
                            .map(String::as_str)
                            == Some("true")
                    })
            })
            .unwrap_or(false);
        let verdict_receipt = self.transition_receipt(
            &harness_run_id,
            status,
            &correlation,
            &verdict_decision,
            "harness.pipeline.verdict.recorded",
            payload(&[
                ("pipeline_id", request.pipeline_id),
                ("manifest_run_id", manifest.run_id),
                ("status", status),
                ("stages_run", &run_count),
                ("stages_denied", &denied_count),
                ("stages_held", &held_count),
                ("quality_sensors", &sensor_list),
                ("sensors_passed", &sensors_passed_count),
                ("sensors_failed", &sensors_failed_count),
                ("sensors_skipped", &sensors_skipped_count),
                ("failed_sensor_ids", &failed_sensor_ids),
                (
                    "never_advisory_sensors_deferred",
                    &never_advisory_deferred_count,
                ),
                (
                    "quality_blocked",
                    if sensor_summary.blocked {
                        "true"
                    } else {
                        "false"
                    },
                ),
                (
                    "provider_profile_id",
                    gateway
                        .as_ref()
                        .and_then(|report| report.selected_profile_id.as_deref())
                        .unwrap_or(""),
                ),
                (
                    "model_id",
                    gateway
                        .as_ref()
                        .and_then(|report| report.model_id.as_deref())
                        .unwrap_or(""),
                ),
                ("live_provider_call_allowed", "false"),
                (
                    "live_provider_call_executed",
                    if live_provider_call_executed {
                        "true"
                    } else {
                        "false"
                    },
                ),
                ("patch_application_allowed", "false"),
                ("shell_execution_allowed", "false"),
                ("worker_spawn_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        receipt_ids.push(verdict_receipt.receipt_id.clone());
        self.storage
            .ledger()
            .verify()
            .map_err(HarnessPipelineError::Internal)?;

        Ok(HarnessPipelineVerdict {
            pipeline_id: request.pipeline_id.to_string(),
            harness_run_id,
            status,
            admitted,
            denied_reason: denial,
            stages_run,
            stages_denied,
            stages_held,
            provider_profile_id: gateway
                .as_ref()
                .and_then(|report| report.selected_profile_id.clone()),
            provider_kind: gateway
                .as_ref()
                .and_then(|report| report.provider_kind.clone()),
            model_id: gateway.as_ref().and_then(|report| report.model_id.clone()),
            provider_connected_observed,
            live_provider_call_executed,
            quality_sensors_requested: request
                .quality_sensors
                .iter()
                .map(|sensor| sensor.to_string())
                .collect(),
            sensors_passed: sensor_summary.passed,
            sensors_failed: sensor_summary.failed,
            sensors_skipped: sensor_summary.skipped,
            quality_blocked: sensor_summary.blocked,
            never_advisory_sensors_deferred: sensor_summary.never_advisory_deferred,
            sensor_outcomes,
            outcomes,
            proposed_receipt_id,
            admission_receipt_id,
            verdict_receipt_id: verdict_receipt.receipt_id,
            receipt_ids,
            live_provider_call_allowed: false,
            patch_application_allowed: false,
            shell_execution_allowed: false,
            worker_spawn_allowed: false,
            production_write_allowed: false,
        })
    }

    fn judge_pipeline_admission(&self, request: &HarnessPipelineRequest<'_>) -> Option<String> {
        if request.pipeline_id.trim().is_empty() {
            return Some("missing_pipeline_id".to_string());
        }
        if request.intent.trim().is_empty() {
            return Some("missing_intent".to_string());
        }
        if request.stages.is_empty() {
            return Some("empty_pipeline".to_string());
        }
        if request.max_stages == 0 || request.max_tool_calls == 0 || request.max_output_bytes == 0 {
            return Some("empty_pipeline_budget".to_string());
        }
        if request.stages.len() > request.max_stages {
            return Some("stage_budget_exceeded".to_string());
        }
        for stage in request.stages {
            if stage.stage_id.trim().is_empty() {
                return Some("missing_stage_id".to_string());
            }
            let runnable = PIPELINE_RUNNABLE_STAGE_KINDS.contains(&stage.kind);
            let declared = PIPELINE_NOT_RUNNABLE_STAGE_KINDS
                .iter()
                .any(|(kind, _)| *kind == stage.kind);
            if !runnable && !declared {
                return Some(format!("unknown_stage_kind:{}", stage.kind));
            }
        }
        let model_stages = request
            .stages
            .iter()
            .filter(|stage| stage.kind == "model_stage")
            .count();
        if model_stages > request.max_model_calls {
            return Some("model_call_budget_exceeded".to_string());
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    fn mediate_pipeline_stage(
        &mut self,
        manifest: &HarnessRunManifest<'_>,
        correlation: &CorrelationIds,
        harness_run_id: &str,
        request: &HarnessPipelineRequest<'_>,
        stage: &HarnessPipelineStage<'_>,
        gateway: Option<&HarnessProviderGatewayReport>,
        tool_calls_used: &mut usize,
        model_calls_used: &mut usize,
        prior_outputs: &[String],
    ) -> HarnessPipelineStageOutcome {
        // Declared-not-runnable kinds deny with the missing authority named.
        if let Some((_, missing)) = PIPELINE_NOT_RUNNABLE_STAGE_KINDS
            .iter()
            .find(|(kind, _)| *kind == stage.kind)
        {
            return self.record_stage(
                correlation,
                harness_run_id,
                stage,
                "DENIED",
                missing,
                String::new(),
                Some((*missing).to_string()),
                false,
                String::new(),
            );
        }
        let is_tool_stage = matches!(
            stage.kind,
            "virtual_read" | "virtual_search" | "virtual_list" | "patch_preview"
        );
        if is_tool_stage {
            if *tool_calls_used >= request.max_tool_calls {
                return self.record_stage(
                    correlation,
                    harness_run_id,
                    stage,
                    "DENIED",
                    "tool call budget exhausted - the cap denies, it never truncates silently",
                    String::new(),
                    Some("tool_call_budget_exhausted".to_string()),
                    false,
                    String::new(),
                );
            }
            *tool_calls_used += 1;
        }
        let cap = stage.max_output_bytes.min(request.max_output_bytes).max(1);
        match stage.kind {
            "virtual_read" => {
                let report = LocalHarnessReadToolPlane.read_virtual_artifact(
                    self,
                    manifest,
                    &HarnessReadToolRequest {
                        path: stage.path,
                        contents: stage.contents,
                        allowed_read_roots: stage.allowed_roots,
                        max_output_bytes: cap,
                    },
                );
                self.tool_stage_outcome(
                    correlation,
                    harness_run_id,
                    stage,
                    report.map(|r| (r.status, r.output, r.denied_reason, r.receipt_id)),
                )
            }
            "virtual_search" => {
                let report = LocalHarnessReadToolPlane.search_virtual_artifact(
                    self,
                    manifest,
                    &HarnessSearchToolRequest {
                        path: stage.path,
                        contents: stage.contents,
                        query: stage.query,
                        allowed_read_roots: stage.allowed_roots,
                        max_matches: 8,
                        max_output_bytes: cap,
                    },
                );
                self.tool_stage_outcome(
                    correlation,
                    harness_run_id,
                    stage,
                    report.map(|r| {
                        let output = r
                            .matches
                            .iter()
                            .map(|m| format!("{}: {}", m.line_number, m.line))
                            .collect::<Vec<_>>()
                            .join("\n");
                        (r.status, output, r.denied_reason, r.receipt_id)
                    }),
                )
            }
            "virtual_list" => {
                let report = LocalHarnessReadToolPlane.list_virtual_artifacts(
                    self,
                    manifest,
                    &HarnessListToolRequest {
                        path: stage.path,
                        entries: stage.items,
                        allowed_read_roots: stage.allowed_roots,
                        max_entries: stage.items.len().max(1),
                        max_output_bytes: cap,
                    },
                );
                self.tool_stage_outcome(
                    correlation,
                    harness_run_id,
                    stage,
                    report.map(|r| {
                        (
                            r.status,
                            r.entries.join("\n"),
                            r.denied_reason,
                            r.receipt_id,
                        )
                    }),
                )
            }
            "patch_preview" => {
                let report = LocalHarnessReadToolPlane.mediate_virtual_patch(
                    self,
                    manifest,
                    &HarnessPatchToolRequest {
                        path: stage.path,
                        patch: stage.contents,
                        max_patch_bytes: cap,
                        max_output_bytes: cap,
                    },
                );
                self.tool_stage_outcome(
                    correlation,
                    harness_run_id,
                    stage,
                    report.map(|r| {
                        (
                            r.status,
                            format!("preview only, applied=false\n{}", r.patch_preview),
                            r.denied_reason,
                            r.receipt_id,
                        )
                    }),
                )
            }
            "memory_recall" => {
                // Local context tiers only: titles and source types, never
                // bodies - recall stays a lead, not a data channel.
                let recall = self.assemble_local_context_summary(stage.query, cap);
                self.record_stage(
                    correlation,
                    harness_run_id,
                    stage,
                    "MEDIATED",
                    "local context recall - titles and source types only",
                    recall,
                    None,
                    true,
                    String::new(),
                )
            }
            "dedupe_rank_select" => {
                // Pure transform over caller items plus prior stage outputs.
                let mut pool: Vec<String> =
                    stage.items.iter().map(|item| item.to_string()).collect();
                pool.extend(prior_outputs.iter().cloned());
                let mut seen: Vec<String> = Vec::new();
                for item in pool {
                    for line in item.lines() {
                        let line = line.trim();
                        if !line.is_empty() && !seen.iter().any(|existing| existing == line) {
                            seen.push(line.to_string());
                        }
                    }
                }
                let query = stage.query.to_lowercase();
                seen.sort_by_key(|line| {
                    std::cmp::Reverse(line.to_lowercase().matches(&query).count())
                });
                let mut output = seen.join("\n");
                output.truncate(cap);
                self.record_stage(
                    correlation,
                    harness_run_id,
                    stage,
                    "MEDIATED",
                    "pure dedupe and rank over prior stage outputs - no new access",
                    output,
                    None,
                    true,
                    String::new(),
                )
            }
            "model_stage" => {
                let Some(report) = gateway else {
                    return self.record_stage(
                        correlation,
                        harness_run_id,
                        stage,
                        "DENIED",
                        "no provider profile selected",
                        String::new(),
                        Some("provider_profile_missing".to_string()),
                        false,
                        String::new(),
                    );
                };
                if *model_calls_used >= request.max_model_calls {
                    return self.record_stage(
                        correlation,
                        harness_run_id,
                        stage,
                        "DENIED",
                        "model call budget exhausted",
                        String::new(),
                        Some("model_call_budget_exhausted".to_string()),
                        false,
                        String::new(),
                    );
                }
                *model_calls_used += 1;
                let kind_is_live = report
                    .provider_kind
                    .as_deref()
                    .map(|kind| kind != "deterministic_stub")
                    .unwrap_or(false);
                if kind_is_live {
                    // A live profile reaches here only when the suspend
                    // protocol did not prepare the call (advance_pipeline
                    // suspends every budgeted live stage before mediation).
                    // The stage stays honestly held - the kernel never
                    // makes the call itself, never deterministic theater
                    // for a live ask.
                    return self.record_stage(
                        correlation,
                        harness_run_id,
                        stage,
                        "HELD",
                        "live profile selected with turn-on evidence - execution held: no live executor admitted the call; the deterministic fallback stays available",
                        String::new(),
                        None,
                        false,
                        report.receipt_id.clone(),
                    );
                }
                // The deterministic local driver is the executing model.
                let trace = DeterministicModelGateway
                    .run_eval_suite("harness_pipeline_model_stage")
                    .unwrap_or_else(|_| unreachable!("deterministic gateway is infallible"));
                let mut output = format!(
                    "deterministic local model run: inference={} variant={} stream={} terminal={} score={}",
                    trace.inference_id,
                    trace.variant,
                    trace.stream_contract,
                    trace.terminal_event,
                    trace.score
                );
                output.truncate(cap);
                self.record_stage(
                    correlation,
                    harness_run_id,
                    stage,
                    "MEDIATED",
                    "deterministic local model stage - no prompt or output text recorded",
                    output,
                    None,
                    true,
                    report.receipt_id.clone(),
                )
            }
            _ => unreachable!("admission validated stage kinds"),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn tool_stage_outcome(
        &mut self,
        correlation: &CorrelationIds,
        harness_run_id: &str,
        stage: &HarnessPipelineStage<'_>,
        result: Result<(String, String, Option<String>, String), HarnessToolPlaneError>,
    ) -> HarnessPipelineStageOutcome {
        match result {
            Ok((status, output, denied_reason, inner_receipt_id)) => {
                let denied = denied_reason.is_some() || status.contains("DENIED");
                self.record_stage(
                    correlation,
                    harness_run_id,
                    stage,
                    if denied { "DENIED" } else { "MEDIATED" },
                    if denied {
                        "the safe tool plane denied this stage"
                    } else {
                        "mediated through the safe tool plane"
                    },
                    output,
                    denied_reason,
                    !denied,
                    inner_receipt_id,
                )
            }
            Err(error) => self.record_stage(
                correlation,
                harness_run_id,
                stage,
                "DENIED",
                "the safe tool plane rejected this stage",
                String::new(),
                Some(error.message()),
                false,
                String::new(),
            ),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn record_stage(
        &mut self,
        correlation: &CorrelationIds,
        harness_run_id: &str,
        stage: &HarnessPipelineStage<'_>,
        status: &'static str,
        summary: &str,
        output: String,
        denied_reason: Option<String>,
        executed: bool,
        inner_receipt_id: String,
    ) -> HarnessPipelineStageOutcome {
        let decision =
            self.decide_with_receipt(correlation, ActionKind::MediateHarnessPipelineStage);
        let receipt = self.transition_receipt(
            harness_run_id,
            status,
            correlation,
            &decision,
            if status == "DENIED" {
                "harness.pipeline.stage.denied"
            } else {
                "harness.pipeline.stage.mediated"
            },
            payload(&[
                ("stage_id", stage.stage_id),
                ("stage_kind", stage.kind),
                ("status", status),
                ("summary", summary),
                ("denied_reason", denied_reason.as_deref().unwrap_or("")),
                ("inner_receipt_id", &inner_receipt_id),
                ("executed", if executed { "true" } else { "false" }),
            ]),
        );
        HarnessPipelineStageOutcome {
            stage_id: stage.stage_id.to_string(),
            kind: stage.kind.to_string(),
            status,
            summary: summary.to_string(),
            output,
            denied_reason,
            executed,
            receipt_id: receipt.receipt_id,
            inner_receipt_id,
            live_provider_call: false,
        }
    }

    // Record a live model stage: a real provider call happened through the
    // governed executor. The receipt carries identity and usage only -
    // provider, adapter, model, inference id, token counts - never the
    // prompt or the output text. This is the same presence-only discipline
    // as the provider turn-on observation.
    fn record_live_model_stage(
        &mut self,
        correlation: &CorrelationIds,
        harness_run_id: &str,
        stage: &HarnessPipelineStage<'_>,
        execution: &PipelineModelExecution,
        output: String,
        gateway_receipt_id: String,
    ) -> HarnessPipelineStageOutcome {
        let decision =
            self.decide_with_receipt(correlation, ActionKind::MediateHarnessPipelineStage);
        let input_tokens = execution.input_tokens.to_string();
        let output_tokens = execution.output_tokens.to_string();
        let receipt = self.transition_receipt(
            harness_run_id,
            "MEDIATED",
            correlation,
            &decision,
            "harness.pipeline.stage.mediated",
            payload(&[
                ("stage_id", stage.stage_id),
                ("stage_kind", stage.kind),
                ("status", "MEDIATED"),
                (
                    "summary",
                    "live model answered through the governed executor - no prompt or output text recorded",
                ),
                ("executed", "true"),
                ("live_provider_call_executed", "true"),
                ("provider", &execution.provider),
                ("adapter", &execution.adapter),
                ("model_id", &execution.model_id),
                ("inference_id", &execution.inference_id),
                ("input_tokens", &input_tokens),
                ("output_tokens", &output_tokens),
                ("provider_secret_values_recorded", "false"),
                ("prompt_text_recorded", "false"),
                ("output_text_recorded", "false"),
                ("inner_receipt_id", &gateway_receipt_id),
            ]),
        );
        HarnessPipelineStageOutcome {
            stage_id: stage.stage_id.to_string(),
            kind: stage.kind.to_string(),
            status: "MEDIATED",
            summary: "live model answered through the governed executor - no prompt or output text recorded"
                .to_string(),
            output,
            denied_reason: None,
            executed: true,
            receipt_id: receipt.receipt_id,
            inner_receipt_id: gateway_receipt_id,
            live_provider_call: true,
        }
    }

    // A capped recall summary from the local context engine: source types
    // and titles only, never bodies - recall stays a lead, not a data
    // channel. Engine failures read as honest silence.
    fn assemble_local_context_summary(&mut self, topic: &str, cap: usize) -> String {
        let packet = self.assemble_local_context(LocalContextRequest {
            tenant_id: "tenant_local",
            actor_id: "harness_pipeline",
            query: topic,
            token_budget: 1024,
            source_preferences: &[],
            query_entities: &[],
            local_memories: &[],
        });
        let mut lines: Vec<String> = match packet {
            Ok(packet) => packet
                .sources
                .iter()
                .take(8)
                .map(|source| format!("{}: {}", source.source_type, source.title))
                .collect(),
            Err(_) => Vec::new(),
        };
        if lines.is_empty() {
            lines.push("no local context matched - recall is honest about silence".to_string());
        }
        let mut output = lines.join("\n");
        output.truncate(cap);
        output
    }
}
