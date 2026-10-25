//! What the matching found, and the hangout that is built on it.
//!
//! [`MatchResults`] is the output of [`crate::matching`]: whether the answers
//! of a ping have a time in common, and which. When the initiator confirms the
//! match, the ping gets a [`HangoutData`] with the agreed [`Timeline`] and the
//! [`Attendees`], whose [`AttendeeStatus`] then follows them to the venue.
//!
//! A hangout is not an entity of its own. It is the data of a ping in one of
//! the states that carry it (see [`PingLifecycle::hangout`](super::PingLifecycle::hangout)),
//! and the id of the ping is the id of the hangout.
//!
//! # One fact, one place
//!
//! The JSON of this module says two facts twice: `has_match` repeats whether
//! `overlap` is present, and `attendee_statuses` has the users of
//! `confirmed_attendees` again. The types do not: [`MatchResults`] stores the
//! overlap and works `has_match` out from it, and [`Attendees`] is one list of
//! users with their statuses, so there is no status without an attendee and no
//! attendee without a status.
//!
//! The JSON keeps both fields. A private struct for each type, `MatchResultsWire`
//! and `HangoutDataWire`, has the fields as they are on the wire. It is what
//! serde writes and reads, and what the OpenAPI document describes, so the
//! document and the JSON stay what they were. A body whose copies disagree is
//! refused where it is read.

use std::{borrow::Cow, collections::HashMap};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{
    PartialSchema, ToSchema,
    openapi::{RefOr, Schema},
};

use super::value::{TimeWindow, TimeWindowError};
use super::{PingId, UserId};

/// Gives `$model` the OpenAPI schema of `$wire`: the schema of the JSON, which
/// is not the shape of the model (see the [module](self)).
///
/// `$wire` derives `ToSchema` with `#[schema(as = ...)]`, so its name is the
/// name of `$model`, and a reference to `$model` points at the schema of
/// `$wire`.
macro_rules! schema_of_wire {
    ($model:ty => $wire:ty) => {
        impl PartialSchema for $model {
            fn schema() -> RefOr<Schema> {
                <$wire>::schema()
            }
        }

        impl ToSchema for $model {
            fn name() -> Cow<'static, str> {
                <$wire>::name()
            }

            fn schemas(schemas: &mut Vec<(String, RefOr<Schema>)>) {
                <$wire>::schemas(schemas)
            }
        }
    };
}

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

/// The users who come to a hangout, each with their [`AttendeeStatus`].
///
/// The users and the statuses are one list of pairs, so an attendee always has
/// a status and a status always belongs to an attendee. [`Attendees::set_status`]
/// refuses a user who is not in the list, and nothing else adds or removes one.
/// The order is the order of [`Attendees::new`]. A lookup walks the list, which
/// is as long as the people who come to one hangout.
///
/// # Examples
///
/// ```
/// use sens_o_matic::models::{AttendeeStatus, Attendees, UserId};
///
/// let (ada, grace) = (UserId::random(), UserId::random());
/// let mut attendees = Attendees::new([ada, grace]);
/// assert_eq!(attendees.status(ada), Some(AttendeeStatus::Pending));
///
/// attendees.set_status(ada, AttendeeStatus::Enroute).unwrap();
/// assert_eq!(attendees.status(ada), Some(AttendeeStatus::Enroute));
///
/// // A status for someone who is not an attendee cannot be stored.
/// let stranger = UserId::random();
/// assert!(attendees.set_status(stranger, AttendeeStatus::Arrived).is_err());
/// assert!(!attendees.contains(stranger));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attendees(Vec<(UserId, AttendeeStatus)>);

/// The user is not an attendee, so they have no status to set. See
/// [`Attendees::set_status`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "user {0} is not an attendee of the hangout, so they have no status to set; \
     only an attendee has one. See sens_o_matic::models::hangout::Attendees"
)]
pub struct NotAnAttendee(pub UserId);

/// The attendees and the statuses of a body do not describe the same users. See
/// [`HangoutData`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AttendeesError {
    /// The user is in `confirmed_attendees` twice.
    #[error(
        "user {0} is in confirmed_attendees twice; an attendee is listed once. \
         See sens_o_matic::models::hangout::Attendees"
    )]
    Duplicate(UserId),
    /// The user has a status but is not in `confirmed_attendees`.
    #[error(
        "user {0} has an entry in attendee_statuses but is not in confirmed_attendees; \
         only an attendee has a status. See sens_o_matic::models::hangout::Attendees"
    )]
    StatusWithoutAttendee(UserId),
    /// The user is in `confirmed_attendees` but has no status.
    #[error(
        "user {0} is in confirmed_attendees but has no entry in attendee_statuses; \
         every attendee has a status. See sens_o_matic::models::hangout::Attendees"
    )]
    AttendeeWithoutStatus(UserId),
}

