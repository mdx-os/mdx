use super::*;

#[test]
fn memory_store_contract_names_local_default_and_vendor_slot() {
    let contracts = memory_store_driver_contracts();
    let local = contracts
        .iter()
        .find(|contract| contract.driver_id == "local_memory_store")
        .expect("local memory driver");
    assert_eq!(local.mode, "local");
    assert!(!local.live_substrate_required);
    assert!(!local.production_write_allowed);
    assert_eq!(local.provider, "InMemoryProvider");
    assert_eq!(local.durable_driver, "postgres_memory_records");
    assert_eq!(local.durable_table, "memory_records");
    assert_eq!(local.durable_status, "POSTGRES_DURABLE_MEMORY_SHAPE");
    assert_eq!(local.status, "LIVE-LOCAL-CONFORMANCE");
    assert!(!local.live_database_write_allowed);
    assert_eq!(local.receipt_driver_field, "memory_driver");

    let vendor = contracts
        .iter()
        .find(|contract| contract.driver_id == "mem0_memory_store")
        .expect("vendor memory slot");
    assert_eq!(vendor.provider, "Mem0MemoryProvider");
    assert_eq!(vendor.status, "PENDING-LIVE-RUN");
    assert!(vendor.live_substrate_required);
}

#[test]
fn memory_store_postgres_export_renders_memory_records_without_live_write() {
    let mut kernel = MdxKernel::boot_local();
    kernel.run_evals_runner_agent().expect("loop run");
    kernel
        .run_memory_brain_eval_harness(&local_correlation(), "postgres export eval proof")
        .expect("eval harness");
    kernel
        .evaluate_memory_lifecycle(&local_correlation(), "postgres export lifecycle proof")
        .expect("lifecycle evaluation");
    kernel
        .run_memory_topology_validation(&local_correlation(), "postgres export topology proof")
        .expect("topology validation");
    let sql = render_postgres_memory_export_sql(&kernel);

    assert!(sql.contains("-- mdx MemoryStore local export proof"));
    assert!(sql.contains("-- memory_episode episode_id=episode_memory_"));
    assert!(sql.contains("scope=agent_operational_memory"));
    assert!(sql.contains("tier=episodic"));
    assert!(sql.contains("INSERT INTO memory_records"));
    assert!(sql.contains("ON CONFLICT (memory_id) DO UPDATE SET"));
    assert!(sql.contains("SELECT count(*) FROM memory_records"));
    assert!(sql.contains("INSERT INTO memory_graph_nodes"));
    assert!(sql.contains("INSERT INTO memory_graph_edges"));
    assert!(sql.contains("INSERT INTO memory_lifecycle_events"));
    assert!(sql.contains("INSERT INTO memory_lifecycle_evaluations"));
    assert!(sql.contains("INSERT INTO memory_recall_rankings"));
    assert!(sql.contains("INSERT INTO memory_brain_eval_runs"));
    assert!(sql.contains("INSERT INTO memory_eval_fixture_results"));
    assert!(sql.contains("INSERT INTO memory_vendor_comparator_runs"));
    assert!(sql.contains("INSERT INTO memory_surface_access"));
    assert!(sql.contains("INSERT INTO memory_production_topology_checks"));
    assert!(sql.contains("INSERT INTO memory_topology_runtime_events"));
    assert!(sql.contains("SELECT count(*) FROM memory_graph_nodes"));
    assert!(sql.contains("SELECT count(*) FROM memory_recall_rankings"));
    assert!(sql.contains("'RETAIN'"));
    assert_eq!(sql.matches("INSERT INTO memory_records").count(), 1);
    assert!(sql.matches("INSERT INTO memory_graph_nodes").count() >= 4);
    assert!(sql.matches("INSERT INTO memory_recall_rankings").count() >= 1);
}

