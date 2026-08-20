use args::{enabled_loop_ids, parse_usize_arg};
use forge_fleet_eval_scoreboard::render_forge_fleet_eval_scoreboard_json;
use local_evidence::*;
use loop_report_render::render_report;
use mdx_core::*;
use read_surfaces::*;
use receipt_routes::{render_receipt_json, render_receipts_json};
use status::{render_status_json, render_status_text};
use std::{io::Write, net::TcpListener, net::TcpStream, sync::Arc, sync::RwLock};
mod activation_deliberation_stream;
mod activation_route;
mod app_state_export;
mod args;
mod auth_session_route;
mod auth_tenant_policy;
mod auth_verifier;
mod autonomy_envelope_route;
mod beta_auth_bootstrap;
mod beta_program_route;
mod capability_execution_route;
mod changelog_route;
mod connector_route;
mod deployment;
mod evidence_anchor;
mod evidence_checkpoint_route;
mod evidence_signing;
mod feedback;
mod feedback_autonomy_route;
mod fleet_executor;
mod fleet_integration;
mod fleet_plan_route;
mod fleet_run_conductor;
mod fleet_run_route;
mod forge_apply_patch;
mod forge_bench_cli;
mod forge_browser_route;
mod forge_builder_loop_route;
mod forge_candidate_selection_route;
mod forge_control_plane_projection;
mod forge_diff_classify;
mod forge_diff_route;
mod forge_exec_sandbox;
mod forge_execution_posture;
mod forge_external_harness_runner;
mod forge_finish_evaluator;
mod forge_fleet_eval_scoreboard;
mod forge_flywheel_proof_route;
mod forge_long_horizon_mission;
mod forge_loop_runner;
mod forge_model_pricing;
mod forge_model_provider_routing;
mod forge_model_scorecard_route;
mod forge_outcome_signal_route;
mod forge_pr_handoff_route;
mod forge_recipes_route;
mod forge_repo_connect_route;
mod forge_repo_onboarding_packet_route;
mod forge_repo_profile;
mod forge_repo_readiness_route;
mod forge_repo_route;
mod forge_repo_standards_packet_route;
mod forge_repo_task_scout_route;
mod forge_review_packet;
mod forge_review_panel_route;
mod forge_revise_route;
mod forge_run_control_route;
mod forge_run_route;
mod forge_run_ship_route;
mod forge_run_strategy;
mod forge_run_stream;
mod forge_self_delivery;
mod forge_semantic_lsp;
mod forge_semantic_query_route;
mod forge_ship_decision;
mod forge_source_host_live_delivery_route;
mod forge_source_host_pr_draft_route;
mod forge_source_host_readiness_route;
mod forge_startup_reconciler;
mod forge_subagent;
mod forge_transcript_store;
mod forge_turn_client;
mod forge_work_classification;
mod forge_workspace_checkpoint;
mod harness_ci_evidence_adapter;
mod harness_patch_applier;
mod harness_sandbox_runner;
mod harness_worker_workspace;
mod http_rate_limit;
mod http_read;
mod http_response;
mod http_telemetry;
mod install_first_run_profile;
mod install_model_connect;
mod install_owner;
mod install_setup_track;
mod kernel_snapshot;
mod learning_routes;
mod local_evidence;
mod loop_report_render;
mod marketplace;
mod marketplace_skill_route;
mod memory_backfill;
mod memory_benchmark;
mod memory_consolidation_ratify_route;
mod memory_embedding;
mod memory_extraction;
mod memory_store;
mod memory_store_eval;
mod memory_store_graph;
mod memory_store_runtime;
mod message_action;
mod message_activity_route;
mod message_bridge_route;
mod message_channel_route;
mod message_controls;
mod message_fanout_request;
mod message_presence_request;
mod message_presence_route;
mod message_realtime_cutover_preflight;
mod message_realtime_readiness;
mod message_relay_observation;
mod message_routes;
mod message_thread_message;
mod mobile_app_attest;
mod mobile_cloud_route;
mod mobile_command_route;
mod mobile_hosted_sandbox;
mod mobile_pairing;
mod mobile_review_route;
mod mobile_session_route;
mod model_adaptive_service;
mod model_fabric_registry;
mod model_fabric_route;
mod model_fabric_service;
mod model_gateway_runtime;
mod model_inference_route;
mod needs_you;
mod otel_runtime;
mod pages_approval_request;
mod pages_authored;
mod pages_body_store;
mod pages_context_sources;
mod pages_decision_graph;
mod pages_edit_draft;
mod pages_lifecycle;
mod pages_publication;
mod pages_runtime_readiness;
mod pages_search;
mod pages_stewardship;
mod pages_world_model;
mod postgres_exec;
mod process_supervisor;
mod product_board;
mod product_ratification_decision;
mod provider_observation_restore;
mod read_surfaces;
mod receipt_routes;
mod request_security;
mod runtime_projection;
mod secret_store;
mod status;
mod strategy_board;
mod strategy_direction_proposal;
mod strategy_direction_record;
mod strategy_ratification_decision;
mod studio;
mod studio_steering;
mod talent_floor;
mod telemetry_health_route;
mod twin_artifact_blob_store;
mod twin_artifact_context;
mod twin_boundary;
mod twin_capabilities;
mod twin_capability_audit;
mod twin_guard_preflight;
mod twin_guards;
mod twin_intelligence_readiness;
mod twin_live_gateway;
mod twin_model_gateway;
mod twin_office_capability;
mod twin_prompt_packs;
mod twin_session;
mod twin_session_projection;
mod twin_session_stream;
mod twin_skill_proposals;
mod twin_stream_limits;
mod v1_read_shadow_approval_request;
mod v1_replacement;
mod version_route;
mod work_plane;
mod work_triage;
const TEXT_CONTENT_TYPE: &str = "text/plain; charset=utf-8";
const JSON_CONTENT_TYPE: &str = "application/json; charset=utf-8";
pub(crate) const LOCAL_CORS_HEADERS: &str = "Access-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Accept, Content-Type";
fn main() {
    if let Err(error) = run() {
        eprintln!("mdx-server error: {error}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), String> {
    let args = std::env::args().collect::<Vec<_>>();
    match args.get(1).map(String::as_str) {
        Some("run-loop") => {
            let loop_id = args
                .get(2)
                .map(String::as_str)
                .unwrap_or("evals_runner_agent");
            let (kernel, report) = run_local_loop(loop_id)?;
            print_loop_run(loop_id, &kernel, &report);
            Ok(())
        }
        Some("run-local-loops") => {
            for budget in local_loop_budgets() {
                let (kernel, report) = run_local_loop(budget.loop_id)?;
                print_loop_run(budget.loop_id, &kernel, &report);
            }
            Ok(())
        }
        Some("twin-session-draft-local") => { println!("{}", twin_session::render_local_command_json()?); Ok(()) }
        Some("twin-artifact-context-local") => { println!("{}", twin_artifact_context::render_local_command_json()?); Ok(()) }
        Some("object-storage-live-proof") => twin_artifact_blob_store::live_proof_command(),
        Some("bootstrap-beta-auth") =>
            beta_auth_bootstrap::run(&args[2..]).map(|output| println!("{output}")),
        Some("twin-office-capability-local") => { println!("{}", twin_office_capability::render_local_command_json()?); Ok(()) }
        Some("twin-office-download-local") => { println!("{}", twin_office_capability::render_local_download_command_json()?); Ok(()) }
        Some("export-loop-ledger-sql") => {
            let loop_id = args
                .get(2)
                .map(String::as_str)
                .unwrap_or("evals_runner_agent");
            println!("{}", export_loop_ledger_sql(loop_id)?);
            Ok(())
        }
        Some("export-postgres-storage-write-sql") => {
            let loop_id = args
                .get(2)
                .map(String::as_str)
                .unwrap_or("evals_runner_agent");
            println!("{}", export_postgres_storage_write_sql(loop_id)?);
            Ok(())
        }
        Some("export-memory-store-sql") => export_memory_store_sql().map(|sql| println!("{sql}")),
        Some("memory-benchmark-run") => {
            println!("{}", memory_benchmark::run_memory_benchmark()?);
            Ok(())
        }
        Some("memory-backfill-from-ledger") => {
            println!("{}", memory_backfill::run_memory_backfill()?);
            Ok(())
        }
        Some("export-app-state-sql") => {
            println!("{}", app_state_export::render_local_app_state_export_sql()?);
            Ok(())
        }
        Some("write-postgres-app-state") => {
            println!("{}", app_state_export::write_local_app_state_postgres()?);
            Ok(())
        }
        Some("repair-postgres-ledger-from-snapshot") => {
            postgres_exec::ledger_repair::run().map(|report| println!("{report}"))
        }
        Some("write-postgres-storage-receipts") => {
            let loop_id = args
                .get(2)
                .map(String::as_str)
                .unwrap_or("evals_runner_agent");
            println!("{}", write_postgres_storage_receipts(loop_id)?);
            Ok(())
        }
        Some("run-loop-postgres") => {
            let loop_id = args
                .get(2)
                .map(String::as_str)
                .unwrap_or("evals_runner_agent");
            println!("{}", run_loop_postgres_storage(loop_id)?);
            Ok(())
        }
        Some("write-postgres-provider-evidence") => {
            let observed_loops = parse_usize_arg(args.get(2), "observed_loops")?;
            let observed_receipts = parse_usize_arg(args.get(3), "observed_receipts")?;
            let observed_chain_heads = parse_usize_arg(args.get(4), "observed_chain_heads")?;
            let previous_hash = args
                .get(5)
                .cloned()
                .ok_or_else(|| "previous_hash is required".to_string())?;
            println!(
                "{}",
                write_postgres_provider_evidence(
                    observed_loops,
                    observed_receipts,
                    observed_chain_heads,
                    Some(previous_hash)
                )?
            );
            Ok(())
        }
        Some("serve") => {
            let addr = args.get(2).map(String::as_str).unwrap_or("127.0.0.1:8787");
            serve(addr)
        }
        Some("forge-bench") => forge_bench_cli::run(&args[2..]),
        Some("mint-audit-signing-key") => {
            let dir = args.get(2).map(String::as_str).unwrap_or(".mdx-local/audit-keys");
            println!("{}", evidence_signing::mint_signing_key_cli(dir)?);
            Ok(())
        }
        Some("export-evidence-checkpoint") => {
            println!("{}", evidence_checkpoint_route::export_latest_packet_cli()?);
            Ok(())
        }
        Some("verify-evidence-checkpoint") => {
            let path = args
                .get(2)
                .ok_or_else(|| "usage: verify-evidence-checkpoint <packet.json>".to_string())?;
            let verdict = evidence_checkpoint_route::verify_packet_cli(path)?;
            println!("{verdict}");
            if verdict != "VERIFIED" {
                std::process::exit(1);
            }
            Ok(())
        }
        Some("mint-local-secure-token") => {
            println!("{}", auth_verifier::mint_local_secure_token_cli()?);
            Ok(())
        }
        Some("check-migrations") => {
            let report = validate_migration_contracts()?;
            println!(
                "migrations: {}\ntenant_owned_markers: {}\nrls_enabled_tables: {}\npolicy_definitions: {}\npolicy_drop_guards: {}",
                report.migration_count,
                report.tenant_owned_tables,
                report.rls_enabled_tables,
                report.policy_definitions,
                report.policy_drop_guards
            );
            Ok(())
        }
        Some("local-loop-budgets") => {
            for budget in local_loop_budgets() {
                println!("{}\t{}", budget.loop_id, budget.max_receipts);
            }
            Ok(())
        }
        Some("check-postgres-boundary") => {
            let report = validate_migration_contracts()?;
            if std::env::var("MDX_POSTGRES_MIGRATIONS_OBSERVED").ok().as_deref() == Some("1") {
                let storage = PostgresStorage::connect_after_observed_migrations(
                    std::env::var("DATABASE_URL").ok().as_deref(),
                    PostgresMigrationEvidence {
                        migration_count: report.migration_count,
                        tenant_owned_tables: report.tenant_owned_tables,
                        rls_enabled_tables: report.rls_enabled_tables,
                        observed_by: "mdx-server check-postgres-boundary".to_string(),
                    },
                )
                .map_err(|error| error.to_string())?;
                println!(
                    "postgres_boundary: OBSERVED-MIGRATION-EVIDENCE-ACCEPTED\nadapter: {}\ndatabase_url_present: {}",
                    PostgresStorage::adapter_name(),
                    !storage.database_url().is_empty()
                );
                return Ok(());
            }
            let pending = PostgresStorage::connect(std::env::var("DATABASE_URL").ok().as_deref());
            match pending {
                Err(StorageAdapterError::MissingDatabaseUrl)
                | Err(StorageAdapterError::PendingLiveRun { .. }) => {
                    println!(
                        "postgres_boundary: PENDING-LIVE-RUN\nmigrations: {}\ntenant_owned_markers: {}\nrls_enabled_tables: {}\npolicy_definitions: {}\npolicy_drop_guards: {}",
                        report.migration_count,
                        report.tenant_owned_tables,
                        report.rls_enabled_tables,
                        report.policy_definitions,
                        report.policy_drop_guards
                    );
                    Ok(())
                }
                Ok(_) => Err("PostgresStorage connected without observed migration evidence".to_string()),
                Err(error) => Err(error.to_string()),
            }
        }
        Some("status") => {
            if args.get(2).map(String::as_str) == Some("--json") {
                println!("{}", render_status_json());
            } else {
                println!("{}", render_status_text());
            }
            Ok(())
        }
        _ => Err(
            "usage: mdx-server run-loop <loop_id> | harness inspect | harness-inspect | run-loop-postgres <loop_id> | run-local-loops | export-loop-ledger-sql <loop_id> | export-postgres-storage-write-sql <loop_id> | export-memory-store-sql | memory-benchmark-run | memory-backfill-from-ledger | export-app-state-sql | write-postgres-app-state | repair-postgres-ledger-from-snapshot | bootstrap-beta-auth <tenant_id> <target_auth_user_id> <mapped_role> | write-postgres-storage-receipts <loop_id> | write-postgres-provider-evidence <loops> <receipts> <chain_heads> <previous_hash> | local-loop-budgets | serve 127.0.0.1:8787 | check-migrations | check-postgres-boundary | status [--json]"
                .to_string(),
        ),
    }
}
fn print_loop_run(loop_id: &str, kernel: &MdxKernel, report: &LoopRunReport) {
    println!("{}", render_report(report));
    println!("{}", kernel.observatory_view());
    if loop_id == "evals_runner_agent" {
        println!("{}", kernel.concierge_answer("what happened and why?"));
    }
}
fn run_local_loop(loop_id: &str) -> Result<(MdxKernel, LoopRunReport), String> {
    let mut kernel = MdxKernel::boot_local();
    let runner = LocalLoopRunner;
    let report = match loop_id {
        "evals_runner_agent" => runner.run_evals_runner_agent(&mut kernel)?,
        "aegis_scanner_agent" => runner.run_aegis_scanner_agent(&mut kernel)?,
        "charter_attestation_agent" => runner.run_charter_attestation_agent(&mut kernel)?,
        "forge_orchestrator_agent" => runner.run_forge_orchestrator_agent(&mut kernel)?,
        "product_shaping_agent" => runner.run_product_shaping_agent(&mut kernel)?,
        "talent_autonomy_agent" => runner.run_talent_autonomy_agent(&mut kernel)?,
        _ => {
            return Err(format!(
                "unknown loop {loop_id}; enabled loops: {}",
                enabled_loop_ids()
            ));
        }
    };
    Ok((kernel, report))
}
fn export_loop_ledger_sql(loop_id: &str) -> Result<String, String> {
    let (kernel, report) = run_local_loop(loop_id)?;
    Ok(render_postgres_ledger_export_sql(
        kernel.ledger().entries(),
        &report.loop_id,
    ))
}
fn export_postgres_storage_write_sql(loop_id: &str) -> Result<String, String> {
    let (storage, kernel, report) = observed_postgres_storage_loop(loop_id)?;
    Ok(storage.render_receipt_write_sql(kernel.ledger().entries(), &report.loop_id))
}
fn export_memory_store_sql() -> Result<String, String> {
    let mut kernel = MdxKernel::boot_local();
    kernel.run_evals_runner_agent()?;
    let correlation = CorrelationIds {
        tenant_id: TenantId::new("local_tenant"),
        trace_id: TraceId::new(kernel.mint_id("trace")),
        actor_id: ActorId::new("human:local_user"),
        loop_id: LoopId::new("memory_export"),
        workflow_id: WorkflowId::new(kernel.mint_id("workflow")),
    };
    kernel.run_memory_beta_readiness(&correlation, 1000)?;
    kernel.evaluate_memory_lifecycle(&correlation, "memory export proof")?;
    Ok(render_postgres_memory_export_sql(&kernel))
}
fn write_postgres_storage_receipts(loop_id: &str) -> Result<String, String> {
    let (storage, kernel, report) = observed_postgres_storage_loop(loop_id)?;
    let write_report = storage
        .write_receipts_live(kernel.ledger().entries(), &report.loop_id)
        .map_err(|error| error.to_string())?;
    Ok(format!(
        "postgres_live_transport: OK loop_id={} ledger_receipts={} chain_heads={} provider_receipt_kind={}",
        write_report.loop_id,
        write_report.ledger_receipts,
        write_report.chain_heads,
        write_report.provider_receipt_kind
    ))
}
fn run_loop_postgres_storage(loop_id: &str) -> Result<String, String> {
    let (storage, kernel, report) = observed_postgres_storage_loop(loop_id)?;
    let write_report = storage
        .write_receipts_live(kernel.ledger().entries(), &report.loop_id)
        .map_err(|error| error.to_string())?;
    Ok(format!(
        "{}postgres_runtime_storage: OK loop_id={} ledger_receipts={} chain_heads={} provider_receipt_kind={}",
        render_report(&report),
        write_report.loop_id,
        write_report.ledger_receipts,
        write_report.chain_heads,
        write_report.provider_receipt_kind
    ))
}
fn write_postgres_provider_evidence(
    observed_loops: usize,
    observed_receipts: usize,
    observed_chain_heads: usize,
    previous_hash: Option<String>,
) -> Result<String, String> {
    let storage = observed_postgres_storage()?;
    let receipt = postgres_provider_turn_on_evidence_receipt(
        observed_loops,
        observed_receipts,
        observed_chain_heads,
        previous_hash,
    );
    let write_report = storage
        .write_receipts_live(std::slice::from_ref(&receipt), "postgres_live_transport")
        .map_err(|error| error.to_string())?;
    Ok(format!(
        "postgres_provider_turn_on_evidence: OK receipt_id={} receipt_kind={} ledger_receipts={}",
        receipt.receipt_id, write_report.provider_receipt_kind, write_report.ledger_receipts
    ))
}
fn observed_postgres_storage_loop(
    loop_id: &str,
) -> Result<(PostgresStorage, MdxKernel, LoopRunReport), String> {
    let storage = observed_postgres_storage()?;
    let (kernel, report) = run_local_loop(loop_id)?;
    Ok((storage, kernel, report))
}
fn observed_postgres_storage() -> Result<PostgresStorage, String> {
    if std::env::var("MDX_POSTGRES_MIGRATIONS_OBSERVED")
        .ok()
        .as_deref()
        != Some("1")
    {
        return Err("MDX_POSTGRES_MIGRATIONS_OBSERVED=1 is required".to_string());
    }
    let report = validate_migration_contracts()?;
    PostgresStorage::connect_after_observed_migrations(
        std::env::var("DATABASE_URL").ok().as_deref(),
        PostgresMigrationEvidence {
            migration_count: report.migration_count,
            tenant_owned_tables: report.tenant_owned_tables,
            rls_enabled_tables: report.rls_enabled_tables,
            observed_by: "mdx-server export-postgres-storage-write-sql".to_string(),
        },
    )
    .map_err(|error| error.to_string())
}
fn serve(addr: &str) -> Result<(), String> {
    // Resolve the deployment mode and run the production boot gate before we
    // listen. A production node refuses to start without a configured auth
    // profile; local modes always boot. See docs/AUTH-PRODUCTION-BOUNDARY.md.
    let mode_raw = std::env::var("MDX_DEPLOYMENT_MODE").unwrap_or_default();
    let mode =
        deployment::startup_deployment_gate(&mode_raw, deployment::auth_profile_configured())?;
    // Phase 5: a production node also refuses to start with missing posture or an
    // unsafe shortcut. Local modes pass through their own posture.
    deployment::startup_profile_gate(mode, deployment::startup_posture_from_env())?;
    twin_artifact_blob_store::initialize(mode)?;
    otel_runtime::initialize(mode)?;
    println!("mdx deployment mode: {}", mode.as_str());
    let listener = TcpListener::bind(addr).map_err(|error| format!("bind {addr}: {error}"))?;
    println!("mdx local server listening on http://{addr}");
    let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
    // Durable ledger restore comes first. Invalid snapshots refuse boot rather
    // than starting empty over real evidence. File provider restore only runs
    // when no receipt snapshot was restored.
    let mut restored_from_snapshot = false;
    let mut ledger_file_restored = false;
    let mut memory_restored_from_postgres = false;
    if kernel_snapshot::enabled(mode) {
        let mut booted = kernel
            .write()
            .map_err(|_| "kernel lock poisoned at boot".to_string())?;
        match kernel_snapshot::restore_into(&mut booted) {
            Ok(Some(count)) => {
                println!(
                    "mdx kernel restored {count} receipts from {}",
                    kernel_snapshot::SNAPSHOT_PATH
                );
                restored_from_snapshot = true;
                ledger_file_restored = true;
            }
            Ok(None) => {}
            Err(error) => {
                return Err(format!(
                    "kernel snapshot restore refused: {error}; move {} aside to boot empty",
                    kernel_snapshot::SNAPSHOT_PATH
                ));
            }
        }
    }
    if restored_from_snapshot {
        memory_restored_from_postgres = app_state_export::reconcile_snapshot(&kernel)?;
    }
    // No snapshot, but the persistence plane is configured: the database is
    // the source of truth (new host, lost disk). Restore the chain from
    // ledger_entries through the native driver. A configured store that
    // cannot be restored refuses boot - booting empty beside a populated
    // database would fork the chain on the next export.
    if !restored_from_snapshot
        && kernel_snapshot::enabled(mode)
        && app_state_export::route_app_state_writer_enabled()
        && let Ok(url) = std::env::var("DATABASE_URL")
        && !url.trim().is_empty()
    {
        let mut booted = kernel
            .write()
            .map_err(|_| "kernel lock poisoned at boot".to_string())?;
        if let Some((count, memory_count)) = postgres_exec::restore_kernel(&url, &mut booted)? {
            println!("mdx kernel restored {count} receipts from postgres ledger_entries");
            println!(
                "mdx kernel restored {memory_count} memory records from postgres memory_records"
            );
            // A fresh full snapshot so the next boot can restore both the
            // evidence chain and Memory without reaching the database first.
            let rendered = kernel_snapshot::render_snapshots(&booted);
            if let Err(error) =
                rendered.and_then(|rendered| kernel_snapshot::write_snapshots(&rendered))
            {
                eprintln!("mdx-server snapshot write failed after postgres restore: {error}");
            }
            restored_from_snapshot = true;
        }
    }
    // Memory restores from the authoritative ledger bundle after the ledger,
    // with the historical sibling used only for legacy snapshots. The kernel
    // refuses any record citing a receipt absent from the restored chain, so
    // the ledger never attests to Memory that no longer exists and persisted
    // Memory never resurrects content the chain cannot back. An absent legacy
    // sibling boots with empty Memory. A sibling that does not parse or verify
    // refuses boot when its paired legacy ledger was restored; when the ledger
    // file itself is gone, the sibling is an orphan, not evidence, so it is set
    // aside and boot continues with empty Memory.
    if kernel_snapshot::enabled(mode) && !memory_restored_from_postgres {
        let mut booted = kernel
            .write()
            .map_err(|_| "kernel lock poisoned at boot".to_string())?;
        match kernel_snapshot::restore_memory_into(&mut booted) {
            Ok(Some(count)) => {
                println!("mdx kernel restored {count} memory records from durable snapshot state");
            }
            Ok(None) => {}
            Err(error) if !ledger_file_restored => {
                let orphan_path = format!("{}.orphaned", kernel_snapshot::MEMORY_SNAPSHOT_PATH);
                if let Err(rename_error) =
                    std::fs::rename(kernel_snapshot::MEMORY_SNAPSHOT_PATH, &orphan_path)
                {
                    return Err(format!(
                        "memory snapshot restore refused: {error}; moving the orphaned {} aside also failed: {rename_error}",
                        kernel_snapshot::MEMORY_SNAPSHOT_PATH
                    ));
                }
                eprintln!(
                    "mdx-server set aside orphaned memory snapshot ({error}); moved {} to {orphan_path}",
                    kernel_snapshot::MEMORY_SNAPSHOT_PATH
                );
            }
            Err(error) => {
                return Err(format!(
                    "memory snapshot restore refused: {error}; move {} and {} aside to boot without restored memory",
                    kernel_snapshot::SNAPSHOT_PATH,
                    kernel_snapshot::MEMORY_SNAPSHOT_PATH
                ));
            }
        }
    }
    if !restored_from_snapshot && let Ok(mut booted) = kernel.write() {
        provider_observation_restore::restore_provider_observations(&mut booted);
    }
    {
        let booted = kernel
            .read()
            .map_err(|_| "kernel lock poisoned during Model Fabric restore".to_string())?;
        model_fabric_registry::global().restore_from_kernel(&booted, secret_store::global())?;
    }
    // The evidence teeth: a restored chain must still satisfy its latest
    // signed checkpoint. An attacker who rewrites history can recompute the
    // internal hashes but cannot re-sign the checkpoint - so either the
    // signature or the Merkle root gives them away, and boot refuses.
    if restored_from_snapshot {
        let booted = kernel
            .read()
            .map_err(|_| "kernel lock poisoned at boot".to_string())?;
        match evidence_signing::verify_restored_chain(&booted, mode) {
            Ok(Some(checkpoint_id)) => {
                println!(
                    "mdx restored chain verified against signed evidence checkpoint {checkpoint_id}"
                );
            }
            Ok(None) => {}
            Err(error) => {
                return Err(format!(
                    "evidence verification refused the restored chain: {error}"
                ));
            }
        }
    }
    let forge_reconciliation = forge_startup_reconciler::reconcile(&kernel)?;
    println!(
        "mdx Forge startup reconciliation: {}",
        forge_reconciliation.summary()
    );
    for error in &forge_reconciliation.errors {
        eprintln!("mdx Forge startup reconciliation note: {error}");
    }
    // Durable execution: re-drive fleets killed mid-run (opt-in, no-op off).
    fleet_run_conductor::resume_unfinished_fleets(&kernel);
    // Execution posture is observed at the gates that matter (run start,
    // live delivery), not at boot: a boot-time receipt would land before
    // the deterministic local seed and shift every seeded receipt id.
    // Catch a graceful stop before the flusher window can eat the last writes.
    // Blocking SIGTERM/SIGINT here - before the flusher and connection threads
    // spawn - means only the dedicated waiter receives them; it runs one final
    // synchronous durable flush so a container stop, deploy, Ctrl-C, or
    // dogfood-stack teardown loses nothing. After this, only a SIGKILL or power
    // loss can drop the trailing coalescing window. No-op when snapshots are
    // off or the interval is 0.
    kernel_snapshot::install_shutdown_flush();
    // Start the coalescing snapshot flusher so governed writes no longer pay a
    // full ledger serialize+fsync synchronously on the request thread. No-op
    // when snapshots are off or the interval is 0 (synchronous mode).
    kernel_snapshot::init_flusher(Arc::clone(&kernel), mode);
    let open_connections = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let max_open_connections = max_open_connections_from_env();
    let rate_limiter = Arc::new(http_rate_limit::RateLimiter::from_env());
    let rate_limit_on = http_rate_limit::enabled(mode);
    for stream in listener.incoming() {
        let stream = stream.map_err(|error| format!("incoming connection: {error}"))?;
        // The front door is bounded: past the cap a connection gets an
        // immediate 503 instead of an unbounded thread. The worker-side
        // admission envelope cannot protect a server whose entrance spawns
        // without limit.
        let admitted = ConnectionSlot::admit(&open_connections, max_open_connections);
        let Some(slot) = admitted else {
            http_response::refuse_over_capacity(stream);
            continue;
        };
        let kernel = Arc::clone(&kernel);
        let rate_limiter = rate_limit_on.then(|| Arc::clone(&rate_limiter));
        // A thread per connection: a streaming answer must never block
        // the rest of the kernel (it used to - one live stream stalled
        // every other request, and two concurrent specialists were
        // impossible). The kernel stays consistent behind its mutex;
        // handlers hold the lock only around reads and saves, never
        // across a provider call. One named exception remains: the
        // opt-in live mode of /harness/programmable-pipeline.json still
        // executes under the lock (tracked follow-up); its default
        // deterministic mode does not call out. And one bad connection
        // must never take the server down: a client closing a stream
        // mid-answer is normal life, not a fatal error.
        std::thread::spawn(move || {
            let _slot = slot;
            if let Err(error) = handle_connection(stream, kernel, mode, rate_limiter) {
                eprintln!("mdx-server connection error: {error}");
            }
        });
    }
    Ok(())
}
fn max_open_connections_from_env() -> usize {
    std::env::var("MDX_MAX_OPEN_CONNECTIONS")
        .ok()
        .and_then(|raw| raw.parse::<usize>().ok())
        .filter(|cap| *cap > 0)
        .unwrap_or(256)
}
/// One admitted connection. Dropping the slot releases it, including on any
/// panic inside the handler thread.
struct ConnectionSlot {
    open: Arc<std::sync::atomic::AtomicUsize>,
}
impl ConnectionSlot {
    fn admit(open: &Arc<std::sync::atomic::AtomicUsize>, cap: usize) -> Option<Self> {
        let previous = open.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if previous >= cap {
            open.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            return None;
        }
        Some(Self { open: open.clone() })
    }
}

impl Drop for ConnectionSlot {
    fn drop(&mut self) {
        self.open.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

fn handle_connection(
    mut stream: TcpStream,
    kernel: Arc<RwLock<MdxKernel>>,
    mode: DeploymentMode,
    rate_limiter: Option<Arc<http_rate_limit::RateLimiter>>,
) -> Result<(), String> {
    // Per-source throttle before any read: an over-limit peer gets 429 and
    // releases the thread immediately.
    if let Some(limiter) = &rate_limiter
        && let Ok(peer) = stream.peer_addr()
        && !limiter.admit_ip(peer.ip())
    {
        let body = "{\"error\":\"rate_limited\"}";
        let wire = format!(
            "HTTP/1.1 429 Too Many Requests\r\nContent-Type: {JSON_CONTENT_TYPE}\r\nRetry-After: 1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(wire.as_bytes());
        return Ok(());
    }
    // Reading stops being best-effort here: headers are read to completion and
    // the body to its declared Content-Length, with caps and timeouts. The old
    // single 8 KB read truncated segmented or large requests silently.
    let _ = stream.set_read_timeout(Some(http_read::read_timeout()));
    let _ = stream.set_write_timeout(Some(http_read::write_timeout()));
    let request = match http_read::read_http_request(&mut stream)? {
        http_read::RequestRead::Complete(request) => request,
        http_read::RequestRead::Refused { status, code } => {
            let body = format!("{{\"error\":\"{code}\"}}");
            let wire = format!(
                "HTTP/1.1 {status}\r\nContent-Type: {JSON_CONTENT_TYPE}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(wire.as_bytes())
                .map_err(|error| format!("write refusal: {error}"))?;
            return Ok(());
        }
    };
    let request = request.as_str();
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");
    let method = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().next())
        .unwrap_or("GET");
    let body = request.split("\r\n\r\n").nth(1).unwrap_or("");
    let security = request_security::RequestSecurity::for_connection(mode, request);
    if let Some(limiter) = &rate_limiter
        && let Some(key) = security.rate_limit_key()
        && !limiter.admit_principal(&key)
    {
        let body = "{\"error\":\"principal_rate_limited\"}";
        let wire = format!(
            "HTTP/1.1 429 Too Many Requests\r\nContent-Type: {JSON_CONTENT_TYPE}\r\nRetry-After: 1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(wire.as_bytes());
        return Ok(());
    }
    if path == "/twin/session-stream" && method.eq_ignore_ascii_case("POST") {
        let identity_guard = match security.classify_governed_write(method, path, body) {
            request_security::GovernedWrite::Refused(response) => {
                http_response::write_route_response(&mut stream, mode, request, response)?;
                return Ok(());
            }
            request_security::GovernedWrite::Admitted(identity) => {
                Some(request_security::set_verified_identity(Some(identity)))
            }
            request_security::GovernedWrite::Ungated => None,
        };
        // The stream saves its receipt when it completes, so it snapshots on
        // the way out like any other successful write.
        let _identity_guard = identity_guard;
        let outcome = twin_session_stream::handle(stream, request, &kernel);
        if outcome.is_ok() {
            snapshot_after_write(&kernel, mode);
        }
        return outcome;
    }
    if path.starts_with("/forge/runs/stream") && method.eq_ignore_ascii_case("GET") {
        if http_response::reject_production_stream(&mut stream, mode, request, path, &security)? {
            return Ok(());
        }
        let _identity_guard =
            request_security::set_verified_identity(security.verified_identity_for_read());
        return forge_run_stream::handle(stream, request, &kernel);
    }
    if path.starts_with("/activation/first-mission/shaping/stream")
        && method.eq_ignore_ascii_case("GET")
    {
        if http_response::reject_production_stream(&mut stream, mode, request, path, &security)? {
            return Ok(());
        }
        let _identity_guard =
            request_security::set_verified_identity(security.verified_identity_for_read());
        return activation_deliberation_stream::handle(stream, request, &kernel);
    }
    let routed_at = std::time::Instant::now();
    let response =
        route_request_secured_with_raw(method, path, body, &kernel, &security, Some(request))?;
    let routed_micros = routed_at.elapsed().as_micros() as u64;
    http_telemetry::record(path, response.status, routed_micros);
    otel_runtime::record_http(
        method,
        path,
        response.status,
        routed_micros,
        request,
        security.telemetry_identity(),
    );
    let wire_response = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\n{}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        response.status,
        response.content_type,
        request_security::cors_headers(mode, request),
        response.body.len(),
        response.body
    );
    stream
        .write_all(wire_response.as_bytes())
        .map_err(|error| format!("write response: {error}"))?;
    // Durability rides every successful mutation: serialize under the lock, write outside, fail loudly.
    if method.eq_ignore_ascii_case("POST") && response.status.starts_with("200") {
        snapshot_after_write(&kernel, mode);
        app_state_export::spawn_after_response(
            method,
            path,
            response.status,
            Arc::clone(&kernel),
            mode,
        );
    }
    Ok(())
}

/// Snapshot the ledger after a successful write when snapshots are enabled
/// for this mode. Serialization happens under the kernel lock; the file write
/// happens after it is released so a slow disk never blocks other requests.
fn snapshot_after_write(kernel: &Arc<RwLock<MdxKernel>>, mode: DeploymentMode) {
    if !kernel_snapshot::enabled(mode) {
        return;
    }
    // With the background flusher installed, the hot path only marks the ledger
    // dirty in O(1); the flusher coalesces a burst of writes into one
    // serialize+fsync off this thread, which is what keeps p95 flat under
    // concurrent governed writes as the ledger grows.
    if let Some(flusher) = kernel_snapshot::flusher() {
        flusher.request();
        return;
    }
    // Synchronous fallback (MDX_KERNEL_SNAPSHOT_INTERVAL_MS=0): serialize under
    // the lock, write outside it. Recover a poisoned lock rather than skip
    // durability - the receipt hash-chain, verified on every write and at boot,
    // is the real integrity guarantee, not the poison flag.
    let rendered = {
        let kernel = kernel
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        kernel_snapshot::render_snapshots(&kernel)
    };
    if let Err(error) = rendered.and_then(|rendered| kernel_snapshot::write_snapshots(&rendered)) {
        eprintln!("mdx-server snapshot write failed: {error}");
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RouteResponse {
    status: &'static str,
    content_type: &'static str,
    body: String,
}
impl RouteResponse {
    pub(crate) fn text(status: &'static str, body: String) -> Self {
        Self {
            status,
            content_type: TEXT_CONTENT_TYPE,
            body,
        }
    }
    pub(crate) fn json(status: &'static str, body: String) -> Self {
        Self {
            status,
            content_type: JSON_CONTENT_TYPE,
            body,
        }
    }
    #[cfg(test)]
    pub(crate) fn body_text(&self) -> &str {
        &self.body
    }
    #[cfg(test)]
    pub(crate) fn status_for_test(&self) -> &str {
        self.status
    }
}
#[rustfmt::skip]
fn read_only_json_route(method: &str, render: impl FnOnce() -> Result<String, String>) -> Result<RouteResponse, String> {
    if let Some(response) = reject_unless_method(method, "GET") { Ok(response) } else { Ok(RouteResponse::json("200 OK", render()?)) }
}
#[cfg(test)]
fn route_request(
    method: &str,
    path: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    route_request_with_body(method, path, "", kernel)
}
#[cfg(test)]
fn json_string_field_for_test(body: &str, key: &str) -> String {
    let after_key = body
        .split(&format!("\"{key}\":\""))
        .nth(1)
        .unwrap_or_else(|| panic!("missing json string field {key}"));
    after_key
        .split('"')
        .next()
        .unwrap_or_else(|| panic!("missing json string value {key}"))
        .to_string()
}
/// The secured entry point for the live serving path. In a deployment mode that
/// requires a verified session, a governed write is refused here before any
/// handler runs, so a handler can never authorize from the request body or emit a
/// local stub in production. local-demo is not gated. Every network request goes
/// through this; direct `route_request_with_body` callers are local-demo test
/// paths.
#[cfg(test)]
fn route_request_secured(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
    security: &request_security::RequestSecurity,
) -> Result<RouteResponse, String> {
    route_request_secured_with_raw(method, path, body, kernel, security, None)
}

fn route_request_secured_with_raw(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
    security: &request_security::RequestSecurity,
    raw_request: Option<&str>,
) -> Result<RouteResponse, String> {
    // Trusted-session reads need a verified identity for tenant-scoped views.
    if let Some(refusal) = security.classify_read(method, path) {
        return Ok(refusal);
    }
    let _read_identity_guard = if method.eq_ignore_ascii_case("POST") {
        None
    } else {
        Some(request_security::set_verified_identity(
            security.verified_identity_for_read(),
        ))
    };
    let _identity_guard = match security.classify_governed_write(method, path, body) {
        request_security::GovernedWrite::Refused(refusal) => return Ok(refusal),
        request_security::GovernedWrite::Admitted(identity) => {
            Some(request_security::set_verified_identity(Some(identity)))
        }
        request_security::GovernedWrite::Ungated => None,
    };
    let response = if path.split('?').next() == Some("/mobile/cloud/github-webhooks.json") {
        mobile_cloud_route::webhook_response(method, body, raw_request.unwrap_or(""), kernel)?
    } else {
        route_request_with_body(method, path, body, kernel)?
    };
    if !kernel_snapshot::enabled(security.mode())
        && let Err(error) =
            app_state_export::persist_after_governed_post(method, path, response.status, kernel)
    {
        // Without durable snapshots, the export is the only app-state
        // durability path, so the 500 stands and the divergence is visible.
        return Ok(RouteResponse::text("500 Internal Server Error", error));
    }
    Ok(response)
}
fn route_request_with_body(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = model_fabric_route::route_response(method, path, body, kernel) {
        return response;
    }
    if let Some(response) = twin_prompt_packs::route_response(method, path, body) {
        return Ok(response);
    }
    if let Some(response) = version_route::route_response(method, path) {
        return response;
    }
    macro_rules! try_body_route {
        ($module:ident) => {
            if let Some(response) = $module::route_response(method, path, body, kernel) {
                return response;
            }
        };
    }
    try_body_route!(talent_floor);
    try_body_route!(product_board);
    try_body_route!(work_plane);
    try_body_route!(work_triage);
    try_body_route!(feedback);
    try_body_route!(feedback_autonomy_route);
    try_body_route!(telemetry_health_route);
    try_body_route!(autonomy_envelope_route);
    try_body_route!(forge_repo_connect_route);
    try_body_route!(activation_route);
    try_body_route!(forge_run_route);
    try_body_route!(forge_browser_route);
    try_body_route!(forge_semantic_query_route);
    try_body_route!(forge_outcome_signal_route);
    if let Some(response) = forge_flywheel_proof_route::route_response(method, path, kernel) {
        return response;
    }
    try_body_route!(forge_fleet_eval_scoreboard);
    try_body_route!(forge_long_horizon_mission);
    try_body_route!(forge_work_classification);
    try_body_route!(forge_diff_route);
    try_body_route!(forge_review_packet);
    try_body_route!(forge_candidate_selection_route);
    try_body_route!(forge_pr_handoff_route);
    try_body_route!(forge_repo_onboarding_packet_route);
    try_body_route!(forge_repo_task_scout_route);
    try_body_route!(forge_source_host_readiness_route);
    try_body_route!(forge_source_host_pr_draft_route);
    try_body_route!(forge_source_host_live_delivery_route);
    try_body_route!(forge_review_panel_route);
    try_body_route!(forge_run_ship_route);
    try_body_route!(forge_run_control_route);
    try_body_route!(fleet_plan_route);
    try_body_route!(connector_route);
    try_body_route!(message_channel_route);
    if let Some(response) = message_activity_route::route_response(method, path, kernel) {
        return response;
    }
    if let Some(response) = message_presence_route::route_response(method, path, kernel) {
        return response;
    }
    try_body_route!(message_bridge_route);
    try_body_route!(mobile_app_attest);
    try_body_route!(mobile_pairing);
    try_body_route!(mobile_session_route);
    try_body_route!(mobile_command_route);
    try_body_route!(mobile_cloud_route);
    try_body_route!(mobile_review_route);
    try_body_route!(changelog_route);
    try_body_route!(fleet_run_route);
    try_body_route!(forge_model_scorecard_route);
    try_body_route!(forge_model_provider_routing);
    try_body_route!(forge_revise_route);
    try_body_route!(forge_repo_route);
    try_body_route!(forge_repo_readiness_route);
    try_body_route!(forge_repo_standards_packet_route);
    try_body_route!(forge_recipes_route);
    try_body_route!(forge_builder_loop_route);
    try_body_route!(needs_you);
    try_body_route!(strategy_direction_record);
    try_body_route!(strategy_board);
    if let Some(response) = strategy_direction_proposal::route_response(method, path, body, kernel)
    {
        return response;
    }
    if let Some(response) =
        strategy_ratification_decision::route_response(method, path, body, kernel)
    {
        return response;
    }
    if let Some(response) =
        product_ratification_decision::route_response(method, path, body, kernel)
    {
        return response;
    }
    if let Some(response) = learning_routes::route_response(method, path, body, kernel) {
        return response;
    }
    if let Some(response) = marketplace_skill_route::route_response(method, path, body, kernel) {
        return response;
    }
    if let Some(response) = capability_execution_route::route_response(method, path, body, kernel) {
        return response;
    }
    if let Some(response) =
        memory_consolidation_ratify_route::route_response(method, path, body, kernel)
    {
        return response;
    }
    if let Some(response) = evidence_checkpoint_route::route_response(method, path, body, kernel) {
        return response;
    }
    if let Some(response) = twin_capabilities::route_response(method, path, body) {
        return Ok(response);
    }
    if let Some(response) = twin_skill_proposals::route_response(method, path, body) {
        return Ok(response);
    }
    if let Some(response) = twin_guard_preflight::route_response(method, path, body) {
        return Ok(response);
    }
    if let Some(response) = twin_stream_limits::route_response(method, path, body) {
        return Ok(response);
    }
    if let Some(response) = message_controls::route_response(method, path, body) {
        return Ok(response);
    }
    if let Some(response) = pages_stewardship::route_response(method, path, body) {
        return Ok(response);
    }
    if method.eq_ignore_ascii_case("OPTIONS")
        && (request_security::is_governed_write_route(path)
            || request_security::is_public_intake_write_route(path))
    {
        return Ok(RouteResponse::text("204 No Content", String::new()));
    }
    let response = if path == "/health" {
        reject_unless_method(method, "GET")
            .unwrap_or_else(|| RouteResponse::text("200 OK", "ok\n".to_string()))
    } else if path == "/status" {
        reject_unless_method(method, "GET")
            .unwrap_or_else(|| RouteResponse::text("200 OK", render_status_text()))
    } else if path == "/status.json" {
        reject_unless_method(method, "GET")
            .unwrap_or_else(|| RouteResponse::json("200 OK", render_status_json()))
    } else if let Some(body) = render_local_evidence_route(path) {
        reject_unless_method(method, "GET").unwrap_or_else(|| RouteResponse::json("200 OK", body))
    } else if path == "/local/auth-session.json" {
        reject_unless_method(method, "GET")
            .unwrap_or_else(|| RouteResponse::json("200 OK", auth_session_route::render_json()))
    } else if path == "/forge/fleet-eval-scoreboard.json" {
        read_only_json_route(method, render_forge_fleet_eval_scoreboard_json)?
    } else if path == "/runtime/queue-projection.json" {
        if let Some(response) = reject_unless_method(method, "GET") {
            response
        } else {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            RouteResponse::json(
                "200 OK",
                runtime_projection::render_runtime_queue_projection_json(&kernel)?,
            )
        }
    } else if path == "/runtime/metrics.json" {
        if let Some(response) = reject_unless_method(method, "GET") {
            response
        } else {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            RouteResponse::json(
                "200 OK",
                runtime_projection::render_runtime_metrics_json(&kernel)?,
            )
        }
    } else if path == "/runtime/operator-packet.json" {
        if let Some(response) = reject_unless_method(method, "GET") {
            response
        } else {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            RouteResponse::json(
                "200 OK",
                runtime_projection::render_runtime_operator_packet_json(&kernel)?,
            )
        }
    } else if path == "/forge/runtime-projection.json" {
        if let Some(response) = reject_unless_method(method, "GET") {
            response
        } else {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            RouteResponse::json(
                "200 OK",
                runtime_projection::render_forge_runtime_projection_json(&kernel)?,
            )
        }
    } else if path == "/forge/control-plane/projection.json" {
        if let Some(response) = reject_unless_method(method, "GET") {
            response
        } else {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            RouteResponse::json(
                "200 OK",
                forge_control_plane_projection::render_forge_control_plane_projection_json(
                    &kernel,
                )?,
            )
        }
    } else if path == "/autonomy/envelopes/projection.json" {
        if let Some(response) = reject_unless_method(method, "GET") {
            response
        } else {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            RouteResponse::json(
                "200 OK",
                runtime_projection::render_autonomy_envelope_projection_json(&kernel)?,
            )
        }
    } else if path == "/runtime/load-proof-summary.json" {
        read_only_json_route(
            method,
            runtime_projection::render_runtime_load_proof_summary_json,
        )?
    } else if path == "/runtime/http-metrics.json" {
        read_only_json_route(method, || Ok(http_telemetry::render_json()))?
    } else if path == "/runtime/watchtower.json" {
        if let Some(response) = reject_unless_method(method, "GET") {
            response
        } else {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            RouteResponse::json(
                "200 OK",
                runtime_projection::render_runtime_watchtower_json(&kernel)?,
            )
        }
    } else if let Some(response) = twin_intelligence_readiness::route_response(method, path, kernel)
    {
        response?
    } else if let Some(response) = twin_model_gateway::route_request(method, path, body, kernel) {
        response?
    } else if let Some(response) = twin_office_capability::route_request(method, path, body, kernel)
    {
        response?
    } else if path == "/receipts.json" {
        if let Some(response) = reject_unless_method(method, "GET") {
            response
        } else {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            RouteResponse::json("200 OK", render_receipts_json(&kernel))
        }
    } else if let Some(receipt_id) = path.strip_prefix("/receipts/") {
        if let Some(response) = reject_unless_method(method, "GET") {
            response
        } else {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            if let Some(receipt) = kernel.ledger().query().by_id(receipt_id) {
                RouteResponse::json("200 OK", render_receipt_json(receipt))
            } else {
                RouteResponse::text("404 Not Found", "receipt not found\n".to_string())
            }
        }
    } else if path == "/observatory" {
        if let Some(response) = reject_unless_method(method, "GET") {
            response
        } else {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            RouteResponse::text("200 OK", kernel.observatory_view())
        }
    } else if path == "/observatory.json" {
        if let Some(response) = reject_unless_method(method, "GET") {
            response
        } else {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            RouteResponse::json("200 OK", kernel.observatory_read_model().render_json())
        }
    } else if path == "/concierge.json" {
        if let Some(response) = reject_unless_method(method, "GET") {
            response
        } else {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            RouteResponse::json("200 OK", kernel.concierge_read_model().render_json())
        }
    } else if path.starts_with("/concierge") {
        if let Some(response) = reject_unless_method(method, "GET") {
            response
        } else {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            RouteResponse::text("200 OK", kernel.concierge_answer("what happened and why?"))
        }
    } else if let Some(response) = beta_program_route::route_response(method, path, body, kernel) {
        response?
    } else if let Some(response) = twin_artifact_context::route_request(method, path, body, kernel)
    {
        response?
    } else if path == "/twin/session-draft.json" {
        read_only_json_route(method, twin_session::render_json)?
    } else if path == "/twin/session-drafts.json" {
        if let Some(response) = reject_unless_method(method, "POST") {
            response
        } else {
            // Prepare under the lock, call the provider with the lock
            // released, save under the lock again - the streaming route's
            // contract. One slow provider answer must never freeze every
            // other surface for every other user.
            let prepared = {
                let mut kernel = kernel
                    .write()
                    .map_err(|_| "kernel lock poisoned".to_string())?;
                twin_session::prepare_local_post(body, &mut kernel)?
            };
            let execution = prepared
                .live_call
                .as_ref()
                .and_then(|call| twin_live_gateway::execute_prepared(call, &prepared.draft_text));
            let mut kernel = kernel
                .write()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            model_fabric_service::persist_pending_model_runtime_evidence(
                &mut kernel,
                &prepared.tenant_id,
                &prepared.actor_id,
            )?;
            if let Some(execution) = execution.as_ref() {
                let correlation = mdx_core::CorrelationIds {
                    tenant_id: mdx_core::TenantId::new(&prepared.tenant_id),
                    trace_id: mdx_core::TraceId::new(kernel.mint_id("trace")),
                    actor_id: mdx_core::ActorId::new(&prepared.actor_id),
                    loop_id: mdx_core::LoopId::new("model_fabric"),
                    workflow_id: mdx_core::WorkflowId::new(kernel.mint_id("workflow")),
                };
                kernel
                    .record_model_outcome(&correlation, &execution.outcome)
                    .map_err(str::to_string)?;
            }
            RouteResponse::json(
                "200 OK",
                twin_session::complete_local_post(
                    &prepared,
                    execution.map(|execution| execution.answer),
                    &mut kernel,
                )?,
            )
        }
    } else if path == "/twin/session-drafts/projection.json" {
        if let Some(response) = reject_unless_method(method, "GET") {
            response
        } else {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            RouteResponse::json(
                "200 OK",
                twin_session_projection::render_local_projection_json(&kernel)?,
            )
        }
    } else if let Some(response) = twin_boundary::route_request(method, path, body, kernel) {
        response?
    } else if let Some(companion_id) = path
        .strip_prefix("/twin/")
        .and_then(|rest| rest.strip_suffix(".json"))
    {
        if let Some(response) = reject_unless_method(method, "GET") {
            response
        } else {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            RouteResponse::json(
                "200 OK",
                kernel.twin_text_response(companion_id).render_json(),
            )
        }
    } else if let Some(companion_id) = path.strip_prefix("/twin/") {
        if let Some(response) = reject_unless_method(method, "GET") {
            response
        } else {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            RouteResponse::text(
                "200 OK",
                kernel.twin_text_response(companion_id).render_text(),
            )
        }
    } else if path == "/twin.json" {
        if let Some(response) = reject_unless_method(method, "GET") {
            response
        } else {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            RouteResponse::json(
                "200 OK",
                kernel.twin_text_response("twin_advisor").render_json(),
            )
        }
    } else if path.starts_with("/twin") {
        if let Some(response) = reject_unless_method(method, "GET") {
            response
        } else {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            RouteResponse::text(
                "200 OK",
                kernel.twin_text_response("twin_advisor").render_text(),
            )
        }
    } else if path == "/strategy.json" {
        if let Some(response) = reject_unless_method(method, "GET") {
            response
        } else {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            RouteResponse::json(
                "200 OK",
                kernel.strategy_ratification_response().render_json(),
            )
        }
    } else if path.starts_with("/strategy") {
        if let Some(response) = reject_unless_method(method, "GET") {
            response
        } else {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            RouteResponse::text(
                "200 OK",
                kernel.strategy_ratification_response().render_text(),
            )
        }
    } else if path == "/product.json" {
        if let Some(response) = reject_unless_method(method, "GET") {
            response
        } else {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            RouteResponse::json(
                "200 OK",
                kernel.product_ratification_response().render_json(),
            )
        }
    } else if let Some(response) = v1_replacement::route_response(method, path, kernel) {
        response?
    } else if let Some(response) =
        v1_read_shadow_approval_request::route_response(method, path, body, kernel)
    {
        response?
    } else if let Some(response) = auth_tenant_policy::route_response(method, path, body, kernel) {
        response?
    } else if path == "/memory/records.json" {
        if let Some(response) = reject_unless_method(method, "GET") {
            response
        } else {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            RouteResponse::json("200 OK", memory_store::render_records_json(&kernel))
        }
    } else if path == "/memory/brain-map.json" {
        if let Some(response) = reject_unless_method(method, "GET") {
            response
        } else {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            RouteResponse::json("200 OK", memory_store::render_brain_map_json(&kernel))
        }
    } else if path == "/memory/brain-substrate.json" {
        if let Some(response) = reject_unless_method(method, "GET") {
            response
        } else {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            RouteResponse::json("200 OK", memory_store::render_brain_substrate_json(&kernel))
        }
    } else if path == "/memory/brain-runtime.json" {
        if let Some(response) = reject_unless_method(method, "GET") {
            response
        } else {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            RouteResponse::json("200 OK", memory_store::render_brain_runtime_json(&kernel))
        }
    } else if path == "/memory/graph.json" {
        if let Some(response) = reject_unless_method(method, "GET") {
            response
        } else {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            RouteResponse::json("200 OK", memory_store::render_graph_json(&kernel))
        }
    } else if path == "/memory/lifecycle.json" {
        if let Some(response) = reject_unless_method(method, "GET") {
            response
        } else {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            RouteResponse::json("200 OK", memory_store::render_lifecycle_json(&kernel))
        }
    } else if path == "/memory/lifecycle-actions.json" {
        if let Some(response) = reject_unless_method(method, "POST") {
            response
        } else {
            let mut kernel = kernel
                .write()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            let memory_id = studio::json_string_field(body, "memory_id").unwrap_or_else(|| {
                kernel
                    .memory_records()
                    .last()
                    .map(|record| record.memory_id.clone())
                    .unwrap_or_default()
            });
            let action =
                studio::json_string_field(body, "action").unwrap_or_else(|| "decay".to_string());
            let reason = studio::json_string_field(body, "reason")
                .unwrap_or_else(|| "local lifecycle route action".to_string());
            let correlation = CorrelationIds {
                tenant_id: TenantId::new(
                    studio::json_string_field(body, "tenant_id")
                        .unwrap_or_else(|| "local_tenant".to_string()),
                ),
                trace_id: TraceId::new(kernel.mint_id("trace")),
                actor_id: ActorId::new(
                    studio::json_string_field(body, "actor_id")
                        .unwrap_or_else(|| "human:local_user".to_string()),
                ),
                loop_id: LoopId::new("memory_lifecycle"),
                workflow_id: WorkflowId::new(kernel.mint_id("workflow")),
            };
            match kernel.apply_memory_lifecycle_action(&correlation, &memory_id, &action, &reason) {
                Ok(event) => RouteResponse::json(
                    "200 OK",
                    format!(
                        r#"{{"name":"mdx-memory-lifecycle-action","status":"APPLIED","memory_id":{},"action":{},"lifecycle_state":{},"receipt_id":{},"production_write_allowed":false}}"#,
                        json_string_literal(&event.memory_id),
                        json_string_literal(event.action),
                        json_string_literal(event.lifecycle_state),
                        json_string_literal(&event.receipt_id)
                    ),
                ),
                Err(message) => RouteResponse::json(
                    "400 Bad Request",
                    format!(
                        r#"{{"name":"mdx-memory-lifecycle-action","status":"REFUSED","reason":{},"production_write_allowed":false}}"#,
                        json_string_literal(&message)
                    ),
                ),
            }
        }
    } else if path == "/memory/lifecycle-evaluations.json" {
        if let Some(response) = reject_unless_method(method, "POST") {
            response
        } else {
            let mut kernel = kernel
                .write()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            let reason = studio::json_string_field(body, "reason")
                .unwrap_or_else(|| "local lifecycle evaluation".to_string());
            let correlation = CorrelationIds {
                tenant_id: TenantId::new(
                    studio::json_string_field(body, "tenant_id")
                        .unwrap_or_else(|| "local_tenant".to_string()),
                ),
                trace_id: TraceId::new(kernel.mint_id("trace")),
                actor_id: ActorId::new(
                    studio::json_string_field(body, "actor_id")
                        .unwrap_or_else(|| "human:local_user".to_string()),
                ),
                loop_id: LoopId::new("memory_lifecycle_eval"),
                workflow_id: WorkflowId::new(kernel.mint_id("workflow")),
            };
            match kernel.evaluate_memory_lifecycle(&correlation, &reason) {
                Ok(evaluation) => RouteResponse::json(
                    "200 OK",
                    format!(
                        r#"{{"name":"mdx-memory-lifecycle-evaluation","status":"EVALUATED","evaluation_id":{},"evaluated_memory_count":{},"stale_count":{},"contradiction_count":{},"supersession_count":{},"receipt_id":{},"trusted_time_required":true,"production_write_allowed":false}}"#,
                        json_string_literal(&evaluation.evaluation_id),
                        evaluation.evaluated_memory_count,
                        evaluation.stale_count,
                        evaluation.contradiction_count,
                        evaluation.supersession_count,
                        json_string_literal(&evaluation.receipt_id)
                    ),
                ),
                Err(message) => RouteResponse::json(
                    "400 Bad Request",
                    format!(
                        r#"{{"name":"mdx-memory-lifecycle-evaluation","status":"REFUSED","reason":{},"production_write_allowed":false}}"#,
                        json_string_literal(&message)
                    ),
                ),
            }
        }
    } else if path == "/memory/recall-rankings.json" {
        if let Some(response) = reject_unless_method(method, "GET") {
            response
        } else {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            RouteResponse::json("200 OK", memory_store::render_rankings_json(&kernel))
        }
    } else if path == "/memory/brain-evals.json" {
        if let Some(response) = reject_unless_method(method, "GET") {
            response
        } else {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            RouteResponse::json("200 OK", memory_store::render_evals_json(&kernel))
        }
    } else if path == "/memory/brain-eval-runs.json" {
        if let Some(response) = reject_unless_method(method, "POST") {
            response
        } else {
            let mut kernel = kernel
                .write()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            let reason = studio::json_string_field(body, "reason")
                .unwrap_or_else(|| "local memory brain eval".to_string());
            let correlation = CorrelationIds {
                tenant_id: TenantId::new(
                    studio::json_string_field(body, "tenant_id")
                        .unwrap_or_else(|| "local_tenant".to_string()),
                ),
                trace_id: TraceId::new(kernel.mint_id("trace")),
                actor_id: ActorId::new(
                    studio::json_string_field(body, "actor_id")
                        .unwrap_or_else(|| "human:local_user".to_string()),
                ),
                loop_id: LoopId::new("memory_eval"),
                workflow_id: WorkflowId::new(kernel.mint_id("workflow")),
            };
            match kernel.run_memory_brain_eval_harness(&correlation, &reason) {
                Ok(runs) => {
                    let score = runs
                        .iter()
                        .rev()
                        .find(|run| run.fixture_family == "MDx Brain Score")
                        .map(|run| run.brain_score)
                        .unwrap_or(0);
                    let source_receipt_ids = runs
                        .iter()
                        .map(|run| json_string_literal(&run.receipt_id))
                        .collect::<Vec<_>>()
                        .join(",");
                    RouteResponse::json(
                        "200 OK",
                        format!(
                            r#"{{"name":"mdx-memory-brain-eval-run","status":"MEASURED","run_count":{},"mdx_brain_score":{},"fixture_result_count":{},"source_receipt_ids":[{}],"production_write_allowed":false}}"#,
                            runs.len(),
                            score,
                            kernel.memory_eval_fixture_results().len(),
                            source_receipt_ids
                        ),
                    )
                }
                Err(message) => RouteResponse::json(
                    "400 Bad Request",
                    format!(
                        r#"{{"name":"mdx-memory-brain-eval-run","status":"REFUSED","reason":{},"production_write_allowed":false}}"#,
                        json_string_literal(&message)
                    ),
                ),
            }
        }
    } else if path == "/memory/governance.json" {
        if let Some(response) = reject_unless_method(method, "GET") {
            response
        } else {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            RouteResponse::json("200 OK", memory_store::render_governance_json(&kernel))
        }
    } else if path == "/memory/vendor-comparators.json" {
        if let Some(response) = reject_unless_method(method, "GET") {
            response
        } else {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            RouteResponse::json("200 OK", memory_store::render_comparators_json(&kernel))
        }
    } else if path == "/memory/topology.json" {
        if let Some(response) = reject_unless_method(method, "GET") {
            response
        } else {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            RouteResponse::json("200 OK", memory_store::render_topology_json(&kernel))
        }
    } else if path == "/memory/topology-validations.json" {
        if let Some(response) = reject_unless_method(method, "POST") {
            response
        } else {
            let mut kernel = kernel
                .write()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            let reason = studio::json_string_field(body, "reason")
                .unwrap_or_else(|| "local topology validation".to_string());
            let correlation = CorrelationIds {
                tenant_id: TenantId::new(
                    studio::json_string_field(body, "tenant_id")
                        .unwrap_or_else(|| "local_tenant".to_string()),
                ),
                trace_id: TraceId::new(kernel.mint_id("trace")),
                actor_id: ActorId::new(
                    studio::json_string_field(body, "actor_id")
                        .unwrap_or_else(|| "human:local_user".to_string()),
                ),
                loop_id: LoopId::new("memory_topology"),
                workflow_id: WorkflowId::new(kernel.mint_id("workflow")),
            };
            match kernel.run_memory_topology_validation(&correlation, &reason) {
                Ok(events) => {
                    let source_receipt_ids = events
                        .iter()
                        .map(|event| json_string_literal(&event.receipt_id))
                        .collect::<Vec<_>>()
                        .join(",");
                    RouteResponse::json(
                        "200 OK",
                        format!(
                            r#"{{"name":"mdx-memory-topology-validation","status":"VALIDATED","runtime_event_count":{},"latency_budget_ms":250,"source_receipt_ids":[{}],"production_write_allowed":false}}"#,
                            events.len(),
                            source_receipt_ids
                        ),
                    )
                }
                Err(message) => RouteResponse::json(
                    "400 Bad Request",
                    format!(
                        r#"{{"name":"mdx-memory-topology-validation","status":"REFUSED","reason":{},"production_write_allowed":false}}"#,
                        json_string_literal(&message)
                    ),
                ),
            }
        }
    } else if path == "/memory/beta-readiness.json" {
        if let Some(response) = reject_unless_method(method, "GET") {
            response
        } else {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            RouteResponse::json("200 OK", memory_store::render_beta_readiness_json(&kernel))
        }
    } else if path == "/memory/beta-readiness-runs.json" {
        if let Some(response) = reject_unless_method(method, "POST") {
            response
        } else {
            let mut kernel = kernel
                .write()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            let sessions = studio::json_string_field(body, "synthetic_sessions")
                .and_then(|value| value.parse::<u16>().ok())
                .unwrap_or(1000);
            let correlation = CorrelationIds {
                tenant_id: TenantId::new(
                    studio::json_string_field(body, "tenant_id")
                        .unwrap_or_else(|| "local_tenant".to_string()),
                ),
                trace_id: TraceId::new(kernel.mint_id("trace")),
                actor_id: ActorId::new(
                    studio::json_string_field(body, "actor_id")
                        .unwrap_or_else(|| "human:local_user".to_string()),
                ),
                loop_id: LoopId::new("memory_beta_readiness"),
                workflow_id: WorkflowId::new(kernel.mint_id("workflow")),
            };
            match kernel.run_memory_beta_readiness(&correlation, sessions) {
                Ok(run) => RouteResponse::json(
                    "200 OK",
                    format!(
                        r#"{{"name":"mdx-memory-beta-readiness-run","status":"LOCAL_SYNTHETIC_DRILL_COMPLETE","scale_run_id":{},"synthetic_session_count":{},"memory_record_count":{},"ranking_count":{},"latency_budget_ms":{},"observed_p95_latency_ms":{},"brain_score":{},"source_receipt_ids":[{}],"owned_stack_canonical":true,"vendor_dependency_required":false,"production_write_allowed":false}}"#,
                        json_string_literal(&run.scale_run_id),
                        run.synthetic_session_count,
                        run.memory_record_count,
                        run.ranking_count,
                        run.latency_budget_ms,
                        run.observed_p95_latency_ms,
                        run.brain_score,
                        json_string_literal(&run.receipt_id)
                    ),
                ),
                Err(message) => RouteResponse::json(
                    "400 Bad Request",
                    format!(
                        r#"{{"name":"mdx-memory-beta-readiness-run","status":"REFUSED","reason":{},"production_write_allowed":false}}"#,
                        json_string_literal(&message)
                    ),
                ),
            }
        }
    } else if path.starts_with("/product") {
        if let Some(response) = reject_unless_method(method, "GET") {
            response
        } else {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            RouteResponse::text(
                "200 OK",
                kernel.product_ratification_response().render_text(),
            )
        }
    } else if let Some(response) = pages_publication::route_response(method, path, body, kernel) {
        response?
    } else if let Some(response) = studio::route_response(method, path, body, kernel) {
        response?
    } else if let Some(response) = studio_steering::route_response(method, path, body, kernel) {
        response?
    } else if let Some(response) = install_model_connect::route_response(method, path, body, kernel)
    {
        response?
    } else if let Some(response) = install_owner::route_response(method, path, body, kernel) {
        response?
    } else if let Some(response) = install_setup_track::route_response(method, path, body, kernel) {
        response?
    } else if let Some(response) =
        install_first_run_profile::route_response(method, path, body, kernel)
    {
        response?
    } else if let Some(response) = pages_edit_draft::route_response(method, path, body, kernel) {
        response?
    } else if let Some(response) =
        pages_approval_request::route_response(method, path, body, kernel)
    {
        response?
    } else if let Some(response) = marketplace::route_response(method, path, body, kernel) {
        response
    } else if let Some(response) = pages_lifecycle::route_response(method, path, body, kernel) {
        response?
    } else if let Some(response) = pages_context_sources::route_response(method, path, body, kernel)
    {
        response?
    } else if let Some(response) = pages_world_model::route_response(method, path, body, kernel) {
        response?
    } else if let Some(response) = pages_decision_graph::route_response(method, path, body, kernel)
    {
        response?
    } else if let Some(response) = pages_search::route_response(method, path, body, kernel) {
        response?
    } else if let Some(response) = pages_runtime_readiness::route_response(method, path, kernel) {
        response?
    } else if let Some(response) = message_routes::route_response(method, path, body, kernel) {
        response?
    } else if path == "/pages.json" {
        if let Some(response) = reject_unless_method(method, "GET") {
            response
        } else {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            RouteResponse::json("200 OK", render_pages_list_json(&kernel))
        }
    } else if let Some(document_id) = path
        .strip_prefix("/pages/")
        .and_then(|value| value.strip_suffix("/body"))
    {
        if let Some(response) = reject_unless_method(method, "GET") {
            response
        } else if has_known_pages_document_body(document_id) {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            RouteResponse::json(
                "200 OK",
                render_pages_document_body_json(&kernel, document_id),
            )
        } else {
            // Authored pages: kernel-published documents serve from their
            // recorded store reference.
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            match pages_authored::authored_body_json(&kernel, document_id) {
                Some(json) => RouteResponse::json("200 OK", json),
                None => RouteResponse::text("404 Not Found", "page body not found\n".to_string()),
            }
        }
    } else if let Some(document_id) = path.strip_prefix("/pages/") {
        if let Some(response) = reject_unless_method(method, "GET") {
            response
        } else if is_known_pages_document(document_id) {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            RouteResponse::json("200 OK", render_pages_document_json(&kernel, document_id))
        } else {
            RouteResponse::text("404 Not Found", "page not found\n".to_string())
        }
    } else if path == "/messages/threads.json" {
        if let Some(response) = reject_unless_method(method, "GET") {
            response
        } else {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            RouteResponse::json("200 OK", render_message_threads_json(&kernel))
        }
    } else if let Some(channel_id) = path
        .strip_prefix("/messages/channels/")
        .and_then(|rest| rest.strip_suffix(".json"))
    {
        if let Some(response) = reject_unless_method(method, "GET") {
            response
        } else {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            RouteResponse::json("200 OK", render_message_channel_json(&kernel, channel_id))
        }
    } else if let Some(response) = forge_ship_decision::route_response(method, path, body, kernel) {
        response?
    } else if path == "/run-loop/evals_runner_agent" {
        if let Some(response) = reject_unless_method(method, "POST") {
            response
        } else {
            let mut kernel = kernel
                .write()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            let runner = LocalLoopRunner;
            let report = runner.run_evals_runner_agent(&mut kernel)?;
            RouteResponse::text("200 OK", render_report(&report))
        }
    } else if path == "/run-loop/aegis_scanner_agent" {
        if let Some(response) = reject_unless_method(method, "POST") {
            response
        } else {
            let mut kernel = kernel
                .write()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            let runner = LocalLoopRunner;
            let report = runner.run_aegis_scanner_agent(&mut kernel)?;
            RouteResponse::text("200 OK", render_report(&report))
        }
    } else if path == "/run-loop/charter_attestation_agent" {
        if let Some(response) = reject_unless_method(method, "POST") {
            response
        } else {
            let mut kernel = kernel
                .write()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            let runner = LocalLoopRunner;
            let report = runner.run_charter_attestation_agent(&mut kernel)?;
            RouteResponse::text("200 OK", render_report(&report))
        }
    } else if path == "/run-loop/forge_orchestrator_agent" {
        if let Some(response) = reject_unless_method(method, "POST") {
            response
        } else {
            let mut kernel = kernel
                .write()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            let runner = LocalLoopRunner;
            let report = runner.run_forge_orchestrator_agent(&mut kernel)?;
            RouteResponse::text("200 OK", render_report(&report))
        }
    } else if path == "/run-loop/product_shaping_agent" {
        if let Some(response) = reject_unless_method(method, "POST") {
            response
        } else {
            let mut kernel = kernel
                .write()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            let runner = LocalLoopRunner;
            let report = runner.run_product_shaping_agent(&mut kernel)?;
            RouteResponse::text("200 OK", render_report(&report))
        }
    } else if path == "/run-loop/talent_autonomy_agent" {
        if let Some(response) = reject_unless_method(method, "POST") {
            response
        } else {
            let mut kernel = kernel
                .write()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            let runner = LocalLoopRunner;
            let report = runner.run_talent_autonomy_agent(&mut kernel)?;
            RouteResponse::text("200 OK", render_report(&report))
        }
    } else {
        RouteResponse::text("404 Not Found", "not found\n".to_string())
    };
    Ok(response)
}
pub(crate) fn reject_unless_method(method: &str, expected: &str) -> Option<RouteResponse> {
    if method.eq_ignore_ascii_case(expected) {
        None
    } else {
        Some(RouteResponse::text(
            "405 Method Not Allowed",
            "method not allowed\n".to_string(),
        ))
    }
}
#[cfg(test)]
mod tests;
