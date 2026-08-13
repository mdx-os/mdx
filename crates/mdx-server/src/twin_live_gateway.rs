// The live model gateway for Twin answers: the serve-layer half of the
// provider-connected-local contract. The kernel never opens the network;
// this module does, and only when every gate holds:
//   1. A supported provider is selected: MDX_DEFAULT_MODEL_PROVIDER may
//      override, otherwise the latest accepted GUI turn-on observation wins.
//   2. The provider's API key is present in the process environment
//      (presence is read, the value is never logged or recorded).
//   3. The kernel holds an accepted provider turn-on observation for that
//      provider (the human-approved, receipt-backed ceremony ran).
// Any gate failing - or the call itself failing - returns None and the
// answer falls back to the deterministic gateway, honestly labeled.
use mdx_core::{
    MdxKernel, MemoryRecallCandidate, StorageProvider, TwinLiveAnswer, TwinRecallPacket,
};

pub(crate) const LIVE_ANSWER_MAX_TOKENS: u32 = 700;
// Tuned for a reasoning model (grok-4.5 and the Opus/GPT tier): they can think
// for a while before the response is readable, so a chat-model-era 45s read
// timeout cut a valid reply off and fell back to deterministic. Forge's provider
// path already allows 240s; Twin's interactive call gets a generous 180s.
const LIVE_CALL_TIMEOUT_SECS: u64 = 180;

// The live prompt's memory budget: 1200 bytes of ranked private memory plus
// 1200 bytes of active lesson summaries - the same 2400-byte ceiling as the
// Forge advisory block. Measured bytes land on the brain_recall receipt.
pub(crate) const MEMORY_LAYER_MAX_BYTES: usize = 1200;
pub(crate) const LESSON_LAYER_MAX_BYTES: usize = 1200;
const MEMORY_RECALL_TOP_K: usize = 5;
const MEMORY_SNIPPET_MAX_CHARS: usize = 200;

// The data-sensitivity of a question, which decides the model class that
// may answer it. The middle tier (Internal) is a deployment knob: today
// frontier is allowed; a conservative deployment can tighten it to require
// sovereign without touching call sites.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Sensitivity {
    Open,
    Internal,
    Sensitive,
}

impl Sensitivity {
    fn from_label(label: &str) -> Self {
        match label.trim().to_ascii_lowercase().as_str() {
            "open" | "tier0" | "0" => Sensitivity::Open,
            "sensitive" | "sovereign" | "tier2" | "2" => Sensitivity::Sensitive,
            // Default and explicit "internal" both land here: company work
            // that is not regulated. Unknown labels fail safe to Internal,
            // never to Open.
            _ => Sensitivity::Internal,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Sensitivity::Open => "open",
            Sensitivity::Internal => "internal",
            Sensitivity::Sensitive => "sensitive",
        }
    }
}

