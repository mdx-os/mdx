use serde_json::{Value, json};

const STATUS: &str = "LIVE-LOCAL-CTX-WORLD-MODEL-RETRIEVAL-FLOOR";
const DEFAULT_TOP_K: usize = 8;
const DEFAULT_TOKEN_BUDGET: usize = 260;
const MAX_SOURCE_CANDIDATES: usize = 64;

#[derive(Default)]
pub struct CtxWorldModelRetrievalRuntime {
    retrievals: Vec<CtxWorldModelRetrieval>,
    next_retrieval: usize,
}

pub struct CtxWorldModelRetrievalResult {
    pub body: String,
    pub events: Vec<CtxWorldModelRetrievalEvent>,
}

pub struct CtxWorldModelRetrievalEvent {
    pub event_type: String,
    pub tenant_id: String,
    pub subject_id: String,
}

#[derive(Clone)]
pub struct CtxWorldModelMemoryCandidate {
    pub memory_id: String,
    pub tier: String,
    pub content: String,
    pub source_receipt_id: String,
    pub importance: usize,
    pub access_count: usize,
    pub usefulness_score_percent: usize,
    pub edge_weight_percent: usize,
}

#[derive(Clone)]
struct CtxWorldModelRetrieval {
    retrieval_id: String,
    tenant_id: String,
    actor_id: String,
    query: String,
    query_entities: Vec<String>,
    token_budget: usize,
    top_k: usize,
    terminal_state: String,
    rejected: bool,
    rejection_reason: String,
    source_candidate_count: usize,
    ranked_count: usize,
    included_count: usize,
    truncated_count: usize,
    total_tokens: usize,
    budget_remaining: usize,
    vector_hits_requested: bool,
    vector_provider_blocked: bool,
    vector_status: String,
    working_cache_status: String,
    event_stream_status: String,
    ranked_sources: Vec<RankedWorldModelSource>,
    included_sources: Vec<RankedWorldModelSource>,
    truncated_sources: Vec<RankedWorldModelSource>,
    source_mix: Vec<SourceMixCount>,
}

#[derive(Clone)]
struct RankedWorldModelSource {
    source_id: String,
    source_type: String,
    title: String,
    content: String,
    receipt_id: String,
    memory_tier: String,
    token_estimate: usize,
    lexical_score: usize,
    recency_score: usize,
    importance_score: usize,
    graph_score: usize,
    composite_score: usize,
    vector_score: usize,
    local_only: bool,
}

#[derive(Clone)]
struct SourceMixCount {
    source_type: String,
    candidate_count: usize,
    included_count: usize,
    truncated_count: usize,
}

struct StaticWorldModelSource {
    source_id: &'static str,
    source_type: &'static str,
    title: &'static str,
    content: &'static str,
    receipt_id: &'static str,
    memory_tier: &'static str,
    token_estimate: usize,
    recency_score: usize,
    importance_score: usize,
}

struct RetrievalRequest {
    tenant_id: String,
    actor_id: String,
    query: String,
    query_entities: Vec<String>,
    token_budget: usize,
    top_k: usize,
    vector_hits_requested: bool,
    provider_calls_requested: bool,
    embedding_provider_calls_requested: bool,
    vendor_memory_requested: bool,
    network_requested: bool,
    secret_export_requested: bool,
    production_write_requested: bool,
    embedding_values_recorded: bool,
    private_memory_bodies_exported: bool,
}

impl CtxWorldModelRetrievalRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn status(&self) -> &'static str {
        STATUS
    }

    pub fn submit_json(
        &mut self,
        body: &str,
        memories: &[CtxWorldModelMemoryCandidate],
        vector_status: &str,
        working_cache_status: &str,
        event_stream_status: &str,
    ) -> Result<CtxWorldModelRetrievalResult, String> {
        let request = parse_retrieval_request(body)?;
        self.next_retrieval += 1;
        let unsafe_authority = request.provider_calls_requested
            || request.embedding_provider_calls_requested
            || request.vendor_memory_requested
            || request.network_requested
            || request.secret_export_requested
            || request.production_write_requested
            || request.embedding_values_recorded
            || request.private_memory_bodies_exported;
        let missing_evidence = request.query.trim().is_empty() || request.token_budget == 0;
        let (terminal_state, rejected, rejection_reason) = if unsafe_authority {
            (
                "CTX_WORLD_MODEL_RETRIEVAL_REJECTED_SECURITY_BOUNDARY",
                true,
                "provider_vendor_network_secret_embedding_or_production_authority_requested",
            )
        } else if missing_evidence {
            (
                "CTX_WORLD_MODEL_RETRIEVAL_REJECTED_MISSING_EVIDENCE",
                true,
                "query_and_token_budget_required",
            )
        } else if request.vector_hits_requested {
            (
                "CTX_WORLD_MODEL_RETRIEVAL_RECORDED_VECTOR_PROVIDER_BLOCKED",
                false,
                "none",
            )
        } else {
            (
                "CTX_WORLD_MODEL_RETRIEVAL_RECORDED_RANKED_LOCAL",
                false,
                "none",
            )
        };

        let candidates = if rejected {
            Vec::new()
        } else {
            source_candidates(&request, memories)
        };
        let ranked_sources = rank_sources(&request, candidates);
        let (included_sources, truncated_sources, total_tokens, budget_remaining) =
            fit_budget(&ranked_sources, request.token_budget, request.top_k);
        let terminal_state = if !rejected
            && !request.vector_hits_requested
            && !truncated_sources.is_empty()
            && request.token_budget < DEFAULT_TOKEN_BUDGET
        {
            "CTX_WORLD_MODEL_RETRIEVAL_RECORDED_BUDGET_TRUNCATED"
        } else {
            terminal_state
        };
        let source_mix = source_mix(&ranked_sources, &included_sources, &truncated_sources);
        let retrieval = CtxWorldModelRetrieval {
            retrieval_id: format!("ctx_world_model_retrieval_{:06}", self.next_retrieval),
            tenant_id: request.tenant_id,
            actor_id: request.actor_id,
            query: request.query,
            query_entities: request.query_entities,
            token_budget: request.token_budget,
            top_k: request.top_k,
            terminal_state: terminal_state.to_string(),
            rejected,
            rejection_reason: rejection_reason.to_string(),
            source_candidate_count: ranked_sources.len(),
            ranked_count: ranked_sources.len(),
            included_count: included_sources.len(),
            truncated_count: truncated_sources.len(),
            total_tokens,
            budget_remaining,
            vector_hits_requested: request.vector_hits_requested,
            vector_provider_blocked: request.vector_hits_requested,
            vector_status: vector_status.to_string(),
            working_cache_status: working_cache_status.to_string(),
            event_stream_status: event_stream_status.to_string(),
            ranked_sources,
            included_sources,
            truncated_sources,
            source_mix,
        };
        let events = retrieval_events(&retrieval);
        let body = render_retrieval_response_json(&retrieval);
        self.retrievals.push(retrieval);
        Ok(CtxWorldModelRetrievalResult { body, events })
    }

    pub fn projection_json(&self) -> String {
        let ranked_count = self
            .retrievals
            .iter()
            .filter(|retrieval| {
                retrieval.terminal_state == "CTX_WORLD_MODEL_RETRIEVAL_RECORDED_RANKED_LOCAL"
            })
            .count();
        let truncated_count = self
            .retrievals
            .iter()
            .filter(|retrieval| {
                retrieval.terminal_state == "CTX_WORLD_MODEL_RETRIEVAL_RECORDED_BUDGET_TRUNCATED"
            })
            .count();
        let vector_blocked_count = self
            .retrievals
            .iter()
            .filter(|retrieval| {
                retrieval.terminal_state
                    == "CTX_WORLD_MODEL_RETRIEVAL_RECORDED_VECTOR_PROVIDER_BLOCKED"
            })
            .count();
        let rejected_count = self
            .retrievals
            .iter()
            .filter(|retrieval| retrieval.rejected)
            .count();
        json!({
            "name": "mdx-ctx-world-model-retrievals",
            "status": STATUS,
            "runtime": "mdx-ctx-engine",
            "route": "/ctx/world-model-retrievals.json",
            "submit_route": "/v1/ctx/world-model-retrievals",
            "retrieval_count": self.retrievals.len(),
            "ranked_local_count": ranked_count,
            "budget_truncated_count": truncated_count,
            "vector_provider_blocked_count": vector_blocked_count,
            "rejected_count": rejected_count,
            "max_source_candidates": MAX_SOURCE_CANDIDATES,
            "default_top_k": DEFAULT_TOP_K,
            "default_token_budget": DEFAULT_TOKEN_BUDGET,
            "rank_formula": "lexical_42_recency_24_importance_17_graph_17_plus_vector_hint",
            "source_types": [
                "pages_world_model_document",
                "episodic_memory",
                "semantic_memory",
                "procedural_memory",
                "working_memory",
                "compiled_knowledge",
                "signal_intelligence",
                "rag_projection"
            ],
            "required_gates": [
                "tenant_scoped_query",
                "source_receipts",
                "budget_fitting",
                "ranking_components",
                "vector_provider_blocked_until_turn_on",
                "no_raw_embeddings",
                "no_secret_values",
                "no_vendor_memory_export",
                "no_production_writes"
            ],
            "provider_calls_allowed": false,
            "embedding_provider_calls_allowed": false,
            "vendor_memory_allowed": false,
            "network_allowed": false,
            "secret_export_allowed": false,
            "embedding_values_recorded": false,
            "private_memory_bodies_exported": false,
            "production_writes_allowed": false,
            "retrievals": self.retrievals.iter().map(retrieval_value).collect::<Vec<_>>()
        })
        .to_string()
    }
}

