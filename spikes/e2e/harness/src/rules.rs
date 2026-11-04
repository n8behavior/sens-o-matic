//! The rules on replay (D4 c of #61).
//!
//! The server orders the group log and cannot judge it. Every device applies
//! the same rules to the same log, in the order of the log, and ends in the same
//! state; an event that breaks a rule is ignored, and every device ignores it the
//! same way. [`GroupState::apply`] is that function: it has no I/O, no clock and
//! no randomness, and the time of an event is the timestamp in its envelope.
//!
//! The rules are small and are all here:
//!
//! 1. **The first event creates the group.** `Create` at position 0 makes the
//!    author the first device of the first user, with the introduction it
//!    carries. Any other event before that is refused, and a second `Create` is
//!    refused.
//! 2. **An event of a device that no user vouches for is ignored.** A device is
//!    vouched for by the `Create` (the founder), by the `Approve` of its join
//!    request, or by an `AddDevice` of a device of the same user. The one
//!    exception is a `JoinRequest`: asking to be vouched for is what an
//!    unvouched device does.
//! 3. **An invite code is single use and has no expiry** (D11). The first valid
//!    `JoinRequest` for an issued code holds the code; a later request is refused
//!    as used. A request for a revoked or an unknown code is refused. A device
//!    has at most one request waiting, so that it cannot burn several codes.
//! 4. **A member may revoke a code that is not used yet.** Once a request holds
//!    the code, a `Revoke` is refused; the members simply do not approve.
//! 5. **Only a request that holds its code can be approved**, once, by a
//!    vouched device. The requester becomes a new user whose id is the position
//!    of the `Approve`.
//! 6. **A member removes another user, not themselves.** The leaver's own
//!    `Leave` changes no key; another member writes the `Remove` (question 7 of
//!    #62). A removed user and all their devices are unvouched from that
//!    position on, so a late event of theirs is ignored.
//!
//! A refused event is not an error of the device that reads it: it is data of
//! the log. [`crate::device::Device`] records it with the position and the
//! [`Refusal`], so that a test can see that two devices refused the same events.
//!
//! ```
//! use e2e_harness::ids::{InviteCode, Material, Seq};
//! use e2e_harness::keys::Identity;
//! use e2e_harness::payload::GroupChange;
//! use e2e_harness::rng::{Seed, SimRng};
//! use e2e_harness::rules::{GroupState, Refusal};
//!
//! let mut rng = SimRng::new(Seed(1));
//! let (alice, bob, carol) = (
//!     Identity::generate(&mut rng).device_key(),
//!     Identity::generate(&mut rng).device_key(),
//!     Identity::generate(&mut rng).device_key(),
//! );
//! let code = InviteCode::generate(&mut rng);
//! let ask = |code| GroupChange::JoinRequest { code, introduction: Material::empty() };
//!
//! let mut state = GroupState::default();
//! let create = GroupChange::Create { introduction: Material::empty() };
//! state.apply(Seq(0), &alice, &create)?;
//! state.apply(Seq(1), &alice, &GroupChange::Invite { code })?;
//!
//! // Bob asks first, so he holds the code; Carol's later request is refused.
//! state.apply(Seq(2), &bob, &ask(code))?;
//! assert_eq!(state.apply(Seq(3), &carol, &ask(code)), Err(Refusal::CodeUsed(code)));
//!
//! // Only Alice can approve; Bob cannot approve himself.
//! assert_eq!(
//!     state.apply(Seq(4), &bob, &GroupChange::Approve { request: Seq(2) }),
//!     Err(Refusal::Unvouched)
//! );
//! state.apply(Seq(5), &alice, &GroupChange::Approve { request: Seq(2) })?;
//! assert!(state.roster().contains(&bob));
//! assert!(!state.roster().contains(&carol));
//! # Ok::<(), Refusal>(())
//! ```

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use thiserror::Error;

use crate::{
    codec::Writer,
    ids::{InviteCode, Seq, UserId},
    keys::{DeviceKey, EventHash},
    payload::{GroupChange, PingOpen},
    roster::{DeviceInfo, Roster},
};

