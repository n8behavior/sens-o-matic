//! A request to meet, and the lifecycle it goes through.
//!
//! A [`Ping`] is what a member sends to a group: an activity, a rough timing
//! and an optional vibe. From then on it goes through the states of the
//! lifecycle below, gathers the [`Response`]s of the members, and ends as a
//! hangout, a cancellation or a miss. This module holds the states, the rules
//! of who may do what in which state, and the transitions.
//!
//! # The lifecycle
//!
//! ```text
//! PingSent ──first response──▶ Gathering ──match found──▶ Matching ──confirm──▶ VenueConfirmed
//!                                  │                                                  │
//!                                  │ no match                                         │ activate
//!                                  ▼                                                  ▼
//!                               NoMatch                  Complete ◀──complete── ActiveHangout
//!
//! Cancelled: from any state that is not terminal.
//! Terminal states: Complete, Cancelled, NoMatch.
//! ```
//!
//! | State | Answers can be added | Carries besides the answers |
//! |---|---|---|
//! | [`PingSent`] | yes | – |
//! | [`Gathering`] | yes | – |
//! | [`Matching`] | no | the [`MatchResults`] |
//! | [`VenueConfirmed`], [`ActiveHangout`], [`Complete`] | no | the [`HangoutData`] |
//! | [`Cancelled`], [`NoMatch`] | no | – |
//!
//! # One type for each state
//!
//! A ping in a state is a type of its own: `Ping<Gathering>`, `Ping<Matching>`
//! and so on. The fields that every ping has are in [`Ping`] once, and the data
//! of a state is in the type of that state, so a ping that was not confirmed
//! has no hangout to read or to change, and the type says so.
//!
//! A transition is a method that exists only on the state it leaves. It takes
//! the ping by value and returns the ping in the next state (or a
//! [`MatchOutcome`], when the next state depends on a result). Calling it from
//! another state **does not compile**; the `compile_fail` examples of
//! [`Ping::<Matching>::confirm`](Ping::confirm), [`Ping::cancel`],
//! [`Ping::set_attendee_status`] and [`Ping::add_response`] show it. Because a
//! transition takes the ping, the old one is gone after the call, so it cannot
//! be moved on twice either; the example of [`Ping::activate`] shows that.
//!
//! The transitions are:
//!
//! | From | Method | To |
//! |---|---|---|
//! | [`PingSent`] | [`Ping::respond`] | [`Gathering`] |
//! | [`Gathering`] | [`Ping::match_responses`] | [`Matching`] or [`NoMatch`] |
//! | [`Matching`] | [`Ping::confirm`] | [`VenueConfirmed`] |
//! | [`VenueConfirmed`] | [`Ping::activate`] | [`ActiveHangout`] |
//! | [`ActiveHangout`] | [`Ping::complete`] | [`Complete`] |
//! | any [`Open`] state | [`Ping::cancel`] | [`Cancelled`] |
//!
//! # The ping in the store
//!
//! The stores hold pings of every state, so what they hold is [`AnyPing`]: one
//! enum with a variant for each `Ping<S>`. It is also what serde writes and
//! reads, in the JSON that the API has always had: the `state` and the data of
//! the state are at the top level, next to the other fields.
//!
//! [`AnyPing`] is the one place where "the ping is in state X and the action
//! needs Y" becomes an error. Each of its action methods ([`AnyPing::cancel`],
//! [`AnyPing::trigger_match`], …) checks who acts (the initiator, where the
//! action is the initiator's: [`AppError::Forbidden`]) and then the state
//! ([`AppError::Conflict`]), and makes the transition, in one call on one
//! value. The handler calls it inside the closure of
//! [`InMemoryStore::update`](crate::state::InMemoryStore::update), which runs
//! under the lock of the store, so the check and the transition cannot be
//! separated by another request. Two requests that arrive together cannot both
//! pass the check: the second finds the ping in the next state and gets the
//! conflict.
//!
//! The message of a conflict names the action, the state the ping is in, the
//! states that allow the action, and this module, where the lifecycle is
//! described:
//!
//! ```text
//! Cannot activate the hangout: the ping is in the gathering state, and this needs the
//! venue_confirmed state. See sens_o_matic::models::ping for the lifecycle.
//! ```
//!
//! [`crate::matching`] decides whether [`Gathering`] ends in [`Matching`] or in
//! [`NoMatch`]; the caller passes it in, so that this module does not depend on
//! it.
//!
//! # A lifecycle from end to end
//!
//! The doc test below sends a ping through every state of the happy path, with
//! the modules that take part: this one for the ping and its states,
//! [`crate::matching`] for the overlap, and [`crate::models`] for the answers.
//! At the end it hands the ping to the store's type, [`AnyPing`], which refuses
//! what the lifecycle does not allow.
//!
//! ```
//! use chrono::{TimeZone, Utc};
//! use sens_o_matic::matching::MatchingEngine;
//! use sens_o_matic::models::{
//!     AnyPing, AppError, Availability, CreatePingRequest, CreateResponseRequest, GroupId,
//!     MatchOutcome, Ping, PingState, Response, Timeline, UserId,
//! };
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let initiator = UserId::random();
//! let friend = UserId::random();
//! let at = |hour| Utc.with_ymd_and_hms(2024, 12, 15, hour, 0, 0).unwrap();
//!
//! // PingSent: the initiator sends the ping.
//! let ping = Ping::new(CreatePingRequest {
//!     initiator,
//!     group: GroupId::random(),
//!     activity_type: "drinks".parse()?,
//!     rough_timing: "tonight".parse()?,
//!     vibe: None,
//! });
//!
//! // Gathering: a friend says yes, and when they are free.
//! let mut ping = ping.respond(Response::new(CreateResponseRequest {
//!     user: friend,
//!     answer: true,
//!     availability: Some(Availability::new(at(18), at(21))?),
//!     preferences: None,
//! }));
//! assert_eq!(ping.responses().len(), 1);
//!
//! // A member answers once.
//! let again = Response::new(CreateResponseRequest {
//!     user: friend,
//!     answer: false,
//!     availability: None,
//!     preferences: None,
//! });
//! assert!(ping.add_response(again).is_err());
//!
//! // Matching: the answers share a time, so the ping moves on.
//! let MatchOutcome::Matching(ping) = ping.match_responses(MatchingEngine::calculate_match) else {
//!     panic!("the only answer is a yes with a time");
//! };
//! assert!(ping.match_results().has_match());
//!
//! // VenueConfirmed: the initiator confirms the hangout for the common window.
//! let ping = ping.confirm(Timeline::new(at(18), at(21))?);
//!
//! // ActiveHangout, then Complete.
//! let ping = ping.activate();
//! let ping = ping.complete();
//!
//! // Complete is terminal: `ping.cancel()` does not compile (see `Ping::cancel`).
//! // The store holds the ping as an `AnyPing`, which says so with a conflict.
//! let mut stored = AnyPing::from(ping);
//! assert_eq!(stored.state(), PingState::Complete);
//! assert!(matches!(stored.cancel(initiator), Err(AppError::Conflict(_))));
//! # Ok(())
//! # }
//! ```

use std::{fmt, str::FromStr};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::error::AppError;
use super::hangout::{AttendeeStatus, HangoutData, MatchResults, NotAnAttendee, Timeline};
use super::response::{Response, UpdateResponseRequest};
use super::value::{ActivityType, RoughTiming, Vibe};
use super::{GroupId, PingId, ResponseId, UserId};

/// The name of a state of a ping.
///
/// It is the `state` field of the JSON of a ping, in `snake_case` (see
/// [`PingState::as_str`]), and the `state` filter of the list of pings of a
/// group. It names a state; the data of the state is in the types of
/// [`State`].
///
/// # Examples
///
/// ```
/// use sens_o_matic::models::PingState;
///
/// assert_eq!(PingState::NoMatch.to_string(), "no_match");
/// assert_eq!("no_match".parse::<PingState>(), Ok(PingState::NoMatch));
/// assert!("finished".parse::<PingState>().is_err());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PingState {
    /// The ping was sent and nobody has answered yet.
    PingSent,
    /// Members are answering.
    Gathering,
    /// The answers have a time in common.
    Matching,
    /// The initiator confirmed the hangout.
    VenueConfirmed,
    /// The hangout is under way.
    ActiveHangout,
    /// The hangout is over.
    Complete,
    /// The initiator cancelled the ping.
    Cancelled,
    /// The matching found no time in common.
    NoMatch,
}

