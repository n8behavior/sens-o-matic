//! The rules of the lifecycle of a ping: who may do what in which state, and
//! the transitions.
//!
//! The states and the actions that each one allows are in
//! [`crate::models::ping`], with the diagram of the lifecycle. This module adds
//! what a model cannot know:
//!
//! - **Who acts.** Only the initiator of a ping can trigger the matching and
//!   cancel it. The other actions are open to anyone who reaches them.
//! - **The refusal.** A `can_*` function checks an action and returns the
//!   [`AppError`] that the API answers with: [`AppError::Forbidden`] for the
//!   wrong user, [`AppError::Conflict`] for the wrong state. The user is
//!   checked first.
//! - **The transitions.** A `transition_to_*` function moves a ping to its next
//!   state and carries the data over.
//!
//! # Check, then transition
//!
//! A `transition_to_*` function does not check anything and does not fail: on a
//! ping that is not in the state it expects, it leaves the ping as it is. The
//! caller runs the matching `can_*` function first and acts on the answer. The
//! one exception is [`StateMachine::transition_to_cancelled`], which acts in
//! any state.
//!
//! A handler checks a copy of the ping and then updates the stored one, in two
//! steps (see [`crate::state`]). Two requests that arrive together can both
//! pass the check; the second transition then finds the ping in another state
//! and does nothing.
//!
//! # A lifecycle from end to end
//!
//! The doc test below sends a ping through every state of the happy path, with
//! the modules that take part: [`crate::models`] for the ping and its answers,
//! [`crate::matching`] for the overlap, and this module for the rules.
//!
//! ```
//! use chrono::{TimeZone, Utc};
//! use sens_o_matic::matching::MatchingEngine;
//! use sens_o_matic::models::{
//!     AppError, Availability, CreatePingRequest, CreateResponseRequest, Ping, Response, Timeline,
//! };
//! use sens_o_matic::state_machine::StateMachine;
//! use uuid::Uuid;
//!
//! # fn main() -> Result<(), AppError> {
//! let initiator = Uuid::new_v4();
//! let friend = Uuid::new_v4();
//! let at = |hour| Utc.with_ymd_and_hms(2024, 12, 15, hour, 0, 0).unwrap();
//!
//! // PingSent: the initiator sends the ping.
//! let mut ping = Ping::new(CreatePingRequest {
//!     initiator,
//!     group: Uuid::new_v4(),
//!     activity_type: "drinks".to_string(),
//!     rough_timing: "tonight".to_string(),
//!     vibe: None,
//! });
//! assert_eq!(ping.lifecycle.state_name(), "ping_sent");
//!
//! // Gathering: a friend says yes, and when they are free.
//! StateMachine::can_add_response(&ping)?;
//! ping.add_response(Response::new(CreateResponseRequest {
//!     user: friend,
//!     answer: true,
//!     availability: Some(Availability { earliest: at(18), latest: at(21) }),
//!     preferences: None,
//! }));
//! assert_eq!(ping.lifecycle.state_name(), "gathering");
//!
//! // Matching: only the initiator can end the gathering.
//! assert!(matches!(
//!     StateMachine::can_trigger_match(&ping, friend),
//!     Err(AppError::Forbidden(_))
//! ));
//! StateMachine::can_trigger_match(&ping, initiator)?;
//! let results = MatchingEngine::calculate_match(&ping);
//! StateMachine::transition_to_matching(&mut ping, results);
//! assert_eq!(ping.lifecycle.state_name(), "matching");
//! assert!(ping.lifecycle.match_results().unwrap().has_match);
//!
//! // VenueConfirmed: the initiator confirms the hangout for the common window.
//! StateMachine::can_confirm(&ping)?;
//! let hangout = StateMachine::create_hangout_data(&ping, Timeline { start: at(18), end: at(21) });
//! StateMachine::transition_to_venue_confirmed(&mut ping, hangout);
//! assert_eq!(ping.lifecycle.state_name(), "venue_confirmed");
//!
//! // ActiveHangout, then Complete.
//! StateMachine::can_activate(&ping)?;
//! StateMachine::transition_to_active(&mut ping);
//! StateMachine::can_complete(&ping)?;
//! StateMachine::transition_to_complete(&mut ping);
//! assert_eq!(ping.lifecycle.state_name(), "complete");
//!
//! // Complete is terminal: nothing is allowed any more.
//! assert!(ping.lifecycle.is_terminal());
//! assert!(matches!(
//!     StateMachine::can_cancel(&ping, initiator),
//!     Err(AppError::Conflict(_))
//! ));
//! # Ok(())
//! # }
//! ```

use uuid::Uuid;

use crate::models::{AppError, HangoutData, MatchResults, Ping, PingLifecycle, Timeline};

/// The checks and the transitions of a ping. It has no data: the type is only
/// a name for the functions.
pub struct StateMachine;

