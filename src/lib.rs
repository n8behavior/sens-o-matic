mod what;
mod when;
mod r#where;
mod who;

use what::What;
use who::Member;

use crate::when::Availability;
use std::rc::Rc;

trait IntentState {
    fn itinerary(&self) -> String;

    /// Apply timeout logic for this state.
    /// Most states need timeout handling to move the flow forward.
    /// Default implementation does nothing (override in states that need it).
    fn apply_timeout(&mut self) {}
}

struct Wanting {
    what: What,
}

impl IntentState for Wanting {
    fn itinerary(&self) -> String {
        "Currently wanting to start an intent".to_string()
    }
}

#[derive(Clone)]
pub struct Invitation {
    pub invitees: Vec<Member>,
    pub interested: Vec<Member>,
    pub unavailable: Vec<Member>,
}

impl Invitation {
    pub fn new(invitees: Vec<Member>) -> Self {
        Invitation {
            invitees: invitees.clone(),
            interested: Vec::new(),
            unavailable: Vec::new(),
        }
    }

    pub fn pending(&self) -> Vec<Member> {
        self.invitees
            .iter()
            .filter(|p| !self.interested.contains(p) && !self.unavailable.contains(p))
            .cloned()
            .collect()
    }

    pub fn all_responded(&self) -> bool {
        self.pending().is_empty()
    }

    pub fn has_interested_participants(&self) -> bool {
        !self.interested.is_empty()
    }

    pub fn should_fail_intent(&self) -> bool {
        self.interested.is_empty() && self.all_responded()
    }
}
// TODO: Implement similar methods for other state transitions

impl IntentState for Invitation {
    fn itinerary(&self) -> String {
        let interested_count = self.interested.len();
        let pending_count = self.pending().len();
        let unavailable_count = self.unavailable.len();

        format!(
            "Invitation: {interested_count} interested, {pending_count} pending, {unavailable_count} unavailable"
        )
    }

    fn apply_timeout(&mut self) {
        // Move all pending invitees to unavailable
        let pending = self.pending();
        self.unavailable.extend(pending);
    }
}

struct When {
    availabilities: Vec<Availability>,
}