impl PingState {
    /// Every state, in the order of the lifecycle.
    pub const ALL: [PingState; 8] = [
        PingState::PingSent,
        PingState::Gathering,
        PingState::Matching,
        PingState::VenueConfirmed,
        PingState::ActiveHangout,
        PingState::Complete,
        PingState::Cancelled,
        PingState::NoMatch,
    ];

    /// The name of the state in JSON and in the `state` filter.
    pub const fn as_str(self) -> &'static str {
        match self {
            PingState::PingSent => "ping_sent",
            PingState::Gathering => "gathering",
            PingState::Matching => "matching",
            PingState::VenueConfirmed => "venue_confirmed",
            PingState::ActiveHangout => "active_hangout",
            PingState::Complete => "complete",
            PingState::Cancelled => "cancelled",
            PingState::NoMatch => "no_match",
        }
    }
}

impl fmt::Display for PingState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The text is not the name of a state of a ping. See [`PingState`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "{0:?} is not the name of a state of a ping. See sens_o_matic::models::ping for the states"
)]
pub struct UnknownState(pub String);

impl FromStr for PingState {
    type Err = UnknownState;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|state| state.as_str() == name)
            .ok_or_else(|| UnknownState(name.to_string()))
    }
}

/// A state a ping can be in, and the data that belongs to it.
///
/// The states are the types [`PingSent`], [`Gathering`], [`Matching`],
/// [`VenueConfirmed`], [`ActiveHangout`], [`Complete`], [`Cancelled`] and
/// [`NoMatch`]; they are the parameter of [`Ping`]. The lifecycle is in the
/// [module docs](self).
pub trait State {
    /// The name of this state.
    const KIND: PingState;

    /// The answers of the ping in this state; empty in [`PingSent`].
    fn responses(&self) -> &[Response];
}

/// A state in which the ping can still be cancelled, that is, one that is not
/// terminal: every state except [`Complete`], [`Cancelled`] and [`NoMatch`].
pub trait Open: State {
    /// The answers of the ping, to carry them into [`Cancelled`].
    fn into_responses(self) -> Vec<Response>;
}

/// A state that has a hangout: [`VenueConfirmed`], [`ActiveHangout`] and
/// [`Complete`].
pub trait HasHangout: State {
    /// The hangout.
    fn hangout(&self) -> &HangoutData;

    /// The hangout, to change it.
    fn hangout_mut(&mut self) -> &mut HangoutData;
}

/// The ping was sent and nobody has answered yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PingSent;

/// Members are answering. The initiator ends this state by triggering the
/// matching.
#[derive(Debug, Clone)]
pub struct Gathering {
    responses: Vec<Response>,
}

/// The answers have a time in common. The initiator picks the hangout.
#[derive(Debug, Clone)]
pub struct Matching {
    responses: Vec<Response>,
    match_results: MatchResults,
}

/// The initiator confirmed the hangout; it has not started.
#[derive(Debug, Clone)]
pub struct VenueConfirmed {
    responses: Vec<Response>,
    hangout: HangoutData,
}

/// The hangout is under way.
#[derive(Debug, Clone)]
pub struct ActiveHangout {
    responses: Vec<Response>,
    hangout: HangoutData,
}

/// The hangout is over. Terminal.
#[derive(Debug, Clone)]
pub struct Complete {
    responses: Vec<Response>,
    hangout: HangoutData,
}

/// The initiator cancelled the ping. Terminal.
#[derive(Debug, Clone)]
pub struct Cancelled {
    responses: Vec<Response>,
}

/// The matching found no time in common. Terminal.
#[derive(Debug, Clone)]
pub struct NoMatch {
    responses: Vec<Response>,
}

impl State for PingSent {
    const KIND: PingState = PingState::PingSent;

    fn responses(&self) -> &[Response] {
        &[]
    }
}

impl State for Gathering {
    const KIND: PingState = PingState::Gathering;

    fn responses(&self) -> &[Response] {
        &self.responses
    }
}

impl State for Matching {
    const KIND: PingState = PingState::Matching;

    fn responses(&self) -> &[Response] {
        &self.responses
    }
}

impl State for VenueConfirmed {
    const KIND: PingState = PingState::VenueConfirmed;

    fn responses(&self) -> &[Response] {
        &self.responses
    }
}

impl State for ActiveHangout {
    const KIND: PingState = PingState::ActiveHangout;

    fn responses(&self) -> &[Response] {
        &self.responses
    }
}

impl State for Complete {
    const KIND: PingState = PingState::Complete;

    fn responses(&self) -> &[Response] {
        &self.responses
    }
}

impl State for Cancelled {
    const KIND: PingState = PingState::Cancelled;

    fn responses(&self) -> &[Response] {
        &self.responses
    }
}

impl State for NoMatch {
    const KIND: PingState = PingState::NoMatch;

    fn responses(&self) -> &[Response] {
        &self.responses
    }
}

impl Open for PingSent {
    fn into_responses(self) -> Vec<Response> {
        Vec::new()
    }
}

impl Open for Gathering {
    fn into_responses(self) -> Vec<Response> {
        self.responses
    }
}

impl Open for Matching {
    fn into_responses(self) -> Vec<Response> {
        self.responses
    }
}

impl Open for VenueConfirmed {
    fn into_responses(self) -> Vec<Response> {
        self.responses
    }
}

impl Open for ActiveHangout {
    fn into_responses(self) -> Vec<Response> {
        self.responses
    }
}

impl HasHangout for VenueConfirmed {
    fn hangout(&self) -> &HangoutData {
        &self.hangout
    }

    fn hangout_mut(&mut self) -> &mut HangoutData {
        &mut self.hangout
    }
}

impl HasHangout for ActiveHangout {
    fn hangout(&self) -> &HangoutData {
        &self.hangout
    }

    fn hangout_mut(&mut self) -> &mut HangoutData {
        &mut self.hangout
    }
}

impl HasHangout for Complete {
    fn hangout(&self) -> &HangoutData {
        &self.hangout
    }

    fn hangout_mut(&mut self) -> &mut HangoutData {
        &mut self.hangout
    }
}

/// A request from a member to the group to meet, in the state `S`.
///
/// The fields here are the ones that every ping has, whatever its state; the
/// data of the state is in `state`. The lifecycle, and the methods that move a
/// ping from one state to the next, are in the [module docs](self).
///
/// A `Ping<S>` is one state of a ping. The stores hold [`AnyPing`], which is
/// any of them.
#[derive(Debug, Clone)]
pub struct Ping<S> {
    /// Assigned by the server when the ping is created. It is also the id of
    /// the hangout, if the ping gets that far.
    pub id: PingId,
    /// The member who sent the ping. Only they can trigger the matching and
    /// cancel the ping.
    pub initiator: UserId,
    /// The group the ping was sent to; only its members can answer.
    pub group: GroupId,
    /// What to do, in the words of the initiator ("drinks", "dinner").
    pub activity_type: ActivityType,
    /// When, roughly ("tonight", "this afternoon"). Free text: the matching
    /// works on the availabilities of the answers, not on this.
    pub rough_timing: RoughTiming,
    /// The mood of the meetup ("chill").
    pub vibe: Option<Vibe>,
    /// When the ping was sent.
    pub created_at: DateTime<Utc>,
    /// The state of the ping and the data that belongs to it.
    pub state: S,
}

impl<S> Ping<S> {
    /// Takes the state out of the ping, and returns the fields that every
    /// ping has, without a state, next to it.
    fn take_state(self) -> (Ping<()>, S) {
        let ping = Ping {
            id: self.id,
            initiator: self.initiator,
            group: self.group,
            activity_type: self.activity_type,
            rough_timing: self.rough_timing,
            vibe: self.vibe,
            created_at: self.created_at,
            state: (),
        };
        (ping, self.state)
    }
}

