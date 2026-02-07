use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Location {
    pub lat: f64,
    pub lng: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Default)]
pub struct UserPreferences {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_distance: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub favorite_areas: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub home_location: Option<Location>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct User {
    pub id: Uuid,
    pub name: String,
    pub avatar: Option<String>,
    pub preferences: Option<UserPreferences>,
}

#[derive(Debug, Clone, Deserialize, ToSchema, Validate)]
pub struct CreateUserRequest {
    #[validate(length(min = 1, max = 100))]
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferences: Option<UserPreferences>,
}

#[derive(Debug, Clone, Deserialize, ToSchema, Validate)]
pub struct UpdateUserRequest {
    #[validate(length(min = 1, max = 100))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferences: Option<UserPreferences>,
}

impl User {
    pub fn new(request: CreateUserRequest) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: request.name,
            avatar: request.avatar,
            preferences: request.preferences,
        }
    }

    pub fn update(&mut self, request: UpdateUserRequest) {
        if let Some(name) = request.name {
            self.name = name;
        }
        if request.avatar.is_some() || request.preferences.is_some() {
            if request.avatar.is_some() {
                self.avatar = request.avatar;
            }
            if let Some(prefs) = request.preferences {
                self.preferences = Some(prefs);
            }
        }
    }
}
