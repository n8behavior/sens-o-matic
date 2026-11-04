//! MLS (RFC 9420) with OpenMLS behind `GroupCipher`: one of the two candidates of
//! the E2E spike (D9 of #61; the other is the plain group key of #68). The harness
//! with the scenarios is `spikes/e2e/harness` (#67). The person chooses after both
//! candidates report, so this crate exists to show what MLS buys and what it
//! costs, with numbers; it may be deleted after the decision.
//!
//! # Map
//!
//! | Module | What it holds |
//! |---|---|
//! | [`cipher`] | [`OpenMls`], the implementation of `GroupCipher`: the flows of D11 and D6 as Commits and Welcomes, the key of a ping, the check of every Commit against the roster |
//! | [`provider`] | The OpenMLS provider (crypto, randomness, storage) whose only source of randomness is the generator of the harness, and its HPKE |
//! | [`identity`] | The credential of a device: its identity key is its signature key |
//! | [`wire`] | The bytes of the material of an event and of the saved state |
//!
//! # Tests
//!
//! - `tests/scenarios.rs` runs the 13 scenarios of the harness and
//!   `same_seed_twice_gives_identical_logs` against [`OpenMls`].
//! - The other files of `tests/` answer one question of #62 each: `clock`
//!   (question 5), `ping_epoch` (1), `joining` (2 and 8), `fork` (3), `state` (4
//!   and C13), `leaver` (7), and `provider` (the HPKE against `hpke-rs`).
//! - `tests/numbers.rs` prints the sizes and the times of #69; it reads the wall
//!   clock, so it is `#[ignore]`: `cargo test --release -- --ignored --nocapture`.
//!
//! The code reads neither the system time nor the OS randomness (`clippy.toml`
//! refuses both). The version of OpenMLS is pinned with `=` in `Cargo.toml`.

pub mod cipher;
pub mod identity;
pub mod provider;
pub mod wire;

pub use cipher::OpenMls;
