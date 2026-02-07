use uuid::Uuid;

use crate::models::{AppError, HangoutData, MatchResults, Ping, PingLifecycle, Timeline};

pub struct StateMachine;

impl StateMachine {
    pub fn can_add_response(ping: &Ping) -> Result<(), AppError> {
        if !ping.lifecycle.can_add_response() {
            return Err(AppError::Conflict(format!(
                "Cannot add response when ping is in {} state",
                ping.lifecycle.state_name()
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

        if !ping.lifecycle.can_trigger_match() {
            return Err(AppError::Conflict(format!(
                "Cannot trigger match when ping is in {} state",
                ping.lifecycle.state_name()
            )));
        }

        Ok(())
    }

    pub fn can_confirm(ping: &Ping) -> Result<(), AppError> {
        if !ping.lifecycle.can_confirm() {
            return Err(AppError::Conflict(format!(
                "Cannot confirm when ping is in {} state",
                ping.lifecycle.state_name()
            )));
        }
        Ok(())
    }

    pub fn can_activate(ping: &Ping) -> Result<(), AppError> {
        if !ping.lifecycle.can_activate() {
            return Err(AppError::Conflict(format!(
                "Cannot activate when ping is in {} state",
                ping.lifecycle.state_name()
            )));
        }
        Ok(())
    }

    pub fn can_complete(ping: &Ping) -> Result<(), AppError> {
        if !ping.lifecycle.can_complete() {
            return Err(AppError::Conflict(format!(
                "Cannot complete when ping is in {} state",
                ping.lifecycle.state_name()
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

        if !ping.lifecycle.can_cancel() {
            return Err(AppError::Conflict(format!(
                "Cannot cancel when ping is in {} state",
                ping.lifecycle.state_name()
            )));
        }

        Ok(())
    }

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

    pub fn transition_to_venue_confirmed(ping: &mut Ping, hangout: HangoutData) {
        if let PingLifecycle::Matching { responses, .. } = &ping.lifecycle {
            ping.lifecycle = PingLifecycle::VenueConfirmed {
                responses: responses.clone(),
                hangout,
            };
        }
    }

    pub fn transition_to_active(ping: &mut Ping) {
        if let PingLifecycle::VenueConfirmed { responses, hangout } = &ping.lifecycle {
            ping.lifecycle = PingLifecycle::ActiveHangout {
                responses: responses.clone(),
                hangout: hangout.clone(),
            };
        }
    }

    pub fn transition_to_complete(ping: &mut Ping) {
        if let PingLifecycle::ActiveHangout { responses, hangout } = &ping.lifecycle {
            ping.lifecycle = PingLifecycle::Complete {
                responses: responses.clone(),
                hangout: hangout.clone(),
            };
        }
    }

    pub fn transition_to_cancelled(ping: &mut Ping) {
        let responses = ping.lifecycle.responses().to_vec();
        ping.lifecycle = PingLifecycle::Cancelled { responses };
    }

    pub fn create_hangout_data(ping: &Ping, timeline: Timeline) -> HangoutData {
        let attendees: Vec<Uuid> = ping.positive_responses().iter().map(|r| r.user).collect();
        HangoutData::new(attendees, timeline)
    }
}
