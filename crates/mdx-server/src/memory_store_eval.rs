use mdx_core::{MdxKernel, json_string_literal};

use crate::memory_store::json_string_array;

pub(crate) fn render_evals_json(kernel: &MdxKernel) -> String {
    let source_receipt_ids = json_string_array(
        kernel
            .memory_brain_eval_runs()
            .iter()
            .map(|run| run.receipt_id.as_str())
            .chain(
                kernel
                    .memory_eval_fixture_results()
                    .iter()
                    .map(|fixture| fixture.receipt_id.as_str()),
            ),
    );
    let runs = kernel
        .memory_brain_eval_runs()
        .iter()
        .map(|run| {
            format!(
                r#"{{"eval_run_id":{},"tenant_id":{},"fixture_family":{},"fixture_count":{},"correct_count":{},"latency_budget_ms":{},"observed_latency_ms":{},"brain_score":{},"receipt_id":{}}}"#,
                json_string_literal(&run.eval_run_id),
                json_string_literal(run.tenant_id.as_str()),
                json_string_literal(run.fixture_family),
                run.fixture_count,
                run.correct_count,
                run.latency_budget_ms,
                run.observed_latency_ms,
                run.brain_score,
                json_string_literal(&run.receipt_id)
            )
        })
        .collect::<Vec<_>>();
    let fixtures = kernel
        .memory_eval_fixture_results()
        .iter()
        .map(|fixture| {
            format!(
                r#"{{"fixture_result_id":{},"tenant_id":{},"fixture_family":{},"query":{},"expected_memory_id":{},"matched_memory_id":{},"final_score":{},"passed":{},"receipt_id":{}}}"#,
                json_string_literal(&fixture.fixture_result_id),
                json_string_literal(fixture.tenant_id.as_str()),
                json_string_literal(fixture.fixture_family),
                json_string_literal(&fixture.query),
                json_string_literal(&fixture.expected_memory_id),
                json_string_literal(&fixture.matched_memory_id),
                fixture.final_score,
                fixture.passed,
                json_string_literal(&fixture.receipt_id)
            )
        })
        .collect::<Vec<_>>();
    let brain_score = kernel
        .memory_brain_eval_runs()
        .iter()
        .rev()
        .find(|run| run.fixture_family == "MDx Brain Score")
        .map(|run| run.brain_score)
        .unwrap_or(0);
    format!(
        r#"{{"name":"mdx-memory-brain-evals","status":"LOCAL_EVAL_HARNESS_READY","route":"/memory/brain-evals.json","run_route":"/memory/brain-eval-runs.json","read_only":true,"eval_run_count":{},"fixture_result_count":{},"mdx_brain_score":{},"fixtures":["self_retrieval_smoke","MDx Brain Score"],"fixture_source":"self_retrieval_smoke","storage":"memory_brain_eval_runs","fixture_storage":"memory_eval_fixture_results","source_receipt_ids":[{}],"runs":[{}],"fixture_results":[{}]}}"#,
        runs.len(),
        fixtures.len(),
        brain_score,
        source_receipt_ids,
        runs.join(","),
        fixtures.join(",")
    )
}

pub(crate) fn render_governance_json(kernel: &MdxKernel) -> String {
    let source_receipt_ids = json_string_array(
        kernel
            .memory_surface_access()
            .iter()
            .map(|row| row.receipt_id.as_str()),
    );
    let access = kernel
        .memory_surface_access()
        .iter()
        .map(|row| {
            format!(
                r#"{{"access_id":{},"tenant_id":{},"surface":{},"scope":{},"can_read":{},"can_write":{},"review_required":{},"receipt_id":{}}}"#,
                json_string_literal(&row.access_id),
                json_string_literal(row.tenant_id.as_str()),
                json_string_literal(row.surface),
                json_string_literal(row.scope),
                row.can_read,
                row.can_write,
                row.review_required,
                json_string_literal(&row.receipt_id)
            )
        })
        .collect::<Vec<_>>();
    format!(
        r#"{{"name":"mdx-memory-brain-governance","status":"LOCAL_SCOPE_GOVERNANCE_READY","route":"/memory/governance.json","read_only":true,"scope_count":{},"storage":"memory_surface_access","scopes":["private_user_memory","project_memory","team_memory","company_memory","scoped_memory_port"],"source_receipt_ids":[{}],"access":[{}]}}"#,
        access.len(),
        source_receipt_ids,
        access.join(",")
    )
}

