use std::{
    collections::BTreeMap,
    fmt,
    io::Write,
    path::{Component, Path},
    time::{SystemTime, UNIX_EPOCH},
};
mod access_control_matrix;
mod action_kind;
mod activation;
mod actor_admission;
mod advisor_call;
mod advisor_policy;
mod agent_delegation_runtime;
mod app_health;
mod app_state_export;
mod auth_access_management;
mod auth_session_control;
mod auth_tenant_policy_preflight;
mod auth_user_admission;
mod beta_feedback;
mod beta_program;
mod capability_execution;
mod changelog;
mod connector_ingest;
mod ctx_local_engine;
mod deployment_profile;
mod dxr_local_runtime;
mod dxr_run_control;
mod evidence_checkpoint;
mod feedback_autonomy;
mod first_run_profile;
mod fixture_graduation;
mod fleet_plan;
mod fleet_run;
mod forge_builder_loop;
mod forge_dogfood;
mod forge_execution_geometry;
mod forge_execution_posture;
mod forge_outcome_signal;
mod forge_pr_handoff;
mod forge_recipes;
mod forge_repo;
mod forge_review_panel;
mod forge_run;
mod forge_run_control;
mod forge_run_ship;
mod forge_run_strategy;
mod forge_ship_ratification;
mod harness_pipeline;
mod harness_sensors;
mod http_routes;
mod install_owner;
mod learning_adaptation;
mod learning_candidate_rejection;
mod learning_evidence_request;
mod learning_implicit_signal;
mod learning_judgment_decision;
mod learning_memory_activation;
mod learning_memory_promotion;
mod learning_memory_supersede;
mod marketplace_act;
mod marketplace_skill;
mod memory_brain;
mod memory_consolidation;
mod memory_store;
mod message_action;
mod message_bridge;
mod message_channel;
mod message_fanout_request;
mod message_presence_request;
mod message_realtime_cutover_preflight;
mod message_relay_observation;
mod message_thread_message;
mod mobile_trust;
mod model_fabric;
mod model_turn_on;
mod pages_approval_request;
mod pages_context_trust_decision;
mod pages_edit_draft;
mod pages_publication;
mod pages_search_preflight;
mod product_board;
mod product_ratification_decision;
mod production_auth_boundary;
mod setup_track;
mod strategy_board;
mod strategy_direction_proposal;
mod strategy_direction_record;
mod strategy_ratification_decision;
mod studio_presence;
mod studio_run;
mod studio_steering;
mod trusted_session_verifier;
mod twin_artifact_context;
mod twin_boundary_refusal;
mod twin_model_gateway_observation;
mod twin_office_capability;
mod twin_runtime_control;
mod twin_session_draft;
mod twin_session_draft_json;
mod twin_session_live;
mod twin_session_trusted_context;
mod v1_read_shadow_approval_request;
mod work_plane;
mod work_triage;
pub use access_control_matrix::{
    ACCESS_DENY_CASES, DbBackstop, Decision, LEAKAGE_CLASSES, LEAKAGE_MATRIX, LeakageMatrixReport,
    MatrixCell, PENDING_RESOURCE_AWARE_RLS_SURFACES, Principal, Surface, evaluate,
    validate_leakage_matrix,
};
pub use action_kind::ActionKind;
pub use activation::{
    ACTIVATION_EVENT_KINDS, ACTIVATION_FIRST_MISSION_BRIEF_SHAPED,
    ACTIVATION_FIRST_MISSION_COMPLETED, ACTIVATION_FIRST_MISSION_RESULT_RECORDED,
    ACTIVATION_FIRST_MISSION_RUN_ADMITTED, ACTIVATION_FIRST_MISSION_STARTED,
    ACTIVATION_FIRST_PROOF_RECORDED, ACTIVATION_FORGE_WORKSPACE_SAVED,
    ACTIVATION_MODEL_SETUP_RECORDED, ACTIVATION_PROFILE_SAVED, ACTIVATION_STARTER_WORKSPACE_SEEDED,
    ActivationEvent, ActivationEventError, ActivationEventReport,
};
pub use actor_admission::{
    ActorAdmission, ActorAdmissionError, ActorAdmissionReport, GOVERNED_WRITE_ROUTES,
    admit_local_actor_for_governed_write, admit_local_route_actor,
};
pub use advisor_call::{
    ADVISOR_CALL_PAYLOAD_FIELDS, ADVISOR_CALL_RECEIPT_KIND, ADVISOR_GRANTS_EXECUTION_AUTHORITY,
    ADVISOR_MAX_DIGEST_LEN, ADVISOR_OUTCOMES, ADVISOR_TRIGGERS, AdvisorCall, AdvisorCallError,
    AdvisorCallReport, RecordAdvisorCall,
};
pub use advisor_policy::{
    AdvisorConsultContext, AdvisorGateDecision, AdvisorPolicy, decide_advisor_consult,
};
pub use agent_delegation_runtime::{
    ActorKind, AdmittedDelegation, DELEGATION_REFUSAL_CASES, DelegationError, DelegationLease,
    SponsorAuthority, admit_agent_delegation,
};
pub use app_health::*;
pub use app_state_export::{
    PostgresAppStateWriteReport, PostgresAppStateWriter, PostgresAppStateWriterContract,
    render_postgres_app_state_export_sql,
};
pub use auth_access_management::*;
pub use auth_session_control::*;
pub use auth_tenant_policy_preflight::{
    AuthTenantPolicyPreflight, AuthTenantPolicyPreflightError, AuthTenantPolicyPreflightReport,
};
pub use auth_user_admission::*;
pub use beta_feedback::*;
pub use beta_program::*;
pub use changelog::{ChangelogEntry, ChangelogError, ChangelogReport};
pub use connector_ingest::{
    CONNECTOR_GRADES, CONNECTOR_HANDLING, CONNECTOR_KINDS, CONNECTOR_SCOPES, CONNECTOR_SENSITIVITY,
    ExternalItem, ExternalItemDeletion, ExternalItemDeletionReport, ExternalItemError,
    ExternalItemReport,
};
pub use ctx_local_engine::*;
pub use deployment_profile::{
    DEPLOYMENT_PROFILES, DeploymentReadiness, PROFILE_REFUSAL_CASES, ProfileRefusal,
    StartupPosture, evaluate_startup_profile,
};
pub use dxr_local_runtime::*;
pub use dxr_run_control::{DxrRunControl, DxrRunControlError, DxrRunControlReport};
pub use evidence_checkpoint::*;
pub use feedback_autonomy::*;
pub use first_run_profile::{
    FIRST_RUN_PROFILE_RECEIPT_KIND, FIRST_RUN_ROLES, FIRST_RUN_WORKING_MODES, FirstRunProfileError,
    FirstRunProfileReport, FirstRunProfileState, RecordFirstRunProfile,
};
pub use fleet_plan::{
    FleetPlanDraft, FleetPlanError, FleetPlanReport, FleetRatifyReport, FleetStream,
    fleet_integration_order, fleet_plan_streams, validate_fleet_streams,
};
pub use fleet_run::{FLEET_RUN_EVENTS, FleetRunError, FleetRunEvent, FleetRunEventReport};
pub use forge_builder_loop::{
    BUILDER_LOOP_CHECKER_ATTACHMENT_KIND, BUILDER_LOOP_EXTERNAL_TRIAL_ATTACHMENT_KIND,
    BUILDER_LOOP_FLEET_ATTACHMENT_KIND, BUILDER_LOOP_RUN_ATTACHMENT_KIND, BUILDER_LOOP_TEMPLATES,
    BUILDER_LOOP_TICK_KIND, BUILDER_LOOP_TRIGGERS, BuilderLoopCheckerAttachment,
    BuilderLoopCheckerAttachmentReport, BuilderLoopError, BuilderLoopExternalTrialAttachment,
    BuilderLoopExternalTrialAttachmentReport, BuilderLoopFleetAttachment,
    BuilderLoopFleetAttachmentReport, BuilderLoopRunAttachment, BuilderLoopRunAttachmentReport,
    BuilderLoopState, BuilderLoopTemplate, BuilderLoopTick, BuilderLoopTickReport,
    builder_loop_template,
};
pub use forge_dogfood::{ForgeDogfoodError, ForgeDogfoodReport, run_forge_dogfood};
pub use forge_execution_geometry::{
    DIRECT_RUN_MAX_WORKERS, FLEET_GEOMETRY_MAX_WORKERS, ForgeExecutionGeometry,
    forge_execution_geometry_for_width,
};
pub use forge_execution_posture::{ForgeExecutionPosture, ForgeExecutionPostureReport};
pub use forge_outcome_signal::{
    FORGE_OUTCOME_DISPOSITIONS, FORGE_OUTCOME_LESSON_SOURCES, ForgeOutcomeSignal,
    ForgeOutcomeSignalError, ForgeOutcomeSignalReport,
};
pub use forge_pr_handoff::{ForgePrHandoff, ForgePrHandoffError, ForgePrHandoffReport};
pub use forge_recipes::{FORGE_RECIPES, ForgeRecipe, forge_recipe};
pub use forge_repo::{
    FORGE_REPO_KINDS, ForgeRepoConnect, ForgeRepoError, ForgeRepoIndex, ForgeRepoIndexReport,
    ForgeRepoReport,
};
pub use forge_review_panel::{
    ForgeReviewPanel, ForgeReviewPanelError, ForgeReviewPanelReport, PanelMember,
};
pub use forge_run::*;
pub use forge_run_control::{
    FORGE_RUN_CONTROLS, ForgeRunControl, ForgeRunControlError, ForgeRunControlReport,
    PendingForgeRunControl,
};
pub use forge_run_ship::{ForgeRunShipDecision, ForgeRunShipError, ForgeRunShipReport};
pub use forge_ship_ratification::{
    ForgeShipDecisionReport, ForgeShipDecisionRequest, HumanShipDecision,
    LocalForgeShipRatification,
};
pub use harness_pipeline::{
    HarnessPipelineError, HarnessPipelineRequest, HarnessPipelineRunState, HarnessPipelineStage,
    HarnessPipelineStageOutcome, HarnessPipelineStep, HarnessPipelineVerdict,
    LocalHarnessPipelineRuntime, PIPELINE_NOT_RUNNABLE_STAGE_KINDS, PIPELINE_RUNNABLE_STAGE_KINDS,
    PipelineModelExecution, PipelineModelExecutor, PipelineModelStageContext,
    PreparedPipelineModelCall,
};
pub use harness_sensors::{
    HarnessSensorOutcome, SENSOR_BANNED_FIRST_READ_PHRASES, SENSOR_NEVER_ADVISORY_DOMAINS,
    SensorRunEvidence, SensorSummary, evaluate_pipeline_sensors, summarize_sensors,
};
pub use http_routes::{HttpRouteDeclaration, local_http_routes};
pub use mobile_trust::{
    MOBILE_DEVICE_ATTESTATION_VERIFIED_KIND, MOBILE_DEVICE_REGISTERED_KIND,
    MOBILE_DEVICE_REVOKED_KIND, MOBILE_HOST_REGISTERED_KIND, MOBILE_HOST_REVOKED_KIND,
    MOBILE_PAIRING_RECORDED_KIND, MOBILE_PUSH_REGISTRATION_RECORDED_KIND, MobileDeviceAttestation,
    MobileDeviceRegistration, MobileDeviceTrust, MobileHostRegistration, MobileHostTrust,
    MobilePairingRegistration, MobilePairingTrust, MobilePushRegistration,
    MobilePushRegistrationTrust, MobileTrustError, MobileTrustProjection, MobileTrustWriteReport,
};
pub const PAGES_RUNTIME_READINESS_ROUTE: &str = "/pages/runtime-readiness.json";
pub use capability_execution::{
    CAPABILITY_EXECUTION_GRANT_RECORDED_KIND, CAPABILITY_EXECUTION_GRANT_REFUSED_KIND,
    CAPABILITY_EXECUTION_GRANT_REVOKED_KIND, CAPABILITY_EXECUTION_RAN_KIND,
    CAPABILITY_EXECUTION_REFUSED_KIND, CapabilityExecutionAuthorization, CapabilityExecutionError,
    CapabilityExecutionGrant, CapabilityExecutionGrantReport, CapabilityExecutionGrantRequest,
    CapabilityExecutionGrantRevoke, CapabilityExecutionGrantRevokeReport,
    CapabilityExecutionGrantView, CapabilityExecutionOutcome, CapabilityExecutionReceiptReport,
    CapabilityExecutionRunView, MAX_EXECUTION_STEPS_CEILING,
};
pub use fixture_graduation::{
    FIXTURE_GRADUATION_DRAFTED_KIND, FIXTURE_GRADUATION_GRADUATED_KIND_RESERVED,
    FIXTURE_GRADUATION_SOURCE_KINDS, FixtureGraduationDraft, FixtureGraduationDraftError,
    FixtureGraduationDraftReport,
};
pub use install_owner::{
    ClaimInstallOwner, INSTALL_OWNER_RECEIPT_KIND, InstallOwnerError, InstallOwnerReport,
    InstallOwnerState,
};
pub use learning_adaptation::{
    ActiveAdaptationGrant, LEARNING_ADAPTATION_TARGET_TYPES, LEARNING_ADAPTATION_TYPE,
    LearningAdaptationApplied, LearningAdaptationAppliedReport, LearningAdaptationGrant,
    LearningAdaptationGrantError, LearningAdaptationGrantReport, LearningAdaptationSupersede,
    LearningAdaptationSupersedeError, LearningAdaptationSupersedeReport,
};
pub use learning_candidate_rejection::{
    LEARNING_CANDIDATE_SOURCE_KINDS, LearningCandidateRejection, LearningCandidateRejectionError,
    LearningCandidateRejectionReport,
};
pub use learning_evidence_request::{
    LearningEvidenceRequest, LearningEvidenceRequestError, LearningEvidenceRequestReport,
};
pub use learning_implicit_signal::{
    LEARNING_IMPLICIT_SIGNAL_KINDS, LearningImplicitSignal, LearningImplicitSignalError,
    LearningImplicitSignalReport,
};
pub use learning_judgment_decision::{
    LEARNING_JUDGMENT_DECISIONS, LearningJudgmentDecision, LearningJudgmentDecisionError,
    LearningJudgmentDecisionReport,
};
pub use learning_memory_activation::{
    LearningMemoryActivation, LearningMemoryActivationError, LearningMemoryActivationReport,
};
pub use learning_memory_promotion::{
    LEARNING_MEMORY_TARGET_TYPES, LearningMemoryApplicability, LearningMemoryPromotion,
    LearningMemoryPromotionError, LearningMemoryPromotionReport,
};
pub use learning_memory_supersede::{
    LearningMemorySupersede, LearningMemorySupersedeError, LearningMemorySupersedeReport,
};
pub use marketplace_act::{
    MARKETPLACE_ACT_KINDS, MarketplaceAct, MarketplaceActError, MarketplaceActReport,
};
pub use marketplace_skill::{
    DistilledSkillView, MARKETPLACE_SKILL_DRAFTED_KIND, MARKETPLACE_SKILL_PROCEDURE_SOURCES,
    MARKETPLACE_SKILL_RATIFIED_KIND, MARKETPLACE_SKILL_SENSITIVITIES, MarketplaceSkillDraft,
    MarketplaceSkillDraftReport, MarketplaceSkillError, MarketplaceSkillRatification,
    MarketplaceSkillRatificationReport,
};
pub use memory_brain::*;
pub use memory_consolidation::*;
pub use memory_store::{
    LOCAL_MEMORY_DRIVER, MEM0_VENDOR_MEMORY_DRIVER, MemoryDriverContract,
    memory_store_driver_contracts, render_postgres_memory_export_sql,
};
pub use twin_office_capability::{
    TWIN_OFFICE_GENERATION_POLICY, TWIN_OFFICE_NATIVE_PACK_ID, TWIN_OFFICE_NATIVE_PACK_VERSION,
    TwinOfficeCapabilityError, TwinOfficeDownloadReport, TwinOfficeDownloadRequest,
    TwinOfficeGeneratedArtifactReport, TwinOfficeGeneratedArtifactRequest,
};

pub mod memory {
    pub use super::{
        InMemoryProvider, LOCAL_MEMORY_DRIVER, MEM0_VENDOR_MEMORY_DRIVER, Mem0MemoryProvider,
        MemoryDriverContract, MemoryProvenance, MemoryProvider, MemoryProviderAdapterError,
        memory_store_driver_contracts,
    };
}
pub use message_action::{
    MESSAGE_ACTION_VERDICTS, MessageActionError, MessageActionRequest, MessageActionRequestReport,
    MessageActionVerdict, MessageActionVerdictReport,
};
pub use message_bridge::{
    BridgeError, BridgePost, BridgePostReport, MESSAGE_BRIDGE_KINDS, MESSAGE_BRIDGE_POST_KIND,
};
pub use message_channel::{
    ChannelCreate, ChannelError, ChannelMemberAdd, ChannelMemberRemove, ChannelMemberReport,
    ChannelReport, ChannelUpdate, MESSAGE_CHANNEL_CREATED_KIND, MESSAGE_CHANNEL_KINDS,
    MESSAGE_CHANNEL_MEMBER_ADDED_KIND, MESSAGE_CHANNEL_MEMBER_REMOVED_KIND,
    MESSAGE_CHANNEL_STATUSES, MESSAGE_CHANNEL_UPDATED_KIND, MESSAGE_CHANNEL_VISIBILITIES,
    MESSAGE_MEMBER_ROLES,
};
pub use message_fanout_request::{
    MessageFanoutRequest, MessageFanoutRequestError, MessageFanoutRequestReport,
};
pub use message_presence_request::{
    MessagePresenceRequest, MessagePresenceRequestError, MessagePresenceRequestReport,
};
pub use message_realtime_cutover_preflight::*;
pub use message_relay_observation::*;
pub use message_thread_message::{
    MESSAGE_THREAD_MESSAGE_CONTENT_TYPES, MESSAGE_THREAD_MESSAGE_POSTED_KINDS,
    MessageThreadMessage, MessageThreadMessageError, MessageThreadMessageReport,
    is_message_thread_message_receipt_kind, message_receipt_kind_for_actor_id,
};
pub use model_fabric::*;
pub use model_turn_on::{
    ApproveModelTurnOn, ConnectedModel, MODEL_TURN_ON_APPROVAL_RECEIPT_KIND,
    ModelTurnOnApprovalReport, ModelTurnOnError,
};
pub use pages_approval_request::*;
pub use pages_context_trust_decision::*;
pub use pages_edit_draft::{PagesEditDraft, PagesEditDraftError, PagesEditDraftReport};
pub use pages_publication::{
    PAGE_TYPES, PagesApprovedPublication, PagesPublication, PagesPublicationError,
    PagesPublicationReport, normalize_page_type,
};
pub use pages_search_preflight::*;
pub use product_board::*;
pub use product_ratification_decision::*;
pub use production_auth_boundary::{
    AdmittedIdentity, ClientClaimedIdentity, DeploymentMode, GovernedWriteIdentity,
    PRODUCTION_AUTH_REFUSAL_CASES, ProductionAuthError, TrustedSession,
    admit_governed_write_identity, production_boot_gate,
};
pub use setup_track::{
    ChooseSetupTrack, ChosenSetupTrack, RefusedSetupTrackReport, SETUP_TRACK_CHOSEN_RECEIPT_KIND,
    SETUP_TRACK_REFUSED_RECEIPT_KIND, SETUP_TRACKS, SetupTrackError, SetupTrackReport,
};
pub use strategy_board::*;
pub use strategy_direction_proposal::*;
pub use strategy_direction_record::*;
pub use strategy_ratification_decision::*;
pub use studio_presence::{
    STUDIO_PRESENCE_ACTIVE_RECENT, STUDIO_PRESENCE_ROSTER_LIMIT, StudioPresenceEntry,
    StudioPresenceMark, StudioPresenceReport,
};
pub use studio_run::{
    STUDIO_ARTIFACT_KINDS, STUDIO_IMPLEMENTED_KINDS, STUDIO_SOURCE_KINDS, StudioDocumentRun,
    StudioDocumentRunReport, StudioRunError, normalize_studio_artifact_kind,
    normalize_studio_source_kind, studio_kind_page_type,
};
pub use studio_steering::{
    StudioControl, StudioControlReport, StudioLandReport, StudioLanding, StudioOpenReport,
    StudioOpenRun, StudioRunState, StudioSteering,
};
pub use trusted_session_verifier::{
    AuthProfile, TrustedSessionVerifier, VERIFIER_REFUSAL_CASES, VerifiedClaims, VerifierRefusal,
    admit_verified_claims, parse_claim_token, precheck_token_presence,
};
pub use twin_artifact_context::*;
pub use twin_boundary_refusal::*;
pub use twin_model_gateway_observation::*;
pub use twin_runtime_control::*;
pub use twin_session_draft::{TwinSessionDraftError, TwinSessionDraftRequest};
pub use twin_session_live::{TwinLiveAnswer, TwinSessionDraftReport};
pub use v1_read_shadow_approval_request::*;
pub use work_plane::*;
pub use work_triage::*;
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TenantId(String);
impl TenantId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl fmt::Display for TenantId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ActorId(String);
impl ActorId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl fmt::Display for ActorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TraceId(String);
impl TraceId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl fmt::Display for TraceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LoopId(String);
impl LoopId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl fmt::Display for LoopId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WorkflowId(String);
impl WorkflowId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl fmt::Display for WorkflowId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CorrelationIds {
    pub tenant_id: TenantId,
    pub trace_id: TraceId,
    pub actor_id: ActorId,
    pub loop_id: LoopId,
    pub workflow_id: WorkflowId,
}
#[derive(Debug, Default)]
pub struct IdFactory {
    next: u64,
}
impl IdFactory {
    pub fn next(&mut self, prefix: &str) -> String {
        self.next += 1;
        format!("{prefix}_{:06}", self.next)
    }
    /// The current counter value, for durable snapshots.
    pub fn counter(&self) -> u64 {
        self.next
    }

