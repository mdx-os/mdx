// The decision graph: the company's recorded judgment, projected from the
// receipt ledger into decision-record nodes. Every node and every edge is
// DERIVED from receipts already on the chain - never asserted, never an LLM
// reading the ledger and guessing. A decision is a human verb that the
// kernel already wrote down (a fleet ratified, a build shipped, a bet
// resolved); this surface reads those verbs back as a queryable precedent
// graph with the canonical record shape: a stable id, the decision class,
// a lifecycle status, valid-from ordering, what it touched, what it
// superseded, and - the edge the market cannot fake without an enforced
// substrate - the OUTCOME the decision actually led to.
//
// Read-only. Opens no authority, writes nothing, hides nothing.
use crate::RouteResponse;
use mdx_core::{MdxKernel, Receipt};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    _body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path != "/pages/decision-graph/projection.json" {
        return None;
    }
    if !method.eq_ignore_ascii_case("GET") {
        return Some(Ok(RouteResponse::text(
            "405 Method Not Allowed",
            "method not allowed\n".to_string(),
        )));
    }
    let kernel = match kernel.read() {
        Ok(kernel) => kernel,
        Err(_) => return Some(Err("kernel lock poisoned".to_string())),
    };
    Some(Ok(RouteResponse::json("200 OK", render_json(&kernel))))
}

// The decision verbs the kernel already records. Each maps a receipt kind to
// the decision class, a default lifecycle status, and the payload key that
// names the subject the decision is ABOUT (the lineage key for supersession).
struct DecisionShape {
    kind: &'static str,
    class: &'static str,
    status: &'static str,
    subject_key: &'static str,
}

const DECISION_SHAPES: [DecisionShape; 14] = [
    DecisionShape {
        kind: "fleet.plan.ratified",
        class: "fleet_ratification",
        status: "accepted",
        subject_key: "fleet_id",
    },
    DecisionShape {
        kind: "forge.run.ship.decided",
        class: "ship_decision",
        status: "committed",
        subject_key: "run_id",
    },
    DecisionShape {
        kind: "forge.ship.ratified",
        class: "ship_decision",
        status: "committed",
        subject_key: "run_id",
    },
    DecisionShape {
        kind: "forge.ship.rejected",
        class: "ship_decision",
        status: "abandoned",
        subject_key: "run_id",
    },
    DecisionShape {
        kind: "forge.ship.revision_requested",
        class: "ship_decision",
        status: "proposed",
        subject_key: "run_id",
    },
    DecisionShape {
        kind: "forge.ship.escalated",
        class: "ship_decision",
        status: "proposed",
        subject_key: "run_id",
    },
    DecisionShape {
        kind: "forge.production.deployed",
        class: "deployment",
        status: "committed",
        subject_key: "run_id",
    },
    DecisionShape {
        kind: "product.ratification.recorded",
        class: "product_ratification",
        status: "accepted",
        subject_key: "bet_id",
    },
    DecisionShape {
        kind: "product.bet.resolved",
        class: "bet_resolution",
        status: "committed",
        subject_key: "bet_id",
    },
    DecisionShape {
        kind: "strategy.ratification.recorded",
        class: "strategy_ratification",
        status: "accepted",
        subject_key: "direction_id",
    },
    DecisionShape {
        kind: "strategy.direction.recorded",
        class: "strategy_direction",
        status: "accepted",
        subject_key: "direction_id",
    },
    DecisionShape {
        kind: "strategy.proposal.resolved",
        class: "strategy_resolution",
        status: "committed",
        subject_key: "proposal_id",
    },
    DecisionShape {
        kind: "pages.approval.decision.recorded",
        class: "pages_approval",
        status: "accepted",
        subject_key: "document_id",
    },
    DecisionShape {
        kind: "policy.decision.recorded",
        class: "policy_decision",
        status: "committed",
        subject_key: "policy_decision_id",
    },
];

fn shape_for(kind: &str) -> Option<&'static DecisionShape> {
    DECISION_SHAPES.iter().find(|shape| shape.kind == kind)
}

