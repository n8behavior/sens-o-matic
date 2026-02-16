use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

use crate::models::error::AppError;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Availability {
    pub earliest: DateTime<Utc>,
    pub latest: DateTime<Utc>,
}

impl Availability {
    pub fn validate_times(&self) -> Result<(), AppError> {
        if self.latest <= self.earliest {
            return Err(AppError::BadRequest(
                "latest must be after earliest".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Default)]
pub struct ResponsePreferences {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_distance: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_areas: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excluded_areas: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Response {
    pub id: Uuid,
    pub user: Uuid,
    pub answer: bool,
    pub availability: Option<Availability>,
    pub preferences: Option<ResponsePreferences>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, ToSchema, Validate)]
pub struct CreateResponseRequest {
    pub user: Uuid,
    pub answer: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub availability: Option<Availability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferences: Option<ResponsePreferences>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct UpdateResponseRequest {
    pub user: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answer: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub availability: Option<Availability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferences: Option<ResponsePreferences>,
}

impl CreateResponseRequest {
    pub fn validate_preferences(&self) -> Result<(), AppError> {
        if let Some(ref prefs) = self.preferences
            && let Some(distance) = prefs.max_distance
            && distance < 0.0
        {
            return Err(AppError::BadRequest(
                "max_distance must be non-negative".to_string(),
            ));
        }
        if let Some(ref availability) = self.availability {
            availability.validate_times()?;
        }
        Ok(())
    }
}

impl Response {
    pub fn new(request: CreateResponseRequest) -> Self {
        Self {
            id: Uuid::new_v4(),
            user: request.user,
            answer: request.answer,
            availability: request.availability,
            preferences: request.preferences,
            updated_at: Utc::now(),
        }
    }

    pub fn update(&mut self, request: UpdateResponseRequest) {
        if let Some(answer) = request.answer {
            self.answer = answer;
        }
        if request.availability.is_some() {
            self.availability = request.availability;
        }
        if request.preferences.is_some() {
            self.preferences = request.preferences;
        }
        self.updated_at = Utc::now();
    }
}
