// The governed worker run: the integration unlock. A worker runs a multi-step
// build (apply a patch, then run tests) under a lease - leased, scoped,
// budgeted, retired, and unable to smuggle authority. It does not invent a new
// execution path: it composes the already-proven plan-execute loop drivers
// (patch applier + sandbox runner), wraps them in the worker lifecycle
// (delegated -> build -> completed/failed -> handoff -> retired), and records
// every step as a replay-covered receipt. The kernel never spawns an OS
// process or opens authority; the worker is a bounded run over governed
// actions. Admission goes through the full live-worker-execution governance
// packet, and only PatchApplication and TestExecution build steps are allowed.
use crate::*;

#[derive(Clone, Copy)]
pub struct HarnessWorkerRunRequest<'a> {
    /// The full governed worker-execution admission packet (lease, credential,
    /// handoff, retirement, provider turn-on, dispatch, heartbeat, durable
    /// workflow, sandbox authority, tool policy, reviewer separation).
    pub admission: LiveWorkerExecutionAdmission<'a>,
    pub parent_loop_id: &'a str,
    pub worker_template_id: &'a str,
    /// Lease budget: the maximum number of build steps the worker may run.
    pub max_steps: usize,
    /// The multi-step build the worker runs - patch and test actions only.
    pub build: &'a HarnessPlanExecuteLoopRequest<'a>,
    /// Handoff evidence the worker leaves for its next owner.
    pub output_artifacts: &'a [&'a str],
    pub handoff_summary: &'a str,
    pub next_owner: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HarnessWorkerRunReport {
    pub worker_run_id: String,
    pub status: &'static str,
    pub denied_reason: Option<String>,
    pub delegated_receipt_id: String,
    pub build_status: String,
    pub completion_receipt_id: String,
    pub handoff_receipt_id: String,
    pub retirement_receipt_id: String,
    pub steps_executed: usize,
    pub within_budget: bool,
    pub authority_smuggled: bool,
    pub build_receipt_ids: Vec<String>,
    pub receipt_ids: Vec<String>,
}

/// The scale/backpressure admission context for a worker run. The kernel has no
/// live scheduler; the server runtime supplies the observed capacity counts and
/// the standing policy, and the kernel records the admission decision against
/// them before any worker is delegated.
#[derive(Clone, Copy)]
pub struct WorkerScaleAdmissionContext<'a> {
    pub policy_id: &'a str,
    pub policy: ScaleBackpressurePolicy,
    pub priority: WorkerPriority,
    /// Worker builds running right now across all tenants (observed).
    pub global_running: u32,
    /// Builds queued right now across all tenants (observed).
    pub global_queued: u32,
    /// Builds running right now for this run's tenant (observed).
    pub tenant_running: u32,
}

/// The outcome of scale admission. A worker only runs when admitted; queue and
/// shed return without starting one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkerAdmissionReport {
    pub status: &'static str,
    pub decision: String,
    pub reason: Option<String>,
    pub admission_receipt_id: String,
    /// The position this job would take in the queue, when queued.
    pub queue_position: Option<u32>,
    /// The worker run, present only when admitted and executed.
    pub run: Option<HarnessWorkerRunReport>,
    pub receipt_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HarnessWorkerRunError {
    PlanExecuteFailed(String),
    Internal(String),
}