fn parse_retrieval_request(body: &str) -> Result<RetrievalRequest, String> {
    let value: Value = serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid CTX world model retrieval json: {error}"))?;
    Ok(RetrievalRequest {
        tenant_id: string_value(&value, "tenant_id", "tenant_local"),
        actor_id: string_value(&value, "actor_id", "local_user"),
        query: string_value(&value, "query", "forge runtime grounded memory"),
        query_entities: string_array(&value, "query_entities"),
        token_budget: usize_value(&value, "token_budget", DEFAULT_TOKEN_BUDGET),
        top_k: usize_value(&value, "top_k", DEFAULT_TOP_K).clamp(1, 32),
        vector_hits_requested: bool_value(&value, "vector_hits_requested", false)
            || bool_value(&value, "vector_retrieval_requested", false),
        provider_calls_requested: bool_value(&value, "provider_calls_requested", false)
            || bool_value(&value, "provider_calls_allowed", false),
        embedding_provider_calls_requested: bool_value(
            &value,
            "embedding_provider_calls_requested",
            false,
        ) || bool_value(
            &value,
            "embedding_provider_calls_allowed",
            false,
        ),
        vendor_memory_requested: bool_value(&value, "vendor_memory_requested", false)
            || bool_value(&value, "vendor_memory_allowed", false),
        network_requested: bool_value(&value, "network_requested", false)
            || bool_value(&value, "network_allowed", false),
        secret_export_requested: bool_value(&value, "secret_export_requested", false)
            || bool_value(&value, "secret_export_allowed", false),
        production_write_requested: bool_value(&value, "production_write_requested", false)
            || bool_value(&value, "production_writes_allowed", false),
        embedding_values_recorded: bool_value(&value, "embedding_values_recorded", false),
        private_memory_bodies_exported: bool_value(&value, "private_memory_bodies_exported", false),
    })
}

fn source_candidates(
    request: &RetrievalRequest,
    memories: &[CtxWorldModelMemoryCandidate],
) -> Vec<RankedWorldModelSource> {
    let mut candidates = static_sources()
        .into_iter()
        .map(static_candidate)
        .collect::<Vec<_>>();

    candidates.extend(memories.iter().take(32).map(memory_candidate));
    candidates
        .into_iter()
        .take(MAX_SOURCE_CANDIDATES)
        .map(|mut source| {
            let lexical = lexical_score(&request.query, &source.content, &source.title);
            let graph = graph_score(&request.query_entities, &source.content, &source.title);
            let vector = if request.vector_hits_requested {
                lexical.saturating_add(graph / 2).min(100)
            } else {
                0
            };
            source.lexical_score = lexical.max(source.lexical_score / 2);
            source.graph_score = graph;
            source.vector_score = vector;
            source
        })
        .collect()
}

