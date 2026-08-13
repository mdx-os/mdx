//! The ship decision for a Forge harness run. A run produced a branch and
//! proved it with the project's own checks; a human reviewed the diff;
//! now a human - and only a human - ratifies that it ships. This is the
//! ship rail for the forge-run execution model: its integrity comes from
//! the run's OWN receipts, not a separate ceremony.
//!
//! The gate is real, not decorative. A ship decision is refused unless
//! the named run actually finished done with a branch and at least one
//! passed check and no failure after its last pass - you cannot ship a
//! run that did not finish green. The human's reason is required, and the
//! decision is the human's verb: the harness assembled the evidence, the
//! person ratifies it. Nothing here authorizes deployment; ratification
//! clears the ship door, and the separate deploy step reads this receipt.

use crate::strategy_board::BoardWrite;
use crate::{ActionKind, GovernedWriteIdentity, MdxKernel, StorageProvider, payload};

#[derive(Clone, Copy, Debug)]
pub struct ForgeRunShipDecision<'a> {
    pub tenant_id: &'a str,
    pub human_actor_id: &'a str,
    pub run_id: &'a str,
    /// The commit the human reviewed and is ratifying.
    pub commit_sha: &'a str,
    /// The branch tip the caller observed in the actual repo at decision
    /// time. The gate compares this against commit_sha so a ratification
    /// can never bind to a commit that is not what the branch currently
    /// holds. Empty means the caller could not observe a tip, which is
    /// refused - a ship decision the kernel cannot verify does not ship.
    pub observed_branch_tip: &'a str,
    /// Why ship - the human's own words, required.
    pub reason: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeRunShipReport {
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub branch: String,
    pub terminal_state: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ForgeRunShipError {
    Missing(&'static str),
    RunNotFound,
    RunNotFinishedDone,
    NoBranch,
    NoPassedCheck,
    CommitMismatch,
}

impl ForgeRunShipError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("ship decision missing {field}"),
            Self::RunNotFound => "no such run - nothing to ship".to_string(),
            Self::RunNotFinishedDone => {
                "this run did not finish done - only a run that completed its work and went green can ship".to_string()
            }
            Self::NoBranch => "this run produced no branch - there is nothing to ship".to_string(),
            Self::NoPassedCheck => {
                "this run has no passed check - a ship needs proof the work holds".to_string()
            }
            Self::CommitMismatch => {
                "the commit you are ratifying is not this run's branch tip - review the current diff".to_string()
            }
        }
    }
}

/// What the run's receipts say about its shippability: derived, never
/// stored. The gate reads this, so the truth a ship rests on is the same
/// truth the run viewer shows.
struct RunShipFacts {
    found: bool,
    finished_done: bool,
    branch: String,
    passed_checks: u32,
    failed_after_last_pass: bool,
}

impl<S: StorageProvider> MdxKernel<S> {
    fn forge_run_ship_facts(&self, run_id: &str) -> RunShipFacts {
        let mut facts = RunShipFacts {
            found: false,
            finished_done: false,
            branch: String::new(),
            passed_checks: 0,
            failed_after_last_pass: false,
        };
        let mut failed_since_pass = false;
        for receipt in self.ledger().query().by_kind("forge.run.event").iter() {
            if receipt.payload.get("run_id").map(String::as_str) != Some(run_id) {
                continue;
            }
            facts.found = true;
            let event = receipt
                .payload
                .get("event")
                .map(String::as_str)
                .unwrap_or("");
            let detail = receipt
                .payload
                .get("detail")
                .map(String::as_str)
                .unwrap_or("");
            match event {
                "check_passed" => {
                    facts.passed_checks += 1;
                    failed_since_pass = false;
                }
                "check_failed" => {
                    failed_since_pass = true;
                }
                "evidence_appended" if detail.starts_with("branch=") => {
                    facts.branch = detail
                        .trim_start_matches("branch=")
                        .split(' ')
                        .next()
                        .unwrap_or("")
                        .to_string();
                }
                "run_finished" => {
                    facts.finished_done = detail.contains("RUN_FINISHED_DONE");
                }
                _ => {}
            }
        }
        facts.failed_after_last_pass = failed_since_pass;
        facts
    }