impl Ping<()> {
    /// Puts a state on the fields that every ping has. This and
    /// [`Ping::take_state`] are the only places that carry those fields from
    /// one state to the next.
    fn with_state<S>(self, state: S) -> Ping<S> {
        Ping {
            id: self.id,
            initiator: self.initiator,
            group: self.group,
            activity_type: self.activity_type,
            rough_timing: self.rough_timing,
            vibe: self.vibe,
            created_at: self.created_at,
            state,
        }
    }
}

impl<S: State> Ping<S> {
    /// The answers of the ping; empty in [`PingSent`].
    pub fn responses(&self) -> &[Response] {
        self.state.responses()
    }

    /// The answers that say yes (`answer == true`), whether or not they give an
    /// availability.
    pub fn positive_responses(&self) -> Vec<&Response> {
        self.responses().iter().filter(|r| r.answer).collect()
    }
}

/// The body of the request that sends a ping.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct CreatePingRequest {
    /// The member who sends the ping; must be a member of `group`.
    #[schema(value_type = String, format = "uuid")]
    pub initiator: UserId,
    /// The group to send the ping to.
    #[schema(value_type = String, format = "uuid")]
    pub group: GroupId,
    /// What to do, 1 to 50 characters.
    #[schema(value_type = String, min_length = 1, max_length = 50)]
    pub activity_type: ActivityType,
    /// When, roughly, 1 to 50 characters.
    #[schema(value_type = String, min_length = 1, max_length = 50)]
    pub rough_timing: RoughTiming,
    /// The mood of the meetup, up to 100 characters.
    #[schema(value_type = Option<String>, max_length = 100)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vibe: Option<Vibe>,
}

/// The body of the request that cancels a ping.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct CancelPingRequest {
    /// The user who cancels; must be the initiator of the ping.
    #[schema(value_type = String, format = "uuid")]
    pub user_id: UserId,
}

/// The body of the request that ends the gathering of answers and runs the
/// matching.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct TriggerMatchRequest {
    /// The user who triggers the matching; must be the initiator of the ping.
    #[schema(value_type = String, format = "uuid")]
    pub user_id: UserId,
}

impl Ping<PingSent> {
    /// Creates a ping in the [`PingSent`] state, with a new random id, stamped
    /// with the current time. The request holds only valid values, so this
    /// cannot fail.
    ///
    /// # Examples
    ///
    /// The lengths that [`CreatePingRequest`] allows count characters, not
    /// bytes, and a body that breaks them is refused where it is read:
    ///
    /// ```
    /// use sens_o_matic::models::{CreatePingRequest, GroupId, Ping, UserId};
    ///
    /// let body = |activity_type: &str| {
    ///     serde_json::json!({
    ///         "initiator": UserId::random(),
    ///         "group": GroupId::random(),
    ///         "activity_type": activity_type,
    ///         "rough_timing": "tonight",
    ///     })
    /// };
    /// let parse = |activity_type: &str| serde_json::from_value::<CreatePingRequest>(body(activity_type));
    ///
    /// assert!(parse("").is_err());
    /// assert!(parse(&"é".repeat(51)).is_err());
    ///
    /// let valid = parse(&"é".repeat(50)).unwrap(); // 50 characters, 100 bytes
    /// let ping = Ping::new(valid);
    /// assert!(ping.responses().is_empty());
    /// ```
    pub fn new(request: CreatePingRequest) -> Self {
        Self {
            id: PingId::random(),
            initiator: request.initiator,
            group: request.group,
            activity_type: request.activity_type,
            rough_timing: request.rough_timing,
            vibe: request.vibe,
            created_at: Utc::now(),
            state: PingSent,
        }
    }

    /// The first answer: moves the ping to [`Gathering`], whatever the answer
    /// is.
    ///
    /// # Examples
    ///
    /// ```
    /// use sens_o_matic::models::{
    ///     CreatePingRequest, CreateResponseRequest, GroupId, Ping, Response, UserId,
    /// };
    ///
    /// let ping = Ping::new(CreatePingRequest {
    ///     initiator: UserId::random(),
    ///     group: GroupId::random(),
    ///     activity_type: "drinks".parse().unwrap(),
    ///     rough_timing: "tonight".parse().unwrap(),
    ///     vibe: None,
    /// });
    ///
    /// let ping = ping.respond(Response::new(CreateResponseRequest {
    ///     user: UserId::random(),
    ///     answer: false,
    ///     availability: None,
    ///     preferences: None,
    /// }));
    /// assert_eq!(ping.responses().len(), 1);
    /// ```
    pub fn respond(self, response: Response) -> Ping<Gathering> {
        let (ping, PingSent) = self.take_state();
        ping.with_state(Gathering {
            responses: vec![response],
        })
    }
}

/// The member has answered the ping already. A member answers once; they can
/// change the answer instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "User {0} has already responded to this ping; a member answers once and can change the \
     answer with an update. See sens_o_matic::models::ping"
)]
pub struct AlreadyResponded(pub UserId);

/// An answer cannot be changed. See [`Ping::update_response`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum UpdateResponseError {
    /// The ping has no answer with this id.
    #[error("the ping has no response {0}. See sens_o_matic::models::ping")]
    NoSuchResponse(ResponseId),
    /// The answer is another member's.
    #[error(
        "user {user} did not give response {response}, so they cannot change it; only the member \
         who answered can. See sens_o_matic::models::ping"
    )]
    NotYours {
        /// The answer that was to be changed.
        response: ResponseId,
        /// The member who tried to change it.
        user: UserId,
    },
}

impl From<AlreadyResponded> for AppError {
    fn from(error: AlreadyResponded) -> Self {
        AppError::Conflict(error.to_string())
    }
}

impl From<UpdateResponseError> for AppError {
    fn from(error: UpdateResponseError) -> Self {
        match error {
            UpdateResponseError::NoSuchResponse(_) => AppError::NotFound("Response".to_string()),
            UpdateResponseError::NotYours { .. } => {
                AppError::Forbidden("User can only update their own response".to_string())
            }
        }
    }
}

/// Where a [`Gathering`] ping goes when the matching has run.
#[derive(Debug, Clone)]
pub enum MatchOutcome {
    /// The answers have a time in common.
    Matching(Ping<Matching>),
    /// They do not.
    NoMatch(Ping<NoMatch>),
}

impl Ping<Gathering> {
    /// Adds the answer of a member who has not answered yet. The ping stays in
    /// [`Gathering`].
    ///
    /// A ping that no longer takes answers has no such method, and the call
    /// does not compile:
    ///
    /// ```
    /// use sens_o_matic::models::{Gathering, Ping, Response};
    ///
    /// fn answer(mut ping: Ping<Gathering>, response: Response) {
    ///     let _ = ping.add_response(response);
    /// }
    /// ```
    ///
    /// ```compile_fail,E0599
    /// use sens_o_matic::models::{Matching, Ping, Response};
    ///
    /// fn answer(mut ping: Ping<Matching>, response: Response) {
    ///     let _ = ping.add_response(response);
    /// }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`AlreadyResponded`] when the member has answered, and changes
    /// nothing.
    pub fn add_response(&mut self, response: Response) -> Result<(), AlreadyResponded> {
        if self.state.responses.iter().any(|r| r.user == response.user) {
            return Err(AlreadyResponded(response.user));
        }
        self.state.responses.push(response);
        Ok(())
    }

    /// Changes an answer, and returns it as it is now.
    ///
    /// # Errors
    ///
    /// Returns [`UpdateResponseError::NoSuchResponse`] when the ping has no
    /// answer with this id, and [`UpdateResponseError::NotYours`] when
    /// `request.user` is not the member who gave it. Nothing changes.
    pub fn update_response(
        &mut self,
        id: ResponseId,
        request: UpdateResponseRequest,
    ) -> Result<&Response, UpdateResponseError> {
        let response = self
            .state
            .responses
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or(UpdateResponseError::NoSuchResponse(id))?;
        if response.user != request.user {
            return Err(UpdateResponseError::NotYours {
                response: id,
                user: request.user,
            });
        }
        response.update(request);
        Ok(response)
    }

