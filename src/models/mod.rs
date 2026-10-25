//! The entities of the domain and the request bodies of the API.
//!
//! | Module | Entities | Concept |
//! |---|---|---|
//! | [`user`] | [`User`] | A person, with optional preferences |
//! | [`group`] | [`Group`] | The people a ping is sent to, joined by invite code |
//! | [`ping`] | [`Ping`], [`PingLifecycle`] | One request to meet, and the state it is in |
//! | [`response`] | [`Response`] | One member's answer to a ping |
//! | [`hangout`] | [`HangoutData`], [`MatchResults`] | What the matching found and the hangout built on it |
//! | [`error`] | [`AppError`], [`ApiError`] | How a failure reaches the client |
//!
//! # Conventions
//!
//! - An entity is created from a `Create…Request` (`Entity::new`) and changed
//!   by an `Update…Request` (`Entity::update`). The server assigns every `id`
//!   (a random UUID v4); a client never chooses one.
//! - Entities refer to each other by id, never by nesting. Whether an id
//!   refers to an entity that exists is checked by the handler that uses it,
//!   not by the model.
//! - Every timestamp is UTC and is written in RFC 3339 (`2024-12-15T18:00:00Z`).
//! - The types derive [`utoipa::ToSchema`], so their doc comments are also the
//!   descriptions in the OpenAPI document (see [`crate::router`]).
//!
//! # Validation
//!
//! A request body goes through three checks, in this order, and the first to
//! fail ends the request with `400 Bad Request` (see [`error`]):
//!
//! 1. The shape: [`AppJson`] rejects a body that is not valid JSON of the
//!    expected type.
//! 2. The constraints on the fields: the `#[validate(...)]` attributes of the
//!    [`validator`] crate, run by the handler with `request.validate()?`.
//! 3. The rules that need code: [`Availability::validate_times`] and
//!    [`CreateResponseRequest::validate_preferences`].
//!
//! A rule that needs the state of the server, such as "the user is a member of
//! the group", is checked by the handler and answered with `403` or `404`.

pub mod error;
pub mod group;
pub mod hangout;
pub mod ping;
pub mod response;
pub mod user;

pub use error::{ApiError, AppError, AppJson};
pub use group::{
    CreateGroupRequest, Group, JoinGroupRequest, LeaveGroupRequest, RegenerateInviteRequest,
};
pub use hangout::{
    AttendeeStatus, ConfirmHangoutRequest, HangoutData, MatchResults, TimeOverlap, Timeline,
    UpdateAttendeeStatusRequest,
};
pub use ping::{CancelPingRequest, CreatePingRequest, Ping, PingLifecycle, TriggerMatchRequest};
pub use response::{
    Availability, CreateResponseRequest, Response, ResponsePreferences, UpdateResponseRequest,
};
pub use user::{CreateUserRequest, Location, UpdateUserRequest, User, UserPreferences};
