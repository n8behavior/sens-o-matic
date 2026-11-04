//! The harness of the E2E spike (D5 of #61).
//!
//! The spike runs the same scenarios against two ways to encrypt a group, MLS
//! with OpenMLS (#69) and a plain group key (#68), and the person chooses (D9).
//! This crate is what both candidates share: a deterministic simulator, the log
//! they write to, a server that can lie, the rules every device applies, and
//! the scenarios. It may be deleted after the decision; its types may be kept
//! for the shared crate of D2.
//!
//! # Map
//!
//! | Module | What it holds |
//! |---|---|
//! | [`clock`], [`rng`] | The injected time and the seeded randomness; the library reads neither the system time nor the OS |
//! | [`ids`], [`keys`] | The newtypes: groups, pings, positions, users, codes, device keys, hashes |
//! | [`event`] | The envelope of an event: hash chain, author signature, timestamp of the server |
//! | [`log`], [`server`] | The storage trait (D7) with `MemoryLog`, and the server in front of it |
//! | [`payload`], [`roster`], [`rules`] | The group events in the clear, the members, and the rules on replay (D4 c) |
//! | [`cipher`], [`fake`] | `GroupCipher`, the contract of the candidates, and `FakeCipher`, which has no cryptography |
//! | [`device`] | An honest client: checks the chain, applies the rules, writes |
//! | [`sim`] | The world: one thread, a seeded scheduler, fault injection |
//! | [`evil`] | A `Log` wrapper that drops, holds back, reorders or forks, and how long devices take to notice |
//! | [`scenarios`] | The scenarios, generic over `GroupCipher`, and `run_all` |
//!
//! # For a candidate
//!
//! Implement [`cipher::GroupCipher`] in your own crate, with a path dependency
//! on this one, and run every scenario with one line:
//!
//! ```
//! use e2e_harness::fake::FakeCipher; // your cipher, in your crate
//!
//! e2e_harness::scenario_tests!(fake, FakeCipher);
//! # fn main() {}
//! ```
//!
//! The macro writes one `#[test]` for each scenario, named as the scenario, and
//! `same_seed_twice_gives_identical_logs`. Copy `clippy.toml` and the
//! `[profile.dev.package."*"]` table of `Cargo.toml` too: the first keeps the
//! system time and the OS randomness out, the second keeps 90 signatures
//! fast. The scenario functions return a [`scenarios::Report`] that you can
//! print for the numbers of your issue.
//!
//! # Limits
//!
//! The harness models what #61 asks and no more. The server does not check that
//! a caller is a member (the rules do, on replay), has no rate limit, and does
//! not forbid a removed user to read a log; a ping event is not checked against
//! the roster at the time it was written, because the order of two logs is not
//! defined (D10 closes it by ending the ping). See [`rules`] and [`device`].

pub mod cipher;
pub mod clock;
mod codec;
pub mod device;
pub mod event;
pub mod evil;
pub mod fake;
pub mod ids;
pub mod keys;
pub mod log;
pub mod payload;
pub mod rng;
pub mod roster;
pub mod rules;
pub mod scenarios;
pub mod server;
pub mod sim;

pub use codec::DecodeError;
pub use rng::Seed;