impl Attendees {
    /// Makes the attendees of a hangout, every one [`AttendeeStatus::Pending`].
    /// A user who is given twice counts once, at the place of the first.
    pub fn new(users: impl IntoIterator<Item = UserId>) -> Self {
        let mut attendees = Vec::new();
        for user in users {
            if !attendees.iter().any(|&(id, _)| id == user) {
                attendees.push((user, AttendeeStatus::Pending));
            }
        }
        Self(attendees)
    }

    /// Makes the attendees from the two views that the JSON has of them, and
    /// refuses a body in which the views name different users.
    fn from_wire(
        users: Vec<UserId>,
        mut statuses: HashMap<UserId, AttendeeStatus>,
    ) -> Result<Self, AttendeesError> {
        let mut attendees: Vec<(UserId, AttendeeStatus)> = Vec::with_capacity(users.len());
        for user in users {
            if attendees.iter().any(|&(id, _)| id == user) {
                return Err(AttendeesError::Duplicate(user));
            }
            let status = statuses
                .remove(&user)
                .ok_or(AttendeesError::AttendeeWithoutStatus(user))?;
            attendees.push((user, status));
        }
        match statuses.into_keys().next() {
            Some(stranger) => Err(AttendeesError::StatusWithoutAttendee(stranger)),
            None => Ok(Self(attendees)),
        }
    }

    /// Tells whether the user is an attendee.
    pub fn contains(&self, user: UserId) -> bool {
        self.status(user).is_some()
    }

    /// The status of the user, or `None` when they are not an attendee.
    pub fn status(&self, user: UserId) -> Option<AttendeeStatus> {
        self.0
            .iter()
            .find(|&&(id, _)| id == user)
            .map(|&(_, status)| status)
    }

    /// Sets the status of an attendee. Any status can follow any other.
    ///
    /// # Errors
    ///
    /// Returns [`NotAnAttendee`] when the user is not an attendee, and changes
    /// nothing: the only way to have a status is to be an attendee.
    pub fn set_status(
        &mut self,
        user: UserId,
        status: AttendeeStatus,
    ) -> Result<(), NotAnAttendee> {
        let (_, current) = self
            .0
            .iter_mut()
            .find(|(id, _)| *id == user)
            .ok_or(NotAnAttendee(user))?;
        *current = status;
        Ok(())
    }

    /// The attendees with their statuses, in order.
    pub fn iter(&self) -> impl Iterator<Item = (UserId, AttendeeStatus)> + '_ {
        self.0.iter().copied()
    }
}

/// The people who come to the hangout and when it takes place. It is the data of
/// a ping that was confirmed; see the [module](self).
///
/// The attendees and their statuses are one [`Attendees`], and in JSON they are
/// the two fields `confirmed_attendees` (an array, in the order of the
/// attendees) and `attendee_statuses` (an object, by user id). A body in which
/// the two name different users is refused where it is read.
///
/// # Examples
///
/// ```
/// use chrono::{TimeZone, Utc};
/// use sens_o_matic::models::{AttendeeStatus, HangoutData, Timeline, UserId};
///
/// let (ada, grace) = (UserId::random(), UserId::random());
/// let timeline = Timeline::new(
///     Utc.with_ymd_and_hms(2024, 12, 15, 18, 0, 0).unwrap(),
///     Utc.with_ymd_and_hms(2024, 12, 15, 21, 0, 0).unwrap(),
/// )
/// .unwrap();
/// let hangout = HangoutData::new(vec![ada, grace], timeline);
///
/// let json = serde_json::to_value(&hangout).unwrap();
/// assert_eq!(json["confirmed_attendees"], serde_json::json!([ada, grace]));
/// assert_eq!(json["attendee_statuses"][ada.to_string()], "pending");
///
/// // A status for a user who is not in `confirmed_attendees` is refused.
/// let mut stranger = json.clone();
/// stranger["attendee_statuses"][UserId::random().to_string()] = "arrived".into();
/// assert!(serde_json::from_value::<HangoutData>(stranger).is_err());
/// assert_eq!(serde_json::from_value::<HangoutData>(json).unwrap(), hangout);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "HangoutDataWire", into = "HangoutDataWire")]
pub struct HangoutData {
    /// The users who said yes before the match was confirmed, with their
    /// statuses.
    pub attendees: Attendees,
    /// When the hangout takes place.
    pub timeline: Timeline,
}

