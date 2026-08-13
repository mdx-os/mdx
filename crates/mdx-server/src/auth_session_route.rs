pub(crate) fn render_json() -> String {
    if let Some(identity) = crate::request_security::current_verified_identity() {
        return format!(
            "{}\n",
            serde_json::json!({
                "name": "mdx-auth-session-boundary",
                "source": "verified_trusted_session",
                "status": "VERIFIED_TRUSTED_SESSION",
                "local_route": "/local/auth-session.json",
                "verified_session": {
                    "tenant_id": identity.tenant_id,
                    "user_id": identity.actor_id,
                    "role": identity.actor_role,
                    "actor_kind": identity.actor_kind,
                    "subject_actor_id": identity.subject_actor_id,
                    "authority_scope": identity.authority_scope,
                    "delegation_id": identity.delegation_id.unwrap_or_default()
                },
                "roles": ["owner", "operator", "auditor", "viewer"],
                "actor_admission": {
                    "status": "VERIFIED-TRUSTED-SESSION",
                    "scope": "governed_write_commands"
                }
            })
        );
    }
    format!(
        "{}\n",
        include_str!("../../../generated/auth/mdx-auth-session-boundary.json").trim()
    )
}