// Truncate to a char boundary at or below max_bytes: the layer caps are
// byte budgets, and a multibyte char on the boundary must not split.
fn cap_layer_bytes(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

/// The memory layer for a live Twin prompt, assembled read-only at prepare
/// time (the streaming route holds kernel.read() here by design): the top-k
/// consolidated ACTIVE private_user records ranked against this question,
/// plus the latest active lesson summaries - the same pattern Forge's
/// context assembly proved. Returns the packet (what actually rode, with
/// measured bytes and build time, for the truthful save-time receipt) and
/// the two prompt layers. The packet naturally sees only PRIOR
/// conversations: this draft has not been saved yet.
pub(crate) fn assemble_recall_context<S: StorageProvider>(
    kernel: &MdxKernel<S>,
    tenant_id: &str,
    draft_text: &str,
) -> (TwinRecallPacket, String, String) {
    let started = std::time::Instant::now();
    // Embed the draft with the boundary-locked local embedder so recall can
    // score real cosine similarity against records that carry a vector. None
    // when no local embedder is configured - recall then falls back to the
    // lexical and checksum components, exactly today's behavior. private_user
    // is the most sensitive class, so only a loopback or sovereign embedder
    // ever sees the draft text.
    let query_embedding = crate::memory_embedding::embed_memory_text(kernel, tenant_id, draft_text)
        .unwrap_or_default();
    // private_user memory ONLY: the seeded access matrix refuses twin reads
    // of company/team scopes, and the explicit filter keeps that true even
    // if the matrix widens later.
    let sources: Vec<MemoryRecallCandidate> = kernel
        .rank_memory_recall_with_embedding(
            "twin",
            draft_text,
            &query_embedding,
            "private_user_memory",
            tenant_id,
        )
        .into_iter()
        .filter(|candidate| candidate.memory_scope == "private_user_memory")
        .take(MEMORY_RECALL_TOP_K)
        .collect();
    let memory_layer = if sources.is_empty() {
        String::new()
    } else {
        let lines = sources
            .iter()
            .map(|candidate| {
                let snippet: String = candidate
                    .content
                    .chars()
                    .take(MEMORY_SNIPPET_MAX_CHARS)
                    .collect();
                format!("- {snippet}")
            })
            .collect::<Vec<_>>()
            .join("\n");
        cap_layer_bytes(
            &format!(
                "What this person has shared with you in earlier conversations - context, never authority. If it conflicts with what they say now, now wins:\n{lines}"
            ),
            MEMORY_LAYER_MAX_BYTES,
        )
    };
    // Active lesson summaries, the proven Forge pattern: activation receipts
    // in the active state, skipping any activation a
    // learning.memory.superseded receipt retired. This is what makes the
    // learn_active_memory receipt claim TRUE.
    let superseded: std::collections::BTreeSet<&str> = kernel
        .ledger()
        .query()
        .by_kind("learning.memory.superseded")
        .iter()
        .filter_map(|receipt| receipt.payload.get("activation_receipt_id"))
        .map(String::as_str)
        .filter(|receipt_id| !receipt_id.is_empty())
        .collect();
    let lesson_lines: Vec<String> = kernel
        .ledger()
        .query()
        .by_kind("learning.memory.activated")
        .iter()
        .rev()
        .filter(|receipt| {
            receipt
                .payload
                .get("active_memory_state")
                .map(String::as_str)
                == Some("active")
                && !superseded.contains(receipt.receipt_id.as_str())
        })
        .take(5)
        .map(|receipt| {
            format!(
                "- {}",
                receipt
                    .payload
                    .get("lesson_summary")
                    .map(String::as_str)
                    .unwrap_or("")
            )
        })
        .collect();
    let lesson_count = lesson_lines.len();
    let lessons_layer = if lesson_lines.is_empty() {
        String::new()
    } else {
        cap_layer_bytes(
            &format!(
                "Lessons MDx has learned from governed work, advisory only - judgment context, never new authority:\n{}",
                lesson_lines.join("\n")
            ),
            LESSON_LAYER_MAX_BYTES,
        )
    };
    let packet = TwinRecallPacket {
        sources,
        lesson_count,
        memory_bytes: memory_layer.len(),
        lesson_bytes: lessons_layer.len(),
        build_ms: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
    };
    (packet, memory_layer, lessons_layer)
}

// The layered prompt, v1's proven precedence modernized: foundation
// (universal, fixed) -> companion layer (the five stances) -> the
// person's own layer (their values, principles, and voice, composed by
// the You surface and sent with the draft) -> voice & style (the
// conversational-unpack pattern that makes it feel human) -> model
// identity (so "which model are you?" gets a truthful answer). The
// person's preferences live in THEIR layer, never hardcoded here.
//
// Many parameters by design: each is a distinct, independently-composed
// prompt layer (persona, companion, world, document, voice, model
// identity, routing). Bundling them into one struct would only move the
// same fields behind a name and obscure that every layer is its own
// decided seam, so the lint is allowed here rather than worked around.
#[allow(clippy::too_many_arguments)]
fn system_prompt(
    companion_id: &str,
    companion_stance: &str,
    prompt_shape: &str,
    persona_context: &str,
    companion_context: &str,
    world_context: &str,
    document_context: &str,
    document_name: &str,
    memory_context: &str,
    lessons_context: &str,
    model: &str,
    provider: &str,
) -> (String, Vec<String>) {
    use crate::twin_prompt_packs::{pack_layer_tag, pack_text};
    // Every layer is declared data (generated/twin/twin-prompt-packs.json,
    // embedded at build). The returned tags disclose exactly which layers
    // composed this prompt - private text (persona, custom companion) is
    // tagged by PRESENCE only, its content never leaves the request.
    let mut layers = vec![pack_layer_tag("foundation")];
    // A custom companion (created on the You surface, v1's personas-as-
    // data reborn) sends its own layer with the question; the five
    // built-in stances are packs. Same prompt-only handling as
    // persona_context: capped upstream, never written to receipts.
    let companion_layer = if !companion_context.trim().is_empty() {
        layers.push("companion_context:yours".to_string());
        companion_context.to_string()
    } else {
        let pack_id = format!("companion:{companion_id}");
        let text = pack_text(&pack_id);
        if text.is_empty() {
            layers.push(pack_layer_tag("companion_fallback"));
            pack_text("companion_fallback").to_string()
        } else {
            layers.push(pack_layer_tag(&pack_id));
            text.to_string()
        }
    };
    let stance = pack_text("stance_and_shape")
        .replace("{companion_stance}", companion_stance)
        .replace("{prompt_shape}", prompt_shape);
    layers.push(pack_layer_tag("stance_and_shape"));
    let persona_layer = if persona_context.trim().is_empty() {
        String::new()
    } else {
        layers.push("persona_context:yours".to_string());
        format!(
            "\n\n{}",
            pack_text("persona_frame").replace("{persona_context}", persona_context)
        )
    };
    // The cross-MDx snapshot: what is moving across the other surfaces,
    // read from real kernel projections and relayed with the question (the
    // same prompt-only handling as persona/companion). It rides AFTER the
    // person's frame so company state never overrides who they are, and it
    // is the only company truth the model is given - the frame itself tells
    // it not to invent past these lines. Tagged by presence; the lines are
    // not written to receipts.
    let world_layer = if world_context.trim().is_empty() {
        String::new()
    } else {
        layers.push("world_context:live".to_string());
        format!(
            "\n\n{}",
            pack_text("world_frame").replace("{world_context}", world_context)
        )
    };
    // An attached document the person brought into the conversation. Its
    // text rides as its own layer so provenance keeps it distinct from the
    // company snapshot, and the sensitivity rail decides which model may
    // read it. Tagged by presence; the contents never reach a receipt.
    let document_layer = if document_context.trim().is_empty() {
        String::new()
    } else {
        layers.push("document_context:attached".to_string());
        format!(
            "\n\n{}",
            pack_text("document_frame")
                .replace("{document_name}", document_name)
                .replace("{document_context}", document_context)
        )
    };
    // The person's own memory, ranked from their prior consolidated
    // conversations at prepare time. Same prompt-only handling as
    // persona_context: budgeted upstream, tagged by PRESENCE only, its
    // content never written to receipts.
    let memory_layer = if memory_context.trim().is_empty() {
        String::new()
    } else {
        layers.push("memory_context:yours".to_string());
        format!("\n\n{memory_context}")
    };
    // Active lesson summaries from MDx's learning ledger - advisory
    // judgment context, the same block Forge runs with. Presence-tagged.
    let lessons_layer = if lessons_context.trim().is_empty() {
        String::new()
    } else {
        layers.push("lesson_context:active".to_string());
        format!("\n\n{lessons_context}")
    };
    layers.push(pack_layer_tag("voice_and_style"));
    let identity = pack_text("model_identity")
        .replace("{model}", model)
        .replace("{provider}", provider);
    layers.push(pack_layer_tag("model_identity"));
    let system = format!(
        "{foundation}\n\n## Your role\n{companion_layer}\n{stance}{persona_layer}{world_layer}{document_layer}{memory_layer}{lessons_layer}\n\n## {voice}\n\n{identity}",
        foundation = pack_text("foundation"),
        voice = pack_text("voice_and_style"),
    );
    (system, layers)
}

// Multi-turn memory: the prior live turns of this session, read from the
// kernel's own receipts - the saved conversation IS the history, no
// shadow state. Only live Q/A pairs ride (a deterministic stub answer
// would teach the model to sound like a receipt). Bounded so the context
// stays sane on long sessions.
const HISTORY_MAX_TURNS: usize = 12;
const HISTORY_ANSWER_CAP_CHARS: usize = 2000;

fn conversation_history<S: StorageProvider>(
    kernel: &MdxKernel<S>,
    session_id: &str,
) -> Vec<(String, String)> {
    if session_id.trim().is_empty() {
        return Vec::new();
    }
    let entries = kernel.ledger().entries();
    let mut pairs: Vec<(String, String)> = Vec::new();
    for answer in entries.iter().filter(|receipt| {
        receipt.kind == "twin.session.answer.grounded"
            && receipt.payload.get("session_id").map(String::as_str) == Some(session_id)
            && receipt
                .payload
                .get("live_provider_used")
                .map(String::as_str)
                == Some("true")
    }) {
        let Some(draft_id) = answer.payload.get("source_draft_receipt_id") else {
            continue;
        };
        let Some(draft) = entries.iter().find(|entry| &entry.receipt_id == draft_id) else {
            continue;
        };
        let question = draft.payload.get("draft_text").cloned().unwrap_or_default();
        let reply: String = answer
            .payload
            .get("grounded_answer")
            .map(String::as_str)
            .unwrap_or_default()
            .chars()
            .take(HISTORY_ANSWER_CAP_CHARS)
            .collect();
        if !question.is_empty() && !reply.is_empty() {
            pairs.push((question, reply));
        }
    }
    let skip = pairs.len().saturating_sub(HISTORY_MAX_TURNS);
    pairs.split_off(skip)
}

pub(crate) struct LiveAnswerRequest<'a> {
    pub actor_id: &'a str,
    pub session_id: &'a str,
    pub companion_id: &'a str,
    pub companion_stance: &'a str,
    pub prompt_shape: &'a str,
    pub persona_context: &'a str,
    pub companion_context: &'a str,
    pub world_context: &'a str,
    // A document the person attached, relayed with the question for Twin to
    // reason over (text + filename). Prompt-only, capped upstream.
    pub document_context: &'a str,
    pub document_name: &'a str,
    // The data-sensitivity label for this question ("open"/"internal"/
    // "sensitive"), relayed with the request. Decides the model class.
    pub sensitivity_tier: &'a str,
    // The tenant whose memory this call may read, and the question itself:
    // together they drive the read-only recall ranking that assembles the
    // budgeted memory layer at prepare time.
    pub tenant_id: &'a str,
    pub draft_text: &'a str,
}

