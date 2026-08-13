use crate::*;

pub const TWIN_ARTIFACT_DEFAULT_PRIVACY_CLASS: &str = "user_private_sensitive";
pub const TWIN_ARTIFACT_CONTEXT_POLICY: &str = "local_twin_artifact_context_packet_v1";
pub const TWIN_ARTIFACT_GROUNDING_POLICY: &str = "local_company_context_grounding_v1";
pub const TWIN_ARTIFACT_STORAGE_POLICY: &str = "local_twin_artifact_storage_metadata_v1";
pub const TWIN_ARTIFACT_LITEPARSE_POLICY: &str = "local_liteparse_pdf_parser_v1";
pub const TWIN_ARTIFACT_LATENCY_BUDGET_MS: u16 = 250;
pub const TWIN_ARTIFACT_FILE_SIZE_LIMIT_BYTES: usize = 25 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TwinArtifactContextRequest<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub session_id: &'a str,
    pub artifact_name: &'a str,
    pub mime_type: &'a str,
    pub artifact_text: &'a str,
    pub privacy_class: &'a str,
    pub company_context: &'a str,
    pub company_context_source: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TwinArtifactContextReport {
    pub status: &'static str,
    pub artifact_id: String,
    pub storage_object_id: String,
    pub context_packet_id: String,
    pub grounding_packet_id: String,
    pub session_id: String,
    pub artifact_name: String,
    pub normalized_type: String,
    pub mime_type: String,
    pub privacy_class: String,
    pub parse_state: &'static str,
    pub next_move: &'static str,
    pub artifact_admission_receipt_id: String,
    pub storage_receipt_id: String,
    pub parse_receipt_id: String,
    pub grounding_receipt_id: String,
    pub policy_decision_id: String,
    pub source_artifact_hash: String,
    pub storage_path: String,
    pub storage_path_posture: &'static str,
    pub storage_contract_state: &'static str,
    pub retention_policy: &'static str,
    pub deletion_state: &'static str,
    pub download_authorization: &'static str,
    pub download_allowed: bool,
    pub blob_bytes_stored: bool,
    pub malware_scan_state: &'static str,
    pub quarantine_state: &'static str,
    pub file_size_limit_bytes: usize,
    pub byte_size: usize,
    pub extracted_text_char_count: usize,
    pub citation_count: u8,
    pub artifact_citation_label: &'static str,
    pub company_context_citation_label: &'static str,
    pub company_context_source: String,
    pub trusted_context_used: bool,
    pub trusted_context_source_count: usize,
    pub trusted_context_source_ids: String,
    pub trusted_context_receipt_ids: String,
    pub trusted_context_projection_route: &'static str,
    pub interpretation: String,
    pub proposed_next_action: &'static str,
    pub memory_eligible: bool,
    pub memory_write_performed: bool,
    pub provider_class: &'static str,
    pub provider_id: String,
    pub model_id: String,
    pub model_gateway_routing: String,
    pub provider_observation_receipt_id: String,
    pub sovereign_provider_required: bool,
    pub sovereign_provider_connected: bool,
    pub provider_not_connected_reason: String,
    pub frontier_refusal_reason: &'static str,
    pub provider_call_allowed: bool,
    pub frontier_raw_content_allowed: bool,
    pub storage_bucket_required: bool,
    pub parser_sandbox_required: bool,
    pub production_write_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TwinArtifactBlobStoredRequest<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub artifact_id: &'a str,
    pub source_body_ref: &'a str,
    pub extracted_text_ref: &'a str,
    pub source_body_hash: &'a str,
    pub extracted_text_hash: &'a str,
    pub byte_size: usize,
    pub extracted_text_char_count: usize,
    pub storage_path_posture: &'static str,
    pub storage_contract_state: &'static str,
    pub encrypted_at_rest_posture: &'static str,
    pub malware_scan_state: &'static str,
    pub quarantine_state: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TwinArtifactBlobStoredReport {
    pub storage_receipt_id: String,
    pub policy_decision_id: String,
    pub storage_path: String,
    pub storage_path_posture: &'static str,
    pub storage_contract_state: &'static str,
    pub download_authorization: &'static str,
    pub download_allowed: bool,
    pub blob_bytes_stored: bool,
    pub malware_scan_state: &'static str,
    pub quarantine_state: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TwinArtifactLiteParseRequest<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub artifact_id: &'a str,
    pub artifact_name: &'a str,
    pub mime_type: &'a str,
    pub source_body_ref: &'a str,
    pub extracted_text_ref: &'a str,
    pub source_body_hash: &'a str,
    pub extracted_text_hash: &'a str,
    pub byte_size: usize,
    pub page_count: usize,
    pub extracted_text_char_count: usize,
    pub parser_elapsed_ms: u128,
    pub storage_path_posture: &'static str,
    pub storage_contract_state: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TwinArtifactLiteParseReport {
    pub status: &'static str,
    pub artifact_id: String,
    pub artifact_name: String,
    pub mime_type: String,
    pub parser_id: &'static str,
    pub parser_version: &'static str,
    pub parser_profile: &'static str,
    pub parse_state: &'static str,
    pub output_format: &'static str,
    pub ocr_enabled: bool,
    pub source_body_ref: String,
    pub extracted_text_ref: String,
    pub source_body_hash: String,
    pub extracted_text_hash: String,
    pub byte_size: usize,
    pub page_count: usize,
    pub extracted_text_char_count: usize,
    pub parser_elapsed_ms: u128,
    pub liteparse_receipt_id: String,
    pub policy_decision_id: String,
    pub source_intake_surface: &'static str,
    pub source_trust_state: &'static str,
    pub trusted_context_state: &'static str,
    pub raw_private_content_recorded: bool,
    pub provider_call_allowed: bool,
    pub frontier_raw_content_allowed: bool,
    pub memory_write_performed: bool,
    pub production_write_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TwinArtifactDownloadRequest<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub artifact_id: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TwinArtifactDownloadReport {
    pub status: &'static str,
    pub artifact_id: String,
    pub artifact_name: String,
    pub mime_type: String,
    pub storage_object_id: String,
    pub source_body_ref: String,
    pub source_body_hash: String,
    pub byte_size: usize,
    pub download_receipt_id: String,
    pub policy_decision_id: String,
    pub source_artifact_receipt_id: String,
    pub storage_receipt_id: String,
    pub download_authorization: &'static str,
    pub download_allowed: bool,
    pub owner_authorized: bool,
    pub content_type: String,
    pub raw_private_content_recorded: bool,
    pub provider_call_allowed: bool,
    pub frontier_raw_content_allowed: bool,
    pub memory_write_performed: bool,
    pub production_write_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TwinArtifactDeletionRequest<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub artifact_id: &'a str,
    pub reason: &'a str,
    pub local_blob_bytes_deleted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TwinArtifactDeletionReport {
    pub status: &'static str,
    pub artifact_id: String,
    pub storage_object_id: String,
    pub context_packet_id: String,
    pub grounding_packet_id: String,
    pub deletion_state: &'static str,
    pub artifact_deleted_receipt_id: String,
    pub storage_tombstone_receipt_id: String,
    pub context_invalidated_receipt_id: String,
    pub policy_decision_id: String,
    pub source_artifact_receipt_id: String,
    pub storage_receipt_id: String,
    pub parse_receipt_id: String,
    pub grounding_receipt_id: String,
    pub blob_delete_required: bool,
    pub local_blob_bytes_deleted: bool,
    pub context_packets_invalidated: bool,
    pub memory_atoms_invalidated: bool,
    pub generated_artifacts_invalidated: bool,
    pub provider_call_allowed: bool,
    pub frontier_raw_content_allowed: bool,
    pub production_write_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TwinArtifactPagesDraftRequest<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub artifact_id: &'a str,
    pub pages_draft_id: &'a str,
    pub pages_document_id: &'a str,
    pub pages_title: &'a str,
    pub pages_body_ref: &'a str,
    pub pages_revision_id: &'a str,
    pub approval_request_id: &'a str,
    pub requested_visibility: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TwinArtifactPagesDraftReport {
    pub status: &'static str,
    pub artifact_id: String,
    pub pages_draft_id: String,
    pub pages_document_id: String,
    pub pages_title: String,
    pub pages_route: &'static str,
    pub draft_state: &'static str,
    pub approval_state: &'static str,
    pub pages_draft_receipt_id: String,
    pub approval_request_id: String,
    pub approval_request_receipt_id: String,
    pub twin_pages_draft_receipt_id: String,
    pub policy_decision_id: String,
    pub source_artifact_receipt_id: String,
    pub source_artifact_id: String,
    pub parse_receipt_id: String,
    pub grounding_receipt_id: String,
    pub source_artifact_hash: String,
    pub artifact_citation_label: &'static str,
    pub company_context_citation_label: &'static str,
    pub company_context_source: String,
    pub body_ref: String,
    pub production_publish_allowed: bool,
    pub provider_call_allowed: bool,
    pub frontier_raw_content_allowed: bool,
    pub memory_write_performed: bool,
    pub production_write_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TwinArtifactContextError {
    Missing(&'static str),
    UnsupportedMime(String),
    ActorAdmission(String),
    UnknownArtifact(String),
    DeletedArtifact(String),
    PagesDraft(String),
    Storage(String),
    Parser(String),
}

impl TwinArtifactContextError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("twin artifact context missing {field}"),
            Self::UnsupportedMime(mime) => {
                format!("twin artifact context unsupported mime type {mime}")
            }
            Self::ActorAdmission(message) => message.clone(),
            Self::UnknownArtifact(artifact_id) => {
                format!("twin artifact context unknown artifact {artifact_id}")
            }
            Self::DeletedArtifact(artifact_id) => {
                format!("twin artifact context artifact {artifact_id} is deleted")
            }
            Self::PagesDraft(message) => message.clone(),
            Self::Storage(message) => message.clone(),
            Self::Parser(message) => message.clone(),
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn save_twin_artifact_context_local(
        &mut self,
        request: TwinArtifactContextRequest<'_>,
    ) -> Result<TwinArtifactContextReport, TwinArtifactContextError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("session_id", request.session_id),
            ("artifact_name", request.artifact_name),
            ("mime_type", request.mime_type),
            ("artifact_text", request.artifact_text),
        ] {
            if value.trim().is_empty() {
                return Err(TwinArtifactContextError::Missing(field));
            }
        }

        let normalized_type = normalize_twin_artifact_type(request.mime_type)
            .ok_or_else(|| TwinArtifactContextError::UnsupportedMime(request.mime_type.into()))?;
        let privacy_class = if request.privacy_class.trim().is_empty() {
            TWIN_ARTIFACT_DEFAULT_PRIVACY_CLASS
        } else {
            request.privacy_class
        };
        let trusted_context_sources =
            trusted_context_sources_from_receipts(self.ledger().entries());
        let trusted_context_source_ids = trusted_context_sources
            .iter()
            .map(|source| source.source_id.as_str())
            .collect::<Vec<_>>()
            .join(",");
        let trusted_context_receipt_ids = trusted_context_sources
            .iter()
            .map(|source| source.trust_decision_receipt_id.as_str())
            .collect::<Vec<_>>()
            .join(",");
        let trusted_context_source_count = trusted_context_sources.len();
        let trusted_context_used = trusted_context_source_count > 0;
        let trusted_context_summary =
            render_trusted_context_grounding_summary(&trusted_context_sources);
        let company_context_source_owned = if trusted_context_used {
            if request.company_context_source.trim().is_empty() {
                "Pages:trusted Context sources".to_string()
            } else {
                format!(
                    "{} + Pages:trusted Context sources",
                    request.company_context_source.trim()
                )
            }
        } else if request.company_context_source.trim().is_empty() {
            "Pages:local company context".to_string()
        } else {
            request.company_context_source.to_string()
        };
        let company_context_owned = if trusted_context_used {
            if request.company_context.trim().is_empty() {
                trusted_context_summary
            } else {
                format!(
                    "{}\n\nTrusted Context sources:\n{}",
                    request.company_context.trim(),
                    trusted_context_summary
                )
            }
        } else if request.company_context.trim().is_empty() {
            "No extra company context was supplied; Twin used the local Pages and decisions lane."
                .to_string()
        } else {
            request.company_context.to_string()
        };
        let company_context_source = company_context_source_owned.as_str();
        let company_context = company_context_owned.as_str();

        let actor_admission = admit_local_route_actor(
            request.tenant_id,
            request.actor_id,
            "operator",
            "/twin/artifact-contexts.json",
            "twin.artifact.admitted",
            request.session_id,
        )
        .map_err(|error| TwinArtifactContextError::ActorAdmission(error.message()))?;

        let correlation = CorrelationIds {
            tenant_id: TenantId::new(request.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(request.actor_id),
            loop_id: LoopId::new("twin_artifact_context"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("run");
        self.storage.push_loop_run(LoopRun {
            run_id: run_id.clone(),
            loop_id: correlation.loop_id.clone(),
            agent_id: correlation.actor_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            status: "RUNNING".to_string(),
        });

        let artifact_hash = local_artifact_hash(request.artifact_text);
        let byte_size = request.artifact_text.len();
        let extracted_text_char_count = request.artifact_text.chars().count().min(16_000);
        let artifact_id = format!("twin_artifact_{}", self.ids.next("artifact"));
        let storage_object_id = format!("twin_artifact_object_{}", self.ids.next("object"));
        let storage_path = format!(
            "tenant/{}/twin/artifacts/{}/source",
            request.tenant_id, artifact_id
        );
        let context_packet_id = format!("twin_context_packet_{}", self.ids.next("ctx"));
        let grounding_packet_id = format!("twin_grounding_packet_{}", self.ids.next("grounding"));
        let provider_route = twin_artifact_provider_route(self.ledger().entries(), privacy_class);
        let interpretation = if provider_route.provider_call_allowed {
            private_model_artifact_interpretation(
                request.artifact_name,
                normalized_type,
                request.artifact_text,
                company_context_source,
                company_context,
                &provider_route,
            )
        } else {
            local_artifact_interpretation(
                request.artifact_name,
                normalized_type,
                extracted_text_char_count,
                company_context_source,
            )
        };

        let admission_decision =
            self.decide_with_receipt(&correlation, ActionKind::AdmitTwinArtifact);
        let admission_receipt = self.transition_receipt(
            &run_id,
            "ADMIT_ARTIFACT",
            &correlation,
            &admission_decision,
            "twin.artifact.admitted",
            payload(&[
                ("artifact_id", &artifact_id),
                ("source_id", &format!("source_{artifact_id}")),
                ("source_kind", "uploaded_file"),
                ("source_intake_surface", "twin"),
                ("source_trust_state", "read_here"),
                ("trusted_context_state", "read_here"),
                (
                    "allowed_consumers",
                    "twin_current_session,pages_draft_review",
                ),
                ("session_id", request.session_id),
                ("artifact_name", request.artifact_name),
                ("mime_type", request.mime_type),
                ("normalized_type", normalized_type),
                ("privacy_class", privacy_class),
                ("source_artifact_hash", &artifact_hash),
                ("byte_size", &byte_size.to_string()),
                (
                    "file_size_limit_bytes",
                    &TWIN_ARTIFACT_FILE_SIZE_LIMIT_BYTES.to_string(),
                ),
                ("raw_text_recorded", "false"),
                ("storage_object_id", &storage_object_id),
                ("storage_path", &storage_path),
                ("storage_path_posture", "declared_not_live"),
                ("storage_contract_state", "LOCAL_METADATA_ONLY"),
                ("retention_policy", "conversation_scoped_until_delete"),
                ("deletion_state", "ACTIVE"),
                ("download_authorization", "artifact_owner_session_required"),
                ("download_allowed", "false"),
                ("blob_bytes_stored", "false"),
                ("malware_scan_state", "REQUIRED_NOT_RUN_LOCAL_TEXT_PROOF"),
                ("quarantine_state", "METADATA_ONLY_NO_BINARY_BYTES"),
                ("storage_bucket_required", "true"),
                ("parser_sandbox_required", "true"),
                ("memory_eligible", "false"),
                ("provider_call_allowed", "false"),
                ("frontier_raw_content_allowed", "false"),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
            ]),
        );

        let storage_decision =
            self.decide_with_receipt(&correlation, ActionKind::DeclareTwinArtifactStorage);
        let storage_receipt = self.transition_receipt(
            &run_id,
            "DECLARE_STORAGE_METADATA",
            &correlation,
            &storage_decision,
            "twin.artifact.storage.declared",
            payload(&[
                ("artifact_id", &artifact_id),
                ("source_id", &format!("source_{artifact_id}")),
                ("storage_object_id", &storage_object_id),
                ("source_artifact_receipt_id", &admission_receipt.receipt_id),
                ("storage_policy", TWIN_ARTIFACT_STORAGE_POLICY),
                ("storage_path", &storage_path),
                ("storage_path_posture", "declared_not_live"),
                ("storage_contract_state", "LOCAL_METADATA_ONLY"),
                ("retention_policy", "conversation_scoped_until_delete"),
                ("deletion_state", "ACTIVE"),
                ("download_authorization", "artifact_owner_session_required"),
                ("download_allowed", "false"),
                ("blob_bytes_stored", "false"),
                ("malware_scan_state", "REQUIRED_NOT_RUN_LOCAL_TEXT_PROOF"),
                ("quarantine_state", "METADATA_ONLY_NO_BINARY_BYTES"),
                ("storage_bucket_required", "true"),
                ("production_write_allowed", "false"),
            ]),
        );

        let parse_decision =
            self.decide_with_receipt(&correlation, ActionKind::ParseTwinArtifactContext);
        let parse_receipt = self.transition_receipt(
            &run_id,
            "PARSE_CONTEXT_PACKET",
            &correlation,
            &parse_decision,
            "twin.artifact.context_packet.parsed",
            payload(&[
                ("artifact_id", &artifact_id),
                ("context_packet_id", &context_packet_id),
                ("source_artifact_receipt_id", &admission_receipt.receipt_id),
                ("parse_state", "PARSED_LOCAL_TEXT"),
                ("parser_profile", "local_text_bearing_parser_v1"),
                ("parser_sandbox_required", "true"),
                (
                    "extracted_text_char_count",
                    &extracted_text_char_count.to_string(),
                ),
                ("citation_count", "2"),
                ("raw_text_recorded", "false"),
                ("provider_call_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );

        let grounding_decision =
            self.decide_with_receipt(&correlation, ActionKind::GroundTwinArtifactContext);
        let grounding_receipt = self.transition_receipt(
            &run_id,
            "GROUND_COMPANY_CONTEXT",
            &correlation,
            &grounding_decision,
            "twin.artifact.company_context.grounded",
            payload(&[
                ("artifact_id", &artifact_id),
                ("context_packet_id", &context_packet_id),
                ("grounding_packet_id", &grounding_packet_id),
                ("source_parse_receipt_id", &parse_receipt.receipt_id),
                ("grounding_policy", TWIN_ARTIFACT_GROUNDING_POLICY),
                ("company_context_source", company_context_source),
                (
                    "company_context_hash",
                    &local_artifact_hash(company_context),
                ),
                ("artifact_citation_label", "uploaded artifact"),
                ("company_context_citation_label", "company context"),
                ("trusted_context_used", bool_string(trusted_context_used)),
                (
                    "trusted_context_source_count",
                    &trusted_context_source_count.to_string(),
                ),
                ("trusted_context_source_ids", &trusted_context_source_ids),
                ("trusted_context_receipt_ids", &trusted_context_receipt_ids),
                (
                    "trusted_context_projection_route",
                    "/pages/context-sources/projection.json",
                ),
                ("privacy_inheritance", "max(upload,company_context)"),
                ("raw_text_recorded", "false"),
                ("memory_write_performed", "false"),
                ("provider_class", provider_route.provider_class),
                ("provider_id", &provider_route.provider_id),
                ("model_id", &provider_route.model_id),
                (
                    "model_gateway_routing",
                    &provider_route.model_gateway_routing,
                ),
                (
                    "provider_observation_receipt_id",
                    &provider_route.provider_observation_receipt_id,
                ),
                (
                    "sovereign_provider_required",
                    bool_string(provider_route.sovereign_provider_required),
                ),
                (
                    "sovereign_provider_connected",
                    bool_string(provider_route.sovereign_provider_connected),
                ),
                (
                    "provider_not_connected_reason",
                    &provider_route.provider_not_connected_reason,
                ),
                (
                    "frontier_refusal_reason",
                    provider_route.frontier_refusal_reason,
                ),
                (
                    "provider_call_allowed",
                    bool_string(provider_route.provider_call_allowed),
                ),
                ("frontier_raw_content_allowed", "false"),
                ("raw_private_content_routed_to_frontier", "false"),
                ("provider_output_text_recorded", "false"),
                ("production_write_allowed", "false"),
            ]),
        );

        Ok(TwinArtifactContextReport {
            status: "TWIN_ARTIFACT_CONTEXT_READY_LOCAL",
            artifact_id,
            storage_object_id,
            context_packet_id,
            grounding_packet_id,
            session_id: request.session_id.to_string(),
            artifact_name: request.artifact_name.to_string(),
            normalized_type: normalized_type.to_string(),
            mime_type: request.mime_type.to_string(),
            privacy_class: privacy_class.to_string(),
            parse_state: "PARSED_LOCAL_TEXT",
            next_move: "review_interpretation_or_create_pages_draft",
            artifact_admission_receipt_id: admission_receipt.receipt_id,
            storage_receipt_id: storage_receipt.receipt_id,
            parse_receipt_id: parse_receipt.receipt_id,
            grounding_receipt_id: grounding_receipt.receipt_id,
            policy_decision_id: grounding_decision.policy_decision_id,
            source_artifact_hash: artifact_hash,
            storage_path,
            storage_path_posture: "declared_not_live",
            storage_contract_state: "LOCAL_METADATA_ONLY",
            retention_policy: "conversation_scoped_until_delete",
            deletion_state: "ACTIVE",
            download_authorization: "artifact_owner_session_required",
            download_allowed: false,
            blob_bytes_stored: false,
            malware_scan_state: "REQUIRED_NOT_RUN_LOCAL_TEXT_PROOF",
            quarantine_state: "METADATA_ONLY_NO_BINARY_BYTES",
            file_size_limit_bytes: TWIN_ARTIFACT_FILE_SIZE_LIMIT_BYTES,
            byte_size,
            extracted_text_char_count,
            citation_count: 2,
            artifact_citation_label: "uploaded artifact",
            company_context_citation_label: "company context",
            company_context_source: company_context_source.to_string(),
            trusted_context_used,
            trusted_context_source_count,
            trusted_context_source_ids,
            trusted_context_receipt_ids,
            trusted_context_projection_route: "/pages/context-sources/projection.json",
            interpretation,
            proposed_next_action: "draft_pages_summary",
            memory_eligible: false,
            memory_write_performed: false,
            provider_class: provider_route.provider_class,
            provider_id: provider_route.provider_id,
            model_id: provider_route.model_id,
            model_gateway_routing: provider_route.model_gateway_routing,
            provider_observation_receipt_id: provider_route.provider_observation_receipt_id,
            sovereign_provider_required: provider_route.sovereign_provider_required,
            sovereign_provider_connected: provider_route.sovereign_provider_connected,
            provider_not_connected_reason: provider_route.provider_not_connected_reason,
            frontier_refusal_reason: provider_route.frontier_refusal_reason,
            provider_call_allowed: provider_route.provider_call_allowed,
            frontier_raw_content_allowed: false,
            storage_bucket_required: true,
            parser_sandbox_required: true,
            production_write_allowed: false,
        })
    }

    pub fn record_twin_artifact_blob_stored_local(
        &mut self,
        request: TwinArtifactBlobStoredRequest<'_>,
    ) -> Result<TwinArtifactBlobStoredReport, TwinArtifactContextError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("artifact_id", request.artifact_id),
            ("source_body_ref", request.source_body_ref),
            ("extracted_text_ref", request.extracted_text_ref),
            ("source_body_hash", request.source_body_hash),
            ("extracted_text_hash", request.extracted_text_hash),
            ("storage_path_posture", request.storage_path_posture),
            ("storage_contract_state", request.storage_contract_state),
            (
                "encrypted_at_rest_posture",
                request.encrypted_at_rest_posture,
            ),
            ("malware_scan_state", request.malware_scan_state),
            ("quarantine_state", request.quarantine_state),
        ] {
            if value.trim().is_empty() {
                return Err(TwinArtifactContextError::Missing(field));
            }
        }
        let valid_storage_observation = matches!(
            (request.storage_path_posture, request.storage_contract_state),
            ("live_local", "LOCAL_BLOB_STORED_BY_REFERENCE")
                | ("live_provider_observed", "S3_OBJECT_STORED_BY_REFERENCE")
        );
        if !valid_storage_observation {
            return Err(TwinArtifactContextError::Storage(
                "storage observation posture and contract state do not match".to_string(),
            ));
        }

        let admission = self
            .ledger()
            .entries()
            .iter()
            .find(|receipt| {
                receipt.kind == "twin.artifact.admitted"
                    && receipt_payload_value(receipt, "artifact_id") == request.artifact_id
            })
            .map(|receipt| {
                (
                    receipt.receipt_id.clone(),
                    receipt_payload_value(receipt, "source_id"),
                    receipt_payload_value(receipt, "storage_object_id"),
                    receipt_payload_value(receipt, "storage_path"),
                    receipt_payload_value(receipt, "retention_policy"),
                )
            })
            .ok_or_else(|| TwinArtifactContextError::UnknownArtifact(request.artifact_id.into()))?;
        let (
            source_artifact_receipt_id,
            source_id,
            storage_object_id,
            storage_path,
            retention_policy,
        ) = admission;
        let declared_storage_receipt_id = latest_receipt_for_artifact(
            self.ledger().entries(),
            request.artifact_id,
            "twin.artifact.storage.declared",
        );
        if declared_storage_receipt_id.is_empty() {
            return Err(TwinArtifactContextError::Storage(
                "missing declared storage receipt".to_string(),
            ));
        }

        let actor_admission = admit_local_route_actor(
            request.tenant_id,
            request.actor_id,
            "operator",
            "/twin/artifact-contexts.json",
            "twin.artifact.storage.bytes_stored",
            request.artifact_id,
        )
        .map_err(|error| TwinArtifactContextError::ActorAdmission(error.message()))?;

        let correlation = CorrelationIds {
            tenant_id: TenantId::new(request.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(request.actor_id),
            loop_id: LoopId::new("twin_artifact_storage"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("run");
        self.storage.push_loop_run(LoopRun {
            run_id: run_id.clone(),
            loop_id: correlation.loop_id.clone(),
            agent_id: correlation.actor_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            status: "RUNNING".to_string(),
        });

        let decision =
            self.decide_with_receipt(&correlation, ActionKind::RecordTwinArtifactBlobStored);
        let byte_size = request.byte_size.to_string();
        let extracted_text_char_count = request.extracted_text_char_count.to_string();
        let receipt = self.transition_receipt(
            &run_id,
            "STORE_ARTIFACT_BYTES",
            &correlation,
            &decision,
            "twin.artifact.storage.bytes_stored",
            payload(&[
                ("artifact_id", request.artifact_id),
                ("source_id", &source_id),
                ("storage_object_id", &storage_object_id),
                ("source_artifact_receipt_id", &source_artifact_receipt_id),
                ("declared_storage_receipt_id", &declared_storage_receipt_id),
                ("storage_policy", TWIN_ARTIFACT_STORAGE_POLICY),
                ("storage_path", &storage_path),
                ("source_body_ref", request.source_body_ref),
                ("extracted_text_ref", request.extracted_text_ref),
                ("source_body_hash", request.source_body_hash),
                ("extracted_text_hash", request.extracted_text_hash),
                ("byte_size", &byte_size),
                ("extracted_text_char_count", &extracted_text_char_count),
                ("storage_path_posture", request.storage_path_posture),
                ("storage_contract_state", request.storage_contract_state),
                ("retention_policy", &retention_policy),
                ("deletion_state", "ACTIVE"),
                ("download_authorization", "artifact_owner_session_required"),
                ("download_allowed", "true"),
                ("blob_bytes_stored", "true"),
                (
                    "encrypted_at_rest_posture",
                    request.encrypted_at_rest_posture,
                ),
                ("malware_scan_state", request.malware_scan_state),
                ("quarantine_state", request.quarantine_state),
                ("raw_text_recorded", "false"),
                ("raw_blob_recorded", "false"),
                ("provider_call_allowed", "false"),
                ("frontier_raw_content_allowed", "false"),
                ("production_write_allowed", "false"),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
            ]),
        );

        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status = if request.storage_contract_state == "S3_OBJECT_STORED_BY_REFERENCE" {
                "TWIN_ARTIFACT_STORAGE_BYTES_STORED_S3"
            } else {
                "TWIN_ARTIFACT_STORAGE_BYTES_STORED_LOCAL"
            }
            .to_string();
        }

        Ok(TwinArtifactBlobStoredReport {
            storage_receipt_id: receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            storage_path,
            storage_path_posture: request.storage_path_posture,
            storage_contract_state: request.storage_contract_state,
            download_authorization: "artifact_owner_session_required",
            download_allowed: true,
            blob_bytes_stored: true,
            malware_scan_state: request.malware_scan_state,
            quarantine_state: request.quarantine_state,
        })
    }

    pub fn authorize_twin_artifact_download_local(
        &mut self,
        request: TwinArtifactDownloadRequest<'_>,
    ) -> Result<TwinArtifactDownloadReport, TwinArtifactContextError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("artifact_id", request.artifact_id),
        ] {
            if value.trim().is_empty() {
                return Err(TwinArtifactContextError::Missing(field));
            }
        }

        if self.ledger().entries().iter().any(|receipt| {
            receipt.kind == "twin.artifact.delete_requested"
                && receipt_payload_value(receipt, "artifact_id") == request.artifact_id
        }) {
            return Err(TwinArtifactContextError::DeletedArtifact(
                request.artifact_id.to_string(),
            ));
        }

        let admission = self
            .ledger()
            .entries()
            .iter()
            .find(|receipt| {
                receipt.kind == "twin.artifact.admitted"
                    && receipt_payload_value(receipt, "artifact_id") == request.artifact_id
            })
            .map(|receipt| {
                (
                    receipt.receipt_id.clone(),
                    receipt_payload_value(receipt, "source_id"),
                    receipt_payload_value(receipt, "storage_object_id"),
                    receipt_payload_value(receipt, "artifact_name"),
                    receipt_payload_value(receipt, "mime_type"),
                )
            })
            .ok_or_else(|| TwinArtifactContextError::UnknownArtifact(request.artifact_id.into()))?;
        let (source_artifact_receipt_id, source_id, storage_object_id, artifact_name, mime_type) =
            admission;

        let storage = self
            .ledger()
            .entries()
            .iter()
            .rev()
            .find(|receipt| {
                receipt.kind == "twin.artifact.storage.bytes_stored"
                    && receipt_payload_value(receipt, "artifact_id") == request.artifact_id
            })
            .map(|receipt| {
                (
                    receipt.receipt_id.clone(),
                    receipt_payload_value(receipt, "source_body_ref"),
                    receipt_payload_value(receipt, "source_body_hash"),
                    receipt_payload_value(receipt, "byte_size")
                        .parse::<usize>()
                        .unwrap_or(0),
                )
            })
            .ok_or_else(|| {
                TwinArtifactContextError::Storage(
                    "stored artifact bytes are required before download".to_string(),
                )
            })?;
        let (storage_receipt_id, source_body_ref, source_body_hash, byte_size) = storage;

        let actor_admission = admit_local_route_actor(
            request.tenant_id,
            request.actor_id,
            "operator",
            "/twin/artifacts/downloads.json",
            "twin.artifact.download.authorized",
            request.artifact_id,
        )
        .map_err(|error| TwinArtifactContextError::ActorAdmission(error.message()))?;

        let correlation = CorrelationIds {
            tenant_id: TenantId::new(request.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(request.actor_id),
            loop_id: LoopId::new("twin_artifact_download"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("run");
        self.storage.push_loop_run(LoopRun {
            run_id: run_id.clone(),
            loop_id: correlation.loop_id.clone(),
            agent_id: correlation.actor_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            status: "RUNNING".to_string(),
        });

        let decision =
            self.decide_with_receipt(&correlation, ActionKind::AuthorizeTwinArtifactDownload);
        let byte_size_string = byte_size.to_string();
        let receipt = self.transition_receipt(
            &run_id,
            "AUTHORIZE_ARTIFACT_DOWNLOAD",
            &correlation,
            &decision,
            "twin.artifact.download.authorized",
            payload(&[
                ("artifact_id", request.artifact_id),
                ("source_id", &source_id),
                ("storage_object_id", &storage_object_id),
                ("source_artifact_receipt_id", &source_artifact_receipt_id),
                ("storage_receipt_id", &storage_receipt_id),
                ("source_body_ref", &source_body_ref),
                ("source_body_hash", &source_body_hash),
                ("byte_size", &byte_size_string),
                ("download_authorization", "artifact_owner_session_required"),
                ("download_allowed", "true"),
                ("owner_authorized", "true"),
                ("raw_private_content_recorded", "false"),
                ("download_body_recorded", "false"),
                ("provider_call_allowed", "false"),
                ("frontier_raw_content_allowed", "false"),
                ("memory_write_performed", "false"),
                ("production_write_allowed", "false"),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
            ]),
        );

        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status = "TWIN_ARTIFACT_DOWNLOAD_AUTHORIZED_LOCAL".to_string();
        }

        Ok(TwinArtifactDownloadReport {
            status: "TWIN_ARTIFACT_DOWNLOAD_READY_LOCAL",
            artifact_id: request.artifact_id.to_string(),
            artifact_name,
            mime_type: mime_type.clone(),
            storage_object_id,
            source_body_ref,
            source_body_hash,
            byte_size,
            download_receipt_id: receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            source_artifact_receipt_id,
            storage_receipt_id,
            download_authorization: "artifact_owner_session_required",
            download_allowed: true,
            owner_authorized: true,
            content_type: mime_type,
            raw_private_content_recorded: false,
            provider_call_allowed: false,
            frontier_raw_content_allowed: false,
            memory_write_performed: false,
            production_write_allowed: false,
        })
    }

    pub fn record_twin_artifact_liteparse_pdf_local(
        &mut self,
        request: TwinArtifactLiteParseRequest<'_>,
    ) -> Result<TwinArtifactLiteParseReport, TwinArtifactContextError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("artifact_id", request.artifact_id),
            ("artifact_name", request.artifact_name),
            ("mime_type", request.mime_type),
            ("source_body_ref", request.source_body_ref),
            ("extracted_text_ref", request.extracted_text_ref),
            ("source_body_hash", request.source_body_hash),
            ("extracted_text_hash", request.extracted_text_hash),
        ] {
            if value.trim().is_empty() {
                return Err(TwinArtifactContextError::Missing(field));
            }
        }
        if request.mime_type != "application/pdf" {
            return Err(TwinArtifactContextError::UnsupportedMime(
                request.mime_type.into(),
            ));
        }
        if request.byte_size == 0 || request.byte_size > TWIN_ARTIFACT_FILE_SIZE_LIMIT_BYTES {
            return Err(TwinArtifactContextError::Parser(
                "liteparse pdf source byte size is outside the local limit".to_string(),
            ));
        }
        if request.extracted_text_char_count == 0 {
            return Err(TwinArtifactContextError::Parser(
                "liteparse pdf extracted no readable text".to_string(),
            ));
        }

        let actor_admission = admit_local_route_actor(
            request.tenant_id,
            request.actor_id,
            "operator",
            "/twin/artifacts/liteparse-parses.json",
            "twin.artifact.parser.liteparse_pdf.parsed",
            request.artifact_id,
        )
        .map_err(|error| TwinArtifactContextError::ActorAdmission(error.message()))?;

        let correlation = CorrelationIds {
            tenant_id: TenantId::new(request.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(request.actor_id),
            loop_id: LoopId::new("twin_artifact_liteparse_pdf"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("run");
        self.storage.push_loop_run(LoopRun {
            run_id: run_id.clone(),
            loop_id: correlation.loop_id.clone(),
            agent_id: correlation.actor_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            status: "RUNNING".to_string(),
        });

        let decision =
            self.decide_with_receipt(&correlation, ActionKind::ParseTwinArtifactWithLiteParse);
        let byte_size = request.byte_size.to_string();
        let page_count = request.page_count.to_string();
        let extracted_text_char_count = request.extracted_text_char_count.to_string();
        let parser_elapsed_ms = request.parser_elapsed_ms.to_string();
        let receipt = self.transition_receipt(
            &run_id,
            "PARSE_PDF_WITH_LITEPARSE",
            &correlation,
            &decision,
            "twin.artifact.parser.liteparse_pdf.parsed",
            payload(&[
                ("artifact_id", request.artifact_id),
                ("artifact_name", request.artifact_name),
                ("mime_type", request.mime_type),
                ("source_id", &format!("source_{}", request.artifact_id)),
                ("source_intake_surface", "twin"),
                ("source_kind", "uploaded_file"),
                ("source_trust_state", "read_here"),
                ("trusted_context_state", "read_here"),
                (
                    "allowed_consumers",
                    "twin_current_session,pages_draft_review",
                ),
                ("parser_policy", TWIN_ARTIFACT_LITEPARSE_POLICY),
                ("parser_id", "liteparse"),
                ("parser_version", "2.1.1"),
                ("parser_profile", "liteparse_pdf_markdown_no_ocr_v1"),
                ("parse_state", "PARSED_LOCAL_PDF_LITEPARSE"),
                ("output_format", "markdown"),
                ("ocr_enabled", "false"),
                ("page_count", &page_count),
                ("source_body_ref", request.source_body_ref),
                ("extracted_text_ref", request.extracted_text_ref),
                ("source_body_hash", request.source_body_hash),
                ("extracted_text_hash", request.extracted_text_hash),
                ("byte_size", &byte_size),
                ("extracted_text_char_count", &extracted_text_char_count),
                ("parser_elapsed_ms", &parser_elapsed_ms),
                ("storage_path_posture", request.storage_path_posture),
                ("storage_contract_state", request.storage_contract_state),
                ("raw_private_content_recorded", "false"),
                ("raw_text_recorded", "false"),
                ("raw_blob_recorded", "false"),
                ("provider_call_allowed", "false"),
                ("frontier_raw_content_allowed", "false"),
                ("memory_write_performed", "false"),
                ("production_write_allowed", "false"),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
            ]),
        );

        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status = "TWIN_ARTIFACT_LITEPARSE_PDF_PARSED_LOCAL".to_string();
        }

        Ok(TwinArtifactLiteParseReport {
            status: "TWIN_ARTIFACT_LITEPARSE_PDF_PARSED_LOCAL",
            artifact_id: request.artifact_id.to_string(),
            artifact_name: request.artifact_name.to_string(),
            mime_type: request.mime_type.to_string(),
            parser_id: "liteparse",
            parser_version: "2.1.1",
            parser_profile: "liteparse_pdf_markdown_no_ocr_v1",
            parse_state: "PARSED_LOCAL_PDF_LITEPARSE",
            output_format: "markdown",
            ocr_enabled: false,
            source_body_ref: request.source_body_ref.to_string(),
            extracted_text_ref: request.extracted_text_ref.to_string(),
            source_body_hash: request.source_body_hash.to_string(),
            extracted_text_hash: request.extracted_text_hash.to_string(),
            byte_size: request.byte_size,
            page_count: request.page_count,
            extracted_text_char_count: request.extracted_text_char_count,
            parser_elapsed_ms: request.parser_elapsed_ms,
            liteparse_receipt_id: receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            source_intake_surface: "twin",
            source_trust_state: "read_here",
            trusted_context_state: "read_here",
            raw_private_content_recorded: false,
            provider_call_allowed: false,
            frontier_raw_content_allowed: false,
            memory_write_performed: false,
            production_write_allowed: false,
        })
    }

    pub fn save_twin_artifact_pages_draft_local(
        &mut self,
        request: TwinArtifactPagesDraftRequest<'_>,
    ) -> Result<TwinArtifactPagesDraftReport, TwinArtifactContextError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("artifact_id", request.artifact_id),
            ("pages_draft_id", request.pages_draft_id),
            ("pages_document_id", request.pages_document_id),
            ("pages_title", request.pages_title),
            ("pages_body_ref", request.pages_body_ref),
            ("pages_revision_id", request.pages_revision_id),
            ("approval_request_id", request.approval_request_id),
            ("requested_visibility", request.requested_visibility),
        ] {
            if value.trim().is_empty() {
                return Err(TwinArtifactContextError::Missing(field));
            }
        }

        if self.ledger().entries().iter().any(|receipt| {
            receipt.kind == "twin.artifact.delete_requested"
                && receipt_payload_value(receipt, "artifact_id") == request.artifact_id
        }) {
            return Err(TwinArtifactContextError::DeletedArtifact(
                request.artifact_id.to_string(),
            ));
        }

        let admission = self
            .ledger()
            .entries()
            .iter()
            .find(|receipt| {
                receipt.kind == "twin.artifact.admitted"
                    && receipt_payload_value(receipt, "artifact_id") == request.artifact_id
            })
            .map(|receipt| {
                (
                    receipt.receipt_id.clone(),
                    receipt_payload_value(receipt, "source_artifact_hash"),
                    receipt_payload_value(receipt, "artifact_name"),
                )
            })
            .ok_or_else(|| TwinArtifactContextError::UnknownArtifact(request.artifact_id.into()))?;
        let (source_artifact_receipt_id, source_artifact_hash, artifact_name) = admission;

        let parse_receipt_id = latest_receipt_for_artifact(
            self.ledger().entries(),
            request.artifact_id,
            "twin.artifact.context_packet.parsed",
        );
        if parse_receipt_id.is_empty() {
            return Err(TwinArtifactContextError::Missing("parse_receipt_id"));
        }
        let grounding_receipt_id = latest_receipt_for_artifact(
            self.ledger().entries(),
            request.artifact_id,
            "twin.artifact.company_context.grounded",
        );
        if grounding_receipt_id.is_empty() {
            return Err(TwinArtifactContextError::Missing("grounding_receipt_id"));
        }
        let company_context_source = latest_payload_for_artifact(
            self.ledger().entries(),
            request.artifact_id,
            "twin.artifact.company_context.grounded",
            "company_context_source",
        );

        let actor_admission = admit_local_route_actor(
            request.tenant_id,
            request.actor_id,
            "operator",
            "/twin/artifacts/pages-drafts.json",
            "twin.artifact.pages_draft.created",
            request.artifact_id,
        )
        .map_err(|error| TwinArtifactContextError::ActorAdmission(error.message()))?;

        let pages_draft = self
            .save_pages_edit_draft_local(PagesEditDraft {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                draft_id: request.pages_draft_id,
                document_id: request.pages_document_id,
                title: request.pages_title,
                body_ref: request.pages_body_ref,
                source_publication_receipt_id: "",
                origin_receipt_id: "",
                origin_surface: "",
                revision_id: request.pages_revision_id,
            })
            .map_err(|error| TwinArtifactContextError::PagesDraft(error.message()))?;
        let approval_request = self
            .save_pages_approval_request_local(PagesApprovalRequest {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                approval_request_id: request.approval_request_id,
                source_edit_draft_receipt_id: &pages_draft.edit_draft_receipt_id,
                document_id: request.pages_document_id,
                draft_id: request.pages_draft_id,
                requested_visibility: request.requested_visibility,
            })
            .map_err(|error| TwinArtifactContextError::PagesDraft(error.message()))?;

        let correlation = CorrelationIds {
            tenant_id: TenantId::new(request.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(request.actor_id),
            loop_id: LoopId::new("twin_artifact_pages_draft"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("run");
        self.storage.push_loop_run(LoopRun {
            run_id: run_id.clone(),
            loop_id: correlation.loop_id.clone(),
            agent_id: correlation.actor_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            status: "RUNNING".to_string(),
        });

        let decision =
            self.decide_with_receipt(&correlation, ActionKind::CreateTwinArtifactPagesDraft);
        let pages_receipt = self.transition_receipt(
            &run_id,
            "CREATE_ARTIFACT_PAGES_DRAFT",
            &correlation,
            &decision,
            "twin.artifact.pages_draft.created",
            payload(&[
                ("artifact_id", request.artifact_id),
                ("source_artifact_id", request.artifact_id),
                ("source_artifact_receipt_id", &source_artifact_receipt_id),
                ("source_artifact_hash", &source_artifact_hash),
                ("artifact_name", &artifact_name),
                ("parse_receipt_id", &parse_receipt_id),
                ("grounding_receipt_id", &grounding_receipt_id),
                ("artifact_citation_label", "uploaded artifact"),
                ("company_context_citation_label", "company context"),
                ("company_context_source", &company_context_source),
                ("pages_draft_id", request.pages_draft_id),
                ("pages_document_id", request.pages_document_id),
                ("pages_title", request.pages_title),
                ("pages_route", "/pages"),
                ("pages_draft_receipt_id", &pages_draft.edit_draft_receipt_id),
                ("approval_request_id", &approval_request.approval_request_id),
                (
                    "approval_request_receipt_id",
                    &approval_request.approval_request_receipt_id,
                ),
                ("draft_state", "pending_approval"),
                ("approval_state", "PENDING_DECISION"),
                ("body_ref", request.pages_body_ref),
                ("raw_private_content_recorded", "false"),
                ("provider_call_allowed", "false"),
                ("frontier_raw_content_allowed", "false"),
                ("memory_write_performed", "false"),
                ("production_publish_allowed", "false"),
                ("production_write_allowed", "false"),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
            ]),
        );
        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status = "TWIN_ARTIFACT_PAGES_DRAFT_CREATED_LOCAL".to_string();
        }

        Ok(TwinArtifactPagesDraftReport {
            status: "TWIN_ARTIFACT_PAGES_DRAFT_CREATED_LOCAL",
            artifact_id: request.artifact_id.to_string(),
            pages_draft_id: request.pages_draft_id.to_string(),
            pages_document_id: request.pages_document_id.to_string(),
            pages_title: request.pages_title.to_string(),
            pages_route: "/pages",
            draft_state: "pending_approval",
            approval_state: "PENDING_DECISION",
            pages_draft_receipt_id: pages_draft.edit_draft_receipt_id,
            approval_request_id: approval_request.approval_request_id,
            approval_request_receipt_id: approval_request.approval_request_receipt_id,
            twin_pages_draft_receipt_id: pages_receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            source_artifact_receipt_id,
            source_artifact_id: request.artifact_id.to_string(),
            parse_receipt_id,
            grounding_receipt_id,
            source_artifact_hash,
            artifact_citation_label: "uploaded artifact",
            company_context_citation_label: "company context",
            company_context_source,
            body_ref: request.pages_body_ref.to_string(),
            production_publish_allowed: false,
            provider_call_allowed: false,
            frontier_raw_content_allowed: false,
            memory_write_performed: false,
            production_write_allowed: false,
        })
    }

    pub fn delete_twin_artifact_local(
        &mut self,
        request: TwinArtifactDeletionRequest<'_>,
    ) -> Result<TwinArtifactDeletionReport, TwinArtifactContextError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("artifact_id", request.artifact_id),
        ] {
            if value.trim().is_empty() {
                return Err(TwinArtifactContextError::Missing(field));
            }
        }

        let artifact = self
            .ledger()
            .entries()
            .iter()
            .find(|receipt| {
                receipt.kind == "twin.artifact.admitted"
                    && receipt_payload_value(receipt, "artifact_id") == request.artifact_id
            })
            .map(|receipt| {
                (
                    receipt.receipt_id.clone(),
                    receipt_payload_value(receipt, "source_id"),
                    receipt_payload_value(receipt, "storage_object_id"),
                    receipt_payload_value(receipt, "session_id"),
                )
            })
            .ok_or_else(|| TwinArtifactContextError::UnknownArtifact(request.artifact_id.into()))?;
        let (source_artifact_receipt_id, source_id, storage_object_id, session_id) = artifact;
        let storage_receipt_id = latest_receipt_for_artifact(
            self.ledger().entries(),
            request.artifact_id,
            "twin.artifact.storage.declared",
        );
        let parse_receipt_id = latest_receipt_for_artifact(
            self.ledger().entries(),
            request.artifact_id,
            "twin.artifact.context_packet.parsed",
        );
        let grounding_receipt_id = latest_receipt_for_artifact(
            self.ledger().entries(),
            request.artifact_id,
            "twin.artifact.company_context.grounded",
        );
        let context_packet_id = latest_payload_for_artifact(
            self.ledger().entries(),
            request.artifact_id,
            "twin.artifact.context_packet.parsed",
            "context_packet_id",
        );
        let grounding_packet_id = latest_payload_for_artifact(
            self.ledger().entries(),
            request.artifact_id,
            "twin.artifact.company_context.grounded",
            "grounding_packet_id",
        );

        let actor_admission = admit_local_route_actor(
            request.tenant_id,
            request.actor_id,
            "operator",
            "/twin/artifacts/deletions.json",
            "twin.artifact.delete_requested",
            request.artifact_id,
        )
        .map_err(|error| TwinArtifactContextError::ActorAdmission(error.message()))?;

        let correlation = CorrelationIds {
            tenant_id: TenantId::new(request.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(request.actor_id),
            loop_id: LoopId::new("twin_artifact_delete"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("run");
        self.storage.push_loop_run(LoopRun {
            run_id: run_id.clone(),
            loop_id: correlation.loop_id.clone(),
            agent_id: correlation.actor_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            status: "RUNNING".to_string(),
        });

        let delete_decision =
            self.decide_with_receipt(&correlation, ActionKind::DeleteTwinArtifact);
        let deletion_state = if request.local_blob_bytes_deleted {
            "DELETED_LOCAL_PROPAGATED"
        } else {
            "DELETED_LOCAL_TOMBSTONE"
        };
        let reason_hash = local_artifact_hash(if request.reason.trim().is_empty() {
            "user_requested_delete"
        } else {
            request.reason
        });
        let delete_receipt = self.transition_receipt(
            &run_id,
            "DELETE_ARTIFACT",
            &correlation,
            &delete_decision,
            "twin.artifact.delete_requested",
            payload(&[
                ("artifact_id", request.artifact_id),
                ("source_id", &source_id),
                ("session_id", &session_id),
                ("source_artifact_receipt_id", &source_artifact_receipt_id),
                ("storage_receipt_id", &storage_receipt_id),
                ("parse_receipt_id", &parse_receipt_id),
                ("grounding_receipt_id", &grounding_receipt_id),
                ("deletion_state", deletion_state),
                ("reason_hash", &reason_hash),
                ("raw_reason_recorded", "false"),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
                ("provider_call_allowed", "false"),
                ("frontier_raw_content_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );

        let storage_tombstone = self.transition_receipt(
            &run_id,
            "TOMBSTONE_STORAGE_OBJECT",
            &correlation,
            &delete_decision,
            "twin.artifact.storage_object.tombstoned",
            payload(&[
                ("artifact_id", request.artifact_id),
                ("source_id", &source_id),
                ("storage_object_id", &storage_object_id),
                ("source_delete_receipt_id", &delete_receipt.receipt_id),
                ("blob_delete_required", "true"),
                (
                    "local_blob_bytes_deleted",
                    bool_string(request.local_blob_bytes_deleted),
                ),
                (
                    "parsed_text_deleted",
                    bool_string(request.local_blob_bytes_deleted),
                ),
                ("download_allowed", "false"),
                (
                    "storage_contract_state",
                    if request.local_blob_bytes_deleted {
                        "LOCAL_BLOB_DELETED"
                    } else {
                        "LOCAL_BLOB_DELETE_NOT_OBSERVED"
                    },
                ),
                ("production_write_allowed", "false"),
            ]),
        );

        let context_invalidated = self.transition_receipt(
            &run_id,
            "INVALIDATE_ARTIFACT_CONTEXT",
            &correlation,
            &delete_decision,
            "twin.artifact.context_packet.invalidated",
            payload(&[
                ("artifact_id", request.artifact_id),
                ("context_packet_id", &context_packet_id),
                ("grounding_packet_id", &grounding_packet_id),
                ("source_delete_receipt_id", &delete_receipt.receipt_id),
                ("context_packets_invalidated", "true"),
                ("memory_atoms_invalidated", "false"),
                ("generated_artifacts_invalidated", "false"),
                ("production_write_allowed", "false"),
            ]),
        );

        Ok(TwinArtifactDeletionReport {
            status: if request.local_blob_bytes_deleted {
                "TWIN_ARTIFACT_DELETED_LOCAL_PROPAGATED"
            } else {
                "TWIN_ARTIFACT_DELETED_LOCAL_TOMBSTONE"
            },
            artifact_id: request.artifact_id.to_string(),
            storage_object_id,
            context_packet_id,
            grounding_packet_id,
            deletion_state,
            artifact_deleted_receipt_id: delete_receipt.receipt_id,
            storage_tombstone_receipt_id: storage_tombstone.receipt_id,
            context_invalidated_receipt_id: context_invalidated.receipt_id,
            policy_decision_id: delete_decision.policy_decision_id,
            source_artifact_receipt_id,
            storage_receipt_id,
            parse_receipt_id,
            grounding_receipt_id,
            blob_delete_required: true,
            local_blob_bytes_deleted: request.local_blob_bytes_deleted,
            context_packets_invalidated: true,
            memory_atoms_invalidated: false,
            generated_artifacts_invalidated: false,
            provider_call_allowed: false,
            frontier_raw_content_allowed: false,
            production_write_allowed: false,
        })
    }
}

impl TwinArtifactContextReport {
    pub fn render_post_json(&self) -> String {
        format!(
            r#"{{"name":"mdx-twin-artifact-context-local-post","status":{},"auth_session_required":true,"auth_session_route":"/local/auth-session.json","route":"/twin/artifact-contexts.json","projection_route":"/twin/artifact-contexts/projection.json","download_route":"/twin/artifacts/downloads.json","delete_route":"/twin/artifacts/deletions.json","artifact_id":{},"storage_object_id":{},"context_packet_id":{},"grounding_packet_id":{},"session_id":{},"artifact_name":{},"normalized_type":{},"mime_type":{},"privacy_class":{},"parse_state":{},"next_move":{},"artifact_admission_receipt_id":{},"storage_receipt_id":{},"parse_receipt_id":{},"grounding_receipt_id":{},"policy_decision_id":{},"source_artifact_hash":{},"storage_path":{},"storage_path_posture":{},"storage_contract_state":{},"retention_policy":{},"deletion_state":{},"download_authorization":{},"download_allowed":{},"blob_bytes_stored":{},"malware_scan_state":{},"quarantine_state":{},"file_size_limit_bytes":{},"byte_size":{},"extracted_text_char_count":{},"citation_count":{},"artifact_citation_label":{},"company_context_citation_label":{},"company_context_source":{},"trusted_context_used":{},"trusted_context_source_count":{},"trusted_context_source_ids":{},"trusted_context_receipt_ids":{},"trusted_context_projection_route":{},"interpretation":{},"proposed_next_action":{},"memory_eligible":{},"memory_write_performed":{},"provider_class":{},"provider_id":{},"model_id":{},"model_gateway_routing":{},"provider_observation_receipt_id":{},"sovereign_provider_required":{},"sovereign_provider_connected":{},"provider_not_connected_reason":{},"frontier_refusal_reason":{},"provider_call_allowed":{},"frontier_raw_content_allowed":{},"storage_bucket_required":{},"parser_sandbox_required":{},"production_write_allowed":{}}}"#,
            json_string_literal(self.status),
            json_string_literal(&self.artifact_id),
            json_string_literal(&self.storage_object_id),
            json_string_literal(&self.context_packet_id),
            json_string_literal(&self.grounding_packet_id),
            json_string_literal(&self.session_id),
            json_string_literal(&self.artifact_name),
            json_string_literal(&self.normalized_type),
            json_string_literal(&self.mime_type),
            json_string_literal(&self.privacy_class),
            json_string_literal(self.parse_state),
            json_string_literal(self.next_move),
            json_string_literal(&self.artifact_admission_receipt_id),
            json_string_literal(&self.storage_receipt_id),
            json_string_literal(&self.parse_receipt_id),
            json_string_literal(&self.grounding_receipt_id),
            json_string_literal(&self.policy_decision_id),
            json_string_literal(&self.source_artifact_hash),
            json_string_literal(&self.storage_path),
            json_string_literal(self.storage_path_posture),
            json_string_literal(self.storage_contract_state),
            json_string_literal(self.retention_policy),
            json_string_literal(self.deletion_state),
            json_string_literal(self.download_authorization),
            self.download_allowed,
            self.blob_bytes_stored,
            json_string_literal(self.malware_scan_state),
            json_string_literal(self.quarantine_state),
            self.file_size_limit_bytes,
            self.byte_size,
            self.extracted_text_char_count,
            self.citation_count,
            json_string_literal(self.artifact_citation_label),
            json_string_literal(self.company_context_citation_label),
            json_string_literal(&self.company_context_source),
            self.trusted_context_used,
            self.trusted_context_source_count,
            json_string_literal(&self.trusted_context_source_ids),
            json_string_literal(&self.trusted_context_receipt_ids),
            json_string_literal(self.trusted_context_projection_route),
            json_string_literal(&self.interpretation),
            json_string_literal(self.proposed_next_action),
            self.memory_eligible,
            self.memory_write_performed,
            json_string_literal(self.provider_class),
            json_string_literal(&self.provider_id),
            json_string_literal(&self.model_id),
            json_string_literal(&self.model_gateway_routing),
            json_string_literal(&self.provider_observation_receipt_id),
            self.sovereign_provider_required,
            self.sovereign_provider_connected,
            json_string_literal(&self.provider_not_connected_reason),
            json_string_literal(self.frontier_refusal_reason),
            self.provider_call_allowed,
            self.frontier_raw_content_allowed,
            self.storage_bucket_required,
            self.parser_sandbox_required,
            self.production_write_allowed
        )
    }
}

impl TwinArtifactDownloadReport {
    pub fn render_download_json(&self, artifact_text: &str) -> String {
        format!(
            r#"{{"name":"mdx-twin-artifact-download-local-post","status":{},"auth_session_required":true,"auth_session_route":"/local/auth-session.json","route":"/twin/artifacts/downloads.json","projection_route":"/twin/artifact-contexts/projection.json","artifact_id":{},"artifact_name":{},"mime_type":{},"storage_object_id":{},"source_body_ref":{},"source_body_hash":{},"byte_size":{},"download_receipt_id":{},"policy_decision_id":{},"source_artifact_receipt_id":{},"storage_receipt_id":{},"download_authorization":{},"download_allowed":{},"owner_authorized":{},"content_type":{},"artifact_text":{},"raw_private_content_recorded":{},"provider_call_allowed":{},"frontier_raw_content_allowed":{},"memory_write_performed":{},"production_write_allowed":{}}}"#,
            json_string_literal(self.status),
            json_string_literal(&self.artifact_id),
            json_string_literal(&self.artifact_name),
            json_string_literal(&self.mime_type),
            json_string_literal(&self.storage_object_id),
            json_string_literal(&self.source_body_ref),
            json_string_literal(&self.source_body_hash),
            self.byte_size,
            json_string_literal(&self.download_receipt_id),
            json_string_literal(&self.policy_decision_id),
            json_string_literal(&self.source_artifact_receipt_id),
            json_string_literal(&self.storage_receipt_id),
            json_string_literal(self.download_authorization),
            self.download_allowed,
            self.owner_authorized,
            json_string_literal(&self.content_type),
            json_string_literal(artifact_text),
            self.raw_private_content_recorded,
            self.provider_call_allowed,
            self.frontier_raw_content_allowed,
            self.memory_write_performed,
            self.production_write_allowed
        )
    }
}

impl TwinArtifactLiteParseReport {
    pub fn render_post_json(&self, extracted_text: &str) -> String {
        let extracted_text_preview = extracted_text.chars().take(1_500).collect::<String>();
        format!(
            r#"{{"name":"mdx-twin-artifact-liteparse-local-post","status":{},"auth_session_required":true,"auth_session_route":"/local/auth-session.json","route":"/twin/artifacts/liteparse-parses.json","runtime_route":"/twin/parser-sandbox-runtime.json","artifact_context_route":"/twin/artifact-contexts.json","artifact_id":{},"artifact_name":{},"mime_type":{},"parser_id":{},"parser_version":{},"parser_profile":{},"parse_state":{},"output_format":{},"ocr_enabled":{},"source_body_ref":{},"extracted_text_ref":{},"source_body_hash":{},"extracted_text_hash":{},"byte_size":{},"page_count":{},"extracted_text_char_count":{},"parser_elapsed_ms":{},"liteparse_receipt_id":{},"policy_decision_id":{},"source_intake_surface":{},"source_trust_state":{},"trusted_context_state":{},"extracted_text":{},"extracted_text_preview":{},"raw_private_content_recorded":{},"provider_call_allowed":{},"frontier_raw_content_allowed":{},"memory_write_performed":{},"production_write_allowed":{}}}"#,
            json_string_literal(self.status),
            json_string_literal(&self.artifact_id),
            json_string_literal(&self.artifact_name),
            json_string_literal(&self.mime_type),
            json_string_literal(self.parser_id),
            json_string_literal(self.parser_version),
            json_string_literal(self.parser_profile),
            json_string_literal(self.parse_state),
            json_string_literal(self.output_format),
            self.ocr_enabled,
            json_string_literal(&self.source_body_ref),
            json_string_literal(&self.extracted_text_ref),
            json_string_literal(&self.source_body_hash),
            json_string_literal(&self.extracted_text_hash),
            self.byte_size,
            self.page_count,
            self.extracted_text_char_count,
            self.parser_elapsed_ms,
            json_string_literal(&self.liteparse_receipt_id),
            json_string_literal(&self.policy_decision_id),
            json_string_literal(self.source_intake_surface),
            json_string_literal(self.source_trust_state),
            json_string_literal(self.trusted_context_state),
            json_string_literal(extracted_text),
            json_string_literal(&extracted_text_preview),
            self.raw_private_content_recorded,
            self.provider_call_allowed,
            self.frontier_raw_content_allowed,
            self.memory_write_performed,
            self.production_write_allowed
        )
    }
}

impl TwinArtifactPagesDraftReport {
    pub fn render_post_json(&self) -> String {
        format!(
            r#"{{"name":"mdx-twin-artifact-pages-draft-local-post","status":{},"auth_session_required":true,"auth_session_route":"/local/auth-session.json","route":"/twin/artifacts/pages-drafts.json","pages_route":{},"pages_projection_route":"/pages/edit-drafts/projection.json","pages_approval_route":"/pages/approval-requests/projection.json","artifact_id":{},"source_artifact_id":{},"pages_draft_id":{},"pages_document_id":{},"pages_title":{},"draft_state":{},"approval_state":{},"pages_draft_receipt_id":{},"approval_request_id":{},"approval_request_receipt_id":{},"twin_pages_draft_receipt_id":{},"policy_decision_id":{},"source_artifact_receipt_id":{},"parse_receipt_id":{},"grounding_receipt_id":{},"source_artifact_hash":{},"artifact_citation_label":{},"company_context_citation_label":{},"company_context_source":{},"body_ref":{},"production_publish_allowed":{},"provider_call_allowed":{},"frontier_raw_content_allowed":{},"memory_write_performed":{},"production_write_allowed":{}}}"#,
            json_string_literal(self.status),
            json_string_literal(self.pages_route),
            json_string_literal(&self.artifact_id),
            json_string_literal(&self.source_artifact_id),
            json_string_literal(&self.pages_draft_id),
            json_string_literal(&self.pages_document_id),
            json_string_literal(&self.pages_title),
            json_string_literal(self.draft_state),
            json_string_literal(self.approval_state),
            json_string_literal(&self.pages_draft_receipt_id),
            json_string_literal(&self.approval_request_id),
            json_string_literal(&self.approval_request_receipt_id),
            json_string_literal(&self.twin_pages_draft_receipt_id),
            json_string_literal(&self.policy_decision_id),
            json_string_literal(&self.source_artifact_receipt_id),
            json_string_literal(&self.parse_receipt_id),
            json_string_literal(&self.grounding_receipt_id),
            json_string_literal(&self.source_artifact_hash),
            json_string_literal(self.artifact_citation_label),
            json_string_literal(self.company_context_citation_label),
            json_string_literal(&self.company_context_source),
            json_string_literal(&self.body_ref),
            self.production_publish_allowed,
            self.provider_call_allowed,
            self.frontier_raw_content_allowed,
            self.memory_write_performed,
            self.production_write_allowed
        )
    }
}

impl TwinArtifactDeletionReport {
    pub fn render_post_json(&self) -> String {
        format!(
            r#"{{"name":"mdx-twin-artifact-deletion-local-post","status":{},"auth_session_required":true,"auth_session_route":"/local/auth-session.json","route":"/twin/artifacts/deletions.json","projection_route":"/twin/artifact-contexts/projection.json","artifact_id":{},"storage_object_id":{},"context_packet_id":{},"grounding_packet_id":{},"deletion_state":{},"artifact_deleted_receipt_id":{},"storage_tombstone_receipt_id":{},"context_invalidated_receipt_id":{},"policy_decision_id":{},"source_artifact_receipt_id":{},"storage_receipt_id":{},"parse_receipt_id":{},"grounding_receipt_id":{},"blob_delete_required":{},"local_blob_bytes_deleted":{},"context_packets_invalidated":{},"memory_atoms_invalidated":{},"generated_artifacts_invalidated":{},"provider_call_allowed":{},"frontier_raw_content_allowed":{},"production_write_allowed":{}}}"#,
            json_string_literal(self.status),
            json_string_literal(&self.artifact_id),
            json_string_literal(&self.storage_object_id),
            json_string_literal(&self.context_packet_id),
            json_string_literal(&self.grounding_packet_id),
            json_string_literal(self.deletion_state),
            json_string_literal(&self.artifact_deleted_receipt_id),
            json_string_literal(&self.storage_tombstone_receipt_id),
            json_string_literal(&self.context_invalidated_receipt_id),
            json_string_literal(&self.policy_decision_id),
            json_string_literal(&self.source_artifact_receipt_id),
            json_string_literal(&self.storage_receipt_id),
            json_string_literal(&self.parse_receipt_id),
            json_string_literal(&self.grounding_receipt_id),
            self.blob_delete_required,
            self.local_blob_bytes_deleted,
            self.context_packets_invalidated,
            self.memory_atoms_invalidated,
            self.generated_artifacts_invalidated,
            self.provider_call_allowed,
            self.frontier_raw_content_allowed,
            self.production_write_allowed
        )
    }
}

fn normalize_twin_artifact_type(mime_type: &str) -> Option<&'static str> {
    match mime_type.trim() {
        "application/pdf" => Some("pdf_text"),
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => {
            Some("docx_text")
        }
        "text/csv" | "application/csv" => Some("csv_table_preview"),
        "text/plain" | "text/markdown" => Some("plain_text"),
        _ => None,
    }
}

fn local_artifact_interpretation(
    artifact_name: &str,
    normalized_type: &str,
    extracted_text_char_count: usize,
    company_context_source: &str,
) -> String {
    format!(
        "Twin parsed {artifact_name} as {normalized_type}, built a private context packet with {extracted_text_char_count} characters, and grounded the read against {company_context_source}. The useful next move is a Pages draft with citations separated between the uploaded artifact and company context."
    )
}

fn render_trusted_context_grounding_summary(sources: &[TrustedContextSourceSummary]) -> String {
    if sources.is_empty() {
        return "No trusted Context sources are currently approved for broader AI use.".to_string();
    }
    sources
        .iter()
        .map(|source| {
            format!(
                "{} is a trusted Context source from {} with source hash {} and trust receipt {}.",
                source.title,
                source.company_context_source,
                source.source_artifact_hash,
                source.trust_decision_receipt_id
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TwinArtifactProviderRoute {
    provider_class: &'static str,
    provider_id: String,
    model_id: String,
    model_gateway_routing: String,
    provider_observation_receipt_id: String,
    sovereign_provider_required: bool,
    sovereign_provider_connected: bool,
    provider_not_connected_reason: String,
    frontier_refusal_reason: &'static str,
    provider_call_allowed: bool,
}

fn twin_artifact_provider_route(
    entries: &[Receipt],
    privacy_class: &str,
) -> TwinArtifactProviderRoute {
    let sensitive = privacy_class == TWIN_ARTIFACT_DEFAULT_PRIVACY_CLASS
        || privacy_class.contains("sensitive")
        || privacy_class.contains("private");
    if !sensitive {
        return TwinArtifactProviderRoute {
            provider_class: "local",
            provider_id: "deterministic_local".to_string(),
            model_id: "deterministic_local_v1".to_string(),
            model_gateway_routing: "tier:internal class:local provider:deterministic_local"
                .to_string(),
            provider_observation_receipt_id: String::new(),
            sovereign_provider_required: false,
            sovereign_provider_connected: true,
            provider_not_connected_reason: String::new(),
            frontier_refusal_reason: "not_required_for_non_sensitive_artifact",
            provider_call_allowed: false,
        };
    }
    if let Some(receipt) = entries
        .iter()
        .rev()
        .find(|receipt| private_artifact_provider_receipt_allowed(receipt))
    {
        let model_id = receipt_payload_value(receipt, "model_id");
        let provider_id = receipt_payload_value(receipt, "provider_id");
        return TwinArtifactProviderRoute {
            provider_class: "local",
            provider_id: provider_id.clone(),
            model_gateway_routing: format!(
                "tier:sensitive class:local provider:{provider_id} model:{model_id}"
            ),
            model_id,
            provider_observation_receipt_id: receipt.receipt_id.clone(),
            sovereign_provider_required: true,
            sovereign_provider_connected: true,
            provider_not_connected_reason: String::new(),
            frontier_refusal_reason: "frontier_refused_for_raw_sensitive_artifact",
            provider_call_allowed: true,
        };
    }
    TwinArtifactProviderRoute {
        provider_class: "local",
        provider_id: String::new(),
        model_id: String::new(),
        model_gateway_routing: "tier:sensitive class:local provider:none".to_string(),
        provider_observation_receipt_id: String::new(),
        sovereign_provider_required: true,
        sovereign_provider_connected: false,
        provider_not_connected_reason: "connect_private_model".to_string(),
        frontier_refusal_reason: "frontier_refused_for_raw_sensitive_artifact",
        provider_call_allowed: false,
    }
}

fn private_artifact_provider_receipt_allowed(receipt: &Receipt) -> bool {
    receipt.kind == "twin.model_gateway.provider.observed"
        && receipt_payload_value(receipt, "provider_id") == "ollama"
        && receipt_payload_value(receipt, "provider_connected_local_allowed") == "true"
        && receipt_payload_value(receipt, "credential_values_recorded") == "false"
        && receipt_payload_value(receipt, "provider_secret_values_recorded") == "false"
        && receipt_payload_value(receipt, "output_text_recorded") == "false"
}

fn private_model_artifact_interpretation(
    artifact_name: &str,
    normalized_type: &str,
    artifact_text: &str,
    company_context_source: &str,
    company_context: &str,
    provider_route: &TwinArtifactProviderRoute,
) -> String {
    let artifact_signal = first_meaningful_sentence(artifact_text)
        .unwrap_or_else(|| format!("{artifact_name} is a {normalized_type} artifact"));
    let company_signal = first_meaningful_sentence(company_context)
        .unwrap_or_else(|| company_context_source.to_string());
    format!(
        "The important read: {artifact_signal} In company context, this connects to {company_signal} My take is that this is not just a file to summarize; it is a decision prompt. The next useful move is to turn it into a Pages draft that names the owner, the risk, and the follow-up decision while keeping the uploaded artifact and company context cited separately. Read privately through {} using {}.",
        provider_route.provider_id, provider_route.model_id
    )
}

fn first_meaningful_sentence(value: &str) -> Option<String> {
    value
        .split(['.', '\n'])
        .map(str::trim)
        .find(|part| {
            part.chars()
                .filter(|character| character.is_alphanumeric())
                .count()
                >= 12
        })
        .map(|part| {
            let mut sentence: String = part.chars().take(220).collect();
            if !sentence.ends_with('.') {
                sentence.push('.');
            }
            sentence
        })
}

fn bool_string(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

fn local_artifact_hash(value: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("fnv1a64:{hash:016x}")
}

fn latest_receipt_for_artifact(entries: &[Receipt], artifact_id: &str, kind: &str) -> String {
    entries
        .iter()
        .rev()
        .find(|receipt| {
            receipt.kind == kind && receipt_payload_value(receipt, "artifact_id") == artifact_id
        })
        .map(|receipt| receipt.receipt_id.clone())
        .unwrap_or_default()
}

fn latest_payload_for_artifact(
    entries: &[Receipt],
    artifact_id: &str,
    kind: &str,
    key: &str,
) -> String {
    entries
        .iter()
        .rev()
        .find(|receipt| {
            receipt.kind == kind && receipt_payload_value(receipt, "artifact_id") == artifact_id
        })
        .map(|receipt| receipt_payload_value(receipt, key))
        .unwrap_or_default()
}

fn receipt_payload_value(receipt: &Receipt, key: &str) -> String {
    receipt.payload.get(key).cloned().unwrap_or_default()
}