/// Why an event of the group log is ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Error)]
pub enum Refusal {
    /// The payload is not a group event, or its kind is unknown to this build.
    #[error(
        "the payload is not a group event this build knows (see the module docs of e2e_harness::payload)"
    )]
    Malformed,
    /// There is no group yet: the log has no accepted `Create`.
    #[error(
        "the log has no accepted Create, so there is no group (see rule 1 in the module docs of e2e_harness::rules)"
    )]
    NoGroup,
    /// A `Create` that is not the first event, or a second `Create`.
    #[error(
        "Create is valid only as the first event of the log (see rule 1 in the module docs of e2e_harness::rules)"
    )]
    CreateNotFirst,
    /// No user vouches for the device that wrote the event.
    #[error(
        "no user vouches for the author of the event (see rule 2 in the module docs of e2e_harness::rules)"
    )]
    Unvouched,
    /// The device is already vouched for.
    #[error(
        "the device is already vouched for by a user (see the module docs of e2e_harness::rules)"
    )]
    AlreadyMember,
    /// The device has a join request that waits.
    #[error(
        "the device already has a join request that waits (see rule 3 in the module docs of e2e_harness::rules)"
    )]
    AlreadyRequested,
    /// The code was never issued.
    #[error("the code {0} was never issued (see rule 3 in the module docs of e2e_harness::rules)")]
    UnknownCode(InviteCode),
    /// The code is already issued.
    #[error("the code {0} is already issued (see rule 3 in the module docs of e2e_harness::rules)")]
    CodeTaken(InviteCode),
    /// A request already holds the code: it is single use.
    #[error(
        "the code {0} is single use and a request already holds it (see rule 3 in the module docs of e2e_harness::rules)"
    )]
    CodeUsed(InviteCode),
    /// The code was revoked.
    #[error("the code {0} was revoked (see rule 3 in the module docs of e2e_harness::rules)")]
    CodeRevoked(InviteCode),
    /// No join request waits at that position.
    #[error("no join request waits at {0} (see rule 5 in the module docs of e2e_harness::rules)")]
    UnknownRequest(Seq),
    /// The user is not in the group.
    #[error(
        "the user {0} is not in the group (see rule 6 in the module docs of e2e_harness::rules)"
    )]
    UnknownUser(UserId),
    /// A user removes themselves.
    #[error(
        "a user cannot remove themselves: another member writes the Remove (see rule 6 in the module docs of e2e_harness::rules)"
    )]
    SelfRemoval,
}

/// What the group log says about an invite code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeState {
    /// A member issued it and nobody used it.
    Issued {
        /// The user that made the invite.
        by: UserId,
    },
    /// The request at this position holds it.
    Used {
        /// The position of the `JoinRequest`.
        request: Seq,
    },
    /// A member revoked it before anyone used it.
    Revoked,
}

/// A join request that holds its code and waits for a member's approval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingRequest {
    /// The code the request holds.
    pub code: InviteCode,
    /// The device that asks, with its introduction.
    pub device: Arc<DeviceInfo>,
}

/// The state that the rules derive from the group log.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GroupState {
    roster: Roster,
    codes: BTreeMap<InviteCode, CodeState>,
    requests: BTreeMap<Seq, PendingRequest>,
    leaving: BTreeSet<UserId>,
    last_removal: Option<Seq>,
}