    /// Ends the gathering: runs the matching and moves the ping to
    /// [`Matching`] when the results have a match, and to [`NoMatch`] when they
    /// do not.
    ///
    /// `calculate` is the matching, such as
    /// [`MatchingEngine::calculate_match`](crate::matching::MatchingEngine::calculate_match);
    /// it gets the id and the answers of the ping, so its results are about
    /// this ping. [`Matching`] keeps the results; [`NoMatch`] drops them,
    /// because they say only that there is no match.
    ///
    /// # Examples
    ///
    /// A member who says no leaves nobody to match, so the ping ends in
    /// [`NoMatch`]:
    ///
    /// ```
    /// use sens_o_matic::matching::MatchingEngine;
    /// use sens_o_matic::models::{
    ///     CreatePingRequest, CreateResponseRequest, GroupId, MatchOutcome, Ping, Response, UserId,
    /// };
    ///
    /// let ping = Ping::new(CreatePingRequest {
    ///     initiator: UserId::random(),
    ///     group: GroupId::random(),
    ///     activity_type: "drinks".parse().unwrap(),
    ///     rough_timing: "tonight".parse().unwrap(),
    ///     vibe: None,
    /// })
    /// .respond(Response::new(CreateResponseRequest {
    ///     user: UserId::random(),
    ///     answer: false,
    ///     availability: None,
    ///     preferences: None,
    /// }));
    ///
    /// let outcome = ping.match_responses(MatchingEngine::calculate_match);
    /// assert!(matches!(outcome, MatchOutcome::NoMatch(_)));
    /// ```
    pub fn match_responses(
        self,
        calculate: impl FnOnce(PingId, &[Response]) -> MatchResults,
    ) -> MatchOutcome {
        let results = calculate(self.id, self.responses());
        let (ping, Gathering { responses }) = self.take_state();
        if results.has_match() {
            MatchOutcome::Matching(ping.with_state(Matching {
                responses,
                match_results: results,
            }))
        } else {
            MatchOutcome::NoMatch(ping.with_state(NoMatch { responses }))
        }
    }
}

impl Ping<Matching> {
    /// The results that the matching stored.
    pub fn match_results(&self) -> &MatchResults {
        &self.state.match_results
    }

    /// Confirms the hangout: moves the ping to [`VenueConfirmed`] with the
    /// agreed timeline. The attendees are all the members who said yes, every
    /// one `pending`. The match results are dropped.
    ///
    /// A yes without an availability counts as an attendee here, although
    /// [`crate::matching`] ignored it when it looked for a common time. The
    /// initiator is an attendee only if they answered their own ping.
    ///
    /// Only a ping in [`Matching`] has this method; a ping that is still
    /// gathering cannot be confirmed:
    ///
    /// ```
    /// use sens_o_matic::models::{Matching, Ping, Timeline};
    ///
    /// fn confirm(ping: Ping<Matching>, timeline: Timeline) {
    ///     let _ = ping.confirm(timeline);
    /// }
    /// ```
    ///
    /// ```compile_fail,E0599
    /// use sens_o_matic::models::{Gathering, Ping, Timeline};
    ///
    /// fn confirm(ping: Ping<Gathering>, timeline: Timeline) {
    ///     let _ = ping.confirm(timeline);
    /// }
    /// ```
    pub fn confirm(self, timeline: Timeline) -> Ping<VenueConfirmed> {
        let attendees = self.positive_responses().iter().map(|r| r.user).collect();
        let (ping, Matching { responses, .. }) = self.take_state();
        ping.with_state(VenueConfirmed {
            responses,
            hangout: HangoutData::new(attendees, timeline),
        })
    }
}

impl Ping<VenueConfirmed> {
    /// Starts the hangout: moves the ping to [`ActiveHangout`].
    ///
    /// The ping is moved: after the call only the new one exists, so the old
    /// one cannot be used, or activated twice.
    ///
    /// ```compile_fail,E0382
    /// use sens_o_matic::models::{Ping, VenueConfirmed};
    ///
    /// fn activate_twice(ping: Ping<VenueConfirmed>) {
    ///     let _active = ping.activate();
    ///     let _again = ping.activate();
    /// }
    /// ```
    pub fn activate(self) -> Ping<ActiveHangout> {
        let (ping, VenueConfirmed { responses, hangout }) = self.take_state();
        ping.with_state(ActiveHangout { responses, hangout })
    }
}

impl Ping<ActiveHangout> {
    /// Ends the hangout: moves the ping to [`Complete`].
    pub fn complete(self) -> Ping<Complete> {
        let (ping, ActiveHangout { responses, hangout }) = self.take_state();
        ping.with_state(Complete { responses, hangout })
    }
}

impl<S: Open> Ping<S> {
    /// Cancels the ping, keeping its answers. The hangout, if there was one, is
    /// dropped.
    ///
    /// Every [`Open`] state has this method, and the three terminal states do
    /// not: a ping that is [`Complete`], [`Cancelled`] or [`NoMatch`] cannot be
    /// cancelled, and the call does not compile.
    ///
    /// ```
    /// use sens_o_matic::models::{ActiveHangout, Ping};
    ///
    /// fn cancel(ping: Ping<ActiveHangout>) {
    ///     let _ = ping.cancel();
    /// }
    /// ```
    ///
    /// ```compile_fail,E0599
    /// use sens_o_matic::models::{Complete, Ping};
    ///
    /// fn cancel(ping: Ping<Complete>) {
    ///     let _ = ping.cancel();
    /// }
    /// ```
    pub fn cancel(self) -> Ping<Cancelled> {
        let (ping, state) = self.take_state();
        ping.with_state(Cancelled {
            responses: state.into_responses(),
        })
    }
}

impl<S: HasHangout> Ping<S> {
    /// The hangout of the ping.
    pub fn hangout(&self) -> &HangoutData {
        self.state.hangout()
    }

    /// Sets the status of one attendee of the hangout. Any status can follow
    /// any other.
    ///
    /// Only the states that have a hangout have this method ([`HasHangout`]); a
    /// ping that has none cannot have an attendee status set, and the call does
    /// not compile:
    ///
    /// ```
    /// use sens_o_matic::models::{ActiveHangout, AttendeeStatus, Ping, UserId};
    ///
    /// fn arrived(mut ping: Ping<ActiveHangout>, user: UserId) {
    ///     let _ = ping.set_attendee_status(user, AttendeeStatus::Arrived);
    /// }
    /// ```
    ///
    /// ```compile_fail,E0599
    /// use sens_o_matic::models::{AttendeeStatus, Gathering, Ping, UserId};
    ///
    /// fn arrived(mut ping: Ping<Gathering>, user: UserId) {
    ///     let _ = ping.set_attendee_status(user, AttendeeStatus::Arrived);
    /// }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`NotAnAttendee`] when the user is not an attendee. Nothing
    /// changes.
    pub fn set_attendee_status(
        &mut self,
        user: UserId,
        status: AttendeeStatus,
    ) -> Result<(), NotAnAttendee> {
        self.state.hangout_mut().attendees.set_status(user, status)
    }
}

