// Connect a repo Forge can push back to. The mdx-self connect wires the repo
// with an empty origin - fine for local runs, but self-delivery (ADR 0488)
// needs a real push target so a run can open its own PR. This governed route
// records a forge.repo.connected receipt with an origin, so a run on that repo
// can branch, push, and open a pull request against it. It grants no ship or
// deploy authority; those stay their own gates.
use crate::RouteResponse;
use mdx_core::{MdxKernel, json_string_literal};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path != "/forge/repo-connections.json" {
        return None;
    }
    if let Some(response) = crate::reject_unless_method(method, "POST") {
        return Some(Ok(response));
    }
    Some(handle(body, kernel))
}

fn json_string_field(body: &str, key: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    value[key].as_str().map(str::to_string)
}

fn refusal(reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            "{{\"name\":\"mdx-forge-repo-connection\",\"status\":\"REFUSED\",\"reason\":{}}}",
            json_string_literal(reason)
        ),
    )
}

fn handle(body: &str, kernel: &Arc<RwLock<MdxKernel>>) -> Result<RouteResponse, String> {
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "human:owner",
        "owner",
    );
    let repo_id = json_string_field(body, "repo_id").unwrap_or_default();
    let root = json_string_field(body, "root").unwrap_or_default();
    if repo_id.trim().is_empty() || root.trim().is_empty() {
        return Ok(refusal("a connection needs a repo_id and a root path"));
    }
    let origin = json_string_field(body, "origin").unwrap_or_default();
    let label = json_string_field(body, "label").unwrap_or_else(|| repo_id.clone());
    let kind = json_string_field(body, "kind").unwrap_or_else(|| "local".to_string());

    let effective_kind = if origin.trim().is_empty() {
        kind
    } else {
        "remote".to_string()
    };
    let default_branch = json_string_field(body, "default_branch").unwrap_or_default();
    let connected = crate::forge_repo_route::connect_prepared_repo(
        &resolved,
        crate::forge_repo_route::PreparedRepo {
            repo_id: &repo_id,
            label: &label,
            root: &root,
            kind: &effective_kind,
            origin: &origin,
            default_branch_hint: &default_branch,
        },
        kernel,
    )?;
    let connected_value: serde_json::Value =
        serde_json::from_str(&connected.body).unwrap_or_default();
    if connected_value["status"].as_str() != Some("CONNECTED") {
        return Ok(connected);
    }

    Ok(RouteResponse::json(
        "200 OK",
        format!(
            "{{\"name\":\"mdx-forge-repo-connection\",\"status\":\"REPO_CONNECTED\",\"repo_id\":{},\"root\":{},\"origin\":{},\"default_branch\":{},\"push_capable\":{},\"receipt_id\":{},\"deployment_authority_granted\":false}}",
            json_string_literal(&repo_id),
            json_string_literal(&root),
            json_string_literal(&origin),
            json_string_literal(connected_value["default_branch"].as_str().unwrap_or("")),
            !origin.trim().is_empty(),
            json_string_literal(connected_value["repo_receipt_id"].as_str().unwrap_or("")),
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connects_a_repo_with_a_push_capable_origin() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let root =
            std::env::temp_dir().join(format!("mdx-forge-repo-connect-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("mkdir");
        let init = std::process::Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["init", "--initial-branch", "trunk"])
            .output()
            .expect("git init");
        assert!(init.status.success());
        let body = serde_json::json!({
            "repo_id": "mdx",
            "root": root,
            "origin": "https://github.com/mdx-os/mdx.git",
            "label": "MDx",
            "default_branch": "trunk"
        })
        .to_string();
        let response = route_response("POST", "/forge/repo-connections.json", &body, &kernel)
            .expect("route matched")
            .expect("handler ran");
        assert_eq!(response.status_for_test(), "200 OK");
        assert!(response.body_text().contains("REPO_CONNECTED"));
        assert!(response.body_text().contains("\"push_capable\":true"));
        let guard = kernel.read().expect("kernel");
        assert_eq!(
            guard.forge_repo_root("mdx").as_deref(),
            body_root(&body).as_deref()
        );
        assert_eq!(
            guard.forge_repo_origin("mdx").as_deref(),
            Some("https://github.com/mdx-os/mdx.git")
        );
        drop(guard);
        let _ = std::fs::remove_dir_all(root);
    }

    fn body_root(body: &str) -> Option<String> {
        serde_json::from_str::<serde_json::Value>(body).ok()?["root"]
            .as_str()
            .map(str::to_string)
    }

    #[test]
    fn refuses_without_a_repo_id_or_root() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = route_response("POST", "/forge/repo-connections.json", "{}", &kernel)
            .expect("route matched")
            .expect("handler ran");
        assert!(response.body_text().contains("needs a repo_id and a root"));
    }
}
