use crate::pages::pages_onboarding_documents;
use crate::*;

pub const LOCAL_CTX_DRIVER: &str = "local_ctx_engine";
pub const LOCAL_CTX_PROVIDER: &str = "DeterministicContextAssembler";
const CTX_STATUS: &str = "LIVE-LOCAL-CONTEXT-ASSEMBLY";
const DEFAULT_TOKEN_BUDGET: usize = 240;
const HOT_PATH_BUDGET_MS: usize = 50;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalContextRequest<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub query: &'a str,
    pub token_budget: usize,
    pub source_preferences: &'a [LocalContextSourcePreference<'a>],
    pub query_entities: &'a [&'a str],
    pub local_memories: &'a [LocalContextMemoryInput<'a>],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalContextSourcePreference<'a> {
    pub source_type: &'a str,
    pub enabled: bool,
    pub token_budget_override: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalContextMemoryInput<'a> {
    pub memory_id: &'a str,
    pub tier: &'a str,
    pub content: &'a str,
    pub source_receipt_id: &'a str,
    pub importance: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalContextPacket {
    pub status: &'static str,
    pub ctx_driver: &'static str,
    pub ctx_provider: &'static str,
    pub memory_driver: &'static str,
    pub memory_provider: &'static str,
    pub vector_driver: &'static str,
    pub working_cache_driver: &'static str,
    pub assembly_log_id: String,
    pub query: String,
    pub query_entity_count: usize,
    pub local_memory_input_count: usize,
    pub token_budget: usize,
    pub total_tokens: usize,
    pub budget_remaining: usize,
    pub source_type_count: usize,
    pub source_count: usize,
    pub included_source_count: usize,
    pub truncated_source_count: usize,
    pub disabled_source_count: usize,
    pub hot_path_budget_ms: usize,
    pub estimated_p95_ms: usize,
    pub assembly_receipt_id: String,
    pub embedding_refusal_receipt_id: String,
    pub provider_call_allowed: bool,
    pub embedding_provider_call_allowed: bool,
    pub production_write_allowed: bool,
    pub budget_allocations: Vec<LocalContextBudgetAllocation>,
    pub sources: Vec<LocalContextSource>,
    pub truncated_sources: Vec<LocalContextTruncatedSource>,
    pub source_preferences_applied: Vec<LocalContextSourcePreferenceReport>,
    pub hygiene_report: LocalContextHygieneReport,
    pub memory_tiers: Vec<LocalContextMemoryTierReport>,
    pub consolidation_report: LocalContextConsolidationReport,
    pub handoff_report: LocalContextHandoffReport,
    pub vector_report: LocalContextVectorReport,
    pub working_cache_report: LocalContextWorkingCacheReport,
    pub signal_report: LocalContextSignalReport,
    pub event_stream_report: LocalContextEventStreamReport,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalContextBudgetAllocation {
    pub source_type: &'static str,
    pub priority: usize,
    pub token_budget: usize,
    pub fixed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalContextSource {
    pub source_id: String,
    pub source_type: &'static str,
    pub priority: usize,
    pub title: String,
    pub content: String,
    pub token_estimate: usize,
    pub budget_allocated: usize,
    pub fixed: bool,
    pub score: LocalContextScore,
    pub source_receipt_ids: Vec<String>,
    pub memory_driver: Option<&'static str>,
    pub memory_provider: Option<&'static str>,
    pub memory_tier: Option<&'static str>,
    pub world_model_source: Option<&'static str>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalContextTruncatedSource {
    pub source_id: String,
    pub source_type: &'static str,
    pub token_estimate: usize,
    pub composite_score: usize,
    pub reason: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalContextSourcePreferenceReport {
    pub source_type: String,
    pub enabled: bool,
    pub token_budget_override: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalContextHygieneReport {
    pub tokens_in: usize,
    pub tokens_out: usize,
    pub reduction_percent_x100: usize,
    pub duplicate_sources_removed: usize,
    pub strategies_applied: Vec<&'static str>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalContextMemoryTierReport {
    pub tier: &'static str,
    pub source_count: usize,
    pub included_source_count: usize,
    pub token_count: usize,
    pub retention_policy: &'static str,
    pub recall_policy: &'static str,
    pub write_path: &'static str,
    pub cache_driver: Option<&'static str>,
    pub consolidation_state: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalContextConsolidationReport {
    pub status: &'static str,
    pub candidate_count: usize,
    pub pending_review_count: usize,
    pub auto_promotion_allowed: bool,
    pub human_review_required: bool,
    pub dedup_threshold_percent_x100: usize,
    pub promotion_targets: Vec<&'static str>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalContextHandoffReport {
    pub status: &'static str,
    pub summary_sections: Vec<&'static str>,
    pub strategy_order: Vec<&'static str>,
    pub max_summary_tokens: usize,
    pub preserve_verbatim_supported: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalContextVectorReport {
    pub status: &'static str,
    pub driver: &'static str,
    pub table: &'static str,
    pub dimensions: usize,
    pub tenant_scoped: bool,
    pub live_index_ready: bool,
    pub provider_call_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalContextWorkingCacheReport {
    pub status: &'static str,
    pub driver: &'static str,
    pub namespace: &'static str,
    pub ttl_seconds: usize,
    pub write_through_postgres: bool,
    pub live_cache_ready: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalContextSignalReport {
    pub status: &'static str,
    pub signal_source_count: usize,
    pub search_policy: &'static str,
    pub cross_connection_policy: &'static str,
    pub tenant_scoped: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalContextEventStreamReport {
    pub status: &'static str,
    pub stream: &'static str,
    pub websocket_allowed: bool,
    pub provider_call_allowed: bool,
    pub replay_policy: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalContextScore {
    pub lexical: usize,
    pub recency: usize,
    pub importance: usize,
    pub graph: usize,
    pub composite: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocalContextError {
    Missing(&'static str),
    NoContextSources,
}

impl LocalContextError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("local CTX request missing {field}"),
            Self::NoContextSources => "local CTX assembler found no context sources".to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LocalContextCandidate {
    source_id: String,
    source_type: &'static str,
    priority: usize,
    title: String,
    content: String,
    token_estimate: usize,
    score: LocalContextScore,
    source_receipt_ids: Vec<String>,
    memory_driver: Option<&'static str>,
    memory_provider: Option<&'static str>,
    memory_tier: Option<&'static str>,
    world_model_source: Option<&'static str>,
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn assemble_local_context(
        &mut self,
        request: LocalContextRequest<'_>,
    ) -> Result<LocalContextPacket, LocalContextError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("query", request.query),
        ] {
            if value.trim().is_empty() {
                return Err(LocalContextError::Missing(field));
            }
        }
        let token_budget = if request.token_budget == 0 {
            DEFAULT_TOKEN_BUDGET
        } else {
            request.token_budget
        };
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(request.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(request.actor_id),
            loop_id: LoopId::new("ctx_local_engine"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("run");
        self.storage.push_loop_run(LoopRun {
            run_id: run_id.clone(),
            loop_id: correlation.loop_id.clone(),
            agent_id: correlation.actor_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            status: "RUNNING".to_string(),
        });

        let ranked = self.rank_local_context_sources(&request);
        if ranked.is_empty() {
            return Err(LocalContextError::NoContextSources);
        }
        let allocations =
            allocate_source_budgets(token_budget, &ranked, request.source_preferences);
        let source_count = ranked.len();
        let hygiene_report = hygiene_report(&ranked);
        let memory_tiers = memory_tier_reports(&ranked);
        let consolidation_report = consolidation_report(&ranked);
        let handoff_report = handoff_report();
        let vector_report = vector_report();
        let working_cache_report = working_cache_report();
        let signal_report = signal_report(&ranked);
        let event_stream_report = event_stream_report();
        let (included, truncated, total_tokens) =
            fit_context_sources(ranked, &allocations, token_budget);
        let memory_tiers = merge_included_tier_counts(memory_tiers, &included);
        let source_ids = included
            .iter()
            .map(|source| source.source_id.as_str())
            .collect::<Vec<_>>()
            .join(",");
        let source_types = included
            .iter()
            .map(|source| source.source_type)
            .collect::<Vec<_>>()
            .join(",");
        let memory_tier_names = memory_tiers
            .iter()
            .map(|tier| tier.tier)
            .collect::<Vec<_>>()
            .join(",");
        let consolidation_candidate_count_text = consolidation_report.candidate_count.to_string();
        let local_memory_input_count_text = request.local_memories.len().to_string();
        let assembly_log_id = self.ids.next("ctx_assembly_log");

        let decision = self.decide_with_receipt(&correlation, ActionKind::AssembleLocalContext);
        let token_budget_text = token_budget.to_string();
        let total_tokens_text = total_tokens.to_string();
        let source_count_text = included.len().to_string();
        let truncated_count_text = truncated.len().to_string();
        let budget_remaining_text = token_budget.saturating_sub(total_tokens).to_string();
        let assembly_receipt = self.transition_receipt(
            &run_id,
            "ASSEMBLE_CONTEXT",
            &correlation,
            &decision,
            "ctx.context.assembled",
            payload(&[
                ("ctx_driver", LOCAL_CTX_DRIVER),
                ("ctx_provider", LOCAL_CTX_PROVIDER),
                ("memory_driver", LOCAL_MEMORY_DRIVER.driver_id),
                ("memory_provider", LOCAL_MEMORY_DRIVER.provider),
                ("vector_driver", "pgvector_ready_local_projection"),
                ("working_cache_driver", "valkey_ready_local_projection"),
                ("assembly_log_id", &assembly_log_id),
                ("query", request.query),
                ("local_memory_input_count", &local_memory_input_count_text),
                ("token_budget", &token_budget_text),
                ("total_tokens", &total_tokens_text),
                ("budget_remaining", &budget_remaining_text),
                ("source_count", &source_count_text),
                ("truncated_source_count", &truncated_count_text),
                ("source_ids", &source_ids),
                ("source_types", &source_types),
                ("memory_tiers", &memory_tier_names),
                ("consolidation_status", consolidation_report.status),
                (
                    "consolidation_candidate_count",
                    &consolidation_candidate_count_text,
                ),
                ("handoff_status", handoff_report.status),
                ("signal_status", signal_report.status),
                ("event_stream_status", event_stream_report.status),
                ("provider_call_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        let refusal_decision =
            self.decide_with_receipt(&correlation, ActionKind::RefuseCtxEmbeddingProvider);
        let embedding_refusal = self.transition_receipt(
            &run_id,
            "REFUSE_EMBEDDING_PROVIDER",
            &correlation,
            &refusal_decision,
            "ctx.embedding_provider.refused",
            payload(&[
                ("ctx_driver", LOCAL_CTX_DRIVER),
                ("embedding_provider_turn_on_observed", "false"),
                ("embedding_provider_call_allowed", "false"),
                ("vector_store_adapter_allowed", "false"),
                ("provider_call_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );

        Ok(LocalContextPacket {
            status: CTX_STATUS,
            ctx_driver: LOCAL_CTX_DRIVER,
            ctx_provider: LOCAL_CTX_PROVIDER,
            memory_driver: LOCAL_MEMORY_DRIVER.driver_id,
            memory_provider: LOCAL_MEMORY_DRIVER.provider,
            vector_driver: "pgvector_ready_local_projection",
            working_cache_driver: "valkey_ready_local_projection",
            assembly_log_id,
            query: request.query.to_string(),
            query_entity_count: request.query_entities.len(),
            local_memory_input_count: request.local_memories.len(),
            token_budget,
            total_tokens,
            budget_remaining: token_budget.saturating_sub(total_tokens),
            source_type_count: CTX_SOURCE_TYPES.len(),
            source_count,
            included_source_count: included.len(),
            truncated_source_count: truncated.len(),
            disabled_source_count: disabled_source_count(request.source_preferences),
            hot_path_budget_ms: HOT_PATH_BUDGET_MS,
            estimated_p95_ms: estimated_p95_ms(source_count, included.len()),
            assembly_receipt_id: assembly_receipt.receipt_id,
            embedding_refusal_receipt_id: embedding_refusal.receipt_id,
            provider_call_allowed: false,
            embedding_provider_call_allowed: false,
            production_write_allowed: false,
            budget_allocations: allocations,
            sources: included,
            truncated_sources: truncated,
            source_preferences_applied: request
                .source_preferences
                .iter()
                .map(|preference| LocalContextSourcePreferenceReport {
                    source_type: preference.source_type.to_string(),
                    enabled: preference.enabled,
                    token_budget_override: preference.token_budget_override,
                })
                .collect(),
            hygiene_report,
            memory_tiers,
            consolidation_report,
            handoff_report,
            vector_report,
            working_cache_report,
            signal_report,
            event_stream_report,
        })
    }

    fn rank_local_context_sources(
        &self,
        request: &LocalContextRequest<'_>,
    ) -> Vec<LocalContextCandidate> {
        let mut sources = self.collect_local_context_sources(request);
        sources.retain(|source| source_enabled(source.source_type, request.source_preferences));
        sources = dedupe_sources(sources);
        sources.sort_by(|left, right| {
            fixed_sort_key(left.source_type)
                .cmp(&fixed_sort_key(right.source_type))
                .then_with(|| right.score.composite.cmp(&left.score.composite))
                .then_with(|| left.priority.cmp(&right.priority))
                .then_with(|| left.source_id.cmp(&right.source_id))
        });
        sources
    }

    fn collect_local_context_sources(
        &self,
        request: &LocalContextRequest<'_>,
    ) -> Vec<LocalContextCandidate> {
        let mut sources = vec![
            candidate(
                "system_prompt:local_ctx",
                "system_prompt",
                0,
                "Local CTX operating stance",
                "Assemble only grounded local context. Do not call providers, tools, network, workers, or production stores.",
                request,
                100,
            ),
            candidate(
                "tier1:governed_local_driver",
                "tier1_knowledge",
                1,
                "Governed local driver rule",
                "Local drivers are first-class governed implementations. Vendor drivers may replace them only after matching receipts and proof.",
                request,
                95,
            ),
            candidate(
                "handoff:ctx_to_dxr_boundary",
                "handoff",
                2,
                "CTX to DXR boundary",
                "CTX assembles memory and world-model context. DXR consumes context later, but CTX does not execute workflows or tools.",
                request,
                75,
            ),
            candidate(
                "compiled_knowledge:v1_runtime_floor",
                "compiled_knowledge",
                5,
                "v1 runtime floor",
                "v1 CTX used a standalone Rust engine with source adapters, budgeter, ranker, Postgres, Redis, pgvector, handoff hygiene, and assembly logs.",
                request,
                90,
            ),
            candidate(
                "signal:runtime_gap",
                "signal_intelligence",
                6,
                "Runtime gap signal",
                "Embedding providers, mem0, pgvector writes, Valkey cache, and CTX event streams remain pending live turn-on evidence.",
                request,
                70,
            ),
        ];
        let mut working_memory = candidate(
            "working_memory:local_session",
            "working_memory",
            3,
            "Local working memory",
            "The current local session prefers fresh install readiness, v1 parity, governed writes, proof, CTX, and DXR runtime split work.",
            request,
            80,
        );
        working_memory.memory_driver = Some(LOCAL_MEMORY_DRIVER.driver_id);
        working_memory.memory_provider = Some(LOCAL_MEMORY_DRIVER.provider);
        working_memory.memory_tier = Some("working");
        sources.push(working_memory);
        for (index, memory) in request.local_memories.iter().enumerate() {
            let tier = normalize_memory_tier(memory.tier);
            sources.push(LocalContextCandidate {
                source_id: if memory.memory_id.trim().is_empty() {
                    format!("local_memory_input:{index}")
                } else {
                    memory.memory_id.to_string()
                },
                source_type: memory_source_type(tier),
                priority: memory_priority(tier),
                title: format!("Local {tier} memory input"),
                content: memory.content.to_string(),
                token_estimate: token_estimate(memory.content),
                score: score_context(
                    request,
                    memory.content,
                    memory.importance.min(100),
                    memory_priority(tier),
                ),
                source_receipt_ids: if memory.source_receipt_id.trim().is_empty() {
                    Vec::new()
                } else {
                    vec![memory.source_receipt_id.to_string()]
                },
                memory_driver: Some(LOCAL_MEMORY_DRIVER.driver_id),
                memory_provider: Some(LOCAL_MEMORY_DRIVER.provider),
                memory_tier: Some(tier),
                world_model_source: None,
            });
        }
        let mut semantic_memory = candidate(
            "semantic_memory:v1_floor",
            "compiled_knowledge",
            5,
            "Local semantic memory floor",
            "Semantic memory carries long-lived high-confidence facts, deduplicated before promotion and recalled without recency decay.",
            request,
            88,
        );
        semantic_memory.memory_driver = Some(LOCAL_MEMORY_DRIVER.driver_id);
        semantic_memory.memory_provider = Some(LOCAL_MEMORY_DRIVER.provider);
        semantic_memory.memory_tier = Some("semantic");
        sources.push(semantic_memory);
        let mut procedural_memory = candidate(
            "procedural_memory:v1_floor",
            "compiled_knowledge",
            5,
            "Local procedural memory floor",
            "Procedural memory carries cross-agent learned patterns with effectiveness and agent-affinity scoring.",
            request,
            86,
        );
        procedural_memory.memory_driver = Some(LOCAL_MEMORY_DRIVER.driver_id);
        procedural_memory.memory_provider = Some(LOCAL_MEMORY_DRIVER.provider);
        procedural_memory.memory_tier = Some("procedural");
        sources.push(procedural_memory);
        for record in self.storage.memory_records() {
            sources.push(LocalContextCandidate {
                source_id: record.memory_id.clone(),
                source_type: "episodic_memory",
                priority: 4,
                title: record.provenance.source_receipt_kind.clone(),
                content: record.content.clone(),
                token_estimate: token_estimate(&record.content),
                score: score_context(request, &record.content, 85, 1),
                source_receipt_ids: vec![
                    record.source_receipt_id.clone(),
                    record.provenance.gate_receipt_id.clone(),
                ],
                memory_driver: Some(record.provenance.driver_id),
                memory_provider: Some(record.provenance.provider),
                memory_tier: Some("episodic"),
                world_model_source: None,
            });
        }
        for document in pages_onboarding_documents() {
            let content = format!("{} {}", document.title, document.summary);
            sources.push(LocalContextCandidate {
                source_id: document.document_id.to_string(),
                source_type: "pages_world_model_document",
                priority: 7,
                title: document.title.to_string(),
                content: content.clone(),
                token_estimate: token_estimate(&content),
                score: score_context(request, &content, 65, 2),
                source_receipt_ids: document
                    .source_receipt_ids
                    .iter()
                    .map(|receipt| (*receipt).to_string())
                    .collect(),
                memory_driver: None,
                memory_provider: None,
                memory_tier: None,
                world_model_source: Some("generated/world-model/pages-projection-fixtures.json"),
            });
        }
        for source in trusted_context_sources_from_receipts(self.storage.ledger().entries()) {
            let content = format!(
                "Trusted Context source {} was approved for AI use from {}. Source kind: {}. Company context source: {}. Source hash: {}. Allowed consumers: {}.",
                source.title,
                source.intake_surface,
                source.source_kind,
                source.company_context_source,
                source.source_artifact_hash,
                source.allowed_consumers
            );
            let source_receipt_ids = trusted_context_receipt_ids(&source);
            sources.push(LocalContextCandidate {
                source_id: source.source_id,
                source_type: "trusted_context_source",
                priority: 2,
                title: source.title,
                token_estimate: token_estimate(&content),
                score: score_context(request, &content, 98, 2),
                content,
                source_receipt_ids,
                memory_driver: None,
                memory_provider: None,
                memory_tier: None,
                world_model_source: Some("pages_context_source_rail"),
            });
        }
        sources.push(candidate(
            "rag:pages_world_model_projection",
            "rag",
            8,
            "Pages projection retrieval",
            "RAG is locally represented by deterministic Pages world-model projection fragments until live embedding retrieval is turned on.",
            request,
        60,
    ));
        sources
    }
}

const CTX_SOURCE_TYPES: [&str; 10] = [
    "system_prompt",
    "tier1_knowledge",
    "handoff",
    "working_memory",
    "episodic_memory",
    "compiled_knowledge",
    "signal_intelligence",
    "rag",
    "pages_world_model_document",
    "trusted_context_source",
];

fn trusted_context_receipt_ids(source: &TrustedContextSourceSummary) -> Vec<String> {
    [
        source.source_admission_receipt_id.as_str(),
        source.trust_decision_receipt_id.as_str(),
        source.storage_receipt_id.as_str(),
        source.parse_receipt_id.as_str(),
        source.grounding_receipt_id.as_str(),
    ]
    .iter()
    .filter(|receipt_id| !receipt_id.trim().is_empty())
    .map(|receipt_id| (*receipt_id).to_string())
    .collect()
}

fn candidate(
    source_id: &str,
    source_type: &'static str,
    priority: usize,
    title: &str,
    content: &str,
    request: &LocalContextRequest<'_>,
    importance: usize,
) -> LocalContextCandidate {
    LocalContextCandidate {
        source_id: source_id.to_string(),
        source_type,
        priority,
        title: title.to_string(),
        content: content.to_string(),
        token_estimate: token_estimate(content),
        score: score_context(request, content, importance, priority),
        source_receipt_ids: Vec::new(),
        memory_driver: None,
        memory_provider: None,
        memory_tier: None,
        world_model_source: None,
    }
}

fn score_context(
    request: &LocalContextRequest<'_>,
    content: &str,
    importance: usize,
    priority: usize,
) -> LocalContextScore {
    let lexical = lexical_score(request.query, content).min(100);
    let recency = 100usize.saturating_sub(priority.saturating_mul(8)).max(20);
    let graph = graph_score(request.query_entities, content);
    let composite = (lexical * 42) + (recency * 24) + (importance * 17) + (graph * 17);
    LocalContextScore {
        lexical,
        recency,
        importance,
        graph,
        composite,
    }
}

fn lexical_score(query: &str, content: &str) -> usize {
    let content = content.to_ascii_lowercase();
    let terms = query
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|term| term.len() > 2)
        .collect::<Vec<_>>();
    if terms.is_empty() {
        return 0;
    }
    let hits = terms
        .iter()
        .filter(|term| content.contains(&term.to_ascii_lowercase()))
        .count();
    ((hits * 100) / terms.len()).min(100)
}

fn graph_score(query_entities: &[&str], content: &str) -> usize {
    if query_entities.is_empty() {
        return 0;
    }
    let content = content.to_ascii_lowercase();
    let hits = query_entities
        .iter()
        .filter(|entity| content.contains(&entity.to_ascii_lowercase()))
        .count();
    ((hits * 100) / query_entities.len()).min(100)
}

fn source_enabled(source_type: &str, preferences: &[LocalContextSourcePreference<'_>]) -> bool {
    preferences
        .iter()
        .find(|preference| preference.source_type == source_type)
        .map(|preference| preference.enabled)
        .unwrap_or(true)
}

fn source_override(
    source_type: &str,
    preferences: &[LocalContextSourcePreference<'_>],
) -> Option<usize> {
    preferences
        .iter()
        .find(|preference| preference.source_type == source_type)
        .and_then(|preference| preference.token_budget_override)
}

fn disabled_source_count(preferences: &[LocalContextSourcePreference<'_>]) -> usize {
    preferences
        .iter()
        .filter(|preference| !preference.enabled)
        .count()
}

fn dedupe_sources(sources: Vec<LocalContextCandidate>) -> Vec<LocalContextCandidate> {
    let mut deduped = Vec::new();
    for source in sources {
        if !deduped
            .iter()
            .any(|kept: &LocalContextCandidate| kept.content == source.content)
        {
            deduped.push(source);
        }
    }
    deduped
}

fn hygiene_report(sources: &[LocalContextCandidate]) -> LocalContextHygieneReport {
    let tokens_in = sources
        .iter()
        .map(|source| source.token_estimate)
        .sum::<usize>();
    let deduped = dedupe_sources(sources.to_vec());
    let tokens_out = deduped
        .iter()
        .map(|source| source.token_estimate)
        .sum::<usize>();
    let duplicate_sources_removed = sources.len().saturating_sub(deduped.len());
    let reduction_percent_x100 = tokens_in
        .saturating_sub(tokens_out)
        .checked_mul(10_000)
        .and_then(|reduction| reduction.checked_div(tokens_in))
        .unwrap_or(0);
    let mut strategies_applied = vec!["dedupe", "prune_by_relevance"];
    if duplicate_sources_removed == 0 {
        strategies_applied = vec!["prune_by_relevance"];
    }
    LocalContextHygieneReport {
        tokens_in,
        tokens_out,
        reduction_percent_x100,
        duplicate_sources_removed,
        strategies_applied,
    }
}

fn memory_tier_reports(sources: &[LocalContextCandidate]) -> Vec<LocalContextMemoryTierReport> {
    ["working", "episodic", "semantic", "procedural"]
        .iter()
        .map(|tier| {
            let tier_sources = sources
                .iter()
                .filter(|source| source.memory_tier == Some(*tier))
                .collect::<Vec<_>>();
            LocalContextMemoryTierReport {
                tier,
                source_count: tier_sources.len(),
                included_source_count: 0,
                token_count: tier_sources
                    .iter()
                    .map(|source| source.token_estimate)
                    .sum::<usize>(),
                retention_policy: memory_retention_policy(tier),
                recall_policy: memory_recall_policy(tier),
                write_path: memory_write_path(tier),
                cache_driver: if *tier == "working" {
                    Some("valkey_ready_local_projection")
                } else {
                    None
                },
                consolidation_state: memory_consolidation_state(tier),
            }
        })
        .collect()
}

fn normalize_memory_tier(tier: &str) -> &'static str {
    match tier {
        "working" => "working",
        "semantic" => "semantic",
        "procedural" => "procedural",
        _ => "episodic",
    }
}

fn memory_source_type(tier: &str) -> &'static str {
    match tier {
        "working" => "working_memory",
        "episodic" => "episodic_memory",
        "semantic" | "procedural" => "compiled_knowledge",
        _ => "episodic_memory",
    }
}

fn memory_priority(tier: &str) -> usize {
    match tier {
        "working" => 3,
        "episodic" => 4,
        "semantic" | "procedural" => 5,
        _ => 4,
    }
}

fn merge_included_tier_counts(
    mut reports: Vec<LocalContextMemoryTierReport>,
    included: &[LocalContextSource],
) -> Vec<LocalContextMemoryTierReport> {
    for report in &mut reports {
        report.included_source_count = included
            .iter()
            .filter(|source| source.memory_tier == Some(report.tier))
            .count();
    }
    reports
}

fn memory_retention_policy(tier: &str) -> &'static str {
    match tier {
        "working" => "session_scoped_ttl_86400s",
        "episodic" => "conversation_turns_with_recency_decay",
        "semantic" => "long_lived_deduplicated_facts",
        "procedural" => "cross_agent_patterns_and_preferences",
        _ => "unknown",
    }
}

fn memory_recall_policy(tier: &str) -> &'static str {
    match tier {
        "working" => "redis_first_postgres_fallback_projection",
        "episodic" => "semantic_similarity_recency_importance_rerank",
        "semantic" => "semantic_similarity_importance_dedup_rerank",
        "procedural" => "cross_agent_similarity_effectiveness_affinity_rerank",
        _ => "unknown",
    }
}

fn memory_write_path(tier: &str) -> &'static str {
    match tier {
        "working" => "write_through_postgres_plus_valkey_projection",
        "episodic" => "postgres_memory_record_with_embedding_slot",
        "semantic" => "human_reviewed_consolidation_promotion",
        "procedural" => "human_reviewed_pattern_promotion",
        _ => "unknown",
    }
}

fn memory_consolidation_state(tier: &str) -> &'static str {
    match tier {
        "working" => "not_promoted_directly",
        "episodic" => "promotion_candidates_scanned",
        "semantic" | "procedural" => "human_review_required",
        _ => "unknown",
    }
}

fn consolidation_report(sources: &[LocalContextCandidate]) -> LocalContextConsolidationReport {
    let candidate_count = sources
        .iter()
        .filter(|source| {
            matches!(
                source.memory_tier,
                Some("episodic" | "semantic" | "procedural")
            )
        })
        .count();
    LocalContextConsolidationReport {
        status: "LIVE-LOCAL-CONSOLIDATION-PROPOSAL-FLOOR",
        candidate_count,
        pending_review_count: candidate_count,
        auto_promotion_allowed: false,
        human_review_required: true,
        dedup_threshold_percent_x100: 9500,
        promotion_targets: vec!["semantic", "procedural"],
    }
}

fn handoff_report() -> LocalContextHandoffReport {
    LocalContextHandoffReport {
        status: "LIVE-LOCAL-HANDOFF-SUMMARY-FLOOR",
        summary_sections: vec![
            "key_findings",
            "decisions_made",
            "constraints_identified",
            "open_questions",
            "recommended_next_steps",
        ],
        strategy_order: vec![
            "dedupe",
            "prune_by_recency",
            "prune_by_relevance",
            "summarize",
        ],
        max_summary_tokens: 800,
        preserve_verbatim_supported: true,
    }
}

fn vector_report() -> LocalContextVectorReport {
    LocalContextVectorReport {
        status: "LOCAL-PGVECTOR-PROJECTION-READY-PROVIDER-BLOCKED",
        driver: "pgvector_ready_local_projection",
        table: "ctx_memories",
        dimensions: 1536,
        tenant_scoped: true,
        live_index_ready: false,
        provider_call_allowed: false,
    }
}

fn working_cache_report() -> LocalContextWorkingCacheReport {
    LocalContextWorkingCacheReport {
        status: "LOCAL-VALKEY-PROJECTION-READY-PROVIDER-BLOCKED",
        driver: "valkey_ready_local_projection",
        namespace: "ctx:working:{tenant_id}:{session_id}:{key}",
        ttl_seconds: 86_400,
        write_through_postgres: true,
        live_cache_ready: false,
    }
}

fn signal_report(sources: &[LocalContextCandidate]) -> LocalContextSignalReport {
    LocalContextSignalReport {
        status: "LIVE-LOCAL-SIGNAL-INTELLIGENCE-FLOOR",
        signal_source_count: sources
            .iter()
            .filter(|source| source.source_type == "signal_intelligence")
            .count(),
        search_policy: "tenant_scoped_semantic_filters_projection",
        cross_connection_policy: "bounded_local_similarity_scan",
        tenant_scoped: true,
    }
}

fn event_stream_report() -> LocalContextEventStreamReport {
    LocalContextEventStreamReport {
        status: "PENDING-LIVE-RUN",
        stream: "ctx_event_stream",
        websocket_allowed: false,
        provider_call_allowed: false,
        replay_policy: "pending_relay_or_local_event_bus_decision",
    }
}

fn allocate_source_budgets(
    total_budget: usize,
    sources: &[LocalContextCandidate],
    preferences: &[LocalContextSourcePreference<'_>],
) -> Vec<LocalContextBudgetAllocation> {
    let mut source_types = sources
        .iter()
        .map(|source| (source.source_type, source.priority))
        .collect::<Vec<_>>();
    source_types.sort_by(|left, right| left.0.cmp(right.0));
    source_types.dedup_by(|left, right| left.0 == right.0);
    source_types.sort_by(|left, right| left.1.cmp(&right.1).then_with(|| left.0.cmp(right.0)));

    let fixed_total = source_types
        .iter()
        .filter(|(source_type, _)| fixed_source(source_type))
        .map(|(source_type, _)| source_override(source_type, preferences).unwrap_or(24))
        .sum::<usize>();
    let dynamic_budget = total_budget.saturating_sub(fixed_total);
    let dynamic_weight = source_types
        .iter()
        .filter(|(source_type, _)| !fixed_source(source_type))
        .map(|(_, priority)| 10usize.saturating_sub(*priority).max(1))
        .sum::<usize>()
        .max(1);
    source_types
        .iter()
        .map(|(source_type, priority)| {
            let fixed = fixed_source(source_type);
            let token_budget = source_override(source_type, preferences).unwrap_or_else(|| {
                if fixed {
                    24
                } else {
                    let weight = 10usize.saturating_sub(*priority).max(1);
                    (dynamic_budget * weight / dynamic_weight).max(1)
                }
            });
            LocalContextBudgetAllocation {
                source_type,
                priority: *priority,
                token_budget,
                fixed,
            }
        })
        .collect()
}

fn fit_context_sources(
    ranked: Vec<LocalContextCandidate>,
    allocations: &[LocalContextBudgetAllocation],
    total_budget: usize,
) -> (
    Vec<LocalContextSource>,
    Vec<LocalContextTruncatedSource>,
    usize,
) {
    let mut included = Vec::new();
    let mut truncated = Vec::new();
    let mut total_tokens = 0usize;
    for candidate in ranked {
        let allocation = allocations
            .iter()
            .find(|allocation| allocation.source_type == candidate.source_type)
            .expect("allocation for context source");
        if allocation.fixed
            || (total_tokens + candidate.token_estimate <= total_budget)
            || included.is_empty()
        {
            total_tokens += candidate.token_estimate;
            included.push(LocalContextSource {
                source_id: candidate.source_id,
                source_type: candidate.source_type,
                priority: candidate.priority,
                title: candidate.title,
                content: candidate.content,
                token_estimate: candidate.token_estimate,
                budget_allocated: allocation.token_budget,
                fixed: allocation.fixed,
                score: candidate.score,
                source_receipt_ids: candidate.source_receipt_ids,
                memory_driver: candidate.memory_driver,
                memory_provider: candidate.memory_provider,
                memory_tier: candidate.memory_tier,
                world_model_source: candidate.world_model_source,
            });
        } else {
            truncated.push(LocalContextTruncatedSource {
                source_id: candidate.source_id,
                source_type: candidate.source_type,
                token_estimate: candidate.token_estimate,
                composite_score: candidate.score.composite,
                reason: "excluded_by_budget",
            });
        }
    }
    (included, truncated, total_tokens)
}

fn fixed_source(source_type: &str) -> bool {
    matches!(source_type, "system_prompt" | "tier1_knowledge")
}

fn fixed_sort_key(source_type: &str) -> usize {
    usize::from(!fixed_source(source_type))
}

fn estimated_p95_ms(source_count: usize, included_count: usize) -> usize {
    (8 + source_count + included_count).min(HOT_PATH_BUDGET_MS)
}

fn token_estimate(content: &str) -> usize {
    content.split_whitespace().count().max(1)
}
