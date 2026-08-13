use crate::{RouteResponse, mobile_pairing, request_security};
use mdx_core::{ForgeRunEvent, MdxKernel, Receipt};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

const MAX_MOBILE_REVIEWS: usize = 12;
const MAX_MOBILE_REVIEW_FILES: usize = 80;
const MAX_MOBILE_PATCH_CHARS: usize = 32_000;
const MAX_MOBILE_TOTAL_PATCH_CHARS: usize = 480_000;
const MAX_ACCOUNT_DELETION_LIFETIME_SECONDS: i64 = 300;
const MAX_ACCOUNT_DELETION_CLOCK_SKEW_SECONDS: i64 = 30;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MobileAccountDeletionRequest {
    request_id: String,
    confirm: bool,
    user_presence: String,
    actor_device_id: String,
    issued_at: String,
    expires_at: String,
    device_signature: String,
}

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/mobile/reviews.json" => Some(handle_reviews(method, kernel)),
        "/mobile/handoff-targets.json" => Some(handle_handoff_targets(method, kernel)),
        "/mobile/support-diagnostics.json" => Some(handle_diagnostics(method, kernel)),
        "/mobile/privacy.json" => Some(handle_privacy(method)),
        "/mobile/account-deletion-requests.json" => {
            Some(handle_account_deletion(method, body, kernel))
        }
        _ => None,
    }
}

fn handle_handoff_targets(
    method: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if !method.eq_ignore_ascii_case("GET") {
        return Ok(method_not_allowed());
    }
    let verified = request_security::current_verified_identity();
    let tenant_id = verified
        .as_ref()
        .map(|identity| identity.tenant_id.as_str())
        .unwrap_or("local_tenant");
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned while listing handoff targets".to_string())?;
    let mut latest_repositories = std::collections::BTreeMap::<String, &Receipt>::new();
    for receipt in kernel.ledger().entries().iter().filter(|receipt| {
        receipt.kind == "forge.repo.connected" && receipt.tenant_id.as_str() == tenant_id
    }) {
        latest_repositories.insert(payload(receipt, "repo_id").to_string(), receipt);
    }
    let repositories = latest_repositories
        .into_values()
        .filter(|receipt| payload(receipt, "kind") == "local")
        .map(|receipt| {
            let repo_id = payload(receipt, "repo_id");
            let root = payload(receipt, "root");
            let identity_fingerprint =
                repository_identity_fingerprint(payload(receipt, "origin_url"), root);
            let suggested_checks = if root.is_empty() {
                Vec::new()
            } else {
                crate::forge_repo_profile::suggested_checks(Path::new(root))
            };
            let (default_branch, default_branch_source) =
                crate::forge_repo_route::recorded_default_branch(&kernel, repo_id)
                    .unwrap_or_default();
            json!({
                "repository_id": repo_id,
                "display_name": payload(receipt, "label"),
                "kind": "paired_host_local",
                "identity_fingerprint": identity_fingerprint,
                "default_branch": default_branch,
                "default_branch_source": default_branch_source,
                "suggested_checks": suggested_checks,
                "source_receipt_id": receipt.receipt_id,
                "root_recorded": !root.is_empty(),
                "root_value_included": false,
                "origin_value_included": false
            })
        })
        .collect::<Vec<_>>();
    let mut latest_environments = std::collections::BTreeMap::<String, &Receipt>::new();
    for receipt in kernel.ledger().entries().iter().filter(|receipt| {
        receipt.kind == "forge.run.event"
            && receipt.tenant_id.as_str() == tenant_id
            && payload(receipt, "cloud_record_kind") == "cloud_environment"
    }) {
        latest_environments.insert(
            payload(receipt, "cloud_environment_id").to_string(),
            receipt,
        );
    }
    let cloud_environments = latest_environments
        .into_values()
        .filter(|receipt| payload(receipt, "cloud_environment_status") == "VERIFIED")
        .map(|receipt| {
            let repo_id = payload(receipt, "cloud_environment_repo_id");
            let origin = kernel.forge_repo_origin(repo_id).unwrap_or_default();
            json!({
                "environment_id": payload(receipt, "cloud_environment_id"),
                "repository_id": payload(receipt, "cloud_environment_repo_id"),
                "display_name": "MDx Cloud",
                "kind": "mdx_cloud",
                "identity_fingerprint": repository_identity_fingerprint(&origin, ""),
                "source_receipt_id": receipt.receipt_id,
                "verified": true
            })
        })
        .collect::<Vec<_>>();
    let source_receipt_ids = repositories
        .iter()
        .chain(cloud_environments.iter())
        .filter_map(|target| target["source_receipt_id"].as_str())
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    Ok(RouteResponse::json(
        "200 OK",
        json!({
            "name": "mdx-mobile-handoff-targets",
            "status": "OK",
            "tenant_id": tenant_id,
            "source_receipt_ids": source_receipt_ids,
            "paired_host_repositories": repositories,
            "cloud_environments": cloud_environments,
            "raw_roots_included": false,
            "origin_values_included": false,
            "contains_secret_values": false,
            "grants_execution_authority": false,
            "production_write_allowed": false
        })
        .to_string(),
    ))
}