pub(crate) fn render_comparators_json(kernel: &MdxKernel) -> String {
    let source_receipt_ids = json_string_array(
        kernel
            .memory_vendor_comparator_runs()
            .iter()
            .map(|run| run.receipt_id.as_str()),
    );
    let runs = kernel
        .memory_vendor_comparator_runs()
        .iter()
        .map(|run| {
            format!(
                r#"{{"comparator_run_id":{},"tenant_id":{},"vendor_id":{},"status":{},"accuracy_score":{},"latency_ms":{},"cost_micros":{},"receipt_id":{}}}"#,
                json_string_literal(&run.comparator_run_id),
                json_string_literal(run.tenant_id.as_str()),
                json_string_literal(run.vendor_id),
                json_string_literal(run.status),
                run.accuracy_score,
                run.latency_ms,
                run.cost_micros,
                json_string_literal(&run.receipt_id)
            )
        })
        .collect::<Vec<_>>();
    format!(
        r#"{{"name":"mdx-memory-brain-vendor-comparators","status":"LOCAL_COMPARATOR_READY","route":"/memory/vendor-comparators.json","read_only":true,"canonical_vendor":"owned_mdx","comparator_count":{},"storage":"memory_vendor_comparator_runs","source_receipt_ids":[{}],"runs":[{}]}}"#,
        runs.len(),
        source_receipt_ids,
        runs.join(",")
    )
}

pub(crate) fn render_topology_json(kernel: &MdxKernel) -> String {
    let source_receipt_ids = json_string_array(
        kernel
            .memory_production_topology_checks()
            .iter()
            .map(|check| check.receipt_id.as_str())
            .chain(
                kernel
                    .memory_topology_runtime_events()
                    .iter()
                    .map(|event| event.receipt_id.as_str()),
            ),
    );
    let checks = kernel
        .memory_production_topology_checks()
        .iter()
        .map(|check| {
            format!(
                r#"{{"topology_check_id":{},"tenant_id":{},"service":{},"deployment_shape":{},"latency_role":{},"queue_or_cache":{},"observed_latency_ms":{},"receipt_id":{}}}"#,
                json_string_literal(&check.topology_check_id),
                json_string_literal(check.tenant_id.as_str()),
                json_string_literal(check.service),
                json_string_literal(check.deployment_shape),
                json_string_literal(check.latency_role),
                json_string_literal(check.queue_or_cache),
                check.observed_latency_ms,
                json_string_literal(&check.receipt_id)
            )
        })
        .collect::<Vec<_>>();
    let events = kernel
        .memory_topology_runtime_events()
        .iter()
        .map(|event| {
            format!(
                r#"{{"topology_event_id":{},"tenant_id":{},"service":{},"event_kind":{},"queue_or_cache":{},"cache_key":{},"observed_latency_ms":{},"receipt_id":{}}}"#,
                json_string_literal(&event.topology_event_id),
                json_string_literal(event.tenant_id.as_str()),
                json_string_literal(event.service),
                json_string_literal(event.event_kind),
                json_string_literal(event.queue_or_cache),
                json_string_literal(&event.cache_key),
                event.observed_latency_ms,
                json_string_literal(&event.receipt_id)
            )
        })
        .collect::<Vec<_>>();
    format!(
        r#"{{"name":"mdx-memory-brain-topology","status":"LOCAL_TOPOLOGY_READY","route":"/memory/topology.json","validation_route":"/memory/topology-validations.json","read_only":true,"latency_budget_ms":250,"storage":"memory_production_topology_checks","runtime_event_storage":"memory_topology_runtime_events","check_count":{},"runtime_event_count":{},"source_receipt_ids":[{}],"checks":[{}],"runtime_events":[{}]}}"#,
        checks.len(),
        events.len(),
        source_receipt_ids,
        checks.join(","),
        events.join(",")
    )
}

