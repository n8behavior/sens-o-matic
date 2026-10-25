//! The overlap algorithm: does a ping have a time that its yes-sayers share?
//!
//! [`MatchingEngine::calculate_match`] reads the answers of a [`Ping`] and
//! returns [`MatchResults`]. It is a pure function of the ping: it changes
//! nothing and does not look at the clock. The caller decides what to do with
//! the result; [`crate::state_machine`] moves the ping to `Matching` or to
//! `NoMatch`.
//!
//! # The algorithm
//!
//! 1. Keep the answers that say yes *and* give an availability. A yes without
//!    an availability cannot be placed in time, so it takes no part, and it is
//!    not counted. (It still makes that member an attendee of the hangout; see
//!    [`StateMachine::create_hangout_data`](crate::state_machine::StateMachine::create_hangout_data).)
//! 2. If none is left, there is no match.
//! 3. Intersect the windows of all that are left: the common window starts at
//!    the *latest* start and ends at the *earliest* end.
//! 4. The window is a match only if it has a length, that is, its start is
//!    strictly before its end. Two windows that only touch (one ends at 18:00,
//!    the other starts at 18:00) have no time in common.
//!
//! Two consequences follow.
//!
//! - One yes with an availability is enough. The initiator is taken as
//!   interested without answering, so one member makes two people.
//! - The result is the intersection of *all* the windows, not the window that
//!   suits the most members. When one member's window does not meet the
//!   others, there is no match at all, even though the others agree among
//!   themselves.
//!
//! The members that the match holds are counted in
//! [`TimeOverlap::attendee_count`], which is the number of windows that were
//! intersected.
//!
//! # Where it runs
//!
//! The handler that triggers the matching runs it once and stores the result
//! in the ping. The handler that reads the match results returns the stored
//! result while the ping is in `Matching`; in any other state it runs the
//! algorithm again on the current answers.

use chrono::{DateTime, Utc};

use crate::models::{MatchResults, Ping, Response, TimeOverlap};

#[cfg(test)]
use crate::models::PingLifecycle;

/// The matching functions. It has no data: the type is only a name for the
/// functions.
pub struct MatchingEngine;

impl MatchingEngine {
    /// Finds the time window that the yes-answers of the ping share; see the
    /// [module docs](self) for the rules.
    ///
    /// # Examples
    ///
    /// Two members who are free from 17:00 to 21:00 and from 18:00 to 22:00
    /// share the time from 18:00 to 21:00:
    ///
    /// ```
    /// use chrono::{DateTime, Utc};
    /// use sens_o_matic::matching::MatchingEngine;
    /// use sens_o_matic::models::{
    ///     Availability, CreatePingRequest, CreateResponseRequest, Ping, Response,
    /// };
    /// use uuid::Uuid;
    ///
    /// let at = |time: &str| time.parse::<DateTime<Utc>>().unwrap();
    /// let mut ping = Ping::new(CreatePingRequest {
    ///     initiator: Uuid::new_v4(),
    ///     group: Uuid::new_v4(),
    ///     activity_type: "drinks".to_string(),
    ///     rough_timing: "tonight".to_string(),
    ///     vibe: None,
    /// });
    /// let mut say_yes = |earliest: &str, latest: &str| {
    ///     ping.add_response(Response::new(CreateResponseRequest {
    ///         user: Uuid::new_v4(),
    ///         answer: true,
    ///         availability: Some(Availability {
    ///             earliest: at(earliest),
    ///             latest: at(latest),
    ///         }),
    ///         preferences: None,
    ///     }));
    /// };
    /// say_yes("2024-12-15T17:00:00Z", "2024-12-15T21:00:00Z");
    /// say_yes("2024-12-15T18:00:00Z", "2024-12-15T22:00:00Z");
    ///
    /// let results = MatchingEngine::calculate_match(&ping);
    /// assert!(results.has_match);
    ///
    /// let overlap = results.overlap.unwrap();
    /// assert_eq!(overlap.start, at("2024-12-15T18:00:00Z"));
    /// assert_eq!(overlap.end, at("2024-12-15T21:00:00Z"));
    /// assert_eq!(overlap.attendee_count, 2);
    /// ```
    ///
    /// Windows that only touch have no time in common, and neither has a
    /// ping without answers:
    ///
    /// ```
    /// # use sens_o_matic::matching::MatchingEngine;
    /// # use sens_o_matic::models::{
    /// #     Availability, CreatePingRequest, CreateResponseRequest, Ping, Response,
    /// # };
    /// # use uuid::Uuid;
    /// # let mut ping = Ping::new(CreatePingRequest {
    /// #     initiator: Uuid::new_v4(),
    /// #     group: Uuid::new_v4(),
    /// #     activity_type: "drinks".to_string(),
    /// #     rough_timing: "tonight".to_string(),
    /// #     vibe: None,
    /// # });
    /// assert!(!MatchingEngine::calculate_match(&ping).has_match);
    ///
    /// for (earliest, latest) in [
    ///     ("2024-12-15T16:00:00Z", "2024-12-15T18:00:00Z"),
    ///     ("2024-12-15T18:00:00Z", "2024-12-15T20:00:00Z"),
    /// ] {
    ///     ping.add_response(Response::new(CreateResponseRequest {
    ///         user: Uuid::new_v4(),
    ///         answer: true,
    ///         availability: Some(Availability {
    ///             earliest: earliest.parse().unwrap(),
    ///             latest: latest.parse().unwrap(),
    ///         }),
    ///         preferences: None,
    ///     }));
    /// }
    ///
    /// let results = MatchingEngine::calculate_match(&ping);
    /// assert!(!results.has_match);
    /// assert!(results.overlap.is_none());
    /// ```
    pub fn calculate_match(ping: &Ping) -> MatchResults {
        let positive_responses: Vec<&Response> = ping.positive_responses();

        // Need at least 1 person with a positive response and availability
        // (initiator is implicitly interested, so 1 responder = 2 people total)
        let responses_with_availability: Vec<&Response> = positive_responses
            .iter()
            .filter(|r| r.availability.is_some())
            .copied()
            .collect();

        if responses_with_availability.is_empty() {
            return MatchResults {
                ping_id: ping.id,
                overlap: None,
                has_match: false,
            };
        }

        // Calculate overlap window (or single window if only 1 response)
        let overlap = Self::find_overlap(&responses_with_availability);
        let has_match = overlap.is_some();

        MatchResults {
            ping_id: ping.id,
            overlap,
            has_match,
        }
    }

