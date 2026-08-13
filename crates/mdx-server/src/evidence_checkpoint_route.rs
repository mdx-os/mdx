// The evidence checkpoint routes. POST cuts a checkpoint over everything
// since the last one and records the act as a receipt - safe metadata
// only, no payload text, no authority opened. The projection serves the
// chain of claims so an operator or auditor can see what is covered.
use crate::RouteResponse;
use mdx_core::{EVIDENCE_CHECKPOINT_RECEIPT_KIND, MdxKernel, json_string_literal};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/audit/evidence-checkpoints.json" => Some(handle_post(method, body, kernel)),
        "/audit/evidence-checkpoints/projection.json" => Some(handle_projection(method, kernel)),
        "/audit/evidence-anchors.json" => Some(handle_anchor(method, body, kernel)),
        _ => None,
    }
}

fn handle_anchor(
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
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    // Anchor the latest checkpoint that is not anchored yet.
    let latest = kernel
        .ledger()
        .query()
        .by_kind(EVIDENCE_CHECKPOINT_RECEIPT_KIND)
        .into_iter()
        .last()
        .map(|receipt| {
            let field = |key: &str| receipt.payload.get(key).cloned().unwrap_or_default();
            (
                field("checkpoint_id"),
                field("checkpoint_hash"),
                field("merkle_root"),
                field("signature"),
            )
        });
    let Some((checkpoint_id, checkpoint_hash, merkle_root, signature)) = latest else {
        return Ok(RouteResponse::json(
            "200 OK",
            r#"{"name":"mdx-evidence-anchor-local-post","status":"REFUSED","reason":"there is no checkpoint to anchor yet","production_write_allowed":false}"#.to_string(),
        ));
    };
    let adapter = crate::evidence_anchor::LocalFileAnchorAdapter::from_env();
    let outcome = {
        use crate::evidence_anchor::AuditAnchorAdapter;
        adapter.anchor(&crate::evidence_anchor::AnchorRequest {
            checkpoint_id: &checkpoint_id,
            checkpoint_hash: &checkpoint_hash,
            merkle_root: &merkle_root,
            signature: &signature,
        })
    };
    let (status, anchor_adapter, anchor_ref) = match outcome {
        Ok(result) => (result.status, result.adapter, result.anchor_ref),
        Err(reason) => {
            return Ok(RouteResponse::json(
                "200 OK",
                format!(
                    r#"{{"name":"mdx-evidence-anchor-local-post","status":"REFUSED","reason":{},"production_write_allowed":false}}"#,
                    json_string_literal(&reason)
                ),
            ));
        }
    };
    let anchor_receipt_id = kernel.save_evidence_anchor_local_with_identity(
        &resolved.tenant_id,
        &resolved.actor_id,
        &resolved.identity,
        &checkpoint_id,
        &checkpoint_hash,
        &merkle_root,
        &anchor_adapter,
        &status,
        &anchor_ref,
    )?;
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-evidence-anchor-local-post","status":"RECORDED","auth_session_status":{},"checkpoint_id":{},"anchor_adapter":{},"anchor_status":{},"anchor_ref":{},"anchor_receipt_id":{},"authority_opened":"none","production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&checkpoint_id),
            json_string_literal(&anchor_adapter),
            json_string_literal(&status),
            json_string_literal(&anchor_ref),
            json_string_literal(&anchor_receipt_id),
        ),
    ))
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
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let report = match kernel.save_evidence_checkpoint_local_with_identity(
        &resolved.tenant_id,
        &resolved.actor_id,
        &resolved.identity,
        |checkpoint_hash| {
            crate::evidence_signing::sign_checkpoint_hash(checkpoint_hash)
                .map(|signed| signed.map(|signature| (signature.key_id, signature.signature_hex)))
        },
    ) {
        Ok(report) => report,
        Err(reason) => {
            return Ok(RouteResponse::json(
                "200 OK",
                format!(
                    r#"{{"name":"mdx-evidence-checkpoint-local-post","status":"REFUSED","reason":{},"production_write_allowed":false}}"#,
                    json_string_literal(&reason)
                ),
            ));
        }
    };
    // A cut checkpoint is a durability commit point: drop the kernel lock and
    // force the snapshot now rather than waiting for the coalescing window, so
    // the signed checkpoint and the receipts it covers are on disk before this
    // returns. A no-op in synchronous mode, where the write already landed.
    drop(kernel);
    if let Err(error) = crate::kernel_snapshot::flush_now() {
        eprintln!("mdx-server checkpoint snapshot flush failed: {error}");
    }
    let checkpoint = &report.checkpoint;
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-evidence-checkpoint-local-post","status":"RECORDED","auth_session_status":{},"checkpoint_id":{},"checkpoint_version":{},"range_start_receipt_id":{},"range_end_receipt_id":{},"receipt_count":{},"ledger_head_hash":{},"previous_checkpoint_id":{},"previous_checkpoint_hash":{},"merkle_root":{},"hash_algorithm":{},"checkpoint_hash":{},"external_anchor_status":{},"signature_algorithm":{},"signer_key_id":{},"signature":{},"checkpoint_receipt_id":{},"policy_decision_id":{},"authority_opened":"none","projection_route":"/audit/evidence-checkpoints/projection.json","production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&checkpoint.checkpoint_id),
            checkpoint.checkpoint_version,
            json_string_literal(&checkpoint.range_start_receipt_id),
            json_string_literal(&checkpoint.range_end_receipt_id),
            checkpoint.receipt_count,
            json_string_literal(&checkpoint.ledger_head_hash),
            json_string_literal(checkpoint.previous_checkpoint_id.as_deref().unwrap_or("")),
            json_string_literal(checkpoint.previous_checkpoint_hash.as_deref().unwrap_or("")),
            json_string_literal(&checkpoint.merkle_root),
            json_string_literal(checkpoint.hash_algorithm),
            json_string_literal(&checkpoint.checkpoint_hash),
            json_string_literal(checkpoint.external_anchor_status),
            json_string_literal(if report.signature.is_empty() {
                ""
            } else {
                "ed25519"
            }),
            json_string_literal(&report.signer_key_id),
            json_string_literal(&report.signature),
            json_string_literal(&report.checkpoint_receipt_id),
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
    // Which checkpoint hashes have an anchor receipt: the projection shows
    // anchored vs not from the chain's own record, not a mutable field.
    let anchored: std::collections::BTreeSet<String> = kernel
        .ledger()
        .query()
        .by_kind(mdx_core::EVIDENCE_ANCHOR_RECEIPT_KIND)
        .iter()
        .filter_map(|receipt| receipt.payload.get("checkpoint_hash").cloned())
        .collect();
    let checkpoints: Vec<String> = kernel
        .ledger()
        .query()
        .by_kind(EVIDENCE_CHECKPOINT_RECEIPT_KIND)
        .iter()
        .map(|receipt| {
            let value = |key: &str| {
                json_string_literal(receipt.payload.get(key).map(String::as_str).unwrap_or(""))
            };
            let signed = receipt
                .payload
                .get("signature")
                .map(|s| !s.is_empty())
                .unwrap_or(false);
            let is_anchored = receipt
                .payload
                .get("checkpoint_hash")
                .map(|hash| anchored.contains(hash))
                .unwrap_or(false);
            format!(
                r#"{{"checkpoint_receipt_id":{},"checkpoint_id":{},"range_start_receipt_id":{},"range_end_receipt_id":{},"receipt_count":{},"merkle_root":{},"checkpoint_hash":{},"previous_checkpoint_id":{},"signed":{},"externally_anchored":{}}}"#,
                json_string_literal(&receipt.receipt_id),
                value("checkpoint_id"),
                value("range_start_receipt_id"),
                value("range_end_receipt_id"),
                value("receipt_count"),
                value("merkle_root"),
                value("checkpoint_hash"),
                value("previous_checkpoint_id"),
                signed,
                is_anchored,
            )
        })
        .collect();
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-evidence-checkpoint-local-projection","receipt_kind":"audit.evidence.checkpoint.recorded","checkpoint_count":{},"checkpoints":[{}],"authority_opened":"none","production_write_allowed":false}}"#,
            checkpoints.len(),
            checkpoints.join(","),
        ),
    ))
}