/// Data associated with a hangout phase of a ping.
/// This is a value object, not an entity - the Ping ID is the hangout identifier.
///
/// `attendee_statuses` has one entry for each of `confirmed_attendees`, and the
/// server sets statuses only for attendees.
#[derive(Serialize, Deserialize, ToSchema)]
#[schema(as = HangoutData)]
struct HangoutDataWire {
    /// The users who said yes before the match was confirmed. The initiator is
    /// in the list only if they answered their own ping.
    #[schema(value_type = Vec<String>, format = "uuid")]
    confirmed_attendees: Vec<UserId>,
    /// When the hangout takes place, as the initiator confirmed it.
    timeline: Timeline,
    /// The status of each attendee, by user id.
    attendee_statuses: HashMap<UserId, AttendeeStatus>,
}

schema_of_wire!(HangoutData => HangoutDataWire);

impl From<HangoutData> for HangoutDataWire {
    fn from(hangout: HangoutData) -> Self {
        Self {
            confirmed_attendees: hangout.attendees.iter().map(|(user, _)| user).collect(),
            timeline: hangout.timeline,
            attendee_statuses: hangout.attendees.iter().collect(),
        }
    }
}

impl TryFrom<HangoutDataWire> for HangoutData {
    type Error = AttendeesError;

    fn try_from(wire: HangoutDataWire) -> Result<Self, Self::Error> {
        Ok(Self {
            attendees: Attendees::from_wire(wire.confirmed_attendees, wire.attendee_statuses)?,
            timeline: wire.timeline,
        })
    }
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

/// The outcome of the matching of a ping: the common window, if there is one.
///
/// Whether the members have a time in common is [`MatchResults::has_match`],
/// worked out from `overlap`, so the two cannot disagree. In JSON it is the
/// field `has_match`, next to `overlap`, and a body in which they disagree is
/// refused where it is read.
///
/// # Examples
///
/// ```
/// use chrono::{TimeZone, Utc};
/// use sens_o_matic::models::{MatchResults, PingId, TimeOverlap, value::TimeWindow};
///
/// let at = |hour| Utc.with_ymd_and_hms(2024, 12, 15, hour, 0, 0).unwrap();
/// let ping_id = PingId::random();
///
/// let window = TimeWindow::new(at(18), at(21)).unwrap();
/// let found = MatchResults { ping_id, overlap: Some(TimeOverlap::new(window, 2)) };
/// let none = MatchResults { ping_id, overlap: None };
/// assert!(found.has_match());
/// assert!(!none.has_match());
///
/// // In JSON `has_match` is a field, and it has to agree with `overlap`.
/// assert_eq!(serde_json::to_value(&none).unwrap()["has_match"], false);
/// let json = format!(r#"{{ "ping_id": "{ping_id}", "overlap": null, "has_match": true }}"#);
/// assert!(serde_json::from_str::<MatchResults>(&json).is_err());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "MatchResultsWire", into = "MatchResultsWire")]
pub struct MatchResults {
    /// The ping that was matched.
    pub ping_id: PingId,
    /// The common window, or `None` when there is none.
    pub overlap: Option<TimeOverlap>,
}

/// The `has_match` of a body and its `overlap` disagree. See [`MatchResults`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MatchResultsError {
    /// `has_match` is `true` and `overlap` is `null`.
    #[error(
        "has_match is true but overlap is null; has_match is true exactly when overlap is \
         present. See sens_o_matic::models::hangout::MatchResults"
    )]
    MatchWithoutOverlap,
    /// `has_match` is `false` and `overlap` is a window.
    #[error(
        "has_match is false but overlap is present; has_match is true exactly when overlap is \
         present. See sens_o_matic::models::hangout::MatchResults"
    )]
    OverlapWithoutMatch,
}

impl MatchResults {
    /// Tells whether the members have a time in common, that is, whether there
    /// is an `overlap`.
    pub fn has_match(&self) -> bool {
        self.overlap.is_some()
    }
}