// Everything a live call needs, gathered behind the SAME three gates the
// blocking path has always enforced (named provider + key present +
// accepted turn-on observation). The streaming route shares this - one
// gate, two transports.
pub(crate) struct PreparedLiveCall {
    pub provider: &'static str,
    pub adapter: &'static str,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub system: String,
    pub prompt_layers: Vec<String>,
    pub history: Vec<(String, String)>,
    // The residency decision for this call, recorded on the answer receipt
    // so a person can prove what saw their data: the sensitivity tier and
    // the privacy class of the provider that actually answered.
    pub privacy_tier: &'static str,
    pub privacy_class: String,
    pub decision_receipt_id: String,
    pub workload_id: String,
    pub tenant_id: String,
    pub app_id: String,
    pub environment: String,
    pub session_id: String,
    pub deployment_id: String,
    pub input_price_microusd_per_million: u64,
    pub output_price_microusd_per_million: u64,
    pub fallbacks: Vec<PreparedFallback>,
    // The memory packet actually assembled into this prompt. Threaded into
    // the save so the brain_recall receipt records observed values.
    pub recall: TwinRecallPacket,
}

pub(crate) struct PreparedFallback {
    pub provider: &'static str,
    pub adapter: &'static str,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub privacy_class: String,
    pub deployment_id: String,
    pub input_price_microusd_per_million: u64,
    pub output_price_microusd_per_million: u64,
}

impl PreparedLiveCall {
    // The routing label that lands in model_gateway_routing on the receipt -
    // human and parseable, so "where did this conversation's data go" is one
    // read away, not an inference.
    pub fn routing_label(&self) -> String {
        format!(
            "tier:{} class:{} provider:{} decision:{}",
            self.privacy_tier, self.privacy_class, self.provider, self.decision_receipt_id
        )
    }
}

