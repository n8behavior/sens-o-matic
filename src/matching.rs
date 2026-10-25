//! The overlap algorithm: do the answers of a ping have a time that its yes-sayers share?
//!
//! [`MatchingEngine::calculate_match`] reads the answers of a ping and returns
//! [`MatchResults`]. It is a pure function of the id and the answers: it
//! changes nothing and does not look at the clock. The caller decides what to
//! do with the result; [`Ping::match_responses`](crate::models::Ping::match_responses)
//! moves the ping to `Matching` or to `NoMatch`, and takes the function as an
//! argument, so that [`crate::models`] does not depend on this module.
//!
//! # The algorithm
//!
//! 1. Keep the answers that say yes *and* give an availability. A yes without
//!    an availability cannot be placed in time, so it takes no part, and it is
//!    not counted. (It still makes that member an attendee of the hangout; see
//!    [`Ping::confirm`](crate::models::Ping::confirm).)
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

use crate::models::{Availability, MatchResults, PingId, Response, TimeOverlap, TimeWindow};

/// The matching functions. It has no data: the type is only a name for the
/// functions.
pub struct MatchingEngine;

impl MatchingEngine {
    /// Finds the time window that the yes-answers share; see the
    /// [module docs](self) for the rules. `ping_id` is the ping that the
    /// answers belong to; the results say so.
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
    ///     Availability, CreatePingRequest, CreateResponseRequest, GroupId, Ping, Response, UserId,
    /// };
    ///
    /// let at = |time: &str| time.parse::<DateTime<Utc>>().unwrap();
    /// let say_yes = |earliest: &str, latest: &str| {
    ///     Response::new(CreateResponseRequest {
    ///         user: UserId::random(),
    ///         answer: true,
    ///         availability: Some(Availability::new(at(earliest), at(latest)).unwrap()),
    ///         preferences: None,
    ///     })
    /// };
    /// let mut ping = Ping::new(CreatePingRequest {
    ///     initiator: UserId::random(),
    ///     group: GroupId::random(),
    ///     activity_type: "drinks".parse().unwrap(),
    ///     rough_timing: "tonight".parse().unwrap(),
    ///     vibe: None,
    /// })
    /// .respond(say_yes("2024-12-15T17:00:00Z", "2024-12-15T21:00:00Z"));
    /// ping.add_response(say_yes("2024-12-15T18:00:00Z", "2024-12-15T22:00:00Z")).unwrap();
    ///
    /// let results = MatchingEngine::calculate_match(ping.id, ping.responses());
    /// assert!(results.has_match());
    ///
    /// let overlap = results.overlap.unwrap();
    /// assert_eq!(overlap.start(), at("2024-12-15T18:00:00Z"));
    /// assert_eq!(overlap.end(), at("2024-12-15T21:00:00Z"));
    /// assert_eq!(overlap.attendee_count(), 2);
    /// ```
    ///
    /// Windows that only touch have no time in common, and neither have no
    /// answers at all:
    ///
    /// ```
    /// # use sens_o_matic::matching::MatchingEngine;
    /// # use sens_o_matic::models::{
    /// #     Availability, CreateResponseRequest, PingId, Response, UserId,
    /// # };
    /// let ping_id = PingId::random();
    /// assert!(!MatchingEngine::calculate_match(ping_id, &[]).has_match());
    ///
    /// let answers: Vec<Response> = [
    ///     ("2024-12-15T16:00:00Z", "2024-12-15T18:00:00Z"),
    ///     ("2024-12-15T18:00:00Z", "2024-12-15T20:00:00Z"),
    /// ]
    /// .into_iter()
    /// .map(|(earliest, latest)| {
    ///     Response::new(CreateResponseRequest {
    ///         user: UserId::random(),
    ///         answer: true,
    ///         availability: Some(
    ///             Availability::new(earliest.parse().unwrap(), latest.parse().unwrap()).unwrap(),
    ///         ),
    ///         preferences: None,
    ///     })
    /// })
    /// .collect();
    ///
    /// let results = MatchingEngine::calculate_match(ping_id, &answers);
    /// assert!(!results.has_match());
    /// assert!(results.overlap.is_none());
    /// ```
    pub fn calculate_match(ping_id: PingId, responses: &[Response]) -> MatchResults {
        // Need at least 1 person with a positive response and availability
        // (initiator is implicitly interested, so 1 responder = 2 people total)
        let responses_with_availability: Vec<&Response> = responses
            .iter()
            .filter(|r| r.answer && r.availability.is_some())
            .collect();

        if responses_with_availability.is_empty() {
            return MatchResults {
                ping_id,
                overlap: None,
            };
        }

        // Calculate overlap window (or single window if only 1 response)
        let overlap = Self::find_overlap(&responses_with_availability);

        MatchResults { ping_id, overlap }
    }