/// The outcome of the matching of a ping.
///
/// `has_match` is true exactly when `overlap` is present.
#[derive(Serialize, Deserialize, ToSchema)]
#[schema(as = MatchResults)]
struct MatchResultsWire {
    /// The ping that was matched.
    #[schema(value_type = String, format = "uuid")]
    ping_id: PingId,
    /// The common window, or `null` when there is none.
    overlap: Option<TimeOverlap>,
    /// Whether the members have a time in common.
    has_match: bool,
}

schema_of_wire!(MatchResults => MatchResultsWire);

impl From<MatchResults> for MatchResultsWire {
    fn from(results: MatchResults) -> Self {
        Self {
            ping_id: results.ping_id,
            has_match: results.has_match(),
            overlap: results.overlap,
        }
    }
}

impl TryFrom<MatchResultsWire> for MatchResults {
    type Error = MatchResultsError;

    fn try_from(wire: MatchResultsWire) -> Result<Self, Self::Error> {
        match (wire.has_match, wire.overlap) {
            (true, None) => Err(MatchResultsError::MatchWithoutOverlap),
            (false, Some(_)) => Err(MatchResultsError::OverlapWithoutMatch),
            (_, overlap) => Ok(Self {
                ping_id: wire.ping_id,
                overlap,
            }),
        }
    }
}

impl HangoutData {
    /// Creates the data of a hangout. Every attendee starts as
    /// [`AttendeeStatus::Pending`], and a user who is given twice is an
    /// attendee once.
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
    /// assert_eq!(hangout.attendees.status(attendee), Some(AttendeeStatus::Pending));
    /// assert!(!hangout.attendees.contains(UserId::random()));
    /// ```
    pub fn new(attendees: Vec<UserId>, timeline: Timeline) -> Self {
        Self {
            attendees: Attendees::new(attendees),
            timeline,
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use serde_json::json;

    use super::*;
    use crate::models::{CreatePingRequest, GroupId, Ping, PingLifecycle};

    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2024, 12, 15, hour, 0, 0).unwrap()
    }

    fn timeline() -> Timeline {
        Timeline::new(at(18), at(21)).unwrap()
    }

    fn overlap() -> TimeOverlap {
        TimeOverlap::new(TimeWindow::new(at(18), at(21)).unwrap(), 2)
    }

    /// The test that the issue names: the type holds no status for a user who
    /// is not an attendee, whichever way the status comes in.
    #[test]
    fn a_status_for_a_non_attendee_cannot_be_stored() {
        let (ada, grace, stranger) = (UserId::random(), UserId::random(), UserId::random());
        let mut hangout = HangoutData::new(vec![ada, grace], timeline());
        let before = hangout.clone();

        // The call returns an error, and the hangout is as it was.
        assert_eq!(
            hangout
                .attendees
                .set_status(stranger, AttendeeStatus::Arrived),
            Err(NotAnAttendee(stranger))
        );
        assert_eq!(hangout, before);
        assert_eq!(hangout.attendees.status(stranger), None);

        // The JSON has the same users in both fields, and not the stranger.
        let json = serde_json::to_value(&hangout).unwrap();
        assert_eq!(json["confirmed_attendees"], json!([ada, grace]));
        assert_eq!(json["attendee_statuses"].as_object().unwrap().len(), 2);
        assert!(
            json["attendee_statuses"]
                .get(stranger.to_string())
                .is_none()
        );

        // The other way in is a body, and it is refused where it is read.
        let mut body = json;
        body["attendee_statuses"][stranger.to_string()] = json!("arrived");
        let refused = serde_json::from_value::<HangoutData>(body).unwrap_err();
        assert!(
            refused.to_string().contains(&stranger.to_string()),
            "the message names the user: {refused}"
        );
    }

    #[test]
    fn a_status_for_an_attendee_is_stored() {
        let (ada, grace) = (UserId::random(), UserId::random());
        let mut hangout = HangoutData::new(vec![ada, grace], timeline());

        hangout
            .attendees
            .set_status(grace, AttendeeStatus::Enroute)
            .unwrap();

        assert_eq!(hangout.attendees.status(ada), Some(AttendeeStatus::Pending));
        assert_eq!(
            hangout.attendees.status(grace),
            Some(AttendeeStatus::Enroute)
        );
    }