pub(crate) fn prepare_live_call(
    kernel: &mut MdxKernel,
    request: &LiveAnswerRequest<'_>,
) -> Option<PreparedLiveCall> {
    // The residency rail runs FIRST, before the connection gates. The
    // sensitivity tier decides the model class that may answer: a Sensitive
    // (Tier 2) question may only go to a sovereign-class slot, never to the
    // default frontier provider. If no slot satisfies the tier - which today
    // is every Sensitive question, since the sovereign slot has no key wired -
    // we return None and the caller stays deterministic. Sensitive data is
    // never silently sent to a frontier model: fail closed by construction.
    let sensitivity = Sensitivity::from_label(request.sensitivity_tier);
    let tenant_id = request.tenant_id;
    let workload_id = if sensitivity == Sensitivity::Sensitive {
        "mdx/twin/private-reasoning"
    } else {
        "mdx/twin/conversation"
    };
    let selected = crate::model_fabric_service::select(
        kernel,
        &crate::model_fabric_service::ModelSelectionRequest {
            tenant_id,
            actor_id: request.actor_id,
            workload_id,
            preset: if sensitivity == Sensitivity::Sensitive {
                "private"
            } else {
                ""
            },
            policy_id: "auto",
            expected_policy_version: None,
            required_region: None,
            required_residency: None,
            session_id: Some(request.session_id),
            pinned_deployment_id: None,
            required_context_tokens: 0,
            max_input_cost_microusd_per_million: 0,
            max_output_cost_microusd_per_million: 0,
        },
    )
    .ok()?;
    if selected.decision.status != "ROUTE_SELECTED" {
        return None;
    }
    let fallbacks = selected
        .fallbacks
        .into_iter()
        .filter_map(|target| {
            let provider = mdx_core::model_provider(&target.endpoint.connection.provider_id)?;
            Some(PreparedFallback {
                provider: provider.provider_id,
                adapter: provider.adapter,
                base_url: target.endpoint.connection.endpoint_base_url,
                api_key: target.api_key,
                model: target.endpoint.model.provider_model_id,
                privacy_class: target.endpoint.deployment.privacy_class,
                deployment_id: target.endpoint.deployment.deployment_id,
                input_price_microusd_per_million: target.endpoint.price.input_microusd_per_million,
                output_price_microusd_per_million: target
                    .endpoint
                    .price
                    .output_microusd_per_million,
            })
        })
        .collect();
    let endpoint = selected.endpoint?;
    let provider = mdx_core::model_provider(&endpoint.connection.provider_id)?;
    let api_key = selected.api_key?;
    let model = endpoint.model.provider_model_id;
    // The memory layer, assembled read-only at prepare time so the
    // streaming route never needs a write lock before the provider call.
    let (recall, memory_layer, lessons_layer) =
        assemble_recall_context(kernel, request.tenant_id, request.draft_text);
    let (system, prompt_layers) = system_prompt(
        request.companion_id,
        request.companion_stance,
        request.prompt_shape,
        request.persona_context,
        request.companion_context,
        request.world_context,
        request.document_context,
        request.document_name,
        &memory_layer,
        &lessons_layer,
        &model,
        provider.provider_id,
    );
    Some(PreparedLiveCall {
        provider: provider.provider_id,
        adapter: provider.adapter,
        base_url: endpoint.connection.endpoint_base_url,
        api_key,
        model,
        system,
        prompt_layers,
        history: conversation_history(kernel, request.session_id),
        privacy_tier: sensitivity.as_str(),
        privacy_class: endpoint.deployment.privacy_class,
        decision_receipt_id: selected.decision_receipt_id,
        workload_id: selected.decision.workload_id,
        tenant_id: tenant_id.to_string(),
        app_id: selected.decision.app_id,
        environment: selected.decision.environment,
        session_id: selected.decision.session_id,
        deployment_id: endpoint.deployment.deployment_id,
        input_price_microusd_per_million: endpoint.price.input_microusd_per_million,
        output_price_microusd_per_million: endpoint.price.output_microusd_per_million,
        fallbacks,
        recall,
    })
}

// The first governed skills: tools the model may PROPOSE, never execute.
// Nothing runs server-side - the person approves on the surface and the
// EXISTING governed write rails do the work (least privilege by
// construction: the model holds no authority, only the ability to ask).
// v1's decision_capture and approval_workflow, reborn on kernel rails.
const SKILL_DEFS: &[(&str, &str, &str)] = &[
    (
        "record_decision",
        "Offer to record a decision for the company record when a real decision with reasoning has emerged from the conversation. The person approves before anything is saved. Always also answer normally in text.",
        r#"{"type":"object","properties":{"title":{"type":"string","description":"the decision in one short line"},"decision":{"type":"string","description":"what was decided, 1-3 sentences"},"reasoning":{"type":"string","description":"why - the tradeoffs that mattered"}},"required":["title","decision"]}"#,
    ),
    (
        "share_with_team",
        "Offer to post a short summary of this thinking to the team room when the person seems ready to socialize it. The person approves before anything is posted. Always also answer normally in text.",
        r#"{"type":"object","properties":{"summary":{"type":"string","description":"the message to the team, in the person's voice, under 600 characters"}},"required":["summary"]}"#,
    ),
    (
        "suggest_follow_ups",
        "After answering, offer up to three natural next questions the person might actually ask - short, specific to this conversation, in their casual register. Presentation only; nothing is saved.",
        r#"{"type":"object","properties":{"questions":{"type":"array","items":{"type":"string"},"description":"up to three short follow-up questions"}},"required":["questions"]}"#,
    ),
    (
        "draft_document",
        "Offer to draft a document - a spec, one-pager, plan, summary, or note - when the person asks you to write, draft, or put something together as a document. Give a clear title and the full body in markdown. The person approves before it is saved to Pages, and can also just download it. Always also answer normally in text.",
        r#"{"type":"object","properties":{"title":{"type":"string","description":"a clear document title, one line"},"body":{"type":"string","description":"the full document in markdown - headings, lists, and prose as the content needs"}},"required":["title","body"]}"#,
    ),
];

fn skill_schema(schema: &str) -> serde_json::Value {
    serde_json::from_str::<serde_json::Value>(schema).unwrap_or_else(|_| {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    })
}

pub(crate) fn openai_tools(capabilities: &crate::twin_guards::Capabilities) -> serde_json::Value {
    SKILL_DEFS
        .iter()
        .filter(|(name, _, _)| capabilities.skill_enabled(name))
        .map(|(name, description, schema)| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": name,
                    "description": description,
                    "parameters": skill_schema(schema),
                }
            })
        })
        .collect()
}

