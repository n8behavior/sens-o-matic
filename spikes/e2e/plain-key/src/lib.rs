//! The plain group key, behind `GroupCipher`: a candidate of the E2E spike (D9 of
//! #61), and the bar that MLS with OpenMLS (#69) must beat.
//!
//! One key for the group, which every device of the group holds, and a new key
//! after a member leaves or is removed (SPEC section 9). The key of a ping lives
//! in the first event of the ping log. No primitive is written here: HPKE
//! (RFC 9180) wraps a key for a device, XChaCha20-Poly1305 seals the events, and
//! SHA-256 commits to a key. The protocol on top is ours and unaudited, which is
//! the price of this candidate.
//!
//! ```
//! use e2e_harness::{Seed, sim::World};
//! use e2e_plain_key::PlainKey;
//!
//! // Alice founds a group, Bob joins with a code that Alice approves, and they
//! // talk in a ping.
//! let mut world = World::<PlainKey>::new(Seed(1));
//! let alice = world.found_group();
//! let bob = world.admit(alice);
//! let ping = world.start_ping(alice);
//! world.post(bob, ping, b"hello");
//! world.run_until_idle();
//!
//! let view = world.device(alice).ping(ping).expect("Alice read the ping");
//! assert_eq!(view.readable().values().next().unwrap(), b"hello");
//! ```
//!
//! # Map
//!
//! | Module | What it holds |
//! |---|---|
//! | [`cipher`] | [`PlainKey`]: the keys, who gets which key from which event, and what ends a ping |
//! | [`material`] | The messages of the protocol, their layout, and what every device checks |
//! | [`crypto`] | The primitives and the crates they come from |
//!
//! # Tests
//!
//! `tests/scenarios.rs` runs the 13 scenarios of the harness
//! (`e2e_harness::scenario_tests!`). `tests/questions.rs` and `tests/attacks.rs`
//! hold the cases that the eight questions of #62 add, and `tests/numbers.rs`
//! (ignored, it reads the wall clock) prints the sizes and the times. The answers
//! are in the comments `## Numbers` and `## Answers` of #68.
//!
//! # Randomness and time
//!
//! The library reads neither the clock nor the OS randomness: every key, nonce
//! and ephemeral key of HPKE comes from the `SimRng` that the cipher is given, and
//! `clippy.toml` refuses the calls. The crates are built without default
//! features, so the OS randomness is not compiled in.

use std::fmt;

use e2e_harness::cipher::CipherError;

pub mod cipher;
pub mod crypto;
pub mod material;
mod ring;
mod wire;

pub use cipher::PlainKey;

/// A material or a wrap that the protocol refuses, with the doc that says why.
pub(crate) fn invalid(what: impl fmt::Display) -> CipherError {
    CipherError::Invalid(format!(
        "{what} (see the module docs of e2e_plain_key::material)"
    ))
}
