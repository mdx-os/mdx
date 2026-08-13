// The fresh-eyes finish evaluator. When a run auto-closes because its checks
// went green and stable, the builder model is judging its own work - and a
// weaker model will happily declare done on a diff that passes a shallow
// check but misses the task. This seam puts ONE skeptical reviewer, with zero
// shared context, between "checks are green" and RUN_FINISHED_DONE: it sees
// only the task and the diff, reasons backward from the change to the intent,
// and either approves the finish or names one concrete gap.
//
// It is exactly one seam, not a pyramid of judges (the audit's fleet evidence
// says execution beats judging). It ADVISES: it can send the run back to work
// a bounded number of times, then the builder and its checks decide - so a
// stubborn evaluator can never trap a run in a bot-to-bot loop. Availability
// is interpreted by the caller's review policy: conditional review fails open,
// while required review fails closed. The verdict is receipted either way so
// it can feed the scorecard.
use crate::forge_turn_client::TurnClient;
use mdx_core::MdxKernel;
use std::path::Path;
use std::sync::{Arc, RwLock};

/// A fresh-eyes verdict on a proposed finish.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FinishVerdict {
    pub approved: bool,
    pub concern: String,
    pub model_id: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// How many times the evaluator may send a run back to work before the finish
/// proceeds regardless. The floor is 1 - the seam must always relent - and the
/// default is deliberately small: the evaluator is a check, not a gate that
/// can hold a run hostage.
pub(crate) fn max_finish_blocks() -> u32 {
    std::env::var("MDX_FORGE_FINISH_EVALUATOR_MAX_BLOCKS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value >= 1)
        .unwrap_or(2)
}

/// Parse a reviewer's reply into a verdict. The contract asked for `APPROVE`
/// or `REVISE: <concern>`. A reply that clearly revises blocks the finish and
/// carries its concern; anything else - a plain approval, an ambiguous or
/// garbled answer, empty text - approves, so a confused reviewer relaxes the
/// gate rather than trapping the run.
pub(crate) fn parse_verdict(text: &str) -> FinishVerdict {
    let upper = text.to_ascii_uppercase();
    // A REVISE verdict only counts when it is not itself negated ("no revision
    // needed", "nothing to revise") - the reviewer's own approval language.
    let revises = upper.contains("REVISE")
        && !upper.contains("NO REVISE")
        && !upper.contains("NOTHING TO REVISE")
        && !upper.contains("NO REVISION")
        && !upper.contains("APPROVE");
    if !revises {
        return FinishVerdict {
            approved: true,
            concern: String::new(),
            model_id: String::new(),
            input_tokens: 0,
            output_tokens: 0,
        };
    }
    // Pull the concern that follows the first REVISE marker, bounded so a
    // runaway reply cannot bloat the receipt or the re-work prompt.
    let concern = text
        .split_once("REVISE")
        .map(|(_, rest)| rest.trim_start_matches([':', ' ', '-']).trim())
        .filter(|rest| !rest.is_empty())
        .unwrap_or("the change may not fully accomplish the task")
        .chars()
        .take(400)
        .collect::<String>();
    FinishVerdict {
        approved: false,
        concern,
        model_id: String::new(),
        input_tokens: 0,
        output_tokens: 0,
    }
}

/// The fresh-eyes prompt: the reviewer is told it has no shared context and
/// must reason from the diff to the task, and is held to the APPROVE / REVISE
/// contract so the reply parses.
pub(crate) fn evaluator_messages(task: &str, diff: &str) -> Vec<serde_json::Value> {
    let system = "You are a skeptical senior reviewer with fresh eyes. You did not \
write this change and have none of the author's context - you see only the task and \
the diff. Reason backward from the diff to the task: does the change actually \
accomplish what was asked, without leaving the objective half-done, breaking an \
adjacent behavior, or passing a check while missing the point? Judge only what the \
diff shows; do not assume unseen work. Reply with exactly one of: `APPROVE` when the \
diff fully accomplishes the task, or `REVISE: <one concrete gap>` naming the single \
most important thing still missing.";
    let user = format!(
        "Task:\n{task}\n\nDiff (unified, may be truncated):\n{diff}\n\nVerdict (APPROVE or REVISE: <gap>):"
    );
    vec![
        serde_json::json!({ "role": "system", "content": system }),
        serde_json::json!({ "role": "user", "content": user }),
    ]
}

/// The uncommitted workspace diff at finish time, bounded. The run has not
/// committed to a branch yet at the auto-finish gate, so the change lives only
/// in the workspace - `add -N` makes new files visible to `diff` without
/// staging content. Returns None when there is no change to review or git is
/// unavailable, in which case the caller skips the evaluator.
pub(crate) fn workspace_diff(workspace_path: &Path) -> Option<String> {
    let _ = std::process::Command::new("git")
        .arg("-C")
        .arg(workspace_path)
        .args(["add", "-N", "."])
        .output();
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(workspace_path)
        .args(["diff", "--no-color"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    const LIMIT: usize = 12_000;
    if trimmed.len() <= LIMIT {
        Some(trimmed.to_string())
    } else {
        let mut end = LIMIT;
        while !trimmed.is_char_boundary(end) {
            end -= 1;
        }
        Some(format!(
            "{}\n... (diff truncated at {LIMIT} bytes for review)",
            &trimmed[..end]
        ))
    }
}

/// Run the fresh-eyes evaluator for a proposed finish. Returns None - meaning
/// "do not block, finish as usual" - when there is no reviewer model, no diff
/// to review, or the model call fails. Some(verdict) carries the reviewer's
/// approve/revise decision.
pub(crate) fn evaluate_finish(
    kernel: &Arc<RwLock<MdxKernel>>,
    tenant_id: &str,
    task: &str,
    diff: &str,
    reasoning_effort: &str,
) -> Option<FinishVerdict> {
    // Resolve the reviewer client under a brief lock, then drop it before
    // the model call so the loop's suspend-for-external-call discipline holds.
    let client = {
        let mut guard = kernel.write().ok()?;
        match TurnClient::routed_for_role(
            &mut guard,
            tenant_id,
            "agent:forge-finish-evaluator",
            "forge-finish-evaluator",
            "evaluator",
            reasoning_effort,
        ) {
            Ok(Some(client)) => client,
            Ok(None) => TurnClient::for_role_with_reasoning_effort(
                &guard,
                tenant_id,
                "reviewer",
                reasoning_effort,
            )
            .or_else(|| {
                TurnClient::for_role_with_reasoning_effort(
                    &guard,
                    tenant_id,
                    "planner",
                    reasoning_effort,
                )
            })?,
            Err(_) => return None,
        }
    };
    let messages = evaluator_messages(task, diff);
    let started = std::time::Instant::now();
    let turn = match client.complete(&messages) {
        Ok(turn) => turn,
        Err(_) => {
            if let Ok(mut guard) = kernel.write() {
                let _ = crate::model_fabric_service::persist_pending_model_runtime_evidence(
                    &mut guard,
                    tenant_id,
                    "agent:forge-finish-evaluator",
                );
            }
            return None;
        }
    };
    if let Ok(mut guard) = kernel.write() {
        let _ = client.record_route_outcome(
            &mut guard,
            tenant_id,
            "agent:forge-finish-evaluator",
            &turn,
            started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
            "forge_evaluator_runtime",
        );
    }
    let mut verdict = parse_verdict(&turn.text);
    verdict.model_id = client.model_id().to_string();
    verdict.input_tokens = turn.input_tokens;
    verdict.output_tokens = turn.output_tokens;
    Some(verdict)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approve_reply_does_not_block() {
        let verdict =
            parse_verdict("APPROVE - the diff adds the missing guard and the test covers it.");
        assert!(verdict.approved);
        assert!(verdict.concern.is_empty());
    }

    #[test]
    fn revise_reply_blocks_and_carries_the_concern() {
        let verdict = parse_verdict(
            "REVISE: the task asked to handle the empty-input case but the diff only fixes the happy path.",
        );
        assert!(!verdict.approved);
        assert!(verdict.concern.contains("empty-input case"));
    }

    #[test]
    fn negated_revise_language_still_approves() {
        assert!(parse_verdict("No revision needed; APPROVE.").approved);
        assert!(parse_verdict("Nothing to revise here.").approved);
    }

    #[test]
    fn ambiguous_or_empty_reply_fails_open() {
        assert!(parse_verdict("").approved);
        assert!(parse_verdict("Looks fine to me.").approved);
        assert!(parse_verdict("I am not sure what to say.").approved);
    }

    #[test]
    fn concern_is_bounded() {
        let long = format!("REVISE: {}", "x".repeat(1000));
        let verdict = parse_verdict(&long);
        assert!(!verdict.approved);
        assert!(verdict.concern.len() <= 400);
    }

    #[test]
    fn evaluator_prompt_states_fresh_eyes_and_the_verdict_contract() {
        let messages = evaluator_messages("Fix the failing test", "diff --git a/x b/x");
        let system = messages[0]["content"].as_str().unwrap();
        assert!(system.contains("fresh eyes"));
        assert!(system.contains("APPROVE"));
        assert!(system.contains("REVISE"));
        let user = messages[1]["content"].as_str().unwrap();
        assert!(user.contains("Fix the failing test"));
        assert!(user.contains("diff --git a/x b/x"));
    }

    #[test]
    fn max_blocks_has_a_floor_of_one() {
        // A misconfigured 0 must not disable the seam's ability to relent nor
        // trap a run: the floor keeps it at the default.
        unsafe {
            std::env::set_var("MDX_FORGE_FINISH_EVALUATOR_MAX_BLOCKS", "0");
        }
        assert!(max_finish_blocks() >= 1);
        unsafe {
            std::env::remove_var("MDX_FORGE_FINISH_EVALUATOR_MAX_BLOCKS");
        }
    }
}
