use super::harness::{HarnessProviderGatewayRequest, HarnessProviderProfile, HarnessRunManifest};

pub(super) fn evaluate_harness_enterprise_pack(
    manifest: &HarnessRunManifest<'_>,
    request: &HarnessProviderGatewayRequest<'_>,
    profile: &HarnessProviderProfile<'_>,
) -> Result<(), &'static str> {
    if request.enterprise_packs.is_empty() {
        return Err("missing_enterprise_pack_contract");
    }
    let enterprise_pack = request
        .enterprise_packs
        .iter()
        .find(|pack| pack.enterprise_pack_id == profile.enterprise_pack)
        .ok_or("enterprise_pack_contract_missing")?;
    if enterprise_pack.enterprise_pack_id.trim().is_empty() {
        return Err("missing_enterprise_pack_id");
    }
    if empty_or_blank(enterprise_pack.allowed_policy_profiles) {
        return Err("enterprise_pack_policy_profiles_missing");
    }
    if !enterprise_pack
        .allowed_policy_profiles
        .contains(&profile.policy_profile)
    {
        return Err("enterprise_pack_policy_profile_not_allowed");
    }
    if empty_or_blank(enterprise_pack.provider_profile_allowlist) {
        return Err("enterprise_pack_profile_allowlist_missing");
    }
    if !enterprise_pack
        .provider_profile_allowlist
        .contains(&profile.profile_id)
    {
        return Err("enterprise_pack_profile_not_allowed");
    }
    if profile.max_context_tokens > enterprise_pack.max_context_tokens {
        return Err("enterprise_pack_context_budget_exceeded");
    }
    if profile.max_output_tokens > enterprise_pack.max_output_tokens {
        return Err("enterprise_pack_output_budget_exceeded");
    }
    if profile.max_cost_cents > enterprise_pack.max_cost_cents {
        return Err("enterprise_pack_cost_budget_exceeded");
    }
    if enterprise_pack.data_retention.trim().is_empty() {
        return Err("enterprise_pack_missing_data_retention");
    }
    if enterprise_pack.data_retention != profile.data_retention {
        return Err("enterprise_pack_retention_mismatch");
    }
    if enterprise_pack.context_redaction_required && !profile.context_redaction_required {
        return Err("enterprise_pack_context_redaction_required");
    }
    if enterprise_pack.telemetry_redaction_required && !profile.telemetry_redaction_required {
        return Err("enterprise_pack_telemetry_redaction_required");
    }
    if enterprise_pack.audit_export_allowed {
        return Err("enterprise_pack_audit_export_not_local");
    }
    if enterprise_pack.approval_receipt_required
        && (!manifest.approval_receipt_required
            || manifest.trust_boundary.approval_receipt_id.is_none())
    {
        return Err("enterprise_pack_approval_receipt_required");
    }
    Ok(())
}

fn empty_or_blank(items: &[&str]) -> bool {
    items.is_empty() || items.iter().any(|item| item.trim().is_empty())
}