fn memory_candidate(memory: &CtxWorldModelMemoryCandidate) -> RankedWorldModelSource {
    let source_type = match memory.tier.as_str() {
        "procedural" => "procedural_memory",
        "semantic" => "semantic_memory",
        "working" => "working_memory",
        _ => "episodic_memory",
    };
    RankedWorldModelSource {
        source_id: memory.memory_id.clone(),
        source_type: source_type.to_string(),
        title: format!("{} local memory", memory.tier),
        content: memory.content.clone(),
        receipt_id: memory.source_receipt_id.clone(),
        memory_tier: memory.tier.clone(),
        token_estimate: estimate_tokens(&memory.content).max(16),
        lexical_score: 0,
        recency_score: memory
            .access_count
            .saturating_mul(7)
            .saturating_add(55)
            .min(100),
        importance_score: memory
            .importance
            .max(memory.usefulness_score_percent)
            .max((memory.edge_weight_percent / 5).min(100))
            .min(100),
        graph_score: 0,
        composite_score: 0,
        vector_score: 0,
        local_only: true,
    }
}

fn static_sources() -> Vec<StaticWorldModelSource> {
    vec![
        StaticWorldModelSource {
            source_id: "pages_world_model_onboarding",
            source_type: "pages_world_model_document",
            title: "Pages onboarding world model",
            content: "Pages onboarding documents define the local install, proof routes, receipts, and operator evidence needed before DXR execution.",
            receipt_id: "pages_world_model_projection_receipt",
            memory_tier: "semantic",
            token_estimate: 48,
            recency_score: 88,
            importance_score: 82,
        },
        StaticWorldModelSource {
            source_id: "compiled_knowledge_focus_runtime",
            source_type: "compiled_knowledge",
            title: "Compiled focus knowledge",
            content: "Focus compiles watched runtime signals into semantic knowledge without provider calls, network access, or production writes.",
            receipt_id: "ctx_focus_compiled_receipt",
            memory_tier: "semantic",
            token_estimate: 42,
            recency_score: 79,
            importance_score: 76,
        },
        StaticWorldModelSource {
            source_id: "signal_runtime_dxr_ctx_floor",
            source_type: "signal_intelligence",
            title: "DXR and CTX runtime floor signal",
            content: "Signal intelligence carries source trends and cross-connections that should ground Forge and DXR work before execution.",
            receipt_id: "ctx_signal_runtime_receipt",
            memory_tier: "semantic",
            token_estimate: 36,
            recency_score: 84,
            importance_score: 74,
        },
        StaticWorldModelSource {
            source_id: "rag_pages_world_model_projection",
            source_type: "rag_projection",
            title: "Pages world model projection",
            content: "RAG projection binds Pages documents to receipts, tenant scope, source references, and deterministic assembly budgets.",
            receipt_id: "ctx_rag_pages_projection_receipt",
            memory_tier: "semantic",
            token_estimate: 40,
            recency_score: 75,
            importance_score: 72,
        },
        StaticWorldModelSource {
            source_id: "working_memory_local_session",
            source_type: "working_memory",
            title: "Local session working memory",
            content: "Working memory keeps the current tenant session, actor intent, handoff summary, and active execution boundary visible.",
            receipt_id: "ctx_working_memory_receipt",
            memory_tier: "working",
            token_estimate: 34,
            recency_score: 70,
            importance_score: 68,
        },
    ]
}

fn static_candidate(source: StaticWorldModelSource) -> RankedWorldModelSource {
    RankedWorldModelSource {
        source_id: source.source_id.to_string(),
        source_type: source.source_type.to_string(),
        title: source.title.to_string(),
        content: source.content.to_string(),
        receipt_id: source.receipt_id.to_string(),
        memory_tier: source.memory_tier.to_string(),
        token_estimate: source.token_estimate,
        lexical_score: 0,
        recency_score: source.recency_score,
        importance_score: source.importance_score,
        graph_score: 0,
        composite_score: 0,
        vector_score: 0,
        local_only: true,
    }
}

