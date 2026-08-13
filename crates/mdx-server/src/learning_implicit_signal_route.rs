// Implicit learning signals over HTTP, plus the fail-soft derivation helper the
// Forge disposition routes call. A human never assembles these: they are the
// zero-cost learning source the frontier mines. The Twin answer correction is
// the one user-initiated implicit signal - a person says an answer was not
// right and the kernel records a governed learning.implicit.signal citing the
// twin session receipt. Everything here stays advisory: no memory activation,
// adaptation, or production write opens.
use crate::RouteResponse;
use mdx_core::{GovernedWriteIdentity, LearningImplicitSignal, MdxKernel, json_string_literal};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/learning/implicit-signals.json" => Some(handle_post(method, body, kernel)),
        "/learning/implicit-signals/projection.json" => Some(handle_projection(method, kernel)),
        _ => None,
    }
}

/// The single seam every disposition route calls after its own governed write
/// succeeds. It is fail-soft by contract: a disposition (ship, reject, revise)
/// must never break because its implicit learning signal could not be derived,
/// so a refusal is witnessed on stderr, never raised. The caller already holds
/// the kernel write lock.
pub(crate) fn derive_implicit_signal(
    kernel: &mut MdxKernel,
    identity: &GovernedWriteIdentity,
    signal: LearningImplicitSignal<'_>,
) {
    if let Err(error) = kernel.record_learning_implicit_signal_with_identity(signal, identity) {
        eprintln!(
            "implicit learning signal ({}) not derived for source {}: {}",
            signal.signal_kind,
            signal.source_receipt_id,
            error.message()
        );
    }
}

/// The deterministic draft lesson for a Forge disposition signal. This is the
/// text the lane folds into a candidate a human edits and gates; it names the
/// disposition, the work, and the delta so the reviewer starts from evidence.
pub(crate) fn disposition_lesson(signal_kind: &str, work_item: &str, delta: &str) -> String {
    let work = if work_item.trim().is_empty() {
        "this work".to_string()
    } else {
        format!("this work ({work_item})")
    };
    let delta_clause = if delta.trim().is_empty() {
        String::new()
    } else {
        format!(" What changed: {}.", delta.trim())
    };
    match signal_kind {
        "change_accepted" => format!(
            "A change on {work} was accepted as produced - a clean acceptance worth remembering for similar work.{delta_clause}"
        ),
        "change_rejected" => format!(
            "A change on {work} was rejected. Treat this kind of change on this kind of work as one that needs a different approach.{delta_clause}"
        ),
        "revision_requested" => format!(
            "A change on {work} was sent back for revision before it could ship.{delta_clause} Consider what the first pass missed."
        ),
        "edit_after_accept" => format!(
            "A change on {work} was edited right after it was accepted - the accept missed something the edit caught.{delta_clause}"
        ),
        _ => format!("A disposition on {work} produced a learning signal.{delta_clause}"),
    }
}

fn handle_post(
    method: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "POST") {
        return Ok(response);
    }
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    let field = |name: &str| json_string_field(body, name).unwrap_or_default();
    // The user-initiated implicit signal is a Twin answer correction. The route
    // defaults to it, and the kernel coherence table refuses any signal_kind
    // that does not match the cited receipt's kind.
    let signal_kind = {
        let requested = field("signal_kind");
        if requested.trim().is_empty() {
            "twin_answer_corrected".to_string()
        } else {
            requested
        }
    };
    let source_receipt_id = field("source_receipt_id");
    let note = field("note");
    let work_item = field("work_item");
    let lesson_candidate = {
        let provided = field("lesson_candidate");
        if !provided.trim().is_empty() {
            provided
        } else if signal_kind == "twin_answer_corrected" {
            let tail = if note.trim().is_empty() {
                String::new()
            } else {
                format!(" They said: {}.", note.trim())
            };
            format!(
                "A person marked a Twin answer as not right.{tail} Review what the answer got wrong and what it should have known."
            )
        } else {
            disposition_lesson(&signal_kind, &work_item, &note)
        }
    };
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let report = match kernel.record_learning_implicit_signal_with_identity(
        LearningImplicitSignal {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            source_receipt_id: &source_receipt_id,
            signal_kind: &signal_kind,
            work_item: &work_item,
            delta_summary: &note,
            lesson_candidate: &lesson_candidate,
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => return Ok(refusal(&error.message())),
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-learning-implicit-signal-local-post","status":"RECORDED","auth_session_status":{},"implicit_signal_id":{},"signal_kind":{},"source_receipt_id":{},"source_receipt_kind":{},"implicit_signal_receipt_id":{},"policy_decision_id":{},"projection_route":"/learning/implicit-signals/projection.json","learning_candidate_allowed":true,"active_memory_write_allowed":false,"adaptation_allowed":false,"runtime_behavior_change_allowed":false,"production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&report.implicit_signal_id),
            json_string_literal(&report.signal_kind),
            json_string_literal(&report.source_receipt_id),
            json_string_literal(&report.source_receipt_kind),
            json_string_literal(&report.receipt_id),
            json_string_literal(&report.policy_decision_id),
        ),
    ))
}