#[test]
fn memory_brain_snapshot_restores_records_graph_lifecycle_rankings_and_evals() {
    let mut kernel = MdxKernel::boot_local();
    kernel.run_evals_runner_agent().expect("loop run");
    kernel
        .run_memory_brain_eval_harness(&local_correlation(), "snapshot replay proof")
        .expect("eval harness");
    kernel
        .evaluate_memory_lifecycle(&local_correlation(), "snapshot lifecycle proof")
        .expect("lifecycle evaluation");
    kernel
        .run_memory_topology_validation(&local_correlation(), "snapshot topology proof")
        .expect("topology validation");
    let ledger = kernel.ledger().entries().to_vec();
    let snapshot = kernel.memory_brain_snapshot();

    assert_eq!(snapshot.records.len(), 1);
    assert!(snapshot.graph_nodes.len() >= 4);
    assert!(snapshot.graph_edges.len() >= 4);
    assert!(!snapshot.lifecycle_events.is_empty());
    assert!(!snapshot.recall_rankings.is_empty());
    assert!(!snapshot.eval_runs.is_empty());
    assert!(!snapshot.lifecycle_evaluations.is_empty());
    assert!(!snapshot.eval_fixture_results.is_empty());
    assert!(!snapshot.topology_runtime_events.is_empty());

    let mut restored = MdxKernel::boot_local();
    restored
        .restore_ledger_entries(ledger)
        .expect("ledger restores before memory");
    let restored_count = restored
        .restore_memory_brain_snapshot(snapshot.clone())
        .expect("memory brain restores with receipt evidence");

    assert_eq!(restored_count, snapshot.records.len());
    assert_eq!(restored.memory_records(), snapshot.records.as_slice());
    assert_eq!(
        restored.memory_graph_nodes(),
        snapshot.graph_nodes.as_slice()
    );
    assert_eq!(
        restored.memory_graph_edges(),
        snapshot.graph_edges.as_slice()
    );
    assert_eq!(
        restored.memory_lifecycle_events(),
        snapshot.lifecycle_events.as_slice()
    );
    assert_eq!(
        restored.memory_recall_rankings(),
        snapshot.recall_rankings.as_slice()
    );
    assert_eq!(
        restored.memory_brain_eval_runs(),
        snapshot.eval_runs.as_slice()
    );
    assert_eq!(
        restored.memory_lifecycle_evaluations(),
        snapshot.lifecycle_evaluations.as_slice()
    );
    assert_eq!(
        restored.memory_eval_fixture_results(),
        snapshot.eval_fixture_results.as_slice()
    );
    assert_eq!(
        restored.memory_topology_runtime_events(),
        snapshot.topology_runtime_events.as_slice()
    );
    assert!(
        render_postgres_memory_export_sql(&restored).contains("INSERT INTO memory_graph_nodes")
    );
}

#[test]
fn memory_brain_snapshot_restore_refuses_record_citing_missing_receipt() {
    let mut kernel = MdxKernel::boot_local();
    kernel.run_evals_runner_agent().expect("loop run");
    let source_receipt = kernel
        .ledger()
        .entries()
        .last()
        .expect("source receipt")
        .receipt_id
        .clone();
    kernel
        .record_surface_memory_local(
            "local_tenant",
            "human:local_user",
            "twin",
            "private_user_memory",
            &source_receipt,
            "Durable memory must stay receipt-backed",
        )
        .expect("surface memory");
    let ledger = kernel.ledger().entries().to_vec();

    // A record edited to cite a receipt the restored ledger never held
    // refuses the whole snapshot and applies nothing.
    let mut tampered = kernel.memory_brain_snapshot();
    tampered.records[0].source_receipt_id = "receipt_never_written_000001".to_string();
    let mut restored = MdxKernel::boot_local();
    restored
        .restore_ledger_entries(ledger)
        .expect("ledger restores before memory");
    let error = restored
        .restore_memory_brain_snapshot(tampered)
        .expect_err("record citing a missing receipt must refuse");
    assert!(error.contains("receipt_never_written_000001"), "{error}");
    assert!(error.contains("missing"), "{error}");
    assert!(
        restored.memory_records().is_empty(),
        "refused restore must not partially apply"
    );
    assert!(restored.memory_graph_nodes().is_empty());

    // Even an untampered snapshot refuses against a kernel whose ledger
    // does not hold the cited receipts.
    let mut empty = MdxKernel::boot_local();
    let error = empty
        .restore_memory_brain_snapshot(kernel.memory_brain_snapshot())
        .expect_err("snapshot must refuse without its ledger evidence");
    assert!(error.contains("missing"), "{error}");
    assert!(empty.memory_records().is_empty());
}

