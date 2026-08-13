//! The read-only investigator subagent - a context firewall.
//!
//! The pattern the leading harnesses converged on (Claude Code's Agent tool,
//! Amp's Librarian, HumanLayer's ACE-FCA): when answering a question would
//! take many reads and searches, the parent delegates it to a fresh subagent
//! with its OWN clean context. The subagent explores, then returns ONE
//! distilled finding. The parent's context never sees the twenty file reads -
//! only the answer. That is the whole value: exploration that would bloat the
//! parent's window costs it a single message instead.
//!
//! Two guardrails make this safe, both from the multi-agent failure research
//! (Cognition's "Don't Build Multi-Agents"):
//!   1. READ-ONLY. The investigator gets read_file/search/semantic_query/
//!      list_dir and nothing else - no edit, write, run_command, or finish.
//!      The parent keeps SOLE write authority (the single-writer principle),
//!      so a subagent can never take a conflicting action.
//!   2. NO NESTING. The investigate tool is absent from the subagent's own
//!      schema, so an investigator cannot spawn investigators. Depth is 1.
//!
//! The only channel in is the question string; the only channel out is the
//! final text. Everything the subagent needs (paths, errors, the exact
//! question) must be in the question - there is no back-channel.

use crate::forge_turn_client::TurnClient;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

#[derive(Clone, Debug, Default)]
pub(crate) struct SubagentModelUsage {
    pub model_id: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SubagentReport {
    pub text: String,
    pub model_usage: Vec<SubagentModelUsage>,
}

impl SubagentReport {
    pub(crate) fn input_tokens(&self) -> u64 {
        self.model_usage
            .iter()
            .map(|usage| usage.input_tokens)
            .sum()
    }

    pub(crate) fn output_tokens(&self) -> u64 {
        self.model_usage
            .iter()
            .map(|usage| usage.output_tokens)
            .sum()
    }

