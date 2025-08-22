mod what;
mod when;
mod r#where;
mod who;

use what::What;
use who::{Gathering, Participant};

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
    recipients: Vec<Participant>,
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

impl IntentState for Gathering {
    fn itinerary(&self) -> String {
        let interested_count = self.interested.len();
        let pending_count = self.pending().len();
        let unavailable_count = self.unavailable.len();
        
        format!(
            "Gathering responses: {} interested, {} pending, {} unavailable",
            interested_count, pending_count, unavailable_count
        )
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

    fn start_pinging(self, what: What, recipients: Vec<Participant>) -> Intent<Pinging> {
        let pinging_state = Rc::new(Pinging { what, recipients });
        Intent {
            name: self.name,
            plan: vec![pinging_state.clone()],
            state: pinging_state,
        }
    }
}

impl Intent<Pinging> {
    fn start_gathering(self) -> Intent<Gathering> {
        let gathering_state = Rc::new(Gathering::new(self.state.recipients.clone()));
        let mut plan = self.plan;
        plan.push(gathering_state.clone() as Rc<dyn IntentState>);
        Intent {
            name: self.name,
            plan,
            state: gathering_state,
        }
    }

    fn schedule(self, availabilities: Vec<Availability>) -> Intent<Scheduling> {
        let scheduling_state = Rc::new(Scheduling { availabilities });
        Intent {
            name: self.name,
            plan: self.plan,
            state: scheduling_state,
        }
    }
}

impl Intent<Gathering> {
    fn add_interested(&mut self, participant: Participant) {
        let state = Rc::make_mut(&mut self.state);
        if state.recipients.contains(&participant) {
            // Remove from unavailable if present
            state.unavailable.retain(|p| p != &participant);
            // Add to interested if not already there
            if !state.interested.contains(&participant) {
                state.interested.push(participant);
            }
        }
    }

    fn mark_unavailable(&mut self, participant: Participant) {
        let state = Rc::make_mut(&mut self.state);
        if state.recipients.contains(&participant) {
            // Remove from interested if present
            state.interested.retain(|p| p != &participant);
            // Add to unavailable if not already there
            if !state.unavailable.contains(&participant) {
                state.unavailable.push(participant);
            }
        }
    }

    fn apply_timeout(&mut self) {
        let state = Rc::make_mut(&mut self.state);
        state.apply_timeout();
    }

    fn interested(&self) -> Vec<Participant> {
        self.state.interested.clone()
    }

    fn unavailable(&self) -> Vec<Participant> {
        self.state.unavailable.clone()
    }

    fn pending(&self) -> Vec<Participant> {
        self.state.pending()
    }

    fn all_responded(&self) -> bool {
        self.state.all_responded()
    }

    fn has_interested_participants(&self) -> bool {
        self.state.has_interested_participants()
    }

    fn should_fail_intent(&self) -> bool {
        self.state.should_fail_intent()
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
        let recipients = vec![];
        let intent_pinging = intent_wanting.start_pinging(what.clone(), recipients);
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
        let intent_pinging = intent_wanting.start_pinging(what, vec![]);

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
        let intent_pinging = intent_wanting.start_pinging(what, vec![]);

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
        let intent_pinging = intent_wanting.start_pinging(what, vec![]);

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
        let intent_pinging = intent_wanting.start_pinging(what, vec![]);

        assert_eq!(intent_pinging.state.what.activity, Some(Activity::Lunch));
        assert_eq!(intent_pinging.state.what.mood, None);
        assert_eq!(intent_pinging.state.what.group_size, None);
    }

    // Gathering State Tests (TDD - will fail until implementation)

    #[test]
    fn test_gathering_recipient_responses() {
        let alice = Participant::new("Alice", "alice@example.com");
        let bob = Participant::new("Bob", "bob@example.com");
        let carol = Participant::new("Carol", "carol@example.com");
        let dave = Participant::new("Dave", "dave@example.com");
        let recipients = vec![alice.clone(), bob.clone(), carol.clone(), dave.clone()];

        // Must go through typestate pattern
        let intent_wanting = Intent::new("Test Responses");
        let intent_pinging = intent_wanting.start_pinging(What::new(), recipients);
        let mut intent_gathering = intent_pinging.start_gathering();

        // Alice and Bob are interested
        intent_gathering.add_interested(alice.clone());
        intent_gathering.add_interested(bob.clone());

        // Carol is unavailable
        intent_gathering.mark_unavailable(carol.clone());

        // Dave doesn't respond (still in pending)

        assert_eq!(intent_gathering.interested(), vec![alice, bob]);
        assert_eq!(intent_gathering.unavailable(), vec![carol]);
        assert!(intent_gathering.pending().contains(&dave));
    }

