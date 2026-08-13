//! Evidence checkpoints: deterministic, independently verifiable summaries
//! of bounded receipt ranges.
//!
//! The receipt chain stays canonical and local. A checkpoint is a portable
//! claim about it: "these receipts, in this order, with these hashes,
//! existed when this checkpoint was cut" - linked to the previous
//! checkpoint so the sequence of claims is itself a chain.
//!
//! Hash posture, stated plainly: the ledger's internal linkage uses
//! FNV-1a 64 - fast, fine for consistency, and not cryptographic. A
//! checkpoint therefore computes its own SHA-256 leaf hash over each
//! receipt's canonical content and a Merkle root over those leaves.
//! Tamper evidence comes from this layer, not from the chain's internal
//! hash; rewriting any covered receipt changes the root.
//!
//! Privacy is enforced in the types: the canonical leaf covers receipt
//! payload values (so tampering with them is detectable) but the
//! checkpoint itself carries only ids, hashes, counts, and linkage -
//! never payload text. There is no field a private value could ride in.

use crate::{Ledger, Receipt};

pub const EVIDENCE_CHECKPOINT_VERSION: u64 = 3;
// v2: length-prefixed canonical fields (v1 used a unit-separator delimiter,
// which a field value could in principle contain). Length-prefixing removes
// the ambiguity class entirely - the byte stream is unforgeable regardless
// of what a payload value holds. The version is in the algorithm string so a
// verifier always knows the exact serialization it must reproduce.
pub const EVIDENCE_HASH_ALGORITHM: &str = "sha256-merkle-v3";
pub const EVIDENCE_CHECKPOINT_RECEIPT_KIND: &str = "audit.evidence.checkpoint.recorded";
pub const EVIDENCE_ANCHOR_RECEIPT_KIND: &str = "audit.evidence.anchor.recorded";

pub fn evidence_hash_algorithm_for_version(version: u64) -> &'static str {
    if version >= 3 {
        "sha256-merkle-v3"
    } else {
        "sha256-merkle-v2"
    }
}

/// A cut checkpoint. Every field is safe metadata by construction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceCheckpoint {
    pub checkpoint_id: String,
    pub checkpoint_version: u64,
    pub range_start_receipt_id: String,
    pub range_end_receipt_id: String,
    pub receipt_count: usize,
    pub ledger_head_hash: String,
    pub previous_checkpoint_id: Option<String>,
    pub previous_checkpoint_hash: Option<String>,
    pub merkle_root: String,
    pub hash_algorithm: &'static str,
    pub external_anchor_status: &'static str,
    pub checkpoint_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvidenceCheckpointError {
    EmptyRange,
    ChainBroken(String),
    PreviousLinkageBroken(String),
}

impl EvidenceCheckpointError {
    pub fn message(&self) -> String {
        match self {
            Self::EmptyRange => {
                "nothing new to checkpoint - every receipt is already covered".to_string()
            }
            Self::ChainBroken(receipt_id) => format!(
                "the receipt chain does not link at {receipt_id}; a checkpoint is refused over a broken chain"
            ),
            Self::PreviousLinkageBroken(detail) => {
                format!("the previous checkpoint does not line up: {detail}")
            }
        }
    }
}

/// The canonical leaf serialization: every field that makes a receipt what
/// it is, in fixed order. Payload values are covered (tampering with them
/// must change the leaf) but never copied out of this function.
/// Length-prefixed field append: an 8-byte big-endian byte length, then the
/// bytes. No delimiter, so no value can forge a field boundary whatever
/// bytes it holds - the audit-grade canonicalization rule.
fn push_field(buf: &mut Vec<u8>, part: &str) {
    buf.extend_from_slice(&(part.len() as u64).to_be_bytes());
    buf.extend_from_slice(part.as_bytes());
}

fn canonical_leaf_for_version(receipt: &Receipt, checkpoint_version: u64) -> [u8; 32] {
    let mut input: Vec<u8> = Vec::new();
    if checkpoint_version >= 3 {
        push_field(&mut input, &receipt.hash_version.to_string());
        push_field(&mut input, &receipt.receipt_timestamp);
    }
    push_field(&mut input, &receipt.receipt_id);
    push_field(&mut input, receipt.tenant_id.as_str());
    push_field(&mut input, receipt.trace_id.as_str());
    push_field(&mut input, receipt.actor_id.as_str());
    push_field(&mut input, receipt.loop_id.as_str());
    push_field(&mut input, receipt.workflow_id.as_str());
    push_field(&mut input, &receipt.kind);
    push_field(
        &mut input,
        receipt.policy_decision_id.as_deref().unwrap_or(""),
    );
    push_field(&mut input, receipt.previous_hash.as_deref().unwrap_or(""));
    push_field(&mut input, &receipt.hash);
    // The payload field count is itself prefixed, so two receipts cannot
    // collide by one having a key/value pair the other folds into a field.
    input.extend_from_slice(&(receipt.payload.len() as u64).to_be_bytes());
    for (key, value) in &receipt.payload {
        push_field(&mut input, key);
        push_field(&mut input, value);
    }
    sha256(&input)
}

