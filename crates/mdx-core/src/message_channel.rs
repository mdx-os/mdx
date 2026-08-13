//! First-class message channels. Until now a channel was only a string on each
//! message receipt plus a list in the browser - it could not be renamed, given
//! a description, archived, or owned. This module makes a channel a governed
//! object: creating one records `message.channel.created`, and every later edit
//! (rename, description, topic, archive) records `message.channel.updated`. The
//! current state of a channel is the fold of its created receipt and its
//! updates, latest value wins per field - the same append-only shape the rest
//! of the kernel uses, so a channel's whole history stays on the record.
//!
//! Channels carry members (people and agents) and a visibility. A `private`
//! channel is read-gated to its members: `message_channel_readable_by` and
//! `message_unreadable_channels_for` are the in-memory enforcement the channel
//! and message projections apply fail-closed on the local serving path. The
//! Postgres serving path adds the resource-aware RLS policy the access-control
//! matrix declares (a tenant admin cannot read a private channel's content).
//! A DM is just a `dm`-kind private channel with the two participants as members.

use crate::strategy_board::BoardWrite;
use crate::{ActionKind, GovernedWriteIdentity, MdxKernel, StorageProvider, payload};

/// What a channel is for. `team` is the default shared channel; `dm` is a
/// direct conversation (membership-gated, exactly its participants); `system`
/// carries stack activity. Anything else is refused.
pub const MESSAGE_CHANNEL_KINDS: &[&str] = &["team", "dm", "system"];

/// Who can see a channel. `public` is visible to the tenant; `private` is
/// membership-gated. The read gate for `private` lands with membership; this
/// slice records the intent on the receipt.
pub const MESSAGE_CHANNEL_VISIBILITIES: &[&str] = &["public", "private"];

/// A channel's lifecycle. `archived` removes it from the active list without
/// deleting its history - the receipts-honest form of "delete a channel".
pub const MESSAGE_CHANNEL_STATUSES: &[&str] = &["active", "archived"];

const MAX_CHANNEL_NAME_CHARS: usize = 80;
const MAX_CHANNEL_DESCRIPTION_CHARS: usize = 280;
const MAX_CHANNEL_TOPIC_CHARS: usize = 120;
const MAX_CHANNEL_ID_CHARS: usize = 48;
const MIN_CHANNEL_ID_CHARS: usize = 2;

pub const MESSAGE_CHANNEL_CREATED_KIND: &str = "message.channel.created";
pub const MESSAGE_CHANNEL_UPDATED_KIND: &str = "message.channel.updated";
pub const MESSAGE_CHANNEL_MEMBER_ADDED_KIND: &str = "message.channel.member.added";
pub const MESSAGE_CHANNEL_MEMBER_REMOVED_KIND: &str = "message.channel.member.removed";

/// A member's standing in a channel. `owner` can edit and archive the channel
/// and manage members; `member` reads and posts; `observer` reads only. The
/// channel's creator is its first owner.
pub const MESSAGE_MEMBER_ROLES: &[&str] = &["owner", "member", "observer"];

#[derive(Clone, Copy, Debug)]
pub struct ChannelCreate<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    /// The channel slug, e.g. "forge" - lowercase letters, digits, hyphens.
    pub channel_id: &'a str,
    pub name: &'a str,
    pub description: &'a str,
    pub topic: &'a str,
    /// One of MESSAGE_CHANNEL_KINDS. Empty defaults to `team`.
    pub channel_kind: &'a str,
    /// One of MESSAGE_CHANNEL_VISIBILITIES. Empty defaults to `public`.
    pub visibility: &'a str,
}

