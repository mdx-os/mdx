//! Production leakage matrix.
//!
//! This is the permanent "no accidental leakage" gate for the production access
//! model. It enumerates, for every sensitive surface (Twin, Message, Pages,
//! Forge, Talent, Product, Strategy, Observatory, Charter, receipts, and admin),
//! the principal relationships that may and may not reach a resource, and proves
//! both the application authorization decision (the API path) and the Postgres
//! row policy (the direct-DB path) agree with the declared verdict.
//!
//! Two honesty rules keep this matrix from drifting into theater:
//!
//! 1. The declared verdict in [`LEAKAGE_MATRIX`] must equal what the independent
//!    rule engine [`evaluate`] computes for the same cell. A change to either the
//!    enumerated cases or the rules that turns a `Deny` into an `Allow` breaks the
//!    test, so a leak cannot be introduced quietly.
//!
//! 2. A cell may only claim a resource-aware database backstop
//!    ([`DbBackstop::ResourceAware`]) when the surface's private table actually
//!    carries a resource-aware RLS policy (RLS v2 migration 0027; RLS v3
//!    migrations 0028 Pages, 0029 receipts, 0030 authority surfaces). Surfaces
//!    whose Postgres policy is still tenant-only are declared honestly as
//!    [`DbBackstop::TenantOnly`]: their cross-tenant isolation is enforced at the
//!    database, but their same-tenant isolation rests on the application decision.
//!    Those surfaces are named in [`PENDING_RESOURCE_AWARE_RLS_SURFACES`] rather
//!    than hidden; Charter remains there deliberately, as tenant-visible
//!    governance content, not as a gap.
//!
//! Nothing here flips a live substrate to "ready". The matrix proves the access
//! decisions and policies are correct as written; wiring a verified trusted
//! session and the per-request RLS context into every live route rides on the
//! auth boundary (Phase 1/5). Until then governed paths fail closed.

/// A product surface that holds tenant or user data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    Twin,
    Message,
    Pages,
    Forge,
    Talent,
    Product,
    Strategy,
    Observatory,
    Charter,
    Receipt,
    Admin,
}

impl Surface {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Twin => "twin",
            Self::Message => "message",
            Self::Pages => "pages",
            Self::Forge => "forge",
            Self::Talent => "talent",
            Self::Product => "product",
            Self::Strategy => "strategy",
            Self::Observatory => "observatory",
            Self::Charter => "charter",
            Self::Receipt => "receipt",
            Self::Admin => "admin",
        }
    }

    /// The canonical private Postgres table that backs this surface. The leakage
    /// test cross-references this against the real RLS table set and the RLS v2
    /// resource-aware overrides so a matrix cell cannot claim a database backstop
    /// the migrations do not provide.
    pub fn private_table(&self) -> &'static str {
        match self {
            Self::Twin => "twin_sessions",
            Self::Message => "message_threads",
            Self::Pages => "pages_documents",
            Self::Forge => "forge_build_requests",
            Self::Talent => "talent_worker_lease_authorities",
            Self::Product => "product_bet_drafts",
            Self::Strategy => "strategy_ratification_snapshots",
            Self::Observatory => "observatory_role_view_snapshots",
            Self::Charter => "charter_records",
            Self::Receipt => "ledger_entries",
            Self::Admin => "auth_tenant_memberships",
        }
    }
}

/// The relationship a requesting principal has to the target resource. This is
/// the axis access decisions turn on: tenant membership alone is never one of
/// these, because membership is not access.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Principal {
    /// The human who owns the resource.
    ResourceOwner,
    /// An explicit participant of a Message channel or thread.
    ThreadParticipant,
    /// An explicit collaborator on a Forge build.
    BuildCollaborator,
    /// A human in the same tenant with no relationship to the resource.
    SameTenantOutsider,
    /// A principal in a different tenant.
    CrossTenantActor,
    /// An agent holding a valid, unexpired delegation lease for this resource.
    DelegatedAgentInScope,
    /// An agent whose lease does not cover this resource.
    DelegatedAgentOutOfScope,
    /// A service identity with an explicit scope for this surface.
    ServiceActorScoped,
    /// A service identity with no scope: the request-path service-role case.
    ServiceActorUnscoped,
    /// A control-plane delegated agent, configured and in scope.
    ControlPlaneDelegatedInScope,
    /// A tenant admin acting on governance, not private user content.
    TenantAdminGovernance,
    /// A tenant admin attempting to read private user content.
    TenantAdminPrivateContent,
    /// A security operator reading audit metadata, not resource content.
    SecurityOperatorAuditMetadata,
}