    fn find_overlap(responses: &[&Response]) -> Option<TimeOverlap> {
        if responses.is_empty() {
            return None;
        }

        // Collect all availability windows
        let windows: Vec<(DateTime<Utc>, DateTime<Utc>)> = responses
            .iter()
            .filter_map(|r| r.availability.as_ref().map(|a| (a.earliest, a.latest)))
            .collect();

        if windows.is_empty() {
            return None;
        }

        // For 1 window, it's the overlap (the person's availability)
        // For multiple windows, find intersection
        let overlap_start = windows.iter().map(|(start, _)| *start).max()?;
        let overlap_end = windows.iter().map(|(_, end)| *end).min()?;

        // Check if there's actually an overlap
        if overlap_start >= overlap_end {
            return None;
        }

        Some(TimeOverlap {
            start: overlap_start,
            end: overlap_end,
            attendee_count: responses.len() as i32,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Availability;
    use uuid::Uuid;

    fn create_test_response(user_id: Uuid, earliest: &str, latest: &str) -> Response {
        Response {
            id: Uuid::new_v4(),
            user: user_id,
            answer: true,
            availability: Some(Availability {
                earliest: earliest.parse().unwrap(),
                latest: latest.parse().unwrap(),
            }),
            preferences: None,
            updated_at: Utc::now(),
        }
    }

    fn create_test_ping() -> Ping {
        Ping {
            id: Uuid::new_v4(),
            initiator: Uuid::new_v4(),
            group: Uuid::new_v4(),
            activity_type: "drinks".to_string(),
            rough_timing: "tonight".to_string(),
            vibe: None,
            lifecycle: PingLifecycle::Gathering { responses: vec![] },
            created_at: Utc::now(),
        }
    }

    #[test]
    fn test_overlapping_times() {
        let mut ping = create_test_ping();

        // User 1: 17:00 - 21:00
        let response1 = create_test_response(
            Uuid::new_v4(),
            "2024-12-15T17:00:00Z",
            "2024-12-15T21:00:00Z",
        );

        // User 2: 18:00 - 22:00
        let response2 = create_test_response(
            Uuid::new_v4(),
            "2024-12-15T18:00:00Z",
            "2024-12-15T22:00:00Z",
        );

        if let PingLifecycle::Gathering { responses } = &mut ping.lifecycle {
            responses.push(response1);
            responses.push(response2);
        }

        let result = MatchingEngine::calculate_match(&ping);
        assert!(result.has_match);
        assert!(result.overlap.is_some());

        let overlap = result.overlap.unwrap();
        // Overlap should be 18:00 - 21:00
        assert_eq!(
            overlap.start,
            "2024-12-15T18:00:00Z".parse::<DateTime<Utc>>().unwrap()
        );
        assert_eq!(
            overlap.end,
            "2024-12-15T21:00:00Z".parse::<DateTime<Utc>>().unwrap()
        );
        assert_eq!(overlap.attendee_count, 2);
    }

    #[test]
    fn test_no_overlap() {
        let mut ping = create_test_ping();

        // User 1: 16:00 - 18:00
        let response1 = create_test_response(
            Uuid::new_v4(),
            "2024-12-15T16:00:00Z",
            "2024-12-15T18:00:00Z",
        );

        // User 2: 19:00 - 23:00 (no overlap)
        let response2 = create_test_response(
            Uuid::new_v4(),
            "2024-12-15T19:00:00Z",
            "2024-12-15T23:00:00Z",
        );

        if let PingLifecycle::Gathering { responses } = &mut ping.lifecycle {
            responses.push(response1);
            responses.push(response2);
        }

        let result = MatchingEngine::calculate_match(&ping);
        assert!(!result.has_match);
        assert!(result.overlap.is_none());
    }
}
