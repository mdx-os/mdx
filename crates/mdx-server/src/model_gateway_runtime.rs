use serde_json::{Value, json};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GatewayProtocol {
    OpenAiResponses,
    OpenAiChat,
    AnthropicMessages,
    GeminiGenerateContent,
    TensorZeroInference,
}

pub(crate) fn protocol_for_provider(provider_id: &str) -> Option<GatewayProtocol> {
    match provider_id {
        "openai" | "azure_foundry" => Some(GatewayProtocol::OpenAiResponses),
        "anthropic" => Some(GatewayProtocol::AnthropicMessages),
        "google" | "gemini" => Some(GatewayProtocol::GeminiGenerateContent),
        "tensorzero" => Some(GatewayProtocol::TensorZeroInference),
        "xai" | "moonshot" | "mistral" | "huggingface" | "together" | "fireworks"
        | "openrouter" | "vercel" | "ollama" | "vllm" | "sovereign" => {
            Some(GatewayProtocol::OpenAiChat)
        }
        _ => None,
    }
}

pub(crate) fn default_base_url(provider_id: &str) -> Option<&'static str> {
    match provider_id {
        "openai" => Some("https://api.openai.com/v1"),
        "anthropic" => Some("https://api.anthropic.com/v1"),
        "xai" => Some("https://api.x.ai/v1"),
        "moonshot" => Some("https://api.moonshot.ai/v1"),
        "google" | "gemini" => Some("https://generativelanguage.googleapis.com/v1beta"),
        "mistral" => Some("https://api.mistral.ai/v1"),
        "huggingface" => Some("https://router.huggingface.co/v1"),
        "together" => Some("https://api.together.xyz/v1"),
        "fireworks" => Some("https://api.fireworks.ai/inference/v1"),
        "openrouter" => Some("https://openrouter.ai/api/v1"),
        "vercel" => Some("https://ai-gateway.vercel.sh/v1"),
        "ollama" => Some("http://127.0.0.1:11434/v1"),
        "vllm" => Some("http://127.0.0.1:8000/v1"),
        "tensorzero" => Some("http://127.0.0.1:3000"),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GatewayTool {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GatewayMessage {
    pub role: String,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GatewayCallRequest {
    pub provider_id: String,
    pub adapter_id: String,
    pub protocol: GatewayProtocol,
    pub base_url: String,
    pub api_key: String,
    pub model_id: String,
    pub system: String,
    pub messages: Vec<GatewayMessage>,
    pub tools: Vec<GatewayTool>,
    pub max_output_tokens: u32,
    pub timeout: Duration,
    pub session_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GatewayToolCall {
    pub call_id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GatewayCallResult {
    pub provider_id: String,
    pub adapter_id: String,
    pub model_id: String,
    pub inference_id: String,
    pub text: String,
    pub tool_calls: Vec<GatewayToolCall>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub latency_ms: u64,
    pub finish_reason: String,
}

#[derive(Clone, Debug, PartialEq)]
struct WireRequest {
    url: String,
    body: Value,
    auth: WireAuth,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WireAuth {
    Bearer,
    Anthropic,
    Gemini,
    None,
}

pub(crate) fn execute(request: &GatewayCallRequest) -> Result<GatewayCallResult, String> {
    validate_request(request)?;
    let wire = prepare_wire_request(request)?;
    let agent = ureq::AgentBuilder::new().timeout(request.timeout).build();
    let mut http = agent
        .post(&wire.url)
        .set("Content-Type", "application/json");
    http = match wire.auth {
        WireAuth::Bearer => http.set("Authorization", &format!("Bearer {}", request.api_key)),
        WireAuth::Anthropic => http
            .set("x-api-key", &request.api_key)
            .set("anthropic-version", "2023-06-01"),
        WireAuth::Gemini => http.set("x-goog-api-key", &request.api_key),
        WireAuth::None => http,
    };
    let started = Instant::now();
    let body = wire.body.to_string();
    let mut attempt = 0_u64;
    let response = loop {
        match http.clone().send_string(&body) {
            Ok(response) => break response,
            Err(ureq::Error::Status(status, _))
                if attempt < 2 && (status == 429 || status >= 500) =>
            {
                attempt += 1;
                std::thread::sleep(Duration::from_millis(100 * attempt));
            }
            Err(ureq::Error::Transport(_)) if attempt < 2 => {
                attempt += 1;
                std::thread::sleep(Duration::from_millis(100 * attempt));
            }
            Err(ureq::Error::Status(status, _)) => {
                return Err(format!(
                    "{} upstream returned HTTP {status}",
                    request.adapter_id
                ));
            }
            Err(ureq::Error::Transport(error)) => {
                return Err(format!("{} transport failed: {error}", request.adapter_id));
            }
        }
    };
    let parsed: Value = response
        .into_json()
        .map_err(|error| format!("{} response parse failed: {error}", request.adapter_id))?;
    let mut result = parse_wire_response(request.protocol, &parsed)?;
    result.provider_id.clone_from(&request.provider_id);
    result.adapter_id.clone_from(&request.adapter_id);
    result.latency_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    Ok(result)
}

fn validate_request(request: &GatewayCallRequest) -> Result<(), String> {
    if request.provider_id.trim().is_empty()
        || request.adapter_id.trim().is_empty()
        || request.base_url.trim().is_empty()
        || request.model_id.trim().is_empty()
    {
        return Err("gateway provider, adapter, base URL, and model are required".to_string());
    }
    if request.model_id.chars().count() > 256
        || request.model_id.chars().any(char::is_control)
        || (request.protocol == GatewayProtocol::GeminiGenerateContent
            && !request.model_id.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
            }))
    {
        return Err("gateway model id is invalid or too long".to_string());
    }
    if request.tools.iter().any(|tool| {
        tool.name.is_empty()
            || tool.name.len() > 64
            || !tool.name.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '_' | '-')
            })
    }) {
        return Err(
            "gateway tool names must use at most 64 letters, digits, underscores, or hyphens"
                .to_string(),
        );
    }
    let accepts_credentialless_local = matches!(request.provider_id.as_str(), "ollama" | "vllm")
        || request.protocol == GatewayProtocol::TensorZeroInference;
    validate_base_url(&request.base_url, accepts_credentialless_local)?;
    if !accepts_credentialless_local && request.api_key.trim().is_empty() {
        return Err("gateway credential is required".to_string());
    }
    if request.max_output_tokens == 0 {
        return Err("gateway max output tokens must be greater than zero".to_string());
    }
    Ok(())
}

pub(crate) fn validate_base_url(
    base_url: &str,
    allows_plaintext_local: bool,
) -> Result<(), String> {
    let parsed = url::Url::parse(base_url)
        .map_err(|_| "gateway endpoint must be an absolute URL".to_string())?;
    if parsed.scheme() != "https" && !(allows_plaintext_local && parsed.scheme() == "http") {
        return Err(
            "gateway endpoint must use HTTPS unless it is a credentialless local gateway"
                .to_string(),
        );
    }
    if parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(
            "gateway endpoint must not contain credentials, query parameters, or fragments"
                .to_string(),
        );
    }
    Ok(())
}

