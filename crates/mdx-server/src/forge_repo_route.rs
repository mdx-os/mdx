// Connected repos over HTTP: connect a repo Forge can work in, and read
// the connected list. The kernel records the connection's shape; the
// server validates the path is a real git repo first - the kernel never
// touches the filesystem. Local clones today; remote URLs are the next
// layer (validated differently, recorded the same).
use crate::RouteResponse;
use mdx_core::{
    ForgeRepoConnect, ForgeRepoIndex, ForgeRepoIndexReport, MdxKernel, json_string_literal,
};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

const PLAYGROUND_REPO_ID: &str = "mdx-playground";
const PLAYGROUND_REPO_LABEL: &str = "MDx Playground";
const PLAYGROUND_CHECK: &str = "npm test";

#[derive(Clone, Debug, PartialEq, Eq)]
struct RecordedDefaultBranch {
    name: String,
    source: String,
}

pub(crate) struct PreparedRepo<'a> {
    pub(crate) repo_id: &'a str,
    pub(crate) label: &'a str,
    pub(crate) root: &'a str,
    pub(crate) kind: &'a str,
    pub(crate) origin: &'a str,
    pub(crate) default_branch_hint: &'a str,
}

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/forge/repos.json" => Some(handle_connect(method, body, kernel)),
        "/forge/repo-indexes.json" => Some(handle_index(method, body, kernel)),
        "/forge/repos/projection.json" => Some(handle_projection(method, kernel)),
        "/forge/playground-repo.json" => Some(handle_playground_repo(method, body, kernel)),
        "/forge/local-folder-dialog.json" => Some(handle_local_folder_dialog(method, body)),
        _ if path_base(path) == "/forge/local-folder-list.json" => {
            Some(handle_local_folder_list(method, path))
        }
        _ => None,
    }
}

fn handle_local_folder_dialog(method: &str, body: &str) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "POST") {
        return Ok(response);
    }
    if !json_bool_field(body, "open_dialog") {
        return Ok(local_folder_dialog_status(
            "DIALOG_NOT_REQUESTED",
            "",
            false,
            false,
            "send open_dialog=true to open the local OS folder picker",
        ));
    }
    match open_native_folder_dialog() {
        FolderDialogOutcome::Selected(path) => Ok(local_folder_selection_response(path)),
        FolderDialogOutcome::Cancelled => Ok(local_folder_dialog_status(
            "CANCELLED",
            "",
            false,
            false,
            "operator cancelled the folder picker",
        )),
        FolderDialogOutcome::Unavailable(reason) => Ok(local_folder_dialog_status(
            "DIALOG_UNAVAILABLE",
            "",
            false,
            false,
            &reason,
        )),
    }
}

fn handle_local_folder_list(method: &str, path: &str) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "GET") {
        return Ok(response);
    }
    let Some(requested) = query_param(path, "path").filter(|value| !value.trim().is_empty()) else {
        return Ok(RouteResponse::json(
            "200 OK",
            r#"{"name":"mdx-forge-local-folder-list","status":"PATH_REQUIRED","path":"","parent":"","entries":[],"reason":"pass a path query parameter to list local folders","production_write_allowed":false}"#.to_string(),
        ));
    };
    let resolved = PathBuf::from(expand_home(&requested));
    let canonical = match std::fs::canonicalize(&resolved) {
        Ok(path) => path,
        Err(error) => {
            return Ok(RouteResponse::json(
                "200 OK",
                format!(
                    r#"{{"name":"mdx-forge-local-folder-list","status":"PATH_NOT_FOUND","path":{},"parent":"","entries":[],"reason":{},"production_write_allowed":false}}"#,
                    json_string_literal(&requested),
                    json_string_literal(&error.to_string()),
                ),
            ));
        }
    };
    if !canonical.is_dir() {
        return Ok(RouteResponse::json(
            "200 OK",
            format!(
                r#"{{"name":"mdx-forge-local-folder-list","status":"NOT_A_DIRECTORY","path":{},"parent":"","entries":[],"reason":"path is not a directory","production_write_allowed":false}}"#,
                json_string_literal(&canonical.to_string_lossy()),
            ),
        ));
    }
    let entries = folder_entries(&canonical)?;
    let parent = canonical
        .parent()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_default();
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-local-folder-list","status":"OK","path":{},"parent":{},"entries":[{}],"production_write_allowed":false}}"#,
            json_string_literal(&canonical.to_string_lossy()),
            json_string_literal(&parent),
            entries.join(","),
        ),
    ))
}

fn handle_connect(
    method: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "POST") {
        return Ok(response);
    }
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    let field = |name: &str| json_string_field(body, name).unwrap_or_default();
    let repo_id = field("repo_id");
    let root = field("root");
    let origin_url = field("origin_url");
    let kind = {
        let k = field("kind");
        if !origin_url.trim().is_empty() {
            "remote".to_string()
        } else if k.is_empty() {
            "local".to_string()
        } else {
            k
        }
    };
    // The workspace root runs operate against, and the origin URL we keep
    // for push-back. For a local repo they collapse to the same on-disk
    // path; for a remote repo, root becomes the managed clone and origin
    // holds the URL the engineer pointed us at.
    let (resolved_root, origin) = if kind == "remote" {
        let remote = if origin_url.trim().is_empty() {
            root.clone()
        } else {
            origin_url.clone()
        };
        match clone_remote_repo(&repo_id, &remote) {
            Ok(clone_path) => (clone_path, remote),
            Err(reason) => return Ok(refusal(&reason)),
        }
    } else {
        // A local repo must actually be a git repo on disk. The kernel
        // records only validated shape.
        let path = std::path::Path::new(&root);
        if !path.join(".git").exists() {
            return Ok(refusal(&format!(
                "that path is not a git repository: {root} - connect the root of a cloned repo"
            )));
        }
        (root.clone(), String::new())
    };
    connect_prepared_repo(
        &resolved,
        PreparedRepo {
            repo_id: &repo_id,
            label: &field("label"),
            root: &resolved_root,
            kind: &kind,
            origin: &origin,
            default_branch_hint: &field("default_branch"),
        },
        kernel,
    )
}

