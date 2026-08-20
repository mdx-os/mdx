use std::process::Command;

fn mdx_server() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mdx-server"))
}

#[test]
fn run_loop_cli_completes_evals_runner() {
    let output = mdx_server()
        .args(["run-loop", "evals_runner_agent"])
        .output()
        .expect("run loop command");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("status: COMPLETED"));
    assert!(stdout.contains("credential_status: MINTED"));
    assert!(stdout.contains("receipt_count: 14"));
    assert!(stdout.contains("Source receipts:"));
}

#[test]
fn run_loop_cli_completes_aegis_scanner() {
    let output = mdx_server()
        .args(["run-loop", "aegis_scanner_agent"])
        .output()
        .expect("aegis loop command");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("loop_id: aegis_scanner_agent"));
    assert!(stdout.contains("status: COMPLETED"));
    assert!(stdout.contains("credential_status: NOT_APPLICABLE"));
    assert!(stdout.contains("receipt_count: 10"));
    assert!(stdout.contains("aegis_local_finding_001"));
}

#[test]
fn run_loop_cli_completes_charter_attestation() {
    let output = mdx_server()
        .args(["run-loop", "charter_attestation_agent"])
        .output()
        .expect("charter loop command");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("loop_id: charter_attestation_agent"));
    assert!(stdout.contains("status: COMPLETED"));
    assert!(stdout.contains("credential_status: NOT_APPLICABLE"));
    assert!(stdout.contains("receipt_count: 10"));
    assert!(stdout.contains("charter_attestation_agent checked a declared obligation"));
}

#[test]
fn run_loop_cli_blocks_forge_orchestrator_at_talent_boundary() {
    let output = mdx_server()
        .args(["run-loop", "forge_orchestrator_agent"])
        .output()
        .expect("forge loop command");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("loop_id: forge_orchestrator_agent"));
    assert!(stdout.contains("status: BLOCKED_ON_TALENT_AUTHORITY"));
    assert!(stdout.contains("credential_status: NOT_APPLICABLE"));
    assert!(stdout.contains("receipt_count: 10"));
    assert!(stdout.contains("blocked before worker spawn"));
}

#[test]
fn run_loop_cli_blocks_talent_autonomy_at_live_worker_execution() {
    let output = mdx_server()
        .args(["run-loop", "talent_autonomy_agent"])
        .output()
        .expect("talent loop command");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("loop_id: talent_autonomy_agent"));
    assert!(stdout.contains("status: BLOCKED_ON_LIVE_WORKER_EXECUTION"));
    assert!(stdout.contains("credential_status: CHECKED_LOCAL_AUTHORITY"));
    assert!(stdout.contains("receipt_count: 18"));
    assert!(stdout.contains("blocked before live worker execution"));
}

#[test]
fn run_loop_cli_blocks_product_shaping_at_human_ratification() {
    let output = mdx_server()
        .args(["run-loop", "product_shaping_agent"])
        .output()
        .expect("product shaping loop command");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("loop_id: product_shaping_agent"));
    assert!(stdout.contains("status: BLOCKED_ON_HUMAN_PRODUCT_RATIFICATION"));
    assert!(stdout.contains("credential_status: NOT_APPLICABLE"));
    assert!(stdout.contains("receipt_count: 10"));
    assert!(stdout.contains("human ratification"));
}

#[test]
fn run_local_loops_cli_completes_declared_loop_set() {
    let output = mdx_server()
        .arg("run-local-loops")
        .output()
        .expect("run local loops command");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("loop_id: evals_runner_agent"));
    assert!(stdout.contains("loop_id: aegis_scanner_agent"));
    assert!(stdout.contains("loop_id: charter_attestation_agent"));
    assert!(stdout.contains("loop_id: forge_orchestrator_agent"));
    assert!(stdout.contains("loop_id: product_shaping_agent"));
    assert!(stdout.contains("loop_id: talent_autonomy_agent"));
    assert!(stdout.contains("receipt_count: 14"));
    assert_eq!(stdout.matches("receipt_count: 10").count(), 4);
    assert!(stdout.contains("receipt_count: 18"));
    assert_eq!(stdout.matches("status: COMPLETED").count(), 3);
    assert_eq!(
        stdout
            .matches("status: BLOCKED_ON_TALENT_AUTHORITY")
            .count(),
        1
    );
    assert_eq!(
        stdout
            .matches("status: BLOCKED_ON_HUMAN_PRODUCT_RATIFICATION")
            .count(),
        1
    );
    assert_eq!(
        stdout
            .matches("status: BLOCKED_ON_LIVE_WORKER_EXECUTION")
            .count(),
        1
    );
}

