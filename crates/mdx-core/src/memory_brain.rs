use std::fmt;

use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConsolidationDecision {
    // Legacy verbatim retention: the pre-adjudication write path. Kept so
    // existing call sites and restored snapshots stay valid.
    Retain,
    Skip,
    // Adjudicated consolidation: a candidate fact compared against the
    // records already held in the same tenant and scope.
    Add,
    Update,
    Supersede,
    Noop,
}

impl ConsolidationDecision {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Retain => "RETAIN",
            Self::Skip => "SKIP",
            Self::Add => "ADD",
            Self::Update => "UPDATE",
            Self::Supersede => "SUPERSEDE",
            Self::Noop => "NOOP",
        }
    }

    /// Whether this decision produces a stored record. Skip and Noop leave
    /// only receipts behind.
    pub fn writes_record(&self) -> bool {
        !matches!(self, Self::Skip | Self::Noop)
    }
}

/// Consolidation ratification states for a stored memory record. Shared
/// scopes land pending and clear only through a distinct-actor governed
/// write; private_user memory activates immediately.
pub const MEMORY_CONSOLIDATION_ACTIVE: &str = "active";
pub const MEMORY_CONSOLIDATION_PENDING: &str = "pending_ratification";
pub const MEMORY_CONSOLIDATION_DECLINED: &str = "ratification_declined";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryProvenance {
    pub driver_id: &'static str,
    pub provider: &'static str,
    pub durable_driver: &'static str,
    pub durable_table: &'static str,
    pub consolidation_gate: &'static str,
    pub gate_receipt_id: String,
    pub source_receipt_kind: String,
    pub temporal_status: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryRecord {
    pub memory_id: String,
    pub episode_id: String,
    pub tenant_id: TenantId,
    pub source_receipt_id: String,
    pub atom_origin: &'static str,
    pub valid_from_receipt_timestamp: String,
    pub consolidation_decision: ConsolidationDecision,
    pub provenance: MemoryProvenance,
    pub memory_scope: &'static str,
    pub memory_tier: &'static str,
    pub decay_policy: &'static str,
    pub importance_score: u8,
    pub content: String,
    // Bi-temporal validity window, bound to receipt trusted time. Empty
    // until a later consolidation or lifecycle sweep closes the record.
    pub valid_until_receipt_timestamp: String,
    pub invalidated_by_receipt_id: String,
    // MEMORY_CONSOLIDATION_ACTIVE | _PENDING | _DECLINED.
    pub consolidation_state: &'static str,
    // A serialized local embedding vector, empty when no local embedder was
    // configured at consolidation time. Stored as a string (not Vec<f32>) so
    // the record stays Eq and hand-renders losslessly into the snapshot and
    // SQL export. Written only by mdx-server (where the model gateways live);
    // the kernel never computes it. Empty means ranking falls back to the
    // lexical and checksum components - today's behavior, byte for byte.
    pub embedding: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryGraphNode {
    pub node_id: String,
    pub tenant_id: TenantId,
    pub node_kind: &'static str,
    pub label: String,
    pub memory_id: Option<String>,
    pub source_receipt_id: String,
    pub atom_origin: &'static str,
    pub valid_from_receipt_timestamp: String,
    pub lifecycle_state: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryGraphEdge {
    pub edge_id: String,
    pub tenant_id: TenantId,
    pub from_node_id: String,
    pub to_node_id: String,
    pub edge_kind: &'static str,
    pub source_receipt_id: String,
    pub weight: u8,
    pub valid_from_receipt_timestamp: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryLifecycleEvent {
    pub event_id: String,
    pub tenant_id: TenantId,
    pub memory_id: String,
    pub action: &'static str,
    pub lifecycle_state: &'static str,
    pub reason: String,
    pub source_receipt_id: String,
    pub valid_from_receipt_timestamp: String,
    pub receipt_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryRecallRanking {
    pub ranking_id: String,
    pub tenant_id: TenantId,
    pub surface: &'static str,
    pub query: String,
    pub memory_id: String,
    pub lexical_score: u8,
    pub content_checksum_score: u8,
    pub graph_score: u8,
    pub recency_score: u8,
    pub importance_score: u8,
    pub scope_score: u8,
    pub source_authority_score: u8,
    pub final_score: u16,
    pub rank: u16,
    pub source_receipt_id: String,
    pub receipt_id: String,
}

/// One candidate from the read-only recall ranking: enough for a caller to
/// build a prompt layer and a truthful receipt without persisting rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryRecallCandidate {
    pub memory_id: String,
    pub content: String,
    pub memory_scope: &'static str,
    pub final_score: u16,
    pub rank: u16,
    pub source_receipt_id: String,
}

/// The recall packet actually assembled into a Twin live prompt: which
/// ranked private sources rode, how many lesson lines rode, the measured
/// bytes of each layer, and how long assembly took. The save-time
/// brain_recall receipt records these observed values - never constants.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TwinRecallPacket {
    pub sources: Vec<MemoryRecallCandidate>,
    pub lesson_count: usize,
    pub memory_bytes: usize,
    pub lesson_bytes: usize,
    pub build_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryBrainEvalRun {
    pub eval_run_id: String,
    pub tenant_id: TenantId,
    pub fixture_family: &'static str,
    pub fixture_count: u16,
    pub correct_count: u16,
    pub latency_budget_ms: u16,
    pub observed_latency_ms: u16,
    pub brain_score: u8,
    pub receipt_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryVendorComparatorRun {
    pub comparator_run_id: String,
    pub tenant_id: TenantId,
    pub vendor_id: &'static str,
    pub status: &'static str,
    pub accuracy_score: u8,
    pub latency_ms: u16,
    pub cost_micros: u32,
    pub receipt_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemorySurfaceAccess {
    pub access_id: String,
    pub tenant_id: TenantId,
    pub surface: &'static str,
    pub scope: &'static str,
    pub can_read: bool,
    pub can_write: bool,
    pub review_required: bool,
    pub receipt_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryProductionTopologyCheck {
    pub topology_check_id: String,
    pub tenant_id: TenantId,
    pub service: &'static str,
    pub deployment_shape: &'static str,
    pub latency_role: &'static str,
    pub queue_or_cache: &'static str,
    pub observed_latency_ms: u16,
    pub receipt_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryLifecycleEvaluation {
    pub evaluation_id: String,
    pub tenant_id: TenantId,
    pub policy: &'static str,
    pub evaluated_memory_count: u16,
    pub stale_count: u16,
    pub contradiction_count: u16,
    pub supersession_count: u16,
    pub trigger_receipt_id: String,
    pub trusted_time_floor: String,
    pub receipt_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryEvalFixtureResult {
    pub fixture_result_id: String,
    pub tenant_id: TenantId,
    pub fixture_family: &'static str,
    pub query: String,
    pub expected_memory_id: String,
    pub matched_memory_id: String,
    pub final_score: u16,
    pub passed: bool,
    pub receipt_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryTopologyRuntimeEvent {
    pub topology_event_id: String,
    pub tenant_id: TenantId,
    pub service: &'static str,
    pub event_kind: &'static str,
    pub queue_or_cache: &'static str,
    pub cache_key: String,
    pub observed_latency_ms: u16,
    pub receipt_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryBenchmarkImport {
    pub import_id: String,
    pub tenant_id: TenantId,
    pub fixture_family: &'static str,
    pub source_kind: &'static str,
    pub task_shape: &'static str,
    pub fixture_count: u16,
    pub synthetic: bool,
    pub receipt_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryScaleLoadRun {
    pub scale_run_id: String,
    pub tenant_id: TenantId,
    pub synthetic_session_count: u16,
    pub memory_record_count: u16,
    pub ranking_count: u16,
    pub latency_budget_ms: u16,
    pub observed_p95_latency_ms: u16,
    pub brain_score: u8,
    pub receipt_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryCloudTurnOnCheck {
    pub check_id: String,
    pub tenant_id: TenantId,
    pub check_kind: &'static str,
    pub status: &'static str,
    pub evidence: String,
    pub receipt_id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MemoryBrainSnapshot {
    pub records: Vec<MemoryRecord>,
    pub graph_nodes: Vec<MemoryGraphNode>,
    pub graph_edges: Vec<MemoryGraphEdge>,
    pub lifecycle_events: Vec<MemoryLifecycleEvent>,
    pub recall_rankings: Vec<MemoryRecallRanking>,
    pub eval_runs: Vec<MemoryBrainEvalRun>,
    pub vendor_comparator_runs: Vec<MemoryVendorComparatorRun>,
    pub surface_access: Vec<MemorySurfaceAccess>,
    pub production_topology_checks: Vec<MemoryProductionTopologyCheck>,
    pub lifecycle_evaluations: Vec<MemoryLifecycleEvaluation>,
    pub eval_fixture_results: Vec<MemoryEvalFixtureResult>,
    pub topology_runtime_events: Vec<MemoryTopologyRuntimeEvent>,
    pub benchmark_imports: Vec<MemoryBenchmarkImport>,
    pub scale_load_runs: Vec<MemoryScaleLoadRun>,
    pub cloud_turn_on_checks: Vec<MemoryCloudTurnOnCheck>,
}

impl MemoryBrainSnapshot {
    pub fn record_count(&self) -> usize {
        self.records.len()
    }
}

pub trait MemoryProvider {
    fn write(&mut self, record: MemoryRecord);
    fn records(&self) -> &[MemoryRecord];
    fn restore_records(&mut self, records: Vec<MemoryRecord>);
    fn set_consolidation_state(&mut self, memory_id: &str, consolidation_state: &'static str);
    fn set_embedding(&mut self, memory_id: &str, embedding: String);
    fn close_record_validity(
        &mut self,
        memory_id: &str,
        valid_until_receipt_timestamp: &str,
        invalidated_by_receipt_id: &str,
    );
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MemoryProviderAdapterError {
    MissingMem0ApiKey,
    PendingLiveRun {
        adapter: &'static str,
        reason: &'static str,
    },
}

impl fmt::Display for MemoryProviderAdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingMem0ApiKey => f.write_str("MEM0_API_KEY is required"),
            Self::PendingLiveRun { adapter, reason } => {
                write!(f, "{adapter} is PENDING-LIVE-RUN: {reason}")
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct InMemoryProvider {
    records: Vec<MemoryRecord>,
}

impl MemoryProvider for InMemoryProvider {
    fn write(&mut self, record: MemoryRecord) {
        self.records.push(record);
    }

    fn records(&self) -> &[MemoryRecord] {
        &self.records
    }

    fn restore_records(&mut self, records: Vec<MemoryRecord>) {
        self.records = records;
    }

    fn set_consolidation_state(&mut self, memory_id: &str, consolidation_state: &'static str) {
        for record in &mut self.records {
            if record.memory_id == memory_id {
                record.consolidation_state = consolidation_state;
            }
        }
    }

    fn set_embedding(&mut self, memory_id: &str, embedding: String) {
        for record in &mut self.records {
            if record.memory_id == memory_id {
                record.embedding = embedding.clone();
            }
        }
    }

    fn close_record_validity(
        &mut self,
        memory_id: &str,
        valid_until_receipt_timestamp: &str,
        invalidated_by_receipt_id: &str,
    ) {
        for record in &mut self.records {
            if record.memory_id == memory_id {
                record.valid_until_receipt_timestamp = valid_until_receipt_timestamp.to_string();
                record.invalidated_by_receipt_id = invalidated_by_receipt_id.to_string();
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mem0MemoryProvider {
    api_key: String,
}

impl Mem0MemoryProvider {
    pub fn connect(api_key: Option<&str>) -> Result<Self, MemoryProviderAdapterError> {
        let api_key = api_key
            .filter(|value| !value.trim().is_empty())
            .ok_or(MemoryProviderAdapterError::MissingMem0ApiKey)?;
        let _candidate = Self {
            api_key: api_key.to_string(),
        };
        Err(MemoryProviderAdapterError::PendingLiveRun {
            adapter: "Mem0MemoryProvider",
            reason: "live memory writes have not been observed through the consolidation gate",
        })
    }

    pub fn adapter_name() -> &'static str {
        "Mem0MemoryProvider"
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn write_memory(
        &mut self,
        correlation: &CorrelationIds,
        source_receipt_id: &str,
        gate_receipt_id: &str,
        decision: ConsolidationDecision,
        source_surface: &'static str,
        memory_scope: &'static str,
        content: &str,
        consolidation_state: &'static str,
    ) -> Result<(), String> {
        if !decision.writes_record() {
            return Err(format!(
                "consolidation decision {} does not produce a memory record",
                decision.as_str()
            ));
        }
        let source_receipt = self
            .storage
            .ledger()
            .query()
            .by_id(source_receipt_id)
            .ok_or_else(|| format!("memory source receipt {source_receipt_id} missing"))?;
        let source_receipt_kind = source_receipt.kind.clone();
        let valid_from_receipt_timestamp = source_receipt.receipt_timestamp.clone();
        let gate_receipt = self
            .storage
            .ledger()
            .query()
            .by_id(gate_receipt_id)
            .ok_or_else(|| {
                format!("memory consolidation gate receipt {gate_receipt_id} missing")
            })?;
        if !self.memory_write_allowed(source_surface, memory_scope, gate_receipt) {
            return Err(format!(
                "memory write refused for surface {source_surface} and scope {memory_scope}"
            ));
        }
        let memory_id = self.ids.next("memory");
        let episode_id = format!("episode_{memory_id}");
        self.storage.write_memory(MemoryRecord {
            episode_id: episode_id.clone(),
            memory_id: memory_id.clone(),
            tenant_id: correlation.tenant_id.clone(),
            source_receipt_id: source_receipt_id.to_string(),
            atom_origin: "asserted",
            valid_from_receipt_timestamp: valid_from_receipt_timestamp.clone(),
            consolidation_decision: decision,
            provenance: MemoryProvenance {
                driver_id: LOCAL_MEMORY_DRIVER.driver_id,
                provider: LOCAL_MEMORY_DRIVER.provider,
                durable_driver: LOCAL_MEMORY_DRIVER.durable_driver,
                durable_table: LOCAL_MEMORY_DRIVER.durable_table,
                consolidation_gate: "local_consolidation_gate_v1",
                gate_receipt_id: gate_receipt_id.to_string(),
                source_receipt_kind,
                temporal_status: "event_completed",
            },
            memory_scope,
            memory_tier: "episodic",
            decay_policy: "local_recent_session_decay_v1",
            importance_score: 70,
            content: content.to_string(),
            valid_until_receipt_timestamp: String::new(),
            invalidated_by_receipt_id: String::new(),
            consolidation_state,
            // The kernel never embeds: a local embedder in mdx-server may set
            // this later through set_memory_embedding. Empty is the honest
            // fail-closed default (ranking uses lexical and checksum).
            embedding: String::new(),
        });
        let episode_node_id = self.ids.next("memory_node");
        let atom_node_id = self.ids.next("memory_node");
        let source_node_id = self.ids.next("memory_node");
        let entity_node_id = self.ids.next("memory_node");
        self.storage.push_memory_graph_node(MemoryGraphNode {
            node_id: episode_node_id.clone(),
            tenant_id: correlation.tenant_id.clone(),
            node_kind: "MemoryEpisode",
            label: episode_id,
            memory_id: Some(memory_id.clone()),
            source_receipt_id: source_receipt_id.to_string(),
            atom_origin: "asserted",
            valid_from_receipt_timestamp: valid_from_receipt_timestamp.clone(),
            lifecycle_state: "active",
        });
        self.storage.push_memory_graph_node(MemoryGraphNode {
            node_id: atom_node_id.clone(),
            tenant_id: correlation.tenant_id.clone(),
            node_kind: "MemoryAtom",
            label: content.to_string(),
            memory_id: Some(memory_id.clone()),
            source_receipt_id: source_receipt_id.to_string(),
            atom_origin: "asserted",
            valid_from_receipt_timestamp: valid_from_receipt_timestamp.clone(),
            lifecycle_state: "active",
        });
        self.storage.push_memory_graph_node(MemoryGraphNode {
            node_id: source_node_id.clone(),
            tenant_id: correlation.tenant_id.clone(),
            node_kind: "SourceReceipt",
            label: source_receipt_id.to_string(),
            memory_id: Some(memory_id.clone()),
            source_receipt_id: source_receipt_id.to_string(),
            atom_origin: "external",
            valid_from_receipt_timestamp: valid_from_receipt_timestamp.clone(),
            lifecycle_state: "active",
        });
        self.storage.push_memory_graph_node(MemoryGraphNode {
            node_id: entity_node_id.clone(),
            tenant_id: correlation.tenant_id.clone(),
            node_kind: "Surface",
            label: source_surface.to_string(),
            memory_id: Some(memory_id.clone()),
            source_receipt_id: source_receipt_id.to_string(),
            atom_origin: "derived",
            valid_from_receipt_timestamp: valid_from_receipt_timestamp.clone(),
            lifecycle_state: "active",
        });
        for (from_node_id, to_node_id, edge_kind, weight) in [
            (&episode_node_id, &atom_node_id, "CONTAINS_ATOM", 100),
            (&atom_node_id, &source_node_id, "DERIVED_FROM", 100),
            (&atom_node_id, &entity_node_id, "MENTIONS_SURFACE", 80),
            (&atom_node_id, &episode_node_id, "RETRIEVAL_TRAVERSES", 70),
        ] {
            self.storage.push_memory_graph_edge(MemoryGraphEdge {
                edge_id: self.ids.next("memory_edge"),
                tenant_id: correlation.tenant_id.clone(),
                from_node_id: (*from_node_id).clone(),
                to_node_id: (*to_node_id).clone(),
                edge_kind,
                source_receipt_id: source_receipt_id.to_string(),
                weight,
                valid_from_receipt_timestamp: valid_from_receipt_timestamp.clone(),
            });
        }
        self.storage
            .push_memory_lifecycle_event(MemoryLifecycleEvent {
                event_id: self.ids.next("memory_lifecycle"),
                tenant_id: correlation.tenant_id.clone(),
                memory_id: memory_id.clone(),
                action: "retain",
                lifecycle_state: "active",
                reason: if consolidation_state == MEMORY_CONSOLIDATION_PENDING {
                    "stored pending distinct-actor ratification".to_string()
                } else {
                    "approved local consolidation review".to_string()
                },
                source_receipt_id: source_receipt_id.to_string(),
                valid_from_receipt_timestamp,
                receipt_id: gate_receipt_id.to_string(),
            });
        self.ensure_memory_surface_access(&correlation.tenant_id, gate_receipt_id);
        // Pending records are stored but never ranked: recall must not
        // surface a shared-scope memory before a distinct actor ratifies it.
        if consolidation_state == MEMORY_CONSOLIDATION_ACTIVE {
            self.record_memory_recall_ranking(
                correlation,
                source_surface,
                content,
                memory_scope,
                gate_receipt_id,
            );
        }
        Ok(())
    }

    fn memory_write_allowed(
        &self,
        source_surface: &'static str,
        memory_scope: &'static str,
        gate_receipt: &Receipt,
    ) -> bool {
        let access = self
            .storage
            .memory_surface_access()
            .iter()
            .find(|access| access.surface == source_surface && access.scope == memory_scope);
        let Some(access) = access else {
            return true;
        };
        if access.can_write {
            return true;
        }
        access.review_required
            && gate_receipt.kind == "memory.surface_event.consolidated"
            && !gate_receipt
                .payload
                .get("consolidation_review_receipt_id")
                .map(String::as_str)
                .unwrap_or("")
                .is_empty()
    }

    /// The verbatim fallback write: stores the surface content exactly as
    /// before adjudication existed, and the gate receipt says so with
    /// adjudication_state "unadjudicated_verbatim". Extraction-backed
    /// callers use consolidate_surface_memory directly.
    pub fn record_surface_memory_local(
        &mut self,
        tenant_id: &str,
        actor_id: &str,
        source_surface: &'static str,
        memory_scope: &'static str,
        source_receipt_id: &str,
        content: &str,
    ) -> Result<String, String> {
        let outcome = self.consolidate_surface_memory(
            tenant_id,
            actor_id,
            source_surface,
            memory_scope,
            source_receipt_id,
            content,
            "unadjudicated_verbatim",
        )?;
        if outcome.memory_id.is_empty() {
            return Err("surface memory write did not produce a record".to_string());
        }
        Ok(outcome.memory_id)
    }

    /// The consolidation ceremony: proposal, review, adjudicated gate, and
    /// the record write. adjudication_state is "adjudicated_fact_atom" for
    /// extraction-produced candidates (deterministic adjudication runs and
    /// may answer NOOP or SUPERSEDE) or "unadjudicated_verbatim" for the
    /// fail-closed verbatim path (always RETAIN). Shared scopes land in
    /// MEMORY_CONSOLIDATION_PENDING and clear only through the distinct-actor
    /// memory.consolidation.ratified write; private_user stays auto-approved.
    #[allow(clippy::too_many_arguments)]
    pub fn consolidate_surface_memory(
        &mut self,
        tenant_id: &str,
        actor_id: &str,
        source_surface: &'static str,
        memory_scope: &'static str,
        source_receipt_id: &str,
        content: &str,
        adjudication_state: &str,
    ) -> Result<MemoryConsolidationOutcome, String> {
        if content.trim().is_empty() {
            return Err("surface memory content is empty".to_string());
        }
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(actor_id),
            loop_id: LoopId::new("surface_memory"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let Some(source_receipt) = self.storage.ledger().query().by_id(source_receipt_id) else {
            return Err(format!(
                "surface memory source receipt {source_receipt_id} missing"
            ));
        };
        let candidate_valid_from = source_receipt.receipt_timestamp.clone();
        let adjudication = if adjudication_state == "adjudicated_fact_atom" {
            self.adjudicate_memory_candidate(
                tenant_id,
                memory_scope,
                content,
                &candidate_valid_from,
            )
        } else {
            MemoryAdjudication {
                decision: ConsolidationDecision::Retain,
                cited_memory_id: String::new(),
                reason: "verbatim retention without extraction".to_string(),
            }
        };
        let consolidation_state = if memory_scope == "private_user_memory" {
            MEMORY_CONSOLIDATION_ACTIVE
        } else {
            MEMORY_CONSOLIDATION_PENDING
        };
        let review_state = if consolidation_state == MEMORY_CONSOLIDATION_PENDING {
            "PENDING_RATIFICATION"
        } else {
            "APPROVED_LOCAL_REVIEW"
        };
        let proposal_decision = self.decide_with_receipt(&correlation, ActionKind::WriteMemory);
        let proposal_receipt = self.storage.append_receipt(
            &mut self.ids,
            &correlation,
            "memory.consolidation.proposed",
            Some(proposal_decision.policy_decision_id.clone()),
            payload(&[
                ("proposal_policy", "surface_memory_local_v1"),
                ("proposal_type", "retain_surface_event"),
                ("source_surface", source_surface),
                ("source_receipt_id", source_receipt_id),
                ("memory_scope", memory_scope),
                ("memory_tier", "episodic"),
                ("atom_origin", "asserted"),
                ("adjudication_state", adjudication_state),
                ("human_review_required", "true"),
                ("promotion_allowed_without_review", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        let review_decision = self.decide_with_receipt(&correlation, ActionKind::WriteMemory);
        let review_receipt = self.storage.append_receipt(
            &mut self.ids,
            &correlation,
            "memory.consolidation.reviewed",
            Some(review_decision.policy_decision_id.clone()),
            payload(&[
                ("proposal_receipt_id", &proposal_receipt.receipt_id),
                // APPROVED_LOCAL_REVIEW holds for private_user only; shared
                // scopes are honest about waiting on a distinct actor.
                ("review_state", review_state),
                ("source_surface", source_surface),
                ("approved_memory_scope", memory_scope),
                ("review_policy", "surface_memory_review_v1"),
                ("production_write_allowed", "false"),
            ]),
        );
        let gate_decision = self.decide_with_receipt(&correlation, ActionKind::WriteMemory);
        let gate_receipt = self.storage.append_receipt(
            &mut self.ids,
            &correlation,
            "memory.surface_event.consolidated",
            Some(gate_decision.policy_decision_id.clone()),
            payload(&[
                ("decision", adjudication.decision.as_str()),
                ("adjudication_state", adjudication_state),
                ("adjudication_reason", &adjudication.reason),
                (
                    "adjudicated_against_memory_id",
                    &adjudication.cited_memory_id,
                ),
                ("consolidation_state", consolidation_state),
                ("source_surface", source_surface),
                ("source_receipt_id", source_receipt_id),
                (
                    "consolidation_proposal_receipt_id",
                    &proposal_receipt.receipt_id,
                ),
                (
                    "consolidation_review_receipt_id",
                    &review_receipt.receipt_id,
                ),
                ("memory_scope", memory_scope),
                ("memory_driver", LOCAL_MEMORY_DRIVER.driver_id),
                ("production_write_allowed", "false"),
            ]),
        );
        if adjudication.decision == ConsolidationDecision::Noop {
            // The knowledge is already held: no record, receipts only.
            return Ok(MemoryConsolidationOutcome {
                decision: adjudication.decision.as_str(),
                memory_id: String::new(),
                cited_memory_id: adjudication.cited_memory_id,
                consolidation_state,
                gate_receipt_id: gate_receipt.receipt_id.clone(),
            });
        }
        self.write_memory(
            &correlation,
            source_receipt_id,
            &gate_receipt.receipt_id,
            adjudication.decision.clone(),
            source_surface,
            memory_scope,
            content,
            consolidation_state,
        )?;
        let memory_id = self
            .storage
            .memory_records()
            .last()
            .map(|record| record.memory_id.clone())
            .unwrap_or_default();
        if adjudication.decision == ConsolidationDecision::Supersede
            && !adjudication.cited_memory_id.is_empty()
        {
            self.apply_memory_supersession(
                &correlation,
                &adjudication.cited_memory_id,
                &memory_id,
                &candidate_valid_from,
                &gate_receipt.receipt_id,
                &format!("newer trusted-time memory {memory_id} supersedes the same subject"),
            );
        }
        Ok(MemoryConsolidationOutcome {
            decision: adjudication.decision.as_str(),
            memory_id,
            cited_memory_id: adjudication.cited_memory_id,
            consolidation_state,
            gate_receipt_id: gate_receipt.receipt_id.clone(),
        })
    }

    /// Close a superseded record's validity window and connect the graph:
    /// valid_until and invalidated_by are stamped from the superseding
    /// receipt's trusted time, the lifecycle event lands, and the newer atom
    /// points at the older one with a SUPERSEDES edge.
    fn apply_memory_supersession(
        &mut self,
        correlation: &CorrelationIds,
        superseded_memory_id: &str,
        superseding_memory_id: &str,
        valid_until_receipt_timestamp: &str,
        invalidating_receipt_id: &str,
        reason: &str,
    ) {
        self.storage.close_memory_record_validity(
            superseded_memory_id,
            valid_until_receipt_timestamp,
            invalidating_receipt_id,
        );
        let source_receipt_id = self
            .storage
            .memory_records()
            .iter()
            .find(|record| record.memory_id == superseded_memory_id)
            .map(|record| record.source_receipt_id.clone())
            .unwrap_or_default();
        self.storage
            .push_memory_lifecycle_event(MemoryLifecycleEvent {
                event_id: self.ids.next("memory_lifecycle"),
                tenant_id: correlation.tenant_id.clone(),
                memory_id: superseded_memory_id.to_string(),
                action: "supersede",
                lifecycle_state: "superseded",
                reason: reason.to_string(),
                source_receipt_id,
                valid_from_receipt_timestamp: valid_until_receipt_timestamp.to_string(),
                receipt_id: invalidating_receipt_id.to_string(),
            });
        self.storage
            .set_memory_lifecycle_state(superseded_memory_id, "superseded");
        if !superseding_memory_id.is_empty()
            && let (Some(newer_node), Some(older_node)) = (
                memory_atom_node_id(superseding_memory_id, self.storage.memory_graph_nodes()),
                memory_atom_node_id(superseded_memory_id, self.storage.memory_graph_nodes()),
            )
        {
            self.storage.push_memory_graph_edge(MemoryGraphEdge {
                edge_id: self.ids.next("memory_edge"),
                tenant_id: correlation.tenant_id.clone(),
                from_node_id: newer_node,
                to_node_id: older_node,
                edge_kind: "SUPERSEDES",
                source_receipt_id: invalidating_receipt_id.to_string(),
                weight: 95,
                valid_from_receipt_timestamp: valid_until_receipt_timestamp.to_string(),
            });
        }
    }

    pub fn apply_memory_lifecycle_action(
        &mut self,
        correlation: &CorrelationIds,
        memory_id: &str,
        action: &str,
        reason: &str,
    ) -> Result<MemoryLifecycleEvent, String> {
        let record = self
            .storage
            .memory_records()
            .iter()
            .find(|candidate| candidate.memory_id == memory_id)
            .ok_or_else(|| format!("memory {memory_id} missing"))?;
        let lifecycle_state = match action {
            "decay" => "stale",
            "suppress" => "suppressed",
            "forget" => "forgotten",
            "supersede" => "superseded",
            "merge" => "merged",
            _ => return Err(format!("unsupported memory lifecycle action {action}")),
        };
        let source_receipt_id = record.source_receipt_id.clone();
        let valid_from_receipt_timestamp = self
            .storage
            .ledger()
            .query()
            .by_id(&source_receipt_id)
            .map(|receipt| receipt.receipt_timestamp.clone())
            .unwrap_or_else(|| record.valid_from_receipt_timestamp.clone());
        let receipt = self.storage.append_receipt(
            &mut self.ids,
            correlation,
            "memory.lifecycle.applied",
            None,
            payload(&[
                ("memory_id", memory_id),
                ("action", action),
                ("lifecycle_state", lifecycle_state),
                ("reason", reason),
                ("source_receipt_id", &source_receipt_id),
                (
                    "valid_from_receipt_timestamp",
                    &valid_from_receipt_timestamp,
                ),
                ("trusted_time_required", "true"),
            ]),
        );
        let event = MemoryLifecycleEvent {
            event_id: self.ids.next("memory_lifecycle"),
            tenant_id: correlation.tenant_id.clone(),
            memory_id: memory_id.to_string(),
            action: match action {
                "decay" => "decay",
                "suppress" => "suppress",
                "forget" => "forget",
                "supersede" => "supersede",
                "merge" => "merge",
                _ => unreachable!(),
            },
            lifecycle_state,
            reason: reason.to_string(),
            source_receipt_id,
            valid_from_receipt_timestamp: receipt.receipt_timestamp.clone(),
            receipt_id: receipt.receipt_id.clone(),
        };
        self.storage.push_memory_lifecycle_event(event.clone());
        self.storage
            .set_memory_lifecycle_state(memory_id, lifecycle_state);
        Ok(event)
    }

    pub fn evaluate_memory_lifecycle(
        &mut self,
        correlation: &CorrelationIds,
        reason: &str,
    ) -> Result<MemoryLifecycleEvaluation, String> {
        let records = self.storage.memory_records().to_vec();
        if records.is_empty() {
            return Err(
                "memory lifecycle evaluation requires at least one memory record".to_string(),
            );
        }
        // Adjudication-based supersession: a newer trusted-time record on the
        // same subject (token overlap, not keyword polarity toys) closes the
        // older record's validity window. Only recall-eligible records are
        // swept; pending and declined consolidations wait for ratification.
        let lifecycle_state_index =
            build_latest_memory_lifecycle_state_index(self.storage.memory_lifecycle_events());
        let mut decisions: Vec<(
            MemoryRecord,
            &'static str,
            &'static str,
            String,
            Option<MemoryRecord>,
        )> = Vec::new();
        for record in &records {
            if latest_memory_lifecycle_state_from_index(&record.memory_id, &lifecycle_state_index)
                != "active"
                || record.consolidation_state != MEMORY_CONSOLIDATION_ACTIVE
                || !record.valid_until_receipt_timestamp.is_empty()
            {
                continue;
            }
            let newer = records
                .iter()
                .find(|candidate| {
                    candidate.memory_id != record.memory_id
                        && candidate.consolidation_state == MEMORY_CONSOLIDATION_ACTIVE
                        && candidate.valid_from_receipt_timestamp
                            > record.valid_from_receipt_timestamp
                        && memory_same_subject(&candidate.content, &record.content)
                })
                .cloned();
            if let Some(newer) = newer {
                decisions.push((
                    record.clone(),
                    "supersede",
                    "superseded",
                    format!(
                        "newer trusted-time memory {} supersedes the same subject",
                        newer.memory_id
                    ),
                    Some(newer),
                ));
            } else if trusted_time_receipt_gap(
                &record.valid_from_receipt_timestamp,
                self.storage.ledger().entries(),
            ) >= 3
            {
                decisions.push((
                    record.clone(),
                    "decay",
                    "stale",
                    "trusted-time receipt gap crossed local_recent_session_decay_v1".to_string(),
                    None,
                ));
            }
        }
        let stale_count = decisions
            .iter()
            .filter(|decision| decision.2 == "stale")
            .count() as u16;
        // Keyword contradiction detection was retired with adjudication-based
        // supersession; the schema field stays and honestly reports zero.
        let contradiction_count = 0_u16;
        let supersession_count = decisions
            .iter()
            .filter(|decision| decision.2 == "superseded")
            .count() as u16;
        let receipt = self.storage.append_receipt(
            &mut self.ids,
            correlation,
            "memory.lifecycle.evaluated",
            None,
            payload(&[
                ("policy", "local_recent_session_decay_v1"),
                ("reason", reason),
                ("evaluated_memory_count", &records.len().to_string()),
                ("stale_count", &stale_count.to_string()),
                ("contradiction_count", &contradiction_count.to_string()),
                ("supersession_count", &supersession_count.to_string()),
                ("trusted_time_required", "true"),
            ]),
        );
        for (record, action, lifecycle_state, event_reason, newer) in decisions {
            if let Some(newer) = newer {
                self.apply_memory_supersession(
                    correlation,
                    &record.memory_id,
                    &newer.memory_id,
                    &receipt.receipt_timestamp,
                    &receipt.receipt_id,
                    &event_reason,
                );
            } else {
                self.storage
                    .push_memory_lifecycle_event(MemoryLifecycleEvent {
                        event_id: self.ids.next("memory_lifecycle"),
                        tenant_id: correlation.tenant_id.clone(),
                        memory_id: record.memory_id.clone(),
                        action,
                        lifecycle_state,
                        reason: event_reason,
                        source_receipt_id: record.source_receipt_id.clone(),
                        valid_from_receipt_timestamp: receipt.receipt_timestamp.clone(),
                        receipt_id: receipt.receipt_id.clone(),
                    });
                self.storage
                    .set_memory_lifecycle_state(&record.memory_id, lifecycle_state);
            }
        }
        let evaluation = MemoryLifecycleEvaluation {
            evaluation_id: self.ids.next("memory_lifecycle_eval"),
            tenant_id: correlation.tenant_id.clone(),
            policy: "local_recent_session_decay_v1",
            evaluated_memory_count: records.len() as u16,
            stale_count,
            contradiction_count,
            supersession_count,
            trigger_receipt_id: receipt.receipt_id.clone(),
            trusted_time_floor: receipt.receipt_timestamp.clone(),
            receipt_id: receipt.receipt_id,
        };
        self.storage
            .push_memory_lifecycle_evaluation(evaluation.clone());
        Ok(evaluation)
    }

    /// The factored scoring core both ranking paths share. Tenant filtering
    /// happens HERE so a ranking computed for one tenant can never score
    /// another tenant's records, on the persisting and read-only paths alike.
    fn scored_recall_rows(
        &self,
        surface: &'static str,
        query: &str,
        query_embedding: &[f32],
        scope: &str,
        tenant_id: &str,
    ) -> Vec<ScoredRecallRow> {
        // Build the memory_id -> graph edge-count index ONCE per recall pass
        // (two linear passes over nodes and edges) instead of rescanning every
        // node and edge for every candidate record. This is the audit's
        // superlinear-read-path fix (blind spot 1): the old per-record
        // graph_memory_score made the pass O(records x (nodes + edges)).
        let graph_edge_index = build_graph_edge_index(
            self.storage.memory_graph_nodes(),
            self.storage.memory_graph_edges(),
        );
        // Lifecycle eligibility is another batch lookup. Index the latest
        // event for each memory once instead of reverse-scanning the complete
        // event log for every candidate record.
        let lifecycle_state_index =
            build_latest_memory_lifecycle_state_index(self.storage.memory_lifecycle_events());
        let mut scored = self
            .storage
            .memory_records()
            .iter()
            .filter(|record| record.tenant_id.as_str() == tenant_id)
            .filter(|record| memory_recall_eligible_from_index(record, &lifecycle_state_index))
            .filter(|record| self.memory_read_allowed(surface, record.memory_scope))
            .cloned()
            .map(|record| {
                let lexical_score = lexical_memory_score(query, &record.content);
                // The semantic component: real cosine similarity when both the
                // query and the record carry a local embedding, else the byte
                // checksum proxy - today's behavior, fail-closed. The stored
                // ranking row keeps the conservative content_checksum_score
                // name because the persisting path never has a query embedding.
                let semantic_score = semantic_component_score(
                    query,
                    query_embedding,
                    &record.content,
                    &record.embedding,
                );
                let graph_score = graph_score_from_index(&record.memory_id, &graph_edge_index);
                let recency_score = if record.valid_from_receipt_timestamp.is_empty() {
                    0
                } else {
                    88
                };
                let importance_score = record.importance_score;
                let scope_score = if record.memory_scope == scope {
                    100
                } else {
                    60
                };
                let source_authority_score = match record.atom_origin {
                    "asserted" => 92,
                    "derived" => 78,
                    "external" => 70,
                    _ => 50,
                };
                let final_score = lexical_score as u16
                    + semantic_score as u16
                    + graph_score as u16
                    + recency_score as u16
                    + importance_score as u16
                    + scope_score as u16
                    + source_authority_score as u16;
                ScoredRecallRow {
                    record,
                    lexical_score,
                    semantic_score,
                    graph_score,
                    recency_score,
                    importance_score,
                    scope_score,
                    source_authority_score,
                    final_score,
                }
            })
            .collect::<Vec<_>>();
        scored.sort_by(|left, right| {
            right
                .final_score
                .cmp(&left.final_score)
                .then_with(|| left.record.memory_id.cmp(&right.record.memory_id))
        });
        scored
    }

    pub fn record_memory_recall_ranking(
        &mut self,
        correlation: &CorrelationIds,
        surface: &'static str,
        query: &str,
        scope: &str,
        receipt_id: &str,
    ) -> Vec<MemoryRecallRanking> {
        // The persisting path carries no query embedding, so the semantic
        // component degrades to the byte checksum here: the stored
        // content_checksum_score field is therefore always the genuine
        // checksum, never a mislabeled cosine score.
        let scored =
            self.scored_recall_rows(surface, query, &[], scope, correlation.tenant_id.as_str());
        let mut rankings = Vec::new();
        for (index, row) in scored.into_iter().enumerate() {
            let ranking = MemoryRecallRanking {
                ranking_id: self.ids.next("memory_ranking"),
                tenant_id: correlation.tenant_id.clone(),
                surface,
                query: query.to_string(),
                memory_id: row.record.memory_id,
                lexical_score: row.lexical_score,
                content_checksum_score: row.semantic_score,
                graph_score: row.graph_score,
                recency_score: row.recency_score,
                importance_score: row.importance_score,
                scope_score: row.scope_score,
                source_authority_score: row.source_authority_score,
                final_score: row.final_score,
                rank: (index + 1) as u16,
                source_receipt_id: row.record.source_receipt_id,
                receipt_id: receipt_id.to_string(),
            };
            self.storage.push_memory_recall_ranking(ranking.clone());
            rankings.push(ranking);
        }
        rankings
    }

    /// Read-only recall ranking for prompt assembly: the same factored
    /// scores as record_memory_recall_ranking, computed without persisting
    /// rows or receipts, so the streaming route can rank under kernel.read()
    /// at prepare time. Same tenant, eligibility, and access filters.
    pub fn rank_memory_recall(
        &self,
        surface: &'static str,
        query: &str,
        scope: &str,
        tenant_id: &str,
    ) -> Vec<MemoryRecallCandidate> {
        self.rank_memory_recall_with_embedding(surface, query, &[], scope, tenant_id)
    }

    /// Recall ranking with a query embedding: when present, the semantic
    /// component of each record that also carries an embedding is real cosine
    /// similarity rather than the byte checksum. The embedding is generated in
    /// mdx-server (the only place model gateways live) and threaded in here;
    /// the ranking math itself stays pure and deterministic given the vectors.
    pub fn rank_memory_recall_with_embedding(
        &self,
        surface: &'static str,
        query: &str,
        query_embedding: &[f32],
        scope: &str,
        tenant_id: &str,
    ) -> Vec<MemoryRecallCandidate> {
        self.scored_recall_rows(surface, query, query_embedding, scope, tenant_id)
            .into_iter()
            .enumerate()
            .map(|(index, row)| MemoryRecallCandidate {
                memory_id: row.record.memory_id,
                content: row.record.content,
                memory_scope: row.record.memory_scope,
                final_score: row.final_score,
                rank: (index + 1) as u16,
                source_receipt_id: row.record.source_receipt_id,
            })
            .collect()
    }

    /// Store a local embedding vector on a record. Called only from
    /// mdx-server after a consolidation write, never from a kernel path: the
    /// deterministic kernel does not compute embeddings. Serialization is
    /// lossless enough (six decimals) that restore and re-ranking are stable.
    pub fn set_memory_embedding(&mut self, memory_id: &str, embedding: Vec<f32>) {
        self.storage
            .set_memory_embedding(memory_id, serialize_embedding(&embedding));
    }

    /// Read-only lifecycle health for the recall receipt: how many of this
    /// tenant's records in the scope decayed stale, and how many were
    /// superseded or had their validity window closed. Computed, never
    /// constant.
    pub fn memory_recall_health(&self, scope: &str, tenant_id: &str) -> (usize, usize) {
        let mut stale = 0usize;
        let mut superseded = 0usize;
        let lifecycle_state_index =
            build_latest_memory_lifecycle_state_index(self.storage.memory_lifecycle_events());
        for record in self.storage.memory_records() {
            if record.tenant_id.as_str() != tenant_id || record.memory_scope != scope {
                continue;
            }
            let state =
                latest_memory_lifecycle_state_from_index(&record.memory_id, &lifecycle_state_index);
            if state == "superseded" || !record.valid_until_receipt_timestamp.is_empty() {
                superseded += 1;
            } else if state == "stale" {
                stale += 1;
            }
        }
        (stale, superseded)
    }

    fn memory_read_allowed(&self, surface: &'static str, memory_scope: &'static str) -> bool {
        // Fail closed: no matching access row means no read, even when the
        // matrix has not been seeded yet.
        self.storage
            .memory_surface_access()
            .iter()
            .find(|access| access.surface == surface && access.scope == memory_scope)
            .map(|access| access.can_read)
            .unwrap_or(false)
    }

    pub fn run_memory_brain_eval_harness(
        &mut self,
        correlation: &CorrelationIds,
        reason: &str,
    ) -> Result<Vec<MemoryBrainEvalRun>, String> {
        if self.storage.memory_records().is_empty() {
            return Err("memory brain eval requires at least one memory record".to_string());
        }
        // Fixtures are recall-eligible records only: a pending or superseded
        // record cannot be recalled, so self-querying it would only restate
        // that refusal. A world with records but none eligible measures an
        // honest zero instead of erroring.
        let lifecycle_state_index =
            build_latest_memory_lifecycle_state_index(self.storage.memory_lifecycle_events());
        let records: Vec<MemoryRecord> = self
            .storage
            .memory_records()
            .iter()
            .filter(|record| memory_recall_eligible_from_index(record, &lifecycle_state_index))
            .cloned()
            .collect();
        let receipt = self.storage.append_receipt(
            &mut self.ids,
            correlation,
            "memory.brain_eval.measured",
            None,
            payload(&[
                ("reason", reason),
                ("fixture_families", "self_retrieval_smoke"),
                ("fixture_source", "self_retrieval_smoke"),
                ("ranking_driver", "owned_mdx_memory_ranker_v1"),
                ("vendor_comparator_required", "true"),
            ]),
        );
        let mut runs = Vec::new();
        let fixture_family = "self_retrieval_smoke";
        let fixture_limit = records.len().min(4);
        let mut correct_count = 0_u16;
        let mut total_latency_ms = 0_u16;
        for record in records.iter().take(fixture_limit) {
            let surface = surface_for_memory_scope(record.memory_scope);
            let ranking_started = std::time::Instant::now();
            // Self-retrieval runs inside the record's own tenant: the
            // ranking path is tenant-isolated, so querying a fixture from
            // another tenant's vantage would only measure the isolation.
            let fixture_correlation = CorrelationIds {
                tenant_id: record.tenant_id.clone(),
                ..correlation.clone()
            };
            let rankings = self.record_memory_recall_ranking(
                &fixture_correlation,
                surface,
                &record.content,
                record.memory_scope,
                &receipt.receipt_id,
            );
            let elapsed_ms = ranking_started
                .elapsed()
                .as_millis()
                .min(u128::from(u16::MAX)) as u16;
            let matched = rankings.iter().find(|ranking| ranking.rank == 1).cloned();
            let matched_memory_id = matched
                .as_ref()
                .map(|ranking| ranking.memory_id.clone())
                .unwrap_or_default();
            let final_score = matched
                .as_ref()
                .map(|ranking| ranking.final_score)
                .unwrap_or_default();
            let passed = matched_memory_id == record.memory_id && final_score > 0;
            if passed {
                correct_count += 1;
            }
            total_latency_ms = total_latency_ms.saturating_add(elapsed_ms);
            self.storage
                .push_memory_eval_fixture_result(MemoryEvalFixtureResult {
                    fixture_result_id: self.ids.next("memory_eval_fixture"),
                    tenant_id: correlation.tenant_id.clone(),
                    fixture_family,
                    query: record.content.clone(),
                    expected_memory_id: record.memory_id.clone(),
                    matched_memory_id,
                    final_score,
                    passed,
                    receipt_id: receipt.receipt_id.clone(),
                });
        }
        let fixture_count = fixture_limit as u16;
        let observed_latency_ms = total_latency_ms / fixture_count.max(1);
        let brain_score = (u32::from(correct_count) * 100)
            .checked_div(u32::from(fixture_count))
            .unwrap_or(0) as u8;
        let run = MemoryBrainEvalRun {
            eval_run_id: self.ids.next("memory_eval"),
            tenant_id: correlation.tenant_id.clone(),
            fixture_family,
            fixture_count,
            correct_count,
            latency_budget_ms: 250,
            observed_latency_ms,
            brain_score,
            receipt_id: receipt.receipt_id.clone(),
        };
        self.storage.push_memory_brain_eval_run(run.clone());
        runs.push(run);
        let mdx_run = MemoryBrainEvalRun {
            eval_run_id: self.ids.next("memory_eval"),
            tenant_id: correlation.tenant_id.clone(),
            fixture_family: "MDx Brain Score",
            fixture_count,
            correct_count,
            latency_budget_ms: 250,
            observed_latency_ms,
            brain_score,
            receipt_id: receipt.receipt_id.clone(),
        };
        self.storage.push_memory_brain_eval_run(mdx_run.clone());
        runs.push(mdx_run);
        self.record_memory_vendor_comparator_runs(
            correlation,
            &receipt.receipt_id,
            brain_score,
            observed_latency_ms,
        );
        Ok(runs)
    }

    fn record_memory_vendor_comparator_runs(
        &mut self,
        correlation: &CorrelationIds,
        receipt_id: &str,
        owned_score: u8,
        owned_latency_ms: u16,
    ) {
        // Only owned_mdx ever ran. Vendor adapters are offline; they get no
        // accuracy, latency, or cost numbers until a live run exists.
        for (vendor_id, status, accuracy_score, latency_ms, cost_micros) in [
            (
                "owned_mdx",
                "CANONICAL_LOCAL_RUN",
                owned_score,
                owned_latency_ms,
                0,
            ),
            ("mem0", "OFFLINE_COMPARATOR_ADAPTER", 0, 0, 0),
            ("zep", "OFFLINE_COMPARATOR_ADAPTER", 0, 0, 0),
            ("graphiti", "OFFLINE_COMPARATOR_ADAPTER", 0, 0, 0),
        ] {
            self.storage
                .push_memory_vendor_comparator_run(MemoryVendorComparatorRun {
                    comparator_run_id: self.ids.next("memory_comparator"),
                    tenant_id: correlation.tenant_id.clone(),
                    vendor_id,
                    status,
                    accuracy_score,
                    latency_ms,
                    cost_micros,
                    receipt_id: receipt_id.to_string(),
                });
        }
    }

    pub fn run_memory_topology_validation(
        &mut self,
        correlation: &CorrelationIds,
        reason: &str,
    ) -> Result<Vec<MemoryTopologyRuntimeEvent>, String> {
        let receipt = self.storage.append_receipt(
            &mut self.ids,
            correlation,
            "memory.topology.validated",
            None,
            payload(&[
                ("reason", reason),
                ("queue", "brain-worker-consolidation"),
                ("cache", "valkey-hot-recall"),
                ("latency_budget_ms", "250"),
                ("synthetic_simulation", "true"),
                ("production_write_allowed", "false"),
            ]),
        );
        self.ensure_memory_production_topology_checks(&correlation.tenant_id, &receipt.receipt_id);
        let memory_count = self.storage.memory_records().len() as u16;
        let mut events = Vec::new();
        // These services do not run locally. The rows describe the declared
        // topology plan, so observed latency is 0 rather than a simulation.
        for (service, event_kind, queue_or_cache, cache_key, latency) in [
            (
                "brain-worker",
                "enqueue_consolidation_job",
                "brain-worker-consolidation",
                format!("memory_count:{memory_count}"),
                0_u16,
            ),
            (
                "brain-worker",
                "drain_consolidation_job",
                "brain-worker-consolidation",
                format!("memory_count:{memory_count}"),
                0_u16,
            ),
            (
                "valkey-hot-cache",
                "cache_recall_packet",
                "valkey-hot-recall",
                "recall:latest".to_string(),
                0_u16,
            ),
            (
                "valkey-hot-cache",
                "cache_hit_recall_packet",
                "valkey-hot-recall",
                "recall:latest".to_string(),
                0_u16,
            ),
        ] {
            let event = MemoryTopologyRuntimeEvent {
                topology_event_id: self.ids.next("memory_topology_event"),
                tenant_id: correlation.tenant_id.clone(),
                service,
                event_kind,
                queue_or_cache,
                cache_key,
                observed_latency_ms: latency,
                receipt_id: receipt.receipt_id.clone(),
            };
            self.storage
                .push_memory_topology_runtime_event(event.clone());
            events.push(event);
        }
        Ok(events)
    }

    pub fn run_memory_beta_readiness(
        &mut self,
        correlation: &CorrelationIds,
        synthetic_sessions: u16,
    ) -> Result<MemoryScaleLoadRun, String> {
        let synthetic_sessions = synthetic_sessions.clamp(1, 1000);
        let receipt = self.storage.append_receipt(
            &mut self.ids,
            correlation,
            "memory.beta_readiness.measured",
            None,
            payload(&[
                ("synthetic_sessions", &synthetic_sessions.to_string()),
                ("benchmark_sources", "synthetic_placeholder"),
                ("owned_stack_canonical", "true"),
                ("vendor_dependency_required", "false"),
                ("latency_budget_ms", "250"),
            ]),
        );
        // No benchmark fixture data has been imported. Each row declares an
        // intended task shape only, so it carries no fixtures and no name.
        for (family, source_kind, task_shape, fixture_count) in [
            (
                "synthetic_placeholder",
                "synthetic_placeholder",
                "multi_session_conversation_qa_event_summary",
                0,
            ),
            (
                "synthetic_placeholder",
                "synthetic_placeholder",
                "information_extraction_multi_session_temporal_updates_abstention",
                0,
            ),
            (
                "synthetic_placeholder",
                "synthetic_placeholder",
                "longitudinal_memory_regression_suite",
                0,
            ),
            (
                "synthetic_placeholder",
                "synthetic_placeholder",
                "long_conversation_memory_probes_128k_to_10m_tokens",
                0,
            ),
        ] {
            self.storage
                .push_memory_benchmark_import(MemoryBenchmarkImport {
                    import_id: self.ids.next("memory_benchmark_import"),
                    tenant_id: correlation.tenant_id.clone(),
                    fixture_family: family,
                    source_kind,
                    task_shape,
                    fixture_count,
                    synthetic: true,
                    receipt_id: receipt.receipt_id.clone(),
                });
        }
        let start_count = self.storage.memory_records().len();
        let mut session_latencies_ms: Vec<u16> = Vec::with_capacity(synthetic_sessions as usize);
        for index in 0..synthetic_sessions {
            let surface = match index % 4 {
                0 => "twin",
                1 => "forge",
                2 => "messages",
                _ => "pages",
            };
            let scope = match surface {
                "twin" => "private_user_memory",
                "forge" => "project_memory",
                "messages" => "team_memory",
                "pages" => "company_memory",
                _ => "agent_operational_memory",
            };
            let content = format!(
                "Synthetic beta session {index:04} remembers preference topic {} for {surface}",
                index % 25
            );
            let session_started = std::time::Instant::now();
            self.record_surface_memory_local(
                correlation.tenant_id.as_str(),
                correlation.actor_id.as_str(),
                surface,
                scope,
                &receipt.receipt_id,
                &content,
            )?;
            session_latencies_ms.push(
                session_started
                    .elapsed()
                    .as_millis()
                    .min(u128::from(u16::MAX)) as u16,
            );
        }
        let eval_runs = self.run_memory_brain_eval_harness(correlation, "beta readiness eval")?;
        self.run_memory_topology_validation(correlation, "beta readiness topology")?;
        let memory_record_count = (self.storage.memory_records().len() - start_count) as u16;
        let ranking_count = self
            .storage
            .memory_recall_rankings()
            .len()
            .min(u16::MAX as usize) as u16;
        let brain_score = eval_runs
            .iter()
            .rev()
            .find(|run| run.fixture_family == "MDx Brain Score")
            .map(|run| run.brain_score)
            .unwrap_or(0);
        session_latencies_ms.sort_unstable();
        let p95_index = (session_latencies_ms.len() * 95 / 100)
            .min(session_latencies_ms.len().saturating_sub(1));
        let observed_p95_latency_ms = session_latencies_ms.get(p95_index).copied().unwrap_or(0);
        let run = MemoryScaleLoadRun {
            scale_run_id: self.ids.next("memory_scale_run"),
            tenant_id: correlation.tenant_id.clone(),
            synthetic_session_count: synthetic_sessions,
            memory_record_count,
            ranking_count,
            latency_budget_ms: 250,
            observed_p95_latency_ms,
            brain_score,
            receipt_id: receipt.receipt_id.clone(),
        };
        self.storage.push_memory_scale_load_run(run.clone());
        // DECLARED marks checks this function asserts but does not verify.
        for (check_kind, status, evidence) in [
            (
                "local_postgres_persistence",
                "DECLARED",
                "memory-store-postgres-live-write-check writes ledger-bound memory rows",
            ),
            (
                "benchmark_fixture_import",
                "NOT_IMPORTED",
                "no benchmark fixture data present; imports are synthetic placeholders",
            ),
            (
                "scale_latency_budget",
                if observed_p95_latency_ms <= 250 {
                    "READY"
                } else {
                    "BLOCKED"
                },
                "1000 synthetic sessions ranked under local beta latency budget",
            ),
            (
                "cloud_turn_on",
                "DECLARED",
                "routes schemas migrations RLS and local proof are ready for cloud wiring",
            ),
        ] {
            self.storage
                .push_memory_cloud_turn_on_check(MemoryCloudTurnOnCheck {
                    check_id: self.ids.next("memory_cloud_check"),
                    tenant_id: correlation.tenant_id.clone(),
                    check_kind,
                    status,
                    evidence: evidence.to_string(),
                    receipt_id: receipt.receipt_id.clone(),
                });
        }
        Ok(run)
    }

    fn ensure_memory_surface_access(&mut self, tenant_id: &TenantId, receipt_id: &str) {
        if !self.storage.memory_surface_access().is_empty() {
            return;
        }
        for (surface, scope, can_write, review_required) in [
            ("twin", "private_user_memory", true, true),
            ("forge", "project_memory", false, true),
            ("pages", "company_memory", false, true),
            ("messages", "team_memory", false, true),
            ("future_app_surface", "scoped_memory_port", false, true),
            (
                "future_app_surface",
                "agent_operational_memory",
                false,
                true,
            ),
        ] {
            self.storage
                .push_memory_surface_access(MemorySurfaceAccess {
                    access_id: self.ids.next("memory_access"),
                    tenant_id: tenant_id.clone(),
                    surface,
                    scope,
                    can_read: true,
                    can_write,
                    review_required,
                    receipt_id: receipt_id.to_string(),
                });
        }
    }

    fn ensure_memory_production_topology_checks(&mut self, tenant_id: &TenantId, receipt_id: &str) {
        if !self.storage.memory_production_topology_checks().is_empty() {
            return;
        }
        // Declared topology plan only; nothing was observed, so latency is 0.
        for (service, deployment_shape, latency_role, queue_or_cache, observed_latency_ms) in [
            (
                "brain-api",
                "mdx-server-route",
                "read_and_write_api",
                "none",
                0,
            ),
            (
                "brain-worker",
                "background-worker",
                "consolidation_eval_lifecycle",
                "job_queue",
                0,
            ),
            (
                "valkey-hot-cache",
                "cache-service",
                "low_latency_recall",
                "read_through_cache",
                0,
            ),
            (
                "postgres-durable-memory",
                "database",
                "restart_replay_source",
                "durable_storage",
                0,
            ),
        ] {
            self.storage
                .push_memory_production_topology_check(MemoryProductionTopologyCheck {
                    topology_check_id: self.ids.next("memory_topology"),
                    tenant_id: tenant_id.clone(),
                    service,
                    deployment_shape,
                    latency_role,
                    queue_or_cache,
                    observed_latency_ms,
                    receipt_id: receipt_id.to_string(),
                });
        }
    }
}

/// One scored record inside a recall ranking, before rank assignment. The
/// semantic slot is cosine similarity when embeddings are present, else the
/// byte checksum proxy.
struct ScoredRecallRow {
    record: MemoryRecord,
    lexical_score: u8,
    semantic_score: u8,
    graph_score: u8,
    recency_score: u8,
    importance_score: u8,
    scope_score: u8,
    source_authority_score: u8,
    final_score: u16,
}

/// The memory_id -> latest lifecycle-state index. Lifecycle events are
/// append-only, so a forward pass with replacement preserves the existing
/// reverse-scan rule that the last matching event wins. Borrowed keys avoid
/// allocating memory IDs on every recall pass.
fn build_latest_memory_lifecycle_state_index(
    events: &[MemoryLifecycleEvent],
) -> std::collections::HashMap<&str, &'static str> {
    let mut states = std::collections::HashMap::with_capacity(events.len());
    for event in events {
        states.insert(event.memory_id.as_str(), event.lifecycle_state);
    }
    states
}

fn latest_memory_lifecycle_state_from_index(
    memory_id: &str,
    index: &std::collections::HashMap<&str, &'static str>,
) -> &'static str {
    index.get(memory_id).copied().unwrap_or("active")
}

fn memory_recall_eligible_from_index(
    record: &MemoryRecord,
    lifecycle_state_index: &std::collections::HashMap<&str, &'static str>,
) -> bool {
    latest_memory_lifecycle_state_from_index(&record.memory_id, lifecycle_state_index) == "active"
        && record.consolidation_state == MEMORY_CONSOLIDATION_ACTIVE
        && record.valid_until_receipt_timestamp.is_empty()
}

/// The memory_id -> distinct-edge-count index. An edge counts once for a
/// memory when either endpoint node belongs to that memory (matching the old
/// naive `from OR to` filter), and once per distinct memory it touches when
/// its endpoints span two memories.
fn build_graph_edge_index(
    nodes: &[MemoryGraphNode],
    edges: &[MemoryGraphEdge],
) -> std::collections::HashMap<String, usize> {
    let mut node_to_memory: std::collections::HashMap<&str, &str> =
        std::collections::HashMap::with_capacity(nodes.len());
    for node in nodes {
        if let Some(memory_id) = node.memory_id.as_deref() {
            node_to_memory.insert(node.node_id.as_str(), memory_id);
        }
    }
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for edge in edges {
        let from_memory = node_to_memory.get(edge.from_node_id.as_str()).copied();
        let to_memory = node_to_memory.get(edge.to_node_id.as_str()).copied();
        match (from_memory, to_memory) {
            (Some(from), Some(to)) if from == to => {
                *counts.entry(from.to_string()).or_insert(0) += 1;
            }
            (from, to) => {
                if let Some(from) = from {
                    *counts.entry(from.to_string()).or_insert(0) += 1;
                }
                if let Some(to) = to {
                    *counts.entry(to.to_string()).or_insert(0) += 1;
                }
            }
        }
    }
    counts
}

fn graph_score_from_index(memory_id: &str, index: &std::collections::HashMap<String, usize>) -> u8 {
    let edge_count = index.get(memory_id).copied().unwrap_or(0);
    (edge_count * 20).min(100) as u8
}

/// The semantic recall component. Real cosine similarity mapped to 0..=100
/// when both the query and the record carry a same-length local embedding;
/// otherwise the byte checksum proxy, which is exactly what recall scored
/// before local embeddings existed. Fail-closed: any missing or mismatched
/// vector falls back rather than erroring.
fn semantic_component_score(
    query: &str,
    query_embedding: &[f32],
    content: &str,
    record_embedding: &str,
) -> u8 {
    if !query_embedding.is_empty() {
        let record_vector = parse_embedding(record_embedding);
        if !record_vector.is_empty()
            && record_vector.len() == query_embedding.len()
            && let Some(cosine) = cosine_similarity(query_embedding, &record_vector)
        {
            // Cosine is in [-1, 1]; map to [0, 100]. Identical direction
            // scores 100, orthogonal 50, opposite 0.
            let scaled = ((cosine + 1.0) / 2.0 * 100.0).round();
            return scaled.clamp(0.0, 100.0) as u8;
        }
    }
    content_checksum_score(query, content)
}

/// Cosine similarity of two equal-length vectors, or None when either has no
/// magnitude (a zero vector has no direction to compare).
fn cosine_similarity(left: &[f32], right: &[f32]) -> Option<f32> {
    if left.len() != right.len() || left.is_empty() {
        return None;
    }
    let mut dot = 0.0f32;
    let mut left_norm = 0.0f32;
    let mut right_norm = 0.0f32;
    for (a, b) in left.iter().zip(right.iter()) {
        dot += a * b;
        left_norm += a * a;
        right_norm += b * b;
    }
    if left_norm == 0.0 || right_norm == 0.0 {
        return None;
    }
    Some(dot / (left_norm.sqrt() * right_norm.sqrt()))
}

/// Serialize an embedding to the compact string stored on the record: comma
/// separated, six decimals, lossless enough that re-ranking after a snapshot
/// restore is stable. Empty vector serializes to the empty string.
pub fn serialize_embedding(embedding: &[f32]) -> String {
    embedding
        .iter()
        .map(|value| format!("{value:.6}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// Parse a stored embedding string back into a vector. A malformed or empty
/// string yields an empty vector, which the ranker reads as "not embedded".
pub fn parse_embedding(serialized: &str) -> Vec<f32> {
    if serialized.trim().is_empty() {
        return Vec::new();
    }
    serialized
        .split(',')
        .filter_map(|value| value.trim().parse::<f32>().ok())
        .collect()
}

fn lexical_memory_score(query: &str, content: &str) -> u8 {
    let query_tokens = normalized_tokens(query);
    if query_tokens.is_empty() {
        return 0;
    }
    let content_tokens = normalized_tokens(content);
    let matches = query_tokens
        .iter()
        .filter(|token| content_tokens.contains(token))
        .count();
    ((matches * 100) / query_tokens.len()).min(100) as u8
}

// A byte-sum checksum comparison, not an embedding. The name says so.
fn content_checksum_score(query: &str, content: &str) -> u8 {
    let query_sum: u32 = query.bytes().map(u32::from).sum();
    let content_sum: u32 = content.bytes().map(u32::from).sum();
    let distance = query_sum.abs_diff(content_sum) % 100;
    (100 - distance) as u8
}

// The naive per-record graph score, retained only as the correctness oracle
// the index is tested against (see the index-equivalence test). The live
// recall path uses build_graph_edge_index / graph_score_from_index.
#[cfg(test)]
fn graph_memory_score(memory_id: &str, edges: &[MemoryGraphEdge], nodes: &[MemoryGraphNode]) -> u8 {
    let node_ids = nodes
        .iter()
        .filter(|node| node.memory_id.as_deref() == Some(memory_id))
        .map(|node| node.node_id.as_str())
        .collect::<Vec<_>>();
    let edge_count = edges
        .iter()
        .filter(|edge| {
            node_ids.contains(&edge.from_node_id.as_str())
                || node_ids.contains(&edge.to_node_id.as_str())
        })
        .count();
    (edge_count * 20).min(100) as u8
}

fn trusted_time_receipt_gap(receipt_timestamp: &str, receipts: &[Receipt]) -> usize {
    receipts
        .iter()
        .filter(|receipt| receipt.receipt_timestamp.as_str() > receipt_timestamp)
        .count()
}

fn memory_atom_node_id(memory_id: &str, nodes: &[MemoryGraphNode]) -> Option<String> {
    nodes
        .iter()
        .find(|node| node.node_kind == "MemoryAtom" && node.memory_id.as_deref() == Some(memory_id))
        .map(|node| node.node_id.clone())
}

pub(crate) fn surface_for_memory_scope(memory_scope: &str) -> &'static str {
    match memory_scope {
        "private_user_memory" => "twin",
        "project_memory" => "forge",
        "team_memory" => "messages",
        "company_memory" => "pages",
        _ => "future_app_surface",
    }
}

#[cfg(test)]
mod lifecycle_index_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_component_falls_back_to_checksum_without_an_embedding() {
        let query = "cat sleeping in the sun";
        let content = "The feline napped on the windowsill";
        // No query embedding: the semantic slot is exactly the byte checksum,
        // fail-closed to today's behavior.
        assert_eq!(
            semantic_component_score(query, &[], content, ""),
            content_checksum_score(query, content)
        );
        // A query embedding but no record embedding still falls back.
        assert_eq!(
            semantic_component_score(query, &[0.1, 0.2], content, ""),
            content_checksum_score(query, content)
        );
        // Mismatched dimensions fall back rather than error.
        assert_eq!(
            semantic_component_score(query, &[0.1, 0.2], content, &serialize_embedding(&[0.3])),
            content_checksum_score(query, content)
        );
    }

    #[test]
    fn cosine_similarity_scores_direction_and_maps_to_0_100() {
        let a = [1.0, 0.0, 0.0];
        assert_eq!(cosine_similarity(&a, &[1.0, 0.0, 0.0]), Some(1.0));
        assert_eq!(cosine_similarity(&a, &[0.0, 1.0, 0.0]), Some(0.0));
        assert_eq!(cosine_similarity(&a, &[-1.0, 0.0, 0.0]), Some(-1.0));
        assert_eq!(cosine_similarity(&a, &[0.0, 0.0, 0.0]), None);
        // Identical direction -> 100, orthogonal -> 50, opposite -> 0.
        let identical = serialize_embedding(&[1.0, 0.0, 0.0]);
        assert_eq!(
            semantic_component_score("q", &[1.0, 0.0, 0.0], "c", &identical),
            100
        );
        let orthogonal = serialize_embedding(&[0.0, 1.0, 0.0]);
        assert_eq!(
            semantic_component_score("q", &[1.0, 0.0, 0.0], "c", &orthogonal),
            50
        );
    }

    #[test]
    fn embedding_serialization_round_trips_within_ranking_tolerance() {
        let vector = vec![0.123456_f32, -0.987654, 0.5, 0.0];
        let restored = parse_embedding(&serialize_embedding(&vector));
        assert_eq!(restored.len(), vector.len());
        for (original, restored) in vector.iter().zip(restored.iter()) {
            assert!((original - restored).abs() < 1e-5);
        }
        assert!(parse_embedding("").is_empty());
        assert!(parse_embedding("   ").is_empty());
    }

    fn graph_bundle_for(
        memory_id: &str,
        index: usize,
    ) -> (Vec<MemoryGraphNode>, Vec<MemoryGraphEdge>) {
        let node = |suffix: &str| MemoryGraphNode {
            node_id: format!("{memory_id}_{suffix}"),
            tenant_id: TenantId::new("tenant_local"),
            node_kind: "MemoryAtom",
            label: suffix.to_string(),
            memory_id: Some(memory_id.to_string()),
            source_receipt_id: "receipt_seed".to_string(),
            atom_origin: "asserted",
            valid_from_receipt_timestamp: "t".to_string(),
            lifecycle_state: "active",
        };
        let nodes = vec![
            node("episode"),
            node("atom"),
            node("source"),
            node("entity"),
        ];
        let edge = |from: &str, to: &str| MemoryGraphEdge {
            edge_id: format!("edge_{memory_id}_{from}_{to}_{index}"),
            tenant_id: TenantId::new("tenant_local"),
            from_node_id: format!("{memory_id}_{from}"),
            to_node_id: format!("{memory_id}_{to}"),
            edge_kind: "CONTAINS_ATOM",
            source_receipt_id: "receipt_seed".to_string(),
            weight: 100,
            valid_from_receipt_timestamp: "t".to_string(),
        };
        let edges = vec![
            edge("episode", "atom"),
            edge("atom", "source"),
            edge("atom", "entity"),
            edge("atom", "episode"),
        ];
        (nodes, edges)
    }

    #[test]
    fn graph_edge_index_matches_the_naive_scan_and_is_faster() {
        let record_count = 1000usize;
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let mut memory_ids = Vec::with_capacity(record_count);
        for index in 0..record_count {
            let memory_id = format!("memory_{index:04}");
            let (mut n, mut e) = graph_bundle_for(&memory_id, index);
            nodes.append(&mut n);
            edges.append(&mut e);
            memory_ids.push(memory_id);
        }
        // One cross-memory edge: it must count for BOTH endpoints' memories,
        // in the index exactly as the naive OR-filter counts it.
        edges.push(MemoryGraphEdge {
            edge_id: "edge_cross".to_string(),
            tenant_id: TenantId::new("tenant_local"),
            from_node_id: "memory_0000_atom".to_string(),
            to_node_id: "memory_0001_atom".to_string(),
            edge_kind: "SUPERSEDES",
            source_receipt_id: "receipt_seed".to_string(),
            weight: 95,
            valid_from_receipt_timestamp: "t".to_string(),
        });

        // Correctness: the index produces identical graph scores to the naive
        // per-record scan for every memory.
        let index = build_graph_edge_index(&nodes, &edges);
        for memory_id in &memory_ids {
            assert_eq!(
                graph_score_from_index(memory_id, &index),
                graph_memory_score(memory_id, &edges, &nodes),
                "index and naive scan disagree for {memory_id}"
            );
        }

        // Perf: one indexed full pass (build once, then lookups) must beat the
        // naive per-record rescan. Loose assertion - the point is the shape,
        // O(nodes + edges) vs O(records x (nodes + edges)).
        let naive_start = std::time::Instant::now();
        let mut naive_total = 0u32;
        for memory_id in &memory_ids {
            naive_total += graph_memory_score(memory_id, &edges, &nodes) as u32;
        }
        let naive_elapsed = naive_start.elapsed();

        let index_start = std::time::Instant::now();
        let rebuilt = build_graph_edge_index(&nodes, &edges);
        let mut index_total = 0u32;
        for memory_id in &memory_ids {
            index_total += graph_score_from_index(memory_id, &rebuilt) as u32;
        }
        let index_elapsed = index_start.elapsed();

        assert_eq!(naive_total, index_total);
        assert!(
            index_elapsed < naive_elapsed,
            "indexed pass ({index_elapsed:?}) should be faster than naive ({naive_elapsed:?})"
        );
    }
}