fn repository_identity_fingerprint(recorded_origin: &str, root: &str) -> String {
    let origin = if !recorded_origin.trim().is_empty() {
        recorded_origin.trim().to_string()
    } else if !root.trim().is_empty() {
        std::process::Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["config", "--get", "remote.origin.url"])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
            .unwrap_or_default()
    } else {
        String::new()
    };
    if origin.is_empty() {
        return String::new();
    }
    let normalized = normalize_repository_origin(&origin);
    digest_prefix(&normalized)
}

fn normalize_repository_origin(origin: &str) -> String {
    let value = origin
        .trim()
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .to_ascii_lowercase();
    if let Some((_, remainder)) = value.split_once("://") {
        return remainder
            .rsplit_once('@')
            .map(|(_, host_path)| host_path)
            .unwrap_or(remainder)
            .trim_end_matches('/')
            .to_string();
    }
    let without_user = value
        .rsplit_once('@')
        .map(|(_, host_path)| host_path)
        .unwrap_or(&value);
    if let Some((host, path)) = without_user.split_once(':') {
        return format!("{host}/{path}").trim_end_matches('/').to_string();
    }
    without_user.trim_end_matches('/').to_string()
}

fn handle_reviews(method: &str, kernel: &Arc<RwLock<MdxKernel>>) -> Result<RouteResponse, String> {
    if !method.eq_ignore_ascii_case("GET") {
        return Ok(method_not_allowed());
    }
    let verified = request_security::current_verified_identity();
    let tenant_id = verified
        .as_ref()
        .map(|identity| identity.tenant_id.as_str())
        .unwrap_or("local_tenant");
    let session_ids = {
        let kernel = kernel
            .read()
            .map_err(|_| "kernel lock poisoned while listing mobile reviews".to_string())?;
        reviewable_session_ids(&kernel, tenant_id)
    };
    let mut reviews = Vec::new();
    for session_id in session_ids {
        if let Some(review) = mobile_review(kernel, tenant_id, &session_id)? {
            reviews.push(review);
        }
    }
    let source_receipt_ids = projected_receipt_ids(&reviews, "source_receipt_ids");
    Ok(RouteResponse::json(
        "200 OK",
        json!({
            "name": "mdx-mobile-reviews",
            "status": "OK",
            "tenant_id": tenant_id,
            "source_receipt_ids": source_receipt_ids,
            "reviews": reviews,
            "review_count": reviews.len(),
            "summary_is_navigation_not_proof": true,
            "raw_secret_values_included": false,
            "merge_authority_granted": false,
            "deployment_authority_granted": false,
            "production_write_allowed": false
        })
        .to_string(),
    ))
}