/// A partial edit: any empty field is left unchanged, so a rename does not also
/// have to resend the description. `status` empty means the lifecycle is
/// untouched.
#[derive(Clone, Copy, Debug)]
pub struct ChannelUpdate<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub channel_id: &'a str,
    pub name: &'a str,
    pub description: &'a str,
    pub topic: &'a str,
    /// One of MESSAGE_CHANNEL_STATUSES, or empty to leave unchanged.
    pub status: &'a str,
    /// One of MESSAGE_CHANNEL_VISIBILITIES, or empty to leave unchanged.
    pub visibility: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelReport {
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub channel_id: String,
    pub channel_kind: &'static str,
    pub visibility: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChannelError {
    Missing(&'static str),
    TooLong(&'static str, usize, usize),
    BadSlug(String),
    UnknownKind(String),
    UnknownVisibility(String),
    UnknownStatus(String),
    UnknownRole(String),
    AlreadyExists(String),
    NotFound(String),
    NothingToUpdate,
}

impl ChannelError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("name a channel: missing {field}"),
            Self::TooLong(field, len, max) => {
                format!("channel {field} is {len} characters; the limit is {max}")
            }
            Self::BadSlug(slug) => format!(
                "channel id \"{slug}\" must be {MIN_CHANNEL_ID_CHARS}-{MAX_CHANNEL_ID_CHARS} lowercase letters, digits, or hyphens"
            ),
            Self::UnknownKind(other) => format!(
                "channel kind must be one of {}; got {other}",
                MESSAGE_CHANNEL_KINDS.join(", ")
            ),
            Self::UnknownVisibility(other) => format!(
                "channel visibility must be one of {}; got {other}",
                MESSAGE_CHANNEL_VISIBILITIES.join(", ")
            ),
            Self::UnknownStatus(other) => format!(
                "channel status must be one of {}; got {other}",
                MESSAGE_CHANNEL_STATUSES.join(", ")
            ),
            Self::UnknownRole(other) => format!(
                "member role must be one of {}; got {other}",
                MESSAGE_MEMBER_ROLES.join(", ")
            ),
            Self::AlreadyExists(id) => format!("a channel named \"{id}\" already exists"),
            Self::NotFound(id) => format!("no channel named \"{id}\" to edit"),
            Self::NothingToUpdate => {
                "edit a channel: give a new name, description, topic, or status".to_string()
            }
        }
    }
}

/// A slug the relay topic and the message receipts can carry safely: lowercase
/// letters, digits, and single hyphens, no leading/trailing hyphen.
fn valid_slug(slug: &str) -> bool {
    let len = slug.chars().count();
    if !(MIN_CHANNEL_ID_CHARS..=MAX_CHANNEL_ID_CHARS).contains(&len) {
        return false;
    }
    if slug.starts_with('-') || slug.ends_with('-') {
        return false;
    }
    slug.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn normalize_one(
    raw: &str,
    allowed: &[&'static str],
    default: &'static str,
    err: impl FnOnce(String) -> ChannelError,
) -> Result<&'static str, ChannelError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(default);
    }
    let lowered = trimmed.to_ascii_lowercase();
    allowed
        .iter()
        .copied()
        .find(|value| *value == lowered)
        .ok_or_else(|| err(trimmed.to_string()))
}

impl<S: StorageProvider> MdxKernel<S> {
    /// Does a channel with this id already exist in this tenant?
    pub fn message_channel_exists(&self, tenant_id: &str, channel_id: &str) -> bool {
        self.ledger()
            .query()
            .by_kind(MESSAGE_CHANNEL_CREATED_KIND)
            .into_iter()
            .any(|receipt| {
                pv(receipt, "channel_id") == channel_id && receipt.tenant_id.as_str() == tenant_id
            })
    }

