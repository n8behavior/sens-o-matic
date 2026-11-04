//! A rig that drives the ciphers by hand: no simulator, no log, no server.
//!
//! The rig does what a [`Device`](e2e_harness::device::Device) does with an event
//! of the group log: it runs the rules, and then calls `apply_group_event` of
//! the cipher of each device that is a member after the event. A test can hand it
//! any material, also a forged one, and read the verdict of every device.

#![allow(dead_code, reason = "each test file uses part of the rig")]

use e2e_harness::{
    cipher::{CipherError, GroupCipher, GroupEvent},
    ids::{GroupId, InviteCode, Material, Seq, UserId},
    keys::{DeviceKey, Identity},
    payload::GroupChange,
    rng::{Seed, SimRng},
    roster::DeviceInfo,
    rules::{GroupState, Refusal},
};
use e2e_plain_key::PlainKey;

pub struct Member {
    pub identity: Identity,
    pub cipher: PlainKey,
}

pub struct Rig {
    rng: SimRng,
    pub state: GroupState,
    pub next: Seq,
    pub members: Vec<Member>,
}

/// What each device that is a member after an event said about it.
pub type Verdicts = Vec<(usize, Result<(), CipherError>)>;

impl Rig {
    pub fn new(seed: u64) -> Self {
        Self {
            rng: SimRng::new(Seed(seed)),
            state: GroupState::default(),
            next: Seq::FIRST,
            members: Vec::new(),
        }
    }

    /// A device that is in no group yet.
    pub fn add_device(&mut self) -> usize {
        let identity = Identity::generate(&mut self.rng.fork());
        let cipher = PlainKey::new(&identity, self.rng.fork());
        self.members.push(Member { identity, cipher });
        self.members.len() - 1
    }

    pub fn key(&self, device: usize) -> DeviceKey {
        self.members[device].identity.device_key()
    }

    pub fn info(&mut self, device: usize) -> DeviceInfo {
        DeviceInfo {
            key: self.key(device),
            introduction: self.members[device].cipher.introduction(),
        }
    }

    pub fn user(&self, device: usize) -> UserId {
        self.state
            .roster()
            .user_of(&self.key(device))
            .expect("the device is in the group")
    }

    /// Run the event through the rules and then through each cipher that should
    /// see it. A refusal of the rules is an `Err`; the position is used up, as in
    /// a log.
    pub fn deliver(
        &mut self,
        author: usize,
        change: GroupChange,
        material: Material,
    ) -> Result<Verdicts, Refusal> {
        let seq = self.next;
        self.next = seq.next();
        let author_key = self.key(author);
        let mut after = self.state.clone();
        after.apply(seq, &author_key, &change)?;
        let roster = after.roster().clone();
        let verdicts = self
            .members
            .iter_mut()
            .enumerate()
            .filter(|(_, member)| roster.contains(&member.identity.device_key()))
            .map(|(index, member)| {
                let event = GroupEvent {
                    seq,
                    author: &author_key,
                    change: &change,
                    material: &material,
                    roster: &roster,
                };
                (index, member.cipher.apply_group_event(&event))
            })
            .collect();
        self.state = after;
        Ok(verdicts)
    }

    /// Deliver an event and require that it was accepted and that every cipher
    /// said yes.
    pub fn deliver_ok(&mut self, author: usize, change: GroupChange, material: Material) {
        let verdicts = self
            .deliver(author, change, material)
            .expect("the rules accept the event");
        for (device, verdict) in verdicts {
            verdict.unwrap_or_else(|error| panic!("device {device} refused the event: {error}"));
        }
    }

    /// Device 0 founds the group.
    pub fn found(&mut self, founder: usize) {
        let introduction = self.members[founder].cipher.introduction();
        let material = self.members[founder]
            .cipher
            .create_group(GroupId(1))
            .expect("the founder makes the group");
        self.deliver_ok(founder, GroupChange::Create { introduction }, material);
    }

    /// `joiner` joins with a code that `approver` issues and approves.
    pub fn admit(&mut self, approver: usize, joiner: usize) {
        let code = InviteCode::generate(&mut self.rng);
        let invite = self.members[approver].cipher.invite(&code).unwrap();
        self.deliver_ok(approver, GroupChange::Invite { code }, invite);

        let info = self.info(joiner);
        self.deliver_ok(
            joiner,
            GroupChange::JoinRequest {
                code,
                introduction: info.introduction.clone(),
            },
            Material::empty(),
        );
        let request = Seq(self.next.0 - 1);
        let material = self.members[approver]
            .cipher
            .approve_join(self.state.roster(), &info)
            .expect("the approver makes the approval");
        self.deliver_ok(approver, GroupChange::Approve { request }, material);
    }

    /// `owner` vouches for `new`, a new device of the same user.
    pub fn add_device_to(&mut self, owner: usize, new: usize) {
        let info = self.info(new);
        let material = self.members[owner]
            .cipher
            .add_device(self.state.roster(), &info)
            .expect("the sibling makes the handover");
        self.deliver_ok(
            owner,
            GroupChange::AddDevice {
                device: info.key,
                introduction: info.introduction,
            },
            material,
        );
    }

    /// The material that `remover` makes to remove `user`, not yet delivered.
    pub fn removal(&mut self, remover: usize, user: UserId) -> Material {
        let roster = self.state.roster().clone();
        self.members[remover]
            .cipher
            .remove_member(&roster, user)
            .expect("the remover makes the rekey")
    }

    /// `remover` removes `user`.
    pub fn remove(&mut self, remover: usize, user: UserId) {
        let material = self.removal(remover, user);
        self.deliver_ok(remover, GroupChange::Remove { user }, material);
    }

    /// A group of `devices` ciphers: the first founds it, the others join as
    /// users, each approved by the first.
    pub fn group_of(seed: u64, users: usize) -> Self {
        let mut rig = Self::new(seed);
        let founder = rig.add_device();
        rig.found(founder);
        for _ in 1..users {
            let joiner = rig.add_device();
            rig.admit(founder, joiner);
        }
        rig
    }
}