/// A ping in any state: what the stores hold, and what the API sends.
///
/// There is a variant for each state, and each holds the [`Ping`] in that state
/// with the data of the state. The transitions of the lifecycle are methods of
/// those `Ping<S>`; the methods here are the ones that a caller who holds a
/// ping of unknown state uses. Each one checks who acts and the state, makes
/// the transition, and changes the ping in place, so the caller runs it inside
/// the closure of
/// [`InMemoryStore::update`](crate::state::InMemoryStore::update). The module
/// docs ([`crate::models::ping`](self)) explain why.
///
/// In JSON the `state` and the data of that state are at the top level, next to
/// the other fields of the ping, as the schema `Ping` of the OpenAPI document
/// says.
///
/// # Examples
///
/// A wrong state is a conflict, and the message says what the lifecycle needs:
///
/// ```
/// use sens_o_matic::models::{AnyPing, AppError, CreatePingRequest, GroupId, Ping, UserId};
///
/// let mut ping = AnyPing::from(Ping::new(CreatePingRequest {
///     initiator: UserId::random(),
///     group: GroupId::random(),
///     activity_type: "drinks".parse().unwrap(),
///     rough_timing: "tonight".parse().unwrap(),
///     vibe: None,
/// }));
///
/// let Err(AppError::Conflict(message)) = ping.activate() else {
///     panic!("a ping that was just sent cannot be activated");
/// };
/// assert!(message.contains("activate the hangout"));
/// assert!(message.contains("ping_sent"));
/// assert!(message.contains("venue_confirmed"));
/// assert!(message.contains("sens_o_matic::models::ping"));
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "PingWire", into = "PingWire")]
pub enum AnyPing {
    /// See [`PingSent`].
    PingSent(Ping<PingSent>),
    /// See [`Gathering`].
    Gathering(Ping<Gathering>),
    /// See [`Matching`].
    Matching(Ping<Matching>),
    /// See [`VenueConfirmed`].
    VenueConfirmed(Ping<VenueConfirmed>),
    /// See [`ActiveHangout`].
    ActiveHangout(Ping<ActiveHangout>),
    /// See [`Complete`].
    Complete(Ping<Complete>),
    /// See [`Cancelled`].
    Cancelled(Ping<Cancelled>),
    /// See [`NoMatch`].
    NoMatch(Ping<NoMatch>),
}

/// Runs `$body` on the `Ping<S>` inside an [`AnyPing`], with the name `$ping`
/// for it, whatever the state is.
macro_rules! in_any_state {
    ($any:expr, $ping:ident => $body:expr) => {
        match $any {
            AnyPing::PingSent($ping) => $body,
            AnyPing::Gathering($ping) => $body,
            AnyPing::Matching($ping) => $body,
            AnyPing::VenueConfirmed($ping) => $body,
            AnyPing::ActiveHangout($ping) => $body,
            AnyPing::Complete($ping) => $body,
            AnyPing::Cancelled($ping) => $body,
            AnyPing::NoMatch($ping) => $body,
        }
    };
}

macro_rules! any_ping_from {
    ($($state:ident),* $(,)?) => {
        $(
            impl From<Ping<$state>> for AnyPing {
                fn from(ping: Ping<$state>) -> Self {
                    AnyPing::$state(ping)
                }
            }
        )*
    };
}

any_ping_from!(
    PingSent,
    Gathering,
    Matching,
    VenueConfirmed,
    ActiveHangout,
    Complete,
    Cancelled,
    NoMatch,
);

impl From<MatchOutcome> for AnyPing {
    fn from(outcome: MatchOutcome) -> Self {
        match outcome {
            MatchOutcome::Matching(ping) => AnyPing::Matching(ping),
            MatchOutcome::NoMatch(ping) => AnyPing::NoMatch(ping),
        }
    }
}

/// What a caller asks of a ping. It names the actions of [`AnyPing`] in the
/// message of a conflict, and says which states allow each one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    AddResponse,
    ChangeResponse,
    TriggerMatch,
    Confirm,
    Activate,
    Complete,
    Cancel,
    SetAttendeeStatus,
}

impl Action {
    /// What the action does, as the message of a conflict says it.
    const fn description(self) -> &'static str {
        match self {
            Action::AddResponse => "add a response",
            Action::ChangeResponse => "change a response",
            Action::TriggerMatch => "trigger the matching",
            Action::Confirm => "confirm the hangout",
            Action::Activate => "activate the hangout",
            Action::Complete => "complete the hangout",
            Action::Cancel => "cancel the ping",
            Action::SetAttendeeStatus => "set the status of an attendee",
        }
    }

    /// The states that allow the action. The methods of [`AnyPing`] act in
    /// exactly these; the test `the_actions_run_in_the_states_that_allow_them` holds
    /// them to it.
    const fn allowed_in(self) -> &'static [PingState] {
        match self {
            Action::AddResponse | Action::ChangeResponse => {
                &[PingState::PingSent, PingState::Gathering]
            }
            Action::TriggerMatch => &[PingState::Gathering],
            Action::Confirm => &[PingState::Matching],
            Action::Activate => &[PingState::VenueConfirmed],
            Action::Complete => &[PingState::ActiveHangout],
            Action::Cancel => &[
                PingState::PingSent,
                PingState::Gathering,
                PingState::Matching,
                PingState::VenueConfirmed,
                PingState::ActiveHangout,
            ],
            Action::SetAttendeeStatus => &[
                PingState::VenueConfirmed,
                PingState::ActiveHangout,
                PingState::Complete,
            ],
        }
    }
}

impl AnyPing {
    /// The id of the ping.
    pub fn id(&self) -> PingId {
        in_any_state!(self, ping => ping.id)
    }

    /// The member who sent the ping.
    pub fn initiator(&self) -> UserId {
        in_any_state!(self, ping => ping.initiator)
    }

    /// The group the ping was sent to.
    pub fn group(&self) -> GroupId {
        in_any_state!(self, ping => ping.group)
    }

    /// The state the ping is in.
    pub fn state(&self) -> PingState {
        fn kind<S: State>(_: &Ping<S>) -> PingState {
            S::KIND
        }
        in_any_state!(self, ping => kind(ping))
    }

    /// The answers of the ping; empty in `PingSent`.
    pub fn responses(&self) -> &[Response] {
        in_any_state!(self, ping => ping.responses())
    }

    /// The results that the matching stored: `Some` only in `Matching`. The
    /// later states do not keep them; [`crate::matching`] can compute them
    /// again from the answers.
    pub fn match_results(&self) -> Option<&MatchResults> {
        match self {
            AnyPing::Matching(ping) => Some(ping.match_results()),
            AnyPing::PingSent(_)
            | AnyPing::Gathering(_)
            | AnyPing::VenueConfirmed(_)
            | AnyPing::ActiveHangout(_)
            | AnyPing::Complete(_)
            | AnyPing::Cancelled(_)
            | AnyPing::NoMatch(_) => None,
        }
    }

    /// The hangout, in the states that have one: `VenueConfirmed`,
    /// `ActiveHangout` and `Complete`. `None` in the others.
    pub fn hangout(&self) -> Option<&HangoutData> {
        match self {
            AnyPing::VenueConfirmed(ping) => Some(ping.hangout()),
            AnyPing::ActiveHangout(ping) => Some(ping.hangout()),
            AnyPing::Complete(ping) => Some(ping.hangout()),
            AnyPing::PingSent(_)
            | AnyPing::Gathering(_)
            | AnyPing::Matching(_)
            | AnyPing::Cancelled(_)
            | AnyPing::NoMatch(_) => None,
        }
    }

    /// The conflict for an action that the state of the ping does not allow.
    fn refuse(&self, action: Action) -> AppError {
        let allowed = action.allowed_in();
        let needed = match allowed {
            [] => String::new(),
            [only] => only.to_string(),
            [init @ .., last] => {
                let init: Vec<String> = init.iter().map(ToString::to_string).collect();
                format!("{} or {last}", init.join(", "))
            }
        };
        AppError::Conflict(format!(
            "Cannot {}: the ping is in the {} state, and this needs the {needed} state. \
             See sens_o_matic::models::ping for the lifecycle.",
            action.description(),
            self.state(),
        ))
    }

    /// The refusal of a user who is not the initiator.
    fn require_initiator(&self, user: UserId, message: &str) -> Result<(), AppError> {
        if self.initiator() == user {
            Ok(())
        } else {
            Err(AppError::Forbidden(message.to_string()))
        }
    }

    /// Adds the answer of a member, in `PingSent` and `Gathering`. The first
    /// answer moves the ping to `Gathering`.
    ///
    /// Whether the member belongs to the group is not checked here; the
    /// handler does that, with the group.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Conflict`] in any other state, and when the member
    /// has answered already.
    pub fn add_response(&mut self, response: Response) -> Result<(), AppError> {
        match self {
            AnyPing::PingSent(ping) => {
                *self = AnyPing::Gathering(ping.clone().respond(response));
                Ok(())
            }
            AnyPing::Gathering(ping) => Ok(ping.add_response(response)?),
            AnyPing::Matching(_)
            | AnyPing::VenueConfirmed(_)
            | AnyPing::ActiveHangout(_)
            | AnyPing::Complete(_)
            | AnyPing::Cancelled(_)
            | AnyPing::NoMatch(_) => Err(self.refuse(Action::AddResponse)),
        }
    }