    fn find_overlap(responses: &[&Response]) -> Option<TimeOverlap> {
        // Collect all availability windows
        let windows: Vec<&Availability> = responses
            .iter()
            .filter_map(|r| r.availability.as_ref())
            .collect();

        // For 1 window, it's the overlap (the person's availability)
        // For multiple windows, find intersection
        let overlap_start = windows.iter().map(|a| a.earliest()).max()?;
        let overlap_end = windows.iter().map(|a| a.latest()).min()?;

        // A window has a length: if the intersection ends before it starts, or
        // only touches, there is no overlap.
        let window = TimeWindow::new(overlap_start, overlap_end).ok()?;

        Some(TimeOverlap::new(window, windows.len() as u32))
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};

    use super::*;
    use crate::models::{Availability, ResponseId, UserId};

    fn answer(earliest: &str, latest: &str) -> Response {
        Response {
            id: ResponseId::random(),
            user: UserId::random(),
            answer: true,
            availability: Some(
                Availability::new(earliest.parse().unwrap(), latest.parse().unwrap()).unwrap(),
            ),
            preferences: None,
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn test_overlapping_times() {
        let answers = [
            answer("2024-12-15T17:00:00Z", "2024-12-15T21:00:00Z"),
            answer("2024-12-15T18:00:00Z", "2024-12-15T22:00:00Z"),
        ];

        let result = MatchingEngine::calculate_match(PingId::random(), &answers);
        assert!(result.has_match());
        assert!(result.overlap.is_some());

        let overlap = result.overlap.unwrap();
        // Overlap should be 18:00 - 21:00
        assert_eq!(
            overlap.start(),
            "2024-12-15T18:00:00Z".parse::<DateTime<Utc>>().unwrap()
        );
        assert_eq!(
            overlap.end(),
            "2024-12-15T21:00:00Z".parse::<DateTime<Utc>>().unwrap()
        );
        assert_eq!(overlap.attendee_count(), 2);
    }

    #[test]
    fn test_no_overlap() {
        let answers = [
            answer("2024-12-15T16:00:00Z", "2024-12-15T18:00:00Z"),
            // No overlap with the first
            answer("2024-12-15T19:00:00Z", "2024-12-15T23:00:00Z"),
        ];

        let result = MatchingEngine::calculate_match(PingId::random(), &answers);
        assert!(!result.has_match());
        assert!(result.overlap.is_none());
    }

    #[test]
    fn the_results_name_the_ping() {
        let ping_id = PingId::random();
        let result = MatchingEngine::calculate_match(ping_id, &[]);
        assert_eq!(result.ping_id, ping_id);
    }

    #[test]
    fn a_no_takes_no_part() {
        let mut no = answer("2024-12-15T17:00:00Z", "2024-12-15T21:00:00Z");
        no.answer = false;

        let result = MatchingEngine::calculate_match(PingId::random(), &[no]);
        assert!(!result.has_match());
    }
}