fn mobile_review(
    kernel: &Arc<RwLock<MdxKernel>>,
    tenant_id: &str,
    session_id: &str,
) -> Result<Option<Value>, String> {
    let packet_body = crate::forge_review_packet::assemble_review_packet_json(kernel, session_id)?;
    let packet: Value = serde_json::from_str(&packet_body)
        .map_err(|error| format!("mobile review packet was invalid: {error}"))?;
    if packet["status"].as_str() != Some("OK") {
        return Ok(None);
    }
    let diff_response = crate::forge_diff_route::route_response(
        "POST",
        "/forge/run-diff.json",
        &json!({"run_id": session_id}).to_string(),
        kernel,
    )
    .ok_or_else(|| "Forge diff route unavailable".to_string())??;
    let diff: Value = serde_json::from_str(&diff_response.body).unwrap_or_default();
    let mut remaining_patch_chars = MAX_MOBILE_TOTAL_PATCH_CHARS;
    let files = diff["files"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .take(MAX_MOBILE_REVIEW_FILES)
        .map(|file| bound_mobile_file(file, &mut remaining_patch_chars))
        .collect::<Vec<_>>();
    let patches_truncated = files
        .iter()
        .any(|file| file["patch_truncated"].as_bool() == Some(true));
    let proof = packet["checks"]["names"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|check| {
            json!({
                "name": check["name"].as_str().unwrap_or("Recorded check"),
                "passed": check["ok"].as_bool().unwrap_or(false)
            })
        })
        .collect::<Vec<_>>();
    let change_map = packet["diff"]["sections"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|section| {
            json!({
                "id": section["section"],
                "label": section["label"],
                "file_count": section["file_count"],
                "added": section["added"],
                "removed": section["removed"],
                "generated": section["generated"],
                "files": section["files"]
            })
        })
        .collect::<Vec<_>>();
    let proof_summary = packet["proof_coverage"]["summary"]
        .as_str()
        .unwrap_or("Proof coverage is unavailable");
    let failed = packet["checks"]["failed"].as_u64().unwrap_or(0);
    let missing = packet["proof_coverage"]["missing_checks"]
        .as_array()
        .map(Vec::len)
        .unwrap_or(0);
    let risk = if failed > 0 || missing > 0 {
        format!("Review is blocked by {failed} failed and {missing} missing proof checks.")
    } else if packet["review_status"].as_str() == Some("ready_to_ship") {
        "No unresolved proof gap is recorded. Source-host and merge policy still decide whether the change may land.".to_string()
    } else {
        format!(
            "{} The existing human ship, source-host, CI, and deployment gates remain authoritative.",
            packet["next_move"]
                .as_str()
                .unwrap_or("Review remains required.")
        )
    };
    let goal = session_goal(kernel, session_id)?;
    let outcome = if goal.is_empty() {
        "Forge produced a reviewable change".to_string()
    } else {
        format!("Forge produced a reviewable change for: {goal}")
    };
    let documentation_only = !files.is_empty()
        && files.iter().all(|file| {
            file["path"].as_str().is_some_and(|path| {
                [".md", ".markdown", ".txt"]
                    .iter()
                    .any(|extension| path.to_ascii_lowercase().ends_with(extension))
            })
        });
    let changed_file_count = packet["diff"]["real_change_count"].as_u64().unwrap_or(0);
    let demo = if documentation_only {
        "No preview is needed for this documentation-only change. Review the focused diff below."
            .to_string()
    } else {
        format!(
            "No preview artifact is attached. Review the {changed_file_count} changed file{} in the focused diff below.",
            if changed_file_count == 1 { "" } else { "s" }
        )
    };
    let source_receipt_ids = {
        let kernel = kernel.read().map_err(|_| {
            "kernel lock poisoned while collecting mobile review provenance".to_string()
        })?;
        kernel
            .ledger()
            .entries()
            .iter()
            .filter(|receipt| {
                receipt.kind == "forge.run.event"
                    && receipt.tenant_id.as_str() == tenant_id
                    && payload(receipt, "run_id") == session_id
            })
            .map(|receipt| receipt.receipt_id.clone())
            .collect::<BTreeSet<_>>()
    };
    Ok(Some(json!({
        "id": format!("mobile_review_{session_id}"),
        "session_id": session_id,
        "source_receipt_ids": source_receipt_ids,
        "review_status": packet["review_status"],
        "next_move": packet["next_move"],
        "outcome": outcome,
        "demo": demo,
        "proof_summary": proof_summary,
        "proof": proof,
        "risk": risk,
        "branch": packet["branch"],
        "commit_sha": packet["commit_sha"],
        "changed_files": packet["diff"]["real_change_count"],
        "added": packet["diff"]["added_total"],
        "removed": packet["diff"]["removed_total"],
        "change_map": change_map,
        "files": files,
        "files_truncated": diff["review_file_count"].as_u64().unwrap_or(0) as usize > MAX_MOBILE_REVIEW_FILES,
        "patches_truncated": patches_truncated,
        "review_packet_route": "/forge/review-packet.json",
        "diff_route": "/forge/run-diff.json",
        "request_changes_route": "/forge/run-revisions.json",
        "ship_decision_route": "/forge/run-ship-decisions.json",
        "source_host_delivery_route": "/forge/source-host-live-deliveries.json",
        "summary_is_navigation_not_proof": true,
        "contains_secret_values": false,
        "merge_authority_granted": false,
        "deployment_authority_granted": false,
        "production_write_allowed": false
    })))
}

fn session_goal(kernel: &Arc<RwLock<MdxKernel>>, session_id: &str) -> Result<String, String> {
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned while reading mobile review goal".to_string())?;
    Ok(kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| {
            receipt.kind == "forge.run.event" && payload(receipt, "run_id") == session_id
        })
        .find_map(|receipt| {
            ["operator_intent", "operator_ask"].iter().find_map(|key| {
                let value = payload(receipt, key);
                (!value.is_empty()).then(|| mdx_mobile_relay::sanitize_summary(value))
            })
        })
        .unwrap_or_default())
}

fn bound_mobile_file(mut file: Value, remaining_patch_chars: &mut usize) -> Value {
    let Some(patch) = file["patch"].as_str() else {
        file["patch_truncated"] = json!(false);
        return file;
    };
    let patch_chars = patch.chars().count();
    let allowed = MAX_MOBILE_PATCH_CHARS.min(*remaining_patch_chars);
    let truncated = patch_chars > allowed;
    if truncated {
        let bounded = patch.chars().take(allowed).collect::<String>();
        file["patch"] = json!(if bounded.is_empty() {
            "... patch omitted after the mobile review budget was reached".to_string()
        } else {
            format!("{bounded}\n... patch truncated for mobile review")
        });
    }
    *remaining_patch_chars = remaining_patch_chars.saturating_sub(patch_chars.min(allowed));
    file["patch_truncated"] = json!(truncated);
    file
}

