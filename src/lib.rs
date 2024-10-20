use chrono::{DateTime, Datelike, Duration, NaiveDateTime, NaiveTime, Utc, Weekday};

#[derive(Debug, Clone)]
pub struct AvailabilityWindow {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
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
            // Future variants can be added here
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
                    let start_naive = NaiveDateTime::new(date, time_range.start_time);
                    let end_naive = NaiveDateTime::new(date, time_range.end_time);
                    let start = DateTime::from_naive_utc_and_offset(start_naive, Utc);
                    let end = DateTime::from_naive_utc_and_offset(end_naive, Utc);
                    windows.push(AvailabilityWindow { start, end });
                }
            }
            date += Duration::days(1);
        }
        windows
    }
}

fn earliest_possible_time() -> DateTime<Utc> {
    Utc::now()
}

fn latest_possible_time() -> DateTime<Utc> {
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

trait PingState {
    fn itenarary(&self) -> String;
}

struct Wanting;
struct Pinging {
    activity: Activity,
    mood: Mood,
}
struct Scheduling {
    availability: Availability,
}
