//! Connected repositories: the repos an engineer points Forge at. By
//! default Forge works on MDx itself; this rail lets a run target any
//! repo the engineer has connected, so the same harness serves whatever
//! team or app they support.
//!
//! A connection is a governed record - which repo, where it lives, what
//! kind (a local clone today; a remote URL later). The kernel never
//! touches the filesystem: it records the connection's shape, and the
//! server validates the path is a real repo before recording. Runs then
//! resolve their workspace against the connected repo's root.

use crate::strategy_board::BoardWrite;
use crate::{ActionKind, GovernedWriteIdentity, MdxKernel, StorageProvider, payload};

/// How a connected repo is reached. Local is a clone already on disk;
/// remote (a clone URL with credentials) is the next layer.
pub const FORGE_REPO_KINDS: &[&str] = &["local", "remote"];

const MAX_FIELD_CHARS: usize = 500;
const MAX_PATH_CHARS: usize = 2000;

#[derive(Clone, Copy, Debug)]
pub struct ForgeRepoConnect<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    /// Stable, human-citable id (e.g. "mdx" or "billing-service").
    pub repo_id: &'a str,
    pub label: &'a str,
    /// The repo's root - the local filesystem path runs operate against.
    /// For a remote repo this is the managed clone on disk; the original
    /// URL travels in `origin`.
    pub root: &'a str,
    /// One of FORGE_REPO_KINDS; empty means local.
    pub kind: &'a str,
    /// The remote URL a remote repo was cloned from, so a shipped branch
    /// can be pushed back. Empty for a local repo.
    pub origin: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeRepoReport {
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub repo_id: String,
}