fn handle_diagnostics(
    method: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if !method.eq_ignore_ascii_case("GET") {
        return Ok(method_not_allowed());
    }
    let verified = request_security::current_verified_identity();
    let tenant_id = verified
        .as_ref()
        .map(|identity| identity.tenant_id.as_str())
        .unwrap_or("local_tenant");
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned while building diagnostics".to_string())?;
    let sessions = reviewable_session_ids(&kernel, tenant_id);
    let forge_event_count = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| {
            receipt.tenant_id.as_str() == tenant_id && receipt.kind == "forge.run.event"
        })
        .count();
    let last_receipt = kernel
        .ledger()
        .entries()
        .iter()
        .rev()
        .find(|receipt| receipt.tenant_id.as_str() == tenant_id);
    Ok(RouteResponse::json(
        "200 OK",
        json!({
            "name": "mdx-mobile-support-diagnostics",
            "status": "OK",
            "tenant_fingerprint": digest_prefix(tenant_id),
            "forge_event_count": forge_event_count,
            "reviewable_session_count": sessions.len(),
            "last_receipt_kind": last_receipt.map(|receipt| receipt.kind.as_str()),
            "last_receipt_at": last_receipt.map(|receipt| receipt.receipt_timestamp.as_str()),
            "relay_configured": std::env::var("MDX_MOBILE_RELAY_PUBLIC_URL").is_ok(),
            "source_host_live_enabled": env_enabled("MDX_FORGE_SOURCE_HOST_LIVE"),
            "hosted_sandbox_configured": crate::mobile_hosted_sandbox::hosted_backend_configured(),
            "safe_export_fields": ["app_version", "device_model", "os_version", "connection_state", "last_refresh_at", "last_error_code", "event_cursor"],
            "excluded_from_support_export": ["source", "diffs", "raw_logs", "tokens", "credentials", "secret_values", "prompt_content"],
            "raw_source_included": false,
            "credential_values_included": false,
            "production_write_allowed": false
        })
        .to_string(),
    ))
}

fn handle_privacy(method: &str) -> Result<RouteResponse, String> {
    if !method.eq_ignore_ascii_case("GET") {
        return Ok(method_not_allowed());
    }
    Ok(RouteResponse::json(
        "200 OK",
        json!({
            "name": "mdx-mobile-privacy",
            "status": "OK",
            "mobile_projection_retention": "bounded_on_device_cache_until_sign_out_or_local_delete",
            "relay_event_retention": "tenant_policy_with_bounded_mobile_projection",
            "hosted_workspace_retention": "environment_definition_retention_hours_max_168",
            "push_payload_retention": "opaque_reference_and_safe_summary_only",
            "offline_drafts": "device_only_until_explicit_send_or_delete",
            "support_export": "explicit_user_action_secret_free",
            "delete_local_data_supported": true,
            "account_deletion_request_route": "/mobile/account-deletion-requests.json",
            "account_deletion_execution": "operator_fulfilled_until_identity_provider_and_storage_erasure_workers_are_connected",
            "silent_account_deletion_claim_allowed": false,
            "production_write_allowed": false
        })
        .to_string(),
    ))
}