// Outcome kinds: a decision's realized consequence, recorded later in the same
// trace. These are RESULTS the world produced, not more judgment - so a
// decision -> outcome edge means "this call actually led to that happening".
const OUTCOME_KINDS: [&str; 4] = [
    "forge.production.deployed",
    "fleet.run.event",
    "forge.run.event",
    "eval.verdict.recorded",
];

// Entity id keys we will surface as decision -> entity edges when they appear
// literally in a decision's own payload. Only explicit references, never
// lexical guesses.
const ENTITY_KEYS: [(&str, &str); 9] = [
    ("fleet_id", "fleet"),
    ("bet_id", "bet"),
    ("run_id", "run"),
    ("forge_run_id", "run"),
    ("document_id", "page"),
    ("channel_id", "channel"),
    ("condition_id", "condition"),
    ("proposal_id", "proposal"),
    ("direction_id", "direction"),
];

fn payload<'a>(receipt: &'a Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}

// The subject this decision is about - the lineage key for supersession.
fn subject_id(receipt: &Receipt, shape: &DecisionShape) -> String {
    if shape.subject_key == "policy_decision_id" {
        return receipt.policy_decision_id.clone().unwrap_or_default();
    }
    payload(receipt, shape.subject_key).to_string()
}

// A short human title for the decision: the recorded verb plus the subject and
// the human's reason when one was required, trimmed. No invention.
fn title_for(receipt: &Receipt, shape: &DecisionShape, subject: &str) -> String {
    let verb = match shape.class {
        "fleet_ratification" => "Ratified fleet",
        "ship_decision" => match receipt.kind.as_str() {
            "forge.ship.rejected" => "Rejected ship of run",
            "forge.ship.revision_requested" => "Asked revision on run",
            "forge.ship.escalated" => "Escalated ship of run",
            _ => "Shipped run",
        },
        "deployment" => "Deployed run to production",
        "product_ratification" => "Product ratification on bet",
        "bet_resolution" => "Resolved bet",
        "strategy_ratification" => "Ratified strategy direction",
        "strategy_direction" => "Recorded strategy direction",
        "strategy_resolution" => "Resolved strategy proposal",
        "pages_approval" => "Approved page",
        "policy_decision" => "Policy decision",
        _ => "Decision",
    };
    let decision = payload(receipt, "decision");
    let head = if subject.is_empty() {
        verb.to_string()
    } else {
        format!("{verb} {subject}")
    };
    if !decision.is_empty() && shape.class != "ship_decision" {
        format!("{head}: {decision}")
    } else {
        head
    }
}

// The reason the human gave, if the verb required one. This is the "why" the
// market wants and most systems do not capture.
fn reason_for(receipt: &Receipt) -> String {
    for key in ["reason", "rationale", "justification", "context"] {
        let value = payload(receipt, key);
        if !value.is_empty() {
            return value.chars().take(280).collect();
        }
    }
    String::new()
}