#[test]
fn check_migrations_cli_reports_contract_counts() {
    let output = mdx_server()
        .arg("check-migrations")
        .output()
        .expect("migration command");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("migrations: 48"));
    assert!(stdout.contains("rls_enabled_tables: 128"));
    assert!(stdout.contains("policy_definitions: 180"));
    assert!(stdout.contains("policy_drop_guards: 180"));
}

#[test]
fn check_postgres_boundary_reports_pending_without_live_evidence() {
    let output = mdx_server()
        .arg("check-postgres-boundary")
        .output()
        .expect("postgres boundary command");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("postgres_boundary: PENDING-LIVE-RUN"));
    assert!(stdout.contains("migrations: 48"));
    assert!(stdout.contains("rls_enabled_tables: 128"));
    assert!(stdout.contains("policy_drop_guards: 180"));
}

#[test]
fn local_loop_budgets_cli_reports_declared_receipt_counts() {
    let output = mdx_server()
        .arg("local-loop-budgets")
        .output()
        .expect("local loop budgets command");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("evals_runner_agent\t14"));
    assert!(stdout.contains("aegis_scanner_agent\t10"));
    assert!(stdout.contains("charter_attestation_agent\t10"));
    assert!(stdout.contains("forge_orchestrator_agent\t10"));
    assert!(stdout.contains("product_shaping_agent\t10"));
    assert!(stdout.contains("talent_autonomy_agent\t18"));
}

#[test]
fn export_loop_ledger_sql_emits_postgres_receipt_inserts() {
    let output = mdx_server()
        .args(["export-loop-ledger-sql", "evals_runner_agent"])
        .output()
        .expect("export loop ledger sql command");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("INSERT INTO tenants"));
    assert!(stdout.contains("INSERT INTO actors"));
    assert!(stdout.contains("INSERT INTO ledger_entries"));
    assert!(stdout.contains("INSERT INTO ledger_chain_heads"));
    assert!(stdout.contains("ON CONFLICT (tenant_id) DO UPDATE SET"));
    assert!(stdout.contains("'evals_runner_agent'"));
    assert!(stdout.contains("SELECT count(*) FROM ledger_entries"));
    assert_eq!(stdout.matches("INSERT INTO ledger_entries").count(), 14);
    assert_eq!(stdout.matches("INSERT INTO ledger_chain_heads").count(), 1);
}

#[test]
fn export_postgres_storage_write_sql_requires_observed_migrations() {
    let output = mdx_server()
        .args(["export-postgres-storage-write-sql", "evals_runner_agent"])
        .output()
        .expect("export postgres storage write sql command");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stderr.contains("MDX_POSTGRES_MIGRATIONS_OBSERVED=1 is required"));
}

#[test]
fn export_memory_store_sql_emits_local_memory_records_without_live_write() {
    let output = mdx_server()
        .arg("export-memory-store-sql")
        .output()
        .expect("export memory store sql command");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("-- mdx MemoryStore local export proof"));
    assert!(stdout.contains("INSERT INTO memory_records"));
    assert!(stdout.contains("ON CONFLICT (memory_id) DO UPDATE SET"));
    assert!(stdout.contains("SELECT count(*) FROM memory_records"));
    assert_eq!(stdout.matches("INSERT INTO memory_records").count(), 1001);
    assert!(
        stdout.contains("1000 synthetic sessions ranked under local beta latency budget"),
        "export should include the Memory Brain beta readiness scale proof"
    );
}