    pub fn create_message_channel(
        &mut self,
        request: ChannelCreate<'_>,
    ) -> Result<ChannelReport, ChannelError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.create_message_channel_with_identity(request, &identity)
    }

    pub fn create_message_channel_with_identity(
        &mut self,
        request: ChannelCreate<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<ChannelReport, ChannelError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("channel_id", request.channel_id),
            ("name", request.name),
        ] {
            if value.trim().is_empty() {
                return Err(ChannelError::Missing(field));
            }
        }
        let channel_id = request.channel_id.trim();
        if !valid_slug(channel_id) {
            return Err(ChannelError::BadSlug(channel_id.to_string()));
        }
        check_lengths(request.name, request.description, request.topic)?;
        let channel_kind = normalize_one(
            request.channel_kind,
            MESSAGE_CHANNEL_KINDS,
            "team",
            ChannelError::UnknownKind,
        )?;
        let visibility = normalize_one(
            request.visibility,
            MESSAGE_CHANNEL_VISIBILITIES,
            "public",
            ChannelError::UnknownVisibility,
        )?;
        if self.message_channel_exists(request.tenant_id, channel_id) {
            return Err(ChannelError::AlreadyExists(channel_id.to_string()));
        }
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "message_channel",
                action: ActionKind::CreateMessageChannel,
                transition: "CREATE_MESSAGE_CHANNEL",
                kind: MESSAGE_CHANNEL_CREATED_KIND,
            },
            payload(&[
                ("channel_id", channel_id),
                ("name", request.name.trim()),
                ("description", request.description.trim()),
                ("topic", request.topic.trim()),
                ("channel_kind", channel_kind),
                ("visibility", visibility),
                ("status", "active"),
                ("identity_source", &identity.identity_source),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(ChannelReport {
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            channel_id: channel_id.to_string(),
            channel_kind,
            visibility,
        })
    }

    pub fn update_message_channel(
        &mut self,
        request: ChannelUpdate<'_>,
    ) -> Result<ChannelReport, ChannelError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.update_message_channel_with_identity(request, &identity)
    }

    pub fn update_message_channel_with_identity(
        &mut self,
        request: ChannelUpdate<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<ChannelReport, ChannelError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("channel_id", request.channel_id),
        ] {
            if value.trim().is_empty() {
                return Err(ChannelError::Missing(field));
            }
        }
        let channel_id = request.channel_id.trim();
        check_lengths(request.name, request.description, request.topic)?;
        let status = normalize_one(
            request.status,
            MESSAGE_CHANNEL_STATUSES,
            "",
            ChannelError::UnknownStatus,
        )?;
        let visibility = normalize_one(
            request.visibility,
            MESSAGE_CHANNEL_VISIBILITIES,
            "",
            ChannelError::UnknownVisibility,
        )?;
        let touches_anything = !request.name.trim().is_empty()
            || !request.description.trim().is_empty()
            || !request.topic.trim().is_empty()
            || !status.is_empty()
            || !visibility.is_empty();
        if !touches_anything {
            return Err(ChannelError::NothingToUpdate);
        }
        if !self.message_channel_exists(request.tenant_id, channel_id) {
            return Err(ChannelError::NotFound(channel_id.to_string()));
        }
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "message_channel",
                action: ActionKind::UpdateMessageChannel,
                transition: "UPDATE_MESSAGE_CHANNEL",
                kind: MESSAGE_CHANNEL_UPDATED_KIND,
            },
            payload(&[
                ("channel_id", channel_id),
                ("name", request.name.trim()),
                ("description", request.description.trim()),
                ("topic", request.topic.trim()),
                ("status", status),
                ("visibility", visibility),
                ("identity_source", &identity.identity_source),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(ChannelReport {
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            channel_id: channel_id.to_string(),
            channel_kind: "team",
            visibility: "public",
        })
    }
}

/// Adding (or re-roling) a member of a channel.
#[derive(Clone, Copy, Debug)]
pub struct ChannelMemberAdd<'a> {
    pub tenant_id: &'a str,
    /// Who is doing the adding.
    pub actor_id: &'a str,
    pub channel_id: &'a str,
    /// The person or agent being added, e.g. "human:priya" or "agent:scout".
    pub member_actor_id: &'a str,
    /// One of MESSAGE_MEMBER_ROLES. Empty defaults to `member`.
    pub member_role: &'a str,
}

