use std::collections::BTreeMap;
use std::io::Write;

use crate::{
    MESSAGE_THREAD_MESSAGE_POSTED_KINDS, MemoryRecord, POLICY_DECISION_RECEIPT_KIND,
    PostgresMigrationEvidence, Receipt, StorageAdapterError, migration_report,
    render_postgres_chain_head_upsert_sql, render_postgres_receipt_insert_sql, sql_string_literal,
};

const APP_STATE_TABLES: [&str; 70] = [
    "model_provider_connections",
    "model_catalog_models",
    "model_deployments",
    "model_price_observations",
    "model_route_policies",
    "model_route_decisions",
    "model_outcomes",
    "model_adaptive_policy_versions",
    "model_adaptive_comparisons",
    "message_threads",
    "message_thread_messages",
    "message_fanout_requests",
    "message_presence_records",
    "message_channels",
    "message_channel_members",
    "message_thread_participants",
    "message_envelopes",
    "message_realtime_cutover_preflights",
    "message_delivery_replay_batches",
    "message_subscription_isolation_checks",
    "message_service_role_fanout_refusals",
    "pages_documents",
    "pages_revisions",
    "pages_publications",
    "pages_approval_requests",
    "pages_search_preflights",
    "pages_citations",
    "pages_publication_targets",
    "pages_revision_citations",
    "pages_attachments",
    "pages_search_index_records",
    "twin_sessions",
    "twin_session_messages",
    "twin_memory_snapshots",
    "twin_model_traces",
    "twin_conversation_sessions",
    "twin_companion_stance_records",
    "twin_memory_retrievals",
    "twin_grounded_answers",
    "twin_conversation_summaries",
    "forge_ci_evidence_preflights",
    "forge_human_ratification_preflights",
    "forge_deployment_preflights",
    "forge_outcome_signals",
    "marketplace_acts",
    "marketplace_installed_capabilities",
    "product_signals",
    "product_bet_drafts",
    "product_handoff_requests",
    "eval_guardrail_verdicts",
    "security_findings",
    "charter_attestations",
    "strategy_ratification_snapshots",
    "talent_sponsor_chain_authorities",
    "talent_worker_lease_authorities",
    "talent_budget_authorities",
    "talent_tool_allowlist_authorities",
    "talent_worker_spawn_requests",
    "worker_runtime_handoffs",
    "worker_runtime_retirements",
    "observatory_role_view_snapshots",
    "treasury_reserve_postures",
    "auth_tenant_orgs",
    "auth_tenant_memberships",
    "auth_role_mappings",
    "auth_invite_states",
    "auth_visibility_policies",
    "auth_approved_model_policies",
    "auth_session_evidence",
    "auth_tenant_policy_preflights",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PostgresAppStateWriter {
    database_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PostgresAppStateWriterContract {
    pub adapter: &'static str,
    pub database_url_required: bool,
    pub observed_migrations_required: bool,
    pub table_count: usize,
    pub writes_live_database: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PostgresAppStateWriteReport {
    pub app_state_tables: usize,
    pub ledger_receipts: usize,
    pub memory_records: usize,
}

pub fn render_postgres_app_state_export_sql(
    receipts: &[Receipt],
    memory: &[MemoryRecord],
) -> String {
    let mut sql = String::from("-- mdx governed app-state local export proof\n");
    append_identity_sql(&mut sql, receipts);
    append_receipt_sql(&mut sql, receipts);
    append_memory_sql(&mut sql, memory);
    append_model_fabric_sql(&mut sql, receipts);
    append_message_sql(&mut sql, receipts);
    append_pages_sql(&mut sql, receipts);
    append_twin_sql(&mut sql, receipts, memory);
    append_forge_sql(&mut sql, receipts);
    append_marketplace_sql(&mut sql, receipts);
    append_product_governance_sql(&mut sql, receipts);
    append_strategy_talent_observatory_sql(&mut sql, receipts);
    append_auth_tenant_sql(&mut sql, receipts);
    for table in APP_STATE_TABLES {
        sql.push_str(&format!("SELECT count(*) FROM {table};\n"));
    }
    sql
}

impl PostgresAppStateWriter {
    pub fn connect(database_url: Option<&str>) -> Result<Self, StorageAdapterError> {
        let database_url = database_url
            .filter(|value| !value.trim().is_empty())
            .ok_or(StorageAdapterError::MissingDatabaseUrl)?;
        let _candidate = Self {
            database_url: database_url.to_string(),
        };
        Err(StorageAdapterError::PendingLiveRun {
            adapter: Self::adapter_name(),
            reason: "durable app-state writes have not observed local Postgres migrations",
        })
    }

    pub fn connect_after_observed_migrations(
        database_url: Option<&str>,
        evidence: PostgresMigrationEvidence,
    ) -> Result<Self, StorageAdapterError> {
        let database_url = database_url
            .filter(|value| !value.trim().is_empty())
            .ok_or(StorageAdapterError::MissingDatabaseUrl)?;
        let expected = migration_report();
        if evidence.migration_count != expected.migration_count
            || evidence.tenant_owned_tables != expected.tenant_owned_tables
            || evidence.rls_enabled_tables != expected.rls_enabled_tables
        {
            return Err(StorageAdapterError::MigrationEvidenceMismatch {
                expected_migrations: expected.migration_count,
                observed_migrations: evidence.migration_count,
                expected_tenant_owned_tables: expected.tenant_owned_tables,
                observed_tenant_owned_tables: evidence.tenant_owned_tables,
                expected_rls_enabled_tables: expected.rls_enabled_tables,
                observed_rls_enabled_tables: evidence.rls_enabled_tables,
            });
        }
        Ok(Self {
            database_url: database_url.to_string(),
        })
    }

    pub fn adapter_name() -> &'static str {
        "PostgresAppStateWriter"
    }

    pub fn database_url(&self) -> &str {
        &self.database_url
    }

    pub fn contract() -> PostgresAppStateWriterContract {
        PostgresAppStateWriterContract {
            adapter: Self::adapter_name(),
            database_url_required: true,
            observed_migrations_required: true,
            table_count: APP_STATE_TABLES.len(),
            writes_live_database: true,
        }
    }

    pub fn render_app_state_write_sql(
        &self,
        receipts: &[Receipt],
        memory: &[MemoryRecord],
    ) -> String {
        render_postgres_app_state_export_sql(receipts, memory)
    }

    pub fn write_app_state_live(
        &self,
        receipts: &[Receipt],
        memory: &[MemoryRecord],
    ) -> Result<PostgresAppStateWriteReport, StorageAdapterError> {
        let mut child = std::process::Command::new("psql")
            .arg(&self.database_url)
            .arg("-v")
            .arg("ON_ERROR_STOP=1")
            .arg("-q")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(Self::transport_error)?;
        let mut sql = String::from("BEGIN;\n");
        if let Some(receipt) = receipts.first() {
            sql.push_str(&format!(
                "SET LOCAL mdx.tenant_id = {};\n",
                sql_string_literal(receipt.tenant_id.as_str())
            ));
        }
        sql.push_str(&self.render_app_state_write_sql(receipts, memory));
        sql.push_str("COMMIT;\n");
        child
            .stdin
            .as_mut()
            .ok_or_else(|| Self::transport_message("psql stdin was not available"))?
            .write_all(sql.as_bytes())
            .map_err(Self::transport_error)?;
        let output = child.wait_with_output().map_err(Self::transport_error)?;
        if !output.status.success() {
            return Err(Self::transport_message(&String::from_utf8_lossy(
                &output.stderr,
            )));
        }
        Ok(PostgresAppStateWriteReport {
            app_state_tables: APP_STATE_TABLES.len(),
            ledger_receipts: receipts.len(),
            memory_records: memory.len(),
        })
    }

    fn transport_error(error: impl std::fmt::Display) -> StorageAdapterError {
        Self::transport_message(&error.to_string())
    }

    fn transport_message(message: &str) -> StorageAdapterError {
        StorageAdapterError::LiveTransport {
            adapter: Self::adapter_name(),
            message: message.trim().to_string(),
        }
    }
}

fn append_identity_sql(sql: &mut String, receipts: &[Receipt]) {
    let mut actors = BTreeMap::new();
    for receipt in receipts {
        actors.insert(
            receipt.actor_id.as_str().to_string(),
            receipt.tenant_id.as_str().to_string(),
        );
    }
    for tenant_id in actors.values() {
        sql.push_str(&format!(
            "INSERT INTO tenants (tenant_id, name) VALUES ({}, {}) ON CONFLICT (tenant_id) DO NOTHING;\n",
            sql_string_literal(tenant_id),
            sql_string_literal("MDx local tenant")
        ));
    }
    for (actor_id, tenant_id) in actors {
        sql.push_str(&format!(
            "INSERT INTO actors (actor_id, tenant_id, display_name, role) VALUES ({}, {}, {}, {}) ON CONFLICT (actor_id) DO NOTHING;\n",
            sql_string_literal(&actor_id),
            sql_string_literal(&tenant_id),
            sql_string_literal(&actor_id),
            sql_string_literal("operator")
        ));
    }
}

fn append_receipt_sql(sql: &mut String, receipts: &[Receipt]) {
    for receipt in receipts {
        sql.push_str(&render_postgres_receipt_insert_sql(receipt));
    }
    let mut chain_heads: BTreeMap<&str, &Receipt> = BTreeMap::new();
    for receipt in receipts {
        chain_heads.insert(receipt.tenant_id.as_str(), receipt);
    }
    for receipt in chain_heads.values() {
        sql.push_str(&render_postgres_chain_head_upsert_sql(receipt));
    }
}

fn append_memory_sql(sql: &mut String, memory: &[MemoryRecord]) {
    for record in memory {
        sql.push_str(&format!(
            "-- memory_episode episode_id={} scope={} tier={} origin={} valid_from_receipt_timestamp={} decay_policy={} importance_score={}\n",
            record.episode_id,
            record.memory_scope,
            record.memory_tier,
            record.atom_origin,
            record.valid_from_receipt_timestamp,
            record.decay_policy,
            record.importance_score
        ));
        sql.push_str(&format!(
            "INSERT INTO memory_records (memory_id, tenant_id, source_receipt_id, atom_origin, valid_from_receipt_timestamp, memory_scope, memory_tier, decay_policy, importance_score, consolidation_decision, content) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}) ON CONFLICT (memory_id) DO UPDATE SET source_receipt_id = EXCLUDED.source_receipt_id, atom_origin = EXCLUDED.atom_origin, valid_from_receipt_timestamp = EXCLUDED.valid_from_receipt_timestamp, memory_scope = EXCLUDED.memory_scope, memory_tier = EXCLUDED.memory_tier, decay_policy = EXCLUDED.decay_policy, importance_score = EXCLUDED.importance_score, consolidation_decision = EXCLUDED.consolidation_decision, content = EXCLUDED.content;\n",
            sql_string_literal(&record.memory_id),
            sql_string_literal(record.tenant_id.as_str()),
            sql_string_literal(&record.source_receipt_id),
            sql_string_literal(record.atom_origin),
            sql_string_literal(&record.valid_from_receipt_timestamp),
            sql_string_literal(record.memory_scope),
            sql_string_literal(record.memory_tier),
            sql_string_literal(record.decay_policy),
            record.importance_score,
            sql_string_literal(record.consolidation_decision.as_str()),
            sql_string_literal(&record.content)
        ));
    }
}

fn append_model_fabric_sql(sql: &mut String, receipts: &[Receipt]) {
    for receipt in receipts_by_kind(receipts, "model.connection.configured") {
        sql.push_str(&format!(
            "INSERT INTO model_provider_connections (connection_id, tenant_id, source_receipt_id, provider_id, credential_ref, endpoint_base_url, region, residency, data_retention, training_policy, data_policy_provenance, data_policy_observed_at, health, live_call_allowed, secret_value_recorded) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, false) ON CONFLICT (connection_id) DO UPDATE SET source_receipt_id = EXCLUDED.source_receipt_id, credential_ref = EXCLUDED.credential_ref, endpoint_base_url = EXCLUDED.endpoint_base_url, region = EXCLUDED.region, residency = EXCLUDED.residency, data_retention = EXCLUDED.data_retention, training_policy = EXCLUDED.training_policy, data_policy_provenance = EXCLUDED.data_policy_provenance, data_policy_observed_at = EXCLUDED.data_policy_observed_at, health = EXCLUDED.health, live_call_allowed = EXCLUDED.live_call_allowed, updated_at = now();\n",
            sql_string_literal(payload(receipt, "connection_id")),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(payload(receipt, "provider_id")),
            sql_string_literal(payload(receipt, "credential_ref")),
            sql_string_literal(payload(receipt, "endpoint_base_url")),
            sql_string_literal(payload(receipt, "region")),
            sql_string_literal(payload(receipt, "residency")),
            sql_string_literal(payload(receipt, "data_retention")),
            sql_string_literal(payload(receipt, "training_policy")),
            sql_string_literal(payload(receipt, "data_policy_provenance")),
            sql_string_literal(payload(receipt, "data_policy_observed_at")),
            sql_string_literal(payload(receipt, "health")),
            payload(receipt, "live_call_allowed") == "true",
        ));
    }
    for receipt in receipts_by_kind(receipts, "model.deployment.registered") {
        sql.push_str(&format!(
            "INSERT INTO model_catalog_models (model_id, tenant_id, source_receipt_id, provider_model_id, provider_id, display_name, lifecycle, modality) VALUES ({}, {}, {}, {}, {}, {}, {}, {}) ON CONFLICT (tenant_id, model_id) DO UPDATE SET source_receipt_id = EXCLUDED.source_receipt_id, provider_model_id = EXCLUDED.provider_model_id, display_name = EXCLUDED.display_name, lifecycle = EXCLUDED.lifecycle, modality = EXCLUDED.modality, updated_at = now();\n",
            sql_string_literal(payload(receipt, "model_id")),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(payload(receipt, "provider_model_id")),
            sql_string_literal(payload(receipt, "provider_id")),
            sql_string_literal(payload(receipt, "display_name")),
            sql_string_literal(payload(receipt, "lifecycle")),
            sql_string_literal(payload(receipt, "modality")),
        ));
        sql.push_str(&format!(
            "INSERT INTO model_deployments (deployment_id, tenant_id, source_receipt_id, connection_id, provider_id, model_id, privacy_class, region, residency, enabled, tools, structured_output, vision, context_tokens, capability_provenance, capability_observed_at) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}) ON CONFLICT (deployment_id) DO UPDATE SET source_receipt_id = EXCLUDED.source_receipt_id, privacy_class = EXCLUDED.privacy_class, region = EXCLUDED.region, residency = EXCLUDED.residency, enabled = EXCLUDED.enabled, tools = EXCLUDED.tools, structured_output = EXCLUDED.structured_output, vision = EXCLUDED.vision, context_tokens = EXCLUDED.context_tokens, capability_provenance = EXCLUDED.capability_provenance, capability_observed_at = EXCLUDED.capability_observed_at, updated_at = now();\n",
            sql_string_literal(payload(receipt, "deployment_id")),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(payload(receipt, "connection_id")),
            sql_string_literal(payload(receipt, "provider_id")),
            sql_string_literal(payload(receipt, "model_id")),
            sql_string_literal(payload(receipt, "privacy_class")),
            sql_string_literal(payload(receipt, "region")),
            sql_string_literal(payload(receipt, "residency")),
            payload(receipt, "enabled") == "true",
            payload(receipt, "tools") == "true",
            payload(receipt, "structured_output") == "true",
            payload(receipt, "vision") == "true",
            payload(receipt, "context_tokens").parse::<u64>().unwrap_or(0),
            sql_string_literal(payload(receipt, "capability_provenance")),
            sql_string_literal(payload(receipt, "capability_observed_at")),
        ));
        sql.push_str(&format!(
            "INSERT INTO model_price_observations (price_observation_id, tenant_id, source_receipt_id, deployment_id, input_microusd_per_million, output_microusd_per_million, currency, source, observed_at) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}) ON CONFLICT (price_observation_id) DO NOTHING;\n",
            sql_string_literal(&format!("{}_price", receipt.receipt_id)),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(payload(receipt, "deployment_id")),
            payload(receipt, "input_microusd_per_million").parse::<u64>().unwrap_or(0),
            payload(receipt, "output_microusd_per_million").parse::<u64>().unwrap_or(0),
            sql_string_literal(payload(receipt, "currency")),
            sql_string_literal(payload(receipt, "price_source")),
            sql_string_literal(payload(receipt, "price_observed_at")),
        ));
    }
    for receipt in receipts_by_kind(receipts, "model.route_policy.configured") {
        let policy_key = format!(
            "{}:{}:{}",
            receipt.tenant_id.as_str(),
            payload(receipt, "policy_id"),
            payload(receipt, "policy_version")
        );
        let policy_json = format!(
            "allowed_providers={};preferred_deployments={};denied_providers={};denied_deployments={};allowed_regions={};required_residency={};allow_retention={};allow_training={}",
            payload(receipt, "allowed_provider_ids"),
            payload(receipt, "preferred_deployment_ids"),
            payload(receipt, "denied_provider_ids"),
            payload(receipt, "denied_deployment_ids"),
            payload(receipt, "allowed_regions"),
            payload(receipt, "required_residency"),
            payload(receipt, "allow_data_retention"),
            payload(receipt, "allow_training"),
        );
        sql.push_str(&format!(
            "INSERT INTO model_route_policies (policy_key, tenant_id, source_receipt_id, policy_id, policy_version, lifecycle, workload_ids, canary_percent, app_id, environment, policy_json, state) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}) ON CONFLICT (policy_key) DO UPDATE SET source_receipt_id = EXCLUDED.source_receipt_id, lifecycle = EXCLUDED.lifecycle, workload_ids = EXCLUDED.workload_ids, canary_percent = EXCLUDED.canary_percent, policy_json = EXCLUDED.policy_json, state = EXCLUDED.state, updated_at = now();\n",
            sql_string_literal(&policy_key),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(payload(receipt, "policy_id")),
            sql_string_literal(payload(receipt, "policy_version")),
            sql_string_literal(payload(receipt, "lifecycle")),
            sql_string_literal(payload(receipt, "workload_ids")),
            payload(receipt, "canary_percent").parse::<u8>().unwrap_or(0),
            sql_string_literal(payload(receipt, "allowed_app_ids")),
            sql_string_literal(payload(receipt, "allowed_environments")),
            sql_string_literal(&policy_json),
            sql_string_literal(payload(receipt, "state")),
        ));
    }
    for receipt in receipts.iter().filter(|receipt| {
        matches!(
            receipt.kind.as_str(),
            "model.route.selected" | "model.route.denied"
        )
    }) {
        sql.push_str(&format!(
            "INSERT INTO model_route_decisions (decision_id, tenant_id, source_receipt_id, workload_id, app_id, environment, policy_id, policy_version, preset, selected_deployment_id, selection_reason, session_id, session_sticky_applied, provider_failover_deployment_ids, model_fallback_deployment_ids, exclusions_json, grants_execution_authority) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, false) ON CONFLICT (decision_id) DO NOTHING;\n",
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(payload(receipt, "workload_id")),
            sql_string_literal(payload(receipt, "app_id")),
            sql_string_literal(payload(receipt, "environment")),
            sql_string_literal(payload(receipt, "policy_id")),
            sql_string_literal(payload(receipt, "policy_version")),
            sql_string_literal(payload(receipt, "preset")),
            sql_optional_string(receipt.payload.get("selected_deployment_id").map(String::as_str).filter(|value| !value.is_empty())),
            sql_string_literal(payload(receipt, "selection_reason")),
            sql_string_literal(payload(receipt, "session_id")),
            payload(receipt, "session_sticky_applied") == "true",
            sql_string_literal(payload(receipt, "provider_failover_deployment_ids")),
            sql_string_literal(payload(receipt, "model_fallback_deployment_ids")),
            sql_string_literal(payload(receipt, "exclusions")),
        ));
    }
    for receipt in receipts_by_kind(receipts, "model.outcome.recorded") {
        sql.push_str(&format!(
            "INSERT INTO model_outcomes (outcome_id, tenant_id, source_receipt_id, decision_id, workload_id, deployment_id, latency_ms, cost_microusd, quality_score, safety_status, task_status, correction_status, provenance) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}) ON CONFLICT (outcome_id) DO NOTHING;\n",
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(payload(receipt, "decision_id")),
            sql_string_literal(payload(receipt, "workload_id")),
            sql_string_literal(payload(receipt, "deployment_id")),
            payload(receipt, "latency_ms").parse::<u64>().unwrap_or(0),
            payload(receipt, "cost_microusd").parse::<u64>().map(|cost| cost.to_string()).unwrap_or_else(|_| "NULL".to_string()),
            payload(receipt, "quality_score").parse::<u32>().map(|score| score.to_string()).unwrap_or_else(|_| "NULL".to_string()),
            sql_string_literal(payload(receipt, "safety_status")),
            sql_string_literal(payload(receipt, "task_status")),
            sql_string_literal(payload(receipt, "correction_status")),
            sql_string_literal(payload(receipt, "provenance")),
        ));
    }
    for receipt in receipts.iter().filter(|receipt| {
        matches!(
            receipt.kind.as_str(),
            "model.adaptive_policy.replay" | "model.adaptive_policy.shadow"
        )
    }) {
        let comparison_kind = if receipt.kind.ends_with("replay") {
            "replay"
        } else {
            "shadow"
        };
        sql.push_str(&format!(
            "INSERT INTO model_adaptive_comparisons (comparison_id, tenant_id, source_receipt_id, comparison_kind, policy_id, policy_version, baseline_decision_id, baseline_deployment_id, candidate_status, candidate_deployment_id, candidate_changed_route, grants_execution_authority) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, false) ON CONFLICT (comparison_id) DO NOTHING;\n",
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(comparison_kind),
            sql_string_literal(payload(receipt, "policy_id")),
            sql_string_literal(payload(receipt, "policy_version")),
            sql_string_literal(payload(receipt, "baseline_decision_id")),
            sql_string_literal(payload(receipt, "baseline_deployment_id")),
            sql_string_literal(payload(receipt, "candidate_status")),
            sql_optional_string(receipt.payload.get("candidate_deployment_id").map(String::as_str).filter(|value| !value.is_empty())),
            payload(receipt, "candidate_changed_route") == "true",
        ));
    }
    for receipt in receipts_by_kind(receipts, "model.adaptive_policy.evaluated") {
        let policy_key = format!(
            "{}:{}:{}",
            receipt.tenant_id.as_str(),
            payload(receipt, "policy_id"),
            payload(receipt, "policy_version")
        );
        sql.push_str(&format!(
            "INSERT INTO model_adaptive_policy_versions (adaptive_policy_key, tenant_id, source_receipt_id, policy_id, policy_version, state, replay_cases, shadow_decisions, canary_decisions, guardrail_evidence_sufficient, guardrails_passed, grants_execution_authority) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, false) ON CONFLICT (adaptive_policy_key) DO UPDATE SET source_receipt_id = EXCLUDED.source_receipt_id, state = EXCLUDED.state, replay_cases = EXCLUDED.replay_cases, shadow_decisions = EXCLUDED.shadow_decisions, canary_decisions = EXCLUDED.canary_decisions, guardrail_evidence_sufficient = EXCLUDED.guardrail_evidence_sufficient, guardrails_passed = EXCLUDED.guardrails_passed, updated_at = now();\n",
            sql_string_literal(&policy_key),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(payload(receipt, "policy_id")),
            sql_string_literal(payload(receipt, "policy_version")),
            sql_string_literal(payload(receipt, "state")),
            payload(receipt, "replay_cases").parse::<u64>().unwrap_or(0),
            payload(receipt, "shadow_decisions").parse::<u64>().unwrap_or(0),
            payload(receipt, "canary_decisions").parse::<u64>().unwrap_or(0),
            payload(receipt, "guardrail_evidence_sufficient") == "true",
            payload(receipt, "guardrails_passed") == "true",
        ));
        let route_policy_key = format!(
            "{}:{}:{}",
            receipt.tenant_id.as_str(),
            payload(receipt, "policy_id"),
            payload(receipt, "policy_version")
        );
        sql.push_str(&format!(
            "UPDATE model_route_policies SET state = {}, source_receipt_id = {}, updated_at = now() WHERE policy_key = {};\n",
            sql_string_literal(payload(receipt, "state")),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(&route_policy_key),
        ));
    }
    for receipt in receipts.iter().filter(|receipt| {
        matches!(
            receipt.kind.as_str(),
            "model.adaptive_policy.promoted" | "model.adaptive_policy.auto_rollback"
        )
    }) {
        let policy_key = format!(
            "{}:{}:{}",
            receipt.tenant_id.as_str(),
            payload(receipt, "policy_id"),
            payload(receipt, "policy_version")
        );
        sql.push_str(&format!(
            "UPDATE model_adaptive_policy_versions SET state = {}, source_receipt_id = {}, updated_at = now() WHERE adaptive_policy_key = {};\nUPDATE model_route_policies SET state = {}, source_receipt_id = {}, updated_at = now() WHERE policy_key = {};\n",
            sql_string_literal(payload(receipt, "state")),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(&policy_key),
            sql_string_literal(payload(receipt, "state")),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(&policy_key),
        ));
    }
}

fn append_message_sql(sql: &mut String, receipts: &[Receipt]) {
    for receipt in MESSAGE_THREAD_MESSAGE_POSTED_KINDS
        .iter()
        .flat_map(|kind| receipts_by_kind(receipts, kind))
    {
        let thread_id = payload(receipt, "thread_id");
        let channel_id = payload(receipt, "channel_id");
        let message_id = payload(receipt, "message_id");
        let body = payload(receipt, "body");
        sql.push_str(&format!(
            "INSERT INTO message_channels (channel_id, tenant_id, name, channel_kind, source_receipt_id) VALUES ({}, {}, {}, {}, {}) ON CONFLICT (channel_id) DO NOTHING;\n",
            sql_string_literal(channel_id),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(channel_id),
            sql_string_literal("LOCAL_OPS"),
            sql_string_literal(&receipt.receipt_id)
        ));
        sql.push_str(&format!(
            "INSERT INTO message_channel_members (membership_id, tenant_id, channel_id, actor_id, member_role, source_receipt_id) VALUES ({}, {}, {}, {}, {}, {}) ON CONFLICT (membership_id) DO NOTHING;\n",
            sql_string_literal(&format!("{}_channel_member", receipt.receipt_id)),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(channel_id),
            sql_string_literal(receipt.actor_id.as_str()),
            sql_string_literal("OWNER"),
            sql_string_literal(&receipt.receipt_id)
        ));
        sql.push_str(&format!(
            "INSERT INTO message_threads (thread_id, tenant_id, channel_id, title, status, source_receipt_id) VALUES ({}, {}, {}, {}, {}, {}) ON CONFLICT (thread_id) DO NOTHING;\n",
            sql_string_literal(thread_id),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(channel_id),
            sql_string_literal("Local thread"),
            sql_string_literal("OPEN"),
            sql_string_literal(&receipt.receipt_id)
        ));
        sql.push_str(&format!(
            "INSERT INTO message_thread_messages (message_id, tenant_id, thread_id, actor_id, body, source_receipt_id, fanout_status) VALUES ({}, {}, {}, {}, {}, {}, {}) ON CONFLICT (message_id) DO NOTHING;\n",
            sql_string_literal(message_id),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(thread_id),
            sql_string_literal(receipt.actor_id.as_str()),
            sql_string_literal(body),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal("BLOCKED")
        ));
        sql.push_str(&format!(
            "INSERT INTO message_thread_participants (participant_id, tenant_id, thread_id, actor_id, participant_role, source_receipt_id) VALUES ({}, {}, {}, {}, {}, {}) ON CONFLICT (participant_id) DO NOTHING;\n",
            sql_string_literal(&format!("{}_thread_participant", receipt.receipt_id)),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(thread_id),
            sql_string_literal(receipt.actor_id.as_str()),
            sql_string_literal("AUTHOR"),
            sql_string_literal(&receipt.receipt_id)
        ));
        sql.push_str(&format!(
            "INSERT INTO message_envelopes (envelope_id, tenant_id, message_id, thread_id, channel_id, envelope_state, source_receipt_id) VALUES ({}, {}, {}, {}, {}, {}, {}) ON CONFLICT (envelope_id) DO NOTHING;\n",
            sql_string_literal(&format!("{}_envelope", receipt.receipt_id)),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(message_id),
            sql_string_literal(thread_id),
            sql_string_literal(channel_id),
            sql_string_literal("FANOUT_BLOCKED"),
            sql_string_literal(&receipt.receipt_id)
        ));
    }
    for receipt in receipts_by_kind(receipts, "message.fanout.requested") {
        let message_id = receipt_by_id(receipts, payload(receipt, "message_receipt_id"))
            .map(|source| payload(source, "message_id"))
            .unwrap_or_default();
        sql.push_str(&format!(
            "INSERT INTO message_fanout_requests (fanout_request_id, tenant_id, message_id, source_receipt_id, delivery_authority) VALUES ({}, {}, {}, {}, {}) ON CONFLICT (fanout_request_id) DO NOTHING;\n",
            sql_string_literal(payload(receipt, "fanout_request_id")),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(message_id),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal("BLOCKED")
        ));
    }
    for receipt in receipts_by_kind(receipts, "message.presence.requested") {
        sql.push_str(&format!(
            "INSERT INTO message_presence_records (presence_id, tenant_id, actor_id, thread_id, status, source_receipt_id) VALUES ({}, {}, {}, {}, {}, {}) ON CONFLICT (presence_id) DO NOTHING;\n",
            sql_string_literal(payload(receipt, "presence_request_id")),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(receipt.actor_id.as_str()),
            sql_string_literal(payload(receipt, "thread_id")),
            sql_string_literal("BLOCKED"),
            sql_string_literal(&receipt.receipt_id)
        ));
    }
    for receipt in receipts_by_kind(receipts, "message.realtime.cutover.preflighted") {
        let preflight_id = payload(receipt, "preflight_id");
        if preflight_id.is_empty() {
            continue;
        }
        sql.push_str(&format!(
            "INSERT INTO message_realtime_cutover_preflights (realtime_preflight_id, tenant_id, source_receipt_id, presence_request_receipt_id, thread_id, channel_id, requested_realtime_scope, terminal_state, realtime_provider_turn_on_observed, production_delivery_allowed) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, false, false) ON CONFLICT (realtime_preflight_id) DO NOTHING;\n",
            sql_string_literal(preflight_id),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(payload(receipt, "presence_request_receipt_id")),
            sql_string_literal(payload(receipt, "thread_id")),
            sql_string_literal(payload(receipt, "channel_id")),
            sql_string_literal(payload(receipt, "requested_realtime_scope")),
            sql_string_literal(payload(receipt, "terminal_state"))
        ));
        sql.push_str(&format!(
            "INSERT INTO message_delivery_replay_batches (delivery_replay_batch_id, tenant_id, source_receipt_id, realtime_preflight_id, channel_id, replay_scope, replay_state, websocket_fanout_allowed) VALUES ({}, {}, {}, {}, {}, {}, {}, false) ON CONFLICT (delivery_replay_batch_id) DO NOTHING;\n",
            sql_string_literal(&format!("{preflight_id}_delivery_replay")),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(preflight_id),
            sql_string_literal(payload(receipt, "channel_id")),
            sql_string_literal(payload(receipt, "requested_realtime_scope")),
            sql_string_literal("LOCAL_REPLAY_READY_PROVIDER_BLOCKED")
        ));
        sql.push_str(&format!(
            "INSERT INTO message_subscription_isolation_checks (subscription_isolation_check_id, tenant_id, source_receipt_id, realtime_preflight_id, channel_id, isolation_status, tenant_subscription_isolation_proven, service_role_fanout_allowed) VALUES ({}, {}, {}, {}, {}, {}, true, false) ON CONFLICT (subscription_isolation_check_id) DO NOTHING;\n",
            sql_string_literal(&format!("{preflight_id}_isolation")),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(preflight_id),
            sql_string_literal(payload(receipt, "channel_id")),
            sql_string_literal("TENANT_CHANNEL_ISOLATED_LOCAL_PROOF")
        ));
        sql.push_str(&format!(
            "INSERT INTO message_service_role_fanout_refusals (service_role_fanout_refusal_id, tenant_id, source_receipt_id, realtime_preflight_id, refusal_reason, service_role_fanout_refused, presence_mutation_allowed, typing_indicator_allowed) VALUES ({}, {}, {}, {}, {}, true, false, false) ON CONFLICT (service_role_fanout_refusal_id) DO NOTHING;\n",
            sql_string_literal(&format!("{preflight_id}_service_role_refusal")),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(preflight_id),
            sql_string_literal("service-role fanout remains blocked until provider and security approval")
        ));
    }
}

fn append_pages_sql(sql: &mut String, receipts: &[Receipt]) {
    for receipt in receipts_by_kind(receipts, "pages.document.published") {
        let page_id = payload(receipt, "document_id");
        let title = payload(receipt, "title");
        let revision_id = payload(receipt, "revision_id");
        let body_ref = payload(receipt, "body_ref");
        sql.push_str(&format!(
            "INSERT INTO pages_documents (page_id, tenant_id, slug, title, visibility, current_revision_id, source_receipt_id, owner_actor_id) VALUES ({}, {}, {}, {}, {}, {}, {}, {}) ON CONFLICT (page_id) DO NOTHING;\n",
            sql_string_literal(page_id),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(page_id),
            sql_string_literal(title),
            sql_string_literal("TEAM"),
            sql_string_literal(revision_id),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(receipt.actor_id.as_str())
        ));
        sql.push_str(&format!(
            "INSERT INTO pages_revisions (revision_id, tenant_id, page_id, author_id, body, source_receipt_id) VALUES ({}, {}, {}, {}, {}, {}) ON CONFLICT (revision_id) DO NOTHING;\n",
            sql_string_literal(revision_id),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(page_id),
            sql_string_literal(receipt.actor_id.as_str()),
            sql_string_literal(body_ref),
            sql_string_literal(&receipt.receipt_id)
        ));
        sql.push_str(&format!(
            "INSERT INTO pages_publications (publication_id, tenant_id, page_id, revision_id, publication_state, source_receipt_id) VALUES ({}, {}, {}, {}, {}, {}) ON CONFLICT (publication_id) DO NOTHING;\n",
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(page_id),
            sql_string_literal(revision_id),
            sql_string_literal("PUBLICATION_BLOCKED"),
            sql_string_literal(&receipt.receipt_id)
        ));
        sql.push_str(&format!(
            "INSERT INTO pages_publication_targets (publication_target_id, tenant_id, publication_id, target_scope, source_receipt_id) VALUES ({}, {}, {}, {}, {}) ON CONFLICT (publication_target_id) DO NOTHING;\n",
            sql_string_literal(&format!("{}_target", receipt.receipt_id)),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal("TENANT_ONLY"),
            sql_string_literal(&receipt.receipt_id)
        ));
    }
    for receipt in receipts_by_kind(receipts, "pages.edit.draft.saved") {
        let page_id = payload(receipt, "document_id");
        let revision_id = payload(receipt, "revision_id");
        sql.push_str(&format!(
            "INSERT INTO pages_documents (page_id, tenant_id, slug, title, visibility, current_revision_id, source_receipt_id) VALUES ({}, {}, {}, {}, {}, {}, {}) ON CONFLICT (page_id) DO NOTHING;\n",
            sql_string_literal(page_id),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(page_id),
            sql_string_literal(payload(receipt, "title")),
            sql_string_literal("TEAM"),
            sql_string_literal(revision_id),
            sql_string_literal(&receipt.receipt_id)
        ));
        sql.push_str(&format!(
            "INSERT INTO pages_revisions (revision_id, tenant_id, page_id, author_id, body, source_receipt_id) VALUES ({}, {}, {}, {}, {}, {}) ON CONFLICT (revision_id) DO NOTHING;\n",
            sql_string_literal(revision_id),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(page_id),
            sql_string_literal(receipt.actor_id.as_str()),
            sql_string_literal(payload(receipt, "body_ref")),
            sql_string_literal(&receipt.receipt_id)
        ));
    }
    for receipt in receipts_by_kind(receipts, "pages.approval.requested") {
        let source = receipt_by_id(receipts, payload(receipt, "source_edit_draft_receipt_id"));
        let revision_id = source
            .map(|source| payload(source, "revision_id"))
            .unwrap_or_default();
        sql.push_str(&format!(
            "INSERT INTO pages_approval_requests (approval_request_id, tenant_id, page_id, revision_id, requested_by, source_receipt_id, approval_state) VALUES ({}, {}, {}, {}, {}, {}, {}) ON CONFLICT (approval_request_id) DO NOTHING;\n",
            sql_string_literal(payload(receipt, "approval_request_id")),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(payload(receipt, "document_id")),
            sql_string_literal(revision_id),
            sql_string_literal(receipt.actor_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal("BLOCKED")
        ));
    }
    for receipt in receipts_by_kind(receipts, "pages.search.preflighted") {
        let page_id = payload(receipt, "document_id");
        sql.push_str(&format!(
            "INSERT INTO pages_search_preflights (search_preflight_id, tenant_id, page_id, source_receipt_id, indexing_authority) VALUES ({}, {}, {}, {}, {}) ON CONFLICT (search_preflight_id) DO NOTHING;\n",
            sql_string_literal(payload(receipt, "preflight_id")),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(page_id),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal("BLOCKED")
        ));
        sql.push_str(&format!(
            "INSERT INTO pages_citations (citation_id, tenant_id, page_id, source_receipt_id, target_kind, target_ref) VALUES ({}, {}, {}, {}, {}, {}) ON CONFLICT (citation_id) DO NOTHING;\n",
            sql_string_literal(payload(receipt, "citation_handle")),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(page_id),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal("revision"),
            sql_string_literal(payload(receipt, "revision_id"))
        ));
        sql.push_str(&format!(
            "INSERT INTO pages_revision_citations (revision_citation_id, tenant_id, revision_id, citation_handle, target_kind, target_ref, source_receipt_id) VALUES ({}, {}, {}, {}, {}, {}, {}) ON CONFLICT (revision_citation_id) DO NOTHING;\n",
            sql_string_literal(&format!("{}_revision_citation", receipt.receipt_id)),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(payload(receipt, "revision_id")),
            sql_string_literal(payload(receipt, "citation_handle")),
            sql_string_literal("revision"),
            sql_string_literal(payload(receipt, "revision_id")),
            sql_string_literal(&receipt.receipt_id)
        ));
        sql.push_str(&format!(
            "INSERT INTO pages_attachments (attachment_id, tenant_id, page_id, revision_id, attachment_policy_id, attachment_state, source_receipt_id) VALUES ({}, {}, {}, {}, {}, {}, {}) ON CONFLICT (attachment_id) DO NOTHING;\n",
            sql_string_literal(&format!("{}_attachment", receipt.receipt_id)),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(page_id),
            sql_string_literal(payload(receipt, "revision_id")),
            sql_string_literal(payload(receipt, "attachment_policy_id")),
            sql_string_literal("POLICY_RECORDED"),
            sql_string_literal(&receipt.receipt_id)
        ));
        sql.push_str(&format!(
            "INSERT INTO pages_search_index_records (search_index_record_id, tenant_id, page_id, revision_id, search_preflight_id, indexing_state, embedding_provider, source_receipt_id) VALUES ({}, {}, {}, {}, {}, {}, {}, {}) ON CONFLICT (search_index_record_id) DO NOTHING;\n",
            sql_string_literal(&format!("{}_search_index", receipt.receipt_id)),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(page_id),
            sql_string_literal(payload(receipt, "revision_id")),
            sql_string_literal(payload(receipt, "preflight_id")),
            sql_string_literal("INDEXING_BLOCKED"),
            sql_string_literal("local_disabled"),
            sql_string_literal(&receipt.receipt_id)
        ));
    }
}

fn append_twin_sql(sql: &mut String, receipts: &[Receipt], memory: &[MemoryRecord]) {
    for draft in receipts_by_kind(receipts, "twin.session.draft.admitted") {
        let session_id = payload(draft, "session_id");
        let stance = payload(draft, "companion_stance");
        sql.push_str(&format!(
            "INSERT INTO twin_sessions (session_id, tenant_id, actor_id, stance, source_receipt_id) VALUES ({}, {}, {}, {}, {}) ON CONFLICT (session_id) DO NOTHING;\n",
            sql_string_literal(session_id),
            sql_string_literal(draft.tenant_id.as_str()),
            sql_string_literal(draft.actor_id.as_str()),
            sql_string_literal(stance),
            sql_string_literal(&draft.receipt_id)
        ));
        sql.push_str(&format!(
            "INSERT INTO twin_conversation_sessions (twin_conversation_session_id, tenant_id, session_id, companion_id, companion_stance, persona_profile_id, prompt_shape, conversation_state, source_receipt_id) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}) ON CONFLICT (twin_conversation_session_id) DO NOTHING;\n",
            sql_string_literal(&format!("{}_conversation", draft.receipt_id)),
            sql_string_literal(draft.tenant_id.as_str()),
            sql_string_literal(session_id),
            sql_string_literal(payload(draft, "companion_id")),
            sql_string_literal(stance),
            sql_string_literal(payload(draft, "persona_profile_id")),
            sql_string_literal(payload(draft, "prompt_shape")),
            sql_string_literal("LOCAL_GROUNDED"),
            sql_string_literal(&draft.receipt_id)
        ));
        sql.push_str(&format!(
            "INSERT INTO twin_companion_stance_records (twin_companion_stance_record_id, tenant_id, session_id, companion_id, companion_stance, persona_profile_id, source_receipt_id) VALUES ({}, {}, {}, {}, {}, {}, {}) ON CONFLICT (twin_companion_stance_record_id) DO NOTHING;\n",
            sql_string_literal(&format!("{}_stance", draft.receipt_id)),
            sql_string_literal(draft.tenant_id.as_str()),
            sql_string_literal(session_id),
            sql_string_literal(payload(draft, "companion_id")),
            sql_string_literal(stance),
            sql_string_literal(payload(draft, "persona_profile_id")),
            sql_string_literal(&draft.receipt_id)
        ));
        if let Some(record) = memory
            .iter()
            .find(|record| record.source_receipt_id == draft.receipt_id)
        {
            sql.push_str(&format!(
                "INSERT INTO twin_session_messages (twin_message_id, tenant_id, session_id, actor_id, role, body, source_receipt_id) VALUES ({}, {}, {}, {}, {}, {}, {}) ON CONFLICT (twin_message_id) DO NOTHING;\n",
                sql_string_literal(&format!("{}_human", draft.receipt_id)),
                sql_string_literal(draft.tenant_id.as_str()),
                sql_string_literal(session_id),
                sql_string_literal(draft.actor_id.as_str()),
                sql_string_literal("HUMAN"),
                sql_string_literal(&record.content),
                sql_string_literal(&draft.receipt_id)
            ));
            sql.push_str(&format!(
                "INSERT INTO twin_memory_snapshots (twin_memory_snapshot_id, tenant_id, session_id, memory_id, source_receipt_id, consolidation_decision) VALUES ({}, {}, {}, {}, {}, {}) ON CONFLICT (twin_memory_snapshot_id) DO NOTHING;\n",
                sql_string_literal(&format!("{}_memory", draft.receipt_id)),
                sql_string_literal(draft.tenant_id.as_str()),
                sql_string_literal(session_id),
                sql_string_literal(&record.memory_id),
                sql_string_literal(&record.provenance.gate_receipt_id),
                sql_string_literal(record.consolidation_decision.as_str())
            ));
        }
    }
    for retrieval in receipts_by_kind(receipts, "twin.session.memory.retrieved") {
        let draft_id = payload(retrieval, "source_draft_receipt_id");
        if let Some(draft) = receipt_by_id(receipts, draft_id) {
            let scoring = receipts_by_kind(receipts, "twin.session.memory.scored")
                .into_iter()
                .find(|scoring| payload(scoring, "source_draft_receipt_id") == draft_id);
            sql.push_str(&format!(
                "INSERT INTO twin_memory_retrievals (twin_memory_retrieval_id, tenant_id, session_id, memory_id, retrieval_driver, retrieval_scope, source_receipt_id, provider_call_allowed, memory_relevance_score, memory_decay_state, memory_decay_policy, world_model_source) VALUES ({}, {}, {}, {}, {}, {}, {}, false, {}, {}, {}, {}) ON CONFLICT (twin_memory_retrieval_id) DO NOTHING;\n",
                sql_string_literal(&retrieval.receipt_id),
                sql_string_literal(retrieval.tenant_id.as_str()),
                sql_string_literal(payload(draft, "session_id")),
                sql_string_literal(payload(retrieval, "memory_record_id")),
                sql_string_literal(payload(retrieval, "retrieval_driver")),
                sql_string_literal(payload(retrieval, "retrieval_scope")),
                sql_string_literal(&retrieval.receipt_id),
                scoring
                    .map(|receipt| payload(receipt, "memory_relevance_score"))
                    .unwrap_or("0"),
                sql_string_literal(
                    scoring
                        .map(|receipt| payload(receipt, "memory_decay_state"))
                        .unwrap_or("fresh_session_memory"),
                ),
                sql_string_literal(
                    scoring
                        .map(|receipt| payload(receipt, "memory_decay_policy"))
                        .unwrap_or("local_recent_session_decay_v1"),
                ),
                sql_string_literal(
                    scoring
                        .map(|receipt| payload(receipt, "world_model_source"))
                        .unwrap_or("generated/world-model/pages-projection-fixtures.json"),
                )
            ));
        }
    }
    for answer in receipts_by_kind(receipts, "twin.session.answer.grounded") {
        let draft_id = payload(answer, "source_draft_receipt_id");
        if let Some(draft) = receipts
            .iter()
            .find(|receipt| receipt.receipt_id == draft_id)
        {
            let drift = receipts_by_kind(receipts, "twin.session.persona.drift_checked")
                .into_iter()
                .find(|drift| payload(drift, "source_draft_receipt_id") == draft_id);
            let session_id = payload(draft, "session_id");
            sql.push_str(&format!(
                "INSERT INTO twin_session_messages (twin_message_id, tenant_id, session_id, actor_id, role, body, source_receipt_id) VALUES ({}, {}, {}, {}, {}, {}, {}) ON CONFLICT (twin_message_id) DO NOTHING;\n",
                sql_string_literal(&format!("{}_answer", answer.receipt_id)),
                sql_string_literal(answer.tenant_id.as_str()),
                sql_string_literal(session_id),
                sql_string_literal(answer.actor_id.as_str()),
                sql_string_literal("COMPANION"),
                sql_string_literal(payload(answer, "grounded_answer")),
                sql_string_literal(&answer.receipt_id)
            ));
            sql.push_str(&format!(
                "INSERT INTO twin_model_traces (model_trace_id, tenant_id, session_id, driver_id, source_receipt_id, provider_call_allowed) VALUES ({}, {}, {}, {}, {}, false) ON CONFLICT (model_trace_id) DO NOTHING;\n",
                sql_string_literal(&format!("{}_model", answer.receipt_id)),
                sql_string_literal(answer.tenant_id.as_str()),
                sql_string_literal(session_id),
                sql_string_literal(payload(answer, "model_gateway_driver")),
                sql_string_literal(&answer.receipt_id)
            ));
            sql.push_str(&format!(
                "INSERT INTO twin_grounded_answers (twin_grounded_answer_id, tenant_id, session_id, answer_text, model_gateway_driver, model_gateway_provider, model_gateway_model_id, model_gateway_routing, model_gateway_inference_id, source_receipt_id, provider_call_allowed, persona_contract_status, voice_drift_status, voice_drift_score, world_model_source) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, false, {}, {}, {}, {}) ON CONFLICT (twin_grounded_answer_id) DO NOTHING;\n",
                sql_string_literal(&answer.receipt_id),
                sql_string_literal(answer.tenant_id.as_str()),
                sql_string_literal(session_id),
                sql_string_literal(payload(answer, "grounded_answer")),
                sql_string_literal(payload(answer, "model_gateway_driver")),
                sql_string_literal(payload(answer, "model_gateway_provider")),
                sql_string_literal(payload(answer, "model_gateway_model_id")),
                sql_string_literal(payload(answer, "model_gateway_routing")),
                sql_string_literal(payload(answer, "model_gateway_inference_id")),
                sql_string_literal(&answer.receipt_id),
                sql_string_literal(
                    drift.map(|receipt| payload(receipt, "persona_contract_status"))
                        .unwrap_or("MATCHED_DECLARED_STANCE"),
                ),
                sql_string_literal(
                    drift.map(|receipt| payload(receipt, "voice_drift_status"))
                        .unwrap_or("IN_BOUNDS"),
                ),
                drift.map(|receipt| payload(receipt, "voice_drift_score"))
                    .unwrap_or("0"),
                sql_string_literal(if payload(answer, "world_model_source").is_empty() {
                    "generated/world-model/pages-projection-fixtures.json"
                } else {
                    payload(answer, "world_model_source")
                })
            ));
        }
    }
    for summary in receipts_by_kind(receipts, "twin.session.conversation.summarized") {
        let draft_id = payload(summary, "source_draft_receipt_id");
        if let Some(draft) = receipt_by_id(receipts, draft_id) {
            sql.push_str(&format!(
                "INSERT INTO twin_conversation_summaries (twin_conversation_summary_id, tenant_id, session_id, summary_text, summary_state, message_count, memory_reference_count, model_trace_count, source_receipt_id, production_write_allowed, compaction_policy, compaction_state, world_model_source_count) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, false, {}, {}, {}) ON CONFLICT (twin_conversation_summary_id) DO NOTHING;\n",
                sql_string_literal(&summary.receipt_id),
                sql_string_literal(summary.tenant_id.as_str()),
                sql_string_literal(payload(draft, "session_id")),
                sql_string_literal(payload(summary, "summary_text")),
                sql_string_literal(payload(summary, "summary_state")),
                payload(summary, "message_count"),
                payload(summary, "memory_reference_count"),
                payload(summary, "model_trace_count"),
                sql_string_literal(&summary.receipt_id),
                sql_string_literal(payload(summary, "compaction_policy")),
                sql_string_literal(payload(summary, "compaction_state")),
                payload(summary, "world_model_source_count")
            ));
        }
    }
}

fn append_product_governance_sql(sql: &mut String, receipts: &[Receipt]) {
    for receipt in receipts_by_kind(receipts, "product.signal.ingested") {
        sql.push_str(&format!(
            "INSERT INTO product_signals (product_signal_id, tenant_id, source_receipt_id, signal_source, signal_kind, source_surface, production_write_allowed) VALUES ({}, {}, {}, {}, {}, {}, false) ON CONFLICT (product_signal_id) DO NOTHING;\n",
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(payload(receipt, "signal_source")),
            sql_string_literal(payload(receipt, "signal_kind")),
            sql_string_literal(payload(receipt, "source_surface"))
        ));
    }
    for receipt in receipts_by_kind(receipts, "product.bet.shaped") {
        sql.push_str(&format!(
            "INSERT INTO product_bet_drafts (product_bet_id, tenant_id, source_signal_receipt_id, source_receipt_id, shape_status, forbidden_action, production_write_allowed) VALUES ({}, {}, {}, {}, {}, {}, false) ON CONFLICT (product_bet_id) DO NOTHING;\n",
            sql_string_literal(payload(receipt, "bet_id")),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(payload(receipt, "source_signal_receipt_id")),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(payload(receipt, "shape_status")),
            sql_string_literal(payload(receipt, "forbidden_action"))
        ));
    }
    for receipt in receipts_by_kind(receipts, "product.handoff.requested") {
        sql.push_str(&format!(
            "INSERT INTO product_handoff_requests (product_handoff_request_id, tenant_id, shaped_bet_receipt_id, source_receipt_id, human_edge_surface, ratification_required, runtime_status, production_write_allowed) VALUES ({}, {}, {}, {}, {}, {}, {}, false) ON CONFLICT (product_handoff_request_id) DO NOTHING;\n",
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(payload(receipt, "shaped_bet_receipt_id")),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(payload(receipt, "human_edge_surface")),
            payload(receipt, "ratification_required"),
            sql_string_literal(payload(receipt, "runtime_status"))
        ));
    }
    for receipt in receipts_by_kind(receipts, "eval.verdict.recorded") {
        sql.push_str(&format!(
            "INSERT INTO eval_guardrail_verdicts (eval_guardrail_verdict_id, tenant_id, source_receipt_id, trace_receipt_id, suite, score, passed, worker_credential_issuance_allowed) VALUES ({}, {}, {}, {}, {}, {}, {}, false) ON CONFLICT (eval_guardrail_verdict_id) DO NOTHING;\n",
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(payload(receipt, "trace_receipt_id")),
            sql_string_literal(payload(receipt, "suite")),
            payload(receipt, "score"),
            payload(receipt, "passed")
        ));
    }
    for receipt in receipts_by_kind(receipts, "aegis.finding.classified") {
        sql.push_str(&format!(
            "INSERT INTO security_findings (security_finding_id, tenant_id, source_receipt_id, scan_receipt_id, severity, classification, remediation_write_allowed) VALUES ({}, {}, {}, {}, {}, {}, false) ON CONFLICT (security_finding_id) DO NOTHING;\n",
            sql_string_literal(payload(receipt, "finding_id")),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(payload(receipt, "source_receipt_id")),
            sql_string_literal(payload(receipt, "severity")),
            sql_string_literal(payload(receipt, "classification"))
        ));
    }
    for receipt in receipts_by_kind(receipts, "charter.evidence.attested") {
        sql.push_str(&format!(
            "INSERT INTO charter_attestations (charter_attestation_id, tenant_id, source_receipt_id, obligation_receipt_id, verdict, exception_status, production_cutover_allowed) VALUES ({}, {}, {}, {}, {}, {}, false) ON CONFLICT (charter_attestation_id) DO NOTHING;\n",
            sql_string_literal(payload(receipt, "attestation_id")),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(payload(receipt, "obligation_receipt_id")),
            sql_string_literal(payload(receipt, "verdict")),
            sql_string_literal("NONE_OPEN")
        ));
    }
}

fn append_forge_sql(sql: &mut String, receipts: &[Receipt]) {
    for receipt in receipts_by_kind(receipts, "forge.outcome.signal.recorded") {
        if payload(receipt, "outcome_signal_id").is_empty() {
            continue;
        }
        sql.push_str(&format!(
            "INSERT INTO forge_outcome_signals (outcome_signal_id, tenant_id, run_id, source_receipt_id, source_receipt_kind, disposition, summary, capability_ids, model_or_worker, lesson_candidate, message_channel_id, learning_candidate_allowed, message_activity_allowed, active_memory_write_allowed, adaptation_allowed, execution_authority_opened, production_write_allowed) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, true, true, false, false, false, false) ON CONFLICT (outcome_signal_id) DO NOTHING;\n",
            sql_string_literal(payload(receipt, "outcome_signal_id")),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(payload(receipt, "run_id")),
            sql_string_literal(payload(receipt, "source_receipt_id")),
            sql_string_literal(payload(receipt, "source_receipt_kind")),
            sql_string_literal(payload(receipt, "disposition")),
            sql_string_literal(payload(receipt, "summary")),
            sql_string_literal(payload(receipt, "capability_ids")),
            sql_string_literal(payload(receipt, "model_or_worker")),
            sql_string_literal(payload(receipt, "lesson_candidate")),
            sql_string_literal(payload(receipt, "message_channel_id"))
        ));
    }
}

#[derive(Clone, Debug)]
struct MarketplaceInstalledExport {
    tenant_id: String,
    capability_id: String,
    scope: String,
    install_receipt_id: String,
    approval_receipt_id: Option<String>,
}

fn append_marketplace_sql(sql: &mut String, receipts: &[Receipt]) {
    let mut installed = BTreeMap::<String, MarketplaceInstalledExport>::new();
    for receipt in receipts_by_kind(receipts, "marketplace.act.recorded") {
        let read_only = payload(receipt, "read_only") == "true";
        let capability_execution_allowed =
            payload(receipt, "capability_execution_allowed") == "true";
        let secret_access_allowed = payload(receipt, "secret_access_allowed") == "true";
        sql.push_str(&format!(
            "INSERT INTO marketplace_acts (marketplace_act_id, tenant_id, actor_id, source_receipt_id, act, source_route, capability_id, scope, decision, read_only, pack_id, url, note, reason, items_added, items_held, item_record_ids, actor_role, capability_execution_allowed, secret_access_allowed) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}) ON CONFLICT (marketplace_act_id) DO NOTHING;\n",
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(receipt.actor_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(payload(receipt, "act")),
            sql_string_literal(payload(receipt, "source_route")),
            sql_string_literal(payload(receipt, "capability_id")),
            sql_string_literal(payload(receipt, "scope")),
            sql_string_literal(payload(receipt, "decision")),
            read_only,
            sql_string_literal(payload(receipt, "pack_id")),
            sql_string_literal(payload(receipt, "url")),
            sql_string_literal(payload(receipt, "note")),
            sql_string_literal(payload(receipt, "reason")),
            sql_string_literal(payload(receipt, "items_added")),
            sql_string_literal(payload(receipt, "items_held")),
            sql_string_literal(payload(receipt, "item_record_ids")),
            sql_string_literal(payload(receipt, "actor_role")),
            capability_execution_allowed,
            secret_access_allowed
        ));
        let capability_id = payload(receipt, "capability_id");
        let scope = payload(receipt, "scope");
        if capability_id.is_empty() || scope.is_empty() {
            continue;
        }
        let key = format!("{}|{}|{}", receipt.tenant_id.as_str(), capability_id, scope);
        match payload(receipt, "act") {
            "install" | "pack_install" => {
                installed.insert(
                    key,
                    MarketplaceInstalledExport {
                        tenant_id: receipt.tenant_id.as_str().to_string(),
                        capability_id: capability_id.to_string(),
                        scope: scope.to_string(),
                        install_receipt_id: receipt.receipt_id.clone(),
                        approval_receipt_id: None,
                    },
                );
            }
            "approval" => {
                if let Some(record) = installed.get_mut(&key) {
                    record.approval_receipt_id = Some(receipt.receipt_id.clone());
                }
            }
            "revocation" => {
                installed.remove(&key);
            }
            _ => {}
        }
    }
    for (key, record) in installed {
        let status = if record.approval_receipt_id.is_some() {
            "approved_installed"
        } else {
            "installed_pending_review"
        };
        sql.push_str(&format!(
            "INSERT INTO marketplace_installed_capabilities (installed_capability_key, tenant_id, capability_id, scope, install_receipt_id, approval_receipt_id, effective_status, capability_execution_allowed, secret_access_allowed, inherited_agent_permissions_allowed, untrusted_execution_isolation, production_write_allowed) VALUES ({}, {}, {}, {}, {}, {}, {}, false, false, false, {}, false) ON CONFLICT (installed_capability_key) DO UPDATE SET approval_receipt_id = EXCLUDED.approval_receipt_id, effective_status = EXCLUDED.effective_status, capability_execution_allowed = false, secret_access_allowed = false, inherited_agent_permissions_allowed = false, untrusted_execution_isolation = EXCLUDED.untrusted_execution_isolation, production_write_allowed = false;\n",
            sql_string_literal(&key.replace('|', "::")),
            sql_string_literal(&record.tenant_id),
            sql_string_literal(&record.capability_id),
            sql_string_literal(&record.scope),
            sql_string_literal(&record.install_receipt_id),
            sql_optional_string(record.approval_receipt_id.as_deref()),
            sql_string_literal(status),
            sql_string_literal("stronger_isolation_required_before_execution")
        ));
    }
}

fn append_strategy_talent_observatory_sql(sql: &mut String, receipts: &[Receipt]) {
    for receipt in receipts_by_kind(receipts, "product.handoff.requested") {
        sql.push_str(&format!(
            "INSERT INTO strategy_ratification_snapshots (strategy_ratification_snapshot_id, tenant_id, source_receipt_id, proposal_id, runtime_status, ratification_required, direction_setting_allowed) VALUES ({}, {}, {}, {}, {}, {}, false) ON CONFLICT (strategy_ratification_snapshot_id) DO NOTHING;\n",
            sql_string_literal(&format!("{}_strategy_snapshot", receipt.receipt_id)),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal("strategy_local_proposal"),
            sql_string_literal("DECLARED_LOCAL_READ_SURFACE"),
            payload(receipt, "ratification_required")
        ));
    }
    for receipt in receipts_by_kind(receipts, "strategy.ratification.recorded") {
        sql.push_str(&format!(
            "INSERT INTO strategy_ratification_snapshots (strategy_ratification_snapshot_id, tenant_id, source_receipt_id, proposal_id, runtime_status, ratification_required, direction_setting_allowed) VALUES ({}, {}, {}, {}, {}, false, false) ON CONFLICT (strategy_ratification_snapshot_id) DO NOTHING;\n",
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(payload(receipt, "proposal_id")),
            sql_string_literal("RATIFICATION_RECORDED_DIRECTION_BLOCKED")
        ));
    }
    for receipt in receipts_by_kind(receipts, "strategy.ratification.decided") {
        let decision = payload(receipt, "decision");
        let (runtime_status, ratification_required) = match decision {
            "ratify_next_local_strategy_option" => {
                ("RATIFICATION_RECORDED_DIRECTION_AUTHORITY_BLOCKED", "false")
            }
            "request_more_evidence" => ("MORE_EVIDENCE_REQUESTED_RATIFICATION_OPEN", "true"),
            "hold_current_direction" => ("CURRENT_DIRECTION_HELD_RATIFICATION_OPEN", "true"),
            _ => ("UNKNOWN_STRATEGY_DECISION_RATIFICATION_OPEN", "true"),
        };
        sql.push_str(&format!(
            "INSERT INTO strategy_ratification_snapshots (strategy_ratification_snapshot_id, tenant_id, source_receipt_id, proposal_id, runtime_status, ratification_required, direction_setting_allowed) VALUES ({}, {}, {}, {}, {}, {}, false) ON CONFLICT (strategy_ratification_snapshot_id) DO NOTHING;\n",
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(payload(receipt, "proposal_id")),
            sql_string_literal(runtime_status),
            ratification_required,
        ));
    }
    for receipt in receipts_by_kind(receipts, "talent.sponsor_chain.authorized") {
        if payload(receipt, "parent_loop_id").is_empty() || payload(receipt, "scope").is_empty() {
            continue;
        }
        sql.push_str(&format!(
            "INSERT INTO talent_sponsor_chain_authorities (sponsor_chain_authority_id, tenant_id, source_receipt_id, parent_loop_id, requested_by_loop_id, human_sponsor_chain, scope, live_worker_execution_allowed) VALUES ({}, {}, {}, {}, {}, {}, {}, false) ON CONFLICT (sponsor_chain_authority_id) DO NOTHING;\n",
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(payload(receipt, "parent_loop_id")),
            sql_string_literal(payload(receipt, "requested_by_loop_id")),
            sql_string_literal(payload(receipt, "human_sponsor_chain")),
            sql_string_literal(payload(receipt, "scope"))
        ));
    }
    for receipt in receipts_by_kind(receipts, "talent.worker_lease.authorized") {
        if payload(receipt, "parent_loop_id").is_empty()
            || payload(receipt, "scope").is_empty()
            || payload(receipt, "expires_at").is_empty()
        {
            continue;
        }
        sql.push_str(&format!(
            "INSERT INTO talent_worker_lease_authorities (worker_lease_authority_id, tenant_id, source_receipt_id, parent_loop_id, scope, lease_state, expires_at, live_worker_execution_allowed) VALUES ({}, {}, {}, {}, {}, {}, {}, false) ON CONFLICT (worker_lease_authority_id) DO NOTHING;\n",
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(payload(receipt, "parent_loop_id")),
            sql_string_literal(payload(receipt, "scope")),
            sql_string_literal("AUTHORIZED_LOCAL_ONLY"),
            sql_string_literal(payload(receipt, "expires_at"))
        ));
    }
    for receipt in receipts_by_kind(receipts, "talent.budget.authorized") {
        if payload(receipt, "parent_loop_id").is_empty()
            || payload(receipt, "budget_limit").is_empty()
            || payload(receipt, "budget_unit").is_empty()
        {
            continue;
        }
        sql.push_str(&format!(
            "INSERT INTO talent_budget_authorities (budget_authority_id, tenant_id, source_receipt_id, parent_loop_id, budget_limit, budget_unit, treasury_status, live_spend_allowed) VALUES ({}, {}, {}, {}, {}, {}, {}, false) ON CONFLICT (budget_authority_id) DO NOTHING;\n",
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(payload(receipt, "parent_loop_id")),
            payload(receipt, "budget_limit"),
            sql_string_literal(payload(receipt, "budget_unit")),
            sql_string_literal(payload(receipt, "treasury_status"))
        ));
        sql.push_str(&format!(
            "INSERT INTO treasury_reserve_postures (treasury_reserve_posture_id, tenant_id, source_receipt_id, budget_authority_receipt_id, reserve_status, budget_limit, budget_unit, live_spend_allowed) VALUES ({}, {}, {}, {}, {}, {}, {}, false) ON CONFLICT (treasury_reserve_posture_id) DO NOTHING;\n",
            sql_string_literal(&format!("{}_treasury_reserve", receipt.receipt_id)),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(payload(receipt, "treasury_status")),
            payload(receipt, "budget_limit"),
            sql_string_literal(payload(receipt, "budget_unit"))
        ));
    }
    for receipt in receipts_by_kind(receipts, "talent.tool_allowlist.authorized") {
        if payload(receipt, "worker_template_id").is_empty()
            || payload(receipt, "tool_allowlist").is_empty()
        {
            continue;
        }
        sql.push_str(&format!(
            "INSERT INTO talent_tool_allowlist_authorities (tool_allowlist_authority_id, tenant_id, source_receipt_id, worker_template_id, tool_allowlist, forbidden_tools, shell_execution_allowed, patch_application_allowed) VALUES ({}, {}, {}, {}, {}, {}, false, false) ON CONFLICT (tool_allowlist_authority_id) DO NOTHING;\n",
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(payload(receipt, "worker_template_id")),
            sql_string_literal(payload(receipt, "tool_allowlist")),
            sql_string_literal(payload(receipt, "forbidden_tools"))
        ));
    }
    for receipt in receipts_by_kind(receipts, "worker.spawn_requested") {
        if payload(receipt, "worker_template_id").is_empty()
            || payload(receipt, "parent_id").is_empty()
            || payload(receipt, "runtime_status").is_empty()
        {
            continue;
        }
        sql.push_str(&format!(
            "INSERT INTO talent_worker_spawn_requests (worker_spawn_request_id, tenant_id, source_receipt_id, worker_template_id, parent_id, runtime_status, live_worker_execution_allowed) VALUES ({}, {}, {}, {}, {}, {}, false) ON CONFLICT (worker_spawn_request_id) DO NOTHING;\n",
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(payload(receipt, "worker_template_id")),
            sql_string_literal(payload(receipt, "parent_id")),
            sql_string_literal(payload(receipt, "runtime_status"))
        ));
    }
    for receipt in receipts_by_kind(receipts, "worker.handoff.recorded") {
        if payload(receipt, "spawn_receipt_id").is_empty()
            || payload(receipt, "credential_check_receipt_id").is_empty()
            || payload(receipt, "worker_run_id").is_empty()
        {
            continue;
        }
        sql.push_str(&format!(
            "INSERT INTO worker_runtime_handoffs (worker_handoff_id, tenant_id, source_receipt_id, spawn_receipt_id, credential_check_receipt_id, worker_template_id, worker_run_id, next_owner, production_write_allowed) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, false) ON CONFLICT (worker_handoff_id) DO NOTHING;\n",
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(payload(receipt, "spawn_receipt_id")),
            sql_string_literal(payload(receipt, "credential_check_receipt_id")),
            sql_string_literal(payload(receipt, "worker_template_id")),
            sql_string_literal(payload(receipt, "worker_run_id")),
            sql_string_literal(payload(receipt, "next_owner"))
        ));
    }
    for receipt in receipts_by_kind(receipts, "worker.retired") {
        if payload(receipt, "spawn_receipt_id").is_empty()
            || payload(receipt, "handoff_receipt_id").is_empty()
            || payload(receipt, "worker_run_id").is_empty()
        {
            continue;
        }
        sql.push_str(&format!(
            "INSERT INTO worker_runtime_retirements (worker_retirement_id, tenant_id, source_receipt_id, spawn_receipt_id, handoff_receipt_id, worker_template_id, worker_run_id, active_runtime_allowed) VALUES ({}, {}, {}, {}, {}, {}, {}, false) ON CONFLICT (worker_retirement_id) DO NOTHING;\n",
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(payload(receipt, "spawn_receipt_id")),
            sql_string_literal(payload(receipt, "handoff_receipt_id")),
            sql_string_literal(payload(receipt, "worker_template_id")),
            sql_string_literal(payload(receipt, "worker_run_id"))
        ));
    }
    if let Some(receipt) = receipts.last() {
        sql.push_str(&format!(
            "INSERT INTO observatory_role_view_snapshots (observatory_role_view_snapshot_id, tenant_id, source_receipt_id, role_count, declared_source_count, latest_run_status, hidden_authority_allowed) VALUES ({}, {}, {}, 8, 7, {}, false) ON CONFLICT (observatory_role_view_snapshot_id) DO NOTHING;\n",
            sql_string_literal(&format!("{}_observatory_roles", receipt.receipt_id)),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(receipt.kind.as_str())
        ));
    }
}

fn append_auth_tenant_sql(sql: &mut String, receipts: &[Receipt]) {
    for receipt in receipts_by_kind(receipts, "auth.user_admission.approved") {
        let target_actor_id = payload(receipt, "target_auth_user_id");
        let mapped_role = payload(receipt, "mapped_role");
        let membership_state = payload(receipt, "membership_state");
        let policy_decision_backed = receipt.policy_decision_id.as_ref().is_some_and(|id| {
            receipts.iter().any(|candidate| {
                candidate.kind == POLICY_DECISION_RECEIPT_KIND
                    && candidate.tenant_id == receipt.tenant_id
                    && payload(candidate, "policy_decision_id") == id
                    && payload(candidate, "action") == "approve_auth_user_admission"
                    && payload(candidate, "outcome") == "ALLOW"
            })
        });
        if target_actor_id.is_empty()
            || mapped_role.is_empty()
            || !policy_decision_backed
            || !["active", "beta_active", "production_active"].contains(&membership_state)
            || payload(receipt, "service_role_shortcut_allowed") != "false"
            || payload(receipt, "user_metadata_authorization_allowed") != "false"
            || payload(receipt, "hosted_auth_claim_authority_allowed") != "true"
        {
            continue;
        }
        sql.push_str(&format!(
            "INSERT INTO auth_tenant_orgs (auth_tenant_org_id, tenant_id, source_receipt_id, tenant_key, org_state, production_auth_allowed) VALUES ({}, {}, {}, {}, {}, true) ON CONFLICT (auth_tenant_org_id) DO NOTHING;\n",
            sql_string_literal(&format!("{}_tenant_org", receipt.receipt_id)),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal("BETA_AUTH_BOOTSTRAP_APPROVED")
        ));
        sql.push_str(&format!(
            "INSERT INTO auth_tenant_memberships (auth_tenant_membership_id, tenant_id, source_receipt_id, actor_id, tenant_membership_scope, membership_state, service_role_shortcut_allowed) VALUES ({}, {}, {}, {}, {}, {}, false) ON CONFLICT (auth_tenant_membership_id) DO NOTHING;\n",
            sql_string_literal(&format!("{}_tenant_membership", receipt.receipt_id)),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(target_actor_id),
            sql_string_literal(payload(receipt, "tenant_membership_scope")),
            sql_string_literal(membership_state)
        ));
        sql.push_str(&format!(
            "INSERT INTO auth_role_mappings (auth_role_mapping_id, tenant_id, source_receipt_id, actor_id, role_mapping_scope, mapped_role, role_escalation_allowed) VALUES ({}, {}, {}, {}, {}, {}, false) ON CONFLICT (auth_role_mapping_id) DO NOTHING;\n",
            sql_string_literal(&format!("{}_role_mapping", receipt.receipt_id)),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(target_actor_id),
            sql_string_literal(payload(receipt, "role_mapping_scope")),
            sql_string_literal(mapped_role)
        ));
    }
    for receipt in receipts_by_kind(receipts, "auth.tenant_policy.preflighted") {
        let preflight_id = payload(receipt, "preflight_id");
        if preflight_id.is_empty() {
            continue;
        }
        sql.push_str(&format!(
            "INSERT INTO auth_tenant_orgs (auth_tenant_org_id, tenant_id, source_receipt_id, tenant_key, org_state, production_auth_allowed) VALUES ({}, {}, {}, {}, {}, false) ON CONFLICT (auth_tenant_org_id) DO NOTHING;\n",
            sql_string_literal(&format!("{}_tenant_org", receipt.receipt_id)),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal("LOCAL_TENANT_STUB_PRODUCTION_AUTH_BLOCKED")
        ));
        sql.push_str(&format!(
            "INSERT INTO auth_tenant_memberships (auth_tenant_membership_id, tenant_id, source_receipt_id, actor_id, tenant_membership_scope, membership_state, service_role_shortcut_allowed) VALUES ({}, {}, {}, {}, {}, {}, false) ON CONFLICT (auth_tenant_membership_id) DO NOTHING;\n",
            sql_string_literal(&format!("{}_tenant_membership", receipt.receipt_id)),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(receipt.actor_id.as_str()),
            sql_string_literal(payload(receipt, "tenant_membership_scope")),
            sql_string_literal("LOCAL_MEMBERSHIP_RECORDED_SERVICE_ROLE_BLOCKED")
        ));
        sql.push_str(&format!(
            "INSERT INTO auth_role_mappings (auth_role_mapping_id, tenant_id, source_receipt_id, actor_id, role_mapping_scope, mapped_role, role_escalation_allowed) VALUES ({}, {}, {}, {}, {}, {}, false) ON CONFLICT (auth_role_mapping_id) DO NOTHING;\n",
            sql_string_literal(&format!("{}_role_mapping", receipt.receipt_id)),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(receipt.actor_id.as_str()),
            sql_string_literal(payload(receipt, "role_mapping_scope")),
            sql_string_literal("owner")
        ));
        sql.push_str(&format!(
            "INSERT INTO auth_invite_states (auth_invite_state_id, tenant_id, source_receipt_id, invite_state_scope, invite_state, production_role_write_allowed) VALUES ({}, {}, {}, {}, {}, false) ON CONFLICT (auth_invite_state_id) DO NOTHING;\n",
            sql_string_literal(&format!("{}_invite_state", receipt.receipt_id)),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(payload(receipt, "invite_state_scope")),
            sql_string_literal("LOCAL_INVITE_STATE_RECORDED_ROLE_WRITE_BLOCKED")
        ));
        sql.push_str(&format!(
            "INSERT INTO auth_visibility_policies (auth_visibility_policy_id, tenant_id, source_receipt_id, visibility_scope, policy_state, user_metadata_authorization_allowed) VALUES ({}, {}, {}, {}, {}, false) ON CONFLICT (auth_visibility_policy_id) DO NOTHING;\n",
            sql_string_literal(&format!("{}_visibility_policy", receipt.receipt_id)),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal("tenant"),
            sql_string_literal("LOCAL_VISIBILITY_POLICY_RECORDED_METADATA_AUTH_BLOCKED")
        ));
        sql.push_str(&format!(
            "INSERT INTO auth_approved_model_policies (auth_approved_model_policy_id, tenant_id, source_receipt_id, approved_model_scope, approved_model_id, provider_call_allowed) VALUES ({}, {}, {}, {}, {}, false) ON CONFLICT (auth_approved_model_policy_id) DO NOTHING;\n",
            sql_string_literal(&format!("{}_approved_model", receipt.receipt_id)),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(payload(receipt, "approved_model_scope")),
            sql_string_literal("local-deterministic")
        ));
        sql.push_str(&format!(
            "INSERT INTO auth_session_evidence (auth_session_evidence_id, tenant_id, source_receipt_id, actor_id, source_auth_session_evidence_id, auth_session_status, production_auth_provider_allowed) VALUES ({}, {}, {}, {}, {}, {}, false) ON CONFLICT (auth_session_evidence_id) DO NOTHING;\n",
            sql_string_literal(&format!("{}_session_evidence", receipt.receipt_id)),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(receipt.actor_id.as_str()),
            sql_string_literal(payload(receipt, "source_auth_session_evidence_id")),
            sql_string_literal("ACCEPTED_LOCAL_STUB")
        ));
        sql.push_str(&format!(
            "INSERT INTO auth_tenant_policy_preflights (auth_tenant_policy_preflight_id, tenant_id, source_receipt_id, preflight_id, policy_decision_id, terminal_state, service_role_shortcut_allowed, implicit_tenant_inference_allowed, supabase_auth_claim_authority_allowed, cutover_allowed) VALUES ({}, {}, {}, {}, {}, {}, false, false, false, false) ON CONFLICT (auth_tenant_policy_preflight_id) DO NOTHING;\n",
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(receipt.tenant_id.as_str()),
            sql_string_literal(&receipt.receipt_id),
            sql_string_literal(preflight_id),
            sql_string_literal(receipt.policy_decision_id.as_deref().unwrap_or("")),
            sql_string_literal(payload(receipt, "terminal_state"))
        ));
    }
}

fn receipts_by_kind<'a>(receipts: &'a [Receipt], kind: &str) -> Vec<&'a Receipt> {
    receipts
        .iter()
        .filter(|receipt| receipt.kind == kind)
        .collect()
}

fn receipt_by_id<'a>(receipts: &'a [Receipt], receipt_id: &str) -> Option<&'a Receipt> {
    receipts
        .iter()
        .find(|receipt| receipt.receipt_id == receipt_id)
}

fn sql_optional_string(value: Option<&str>) -> String {
    value
        .map(sql_string_literal)
        .unwrap_or_else(|| "NULL".to_string())
}

fn payload<'a>(receipt: &'a Receipt, key: &str) -> &'a str {
    receipt
        .payload
        .get(key)
        .map(String::as_str)
        .unwrap_or_default()
}