/// Merkle root over the canonical leaves, duplicating the last node at odd
/// levels (the bitcoin-style convention, stated so verifiers can match it).
pub fn evidence_merkle_root(entries: &[Receipt]) -> String {
    evidence_merkle_root_for_version(entries, EVIDENCE_CHECKPOINT_VERSION)
}

pub fn evidence_merkle_root_for_version(entries: &[Receipt], checkpoint_version: u64) -> String {
    let mut level: Vec<[u8; 32]> = entries
        .iter()
        .map(|receipt| canonical_leaf_for_version(receipt, checkpoint_version))
        .collect();
    if level.is_empty() {
        return hex(&sha256(b""));
    }
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        for pair in level.chunks(2) {
            let left = pair[0];
            let right = if pair.len() == 2 { pair[1] } else { pair[0] };
            let mut joined = [0u8; 64];
            joined[..32].copy_from_slice(&left);
            joined[32..].copy_from_slice(&right);
            next.push(sha256(&joined));
        }
        level = next;
    }
    hex(&level[0])
}

/// The per-receipt leaf hashes, for export packets: an external verifier
/// can rebuild the root from these without ever seeing a payload value.
pub fn evidence_leaf_hashes(entries: &[Receipt]) -> Vec<String> {
    evidence_leaf_hashes_for_version(entries, EVIDENCE_CHECKPOINT_VERSION)
}

pub fn evidence_leaf_hashes_for_version(
    entries: &[Receipt],
    checkpoint_version: u64,
) -> Vec<String> {
    entries
        .iter()
        .map(|receipt| hex(&canonical_leaf_for_version(receipt, checkpoint_version)))
        .collect()
}

/// Rebuild a Merkle root from leaf hashes alone (the packet-math side of
/// verification; the live side recomputes the leaves from the chain).
pub fn evidence_root_from_leaf_hashes(leaves: &[String]) -> Result<String, String> {
    let mut level: Vec<[u8; 32]> = Vec::with_capacity(leaves.len());
    for leaf in leaves {
        let mut bytes = [0u8; 32];
        if leaf.len() != 64 {
            return Err(format!("leaf hash {leaf} is not 32 bytes of hex"));
        }
        for i in 0..32 {
            bytes[i] = u8::from_str_radix(&leaf[i * 2..i * 2 + 2], 16)
                .map_err(|error| format!("leaf hash {leaf}: {error}"))?;
        }
        level.push(bytes);
    }
    if level.is_empty() {
        return Ok(hex(&sha256(b"")));
    }
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        for pair in level.chunks(2) {
            let left = pair[0];
            let right = if pair.len() == 2 { pair[1] } else { pair[0] };
            let mut joined = [0u8; 64];
            joined[..32].copy_from_slice(&left);
            joined[32..].copy_from_slice(&right);
            next.push(sha256(&joined));
        }
        level = next;
    }
    Ok(hex(&level[0]))
}

/// Recompute a checkpoint's own hash (public so verifiers can re-derive it
/// from packet fields instead of trusting the recorded value).
pub fn recompute_checkpoint_hash(checkpoint: &EvidenceCheckpoint) -> String {
    checkpoint_hash_of(checkpoint)
}

fn checkpoint_hash_of(checkpoint: &EvidenceCheckpoint) -> String {
    let mut input: Vec<u8> = Vec::new();
    push_field(&mut input, &checkpoint.checkpoint_id);
    push_field(&mut input, &checkpoint.checkpoint_version.to_string());
    push_field(&mut input, &checkpoint.range_start_receipt_id);
    push_field(&mut input, &checkpoint.range_end_receipt_id);
    push_field(&mut input, &checkpoint.receipt_count.to_string());
    push_field(&mut input, &checkpoint.ledger_head_hash);
    push_field(
        &mut input,
        checkpoint.previous_checkpoint_id.as_deref().unwrap_or(""),
    );
    push_field(
        &mut input,
        checkpoint.previous_checkpoint_hash.as_deref().unwrap_or(""),
    );
    push_field(&mut input, &checkpoint.merkle_root);
    push_field(&mut input, checkpoint.hash_algorithm);
    hex(&sha256(&input))
}

