use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

use super::hangout::{HangoutData, MatchResults};
use super::response::Response;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PingLifecycle {
    PingSent,
    Gathering {
        responses: Vec<Response>,
    },
    Matching {
        responses: Vec<Response>,
        match_results: MatchResults,
    },
    VenueConfirmed {
        responses: Vec<Response>,
        hangout: HangoutData,
    },
    ActiveHangout {
        responses: Vec<Response>,
        hangout: HangoutData,
    },
    Complete {
        responses: Vec<Response>,
        hangout: HangoutData,
    },
    Cancelled {
        responses: Vec<Response>,
    },
    NoMatch {
        responses: Vec<Response>,
    },
}

impl PingLifecycle {
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

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            PingLifecycle::Complete { .. }
                | PingLifecycle::Cancelled { .. }
                | PingLifecycle::NoMatch { .. }
        )
    }

    pub fn can_add_response(&self) -> bool {
        matches!(
            self,
            PingLifecycle::PingSent | PingLifecycle::Gathering { .. }
        )
    }

    pub fn can_trigger_match(&self) -> bool {
        matches!(self, PingLifecycle::Gathering { .. })
    }

    pub fn can_confirm(&self) -> bool {
        matches!(self, PingLifecycle::Matching { .. })
    }

    pub fn can_activate(&self) -> bool {
        matches!(self, PingLifecycle::VenueConfirmed { .. })
    }

    pub fn can_complete(&self) -> bool {
        matches!(self, PingLifecycle::ActiveHangout { .. })
    }

    pub fn can_cancel(&self) -> bool {
        !self.is_terminal()
    }

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

    pub fn hangout(&self) -> Option<&HangoutData> {
        match self {
            PingLifecycle::VenueConfirmed { hangout, .. }
            | PingLifecycle::ActiveHangout { hangout, .. }
            | PingLifecycle::Complete { hangout, .. } => Some(hangout),
            _ => None,
        }
    }

    pub fn hangout_mut(&mut self) -> Option<&mut HangoutData> {
        match self {
            PingLifecycle::VenueConfirmed { hangout, .. }
            | PingLifecycle::ActiveHangout { hangout, .. }
            | PingLifecycle::Complete { hangout, .. } => Some(hangout),
            _ => None,
        }
    }

    pub fn match_results(&self) -> Option<&MatchResults> {
        match self {
            PingLifecycle::Matching { match_results, .. } => Some(match_results),
            _ => None,
        }
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
    pub created_at: DateTime<Utc>,
    #[serde(flatten)]
    pub lifecycle: PingLifecycle,
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
            created_at: Utc::now(),
            lifecycle: PingLifecycle::PingSent,
        }
    }

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
            _ => {}
        }
    }

    pub fn responses(&self) -> &[Response] {
        self.lifecycle.responses()
    }

    pub fn find_response(&self, user_id: Uuid) -> Option<&Response> {
        self.responses().iter().find(|r| r.user == user_id)
    }

    pub fn find_response_mut(&mut self, response_id: Uuid) -> Option<&mut Response> {
        self.lifecycle
            .responses_mut()
            .and_then(|responses| responses.iter_mut().find(|r| r.id == response_id))
    }

    pub fn has_user_responded(&self, user_id: Uuid) -> bool {
        self.responses().iter().any(|r| r.user == user_id)
    }

    pub fn positive_responses(&self) -> Vec<&Response> {
        self.responses().iter().filter(|r| r.answer).collect()
    }
}