pub(crate) fn connect_prepared_repo(
    resolved: &crate::request_security::ResolvedWriteIdentity,
    repo: PreparedRepo<'_>,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if (repo.kind == "remote" && repo.origin.trim().is_empty())
        || (repo.kind == "local" && !repo.origin.is_empty())
        || !matches!(repo.kind, "local" | "remote")
    {
        return Ok(refusal("repository kind and origin are inconsistent"));
    }
    let root_path = Path::new(repo.root);
    if !root_path.join(".git").exists() {
        return Ok(refusal("managed remote checkout is not a git repository"));
    }
    let default_branch = match resolve_default_branch(root_path, repo.default_branch_hint) {
        Ok(value) => value,
        Err(reason) => return Ok(refusal(&reason)),
    };
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let report = match kernel.connect_forge_repo_with_identity(
        ForgeRepoConnect {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            repo_id: repo.repo_id,
            label: repo.label,
            root: repo.root,
            kind: repo.kind,
            origin: repo.origin,
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => return Ok(refusal(&error.message())),
    };
    let index = match record_repo_index(
        &mut kernel,
        resolved,
        &report.repo_id,
        &report.receipt_id,
        repo.root,
        &default_branch,
    ) {
        Ok(index) => index,
        Err(error) => return Ok(refusal(&error.message())),
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-repo-local-post","status":"CONNECTED","auth_session_status":{},"repo_id":{},"default_branch":{},"default_branch_source":{},"repo_receipt_id":{},"repo_index_receipt_id":{},"repo_profile_fingerprint":{},"policy_decision_id":{},"projection_route":"/forge/repos/projection.json","production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&report.repo_id),
            json_string_literal(&default_branch.name),
            json_string_literal(&default_branch.source),
            json_string_literal(&report.receipt_id),
            json_string_literal(&index.receipt_id),
            json_string_literal(&index.profile_fingerprint),
            json_string_literal(&report.policy_decision_id),
        ),
    ))
}

fn handle_index(
    method: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "POST") {
        return Ok(response);
    }
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    let repo_id = json_string_field(body, "repo_id").unwrap_or_default();
    if repo_id.trim().is_empty() {
        return Ok(refusal("repo_id is required to refresh repo intelligence"));
    }
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let Some(root) = kernel.forge_repo_root(&repo_id) else {
        return Ok(refusal("connect the repo before refreshing its index"));
    };
    let repo_receipt_id = latest_repo_receipt_id(&kernel, &repo_id).unwrap_or_default();
    let default_branch = recorded_default_branch(&kernel, &repo_id)
        .map(|(name, source)| RecordedDefaultBranch { name, source })
        .or_else(|| resolve_default_branch(Path::new(&root), "").ok());
    let Some(default_branch) = default_branch else {
        return Ok(refusal(
            "could not preserve or determine the repository default branch",
        ));
    };
    let index = match record_repo_index(
        &mut kernel,
        &resolved,
        &repo_id,
        &repo_receipt_id,
        &root,
        &default_branch,
    ) {
        Ok(index) => index,
        Err(error) => return Ok(refusal(&error.message())),
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-repo-index-local-post","status":"INDEXED","auth_session_status":{},"repo_id":{},"repo_index_receipt_id":{},"repo_profile_fingerprint":{},"projection_route":"/forge/repos/projection.json","production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&index.repo_id),
            json_string_literal(&index.receipt_id),
            json_string_literal(&index.profile_fingerprint),
        ),
    ))
}

fn handle_playground_repo(
    method: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "POST") {
        return Ok(response);
    }
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    let root_path = match playground_repo_path() {
        Ok(path) => path,
        Err(reason) => return Ok(refusal(&reason)),
    };
    if let Err(reason) = seed_playground_repo_at(&root_path) {
        return Ok(refusal(&reason));
    }
    let root = root_path.to_string_lossy().to_string();
    let default_branch = match resolve_default_branch(&root_path, "") {
        Ok(value) => value,
        Err(reason) => return Ok(refusal(&reason)),
    };
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let report = match kernel.connect_forge_repo_with_identity(
        ForgeRepoConnect {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            repo_id: PLAYGROUND_REPO_ID,
            label: PLAYGROUND_REPO_LABEL,
            root: &root,
            kind: "local",
            origin: "",
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => return Ok(refusal(&error.message())),
    };
    let index = match record_repo_index(
        &mut kernel,
        &resolved,
        &report.repo_id,
        &report.receipt_id,
        &root,
        &default_branch,
    ) {
        Ok(index) => index,
        Err(error) => return Ok(refusal(&error.message())),
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-playground-repo-local-post","status":"CONNECTED","auth_session_status":{},"repo_id":{},"label":{},"root":{},"repo_receipt_id":{},"repo_index_receipt_id":{},"repo_profile_fingerprint":{},"suggested_check":{},"line":"Playground connected - broad demo builds can run here without touching MDx self.","production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&report.repo_id),
            json_string_literal(PLAYGROUND_REPO_LABEL),
            json_string_literal(&root),
            json_string_literal(&report.receipt_id),
            json_string_literal(&index.receipt_id),
            json_string_literal(&index.profile_fingerprint),
            json_string_literal(PLAYGROUND_CHECK),
        ),
    ))
}