#[test]
fn lifecycle_evaluation_supersedes_same_subject_with_trusted_time_and_validity_window() {
    let mut kernel = MdxKernel::boot_local();
    kernel.run_evals_runner_agent().expect("loop run");
    let first_source = kernel
        .ledger()
        .entries()
        .last()
        .expect("source receipt")
        .receipt_id
        .clone();
    let first_memory = kernel
        .record_surface_memory_local(
            "local_tenant",
            "human:local_user",
            "twin",
            "private_user_memory",
            &first_source,
            "User prefers concise planning",
        )
        .expect("first surface memory");
    let second_source = kernel
        .ledger()
        .entries()
        .last()
        .expect("second source receipt")
        .receipt_id
        .clone();
    kernel
        .record_surface_memory_local(
            "local_tenant",
            "human:local_user",
            "twin",
            "private_user_memory",
            &second_source,
            "User does not prefer concise planning",
        )
        .expect("second surface memory");

    let evaluation = kernel
        .evaluate_memory_lifecycle(&local_correlation(), "trusted supersession test")
        .expect("lifecycle evaluation");

    // Keyword contradiction detection is retired; a newer same-subject
    // record supersedes the older one and closes its validity window.
    assert!(evaluation.supersession_count >= 1);
    assert_eq!(evaluation.contradiction_count, 0);
    assert!(
        kernel
            .memory_lifecycle_events()
            .iter()
            .any(|event| event.memory_id == first_memory
                && event.lifecycle_state == "superseded"
                && !event.valid_from_receipt_timestamp.is_empty())
    );
    let superseded = kernel
        .memory_records()
        .iter()
        .find(|record| record.memory_id == first_memory)
        .expect("superseded record stays stored");
    assert_eq!(
        superseded.valid_until_receipt_timestamp,
        evaluation.trusted_time_floor
    );
    assert_eq!(
        superseded.invalidated_by_receipt_id,
        evaluation.trigger_receipt_id
    );
    assert!(
        kernel
            .memory_graph_nodes()
            .iter()
            .any(
                |node| node.memory_id.as_deref() == Some(first_memory.as_str())
                    && node.lifecycle_state == "superseded"
            )
    );
    assert!(
        kernel
            .memory_graph_edges()
            .iter()
            .any(|edge| edge.edge_kind == "SUPERSEDES")
    );
}

#[test]
fn eval_harness_records_fixture_results_and_offline_vendor_comparators() {
    let mut kernel = MdxKernel::boot_local();
    kernel.run_evals_runner_agent().expect("loop run");

    let runs = kernel
        .run_memory_brain_eval_harness(&local_correlation(), "measured eval proof")
        .expect("eval harness");

    assert!(
        runs.iter()
            .any(|run| run.fixture_family == "self_retrieval_smoke")
    );
    assert!(
        runs.iter()
            .any(|run| run.fixture_family == "MDx Brain Score")
    );
    assert!(!kernel.memory_eval_fixture_results().is_empty());
    assert!(
        kernel
            .memory_eval_fixture_results()
            .iter()
            .all(|fixture| fixture.fixture_family == "self_retrieval_smoke"
                && !fixture.expected_memory_id.is_empty()
                && fixture.final_score > 0)
    );
    assert!(
        kernel
            .memory_vendor_comparator_runs()
            .iter()
            .any(|run| run.vendor_id == "owned_mdx"
                && run.status == "CANONICAL_LOCAL_RUN"
                && run.cost_micros == 0)
    );
    assert!(
        kernel
            .memory_vendor_comparator_runs()
            .iter()
            .filter(|run| run.vendor_id != "owned_mdx")
            .all(|run| run.status == "OFFLINE_COMPARATOR_ADAPTER"
                && run.accuracy_score == 0
                && run.latency_ms == 0
                && run.cost_micros == 0)
    );
    assert!(
        render_postgres_memory_export_sql(&kernel)
            .contains("INSERT INTO memory_eval_fixture_results")
    );
}

#[test]
fn topology_validation_records_queue_cache_runtime_events() {
    let mut kernel = MdxKernel::boot_local();
    kernel.run_evals_runner_agent().expect("loop run");

    let events = kernel
        .run_memory_topology_validation(&local_correlation(), "queue cache proof")
        .expect("topology validation");

    assert_eq!(events.len(), 4);
    assert!(
        events
            .iter()
            .any(|event| event.event_kind == "enqueue_consolidation_job")
    );
    assert!(
        events
            .iter()
            .any(|event| event.event_kind == "cache_hit_recall_packet")
    );
    assert!(events.iter().all(|event| event.observed_latency_ms == 0));
    assert!(
        render_postgres_memory_export_sql(&kernel)
            .contains("INSERT INTO memory_topology_runtime_events")
    );
}