fn prepare_wire_request(request: &GatewayCallRequest) -> Result<WireRequest, String> {
    match request.protocol {
        GatewayProtocol::OpenAiResponses => Ok(WireRequest {
            url: endpoint(&request.base_url, "responses"),
            body: json!({
                "model": request.model_id,
                "instructions": request.system,
                "input": request.messages.iter().map(|message| json!({
                    "role": message.role,
                    "content": message.content
                })).collect::<Vec<_>>(),
                "tools": request.tools.iter().map(response_tool).collect::<Vec<_>>(),
                "max_output_tokens": request.max_output_tokens,
                "store": false,
                "metadata": request.session_id.as_ref().map(|session| json!({"mdx_session": session})).unwrap_or_else(|| json!({}))
            }),
            auth: WireAuth::Bearer,
        }),
        GatewayProtocol::OpenAiChat => {
            let mut body = json!({
                "model": request.model_id,
                "messages": chat_messages(request),
                "tools": request.tools.iter().map(chat_tool).collect::<Vec<_>>()
            });
            if request.provider_id == "moonshot" && request.model_id.starts_with("kimi-k3") {
                body["max_completion_tokens"] = json!(request.max_output_tokens);
                body["reasoning_effort"] = json!("max");
            } else {
                body["max_tokens"] = json!(request.max_output_tokens);
            }
            Ok(WireRequest {
                url: endpoint(&request.base_url, "chat/completions"),
                body,
                auth: if matches!(request.provider_id.as_str(), "ollama" | "vllm")
                    && request.api_key.trim().is_empty()
                {
                    WireAuth::None
                } else {
                    WireAuth::Bearer
                },
            })
        }
        GatewayProtocol::AnthropicMessages => Ok(WireRequest {
            url: endpoint(&request.base_url, "messages"),
            body: json!({
                "model": request.model_id,
                "system": request.system,
                "messages": request.messages.iter().map(|message| json!({
                    "role": message.role,
                    "content": message.content
                })).collect::<Vec<_>>(),
                "tools": request.tools.iter().map(anthropic_tool).collect::<Vec<_>>(),
                "max_tokens": request.max_output_tokens
            }),
            auth: WireAuth::Anthropic,
        }),
        GatewayProtocol::GeminiGenerateContent => Ok(WireRequest {
            url: endpoint(
                &request.base_url,
                &format!("models/{}:generateContent", request.model_id),
            ),
            body: json!({
                "systemInstruction": {"parts": [{"text": request.system}]},
                "contents": request.messages.iter().map(|message| json!({
                    "role": if message.role == "assistant" { "model" } else { "user" },
                    "parts": [{"text": message.content}]
                })).collect::<Vec<_>>(),
                "tools": if request.tools.is_empty() { Vec::<Value>::new() } else { vec![json!({
                    "functionDeclarations": request.tools.iter().map(gemini_tool).collect::<Vec<_>>()
                })] },
                "generationConfig": {"maxOutputTokens": request.max_output_tokens}
            }),
            auth: WireAuth::Gemini,
        }),
        GatewayProtocol::TensorZeroInference => Ok(WireRequest {
            url: endpoint(&request.base_url, "inference"),
            body: json!({
                "model_name": request.model_id,
                "input": {
                    "system": request.system,
                    "messages": request.messages.iter().map(|message| json!({
                        "role": message.role,
                        "content": [{"type": "text", "value": message.content}]
                    })).collect::<Vec<_>>()
                },
                "additional_tools": request.tools.iter().map(tensorzero_tool).collect::<Vec<_>>(),
                "params": {"chat_completion": {"max_tokens": request.max_output_tokens}},
                "stream": false,
                "episode_id": request.session_id
            }),
            auth: if request.api_key.trim().is_empty() {
                WireAuth::None
            } else {
                WireAuth::Bearer
            },
        }),
    }
}

