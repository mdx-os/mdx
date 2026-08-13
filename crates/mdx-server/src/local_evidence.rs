use mdx_core::json_string_literal;
use std::fs;

pub(crate) fn render_local_index_json() -> String {
    concat!(
        "{\n",
        "  \"name\": \"mdx-local-api-index\",\n",
        "  \"mode\": \"deterministic-local\",\n",
        "  \"status\": \"OK\",\n",
        "  \"routes\": [\n",
        "    { \"path\": \"/local/auth-session.json\", \"purpose\": \"deterministic local identity boundary\" },\n    { \"path\": \"/local/evidence-index.json\", \"purpose\": \"aggregated local proof packet\" },\n",
        "    { \"path\": \"/local/activation-summary.json\", \"purpose\": \"compact local activation summary\" },\n    { \"path\": \"/local/activation-report.json\", \"purpose\": \"local readiness report\" },\n",
        "    { \"path\": \"/local/dogfood-proof.json\", \"purpose\": \"local dogfood provenance\" },\n",
        "    { \"path\": \"/local/operational-handoff.json\", \"purpose\": \"final local handoff packet\" },\n",
        "    { \"path\": \"/v2/provider-turn-on-matrix.json\", \"purpose\": \"v2 optional provider turn-on controls\" },\n    { \"path\": \"/v2/provider-activation-request.json\", \"purpose\": \"v2 provider activation request controls\" },\n    { \"path\": \"/v2/provider-turn-on-decision.json\", \"purpose\": \"v2 ranked provider turn-on decision packet\" },\n    { \"path\": \"/v2/provider-turn-on-drill.json\", \"purpose\": \"v2 provider turn-on drill with live calls blocked\" },\n    { \"path\": \"/v2/provider-connected-local.json\", \"purpose\": \"v2 provider-connected local runtime bridge proof\" },\n    { \"path\": \"/v2/seeded-tenant-demo.json\", \"purpose\": \"v2 seeded tenant and demo proof\" },\n    { \"path\": \"/v2/fresh-install-readiness.json\", \"purpose\": \"local-first v2 fresh install readiness packet\" },\n    { \"path\": \"/v2/local-round-trip.json\", \"purpose\": \"served v2 install route to Start UI round-trip proof\" },\n    { \"path\": \"/v2/deployment-plan.json\", \"purpose\": \"v2 local-first deployment plan and cloud readiness boundary\" },\n    { \"path\": \"/v2/alpha-env-contract.json\", \"purpose\": \"v2 alpha env, secret, auth, provider, and cloud profile contract\" },\n    { \"path\": \"/v2/alpha-auth-bootstrap.json\", \"purpose\": \"v2 alpha auth, tenant, invite, role, and session bootstrap contract\" },\n    { \"path\": \"/v2/deployment-profile-bootstrap.json\", \"purpose\": \"v2 deployment profile bootstrap and cloud artifact boundary\" },\n    { \"path\": \"/v2/public-release-plan.json\", \"purpose\": \"v2 public release blocker and packaging plan\" },\n    { \"path\": \"/v2/alpha-onboarding.json\", \"purpose\": \"v2 alpha engineer and operator onboarding packet\" },\n    { \"path\": \"/auth/provider-profiles.json\", \"purpose\": \"auth provider profile, tenant claim, and SSO readiness boundary\" }\n",
        "  ],\n",
        "  \"next_commands\": [\"make local-ui-proof\", \"make operational-proof\", \"make local-dev-stack\", \"make v2-fresh-install-proof\"],\n",
        "  \"notes\": [\"read-only local API index\", \"all mutation remains behind policy and receipt gates\"]\n",
        "}\n"
    )
    .to_string()
}

pub(crate) fn render_local_evidence_route(path: &str) -> Option<String> {
    Some(match path {
        "/local/index.json" => render_local_index_json(),
        "/local/evidence-index.json" => render_local_evidence_index_json(),
        "/local/activation-summary.json" => render_local_activation_summary_json(),
        "/local/activation-report.json" => render_local_activation_report_json(),
        "/local/dogfood-proof.json" => render_local_dogfood_proof_json(),
        "/local/operational-handoff.json" => render_local_operational_handoff_json(),
        "/v2/provider-turn-on-matrix.json" => render_v2_provider_turn_on_matrix_json(),
        "/v2/provider-activation-request.json" => render_v2_provider_activation_request_json(),
        "/v2/provider-turn-on-decision.json" => render_v2_provider_turn_on_decision_json(),
        "/v2/provider-turn-on-drill.json" => render_v2_provider_turn_on_drill_json(),
        "/v2/provider-connected-local.json" => render_v2_provider_connected_local_json(),
        "/v2/seeded-tenant-demo.json" => render_v2_seeded_tenant_demo_json(),
        "/v2/fresh-install-readiness.json" => render_v2_fresh_install_readiness_json(),
        "/v2/local-round-trip.json" => render_v2_local_round_trip_json(),
        "/v2/deployment-plan.json" => render_v2_deployment_plan_json(),
        "/v2/alpha-env-contract.json" => render_v2_alpha_env_contract_json(),
        "/v2/alpha-auth-bootstrap.json" => render_v2_alpha_auth_bootstrap_json(),
        "/v2/deployment-profile-bootstrap.json" => render_v2_deployment_profile_bootstrap_json(),
        "/v2/public-release-plan.json" => render_v2_public_release_plan_json(),
        "/v2/alpha-onboarding.json" => render_v2_alpha_onboarding_json(),
        "/auth/provider-profiles.json" => render_auth_provider_profiles_json(),
        _ => return None,
    })
}

