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

use super::value::{TimeWindow, TimeWindowError};
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

/// A span of time with a start and an end, both UTC. The end is after the
/// start; the type cannot hold anything else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(try_from = "TimelineWire")]
pub struct Timeline {
    /// When the hangout starts.
    start: DateTime<Utc>,
    /// When the hangout ends. Must be after `start`.
    end: DateTime<Utc>,
}

/// The shape of a [`Timeline`] on the wire, before the rule is checked.
#[derive(Deserialize)]
struct TimelineWire {
    start: DateTime<Utc>,
    end: DateTime<Utc>,
}

impl Timeline {
    /// Makes the timeline from its start and its end.
    ///
    /// # Errors
    ///
    /// Returns a [`TimeWindowError`] when `end` is the same as `start` or
    /// before it.
    ///
    /// # Examples
    ///
    /// ```
    /// use chrono::{TimeZone, Utc};
    /// use sens_o_matic::models::Timeline;
    ///
    /// let at = |hour| Utc.with_ymd_and_hms(2024, 12, 15, hour, 0, 0).unwrap();
    ///
    /// let timeline = Timeline::new(at(18), at(21)).unwrap();
    /// assert_eq!((timeline.start(), timeline.end()), (at(18), at(21)));
    ///
    /// // The end before the start, and a timeline of no length.
    /// assert!(Timeline::new(at(21), at(18)).is_err());
    /// assert!(Timeline::new(at(18), at(18)).is_err());
    ///
    /// // A body is refused where it is read.
    /// let json = r#"{ "start": "2024-12-15T18:00:00Z", "end": "2024-12-15T21:00:00Z" }"#;
    /// assert!(serde_json::from_str::<Timeline>(json).is_ok());
    /// let json = r#"{ "start": "2024-12-15T21:00:00Z", "end": "2024-12-15T18:00:00Z" }"#;
    /// assert!(serde_json::from_str::<Timeline>(json).is_err());
    /// ```
    pub fn new(start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Self, TimeWindowError> {
        TimeWindow::new(start, end).map(Self::from)
    }

    /// When the hangout starts.
    pub fn start(&self) -> DateTime<Utc> {
        self.start
    }

    /// When the hangout ends; always after [`Timeline::start`].
    pub fn end(&self) -> DateTime<Utc> {
        self.end
    }
}

impl From<TimeWindow> for Timeline {
    fn from(window: TimeWindow) -> Self {
        Self {
            start: window.start(),
            end: window.end(),
        }
    }
}

impl TryFrom<TimelineWire> for Timeline {
    type Error = TimeWindowError;

    fn try_from(wire: TimelineWire) -> Result<Self, Self::Error> {
        Self::new(wire.start, wire.end)
    }
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(try_from = "TimeOverlapWire")]
pub struct TimeOverlap {
    /// The start of the common window: the latest of the starts of the
    /// availabilities.
    start: DateTime<Utc>,
    /// The end of the common window: the earliest of the ends of the
    /// availabilities. Always after `start`.
    end: DateTime<Utc>,
    /// How many members the window holds: the yes-answers that gave an
    /// availability, all of which the window fits. The initiator counts only if
    /// they answered their own ping.
    attendee_count: u32,
}

/// The shape of a [`TimeOverlap`] on the wire, before the rule is checked.
#[derive(Deserialize)]
struct TimeOverlapWire {
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    attendee_count: u32,
}

impl TimeOverlap {
    /// Makes the overlap of `attendee_count` members in a common window.
    ///
    /// The window has a length by its type, so unlike [`Timeline::new`] this
    /// cannot fail; [`crate::matching`] gets the window from
    /// [`TimeWindow::new`], which refuses the windows that only touch.
    ///
    /// # Examples
    ///
    /// ```
    /// use chrono::{TimeZone, Utc};
    /// use sens_o_matic::models::{TimeOverlap, value::TimeWindow};
    ///
    /// let at = |hour| Utc.with_ymd_and_hms(2024, 12, 15, hour, 0, 0).unwrap();
    ///
    /// let overlap = TimeOverlap::new(TimeWindow::new(at(18), at(21)).unwrap(), 2);
    /// assert_eq!((overlap.start(), overlap.end()), (at(18), at(21)));
    /// assert_eq!(overlap.attendee_count(), 2);
    ///
    /// // A body is refused where it is read, if its window has no length or
    /// // its count is negative.
    /// let json = |start, end, count| {
    ///     format!(r#"{{ "start": "{start}", "end": "{end}", "attendee_count": {count} }}"#)
    /// };
    /// let ok = json("2024-12-15T18:00:00Z", "2024-12-15T21:00:00Z", 2);
    /// assert!(serde_json::from_str::<TimeOverlap>(&ok).is_ok());
    /// let backwards = json("2024-12-15T21:00:00Z", "2024-12-15T18:00:00Z", 2);
    /// assert!(serde_json::from_str::<TimeOverlap>(&backwards).is_err());
    /// let negative = json("2024-12-15T18:00:00Z", "2024-12-15T21:00:00Z", -1);
    /// assert!(serde_json::from_str::<TimeOverlap>(&negative).is_err());
    /// ```
    pub fn new(window: TimeWindow, attendee_count: u32) -> Self {
        Self {
            start: window.start(),
            end: window.end(),
            attendee_count,
        }
    }

    /// The start of the common window.
    pub fn start(&self) -> DateTime<Utc> {
        self.start
    }

    /// The end of the common window; always after [`TimeOverlap::start`].
    pub fn end(&self) -> DateTime<Utc> {
        self.end
    }

    /// How many members the window holds.
    pub fn attendee_count(&self) -> u32 {
        self.attendee_count
    }
}

impl TryFrom<TimeOverlapWire> for TimeOverlap {
    type Error = TimeWindowError;

    fn try_from(wire: TimeOverlapWire) -> Result<Self, Self::Error> {
        TimeWindow::new(wire.start, wire.end).map(|window| Self::new(window, wire.attendee_count))
    }
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
    /// let timeline = Timeline::new(
    ///     Utc.with_ymd_and_hms(2024, 12, 15, 18, 0, 0).unwrap(),
    ///     Utc.with_ymd_and_hms(2024, 12, 15, 21, 0, 0).unwrap(),
    /// )
    /// .unwrap();
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