impl IntentState for When {
    fn itinerary(&self) -> String {
        // Provide details about the availability
        if self.availabilities.is_empty() {
            "When: flexible availability".to_string()
        } else {
            format!("When: {} availability windows.", self.availabilities.len())
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

// TODO: thinking this sholud be `Rc<str>`
type Name = &'static str;

// TODO: should likely derive Clone
struct Intent<S: IntentState> {
    name: Name,
    plan: Vec<Rc<dyn IntentState>>,
    state: Rc<S>,
}

impl Intent<Wanting> {
    // TODO: impl Into<Name> would be more ergonomic for passing in litteral and owned strings but I'm not sure why I have this as a
    // borrowed static String. It's going to be for UI display only. Since other
    // fields in Intent<S> are `Rc`, Clone for the whole struct would be cheap and easy without
    // worry about borrowing or lifestimes. Thoughts?
    fn new(name: Name, what: What) -> Self {
        Intent {
            name,
            plan: vec![],
            state: Rc::new(Wanting { what }),
        }
    }

    fn send_invitation(self, invitees: Vec<Member>) -> Intent<Invitation> {
        let invitation_state = Rc::new(Invitation::new(invitees));
        let mut plan = self.plan;
        plan.push(self.state.clone() as Rc<dyn IntentState>);
        Intent {
            name: self.name,
            plan,
            state: invitation_state,
        }
    }
}

impl Intent<Invitation> {
    fn schedule(self, availabilities: Vec<Availability>) -> Intent<When> {
        let when_state = Rc::new(When { availabilities });
        let mut plan = self.plan;
        plan.push(when_state.clone() as Rc<dyn IntentState>);
        Intent {
            name: self.name,
            plan,
            state: when_state,
        }
    }

    // Methods for managing invitation responses
    fn add_interested(&mut self, member: Member) {
        let state = Rc::make_mut(&mut self.state);
        if state.invitees.contains(&member) {
            // Remove from unavailable if present
            state.unavailable.retain(|p| p != &member);
            // Add to interested if not already there
            if !state.interested.contains(&member) {
                state.interested.push(member);
            }
        }
    }

    fn mark_unavailable(&mut self, member: Member) {
        let state = Rc::make_mut(&mut self.state);
        if state.invitees.contains(&member) {
            // Remove from interested if present
            state.interested.retain(|p| p != &member);
            // Add to unavailable if not already there
            if !state.unavailable.contains(&member) {
                state.unavailable.push(member);
            }
        }
    }

    fn apply_timeout(&mut self) {
        let state = Rc::make_mut(&mut self.state);
        state.apply_timeout();
    }

    fn interested(&self) -> Vec<Member> {
        self.state.interested.clone()
    }

    fn unavailable(&self) -> Vec<Member> {
        self.state.unavailable.clone()
    }

    fn pending(&self) -> Vec<Member> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use what::{Activity, GroupSize, Mood, What};
    use when::Availability;

    #[test]
    fn test_intent_state_transitions() {
        // Create an Intent in the Wanting state
        let what = What::new()
            .with_activity(Activity::Drinks)
            .with_mood(Mood::Chill)
            .with_group_size(GroupSize::Small);
        let intent_wanting = Intent::new("Zara's Hangout", what.clone());

        // Transition to Invitation state
        let invitees = vec![];
        let intent_invitation = intent_wanting.send_invitation(invitees);
        assert_eq!(intent_invitation.state.invitees.len(), 0);

        // Transition to When state with specific availability
        let start = Utc::now() + Duration::hours(2);
        let availability = Availability::new(start, Duration::hours(3));
        let intent_when = intent_invitation.schedule(vec![availability]);
        assert_eq!(intent_when.state.availabilities.len(), 1);
    }

    #[test]
    fn test_when_with_flexible_availability() {
        let what = What::new()
            .with_activity(Activity::Coffee)
            .with_mood(Mood::Quick)
            .with_group_size(GroupSize::OneOnOne);
        let intent_wanting = Intent::new("Bob's Meetup", what);
        let intent_invitation = intent_wanting.send_invitation(vec![]);

        // Schedule with empty vec means flexible/anytime
        let intent_when = intent_invitation.schedule(vec![]);
        assert!(intent_when.state.availabilities.is_empty());
        assert_eq!(intent_when.state.itinerary(), "When: flexible availability");
    }

    #[test]
    fn test_when_with_multiple_windows() {
        let what = What::new()
            .with_activity(Activity::BoardGame)
            .with_mood(Mood::Energetic)
            .with_group_size(GroupSize::Large);
        let intent_wanting = Intent::new("Weekend Plans", what);
        let intent_invitation = intent_wanting.send_invitation(vec![]);

        let now = Utc::now();
        let avail1 = Availability::new(now + Duration::hours(2), Duration::hours(2));
        let avail2 = Availability::new(now + Duration::days(1), Duration::hours(3));

        let intent_when = intent_invitation.schedule(vec![avail1, avail2]);
        assert_eq!(intent_when.state.availabilities.len(), 2);
        assert_eq!(
            intent_when.state.itinerary(),
            "When: 2 availability windows."
        );
    }

    #[test]
    fn test_what_with_no_preferences() {
        let what = What::new(); // All None - totally flexible
        let intent_wanting = Intent::new("Spontaneous Hangout", what);
        let intent_invitation = intent_wanting.send_invitation(vec![]);

        // What is now stored in Wanting state, not Invitation
        assert_eq!(
            intent_invitation.state.itinerary(),
            "Invitation: 0 interested, 0 pending, 0 unavailable"
        );
    }

    #[test]
    fn test_what_with_partial_preferences() {
        let what = What::new().with_activity(Activity::Lunch); // Only activity specified
        let intent_wanting = Intent::new("Lunch Plans", what);
        let intent_invitation = intent_wanting.send_invitation(vec![]);

        // What is stored in Wanting state, verified by successful creation
    }

    // Responses State Tests

    #[test]
    fn test_responses_invitee_tracking() {
        let alice = Member::new("Alice", "alice@example.com");
        let bob = Member::new("Bob", "bob@example.com");
        let carol = Member::new("Carol", "carol@example.com");
        let dave = Member::new("Dave", "dave@example.com");
        let invitees = vec![alice.clone(), bob.clone(), carol.clone(), dave.clone()];

        // Must go through typestate pattern
        let intent_wanting = Intent::new("Test Responses", What::new());
        let mut intent_invitation = intent_wanting.send_invitation(invitees);

        // Alice and Bob are interested
        intent_invitation.add_interested(alice.clone());
        intent_invitation.add_interested(bob.clone());

        // Carol is unavailable
        intent_invitation.mark_unavailable(carol.clone());

        // Dave doesn't respond (still in pending)

        assert_eq!(intent_invitation.interested(), vec![alice, bob]);
        assert_eq!(intent_invitation.unavailable(), vec![carol]);
        assert!(intent_invitation.pending().contains(&dave));
    }

    #[test]
    fn test_responses_timeout_behavior() {
        let alice = Member::new("Alice", "alice@example.com");
        let bob = Member::new("Bob", "bob@example.com");
        let carol = Member::new("Carol", "carol@example.com");
        let invitees = vec![alice.clone(), bob.clone(), carol.clone()];

        // Must go through typestate pattern
        let intent_wanting = Intent::new("Test Timeout", What::new());
        let mut intent_invitation = intent_wanting.send_invitation(invitees);

        intent_invitation.add_interested(alice.clone());
        // Bob and Carol don't respond

        // Apply timeout - pending invitees become unavailable
        intent_invitation.apply_timeout();

        assert_eq!(intent_invitation.interested(), vec![alice]);
        assert_eq!(intent_invitation.unavailable().len(), 2);
        assert!(intent_invitation.unavailable().contains(&bob));
        assert!(intent_invitation.unavailable().contains(&carol));
        assert_eq!(intent_invitation.pending().len(), 0); // No one left pending
    }

    #[test]
    fn test_responses_no_one_interested() {
        let alice = Member::new("Alice", "alice@example.com");
        let bob = Member::new("Bob", "bob@example.com");
        let invitees = vec![alice.clone(), bob.clone()];

        // Must go through typestate pattern
        let intent_wanting = Intent::new("Test No Interest", What::new());
        let mut intent_invitation = intent_wanting.send_invitation(invitees);

        intent_invitation.mark_unavailable(alice);
        intent_invitation.mark_unavailable(bob);

        // Should indicate intent failure when no one is interested
        assert!(!intent_invitation.has_interested_participants());
        assert!(intent_invitation.should_fail_intent());
    }

    #[test]
    fn test_responses_incremental_tracking() {
        // Test that responses can trickle in over time
        let alice = Member::new("Alice", "alice@example.com");
        let bob = Member::new("Bob", "bob@example.com");
        let carol = Member::new("Carol", "carol@example.com");
        let invitees = vec![alice.clone(), bob.clone(), carol.clone()];

        // Must go through typestate pattern
        let intent_wanting = Intent::new("Test Incremental", What::new());
        let mut intent_invitation = intent_wanting.send_invitation(invitees);

        // Initially all pending
        assert_eq!(intent_invitation.pending().len(), 3);

        // Alice responds first
        intent_invitation.add_interested(alice.clone());
        assert_eq!(intent_invitation.pending().len(), 2);
        assert_eq!(intent_invitation.interested().len(), 1);

        // Bob responds later
        intent_invitation.mark_unavailable(bob.clone());
        assert_eq!(intent_invitation.pending().len(), 1);
        assert_eq!(intent_invitation.unavailable().len(), 1);

        // Carol still pending
        assert!(intent_invitation.pending().contains(&carol));
    }

    #[test]
    fn test_responses_all_respond_early() {
        // Test that we can proceed before timeout if everyone responds
        let alice = Member::new("Alice", "alice@example.com");
        let bob = Member::new("Bob", "bob@example.com");
        let invitees = vec![alice.clone(), bob.clone()];

        // Must go through typestate pattern
        let intent_wanting = Intent::new("Test Early Response", What::new());
        let mut intent_invitation = intent_wanting.send_invitation(invitees);

        intent_invitation.add_interested(alice);
        intent_invitation.mark_unavailable(bob);

        // All have responded, none pending
        assert_eq!(intent_invitation.pending().len(), 0);
        assert!(intent_invitation.all_responded());
        // Can proceed without waiting for timeout
    }
}