pub(crate) fn render_local_evidence_index_json() -> String {
    render_local_json_file(".mdx-local/evidence-index.json", |status, file| {
        fallback_json(
            status,
            "mdx-local-evidence-index",
            "local-only-ignored",
            "evidence_file",
            file,
            "",
            "run make operational-proof or make local-proof to refresh local evidence",
        )
    })
}

pub(crate) fn render_local_activation_summary_json() -> String {
    render_local_json_file(".mdx-local/activation-summary.json", |status, file| {
        fallback_json(
            status,
            "mdx-local-activation-summary",
            "local-only-ignored",
            "summary_file",
            file,
            "",
            "run make local-activation-summary to refresh compact activation evidence",
        )
    })
}

pub(crate) fn render_local_activation_report_json() -> String {
    render_local_json_file(".mdx-local/activation-report.json", |status, file| {
        fallback_json(
            status,
            "mdx-local-activation-report",
            "local-only-ignored",
            "evidence_file",
            file,
            "\n  \"readiness\": { \"handoff_status\": \"MISSING\" },",
            "run make local-activation-report to refresh the local readiness report",
        )
    })
}

pub(crate) fn render_local_dogfood_proof_json() -> String {
    render_local_json_file(
        ".mdx-local/dogfood/local-dogfood-proof.json",
        |status, file| {
            fallback_json(
                status,
                "mdx-local-dogfood-proof",
                "local-ship-provenance",
                "evidence_file",
                file,
                "\n  \"dogfood_status\": \"MISSING\",\n  \"full_dogfood_pass\": false,",
                "run make dogfood-proof to refresh local dogfood evidence",
            )
        },
    )
}

pub(crate) fn render_local_operational_handoff_json() -> String {
    render_local_json_file(
        ".mdx-local/handoff/local-operational-handoff.json",
        |status, file| {
            fallback_json(
                status,
                "mdx-local-operational-handoff",
                "local-only-ignored",
                "evidence_file",
                file,
                "\n  \"handoff_status\": \"MISSING\",",
                "run make local-operational-handoff to refresh the local handoff packet",
            )
        },
    )
}

pub(crate) fn render_v2_fresh_install_readiness_json() -> String {
    render_local_json_file(
        ".mdx-local/v2-fresh-install-readiness.json",
        |status, file| {
            fallback_json(
                status,
                "mdx-v2-fresh-install-readiness",
                "local-first-fresh-install",
                "evidence_file",
                file,
                "\n  \"route\": \"/v2/fresh-install-readiness.json\",\n  \"local_install_ready\": false,\n  \"fresh_install_default\": true,\n  \"v1_migration_required\": false,\n  \"cloud_deployment_ready\": false,\n  \"production_write_allowed\": false,\n  \"production_cutover_allowed\": false,",
                "run make v2-fresh-install-proof to refresh the local-first install readiness packet",
            )
        },
    )
}

pub(crate) fn render_v2_seeded_tenant_demo_json() -> String {
    render_local_json_file(".mdx-local/v2-seeded-tenant-demo.json", |status, file| {
        fallback_json(
            status,
            "mdx-v2-seeded-tenant-demo",
            "local-first-seeded-demo",
            "evidence_file",
            file,
            "\n  \"route\": \"/v2/seeded-tenant-demo.json\",\n  \"read_only\": true,\n  \"fresh_install_default\": true,\n  \"local_only\": true,\n  \"demo_flow_count\": 0,\n  \"receipt_backed_demo_flow_count\": 0,",
            "run make v2-seeded-tenant-demo to refresh the local seeded tenant demo packet",
        )
    })
}

