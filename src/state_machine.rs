use uuid::Uuid;

use crate::models::{AppError, Hangout, Ping, PingState, Timeline};
use crate::state::AppState;

pub struct StateMachine;

impl StateMachine {
    pub fn can_add_response(ping: &Ping) -> Result<(), AppError> {
        if !ping.state.can_add_response() {
            return Err(AppError::Conflict(format!(
                "Cannot add response when ping is in {:?} state",
                ping.state
            )));
        }
        Ok(())
    }

    pub fn can_trigger_match(ping: &Ping, user_id: Uuid) -> Result<(), AppError> {
        if ping.initiator != user_id {
            return Err(AppError::Forbidden(
                "Only initiator can trigger matching".to_string(),
            ));
        }

        if !ping.state.can_trigger_match() {
            return Err(AppError::Conflict(format!(
                "Cannot trigger match when ping is in {:?} state",
                ping.state
            )));
        }

        Ok(())
    }

    pub fn can_confirm(ping: &Ping) -> Result<(), AppError> {
        if !ping.state.can_confirm() {
            return Err(AppError::Conflict(format!(
                "Cannot confirm when ping is in {:?} state",
                ping.state
            )));
        }
        Ok(())
    }

    pub fn can_cancel(ping: &Ping, user_id: Uuid) -> Result<(), AppError> {
        if ping.initiator != user_id {
            return Err(AppError::Forbidden(
                "Only initiator can cancel ping".to_string(),
            ));
        }

        if !ping.state.can_cancel() {
            return Err(AppError::Conflict(format!(
                "Cannot cancel when ping is in {:?} state",
                ping.state
            )));
        }

        Ok(())
    }

    pub fn transition_to_matching(ping: &mut Ping, has_match: bool) {
        if has_match {
            ping.state = PingState::Matching;
        } else {
            ping.state = PingState::NoMatch;
        }
    }

    pub fn transition_to_venue_confirmed(ping: &mut Ping, hangout_id: Uuid) {
        ping.state = PingState::VenueConfirmed;
        ping.hangout_id = Some(hangout_id);
    }

    pub fn transition_to_active(ping: &mut Ping) {
        ping.state = PingState::ActiveHangout;
    }

    pub fn transition_to_complete(ping: &mut Ping) {
        ping.state = PingState::Complete;
    }

    pub fn transition_to_cancelled(ping: &mut Ping) {
        ping.state = PingState::Cancelled;
    }

    pub fn create_hangout(ping: &Ping, timeline: Timeline) -> Hangout {
        let attendees: Vec<Uuid> = ping.positive_responses().iter().map(|r| r.user).collect();
        Hangout::new(ping.id, attendees, timeline)
    }

    pub fn can_activate_hangout(hangout: &Hangout) -> Result<(), AppError> {
        if hangout.status != crate::models::HangoutStatus::Confirmed {
            return Err(AppError::Conflict(
                "Hangout is not in confirmed state".to_string(),
            ));
        }
        Ok(())
    }

    pub fn can_complete_hangout(hangout: &Hangout) -> Result<(), AppError> {
        if hangout.status != crate::models::HangoutStatus::Active {
            return Err(AppError::Conflict(
                "Hangout is not in active state".to_string(),
            ));
        }
        Ok(())
    }

    pub fn sync_ping_state_from_hangout(state: &AppState, hangout: &Hangout) {
        let status = hangout.status;
        state.pings.update(&hangout.ping, |ping| {
            match status {
                crate::models::HangoutStatus::Active => {
                    ping.state = PingState::ActiveHangout;
                }
                crate::models::HangoutStatus::Complete => {
                    ping.state = PingState::Complete;
                }
                crate::models::HangoutStatus::Confirmed => {
                    // Already set during confirm
                }
            }
        });
    }
}
