mod what;
mod when;
mod r#where;
mod who;

use what::{Activity, Mood};

use crate::when::Availability;
use std::rc::Rc;

trait IntentState {
    fn itinerary(&self) -> String;
}

struct Wanting;

impl IntentState for Wanting {
    fn itinerary(&self) -> String {
        // Provide a meaningful implementation
        "Currently wanting to start an intent.".to_string()
    }
}

struct Pinging {
    activity: Activity,
    mood: Mood,
}

impl IntentState for Pinging {
    fn itinerary(&self) -> String {
        // Include details from the state
        format!(
            "Pinging friends about a {:?} with a {:?} mood.",
            self.activity, self.mood
        )
    }
}

struct Scheduling {
    availability: Availability,
}

impl IntentState for Scheduling {
    fn itinerary(&self) -> String {
        // Provide details about the availability
        "Scheduling based on provided availability.".to_string()
    }
}

struct Voting {
    passed: bool,
}

impl IntentState for Voting {
    fn itinerary(&self) -> String {
        format!(
            "Voting has {}.",
            if self.passed { "passed" } else { "failed" }
        )
    }
}

type Name = &'static str;

struct Intent<S: IntentState> {
    name: Name,
    plan: Vec<Rc<dyn IntentState>>,
    state: Rc<S>,
}

impl Intent<Wanting> {
    fn new(name: Name) -> Self {
        Intent {
            name,
            plan: vec![],
            state: Rc::new(Wanting),
        }
    }

    fn start_pinging(self, activity: Activity, mood: Mood) -> Intent<Pinging> {
        let pinging_state = Rc::new(Pinging { activity, mood });
        Intent {
            name: self.name,
            plan: vec![pinging_state.clone()],
            state: pinging_state,
        }
    }
}

impl Intent<Pinging> {
    fn schedule(self, availability: Availability) -> Intent<Scheduling> {
        let scheduling_state = Rc::new(Scheduling { availability });
        Intent {
            name: self.name,
            plan: self.plan,
            state: scheduling_state,
        }
    }
}

// TODO: Implement similar methods for other state transitions

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, NaiveTime, Utc};
    use when::{Availability, AvailabilityWindow, TimeRange};

    #[test]
    fn test_intent_state_transitions() {
        // Create an Intent in the Wanting state
        let intent_wanting = Intent::new("Zara's Hangout");

        // Transition to Pinging state
        let intent_pinging = intent_wanting.start_pinging(Activity::Drinks, Mood::Relaxed);
        assert_eq!(intent_pinging.state.activity, Activity::Drinks);
        assert_eq!(intent_pinging.state.mood, Mood::Relaxed);

        // Transition to Scheduling state
        let availability = Availability::Anytime;
        let intent_scheduling = intent_pinging.schedule(availability);
        // TODO: Add assertions to verify the Scheduling state
    }

    #[test]
    fn test_time_range_creation() {
        let start_time = NaiveTime::from_hms_opt(9, 0, 0).unwrap();
        let end_time = NaiveTime::from_hms_opt(17, 0, 0).unwrap();
        let time_range = TimeRange {
            start_time,
            end_time,
        };

        assert!(time_range.start_time < time_range.end_time);
    }

    #[test]
    fn test_specific_availability() {
        let start = Utc::now() + Duration::days(1);
        let end = start + Duration::hours(3);
        let window = AvailabilityWindow { start, end };
        let availability = Availability::Specific(vec![window.clone()]);

        let windows: Vec<AvailabilityWindow> = availability.into();
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].start, window.start);
        assert_eq!(windows[0].end, window.end);
    }
}
