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

use super::value::{Distance, TimeWindow, TimeWindowError};
use super::{ResponseId, UserId};

/// The window of time in which a member can join, both ends in UTC.
///
/// A window has a length: `latest` is after `earliest`. The type cannot hold
/// anything else, so a body that breaks the rule is refused when it is read,
/// whether it creates a response or changes one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(try_from = "AvailabilityWire")]
pub struct Availability {
    /// The earliest the member can start.
    earliest: DateTime<Utc>,
    /// The latest the member can stay until. Must be after `earliest`.
    latest: DateTime<Utc>,
}

/// The shape of an [`Availability`] on the wire, before the rule is checked.
#[derive(Deserialize)]
struct AvailabilityWire {
    earliest: DateTime<Utc>,
    latest: DateTime<Utc>,
}

impl Availability {
    /// Makes the availability from the earliest and the latest time.
    ///
    /// # Errors
    ///
    /// Returns a [`TimeWindowError`] when `latest` is the same as `earliest`
    /// or before it.
    ///
    /// # Examples
    ///
    /// ```
    /// use chrono::{TimeZone, Utc};
    /// use sens_o_matic::models::Availability;
    ///
    /// let at = |hour| Utc.with_ymd_and_hms(2024, 12, 15, hour, 0, 0).unwrap();
    ///
    /// let availability = Availability::new(at(17), at(21)).unwrap();
    /// assert_eq!((availability.earliest(), availability.latest()), (at(17), at(21)));
    ///
    /// // `latest` before `earliest`, and a window of no length.
    /// assert!(Availability::new(at(21), at(17)).is_err());
    /// assert!(Availability::new(at(17), at(17)).is_err());
    ///
    /// // A body is refused where it is read.
    /// let body = |earliest, latest| {
    ///     format!(r#"{{ "earliest": "{earliest}", "latest": "{latest}" }}"#)
    /// };
    /// let json = body("2024-12-15T17:00:00Z", "2024-12-15T21:00:00Z");
    /// assert!(serde_json::from_str::<Availability>(&json).is_ok());
    /// let json = body("2024-12-15T21:00:00Z", "2024-12-15T17:00:00Z");
    /// assert!(serde_json::from_str::<Availability>(&json).is_err());
    /// ```
    pub fn new(earliest: DateTime<Utc>, latest: DateTime<Utc>) -> Result<Self, TimeWindowError> {
        TimeWindow::new(earliest, latest).map(Self::from)
    }

    /// The earliest the member can start.
    pub fn earliest(&self) -> DateTime<Utc> {
        self.earliest
    }

    /// The latest the member can stay until; always after
    /// [`Availability::earliest`].
    pub fn latest(&self) -> DateTime<Utc> {
        self.latest
    }
}

impl From<TimeWindow> for Availability {
    fn from(window: TimeWindow) -> Self {
        Self {
            earliest: window.start(),
            latest: window.end(),
        }
    }
}

impl TryFrom<AvailabilityWire> for Availability {
    type Error = TimeWindowError;

    fn try_from(wire: AvailabilityWire) -> Result<Self, Self::Error> {
        Self::new(wire.earliest, wire.latest)
    }
}

/// What a member wants from the hangout. Every field is optional.
///
/// The server stores these and returns them; the matching ignores them.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Default)]
pub struct ResponsePreferences {
    /// How far the member will travel, in miles. Must not be negative.
    #[schema(value_type = Option<f64>, minimum = 0)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_distance: Option<Distance>,
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
#[derive(Debug, Clone, Deserialize, ToSchema)]
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
/// keeps its value. The new values follow the rules of a new response: a
/// negative `max_distance`, or an `availability` whose `latest` is not after its
/// `earliest`, is refused.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct UpdateResponseRequest {
    /// The member who changes the response; must be the one who made it.
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
