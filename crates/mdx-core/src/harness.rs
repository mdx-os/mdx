use super::{MdxKernel, StorageProvider};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HarnessRunMode {
    ReadOnly,
    PlanOnly,
    AskBeforeAction,
    ApprovedExecution,
    HeadlessGuarded,
    WorkerBounded,
    ReviewOnly,
    SecurityReview,
    CiRepair,
    GoalPersistent,
}

impl HarnessRunMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ReadOnly => "read_only",
            Self::PlanOnly => "plan_only",
            Self::AskBeforeAction => "ask_before_action",
            Self::ApprovedExecution => "approved_execution",
            Self::HeadlessGuarded => "headless_guarded",
            Self::WorkerBounded => "worker_bounded",
            Self::ReviewOnly => "review_only",
            Self::SecurityReview => "security_review",
            Self::CiRepair => "ci_repair",
            Self::GoalPersistent => "goal_persistent",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HarnessPermissionLevel {
    Forbidden,
    ReadOnly,
    AskFirst,
    AllowedWithPolicy,
}

impl HarnessPermissionLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Forbidden => "forbidden",
            Self::ReadOnly => "read_only",
            Self::AskFirst => "ask_first",
            Self::AllowedWithPolicy => "allowed_with_policy",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HarnessPermissionPolicy {
    pub file_read: HarnessPermissionLevel,
    pub file_write: HarnessPermissionLevel,
    pub shell: HarnessPermissionLevel,
    pub git: HarnessPermissionLevel,
    pub patch: HarnessPermissionLevel,
    pub browser: HarnessPermissionLevel,
    pub mcp: HarnessPermissionLevel,
    pub network: HarnessPermissionLevel,
}

impl HarnessPermissionPolicy {
    pub fn plan_only_local() -> Self {
        Self {
            file_read: HarnessPermissionLevel::ReadOnly,
            file_write: HarnessPermissionLevel::Forbidden,
            shell: HarnessPermissionLevel::Forbidden,
            git: HarnessPermissionLevel::Forbidden,
            patch: HarnessPermissionLevel::Forbidden,
            browser: HarnessPermissionLevel::Forbidden,
            mcp: HarnessPermissionLevel::Forbidden,
            network: HarnessPermissionLevel::Forbidden,
        }
    }

