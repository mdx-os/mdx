use crate::*;

const TENANT: &str = "tenant_local";
const ACTOR: &str = "human:local_user";

#[test]
fn a_fresh_install_has_no_chosen_track() {
    let kernel = MdxKernel::boot_local();
    let chosen = kernel.chosen_setup_track_local();
    assert!(!chosen.chosen);
    assert!(chosen.track.is_empty());
}

#[test]
fn choosing_a_track_records_it_and_derives_it() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel
        .choose_setup_track(ChooseSetupTrack {
            tenant_id: TENANT,
            actor_id: ACTOR,
            track: "try",
        })
        .expect("choose");
    assert_eq!(report.status, "SETUP_TRACK_CHOSEN");
    assert_eq!(report.track, "try");
    assert!(!report.receipt_id.is_empty());

    let receipt = kernel
        .ledger()
        .query()
        .by_id(&report.receipt_id)
        .expect("track receipt");
    assert_eq!(receipt.kind, SETUP_TRACK_CHOSEN_RECEIPT_KIND);
    assert_eq!(receipt.payload["track"], "try");
    assert_eq!(receipt.payload["chosen_by"], ACTOR);

    let chosen = kernel.chosen_setup_track_local();
    assert!(chosen.chosen);
    assert_eq!(chosen.track, "try");
}

#[test]
fn the_latest_choice_wins_so_the_track_is_revisitable() {
    let mut kernel = MdxKernel::boot_local();
    for track in ["try", "build", "prepare"] {
        kernel
            .choose_setup_track(ChooseSetupTrack {
                tenant_id: TENANT,
                actor_id: ACTOR,
                track,
            })
            .expect("choose");
    }
    // The most recent choice is the one the router and front door read.
    assert_eq!(kernel.chosen_setup_track_local().track, "prepare");
}

#[test]
fn an_unknown_track_is_refused() {
    let mut kernel = MdxKernel::boot_local();
    let error = kernel
        .choose_setup_track(ChooseSetupTrack {
            tenant_id: TENANT,
            actor_id: ACTOR,
            track: "deploy-to-mars",
        })
        .expect_err("unknown track should fail");
    assert!(matches!(error, SetupTrackError::UnknownTrack(_)));
    // Nothing was recorded.
    assert!(!kernel.chosen_setup_track_local().chosen);
}

#[test]
fn a_blank_track_is_refused() {
    let mut kernel = MdxKernel::boot_local();
    let error = kernel
        .choose_setup_track(ChooseSetupTrack {
            tenant_id: TENANT,
            actor_id: ACTOR,
            track: "  ",
        })
        .expect_err("blank track should fail");
    assert!(matches!(error, SetupTrackError::Missing("track")));
}

#[test]
fn pre_activation_refusal_records_without_choosing_a_track() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel
        .refuse_setup_track_pre_activation(
            ChooseSetupTrack {
                tenant_id: TENANT,
                actor_id: ACTOR,
                track: "try",
            },
            "owner and usable model must be established before choosing a path",
            true,
            false,
        )
        .expect("refuse");

    assert_eq!(report.status, "SETUP_TRACK_REFUSED_PRE_ACTIVATION");
    assert!(!report.receipt_id.is_empty());
    assert!(!kernel.chosen_setup_track_local().chosen);
    let receipt = kernel
        .ledger()
        .query()
        .by_id(&report.receipt_id)
        .expect("refusal receipt");
    assert_eq!(receipt.kind, SETUP_TRACK_REFUSED_RECEIPT_KIND);
    assert_eq!(receipt.payload["track"], "try");
    assert_eq!(receipt.payload["owner_claimed"], "true");
    assert_eq!(receipt.payload["model_connected"], "false");
}
