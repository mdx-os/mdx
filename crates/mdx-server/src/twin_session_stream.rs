// Streaming Twin answers: POST /twin/session-stream answers as a live
// SSE stream while keeping every architectural promise the blocking
// route makes. The SAME gates decide live vs deterministic (named
// provider + key present + accepted turn-on observation, via
// prepare_live_call); when any gate fails the route emits one honest
// "fallback" event and the client uses the ordinary governed POST.
// When the gates hold, provider tokens are re-framed into a normalized
// envelope (delta / done / error) and the kernel save happens ON STREAM
// COMPLETION through the same contract as the blocking route - so the
// receipt records what was actually answered, and a stream that dies
// mid-way wrote no receipt, which is the truth. The kernel still never
// opens the network; this module is serve-layer, like the gateway.
use crate::twin_live_gateway::{LiveAnswerRequest, PreparedLiveCall, chat_messages};
use crate::twin_session::{capped_document_field, capped_field, credential_receipt_id, field_or};
use mdx_core::{MdxKernel, TwinLiveAnswer, TwinSessionDraftRequest, json_string_literal};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, RwLock};

// A reasoning model can stay silent while it thinks before the first token, so
// the per-read window must tolerate that quiet stretch or the stream aborts
// mid-think (grok-4.5 tripped the old 60s). 120s covers reasoning latency while
// still surfacing a genuinely dead stream.
pub(crate) const STREAM_READ_TIMEOUT_SECS: u64 = 120;
const MAX_REQUEST_BODY_BYTES: usize = 65536;