pub(crate) fn render_v2_provider_turn_on_matrix_json() -> String {
    render_local_json_file(
        ".mdx-local/v2-provider-turn-on-matrix.json",
        |status, file| {
            fallback_json(
                status,
                "mdx-v2-provider-turn-on-matrix",
                "local-first-provider-controls",
                "evidence_file",
                file,
                "\n  \"route\": \"/v2/provider-turn-on-matrix.json\",\n  \"read_only\": true,\n  \"provider_calls_allowed_without_receipts\": false,\n  \"provider_secret_material_allowed\": false,",
                "run make v2-provider-turn-on-matrix to refresh optional provider turn-on controls",
            )
        },
    )
}

pub(crate) fn render_v2_provider_activation_request_json() -> String {
    render_local_json_file(
        ".mdx-local/v2-provider-activation-request.json",
        |status, file| {
            fallback_json(
                status,
                "mdx-v2-provider-activation-request",
                "local-first-provider-activation",
                "evidence_file",
                file,
                "\n  \"route\": \"/v2/provider-activation-request.json\",\n  \"read_only\": true,\n  \"provider_call_allowed_now\": false,\n  \"human_approval_required\": true,",
                "run make v2-provider-activation-request to refresh provider activation request controls",
            )
        },
    )
}

pub(crate) fn render_v2_provider_turn_on_decision_json() -> String {
    render_local_json_file(
        ".mdx-local/v2-provider-turn-on-decision.json",
        |status, file| {
            fallback_json(
                status,
                "mdx-v2-provider-turn-on-decision",
                "local-first-provider-decision",
                "evidence_file",
                file,
                "\n  \"route\": \"/v2/provider-turn-on-decision.json\",\n  \"read_only\": true,\n  \"provider_call_allowed_now\": false,\n  \"human_approval_required\": true,",
                "run make v2-provider-turn-on-decision to refresh the provider turn-on decision packet",
            )
        },
    )
}

pub(crate) fn render_v2_provider_turn_on_drill_json() -> String {
    render_local_json_file(
        ".mdx-local/v2-provider-turn-on-drill.json",
        |status, file| {
            fallback_json(
                status,
                "mdx-v2-provider-turn-on-drill",
                "local-first-provider-turn-on-drill",
                "evidence_file",
                file,
                "\n  \"route\": \"/v2/provider-turn-on-drill.json\",\n  \"read_only\": true,\n  \"provider_call_allowed_now\": false,\n  \"live_provider_call_allowed_now\": false,\n  \"human_approval_required\": true,",
                "run make v2-provider-turn-on-drill to refresh the provider turn-on drill packet",
            )
        },
    )
}

pub(crate) fn render_v2_local_round_trip_json() -> String {
    render_local_json_file(".mdx-local/v2-local-round-trip.json", |status, file| {
        fallback_json(
            status,
            "mdx-v2-local-round-trip",
            "fresh-install-current-commit",
            "evidence_file",
            file,
            "\n  \"route\": \"/v2/local-round-trip.json\",\n  \"read_only\": true,\n  \"fresh_install_default\": true,\n  \"v1_migration_required\": false,\n  \"provider_call_allowed_now\": false,\n  \"production_write_allowed\": false,",
            "run make v2-local-round-trip to refresh the served local round-trip proof",
        )
    })
}

#[rustfmt::skip]
pub(crate) fn render_v2_deployment_plan_json() -> String {
    render_local_json_file(".mdx-local/v2-deployment-plan.json", |status, file| {
        fallback_json(status, "mdx-v2-deployment-plan", "local-first-deployment-planning", "evidence_file", file, "\n  \"route\": \"/v2/deployment-plan.json\",\n  \"read_only\": true,\n  \"cloud_deployment_ready\": false,\n  \"deployment_authority_allowed\": false,\n  \"production_write_allowed\": false,", "run make v2-deployment-plan to refresh the local-first deployment plan packet")
    })
}

