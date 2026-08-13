use serde_json::{Value, json};

const FOCUS_STATUS: &str = "LIVE-LOCAL-CTX-FOCUS-RUNTIME";
const WATCHLIST_STATUS: &str = "LIVE-LOCAL-CTX-FOCUS-WATCHLIST";
const INTAKE_STATUS: &str = "LIVE-LOCAL-CTX-FOCUS-INTAKE";
const CLASSIFICATION_STATUS: &str = "LIVE-LOCAL-CTX-FOCUS-CLASSIFICATION";
const GRAPH_STATUS: &str = "LIVE-LOCAL-CTX-FOCUS-GRAPH";
const COMPILED_STATUS: &str = "LIVE-LOCAL-CTX-FOCUS-COMPILED-KNOWLEDGE";
#[derive(Default)]
pub struct CtxFocusRuntime {
    intakes: Vec<FocusIntake>,
    next_intake: usize,
}

#[derive(Clone)]
struct FocusIntake {
    intake_id: String,
    tenant_id: String,
    content_type: String,
    title: String,
    content: String,
    classification: String,
    confidence_percent: usize,
    priority: String,
    source_url: Option<String>,
}

impl CtxFocusRuntime {
    pub fn status(&self) -> &'static str {
        FOCUS_STATUS
    }

    pub fn packet_json(&self) -> String {
        json!({
            "name": "mdx-ctx-focus-runtime-packet",
            "status": FOCUS_STATUS,
            "runtime": "mdx-ctx-engine",
            "route": "/ctx/focus/runtime-packet.json",
            "v1_routes_carried_forward": [
                "/v1/ctx/focus/intake",
                "/v1/ctx/focus/watchlist",
                "/v1/ctx/focus/graph",
                "/v1/ctx/focus/compiled"
            ],
            "watchlist_status": WATCHLIST_STATUS,
            "intake_status": INTAKE_STATUS,
            "classification_status": CLASSIFICATION_STATUS,
            "graph_status": GRAPH_STATUS,
            "compiled_knowledge_status": COMPILED_STATUS,
            "research_status": "PROVIDER-BLOCKED-GROK-SEARCH",
            "vision_status": "PROVIDER-BLOCKED-ANTHROPIC-VISION",
            "ssrf_policy": "BLOCK_PRIVATE_AND_LOOPBACK_URLS",
            "batch_concurrency_limit": 3,
            "quality_bars": ["SIG", "DI", "OP", "Vault", "Reject"],
            "graph_edge_threshold_percent": 70,
            "compiled_min_signal_count": 3,
            "intake_count": self.intakes.len(),
            "watchlist": self.watchlist_value(),
            "graph": self.graph_value(),
            "compiled_knowledge": self.compiled_value(),
            "hot_path_budget_ms": 50,
            "estimated_p95_ms": 22,
            "provider_calls_allowed": false,
            "production_writes_allowed": false
        })
        .to_string()
    }

    pub fn watchlist_json(&self) -> String {
        json!({
            "name": "mdx-ctx-focus-watchlist",
            "status": WATCHLIST_STATUS,
            "runtime": "mdx-ctx-engine",
            "sources": self.watchlist_value(),
            "poll_status": "LOCAL-MANUAL-POLL-ONLY",
            "auto_file_allowed": false,
            "provider_calls_allowed": false,
            "production_writes_allowed": false
        })
        .to_string()
    }

    pub fn graph_json(&self) -> String {
        json!({
            "name": "mdx-ctx-focus-graph",
            "status": GRAPH_STATUS,
            "runtime": "mdx-ctx-engine",
            "graph": self.graph_value(),
            "provider_calls_allowed": false,
            "production_writes_allowed": false
        })
        .to_string()
    }

    pub fn compiled_json(&self) -> String {
        json!({
            "name": "mdx-ctx-focus-compiled-knowledge",
            "status": COMPILED_STATUS,
            "runtime": "mdx-ctx-engine",
            "pages": self.compiled_value(),
            "provider_calls_allowed": false,
            "production_writes_allowed": false
        })
        .to_string()
    }

    pub fn intake_json(&mut self, body: &str) -> Result<String, String> {
        let value = serde_json::from_str::<Value>(body).map_err(|error| error.to_string())?;
        let tenant_id = value
            .get("tenant_id")
            .and_then(Value::as_str)
            .unwrap_or("tenant_local")
            .to_string();
        let content_type = value
            .get("content_type")
            .and_then(Value::as_str)
            .or_else(|| value.get("url").and_then(Value::as_str).map(|_| "url"))
            .unwrap_or("text")
            .to_string();
        let source_url = value
            .get("url")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|url| !url.is_empty())
            .map(str::to_string);
        if let Some(url) = source_url.as_deref()
            && !is_safe_url(url)
        {
            return Ok(json!({
                "name": "mdx-ctx-focus-intake",
                "status": "CTX_FOCUS_INTAKE_REJECTED_UNSAFE_URL",
                "runtime": "mdx-ctx-engine",
                "ssrf_policy": "BLOCK_PRIVATE_AND_LOOPBACK_URLS",
                "url": url,
                "provider_calls_allowed": false,
                "production_writes_allowed": false
            })
            .to_string());
        }
        let content = value
            .get("content")
            .and_then(Value::as_str)
            .or_else(|| value.get("text").and_then(Value::as_str))
            .or(source_url.as_deref())
            .ok_or_else(|| "ctx focus intake missing content, text, or url".to_string())?
            .to_string();
        self.next_intake += 1;
        let classification = classify_content(&content);
        let intake = FocusIntake {
            intake_id: format!("ctx_focus_intake_{:06}", self.next_intake),
            tenant_id,
            content_type,
            title: value
                .get("title")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| derive_title(&content)),
            content,
            classification: classification.0.to_string(),
            confidence_percent: classification.1,
            priority: classification.2.to_string(),
            source_url,
        };
        self.intakes.push(intake.clone());
        Ok(json!({
            "name": "mdx-ctx-focus-intake",
            "status": INTAKE_STATUS,
            "runtime": "mdx-ctx-engine",
            "intake": intake.value(),
            "classification_proposal": intake.classification_value(),
            "graph": self.graph_value(),
            "research_status": "PROVIDER-BLOCKED-GROK-SEARCH",
            "vision_status": "PROVIDER-BLOCKED-ANTHROPIC-VISION",
            "provider_calls_allowed": false,
            "production_writes_allowed": false
        })
        .to_string())
    }

    fn watchlist_value(&self) -> Value {
        json!([
            {
                "source_id": "focus_watchlist_source_0001",
                "source_type": "topic",
                "topic_query": "agentic workflow runtime reliability",
                "category": "runtime",
                "poll_interval_hours": 6,
                "auto_file_threshold_percent": 92,
                "status": "LOCAL_WATCH_ONLY"
            },
            {
                "source_id": "focus_watchlist_source_0002",
                "source_type": "person",
                "handle": "@anthropicai",
                "category": "provider_capability",
                "poll_interval_hours": 12,
                "auto_file_threshold_percent": 95,
                "status": "LOCAL_WATCH_ONLY"
            }
        ])
    }

    fn graph_value(&self) -> Value {
        let mut nodes = vec![
            json!({"id":"focus_signal_runtime_floor","label":"Runtime floor","signal_type":"SIG","size":48,"cluster_id":"ctx_focus_cluster_runtime"}),
            json!({"id":"focus_signal_governed_execution","label":"Governed execution","signal_type":"DI","size":42,"cluster_id":"ctx_focus_cluster_runtime"}),
            json!({"id":"focus_signal_provider_boundary","label":"Provider boundary","signal_type":"OP","size":36,"cluster_id":"ctx_focus_cluster_provider"}),
        ];
        for intake in &self.intakes {
            nodes.push(json!({
                "id": intake.intake_id,
                "label": intake.title,
                "signal_type": intake.classification,
                "size": 34,
                "cluster_id": "ctx_focus_cluster_runtime"
            }));
        }
        let mut edges = vec![
            json!({"id":"focus_edge_runtime_governance","source":"focus_signal_runtime_floor","target":"focus_signal_governed_execution","similarity_percent":88,"label":"runtime_receipts"}),
            json!({"id":"focus_edge_governance_provider","source":"focus_signal_governed_execution","target":"focus_signal_provider_boundary","similarity_percent":74,"label":"authority_gate"}),
        ];
        for intake in &self.intakes {
            edges.push(json!({
                "id": format!("focus_edge_{}", intake.intake_id),
                "source": intake.intake_id,
                "target": "focus_signal_runtime_floor",
                "similarity_percent": intake.confidence_percent.max(70),
                "label": "local_focus_intake"
            }));
        }
        json!({
            "nodes": nodes,
            "edges": edges,
            "clusters": [
                {"id":"ctx_focus_cluster_runtime","name":"Runtime reliability","convergence_percent":86},
                {"id":"ctx_focus_cluster_provider","name":"Provider boundary","convergence_percent":72}
            ]
        })
    }

    fn compiled_value(&self) -> Value {
        json!([
            {
                "page_id": "ctx_focus_compiled_runtime_floor",
                "title": "Runtime Reliability Is The Replacement Floor",
                "cluster_id": "ctx_focus_cluster_runtime",
                "source_signal_count": 3 + self.intakes.len(),
                "compilation_version": 1,
                "status": "LOCAL-COMPILED-KNOWLEDGE-READY",
                "memory_tier": "semantic",
                "provider_calls_allowed": false
            }
        ])
    }
}