pub(crate) fn handle(
    mut stream: TcpStream,
    request_head: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<(), String> {
    let body = read_full_body(&mut stream, request_head)?;
    let session_id = field_or(&body, "session_id", "twin_session_local_post_001");
    let companion_id = field_or(&body, "companion_id", "twin_architect");
    let companion_stance = field_or(&body, "companion_stance", "risk_and_architecture");
    let persona_profile_id = field_or(
        &body,
        "persona_profile_id",
        "architect_stress_tests_assumptions",
    );
    let prompt_shape = field_or(
        &body,
        "prompt_shape",
        "help me think through the next migration slice",
    );
    let draft_text = field_or(
        &body,
        "draft_text",
        "Local Twin draft saved from authenticated local POST route.",
    );
    // Match the blocking Twin route and the install/model-fabric tenant. A
    // different local fallback here makes the runtime claim a model is
    // connected while every streamed conversation still falls back.
    let tenant_id = field_or(&body, "tenant_id", "local_tenant");
    let actor_id = field_or(&body, "actor_id", "local_user");
    let persona_context = capped_field(&body, "persona_context");
    let companion_context = capped_field(&body, "companion_context");
    let world_context = capped_field(&body, "world_context");
    let document_context = capped_document_field(&body, "document_context");
    let document_name = field_or(&body, "document_name", "");
    let sensitivity_tier = field_or(&body, "sensitivity_tier", "internal");
    let capabilities = crate::twin_guards::Capabilities::load();

    // Input guards run BEFORE anything reaches the model or the record:
    // a blocked question was never sent and never saved, and the event
    // says so plainly. Deterministic checks, v1's pipeline.
    if let crate::twin_guards::InputVerdict::Blocked(reason) =
        crate::twin_guards::check_input(&capabilities, &draft_text)
    {
        let _ = stream.set_nodelay(true);
        write_head(&mut stream)?;
        let frame = format!(
            r#"{{"type":"guarded","message":{}}}"#,
            json_string_literal(reason)
        );
        let _ = write_event(&mut stream, &frame);
        return finish(&mut stream);
    }

    let prepared = match kernel.write() {
        Ok(mut kernel) => crate::twin_live_gateway::prepare_live_call(
            &mut kernel,
            &LiveAnswerRequest {
                actor_id: &actor_id,
                session_id: &session_id,
                companion_id: &companion_id,
                companion_stance: &companion_stance,
                prompt_shape: &prompt_shape,
                persona_context: &persona_context,
                companion_context: &companion_context,
                world_context: &world_context,
                document_context: &document_context,
                document_name: &document_name,
                sensitivity_tier: &sensitivity_tier,
                tenant_id: &tenant_id,
                draft_text: &draft_text,
            },
        ),
        Err(_) => return Err("kernel lock poisoned".to_string()),
    };

    // Low-latency from here on: tokens must leave as they arrive.
    let _ = stream.set_nodelay(true);
    write_head(&mut stream)?;

    let Some(call) = prepared else {
        // Honest fallback: the live gates do not hold right now. The
        // client retries on the ordinary POST and gets the deterministic
        // answer with full receipts.
        let _ = write_event(
            &mut stream,
            r#"{"type":"fallback","reason":"live gates not held"}"#,
        );
        return finish(&mut stream);
    };

    if !matches!(
        crate::model_gateway_runtime::protocol_for_provider(call.provider),
        Some(crate::model_gateway_runtime::GatewayProtocol::OpenAiChat)
            | Some(crate::model_gateway_runtime::GatewayProtocol::AnthropicMessages)
    ) {
        let _ = write_event(
            &mut stream,
            r#"{"type":"fallback","reason":"this provider uses the complete-response path"}"#,
        );
        return finish(&mut stream);
    }

    // Day-1 scale rails (ADR 0472): a person past the per-minute window or
    // a server at its concurrent-stream cap falls back to the
    // deterministic path - an honest answer, never an error.
    if !crate::twin_stream_limits::actor_within_rate(&actor_id) {
        crate::twin_stream_limits::record(crate::twin_stream_limits::StreamRecord {
            provider: call.provider.to_string(),
            model: call.model.clone(),
            outcome: "rate_limited",
            ttft_ms: None,
            duration_ms: 0,
            delta_count: 0,
        });
        let _ = write_event(
            &mut stream,
            r#"{"type":"fallback","reason":"a lot of live questions this minute - this one is answered deterministically while the window clears"}"#,
        );
        return finish(&mut stream);
    }
    let Some(_slot) = crate::twin_stream_limits::try_acquire_slot() else {
        crate::twin_stream_limits::record(crate::twin_stream_limits::StreamRecord {
            provider: call.provider.to_string(),
            model: call.model.clone(),
            outcome: "busy",
            ttft_ms: None,
            duration_ms: 0,
            delta_count: 0,
        });
        let _ = write_event(
            &mut stream,
            r#"{"type":"fallback","reason":"every live seat is taken right now - answered deterministically"}"#,
        );
        return finish(&mut stream);
    };

    let stream_started = std::time::Instant::now();
    let outcome = stream_provider(&mut stream, &call, &capabilities, &draft_text, true).and_then(
        |mut streamed| {
            if !needs_direct_answer_retry(&draft_text, &streamed) {
                return Ok(streamed);
            }

            // Some models choose the presentation-only follow-up tool as the
            // whole turn even though its contract says "after answering".
            // Retry once with tools absent so the person always gets an actual
            // answer. Preserve the useful suggestions and emit them only after
            // the direct prose arrives.
            let proposals = std::mem::take(&mut streamed.proposals);
            if !streamed.text.trim().is_empty() {
                write_event(&mut stream, r#"{"type":"redaction","text":""}"#)?;
            }
            let retry_prompt = crate::twin_live_gateway::substantive_retry_prompt(&draft_text);
            let mut direct =
                stream_provider(&mut stream, &call, &capabilities, &retry_prompt, false)?;
            if direct.text.trim().is_empty()
                || crate::twin_live_gateway::answer_needs_substantive_retry(
                    &draft_text,
                    &direct.text,
                )
            {
                return Err("provider returned no substantive direct answer".to_string());
            }
            direct.proposals = proposals;
            Ok(direct)
        },
    );
    let duration_ms = stream_started.elapsed().as_millis() as u64;
    let (outcome_kind, ttft_ms, delta_count) = match &outcome {
        Ok(streamed) if !streamed.text.is_empty() || !streamed.proposals.is_empty() => {
            ("completed", streamed.ttft_ms, streamed.delta_count)
        }
        Ok(streamed) => ("empty", streamed.ttft_ms, streamed.delta_count),
        Err(_) => ("failed", None, 0),
    };
    crate::twin_stream_limits::record(crate::twin_stream_limits::StreamRecord {
        provider: call.provider.to_string(),
        model: call.model.clone(),
        outcome: outcome_kind,
        ttft_ms,
        duration_ms,
        delta_count,
    });
    match outcome {
        Ok(streamed) if !streamed.text.is_empty() || !streamed.proposals.is_empty() => {
            let first_token_ms = streamed.ttft_ms.unwrap_or(0);
            let _ = crate::model_fabric_registry::global().report_execution(
                &call.tenant_id,
                &call.deployment_id,
                true,
            );
            // Skill proposals ride ahead of done: the surface renders an
            // approval card; nothing executes unless the person says yes,
            // and then it runs through the existing governed write rails.
            for (tool, args) in &streamed.proposals {
                // Every offer the model makes leaves a ledger record at the
                // moment it is made - sizes and ids only, never content.
                let proposal_id = crate::twin_skill_proposals::mint_proposal_id();
                let _ = crate::twin_skill_proposals::record_event(
                    &crate::twin_skill_proposals::ProposalEvent {
                        event: "proposed",
                        proposal_id: &proposal_id,
                        skill: tool,
                        session_id: &session_id,
                        actor_id: &actor_id,
                        args_chars: args.chars().count(),
                        executed_receipt_id: "",
                    },
                );
                let frame = format!(
                    r#"{{"type":"tool_proposal","proposal_id":{},"tool":{},"args":{}}}"#,
                    json_string_literal(&proposal_id),
                    json_string_literal(tool),
                    args
                );
                let _ = write_event(&mut stream, &frame);
            }
            // The proposal-only case was retried above with tools removed, so
            // this is always provider-authored prose. Never manufacture a
            // generic sentence that could be mistaken for a real answer.
            let raw_text = streamed.text;
            // Output guards on the COMPLETED answer: redactions land in
            // the saved record, and a correction event keeps the display
            // matching the receipt. Voice drift rides as telemetry.
            let guard = crate::twin_guards::check_output(&capabilities, &raw_text);
            if guard.redacted {
                let frame = format!(
                    r#"{{"type":"redaction","text":{}}}"#,
                    json_string_literal(&guard.text)
                );
                let _ = write_event(&mut stream, &frame);
            }
            let answer_text = guard.text;
            // The stream completed: save through the SAME kernel contract
            // as the blocking route, then hand the receipts to the client.
            let live = TwinLiveAnswer {
                answer_text,
                provider: call.provider.to_string(),
                adapter: call.adapter.to_string(),
                model_id: streamed.model_id,
                inference_id: streamed.inference_id,
                routing: call.routing_label(),
            };
            let saved = match kernel.write() {
                Ok(mut kernel) => {
                    let evidence_receipt_id = credential_receipt_id(&mut kernel)?;
                    let saved = kernel
                        .save_twin_session_draft_local(TwinSessionDraftRequest {
                            tenant_id: &tenant_id,
                            actor_id: &actor_id,
                            session_id: &session_id,
                            companion_id: &companion_id,
                            companion_stance: &companion_stance,
                            persona_profile_id: &persona_profile_id,
                            prompt_shape: &prompt_shape,
                            evidence_receipt_id: &evidence_receipt_id,
                            draft_text: &draft_text,
                            live_answer: Some(&live),
                            // The packet assembled into THIS prompt at
                            // prepare time: the receipt records what rode.
                            recall_packet: Some(&call.recall),
                        })
                        .map_err(|error| error.message())?;
                    let correlation = mdx_core::CorrelationIds {
                        tenant_id: mdx_core::TenantId::new(&tenant_id),
                        trace_id: mdx_core::TraceId::new(kernel.mint_id("trace")),
                        actor_id: mdx_core::ActorId::new(&actor_id),
                        loop_id: mdx_core::LoopId::new("model_fabric"),
                        workflow_id: mdx_core::WorkflowId::new(kernel.mint_id("workflow")),
                    };
                    crate::model_fabric_service::persist_pending_model_runtime_evidence(
                        &mut kernel,
                        &tenant_id,
                        &actor_id,
                    )?;
                    kernel
                        .record_model_outcome(
                            &correlation,
                            &mdx_core::ModelOutcome {
                                decision_id: call.decision_receipt_id.clone(),
                                workload_id: call.workload_id.clone(),
                                app_id: call.app_id.clone(),
                                environment: call.environment.clone(),
                                session_id: call.session_id.clone(),
                                deployment_id: call.deployment_id.clone(),
                                latency_ms: duration_ms,
                                cost_microusd: None,
                                quality_score: None,
                                safety_status: "not_evaluated".to_string(),
                                task_status: "not_evaluated".to_string(),
                                correction_status: "not_observed".to_string(),
                                provenance: format!(
                                    "provider_stream_runtime:{}",
                                    live.inference_id
                                ),
                            },
                        )
                        .map_err(str::to_string)?;
                    Ok(saved)
                }
                Err(_) => Err("kernel lock poisoned".to_string()),
            };
            let drift = guard.voice_drift;
            match saved {
                Ok(report) => {
                    let prompt_layers = call
                        .prompt_layers
                        .iter()
                        .map(|tag| json_string_literal(tag))
                        .collect::<Vec<_>>()
                        .join(",");
                    // The memories that shaped this answer, disclosed to the
                    // person whose memories they are - same trust boundary as
                    // the streamed answer itself. Receipts still carry ids
                    // and counts only, never content.
                    let recall_sources = call
                        .recall
                        .sources
                        .iter()
                        .map(|source| {
                            let snippet: String = source.content.chars().take(120).collect();
                            format!(
                                r#"{{"memory_id":{},"snippet":{},"score":{}}}"#,
                                json_string_literal(&source.memory_id),
                                json_string_literal(&snippet),
                                source.final_score
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(",");
                    let done = format!(
                        r#"{{"type":"done","first_token_ms":{first_token_ms},"duration_ms":{duration_ms},"guard_voice_drift":{drift},"prompt_layers":[{prompt_layers}],"memory_relevance_score":{score},"brain_recall_source_count":{source_count},"brain_recall_receipt_id":{recall_receipt},"memory_recall_sources":[{recall_sources}],"session_id":{},"draft_receipt_id":{},"answer_receipt_id":{},"created_at":{},"model_gateway_driver":{},"model_gateway_provider":{},"model_gateway_model_id":{},"model_gateway_inference_id":{},"trusted_context_used":{},"trusted_context_source_count":{},"trusted_context_source_ids":{},"trusted_context_receipt_ids":{},"trusted_context_projection_route":{}}}"#,
                        json_string_literal(&report.session_id),
                        json_string_literal(&report.draft_receipt_id),
                        json_string_literal(&report.answer_receipt_id),
                        json_string_literal(&report.created_at),
                        json_string_literal(&report.model_gateway_driver),
                        json_string_literal(&report.model_gateway_provider),
                        json_string_literal(&report.model_gateway_model_id),
                        json_string_literal(&report.model_gateway_inference_id),
                        report.trusted_context_used,
                        report.trusted_context_source_count,
                        json_string_literal(&report.trusted_context_source_ids),
                        json_string_literal(&report.trusted_context_receipt_ids),
                        json_string_literal(report.trusted_context_projection_route),
                        score = report.memory_relevance_score,
                        source_count = report.brain_recall_source_count,
                        recall_receipt = json_string_literal(&report.brain_recall_receipt_id),
                        recall_sources = recall_sources
                    );
                    let _ = write_event(&mut stream, &done);
                }
                Err(error) => {
                    eprintln!("twin_session_stream: save after stream failed: {error}");
                    let _ = write_event(
                        &mut stream,
                        r#"{"type":"error","message":"answer streamed but was not saved - ask again to keep it"}"#,
                    );
                }
            }
        }
        Ok(_) => {
            record_failed_stream_attempt(kernel, &call, &actor_id, duration_ms, "invalid response");
            let _ = write_event(
                &mut stream,
                r#"{"type":"error","message":"the model returned an empty answer"}"#,
            );
        }
        Err(error) => {
            record_failed_stream_attempt(kernel, &call, &actor_id, duration_ms, &error);
            // No answer receipt was written. Never echo upstream details that
            // could carry sensitive material; log them locally instead.
            eprintln!("twin_session_stream: provider stream failed: {error}");
            let _ = write_event(
                &mut stream,
                r#"{"type":"error","message":"the live answer was interrupted and nothing was saved"}"#,
            );
        }
    }
    finish(&mut stream)
}

fn record_failed_stream_attempt(
    kernel: &Arc<RwLock<MdxKernel>>,
    call: &PreparedLiveCall,
    actor_id: &str,
    latency_ms: u64,
    error: &str,
) {
    let registry = crate::model_fabric_registry::global();
    let queued = registry.report_failed_attempt(
        &call.tenant_id,
        crate::model_fabric_registry::FailedInferenceAttempt {
            decision_id: call.decision_receipt_id.clone(),
            workload_id: call.workload_id.clone(),
            app_id: call.app_id.clone(),
            environment: call.environment.clone(),
            session_id: call.session_id.clone(),
            deployment_id: call.deployment_id.clone(),
            latency_ms,
            failure_class: crate::model_fabric_service::inference_failure_class(error).to_string(),
            provenance: "twin_stream_runtime_failure".to_string(),
        },
    );
    let _ = registry.report_execution(&call.tenant_id, &call.deployment_id, false);
    if queued.is_ok()
        && let Ok(mut kernel) = kernel.write()
        && let Err(persist_error) =
            crate::model_fabric_service::persist_pending_model_runtime_evidence(
                &mut kernel,
                &call.tenant_id,
                actor_id,
            )
    {
        eprintln!("twin_session_stream: failure evidence receipt failed: {persist_error}");
    }
}

struct StreamedAnswer {
    text: String,
    model_id: String,
    inference_id: String,
    // Skill proposals the model made: (tool name, raw json args). Never
    // executed here - the surface renders an approval card and the
    // person's yes runs the existing governed write.
    proposals: Vec<(String, String)>,
    // Telemetry for the since-boot ring: when the first token left for
    // the client, and how many deltas the provider produced.
    ttft_ms: Option<u64>,
    delta_count: usize,
}

fn needs_direct_answer_retry(draft_text: &str, answer: &StreamedAnswer) -> bool {
    (answer.text.trim().is_empty() && !answer.proposals.is_empty())
        || crate::twin_live_gateway::answer_needs_substantive_retry(draft_text, &answer.text)
}

fn stream_provider(
    client: &mut TcpStream,
    call: &PreparedLiveCall,
    capabilities: &crate::twin_guards::Capabilities,
    draft_text: &str,
    allow_tools: bool,
) -> Result<StreamedAnswer, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_read(std::time::Duration::from_secs(STREAM_READ_TIMEOUT_SECS))
        .build();
    let (request, body) = match call.provider {
        "xai" | "mistral" | "huggingface" | "together" | "fireworks" | "openrouter" | "vercel"
        | "ollama" | "vllm" | "sovereign" => {
            let mut messages =
                vec![serde_json::json!({ "role": "system", "content": call.system })];
            messages.extend(chat_messages(&call.history, draft_text));
            let tools = if allow_tools {
                super::twin_live_gateway::openai_tools(capabilities)
            } else {
                serde_json::json!([])
            };
            let mut xai_body = serde_json::json!({
                "model": call.model,
                "messages": messages,
                "max_tokens": super::twin_live_gateway::LIVE_ANSWER_MAX_TOKENS,
                "tools": tools,
                "stream": true
            });
            // Low reasoning effort keeps the streamed reply snappy on grok-4.5+
            // (which default to high); gated on model support.
            if super::twin_live_gateway::model_accepts_reasoning_effort(&call.model) {
                xai_body["reasoning_effort"] = serde_json::json!("low");
            }
            (
                agent
                    .post(&format!("{}/chat/completions", call.base_url))
                    .set("Authorization", &format!("Bearer {}", call.api_key))
                    .set("Content-Type", "application/json"),
                xai_body,
            )
        }
        "anthropic" => {
            let tools = if allow_tools {
                super::twin_live_gateway::anthropic_tools(capabilities)
            } else {
                serde_json::json!([])
            };
            (
                agent
                    // base_url already includes /v1 (e.g. https://api.anthropic.com/v1).
                    // Appending /v1/messages produced /v1/v1/messages and a 401.
                    .post(&format!("{}/messages", call.base_url.trim_end_matches('/')))
                    .set("x-api-key", &call.api_key)
                    .set("anthropic-version", "2023-06-01")
                    .set("Content-Type", "application/json"),
                serde_json::json!({
                "model": call.model,
                "max_tokens": super::twin_live_gateway::LIVE_ANSWER_MAX_TOKENS,
                "system": call.system,
                "messages": chat_messages(&call.history, draft_text),
                "tools": tools,
                "stream": true
                }),
            )
        }
        other => return Err(format!("unsupported provider {other}")),
    };
    let provider_started = std::time::Instant::now();
    let response = request
        .send_string(&body.to_string())
        .map_err(|error| format!("transport: {error}"))?;
    let reader = BufReader::new(response.into_reader());

    let mut ttft_ms: Option<u64> = None;
    let mut delta_count: usize = 0;
    let mut text = String::new();
    let mut model_id = call.model.clone();
    let mut inference_id = "live_inference".to_string();
    // Tool-call fragments accumulate per provider index: both providers
    // stream a tool's name first, then its arguments as partial JSON
    // string pieces that only parse once complete.
    let mut tools: Vec<(String, String)> = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(|error| format!("read: {error}"))?;
        let Some(data) = line.strip_prefix("data: ") else {
            continue;
        };
        if data == "[DONE]" {
            break;
        }
        let Ok(event) = serde_json::from_str::<serde_json::Value>(data) else {
            continue;
        };
        let delta = match call.provider {
            "xai" => {
                if let Some(id) = event["id"].as_str() {
                    inference_id = id.to_string();
                }
                if let Some(model) = event["model"].as_str() {
                    model_id = model.to_string();
                }
                if let Some(calls) = event["choices"][0]["delta"]["tool_calls"].as_array() {
                    for call_delta in calls {
                        let index = call_delta["index"].as_u64().unwrap_or(0) as usize;
                        while tools.len() <= index {
                            tools.push((String::new(), String::new()));
                        }
                        if let Some(name) = call_delta["function"]["name"].as_str() {
                            tools[index].0.push_str(name);
                        }
                        if let Some(args) = call_delta["function"]["arguments"].as_str() {
                            tools[index].1.push_str(args);
                        }
                    }
                }
                event["choices"][0]["delta"]["content"]
                    .as_str()
                    .unwrap_or("")
                    .to_string()
            }
            _ => match event["type"].as_str() {
                Some("message_start") => {
                    if let Some(id) = event["message"]["id"].as_str() {
                        inference_id = id.to_string();
                    }
                    if let Some(model) = event["message"]["model"].as_str() {
                        model_id = model.to_string();
                    }
                    String::new()
                }
                Some("content_block_start") => {
                    if event["content_block"]["type"].as_str() == Some("tool_use") {
                        let index = event["index"].as_u64().unwrap_or(0) as usize;
                        while tools.len() <= index {
                            tools.push((String::new(), String::new()));
                        }
                        if let Some(name) = event["content_block"]["name"].as_str() {
                            tools[index].0 = name.to_string();
                        }
                    }
                    String::new()
                }
                Some("content_block_delta") => {
                    if let Some(partial) = event["delta"]["partial_json"].as_str() {
                        let index = event["index"].as_u64().unwrap_or(0) as usize;
                        while tools.len() <= index {
                            tools.push((String::new(), String::new()));
                        }
                        tools[index].1.push_str(partial);
                        String::new()
                    } else {
                        event["delta"]["text"].as_str().unwrap_or("").to_string()
                    }
                }
                Some("message_stop") => break,
                Some("error") => {
                    return Err(format!(
                        "provider error: {}",
                        event["error"]["type"].as_str().unwrap_or("unknown")
                    ));
                }
                _ => String::new(),
            },
        };
        if delta.is_empty() {
            continue;
        }
        if ttft_ms.is_none() {
            ttft_ms = Some(provider_started.elapsed().as_millis() as u64);
        }
        delta_count += 1;
        text.push_str(&delta);
        let frame = format!(
            r#"{{"type":"delta","text":{}}}"#,
            json_string_literal(&delta)
        );
        // The person closed the tab or hit stop: the write fails, we
        // abandon the upstream read, and no receipt is written.
        write_event(client, &frame).map_err(|_| "client went away".to_string())?;
    }
    // Only complete, declared skills with parseable arguments survive -
    // a hallucinated tool name or broken JSON proposes nothing.
    let proposals = tools
        .into_iter()
        .filter(|(name, args)| {
            super::twin_live_gateway::known_skill(name)
                && serde_json::from_str::<serde_json::Value>(args).is_ok()
        })
        .take(2)
        .collect();
    Ok(StreamedAnswer {
        text,
        model_id,
        inference_id,
        proposals,
        ttft_ms,
        delta_count,
    })
}

// The request head arrives from the connection handler's first read; a
// large body (two 4KB context layers ride along) may still be in flight.
fn read_full_body(stream: &mut TcpStream, request_head: &str) -> Result<String, String> {
    let content_length = request_head
        .lines()
        .take_while(|line| !line.is_empty())
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())?
        })
        .unwrap_or(0)
        .min(MAX_REQUEST_BODY_BYTES);
    let mut body = request_head
        .split("\r\n\r\n")
        .nth(1)
        .unwrap_or("")
        .to_string();
    while body.len() < content_length {
        let mut chunk = [0_u8; 8192];
        let read = stream
            .read(&mut chunk)
            .map_err(|error| format!("read body: {error}"))?;
        if read == 0 {
            break;
        }
        body.push_str(&String::from_utf8_lossy(&chunk[..read]));
    }
    Ok(body)
}