fn handle_projection(
    method: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "GET") {
        return Ok(response);
    }
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    // Fold to the latest connection per repo id.
    use std::collections::BTreeMap;
    let mut latest: BTreeMap<String, (String, BTreeMap<String, String>)> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    for receipt in kernel
        .ledger()
        .query()
        .by_kind("forge.repo.connected")
        .iter()
    {
        let repo_id = receipt.payload.get("repo_id").cloned().unwrap_or_default();
        if repo_id.is_empty() {
            continue;
        }
        if !latest.contains_key(&repo_id) {
            order.push(repo_id.clone());
        }
        latest.insert(
            repo_id,
            (receipt.receipt_id.clone(), receipt.payload.clone()),
        );
    }
    let repos: Vec<String> = order
        .iter()
        .map(|repo_id| {
            let (receipt_id, fields) = &latest[repo_id];
            let value =
                |key: &str| json_string_literal(fields.get(key).map(String::as_str).unwrap_or(""));
            let root = fields.get("root").map(String::as_str).unwrap_or("");
            let indexed_profile = kernel.forge_repo_index_profile_json(repo_id);
            let default_branch = indexed_profile
                .as_deref()
                .and_then(default_branch_from_profile);
            let (profile, profile_source) =
                crate::forge_repo_profile::profile_json_with_current_setup(
                    std::path::Path::new(root),
                    indexed_profile,
                );
            let index_receipt_id = kernel.forge_repo_index_receipt_id(repo_id).unwrap_or_default();
            format!(
                r#"{{"repo_id":{},"label":{},"root":{},"kind":{},"origin_url":{},"default_branch":{},"repo_receipt_id":{},"repo_index_receipt_id":{},"profile_source":{},"profile":{}}}"#,
                json_string_literal(repo_id),
                value("label"),
                value("root"),
                value("kind"),
                value("origin_url"),
                json_string_literal(
                    default_branch
                        .as_ref()
                        .map(|branch| branch.name.as_str())
                        .unwrap_or(""),
                ),
                json_string_literal(receipt_id),
                json_string_literal(&index_receipt_id),
                json_string_literal(profile_source),
                profile,
            )
        })
        .collect();
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-repo-local-projection","receipt_kind":"forge.repo.connected","index_receipt_kind":"forge.repo.indexed","repo_count":{},"repos":[{}],"production_write_allowed":false}}"#,
            repos.len(),
            repos.join(","),
        ),
    ))
}

enum FolderDialogOutcome {
    Selected(PathBuf),
    Cancelled,
    Unavailable(String),
}

fn open_native_folder_dialog() -> FolderDialogOutcome {
    #[cfg(target_os = "macos")]
    {
        return run_folder_dialog_command(
            "osascript",
            &[
                "-e",
                "POSIX path of (choose folder with prompt \"Choose a repository folder for Forge\")",
            ],
        );
    }
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
            return FolderDialogOutcome::Unavailable(
                "no graphical display is available for a native folder picker".to_string(),
            );
        }
        if command_available("zenity") {
            return run_folder_dialog_command(
                "zenity",
                &[
                    "--file-selection",
                    "--directory",
                    "--title=Choose a repository folder for Forge",
                ],
            );
        }
        if command_available("kdialog") {
            return run_folder_dialog_command("kdialog", &["--getexistingdirectory"]);
        }
        return FolderDialogOutcome::Unavailable(
            "no supported folder picker command found (zenity or kdialog)".to_string(),
        );
    }
    #[cfg(target_os = "windows")]
    {
        return run_folder_dialog_command(
            "powershell",
            &[
                "-NoProfile",
                "-Command",
                "Add-Type -AssemblyName System.Windows.Forms; $d = New-Object System.Windows.Forms.FolderBrowserDialog; $d.Description = 'Choose a repository folder for Forge'; if ($d.ShowDialog() -eq 'OK') { $d.SelectedPath }",
            ],
        );
    }
    #[allow(unreachable_code)]
    FolderDialogOutcome::Unavailable(
        "native folder picker is not implemented for this operating system".to_string(),
    )
}

fn run_folder_dialog_command(command: &str, args: &[&str]) -> FolderDialogOutcome {
    match std::process::Command::new(command).args(args).output() {
        Ok(output) if output.status.success() => {
            let selected = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if selected.is_empty() {
                FolderDialogOutcome::Cancelled
            } else {
                FolderDialogOutcome::Selected(PathBuf::from(selected))
            }
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if stderr.is_empty() || stderr.to_ascii_lowercase().contains("cancel") {
                FolderDialogOutcome::Cancelled
            } else {
                FolderDialogOutcome::Unavailable(stderr)
            }
        }
        Err(error) => FolderDialogOutcome::Unavailable(error.to_string()),
    }
}

#[cfg(target_os = "linux")]
fn command_available(command: &str) -> bool {
    std::process::Command::new("sh")
        .args(["-c", &format!("command -v {command} >/dev/null 2>&1")])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn local_folder_selection_response(path: PathBuf) -> RouteResponse {
    match std::fs::canonicalize(&path) {
        Ok(canonical) if canonical.is_dir() => local_folder_dialog_status(
            "OK",
            &canonical.to_string_lossy(),
            true,
            canonical.join(".git").exists(),
            "",
        ),
        Ok(canonical) => local_folder_dialog_status(
            "NOT_A_DIRECTORY",
            &canonical.to_string_lossy(),
            canonical.exists(),
            false,
            "selected path is not a directory",
        ),
        Err(error) => local_folder_dialog_status(
            "PATH_NOT_FOUND",
            &path.to_string_lossy(),
            false,
            false,
            &error.to_string(),
        ),
    }
}

fn local_folder_dialog_status(
    status: &str,
    path: &str,
    exists: bool,
    is_git_repo: bool,
    reason: &str,
) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-local-folder-dialog","status":{},"path":{},"exists":{},"is_git_repo":{},"reason":{},"connect_route":"/forge/repos.json","records_receipt":false,"production_write_allowed":false}}"#,
            json_string_literal(status),
            json_string_literal(path),
            exists,
            is_git_repo,
            json_string_literal(reason),
        ),
    )
}