fn rank_sources(
    request: &RetrievalRequest,
    mut candidates: Vec<RankedWorldModelSource>,
) -> Vec<RankedWorldModelSource> {
    for candidate in &mut candidates {
        let composite = candidate.lexical_score.saturating_mul(42)
            + candidate.recency_score.saturating_mul(24)
            + candidate.importance_score.saturating_mul(17)
            + candidate.graph_score.saturating_mul(17)
            + candidate.vector_score.saturating_mul(8);
        candidate.composite_score = (composite
            / if request.vector_hits_requested {
                108
            } else {
                100
            })
        .min(100);
    }
    candidates.sort_by(|left, right| {
        right
            .composite_score
            .cmp(&left.composite_score)
            .then_with(|| right.importance_score.cmp(&left.importance_score))
            .then_with(|| left.source_id.cmp(&right.source_id))
    });
    candidates
}

fn fit_budget(
    ranked: &[RankedWorldModelSource],
    token_budget: usize,
    top_k: usize,
) -> (
    Vec<RankedWorldModelSource>,
    Vec<RankedWorldModelSource>,
    usize,
    usize,
) {
    let mut included = Vec::new();
    let mut truncated = Vec::new();
    let mut used = 0_usize;
    for source in ranked {
        if included.len() >= top_k {
            truncated.push(source.clone());
            continue;
        }
        if used.saturating_add(source.token_estimate) <= token_budget {
            used = used.saturating_add(source.token_estimate);
            included.push(source.clone());
        } else {
            truncated.push(source.clone());
        }
    }
    let remaining = token_budget.saturating_sub(used);
    (included, truncated, used, remaining)
}

fn source_mix(
    ranked: &[RankedWorldModelSource],
    included: &[RankedWorldModelSource],
    truncated: &[RankedWorldModelSource],
) -> Vec<SourceMixCount> {
    let mut types = ranked
        .iter()
        .map(|source| source.source_type.clone())
        .collect::<Vec<_>>();
    types.sort();
    types.dedup();
    types
        .into_iter()
        .map(|source_type| SourceMixCount {
            candidate_count: ranked
                .iter()
                .filter(|source| source.source_type == source_type)
                .count(),
            included_count: included
                .iter()
                .filter(|source| source.source_type == source_type)
                .count(),
            truncated_count: truncated
                .iter()
                .filter(|source| source.source_type == source_type)
                .count(),
            source_type,
        })
        .collect()
}

fn retrieval_events(retrieval: &CtxWorldModelRetrieval) -> Vec<CtxWorldModelRetrievalEvent> {
    let mut events = vec![
        event(retrieval, "world_model_retrieval_recorded"),
        event(retrieval, "world_model_query_intent_bound"),
    ];
    if retrieval.rejected {
        events.push(event(retrieval, "world_model_retrieval_rejected"));
        return events;
    }
    events.extend([
        event(retrieval, "world_model_pages_projection_ranked"),
        event(retrieval, "world_model_memory_tiers_ranked"),
        event(retrieval, "world_model_signal_sources_ranked"),
        event(retrieval, "world_model_budget_applied"),
        event(retrieval, "world_model_retrieval_authority_blocked"),
    ]);
    if retrieval.truncated_count > 0 {
        events.push(event(retrieval, "world_model_truncation_retained"));
    }
    if retrieval.vector_provider_blocked {
        events.push(event(retrieval, "world_model_vector_provider_blocked"));
    }
    events
}

fn event(retrieval: &CtxWorldModelRetrieval, event_type: &str) -> CtxWorldModelRetrievalEvent {
    CtxWorldModelRetrievalEvent {
        event_type: event_type.to_string(),
        tenant_id: retrieval.tenant_id.clone(),
        subject_id: retrieval.retrieval_id.clone(),
    }
}

fn render_retrieval_response_json(retrieval: &CtxWorldModelRetrieval) -> String {
    let mut value = retrieval_value(retrieval);
    value["name"] = json!("mdx-ctx-world-model-retrieval");
    value["status"] = json!(STATUS);
    value.to_string()
}

