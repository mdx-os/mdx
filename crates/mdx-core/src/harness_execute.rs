use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HarnessPlanExecutionActionKind {
    PatchApplication,
    // A declared test command run inside a sandbox - distinct from arbitrary
    // ShellCommand, which is never unlocked. Only TestExecution runs, and only
    // through the injected sandbox runner with no network and resource caps.
    TestExecution,
    ShellCommand,
    GitCommand,
    BrowserAutomation,
    McpStartup,
    NetworkAccess,
    WorkerHandoff,
    ExternalMachine,
}

impl HarnessPlanExecutionActionKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PatchApplication => "patch_application",
            Self::TestExecution => "test_execution",
            Self::ShellCommand => "shell_command",
            Self::GitCommand => "git_command",
            Self::BrowserAutomation => "browser_automation",
            Self::McpStartup => "mcp_startup",
            Self::NetworkAccess => "network_access",
            Self::WorkerHandoff => "worker_handoff",
            Self::ExternalMachine => "external_machine",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HarnessPlanExecutionAction<'a> {
    pub action_id: &'a str,
    pub action_kind: HarnessPlanExecutionActionKind,
    pub target: &'a str,
    pub summary: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HarnessPlanBlockedActionReport {
    pub action_id: String,
    pub action_kind: String,
    pub target: String,
    pub summary: String,
    pub status: String,
    pub allowed: bool,
    pub blocked_by: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HarnessPlanExecuteLoopRequest<'a> {
    pub admission_receipt_id: &'a str,
    pub plan_receipt_id: &'a str,
    pub approved_plan_hash: &'a str,
    pub selected_provider_profile_receipt_id: &'a str,
    pub requested_actions: &'a [HarnessPlanExecutionAction<'a>],
    pub quality_gates: &'a [&'a str],
    pub final_verdict_criteria: &'a [&'a str],
    pub produced_artifacts: &'a [&'a str],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HarnessPlanExecuteLoopReport {
    pub manifest_run_id: String,
    pub harness_run_id: String,
    pub status: String,
    pub approved_plan_hash: String,
    pub selected_provider_profile_receipt_id: String,
    pub plan_hash_approval_receipt_id: String,
    pub execution_denied_receipt_id: String,
    pub quality_gate_receipt_ids: Vec<String>,
    pub final_verdict_receipt_id: String,
    pub blocked_dangerous_actions: Vec<String>,
    pub blocked_action_reports: Vec<HarnessPlanBlockedActionReport>,
    pub completed_quality_gates: Vec<String>,
    pub failed_quality_gates: Vec<String>,
    pub skipped_quality_gates: Vec<String>,
    pub produced_artifacts: Vec<String>,
    pub runtime_execution_allowed: bool,
    pub patch_application_allowed: bool,
    pub applied_patch_actions: Vec<String>,
    pub applied_patch_receipt_ids: Vec<String>,
    pub test_execution_allowed: bool,
    pub executed_test_actions: Vec<String>,
    pub executed_test_receipt_ids: Vec<String>,
    pub policy_decision_ids: Vec<String>,
    pub receipts: Vec<String>,
    pub transcript: HarnessRunTranscript,
}

/// The context the loop hands the injected patch applier for one approved
/// PatchApplication action. The diff itself rides in the applier (the server
/// builds it with the plan's diffs, keyed by action id) so the kernel keeps
/// no patch content. The kernel never touches the filesystem - the applier
/// writes only inside an isolated worktree, never the live repo.
pub struct HarnessPatchApplyContext<'a> {
    pub action_id: &'a str,
    pub target: &'a str,
    pub allowed_write_scope: &'a [&'a str],
    pub approved_plan_hash: &'a str,
}

/// Presence-only result of an isolated-worktree patch application: which
/// worktree, how much changed, applied or not. No file content, no diff
/// body - the numbers and the isolation are the evidence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HarnessPatchApplyResult {
    pub worktree_id: String,
    pub files_changed: u32,
    pub insertions: u32,
    pub deletions: u32,
    pub applied: bool,
    pub denied_reason: Option<String>,
}