    #[test]
    fn an_attendee_is_listed_once() {
        let ada = UserId::random();
        let hangout = HangoutData::new(vec![ada, ada], timeline());

        let json = serde_json::to_value(&hangout).unwrap();
        assert_eq!(json["confirmed_attendees"], json!([ada]));

        let mut body = json;
        body["confirmed_attendees"] = json!([ada, ada]);
        assert!(serde_json::from_value::<HangoutData>(body).is_err());
    }

    #[test]
    fn an_attendee_without_a_status_is_refused() {
        let (ada, grace) = (UserId::random(), UserId::random());
        let body = json!({
            "confirmed_attendees": [ada, grace],
            "timeline": { "start": at(18), "end": at(21) },
            "attendee_statuses": { ada.to_string(): "pending" },
        });
        let refused = serde_json::from_value::<HangoutData>(body).unwrap_err();
        assert!(refused.to_string().contains(&grace.to_string()));
    }

    /// The JSON of a hangout is what it was before the attendees and their
    /// statuses became one list: the attendees as an array in their order, and
    /// the statuses as an object.
    #[test]
    fn the_json_of_a_hangout_keeps_its_shape() {
        let users: Vec<UserId> = (0..5).map(|_| UserId::random()).collect();
        let mut hangout = HangoutData::new(users.clone(), timeline());
        hangout
            .attendees
            .set_status(users[3], AttendeeStatus::Left)
            .unwrap();

        let json = serde_json::to_value(&hangout).unwrap();

        assert_eq!(
            json,
            json!({
                "confirmed_attendees": users,
                "timeline": { "start": at(18), "end": at(21) },
                "attendee_statuses": {
                    users[0].to_string(): "pending",
                    users[1].to_string(): "pending",
                    users[2].to_string(): "pending",
                    users[3].to_string(): "left",
                    users[4].to_string(): "pending",
                },
            })
        );
        assert_eq!(
            serde_json::from_value::<HangoutData>(json).unwrap(),
            hangout
        );
    }

    /// A ping in a state that carries a hangout still reads back from its own
    /// JSON, where the hangout sits inside the flattened `state` of the ping.
    #[test]
    fn a_ping_with_a_hangout_reads_back() {
        let user = UserId::random();
        let mut ping = Ping::new(CreatePingRequest {
            initiator: user,
            group: GroupId::random(),
            activity_type: "drinks".parse().unwrap(),
            rough_timing: "tonight".parse().unwrap(),
            vibe: None,
        });
        ping.lifecycle = PingLifecycle::VenueConfirmed {
            responses: vec![],
            hangout: HangoutData::new(vec![user], timeline()),
        };

        let json = serde_json::to_value(&ping).unwrap();
        assert_eq!(json["state"], "venue_confirmed");
        assert_eq!(json["hangout"]["confirmed_attendees"], json!([user]));

        let back: Ping = serde_json::from_value(json).unwrap();
        assert_eq!(
            back.lifecycle.hangout().unwrap().attendees,
            ping.lifecycle.hangout().unwrap().attendees
        );
    }

    #[test]
    fn has_match_is_worked_out_from_the_overlap() {
        let ping_id = PingId::random();
        let found = MatchResults {
            ping_id,
            overlap: Some(overlap()),
        };
        let none = MatchResults {
            ping_id,
            overlap: None,
        };

        assert!(found.has_match());
        assert!(!none.has_match());
        assert_eq!(
            serde_json::to_value(&found).unwrap(),
            json!({
                "ping_id": ping_id,
                "overlap": { "start": at(18), "end": at(21), "attendee_count": 2 },
                "has_match": true,
            })
        );
        assert_eq!(
            serde_json::to_value(&none).unwrap(),
            json!({ "ping_id": ping_id, "overlap": null, "has_match": false })
        );
    }

    #[test]
    fn a_has_match_that_disagrees_with_the_overlap_is_refused() {
        let ping_id = PingId::random();
        let window = json!({ "start": at(18), "end": at(21), "attendee_count": 2 });

        let read = |overlap: &serde_json::Value, has_match: bool| {
            serde_json::from_value::<MatchResults>(
                json!({ "ping_id": ping_id, "overlap": overlap, "has_match": has_match }),
            )
        };

        assert!(read(&window, true).is_ok());
        assert!(read(&json!(null), false).is_ok());
        assert!(
            read(&json!(null), true)
                .unwrap_err()
                .to_string()
                .contains("has_match is true but overlap is null")
        );
        assert!(
            read(&window, false)
                .unwrap_err()
                .to_string()
                .contains("has_match is false but overlap is present")
        );
    }
}