    pub fn record_forge_run_ship_decision(
        &mut self,
        request: ForgeRunShipDecision<'_>,
    ) -> Result<ForgeRunShipReport, ForgeRunShipError> {
        let identity = GovernedWriteIdentity::local_demo(request.human_actor_id);
        self.record_forge_run_ship_decision_with_identity(request, &identity)
    }

    pub fn record_forge_run_ship_decision_with_identity(
        &mut self,
        request: ForgeRunShipDecision<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<ForgeRunShipReport, ForgeRunShipError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("human_actor_id", request.human_actor_id),
            ("run_id", request.run_id),
            ("reason", request.reason),
        ] {
            if value.trim().is_empty() {
                return Err(ForgeRunShipError::Missing(field));
            }
        }
        // The gate, read from the run's own receipts.
        let facts = self.forge_run_ship_facts(request.run_id.trim());
        if !facts.found {
            return Err(ForgeRunShipError::RunNotFound);
        }
        if !facts.finished_done {
            return Err(ForgeRunShipError::RunNotFinishedDone);
        }
        if facts.branch.is_empty() {
            return Err(ForgeRunShipError::NoBranch);
        }
        if facts.passed_checks == 0 || facts.failed_after_last_pass {
            return Err(ForgeRunShipError::NoPassedCheck);
        }
        // Last gate: the ratified commit must be the branch tip the caller
        // actually observed in the repo. A ship decision the kernel cannot
        // verify against a real tip does not ship, and a stale one - the
        // branch moved after review - is refused so the human re-reviews
        // the diff that would actually leave the machine.
        if request.commit_sha.trim().is_empty() {
            return Err(ForgeRunShipError::Missing("commit_sha"));
        }
        if request.observed_branch_tip.trim().is_empty() {
            return Err(ForgeRunShipError::Missing("observed_branch_tip"));
        }
        // Run receipts and reviewer surfaces carry abbreviated shas, so a
        // ratification may name a prefix of the observed tip - but never a
        // prefix short enough to be ambiguous, and never a different
        // commit.
        let ratified = request.commit_sha.trim();
        let observed = request.observed_branch_tip.trim();
        let matches_tip =
            ratified == observed || (ratified.len() >= 12 && observed.starts_with(ratified));
        if !matches_tip {
            return Err(ForgeRunShipError::CommitMismatch);
        }