fn handle_projection(
    method: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "GET") {
        return Ok(response);
    }
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let signals: Vec<String> = kernel
        .ledger()
        .query()
        .by_kind("learning.implicit.signal")
        .iter()
        .map(|receipt| {
            let value = |key: &str| {
                json_string_literal(receipt.payload.get(key).map(String::as_str).unwrap_or(""))
            };
            format!(
                r#"{{"implicit_signal_receipt_id":{},"implicit_signal_id":{},"signal_kind":{},"source_receipt_id":{},"source_receipt_kind":{},"work_item":{},"delta_summary":{},"lesson_candidate":{},"high_signal_disagreement":{},"actor_id":{}}}"#,
                json_string_literal(&receipt.receipt_id),
                value("implicit_signal_id"),
                value("signal_kind"),
                value("source_receipt_id"),
                value("source_receipt_kind"),
                value("work_item"),
                value("delta_summary"),
                value("lesson_candidate"),
                value("high_signal_disagreement"),
                json_string_literal(receipt.actor_id.as_str()),
            )
        })
        .collect();
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-learning-implicit-signal-local-projection","receipt_kind":"learning.implicit.signal","writes_route":"/learning/implicit-signals.json","signal_count":{},"signals":[{}],"active_memory_write_allowed":false,"adaptation_allowed":false,"runtime_behavior_change_allowed":false,"production_write_allowed":false}}"#,
            signals.len(),
            signals.join(","),
        ),
    ))
}

fn refusal(reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-learning-implicit-signal-local-post","status":"REFUSED","reason":{},"implicit_signal_receipt_id":"","active_memory_write_allowed":false,"adaptation_allowed":false,"runtime_behavior_change_allowed":false,"production_write_allowed":false}}"#,
            json_string_literal(reason)
        ),
    )
}