impl HarnessWorkerRunError {
    pub fn message(&self) -> String {
        match self {
            Self::PlanExecuteFailed(message) => message.clone(),
            Self::Internal(message) => message.clone(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LocalHarnessWorkerRun;

impl LocalHarnessWorkerRun {
    pub fn run<S: StorageProvider>(
        &self,
        kernel: &mut MdxKernel<S>,
        manifest: &HarnessRunManifest<'_>,
        request: &HarnessWorkerRunRequest<'_>,
        drivers: HarnessExecutionDrivers<'_>,
    ) -> Result<HarnessWorkerRunReport, HarnessWorkerRunError> {
        kernel.run_harness_worker_run_with_drivers(manifest, request, drivers)
    }

    /// The production worker-admission entry point: classify the run against the
    /// scale/backpressure policy on observed counts, record the decision, and
    /// only delegate a worker when the decision is admit. Queue and shed return
    /// without starting a worker.
    pub fn admit_and_run<S: StorageProvider>(
        &self,
        kernel: &mut MdxKernel<S>,
        manifest: &HarnessRunManifest<'_>,
        scale: &WorkerScaleAdmissionContext<'_>,
        request: &HarnessWorkerRunRequest<'_>,
        drivers: HarnessExecutionDrivers<'_>,
    ) -> Result<WorkerAdmissionReport, HarnessWorkerRunError> {
        kernel.admit_and_run_harness_worker(manifest, scale, request, drivers)
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn admit_and_run_harness_worker(
        &mut self,
        manifest: &HarnessRunManifest<'_>,
        scale: &WorkerScaleAdmissionContext<'_>,
        request: &HarnessWorkerRunRequest<'_>,
        drivers: HarnessExecutionDrivers<'_>,
    ) -> Result<WorkerAdmissionReport, HarnessWorkerRunError> {
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(manifest.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(manifest.actor_id),
            loop_id: LoopId::new("harness_worker_admission"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let admission_run_id = self.ids.next("worker_admission");
        let decision = classify_scale_admission(
            &scale.policy,
            &ScaleAdmissionRequest {
                tenant_id: manifest.tenant_id,
                priority: scale.priority,
                global_running: scale.global_running,
                global_queued: scale.global_queued,
                tenant_running: scale.tenant_running,
            },
        );
        // The position a queued job would take: behind everything already queued.
        let queue_position = if matches!(decision, ScaleAdmissionDecision::Queue(_)) {
            Some(scale.global_queued + 1)
        } else {
            None
        };
        let (status, receipt_kind) = match &decision {
            ScaleAdmissionDecision::Admit => ("WORKER_ADMITTED", "worker.admission.admitted"),
            ScaleAdmissionDecision::Queue(_) => ("WORKER_QUEUED", "worker.admission.queued"),
            ScaleAdmissionDecision::Shed(_) => ("WORKER_SHED", "worker.admission.shed"),
        };
        let global_running = scale.global_running.to_string();
        let global_queued = scale.global_queued.to_string();
        let tenant_running = scale.tenant_running.to_string();
        let queue_position_s = queue_position.map(|p| p.to_string()).unwrap_or_default();
        let reason = decision.reason().unwrap_or("admitted").to_string();
        let admit_decision =
            self.decide_with_receipt(&correlation, ActionKind::AdmitHarnessWorkerScale);
        let admission = self.transition_receipt(
            &admission_run_id,
            status,
            &correlation,
            &admit_decision,
            receipt_kind,
            payload(&[
                ("worker_run_id", request.admission.worker_run_id),
                ("policy_id", scale.policy_id),
                ("tenant_id", manifest.tenant_id),
                ("priority", scale.priority.as_str()),
                ("decision", decision.as_str()),
                ("decision_reason", &reason),
                ("global_running", &global_running),
                ("global_queued", &global_queued),
                ("tenant_running", &tenant_running),
                (
                    "max_concurrent_workers",
                    &scale.policy.max_concurrent_workers.to_string(),
                ),
                ("queue_position", &queue_position_s),
                (
                    "worker_started",
                    if decision.is_admit() { "true" } else { "false" },
                ),
            ]),
        );
        self.storage
            .ledger()
            .verify()
            .map_err(HarnessWorkerRunError::Internal)?;

        // Only an admit decision delegates a worker. Queue and shed stop here -
        // no worker starts, by construction.
        if !decision.is_admit() {
            return Ok(WorkerAdmissionReport {
                status,
                decision: decision.as_str().to_string(),
                reason: Some(reason),
                admission_receipt_id: admission.receipt_id.clone(),
                queue_position,
                run: None,
                receipt_ids: vec![admission.receipt_id],
            });
        }

        let run = self.run_harness_worker_run_with_drivers(manifest, request, drivers)?;
        let mut receipt_ids = vec![admission.receipt_id.clone()];
        receipt_ids.extend(run.receipt_ids.iter().cloned());
        Ok(WorkerAdmissionReport {
            status,
            decision: decision.as_str().to_string(),
            reason: None,
            admission_receipt_id: admission.receipt_id,
            queue_position: None,
            run: Some(run),
            receipt_ids,
        })
    }

    pub fn run_harness_worker_run_with_drivers(
        &mut self,
        manifest: &HarnessRunManifest<'_>,
        request: &HarnessWorkerRunRequest<'_>,
        drivers: HarnessExecutionDrivers<'_>,
    ) -> Result<HarnessWorkerRunReport, HarnessWorkerRunError> {
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(manifest.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(manifest.actor_id),
            loop_id: LoopId::new("harness_worker_run"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let worker_run_id = if request.admission.worker_run_id.trim().is_empty() {
            self.ids.next("worker_run")
        } else {
            request.admission.worker_run_id.to_string()
        };
        let mut receipt_ids: Vec<String> = Vec::new();

        // The lease gate: the full governed worker-execution packet must be
        // present, and the build may contain only patch and test steps within
        // the step budget. Anything else is an attempt to smuggle authority.
        let denial = self.judge_worker_run(request);
        if let Some(reason) = denial {
            let decision =
                self.decide_with_receipt(&correlation, ActionKind::DelegateHarnessWorker);
            let receipt = self.transition_receipt(
                &worker_run_id,
                "WORKER_RUN_DENIED",
                &correlation,
                &decision,
                "worker.run.denied",
                payload(&[
                    ("worker_run_id", &worker_run_id),
                    ("parent_loop_id", request.parent_loop_id),
                    ("denied_reason", &reason),
                    ("authority_smuggled", "false"),
                ]),
            );
            self.storage
                .ledger()
                .verify()
                .map_err(HarnessWorkerRunError::Internal)?;
            return Ok(HarnessWorkerRunReport {
                worker_run_id,
                status: "WORKER_RUN_DENIED",
                denied_reason: Some(reason),
                delegated_receipt_id: receipt.receipt_id.clone(),
                build_status: "NOT_RUN".to_string(),
                completion_receipt_id: String::new(),
                handoff_receipt_id: String::new(),
                retirement_receipt_id: String::new(),
                steps_executed: 0,
                within_budget: true,
                authority_smuggled: false,
                build_receipt_ids: Vec::new(),
                receipt_ids: vec![receipt.receipt_id],
            });
        }

        // 1. Delegation: the worker is handed the build, under its lease.
        let step_count = request.build.requested_actions.len().to_string();
        let max_steps = request.max_steps.to_string();
        let delegate_decision =
            self.decide_with_receipt(&correlation, ActionKind::DelegateHarnessWorker);
        let delegated = self.transition_receipt(
            &worker_run_id,
            "WORKER_DELEGATED",
            &correlation,
            &delegate_decision,
            "worker.delegated",
            payload(&[
                ("worker_run_id", &worker_run_id),
                ("parent_loop_id", request.parent_loop_id),
                ("worker_template_id", request.worker_template_id),
                ("spawn_receipt_id", request.admission.spawn_receipt_id),
                (
                    "credential_check_receipt_id",
                    request.admission.credential_check_receipt_id,
                ),
                ("build_step_count", &step_count),
                ("max_steps", &max_steps),
                (
                    "lease_scope",
                    manifest.allowed_write_scope.first().unwrap_or(&""),
                ),
            ]),
        );
        receipt_ids.push(delegated.receipt_id.clone());

        // 2. The build itself: the proven plan-execute loop with drivers. This
        // composes patch application (isolated worktree) and test execution
        // (sandbox); the worker opens no new path.
        let build_report = self
            .run_harness_plan_execute_loop_with_drivers(manifest, request.build, drivers)
            .map_err(|error| HarnessWorkerRunError::PlanExecuteFailed(error.message()))?;
        let build_receipt_ids = build_report.receipts.clone();
        let steps_executed =
            build_report.applied_patch_actions.len() + build_report.executed_test_actions.len();
        let within_budget = steps_executed <= request.max_steps;

        // 3. Outcome: completed only when the build ran every step and every
        // test that ran passed. Otherwise failed - honestly.
        let tests_passed = build_report.executed_test_receipt_ids.iter().all(|id| {
            self.storage
                .ledger()
                .query()
                .by_id(id)
                .map(|receipt| receipt.payload.get("passed").map(String::as_str) == Some("true"))
                .unwrap_or(false)
        });
        let completed = build_report.status == "EXECUTION_COMPLETED" && tests_passed;
        let status = if completed {
            "WORKER_BUILD_COMPLETED"
        } else {
            "WORKER_BUILD_FAILED"
        };
        let steps_executed_str = steps_executed.to_string();
        let completion_decision =
            self.decide_with_receipt(&correlation, ActionKind::RecordHarnessWorkerBuild);
        let completion = self.transition_receipt(
            &worker_run_id,
            status,
            &correlation,
            &completion_decision,
            "worker.build.recorded",
            payload(&[
                ("worker_run_id", &worker_run_id),
                (
                    "approved_plan_hash",
                    request.build.approved_plan_hash.trim(),
                ),
                ("build_status", &build_report.status),
                ("worker_status", status),
                ("steps_executed", &steps_executed_str),
                ("max_steps", &max_steps),
                (
                    "within_budget",
                    if within_budget { "true" } else { "false" },
                ),
                ("tests_passed", if tests_passed { "true" } else { "false" }),
                (
                    "build_final_verdict_receipt_id",
                    &build_report.final_verdict_receipt_id,
                ),
                ("authority_smuggled", "false"),
            ]),
        );
        receipt_ids.push(completion.receipt_id.clone());

        // 4. Handoff: worker outputs become company state only with handoff
        // evidence citing the build. 5. Retirement: the lease is released.
        let verification_evidence: Vec<&str> = if build_report.executed_test_receipt_ids.is_empty()
        {
            vec![completion.receipt_id.as_str()]
        } else {
            build_report
                .executed_test_receipt_ids
                .iter()
                .map(String::as_str)
                .collect()
        };
        let handoff_admission = WorkerHandoffAdmission {
            parent_loop_id: request.parent_loop_id,
            worker_template_id: request.worker_template_id,
            worker_run_id: &worker_run_id,
            spawn_receipt_id: request.admission.spawn_receipt_id,
            credential_check_receipt_id: request.admission.credential_check_receipt_id,
            output_artifacts: request.output_artifacts,
            verification_evidence: &verification_evidence,
            summary: request.handoff_summary,
            next_owner: request.next_owner,
            requested_receipt_kind: "worker.handoff.recorded",
        };
        admit_worker_handoff(&handoff_admission)
            .map_err(|rejection| HarnessWorkerRunError::Internal(rejection.message()))?;
        let handoff_decision =
            self.decide_with_receipt(&correlation, ActionKind::RecordHarnessWorkerHandoff);
        let handoff = self.transition_receipt(
            &worker_run_id,
            "WORKER_HANDOFF_RECORDED",
            &correlation,
            &handoff_decision,
            "worker.handoff.recorded",
            payload(&[
                ("worker_run_id", &worker_run_id),
                ("parent_loop_id", request.parent_loop_id),
                ("spawn_receipt_id", request.admission.spawn_receipt_id),
                ("worker_build_receipt_id", &completion.receipt_id),
                ("summary", request.handoff_summary),
                ("next_owner", request.next_owner),
                (
                    "output_artifact_count",
                    &request.output_artifacts.len().to_string(),
                ),
            ]),
        );
        receipt_ids.push(handoff.receipt_id.clone());

        let retirement_admission = WorkerRetirementAdmission {
            parent_loop_id: request.parent_loop_id,
            worker_template_id: request.worker_template_id,
            worker_run_id: &worker_run_id,
            spawn_receipt_id: request.admission.spawn_receipt_id,
            handoff_receipt_id: &handoff.receipt_id,
            requested_receipt_kind: "worker.retired",
        };
        admit_worker_retirement(&retirement_admission)
            .map_err(|rejection| HarnessWorkerRunError::Internal(rejection.message()))?;
        let retire_decision =
            self.decide_with_receipt(&correlation, ActionKind::RetireHarnessWorker);
        let retirement = self.transition_receipt(
            &worker_run_id,
            "WORKER_RETIRED",
            &correlation,
            &retire_decision,
            "worker.retired",
            payload(&[
                ("worker_run_id", &worker_run_id),
                ("parent_loop_id", request.parent_loop_id),
                ("handoff_receipt_id", &handoff.receipt_id),
                ("lease_released", "true"),
            ]),
        );
        receipt_ids.push(retirement.receipt_id.clone());

        self.storage
            .ledger()
            .verify()
            .map_err(HarnessWorkerRunError::Internal)?;

        Ok(HarnessWorkerRunReport {
            worker_run_id,
            status,
            denied_reason: None,
            delegated_receipt_id: delegated.receipt_id,
            build_status: build_report.status.clone(),
            completion_receipt_id: completion.receipt_id,
            handoff_receipt_id: handoff.receipt_id,
            retirement_receipt_id: retirement.receipt_id,
            steps_executed,
            within_budget,
            authority_smuggled: false,
            build_receipt_ids,
            receipt_ids,
        })
    }

    // The lease gate. Returns a denial reason, or None when the worker may run.
    fn judge_worker_run(&self, request: &HarnessWorkerRunRequest<'_>) -> Option<String> {
        if let Err(rejection) = admit_live_worker_execution(&request.admission) {
            return Some(format!("worker_execution_admission:{rejection:?}"));
        }
        if request.max_steps == 0 {
            return Some("empty_lease_budget".to_string());
        }
        if request.build.requested_actions.is_empty() {
            return Some("empty_build".to_string());
        }
        if request.build.requested_actions.len() > request.max_steps {
            return Some("build_exceeds_lease_budget".to_string());
        }
        // No authority smuggling: a worker build runs ONLY patch and test
        // steps. Any other action kind is refused before delegation.
        for action in request.build.requested_actions {
            match action.action_kind {
                HarnessPlanExecutionActionKind::PatchApplication
                | HarnessPlanExecutionActionKind::TestExecution => {}
                other => {
                    return Some(format!("authority_smuggling_attempt:{}", other.as_str()));
                }
            }
        }
        None
    }
}