fn folder_entries(path: &Path) -> Result<Vec<String>, String> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(path).map_err(|error| error.to_string())? {
        let Ok(entry) = entry else {
            continue;
        };
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let entry_path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name == "." || name == ".." {
            continue;
        }
        entries.push(format!(
            r#"{{"name":{},"path":{},"is_dir":true,"is_git_repo":{}}}"#,
            json_string_literal(&name),
            json_string_literal(&entry_path.to_string_lossy()),
            entry_path.join(".git").exists(),
        ));
    }
    entries.sort();
    Ok(entries)
}

fn default_folder_list_root() -> String {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string())
}

fn expand_home(path: &str) -> String {
    if path == "~" {
        return default_folder_list_root();
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return format!("{}/{}", default_folder_list_root(), rest);
    }
    path.to_string()
}

fn playground_repo_path() -> Result<PathBuf, String> {
    let root = mdx_workspace_root()?;
    let local_root = root.join(".mdx-local");
    let playground = local_root.join("forge-playground");
    if !playground.starts_with(&local_root) {
        return Err("playground path escaped the MDx local workspace".to_string());
    }
    Ok(playground)
}

fn mdx_workspace_root() -> Result<PathBuf, String> {
    if let Ok(root) = std::env::var("MDX_REPO_ROOT")
        && !root.trim().is_empty()
    {
        return validate_mdx_workspace_root(PathBuf::from(root), "MDX_REPO_ROOT");
    }
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let Some(root) = manifest_dir.parent().and_then(Path::parent) else {
        return Err("could not locate the compiled MDx workspace root".to_string());
    };
    validate_mdx_workspace_root(root.to_path_buf(), "compiled workspace root")
}

fn validate_mdx_workspace_root(root: PathBuf, source: &str) -> Result<PathBuf, String> {
    let root = std::fs::canonicalize(&root)
        .map_err(|error| format!("could not resolve {source} {}: {error}", root.display()))?;
    let markers = [
        root.join("Cargo.toml"),
        root.join("crates").join("mdx-server").join("Cargo.toml"),
        root.join("apps").join("mdx-host").join("package.json"),
    ];
    if markers.iter().all(|path| path.is_file()) {
        return Ok(root);
    }
    Err(format!(
        "{source} {} is not an MDx workspace root",
        root.display()
    ))
}

fn seed_playground_repo_at(path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(path.join("src"))
        .map_err(|error| format!("could not create playground source folder: {error}"))?;
    std::fs::create_dir_all(path.join("scripts"))
        .map_err(|error| format!("could not create playground scripts folder: {error}"))?;
    write_seed_file(
        &path.join("README.md"),
        "# MDx Playground\n\nThis local repo is intentionally small. Forge can use it for first builds, demos, and playful experiments without editing the MDx monorepo.\n\nRun `npm test` to prove the project shape.\n",
    )?;
    write_seed_file(
        &path.join("package.json"),
        "{\n  \"name\": \"mdx-playground\",\n  \"private\": true,\n  \"type\": \"module\",\n  \"scripts\": {\n    \"test\": \"node scripts/check.mjs\",\n    \"dev\": \"python3 -m http.server 5173\"\n  }\n}\n",
    )?;
    write_seed_file(
        &path.join("index.html"),
        "<!doctype html>\n<html lang=\"en\">\n<head>\n  <meta charset=\"utf-8\">\n  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n  <title>MDx Playground</title>\n  <link rel=\"stylesheet\" href=\"src/style.css\">\n</head>\n<body>\n  <main class=\"stage\">\n    <h1>MDx Playground</h1>\n    <p>Ask Forge to turn this into a tiny game, page, or demo.</p>\n    <button id=\"spark\">Add a spark</button>\n    <p id=\"spark-count\" aria-live=\"polite\">0 sparks</p>\n  </main>\n  <script type=\"module\" src=\"src/main.js\"></script>\n</body>\n</html>\n",
    )?;
    write_seed_file(
        &path.join("src").join("main.js"),
        "const button = document.querySelector('#spark');\nconst count = document.querySelector('#spark-count');\nlet sparks = 0;\n\nbutton?.addEventListener('click', () => {\n  sparks += 1;\n  if (count) count.textContent = `${sparks} ${sparks === 1 ? 'spark' : 'sparks'}`;\n});\n",
    )?;
    write_seed_file(
        &path.join("src").join("style.css"),
        ":root { color-scheme: light; font-family: Inter, ui-sans-serif, system-ui, sans-serif; }\nbody { margin: 0; min-height: 100vh; display: grid; place-items: center; background: #f6f7fb; color: #17181c; }\n.stage { width: min(640px, calc(100vw - 32px)); padding: 36px; border: 1px solid #dde1ea; border-radius: 14px; background: white; box-shadow: 0 18px 50px rgba(20, 30, 50, 0.08); }\nh1 { margin: 0 0 10px; font-size: 40px; }\np { color: #5c6370; line-height: 1.5; }\nbutton { border: 0; border-radius: 10px; padding: 12px 16px; background: #3f7df6; color: white; font: inherit; font-weight: 700; cursor: pointer; }\n",
    )?;
    write_seed_file(
        &path.join("scripts").join("check.mjs"),
        "import { readFileSync } from 'node:fs';\n\nconst required = ['index.html', 'src/main.js', 'src/style.css'];\nfor (const file of required) {\n  readFileSync(new URL(`../${file}`, import.meta.url), 'utf8');\n}\nconst pkg = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8'));\nif (pkg.scripts?.test !== 'node scripts/check.mjs') {\n  throw new Error('package.json must keep npm test wired to scripts/check.mjs');\n}\nconsole.log('playground_check: OK');\n",
    )?;
    ensure_git_repo(path)?;
    Ok(())
}