/// Export the latest checkpoint as an auditor packet: checkpoint fields,
/// signature when present, and the covered receipts' ids and leaf hashes.
/// No payload values - the leaves are hashes, which is the whole point.
pub(crate) fn export_latest_packet_cli() -> Result<String, String> {
    let mut kernel = mdx_core::MdxKernel::boot_local();
    let restored = crate::kernel_snapshot::restore_into(&mut kernel)
        .map_err(|error| format!("snapshot restore: {error}"))?;
    if restored.is_none() {
        return Err(
            "no kernel snapshot to export from - run the server in a snapshotting mode first"
                .to_string(),
        );
    }
    let entries = kernel.ledger().entries();
    let receipt = entries
        .iter()
        .rev()
        .find(|receipt| receipt.kind == mdx_core::EVIDENCE_CHECKPOINT_RECEIPT_KIND)
        .ok_or_else(|| "no evidence checkpoint in the restored chain".to_string())?;
    let field = |key: &str| receipt.payload.get(key).cloned().unwrap_or_default();
    let start = entries
        .iter()
        .position(|entry| entry.receipt_id == field("range_start_receipt_id"))
        .ok_or_else(|| "checkpoint start receipt missing".to_string())?;
    let end = entries
        .iter()
        .position(|entry| entry.receipt_id == field("range_end_receipt_id"))
        .ok_or_else(|| "checkpoint end receipt missing".to_string())?;
    let range = &entries[start..=end];
    let checkpoint_version = field("checkpoint_version").parse::<u64>().unwrap_or(0);
    let leaf_hashes = mdx_core::evidence_leaf_hashes_for_version(range, checkpoint_version);
    let receipt_ids: Vec<String> = range.iter().map(|entry| entry.receipt_id.clone()).collect();
    Ok(serde_json::json!({
        "name": "mdx-evidence-checkpoint-packet",
        "checkpoint_id": field("checkpoint_id"),
        "checkpoint_version": checkpoint_version,
        "range_start_receipt_id": field("range_start_receipt_id"),
        "range_end_receipt_id": field("range_end_receipt_id"),
        "receipt_count": field("receipt_count").parse::<u64>().unwrap_or(0),
        "ledger_head_hash": field("ledger_head_hash"),
        "previous_checkpoint_id": field("previous_checkpoint_id"),
        "previous_checkpoint_hash": field("previous_checkpoint_hash"),
        "merkle_root": field("merkle_root"),
        "hash_algorithm": field("hash_algorithm"),
        "checkpoint_hash": field("checkpoint_hash"),
        "signature_algorithm": field("signature_algorithm"),
        "signer_key_id": field("signer_key_id"),
        "signature": field("signature"),
        "receipt_ids": receipt_ids,
        "leaf_hashes": leaf_hashes,
    })
    .to_string())
}

