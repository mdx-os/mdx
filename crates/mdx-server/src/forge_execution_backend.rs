use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ForgeExecutionBackendKind {
    Local,
    HostedSandbox,
}

impl ForgeExecutionBackendKind {
    pub(crate) fn from_request(value: &str) -> Self {
        match value.trim() {
            "hosted" | "hosted_sandbox" | "cloud" | "aws_sandbox" => Self::HostedSandbox,
            _ => Self::Local,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::HostedSandbox => "hosted_sandbox",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ForgeExecutionBackend {
    kind: ForgeExecutionBackendKind,
}

impl ForgeExecutionBackend {
    pub(crate) fn new(kind: ForgeExecutionBackendKind) -> Self {
        Self { kind }
    }

    pub(crate) fn kind(&self) -> ForgeExecutionBackendKind {
        self.kind
    }

    pub(crate) fn prepare_workspace(
        &self,
        request: ExecutionWorkspaceRequest<'_>,
    ) -> Result<ExecutionWorkspace, String> {
        match self.kind {
            ForgeExecutionBackendKind::Local => prepare_local_workspace(request),
            ForgeExecutionBackendKind::HostedSandbox => {
                Err(hosted_sandbox_not_configured(&request).reason)
            }
        }
    }

    pub(crate) fn hosted_sandbox_not_configured(
        &self,
        request: ExecutionWorkspaceRequest<'_>,
    ) -> HostedSandboxUnavailable {
        let _ = self;
        hosted_sandbox_not_configured(&request)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ExecutionWorkspaceRequest<'a> {
    pub(crate) run_id: &'a str,
    pub(crate) eval_fixture: &'a str,
    pub(crate) workspace_kind: ExecutionWorkspaceKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExecutionWorkspaceKind {
    RepoSnapshot,
    HeldOutFixture,
}

#[derive(Clone, Debug)]
pub(crate) struct ExecutionWorkspace {
    pub(crate) workspace_root: PathBuf,
    pub(crate) execution_dir: PathBuf,
    pub(crate) artifact_dir: PathBuf,
}

#[derive(Clone, Debug)]
pub(crate) struct HostedSandboxUnavailable {
    pub(crate) backend_id: &'static str,
    pub(crate) backend_status: &'static str,
    pub(crate) reason: String,
    pub(crate) runtime_image_ref: String,
    pub(crate) credential_binding_ref: String,
    pub(crate) sandbox_session_ref: String,
}

fn prepare_local_workspace(
    request: ExecutionWorkspaceRequest<'_>,
) -> Result<ExecutionWorkspace, String> {
    let root = std::env::current_dir().map_err(|error| error.to_string())?;
    let base_dir = root.join(".mdx-local").join("machine-league");
    let workspace_root = base_dir.join("worktrees").join(request.run_id);
    let artifact_dir = base_dir.join("artifacts").join(request.run_id);
    reset_dir(&workspace_root)?;
    reset_dir(&artifact_dir)?;
    let execution_dir = match request.workspace_kind {
        ExecutionWorkspaceKind::RepoSnapshot => workspace_root.clone(),
        ExecutionWorkspaceKind::HeldOutFixture => workspace_root.join(request.eval_fixture),
    };
    Ok(ExecutionWorkspace {
        workspace_root,
        execution_dir,
        artifact_dir,
    })
}

fn hosted_sandbox_not_configured(
    request: &ExecutionWorkspaceRequest<'_>,
) -> HostedSandboxUnavailable {
    HostedSandboxUnavailable {
        backend_id: "hosted_sandbox",
        backend_status: "HOSTED_SANDBOX_BACKEND_NOT_CONFIGURED",
        reason: "hosted sandbox execution requires a configured sandbox provider, runtime image, and managed credential binding before task execution can start"
            .to_string(),
        runtime_image_ref: format!(
            "sandbox-image://mdx/machine-league/{}:pending",
            request.eval_fixture
        ),
        credential_binding_ref: "managed-secret-binding://mdx/machine-league/pending".to_string(),
        sandbox_session_ref: format!("sandbox-session://pending/{}", request.run_id),
    }
}

fn reset_dir(path: &Path) -> Result<(), String> {
    if path.exists() {
        std::fs::remove_dir_all(path).map_err(|error| error.to_string())?;
    }
    std::fs::create_dir_all(path).map_err(|error| error.to_string())
}