fn write_seed_file(path: &Path, contents: &str) -> Result<(), String> {
    if path.exists() {
        return Ok(());
    }
    std::fs::write(path, contents)
        .map_err(|error| format!("could not write {}: {error}", path.display()))
}

fn ensure_git_repo(path: &Path) -> Result<(), String> {
    if !path.join(".git").exists() {
        run_git(path, &["init"])?;
    }
    run_git(
        path,
        &["config", "user.email", "forge-playground@mdx.local"],
    )?;
    run_git(path, &["config", "user.name", "MDx Forge Playground"])?;
    let has_head = std::process::Command::new("git")
        .current_dir(path)
        .args(["rev-parse", "--verify", "HEAD"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    if !has_head {
        run_git(path, &["add", "."])?;
        run_git(path, &["commit", "-m", "Seed MDx playground"])?;
    }
    Ok(())
}

fn run_git(path: &Path, args: &[&str]) -> Result<(), String> {
    let output = std::process::Command::new("git")
        .current_dir(path)
        .args(args)
        .output()
        .map_err(|error| format!("could not run git {}: {error}", args.join(" ")))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(format!(
        "git {} failed in {}: {}",
        args.join(" "),
        path.display(),
        stderr.lines().last().unwrap_or("").trim()
    ))
}

fn query_param(path: &str, key: &str) -> Option<String> {
    let query = path.split_once('?')?.1;
    for pair in query.split('&') {
        let (raw_key, raw_value) = pair.split_once('=').unwrap_or((pair, ""));
        if percent_decode(raw_key) == key {
            return Some(percent_decode(raw_value));
        }
    }
    None
}

fn path_base(path: &str) -> &str {
    path.split_once('?').map(|(base, _)| base).unwrap_or(path)
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hex = &value[i + 1..i + 3];
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    out.push(byte);
                    i += 3;
                } else {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

/// Clone (or refresh) a remote repo into the managed forge-repos area and
/// return the clone path. The URL is user-supplied, so we validate the
/// scheme and pass it after `--` so it can never be read as a git flag,
/// and we reach it with `git ls-remote` before committing to a clone.
/// Auth uses the host's own git credentials - the kernel holds no secret.
fn clone_remote_repo(repo_id: &str, url: &str) -> Result<String, String> {
    let repo_id = repo_id.trim();
    if repo_id.is_empty()
        || !repo_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(format!(
            "give the repo a simple id (letters, numbers, dash, underscore) before connecting a remote: got {repo_id}"
        ));
    }
    let url = url.trim();
    let scheme_ok = url.starts_with("https://")
        || url.starts_with("http://")
        || url.starts_with("ssh://")
        || url.starts_with("git@");
    if !scheme_ok {
        return Err(format!(
            "that is not a clone URL: {url} - use an https://, ssh://, or git@ remote"
        ));
    }
    // Reachable and authorized? ls-remote is cheap and proves both before
    // we spend a clone.
    let reachable = std::process::Command::new("git")
        .args(["ls-remote", "--", url])
        .output();
    match reachable {
        Ok(output) if output.status.success() => {}
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let hint = stderr.lines().last().unwrap_or("").trim();
            return Err(format!(
                "could not reach that remote - check the URL and that this machine is authorized: {hint}"
            ));
        }
        Err(error) => return Err(format!("could not run git to reach the remote: {error}")),
    }
    let base = managed_remote_repo_base();
    if let Err(error) = std::fs::create_dir_all(&base) {
        return Err(format!("could not prepare the clone area: {error}"));
    }
    let clone_path = base.join(repo_id);
    let clone_path_str = clone_path.to_string_lossy().to_string();
    if clone_path.join(".git").exists() {
        // Already cloned once: refresh it so a re-connect picks up new work.
        let fetched = std::process::Command::new("git")
            .current_dir(&clone_path)
            .args(["fetch", "--all", "--prune"])
            .output();
        match fetched {
            Ok(output) if output.status.success() => return Ok(clone_path_str),
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(format!(
                    "the repo is connected but refreshing it failed: {}",
                    stderr.lines().last().unwrap_or("").trim()
                ));
            }
            Err(error) => return Err(format!("could not refresh the clone: {error}")),
        }
    }
    let cloned = std::process::Command::new("git")
        .args(["clone", "--", url, &clone_path_str])
        .output();
    match cloned {
        Ok(output) if output.status.success() => Ok(clone_path_str),
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!(
                "could not clone that remote: {}",
                stderr.lines().last().unwrap_or("").trim()
            ))
        }
        Err(error) => Err(format!("could not run git clone: {error}")),
    }
}

pub(crate) fn ensure_managed_remote_checkout(
    kernel: &Arc<RwLock<MdxKernel>>,
    repo_id: &str,
) -> Result<Option<PathBuf>, String> {
    ensure_managed_remote_checkout_with(kernel, repo_id, clone_remote_repo)
}

fn ensure_managed_remote_checkout_with<F>(
    kernel: &Arc<RwLock<MdxKernel>>,
    repo_id: &str,
    clone_repo: F,
) -> Result<Option<PathBuf>, String>
where
    F: FnOnce(&str, &str) -> Result<String, String>,
{
    let (recorded_root, origin) = {
        let kernel = kernel
            .read()
            .map_err(|_| "kernel lock poisoned while recovering remote checkout".to_string())?;
        let Some(root) = kernel.forge_repo_root(repo_id) else {
            return Ok(None);
        };
        (root, kernel.forge_repo_origin(repo_id))
    };
    let recorded_path = PathBuf::from(&recorded_root);
    if recorded_path.join(".git").is_dir() {
        return Ok(Some(recorded_path));
    }
    let Some(origin) = origin else {
        return Ok(None);
    };
    let recovered_root = clone_repo(repo_id, &origin)?;
    if recovered_root != recorded_root {
        return Err("recovered checkout did not match its recorded managed root".to_string());
    }
    Ok(Some(PathBuf::from(recovered_root)))
}