        let passed = facts.passed_checks.to_string();
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.human_actor_id,
                loop_id: "forge_run_ship",
                action: ActionKind::RecordForgeRunShipDecision,
                transition: "RECORD_FORGE_RUN_SHIP_DECISION",
                kind: "forge.run.ship.decided",
            },
            payload(&[
                ("run_id", request.run_id),
                ("branch", &facts.branch),
                ("commit_sha", request.commit_sha),
                ("observed_branch_tip", request.observed_branch_tip),
                ("commit_tip_verified", "true"),
                ("reason", request.reason),
                ("passed_checks", &passed),
                ("human_ratified", "true"),
                ("identity_source", &identity.identity_source),
                // Ratification clears the ship door only. Deployment is a
                // separate step that READS this receipt; it is not opened here.
                ("deployment_eligible", "true"),
                ("deployment_authority_granted", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(ForgeRunShipReport {
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            branch: facts.branch,
            terminal_state: "FORGE_RUN_SHIP_RATIFIED",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(kernel: &mut MdxKernel, run_id: &str, event: &str, detail: &str) {
        kernel
            .record_forge_run_event(crate::ForgeRunEvent {
                tenant_id: "t",
                actor_id: "forge_orchestrator_agent",
                run_id,
                event,
                work_item_id: "w1",
                detail,
                turn: 1,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("event records");
    }

    fn ship(kernel: &mut MdxKernel, run_id: &str) -> Result<ForgeRunShipReport, ForgeRunShipError> {
        kernel.record_forge_run_ship_decision(ForgeRunShipDecision {
            tenant_id: "t",
            human_actor_id: "human:dev",
            run_id,
            commit_sha: "abc123",
            observed_branch_tip: "abc123",
            reason: "reviewed the diff; it is correct and minimal",
        })
    }

    #[test]
    fn a_green_finished_run_with_a_branch_ships_and_a_red_one_is_refused() {
        let mut kernel = MdxKernel::boot_local();
        // An unfinished run: refused.
        event(&mut kernel, "r_open", "run_started", "accepted");
        assert_eq!(
            ship(&mut kernel, "r_open"),
            Err(ForgeRunShipError::RunNotFinishedDone)
        );

        // A run that finished done, passed a check, and committed a branch.
        event(&mut kernel, "r_ok", "run_started", "accepted");
        event(
            &mut kernel,
            "r_ok",
            "check_passed",
            "run_command cargo build exit=0",
        );
        event(
            &mut kernel,
            "r_ok",
            "evidence_appended",
            "branch=forge/run-r_ok sha=abc123",
        );
        event(
            &mut kernel,
            "r_ok",
            "run_finished",
            "status=RUN_FINISHED_DONE turns=7",
        );
        let report = ship(&mut kernel, "r_ok").expect("a green run ships");
        assert_eq!(report.branch, "forge/run-r_ok");
        assert_eq!(report.terminal_state, "FORGE_RUN_SHIP_RATIFIED");

        // A run whose last check FAILED after its pass: refused.
        event(&mut kernel, "r_red", "check_passed", "exit=0");
        event(&mut kernel, "r_red", "check_failed", "exit=1");
        event(
            &mut kernel,
            "r_red",
            "evidence_appended",
            "branch=forge/run-r_red",
        );
        event(
            &mut kernel,
            "r_red",
            "run_finished",
            "status=RUN_FINISHED_DONE",
        );
        assert_eq!(
            ship(&mut kernel, "r_red"),
            Err(ForgeRunShipError::NoPassedCheck)
        );

        // Unknown run: refused. Missing reason: refused.
        assert_eq!(
            ship(&mut kernel, "nope"),
            Err(ForgeRunShipError::RunNotFound)
        );
        let no_reason = kernel.record_forge_run_ship_decision(ForgeRunShipDecision {
            tenant_id: "t",
            human_actor_id: "human:dev",
            run_id: "r_ok",
            commit_sha: "abc123",
            observed_branch_tip: "abc123",
            reason: "",
        });
        assert_eq!(no_reason, Err(ForgeRunShipError::Missing("reason")));
        assert!(kernel.ledger().verify().is_ok());
    }

    fn green_run(kernel: &mut MdxKernel, run_id: &str) {
        event(kernel, run_id, "run_started", "accepted");
        event(
            kernel,
            run_id,
            "check_passed",
            "run_command cargo build exit=0",
        );
        event(
            kernel,
            run_id,
            "evidence_appended",
            &format!("branch=forge/run-{run_id} sha=abc123"),
        );
        event(kernel, run_id, "run_finished", "status=RUN_FINISHED_DONE");
    }

    #[test]
    fn a_blank_tenant_is_refused_as_missing() {
        let mut kernel = MdxKernel::boot_local();
        green_run(&mut kernel, "r_ok");
        let result = kernel.record_forge_run_ship_decision(ForgeRunShipDecision {
            tenant_id: "  ",
            human_actor_id: "human:dev",
            run_id: "r_ok",
            commit_sha: "abc123",
            observed_branch_tip: "abc123",
            reason: "reviewed the diff; it is correct and minimal",
        });
        assert_eq!(result, Err(ForgeRunShipError::Missing("tenant_id")));
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn a_blank_human_actor_is_refused_as_missing() {
        let mut kernel = MdxKernel::boot_local();
        green_run(&mut kernel, "r_ok");
        let result = kernel.record_forge_run_ship_decision(ForgeRunShipDecision {
            tenant_id: "t",
            human_actor_id: "",
            run_id: "r_ok",
            commit_sha: "abc123",
            observed_branch_tip: "abc123",
            reason: "reviewed the diff; it is correct and minimal",
        });
        assert_eq!(result, Err(ForgeRunShipError::Missing("human_actor_id")));
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn a_blank_run_id_is_refused_as_missing() {
        let mut kernel = MdxKernel::boot_local();
        let result = kernel.record_forge_run_ship_decision(ForgeRunShipDecision {
            tenant_id: "t",
            human_actor_id: "human:dev",
            run_id: "   ",
            commit_sha: "abc123",
            observed_branch_tip: "abc123",
            reason: "reviewed the diff; it is correct and minimal",
        });
        assert_eq!(result, Err(ForgeRunShipError::Missing("run_id")));
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn a_finished_run_with_no_branch_is_refused() {
        let mut kernel = MdxKernel::boot_local();
        // Finished done and a check passed, but no branch was ever appended.
        event(&mut kernel, "r_nobranch", "run_started", "accepted");
        event(&mut kernel, "r_nobranch", "check_passed", "exit=0");
        event(
            &mut kernel,
            "r_nobranch",
            "run_finished",
            "status=RUN_FINISHED_DONE",
        );
        assert_eq!(
            ship(&mut kernel, "r_nobranch"),
            Err(ForgeRunShipError::NoBranch)
        );
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn a_finished_run_with_no_passed_check_is_refused() {
        let mut kernel = MdxKernel::boot_local();
        // Finished done with a branch, but no check ever passed.
        event(&mut kernel, "r_nopass", "run_started", "accepted");
        event(
            &mut kernel,
            "r_nopass",
            "evidence_appended",
            "branch=forge/run-r_nopass sha=abc123",
        );
        event(
            &mut kernel,
            "r_nopass",
            "run_finished",
            "status=RUN_FINISHED_DONE",
        );
        assert_eq!(
            ship(&mut kernel, "r_nopass"),
            Err(ForgeRunShipError::NoPassedCheck)
        );
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn a_ratification_that_does_not_match_the_observed_branch_tip_is_refused() {
        let mut kernel = MdxKernel::boot_local();
        green_run(&mut kernel, "r_ok");
        let result = kernel.record_forge_run_ship_decision(ForgeRunShipDecision {
            tenant_id: "t",
            human_actor_id: "human:dev",
            run_id: "r_ok",
            commit_sha: "abc123",
            observed_branch_tip: "def456",
            reason: "reviewed the diff; it is correct and minimal",
        });
        assert_eq!(result, Err(ForgeRunShipError::CommitMismatch));
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn a_ship_decision_without_a_commit_or_observed_tip_is_refused() {
        let mut kernel = MdxKernel::boot_local();
        green_run(&mut kernel, "r_ok");
        let no_commit = kernel.record_forge_run_ship_decision(ForgeRunShipDecision {
            tenant_id: "t",
            human_actor_id: "human:dev",
            run_id: "r_ok",
            commit_sha: "",
            observed_branch_tip: "abc123",
            reason: "reviewed the diff; it is correct and minimal",
        });
        assert_eq!(no_commit, Err(ForgeRunShipError::Missing("commit_sha")));
        let no_tip = kernel.record_forge_run_ship_decision(ForgeRunShipDecision {
            tenant_id: "t",
            human_actor_id: "human:dev",
            run_id: "r_ok",
            commit_sha: "abc123",
            observed_branch_tip: "  ",
            reason: "reviewed the diff; it is correct and minimal",
        });
        assert_eq!(
            no_tip,
            Err(ForgeRunShipError::Missing("observed_branch_tip"))
        );
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn a_green_run_ships_through_the_explicit_identity_entry_point() {
        let mut kernel = MdxKernel::boot_local();
        green_run(&mut kernel, "r_ok");
        let identity = GovernedWriteIdentity::local_demo("human:dev");
        let report = kernel
            .record_forge_run_ship_decision_with_identity(
                ForgeRunShipDecision {
                    tenant_id: "t",
                    human_actor_id: "human:dev",
                    run_id: "r_ok",
                    commit_sha: "abc123",
                    observed_branch_tip: "abc123",
                    reason: "reviewed the diff; it is correct and minimal",
                },
                &identity,
            )
            .expect("a green run ships");
        assert_eq!(report.branch, "forge/run-r_ok");
        assert_eq!(report.terminal_state, "FORGE_RUN_SHIP_RATIFIED");
        assert!(kernel.ledger().verify().is_ok());
    }
}