    /// Changes an answer, in `PingSent` and `Gathering`, and returns it as it
    /// is now. The ping in `PingSent` has no answer, so there is none to find.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Conflict`] in any other state, [`AppError::NotFound`]
    /// when the ping has no answer with this id, and [`AppError::Forbidden`]
    /// when `request.user` did not give it. The state is checked first.
    pub fn update_response(
        &mut self,
        id: ResponseId,
        request: UpdateResponseRequest,
    ) -> Result<Response, AppError> {
        match self {
            AnyPing::PingSent(_) => Err(AppError::NotFound("Response".to_string())),
            AnyPing::Gathering(ping) => Ok(ping.update_response(id, request)?.clone()),
            AnyPing::Matching(_)
            | AnyPing::VenueConfirmed(_)
            | AnyPing::ActiveHangout(_)
            | AnyPing::Complete(_)
            | AnyPing::Cancelled(_)
            | AnyPing::NoMatch(_) => Err(self.refuse(Action::ChangeResponse)),
        }
    }

    /// Ends the gathering, as `user`: runs the matching with `calculate` (see
    /// [`Ping::match_responses`]) and moves the ping to `Matching` or to
    /// `NoMatch`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Forbidden`] when `user` is not the initiator, and
    /// otherwise [`AppError::Conflict`] when the ping is not in `Gathering`. A
    /// ping that nobody has answered is in `PingSent`, so it is a conflict. The
    /// user is checked first.
    pub fn trigger_match(
        &mut self,
        user: UserId,
        calculate: impl FnOnce(PingId, &[Response]) -> MatchResults,
    ) -> Result<(), AppError> {
        self.require_initiator(user, "Only initiator can trigger matching")?;
        let AnyPing::Gathering(ping) = self else {
            return Err(self.refuse(Action::TriggerMatch));
        };
        *self = ping.clone().match_responses(calculate).into();
        Ok(())
    }

    /// Confirms the hangout, in `Matching`; see [`Ping::confirm`]. It does not
    /// check who confirms.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Conflict`] in any other state.
    pub fn confirm(&mut self, timeline: Timeline) -> Result<(), AppError> {
        let AnyPing::Matching(ping) = self else {
            return Err(self.refuse(Action::Confirm));
        };
        *self = AnyPing::VenueConfirmed(ping.clone().confirm(timeline));
        Ok(())
    }

    /// Starts the hangout, in `VenueConfirmed`. It does not check who starts
    /// it.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Conflict`] in any other state.
    pub fn activate(&mut self) -> Result<(), AppError> {
        let AnyPing::VenueConfirmed(ping) = self else {
            return Err(self.refuse(Action::Activate));
        };
        *self = AnyPing::ActiveHangout(ping.clone().activate());
        Ok(())
    }

    /// Ends the hangout, in `ActiveHangout`. It does not check who ends it.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Conflict`] in any other state.
    pub fn complete(&mut self) -> Result<(), AppError> {
        let AnyPing::ActiveHangout(ping) = self else {
            return Err(self.refuse(Action::Complete));
        };
        *self = AnyPing::Complete(ping.clone().complete());
        Ok(())
    }

    /// Cancels the ping, as `user`, keeping its answers; see [`Ping::cancel`].
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Forbidden`] when `user` is not the initiator, and
    /// otherwise [`AppError::Conflict`] when the ping is `Complete`,
    /// `Cancelled` or `NoMatch`. The user is checked first.
    ///
    /// # Examples
    ///
    /// ```
    /// use sens_o_matic::models::{AnyPing, AppError, CreatePingRequest, GroupId, Ping, UserId};
    ///
    /// let initiator = UserId::random();
    /// let mut ping = AnyPing::from(Ping::new(CreatePingRequest {
    ///     initiator,
    ///     group: GroupId::random(),
    ///     activity_type: "drinks".parse().unwrap(),
    ///     rough_timing: "tonight".parse().unwrap(),
    ///     vibe: None,
    /// }));
    ///
    /// let stranger = ping.cancel(UserId::random());
    /// assert!(matches!(stranger, Err(AppError::Forbidden(_))));
    ///
    /// assert!(ping.cancel(initiator).is_ok());
    ///
    /// // A cancelled ping is terminal, so it cannot be cancelled again.
    /// let again = ping.cancel(initiator);
    /// assert!(matches!(again, Err(AppError::Conflict(_))));
    /// ```
    pub fn cancel(&mut self, user: UserId) -> Result<(), AppError> {
        self.require_initiator(user, "Only initiator can cancel ping")?;
        let cancelled = match self {
            AnyPing::PingSent(ping) => ping.clone().cancel(),
            AnyPing::Gathering(ping) => ping.clone().cancel(),
            AnyPing::Matching(ping) => ping.clone().cancel(),
            AnyPing::VenueConfirmed(ping) => ping.clone().cancel(),
            AnyPing::ActiveHangout(ping) => ping.clone().cancel(),
            AnyPing::Complete(_) | AnyPing::Cancelled(_) | AnyPing::NoMatch(_) => {
                return Err(self.refuse(Action::Cancel));
            }
        };
        *self = AnyPing::Cancelled(cancelled);
        Ok(())
    }

    /// Sets the status of one attendee of the hangout, in the states that have
    /// one: `VenueConfirmed`, `ActiveHangout` and `Complete`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Conflict`] in any other state, and
    /// [`AppError::NotFound`] when the user is not an attendee. The state is
    /// checked first.
    pub fn set_attendee_status(
        &mut self,
        user: UserId,
        status: AttendeeStatus,
    ) -> Result<(), AppError> {
        let result = match self {
            AnyPing::VenueConfirmed(ping) => ping.set_attendee_status(user, status),
            AnyPing::ActiveHangout(ping) => ping.set_attendee_status(user, status),
            AnyPing::Complete(ping) => ping.set_attendee_status(user, status),
            AnyPing::PingSent(_)
            | AnyPing::Gathering(_)
            | AnyPing::Matching(_)
            | AnyPing::Cancelled(_)
            | AnyPing::NoMatch(_) => return Err(self.refuse(Action::SetAttendeeStatus)),
        };
        result.map_err(|_| AppError::NotFound("Attendee".to_string()))
    }
}

// The wire form of the types of `State`, and the pair of `PingWire`. Their doc
// comments are the descriptions in the OpenAPI document, so they are written
// for the reader of the API.

/// The stage of a ping, and the data that belongs to the stage.
///
/// In JSON the stage is the `state` field of the ping, in `snake_case`, and
/// the data of the stage sits next to it, in the ping itself.
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(tag = "state", rename_all = "snake_case")]
enum PingLifecycle {
    /// The ping was sent and nobody has answered yet.
    PingSent,
    /// Members are answering. The initiator ends this stage by triggering the
    /// matching.
    Gathering {
        /// The answers so far, in the order they came.
        responses: Vec<Response>,
    },
    /// The answers have a time in common. The initiator picks the hangout.
    Matching {
        /// The answers that were matched.
        responses: Vec<Response>,
        /// The common time window that was found.
        match_results: MatchResults,
    },
    /// The initiator confirmed the hangout; it has not started.
    VenueConfirmed {
        /// The answers that led to the hangout.
        responses: Vec<Response>,
        /// The hangout: when, and who comes.
        hangout: HangoutData,
    },
    /// The hangout is under way.
    ActiveHangout {
        /// The answers that led to the hangout.
        responses: Vec<Response>,
        /// The hangout: when, and who comes.
        hangout: HangoutData,
    },
    /// The hangout is over. Terminal.
    Complete {
        /// The answers that led to the hangout.
        responses: Vec<Response>,
        /// The hangout: when, and who came.
        hangout: HangoutData,
    },
    /// The initiator cancelled the ping. Terminal.
    Cancelled {
        /// The answers that had come in when the ping was cancelled.
        responses: Vec<Response>,
    },
    /// The matching found no time in common. Terminal.
    NoMatch {
        /// The answers that were matched.
        responses: Vec<Response>,
    },
}

