use mdx_core::{
    ForgeRunStrategy, ForgeRunStrategyOverrides, ForgeWorkClassificationDraft, json_string_literal,
    resolve_forge_run_strategy,
};

pub(crate) fn strategy_enabled(body: &str) -> bool {
    matches!(
        json_value(body)
            .get("strategy_mode")
            .and_then(serde_json::Value::as_str),
        Some("auto" | "custom")
    )
}

pub(crate) fn resolve_strategy(
    classification: &ForgeWorkClassificationDraft,
    body: &str,
    legacy_requested_width: Option<u32>,
) -> ForgeRunStrategy {
    let value = json_value(body);
    let enabled = strategy_enabled(body);
    let width_locked = value
        .get("width_locked")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(!enabled);
    let requested_width = if width_locked {
        value
            .get("fleet_width")
            .or_else(|| value.get("requested_workers"))
            .and_then(json_u32)
            .or(legacy_requested_width)
    } else {
        None
    };
    let mut strategy = resolve_forge_run_strategy(
        classification,
        &ForgeRunStrategyOverrides {
            outcome_preference: json_string(&value, "outcome_preference"),
            harness_id: json_string(&value, "harness_id"),
            planner_mode: json_string(&value, "planner_mode"),
            review_mode: json_string(&value, "review_mode"),
            requested_width,
            max_turns: value.get("max_turns").and_then(json_u32).filter(|v| *v > 0),
            max_cost_cents: value
                .get("max_cost_cents")
                .and_then(json_u32)
                .filter(|v| *v > 0),
            max_runtime_ms: value
                .get("max_runtime_ms")
                .and_then(serde_json::Value::as_u64)
                .filter(|v| *v > 0),
        },
    );
    if !enabled {
        // Existing API callers keep the pre-strategy loop semantics. The new
        // adaptive policy is activated only by an explicit strategy_mode,
        // which the Forge product surface always sends.
        strategy.harness_id = "mdx_native".to_string();
        strategy.harness_source = "native_learning_default";
        strategy.planner_mode = "bounded";
        strategy.planner_source = "adaptive_policy";
        strategy.review_mode = "conditional_independent";
        strategy.review_source = "adaptive_policy";
        strategy.rationale = format!(
            "Legacy Forge caller preserved the native bounded-planning and conditional-review loop at width {}.",
            strategy.width
        );
    }
    strategy
}

pub(crate) fn strategy_json(strategy: &ForgeRunStrategy) -> String {
    format!(
        r#"{{"version":{},"mode":{},"outcome_preference":{},"execution_shape":{},"width":{},"harness_id":{},"harness_source":{},"planner_mode":{},"planner_source":{},"review_mode":{},"review_source":{},"advisor_mode":{},"max_turns":{},"max_cost_cents":{},"max_runtime_ms":{},"operator_locked_fields":[{}],"policy_floor_fields":[{}],"rationale":{},"grants_execution_authority":{}}}"#,
        json_string_literal(strategy.version),
        json_string_literal(strategy.mode),
        json_string_literal(&strategy.outcome_preference),
        json_string_literal(strategy.execution_shape),
        strategy.width,
        json_string_literal(&strategy.harness_id),
        json_string_literal(strategy.harness_source),
        json_string_literal(strategy.planner_mode),
        json_string_literal(strategy.planner_source),
        json_string_literal(strategy.review_mode),
        json_string_literal(strategy.review_source),
        json_string_literal(strategy.advisor_mode),
        strategy.max_turns,
        strategy.max_cost_cents,
        strategy.max_runtime_ms,
        string_array_json(&strategy.operator_locked_fields),
        string_array_json(&strategy.policy_floor_fields),
        json_string_literal(&strategy.rationale),
        strategy.grants_execution_authority,
    )
}

fn json_value(body: &str) -> serde_json::Value {
    serde_json::from_str(body).unwrap_or_else(|_| serde_json::json!({}))
}

fn json_string(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn json_u32(value: &serde_json::Value) -> Option<u32> {
    value.as_u64().and_then(|value| u32::try_from(value).ok())
}

fn string_array_json(values: &[&str]) -> String {
    values
        .iter()
        .map(|value| json_string_literal(value))
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdx_core::classify_forge_work;

    #[test]
    fn auto_strategy_ignores_unlocked_width() {
        let classification = classify_forge_work(
            "Implement a Forge feature with tests",
            "cargo test -p mdx-server",
        );
        let strategy = resolve_strategy(
            &classification,
            r#"{"strategy_mode":"auto","fleet_width":1,"width_locked":false}"#,
            Some(1),
        );

        assert_eq!(strategy.width, classification.recommended_width);
        assert!(!strategy.operator_locked_fields.contains(&"width"));
    }

    #[test]
    fn strategy_json_keeps_authority_closed() {
        let classification =
            classify_forge_work("Fix a small Forge bug", "cargo test -p mdx-server");
        let strategy = resolve_strategy(&classification, "{}", Some(1));
        let json: serde_json::Value =
            serde_json::from_str(&strategy_json(&strategy)).expect("strategy json");

        assert_eq!(json["harness_id"], "mdx_native");
        assert_eq!(json["grants_execution_authority"], false);
    }

    #[test]
    fn callers_without_strategy_mode_keep_the_existing_loop_shape() {
        let classification =
            classify_forge_work("Fix a tiny Forge label", "cargo test -p mdx-server");
        let strategy = resolve_strategy(&classification, r#"{"fleet_width":1}"#, Some(1));

        assert_eq!(strategy.harness_id, "mdx_native");
        assert_eq!(strategy.planner_mode, "bounded");
        assert_eq!(strategy.review_mode, "conditional_independent");
        assert_eq!(strategy.width, 1);
    }
}
