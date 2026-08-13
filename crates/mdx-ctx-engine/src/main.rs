use mdx_core::{
    LocalContextMemoryInput, LocalContextPacket, LocalContextRequest, LocalContextSourcePreference,
    MdxKernel, json_string_literal,
};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Stdio};

mod focus;
mod memory_admission;
mod provider;
mod session;
mod topk;
mod world_model_retrieval;
use focus::CtxFocusRuntime;
use memory_admission::{CtxMemoryAdmissionResult, CtxMemoryAdmissionRuntime};
use provider::CtxProviderRuntime;
use session::{CtxSessionAction, CtxSessionRuntime};
use topk::top_scored_memories;
use world_model_retrieval::{
    CtxWorldModelMemoryCandidate, CtxWorldModelRetrievalResult, CtxWorldModelRetrievalRuntime,
};

const JSON_CONTENT_TYPE: &str = "application/json; charset=utf-8";
const TEXT_CONTENT_TYPE: &str = "text/plain; charset=utf-8";
const LOCAL_CORS_HEADERS: &str = "Access-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Accept, Content-Type";
const CTX_VECTOR_TABLE: &str = "ctx_memory_vectors";
const CTX_VECTOR_INDEX: &str = "ctx_memory_vectors_embedding_hnsw_idx";
const CTX_VECTOR_DIMENSIONS: usize = 1536;
const CTX_VECTOR_DISTANCE_METRIC: &str = "cosine";
const CTX_VECTOR_DEFAULT_TOP_K: usize = 5;

fn main() {
    if let Err(error) = run() {
        eprintln!("mdx-ctx-engine error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = std::env::args().collect::<Vec<_>>();
    match args.get(1).map(String::as_str) {
        Some("local-context-packet") => {
            println!("{}", render_local_context_packet_json()?);
            Ok(())
        }
        Some("serve") => {
            let addr = args.get(2).map(String::as_str).unwrap_or("127.0.0.1:9020");
            serve(addr)
        }
        _ => Err("usage: mdx-ctx-engine local-context-packet | serve 127.0.0.1:9020".to_string()),
    }
}

fn local_context_packet() -> Result<LocalContextPacket, String> {
    let mut kernel = MdxKernel::boot_local();
    kernel.run_evals_runner_agent()?;
    kernel
        .assemble_local_context(LocalContextRequest {
            tenant_id: "tenant_local",
            actor_id: "local_user",
            query: "developer onboarding memory proof world model",
            token_budget: 240,
            source_preferences: &[],
            query_entities: &["developer", "world_model"],
            local_memories: &[],
        })
        .map_err(|error| error.message())
}

fn render_local_context_packet_json() -> Result<String, String> {
    let packet = local_context_packet()?;
    Ok(render_packet_json(&packet))
}

fn serve(addr: &str) -> Result<(), String> {
    let listener = TcpListener::bind(addr).map_err(|error| format!("bind {addr}: {error}"))?;
    println!("mdx ctx engine listening on http://{addr}");
    let mut state = CtxRuntimeState::default();
    for stream in listener.incoming() {
        let stream = stream.map_err(|error| format!("incoming connection: {error}"))?;
        handle_connection(stream, &mut state)?;
    }
    Ok(())
}

fn handle_connection(mut stream: TcpStream, state: &mut CtxRuntimeState) -> Result<(), String> {
    let mut buffer = [0_u8; 4096];
    let bytes = stream
        .read(&mut buffer)
        .map_err(|error| format!("read request: {error}"))?;
    let request = String::from_utf8_lossy(&buffer[..bytes]);
    let method = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().next())
        .unwrap_or("GET");
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");
    let body = request
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .unwrap_or("");
    let response = route(method, path, body, state)?;
    let wire_response = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\n{}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        response.status,
        response.content_type,
        LOCAL_CORS_HEADERS,
        response.body.len(),
        response.body
    );
    stream
        .write_all(wire_response.as_bytes())
        .map_err(|error| format!("write response: {error}"))
}