/// The port the plan-execute loop delegates patch application to. The kernel
/// never opens the filesystem; a server-layer applier writes the diff into an
/// isolated git worktree, captures the numbers, and removes the worktree -
/// the live repo is never mutated. Returning None (or applied=false) keeps
/// the action denied, honestly.
pub trait HarnessPatchApplier {
    fn apply(&self, context: &HarnessPatchApplyContext<'_>) -> Option<HarnessPatchApplyResult>;
}

/// The context the loop hands the injected sandbox runner for one approved
/// TestExecution action. The command rides here; the kernel never runs it.
pub struct HarnessSandboxRunContext<'a> {
    pub action_id: &'a str,
    pub command: &'a str,
    pub allowed_write_scope: &'a [&'a str],
    pub approved_plan_hash: &'a str,
}

/// Presence-only result of a sandboxed test run: which sandbox, the outcome,
/// and the isolation facts. Never the command output text - exit code,
/// duration, output byte count, and the no-network proof are the evidence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HarnessSandboxRunResult {
    pub sandbox_id: String,
    pub exit_code: i32,
    pub duration_ms: u64,
    pub output_bytes: u64,
    pub passed: bool,
    pub network_disabled: bool,
    pub timed_out: bool,
    /// The combined output hit the byte cap and was truncated before the host
    /// read it. Presence-only: the count is capped, the text is never recorded.
    pub output_truncated: bool,
}

