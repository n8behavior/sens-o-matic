//! A member's answer to a ping.
//!
//! A [`Response`] says yes or no and, for a yes, when the member is free
//! ([`Availability`]). The answers of a ping are what [`crate::matching`]
//! works on. A member answers a ping once and can change the answer until the
//! initiator triggers the matching; after that the answers are frozen (see
//! [`crate::state_machine`]).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use super::{ResponseId, UserId};
use crate::models::error::AppError;

/// The window of time in which a member can join, both ends in UTC.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Availability {
    /// The earliest the member can start.
    pub earliest: DateTime<Utc>,
    /// The latest the member can stay until. Must be after `earliest`.
    pub latest: DateTime<Utc>,
}

impl Availability {
    /// Checks that the window is not empty, that is, `latest` is strictly after
    /// `earliest`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::BadRequest`] when `latest` is the same as or before
    /// `earliest`.
    ///
    /// # Examples
    ///
    /// ```
    /// use chrono::{TimeZone, Utc};
    /// use sens_o_matic::models::Availability;
    ///
    /// let at = |hour| Utc.with_ymd_and_hms(2024, 12, 15, hour, 0, 0).unwrap();
    ///
    /// assert!(Availability { earliest: at(17), latest: at(21) }.validate_times().is_ok());
    /// assert!(Availability { earliest: at(21), latest: at(17) }.validate_times().is_err());
    /// // A window of no length is empty too.
    /// assert!(Availability { earliest: at(17), latest: at(17) }.validate_times().is_err());
    /// ```
    pub fn validate_times(&self) -> Result<(), AppError> {
        if self.latest <= self.earliest {
            return Err(AppError::BadRequest(
                "latest must be after earliest".to_string(),
            ));
        }
        Ok(())
    }
}

/// What a member wants from the hangout. Every field is optional.
///
/// The server stores these and returns them; the matching ignores them.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Default)]
pub struct ResponsePreferences {
    /// How far the member will travel, in miles. Must not be negative.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_distance: Option<f64>,
    /// Names of the areas the member would like to meet in, free text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_areas: Option<Vec<String>>,
    /// Names of the areas the member does not want to meet in, free text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excluded_areas: Option<Vec<String>>,
}

/// The answer of one member to one ping.
///
/// A member has at most one response to a ping. The handler enforces it; the
/// type does not.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Response {
    /// Assigned by the server when the response is created.
    #[schema(value_type = String, format = "uuid")]
    pub id: ResponseId,
    /// The member who answered.
    #[schema(value_type = String, format = "uuid")]
    pub user: UserId,
    /// `true` for yes, `false` for no.
    pub answer: bool,
    /// When the member is free. A yes can come without it, but then the
    /// matching has nothing to place that member in, and ignores the answer.
    pub availability: Option<Availability>,
    /// What the member wants from the hangout.
    pub preferences: Option<ResponsePreferences>,
    /// When the response was created or last changed.
    pub updated_at: DateTime<Utc>,
}

/// The body of the request that answers a ping.
#[derive(Debug, Clone, Deserialize, ToSchema, Validate)]
pub struct CreateResponseRequest {
    /// The member who answers; must be a member of the group of the ping.
    #[schema(value_type = String, format = "uuid")]
    pub user: UserId,
    /// `true` for yes, `false` for no.
    pub answer: bool,
    /// When the member is free.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub availability: Option<Availability>,
    /// What the member wants from the hangout.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferences: Option<ResponsePreferences>,
}

/// The body of the request that changes an answer. A field that is left out
/// keeps its value.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct UpdateResponseRequest {
    /// The member who changes the response; must be the one who made it. The
    /// new values are not validated like those of a new response: a negative
    /// `max_distance` or an empty `availability` is accepted.
    #[schema(value_type = String, format = "uuid")]
    pub user: UserId,
    /// The new answer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answer: Option<bool>,
    /// The new availability. It replaces the old one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub availability: Option<Availability>,
    /// The new preferences. They replace the old ones as a whole.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferences: Option<ResponsePreferences>,
}

impl CreateResponseRequest {
    /// Checks the rules of the request that the `#[validate(...)]` attributes
    /// cannot express: `max_distance` is not negative, and the availability, if
    /// there is one, is a window ([`Availability::validate_times`]).
    ///
    /// # Errors
    ///
    /// Returns [`AppError::BadRequest`] with the rule that is broken.
    ///
    /// # Examples
    ///
    /// ```
    /// use sens_o_matic::models::{CreateResponseRequest, ResponsePreferences, UserId};
    ///
    /// let request = |max_distance| CreateResponseRequest {
    ///     user: UserId::random(),
    ///     answer: true,
    ///     availability: None,
    ///     preferences: Some(ResponsePreferences {
    ///         max_distance: Some(max_distance),
    ///         ..Default::default()
    ///     }),
    /// };
    ///
    /// assert!(request(0.5).validate_preferences().is_ok());
    /// assert!(request(-1.0).validate_preferences().is_err());
    /// ```
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
    /// Creates a response with a new random id, stamped with the current time.
    pub fn new(request: CreateResponseRequest) -> Self {
        Self {
            id: ResponseId::random(),
            user: request.user,
            answer: request.answer,
            availability: request.availability,
            preferences: request.preferences,
            updated_at: Utc::now(),
        }
    }

    /// Applies an update: a field that the request sets replaces the current
    /// value, and a field that it leaves out keeps it. `updated_at` is set to
    /// the current time in every case.
    ///
    /// Because a missing field means "keep", an update cannot clear the
    /// availability or the preferences, only replace them.
    ///
    /// # Examples
    ///
    /// ```
    /// use sens_o_matic::models::{CreateResponseRequest, Response, UpdateResponseRequest, UserId};
    ///
    /// let user = UserId::random();
    /// let mut response = Response::new(CreateResponseRequest {
    ///     user,
    ///     answer: true,
    ///     availability: None,
    ///     preferences: None,
    /// });
    ///
    /// response.update(UpdateResponseRequest {
    ///     user,
    ///     answer: Some(false),
    ///     availability: None,
    ///     preferences: None,
    /// });
    ///
    /// assert!(!response.answer);
    /// ```
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