fn managed_remote_repo_base() -> PathBuf {
    std::env::var("MDX_CLOUD_REPO_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(".mdx-local/forge-repos"))
}

fn record_repo_index(
    kernel: &mut MdxKernel,
    resolved: &crate::request_security::ResolvedWriteIdentity,
    repo_id: &str,
    repo_receipt_id: &str,
    root: &str,
    default_branch: &RecordedDefaultBranch,
) -> Result<ForgeRepoIndexReport, mdx_core::ForgeRepoError> {
    let root_path = std::path::Path::new(root);
    let profile = crate::forge_repo_profile::profile_repo(root_path);
    let mut profile_value: serde_json::Value =
        serde_json::from_str(&crate::forge_repo_profile::profile_json(root_path))
            .unwrap_or_else(|_| serde_json::json!({}));
    if let Some(object) = profile_value.as_object_mut() {
        object.insert(
            "default_branch".to_string(),
            serde_json::Value::String(default_branch.name.clone()),
        );
        object.insert(
            "default_branch_source".to_string(),
            serde_json::Value::String(default_branch.source.clone()),
        );
    }
    let profile_json = profile_value.to_string();
    let profile_fingerprint = profile_fingerprint(&profile_json);
    let detected_language_packs = profile.detected_language_packs.join(",");
    let semantic_tool_readiness = profile.semantic_tool_readiness.join(",");
    let toolchain_readiness = profile.toolchain_readiness.join(",");
    let standards_source_summaries = profile.standards_source_summaries.join(",");
    kernel.index_forge_repo_with_identity(
        ForgeRepoIndex {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            repo_id,
            repo_receipt_id,
            profile_json: &profile_json,
            profile_fingerprint: &profile_fingerprint,
            primary_language: profile.primary_language,
            language_pack_id: profile.language_pack_id,
            detected_language_packs: &detected_language_packs,
            semantic_tool_readiness: &semantic_tool_readiness,
            toolchain_readiness: &toolchain_readiness,
            proof_plan_status: profile.proof_plan_status,
            standards_source_summaries: &standards_source_summaries,
        },
        &resolved.identity,
    )
}

pub(crate) fn recorded_default_branch(
    kernel: &MdxKernel,
    repo_id: &str,
) -> Option<(String, String)> {
    kernel
        .forge_repo_index_profile_json(repo_id)
        .as_deref()
        .and_then(default_branch_from_profile)
        .map(|branch| (branch.name, branch.source))
}

fn default_branch_from_profile(profile_json: &str) -> Option<RecordedDefaultBranch> {
    let value: serde_json::Value = serde_json::from_str(profile_json).ok()?;
    let name = value["default_branch"].as_str()?.trim();
    let source = value["default_branch_source"].as_str()?.trim();
    if !is_safe_default_branch(name)
        || !matches!(
            source,
            "caller_provided" | "origin_head" | "current_head_fallback"
        )
    {
        return None;
    }
    Some(RecordedDefaultBranch {
        name: name.to_string(),
        source: source.to_string(),
    })
}

fn resolve_default_branch(root: &Path, requested: &str) -> Result<RecordedDefaultBranch, String> {
    let requested = requested.trim();
    if !requested.is_empty() {
        if !is_safe_default_branch(requested) {
            return Err("default_branch must be a safe Git branch name".to_string());
        }
        return Ok(RecordedDefaultBranch {
            name: requested.to_string(),
            source: "caller_provided".to_string(),
        });
    }
    if let Some(remote_head) = git_text(
        root,
        &[
            "symbolic-ref",
            "--quiet",
            "--short",
            "refs/remotes/origin/HEAD",
        ],
    ) {
        let branch = remote_head.strip_prefix("origin/").unwrap_or(&remote_head);
        if is_safe_default_branch(branch) {
            return Ok(RecordedDefaultBranch {
                name: branch.to_string(),
                source: "origin_head".to_string(),
            });
        }
    }
    if let Some(head) = git_text(root, &["symbolic-ref", "--quiet", "--short", "HEAD"])
        && is_safe_default_branch(&head)
    {
        return Ok(RecordedDefaultBranch {
            name: head,
            source: "current_head_fallback".to_string(),
        });
    }
    Err(
        "could not determine a default branch; pass default_branch when connecting the repository"
            .to_string(),
    )
}

fn git_text(root: &Path, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!value.is_empty()).then_some(value)
}

pub(crate) fn is_safe_default_branch(value: &str) -> bool {
    let value = value.trim();
    if value.is_empty() || value.len() > 240 || value.starts_with('-') {
        return false;
    }
    std::process::Command::new("git")
        .args(["check-ref-format", "--branch", value])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn latest_repo_receipt_id(kernel: &MdxKernel, repo_id: &str) -> Option<String> {
    let repo_id = repo_id.trim();
    if repo_id.is_empty() {
        return None;
    }
    let mut latest = None;
    for receipt in kernel
        .ledger()
        .query()
        .by_kind("forge.repo.connected")
        .iter()
    {
        if receipt.payload.get("repo_id").map(String::as_str) == Some(repo_id) {
            latest = Some(receipt.receipt_id.clone());
        }
    }
    latest
}

fn refusal(reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-repo-local-post","status":"REFUSED","reason":{},"repo_receipt_id":"","production_write_allowed":false}}"#,
            json_string_literal(reason)
        ),
    )
}

