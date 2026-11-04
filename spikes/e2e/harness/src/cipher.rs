//! `GroupCipher`: what a device needs from a group-encryption protocol.
//!
//! The two candidates of the spike (the plain group key, #68, and MLS with
//! OpenMLS, #69) implement this trait in their own crates, and the scenarios of
//! [`crate::scenarios`] run against it. A candidate may change the trait in its
//! own pull request, and then says so in a comment on its issue.
//!
//! # How a device uses it
//!
//! One cipher belongs to one device. The device (see [`crate::device`]) signs
//! the events, keeps the hash chain, applies the rules ([`crate::rules`]) and
//! calls the cipher for the cryptography only:
//!
//! - To **make** an event the cipher returns [`Material`]; the device wraps it
//!   in the payload of a group event ([`crate::payload::GroupPayload`]) and
//!   appends it. A `make_*` method must not change what [`apply_group_event`]
//!   yields later: the event takes effect when the log gives it back, for the
//!   author as for everyone else. That is what lets the loser of a race retry
//!   (the conditional append refuses it, the cipher state is untouched, and the
//!   next attempt is made from the new state).
//! - To **apply** an event the device calls [`apply_group_event`] in log order,
//!   once for each event that the rules accepted, and only while the device is a
//!   member after that event: a cipher never sees an event of the time before
//!   its device joined, and never sees the event that removes its device.
//! - A ping event is encrypted with [`encrypt_ping`] and read with
//!   [`decrypt_ping`]. The first event of a ping log is made by [`start_ping`]
//!   and read by [`open_ping`].
//!
//! Every method that can fail returns a [`CipherError`]. An error from
//! [`apply_group_event`] means that the event is invalid *for the protocol*; the
//! device then ignores the event, as it ignores one that breaks a rule, and every
//! device must reach the same verdict.
//!
//! # What the scenarios rely on
//!
//! 1. **Convergence.** Devices that apply the same accepted group events in the
//!    same order can all [`decrypt_ping`] what any of them [`encrypt_ping`]s for
//!    a ping that is current for all of them.
//! 2. **Determinism.** The bytes that a cipher returns are a function of its
//!    inputs and of the generator it was given. A cipher reads no clock and no
//!    OS randomness (`clippy.toml` refuses the calls), so
//!    `same_seed_twice_gives_identical_logs` holds.
//! 3. **A removed member cannot read what is encrypted after the removal.** A
//!    ping started after a [`remove_member`] is applied is unreadable for the
//!    removed user's devices. What the removed user could read before stays
//!    readable (D10 accepts it).
//! 4. **A joiner reads from the join on.** What a new member can read of the
//!    past is the candidate's choice; the scenarios report it and assert nothing
//!    about it.
//! 5. **A new device of a known user** can read what its sibling devices could
//!    read at the time, or less, and everything written after it is added.
//! 6. **Restore.** [`export`] then [`restore`] gives a cipher that continues as
//!    if it had never stopped.
//!
//! [`apply_group_event`]: GroupCipher::apply_group_event
//! [`encrypt_ping`]: GroupCipher::encrypt_ping
//! [`decrypt_ping`]: GroupCipher::decrypt_ping
//! [`start_ping`]: GroupCipher::start_ping
//! [`open_ping`]: GroupCipher::open_ping
//! [`remove_member`]: GroupCipher::remove_member
//! [`export`]: GroupCipher::export
//! [`restore`]: GroupCipher::restore

use thiserror::Error;

use crate::{
    ids::{GroupId, InviteCode, Material, PingId, Seq, UserId},
    keys::{DeviceKey, Identity},
    payload::GroupChange,
    rng::SimRng,
    roster::{DeviceInfo, Roster},
};

/// Why a cipher could not do what the device asked.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CipherError {
    /// The device does not hold the key that the operation needs: it never had
    /// it (a joiner and a past epoch, a removed member and a later epoch).
    #[error("the device has no key for this: {0}")]
    NoKey(String),
    /// The input is not valid for the protocol: a Commit that does not match,
    /// a forged message, a malformed Welcome.
    #[error("the protocol refuses the input: {0}")]
    Invalid(String),
    /// The cipher is in a state that cannot do this: it has no group yet.
    #[error("the cipher is in the wrong state: {0}")]
    State(String),
}

/// An accepted group-log event, as a cipher sees it.
#[derive(Debug, Clone, Copy)]
pub struct GroupEvent<'a> {
    /// The position in the group log.
    pub seq: Seq,
    /// The device that wrote it.
    pub author: &'a DeviceKey,
    /// What it does, in the clear.
    pub change: &'a GroupChange,
    /// What the author's cipher made for it.
    pub material: &'a Material,
    /// The roster after the rules applied this event. For `Approve` it holds the
    /// new user; for `Remove` it no longer holds the removed user.
    pub roster: &'a Roster,
}