pub(crate) fn anthropic_tools(
    capabilities: &crate::twin_guards::Capabilities,
) -> serde_json::Value {
    SKILL_DEFS
        .iter()
        .filter(|(name, _, _)| capabilities.skill_enabled(name))
        .map(|(name, description, schema)| {
            serde_json::json!({
                "name": name,
                "description": description,
                "input_schema": skill_schema(schema),
            })
        })
        .collect()
}

pub(crate) fn known_skill(name: &str) -> bool {
    SKILL_DEFS.iter().any(|(skill, _, _)| *skill == name)
}

/// Execute a prepared live call with no kernel access at all. The serving
/// path prepares under the kernel lock, releases it, and calls here - one
/// slow provider answer must never freeze every other surface.
pub(crate) struct TwinLiveExecution {
    pub answer: TwinLiveAnswer,
    pub outcome: mdx_core::ModelOutcome,
}

/// A live provider can technically answer while still failing the operator's
/// job. Retry only when the ask clearly requests a grounded, substantive
/// read and the answer is a generic handoff or too short to contain it.
/// Greetings and ordinary short questions remain short.
pub(crate) fn answer_needs_substantive_retry(draft_text: &str, answer_text: &str) -> bool {
    let ask = draft_text.trim().to_ascii_lowercase();
    let substantive_ask = ask.split_whitespace().count() >= 8
        && [
            "current workspace",
            "current work",
            "what needs",
            "top three",
            "identify",
            "recommend",
            "compare",
            "summarize",
            "be specific",
        ]
        .iter()
        .any(|marker| ask.contains(marker));
    if !substantive_ask {
        return false;
    }
    let answer = answer_text.trim().to_ascii_lowercase();
    answer.split_whitespace().count() < 12
        || answer.chars().count() < 80
        || answer == "here you go"
        || answer.starts_with("here you go -")
        || answer.starts_with("your call below")
}

pub(crate) fn substantive_retry_prompt(draft_text: &str) -> String {
    format!(
        "{draft_text}\n\nRetry requirement: answer the request itself now. Use the provided workspace snapshot, name the concrete apps or work items involved, and give the requested safe next move. Do not return a placeholder, preamble, or tool-only handoff."
    )
}

pub(crate) fn execute_prepared(
    call: &PreparedLiveCall,
    draft_text: &str,
) -> Option<TwinLiveExecution> {
    let mut messages = Vec::with_capacity(call.history.len() * 2 + 1);
    for (question, answer) in &call.history {
        messages.push(crate::model_gateway_runtime::GatewayMessage {
            role: "user".to_string(),
            content: question.clone(),
        });
        messages.push(crate::model_gateway_runtime::GatewayMessage {
            role: "assistant".to_string(),
            content: answer.clone(),
        });
    }
    messages.push(crate::model_gateway_runtime::GatewayMessage {
        role: "user".to_string(),
        content: draft_text.to_string(),
    });
    struct Attempt<'a> {
        provider: &'a str,
        adapter: &'a str,
        base_url: &'a str,
        api_key: &'a str,
        model: &'a str,
        privacy_class: &'a str,
        deployment_id: &'a str,
        input_price: u64,
        output_price: u64,
    }
    let mut attempts = vec![Attempt {
        provider: call.provider,
        adapter: call.adapter,
        base_url: &call.base_url,
        api_key: &call.api_key,
        model: &call.model,
        privacy_class: &call.privacy_class,
        deployment_id: &call.deployment_id,
        input_price: call.input_price_microusd_per_million,
        output_price: call.output_price_microusd_per_million,
    }];
    attempts.extend(call.fallbacks.iter().map(|fallback| Attempt {
        provider: fallback.provider,
        adapter: fallback.adapter,
        base_url: &fallback.base_url,
        api_key: &fallback.api_key,
        model: &fallback.model,
        privacy_class: &fallback.privacy_class,
        deployment_id: &fallback.deployment_id,
        input_price: fallback.input_price_microusd_per_million,
        output_price: fallback.output_price_microusd_per_million,
    }));
    for (index, attempt) in attempts.into_iter().enumerate() {
        let Some(protocol) = crate::model_gateway_runtime::protocol_for_provider(attempt.provider)
        else {
            let _ = crate::model_fabric_registry::global().report_failed_attempt(
                &call.tenant_id,
                crate::model_fabric_registry::FailedInferenceAttempt {
                    decision_id: call.decision_receipt_id.clone(),
                    workload_id: call.workload_id.clone(),
                    app_id: call.app_id.clone(),
                    environment: call.environment.clone(),
                    session_id: call.session_id.clone(),
                    deployment_id: attempt.deployment_id.to_string(),
                    latency_ms: 0,
                    failure_class: "adapter_unavailable".to_string(),
                    provenance: "twin_runtime_failure".to_string(),
                },
            );
            continue;
        };
        let attempt_started = std::time::Instant::now();
        let request = |messages: Vec<crate::model_gateway_runtime::GatewayMessage>| {
            crate::model_gateway_runtime::execute(
                &crate::model_gateway_runtime::GatewayCallRequest {
                    provider_id: attempt.provider.to_string(),
                    adapter_id: attempt.adapter.to_string(),
                    protocol,
                    base_url: attempt.base_url.to_string(),
                    api_key: attempt.api_key.to_string(),
                    model_id: attempt.model.to_string(),
                    system: call.system.clone(),
                    messages,
                    tools: Vec::new(),
                    max_output_tokens: LIVE_ANSWER_MAX_TOKENS,
                    timeout: std::time::Duration::from_secs(LIVE_CALL_TIMEOUT_SECS),
                    session_id: None,
                },
            )
        };
        let mut result = request(messages.clone());
        if let Ok(first) = &result
            && answer_needs_substantive_retry(draft_text, &first.text)
        {
            let mut retry_messages = messages.clone();
            if let Some(last) = retry_messages.last_mut() {
                last.content = substantive_retry_prompt(draft_text);
            }
            result = request(retry_messages);
        }
        match result {
            Ok(result) => {
                let _ = crate::model_fabric_registry::global().report_execution(
                    &call.tenant_id,
                    attempt.deployment_id,
                    true,
                );
                let cost_microusd = estimated_cost_microusd(
                    result.input_tokens,
                    result.output_tokens,
                    attempt.input_price,
                    attempt.output_price,
                );
                return Some(TwinLiveExecution {
                    answer: TwinLiveAnswer {
                        answer_text: result.text,
                        provider: attempt.provider.to_string(),
                        adapter: attempt.adapter.to_string(),
                        model_id: result.model_id,
                        inference_id: result.inference_id.clone(),
                        routing: format!(
                            "tier:{} class:{} provider:{} decision:{} failover_index:{}",
                            call.privacy_tier,
                            attempt.privacy_class,
                            attempt.provider,
                            call.decision_receipt_id,
                            index
                        ),
                    },
                    outcome: mdx_core::ModelOutcome {
                        decision_id: call.decision_receipt_id.clone(),
                        workload_id: call.workload_id.clone(),
                        app_id: call.app_id.clone(),
                        environment: call.environment.clone(),
                        session_id: call.session_id.clone(),
                        deployment_id: attempt.deployment_id.to_string(),
                        latency_ms: result.latency_ms,
                        cost_microusd: Some(cost_microusd),
                        quality_score: None,
                        safety_status: "not_evaluated".to_string(),
                        task_status: "not_evaluated".to_string(),
                        correction_status: "not_observed".to_string(),
                        provenance: format!("provider_runtime:{}", result.inference_id),
                    },
                });
            }
            Err(error) => {
                let _ = crate::model_fabric_registry::global().report_failed_attempt(
                    &call.tenant_id,
                    crate::model_fabric_registry::FailedInferenceAttempt {
                        decision_id: call.decision_receipt_id.clone(),
                        workload_id: call.workload_id.clone(),
                        app_id: call.app_id.clone(),
                        environment: call.environment.clone(),
                        session_id: call.session_id.clone(),
                        deployment_id: attempt.deployment_id.to_string(),
                        latency_ms: attempt_started
                            .elapsed()
                            .as_millis()
                            .min(u128::from(u64::MAX)) as u64,
                        failure_class: crate::model_fabric_service::inference_failure_class(&error)
                            .to_string(),
                        provenance: "twin_runtime_failure".to_string(),
                    },
                );
                let _ = crate::model_fabric_registry::global().report_execution(
                    &call.tenant_id,
                    attempt.deployment_id,
                    false,
                );
                eprintln!(
                    "twin_live_gateway: {} call failed ({error}); trying the next eligible route",
                    attempt.provider
                );
            }
        }
    }
    None
}

