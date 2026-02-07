use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

use super::response::Response;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PingState {
    PingSent,
    Gathering,
    Matching,
    VenueConfirmed,
    ActiveHangout,
    Complete,
    Cancelled,
    NoMatch,
}

impl PingState {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            PingState::Complete | PingState::Cancelled | PingState::NoMatch
        )
    }

    pub fn can_add_response(&self) -> bool {
        matches!(self, PingState::PingSent | PingState::Gathering)
    }

    pub fn can_trigger_match(&self) -> bool {
        matches!(self, PingState::Gathering)
    }

    pub fn can_confirm(&self) -> bool {
        matches!(self, PingState::Matching)
    }

    pub fn can_cancel(&self) -> bool {
        !self.is_terminal()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Ping {
    pub id: Uuid,
    pub initiator: Uuid,
    pub group: Uuid,
    pub activity_type: String,
    pub rough_timing: String,
    pub vibe: Option<String>,
    pub state: PingState,
    pub responses: Vec<Response>,
    pub hangout_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, ToSchema, Validate)]
pub struct CreatePingRequest {
    pub initiator: Uuid,
    pub group: Uuid,
    #[validate(length(min = 1, max = 50))]
    pub activity_type: String,
    #[validate(length(min = 1, max = 50))]
    pub rough_timing: String,
    #[validate(length(max = 100))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vibe: Option<String>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct CancelPingRequest {
    pub user_id: Uuid,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct TriggerMatchRequest {
    pub user_id: Uuid,
}

impl Ping {
    pub fn new(request: CreatePingRequest) -> Self {
        Self {
            id: Uuid::new_v4(),
            initiator: request.initiator,
            group: request.group,
            activity_type: request.activity_type,
            rough_timing: request.rough_timing,
            vibe: request.vibe,
            state: PingState::PingSent,
            responses: Vec::new(),
            hangout_id: None,
            created_at: Utc::now(),
        }
    }

    pub fn add_response(&mut self, response: Response) {
        self.responses.push(response);
        if self.state == PingState::PingSent {
            self.state = PingState::Gathering;
        }
    }

    pub fn find_response(&self, user_id: Uuid) -> Option<&Response> {
        self.responses.iter().find(|r| r.user == user_id)
    }

    pub fn find_response_mut(&mut self, response_id: Uuid) -> Option<&mut Response> {
        self.responses.iter_mut().find(|r| r.id == response_id)
    }

    pub fn has_user_responded(&self, user_id: Uuid) -> bool {
        self.responses.iter().any(|r| r.user == user_id)
    }

    pub fn positive_responses(&self) -> Vec<&Response> {
        self.responses.iter().filter(|r| r.answer).collect()
    }
}
