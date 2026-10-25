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
    pub members: Members,
    /// The code that lets a person join: 8 letters or digits. Any member can
    /// replace it with a new one, and the old one stops working.
    #[schema(value_type = String, min_length = 6, max_length = 8, pattern = "^[A-Za-z0-9]+$")]
    pub invite_code: InviteCode,
}

/// The members of a group: a list of users, in the order they joined, that holds
/// each user once.
///
/// It is a set that keeps its order, because the order is in the JSON (the
/// creator is the first). [`Members::insert`] is the only way to add a user and
/// it does nothing for one who is in already, so a member is never listed
/// twice; a body that lists one twice is refused where it is read. A lookup
/// walks the list, which is as long as the people of one group.
///
/// # Examples
///
/// ```
/// use sens_o_matic::models::{Members, UserId};
///
/// let (ada, grace) = (UserId::random(), UserId::random());
/// let mut members = Members::new(ada);
///
/// assert!(members.insert(grace));
/// assert!(!members.insert(grace)); // already a member
/// assert_eq!(members.as_slice(), [ada, grace]);
///
/// // The JSON is an array, in the same order.
/// let json = serde_json::to_string(&members).unwrap();
/// assert_eq!(json, format!(r#"["{ada}","{grace}"]"#));
/// assert_eq!(serde_json::from_str::<Members>(&json).unwrap(), members);
///
/// // A body that lists a user twice is refused.
/// let twice = format!(r#"["{ada}","{ada}"]"#);
/// assert!(serde_json::from_str::<Members>(&twice).is_err());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Vec<UserId>")]
pub struct Members(Vec<UserId>);

/// The same user is listed twice in the members of a group. See [`Members`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "user {0} is listed twice in members; a member is listed once. \
     See sens_o_matic::models::group::Members"
)]
pub struct DuplicateMember(pub UserId);

impl Members {
    /// Makes the members of a new group: only the creator.
    pub fn new(creator: UserId) -> Self {
        Self(vec![creator])
    }

    /// Adds a user at the end. Returns `false` and changes nothing when the user
    /// is a member already.
    pub fn insert(&mut self, user: UserId) -> bool {
        let is_new = !self.contains(user);
        if is_new {
            self.0.push(user);
        }
        is_new
    }

    /// Removes a user. Returns `false` and changes nothing when the user is not
    /// a member. The others keep their order.
    pub fn remove(&mut self, user: UserId) -> bool {
        let len = self.0.len();
        self.0.retain(|&id| id != user);
        self.0.len() != len
    }

    /// Tells whether the user is a member.
    pub fn contains(&self, user: UserId) -> bool {
        self.0.contains(&user)
    }

    /// The members, in the order they joined.
    pub fn as_slice(&self) -> &[UserId] {
        &self.0
    }
}

impl TryFrom<Vec<UserId>> for Members {
    type Error = DuplicateMember;

    fn try_from(users: Vec<UserId>) -> Result<Self, Self::Error> {
        let mut members = Vec::with_capacity(users.len());
        for user in users {
            if members.contains(&user) {
                return Err(DuplicateMember(user));
            }
            members.push(user);
        }
        Ok(Self(members))
    }
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
    /// assert_eq!(group.members.as_slice(), [creator]);
    /// assert!(group.is_member(creator));
    /// ```
    pub fn new(request: CreateGroupRequest) -> Self {
        Self {
            id: GroupId::random(),
            name: request.name,
            members: Members::new(request.creator_id),
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
    /// assert_eq!(group.members.as_slice().len(), 2);
    /// ```
    pub fn add_member(&mut self, user_id: UserId) {
        self.members.insert(user_id);
    }

    /// Removes a member. Does nothing when the user is not a member. Nothing
    /// stops the last member, or the creator, from leaving; the group then
    /// stays, possibly with no member.
    pub fn remove_member(&mut self, user_id: UserId) {
        self.members.remove(user_id);
    }

    /// Replaces the invite code with a new one; the old code stops working.
    pub fn regenerate_invite_code(&mut self) {
        self.invite_code = InviteCode::random();
    }

    /// Tells whether the user is a member of the group.
    pub fn is_member(&self, user_id: UserId) -> bool {
        self.members.contains(user_id)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// The JSON array of `members` is in the order the users joined, as it was
    /// when `members` was a `Vec`, and a member who leaves does not move the
    /// others.
    #[test]
    fn members_are_an_array_in_the_order_they_joined() {
        let creator = UserId::random();
        let (ada, grace) = (UserId::random(), UserId::random());
        let mut group = Group::new(CreateGroupRequest {
            name: "Friday crew".parse().unwrap(),
            creator_id: creator,
        });
        group.add_member(ada);
        group.add_member(grace);
        group.add_member(ada);

        let json = serde_json::to_value(&group).unwrap();
        assert_eq!(json["members"], json!([creator, ada, grace]));

        group.remove_member(ada);
        let json = serde_json::to_value(&group).unwrap();
        assert_eq!(json["members"], json!([creator, grace]));
        assert_eq!(
            serde_json::from_value::<Group>(json).unwrap().members,
            group.members
        );
    }
}