impl GroupState {
    /// Apply the event at `seq` of the group log that `author` wrote.
    ///
    /// On `Err` the state is unchanged. `seq` must be the position of the event
    /// in the log; the caller (the device) has checked the chain.
    pub fn apply(
        &mut self,
        seq: Seq,
        author: &DeviceKey,
        change: &GroupChange,
    ) -> Result<(), Refusal> {
        if self.roster.is_empty() && !matches!(change, GroupChange::Create { .. }) {
            return Err(Refusal::NoGroup);
        }
        let member = self.roster.user_of(author);
        match change {
            GroupChange::Create { introduction } => {
                if seq != Seq::FIRST || !self.roster.is_empty() {
                    return Err(Refusal::CreateNotFirst);
                }
                let founder = Arc::new(DeviceInfo {
                    key: *author,
                    introduction: introduction.clone(),
                });
                self.roster.vouch(UserId::admitted_at(seq), founder);
                Ok(())
            }
            GroupChange::JoinRequest { code, introduction } => {
                if member.is_some() {
                    return Err(Refusal::AlreadyMember);
                }
                if self.request_of(author).is_some() {
                    return Err(Refusal::AlreadyRequested);
                }
                match self.codes.get(code) {
                    None => return Err(Refusal::UnknownCode(*code)),
                    Some(CodeState::Used { .. }) => return Err(Refusal::CodeUsed(*code)),
                    Some(CodeState::Revoked) => return Err(Refusal::CodeRevoked(*code)),
                    Some(CodeState::Issued { .. }) => {}
                }
                self.codes.insert(*code, CodeState::Used { request: seq });
                let device = Arc::new(DeviceInfo {
                    key: *author,
                    introduction: introduction.clone(),
                });
                self.requests.insert(
                    seq,
                    PendingRequest {
                        code: *code,
                        device,
                    },
                );
                Ok(())
            }
            GroupChange::Invite { code } => {
                let user = member.ok_or(Refusal::Unvouched)?;
                if self.codes.contains_key(code) {
                    return Err(Refusal::CodeTaken(*code));
                }
                self.codes.insert(*code, CodeState::Issued { by: user });
                Ok(())
            }
            GroupChange::Revoke { code } => {
                member.ok_or(Refusal::Unvouched)?;
                match self.codes.get(code) {
                    None => Err(Refusal::UnknownCode(*code)),
                    Some(CodeState::Used { .. }) => Err(Refusal::CodeUsed(*code)),
                    Some(CodeState::Revoked) => Err(Refusal::CodeRevoked(*code)),
                    Some(CodeState::Issued { .. }) => {
                        self.codes.insert(*code, CodeState::Revoked);
                        Ok(())
                    }
                }
            }
            GroupChange::Approve { request } => {
                member.ok_or(Refusal::Unvouched)?;
                let pending = self
                    .requests
                    .get(request)
                    .ok_or(Refusal::UnknownRequest(*request))?;
                if self.roster.contains(&pending.device.key) {
                    return Err(Refusal::AlreadyMember);
                }
                let pending = self
                    .requests
                    .remove(request)
                    .expect("the request was found above");
                self.roster.vouch(UserId::admitted_at(seq), pending.device);
                Ok(())
            }
            GroupChange::AddDevice {
                device,
                introduction,
            } => {
                let user = member.ok_or(Refusal::Unvouched)?;
                if self.roster.contains(device) {
                    return Err(Refusal::AlreadyMember);
                }
                // The device is vouched for now: a request of it has no use.
                self.requests
                    .retain(|_, request| request.device.key != *device);
                self.roster.vouch(
                    user,
                    Arc::new(DeviceInfo {
                        key: *device,
                        introduction: introduction.clone(),
                    }),
                );
                Ok(())
            }
            GroupChange::Leave => {
                let user = member.ok_or(Refusal::Unvouched)?;
                self.leaving.insert(user);
                Ok(())
            }
            GroupChange::Remove { user } => {
                let remover = member.ok_or(Refusal::Unvouched)?;
                if !self.roster.contains_user(*user) {
                    return Err(Refusal::UnknownUser(*user));
                }
                if *user == remover {
                    return Err(Refusal::SelfRemoval);
                }
                self.roster.remove(*user);
                self.leaving.remove(user);
                self.last_removal = Some(seq);
                Ok(())
            }
        }
    }

    /// The users and devices that the log vouches for.
    pub fn roster(&self) -> &Roster {
        &self.roster
    }

    /// What the log says about a code, or `None` when it was never issued.
    pub fn code(&self, code: &InviteCode) -> Option<CodeState> {
        self.codes.get(code).copied()
    }