pub(crate) fn render_beta_readiness_json(kernel: &MdxKernel) -> String {
    let source_receipt_ids = json_string_array(
        kernel
            .memory_benchmark_imports()
            .iter()
            .map(|import| import.receipt_id.as_str())
            .chain(
                kernel
                    .memory_scale_load_runs()
                    .iter()
                    .map(|run| run.receipt_id.as_str()),
            )
            .chain(
                kernel
                    .memory_cloud_turn_on_checks()
                    .iter()
                    .map(|check| check.receipt_id.as_str()),
            ),
    );
    let imports = kernel
        .memory_benchmark_imports()
        .iter()
        .map(|import| {
            format!(
                r#"{{"import_id":{},"tenant_id":{},"fixture_family":{},"source_kind":{},"task_shape":{},"fixture_count":{},"synthetic":{},"receipt_id":{}}}"#,
                json_string_literal(&import.import_id),
                json_string_literal(import.tenant_id.as_str()),
                json_string_literal(import.fixture_family),
                json_string_literal(import.source_kind),
                json_string_literal(import.task_shape),
                import.fixture_count,
                import.synthetic,
                json_string_literal(&import.receipt_id)
            )
        })
        .collect::<Vec<_>>();
    let runs = kernel
        .memory_scale_load_runs()
        .iter()
        .map(|run| {
            format!(
                r#"{{"scale_run_id":{},"tenant_id":{},"synthetic_session_count":{},"memory_record_count":{},"ranking_count":{},"latency_budget_ms":{},"observed_p95_latency_ms":{},"brain_score":{},"receipt_id":{}}}"#,
                json_string_literal(&run.scale_run_id),
                json_string_literal(run.tenant_id.as_str()),
                run.synthetic_session_count,
                run.memory_record_count,
                run.ranking_count,
                run.latency_budget_ms,
                run.observed_p95_latency_ms,
                run.brain_score,
                json_string_literal(&run.receipt_id)
            )
        })
        .collect::<Vec<_>>();
    let checks = kernel
        .memory_cloud_turn_on_checks()
        .iter()
        .map(|check| {
            format!(
                r#"{{"check_id":{},"tenant_id":{},"check_kind":{},"status":{},"evidence":{},"receipt_id":{}}}"#,
                json_string_literal(&check.check_id),
                json_string_literal(check.tenant_id.as_str()),
                json_string_literal(check.check_kind),
                json_string_literal(check.status),
                json_string_literal(&check.evidence),
                json_string_literal(&check.receipt_id)
            )
        })
        .collect::<Vec<_>>();
    let latest = kernel.memory_scale_load_runs().last();
    // Cloud turn-on stays false while any check is DECLARED or NOT_IMPORTED:
    // this projection only reports readiness the drill actually verified.
    let ready = latest
        .map(|run| {
            run.synthetic_session_count >= 1000
                && run.memory_record_count >= 1000
                && run.observed_p95_latency_ms <= run.latency_budget_ms
                && kernel
                    .memory_cloud_turn_on_checks()
                    .iter()
                    .all(|check| check.status == "READY")
        })
        .unwrap_or(false);
    format!(
        r#"{{"name":"mdx-memory-brain-beta-readiness","status":{},"route":"/memory/beta-readiness.json","run_route":"/memory/beta-readiness-runs.json","owned_stack_canonical":true,"vendor_dependency_required":false,"target_beta_engineers":1000,"latency_budget_ms":250,"benchmark_import_count":{},"scale_run_count":{},"cloud_turn_on_check_count":{},"ready_for_cloud_turn_on":{},"storage":["memory_benchmark_imports","memory_scale_load_runs","memory_cloud_turn_on_checks"],"source_receipt_ids":[{}],"benchmark_imports":[{}],"scale_runs":[{}],"cloud_turn_on_checks":[{}]}}"#,
        json_string_literal(if latest.is_some() {
            "LOCAL_SYNTHETIC_DRILL_RECORDED"
        } else {
            "PENDING_BETA_READINESS_RUN"
        }),
        imports.len(),
        runs.len(),
        checks.len(),
        ready,
        source_receipt_ids,
        imports.join(","),
        runs.join(","),
        checks.join(",")
    )
}
