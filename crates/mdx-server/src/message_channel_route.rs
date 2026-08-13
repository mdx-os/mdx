// First-class channels over HTTP. POST /messages/channels.json creates a
// channel as a governed receipt; POST /messages/channel-updates.json records a
// rename, description/topic change, or archive; GET
// /messages/channels/projection.json folds the created receipt and every update
// into the current state of each channel (latest value wins per field, archived
// channels marked). This is what lets a channel be renamed, described, and
// retired - until now a channel was only a string on each message.
// Integration: mod + dispatch in main.rs, three HttpRouteDeclaration entries in
// mdx-core http_routes.rs (+3 count), two POST paths in GOVERNED_WRITE_ROUTES,
// two ActionKind variants, smoke bodies in mdx-codegen route_smoke_bodies.rs.
use crate::RouteResponse;
use mdx_core::{
    ChannelCreate, ChannelMemberAdd, ChannelMemberRemove, ChannelUpdate, MdxKernel,
    json_string_literal,
};
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

// The reader on the local serving path is the deterministic local session
// (user_id "local_user"). The Postgres serving path threads the verified
// trusted-session actor instead; the resource-aware RLS policy is its backstop.
const LOCAL_REQUESTER: &str = "local_user";

fn normalize_actor(id: &str) -> &str {
    id.trim()
        .trim_start_matches("human:")
        .trim_start_matches("agent:")
        .trim_start_matches("system:")
}

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/messages/channels.json" => Some(handle_create(method, body, kernel)),
        "/messages/channel-updates.json" => Some(handle_update(method, body, kernel)),
        "/messages/channel-members.json" => Some(handle_member_add(method, body, kernel)),
        "/messages/channel-member-removals.json" => {
            Some(handle_member_remove(method, body, kernel))
        }
        "/messages/channels/projection.json" => Some(handle_projection(method, kernel)),
        _ => None,
    }
}

fn handle_member_add(
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
    let channel_id = field("channel_id");
    let member_actor_id = field("member_actor_id");
    let member_role = field("member_role");
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let report = match kernel.add_message_channel_member_with_identity(
        ChannelMemberAdd {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            channel_id: &channel_id,
            member_actor_id: &member_actor_id,
            member_role: &member_role,
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => return Ok(member_refusal(&error.message())),
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-message-channel-member-local-post","status":"ADDED","auth_session_status":{},"channel_id":{},"member_actor_id":{},"member_role":{},"channel_receipt_id":{},"policy_decision_id":{},"projection_route":"/messages/channels/projection.json","production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&report.channel_id),
            json_string_literal(&report.member_actor_id),
            json_string_literal(&report.member_role),
            json_string_literal(&report.receipt_id),
            json_string_literal(&report.policy_decision_id),
        ),
    ))
}

fn handle_member_remove(
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
    let channel_id = field("channel_id");
    let member_actor_id = field("member_actor_id");
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let report = match kernel.remove_message_channel_member_with_identity(
        ChannelMemberRemove {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            channel_id: &channel_id,
            member_actor_id: &member_actor_id,
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => return Ok(member_refusal(&error.message())),
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-message-channel-member-removal-local-post","status":"REMOVED","auth_session_status":{},"channel_id":{},"member_actor_id":{},"channel_receipt_id":{},"policy_decision_id":{},"projection_route":"/messages/channels/projection.json","production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&report.channel_id),
            json_string_literal(&report.member_actor_id),
            json_string_literal(&report.receipt_id),
            json_string_literal(&report.policy_decision_id),
        ),
    ))
}

fn member_refusal(reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-message-channel-member-local-post","status":"REFUSED","reason":{},"channel_receipt_id":"","production_write_allowed":false}}"#,
            json_string_literal(reason)
        ),
    )
}

