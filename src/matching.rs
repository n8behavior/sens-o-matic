use chrono::{DateTime, Utc};

use crate::models::{MatchResults, Ping, Response, TimeOverlap};

#[cfg(test)]
use crate::models::PingLifecycle;

pub struct MatchingEngine;

impl MatchingEngine {
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
