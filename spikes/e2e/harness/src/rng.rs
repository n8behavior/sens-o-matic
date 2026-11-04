//! The randomness of a simulated run.
//!
//! The library never reads the OS randomness. A run has one [`Seed`], and every
//! random choice comes from a [`SimRng`] that descends from it: the keys of the
//! devices, the invite codes, the pick of the next step, the faults. Two runs
//! with one seed make the same choices and store the same bytes
//! (`same_seed_twice_gives_identical_logs`); a failing run replays from its
//! seed.
//!
//! One generator would couple the streams: a candidate that draws one more
//! nonce would shift the scheduler. [`SimRng::fork`] gives each consumer its
//! own stream, so the scheduler picks the same steps whatever a cipher draws.
//!
//! `clippy.toml` lists `rand::rng` and `rand::random` as `disallowed-methods`.
//!
//! ```
//! use e2e_harness::rng::{Seed, SimRng};
//! use rand::RngCore;
//!
//! let mut a = SimRng::new(Seed(7));
//! let mut b = SimRng::new(Seed(7));
//! assert_eq!(a.next_u64(), b.next_u64());
//!
//! let mut child = a.fork(); // its own stream; `a` moves on
//! assert_ne!(child.next_u64(), a.next_u64());
//! ```

use rand::{CryptoRng, Rng, RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;

/// The seed of one run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Seed(pub u64);

impl Default for Seed {
    /// The seed that `run_all` and the tests use when they take none.
    fn default() -> Self {
        Self(0xE2E)
    }
}

/// A chance between 0 and 1, so that a rate outside that range cannot exist.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Probability(f64);

impl Probability {
    /// It never happens.
    pub const NEVER: Self = Self(0.0);
    /// It always happens.
    pub const ALWAYS: Self = Self(1.0);

    /// The chance `p`, or `None` when `p` is not in `0.0..=1.0`.
    pub fn new(p: f64) -> Option<Self> {
        (0.0..=1.0).contains(&p).then_some(Self(p))
    }

    /// The chance as a number.
    pub fn get(self) -> f64 {
        self.0
    }
}

/// A seeded generator: ChaCha20, so a candidate can take keys from it.
///
/// The output has the quality of a CSPRNG, but the seed is a small number that
/// anyone can guess: never use it outside a simulation.
#[derive(Debug, Clone)]
pub struct SimRng(ChaCha20Rng);

impl SimRng {
    /// The generator of `seed`.
    pub fn new(seed: Seed) -> Self {
        Self(ChaCha20Rng::seed_from_u64(seed.0))
    }

    /// A new generator with its own stream, seeded from this one.
    pub fn fork(&mut self) -> Self {
        let mut seed = [0u8; 32];
        self.0.fill_bytes(&mut seed);
        Self(ChaCha20Rng::from_seed(seed))
    }

    /// A number below `bound`.
    ///
    /// # Panics
    ///
    /// Panics when `bound` is zero: there is no number below it.
    pub fn below(&mut self, bound: usize) -> usize {
        assert!(bound > 0, "SimRng::below needs a bound above zero");
        self.0.random_range(0..bound)
    }

    /// A number in `low..=high`.
    pub fn between(&mut self, low: u64, high: u64) -> u64 {
        self.0.random_range(low..=high)
    }

    /// True with the chance `p`.
    pub fn chance(&mut self, p: Probability) -> bool {
        self.0.random_bool(p.get())
    }

    /// An item of `items`, or `None` when it is empty.
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        (!items.is_empty()).then(|| &items[self.below(items.len())])
    }
}

impl RngCore for SimRng {
    fn next_u32(&mut self) -> u32 {
        self.0.next_u32()
    }

    fn next_u64(&mut self) -> u64 {
        self.0.next_u64()
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        self.0.fill_bytes(dest);
    }
}

impl CryptoRng for SimRng {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fork_does_not_shift_its_parent() {
        let mut a = SimRng::new(Seed(1));
        let mut b = SimRng::new(Seed(1));
        let mut child = a.fork();
        let _ = b.fork();
        for _ in 0..10 {
            child.next_u64();
        }
        assert_eq!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn a_probability_stays_in_range() {
        assert!(Probability::new(1.5).is_none());
        assert!(Probability::new(-0.1).is_none());
        assert!(Probability::new(f64::NAN).is_none());
        assert!(Probability::new(0.25).is_some());
    }
}