fn handle_account_deletion(
    method: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if !method.eq_ignore_ascii_case("POST") {
        return Ok(method_not_allowed());
    }
    let parsed: Value = match serde_json::from_str(body) {
        Ok(value) => value,
        Err(_) => {
            return Ok(refusal(
                "mobile_account_deletion_shape_invalid",
                "body must be JSON",
            ));
        }
    };
    let request: MobileAccountDeletionRequest = match serde_json::from_value(parsed.clone()) {
        Ok(request) => request,
        Err(error) => {
            return Ok(refusal(
                "mobile_account_deletion_shape_invalid",
                &format!("request shape is invalid: {error}"),
            ));
        }
    };
    if !valid_mobile_identifier(&request.request_id)
        || !request.request_id.starts_with("mobile_account_deletion_")
        || !valid_mobile_identifier(&request.actor_device_id)
        || request.device_signature.is_empty()
        || request.device_signature.len() > 256
    {
        return Ok(refusal(
            "mobile_account_deletion_shape_invalid",
            "device identity or signature shape is invalid",
        ));
    }
    if !request.confirm || request.user_presence != "biometric_verified" {
        return Ok(refusal(
            "mobile_account_deletion_confirmation_required",
            "account deletion requires an explicit biometric confirmation",
        ));
    }
    let issued = match OffsetDateTime::parse(&request.issued_at, &Rfc3339) {
        Ok(value) => value,
        Err(_) => {
            return Ok(refusal(
                "mobile_account_deletion_time_invalid",
                "issued_at must be RFC3339",
            ));
        }
    };
    let expires = match OffsetDateTime::parse(&request.expires_at, &Rfc3339) {
        Ok(value) => value,
        Err(_) => {
            return Ok(refusal(
                "mobile_account_deletion_time_invalid",
                "expires_at must be RFC3339",
            ));
        }
    };
    let now = OffsetDateTime::now_utc();
    if issued > now + time::Duration::seconds(MAX_ACCOUNT_DELETION_CLOCK_SKEW_SECONDS)
        || expires <= now
        || expires <= issued
        || (expires - issued).whole_seconds() > MAX_ACCOUNT_DELETION_LIFETIME_SECONDS
    {
        return Ok(refusal(
            "mobile_account_deletion_expired_or_future",
            "account deletion request must be current and expire within five minutes",
        ));
    }
    let resolved = request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    let attestation_payload = account_deletion_attestation_payload(&parsed)?;
    if let Err(code) = mobile_pairing::verify_registered_device_signature(
        &resolved,
        &request.actor_device_id,
        &attestation_payload,
        &request.device_signature,
        kernel,
    ) {
        return Ok(refusal(
            &code,
            "account deletion must be signed by this account's active registered device key",
        ));
    }
    let fingerprint = digest_hex(&attestation_payload);
    let _request_guard = account_deletion_gate()
        .lock()
        .map_err(|_| "account deletion request gate lock poisoned".to_string())?;
    if let Some(response) =
        account_deletion_idempotent_response(kernel, &resolved, &request.request_id, &fingerprint)?
    {
        return Ok(response);
    }
    let request_id = request.request_id.clone();
    let report = kernel
        .write()
        .map_err(|_| "kernel lock poisoned while recording deletion request".to_string())?
        .record_forge_run_event_with_evidence_fields(
            ForgeRunEvent {
                tenant_id: &resolved.tenant_id,
                actor_id: &resolved.actor_id,
                run_id: &format!("forge_run_{request_id}"),
                event: "evidence_appended",
                work_item_id: "mobile_account_deletion",
                detail: "Account deletion requested; fulfillment remains pending",
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            },
            &resolved.identity,
            &[
                ("mobile_account_deletion_request_id", request_id.as_str()),
                ("mobile_account_deletion_fingerprint", fingerprint.as_str()),
                ("mobile_account_deletion_status", "PENDING_FULFILLMENT"),
                (
                    "mobile_account_deletion_user_presence",
                    "biometric_verified",
                ),
                (
                    "mobile_account_deletion_user_presence_verification",
                    "client_asserted_device_key_bound",
                ),
                (
                    "mobile_account_deletion_device_signature_verification",
                    "verified_registered_device_key",
                ),
                (
                    "mobile_account_deletion_actor_device_id",
                    request.actor_device_id.as_str(),
                ),
                (
                    "mobile_account_deletion_issued_at",
                    request.issued_at.as_str(),
                ),
                (
                    "mobile_account_deletion_expires_at",
                    request.expires_at.as_str(),
                ),
                ("mobile_account_deletion_secret_values_recorded", "false"),
            ],
        )
        .map_err(|error| error.message())?;
    Ok(RouteResponse::json(
        "200 OK",
        json!({
            "name": "mdx-mobile-account-deletion-request",
            "status": "PENDING_FULFILLMENT",
            "request_id": request_id,
            "request_receipt_id": report.receipt_id,
            "next_safe_action": "Sign out and delete local data. MDx support must complete identity-provider and retained-storage erasure before the account is reported deleted.",
            "account_reported_deleted": false,
            "credential_values_recorded": false,
            "user_presence_verification": "client_asserted_device_key_bound",
            "device_signature_verification": "verified_registered_device_key",
            "idempotent_replay": false,
            "production_write_allowed": false
        })
        .to_string(),
    ))
}

fn account_deletion_attestation_payload(body: &Value) -> Result<Vec<u8>, String> {
    let mut unsigned = body.clone();
    let object = unsigned
        .as_object_mut()
        .ok_or_else(|| "account deletion attestation requires a JSON object".to_string())?;
    object.remove("device_signature");
    serde_json::to_vec(&unsigned)
        .map_err(|error| format!("account deletion attestation serialization failed: {error}"))
}