#[test]
fn export_app_state_sql_emits_governed_app_state_without_live_write() {
    let output = mdx_server()
        .arg("export-app-state-sql")
        .output()
        .expect("export app-state sql command");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    for expected in [
        "-- mdx governed app-state local export proof",
        "INSERT INTO ledger_entries",
        "INSERT INTO memory_records",
        "INSERT INTO twin_sessions",
        "INSERT INTO twin_session_messages",
        "INSERT INTO twin_memory_snapshots",
        "INSERT INTO twin_model_traces",
        "INSERT INTO auth_tenant_orgs",
        "INSERT INTO auth_tenant_memberships",
        "INSERT INTO auth_role_mappings",
        "INSERT INTO auth_invite_states",
        "INSERT INTO auth_visibility_policies",
        "INSERT INTO auth_approved_model_policies",
        "INSERT INTO auth_session_evidence",
        "INSERT INTO auth_tenant_policy_preflights",
        "INSERT INTO message_threads",
        "INSERT INTO message_thread_messages",
        "INSERT INTO message_channels",
        "INSERT INTO message_channel_members",
        "INSERT INTO message_thread_participants",
        "INSERT INTO message_envelopes",
        "INSERT INTO message_realtime_cutover_preflights",
        "INSERT INTO message_delivery_replay_batches",
        "INSERT INTO message_subscription_isolation_checks",
        "INSERT INTO message_service_role_fanout_refusals",
        "INSERT INTO pages_documents",
        "INSERT INTO pages_revisions",
        "INSERT INTO pages_publications",
        "INSERT INTO pages_publication_targets",
        "INSERT INTO pages_revision_citations",
        "INSERT INTO pages_attachments",
        "INSERT INTO pages_search_index_records",
        "SELECT count(*) FROM pages_publications",
        "SELECT count(*) FROM pages_search_index_records",
        "SELECT count(*) FROM auth_tenant_policy_preflights",
        "SELECT count(*) FROM message_realtime_cutover_preflights",
    ] {
        assert!(stdout.contains(expected), "{expected}");
    }
}

#[test]
fn write_postgres_app_state_requires_observed_migrations() {
    let output = mdx_server()
        .arg("write-postgres-app-state")
        .output()
        .expect("write postgres app-state command");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stderr.contains("MDX_POSTGRES_MIGRATIONS_OBSERVED=1 is required"));
}

#[test]
fn export_postgres_storage_write_sql_emits_adapter_authored_inserts() {
    let output = mdx_server()
        .env("DATABASE_URL", "postgres://mdx:mdx@localhost/mdx")
        .env("MDX_POSTGRES_MIGRATIONS_OBSERVED", "1")
        .args(["export-postgres-storage-write-sql", "evals_runner_agent"])
        .output()
        .expect("export postgres storage write sql command");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("INSERT INTO ledger_entries"));
    assert!(stdout.contains("INSERT INTO ledger_chain_heads"));
    assert!(stdout.contains("'evals_runner_agent'"));
    assert!(stdout.contains("SELECT count(*) FROM ledger_entries"));
    assert_eq!(stdout.matches("INSERT INTO ledger_entries").count(), 14);
    assert_eq!(stdout.matches("INSERT INTO ledger_chain_heads").count(), 1);
}

#[test]
fn write_postgres_storage_receipts_requires_observed_migrations() {
    let output = mdx_server()
        .args(["write-postgres-storage-receipts", "evals_runner_agent"])
        .output()
        .expect("write postgres storage receipts command");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stderr.contains("MDX_POSTGRES_MIGRATIONS_OBSERVED=1 is required"));
}

#[test]
fn run_loop_postgres_requires_observed_migrations() {
    let output = mdx_server()
        .args(["run-loop-postgres", "evals_runner_agent"])
        .output()
        .expect("run loop postgres command");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stderr.contains("MDX_POSTGRES_MIGRATIONS_OBSERVED=1 is required"));
}

#[test]
fn status_cli_reports_each_live_substrate_boundary() {
    let output = mdx_server().arg("status").output().expect("status command");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    for substrate in [
        "postgres",
        "temporal",
        "tensorzero",
        "mem0",
        "opentelemetry",
        "render",
    ] {
        assert!(stdout.contains(substrate), "{substrate}");
    }
    assert_eq!(stdout.matches("PENDING-LIVE-RUN").count(), 5);
    assert!(stdout.contains("postgres: LIVE-LOCAL"));
}

#[test]
fn status_json_cli_reports_machine_readable_boundaries() {
    let output = mdx_server()
        .args(["status", "--json"])
        .output()
        .expect("status json command");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("\"mode\": \"deterministic-local\""));
    assert!(stdout.contains("\"live_substrate\""));
    assert!(stdout.contains("\"substrate\": \"opentelemetry\""));
    assert!(stdout.contains("\"live_adapter\": \"OpenTelemetryExporter\""));
    assert_eq!(
        stdout.matches("\"status\": \"PENDING-LIVE-RUN\"").count(),
        5
    );
    assert!(stdout.contains("\"status\": \"LIVE-LOCAL\""));
}

#[test]
fn unknown_loop_fails_closed() {
    let output = mdx_server()
        .args(["run-loop", "unknown_agent"])
        .output()
        .expect("unknown loop command");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stderr.contains("unknown loop unknown_agent"));
}
