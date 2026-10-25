//! A request to meet, and the state it is in.
//!
//! A [`Ping`] is what a member sends to a group: an activity, a rough timing
//! and an optional vibe. From then on it goes through the stages of
//! [`PingLifecycle`], gathers the [`Response`]s of the members, and ends as a
//! hangout, a cancellation or a miss.
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
//! | `PingSent` | yes | – |
//! | `Gathering` | yes | – |
//! | `Matching` | no | the [`MatchResults`] |
//! | `VenueConfirmed`, `ActiveHangout`, `Complete` | no | the [`HangoutData`] |
//! | `Cancelled`, `NoMatch` | no | – |
//!
//! The data that belongs to a stage is in the variant of that stage. A ping
//! that was not confirmed has no hangout to read or to change, and the type
//! says so.
//!
//! # Who does what
//!
//! - This module says which states exist and which actions each one allows
//!   (the `can_*` methods of [`PingLifecycle`]). The only transition it makes
//!   is the first one, from `PingSent` to `Gathering`, in [`Ping::add_response`].
//! - [`crate::state_machine`] adds the rule of *who* may act (the initiator
//!   triggers the matching and cancels), turns a refusal into an
//!   [`AppError`](super::AppError), and makes the other transitions. A doc
//!   test there walks a ping through the whole lifecycle.
//! - [`crate::matching`] decides whether `Gathering` ends in `Matching` or in
//!   `NoMatch`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

use super::hangout::{HangoutData, MatchResults};
use super::response::Response;

/// The stage of a ping, and the data that belongs to the stage.
///
/// In JSON the stage is the `state` field of the ping, in `snake_case`, and
/// the data of the stage sits next to it, in the ping itself.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PingLifecycle {
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

impl PingLifecycle {
    /// The name of the state, as it appears in the `state` field of the JSON of
    /// a ping and in the `state` filter of the list of pings of a group.
    ///
    /// # Examples
    ///
    /// ```
    /// use sens_o_matic::models::PingLifecycle;
    ///
    /// assert_eq!(PingLifecycle::PingSent.state_name(), "ping_sent");
    /// assert_eq!(
    ///     PingLifecycle::NoMatch { responses: vec![] }.state_name(),
    ///     "no_match"
    /// );
    /// ```
    pub fn state_name(&self) -> &'static str {
        match self {
            PingLifecycle::PingSent => "ping_sent",
            PingLifecycle::Gathering { .. } => "gathering",
            PingLifecycle::Matching { .. } => "matching",
            PingLifecycle::VenueConfirmed { .. } => "venue_confirmed",
            PingLifecycle::ActiveHangout { .. } => "active_hangout",
            PingLifecycle::Complete { .. } => "complete",
            PingLifecycle::Cancelled { .. } => "cancelled",
            PingLifecycle::NoMatch { .. } => "no_match",
        }
    }

    /// Tells whether the ping has reached an end: `Complete`, `Cancelled` or
    /// `NoMatch`. Nothing changes a ping after that.
    ///
    /// # Examples
    ///
    /// ```
    /// use sens_o_matic::models::PingLifecycle;
    ///
    /// assert!(!PingLifecycle::PingSent.is_terminal());
    /// assert!(PingLifecycle::Cancelled { responses: vec![] }.is_terminal());
    /// ```
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            PingLifecycle::Complete { .. }
                | PingLifecycle::Cancelled { .. }
                | PingLifecycle::NoMatch { .. }
        )
    }

    /// Tells whether members may still answer or change their answer:
    /// `PingSent` and `Gathering`.
    pub fn can_add_response(&self) -> bool {
        matches!(
            self,
            PingLifecycle::PingSent | PingLifecycle::Gathering { .. }
        )
    }

    /// Tells whether the matching may run: only in `Gathering`. A ping with no
    /// answer yet is in `PingSent`, so it cannot be matched.
    pub fn can_trigger_match(&self) -> bool {
        matches!(self, PingLifecycle::Gathering { .. })
    }

    /// Tells whether the hangout may be confirmed: only in `Matching`.
    pub fn can_confirm(&self) -> bool {
        matches!(self, PingLifecycle::Matching { .. })
    }

    /// Tells whether the hangout may start: only in `VenueConfirmed`.
    pub fn can_activate(&self) -> bool {
        matches!(self, PingLifecycle::VenueConfirmed { .. })
    }

    /// Tells whether the hangout may end: only in `ActiveHangout`.
    pub fn can_complete(&self) -> bool {
        matches!(self, PingLifecycle::ActiveHangout { .. })
    }

    /// Tells whether the ping may be cancelled: in every state that is not
    /// [terminal](PingLifecycle::is_terminal).
    pub fn can_cancel(&self) -> bool {
        !self.is_terminal()
    }

    /// The answers of the ping; empty in `PingSent`.
    pub fn responses(&self) -> &[Response] {
        match self {
            PingLifecycle::PingSent => &[],
            PingLifecycle::Gathering { responses }
            | PingLifecycle::Matching { responses, .. }
            | PingLifecycle::VenueConfirmed { responses, .. }
            | PingLifecycle::ActiveHangout { responses, .. }
            | PingLifecycle::Complete { responses, .. }
            | PingLifecycle::Cancelled { responses }
            | PingLifecycle::NoMatch { responses } => responses,
        }
    }

    /// The answers of the ping, to change them; `None` in `PingSent`, which has
    /// no list to change. [`Ping::add_response`] adds the first answer.
    pub fn responses_mut(&mut self) -> Option<&mut Vec<Response>> {
        match self {
            PingLifecycle::PingSent => None,
            PingLifecycle::Gathering { responses }
            | PingLifecycle::Matching { responses, .. }
            | PingLifecycle::VenueConfirmed { responses, .. }
            | PingLifecycle::ActiveHangout { responses, .. }
            | PingLifecycle::Complete { responses, .. }
            | PingLifecycle::Cancelled { responses }
            | PingLifecycle::NoMatch { responses } => Some(responses),
        }
    }

    /// The hangout, in the states that have one: `VenueConfirmed`,
    /// `ActiveHangout` and `Complete`. `None` in the others.
    pub fn hangout(&self) -> Option<&HangoutData> {
        match self {
            PingLifecycle::VenueConfirmed { hangout, .. }
            | PingLifecycle::ActiveHangout { hangout, .. }
            | PingLifecycle::Complete { hangout, .. } => Some(hangout),
            PingLifecycle::PingSent
            | PingLifecycle::Gathering { .. }
            | PingLifecycle::Matching { .. }
            | PingLifecycle::Cancelled { .. }
            | PingLifecycle::NoMatch { .. } => None,
        }
    }

    /// The hangout, to change it; `Some` in the same states as
    /// [`PingLifecycle::hangout`].
    pub fn hangout_mut(&mut self) -> Option<&mut HangoutData> {
        match self {
            PingLifecycle::VenueConfirmed { hangout, .. }
            | PingLifecycle::ActiveHangout { hangout, .. }
            | PingLifecycle::Complete { hangout, .. } => Some(hangout),
            PingLifecycle::PingSent
            | PingLifecycle::Gathering { .. }
            | PingLifecycle::Matching { .. }
            | PingLifecycle::Cancelled { .. }
            | PingLifecycle::NoMatch { .. } => None,
        }
    }

    /// The results that the matching stored: `Some` only in `Matching`. The
    /// later states do not keep them; [`crate::matching`] can compute them
    /// again from the answers.
    pub fn match_results(&self) -> Option<&MatchResults> {
        match self {
            PingLifecycle::Matching { match_results, .. } => Some(match_results),
            PingLifecycle::PingSent
            | PingLifecycle::Gathering { .. }
            | PingLifecycle::VenueConfirmed { .. }
            | PingLifecycle::ActiveHangout { .. }
            | PingLifecycle::Complete { .. }
            | PingLifecycle::Cancelled { .. }
            | PingLifecycle::NoMatch { .. } => None,
        }
    }
}