impl Principal {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ResourceOwner => "resource_owner",
            Self::ThreadParticipant => "thread_participant",
            Self::BuildCollaborator => "build_collaborator",
            Self::SameTenantOutsider => "same_tenant_outsider",
            Self::CrossTenantActor => "cross_tenant_actor",
            Self::DelegatedAgentInScope => "delegated_agent_in_scope",
            Self::DelegatedAgentOutOfScope => "delegated_agent_out_of_scope",
            Self::ServiceActorScoped => "service_actor_scoped",
            Self::ServiceActorUnscoped => "service_actor_unscoped",
            Self::ControlPlaneDelegatedInScope => "control_plane_delegated_in_scope",
            Self::TenantAdminGovernance => "tenant_admin_governance",
            Self::TenantAdminPrivateContent => "tenant_admin_private_content",
            Self::SecurityOperatorAuditMetadata => "security_operator_audit_metadata",
        }
    }
}

/// The verdict of an access decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Deny,
}

/// How far the database row policy backs an application `Deny`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DbBackstop {
    /// A resource-aware RLS v2 policy denies this at the database even if the
    /// application layer is bypassed. Same-tenant and cross-tenant both covered.
    ResourceAware,
    /// Only tenant isolation at the database. Cross-tenant is denied at the row
    /// level; same-tenant denial currently rests on the application decision
    /// until resource-aware RLS is extended to this surface.
    TenantOnly,
}

/// One declared cell of the leakage matrix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatrixCell {
    pub surface: Surface,
    pub principal: Principal,
    /// The leakage class this case belongs to (see [`LEAKAGE_CLASSES`]).
    pub leakage_class: &'static str,
    /// The application authorization decision (the API path).
    pub api: Decision,
    /// The database row-policy backstop behind the decision (the direct-DB path).
    pub db: DbBackstop,
    /// Plain-language reason, for the audit artifact and human review.
    pub note: &'static str,
}

/// The leakage classes the matrix must cover. The check fails if any class is
/// absent from [`LEAKAGE_MATRIX`].
pub const LEAKAGE_CLASSES: &[&str] = &[
    "same_tenant_unauthorized",
    "cross_tenant",
    "delegated_agent_out_of_scope",
    "service_or_control_plane_unscoped",
    "admin_or_audit_visibility",
];

/// The deny vocabulary the contract doc and the check script keep in lockstep
/// with this module. Each is a denial the matrix proves holds.
pub const ACCESS_DENY_CASES: &[&str] = &[
    "deny_same_tenant_outsider",
    "deny_cross_tenant",
    "deny_delegated_agent_out_of_scope",
    "deny_service_actor_unscoped",
    "deny_tenant_admin_private_content",
    "deny_security_operator_content",
];

/// Surfaces whose same-tenant isolation is not yet backed by a resource-aware
/// RLS policy. Their cross-tenant isolation is enforced at the database; their
/// same-tenant isolation rests on the application decision until RLS v3 extends
/// resource-aware policies to them. Named here so the gap is tracked, not hidden.
pub const PENDING_RESOURCE_AWARE_RLS_SURFACES: &[&str] = &[
    // Charter is deliberately deferred, not a gap: charter_records is tenant-
    // visible governance content by design, and per-section restriction is a
    // future product feature, not a same-tenant leak. See docs/SECURITY-RLS-V3.md.
    "charter",
];