/// What a device needs from a group-encryption protocol. See the module docs.
pub trait GroupCipher: Sized {
    /// The name of the candidate, for the reports.
    const NAME: &'static str;

    /// A cipher for a device with no group. `rng` is its own stream of the seeded
    /// generator: take every random number from it.
    fn new(identity: &Identity, rng: SimRng) -> Self;

    /// The state of the cipher as bytes, to restore it later. Take it between
    /// two events; it holds everything the cipher needs to continue.
    fn export(&self) -> Vec<u8>;

    /// A cipher that continues from [`export`](Self::export). `rng` is a fresh
    /// stream: the restored cipher does not repeat the numbers of the old one.
    fn restore(identity: &Identity, rng: SimRng, state: &[u8]) -> Result<Self, CipherError>;

    /// What others need to hand this device keys: an MLS key package, a public
    /// key for HPKE. This is the cipher's part of a join request (D11): it goes
    /// in the clear into the `JoinRequest` of a joiner or the `AddDevice` of a
    /// new device of a known user, so it must hold nothing secret.
    ///
    /// The device calls it each time it needs one, so a cipher with one-time
    /// packages makes a fresh one on each call.
    fn introduction(&mut self) -> Material;

    /// The material of the `Create` event: the founder makes the group.
    ///
    /// After [`apply_group_event`](Self::apply_group_event) of that event the
    /// founder can encrypt for the group.
    fn create_group(&mut self, group: GroupId) -> Result<Material, CipherError>;

    /// The material of an `Invite` event for `code`. Many protocols need none;
    /// an MLS member publishes what a joiner needs for an external join.
    ///
    /// The code is a lookup name and never a key (D11): do not derive a secret
    /// from it.
    fn invite(&mut self, code: &InviteCode) -> Result<Material, CipherError>;

    /// The material of a `Revoke` event for `code`.
    fn revoke_code(&mut self, code: &InviteCode) -> Result<Material, CipherError>;

    /// The material of the `Approve` event for a join request (D11): the keys of
    /// the group, locked to `joiner`, made by a member's device.
    ///
    /// After [`apply_group_event`](Self::apply_group_event) of that event the
    /// joiner can read what is encrypted from then on. `roster` is the roster
    /// before the join.
    fn approve_join(
        &mut self,
        roster: &Roster,
        joiner: &DeviceInfo,
    ) -> Result<Material, CipherError>;

    /// The material of the `AddDevice` event: a device of a known user hands
    /// `device` the keys it may read (D6). `roster` is the roster before.
    ///
    /// Nobody but a sibling device is needed, and the server learns no secret
    /// (C3 of #62).
    fn add_device(&mut self, roster: &Roster, device: &DeviceInfo)
    -> Result<Material, CipherError>;

    /// The material of the `Remove` event: a new key for everyone who stays, made
    /// by a member other than `user` (D10: the ping ends). `roster` is the roster
    /// before the removal.
    ///
    /// After [`apply_group_event`](Self::apply_group_event) of that event the
    /// devices that stay can read what is encrypted from then on, and the
    /// devices of `user` cannot.
    fn remove_member(&mut self, roster: &Roster, user: UserId) -> Result<Material, CipherError>;

    /// Apply an accepted group event, in log order. See the module docs for which
    /// events a cipher gets.
    ///
    /// This is the only call that changes what the cipher can read of the group.
    /// Return an error only when the event is invalid for the protocol, and then
    /// leave the state as it was: the device ignores the event.
    fn apply_group_event(&mut self, event: &GroupEvent<'_>) -> Result<(), CipherError>;

    /// The material of the first event of the log of `ping`: the key of the ping
    /// (D3 and question 1 of #62). `group_head` is the head of the group log that
    /// the device has applied, the epoch that the ping belongs to.
    fn start_ping(&mut self, ping: PingId, group_head: Seq) -> Result<Material, CipherError>;

    /// Read the first event of the log of `ping`, so that
    /// [`decrypt_ping`](Self::decrypt_ping) and
    /// [`encrypt_ping`](Self::encrypt_ping) work for it. An error means the
    /// device cannot read this ping at all.
    fn open_ping(
        &mut self,
        ping: PingId,
        group_head: Seq,
        material: &Material,
    ) -> Result<(), CipherError>;

    /// Encrypt an event of `ping` under the key of that ping.
    fn encrypt_ping(&mut self, ping: PingId, plaintext: &[u8]) -> Result<Vec<u8>, CipherError>;

    /// Decrypt the event at `seq` of the log of `ping`. Events arrive in log
    /// order; `Err(NoKey)` is the normal answer for a leaver and a new device.
    fn decrypt_ping(
        &mut self,
        ping: PingId,
        seq: Seq,
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, CipherError>;
}
