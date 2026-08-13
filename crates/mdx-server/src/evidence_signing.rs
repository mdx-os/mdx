//! Ed25519 signing for evidence checkpoints, through the ring primitive
//! already in the tree. Unconfigured is honest: checkpoints record as
//! integrity-only. Configured-but-broken fails closed: a half-working
//! signing setup must refuse, never silently downgrade to unsigned.
//!
//! Key posture: the private key (PKCS#8) lives wherever the operator
//! points MDX_AUDIT_SIGNING_PRIVATE_KEY_PATH; nothing is committed and no
//! key material is ever recorded. The public key (raw 32 bytes) verifies -
//! at boot it is the restore path's defense against a rewritten chain,
//! because an attacker who rewrites history cannot re-sign it.

use mdx_core::{DeploymentMode, EvidenceCheckpoint, MdxKernel, hex, verify_evidence_checkpoint};
use ring::signature::{ED25519, Ed25519KeyPair, KeyPair, UnparsedPublicKey};

pub(crate) struct CheckpointSignature {
    pub key_id: String,
    pub signature_hex: String,
}

/// Sign a checkpoint hash. None when signing is not configured;
/// Err when it is configured and cannot work.
pub(crate) fn sign_checkpoint_hash(
    checkpoint_hash: &str,
) -> Result<Option<CheckpointSignature>, String> {
    let key_path = match std::env::var("MDX_AUDIT_SIGNING_PRIVATE_KEY_PATH") {
        Ok(path) if !path.trim().is_empty() => path,
        _ => return Ok(None),
    };
    let key_id = std::env::var("MDX_AUDIT_SIGNING_KEY_ID")
        .ok()
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| {
            "MDX_AUDIT_SIGNING_KEY_ID is required when a signing key is configured".to_string()
        })?;
    let pkcs8 = std::fs::read(&key_path)
        .map_err(|error| format!("read signing key {key_path}: {error}"))?;
    let key_pair = Ed25519KeyPair::from_pkcs8(&pkcs8)
        .map_err(|_| format!("{key_path} is not a valid Ed25519 PKCS#8 key"))?;
    let signature = key_pair.sign(checkpoint_hash.as_bytes());
    Ok(Some(CheckpointSignature {
        key_id,
        signature_hex: hex(signature.as_ref()),
    }))
}

/// Verify a signature over a checkpoint hash with a raw 32-byte public key.
pub(crate) fn verify_signature(
    public_key_bytes: &[u8],
    checkpoint_hash: &str,
    signature_hex: &str,
) -> Result<(), String> {
    let signature = from_hex(signature_hex)?;
    UnparsedPublicKey::new(&ED25519, public_key_bytes)
        .verify(checkpoint_hash.as_bytes(), &signature)
        .map_err(|_| "the signature does not verify against this public key".to_string())
}

pub(crate) fn from_hex(text: &str) -> Result<Vec<u8>, String> {
    if !text.len().is_multiple_of(2) {
        return Err("odd-length hex".to_string());
    }
    (0..text.len() / 2)
        .map(|i| {
            u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).map_err(|error| format!("hex: {error}"))
        })
        .collect()
}

/// Generate a fresh signing key pair for an operator: PKCS#8 private key
/// and raw public key, written to the given directory. Convenience only -
/// any Ed25519 PKCS#8 source works.
pub(crate) fn mint_signing_key_cli(dir: &str) -> Result<String, String> {
    let rng = ring::rand::SystemRandom::new();
    let pkcs8 =
        Ed25519KeyPair::generate_pkcs8(&rng).map_err(|_| "key generation failed".to_string())?;
    let key_pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref())
        .map_err(|_| "generated key did not parse".to_string())?;
    std::fs::create_dir_all(dir).map_err(|error| format!("create {dir}: {error}"))?;
    let private_path = format!("{dir}/audit-signing.pk8");
    let public_path = format!("{dir}/audit-signing.pub");
    std::fs::write(&private_path, pkcs8.as_ref())
        .map_err(|error| format!("write {private_path}: {error}"))?;
    std::fs::write(&public_path, key_pair.public_key().as_ref())
        .map_err(|error| format!("write {public_path}: {error}"))?;
    Ok(format!(
        "audit signing key minted\nprivate: {private_path}\npublic: {public_path}\nset MDX_AUDIT_SIGNING_PRIVATE_KEY_PATH, MDX_AUDIT_SIGNING_KEY_ID, and (for verification) MDX_AUDIT_SIGNING_PUBLIC_KEY_PATH"
    ))
}