/// A request from a member to the group to meet.
///
/// In JSON the `state` and the data of that state are at the top level, next to
/// the other fields.
#[derive(Serialize, Deserialize, ToSchema)]
#[schema(as = Ping)]
struct PingWire {
    /// Assigned by the server when the ping is created. It is also the id of
    /// the hangout, if the ping gets that far.
    #[schema(value_type = String, format = "uuid")]
    id: PingId,
    /// The member who sent the ping. Only they can trigger the matching and
    /// cancel the ping.
    #[schema(value_type = String, format = "uuid")]
    initiator: UserId,
    /// The group the ping was sent to; only its members can answer.
    #[schema(value_type = String, format = "uuid")]
    group: GroupId,
    /// What to do, in the words of the initiator ("drinks", "dinner"), 1 to 50
    /// characters.
    #[schema(value_type = String, min_length = 1, max_length = 50)]
    activity_type: ActivityType,
    /// When, roughly ("tonight", "this afternoon"), 1 to 50 characters. Free
    /// text: the matching works on the availabilities of the answers, not on
    /// this.
    #[schema(value_type = String, min_length = 1, max_length = 50)]
    rough_timing: RoughTiming,
    /// The mood of the meetup ("chill"), up to 100 characters.
    #[schema(value_type = Option<String>, max_length = 100)]
    vibe: Option<Vibe>,
    /// When the ping was sent.
    created_at: DateTime<Utc>,
    /// The stage of the ping and the data of that stage.
    #[serde(flatten)]
    lifecycle: PingLifecycle,
}

schema_of_wire!(AnyPing => PingWire);

impl<S> Ping<S> {
    /// The ping as it is in JSON, with the wire form of its state.
    fn into_wire(self, lifecycle: impl FnOnce(S) -> PingLifecycle) -> PingWire {
        let (ping, state) = self.take_state();
        PingWire {
            id: ping.id,
            initiator: ping.initiator,
            group: ping.group,
            activity_type: ping.activity_type,
            rough_timing: ping.rough_timing,
            vibe: ping.vibe,
            created_at: ping.created_at,
            lifecycle: lifecycle(state),
        }
    }
}

impl From<AnyPing> for PingWire {
    fn from(ping: AnyPing) -> Self {
        match ping {
            AnyPing::PingSent(ping) => ping.into_wire(|PingSent| PingLifecycle::PingSent),
            AnyPing::Gathering(ping) => {
                ping.into_wire(|Gathering { responses }| PingLifecycle::Gathering { responses })
            }
            AnyPing::Matching(ping) => ping.into_wire(
                |Matching {
                     responses,
                     match_results,
                 }| PingLifecycle::Matching {
                    responses,
                    match_results,
                },
            ),
            AnyPing::VenueConfirmed(ping) => {
                ping.into_wire(|VenueConfirmed { responses, hangout }| {
                    PingLifecycle::VenueConfirmed { responses, hangout }
                })
            }
            AnyPing::ActiveHangout(ping) => {
                ping.into_wire(|ActiveHangout { responses, hangout }| {
                    PingLifecycle::ActiveHangout { responses, hangout }
                })
            }
            AnyPing::Complete(ping) => ping.into_wire(|Complete { responses, hangout }| {
                PingLifecycle::Complete { responses, hangout }
            }),
            AnyPing::Cancelled(ping) => {
                ping.into_wire(|Cancelled { responses }| PingLifecycle::Cancelled { responses })
            }
            AnyPing::NoMatch(ping) => {
                ping.into_wire(|NoMatch { responses }| PingLifecycle::NoMatch { responses })
            }
        }
    }
}

