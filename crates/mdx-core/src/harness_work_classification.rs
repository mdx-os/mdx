use crate::{
    ActionKind, ActorId, CorrelationIds, GovernedWriteIdentity, LoopId, LoopRun, MdxKernel,
    StorageProvider, TenantId, TraceId, WorkflowId, admit_local_route_actor,
    forge_fleet_benchmark_tasks, json_string_literal, payload,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeWorkClassificationRequest<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub actor_role: &'a str,
    pub classification_id: &'a str,
    pub ask: &'a str,
    pub repo: &'a str,
    pub declared_checks: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeWorkClassificationDraft {
    pub recommended_shape: String,
    pub recommended_width: u32,
    pub recommendation_label: String,
    pub recommendation_reason: String,
    pub next_route: String,
    pub next_gate: String,
    pub adjustable_fields: String,
    pub new_project_supported: bool,
    pub task_class: String,
    pub complexity_tier: String,
    pub confidence_pct: u32,
    pub rationale: String,
    pub suggested_checks: String,
    pub allowed_write_scope: String,
    pub blocked_paths: String,
    pub mission_recommended: bool,
    pub run_route: String,
    pub mission_route: String,
    pub scoreboard_route: String,
    pub dashboard_route: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeWorkClassificationReport {
    pub status: &'static str,
    pub classification_id: String,
    pub classification_receipt_id: String,
    pub policy_decision_id: String,
    pub ask: String,
    pub repo: String,
    pub recommended_shape: String,
    pub recommended_width: u32,
    pub recommendation_label: String,
    pub recommendation_reason: String,
    pub next_route: String,
    pub next_gate: String,
    pub adjustable_fields: String,
    pub new_project_supported: bool,
    pub task_class: String,
    pub complexity_tier: String,
    pub confidence_pct: u32,
    pub rationale: String,
    pub suggested_checks: String,
    pub allowed_write_scope: String,
    pub blocked_paths: String,
    pub mission_recommended: bool,
    pub run_route: String,
    pub mission_route: String,
    pub scoreboard_route: String,
    pub dashboard_route: String,
    pub classification_grants_execution_authority: bool,
    pub live_provider_calls_allowed: bool,
    pub adapter_execution_allowed: bool,
    pub production_write_allowed: bool,
    pub blocked_reason: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeWorkClassificationPacket {
    pub classification_id: String,
    pub classification_receipt_id: String,
    pub policy_decision_id: String,
    pub ask: String,
    pub repo: String,
    pub recommended_shape: String,
    pub recommended_width: u32,
    pub recommendation_label: String,
    pub recommendation_reason: String,
    pub next_route: String,
    pub next_gate: String,
    pub adjustable_fields: String,
    pub new_project_supported: bool,
    pub task_class: String,
    pub complexity_tier: String,
    pub confidence_pct: u32,
    pub rationale: String,
    pub suggested_checks: String,
    pub allowed_write_scope: String,
    pub blocked_paths: String,
    pub mission_recommended: bool,
    pub run_route: String,
    pub mission_route: String,
    pub scoreboard_route: String,
    pub dashboard_route: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeWorkClassificationProjection {
    pub status: &'static str,
    pub classification_count: u32,
    pub mission_recommended_count: u32,
    pub run_recommended_count: u32,
    pub live_provider_calls_allowed: bool,
    pub adapter_execution_allowed: bool,
    pub production_write_allowed: bool,
    pub packets: Vec<ForgeWorkClassificationPacket>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ForgeWorkClassificationError {
    Missing(&'static str),
    ActorAdmission(String),
}

impl ForgeWorkClassificationError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("forge work classification missing {field}"),
            Self::ActorAdmission(message) => message.clone(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LocalForgeWorkClassifier;

impl LocalForgeWorkClassifier {
    pub fn classify(&self, ask: &str, declared_checks: &str) -> ForgeWorkClassificationDraft {
        classify_forge_work(ask, declared_checks)
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn classify_forge_work_local(
        &mut self,
        request: ForgeWorkClassificationRequest<'_>,
    ) -> Result<ForgeWorkClassificationReport, ForgeWorkClassificationError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.classify_forge_work_local_with_identity(request, &identity)
    }

    pub fn classify_forge_work_local_with_identity(
        &mut self,
        request: ForgeWorkClassificationRequest<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<ForgeWorkClassificationReport, ForgeWorkClassificationError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("actor_role", request.actor_role),
            ("classification_id", request.classification_id),
            ("ask", request.ask),
            ("repo", request.repo),
        ] {
            if value.trim().is_empty() {
                return Err(ForgeWorkClassificationError::Missing(field));
            }
        }
        let actor_admission = admit_local_route_actor(
            request.tenant_id,
            request.actor_id,
            request.actor_role,
            "/forge/work-classifications.json",
            "forge.work_classification.recorded",
            request.classification_id,
        )
        .map_err(|error| ForgeWorkClassificationError::ActorAdmission(error.message()))?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(request.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(request.actor_id),
            loop_id: LoopId::new("forge_work_classification"),
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
        let mut draft = classify_forge_work(request.ask, request.declared_checks);
        // The benchmark classifier's fallback scopes describe the implicit
        // first-party MDx repo. A connected repository is external even when
        // its chosen id is literally "mdx", so the governed connection
        // ledger, not the display name alone, decides whether those fallback
        // paths are relevant. External runs discover and record their concrete
        // file boundary from that repository before execution.
        let implicit_first_party_repo = request.repo.eq_ignore_ascii_case("mdx")
            && self.forge_repo_root(request.repo).is_none();
        if !implicit_first_party_repo {
            draft.allowed_write_scope.clear();
        }
        let decision = self.decide_with_receipt(&correlation, ActionKind::ClassifyForgeWork);
        let confidence_pct = draft.confidence_pct.to_string();
        let mission_recommended = bool_string(draft.mission_recommended);
        let receipt = self.transition_receipt(
            &run_id,
            "CLASSIFY_FORGE_WORK",
            &correlation,
            &decision,
            "forge.work_classification.recorded",
            payload(&[
                ("classification_id", request.classification_id),
                ("source_route", "/forge/work-classifications.json"),
                (
                    "projection_route",
                    "/forge/work-classifications/projection.json",
                ),
                ("run_route", &draft.run_route),
                ("mission_route", &draft.mission_route),
                ("scoreboard_route", &draft.scoreboard_route),
                ("dashboard_route", &draft.dashboard_route),
                ("source_contract", "docs/FORGE-WORK-CLASSIFICATION.md"),
                (
                    "architecture_contract",
                    "generated/architecture/forge-work-classification.json",
                ),
                ("ask", request.ask),
                ("repo", request.repo),
                ("declared_checks", request.declared_checks),
                ("recommended_shape", &draft.recommended_shape),
                ("recommended_width", &draft.recommended_width.to_string()),
                ("recommendation_label", &draft.recommendation_label),
                ("recommendation_reason", &draft.recommendation_reason),
                ("next_route", &draft.next_route),
                ("next_gate", &draft.next_gate),
                ("adjustable_fields", &draft.adjustable_fields),
                (
                    "new_project_supported",
                    bool_string(draft.new_project_supported),
                ),
                ("task_class", &draft.task_class),
                ("complexity_tier", &draft.complexity_tier),
                ("confidence_pct", &confidence_pct),
                ("rationale", &draft.rationale),
                ("suggested_checks", &draft.suggested_checks),
                ("allowed_write_scope", &draft.allowed_write_scope),
                ("blocked_paths", &draft.blocked_paths),
                ("mission_recommended", mission_recommended),
                ("classification_grants_execution_authority", "false"),
                ("live_provider_calls_allowed", "false"),
                ("adapter_execution_allowed", "false"),
                ("production_write_allowed", "false"),
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
                (
                    "blocked_reason",
                    "classification_recorded_waiting_for_operator_confirmation",
                ),
            ]),
        );
        self.finish_forge_work_classification_run(&run_id);
        Ok(ForgeWorkClassificationReport {
            status: "FORGE_WORK_CLASSIFICATION_RECORDED",
            classification_id: request.classification_id.to_string(),
            classification_receipt_id: receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            ask: request.ask.to_string(),
            repo: request.repo.to_string(),
            recommended_shape: draft.recommended_shape,
            recommended_width: draft.recommended_width,
            recommendation_label: draft.recommendation_label,
            recommendation_reason: draft.recommendation_reason,
            next_route: draft.next_route,
            next_gate: draft.next_gate,
            adjustable_fields: draft.adjustable_fields,
            new_project_supported: draft.new_project_supported,
            task_class: draft.task_class,
            complexity_tier: draft.complexity_tier,
            confidence_pct: draft.confidence_pct,
            rationale: draft.rationale,
            suggested_checks: draft.suggested_checks,
            allowed_write_scope: draft.allowed_write_scope,
            blocked_paths: draft.blocked_paths,
            mission_recommended: draft.mission_recommended,
            run_route: draft.run_route,
            mission_route: draft.mission_route,
            scoreboard_route: draft.scoreboard_route,
            dashboard_route: draft.dashboard_route,
            classification_grants_execution_authority: false,
            live_provider_calls_allowed: false,
            adapter_execution_allowed: false,
            production_write_allowed: false,
            blocked_reason: "classification_recorded_waiting_for_operator_confirmation",
        })
    }

    pub fn project_forge_work_classifications(&self) -> ForgeWorkClassificationProjection {
        let packets = self
            .ledger()
            .entries()
            .iter()
            .filter(|receipt| receipt.kind == "forge.work_classification.recorded")
            .map(|receipt| ForgeWorkClassificationPacket {
                classification_id: payload_value(receipt, "classification_id").to_string(),
                classification_receipt_id: receipt.receipt_id.clone(),
                policy_decision_id: receipt.policy_decision_id.clone().unwrap_or_default(),
                ask: payload_value(receipt, "ask").to_string(),
                repo: payload_value(receipt, "repo").to_string(),
                recommended_shape: payload_value(receipt, "recommended_shape").to_string(),
                recommended_width: payload_value(receipt, "recommended_width")
                    .parse()
                    .unwrap_or(1),
                recommendation_label: payload_value(receipt, "recommendation_label").to_string(),
                recommendation_reason: payload_value(receipt, "recommendation_reason").to_string(),
                next_route: payload_value(receipt, "next_route").to_string(),
                next_gate: payload_value(receipt, "next_gate").to_string(),
                adjustable_fields: payload_value(receipt, "adjustable_fields").to_string(),
                new_project_supported: payload_value(receipt, "new_project_supported") == "true",
                task_class: payload_value(receipt, "task_class").to_string(),
                complexity_tier: payload_value(receipt, "complexity_tier").to_string(),
                confidence_pct: payload_value(receipt, "confidence_pct")
                    .parse()
                    .unwrap_or(0),
                rationale: payload_value(receipt, "rationale").to_string(),
                suggested_checks: payload_value(receipt, "suggested_checks").to_string(),
                allowed_write_scope: payload_value(receipt, "allowed_write_scope").to_string(),
                blocked_paths: payload_value(receipt, "blocked_paths").to_string(),
                mission_recommended: payload_value(receipt, "mission_recommended") == "true",
                run_route: payload_value(receipt, "run_route").to_string(),
                mission_route: payload_value(receipt, "mission_route").to_string(),
                scoreboard_route: payload_value(receipt, "scoreboard_route").to_string(),
                dashboard_route: payload_value(receipt, "dashboard_route").to_string(),
            })
            .collect::<Vec<_>>();
        let mission_recommended_count = packets
            .iter()
            .filter(|packet| packet.mission_recommended)
            .count() as u32;
        let run_recommended_count = packets.len() as u32 - mission_recommended_count;
        ForgeWorkClassificationProjection {
            status: if packets.is_empty() {
                "NO_WORK_CLASSIFICATIONS_RECORDED"
            } else {
                "LIVE-LOCAL-WORK-CLASSIFICATION-PROJECTION"
            },
            classification_count: packets.len() as u32,
            mission_recommended_count,
            run_recommended_count,
            live_provider_calls_allowed: false,
            adapter_execution_allowed: false,
            production_write_allowed: false,
            packets,
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    fn finish_forge_work_classification_run(&mut self, run_id: &str) {
        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status = "FORGE_WORK_CLASSIFICATION_RECORDED".to_string();
        }
    }
}

pub fn classify_forge_work(ask: &str, declared_checks: &str) -> ForgeWorkClassificationDraft {
    let normalized = ask.to_ascii_lowercase();
    let class = detect_task_class(&normalized);
    let task = forge_fleet_benchmark_tasks()
        .into_iter()
        .find(|task| task.class == class)
        .unwrap_or_else(|| {
            forge_fleet_benchmark_tasks()
                .into_iter()
                .find(|task| task.class == "feature")
                .expect("feature task declared")
        });
    let complexity_tier = detect_complexity_tier(&normalized, task.complexity_tier);
    let mission_recommended = should_recommend_mission(&normalized, class, complexity_tier);
    let recommended_width =
        recommended_width_for(&normalized, class, complexity_tier, mission_recommended);
    let next_route = next_route_for(mission_recommended, recommended_width);
    let next_gate = next_gate_for(mission_recommended, recommended_width);
    let suggested_checks = suggested_checks_for(&normalized, declared_checks, task.expected_check);
    let allowed_write_scope = allowed_write_scope_for(&normalized, task.allowed_scope);
    ForgeWorkClassificationDraft {
        recommended_shape: if mission_recommended {
            "mission"
        } else {
            "run"
        }
        .to_string(),
        recommended_width,
        recommendation_label: recommendation_label_for(mission_recommended, recommended_width)
            .to_string(),
        recommendation_reason: recommendation_reason_for(
            class,
            complexity_tier,
            mission_recommended,
            recommended_width,
        )
        .to_string(),
        next_route: next_route.to_string(),
        next_gate: next_gate.to_string(),
        adjustable_fields: "checks,write_scope,width,budget,model_roles,review_depth".to_string(),
        new_project_supported: false,
        task_class: class.to_string(),
        complexity_tier: complexity_tier.to_string(),
        confidence_pct: confidence_for(&normalized, class, mission_recommended),
        rationale: rationale_for(class, complexity_tier, mission_recommended).to_string(),
        suggested_checks,
        allowed_write_scope,
        blocked_paths:
            ".git/,.env,.mdx-local/,provider secrets,production state,live adapter execution"
                .to_string(),
        mission_recommended,
        run_route: "/forge/runs.json".to_string(),
        mission_route: "/forge/long-horizon-missions.json".to_string(),
        scoreboard_route: "/forge/fleet-eval-scoreboard.json".to_string(),
        dashboard_route: "/forge/long-horizon-mission-dashboard.json".to_string(),
    }
}

pub fn forge_work_classification_packets_json(
    projection: &ForgeWorkClassificationProjection,
) -> String {
    projection
        .packets
        .iter()
        .map(forge_work_classification_packet_json)
        .collect::<Vec<_>>()
        .join(",")
}

pub fn forge_work_classification_packet_json(packet: &ForgeWorkClassificationPacket) -> String {
    format!(
        r#"{{"classification_id":{},"classification_receipt_id":{},"policy_decision_id":{},"ask":{},"repo":{},"recommended_shape":{},"recommended_width":{},"recommendation_label":{},"recommendation_reason":{},"next_route":{},"next_gate":{},"adjustable_fields":{},"new_project_supported":{},"task_class":{},"complexity_tier":{},"confidence_pct":{},"rationale":{},"suggested_checks":{},"allowed_write_scope":{},"blocked_paths":{},"mission_recommended":{},"run_route":{},"mission_route":{},"scoreboard_route":{},"dashboard_route":{}}}"#,
        json_string_literal(&packet.classification_id),
        json_string_literal(&packet.classification_receipt_id),
        json_string_literal(&packet.policy_decision_id),
        json_string_literal(&packet.ask),
        json_string_literal(&packet.repo),
        json_string_literal(&packet.recommended_shape),
        packet.recommended_width,
        json_string_literal(&packet.recommendation_label),
        json_string_literal(&packet.recommendation_reason),
        json_string_literal(&packet.next_route),
        json_string_literal(&packet.next_gate),
        json_string_literal(&packet.adjustable_fields),
        packet.new_project_supported,
        json_string_literal(&packet.task_class),
        json_string_literal(&packet.complexity_tier),
        packet.confidence_pct,
        json_string_literal(&packet.rationale),
        json_string_literal(&packet.suggested_checks),
        json_string_literal(&packet.allowed_write_scope),
        json_string_literal(&packet.blocked_paths),
        packet.mission_recommended,
        json_string_literal(&packet.run_route),
        json_string_literal(&packet.mission_route),
        json_string_literal(&packet.scoreboard_route),
        json_string_literal(&packet.dashboard_route),
    )
}

fn detect_task_class(normalized: &str) -> &'static str {
    if explicit_docs_only(normalized) {
        "docs_code"
    } else if contains_any(
        normalized,
        &[
            "long horizon",
            "long-running",
            "multi-stage",
            "meaty",
            "greenfield",
            "brownfield",
            "whole app",
            "end to end",
        ],
    ) {
        "long_horizon"
    } else if has_security_cue(normalized) {
        "security"
    } else if contains_any(
        normalized,
        &[
            "ci",
            "github action",
            "build failing",
            "test failing",
            "pipeline",
        ],
    ) {
        "ci_repair"
    } else if contains_any(
        normalized,
        &[
            "multi-file",
            "many files",
            "high complexity",
            "high-complexity",
            "high/wide",
            "high or wide",
            "wide work",
            "wide run",
            "wide runs",
            "10+ files",
            "ten files",
            "dozens of files",
            "hundreds of lines",
            "100s of lines",
            "serious work",
            "serious serious",
            "cross-module",
        ],
    ) {
        "multi_file"
    } else if contains_any(normalized, &["bug", "fix", "error", "regression"]) {
        "bug_fix"
    } else if contains_any(
        normalized,
        &["refactor", "cleanup", "restructure", "untangle"],
    ) {
        "refactor"
    } else if contains_any(normalized, &["doc", "readme", "documentation"]) {
        "docs_code"
    } else if contains_any(normalized, &["migration", "migrate", "backfill", "schema"]) {
        "migration"
    } else if contains_any(
        normalized,
        &["performance", "latency", "perf", "slow", "allocation"],
    ) {
        "performance"
    } else if contains_any(normalized, &["concurrency", "race", "deadlock"]) {
        "concurrency"
    } else if contains_any(
        normalized,
        &["observability", "telemetry", "logs", "tracing"],
    ) {
        "observability"
    } else if contains_any(
        normalized,
        &[
            "ui",
            "ux",
            "svelte",
            "page",
            "modal",
            "copy",
            "polish",
            "readability",
            "surface",
        ],
    ) {
        "product_ux"
    } else if contains_any(
        normalized,
        &["architecture", "contract", "boundary", "adr", "harness"],
    ) {
        "architecture"
    } else if contains_any(
        normalized,
        &["api", "compat", "backward compatible", "route"],
    ) {
        "api_compat"
    } else if contains_any(normalized, &["feature", "add ", "implement", "build"]) {
        "feature"
    } else {
        "bug_fix"
    }
}

fn detect_complexity_tier(normalized: &str, default_tier: &'static str) -> &'static str {
    if explicit_docs_only(normalized)
        && contains_any(
            normalized,
            &["small", "tiny", "one sentence", "two sentence", "clarify"],
        )
    {
        "small"
    } else if contains_any(
        normalized,
        &[
            "extreme",
            "mission-scale",
            "massive",
            "messy enterprise",
            "multi-week",
            "program",
            "decompose",
            "decomposition",
            "break into",
            "break down",
            "divide and conquer",
            "orchestrate",
            "integrate the work",
            "integration agent",
            "repair agent",
            "multiple fleets",
            "fleet of fleets",
            "96+",
        ],
    ) {
        "extreme"
    } else if contains_any(
        normalized,
        &[
            "greenfield",
            "brownfield",
            "long horizon",
            "long-running",
            "multi-stage",
            "hundreds of agents",
            "whole app",
        ],
    ) {
        "xl"
    } else if contains_any(
        normalized,
        &[
            "large",
            "high complexity",
            "high-complexity",
            "high/wide",
            "high or wide",
            "wide work",
            "wide run",
            "wide runs",
            "multi-file",
            "many files",
            "10+ files",
            "ten files",
            "dozens of files",
            "hundreds of lines",
            "100s of lines",
            "serious work",
            "serious serious",
            "cross-module",
            "architecture",
            "migration",
        ],
    ) {
        "large"
    } else if contains_any(
        normalized,
        &[
            "small",
            "simple",
            "tiny",
            "hello world",
            "single-file",
            "single file",
            "one-file",
            "one file",
            "typo",
        ],
    ) {
        "small"
    } else if contains_any(normalized, &["medium", "refactor", "feature", "workflow"]) {
        "medium"
    } else {
        default_tier
    }
}

fn should_recommend_mission(normalized: &str, class: &str, complexity_tier: &str) -> bool {
    let explicit_mission_ask = contains_any(
        normalized,
        &[
            "forge mission",
            "long horizon mission",
            "long-horizon mission",
            "set up a mission",
            "create a mission",
            "shape a mission",
            "mission-scale",
        ],
    );
    complexity_tier == "large"
        || complexity_tier == "xl"
        || complexity_tier == "extreme"
        || matches!(
            class,
            "long_horizon" | "architecture" | "migration" | "concurrency"
        )
        || explicit_mission_ask
        || contains_any(
            normalized,
            &[
                "checkpoint",
                "milestone",
                "tens of agents",
                "hundreds of agents",
                "end to end",
            ],
        )
}

fn recommended_width_for(
    normalized: &str,
    class: &str,
    complexity_tier: &str,
    mission_recommended: bool,
) -> u32 {
    let explicit = if contains_any(normalized, &["96+", "96 agents", "96 lanes"]) {
        Some(96)
    } else if contains_any(normalized, &["48 agents", "48 lanes"]) {
        Some(48)
    } else if contains_any(normalized, &["24 agents", "24 lanes"]) {
        Some(24)
    } else if contains_any(normalized, &["16 agents", "16 lanes", "sixteen"]) {
        Some(16)
    } else if contains_any(normalized, &["8 agents", "8 lanes", "eight"]) {
        Some(8)
    } else if contains_any(normalized, &["4 agents", "4 lanes", "four"]) {
        Some(4)
    } else if contains_any(normalized, &["2 agents", "2 lanes", "two"]) {
        Some(2)
    } else {
        None
    };
    if let Some(width) = explicit {
        return width.min(128);
    }
    if !mission_recommended
        && contains_any(
            normalized,
            &["fleet", "parallel candidates", "competing candidates"],
        )
    {
        return tier_width_for(class, complexity_tier, mission_recommended).max(8);
    }
    tier_width_for(class, complexity_tier, mission_recommended)
}

fn tier_width_for(class: &str, complexity_tier: &str, mission_recommended: bool) -> u32 {
    match complexity_tier {
        "extreme" => 16,
        "xl" => 8,
        "large" => {
            if mission_recommended {
                4
            } else {
                3
            }
        }
        "medium" => {
            if matches!(
                class,
                "multi_file" | "refactor" | "product_ux" | "performance" | "concurrency"
            ) {
                2
            } else {
                1
            }
        }
        _ => 1,
    }
}

fn recommendation_label_for(mission_recommended: bool, recommended_width: u32) -> &'static str {
    if mission_recommended {
        "A mission"
    } else if recommended_width == 1 {
        "A single run"
    } else if recommended_width <= 4 {
        "Parallel lanes"
    } else {
        "A fleet"
    }
}

fn recommendation_reason_for(
    class: &str,
    complexity_tier: &str,
    mission_recommended: bool,
    recommended_width: u32,
) -> &'static str {
    if mission_recommended {
        return "This looks broad enough to shape milestones, gates, and repair checkpoints before workers start.";
    }
    if recommended_width > 4 {
        return "This looks separable enough for a fleet to explore and compare candidate work before review.";
    }
    if recommended_width > 1 {
        return "This looks bounded but cross-cutting, so a few parallel lanes can explore safer options.";
    }
    match (class, complexity_tier) {
        ("bug_fix", _) => "This looks like a bounded fix with one focused proof path.",
        ("docs_code", _) => "This looks bounded enough for one run with a clear check.",
        _ => "This looks narrow enough for one worker to build and prove before review.",
    }
}

fn next_route_for(mission_recommended: bool, recommended_width: u32) -> &'static str {
    if mission_recommended {
        "/forge/long-horizon-missions.json"
    } else if recommended_width > 4 {
        "/forge/fleet-plans.json"
    } else {
        "/forge/runs.json"
    }
}

fn next_gate_for(mission_recommended: bool, recommended_width: u32) -> &'static str {
    if mission_recommended {
        "mission_admission"
    } else if recommended_width > 4 {
        "fleet_plan_ratification"
    } else {
        "run_start"
    }
}

fn confidence_for(normalized: &str, class: &str, mission_recommended: bool) -> u32 {
    let class_hit = class != "bug_fix" || contains_any(normalized, &["bug", "fix", "error"]);
    match (class_hit, mission_recommended) {
        (true, true) => 86,
        (true, false) => 82,
        (false, true) => 72,
        (false, false) => 68,
    }
}

fn rationale_for(class: &str, complexity_tier: &str, mission_recommended: bool) -> &'static str {
    match (class, complexity_tier, mission_recommended) {
        ("long_horizon", _, _) => {
            "matched long-horizon language, so Forge should shape milestones before work starts"
        }
        (_, "extreme", _) => {
            "matched extreme work cues, so Forge should decompose the mission, fan out workers, integrate results, and keep repair checkpoints explicit"
        }
        (_, "xl", _) => {
            "matched extra-large work cues, so Forge should use the mission cockpit flow"
        }
        (_, "large", true) => {
            "matched large work cues, so Forge should confirm scope and checkpoints first"
        }
        (_, _, true) => {
            "matched mission-style coordination cues, so Forge should confirm the mission packet first"
        }
        _ => "matched a bounded work class, so Forge can start with the normal run flow",
    }
}

fn suggested_checks_for(normalized: &str, declared_checks: &str, fallback_check: &str) -> String {
    let declared = declared_checks.trim();
    if !declared.is_empty() {
        return declared.to_string();
    }
    if native_macos_work(normalized) {
        return "make native-macos-operator-check".to_string();
    }
    fallback_check.to_string()
}

fn allowed_write_scope_for(normalized: &str, fallback_scope: &str) -> String {
    if native_macos_work(normalized) {
        return "apps/mdx-operator-macos/".to_string();
    }
    if explicit_docs_only(normalized) {
        let paths = explicit_document_paths(normalized);
        if !paths.is_empty() {
            return paths.join(",");
        }
    }
    fallback_scope.to_string()
}

fn explicit_docs_only(normalized: &str) -> bool {
    contains_any(
        normalized,
        &[
            "documentation-only",
            "documentation only",
            "docs-only",
            "docs only",
        ],
    )
}

fn explicit_document_paths(normalized: &str) -> Vec<String> {
    let mut paths = Vec::new();
    for token in normalized.split_whitespace() {
        let path = token.trim_matches(|ch: char| {
            !ch.is_ascii_alphanumeric() && !matches!(ch, '.' | '_' | '-' | '/')
        });
        if path.is_empty()
            || path.starts_with('/')
            || path.contains("..")
            || !(path.ends_with(".md") || path.ends_with(".mdx"))
            || !path
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-' | '/'))
        {
            continue;
        }
        let path = path.to_string();
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    paths
}

fn native_macos_work(normalized: &str) -> bool {
    contains_any(
        normalized,
        &[
            "macos",
            "mac app",
            "native mac",
            "swiftui",
            "swift ",
            "mdx-operator-macos",
            "operator mac",
        ],
    )
}

fn has_security_cue(normalized: &str) -> bool {
    let tokens = normalized
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    tokens.iter().any(|token| {
        matches!(
            *token,
            "security"
                | "auth"
                | "authentication"
                | "authorization"
                | "authorize"
                | "rls"
                | "secret"
                | "secrets"
                | "xss"
                | "csrf"
        )
    })
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

fn bool_string(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

fn payload_value<'a>(receipt: &'a crate::Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}
