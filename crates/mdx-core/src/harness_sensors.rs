// Quality sensors as backpressure inside the pipeline verdict: each
// requested sensor is evaluated against the run's OWN evidence and named
// passed, failed, or skipped - skips are visible, never silent. In-process
// sensors judge the run's honesty (no_fake_green) and its human-facing copy
// (product_copy_quiet). Sensors that need an external runner are skipped
// naming the make target that owns their truth - the runtime never forks a
// second runner. Advisory model-judged sensors are skipped as not gated
// here. Never-advisory domains (security, receipt, tenant, authority) may be
// skipped-pending-external, but that is surfaced loudly and never silently
// treated as green; a never-advisory in-process failure blocks the verdict.

pub const SENSOR_NEVER_ADVISORY_DOMAINS: &[&str] =
    &["security", "receipts", "receipt", "tenant", "authority"];

// The phrases a human must never meet on first read of a primary surface -
// mirrors the product-copy-quiet first-read contract. A pipeline that puts
// any of these in its own human-facing summaries fails the sensor.
pub const SENSOR_BANNED_FIRST_READ_PHRASES: &[&str] = &[
    "saved with proof",
    "governed write",
    "receipt-backed",
    "terminal state",
    "copy receipt",
    "proofs of work",
    "receipt id",
    "recorded with a receipt",
    "on the record -",
];