fn endpoint(base_url: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

fn chat_messages(request: &GatewayCallRequest) -> Vec<Value> {
    let mut messages = Vec::with_capacity(request.messages.len() + 1);
    if !request.system.trim().is_empty() {
        messages.push(json!({"role": "system", "content": request.system}));
    }
    messages.extend(
        request
            .messages
            .iter()
            .map(|message| json!({"role": message.role, "content": message.content})),
    );
    messages
}

fn response_tool(tool: &GatewayTool) -> Value {
    json!({"type": "function", "name": tool.name, "description": tool.description, "parameters": tool.parameters, "strict": true})
}

fn chat_tool(tool: &GatewayTool) -> Value {
    json!({"type": "function", "function": {"name": tool.name, "description": tool.description, "parameters": tool.parameters}})
}

fn anthropic_tool(tool: &GatewayTool) -> Value {
    json!({"name": tool.name, "description": tool.description, "input_schema": tool.parameters})
}

fn gemini_tool(tool: &GatewayTool) -> Value {
    json!({"name": tool.name, "description": tool.description, "parameters": tool.parameters})
}

fn tensorzero_tool(tool: &GatewayTool) -> Value {
    json!({"name": tool.name, "description": tool.description, "parameters": tool.parameters, "strict": true})
}

fn parse_wire_response(
    protocol: GatewayProtocol,
    value: &Value,
) -> Result<GatewayCallResult, String> {
    match protocol {
        GatewayProtocol::OpenAiResponses => parse_openai_response(value),
        GatewayProtocol::OpenAiChat => parse_chat_response(value),
        GatewayProtocol::AnthropicMessages => parse_anthropic_response(value),
        GatewayProtocol::GeminiGenerateContent => parse_gemini_response(value),
        GatewayProtocol::TensorZeroInference => parse_tensorzero_response(value),
    }
}

fn empty_result(model_id: &str, inference_id: &str) -> GatewayCallResult {
    GatewayCallResult {
        provider_id: String::new(),
        adapter_id: String::new(),
        model_id: model_id.to_string(),
        inference_id: inference_id.to_string(),
        text: String::new(),
        tool_calls: Vec::new(),
        input_tokens: 0,
        output_tokens: 0,
        latency_ms: 0,
        finish_reason: String::new(),
    }
}

fn parse_openai_response(value: &Value) -> Result<GatewayCallResult, String> {
    let mut result = empty_result(string(value, "model"), string(value, "id"));
    result.finish_reason = string(value, "status").to_string();
    result.input_tokens = value["usage"]["input_tokens"].as_u64().unwrap_or(0);
    result.output_tokens = value["usage"]["output_tokens"].as_u64().unwrap_or(0);
    for item in value["output"].as_array().into_iter().flatten() {
        match item["type"].as_str() {
            Some("message") => {
                for content in item["content"].as_array().into_iter().flatten() {
                    if content["type"].as_str() == Some("output_text") {
                        result.text.push_str(content["text"].as_str().unwrap_or(""));
                    }
                }
            }
            Some("function_call") => result.tool_calls.push(GatewayToolCall {
                call_id: string(item, "call_id").to_string(),
                name: string(item, "name").to_string(),
                arguments: parse_arguments(&item["arguments"]),
            }),
            _ => {}
        }
    }
    require_output(result)
}

fn parse_chat_response(value: &Value) -> Result<GatewayCallResult, String> {
    let mut result = empty_result(string(value, "model"), string(value, "id"));
    let choice = &value["choices"][0];
    result.text = choice["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_string();
    result.finish_reason = string(choice, "finish_reason").to_string();
    result.input_tokens = value["usage"]["prompt_tokens"].as_u64().unwrap_or(0);
    result.output_tokens = value["usage"]["completion_tokens"].as_u64().unwrap_or(0);
    result.tool_calls = choice["message"]["tool_calls"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|call| GatewayToolCall {
            call_id: string(call, "id").to_string(),
            name: string(&call["function"], "name").to_string(),
            arguments: parse_arguments(&call["function"]["arguments"]),
        })
        .collect();
    require_output(result)
}

fn parse_anthropic_response(value: &Value) -> Result<GatewayCallResult, String> {
    let mut result = empty_result(string(value, "model"), string(value, "id"));
    result.finish_reason = string(value, "stop_reason").to_string();
    result.input_tokens = value["usage"]["input_tokens"].as_u64().unwrap_or(0);
    result.output_tokens = value["usage"]["output_tokens"].as_u64().unwrap_or(0);
    for content in value["content"].as_array().into_iter().flatten() {
        match content["type"].as_str() {
            Some("text") => result.text.push_str(content["text"].as_str().unwrap_or("")),
            Some("tool_use") => result.tool_calls.push(GatewayToolCall {
                call_id: string(content, "id").to_string(),
                name: string(content, "name").to_string(),
                arguments: content["input"].clone(),
            }),
            _ => {}
        }
    }
    require_output(result)
}

fn parse_gemini_response(value: &Value) -> Result<GatewayCallResult, String> {
    let mut result = empty_result(string(value, "modelVersion"), string(value, "responseId"));
    let candidate = &value["candidates"][0];
    result.finish_reason = string(candidate, "finishReason").to_string();
    result.input_tokens = value["usageMetadata"]["promptTokenCount"]
        .as_u64()
        .unwrap_or(0);
    result.output_tokens = value["usageMetadata"]["candidatesTokenCount"]
        .as_u64()
        .unwrap_or(0);
    for part in candidate["content"]["parts"]
        .as_array()
        .into_iter()
        .flatten()
    {
        if let Some(text) = part["text"].as_str() {
            result.text.push_str(text);
        }
        if let Some(call) = part.get("functionCall") {
            result.tool_calls.push(GatewayToolCall {
                call_id: String::new(),
                name: string(call, "name").to_string(),
                arguments: call["args"].clone(),
            });
        }
    }
    require_output(result)
}

fn parse_tensorzero_response(value: &Value) -> Result<GatewayCallResult, String> {
    let mut result = empty_result(string(value, "model_name"), string(value, "inference_id"));
    result.finish_reason = "completed".to_string();
    result.input_tokens = value["usage"]["input_tokens"].as_u64().unwrap_or(0);
    result.output_tokens = value["usage"]["output_tokens"].as_u64().unwrap_or(0);
    for content in value["content"].as_array().into_iter().flatten() {
        match content["type"].as_str() {
            Some("text") => result.text.push_str(content["text"].as_str().unwrap_or("")),
            Some("tool_call") => result.tool_calls.push(GatewayToolCall {
                call_id: string(content, "id").to_string(),
                name: string(content, "name").to_string(),
                arguments: content["arguments"].clone(),
            }),
            _ => {}
        }
    }
    require_output(result)
}

fn require_output(result: GatewayCallResult) -> Result<GatewayCallResult, String> {
    if result.text.trim().is_empty() && result.tool_calls.is_empty() {
        Err("model gateway returned no text or tool calls".to_string())
    } else {
        Ok(result)
    }
}

fn parse_arguments(value: &Value) -> Value {
    value
        .as_str()
        .and_then(|arguments| serde_json::from_str(arguments).ok())
        .unwrap_or_else(|| value.clone())
}

fn string<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_requests_use_native_wire_contracts() {
        let request = GatewayCallRequest {
            provider_id: "openai".to_string(),
            adapter_id: "OpenAIResponsesModelGateway".to_string(),
            protocol: GatewayProtocol::OpenAiResponses,
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: "test".to_string(),
            model_id: "test-model".to_string(),
            system: "system".to_string(),
            messages: vec![GatewayMessage {
                role: "user".to_string(),
                content: "hello".to_string(),
            }],
            tools: vec![GatewayTool {
                name: "answer".to_string(),
                description: "Answer".to_string(),
                parameters: json!({"type":"object","properties":{},"additionalProperties":false}),
            }],
            max_output_tokens: 100,
            timeout: Duration::from_secs(1),
            session_id: Some("session-1".to_string()),
        };
        let responses = prepare_wire_request(&request).expect("responses request");
        assert!(responses.url.ends_with("/responses"));
        assert_eq!(responses.body["store"], false);
        assert_eq!(responses.body["tools"][0]["name"], "answer");

        let gemini = prepare_wire_request(&GatewayCallRequest {
            provider_id: "google".to_string(),
            adapter_id: "GeminiModelGateway".to_string(),
            protocol: GatewayProtocol::GeminiGenerateContent,
            base_url: "https://generativelanguage.googleapis.com/v1beta".to_string(),
            ..request.clone()
        })
        .expect("gemini request");
        assert!(gemini.url.ends_with("models/test-model:generateContent"));
        assert_eq!(gemini.auth, WireAuth::Gemini);
    }

    #[test]
    fn local_gateways_do_not_require_cloud_credentials() {
        for provider_id in ["ollama", "vllm"] {
            let request = GatewayCallRequest {
                provider_id: provider_id.to_string(),
                adapter_id: "OpenAICompatibleModelGateway".to_string(),
                protocol: GatewayProtocol::OpenAiChat,
                base_url: "http://127.0.0.1:8000/v1".to_string(),
                api_key: String::new(),
                model_id: "local-model".to_string(),
                system: String::new(),
                messages: vec![GatewayMessage {
                    role: "user".to_string(),
                    content: "hello".to_string(),
                }],
                tools: Vec::new(),
                max_output_tokens: 32,
                timeout: Duration::from_secs(1),
                session_id: None,
            };
            validate_request(&request).expect("credentialless local gateway");
            assert_eq!(
                prepare_wire_request(&request)
                    .expect("credentialless local request")
                    .auth,
                WireAuth::None
            );
        }
    }

    #[test]
    fn moonshot_k3_uses_the_documented_max_reasoning_contract() {
        let request = GatewayCallRequest {
            provider_id: "moonshot".to_string(),
            adapter_id: "MoonshotChatModelGateway".to_string(),
            protocol: protocol_for_provider("moonshot").expect("moonshot protocol"),
            base_url: default_base_url("moonshot")
                .expect("moonshot base")
                .to_string(),
            api_key: "test".to_string(),
            model_id: "kimi-k3".to_string(),
            system: String::new(),
            messages: vec![GatewayMessage {
                role: "user".to_string(),
                content: "hello".to_string(),
            }],
            tools: Vec::new(),
            max_output_tokens: 32_768,
            timeout: Duration::from_secs(1),
            session_id: None,
        };
        let wire = prepare_wire_request(&request).expect("K3 request");
        assert_eq!(wire.url, "https://api.moonshot.ai/v1/chat/completions");
        assert_eq!(wire.body["reasoning_effort"], "max");
        assert_eq!(wire.body["max_completion_tokens"], 32_768);
        assert!(wire.body.get("max_tokens").is_none());
    }

    #[test]
    fn gateway_endpoints_refuse_secret_bearing_or_plaintext_cloud_urls() {
        assert!(validate_base_url("https://api.example.com/v1", false).is_ok());
        assert!(validate_base_url("http://127.0.0.1:8000/v1", true).is_ok());
        assert!(validate_base_url("http://api.example.com/v1", false).is_err());
        assert!(validate_base_url("https://key@api.example.com/v1", false).is_err());
        assert!(validate_base_url("https://api.example.com/v1?api_key=secret", false).is_err());
    }

    #[test]
    fn provider_responses_normalize_text_tools_and_usage() {
        let openai = parse_wire_response(
            GatewayProtocol::OpenAiResponses,
            &json!({
                "id":"resp-1","model":"test","status":"completed",
                "output":[
                    {"type":"message","content":[{"type":"output_text","text":"hello"}]},
                    {"type":"function_call","call_id":"call-1","name":"answer","arguments":"{\"ok\":true}"}
                ],
                "usage":{"input_tokens":10,"output_tokens":4}
            }),
        )
        .expect("openai response");
        assert_eq!(openai.text, "hello");
        assert_eq!(openai.tool_calls[0].arguments["ok"], true);
        assert_eq!(openai.input_tokens, 10);

        let anthropic = parse_wire_response(
            GatewayProtocol::AnthropicMessages,
            &json!({
                "id":"msg-1","model":"test","stop_reason":"end_turn",
                "content":[{"type":"text","text":"done"}],
                "usage":{"input_tokens":7,"output_tokens":2}
            }),
        )
        .expect("anthropic response");
        assert_eq!(anthropic.text, "done");
        assert_eq!(anthropic.output_tokens, 2);

        let gemini = parse_wire_response(
            GatewayProtocol::GeminiGenerateContent,
            &json!({
                "responseId":"gem-1","modelVersion":"test",
                "candidates":[{"finishReason":"STOP","content":{"parts":[{"text":"hi"}]}}],
                "usageMetadata":{"promptTokenCount":3,"candidatesTokenCount":1}
            }),
        )
        .expect("gemini response");
        assert_eq!(gemini.text, "hi");
        assert_eq!(gemini.input_tokens, 3);
    }
}