/// Independent rule engine for the access decision. Authored from rules (by
/// principal relationship and surface) rather than enumerated cases, so it is a
/// genuine cross-check against [`LEAKAGE_MATRIX`], which is authored from cases.
pub fn evaluate(surface: Surface, principal: Principal) -> Decision {
    use Decision::{Allow, Deny};
    use Principal::*;
    match principal {
        // The owner of a resource reaches their own resource.
        ResourceOwner => Allow,
        // Participation and collaboration are surface-specific memberships.
        ThreadParticipant => {
            if matches!(surface, Surface::Message) {
                Allow
            } else {
                Deny
            }
        }
        BuildCollaborator => {
            if matches!(surface, Surface::Forge) {
                Allow
            } else {
                Deny
            }
        }
        // Tenant membership is not access. Cross-tenant is never access.
        SameTenantOutsider => Deny,
        CrossTenantActor => Deny,
        // An in-scope lease for this exact resource is access; anything outside
        // the lease is not.
        DelegatedAgentInScope => Allow,
        DelegatedAgentOutOfScope => Deny,
        // Service identity reaches only its explicit scope; an unscoped service
        // identity in the request path is the service-role bypass we refuse.
        ServiceActorScoped => Allow,
        ServiceActorUnscoped => Deny,
        // A configured, in-scope control-plane delegation is access.
        ControlPlaneDelegatedInScope => Allow,
        // Admin override is explicit and narrow: governance surfaces, never
        // private user content.
        TenantAdminGovernance => {
            if matches!(
                surface,
                Surface::Admin | Surface::Charter | Surface::Observatory
            ) {
                Allow
            } else {
                Deny
            }
        }
        TenantAdminPrivateContent => Deny,
        // A security operator sees audit metadata on the Observatory, never the
        // content of a private resource or a receipt's source resource.
        SecurityOperatorAuditMetadata => {
            if matches!(surface, Surface::Observatory) {
                Allow
            } else {
                Deny
            }
        }
    }
}

/// True when the surface's private table carries a resource-aware RLS v2 policy
/// today (Twin, Message, Forge from RLS v2; Pages from RLS v3). Used to assign
/// and verify [`DbBackstop`].
fn surface_is_resource_aware(surface: Surface) -> bool {
    matches!(
        surface,
        Surface::Twin
            | Surface::Message
            | Surface::Forge
            | Surface::Pages
            | Surface::Receipt
            | Surface::Talent
            | Surface::Product
            | Surface::Strategy
            | Surface::Observatory
            | Surface::Admin
    )
}

#[rustfmt::skip]
const fn cell(surface: Surface, principal: Principal, leakage_class: &'static str, api: Decision, db: DbBackstop, note: &'static str) -> MatrixCell {
    MatrixCell { surface, principal, leakage_class, api, db, note }
}

