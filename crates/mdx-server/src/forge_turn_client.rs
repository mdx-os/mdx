// The tool-calling client for the Forge harness loop. One job: carry a
// multi-turn conversation with the coding model (grok-build via the
// openai-compatible endpoint) where the model answers with TOOL CALLS
// and the loop answers back with tool results. The client holds no
// authority - it shapes requests and parses responses; every tool a
// call names is dispatched (or refused) by the loop runner against the
// separately-gated primitives. The runner may record bounded, redacted
// operator-facing prose, but prompts and raw command output never land in
// receipts.
#![allow(dead_code)]
use mdx_core::{MdxKernel, StorageProvider};
use std::cell::Cell;
use std::io::{BufRead, BufReader};
use std::time::{Duration, Instant};

const DEFAULT_PROVIDER_REQUEST_TIMEOUT_SECS: u64 = 240;
const WATCHDOG_DEFAULT_SECS_FOR_PROVIDER_ALIGNMENT: u64 = 300;
const MODEL_TURN_DEADLINE_SAFETY_MARGIN: Duration = Duration::from_millis(50);
const MAX_RETRY_AFTER_SECS: u64 = 60;

thread_local! {
    static MODEL_TURN_DEADLINE: Cell<Option<Instant>> = const { Cell::new(None) };
}

pub(crate) fn with_model_turn_deadline<T>(deadline: Instant, run: impl FnOnce() -> T) -> T {
    MODEL_TURN_DEADLINE.with(|cell| {
        let previous = cell.replace(Some(deadline));
        let result = run();
        cell.set(previous);
        result
    })
}

fn model_turn_deadline_remaining() -> Option<Duration> {
    MODEL_TURN_DEADLINE.with(|cell| {
        cell.get().map(|deadline| {
            deadline
                .checked_duration_since(Instant::now())
                .unwrap_or(Duration::ZERO)
        })
    })
}

fn model_turn_deadline_expired() -> bool {
    model_turn_deadline_remaining()
        .map(|remaining| remaining <= MODEL_TURN_DEADLINE_SAFETY_MARGIN)
        .unwrap_or(false)
}

/// The tools the coding model may call. The schema is fixed and small on
/// purpose - a constrained surface is what makes runs auditable. Each
/// maps to one PROVEN primitive: reads through the workspace files,
/// edits through the patch applier, commands through the sandbox runner.
/// Which edit wire format a model family was trained on. Frontier labs
/// reinforcement-train their models against their own harness's edit tool:
/// GPT/Codex models on the apply_patch envelope, Claude and the families
/// trained against str-replace-style harnesses on unique-span replacement.
/// Serving each family its native surface removes the one durable
/// co-training penalty a third-party harness pays; everything else about
/// the loop stays identical.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToolSurface {
    /// edit_file (unique old_str/new_str span) + write_file. The default.
    StrReplace,
    /// apply_patch (V4A envelope) replacing edit_file + write_file.
    OpenAiPatch,
}

/// Pick the edit surface from the model id. Conservative on purpose: only
/// the families known to be patch-trained flip; anything unrecognized keeps
/// the str-replace default the rest of the harness is proven on.
/// The configured reasoning effort for reasoning-capable models (gpt-5, grok,
/// o-series). Opt-in via MDX_FORGE_REASONING_EFFORT (minimal|low|medium|high|
/// xhigh); unset means the provider default, so a model that does not support
/// the field is never sent it. This is the dial to turn up for hard tasks.
pub(crate) fn configured_reasoning_effort() -> Option<String> {
    let raw = std::env::var("MDX_FORGE_REASONING_EFFORT").ok()?;
    let effort = raw.trim().to_ascii_lowercase();
    matches!(
        effort.as_str(),
        "minimal" | "low" | "medium" | "high" | "xhigh"
    )
    .then_some(effort)
}

fn default_max_output_tokens(model: &str) -> u32 {
    if model.to_ascii_lowercase().starts_with("kimi-k3") {
        // Moonshot recommends at least 16K for reasoning plus final content.
        32_768
    } else {
        8_192
    }
}

/// Map an OpenAI-style effort (minimal|low|medium|high|xhigh) to Anthropic's
/// adaptive thinking effort (low|medium|high).
fn effort_to_anthropic(effort: &str) -> Option<String> {
    match effort {
        "minimal" | "low" => Some("low".to_string()),
        "medium" => Some("medium".to_string()),
        "high" | "xhigh" => Some("high".to_string()),
        _ => None,
    }
}

pub(crate) fn tool_surface_for_model(model: &str) -> ToolSurface {
    let lower = model.to_ascii_lowercase();
    if lower.contains("gpt") || lower.contains("codex") {
        ToolSurface::OpenAiPatch
    } else {
        ToolSurface::StrReplace
    }
}

/// The tool schema for one surface: OpenAiPatch swaps edit_file+write_file
/// for the single apply_patch tool and keeps every other tool unchanged.
pub(crate) fn coding_tool_schema_for(surface: ToolSurface) -> serde_json::Value {
    let base = coding_tool_schema();
    if surface == ToolSurface::StrReplace {
        return base;
    }
    let mut tools: Vec<serde_json::Value> = Vec::new();
    let mut patch_tool_inserted = false;
    for tool in base.as_array().cloned().unwrap_or_default() {
        let name = tool["function"]["name"].as_str().unwrap_or_default();
        if name == "edit_file" || name == "write_file" {
            if !patch_tool_inserted {
                tools.push(apply_patch_tool_schema());
                patch_tool_inserted = true;
            }
            continue;
        }
        tools.push(tool);
    }
    serde_json::Value::Array(tools)
}

fn apply_patch_tool_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": "apply_patch",
            "description": "Edit workspace files with a patch in the standard apply_patch envelope: *** Begin Patch, then *** Add File: / *** Update File: / *** Delete File: sections (optional *** Move to: right after Update File), hunks led by @@ anchors with space-prefixed context lines and -/+ edit lines, then *** End Patch. Applied atomically: a patch that fails to match applies nothing. This run's ONLY edit tool - it replaces edit_file and write_file.",
            "parameters": {
                "type": "object",
                "properties": {
                    "input": { "type": "string", "description": "The full patch envelope, from *** Begin Patch to *** End Patch" }
                },
                "required": ["input"]
            }
        }
    })
}

pub(crate) fn coding_tool_schema() -> serde_json::Value {
    serde_json::json!([
        {
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "Read a file from the workspace. Returns up to max_bytes of content.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Workspace-relative file path" }
                    },
                    "required": ["path"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "search",
                "description": "Search workspace files for a pattern. Returns matching lines with file and line number.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "pattern": { "type": "string" },
                        "path_prefix": { "type": "string", "description": "Limit the search to paths starting with this prefix (optional)" }
                    },
                    "required": ["pattern"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "semantic_query",
                "description": "Read-only semantic repo lookup. Use this before broad edits to get an orientation pack or find symbols, definitions, references, dependency maps, symbol graphs, file outlines, related tests, line context, semantic capabilities, diagnostics status, or gated LSP readiness. It never writes files or marks checks passed.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "operation": { "type": "string", "enum": ["capabilities", "orientation", "workspace_symbol", "definition", "references", "impact_radius", "dependency_map", "symbol_graph", "hover", "file_outline", "related_tests", "diagnostics", "lsp_probe"] },
                        "query": { "type": "string", "description": "Symbol or text query for workspace_symbol/definition/references/related_tests" },
                        "path": { "type": "string", "description": "Workspace-relative file path for hover, file_outline, or related_tests" },
                        "line": { "type": "integer", "description": "1-based line number for hover" }
                    },
                    "required": ["operation"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "list_dir",
                "description": "List a workspace directory.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Workspace-relative directory path; empty for the root" }
                    },
                    "required": []
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "edit_file",
                "description": "Make a surgical edit to an existing file: replace an exact span of text with new text. PREFER THIS for changes to existing files - it is reliable on large files. old_str must appear EXACTLY ONCE in the file (include enough surrounding context to make it unique); it is replaced by new_str. To delete, pass an empty new_str.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "old_str": { "type": "string", "description": "The exact text to replace, unique in the file" },
                        "new_str": { "type": "string", "description": "The replacement text" }
                    },
                    "required": ["path", "old_str", "new_str"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "write_file",
                "description": "Write the FULL content of a file. Use this only for NEW files, or a complete rewrite of a small file. For changing part of an existing file, use edit_file instead - it is far more reliable.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "content": { "type": "string" }
                    },
                    "required": ["path", "content"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "run_command",
                "description": "Run one allowlisted check command in the sandboxed workspace (no network). Returns exit code and tail of output. This is the ONLY way to record proof - shell output never counts.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": { "type": "string", "description": "One of the run's allowed check commands, exactly as listed" }
                    },
                    "required": ["command"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "shell",
                "description": "Exploration shell in the sandboxed workspace: read-only source, no network, writes only to build caches and temp. Use it to look around (ls, git diff, grep with flags) or run one narrowed test while iterating. It is NOT proof - ship eligibility comes only from run_command checks. Unavailable on hosts without OS sandboxing.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": { "type": "string", "description": "The shell command to run" }
                    },
                    "required": ["command"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "investigate",
                "description": "Delegate a focused, read-only question to a fresh subagent with its own clean context. Use it when answering would take many reads or searches - how does X work, where is Y defined or used, does the codebase already have Z, why is this test failing - so your OWN context stays on the change instead of the exploration. The subagent explores and returns one concise finding; it CANNOT edit files, so you keep all the writing. Its only input is your question, so put everything it needs in there: exact file paths, the error text, and precisely what you want to know. One question per call.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "question": { "type": "string", "description": "The self-contained question, including any paths, errors, and context the investigator needs" }
                    },
                    "required": ["question"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "update_plan",
                "description": "Maintain a short, visible plan of the steps to satisfy the task, so your progress is legible. Use it for multi-step work (skip it for a one-step change); state 3 to 7 concrete steps, mark exactly one in_progress, and CALL IT AGAIN to update statuses as you finish each step. The plan tracks HOW you are carrying out the task - it does not change WHAT the task asks. Never end a run with only a plan: the deliverable is working code the checks pass.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "steps": {
                            "type": "array",
                            "description": "The ordered steps",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "step": { "type": "string", "description": "One concrete step" },
                                    "status": { "type": "string", "enum": ["pending", "in_progress", "completed"] }
                                },
                                "required": ["step", "status"]
                            }
                        }
                    },
                    "required": ["steps"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "verify",
                "description": "Get an independent second opinion on your current change from a fresh reviewer (often a different, stronger model) before you finish. It sees the task and your uncommitted diff, reads the workspace to judge it in context, and replies APPROVE or CONCERNS with specifics. Use it after a non-trivial change and before finish, especially when correctness matters or you are unsure. It cannot edit - you act on its verdict.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "focus": { "type": "string", "description": "Optional: what to scrutinize most (an edge case, a risky part). Leave empty for a general review." }
                    },
                    "required": []
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "finish",
                "description": "Declare the work done (or impossible). Call this exactly once, when the checks you ran are green or you cannot proceed.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "outcome": { "type": "string", "enum": ["done", "cannot_proceed"] },
                        "summary": { "type": "string", "description": "2 to 4 short sentences for the operator: what they get, which files/routes/scripts changed, which checks passed, or what blocked you" }
                    },
                    "required": ["outcome", "summary"]
                }
            }
        }
    ])
}

/// One tool call the model asked for, parsed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ParsedToolCall {
    pub call_id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// What one model turn returned: assistant text (may be empty), the tool
/// calls to dispatch, and presence-only usage for the receipts.
#[derive(Clone, Debug)]
pub(crate) struct ModelTurn {
    pub assistant_message: serde_json::Value,
    pub text: String,
    pub tool_calls: Vec<ParsedToolCall>,
    pub finish_reason: String,
    pub model_id: String,
    pub inference_id: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    route: Option<ForgeModelRoute>,
}

#[derive(Clone, Debug, Default)]
struct StreamingToolCall {
    id: String,
    name: String,
    arguments: String,
}

/// One process-wide HTTP agent, reused across every provider call. ureq's
/// Agent holds a thread-safe connection pool, so sharing it lets concurrent
/// fleet workers keep TLS connections alive and reuse them instead of
/// paying a fresh TCP+TLS handshake on every single model turn. ureq's
/// default keeps only ONE idle connection per host, which defeats pooling
/// the moment more than one call is in flight; the high per-host ceiling is
/// what turns "a handful of concurrent calls" into "hundreds landing
/// cleanly." The agent pools per host, so one shared instance serves every
/// provider (xAI, sovereign, ...) at once. Auth and timeouts are per call,
/// not baked into the pooled connection.
fn http_agent() -> &'static ureq::Agent {
    static AGENT: std::sync::OnceLock<ureq::Agent> = std::sync::OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(240))
            .max_idle_connections(1024)
            .max_idle_connections_per_host(1024)
            .build()
    })
}

fn provider_request_timeout() -> Duration {
    let watchdog_secs = std::env::var("MDX_FORGE_MODEL_TURN_TIMEOUT_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|seconds| *seconds >= 30)
        .unwrap_or(WATCHDOG_DEFAULT_SECS_FOR_PROVIDER_ALIGNMENT);
    let base = Duration::from_secs(
        watchdog_secs
            .saturating_sub(5)
            .clamp(1, DEFAULT_PROVIDER_REQUEST_TIMEOUT_SECS),
    );
    model_turn_deadline_remaining()
        .map(|remaining| {
            remaining
                .saturating_sub(MODEL_TURN_DEADLINE_SAFETY_MARGIN)
                .max(Duration::from_millis(1))
                .min(base)
        })
        .unwrap_or(base)
}

fn provider_request(request: ureq::Request) -> ureq::Request {
    request.timeout(provider_request_timeout())
}

fn env_provider_flag_enabled(name: &str, default: bool) -> bool {
    std::env::var(name)
        .ok()
        .map(|value| {
            !matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "no" | "off" | "disabled"
            )
        })
        .unwrap_or(default)
}

/// Serializes the tests that mutate process-global env. std::env::set_var is
/// not thread-safe, so any test that sets or removes an env var must hold
/// this lock for the duration, or two of them racing is undefined behavior.
#[cfg(test)]
pub(crate) static ENV_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The unmapped-intent branch of `deterministic_turn` must draw its
/// run_command from the "Allowed check commands" section, which the harness
/// places in the USER message (not the system prompt), and it must return the
/// BARE command with no "[proof]"/"[setup]" tag so dispatch (exact equality)
/// accepts it. Without both, the deterministic provider loops on an unmapped
/// intent forever. This pins the parse against the real message shape.
#[cfg(test)]
mod unmapped_intent_tests {
    use super::*;

    fn deterministic_client() -> TurnClient {
        TurnClient {
            provider_family: "local_deterministic".to_string(),
            base_url: "deterministic://forge".to_string(),
            api_key: String::new(),
            model: "mdx-forge-deterministic-v1".to_string(),
            max_output_tokens: 1024,
            reasoning_effort: None,
            route: None,
            fallbacks: Vec::new(),
        }
    }

    #[test]
    fn unmapped_intent_runs_bare_proof_command_from_user_message() {
        let system_message = serde_json::json!({
            "role": "system",
            "content": "You are the Forge build agent. Prove work with the project's checks."
        });
        let user_message = serde_json::json!({
            "role": "user",
            "content": "Do something the deterministic provider has no scripted slice for.\n\n\
        Allowed check commands (run exactly as written, in listed order when setup commands precede proof commands):\n\
        - [setup] pnpm install --frozen-lockfile\n\
        - [proof] npm test\n"
        });

        let client = deterministic_client();

        let turn = client.deterministic_turn(&[system_message, user_message]);
        let names: Vec<&str> = turn
            .tool_calls
            .iter()
            .map(|call| call.name.as_str())
            .collect();
        assert_eq!(names, vec!["semantic_query", "run_command", "finish"]);

        let orient = &turn.tool_calls[0];
        assert_eq!(orient.arguments["operation"], "orientation");

        let run = &turn.tool_calls[1];
        // The proof entry wins over the setup entry, and the "[proof]" tag is
        // stripped so the command is byte-identical to a dispatch allowlist entry.
        assert_eq!(run.arguments["command"], "npm test");
    }

    #[test]
    fn deterministic_small_app_ignores_recent_failure_context() {
        let user_message = serde_json::json!({
            "role": "user",
            "content": "--- Context from MDx ---\n\
        Recent Forge outcomes to consider before planning:\n\
        - budget_exhausted: RUN_BUDGET_EXHAUSTED after 4 turns. Lesson candidate: principal_orientation_gate refused finish.\n\
        - blocked: broken-test-app failed on arrival and should report honestly.\n\
        --- end context ---\n\n\
        Task: In small-app__sim_persona_038, make a one-file copy-safe fix, prove it with the existing test, and prepare a reviewable handoff.\n\n\
        Allowed check commands:\n\
        - [proof] npm test\n"
        });
        let baseline_message = serde_json::json!({
            "role": "user",
            "content": "Baseline selected proof passed before model turns. Make the requested change and rerun proof after editing."
        });

        let turn = deterministic_client().deterministic_turn(&[user_message, baseline_message]);
        let names: Vec<&str> = turn
            .tool_calls
            .iter()
            .map(|call| call.name.as_str())
            .collect();
        assert_eq!(
            names,
            vec![
                "semantic_query",
                "read_file",
                "edit_file",
                "run_command",
                "semantic_query",
                "finish"
            ]
        );
        assert_eq!(turn.tool_calls[2].name, "edit_file");
        assert_eq!(turn.tool_calls[5].arguments["outcome"], "done");
    }