/// The previous checkpoint's linkage facts, reconstructed from its receipt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreviousCheckpoint {
    pub checkpoint_id: String,
    pub checkpoint_hash: String,
    pub range_end_receipt_id: String,
}

/// Cut a checkpoint over every receipt after the previous checkpoint's
/// range (or the whole chain when there is none). The caller verifies the
/// ledger first; this function re-checks the linkage of the slice it
/// covers so a torn range can never be summarized.
pub fn build_evidence_checkpoint(
    checkpoint_id: String,
    entries: &[Receipt],
    previous: Option<&PreviousCheckpoint>,
) -> Result<EvidenceCheckpoint, EvidenceCheckpointError> {
    let start_index = match previous {
        None => 0,
        Some(prior) => {
            let end_index = entries
                .iter()
                .position(|receipt| receipt.receipt_id == prior.range_end_receipt_id)
                .ok_or_else(|| {
                    EvidenceCheckpointError::PreviousLinkageBroken(format!(
                        "its end receipt {} is not in the chain",
                        prior.range_end_receipt_id
                    ))
                })?;
            end_index + 1
        }
    };
    let range = &entries[start_index..];
    if range.is_empty() {
        return Err(EvidenceCheckpointError::EmptyRange);
    }
    // The slice must link internally and to the receipt before it.
    let mut expected_previous = if start_index == 0 {
        None
    } else {
        Some(entries[start_index - 1].hash.clone())
    };
    for receipt in range {
        if receipt.previous_hash != expected_previous {
            return Err(EvidenceCheckpointError::ChainBroken(
                receipt.receipt_id.clone(),
            ));
        }
        expected_previous = Some(receipt.hash.clone());
    }

    let mut checkpoint = EvidenceCheckpoint {
        checkpoint_id,
        checkpoint_version: EVIDENCE_CHECKPOINT_VERSION,
        range_start_receipt_id: range[0].receipt_id.clone(),
        range_end_receipt_id: range[range.len() - 1].receipt_id.clone(),
        receipt_count: range.len(),
        ledger_head_hash: range[range.len() - 1].hash.clone(),
        previous_checkpoint_id: previous.map(|prior| prior.checkpoint_id.clone()),
        previous_checkpoint_hash: previous.map(|prior| prior.checkpoint_hash.clone()),
        merkle_root: evidence_merkle_root_for_version(range, EVIDENCE_CHECKPOINT_VERSION),
        hash_algorithm: evidence_hash_algorithm_for_version(EVIDENCE_CHECKPOINT_VERSION),
        external_anchor_status: "not_anchored",
        checkpoint_hash: String::new(),
    };
    checkpoint.checkpoint_hash = checkpoint_hash_of(&checkpoint);
    Ok(checkpoint)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvidenceVerification {
    Verified,
    CheckpointHashMismatch,
    MerkleRootMismatch,
    ReceiptChainBroken,
    ReceiptRangeIncomplete,
}

impl EvidenceVerification {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Verified => "VERIFIED",
            Self::CheckpointHashMismatch => "CHECKPOINT_HASH_MISMATCH",
            Self::MerkleRootMismatch => "MERKLE_ROOT_MISMATCH",
            Self::ReceiptChainBroken => "RECEIPT_CHAIN_BROKEN",
            Self::ReceiptRangeIncomplete => "RECEIPT_RANGE_INCOMPLETE",
        }
    }
}

/// Independently verify a checkpoint against the receipts it claims to
/// cover. This is the auditor's function: it trusts nothing but the math.
pub fn verify_evidence_checkpoint(
    checkpoint: &EvidenceCheckpoint,
    range: &[Receipt],
) -> EvidenceVerification {
    if checkpoint_hash_of(checkpoint) != checkpoint.checkpoint_hash {
        return EvidenceVerification::CheckpointHashMismatch;
    }
    if range.len() != checkpoint.receipt_count
        || range.first().map(|receipt| receipt.receipt_id.as_str())
            != Some(checkpoint.range_start_receipt_id.as_str())
        || range.last().map(|receipt| receipt.receipt_id.as_str())
            != Some(checkpoint.range_end_receipt_id.as_str())
    {
        return EvidenceVerification::ReceiptRangeIncomplete;
    }
    let mut expected_previous = range[0].previous_hash.clone();
    for receipt in range {
        if receipt.previous_hash != expected_previous {
            return EvidenceVerification::ReceiptChainBroken;
        }
        expected_previous = Some(receipt.hash.clone());
    }
    if range.last().map(|receipt| receipt.hash.as_str())
        != Some(checkpoint.ledger_head_hash.as_str())
    {
        return EvidenceVerification::ReceiptChainBroken;
    }
    if evidence_merkle_root_for_version(range, checkpoint.checkpoint_version)
        != checkpoint.merkle_root
    {
        return EvidenceVerification::MerkleRootMismatch;
    }
    EvidenceVerification::Verified
}

