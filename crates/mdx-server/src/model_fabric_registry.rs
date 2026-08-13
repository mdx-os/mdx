use mdx_core::{Model, ModelDeployment, ModelPriceObservation, ProviderConnection};
use std::collections::BTreeMap;
use std::sync::{OnceLock, RwLock};

#[derive(Clone, Debug, Default)]
struct TenantRegistry {
    connections: BTreeMap<String, ProviderConnection>,
    models: BTreeMap<String, Model>,
    deployments: BTreeMap<String, ModelDeployment>,
    prices: BTreeMap<String, ModelPriceObservation>,
    policies: BTreeMap<String, mdx_core::RoutePolicy>,
    consecutive_failures: BTreeMap<String, u32>,
    circuit_open_until_epoch_seconds: BTreeMap<String, u64>,
    pending_circuit_transitions: Vec<CircuitTransition>,
    pending_failed_attempts: Vec<FailedInferenceAttempt>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CircuitTransition {
    pub connection_id: String,
    pub deployment_id: String,
    pub state: String,
    pub consecutive_failures: u32,
    pub open_until_epoch_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FailedInferenceAttempt {
    pub decision_id: String,
    pub workload_id: String,
    pub app_id: String,
    pub environment: String,
    pub session_id: String,
    pub deployment_id: String,
    pub latency_ms: u64,
    pub failure_class: String,
    pub provenance: String,
}

#[derive(Default)]
pub(crate) struct ModelFabricRegistry {
    tenants: RwLock<BTreeMap<String, TenantRegistry>>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct TenantRegistrySnapshot {
    pub connections: Vec<ProviderConnection>,
    pub models: Vec<Model>,
    pub deployments: Vec<ModelDeployment>,
    pub prices: Vec<ModelPriceObservation>,
    pub policies: Vec<mdx_core::RoutePolicy>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OwnedRouteCandidate {
    pub deployment_id: String,
    pub connection_id: String,
    pub tenant_id: String,
    pub provider_id: String,
    pub credential_ref: String,
    pub model_id: String,
    pub privacy_class: String,
    pub region: String,
    pub residency: String,
    pub data_retention: String,
    pub training_policy: String,
    pub connected: bool,
    pub healthy: bool,
    pub live_call_allowed: bool,
    pub tools: bool,
    pub structured_output: bool,
    pub vision: bool,
    pub context_tokens: u64,
    pub input_cost_microusd_per_million: u64,
    pub output_cost_microusd_per_million: u64,
    pub price_known: bool,
    pub p95_latency_ms: u64,
    pub latency_known: bool,
    pub quality_score: u32,
    pub quality_known: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ResolvedRegistryDeployment {
    pub connection: ProviderConnection,
    pub model: Model,
    pub deployment: ModelDeployment,
    pub price: ModelPriceObservation,
}

impl ModelFabricRegistry {
    pub(crate) fn policy_version_for_tenant(
        &self,
        tenant_id: &str,
        policy_id: &str,
        policy_version: &str,
    ) -> Result<mdx_core::RoutePolicy, String> {
        let tenants = self
            .tenants
            .read()
            .map_err(|_| "model fabric registry lock poisoned".to_string())?;
        tenants
            .get(tenant_id)
            .and_then(|tenant| tenant.policies.get(&policy_key(policy_id, policy_version)))
            .cloned()
            .ok_or_else(|| {
                format!("unknown model route policy version: {policy_id}@{policy_version}")
            })
    }

    pub(crate) fn transition_policy(
        &self,
        tenant_id: &str,
        policy_id: &str,
        policy_version: &str,
        state: &str,
    ) -> Result<mdx_core::RoutePolicy, String> {
        if !matches!(
            state,
            "draft" | "shadow" | "canary" | "promoted" | "rollback_required"
        ) {
            return Err("adaptive policy transition state is invalid".to_string());
        }
        let mut tenants = self
            .tenants
            .write()
            .map_err(|_| "model fabric registry lock poisoned".to_string())?;
        let tenant = tenants.get_mut(tenant_id).ok_or_else(|| {
            format!("unknown model route policy version: {policy_id}@{policy_version}")
        })?;
        let key = policy_key(policy_id, policy_version);
        let workload_ids = tenant
            .policies
            .get(&key)
            .map(|policy| policy.workload_ids.clone())
            .ok_or_else(|| {
                format!("unknown model route policy version: {policy_id}@{policy_version}")
            })?;
        if matches!(state, "canary" | "promoted")
            && tenant.policies.values().any(|existing| {
                existing.policy_id != policy_id
                    && matches!(existing.state.as_str(), "promoted" | "canary")
                    && existing
                        .workload_ids
                        .iter()
                        .any(|workload| workload_ids.contains(workload))
            })
        {
            return Err("another active policy already binds one of these workloads".to_string());
        }
        let policy = tenant.policies.get_mut(&key).ok_or_else(|| {
            format!("unknown model route policy version: {policy_id}@{policy_version}")
        })?;
        if policy.lifecycle != "adaptive" {
            return Err("static policies do not use adaptive transitions".to_string());
        }
        policy.state = state.to_string();
        if state == "canary" && policy.canary_percent == 0 {
            policy.canary_percent = 10;
        }
        if matches!(state, "promoted" | "rollback_required") {
            policy.canary_percent = 0;
        }
        Ok(policy.clone())
    }

    pub(crate) fn restore_from_kernel<S: mdx_core::StorageProvider>(
        &self,
        kernel: &mdx_core::MdxKernel<S>,
        secrets: &crate::secret_store::SecretStore,
    ) -> Result<(), String> {
        for receipt in kernel.ledger().entries() {
            if receipt.kind == mdx_core::MODEL_CONNECTION_CONFIGURED_RECEIPT_KIND {
                let provider_id = payload(receipt, "provider_id");
                let credential_ref = payload(receipt, "credential_ref");
                if let Some(provider) = mdx_core::model_provider(provider_id) {
                    secrets.request_credential_refresh_for_connection(
                        receipt.tenant_id.as_str(),
                        credential_ref,
                        provider.credential_env,
                    );
                }
                let credential_available = mdx_core::model_provider(provider_id)
                    .map(|provider| {
                        matches!(provider.provider_id, "ollama" | "vllm" | "tensorzero")
                            || secrets.has_credential_for_connection(
                                receipt.tenant_id.as_str(),
                                credential_ref,
                                provider.credential_env,
                            )
                    })
                    .unwrap_or(false);
                self.upsert_connection(ProviderConnection {
                    connection_id: payload(receipt, "connection_id").to_string(),
                    tenant_id: receipt.tenant_id.as_str().to_string(),
                    provider_id: provider_id.to_string(),
                    credential_ref: credential_ref.to_string(),
                    endpoint_base_url: payload(receipt, "endpoint_base_url").to_string(),
                    region: payload(receipt, "region").to_string(),
                    residency: payload(receipt, "residency").to_string(),
                    data_retention: payload(receipt, "data_retention").to_string(),
                    training_policy: payload(receipt, "training_policy").to_string(),
                    data_policy_provenance: payload(receipt, "data_policy_provenance").to_string(),
                    data_policy_observed_at: payload(receipt, "data_policy_observed_at")
                        .to_string(),
                    health: if credential_available {
                        payload(receipt, "health").to_string()
                    } else {
                        "disconnected".to_string()
                    },
                    live_call_allowed: credential_available
                        && payload(receipt, "live_call_allowed") == "true",
                })?;
            }
        }
        for receipt in kernel.ledger().entries() {
            if receipt.kind != mdx_core::MODEL_DEPLOYMENT_REGISTERED_RECEIPT_KIND {
                continue;
            }
            let tenant_id = receipt.tenant_id.as_str();
            let model = Model {
                model_id: payload(receipt, "model_id").to_string(),
                provider_model_id: payload(receipt, "provider_model_id").to_string(),
                provider_id: payload(receipt, "provider_id").to_string(),
                display_name: payload(receipt, "display_name").to_string(),
                lifecycle: payload(receipt, "lifecycle").to_string(),
                modality: payload(receipt, "modality").to_string(),
            };
            let deployment = ModelDeployment {
                deployment_id: payload(receipt, "deployment_id").to_string(),
                connection_id: payload(receipt, "connection_id").to_string(),
                provider_id: payload(receipt, "provider_id").to_string(),
                model_id: model.model_id.clone(),
                privacy_class: payload(receipt, "privacy_class").to_string(),
                region: payload(receipt, "region").to_string(),
                residency: payload(receipt, "residency").to_string(),
                enabled: payload(receipt, "enabled") == "true",
                capabilities: mdx_core::CapabilityProfile {
                    tools: payload(receipt, "tools") == "true",
                    structured_output: payload(receipt, "structured_output") == "true",
                    vision: payload(receipt, "vision") == "true",
                    context_tokens: payload(receipt, "context_tokens").parse().unwrap_or(0),
                    provenance: payload(receipt, "capability_provenance").to_string(),
                    observed_at: payload(receipt, "capability_observed_at").to_string(),
                },
            };
            let price = ModelPriceObservation {
                deployment_id: deployment.deployment_id.clone(),
                input_microusd_per_million: payload(receipt, "input_microusd_per_million")
                    .parse()
                    .unwrap_or(0),
                output_microusd_per_million: payload(receipt, "output_microusd_per_million")
                    .parse()
                    .unwrap_or(0),
                currency: payload(receipt, "currency").to_string(),
                source: payload(receipt, "price_source").to_string(),
                observed_at: payload(receipt, "price_observed_at").to_string(),
            };
            self.upsert_model(tenant_id, model)?;
            self.upsert_deployment(tenant_id, deployment, price)?;
        }
        for receipt in kernel.ledger().entries() {
            if receipt.kind != mdx_core::MODEL_ROUTE_POLICY_CONFIGURED_RECEIPT_KIND {
                continue;
            }
            self.upsert_policy(mdx_core::RoutePolicy {
                policy_id: payload(receipt, "policy_id").to_string(),
                policy_version: payload(receipt, "policy_version").to_string(),
                lifecycle: value_or(payload(receipt, "lifecycle"), "static").to_string(),
                state: value_or(payload(receipt, "state"), "promoted").to_string(),
                canary_percent: payload(receipt, "canary_percent").parse().unwrap_or(0),
                tenant_id: receipt.tenant_id.as_str().to_string(),
                workload_ids: split_values(payload(receipt, "workload_ids")),
                allowed_app_ids: split_values(payload(receipt, "allowed_app_ids")),
                allowed_environments: split_values(payload(receipt, "allowed_environments")),
                allowed_provider_ids: split_values(payload(receipt, "allowed_provider_ids")),
                preferred_deployment_ids: split_values(payload(
                    receipt,
                    "preferred_deployment_ids",
                )),
                denied_provider_ids: split_values(payload(receipt, "denied_provider_ids")),
                denied_deployment_ids: split_values(payload(receipt, "denied_deployment_ids")),
                allowed_regions: split_values(payload(receipt, "allowed_regions")),
                required_residency: nonempty(payload(receipt, "required_residency")),
                allow_data_retention: payload(receipt, "allow_data_retention") == "true",
                allow_training: payload(receipt, "allow_training") == "true",
                max_input_cost_microusd_per_million: payload(
                    receipt,
                    "max_input_cost_microusd_per_million",
                )
                .parse()
                .unwrap_or(0),
                max_output_cost_microusd_per_million: payload(
                    receipt,
                    "max_output_cost_microusd_per_million",
                )
                .parse()
                .unwrap_or(0),
            })?;
        }
        for receipt in kernel.ledger().entries() {
            if matches!(
                receipt.kind.as_str(),
                mdx_core::MODEL_ADAPTIVE_POLICY_EVALUATED_RECEIPT_KIND
                    | mdx_core::MODEL_ADAPTIVE_POLICY_PROMOTED_RECEIPT_KIND
                    | mdx_core::MODEL_ADAPTIVE_POLICY_AUTO_ROLLBACK_RECEIPT_KIND
            ) {
                let _ = self.transition_policy(
                    receipt.tenant_id.as_str(),
                    payload(receipt, "policy_id"),
                    payload(receipt, "policy_version"),
                    payload(receipt, "state"),
                );
            }
        }
        for receipt in kernel.ledger().entries() {
            if receipt.kind != mdx_core::MODEL_CIRCUIT_TRANSITION_RECEIPT_KIND {
                continue;
            }
            let tenant_id = receipt.tenant_id.as_str();
            let mut tenants = self
                .tenants
                .write()
                .map_err(|_| "model fabric registry lock poisoned".to_string())?;
            let Some(tenant) = tenants.get_mut(tenant_id) else {
                continue;
            };
            let connection_id = payload(receipt, "connection_id").to_string();
            if payload(receipt, "state") == "open" {
                tenant.consecutive_failures.insert(
                    connection_id.clone(),
                    payload(receipt, "consecutive_failures")
                        .parse()
                        .unwrap_or(3),
                );
                tenant.circuit_open_until_epoch_seconds.insert(
                    connection_id.clone(),
                    payload(receipt, "open_until_epoch_seconds")
                        .parse()
                        .unwrap_or(0),
                );
                if let Some(connection) = tenant.connections.get_mut(&connection_id)
                    && connection.health != "disconnected"
                {
                    connection.health = "unhealthy".to_string();
                }
            } else {
                tenant.consecutive_failures.insert(connection_id.clone(), 0);
                tenant
                    .circuit_open_until_epoch_seconds
                    .remove(&connection_id);
                if let Some(connection) = tenant.connections.get_mut(&connection_id)
                    && connection.health != "disconnected"
                {
                    connection.health = "healthy".to_string();
                }
            }
        }
        Ok(())
    }

    pub(crate) fn snapshot_for_tenant(
        &self,
        tenant_id: &str,
    ) -> Result<TenantRegistrySnapshot, String> {
        let tenants = self
            .tenants
            .read()
            .map_err(|_| "model fabric registry lock poisoned".to_string())?;
        let Some(tenant) = tenants.get(tenant_id) else {
            return Ok(TenantRegistrySnapshot::default());
        };
        Ok(TenantRegistrySnapshot {
            connections: tenant.connections.values().cloned().collect(),
            models: tenant.models.values().cloned().collect(),
            deployments: tenant.deployments.values().cloned().collect(),
            prices: tenant.prices.values().cloned().collect(),
            policies: tenant.policies.values().cloned().collect(),
        })
    }

    pub(crate) fn tenant_has_routing_state(&self, tenant_id: &str) -> Result<bool, String> {
        let tenants = self
            .tenants
            .read()
            .map_err(|_| "model fabric registry lock poisoned".to_string())?;
        Ok(tenants.get(tenant_id).is_some_and(|tenant| {
            !tenant.connections.is_empty()
                || !tenant.models.is_empty()
                || !tenant.deployments.is_empty()
                || !tenant.policies.is_empty()
        }))
    }

    pub(crate) fn upsert_policy(&self, policy: mdx_core::RoutePolicy) -> Result<(), String> {
        let list_fields = [
            &policy.workload_ids,
            &policy.allowed_app_ids,
            &policy.allowed_environments,
            &policy.allowed_provider_ids,
            &policy.preferred_deployment_ids,
            &policy.denied_provider_ids,
            &policy.denied_deployment_ids,
            &policy.allowed_regions,
        ];
        if !valid_token(&policy.tenant_id, 256)
            || !valid_token(&policy.policy_id, 128)
            || !valid_token(&policy.policy_version, 128)
            || list_fields.iter().any(|values| {
                values.len() > 128 || values.iter().any(|value| !valid_token(value, 256))
            })
            || policy
                .required_residency
                .as_deref()
                .is_some_and(|value| !valid_token(value, 128))
            || policy
                .workload_ids
                .iter()
                .any(|value| mdx_core::model_workload(value).is_none())
            || policy
                .allowed_provider_ids
                .iter()
                .chain(policy.denied_provider_ids.iter())
                .any(|value| mdx_core::model_provider(value).is_none())
        {
            return Err("policy identity or overlay values are invalid or too large".to_string());
        }
        if !matches!(policy.lifecycle.as_str(), "static" | "adaptive")
            || !matches!(
                policy.state.as_str(),
                "draft" | "shadow" | "canary" | "promoted" | "rollback_required"
            )
            || policy.canary_percent > 100
        {
            return Err("policy lifecycle, state, or canary percentage is invalid".to_string());
        }
        let mut tenants = self
            .tenants
            .write()
            .map_err(|_| "model fabric registry lock poisoned".to_string())?;
        if !policy.preferred_deployment_ids.is_empty()
            && tenants.get(&policy.tenant_id).is_none_or(|tenant| {
                policy
                    .preferred_deployment_ids
                    .iter()
                    .any(|deployment_id| !tenant.deployments.contains_key(deployment_id))
            })
        {
            return Err("policy preference names an unregistered tenant deployment".to_string());
        }
        if matches!(policy.state.as_str(), "promoted" | "canary")
            && tenants.get(&policy.tenant_id).is_some_and(|tenant| {
                tenant.policies.values().any(|existing| {
                    existing.policy_id != policy.policy_id
                        && matches!(existing.state.as_str(), "promoted" | "canary")
                        && existing
                            .workload_ids
                            .iter()
                            .any(|workload| policy.workload_ids.contains(workload))
                })
            })
        {
            return Err("another active policy already binds one of these workloads".to_string());
        }
        tenants
            .entry(policy.tenant_id.clone())
            .or_default()
            .policies
            .insert(
                policy_key(&policy.policy_id, &policy.policy_version),
                policy,
            );
        Ok(())
    }

    pub(crate) fn policy_for_tenant(
        &self,
        tenant_id: &str,
        policy_id: &str,
        expected_version: Option<&str>,
        session_id: Option<&str>,
    ) -> Result<mdx_core::RoutePolicy, String> {
        let tenants = self
            .tenants
            .read()
            .map_err(|_| "model fabric registry lock poisoned".to_string())?;
        if policy_id == "core" {
            return Ok(mdx_core::RoutePolicy::core(tenant_id));
        }
        let policies = tenants
            .get(tenant_id)
            .map(|tenant| {
                tenant
                    .policies
                    .values()
                    .filter(|policy| policy.policy_id == policy_id)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if let Some(version) = expected_version {
            return policies
                .into_iter()
                .find(|policy| {
                    policy.policy_version == version
                        && matches!(policy.state.as_str(), "promoted" | "canary")
                })
                .cloned()
                .ok_or_else(|| {
                    format!("model route policy version is not active: {policy_id}@{version}")
                });
        }
        let mut promoted = policies
            .iter()
            .copied()
            .filter(|policy| policy.state == "promoted")
            .collect::<Vec<_>>();
        promoted.sort_by(|a, b| compare_versions(&a.policy_version, &b.policy_version));
        let baseline = promoted.last().copied();
        let mut canaries = policies
            .iter()
            .copied()
            .filter(|policy| policy.state == "canary" && policy.canary_percent > 0)
            .collect::<Vec<_>>();
        canaries.sort_by(|a, b| compare_versions(&a.policy_version, &b.policy_version));
        if let (Some(session), Some(canary)) = (session_id, canaries.last().copied())
            && stable_bucket(session, policy_id) < canary.canary_percent
        {
            return Ok(canary.clone());
        }
        baseline
            .cloned()
            .ok_or_else(|| format!("model route policy has no promoted version: {policy_id}"))
    }

    pub(crate) fn policy_id_for_workload(
        &self,
        tenant_id: &str,
        workload_id: &str,
    ) -> Result<Option<String>, String> {
        let tenants = self
            .tenants
            .read()
            .map_err(|_| "model fabric registry lock poisoned".to_string())?;
        let Some(tenant) = tenants.get(tenant_id) else {
            return Ok(None);
        };
        let mut matches = tenant
            .policies
            .values()
            .filter(|policy| {
                policy.workload_ids.iter().any(|id| id == workload_id)
                    && matches!(policy.state.as_str(), "promoted" | "canary")
            })
            .map(|policy| policy.policy_id.clone())
            .collect::<Vec<_>>();
        matches.sort();
        matches.dedup();
        if matches.len() > 1 {
            return Err(format!(
                "multiple active policies bind workload {workload_id}"
            ));
        }
        Ok(matches.pop())
    }

    pub(crate) fn upsert_connection(&self, connection: ProviderConnection) -> Result<(), String> {
        validate_connection(&connection)?;
        let mut tenants = self
            .tenants
            .write()
            .map_err(|_| "model fabric registry lock poisoned".to_string())?;
        tenants
            .entry(connection.tenant_id.clone())
            .or_default()
            .connections
            .insert(connection.connection_id.clone(), connection);
        Ok(())
    }

    pub(crate) fn upsert_model(&self, tenant_id: &str, model: Model) -> Result<(), String> {
        if !valid_token(tenant_id, 256)
            || !valid_token(&model.model_id, 256)
            || !valid_token(&model.provider_model_id, 256)
            || !valid_token(&model.provider_id, 128)
            || !valid_text(&model.display_name, 256)
            || !valid_token(&model.lifecycle, 64)
            || !valid_token(&model.modality, 64)
            || mdx_core::model_provider(&model.provider_id).is_none()
        {
            return Err(
                "model identity, provider, or metadata is invalid or too large".to_string(),
            );
        }
        let mut tenants = self
            .tenants
            .write()
            .map_err(|_| "model fabric registry lock poisoned".to_string())?;
        tenants
            .entry(tenant_id.to_string())
            .or_default()
            .models
            .insert(model.model_id.clone(), model);
        Ok(())
    }

    pub(crate) fn upsert_deployment(
        &self,
        tenant_id: &str,
        deployment: ModelDeployment,
        price: ModelPriceObservation,
    ) -> Result<(), String> {
        if !valid_token(tenant_id, 256)
            || !valid_token(&deployment.deployment_id, 256)
            || !valid_token(&deployment.connection_id, 256)
            || !valid_token(&deployment.provider_id, 128)
            || !valid_token(&deployment.model_id, 256)
            || !matches!(
                deployment.privacy_class.as_str(),
                "open" | "frontier" | "sovereign" | "local"
            )
            || !valid_token(&deployment.region, 128)
            || !valid_token(&deployment.residency, 128)
            || !valid_text(&deployment.capabilities.provenance, 2_048)
            || !valid_token(&deployment.capabilities.observed_at, 64)
            || deployment.capabilities.context_tokens > 10_000_000
            || !valid_text(&price.source, 2_048)
            || !valid_token(&price.observed_at, 64)
            || !valid_token(&price.currency, 16)
        {
            return Err(
                "deployment capability or price metadata is invalid or too large".to_string(),
            );
        }
        let mut tenants = self
            .tenants
            .write()
            .map_err(|_| "model fabric registry lock poisoned".to_string())?;
        let tenant = tenants.entry(tenant_id.to_string()).or_default();
        let Some(connection) = tenant.connections.get(&deployment.connection_id) else {
            return Err("deployment connection is not registered for tenant".to_string());
        };
        if connection.provider_id != deployment.provider_id {
            return Err("deployment provider does not match connection".to_string());
        }
        validate_deployment_privacy(connection, &deployment)?;
        if !tenant.models.contains_key(&deployment.model_id) {
            return Err("deployment model is not registered for tenant".to_string());
        }
        if price.deployment_id != deployment.deployment_id {
            return Err("price observation does not match deployment".to_string());
        }
        tenant
            .prices
            .insert(deployment.deployment_id.clone(), price);
        tenant
            .deployments
            .insert(deployment.deployment_id.clone(), deployment);
        Ok(())
    }

    pub(crate) fn candidates_for_tenant(
        &self,
        tenant_id: &str,
    ) -> Result<Vec<OwnedRouteCandidate>, String> {
        let tenants = self
            .tenants
            .read()
            .map_err(|_| "model fabric registry lock poisoned".to_string())?;
        let Some(tenant) = tenants.get(tenant_id) else {
            return Ok(Vec::new());
        };
        let now = epoch_seconds();
        Ok(tenant
            .deployments
            .values()
            .filter_map(|deployment| {
                let connection = tenant.connections.get(&deployment.connection_id)?;
                let price = tenant.prices.get(&deployment.deployment_id)?;
                Some(OwnedRouteCandidate {
                    deployment_id: deployment.deployment_id.clone(),
                    connection_id: deployment.connection_id.clone(),
                    tenant_id: tenant_id.to_string(),
                    provider_id: deployment.provider_id.clone(),
                    credential_ref: connection.credential_ref.clone(),
                    model_id: deployment.model_id.clone(),
                    privacy_class: deployment.privacy_class.clone(),
                    region: deployment.region.clone(),
                    residency: deployment.residency.clone(),
                    data_retention: connection.data_retention.clone(),
                    training_policy: connection.training_policy.clone(),
                    connected: connection.health != "disconnected",
                    healthy: connection.health == "healthy"
                        || connection.health == "observed"
                        || (connection.health == "unhealthy"
                            && tenant
                                .circuit_open_until_epoch_seconds
                                .get(&connection.connection_id)
                                .is_some_and(|until| *until <= now)),
                    live_call_allowed: connection.live_call_allowed && deployment.enabled,
                    tools: deployment.capabilities.tools,
                    structured_output: deployment.capabilities.structured_output,
                    vision: deployment.capabilities.vision,
                    context_tokens: deployment.capabilities.context_tokens,
                    input_cost_microusd_per_million: price.input_microusd_per_million,
                    output_cost_microusd_per_million: price.output_microusd_per_million,
                    price_known: price.source != "unknown" && !price.source.trim().is_empty(),
                    p95_latency_ms: 0,
                    latency_known: false,
                    quality_score: 0,
                    quality_known: false,
                })
            })
            .collect())
    }

    pub(crate) fn report_execution(
        &self,
        tenant_id: &str,
        deployment_id: &str,
        succeeded: bool,
    ) -> Result<(), String> {
        let mut tenants = self
            .tenants
            .write()
            .map_err(|_| "model fabric registry lock poisoned".to_string())?;
        let tenant = tenants
            .get_mut(tenant_id)
            .ok_or_else(|| "model execution tenant is not registered".to_string())?;
        let connection_id = tenant
            .deployments
            .get(deployment_id)
            .map(|deployment| deployment.connection_id.clone())
            .ok_or_else(|| "model execution deployment is not registered".to_string())?;
        if succeeded {
            let was_open = tenant
                .circuit_open_until_epoch_seconds
                .remove(&connection_id)
                .is_some();
            tenant.consecutive_failures.insert(connection_id.clone(), 0);
            if let Some(connection) = tenant.connections.get_mut(&connection_id) {
                connection.health = "healthy".to_string();
            }
            if was_open {
                tenant.pending_circuit_transitions.push(CircuitTransition {
                    connection_id,
                    deployment_id: deployment_id.to_string(),
                    state: "closed".to_string(),
                    consecutive_failures: 0,
                    open_until_epoch_seconds: 0,
                });
            }
            return Ok(());
        }
        let failures = tenant
            .consecutive_failures
            .entry(connection_id.clone())
            .or_default();
        *failures = failures.saturating_add(1);
        if *failures >= 3 {
            let now = epoch_seconds();
            let already_open = tenant
                .circuit_open_until_epoch_seconds
                .get(&connection_id)
                .is_some_and(|until| *until > now);
            let open_until = now.saturating_add(60);
            tenant
                .circuit_open_until_epoch_seconds
                .insert(connection_id.clone(), open_until);
            if let Some(connection) = tenant.connections.get_mut(&connection_id) {
                connection.health = "unhealthy".to_string();
            }
            if !already_open {
                tenant.pending_circuit_transitions.push(CircuitTransition {
                    connection_id,
                    deployment_id: deployment_id.to_string(),
                    state: "open".to_string(),
                    consecutive_failures: *failures,
                    open_until_epoch_seconds: open_until,
                });
            }
        }
        Ok(())
    }

    pub(crate) fn take_pending_circuit_transitions(
        &self,
        tenant_id: &str,
    ) -> Result<Vec<CircuitTransition>, String> {
        let mut tenants = self
            .tenants
            .write()
            .map_err(|_| "model fabric registry lock poisoned".to_string())?;
        Ok(tenants
            .get_mut(tenant_id)
            .map(|tenant| std::mem::take(&mut tenant.pending_circuit_transitions))
            .unwrap_or_default())
    }

    pub(crate) fn report_failed_attempt(
        &self,
        tenant_id: &str,
        attempt: FailedInferenceAttempt,
    ) -> Result<(), String> {
        let mut tenants = self
            .tenants
            .write()
            .map_err(|_| "model fabric registry lock poisoned".to_string())?;
        let tenant = tenants
            .get_mut(tenant_id)
            .ok_or_else(|| "model inference tenant is not registered".to_string())?;
        if !tenant.deployments.contains_key(&attempt.deployment_id) {
            return Err("model inference deployment is not registered".to_string());
        }
        tenant.pending_failed_attempts.push(attempt);
        Ok(())
    }

    pub(crate) fn take_pending_failed_attempts(
        &self,
        tenant_id: &str,
    ) -> Result<Vec<FailedInferenceAttempt>, String> {
        let mut tenants = self
            .tenants
            .write()
            .map_err(|_| "model fabric registry lock poisoned".to_string())?;
        Ok(tenants
            .get_mut(tenant_id)
            .map(|tenant| std::mem::take(&mut tenant.pending_failed_attempts))
            .unwrap_or_default())
    }

    pub(crate) fn deployment_for_tenant(
        &self,
        tenant_id: &str,
        deployment_id: &str,
    ) -> Result<Option<ResolvedRegistryDeployment>, String> {
        let tenants = self
            .tenants
            .read()
            .map_err(|_| "model fabric registry lock poisoned".to_string())?;
        let Some(tenant) = tenants.get(tenant_id) else {
            return Ok(None);
        };
        let Some(deployment) = tenant.deployments.get(deployment_id) else {
            return Ok(None);
        };
        let Some(connection) = tenant.connections.get(&deployment.connection_id) else {
            return Ok(None);
        };
        let Some(model) = tenant.models.get(&deployment.model_id) else {
            return Ok(None);
        };
        let Some(price) = tenant.prices.get(deployment_id) else {
            return Ok(None);
        };
        Ok(Some(ResolvedRegistryDeployment {
            connection: connection.clone(),
            model: model.clone(),
            deployment: deployment.clone(),
            price: price.clone(),
        }))
    }

    #[cfg(test)]
    pub(crate) fn clear_tenant(&self, tenant_id: &str) {
        if let Ok(mut tenants) = self.tenants.write() {
            tenants.remove(tenant_id);
        }
    }
}

fn payload<'a>(receipt: &'a mdx_core::Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}

fn split_values(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

fn nonempty(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.to_string())
}

fn value_or<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.is_empty() { fallback } else { value }
}

fn policy_key(policy_id: &str, policy_version: &str) -> String {
    format!("{policy_id}@{policy_version}")
}

fn stable_bucket(session_id: &str, policy_id: &str) -> u8 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in session_id.bytes().chain(policy_id.bytes()) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    (hash % 100) as u8
}

fn compare_versions(left: &str, right: &str) -> std::cmp::Ordering {
    let numeric = |value: &str| {
        value
            .split(|character: char| !character.is_ascii_digit())
            .filter(|part| !part.is_empty())
            .filter_map(|part| part.parse::<u64>().ok())
            .collect::<Vec<_>>()
    };
    numeric(left)
        .cmp(&numeric(right))
        .then_with(|| left.cmp(right))
}

fn epoch_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn validate_connection(connection: &ProviderConnection) -> Result<(), String> {
    if !valid_token(&connection.connection_id, 256)
        || !valid_token(&connection.tenant_id, 256)
        || !valid_token(&connection.provider_id, 128)
        || !valid_text(&connection.credential_ref, 256)
        || !valid_text(&connection.endpoint_base_url, 2_048)
        || !valid_token(&connection.region, 128)
        || !valid_token(&connection.residency, 128)
        || !valid_text(&connection.data_policy_provenance, 2_048)
        || !valid_token(&connection.data_policy_observed_at, 64)
    {
        return Err(
            "connection id, tenant, provider, credential reference, endpoint, and policy provenance are required"
                .to_string(),
        );
    }
    let provider = mdx_core::model_provider(&connection.provider_id)
        .ok_or_else(|| "connection provider is not declared".to_string())?;
    crate::model_gateway_runtime::validate_base_url(
        &connection.endpoint_base_url,
        matches!(provider.provider_id, "ollama" | "vllm" | "tensorzero"),
    )?;
    if !matches!(
        connection.health.as_str(),
        "disconnected" | "healthy" | "observed" | "unhealthy"
    ) {
        return Err("unknown connection health".to_string());
    }
    if !matches!(
        connection.data_retention.as_str(),
        "none" | "provider" | "unknown"
    ) {
        return Err("unknown data retention policy".to_string());
    }
    if !matches!(
        connection.training_policy.as_str(),
        "none" | "allowed" | "unknown"
    ) {
        return Err("unknown training policy".to_string());
    }
    Ok(())
}

fn validate_deployment_privacy(
    connection: &ProviderConnection,
    deployment: &ModelDeployment,
) -> Result<(), String> {
    if !matches!(deployment.privacy_class.as_str(), "local" | "sovereign") {
        return Ok(());
    }
    let provider = mdx_core::model_provider(&deployment.provider_id)
        .ok_or_else(|| "deployment provider is not declared".to_string())?;
    if !matches!(
        provider.hosting_class,
        "local" | "self_hosted" | "self_hosted_gateway"
    ) {
        return Err(format!(
            "{} providers cannot claim {} privacy",
            provider.hosting_class, deployment.privacy_class
        ));
    }
    let loopback = endpoint_is_loopback(&connection.endpoint_base_url);
    if deployment.privacy_class == "local" && !loopback {
        return Err("local privacy requires a loopback endpoint".to_string());
    }
    if deployment.privacy_class == "sovereign"
        && !loopback
        && !endpoint_is_sovereign_allowlisted(&connection.endpoint_base_url)
    {
        return Err(
            "sovereign privacy requires a loopback or operator-allowlisted endpoint".to_string(),
        );
    }
    Ok(())
}

fn endpoint_is_loopback(base_url: &str) -> bool {
    url::Url::parse(base_url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .is_some_and(|host| {
            host.eq_ignore_ascii_case("localhost")
                || host
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|address| address.is_loopback())
        })
}

fn endpoint_is_sovereign_allowlisted(base_url: &str) -> bool {
    let Ok(endpoint) = url::Url::parse(base_url) else {
        return false;
    };
    std::env::var("MDX_MODEL_SOVEREIGN_ENDPOINT_ALLOWLIST")
        .ok()
        .into_iter()
        .flat_map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|entry| !entry.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .filter_map(|entry| url::Url::parse(&entry).ok())
        .any(|allowed| {
            allowed.scheme() == "https"
                && endpoint.scheme() == allowed.scheme()
                && endpoint.host_str() == allowed.host_str()
                && endpoint.port_or_known_default() == allowed.port_or_known_default()
                && allowed.username().is_empty()
                && allowed.password().is_none()
                && allowed.query().is_none()
                && allowed.fragment().is_none()
                && allowed.path() == "/"
        })
}

fn valid_token(value: &str, max_chars: usize) -> bool {
    valid_text(value, max_chars) && !value.contains(',')
}

fn valid_text(value: &str, max_chars: usize) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= max_chars
        && !value.chars().any(char::is_control)
}

pub(crate) fn global() -> &'static ModelFabricRegistry {
    static REGISTRY: OnceLock<ModelFabricRegistry> = OnceLock::new();
    REGISTRY.get_or_init(ModelFabricRegistry::default)
}

#[cfg(test)]
pub(crate) fn observed_capability_profile(
    tools: bool,
    structured_output: bool,
    vision: bool,
    context_tokens: u64,
    provenance: &str,
    observed_at: &str,
) -> mdx_core::CapabilityProfile {
    mdx_core::CapabilityProfile {
        tools,
        structured_output,
        vision,
        context_tokens,
        provenance: provenance.to_string(),
        observed_at: observed_at.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdx_core::{ActorId, CorrelationIds, LoopId, TenantId, TraceId, WorkflowId};

    fn test_connection(
        tenant_id: &str,
        provider_id: &str,
        endpoint_base_url: &str,
    ) -> ProviderConnection {
        ProviderConnection {
            connection_id: format!("{tenant_id}:{provider_id}"),
            tenant_id: tenant_id.to_string(),
            provider_id: provider_id.to_string(),
            credential_ref: "test:credential".to_string(),
            endpoint_base_url: endpoint_base_url.to_string(),
            region: "test".to_string(),
            residency: "test".to_string(),
            data_retention: "none".to_string(),
            training_policy: "none".to_string(),
            data_policy_provenance: "test".to_string(),
            data_policy_observed_at: "2026-07-13".to_string(),
            health: "healthy".to_string(),
            live_call_allowed: true,
        }
    }

    fn test_model(provider_id: &str) -> Model {
        Model {
            model_id: format!("{provider_id}/test"),
            provider_model_id: "test".to_string(),
            provider_id: provider_id.to_string(),
            display_name: "Test".to_string(),
            lifecycle: "active".to_string(),
            modality: "text".to_string(),
        }
    }

    fn test_deployment(tenant_id: &str, provider_id: &str, privacy_class: &str) -> ModelDeployment {
        ModelDeployment {
            deployment_id: format!("{tenant_id}:{provider_id}/test"),
            connection_id: format!("{tenant_id}:{provider_id}"),
            provider_id: provider_id.to_string(),
            model_id: format!("{provider_id}/test"),
            privacy_class: privacy_class.to_string(),
            region: "test".to_string(),
            residency: "test".to_string(),
            enabled: true,
            capabilities: observed_capability_profile(
                true,
                true,
                false,
                8_192,
                "test",
                "2026-07-13",
            ),
        }
    }

    fn test_price(deployment_id: &str) -> ModelPriceObservation {
        ModelPriceObservation {
            deployment_id: deployment_id.to_string(),
            input_microusd_per_million: 1,
            output_microusd_per_million: 1,
            currency: "USD".to_string(),
            source: "test".to_string(),
            observed_at: "2026-07-13".to_string(),
        }
    }

    #[test]
    fn managed_provider_cannot_self_label_as_sovereign_or_local() {
        let registry = ModelFabricRegistry::default();
        let tenant_id = "managed-privacy-tenant";
        registry
            .upsert_connection(test_connection(
                tenant_id,
                "openai",
                "https://api.openai.com/v1",
            ))
            .expect("connection");
        registry
            .upsert_model(tenant_id, test_model("openai"))
            .expect("model");
        for privacy_class in ["sovereign", "local"] {
            let deployment = test_deployment(tenant_id, "openai", privacy_class);
            let error = registry
                .upsert_deployment(
                    tenant_id,
                    deployment.clone(),
                    test_price(&deployment.deployment_id),
                )
                .expect_err("managed endpoint must not gain a self-asserted private class");
            assert!(error.contains("frontier_managed providers cannot claim"));
        }
    }

    #[test]
    fn self_hosted_sovereign_endpoint_requires_loopback_or_operator_allowlist() {
        let _env = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let previous = std::env::var("MDX_MODEL_SOVEREIGN_ENDPOINT_ALLOWLIST").ok();
        unsafe {
            std::env::remove_var("MDX_MODEL_SOVEREIGN_ENDPOINT_ALLOWLIST");
        }
        let registry = ModelFabricRegistry::default();
        let tenant_id = "sovereign-endpoint-tenant";
        registry
            .upsert_connection(test_connection(
                tenant_id,
                "vllm",
                "https://models.example.test/v1",
            ))
            .expect("connection");
        registry
            .upsert_model(tenant_id, test_model("vllm"))
            .expect("model");
        let sovereign = test_deployment(tenant_id, "vllm", "sovereign");
        assert!(
            registry
                .upsert_deployment(
                    tenant_id,
                    sovereign.clone(),
                    test_price(&sovereign.deployment_id),
                )
                .expect_err("unlisted remote endpoint")
                .contains("operator-allowlisted")
        );

        unsafe {
            std::env::set_var(
                "MDX_MODEL_SOVEREIGN_ENDPOINT_ALLOWLIST",
                "https://models.example.test",
            );
        }
        registry
            .upsert_deployment(
                tenant_id,
                sovereign.clone(),
                test_price(&sovereign.deployment_id),
            )
            .expect("allowlisted sovereign endpoint");
        let local = test_deployment(tenant_id, "vllm", "local");
        assert!(
            registry
                .upsert_deployment(tenant_id, local.clone(), test_price(&local.deployment_id),)
                .expect_err("remote endpoint cannot claim local")
                .contains("loopback")
        );
        unsafe {
            if let Some(value) = previous {
                std::env::set_var("MDX_MODEL_SOVEREIGN_ENDPOINT_ALLOWLIST", value);
            } else {
                std::env::remove_var("MDX_MODEL_SOVEREIGN_ENDPOINT_ALLOWLIST");
            }
        }
    }

    #[test]
    fn receipt_restore_requires_current_credential_presence() {
        let mut kernel = mdx_core::MdxKernel::boot_local();
        let correlation = CorrelationIds {
            tenant_id: TenantId::new("restore-tenant"),
            trace_id: TraceId::new("restore-trace"),
            actor_id: ActorId::new("human:restore"),
            loop_id: LoopId::new("model_fabric"),
            workflow_id: WorkflowId::new("restore-workflow"),
        };
        let connection = ProviderConnection {
            connection_id: "restore-tenant:openai".to_string(),
            tenant_id: "restore-tenant".to_string(),
            provider_id: "openai".to_string(),
            credential_ref: "secret:session/OPENAI_API_KEY".to_string(),
            endpoint_base_url: "https://api.openai.com/v1".to_string(),
            region: "global".to_string(),
            residency: "provider_managed".to_string(),
            data_retention: "unknown".to_string(),
            training_policy: "unknown".to_string(),
            data_policy_provenance: "operator_assertion:test".to_string(),
            data_policy_observed_at: "2026-07-12".to_string(),
            health: "observed".to_string(),
            live_call_allowed: true,
        };
        kernel.record_model_connection_configured(&correlation, &connection);
        kernel
            .record_model_circuit_transition(
                &correlation,
                "restore-tenant:openai",
                "restore-tenant:openai/test",
                "closed",
                0,
                0,
            )
            .expect("circuit close receipt");
        let registry = ModelFabricRegistry::default();
        let secrets = crate::secret_store::SecretStore::default();
        secrets.disable_keychain_lookup_for_tenant("restore-tenant", "OPENAI_API_KEY");
        registry
            .restore_from_kernel(&kernel, &secrets)
            .expect("restore");
        let snapshot = registry
            .snapshot_for_tenant("restore-tenant")
            .expect("snapshot");
        assert_eq!(snapshot.connections[0].health, "disconnected");
        assert!(!snapshot.connections[0].live_call_allowed);

        secrets.set_for_tenant("restore-tenant", "OPENAI_API_KEY", "test-key");
        registry
            .restore_from_kernel(&kernel, &secrets)
            .expect("restore with key");
        let snapshot = registry
            .snapshot_for_tenant("restore-tenant")
            .expect("snapshot with key");
        assert_eq!(snapshot.connections[0].health, "healthy");
        assert!(snapshot.connections[0].live_call_allowed);
    }

    #[test]
    fn missing_named_policy_fails_closed_while_core_is_available() {
        let registry = ModelFabricRegistry::default();
        assert_eq!(
            registry
                .policy_for_tenant("tenant-a", "core", None, None)
                .expect("generated core policy")
                .policy_id,
            "core"
        );
        assert!(
            registry
                .policy_for_tenant("tenant-a", "not-configured", None, None)
                .is_err()
        );
    }

    #[test]
    fn actor_identity_can_supply_the_stable_canary_bucket() {
        let registry = ModelFabricRegistry::default();
        let tenant_id = "actor-canary-tenant";
        let mut baseline = mdx_core::RoutePolicy::core(tenant_id);
        baseline.policy_id = "twin-default".to_string();
        baseline.policy_version = "v1".to_string();
        baseline.workload_ids = vec!["mdx/twin/conversation".to_string()];
        registry.upsert_policy(baseline.clone()).expect("baseline");
        let mut canary = baseline;
        canary.policy_version = "v2".to_string();
        canary.lifecycle = "adaptive".to_string();
        canary.state = "canary".to_string();
        canary.canary_percent = 100;
        registry.upsert_policy(canary).expect("canary");
        let selected = registry
            .policy_for_tenant(tenant_id, "twin-default", None, Some("human:stable-actor"))
            .expect("actor-stable canary");
        assert_eq!(selected.policy_version, "v2");
    }

    #[test]
    fn circuit_open_and_close_transitions_are_drainable_for_receipts() {
        let registry = ModelFabricRegistry::default();
        let tenant_id = "circuit-tenant";
        registry
            .upsert_connection(ProviderConnection {
                connection_id: "circuit-tenant:ollama".to_string(),
                tenant_id: tenant_id.to_string(),
                provider_id: "ollama".to_string(),
                credential_ref: "local:none".to_string(),
                endpoint_base_url: "http://127.0.0.1:11434".to_string(),
                region: "local".to_string(),
                residency: "local".to_string(),
                data_retention: "none".to_string(),
                training_policy: "none".to_string(),
                data_policy_provenance: "test".to_string(),
                data_policy_observed_at: "2026-07-12".to_string(),
                health: "healthy".to_string(),
                live_call_allowed: true,
            })
            .expect("connection");
        registry
            .upsert_model(
                tenant_id,
                Model {
                    model_id: "ollama/test".to_string(),
                    provider_model_id: "test".to_string(),
                    provider_id: "ollama".to_string(),
                    display_name: "Test".to_string(),
                    lifecycle: "active".to_string(),
                    modality: "text".to_string(),
                },
            )
            .expect("model");
        registry
            .upsert_deployment(
                tenant_id,
                ModelDeployment {
                    deployment_id: "circuit-tenant:ollama/test".to_string(),
                    connection_id: "circuit-tenant:ollama".to_string(),
                    provider_id: "ollama".to_string(),
                    model_id: "ollama/test".to_string(),
                    privacy_class: "local".to_string(),
                    region: "local".to_string(),
                    residency: "local".to_string(),
                    enabled: true,
                    capabilities: observed_capability_profile(
                        true,
                        true,
                        false,
                        8_192,
                        "test",
                        "2026-07-12",
                    ),
                },
                ModelPriceObservation {
                    deployment_id: "circuit-tenant:ollama/test".to_string(),
                    input_microusd_per_million: 0,
                    output_microusd_per_million: 0,
                    currency: "USD".to_string(),
                    source: "test".to_string(),
                    observed_at: "2026-07-12".to_string(),
                },
            )
            .expect("deployment");
        for _ in 0..3 {
            registry
                .report_execution(tenant_id, "circuit-tenant:ollama/test", false)
                .expect("failure report");
        }
        let opened = registry
            .take_pending_circuit_transitions(tenant_id)
            .expect("open transition");
        assert_eq!(opened.len(), 1);
        assert_eq!(opened[0].state, "open");
        assert_eq!(opened[0].consecutive_failures, 3);

        registry
            .report_execution(tenant_id, "circuit-tenant:ollama/test", true)
            .expect("success report");
        let closed = registry
            .take_pending_circuit_transitions(tenant_id)
            .expect("close transition");
        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0].state, "closed");
    }
}