/// A request from a member to the group to meet.
///
/// In JSON the `state` and the data of that state are at the top level, next to
/// the other fields.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Ping {
    /// Assigned by the server when the ping is created. It is also the id of
    /// the hangout, if the ping gets that far.
    pub id: Uuid,
    /// The member who sent the ping. Only they can trigger the matching and
    /// cancel the ping.
    pub initiator: Uuid,
    /// The group the ping was sent to; only its members can answer.
    pub group: Uuid,
    /// What to do, in the words of the initiator ("drinks", "dinner"), 1 to 50
    /// characters.
    pub activity_type: String,
    /// When, roughly ("tonight", "this afternoon"), 1 to 50 characters. Free
    /// text: the matching works on the availabilities of the answers, not on
    /// this.
    pub rough_timing: String,
    /// The mood of the meetup ("chill"), up to 100 characters.
    pub vibe: Option<String>,
    /// When the ping was sent.
    pub created_at: DateTime<Utc>,
    /// The stage of the ping and the data of that stage.
    #[serde(flatten)]
    pub lifecycle: PingLifecycle,
}

/// The body of the request that sends a ping.
#[derive(Debug, Clone, Deserialize, ToSchema, Validate)]
pub struct CreatePingRequest {
    /// The member who sends the ping; must be a member of `group`.
    pub initiator: Uuid,
    /// The group to send the ping to.
    pub group: Uuid,
    /// What to do, 1 to 50 characters.
    #[validate(length(min = 1, max = 50))]
    pub activity_type: String,
    /// When, roughly, 1 to 50 characters.
    #[validate(length(min = 1, max = 50))]
    pub rough_timing: String,
    /// The mood of the meetup, up to 100 characters.
    #[validate(length(max = 100))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vibe: Option<String>,
}

/// The body of the request that cancels a ping.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct CancelPingRequest {
    /// The user who cancels; must be the initiator of the ping.
    pub user_id: Uuid,
}