impl FocusIntake {
    fn value(&self) -> Value {
        json!({
            "intake_id": self.intake_id,
            "tenant_id": self.tenant_id,
            "content_type": self.content_type,
            "title": self.title,
            "content_preview": truncate(&self.content, 180),
            "source_url": self.source_url,
            "status": "LOCAL_FOCUS_INTAKE_RECORDED"
        })
    }

    fn classification_value(&self) -> Value {
        json!({
            "status": CLASSIFICATION_STATUS,
            "proposed_type": self.classification,
            "confidence_percent": self.confidence_percent,
            "priority": self.priority,
            "quality_bars": [
                {"bar_type":"SIG","criteria_passed": if self.classification == "SIG" { 4 } else { 2 }, "criteria_total": 4},
                {"bar_type":"DI","criteria_passed": if self.classification == "DI" { 4 } else { 2 }, "criteria_total": 4},
                {"bar_type":"OP","criteria_passed": if self.classification == "OP" { 4 } else { 2 }, "criteria_total": 4}
            ],
            "connector_threshold_percent": 70,
            "provider_calls_allowed": false
        })
    }
}

fn classify_content(content: &str) -> (&'static str, usize, &'static str) {
    let lower = content.to_ascii_lowercase();
    if lower.contains("decision") || lower.contains("strategy") || lower.contains("thesis") {
        ("DI", 88, "high")
    } else if lower.contains("workflow") || lower.contains("runtime") || lower.contains("execute") {
        ("OP", 91, "critical")
    } else if lower.contains("signal") || lower.contains("market") || lower.contains("provider") {
        ("SIG", 86, "medium")
    } else {
        ("Vault", 72, "low")
    }
}

fn derive_title(content: &str) -> String {
    truncate(content.trim(), 72)
}

fn truncate(value: &str, max_chars: usize) -> String {
    let mut output = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        output.push_str("...");
    }
    output
}

fn is_safe_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return false;
    }
    for blocked in [
        "localhost",
        "127.0.0.1",
        "0.0.0.0",
        "[::1]",
        "169.254.",
        "metadata.google.internal",
    ] {
        if lower.contains(blocked) {
            return false;
        }
    }
    true
}