fn render_json(kernel: &MdxKernel) -> String {
    let entries = kernel.ledger().entries();

    // Outcome index: every outcome-kind receipt, indexed by each entity id it
    // names AND by its trace. A decision finds its realized outcome through a
    // shared identifier - the SUBJECT it was about (a fleet, a run) showing up
    // again in a later result receipt. Both linking keys are literal: the same
    // id written on two receipts, never a lexical guess.
    let mut outcome_index: std::collections::BTreeMap<String, Vec<(usize, &Receipt)>> =
        std::collections::BTreeMap::new();
    for (seq, receipt) in entries.iter().enumerate() {
        if !OUTCOME_KINDS.contains(&receipt.kind.as_str()) {
            continue;
        }
        // Key on the trace and on every entity id this outcome names.
        let mut keys: Vec<String> = vec![format!("trace:{}", receipt.trace_id.as_str())];
        for (key, _) in ENTITY_KEYS {
            let value = payload(receipt, key);
            if !value.is_empty() {
                keys.push(format!("id:{value}"));
            }
        }
        keys.sort();
        keys.dedup();
        for key in keys {
            outcome_index.entry(key).or_default().push((seq, receipt));
        }
    }

    // The realized outcome of a decision: the LATEST later outcome receipt that
    // shares the decision's subject id, else the latest in its trace. Latest,
    // not first, so the edge points at the settled result, not a mid-flight
    // step. Returns (kind, receipt_id, seq, event, detail).
    let latest_for_key = |key: &str, decision_kind: &str, after: usize| {
        let mut best: Option<(usize, &Receipt)> = None;
        if let Some(members) = outcome_index.get(key) {
            for (seq, receipt) in members {
                if *seq <= after || receipt.kind == decision_kind {
                    continue;
                }
                if best.map(|(b, _)| *seq > b).unwrap_or(true) {
                    best = Some((*seq, receipt));
                }
            }
        }
        best
    };
    let find_outcome = |subject: &str, trace: &str, decision_kind: &str, after: usize| {
        let mut best: Option<(usize, &Receipt)> = None;
        if !subject.is_empty() {
            best = latest_for_key(&format!("id:{subject}"), decision_kind, after);
        }
        if best.is_none() {
            best = latest_for_key(&format!("trace:{trace}"), decision_kind, after);
        }
        best.map(|(seq, receipt)| {
            (
                receipt.kind.clone(),
                receipt.receipt_id.clone(),
                seq,
                payload(receipt, "event").to_string(),
                payload(receipt, "detail")
                    .chars()
                    .take(160)
                    .collect::<String>(),
            )
        })
    };

    // First pass: build the raw decision nodes with their subject lineage key.
    // A node is either DERIVED (projected from a decision receipt) or ASSERTED
    // (a human published a page whose type is `decision`). Both live in one
    // graph; origin says which, and the graph never pretends one is the other.
    struct Node {
        seq: usize,
        receipt_id: String,
        class: &'static str,
        status: String,
        subject: String,
        subject_key: &'static str,
        title: String,
        reason: String,
        actor: String,
        trace_id: String,
        touches: Vec<(String, String)>,
        outcome: Option<(String, String, usize, String, String)>,
        origin: &'static str,
        body_route: String,
    }

    let mut nodes: Vec<Node> = Vec::new();
    for (seq, receipt) in entries.iter().enumerate() {
        let Some(shape) = shape_for(&receipt.kind) else {
            continue;
        };
        let subject = subject_id(receipt, shape);

        // decision -> entity edges: ids the decision literally recorded.
        let mut touches: Vec<(String, String)> = Vec::new();
        for (key, entity_kind) in ENTITY_KEYS {
            let value = payload(receipt, key);
            if value.is_empty() {
                continue;
            }
            if touches.iter().any(|(_, id)| id == value) {
                continue;
            }
            touches.push((entity_kind.to_string(), value.to_string()));
        }

        // decision -> outcome edge: the realized consequence this decision led
        // to, found through the subject it named or its trace. A real result,
        // not more judgment.
        let outcome = find_outcome(&subject, receipt.trace_id.as_str(), &receipt.kind, seq);

        nodes.push(Node {
            seq,
            receipt_id: receipt.receipt_id.clone(),
            class: shape.class,
            status: shape.status.to_string(),
            subject: subject.clone(),
            subject_key: shape.subject_key,
            title: title_for(receipt, shape, &subject),
            reason: reason_for(receipt),
            actor: receipt.actor_id.as_str().to_string(),
            trace_id: receipt.trace_id.as_str().to_string(),
            touches,
            outcome,
            origin: "derived",
            body_route: String::new(),
        });
    }

    // Asserted decisions: a human published a page whose declared type is
    // `decision` (swing #2 made type first-class). That published page IS an
    // asserted decision record - the why lives in its body, which we LINK to
    // rather than read into the graph (the same never-read-the-body law the
    // world model holds). The lineage key is the document_id, so republishing
    // the page supersedes the prior revision exactly like a derived lineage.
    for (seq, receipt) in entries.iter().enumerate() {
        if receipt.kind != "pages.document.published" {
            continue;
        }
        let page_type = payload(receipt, "page_type");
        if page_type != "decision" {
            continue;
        }
        let document_id = payload(receipt, "document_id").to_string();
        if document_id.is_empty() {
            continue;
        }
        let title = {
            let recorded = payload(receipt, "title");
            if recorded.is_empty() {
                document_id.clone()
            } else {
                recorded.to_string()
            }
        };
        nodes.push(Node {
            seq,
            receipt_id: receipt.receipt_id.clone(),
            class: "asserted_decision",
            status: "accepted".to_string(),
            subject: document_id.clone(),
            subject_key: "document_id",
            title,
            // The why is in the page body; we link, never lift it lexically.
            reason: String::new(),
            actor: receipt.actor_id.as_str().to_string(),
            trace_id: receipt.trace_id.as_str().to_string(),
            touches: vec![("page".to_string(), document_id.clone())],
            outcome: None,
            origin: "asserted",
            body_route: format!("/pages/{document_id}/body"),
        });
    }

    // Second pass: supersession lineage. Within a (class, subject) lineage, an
    // earlier decision is superseded by the next one. Strict id match only.
    let mut superseded_by: Vec<String> = vec![String::new(); nodes.len()];
    let mut supersedes: Vec<String> = vec![String::new(); nodes.len()];
    for i in 0..nodes.len() {
        if nodes[i].subject.is_empty() {
            continue;
        }
        // The earliest later node in the same lineage supersedes node i.
        let mut next: Option<usize> = None;
        for j in 0..nodes.len() {
            if j == i
                || nodes[j].class != nodes[i].class
                || nodes[j].subject_key != nodes[i].subject_key
                || nodes[j].subject != nodes[i].subject
                || nodes[j].seq <= nodes[i].seq
            {
                continue;
            }
            if next.map(|n| nodes[j].seq < nodes[n].seq).unwrap_or(true) {
                next = Some(j);
            }
        }
        if let Some(j) = next {
            superseded_by[i] = nodes[j].receipt_id.clone();
            supersedes[j] = nodes[i].receipt_id.clone();
        }
    }

    let documents: Vec<serde_json::Value> = nodes
        .iter()
        .enumerate()
        .map(|(i, node)| {
            // A superseded decision is no longer the live one in its lineage.
            let status = if !superseded_by[i].is_empty() {
                "superseded".to_string()
            } else {
                node.status.clone()
            };
            // Confidence reads off provenance. A derived decision's strength is
            // how grounded its edges are; an asserted decision is a human's
            // stated call - high-trust on the why, with no machine-derived
            // outcome unless one later links to it.
            let confidence = if node.origin == "asserted" {
                "stated"
            } else if !node.subject.is_empty() && node.outcome.is_some() {
                "high"
            } else if !node.subject.is_empty() || node.outcome.is_some() {
                "medium"
            } else {
                "low"
            };
            let (derived_by, review_state) = if node.origin == "asserted" {
                ("", "authored")
            } else {
                ("ledger-decision-projection", "machine")
            };
            let touches: Vec<serde_json::Value> = node
                .touches
                .iter()
                .map(|(kind, id)| serde_json::json!({ "kind": kind, "id": id }))
                .collect();
            let outcome = node.outcome.as_ref().map(|(kind, id, seq, event, detail)| {
                serde_json::json!({
                    "kind": kind,
                    "receipt_id": id,
                    "valid_from": seq,
                    "event": event,
                    "detail": detail,
                })
            });
            serde_json::json!({
                "id": node.receipt_id,
                "type": "decision",
                "decision_class": node.class,
                "status": status,
                "valid_from": node.seq,
                "title": node.title,
                "subject": { "kind": node.subject_key, "id": node.subject },
                "reason": node.reason,
                "decided_by": node.actor,
                "trace_id": node.trace_id,
                "touches": touches,
                "outcome": outcome,
                "body_route": node.body_route,
                "supersedes": supersedes[i],
                "superseded_by": superseded_by[i],
                "provenance": {
                    "origin": node.origin,
                    "derived_by": derived_by,
                    "confidence": confidence,
                    "review_state": review_state,
                },
            })
        })
        .collect();

    // Lineage rollup: how many live vs superseded decisions per class. A cheap
    // legible read of where judgment is settled and where it keeps moving.
    let mut class_counts: std::collections::BTreeMap<&str, (usize, usize)> =
        std::collections::BTreeMap::new();
    for (i, node) in nodes.iter().enumerate() {
        let slot = class_counts.entry(node.class).or_insert((0, 0));
        if superseded_by[i].is_empty() {
            slot.0 += 1;
        } else {
            slot.1 += 1;
        }
    }
    let classes: Vec<serde_json::Value> = class_counts
        .iter()
        .map(|(class, (live, superseded))| {
            serde_json::json!({ "class": class, "live": live, "superseded": superseded })
        })
        .collect();
    let with_outcome = nodes.iter().filter(|n| n.outcome.is_some()).count();
    let asserted_count = nodes.iter().filter(|n| n.origin == "asserted").count();
    let derived_count = nodes.len() - asserted_count;

    serde_json::json!({
        "name": "mdx-pages-decision-graph-projection",
        "status": "OK",
        "derivation": "one graph of two origins: derived decisions read from recorded receipts (never an LLM reading the ledger), and asserted decisions a human published as decision-typed pages (the why linked, never lifted from the body)",
        "schema": "id,type,decision_class,status,valid_from,subject,reason,touches,outcome,body_route,supersedes,superseded_by,provenance",
        "declared_not_built": {
            "valid_until": "wall-clock validity lands when receipts carry timestamps - ordering here is by ledger sequence",
            "asserted_outcome_edges": "an asserted decision links to its outcome once the author references the entity it touched - today asserted nodes carry no derived outcome"
        },
        "decision_count": documents.len(),
        "derived_count": derived_count,
        "asserted_count": asserted_count,
        "with_outcome": with_outcome,
        "classes": classes,
        "decisions": documents,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdx_core::MdxKernel;

    fn kernel_with_seed() -> MdxKernel {
        let mut kernel = MdxKernel::boot_local();
        kernel.run_evals_runner_agent().expect("seed");
        kernel
    }

    #[test]
    fn shapes_are_unique_and_resolvable() {
        for shape in DECISION_SHAPES {
            assert!(
                shape_for(shape.kind).is_some(),
                "kind {} resolves",
                shape.kind
            );
        }
    }

    #[test]
    fn projection_is_valid_json_with_schema_envelope() {
        let kernel = kernel_with_seed();
        let body = render_json(&kernel);
        let value: serde_json::Value = serde_json::from_str(&body).expect("valid json");
        assert_eq!(value["status"], "OK");
        assert_eq!(value["name"], "mdx-pages-decision-graph-projection");
        assert!(value["decisions"].is_array());
        // Every node declares a known origin and is typed as a decision.
        for node in value["decisions"].as_array().unwrap() {
            let origin = node["provenance"]["origin"].as_str().unwrap();
            assert!(
                origin == "derived" || origin == "asserted",
                "origin {origin}"
            );
            assert_eq!(node["type"], "decision");
        }
    }

    #[test]
    fn decision_typed_page_becomes_an_asserted_node() {
        let mut kernel = kernel_with_seed();
        let source_receipt_id = kernel
            .ledger()
            .entries()
            .first()
            .map(|receipt| receipt.receipt_id.clone())
            .expect("source receipt");
        kernel
            .save_pages_publication_local(mdx_core::PagesPublication {
                tenant_id: "tenant_local",
                actor_id: "human:founder",
                document_id: "page_route_by_sensitivity",
                title: "Route model calls by data sensitivity",
                body_ref: "world_model://pages/page_route_by_sensitivity/body/v1",
                source_receipt_id: &source_receipt_id,
                revision_id: "rev_route_001",
                page_type: "decision",
            })
            .expect("publish decision page");

        let value: serde_json::Value =
            serde_json::from_str(&render_json(&kernel)).expect("valid json");
        assert_eq!(value["asserted_count"], 1);
        let asserted = value["decisions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|node| node["provenance"]["origin"] == "asserted")
            .expect("an asserted node");
        assert_eq!(asserted["decision_class"], "asserted_decision");
        assert_eq!(asserted["title"], "Route model calls by data sensitivity");
        assert_eq!(asserted["decided_by"], "human:founder");
        assert_eq!(asserted["provenance"]["confidence"], "stated");
        assert_eq!(asserted["provenance"]["review_state"], "authored");
        assert_eq!(
            asserted["body_route"],
            "/pages/page_route_by_sensitivity/body"
        );
        // The why is linked, never lifted from the body into the graph.
        assert_eq!(asserted["reason"], "");
    }
}
