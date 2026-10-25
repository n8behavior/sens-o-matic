//! Backend API server for coordinating spontaneous group meetups.
//!
//! A user sends a *ping* to a group ("who's up for drinks tonight?"). Members
//! answer yes or no and say when they are free. The server computes the time
//! window in which the yes-sayers overlap, the initiator confirms a hangout
//! for that window, and the attendees report on their way there. The binary
//! (`src/main.rs`) only binds a port and serves [`create_router`].
//!
//! # Map of the modules
//!
//! - [`models`]: the entities and the request bodies, with their validation.
//!   [`models::Ping`] is the central type: a ping in a state, with the rules of
//!   the lifecycle, and [`models::AnyPing`] is a ping in any state.
//! - [`matching`]: the overlap algorithm that decides whether a ping has a
//!   match.
//! - [`state`]: the in-memory stores that hold the entities.
//! - [`handlers`]: one function per endpoint; they combine the modules above.
//! - [`router`]: the route table, the `/health` check and the OpenAPI document.
//!
//! # Where the lifecycle of a ping is documented
//!
//! The states, the rules for each transition and the walk through a whole lifecycle
//! are in [`models::ping`], with a doc test that runs it.
//!
//! # These docs are also the API docs
//!
//! The doc comments of the schema types in [`models`] and of the handlers in
//! [`handlers`] become the descriptions in the OpenAPI document; see
//! [`router`].

pub mod handlers;
pub mod matching;
pub mod models;
pub mod router;
pub mod state;

pub use router::create_router;
pub use state::AppState;
