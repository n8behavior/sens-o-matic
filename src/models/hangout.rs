//! What the matching found, and the hangout that is built on it.
//!
//! [`MatchResults`] is the output of [`crate::matching`]: whether the answers
//! of a ping have a time in common, and which. When the initiator confirms the
//! match, the ping gets a [`HangoutData`] with the agreed [`Timeline`] and the
//! list of attendees, whose [`AttendeeStatus`] then follows them to the venue.
//!
//! A hangout is not an entity of its own. It is the data of a ping in one of
//! the states that carry it (see [`PingLifecycle::hangout`](super::PingLifecycle::hangout)),
//! and the id of the ping is the id of the hangout.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::{PingId, UserId};

/// Where an attendee is on the way to, at or from the hangout.
///
/// Every attendee starts as `pending`. Any status can follow any other: the
/// server does not enforce an order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AttendeeStatus {
    /// Has not said anything yet.
    Pending,
    /// On the way.
    Enroute,
    /// There.
    Arrived,
    /// Gone.
    Left,
}

/// A span of time with a start and an end, both UTC.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Timeline {
    /// When the hangout starts.
    pub start: DateTime<Utc>,
    /// When the hangout ends. The server does not check that it is after
    /// `start`.
    pub end: DateTime<Utc>,
}

/// Data associated with a hangout phase of a ping.
/// This is a value object, not an entity - the Ping ID is the hangout identifier.
///
/// `attendee_statuses` has one entry for each of `confirmed_attendees`, and the
/// server sets statuses only for attendees.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HangoutData {
    /// The users who said yes before the match was confirmed. The initiator is
    /// in the list only if they answered their own ping.
    #[schema(value_type = Vec<String>, format = "uuid")]
    pub confirmed_attendees: Vec<UserId>,
    /// When the hangout takes place, as the initiator confirmed it.
    pub timeline: Timeline,
    /// The status of each attendee, by user id.
    pub attendee_statuses: HashMap<UserId, AttendeeStatus>,
}

/// The body of the request that confirms a match as a hangout.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct ConfirmHangoutRequest {
    /// The user who confirms. The server does not check it against the
    /// initiator of the ping.
    #[schema(value_type = String, format = "uuid")]
    pub user_id: UserId,
    /// When the hangout takes place. The server stores it as it is: it does not
    /// compare it with the overlap that the matching found.
    pub timeline: Timeline,
}

/// The body of the request that sets the status of an attendee.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct UpdateAttendeeStatusRequest {
    /// The new status.
    pub status: AttendeeStatus,
}

/// The time window in which all the interested members are free: the
/// intersection of the availabilities of the members who said yes.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TimeOverlap {
    /// The start of the common window: the latest of the starts of the
    /// availabilities.
    pub start: DateTime<Utc>,
    /// The end of the common window: the earliest of the ends of the
    /// availabilities. Always after `start`.
    pub end: DateTime<Utc>,
    /// How many members the window holds: the yes-answers that gave an
    /// availability, all of which the window fits. The initiator counts only if
    /// they answered their own ping.
    pub attendee_count: i32,
}

/// The outcome of the matching of a ping.
///
/// `has_match` is true exactly when `overlap` is present.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MatchResults {
    /// The ping that was matched.
    #[schema(value_type = String, format = "uuid")]
    pub ping_id: PingId,
    /// The common window, or `null` when there is none.
    pub overlap: Option<TimeOverlap>,
    /// Whether the members have a time in common.
    pub has_match: bool,
}

impl HangoutData {
    /// Creates the data of a hangout. Every attendee starts as
    /// [`AttendeeStatus::Pending`].
    ///
    /// # Examples
    ///
    /// ```
    /// use chrono::{TimeZone, Utc};
    /// use sens_o_matic::models::{AttendeeStatus, HangoutData, Timeline, UserId};
    ///
    /// let attendee = UserId::random();
    /// let timeline = Timeline {
    ///     start: Utc.with_ymd_and_hms(2024, 12, 15, 18, 0, 0).unwrap(),
    ///     end: Utc.with_ymd_and_hms(2024, 12, 15, 21, 0, 0).unwrap(),
    /// };
    ///
    /// let hangout = HangoutData::new(vec![attendee], timeline);
    /// assert_eq!(hangout.attendee_statuses[&attendee], AttendeeStatus::Pending);
    /// assert!(hangout.is_attendee(attendee));
    /// assert!(!hangout.is_attendee(UserId::random()));
    /// ```
    pub fn new(attendees: Vec<UserId>, timeline: Timeline) -> Self {
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

    /// Sets the status of a user.
    ///
    /// It does not check that the user is an attendee: the caller does, with
    /// [`HangoutData::is_attendee`]. A status set for someone else would be in
    /// `attendee_statuses` but not in `confirmed_attendees`.
    pub fn update_attendee_status(&mut self, user_id: UserId, status: AttendeeStatus) {
        self.attendee_statuses.insert(user_id, status);
    }

    /// Tells whether the user is one of the confirmed attendees.
    pub fn is_attendee(&self, user_id: UserId) -> bool {
        self.confirmed_attendees.contains(&user_id)
    }
}
