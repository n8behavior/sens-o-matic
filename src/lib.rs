mod what;
mod when;
mod r#where;
mod who;

use what::What;

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
    what: What,
}

impl IntentState for Pinging {
    fn itinerary(&self) -> String {
        let activity = self
            .what
            .activity
            .as_ref()
            .map(|a| format!("{a:?}"))
            .unwrap_or_else(|| "any activity".to_string());

        let mood = self
            .what
            .mood
            .as_ref()
            .map(|m| format!("{m:?}"))
            .unwrap_or_else(|| "any".to_string());

        let group = self
            .what
            .group_size
            .as_ref()
            .map(|g| format!("{g:?} group"))
            .unwrap_or_else(|| "any size group".to_string());

        format!("Pinging friends about {activity} with a {mood} mood, preferring {group}.")
    }
}

struct Scheduling {
    availabilities: Vec<Availability>,
}

impl IntentState for Scheduling {
    fn itinerary(&self) -> String {
        // Provide details about the availability
        if self.availabilities.is_empty() {
            "Scheduling with flexible availability.".to_string()
        } else {
            format!(
                "Scheduling with {} availability windows.",
                self.availabilities.len()
            )
        }
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

    fn start_pinging(self, what: What) -> Intent<Pinging> {
        let pinging_state = Rc::new(Pinging { what });
        Intent {
            name: self.name,
            plan: vec![pinging_state.clone()],
            state: pinging_state,
        }
    }
}

impl Intent<Pinging> {
    fn schedule(self, availabilities: Vec<Availability>) -> Intent<Scheduling> {
        let scheduling_state = Rc::new(Scheduling { availabilities });
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
    use chrono::{Duration, Utc};
    use what::{Activity, GroupSize, Mood, What};
    use when::Availability;

    #[test]
    fn test_intent_state_transitions() {
        // Create an Intent in the Wanting state
        let intent_wanting = Intent::new("Zara's Hangout");

        // Transition to Pinging state
        let what = What::new()
            .with_activity(Activity::Drinks)
            .with_mood(Mood::Chill)
            .with_group_size(GroupSize::Small);
        let intent_pinging = intent_wanting.start_pinging(what.clone());
        assert_eq!(intent_pinging.state.what, what);

        // Transition to Scheduling state with specific availability
        let start = Utc::now() + Duration::hours(2);
        let availability = Availability::new(start, Duration::hours(3));
        let intent_scheduling = intent_pinging.schedule(vec![availability]);
        assert_eq!(intent_scheduling.state.availabilities.len(), 1);
    }

    #[test]
    fn test_scheduling_with_flexible_availability() {
        let intent_wanting = Intent::new("Bob's Gathering");
        let what = What::new()
            .with_activity(Activity::Coffee)
            .with_mood(Mood::Quick)
            .with_group_size(GroupSize::OneOnOne);
        let intent_pinging = intent_wanting.start_pinging(what);

        // Schedule with empty vec means flexible/anytime
        let intent_scheduling = intent_pinging.schedule(vec![]);
        assert!(intent_scheduling.state.availabilities.is_empty());
        assert_eq!(
            intent_scheduling.state.itinerary(),
            "Scheduling with flexible availability."
        );
    }

    #[test]
    fn test_scheduling_with_multiple_windows() {
        let intent_wanting = Intent::new("Weekend Plans");
        let what = What::new()
            .with_activity(Activity::Active)
            .with_mood(Mood::Energetic)
            .with_group_size(GroupSize::Large);
        let intent_pinging = intent_wanting.start_pinging(what);

        let now = Utc::now();
        let avail1 = Availability::new(now + Duration::hours(2), Duration::hours(2));
        let avail2 = Availability::new(now + Duration::days(1), Duration::hours(3));

        let intent_scheduling = intent_pinging.schedule(vec![avail1, avail2]);
        assert_eq!(intent_scheduling.state.availabilities.len(), 2);
        assert_eq!(
            intent_scheduling.state.itinerary(),
            "Scheduling with 2 availability windows."
        );
    }

    #[test]
    fn test_what_with_no_preferences() {
        let intent_wanting = Intent::new("Spontaneous Hangout");
        let what = What::new(); // All None - totally flexible
        let intent_pinging = intent_wanting.start_pinging(what);

        assert_eq!(intent_pinging.state.what.activity, None);
        assert_eq!(intent_pinging.state.what.mood, None);
        assert_eq!(intent_pinging.state.what.group_size, None);
        assert_eq!(
            intent_pinging.state.itinerary(),
            "Pinging friends about any activity with a any mood, preferring any size group."
        );
    }

    #[test]
    fn test_what_with_partial_preferences() {
        let intent_wanting = Intent::new("Lunch Plans");
        let what = What::new().with_activity(Activity::Lunch); // Only activity specified
        let intent_pinging = intent_wanting.start_pinging(what);

        assert_eq!(intent_pinging.state.what.activity, Some(Activity::Lunch));
        assert_eq!(intent_pinging.state.what.mood, None);
        assert_eq!(intent_pinging.state.what.group_size, None);
    }
}
