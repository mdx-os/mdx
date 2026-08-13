use crate::ForgeWorkClassificationDraft;

pub const FORGE_RUN_STRATEGY_VERSION: &str = "forge_run_strategy_v1";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ForgeRunStrategyOverrides {
    pub outcome_preference: String,
    pub harness_id: String,
    pub planner_mode: String,
    pub review_mode: String,
    pub requested_width: Option<u32>,
    pub max_turns: Option<u32>,
    pub max_cost_cents: Option<u32>,
    pub max_runtime_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeRunStrategy {
    pub version: &'static str,
    pub mode: &'static str,
    pub outcome_preference: String,
    pub execution_shape: &'static str,
    pub width: u32,
    pub harness_id: String,
    pub harness_source: &'static str,
    pub planner_mode: &'static str,
    pub planner_source: &'static str,
    pub review_mode: &'static str,
    pub review_source: &'static str,
    pub advisor_mode: &'static str,
    pub max_turns: u32,
    pub max_cost_cents: u32,
    pub max_runtime_ms: u64,
    pub operator_locked_fields: Vec<&'static str>,
    pub policy_floor_fields: Vec<&'static str>,
    pub rationale: String,
    pub grants_execution_authority: bool,
}

pub fn resolve_forge_run_strategy(
    classification: &ForgeWorkClassificationDraft,
    overrides: &ForgeRunStrategyOverrides,
) -> ForgeRunStrategy {
    let outcome_preference = normalize_outcome_preference(&overrides.outcome_preference);
    let high_risk = high_risk_class(&classification.task_class);
    let mission_scale = matches!(
        classification.complexity_tier.as_str(),
        "large" | "xl" | "extreme"
    ) || classification.mission_recommended;

    let mut locked = Vec::new();
    let mut floors = Vec::new();

    let width = overrides
        .requested_width
        .unwrap_or(classification.recommended_width);
    if overrides.requested_width.is_some() {
        locked.push("width");
    }

    let requested_harness = normalize_harness(&overrides.harness_id);
    let (harness_id, harness_source) = if requested_harness == "auto" {
        ("mdx_native".to_string(), "native_learning_default")
    } else {
        locked.push("harness");
        (requested_harness, "operator_override")
    };

    let recommended_planner = recommended_planner_mode(
        &classification.complexity_tier,
        classification.confidence_pct,
        high_risk,
        &outcome_preference,
    );
    let requested_planner = normalize_planner_mode(&overrides.planner_mode);
    let (planner_mode, planner_source) = if requested_planner == "auto" {
        (recommended_planner, "adaptive_policy")
    } else if (high_risk || mission_scale)
        && planner_rank(requested_planner) < planner_rank("required")
    {
        locked.push("planner_mode");
        floors.push("planner_mode");
        ("required", "policy_floor")
    } else {
        locked.push("planner_mode");
        (requested_planner, "operator_override")
    };

    let recommended_review = recommended_review_mode(
        &classification.complexity_tier,
        high_risk,
        &outcome_preference,
    );
    let requested_review = normalize_review_mode(&overrides.review_mode);
    let (review_mode, review_source) = if requested_review == "auto" {
        (recommended_review, "adaptive_policy")
    } else if (high_risk || mission_scale)
        && review_rank(requested_review) < review_rank("required_independent")
    {
        locked.push("review_mode");
        floors.push("review_mode");
        ("required_independent", "policy_floor")
    } else {
        locked.push("review_mode");
        (requested_review, "operator_override")
    };

    let (default_turns, default_cost, default_runtime) =
        recommended_budget(&classification.complexity_tier);
    let max_turns = overrides.max_turns.unwrap_or(default_turns);
    let max_cost_cents = overrides.max_cost_cents.unwrap_or(default_cost);
    let max_runtime_ms = overrides.max_runtime_ms.unwrap_or(default_runtime);
    if overrides.max_turns.is_some() {
        locked.push("max_turns");
    }
    if overrides.max_cost_cents.is_some() {
        locked.push("max_cost_cents");
    }
    if overrides.max_runtime_ms.is_some() {
        locked.push("max_runtime_ms");
    }

    let mode = if locked.is_empty() { "auto" } else { "custom" };
    let execution_shape = if classification.mission_recommended {
        "mission"
    } else if width > 4 {
        "fleet"
    } else if width > 1 {
        "candidates"
    } else {
        "run"
    };
    let advisor_mode = if high_risk || mission_scale || width > 4 {
        "eligible_on_trigger"
    } else {
        "skip"
    };
    let rationale = format!(
        "{} {} work at {}% classification confidence resolves to {} with {} worker(s), {} planning, {} review, and {} as the harness.",
        classification.complexity_tier,
        classification.task_class,
        classification.confidence_pct,
        execution_shape,
        width,
        planner_mode,
        review_mode,
        harness_id
    );

    ForgeRunStrategy {
        version: FORGE_RUN_STRATEGY_VERSION,
        mode,
        outcome_preference,
        execution_shape,
        width,
        harness_id,
        harness_source,
        planner_mode,
        planner_source,
        review_mode,
        review_source,
        advisor_mode,
        max_turns,
        max_cost_cents,
        max_runtime_ms,
        operator_locked_fields: locked,
        policy_floor_fields: floors,
        rationale,
        grants_execution_authority: false,
    }
}

fn normalize_outcome_preference(value: &str) -> String {
    match value.trim() {
        "faster" | "confidence" => value.trim().to_string(),
        _ => "balanced".to_string(),
    }
}

fn normalize_harness(value: &str) -> String {
    match value.trim() {
        "mdx_native" | "codex_cli" | "grok_build" => value.trim().to_string(),
        _ => "auto".to_string(),
    }
}

fn normalize_planner_mode(value: &str) -> &'static str {
    match value.trim() {
        "skip" => "skip",
        "bounded" => "bounded",
        "required" => "required",
        _ => "auto",
    }
}