fn handle_create(
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
    let channel_id = field("channel_id");
    let name = field("name");
    let description = field("description");
    let topic = field("topic");
    let channel_kind = field("channel_kind");
    let visibility = field("visibility");
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let report = match kernel.create_message_channel_with_identity(
        ChannelCreate {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            channel_id: &channel_id,
            name: &name,
            description: &description,
            topic: &topic,
            channel_kind: &channel_kind,
            visibility: &visibility,
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => return Ok(refusal(&error.message())),
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-message-channel-local-post","status":"CREATED","auth_session_status":{},"channel_id":{},"channel_kind":{},"visibility":{},"channel_receipt_id":{},"policy_decision_id":{},"projection_route":"/messages/channels/projection.json","production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&report.channel_id),
            json_string_literal(report.channel_kind),
            json_string_literal(report.visibility),
            json_string_literal(&report.receipt_id),
            json_string_literal(&report.policy_decision_id),
        ),
    ))
}

fn handle_update(
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
    let channel_id = field("channel_id");
    let name = field("name");
    let description = field("description");
    let topic = field("topic");
    let status = field("status");
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let visibility = field("visibility");
    let report = match kernel.update_message_channel_with_identity(
        ChannelUpdate {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            channel_id: &channel_id,
            name: &name,
            description: &description,
            topic: &topic,
            status: &status,
            visibility: &visibility,
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => return Ok(refusal(&error.message())),
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-message-channel-update-local-post","status":"UPDATED","auth_session_status":{},"channel_id":{},"channel_receipt_id":{},"policy_decision_id":{},"projection_route":"/messages/channels/projection.json","production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&report.channel_id),
            json_string_literal(&report.receipt_id),
            json_string_literal(&report.policy_decision_id),
        ),
    ))
}

// The current shape of a channel, folded from its created receipt and its
// updates in ledger order.
struct ChannelState {
    channel_id: String,
    name: String,
    description: String,
    topic: String,
    channel_kind: String,
    visibility: String,
    status: String,
    created_by: String,
    created_receipt_id: String,
    last_receipt_id: String,
    // member_actor_id -> role, in insertion order; the creator is first owner.
    members: Vec<(String, String)>,
}

fn set_member(members: &mut Vec<(String, String)>, actor_id: &str, role: &str) {
    if let Some(entry) = members.iter_mut().find(|(id, _)| id == actor_id) {
        entry.1 = role.to_string();
    } else {
        members.push((actor_id.to_string(), role.to_string()));
    }
}