    #[test]
    fn test_gathering_timeout_behavior() {
        let alice = Participant::new("Alice", "alice@example.com");
        let bob = Participant::new("Bob", "bob@example.com");
        let carol = Participant::new("Carol", "carol@example.com");
        let recipients = vec![alice.clone(), bob.clone(), carol.clone()];

        // Must go through typestate pattern
        let intent_wanting = Intent::new("Test Timeout");
        let intent_pinging = intent_wanting.start_pinging(What::new(), recipients);
        let mut intent_gathering = intent_pinging.start_gathering();

        intent_gathering.add_interested(alice.clone());
        // Bob and Carol don't respond

        // Apply timeout - pending participants become unavailable
        intent_gathering.apply_timeout();

        assert_eq!(intent_gathering.interested(), vec![alice]);
        assert_eq!(intent_gathering.unavailable().len(), 2);
        assert!(intent_gathering.unavailable().contains(&bob));
        assert!(intent_gathering.unavailable().contains(&carol));
        assert_eq!(intent_gathering.pending().len(), 0); // No one left pending
    }

    #[test]
    fn test_gathering_no_one_interested() {
        let alice = Participant::new("Alice", "alice@example.com");
        let bob = Participant::new("Bob", "bob@example.com");
        let recipients = vec![alice.clone(), bob.clone()];

        // Must go through typestate pattern
        let intent_wanting = Intent::new("Test No Interest");
        let intent_pinging = intent_wanting.start_pinging(What::new(), recipients);
        let mut intent_gathering = intent_pinging.start_gathering();

        intent_gathering.mark_unavailable(alice);
        intent_gathering.mark_unavailable(bob);

        // Should indicate intent failure when no one is interested
        assert!(!intent_gathering.has_interested_participants());
        assert!(intent_gathering.should_fail_intent());
    }

    #[test]
    fn test_gathering_incremental_responses() {
        // Test that responses can trickle in over time
        let alice = Participant::new("Alice", "alice@example.com");
        let bob = Participant::new("Bob", "bob@example.com");
        let carol = Participant::new("Carol", "carol@example.com");
        let recipients = vec![alice.clone(), bob.clone(), carol.clone()];

        // Must go through typestate pattern
        let intent_wanting = Intent::new("Test Incremental");
        let intent_pinging = intent_wanting.start_pinging(What::new(), recipients);
        let mut intent_gathering = intent_pinging.start_gathering();

        // Initially all pending
        assert_eq!(intent_gathering.pending().len(), 3);

        // Alice responds first
        intent_gathering.add_interested(alice.clone());
        assert_eq!(intent_gathering.pending().len(), 2);
        assert_eq!(intent_gathering.interested().len(), 1);

        // Bob responds later
        intent_gathering.mark_unavailable(bob.clone());
        assert_eq!(intent_gathering.pending().len(), 1);
        assert_eq!(intent_gathering.unavailable().len(), 1);

        // Carol still pending
        assert!(intent_gathering.pending().contains(&carol));
    }

    #[test]
    fn test_gathering_all_respond_early() {
        // Test that we can proceed before timeout if everyone responds
        let alice = Participant::new("Alice", "alice@example.com");
        let bob = Participant::new("Bob", "bob@example.com");
        let recipients = vec![alice.clone(), bob.clone()];

        // Must go through typestate pattern
        let intent_wanting = Intent::new("Test Early Response");
        let intent_pinging = intent_wanting.start_pinging(What::new(), recipients);
        let mut intent_gathering = intent_pinging.start_gathering();

        intent_gathering.add_interested(alice);
        intent_gathering.mark_unavailable(bob);

        // All have responded, none pending
        assert_eq!(intent_gathering.pending().len(), 0);
        assert!(intent_gathering.all_responded());
        // Can proceed without waiting for timeout
    }
}