    /// The join requests that hold their code and wait for an approval.
    pub fn pending_requests(&self) -> impl Iterator<Item = (Seq, &PendingRequest)> + '_ {
        self.requests.iter().map(|(seq, request)| (*seq, request))
    }

    /// The position of the pending request of `device`, if there is one.
    pub fn request_of(&self, device: &DeviceKey) -> Option<Seq> {
        self.pending_requests()
            .find(|(_, request)| request.device.key == *device)
            .map(|(seq, _)| seq)
    }

    /// The pending request at `seq`.
    pub fn request(&self, seq: Seq) -> Option<&PendingRequest> {
        self.requests.get(&seq)
    }

    /// True when `user` said `Leave` and no member has removed them yet.
    pub fn is_leaving(&self, user: UserId) -> bool {
        self.leaving.contains(&user)
    }

    /// The position of the newest accepted `Remove`, if there is one.
    pub fn last_removal(&self) -> Option<Seq> {
        self.last_removal
    }

    /// True when a member was removed after the ping began (D10: a change of
    /// members ends the ping, and a new ping starts for the new group).
    ///
    /// The ping began at `open.group_head`, so a removal at that position or
    /// later ended it. A device decides this from the group log it has applied,
    /// so two devices agree once they have applied the same prefix.
    pub fn ping_ended(&self, open: &PingOpen) -> bool {
        self.last_removal
            .is_some_and(|removal| removal >= open.group_head)
    }

    /// A hash of the whole state: equal for two devices exactly when they
    /// derived the same state from the log.
    pub fn digest(&self) -> EventHash {
        let mut w = Writer::new();
        self.roster.encode(&mut w);
        w.u32(self.codes.len() as u32);
        for (code, state) in &self.codes {
            code.encode(&mut w);
            match state {
                CodeState::Issued { by } => {
                    w.u8(0);
                    by.encode(&mut w);
                }
                CodeState::Used { request } => {
                    w.u8(1).u64(request.0);
                }
                CodeState::Revoked => {
                    w.u8(2);
                }
            }
        }
        w.u32(self.requests.len() as u32);
        for (seq, request) in &self.requests {
            w.u64(seq.0).raw(request.device.key.as_bytes());
        }
        w.u32(self.leaving.len() as u32);
        for user in &self.leaving {
            user.encode(&mut w);
        }
        w.u64(self.last_removal.map_or(u64::MAX, |seq| seq.0));
        EventHash::of(&w.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ids::Material,
        rng::{Seed, SimRng},
    };

    struct Fixture {
        state: GroupState,
        alice: DeviceKey,
        bob: DeviceKey,
        code: InviteCode,
        next: u64,
    }

    impl Fixture {
        fn new() -> Self {
            let mut rng = SimRng::new(Seed(11));
            let mut fixture = Self {
                state: GroupState::default(),
                alice: crate::keys::Identity::generate(&mut rng).device_key(),
                bob: crate::keys::Identity::generate(&mut rng).device_key(),
                code: InviteCode::generate(&mut rng),
                next: 0,
            };
            fixture.apply(fixture.alice, fixture.create()).unwrap();
            fixture
        }

        fn apply(&mut self, author: DeviceKey, change: GroupChange) -> Result<(), Refusal> {
            let seq = Seq(self.next);
            self.next += 1;
            self.state.apply(seq, &author, &change)
        }

        fn create(&self) -> GroupChange {
            GroupChange::Create {
                introduction: Material::empty(),
            }
        }

        fn ask(&self) -> GroupChange {
            GroupChange::JoinRequest {
                code: self.code,
                introduction: Material::empty(),
            }
        }
    }

    #[test]
    fn a_refused_event_leaves_the_state_alone() {
        let mut f = Fixture::new();
        let before = f.state.digest();
        assert_eq!(f.apply(f.bob, GroupChange::Leave), Err(Refusal::Unvouched));
        assert_eq!(f.apply(f.alice, f.create()), Err(Refusal::CreateNotFirst));
        assert_eq!(f.state.digest(), before);
    }

    #[test]
    fn a_code_cannot_be_issued_twice_or_used_before_it_is_issued() {
        let mut f = Fixture::new();
        assert_eq!(f.apply(f.bob, f.ask()), Err(Refusal::UnknownCode(f.code)));
        f.apply(f.alice, GroupChange::Invite { code: f.code })
            .unwrap();
        assert_eq!(
            f.apply(f.alice, GroupChange::Invite { code: f.code }),
            Err(Refusal::CodeTaken(f.code))
        );
    }

    #[test]
    fn a_revoke_is_too_late_once_a_request_holds_the_code() {
        let mut f = Fixture::new();
        f.apply(f.alice, GroupChange::Invite { code: f.code })
            .unwrap();
        f.apply(f.bob, f.ask()).unwrap();
        assert_eq!(
            f.apply(f.alice, GroupChange::Revoke { code: f.code }),
            Err(Refusal::CodeUsed(f.code))
        );
    }

    #[test]
    fn a_device_with_a_waiting_request_cannot_burn_a_second_code() {
        let mut f = Fixture::new();
        let mut rng = SimRng::new(Seed(12));
        let other = InviteCode::generate(&mut rng);
        f.apply(f.alice, GroupChange::Invite { code: f.code })
            .unwrap();
        f.apply(f.alice, GroupChange::Invite { code: other })
            .unwrap();
        f.apply(f.bob, f.ask()).unwrap();
        let again = GroupChange::JoinRequest {
            code: other,
            introduction: Material::empty(),
        };
        assert_eq!(f.apply(f.bob, again), Err(Refusal::AlreadyRequested));
        assert_eq!(
            f.state.code(&other),
            Some(CodeState::Issued {
                by: UserId::admitted_at(Seq(0))
            })
        );
    }

    #[test]
    fn a_device_that_a_user_vouches_for_loses_its_waiting_request() {
        let mut f = Fixture::new();
        f.apply(f.alice, GroupChange::Invite { code: f.code })
            .unwrap();
        f.apply(f.bob, f.ask()).unwrap();
        f.apply(
            f.alice,
            GroupChange::AddDevice {
                device: f.bob,
                introduction: Material::empty(),
            },
        )
        .unwrap();
        assert_eq!(f.state.request_of(&f.bob), None);
        assert_eq!(
            f.apply(f.alice, GroupChange::Approve { request: Seq(2) }),
            Err(Refusal::UnknownRequest(Seq(2)))
        );
    }

    #[test]
    fn a_request_is_approved_once() {
        let mut f = Fixture::new();
        f.apply(f.alice, GroupChange::Invite { code: f.code })
            .unwrap();
        f.apply(f.bob, f.ask()).unwrap();
        f.apply(f.alice, GroupChange::Approve { request: Seq(2) })
            .unwrap();
        assert_eq!(
            f.apply(f.alice, GroupChange::Approve { request: Seq(2) }),
            Err(Refusal::UnknownRequest(Seq(2)))
        );
        assert_eq!(
            f.state.roster().user_of(&f.bob),
            Some(UserId::admitted_at(Seq(3)))
        );
    }

    #[test]
    fn a_removed_user_is_unvouched_and_cannot_return_by_leaving() {
        let mut f = Fixture::new();
        f.apply(f.alice, GroupChange::Invite { code: f.code })
            .unwrap();
        f.apply(f.bob, f.ask()).unwrap();
        f.apply(f.alice, GroupChange::Approve { request: Seq(2) })
            .unwrap();
        let bob = f.state.roster().user_of(&f.bob).unwrap();

        f.apply(f.bob, GroupChange::Leave).unwrap();
        assert!(f.state.is_leaving(bob));
        assert_eq!(
            f.apply(f.bob, GroupChange::Remove { user: bob }),
            Err(Refusal::SelfRemoval)
        );
        f.apply(f.alice, GroupChange::Remove { user: bob }).unwrap();
        assert!(!f.state.is_leaving(bob));
        assert_eq!(f.apply(f.bob, GroupChange::Leave), Err(Refusal::Unvouched));
        assert_eq!(f.state.last_removal(), Some(Seq(6)));
    }

    #[test]
    fn a_removal_ends_only_the_pings_that_began_before_it() {
        let mut f = Fixture::new();
        f.apply(f.alice, GroupChange::Invite { code: f.code })
            .unwrap();
        f.apply(f.bob, f.ask()).unwrap();
        f.apply(f.alice, GroupChange::Approve { request: Seq(2) })
            .unwrap();
        let bob = f.state.roster().user_of(&f.bob).unwrap();
        let open = |head| PingOpen {
            group_head: Seq(head),
            material: Material::empty(),
        };
        assert!(!f.state.ping_ended(&open(4)));
        f.apply(f.alice, GroupChange::Remove { user: bob }).unwrap();
        assert!(f.state.ping_ended(&open(4)));
        assert!(!f.state.ping_ended(&open(5)));
    }
}