fn retrieval_value(retrieval: &CtxWorldModelRetrieval) -> Value {
    json!({
        "retrieval_id": retrieval.retrieval_id,
        "tenant_id": retrieval.tenant_id,
        "actor_id": retrieval.actor_id,
        "query": retrieval.query,
        "query_entities": retrieval.query_entities,
        "terminal_state": retrieval.terminal_state,
        "rejected": retrieval.rejected,
        "rejection_reason": retrieval.rejection_reason,
        "token_budget": retrieval.token_budget,
        "top_k": retrieval.top_k,
        "source_candidate_count": retrieval.source_candidate_count,
        "ranked_count": retrieval.ranked_count,
        "included_count": retrieval.included_count,
        "truncated_count": retrieval.truncated_count,
        "total_tokens": retrieval.total_tokens,
        "budget_remaining": retrieval.budget_remaining,
        "estimated_p95_ms": 18,
        "hot_path_budget_ms": 40,
        "rank_formula": "lexical_42_recency_24_importance_17_graph_17_plus_vector_hint",
        "source_mix": retrieval.source_mix.iter().map(source_mix_value).collect::<Vec<_>>(),
        "ranked_sources": retrieval.ranked_sources.iter().map(source_value).collect::<Vec<_>>(),
        "included_sources": retrieval.included_sources.iter().map(source_value).collect::<Vec<_>>(),
        "truncated_sources": retrieval.truncated_sources.iter().map(source_value).collect::<Vec<_>>(),
        "vector_retrieval": {
            "requested": retrieval.vector_hits_requested,
            "status": retrieval.vector_status,
            "table": "ctx_memory_vectors",
            "index": "ctx_memory_vectors_embedding_hnsw_idx",
            "dimensions": 1536,
            "distance_metric": "cosine",
            "provider_blocked_until_turn_on": retrieval.vector_provider_blocked,
            "embedding_values_recorded": false,
            "provider_calls_allowed": false,
            "production_writes_allowed": false
        },
        "working_cache": {
            "status": retrieval.working_cache_status,
            "driver": "valkey",
            "provider_calls_allowed": false,
            "production_writes_allowed": false
        },
        "event_stream": {
            "status": retrieval.event_stream_status,
            "driver": "mdx-message-relay",
            "topic_pattern": "mdx:ctx:{tenant_id}:events",
            "provider_calls_allowed": false,
            "production_writes_allowed": false
        },
        "provider_calls_allowed": false,
        "embedding_provider_calls_allowed": false,
        "vendor_memory_allowed": false,
        "network_allowed": false,
        "secret_export_allowed": false,
        "embedding_values_recorded": false,
        "private_memory_bodies_exported": false,
        "production_writes_allowed": false
    })
}

fn source_value(source: &RankedWorldModelSource) -> Value {
    json!({
        "source_id": source.source_id,
        "source_type": source.source_type,
        "title": source.title,
        "content": source.content,
        "source_receipt_ids": [source.receipt_id],
        "memory_tier": source.memory_tier,
        "token_estimate": source.token_estimate,
        "score": {
            "lexical": source.lexical_score,
            "recency": source.recency_score,
            "importance": source.importance_score,
            "graph": source.graph_score,
            "vector": source.vector_score,
            "composite": source.composite_score
        },
        "local_only": source.local_only,
        "provider_calls_allowed": false,
        "production_writes_allowed": false
    })
}

fn source_mix_value(source_mix: &SourceMixCount) -> Value {
    json!({
        "source_type": source_mix.source_type,
        "candidate_count": source_mix.candidate_count,
        "included_count": source_mix.included_count,
        "truncated_count": source_mix.truncated_count
    })
}

fn lexical_score(query: &str, content: &str, title: &str) -> usize {
    let haystack = format!(
        "{} {}",
        title.to_ascii_lowercase(),
        content.to_ascii_lowercase()
    );
    let terms = query_terms(query);
    if terms.is_empty() {
        return 0;
    }
    let hits = terms
        .iter()
        .filter(|term| haystack.contains(term.as_str()))
        .count();
    ((hits * 100) / terms.len()).min(100)
}