/// Removing a member from a channel.
#[derive(Clone, Copy, Debug)]
pub struct ChannelMemberRemove<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub channel_id: &'a str,
    pub member_actor_id: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelMemberReport {
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub channel_id: String,
    pub member_actor_id: String,
    pub member_role: String,
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn add_message_channel_member_with_identity(
        &mut self,
        request: ChannelMemberAdd<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<ChannelMemberReport, ChannelError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("channel_id", request.channel_id),
            ("member_actor_id", request.member_actor_id),
        ] {
            if value.trim().is_empty() {
                return Err(ChannelError::Missing(field));
            }
        }
        let channel_id = request.channel_id.trim();
        let role = normalize_one(
            request.member_role,
            MESSAGE_MEMBER_ROLES,
            "member",
            ChannelError::UnknownRole,
        )?;
        if !self.message_channel_exists(request.tenant_id, channel_id) {
            return Err(ChannelError::NotFound(channel_id.to_string()));
        }
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "message_channel",
                action: ActionKind::AddMessageChannelMember,
                transition: "ADD_MESSAGE_CHANNEL_MEMBER",
                kind: MESSAGE_CHANNEL_MEMBER_ADDED_KIND,
            },
            payload(&[
                ("channel_id", channel_id),
                ("member_actor_id", request.member_actor_id.trim()),
                ("member_role", role),
                ("identity_source", &identity.identity_source),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(ChannelMemberReport {
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            channel_id: channel_id.to_string(),
            member_actor_id: request.member_actor_id.trim().to_string(),
            member_role: role.to_string(),
        })
    }

    pub fn remove_message_channel_member_with_identity(
        &mut self,
        request: ChannelMemberRemove<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<ChannelMemberReport, ChannelError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("channel_id", request.channel_id),
            ("member_actor_id", request.member_actor_id),
        ] {
            if value.trim().is_empty() {
                return Err(ChannelError::Missing(field));
            }
        }
        let channel_id = request.channel_id.trim();
        if !self.message_channel_exists(request.tenant_id, channel_id) {
            return Err(ChannelError::NotFound(channel_id.to_string()));
        }
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "message_channel",
                action: ActionKind::RemoveMessageChannelMember,
                transition: "REMOVE_MESSAGE_CHANNEL_MEMBER",
                kind: MESSAGE_CHANNEL_MEMBER_REMOVED_KIND,
            },
            payload(&[
                ("channel_id", channel_id),
                ("member_actor_id", request.member_actor_id.trim()),
                ("identity_source", &identity.identity_source),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(ChannelMemberReport {
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            channel_id: channel_id.to_string(),
            member_actor_id: request.member_actor_id.trim().to_string(),
            member_role: "removed".to_string(),
        })
    }
}

/// An actor id with its kind prefix stripped, so "local_user" and
/// "human:local_user" compare equal for membership.
fn normalize_actor(id: &str) -> &str {
    id.trim()
        .trim_start_matches("human:")
        .trim_start_matches("agent:")
        .trim_start_matches("system:")
}

impl<S: StorageProvider> MdxKernel<S> {
    /// The current visibility and members of a channel, folded from its
    /// created/updated and member receipts. None if the channel was never
    /// created as a governed object (a legacy message-only channel).
    fn fold_channel_access(
        &self,
        tenant_id: &str,
        channel_id: &str,
    ) -> Option<(String, Vec<String>)> {
        let mut found = false;
        let mut visibility = "public".to_string();
        let mut members: Vec<String> = Vec::new();
        for receipt in self.ledger().entries() {
            if receipt.tenant_id.as_str() != tenant_id || pv(receipt, "channel_id") != channel_id {
                continue;
            }
            match receipt.kind.as_str() {
                MESSAGE_CHANNEL_CREATED_KIND => {
                    found = true;
                    let value = pv(receipt, "visibility");
                    if !value.is_empty() {
                        visibility = value.to_string();
                    }
                    let creator = normalize_actor(receipt.actor_id.as_str()).to_string();
                    if !members.contains(&creator) {
                        members.push(creator);
                    }
                }
                MESSAGE_CHANNEL_UPDATED_KIND => {
                    let value = pv(receipt, "visibility");
                    if !value.is_empty() {
                        visibility = value.to_string();
                    }
                }
                MESSAGE_CHANNEL_MEMBER_ADDED_KIND => {
                    let member = normalize_actor(pv(receipt, "member_actor_id")).to_string();
                    if !member.is_empty() && !members.contains(&member) {
                        members.push(member);
                    }
                }
                MESSAGE_CHANNEL_MEMBER_REMOVED_KIND => {
                    let member = normalize_actor(pv(receipt, "member_actor_id")).to_string();
                    members.retain(|existing| existing != &member);
                }
                _ => {}
            }
        }
        found.then_some((visibility, members))
    }

