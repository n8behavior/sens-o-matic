use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum HangoutStatus {
    Confirmed,
    Active,
    Complete,
}

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

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Hangout {
    pub id: Uuid,
    pub ping: Uuid,
    pub confirmed_attendees: Vec<Uuid>,
    pub timeline: Timeline,
    pub status: HangoutStatus,
    pub attendee_statuses: HashMap<String, AttendeeStatus>,
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

impl Hangout {
    pub fn new(ping_id: Uuid, attendees: Vec<Uuid>, timeline: Timeline) -> Self {
        let attendee_statuses = attendees
            .iter()
            .map(|id| (id.to_string(), AttendeeStatus::Pending))
            .collect();

        Self {
            id: Uuid::new_v4(),
            ping: ping_id,
            confirmed_attendees: attendees,
            timeline,
            status: HangoutStatus::Confirmed,
            attendee_statuses,
        }
    }

    pub fn activate(&mut self) {
        self.status = HangoutStatus::Active;
    }

    pub fn complete(&mut self) {
        self.status = HangoutStatus::Complete;
    }

    pub fn update_attendee_status(&mut self, user_id: Uuid, status: AttendeeStatus) {
        self.attendee_statuses.insert(user_id.to_string(), status);
    }

    pub fn is_attendee(&self, user_id: Uuid) -> bool {
        self.confirmed_attendees.contains(&user_id)
    }
}