#[test]
fn recall_ranking_respects_shared_scope_read_boundaries() {
    let mut kernel = MdxKernel::boot_local();
    kernel.run_evals_runner_agent().expect("loop run");
    let source_receipt = kernel
        .ledger()
        .entries()
        .last()
        .expect("source receipt")
        .receipt_id
        .clone();
    let team_memory = kernel
        .record_surface_memory_local(
            "local_tenant",
            "human:local_user",
            "messages",
            "team_memory",
            &source_receipt,
            "Team decision is launch memory in Messages",
        )
        .expect("team memory write");

    let rankings = kernel.record_memory_recall_ranking(
        &local_correlation(),
        "twin",
        "Team decision is launch memory in Messages",
        "private_user_memory",
        &source_receipt,
    );

    assert!(
        rankings
            .iter()
            .all(|ranking| ranking.memory_id != team_memory),
        "Twin private recall should not rank team memory without a matching access row"
    );
}

#[test]
fn recall_ranking_never_scores_another_tenants_records() {
    let mut kernel = MdxKernel::boot_local();
    kernel.run_evals_runner_agent().expect("loop run");
    let source_receipt = kernel
        .ledger()
        .entries()
        .last()
        .expect("source receipt")
        .receipt_id
        .clone();
    let ours = kernel
        .record_surface_memory_local(
            "tenant_a",
            "human:tenant_a_user",
            "twin",
            "private_user_memory",
            &source_receipt,
            "Tenant A prefers renewable strategy memos",
        )
        .expect("tenant a memory");
    let theirs = kernel
        .record_surface_memory_local(
            "tenant_b",
            "human:tenant_b_user",
            "twin",
            "private_user_memory",
            &source_receipt,
            "Tenant B prefers renewable strategy memos",
        )
        .expect("tenant b memory");
    // Read-only path: tenant A's ranking sees only tenant A's records.
    let candidates = kernel.rank_memory_recall(
        "twin",
        "renewable strategy memos",
        "private_user_memory",
        "tenant_a",
    );
    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.memory_id == ours)
    );
    assert!(
        candidates
            .iter()
            .all(|candidate| candidate.memory_id != theirs)
    );
    // Persisting path: the same tenant filter holds.
    let correlation = CorrelationIds {
        tenant_id: TenantId::new("tenant_a"),
        trace_id: TraceId::new("trace_tenant_a"),
        actor_id: ActorId::new("actor_tenant_a"),
        loop_id: LoopId::new("loop_tenant_a"),
        workflow_id: WorkflowId::new("workflow_tenant_a"),
    };
    let rankings = kernel.record_memory_recall_ranking(
        &correlation,
        "twin",
        "renewable strategy memos",
        "private_user_memory",
        &source_receipt,
    );
    assert!(rankings.iter().any(|ranking| ranking.memory_id == ours));
    assert!(rankings.iter().all(|ranking| ranking.memory_id != theirs));
}

#[test]
fn beta_readiness_generates_1000_session_scale_proof() {
    let mut kernel = MdxKernel::boot_local();
    kernel.run_evals_runner_agent().expect("loop run");

    let run = kernel
        .run_memory_beta_readiness(&local_correlation(), 1000)
        .expect("beta readiness");

    assert_eq!(run.synthetic_session_count, 1000);
    assert_eq!(run.memory_record_count, 1000);
    assert_eq!(run.latency_budget_ms, 250);
    let imports = kernel.memory_benchmark_imports();
    assert_eq!(imports.len(), 4);
    assert!(imports.iter().all(|import| {
        import.fixture_family == "synthetic_placeholder"
            && import.source_kind == "synthetic_placeholder"
            && import.fixture_count == 0
            && import.synthetic
    }));
    let checks = kernel.memory_cloud_turn_on_checks();
    assert!(checks.iter().any(|check| {
        check.check_kind == "benchmark_fixture_import" && check.status == "NOT_IMPORTED"
    }));
    assert!(checks.iter().any(|check| {
        check.check_kind == "local_postgres_persistence" && check.status == "DECLARED"
    }));
    assert!(
        checks
            .iter()
            .any(|check| check.check_kind == "cloud_turn_on" && check.status == "DECLARED")
    );
    let expected_latency_status = if run.observed_p95_latency_ms <= run.latency_budget_ms {
        "READY"
    } else {
        "BLOCKED"
    };
    assert!(checks.iter().any(|check| {
        check.check_kind == "scale_latency_budget" && check.status == expected_latency_status
    }));
    assert!(
        render_postgres_memory_export_sql(&kernel).contains("INSERT INTO memory_scale_load_runs")
    );
}

