// Slice 3 (Track 1): the durable queue and its replay projection.
//
// When scale admission queues a worker run, that job must survive: it needs a
// stable identity and a recorded lifecycle so it can be resumed, cancelled, and
// audited - and so a replay can reconstruct why it is where it is. This is the
// orchestration record (WorkflowRunner's job, not the pipeline's): the queue
// lifecycle is written as receipts, and the projection is rebuilt purely from
// those receipts, never from in-memory state.
//
// Durability honesty: the backend here is the local ledger. Live durable
// execution (Temporal) is not observed, so the receipts say so - no false
// durability claim. The lifecycle and replay are real and local.
use crate::*;

/// The durable record of a queued worker job.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueuedJobSpec<'a> {
    pub job_id: &'a str,
    pub tenant_id: &'a str,
    pub priority: WorkerPriority,
    pub requested_worker_template: &'a str,
    pub envelope_id: &'a str,
    /// How many run attempts the job may make before it is exhausted.
    pub retry_budget: u32,
    /// After this long without progress a job is considered stale (the caller
    /// supplies wall-clock; the kernel records the bound, it has no clock).
    pub stale_timeout_ms: u64,
}

/// The replayed state of a job, reconstructed from its receipts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueueStateProjection {
    pub job_id: String,
    pub found: bool,
    pub current_state: String,
    pub tenant_id: String,
    pub priority: String,
    pub envelope_id: String,
    pub retry_budget: u32,
    pub attempts: u32,
    pub stale_timeout_ms: u64,
    pub cancelled: bool,
    pub resumable: bool,
    pub reason: String,
    /// The lifecycle receipt kinds in ledger order - the replay trail.
    pub event_kinds: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LocalDurableQueue;

impl LocalDurableQueue {
    pub fn enqueue<S: StorageProvider>(
        &self,
        kernel: &mut MdxKernel<S>,
        manifest: &HarnessRunManifest<'_>,
        spec: &QueuedJobSpec<'_>,
    ) -> Result<String, String> {
        kernel.record_queue_event(manifest, spec.job_id, "queue.job.enqueued", Some(spec), "")
    }

    pub fn dequeue<S: StorageProvider>(
        &self,
        kernel: &mut MdxKernel<S>,
        manifest: &HarnessRunManifest<'_>,
        job_id: &str,
    ) -> Result<String, String> {
        kernel.record_queue_event(manifest, job_id, "queue.job.dequeued", None, "")
    }

    pub fn mark_run_started<S: StorageProvider>(
        &self,
        kernel: &mut MdxKernel<S>,
        manifest: &HarnessRunManifest<'_>,
        job_id: &str,
    ) -> Result<String, String> {
        kernel.record_queue_event(manifest, job_id, "queue.run.started", None, "")
    }

    pub fn mark_terminal<S: StorageProvider>(
        &self,
        kernel: &mut MdxKernel<S>,
        manifest: &HarnessRunManifest<'_>,
        job_id: &str,
        terminal_state: &str,
    ) -> Result<String, String> {
        kernel.record_queue_event(manifest, job_id, "queue.run.terminal", None, terminal_state)
    }

    pub fn cancel<S: StorageProvider>(
        &self,
        kernel: &mut MdxKernel<S>,
        manifest: &HarnessRunManifest<'_>,
        job_id: &str,
    ) -> Result<String, String> {
        kernel.record_queue_event(manifest, job_id, "queue.job.cancelled", None, "")
    }

