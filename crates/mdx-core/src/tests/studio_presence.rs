use crate::*;

const TENANT: &str = "tenant_local";
const ACTOR: &str = "human:local_user";

fn seed(kernel: &mut MdxKernel) -> String {
    kernel.run_evals_runner_agent().expect("seed").receipts[0].clone()
}

fn open(kernel: &mut MdxKernel, run_id: &str, source: &str) {
    kernel
        .open_studio_run(StudioOpenRun {
            tenant_id: TENANT,
            actor_id: ACTOR,
            run_id,
            goal: "Draft the board memo",
            artifact_kind: "document",
            source_receipt_id: source,
            source_kind: "",
            source_title: "",
        })
        .expect("open run");
}

#[test]
fn a_second_operator_marking_presence_joins_the_roster() {
    let mut kernel = MdxKernel::boot_local();
    let source = seed(&mut kernel);
    let run = "studio_run_presence";
    open(&mut kernel, run, &source);

    let report = kernel
        .mark_studio_presence(StudioPresenceMark {
            tenant_id: TENANT,
            actor_id: "human:reviewer",
            run_id: run,
            operator_id: "human:reviewer",
            presence_scope: "reviewing",
        })
        .expect("mark presence");
    assert_eq!(report.status, "STUDIO_PRESENCE_MARKED");

    let roster = kernel.studio_run_presence(run);
    let actors: Vec<&str> = roster.iter().map(|e| e.actor_id.as_str()).collect();
    // The opener (from the opened receipt's identity) and the reviewer are both
    // on the run; the most recent (the reviewer) leads the roster.
    assert!(
        actors.contains(&"human:reviewer"),
        "reviewer present, got {actors:?}"
    );
    assert_eq!(
        roster.first().map(|e| e.actor_id.as_str()),
        Some("human:reviewer")
    );
    assert!(
        roster
            .iter()
            .find(|e| e.actor_id == "human:reviewer")
            .unwrap()
            .active
    );
}

#[test]
fn presence_on_an_unknown_run_is_refused() {
    let mut kernel = MdxKernel::boot_local();
    let error = kernel
        .mark_studio_presence(StudioPresenceMark {
            tenant_id: TENANT,
            actor_id: ACTOR,
            run_id: "studio_run_ghost",
            operator_id: ACTOR,
            presence_scope: "",
        })
        .expect_err("presence on a non-existent run should fail");
    assert!(matches!(error, StudioRunError::UnknownRun(_)));
}

#[test]
fn presence_is_bounded_and_marks_recent_actors_active() {
    let mut kernel = MdxKernel::boot_local();
    let source = seed(&mut kernel);
    let run = "studio_run_crowd";
    open(&mut kernel, run, &source);
    for n in 0..6 {
        kernel
            .mark_studio_presence(StudioPresenceMark {
                tenant_id: TENANT,
                actor_id: ACTOR,
                run_id: run,
                operator_id: &format!("human:op_{n}"),
                presence_scope: "",
            })
            .expect("mark");
    }
    let roster = kernel.studio_run_presence(run);
    assert!(roster.len() <= STUDIO_PRESENCE_ROSTER_LIMIT);
    let active = roster.iter().filter(|e| e.active).count();
    assert_eq!(active, STUDIO_PRESENCE_ACTIVE_RECENT);
    // The most recent operator is active; an early one is around, not active.
    assert_eq!(
        roster.first().map(|e| e.actor_id.as_str()),
        Some("human:op_5")
    );
}