    #[test]
    fn deterministic_fleet_boundary_lane_edits_only_its_named_scope() {
        let user_message = serde_json::json!({
            "role": "user",
            "content": "Task: mdx-fleet-boundary-gauntlet lane-4 finish the isolated lane.\n\nAllowed check commands:\n- [proof] node --test test/lane4.test.js\n"
        });

        let turn = deterministic_client().deterministic_turn(&[user_message]);
        let edit = turn
            .tool_calls
            .iter()
            .find(|call| call.name == "edit_file")
            .expect("edit call");
        let check = turn
            .tool_calls
            .iter()
            .find(|call| call.name == "run_command")
            .expect("check call");
        let semantic_operations = turn
            .tool_calls
            .iter()
            .filter(|call| call.name == "semantic_query")
            .map(|call| call.arguments["operation"].as_str().unwrap_or_default())
            .collect::<Vec<_>>();

        assert_eq!(edit.arguments["path"], "src/lane4.js");
        assert_eq!(
            edit.arguments["new_str"],
            "export const lane4 = \"done\";\n"
        );
        assert_eq!(check.arguments["command"], "node --test test/lane4.test.js");
        for required in [
            "symbol_graph",
            "definition",
            "dependency_map",
            "related_tests",
        ] {
            assert!(
                semantic_operations.contains(&required),
                "missing {required}"
            );
        }
    }

    #[test]
    fn deterministic_broken_test_task_keeps_arrival_failure_path() {
        let user_message = serde_json::json!({
            "role": "user",
            "content": "--- Context from MDx ---\n\
        Recent Forge outcomes to consider before planning:\n\
        - completed: small-app copied a helper and passed.\n\
        --- end context ---\n\n\
        Task: In broken-test-app__sim_persona_022, add a tiny feature but first run the existing checks and report honestly if they fail on arrival.\n\n\
        Allowed check commands:\n\
        - [proof] npm test\n"
        });

        let turn = deterministic_client().deterministic_turn(&[user_message]);
        let names: Vec<&str> = turn
            .tool_calls
            .iter()
            .map(|call| call.name.as_str())
            .collect();
        assert_eq!(names, vec!["run_command", "finish"]);
        assert_eq!(turn.tool_calls[1].arguments["outcome"], "cannot_proceed");
    }

    #[test]
    fn parse_prefers_proof_and_strips_bracket_tag() {
        let content = "\
Allowed check commands:\n\
- [setup] pnpm install --frozen-lockfile\n\
- [proof] cargo check -p mdx-server\n";
        assert_eq!(
            TurnClient::parse_first_allowed_check_command(content),
            Some("cargo check -p mdx-server".to_string())
        );
    }

    #[test]
    fn parse_falls_back_to_setup_when_no_proof_entry() {
        let content = "\
Allowed check commands:\n\
- [setup] pnpm install --frozen-lockfile\n";
        assert_eq!(
            TurnClient::parse_first_allowed_check_command(content),
            Some("pnpm install --frozen-lockfile".to_string())
        );
    }
}

#[derive(Clone)]
pub(crate) struct TurnClient {
    provider_family: String,
    base_url: String,
    api_key: String,
    model: String,
    max_output_tokens: u32,
    /// Resolved reasoning effort for this client, defaulting from the env at
    /// construction and overridable per run via with_reasoning_effort - so the
    /// operator's control-surface choice binds to one run, not the process.
    reasoning_effort: Option<String>,
    route: Option<ForgeModelRoute>,
    fallbacks: Vec<TurnClient>,
}

#[derive(Clone, Debug)]
struct ForgeModelRoute {
    tenant_id: String,
    decision_id: String,
    workload_id: String,
    app_id: String,
    environment: String,
    session_id: String,
    deployment_id: String,
    input_price: u64,
    output_price: u64,
}

impl TurnClient {
    fn deterministic_builder() -> Option<Self> {
        if !env_provider_flag_enabled("MDX_FORGE_DETERMINISTIC_PROVIDER", false) {
            return None;
        }
        Some(Self {
            provider_family: "local_deterministic".to_string(),
            base_url: "deterministic://forge".to_string(),
            api_key: String::new(),
            model: "mdx-forge-deterministic-v1".to_string(),
            max_output_tokens: 1024,
            reasoning_effort: configured_reasoning_effort(),
            route: None,
            fallbacks: Vec::new(),
        })
    }

    fn from_resolved_endpoint(
        endpoint: crate::forge_model_provider_routing::ResolvedModelEndpoint,
    ) -> Self {
        let max_output_tokens = default_max_output_tokens(&endpoint.model_id);
        Self {
            provider_family: endpoint.provider_family,
            base_url: endpoint.base_url,
            api_key: endpoint.api_key,
            model: endpoint.model_id,
            max_output_tokens,
            reasoning_effort: configured_reasoning_effort(),
            route: None,
            fallbacks: Vec::new(),
        }
    }

    /// The build model from the founder's split: MDX_XAI_BUILD_MODEL when
    /// set, else the chat model. Returns None when the provider is not
    /// xai or the key is absent - the loop then refuses to start, honestly.
    pub(crate) fn from_env() -> Option<Self> {
        if std::env::var("MDX_DEFAULT_MODEL_PROVIDER").ok()?.as_str() != "xai" {
            return None;
        }
        let api_key = std::env::var("XAI_API_KEY")
            .ok()
            .filter(|key| !key.is_empty())?;
        let model = std::env::var("MDX_XAI_BUILD_MODEL")
            .ok()
            .filter(|model| !model.is_empty())
            .or_else(|| std::env::var("MDX_XAI_MODEL").ok())
            .filter(|model| !model.is_empty())?;
        let base_url =
            std::env::var("XAI_BASE_URL").unwrap_or_else(|_| "https://api.x.ai/v1".to_string());
        Some(Self {
            provider_family: "xai".to_string(),
            base_url,
            api_key,
            model,
            max_output_tokens: 4096,
            reasoning_effort: configured_reasoning_effort(),
            route: None,
            fallbacks: Vec::new(),
        })
    }