#[test]
fn cosine_semantic_ranking_beats_checksum_on_a_lexically_different_match() {
    let mut kernel = MdxKernel::boot_local();
    kernel.run_evals_runner_agent().expect("loop run");
    let source_receipt = kernel
        .ledger()
        .entries()
        .last()
        .expect("source receipt")
        .receipt_id
        .clone();
    // Two private records. The relevant one shares NO tokens with the query
    // (feline/napped/windowsill vs cat/sleeping/sun), so lexical and checksum
    // scoring cannot know it is the better match. Every other component is
    // equal between the two records, so the semantic slot decides the winner.
    let relevant = kernel
        .record_surface_memory_local(
            "local_tenant",
            "human:local_user",
            "twin",
            "private_user_memory",
            &source_receipt,
            "The feline napped on the windowsill",
        )
        .expect("relevant write");
    let unrelated = kernel
        .record_surface_memory_local(
            "local_tenant",
            "human:local_user",
            "twin",
            "private_user_memory",
            &source_receipt,
            "Quarterly revenue projections spreadsheet",
        )
        .expect("unrelated write");

    // Craft embeddings: the query points almost exactly at the relevant
    // record and orthogonally to the unrelated one.
    kernel.set_memory_embedding(&relevant, vec![1.0, 0.0, 0.0]);
    kernel.set_memory_embedding(&unrelated, vec![0.0, 0.0, 1.0]);
    let query = "cat sleeping in the sun";
    let query_embedding = vec![0.95, 0.05, 0.0];

    let semantic = kernel.rank_memory_recall_with_embedding(
        "twin",
        query,
        &query_embedding,
        "private_user_memory",
        "local_tenant",
    );
    let top = semantic.first().expect("a ranked candidate");
    assert_eq!(
        top.memory_id, relevant,
        "the embedding-backed semantic component must rank the lexically-different but semantically-close record first"
    );

    // Fail-closed: with no query embedding the ranker falls back to the
    // lexical and checksum path, exactly today's behavior, and still returns
    // a ranking without erroring.
    let lexical = kernel.rank_memory_recall("twin", query, "private_user_memory", "local_tenant");
    assert_eq!(lexical.len(), semantic.len());
    assert!(
        lexical
            .iter()
            .any(|candidate| candidate.memory_id == relevant)
    );
    assert!(
        lexical
            .iter()
            .any(|candidate| candidate.memory_id == unrelated)
    );
}

#[test]
fn memory_snapshot_round_trips_a_stored_embedding() {
    let mut kernel = MdxKernel::boot_local();
    kernel.run_evals_runner_agent().expect("loop run");
    let source_receipt = kernel
        .ledger()
        .entries()
        .last()
        .expect("source receipt")
        .receipt_id
        .clone();
    let memory_id = kernel
        .record_surface_memory_local(
            "local_tenant",
            "human:local_user",
            "twin",
            "private_user_memory",
            &source_receipt,
            "Vectorized memory survives a restart",
        )
        .expect("surface memory");
    kernel.set_memory_embedding(&memory_id, vec![0.125_5, -0.5, 0.75]);

    // The durable snapshot carries the serialized vector, and a restore into a
    // fresh kernel reproduces it byte for byte (records compare equal).
    let snapshot = kernel.memory_brain_snapshot();
    let stored = snapshot
        .records
        .iter()
        .find(|record| record.memory_id == memory_id)
        .expect("record in snapshot");
    assert!(!stored.embedding.is_empty(), "embedding must be serialized");

    let ledger = kernel.ledger().entries().to_vec();
    let mut restored = MdxKernel::boot_local();
    restored
        .restore_ledger_entries(ledger)
        .expect("ledger restores before memory");
    restored
        .restore_memory_brain_snapshot(snapshot.clone())
        .expect("memory restores");
    let restored_record = restored
        .memory_records()
        .iter()
        .find(|record| record.memory_id == memory_id)
        .expect("restored record");
    assert_eq!(restored_record.embedding, stored.embedding);
    assert_eq!(restored.memory_records(), kernel.memory_records());
}
