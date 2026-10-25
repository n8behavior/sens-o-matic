//! A person who uses the service.
//!
//! A [`User`] has a name, and optionally an avatar and preferences. There is no
//! authentication: a user is an id that the client sends along, and the server
//! does not check who sends it. A user belongs to groups through
//! [`Group::members`](super::group::Group::members), not through a field of
//! its own.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::UserId;
use super::value::{Distance, Latitude, Longitude, Name};

/// A point on the earth, in decimal degrees: `lat` is in `-90..=90` and `lng`
/// in `-180..=180`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Location {
    /// Degrees north of the equator; negative is south.
    #[schema(value_type = f64, minimum = -90, maximum = 90)]
    pub lat: Latitude,
    /// Degrees east of the prime meridian; negative is west.
    #[schema(value_type = f64, minimum = -180, maximum = 180)]
    pub lng: Longitude,
}

/// What a user usually wants. Every field is optional.
///
/// The server stores these and returns them; nothing in the matching uses
/// them yet.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Default)]
pub struct UserPreferences {
    /// How far the user usually travels, in miles. Must not be negative.
    #[schema(value_type = Option<f64>, minimum = 0)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_distance: Option<Distance>,
    /// Names of the areas the user likes to meet in, free text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub favorite_areas: Option<Vec<String>>,
    /// Where the user lives, to estimate how long they need to arrive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub home_location: Option<Location>,
}

/// A person who can join groups, send pings and answer them.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct User {
    /// Assigned by the server when the user is created.
    #[schema(value_type = String, format = "uuid")]
    pub id: UserId,
    /// Display name, 1 to 100 characters.
    #[schema(value_type = String, min_length = 1, max_length = 100)]
    pub name: Name,
    /// The avatar of the user, as the client wrote it (for example a URL). The
    /// server does not interpret it.
    pub avatar: Option<String>,
    /// The preferences of the user, when they set some.
    pub preferences: Option<UserPreferences>,
}

/// The body of the request that creates a user.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct CreateUserRequest {
    /// Display name, 1 to 100 characters.
    #[schema(value_type = String, min_length = 1, max_length = 100)]
    pub name: Name,
    /// The avatar, as a string. The server does not interpret it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    /// The preferences of the user.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferences: Option<UserPreferences>,
}

/// The body of the request that changes a user. Every field is optional, and a
/// field that is left out keeps its value. The avatar and the preferences can be
/// replaced but not cleared.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct UpdateUserRequest {
    /// The new display name, 1 to 100 characters.
    #[schema(value_type = Option<String>, min_length = 1, max_length = 100)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<Name>,
    /// The new avatar.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    /// The new preferences. They replace the old ones as a whole; they are
    /// not merged field by field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferences: Option<UserPreferences>,
}

impl User {
    /// Creates a user with a new random id. The request holds only valid
    /// values, so this cannot fail.
    ///
    /// # Examples
    ///
    /// ```
    /// use sens_o_matic::models::{CreateUserRequest, User};
    ///
    /// let request = CreateUserRequest {
    ///     name: "Ada".parse().unwrap(),
    ///     avatar: None,
    ///     preferences: None,
    /// };
    /// let user = User::new(request);
    /// assert_eq!(user.name.as_str(), "Ada");
    ///
    /// // An empty name is refused where the body is read, before `User::new`.
    /// assert!(serde_json::from_str::<CreateUserRequest>(r#"{ "name": "" }"#).is_err());
    /// ```
    pub fn new(request: CreateUserRequest) -> Self {
        Self {
            id: UserId::random(),
            name: request.name,
            avatar: request.avatar,
            preferences: request.preferences,
        }
    }

    /// Applies an update: a field that the request sets replaces the current
    /// value, and a field that it leaves out keeps it.
    ///
    /// Because a missing field means "keep", an update cannot clear the avatar
    /// or the preferences, only replace them.
    ///
    /// # Examples
    ///
    /// ```
    /// use sens_o_matic::models::{CreateUserRequest, UpdateUserRequest, User};
    ///
    /// let mut user = User::new(CreateUserRequest {
    ///     name: "Ada".parse().unwrap(),
    ///     avatar: Some("https://example.com/ada.png".to_string()),
    ///     preferences: None,
    /// });
    ///
    /// user.update(UpdateUserRequest {
    ///     name: Some("Ada Lovelace".parse().unwrap()),
    ///     avatar: None,
    ///     preferences: None,
    /// });
    ///
    /// assert_eq!(user.name.as_str(), "Ada Lovelace");
    /// assert_eq!(user.avatar.as_deref(), Some("https://example.com/ada.png"));
    /// ```
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
