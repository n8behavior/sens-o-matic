use chrono::{Datelike, Duration, NaiveTime, Utc, Weekday};

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

#[cfg(test)]
mod test {
    use chrono::{Datelike, Duration, NaiveTime, Utc, Weekday};

    use crate::when::{Availability, AvailabilityWindow, RecurringAvailability, TimeRange};

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
}