    pub(crate) fn model_call_count(&self) -> usize {
        self.model_usage.len()
    }
}

/// Max agentic turns one investigation may take before it must report. Small:
/// an investigator answers a focused question, it does not do open-ended work.
fn investigation_max_turns() -> u32 {
    std::env::var("MDX_FORGE_INVESTIGATE_MAX_TURNS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|v| *v >= 2)
        .unwrap_or(10)
}

/// Max investigations one run may spawn, so a run cannot burn its whole budget
/// on delegation. The parent tracks the count; this is the ceiling.
pub(crate) fn investigation_max_calls() -> u32 {
    std::env::var("MDX_FORGE_INVESTIGATE_MAX_CALLS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|v| *v >= 1)
        .unwrap_or(6)
}

/// Wall-clock cap for one delegated read-only subagent. The provider call
/// timeout protects each model request; this protects the parent Forge run from
/// looking frozen while a subagent spends many turns.
fn investigation_wall_timeout() -> Duration {
    Duration::from_secs(
        std::env::var("MDX_FORGE_INVESTIGATE_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|v| *v >= 5)
            .unwrap_or(90),
    )
}

const INVESTIGATOR_SYSTEM: &str = "\
You are a read-only investigator with fresh eyes and your own clean context. Another agent \
delegated one focused question about this workspace. Explore with read_file, search, \
semantic_query, and list_dir to answer it precisely - then STOP and reply with your finding \
as plain text (no tool call). You cannot edit, write, or run commands; you only investigate \
and report. Keep the finding concise and concrete: name the files and line numbers, the actual \
mechanism, or a direct yes/no with the reason. The delegating agent will act on your answer, \
so give it the essentials, not a narration of your search.";

/// The subagent's tool surface: the read-only exploration tools only, taken
/// verbatim from the coding schema (so semantic_query keeps its full operation
/// set) minus everything that writes, runs, finishes, or delegates. The
/// absence of `investigate` here is what forbids nesting.
fn readonly_investigation_schema() -> serde_json::Value {
    const READ_ONLY: [&str; 4] = ["read_file", "search", "semantic_query", "list_dir"];
    let tools = crate::forge_turn_client::coding_tool_schema()
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|tool| {
            tool["function"]["name"]
                .as_str()
                .map(|name| READ_ONLY.contains(&name))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    serde_json::Value::Array(tools)
}

/// The maximum length of a returned finding. The firewall is a compressor: a
/// finding longer than this is truncated so a runaway subagent cannot re-bloat
/// the parent's context it was meant to protect.
const MAX_FINDING_BYTES: usize = 4_000;

const REVIEWER_SYSTEM: &str = "\
You are an independent reviewer with fresh eyes - the Oracle. You did NOT write the change you \
are reviewing and you have none of the author's context or assumptions. You are given the task \
and the proposed change (a diff). Read the workspace as needed to judge the change IN CONTEXT: \
does it actually accomplish the task, handle the edge cases, and not break adjacent behavior? \
Look for the mistakes an author is blind to. You cannot edit - you only review and report. \
Reply with your verdict as plain text: start with `APPROVE` if the change is correct and \
complete, or `CONCERNS:` followed by the specific, concrete problems (with file:line where you \
can). Be direct and brief; the author will act on your verdict.";

/// Run one read-only investigation to answer `question`. Returns the
/// subagent's distilled finding and its model usage, so the parent run can
/// account for delegated spend under the same budget.
pub(crate) fn run_investigation(
    client: &TurnClient,
    question: &str,
    workspace_path: &Path,
) -> SubagentReport {
    let opening = format!(
        "Question from the delegating agent:\n{question}\n\nThe workspace is at the repository root. Investigate, then reply with your finding as plain text."
    );
    run_readonly_subagent_bounded(
        client,
        INVESTIGATOR_SYSTEM,
        &opening,
        workspace_path,
        investigation_max_turns(),
        "investigation",
    )
}

/// Run several read-only investigations concurrently (Codex's bounded parallel
/// fan-out). Each runs in its own thread with a cloned client and its own clean
/// context - they share no mutable state and only read the workspace, so this
/// is safe. Concurrency is bounded by the caller (the per-run delegation
/// budget, ~6, matching Codex's max_threads). Returns (call_id, report) pairs;
/// a panicked investigation yields an honest note, never a lost result.
pub(crate) fn run_investigations_parallel(
    client: &TurnClient,
    questions: &[(String, String)],
    workspace_path: &Path,
) -> Vec<(String, SubagentReport)> {
    std::thread::scope(|scope| {
        let handles: Vec<(String, _)> = questions
            .iter()
            .map(|(call_id, question)| {
                let client = client.clone();
                let question = question.clone();
                let workspace = workspace_path.to_path_buf();
                let handle = scope.spawn(move || run_investigation(&client, &question, &workspace));
                (call_id.clone(), handle)
            })
            .collect();
        handles
            .into_iter()
            .map(|(call_id, handle)| {
                let finding = handle.join().unwrap_or_else(|_| SubagentReport {
                    text: "investigation failed (thread panicked)".to_string(),
                    model_usage: Vec::new(),
                });
                (call_id, finding)
            })
            .collect()
    })
}

/// Run one independent review of a proposed change (the Oracle pattern). The
/// reviewer client is resolved to the reviewer role by the caller, so a
/// different, stronger model can review than the one that wrote the change -
/// genuine independence, not the author grading itself. Read-only: it can read
/// the workspace to judge the diff in context, but never edits.
pub(crate) fn run_verification(
    reviewer: &TurnClient,
    task: &str,
    diff: &str,
    focus: &str,
    workspace_path: &Path,
) -> SubagentReport {
    let focus_line = if focus.trim().is_empty() {
        String::new()
    } else {
        format!("\n\nThe author especially wants you to check: {focus}")
    };
    let opening = format!(
        "Task:\n{task}\n\nProposed change (diff):\n{diff}{focus_line}\n\nReview it against the task. Read the workspace as needed, then reply with APPROVE or CONCERNS: <specifics>."
    );
    run_readonly_subagent_bounded(
        reviewer,
        REVIEWER_SYSTEM,
        &opening,
        workspace_path,
        investigation_max_turns(),
        "verification",
    )
}

fn run_readonly_subagent_bounded(
    client: &TurnClient,
    system: &str,
    opening_user_content: &str,
    workspace_path: &Path,
    max_turns: u32,
    label: &'static str,
) -> SubagentReport {
    let timeout = investigation_wall_timeout();
    let (tx, rx) = mpsc::channel();
    let client = client.clone();
    let system = system.to_string();
    let opening_user_content = opening_user_content.to_string();
    let workspace_path = workspace_path.to_path_buf();
    std::thread::spawn(move || {
        let report = run_readonly_subagent(
            &client,
            &system,
            &opening_user_content,
            &workspace_path,
            max_turns,
        );
        let _ = tx.send(report);
    });
    rx.recv_timeout(timeout).unwrap_or_else(|_| SubagentReport {
        text: format!(
            "{label} timed out after {}s; continue with the context already gathered, make the smallest safe next move, and only delegate again if the missing fact is essential.",
            timeout.as_secs()
        ),
        model_usage: Vec::new(),
    })
}

/// The shared bounded read-only sub-loop behind both investigate and verify:
/// a fresh conversation (system + one opening user message), the read-only
/// tool surface, run until the subagent replies with plain text (its report)
/// or the turn ceiling. Returns the final text as the firewall - the parent
/// never sees the intermediate reads. Model usage is retained for the parent
/// run's cost and token accounting.
fn run_readonly_subagent(
    client: &TurnClient,
    system: &str,
    opening_user_content: &str,
    workspace_path: &Path,
    max_turns: u32,
) -> SubagentReport {
    let schema = readonly_investigation_schema();
    let mut messages = vec![
        serde_json::json!({ "role": "system", "content": system }),
        serde_json::json!({ "role": "user", "content": opening_user_content }),
    ];
    let mut last_text = String::new();
    let mut report = SubagentReport::default();
    for _ in 0..max_turns {
        let turn = match client.turn_with_tools(&messages, Some(&schema)) {
            Ok(turn) => turn,
            // A provider error is not a finding. Report what we have honestly.
            Err(error) => {
                report.text = finalize(
                    &last_text,
                    &format!("investigation could not complete (model error: {error})"),
                );
                return report;
            }
        };
        report.model_usage.push(SubagentModelUsage {
            model_id: turn.model_id.clone(),
            input_tokens: turn.input_tokens,
            output_tokens: turn.output_tokens,
        });
        if !turn.text.trim().is_empty() {
            last_text = turn.text.clone();
        }
        // No tool calls means the subagent is reporting its finding: done.
        if turn.tool_calls.is_empty() {
            report.text = finalize(&turn.text, "investigation returned no finding text");
            return report;
        }
        messages.push(turn.assistant_message.clone());
        for call in &turn.tool_calls {
            let result = dispatch_readonly(call, workspace_path);
            messages.push(crate::forge_turn_client::tool_result_message(
                &call.call_id,
                &result,
            ));
        }
    }
    // Hit the turn ceiling without a plain-text conclusion.
    report.text = finalize(
        &last_text,
        "investigation hit its turn limit before concluding",
    );
    report
}

/// Trim a finding to the firewall's byte cap, falling back to a note when the
/// subagent produced no usable text (the require-text-final-message rule: a
/// tool-calls-only or empty result is a soft failure, reported honestly).
fn finalize(text: &str, empty_note: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return empty_note.to_string();
    }
    if trimmed.len() <= MAX_FINDING_BYTES {
        return trimmed.to_string();
    }
    let mut end = MAX_FINDING_BYTES;
    while !trimmed.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n... (finding truncated)", &trimmed[..end])
}

/// Dispatch one read-only tool call inside an investigation. Only the four
/// exploration tools resolve; anything else is refused (defense in depth - the
/// schema already excludes them, but a model could still hallucinate a name).
fn dispatch_readonly(
    call: &crate::forge_turn_client::ParsedToolCall,
    workspace_path: &Path,
) -> String {
    let arg = |key: &str| call.arguments[key].as_str().unwrap_or("").to_string();
    match call.name.as_str() {
        "read_file" => crate::forge_loop_runner::read_confined(workspace_path, &arg("path"))
            .unwrap_or_else(|reason| format!("error: {reason}")),
        "list_dir" => crate::forge_loop_runner::list_confined(workspace_path, &arg("path"))
            .unwrap_or_else(|reason| format!("error: {reason}")),
        "search" => crate::forge_loop_runner::search_confined(
            workspace_path,
            &arg("pattern"),
            &arg("path_prefix"),
        ),
        "semantic_query" => crate::forge_semantic_query_route::semantic_query_tool_text(
            workspace_path,
            &arg("operation"),
            &arg("query"),
            &arg("path"),
            call.arguments["line"].as_u64().unwrap_or(0) as usize,
        ),
        other => format!(
            "error: {other} is not available to an investigator - you may only read, search, list, or query. Report your finding as text."
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readonly_schema_is_exploration_only_and_forbids_nesting() {
        let names: Vec<String> = readonly_investigation_schema()
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["function"]["name"].as_str().unwrap().to_string())
            .collect();
        assert!(names.contains(&"read_file".to_string()));
        assert!(names.contains(&"search".to_string()));
        assert!(names.contains(&"semantic_query".to_string()));
        assert!(names.contains(&"list_dir".to_string()));
        for forbidden in [
            "edit_file",
            "write_file",
            "apply_patch",
            "run_command",
            "shell",
            "finish",
            "investigate",
        ] {
            assert!(
                !names.contains(&forbidden.to_string()),
                "{forbidden} must not be available to a subagent"
            );
        }
    }

    #[test]
    fn finalize_reports_a_note_when_empty_and_bounds_length() {
        assert_eq!(finalize("   ", "nothing"), "nothing");
        assert_eq!(finalize("a real finding", "nothing"), "a real finding");
        let long = "x".repeat(MAX_FINDING_BYTES + 500);
        let out = finalize(&long, "nothing");
        assert!(out.len() < MAX_FINDING_BYTES + 40);
        assert!(out.ends_with("(finding truncated)"));
    }

    #[test]
    fn subagent_report_sums_model_usage() {
        let report = SubagentReport {
            text: "done".to_string(),
            model_usage: vec![
                SubagentModelUsage {
                    model_id: "model-a".to_string(),
                    input_tokens: 10,
                    output_tokens: 5,
                },
                SubagentModelUsage {
                    model_id: "model-a".to_string(),
                    input_tokens: 20,
                    output_tokens: 7,
                },
            ],
        };
        assert_eq!(report.input_tokens(), 30);
        assert_eq!(report.output_tokens(), 12);
        assert_eq!(report.model_call_count(), 2);
    }

    #[test]
    fn dispatch_refuses_non_readonly_tools() {
        let call = crate::forge_turn_client::ParsedToolCall {
            call_id: "c1".to_string(),
            name: "edit_file".to_string(),
            arguments: serde_json::json!({"path": "x"}),
        };
        let out = dispatch_readonly(&call, Path::new("."));
        assert!(out.contains("not available to an investigator"));
    }

    #[test]
    fn call_ceiling_has_a_floor() {
        unsafe {
            std::env::set_var("MDX_FORGE_INVESTIGATE_MAX_CALLS", "0");
        }
        assert!(investigation_max_calls() >= 1);
        unsafe {
            std::env::remove_var("MDX_FORGE_INVESTIGATE_MAX_CALLS");
        }
    }

    #[test]
    fn investigation_wall_timeout_has_a_sane_default() {
        assert_eq!(investigation_wall_timeout(), Duration::from_secs(90));
    }
}