fn json_string_field(body: &str, key: &str) -> Option<String> {
    let marker = format!("\"{key}\":");
    let after = body.split(&marker).nth(1)?;
    let after = after.trim_start();
    let rest = after.strip_prefix('"')?;
    let mut value = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(escaped) = chars.next() {
                    value.push(escaped);
                }
            }
            '"' => return Some(value),
            other => value.push(other),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdx_core::{ForgeRunEvent, GovernedWriteIdentity, TwinSessionDraftRequest};

    #[test]
    fn twin_correction_posts_and_projection_lists_it() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        // A grounded Twin answer receipt to cite.
        let answer_receipt_id = {
            let mut k = kernel.write().expect("kernel");
            k.run_evals_runner_agent().expect("seed evidence");
            let credential = k
                .ledger()
                .entries()
                .iter()
                .find(|receipt| receipt.kind == "credential.minted")
                .expect("credential evidence")
                .receipt_id
                .clone();
            let report = k
                .save_twin_session_draft_local(TwinSessionDraftRequest {
                    tenant_id: "local_tenant",
                    actor_id: "human:owner",
                    session_id: "twin_session_1",
                    companion_id: "twin_architect",
                    companion_stance: "risk_and_architecture",
                    persona_profile_id: "architect_stress_tests_assumptions",
                    prompt_shape: "help me think through the launch date",
                    evidence_receipt_id: &credential,
                    draft_text: "We should decide the launch date.",
                    live_answer: None,
                    recall_packet: None,
                })
                .expect("twin draft");
            report.answer_receipt_id.clone()
        };
        assert!(!answer_receipt_id.is_empty(), "twin answer receipt exists");
        let post = route_response(
            "POST",
            "/learning/implicit-signals.json",
            &format!(
                r#"{{"source_receipt_id":"{answer_receipt_id}","note":"That launch date is wrong.","work_item":"twin_session_1"}}"#
            ),
            &kernel,
        )
        .expect("route")
        .expect("response");
        assert!(post.body.contains("\"status\":\"RECORDED\""));
        assert!(
            post.body
                .contains("\"signal_kind\":\"twin_answer_corrected\"")
        );
        assert!(post.body.contains("\"adaptation_allowed\":false"));

        let projection = route_response(
            "GET",
            "/learning/implicit-signals/projection.json",
            "",
            &kernel,
        )
        .expect("route")
        .expect("response");
        assert!(projection.body.contains("\"signal_count\":1"));
        assert!(
            projection
                .body
                .contains("\"receipt_kind\":\"learning.implicit.signal\"")
        );
        assert!(projection.body.contains("not right"));
    }

    #[test]
    fn post_refuses_missing_source_receipt() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let post = route_response(
            "POST",
            "/learning/implicit-signals.json",
            r#"{"source_receipt_id":"receipt_missing","note":"wrong"}"#,
            &kernel,
        )
        .expect("route")
        .expect("response");
        assert!(post.body.contains("\"status\":\"REFUSED\""));
        assert!(post.body.contains("is unknown"));
    }

    #[test]
    fn derive_implicit_signal_is_fail_soft_on_a_missing_source() {
        // A disposition path calling the helper with a source that does not
        // resolve must not panic or raise; it swallows the refusal.
        let mut kernel = MdxKernel::boot_local();
        let identity = GovernedWriteIdentity::local_demo("human:eng");
        derive_implicit_signal(
            &mut kernel,
            &identity,
            LearningImplicitSignal {
                tenant_id: "t",
                actor_id: "human:eng",
                source_receipt_id: "receipt_missing",
                signal_kind: "change_rejected",
                work_item: "",
                delta_summary: "",
                lesson_candidate: "This should be swallowed.",
            },
        );
        // No implicit signal was recorded, and the ledger is intact.
        assert_eq!(
            kernel
                .ledger()
                .query()
                .by_kind("learning.implicit.signal")
                .len(),
            0
        );
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn derive_implicit_signal_records_a_revision_from_a_run_event() {
        let mut kernel = MdxKernel::boot_local();
        let identity = GovernedWriteIdentity::local_demo("agent:forge");
        kernel
            .record_forge_run_event_with_evidence_fields(
                ForgeRunEvent {
                    tenant_id: "t",
                    actor_id: "agent:forge",
                    run_id: "forge_run_revise_1",
                    event: "run_started",
                    work_item_id: "",
                    detail: "accepted revising=fleet/demo",
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &identity,
                &[],
            )
            .expect("run event");
        let source = kernel
            .ledger()
            .query()
            .by_kind("forge.run.event")
            .last()
            .expect("run receipt")
            .receipt_id
            .clone();
        let lesson =
            disposition_lesson("revision_requested", "forge_run_revise_1", "diff too broad");
        derive_implicit_signal(
            &mut kernel,
            &identity,
            LearningImplicitSignal {
                tenant_id: "t",
                actor_id: "human:eng",
                source_receipt_id: &source,
                signal_kind: "revision_requested",
                work_item: "forge_run_revise_1",
                delta_summary: "diff too broad",
                lesson_candidate: &lesson,
            },
        );
        assert_eq!(
            kernel
                .ledger()
                .query()
                .by_kind("learning.implicit.signal")
                .len(),
            1
        );
    }
}