fn estimated_cost_microusd(
    input_tokens: u64,
    output_tokens: u64,
    input_price: u64,
    output_price: u64,
) -> u64 {
    let numerator = u128::from(input_tokens) * u128::from(input_price)
        + u128::from(output_tokens) * u128::from(output_price);
    numerator.div_ceil(1_000_000).min(u128::from(u64::MAX)) as u64
}

// True for xAI models that accept the reasoning_effort parameter (grok-4.5 and
// later). Older grok chat models reject the field, so Twin only sends it when
// the model id opts in - letting interactive chat request low effort for speed.
pub(crate) fn model_accepts_reasoning_effort(model: &str) -> bool {
    let m = model.to_ascii_lowercase();
    m.contains("grok-4.5") || m.contains("grok-4-5")
}

pub(crate) fn chat_messages(history: &[(String, String)], user: &str) -> Vec<serde_json::Value> {
    let mut messages = Vec::with_capacity(history.len() * 2 + 1);
    for (question, reply) in history {
        messages.push(serde_json::json!({ "role": "user", "content": question }));
        messages.push(serde_json::json!({ "role": "assistant", "content": reply }));
    }
    messages.push(serde_json::json!({ "role": "user", "content": user }));
    messages
}

#[cfg(test)]
mod tests {
    use super::system_prompt;

    #[test]
    fn custom_companion_layer_replaces_builtin_and_foundation_still_leads() {
        let (prompt, layers) = system_prompt(
            "custom_the_editor",
            "Your companion",
            "tighten this draft",
            "Name: Sam.",
            "You are The Editor, a companion this person created inside MDx.\nWhat you are for: cut every wasted word.",
            "Across MDx right now:\n- Forge: 66 builds run, the latest finished.",
            "Quarterly plan: ship the editor by March.",
            "plan.md",
            "What this person has shared with you in earlier conversations:\n- They prefer short memos.",
            "Lessons MDx has learned from governed work:\n- Screenshot proof is required for visual work.",
            "grok-4.3",
            "xai",
        );
        assert!(prompt.starts_with("You are part of MDx"));
        assert!(prompt.contains("You are The Editor"));
        assert!(prompt.contains("cut every wasted word"));
        assert!(!prompt.contains("grounded thinking partner"));
        assert!(prompt.contains("Name: Sam."));
        // The cross-MDx snapshot rides into the prompt when present.
        assert!(prompt.contains("66 builds run"));
        // Disclosure: presence-only tags for private layers, versioned
        // tags for packs, in composition order.
        assert_eq!(layers[0], "foundation@1");
        assert!(layers.contains(&"companion_context:yours".to_string()));
        assert!(layers.contains(&"persona_context:yours".to_string()));
        assert!(layers.contains(&"world_context:live".to_string()));
        assert!(layers.contains(&"voice_and_style@1".to_string()));
        // The attached document rides in as its own layer, filename and all.
        assert!(prompt.contains("ship the editor by March"));
        assert!(prompt.contains("plan.md"));
        assert!(layers.contains(&"document_context:attached".to_string()));
        // The memory and lesson layers ride in, disclosed by presence.
        assert!(prompt.contains("They prefer short memos"));
        assert!(prompt.contains("Screenshot proof is required"));
        assert!(layers.contains(&"memory_context:yours".to_string()));
        assert!(layers.contains(&"lesson_context:active".to_string()));
        // The private TEXT never appears in the tags themselves.
        assert!(layers.iter().all(|tag| !tag.contains("Sam")));
        // The snapshot and the document are disclosed by presence, not content.
        assert!(layers.iter().all(|tag| !tag.contains("builds run")));
        assert!(layers.iter().all(|tag| !tag.contains("ship the editor")));
        assert!(layers.iter().all(|tag| !tag.contains("short memos")));
    }

