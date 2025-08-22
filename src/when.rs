use chrono::{DateTime, Duration, Utc};

#[derive(Debug, Clone)]
pub struct Availability {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

impl Availability {
    pub fn new(start: DateTime<Utc>, duration: Duration) -> Self {
        let duration = duration.abs(); // Ensure positive duration
        Self {
            start,
            end: start + duration,
        }
    }

    pub fn overlaps_with(&self, other: &Availability) -> bool {
        self.start < other.end && other.start < self.end
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn test_availability_creation() {
        let start = Utc::now();
        let duration = Duration::hours(2);
        let availability = Availability::new(start, duration);

        assert_eq!(availability.start, start);
        assert_eq!(availability.end, start + duration);
    }

    #[test]
    fn test_negative_duration_becomes_positive() {
        let start = Utc::now();
        let negative_duration = Duration::hours(-3);
        let availability = Availability::new(start, negative_duration);

        // Should use absolute value of duration
        assert_eq!(availability.start, start);
        assert_eq!(availability.end, start + Duration::hours(3));
        assert!(availability.end > availability.start);
    }

    #[test]
    fn test_overlapping_availabilities() {
        let now = Utc::now();
        let avail1 = Availability::new(now, Duration::hours(2));
        let avail2 = Availability::new(now + Duration::hours(1), Duration::hours(2));
        let avail3 = Availability::new(now + Duration::hours(3), Duration::hours(1));

        assert!(avail1.overlaps_with(&avail2));
        assert!(avail2.overlaps_with(&avail1));
        assert!(!avail1.overlaps_with(&avail3));
        assert!(!avail3.overlaps_with(&avail1));
    }
}