#[derive(Clone, Copy)]
enum SensorEval {
    InProcess,
    External(&'static str),
    Advisory,
}

struct KnownSensor {
    id: &'static str,
    domain: &'static str,
    authority: &'static str,
    eval: SensorEval,
}

// The sensors the runtime knows by name. Domain and authority mirror the
// generated quality-sensor-registry; the registry stays the source of
// truth and the registry check asserts the two in-process sensors here
// stay consistent with it. Unlisted ids skip as not-in-the-registry.
const KNOWN_SENSORS: &[KnownSensor] = &[
    KnownSensor {
        id: "no_fake_green",
        domain: "receipts",
        authority: "never_advisory",
        eval: SensorEval::InProcess,
    },
    KnownSensor {
        id: "product_copy_quiet",
        domain: "product_copy",
        authority: "deterministic",
        eval: SensorEval::External("make product-copy-quiet-check"),
    },
    KnownSensor {
        id: "security_invariants",
        domain: "security",
        authority: "never_advisory",
        eval: SensorEval::External("make security-check"),
    },
    KnownSensor {
        id: "contract_battery",
        domain: "contracts",
        authority: "deterministic",
        eval: SensorEval::External("cargo run -p xtask -- check"),
    },
    KnownSensor {
        id: "workspace_tests",
        domain: "code",
        authority: "deterministic",
        eval: SensorEval::External("cargo test --workspace"),
    },
    KnownSensor {
        id: "generated_clean",
        domain: "generated",
        authority: "deterministic",
        eval: SensorEval::External("make generated-clean"),
    },
    KnownSensor {
        id: "local_smoke",
        domain: "runtime",
        authority: "deterministic",
        eval: SensorEval::External("make local-smoke"),
    },
    KnownSensor {
        id: "rust_format",
        domain: "code",
        authority: "deterministic",
        eval: SensorEval::External("cargo fmt --check"),
    },
    KnownSensor {
        id: "rust_lint",
        domain: "code",
        authority: "deterministic",
        eval: SensorEval::External("cargo clippy --workspace --all-targets -- -D warnings"),
    },
    KnownSensor {
        id: "taste_and_voice_review",
        domain: "product_copy",
        authority: "advisory",
        eval: SensorEval::Advisory,
    },
    KnownSensor {
        id: "review_quality_judge",
        domain: "review",
        authority: "advisory",
        eval: SensorEval::Advisory,
    },
];

// Special-cased: product_copy_quiet's deterministic make target stays the
// external truth, but the pipeline ALSO screens its own emitted copy in
// process so a bad summary cannot reach a human between CI runs. This is
// the in-process half; the make target is the full-surface authority.
const PRODUCT_COPY_QUIET_ID: &str = "product_copy_quiet";

/// The honesty facts the caller distills from the run's own outcomes and
/// verdict flags. Kept primitive so the evaluator stays pure and trivially
/// testable, and so a dishonest run can be simulated in a unit test.
pub struct SensorRunEvidence<'a> {
    /// Every stage marked executed carries a real stage receipt id.
    pub all_executed_stages_have_receipts: bool,
    /// No stage is marked executed unless it actually mediated.
    pub executed_only_when_mediated: bool,
    /// All five forbidden-authority flags on the verdict are closed.
    pub all_authority_flags_closed: bool,
    /// The pipeline was admitted.
    pub admitted: bool,
    /// The top-line status claims completion.
    pub status_claims_completion: bool,
    /// The human-facing strings the pipeline surfaces for this run.
    pub human_copy: &'a [&'a str],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HarnessSensorOutcome {
    pub sensor_id: String,
    pub domain: String,
    pub authority: String,
    pub status: &'static str,
    pub reason: String,
    pub blocking: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SensorSummary {
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub blocked: bool,
    pub never_advisory_deferred: usize,
}

fn is_never_advisory(domain: &str) -> bool {
    SENSOR_NEVER_ADVISORY_DOMAINS.contains(&domain)
}

fn evaluate_no_fake_green(evidence: &SensorRunEvidence<'_>) -> (&'static str, String) {
    if !evidence.all_authority_flags_closed {
        return (
            "FAILED",
            "a forbidden-authority flag is open on the verdict".to_string(),
        );
    }
    if !evidence.all_executed_stages_have_receipts {
        return (
            "FAILED",
            "a stage claims execution without a receipt".to_string(),
        );
    }
    if !evidence.executed_only_when_mediated {
        return (
            "FAILED",
            "a stage is marked executed without mediating".to_string(),
        );
    }
    if evidence.status_claims_completion && !evidence.admitted {
        return (
            "FAILED",
            "the status claims completion for an unadmitted pipeline".to_string(),
        );
    }
    (
        "PASSED",
        "every executed stage carries a receipt and all authority flags stay closed".to_string(),
    )
}

fn evaluate_product_copy_quiet_in_process(
    evidence: &SensorRunEvidence<'_>,
) -> (&'static str, String) {
    for line in evidence.human_copy {
        let lowered = line.to_lowercase();
        for phrase in SENSOR_BANNED_FIRST_READ_PHRASES {
            if lowered.contains(phrase) {
                return (
                    "FAILED",
                    format!("human copy carries a first-read banned phrase: {phrase}"),
                );
            }
        }
    }
    (
        "PASSED",
        "the surfaced human copy carries no first-read banned phrase".to_string(),
    )
}

/// Evaluate each requested sensor against the run's own evidence. The order
/// of the returned outcomes follows the requested order.
pub fn evaluate_pipeline_sensors(
    requested: &[&str],
    evidence: &SensorRunEvidence<'_>,
) -> Vec<HarnessSensorOutcome> {
    requested
        .iter()
        .map(|id| {
            let known = KNOWN_SENSORS.iter().find(|sensor| sensor.id == *id);
            let Some(known) = known else {
                return HarnessSensorOutcome {
                    sensor_id: (*id).to_string(),
                    domain: "unknown".to_string(),
                    authority: "unknown".to_string(),
                    status: "SKIPPED",
                    reason: "not in the quality sensor registry".to_string(),
                    blocking: false,
                };
            };
            let (status, reason) = match known.eval {
                SensorEval::InProcess => evaluate_no_fake_green(evidence),
                SensorEval::External(check) if known.id == PRODUCT_COPY_QUIET_ID => {
                    // product_copy_quiet runs its full surface in CI, but the
                    // pipeline screens its own emitted copy in process too.
                    let (status, reason) = evaluate_product_copy_quiet_in_process(evidence);
                    if status == "FAILED" {
                        (status, reason)
                    } else {
                        (
                            "PASSED",
                            format!(
                                "emitted copy is quiet in process; full surface owned by {check}"
                            ),
                        )
                    }
                }
                SensorEval::External(check) => (
                    "SKIPPED",
                    format!("deferred to the external check runner: {check}"),
                ),
                SensorEval::Advisory => (
                    "SKIPPED",
                    "advisory - model-judged, not gated in the runtime".to_string(),
                ),
            };
            let blocking = status == "FAILED" && is_never_advisory(known.domain);
            HarnessSensorOutcome {
                sensor_id: known.id.to_string(),
                domain: known.domain.to_string(),
                authority: known.authority.to_string(),
                status,
                reason,
                blocking,
            }
        })
        .collect()
}

pub fn summarize_sensors(outcomes: &[HarnessSensorOutcome]) -> SensorSummary {
    let mut summary = SensorSummary::default();
    for outcome in outcomes {
        match outcome.status {
            "PASSED" => summary.passed += 1,
            "FAILED" => summary.failed += 1,
            _ => {
                summary.skipped += 1;
                if is_never_advisory(&outcome.domain) {
                    summary.never_advisory_deferred += 1;
                }
            }
        }
        if outcome.blocking {
            summary.blocked = true;
        }
    }
    summary
}