    #[test]
    fn builtin_companion_layer_holds_when_no_custom_context() {
        let (prompt, layers) = system_prompt(
            "twin_advisor",
            "Direct, options-first",
            "weigh a decision",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "grok-4.3",
            "xai",
        );
        assert!(prompt.contains("Strategic Advisor"));
        assert!(!prompt.contains("The person you are working with"));
        assert!(layers.contains(&"companion:twin_advisor@1".to_string()));
        assert!(!layers.iter().any(|tag| tag.starts_with("persona_context")));
        // No snapshot or document relayed, neither layer composed or disclosed.
        assert!(!layers.iter().any(|tag| tag.starts_with("world_context")));
        assert!(!layers.iter().any(|tag| tag.starts_with("document_context")));
        // No memory recalled and no lessons active: neither layer appears.
        assert!(!layers.iter().any(|tag| tag.starts_with("memory_context")));
        assert!(!layers.iter().any(|tag| tag.starts_with("lesson_context")));
    }

    #[test]
    fn sensitivity_labels_fail_safe_to_internal_never_open() {
        use super::Sensitivity;
        assert_eq!(Sensitivity::from_label("open").as_str(), "open");
        assert_eq!(Sensitivity::from_label("sensitive").as_str(), "sensitive");
        assert_eq!(Sensitivity::from_label("internal").as_str(), "internal");
        // Garbage and empty both land on internal, never open.
        assert_eq!(Sensitivity::from_label("whatever").as_str(), "internal");
        assert_eq!(Sensitivity::from_label("").as_str(), "internal");
    }

    #[test]
    fn history_returns_only_live_pairs_for_the_session_in_order() {
        let mut kernel = mdx_core::MdxKernel::boot_local();
        kernel.run_evals_runner_agent().expect("evals agent");
        let evidence = kernel
            .ledger()
            .entries()
            .iter()
            .find(|receipt| receipt.kind == "credential.minted")
            .map(|receipt| receipt.receipt_id.clone())
            .expect("credential evidence");
        let live = |text: &str| mdx_core::TwinLiveAnswer {
            answer_text: text.to_string(),
            provider: "xai".to_string(),
            adapter: "XaiChatModelGateway".to_string(),
            model_id: "grok-4.3".to_string(),
            inference_id: "inf_test".to_string(),
            routing: "tier:internal class:frontier provider:xai".to_string(),
        };
        let mut save = |session: &str, question: &str, answer: Option<mdx_core::TwinLiveAnswer>| {
            kernel
                .save_twin_session_draft_local(mdx_core::TwinSessionDraftRequest {
                    tenant_id: "tenant_local",
                    actor_id: "local_user",
                    session_id: session,
                    companion_id: "twin_advisor",
                    companion_stance: "Direct, options-first",
                    persona_profile_id: "test_persona",
                    prompt_shape: "test",
                    evidence_receipt_id: &evidence,
                    draft_text: question,
                    live_answer: answer.as_ref(),
                    recall_packet: None,
                })
                .expect("draft saved");
        };
        save("sess_a", "first question", Some(live("first answer")));
        save("sess_a", "stub question", None);
        save("sess_b", "other session", Some(live("other answer")));
        save("sess_a", "second question", Some(live("second answer")));
        let history = super::conversation_history(&kernel, "sess_a");
        assert_eq!(
            history,
            vec![
                ("first question".to_string(), "first answer".to_string()),
                ("second question".to_string(), "second answer".to_string()),
            ]
        );
        assert!(super::conversation_history(&kernel, "").is_empty());
    }

