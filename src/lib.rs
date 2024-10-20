use chrono::{Datelike, Duration, NaiveTime, Utc, Weekday};
use std::rc::Rc;

#[derive(Debug, Clone)]
pub struct AvailabilityWindow {
    pub start: chrono::DateTime<Utc>,
    pub end: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct RecurringAvailability {
    pub days_of_week: Vec<Weekday>,
    pub time_ranges: Vec<TimeRange>,
}

#[derive(Debug, Clone)]
pub struct TimeRange {
    pub start_time: NaiveTime,
    pub end_time: NaiveTime,
}

#[derive(Debug, Clone)]
pub enum Availability {
    Specific(Vec<AvailabilityWindow>),
    Recurring(RecurringAvailability),
    Anytime, // Available at any time
}

impl From<Availability> for Vec<AvailabilityWindow> {
    fn from(availability: Availability) -> Self {
        match availability {
            Availability::Specific(windows) => windows,
            Availability::Recurring(recurring) => recurring.into(),
            Availability::Anytime => vec![AvailabilityWindow {
                start: earliest_possible_time(),
                end: latest_possible_time(),
            }],
        }
    }
}

impl From<RecurringAvailability> for Vec<AvailabilityWindow> {
    fn from(recurring: RecurringAvailability) -> Self {
        let mut windows = Vec::new();
        let today = Utc::now().date_naive();
        let two_weeks_from_now = today + Duration::days(14);

        let mut date = today;
        while date <= two_weeks_from_now {
            if recurring.days_of_week.contains(&date.weekday()) {
                for time_range in &recurring.time_ranges {
                    let start_naive = chrono::NaiveDateTime::new(date, time_range.start_time);
                    let end_naive = chrono::NaiveDateTime::new(date, time_range.end_time);
                    let start = chrono::DateTime::from_naive_utc_and_offset(start_naive, Utc);
                    let end = chrono::DateTime::from_naive_utc_and_offset(end_naive, Utc);
                    windows.push(AvailabilityWindow { start, end });
                }
            }
            date += Duration::days(1);
        }
        windows
    }
}

fn earliest_possible_time() -> chrono::DateTime<Utc> {
    Utc::now()
}

fn latest_possible_time() -> chrono::DateTime<Utc> {
    Utc::now() + Duration::days(365)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Activity {
    Coffee,
    Dinner,
    Drinks,
    GameNight,
    Virtual,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mood {
    Relaxed,
    Energetic,
    CasualCatchUp,
    Celebratory,
    Brainstorming,
}

pub enum Days {
    Any,
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

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
    use chrono::NaiveTime;

    #[test]
    fn test_availability_window_creation() {
        let start = Utc::now();
        let end = start + Duration::hours(2);
        let window = AvailabilityWindow { start, end };

        assert!(window.start < window.end);
    }

    #[test]
    fn test_recurring_availability_expansion() {
        let time_range = TimeRange {
            start_time: NaiveTime::from_hms_opt(18, 0, 0).unwrap(),
            end_time: NaiveTime::from_hms_opt(20, 0, 0).unwrap(),
        };
        let recurring = RecurringAvailability {
            days_of_week: vec![Weekday::Fri, Weekday::Sat],
            time_ranges: vec![time_range],
        };

        let windows: Vec<AvailabilityWindow> = recurring.into();
        assert!(!windows.is_empty());

        // Check that all windows fall on the specified days
        for window in windows {
            let weekday = window.start.date_naive().weekday();
            assert!(weekday == Weekday::Fri || weekday == Weekday::Sat);
        }
    }

    #[test]
    fn test_availability_anytime() {
        let availability = Availability::Anytime;
        let windows: Vec<AvailabilityWindow> = availability.into();

        assert_eq!(windows.len(), 1);
        let window = &windows[0];
        assert!(window.start < window.end);
    }

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

    #[test]
    fn test_activity_enum() {
        let activity = Activity::Dinner;
        match activity {
            Activity::Dinner => assert!(true),
            _ => assert!(false, "Activity should be Dinner"),
        }
    }

    #[test]
    fn test_mood_enum() {
        let mood = Mood::Celebratory;
        match mood {
            Mood::Celebratory => assert!(true),
            _ => assert!(false, "Mood should be Celebratory"),
        }
    }

    #[test]
    fn test_availability_conversion() {
        // TODO: Test conversion from Availability to Vec<AvailabilityWindow> thoroughly
        // Including edge cases, such as empty time ranges or invalid dates
    }

    // Additional tests can be added here
}