    /// Restore the counter from a durable snapshot. Never moves backwards:
    /// a stale snapshot must not reissue ids that already exist.
    pub fn restore_counter(&mut self, value: u64) {
        self.next = self.next.max(value);
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt {
    pub receipt_id: String,
    pub tenant_id: TenantId,
    pub trace_id: TraceId,
    pub actor_id: ActorId,
    pub loop_id: LoopId,
    pub workflow_id: WorkflowId,
    pub kind: String,
    pub policy_decision_id: Option<String>,
    pub payload: BTreeMap<String, String>,
    pub previous_hash: Option<String>,
    pub receipt_timestamp: String,
    pub hash_version: u64,
    pub hash: String,
}
pub const RECEIPT_HASH_VERSION_TIMELESS: u64 = 1;
pub const RECEIPT_HASH_VERSION_TRUSTED_TIME: u64 = 2;
pub const RECEIPT_TRUSTED_TIME_EPOCH: &str = "2026-01-01T00:00:00";
pub const POLICY_DECISION_RECEIPT_KIND: &str = "loop.action.policy_decided";
pub const CONSEQUENTIAL_RECEIPT_KINDS: [&str; 48] = [
    "loop.triggered",
    "harness.run.admitted",
    "harness.plan.emitted",
    "harness.write.refused",
    "harness.tool.read.allowed",
    "harness.tool.read.denied",
    "harness.tool.search.allowed",
    "harness.tool.search.denied",
    "harness.tool.list.allowed",
    "harness.tool.list.denied",
    "harness.tool.patch.allowed",
    "harness.tool.patch.denied",
    "eval.suite.ran",
    "eval.verdict.recorded",
    "credential.minted",
    "memory.consolidation_decided",
    "ctx.context.assembled",
    "ctx.embedding_provider.refused",
    "auth.user_admission.approved",
    "charter.evidence.recorded",
    "charter.obligation.checked",
    "charter.evidence.attested",
    "charter.exception.reviewed",
    "forge.spec.accepted",
    "forge.stage.transition.recorded",
    "forge.delegation.requested",
    "talent.sponsor_chain.authorized",
    "talent.worker_lease.authorized",
    "talent.budget.authorized",
    "talent.tool_allowlist.authorized",
    "talent.authorization.recorded",
    "worker.credential.checked",
    "worker.spawn_requested",
    "worker.handoff.recorded",
    "worker.retired",
    "forge.fleet_eval_result.ingested",
    "forge.fleet_eval_runner.execution.recorded",
    "forge.fleet_eval_result.scored",
    "forge.fleet_eval_live_run.approved",
    "forge.long_horizon_mission.admitted",
    "forge.work_classification.recorded",
    "loop.adjustment.recorded",
    "treasury.authority.granted",
    MOBILE_DEVICE_REGISTERED_KIND,
    MOBILE_HOST_REGISTERED_KIND,
    MOBILE_PAIRING_RECORDED_KIND,
    MOBILE_DEVICE_REVOKED_KIND,
    MOBILE_HOST_REVOKED_KIND,
];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalLoopBudget {
    pub loop_id: &'static str,
    pub max_runtime_ms: usize,
    pub max_receipts: usize,
    pub max_policy_decisions: usize,
    pub required_transitions: usize,
    pub max_outbox_events: usize,
    pub max_credentials: usize,
    pub max_memory_records: usize,
    pub max_charter_records: usize,
}
include!(concat!(env!("OUT_DIR"), "/loop_budgets.rs"));
pub fn local_loop_budget(loop_id: &str) -> Option<&'static LocalLoopBudget> {
    local_loop_budgets()
        .iter()
        .find(|budget| budget.loop_id == loop_id)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkerSpawnBudget {
    pub max_runtime_ms: usize,
    pub max_tool_calls: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkerSpawnAdmission<'a> {
    pub worker_template_id: &'a str,
    pub parent_id: &'a str,
    pub sponsor_chain_authority_receipt_id: &'a str,
    pub human_sponsor_chain: &'a [&'a str],
    pub worker_lease_authority_receipt_id: &'a str,
    pub scope: &'a str,
    pub credential_scope: &'a str,
    pub expires_at: &'a str,
    pub now: &'a str,
    pub budget_authority_receipt_id: &'a str,
    pub budget: WorkerSpawnBudget,
    pub tool_allowlist_authority_receipt_id: &'a str,
    pub tool_allowlist: &'a [&'a str],
    pub credential_check_receipt_id: &'a str,
    pub credential_requested_receipt_kind: &'a str,
    pub issuer_loop_id: &'a str,
    pub requested_receipt_kind: &'a str,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkerSpawnRejection {
    MissingField(&'static str),
    MissingAuthorityReceipt(&'static str),
    MissingHumanSponsorChain,
    ScopeMismatch,
    ExpiredLeaseOrCredential,
    EmptyToolAllowlist,
    InvalidCredentialReceiptKind,
    InvalidIssuerLoop,
    InvalidSpawnReceiptKind,
    EmptyBudget,
}
impl WorkerSpawnRejection {
    pub fn message(&self) -> String {
        match self {
            Self::MissingField(field) => format!("missing worker spawn field {field}"),
            Self::MissingAuthorityReceipt(field) => {
                format!("missing worker spawn authority receipt {field}")
            }
            Self::MissingHumanSponsorChain => "missing human sponsor chain".to_string(),
            Self::ScopeMismatch => "worker credential scope does not match spawn scope".to_string(),
            Self::ExpiredLeaseOrCredential => {
                "worker lease or credential is expired at admission time".to_string()
            }
            Self::EmptyToolAllowlist => "worker tool allowlist is empty".to_string(),
            Self::InvalidCredentialReceiptKind => {
                "credential check must request worker.credential.checked".to_string()
            }
            Self::InvalidIssuerLoop => {
                "worker credential issuer must be evals_runner_agent".to_string()
            }
            Self::InvalidSpawnReceiptKind => {
                "worker spawn must request worker.spawn_requested".to_string()
            }
            Self::EmptyBudget => {
                "worker spawn budget must allow runtime and tool calls".to_string()
            }
        }
    }
}
pub fn admit_worker_spawn(request: &WorkerSpawnAdmission<'_>) -> Result<(), WorkerSpawnRejection> {
    for (field, value) in [
        ("worker_template_id", request.worker_template_id),
        ("parent_id", request.parent_id),
        ("scope", request.scope),
        ("credential_scope", request.credential_scope),
        ("expires_at", request.expires_at),
        ("now", request.now),
    ] {
        if value.trim().is_empty() {
            return Err(WorkerSpawnRejection::MissingField(field));
        }
    }
    for (field, value) in [
        (
            "sponsor_chain_authority_receipt_id",
            request.sponsor_chain_authority_receipt_id,
        ),
        (
            "worker_lease_authority_receipt_id",
            request.worker_lease_authority_receipt_id,
        ),
        (
            "budget_authority_receipt_id",
            request.budget_authority_receipt_id,
        ),
        (
            "tool_allowlist_authority_receipt_id",
            request.tool_allowlist_authority_receipt_id,
        ),
        (
            "credential_check_receipt_id",
            request.credential_check_receipt_id,
        ),
    ] {
        if value.trim().is_empty() {
            return Err(WorkerSpawnRejection::MissingAuthorityReceipt(field));
        }
    }
    if request.human_sponsor_chain.is_empty()
        || request
            .human_sponsor_chain
            .iter()
            .any(|sponsor| sponsor.trim().is_empty())
    {
        return Err(WorkerSpawnRejection::MissingHumanSponsorChain);
    }
    if request.scope != request.credential_scope {
        return Err(WorkerSpawnRejection::ScopeMismatch);
    }
    if request.expires_at <= request.now {
        return Err(WorkerSpawnRejection::ExpiredLeaseOrCredential);
    }
    if request.budget.max_runtime_ms == 0 || request.budget.max_tool_calls == 0 {
        return Err(WorkerSpawnRejection::EmptyBudget);
    }
    if request.tool_allowlist.is_empty()
        || request
            .tool_allowlist
            .iter()
            .any(|tool| tool.trim().is_empty())
    {
        return Err(WorkerSpawnRejection::EmptyToolAllowlist);
    }
    if request.credential_requested_receipt_kind != "worker.credential.checked" {
        return Err(WorkerSpawnRejection::InvalidCredentialReceiptKind);
    }
    if request.issuer_loop_id != "evals_runner_agent" {
        return Err(WorkerSpawnRejection::InvalidIssuerLoop);
    }
    if request.requested_receipt_kind != "worker.spawn_requested" {
        return Err(WorkerSpawnRejection::InvalidSpawnReceiptKind);
    }
    Ok(())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkerHandoffAdmission<'a> {
    pub parent_loop_id: &'a str,
    pub worker_template_id: &'a str,
    pub worker_run_id: &'a str,
    pub spawn_receipt_id: &'a str,
    pub credential_check_receipt_id: &'a str,
    pub output_artifacts: &'a [&'a str],
    pub verification_evidence: &'a [&'a str],
    pub summary: &'a str,
    pub next_owner: &'a str,
    pub requested_receipt_kind: &'a str,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkerHandoffRejection {
    MissingField(&'static str),
    MissingOutputArtifacts,
    MissingVerificationEvidence,
    InvalidHandoffReceiptKind,
}
impl WorkerHandoffRejection {
    pub fn message(&self) -> String {
        match self {
            Self::MissingField(field) => format!("missing worker handoff field {field}"),
            Self::MissingOutputArtifacts => "worker handoff output artifacts are empty".to_string(),
            Self::MissingVerificationEvidence => {
                "worker handoff verification evidence is empty".to_string()
            }
            Self::InvalidHandoffReceiptKind => {
                "worker handoff must request worker.handoff.recorded".to_string()
            }
        }
    }
}
pub fn admit_worker_handoff(
    request: &WorkerHandoffAdmission<'_>,
) -> Result<(), WorkerHandoffRejection> {
    for (field, value) in [
        ("parent_loop_id", request.parent_loop_id),
        ("worker_template_id", request.worker_template_id),
        ("worker_run_id", request.worker_run_id),
        ("spawn_receipt_id", request.spawn_receipt_id),
        (
            "credential_check_receipt_id",
            request.credential_check_receipt_id,
        ),
        ("summary", request.summary),
        ("next_owner", request.next_owner),
    ] {
        if value.trim().is_empty() {
            return Err(WorkerHandoffRejection::MissingField(field));
        }
    }
    if request.output_artifacts.is_empty()
        || request
            .output_artifacts
            .iter()
            .any(|artifact| artifact.trim().is_empty())
    {
        return Err(WorkerHandoffRejection::MissingOutputArtifacts);
    }
    if request.verification_evidence.is_empty()
        || request
            .verification_evidence
            .iter()
            .any(|evidence| evidence.trim().is_empty())
    {
        return Err(WorkerHandoffRejection::MissingVerificationEvidence);
    }
    if request.requested_receipt_kind != "worker.handoff.recorded" {
        return Err(WorkerHandoffRejection::InvalidHandoffReceiptKind);
    }
    Ok(())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkerRetirementAdmission<'a> {
    pub parent_loop_id: &'a str,
    pub worker_template_id: &'a str,
    pub worker_run_id: &'a str,
    pub spawn_receipt_id: &'a str,
    pub handoff_receipt_id: &'a str,
    pub requested_receipt_kind: &'a str,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkerRetirementRejection {
    MissingField(&'static str),
    MissingHandoffReceipt,
    InvalidRetirementReceiptKind,
}
impl WorkerRetirementRejection {
    pub fn message(&self) -> String {
        match self {
            Self::MissingField(field) => format!("missing worker retirement field {field}"),
            Self::MissingHandoffReceipt => {
                "worker retirement requires handoff receipt evidence".to_string()
            }
            Self::InvalidRetirementReceiptKind => {
                "worker retirement must request worker.retired".to_string()
            }
        }
    }
}
pub fn admit_worker_retirement(
    request: &WorkerRetirementAdmission<'_>,
) -> Result<(), WorkerRetirementRejection> {
    for (field, value) in [
        ("parent_loop_id", request.parent_loop_id),
        ("worker_template_id", request.worker_template_id),
        ("worker_run_id", request.worker_run_id),
        ("spawn_receipt_id", request.spawn_receipt_id),
    ] {
        if value.trim().is_empty() {
            return Err(WorkerRetirementRejection::MissingField(field));
        }
    }
    if request.handoff_receipt_id.trim().is_empty() {
        return Err(WorkerRetirementRejection::MissingHandoffReceipt);
    }
    if request.requested_receipt_kind != "worker.retired" {
        return Err(WorkerRetirementRejection::InvalidRetirementReceiptKind);
    }
    Ok(())
}
pub mod harness;
mod harness_autonomy_envelope;
mod harness_envelope_registry;
mod harness_execute;
mod harness_fleet_eval;
mod harness_load_sim;
mod harness_long_horizon_mission;
mod harness_operator_packet;
mod harness_provider;
mod harness_provider_policy;
mod harness_review_packet;
mod harness_runtime_metrics;
mod harness_scale_backpressure;
mod harness_ship_readiness;
mod harness_work_classification;
mod harness_worker_run;
mod live_worker_execution;
pub mod pages;
pub use forge_run_strategy::{
    FORGE_RUN_STRATEGY_VERSION, ForgeRunStrategy, ForgeRunStrategyOverrides,
    resolve_forge_run_strategy,
};
pub use harness::{
    HarnessApprovalMode, HarnessAuditReplayReport, HarnessBudgetPolicy, HarnessEnterprisePack,
    HarnessListToolReport, HarnessListToolRequest, HarnessPatchToolReport, HarnessPatchToolRequest,
    HarnessPermissionLevel, HarnessPermissionPolicy, HarnessPlanItem, HarnessPlanRunReport,
    HarnessProviderGatewayError, HarnessProviderGatewayReport, HarnessProviderGatewayRequest,
    HarnessProviderKind, HarnessProviderProfile, HarnessReadToolReport, HarnessReadToolRequest,
    HarnessRunItem, HarnessRunItemKind, HarnessRunManifest, HarnessRunManifestRejection,
    HarnessRunMode, HarnessRunTranscript, HarnessSearchMatch, HarnessSearchToolReport,
    HarnessSearchToolRequest, HarnessToolPlaneError, HarnessTranscriptReplayError,
    HarnessTrustBoundary, LocalHarnessPlanRunner, LocalHarnessProviderGateway,
    LocalHarnessReadToolPlane, admit_harness_run_manifest,
};
pub use harness_autonomy_envelope::{
    AutonomyDisposition, AutonomyPolicyEnvelope, AutonomyRiskClass, AutonomyWorkClassification,
    PROTECTED_PATHS, SelfDeliveryScope, classify_autonomy, path_is_protected,
};
pub use harness_envelope_registry::*;
pub use harness_execute::{
    HarnessExecutionDrivers, HarnessPatchApplier, HarnessPatchApplyContext,
    HarnessPatchApplyResult, HarnessPlanExecuteLoopError, HarnessPlanExecuteLoopReport,
    HarnessPlanExecuteLoopRequest, HarnessPlanExecutionAction, HarnessPlanExecutionActionKind,
    HarnessSandboxRunContext, HarnessSandboxRunResult, HarnessSandboxRunner,
    LocalHarnessPlanExecuteLoop,
};
pub use harness_fleet_eval::{
    FleetBenchmarkTask, FleetModelMatrixProfile, FleetRunnerProfile, FleetRunnerScore,
    FleetScoringDimension, ForgeFleetEvalDryRunReport, ForgeFleetEvalLiveRunApproval,
    ForgeFleetEvalLiveRunApprovalReport, ForgeFleetEvalReport, ForgeFleetEvalResultIngestionError,
    ForgeFleetEvalResultIngestionReport, ForgeFleetEvalResultSubmission,
    ForgeLanguageTaskCorpusEntry, LocalForgeFleetEvalHarness, fleet_eval_priority_for_class,
    fleet_eval_scale_policy, forge_fleet_benchmark_tasks, forge_fleet_model_matrix_profiles,
    forge_fleet_runner_profiles, forge_fleet_scoring_dimensions, forge_language_task_corpus,
    language_task_contamination_policy, language_task_engineering_facets,
    language_task_evaluation_oracle, language_task_human_timebox_minutes, run_forge_fleet_eval,
};
pub use harness_load_sim::{LoadSimConfig, LoadSimReport, run_load_sim, thousand_engineer_config};
pub use harness_long_horizon_mission::{
    ForgeLongHorizonMissionAdmission, ForgeLongHorizonMissionAdmissionError,
    ForgeLongHorizonMissionAdmissionReport, ForgeLongHorizonMissionCheckpoint,
    ForgeLongHorizonMissionCheckpointReport, ForgeLongHorizonMissionDashboard,
    ForgeLongHorizonMissionMilestone, ForgeLongHorizonMissionPacket, LocalForgeLongHorizonMission,
    mission_milestones_json,
};
pub use harness_operator_packet::{
    ForgeOperatorDecisionPacket, ForgeOperatorPacketRequest, LocalForgeOperatorPacket,
};
pub use harness_review_packet::{
    HarnessReviewPacketReport, HarnessReviewPacketRequest, LocalHarnessReviewPacket,
};
pub use harness_runtime_metrics::{DurationStats, LocalRuntimeMetrics, RuntimeMetrics};
pub use harness_runtime_metrics::{RuntimeModelAttribution, RuntimeToolAttribution};
pub use harness_scale_backpressure::{
    ScaleAdmissionDecision, ScaleAdmissionRequest, ScaleBackpressurePolicy, WorkerPriority,
    classify_scale_admission,
};
pub use harness_ship_readiness::{
    HarnessCiEvidence, HarnessShipReadinessReport, HarnessShipReadinessRequest,
    LocalHarnessShipReadiness,
};
pub use harness_work_classification::{
    ForgeWorkClassificationDraft, ForgeWorkClassificationError, ForgeWorkClassificationPacket,
    ForgeWorkClassificationProjection, ForgeWorkClassificationReport,
    ForgeWorkClassificationRequest, LocalForgeWorkClassifier, classify_forge_work,
    forge_work_classification_packet_json, forge_work_classification_packets_json,
};
pub use harness_worker_run::{
    HarnessWorkerRunError, HarnessWorkerRunReport, HarnessWorkerRunRequest, LocalHarnessWorkerRun,
    WorkerAdmissionReport, WorkerScaleAdmissionContext,
};
pub use live_worker_execution::{
    LiveWorkerExecutionAdmission, LiveWorkerExecutionRejection, admit_live_worker_execution,
};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkerRuntimeRequest {
    pub spawn_receipt_id: String,
    pub credential_check_receipt_id: String,
    pub output_artifacts: Vec<String>,
    pub verification_evidence: Vec<String>,
    pub summary: String,
    pub next_owner: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkerRuntimeReport {
    pub worker_run_id: String,
    pub status: String,
    pub handoff_receipt_id: String,
    pub retirement_receipt_id: String,
    pub source_receipts: Vec<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProviderTurnOnEvidenceReceipt {
    pub substrate: &'static str,
    pub adapter: &'static str,
    pub receipt_kind: &'static str,
    pub status_before: &'static str,
    pub evidence_required: &'static str,
}
pub fn provider_turn_on_evidence_receipts() -> &'static [ProviderTurnOnEvidenceReceipt] {
    &PROVIDER_TURN_ON_EVIDENCE_RECEIPTS
}
pub fn postgres_provider_turn_on_evidence_receipt(
    observed_loops: usize,
    observed_ledger_receipts: usize,
    observed_chain_heads: usize,
    previous_hash: Option<String>,
) -> Receipt {
    let mut ids = IdFactory::default();
    let correlation = CorrelationIds {
        tenant_id: TenantId::new("tenant_local"),
        trace_id: TraceId::new("trace_postgres_live_transport"),
        actor_id: ActorId::new("postgres_storage"),
        loop_id: LoopId::new("postgres_live_transport"),
        workflow_id: WorkflowId::new("provider_turn_on"),
    };
    let kind = "postgres.receipt.write.observed".to_string();
    let mut payload = BTreeMap::new();
    payload.insert("substrate".to_string(), "postgres".to_string());
    payload.insert("adapter".to_string(), "PostgresStorage".to_string());
    payload.insert("observed_loops".to_string(), observed_loops.to_string());
    payload.insert(
        "observed_ledger_receipts".to_string(),
        observed_ledger_receipts.to_string(),
    );
    payload.insert(
        "observed_chain_heads".to_string(),
        observed_chain_heads.to_string(),
    );
    let receipt_id = format!("{}_{}", correlation.loop_id.as_str(), ids.next("receipt"));
    let receipt_timestamp = trusted_receipt_timestamp(observed_ledger_receipts + 1);
    let hash_version = RECEIPT_HASH_VERSION_TRUSTED_TIME;
    let hash = receipt_hash_for_version(ReceiptHashParts {
        receipt_id: &receipt_id,
        correlation: &correlation,
        kind: &kind,
        policy_decision_id: None,
        payload: &payload,
        previous_hash: previous_hash.as_deref(),
        receipt_timestamp: &receipt_timestamp,
        hash_version,
    });
    Receipt {
        receipt_id,
        tenant_id: correlation.tenant_id,
        trace_id: correlation.trace_id,
        actor_id: correlation.actor_id,
        loop_id: correlation.loop_id,
        workflow_id: correlation.workflow_id,
        kind,
        policy_decision_id: None,
        payload,
        previous_hash,
        receipt_timestamp,
        hash_version,
        hash,
    }
}

const PROVIDER_TURN_ON_EVIDENCE_RECEIPTS: [ProviderTurnOnEvidenceReceipt; 6] = [
    ProviderTurnOnEvidenceReceipt {
        substrate: "postgres",
        adapter: "PostgresStorage",
        receipt_kind: "postgres.receipt.write.observed",
        status_before: "PENDING-LIVE-RUN",
        evidence_required: "durable receipt write and chain-head update observed through PostgresStorage",
    },
    ProviderTurnOnEvidenceReceipt {
        substrate: "temporal",
        adapter: "TemporalLoopRunner",
        receipt_kind: "temporal.workflow.observed",
        status_before: "PENDING-LIVE-RUN",
        evidence_required: "durable workflow execution observed through TemporalLoopRunner",
    },
    ProviderTurnOnEvidenceReceipt {
        substrate: "tensorzero",
        adapter: "TensorZeroModelGateway",
        receipt_kind: "tensorzero.inference.observed",
        status_before: "PENDING-LIVE-RUN",
        evidence_required: "live model call observed through TensorZeroModelGateway",
    },
    ProviderTurnOnEvidenceReceipt {
        substrate: "mem0",
        adapter: "Mem0MemoryProvider",
        receipt_kind: "mem0.memory.write.observed",
        status_before: "PENDING-LIVE-RUN",
        evidence_required: "live memory write observed through the consolidation gate",
    },
    ProviderTurnOnEvidenceReceipt {
        substrate: "opentelemetry",
        adapter: "OpenTelemetryExporter",
        receipt_kind: "opentelemetry.trace.exported",
        status_before: "PENDING-LIVE-RUN",
        evidence_required: "live trace export observed through OpenTelemetryExporter",
    },
    ProviderTurnOnEvidenceReceipt {
        substrate: "render",
        adapter: "render.yaml",
        receipt_kind: "render.deployment.observed",
        status_before: "PENDING-LIVE-RUN",
        evidence_required: "human-approved deployment observed through the Render blueprint",
    },
];
#[derive(Debug, Default)]
pub struct Ledger {
    entries: Vec<Receipt>,
    /// How many leading entries have already passed hash-chain verification.
    /// `verify()` is called on every write path; re-hashing the whole chain
    /// each time made every write O(history). The watermark keeps the same
    /// guarantee for chain construction while verifying only new entries;
    /// restore boundaries reset it and re-verify from the first receipt.
    ///
    /// Atomic, not Cell: `verify()` takes `&self` and is called on read paths
    /// (operator packets, dogfood) as well as write paths. Once the kernel
    /// moves behind an RwLock, two readers can be inside `verify()` under a
    /// shared lock at once, so the watermark must be `Sync`. Relaxed ordering
    /// is sufficient: the watermark is a pure optimization that only moves
    /// forward during append-only construction and is reset under the
    /// exclusive write lock at a restore boundary; a racing reader at worst
    /// re-verifies a few already-valid entries, never skips an unverified one.
    verified_prefix: std::sync::atomic::AtomicUsize,
    /// kind -> positions in `entries`, in insertion order. Every projection
    /// answers a `by_kind` question, and a flat scan over the whole ledger is
    /// O(total receipts) per read - fine at thousands, a serialization tax at
    /// hundreds of concurrent streams over a growing chain. The index turns
    /// `by_kind` into O(matches), so a read holds the kernel lock for
    /// microseconds. Maintained at the only two mutation points (append,
    /// restore); always reflects `entries` exactly.
    kind_index: BTreeMap<String, Vec<usize>>,
    /// receipt_id -> position in `entries`, in insertion order.
    receipt_id_index: BTreeMap<String, usize>,
}
impl Ledger {
    pub fn append(
        &mut self,
        ids: &mut IdFactory,
        correlation: &CorrelationIds,
        kind: impl Into<String>,
        policy_decision_id: Option<String>,
        payload: BTreeMap<String, String>,
    ) -> Receipt {
        let kind = kind.into();
        let previous_hash = self.entries.last().map(|entry| entry.hash.clone());
        let receipt_id = format!("{}_{}", correlation.loop_id.as_str(), ids.next("receipt"));
        let receipt_timestamp = self.next_receipt_timestamp();
        let hash_version = RECEIPT_HASH_VERSION_TRUSTED_TIME;
        let hash = receipt_hash_for_version(ReceiptHashParts {
            receipt_id: &receipt_id,
            correlation,
            kind: &kind,
            policy_decision_id: policy_decision_id.as_deref(),
            payload: &payload,
            previous_hash: previous_hash.as_deref(),
            receipt_timestamp: &receipt_timestamp,
            hash_version,
        });
        let receipt = Receipt {
            receipt_id,
            tenant_id: correlation.tenant_id.clone(),
            trace_id: correlation.trace_id.clone(),
            actor_id: correlation.actor_id.clone(),
            loop_id: correlation.loop_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            kind,
            policy_decision_id,
            payload,
            previous_hash,
            receipt_timestamp,
            hash_version,
            hash,
        };
        let position = self.entries.len();
        self.kind_index
            .entry(receipt.kind.clone())
            .or_default()
            .push(position);
        self.receipt_id_index
            .insert(receipt.receipt_id.clone(), position);
        self.entries.push(receipt.clone());
        receipt
    }
    pub fn entries(&self) -> &[Receipt] {
        &self.entries
    }
    fn next_receipt_timestamp(&self) -> String {
        let wall_clock = wall_clock_receipt_timestamp();
        let Some(previous) = self.entries.last().and_then(non_empty_receipt_timestamp) else {
            return wall_clock;
        };
        if wall_clock.as_str() > previous {
            wall_clock
        } else {
            let deterministic_next = trusted_receipt_timestamp(self.entries.len() + 1);
            if deterministic_next.as_str() > previous {
                deterministic_next
            } else {
                previous.to_string()
            }
        }
    }
    fn rebuild_query_indexes(&mut self) {
        let mut kind_index: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        let mut receipt_id_index: BTreeMap<String, usize> = BTreeMap::new();
        for (position, entry) in self.entries.iter().enumerate() {
            kind_index
                .entry(entry.kind.clone())
                .or_default()
                .push(position);
            receipt_id_index.insert(entry.receipt_id.clone(), position);
        }
        self.kind_index = kind_index;
        self.receipt_id_index = receipt_id_index;
    }
    /// Install a previously journaled chain after verifying it end to end.
    /// A chain that does not verify is refused whole - a partial or tampered
    /// snapshot must never become the ledger.
    pub fn restore_entries(&mut self, entries: Vec<Receipt>) -> Result<usize, String> {
        let candidate = Ledger {
            entries,
            verified_prefix: std::sync::atomic::AtomicUsize::new(0),
            kind_index: BTreeMap::new(),
            receipt_id_index: BTreeMap::new(),
        };
        candidate.verify()?;
        let count = candidate.entries.len();
        self.entries = candidate.entries;
        self.rebuild_query_indexes();
        self.verified_prefix
            .store(count, std::sync::atomic::Ordering::Relaxed);
        Ok(count)
    }
    pub fn query(&self) -> ReceiptQuery<'_> {
        ReceiptQuery {
            entries: &self.entries,
            kind_index: &self.kind_index,
            receipt_id_index: &self.receipt_id_index,
        }
    }
    pub fn verify(&self) -> Result<(), String> {
        let start = self
            .verified_prefix
            .load(std::sync::atomic::Ordering::Relaxed)
            .min(self.entries.len());
        let mut previous_hash: Option<String> = if start == 0 {
            None
        } else {
            Some(self.entries[start - 1].hash.clone())
        };
        let mut previous_receipt_timestamp: Option<&str> = if start == 0 {
            None
        } else {
            non_empty_receipt_timestamp(&self.entries[start - 1])
        };
        for entry in &self.entries[start..] {
            if entry.previous_hash != previous_hash {
                return Err(format!(
                    "receipt {} has broken previous hash",
                    entry.receipt_id
                ));
            }
            if entry.hash_version >= RECEIPT_HASH_VERSION_TRUSTED_TIME {
                if entry.receipt_timestamp.is_empty() {
                    return Err(format!(
                        "receipt {} has trusted-time hash version but no receipt_timestamp",
                        entry.receipt_id
                    ));
                }
                if let Some(previous_timestamp) = previous_receipt_timestamp
                    && entry.receipt_timestamp.as_str() < previous_timestamp
                {
                    return Err(format!(
                        "receipt {} has a receipt_timestamp before the previous receipt",
                        entry.receipt_id
                    ));
                }
            }
            let correlation = CorrelationIds {
                tenant_id: entry.tenant_id.clone(),
                trace_id: entry.trace_id.clone(),
                actor_id: entry.actor_id.clone(),
                loop_id: entry.loop_id.clone(),
                workflow_id: entry.workflow_id.clone(),
            };
            let expected = receipt_hash_for_version(ReceiptHashParts {
                receipt_id: &entry.receipt_id,
                correlation: &correlation,
                kind: &entry.kind,
                policy_decision_id: entry.policy_decision_id.as_deref(),
                payload: &entry.payload,
                previous_hash: entry.previous_hash.as_deref(),
                receipt_timestamp: &entry.receipt_timestamp,
                hash_version: entry.hash_version,
            });
            if entry.hash != expected {
                return Err(format!("receipt {} has invalid hash", entry.receipt_id));
            }
            previous_hash = Some(entry.hash.clone());
            previous_receipt_timestamp = non_empty_receipt_timestamp(entry);
        }
        self.verified_prefix
            .store(self.entries.len(), std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    /// Re-verify the whole chain from the first receipt, ignoring the
    /// watermark. Restore boundaries and audits use this; the per-write
    /// `verify()` covers chain construction incrementally.
    pub fn verify_full(&self) -> Result<(), String> {
        self.verified_prefix
            .store(0, std::sync::atomic::Ordering::Relaxed);
        self.verify()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ReceiptQuery<'a> {
    entries: &'a [Receipt],
    kind_index: &'a BTreeMap<String, Vec<usize>>,
    receipt_id_index: &'a BTreeMap<String, usize>,
}
impl<'a> ReceiptQuery<'a> {
    pub fn count(&self) -> usize {
        self.entries.len()
    }
    pub fn by_id(&self, receipt_id: &str) -> Option<&'a Receipt> {
        self.receipt_id_index
            .get(receipt_id)
            .map(|&position| &self.entries[position])
    }
    /// O(matches) via the kind index, in insertion order - the same result a
    /// flat scan would give, without touching the whole ledger.
    pub fn by_kind(&self, kind: &str) -> Vec<&'a Receipt> {
        match self.kind_index.get(kind) {
            Some(positions) => positions.iter().map(|&i| &self.entries[i]).collect(),
            None => Vec::new(),
        }
    }
    pub fn by_policy_decision(&self, policy_decision_id: &str) -> Vec<&'a Receipt> {
        self.entries
            .iter()
            .filter(|entry| entry.policy_decision_id.as_deref() == Some(policy_decision_id))
            .collect()
    }
    pub fn receipt_ids(&self) -> Vec<&'a str> {
        self.entries
            .iter()
            .map(|entry| entry.receipt_id.as_str())
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceiptEvidence {
    pub receipt_ids: Vec<String>,
}
impl ReceiptEvidence {
    pub fn from_query(query: ReceiptQuery<'_>) -> Self {
        Self {
            receipt_ids: query
                .receipt_ids()
                .into_iter()
                .map(str::to_string)
                .collect(),
        }
    }
    pub fn source_list(&self) -> String {
        self.receipt_ids.join(", ")
    }
}

pub fn render_postgres_ledger_export_sql(receipts: &[Receipt], loop_id: &str) -> String {
    let mut sql = String::new();
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
            sql_string_literal("agent")
        ));
    }
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
    sql.push_str(&format!(
        "SELECT count(*) FROM ledger_entries WHERE loop_id = {};\n",
        sql_string_literal(loop_id)
    ));
    sql
}