    /// Rebuild a job's state from its receipts alone (the replay).
    pub fn project<S: StorageProvider>(
        &self,
        kernel: &MdxKernel<S>,
        job_id: &str,
    ) -> QueueStateProjection {
        kernel.project_queue_state(job_id)
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    fn record_queue_event(
        &mut self,
        manifest: &HarnessRunManifest<'_>,
        job_id: &str,
        kind: &'static str,
        spec: Option<&QueuedJobSpec<'_>>,
        terminal_state: &str,
    ) -> Result<String, String> {
        if job_id.trim().is_empty() {
            return Err("missing_job_id".to_string());
        }
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(manifest.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(manifest.actor_id),
            // The durable queue is WorkflowRunner orchestration, not pipeline work.
            loop_id: LoopId::new("workflow_runner_durable_queue"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("durable_queue");
        let decision = self.decide_with_receipt(&correlation, ActionKind::RecordDurableQueueEvent);
        let mut fields: Vec<(&str, &str)> = vec![
            ("job_id", job_id),
            ("workflow_runner_driver", "local_workflow_runner"),
            // Honest durability posture: local ledger, no live durable execution.
            ("durable_backend", "local_ledger"),
            ("live_durability_observed", "false"),
        ];
        let (retry_budget, stale_timeout, priority);
        if let Some(spec) = spec {
            retry_budget = spec.retry_budget.to_string();
            stale_timeout = spec.stale_timeout_ms.to_string();
            priority = spec.priority.as_str();
            fields.push(("tenant_id", spec.tenant_id));
            fields.push(("priority", priority));
            fields.push(("requested_worker_template", spec.requested_worker_template));
            fields.push(("envelope_id", spec.envelope_id));
            fields.push(("retry_budget", &retry_budget));
            fields.push(("stale_timeout_ms", &stale_timeout));
            fields.push(("cancellation_flag", "false"));
        }
        if !terminal_state.is_empty() {
            fields.push(("terminal_state", terminal_state));
        }
        let receipt = self.transition_receipt(
            &run_id,
            kind,
            &correlation,
            &decision,
            kind,
            payload(&fields),
        );
        self.storage.ledger().verify()?;
        Ok(receipt.receipt_id)
    }

    pub fn project_queue_state(&self, job_id: &str) -> QueueStateProjection {
        let job_id = job_id.trim();
        let mut projection = QueueStateProjection {
            job_id: job_id.to_string(),
            found: false,
            current_state: "UNKNOWN".to_string(),
            tenant_id: String::new(),
            priority: String::new(),
            envelope_id: String::new(),
            retry_budget: 0,
            attempts: 0,
            stale_timeout_ms: 0,
            cancelled: false,
            resumable: false,
            reason: "no durable record for this job".to_string(),
            event_kinds: Vec::new(),
        };
        let mut enqueued = false;
        let mut dequeued = false;
        let mut terminal_state = String::new();
        // Replay: walk the job's lifecycle receipts in ledger order.
        for receipt in self.storage.ledger().entries() {
            if receipt.payload.get("job_id").map(String::as_str) != Some(job_id) {
                continue;
            }
            if !receipt.kind.starts_with("queue.") {
                continue;
            }
            projection.found = true;
            projection.event_kinds.push(receipt.kind.clone());
            match receipt.kind.as_str() {
                "queue.job.enqueued" => {
                    enqueued = true;
                    let get = |k: &str| receipt.payload.get(k).cloned().unwrap_or_default();
                    projection.tenant_id = get("tenant_id");
                    projection.priority = get("priority");
                    projection.envelope_id = get("envelope_id");
                    projection.retry_budget = get("retry_budget").parse().unwrap_or(0);
                    projection.stale_timeout_ms = get("stale_timeout_ms").parse().unwrap_or(0);
                }
                "queue.job.dequeued" => dequeued = true,
                "queue.run.started" => projection.attempts += 1,
                "queue.run.terminal" => {
                    terminal_state = receipt
                        .payload
                        .get("terminal_state")
                        .cloned()
                        .unwrap_or_default();
                }
                "queue.job.cancelled" => projection.cancelled = true,
                _ => {}
            }
        }
        if !projection.found {
            return projection;
        }
        // Determine the current state and why, latest signal first.
        let (state, reason) = if projection.cancelled {
            ("CANCELLED", "the job was cancelled".to_string())
        } else if !terminal_state.is_empty() {
            (
                terminal_state_label(&terminal_state),
                format!("the run reached terminal state {terminal_state}"),
            )
        } else if projection.attempts > 0 {
            ("RUNNING", "a worker is running this job".to_string())
        } else if dequeued {
            (
                "DEQUEUED",
                "the job left the queue, awaiting run start".to_string(),
            )
        } else if enqueued {
            (
                "ENQUEUED",
                "admitted to the queue under backpressure, awaiting a worker slot".to_string(),
            )
        } else {
            ("UNKNOWN", projection.reason.clone())
        };
        projection.current_state = state.to_string();
        projection.reason = reason;
        // Resumable: still open, not cancelled, and attempts remain in budget.
        let open = !matches!(state, "COMPLETED" | "ESCALATED" | "CANCELLED");
        projection.resumable =
            open && !projection.cancelled && projection.attempts < projection.retry_budget;
        projection
    }
}

fn terminal_state_label(terminal_state: &str) -> &'static str {
    match terminal_state {
        "completed" => "COMPLETED",
        "escalated" => "ESCALATED",
        "failed" => "FAILED",
        "shed" => "SHED",
        _ => "TERMINAL",
    }
}