    fn plan_only_execution_closed(&self) -> bool {
        self.file_write == HarnessPermissionLevel::Forbidden
            && self.shell == HarnessPermissionLevel::Forbidden
            && self.git == HarnessPermissionLevel::Forbidden
            && self.patch == HarnessPermissionLevel::Forbidden
            && self.browser == HarnessPermissionLevel::Forbidden
            && self.mcp == HarnessPermissionLevel::Forbidden
            && self.network == HarnessPermissionLevel::Forbidden
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HarnessBudgetPolicy {
    pub max_turns: usize,
    pub max_tool_calls: usize,
    pub max_runtime_ms: usize,
    pub max_context_tokens: usize,
    pub max_output_tokens: usize,
    pub max_event_bytes: usize,
    pub max_transcript_bytes: usize,
    pub max_cost_cents: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HarnessApprovalMode {
    None,
    AskFirst,
    PerAction,
    PlanHash,
    HumanRatified,
}

impl HarnessApprovalMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::AskFirst => "ask_first",
            Self::PerAction => "per_action",
            Self::PlanHash => "plan_hash",
            Self::HumanRatified => "human_ratified",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HarnessTrustBoundary<'a> {
    pub workspace_trusted: bool,
    pub project_local_execution_allowed: bool,
    pub mcp_startup_allowed: bool,
    pub network_allowed: bool,
    pub secrets_available: bool,
    pub approval_receipt_id: Option<&'a str>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HarnessRunManifest<'a> {
    pub run_id: &'a str,
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub origin_surface: &'a str,
    pub mode: HarnessRunMode,
    pub goal_summary: &'a str,
    pub definition_of_done: &'a str,
    pub input_artifacts: &'a [&'a str],
    pub allowed_write_scope: &'a [&'a str],
    pub blocked_paths: &'a [&'a str],
    pub allowed_tools: &'a [&'a str],
    pub blocked_tools: &'a [&'a str],
    pub default_parallelism: usize,
    pub allowed_model_profiles: &'a [&'a str],
    pub provider_fail_closed: bool,
    pub allowed_context_sources: &'a [&'a str],
    pub context_redaction_required: bool,
    pub permission_policy: HarnessPermissionPolicy,
    pub budget_policy: HarnessBudgetPolicy,
    pub quality_gates: &'a [&'a str],
    pub approval_mode: HarnessApprovalMode,
    pub approval_receipt_required: bool,
    pub required_receipt_kinds: &'a [&'a str],
    pub telemetry_redaction_required: bool,
    pub exit_criteria: &'a [&'a str],
    pub trust_boundary: HarnessTrustBoundary<'a>,
    pub policy_profile: Option<&'a str>,
    pub enterprise_pack: Option<&'a str>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HarnessRunManifestRejection {
    MissingField(&'static str),
    EmptyList(&'static str),
    EmptyBudget(&'static str),
    UnsafeParallelism,
    ProviderMustFailClosed,
    RedactionRequired(&'static str),
    MissingReceiptKind(&'static str),
    PlanOnlyRequiresClosedExecutionBoundary(&'static str),
}

impl HarnessRunManifestRejection {
    pub fn message(&self) -> String {
        match self {
            Self::MissingField(field) => format!("missing harness run manifest field {field}"),
            Self::EmptyList(field) => format!("harness run manifest list {field} is empty"),
            Self::EmptyBudget(field) => format!("harness run manifest budget {field} is empty"),
            Self::UnsafeParallelism => {
                "harness run manifest default_parallelism must be 1 locally".to_string()
            }
            Self::ProviderMustFailClosed => {
                "harness run manifest provider policy must fail closed".to_string()
            }
            Self::RedactionRequired(field) => {
                format!("harness run manifest requires redaction for {field}")
            }
            Self::MissingReceiptKind(kind) => {
                format!("harness run manifest missing required receipt kind {kind}")
            }
            Self::PlanOnlyRequiresClosedExecutionBoundary(field) => {
                format!("plan-only harness run must keep {field} closed")
            }
        }
    }
}

pub fn admit_harness_run_manifest(
    manifest: &HarnessRunManifest<'_>,
) -> Result<(), HarnessRunManifestRejection> {
    for (field, value) in [
        ("run_id", manifest.run_id),
        ("tenant_id", manifest.tenant_id),
        ("actor_id", manifest.actor_id),
        ("origin_surface", manifest.origin_surface),
        ("goal_summary", manifest.goal_summary),
        ("definition_of_done", manifest.definition_of_done),
    ] {
        if value.trim().is_empty() {
            return Err(HarnessRunManifestRejection::MissingField(field));
        }
    }
    for (field, values) in [
        ("input_artifacts", manifest.input_artifacts),
        ("blocked_paths", manifest.blocked_paths),
        ("allowed_model_profiles", manifest.allowed_model_profiles),
        ("allowed_context_sources", manifest.allowed_context_sources),
        ("quality_gates", manifest.quality_gates),
        ("required_receipt_kinds", manifest.required_receipt_kinds),
        ("exit_criteria", manifest.exit_criteria),
    ] {
        if values.is_empty() || values.iter().any(|value| value.trim().is_empty()) {
            return Err(HarnessRunManifestRejection::EmptyList(field));
        }
    }
    for (field, value) in [
        ("max_turns", manifest.budget_policy.max_turns),
        ("max_runtime_ms", manifest.budget_policy.max_runtime_ms),
        (
            "max_context_tokens",
            manifest.budget_policy.max_context_tokens,
        ),
        (
            "max_output_tokens",
            manifest.budget_policy.max_output_tokens,
        ),
        ("max_event_bytes", manifest.budget_policy.max_event_bytes),
        (
            "max_transcript_bytes",
            manifest.budget_policy.max_transcript_bytes,
        ),
    ] {
        if value == 0 {
            return Err(HarnessRunManifestRejection::EmptyBudget(field));
        }
    }
    if manifest.default_parallelism != 1 {
        return Err(HarnessRunManifestRejection::UnsafeParallelism);
    }
    if !manifest.provider_fail_closed {
        return Err(HarnessRunManifestRejection::ProviderMustFailClosed);
    }
    if !manifest.context_redaction_required {
        return Err(HarnessRunManifestRejection::RedactionRequired(
            "context_policy",
        ));
    }
    if !manifest.telemetry_redaction_required {
        return Err(HarnessRunManifestRejection::RedactionRequired(
            "telemetry_policy",
        ));
    }
    for kind in [
        "harness.run.admitted",
        "harness.plan.emitted",
        "harness.write.refused",
    ] {
        if !manifest.required_receipt_kinds.contains(&kind) {
            return Err(HarnessRunManifestRejection::MissingReceiptKind(kind));
        }
    }
    if manifest.mode == HarnessRunMode::PlanOnly {
        if !manifest.permission_policy.plan_only_execution_closed() {
            return Err(
                HarnessRunManifestRejection::PlanOnlyRequiresClosedExecutionBoundary(
                    "permission_policy",
                ),
            );
        }
        for (field, value) in [
            (
                "project_local_execution_allowed",
                manifest.trust_boundary.project_local_execution_allowed,
            ),
            (
                "mcp_startup_allowed",
                manifest.trust_boundary.mcp_startup_allowed,
            ),
            ("network_allowed", manifest.trust_boundary.network_allowed),
            (
                "secrets_available",
                manifest.trust_boundary.secrets_available,
            ),
        ] {
            if value {
                return Err(
                    HarnessRunManifestRejection::PlanOnlyRequiresClosedExecutionBoundary(field),
                );
            }
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HarnessPlanItem {
    pub plan_item_id: String,
    pub summary: String,
    pub command: String,
    pub write_allowed: bool,
    pub receipt_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HarnessPlanRunReport {
    pub manifest_run_id: String,
    pub harness_run_id: String,
    pub status: String,
    pub admission_receipt_id: String,
    pub plan_items: Vec<HarnessPlanItem>,
    pub write_refusal_receipt_id: String,
    pub policy_decision_ids: Vec<String>,
    pub receipts: Vec<String>,
    pub transcript: HarnessRunTranscript,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HarnessReadToolRequest<'a> {
    pub path: &'a str,
    pub contents: &'a str,
    pub allowed_read_roots: &'a [&'a str],
    pub max_output_bytes: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HarnessReadToolReport {
    pub manifest_run_id: String,
    pub harness_run_id: String,
    pub status: String,
    pub path: String,
    pub output: String,
    pub output_bytes: usize,
    pub output_truncated: bool,
    pub denied_reason: Option<String>,
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub transcript: HarnessRunTranscript,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HarnessSearchToolRequest<'a> {
    pub path: &'a str,
    pub contents: &'a str,
    pub query: &'a str,
    pub allowed_read_roots: &'a [&'a str],
    pub max_matches: usize,
    pub max_output_bytes: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HarnessSearchMatch {
    pub line_number: usize,
    pub line: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HarnessSearchToolReport {
    pub manifest_run_id: String,
    pub harness_run_id: String,
    pub status: String,
    pub path: String,
    pub query: String,
    pub matches: Vec<HarnessSearchMatch>,
    pub output_bytes: usize,
    pub output_truncated: bool,
    pub denied_reason: Option<String>,
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub transcript: HarnessRunTranscript,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HarnessListToolRequest<'a> {
    pub path: &'a str,
    pub entries: &'a [&'a str],
    pub allowed_read_roots: &'a [&'a str],
    pub max_entries: usize,
    pub max_output_bytes: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HarnessListToolReport {
    pub manifest_run_id: String,
    pub harness_run_id: String,
    pub status: String,
    pub path: String,
    pub entries: Vec<String>,
    pub output_bytes: usize,
    pub output_truncated: bool,
    pub denied_reason: Option<String>,
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub transcript: HarnessRunTranscript,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HarnessPatchToolRequest<'a> {
    pub path: &'a str,
    pub patch: &'a str,
    pub max_patch_bytes: usize,
    pub max_output_bytes: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HarnessPatchToolReport {
    pub manifest_run_id: String,
    pub harness_run_id: String,
    pub status: String,
    pub path: String,
    pub patch_bytes: usize,
    pub patch_preview: String,
    pub output_bytes: usize,
    pub output_truncated: bool,
    pub denied_reason: Option<String>,
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub transcript: HarnessRunTranscript,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HarnessProviderKind {
    DeterministicStub,
    OpenAiModelGateway,
    AnthropicModelGateway,
    XaiModelGateway,
    GeminiModelGateway,
    MistralModelGateway,
    TensorZeroModelGateway,
}

impl HarnessProviderKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::DeterministicStub => "deterministic_stub",
            Self::OpenAiModelGateway => "openai_model_gateway",
            Self::AnthropicModelGateway => "anthropic_model_gateway",
            Self::XaiModelGateway => "xai_model_gateway",
            Self::GeminiModelGateway => "gemini_model_gateway",
            Self::MistralModelGateway => "mistral_model_gateway",
            Self::TensorZeroModelGateway => "tensorzero_model_gateway",
        }
    }

    pub fn is_live(&self) -> bool {
        !matches!(self, Self::DeterministicStub)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HarnessProviderProfile<'a> {
    pub profile_id: &'a str,
    pub provider_kind: HarnessProviderKind,
    pub model_id: &'a str,
    pub allowed_modes: &'a [&'a str],
    pub max_context_tokens: usize,
    pub max_output_tokens: usize,
    pub max_cost_cents: usize,
    pub data_retention: &'a str,
    pub context_redaction_required: bool,
    pub telemetry_redaction_required: bool,
    pub provider_fail_closed: bool,
    pub policy_profile: &'a str,
    pub enterprise_pack: &'a str,
    pub enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HarnessEnterprisePack<'a> {
    pub enterprise_pack_id: &'a str,
    pub allowed_policy_profiles: &'a [&'a str],
    pub provider_profile_allowlist: &'a [&'a str],
    pub max_context_tokens: usize,
    pub max_output_tokens: usize,
    pub max_cost_cents: usize,
    pub data_retention: &'a str,
    pub context_redaction_required: bool,
    pub telemetry_redaction_required: bool,
    pub approval_receipt_required: bool,
    pub audit_export_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HarnessProviderGatewayRequest<'a> {
    pub requested_profile_id: &'a str,
    pub requested_mode: &'a str,
    pub profiles: &'a [HarnessProviderProfile<'a>],
    pub enterprise_packs: &'a [HarnessEnterprisePack<'a>],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HarnessProviderGatewayReport {
    pub manifest_run_id: String,
    pub harness_run_id: String,
    pub status: String,
    pub requested_profile_id: String,
    pub requested_mode: String,
    pub selected_profile_id: Option<String>,
    pub provider_kind: Option<String>,
    pub model_id: Option<String>,
    pub policy_profile: Option<String>,
    pub enterprise_pack: Option<String>,
    pub live_provider_call_allowed: bool,
    pub denied_reason: Option<String>,
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub transcript: HarnessRunTranscript,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HarnessToolPlaneError {
    ManifestRejected(String),
    EmptyAllowedReadRoots,
    EmptyAllowedWriteScope,
    EmptyToolBudget,
    EmptyOutputBudget,
    EmptyPatch,
    EmptyPatchBudget,
    EmptySearchQuery,
    EmptySearchBudget,
    EmptyListBudget,
}

impl HarnessToolPlaneError {
    pub fn message(&self) -> String {
        match self {
            Self::ManifestRejected(message) => message.clone(),
            Self::EmptyAllowedReadRoots => {
                "harness read tool requires at least one allowed read root".to_string()
            }
            Self::EmptyAllowedWriteScope => {
                "harness patch tool requires explicit allowed write scope".to_string()
            }
            Self::EmptyToolBudget => "harness read tool call budget is empty".to_string(),
            Self::EmptyOutputBudget => "harness read tool output budget is empty".to_string(),
            Self::EmptyPatch => "harness patch tool patch body is empty".to_string(),
            Self::EmptyPatchBudget => "harness patch tool patch budget is empty".to_string(),
            Self::EmptySearchQuery => "harness search tool query is empty".to_string(),
            Self::EmptySearchBudget => "harness search tool match budget is empty".to_string(),
            Self::EmptyListBudget => "harness list tool entry budget is empty".to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HarnessProviderGatewayError {
    ManifestRejected(String),
}

impl HarnessProviderGatewayError {
    pub fn message(&self) -> String {
        match self {
            Self::ManifestRejected(message) => message.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HarnessRunItemKind {
    Admission,
    Plan,
    WriteRefusal,
    ToolRead,
    ToolDenied,
    ToolSearch,
    ToolList,
    ToolPatch,
    ProviderProfile,
    ProviderDenied,
    PlanHashApproval,
    PatchApplied,
    TestExecuted,
    ExecutionDenied,
    QualityGate,
    FinalVerdict,
    ForgeReceiptConsumption,
    ForgeActionDenied,
    ForgeVerdict,
    TalentWorkerReceiptConsumption,
    TalentWorkerActionDenied,
    TalentWorkerVerdict,
    ExternalMachineRequest,
    ExternalMachineAdapterDenied,
    ExternalMachineEvidenceImported,
    ExternalMachineVerdict,
}

impl HarnessRunItemKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Admission => "admission",
            Self::Plan => "plan",
            Self::WriteRefusal => "write_refusal",
            Self::ToolRead => "tool_read",
            Self::ToolDenied => "tool_denied",
            Self::ToolSearch => "tool_search",
            Self::ToolList => "tool_list",
            Self::ToolPatch => "tool_patch",
            Self::ProviderProfile => "provider_profile",
            Self::ProviderDenied => "provider_denied",
            Self::PlanHashApproval => "plan_hash_approval",
            Self::PatchApplied => "patch_applied",
            Self::TestExecuted => "test_executed",
            Self::ExecutionDenied => "execution_denied",
            Self::QualityGate => "quality_gate",
            Self::FinalVerdict => "final_verdict",
            Self::ForgeReceiptConsumption => "forge_receipt_consumption",
            Self::ForgeActionDenied => "forge_action_denied",
            Self::ForgeVerdict => "forge_verdict",
            Self::TalentWorkerReceiptConsumption => "talent_worker_receipt_consumption",
            Self::TalentWorkerActionDenied => "talent_worker_action_denied",
            Self::TalentWorkerVerdict => "talent_worker_verdict",
            Self::ExternalMachineRequest => "external_machine_request",
            Self::ExternalMachineAdapterDenied => "external_machine_adapter_denied",
            Self::ExternalMachineEvidenceImported => "external_machine_evidence_imported",
            Self::ExternalMachineVerdict => "external_machine_verdict",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HarnessRunItem {
    pub sequence: usize,
    pub kind: HarnessRunItemKind,
    pub receipt_id: String,
    pub summary: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HarnessRunTranscript {
    pub manifest_run_id: String,
    pub harness_run_id: String,
    pub replay_mode: &'static str,
    items: Vec<HarnessRunItem>,
}

impl HarnessRunTranscript {
    pub fn new(manifest_run_id: impl Into<String>, harness_run_id: impl Into<String>) -> Self {
        Self {
            manifest_run_id: manifest_run_id.into(),
            harness_run_id: harness_run_id.into(),
            replay_mode: "audit_replay",
            items: Vec::new(),
        }
    }
    pub fn append_item(
        &mut self,
        kind: HarnessRunItemKind,
        receipt_id: impl Into<String>,
        summary: impl Into<String>,
    ) -> &HarnessRunItem {
        self.items.push(HarnessRunItem {
            sequence: self.items.len() + 1,
            kind,
            receipt_id: receipt_id.into(),
            summary: summary.into(),
        });
        self.items.last().expect("just appended run item")
    }

    pub fn items(&self) -> &[HarnessRunItem] {
        &self.items
    }
    pub fn audit_replay(&self) -> Result<HarnessAuditReplayReport, HarnessTranscriptReplayError> {
        if self.items.is_empty() {
            return Err(HarnessTranscriptReplayError::EmptyTranscript);
        }
        for (index, item) in self.items.iter().enumerate() {
            let expected = index + 1;
            if item.sequence != expected {
                return Err(HarnessTranscriptReplayError::NonSequentialItem {
                    expected,
                    observed: item.sequence,
                });
            }
            if item.receipt_id.trim().is_empty() {
                return Err(HarnessTranscriptReplayError::MissingReceipt {
                    sequence: item.sequence,
                });
            }
        }
        Ok(HarnessAuditReplayReport {
            manifest_run_id: self.manifest_run_id.clone(),
            harness_run_id: self.harness_run_id.clone(),
            status: "AUDIT_REPLAYED".to_string(),
            replay_mode: self.replay_mode,
            exact_reexecution: false,
            item_count: self.items.len(),
            receipt_ids: self
                .items
                .iter()
                .map(|item| item.receipt_id.clone())
                .collect(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HarnessAuditReplayReport {
    pub manifest_run_id: String,
    pub harness_run_id: String,
    pub status: String,
    pub replay_mode: &'static str,
    pub exact_reexecution: bool,
    pub item_count: usize,
    pub receipt_ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HarnessTranscriptReplayError {
    EmptyTranscript,
    NonSequentialItem { expected: usize, observed: usize },
    MissingReceipt { sequence: usize },
}

impl HarnessTranscriptReplayError {
    pub fn message(&self) -> String {
        match self {
            Self::EmptyTranscript => "harness transcript is empty".to_string(),
            Self::NonSequentialItem { expected, observed } => {
                format!("harness transcript item sequence expected {expected} observed {observed}")
            }
            Self::MissingReceipt { sequence } => {
                format!("harness transcript item {sequence} is missing a receipt id")
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LocalHarnessPlanRunner;
impl LocalHarnessPlanRunner {
    pub fn run_plan_only<S: StorageProvider>(
        &self,
        kernel: &mut MdxKernel<S>,
        manifest: &HarnessRunManifest<'_>,
    ) -> Result<HarnessPlanRunReport, String> {
        kernel.run_harness_plan_only(manifest)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LocalHarnessReadToolPlane;

impl LocalHarnessReadToolPlane {
    pub fn read_virtual_artifact<S: StorageProvider>(
        &self,
        kernel: &mut MdxKernel<S>,
        manifest: &HarnessRunManifest<'_>,
        request: &HarnessReadToolRequest<'_>,
    ) -> Result<HarnessReadToolReport, HarnessToolPlaneError> {
        kernel.run_harness_safe_read(manifest, request)
    }
    pub fn search_virtual_artifact<S: StorageProvider>(
        &self,
        kernel: &mut MdxKernel<S>,
        manifest: &HarnessRunManifest<'_>,
        request: &HarnessSearchToolRequest<'_>,
    ) -> Result<HarnessSearchToolReport, HarnessToolPlaneError> {
        kernel.run_harness_safe_search(manifest, request)
    }
    pub fn list_virtual_artifacts<S: StorageProvider>(
        &self,
        kernel: &mut MdxKernel<S>,
        manifest: &HarnessRunManifest<'_>,
        request: &HarnessListToolRequest<'_>,
    ) -> Result<HarnessListToolReport, HarnessToolPlaneError> {
        kernel.run_harness_safe_list(manifest, request)
    }
    pub fn mediate_virtual_patch<S: StorageProvider>(
        &self,
        kernel: &mut MdxKernel<S>,
        manifest: &HarnessRunManifest<'_>,
        request: &HarnessPatchToolRequest<'_>,
    ) -> Result<HarnessPatchToolReport, HarnessToolPlaneError> {
        kernel.run_harness_safe_patch(manifest, request)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LocalHarnessProviderGateway;
impl LocalHarnessProviderGateway {
    pub fn select_profile<S: StorageProvider>(
        &self,
        kernel: &mut MdxKernel<S>,
        manifest: &HarnessRunManifest<'_>,
        request: &HarnessProviderGatewayRequest<'_>,
    ) -> Result<HarnessProviderGatewayReport, HarnessProviderGatewayError> {
        kernel.run_harness_provider_gateway(manifest, request)
    }
}