fn normalize_review_mode(value: &str) -> &'static str {
    match value.trim() {
        "deterministic_only" => "deterministic_only",
        "conditional_independent" => "conditional_independent",
        "required_independent" => "required_independent",
        _ => "auto",
    }
}

fn high_risk_class(task_class: &str) -> bool {
    matches!(
        task_class,
        "security" | "architecture" | "api_compat" | "migration" | "concurrency"
    )
}

fn recommended_planner_mode(
    complexity: &str,
    confidence: u32,
    high_risk: bool,
    preference: &str,
) -> &'static str {
    if high_risk || matches!(complexity, "large" | "xl" | "extreme") {
        return "required";
    }
    if preference == "confidence" || confidence < 75 {
        return "bounded";
    }
    if complexity == "medium" && preference != "faster" {
        return "bounded";
    }
    "skip"
}

fn recommended_review_mode(complexity: &str, high_risk: bool, preference: &str) -> &'static str {
    if high_risk || matches!(complexity, "large" | "xl" | "extreme") {
        return "required_independent";
    }
    if preference == "confidence" || (complexity == "medium" && preference != "faster") {
        return "conditional_independent";
    }
    "deterministic_only"
}

fn recommended_budget(complexity: &str) -> (u32, u32, u64) {
    match complexity {
        "small" => (20, 200, 15 * 60 * 1_000),
        "medium" => (45, 500, 30 * 60 * 1_000),
        "large" => (80, 1_200, 60 * 60 * 1_000),
        "xl" => (120, 2_500, 2 * 60 * 60 * 1_000),
        "extreme" => (180, 5_000, 4 * 60 * 60 * 1_000),
        _ => (45, 500, 30 * 60 * 1_000),
    }
}

fn planner_rank(mode: &str) -> u8 {
    match mode {
        "required" => 2,
        "bounded" => 1,
        _ => 0,
    }
}

fn review_rank(mode: &str) -> u8 {
    match mode {
        "required_independent" => 2,
        "conditional_independent" => 1,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classify_forge_work;

    #[test]
    fn small_clear_work_stays_lean() {
        let classification =
            classify_forge_work("Fix the typo in the Forge label", "make ui-app-check");
        let strategy = resolve_forge_run_strategy(&classification, &Default::default());

        assert_eq!(strategy.mode, "auto");
        assert_eq!(strategy.execution_shape, "run");
        assert_eq!(strategy.harness_id, "mdx_native");
        assert_eq!(strategy.planner_mode, "skip");
        assert_eq!(strategy.review_mode, "deterministic_only");
        assert_eq!(strategy.max_cost_cents, 200);
        assert!(!strategy.grants_execution_authority);
    }

    #[test]
    fn medium_work_gets_bounded_intelligence() {
        let classification = classify_forge_work(
            "Implement a Forge feature with tests",
            "cargo test -p mdx-server",
        );
        let strategy = resolve_forge_run_strategy(&classification, &Default::default());

        assert_eq!(strategy.planner_mode, "bounded");
        assert_eq!(strategy.review_mode, "conditional_independent");
    }

    #[test]
    fn faster_preference_removes_optional_medium_ceremony() {
        let classification = classify_forge_work(
            "Implement a Forge feature with tests",
            "cargo test -p mdx-server",
        );
        let overrides = ForgeRunStrategyOverrides {
            outcome_preference: "faster".to_string(),
            ..Default::default()
        };
        let strategy = resolve_forge_run_strategy(&classification, &overrides);

        assert_eq!(strategy.planner_mode, "skip");
        assert_eq!(strategy.review_mode, "deterministic_only");
    }

    #[test]
    fn risky_work_keeps_planning_and_review_floors() {
        let classification = classify_forge_work(
            "Change the API compatibility boundary across the architecture",
            "cargo test --workspace",
        );
        let overrides = ForgeRunStrategyOverrides {
            planner_mode: "skip".to_string(),
            review_mode: "deterministic_only".to_string(),
            ..Default::default()
        };
        let strategy = resolve_forge_run_strategy(&classification, &overrides);

        assert_eq!(strategy.planner_mode, "required");
        assert_eq!(strategy.review_mode, "required_independent");
        assert_eq!(strategy.planner_source, "policy_floor");
        assert_eq!(strategy.review_source, "policy_floor");
        assert_eq!(
            strategy.policy_floor_fields,
            vec!["planner_mode", "review_mode"]
        );
    }

    #[test]
    fn senior_engineer_can_lock_supported_harness_and_budget() {
        let classification = classify_forge_work("Fix a small Rust bug", "cargo test -p mdx-core");
        let overrides = ForgeRunStrategyOverrides {
            harness_id: "grok_build".to_string(),
            max_cost_cents: Some(500),
            ..Default::default()
        };
        let strategy = resolve_forge_run_strategy(&classification, &overrides);

        assert_eq!(strategy.mode, "custom");
        assert_eq!(strategy.harness_id, "grok_build");
        assert_eq!(strategy.harness_source, "operator_override");
        assert!(strategy.operator_locked_fields.contains(&"harness"));
        assert!(strategy.operator_locked_fields.contains(&"max_cost_cents"));
    }
}
