// A marketplace act - install, approval, review, revocation, try,
// import candidate, pack install - recorded at the same governance bar
// as every other governed local write: actor admission, a policy
// decision, and a real ledger receipt. The actor is REQUIRED - the
// kernel never invents who acted.
use crate::*;

pub const MARKETPLACE_ACT_KINDS: &[&str] = &[
    "install",
    "approval",
    "review",
    "revocation",
    "try",
    "import_candidate",
    "import_scan",
    "pack_install",
    "pack_configure",
    "pack_connect",
    "pack_authorize",
    "pack_enable",
    "pack_disable",
    "pack_update",
    "pack_rollback",
    "pack_rescope",
    "pack_remove",
    "pack_try",
    "pack_use",
    "pack_request",
    "pack_propose",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarketplaceAct<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub actor_role: &'a str,
    pub act: &'a str,
    pub source_route: &'a str,
    pub capability_id: &'a str,
    pub source_record_id: &'a str,
    pub scope: &'a str,
    pub decision: &'a str,
    pub read_only: bool,
    pub pack_id: &'a str,
    pub pack_version: &'a str,
    pub previous_version: &'a str,
    pub application_targets: &'a str,
    pub return_to: &'a str,
    pub origin_surface: &'a str,
    pub origin_object_id: &'a str,
    pub readiness_state: &'a str,
    pub config_status: &'a str,
    pub outcome: &'a str,
    pub task_class: &'a str,
    pub change_summary: &'a str,
    pub permission_diff: &'a str,
    pub source_lane: &'a str,
    pub url: &'a str,
    pub note: &'a str,
    pub reason: &'a str,
    pub scan_status: &'a str,
    pub prompt_injection_scan: &'a str,
    pub signature_scan: &'a str,
    pub sbom_scan: &'a str,
    pub checksum: &'a str,
    pub scan_findings: &'a str,
    pub items_added: &'a str,
    pub items_held: &'a str,
    pub item_record_ids: &'a str,
}

impl Default for MarketplaceAct<'_> {
    fn default() -> Self {
        MarketplaceAct {
            tenant_id: "local_tenant",
            actor_id: "",
            actor_role: "owner",
            act: "",
            source_route: "",
            capability_id: "",
            source_record_id: "",
            scope: "",
            decision: "",
            read_only: false,
            pack_id: "",
            pack_version: "",
            previous_version: "",
            application_targets: "",
            return_to: "",
            origin_surface: "",
            origin_object_id: "",
            readiness_state: "",
            config_status: "",
            outcome: "",
            task_class: "",
            change_summary: "",
            permission_diff: "",
            source_lane: "",
            url: "",
            note: "",
            reason: "",
            scan_status: "",
            prompt_injection_scan: "",
            signature_scan: "",
            sbom_scan: "",
            checksum: "",
            scan_findings: "",
            items_added: "",
            items_held: "",
            item_record_ids: "",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarketplaceActReport {
    pub status: &'static str,
    pub act_receipt_id: String,
    pub policy_decision_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarketplaceActError {
    Missing(&'static str),
    UnknownAct(String),
    ActorAdmission(String),
}

impl MarketplaceActError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("marketplace act missing {field}"),
            Self::UnknownAct(act) => format!("marketplace act kind {act} is not declared"),
            Self::ActorAdmission(message) => message.clone(),
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn save_marketplace_act(
        &mut self,
        request: MarketplaceAct<'_>,
    ) -> Result<MarketplaceActReport, MarketplaceActError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("actor_role", request.actor_role),
            ("source_route", request.source_route),
        ] {
            if value.trim().is_empty() {
                return Err(MarketplaceActError::Missing(field));
            }
        }
        if !MARKETPLACE_ACT_KINDS.contains(&request.act) {
            return Err(MarketplaceActError::UnknownAct(request.act.to_string()));
        }
        let actor_admission = admit_local_route_actor(
            request.tenant_id,
            request.actor_id,
            request.actor_role,
            request.source_route,
            "marketplace.act.recorded",
            request.act,
        )
        .map_err(|error| MarketplaceActError::ActorAdmission(error.message()))?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(request.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(request.actor_id),
            loop_id: LoopId::new("marketplace_act"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let decision = self.decide_with_receipt(&correlation, ActionKind::RecordMarketplaceAct);
        let read_only = if request.read_only { "true" } else { "false" };
        let act_receipt = self.storage.append_receipt(
            &mut self.ids,
            &correlation,
            "marketplace.act.recorded",
            Some(decision.policy_decision_id.clone()),
            payload(&[
                ("act", request.act),
                ("source_route", request.source_route),
                ("capability_id", request.capability_id),
                ("source_record_id", request.source_record_id),
                ("scope", request.scope),
                ("decision", request.decision),
                ("read_only", read_only),
                ("pack_id", request.pack_id),
                ("pack_version", request.pack_version),
                ("previous_version", request.previous_version),
                ("application_targets", request.application_targets),
                ("return_to", request.return_to),
                ("origin_surface", request.origin_surface),
                ("origin_object_id", request.origin_object_id),
                ("readiness_state", request.readiness_state),
                ("config_status", request.config_status),
                ("outcome", request.outcome),
                ("task_class", request.task_class),
                ("change_summary", request.change_summary),
                ("permission_diff", request.permission_diff),
                ("source_lane", request.source_lane),
                ("url", request.url),
                ("note", request.note),
                ("reason", request.reason),
                ("scan_status", request.scan_status),
                ("prompt_injection_scan", request.prompt_injection_scan),
                ("signature_scan", request.signature_scan),
                ("sbom_scan", request.sbom_scan),
                ("checksum", request.checksum),
                ("scan_findings", request.scan_findings),
                ("items_added", request.items_added),
                ("items_held", request.items_held),
                ("item_record_ids", request.item_record_ids),
                ("actor_role", request.actor_role),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
                ("capability_execution_allowed", "false"),
                ("secret_access_allowed", "false"),
                ("inherited_agent_permissions_allowed", "false"),
            ]),
        );
        Ok(MarketplaceActReport {
            status: "MARKETPLACE_ACT_RECORDED",
            act_receipt_id: act_receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
        })
    }
}