    fn configured_builder_from_env(slot: &str) -> Option<Self> {
        let slot = slot.trim().to_ascii_uppercase();
        let prefix =
            if slot.is_empty() || !slot.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                "MDX_FLEET_BUILDER".to_string()
            } else {
                format!("MDX_FLEET_BUILDER_{slot}")
            };
        let var = |suffix: &str| {
            std::env::var(format!("{prefix}_{suffix}"))
                .ok()
                .filter(|value| !value.is_empty())
        };
        if let (Some(base_url), Some(api_key), Some(model)) =
            (var("BASE_URL"), var("API_KEY"), var("MODEL"))
        {
            let provider_family =
                crate::forge_model_provider_routing::infer_provider_family(&model, &base_url)
                    .to_string();
            let max_output_tokens = default_max_output_tokens(&model);
            return Some(Self {
                provider_family,
                base_url,
                api_key,
                model,
                max_output_tokens,
                reasoning_effort: configured_reasoning_effort(),
                route: None,
                fallbacks: Vec::new(),
            });
        }
        None
    }

    fn from_connected_model(kernel: &MdxKernel, tenant_id: &str) -> Option<Self> {
        let observations =
            crate::twin_model_gateway::accepted_provider_observations_for_tenant(kernel, tenant_id);
        let receipt = observations.into_iter().rev().find(|receipt| {
            receipt.payload.get("provider_id").map(String::as_str) == Some("xai")
        })?;
        let api_key = crate::secret_store::global()
            .provider_key_for_tenant(tenant_id, "XAI_API_KEY")
            .filter(|key| !key.trim().is_empty())?;
        let model = receipt
            .payload
            .get("model_id")
            .cloned()
            .filter(|model| !model.trim().is_empty())
            .or_else(|| std::env::var("MDX_XAI_BUILD_MODEL").ok())
            .or_else(|| std::env::var("MDX_XAI_MODEL").ok())
            .filter(|model| !model.trim().is_empty())?;
        let base_url =
            std::env::var("XAI_BASE_URL").unwrap_or_else(|_| "https://api.x.ai/v1".to_string());
        Some(Self {
            provider_family: "xai".to_string(),
            base_url,
            api_key,
            model,
            max_output_tokens: 4096,
            reasoning_effort: configured_reasoning_effort(),
            route: None,
            fallbacks: Vec::new(),
        })
    }

    /// Resolve the Forge builder for one tenant. The default builder uses the
    /// GUI-connected model first (the same process-only SecretStore Twin uses),
    /// then falls back to process env. Named fleet slots keep their explicit
    /// MDX_FLEET_BUILDER_* config, and if a named slot is absent they degrade to
    /// the default tenant-connected builder instead of failing the run.
    pub(crate) fn builder_for_tenant(
        kernel: &MdxKernel,
        tenant_id: &str,
        slot: &str,
    ) -> Option<Self> {
        Self::deterministic_builder()
            .or_else(|| {
                crate::forge_model_provider_routing::resolve_builder_endpoint(
                    kernel, tenant_id, slot,
                )
                .map(Self::from_resolved_endpoint)
            })
            .or_else(|| Self::from_connected_model(kernel, tenant_id))
            .or_else(Self::from_env)
    }

    pub(crate) fn routed_builder_for_tenant(
        kernel: &mut MdxKernel,
        tenant_id: &str,
        actor_id: &str,
        session_id: &str,
        requested_slot: &str,
    ) -> Result<Option<Self>, String> {
        if let Some(deterministic) = Self::deterministic_builder() {
            return Ok(Some(deterministic));
        }
        let registry = crate::model_fabric_registry::global();
        if !registry.tenant_has_routing_state(tenant_id)? {
            return Ok(None);
        }
        let pin = registry
            .deployment_for_tenant(tenant_id, requested_slot)?
            .map(|endpoint| endpoint.deployment.deployment_id);
        let selected = crate::model_fabric_service::select(
            kernel,
            &crate::model_fabric_service::ModelSelectionRequest {
                tenant_id,
                actor_id,
                workload_id: "mdx/forge/builder",
                preset: "",
                policy_id: "auto",
                expected_policy_version: None,
                required_region: None,
                required_residency: None,
                session_id: Some(session_id),
                pinned_deployment_id: pin.as_deref(),
                required_context_tokens: 0,
                max_input_cost_microusd_per_million: 0,
                max_output_cost_microusd_per_million: 0,
            },
        )?;
        if selected.decision.status != "ROUTE_SELECTED" {
            return Err(format!(
                "model fabric denied builder route (receipt {}): {}",
                selected.decision_receipt_id, selected.decision.selection_reason
            ));
        }
        Self::from_fabric_selection(selected, "")
            .map(Some)
            .ok_or_else(|| {
                "selected Model Fabric route has no supported Forge endpoint".to_string()
            })
    }

    pub(crate) fn default_builder_configured_for_tenant(
        kernel: &MdxKernel,
        tenant_id: &str,
    ) -> bool {
        Self::deterministic_builder().is_some()
            || crate::forge_model_provider_routing::resolve_builder_endpoint(kernel, tenant_id, "")
                .is_some()
            || Self::from_connected_model(kernel, tenant_id).is_some()
            || Self::from_env().is_some()
    }

    pub(crate) fn builder_configured_for_tenant(
        kernel: &MdxKernel,
        tenant_id: &str,
        slot: &str,
    ) -> bool {
        Self::builder_for_tenant(kernel, tenant_id, slot).is_some()
    }

    pub(crate) fn any_builder_configured_for_tenant(kernel: &MdxKernel, tenant_id: &str) -> bool {
        Self::default_builder_configured_for_tenant(kernel, tenant_id)
            || [
                "OPUS",
                "SONNET",
                "GEMINI",
                "GROK",
                "XAI",
                "BEDROCK",
                "CODEX",
                "CODEXMINI",
            ]
            .iter()
            .any(|slot| Self::configured_builder_from_env(slot).is_some())
    }

    pub(crate) fn is_deterministic(&self) -> bool {
        self.provider_family == "local_deterministic"
    }

    fn deterministic_completion(&self, messages: &[serde_json::Value]) -> ModelTurn {
        ModelTurn {
            assistant_message: serde_json::json!({
                "role": "assistant",
                "content": "Deterministic local Forge provider rendered the requested operator text without a live provider call."
            }),
            text: "Deterministic local Forge provider rendered the requested operator text without a live provider call.".to_string(),
            tool_calls: Vec::new(),
            finish_reason: "stop".to_string(),
            model_id: self.model.clone(),
            inference_id: format!(
                "deterministic_completion_{}",
                messages.len()
            ),
            input_tokens: 0,
            output_tokens: 0,
            route: None,
        }
    }

    fn deterministic_task_context(messages: &[serde_json::Value]) -> String {
        if let Some(task) = messages
            .iter()
            .filter_map(|message| message["content"].as_str())
            .filter_map(|content| {
                content
                    .find("Task:")
                    .map(|index| content[index..].to_string())
            })
            .next_back()
        {
            return task;
        }
        messages
            .iter()
            .filter_map(|message| message["content"].as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn deterministic_turn(&self, messages: &[serde_json::Value]) -> ModelTurn {
        let transcript = Self::deterministic_task_context(messages).to_ascii_lowercase();
        if transcript.contains("mdx-fleet-boundary-gauntlet") && transcript.contains("stuck-lane") {
            let delay_ms = std::env::var("MDX_FORGE_DETERMINISTIC_STUCK_MS")
                .ok()
                .and_then(|value| value.trim().parse::<u64>().ok())
                .unwrap_or(0)
                .min(30_000);
            if delay_ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
            }
        }
        let gauntlet_lane = (1..=8)
            .find(|lane| transcript.contains(&format!("mdx-fleet-boundary-gauntlet lane-{lane}")));
        let (text, tool_calls) = if let Some(lane) = gauntlet_lane {
            let path = format!("src/lane{lane}.js");
            let check = messages
                .iter()
                .filter_map(|message| message["content"].as_str())
                .find(|content| content.contains("Allowed check commands"))
                .and_then(Self::parse_first_allowed_check_command)
                .unwrap_or_else(|| "npm test".to_string());
            (
                "I will finish the bounded fixture lane, run its exact proof, and leave the branch ready for fleet integration.",
                vec![
                    ParsedToolCall {
                        call_id: format!("det_orient_gauntlet_lane_{lane}"),
                        name: "semantic_query".to_string(),
                        arguments: serde_json::json!({
                            "operation": "orientation",
                            "path": path.clone()
                        }),
                    },
                    ParsedToolCall {
                        call_id: format!("det_symbol_graph_gauntlet_lane_{lane}"),
                        name: "semantic_query".to_string(),
                        arguments: serde_json::json!({
                            "operation": "symbol_graph",
                            "path": path.clone()
                        }),
                    },
                    ParsedToolCall {
                        call_id: format!("det_definition_gauntlet_lane_{lane}"),
                        name: "semantic_query".to_string(),
                        arguments: serde_json::json!({
                            "operation": "definition",
                            "path": path.clone(),
                            "query": format!("lane{lane}")
                        }),
                    },
                    ParsedToolCall {
                        call_id: format!("det_dependencies_gauntlet_lane_{lane}"),
                        name: "semantic_query".to_string(),
                        arguments: serde_json::json!({
                            "operation": "dependency_map",
                            "path": path.clone()
                        }),
                    },
                    ParsedToolCall {
                        call_id: format!("det_related_tests_gauntlet_lane_{lane}"),
                        name: "semantic_query".to_string(),
                        arguments: serde_json::json!({
                            "operation": "related_tests",
                            "path": path.clone()
                        }),
                    },
                    ParsedToolCall {
                        call_id: format!("det_read_gauntlet_lane_{lane}"),
                        name: "read_file".to_string(),
                        arguments: serde_json::json!({"path": path.clone()}),
                    },
                    ParsedToolCall {
                        call_id: format!("det_edit_gauntlet_lane_{lane}"),
                        name: "edit_file".to_string(),
                        arguments: serde_json::json!({
                            "path": path,
                            "old_str": format!("export const lane{lane} = \"pending\";\n"),
                            "new_str": format!("export const lane{lane} = \"done\";\n")
                        }),
                    },
                    ParsedToolCall {
                        call_id: format!("det_check_gauntlet_lane_{lane}"),
                        name: "run_command".to_string(),
                        arguments: serde_json::json!({"command": check}),
                    },
                    ParsedToolCall {
                        call_id: format!("det_finish_gauntlet_lane_{lane}"),
                        name: "finish".to_string(),
                        arguments: serde_json::json!({
                            "outcome": "done",
                            "summary": format!("Finished lane-{lane}, proved its isolated contract, and left a reviewable branch for governed integration.")
                        }),
                    },
                ],
            )
        } else if transcript.contains("fail on arrival") || transcript.contains("broken-test-app") {
            (
                "I will run the selected proof first and report the arrival failure honestly.",
                vec![
                    ParsedToolCall {
                        call_id: "det_run_arrival_check".to_string(),
                        name: "run_command".to_string(),
                        arguments: serde_json::json!({"command": "npm test"}),
                    },
                    ParsedToolCall {
                        call_id: "det_finish_arrival_failure".to_string(),
                        name: "finish".to_string(),
                        arguments: serde_json::json!({
                            "outcome": "cannot_proceed",
                            "summary": "The selected proof failed on arrival before a safe change could be made. The run stopped honestly with the failing check recorded for review."
                        }),
                    },
                ],
            )
        } else if transcript.contains("small-app")
            || transcript.contains("copy-safe fix")
            || transcript.contains("rosterwith")
        {
            (
                "I will repair rosterWith so it returns a fresh roster, run the selected proof, and finish with the branch ready for review.",
                vec![
                    ParsedToolCall {
                        call_id: "det_orient_small_app".to_string(),
                        name: "semantic_query".to_string(),
                        arguments: serde_json::json!({
                            "operation": "orientation",
                            "path": "src/index.js"
                        }),
                    },
                    ParsedToolCall {
                        call_id: "det_read_small_app".to_string(),
                        name: "read_file".to_string(),
                        arguments: serde_json::json!({"path": "src/index.js"}),
                    },
                    ParsedToolCall {
                        call_id: "det_edit_small_app".to_string(),
                        name: "edit_file".to_string(),
                        arguments: serde_json::json!({
                            "path": "src/index.js",
                            "old_str": "export function rosterWith(roster, name) {\n  roster.push(greeting(name));\n  return roster;\n}\n",
                            "new_str": "export function rosterWith(roster, name) {\n  return [...roster, greeting(name)];\n}\n"
                        }),
                    },
                    ParsedToolCall {
                        call_id: "det_run_small_app".to_string(),
                        name: "run_command".to_string(),
                        arguments: serde_json::json!({"command": "npm test"}),
                    },
                    ParsedToolCall {
                        call_id: "det_related_tests_small_app".to_string(),
                        name: "semantic_query".to_string(),
                        arguments: serde_json::json!({
                            "operation": "related_tests",
                            "path": "src/index.js"
                        }),
                    },
                    ParsedToolCall {
                        call_id: "det_finish_small_app".to_string(),
                        name: "finish".to_string(),
                        arguments: serde_json::json!({
                            "outcome": "done",
                            "summary": "Fixed rosterWith to return a fresh roster without mutating callers and proved the fixture with npm test. The branch is ready for review."
                        }),
                    },
                ],
            )
        } else {
            // Unmapped intent: keep the required semantic_query orientation first,
            // then emit a run_command drawn from the transcript's allowed list
            // so the command is guaranteed to be in the allowlist. The "Allowed
            // check commands" header lives in the USER message, not the system
            // prompt, so scan every message for the one that carries it.
            let allowed = messages
                .iter()
                .filter_map(|m| m["content"].as_str())
                .find(|content| content.contains("Allowed check commands"))
                .and_then(Self::parse_first_allowed_check_command);
            let tool_calls = if let Some(cmd) = allowed {
                vec![
                    ParsedToolCall {
                        call_id: "det_orient_unmapped".to_string(),
                        name: "semantic_query".to_string(),
                        arguments: serde_json::json!({
                            "operation": "orientation",
                            "path": ""
                        }),
                    },
                    ParsedToolCall {
                        call_id: "det_run_unmapped".to_string(),
                        name: "run_command".to_string(),
                        arguments: serde_json::json!({"command": cmd}),
                    },
                    ParsedToolCall {
                        call_id: "det_finish_unmapped".to_string(),
                        name: "finish".to_string(),
                        arguments: serde_json::json!({
                            "outcome": "cannot_proceed",
                            "summary": "Deterministic local provider could not map this request to a scripted simulation slice."
                        }),
                    },
                ]
            } else {
                vec![ParsedToolCall {
                    call_id: "det_finish_unmapped".to_string(),
                    name: "finish".to_string(),
                    arguments: serde_json::json!({
                        "outcome": "cannot_proceed",
                        "summary": "Deterministic local provider could not map this request to a scripted simulation slice."
                    }),
                }]
            };
            (
                "The deterministic local provider could not map this simulation intent to a safe scripted slice.",
                tool_calls,
            )
        };
        model_turn_from_parts(
            text.to_string(),
            tool_calls
                .iter()
                .map(|call| StreamingToolCall {
                    id: call.call_id.clone(),
                    name: call.name.clone(),
                    arguments: call.arguments.to_string(),
                })
                .collect(),
            "tool_calls".to_string(),
            self.model.clone(),
            format!("deterministic_turn_{}", messages.len()),
            0,
            0,
        )
    }

    /// Named builder slots that are fully configured in env. This is the
    /// fleet-width model-diversity rail: when an operator has not pinned a
    /// specific slot, bounded parallel candidates can use distinct configured
    /// coders instead of four copies of the same model.
    pub(crate) fn configured_builder_slots() -> Vec<String> {
        let kernel = MdxKernel::boot_local();
        crate::forge_model_provider_routing::configured_slots_from_env_and_store(
            &kernel,
            "local_tenant",
        )
    }

    /// The builder slot: an explicit MDX_FLEET_BUILDER_* trio when set,
    /// else the original xAI BUILD model. The fallback differs from the
    /// other roles on purpose - a builder that falls back must fall back
    /// to a coder, not a chat model. Today this is identical to from_env
    /// until a builder key lands; then a stronger coder is a key drop.
    pub(crate) fn builder_from_env(slot: &str) -> Option<Self> {
        // A named slot (MDX_FLEET_BUILDER_<NAME>_*) is the A/B/C/D lever:
        // the same task can run under different coders, keys staying in
        // env, the choice riding the request. Empty = the default slot.
        crate::forge_model_provider_routing::resolve_role_endpoint_from_env("executor")
            .filter(|endpoint| {
                slot.trim().is_empty() || endpoint.slot == slot.trim().to_ascii_uppercase()
            })
            .map(Self::from_resolved_endpoint)
            .or_else(|| Self::configured_builder_from_env(slot))
            .or_else(Self::from_env)
    }

    /// The strongest coder SLOT that actually has credentials configured, by
    /// a fixed capability preference. Cross-cutting, high-stakes work - above
    /// all the integration phase, which wires a whole fleet together and is
    /// the most failure-expensive step - casts here rather than to the cheap
    /// default. Returns "" (the default builder) when no stronger slot is
    /// configured, so it degrades safely on a single-provider setup.
    pub(crate) fn strongest_configured_builder_slot() -> String {
        crate::forge_model_provider_routing::strongest_configured_execution_slot()
    }

    /// The model identity this client calls - used to dedup a review panel
    /// so it is genuinely diverse (distinct models) rather than one model
    /// wearing several slot names.
    pub(crate) fn model_id(&self) -> &str {
        &self.model
    }

    /// The model's max output tokens - the upper bound on one completion, which
    /// the compaction trigger reserves out of the window.
    pub(crate) fn max_output(&self) -> u32 {
        self.max_output_tokens
    }

    /// Bind a per-run reasoning effort (from the operator's control surface),
    /// overriding the env default for this client only. Invalid/empty leaves
    /// the existing (env) value.
    pub(crate) fn with_reasoning_effort(mut self, effort: &str) -> Self {
        let effort = effort.trim().to_ascii_lowercase();
        if matches!(
            effort.as_str(),
            "minimal" | "low" | "medium" | "high" | "xhigh"
        ) {
            self.reasoning_effort = Some(effort.clone());
            for fallback in &mut self.fallbacks {
                fallback.reasoning_effort = Some(effort.clone());
            }
        }
        self
    }

    /// Add reasoning_effort only to OpenAI-compatible models that are known to
    /// accept the field. Some OpenAI-wire providers reject unknown parameters
    /// with a hard 400, which would block Forge before the first model token.
    fn apply_reasoning_effort(&self, body: &mut serde_json::Value) {
        if self.is_kimi_k3() {
            body["reasoning_effort"] = serde_json::json!("max");
            return;
        }
        if let Some(effort) = &self.reasoning_effort
            && self.supports_openai_reasoning_effort()
        {
            body["reasoning_effort"] = serde_json::json!(effort);
        }
    }

    fn supports_openai_reasoning_effort(&self) -> bool {
        let provider = self.provider_family.to_ascii_lowercase();
        let model = self.model.to_ascii_lowercase();
        provider == "openai"
            && (model.starts_with("gpt-5")
                || model.starts_with("o1")
                || model.starts_with("o3")
                || model.starts_with("o4"))
    }

    /// This client's Anthropic thinking effort, or None when reasoning is off.
    fn anthropic_thinking_effort(&self) -> Option<String> {
        effort_to_anthropic(self.reasoning_effort.as_deref()?)
    }

    /// The model's context window in tokens. Env MDX_FORGE_CONTEXT_WINDOW
    /// overrides for any model; otherwise a conservative per-family heuristic
    /// (better to compact a little early than to overflow the window). Used to
    /// drive token-budget compaction instead of a blind message count.
    pub(crate) fn context_window(&self) -> u64 {
        Self::context_window_for_model(&self.model)
    }

    pub(crate) fn context_window_for_model(model: &str) -> u64 {
        if let Ok(raw) = std::env::var("MDX_FORGE_CONTEXT_WINDOW")
            && let Ok(value) = raw.parse::<u64>()
            && value > 0
        {
            return value;
        }
        let model = model.to_ascii_lowercase();
        if model.contains("kimi-k3") {
            1_048_576
        } else if model.contains("gpt") || model.contains("codex") {
            272_000
        } else if model.contains("gemini") {
            1_000_000
        } else if model.contains("grok") {
            256_000
        } else if model.contains("claude")
            || model.contains("opus")
            || model.contains("sonnet")
            || model.contains("haiku")
            || model.contains("fable")
        {
            200_000
        } else {
            128_000
        }
    }

    /// The edit tool surface this client's model was trained on. The
    /// Anthropic-native path always speaks str-replace (Claude's trained
    /// format); the OpenAI-compatible path picks by model id so a GPT or
    /// Codex model gets apply_patch and everything else keeps the default.
    pub(crate) fn tool_surface(&self) -> ToolSurface {
        if self.is_anthropic_native() {
            return ToolSurface::StrReplace;
        }
        tool_surface_for_model(&self.model)
    }

    /// The privacy class of a coder slot: "sovereign" when the model runs on
    /// infrastructure the data never leaves (an on-prem or tenant-owned
    /// deployment - e.g. a private model inside a regulated enterprise), "frontier" otherwise (an
    /// external API). A slot is sovereign when MDX_FLEET_BUILDER_<SLOT>_
    /// PRIVACY_CLASS is set to "sovereign"; the default builder ("") is
    /// frontier unless MDX_FLEET_BUILDER_PRIVACY_CLASS says otherwise. This is
    /// what makes data residency PROVABLE per stream: a sensitive stream may
    /// only run on a sovereign slot, and the slot's class lands on the
    /// receipt.
    pub(crate) fn slot_privacy_class(slot: &str) -> &'static str {
        crate::forge_model_provider_routing::slot_privacy_class(slot)
    }

    /// Whether a slot is sovereign AND can actually keep that promise. A
    /// sovereign LABEL is not enough: `builder_from_env` falls back to the
    /// default (frontier) provider when a named slot lacks its own endpoint,
    /// so a sovereign-labeled-but-unconfigured slot would silently run
    /// sensitive work on frontier. Sovereign holds only when the slot carries
    /// its own complete config (base url, key, model) so no fallback is
    /// possible. The residency gate trusts THIS, not the label, and fails
    /// closed when the two disagree.
    pub(crate) fn sovereign_slot_ready(slot: &str) -> bool {
        crate::forge_model_provider_routing::sovereign_slot_ready(slot)
    }

    /// A model slot by fleet role - the founder's split: the planner and
    /// reviewer think (strongest available reasoning model), the builder
    /// codes. Each role resolves its own env first
    /// (MDX_FLEET_<ROLE>_BASE_URL / _API_KEY / _MODEL, openai-compatible
    /// endpoints - xAI, OpenAI, and Anthropic's compatibility layer all
    /// qualify), then falls back to the live xAI slot with the CHAT
    /// model, so fleets run today and stronger providers are a key drop,
    /// not a code change.
    pub(crate) fn from_env_for_role(role: &str) -> Option<Self> {
        crate::forge_model_provider_routing::resolve_role_endpoint_from_env(role)
            .map(Self::from_resolved_endpoint)
            .or_else(|| {
                // Fallback: the live xAI slot, chat model (thinking, not coding).
                if std::env::var("MDX_DEFAULT_MODEL_PROVIDER").ok()?.as_str() != "xai" {
                    return None;
                }
                let api_key = std::env::var("XAI_API_KEY")
                    .ok()
                    .filter(|key| !key.is_empty())?;
                let model = std::env::var("MDX_XAI_MODEL")
                    .ok()
                    .filter(|model| !model.is_empty())?;
                let base_url = std::env::var("XAI_BASE_URL")
                    .unwrap_or_else(|_| "https://api.x.ai/v1".to_string());
                Some(Self {
                    provider_family: "xai".to_string(),
                    base_url,
                    api_key,
                    model,
                    max_output_tokens: 8192,
                    reasoning_effort: configured_reasoning_effort(),
                    route: None,
                    fallbacks: Vec::new(),
                })
            })
    }

    pub(crate) fn for_role<S: StorageProvider>(
        kernel: &MdxKernel<S>,
        tenant_id: &str,
        role: &str,
    ) -> Option<Self> {
        crate::forge_model_provider_routing::resolve_role_endpoint(
            kernel,
            crate::secret_store::global(),
            tenant_id,
            role,
            "",
        )
        .map(Self::from_resolved_endpoint)
        .or_else(|| Self::from_env_for_role(role))
    }

    pub(crate) fn for_role_with_reasoning_effort<S: StorageProvider>(
        kernel: &MdxKernel<S>,
        tenant_id: &str,
        role: &str,
        effort: &str,
    ) -> Option<Self> {
        Self::for_role(kernel, tenant_id, role).map(|client| client.with_reasoning_effort(effort))
    }

    pub(crate) fn routed_for_role(
        kernel: &mut MdxKernel,
        tenant_id: &str,
        actor_id: &str,
        session_id: &str,
        role: &str,
        effort: &str,
    ) -> Result<Option<Self>, String> {
        let workload_id = match role {
            "planner" => "mdx/forge/planner",
            "builder" => "mdx/forge/builder",
            "reviewer" => "mdx/forge/reviewer",
            "evaluator" => "mdx/forge/evaluator",
            "advisor" | "investigator" => "mdx/forge/advisor",
            "voice" => "mdx/forge/voice",
            _ => return Ok(None),
        };
        if !crate::model_fabric_registry::global().tenant_has_routing_state(tenant_id)? {
            return Ok(None);
        }
        let selected = crate::model_fabric_service::select(
            kernel,
            &crate::model_fabric_service::ModelSelectionRequest {
                tenant_id,
                actor_id,
                workload_id,
                preset: "",
                policy_id: "auto",
                expected_policy_version: None,
                required_region: None,
                required_residency: None,
                session_id: Some(session_id),
                pinned_deployment_id: None,
                required_context_tokens: 0,
                max_input_cost_microusd_per_million: 0,
                max_output_cost_microusd_per_million: 0,
            },
        )?;
        if selected.decision.status != "ROUTE_SELECTED" {
            return Err(format!(
                "model fabric denied {role} route (receipt {}): {}",
                selected.decision_receipt_id, selected.decision.selection_reason
            ));
        }
        Self::from_fabric_selection(selected, effort)
            .map(Some)
            .ok_or_else(|| {
                format!("selected Model Fabric {role} route has no supported Forge endpoint")
            })
    }

    fn from_fabric_selection(
        selected: crate::model_fabric_service::ModelSelectionResult,
        effort: &str,
    ) -> Option<Self> {
        let mut targets = Vec::new();
        if let (Some(endpoint), Some(api_key)) = (selected.endpoint, selected.api_key) {
            targets.push(crate::model_fabric_service::ModelExecutionTarget { endpoint, api_key });
        }
        targets.extend(selected.fallbacks);
        let mut clients = targets
            .into_iter()
            .filter(|target| Self::supports_forge_turns(&target.endpoint.connection.provider_id))
            .map(|target| {
                let endpoint = target.endpoint;
                let tenant_id = endpoint.connection.tenant_id.clone();
                Self {
                    provider_family: endpoint.connection.provider_id,
                    base_url: endpoint.connection.endpoint_base_url,
                    api_key: target.api_key,
                    model: endpoint.model.provider_model_id,
                    max_output_tokens: 8192,
                    reasoning_effort: configured_reasoning_effort(),
                    route: Some(ForgeModelRoute {
                        tenant_id,
                        decision_id: selected.decision_receipt_id.clone(),
                        workload_id: selected.decision.workload_id.clone(),
                        app_id: selected.decision.app_id.clone(),
                        environment: selected.decision.environment.clone(),
                        session_id: selected.decision.session_id.clone(),
                        deployment_id: endpoint.deployment.deployment_id,
                        input_price: endpoint.price.input_microusd_per_million,
                        output_price: endpoint.price.output_microusd_per_million,
                    }),
                    fallbacks: Vec::new(),
                }
            })
            .collect::<Vec<_>>();
        if clients.is_empty() {
            return None;
        }
        let mut primary = clients.remove(0);
        primary.fallbacks = clients;
        Some(primary.with_reasoning_effort(effort))
    }

    pub(crate) fn supports_forge_turns(provider_id: &str) -> bool {
        !matches!(
            provider_id,
            "google" | "tensorzero" | "aws_bedrock" | "azure_foundry"
        )
    }

    /// A client that may see boundary-locked content (Twin/Message/Pages
    /// memory text). Resolves ONLY loopback or provably-sovereign endpoints
    /// via the residency rail; None means no qualifying model exists and the
    /// caller must skip its model stage rather than fall through to a
    /// frontier provider.
    pub(crate) fn boundary_locked_for_role<S: StorageProvider>(
        kernel: &MdxKernel<S>,
        tenant_id: &str,
        role: &str,
    ) -> Option<Self> {
        crate::forge_model_provider_routing::resolve_boundary_locked_endpoint(
            kernel, tenant_id, role,
        )
        .map(Self::from_resolved_endpoint)
    }

    pub(crate) fn named_slot_for_role<S: StorageProvider>(
        kernel: &MdxKernel<S>,
        tenant_id: &str,
        role: &str,
        slot: &str,
    ) -> Option<Self> {
        crate::forge_model_provider_routing::resolve_role_endpoint(
            kernel,
            crate::secret_store::global(),
            tenant_id,
            role,
            slot,
        )
        .map(Self::from_resolved_endpoint)
    }

    pub(crate) fn model(&self) -> &str {
        &self.model
    }

    pub(crate) fn record_route_outcome(
        &self,
        kernel: &mut MdxKernel,
        tenant_id: &str,
        actor_id: &str,
        turn: &ModelTurn,
        latency_ms: u64,
        provenance: &str,
    ) -> Result<Option<String>, String> {
        let Some(route) = turn.route.as_ref().or(self.route.as_ref()) else {
            return Ok(None);
        };
        let _ = crate::model_fabric_registry::global().report_execution(
            tenant_id,
            &route.deployment_id,
            true,
        );
        let numerator = u128::from(turn.input_tokens) * u128::from(route.input_price)
            + u128::from(turn.output_tokens) * u128::from(route.output_price);
        let cost = numerator.div_ceil(1_000_000).min(u128::from(u64::MAX)) as u64;
        let correlation = mdx_core::CorrelationIds {
            tenant_id: mdx_core::TenantId::new(tenant_id),
            trace_id: mdx_core::TraceId::new(kernel.mint_id("trace")),
            actor_id: mdx_core::ActorId::new(actor_id),
            loop_id: mdx_core::LoopId::new("model_fabric"),
            workflow_id: mdx_core::WorkflowId::new(kernel.mint_id("workflow")),
        };
        crate::model_fabric_service::persist_pending_model_runtime_evidence(
            kernel, tenant_id, actor_id,
        )?;
        let receipt = kernel
            .record_model_outcome(
                &correlation,
                &mdx_core::ModelOutcome {
                    decision_id: route.decision_id.clone(),
                    workload_id: route.workload_id.clone(),
                    app_id: route.app_id.clone(),
                    environment: route.environment.clone(),
                    session_id: route.session_id.clone(),
                    deployment_id: route.deployment_id.clone(),
                    latency_ms,
                    cost_microusd: Some(cost),
                    quality_score: None,
                    safety_status: "not_evaluated".to_string(),
                    task_status: "not_evaluated".to_string(),
                    correction_status: "not_observed".to_string(),
                    provenance: format!("{provenance}:{}", turn.inference_id),
                },
            )
            .map_err(str::to_string)?;
        Ok(Some(receipt.receipt_id))
    }

    pub(crate) fn provider_family(&self) -> &str {
        &self.provider_family
    }

    fn parse_first_allowed_check_command(original: &str) -> Option<String> {
        let lines = original.lines();
        let mut in_section = false;
        let mut setup_fallback: Option<String> = None;
        for line in lines {
            let trimmed = line.trim_start();
            if trimmed.starts_with("Allowed check commands") {
                in_section = true;
                continue;
            }
            if in_section && trimmed.starts_with("- ") {
                // Entries are formatted "- [proof] <cmd>" or "- [setup] <cmd>".
                // Strip the leading "- " and any "[tag]" prefix so the returned
                // command is byte-identical to a request.allowed_commands entry,
                // which dispatch matches on exact equality.
                let after_dash = trimmed[2..].trim_start();
                let (tag, command) = if let Some(rest) = after_dash.strip_prefix('[') {
                    match rest.split_once(']') {
                        Some((tag, cmd)) => (Some(tag), cmd.trim_start()),
                        None => (None, after_dash),
                    }
                } else {
                    (None, after_dash)
                };
                let command = command.to_string();
                if tag == Some("setup") {
                    // Prefer a proof entry: hold the setup command as a fallback
                    // and keep scanning for a proof entry in this section.
                    setup_fallback.get_or_insert(command);
                    continue;
                }
                return Some(command);
            }
            if in_section && trimmed.is_empty() {
                break;
            }
        }
        setup_fallback
    }

    /// The output-cap parameter name differs by dialect: OpenAI's newer
    /// models reject max_tokens in favor of max_completion_tokens; xAI
    /// and Anthropic's compatibility layer speak max_tokens.
    fn token_cap_field(&self) -> &'static str {
        if self.base_url.contains("openai.com") || self.is_kimi_k3() {
            "max_completion_tokens"
        } else {
            "max_tokens"
        }
    }

    fn is_anthropic_native(&self) -> bool {
        self.provider_family == "anthropic" && self.base_url.contains("api.anthropic.com")
    }

    fn is_kimi_k3(&self) -> bool {
        self.provider_family == "moonshot" && self.model.to_ascii_lowercase().starts_with("kimi-k3")
    }

    fn provider_calls_enabled() -> bool {
        env_provider_flag_enabled("MDX_PROVIDER_CALLS_ENABLED", true)
            && env_provider_flag_enabled("MDX_FORGE_PROVIDER_CALLS_ENABLED", true)
    }

    fn try_routed_turn(
        &self,
        mut call: impl FnMut(&TurnClient) -> Result<ModelTurn, String>,
    ) -> Result<ModelTurn, String> {
        let mut failures = Vec::new();
        for client in std::iter::once(self).chain(self.fallbacks.iter()) {
            let started = std::time::Instant::now();
            match call(client) {
                Ok(mut turn) => {
                    turn.route = client.route.clone();
                    return Ok(turn);
                }
                Err(error) => {
                    client.queue_failed_route_attempt(
                        &error,
                        started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
                        "forge_runtime_failure",
                    );
                    if !Self::permits_provider_failover(&error) {
                        return Err(error);
                    }
                    if let Some(route) = &client.route {
                        let _ = crate::model_fabric_registry::global().report_execution(
                            &route.tenant_id,
                            &route.deployment_id,
                            false,
                        );
                    }
                    failures.push(format!("{}: {error}", client.model));
                }
            }
        }
        Err(format!(
            "all eligible Forge model routes failed: {}",
            failures.join("; ")
        ))
    }

    fn queue_failed_route_attempt(&self, error: &str, latency_ms: u64, provenance: &str) {
        let Some(route) = &self.route else {
            return;
        };
        let _ = crate::model_fabric_registry::global().report_failed_attempt(
            &route.tenant_id,
            crate::model_fabric_registry::FailedInferenceAttempt {
                decision_id: route.decision_id.clone(),
                workload_id: route.workload_id.clone(),
                app_id: route.app_id.clone(),
                environment: route.environment.clone(),
                session_id: route.session_id.clone(),
                deployment_id: route.deployment_id.clone(),
                latency_ms,
                failure_class: crate::model_fabric_service::inference_failure_class(error)
                    .to_string(),
                provenance: provenance.to_string(),
            },
        );
    }

    fn permits_provider_failover(error: &str) -> bool {
        if error.contains("paused by operator") || error.contains("deadline reached") {
            return false;
        }
        let status = error
            .strip_prefix("status ")
            .and_then(|value| {
                value
                    .split(|character: char| !character.is_ascii_digit())
                    .next()
            })
            .and_then(|value| value.parse::<u16>().ok());
        status.is_none_or(|status| status == 408 || status == 409 || status == 429 || status >= 500)
    }

    /// One plain completion - no tools. The planner and reviewer roles
    /// speak in text (the planner's text IS the plan); only the builder
    /// uses the tool-calling turn loop.
    pub(crate) fn complete(&self, messages: &[serde_json::Value]) -> Result<ModelTurn, String> {
        self.try_routed_turn(|client| client.complete_one(messages))
    }

    fn complete_one(&self, messages: &[serde_json::Value]) -> Result<ModelTurn, String> {
        if self.is_deterministic() {
            return Ok(self.deterministic_completion(messages));
        }
        if self.is_anthropic_native() {
            return self.complete_anthropic(messages);
        }
        let mut body = serde_json::json!({
            "model": self.model,
            "messages": messages,
        });
        body[self.token_cap_field()] = serde_json::json!(self.max_output_tokens);
        self.apply_reasoning_effort(&mut body);
        let parsed = self.post_chat(&body)?;
        self.openai_turn_from_response(parsed)
    }

    fn openai_turn_from_response(&self, parsed: serde_json::Value) -> Result<ModelTurn, String> {
        let message = parsed["choices"][0]["message"].clone();
        if message.is_null() {
            return Err("empty completion message".to_string());
        }
        let tool_calls = message["tool_calls"]
            .as_array()
            .map(|calls| {
                calls
                    .iter()
                    .filter_map(|call| {
                        let arguments_raw = call["function"]["arguments"].as_str().unwrap_or("{}");
                        Some(ParsedToolCall {
                            call_id: call["id"].as_str()?.to_string(),
                            name: call["function"]["name"].as_str()?.to_string(),
                            arguments: serde_json::from_str(arguments_raw)
                                .unwrap_or(serde_json::json!({})),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(ModelTurn {
            text: message["content"].as_str().unwrap_or("").to_string(),
            assistant_message: message,
            tool_calls,
            finish_reason: parsed["choices"][0]["finish_reason"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            model_id: parsed["model"].as_str().unwrap_or(&self.model).to_string(),
            inference_id: parsed["id"]
                .as_str()
                .unwrap_or("live_inference")
                .to_string(),
            input_tokens: parsed["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
            output_tokens: parsed["usage"]["completion_tokens"].as_u64().unwrap_or(0),
            route: None,
        })
    }

    /// One embedding for a piece of text through the OpenAI-compatible
    /// /embeddings endpoint (Ollama's local server speaks it). Used only by
    /// the boundary-locked memory embedder: the resolver guarantees this
    /// client is loopback or provably sovereign, so the vector never leaves
    /// the box. Returns the vector, or an error the caller treats as
    /// "no embedding" (fail closed to the lexical path).
    pub(crate) fn embed(&self, input: &str) -> Result<Vec<f32>, String> {
        if self.is_deterministic() {
            return Err("deterministic provider does not embed".to_string());
        }
        if !Self::provider_calls_enabled() {
            return Err("provider calls are paused by operator runtime control".to_string());
        }
        let body = serde_json::json!({ "model": self.model, "input": input });
        let response = provider_request(http_agent().post(&format!(
            "{}/embeddings",
            self.base_url.trim_end_matches('/')
        )))
        .set("Authorization", &format!("Bearer {}", self.api_key))
        .set("Content-Type", "application/json")
        .send_string(&body.to_string())
        .map_err(|error| format!("embeddings transport: {error}"))?;
        let parsed: serde_json::Value = response
            .into_json()
            .map_err(|error| format!("embeddings parse: {error}"))?;
        let vector = parsed["data"][0]["embedding"]
            .as_array()
            .ok_or_else(|| "embeddings response missing data[0].embedding".to_string())?
            .iter()
            .map(|value| value.as_f64().unwrap_or(0.0) as f32)
            .collect::<Vec<f32>>();
        if vector.is_empty() {
            return Err("embeddings response returned an empty vector".to_string());
        }
        Ok(vector)
    }

    fn complete_anthropic(&self, messages: &[serde_json::Value]) -> Result<ModelTurn, String> {
        let (system, anthropic_messages) = anthropic_system_and_messages(messages);
        let mut body = serde_json::json!({
            "model": self.model,
            "max_tokens": self.max_output_tokens,
            "messages": anthropic_messages,
        });
        apply_anthropic_prompt_caching(&mut body, system);
        self.anthropic_turn_from_response(self.post_anthropic_messages(&body)?)
    }

    /// Backoff schedule for transient provider errors (429, 5xx, 529, transport).
    /// Longer than a token three-step pause: same-provider fan-out (many parallel
    /// runs) can stay rate-limited for a while, and the earlier [2,8,20] gave up
    /// after ~30s, killing whole runs on a burst of 429s. Five growing steps reach
    /// ~90s of patience before failing honestly.
    const RETRY_BACKOFF_SECS: [u64; 5] = [2, 5, 12, 25, 45];

    /// Backoff one retry with additive jitter so parallel retriers do not
    /// re-collide in lockstep. The sleep is capped by the model-turn deadline,
    /// so the worker cannot quietly continue after the watchdog has returned.
    fn sleep_backoff(base_secs: u64, retry_after: Option<&str>) -> bool {
        let mut delay = retry_after
            .and_then(Self::parse_retry_after_seconds)
            .unwrap_or_else(|| {
                let nanos = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.subsec_nanos() as u64)
                    .unwrap_or(0);
                let jitter_ms = if base_secs > 0 {
                    nanos % (base_secs * 500 + 1)
                } else {
                    0
                };
                Duration::from_millis(base_secs * 1000 + jitter_ms)
            });
        if let Some(remaining) = model_turn_deadline_remaining() {
            if remaining <= MODEL_TURN_DEADLINE_SAFETY_MARGIN {
                return false;
            }
            delay = delay.min(remaining.saturating_sub(MODEL_TURN_DEADLINE_SAFETY_MARGIN));
        }
        std::thread::sleep(delay);
        true
    }

    fn parse_retry_after_seconds(value: &str) -> Option<Duration> {
        let seconds = value.trim().parse::<u64>().ok()?;
        Some(Duration::from_secs(seconds.min(MAX_RETRY_AFTER_SECS)))
    }

    fn ensure_deadline_allows_provider_attempt() -> Result<(), String> {
        if model_turn_deadline_expired() {
            return Err("model turn deadline reached before provider retry".to_string());
        }
        Ok(())
    }

    fn retry_after_header(response: &ureq::Response) -> Option<String> {
        response.header("Retry-After").map(str::to_string)
    }

    /// One POST to the chat endpoint with retry. Twelve parallel
    /// benchmark runs taught the lesson live: same-provider fan-out hits
    /// rate limits, and a single 429 was killing a whole run. Transient
    /// statuses (429, 5xx, Anthropic's 529) and transport errors retry
    /// with growing pauses; anything else fails fast and honestly.
    fn post_chat(&self, body: &serde_json::Value) -> Result<serde_json::Value, String> {
        if !Self::provider_calls_enabled() {
            return Err("provider calls are paused by operator runtime control".to_string());
        }
        // One initial attempt plus one retry per pause.
        let mut backoff = Self::RETRY_BACKOFF_SECS.iter();
        let last_error;
        loop {
            Self::ensure_deadline_allows_provider_attempt()?;
            let response =
                provider_request(http_agent().post(&format!("{}/chat/completions", self.base_url)))
                    .set("Authorization", &format!("Bearer {}", self.api_key))
                    .set("Content-Type", "application/json")
                    .send_string(&body.to_string());
            let mut retry_after = None;
            match response {
                Ok(response) => {
                    return response
                        .into_json()
                        .map_err(|error| format!("parse: {error}"));
                }
                Err(ureq::Error::Status(code, response))
                    if matches!(code, 429 | 500 | 502 | 503 | 529) =>
                {
                    retry_after = Self::retry_after_header(&response);
                    if backoff.len() == 0 {
                        last_error = format!("status {code}");
                        break;
                    }
                }
                Err(ureq::Error::Status(code, response)) => {
                    let detail = response.into_string().unwrap_or_default();
                    return Err(format!(
                        "status {code}: {}",
                        detail.chars().take(300).collect::<String>()
                    ));
                }
                Err(error) => {
                    if backoff.len() == 0 {
                        last_error = format!("transport: {error}");
                        break;
                    }
                }
            }
            if let Some(seconds) = backoff.next()
                && !Self::sleep_backoff(*seconds, retry_after.as_deref())
            {
                last_error = "model turn deadline reached before provider retry".to_string();
                break;
            }
        }
        Err(format!("{last_error} after retries"))
    }

    fn post_anthropic_messages(
        &self,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        if !Self::provider_calls_enabled() {
            return Err("provider calls are paused by operator runtime control".to_string());
        }
        let mut backoff = Self::RETRY_BACKOFF_SECS.iter();
        let last_error;
        loop {
            Self::ensure_deadline_allows_provider_attempt()?;
            let response = provider_request(
                http_agent().post(&format!("{}/messages", self.base_url.trim_end_matches('/'))),
            )
            .set("x-api-key", &self.api_key)
            .set("anthropic-version", "2023-06-01")
            .set("Content-Type", "application/json")
            .send_string(&body.to_string());
            let mut retry_after = None;
            match response {
                Ok(response) => {
                    return response
                        .into_json()
                        .map_err(|error| format!("parse: {error}"));
                }
                Err(ureq::Error::Status(code, response))
                    if matches!(code, 429 | 500 | 502 | 503 | 529) =>
                {
                    retry_after = Self::retry_after_header(&response);
                    if backoff.len() == 0 {
                        last_error = format!("status {code}");
                        break;
                    }
                }
                Err(ureq::Error::Status(code, response)) => {
                    let detail = response.into_string().unwrap_or_default();
                    return Err(format!(
                        "status {code}: {}",
                        detail.chars().take(300).collect::<String>()
                    ));
                }
                Err(error) => {
                    if backoff.len() == 0 {
                        last_error = format!("transport: {error}");
                        break;
                    }
                }
            }
            if let Some(seconds) = backoff.next()
                && !Self::sleep_backoff(*seconds, retry_after.as_deref())
            {
                last_error = "model turn deadline reached before provider retry".to_string();
                break;
            }
        }
        Err(format!("{last_error} after retries"))
    }

    fn post_chat_streaming(
        &self,
        body: &serde_json::Value,
        on_event: &mut dyn FnMut(&str) -> Result<(), String>,
    ) -> Result<(), String> {
        if !Self::provider_calls_enabled() {
            return Err("provider calls are paused by operator runtime control".to_string());
        }
        let mut backoff = Self::RETRY_BACKOFF_SECS.iter();
        let last_error;
        loop {
            Self::ensure_deadline_allows_provider_attempt()?;
            let response =
                provider_request(http_agent().post(&format!("{}/chat/completions", self.base_url)))
                    .set("Authorization", &format!("Bearer {}", self.api_key))
                    .set("Content-Type", "application/json")
                    .set("Accept", "text/event-stream")
                    .send_string(&body.to_string());
            let mut retry_after = None;
            match response {
                Ok(response) => return read_sse_response(response, on_event),
                Err(ureq::Error::Status(code, response))
                    if matches!(code, 429 | 500 | 502 | 503 | 529) =>
                {
                    retry_after = Self::retry_after_header(&response);
                    if backoff.len() == 0 {
                        last_error = format!("status {code}");
                        break;
                    }
                }
                Err(ureq::Error::Status(code, response)) => {
                    let detail = response.into_string().unwrap_or_default();
                    return Err(format!(
                        "status {code}: {}",
                        detail.chars().take(300).collect::<String>()
                    ));
                }
                Err(error) => {
                    if backoff.len() == 0 {
                        last_error = format!("transport: {error}");
                        break;
                    }
                }
            }
            if let Some(seconds) = backoff.next()
                && !Self::sleep_backoff(*seconds, retry_after.as_deref())
            {
                last_error = "model turn deadline reached before provider retry".to_string();
                break;
            }
        }
        Err(format!("{last_error} after retries"))
    }

    fn post_anthropic_messages_streaming(
        &self,
        body: &serde_json::Value,
        on_event: &mut dyn FnMut(&str) -> Result<(), String>,
    ) -> Result<(), String> {
        if !Self::provider_calls_enabled() {
            return Err("provider calls are paused by operator runtime control".to_string());
        }
        let mut backoff = Self::RETRY_BACKOFF_SECS.iter();
        let last_error;
        loop {
            Self::ensure_deadline_allows_provider_attempt()?;
            let response = provider_request(
                http_agent().post(&format!("{}/messages", self.base_url.trim_end_matches('/'))),
            )
            .set("x-api-key", &self.api_key)
            .set("anthropic-version", "2023-06-01")
            .set("Content-Type", "application/json")
            .set("Accept", "text/event-stream")
            .send_string(&body.to_string());
            let mut retry_after = None;
            match response {
                Ok(response) => return read_sse_response(response, on_event),
                Err(ureq::Error::Status(code, response))
                    if matches!(code, 429 | 500 | 502 | 503 | 529) =>
                {
                    retry_after = Self::retry_after_header(&response);
                    if backoff.len() == 0 {
                        last_error = format!("status {code}");
                        break;
                    }
                }
                Err(ureq::Error::Status(code, response)) => {
                    let detail = response.into_string().unwrap_or_default();
                    return Err(format!(
                        "status {code}: {}",
                        detail.chars().take(300).collect::<String>()
                    ));
                }
                Err(error) => {
                    if backoff.len() == 0 {
                        last_error = format!("transport: {error}");
                        break;
                    }
                }
            }
            if let Some(seconds) = backoff.next()
                && !Self::sleep_backoff(*seconds, retry_after.as_deref())
            {
                last_error = "model turn deadline reached before provider retry".to_string();
                break;
            }
        }
        Err(format!("{last_error} after retries"))
    }

    /// One model turn: send the conversation, get back text and tool
    /// calls. The messages array is owned by the loop runner; this client
    /// never stores conversation state.
    pub(crate) fn turn(&self, messages: &[serde_json::Value]) -> Result<ModelTurn, String> {
        self.turn_with_tools(messages, None)
    }

    /// A turn with an optional OpenAI-format tool-schema override. None uses
    /// the model's default coding surface; the subagent path passes a
    /// restricted read-only schema so an investigator can explore but never
    /// edit, write, or run checks - the parent keeps sole write authority.
    pub(crate) fn turn_with_tools(
        &self,
        messages: &[serde_json::Value],
        tools_override: Option<&serde_json::Value>,
    ) -> Result<ModelTurn, String> {
        self.try_routed_turn(|client| client.turn_with_tools_one(messages, tools_override))
    }

    fn turn_with_tools_one(
        &self,
        messages: &[serde_json::Value],
        tools_override: Option<&serde_json::Value>,
    ) -> Result<ModelTurn, String> {
        if self.is_deterministic() {
            return Ok(self.deterministic_turn(messages));
        }
        if self.is_anthropic_native() {
            return self.turn_anthropic(messages, tools_override);
        }
        let tools = tools_override
            .cloned()
            .unwrap_or_else(|| coding_tool_schema_for(self.tool_surface()));
        let mut body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "tools": tools,
        });
        body[self.token_cap_field()] = serde_json::json!(self.max_output_tokens);
        self.apply_reasoning_effort(&mut body);
        let parsed = self.post_chat(&body)?;
        self.openai_turn_from_response(parsed)
    }

    /// One model turn with provider streaming when available. Streaming is a
    /// live-view affordance only: the caller still receives the same final
    /// ModelTurn shape, and durable receipts are recorded by the worker loop.
    pub(crate) fn turn_streaming(
        &self,
        messages: &[serde_json::Value],
        on_delta: &mut dyn FnMut(&str),
    ) -> Result<ModelTurn, String> {
        if self.is_deterministic() {
            let turn = self.deterministic_turn(messages);
            on_delta(&turn.text);
            return Ok(turn);
        }
        // K3 requires every assistant response, including reasoning_content,
        // to be replayed unchanged across later tool turns. The generic SSE
        // accumulator intentionally normalizes to text and tool calls, so use
        // the non-streaming response object until streaming preserves every
        // provider field losslessly.
        if self.is_kimi_k3() {
            let turn = self.turn(messages)?;
            on_delta(&turn.text);
            return Ok(turn);
        }
        // Extended thinking (Claude) is wired on the non-streaming path, where
        // thinking blocks are captured and replayed correctly. Rather than
        // accumulate thinking blocks from the SSE stream (fiddly and easy to
        // get wrong on the load-bearing path), route thinking-enabled Anthropic
        // turns through the non-streaming path. Live token deltas are a UX
        // nicety; correctness of the reasoning turn matters more.
        if self.is_anthropic_native() && self.anthropic_thinking_effort().is_some() {
            return self.turn(messages);
        }
        let emitted_meaningful_output = std::cell::Cell::new(false);
        let mut sink = |delta: &str| {
            if !delta.is_empty() {
                emitted_meaningful_output.set(true);
            }
            on_delta(delta);
        };
        let started = std::time::Instant::now();
        let result = if self.is_anthropic_native() {
            self.turn_anthropic_streaming(messages, &mut sink, &emitted_meaningful_output)
        } else {
            self.turn_openai_streaming(messages, &mut sink, &emitted_meaningful_output)
        };
        match result {
            Ok(mut turn) => {
                turn.route = self.route.clone();
                Ok(turn)
            }
            Err(error) if !emitted_meaningful_output.get() => {
                self.queue_failed_route_attempt(
                    &error,
                    started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
                    "forge_stream_runtime_failure",
                );
                self.turn(messages).map_err(|fallback| {
                    format!("streaming failed before output ({error}); fallback failed: {fallback}")
                })
            }
            Err(error) => {
                self.queue_failed_route_attempt(
                    &error,
                    started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
                    "forge_stream_runtime_failure",
                );
                if let Some(route) = &self.route {
                    let _ = crate::model_fabric_registry::global().report_execution(
                        &route.tenant_id,
                        &route.deployment_id,
                        false,
                    );
                }
                Err(error)
            }
        }
    }

    fn turn_openai_streaming(
        &self,
        messages: &[serde_json::Value],
        on_delta: &mut dyn FnMut(&str),
        emitted_meaningful_output: &std::cell::Cell<bool>,
    ) -> Result<ModelTurn, String> {
        let mut body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "tools": coding_tool_schema_for(self.tool_surface()),
            "stream": true,
            // OpenAI-wire streaming (xAI/grok included) emits a usage block only
            // when the request asks for it. Without this, every streamed run
            // reports zero tokens and meters zero cost, which makes the enforced
            // per-run cost budget toothless. Anthropic streaming carries usage
            // natively, so this only affects the OpenAI-compatible path.
            "stream_options": { "include_usage": true },
        });
        body[self.token_cap_field()] = serde_json::json!(self.max_output_tokens);
        self.apply_reasoning_effort(&mut body);

        let mut text = String::new();
        let mut tool_calls: Vec<StreamingToolCall> = Vec::new();
        let mut finish_reason = String::new();
        let mut model_id = self.model.clone();
        let mut inference_id = "live_inference".to_string();
        let mut input_tokens = 0;
        let mut output_tokens = 0;

        self.post_chat_streaming(&body, &mut |event| {
            if event.trim() == "[DONE]" {
                return Ok(());
            }
            let parsed: serde_json::Value =
                serde_json::from_str(event).map_err(|error| format!("stream parse: {error}"))?;
            on_delta("");
            if let Some(error) = parsed.get("error") {
                return Err(format!("stream error: {error}"));
            }
            if let Some(id) = parsed["id"].as_str() {
                inference_id = id.to_string();
            }
            if let Some(model) = parsed["model"].as_str() {
                model_id = model.to_string();
            }
            if let Some(usage) = parsed.get("usage") {
                input_tokens = usage["prompt_tokens"].as_u64().unwrap_or(input_tokens);
                output_tokens = usage["completion_tokens"].as_u64().unwrap_or(output_tokens);
            }
            for choice in parsed["choices"].as_array().cloned().unwrap_or_default() {
                if let Some(reason) = choice["finish_reason"].as_str()
                    && !reason.is_empty()
                {
                    finish_reason = reason.to_string();
                }
                let delta = &choice["delta"];
                if let Some(content) = delta["content"].as_str() {
                    text.push_str(content);
                    on_delta(content);
                }
                if let Some(calls) = delta["tool_calls"].as_array() {
                    if calls.iter().any(openai_tool_delta_has_meaningful_output) {
                        emitted_meaningful_output.set(true);
                    }
                    merge_openai_tool_deltas(&mut tool_calls, calls);
                }
            }
            Ok(())
        })?;

        Ok(model_turn_from_parts(
            text,
            tool_calls,
            finish_reason,
            model_id,
            inference_id,
            input_tokens,
            output_tokens,
        ))
    }

    fn turn_anthropic_streaming(
        &self,
        messages: &[serde_json::Value],
        on_delta: &mut dyn FnMut(&str),
        emitted_meaningful_output: &std::cell::Cell<bool>,
    ) -> Result<ModelTurn, String> {
        let (system, anthropic_messages) = anthropic_system_and_messages(messages);
        let mut body = serde_json::json!({
            "model": self.model,
            "max_tokens": self.max_output_tokens,
            "messages": anthropic_messages,
            "tools": anthropic_tool_schema(),
            "tool_choice": {"type": "auto"},
            "stream": true,
        });
        apply_anthropic_prompt_caching(&mut body, system);

        let mut text = String::new();
        let mut tool_blocks: Vec<StreamingToolCall> = Vec::new();
        let mut finish_reason = String::new();
        let mut model_id = self.model.clone();
        let mut inference_id = "live_inference".to_string();
        let mut input_tokens = 0;
        let mut output_tokens = 0;

        self.post_anthropic_messages_streaming(&body, &mut |event| {
            let parsed: serde_json::Value = serde_json::from_str(event)
                .map_err(|error| format!("anthropic stream parse: {error}"))?;
            on_delta("");
            match parsed["type"].as_str().unwrap_or("") {
                "message_start" => {
                    if let Some(id) = parsed["message"]["id"].as_str() {
                        inference_id = id.to_string();
                    }
                    if let Some(model) = parsed["message"]["model"].as_str() {
                        model_id = model.to_string();
                    }
                    input_tokens = parsed["message"]["usage"]["input_tokens"]
                        .as_u64()
                        .unwrap_or(0);
                }
                "content_block_start" => {
                    let index = parsed["index"].as_u64().unwrap_or(0) as usize;
                    ensure_tool_call_slot(&mut tool_blocks, index);
                    let block = &parsed["content_block"];
                    if block["type"].as_str() == Some("tool_use") {
                        emitted_meaningful_output.set(true);
                        tool_blocks[index].id = block["id"].as_str().unwrap_or("").to_string();
                        tool_blocks[index].name = block["name"].as_str().unwrap_or("").to_string();
                    }
                }
                "content_block_delta" => {
                    let delta = &parsed["delta"];
                    match delta["type"].as_str().unwrap_or("") {
                        "text_delta" => {
                            if let Some(fragment) = delta["text"].as_str() {
                                text.push_str(fragment);
                                on_delta(fragment);
                            }
                        }
                        "input_json_delta" => {
                            let index = parsed["index"].as_u64().unwrap_or(0) as usize;
                            ensure_tool_call_slot(&mut tool_blocks, index);
                            if let Some(fragment) = delta["partial_json"].as_str() {
                                if !fragment.is_empty() {
                                    emitted_meaningful_output.set(true);
                                }
                                tool_blocks[index].arguments.push_str(fragment);
                            }
                        }
                        _ => {}
                    }
                }
                "message_delta" => {
                    if let Some(reason) = parsed["delta"]["stop_reason"].as_str() {
                        finish_reason = reason.to_string();
                    }
                    output_tokens = parsed["usage"]["output_tokens"]
                        .as_u64()
                        .unwrap_or(output_tokens);
                }
                "error" => return Err(format!("anthropic stream error: {}", parsed["error"])),
                _ => {}
            }
            Ok(())
        })?;

        tool_blocks.retain(|call| !call.id.is_empty() && !call.name.is_empty());
        Ok(model_turn_from_parts(
            text,
            tool_blocks,
            finish_reason,
            model_id,
            inference_id,
            input_tokens,
            output_tokens,
        ))
    }

    fn turn_anthropic(
        &self,
        messages: &[serde_json::Value],
        tools_override: Option<&serde_json::Value>,
    ) -> Result<ModelTurn, String> {
        let (system, anthropic_messages) = anthropic_system_and_messages(messages);
        let tools = tools_override
            .map(anthropic_tools_from)
            .unwrap_or_else(anthropic_tool_schema);
        let mut body = serde_json::json!({
            "model": self.model,
            "max_tokens": self.max_output_tokens,
            "messages": anthropic_messages,
            "tools": tools,
            "tool_choice": {"type": "auto"},
        });
        // Extended thinking (Claude's reasoning dial). Opt-in via the same
        // MDX_FORGE_REASONING_EFFORT knob; unset means no change. The
        // 4.8/5-generation models use adaptive thinking + output_config.effort.
        if let Some(effort) = self.anthropic_thinking_effort() {
            body["thinking"] = serde_json::json!({ "type": "adaptive" });
            body["output_config"] = serde_json::json!({ "effort": effort });
        }
        apply_anthropic_prompt_caching(&mut body, system);
        self.anthropic_turn_from_response(self.post_anthropic_messages(&body)?)
    }

    fn anthropic_turn_from_response(&self, parsed: serde_json::Value) -> Result<ModelTurn, String> {
        let content = parsed["content"].as_array().cloned().unwrap_or_default();
        let text = content
            .iter()
            .filter(|block| block["type"].as_str() == Some("text"))
            .filter_map(|block| block["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let tool_calls = content
            .iter()
            .filter(|block| block["type"].as_str() == Some("tool_use"))
            .filter_map(|block| {
                Some(ParsedToolCall {
                    call_id: block["id"].as_str()?.to_string(),
                    name: block["name"].as_str()?.to_string(),
                    arguments: block["input"].clone(),
                })
            })
            .collect::<Vec<_>>();
        let mut assistant_message = serde_json::json!({
            "role": "assistant",
            "content": text,
        });
        // Extended-thinking blocks (thinking / redacted_thinking) must be
        // preserved verbatim and replayed at the FRONT of this assistant turn
        // when tool results follow, or the API rejects the next request. Stash
        // them so the message conversion can re-emit them in order.
        let thinking_blocks: Vec<serde_json::Value> = content
            .iter()
            .filter(|block| {
                matches!(
                    block["type"].as_str(),
                    Some("thinking") | Some("redacted_thinking")
                )
            })
            .cloned()
            .collect();
        if !thinking_blocks.is_empty() {
            assistant_message["thinking_blocks"] = serde_json::json!(thinking_blocks);
        }
        if !tool_calls.is_empty() {
            assistant_message["tool_calls"] = serde_json::json!(
                tool_calls
                    .iter()
                    .map(|call| serde_json::json!({
                        "id": call.call_id,
                        "type": "function",
                        "function": {
                            "name": call.name,
                            "arguments": call.arguments.to_string(),
                        }
                    }))
                    .collect::<Vec<_>>()
            );
        }
        Ok(ModelTurn {
            assistant_message,
            text,
            tool_calls,
            finish_reason: parsed["stop_reason"].as_str().unwrap_or("").to_string(),
            model_id: parsed["model"].as_str().unwrap_or(&self.model).to_string(),
            inference_id: parsed["id"]
                .as_str()
                .unwrap_or("live_inference")
                .to_string(),
            input_tokens: parsed["usage"]["input_tokens"].as_u64().unwrap_or(0),
            output_tokens: parsed["usage"]["output_tokens"].as_u64().unwrap_or(0),
            route: None,
        })
    }
}

fn read_sse_response(
    response: ureq::Response,
    on_event: &mut dyn FnMut(&str) -> Result<(), String>,
) -> Result<(), String> {
    let reader = BufReader::new(response.into_reader());
    let mut data_lines: Vec<String> = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(|error| format!("stream read: {error}"))?;
        let trimmed = line.trim_end_matches('\r');
        if trimmed.is_empty() {
            flush_sse_event(&mut data_lines, on_event)?;
            continue;
        }
        if trimmed.starts_with(':') || trimmed.starts_with("event:") || trimmed.starts_with("id:") {
            continue;
        }
        if let Some(data) = trimmed.strip_prefix("data:") {
            data_lines.push(data.trim_start().to_string());
        }
    }
    flush_sse_event(&mut data_lines, on_event)
}

fn flush_sse_event(
    data_lines: &mut Vec<String>,
    on_event: &mut dyn FnMut(&str) -> Result<(), String>,
) -> Result<(), String> {
    if data_lines.is_empty() {
        return Ok(());
    }
    let event = data_lines.join("\n");
    data_lines.clear();
    on_event(&event)
}

fn openai_tool_delta_has_meaningful_output(delta: &serde_json::Value) -> bool {
    delta["function"]["name"]
        .as_str()
        .map(|value| !value.is_empty())
        .unwrap_or(false)
        || delta["function"]["arguments"]
            .as_str()
            .map(|value| !value.is_empty())
            .unwrap_or(false)
}

fn merge_openai_tool_deltas(tool_calls: &mut Vec<StreamingToolCall>, deltas: &[serde_json::Value]) {
    for delta in deltas {
        let index = delta["index"].as_u64().unwrap_or(tool_calls.len() as u64) as usize;
        ensure_tool_call_slot(tool_calls, index);
        if let Some(id) = delta["id"].as_str()
            && !id.is_empty()
        {
            tool_calls[index].id = id.to_string();
        }
        if let Some(name) = delta["function"]["name"].as_str()
            && !name.is_empty()
        {
            tool_calls[index].name = name.to_string();
        }
        if let Some(arguments) = delta["function"]["arguments"].as_str() {
            tool_calls[index].arguments.push_str(arguments);
        }
    }
}

fn ensure_tool_call_slot(tool_calls: &mut Vec<StreamingToolCall>, index: usize) {
    while tool_calls.len() <= index {
        tool_calls.push(StreamingToolCall::default());
    }
}

fn model_turn_from_parts(
    text: String,
    tool_call_parts: Vec<StreamingToolCall>,
    finish_reason: String,
    model_id: String,
    inference_id: String,
    input_tokens: u64,
    output_tokens: u64,
) -> ModelTurn {
    let tool_calls = tool_call_parts
        .into_iter()
        .filter(|call| !call.id.is_empty() && !call.name.is_empty())
        .map(|call| ParsedToolCall {
            call_id: call.id,
            name: call.name,
            arguments: serde_json::from_str(&call.arguments).unwrap_or(serde_json::json!({})),
        })
        .collect::<Vec<_>>();

    let mut assistant_message = serde_json::json!({
        "role": "assistant",
        "content": text.clone(),
    });
    if !tool_calls.is_empty() {
        assistant_message["tool_calls"] = serde_json::json!(
            tool_calls
                .iter()
                .map(|call| serde_json::json!({
                    "id": call.call_id,
                    "type": "function",
                    "function": {
                        "name": call.name,
                        "arguments": call.arguments.to_string(),
                    }
                }))
                .collect::<Vec<_>>()
        );
    }

    ModelTurn {
        assistant_message,
        text,
        tool_calls,
        finish_reason,
        model_id,
        inference_id,
        input_tokens,
        output_tokens,
        route: None,
    }
}

fn anthropic_tool_schema() -> serde_json::Value {
    anthropic_tools_from(&coding_tool_schema())
}

/// Convert any OpenAI-format tool array to Anthropic's tool schema. Used for
/// the full coding surface and for the subagent's restricted read-only set.
fn anthropic_tools_from(openai_schema: &serde_json::Value) -> serde_json::Value {
    let tools = openai_schema
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|tool| {
            let function = &tool["function"];
            let name = function["name"].as_str()?;
            Some(serde_json::json!({
                "name": name,
                "description": function["description"].as_str().unwrap_or(""),
                "input_schema": anthropic_strict_input_schema(&function["parameters"]),
                "strict": true,
            }))
        })
        .collect::<Vec<_>>();
    serde_json::json!(tools)
}

fn anthropic_strict_input_schema(schema: &serde_json::Value) -> serde_json::Value {
    let mut strict = schema.clone();
    close_object_schemas_for_anthropic(&mut strict);
    strict
}

fn close_object_schemas_for_anthropic(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            if map.get("type").and_then(|value| value.as_str()) == Some("object")
                && !map.contains_key("additionalProperties")
            {
                map.insert(
                    "additionalProperties".to_string(),
                    serde_json::Value::Bool(false),
                );
            }

            if let Some(properties) = map
                .get_mut("properties")
                .and_then(|value| value.as_object_mut())
            {
                for property_schema in properties.values_mut() {
                    close_object_schemas_for_anthropic(property_schema);
                }
            }

            for key in ["items", "additionalProperties", "not", "if", "then", "else"] {
                if let Some(child) = map.get_mut(key) {
                    close_object_schemas_for_anthropic(child);
                }
            }

            for key in ["oneOf", "anyOf", "allOf"] {
                if let Some(children) = map.get_mut(key).and_then(|value| value.as_array_mut()) {
                    for child in children {
                        close_object_schemas_for_anthropic(child);
                    }
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                close_object_schemas_for_anthropic(item);
            }
        }
        _ => {}
    }
}

/// Anthropic prompt caching is a prefix match with explicit breakpoints.
/// A bare top-level `cache_control` only marks the last cacheable block,
/// so the large stable prefix (tool schemas + system prompt) re-billed at
/// full input price on every harness turn. Pin two breakpoints instead:
/// the last system block (tools render before system, so this caches both
/// together) and the last content block of the final message (incremental
/// multi-turn reuse as the transcript grows).
fn apply_anthropic_prompt_caching(body: &mut serde_json::Value, system: Option<String>) {
    if let Some(system) = system {
        body["system"] = serde_json::json!([{
            "type": "text",
            "text": system,
            "cache_control": {"type": "ephemeral"},
        }]);
    }
    let Some(messages) = body["messages"].as_array_mut() else {
        return;
    };
    let Some(last) = messages.last_mut() else {
        return;
    };
    if let Some(text) = last["content"].as_str().map(str::to_string) {
        last["content"] = serde_json::json!([{
            "type": "text",
            "text": text,
            "cache_control": {"type": "ephemeral"},
        }]);
    } else if let Some(blocks) = last["content"].as_array_mut()
        && let Some(block) = blocks.last_mut()
    {
        block["cache_control"] = serde_json::json!({"type": "ephemeral"});
    }
}

fn anthropic_system_and_messages(
    messages: &[serde_json::Value],
) -> (Option<String>, Vec<serde_json::Value>) {
    let mut system = Vec::new();
    let mut out = Vec::new();
    for message in messages {
        match message["role"].as_str().unwrap_or("") {
            "system" => {
                let text = openai_message_text(message);
                if !text.trim().is_empty() {
                    system.push(text);
                }
            }
            "assistant" => {
                let mut blocks = Vec::new();
                // Preserved extended-thinking blocks come FIRST, verbatim, so a
                // thinking-enabled turn that made tool calls stays valid when
                // its tool results follow.
                if let Some(thinking) = message["thinking_blocks"].as_array() {
                    blocks.extend(thinking.iter().cloned());
                }
                let text = openai_message_text(message);
                if !text.trim().is_empty() {
                    blocks.push(serde_json::json!({"type": "text", "text": text}));
                }
                if let Some(tool_calls) = message["tool_calls"].as_array() {
                    for call in tool_calls {
                        let arguments_raw = call["function"]["arguments"].as_str().unwrap_or("{}");
                        let input = serde_json::from_str(arguments_raw)
                            .unwrap_or_else(|_| serde_json::json!({}));
                        if let (Some(id), Some(name)) =
                            (call["id"].as_str(), call["function"]["name"].as_str())
                        {
                            blocks.push(serde_json::json!({
                                "type": "tool_use",
                                "id": id,
                                "name": name,
                                "input": input,
                            }));
                        }
                    }
                }
                if !blocks.is_empty() {
                    push_anthropic_message(&mut out, "assistant", serde_json::json!(blocks));
                }
            }
            "tool" => {
                let call_id = message["tool_call_id"].as_str().unwrap_or("");
                let content = openai_message_text(message);
                if !call_id.is_empty() {
                    push_anthropic_message(
                        &mut out,
                        "user",
                        serde_json::json!([{
                            "type": "tool_result",
                            "tool_use_id": call_id,
                            "content": content,
                        }]),
                    );
                }
            }
            "user" => {
                if let Some(content) = anthropic_user_content(message) {
                    push_anthropic_message(&mut out, "user", content);
                }
            }
            _ => {}
        }
    }
    (
        if system.is_empty() {
            None
        } else {
            Some(system.join("\n\n"))
        },
        out,
    )
}

fn anthropic_user_content(message: &serde_json::Value) -> Option<serde_json::Value> {
    if let Some(text) = message["content"].as_str() {
        return (!text.trim().is_empty()).then(|| serde_json::json!(text));
    }
    let blocks = message["content"]
        .as_array()?
        .iter()
        .filter_map(|block| match block["type"].as_str() {
            Some("text") => block["text"]
                .as_str()
                .filter(|text| !text.trim().is_empty())
                .map(|text| serde_json::json!({"type": "text", "text": text})),
            Some("image_url") => {
                let url = block["image_url"]["url"].as_str()?;
                let encoded = url.strip_prefix("data:")?;
                let (media_type, data) = encoded.split_once(";base64,")?;
                matches!(
                    media_type,
                    "image/jpeg" | "image/png" | "image/gif" | "image/webp"
                )
                .then(|| {
                    serde_json::json!({
                        "type": "image",
                        "source": {
                            "type": "base64",
                            "media_type": media_type,
                            "data": data
                        }
                    })
                })
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    (!blocks.is_empty()).then(|| serde_json::json!(blocks))
}

fn openai_message_text(message: &serde_json::Value) -> String {
    if let Some(content) = message["content"].as_str() {
        return content.to_string();
    }
    message["content"]
        .as_array()
        .map(|blocks| {
            blocks
                .iter()
                .filter_map(|block| block["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn push_anthropic_message(
    out: &mut Vec<serde_json::Value>,
    role: &str,
    content: serde_json::Value,
) {
    if let Some(last) = out.last_mut()
        && last["role"].as_str() == Some(role)
    {
        let mut blocks = anthropic_content_blocks(&last["content"]);
        blocks.extend(anthropic_content_blocks(&content));
        last["content"] = serde_json::json!(blocks);
        return;
    }
    out.push(serde_json::json!({
        "role": role,
        "content": content,
    }));
}

fn anthropic_content_blocks(content: &serde_json::Value) -> Vec<serde_json::Value> {
    if let Some(text) = content.as_str() {
        if text.trim().is_empty() {
            Vec::new()
        } else {
            vec![serde_json::json!({"type": "text", "text": text})]
        }
    } else {
        content.as_array().cloned().unwrap_or_default()
    }
}

/// The tool-result message the loop sends back for one dispatched call.
pub(crate) fn tool_result_message(call_id: &str, content: &str) -> serde_json::Value {
    serde_json::json!({
        "role": "tool",
        "tool_call_id": call_id,
        "content": content
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdx_core::{ApproveModelTurnOn, TwinModelGatewayProviderObservation};

    #[test]
    fn coding_tool_schema_exposes_semantic_query_as_read_only_orientation_tool() {
        let schema = coding_tool_schema().to_string();
        assert!(schema.contains("\"semantic_query\""));
        assert!(schema.contains("\"orientation\""));
        assert!(schema.contains("\"workspace_symbol\""));
        assert!(schema.contains("\"definition\""));
        assert!(schema.contains("\"symbol_graph\""));
        assert!(schema.contains("\"file_outline\""));
        assert!(schema.contains("\"references\""));
        assert!(schema.contains("\"lsp_probe\""));
        assert!(schema.contains("It never writes files or marks checks passed"));
    }

    #[test]
    fn thinking_budget_maps_effort_and_conversion_preserves_blocks() {
        unsafe {
            std::env::remove_var("MDX_FORGE_REASONING_EFFORT");
        }
        assert_eq!(super::effort_to_anthropic("high").as_deref(), Some("high"));
        unsafe {
            std::env::set_var("MDX_FORGE_REASONING_EFFORT", "high");
        }
        assert_eq!(super::effort_to_anthropic("high").as_deref(), Some("high"));
        unsafe {
            std::env::remove_var("MDX_FORGE_REASONING_EFFORT");
        }
        // an assistant turn carrying thinking_blocks must replay them FIRST as
        // anthropic blocks, before text/tool_use.
        let messages = vec![serde_json::json!({
            "role": "assistant",
            "content": "the answer",
            "thinking_blocks": [{"type": "thinking", "thinking": "reasoning", "signature": "sig"}]
        })];
        let (_system, out) = super::anthropic_system_and_messages(&messages);
        let blocks = out[0]["content"].as_array().unwrap();
        assert_eq!(blocks[0]["type"].as_str(), Some("thinking"));
        assert_eq!(blocks[1]["type"].as_str(), Some("text"));
    }

    #[test]
    fn with_reasoning_effort_binds_per_run_and_ignores_garbage() {
        let base = TurnClient {
            provider_family: "xai".into(),
            base_url: "u".into(),
            api_key: "k".into(),
            model: "grok".into(),
            max_output_tokens: 4096,
            reasoning_effort: None,
            route: None,
            fallbacks: Vec::new(),
        };
        assert_eq!(
            base.clone()
                .with_reasoning_effort("high")
                .reasoning_effort
                .as_deref(),
            Some("high")
        );
        assert_eq!(
            base.clone().with_reasoning_effort("").reasoning_effort,
            None
        );
        assert_eq!(base.with_reasoning_effort("bogus").reasoning_effort, None);
    }

    #[test]
    fn routed_turn_uses_the_ordered_fallback_and_tracks_its_deployment() {
        let route = |deployment_id: &str| ForgeModelRoute {
            tenant_id: "fallback-test".to_string(),
            decision_id: "decision-1".to_string(),
            workload_id: "mdx/forge/builder".to_string(),
            app_id: "forge".to_string(),
            environment: "local".to_string(),
            session_id: "session-1".to_string(),
            deployment_id: deployment_id.to_string(),
            input_price: 0,
            output_price: 0,
        };
        let fallback = TurnClient {
            provider_family: "anthropic".to_string(),
            base_url: "https://api.anthropic.com/v1".to_string(),
            api_key: "test".to_string(),
            model: "fallback".to_string(),
            max_output_tokens: 32,
            reasoning_effort: None,
            route: Some(route("fallback-deployment")),
            fallbacks: Vec::new(),
        };
        let primary = TurnClient {
            provider_family: "openai".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: "test".to_string(),
            model: "primary".to_string(),
            max_output_tokens: 32,
            reasoning_effort: None,
            route: Some(route("primary-deployment")),
            fallbacks: vec![fallback],
        };
        let turn = primary
            .try_routed_turn(|client| {
                if client.model == "primary" {
                    Err("retryable provider failure".to_string())
                } else {
                    Ok(model_turn_from_parts(
                        "done".to_string(),
                        Vec::new(),
                        "stop".to_string(),
                        client.model.clone(),
                        "inference-1".to_string(),
                        1,
                        1,
                    ))
                }
            })
            .expect("fallback turn");
        assert_eq!(
            turn.route
                .as_ref()
                .map(|route| route.deployment_id.as_str()),
            Some("fallback-deployment")
        );
    }

    #[test]
    fn configured_model_fabric_denial_is_not_a_legacy_fallback_signal() {
        let _env = ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        unsafe {
            std::env::remove_var("MDX_FORGE_DETERMINISTIC_PROVIDER");
        }
        let tenant_id = "forge-binding-denial-tenant";
        let registry = crate::model_fabric_registry::global();
        registry.clear_tenant(tenant_id);
        let mut kernel = MdxKernel::boot_local();
        assert!(
            TurnClient::routed_builder_for_tenant(
                &mut kernel,
                tenant_id,
                "agent:test",
                "session-test",
                "",
            )
            .expect("unconfigured fabric")
            .is_none(),
            "only an entirely unconfigured tenant may ask the caller for legacy routing"
        );
        registry
            .upsert_connection(mdx_core::ProviderConnection {
                connection_id: format!("{tenant_id}:ollama"),
                tenant_id: tenant_id.to_string(),
                provider_id: "ollama".to_string(),
                credential_ref: "local:none".to_string(),
                endpoint_base_url: "http://127.0.0.1:11434/v1".to_string(),
                region: "local".to_string(),
                residency: "local".to_string(),
                data_retention: "none".to_string(),
                training_policy: "none".to_string(),
                data_policy_provenance: "test".to_string(),
                data_policy_observed_at: "2026-07-13".to_string(),
                health: "disconnected".to_string(),
                live_call_allowed: false,
            })
            .expect("configured fabric connection");
        let error = match TurnClient::routed_builder_for_tenant(
            &mut kernel,
            tenant_id,
            "agent:test",
            "session-test",
            "",
        ) {
            Err(error) => error,
            Ok(_) => panic!("configured route denial must bind"),
        };
        assert!(error.contains("model fabric denied builder route"));
        assert!(
            kernel
                .ledger()
                .query()
                .by_kind(mdx_core::MODEL_ROUTE_DENIED_RECEIPT_KIND)
                .iter()
                .any(|receipt| receipt.tenant_id.as_str() == tenant_id)
        );
        registry.clear_tenant(tenant_id);
    }

    #[test]
    fn reasoning_effort_is_only_sent_to_supported_openai_wire_models() {
        let xai = TurnClient {
            provider_family: "xai".into(),
            base_url: "https://api.x.ai/v1".into(),
            api_key: "k".into(),
            model: "grok-build-0.1".into(),
            max_output_tokens: 4096,
            reasoning_effort: Some("high".into()),
            route: None,
            fallbacks: Vec::new(),
        };
        let mut xai_body = serde_json::json!({ "model": "grok-build-0.1" });
        xai.apply_reasoning_effort(&mut xai_body);
        assert!(xai_body.get("reasoning_effort").is_none());

        let openai = TurnClient {
            provider_family: "openai".into(),
            base_url: "https://api.openai.com/v1".into(),
            api_key: "k".into(),
            model: "gpt-5.5".into(),
            max_output_tokens: 4096,
            reasoning_effort: Some("high".into()),
            route: None,
            fallbacks: Vec::new(),
        };
        let mut openai_body = serde_json::json!({ "model": "gpt-5.5" });
        openai.apply_reasoning_effort(&mut openai_body);
        assert_eq!(openai_body["reasoning_effort"].as_str(), Some("high"));
    }

    #[test]
    fn kimi_k3_uses_max_reasoning_and_preserves_the_complete_assistant_turn() {
        let client = TurnClient {
            provider_family: "moonshot".into(),
            base_url: "https://api.moonshot.ai/v1".into(),
            api_key: "test".into(),
            model: "kimi-k3".into(),
            max_output_tokens: default_max_output_tokens("kimi-k3"),
            reasoning_effort: Some("low".into()),
            route: None,
            fallbacks: Vec::new(),
        };
        let assistant_message = serde_json::json!({
            "role": "assistant",
            "reasoning_content": "inspect the manifest before editing",
            "content": "",
            "tool_calls": [{
                "id": "call_1",
                "type": "function",
                "function": {
                    "name": "read_file",
                    "arguments": "{\"path\":\"Cargo.toml\"}"
                }
            }]
        });
        let turn = client
            .openai_turn_from_response(serde_json::json!({
                "id": "cmpl-k3",
                "model": "kimi-k3",
                "choices": [{"message": assistant_message.clone(), "finish_reason": "tool_calls"}],
                "usage": {"prompt_tokens": 11, "completion_tokens": 7}
            }))
            .expect("K3 response");

        assert_eq!(turn.assistant_message, assistant_message);
        assert_eq!(turn.tool_calls.len(), 1);
        assert_eq!(turn.tool_calls[0].arguments["path"], "Cargo.toml");
        assert!(client.is_kimi_k3());
        assert_eq!(client.token_cap_field(), "max_completion_tokens");
        assert_eq!(client.max_output_tokens, 32_768);
        assert_eq!(client.context_window(), 1_048_576);
        assert_eq!(client.tool_surface(), ToolSurface::StrReplace);

        let mut body = serde_json::json!({"model": "kimi-k3"});
        client.apply_reasoning_effort(&mut body);
        assert_eq!(body["reasoning_effort"], "max");
    }

    #[test]
    fn role_resolution_can_bind_per_run_reasoning_effort() {
        let _env = ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        unsafe {
            std::env::set_var("MDX_FLEET_REVIEWER_BASE_URL", "https://reviewer.invalid/v1");
            std::env::set_var("MDX_FLEET_REVIEWER_API_KEY", "reviewer-key");
            std::env::set_var("MDX_FLEET_REVIEWER_MODEL", "gpt-reviewer");
            std::env::set_var("MDX_FORGE_REASONING_EFFORT", "low");
        }
        let kernel = MdxKernel::boot_local();
        let client =
            TurnClient::for_role_with_reasoning_effort(&kernel, "local_tenant", "reviewer", "high")
                .expect("reviewer role resolves from env");
        assert_eq!(client.model_id(), "gpt-reviewer");
        assert_eq!(client.reasoning_effort.as_deref(), Some("high"));
        unsafe {
            std::env::remove_var("MDX_FLEET_REVIEWER_BASE_URL");
            std::env::remove_var("MDX_FLEET_REVIEWER_API_KEY");
            std::env::remove_var("MDX_FLEET_REVIEWER_MODEL");
            std::env::remove_var("MDX_FORGE_REASONING_EFFORT");
        }
    }

    #[test]
    fn reasoning_effort_validates_and_is_opt_in() {
        let _env = ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        unsafe {
            std::env::remove_var("MDX_FORGE_REASONING_EFFORT");
        }
        assert_eq!(super::configured_reasoning_effort(), None);
        unsafe {
            std::env::set_var("MDX_FORGE_REASONING_EFFORT", "High");
        }
        assert_eq!(
            super::configured_reasoning_effort().as_deref(),
            Some("high")
        );
        unsafe {
            std::env::set_var("MDX_FORGE_REASONING_EFFORT", "bogus");
        }
        assert_eq!(super::configured_reasoning_effort(), None);
        unsafe {
            std::env::remove_var("MDX_FORGE_REASONING_EFFORT");
        }
    }

    #[test]
    fn tool_surface_flips_only_for_patch_trained_families() {
        assert_eq!(
            tool_surface_for_model("gpt-5-mini"),
            ToolSurface::OpenAiPatch
        );
        assert_eq!(
            tool_surface_for_model("gpt-5.3-codex"),
            ToolSurface::OpenAiPatch
        );
        assert_eq!(
            tool_surface_for_model("codex-mini"),
            ToolSurface::OpenAiPatch
        );
        assert_eq!(
            tool_surface_for_model("claude-opus-4-8"),
            ToolSurface::StrReplace
        );
        assert_eq!(
            tool_surface_for_model("grok-build-0.1"),
            ToolSurface::StrReplace
        );
        assert_eq!(
            tool_surface_for_model("gemini-3-pro"),
            ToolSurface::StrReplace
        );
        assert_eq!(
            tool_surface_for_model("mdx-forge-deterministic-v1"),
            ToolSurface::StrReplace
        );
    }

    #[test]
    fn openai_patch_surface_swaps_edit_tools_for_apply_patch_only() {
        let schema = coding_tool_schema_for(ToolSurface::OpenAiPatch);
        let names: Vec<&str> = schema
            .as_array()
            .expect("array")
            .iter()
            .map(|tool| tool["function"]["name"].as_str().unwrap_or_default())
            .collect();
        assert!(names.contains(&"apply_patch"));
        assert!(!names.contains(&"edit_file"));
        assert!(!names.contains(&"write_file"));
        // Every non-edit tool survives untouched.
        for kept in [
            "read_file",
            "search",
            "semantic_query",
            "list_dir",
            "run_command",
            "shell",
            "finish",
        ] {
            assert!(names.contains(&kept), "missing {kept}");
        }
        // The str-replace surface is byte-identical to the base schema.
        assert_eq!(
            coding_tool_schema_for(ToolSurface::StrReplace),
            coding_tool_schema()
        );
    }

    #[test]
    fn anthropic_prompt_caching_pins_system_prefix_and_last_message_block() {
        let mut body = serde_json::json!({
            "model": "claude-opus-4-8",
            "messages": [
                {"role": "user", "content": "first turn"},
                {"role": "user", "content": [{
                    "type": "tool_result",
                    "tool_use_id": "call_1",
                    "content": "check passed",
                }]},
            ],
        });
        apply_anthropic_prompt_caching(&mut body, Some("Use the harness.".to_string()));
        assert_eq!(
            body["system"][0]["cache_control"]["type"].as_str(),
            Some("ephemeral"),
            "system prefix must carry a cache breakpoint so tools + system cache together"
        );
        assert_eq!(
            body["messages"][1]["content"][0]["cache_control"]["type"].as_str(),
            Some("ephemeral"),
            "last message block must carry the incremental multi-turn breakpoint"
        );
        assert!(
            body["messages"][0]["content"][0]["cache_control"].is_null()
                && body["cache_control"].is_null(),
            "no stray breakpoints: earlier turns and the top level stay unmarked"
        );
    }

    #[test]
    fn anthropic_prompt_caching_wraps_string_content_into_a_block() {
        let mut body = serde_json::json!({
            "model": "claude-opus-4-8",
            "messages": [{"role": "user", "content": "inspect the repo"}],
        });
        apply_anthropic_prompt_caching(&mut body, None);
        let last = &body["messages"][0]["content"][0];
        assert_eq!(last["type"].as_str(), Some("text"));
        assert_eq!(last["text"].as_str(), Some("inspect the repo"));
        assert_eq!(last["cache_control"]["type"].as_str(), Some("ephemeral"));
        assert!(body["system"].is_null());
    }

    #[test]
    fn anthropic_message_conversion_lifts_system_and_tool_results() {
        let messages = vec![
            serde_json::json!({"role": "system", "content": "Use the harness."}),
            serde_json::json!({"role": "user", "content": "inspect the repo"}),
            serde_json::json!({
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "read_file",
                        "arguments": "{\"path\":\"Package.swift\"}"
                    }
                }]
            }),
            tool_result_message("call_1", "manifest"),
        ];

        let (system, converted) = anthropic_system_and_messages(&messages);
        assert_eq!(system.as_deref(), Some("Use the harness."));
        assert_eq!(converted.len(), 3);
        assert_eq!(converted[0]["role"].as_str(), Some("user"));
        assert_eq!(converted[1]["role"].as_str(), Some("assistant"));
        assert_eq!(
            converted[1]["content"][0]["type"].as_str(),
            Some("tool_use")
        );
        assert_eq!(
            converted[1]["content"][0]["name"].as_str(),
            Some("read_file")
        );
        assert_eq!(
            converted[1]["content"][0]["input"]["path"].as_str(),
            Some("Package.swift")
        );
        assert_eq!(converted[2]["role"].as_str(), Some("user"));
        assert_eq!(
            converted[2]["content"][0]["type"].as_str(),
            Some("tool_result")
        );
        assert_eq!(
            converted[2]["content"][0]["tool_use_id"].as_str(),
            Some("call_1")
        );
    }

    #[test]
    fn anthropic_message_conversion_preserves_visual_review_evidence() {
        let messages = vec![serde_json::json!({
            "role": "user",
            "content": [
                {"type": "text", "text": "Review this render."},
                {"type": "image_url", "image_url": {"url": "data:image/jpeg;base64,YWJj", "detail": "low"}}
            ]
        })];

        let (_, converted) = anthropic_system_and_messages(&messages);
        assert_eq!(converted[0]["content"][0]["type"], "text");
        assert_eq!(converted[0]["content"][1]["type"], "image");
        assert_eq!(
            converted[0]["content"][1]["source"]["media_type"],
            "image/jpeg"
        );
        assert_eq!(converted[0]["content"][1]["source"]["data"], "YWJj");
    }

    #[test]
    fn anthropic_tool_schema_marks_forge_tools_strict() {
        let tools = anthropic_tool_schema();
        let tools = tools.as_array().expect("tool array");
        assert!(
            tools.iter().any(|tool| tool["name"] == "read_file"),
            "expected Forge tools in Anthropic schema"
        );
        assert!(
            tools
                .iter()
                .all(|tool| tool["strict"].as_bool() == Some(true)),
            "every Anthropic tool should request strict input validation"
        );
        assert!(
            tools
                .iter()
                .all(|tool| tool["input_schema"]["type"].as_str() == Some("object")),
            "Anthropic tools should carry the OpenAI function parameters as input_schema"
        );
        for tool in tools {
            assert_object_schemas_are_closed_for_anthropic(&tool["input_schema"]);
        }
    }

    fn assert_object_schemas_are_closed_for_anthropic(schema: &serde_json::Value) {
        match schema {
            serde_json::Value::Object(map) => {
                if map.get("type").and_then(|value| value.as_str()) == Some("object") {
                    assert_eq!(
                        map.get("additionalProperties")
                            .and_then(|value| value.as_bool()),
                        Some(false),
                        "Anthropic object schemas must explicitly close additional properties: {schema}"
                    );
                }
                for child in map.values() {
                    assert_object_schemas_are_closed_for_anthropic(child);
                }
            }
            serde_json::Value::Array(items) => {
                for child in items {
                    assert_object_schemas_are_closed_for_anthropic(child);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn anthropic_response_parser_normalizes_tool_use_calls() {
        let client = TurnClient {
            provider_family: "anthropic".to_string(),
            base_url: "https://api.anthropic.com/v1".to_string(),
            api_key: "test".to_string(),
            model: "claude-opus-test".to_string(),
            max_output_tokens: 4096,
            reasoning_effort: None,
            route: None,
            fallbacks: Vec::new(),
        };
        let parsed = serde_json::json!({
            "id": "msg_123",
            "model": "claude-opus-test",
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 10, "output_tokens": 5},
            "content": [
                {"type": "text", "text": "I'll inspect it."},
                {
                    "type": "tool_use",
                    "id": "toolu_123",
                    "name": "search",
                    "input": {"pattern": "TODO"}
                }
            ]
        });

        let turn = client
            .anthropic_turn_from_response(parsed)
            .expect("parsed response");
        assert_eq!(turn.text, "I'll inspect it.");
        assert_eq!(turn.finish_reason, "tool_use");
        assert_eq!(turn.inference_id, "msg_123");
        assert_eq!(turn.input_tokens, 10);
        assert_eq!(turn.output_tokens, 5);
        assert_eq!(turn.tool_calls.len(), 1);
        assert_eq!(turn.tool_calls[0].call_id, "toolu_123");
        assert_eq!(turn.tool_calls[0].name, "search");
        assert_eq!(
            turn.tool_calls[0].arguments["pattern"].as_str(),
            Some("TODO")
        );
        assert_eq!(
            turn.assistant_message["tool_calls"][0]["function"]["arguments"].as_str(),
            Some("{\"pattern\":\"TODO\"}")
        );
    }

    #[test]
    fn the_strongest_configured_slot_follows_the_capability_preference() {
        let _env = ENV_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let keys = [
            "ANTHROPIC_API_KEY",
            "OPENAI_API_KEY",
            "XAI_API_KEY",
            "GEMINI_API_KEY",
            "AWS_BEDROCK_MODEL_ACCESS",
            "MDX_OLLAMA_API_KEY",
            "MDX_DEFAULT_MODEL_PROVIDER",
            "MDX_ANTHROPIC_MODEL",
            "MDX_ANTHROPIC_OPUS_MODEL",
            "MDX_ANTHROPIC_SONNET_MODEL",
            "MDX_OPENAI_MODEL",
            "MDX_OPENAI_CODEX_MODEL",
            "MDX_OPENAI_CODEX_MINI_MODEL",
            "MDX_XAI_BUILD_MODEL",
            "MDX_XAI_MODEL",
            "MDX_GEMINI_MODEL",
            "MDX_FLEET_BUILDER_OPUS_BASE_URL",
            "MDX_FLEET_BUILDER_OPUS_API_KEY",
            "MDX_FLEET_BUILDER_OPUS_MODEL",
            "MDX_FLEET_BUILDER_SONNET_BASE_URL",
            "MDX_FLEET_BUILDER_SONNET_API_KEY",
            "MDX_FLEET_BUILDER_SONNET_MODEL",
            "MDX_FLEET_BUILDER_CODEXMINI_BASE_URL",
            "MDX_FLEET_BUILDER_CODEXMINI_API_KEY",
            "MDX_FLEET_BUILDER_CODEXMINI_MODEL",
        ];
        unsafe {
            for k in keys {
                std::env::remove_var(k);
            }
        }
        // Nothing configured -> the default builder.
        assert_eq!(TurnClient::strongest_configured_builder_slot(), "");
        // Only a mid slot configured -> it wins.
        unsafe {
            std::env::set_var(
                "MDX_FLEET_BUILDER_SONNET_BASE_URL",
                "https://anthropic-compatible.test/v1",
            );
            std::env::set_var("MDX_FLEET_BUILDER_SONNET_API_KEY", "test");
            std::env::set_var("MDX_FLEET_BUILDER_SONNET_MODEL", "claude-sonnet-4-6");
        }
        assert_eq!(TurnClient::strongest_configured_builder_slot(), "SONNET");
        // The top slot configured too -> preference picks it over the mid.
        unsafe {
            std::env::set_var(
                "MDX_FLEET_BUILDER_OPUS_BASE_URL",
                "https://anthropic-compatible.test/v1",
            );
            std::env::set_var("MDX_FLEET_BUILDER_OPUS_API_KEY", "test");
            std::env::set_var("MDX_FLEET_BUILDER_OPUS_MODEL", "claude-opus-4-8");
        }
        assert_eq!(TurnClient::strongest_configured_builder_slot(), "OPUS");
        unsafe {
            for k in keys {
                std::env::remove_var(k);
            }
        }
    }

    #[test]
    fn configured_builder_slots_reports_full_named_slots_only() {
        let _env = ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        crate::secret_store::global().clear_tenant("local_tenant");
        let keys = [
            "ANTHROPIC_API_KEY",
            "OPENAI_API_KEY",
            "XAI_API_KEY",
            "GEMINI_API_KEY",
            "AWS_BEDROCK_MODEL_ACCESS",
            "MDX_OLLAMA_API_KEY",
            "MDX_DEFAULT_MODEL_PROVIDER",
            "MDX_ANTHROPIC_MODEL",
            "MDX_ANTHROPIC_OPUS_MODEL",
            "MDX_ANTHROPIC_SONNET_MODEL",
            "MDX_OPENAI_MODEL",
            "MDX_OPENAI_CODEX_MODEL",
            "MDX_OPENAI_CODEX_MINI_MODEL",
            "MDX_XAI_BUILD_MODEL",
            "MDX_XAI_MODEL",
            "MDX_GEMINI_MODEL",
            "MDX_FLEET_BUILDER_OPUS_BASE_URL",
            "MDX_FLEET_BUILDER_OPUS_API_KEY",
            "MDX_FLEET_BUILDER_OPUS_MODEL",
            "MDX_FLEET_BUILDER_GEMINI_BASE_URL",
            "MDX_FLEET_BUILDER_GEMINI_API_KEY",
            "MDX_FLEET_BUILDER_GEMINI_MODEL",
            "MDX_FLEET_BUILDER_XAI_BASE_URL",
            "MDX_FLEET_BUILDER_XAI_API_KEY",
            "MDX_FLEET_BUILDER_XAI_MODEL",
        ];
        for key in keys {
            crate::secret_store::global().disable_keychain_lookup_for_tenant("local_tenant", key);
        }
        unsafe {
            for key in keys {
                std::env::remove_var(key);
            }
            std::env::set_var(
                "MDX_FLEET_BUILDER_OPUS_BASE_URL",
                "https://anthropic.test/v1",
            );
            std::env::set_var("MDX_FLEET_BUILDER_OPUS_API_KEY", "test");
            std::env::set_var("MDX_FLEET_BUILDER_OPUS_MODEL", "claude-opus-4-8");
            std::env::set_var(
                "MDX_FLEET_BUILDER_GEMINI_BASE_URL",
                "https://gemini.test/v1",
            );
            std::env::set_var("MDX_FLEET_BUILDER_GEMINI_MODEL", "gemini-2.5-pro");
            std::env::set_var("MDX_FLEET_BUILDER_XAI_BASE_URL", "https://api.x.ai/v1");
            std::env::set_var("MDX_FLEET_BUILDER_XAI_API_KEY", "test");
            std::env::set_var("MDX_FLEET_BUILDER_XAI_MODEL", "grok-4.3");
        }

        assert_eq!(
            TurnClient::configured_builder_slots(),
            vec!["OPUS".to_string(), "XAI".to_string()]
        );

        unsafe {
            for key in keys {
                std::env::remove_var(key);
            }
        }
        crate::secret_store::global().clear_tenant("local_tenant");
    }

    #[test]
    fn a_slot_is_frontier_unless_its_privacy_class_env_says_sovereign() {
        let _env = ENV_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let key = "MDX_FLEET_BUILDER_ONPREM_PRIVACY_CLASS";
        unsafe {
            std::env::remove_var(key);
        }
        // Unset -> frontier (the safe assumption: an external API).
        assert_eq!(TurnClient::slot_privacy_class("ONPREM"), "frontier");
        // Explicitly sovereign -> sovereign.
        unsafe {
            std::env::set_var(key, "sovereign");
        }
        assert_eq!(TurnClient::slot_privacy_class("ONPREM"), "sovereign");
        // Any other value is not a sovereign claim.
        unsafe {
            std::env::set_var(key, "external");
        }
        assert_eq!(TurnClient::slot_privacy_class("ONPREM"), "frontier");
        unsafe {
            std::env::remove_var(key);
        }
    }

    #[test]
    fn a_sovereign_label_without_full_config_is_not_ready() {
        let _env = ENV_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let class = "MDX_FLEET_BUILDER_SOV_PRIVACY_CLASS";
        let base = "MDX_FLEET_BUILDER_SOV_BASE_URL";
        let key = "MDX_FLEET_BUILDER_SOV_API_KEY";
        let model = "MDX_FLEET_BUILDER_SOV_MODEL";
        for var in [class, base, key, model] {
            unsafe {
                std::env::remove_var(var);
            }
        }
        // Labeled sovereign but no endpoint: builder_from_env would fall back to
        // the default frontier provider, so the slot is NOT ready and the
        // residency gate must refuse it - the label alone is not the promise.
        unsafe {
            std::env::set_var(class, "sovereign");
        }
        assert_eq!(TurnClient::slot_privacy_class("SOV"), "sovereign");
        assert!(
            !TurnClient::sovereign_slot_ready("SOV"),
            "a sovereign label with no config can fall back to frontier"
        );
        // Once the slot carries its own complete config, no fallback is
        // possible and the promise holds.
        unsafe {
            std::env::set_var(base, "https://sovereign.local/v1");
            std::env::set_var(key, "k");
            std::env::set_var(model, "onprem-coder");
        }
        assert!(TurnClient::sovereign_slot_ready("SOV"));
        for var in [class, base, key, model] {
            unsafe {
                std::env::remove_var(var);
            }
        }
    }

    #[test]
    fn a_welcome_connected_xai_model_can_power_the_default_forge_builder() {
        let _env = ENV_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        unsafe {
            for key in [
                "MDX_DEFAULT_MODEL_PROVIDER",
                "XAI_API_KEY",
                "MDX_XAI_BUILD_MODEL",
                "MDX_XAI_MODEL",
                "MDX_FLEET_BUILDER_BASE_URL",
                "MDX_FLEET_BUILDER_API_KEY",
                "MDX_FLEET_BUILDER_MODEL",
            ] {
                std::env::remove_var(key);
            }
        }
        let tenant_id = "forge_connected_tenant";
        let mut kernel = MdxKernel::boot_local();
        let approval = kernel
            .approve_model_turn_on(ApproveModelTurnOn {
                tenant_id,
                actor_id: "human:test",
                provider_id: "xai",
            })
            .expect("approval");
        kernel
            .save_twin_model_gateway_provider_observation_local(
                TwinModelGatewayProviderObservation {
                    tenant_id,
                    actor_id: "human:test",
                    provider_id: "xai",
                    adapter: "XaiChatModelGateway",
                    receipt_kind: "xai.chat.observed",
                    approval_receipt_id: &approval.approval_receipt_id,
                    evidence_file: "test:welcome-connect",
                    model_id: "grok-connected-test",
                    response_id: "resp_connected_test",
                    response_status: "completed",
                    observed: true,
                    provider_call_attempted: true,
                    network_call_attempted: true,
                    credential_presence_only: true,
                    credential_values_recorded: false,
                    provider_secret_values_recorded: false,
                    requested_secret_values_recorded: false,
                    output_text_recorded: false,
                    production_write_allowed: false,
                    total_tokens: 3,
                },
            )
            .expect("observation");
        crate::secret_store::global().set_for_tenant(tenant_id, "XAI_API_KEY", "xai-test-key");

        let client = TurnClient::builder_for_tenant(&kernel, tenant_id, "")
            .expect("connected model should power Forge");
        assert_eq!(client.model_id(), "grok-connected-test");
        assert!(TurnClient::default_builder_configured_for_tenant(
            &kernel, tenant_id
        ));
    }

    #[test]
    fn a_session_provider_key_can_power_the_auto_default_forge_builder() {
        let _env = ENV_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tenant_id = "local_tenant";
        crate::secret_store::global().clear_tenant(tenant_id);
        unsafe {
            for key in [
                "MDX_DEFAULT_MODEL_PROVIDER",
                "XAI_API_KEY",
                "MDX_XAI_BUILD_MODEL",
                "MDX_XAI_MODEL",
                "MDX_FLEET_BUILDER_BASE_URL",
                "MDX_FLEET_BUILDER_API_KEY",
                "MDX_FLEET_BUILDER_MODEL",
            ] {
                std::env::remove_var(key);
            }
        }
        let kernel = MdxKernel::boot_local();
        crate::secret_store::global().set_for_tenant(tenant_id, "XAI_API_KEY", "xai-test-key");

        assert!(TurnClient::default_builder_configured_for_tenant(
            &kernel, tenant_id
        ));
        let client = TurnClient::builder_for_tenant(&kernel, tenant_id, "")
            .expect("session provider router should power auto builder");
        assert_eq!(client.model_id(), "grok-4.5");
        crate::secret_store::global().clear_tenant(tenant_id);
    }

    #[test]
    fn streaming_tool_deltas_rebuild_the_final_tool_turn() {
        let mut tool_calls = Vec::new();
        merge_openai_tool_deltas(
            &mut tool_calls,
            &[serde_json::json!({
                "index": 0,
                "id": "call_1",
                "function": {
                    "name": "read_file",
                    "arguments": "{\"path\""
                }
            })],
        );
        merge_openai_tool_deltas(
            &mut tool_calls,
            &[serde_json::json!({
                "index": 0,
                "function": {
                    "arguments": ":\"src/lib.rs\"}"
                }
            })],
        );

        let turn = model_turn_from_parts(
            "I will inspect the file.".to_string(),
            tool_calls,
            "tool_calls".to_string(),
            "grok-test".to_string(),
            "chatcmpl-test".to_string(),
            10,
            3,
        );

        assert_eq!(turn.text, "I will inspect the file.");
        assert_eq!(turn.finish_reason, "tool_calls");
        assert_eq!(turn.model_id, "grok-test");
        assert_eq!(turn.tool_calls.len(), 1);
        assert_eq!(turn.tool_calls[0].name, "read_file");
        assert_eq!(
            turn.tool_calls[0].arguments,
            serde_json::json!({"path": "src/lib.rs"})
        );
        assert_eq!(
            turn.assistant_message["tool_calls"][0]["function"]["name"].as_str(),
            Some("read_file")
        );
    }

    #[test]
    fn stream_fallback_marker_ignores_metadata_only_openai_events() {
        let metadata_only = serde_json::json!({
            "index": 0,
            "id": "call_1",
            "function": {}
        });
        assert!(!openai_tool_delta_has_meaningful_output(&metadata_only));

        let with_name = serde_json::json!({
            "index": 0,
            "id": "call_1",
            "function": { "name": "read_file" }
        });
        assert!(openai_tool_delta_has_meaningful_output(&with_name));

        let with_arguments = serde_json::json!({
            "index": 0,
            "function": { "arguments": "{\"path\":\"src/lib.rs\"}" }
        });
        assert!(openai_tool_delta_has_meaningful_output(&with_arguments));
    }

    #[test]
    fn provider_call_switch_refuses_before_network() {
        let _env = ENV_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        unsafe {
            std::env::set_var("MDX_PROVIDER_CALLS_ENABLED", "0");
            std::env::remove_var("MDX_FORGE_PROVIDER_CALLS_ENABLED");
        }
        let client = TurnClient {
            provider_family: "xai".to_string(),
            base_url: "https://provider.invalid/v1".to_string(),
            api_key: "xai-test-key".to_string(),
            model: "grok-test".to_string(),
            max_output_tokens: 16,
            reasoning_effort: None,
            route: None,
            fallbacks: Vec::new(),
        };

        let error = client
            .complete(&[serde_json::json!({"role":"user","content":"hi"})])
            .expect_err("provider call switch should refuse");
        assert!(error.contains("provider calls are paused"));
        unsafe {
            std::env::remove_var("MDX_PROVIDER_CALLS_ENABLED");
        }
    }

    #[test]
    fn deterministic_provider_is_env_gated_and_emits_simulation_tool_turns() {
        let _env = ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        unsafe {
            std::env::remove_var("MDX_FORGE_DETERMINISTIC_PROVIDER");
            std::env::set_var("MDX_PROVIDER_CALLS_ENABLED", "0");
            std::env::set_var("MDX_FORGE_PROVIDER_CALLS_ENABLED", "0");
        }
        let kernel = MdxKernel::boot_local();
        if let Some(client) = TurnClient::builder_for_tenant(&kernel, "local_tenant", "") {
            assert_ne!(client.provider_family(), "local_deterministic");
        }

        unsafe {
            std::env::set_var("MDX_FORGE_DETERMINISTIC_PROVIDER", "1");
        }
        let client = TurnClient::builder_for_tenant(&kernel, "local_tenant", "")
            .expect("deterministic builder");
        assert_eq!(client.provider_family(), "local_deterministic");
        assert_eq!(client.model_id(), "mdx-forge-deterministic-v1");

        let turn = client
            .turn(&[serde_json::json!({
                "role": "user",
                "content": "Task: In small-app, make a one-file copy-safe fix, prove it with the existing test, and prepare a reviewable handoff.\n\nAllowed check commands:\nnpm test"
            })])
            .expect("deterministic turn");
        assert_eq!(turn.finish_reason, "tool_calls");
        assert_eq!(
            turn.tool_calls
                .iter()
                .map(|call| call.name.as_str())
                .collect::<Vec<_>>(),
            vec![
                "semantic_query",
                "read_file",
                "edit_file",
                "run_command",
                "semantic_query",
                "finish"
            ]
        );
        assert_eq!(
            turn.tool_calls[2].arguments["path"].as_str(),
            Some("src/index.js")
        );
        assert_eq!(
            turn.tool_calls[3].arguments["command"].as_str(),
            Some("npm test")
        );

        unsafe {
            std::env::remove_var("MDX_FORGE_DETERMINISTIC_PROVIDER");
            std::env::remove_var("MDX_PROVIDER_CALLS_ENABLED");
            std::env::remove_var("MDX_FORGE_PROVIDER_CALLS_ENABLED");
        }
    }

    #[test]
    fn forge_provider_call_switch_refuses_before_network() {
        let _env = ENV_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        unsafe {
            std::env::remove_var("MDX_PROVIDER_CALLS_ENABLED");
            std::env::set_var("MDX_FORGE_PROVIDER_CALLS_ENABLED", "false");
        }
        let client = TurnClient {
            provider_family: "xai".to_string(),
            base_url: "https://provider.invalid/v1".to_string(),
            api_key: "xai-test-key".to_string(),
            model: "grok-test".to_string(),
            max_output_tokens: 16,
            reasoning_effort: None,
            route: None,
            fallbacks: Vec::new(),
        };

        let error = client
            .turn(&[serde_json::json!({"role":"user","content":"hi"})])
            .expect_err("Forge provider call switch should refuse");
        assert!(error.contains("provider calls are paused"));
        unsafe {
            std::env::remove_var("MDX_FORGE_PROVIDER_CALLS_ENABLED");
        }
    }

    #[test]
    fn provider_request_timeout_stays_inside_forge_watchdog() {
        let _env = ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        unsafe {
            std::env::set_var("MDX_FORGE_MODEL_TURN_TIMEOUT_SECS", "45");
        }
        assert_eq!(provider_request_timeout(), Duration::from_secs(40));
        unsafe {
            std::env::set_var("MDX_FORGE_MODEL_TURN_TIMEOUT_SECS", "30");
        }
        assert_eq!(provider_request_timeout(), Duration::from_secs(25));
        unsafe {
            std::env::remove_var("MDX_FORGE_MODEL_TURN_TIMEOUT_SECS");
        }
        assert_eq!(
            provider_request_timeout(),
            Duration::from_secs(DEFAULT_PROVIDER_REQUEST_TIMEOUT_SECS)
        );
    }

    #[test]
    fn provider_request_timeout_obeys_model_turn_deadline() {
        let _env = ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        unsafe {
            std::env::set_var("MDX_FORGE_MODEL_TURN_TIMEOUT_SECS", "300");
        }
        let timeout = with_model_turn_deadline(Instant::now() + Duration::from_secs(2), || {
            provider_request_timeout()
        });
        assert!(timeout <= Duration::from_secs(2));
        assert!(timeout >= Duration::from_millis(1));
        unsafe {
            std::env::remove_var("MDX_FORGE_MODEL_TURN_TIMEOUT_SECS");
        }
    }

    #[test]
    fn retry_after_seconds_are_parsed_and_bounded() {
        assert_eq!(
            TurnClient::parse_retry_after_seconds("3"),
            Some(Duration::from_secs(3))
        );
        assert_eq!(
            TurnClient::parse_retry_after_seconds("600"),
            Some(Duration::from_secs(MAX_RETRY_AFTER_SECS))
        );
        assert_eq!(TurnClient::parse_retry_after_seconds("soon"), None);
    }
}