impl From<PingWire> for AnyPing {
    fn from(wire: PingWire) -> Self {
        let ping = Ping {
            id: wire.id,
            initiator: wire.initiator,
            group: wire.group,
            activity_type: wire.activity_type,
            rough_timing: wire.rough_timing,
            vibe: wire.vibe,
            created_at: wire.created_at,
            state: (),
        };
        match wire.lifecycle {
            PingLifecycle::PingSent => AnyPing::PingSent(ping.with_state(PingSent)),
            PingLifecycle::Gathering { responses } => {
                AnyPing::Gathering(ping.with_state(Gathering { responses }))
            }
            PingLifecycle::Matching {
                responses,
                match_results,
            } => AnyPing::Matching(ping.with_state(Matching {
                responses,
                match_results,
            })),
            PingLifecycle::VenueConfirmed { responses, hangout } => {
                AnyPing::VenueConfirmed(ping.with_state(VenueConfirmed { responses, hangout }))
            }
            PingLifecycle::ActiveHangout { responses, hangout } => {
                AnyPing::ActiveHangout(ping.with_state(ActiveHangout { responses, hangout }))
            }
            PingLifecycle::Complete { responses, hangout } => {
                AnyPing::Complete(ping.with_state(Complete { responses, hangout }))
            }
            PingLifecycle::Cancelled { responses } => {
                AnyPing::Cancelled(ping.with_state(Cancelled { responses }))
            }
            PingLifecycle::NoMatch { responses } => {
                AnyPing::NoMatch(ping.with_state(NoMatch { responses }))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use serde_json::json;

    use super::*;
    use crate::models::{
        Availability, CreateResponseRequest, TimeOverlap, UpdateResponseRequest, value::TimeWindow,
    };

    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2024, 12, 15, hour, 0, 0).unwrap()
    }

    fn timeline() -> Timeline {
        Timeline::new(at(18), at(21)).unwrap()
    }

    fn new_ping(initiator: UserId) -> Ping<PingSent> {
        Ping::new(CreatePingRequest {
            initiator,
            group: GroupId::random(),
            activity_type: "drinks".parse().unwrap(),
            rough_timing: "tonight".parse().unwrap(),
            vibe: None,
        })
    }

    fn answer(user: UserId, yes: bool) -> Response {
        Response::new(CreateResponseRequest {
            user,
            answer: yes,
            availability: Some(Availability::new(at(18), at(21)).unwrap()),
            preferences: None,
        })
    }

    /// A matching that finds the window 18:00 to 21:00.
    fn found(ping_id: PingId, _responses: &[Response]) -> MatchResults {
        MatchResults {
            ping_id,
            overlap: Some(TimeOverlap::new(
                TimeWindow::new(at(18), at(21)).unwrap(),
                1,
            )),
        }
    }

    /// A matching that finds nothing.
    fn missed(ping_id: PingId, _responses: &[Response]) -> MatchResults {
        MatchResults {
            ping_id,
            overlap: None,
        }
    }

    /// A ping in `state`, whose initiator and one member who said yes are given.
    fn ping_in(state: PingState, initiator: UserId, member: UserId) -> AnyPing {
        let gathering = || new_ping(initiator).respond(answer(member, true));
        let matching = || match gathering().match_responses(found) {
            MatchOutcome::Matching(ping) => ping,
            MatchOutcome::NoMatch(_) => unreachable!("`found` finds a window"),
        };
        match state {
            PingState::PingSent => new_ping(initiator).into(),
            PingState::Gathering => gathering().into(),
            PingState::Matching => matching().into(),
            PingState::VenueConfirmed => matching().confirm(timeline()).into(),
            PingState::ActiveHangout => matching().confirm(timeline()).activate().into(),
            PingState::Complete => matching().confirm(timeline()).activate().complete().into(),
            PingState::Cancelled => gathering().cancel().into(),
            PingState::NoMatch => match gathering().match_responses(missed) {
                MatchOutcome::NoMatch(ping) => ping.into(),
                MatchOutcome::Matching(_) => unreachable!("`missed` finds nothing"),
            },
        }
    }

    impl Action {
        const ALL: [Action; 8] = [
            Action::AddResponse,
            Action::ChangeResponse,
            Action::TriggerMatch,
            Action::Confirm,
            Action::Activate,
            Action::Complete,
            Action::Cancel,
            Action::SetAttendeeStatus,
        ];
    }

    /// Does `action` to `ping` with arguments that would succeed in a state
    /// that allows it.
    fn act(
        action: Action,
        ping: &mut AnyPing,
        initiator: UserId,
        member: UserId,
    ) -> Result<(), AppError> {
        match action {
            Action::AddResponse => ping.add_response(answer(UserId::random(), true)),
            Action::ChangeResponse => {
                let id = ping.responses().first().map(|r| r.id);
                let update = UpdateResponseRequest {
                    user: member,
                    answer: Some(false),
                    availability: None,
                    preferences: None,
                };
                ping.update_response(id.unwrap_or_else(ResponseId::random), update)
                    .map(|_| ())
            }
            Action::TriggerMatch => ping.trigger_match(initiator, found),
            Action::Confirm => ping.confirm(timeline()),
            Action::Activate => ping.activate(),
            Action::Complete => ping.complete(),
            Action::Cancel => ping.cancel(initiator),
            Action::SetAttendeeStatus => ping.set_attendee_status(member, AttendeeStatus::Arrived),
        }
    }

    /// The table of `Action::allowed_in`, which names the states in the message
    /// of a conflict, is what the methods of `AnyPing` do: an action is refused
    /// with a conflict in every state the table leaves out, and only there, and
    /// a refusal changes nothing.
    #[test]
    fn the_actions_run_in_the_states_that_allow_them() {
        let (initiator, member) = (UserId::random(), UserId::random());
        for action in Action::ALL {
            assert!(
                !action.allowed_in().is_empty(),
                "{action:?} is allowed in no state"
            );
            for state in PingState::ALL {
                let mut ping = ping_in(state, initiator, member);
                assert_eq!(ping.state(), state, "the test builds the state it names");

                let result = act(action, &mut ping, initiator, member);

                let is_conflict = matches!(result, Err(AppError::Conflict(_)));
                if action.allowed_in().contains(&state) {
                    assert!(!is_conflict, "{action:?} in {state}: {result:?}");
                } else {
                    assert!(is_conflict, "{action:?} in {state}: {result:?}");
                    assert_eq!(ping.state(), state, "{action:?} changed a ping in {state}");
                }
            }
        }
    }

    /// The message of a conflict says what the issue asks: the action, the state
    /// the ping is in, the states that allow the action, and the module that has
    /// the lifecycle.
    #[test]
    fn a_conflict_names_the_action_the_state_and_the_module() {
        let initiator = UserId::random();
        let mut ping = ping_in(PingState::Complete, initiator, UserId::random());

        let Err(AppError::Conflict(message)) = ping.cancel(initiator) else {
            panic!("a complete ping cannot be cancelled");
        };

        assert_eq!(
            message,
            "Cannot cancel the ping: the ping is in the complete state, and this needs the \
             ping_sent, gathering, matching, venue_confirmed or active_hangout state. \
             See sens_o_matic::models::ping for the lifecycle."
        );
    }

    /// The user is checked before the state, so a stranger gets `403` for a
    /// ping that is in the wrong state too.
    #[test]
    fn the_initiator_is_checked_before_the_state() {
        let mut ping = ping_in(PingState::Complete, UserId::random(), UserId::random());

        assert!(matches!(
            ping.cancel(UserId::random()),
            Err(AppError::Forbidden(_))
        ));
        assert!(matches!(
            ping.trigger_match(UserId::random(), found),
            Err(AppError::Forbidden(_))
        ));
    }

    /// A ping that nobody has answered is in `PingSent`, which the matching
    /// refuses.
    #[test]
    fn a_ping_without_answers_cannot_be_matched() {
        let initiator = UserId::random();
        let mut ping = ping_in(PingState::PingSent, initiator, UserId::random());

        assert!(matches!(
            ping.trigger_match(initiator, found),
            Err(AppError::Conflict(_))
        ));
    }

    #[test]
    fn the_state_names_are_the_tags_of_the_json() {
        let (initiator, member) = (UserId::random(), UserId::random());
        for state in PingState::ALL {
            let ping = ping_in(state, initiator, member);
            let json = serde_json::to_value(&ping).unwrap();
            assert_eq!(json["state"], state.as_str());
            assert_eq!(state.as_str().parse::<PingState>(), Ok(state));
            assert_eq!(state.to_string(), state.as_str());
        }
    }

    /// Whatever the state, the JSON has the fields that it always had at the top
    /// level, and reads back to the same ping.
    #[test]
    fn a_ping_reads_back_from_its_json() {
        let (initiator, member) = (UserId::random(), UserId::random());
        for state in PingState::ALL {
            let ping = ping_in(state, initiator, member);

            let json = serde_json::to_value(&ping).unwrap();
            for field in [
                "id",
                "initiator",
                "group",
                "activity_type",
                "rough_timing",
                "created_at",
                "state",
            ] {
                assert!(json.get(field).is_some(), "{state}: no `{field}`");
            }
            assert_eq!(json["initiator"], json!(initiator));

            let back: AnyPing = serde_json::from_value(json.clone()).unwrap();
            assert_eq!(back.state(), state);
            assert_eq!(serde_json::to_value(&back).unwrap(), json, "{state}");
        }
    }

    #[test]
    fn the_data_of_a_state_is_in_that_state_only() {
        let (initiator, member) = (UserId::random(), UserId::random());
        for state in PingState::ALL {
            let json = serde_json::to_value(ping_in(state, initiator, member)).unwrap();
            let has = |field: &str| json.get(field).is_some();

            assert_eq!(has("responses"), state != PingState::PingSent, "{state}");
            assert_eq!(
                has("match_results"),
                state == PingState::Matching,
                "{state}"
            );
            assert_eq!(
                has("hangout"),
                matches!(
                    state,
                    PingState::VenueConfirmed | PingState::ActiveHangout | PingState::Complete
                ),
                "{state}"
            );
        }
    }

    #[test]
    fn a_member_answers_once() {
        let member = UserId::random();
        let mut ping = new_ping(UserId::random()).respond(answer(member, true));

        let again = ping.add_response(answer(member, false));

        assert_eq!(again, Err(AlreadyResponded(member)));
        assert_eq!(ping.responses().len(), 1);
        assert!(ping.responses()[0].answer, "the first answer is kept");
    }

    #[test]
    fn only_the_member_who_answered_changes_the_answer() {
        let member = UserId::random();
        let mut ping = new_ping(UserId::random()).respond(answer(member, true));
        let id = ping.responses()[0].id;
        let update = |user| UpdateResponseRequest {
            user,
            answer: Some(false),
            availability: None,
            preferences: None,
        };

        let stranger = UserId::random();
        assert_eq!(
            ping.update_response(id, update(stranger)).unwrap_err(),
            UpdateResponseError::NotYours {
                response: id,
                user: stranger
            }
        );
        assert!(ping.responses()[0].answer, "a refusal changes nothing");

        let unknown = ResponseId::random();
        assert_eq!(
            ping.update_response(unknown, update(member)).unwrap_err(),
            UpdateResponseError::NoSuchResponse(unknown)
        );

        assert!(!ping.update_response(id, update(member)).unwrap().answer);
    }

    #[test]
    fn the_hangout_has_the_members_who_said_yes() {
        let (initiator, yes, no) = (UserId::random(), UserId::random(), UserId::random());
        let mut ping = new_ping(initiator).respond(answer(yes, true));
        ping.add_response(answer(no, false)).unwrap();
        let MatchOutcome::Matching(ping) = ping.match_responses(found) else {
            panic!("`found` finds a window");
        };

        let ping = ping.confirm(timeline());

        let attendees = &ping.hangout().attendees;
        assert!(attendees.contains(yes));
        assert!(!attendees.contains(no));
        assert!(
            !attendees.contains(initiator),
            "the initiator is an attendee only if they answered"
        );
    }

    #[test]
    fn a_state_that_is_not_a_name_is_refused() {
        let error = "finished".parse::<PingState>().unwrap_err();
        assert_eq!(error, UnknownState("finished".to_string()));
        assert!(error.to_string().contains("sens_o_matic::models::ping"));
    }
}