impl StateMachine {
    /// Checks that the ping still takes answers, that is, that it is in
    /// `PingSent` or `Gathering`. The same check guards a change to an answer.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Conflict`] in any other state.
    pub fn can_add_response(ping: &Ping) -> Result<(), AppError> {
        if !ping.lifecycle.can_add_response() {
            return Err(AppError::Conflict(format!(
                "Cannot add response when ping is in {} state",
                ping.lifecycle.state_name()
            )));
        }
        Ok(())
    }

    /// Checks that `user_id` may run the matching: they are the initiator, and
    /// the ping is in `Gathering`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Forbidden`] when the user is not the initiator, and
    /// otherwise [`AppError::Conflict`] when the ping is in another state. A
    /// ping that nobody has answered is in `PingSent`, so it is a conflict.
    ///
    /// # Examples
    ///
    /// ```
    /// use sens_o_matic::models::{AppError, CreatePingRequest, Ping};
    /// use sens_o_matic::state_machine::StateMachine;
    /// use uuid::Uuid;
    ///
    /// let initiator = Uuid::new_v4();
    /// let ping = Ping::new(CreatePingRequest {
    ///     initiator,
    ///     group: Uuid::new_v4(),
    ///     activity_type: "drinks".to_string(),
    ///     rough_timing: "tonight".to_string(),
    ///     vibe: None,
    /// });
    ///
    /// // The user is checked before the state.
    /// let stranger = StateMachine::can_trigger_match(&ping, Uuid::new_v4());
    /// assert!(matches!(stranger, Err(AppError::Forbidden(_))));
    ///
    /// // Nobody has answered yet, so the ping is not in `Gathering`.
    /// let too_early = StateMachine::can_trigger_match(&ping, initiator);
    /// assert!(matches!(too_early, Err(AppError::Conflict(_))));
    /// ```
    pub fn can_trigger_match(ping: &Ping, user_id: Uuid) -> Result<(), AppError> {
        if ping.initiator != user_id {
            return Err(AppError::Forbidden(
                "Only initiator can trigger matching".to_string(),
            ));
        }

        if !ping.lifecycle.can_trigger_match() {
            return Err(AppError::Conflict(format!(
                "Cannot trigger match when ping is in {} state",
                ping.lifecycle.state_name()
            )));
        }

        Ok(())
    }

    /// Checks that the hangout can be confirmed, that is, that the ping is in
    /// `Matching`. It does not check who confirms.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Conflict`] in any other state.
    pub fn can_confirm(ping: &Ping) -> Result<(), AppError> {
        if !ping.lifecycle.can_confirm() {
            return Err(AppError::Conflict(format!(
                "Cannot confirm when ping is in {} state",
                ping.lifecycle.state_name()
            )));
        }
        Ok(())
    }

    /// Checks that the hangout can start, that is, that the ping is in
    /// `VenueConfirmed`. It does not check who starts it.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Conflict`] in any other state.
    pub fn can_activate(ping: &Ping) -> Result<(), AppError> {
        if !ping.lifecycle.can_activate() {
            return Err(AppError::Conflict(format!(
                "Cannot activate when ping is in {} state",
                ping.lifecycle.state_name()
            )));
        }
        Ok(())
    }

    /// Checks that the hangout can end, that is, that the ping is in
    /// `ActiveHangout`. It does not check who ends it.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Conflict`] in any other state.
    pub fn can_complete(ping: &Ping) -> Result<(), AppError> {
        if !ping.lifecycle.can_complete() {
            return Err(AppError::Conflict(format!(
                "Cannot complete when ping is in {} state",
                ping.lifecycle.state_name()
            )));
        }
        Ok(())
    }

    /// Checks that `user_id` may cancel the ping: they are the initiator, and
    /// the ping is not in a terminal state.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Forbidden`] when the user is not the initiator, and
    /// otherwise [`AppError::Conflict`] when the ping is `Complete`,
    /// `Cancelled` or `NoMatch`.
    ///
    /// # Examples
    ///
    /// ```
    /// use sens_o_matic::models::{AppError, CreatePingRequest, Ping};
    /// use sens_o_matic::state_machine::StateMachine;
    /// use uuid::Uuid;
    ///
    /// let initiator = Uuid::new_v4();
    /// let mut ping = Ping::new(CreatePingRequest {
    ///     initiator,
    ///     group: Uuid::new_v4(),
    ///     activity_type: "drinks".to_string(),
    ///     rough_timing: "tonight".to_string(),
    ///     vibe: None,
    /// });
    ///
    /// let stranger = StateMachine::can_cancel(&ping, Uuid::new_v4());
    /// assert!(matches!(stranger, Err(AppError::Forbidden(_))));
    ///
    /// assert!(StateMachine::can_cancel(&ping, initiator).is_ok());
    /// StateMachine::transition_to_cancelled(&mut ping);
    ///
    /// // A cancelled ping is terminal, so it cannot be cancelled again.
    /// let again = StateMachine::can_cancel(&ping, initiator);
    /// assert!(matches!(again, Err(AppError::Conflict(_))));
    /// ```
    pub fn can_cancel(ping: &Ping, user_id: Uuid) -> Result<(), AppError> {
        if ping.initiator != user_id {
            return Err(AppError::Forbidden(
                "Only initiator can cancel ping".to_string(),
            ));
        }

        if !ping.lifecycle.can_cancel() {
            return Err(AppError::Conflict(format!(
                "Cannot cancel when ping is in {} state",
                ping.lifecycle.state_name()
            )));
        }

        Ok(())
    }