fn drop_member(members: &mut Vec<(String, String)>, actor_id: &str) {
    members.retain(|(id, _)| id != actor_id);
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

    // Preserve first-seen order so the list is stable across reloads.
    let mut order: Vec<String> = Vec::new();
    let mut states: BTreeMap<String, ChannelState> = BTreeMap::new();
    for receipt in kernel.ledger().entries() {
        let is_create = receipt.kind == mdx_core::MESSAGE_CHANNEL_CREATED_KIND;
        let is_update = receipt.kind == mdx_core::MESSAGE_CHANNEL_UPDATED_KIND;
        let is_member_add = receipt.kind == mdx_core::MESSAGE_CHANNEL_MEMBER_ADDED_KIND;
        let is_member_remove = receipt.kind == mdx_core::MESSAGE_CHANNEL_MEMBER_REMOVED_KIND;
        if !is_create && !is_update && !is_member_add && !is_member_remove {
            continue;
        }
        let channel_id = pv(receipt, "channel_id").to_string();
        if channel_id.is_empty() {
            continue;
        }
        if is_create {
            if !states.contains_key(&channel_id) {
                order.push(channel_id.clone());
            }
            let creator = receipt.actor_id.as_str().to_string();
            states.insert(
                channel_id.clone(),
                ChannelState {
                    channel_id: channel_id.clone(),
                    name: pv(receipt, "name").to_string(),
                    description: pv(receipt, "description").to_string(),
                    topic: pv(receipt, "topic").to_string(),
                    channel_kind: non_empty(pv(receipt, "channel_kind"), "team"),
                    visibility: non_empty(pv(receipt, "visibility"), "public"),
                    status: non_empty(pv(receipt, "status"), "active"),
                    created_by: creator.clone(),
                    created_receipt_id: receipt.receipt_id.clone(),
                    last_receipt_id: receipt.receipt_id.clone(),
                    // The creator is the channel's first owner.
                    members: vec![(creator, "owner".to_string())],
                },
            );
        } else if is_member_add {
            if let Some(state) = states.get_mut(&channel_id) {
                set_member(
                    &mut state.members,
                    pv(receipt, "member_actor_id"),
                    &non_empty(pv(receipt, "member_role"), "member"),
                );
                state.last_receipt_id = receipt.receipt_id.clone();
            }
        } else if is_member_remove {
            if let Some(state) = states.get_mut(&channel_id) {
                drop_member(&mut state.members, pv(receipt, "member_actor_id"));
                state.last_receipt_id = receipt.receipt_id.clone();
            }
        } else if let Some(state) = states.get_mut(&channel_id) {
            // A partial edit: only non-empty fields move the state.
            let name = pv(receipt, "name");
            let description = pv(receipt, "description");
            let topic = pv(receipt, "topic");
            let status = pv(receipt, "status");
            let visibility = pv(receipt, "visibility");
            if !name.is_empty() {
                state.name = name.to_string();
            }
            if !description.is_empty() {
                state.description = description.to_string();
            }
            if !topic.is_empty() {
                state.topic = topic.to_string();
            }
            if !status.is_empty() {
                state.status = status.to_string();
            }
            if !visibility.is_empty() {
                state.visibility = visibility.to_string();
            }
            state.last_receipt_id = receipt.receipt_id.clone();
        }
    }

    let mut channels: Vec<String> = Vec::new();
    let mut active = 0usize;
    let mut archived = 0usize;
    let mut hidden_private = 0usize;
    for id in &order {
        let Some(state) = states.get(id) else {
            continue;
        };
        // Fail closed: a private channel is omitted entirely unless the local
        // requester is one of its members. The creator is always a member.
        if state.visibility == "private"
            && !state
                .members
                .iter()
                .any(|(actor_id, _)| normalize_actor(actor_id) == normalize_actor(LOCAL_REQUESTER))
        {
            hidden_private += 1;
            continue;
        }
        if state.status == "archived" {
            archived += 1;
        } else {
            active += 1;
        }
        let members: Vec<String> = state
            .members
            .iter()
            .map(|(actor_id, role)| {
                format!(
                    r#"{{"actor_id":{},"role":{}}}"#,
                    json_string_literal(actor_id),
                    json_string_literal(role),
                )
            })
            .collect();
        channels.push(format!(
            r#"{{"channel_id":{},"name":{},"description":{},"topic":{},"channel_kind":{},"visibility":{},"status":{},"created_by":{},"member_count":{},"members":[{}],"channel_receipt_id":{},"last_receipt_id":{},"receipt_backed":true}}"#,
            json_string_literal(&state.channel_id),
            json_string_literal(&state.name),
            json_string_literal(&state.description),
            json_string_literal(&state.topic),
            json_string_literal(&state.channel_kind),
            json_string_literal(&state.visibility),
            json_string_literal(&state.status),
            json_string_literal(&state.created_by),
            state.members.len(),
            members.join(","),
            json_string_literal(&state.created_receipt_id),
            json_string_literal(&state.last_receipt_id),
        ));
    }

    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-message-channels-local-projection","status":"OK","created_kind":"message.channel.created","updated_kind":"message.channel.updated","derivation":"each channel folded from its created receipt and its updates, latest value wins per field; a private channel is omitted unless the reader is a member; archived channels stay on the record","write_route":"/messages/channels.json","update_route":"/messages/channel-updates.json","reader":{},"channel_count":{},"active_count":{},"archived_count":{},"hidden_private_count":{},"channels":[{}],"production_write_allowed":false}}"#,
            json_string_literal(LOCAL_REQUESTER),
            channels.len(),
            active,
            archived,
            hidden_private,
            channels.join(","),
        ),
    ))
}

fn pv<'a>(receipt: &'a mdx_core::Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}

fn non_empty(value: &str, default: &str) -> String {
    if value.is_empty() {
        default.to_string()
    } else {
        value.to_string()
    }
}

fn refusal(reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-message-channel-local-post","status":"REFUSED","reason":{},"channel_receipt_id":"","production_write_allowed":false}}"#,
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