/// Ledger-aware convenience: verify the whole chain first, then cut.
pub fn checkpoint_over_ledger(
    checkpoint_id: String,
    ledger: &Ledger,
    previous: Option<&PreviousCheckpoint>,
) -> Result<EvidenceCheckpoint, String> {
    ledger
        .verify_full()
        .map_err(|error| format!("the ledger does not verify: {error}"))?;
    build_evidence_checkpoint(checkpoint_id, ledger.entries(), previous)
        .map_err(|error| error.message())
}

pub fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// SHA-256, hand-rolled and vector-tested, because mdx-core carries no
/// dependencies and the audit layer needs a cryptographic hash (the
/// ledger's internal FNV linkage is a consistency check, not one).
pub fn sha256(message: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut data = message.to_vec();
    let bit_len = (message.len() as u64).wrapping_mul(8);
    data.push(0x80);
    while data.len() % 64 != 56 {
        data.push(0);
    }
    data.extend_from_slice(&bit_len.to_be_bytes());
    for block in data.chunks(64) {
        let mut w = [0u32; 64];
        for (i, word) in block.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

/// What the route reports back: safe metadata only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceCheckpointReport {
    pub checkpoint: EvidenceCheckpoint,
    pub checkpoint_receipt_id: String,
    pub policy_decision_id: String,
    pub signer_key_id: String,
    pub signature: String,
}

impl<S: crate::StorageProvider> crate::MdxKernel<S> {
    /// Cut a checkpoint over everything since the last one and record the
    /// act as a receipt. The checkpoint receipt itself lands after the cut,
    /// so it is covered by the next checkpoint - the claims chain forward.
    /// Opens no authority.
    pub fn save_evidence_checkpoint_local_with_identity(
        &mut self,
        tenant_id: &str,
        actor_id: &str,
        identity: &crate::GovernedWriteIdentity,
        sign: impl Fn(&str) -> Result<Option<(String, String)>, String>,
    ) -> Result<EvidenceCheckpointReport, String> {
        let previous = self
            .ledger()
            .query()
            .by_kind(EVIDENCE_CHECKPOINT_RECEIPT_KIND)
            .last()
            .map(|receipt| {
                let field = |key: &str| receipt.payload.get(key).cloned().unwrap_or_default();
                PreviousCheckpoint {
                    checkpoint_id: field("checkpoint_id"),
                    checkpoint_hash: field("checkpoint_hash"),
                    range_end_receipt_id: field("range_end_receipt_id"),
                }
            });
        let checkpoint_id = format!("evidence_{}", self.ids.next("checkpoint"));
        let checkpoint = checkpoint_over_ledger(checkpoint_id, self.ledger(), previous.as_ref())?;
        let (signer_key_id, signature) = match sign(&checkpoint.checkpoint_hash)? {
            Some((key_id, signature)) => (key_id, signature),
            None => (String::new(), String::new()),
        };

        let correlation = crate::CorrelationIds {
            tenant_id: crate::TenantId::new(tenant_id),
            trace_id: crate::TraceId::new(self.ids.next("trace")),
            actor_id: crate::ActorId::new(actor_id),
            loop_id: crate::LoopId::new("evidence_checkpoint"),
            workflow_id: crate::WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("run");
        self.storage.push_loop_run(crate::LoopRun {
            run_id: run_id.clone(),
            loop_id: correlation.loop_id.clone(),
            agent_id: correlation.actor_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            status: "RUNNING".to_string(),
        });
        let decision =
            self.decide_with_receipt(&correlation, crate::ActionKind::RecordEvidenceCheckpoint);
        let receipt = self.transition_receipt(
            &run_id,
            "RECORD_EVIDENCE_CHECKPOINT",
            &correlation,
            &decision,
            EVIDENCE_CHECKPOINT_RECEIPT_KIND,
            crate::payload(&[
                ("checkpoint_id", &checkpoint.checkpoint_id),
                (
                    "checkpoint_version",
                    &checkpoint.checkpoint_version.to_string(),
                ),
                ("range_start_receipt_id", &checkpoint.range_start_receipt_id),
                ("range_end_receipt_id", &checkpoint.range_end_receipt_id),
                ("receipt_count", &checkpoint.receipt_count.to_string()),
                ("ledger_head_hash", &checkpoint.ledger_head_hash),
                (
                    "previous_checkpoint_id",
                    checkpoint.previous_checkpoint_id.as_deref().unwrap_or(""),
                ),
                (
                    "previous_checkpoint_hash",
                    checkpoint.previous_checkpoint_hash.as_deref().unwrap_or(""),
                ),
                ("merkle_root", &checkpoint.merkle_root),
                ("hash_algorithm", checkpoint.hash_algorithm),
                ("checkpoint_hash", &checkpoint.checkpoint_hash),
                ("external_anchor_status", checkpoint.external_anchor_status),
                (
                    "signature_algorithm",
                    if signature.is_empty() { "" } else { "ed25519" },
                ),
                ("signer_key_id", &signer_key_id),
                ("signature", &signature),
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("authority_opened", "none"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(EvidenceCheckpointReport {
            checkpoint,
            checkpoint_receipt_id: receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            signer_key_id,
            signature,
        })
    }

    /// Record that a checkpoint was anchored to an external store. The
    /// anchor itself is performed by the caller's adapter; this records the
    /// safe result (the adapter, the status, the reference, and the
    /// checkpoint hash it covers) bound to the checkpoint receipt. No
    /// authority, no private data, no anchor payload.
    #[allow(clippy::too_many_arguments)]
    pub fn save_evidence_anchor_local_with_identity(
        &mut self,
        tenant_id: &str,
        actor_id: &str,
        identity: &crate::GovernedWriteIdentity,
        checkpoint_id: &str,
        checkpoint_hash: &str,
        merkle_root: &str,
        anchor_adapter: &str,
        anchor_status: &str,
        anchor_ref: &str,
    ) -> Result<String, String> {
        let correlation = crate::CorrelationIds {
            tenant_id: crate::TenantId::new(tenant_id),
            trace_id: crate::TraceId::new(self.ids.next("trace")),
            actor_id: crate::ActorId::new(actor_id),
            loop_id: crate::LoopId::new("evidence_anchor"),
            workflow_id: crate::WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("run");
        self.storage.push_loop_run(crate::LoopRun {
            run_id: run_id.clone(),
            loop_id: correlation.loop_id.clone(),
            agent_id: correlation.actor_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            status: "RUNNING".to_string(),
        });
        let decision =
            self.decide_with_receipt(&correlation, crate::ActionKind::RecordEvidenceAnchor);
        let receipt = self.transition_receipt(
            &run_id,
            "RECORD_EVIDENCE_ANCHOR",
            &correlation,
            &decision,
            EVIDENCE_ANCHOR_RECEIPT_KIND,
            crate::payload(&[
                ("checkpoint_id", checkpoint_id),
                ("checkpoint_hash", checkpoint_hash),
                ("merkle_root", merkle_root),
                ("anchor_adapter", anchor_adapter),
                ("anchor_status", anchor_status),
                ("anchor_ref", anchor_ref),
                ("identity_source", &identity.identity_source),
                ("authority_opened", "none"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(receipt.receipt_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActorId, CorrelationIds, IdFactory, LoopId, TenantId, TraceId, WorkflowId};
    use std::collections::BTreeMap;

    #[test]
    fn sha256_matches_known_vectors() {
        assert_eq!(
            hex(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex(&sha256(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            )),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    fn chain(count: usize) -> Ledger {
        let mut ledger = Ledger::default();
        let mut ids = IdFactory::default();
        let correlation = CorrelationIds {
            tenant_id: TenantId::new("tenant_local"),
            trace_id: TraceId::new("trace_checkpoint_test"),
            actor_id: ActorId::new("human:checkpoint_test"),
            loop_id: LoopId::new("checkpoint_test_loop"),
            workflow_id: WorkflowId::new("wf_checkpoint_test"),
        };
        for index in 0..count {
            let mut payload = BTreeMap::new();
            payload.insert("index".to_string(), index.to_string());
            payload.insert("private_note".to_string(), format!("secret-{index}"));
            ledger.append(
                &mut ids,
                &correlation,
                "checkpoint.test.recorded",
                None,
                payload,
            );
        }
        ledger
    }

    #[test]
    fn checkpoint_is_deterministic_and_verifies() {
        let ledger = chain(5);
        let first = checkpoint_over_ledger("cp_1".into(), &ledger, None).expect("cut");
        let again = checkpoint_over_ledger("cp_1".into(), &ledger, None).expect("cut again");
        assert_eq!(first, again, "same chain, same checkpoint, bit for bit");
        assert_eq!(first.checkpoint_version, 3);
        assert_eq!(first.hash_algorithm, "sha256-merkle-v3");
        assert_eq!(first.receipt_count, 5);
        assert_eq!(
            verify_evidence_checkpoint(&first, ledger.entries()),
            EvidenceVerification::Verified
        );
    }

    #[test]
    fn v2_checkpoints_still_verify_with_legacy_leaf_math() {
        let ledger = chain(3);
        let mut checkpoint = EvidenceCheckpoint {
            checkpoint_id: "cp_v2".to_string(),
            checkpoint_version: 2,
            range_start_receipt_id: ledger.entries()[0].receipt_id.clone(),
            range_end_receipt_id: ledger.entries()[2].receipt_id.clone(),
            receipt_count: 3,
            ledger_head_hash: ledger.entries()[2].hash.clone(),
            previous_checkpoint_id: None,
            previous_checkpoint_hash: None,
            merkle_root: evidence_merkle_root_for_version(ledger.entries(), 2),
            hash_algorithm: evidence_hash_algorithm_for_version(2),
            external_anchor_status: "not_anchored",
            checkpoint_hash: String::new(),
        };
        checkpoint.checkpoint_hash = checkpoint_hash_of(&checkpoint);

        assert_eq!(
            verify_evidence_checkpoint(&checkpoint, ledger.entries()),
            EvidenceVerification::Verified
        );
    }

    #[test]
    fn v3_checkpoints_cover_receipt_timestamp_and_hash_version() {
        let ledger = chain(3);
        let checkpoint = checkpoint_over_ledger("cp_1".into(), &ledger, None).expect("cut");
        let mut tampered = ledger.entries().to_vec();
        tampered[1].receipt_timestamp = crate::trusted_receipt_timestamp(99);
        assert_eq!(
            verify_evidence_checkpoint(&checkpoint, &tampered),
            EvidenceVerification::MerkleRootMismatch
        );
    }

    #[test]
    fn no_payload_value_rides_in_the_checkpoint() {
        let ledger = chain(3);
        let checkpoint = checkpoint_over_ledger("cp_1".into(), &ledger, None).expect("cut");
        let rendered = format!("{checkpoint:?}");
        assert!(
            !rendered.contains("secret-"),
            "a payload value crossed into the checkpoint"
        );
    }

    #[test]
    fn tampering_with_a_covered_payload_changes_the_root() {
        let ledger = chain(4);
        let checkpoint = checkpoint_over_ledger("cp_1".into(), &ledger, None).expect("cut");
        let mut tampered = ledger.entries().to_vec();
        tampered[1]
            .payload
            .insert("private_note".to_string(), "rewritten history".to_string());
        // The internal fnv hash still matches nothing here - the auditor's
        // Merkle root alone must catch the rewrite.
        assert_eq!(
            verify_evidence_checkpoint(&checkpoint, &tampered),
            EvidenceVerification::MerkleRootMismatch
        );
    }

    #[test]
    fn a_missing_receipt_fails_the_range() {
        let ledger = chain(4);
        let checkpoint = checkpoint_over_ledger("cp_1".into(), &ledger, None).expect("cut");
        let mut shortened = ledger.entries().to_vec();
        shortened.remove(2);
        assert_eq!(
            verify_evidence_checkpoint(&checkpoint, &shortened),
            EvidenceVerification::ReceiptRangeIncomplete
        );
    }

    #[test]
    fn a_tampered_checkpoint_hash_is_caught_first() {
        let ledger = chain(2);
        let mut checkpoint = checkpoint_over_ledger("cp_1".into(), &ledger, None).expect("cut");
        checkpoint.receipt_count = 99;
        assert_eq!(
            verify_evidence_checkpoint(&checkpoint, ledger.entries()),
            EvidenceVerification::CheckpointHashMismatch
        );
    }

    #[test]
    fn checkpoints_link_and_cover_only_what_is_new() {
        let mut ledger = chain(3);
        let first = checkpoint_over_ledger("cp_1".into(), &ledger, None).expect("cut");
        let mut ids = IdFactory::default();
        ids.restore_counter(10);
        let correlation = CorrelationIds {
            tenant_id: TenantId::new("tenant_local"),
            trace_id: TraceId::new("trace_checkpoint_test"),
            actor_id: ActorId::new("human:checkpoint_test"),
            loop_id: LoopId::new("checkpoint_test_loop"),
            workflow_id: WorkflowId::new("wf_checkpoint_test"),
        };
        ledger.append(
            &mut ids,
            &correlation,
            "checkpoint.test.recorded",
            None,
            BTreeMap::new(),
        );
        let prior = PreviousCheckpoint {
            checkpoint_id: first.checkpoint_id.clone(),
            checkpoint_hash: first.checkpoint_hash.clone(),
            range_end_receipt_id: first.range_end_receipt_id.clone(),
        };
        let second =
            checkpoint_over_ledger("cp_2".into(), &ledger, Some(&prior)).expect("cut second");
        assert_eq!(second.receipt_count, 1);
        assert_eq!(second.previous_checkpoint_id.as_deref(), Some("cp_1"));
        assert_eq!(
            second.previous_checkpoint_hash.as_deref(),
            Some(first.checkpoint_hash.as_str())
        );
        // And cutting again with nothing new refuses.
        let prior_two = PreviousCheckpoint {
            checkpoint_id: second.checkpoint_id.clone(),
            checkpoint_hash: second.checkpoint_hash.clone(),
            range_end_receipt_id: second.range_end_receipt_id.clone(),
        };
        assert!(checkpoint_over_ledger("cp_3".into(), &ledger, Some(&prior_two)).is_err());
    }

    #[test]
    fn a_broken_slice_refuses_to_be_summarized() {
        let ledger = chain(4);
        let mut torn = ledger.entries().to_vec();
        torn[2].previous_hash = Some("forged".to_string());
        let outcome = build_evidence_checkpoint("cp_x".into(), &torn, None);
        assert!(matches!(
            outcome,
            Err(EvidenceCheckpointError::ChainBroken(_))
        ));
    }

    #[test]
    fn a_previous_end_receipt_not_in_the_chain_refuses() {
        let ledger = chain(3);
        let prior = PreviousCheckpoint {
            checkpoint_id: "cp_0".to_string(),
            checkpoint_hash: "deadbeef".to_string(),
            range_end_receipt_id: "receipt_never_appended".to_string(),
        };
        let outcome = build_evidence_checkpoint("cp_1".into(), ledger.entries(), Some(&prior));
        assert!(matches!(
            outcome,
            Err(EvidenceCheckpointError::PreviousLinkageBroken(_))
        ));
    }

    #[test]
    fn a_rewritten_chain_link_fails_verification() {
        let ledger = chain(4);
        let checkpoint = checkpoint_over_ledger("cp_1".into(), &ledger, None).expect("cut");
        let mut torn = ledger.entries().to_vec();
        // Break an internal link without dropping a receipt, so the range
        // is complete but no longer chains: the chain check must catch it.
        torn[2].previous_hash = Some("forged".to_string());
        assert_eq!(
            verify_evidence_checkpoint(&checkpoint, &torn),
            EvidenceVerification::ReceiptChainBroken
        );
    }

    #[test]
    fn a_rewritten_head_hash_fails_verification() {
        let ledger = chain(3);
        let checkpoint = checkpoint_over_ledger("cp_1".into(), &ledger, None).expect("cut");
        let mut tampered = ledger.entries().to_vec();
        // The slice still links internally, but its final hash no longer
        // matches the checkpoint's recorded head: still a broken chain.
        let last = tampered.len() - 1;
        tampered[last].hash = format!("{}-rewritten", tampered[last].hash);
        assert_eq!(
            verify_evidence_checkpoint(&checkpoint, &tampered),
            EvidenceVerification::ReceiptChainBroken
        );
    }

    #[test]
    fn leaf_hashes_rebuild_the_recorded_root() {
        let ledger = chain(5);
        let checkpoint = checkpoint_over_ledger("cp_1".into(), &ledger, None).expect("cut");
        let leaves = evidence_leaf_hashes(ledger.entries());
        assert_eq!(leaves.len(), 5);
        let rebuilt = evidence_root_from_leaf_hashes(&leaves).expect("rebuild");
        assert_eq!(rebuilt, checkpoint.merkle_root);
    }

    #[test]
    fn an_empty_leaf_set_rebuilds_the_empty_root() {
        assert_eq!(
            evidence_root_from_leaf_hashes(&[]).expect("rebuild"),
            hex(&sha256(b""))
        );
    }

    #[test]
    fn a_short_leaf_hash_is_refused() {
        let outcome = evidence_root_from_leaf_hashes(&["abc".to_string()]);
        assert!(matches!(outcome, Err(message) if message.contains("not 32 bytes of hex")));
    }

    #[test]
    fn a_non_hex_leaf_hash_is_refused() {
        // 64 characters long but not valid hex: the parse arm refuses it.
        let leaf = "z".repeat(64);
        let outcome = evidence_root_from_leaf_hashes(std::slice::from_ref(&leaf));
        assert!(matches!(outcome, Err(message) if message.contains(&leaf)));
    }

    #[test]
    fn recompute_checkpoint_hash_matches_the_recorded_hash() {
        let ledger = chain(3);
        let checkpoint = checkpoint_over_ledger("cp_1".into(), &ledger, None).expect("cut");
        assert_eq!(
            recompute_checkpoint_hash(&checkpoint),
            checkpoint.checkpoint_hash
        );
    }

    #[test]
    fn error_messages_describe_each_refusal() {
        assert!(
            EvidenceCheckpointError::EmptyRange
                .message()
                .contains("already covered")
        );
        assert!(
            EvidenceCheckpointError::ChainBroken("receipt_7".to_string())
                .message()
                .contains("receipt_7")
        );
        assert!(
            EvidenceCheckpointError::PreviousLinkageBroken("end missing".to_string())
                .message()
                .contains("end missing")
        );
    }

    #[test]
    fn verification_codes_render_as_stable_strings() {
        assert_eq!(EvidenceVerification::Verified.as_str(), "VERIFIED");
        assert_eq!(
            EvidenceVerification::CheckpointHashMismatch.as_str(),
            "CHECKPOINT_HASH_MISMATCH"
        );
        assert_eq!(
            EvidenceVerification::MerkleRootMismatch.as_str(),
            "MERKLE_ROOT_MISMATCH"
        );
        assert_eq!(
            EvidenceVerification::ReceiptChainBroken.as_str(),
            "RECEIPT_CHAIN_BROKEN"
        );
        assert_eq!(
            EvidenceVerification::ReceiptRangeIncomplete.as_str(),
            "RECEIPT_RANGE_INCOMPLETE"
        );
    }

    #[test]
    fn saving_a_checkpoint_records_a_receipt_and_verifies() {
        let mut kernel = crate::MdxKernel::boot_local();
        // Seed the ledger with real receipts so there is a range to cut.
        kernel.run_evals_runner_agent().expect("loop run");
        let identity = crate::GovernedWriteIdentity::local_demo("human:auditor");
        let report = kernel
            .save_evidence_checkpoint_local_with_identity(
                "tenant_local",
                "human:auditor",
                &identity,
                |hash| Ok(Some(("key_local".to_string(), format!("sig-{hash}")))),
            )
            .expect("save checkpoint");
        assert!(!report.checkpoint_receipt_id.is_empty());
        assert_eq!(report.signer_key_id, "key_local");
        assert_eq!(
            report.signature,
            format!("sig-{}", report.checkpoint.checkpoint_hash)
        );
        // The checkpoint covers the receipts that existed before its own
        // recording receipt landed; verify over exactly that prefix.
        let entries = kernel.ledger().entries().to_vec();
        let end = entries
            .iter()
            .position(|receipt| receipt.receipt_id == report.checkpoint.range_end_receipt_id)
            .expect("range end present");
        assert_eq!(
            verify_evidence_checkpoint(&report.checkpoint, &entries[..=end]),
            EvidenceVerification::Verified
        );
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn recording_an_anchor_lands_a_receipt() {
        let mut kernel = crate::MdxKernel::boot_local();
        // Seed the ledger with real receipts so there is a range to cut.
        kernel.run_evals_runner_agent().expect("loop run");
        let identity = crate::GovernedWriteIdentity::local_demo("human:auditor");
        let report = kernel
            .save_evidence_checkpoint_local_with_identity(
                "tenant_local",
                "human:auditor",
                &identity,
                |_hash| Ok(None),
            )
            .expect("save checkpoint");
        let receipt_id = kernel
            .save_evidence_anchor_local_with_identity(
                "tenant_local",
                "human:auditor",
                &identity,
                &report.checkpoint.checkpoint_id,
                &report.checkpoint.checkpoint_hash,
                &report.checkpoint.merkle_root,
                "local_file",
                "anchored",
                "ref_local_1",
            )
            .expect("save anchor");
        assert!(!receipt_id.is_empty());
        assert!(kernel.ledger().verify().is_ok());
    }
}
