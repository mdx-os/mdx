// Repo standards packet for Forge. This turns allowlisted standards-source
// summaries into a deterministic reviewer checklist that workers, Review, and
// future PR integrations can cite without storing raw standards content.
// Read-only: no provider call, no run admission, no approval, no production
// write.
use crate::RouteResponse;
use mdx_core::{MdxKernel, json_string_literal};
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path != "/forge/repo-standards-packet.json" {
        return None;
    }
    if let Some(response) = crate::reject_unless_method(method, "POST") {
        return Some(Ok(response));
    }
    let repo_id = json_string_field(body, "repo_id").unwrap_or_default();
    if repo_id.trim().is_empty() {
        return Some(Ok(refusal("name the connected repo to inspect")));
    }
    Some(handle(&repo_id, kernel))
}

fn handle(repo_id: &str, kernel: &Arc<RwLock<MdxKernel>>) -> Result<RouteResponse, String> {
    let repo_id = repo_id.trim();
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let mut latest: Option<(&mdx_core::Receipt, BTreeMap<String, String>)> = None;
    for receipt in kernel.ledger().query().by_kind("forge.repo.connected") {
        if receipt.payload.get("repo_id").map(String::as_str) == Some(repo_id) {
            latest = Some((receipt, receipt.payload.clone()));
        }
    }
    let Some((repo_receipt, fields)) = latest else {
        return Ok(refusal("no connected repo by that id"));
    };
    let indexed_profile = kernel.forge_repo_index_profile_json(repo_id);
    let profile_source = if indexed_profile.is_some() {
        "repo_index_cached"
    } else {
        "computed_live"
    };
    let profile_json = indexed_profile.unwrap_or_else(|| {
        fields
            .get("root")
            .map(|root| crate::forge_repo_profile::profile_json(std::path::Path::new(root)))
            .unwrap_or_else(|| crate::forge_repo_profile::profile_json(std::path::Path::new("")))
    });
    let profile: serde_json::Value = serde_json::from_str(&profile_json)
        .unwrap_or_else(|_| serde_json::json!({"language_pack_id":"generic"}));
    let summaries = string_array(&profile["standards_source_summaries"]);
    let sources = string_array(&profile["standards_sources"]);
    let fingerprints = string_array(&profile["standards_source_fingerprints"]);
    let obligations = standards_obligations(&summaries);
    let status = if obligations.is_empty() {
        "NO_STANDARDS_DETECTED"
    } else {
        "STANDARDS_PACKET_READY"
    };
    let safe_next_move = if obligations.is_empty() {
        "Run with the language-pack defaults, then use Review Packet proof and diff evidence."
    } else {
        "Ask the worker to read the cited standards source before changing matching behavior, then verify the checklist in Review."
    };

    Ok(RouteResponse::json(
        "200 OK",
        serde_json::json!({
            "name": "mdx-forge-repo-standards-packet",
            "status": status,
            "repo_id": repo_id,
            "label": fields.get("label").map(String::as_str).unwrap_or(""),
            "repo_receipt_id": repo_receipt.receipt_id,
            "repo_index_receipt_id": kernel.forge_repo_index_receipt_id(repo_id).unwrap_or_default(),
            "profile_source": profile_source,
            "language_pack_id": profile["language_pack_id"].clone(),
            "standards_sources": sources,
            "standards_source_fingerprints": fingerprints,
            "standards_source_summaries": summaries,
            "obligation_count": obligations.len(),
            "obligations": obligations,
            "safe_next_move": safe_next_move,
            "review_packet_route": "/forge/review-packet.json",
            "repo_readiness_route": "/forge/repo-readiness.json",
            "pr_handoff_route": "/forge/pr-handoffs.json",
            "raw_standards_content_recorded": false,
            "provider_calls_allowed": false,
            "adapter_execution_allowed": false,
            "run_started": false,
            "approval_allowed": false,
            "production_write_allowed": false,
        })
        .to_string(),
    ))
}