fn write_head(stream: &mut TcpStream) -> Result<(), String> {
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache, no-transform\r\nX-Accel-Buffering: no\r\n{}\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
        crate::LOCAL_CORS_HEADERS
    );
    stream
        .write_all(head.as_bytes())
        .and_then(|_| stream.flush())
        .map_err(|error| format!("write head: {error}"))
}

fn write_event(stream: &mut TcpStream, json: &str) -> Result<(), String> {
    let frame = format!("data: {json}\n\n");
    let chunk = format!("{:x}\r\n{frame}\r\n", frame.len());
    stream
        .write_all(chunk.as_bytes())
        .and_then(|_| stream.flush())
        .map_err(|error| format!("write event: {error}"))
}

fn finish(stream: &mut TcpStream) -> Result<(), String> {
    stream
        .write_all(b"0\r\n\r\n")
        .and_then(|_| stream.flush())
        .map_err(|error| format!("write end: {error}"))
}

#[cfg(test)]
mod tests {
    use super::{StreamedAnswer, needs_direct_answer_retry};

    fn answer(text: &str, proposal_count: usize) -> StreamedAnswer {
        StreamedAnswer {
            text: text.to_string(),
            model_id: "test-model".to_string(),
            inference_id: "test-inference".to_string(),
            proposals: (0..proposal_count)
                .map(|_| {
                    (
                        "suggest_follow_ups".to_string(),
                        r#"{"questions":[]}"#.to_string(),
                    )
                })
                .collect(),
            ttft_ms: None,
            delta_count: 0,
        }
    }

    #[test]
    fn proposal_only_turn_requires_a_direct_answer_retry() {
        assert!(needs_direct_answer_retry(
            "Give me a suggestion",
            &answer("", 1)
        ));
        assert!(needs_direct_answer_retry(
            "Give me a suggestion",
            &answer("   ", 1)
        ));
        assert!(!needs_direct_answer_retry(
            "Give me a suggestion",
            &answer("A direct answer.", 1)
        ));
        assert!(!needs_direct_answer_retry(
            "Give me a suggestion",
            &answer("", 0)
        ));
    }

    #[test]
    fn shallow_workspace_handoff_requires_retry_but_casual_reply_does_not() {
        let ask = "Review the current workspace, identify the top three things that need me, recommend the safest next move, and be specific.";
        assert!(needs_direct_answer_retry(
            ask,
            &answer("Here you go - your call below.", 0)
        ));
        assert!(!needs_direct_answer_retry(
            ask,
            &answer(
                "Forge has one change ready for review, Message has a pending team decision, and Pages has no trusted source yet. Review the Forge diff first because nothing ships without that call.",
                0
            )
        ));
        assert!(!needs_direct_answer_retry("Hi", &answer("Hey!", 0)));
    }
}