    /// Can this actor read this channel? Public and legacy channels are
    /// readable by anyone in the tenant; a private channel is readable only by
    /// its members. The creator is always a member. This is the in-memory
    /// enforcement on the local serving path; the Postgres serving path adds
    /// the resource-aware RLS policy the access-control matrix declares.
    pub fn message_channel_readable_by(
        &self,
        tenant_id: &str,
        channel_id: &str,
        actor_id: &str,
    ) -> bool {
        match self.fold_channel_access(tenant_id, channel_id) {
            None => true,
            Some((visibility, members)) => {
                if visibility != "private" {
                    return true;
                }
                let who = normalize_actor(actor_id);
                members.iter().any(|member| member == who)
            }
        }
    }

    /// The channel ids in this tenant this actor may NOT read - private
    /// channels they are not a member of. Used to filter the message and
    /// channel projections fail-closed.
    pub fn message_unreadable_channels_for(
        &self,
        tenant_id: &str,
        actor_id: &str,
    ) -> std::collections::HashSet<String> {
        let mut hidden = std::collections::HashSet::new();
        let mut seen = std::collections::HashSet::new();
        for receipt in self.ledger().entries() {
            if receipt.kind != MESSAGE_CHANNEL_CREATED_KIND
                || receipt.tenant_id.as_str() != tenant_id
            {
                continue;
            }
            let channel_id = pv(receipt, "channel_id").to_string();
            if channel_id.is_empty() || !seen.insert(channel_id.clone()) {
                continue;
            }
            if !self.message_channel_readable_by(tenant_id, &channel_id, actor_id) {
                hidden.insert(channel_id);
            }
        }
        hidden
    }
}

fn check_lengths(name: &str, description: &str, topic: &str) -> Result<(), ChannelError> {
    if name.chars().count() > MAX_CHANNEL_NAME_CHARS {
        return Err(ChannelError::TooLong(
            "name",
            name.chars().count(),
            MAX_CHANNEL_NAME_CHARS,
        ));
    }
    if description.chars().count() > MAX_CHANNEL_DESCRIPTION_CHARS {
        return Err(ChannelError::TooLong(
            "description",
            description.chars().count(),
            MAX_CHANNEL_DESCRIPTION_CHARS,
        ));
    }
    if topic.chars().count() > MAX_CHANNEL_TOPIC_CHARS {
        return Err(ChannelError::TooLong(
            "topic",
            topic.chars().count(),
            MAX_CHANNEL_TOPIC_CHARS,
        ));
    }
    Ok(())
}

