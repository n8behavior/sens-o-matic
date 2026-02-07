use rand::Rng;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Group {
    pub id: Uuid,
    pub name: String,
    pub members: Vec<Uuid>,
    pub invite_code: String,
}

#[derive(Debug, Clone, Deserialize, ToSchema, Validate)]
pub struct CreateGroupRequest {
    #[validate(length(min = 1, max = 100))]
    pub name: String,
    pub creator_id: Uuid,
}

#[derive(Debug, Clone, Deserialize, ToSchema, Validate)]
pub struct JoinGroupRequest {
    pub user_id: Uuid,
    #[validate(regex(path = *INVITE_CODE_REGEX))]
    pub invite_code: String,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct LeaveGroupRequest {
    pub user_id: Uuid,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct RegenerateInviteRequest {
    pub user_id: Uuid,
}

use std::sync::LazyLock;
static INVITE_CODE_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^[A-Za-z0-9]{6,8}$").unwrap());

pub fn generate_invite_code() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::rng();
    (0..8)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

impl Group {
    pub fn new(request: CreateGroupRequest) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: request.name,
            members: vec![request.creator_id],
            invite_code: generate_invite_code(),
        }
    }

    pub fn add_member(&mut self, user_id: Uuid) {
        if !self.members.contains(&user_id) {
            self.members.push(user_id);
        }
    }

    pub fn remove_member(&mut self, user_id: Uuid) {
        self.members.retain(|&id| id != user_id);
    }

    pub fn regenerate_invite_code(&mut self) {
        self.invite_code = generate_invite_code();
    }

    pub fn is_member(&self, user_id: Uuid) -> bool {
        self.members.contains(&user_id)
    }
}
