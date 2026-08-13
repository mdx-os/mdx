use super::harness_execute::{low_risk_envelope, run_to_review_packet};
use super::*;

// The envelope registry lifecycle, tested against the registry itself.
// The autonomous-completion gate that once consumed envelopes belonged to
// the deleted DXR/Talent generation; the LIVE consumers are Forge run
// admission and the per-turn revocation poll, both of which go through
// autonomy_envelope_authority_blocker.

#[test]
fn recording_an_envelope_makes_it_active_and_revoking_it_blocks_authority() {
    let (mut kernel, manifest, _packet_id) = run_to_review_packet();
    let envelope = low_risk_envelope();
    let envelope_id = envelope.envelope_id.to_string();
    let receipt_id = LocalAutonomyEnvelopeRegistry
        .record(
            &mut kernel,
            &manifest,
            &AutonomyEnvelopeRecordRequest {
                envelope,
                recorded_by: "human_platform_lead",
            },
        )
        .expect("record envelope");

    // Unknown envelope: blocked as not found.
    assert_eq!(
        kernel.autonomy_envelope_authority_blocker("env_never_recorded"),
        Some("envelope_not_found".to_string())
    );
    // Recorded and unrevoked: active.
    assert_eq!(
        kernel.autonomy_envelope_authority_blocker(&envelope_id),
        None
    );

    // An unauthorized revoker is refused before any state changes.
    let error = LocalAutonomyEnvelopeRegistry
        .revoke(
            &mut kernel,
            &manifest,
            &AutonomyEnvelopeRevokeRequest {
                envelope_receipt_id: &receipt_id,
                revoked_by: "human_other",
                revocation_reason: "not allowed",
            },
        )
        .expect_err("unauthorized revoke should fail");
    assert_eq!(error, "revoker_not_authorized");
    assert_eq!(
        kernel.autonomy_envelope_authority_blocker(&envelope_id),
        None,
        "a refused revoke must not change authority"
    );

    // The owner revokes; the same predicate the run loop polls flips.
    let revoke_id = LocalAutonomyEnvelopeRegistry
        .revoke(
            &mut kernel,
            &manifest,
            &AutonomyEnvelopeRevokeRequest {
                envelope_receipt_id: &receipt_id,
                revoked_by: "human_platform_lead",
                revocation_reason: "beta hardening rollback",
            },
        )
        .expect("revoke envelope");
    assert_eq!(
        kernel
            .ledger()
            .query()
            .by_id(&revoke_id)
            .map(|receipt| receipt.kind.as_str()),
        Some("autonomy.envelope.revoked")
    );
    assert_eq!(
        kernel.autonomy_envelope_authority_blocker(&envelope_id),
        Some("envelope_revoked".to_string())
    );
    assert!(kernel.ledger().verify().is_ok());
}

#[test]
fn amending_an_envelope_supersedes_it_under_the_same_owner() {
    let (mut kernel, manifest, _packet_id) = run_to_review_packet();
    let receipt_id = LocalAutonomyEnvelopeRegistry
        .record(
            &mut kernel,
            &manifest,
            &AutonomyEnvelopeRecordRequest {
                envelope: low_risk_envelope(),
                recorded_by: "human_platform_lead",
            },
        )
        .expect("record envelope");
    let amended = LocalAutonomyEnvelopeRegistry
        .amend(
            &mut kernel,
            &manifest,
            &AutonomyEnvelopeAmendRequest {
                source_envelope_receipt_id: &receipt_id,
                envelope: low_risk_envelope(),
                amended_by: "human_platform_lead",
                amendment_reason: "narrow the scope after review",
            },
        )
        .expect("amend envelope");
    assert!(
        kernel
            .ledger()
            .query()
            .by_id(&amended)
            .map(|receipt| receipt.kind.as_str())
            == Some("autonomy.envelope.amended")
    );
    assert!(kernel.ledger().verify().is_ok());
}

#[test]
fn the_authority_blocker_predicate_tracks_envelope_state() {
    let (mut kernel, manifest, _packet_id) = run_to_review_packet();
    let envelope = low_risk_envelope();
    let envelope_id = envelope.envelope_id.to_string();
    let receipt_id = LocalAutonomyEnvelopeRegistry
        .record(
            &mut kernel,
            &manifest,
            &AutonomyEnvelopeRecordRequest {
                envelope,
                recorded_by: "human_platform_lead",
            },
        )
        .expect("record envelope");

    // Unknown envelope: blocked as not found.
    assert_eq!(
        kernel.autonomy_envelope_authority_blocker("env_never_recorded"),
        Some("envelope_not_found".to_string())
    );
    // Recorded and unrevoked: active.
    assert_eq!(
        kernel.autonomy_envelope_authority_blocker(&envelope_id),
        None
    );

    // Revoked: blocked, and the same predicate the run loop polls says so.
    LocalAutonomyEnvelopeRegistry
        .revoke(
            &mut kernel,
            &manifest,
            &AutonomyEnvelopeRevokeRequest {
                envelope_receipt_id: &receipt_id,
                revoked_by: "human_platform_lead",
                revocation_reason: "beta hardening rollback",
            },
        )
        .expect("revoke envelope");
    assert_eq!(
        kernel.autonomy_envelope_authority_blocker(&envelope_id),
        Some("envelope_revoked".to_string())
    );
    assert!(kernel.ledger().verify().is_ok());
}