#[derive(Clone, Copy, Debug)]
pub struct ForgeRepoIndex<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub repo_id: &'a str,
    pub repo_receipt_id: &'a str,
    pub profile_json: &'a str,
    pub profile_fingerprint: &'a str,
    pub primary_language: &'a str,
    pub language_pack_id: &'a str,
    pub detected_language_packs: &'a str,
    pub semantic_tool_readiness: &'a str,
    pub toolchain_readiness: &'a str,
    pub proof_plan_status: &'a str,
    pub standards_source_summaries: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeRepoIndexReport {
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub repo_id: String,
    pub profile_fingerprint: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ForgeRepoError {
    Missing(&'static str),
    TooLong(&'static str, usize, usize),
    BadRepoId(String),
    UnknownKind(String),
}

impl ForgeRepoError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("connect a repo: missing {field}"),
            Self::TooLong(field, len, max) => {
                format!("repo {field} is {len} characters; the limit is {max}")
            }
            Self::BadRepoId(other) => {
                format!("repo id must be letters, numbers, dash, or underscore; got {other}")
            }
            Self::UnknownKind(other) => format!(
                "repo kind must be one of {}; got {other}",
                FORGE_REPO_KINDS.join(", ")
            ),
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    /// The connected repo's root, resolved from the latest connection for
    /// this id. None means "not connected" - the caller falls back to the
    /// default (MDx itself), so an absent or empty repo_id keeps the
    /// original self-build behavior.
    pub fn forge_repo_root(&self, repo_id: &str) -> Option<String> {
        let repo_id = repo_id.trim();
        if repo_id.is_empty() {
            return None;
        }
        let mut latest: Option<String> = None;
        for receipt in self.ledger().query().by_kind("forge.repo.connected").iter() {
            if receipt.payload.get("repo_id").map(String::as_str) == Some(repo_id) {
                latest = receipt.payload.get("root").cloned();
            }
        }
        latest.filter(|root| !root.is_empty())
    }

    /// The remote URL a connected repo was cloned from, if any. None means
    /// the repo is local (or unconnected) - there is nowhere to push back.
    pub fn forge_repo_origin(&self, repo_id: &str) -> Option<String> {
        let repo_id = repo_id.trim();
        if repo_id.is_empty() {
            return None;
        }
        let mut latest: Option<String> = None;
        for receipt in self.ledger().query().by_kind("forge.repo.connected").iter() {
            if receipt.payload.get("repo_id").map(String::as_str) == Some(repo_id) {
                latest = receipt.payload.get("origin_url").cloned();
            }
        }
        latest.filter(|origin| !origin.is_empty())
    }

    pub fn forge_repo_index_profile_json(&self, repo_id: &str) -> Option<String> {
        latest_repo_index_field(self, repo_id, "profile_json")
    }

    pub fn forge_repo_index_receipt_id(&self, repo_id: &str) -> Option<String> {
        let repo_id = repo_id.trim();
        if repo_id.is_empty() {
            return None;
        }
        let mut latest: Option<String> = None;
        for receipt in self.ledger().query().by_kind("forge.repo.indexed").iter() {
            if receipt.payload.get("repo_id").map(String::as_str) == Some(repo_id) {
                latest = Some(receipt.receipt_id.clone());
            }
        }
        latest
    }

    pub fn connect_forge_repo(
        &mut self,
        request: ForgeRepoConnect<'_>,
    ) -> Result<ForgeRepoReport, ForgeRepoError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.connect_forge_repo_with_identity(request, &identity)
    }

    pub fn connect_forge_repo_with_identity(
        &mut self,
        request: ForgeRepoConnect<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<ForgeRepoReport, ForgeRepoError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("repo_id", request.repo_id),
            ("label", request.label),
            ("root", request.root),
        ] {
            if value.trim().is_empty() {
                return Err(ForgeRepoError::Missing(field));
            }
        }
        let repo_id = request.repo_id.trim();
        if !repo_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(ForgeRepoError::BadRepoId(repo_id.to_string()));
        }
        if request.label.chars().count() > MAX_FIELD_CHARS {
            return Err(ForgeRepoError::TooLong(
                "label",
                request.label.chars().count(),
                MAX_FIELD_CHARS,
            ));
        }
        if request.root.chars().count() > MAX_PATH_CHARS {
            return Err(ForgeRepoError::TooLong(
                "root",
                request.root.chars().count(),
                MAX_PATH_CHARS,
            ));
        }
        if request.origin.chars().count() > MAX_PATH_CHARS {
            return Err(ForgeRepoError::TooLong(
                "origin",
                request.origin.chars().count(),
                MAX_PATH_CHARS,
            ));
        }
        let kind = if request.kind.trim().is_empty() {
            "local"
        } else {
            request.kind.trim()
        };
        if !FORGE_REPO_KINDS.contains(&kind) {
            return Err(ForgeRepoError::UnknownKind(kind.to_string()));
        }
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "forge_repo",
                action: ActionKind::ConnectForgeRepo,
                transition: "CONNECT_FORGE_REPO",
                kind: "forge.repo.connected",
            },
            payload(&[
                ("repo_id", repo_id),
                ("label", request.label),
                ("root", request.root),
                ("kind", kind),
                ("origin_url", request.origin),
                ("identity_source", &identity.identity_source),
                ("authority_opened", "none"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(ForgeRepoReport {
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            repo_id: repo_id.to_string(),
        })
    }

    pub fn index_forge_repo(
        &mut self,
        request: ForgeRepoIndex<'_>,
    ) -> Result<ForgeRepoIndexReport, ForgeRepoError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.index_forge_repo_with_identity(request, &identity)
    }

    pub fn index_forge_repo_with_identity(
        &mut self,
        request: ForgeRepoIndex<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<ForgeRepoIndexReport, ForgeRepoError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("repo_id", request.repo_id),
            ("repo_receipt_id", request.repo_receipt_id),
            ("profile_json", request.profile_json),
            ("profile_fingerprint", request.profile_fingerprint),
            ("primary_language", request.primary_language),
            ("language_pack_id", request.language_pack_id),
            ("proof_plan_status", request.proof_plan_status),
        ] {
            if value.trim().is_empty() {
                return Err(ForgeRepoError::Missing(field));
            }
        }
        let repo_id = request.repo_id.trim();
        if !repo_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(ForgeRepoError::BadRepoId(repo_id.to_string()));
        }
        if request.profile_json.chars().count() > 50_000 {
            return Err(ForgeRepoError::TooLong(
                "profile_json",
                request.profile_json.chars().count(),
                50_000,
            ));
        }
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "forge_repo",
                action: ActionKind::IndexForgeRepo,
                transition: "INDEX_FORGE_REPO",
                kind: "forge.repo.indexed",
            },
            payload(&[
                ("repo_id", repo_id),
                ("repo_receipt_id", request.repo_receipt_id),
                ("profile_json", request.profile_json),
                ("profile_fingerprint", request.profile_fingerprint),
                ("primary_language", request.primary_language),
                ("language_pack_id", request.language_pack_id),
                ("detected_language_packs", request.detected_language_packs),
                ("semantic_tool_readiness", request.semantic_tool_readiness),
                ("toolchain_readiness", request.toolchain_readiness),
                ("proof_plan_status", request.proof_plan_status),
                (
                    "standards_source_summaries",
                    request.standards_source_summaries,
                ),
                ("identity_source", &identity.identity_source),
                ("authority_opened", "none"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(ForgeRepoIndexReport {
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            repo_id: repo_id.to_string(),
            profile_fingerprint: request.profile_fingerprint.to_string(),
        })
    }
}

fn latest_repo_index_field<S: StorageProvider>(
    kernel: &MdxKernel<S>,
    repo_id: &str,
    field: &str,
) -> Option<String> {
    let repo_id = repo_id.trim();
    if repo_id.is_empty() {
        return None;
    }
    let mut latest: Option<String> = None;
    for receipt in kernel.ledger().query().by_kind("forge.repo.indexed").iter() {
        if receipt.payload.get("repo_id").map(String::as_str) == Some(repo_id) {
            latest = receipt.payload.get(field).cloned();
        }
    }
    latest.filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_connected_repo_resolves_its_root_and_the_latest_connection_wins() {
        let mut kernel = MdxKernel::boot_local();
        // Unconnected: no root (the caller falls back to MDx itself).
        assert_eq!(kernel.forge_repo_root("billing"), None);
        assert_eq!(kernel.forge_repo_root(""), None);

        kernel
            .connect_forge_repo(ForgeRepoConnect {
                tenant_id: "t",
                actor_id: "human:eng",
                repo_id: "billing",
                label: "Billing service",
                root: "/work/billing",
                kind: "local",
                origin: "",
            })
            .expect("connect");
        assert_eq!(
            kernel.forge_repo_root("billing").as_deref(),
            Some("/work/billing")
        );
        // A local repo has no origin to push back to.
        assert_eq!(kernel.forge_repo_origin("billing"), None);

        // Re-connecting the same id updates the root - the latest wins.
        kernel
            .connect_forge_repo(ForgeRepoConnect {
                tenant_id: "t",
                actor_id: "human:eng",
                repo_id: "billing",
                label: "Billing service",
                root: "/work/billing-v2",
                kind: "local",
                origin: "",
            })
            .expect("reconnect");
        assert_eq!(
            kernel.forge_repo_root("billing").as_deref(),
            Some("/work/billing-v2")
        );

        // A bad repo id is refused.
        let bad = kernel.connect_forge_repo(ForgeRepoConnect {
            tenant_id: "t",
            actor_id: "human:eng",
            repo_id: "has spaces",
            label: "x",
            root: "/x",
            kind: "local",
            origin: "",
        });
        assert!(matches!(bad, Err(ForgeRepoError::BadRepoId(_))));
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn a_remote_repo_records_its_origin_so_a_branch_can_be_pushed_back() {
        let mut kernel = MdxKernel::boot_local();
        kernel
            .connect_forge_repo(ForgeRepoConnect {
                tenant_id: "t",
                actor_id: "human:eng",
                repo_id: "acme-api",
                label: "Acme API",
                // Runs operate against the managed clone on disk...
                root: ".mdx-local/forge-repos/acme-api",
                kind: "remote",
                // ...while the origin URL is remembered for push-back.
                origin: "https://github.com/acme/api.git",
            })
            .expect("connect remote");
        assert_eq!(
            kernel.forge_repo_root("acme-api").as_deref(),
            Some(".mdx-local/forge-repos/acme-api")
        );
        assert_eq!(
            kernel.forge_repo_origin("acme-api").as_deref(),
            Some("https://github.com/acme/api.git")
        );
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn a_repo_index_records_profile_json_and_the_latest_index_wins() {
        let mut kernel = MdxKernel::boot_local();
        kernel
            .connect_forge_repo(ForgeRepoConnect {
                tenant_id: "t",
                actor_id: "human:eng",
                repo_id: "billing",
                label: "Billing service",
                root: "/work/billing",
                kind: "local",
                origin: "",
            })
            .expect("connect");

        let first = kernel
            .index_forge_repo(ForgeRepoIndex {
                tenant_id: "t",
                actor_id: "human:eng",
                repo_id: "billing",
                repo_receipt_id: "receipt_1",
                profile_json: r#"{"language_pack_id":"java-maven"}"#,
                profile_fingerprint: "fnv1a64:1111111111111111",
                primary_language: "java",
                language_pack_id: "java-maven",
                detected_language_packs: "java-maven",
                semantic_tool_readiness: "lsp:jdtls=missing",
                toolchain_readiness: "tool:mvn=missing",
                proof_plan_status: "setup_required",
                standards_source_summaries: "AGENTS.md=agent_instructions",
            })
            .expect("index");
        assert_eq!(
            kernel.forge_repo_index_profile_json("billing").as_deref(),
            Some(r#"{"language_pack_id":"java-maven"}"#)
        );
        assert_eq!(
            kernel.forge_repo_index_receipt_id("billing").as_deref(),
            Some(first.receipt_id.as_str())
        );

        let second = kernel
            .index_forge_repo(ForgeRepoIndex {
                tenant_id: "t",
                actor_id: "human:eng",
                repo_id: "billing",
                repo_receipt_id: "receipt_2",
                profile_json: r#"{"language_pack_id":"gradle-jvm"}"#,
                profile_fingerprint: "fnv1a64:2222222222222222",
                primary_language: "java",
                language_pack_id: "gradle-jvm",
                detected_language_packs: "gradle-jvm",
                semantic_tool_readiness: "lsp:kotlin-language-server=missing",
                toolchain_readiness: "repo_tool:gradlew=missing",
                proof_plan_status: "setup_required",
                standards_source_summaries: "CODEOWNERS=ownership_review",
            })
            .expect("index again");
        assert_eq!(
            kernel.forge_repo_index_profile_json("billing").as_deref(),
            Some(r#"{"language_pack_id":"gradle-jvm"}"#)
        );
        assert_eq!(
            kernel.forge_repo_index_receipt_id("billing").as_deref(),
            Some(second.receipt_id.as_str())
        );
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn connect_forge_repo_refuses_missing_required_fields() {
        let mut kernel = MdxKernel::boot_local();
        let base = ForgeRepoConnect {
            tenant_id: "t",
            actor_id: "human:eng",
            repo_id: "ok",
            label: "ok",
            root: "/ok",
            kind: "local",
            origin: "",
        };

        let missing_tenant = kernel.connect_forge_repo(ForgeRepoConnect {
            tenant_id: "   ",
            ..base
        });
        assert!(matches!(
            missing_tenant,
            Err(ForgeRepoError::Missing("tenant_id"))
        ));

        let missing_actor = kernel.connect_forge_repo(ForgeRepoConnect {
            actor_id: "",
            ..base
        });
        assert!(matches!(
            missing_actor,
            Err(ForgeRepoError::Missing("actor_id"))
        ));

        let missing_repo = kernel.connect_forge_repo(ForgeRepoConnect {
            repo_id: " ",
            ..base
        });
        assert!(matches!(
            missing_repo,
            Err(ForgeRepoError::Missing("repo_id"))
        ));

        let missing_label = kernel.connect_forge_repo(ForgeRepoConnect {
            label: "\t",
            ..base
        });
        assert!(matches!(
            missing_label,
            Err(ForgeRepoError::Missing("label"))
        ));

        let missing_root = kernel.connect_forge_repo(ForgeRepoConnect { root: "", ..base });
        assert!(matches!(missing_root, Err(ForgeRepoError::Missing("root"))));

        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn connect_forge_repo_refuses_too_long_fields() {
        let mut kernel = MdxKernel::boot_local();

        let long_label: String = "x".repeat(MAX_FIELD_CHARS + 1);
        let bad_label = kernel.connect_forge_repo(ForgeRepoConnect {
            tenant_id: "t",
            actor_id: "human:eng",
            repo_id: "r",
            label: &long_label,
            root: "/r",
            kind: "local",
            origin: "",
        });
        assert!(matches!(
            bad_label,
            Err(ForgeRepoError::TooLong("label", _, MAX_FIELD_CHARS))
        ));

        let long_root: String = "y".repeat(MAX_PATH_CHARS + 1);
        let bad_root = kernel.connect_forge_repo(ForgeRepoConnect {
            tenant_id: "t",
            actor_id: "human:eng",
            repo_id: "r",
            label: "r",
            root: &long_root,
            kind: "local",
            origin: "",
        });
        assert!(matches!(
            bad_root,
            Err(ForgeRepoError::TooLong("root", _, MAX_PATH_CHARS))
        ));

        let long_origin: String = "z".repeat(MAX_PATH_CHARS + 1);
        let bad_origin = kernel.connect_forge_repo(ForgeRepoConnect {
            tenant_id: "t",
            actor_id: "human:eng",
            repo_id: "r",
            label: "r",
            root: "/r",
            kind: "local",
            origin: &long_origin,
        });
        assert!(matches!(
            bad_origin,
            Err(ForgeRepoError::TooLong("origin", _, MAX_PATH_CHARS))
        ));

        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn connect_forge_repo_refuses_unknown_kind() {
        let mut kernel = MdxKernel::boot_local();
        let bad = kernel.connect_forge_repo(ForgeRepoConnect {
            tenant_id: "t",
            actor_id: "human:eng",
            repo_id: "r",
            label: "r",
            root: "/r",
            kind: "git",
            origin: "",
        });
        assert!(matches!(bad, Err(ForgeRepoError::UnknownKind(_))));
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn connect_forge_repo_with_identity_happy_path_records_connection() {
        let mut kernel = MdxKernel::boot_local();
        let identity = GovernedWriteIdentity::local_demo("human:eng");
        let report = kernel
            .connect_forge_repo_with_identity(
                ForgeRepoConnect {
                    tenant_id: "t",
                    actor_id: "human:eng",
                    repo_id: "with-id",
                    label: "With Identity",
                    root: "/work/with-id",
                    kind: "",
                    origin: "",
                },
                &identity,
            )
            .expect("connect with identity");
        assert_eq!(report.repo_id, "with-id");
        assert_eq!(
            kernel.forge_repo_root("with-id").as_deref(),
            Some("/work/with-id")
        );
        assert_eq!(kernel.forge_repo_origin("with-id"), None);
        assert!(kernel.ledger().verify().is_ok());
    }
}