/// Verify a packet with nothing but math and, when a public key is
/// configured, the signature. Returns the explicit verdict vocabulary.
pub(crate) fn verify_packet_cli(path: &str) -> Result<String, String> {
    let raw = std::fs::read_to_string(path).map_err(|error| format!("read {path}: {error}"))?;
    let packet: serde_json::Value =
        serde_json::from_str(&raw).map_err(|error| format!("parse {path}: {error}"))?;
    let text = |key: &str| packet[key].as_str().map(str::to_string).unwrap_or_default();
    let checkpoint_version = packet["checkpoint_version"].as_u64().unwrap_or(0);
    let checkpoint = mdx_core::EvidenceCheckpoint {
        checkpoint_id: text("checkpoint_id"),
        checkpoint_version,
        range_start_receipt_id: text("range_start_receipt_id"),
        range_end_receipt_id: text("range_end_receipt_id"),
        receipt_count: packet["receipt_count"].as_u64().unwrap_or(0) as usize,
        ledger_head_hash: text("ledger_head_hash"),
        previous_checkpoint_id: Some(text("previous_checkpoint_id")).filter(|s| !s.is_empty()),
        previous_checkpoint_hash: Some(text("previous_checkpoint_hash")).filter(|s| !s.is_empty()),
        merkle_root: text("merkle_root"),
        hash_algorithm: mdx_core::evidence_hash_algorithm_for_version(checkpoint_version),
        external_anchor_status: "not_anchored",
        checkpoint_hash: text("checkpoint_hash"),
    };
    if mdx_core::recompute_checkpoint_hash(&checkpoint) != checkpoint.checkpoint_hash {
        return Ok("CHECKPOINT_HASH_MISMATCH".to_string());
    }
    let leaves: Vec<String> = packet["leaf_hashes"]
        .as_array()
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    if leaves.len() != checkpoint.receipt_count {
        return Ok("RECEIPT_RANGE_INCOMPLETE".to_string());
    }
    match mdx_core::evidence_root_from_leaf_hashes(&leaves) {
        Ok(root) if root == checkpoint.merkle_root => {}
        Ok(_) => return Ok("MERKLE_ROOT_MISMATCH".to_string()),
        Err(error) => return Err(error),
    }
    if let Ok(public_key_path) = std::env::var("MDX_AUDIT_SIGNING_PUBLIC_KEY_PATH")
        && !public_key_path.trim().is_empty()
    {
        let signature = text("signature");
        if signature.is_empty() {
            return Ok("SIGNATURE_MISSING".to_string());
        }
        let public_key = std::fs::read(&public_key_path)
            .map_err(|error| format!("read {public_key_path}: {error}"))?;
        if crate::evidence_signing::verify_signature(
            &public_key,
            &checkpoint.checkpoint_hash,
            &signature,
        )
        .is_err()
        {
            return Ok("SIGNATURE_INVALID".to_string());
        }
    }
    Ok("VERIFIED".to_string())
}
