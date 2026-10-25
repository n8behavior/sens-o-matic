//! The entities of the domain and the request bodies of the API.
//!
//! | Module | Entities | Concept |
//! |---|---|---|
//! | [`id`] | [`Id`], [`UserId`], [`GroupId`], [`PingId`], [`ResponseId`] | The id of an entity, one type for each kind |
//! | [`value`] | [`BoundedText`], [`InviteCode`], [`Distance`], [`Latitude`], [`Longitude`], [`TimeWindow`] | A value that can only be valid |
//! | [`user`] | [`User`] | A person, with optional preferences |
//! | [`group`] | [`Group`], [`Members`] | The people a ping is sent to, joined by invite code |
//! | [`ping`] | [`Ping`], [`AnyPing`], [`PingState`] | One request to meet, the state it is in, and the rules of its lifecycle |
//! | [`response`] | [`Response`] | One member's answer to a ping |
//! | [`hangout`] | [`HangoutData`], [`Attendees`], [`MatchResults`] | What the matching found and the hangout built on it |
//! | [`error`] | [`AppError`], [`ApiError`] | How a failure reaches the client |
//!
//! # Conventions
//!
//! - An entity is created from a `Create…Request` (`Entity::new`) and changed
//!   by an `Update…Request` (`Entity::update`). The server assigns every `id`
//!   (see [`id`]); a client never chooses one.
//! - Entities refer to each other by id, never by nesting, and an id has the
//!   type of the entity it names, so a [`GroupId`] cannot stand where a
//!   [`UserId`] is expected. Whether an id refers to an entity that exists is
//!   checked by the handler that uses it, not by the model.
//! - A fact is stored once. A value that can be worked out from another is a
//!   method, not a field ([`MatchResults::has_match`]); a list of users and
//!   something about each of them is one structure ([`Attendees`]); a list used
//!   as a set cannot hold a user twice ([`Members`]). Where the JSON says a fact
//!   twice, the model still stores it once and the JSON is made at the edge,
//!   with the same fields (see [`hangout`]).
//! - Every timestamp is UTC and is written in RFC 3339 (`2024-12-15T18:00:00Z`).
//! - The types derive [`utoipa::ToSchema`], so their doc comments are also the
//!   descriptions in the OpenAPI document (see [`crate::router`]).
//!
//! # Validation
//!
//! A request body is checked once, when it is read, and a handler validates
//! nothing. A field that has a rule has a type that can only hold a valid
//! value, and its [`Deserialize`](serde::Deserialize) goes through the
//! constructor of the type (see [`value`]). A body that breaks a rule, or is
//! not valid JSON of the expected type, is refused by [`AppJson`] with `400
//! Bad Request` (see [`error`]) before the handler runs. The create and the
//! update of an entity use the same types, so they keep the same rules.
//!
//! A rule that needs the state of the server, such as "the user is a member of
//! the group", is checked by the handler and answered with `403` or `404`.
//!
//! # The model and the JSON
//!
//! Where the JSON is not the shape of the model ([`MatchResults`], [`HangoutData`]
//! and [`AnyPing`]), a private *wire* type has the fields as they are in the
//! JSON. serde converts to it and from it, and the model takes its OpenAPI
//! schema from it with `schema_of_wire!`, so the document describes the JSON
//! that serde writes.

/// Gives `$model` the OpenAPI schema of `$wire`: the schema of the JSON, which
/// is not the shape of the model (see the [module docs](self)).
///
/// `$wire` derives `ToSchema` with `#[schema(as = ...)]`, so its name is the
/// name of `$model`, and a reference to `$model` points at the schema of
/// `$wire`.
macro_rules! schema_of_wire {
    ($model:ty => $wire:ty) => {
        impl ::utoipa::PartialSchema for $model {
            fn schema() -> ::utoipa::openapi::RefOr<::utoipa::openapi::Schema> {
                <$wire as ::utoipa::PartialSchema>::schema()
            }
        }

        impl ::utoipa::ToSchema for $model {
            fn name() -> ::std::borrow::Cow<'static, str> {
                <$wire as ::utoipa::ToSchema>::name()
            }

            fn schemas(
                schemas: &mut Vec<(String, ::utoipa::openapi::RefOr<::utoipa::openapi::Schema>)>,
            ) {
                <$wire as ::utoipa::ToSchema>::schemas(schemas)
            }
        }
    };
}

pub mod error;
pub mod group;
pub mod hangout;
pub mod id;
pub mod ping;
pub mod response;
pub mod user;
pub mod value;

pub use error::{ApiError, AppError, AppJson};
pub use group::{
    CreateGroupRequest, Group, JoinGroupRequest, LeaveGroupRequest, Members,
    RegenerateInviteRequest,
};
pub use hangout::{
    AttendeeStatus, Attendees, ConfirmHangoutRequest, HangoutData, MatchResults, TimeOverlap,
    Timeline, UpdateAttendeeStatusRequest,
};
pub use id::{GroupId, Id, PingId, ResponseId, UserId};
pub use ping::{
    ActiveHangout, AlreadyResponded, AnyPing, CancelPingRequest, Cancelled, Complete,
    CreatePingRequest, Gathering, MatchOutcome, Matching, NoMatch, Ping, PingSent, PingState,
    TimelineRefused, TriggerMatchRequest, UnknownState, UpdateResponseError, VenueConfirmed,
};
pub use response::{
    Availability, CreateResponseRequest, Response, ResponsePreferences, UpdateResponseRequest,
};
pub use user::{CreateUserRequest, Location, UpdateUserRequest, User, UserPreferences};
pub use value::{
    ActivityType, BoundedText, Distance, InviteCode, Latitude, Longitude, Name, RoughTiming,
    TimeWindow, Vibe,
};