fn profile_fingerprint(profile_json: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in profile_json.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("fnv1a64:{hash:016x}")
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

fn json_bool_field(body: &str, key: &str) -> bool {
    let marker = format!("\"{key}\":");
    let Some(after) = body.split(&marker).nth(1) else {
        return false;
    };
    after.trim_start().starts_with("true")
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdx_core::{GovernedWriteIdentity, MdxKernel};
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<std::ffi::OsString>,
    }

    impl EnvVarGuard {
        fn set_path(key: &'static str, value: &Path) -> Self {
            let previous = std::env::var_os(key);
            unsafe { std::env::set_var(key, value) };
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            if let Some(value) = &self.previous {
                unsafe { std::env::set_var(self.key, value) };
            } else {
                unsafe { std::env::remove_var(self.key) };
            }
        }
    }

    struct TempRepo {
        path: std::path::PathBuf,
    }

    impl TempRepo {
        fn swift() -> Self {
            let unique = format!(
                "mdx-forge-repo-index-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );
            let path = std::env::temp_dir().join(unique);
            std::fs::create_dir_all(path.join(".git")).expect("git marker");
            std::fs::write(path.join("Package.swift"), "// swift-tools-version: 5.9\n")
                .expect("package");
            Self { path }
        }

        fn node_with_lock() -> Self {
            let unique = format!(
                "mdx-forge-repo-index-node-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );
            let path = std::env::temp_dir().join(unique);
            std::fs::create_dir_all(path.join(".git")).expect("git marker");
            std::fs::write(
                path.join("package.json"),
                "{\"scripts\":{\"test\":\"node test.js\"}}\n",
            )
            .expect("package");
            std::fs::write(path.join("package-lock.json"), "{}\n").expect("lock");
            Self { path }
        }

        fn folder_tree() -> Self {
            let unique = format!(
                "mdx-forge-local-folder-list-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );
            let path = std::env::temp_dir().join(unique);
            std::fs::create_dir_all(path.join("git-repo").join(".git")).expect("git marker");
            std::fs::create_dir_all(path.join("plain-folder")).expect("plain folder");
            std::fs::write(path.join("README.md"), "not listed\n").expect("file");
            Self { path }
        }
    }

    impl Drop for TempRepo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn connect_route_records_repo_index_and_projection_uses_cached_profile() {
        let repo = TempRepo::swift();
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let body = format!(
            r#"{{"repo_id":"swift-canary","label":"Swift Canary","root":{},"kind":"local","default_branch":"trunk"}}"#,
            json_string_literal(repo.path.to_str().unwrap())
        );

        let response = handle_connect("POST", &body, &kernel).expect("connect");

        assert!(response.body.contains(r#""status":"CONNECTED""#));
        assert!(response.body.contains(r#""repo_index_receipt_id":"#));
        assert!(response.body.contains(r#""default_branch":"trunk""#));
        assert!(
            response
                .body
                .contains(r#""repo_profile_fingerprint":"fnv1a64:"#)
        );

        std::fs::remove_file(repo.path.join("Package.swift")).expect("remove package");
        let projection = handle_projection("GET", &kernel).expect("projection");

        assert!(
            projection
                .body
                .contains(r#""index_receipt_kind":"forge.repo.indexed""#)
        );
        assert!(
            projection
                .body
                .contains(r#""profile_source":"repo_index_cached""#)
        );
        assert!(
            projection
                .body
                .contains(r#""language_pack_id":"swift-spm""#)
        );
        assert!(projection.body.contains(r#""repo_index_receipt_id":"#));

        std::fs::write(repo.path.join("pom.xml"), "<project></project>\n").expect("pom");
        let refresh = handle_index(
            "POST",
            r#"{"repo_id":"swift-canary","actor_id":"human:eng"}"#,
            &kernel,
        )
        .expect("refresh index");

        assert!(refresh.body.contains(r#""status":"INDEXED""#));
        assert!(
            refresh
                .body
                .contains(r#""repo_profile_fingerprint":"fnv1a64:"#)
        );

        let refreshed_projection = handle_projection("GET", &kernel).expect("refreshed projection");
        assert!(
            refreshed_projection
                .body
                .contains(r#""profile_source":"repo_index_cached""#)
        );
        assert!(
            refreshed_projection
                .body
                .contains(r#""language_pack_id":"java-maven""#)
        );
    }

    #[test]
    fn projection_refreshes_stale_node_index_to_keep_setup_in_checks() {
        let repo = TempRepo::node_with_lock();
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let identity = GovernedWriteIdentity::local_demo("human:eng");
        let body = format!(
            r#"{{"repo_id":"node-canary","label":"Node Canary","root":{},"kind":"local","default_branch":"trunk"}}"#,
            json_string_literal(repo.path.to_str().unwrap())
        );

        let response = handle_connect("POST", &body, &kernel).expect("connect");
        assert!(response.body.contains(r#""status":"CONNECTED""#));
        {
            let mut kernel = kernel.write().expect("kernel");
            let repo_receipt_id =
                latest_repo_receipt_id(&kernel, "node-canary").expect("repo receipt");
            kernel
                .index_forge_repo_with_identity(
                    ForgeRepoIndex {
                        tenant_id: "t",
                        actor_id: "human:eng",
                        repo_id: "node-canary",
                        repo_receipt_id: &repo_receipt_id,
                        profile_json: r#"{"language_pack_id":"node","suggested_checks":["npm test"]}"#,
                        profile_fingerprint: "fnv1a64:stale",
                        primary_language: "javascript-typescript",
                        language_pack_id: "node",
                        detected_language_packs: "node",
                        semantic_tool_readiness: "",
                        toolchain_readiness: "",
                        proof_plan_status: "ready",
                        standards_source_summaries: "",
                    },
                    &identity,
                )
                .expect("stale index");
        }

        let projection = handle_projection("GET", &kernel).expect("projection");
        let value: serde_json::Value = serde_json::from_str(&projection.body).expect("json");
        let profile = &value["repos"][0]["profile"];

        assert_eq!(
            value["repos"][0]["profile_source"],
            "computed_live_stale_index"
        );
        assert_eq!(profile["suggested_checks"][0], "npm ci");
        assert_eq!(profile["suggested_checks"][1], "npm test");
    }

    #[test]
    fn playground_route_seeds_and_connects_lightweight_demo_repo() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let unique = format!(
            "mdx-forge-playground-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let workspace = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(workspace.join("crates").join("mdx-server"))
            .expect("crate marker dir");
        std::fs::create_dir_all(workspace.join("apps").join("mdx-host")).expect("app marker dir");
        std::fs::write(workspace.join("Cargo.toml"), "[workspace]\n").expect("workspace marker");
        std::fs::write(
            workspace
                .join("crates")
                .join("mdx-server")
                .join("Cargo.toml"),
            "[package]\nname = \"mdx-server\"\nversion = \"0.0.0\"\n",
        )
        .expect("crate marker");
        std::fs::write(
            workspace.join("apps").join("mdx-host").join("package.json"),
            "{\"name\":\"mdx-host\"}\n",
        )
        .expect("app marker");
        let _env = EnvVarGuard::set_path("MDX_REPO_ROOT", &workspace);
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let response = handle_playground_repo(
            "POST",
            r#"{"actor_id":"human:eng","tenant_id":"local_tenant"}"#,
            &kernel,
        )
        .expect("connect playground");
        let path = workspace.join(".mdx-local").join("forge-playground");

        assert!(response.body.contains(r#""status":"CONNECTED""#));
        assert!(response.body.contains(r#""repo_id":"mdx-playground""#));
        assert!(response.body.contains(r#""suggested_check":"npm test""#));
        assert!(path.join(".git").exists());
        assert!(path.join("index.html").exists());
        assert!(path.join("scripts").join("check.mjs").exists());

        let projection = handle_projection("GET", &kernel).expect("projection");
        assert!(projection.body.contains(PLAYGROUND_REPO_ID));
        assert!(projection.body.contains("npm test"));

        let _ = std::fs::remove_dir_all(workspace);
    }

    #[test]
    fn origin_url_promotes_repo_intake_to_remote_without_requiring_root() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let response = handle_connect(
            "POST",
            r#"{"repo_id":"remote-canary","label":"Remote Canary","origin_url":"not-a-clone-url"}"#,
            &kernel,
        )
        .expect("connect response");

        assert!(response.body.contains(r#""status":"REFUSED""#));
        assert!(response.body.contains("that is not a clone URL"));
        assert!(response.body.contains("not-a-clone-url"));
        assert!(
            response
                .body
                .contains(r#""production_write_allowed":false"#)
        );
    }

    #[test]
    fn missing_remote_checkout_rehydrates_from_receipted_origin() {
        let unique = format!(
            "mdx-forge-remote-recovery-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        let root_text = root.to_string_lossy().to_string();
        let mut kernel = MdxKernel::boot_local();
        kernel
            .connect_forge_repo(ForgeRepoConnect {
                tenant_id: "local_tenant",
                actor_id: "human:eng",
                repo_id: "remote-recovery",
                label: "Remote Recovery",
                root: &root_text,
                kind: "remote",
                origin: "https://github.com/example/recovery.git",
            })
            .expect("record remote repository");
        let kernel = Arc::new(RwLock::new(kernel));

        let recovered =
            ensure_managed_remote_checkout_with(&kernel, "remote-recovery", |repo_id, origin| {
                assert_eq!(repo_id, "remote-recovery");
                assert_eq!(origin, "https://github.com/example/recovery.git");
                std::fs::create_dir_all(root.join(".git")).expect("recovered checkout");
                Ok(root_text.clone())
            })
            .expect("recover checkout")
            .expect("remote checkout");

        assert_eq!(recovered, root);
        assert!(recovered.join(".git").is_dir());
        let _ = std::fs::remove_dir_all(recovered);
    }

    #[test]
    fn local_folder_dialog_does_not_open_without_explicit_request() {
        let response = handle_local_folder_dialog("POST", "").expect("dialog");

        assert!(response.body.contains(r#""status":"DIALOG_NOT_REQUESTED""#));
        assert!(response.body.contains(r#""records_receipt":false"#));
        assert!(
            response
                .body
                .contains(r#""connect_route":"/forge/repos.json""#)
        );
    }

    #[test]
    fn local_folder_selection_reports_git_repo_without_connecting_it() {
        let repo = TempRepo::swift();

        let response = local_folder_selection_response(repo.path.clone());

        assert!(response.body.contains(r#""status":"OK""#));
        assert!(response.body.contains(r#""exists":true"#));
        assert!(response.body.contains(r#""is_git_repo":true"#));
        assert!(response.body.contains(r#""records_receipt":false"#));
    }

    #[test]
    fn local_folder_list_returns_immediate_directories_only() {
        let tree = TempRepo::folder_tree();
        let encoded = tree.path.to_string_lossy().replace('/', "%2F");
        let path = format!("/forge/local-folder-list.json?path={encoded}");

        let response = handle_local_folder_list("GET", &path).expect("list");
        let value: serde_json::Value = serde_json::from_str(&response.body).expect("json");

        assert_eq!(value["status"], "OK");
        assert_eq!(value["entries"].as_array().expect("entries").len(), 2);
        assert!(response.body.contains(r#""name":"git-repo","path":"#));
        assert!(response.body.contains(r#""is_git_repo":true"#));
        assert!(response.body.contains(r#""name":"plain-folder","path":"#));
        assert!(!response.body.contains("README.md"));
    }

    #[test]
    fn recorded_default_branch_rejects_unknown_provenance() {
        assert_eq!(
            default_branch_from_profile(
                r#"{"default_branch":"trunk","default_branch_source":"origin_head"}"#
            ),
            Some(RecordedDefaultBranch {
                name: "trunk".to_string(),
                source: "origin_head".to_string(),
            })
        );
        assert_eq!(
            default_branch_from_profile(
                r#"{"default_branch":"trunk","default_branch_source":"unverified"}"#
            ),
            None
        );
    }
}
