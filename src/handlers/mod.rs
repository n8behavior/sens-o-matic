//! One function per endpoint of the API.
//!
//! A handler is glue. It reads the request, asks the other modules what to do,
//! and writes the answer:
//!
//! 1. take the body as it is, because [`crate::models`] has refused a bad one
//!    already, when it was read (see the validation there);
//! 2. load the entities from the stores of [`crate::state`], and answer `404`
//!    if one is missing;
//! 3. change the entity in its store and return it. For a ping, the closure of the
//!    update runs the method of [`crate::models::AnyPing`] that checks the rules of
//!    the lifecycle and makes the transition, under the lock of the store, and the
//!    answer is `403` or `409` if a rule is broken (see [`crate::models::ping`]).
//!
//! The rule itself is never in a handler, with a few exceptions that need data
//! from two stores, such as "the user is a member of the group".
//!
//! # Documentation
//!
//! The doc comment of a handler says what the endpoint does and which rules it
//! applies. The status codes are in the `responses(...)` of its
//! `#[utoipa::path]` attribute, and are not repeated in the doc comment. Both
//! end up in the OpenAPI document (see [`crate::router`]), so the doc comments
//! are written for the reader of the API.
//!
//! The endpoints are grouped by the entity they work on:
//! [`users`], [`groups`], [`pings`] (the lifecycle, see [`crate::models::ping`])
//! and [`responses`].

pub mod groups;
pub mod pings;
pub mod responses;
pub mod users;