fn render_postgres_receipt_insert_sql(receipt: &Receipt) -> String {
    format!(
        "INSERT INTO ledger_entries (receipt_id, tenant_id, trace_id, actor_id, loop_id, workflow_id, kind, policy_decision_id, payload, previous_hash, receipt_timestamp, hash_version, hash) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}::jsonb, {}, {}, {}, {}) ON CONFLICT (receipt_id) DO NOTHING;\n",
        sql_string_literal(&receipt.receipt_id),
        sql_string_literal(receipt.tenant_id.as_str()),
        sql_string_literal(receipt.trace_id.as_str()),
        sql_string_literal(receipt.actor_id.as_str()),
        sql_string_literal(receipt.loop_id.as_str()),
        sql_string_literal(receipt.workflow_id.as_str()),
        sql_string_literal(&receipt.kind),
        sql_optional_string(receipt.policy_decision_id.as_deref()),
        sql_string_literal(&render_payload_json(&receipt.payload)),
        sql_optional_string(receipt.previous_hash.as_deref()),
        sql_string_literal(&receipt.receipt_timestamp),
        receipt.hash_version,
        sql_string_literal(&receipt.hash)
    )
}
fn render_postgres_chain_head_upsert_sql(receipt: &Receipt) -> String {
    format!(
        "INSERT INTO ledger_chain_heads (tenant_id, head_hash, receipt_id) VALUES ({}, {}, {}) ON CONFLICT (tenant_id) DO UPDATE SET head_hash = EXCLUDED.head_hash, receipt_id = EXCLUDED.receipt_id, updated_at = now();\n",
        sql_string_literal(receipt.tenant_id.as_str()),
        sql_string_literal(&receipt.hash),
        sql_string_literal(&receipt.receipt_id)
    )
}
fn render_payload_json(payload: &BTreeMap<String, String>) -> String {
    let values = payload
        .iter()
        .map(|(key, value)| {
            format!(
                "{}: {}",
                json_string_literal(key),
                json_string_literal(value)
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("{{{values}}}")
}
fn sql_optional_string(value: Option<&str>) -> String {
    value
        .map(sql_string_literal)
        .unwrap_or_else(|| "NULL".to_string())
}
fn sql_string_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
struct ReceiptHashParts<'a> {
    receipt_id: &'a str,
    correlation: &'a CorrelationIds,
    kind: &'a str,
    policy_decision_id: Option<&'a str>,
    payload: &'a BTreeMap<String, String>,
    previous_hash: Option<&'a str>,
    receipt_timestamp: &'a str,
    hash_version: u64,
}

fn receipt_hash_for_version(parts: ReceiptHashParts<'_>) -> String {
    if parts.hash_version < RECEIPT_HASH_VERSION_TRUSTED_TIME {
        return receipt_hash_v1(
            parts.receipt_id,
            parts.correlation,
            parts.kind,
            parts.policy_decision_id,
            parts.payload,
            parts.previous_hash,
        );
    }
    let mut input = String::new();
    input.push_str(&parts.hash_version.to_string());
    input.push('|');
    input.push_str(parts.receipt_timestamp);
    input.push('|');
    input.push_str(parts.receipt_id);
    input.push('|');
    input.push_str(parts.correlation.tenant_id.as_str());
    input.push('|');
    input.push_str(parts.correlation.trace_id.as_str());
    input.push('|');
    input.push_str(parts.correlation.actor_id.as_str());
    input.push('|');
    input.push_str(parts.correlation.loop_id.as_str());
    input.push('|');
    input.push_str(parts.correlation.workflow_id.as_str());
    input.push('|');
    input.push_str(parts.kind);
    input.push('|');
    input.push_str(parts.policy_decision_id.unwrap_or(""));
    input.push('|');
    input.push_str(parts.previous_hash.unwrap_or(""));
    for (key, value) in parts.payload {
        input.push('|');
        input.push_str(key);
        input.push('=');
        input.push_str(value);
    }
    format!("{:016x}", fnv1a64(input.as_bytes()))
}
fn receipt_hash_v1(
    receipt_id: &str,
    correlation: &CorrelationIds,
    kind: &str,
    policy_decision_id: Option<&str>,
    payload: &BTreeMap<String, String>,
    previous_hash: Option<&str>,
) -> String {
    let mut input = String::new();
    input.push_str(receipt_id);
    input.push('|');
    input.push_str(correlation.tenant_id.as_str());
    input.push('|');
    input.push_str(correlation.trace_id.as_str());
    input.push('|');
    input.push_str(correlation.actor_id.as_str());
    input.push('|');
    input.push_str(correlation.loop_id.as_str());
    input.push('|');
    input.push_str(correlation.workflow_id.as_str());
    input.push('|');
    input.push_str(kind);
    input.push('|');
    input.push_str(policy_decision_id.unwrap_or(""));
    input.push('|');
    input.push_str(previous_hash.unwrap_or(""));
    for (key, value) in payload {
        input.push('|');
        input.push_str(key);
        input.push('=');
        input.push_str(value);
    }
    format!("{:016x}", fnv1a64(input.as_bytes()))
}
pub fn trusted_receipt_timestamp(sequence: usize) -> String {
    let seconds = 1_767_225_600_u64 + sequence.max(1) as u64 - 1;
    format_receipt_timestamp_from_unix_seconds(seconds)
}
fn wall_clock_receipt_timestamp() -> String {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format_receipt_timestamp_from_unix_parts(duration.as_secs(), duration.subsec_nanos())
}
fn format_receipt_timestamp_from_unix_seconds(seconds: u64) -> String {
    format_receipt_timestamp_from_unix_parts(seconds, 0)
}
fn format_receipt_timestamp_from_unix_parts(seconds: u64, nanos: u32) -> String {
    let days = (seconds / 86_400) as i64;
    let second_of_day = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = second_of_day / 3_600;
    let minute = (second_of_day % 3_600) / 60;
    let second = second_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{nanos:09}Z")
}
fn civil_from_days(days_since_unix_epoch: i64) -> (i64, u64, u64) {
    let z = days_since_unix_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    (year, month as u64, day as u64)
}
fn non_empty_receipt_timestamp(receipt: &Receipt) -> Option<&str> {
    if receipt.receipt_timestamp.is_empty() {
        None
    } else {
        Some(receipt.receipt_timestamp.as_str())
    }
}
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PolicyOutcome {
    Allow,
    Escalate,
    Deny,
}
impl PolicyOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Allow => "ALLOW",
            Self::Escalate => "ESCALATE",
            Self::Deny => "DENY",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyDecision {
    pub policy_decision_id: String,
    pub tenant_id: TenantId,
    pub actor_id: ActorId,
    pub action: ActionKind,
    pub outcome: PolicyOutcome,
    pub reason: String,
    pub receipt_id: Option<String>,
}

#[derive(Debug, Default)]
pub struct ConstraintEvaluator;

impl ConstraintEvaluator {
    pub fn evaluate(
        &self,
        ids: &mut IdFactory,
        correlation: &CorrelationIds,
        action: ActionKind,
    ) -> PolicyDecision {
        let (outcome, reason) = match action {
            ActionKind::EconomicSpend { amount_cents } if amount_cents > 0 => (
                PolicyOutcome::Escalate,
                "economic actions require transaction-scoped Treasury authority".to_string(),
            ),
            _ => (
                PolicyOutcome::Allow,
                format!("{} is inside local guided autonomy ceiling", action.name()),
            ),
        };
        PolicyDecision {
            policy_decision_id: ids.next("policy"),
            tenant_id: correlation.tenant_id.clone(),
            actor_id: correlation.actor_id.clone(),
            action,
            outcome,
            reason,
            receipt_id: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreasuryAuthorization {
    pub treasury_authorization_id: String,
    pub tenant_id: TenantId,
    pub actor_id: ActorId,
    pub policy_decision_id: String,
    pub max_amount_cents: u32,
    pub counterparty: String,
    pub purpose: String,
    pub status: String,
    pub receipt_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvalVerdict {
    pub eval_verdict_id: String,
    pub tenant_id: TenantId,
    pub actor_id: ActorId,
    pub suite_id: String,
    pub passed: bool,
    pub score: u8,
    pub trace_receipt_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentCredential {
    pub credential_id: String,
    pub tenant_id: TenantId,
    pub actor_id: ActorId,
    pub eval_verdict_id: String,
    pub status: String,
    pub receipt_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutboxEvent {
    pub event_id: String,
    pub tenant_id: TenantId,
    pub source_receipt_id: String,
    pub topic: String,
    pub payload: BTreeMap<String, String>,
    pub delivered: bool,
}

impl OutboxEvent {
    pub fn from_receipt(
        ids: &mut IdFactory,
        receipt: &Receipt,
        topic: impl Into<String>,
        payload: BTreeMap<String, String>,
    ) -> Self {
        Self {
            event_id: ids.next("outbox"),
            tenant_id: receipt.tenant_id.clone(),
            source_receipt_id: receipt.receipt_id.clone(),
            topic: topic.into(),
            payload,
            delivered: false,
        }
    }
}

pub trait OutboxProvider {
    fn enqueue(&mut self, event: OutboxEvent);
    fn events(&self) -> &[OutboxEvent];
    fn mark_delivered(&mut self, event_id: &str) -> Result<(), String>;
}

#[derive(Debug, Default)]
pub struct InMemoryOutbox {
    events: Vec<OutboxEvent>,
}

impl OutboxProvider for InMemoryOutbox {
    fn enqueue(&mut self, event: OutboxEvent) {
        self.events.push(event);
    }

    fn events(&self) -> &[OutboxEvent] {
        &self.events
    }

    fn mark_delivered(&mut self, event_id: &str) -> Result<(), String> {
        let event = self
            .events
            .iter_mut()
            .find(|event| event.event_id == event_id)
            .ok_or_else(|| format!("outbox event {event_id} not found"))?;
        event.delivered = true;
        Ok(())
    }
}

pub mod outbox {
    pub use super::{InMemoryOutbox, OutboxEvent, OutboxProvider};
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceEvent {
    pub tenant_id: TenantId,
    pub trace_id: TraceId,
    pub actor_id: ActorId,
    pub loop_id: LoopId,
    pub workflow_id: WorkflowId,
    pub name: String,
}

impl TraceEvent {
    pub fn from_correlation(correlation: &CorrelationIds, name: impl Into<String>) -> Self {
        Self {
            tenant_id: correlation.tenant_id.clone(),
            trace_id: correlation.trace_id.clone(),
            actor_id: correlation.actor_id.clone(),
            loop_id: correlation.loop_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            name: name.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObservabilityAdapterError {
    MissingOtelEndpoint,
    PendingLiveRun {
        adapter: &'static str,
        reason: &'static str,
    },
}

impl fmt::Display for ObservabilityAdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingOtelEndpoint => f.write_str("OTEL_EXPORTER_OTLP_ENDPOINT is required"),
            Self::PendingLiveRun { adapter, reason } => {
                write!(f, "{adapter} is PENDING-LIVE-RUN: {reason}")
            }
        }
    }
}

pub trait TraceExporter {
    fn exporter_name(&self) -> &'static str;
    fn export(&mut self, event: TraceEvent) -> Result<(), ObservabilityAdapterError>;
    fn events(&self) -> &[TraceEvent];
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LocalTraceExporter {
    events: Vec<TraceEvent>,
}

impl TraceExporter for LocalTraceExporter {
    fn exporter_name(&self) -> &'static str {
        "LocalTraceExporter"
    }

    fn export(&mut self, event: TraceEvent) -> Result<(), ObservabilityAdapterError> {
        self.events.push(event);
        Ok(())
    }

    fn events(&self) -> &[TraceEvent] {
        &self.events
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenTelemetryExporter {
    endpoint: String,
}

impl OpenTelemetryExporter {
    pub fn connect(endpoint: Option<&str>) -> Result<Self, ObservabilityAdapterError> {
        let endpoint = endpoint
            .filter(|value| !value.trim().is_empty())
            .ok_or(ObservabilityAdapterError::MissingOtelEndpoint)?;
        let _candidate = Self {
            endpoint: endpoint.to_string(),
        };
        Err(ObservabilityAdapterError::PendingLiveRun {
            adapter: "OpenTelemetryExporter",
            reason: "live trace export has not been observed",
        })
    }

    pub fn adapter_name() -> &'static str {
        "OpenTelemetryExporter"
    }
}

impl TraceExporter for OpenTelemetryExporter {
    fn exporter_name(&self) -> &'static str {
        Self::adapter_name()
    }

    fn export(&mut self, _event: TraceEvent) -> Result<(), ObservabilityAdapterError> {
        Err(ObservabilityAdapterError::PendingLiveRun {
            adapter: Self::adapter_name(),
            reason: "live trace export has not been observed",
        })
    }

    fn events(&self) -> &[TraceEvent] {
        &[]
    }
}
pub mod observability {
    pub use super::{
        LocalTraceExporter, ObservabilityAdapterError, OpenTelemetryExporter, TraceEvent,
        TraceExporter,
    };
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubstrateStatus {
    pub substrate: &'static str,
    pub local_adapter: &'static str,
    pub live_adapter: &'static str,
    pub status: &'static str,
    pub turn_on_signal: &'static str,
    pub blocking_rails: &'static [&'static str],
    pub first_local_proof: &'static str,
}

pub fn live_substrate_statuses() -> &'static [SubstrateStatus] {
    &LIVE_SUBSTRATE_STATUSES
}

pub fn local_status_json(migration_count: usize) -> String {
    let substrates = live_substrate_statuses()
        .iter()
        .map(|status| {
            format!(
                r#"    {{ "substrate": {}, "local_adapter": {}, "live_adapter": {}, "status": {}, "turn_on_signal": {}, "blocking_rails": [{}], "first_local_proof": {} }}"#,
                json_string_literal(status.substrate),
                json_string_literal(status.local_adapter),
                json_string_literal(status.live_adapter),
                json_string_literal(status.status),
                json_string_literal(status.turn_on_signal),
                json_static_array(status.blocking_rails),
                json_string_literal(status.first_local_proof)
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    format!(
        r#"{{
  "name": "mdx-native local",
  "migrations": {migration_count},
  "mode": "deterministic-local",
  "live_substrate": [
{substrates}
  ]
}}"#
    )
}

const LIVE_SUBSTRATE_STATUSES: [SubstrateStatus; 6] = [
    SubstrateStatus {
        substrate: "postgres",
        local_adapter: "PostgresStorage when DATABASE_URL and observed migrations are supplied",
        live_adapter: "PostgresStorage",
        status: "LIVE-LOCAL",
        turn_on_signal: "production Postgres connection approval remains before production live",
        blocking_rails: &[
            "production-database-url",
            "production-credential-approval",
            "production-backup-policy",
        ],
        first_local_proof: "DATABASE_URL=postgres://mdx:mdx@localhost:5432/mdx make local-full-check",
    },
    SubstrateStatus {
        substrate: "temporal",
        local_adapter: "LocalLoopRunner",
        live_adapter: "TemporalLoopRunner",
        status: "PENDING-LIVE-RUN",
        turn_on_signal: "durable workflow execution is observed",
        blocking_rails: &[
            "loop-runner-boundary",
            "workflow-observed",
            "provider-turn-on-evidence",
        ],
        first_local_proof: "make live-turn-on-check",
    },
    SubstrateStatus {
        substrate: "tensorzero",
        local_adapter: "DeterministicModelGateway",
        live_adapter: "TensorZeroModelGateway",
        status: "PENDING-LIVE-RUN",
        turn_on_signal: "live model gateway calls are observed",
        blocking_rails: &[
            "model-gateway-boundary",
            "inference-observed",
            "provider-turn-on-evidence",
        ],
        first_local_proof: "make live-turn-on-check",
    },
    SubstrateStatus {
        substrate: "mem0",
        local_adapter: "InMemoryProvider",
        live_adapter: "Mem0MemoryProvider",
        status: "PENDING-LIVE-RUN",
        turn_on_signal: "live memory writes pass through the consolidation gate",
        blocking_rails: &[
            "memory-provider-boundary",
            "consolidation-gate",
            "provider-turn-on-evidence",
        ],
        first_local_proof: "make live-turn-on-check",
    },
    SubstrateStatus {
        substrate: "opentelemetry",
        local_adapter: "LocalTraceExporter",
        live_adapter: "OpenTelemetryExporter",
        status: "PENDING-LIVE-RUN",
        turn_on_signal: "live trace export is observed",
        blocking_rails: &[
            "observability-exporter-boundary",
            "trace-export-observed",
            "provider-turn-on-evidence",
        ],
        first_local_proof: "make live-turn-on-check",
    },
    SubstrateStatus {
        substrate: "render",
        local_adapter: "local process",
        live_adapter: "render.yaml",
        status: "PENDING-LIVE-RUN",
        turn_on_signal: "human-approved Render deployment succeeds",
        blocking_rails: &[
            "render-blueprint",
            "human-approval",
            "provider-turn-on-evidence",
        ],
        first_local_proof: "make live-turn-on-check",
    },
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CharterRecord {
    pub charter_record_id: String,
    pub tenant_id: TenantId,
    pub source_receipt_id: String,
    pub obligation: String,
    pub evidence: String,
    pub receipt_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoopTransition {
    pub transition_id: String,
    pub run_id: String,
    pub transition: String,
    pub policy_decision_id: String,
    pub receipt_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoopRun {
    pub run_id: String,
    pub loop_id: LoopId,
    pub agent_id: ActorId,
    pub workflow_id: WorkflowId,
    pub status: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoopRunReport {
    pub run_id: String,
    pub loop_id: String,
    pub status: String,
    pub score: u8,
    pub credential_status: String,
    pub receipts: Vec<String>,
    pub concierge_answer: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoopRunSummary {
    pub loop_id: String,
    pub status: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservatoryReadModel {
    pub surface: &'static str,
    pub latest_run: Option<LoopRunSummary>,
    pub receipt_evidence: ReceiptEvidence,
    pub receipt_count: usize,
    pub policy_decision_count: usize,
    pub eval_verdict_count: usize,
    pub credential_count: usize,
    pub charter_record_count: usize,
    pub memory_record_count: usize,
    pub role_modes: Vec<&'static str>,
    pub declared_sources: Vec<&'static str>,
}

impl ObservatoryReadModel {
    pub fn latest_run_label(&self) -> String {
        self.latest_run
            .as_ref()
            .map(|run| format!("{} {}", run.loop_id, run.status))
            .unwrap_or_else(|| "no loop runs yet".to_string())
    }

    pub fn render_text(&self) -> String {
        format!(
            "Observatory\nsurface: {}\nlatest_run: {}\nreceipts: {}\npolicy_decisions: {}\neval_verdicts: {}\ncredentials: {}\ncharter_records: {}\nmemory_records: {}\nroles: {}\nsources: {}\n",
            self.surface,
            self.latest_run_label(),
            self.receipt_count,
            self.policy_decision_count,
            self.eval_verdict_count,
            self.credential_count,
            self.charter_record_count,
            self.memory_record_count,
            self.role_modes.join(", "),
            self.declared_sources.join(", ")
        )
    }

    pub fn render_json(&self) -> String {
        let latest_run = self
            .latest_run
            .as_ref()
            .map(|run| {
                format!(
                    r#"{{ "loop_id": {}, "status": {} }}"#,
                    json_string_literal(&run.loop_id),
                    json_string_literal(&run.status)
                )
            })
            .unwrap_or_else(|| "null".to_string());
        format!(
            r#"{{
  "surface": {},
  "latest_run": {},
  "receipt_ids": [{}],
  "receipt_count": {},
  "policy_decision_count": {},
  "eval_verdict_count": {},
  "credential_count": {},
  "charter_record_count": {},
  "memory_record_count": {},
  "role_modes": [{}],
  "declared_sources": [{}]
}}"#,
            json_string_literal(self.surface),
            latest_run,
            json_owned_array(&self.receipt_evidence.receipt_ids),
            self.receipt_count,
            self.policy_decision_count,
            self.eval_verdict_count,
            self.credential_count,
            self.charter_record_count,
            self.memory_record_count,
            json_static_array(&self.role_modes),
            json_static_array(&self.declared_sources)
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConciergeReadModel {
    pub suite_id: Option<String>,
    pub score: Option<u8>,
    pub credential_id: Option<String>,
    pub credential_status: Option<String>,
    pub receipt_evidence: ReceiptEvidence,
}

impl ConciergeReadModel {
    pub fn render_text(&self) -> String {
        match (
            self.suite_id.as_deref(),
            self.score,
            self.credential_status.as_deref(),
            self.credential_id.as_deref(),
        ) {
            (Some(suite_id), Some(score), Some(status), Some(credential_id)) => format!(
                "evals_runner_agent ran {suite_id}, scored {score}, and {status} credential {credential_id} because policy allowed each consequential step. Source receipts: {}",
                self.receipt_evidence.source_list()
            ),
            _ => "No receipt-backed loop run has been recorded yet.".to_string(),
        }
    }

    pub fn render_json(&self) -> String {
        let score = self
            .score
            .map(|score| score.to_string())
            .unwrap_or_else(|| "null".to_string());
        format!(
            r#"{{
  "suite_id": {},
  "score": {},
  "credential_id": {},
  "credential_status": {},
  "receipt_ids": [{}]
}}"#,
            json_optional_string(self.suite_id.as_deref()),
            score,
            json_optional_string(self.credential_id.as_deref()),
            json_optional_string(self.credential_status.as_deref()),
            json_owned_array(&self.receipt_evidence.receipt_ids)
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TwinTextResponse {
    pub companion_id: &'static str,
    pub role: &'static str,
    pub runtime_status: &'static str,
    pub answer: String,
    pub receipt_evidence: ReceiptEvidence,
    pub world_model_sources: Vec<&'static str>,
}

impl TwinTextResponse {
    pub fn render_text(&self) -> String {
        format!(
            "{} remains {} and can only answer from declared evidence. {} Source receipts: {}",
            self.companion_id,
            self.runtime_status,
            self.answer,
            self.receipt_evidence.source_list()
        )
    }

    pub fn render_json(&self) -> String {
        format!(
            r#"{{
  "companion_id": {},
  "role": {},
  "runtime_status": {},
  "answer": {},
  "receipt_ids": [{}],
  "world_model_sources": [{}]
}}"#,
            json_string_literal(self.companion_id),
            json_string_literal(self.role),
            json_string_literal(self.runtime_status),
            json_string_literal(&self.answer),
            json_owned_array(&self.receipt_evidence.receipt_ids),
            json_static_array(&self.world_model_sources)
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StrategyRatificationResponse {
    pub proposal_id: &'static str,
    pub runtime_status: &'static str,
    pub question: &'static str,
    pub options: Vec<&'static str>,
    pub why_now: String,
    pub blocked_actions: Vec<&'static str>,
    pub ratification_required: bool,
    pub receipt_evidence: ReceiptEvidence,
    pub source_contracts: Vec<&'static str>,
}

impl StrategyRatificationResponse {
    pub fn render_text(&self) -> String {
        format!(
            "{} remains {}. {} Ratification required: {}. Blocked actions: {}. Source receipts: {}",
            self.proposal_id,
            self.runtime_status,
            self.question,
            self.ratification_required,
            self.blocked_actions.join(", "),
            self.receipt_evidence.source_list()
        )
    }

    pub fn render_json(&self) -> String {
        format!(
            r#"{{
  "proposal_id": {},
  "runtime_status": {},
  "question": {},
  "options": [{}],
  "why_now": {},
  "blocked_actions": [{}],
  "ratification_required": {},
  "receipt_ids": [{}],
  "source_contracts": [{}]
}}"#,
            json_string_literal(self.proposal_id),
            json_string_literal(self.runtime_status),
            json_string_literal(self.question),
            json_static_array(&self.options),
            json_string_literal(&self.why_now),
            json_static_array(&self.blocked_actions),
            self.ratification_required,
            json_owned_array(&self.receipt_evidence.receipt_ids),
            json_static_array(&self.source_contracts)
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProductRatificationResponse {
    pub bet_id: &'static str,
    pub runtime_status: &'static str,
    pub question: &'static str,
    pub shaped_bet: String,
    pub required_before_ratification: Vec<&'static str>,
    pub blocked_actions: Vec<&'static str>,
    pub ratification_required: bool,
    pub receipt_evidence: ReceiptEvidence,
    pub source_contracts: Vec<&'static str>,
}

impl ProductRatificationResponse {
    pub fn render_text(&self) -> String {
        format!(
            "{} remains {}. {} Ratification required: {}. Blocked actions: {}. Source receipts: {}",
            self.bet_id,
            self.runtime_status,
            self.question,
            self.ratification_required,
            self.blocked_actions.join(", "),
            self.receipt_evidence.source_list()
        )
    }

    pub fn render_json(&self) -> String {
        format!(
            r#"{{
  "bet_id": {},
  "runtime_status": {},
  "question": {},
  "shaped_bet": {},
  "required_before_ratification": [{}],
  "blocked_actions": [{}],
  "ratification_required": {},
  "receipt_ids": [{}],
  "source_contracts": [{}]
}}"#,
            json_string_literal(self.bet_id),
            json_string_literal(self.runtime_status),
            json_string_literal(self.question),
            json_string_literal(&self.shaped_bet),
            json_static_array(&self.required_before_ratification),
            json_static_array(&self.blocked_actions),
            self.ratification_required,
            json_owned_array(&self.receipt_evidence.receipt_ids),
            json_static_array(&self.source_contracts)
        )
    }
}

pub fn json_string_literal(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            ch if ch.is_control() => escaped.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => escaped.push(ch),
        }
    }
    escaped.push('"');
    escaped
}

fn json_optional_string(value: Option<&str>) -> String {
    value
        .map(json_string_literal)
        .unwrap_or_else(|| "null".to_string())
}

fn json_static_array(values: &[&str]) -> String {
    values
        .iter()
        .map(|value| json_string_literal(value))
        .collect::<Vec<_>>()
        .join(", ")
}

fn json_owned_array(values: &[String]) -> String {
    values
        .iter()
        .map(|value| json_string_literal(value))
        .collect::<Vec<_>>()
        .join(", ")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoleMode {
    Leader,
    Regulator,
    Risk,
    Advisor,
    Member,
    Engineer,
    Operator,
    Product,
}

impl RoleMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Leader => "Leader",
            Self::Regulator => "Regulator",
            Self::Risk => "Risk",
            Self::Advisor => "Advisor",
            Self::Member => "Member",
            Self::Engineer => "Engineer",
            Self::Operator => "Operator",
            Self::Product => "Product",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeclaredSource {
    LedgerEntries,
    LoopRuns,
    PolicyDecisions,
    EvalVerdicts,
    AgentCredentials,
    CharterRecords,
    MemoryRecords,
    PagesProjections,
    MessageProjections,
}

impl DeclaredSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::LedgerEntries => "ledger_entries",
            Self::LoopRuns => "loop_runs",
            Self::PolicyDecisions => "policy_decisions",
            Self::EvalVerdicts => "eval_verdicts",
            Self::AgentCredentials => "agent_credentials",
            Self::CharterRecords => "charter_records",
            Self::MemoryRecords => "memory_records",
            Self::PagesProjections => "pages_projections",
            Self::MessageProjections => "message_projections",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoleViewDeclaration {
    pub surface: &'static str,
    pub role_modes: &'static [RoleMode],
    pub declared_sources: &'static [DeclaredSource],
}

impl RoleViewDeclaration {
    pub fn role_modes_csv(&self) -> String {
        self.role_modes
            .iter()
            .map(RoleMode::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn declared_sources_csv(&self) -> String {
        self.declared_sources
            .iter()
            .map(DeclaredSource::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

pub static OBSERVATORY_ROLE_MODES: [RoleMode; 8] = [
    RoleMode::Leader,
    RoleMode::Regulator,
    RoleMode::Risk,
    RoleMode::Advisor,
    RoleMode::Member,
    RoleMode::Engineer,
    RoleMode::Operator,
    RoleMode::Product,
];

pub static OBSERVATORY_DECLARED_SOURCES: [DeclaredSource; 7] = [
    DeclaredSource::LedgerEntries,
    DeclaredSource::LoopRuns,
    DeclaredSource::PolicyDecisions,
    DeclaredSource::EvalVerdicts,
    DeclaredSource::AgentCredentials,
    DeclaredSource::CharterRecords,
    DeclaredSource::MemoryRecords,
];

pub static OBSERVATORY_ROLE_VIEW: RoleViewDeclaration = RoleViewDeclaration {
    surface: "observatory",
    role_modes: &OBSERVATORY_ROLE_MODES,
    declared_sources: &OBSERVATORY_DECLARED_SOURCES,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadSurfaceDeclaration {
    pub surface: &'static str,
    pub route: &'static str,
    pub read_model: &'static str,
    pub response_schema_path: &'static str,
    pub declared_sources: &'static [DeclaredSource],
    pub receipt_evidence_required: bool,
}

pub static CONCIERGE_DECLARED_SOURCES: [DeclaredSource; 3] = [
    DeclaredSource::LedgerEntries,
    DeclaredSource::EvalVerdicts,
    DeclaredSource::AgentCredentials,
];

pub static PAGES_DECLARED_SOURCES: [DeclaredSource; 2] = [
    DeclaredSource::LedgerEntries,
    DeclaredSource::PagesProjections,
];

pub static MESSAGE_DECLARED_SOURCES: [DeclaredSource; 2] = [
    DeclaredSource::LedgerEntries,
    DeclaredSource::MessageProjections,
];

pub static LOCAL_READ_SURFACES: [ReadSurfaceDeclaration; 4] = [
    ReadSurfaceDeclaration {
        surface: "observatory",
        route: "/observatory",
        read_model: "ObservatoryReadModel",
        response_schema_path: "generated/response-schemas/observatory-read-model.schema.json",
        declared_sources: &OBSERVATORY_DECLARED_SOURCES,
        receipt_evidence_required: true,
    },
    ReadSurfaceDeclaration {
        surface: "concierge",
        route: "/concierge",
        read_model: "ConciergeReadModel",
        response_schema_path: "generated/response-schemas/concierge-read-model.schema.json",
        declared_sources: &CONCIERGE_DECLARED_SOURCES,
        receipt_evidence_required: true,
    },
    ReadSurfaceDeclaration {
        surface: "pages",
        route: "/pages.json",
        read_model: "PagesProjectionReadModel",
        response_schema_path: "generated/response-schemas/pages-projection-list-response.schema.json",
        declared_sources: &PAGES_DECLARED_SOURCES,
        receipt_evidence_required: true,
    },
    ReadSurfaceDeclaration {
        surface: "message",
        route: "/messages/threads.json",
        read_model: "MessageThreadProjectionReadModel",
        response_schema_path: "generated/response-schemas/message-thread-list-response.schema.json",
        declared_sources: &MESSAGE_DECLARED_SOURCES,
        receipt_evidence_required: true,
    },
];

pub fn local_read_surfaces() -> &'static [ReadSurfaceDeclaration] {
    &LOCAL_READ_SURFACES
}

#[derive(Debug, Default)]
pub struct InMemoryStorage {
    ledger: Ledger,
    policy_decisions: Vec<PolicyDecision>,
    treasury_authorizations: Vec<TreasuryAuthorization>,
    eval_verdicts: Vec<EvalVerdict>,
    credentials: Vec<AgentCredential>,
    memory: InMemoryProvider,
    memory_graph_nodes: Vec<MemoryGraphNode>,
    memory_graph_edges: Vec<MemoryGraphEdge>,
    memory_lifecycle_events: Vec<MemoryLifecycleEvent>,
    memory_recall_rankings: Vec<MemoryRecallRanking>,
    memory_brain_eval_runs: Vec<MemoryBrainEvalRun>,
    memory_vendor_comparator_runs: Vec<MemoryVendorComparatorRun>,
    memory_surface_access: Vec<MemorySurfaceAccess>,
    memory_production_topology_checks: Vec<MemoryProductionTopologyCheck>,
    memory_lifecycle_evaluations: Vec<MemoryLifecycleEvaluation>,
    memory_eval_fixture_results: Vec<MemoryEvalFixtureResult>,
    memory_topology_runtime_events: Vec<MemoryTopologyRuntimeEvent>,
    memory_benchmark_imports: Vec<MemoryBenchmarkImport>,
    memory_scale_load_runs: Vec<MemoryScaleLoadRun>,
    memory_cloud_turn_on_checks: Vec<MemoryCloudTurnOnCheck>,
    outbox: InMemoryOutbox,
    charter_records: Vec<CharterRecord>,
    loop_runs: Vec<LoopRun>,
    loop_transitions: Vec<LoopTransition>,
}

pub trait StorageProvider {
    fn ledger(&self) -> &Ledger;
    fn ledger_mut(&mut self) -> &mut Ledger;
    fn append_receipt(
        &mut self,
        ids: &mut IdFactory,
        correlation: &CorrelationIds,
        kind: impl Into<String>,
        policy_decision_id: Option<String>,
        payload: BTreeMap<String, String>,
    ) -> Receipt {
        self.ledger_mut()
            .append(ids, correlation, kind, policy_decision_id, payload)
    }
    fn policy_decisions(&self) -> &[PolicyDecision];
    fn treasury_authorizations(&self) -> &[TreasuryAuthorization];
    fn eval_verdicts(&self) -> &[EvalVerdict];
    fn credentials(&self) -> &[AgentCredential];
    fn memory_records(&self) -> &[MemoryRecord];
    fn memory_graph_nodes(&self) -> &[MemoryGraphNode];
    fn memory_graph_edges(&self) -> &[MemoryGraphEdge];
    fn memory_lifecycle_events(&self) -> &[MemoryLifecycleEvent];
    fn memory_recall_rankings(&self) -> &[MemoryRecallRanking];
    fn memory_brain_eval_runs(&self) -> &[MemoryBrainEvalRun];
    fn memory_vendor_comparator_runs(&self) -> &[MemoryVendorComparatorRun];
    fn memory_surface_access(&self) -> &[MemorySurfaceAccess];
    fn memory_production_topology_checks(&self) -> &[MemoryProductionTopologyCheck];
    fn memory_lifecycle_evaluations(&self) -> &[MemoryLifecycleEvaluation];
    fn memory_eval_fixture_results(&self) -> &[MemoryEvalFixtureResult];
    fn memory_topology_runtime_events(&self) -> &[MemoryTopologyRuntimeEvent];
    fn memory_benchmark_imports(&self) -> &[MemoryBenchmarkImport];
    fn memory_scale_load_runs(&self) -> &[MemoryScaleLoadRun];
    fn memory_cloud_turn_on_checks(&self) -> &[MemoryCloudTurnOnCheck];
    fn restore_memory_brain_snapshot(&mut self, snapshot: MemoryBrainSnapshot);
    fn outbox_events(&self) -> &[OutboxEvent];
    fn charter_records(&self) -> &[CharterRecord];
    fn loop_runs(&self) -> &[LoopRun];
    fn loop_runs_mut(&mut self) -> &mut Vec<LoopRun>;
    fn loop_transitions(&self) -> &[LoopTransition];
    fn push_policy_decision(&mut self, decision: PolicyDecision);
    fn push_treasury_authorization(&mut self, authorization: TreasuryAuthorization);
    fn push_eval_verdict(&mut self, verdict: EvalVerdict);
    fn push_credential(&mut self, credential: AgentCredential);
    fn write_memory(&mut self, record: MemoryRecord);
    fn push_memory_graph_node(&mut self, node: MemoryGraphNode);
    fn push_memory_graph_edge(&mut self, edge: MemoryGraphEdge);
    fn push_memory_lifecycle_event(&mut self, event: MemoryLifecycleEvent);
    fn push_memory_recall_ranking(&mut self, ranking: MemoryRecallRanking);
    fn push_memory_brain_eval_run(&mut self, run: MemoryBrainEvalRun);
    fn push_memory_vendor_comparator_run(&mut self, run: MemoryVendorComparatorRun);
    fn push_memory_surface_access(&mut self, access: MemorySurfaceAccess);
    fn push_memory_production_topology_check(&mut self, check: MemoryProductionTopologyCheck);
    fn push_memory_lifecycle_evaluation(&mut self, evaluation: MemoryLifecycleEvaluation);
    fn push_memory_eval_fixture_result(&mut self, result: MemoryEvalFixtureResult);
    fn push_memory_topology_runtime_event(&mut self, event: MemoryTopologyRuntimeEvent);
    fn push_memory_benchmark_import(&mut self, import: MemoryBenchmarkImport);
    fn push_memory_scale_load_run(&mut self, run: MemoryScaleLoadRun);
    fn push_memory_cloud_turn_on_check(&mut self, check: MemoryCloudTurnOnCheck);
    fn set_memory_lifecycle_state(&mut self, memory_id: &str, lifecycle_state: &'static str);
    fn set_memory_consolidation_state(
        &mut self,
        memory_id: &str,
        consolidation_state: &'static str,
    );
    fn set_memory_embedding(&mut self, memory_id: &str, embedding: String);
    fn close_memory_record_validity(
        &mut self,
        memory_id: &str,
        valid_until_receipt_timestamp: &str,
        invalidated_by_receipt_id: &str,
    );
    fn enqueue_outbox(&mut self, event: OutboxEvent);
    fn mark_outbox_delivered(&mut self, event_id: &str) -> Result<(), String>;
    fn push_charter_record(&mut self, record: CharterRecord);
    fn push_loop_run(&mut self, run: LoopRun);
    fn push_loop_transition(&mut self, transition: LoopTransition);
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StorageAdapterError {
    MissingDatabaseUrl,
    MigrationEvidenceMismatch {
        expected_migrations: usize,
        observed_migrations: usize,
        expected_tenant_owned_tables: usize,
        observed_tenant_owned_tables: usize,
        expected_rls_enabled_tables: usize,
        observed_rls_enabled_tables: usize,
    },
    PendingLiveRun {
        adapter: &'static str,
        reason: &'static str,
    },
    LiveTransport {
        adapter: &'static str,
        message: String,
    },
}

impl fmt::Display for StorageAdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingDatabaseUrl => f.write_str("DATABASE_URL is required"),
            Self::MigrationEvidenceMismatch {
                expected_migrations,
                observed_migrations,
                expected_tenant_owned_tables,
                observed_tenant_owned_tables,
                expected_rls_enabled_tables,
                observed_rls_enabled_tables,
            } => write!(
                f,
                "Postgres migration evidence mismatch: migrations {observed_migrations}/{expected_migrations}, tenant tables {observed_tenant_owned_tables}/{expected_tenant_owned_tables}, rls tables {observed_rls_enabled_tables}/{expected_rls_enabled_tables}"
            ),
            Self::PendingLiveRun { adapter, reason } => {
                write!(f, "{adapter} is PENDING-LIVE-RUN: {reason}")
            }
            Self::LiveTransport { adapter, message } => {
                write!(f, "{adapter} live transport failed: {message}")
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PostgresMigrationEvidence {
    pub migration_count: usize,
    pub tenant_owned_tables: usize,
    pub rls_enabled_tables: usize,
    pub observed_by: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PostgresLoopExportEvidence {
    pub migration_count: usize,
    pub loop_receipt_counts: Vec<LoopReceiptCountEvidence>,
    pub observed_by: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoopReceiptCountEvidence {
    pub loop_id: String,
    pub ledger_receipts: usize,
}

impl PostgresLoopExportEvidence {
    pub fn from_local_loop_budgets(migration_count: usize, observed_by: impl Into<String>) -> Self {
        Self {
            migration_count,
            loop_receipt_counts: local_loop_budgets()
                .iter()
                .map(|budget| LoopReceiptCountEvidence {
                    loop_id: budget.loop_id.to_string(),
                    ledger_receipts: budget.max_receipts,
                })
                .collect(),
            observed_by: observed_by.into(),
        }
    }

    pub fn observed_receipts_for_loop(&self, loop_id: &str) -> Option<usize> {
        self.loop_receipt_counts
            .iter()
            .find(|count| count.loop_id == loop_id)
            .map(|count| count.ledger_receipts)
    }

    pub fn matches_local_loop_budgets(&self) -> bool {
        self.loop_receipt_counts.len() == local_loop_budgets().len()
            && local_loop_budgets().iter().all(|budget| {
                self.observed_receipts_for_loop(budget.loop_id) == Some(budget.max_receipts)
            })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PostgresStorage {
    database_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PostgresReceiptWriter {
    database_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PostgresLiveWriteReport {
    pub loop_id: String,
    pub ledger_receipts: usize,
    pub chain_heads: usize,
    pub provider_receipt_kind: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PostgresReceiptWriterContract {
    pub adapter: &'static str,
    pub database_url_required: bool,
    pub observed_loop_export_required: bool,
    pub ledger_table: &'static str,
    pub chain_head_table: &'static str,
    pub receipt_count_by_loop_required: bool,
}

impl PostgresStorage {
    pub fn connect(database_url: Option<&str>) -> Result<Self, StorageAdapterError> {
        let database_url = database_url
            .filter(|value| !value.trim().is_empty())
            .ok_or(StorageAdapterError::MissingDatabaseUrl)?;
        let _candidate = Self {
            database_url: database_url.to_string(),
        };
        Err(StorageAdapterError::PendingLiveRun {
            adapter: "PostgresStorage",
            reason: "migrations have not been applied against a running Postgres server",
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
        "PostgresStorage"
    }

    pub fn database_url(&self) -> &str {
        &self.database_url
    }

    pub fn render_receipt_write_sql(&self, receipts: &[Receipt], loop_id: &str) -> String {
        render_postgres_ledger_export_sql(receipts, loop_id)
    }

    pub fn write_receipts_live(
        &self,
        receipts: &[Receipt],
        loop_id: &str,
    ) -> Result<PostgresLiveWriteReport, StorageAdapterError> {
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
        sql.push_str(&self.render_receipt_write_sql(receipts, loop_id));
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
        let chain_heads = receipts
            .iter()
            .map(|receipt| receipt.tenant_id.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        Ok(PostgresLiveWriteReport {
            loop_id: loop_id.to_string(),
            ledger_receipts: receipts.len(),
            chain_heads,
            provider_receipt_kind: "postgres.receipt.write.observed",
        })
    }

    fn transport_error(error: impl fmt::Display) -> StorageAdapterError {
        Self::transport_message(&error.to_string())
    }

    fn transport_message(message: &str) -> StorageAdapterError {
        StorageAdapterError::LiveTransport {
            adapter: "PostgresStorage",
            message: message.trim().to_string(),
        }
    }
}

impl PostgresReceiptWriter {
    pub fn connect(database_url: Option<&str>) -> Result<Self, StorageAdapterError> {
        let database_url = database_url
            .filter(|value| !value.trim().is_empty())
            .ok_or(StorageAdapterError::MissingDatabaseUrl)?;
        let _candidate = Self {
            database_url: database_url.to_string(),
        };
        Err(StorageAdapterError::PendingLiveRun {
            adapter: "PostgresReceiptWriter",
            reason: "durable receipt writes have not been observed through PostgresStorage",
        })
    }

    pub fn connect_after_observed_loop_export(
        database_url: Option<&str>,
        evidence: PostgresLoopExportEvidence,
    ) -> Result<Self, StorageAdapterError> {
        let database_url = database_url
            .filter(|value| !value.trim().is_empty())
            .ok_or(StorageAdapterError::MissingDatabaseUrl)?;
        let expected = migration_report();
        if evidence.migration_count != expected.migration_count
            || !evidence.matches_local_loop_budgets()
        {
            return Err(StorageAdapterError::PendingLiveRun {
                adapter: "PostgresReceiptWriter",
                reason: "local loop ledger export evidence does not match expected receipt counts",
            });
        }
        Ok(Self {
            database_url: database_url.to_string(),
        })
    }

    pub fn adapter_name() -> &'static str {
        "PostgresReceiptWriter"
    }

    pub fn database_url(&self) -> &str {
        &self.database_url
    }

    pub fn contract() -> PostgresReceiptWriterContract {
        PostgresReceiptWriterContract {
            adapter: Self::adapter_name(),
            database_url_required: true,
            observed_loop_export_required: true,
            ledger_table: "ledger_entries",
            chain_head_table: "ledger_chain_heads",
            receipt_count_by_loop_required: true,
        }
    }
}

pub mod storage {
    pub use super::{
        PostgresLiveWriteReport, PostgresLoopExportEvidence, PostgresMigrationEvidence,
        PostgresReceiptWriter, PostgresReceiptWriterContract, PostgresStorage, StorageAdapterError,
        StorageProvider,
    };

    pub mod memory {
        pub use super::super::InMemoryStorage;
    }

    pub mod postgres {
        pub use super::super::{
            PostgresLiveWriteReport, PostgresReceiptWriter, PostgresStorage, StorageAdapterError,
        };
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoopRunnerAdapterError {
    MissingTemporalAddress,
    PendingLiveRun {
        adapter: &'static str,
        reason: &'static str,
    },
}

impl fmt::Display for LoopRunnerAdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingTemporalAddress => f.write_str("TEMPORAL_ADDRESS is required"),
            Self::PendingLiveRun { adapter, reason } => {
                write!(f, "{adapter} is PENDING-LIVE-RUN: {reason}")
            }
        }
    }
}

pub trait LoopRunner<S: StorageProvider> {
    fn runner_name(&self) -> &'static str;
    fn run_evals_runner_agent(&self, kernel: &mut MdxKernel<S>) -> Result<LoopRunReport, String>;
    fn run_aegis_scanner_agent(&self, kernel: &mut MdxKernel<S>) -> Result<LoopRunReport, String>;
    fn run_charter_attestation_agent(
        &self,
        kernel: &mut MdxKernel<S>,
    ) -> Result<LoopRunReport, String>;
    fn run_forge_orchestrator_agent(
        &self,
        kernel: &mut MdxKernel<S>,
    ) -> Result<LoopRunReport, String>;
    fn run_product_shaping_agent(&self, kernel: &mut MdxKernel<S>)
    -> Result<LoopRunReport, String>;
    fn run_talent_autonomy_agent(&self, kernel: &mut MdxKernel<S>)
    -> Result<LoopRunReport, String>;
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LocalLoopRunner;

impl<S: StorageProvider> LoopRunner<S> for LocalLoopRunner {
    fn runner_name(&self) -> &'static str {
        "LocalLoopRunner"
    }

    fn run_evals_runner_agent(&self, kernel: &mut MdxKernel<S>) -> Result<LoopRunReport, String> {
        kernel.run_evals_runner_agent()
    }

    fn run_aegis_scanner_agent(&self, kernel: &mut MdxKernel<S>) -> Result<LoopRunReport, String> {
        kernel.run_aegis_scanner_agent()
    }

    fn run_charter_attestation_agent(
        &self,
        kernel: &mut MdxKernel<S>,
    ) -> Result<LoopRunReport, String> {
        kernel.run_charter_attestation_agent()
    }

    fn run_forge_orchestrator_agent(
        &self,
        kernel: &mut MdxKernel<S>,
    ) -> Result<LoopRunReport, String> {
        kernel.run_forge_orchestrator_agent()
    }

    fn run_product_shaping_agent(
        &self,
        kernel: &mut MdxKernel<S>,
    ) -> Result<LoopRunReport, String> {
        kernel.run_product_shaping_agent()
    }

    fn run_talent_autonomy_agent(
        &self,
        kernel: &mut MdxKernel<S>,
    ) -> Result<LoopRunReport, String> {
        kernel.run_talent_autonomy_agent()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemporalLoopRunner {
    address: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemporalWorkflowContract {
    pub workflow_type: &'static str,
    pub task_queue: &'static str,
    pub namespace_required: bool,
    pub retry_policy_required: bool,
    pub activity_timeout_seconds: u32,
    pub receipt_kind: &'static str,
}

impl TemporalLoopRunner {
    pub fn connect(address: Option<&str>) -> Result<Self, LoopRunnerAdapterError> {
        let address = address
            .filter(|value| !value.trim().is_empty())
            .ok_or(LoopRunnerAdapterError::MissingTemporalAddress)?;
        let _candidate = Self {
            address: address.to_string(),
        };
        Err(LoopRunnerAdapterError::PendingLiveRun {
            adapter: "TemporalLoopRunner",
            reason: "durable Temporal workflow execution has not been observed",
        })
    }

    pub fn adapter_name() -> &'static str {
        "TemporalLoopRunner"
    }

    pub fn evals_runner_contract() -> TemporalWorkflowContract {
        TemporalWorkflowContract {
            workflow_type: "evals_runner_agent",
            task_queue: "mdx-local-loop-runners",
            namespace_required: true,
            retry_policy_required: true,
            activity_timeout_seconds: 30,
            receipt_kind: "loop.triggered",
        }
    }
}

pub mod loop_runtime {
    pub use super::{
        LocalLoopRunner, LoopRunner, LoopRunnerAdapterError, TemporalLoopRunner,
        TemporalWorkflowContract,
    };
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelGatewayTrace {
    pub gateway: &'static str,
    pub suite_id: String,
    pub inference_id: String,
    pub variant: String,
    pub routing_strategy: String,
    pub stream_contract: String,
    pub stream_event_count: u32,
    pub terminal_event: String,
    pub fallback_strategy: String,
    pub fallback_provider: String,
    pub first_byte_latency_ms: u32,
    pub failover_slo_ms: u32,
    pub cases: u32,
    pub score: u8,
    pub passed: bool,
}

pub trait ModelGateway {
    fn gateway_name(&self) -> &'static str;
    fn run_eval_suite(&self, suite_id: &str) -> Result<ModelGatewayTrace, String>;
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeterministicModelGateway;

impl ModelGateway for DeterministicModelGateway {
    fn gateway_name(&self) -> &'static str {
        "deterministic_stub"
    }

    fn run_eval_suite(&self, suite_id: &str) -> Result<ModelGatewayTrace, String> {
        Ok(ModelGatewayTrace {
            gateway: self.gateway_name(),
            suite_id: suite_id.to_string(),
            inference_id: format!("local_inference_{suite_id}"),
            variant: "deterministic_local_v1".to_string(),
            routing_strategy: "single_deterministic_stub".to_string(),
            stream_contract: "normalized_model_stream_v1".to_string(),
            stream_event_count: 4,
            terminal_event: "model_stream_completed".to_string(),
            fallback_strategy: "local_first_fail_closed".to_string(),
            fallback_provider: "none_local_single_provider".to_string(),
            first_byte_latency_ms: 1,
            failover_slo_ms: 3000,
            cases: 3,
            score: 100,
            passed: true,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelGatewayAdapterError {
    MissingTensorZeroGatewayUrl,
    PendingLiveRun {
        adapter: &'static str,
        reason: &'static str,
    },
}

impl fmt::Display for ModelGatewayAdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingTensorZeroGatewayUrl => f.write_str("TENSORZERO_GATEWAY_URL is required"),
            Self::PendingLiveRun { adapter, reason } => {
                write!(f, "{adapter} is PENDING-LIVE-RUN: {reason}")
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TensorZeroModelGateway {
    gateway_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TensorZeroGatewayContract {
    pub provider: &'static str,
    pub gateway_url_required: bool,
    pub observability_required: bool,
    pub feedback_required: bool,
    pub fallback_required: bool,
    pub receipt_kind: &'static str,
}

impl TensorZeroModelGateway {
    pub fn connect(gateway_url: Option<&str>) -> Result<Self, ModelGatewayAdapterError> {
        let gateway_url = gateway_url
            .filter(|value| !value.trim().is_empty())
            .ok_or(ModelGatewayAdapterError::MissingTensorZeroGatewayUrl)?;
        let _candidate = Self {
            gateway_url: gateway_url.to_string(),
        };
        Err(ModelGatewayAdapterError::PendingLiveRun {
            adapter: "TensorZeroModelGateway",
            reason: "live model gateway calls have not been observed",
        })
    }

    pub fn adapter_name() -> &'static str {
        "TensorZeroModelGateway"
    }

    pub fn contract() -> TensorZeroGatewayContract {
        TensorZeroGatewayContract {
            provider: "TensorZero",
            gateway_url_required: true,
            observability_required: true,
            feedback_required: true,
            fallback_required: true,
            receipt_kind: "eval.suite.ran",
        }
    }
}

pub mod model_gateway {
    pub use super::{
        DeterministicModelGateway, ModelGateway, ModelGatewayAdapterError, ModelGatewayTrace,
        TensorZeroGatewayContract, TensorZeroModelGateway,
    };
}

impl StorageProvider for InMemoryStorage {
    fn ledger(&self) -> &Ledger {
        &self.ledger
    }

    fn ledger_mut(&mut self) -> &mut Ledger {
        &mut self.ledger
    }

    fn policy_decisions(&self) -> &[PolicyDecision] {
        &self.policy_decisions
    }

    fn treasury_authorizations(&self) -> &[TreasuryAuthorization] {
        &self.treasury_authorizations
    }

    fn eval_verdicts(&self) -> &[EvalVerdict] {
        &self.eval_verdicts
    }

    fn credentials(&self) -> &[AgentCredential] {
        &self.credentials
    }

    fn memory_records(&self) -> &[MemoryRecord] {
        self.memory.records()
    }

    fn memory_graph_nodes(&self) -> &[MemoryGraphNode] {
        &self.memory_graph_nodes
    }

    fn memory_graph_edges(&self) -> &[MemoryGraphEdge] {
        &self.memory_graph_edges
    }

    fn memory_lifecycle_events(&self) -> &[MemoryLifecycleEvent] {
        &self.memory_lifecycle_events
    }

    fn memory_recall_rankings(&self) -> &[MemoryRecallRanking] {
        &self.memory_recall_rankings
    }

    fn memory_brain_eval_runs(&self) -> &[MemoryBrainEvalRun] {
        &self.memory_brain_eval_runs
    }

    fn memory_vendor_comparator_runs(&self) -> &[MemoryVendorComparatorRun] {
        &self.memory_vendor_comparator_runs
    }

    fn memory_surface_access(&self) -> &[MemorySurfaceAccess] {
        &self.memory_surface_access
    }

    fn memory_production_topology_checks(&self) -> &[MemoryProductionTopologyCheck] {
        &self.memory_production_topology_checks
    }

    fn memory_lifecycle_evaluations(&self) -> &[MemoryLifecycleEvaluation] {
        &self.memory_lifecycle_evaluations
    }

    fn memory_eval_fixture_results(&self) -> &[MemoryEvalFixtureResult] {
        &self.memory_eval_fixture_results
    }

    fn memory_topology_runtime_events(&self) -> &[MemoryTopologyRuntimeEvent] {
        &self.memory_topology_runtime_events
    }

    fn memory_benchmark_imports(&self) -> &[MemoryBenchmarkImport] {
        &self.memory_benchmark_imports
    }

    fn memory_scale_load_runs(&self) -> &[MemoryScaleLoadRun] {
        &self.memory_scale_load_runs
    }

    fn memory_cloud_turn_on_checks(&self) -> &[MemoryCloudTurnOnCheck] {
        &self.memory_cloud_turn_on_checks
    }

    fn restore_memory_brain_snapshot(&mut self, snapshot: MemoryBrainSnapshot) {
        self.memory.restore_records(snapshot.records);
        self.memory_graph_nodes = snapshot.graph_nodes;
        self.memory_graph_edges = snapshot.graph_edges;
        self.memory_lifecycle_events = snapshot.lifecycle_events;
        self.memory_recall_rankings = snapshot.recall_rankings;
        self.memory_brain_eval_runs = snapshot.eval_runs;
        self.memory_vendor_comparator_runs = snapshot.vendor_comparator_runs;
        self.memory_surface_access = snapshot.surface_access;
        self.memory_production_topology_checks = snapshot.production_topology_checks;
        self.memory_lifecycle_evaluations = snapshot.lifecycle_evaluations;
        self.memory_eval_fixture_results = snapshot.eval_fixture_results;
        self.memory_topology_runtime_events = snapshot.topology_runtime_events;
        self.memory_benchmark_imports = snapshot.benchmark_imports;
        self.memory_scale_load_runs = snapshot.scale_load_runs;
        self.memory_cloud_turn_on_checks = snapshot.cloud_turn_on_checks;
    }

    fn outbox_events(&self) -> &[OutboxEvent] {
        self.outbox.events()
    }

    fn charter_records(&self) -> &[CharterRecord] {
        &self.charter_records
    }

    fn loop_runs(&self) -> &[LoopRun] {
        &self.loop_runs
    }

    fn loop_runs_mut(&mut self) -> &mut Vec<LoopRun> {
        &mut self.loop_runs
    }

    fn loop_transitions(&self) -> &[LoopTransition] {
        &self.loop_transitions
    }

    fn push_policy_decision(&mut self, decision: PolicyDecision) {
        self.policy_decisions.push(decision);
    }

    fn push_treasury_authorization(&mut self, authorization: TreasuryAuthorization) {
        self.treasury_authorizations.push(authorization);
    }

    fn push_eval_verdict(&mut self, verdict: EvalVerdict) {
        self.eval_verdicts.push(verdict);
    }

    fn push_credential(&mut self, credential: AgentCredential) {
        self.credentials.push(credential);
    }

    fn write_memory(&mut self, record: MemoryRecord) {
        self.memory.write(record);
    }

    fn push_memory_graph_node(&mut self, node: MemoryGraphNode) {
        self.memory_graph_nodes.push(node);
    }

    fn push_memory_graph_edge(&mut self, edge: MemoryGraphEdge) {
        self.memory_graph_edges.push(edge);
    }

    fn push_memory_lifecycle_event(&mut self, event: MemoryLifecycleEvent) {
        self.memory_lifecycle_events.push(event);
    }

    fn push_memory_recall_ranking(&mut self, ranking: MemoryRecallRanking) {
        self.memory_recall_rankings.push(ranking);
    }

    fn push_memory_brain_eval_run(&mut self, run: MemoryBrainEvalRun) {
        self.memory_brain_eval_runs.push(run);
    }

    fn push_memory_vendor_comparator_run(&mut self, run: MemoryVendorComparatorRun) {
        self.memory_vendor_comparator_runs.push(run);
    }

    fn push_memory_surface_access(&mut self, access: MemorySurfaceAccess) {
        self.memory_surface_access.push(access);
    }

    fn push_memory_production_topology_check(&mut self, check: MemoryProductionTopologyCheck) {
        self.memory_production_topology_checks.push(check);
    }

    fn push_memory_lifecycle_evaluation(&mut self, evaluation: MemoryLifecycleEvaluation) {
        self.memory_lifecycle_evaluations.push(evaluation);
    }

    fn push_memory_eval_fixture_result(&mut self, result: MemoryEvalFixtureResult) {
        self.memory_eval_fixture_results.push(result);
    }

    fn push_memory_topology_runtime_event(&mut self, event: MemoryTopologyRuntimeEvent) {
        self.memory_topology_runtime_events.push(event);
    }

    fn push_memory_benchmark_import(&mut self, import: MemoryBenchmarkImport) {
        self.memory_benchmark_imports.push(import);
    }

    fn push_memory_scale_load_run(&mut self, run: MemoryScaleLoadRun) {
        self.memory_scale_load_runs.push(run);
    }

    fn push_memory_cloud_turn_on_check(&mut self, check: MemoryCloudTurnOnCheck) {
        self.memory_cloud_turn_on_checks.push(check);
    }

    fn set_memory_lifecycle_state(&mut self, memory_id: &str, lifecycle_state: &'static str) {
        for node in &mut self.memory_graph_nodes {
            if node.memory_id.as_deref() == Some(memory_id) {
                node.lifecycle_state = lifecycle_state;
            }
        }
    }

    fn set_memory_consolidation_state(
        &mut self,
        memory_id: &str,
        consolidation_state: &'static str,
    ) {
        self.memory
            .set_consolidation_state(memory_id, consolidation_state);
    }

    fn set_memory_embedding(&mut self, memory_id: &str, embedding: String) {
        self.memory.set_embedding(memory_id, embedding);
    }

    fn close_memory_record_validity(
        &mut self,
        memory_id: &str,
        valid_until_receipt_timestamp: &str,
        invalidated_by_receipt_id: &str,
    ) {
        self.memory.close_record_validity(
            memory_id,
            valid_until_receipt_timestamp,
            invalidated_by_receipt_id,
        );
    }

    fn enqueue_outbox(&mut self, event: OutboxEvent) {
        self.outbox.enqueue(event);
    }

    fn mark_outbox_delivered(&mut self, event_id: &str) -> Result<(), String> {
        self.outbox.mark_delivered(event_id)
    }

    fn push_charter_record(&mut self, record: CharterRecord) {
        self.charter_records.push(record);
    }

    fn push_loop_run(&mut self, run: LoopRun) {
        self.loop_runs.push(run);
    }

    fn push_loop_transition(&mut self, transition: LoopTransition) {
        self.loop_transitions.push(transition);
    }
}

#[derive(Debug, Default)]
pub struct MdxKernel<S: StorageProvider = InMemoryStorage> {
    ids: IdFactory,
    policy: ConstraintEvaluator,
    storage: S,
}

impl MdxKernel<InMemoryStorage> {
    pub fn boot_local() -> Self {
        Self::default()
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn with_storage(storage: S) -> Self {
        Self {
            ids: IdFactory::default(),
            policy: ConstraintEvaluator,
            storage,
        }
    }

    pub fn ledger(&self) -> &Ledger {
        self.storage.ledger()
    }

    /// The id counter, for durable snapshots taken alongside the ledger.
    pub fn ids_counter(&self) -> u64 {
        self.ids.counter()
    }

    /// Mint one prefixed id from the kernel's monotonic counter. The server
    /// uses this to name a long-running job (a forge run) before the job
    /// starts emitting its own receipts, so the caller can watch it.
    pub fn mint_id(&mut self, prefix: &str) -> String {
        self.ids.next(prefix)
    }

    /// Restore the id counter from a durable snapshot, never backwards.
    pub fn restore_ids_counter(&mut self, value: u64) {
        self.ids.restore_counter(value);
    }

    /// Restore the receipt ledger from a durable snapshot. The chain is
    /// verified end to end before it is installed; a snapshot that does not
    /// verify is refused whole and the current ledger is untouched.
    pub fn restore_ledger_entries(&mut self, entries: Vec<Receipt>) -> Result<usize, String> {
        self.storage.ledger_mut().restore_entries(entries)
    }

    pub fn policy_decisions(&self) -> &[PolicyDecision] {
        self.storage.policy_decisions()
    }

    pub fn treasury_authorizations(&self) -> &[TreasuryAuthorization] {
        self.storage.treasury_authorizations()
    }

    pub fn eval_verdicts(&self) -> &[EvalVerdict] {
        self.storage.eval_verdicts()
    }

    pub fn credentials(&self) -> &[AgentCredential] {
        self.storage.credentials()
    }

    pub fn memory_records(&self) -> &[MemoryRecord] {
        self.storage.memory_records()
    }

    pub fn memory_graph_nodes(&self) -> &[MemoryGraphNode] {
        self.storage.memory_graph_nodes()
    }

    pub fn memory_graph_edges(&self) -> &[MemoryGraphEdge] {
        self.storage.memory_graph_edges()
    }

    pub fn memory_lifecycle_events(&self) -> &[MemoryLifecycleEvent] {
        self.storage.memory_lifecycle_events()
    }

    pub fn memory_recall_rankings(&self) -> &[MemoryRecallRanking] {
        self.storage.memory_recall_rankings()
    }

    pub fn memory_brain_eval_runs(&self) -> &[MemoryBrainEvalRun] {
        self.storage.memory_brain_eval_runs()
    }

    pub fn memory_vendor_comparator_runs(&self) -> &[MemoryVendorComparatorRun] {
        self.storage.memory_vendor_comparator_runs()
    }

    pub fn memory_surface_access(&self) -> &[MemorySurfaceAccess] {
        self.storage.memory_surface_access()
    }

    pub fn memory_production_topology_checks(&self) -> &[MemoryProductionTopologyCheck] {
        self.storage.memory_production_topology_checks()
    }

    pub fn memory_lifecycle_evaluations(&self) -> &[MemoryLifecycleEvaluation] {
        self.storage.memory_lifecycle_evaluations()
    }

    pub fn memory_eval_fixture_results(&self) -> &[MemoryEvalFixtureResult] {
        self.storage.memory_eval_fixture_results()
    }

    pub fn memory_topology_runtime_events(&self) -> &[MemoryTopologyRuntimeEvent] {
        self.storage.memory_topology_runtime_events()
    }

    pub fn memory_benchmark_imports(&self) -> &[MemoryBenchmarkImport] {
        self.storage.memory_benchmark_imports()
    }

    pub fn memory_scale_load_runs(&self) -> &[MemoryScaleLoadRun] {
        self.storage.memory_scale_load_runs()
    }

    pub fn memory_cloud_turn_on_checks(&self) -> &[MemoryCloudTurnOnCheck] {
        self.storage.memory_cloud_turn_on_checks()
    }

    pub fn memory_brain_snapshot(&self) -> MemoryBrainSnapshot {
        MemoryBrainSnapshot {
            records: self.storage.memory_records().to_vec(),
            graph_nodes: self.storage.memory_graph_nodes().to_vec(),
            graph_edges: self.storage.memory_graph_edges().to_vec(),
            lifecycle_events: self.storage.memory_lifecycle_events().to_vec(),
            recall_rankings: self.storage.memory_recall_rankings().to_vec(),
            eval_runs: self.storage.memory_brain_eval_runs().to_vec(),
            vendor_comparator_runs: self.storage.memory_vendor_comparator_runs().to_vec(),
            surface_access: self.storage.memory_surface_access().to_vec(),
            production_topology_checks: self.storage.memory_production_topology_checks().to_vec(),
            lifecycle_evaluations: self.storage.memory_lifecycle_evaluations().to_vec(),
            eval_fixture_results: self.storage.memory_eval_fixture_results().to_vec(),
            topology_runtime_events: self.storage.memory_topology_runtime_events().to_vec(),
            benchmark_imports: self.storage.memory_benchmark_imports().to_vec(),
            scale_load_runs: self.storage.memory_scale_load_runs().to_vec(),
            cloud_turn_on_checks: self.storage.memory_cloud_turn_on_checks().to_vec(),
        }
    }

    pub fn restore_memory_brain_snapshot(
        &mut self,
        snapshot: MemoryBrainSnapshot,
    ) -> Result<usize, String> {
        for record in &snapshot.records {
            if self
                .storage
                .ledger()
                .query()
                .by_id(&record.source_receipt_id)
                .is_none()
            {
                return Err(format!(
                    "memory snapshot source receipt {} missing",
                    record.source_receipt_id
                ));
            }
            if !record.invalidated_by_receipt_id.is_empty()
                && self
                    .storage
                    .ledger()
                    .query()
                    .by_id(&record.invalidated_by_receipt_id)
                    .is_none()
            {
                return Err(format!(
                    "memory snapshot invalidating receipt {} missing",
                    record.invalidated_by_receipt_id
                ));
            }
        }
        let count = snapshot.record_count();
        self.storage.restore_memory_brain_snapshot(snapshot);
        Ok(count)
    }

    pub fn outbox_events(&self) -> &[OutboxEvent] {
        self.storage.outbox_events()
    }

    pub fn charter_records(&self) -> &[CharterRecord] {
        self.storage.charter_records()
    }

    pub fn loop_runs(&self) -> &[LoopRun] {
        self.storage.loop_runs()
    }

    pub fn loop_transitions(&self) -> &[LoopTransition] {
        self.storage.loop_transitions()
    }

    pub fn run_harness_plan_only(
        &mut self,
        manifest: &HarnessRunManifest<'_>,
    ) -> Result<HarnessPlanRunReport, String> {
        admit_harness_run_manifest(manifest).map_err(|rejection| rejection.message())?;
        if manifest.mode != HarnessRunMode::PlanOnly {
            return Err("local harness plan runner requires plan_only mode".to_string());
        }

        let starting_receipts = self.storage.ledger().entries().len();
        let starting_policy_decisions = self.storage.policy_decisions().len();
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(manifest.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(manifest.actor_id),
            loop_id: LoopId::new("harness_plan_runner"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let harness_run_id = self.ids.next("harness_run");
        self.storage.push_loop_run(LoopRun {
            run_id: harness_run_id.clone(),
            loop_id: correlation.loop_id.clone(),
            agent_id: correlation.actor_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            status: "RUNNING".to_string(),
        });

        let quality_gate_count = manifest.quality_gates.len().to_string();
        let exit_criteria_count = manifest.exit_criteria.len().to_string();
        let input_artifact_count = manifest.input_artifacts.len().to_string();
        let default_parallelism = manifest.default_parallelism.to_string();
        let approval_receipt_id = manifest
            .trust_boundary
            .approval_receipt_id
            .unwrap_or("none");
        let policy_profile = manifest.policy_profile.unwrap_or("core");
        let enterprise_pack = manifest.enterprise_pack.unwrap_or("none");

        let admission_decision =
            self.decide_with_receipt(&correlation, ActionKind::AdmitHarnessRun);
        let admission_receipt = self.transition_receipt(
            &harness_run_id,
            "ADMISSION",
            &correlation,
            &admission_decision,
            "harness.run.admitted",
            payload(&[
                ("manifest_run_id", manifest.run_id),
                ("origin_surface", manifest.origin_surface),
                ("mode", manifest.mode.as_str()),
                ("policy_profile", policy_profile),
                ("enterprise_pack", enterprise_pack),
                ("approval_mode", manifest.approval_mode.as_str()),
                ("approval_receipt_id", approval_receipt_id),
                ("default_parallelism", &default_parallelism),
            ]),
        );

        let plan_decision = self.decide_with_receipt(&correlation, ActionKind::EmitHarnessPlan);
        let plan_item_id = self.ids.next("plan_item");
        let plan_summary = format!("Plan only: {}", manifest.goal_summary);
        let plan_command = format!("satisfy: {}", manifest.definition_of_done);
        let plan_receipt = self.transition_receipt(
            &harness_run_id,
            "PLAN",
            &correlation,
            &plan_decision,
            "harness.plan.emitted",
            payload(&[
                ("manifest_run_id", manifest.run_id),
                ("plan_item_id", &plan_item_id),
                ("goal", manifest.goal_summary),
                ("definition_of_done", manifest.definition_of_done),
                ("quality_gate_count", &quality_gate_count),
                ("exit_criteria_count", &exit_criteria_count),
                ("input_artifact_count", &input_artifact_count),
                ("admission_receipt_id", &admission_receipt.receipt_id),
            ]),
        );

        let allowed_write_scope = join_harness_values(manifest.allowed_write_scope);
        let blocked_paths = join_harness_values(manifest.blocked_paths);
        let refusal_decision =
            self.decide_with_receipt(&correlation, ActionKind::RefuseHarnessWrite);
        let refusal_receipt = self.transition_receipt(
            &harness_run_id,
            "REFUSAL",
            &correlation,
            &refusal_decision,
            "harness.write.refused",
            payload(&[
                ("manifest_run_id", manifest.run_id),
                ("reason", "plan_only_mode"),
                (
                    "file_write_permission",
                    manifest.permission_policy.file_write.as_str(),
                ),
                ("allowed_write_scope", &allowed_write_scope),
                ("blocked_paths", &blocked_paths),
                ("plan_receipt_id", &plan_receipt.receipt_id),
            ]),
        );

        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == harness_run_id)
        {
            run.status = "PLAN_ONLY_COMPLETED".to_string();
        }

        self.storage.ledger().verify()?;
        let receipts = self.storage.ledger().entries()[starting_receipts..]
            .iter()
            .map(|entry| entry.receipt_id.clone())
            .collect();
        let policy_decision_ids = self.storage.policy_decisions()[starting_policy_decisions..]
            .iter()
            .map(|decision| decision.policy_decision_id.clone())
            .collect();
        let mut transcript = HarnessRunTranscript::new(manifest.run_id, &harness_run_id);
        transcript.append_item(
            HarnessRunItemKind::Admission,
            admission_receipt.receipt_id.clone(),
            "manifest admitted for plan-only run",
        );
        transcript.append_item(
            HarnessRunItemKind::Plan,
            plan_receipt.receipt_id.clone(),
            "deterministic plan item emitted",
        );
        transcript.append_item(
            HarnessRunItemKind::WriteRefusal,
            refusal_receipt.receipt_id.clone(),
            "write refused because manifest mode is plan_only",
        );
        Ok(HarnessPlanRunReport {
            manifest_run_id: manifest.run_id.to_string(),
            harness_run_id,
            status: "PLAN_ONLY_COMPLETED".to_string(),
            admission_receipt_id: admission_receipt.receipt_id,
            plan_items: vec![HarnessPlanItem {
                plan_item_id,
                summary: plan_summary,
                command: plan_command,
                write_allowed: false,
                receipt_id: plan_receipt.receipt_id,
            }],
            write_refusal_receipt_id: refusal_receipt.receipt_id,
            policy_decision_ids,
            receipts,
            transcript,
        })
    }

    pub fn run_harness_safe_read(
        &mut self,
        manifest: &HarnessRunManifest<'_>,
        request: &HarnessReadToolRequest<'_>,
    ) -> Result<HarnessReadToolReport, HarnessToolPlaneError> {
        validate_harness_tool_request(
            manifest,
            request.allowed_read_roots,
            request.max_output_bytes,
        )?;

        let correlation = CorrelationIds {
            tenant_id: TenantId::new(manifest.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(manifest.actor_id),
            loop_id: LoopId::new("harness_safe_tool_plane"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let harness_run_id = self.ids.next("harness_run");
        self.storage.push_loop_run(LoopRun {
            run_id: harness_run_id.clone(),
            loop_id: correlation.loop_id.clone(),
            agent_id: correlation.actor_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            status: "RUNNING".to_string(),
        });

        let mediated_path = if manifest.permission_policy.file_read
            != HarnessPermissionLevel::ReadOnly
        {
            Err("file_read_not_allowed")
        } else {
            normalize_harness_relative_path(request.path).and_then(|path| {
                mediate_harness_read_path(&path, request.allowed_read_roots, manifest.blocked_paths)
                    .map(|_| path)
            })
        };
        let allowed_roots = join_harness_values(request.allowed_read_roots);
        let blocked_paths = join_harness_values(manifest.blocked_paths);
        let max_output_bytes = request.max_output_bytes.to_string();

        let (status, output, output_truncated, denied_reason, receipt, policy_decision) =
            match mediated_path {
                Ok(path) => {
                    let capped = cap_harness_output(request.contents, request.max_output_bytes);
                    let output_bytes = capped.output.len().to_string();
                    let truncated = capped.truncated.to_string();
                    let decision =
                        self.decide_with_receipt(&correlation, ActionKind::MediateHarnessRead);
                    let receipt = self.transition_receipt(
                        &harness_run_id,
                        "TOOL_READ",
                        &correlation,
                        &decision,
                        "harness.tool.read.allowed",
                        payload(&[
                            ("manifest_run_id", manifest.run_id),
                            ("path", &path),
                            ("allowed_read_roots", &allowed_roots),
                            ("blocked_paths", &blocked_paths),
                            ("max_output_bytes", &max_output_bytes),
                            ("output_bytes", &output_bytes),
                            ("output_truncated", &truncated),
                        ]),
                    );
                    (
                        "READ_ALLOWED".to_string(),
                        capped.output,
                        capped.truncated,
                        None,
                        receipt,
                        decision,
                    )
                }
                Err(reason) => {
                    let decision =
                        self.decide_with_receipt(&correlation, ActionKind::DenyHarnessTool);
                    let receipt = self.transition_receipt(
                        &harness_run_id,
                        "TOOL_DENIED",
                        &correlation,
                        &decision,
                        "harness.tool.read.denied",
                        payload(&[
                            ("manifest_run_id", manifest.run_id),
                            ("path", request.path),
                            ("reason", reason),
                            ("allowed_read_roots", &allowed_roots),
                            ("blocked_paths", &blocked_paths),
                        ]),
                    );
                    (
                        "READ_DENIED".to_string(),
                        String::new(),
                        false,
                        Some(reason.to_string()),
                        receipt,
                        decision,
                    )
                }
            };

        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == harness_run_id)
        {
            run.status = status.clone();
        }
        self.storage
            .ledger()
            .verify()
            .map_err(HarnessToolPlaneError::ManifestRejected)?;

        let mut transcript = HarnessRunTranscript::new(manifest.run_id, &harness_run_id);
        transcript.append_item(
            if denied_reason.is_some() {
                HarnessRunItemKind::ToolDenied
            } else {
                HarnessRunItemKind::ToolRead
            },
            receipt.receipt_id.clone(),
            if denied_reason.is_some() {
                "read tool request denied by path mediation"
            } else {
                "read tool output returned with cap enforcement"
            },
        );

        Ok(HarnessReadToolReport {
            manifest_run_id: manifest.run_id.to_string(),
            harness_run_id,
            status,
            path: request.path.to_string(),
            output_bytes: output.len(),
            output,
            output_truncated,
            denied_reason,
            receipt_id: receipt.receipt_id,
            policy_decision_id: policy_decision.policy_decision_id,
            transcript,
        })
    }

    pub fn run_harness_safe_search(
        &mut self,
        manifest: &HarnessRunManifest<'_>,
        request: &HarnessSearchToolRequest<'_>,
    ) -> Result<HarnessSearchToolReport, HarnessToolPlaneError> {
        validate_harness_tool_request(
            manifest,
            request.allowed_read_roots,
            request.max_output_bytes,
        )?;
        if request.query.trim().is_empty() {
            return Err(HarnessToolPlaneError::EmptySearchQuery);
        }
        if request.max_matches == 0 {
            return Err(HarnessToolPlaneError::EmptySearchBudget);
        }
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(manifest.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(manifest.actor_id),
            loop_id: LoopId::new("harness_safe_tool_plane"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let harness_run_id = self.ids.next("harness_run");
        self.storage.push_loop_run(LoopRun {
            run_id: harness_run_id.clone(),
            loop_id: correlation.loop_id.clone(),
            agent_id: correlation.actor_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            status: "RUNNING".to_string(),
        });
        let mediated_path = mediate_harness_tool_path(
            manifest,
            request.path,
            request.allowed_read_roots,
            manifest.blocked_paths,
        );
        let allowed_roots = join_harness_values(request.allowed_read_roots);
        let blocked_paths = join_harness_values(manifest.blocked_paths);
        let max_output_bytes = request.max_output_bytes.to_string();
        let max_matches = request.max_matches.to_string();

        let (status, matches, output_truncated, denied_reason, receipt, policy_decision) =
            match mediated_path {
                Ok(path) => {
                    let capped = search_harness_contents(
                        request.contents,
                        request.query,
                        request.max_matches,
                        request.max_output_bytes,
                    );
                    let output_bytes = harness_search_output_bytes(&capped.matches).to_string();
                    let truncated = capped.truncated.to_string();
                    let match_count = capped.matches.len().to_string();
                    let decision =
                        self.decide_with_receipt(&correlation, ActionKind::SearchHarnessArtifact);
                    let receipt = self.transition_receipt(
                        &harness_run_id,
                        "TOOL_SEARCH",
                        &correlation,
                        &decision,
                        "harness.tool.search.allowed",
                        payload(&[
                            ("manifest_run_id", manifest.run_id),
                            ("path", &path),
                            ("query", request.query),
                            ("allowed_read_roots", &allowed_roots),
                            ("blocked_paths", &blocked_paths),
                            ("max_matches", &max_matches),
                            ("max_output_bytes", &max_output_bytes),
                            ("match_count", &match_count),
                            ("output_bytes", &output_bytes),
                            ("output_truncated", &truncated),
                        ]),
                    );
                    (
                        "SEARCH_ALLOWED".to_string(),
                        capped.matches,
                        capped.truncated,
                        None,
                        receipt,
                        decision,
                    )
                }
                Err(reason) => {
                    let decision =
                        self.decide_with_receipt(&correlation, ActionKind::DenyHarnessTool);
                    let receipt = self.transition_receipt(
                        &harness_run_id,
                        "TOOL_SEARCH_DENIED",
                        &correlation,
                        &decision,
                        "harness.tool.search.denied",
                        payload(&[
                            ("manifest_run_id", manifest.run_id),
                            ("path", request.path),
                            ("query", request.query),
                            ("reason", reason),
                            ("allowed_read_roots", &allowed_roots),
                            ("blocked_paths", &blocked_paths),
                        ]),
                    );
                    (
                        "SEARCH_DENIED".to_string(),
                        Vec::new(),
                        false,
                        Some(reason.to_string()),
                        receipt,
                        decision,
                    )
                }
            };
        self.finish_harness_tool_run(&harness_run_id, &status)
            .map_err(HarnessToolPlaneError::ManifestRejected)?;
        let mut transcript = HarnessRunTranscript::new(manifest.run_id, &harness_run_id);
        transcript.append_item(
            if denied_reason.is_some() {
                HarnessRunItemKind::ToolDenied
            } else {
                HarnessRunItemKind::ToolSearch
            },
            receipt.receipt_id.clone(),
            if denied_reason.is_some() {
                "search tool request denied by path mediation"
            } else {
                "search tool output returned with cap enforcement"
            },
        );

        Ok(HarnessSearchToolReport {
            manifest_run_id: manifest.run_id.to_string(),
            harness_run_id,
            status,
            path: request.path.to_string(),
            query: request.query.to_string(),
            output_bytes: harness_search_output_bytes(&matches),
            matches,
            output_truncated,
            denied_reason,
            receipt_id: receipt.receipt_id,
            policy_decision_id: policy_decision.policy_decision_id,
            transcript,
        })
    }

    pub fn run_harness_safe_list(
        &mut self,
        manifest: &HarnessRunManifest<'_>,
        request: &HarnessListToolRequest<'_>,
    ) -> Result<HarnessListToolReport, HarnessToolPlaneError> {
        validate_harness_tool_request(
            manifest,
            request.allowed_read_roots,
            request.max_output_bytes,
        )?;
        if request.max_entries == 0 {
            return Err(HarnessToolPlaneError::EmptyListBudget);
        }
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(manifest.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(manifest.actor_id),
            loop_id: LoopId::new("harness_safe_tool_plane"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let harness_run_id = self.ids.next("harness_run");
        self.storage.push_loop_run(LoopRun {
            run_id: harness_run_id.clone(),
            loop_id: correlation.loop_id.clone(),
            agent_id: correlation.actor_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            status: "RUNNING".to_string(),
        });
        let mediated_path = mediate_harness_tool_path(
            manifest,
            request.path,
            request.allowed_read_roots,
            manifest.blocked_paths,
        );
        let allowed_roots = join_harness_values(request.allowed_read_roots);
        let blocked_paths = join_harness_values(manifest.blocked_paths);
        let max_output_bytes = request.max_output_bytes.to_string();
        let max_entries = request.max_entries.to_string();

        let (status, entries, output_truncated, denied_reason, receipt, policy_decision) =
            match mediated_path {
                Ok(path) => {
                    let capped = list_harness_entries(
                        request.entries,
                        request.max_entries,
                        request.max_output_bytes,
                    );
                    let output_bytes = harness_list_output_bytes(&capped.entries).to_string();
                    let truncated = capped.truncated.to_string();
                    let entry_count = capped.entries.len().to_string();
                    let decision =
                        self.decide_with_receipt(&correlation, ActionKind::ListHarnessArtifacts);
                    let receipt = self.transition_receipt(
                        &harness_run_id,
                        "TOOL_LIST",
                        &correlation,
                        &decision,
                        "harness.tool.list.allowed",
                        payload(&[
                            ("manifest_run_id", manifest.run_id),
                            ("path", &path),
                            ("allowed_read_roots", &allowed_roots),
                            ("blocked_paths", &blocked_paths),
                            ("max_entries", &max_entries),
                            ("max_output_bytes", &max_output_bytes),
                            ("entry_count", &entry_count),
                            ("output_bytes", &output_bytes),
                            ("output_truncated", &truncated),
                        ]),
                    );
                    (
                        "LIST_ALLOWED".to_string(),
                        capped.entries,
                        capped.truncated,
                        None,
                        receipt,
                        decision,
                    )
                }
                Err(reason) => {
                    let decision =
                        self.decide_with_receipt(&correlation, ActionKind::DenyHarnessTool);
                    let receipt = self.transition_receipt(
                        &harness_run_id,
                        "TOOL_LIST_DENIED",
                        &correlation,
                        &decision,
                        "harness.tool.list.denied",
                        payload(&[
                            ("manifest_run_id", manifest.run_id),
                            ("path", request.path),
                            ("reason", reason),
                            ("allowed_read_roots", &allowed_roots),
                            ("blocked_paths", &blocked_paths),
                        ]),
                    );
                    (
                        "LIST_DENIED".to_string(),
                        Vec::new(),
                        false,
                        Some(reason.to_string()),
                        receipt,
                        decision,
                    )
                }
            };
        self.finish_harness_tool_run(&harness_run_id, &status)
            .map_err(HarnessToolPlaneError::ManifestRejected)?;
        let mut transcript = HarnessRunTranscript::new(manifest.run_id, &harness_run_id);
        transcript.append_item(
            if denied_reason.is_some() {
                HarnessRunItemKind::ToolDenied
            } else {
                HarnessRunItemKind::ToolList
            },
            receipt.receipt_id.clone(),
            if denied_reason.is_some() {
                "list tool request denied by path mediation"
            } else {
                "list tool output returned with cap enforcement"
            },
        );

        Ok(HarnessListToolReport {
            manifest_run_id: manifest.run_id.to_string(),
            harness_run_id,
            status,
            path: request.path.to_string(),
            output_bytes: harness_list_output_bytes(&entries),
            entries,
            output_truncated,
            denied_reason,
            receipt_id: receipt.receipt_id,
            policy_decision_id: policy_decision.policy_decision_id,
            transcript,
        })
    }

    pub fn run_harness_safe_patch(
        &mut self,
        manifest: &HarnessRunManifest<'_>,
        request: &HarnessPatchToolRequest<'_>,
    ) -> Result<HarnessPatchToolReport, HarnessToolPlaneError> {
        validate_harness_patch_tool_request(manifest, request)?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(manifest.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(manifest.actor_id),
            loop_id: LoopId::new("harness_safe_tool_plane"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let harness_run_id = self.ids.next("harness_run");
        self.storage.push_loop_run(LoopRun {
            run_id: harness_run_id.clone(),
            loop_id: correlation.loop_id.clone(),
            agent_id: correlation.actor_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            status: "RUNNING".to_string(),
        });
        let mediated_path = mediate_harness_patch_path(manifest, request.path).and_then(|path| {
            if request.patch.len() > request.max_patch_bytes {
                Err("patch_budget_exceeded")
            } else {
                Ok(path)
            }
        });
        let allowed_write_scope = join_harness_values(manifest.allowed_write_scope);
        let blocked_paths = join_harness_values(manifest.blocked_paths);
        let blocked_tools = join_harness_values(manifest.blocked_tools);
        let max_patch_bytes = request.max_patch_bytes.to_string();
        let max_output_bytes = request.max_output_bytes.to_string();
        let patch_bytes = request.patch.len().to_string();

        let (status, patch_preview, output_truncated, denied_reason, receipt, policy_decision) =
            match mediated_path {
                Ok(path) => {
                    let capped = cap_harness_output(request.patch, request.max_output_bytes);
                    let output_bytes = capped.output.len().to_string();
                    let truncated = capped.truncated.to_string();
                    let decision =
                        self.decide_with_receipt(&correlation, ActionKind::MediateHarnessPatch);
                    let receipt = self.transition_receipt(
                        &harness_run_id,
                        "TOOL_PATCH",
                        &correlation,
                        &decision,
                        "harness.tool.patch.allowed",
                        payload(&[
                            ("manifest_run_id", manifest.run_id),
                            ("path", &path),
                            ("allowed_write_scope", &allowed_write_scope),
                            ("blocked_paths", &blocked_paths),
                            ("blocked_tools", &blocked_tools),
                            ("max_patch_bytes", &max_patch_bytes),
                            ("max_output_bytes", &max_output_bytes),
                            ("patch_bytes", &patch_bytes),
                            ("output_bytes", &output_bytes),
                            ("output_truncated", &truncated),
                            ("applied", "false"),
                        ]),
                    );
                    (
                        "PATCH_ALLOWED".to_string(),
                        capped.output,
                        capped.truncated,
                        None,
                        receipt,
                        decision,
                    )
                }
                Err(reason) => {
                    let decision =
                        self.decide_with_receipt(&correlation, ActionKind::DenyHarnessTool);
                    let receipt = self.transition_receipt(
                        &harness_run_id,
                        "TOOL_PATCH_DENIED",
                        &correlation,
                        &decision,
                        "harness.tool.patch.denied",
                        payload(&[
                            ("manifest_run_id", manifest.run_id),
                            ("path", request.path),
                            ("reason", reason),
                            ("allowed_write_scope", &allowed_write_scope),
                            ("blocked_paths", &blocked_paths),
                            ("blocked_tools", &blocked_tools),
                            ("max_patch_bytes", &max_patch_bytes),
                            ("patch_bytes", &patch_bytes),
                        ]),
                    );
                    (
                        "PATCH_DENIED".to_string(),
                        String::new(),
                        false,
                        Some(reason.to_string()),
                        receipt,
                        decision,
                    )
                }
            };
        self.finish_harness_tool_run(&harness_run_id, &status)
            .map_err(HarnessToolPlaneError::ManifestRejected)?;
        let mut transcript = HarnessRunTranscript::new(manifest.run_id, &harness_run_id);
        transcript.append_item(
            if denied_reason.is_some() {
                HarnessRunItemKind::ToolDenied
            } else {
                HarnessRunItemKind::ToolPatch
            },
            receipt.receipt_id.clone(),
            if denied_reason.is_some() {
                "patch tool request denied by policy or path mediation"
            } else {
                "patch proposal mediated with cap enforcement and no application"
            },
        );

        Ok(HarnessPatchToolReport {
            manifest_run_id: manifest.run_id.to_string(),
            harness_run_id,
            status,
            path: request.path.to_string(),
            patch_bytes: request.patch.len(),
            output_bytes: patch_preview.len(),
            patch_preview,
            output_truncated,
            denied_reason,
            receipt_id: receipt.receipt_id,
            policy_decision_id: policy_decision.policy_decision_id,
            transcript,
        })
    }

    fn finish_harness_tool_run(
        &mut self,
        harness_run_id: &str,
        status: &str,
    ) -> Result<(), String> {
        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == harness_run_id)
        {
            run.status = status.to_string();
        }
        self.storage.ledger().verify()
    }

    pub fn run_evals_runner_agent(&mut self) -> Result<LoopRunReport, String> {
        let correlation = CorrelationIds {
            tenant_id: TenantId::new("tenant_local"),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new("evals_runner_agent"),
            loop_id: LoopId::new("evals_runner_agent"),
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

        let trigger_decision = self.decide_with_receipt(&correlation, ActionKind::TriggerLoop);
        self.transition_receipt(
            &run_id,
            "TRIGGER",
            &correlation,
            &trigger_decision,
            "loop.triggered",
            payload(&[
                ("loop", "evals_runner_agent"),
                ("trigger", "local_manual"),
                ("substrate", "local_runner"),
            ]),
        );

        let action_decision = self.decide_with_receipt(&correlation, ActionKind::RunEvalSuite);
        let model_gateway = DeterministicModelGateway;
        let model_trace = model_gateway.run_eval_suite("local_credentialing_smoke")?;
        let cases = model_trace.cases.to_string();
        let score = model_trace.score.to_string();
        let passed = model_trace.passed.to_string();
        let stream_event_count = model_trace.stream_event_count.to_string();
        let first_byte_latency_ms = model_trace.first_byte_latency_ms.to_string();
        let failover_slo_ms = model_trace.failover_slo_ms.to_string();
        let suite_receipt = self.transition_receipt(
            &run_id,
            "ACTION",
            &correlation,
            &action_decision,
            "eval.suite.ran",
            payload(&[
                ("suite", &model_trace.suite_id),
                ("cases", &cases),
                ("model_gateway", model_trace.gateway),
                ("model_gateway_driver", "local_model_gateway"),
                ("model_gateway_provider", "DeterministicModelGateway"),
                ("model_gateway_model_id", &model_trace.variant),
                ("model_gateway_inference_id", &model_trace.inference_id),
                ("model_gateway_variant", &model_trace.variant),
                ("model_gateway_routing", &model_trace.routing_strategy),
                (
                    "model_gateway_stream_contract",
                    &model_trace.stream_contract,
                ),
                ("model_gateway_stream_event_count", &stream_event_count),
                ("model_gateway_terminal_event", &model_trace.terminal_event),
                (
                    "model_gateway_fallback_strategy",
                    &model_trace.fallback_strategy,
                ),
                (
                    "model_gateway_fallback_provider",
                    &model_trace.fallback_provider,
                ),
                (
                    "model_gateway_first_byte_latency_ms",
                    &first_byte_latency_ms,
                ),
                ("model_gateway_failover_slo_ms", &failover_slo_ms),
                ("provider_call_allowed", "false"),
            ]),
        );

        let outcome_decision = self.decide_with_receipt(&correlation, ActionKind::GradeTrace);
        let verdict_receipt = self.transition_receipt(
            &run_id,
            "OUTCOME",
            &correlation,
            &outcome_decision,
            "eval.verdict.recorded",
            payload(&[
                ("suite", &model_trace.suite_id),
                ("score", &score),
                ("passed", &passed),
                ("trace_receipt_id", &suite_receipt.receipt_id),
            ]),
        );
        let verdict = EvalVerdict {
            eval_verdict_id: self.ids.next("verdict"),
            tenant_id: correlation.tenant_id.clone(),
            actor_id: correlation.actor_id.clone(),
            suite_id: model_trace.suite_id.clone(),
            passed: model_trace.passed,
            score: model_trace.score,
            trace_receipt_id: suite_receipt.receipt_id.clone(),
        };
        self.storage.push_eval_verdict(verdict.clone());

        let credential_decision =
            self.decide_with_receipt(&correlation, ActionKind::MintCredential);
        let credential_receipt = self.storage.append_receipt(
            &mut self.ids,
            &correlation,
            "credential.minted",
            Some(credential_decision.policy_decision_id.clone()),
            payload(&[
                ("credential_subject", "evals_runner_agent"),
                ("eval_verdict_id", &verdict.eval_verdict_id),
                ("status", "MINTED"),
            ]),
        );
        let credential = self.mint_credential(&correlation, &verdict, &credential_receipt)?;
        self.storage.enqueue_outbox(OutboxEvent::from_receipt(
            &mut self.ids,
            &credential_receipt,
            "observatory.credential_ready",
            payload(&[
                ("credential_id", &credential.credential_id),
                ("credential_status", &credential.status),
                ("eval_verdict_id", &verdict.eval_verdict_id),
            ]),
        ));

        let signal_decision = self.decide_with_receipt(&correlation, ActionKind::WriteMemory);
        let memory_receipt = self.transition_receipt(
            &run_id,
            "SIGNAL",
            &correlation,
            &signal_decision,
            "memory.consolidation_decided",
            payload(&[
                ("decision", "RETAIN"),
                ("source_receipt_id", &credential_receipt.receipt_id),
                (
                    "reason",
                    "credential outcome changes future autonomy context",
                ),
            ]),
        );
        self.write_memory(
            &correlation,
            &credential_receipt.receipt_id,
            &memory_receipt.receipt_id,
            ConsolidationDecision::Retain,
            "observatory",
            "agent_operational_memory",
            "evals_runner_agent minted local credential from deterministic eval suite",
            // The kernel's own operational loop, not a surface proposal;
            // the surface ratification lane does not govern it.
            MEMORY_CONSOLIDATION_ACTIVE,
        )?;

        let charter_decision = self.decide_with_receipt(&correlation, ActionKind::WriteCharter);
        let charter_receipt = self.storage.append_receipt(
            &mut self.ids,
            &correlation,
            "charter.evidence.recorded",
            Some(charter_decision.policy_decision_id.clone()),
            payload(&[
                ("obligation", "eval credentialing must be receipt-backed"),
                ("evidence_receipt_id", &verdict_receipt.receipt_id),
            ]),
        );
        self.record_charter_evidence(
            &correlation,
            &verdict_receipt.receipt_id,
            "eval credentialing must be receipt-backed",
            "local deterministic eval verdict linked to minted credential",
            &charter_receipt.receipt_id,
        );

        let adjustment_decision =
            self.decide_with_receipt(&correlation, ActionKind::RecordAdjustment);
        self.transition_receipt(
            &run_id,
            "ADJUSTMENT",
            &correlation,
            &adjustment_decision,
            "loop.adjustment.recorded",
            payload(&[
                ("next_action", "renew credential on next eval trigger"),
                ("memory_receipt_id", &memory_receipt.receipt_id),
                ("charter_receipt_id", &charter_receipt.receipt_id),
            ]),
        );

        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status = "COMPLETED".to_string();
        }

        self.storage.ledger().verify()?;
        Ok(LoopRunReport {
            run_id,
            loop_id: "evals_runner_agent".to_string(),
            status: "COMPLETED".to_string(),
            score: verdict.score,
            credential_status: credential.status,
            receipts: self
                .storage
                .ledger()
                .entries()
                .iter()
                .map(|entry| entry.receipt_id.clone())
                .collect(),
            concierge_answer: self.concierge_answer("what happened and why?"),
        })
    }

    pub fn run_aegis_scanner_agent(&mut self) -> Result<LoopRunReport, String> {
        let correlation = CorrelationIds {
            tenant_id: TenantId::new("tenant_local"),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new("aegis_scanner_agent"),
            loop_id: LoopId::new("aegis_scanner_agent"),
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

        let trigger_decision = self.decide_with_receipt(&correlation, ActionKind::TriggerLoop);
        self.transition_receipt(
            &run_id,
            "TRIGGER",
            &correlation,
            &trigger_decision,
            "loop.triggered",
            payload(&[
                ("loop", "aegis_scanner_agent"),
                ("trigger", "local_manual"),
                ("substrate", "local_runner"),
            ]),
        );

        let scan_decision = self.decide_with_receipt(&correlation, ActionKind::RunSecurityScan);
        let scan_receipt = self.transition_receipt(
            &run_id,
            "ACTION",
            &correlation,
            &scan_decision,
            "aegis.scan.ran",
            payload(&[
                ("scanner", "deterministic_local_scanner"),
                ("target", "mdx-native"),
                ("findings", "1"),
            ]),
        );

        let classify_decision = self.decide_with_receipt(&correlation, ActionKind::ClassifyFinding);
        let finding_receipt = self.transition_receipt(
            &run_id,
            "OUTCOME",
            &correlation,
            &classify_decision,
            "aegis.finding.classified",
            payload(&[
                ("finding_id", "aegis_local_finding_001"),
                ("severity", "LOW"),
                ("source_receipt_id", &scan_receipt.receipt_id),
                ("classification", "known_local_stub_gap"),
            ]),
        );

        let charter_decision = self.decide_with_receipt(&correlation, ActionKind::WriteCharter);
        let charter_receipt = self.transition_receipt(
            &run_id,
            "SIGNAL",
            &correlation,
            &charter_decision,
            "charter.evidence.recorded",
            payload(&[
                (
                    "obligation",
                    "security scans must produce receipt-backed findings",
                ),
                ("evidence_receipt_id", &finding_receipt.receipt_id),
            ]),
        );
        self.record_charter_evidence(
            &correlation,
            &finding_receipt.receipt_id,
            "security scans must produce receipt-backed findings",
            "deterministic scanner classified local finding with receipt evidence",
            &charter_receipt.receipt_id,
        );

        let adjustment_decision =
            self.decide_with_receipt(&correlation, ActionKind::PlanRemediation);
        self.transition_receipt(
            &run_id,
            "ADJUSTMENT",
            &correlation,
            &adjustment_decision,
            "aegis.remediation.planned",
            payload(&[
                ("finding_id", "aegis_local_finding_001"),
                (
                    "next_action",
                    "keep deterministic scanner boundary pending live scan adapter",
                ),
                ("charter_receipt_id", &charter_receipt.receipt_id),
            ]),
        );

        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status = "COMPLETED".to_string();
        }

        self.storage.ledger().verify()?;
        let receipts = self
            .storage
            .ledger()
            .entries()
            .iter()
            .map(|entry| entry.receipt_id.clone())
            .collect();
        Ok(LoopRunReport {
            run_id,
            loop_id: "aegis_scanner_agent".to_string(),
            status: "COMPLETED".to_string(),
            score: 100,
            credential_status: "NOT_APPLICABLE".to_string(),
            receipts,
            concierge_answer: format!(
                "aegis_scanner_agent ran deterministic_local_scanner, classified aegis_local_finding_001 as LOW, and planned remediation because policy allowed each consequential step. Source receipts: {}",
                self.storage.ledger().query().receipt_ids().join(", ")
            ),
        })
    }

    pub fn run_charter_attestation_agent(&mut self) -> Result<LoopRunReport, String> {
        let correlation = CorrelationIds {
            tenant_id: TenantId::new("tenant_local"),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new("charter_attestation_agent"),
            loop_id: LoopId::new("charter_attestation_agent"),
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

        let trigger_decision = self.decide_with_receipt(&correlation, ActionKind::TriggerLoop);
        self.transition_receipt(
            &run_id,
            "TRIGGER",
            &correlation,
            &trigger_decision,
            "loop.triggered",
            payload(&[
                ("loop", "charter_attestation_agent"),
                ("trigger", "local_manual"),
                ("substrate", "local_runner"),
            ]),
        );

        let check_decision =
            self.decide_with_receipt(&correlation, ActionKind::CheckCharterObligation);
        let check_receipt = self.transition_receipt(
            &run_id,
            "ACTION",
            &correlation,
            &check_decision,
            "charter.obligation.checked",
            payload(&[
                (
                    "obligation",
                    "loop actions must be policy and receipt backed",
                ),
                ("checker", "deterministic_obligation_checker"),
                ("status", "SATISFIED"),
            ]),
        );

        let attest_decision =
            self.decide_with_receipt(&correlation, ActionKind::AttestCharterEvidence);
        let attestation_receipt = self.transition_receipt(
            &run_id,
            "OUTCOME",
            &correlation,
            &attest_decision,
            "charter.evidence.attested",
            payload(&[
                ("attestation_id", "charter_local_attestation_001"),
                ("obligation_receipt_id", &check_receipt.receipt_id),
                ("verdict", "SATISFIED"),
            ]),
        );
        self.record_charter_evidence(
            &correlation,
            &attestation_receipt.receipt_id,
            "loop actions must be policy and receipt backed",
            "charter attestation linked deterministic obligation evidence",
            &attestation_receipt.receipt_id,
        );
        self.storage.enqueue_outbox(OutboxEvent::from_receipt(
            &mut self.ids,
            &attestation_receipt,
            "observatory.charter_attestation_ready",
            payload(&[
                ("attestation_id", "charter_local_attestation_001"),
                ("verdict", "SATISFIED"),
            ]),
        ));

        let exception_decision =
            self.decide_with_receipt(&correlation, ActionKind::ReviewCharterException);
        let exception_receipt = self.transition_receipt(
            &run_id,
            "SIGNAL",
            &correlation,
            &exception_decision,
            "charter.exception.reviewed",
            payload(&[
                ("exception_status", "NONE_OPEN"),
                ("attestation_receipt_id", &attestation_receipt.receipt_id),
            ]),
        );

        let adjustment_decision =
            self.decide_with_receipt(&correlation, ActionKind::RecordAdjustment);
        self.transition_receipt(
            &run_id,
            "ADJUSTMENT",
            &correlation,
            &adjustment_decision,
            "loop.adjustment.recorded",
            payload(&[
                ("next_action", "rerun attestation on next Charter trigger"),
                ("exception_receipt_id", &exception_receipt.receipt_id),
                ("attestation_receipt_id", &attestation_receipt.receipt_id),
            ]),
        );

        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status = "COMPLETED".to_string();
        }

        self.storage.ledger().verify()?;
        let receipts = self
            .storage
            .ledger()
            .entries()
            .iter()
            .map(|entry| entry.receipt_id.clone())
            .collect();
        Ok(LoopRunReport {
            run_id,
            loop_id: "charter_attestation_agent".to_string(),
            status: "COMPLETED".to_string(),
            score: 100,
            credential_status: "NOT_APPLICABLE".to_string(),
            receipts,
            concierge_answer: format!(
                "charter_attestation_agent checked a declared obligation, attested evidence as SATISFIED, and reviewed exceptions because policy allowed each consequential step. Source receipts: {}",
                self.storage.ledger().query().receipt_ids().join(", ")
            ),
        })
    }

    pub fn run_forge_orchestrator_agent(&mut self) -> Result<LoopRunReport, String> {
        let correlation = CorrelationIds {
            tenant_id: TenantId::new("tenant_local"),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new("forge_orchestrator_agent"),
            loop_id: LoopId::new("forge_orchestrator_agent"),
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

        let trigger_decision = self.decide_with_receipt(&correlation, ActionKind::TriggerLoop);
        self.transition_receipt(
            &run_id,
            "TRIGGER",
            &correlation,
            &trigger_decision,
            "loop.triggered",
            payload(&[
                ("loop", "forge_orchestrator_agent"),
                ("trigger", "local_manual"),
                ("substrate", "local_runner"),
            ]),
        );

        let spec_decision = self.decide_with_receipt(&correlation, ActionKind::AcceptForgeSpec);
        let spec_receipt = self.transition_receipt(
            &run_id,
            "ACTION",
            &correlation,
            &spec_decision,
            "forge.spec.accepted",
            payload(&[
                ("scenario", "clean_build_entry"),
                ("stage", "spec"),
                ("source_contract", "generated/forge/workflow-contract.json"),
            ]),
        );

        let stage_decision =
            self.decide_with_receipt(&correlation, ActionKind::RecordForgeStageTransition);
        let stage_receipt = self.transition_receipt(
            &run_id,
            "OUTCOME",
            &correlation,
            &stage_decision,
            "forge.stage.transition.recorded",
            payload(&[
                ("transition_id", "spec_to_build"),
                ("from_stage", "spec"),
                ("to_stage", "build"),
                ("required_receipt_kind", "forge.spec.accepted"),
                ("source_receipt_ids", &spec_receipt.receipt_id),
                ("blocked_by", "none"),
            ]),
        );

        let delegation_decision =
            self.decide_with_receipt(&correlation, ActionKind::RequestForgeDelegation);
        let delegation_receipt = self.transition_receipt(
            &run_id,
            "SIGNAL",
            &correlation,
            &delegation_decision,
            "forge.delegation.requested",
            payload(&[
                ("forge_stage", "build"),
                (
                    "delegation_reason",
                    "build stage requires bounded Talent worker authority",
                ),
                (
                    "talent_authorization_envelope_id",
                    "PENDING_TALENT_AUTHORITY",
                ),
                ("requested_receipt_kind", "talent.authorization.recorded"),
                ("handoff_required", "true"),
                ("human_edge_ratification_required", "true"),
                ("source_receipt_ids", &stage_receipt.receipt_id),
            ]),
        );
        self.storage.enqueue_outbox(OutboxEvent::from_receipt(
            &mut self.ids,
            &delegation_receipt,
            "observatory.forge_delegation_blocked",
            payload(&[
                ("forge_stage", "build"),
                ("blocked_by", "missing_talent_authority_receipts"),
            ]),
        ));

        let adjustment_decision =
            self.decide_with_receipt(&correlation, ActionKind::RecordAdjustment);
        self.transition_receipt(
            &run_id,
            "ADJUSTMENT",
            &correlation,
            &adjustment_decision,
            "loop.adjustment.recorded",
            payload(&[
                ("terminal_state", "BLOCKED_ON_TALENT_AUTHORITY"),
                (
                    "next_action",
                    "wait for Talent sponsor chain, lease, budget, tool allowlist, and credential receipts",
                ),
                ("delegation_receipt_id", &delegation_receipt.receipt_id),
            ]),
        );

        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status = "BLOCKED_ON_TALENT_AUTHORITY".to_string();
        }

        self.storage.ledger().verify()?;
        let receipts = self
            .storage
            .ledger()
            .entries()
            .iter()
            .map(|entry| entry.receipt_id.clone())
            .collect();
        Ok(LoopRunReport {
            run_id,
            loop_id: "forge_orchestrator_agent".to_string(),
            status: "BLOCKED_ON_TALENT_AUTHORITY".to_string(),
            score: 0,
            credential_status: "NOT_APPLICABLE".to_string(),
            receipts,
            concierge_answer: format!(
                "forge_orchestrator_agent accepted a spec, recorded spec_to_build, and requested Talent delegation, then blocked before worker spawn because authority receipts are missing. Source receipts: {}",
                self.storage.ledger().query().receipt_ids().join(", ")
            ),
        })
    }

    pub fn run_product_shaping_agent(&mut self) -> Result<LoopRunReport, String> {
        let correlation = CorrelationIds {
            tenant_id: TenantId::new("tenant_local"),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new("product_shaping_agent"),
            loop_id: LoopId::new("product_shaping_agent"),
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

        let trigger_decision = self.decide_with_receipt(&correlation, ActionKind::TriggerLoop);
        self.transition_receipt(
            &run_id,
            "TRIGGER",
            &correlation,
            &trigger_decision,
            "loop.triggered",
            payload(&[
                ("loop", "product_shaping_agent"),
                ("trigger", "declared_operating_signal"),
                ("substrate", "local_runner"),
            ]),
        );

        let signal_decision =
            self.decide_with_receipt(&correlation, ActionKind::IngestProductSignal);
        let signal_receipt = self.transition_receipt(
            &run_id,
            "ACTION",
            &correlation,
            &signal_decision,
            "product.signal.ingested",
            payload(&[
                (
                    "signal_source",
                    "generated/world-model/pages-message-contract.json",
                ),
                ("signal_kind", "operator_request"),
                ("source_surface", "concierge"),
            ]),
        );

        let shape_decision = self.decide_with_receipt(&correlation, ActionKind::ShapeProductBet);
        let shaped_bet_receipt = self.transition_receipt(
            &run_id,
            "OUTCOME",
            &correlation,
            &shape_decision,
            "product.bet.shaped",
            payload(&[
                ("source_signal_receipt_id", &signal_receipt.receipt_id),
                ("bet_id", "product_bet_local_stub"),
                ("shape_status", "DRAFT_REQUIRES_HUMAN_RATIFICATION"),
                ("forbidden_action", "set_company_direction"),
            ]),
        );

        let handoff_decision =
            self.decide_with_receipt(&correlation, ActionKind::RequestProductHandoff);
        let handoff_receipt = self.transition_receipt(
            &run_id,
            "SIGNAL",
            &correlation,
            &handoff_decision,
            "product.handoff.requested",
            payload(&[
                ("shaped_bet_receipt_id", &shaped_bet_receipt.receipt_id),
                ("human_edge_surface", "observatory"),
                ("ratification_required", "true"),
                ("runtime_status", "BLOCKED_ON_HUMAN_PRODUCT_RATIFICATION"),
            ]),
        );
        self.storage.enqueue_outbox(OutboxEvent::from_receipt(
            &mut self.ids,
            &handoff_receipt,
            "observatory.product_handoff_blocked",
            payload(&[
                ("blocked_by", "human_product_ratification"),
                ("shaped_bet_receipt_id", &shaped_bet_receipt.receipt_id),
            ]),
        ));

        let adjustment_decision =
            self.decide_with_receipt(&correlation, ActionKind::RecordAdjustment);
        self.transition_receipt(
            &run_id,
            "ADJUSTMENT",
            &correlation,
            &adjustment_decision,
            "loop.adjustment.recorded",
            payload(&[
                ("terminal_state", "BLOCKED_ON_HUMAN_PRODUCT_RATIFICATION"),
                (
                    "next_action",
                    "wait for human product ratification before direction, budget, worker spawn, or build handoff",
                ),
                ("product_handoff_receipt_id", &handoff_receipt.receipt_id),
            ]),
        );

        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status = "BLOCKED_ON_HUMAN_PRODUCT_RATIFICATION".to_string();
        }

        self.storage.ledger().verify()?;
        let receipts = self
            .storage
            .ledger()
            .entries()
            .iter()
            .map(|entry| entry.receipt_id.clone())
            .collect();
        Ok(LoopRunReport {
            run_id,
            loop_id: "product_shaping_agent".to_string(),
            status: "BLOCKED_ON_HUMAN_PRODUCT_RATIFICATION".to_string(),
            score: 0,
            credential_status: "NOT_APPLICABLE".to_string(),
            receipts,
            concierge_answer: format!(
                "product_shaping_agent ingested an operating signal, shaped a draft product bet, and requested human ratification before direction, budget, worker spawn, or build handoff. Source receipts: {}",
                self.storage.ledger().query().receipt_ids().join(", ")
            ),
        })
    }

    pub fn run_talent_autonomy_agent(&mut self) -> Result<LoopRunReport, String> {
        let correlation = CorrelationIds {
            tenant_id: TenantId::new("tenant_local"),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new("talent_autonomy_agent"),
            loop_id: LoopId::new("talent_autonomy_agent"),
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

        let trigger_decision = self.decide_with_receipt(&correlation, ActionKind::TriggerLoop);
        self.transition_receipt(
            &run_id,
            "TRIGGER",
            &correlation,
            &trigger_decision,
            "loop.triggered",
            payload(&[
                ("loop", "talent_autonomy_agent"),
                ("trigger", "forge_delegation_boundary"),
                ("substrate", "local_runner"),
            ]),
        );

        let sponsor_decision =
            self.decide_with_receipt(&correlation, ActionKind::AuthorizeTalentSponsorChain);
        let sponsor_receipt = self.transition_receipt(
            &run_id,
            "ACTION",
            &correlation,
            &sponsor_decision,
            "talent.sponsor_chain.authorized",
            payload(&[
                ("parent_loop_id", "forge_orchestrator_agent"),
                ("requested_by_loop_id", "forge_orchestrator_agent"),
                ("human_sponsor_chain", "local_human_sponsor"),
                ("scope", "forge_clean_build_entry"),
                (
                    "source_contract",
                    "generated/talent/sponsor-chain-authority.json",
                ),
            ]),
        );

        let lease_decision =
            self.decide_with_receipt(&correlation, ActionKind::AuthorizeTalentWorkerLease);
        let lease_receipt = self.transition_receipt(
            &run_id,
            "ACTION",
            &correlation,
            &lease_decision,
            "talent.worker_lease.authorized",
            payload(&[
                ("parent_loop_id", "forge_orchestrator_agent"),
                ("scope", "forge_clean_build_entry"),
                ("expires_at", "2030-01-01T00:00:00Z"),
                (
                    "source_contract",
                    "generated/talent/worker-lease-authority.json",
                ),
            ]),
        );

        let budget_decision =
            self.decide_with_receipt(&correlation, ActionKind::AuthorizeTalentBudget);
        let budget_receipt = self.transition_receipt(
            &run_id,
            "ACTION",
            &correlation,
            &budget_decision,
            "talent.budget.authorized",
            payload(&[
                ("parent_loop_id", "forge_orchestrator_agent"),
                ("budget_limit", "0"),
                ("budget_unit", "local_stub_cents"),
                ("treasury_status", "deterministic_local_no_live_spend"),
                ("source_contract", "generated/talent/budget-authority.json"),
            ]),
        );

        let tool_decision =
            self.decide_with_receipt(&correlation, ActionKind::AuthorizeTalentToolAllowlist);
        let tool_receipt = self.transition_receipt(
            &run_id,
            "ACTION",
            &correlation,
            &tool_decision,
            "talent.tool_allowlist.authorized",
            payload(&[
                ("worker_template_id", "worker_bounded_task_template"),
                ("tool_allowlist", "local_read,local_write_patch,local_test"),
                ("forbidden_tools", "production_deploy,live_spend"),
                ("source_contract", "generated/talent/tool-allowlist.json"),
            ]),
        );

        let authorization_decision =
            self.decide_with_receipt(&correlation, ActionKind::RecordTalentAuthorization);
        let authorization_receipt = self.transition_receipt(
            &run_id,
            "OUTCOME",
            &correlation,
            &authorization_decision,
            "talent.authorization.recorded",
            payload(&[
                ("parent_loop_id", "forge_orchestrator_agent"),
                ("requested_by_loop_id", "forge_orchestrator_agent"),
                (
                    "sponsor_chain_authority_receipt_id",
                    &sponsor_receipt.receipt_id,
                ),
                (
                    "worker_lease_authority_receipt_id",
                    &lease_receipt.receipt_id,
                ),
                ("budget_authority_receipt_id", &budget_receipt.receipt_id),
                (
                    "tool_allowlist_authority_receipt_id",
                    &tool_receipt.receipt_id,
                ),
                ("eval_credential_id", "eval_credential_local_stub"),
                ("credential_check_policy", "fresh_eval_required"),
                ("handoff_required", "true"),
                ("requested_receipt_kind", "worker.spawn_requested"),
            ]),
        );

        let credential_decision =
            self.decide_with_receipt(&correlation, ActionKind::CheckWorkerCredential);
        let credential_receipt = self.transition_receipt(
            &run_id,
            "SIGNAL",
            &correlation,
            &credential_decision,
            "worker.credential.checked",
            payload(&[
                ("eval_credential_id", "eval_credential_local_stub"),
                ("capability_scope", "forge_clean_build_entry"),
                ("expires_at", "2030-01-01T00:00:00Z"),
                ("credential_status", "VALID_FOR_LOCAL_STUB"),
                (
                    "source_contract",
                    "generated/workers/credential-check-contract.json",
                ),
                (
                    "authorization_receipt_id",
                    &authorization_receipt.receipt_id,
                ),
            ]),
        );

        admit_worker_spawn(&WorkerSpawnAdmission {
            worker_template_id: "worker_bounded_task_template",
            parent_id: "forge_orchestrator_agent",
            sponsor_chain_authority_receipt_id: &sponsor_receipt.receipt_id,
            human_sponsor_chain: &["local_human_sponsor"],
            worker_lease_authority_receipt_id: &lease_receipt.receipt_id,
            scope: "forge_clean_build_entry",
            credential_scope: "forge_clean_build_entry",
            expires_at: "2030-01-01T00:00:00Z",
            now: "2026-01-01T00:00:00Z",
            budget_authority_receipt_id: &budget_receipt.receipt_id,
            budget: WorkerSpawnBudget {
                max_runtime_ms: 250,
                max_tool_calls: 1,
            },
            tool_allowlist_authority_receipt_id: &tool_receipt.receipt_id,
            tool_allowlist: &["local_read", "local_write_patch", "local_test"],
            credential_check_receipt_id: &credential_receipt.receipt_id,
            credential_requested_receipt_kind: "worker.credential.checked",
            issuer_loop_id: "evals_runner_agent",
            requested_receipt_kind: "worker.spawn_requested",
        })
        .map_err(|error| error.message())?;

        let spawn_decision = self.decide_with_receipt(&correlation, ActionKind::RequestWorkerSpawn);
        let spawn_receipt = self.transition_receipt(
            &run_id,
            "SIGNAL",
            &correlation,
            &spawn_decision,
            "worker.spawn_requested",
            payload(&[
                ("worker_template_id", "worker_bounded_task_template"),
                ("parent_id", "forge_orchestrator_agent"),
                (
                    "sponsor_chain_authority_receipt_id",
                    &sponsor_receipt.receipt_id,
                ),
                (
                    "worker_lease_authority_receipt_id",
                    &lease_receipt.receipt_id,
                ),
                ("budget_authority_receipt_id", &budget_receipt.receipt_id),
                (
                    "tool_allowlist_authority_receipt_id",
                    &tool_receipt.receipt_id,
                ),
                (
                    "credential_check_receipt_id",
                    &credential_receipt.receipt_id,
                ),
                ("requested_receipt_kind", "worker.spawn_requested"),
                ("runtime_status", "BLOCKED_ON_LIVE_WORKER_EXECUTION"),
            ]),
        );
        self.storage.enqueue_outbox(OutboxEvent::from_receipt(
            &mut self.ids,
            &spawn_receipt,
            "observatory.worker_spawn_blocked",
            payload(&[
                ("worker_template_id", "worker_bounded_task_template"),
                ("blocked_by", "live_worker_execution_not_authorized"),
            ]),
        ));
        let adjustment_decision =
            self.decide_with_receipt(&correlation, ActionKind::RecordAdjustment);
        self.transition_receipt(
            &run_id,
            "ADJUSTMENT",
            &correlation,
            &adjustment_decision,
            "loop.adjustment.recorded",
            payload(&[
                ("terminal_state", "BLOCKED_ON_LIVE_WORKER_EXECUTION"),
                (
                    "next_action",
                    "wait for local worker runtime evidence, live execution gate, provider turn-on evidence, CI evidence, and human ratification",
                ),
                ("worker_spawn_receipt_id", &spawn_receipt.receipt_id),
            ]),
        );
        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status = "BLOCKED_ON_LIVE_WORKER_EXECUTION".to_string();
        }
        self.storage.ledger().verify()?;
        let receipts = self
            .storage
            .ledger()
            .entries()
            .iter()
            .map(|entry| entry.receipt_id.clone())
            .collect();
        Ok(LoopRunReport {
            run_id,
            loop_id: "talent_autonomy_agent".to_string(),
            status: "BLOCKED_ON_LIVE_WORKER_EXECUTION".to_string(),
            score: 0,
            credential_status: "CHECKED_LOCAL_AUTHORITY".to_string(),
            receipts,
            concierge_answer: format!(
                "talent_autonomy_agent recorded sponsor chain, worker lease, budget, tool allowlist, authorization, credential check, and worker spawn request receipts, then blocked before live worker execution. Source receipts: {}",
                self.storage.ledger().query().receipt_ids().join(", ")
            ),
        })
    }

    pub fn run_local_worker_runtime(
        &mut self,
        request: WorkerRuntimeRequest,
    ) -> Result<WorkerRuntimeReport, String> {
        let spawn_receipt = self
            .storage
            .ledger()
            .entries()
            .iter()
            .find(|receipt| receipt.receipt_id == request.spawn_receipt_id)
            .cloned()
            .ok_or_else(|| "worker spawn receipt is required".to_string())?;
        if spawn_receipt.kind != "worker.spawn_requested" {
            return Err("worker runtime requires worker.spawn_requested receipt".to_string());
        }
        let credential_receipt = self
            .storage
            .ledger()
            .entries()
            .iter()
            .find(|receipt| receipt.receipt_id == request.credential_check_receipt_id)
            .cloned()
            .ok_or_else(|| "worker credential check receipt is required".to_string())?;
        if credential_receipt.kind != "worker.credential.checked" {
            return Err("worker runtime requires worker.credential.checked receipt".to_string());
        }
        let parent_loop_id = spawn_receipt
            .payload
            .get("parent_id")
            .ok_or_else(|| "worker spawn receipt missing parent_id".to_string())?;
        let worker_template_id = spawn_receipt
            .payload
            .get("worker_template_id")
            .ok_or_else(|| "worker spawn receipt missing worker_template_id".to_string())?;
        let worker_run_id = self.ids.next("worker_run");
        let output_artifacts = request
            .output_artifacts
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let verification_evidence = request
            .verification_evidence
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        admit_worker_handoff(&WorkerHandoffAdmission {
            parent_loop_id,
            worker_template_id,
            worker_run_id: &worker_run_id,
            spawn_receipt_id: &spawn_receipt.receipt_id,
            credential_check_receipt_id: &credential_receipt.receipt_id,
            output_artifacts: &output_artifacts,
            verification_evidence: &verification_evidence,
            summary: &request.summary,
            next_owner: &request.next_owner,
            requested_receipt_kind: "worker.handoff.recorded",
        })
        .map_err(|error| error.message())?;
        let correlation = CorrelationIds {
            tenant_id: spawn_receipt.tenant_id.clone(),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new("worker_bounded_task_template"),
            loop_id: LoopId::new(parent_loop_id),
            workflow_id: WorkflowId::new(worker_run_id.clone()),
        };
        let handoff_decision =
            self.decide_with_receipt(&correlation, ActionKind::RecordWorkerHandoff);
        let handoff_receipt = self.storage.append_receipt(
            &mut self.ids,
            &correlation,
            "worker.handoff.recorded",
            Some(handoff_decision.policy_decision_id.clone()),
            payload(&[
                ("parent_loop_id", parent_loop_id),
                ("worker_template_id", worker_template_id),
                ("worker_run_id", &worker_run_id),
                ("spawn_receipt_id", &spawn_receipt.receipt_id),
                (
                    "credential_check_receipt_id",
                    &credential_receipt.receipt_id,
                ),
                ("output_artifacts", &request.output_artifacts.join(",")),
                (
                    "verification_evidence",
                    &request.verification_evidence.join(","),
                ),
                ("summary", &request.summary),
                ("next_owner", &request.next_owner),
                ("requested_receipt_kind", "worker.handoff.recorded"),
            ]),
        );
        self.storage.enqueue_outbox(OutboxEvent::from_receipt(
            &mut self.ids,
            &handoff_receipt,
            "observatory.worker_handoff_ready",
            payload(&[
                ("worker_run_id", &worker_run_id),
                ("handoff_receipt_id", &handoff_receipt.receipt_id),
            ]),
        ));
        admit_worker_retirement(&WorkerRetirementAdmission {
            parent_loop_id,
            worker_template_id,
            worker_run_id: &worker_run_id,
            spawn_receipt_id: &spawn_receipt.receipt_id,
            handoff_receipt_id: &handoff_receipt.receipt_id,
            requested_receipt_kind: "worker.retired",
        })
        .map_err(|error| error.message())?;
        let retirement_decision = self.decide_with_receipt(&correlation, ActionKind::RetireWorker);
        let retirement_receipt = self.storage.append_receipt(
            &mut self.ids,
            &correlation,
            "worker.retired",
            Some(retirement_decision.policy_decision_id.clone()),
            payload(&[
                ("parent_loop_id", parent_loop_id),
                ("worker_template_id", worker_template_id),
                ("worker_run_id", &worker_run_id),
                ("spawn_receipt_id", &spawn_receipt.receipt_id),
                ("handoff_receipt_id", &handoff_receipt.receipt_id),
                ("requested_receipt_kind", "worker.retired"),
            ]),
        );
        self.storage.enqueue_outbox(OutboxEvent::from_receipt(
            &mut self.ids,
            &retirement_receipt,
            "observatory.worker_retired",
            payload(&[
                ("worker_run_id", &worker_run_id),
                ("retirement_receipt_id", &retirement_receipt.receipt_id),
            ]),
        ));
        self.storage.ledger().verify()?;
        Ok(WorkerRuntimeReport {
            worker_run_id,
            status: "RETIRED_AFTER_HANDOFF".to_string(),
            handoff_receipt_id: handoff_receipt.receipt_id,
            retirement_receipt_id: retirement_receipt.receipt_id,
            source_receipts: vec![spawn_receipt.receipt_id, credential_receipt.receipt_id],
        })
    }
    pub fn authorize_treasury(
        &mut self,
        correlation: &CorrelationIds,
        max_amount_cents: u32,
        counterparty: &str,
        purpose: &str,
    ) -> Result<TreasuryAuthorization, String> {
        let decision = self.decide_with_receipt(
            correlation,
            ActionKind::EconomicSpend {
                amount_cents: max_amount_cents,
            },
        );
        if decision.outcome != PolicyOutcome::Escalate {
            return Err("economic action did not route through escalation path".to_string());
        }
        let receipt = self.storage.append_receipt(
            &mut self.ids,
            correlation,
            "treasury.authority.granted",
            Some(decision.policy_decision_id.clone()),
            payload(&[
                ("max_amount_cents", &max_amount_cents.to_string()),
                ("counterparty", counterparty),
                ("purpose", purpose),
                ("status", "ACTIVE"),
            ]),
        );
        let authorization = TreasuryAuthorization {
            treasury_authorization_id: self.ids.next("treasury"),
            tenant_id: correlation.tenant_id.clone(),
            actor_id: correlation.actor_id.clone(),
            policy_decision_id: decision.policy_decision_id,
            max_amount_cents,
            counterparty: counterparty.to_string(),
            purpose: purpose.to_string(),
            status: "ACTIVE".to_string(),
            receipt_id: receipt.receipt_id,
        };
        self.storage
            .push_treasury_authorization(authorization.clone());
        Ok(authorization)
    }

    pub fn observatory_read_model(&self) -> ObservatoryReadModel {
        ObservatoryReadModel {
            surface: OBSERVATORY_ROLE_VIEW.surface,
            latest_run: self.storage.loop_runs().last().map(|run| LoopRunSummary {
                loop_id: run.loop_id.to_string(),
                status: run.status.clone(),
            }),
            receipt_evidence: ReceiptEvidence::from_query(self.storage.ledger().query()),
            receipt_count: self.storage.ledger().query().count(),
            policy_decision_count: self.storage.policy_decisions().len(),
            eval_verdict_count: self.storage.eval_verdicts().len(),
            credential_count: self.storage.credentials().len(),
            charter_record_count: self.storage.charter_records().len(),
            memory_record_count: self.storage.memory_records().len(),
            role_modes: OBSERVATORY_ROLE_VIEW
                .role_modes
                .iter()
                .map(RoleMode::as_str)
                .collect(),
            declared_sources: OBSERVATORY_ROLE_VIEW
                .declared_sources
                .iter()
                .map(DeclaredSource::as_str)
                .collect(),
        }
    }
    pub fn observatory_view(&self) -> String {
        self.observatory_read_model().render_text()
    }
    pub fn concierge_read_model(&self) -> ConciergeReadModel {
        let verdict = self.storage.eval_verdicts().last();
        let credential = self.storage.credentials().last();
        ConciergeReadModel {
            suite_id: verdict.map(|verdict| verdict.suite_id.clone()),
            score: verdict.map(|verdict| verdict.score),
            credential_id: credential.map(|credential| credential.credential_id.clone()),
            credential_status: credential.map(|credential| credential.status.clone()),
            receipt_evidence: ReceiptEvidence::from_query(self.storage.ledger().query()),
        }
    }

    pub fn concierge_answer(&self, _query: &str) -> String {
        self.concierge_read_model().render_text()
    }

    pub fn twin_text_response(&self, companion_id: &str) -> TwinTextResponse {
        let (companion_id, role) = match companion_id {
            "twin_architect" => ("twin_architect", "architect"),
            "twin_coder" => ("twin_coder", "coder"),
            "twin_coach" => ("twin_coach", "coach"),
            "twin_compliance" => ("twin_compliance", "compliance"),
            "twin_problem_solver" => ("twin_problem_solver", "problem_solver"),
            _ => ("twin_advisor", "advisor"),
        };
        let concierge = self.concierge_read_model();
        let answer = match (
            concierge.suite_id.as_deref(),
            concierge.score,
            concierge.credential_status.as_deref(),
        ) {
            (Some(suite_id), Some(score), Some(status)) => format!(
                "The latest local evidence says {suite_id} scored {score} and credential status is {status}. No action is available from Twin."
            ),
            _ => "No receipt-backed loop evidence is available yet, so Twin cannot advise beyond the grounding boundary.".to_string(),
        };
        TwinTextResponse {
            companion_id,
            role,
            runtime_status: "PENDING-LIVE-RUN",
            answer,
            receipt_evidence: ReceiptEvidence::from_query(self.storage.ledger().query()),
            world_model_sources: vec![
                "generated/world-model/mdx-local-world-model.json",
                "generated/companions/twin-text-grounding-contract.json",
                "generated/read-model-snapshots/concierge.json",
            ],
        }
    }

    pub fn strategy_ratification_response(&self) -> StrategyRatificationResponse {
        let concierge = self.concierge_read_model();
        let why_now = match (
            concierge.suite_id.as_deref(),
            concierge.score,
            concierge.credential_status.as_deref(),
        ) {
            (Some(suite_id), Some(score), Some(status)) => format!(
                "Latest local evidence: {suite_id} scored {score} and credential status is {status}."
            ),
            _ => "No receipt-backed loop evidence is available yet.".to_string(),
        };
        StrategyRatificationResponse {
            proposal_id: "strategy_local_ratification_001",
            runtime_status: "DECLARED-LOCAL-READ-SURFACE",
            question: "What strategic option needs human ratification before any budget, worker, Forge, or deploy path opens?",
            options: vec![
                "hold_current_direction",
                "request_more_evidence",
                "ratify_next_local_strategy_option",
            ],
            why_now,
            blocked_actions: vec![
                "set_company_direction",
                "allocate_live_budget",
                "spawn_worker",
                "handoff_to_forge",
                "production_deploy",
            ],
            ratification_required: true,
            receipt_evidence: ReceiptEvidence::from_query(self.storage.ledger().query()),
            source_contracts: vec![
                "generated/strategy/strategy-ratification-surface.json",
                "generated/read-surfaces/human-edge-read-surface-matrix.json",
                "generated/product/ratification-contract.json",
                "generated/ui/human-edge-contract.json",
            ],
        }
    }

    pub fn product_ratification_response(&self) -> ProductRatificationResponse {
        let observatory = self.observatory_read_model();
        let shaped_bet = format!(
            "Latest local loop evidence is {} with {} receipts; Product may shape a bet, but handoff waits for human ratification.",
            observatory.latest_run_label(),
            observatory.receipt_count
        );
        ProductRatificationResponse {
            bet_id: "product_bet_local_stub",
            runtime_status: "LOCAL-BLOCKED-AT-HUMAN-RATIFICATION",
            question: "What product bet needs human ratification before Strategy, Treasury, Talent, Forge, or ship paths open?",
            shaped_bet,
            required_before_ratification: vec![
                "product.signal.ingested",
                "product.bet.shaped",
                "policy.decision.recorded",
                "human_sponsor",
                "product.ratification.recorded",
            ],
            blocked_actions: vec![
                "stage_strategy_option",
                "allocate_live_budget",
                "spawn_worker",
                "handoff_to_forge",
                "ship_product_change",
            ],
            ratification_required: true,
            receipt_evidence: ReceiptEvidence::from_query(self.storage.ledger().query()),
            source_contracts: vec![
                "generated/product/ratification-contract.json",
                "generated/product/ratification-scenarios.json",
                "generated/ui/human-edge-contract.json",
            ],
        }
    }

    fn decide_with_receipt(
        &mut self,
        correlation: &CorrelationIds,
        action: ActionKind,
    ) -> PolicyDecision {
        let mut decision = self.policy.evaluate(&mut self.ids, correlation, action);
        let receipt = self.storage.append_receipt(
            &mut self.ids,
            correlation,
            POLICY_DECISION_RECEIPT_KIND,
            None,
            payload(&[
                ("policy_decision_id", &decision.policy_decision_id),
                ("action", decision.action.name()),
                ("outcome", decision.outcome.as_str()),
                ("reason", &decision.reason),
            ]),
        );
        decision.receipt_id = Some(receipt.receipt_id);
        self.storage.push_policy_decision(decision.clone());
        decision
    }

    fn transition_receipt(
        &mut self,
        run_id: &str,
        transition: &str,
        correlation: &CorrelationIds,
        decision: &PolicyDecision,
        kind: &str,
        payload: BTreeMap<String, String>,
    ) -> Receipt {
        assert_eq!(
            decision.outcome,
            PolicyOutcome::Allow,
            "local loop transitions must be explicitly allowed"
        );
        let receipt = self.storage.append_receipt(
            &mut self.ids,
            correlation,
            kind,
            Some(decision.policy_decision_id.clone()),
            payload,
        );
        self.storage.push_loop_transition(LoopTransition {
            transition_id: self.ids.next("transition"),
            run_id: run_id.to_string(),
            transition: transition.to_string(),
            policy_decision_id: decision.policy_decision_id.clone(),
            receipt_id: receipt.receipt_id.clone(),
        });
        receipt
    }

    fn mint_credential(
        &mut self,
        correlation: &CorrelationIds,
        verdict: &EvalVerdict,
        receipt: &Receipt,
    ) -> Result<AgentCredential, String> {
        if !verdict.passed {
            return Err("credential mint requires a passing eval verdict".to_string());
        }
        let credential = AgentCredential {
            credential_id: self.ids.next("credential"),
            tenant_id: correlation.tenant_id.clone(),
            actor_id: correlation.actor_id.clone(),
            eval_verdict_id: verdict.eval_verdict_id.clone(),
            status: "MINTED".to_string(),
            receipt_id: receipt.receipt_id.clone(),
        };
        self.storage.push_credential(credential.clone());
        Ok(credential)
    }

    fn record_charter_evidence(
        &mut self,
        correlation: &CorrelationIds,
        source_receipt_id: &str,
        obligation: &str,
        evidence: &str,
        receipt_id: &str,
    ) {
        self.storage.push_charter_record(CharterRecord {
            charter_record_id: self.ids.next("charter"),
            tenant_id: correlation.tenant_id.clone(),
            source_receipt_id: source_receipt_id.to_string(),
            obligation: obligation.to_string(),
            evidence: evidence.to_string(),
            receipt_id: receipt_id.to_string(),
        });
    }
}

pub fn payload(values: &[(&str, &str)]) -> BTreeMap<String, String> {
    values
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect()
}

fn normalized_tokens(value: &str) -> Vec<String> {
    value
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter_map(|token| {
            let normalized = token.trim().to_ascii_lowercase();
            if normalized.is_empty() {
                None
            } else {
                Some(normalized)
            }
        })
        .collect()
}

fn join_harness_values(values: &[&str]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(",")
    }
}

fn normalize_harness_relative_path(path: &str) -> Result<String, &'static str> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("empty_path");
    }
    if trimmed.contains('\\') {
        return Err("invalid_path_separator");
    }
    let path = Path::new(trimmed);
    if path.is_absolute() {
        return Err("absolute_path");
    }

    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => parts.push(value.to_string_lossy().to_string()),
            Component::CurDir => {}
            Component::ParentDir => return Err("parent_traversal"),
            Component::RootDir | Component::Prefix(_) => return Err("absolute_path"),
        }
    }
    if parts.is_empty() {
        return Err("empty_path");
    }
    Ok(parts.join("/"))
}

fn mediate_harness_read_path(
    normalized_path: &str,
    allowed_read_roots: &[&str],
    blocked_paths: &[&str],
) -> Result<(), &'static str> {
    for blocked in blocked_paths {
        if let Ok(root) = normalize_harness_relative_path(blocked)
            && harness_path_within_root(normalized_path, &root)
        {
            return Err("blocked_path");
        }
    }
    for allowed in allowed_read_roots {
        if let Ok(root) = normalize_harness_relative_path(allowed)
            && harness_path_within_root(normalized_path, &root)
        {
            return Ok(());
        }
    }
    Err("outside_allowed_read_scope")
}

fn mediate_harness_tool_path(
    manifest: &HarnessRunManifest<'_>,
    path: &str,
    allowed_read_roots: &[&str],
    blocked_paths: &[&str],
) -> Result<String, &'static str> {
    if manifest.permission_policy.file_read != HarnessPermissionLevel::ReadOnly {
        return Err("file_read_not_allowed");
    }
    normalize_harness_relative_path(path).and_then(|normalized| {
        mediate_harness_read_path(&normalized, allowed_read_roots, blocked_paths)
            .map(|_| normalized)
    })
}

fn mediate_harness_patch_path(
    manifest: &HarnessRunManifest<'_>,
    path: &str,
) -> Result<String, &'static str> {
    if manifest.permission_policy.patch != HarnessPermissionLevel::AllowedWithPolicy {
        return Err("patch_not_allowed");
    }
    if manifest.permission_policy.file_write != HarnessPermissionLevel::AllowedWithPolicy {
        return Err("file_write_not_allowed");
    }
    if manifest
        .blocked_tools
        .iter()
        .any(|tool| *tool == "patch" || *tool == "local_write_patch")
    {
        return Err("blocked_tool");
    }
    if !manifest.allowed_tools.contains(&"local_write_patch") {
        return Err("tool_not_allowed");
    }
    normalize_harness_relative_path(path).and_then(|normalized| {
        for blocked in manifest.blocked_paths {
            if let Ok(root) = normalize_harness_relative_path(blocked)
                && harness_path_within_root(&normalized, &root)
            {
                return Err("blocked_path");
            }
        }
        for allowed in manifest.allowed_write_scope {
            if let Ok(root) = normalize_harness_relative_path(allowed)
                && harness_path_within_root(&normalized, &root)
            {
                return Ok(normalized);
            }
        }
        Err("outside_allowed_write_scope")
    })
}

fn harness_path_within_root(path: &str, root: &str) -> bool {
    path == root
        || path
            .strip_prefix(root)
            .is_some_and(|remaining| remaining.starts_with('/'))
}

fn validate_harness_tool_request(
    manifest: &HarnessRunManifest<'_>,
    allowed_read_roots: &[&str],
    max_output_bytes: usize,
) -> Result<(), HarnessToolPlaneError> {
    admit_harness_run_manifest(manifest)
        .map_err(|rejection| HarnessToolPlaneError::ManifestRejected(rejection.message()))?;
    if allowed_read_roots.is_empty() {
        return Err(HarnessToolPlaneError::EmptyAllowedReadRoots);
    }
    if manifest.budget_policy.max_tool_calls == 0 {
        return Err(HarnessToolPlaneError::EmptyToolBudget);
    }
    if max_output_bytes == 0 {
        return Err(HarnessToolPlaneError::EmptyOutputBudget);
    }
    Ok(())
}

fn validate_harness_patch_tool_request(
    manifest: &HarnessRunManifest<'_>,
    request: &HarnessPatchToolRequest<'_>,
) -> Result<(), HarnessToolPlaneError> {
    admit_harness_run_manifest(manifest)
        .map_err(|rejection| HarnessToolPlaneError::ManifestRejected(rejection.message()))?;
    if manifest.allowed_write_scope.is_empty()
        || manifest
            .allowed_write_scope
            .iter()
            .any(|scope| scope.trim().is_empty())
    {
        return Err(HarnessToolPlaneError::EmptyAllowedWriteScope);
    }
    if manifest.budget_policy.max_tool_calls == 0 {
        return Err(HarnessToolPlaneError::EmptyToolBudget);
    }
    if request.patch.trim().is_empty() {
        return Err(HarnessToolPlaneError::EmptyPatch);
    }
    if request.max_patch_bytes == 0 {
        return Err(HarnessToolPlaneError::EmptyPatchBudget);
    }
    if request.max_output_bytes == 0 {
        return Err(HarnessToolPlaneError::EmptyOutputBudget);
    }
    for kind in ["harness.tool.patch.allowed", "harness.tool.patch.denied"] {
        if !manifest.required_receipt_kinds.contains(&kind) {
            return Err(HarnessToolPlaneError::ManifestRejected(format!(
                "harness patch tool manifest missing required receipt kind {kind}"
            )));
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct HarnessCappedOutput {
    output: String,
    truncated: bool,
}

fn cap_harness_output(contents: &str, max_output_bytes: usize) -> HarnessCappedOutput {
    if contents.len() <= max_output_bytes {
        return HarnessCappedOutput {
            output: contents.to_string(),
            truncated: false,
        };
    }

    let mut output = String::new();
    for character in contents.chars() {
        if output.len() + character.len_utf8() > max_output_bytes {
            break;
        }
        output.push(character);
    }
    HarnessCappedOutput {
        output,
        truncated: true,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct HarnessSearchCappedOutput {
    matches: Vec<HarnessSearchMatch>,
    truncated: bool,
}

fn search_harness_contents(
    contents: &str,
    query: &str,
    max_matches: usize,
    max_output_bytes: usize,
) -> HarnessSearchCappedOutput {
    let mut matches = Vec::new();
    let mut used_bytes = 0usize;
    let mut truncated = false;
    for (index, line) in contents.lines().enumerate() {
        if !line.contains(query) {
            continue;
        }
        if matches.len() >= max_matches {
            truncated = true;
            break;
        }
        let capped = cap_harness_output(line, max_output_bytes.saturating_sub(used_bytes));
        if capped.output.is_empty() && !line.is_empty() {
            truncated = true;
            break;
        }
        used_bytes += capped.output.len();
        truncated |= capped.truncated;
        matches.push(HarnessSearchMatch {
            line_number: index + 1,
            line: capped.output,
        });
        if used_bytes >= max_output_bytes {
            truncated = true;
            break;
        }
    }
    HarnessSearchCappedOutput { matches, truncated }
}

fn harness_search_output_bytes(matches: &[HarnessSearchMatch]) -> usize {
    matches.iter().map(|matched| matched.line.len()).sum()
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct HarnessListCappedOutput {
    entries: Vec<String>,
    truncated: bool,
}

fn list_harness_entries(
    entries: &[&str],
    max_entries: usize,
    max_output_bytes: usize,
) -> HarnessListCappedOutput {
    let mut listed = Vec::new();
    let mut used_bytes = 0usize;
    let mut truncated = false;
    for entry in entries.iter().take(max_entries) {
        let capped = cap_harness_output(entry, max_output_bytes.saturating_sub(used_bytes));
        if capped.output.is_empty() && !entry.is_empty() {
            truncated = true;
            break;
        }
        used_bytes += capped.output.len();
        truncated |= capped.truncated;
        listed.push(capped.output);
        if used_bytes >= max_output_bytes {
            truncated = true;
            break;
        }
    }
    if entries.len() > listed.len() {
        truncated = true;
    }
    HarnessListCappedOutput {
        entries: listed,
        truncated,
    }
}
fn harness_list_output_bytes(entries: &[String]) -> usize {
    entries.iter().map(|entry| entry.len()).sum()
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigrationReport {
    pub migration_count: usize,
    pub tenant_owned_tables: usize,
    pub rls_enabled_tables: usize,
    pub policy_definitions: usize,
    pub policy_drop_guards: usize,
}
pub fn migration_report() -> MigrationReport {
    let joined = migration_sources().join("\n");
    MigrationReport {
        migration_count: migration_sources().len(),
        // Unique tables, not text occurrences: a migration that re-declares a
        // table (CREATE IF NOT EXISTS plus ENABLE ROW LEVEL SECURITY again,
        // as the ctx vector migration does) must not double-count.
        tenant_owned_tables: unique_tenant_owned_tables(&joined),
        rls_enabled_tables: unique_rls_enabled_tables(&joined),
        policy_definitions: count_occurrences(&joined, "CREATE POLICY"),
        policy_drop_guards: count_occurrences(&joined, "DROP POLICY IF EXISTS"),
    }
}

fn unique_rls_enabled_tables(joined: &str) -> usize {
    let mut tables: Vec<&str> = joined
        .match_indices("ALTER TABLE ")
        .filter_map(|(start, _)| {
            let rest = &joined[start + "ALTER TABLE ".len()..];
            let line_end = rest.find('\n').unwrap_or(rest.len());
            let line = &rest[..line_end];
            line.strip_suffix(" ENABLE ROW LEVEL SECURITY;")
                .map(str::trim)
        })
        .collect();
    tables.sort_unstable();
    tables.dedup();
    tables.len()
}

fn unique_tenant_owned_tables(joined: &str) -> usize {
    let mut tables: Vec<&str> = joined
        .match_indices("CREATE TABLE IF NOT EXISTS ")
        .filter_map(|(start, _)| {
            let rest = &joined[start + "CREATE TABLE IF NOT EXISTS ".len()..];
            let name_end = rest.find([' ', '(', '\n'])?;
            let name = &rest[..name_end];
            let block_end = rest.find(");").unwrap_or(rest.len());
            rest[..block_end]
                .contains("tenant_id TEXT NOT NULL")
                .then_some(name)
        })
        .collect();
    tables.sort_unstable();
    tables.dedup();
    tables.len()
}

pub fn validate_migration_contracts() -> Result<MigrationReport, String> {
    let migrations = migration_sources();
    let joined = migrations.join("\n");
    // The live-path contract: named roles for both access planes and owner
    // binding. A migration set without these is not deployable.
    for marker in [
        "CREATE ROLE mdx_app NOLOGIN",
        "CREATE ROLE mdx_persist NOLOGIN",
        "FORCE ROW LEVEL SECURITY",
    ] {
        if !joined.contains(marker) {
            return Err(format!("migrations missing live-path marker: {marker}"));
        }
    }
    for table in RLS_TABLES {
        let marker = format!("ALTER TABLE {table} ENABLE ROW LEVEL SECURITY");
        if !joined.contains(&marker) {
            return Err(format!("{table} missing RLS"));
        }
    }
    for table in TENANT_OWNED_TABLES {
        let marker = format!("CREATE TABLE IF NOT EXISTS {table}");
        let start = joined
            .find(&marker)
            .ok_or_else(|| format!("{table} migration missing"))?;
        let section = &joined[start..];
        if !section.contains("tenant_id TEXT NOT NULL") {
            return Err(format!("{table} missing tenant_id"));
        }
    }
    // RLS v2/v3: each override re-declares its policy with actor/resource
    // context, so tenant membership alone is no longer enough for a private
    // resource. The policy proves resource-awareness one of three ways: it reads
    // the actor directly (mdx.actor_id), it delegates to the Pages visibility
    // function (mdx_pages_page_visible), or it inherits a parent resource's
    // policy through an EXISTS subquery. A bare tenant-only policy fails here.
    for table in RESOURCE_AWARE_RLS_OVERRIDES {
        let marker = format!("CREATE POLICY {table}_tenant_access ON {table}");
        let last = joined
            .rfind(&marker)
            .ok_or_else(|| format!("{table} missing resource-aware RLS override policy"))?;
        let section = &joined[last..];
        let end = section.find(';').unwrap_or(section.len());
        let policy = &section[..end];
        let resource_aware = policy.contains("mdx.actor_id")
            || policy.contains("mdx.actor_role")
            || policy.contains("mdx_pages_page_visible")
            || policy.contains("mdx_receipt_")
            || policy.contains("EXISTS (");
        if !resource_aware {
            return Err(format!(
                "{table} RLS override policy must use actor or resource context, not tenant alone"
            ));
        }
    }
    let report = migration_report();
    // +1: the live-path migration declares the persist-plane policy once and
    // applies it per row-secured table through a dynamic block.
    // +1 more: the ctx vector migration re-declares the ctx_memory_vectors
    // policy when it widens the embedding dimensions.
    // +26: Memory Brain, flywheel, and Model Fabric tables are created after the live-path migration and
    // therefore declare their own explicit persist-plane policies.
    let expected_policies = RLS_TABLES.len() + RESOURCE_AWARE_RLS_OVERRIDES.len() + 28;
    if report.policy_definitions != expected_policies {
        return Err(format!(
            "expected {expected_policies} RLS policy definitions, found {}",
            report.policy_definitions
        ));
    }
    if report.policy_drop_guards != report.policy_definitions {
        return Err(format!(
            "RLS policy definitions must have matching DROP POLICY IF EXISTS guards: {} definitions, {} guards",
            report.policy_definitions, report.policy_drop_guards
        ));
    }
    Ok(report)
}
#[rustfmt::skip]
pub fn migration_rls_tables() -> &'static [&'static str] { RLS_TABLES }
#[rustfmt::skip]
pub fn migration_tenant_owned_tables() -> &'static [&'static str] { TENANT_OWNED_TABLES }
#[rustfmt::skip]
fn count_occurrences(source: &str, needle: &str) -> usize { source.match_indices(needle).count() }
fn migration_sources() -> Vec<&'static str> {
    vec![
        include_str!("../../../migrations/0001_tenants.sql"),
        include_str!("../../../migrations/0002_ledger.sql"),
        include_str!("../../../migrations/0003_world_model.sql"),
        include_str!("../../../migrations/0004_loop_runtime.sql"),
        include_str!("../../../migrations/0005_receipts.sql"),
        include_str!("../../../migrations/0006_outbox.sql"),
        include_str!("../../../migrations/0007_treasury.sql"),
        include_str!("../../../migrations/0008_role_views.sql"),
        include_str!("../../../migrations/0009_app_state.sql"),
        include_str!("../../../migrations/0010_message_pages_domain.sql"),
        include_str!("../../../migrations/0011_forge_execution_ladder.sql"),
        include_str!("../../../migrations/0012_twin_conversation_substrate.sql"),
        include_str!("../../../migrations/0013_twin_memory_intelligence.sql"),
        include_str!("../../../migrations/0014_product_governance_app_state.sql"),
        include_str!("../../../migrations/0015_strategy_talent_observatory_app_state.sql"),
        include_str!("../../../migrations/0016_auth_tenant_app_state.sql"),
        include_str!("../../../migrations/0017_message_realtime_app_state.sql"),
        include_str!("../../../migrations/0018_forge_intake_app_state.sql"),
        include_str!("../../../migrations/0019_runtime_indexes.sql"),
        include_str!("../../../migrations/0020_ctx_runtime_vector_cache.sql"),
        include_str!("../../../migrations/0021_dxr_runtime_state.sql"),
        include_str!("../../../migrations/0022_dxr_durable_workflow.sql"),
        include_str!("../../../migrations/0023_ctx_session_job_runtime.sql"),
        include_str!("../../../migrations/0024_dxr_ctx_operational_context.sql"),
        include_str!("../../../migrations/0025_dxr_execution_evidence_ledger.sql"),
        include_str!("../../../migrations/0026_runtime_performance_indexes.sql"),
        include_str!("../../../migrations/0027_ctx_vector_provider_ready_dimensions.sql"),
        include_str!("../../../migrations/0027_rls_v2_resource_aware.sql"),
        include_str!("../../../migrations/0028_rls_v3_pages_resource_aware.sql"),
        include_str!("../../../migrations/0029_rls_v3_receipts_resource_aware.sql"),
        include_str!("../../../migrations/0030_rls_v3_authority_resource_aware.sql"),
        include_str!("../../../migrations/0031_rls_live_path.sql"),
        include_str!("../../../migrations/0032_trusted_receipt_time.sql"),
        include_str!("../../../migrations/0033_memory_brain_atom_metadata.sql"),
        include_str!("../../../migrations/0034_memory_brain_graph_runtime.sql"),
        include_str!("../../../migrations/0035_memory_brain_eval_lifecycle_runtime.sql"),
        include_str!("../../../migrations/0036_memory_brain_topology_runtime_events.sql"),
        include_str!("../../../migrations/0037_memory_brain_beta_readiness.sql"),
        include_str!("../../../migrations/0038_forge_marketplace_flywheel_app_state.sql"),
        include_str!("../../../migrations/0039_memory_recall_content_checksum.sql"),
        include_str!("../../../migrations/0040_memory_consolidation_adjudication.sql"),
        include_str!("../../../migrations/0041_memory_record_local_embedding.sql"),
        include_str!("../../../migrations/0042_model_fabric_runtime.sql"),
        include_str!("../../../migrations/0043_marketplace_pack_actions.sql"),
        include_str!("../../../migrations/0044_allow_macos_dmg_release_artifacts.sql"),
    ]
}
const RLS_TABLES: &[&str] = &[
    "model_provider_connections",
    "model_catalog_models",
    "model_deployments",
    "model_price_observations",
    "model_route_policies",
    "model_route_decisions",
    "model_outcomes",
    "model_adaptive_policy_versions",
    "model_adaptive_comparisons",
    "tenants",
    "actors",
    "ledger_entries",
    "ledger_chain_heads",
    "world_facts",
    "memory_records",
    "memory_graph_nodes",
    "memory_graph_edges",
    "memory_lifecycle_events",
    "memory_recall_rankings",
    "memory_brain_eval_runs",
    "memory_vendor_comparator_runs",
    "memory_surface_access",
    "memory_production_topology_checks",
    "memory_lifecycle_evaluations",
    "memory_eval_fixture_results",
    "memory_topology_runtime_events",
    "memory_benchmark_imports",
    "memory_scale_load_runs",
    "memory_cloud_turn_on_checks",
    "ctx_memory_vectors",
    "ctx_sessions",
    "ctx_session_state",
    "ctx_handoffs",
    "ctx_background_jobs",
    "dxr_runtime_jobs",
    "dxr_runtime_events",
    "dxr_model_turns",
    "dxr_worker_boundaries",
    "dxr_workflow_runs",
    "dxr_workflow_events",
    "dxr_ctx_context_inputs",
    "dxr_evidence_ledger",
    "dxr_run_dependencies",
    "dxr_tool_executions",
    "dxr_harness_verdicts",
    "loop_runs",
    "loop_transitions",
    "policy_decisions",
    "eval_verdicts",
    "agent_credentials",
    "outbox_events",
    "treasury_authorizations",
    "charter_records",
    "role_view_declarations",
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
    "pages_document_audiences",
    "twin_sessions",
    "twin_session_messages",
    "twin_memory_snapshots",
    "twin_model_traces",
    "twin_conversation_sessions",
    "twin_companion_stance_records",
    "twin_memory_retrievals",
    "twin_grounded_answers",
    "twin_conversation_summaries",
    "forge_intake_plan_requests",
    "forge_build_requests",
    "forge_build_approvals",
    "forge_workflow_plan_proofs",
    "forge_worker_authority_requests",
    "forge_preflight_records",
    "forge_talent_authorizations",
    "forge_worker_credential_checks",
    "forge_worker_spawn_preflights",
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

// RLS v2 (migration 0027) and RLS v3 (migration 0028) replace the tenant-only
// policy on these user-private tables with an actor/resource-aware one. Each
// override re-declares the policy (DROP + CREATE), so the structural contract
// expects one extra CREATE POLICY and one extra DROP guard per table on top of
// the base set. See docs/SECURITY-RLS-V2.md and docs/SECURITY-RLS-V3.md.
const RESOURCE_AWARE_RLS_OVERRIDES: &[&str] = &[
    "twin_sessions",
    "twin_session_messages",
    "message_threads",
    "message_thread_messages",
    "message_thread_participants",
    "forge_build_requests",
    // RLS v3 (migration 0028): Pages surfaces. pages_documents is owner-private
    // with explicit audiences; the child tables inherit page visibility.
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
    // RLS v3 (migration 0029): receipts inherit source-resource visibility, not
    // tenant visibility. A receipt is visible to its creator or to anyone who can
    // see a resource-aware resource it is the source of.
    "ledger_entries",
    // RLS v3 (migration 0030): authority surfaces. Owner (the source receipt
    // creator) or a governed operator/admin role; never tenant-wide by default.
    "talent_worker_lease_authorities",
    "product_bet_drafts",
    "strategy_ratification_snapshots",
    "observatory_role_view_snapshots",
    "auth_tenant_memberships",
];

const TENANT_OWNED_TABLES: &[&str] = &[
    "model_provider_connections",
    "model_catalog_models",
    "model_deployments",
    "model_price_observations",
    "model_route_policies",
    "model_route_decisions",
    "model_outcomes",
    "model_adaptive_policy_versions",
    "model_adaptive_comparisons",
    "actors",
    "ledger_entries",
    "world_facts",
    "memory_records",
    "memory_graph_nodes",
    "memory_graph_edges",
    "memory_lifecycle_events",
    "memory_recall_rankings",
    "memory_brain_eval_runs",
    "memory_vendor_comparator_runs",
    "memory_surface_access",
    "memory_production_topology_checks",
    "memory_lifecycle_evaluations",
    "memory_eval_fixture_results",
    "memory_topology_runtime_events",
    "memory_benchmark_imports",
    "memory_scale_load_runs",
    "memory_cloud_turn_on_checks",
    "ctx_memory_vectors",
    "ctx_sessions",
    "ctx_session_state",
    "ctx_handoffs",
    "ctx_background_jobs",
    "dxr_runtime_jobs",
    "dxr_runtime_events",
    "dxr_model_turns",
    "dxr_worker_boundaries",
    "dxr_workflow_runs",
    "dxr_workflow_events",
    "dxr_ctx_context_inputs",
    "dxr_evidence_ledger",
    "dxr_run_dependencies",
    "dxr_tool_executions",
    "dxr_harness_verdicts",
    "loop_runs",
    "policy_decisions",
    "eval_verdicts",
    "agent_credentials",
    "treasury_authorizations",
    "charter_records",
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
    "pages_document_audiences",
    "twin_sessions",
    "twin_session_messages",
    "twin_memory_snapshots",
    "twin_model_traces",
    "twin_conversation_sessions",
    "twin_companion_stance_records",
    "twin_memory_retrievals",
    "twin_grounded_answers",
    "twin_conversation_summaries",
    "forge_intake_plan_requests",
    "forge_build_requests",
    "forge_build_approvals",
    "forge_workflow_plan_proofs",
    "forge_worker_authority_requests",
    "forge_preflight_records",
    "forge_talent_authorizations",
    "forge_worker_credential_checks",
    "forge_worker_spawn_preflights",
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
#[cfg(test)]
mod tests;
