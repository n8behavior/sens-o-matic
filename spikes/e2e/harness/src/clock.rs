//! The time of a simulated run.
//!
//! The library never reads the system time. A run has one [`Clock`] in virtual
//! milliseconds, and only the simulator moves it: [`crate::sim::World`]
//! advances it when no step is ready, and a test can advance it by hand. The
//! server writes the timestamp of an event from this clock (D4 of #61), so a
//! rule that depends on a window ("inside the window") sees the same time on
//! every device and in every run with one seed.
//!
//! `clippy.toml` lists `SystemTime::now` and `Instant::now` as
//! `disallowed-methods`, so `cargo clippy` refuses a call in the library and
//! in the tests of a candidate that copies the file.
//!
//! ```
//! use e2e_harness::clock::{Clock, Millis};
//!
//! let clock = Clock::new();
//! let view = clock.clone(); // a clone shares the time
//! clock.advance(Millis::from_hours(2));
//! assert_eq!(view.now(), Millis::from_secs(7200));
//! ```

use std::{
    cell::Cell,
    fmt,
    ops::{Add, Sub},
    rc::Rc,
};

/// A point in time, or a span of time, in virtual milliseconds.
///
/// A run starts at [`Millis::ZERO`], which stands for no real date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Millis(pub u64);

impl Millis {
    /// The start of a run.
    pub const ZERO: Self = Self(0);

    /// A span of `secs` seconds.
    pub const fn from_secs(secs: u64) -> Self {
        Self(secs * 1_000)
    }

    /// A span of `mins` minutes.
    pub const fn from_mins(mins: u64) -> Self {
        Self::from_secs(mins * 60)
    }

    /// A span of `hours` hours.
    pub const fn from_hours(hours: u64) -> Self {
        Self::from_mins(hours * 60)
    }
}

impl Add for Millis {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl Sub for Millis {
    type Output = Self;

    /// The span between two points; zero when `other` is later.
    fn sub(self, other: Self) -> Self {
        Self(self.0.saturating_sub(other.0))
    }
}

impl fmt::Display for Millis {
    /// `1h02m03.004s`, with the leading units left out when they are zero.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let total_secs = self.0 / 1_000;
        let (hours, mins, secs) = (total_secs / 3_600, total_secs / 60 % 60, total_secs % 60);
        let millis = self.0 % 1_000;
        if hours > 0 {
            write!(f, "{hours}h{mins:02}m{secs:02}.{millis:03}s")
        } else if mins > 0 {
            write!(f, "{mins}m{secs:02}.{millis:03}s")
        } else {
            write!(f, "{secs}.{millis:03}s")
        }
    }
}

/// The virtual clock of one run.
///
/// A clone is another handle to the same time: the server holds one and the
/// world holds one. The run is single-threaded, so the handle is not `Send`.
#[derive(Debug, Clone, Default)]
pub struct Clock {
    now: Rc<Cell<Millis>>,
}

impl Clock {
    /// A clock at [`Millis::ZERO`].
    pub fn new() -> Self {
        Self::default()
    }

    /// The time now.
    pub fn now(&self) -> Millis {
        self.now.get()
    }

    /// Move the time forward by `span`. The time never goes back.
    pub fn advance(&self, span: Millis) {
        self.now.set(self.now.get() + span);
    }

    /// Move the time forward to `target`; do nothing when it is already past.
    pub fn advance_to(&self, target: Millis) {
        if target > self.now.get() {
            self.now.set(target);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_leaves_out_zero_units() {
        assert_eq!(Millis(4).to_string(), "0.004s");
        assert_eq!(Millis::from_mins(5).to_string(), "5m00.000s");
        assert_eq!(
            (Millis::from_hours(3) + Millis(5)).to_string(),
            "3h00m00.005s"
        );
    }

    #[test]
    fn the_clock_never_goes_back() {
        let clock = Clock::new();
        clock.advance_to(Millis::from_secs(10));
        clock.advance_to(Millis::from_secs(5));
        assert_eq!(clock.now(), Millis::from_secs(10));
    }
}