/// The permanent leakage matrix. Every sensitive surface declares its allowed
/// control case and the five leakage classes. `ResourceAware` is claimed only
/// for Twin, Message, and Forge (migration 0027), Pages (migration 0028),
/// receipts (migration 0029), and the authority surfaces (migration 0030).
#[rustfmt::skip]
pub const LEAKAGE_MATRIX: &[MatrixCell] = &[
    // Twin: user-private. Tenant admins and agents get no blanket content read.
    cell(Surface::Twin, Principal::ResourceOwner, "owner_control", Decision::Allow, DbBackstop::ResourceAware, "owner reads own Twin session"),
    cell(Surface::Twin, Principal::SameTenantOutsider, "same_tenant_unauthorized", Decision::Deny, DbBackstop::ResourceAware, "same-tenant user cannot read another user's Twin"),
    cell(Surface::Twin, Principal::CrossTenantActor, "cross_tenant", Decision::Deny, DbBackstop::ResourceAware, "cross-tenant actor cannot read Twin"),
    cell(Surface::Twin, Principal::DelegatedAgentOutOfScope, "delegated_agent_out_of_scope", Decision::Deny, DbBackstop::ResourceAware, "agent without a Twin lease cannot read Twin"),
    cell(Surface::Twin, Principal::ServiceActorUnscoped, "service_or_control_plane_unscoped", Decision::Deny, DbBackstop::ResourceAware, "request-path service role cannot read Twin"),
    cell(Surface::Twin, Principal::TenantAdminPrivateContent, "admin_or_audit_visibility", Decision::Deny, DbBackstop::ResourceAware, "tenant admin cannot read private Twin content"),

    // Message: participant-scoped.
    cell(Surface::Message, Principal::ThreadParticipant, "owner_control", Decision::Allow, DbBackstop::ResourceAware, "thread participant reads the thread"),
    cell(Surface::Message, Principal::SameTenantOutsider, "same_tenant_unauthorized", Decision::Deny, DbBackstop::ResourceAware, "same-tenant non-participant cannot read a private thread"),
    cell(Surface::Message, Principal::CrossTenantActor, "cross_tenant", Decision::Deny, DbBackstop::ResourceAware, "cross-tenant actor cannot read a thread"),
    cell(Surface::Message, Principal::DelegatedAgentOutOfScope, "delegated_agent_out_of_scope", Decision::Deny, DbBackstop::ResourceAware, "agent cannot subscribe outside delegated thread scope"),
    cell(Surface::Message, Principal::ServiceActorUnscoped, "service_or_control_plane_unscoped", Decision::Deny, DbBackstop::ResourceAware, "request-path service role cannot read a thread"),
    cell(Surface::Message, Principal::TenantAdminPrivateContent, "admin_or_audit_visibility", Decision::Deny, DbBackstop::ResourceAware, "tenant admin cannot read a private thread"),

    // Forge: owner-scoped today; collaborator sharing is a later contract.
    cell(Surface::Forge, Principal::ResourceOwner, "owner_control", Decision::Allow, DbBackstop::ResourceAware, "owner reads own build request"),
    cell(Surface::Forge, Principal::DelegatedAgentInScope, "owner_control", Decision::Allow, DbBackstop::ResourceAware, "agent with a build lease reads the named build"),
    cell(Surface::Forge, Principal::SameTenantOutsider, "same_tenant_unauthorized", Decision::Deny, DbBackstop::ResourceAware, "same-tenant user cannot read an unshared build"),
    cell(Surface::Forge, Principal::CrossTenantActor, "cross_tenant", Decision::Deny, DbBackstop::ResourceAware, "cross-tenant actor cannot read a build"),
    cell(Surface::Forge, Principal::DelegatedAgentOutOfScope, "delegated_agent_out_of_scope", Decision::Deny, DbBackstop::ResourceAware, "agent cannot mutate a build outside lease scope"),
    cell(Surface::Forge, Principal::ServiceActorUnscoped, "service_or_control_plane_unscoped", Decision::Deny, DbBackstop::ResourceAware, "service actor cannot read arbitrary build content"),

    // Pages: private by default; tenant-only RLS until resource-aware RLS lands.
    cell(Surface::Pages, Principal::ResourceOwner, "owner_control", Decision::Allow, DbBackstop::ResourceAware, "owner reads own page"),
    cell(Surface::Pages, Principal::SameTenantOutsider, "same_tenant_unauthorized", Decision::Deny, DbBackstop::ResourceAware, "same-tenant user cannot read a private page"),
    cell(Surface::Pages, Principal::CrossTenantActor, "cross_tenant", Decision::Deny, DbBackstop::ResourceAware, "cross-tenant actor cannot read a page"),
    cell(Surface::Pages, Principal::DelegatedAgentOutOfScope, "delegated_agent_out_of_scope", Decision::Deny, DbBackstop::ResourceAware, "agent cannot read pages outside delegated audience"),
    cell(Surface::Pages, Principal::ServiceActorUnscoped, "service_or_control_plane_unscoped", Decision::Deny, DbBackstop::ResourceAware, "request-path service role cannot read a page"),

    // Talent: high-authority worker and credential resources.
    cell(Surface::Talent, Principal::ResourceOwner, "owner_control", Decision::Allow, DbBackstop::ResourceAware, "sponsor reads own worker lease"),
    cell(Surface::Talent, Principal::SameTenantOutsider, "same_tenant_unauthorized", Decision::Deny, DbBackstop::ResourceAware, "same-tenant user cannot read another sponsor's worker"),
    cell(Surface::Talent, Principal::CrossTenantActor, "cross_tenant", Decision::Deny, DbBackstop::ResourceAware, "cross-tenant actor cannot read worker authority"),
    cell(Surface::Talent, Principal::DelegatedAgentOutOfScope, "delegated_agent_out_of_scope", Decision::Deny, DbBackstop::ResourceAware, "worker cannot exceed its lease scope"),
    cell(Surface::Talent, Principal::ServiceActorUnscoped, "service_or_control_plane_unscoped", Decision::Deny, DbBackstop::ResourceAware, "service actor cannot read credential authority"),

    // Product: org-scoped by decision state.
    cell(Surface::Product, Principal::ResourceOwner, "owner_control", Decision::Allow, DbBackstop::ResourceAware, "product owner reads own bet draft"),
    cell(Surface::Product, Principal::SameTenantOutsider, "same_tenant_unauthorized", Decision::Deny, DbBackstop::ResourceAware, "tenant member cannot read another team's draft by default"),
    cell(Surface::Product, Principal::CrossTenantActor, "cross_tenant", Decision::Deny, DbBackstop::ResourceAware, "cross-tenant actor cannot read product bets"),
    cell(Surface::Product, Principal::DelegatedAgentOutOfScope, "delegated_agent_out_of_scope", Decision::Deny, DbBackstop::ResourceAware, "agent cannot move product state without delegation"),
    cell(Surface::Product, Principal::ServiceActorUnscoped, "service_or_control_plane_unscoped", Decision::Deny, DbBackstop::ResourceAware, "service actor cannot read product drafts"),

    // Strategy: org or leadership scoped until ratified.
    cell(Surface::Strategy, Principal::ResourceOwner, "owner_control", Decision::Allow, DbBackstop::ResourceAware, "strategy owner reads own snapshot"),
    cell(Surface::Strategy, Principal::SameTenantOutsider, "same_tenant_unauthorized", Decision::Deny, DbBackstop::ResourceAware, "tenant member cannot read an unratified strategy draft"),
    cell(Surface::Strategy, Principal::CrossTenantActor, "cross_tenant", Decision::Deny, DbBackstop::ResourceAware, "cross-tenant actor cannot read strategy decisions"),
    cell(Surface::Strategy, Principal::DelegatedAgentOutOfScope, "delegated_agent_out_of_scope", Decision::Deny, DbBackstop::ResourceAware, "agent cannot move strategy state without delegation"),
    cell(Surface::Strategy, Principal::ServiceActorUnscoped, "service_or_control_plane_unscoped", Decision::Deny, DbBackstop::ResourceAware, "service actor cannot read strategy drafts"),

    // Observatory: role and receipt scoped.
    cell(Surface::Observatory, Principal::SecurityOperatorAuditMetadata, "admin_or_audit_visibility", Decision::Allow, DbBackstop::ResourceAware, "security operator reads audit metadata on the Observatory"),
    cell(Surface::Observatory, Principal::TenantAdminGovernance, "admin_or_audit_visibility", Decision::Allow, DbBackstop::ResourceAware, "tenant admin reads operational posture"),
    cell(Surface::Observatory, Principal::SameTenantOutsider, "same_tenant_unauthorized", Decision::Deny, DbBackstop::ResourceAware, "non-operator tenant member cannot read the Observatory"),
    cell(Surface::Observatory, Principal::CrossTenantActor, "cross_tenant", Decision::Deny, DbBackstop::ResourceAware, "cross-tenant actor cannot read the Observatory"),
    cell(Surface::Observatory, Principal::ServiceActorUnscoped, "service_or_control_plane_unscoped", Decision::Deny, DbBackstop::ResourceAware, "service actor cannot read role-view snapshots"),

    // Charter: tenant-visible stable governance, restricted sections excepted.
    cell(Surface::Charter, Principal::TenantAdminGovernance, "admin_or_audit_visibility", Decision::Allow, DbBackstop::TenantOnly, "governance admin reads and writes charter"),
    cell(Surface::Charter, Principal::SameTenantOutsider, "same_tenant_unauthorized", Decision::Deny, DbBackstop::TenantOnly, "a restricted charter section stays restricted from a tenant outsider"),
    cell(Surface::Charter, Principal::CrossTenantActor, "cross_tenant", Decision::Deny, DbBackstop::TenantOnly, "cross-tenant actor cannot read charter"),
    cell(Surface::Charter, Principal::DelegatedAgentOutOfScope, "delegated_agent_out_of_scope", Decision::Deny, DbBackstop::TenantOnly, "agent cannot read a restricted charter section without delegation"),

    // Receipts: inherit the source resource, never global by tenant membership.
    cell(Surface::Receipt, Principal::SameTenantOutsider, "same_tenant_unauthorized", Decision::Deny, DbBackstop::ResourceAware, "receipt read inherits source; same tenant is not access"),
    cell(Surface::Receipt, Principal::CrossTenantActor, "cross_tenant", Decision::Deny, DbBackstop::ResourceAware, "cross-tenant actor cannot read receipts"),
    cell(Surface::Receipt, Principal::SecurityOperatorAuditMetadata, "admin_or_audit_visibility", Decision::Deny, DbBackstop::ResourceAware, "audit metadata does not expose a receipt's private source content"),
    cell(Surface::Receipt, Principal::ServiceActorUnscoped, "service_or_control_plane_unscoped", Decision::Deny, DbBackstop::ResourceAware, "service actor cannot read receipts by tenant alone"),

    // Admin: governance surface, never a window into private user content.
    cell(Surface::Admin, Principal::TenantAdminGovernance, "owner_control", Decision::Allow, DbBackstop::ResourceAware, "tenant admin manages users, teams, and config"),
    cell(Surface::Admin, Principal::TenantAdminPrivateContent, "admin_or_audit_visibility", Decision::Deny, DbBackstop::ResourceAware, "tenant admin cannot read private user content via admin"),
    cell(Surface::Admin, Principal::CrossTenantActor, "cross_tenant", Decision::Deny, DbBackstop::ResourceAware, "admin has no cross-tenant visibility"),
    cell(Surface::Admin, Principal::ServiceActorUnscoped, "service_or_control_plane_unscoped", Decision::Deny, DbBackstop::ResourceAware, "service actor cannot mint admin data access in the request path"),
];