pub(crate) fn standards_obligations(summaries: &[String]) -> Vec<serde_json::Value> {
    let mut obligations = Vec::new();
    for summary in summaries {
        let Some((source, tags)) = summary.split_once('=') else {
            continue;
        };
        let tags = tags.split('+').collect::<Vec<_>>();
        for (tag, review_axis, worker_instruction) in [
            (
                "agent_instructions",
                "Agent instructions",
                "Read this source before planning, then follow local agent constraints.",
            ),
            (
                "contribution_workflow",
                "Contribution workflow",
                "Preserve the repo's stated contribution, branch, and review workflow.",
            ),
            (
                "ownership_review",
                "Ownership review",
                "Treat owned paths as requiring human reviewer attention in the handoff.",
            ),
            (
                "security_policy",
                "Security policy",
                "Do not weaken security posture; call out security-sensitive changes.",
            ),
            (
                "style_config",
                "Style policy",
                "Use the repo's formatting and style expectations before adding code.",
            ),
            (
                "ci_workflow",
                "CI workflow",
                "Align selected checks and PR handoff with the repo's CI workflow.",
            ),
            (
                "mentions_tests",
                "Test expectation",
                "Show how the selected checks cover the changed behavior.",
            ),
            (
                "mentions_security",
                "Security review",
                "Explain any auth, secret, permission, or input-boundary implications.",
            ),
            (
                "mentions_compatibility",
                "Compatibility review",
                "Preserve public contracts and call out migration or compatibility risk.",
            ),
            (
                "mentions_release",
                "Release review",
                "Call out deployment, versioning, rollout, or changelog implications.",
            ),
        ] {
            if tags.contains(&tag) {
                obligations.push(serde_json::json!({
                    "source": source,
                    "tag": tag,
                    "review_axis": review_axis,
                    "worker_instruction": worker_instruction,
                    "must_read_source_before_claiming_compliance": true,
                    "grants_authority": false,
                }));
            }
        }
    }
    obligations
}

fn string_array(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(ToString::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn refusal(reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-repo-standards-packet","status":"REFUSED","reason":{},"raw_standards_content_recorded":false,"provider_calls_allowed":false,"adapter_execution_allowed":false,"run_started":false,"approval_allowed":false,"production_write_allowed":false}}"#,
            json_string_literal(reason)
        ),
    )
}

fn json_string_field(body: &str, key: &str) -> Option<String> {
    let marker = format!("\"{key}\":");
    let after = body.split(&marker).nth(1)?;
    let after = after.trim_start();
    let rest = after.strip_prefix('"')?;
    let mut value = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(escaped) = chars.next() {
                    value.push(escaped);
                }
            }
            '"' => return Some(value),
            other => value.push(other),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdx_core::{ForgeRepoConnect, GovernedWriteIdentity};

    #[test]
    fn standards_packet_refuses_unknown_repo_without_authority() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = handle("missing", &kernel).expect("response");
        assert!(response.body.contains(r#""status":"REFUSED""#));
        assert!(
            response
                .body
                .contains(r#""raw_standards_content_recorded":false"#)
        );
        assert!(
            response
                .body
                .contains(r#""production_write_allowed":false"#)
        );
    }

    #[test]
    fn standards_packet_derives_obligations_from_allowlisted_summary_tags() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let dir = tempfile_root("standards_packet");
        std::fs::write(
            dir.join("AGENTS.md"),
            "Run tests before handoff. Watch auth and security boundaries.",
        )
        .expect("agents");
        std::fs::create_dir_all(dir.join(".github")).expect("github");
        std::fs::write(dir.join(".github").join("CODEOWNERS"), "* @team/review")
            .expect("codeowners");
        {
            let mut kernel = kernel.write().expect("kernel");
            kernel
                .connect_forge_repo_with_identity(
                    ForgeRepoConnect {
                        tenant_id: "t",
                        actor_id: "human:dev",
                        repo_id: "standards_repo",
                        label: "Standards Repo",
                        root: dir.to_str().unwrap_or(""),
                        kind: "local",
                        origin: "",
                    },
                    &GovernedWriteIdentity::local_demo("human:dev"),
                )
                .expect("connect");
        }
        let response = handle("standards_repo", &kernel).expect("response");
        assert!(
            response
                .body
                .contains(r#""status":"STANDARDS_PACKET_READY""#)
        );
        assert!(
            response
                .body
                .contains(r#""review_axis":"Agent instructions""#)
        );
        assert!(
            response
                .body
                .contains(r#""review_axis":"Ownership review""#)
        );
        assert!(
            response
                .body
                .contains(r#""review_axis":"Test expectation""#)
        );
        assert!(response.body.contains(r#""grants_authority":false"#));
        assert!(
            response
                .body
                .contains(r#""raw_standards_content_recorded":false"#)
        );
    }

    fn tempfile_root(label: &str) -> std::path::PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "mdx_forge_{label}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }
}