fn account_deletion_idempotent_response(
    kernel: &Arc<RwLock<MdxKernel>>,
    identity: &request_security::ResolvedWriteIdentity,
    request_id: &str,
    fingerprint: &str,
) -> Result<Option<RouteResponse>, String> {
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned while checking deletion idempotency".to_string())?;
    let existing = kernel.ledger().entries().iter().find(|receipt| {
        receipt.kind == "forge.run.event"
            && receipt.tenant_id.as_str() == identity.tenant_id.as_str()
            && receipt.actor_id.as_str() == identity.actor_id.as_str()
            && payload(receipt, "mobile_account_deletion_request_id") == request_id
    });
    let Some(existing) = existing else {
        return Ok(None);
    };
    if payload(existing, "mobile_account_deletion_fingerprint") != fingerprint {
        return Ok(Some(refusal(
            "mobile_account_deletion_idempotency_conflict",
            "request_id was already used for a different account deletion request",
        )));
    }
    Ok(Some(RouteResponse::json(
        "200 OK",
        json!({
            "name": "mdx-mobile-account-deletion-request",
            "status": "PENDING_FULFILLMENT",
            "request_id": request_id,
            "request_receipt_id": existing.receipt_id,
            "next_safe_action": "Sign out and delete local data. MDx support must complete identity-provider and retained-storage erasure before the account is reported deleted.",
            "account_reported_deleted": false,
            "credential_values_recorded": false,
            "user_presence_verification": "client_asserted_device_key_bound",
            "device_signature_verification": "verified_registered_device_key",
            "idempotent_replay": true,
            "production_write_allowed": false
        })
        .to_string(),
    )))
}

fn account_deletion_gate() -> &'static Mutex<()> {
    static GATE: OnceLock<Mutex<()>> = OnceLock::new();
    GATE.get_or_init(|| Mutex::new(()))
}

fn reviewable_session_ids(kernel: &MdxKernel, tenant_id: &str) -> Vec<String> {
    let mut ids = Vec::new();
    for receipt in kernel.ledger().entries().iter().rev().filter(|receipt| {
        receipt.kind == "forge.run.event" && receipt.tenant_id.as_str() == tenant_id
    }) {
        let run_id = payload(receipt, "run_id");
        let detail = payload(receipt, "detail");
        if valid_session_id(run_id)
            && (detail.starts_with("branch=") || payload(receipt, "event") == "run_finished")
            && !ids.iter().any(|existing| existing == run_id)
        {
            ids.push(run_id.to_string());
            if ids.len() == MAX_MOBILE_REVIEWS {
                break;
            }
        }
    }
    ids
}

fn payload<'a>(receipt: &'a Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}