#[rustfmt::skip]
pub(crate) fn render_v2_provider_connected_local_json() -> String { render_local_json_file(".mdx-local/v2-provider-connected-local.json", |status, file| { fallback_json(status, "mdx-v2-provider-connected-local", "provider-connected-local-runtime", "evidence_file", file, "\n  \"route\": \"/v2/provider-connected-local.json\",\n  \"read_only\": true,\n  \"provider_connected_local_ready\": false,\n  \"production_write_allowed\": false,", "run make v2-provider-connected-local to refresh provider-connected local runtime evidence") }) }
#[rustfmt::skip]
pub(crate) fn render_v2_alpha_env_contract_json() -> String { render_local_json_file(".mdx-local/v2-alpha-env-contract.json", |status, file| { fallback_json(status, "mdx-v2-alpha-env-contract", "alpha-env-and-secret-readiness", "evidence_file", file, "\n  \"route\": \"/v2/alpha-env-contract.json\",\n  \"read_only\": true,\n  \"secret_values_recorded\": false,\n  \"cloud_deployment_allowed_now\": false,\n  \"production_write_allowed\": false,", "run make v2-alpha-env-contract to refresh alpha env contract evidence") }) }
#[rustfmt::skip]
pub(crate) fn render_v2_alpha_auth_bootstrap_json() -> String { render_local_json_file(".mdx-local/v2-alpha-auth-bootstrap.json", |status, file| { fallback_json(status, "mdx-v2-alpha-auth-bootstrap", "alpha-auth-and-tenant-bootstrap", "evidence_file", file, "\n  \"route\": \"/v2/alpha-auth-bootstrap.json\",\n  \"read_only\": true,\n  \"local_auth_bootstrap_ready\": false,\n  \"real_login_allowed_now\": false,\n  \"production_auth_provider_allowed\": false,", "run make v2-alpha-auth-bootstrap to refresh alpha auth bootstrap evidence") }) }
#[rustfmt::skip]
pub(crate) fn render_v2_deployment_profile_bootstrap_json() -> String { render_local_json_file(".mdx-local/v2-deployment-profile-bootstrap.json", |status, file| { fallback_json(status, "mdx-v2-deployment-profile-bootstrap", "cloud-agnostic-deployment-bootstrap", "evidence_file", file, "\n  \"route\": \"/v2/deployment-profile-bootstrap.json\",\n  \"read_only\": true,\n  \"cloud_deployment_allowed_now\": false,\n  \"deployment_authority_allowed\": false,\n  \"production_write_allowed\": false,", "run make v2-deployment-profile-bootstrap to refresh deployment profile bootstrap evidence") }) }
#[rustfmt::skip]
pub(crate) fn render_v2_public_release_plan_json() -> String { render_local_json_file(".mdx-local/v2-public-release-plan.json", |status, file| { fallback_json(status, "mdx-v2-public-release-plan", "local-first-public-release-planning", "evidence_file", file, "\n  \"route\": \"/v2/public-release-plan.json\",\n  \"read_only\": true,\n  \"public_release_ready\": false,\n  \"package_publish_allowed\": false,\n  \"deployment_authority_allowed\": false,", "run make v2-public-release-plan to refresh the public release blocker packet") }) }
#[rustfmt::skip] pub(crate) fn render_v2_alpha_onboarding_json() -> String { render_local_json_file(".mdx-local/v2-alpha-onboarding.json", |status, file| { fallback_json(status, "mdx-v2-alpha-onboarding", "alpha-engineer-operator-onboarding", "evidence_file", file, "\n  \"route\": \"/v2/alpha-onboarding.json\",\n  \"read_only\": true,\n  \"local_install_ready\": false,\n  \"cloud_deployment_allowed_now\": false,\n  \"deployment_authority_allowed\": false,\n  \"production_write_allowed\": false,", "run make v2-alpha-onboarding to refresh alpha onboarding evidence") }) }
#[rustfmt::skip] pub(crate) fn render_auth_provider_profiles_json() -> String {
    render_local_json_file(".mdx-local/auth/auth-provider-profiles.json", |status, file| {
        fallback_json(status, "mdx-auth-provider-profiles", "auth-provider-profile-readiness", "evidence_file", file, "\n  \"route\": \"/auth/provider-profiles.json\",\n  \"read_only\": true,\n  \"real_login_allowed_now\": false,\n  \"production_auth_provider_allowed\": false,\n  \"production_write_allowed\": false,", "run make auth-provider-profiles to refresh auth provider profile evidence")
    })
}

fn render_local_json_file(
    evidence_file: &'static str,
    fallback: impl Fn(&str, &'static str) -> String,
) -> String {
    match fs::read_to_string(evidence_file) {
        Ok(body) if body.trim().starts_with('{') && body.trim().ends_with('}') => {
            format!("{}\n", body.trim())
        }
        Ok(_) => fallback("INVALID_JSON", evidence_file),
        Err(_) => fallback("MISSING", evidence_file),
    }
}

fn fallback_json(
    status: &str,
    name: &str,
    mode: &str,
    file_key: &str,
    evidence_file: &str,
    extra_fields: &str,
    note: &str,
) -> String {
    format!(
        "{{\n  \"name\": {},\n  \"mode\": {},\n  \"status\": {},{extra_fields}\n  {}: {},\n  \"notes\": [{}]\n}}",
        json_string_literal(name),
        json_string_literal(mode),
        json_string_literal(status),
        json_string_literal(file_key),
        json_string_literal(evidence_file),
        json_string_literal(note)
    )
}