    /// Ends the gathering: moves a `Gathering` ping to `Matching` when the
    /// results have a match, and to `NoMatch` when they do not. Does nothing to
    /// a ping in another state.
    ///
    /// `Matching` keeps the results; `NoMatch` drops them, because they say
    /// only that there is no match.
    ///
    /// # Examples
    ///
    /// A member who says no leaves nobody to match, so the ping ends in
    /// `NoMatch`:
    ///
    /// ```
    /// use sens_o_matic::matching::MatchingEngine;
    /// use sens_o_matic::models::{CreatePingRequest, CreateResponseRequest, Ping, Response};
    /// use sens_o_matic::state_machine::StateMachine;
    /// use uuid::Uuid;
    ///
    /// let mut ping = Ping::new(CreatePingRequest {
    ///     initiator: Uuid::new_v4(),
    ///     group: Uuid::new_v4(),
    ///     activity_type: "drinks".to_string(),
    ///     rough_timing: "tonight".to_string(),
    ///     vibe: None,
    /// });
    /// ping.add_response(Response::new(CreateResponseRequest {
    ///     user: Uuid::new_v4(),
    ///     answer: false,
    ///     availability: None,
    ///     preferences: None,
    /// }));
    ///
    /// let results = MatchingEngine::calculate_match(&ping);
    /// StateMachine::transition_to_matching(&mut ping, results);
    ///
    /// assert_eq!(ping.lifecycle.state_name(), "no_match");
    /// assert!(ping.lifecycle.is_terminal());
    /// ```
    pub fn transition_to_matching(ping: &mut Ping, match_results: MatchResults) {
        if match_results.has_match {
            if let PingLifecycle::Gathering { responses } = &ping.lifecycle {
                ping.lifecycle = PingLifecycle::Matching {
                    responses: responses.clone(),
                    match_results,
                };
            }
        } else if let PingLifecycle::Gathering { responses } = &ping.lifecycle {
            ping.lifecycle = PingLifecycle::NoMatch {
                responses: responses.clone(),
            };
        }
    }

    /// Confirms the hangout: moves a `Matching` ping to `VenueConfirmed` with
    /// the hangout data. Does nothing to a ping in another state. The match
    /// results are dropped.
    pub fn transition_to_venue_confirmed(ping: &mut Ping, hangout: HangoutData) {
        if let PingLifecycle::Matching { responses, .. } = &ping.lifecycle {
            ping.lifecycle = PingLifecycle::VenueConfirmed {
                responses: responses.clone(),
                hangout,
            };
        }
    }

    /// Starts the hangout: moves a `VenueConfirmed` ping to `ActiveHangout`.
    /// Does nothing to a ping in another state.
    pub fn transition_to_active(ping: &mut Ping) {
        if let PingLifecycle::VenueConfirmed { responses, hangout } = &ping.lifecycle {
            ping.lifecycle = PingLifecycle::ActiveHangout {
                responses: responses.clone(),
                hangout: hangout.clone(),
            };
        }
    }

    /// Ends the hangout: moves an `ActiveHangout` ping to `Complete`. Does
    /// nothing to a ping in another state.
    pub fn transition_to_complete(ping: &mut Ping) {
        if let PingLifecycle::ActiveHangout { responses, hangout } = &ping.lifecycle {
            ping.lifecycle = PingLifecycle::Complete {
                responses: responses.clone(),
                hangout: hangout.clone(),
            };
        }
    }

    /// Cancels the ping, keeping its answers. The hangout, if there was one, is
    /// dropped.
    ///
    /// Unlike the other transitions it acts in every state, a terminal one
    /// included: called on a `Complete` ping it makes it `Cancelled`. Only
    /// [`StateMachine::can_cancel`] stops that, so call it first.
    pub fn transition_to_cancelled(ping: &mut Ping) {
        let responses = ping.lifecycle.responses().to_vec();
        ping.lifecycle = PingLifecycle::Cancelled { responses };
    }

    /// Builds the hangout of a ping from its answers: the attendees are all the
    /// members who said yes, every one `pending`.
    ///
    /// A yes without an availability counts as an attendee here, although
    /// [`crate::matching`] ignored it when it looked for a common time. The
    /// initiator is an attendee only if they answered their own ping.
    pub fn create_hangout_data(ping: &Ping, timeline: Timeline) -> HangoutData {
        let attendees: Vec<Uuid> = ping.positive_responses().iter().map(|r| r.user).collect();
        HangoutData::new(attendees, timeline)
    }
}