fn graph_score(query_entities: &[String], content: &str, title: &str) -> usize {
    if query_entities.is_empty() {
        return 0;
    }
    let haystack = format!(
        "{} {}",
        title.to_ascii_lowercase(),
        content.to_ascii_lowercase()
    );
    let hits = query_entities
        .iter()
        .filter(|entity| haystack.contains(&entity.to_ascii_lowercase()))
        .count();
    ((hits * 100) / query_entities.len()).min(100)
}

fn query_terms(query: &str) -> Vec<String> {
    query
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|term| term.len() > 2)
        .map(|term| term.to_ascii_lowercase())
        .collect()
}

fn estimate_tokens(content: &str) -> usize {
    content.split_whitespace().count().saturating_mul(4).max(8)
}

fn string_value(value: &Value, key: &str, default: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or(default)
        .to_string()
}

fn bool_value(value: &Value, key: &str, default: bool) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn usize_value(value: &Value, key: &str, default: usize) -> usize {
    value
        .get(key)
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(default)
}

fn string_array(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_model_retrieval_records_ranked_truncated_provider_blocked_and_rejects_authority() {
        let mut runtime = CtxWorldModelRetrievalRuntime::new();
        let memories = vec![CtxWorldModelMemoryCandidate {
            memory_id: "ctx_memory_runtime_episodic_0001".to_string(),
            tier: "episodic".to_string(),
            content: "Forge runtime work must consume CTX world model memory before DXR execution."
                .to_string(),
            source_receipt_id: "ctx_runtime_memory_receipt_0001".to_string(),
            importance: 98,
            access_count: 1,
            usefulness_score_percent: 94,
            edge_weight_percent: 140,
        }];
        let ranked = runtime
            .submit_json(
                r#"{"query":"forge runtime world model","query_entities":["forge","runtime"],"token_budget":260}"#,
                &memories,
                "POSTGRES_PGVECTOR_CONFIGURED",
                "VALKEY_DOCKER_LOCAL",
                "LIVE-LOCAL-CTX-WEBSOCKET-RELAY",
            )
            .expect("ranked retrieval should record");
        assert!(
            ranked
                .body
                .contains("CTX_WORLD_MODEL_RETRIEVAL_RECORDED_RANKED_LOCAL")
        );
        let truncated = runtime
            .submit_json(
                r#"{"query":"forge runtime world model","query_entities":["forge"],"token_budget":40}"#,
                &memories,
                "POSTGRES_PGVECTOR_CONFIGURED",
                "VALKEY_DOCKER_LOCAL",
                "LIVE-LOCAL-CTX-WEBSOCKET-RELAY",
            )
            .expect("truncated retrieval should record");
        assert!(
            truncated
                .body
                .contains("CTX_WORLD_MODEL_RETRIEVAL_RECORDED_BUDGET_TRUNCATED")
        );
        let vector_blocked = runtime
            .submit_json(
                r#"{"query":"forge runtime world model","vector_hits_requested":true}"#,
                &memories,
                "POSTGRES_PGVECTOR_CONFIGURED",
                "VALKEY_DOCKER_LOCAL",
                "LIVE-LOCAL-CTX-WEBSOCKET-RELAY",
            )
            .expect("vector provider boundary should record");
        assert!(
            vector_blocked
                .body
                .contains("CTX_WORLD_MODEL_RETRIEVAL_RECORDED_VECTOR_PROVIDER_BLOCKED")
        );
        let rejected = runtime
            .submit_json(
                r#"{"query":"unsafe","provider_calls_requested":true,"embedding_values_recorded":true}"#,
                &memories,
                "POSTGRES_PGVECTOR_CONFIGURED",
                "VALKEY_DOCKER_LOCAL",
                "LIVE-LOCAL-CTX-WEBSOCKET-RELAY",
            )
            .expect("unsafe authority should be rejected");
        assert!(
            rejected
                .body
                .contains("CTX_WORLD_MODEL_RETRIEVAL_REJECTED_SECURITY_BOUNDARY")
        );
        let projection = runtime.projection_json();
        assert!(projection.contains(r#""retrieval_count":4"#));
        assert!(projection.contains(r#""provider_calls_allowed":false"#));
        assert!(projection.contains(r#""production_writes_allowed":false"#));
    }
}
