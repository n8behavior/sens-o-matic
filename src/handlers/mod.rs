//! One function per endpoint of the API.
//!
//! A handler is glue. It reads the request, asks the other modules what to do,
//! and writes the answer:
//!
//! 1. validate the body (see the validation in [`crate::models`]);
//! 2. load the entities from the stores of [`crate::state`], and answer `404`
//!    if one is missing;
//! 3. check the rules of the ping with [`crate::state_machine`], and answer
//!    `403` or `409` if one is broken;
//! 4. change the entity in its store and return it.
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
//! [`users`], [`groups`], [`pings`] (the lifecycle, see [`crate::state_machine`])
//! and [`responses`].

pub mod groups;
pub mod pings;
pub mod responses;
pub mod users;
