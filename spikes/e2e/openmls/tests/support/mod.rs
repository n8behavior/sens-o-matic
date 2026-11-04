//! A group without the simulator: the rules of the harness and the ciphers of the
//! devices, driven by hand. The tests that need a number for one operation, or a
//! cipher that is not in a world, use it. A device that is not `live` is a
//! placeholder: it has a key package and a place in the tree, and applies
//! nothing, which keeps a group of 200 devices cheap to build.

#![allow(dead_code)]

use e2e_harness::{
    cipher::{CipherError, GroupCipher, GroupEvent},
    ids::{GroupId, InviteCode, Material, PingId, Seq, UserId},
    keys::{DeviceKey, Identity},
    payload::GroupChange,
    rng::{Seed, SimRng},
    roster::DeviceInfo,
    rules::{GroupState, Refusal},
};
use e2e_openmls::OpenMls;

pub struct Device {
    pub identity: Identity,
    pub key: DeviceKey,
    pub cipher: OpenMls,
    pub live: bool,
}

/// What each live device said of an event: its index and its verdict.
pub type Verdicts = Vec<(usize, Result<(), CipherError>)>;

pub struct Group {
    pub state: GroupState,
    pub devices: Vec<Device>,
    pub next: u64,
    pub rng: SimRng,
    /// Devices whose refusal of an event is expected; it is recorded in `refused`.
    pub tolerate: Vec<usize>,
    pub refused: Vec<(usize, CipherError)>,
}

impl Group {
    /// A group of one device, the founder, which is device 0.
    pub fn new(seed: u64) -> Self {
        let mut group = Self {
            state: GroupState::default(),
            devices: Vec::new(),
            next: 0,
            rng: SimRng::new(Seed(seed)),
            tolerate: Vec::new(),
            refused: Vec::new(),
        };
        let founder = group.spawn(true);
        let introduction = group.devices[founder].cipher.introduction();
        let material = group.devices[founder]
            .cipher
            .create_group(GroupId(1))
            .expect("the founder makes the group");
        group
            .push(founder, GroupChange::Create { introduction }, material)
            .expect("the first event is accepted");
        group
    }

    /// A device that is not in the group yet.
    pub fn spawn(&mut self, live: bool) -> usize {
        let identity = Identity::generate(&mut self.rng.fork());
        let cipher = OpenMls::new(&identity, self.rng.fork());
        self.devices.push(Device {
            key: identity.device_key(),
            identity,
            cipher,
            live,
        });
        self.devices.len() - 1
    }

    pub fn head(&self) -> Seq {
        Seq(self.next)
    }

    /// Run the rules on an event, and give it to every live device that is a
    /// member after it. Returns the verdict of each device that was asked.
    pub fn push(
        &mut self,
        author: usize,
        change: GroupChange,
        material: Material,
    ) -> Result<Verdicts, Refusal> {
        let seq = Seq(self.next);
        let key = self.devices[author].key;
        self.state.apply(seq, &key, &change)?;
        self.next += 1;
        let mut verdicts = Vec::new();
        for (idx, device) in self.devices.iter_mut().enumerate() {
            if device.live && self.state.roster().contains(&device.key) {
                let event = GroupEvent {
                    seq,
                    author: &key,
                    change: &change,
                    material: &material,
                    roster: self.state.roster(),
                };
                verdicts.push((idx, device.cipher.apply_group_event(&event)));
            }
        }
        Ok(verdicts)
    }

    /// Like `push`, and every verdict must be an acceptance.
    pub fn push_ok(&mut self, author: usize, change: GroupChange, material: Material) {
        let verdicts = self
            .push(author, change, material)
            .expect("the rules accept");
        for (idx, verdict) in verdicts {
            match verdict {
                Err(error) if self.tolerate.contains(&idx) => self.refused.push((idx, error)),
                verdict => verdict
                    .unwrap_or_else(|error| panic!("device {idx} refused the event: {error}")),
            }
        }
    }

    /// A new user joins by the flow of D11. Returns the new device.
    pub fn join(&mut self, approver: usize, live: bool) -> usize {
        let joiner = self.spawn(live);
        let code = InviteCode::generate(&mut self.rng);
        let invite = self.devices[approver].cipher.invite(&code).expect("invite");
        self.push_ok(approver, GroupChange::Invite { code }, invite);
        let introduction = self.devices[joiner].cipher.introduction();
        self.push_ok(
            joiner,
            GroupChange::JoinRequest {
                code,
                introduction: introduction.clone(),
            },
            Material::empty(),
        );
        let request = Seq(self.next - 1);
        let info = DeviceInfo {
            key: self.devices[joiner].key,
            introduction,
        };
        let roster = self.state.roster().clone();
        let material = self.devices[approver]
            .cipher
            .approve_join(&roster, &info)
            .expect("approve");
        self.push_ok(approver, GroupChange::Approve { request }, material);
        joiner
    }

    /// A new device of the user of `owner`.
    pub fn add_device(&mut self, owner: usize, live: bool) -> usize {
        let device = self.spawn(live);
        let info = DeviceInfo {
            key: self.devices[device].key,
            introduction: self.devices[device].cipher.introduction(),
        };
        let roster = self.state.roster().clone();
        let material = self.devices[owner]
            .cipher
            .add_device(&roster, &info)
            .expect("add device");
        self.push_ok(
            owner,
            GroupChange::AddDevice {
                device: info.key,
                introduction: info.introduction,
            },
            material,
        );
        device
    }

    pub fn user_of(&self, device: usize) -> UserId {
        self.state
            .roster()
            .user_of(&self.devices[device].key)
            .expect("the device is in the group")
    }

    /// `by` removes the user of `device`.
    pub fn remove(&mut self, by: usize, device: usize) {
        let user = self.user_of(device);
        let roster = self.state.roster().clone();
        let material = self.devices[by]
            .cipher
            .remove_member(&roster, user)
            .expect("remove");
        self.push_ok(by, GroupChange::Remove { user }, material);
    }

    /// `by` starts a ping; every live member opens it.
    pub fn start_ping(&mut self, by: usize, ping: PingId) -> Material {
        let head = self.head();
        let material = self.devices[by]
            .cipher
            .start_ping(ping, head)
            .expect("start");
        for device in &mut self.devices {
            if device.live && self.state.roster().contains(&device.key) {
                let _ = device.cipher.open_ping(ping, head, &material);
            }
        }
        material
    }
}