/// The body of the request that ends the gathering of answers and runs the
/// matching.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct TriggerMatchRequest {
    /// The user who triggers the matching; must be the initiator of the ping.
    pub user_id: Uuid,
}

impl Ping {
    /// Creates a ping in the `PingSent` state, with a new random id, stamped
    /// with the current time, from a request that has passed validation.
    ///
    /// # Examples
    ///
    /// The lengths that [`CreatePingRequest`] allows count characters, not
    /// bytes:
    ///
    /// ```
    /// use sens_o_matic::models::{CreatePingRequest, Ping};
    /// use uuid::Uuid;
    /// use validator::Validate;
    ///
    /// let request = |activity_type: &str| CreatePingRequest {
    ///     initiator: Uuid::new_v4(),
    ///     group: Uuid::new_v4(),
    ///     activity_type: activity_type.to_string(),
    ///     rough_timing: "tonight".to_string(),
    ///     vibe: None,
    /// };
    ///
    /// assert!(request("").validate().is_err());
    /// assert!(request(&"é".repeat(51)).validate().is_err());
    ///
    /// let valid = request(&"é".repeat(50)); // 50 characters, 100 bytes
    /// assert!(valid.validate().is_ok());
    ///
    /// let ping = Ping::new(valid);
    /// assert_eq!(ping.lifecycle.state_name(), "ping_sent");
    /// ```
    pub fn new(request: CreatePingRequest) -> Self {
        Self {
            id: Uuid::new_v4(),
            initiator: request.initiator,
            group: request.group,
            activity_type: request.activity_type,
            rough_timing: request.rough_timing,
            vibe: request.vibe,
            created_at: Utc::now(),
            lifecycle: PingLifecycle::PingSent,
        }
    }

    /// Adds an answer. The first one moves the ping from `PingSent` to
    /// `Gathering`, whatever the answer is.
    ///
    /// In a state that no longer takes answers this does nothing: the answer is
    /// dropped without an error. The caller checks first, with
    /// [`StateMachine::can_add_response`](crate::state_machine::StateMachine::can_add_response).
    /// It does not check that the user has not answered yet, either; see
    /// [`Ping::has_user_responded`].
    ///
    /// # Examples
    ///
    /// ```
    /// use sens_o_matic::models::{CreatePingRequest, CreateResponseRequest, Ping, Response};
    /// use uuid::Uuid;
    ///
    /// let mut ping = Ping::new(CreatePingRequest {
    ///     initiator: Uuid::new_v4(),
    ///     group: Uuid::new_v4(),
    ///     activity_type: "drinks".to_string(),
    ///     rough_timing: "tonight".to_string(),
    ///     vibe: None,
    /// });
    /// assert_eq!(ping.lifecycle.state_name(), "ping_sent");
    ///
    /// ping.add_response(Response::new(CreateResponseRequest {
    ///     user: Uuid::new_v4(),
    ///     answer: false,
    ///     availability: None,
    ///     preferences: None,
    /// }));
    ///
    /// assert_eq!(ping.lifecycle.state_name(), "gathering");
    /// assert_eq!(ping.responses().len(), 1);
    /// ```
    pub fn add_response(&mut self, response: Response) {
        match &mut self.lifecycle {
            PingLifecycle::PingSent => {
                self.lifecycle = PingLifecycle::Gathering {
                    responses: vec![response],
                };
            }
            PingLifecycle::Gathering { responses } => {
                responses.push(response);
            }
            PingLifecycle::Matching { .. }
            | PingLifecycle::VenueConfirmed { .. }
            | PingLifecycle::ActiveHangout { .. }
            | PingLifecycle::Complete { .. }
            | PingLifecycle::Cancelled { .. }
            | PingLifecycle::NoMatch { .. } => {}
        }
    }

    /// The answers of the ping; see [`PingLifecycle::responses`].
    pub fn responses(&self) -> &[Response] {
        self.lifecycle.responses()
    }

    /// The answer of a user, if they gave one.
    pub fn find_response(&self, user_id: Uuid) -> Option<&Response> {
        self.responses().iter().find(|r| r.user == user_id)
    }

    /// The answer with this id, to change it. Unlike
    /// [`Ping::find_response`] it looks up the id of the response, not the id
    /// of the user.
    pub fn find_response_mut(&mut self, response_id: Uuid) -> Option<&mut Response> {
        self.lifecycle
            .responses_mut()
            .and_then(|responses| responses.iter_mut().find(|r| r.id == response_id))
    }

    /// Tells whether the user has answered, yes or no.
    pub fn has_user_responded(&self, user_id: Uuid) -> bool {
        self.responses().iter().any(|r| r.user == user_id)
    }

    /// The answers that say yes (`answer == true`), whether or not they give an
    /// availability.
    pub fn positive_responses(&self) -> Vec<&Response> {
        self.responses().iter().filter(|r| r.answer).collect()
    }
}
