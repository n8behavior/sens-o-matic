use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AttendeeStatus {
    Pending,
    Enroute,
    Arrived,
    Left,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Timeline {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

/// Data associated with a hangout phase of a ping.
/// This is a value object, not an entity - the Ping ID is the hangout identifier.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HangoutData {
    pub confirmed_attendees: Vec<Uuid>,
    pub timeline: Timeline,
    pub attendee_statuses: HashMap<Uuid, AttendeeStatus>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct ConfirmHangoutRequest {
    pub user_id: Uuid,
    pub timeline: Timeline,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct UpdateAttendeeStatusRequest {
    pub status: AttendeeStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TimeOverlap {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub attendee_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MatchResults {
    pub ping_id: Uuid,
    pub overlap: Option<TimeOverlap>,
    pub has_match: bool,
}

impl HangoutData {
    pub fn new(attendees: Vec<Uuid>, timeline: Timeline) -> Self {
        let attendee_statuses = attendees
            .iter()
            .map(|id| (*id, AttendeeStatus::Pending))
            .collect();

        Self {
            confirmed_attendees: attendees,
            timeline,
            attendee_statuses,
        }
    }

    pub fn update_attendee_status(&mut self, user_id: Uuid, status: AttendeeStatus) {
        self.attendee_statuses.insert(user_id, status);
    }

    pub fn is_attendee(&self, user_id: Uuid) -> bool {
        self.confirmed_attendees.contains(&user_id)
    }
}