/// A computed leakage report. `production_write_allowed` stays false: proving the
/// matrix never makes a live substrate ready.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeakageMatrixReport {
    pub cells: usize,
    pub allow_cells: usize,
    pub deny_cells: usize,
    pub resource_aware_cells: usize,
    pub tenant_only_cells: usize,
    pub leakage_classes: usize,
    pub production_write_allowed: bool,
}

/// Validate the matrix and return its report. Fails closed with a message when a
/// declared cell disagrees with the rule engine or claims a database backstop the
/// migrations do not provide.
pub fn validate_leakage_matrix(
    resource_aware_tables: &[&str],
    rls_tables: &[&str],
) -> Result<LeakageMatrixReport, String> {
    let mut allow_cells = 0usize;
    let mut deny_cells = 0usize;
    let mut resource_aware_cells = 0usize;
    let mut tenant_only_cells = 0usize;

    for cell in LEAKAGE_MATRIX {
        // Rule engine and declared verdict must agree.
        let computed = evaluate(cell.surface, cell.principal);
        if computed != cell.api {
            return Err(format!(
                "{}::{} declares {:?} but the rule engine computes {:?}",
                cell.surface.as_str(),
                cell.principal.as_str(),
                cell.api,
                computed
            ));
        }

        // Every surface in the matrix must be a real RLS-protected table, so the
        // cross-tenant backstop genuinely exists.
        let table = cell.surface.private_table();
        if !rls_tables.contains(&table) {
            return Err(format!(
                "{} private table {} is not an RLS-protected table",
                cell.surface.as_str(),
                table
            ));
        }

        match cell.db {
            DbBackstop::ResourceAware => {
                // A resource-aware claim must be backed by the RLS v2 override
                // set, and only the resource-aware surfaces may claim it.
                if !resource_aware_tables.contains(&table) {
                    return Err(format!(
                        "{}::{} claims a resource-aware DB backstop but {} has no RLS v2 policy",
                        cell.surface.as_str(),
                        cell.principal.as_str(),
                        table
                    ));
                }
                if !surface_is_resource_aware(cell.surface) {
                    return Err(format!(
                        "{} is not in the resource-aware surface set",
                        cell.surface.as_str()
                    ));
                }
                resource_aware_cells += 1;
            }
            DbBackstop::TenantOnly => {
                // A tenant-only surface must NOT silently sit in the override set
                // (that would mean we under-claimed a real backstop), and it must
                // be named in the pending list so the gap is tracked.
                if resource_aware_tables.contains(&table) {
                    return Err(format!(
                        "{} has an RLS v2 policy but the matrix only claims tenant-only",
                        cell.surface.as_str()
                    ));
                }
                if !PENDING_RESOURCE_AWARE_RLS_SURFACES.contains(&cell.surface.as_str()) {
                    return Err(format!(
                        "{} is tenant-only at the DB but is not tracked in PENDING_RESOURCE_AWARE_RLS_SURFACES",
                        cell.surface.as_str()
                    ));
                }
                tenant_only_cells += 1;
            }
        }

        match cell.api {
            Decision::Allow => allow_cells += 1,
            Decision::Deny => deny_cells += 1,
        }
    }

    // Every leakage class must appear.
    for class in LEAKAGE_CLASSES {
        if !LEAKAGE_MATRIX.iter().any(|c| &c.leakage_class == class) {
            return Err(format!("leakage class {class} is not covered by any cell"));
        }
    }

    Ok(LeakageMatrixReport {
        cells: LEAKAGE_MATRIX.len(),
        allow_cells,
        deny_cells,
        resource_aware_cells,
        tenant_only_cells,
        leakage_classes: LEAKAGE_CLASSES.len(),
        production_write_allowed: false,
    })
}