/// The port the plan-execute loop delegates a sandboxed test command to. The
/// kernel never runs commands; a server-layer runner executes inside an
/// ephemeral container with no network and resource caps, captures the
/// outcome, and tears the container down. Returning None keeps the action
/// denied. This is the only command-execution path, and it is sandboxed.
pub trait HarnessSandboxRunner {
    fn run(&self, context: &HarnessSandboxRunContext<'_>) -> Option<HarnessSandboxRunResult>;
}

/// The execution drivers the server injects. The kernel owns admission,
/// gating, receipts, and the verdict; these ports hold the gated authority to
/// touch the filesystem (patch) or run a command (sandbox). Both default to
/// None - then the loop is fully fail-closed.
#[derive(Clone, Copy, Default)]
pub struct HarnessExecutionDrivers<'a> {
    pub patch_applier: Option<&'a dyn HarnessPatchApplier>,
    pub sandbox_runner: Option<&'a dyn HarnessSandboxRunner>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HarnessPlanExecuteLoopError {
    ManifestRejected(String),
    MissingField(&'static str),
    EmptyList(&'static str),
    MissingRequiredReceiptKind(&'static str),
    MissingLedgerReceipt {
        receipt_id: String,
        expected_kind: &'static str,
    },
    WrongLedgerReceiptKind {
        receipt_id: String,
        expected_kind: &'static str,
        observed_kind: String,
    },
    ExecutionBoundaryOpen(&'static str),
    InvalidPlanHash,
}

impl HarnessPlanExecuteLoopError {
    pub fn message(&self) -> String {
        match self {
            Self::ManifestRejected(message) => message.clone(),
            Self::MissingField(field) => format!("harness plan execute loop missing {field}"),
            Self::EmptyList(field) => format!("harness plan execute loop list {field} is empty"),
            Self::MissingRequiredReceiptKind(kind) => {
                format!("harness plan execute loop manifest missing required receipt kind {kind}")
            }
            Self::MissingLedgerReceipt {
                receipt_id,
                expected_kind,
            } => format!("harness plan execute loop missing {expected_kind} receipt {receipt_id}"),
            Self::WrongLedgerReceiptKind {
                receipt_id,
                expected_kind,
                observed_kind,
            } => format!(
                "harness plan execute loop receipt {receipt_id} expected {expected_kind} observed {observed_kind}"
            ),
            Self::ExecutionBoundaryOpen(field) => {
                format!("harness plan execute loop requires closed execution boundary for {field}")
            }
            Self::InvalidPlanHash => {
                "harness plan execute loop approved_plan_hash must be a single token".to_string()
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LocalHarnessPlanExecuteLoop;

impl LocalHarnessPlanExecuteLoop {
    pub fn record_execution_attempt<S: StorageProvider>(
        &self,
        kernel: &mut MdxKernel<S>,
        manifest: &HarnessRunManifest<'_>,
        request: &HarnessPlanExecuteLoopRequest<'_>,
    ) -> Result<HarnessPlanExecuteLoopReport, HarnessPlanExecuteLoopError> {
        kernel.run_harness_plan_execute_loop(manifest, request)
    }

    pub fn record_execution_attempt_with_applier<S: StorageProvider>(
        &self,
        kernel: &mut MdxKernel<S>,
        manifest: &HarnessRunManifest<'_>,
        request: &HarnessPlanExecuteLoopRequest<'_>,
        patch_applier: &dyn HarnessPatchApplier,
    ) -> Result<HarnessPlanExecuteLoopReport, HarnessPlanExecuteLoopError> {
        kernel.run_harness_plan_execute_loop_with_applier(manifest, request, Some(patch_applier))
    }

    pub fn record_execution_attempt_with_drivers<S: StorageProvider>(
        &self,
        kernel: &mut MdxKernel<S>,
        manifest: &HarnessRunManifest<'_>,
        request: &HarnessPlanExecuteLoopRequest<'_>,
        drivers: HarnessExecutionDrivers<'_>,
    ) -> Result<HarnessPlanExecuteLoopReport, HarnessPlanExecuteLoopError> {
        kernel.run_harness_plan_execute_loop_with_drivers(manifest, request, drivers)
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn run_harness_plan_execute_loop(
        &mut self,
        manifest: &HarnessRunManifest<'_>,
        request: &HarnessPlanExecuteLoopRequest<'_>,
    ) -> Result<HarnessPlanExecuteLoopReport, HarnessPlanExecuteLoopError> {
        self.run_harness_plan_execute_loop_with_applier(manifest, request, None)
    }

    /// Run with an injected patch applier only (no sandbox runner).
    pub fn run_harness_plan_execute_loop_with_applier(
        &mut self,
        manifest: &HarnessRunManifest<'_>,
        request: &HarnessPlanExecuteLoopRequest<'_>,
        patch_applier: Option<&dyn HarnessPatchApplier>,
    ) -> Result<HarnessPlanExecuteLoopReport, HarnessPlanExecuteLoopError> {
        self.run_harness_plan_execute_loop_with_drivers(
            manifest,
            request,
            HarnessExecutionDrivers {
                patch_applier,
                sandbox_runner: None,
            },
        )
    }

    /// Run with injected execution drivers. Only PatchApplication actions in
    /// the write scope that the applier applies (in an isolated worktree) and
    /// TestExecution actions the sandbox runner runs (in an ephemeral, no-
    /// network container) become executed; every other action stays denied.
    /// The kernel never touches the filesystem or runs a command itself.
    pub fn run_harness_plan_execute_loop_with_drivers(
        &mut self,
        manifest: &HarnessRunManifest<'_>,
        request: &HarnessPlanExecuteLoopRequest<'_>,
        drivers: HarnessExecutionDrivers<'_>,
    ) -> Result<HarnessPlanExecuteLoopReport, HarnessPlanExecuteLoopError> {
        let patch_applier = drivers.patch_applier;
        let sandbox_runner = drivers.sandbox_runner;
        validate_harness_plan_execute_loop_request(self, manifest, request)?;

        let starting_receipts = self.storage.ledger().entries().len();
        let starting_policy_decisions = self.storage.policy_decisions().len();
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(manifest.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(manifest.actor_id),
            loop_id: LoopId::new("harness_plan_execute_loop"),
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

        let quality_gates = request
            .quality_gates
            .iter()
            .map(|gate| gate.trim().to_string())
            .collect::<Vec<_>>();
        let skipped_quality_gates_joined = join_owned_harness_values(&quality_gates);
        let final_verdict_criteria = join_harness_values(request.final_verdict_criteria);
        let produced_artifacts = join_harness_values(request.produced_artifacts);
        let max_tool_calls = manifest.budget_policy.max_tool_calls.to_string();
        let max_runtime_ms = manifest.budget_policy.max_runtime_ms.to_string();
        let max_output_tokens = manifest.budget_policy.max_output_tokens.to_string();

        let approval_decision =
            self.decide_with_receipt(&correlation, ActionKind::ApproveHarnessPlanHash);
        let approval_receipt = self.transition_receipt(
            &harness_run_id,
            "PLAN_HASH_APPROVED",
            &correlation,
            &approval_decision,
            "harness.plan.hash.approved",
            payload(&[
                ("manifest_run_id", manifest.run_id),
                ("admission_receipt_id", request.admission_receipt_id),
                ("plan_receipt_id", request.plan_receipt_id),
                ("approved_plan_hash", request.approved_plan_hash.trim()),
                (
                    "selected_provider_profile_receipt_id",
                    request.selected_provider_profile_receipt_id,
                ),
                ("approval_mode", manifest.approval_mode.as_str()),
                (
                    "approval_receipt_id",
                    manifest
                        .trust_boundary
                        .approval_receipt_id
                        .unwrap_or("none"),
                ),
            ]),
        );

        // The first execution-authority unlock: patch application. The ONLY
        // execution authority this loop opens. A PatchApplication action
        // whose target is inside the manifest write scope (and not a blocked
        // path) is handed to the injected applier, which writes the diff into
        // an ISOLATED worktree and never the live repo. Anything the applier
        // declines, and every non-patch action, stays denied below. The
        // kernel records presence-only evidence: files and line counts, the
        // worktree id, applied=true - never the diff body or file content.
        let mut applied_patch_actions: Vec<String> = Vec::new();
        let mut applied_patch_receipt_ids: Vec<String> = Vec::new();
        if let Some(applier) = patch_applier {
            for action in request.requested_actions {
                if action.action_kind != HarnessPlanExecutionActionKind::PatchApplication {
                    continue;
                }
                if !path_within_harness_write_scope(
                    action.target.trim(),
                    manifest.allowed_write_scope,
                    manifest.blocked_paths,
                ) {
                    continue;
                }
                let Some(result) = applier.apply(&HarnessPatchApplyContext {
                    action_id: action.action_id,
                    target: action.target,
                    allowed_write_scope: manifest.allowed_write_scope,
                    approved_plan_hash: request.approved_plan_hash.trim(),
                }) else {
                    continue;
                };
                if !result.applied {
                    continue;
                }
                let files_changed = result.files_changed.to_string();
                let insertions = result.insertions.to_string();
                let deletions = result.deletions.to_string();
                let apply_decision =
                    self.decide_with_receipt(&correlation, ActionKind::ApplyHarnessPatch);
                let apply_receipt = self.transition_receipt(
                    &harness_run_id,
                    "PATCH_APPLIED",
                    &correlation,
                    &apply_decision,
                    "harness.patch.applied",
                    payload(&[
                        ("manifest_run_id", manifest.run_id),
                        ("action_id", action.action_id),
                        ("target", action.target.trim()),
                        (
                            "plan_hash_approval_receipt_id",
                            &approval_receipt.receipt_id,
                        ),
                        ("approved_plan_hash", request.approved_plan_hash.trim()),
                        ("worktree_id", &result.worktree_id),
                        ("worktree_isolated", "true"),
                        ("live_repo_mutated", "false"),
                        ("files_changed", &files_changed),
                        ("insertions", &insertions),
                        ("deletions", &deletions),
                        ("applied", "true"),
                        ("diff_body_recorded", "false"),
                        ("file_content_recorded", "false"),
                    ]),
                );
                applied_patch_actions.push(action.action_id.to_string());
                applied_patch_receipt_ids.push(apply_receipt.receipt_id);
            }
        }

        // The second execution-authority unlock: sandboxed test execution. A
        // TestExecution action is handed to the injected sandbox runner, which
        // runs the command inside an ephemeral container with no network and
        // resource caps. Arbitrary ShellCommand stays denied - only this
        // declared, sandboxed test path runs. Presence-only evidence: exit
        // code, duration, output byte count, and the no-network proof; never
        // the command output text.
        let mut executed_test_actions: Vec<String> = Vec::new();
        let mut executed_test_receipt_ids: Vec<String> = Vec::new();
        if let Some(runner) = sandbox_runner {
            for action in request.requested_actions {
                if action.action_kind != HarnessPlanExecutionActionKind::TestExecution {
                    continue;
                }
                let Some(result) = runner.run(&HarnessSandboxRunContext {
                    action_id: action.action_id,
                    command: action.target,
                    allowed_write_scope: manifest.allowed_write_scope,
                    approved_plan_hash: request.approved_plan_hash.trim(),
                }) else {
                    continue;
                };
                let exit_code = result.exit_code.to_string();
                let duration_ms = result.duration_ms.to_string();
                let output_bytes = result.output_bytes.to_string();
                let test_decision =
                    self.decide_with_receipt(&correlation, ActionKind::ExecuteHarnessTest);
                let test_receipt = self.transition_receipt(
                    &harness_run_id,
                    "TEST_EXECUTED",
                    &correlation,
                    &test_decision,
                    "harness.test.executed",
                    payload(&[
                        ("manifest_run_id", manifest.run_id),
                        ("action_id", action.action_id),
                        (
                            "plan_hash_approval_receipt_id",
                            &approval_receipt.receipt_id,
                        ),
                        ("approved_plan_hash", request.approved_plan_hash.trim()),
                        ("sandbox_id", &result.sandbox_id),
                        ("sandbox_isolated", "true"),
                        (
                            "network_disabled",
                            if result.network_disabled {
                                "true"
                            } else {
                                "false"
                            },
                        ),
                        ("exit_code", &exit_code),
                        ("passed", if result.passed { "true" } else { "false" }),
                        ("timed_out", if result.timed_out { "true" } else { "false" }),
                        ("duration_ms", &duration_ms),
                        ("output_bytes", &output_bytes),
                        (
                            "output_truncated",
                            if result.output_truncated {
                                "true"
                            } else {
                                "false"
                            },
                        ),
                        ("output_text_recorded", "false"),
                    ]),
                );
                executed_test_actions.push(action.action_id.to_string());
                executed_test_receipt_ids.push(test_receipt.receipt_id);
            }
        }

        // Everything not executed above is blocked. With no drivers, this is
        // every requested action - the loop's historical fail-closed behavior.
        let blocked_actions: Vec<&HarnessPlanExecutionAction<'_>> = request
            .requested_actions
            .iter()
            .filter(|action| {
                !applied_patch_actions
                    .iter()
                    .any(|id| id == action.action_id)
                    && !executed_test_actions
                        .iter()
                        .any(|id| id == action.action_id)
            })
            .collect();
        let blocked_dangerous_actions = blocked_actions
            .iter()
            .map(|action| format_harness_plan_execution_action(action))
            .collect::<Vec<_>>();
        let blocked_action_reports = blocked_actions
            .iter()
            .map(|action| format_harness_blocked_action_report(action))
            .collect::<Vec<_>>();
        let blocked_dangerous_actions_joined =
            join_owned_harness_values(&blocked_dangerous_actions);
        let patch_application_allowed = !applied_patch_actions.is_empty();
        let test_execution_allowed = !executed_test_actions.is_empty();
        let any_executed = patch_application_allowed || test_execution_allowed;
        let patch_application_flag = if patch_application_allowed {
            "true"
        } else {
            "false"
        };
        let status = if !any_executed {
            "EXECUTION_DENIED"
        } else if blocked_actions.is_empty() {
            "EXECUTION_COMPLETED"
        } else {
            "EXECUTION_PARTIAL"
        };

        let denial_decision =
            self.decide_with_receipt(&correlation, ActionKind::DenyHarnessExecution);
        let denial_receipt = self.transition_receipt(
            &harness_run_id,
            "EXECUTION_DENIED",
            &correlation,
            &denial_decision,
            "harness.execution.denied",
            payload(&[
                ("manifest_run_id", manifest.run_id),
                (
                    "plan_hash_approval_receipt_id",
                    &approval_receipt.receipt_id,
                ),
                (
                    "selected_provider_profile_receipt_id",
                    request.selected_provider_profile_receipt_id,
                ),
                (
                    "blocked_dangerous_actions",
                    &blocked_dangerous_actions_joined,
                ),
                ("reason", "local_plan_execute_loop_fail_closed"),
                ("runtime_execution_allowed", "false"),
                ("patch_application_allowed", patch_application_flag),
                ("max_tool_calls", &max_tool_calls),
                ("max_runtime_ms", &max_runtime_ms),
                ("max_output_tokens", &max_output_tokens),
            ]),
        );

        let mut quality_gate_receipt_ids = Vec::new();
        for gate in request.quality_gates {
            let gate_decision =
                self.decide_with_receipt(&correlation, ActionKind::RecordHarnessQualityGate);
            let gate_receipt = self.transition_receipt(
                &harness_run_id,
                "QUALITY_GATE_RECORDED",
                &correlation,
                &gate_decision,
                "harness.quality.gate.recorded",
                payload(&[
                    ("manifest_run_id", manifest.run_id),
                    ("quality_gate", gate.trim()),
                    ("status", "skipped_execution_closed"),
                    ("runtime_execution_allowed", "false"),
                    ("command_executed", "false"),
                    ("execution_denied_receipt_id", &denial_receipt.receipt_id),
                ]),
            );
            quality_gate_receipt_ids.push(gate_receipt.receipt_id);
        }

        let verdict_decision =
            self.decide_with_receipt(&correlation, ActionKind::RecordHarnessFinalVerdict);
        let verdict_receipt = self.transition_receipt(
            &harness_run_id,
            "FINAL_VERDICT_RECORDED",
            &correlation,
            &verdict_decision,
            "harness.final.verdict.recorded",
            payload(&[
                ("manifest_run_id", manifest.run_id),
                ("status", status),
                ("completed_quality_gates", "none"),
                ("failed_quality_gates", "none"),
                ("skipped_quality_gates", &skipped_quality_gates_joined),
                ("produced_artifacts", &produced_artifacts),
                (
                    "blocked_dangerous_actions",
                    &blocked_dangerous_actions_joined,
                ),
                ("final_verdict_criteria", &final_verdict_criteria),
                ("runtime_execution_allowed", "false"),
                ("patch_application_allowed", patch_application_flag),
            ]),
        );

        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == harness_run_id)
        {
            run.status = status.to_string();
        }
        self.storage
            .ledger()
            .verify()
            .map_err(HarnessPlanExecuteLoopError::ManifestRejected)?;

        let receipts = self.storage.ledger().entries()[starting_receipts..]
            .iter()
            .map(|entry| entry.receipt_id.clone())
            .collect();
        let policy_decision_ids = self.storage.policy_decisions()[starting_policy_decisions..]
            .iter()
            .map(|decision| decision.policy_decision_id.clone())
            .collect();
        let mut transcript = HarnessRunTranscript::new(manifest.run_id, &harness_run_id);
        transcript.append_item(
            HarnessRunItemKind::PlanHashApproval,
            approval_receipt.receipt_id.clone(),
            "approved plan hash recorded before local execution denial",
        );
        // Applied patches ride the replayable transcript too, so the workflow
        // layer's checkpoint and replay cover the execution event - patch
        // application is a workflow action, never a side channel.
        for receipt_id in &applied_patch_receipt_ids {
            transcript.append_item(
                HarnessRunItemKind::PatchApplied,
                receipt_id.clone(),
                "patch applied in an isolated worktree - live repo not mutated",
            );
        }
        for receipt_id in &executed_test_receipt_ids {
            transcript.append_item(
                HarnessRunItemKind::TestExecuted,
                receipt_id.clone(),
                "test command run in an ephemeral no-network sandbox container",
            );
        }
        transcript.append_item(
            HarnessRunItemKind::ExecutionDenied,
            denial_receipt.receipt_id.clone(),
            "remaining dangerous plan actions denied by local fail-closed runtime",
        );
        for receipt_id in &quality_gate_receipt_ids {
            transcript.append_item(
                HarnessRunItemKind::QualityGate,
                receipt_id.clone(),
                "quality gate recorded without command execution",
            );
        }
        transcript.append_item(
            HarnessRunItemKind::FinalVerdict,
            verdict_receipt.receipt_id.clone(),
            "final verdict recorded with blocked actions and skipped gates",
        );

        Ok(HarnessPlanExecuteLoopReport {
            manifest_run_id: manifest.run_id.to_string(),
            harness_run_id,
            status: status.to_string(),
            approved_plan_hash: request.approved_plan_hash.trim().to_string(),
            selected_provider_profile_receipt_id: request
                .selected_provider_profile_receipt_id
                .to_string(),
            plan_hash_approval_receipt_id: approval_receipt.receipt_id,
            execution_denied_receipt_id: denial_receipt.receipt_id,
            quality_gate_receipt_ids,
            final_verdict_receipt_id: verdict_receipt.receipt_id,
            blocked_dangerous_actions,
            blocked_action_reports,
            completed_quality_gates: Vec::new(),
            failed_quality_gates: Vec::new(),
            skipped_quality_gates: quality_gates,
            produced_artifacts: request
                .produced_artifacts
                .iter()
                .map(|artifact| artifact.trim().to_string())
                .collect(),
            runtime_execution_allowed: false,
            patch_application_allowed,
            applied_patch_actions,
            applied_patch_receipt_ids,
            test_execution_allowed,
            executed_test_actions,
            executed_test_receipt_ids,
            policy_decision_ids,
            receipts,
            transcript,
        })
    }
}

fn validate_harness_plan_execute_loop_request<S: StorageProvider>(
    kernel: &MdxKernel<S>,
    manifest: &HarnessRunManifest<'_>,
    request: &HarnessPlanExecuteLoopRequest<'_>,
) -> Result<(), HarnessPlanExecuteLoopError> {
    admit_harness_run_manifest(manifest)
        .map_err(|rejection| HarnessPlanExecuteLoopError::ManifestRejected(rejection.message()))?;
    for kind in [
        "harness.run.admitted",
        "harness.plan.emitted",
        "harness.provider.profile.selected",
        "harness.plan.hash.approved",
        "harness.execution.denied",
        "harness.quality.gate.recorded",
        "harness.final.verdict.recorded",
    ] {
        if !manifest.required_receipt_kinds.contains(&kind) {
            return Err(HarnessPlanExecuteLoopError::MissingRequiredReceiptKind(
                kind,
            ));
        }
    }
    if manifest.approval_mode != HarnessApprovalMode::PlanHash {
        return Err(HarnessPlanExecuteLoopError::ExecutionBoundaryOpen(
            "approval_mode",
        ));
    }
    if !manifest.approval_receipt_required {
        return Err(HarnessPlanExecuteLoopError::ExecutionBoundaryOpen(
            "approval_receipt_required",
        ));
    }
    if manifest.trust_boundary.approval_receipt_id.is_none() {
        return Err(HarnessPlanExecuteLoopError::MissingField(
            "approval_receipt_id",
        ));
    }
    validate_harness_plan_execute_boundary(manifest)?;
    validate_harness_plan_execute_request_fields(request)?;
    require_harness_receipt(kernel, request.admission_receipt_id, "harness.run.admitted")?;
    require_harness_receipt(kernel, request.plan_receipt_id, "harness.plan.emitted")?;
    require_harness_receipt(
        kernel,
        request.selected_provider_profile_receipt_id,
        "harness.provider.profile.selected",
    )?;
    Ok(())
}

fn validate_harness_plan_execute_boundary(
    manifest: &HarnessRunManifest<'_>,
) -> Result<(), HarnessPlanExecuteLoopError> {
    for (field, value) in [
        ("file_write", manifest.permission_policy.file_write),
        ("shell", manifest.permission_policy.shell),
        ("git", manifest.permission_policy.git),
        ("patch", manifest.permission_policy.patch),
        ("browser", manifest.permission_policy.browser),
        ("mcp", manifest.permission_policy.mcp),
        ("network", manifest.permission_policy.network),
    ] {
        if value != HarnessPermissionLevel::Forbidden {
            return Err(HarnessPlanExecuteLoopError::ExecutionBoundaryOpen(field));
        }
    }
    for (field, value) in [
        (
            "project_local_execution_allowed",
            manifest.trust_boundary.project_local_execution_allowed,
        ),
        (
            "mcp_startup_allowed",
            manifest.trust_boundary.mcp_startup_allowed,
        ),
        ("network_allowed", manifest.trust_boundary.network_allowed),
        (
            "secrets_available",
            manifest.trust_boundary.secrets_available,
        ),
    ] {
        if value {
            return Err(HarnessPlanExecuteLoopError::ExecutionBoundaryOpen(field));
        }
    }
    Ok(())
}

fn validate_harness_plan_execute_request_fields(
    request: &HarnessPlanExecuteLoopRequest<'_>,
) -> Result<(), HarnessPlanExecuteLoopError> {
    for (field, value) in [
        ("admission_receipt_id", request.admission_receipt_id),
        ("plan_receipt_id", request.plan_receipt_id),
        (
            "selected_provider_profile_receipt_id",
            request.selected_provider_profile_receipt_id,
        ),
    ] {
        if value.trim().is_empty() {
            return Err(HarnessPlanExecuteLoopError::MissingField(field));
        }
    }
    let approved_plan_hash = request.approved_plan_hash.trim();
    if approved_plan_hash.is_empty() {
        return Err(HarnessPlanExecuteLoopError::MissingField(
            "approved_plan_hash",
        ));
    }
    if approved_plan_hash.chars().any(char::is_whitespace) {
        return Err(HarnessPlanExecuteLoopError::InvalidPlanHash);
    }
    if request.requested_actions.is_empty() {
        return Err(HarnessPlanExecuteLoopError::EmptyList("requested_actions"));
    }
    if request.quality_gates.is_empty()
        || request
            .quality_gates
            .iter()
            .any(|gate| gate.trim().is_empty())
    {
        return Err(HarnessPlanExecuteLoopError::EmptyList("quality_gates"));
    }
    if request.final_verdict_criteria.is_empty()
        || request
            .final_verdict_criteria
            .iter()
            .any(|criterion| criterion.trim().is_empty())
    {
        return Err(HarnessPlanExecuteLoopError::EmptyList(
            "final_verdict_criteria",
        ));
    }
    for action in request.requested_actions {
        if action.action_id.trim().is_empty() {
            return Err(HarnessPlanExecuteLoopError::MissingField("action_id"));
        }
        if action.target.trim().is_empty() {
            return Err(HarnessPlanExecuteLoopError::MissingField("action_target"));
        }
        if action.summary.trim().is_empty() {
            return Err(HarnessPlanExecuteLoopError::MissingField("action_summary"));
        }
    }
    Ok(())
}

fn require_harness_receipt<S: StorageProvider>(
    kernel: &MdxKernel<S>,
    receipt_id: &str,
    expected_kind: &'static str,
) -> Result<(), HarnessPlanExecuteLoopError> {
    let receipt = kernel.storage.ledger().query().by_id(receipt_id).ok_or(
        HarnessPlanExecuteLoopError::MissingLedgerReceipt {
            receipt_id: receipt_id.to_string(),
            expected_kind,
        },
    )?;
    if receipt.kind != expected_kind {
        return Err(HarnessPlanExecuteLoopError::WrongLedgerReceiptKind {
            receipt_id: receipt_id.to_string(),
            expected_kind,
            observed_kind: receipt.kind.clone(),
        });
    }
    Ok(())
}

fn format_harness_plan_execution_action(action: &HarnessPlanExecutionAction<'_>) -> String {
    format!("{}:{}", action.action_kind.as_str(), action.target.trim())
}

// A patch target is in scope only if it sits under an allowed write-scope
// prefix and never under a blocked path. Empty or absolute-escaping targets
// are out of scope by construction.
fn path_within_harness_write_scope(
    target: &str,
    allowed_write_scope: &[&str],
    blocked_paths: &[&str],
) -> bool {
    if target.is_empty() || target.contains("..") || target.starts_with('/') {
        return false;
    }
    if blocked_paths
        .iter()
        .any(|blocked| target.starts_with(blocked.trim()))
    {
        return false;
    }
    allowed_write_scope
        .iter()
        .any(|scope| !scope.trim().is_empty() && target.starts_with(scope.trim()))
}

fn format_harness_blocked_action_report(
    action: &HarnessPlanExecutionAction<'_>,
) -> HarnessPlanBlockedActionReport {
    HarnessPlanBlockedActionReport {
        action_id: action.action_id.trim().to_string(),
        action_kind: action.action_kind.as_str().to_string(),
        target: action.target.trim().to_string(),
        summary: action.summary.trim().to_string(),
        status: "BLOCKED".to_string(),
        allowed: false,
        blocked_by: "local_plan_execute_loop_fail_closed".to_string(),
    }
}

fn join_owned_harness_values(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(",")
    }
}