fn valid_session_id(value: &str) -> bool {
    value.starts_with("forge_run_")
        && value.len() <= 200
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

fn valid_mobile_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

fn digest_prefix(value: &str) -> String {
    use ring::digest::{SHA256, digest};
    digest(&SHA256, value.as_bytes())
        .as_ref()
        .iter()
        .take(12)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn digest_hex(value: &[u8]) -> String {
    use ring::digest::{SHA256, digest};
    digest(&SHA256, value)
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn env_enabled(key: &str) -> bool {
    matches!(
        std::env::var(key)
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn refusal(code: &str, detail: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        json!({
            "status": "REFUSED",
            "error": code,
            "detail": detail,
            "receipt_recorded": false,
            "production_write_allowed": false
        })
        .to_string(),
    )
}

fn projected_receipt_ids(values: &[Value], field: &str) -> Vec<String> {
    values
        .iter()
        .filter_map(|value| value[field].as_array())
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn method_not_allowed() -> RouteResponse {
    RouteResponse::text("405 Method Not Allowed", "method not allowed\n".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use ring::digest::{SHA256, digest};
    use ring::rand::SystemRandom;
    use ring::signature::{ECDSA_P256_SHA256_ASN1_SIGNING, EcdsaKeyPair, KeyPair};

    fn temporary_repo(label: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "mdx_mobile_{label}_{}_{}",
            std::process::id(),
            OffsetDateTime::now_utc().unix_timestamp_nanos()
        ));
        std::fs::create_dir_all(&path).expect("temporary repository");
        path
    }

    fn register_deletion_device(device_id: &str, kernel: &Arc<RwLock<MdxKernel>>) -> Vec<u8> {
        let rng = SystemRandom::new();
        let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &rng).unwrap();
        let key = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, pkcs8.as_ref(), &rng)
            .unwrap();
        let public_key = key.public_key().as_ref();
        let public_key_thumbprint = digest(&SHA256, public_key)
            .as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let device = json!({
            "device_id": device_id,
            "platform": "ios",
            "display_name": "Deletion test iPhone",
            "public_key": URL_SAFE_NO_PAD.encode(public_key),
            "public_key_thumbprint": public_key_thumbprint
        });
        mobile_pairing::route_response("POST", "/mobile/devices.json", &device.to_string(), kernel)
            .unwrap()
            .unwrap();
        pkcs8.as_ref().to_vec()
    }

    fn attach_deletion_signature(body: &mut Value, pkcs8: &[u8]) {
        let rng = SystemRandom::new();
        let key = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, pkcs8, &rng).unwrap();
        let payload = account_deletion_attestation_payload(body).unwrap();
        body["device_signature"] =
            json!(URL_SAFE_NO_PAD.encode(key.sign(&rng, &payload).unwrap().as_ref()));
    }

    #[test]
    fn privacy_contract_is_honest_about_pending_account_erasure() {
        let response = handle_privacy("GET").expect("privacy response");
        let value: Value = serde_json::from_str(response.body_text()).expect("privacy JSON");
        assert_eq!(
            value["account_deletion_execution"],
            "operator_fulfilled_until_identity_provider_and_storage_erasure_workers_are_connected"
        );
        assert_eq!(value["silent_account_deletion_claim_allowed"], false);
    }

    #[test]
    fn empty_review_surfaces_keep_explicit_receipt_provenance() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let reviews = handle_reviews("GET", &kernel).expect("reviews response");
        let targets = handle_handoff_targets("GET", &kernel).expect("targets response");
        let reviews: Value = serde_json::from_str(reviews.body_text()).expect("reviews JSON");
        let targets: Value = serde_json::from_str(targets.body_text()).expect("targets JSON");

        assert_eq!(reviews["source_receipt_ids"], json!([]));
        assert_eq!(targets["source_receipt_ids"], json!([]));
    }

    #[test]
    fn handoff_target_exposes_the_proof_commands_mobile_will_submit() {
        let repo = temporary_repo("proof_commands");
        std::fs::write(
            repo.join("package.json"),
            r#"{"scripts":{"test":"node scripts/check.mjs"}}"#,
        )
        .expect("package manifest");
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        kernel
            .write()
            .unwrap()
            .connect_forge_repo(mdx_core::ForgeRepoConnect {
                tenant_id: "local_tenant",
                actor_id: "local_user",
                repo_id: "mobile-proof",
                label: "Mobile proof",
                root: repo.to_str().unwrap(),
                kind: "local",
                origin: "",
            })
            .expect("connect repository");

        let response = handle_handoff_targets("GET", &kernel).expect("targets response");
        let value: Value = serde_json::from_str(response.body_text()).expect("targets JSON");
        assert_eq!(
            value["paired_host_repositories"][0]["suggested_checks"],
            json!(["npm test"])
        );

        std::fs::remove_dir_all(repo).ok();
    }

    #[test]
    fn deletion_refusals_disclose_that_no_receipt_was_recorded() {
        let response = refusal("mobile_deletion_test", "test refusal");
        let value: Value = serde_json::from_str(response.body_text()).expect("refusal JSON");

        assert_eq!(value["receipt_recorded"], false);
    }

    #[test]
    fn diagnostics_never_exports_source_or_credentials() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = handle_diagnostics("GET", &kernel).expect("diagnostics response");
        let value: Value = serde_json::from_str(response.body_text()).expect("diagnostics JSON");
        assert_eq!(value["raw_source_included"], false);
        assert_eq!(value["credential_values_included"], false);
        assert!(
            value["excluded_from_support_export"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item == "diffs")
        );
    }

    #[test]
    fn account_deletion_needs_biometric_confirmation() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let now = OffsetDateTime::now_utc();
        let response = handle_account_deletion(
            "POST",
            &json!({
                "request_id": "mobile_account_deletion_confirmation",
                "actor_device_id": "iphone_deletion_confirmation",
                "confirm": true,
                "device_signature": "placeholder",
                "issued_at": now.format(&Rfc3339).unwrap(),
                "expires_at": (now + time::Duration::minutes(2)).format(&Rfc3339).unwrap(),
                "user_presence": "device_unlocked"
            })
            .to_string(),
            &kernel,
        )
        .expect("refusal");
        assert!(
            response
                .body_text()
                .contains("mobile_account_deletion_confirmation_required")
        );
    }

    #[test]
    fn account_deletion_requires_registered_device_attestation() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let device_id = "iphone_deletion_attestation";
        let pkcs8 = register_deletion_device(device_id, &kernel);
        let now = OffsetDateTime::now_utc();
        let issued_at = now.format(&Rfc3339).unwrap();
        let expires_at = (now + time::Duration::minutes(2)).format(&Rfc3339).unwrap();
        let mut body = json!({
            "request_id": "mobile_account_deletion_attestation",
            "confirm": true,
            "user_presence": "biometric_verified",
            "actor_device_id": device_id,
            "issued_at": issued_at,
            "expires_at": expires_at,
            "device_signature": "pending"
        });
        assert_eq!(
            String::from_utf8(account_deletion_attestation_payload(&body).unwrap()).unwrap(),
            format!(
                r#"{{"actor_device_id":"iphone_deletion_attestation","confirm":true,"expires_at":"{expires_at}","issued_at":"{issued_at}","request_id":"mobile_account_deletion_attestation","user_presence":"biometric_verified"}}"#
            )
        );

        let mut expired = body.clone();
        expired["issued_at"] = json!((now - time::Duration::minutes(3)).format(&Rfc3339).unwrap());
        expired["expires_at"] = json!((now - time::Duration::minutes(1)).format(&Rfc3339).unwrap());
        attach_deletion_signature(&mut expired, &pkcs8);
        let expired = handle_account_deletion("POST", &expired.to_string(), &kernel).unwrap();
        let expired: Value = serde_json::from_str(expired.body_text()).unwrap();
        assert_eq!(
            expired["error"],
            "mobile_account_deletion_expired_or_future"
        );

        let mut wrong_key = body.clone();
        let other_pkcs8 =
            EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &SystemRandom::new())
                .unwrap();
        attach_deletion_signature(&mut wrong_key, other_pkcs8.as_ref());
        let wrong_key = handle_account_deletion("POST", &wrong_key.to_string(), &kernel).unwrap();
        let wrong_key: Value = serde_json::from_str(wrong_key.body_text()).unwrap();
        assert_eq!(wrong_key["error"], "mobile_device_signature_invalid");

        attach_deletion_signature(&mut body, &pkcs8);
        let accepted = handle_account_deletion("POST", &body.to_string(), &kernel).unwrap();
        let accepted: Value = serde_json::from_str(accepted.body_text()).unwrap();
        assert_eq!(accepted["status"], "PENDING_FULFILLMENT");
        assert_eq!(
            accepted["user_presence_verification"],
            "client_asserted_device_key_bound"
        );
        assert_eq!(
            accepted["device_signature_verification"],
            "verified_registered_device_key"
        );
        assert_eq!(accepted["idempotent_replay"], false);
        let receipt_id = accepted["request_receipt_id"].as_str().unwrap();
        let replay = handle_account_deletion("POST", &body.to_string(), &kernel).unwrap();
        let replay: Value = serde_json::from_str(replay.body_text()).unwrap();
        assert_eq!(replay["status"], "PENDING_FULFILLMENT");
        assert_eq!(replay["idempotent_replay"], true);
        assert_eq!(replay["request_receipt_id"], receipt_id);

        let mut conflict = body.clone();
        conflict["expires_at"] =
            json!((now + time::Duration::minutes(3)).format(&Rfc3339).unwrap());
        attach_deletion_signature(&mut conflict, &pkcs8);
        let conflict = handle_account_deletion("POST", &conflict.to_string(), &kernel).unwrap();
        let conflict: Value = serde_json::from_str(conflict.body_text()).unwrap();
        assert_eq!(
            conflict["error"],
            "mobile_account_deletion_idempotency_conflict"
        );

        let guard = kernel.read().unwrap();
        assert_eq!(
            guard
                .ledger()
                .entries()
                .iter()
                .filter(|receipt| {
                    payload(receipt, "mobile_account_deletion_request_id")
                        == "mobile_account_deletion_attestation"
                })
                .count(),
            1
        );
        let receipt = guard
            .ledger()
            .entries()
            .iter()
            .find(|receipt| receipt.receipt_id == receipt_id)
            .unwrap();
        assert_eq!(
            receipt.payload["mobile_account_deletion_user_presence_verification"],
            "client_asserted_device_key_bound"
        );
        assert_eq!(
            receipt.payload["mobile_account_deletion_device_signature_verification"],
            "verified_registered_device_key"
        );
    }

    #[test]
    fn repository_fingerprint_matches_https_and_ssh_origins() {
        assert_eq!(
            repository_identity_fingerprint("https://github.com/Acme/Forge.git", ""),
            repository_identity_fingerprint("git@github.com:acme/forge.git", "")
        );
        assert_eq!(
            repository_identity_fingerprint("ssh://git@github.com/acme/forge.git", ""),
            repository_identity_fingerprint("https://github.com/acme/forge", "")
        );
    }

    #[test]
    fn mobile_patch_budget_is_explicit_and_bounded() {
        let mut remaining = 10;
        let file = bound_mobile_file(
            json!({"path":"src/lib.rs","patch":"abcdefghijklmnopqrstuvwxyz"}),
            &mut remaining,
        );
        assert_eq!(file["patch_truncated"], true);
        assert_eq!(remaining, 0);
        assert!(file["patch"].as_str().unwrap().starts_with("abcdefghij"));
        assert!(file["patch"].as_str().unwrap().contains("truncated"));
    }

    #[test]
    fn mobile_review_history_is_recent_and_bounded() {
        let mut kernel = MdxKernel::boot_local();
        for index in 0..(MAX_MOBILE_REVIEWS + 3) {
            kernel
                .record_forge_run_event(ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "local_user",
                    run_id: &format!("forge_run_mobile_review_{index:02}"),
                    event: "run_finished",
                    work_item_id: "mobile_review_history",
                    detail: "done",
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                })
                .unwrap();
        }
        let ids = reviewable_session_ids(&kernel, "local_tenant");
        assert_eq!(ids.len(), MAX_MOBILE_REVIEWS);
        assert_eq!(ids.first().unwrap(), "forge_run_mobile_review_14");
        assert_eq!(ids.last().unwrap(), "forge_run_mobile_review_03");
    }
}