/// The restore-path teeth: after a chain is restored from disk or the
/// database, the latest signed checkpoint in it must still be true. An
/// attacker who rewrote history and recomputed the internal hashes cannot
/// re-sign the checkpoint, so either a signature fails or a Merkle root no
/// longer matches the restored receipts - and boot refuses.
///
/// This walks EVERY checkpoint in the chain, not just the latest: each
/// covered range is verified against its own checkpoint, and the
/// previous-checkpoint linkage is verified to form an unbroken spine. The
/// incremental design already commits transitively (the latest checkpoint's
/// range includes the prior checkpoint's receipt), but that safety is
/// emergent from a subtle argument plus the caller's verify_full; walking
/// the whole chain makes the guarantee direct and self-evident, and robust
/// against future checkpoint shapes that would break transitivity.
///
/// Enforcement is mode-aware. In optional mode (default; local-demo) a chain
/// with no public key or no signed checkpoint returns Ok(None). In required
/// mode (MDX_AUDIT_REQUIRE_SIGNED_EVIDENCE=1; production/regulated) the
/// absence of a public key, the absence of a signed checkpoint over a
/// non-empty chain, or an unsigned latest checkpoint each fails closed.
pub(crate) fn verify_restored_chain(
    kernel: &MdxKernel,
    mode: DeploymentMode,
) -> Result<Option<String>, String> {
    // Required when explicitly demanded, OR in production - the deployment
    // that faces real attackers must refuse a tampered or unsigned chain at
    // boot rather than fall back to the non-cryptographic FNV hash check
    // (which an attacker who edits the snapshot can recompute, since the
    // algorithm carries no secret). local-secure and local-demo stay
    // permissive: they are operator-controlled local boxes, and local-secure
    // can opt into the teeth with MDX_AUDIT_REQUIRE_SIGNED_EVIDENCE=1.
    let required = std::env::var("MDX_AUDIT_REQUIRE_SIGNED_EVIDENCE")
        .ok()
        .as_deref()
        == Some("1")
        || matches!(mode, DeploymentMode::Production);
    let public_key = match std::env::var("MDX_AUDIT_SIGNING_PUBLIC_KEY_PATH") {
        Ok(path) if !path.trim().is_empty() => Some(
            std::fs::read(&path)
                .map_err(|error| format!("read audit public key {path}: {error}"))?,
        ),
        _ => None,
    };
    if public_key.is_none() {
        if required {
            return Err(
                "required signed evidence: MDX_AUDIT_SIGNING_PUBLIC_KEY_PATH is not configured, so the restored chain cannot be held against any checkpoint".to_string(),
            );
        }
        return Ok(None);
    }
    let Some(public_key) = public_key else {
        return Ok(None);
    };

    let entries = kernel.ledger().entries();
    let checkpoint_receipts: Vec<&mdx_core::Receipt> = entries
        .iter()
        .filter(|receipt| receipt.kind == mdx_core::EVIDENCE_CHECKPOINT_RECEIPT_KIND)
        .collect();

    if checkpoint_receipts.is_empty() {
        if required && !entries.is_empty() {
            return Err(
                "required signed evidence: the restored chain carries no evidence checkpoint to verify it against".to_string(),
            );
        }
        return Ok(None);
    }

    let latest_signed = checkpoint_receipts
        .last()
        .and_then(|receipt| receipt.payload.get("signature"))
        .map(|signature| !signature.is_empty())
        .unwrap_or(false);
    if required && !latest_signed {
        return Err(
            "required signed evidence: the latest evidence checkpoint is unsigned".to_string(),
        );
    }

    let mut verified = 0usize;
    let mut last_checkpoint_id: Option<String> = None;
    let mut previous_checkpoint_hash: Option<String> = None;
    for receipt in &checkpoint_receipts {
        let field = |key: &str| receipt.payload.get(key).cloned().unwrap_or_default();
        let signature_hex = field("signature");
        if signature_hex.is_empty() {
            if required {
                return Err(format!(
                    "required signed evidence: checkpoint {} in the restored chain is unsigned",
                    field("checkpoint_id")
                ));
            }
            // Optional mode: an unsigned checkpoint cannot be held against
            // anything, so skip it but keep walking the spine from here.
            previous_checkpoint_hash = Some(field("checkpoint_hash"));
            last_checkpoint_id = Some(field("checkpoint_id"));
            continue;
        }
        let checkpoint_version = field("checkpoint_version").parse().unwrap_or(0);
        let checkpoint = EvidenceCheckpoint {
            checkpoint_id: field("checkpoint_id"),
            checkpoint_version,
            range_start_receipt_id: field("range_start_receipt_id"),
            range_end_receipt_id: field("range_end_receipt_id"),
            receipt_count: field("receipt_count").parse().unwrap_or(0),
            ledger_head_hash: field("ledger_head_hash"),
            previous_checkpoint_id: Some(field("previous_checkpoint_id")).filter(|s| !s.is_empty()),
            previous_checkpoint_hash: Some(field("previous_checkpoint_hash"))
                .filter(|s| !s.is_empty()),
            merkle_root: field("merkle_root"),
            hash_algorithm: mdx_core::evidence_hash_algorithm_for_version(checkpoint_version),
            external_anchor_status: "not_anchored",
            checkpoint_hash: field("checkpoint_hash"),
        };

        // The spine: this checkpoint's recorded previous_checkpoint_hash must
        // equal the actual previous checkpoint's hash. A forged or reordered
        // checkpoint sequence breaks here.
        if checkpoint.previous_checkpoint_hash != previous_checkpoint_hash {
            return Err(format!(
                "the restored chain's checkpoint spine is broken at {}: its previous-checkpoint link does not match the prior checkpoint",
                checkpoint.checkpoint_id
            ));
        }

        verify_signature(&public_key, &checkpoint.checkpoint_hash, &signature_hex).map_err(
            |error| {
                format!(
                    "checkpoint {} signature is invalid: {error}",
                    checkpoint.checkpoint_id
                )
            },
        )?;
        if mdx_core::recompute_checkpoint_hash(&checkpoint) != checkpoint.checkpoint_hash {
            return Err(format!(
                "checkpoint {} metadata does not match its signed hash",
                checkpoint.checkpoint_id
            ));
        }
        let start = entries
            .iter()
            .position(|entry| entry.receipt_id == checkpoint.range_start_receipt_id)
            .ok_or_else(|| {
                format!(
                    "the restored chain is missing checkpoint {}'s start receipt",
                    checkpoint.checkpoint_id
                )
            })?;
        let end = entries
            .iter()
            .position(|entry| entry.receipt_id == checkpoint.range_end_receipt_id)
            .ok_or_else(|| {
                format!(
                    "the restored chain is missing checkpoint {}'s end receipt",
                    checkpoint.checkpoint_id
                )
            })?;
        if end < start {
            return Err(format!(
                "checkpoint {}'s range is inverted in the restored chain",
                checkpoint.checkpoint_id
            ));
        }
        let verdict = verify_evidence_checkpoint(&checkpoint, &entries[start..=end]);
        if verdict != mdx_core::EvidenceVerification::Verified {
            return Err(format!(
                "the restored chain does not match signed checkpoint {}: {}",
                checkpoint.checkpoint_id,
                verdict.as_str()
            ));
        }
        verified += 1;
        previous_checkpoint_hash = Some(checkpoint.checkpoint_hash.clone());
        last_checkpoint_id = Some(checkpoint.checkpoint_id);
    }

    if verified == 0 {
        // Only unsigned checkpoints existed (optional mode): nothing held.
        return Ok(None);
    }
    Ok(last_checkpoint_id.map(|id| format!("{id} (+{} checkpoints verified)", verified)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> (Ed25519KeyPair, Vec<u8>) {
        let rng = ring::rand::SystemRandom::new();
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).expect("generate");
        let pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).expect("parse");
        let public = pair.public_key().as_ref().to_vec();
        (pair, public)
    }

    #[test]
    fn a_signature_verifies_and_a_wrong_key_refuses() {
        let (pair, public) = test_key();
        let (_other, other_public) = test_key();
        let hash = "deadbeef".repeat(8);
        let signature = hex(pair.sign(hash.as_bytes()).as_ref());
        assert!(verify_signature(&public, &hash, &signature).is_ok());
        assert!(verify_signature(&other_public, &hash, &signature).is_err());
        assert!(verify_signature(&public, &"00".repeat(32), &signature).is_err());
    }

    #[test]
    fn a_rewritten_chain_cannot_satisfy_its_signed_checkpoint() {
        use mdx_core::{
            ActorId, CorrelationIds, IdFactory, Ledger, LoopId, TenantId, TraceId, WorkflowId,
            build_evidence_checkpoint,
        };
        use std::collections::BTreeMap;

        let build_chain = |note: &str| {
            let mut ledger = Ledger::default();
            let mut ids = IdFactory::default();
            let correlation = CorrelationIds {
                tenant_id: TenantId::new("tenant_local"),
                trace_id: TraceId::new("trace_teeth"),
                actor_id: ActorId::new("human:teeth"),
                loop_id: LoopId::new("teeth_loop"),
                workflow_id: WorkflowId::new("wf_teeth"),
            };
            for index in 0..3 {
                let mut payload = BTreeMap::new();
                payload.insert("note".to_string(), format!("{note}-{index}"));
                ledger.append(&mut ids, &correlation, "teeth.recorded", None, payload);
            }
            ledger
        };

        let honest = build_chain("honest");
        let checkpoint =
            build_evidence_checkpoint("cp_teeth".into(), honest.entries(), None).expect("cut");
        let (pair, public) = test_key();
        let signature = hex(pair.sign(checkpoint.checkpoint_hash.as_bytes()).as_ref());

        // The attacker rewrites history. The internal FNV chain of the
        // rewritten ledger is perfectly valid - verify_full passes - but
        // the signed checkpoint's Merkle root no longer matches, and the
        // attacker cannot re-sign a new checkpoint without the key.
        let rewritten = build_chain("rewritten");
        assert!(rewritten.verify_full().is_ok(), "the rewrite self-verifies");
        assert!(
            verify_signature(&public, &checkpoint.checkpoint_hash, &signature).is_ok(),
            "the old signature still verifies - it just no longer describes this chain"
        );
        assert_ne!(
            mdx_core::evidence_merkle_root(rewritten.entries()),
            checkpoint.merkle_root,
            "the rewritten chain cannot reproduce the signed root"
        );
    }
}