fn route(
    method: &str,
    path: &str,
    body: &str,
    state: &mut CtxRuntimeState,
) -> Result<Response, String> {
    if method.eq_ignore_ascii_case("OPTIONS") {
        return Ok(Response::text("204 No Content", String::new()));
    }
    if method.eq_ignore_ascii_case("POST") && matches!(path, "/v1/assemble" | "/ctx/assemble") {
        return Ok(Response::json(
            "200 OK",
            state.render_assembled_context_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST") && matches!(path, "/v1/ctx/memories" | "/ctx/memories") {
        return Ok(Response::json("200 OK", state.remember_json(body)?));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(path, "/v1/ctx/memory-admissions" | "/ctx/memory-admissions")
    {
        return Ok(Response::json(
            "200 OK",
            state.render_memory_admission_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST") && matches!(path, "/v1/ctx/recall" | "/ctx/recall") {
        return Ok(Response::json("200 OK", state.recall_json(body)?));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(path, "/v1/ctx/memory-feedback" | "/ctx/memory-feedback")
    {
        return Ok(Response::json("200 OK", state.memory_feedback_json(body)?));
    }
    if method.eq_ignore_ascii_case("POST") && matches!(path, "/v1/ctx/forget" | "/ctx/forget") {
        return Ok(Response::json("200 OK", state.forget_json(body)?));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(path, "/v1/ctx/focus/intake" | "/ctx/focus/intake")
    {
        return Ok(Response::json(
            "200 OK",
            state.focus_runtime.intake_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST") && matches!(path, "/v1/ctx/sessions" | "/ctx/sessions") {
        let action = state.session_runtime.create_session_json(body)?;
        return Ok(Response::json(
            "200 OK",
            state.render_session_action_json(action)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST") {
        if let Some(session_id) = session_state_path(path) {
            let action = state.session_runtime.set_state_json(session_id, body)?;
            return Ok(Response::json(
                "200 OK",
                state.render_session_action_json(action)?,
            ));
        }
        if let Some(session_id) = session_close_path(path) {
            let action = state.session_runtime.close_session_json(session_id, body)?;
            return Ok(Response::json(
                "200 OK",
                state.render_session_action_json(action)?,
            ));
        }
    }
    if method.eq_ignore_ascii_case("POST") && matches!(path, "/v1/ctx/handoffs" | "/ctx/handoffs") {
        let action = state.session_runtime.create_handoff_json(body)?;
        return Ok(Response::json(
            "200 OK",
            state.render_session_action_json(action)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(path, "/v1/ctx/background-jobs" | "/ctx/background-jobs")
    {
        let action = state.session_runtime.record_job_json(body)?;
        return Ok(Response::json(
            "200 OK",
            state.render_session_action_json(action)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(
            path,
            "/v1/ctx/provider-turn-on-preflights" | "/ctx/provider-turn-on-preflights"
        )
    {
        return Ok(Response::json(
            "200 OK",
            state.provider_runtime.preflight_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(
            path,
            "/v1/ctx/provider-observations" | "/ctx/provider-observations"
        )
    {
        return Ok(Response::json(
            "200 OK",
            state.provider_runtime.observation_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(
            path,
            "/v1/ctx/vector-index-proofs" | "/ctx/vector-index-proofs"
        )
    {
        return Ok(Response::json(
            "200 OK",
            state.vector_index_proof_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(
            path,
            "/v1/ctx/world-model-retrievals" | "/ctx/world-model-retrievals"
        )
    {
        return Ok(Response::json(
            "200 OK",
            state.render_world_model_retrieval_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(
            path,
            "/v1/ctx/consolidation/proposals" | "/ctx/consolidation/proposals"
        )
    {
        return Ok(Response::json("200 OK", state.propose_consolidation_json()));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(
            path,
            "/v1/ctx/consolidation/approve" | "/ctx/consolidation/approve"
        )
    {
        return Ok(Response::json(
            "200 OK",
            state.review_consolidation_json(body, "approved")?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(
            path,
            "/v1/ctx/consolidation/dismiss" | "/ctx/consolidation/dismiss"
        )
    {
        return Ok(Response::json(
            "200 OK",
            state.review_consolidation_json(body, "dismissed")?,
        ));
    }
    if !method.eq_ignore_ascii_case("GET") {
        return Ok(Response::text(
            "405 Method Not Allowed",
            "method not allowed\n".to_string(),
        ));
    }
    if let Some(session_id) = session_state_path(path) {
        return Ok(Response::json(
            "200 OK",
            state.session_runtime.state_json(Some(session_id)),
        ));
    }
    match path {
        "/health" | "/ctx/health" => Ok(Response::json("200 OK", state.render_health_json())),
        "/ctx/operational-health.json" | "/v1/ctx/operational-health.json" => {
            Ok(Response::json("200 OK", state.operational_health_json()?))
        }
        "/ctx/local-context-packet.json" | "/ctx/memory-tiers.json" => Ok(Response::json(
            "200 OK",
            state.render_local_context_packet_json()?,
        )),
        "/ctx/memories.json" | "/v1/ctx/memories.json" => {
            Ok(Response::json("200 OK", state.memories_json()))
        }
        "/ctx/memory-learning.json" | "/v1/ctx/memory-learning.json" => {
            Ok(Response::json("200 OK", state.memory_learning_json()))
        }
        "/ctx/memory-admissions.json" | "/v1/ctx/memory-admissions.json" => Ok(Response::json(
            "200 OK",
            state.memory_admission_runtime.admissions_json(),
        )),
        "/ctx/vector-index.json" | "/v1/ctx/vector-index.json" => Ok(Response::json(
            "200 OK",
            state.vector_index_projection_json(),
        )),
        "/ctx/world-model-retrievals.json" | "/v1/ctx/world-model-retrievals.json" => {
            Ok(Response::json(
                "200 OK",
                state.world_model_retrieval_runtime.projection_json(),
            ))
        }
        "/ctx/sessions.json" | "/v1/ctx/sessions.json" => Ok(Response::json(
            "200 OK",
            state.session_runtime.sessions_json(),
        )),
        "/ctx/session-state.json" | "/v1/ctx/session-state.json" => Ok(Response::json(
            "200 OK",
            state.session_runtime.state_json(None),
        )),
        "/ctx/handoffs.json" | "/v1/ctx/handoffs.json" => Ok(Response::json(
            "200 OK",
            state.session_runtime.handoffs_json(),
        )),
        "/ctx/background-jobs.json" | "/v1/ctx/background-jobs.json" => {
            Ok(Response::json("200 OK", state.session_runtime.jobs_json()))
        }
        "/ctx/provider-slots.json" | "/v1/ctx/provider-slots.json" => Ok(Response::json(
            "200 OK",
            state.provider_runtime.slots_json(),
        )),
        "/ctx/provider-turn-on-preflights.json" | "/v1/ctx/provider-turn-on-preflights.json" => Ok(
            Response::json("200 OK", state.provider_runtime.preflights_json()),
        ),
        "/ctx/provider-observations.json" | "/v1/ctx/provider-observations.json" => Ok(
            Response::json("200 OK", state.provider_runtime.observations_json()),
        ),
        "/ctx/consolidation/proposals.json" | "/v1/ctx/consolidation/proposals.json" => {
            Ok(Response::json("200 OK", state.proposals_json()))
        }
        "/ctx/events.json" | "/v1/ctx/events.json" => {
            Ok(Response::json("200 OK", state.events_json()))
        }
        "/ctx/focus/runtime-packet.json" | "/v1/ctx/focus/runtime-packet.json" => {
            Ok(Response::json("200 OK", state.focus_runtime.packet_json()))
        }
        "/ctx/focus/watchlist" | "/v1/ctx/focus/watchlist" => Ok(Response::json(
            "200 OK",
            state.focus_runtime.watchlist_json(),
        )),
        "/ctx/focus/graph" | "/v1/ctx/focus/graph" => {
            Ok(Response::json("200 OK", state.focus_runtime.graph_json()))
        }
        "/ctx/focus/compiled" | "/v1/ctx/focus/compiled" => Ok(Response::json(
            "200 OK",
            state.focus_runtime.compiled_json(),
        )),
        _ => Ok(Response::text("404 Not Found", "not found\n".to_string())),
    }
}

fn session_state_path(path: &str) -> Option<&str> {
    path.strip_prefix("/v1/ctx/sessions/")
        .or_else(|| path.strip_prefix("/ctx/sessions/"))
        .and_then(|tail| tail.strip_suffix("/state"))
        .filter(|session_id| !session_id.is_empty())
}

fn session_close_path(path: &str) -> Option<&str> {
    path.strip_prefix("/v1/ctx/sessions/")
        .or_else(|| path.strip_prefix("/ctx/sessions/"))
        .and_then(|tail| tail.strip_suffix("/close"))
        .filter(|session_id| !session_id.is_empty())
}

struct OwnedAssembleRequest {
    tenant_id: String,
    actor_id: String,
    query: String,
    token_budget: usize,
    preferences: Vec<OwnedPreference>,
    query_entities: Vec<String>,
    local_memories: Vec<OwnedLocalMemory>,
}

struct OwnedPreference {
    source_type: String,
    enabled: bool,
    token_budget_override: Option<usize>,
}

#[derive(Clone)]
struct OwnedLocalMemory {
    memory_id: String,
    tier: String,
    content: String,
    source_receipt_id: String,
    importance: usize,
    access_count: usize,
    usefulness_score_percent: usize,
    edge_weight_percent: usize,
    last_recalled_query: String,
}

struct CtxRuntimeState {
    memories: Vec<OwnedLocalMemory>,
    proposals: Vec<CtxConsolidationProposal>,
    events: Vec<CtxRuntimeEvent>,
    durable_store: CtxDurableMemoryStore,
    vector_store: CtxVectorStore,
    working_cache: CtxWorkingCache,
    event_relay: CtxEventRelay,
    vector_index_proofs: Vec<CtxVectorIndexProof>,
    memory_admission_runtime: CtxMemoryAdmissionRuntime,
    focus_runtime: CtxFocusRuntime,
    session_runtime: CtxSessionRuntime,
    provider_runtime: CtxProviderRuntime,
    world_model_retrieval_runtime: CtxWorldModelRetrievalRuntime,
    next_memory: usize,
    next_proposal: usize,
    next_event: usize,
    next_vector_proof: usize,
}

impl Default for CtxRuntimeState {
    fn default() -> Self {
        Self {
            memories: Vec::new(),
            proposals: Vec::new(),
            events: Vec::new(),
            durable_store: CtxDurableMemoryStore::from_env(),
            vector_store: CtxVectorStore::from_env(),
            working_cache: CtxWorkingCache::from_env(),
            event_relay: CtxEventRelay::from_env(),
            vector_index_proofs: Vec::new(),
            memory_admission_runtime: CtxMemoryAdmissionRuntime::new(),
            focus_runtime: CtxFocusRuntime::default(),
            session_runtime: CtxSessionRuntime::default(),
            provider_runtime: CtxProviderRuntime::default(),
            world_model_retrieval_runtime: CtxWorldModelRetrievalRuntime::new(),
            next_memory: 0,
            next_proposal: 0,
            next_event: 0,
            next_vector_proof: 0,
        }
    }
}

#[derive(Clone)]
struct CtxConsolidationProposal {
    proposal_id: String,
    source_memory_id: String,
    proposed_tier: String,
    proposed_content: String,
    confidence_percent: usize,
    status: String,
    reviewer_notes: String,
}

struct CtxRuntimeEvent {
    sequence: usize,
    event_id: String,
    event_type: String,
    subject_id: String,
    tenant_id: String,
    topic: String,
    relay_status: String,
}

struct CtxDurableMemoryStore {
    database_url: Option<String>,
}

struct CtxDurableWriteReport {
    status: String,
    memory_count: Option<usize>,
    receipt_id: String,
    table: &'static str,
}

struct CtxVectorStore {
    database_url: Option<String>,
}

struct CtxVectorReport {
    status: String,
    vector_count: Option<usize>,
    table: &'static str,
    dimensions: usize,
}

#[derive(Clone)]
struct CtxVectorSearchHit {
    memory_id: String,
    tier: String,
    score_percent: usize,
}

#[derive(Clone)]
struct CtxVectorIndexProof {
    proof_id: String,
    tenant_id: String,
    query: String,
    top_k: usize,
    requested_dimensions: usize,
    terminal_state: String,
    rejection_reason: String,
    retrieval_result_count: usize,
    query_embedding_hash: String,
    hits: Vec<CtxVectorProofHit>,
}

#[derive(Clone)]
struct CtxVectorProofHit {
    memory_id: String,
    tier: String,
    score_percent: usize,
    memory_embedding_hash: String,
}

struct CtxWorkingCache {
    container: Option<String>,
    ttl_seconds: usize,
}

struct CtxWorkingCacheReport {
    status: String,
    key: String,
    ttl_seconds: usize,
}

struct CtxEventRelay {
    container: Option<String>,
}

impl CtxDurableMemoryStore {
    fn from_env() -> Self {
        Self {
            database_url: std::env::var("DATABASE_URL")
                .ok()
                .filter(|value| !value.trim().is_empty()),
        }
    }

    fn status(&self) -> &'static str {
        if self.database_url.is_some() {
            "POSTGRES_DURABLE_MEMORY_CONFIGURED"
        } else {
            "LOCAL_PROCESS_MEMORY_ONLY"
        }
    }

    fn remember_memory(
        &self,
        memory: &OwnedLocalMemory,
        tenant_id: &str,
        actor_id: &str,
        receipt_kind: &str,
    ) -> CtxDurableWriteReport {
        let Some(database_url) = self.database_url.as_deref() else {
            return self.noop_report(receipt_kind);
        };
        let receipt_id = memory.source_receipt_id.clone();
        let sql = durable_memory_upsert_sql(memory, tenant_id, actor_id, receipt_kind);
        self.run_count_sql(
            database_url,
            &sql,
            receipt_id,
            "POSTGRES_DURABLE_MEMORY_RECORDED",
        )
    }

    fn forget_memory(
        &self,
        memory_id: &str,
        tenant_id: &str,
        actor_id: &str,
    ) -> CtxDurableWriteReport {
        let receipt_id = format!("ctx_memory_forget_receipt_{memory_id}");
        let Some(database_url) = self.database_url.as_deref() else {
            return self.noop_report("ctx.memory.forgotten");
        };
        let sql = durable_memory_forget_sql(memory_id, tenant_id, actor_id, &receipt_id);
        self.run_count_sql(
            database_url,
            &sql,
            receipt_id,
            "POSTGRES_DURABLE_MEMORY_FORGOTTEN",
        )
    }

    fn noop_report(&self, receipt_kind: &str) -> CtxDurableWriteReport {
        CtxDurableWriteReport {
            status: self.status().to_string(),
            memory_count: None,
            receipt_id: receipt_kind.to_string(),
            table: "memory_records",
        }
    }

    fn run_count_sql(
        &self,
        database_url: &str,
        sql: &str,
        receipt_id: String,
        success_status: &str,
    ) -> CtxDurableWriteReport {
        match run_psql(database_url, sql) {
            Ok(memory_count) => CtxDurableWriteReport {
                status: success_status.to_string(),
                memory_count,
                receipt_id,
                table: "memory_records",
            },
            Err(error) => CtxDurableWriteReport {
                status: format!("POSTGRES_DURABLE_MEMORY_ERROR:{error}"),
                memory_count: None,
                receipt_id,
                table: "memory_records",
            },
        }
    }
}

impl CtxDurableWriteReport {
    fn render_json(&self) -> String {
        format!(
            r#"{{"status":{},"table":{},"receipt_id":{},"memory_count":{},"provider_calls_allowed":false,"production_writes_allowed":false}}"#,
            json_string_literal(&self.status),
            json_string_literal(self.table),
            json_string_literal(&self.receipt_id),
            optional_usize(self.memory_count)
        )
    }
}

impl CtxVectorStore {
    fn from_env() -> Self {
        Self {
            database_url: std::env::var("DATABASE_URL")
                .ok()
                .filter(|value| !value.trim().is_empty()),
        }
    }

    fn status(&self) -> &'static str {
        if self.database_url.is_some() {
            "POSTGRES_PGVECTOR_CONFIGURED"
        } else {
            "PENDING-DATABASE-URL"
        }
    }

    fn upsert_memory(&self, memory: &OwnedLocalMemory, tenant_id: &str) -> CtxVectorReport {
        let Some(database_url) = self.database_url.as_deref() else {
            return self.noop_report("PENDING-DATABASE-URL");
        };
        let sql = vector_upsert_sql(memory, tenant_id);
        self.run_count_sql(database_url, &sql, "LIVE-LOCAL-PGVECTOR-INDEXED")
    }

    fn delete_memory(&self, memory_id: &str, tenant_id: &str) -> CtxVectorReport {
        let Some(database_url) = self.database_url.as_deref() else {
            return self.noop_report("PENDING-DATABASE-URL");
        };
        let sql = vector_delete_sql(memory_id, tenant_id);
        self.run_count_sql(database_url, &sql, "LIVE-LOCAL-PGVECTOR-DELETED")
    }

    fn search(
        &self,
        query: &str,
        tenant_id: &str,
        limit: usize,
    ) -> (CtxVectorReport, Vec<CtxVectorSearchHit>) {
        let Some(database_url) = self.database_url.as_deref() else {
            return (self.noop_report("PENDING-DATABASE-URL"), Vec::new());
        };
        let sql = vector_search_sql(query, tenant_id, limit);
        match run_psql_rows(database_url, &sql) {
            Ok(rows) => {
                let hits = rows
                    .into_iter()
                    .filter_map(|row| {
                        let parts = row.split('\t').collect::<Vec<_>>();
                        if parts.len() != 3 {
                            return None;
                        }
                        let score_percent = parts[2].parse::<usize>().ok()?;
                        Some(CtxVectorSearchHit {
                            memory_id: parts[0].to_string(),
                            tier: parts[1].to_string(),
                            score_percent,
                        })
                    })
                    .collect::<Vec<_>>();
                (
                    CtxVectorReport {
                        status: "LIVE-LOCAL-PGVECTOR-SEARCHED".to_string(),
                        vector_count: Some(hits.len()),
                        table: CTX_VECTOR_TABLE,
                        dimensions: CTX_VECTOR_DIMENSIONS,
                    },
                    hits,
                )
            }
            Err(error) => (
                self.noop_report(&format!("POSTGRES_PGVECTOR_ERROR:{error}")),
                Vec::new(),
            ),
        }
    }

    fn noop_report(&self, status: &str) -> CtxVectorReport {
        CtxVectorReport {
            status: status.to_string(),
            vector_count: None,
            table: CTX_VECTOR_TABLE,
            dimensions: CTX_VECTOR_DIMENSIONS,
        }
    }

    fn run_count_sql(
        &self,
        database_url: &str,
        sql: &str,
        success_status: &str,
    ) -> CtxVectorReport {
        match run_psql(database_url, sql) {
            Ok(vector_count) => CtxVectorReport {
                status: success_status.to_string(),
                vector_count,
                table: CTX_VECTOR_TABLE,
                dimensions: CTX_VECTOR_DIMENSIONS,
            },
            Err(error) => self.noop_report(&format!("POSTGRES_PGVECTOR_ERROR:{error}")),
        }
    }
}

impl CtxVectorReport {
    fn render_json(&self) -> String {
        format!(
            r#"{{"status":{},"table":{},"dimensions":{},"vector_count":{},"provider_calls_allowed":false,"production_writes_allowed":false}}"#,
            json_string_literal(&self.status),
            json_string_literal(self.table),
            self.dimensions,
            optional_usize(self.vector_count)
        )
    }
}

impl CtxVectorSearchHit {
    fn render_json(&self) -> String {
        format!(
            r#"{{"memory_id":{},"tier":{},"score_percent":{}}}"#,
            json_string_literal(&self.memory_id),
            json_string_literal(&self.tier),
            self.score_percent
        )
    }
}

impl CtxWorkingCache {
    fn from_env() -> Self {
        Self {
            container: std::env::var("MDX_VALKEY_CONTAINER")
                .ok()
                .filter(|value| !value.trim().is_empty()),
            ttl_seconds: std::env::var("MDX_CTX_WORKING_CACHE_TTL_SECONDS")
                .ok()
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(86_400),
        }
    }

    fn status(&self) -> &'static str {
        "VALKEY_DOCKER_LOCAL"
    }

    fn write_memory(&self, memory: &OwnedLocalMemory, tenant_id: &str) -> CtxWorkingCacheReport {
        let key = self.key(tenant_id, &memory.memory_id);
        let value = render_memory_json(memory);
        match run_valkey_command(
            self.container.as_deref(),
            &["SETEX", &key, &self.ttl_seconds.to_string(), &value],
        ) {
            Ok(_) => CtxWorkingCacheReport {
                status: "LIVE-LOCAL-VALKEY-WORKING-CACHE-WRITTEN".to_string(),
                key,
                ttl_seconds: self.ttl_seconds,
            },
            Err(error) => CtxWorkingCacheReport {
                status: format!("VALKEY_WORKING_CACHE_ERROR:{error}"),
                key,
                ttl_seconds: self.ttl_seconds,
            },
        }
    }

    fn delete_memory(&self, memory_id: &str, tenant_id: &str) -> CtxWorkingCacheReport {
        let key = self.key(tenant_id, memory_id);
        match run_valkey_command(self.container.as_deref(), &["DEL", &key]) {
            Ok(_) => CtxWorkingCacheReport {
                status: "LIVE-LOCAL-VALKEY-WORKING-CACHE-DELETED".to_string(),
                key,
                ttl_seconds: self.ttl_seconds,
            },
            Err(error) => CtxWorkingCacheReport {
                status: format!("VALKEY_WORKING_CACHE_ERROR:{error}"),
                key,
                ttl_seconds: self.ttl_seconds,
            },
        }
    }

    fn key(&self, tenant_id: &str, memory_id: &str) -> String {
        format!("ctx:working:{tenant_id}:local_session:{memory_id}")
    }
}

impl CtxWorkingCacheReport {
    fn render_json(&self) -> String {
        format!(
            r#"{{"status":{},"driver":"valkey","key":{},"ttl_seconds":{},"provider_calls_allowed":false,"production_writes_allowed":false}}"#,
            json_string_literal(&self.status),
            json_string_literal(&self.key),
            self.ttl_seconds
        )
    }
}

impl CtxEventRelay {
    fn from_env() -> Self {
        Self {
            container: std::env::var("MDX_VALKEY_CONTAINER")
                .ok()
                .filter(|value| !value.trim().is_empty()),
        }
    }

    fn status(&self) -> &'static str {
        "VALKEY_DOCKER_LOCAL"
    }

    fn publish(&self, event: &CtxRuntimeEvent) -> String {
        match run_valkey_command(
            self.container.as_deref(),
            &["PUBLISH", &event.topic, &render_event_envelope_json(event)],
        ) {
            Ok(_) => "LIVE-LOCAL-CTX-WEBSOCKET-RELAY-PUBLISHED".to_string(),
            Err(error) => format!("CTX_WEBSOCKET_RELAY_ERROR:{error}"),
        }
    }
}

impl CtxRuntimeState {
    fn render_health_json(&self) -> String {
        format!(
            r#"{{"status":"ok","runtime":"mdx-ctx-engine","assembly_status":"LIVE-LOCAL-CONTEXT-ASSEMBLY","memory_runtime_status":"LIVE-LOCAL-CTX-MEMORY-RUNTIME","memory_learning_status":"LIVE-LOCAL-CTX-MEMORY-LEARNING-FLOOR","memory_admission_status":{},"world_model_retrieval_status":{},"operational_health_status":"LIVE-LOCAL-CTX-OPERATIONAL-HEALTH","focus_runtime_status":{},"session_runtime_status":{},"handoff_runtime_status":"LIVE-LOCAL-CTX-HANDOFF-RUNTIME","background_job_status":"LIVE-LOCAL-CTX-BACKGROUND-JOB-RUNTIME","provider_turn_on_status":{},"durable_memory_status":{},"vector_status":{},"working_cache_status":{},"event_stream_status":"LIVE-LOCAL-CTX-EVENT-LOG","event_relay_status":{},"provider_calls_allowed":false,"production_writes_allowed":false}}"#,
            json_string_literal(self.memory_admission_runtime.status()),
            json_string_literal(self.world_model_retrieval_runtime.status()),
            json_string_literal(self.focus_runtime.status()),
            json_string_literal(self.session_runtime.status()),
            json_string_literal(self.provider_runtime.status()),
            json_string_literal(self.durable_store.status()),
            json_string_literal(self.vector_store.status()),
            json_string_literal(self.working_cache.status()),
            json_string_literal(self.event_relay.status())
        )
    }

    fn operational_health_json(&self) -> Result<String, String> {
        let admissions = parse_rendered_json(
            &self.memory_admission_runtime.admissions_json(),
            "admissions",
        )?;
        let sessions = parse_rendered_json(&self.session_runtime.sessions_json(), "sessions")?;
        let state = parse_rendered_json(&self.session_runtime.state_json(None), "session state")?;
        let handoffs = parse_rendered_json(&self.session_runtime.handoffs_json(), "handoffs")?;
        let jobs = parse_rendered_json(&self.session_runtime.jobs_json(), "background jobs")?;
        let provider_slots =
            parse_rendered_json(&self.provider_runtime.slots_json(), "provider slots")?;
        let world_model_retrievals = parse_rendered_json(
            &self.world_model_retrieval_runtime.projection_json(),
            "world model retrievals",
        )?;
        let pending_proposal_count = self
            .proposals
            .iter()
            .filter(|proposal| proposal.status == "pending")
            .count();
        let approved_proposal_count = self
            .proposals
            .iter()
            .filter(|proposal| proposal.status == "approved")
            .count();
        Ok(format!(
            r#"{{"name":"mdx-ctx-operational-health","status":"LIVE-LOCAL-CTX-OPERATIONAL-HEALTH","runtime":"mdx-ctx-engine","route":"/ctx/operational-health.json","health_route":"/ctx/health","assembly_status":"LIVE-LOCAL-CONTEXT-ASSEMBLY","memory_runtime_status":"LIVE-LOCAL-CTX-MEMORY-RUNTIME","memory_learning_status":"LIVE-LOCAL-CTX-MEMORY-LEARNING-FLOOR","memory_admission_status":{},"world_model_retrieval_status":{},"focus_runtime_status":{},"session_runtime_status":{},"handoff_runtime_status":"LIVE-LOCAL-CTX-HANDOFF-RUNTIME","background_job_status":"LIVE-LOCAL-CTX-BACKGROUND-JOB-RUNTIME","provider_turn_on_status":{},"durable_memory_status":{},"vector_status":{},"working_cache_status":{},"event_stream_status":"LIVE-LOCAL-CTX-EVENT-LOG","websocket_status":{},"memory_count":{},"memory_tier_counts":{},"learned_memory_count":{},"total_memory_access_count":{},"max_edge_weight_percent":{},"admission_count":{},"accepted_admission_count":{},"queued_admission_count":{},"rejected_admission_count":{},"world_model_retrieval_count":{},"world_model_retrieval_rejected_count":{},"world_model_vector_provider_blocked_count":{},"world_model_budget_truncated_count":{},"proposal_count":{},"pending_proposal_count":{},"approved_proposal_count":{},"session_count":{},"session_state_count":{},"handoff_count":{},"background_job_count":{},"event_count":{},"provider_slot_count":{},"observed_provider_count":{},"provider_connected_local_allowed":{},"hot_path_budget_ms":20,"memory_write_performed":false,"vector_write_performed":false,"cache_write_performed":false,"provider_calls_allowed":false,"production_writes_allowed":false}}"#,
            json_string_literal(self.memory_admission_runtime.status()),
            json_string_literal(self.world_model_retrieval_runtime.status()),
            json_string_literal(self.focus_runtime.status()),
            json_string_literal(self.session_runtime.status()),
            json_string_literal(self.provider_runtime.status()),
            json_string_literal(self.durable_store.status()),
            json_string_literal(self.vector_store.status()),
            json_string_literal(self.working_cache.status()),
            json_string_literal(self.event_stream_status()),
            self.memories.len(),
            self.memory_tier_counts_json(),
            self.learned_memory_count(),
            self.total_memory_access_count(),
            self.max_edge_weight_percent(),
            json_usize(&admissions, "admission_count"),
            json_usize(&admissions, "accepted_count"),
            json_usize(&admissions, "queued_count"),
            json_usize(&admissions, "rejected_count"),
            json_usize(&world_model_retrievals, "retrieval_count"),
            json_usize(&world_model_retrievals, "rejected_count"),
            json_usize(&world_model_retrievals, "vector_provider_blocked_count"),
            json_usize(&world_model_retrievals, "budget_truncated_count"),
            self.proposals.len(),
            pending_proposal_count,
            approved_proposal_count,
            json_usize(&sessions, "session_count"),
            json_array_len(&state, "state"),
            json_usize(&handoffs, "handoff_count"),
            json_usize(&jobs, "job_count"),
            self.events.len(),
            json_usize(&provider_slots, "provider_slot_count"),
            json_usize(&provider_slots, "observed_provider_count"),
            json_bool(&provider_slots, "provider_connected_local_allowed")
        ))
    }

    fn memory_tier_counts_json(&self) -> String {
        let tiers = ["working", "episodic", "semantic", "procedural"]
            .iter()
            .map(|tier| {
                let count = self
                    .memories
                    .iter()
                    .filter(|memory| memory.tier == *tier)
                    .count();
                format!(
                    r#"{{"tier":{},"memory_count":{}}}"#,
                    json_string_literal(tier),
                    count
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        format!("[{tiers}]")
    }

    fn render_local_context_packet_json(&self) -> Result<String, String> {
        let packet =
            self.local_context_packet("developer onboarding memory proof world model", 240)?;
        self.render_packet_json_with_runtime(&packet)
    }

    fn render_assembled_context_json(&self, body: &str) -> Result<String, String> {
        let request = parse_assemble_request(body)?;
        let packet = self.assemble_from_request(&request)?;
        self.render_packet_json_with_runtime(&packet)
    }

    fn render_packet_json_with_runtime(
        &self,
        packet: &LocalContextPacket,
    ) -> Result<String, String> {
        let mut value: Value = serde_json::from_str(&render_packet_json(packet))
            .map_err(|error| format!("rendered ctx packet json invalid: {error}"))?;
        value["runtime_vector_report"] = json!({
            "status": self.vector_store.status(),
            "table": CTX_VECTOR_TABLE,
            "index": CTX_VECTOR_INDEX,
            "dimensions": CTX_VECTOR_DIMENSIONS,
            "distance_metric": CTX_VECTOR_DISTANCE_METRIC,
            "top_k_default": CTX_VECTOR_DEFAULT_TOP_K,
            "provider_calls_allowed": false,
            "production_writes_allowed": false
        });
        value["runtime_working_cache_report"] = json!({
            "status": self.working_cache.status(),
            "driver": "valkey",
            "namespace": "ctx:working:{tenant_id}:local_session:{memory_id}",
            "ttl_seconds": self.working_cache.ttl_seconds,
            "provider_calls_allowed": false,
            "production_writes_allowed": false
        });
        value["runtime_event_stream_report"] = json!({
            "status": self.event_stream_status(),
            "driver": "mdx-message-relay",
            "stream": "ctx_event_stream",
            "topic_pattern": "mdx:ctx:{tenant_id}:events",
            "hot_path": "ctx_event_log_valkey_publish_websocket_fanout",
            "websocket_allowed": self.events.iter().any(|event| event.relay_status == "LIVE-LOCAL-CTX-WEBSOCKET-RELAY-PUBLISHED"),
            "provider_calls_allowed": false,
            "production_writes_allowed": false
        });
        value["runtime_provider_turn_on_report"] =
            serde_json::from_str::<Value>(&self.provider_runtime.slots_json())
                .map_err(|error| format!("rendered ctx provider slots json invalid: {error}"))?;
        Ok(value.to_string())
    }

    fn render_session_action_json(&mut self, action: CtxSessionAction) -> Result<String, String> {
        self.push_event(action.event_type, &action.subject_id, &action.tenant_id);
        Ok(action.body)
    }

    fn render_memory_admission_json(&mut self, body: &str) -> Result<String, String> {
        let result = self.memory_admission_runtime.submit_json(body)?;
        Ok(self.render_memory_admission_result(result))
    }

    fn render_memory_admission_result(&mut self, result: CtxMemoryAdmissionResult) -> String {
        for event in result.events {
            self.push_event(&event.event_type, &event.subject_id, &event.tenant_id);
        }
        result.body
    }

    fn render_world_model_retrieval_json(&mut self, body: &str) -> Result<String, String> {
        let memory_candidates = self.world_model_memory_candidates();
        let vector_status = self.vector_store.status().to_string();
        let working_cache_status = self.working_cache.status().to_string();
        let event_stream_status = self.event_stream_status().to_string();
        let result = self.world_model_retrieval_runtime.submit_json(
            body,
            &memory_candidates,
            &vector_status,
            &working_cache_status,
            &event_stream_status,
        )?;
        Ok(self.render_world_model_retrieval_result(result))
    }

    fn render_world_model_retrieval_result(
        &mut self,
        result: CtxWorldModelRetrievalResult,
    ) -> String {
        for event in result.events {
            self.push_event(&event.event_type, &event.subject_id, &event.tenant_id);
        }
        result.body
    }

    fn remember_json(&mut self, body: &str) -> Result<String, String> {
        let value = parse_json(body)?;
        let tier = normalize_tier(
            value
                .get("tier")
                .and_then(Value::as_str)
                .unwrap_or("episodic"),
        );
        let content = value
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(|| "ctx remember missing content".to_string())?
            .to_string();
        self.next_memory += 1;
        let memory = OwnedLocalMemory {
            memory_id: value
                .get("memory_id")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("ctx_memory_{:06}", self.next_memory)),
            tier,
            content,
            source_receipt_id: value
                .get("source_receipt_id")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("ctx_memory_receipt_{:06}", self.next_memory)),
            importance: value
                .get("importance")
                .and_then(Value::as_u64)
                .map(|value| value as usize)
                .unwrap_or(80)
                .min(100),
            access_count: 0,
            usefulness_score_percent: value
                .get("usefulness_score_percent")
                .and_then(Value::as_u64)
                .map(|value| value as usize)
                .unwrap_or(0)
                .min(100),
            edge_weight_percent: value
                .get("edge_weight_percent")
                .and_then(Value::as_u64)
                .map(|value| value as usize)
                .unwrap_or(100)
                .clamp(50, 500),
            last_recalled_query: String::new(),
        };
        let tenant_id = value
            .get("tenant_id")
            .and_then(Value::as_str)
            .unwrap_or("tenant_local")
            .to_string();
        let actor_id = value
            .get("actor_id")
            .and_then(Value::as_str)
            .unwrap_or("local_user")
            .to_string();
        let durable = self.durable_store.remember_memory(
            &memory,
            &tenant_id,
            &actor_id,
            "ctx.memory.remembered",
        );
        let vector = self.vector_store.upsert_memory(&memory, &tenant_id);
        let cache = self.working_cache.write_memory(&memory, &tenant_id);
        self.push_event("memory_created", &memory.memory_id, &tenant_id);
        self.memories.push(memory.clone());
        Ok(format!(
            r#"{{"name":"mdx-ctx-memory-remember","status":"LIVE-LOCAL-MEMORY-RECORDED","runtime":"mdx-ctx-engine","durable_storage":{},"vector_storage":{},"working_cache":{},"memory":{},"memory_count":{},"event_count":{},"provider_calls_allowed":false,"production_writes_allowed":false}}"#,
            durable.render_json(),
            vector.render_json(),
            cache.render_json(),
            render_memory_json(&memory),
            self.memories.len(),
            self.events.len()
        ))
    }

    fn recall_json(&mut self, body: &str) -> Result<String, String> {
        let value = parse_json(body)?;
        let query = value.get("query").and_then(Value::as_str).unwrap_or("");
        let tier = value
            .get("tier")
            .and_then(Value::as_str)
            .map(normalize_tier);
        let limit = value
            .get("limit")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(10);
        let tenant_id = value
            .get("tenant_id")
            .and_then(Value::as_str)
            .unwrap_or("tenant_local");
        let recalled = top_scored_memories(
            self.memories
                .iter()
                .filter(|memory| tier.as_deref().map(|t| memory.tier == t).unwrap_or(true)),
            query,
            limit,
        );
        let recalled = recalled
            .into_iter()
            .map(|(score, memory)| (score, memory.clone()))
            .collect::<Vec<_>>();
        let recalled_ids = recalled
            .iter()
            .map(|(_, memory)| memory.memory_id.clone())
            .collect::<Vec<_>>();
        let access_update_count = self.record_memory_access(&recalled_ids, query);
        let (vector_report, vector_hits) = self.vector_store.search(query, tenant_id, limit);
        self.push_event("memory_recalled", "ctx_recall", tenant_id);
        if access_update_count > 0 {
            self.push_event("memory_access_recorded", "ctx_recall", tenant_id);
        }
        Ok(format!(
            r#"{{"name":"mdx-ctx-memory-recall","status":"LIVE-LOCAL-MEMORY-RECALLED","runtime":"mdx-ctx-engine","query":{},"tier":{},"result_count":{},"results":[{}],"learning_report":{{"status":"LIVE-LOCAL-CTX-MEMORY-LEARNING-FLOOR","access_update_count":{},"total_memory_access_count":{},"learned_memory_count":{},"max_edge_weight_percent":{},"provider_calls_allowed":false,"production_writes_allowed":false}},"vector_recall":{},"vector_results":[{}],"provider_calls_allowed":false,"production_writes_allowed":false}}"#,
            json_string_literal(query),
            optional_owned_string(tier.as_deref()),
            recalled.len(),
            recalled
                .iter()
                .map(|(score, memory)| format!(
                    r#"{{"score":{},"memory":{}}}"#,
                    score,
                    render_memory_json(memory)
                ))
                .collect::<Vec<_>>()
                .join(","),
            access_update_count,
            self.total_memory_access_count(),
            self.learned_memory_count(),
            self.max_edge_weight_percent(),
            vector_report.render_json(),
            vector_hits
                .iter()
                .map(CtxVectorSearchHit::render_json)
                .collect::<Vec<_>>()
                .join(",")
        ))
    }

    fn memory_feedback_json(&mut self, body: &str) -> Result<String, String> {
        let value = parse_json(body)?;
        let memory_id = value
            .get("memory_id")
            .and_then(Value::as_str)
            .ok_or_else(|| "ctx memory feedback missing memory_id".to_string())?;
        let tenant_id = value
            .get("tenant_id")
            .and_then(Value::as_str)
            .unwrap_or("tenant_local")
            .to_string();
        let query = value
            .get("query")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let usefulness = value
            .get("usefulness_score_percent")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(90)
            .min(100);
        let was_useful = value
            .get("was_useful")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let (before_edge_weight_percent, after_edge_weight_percent, access_count) = {
            let memory = self
                .memories
                .iter_mut()
                .find(|memory| memory.memory_id == memory_id)
                .ok_or_else(|| format!("ctx memory {memory_id} not found"))?;
            let before = memory.edge_weight_percent;
            memory.usefulness_score_percent = usefulness;
            memory.access_count = memory.access_count.saturating_add(1);
            memory.last_recalled_query = query.clone();
            let delta = (usefulness / 10).max(1);
            memory.edge_weight_percent = if was_useful {
                memory.edge_weight_percent.saturating_add(delta).min(500)
            } else {
                memory.edge_weight_percent.saturating_sub(delta).max(50)
            };
            (before, memory.edge_weight_percent, memory.access_count)
        };
        self.push_event("memory_learning_feedback_recorded", memory_id, &tenant_id);
        Ok(format!(
            r#"{{"name":"mdx-ctx-memory-feedback","status":"LIVE-LOCAL-CTX-MEMORY-LEARNING-FLOOR","runtime":"mdx-ctx-engine","memory_id":{},"query":{},"was_useful":{},"usefulness_score_percent":{},"before_edge_weight_percent":{},"after_edge_weight_percent":{},"access_count":{},"learned_memory_count":{},"total_memory_access_count":{},"provider_calls_allowed":false,"production_writes_allowed":false}}"#,
            json_string_literal(memory_id),
            json_string_literal(&query),
            was_useful,
            usefulness,
            before_edge_weight_percent,
            after_edge_weight_percent,
            access_count,
            self.learned_memory_count(),
            self.total_memory_access_count()
        ))
    }

    fn forget_json(&mut self, body: &str) -> Result<String, String> {
        let value = parse_json(body)?;
        let memory_id = value
            .get("memory_id")
            .and_then(Value::as_str)
            .ok_or_else(|| "ctx forget missing memory_id".to_string())?;
        let tenant_id = value
            .get("tenant_id")
            .and_then(Value::as_str)
            .unwrap_or("tenant_local")
            .to_string();
        let before = self.memories.len();
        self.memories.retain(|memory| memory.memory_id != memory_id);
        let removed = before.saturating_sub(self.memories.len());
        let durable = if removed > 0 {
            self.durable_store
                .forget_memory(memory_id, &tenant_id, "local_user")
        } else {
            self.durable_store.noop_report("ctx.memory.forget.missed")
        };
        let vector = if removed > 0 {
            self.vector_store.delete_memory(memory_id, &tenant_id)
        } else {
            self.vector_store.noop_report("PENDING-MEMORY-NOT-FOUND")
        };
        let cache = if removed > 0 {
            self.working_cache.delete_memory(memory_id, &tenant_id)
        } else {
            CtxWorkingCacheReport {
                status: "PENDING-MEMORY-NOT-FOUND".to_string(),
                key: self.working_cache.key(&tenant_id, memory_id),
                ttl_seconds: self.working_cache.ttl_seconds,
            }
        };
        if removed > 0 {
            self.push_event("memory_forgotten", memory_id, &tenant_id);
        }
        Ok(format!(
            r#"{{"name":"mdx-ctx-memory-forget","status":"LIVE-LOCAL-MEMORY-FORGOTTEN","runtime":"mdx-ctx-engine","durable_storage":{},"vector_storage":{},"working_cache":{},"memory_id":{},"removed_count":{},"memory_count":{},"provider_calls_allowed":false,"production_writes_allowed":false}}"#,
            durable.render_json(),
            vector.render_json(),
            cache.render_json(),
            json_string_literal(memory_id),
            removed,
            self.memories.len()
        ))
    }

    fn propose_consolidation_json(&mut self) -> String {
        let existing = self
            .proposals
            .iter()
            .map(|proposal| proposal.source_memory_id.as_str())
            .collect::<Vec<_>>();
        let candidates = self
            .memories
            .iter()
            .filter(|memory| {
                memory.tier == "episodic" && !existing.contains(&memory.memory_id.as_str())
            })
            .cloned()
            .collect::<Vec<_>>();
        for memory in candidates {
            self.next_proposal += 1;
            let proposed_tier = if memory.content.to_ascii_lowercase().contains("when ") {
                "procedural"
            } else {
                "semantic"
            };
            let proposal = CtxConsolidationProposal {
                proposal_id: format!("ctx_proposal_{:06}", self.next_proposal),
                source_memory_id: memory.memory_id.clone(),
                proposed_tier: proposed_tier.to_string(),
                proposed_content: memory.content.clone(),
                confidence_percent: memory.importance.max(70),
                status: "pending".to_string(),
                reviewer_notes: String::new(),
            };
            self.push_event("proposal_created", &proposal.proposal_id, "tenant_local");
            self.proposals.push(proposal);
        }
        self.proposals_json()
    }

    fn review_consolidation_json(&mut self, body: &str, status: &str) -> Result<String, String> {
        let value = parse_json(body)?;
        let proposal_id = value
            .get("proposal_id")
            .and_then(Value::as_str)
            .ok_or_else(|| "ctx proposal review missing proposal_id".to_string())?;
        let notes = value
            .get("reviewer_notes")
            .and_then(Value::as_str)
            .unwrap_or("");
        let proposal = self
            .proposals
            .iter_mut()
            .find(|proposal| proposal.proposal_id == proposal_id)
            .ok_or_else(|| format!("ctx proposal {proposal_id} not found"))?;
        if proposal.status != "pending" {
            return Err(format!("ctx proposal {proposal_id} is not pending"));
        }
        proposal.status = status.to_string();
        proposal.reviewer_notes = notes.to_string();
        let rendered_proposal = render_proposal_json(proposal);
        let proposed_tier = proposal.proposed_tier.clone();
        let proposed_content = proposal.proposed_content.clone();
        let proposal_id_for_memory = proposal.proposal_id.clone();
        let confidence_percent = proposal.confidence_percent;
        let mut promoted_memory = None;
        if status == "approved" {
            self.next_memory += 1;
            let memory = OwnedLocalMemory {
                memory_id: format!("ctx_memory_{:06}", self.next_memory),
                tier: proposed_tier,
                content: proposed_content,
                source_receipt_id: proposal_id_for_memory,
                importance: confidence_percent.min(100),
                access_count: 0,
                usefulness_score_percent: 0,
                edge_weight_percent: 100,
                last_recalled_query: String::new(),
            };
            promoted_memory = Some(memory.clone());
            self.memories.push(memory);
        }
        let event_type = if status == "approved" {
            "proposal_approved"
        } else {
            "proposal_dismissed"
        };
        let durable = promoted_memory
            .as_ref()
            .map(|memory| {
                self.durable_store.remember_memory(
                    memory,
                    "tenant_local",
                    "local_user",
                    "ctx.memory.consolidated",
                )
            })
            .unwrap_or_else(|| {
                self.durable_store
                    .noop_report("ctx.memory.consolidation.dismissed")
            });
        let vector = promoted_memory
            .as_ref()
            .map(|memory| self.vector_store.upsert_memory(memory, "tenant_local"))
            .unwrap_or_else(|| {
                self.vector_store
                    .noop_report("PENDING-CONSOLIDATION-DISMISSED")
            });
        let cache = promoted_memory
            .as_ref()
            .map(|memory| self.working_cache.write_memory(memory, "tenant_local"))
            .unwrap_or_else(|| CtxWorkingCacheReport {
                status: "PENDING-CONSOLIDATION-DISMISSED".to_string(),
                key: "ctx:working:tenant_local:local_session:none".to_string(),
                ttl_seconds: self.working_cache.ttl_seconds,
            });
        self.push_event(event_type, proposal_id, "tenant_local");
        Ok(format!(
            r#"{{"name":"mdx-ctx-consolidation-review","status":"LIVE-LOCAL-CONSOLIDATION-{}","runtime":"mdx-ctx-engine","durable_storage":{},"vector_storage":{},"working_cache":{},"proposal":{},"promoted_memory":{},"provider_calls_allowed":false,"production_writes_allowed":false}}"#,
            status.to_ascii_uppercase(),
            durable.render_json(),
            vector.render_json(),
            cache.render_json(),
            rendered_proposal,
            promoted_memory
                .as_ref()
                .map(render_memory_json)
                .unwrap_or_else(|| "null".to_string())
        ))
    }

    fn memories_json(&self) -> String {
        format!(
            r#"{{"name":"mdx-ctx-memories","status":"LIVE-LOCAL-CTX-MEMORY-RUNTIME","runtime":"mdx-ctx-engine","memory_count":{},"memories":[{}],"provider_calls_allowed":false,"production_writes_allowed":false}}"#,
            self.memories.len(),
            self.memories
                .iter()
                .map(render_memory_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    fn memory_learning_json(&self) -> String {
        format!(
            r#"{{"name":"mdx-ctx-memory-learning","status":"LIVE-LOCAL-CTX-MEMORY-LEARNING-FLOOR","runtime":"mdx-ctx-engine","route":"/ctx/memory-learning.json","feedback_route":"/v1/ctx/memory-feedback","recall_route":"/v1/ctx/recall","memory_count":{},"learned_memory_count":{},"total_memory_access_count":{},"max_edge_weight_percent":{},"adaptation_policy":"recall_access_plus_human_usefulness_feedback","provider_calls_allowed":false,"production_writes_allowed":false,"memories":[{}]}}"#,
            self.memories.len(),
            self.learned_memory_count(),
            self.total_memory_access_count(),
            self.max_edge_weight_percent(),
            self.memories
                .iter()
                .map(render_memory_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    fn vector_index_projection_json(&self) -> String {
        let accepted_count = self
            .vector_index_proofs
            .iter()
            .filter(|proof| proof.terminal_state == "CTX_VECTOR_INDEX_PROOF_RECORDED_LOCAL")
            .count();
        let rejected_count = self
            .vector_index_proofs
            .iter()
            .filter(|proof| {
                proof.terminal_state == "CTX_VECTOR_INDEX_PROOF_REJECTED_SECURITY_BOUNDARY"
            })
            .count();
        format!(
            r#"{{"name":"mdx-ctx-vector-index","status":"LIVE-LOCAL-CTX-PGVECTOR-RETRIEVAL-FLOOR","runtime":"mdx-ctx-engine","route":"/ctx/vector-index.json","proof_route":"/v1/ctx/vector-index-proofs","table":{},"vector_column":"embedding","index":{},"index_method":"hnsw","distance_metric":{},"dimensions":{},"top_k_default":{},"tenant_scoped":true,"rls_policy":"ctx_memory_vectors_tenant_access","schema_sql_hash":{},"query_shape":"tenant_scoped_cosine_similarity_top_k","embedding_values_recorded":false,"provider_calls_allowed":false,"live_database_write_allowed":false,"production_writes_allowed":false,"proof_count":{},"accepted_proof_count":{},"rejected_proof_count":{},"proofs":[{}]}}"#,
            json_string_literal(CTX_VECTOR_TABLE),
            json_string_literal(CTX_VECTOR_INDEX),
            json_string_literal(CTX_VECTOR_DISTANCE_METRIC),
            CTX_VECTOR_DIMENSIONS,
            CTX_VECTOR_DEFAULT_TOP_K,
            json_string_literal(&stable_fingerprint(ctx_vector_schema_sql().as_bytes())),
            self.vector_index_proofs.len(),
            accepted_count,
            rejected_count,
            self.vector_index_proofs
                .iter()
                .map(render_vector_index_proof_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    fn vector_index_proof_json(&mut self, body: &str) -> Result<String, String> {
        let value = parse_json(body)?;
        let tenant_id = value
            .get("tenant_id")
            .and_then(Value::as_str)
            .unwrap_or("tenant_local")
            .to_string();
        let query = value
            .get("query")
            .and_then(Value::as_str)
            .unwrap_or("forge runtime grounded memory")
            .to_string();
        let top_k = value
            .get("top_k")
            .or_else(|| value.get("limit"))
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(CTX_VECTOR_DEFAULT_TOP_K)
            .clamp(1, 50);
        let requested_dimensions = value
            .get("vector_dimensions")
            .or_else(|| value.get("dimensions"))
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(CTX_VECTOR_DIMENSIONS);
        let unsafe_authority = json_bool(&value, "provider_calls_allowed")
            || json_bool(&value, "provider_call_allowed")
            || json_bool(&value, "live_database_write_allowed")
            || json_bool(&value, "production_writes_allowed")
            || json_bool(&value, "production_write_authority")
            || json_bool(&value, "embedding_values_recorded");
        let invalid_dimensions = requested_dimensions != CTX_VECTOR_DIMENSIONS;
        let (terminal_state, rejection_reason) = if unsafe_authority {
            (
                "CTX_VECTOR_INDEX_PROOF_REJECTED_SECURITY_BOUNDARY",
                "provider_database_write_production_or_embedding_value_authority_requested",
            )
        } else if invalid_dimensions {
            (
                "CTX_VECTOR_INDEX_PROOF_REJECTED_SECURITY_BOUNDARY",
                "vector_dimensions_must_match_provider_ready_floor",
            )
        } else {
            ("CTX_VECTOR_INDEX_PROOF_RECORDED_LOCAL", "none")
        };
        self.next_vector_proof += 1;
        let hits = if terminal_state == "CTX_VECTOR_INDEX_PROOF_RECORDED_LOCAL" {
            self.vector_proof_hits(&query, top_k)
        } else {
            Vec::new()
        };
        let proof = CtxVectorIndexProof {
            proof_id: format!("ctx_vector_index_proof_{:06}", self.next_vector_proof),
            tenant_id: tenant_id.clone(),
            query: query.clone(),
            top_k,
            requested_dimensions,
            terminal_state: terminal_state.to_string(),
            rejection_reason: rejection_reason.to_string(),
            retrieval_result_count: hits.len(),
            query_embedding_hash: embedding_fingerprint(0, "query", &tenant_id, &query),
            hits,
        };
        let event_type = if proof.terminal_state == "CTX_VECTOR_INDEX_PROOF_RECORDED_LOCAL" {
            "vector_index_retrieval_proof_recorded"
        } else {
            "vector_index_retrieval_proof_rejected"
        };
        self.push_event(event_type, &proof.proof_id, &tenant_id);
        let body = render_vector_index_proof_response_json(&proof);
        self.vector_index_proofs.push(proof);
        Ok(body)
    }

    fn vector_proof_hits(&self, query: &str, top_k: usize) -> Vec<CtxVectorProofHit> {
        top_scored_memories(self.memories.iter(), query, top_k)
            .into_iter()
            .map(|(score_percent, memory)| CtxVectorProofHit {
                memory_id: memory.memory_id.clone(),
                tier: memory.tier.clone(),
                score_percent,
                memory_embedding_hash: embedding_fingerprint(
                    memory.access_count,
                    &memory.memory_id,
                    &memory.tier,
                    &memory.content,
                ),
            })
            .collect()
    }

    fn proposals_json(&self) -> String {
        format!(
            r#"{{"name":"mdx-ctx-consolidation-proposals","status":"LIVE-LOCAL-CONSOLIDATION-REVIEW","runtime":"mdx-ctx-engine","proposal_count":{},"pending_count":{},"auto_promotion_allowed":false,"human_review_required":true,"proposals":[{}],"provider_calls_allowed":false,"production_writes_allowed":false}}"#,
            self.proposals.len(),
            self.proposals
                .iter()
                .filter(|proposal| proposal.status == "pending")
                .count(),
            self.proposals
                .iter()
                .map(render_proposal_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    fn events_json(&self) -> String {
        format!(
            r#"{{"name":"mdx-ctx-events","status":"LIVE-LOCAL-CTX-EVENT-LOG","runtime":"mdx-ctx-engine","stream":"ctx_event_stream","relay_driver":"mdx-message-relay","relay_topic_pattern":"mdx:ctx:{{tenant_id}}:events","event_count":{},"events":[{}],"websocket_status":{},"provider_calls_allowed":false,"production_writes_allowed":false}}"#,
            self.events.len(),
            self.events
                .iter()
                .map(render_event_json)
                .collect::<Vec<_>>()
                .join(","),
            json_string_literal(self.event_stream_status())
        )
    }

    fn local_context_packet(
        &self,
        query: &str,
        token_budget: usize,
    ) -> Result<LocalContextPacket, String> {
        let mut kernel = MdxKernel::boot_local();
        kernel.run_evals_runner_agent()?;
        let local_memories = local_memory_inputs(&self.memories);
        kernel
            .assemble_local_context(LocalContextRequest {
                tenant_id: "tenant_local",
                actor_id: "local_user",
                query,
                token_budget,
                source_preferences: &[],
                query_entities: &["developer", "world_model"],
                local_memories: &local_memories,
            })
            .map_err(|error| error.message())
    }

    fn assemble_from_request(
        &self,
        request: &OwnedAssembleRequest,
    ) -> Result<LocalContextPacket, String> {
        let preferences = request
            .preferences
            .iter()
            .map(|preference| LocalContextSourcePreference {
                source_type: preference.source_type.as_str(),
                enabled: preference.enabled,
                token_budget_override: preference.token_budget_override,
            })
            .collect::<Vec<_>>();
        let query_entities = request
            .query_entities
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let mut combined = self.memories.clone();
        combined.extend(request.local_memories.clone());
        let local_memories = local_memory_inputs(&combined);
        let mut kernel = MdxKernel::boot_local();
        kernel.run_evals_runner_agent()?;
        kernel
            .assemble_local_context(LocalContextRequest {
                tenant_id: &request.tenant_id,
                actor_id: &request.actor_id,
                query: &request.query,
                token_budget: request.token_budget,
                source_preferences: &preferences,
                query_entities: &query_entities,
                local_memories: &local_memories,
            })
            .map_err(|error| error.message())
    }

    fn push_event(&mut self, event_type: &str, subject_id: &str, tenant_id: &str) {
        self.next_event += 1;
        let mut event = CtxRuntimeEvent {
            sequence: self.next_event,
            event_id: format!("ctx_event_{:06}", self.next_event),
            event_type: event_type.to_string(),
            subject_id: subject_id.to_string(),
            tenant_id: tenant_id.to_string(),
            topic: ctx_event_topic(tenant_id),
            relay_status: "PENDING-RELAY-PUBLISH".to_string(),
        };
        event.relay_status = self.event_relay.publish(&event);
        self.events.push(event);
    }

    fn event_stream_status(&self) -> &str {
        if self
            .events
            .iter()
            .any(|event| event.relay_status == "LIVE-LOCAL-CTX-WEBSOCKET-RELAY-PUBLISHED")
        {
            "LIVE-LOCAL-CTX-WEBSOCKET-RELAY"
        } else {
            "PENDING-LIVE-RUN"
        }
    }

    fn record_memory_access(&mut self, memory_ids: &[String], query: &str) -> usize {
        let mut updated = 0;
        for memory in &mut self.memories {
            if memory_ids.contains(&memory.memory_id) {
                memory.access_count = memory.access_count.saturating_add(1);
                memory.last_recalled_query = query.to_string();
                memory.edge_weight_percent = memory.edge_weight_percent.saturating_add(1).min(500);
                updated += 1;
            }
        }
        updated
    }

    fn learned_memory_count(&self) -> usize {
        self.memories
            .iter()
            .filter(|memory| memory.access_count > 0 || memory.usefulness_score_percent > 0)
            .count()
    }

    fn total_memory_access_count(&self) -> usize {
        self.memories.iter().map(|memory| memory.access_count).sum()
    }

    fn max_edge_weight_percent(&self) -> usize {
        self.memories
            .iter()
            .map(|memory| memory.edge_weight_percent)
            .max()
            .unwrap_or(100)
    }

    fn world_model_memory_candidates(&self) -> Vec<CtxWorldModelMemoryCandidate> {
        self.memories
            .iter()
            .map(|memory| CtxWorldModelMemoryCandidate {
                memory_id: memory.memory_id.clone(),
                tier: memory.tier.clone(),
                content: memory.content.clone(),
                source_receipt_id: memory.source_receipt_id.clone(),
                importance: memory.importance,
                access_count: memory.access_count,
                usefulness_score_percent: memory.usefulness_score_percent,
                edge_weight_percent: memory.edge_weight_percent,
            })
            .collect()
    }
}

fn parse_assemble_request(body: &str) -> Result<OwnedAssembleRequest, String> {
    let value: Value = serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid assemble request json: {error}"))?;
    let tenant_id = value
        .get("tenant_id")
        .and_then(Value::as_str)
        .unwrap_or("tenant_local")
        .to_string();
    let actor_id = value
        .get("actor_id")
        .or_else(|| value.get("agent_id"))
        .and_then(Value::as_str)
        .unwrap_or("local_user")
        .to_string();
    let query = value
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or("developer onboarding memory proof world model")
        .to_string();
    let token_budget = value
        .get("token_budget")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(240);
    let preferences = value
        .get("source_preferences")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|preference| {
            let source_type = preference.get("source_type")?.as_str()?.to_string();
            let enabled = preference
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let token_budget_override = preference
                .get("token_budget_override")
                .and_then(Value::as_u64)
                .map(|value| value as usize);
            Some(OwnedPreference {
                source_type,
                enabled,
                token_budget_override,
            })
        })
        .collect();
    let query_entities = value
        .get("query_entities")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();
    let local_memories = value
        .get("local_memories")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|memory| {
            let content = memory.get("content")?.as_str()?.to_string();
            let memory_id = memory
                .get("memory_id")
                .and_then(Value::as_str)
                .unwrap_or("local_memory_input")
                .to_string();
            let tier = memory
                .get("tier")
                .and_then(Value::as_str)
                .unwrap_or("episodic")
                .to_string();
            let source_receipt_id = memory
                .get("source_receipt_id")
                .and_then(Value::as_str)
                .unwrap_or("local_memory_input_receipt")
                .to_string();
            let importance = memory
                .get("importance")
                .and_then(Value::as_u64)
                .map(|value| value as usize)
                .unwrap_or(80);
            Some(OwnedLocalMemory {
                memory_id,
                tier,
                content,
                source_receipt_id,
                importance,
                access_count: memory
                    .get("access_count")
                    .and_then(Value::as_u64)
                    .map(|value| value as usize)
                    .unwrap_or(0),
                usefulness_score_percent: memory
                    .get("usefulness_score_percent")
                    .and_then(Value::as_u64)
                    .map(|value| value as usize)
                    .unwrap_or(0)
                    .min(100),
                edge_weight_percent: memory
                    .get("edge_weight_percent")
                    .and_then(Value::as_u64)
                    .map(|value| value as usize)
                    .unwrap_or(100)
                    .clamp(50, 500),
                last_recalled_query: memory
                    .get("last_recalled_query")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            })
        })
        .collect();
    Ok(OwnedAssembleRequest {
        tenant_id,
        actor_id,
        query,
        token_budget,
        preferences,
        query_entities,
        local_memories,
    })
}

struct Response {
    status: &'static str,
    content_type: &'static str,
    body: String,
}

impl Response {
    fn text(status: &'static str, body: String) -> Self {
        Self {
            status,
            content_type: TEXT_CONTENT_TYPE,
            body,
        }
    }

    fn json(status: &'static str, body: String) -> Self {
        Self {
            status,
            content_type: JSON_CONTENT_TYPE,
            body,
        }
    }
}

fn render_packet_json(packet: &LocalContextPacket) -> String {
    let allocations = packet
        .budget_allocations
        .iter()
        .map(|allocation| {
            format!(
                r#"{{"source_type":{},"priority":{},"token_budget":{},"fixed":{}}}"#,
                json_string_literal(allocation.source_type),
                allocation.priority,
                allocation.token_budget,
                allocation.fixed
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let sources = packet
        .sources
        .iter()
        .map(|source| {
            let receipts = source
                .source_receipt_ids
                .iter()
                .map(|receipt| json_string_literal(receipt))
                .collect::<Vec<_>>()
                .join(",");
            format!(
                r#"{{"source_id":{},"source_type":{},"priority":{},"title":{},"content":{},"token_estimate":{},"budget_allocated":{},"fixed":{},"score":{{"lexical":{},"recency":{},"importance":{},"graph":{},"composite":{}}},"source_receipt_ids":[{}],"memory_driver":{},"memory_provider":{},"memory_tier":{},"world_model_source":{}}}"#,
                json_string_literal(&source.source_id),
                json_string_literal(source.source_type),
                source.priority,
                json_string_literal(&source.title),
                json_string_literal(&source.content),
                source.token_estimate,
                source.budget_allocated,
                source.fixed,
                source.score.lexical,
                source.score.recency,
                source.score.importance,
                source.score.graph,
                source.score.composite,
                receipts,
                optional_string(source.memory_driver),
                optional_string(source.memory_provider),
                optional_string(source.memory_tier),
                optional_string(source.world_model_source)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let truncated = packet
        .truncated_sources
        .iter()
        .map(|source| {
            format!(
                r#"{{"source_id":{},"source_type":{},"token_estimate":{},"composite_score":{},"reason":{}}}"#,
                json_string_literal(&source.source_id),
                json_string_literal(source.source_type),
                source.token_estimate,
                source.composite_score,
                json_string_literal(source.reason)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let preferences = packet
        .source_preferences_applied
        .iter()
        .map(|preference| {
            format!(
                r#"{{"source_type":{},"enabled":{},"token_budget_override":{}}}"#,
                json_string_literal(&preference.source_type),
                preference.enabled,
                optional_usize(preference.token_budget_override)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let hygiene_strategies = packet
        .hygiene_report
        .strategies_applied
        .iter()
        .map(|strategy| json_string_literal(strategy))
        .collect::<Vec<_>>()
        .join(",");
    let memory_tiers = packet
        .memory_tiers
        .iter()
        .map(|tier| {
            format!(
                r#"{{"tier":{},"source_count":{},"included_source_count":{},"token_count":{},"retention_policy":{},"recall_policy":{},"write_path":{},"cache_driver":{},"consolidation_state":{}}}"#,
                json_string_literal(tier.tier),
                tier.source_count,
                tier.included_source_count,
                tier.token_count,
                json_string_literal(tier.retention_policy),
                json_string_literal(tier.recall_policy),
                json_string_literal(tier.write_path),
                optional_string(tier.cache_driver),
                json_string_literal(tier.consolidation_state)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let promotion_targets = string_array(&packet.consolidation_report.promotion_targets);
    let summary_sections = string_array(&packet.handoff_report.summary_sections);
    let handoff_strategy_order = string_array(&packet.handoff_report.strategy_order);
    format!(
        r#"{{"name":"mdx-ctx-local-context-packet","status":{},"runtime":"mdx-ctx-engine","route":"/ctx/local-context-packet.json","memory_tiers_route":"/ctx/memory-tiers.json","assemble_route":"/v1/assemble","read_only":true,"writes_allowed":false,"ctx_driver":{},"ctx_provider":{},"memory_driver":{},"memory_provider":{},"vector_driver":{},"working_cache_driver":{},"vendor_memory_driver":"mem0_memory_store","vendor_embedding_status":"PENDING-LIVE-RUN","query":{},"query_entity_count":{},"local_memory_input_count":{},"token_budget":{},"total_tokens":{},"budget_remaining":{},"source_type_count":{},"source_count":{},"included_source_count":{},"truncated_source_count":{},"disabled_source_count":{},"hot_path_budget_ms":{},"estimated_p95_ms":{},"assembly_log_id":{},"assembly_receipt_id":{},"embedding_refusal_receipt_id":{},"provider_call_allowed":{},"embedding_provider_call_allowed":{},"production_write_allowed":{},"budget_allocations":[{}],"sources":[{}],"truncated_sources":[{}],"source_preferences_applied":[{}],"hygiene_report":{{"tokens_in":{},"tokens_out":{},"reduction_percent_x100":{},"duplicate_sources_removed":{},"strategies_applied":[{}]}},"memory_tiers":[{}],"consolidation_report":{{"status":{},"candidate_count":{},"pending_review_count":{},"auto_promotion_allowed":{},"human_review_required":{},"dedup_threshold_percent_x100":{},"promotion_targets":[{}]}},"handoff_report":{{"status":{},"summary_sections":[{}],"strategy_order":[{}],"max_summary_tokens":{},"preserve_verbatim_supported":{}}},"vector_report":{{"status":{},"driver":{},"table":{},"dimensions":{},"tenant_scoped":{},"live_index_ready":{},"provider_call_allowed":{}}},"working_cache_report":{{"status":{},"driver":{},"namespace":{},"ttl_seconds":{},"write_through_postgres":{},"live_cache_ready":{}}},"signal_report":{{"status":{},"signal_source_count":{},"search_policy":{},"cross_connection_policy":{},"tenant_scoped":{}}},"event_stream_report":{{"status":{},"stream":{},"websocket_allowed":{},"provider_call_allowed":{},"replay_policy":{}}}}}"#,
        json_string_literal(packet.status),
        json_string_literal(packet.ctx_driver),
        json_string_literal(packet.ctx_provider),
        json_string_literal(packet.memory_driver),
        json_string_literal(packet.memory_provider),
        json_string_literal(packet.vector_driver),
        json_string_literal(packet.working_cache_driver),
        json_string_literal(&packet.query),
        packet.query_entity_count,
        packet.local_memory_input_count,
        packet.token_budget,
        packet.total_tokens,
        packet.budget_remaining,
        packet.source_type_count,
        packet.source_count,
        packet.included_source_count,
        packet.truncated_source_count,
        packet.disabled_source_count,
        packet.hot_path_budget_ms,
        packet.estimated_p95_ms,
        json_string_literal(&packet.assembly_log_id),
        json_string_literal(&packet.assembly_receipt_id),
        json_string_literal(&packet.embedding_refusal_receipt_id),
        packet.provider_call_allowed,
        packet.embedding_provider_call_allowed,
        packet.production_write_allowed,
        allocations,
        sources,
        truncated,
        preferences,
        packet.hygiene_report.tokens_in,
        packet.hygiene_report.tokens_out,
        packet.hygiene_report.reduction_percent_x100,
        packet.hygiene_report.duplicate_sources_removed,
        hygiene_strategies,
        memory_tiers,
        json_string_literal(packet.consolidation_report.status),
        packet.consolidation_report.candidate_count,
        packet.consolidation_report.pending_review_count,
        packet.consolidation_report.auto_promotion_allowed,
        packet.consolidation_report.human_review_required,
        packet.consolidation_report.dedup_threshold_percent_x100,
        promotion_targets,
        json_string_literal(packet.handoff_report.status),
        summary_sections,
        handoff_strategy_order,
        packet.handoff_report.max_summary_tokens,
        packet.handoff_report.preserve_verbatim_supported,
        json_string_literal(packet.vector_report.status),
        json_string_literal(packet.vector_report.driver),
        json_string_literal(packet.vector_report.table),
        packet.vector_report.dimensions,
        packet.vector_report.tenant_scoped,
        packet.vector_report.live_index_ready,
        packet.vector_report.provider_call_allowed,
        json_string_literal(packet.working_cache_report.status),
        json_string_literal(packet.working_cache_report.driver),
        json_string_literal(packet.working_cache_report.namespace),
        packet.working_cache_report.ttl_seconds,
        packet.working_cache_report.write_through_postgres,
        packet.working_cache_report.live_cache_ready,
        json_string_literal(packet.signal_report.status),
        packet.signal_report.signal_source_count,
        json_string_literal(packet.signal_report.search_policy),
        json_string_literal(packet.signal_report.cross_connection_policy),
        packet.signal_report.tenant_scoped,
        json_string_literal(packet.event_stream_report.status),
        json_string_literal(packet.event_stream_report.stream),
        packet.event_stream_report.websocket_allowed,
        packet.event_stream_report.provider_call_allowed,
        json_string_literal(packet.event_stream_report.replay_policy)
    )
}

fn optional_string(value: Option<&str>) -> String {
    value
        .map(json_string_literal)
        .unwrap_or_else(|| "null".to_string())
}

fn optional_usize(value: Option<usize>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "null".to_string())
}

fn string_array(values: &[&str]) -> String {
    values
        .iter()
        .map(|value| json_string_literal(value))
        .collect::<Vec<_>>()
        .join(",")
}

fn parse_json(body: &str) -> Result<Value, String> {
    serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid ctx runtime request json: {error}"))
}

fn parse_rendered_json(body: &str, label: &str) -> Result<Value, String> {
    serde_json::from_str(body)
        .map_err(|error| format!("invalid rendered ctx {label} json: {error}"))
}

fn json_usize(value: &Value, key: &str) -> usize {
    value
        .get(key)
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(0)
}

fn json_array_len(value: &Value, key: &str) -> usize {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0)
}

fn json_bool(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn durable_memory_upsert_sql(
    memory: &OwnedLocalMemory,
    tenant_id: &str,
    actor_id: &str,
    receipt_kind: &str,
) -> String {
    let payload = format!(
        r#"{{"memory_id":{},"tier":{},"durable_driver":"postgres_memory_records","memory_driver":"local_memory_store"}}"#,
        json_string_literal(&memory.memory_id),
        json_string_literal(&memory.tier)
    );
    let memory_sql = format!(
        "INSERT INTO memory_records (memory_id, tenant_id, source_receipt_id, consolidation_decision, content) VALUES ({}, {}, {}, {}, {}) ON CONFLICT (memory_id) DO UPDATE SET source_receipt_id = EXCLUDED.source_receipt_id, consolidation_decision = EXCLUDED.consolidation_decision, content = EXCLUDED.content;",
        sql_string_literal(&memory.memory_id),
        sql_string_literal(tenant_id),
        sql_string_literal(&memory.source_receipt_id),
        sql_string_literal("RETAIN"),
        sql_string_literal(&memory.content)
    );
    format!(
        "BEGIN;\nSET LOCAL mdx.tenant_id = {};\n{}\n{}\n{}\n{}\nSELECT count(*) FROM memory_records WHERE tenant_id = {};\nCOMMIT;\n",
        sql_string_literal(tenant_id),
        identity_sql(tenant_id, actor_id),
        receipt_sql(
            &memory.source_receipt_id,
            tenant_id,
            actor_id,
            receipt_kind,
            &payload
        ),
        chain_head_sql(tenant_id, &memory.source_receipt_id),
        memory_sql,
        sql_string_literal(tenant_id)
    )
}

fn durable_memory_forget_sql(
    memory_id: &str,
    tenant_id: &str,
    actor_id: &str,
    receipt_id: &str,
) -> String {
    let payload = format!(
        r#"{{"memory_id":{},"durable_driver":"postgres_memory_records","memory_driver":"local_memory_store"}}"#,
        json_string_literal(memory_id)
    );
    format!(
        "BEGIN;\nSET LOCAL mdx.tenant_id = {};\n{}\n{}\n{}\nDELETE FROM memory_records WHERE tenant_id = {} AND memory_id = {};\nSELECT count(*) FROM memory_records WHERE tenant_id = {};\nCOMMIT;\n",
        sql_string_literal(tenant_id),
        identity_sql(tenant_id, actor_id),
        receipt_sql(
            receipt_id,
            tenant_id,
            actor_id,
            "ctx.memory.forgotten",
            &payload
        ),
        chain_head_sql(tenant_id, receipt_id),
        sql_string_literal(tenant_id),
        sql_string_literal(memory_id),
        sql_string_literal(tenant_id)
    )
}

fn vector_upsert_sql(memory: &OwnedLocalMemory, tenant_id: &str) -> String {
    let embedding = vector_literal(&deterministic_embedding(&memory.content));
    format!(
        "BEGIN;\nSET LOCAL mdx.tenant_id = {};\n{}\nINSERT INTO ctx_memory_vectors (vector_id, tenant_id, memory_id, source_receipt_id, tier, content, embedding) VALUES ({}, {}, {}, {}, {}, {}, {}::vector) ON CONFLICT (vector_id) DO UPDATE SET source_receipt_id = EXCLUDED.source_receipt_id, tier = EXCLUDED.tier, content = EXCLUDED.content, embedding = EXCLUDED.embedding;\nSELECT count(*) FROM ctx_memory_vectors WHERE tenant_id = {};\nCOMMIT;\n",
        sql_string_literal(tenant_id),
        ctx_vector_schema_sql(),
        sql_string_literal(&format!("ctx_vector_{}", memory.memory_id)),
        sql_string_literal(tenant_id),
        sql_string_literal(&memory.memory_id),
        sql_string_literal(&memory.source_receipt_id),
        sql_string_literal(&memory.tier),
        sql_string_literal(&memory.content),
        sql_string_literal(&embedding),
        sql_string_literal(tenant_id)
    )
}

fn vector_delete_sql(memory_id: &str, tenant_id: &str) -> String {
    format!(
        "BEGIN;\nSET LOCAL mdx.tenant_id = {};\nDELETE FROM ctx_memory_vectors WHERE tenant_id = {} AND memory_id = {};\nSELECT count(*) FROM ctx_memory_vectors WHERE tenant_id = {};\nCOMMIT;\n",
        sql_string_literal(tenant_id),
        sql_string_literal(tenant_id),
        sql_string_literal(memory_id),
        sql_string_literal(tenant_id)
    )
}

fn vector_search_sql(query: &str, tenant_id: &str, limit: usize) -> String {
    let embedding = vector_literal(&deterministic_embedding(query));
    format!(
        "BEGIN;\nSET LOCAL mdx.tenant_id = {};\nSELECT memory_id || E'\\t' || tier || E'\\t' || greatest(0, least(100, round((1 - (embedding <=> {}::vector)) * 100)::int)) FROM ctx_memory_vectors WHERE tenant_id = {} ORDER BY embedding <=> {}::vector LIMIT {};\nCOMMIT;\n",
        sql_string_literal(tenant_id),
        sql_string_literal(&embedding),
        sql_string_literal(tenant_id),
        sql_string_literal(&embedding),
        limit.max(1)
    )
}

fn ctx_vector_schema_sql() -> String {
    format!(
        "CREATE EXTENSION IF NOT EXISTS vector;\nCREATE TABLE IF NOT EXISTS ctx_memory_vectors (vector_id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id), memory_id TEXT NOT NULL REFERENCES memory_records(memory_id) ON DELETE CASCADE, source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id), tier TEXT NOT NULL CHECK (tier IN ('working', 'episodic', 'semantic', 'procedural')), content TEXT NOT NULL, embedding VECTOR({CTX_VECTOR_DIMENSIONS}) NOT NULL, created_at TIMESTAMPTZ NOT NULL DEFAULT now());\nALTER TABLE ctx_memory_vectors ENABLE ROW LEVEL SECURITY;\nDROP POLICY IF EXISTS ctx_memory_vectors_tenant_access ON ctx_memory_vectors;\nCREATE POLICY ctx_memory_vectors_tenant_access ON ctx_memory_vectors USING (tenant_id = current_setting('mdx.tenant_id', true));\nCREATE INDEX IF NOT EXISTS ctx_memory_vectors_tenant_tier_idx ON ctx_memory_vectors(tenant_id, tier, created_at DESC);\nCREATE INDEX IF NOT EXISTS {CTX_VECTOR_INDEX} ON ctx_memory_vectors USING hnsw (embedding vector_cosine_ops);"
    )
}

fn identity_sql(tenant_id: &str, actor_id: &str) -> String {
    format!(
        "INSERT INTO tenants (tenant_id, name) VALUES ({}, {}) ON CONFLICT (tenant_id) DO NOTHING;\nINSERT INTO actors (actor_id, tenant_id, display_name, role) VALUES ({}, {}, {}, {}) ON CONFLICT (actor_id) DO NOTHING;",
        sql_string_literal(tenant_id),
        sql_string_literal("MDx local tenant"),
        sql_string_literal(actor_id),
        sql_string_literal(tenant_id),
        sql_string_literal(actor_id),
        sql_string_literal("operator")
    )
}

fn receipt_sql(
    receipt_id: &str,
    tenant_id: &str,
    actor_id: &str,
    kind: &str,
    payload: &str,
) -> String {
    let hash = format!("ctx_hash_{receipt_id}");
    format!(
        "INSERT INTO ledger_entries (receipt_id, tenant_id, trace_id, actor_id, loop_id, workflow_id, kind, policy_decision_id, payload, previous_hash, hash) VALUES ({}, {}, {}, {}, {}, {}, {}, NULL, {}::jsonb, NULL, {}) ON CONFLICT (receipt_id) DO NOTHING;",
        sql_string_literal(receipt_id),
        sql_string_literal(tenant_id),
        sql_string_literal("trace_ctx_durable_memory"),
        sql_string_literal(actor_id),
        sql_string_literal("ctx_local_engine"),
        sql_string_literal("ctx_durable_memory"),
        sql_string_literal(kind),
        sql_string_literal(payload),
        sql_string_literal(&hash)
    )
}

fn chain_head_sql(tenant_id: &str, receipt_id: &str) -> String {
    let hash = format!("ctx_hash_{receipt_id}");
    format!(
        "INSERT INTO ledger_chain_heads (tenant_id, head_hash, receipt_id) VALUES ({}, {}, {}) ON CONFLICT (tenant_id) DO UPDATE SET head_hash = EXCLUDED.head_hash, receipt_id = EXCLUDED.receipt_id, updated_at = now();",
        sql_string_literal(tenant_id),
        sql_string_literal(&hash),
        sql_string_literal(receipt_id)
    )
}

fn run_psql(database_url: &str, sql: &str) -> Result<Option<usize>, String> {
    let mut child = Command::new("psql")
        .arg(database_url)
        .arg("-v")
        .arg("ON_ERROR_STOP=1")
        .arg("-At")
        .arg("-q")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("spawn psql: {error}"))?;
    child
        .stdin
        .as_mut()
        .ok_or_else(|| "psql stdin unavailable".to_string())?
        .write_all(sql.as_bytes())
        .map_err(|error| format!("write psql stdin: {error}"))?;
    let output = child
        .wait_with_output()
        .map_err(|error| format!("wait psql: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .rev()
        .find_map(|line| line.trim().parse::<usize>().ok()))
}

fn run_psql_rows(database_url: &str, sql: &str) -> Result<Vec<String>, String> {
    let mut child = Command::new("psql")
        .arg(database_url)
        .arg("-v")
        .arg("ON_ERROR_STOP=1")
        .arg("-At")
        .arg("-q")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("spawn psql: {error}"))?;
    child
        .stdin
        .as_mut()
        .ok_or_else(|| "psql stdin unavailable".to_string())?
        .write_all(sql.as_bytes())
        .map_err(|error| format!("write psql stdin: {error}"))?;
    let output = child
        .wait_with_output()
        .map_err(|error| format!("wait psql: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::to_string)
        .collect())
}

fn run_valkey_command(container: Option<&str>, args: &[&str]) -> Result<String, String> {
    let candidates = container
        .map(|container| vec![container.to_string()])
        .unwrap_or_else(|| {
            vec![
                "mdx-native-ui-valkey-1".to_string(),
                "mdx-native-valkey-1".to_string(),
            ]
        });
    let mut errors = Vec::new();
    for candidate in candidates {
        let output = Command::new("docker")
            .arg("exec")
            .arg(&candidate)
            .arg("valkey-cli")
            .args(args)
            .output();
        match output {
            Ok(output) if output.status.success() => {
                return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
            }
            Ok(output) => errors.push(format!(
                "{candidate}:{}",
                String::from_utf8_lossy(&output.stderr).trim()
            )),
            Err(error) => errors.push(format!("{candidate}:{error}")),
        }
    }
    Err(errors.join(";"))
}

fn deterministic_embedding(content: &str) -> Vec<f32> {
    let mut dims = vec![0.0_f32; CTX_VECTOR_DIMENSIONS];
    for (index, byte) in content.bytes().enumerate() {
        let slot = index % dims.len();
        let weight = ((slot % 17) + 1) as f32;
        dims[slot] += (byte as f32 / 255.0) * weight;
        let mirrored = (slot * 31 + 7) % dims.len();
        dims[mirrored] += (byte as f32 / 255.0) * 0.125;
    }
    let norm = dims.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm > 0.0 {
        for value in &mut dims {
            *value /= norm;
        }
    }
    dims
}

fn vector_literal(values: &[f32]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| format!("{value:.6}"))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn embedding_fingerprint<T: std::fmt::Display>(
    sequence: usize,
    identity: &str,
    tier_or_tenant: &str,
    content: T,
) -> String {
    let material = format!(
        "{CTX_VECTOR_DIMENSIONS}:{CTX_VECTOR_DISTANCE_METRIC}:{sequence}:{identity}:{tier_or_tenant}:{content}"
    );
    stable_fingerprint(material.as_bytes())
}

fn stable_fingerprint(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("fnv1a64:{hash:016x}")
}

fn sql_string_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn normalize_tier(tier: &str) -> String {
    match tier {
        "working" | "semantic" | "procedural" => tier.to_string(),
        _ => "episodic".to_string(),
    }
}

fn local_memory_inputs(memories: &[OwnedLocalMemory]) -> Vec<LocalContextMemoryInput<'_>> {
    memories
        .iter()
        .map(|memory| LocalContextMemoryInput {
            memory_id: memory.memory_id.as_str(),
            tier: memory.tier.as_str(),
            content: memory.content.as_str(),
            source_receipt_id: memory.source_receipt_id.as_str(),
            importance: memory.importance,
        })
        .collect()
}

fn render_vector_index_proof_response_json(proof: &CtxVectorIndexProof) -> String {
    format!(
        r#"{{"name":"mdx-ctx-vector-index-proof","status":"LIVE-LOCAL-CTX-PGVECTOR-RETRIEVAL-FLOOR","runtime":"mdx-ctx-engine","route":"/v1/ctx/vector-index-proofs","proof_id":{},"tenant_id":{},"terminal_state":{},"rejection_reason":{},"query":{},"table":{},"index":{},"index_method":"hnsw","distance_metric":{},"dimensions":{},"requested_dimensions":{},"top_k":{},"retrieval_result_count":{},"query_embedding_hash":{},"embedding_values_recorded":false,"provider_calls_allowed":false,"live_database_write_allowed":false,"production_writes_allowed":false,"hits":[{}]}}"#,
        json_string_literal(&proof.proof_id),
        json_string_literal(&proof.tenant_id),
        json_string_literal(&proof.terminal_state),
        json_string_literal(&proof.rejection_reason),
        json_string_literal(&proof.query),
        json_string_literal(CTX_VECTOR_TABLE),
        json_string_literal(CTX_VECTOR_INDEX),
        json_string_literal(CTX_VECTOR_DISTANCE_METRIC),
        CTX_VECTOR_DIMENSIONS,
        proof.requested_dimensions,
        proof.top_k,
        proof.retrieval_result_count,
        json_string_literal(&proof.query_embedding_hash),
        proof
            .hits
            .iter()
            .map(render_vector_proof_hit_json)
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn render_vector_index_proof_json(proof: &CtxVectorIndexProof) -> String {
    format!(
        r#"{{"proof_id":{},"tenant_id":{},"terminal_state":{},"rejection_reason":{},"query":{},"top_k":{},"retrieval_result_count":{},"query_embedding_hash":{},"hits":[{}]}}"#,
        json_string_literal(&proof.proof_id),
        json_string_literal(&proof.tenant_id),
        json_string_literal(&proof.terminal_state),
        json_string_literal(&proof.rejection_reason),
        json_string_literal(&proof.query),
        proof.top_k,
        proof.retrieval_result_count,
        json_string_literal(&proof.query_embedding_hash),
        proof
            .hits
            .iter()
            .map(render_vector_proof_hit_json)
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn render_vector_proof_hit_json(hit: &CtxVectorProofHit) -> String {
    format!(
        r#"{{"memory_id":{},"tier":{},"score_percent":{},"memory_embedding_hash":{}}}"#,
        json_string_literal(&hit.memory_id),
        json_string_literal(&hit.tier),
        hit.score_percent,
        json_string_literal(&hit.memory_embedding_hash)
    )
}

fn render_memory_json(memory: &OwnedLocalMemory) -> String {
    format!(
        r#"{{"memory_id":{},"tier":{},"content":{},"source_receipt_id":{},"importance":{},"access_count":{},"usefulness_score_percent":{},"edge_weight_percent":{},"last_recalled_query":{},"memory_driver":"local_memory_store","memory_provider":"InMemoryProvider"}}"#,
        json_string_literal(&memory.memory_id),
        json_string_literal(&memory.tier),
        json_string_literal(&memory.content),
        json_string_literal(&memory.source_receipt_id),
        memory.importance,
        memory.access_count,
        memory.usefulness_score_percent,
        memory.edge_weight_percent,
        json_string_literal(&memory.last_recalled_query)
    )
}

fn render_proposal_json(proposal: &CtxConsolidationProposal) -> String {
    format!(
        r#"{{"proposal_id":{},"source_memory_id":{},"proposed_tier":{},"proposed_content":{},"confidence_percent":{},"status":{},"reviewer_notes":{}}}"#,
        json_string_literal(&proposal.proposal_id),
        json_string_literal(&proposal.source_memory_id),
        json_string_literal(&proposal.proposed_tier),
        json_string_literal(&proposal.proposed_content),
        proposal.confidence_percent,
        json_string_literal(&proposal.status),
        json_string_literal(&proposal.reviewer_notes)
    )
}

fn render_event_json(event: &CtxRuntimeEvent) -> String {
    format!(
        r#"{{"sequence":{},"event_id":{},"event_type":{},"subject_id":{},"tenant_id":{},"topic":{},"stream_id":"ctx_event_stream","relay_status":{},"websocket_fanout_allowed":{},"production_delivery_allowed":false}}"#,
        event.sequence,
        json_string_literal(&event.event_id),
        json_string_literal(&event.event_type),
        json_string_literal(&event.subject_id),
        json_string_literal(&event.tenant_id),
        json_string_literal(&event.topic),
        json_string_literal(&event.relay_status),
        event.relay_status == "LIVE-LOCAL-CTX-WEBSOCKET-RELAY-PUBLISHED"
    )
}

fn render_event_envelope_json(event: &CtxRuntimeEvent) -> String {
    format!(
        r#"{{"type":"envelope","domain":"ctx","stream_id":"ctx_event_stream","sequence":{},"event_id":{},"event_type":{},"subject_id":{},"tenant_id":{},"topic":{},"websocket_fanout_allowed":true,"production_delivery_allowed":false,"provider_calls_allowed":false}}"#,
        event.sequence,
        json_string_literal(&event.event_id),
        json_string_literal(&event.event_type),
        json_string_literal(&event.subject_id),
        json_string_literal(&event.tenant_id),
        json_string_literal(&event.topic)
    )
}

fn ctx_event_topic(tenant_id: &str) -> String {
    format!("mdx:ctx:{tenant_id}:events")
}

fn recall_score(query: &str, memory: &OwnedLocalMemory) -> usize {
    let content = memory.content.to_ascii_lowercase();
    let terms = query
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|term| term.len() > 2)
        .collect::<Vec<_>>();
    let hits = terms
        .iter()
        .filter(|term| content.contains(&term.to_ascii_lowercase()))
        .count();
    let lexical = if terms.is_empty() {
        0
    } else {
        (hits * 100 / terms.len()).min(100)
    };
    let base = lexical * 7 + memory.importance * 3 + memory.access_count.min(5) * 10;
    base.saturating_mul(memory.edge_weight_percent) / 100
}

fn optional_owned_string(value: Option<&str>) -> String {
    value
        .map(json_string_literal)
        .unwrap_or_else(|| "null".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_embedding_matches_provider_ready_dimensions() {
        let embedding = deterministic_embedding("forge runtime grounded memory");
        assert_eq!(embedding.len(), CTX_VECTOR_DIMENSIONS);
        let literal = vector_literal(&embedding);
        assert!(literal.starts_with('['));
        assert!(literal.ends_with(']'));
        assert_eq!(literal.split(',').count(), CTX_VECTOR_DIMENSIONS);
    }

    #[test]
    fn vector_schema_uses_hnsw_cosine_and_provider_ready_dimensions() {
        let schema = ctx_vector_schema_sql();
        assert!(schema.contains("CREATE EXTENSION IF NOT EXISTS vector"));
        assert!(schema.contains("embedding VECTOR(1536) NOT NULL"));
        assert!(schema.contains("USING hnsw (embedding vector_cosine_ops)"));
        assert!(schema.contains("ENABLE ROW LEVEL SECURITY"));
        assert!(schema.contains("current_setting('mdx.tenant_id', true)"));
    }
}
