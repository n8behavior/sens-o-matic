//! The people a ping is sent to.
//!
//! A [`Group`] is a list of members and an invite code. Nobody is added by
//! another member or found by search: a person joins by presenting the invite
//! code, which any member can replace with a new one at any time (the old code
//! stops working at once). The members of a group are the only people who may
//! send a [`Ping`](super::Ping) to it or answer one.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::value::{InviteCode, Name};
use super::{GroupId, UserId};

/// A set of users who coordinate meetups together.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Group {
    /// Assigned by the server when the group is created.
    #[schema(value_type = String, format = "uuid")]
    pub id: GroupId,
    /// Display name, 1 to 100 characters.
    #[schema(value_type = String, min_length = 1, max_length = 100)]
    pub name: Name,
    /// Ids of the members, without duplicates, in the order they joined. The
    /// creator is the first.
    #[schema(value_type = Vec<String>, format = "uuid")]
    pub members: Vec<UserId>,
    /// The code that lets a person join: 8 letters or digits. Any member can
    /// replace it with a new one, and the old one stops working.
    #[schema(value_type = String, min_length = 6, max_length = 8, pattern = "^[A-Za-z0-9]+$")]
    pub invite_code: InviteCode,
}

/// The body of the request that creates a group.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct CreateGroupRequest {
    /// Display name, 1 to 100 characters.
    #[schema(value_type = String, min_length = 1, max_length = 100)]
    pub name: Name,
    /// The user who creates the group and becomes its first member. The server
    /// does not check that this user exists.
    #[schema(value_type = String, format = "uuid")]
    pub creator_id: UserId,
}

/// The body of the request that adds a user to a group by its invite code.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct JoinGroupRequest {
    /// The user who joins. The server does not check that this user exists.
    #[schema(value_type = String, format = "uuid")]
    pub user_id: UserId,
    /// The invite code of the group: 6 to 8 letters or digits. Case matters.
    #[schema(value_type = String, min_length = 6, max_length = 8, pattern = "^[A-Za-z0-9]+$")]
    pub invite_code: InviteCode,
}

/// The body of the request that removes a user from a group.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct LeaveGroupRequest {
    /// The user who leaves.
    #[schema(value_type = String, format = "uuid")]
    pub user_id: UserId,
}

/// The body of the request that replaces the invite code of a group.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct RegenerateInviteRequest {
    /// The user who asks for the new code; must be a member of the group.
    #[schema(value_type = String, format = "uuid")]
    pub user_id: UserId,
}

impl Group {
    /// Creates a group whose only member is the creator, with a new invite
    /// code.
    ///
    /// # Examples
    ///
    /// ```
    /// use sens_o_matic::models::{CreateGroupRequest, Group, UserId};
    ///
    /// let creator = UserId::random();
    /// let group = Group::new(CreateGroupRequest {
    ///     name: "Friday crew".parse().unwrap(),
    ///     creator_id: creator,
    /// });
    ///
    /// assert_eq!(group.members, vec![creator]);
    /// assert!(group.is_member(creator));
    /// ```
    pub fn new(request: CreateGroupRequest) -> Self {
        Self {
            id: GroupId::random(),
            name: request.name,
            members: vec![request.creator_id],
            invite_code: InviteCode::random(),
        }
    }

    /// Adds a member. Does nothing when the user is a member already, so a
    /// member is never listed twice.
    ///
    /// # Examples
    ///
    /// ```
    /// use sens_o_matic::models::{CreateGroupRequest, Group, UserId};
    ///
    /// let mut group = Group::new(CreateGroupRequest {
    ///     name: "Friday crew".parse().unwrap(),
    ///     creator_id: UserId::random(),
    /// });
    /// let friend = UserId::random();
    ///
    /// group.add_member(friend);
    /// group.add_member(friend);
    /// assert_eq!(group.members.len(), 2);
    /// ```
    pub fn add_member(&mut self, user_id: UserId) {
        if !self.members.contains(&user_id) {
            self.members.push(user_id);
        }
    }

    /// Removes a member. Does nothing when the user is not a member. Nothing
    /// stops the last member, or the creator, from leaving; the group then
    /// stays, possibly with no member.
    pub fn remove_member(&mut self, user_id: UserId) {
        self.members.retain(|&id| id != user_id);
    }

    /// Replaces the invite code with a new one; the old code stops working.
    pub fn regenerate_invite_code(&mut self) {
        self.invite_code = InviteCode::random();
    }

    /// Tells whether the user is a member of the group.
    pub fn is_member(&self, user_id: UserId) -> bool {
        self.members.contains(&user_id)
    }
}