fn pv<'a>(receipt: &'a crate::Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create<'a>(channel_id: &'a str, name: &'a str) -> ChannelCreate<'a> {
        ChannelCreate {
            tenant_id: "tenant_local",
            actor_id: "human:md",
            channel_id,
            name,
            description: "",
            topic: "",
            channel_kind: "",
            visibility: "",
        }
    }

    #[test]
    fn creating_a_channel_records_its_shape_and_defaults() {
        let mut kernel = MdxKernel::boot_local();
        let report = kernel
            .create_message_channel(create("forge", "Forge"))
            .expect("create");
        assert_eq!(report.channel_kind, "team");
        assert_eq!(report.visibility, "public");
        assert!(!report.receipt_id.is_empty());
        let receipt = kernel
            .ledger()
            .query()
            .by_kind(MESSAGE_CHANNEL_CREATED_KIND)
            .into_iter()
            .next()
            .expect("created receipt");
        assert_eq!(receipt.payload["channel_id"], "forge");
        assert_eq!(receipt.payload["status"], "active");
    }

    #[test]
    fn a_bad_slug_and_a_duplicate_are_refused() {
        let mut kernel = MdxKernel::boot_local();
        assert!(matches!(
            kernel.create_message_channel(create("Forge Ops", "x")),
            Err(ChannelError::BadSlug(_))
        ));
        kernel
            .create_message_channel(create("forge", "Forge"))
            .expect("first create");
        assert!(matches!(
            kernel.create_message_channel(create("forge", "Forge again")),
            Err(ChannelError::AlreadyExists(_))
        ));
    }

    #[test]
    fn a_private_channel_is_readable_only_by_its_members() {
        let mut kernel = MdxKernel::boot_local();
        // A private channel an agent owns - the local reader is not a member.
        kernel
            .create_message_channel(ChannelCreate {
                tenant_id: "local_tenant",
                actor_id: "agent:scout",
                channel_id: "secret",
                name: "Secret",
                description: "",
                topic: "",
                channel_kind: "",
                visibility: "private",
            })
            .expect("create private");
        assert!(!kernel.message_channel_readable_by("local_tenant", "secret", "local_user"));
        assert!(kernel.message_channel_readable_by("local_tenant", "secret", "agent:scout"));
        assert!(
            kernel
                .message_unreadable_channels_for("local_tenant", "local_user")
                .contains("secret")
        );

        // Adding the local reader (prefixed form) makes it readable - the
        // normalize step matches "human:local_user" to "local_user".
        let identity = GovernedWriteIdentity::local_demo("agent:scout");
        kernel
            .add_message_channel_member_with_identity(
                ChannelMemberAdd {
                    tenant_id: "local_tenant",
                    actor_id: "agent:scout",
                    channel_id: "secret",
                    member_actor_id: "human:local_user",
                    member_role: "member",
                },
                &identity,
            )
            .expect("add member");
        assert!(kernel.message_channel_readable_by("local_tenant", "secret", "local_user"));

        // A public channel is readable by anyone in the tenant.
        kernel
            .create_message_channel(ChannelCreate {
                tenant_id: "local_tenant",
                actor_id: "human:local_user",
                channel_id: "townsquare",
                name: "Town square",
                description: "",
                topic: "",
                channel_kind: "",
                visibility: "",
            })
            .expect("create public");
        assert!(kernel.message_channel_readable_by("local_tenant", "townsquare", "anyone"));
    }

    #[test]
    fn editing_a_channel_requires_it_to_exist_and_to_change_something() {
        let mut kernel = MdxKernel::boot_local();
        let empty = ChannelUpdate {
            tenant_id: "tenant_local",
            actor_id: "human:md",
            channel_id: "forge",
            name: "",
            description: "",
            topic: "",
            status: "",
            visibility: "",
        };
        // An empty edit is caught before existence - there is nothing to write.
        assert!(matches!(
            kernel.update_message_channel(empty),
            Err(ChannelError::NothingToUpdate)
        ));
        // A real edit on a channel that was never created is NotFound.
        let rename_missing = ChannelUpdate {
            name: "Renamed",
            ..empty
        };
        assert!(matches!(
            kernel.update_message_channel(rename_missing),
            Err(ChannelError::NotFound(_))
        ));
        kernel
            .create_message_channel(create("forge", "Forge"))
            .expect("create");
        assert!(matches!(
            kernel.update_message_channel(empty),
            Err(ChannelError::NothingToUpdate)
        ));
        let archived = ChannelUpdate {
            status: "archived",
            ..empty
        };
        let report = kernel
            .update_message_channel(archived)
            .expect("archive update");
        assert!(!report.receipt_id.is_empty());
        let receipt = kernel
            .ledger()
            .query()
            .by_kind(MESSAGE_CHANNEL_UPDATED_KIND)
            .into_iter()
            .next()
            .expect("updated receipt");
        assert_eq!(receipt.payload["status"], "archived");
        assert!(kernel.ledger().verify().is_ok());
    }
}