    #[test]
    fn recall_context_assembles_prior_private_memory_within_budget() {
        let mut kernel = mdx_core::MdxKernel::boot_local();
        kernel.run_evals_runner_agent().expect("evals agent");
        let source_receipt_id = kernel
            .ledger()
            .entries()
            .first()
            .expect("seed receipt")
            .receipt_id
            .clone();
        kernel
            .record_surface_memory_local(
                "tenant_local",
                "local_user",
                "twin",
                "private_user_memory",
                &source_receipt_id,
                "This person prefers concise migration memos with receipts",
            )
            .expect("first memory");
        // A record far past the snippet cap proves per-line truncation.
        let long_content = "budget ".repeat(400);
        let capped_memory_id = kernel
            .record_surface_memory_local(
                "tenant_local",
                "local_user",
                "twin",
                "private_user_memory",
                &source_receipt_id,
                &long_content,
            )
            .expect("long memory");
        // Another tenant's record must never ride this tenant's prompt.
        kernel
            .record_surface_memory_local(
                "tenant_other",
                "other_user",
                "twin",
                "private_user_memory",
                &source_receipt_id,
                "Another tenant's private preference",
            )
            .expect("other tenant memory");
        let (packet, memory_layer, lessons_layer) =
            super::assemble_recall_context(&kernel, "tenant_local", "concise migration memos");
        assert!(!packet.sources.is_empty());
        assert!(packet.sources.len() <= 5);
        assert!(memory_layer.contains("concise migration memos with receipts"));
        assert!(!memory_layer.contains("Another tenant's private preference"));
        // The hard byte cap holds and the packet records measured bytes.
        assert!(memory_layer.len() <= super::MEMORY_LAYER_MAX_BYTES);
        assert_eq!(packet.memory_bytes, memory_layer.len());
        // No lessons are active in a fresh kernel: the layer is absent and
        // the packet says zero, so learn_active_memory is never claimed.
        assert!(lessons_layer.is_empty());
        assert_eq!(packet.lesson_count, 0);
        assert_eq!(packet.lesson_bytes, 0);
        // A superseded record leaves the prompt on the next assembly.
        let correlation = mdx_core::CorrelationIds {
            tenant_id: mdx_core::TenantId::new("tenant_local"),
            trace_id: mdx_core::TraceId::new("trace_recall_test"),
            actor_id: mdx_core::ActorId::new("local_user"),
            loop_id: mdx_core::LoopId::new("recall_test"),
            workflow_id: mdx_core::WorkflowId::new("workflow_recall_test"),
        };
        kernel
            .apply_memory_lifecycle_action(
                &correlation,
                &capped_memory_id,
                "supersede",
                "recall test supersession",
            )
            .expect("supersede");
        let (packet, memory_layer, _) =
            super::assemble_recall_context(&kernel, "tenant_local", "concise migration memos");
        assert!(!memory_layer.contains("budget budget"));
        assert!(
            packet
                .sources
                .iter()
                .all(|source| source.memory_id != capped_memory_id)
        );
    }

    #[test]
    fn recall_context_injects_active_lessons_with_measured_bytes() {
        let mut kernel = mdx_core::MdxKernel::boot_local();
        kernel.run_evals_runner_agent().expect("evals agent");
        let source_receipt_id = kernel
            .ledger()
            .entries()
            .first()
            .expect("seed receipt")
            .receipt_id
            .clone();
        kernel
            .record_forge_outcome_signal(mdx_core::ForgeOutcomeSignal {
                tenant_id: "tenant_local",
                actor_id: "agent:forge",
                run_id: "forge_run_lesson",
                source_receipt_id: &source_receipt_id,
                source_receipt_kind: "forge.run.event",
                disposition: "completed",
                summary: "UI route landed after the screenshot proof.",
                capability_ids: "svelte_ui_skill",
                model_or_worker: "local_forge_worker",
                lesson_candidate: "Run the screenshot proof for visual route work.",
                lesson_source: "",
                message_channel_id: "forge",
            })
            .expect("outcome");
        let judgment = kernel
            .record_learning_judgment_decision(mdx_core::LearningJudgmentDecision {
                tenant_id: "tenant_local",
                actor_id: "human:eng",
                judgment_id: "judgment_recall",
                promotion_id: "promotion_recall",
                decision: "promote_candidate",
                rationale: "Screenshot proof lesson is worth promoting.",
                evidence_refs: "receipt:forge_run_lesson",
            })
            .expect("judgment");
        let promotion = kernel
            .request_learning_memory_promotion(mdx_core::LearningMemoryPromotion {
                tenant_id: "tenant_local",
                actor_id: "human:eng",
                judgment_decision_id: &judgment.judgment_decision_id,
                judgment_decision_receipt_id: &judgment.receipt_id,
                judgment_id: "judgment_recall",
                promotion_id: "promotion_recall",
                target_type: "decision_record",
                target_path: "generated/learning/forge-outcome-memory-targets.json",
                lesson_summary: "Screenshot proof is required for visual route work.",
                evidence_refs: "receipt:forge_run_lesson",
                review_cadence: "review before activation",
                expiry_rule: "supersede when stale",
            })
            .expect("promotion");
        kernel
            .activate_learning_memory(mdx_core::LearningMemoryActivation {
                tenant_id: "tenant_local",
                actor_id: "human:eng",
                memory_promotion_id: "promotion_recall",
                memory_promotion_receipt_id: &promotion.receipt_id,
                judgment_decision_receipt_id: &judgment.receipt_id,
                target_type: "decision_record",
                target_path: "generated/learning/forge-outcome-memory-targets.json",
                lesson_summary: "Screenshot proof is required for visual route work.",
                evidence_refs: "receipt:forge_run_lesson",
                activation_basis: "approved with local proof",
                rollback_plan: "supersede through a later memory receipt",
                local_checks: "make ui-screenshot-proof-check",
                approval_refs: "approval:local",
                review_owner: "human:eng",
            })
            .expect("activation");
        let (packet, _, lessons_layer) =
            super::assemble_recall_context(&kernel, "tenant_local", "anything at all");
        assert_eq!(packet.lesson_count, 1);
        assert!(lessons_layer.contains("Screenshot proof is required for visual route work."));
        assert!(lessons_layer.len() <= super::LESSON_LAYER_MAX_BYTES);
        assert_eq!(packet.lesson_bytes, lessons_layer.len());
    }

    #[test]
    fn layer_byte_cap_truncates_on_a_char_boundary() {
        assert_eq!(super::cap_layer_bytes(&"x".repeat(5000), 1200).len(), 1200);
        // A multibyte char straddling the cap is dropped, never split.
        let text = format!("{}é", "x".repeat(1199));
        let capped = super::cap_layer_bytes(&text, 1200);
        assert_eq!(capped.len(), 1199);
        assert!(capped.is_char_boundary(capped.len()));
    }

    #[test]
    fn chat_messages_interleave_history_before_the_new_question() {
        let history = vec![("q1".to_string(), "a1".to_string())];
        let messages = super::chat_messages(&history, "q2");
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0]["role"], "user");
        assert_eq!(messages[0]["content"], "q1");
        assert_eq!(messages[1]["role"], "assistant");
        assert_eq!(messages[2]["content"], "q2");
    }
}
