//! Questions 2 and 8 of #62: the join of D11 with a single-use code, and the rule
//! "a user is a set of device keys, added by an existing device" as a check that
//! every device makes alike on replay.
//!
//! The tests "a second use of the code fails" and "a revoked code fails" are the
//! scenarios `second_use_of_code_refused` and `revoked_code_refused` of the
//! harness: the rules refuse the request before any cipher sees it. What is left
//! for the cipher is a member who misuses the approval, which is tested here.

use std::cell::RefCell;

use e2e_harness::{
    cipher::{CipherError, GroupCipher, GroupEvent},
    device::{Ignored, Intent},
    ids::{GroupId, InviteCode, Material, PingId, Seq, UserId},
    keys::Identity,
    rng::{Seed, SimRng},
    roster::{DeviceInfo, Roster},
    sim::World,
};
use e2e_openmls::OpenMls;

thread_local! {
    /// The device that a greedy member adds instead of the one it approves.
    static SWAP: RefCell<Option<DeviceInfo>> = const { RefCell::new(None) };
}

/// A cipher whose `approve_join` adds another device than the joiner, when a test
/// says so: a member who abuses the approval. Everything else is `OpenMls`.
struct Greedy(OpenMls);

impl GroupCipher for Greedy {
    const NAME: &'static str = "openmls-greedy";

    fn new(identity: &Identity, rng: SimRng) -> Self {
        Self(OpenMls::new(identity, rng))
    }
    fn export(&self) -> Vec<u8> {
        self.0.export()
    }
    fn restore(identity: &Identity, rng: SimRng, state: &[u8]) -> Result<Self, CipherError> {
        OpenMls::restore(identity, rng, state).map(Self)
    }
    fn introduction(&mut self) -> Material {
        self.0.introduction()
    }
    fn create_group(&mut self, group: GroupId) -> Result<Material, CipherError> {
        self.0.create_group(group)
    }
    fn invite(&mut self, code: &InviteCode) -> Result<Material, CipherError> {
        self.0.invite(code)
    }
    fn revoke_code(&mut self, code: &InviteCode) -> Result<Material, CipherError> {
        self.0.revoke_code(code)
    }
    fn approve_join(
        &mut self,
        roster: &Roster,
        joiner: &DeviceInfo,
    ) -> Result<Material, CipherError> {
        let target = SWAP.with(|swap| swap.borrow().clone());
        self.0
            .approve_join(roster, target.as_ref().unwrap_or(joiner))
    }
    fn add_device(
        &mut self,
        roster: &Roster,
        device: &DeviceInfo,
    ) -> Result<Material, CipherError> {
        self.0.add_device(roster, device)
    }
    fn remove_member(&mut self, roster: &Roster, user: UserId) -> Result<Material, CipherError> {
        self.0.remove_member(roster, user)
    }
    fn apply_group_event(&mut self, event: &GroupEvent<'_>) -> Result<(), CipherError> {
        self.0.apply_group_event(event)
    }
    fn start_ping(&mut self, ping: PingId, group_head: Seq) -> Result<Material, CipherError> {
        self.0.start_ping(ping, group_head)
    }
    fn open_ping(
        &mut self,
        ping: PingId,
        group_head: Seq,
        material: &Material,
    ) -> Result<(), CipherError> {
        self.0.open_ping(ping, group_head, material)
    }
    fn encrypt_ping(&mut self, ping: PingId, plaintext: &[u8]) -> Result<Vec<u8>, CipherError> {
        self.0.encrypt_ping(ping, plaintext)
    }
    fn decrypt_ping(
        &mut self,
        ping: PingId,
        seq: Seq,
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, CipherError> {
        self.0.decrypt_ping(ping, seq, ciphertext)
    }
}

/// A member approves the request of one device and Adds another, a device that no
/// user vouches for. The rules accept the approval (the request is open and the
/// author is a member); the cipher of every device refuses the Commit because the
/// devices of the group after it are not the devices of the roster. So every
/// device, the greedy member's own included, ignores the event with one verdict,
/// the stranger is in no state, and the request stays open for an honest member.
#[test]
fn an_approval_that_adds_somebody_else_is_ignored_by_every_device_alike() {
    let mut world = World::<Greedy>::new(Seed(31));
    let founder = world.found_group();
    let member = world.admit(founder);
    let stranger = world.add_device();
    let stranger_info = world.introduction_of(stranger);

    let joiner = world.add_device();
    let joiner_key = world.device(joiner).key();
    let code = world.new_code();
    world.queue(founder, Intent::Invite(code));
    world.queue(joiner, Intent::Request(code));
    world.run_until(|w| w.device(founder).state().request_of(&joiner_key).is_some());
    let request = world
        .device(founder)
        .state()
        .request_of(&joiner_key)
        .expect("the request is open");

    SWAP.with(|swap| *swap.borrow_mut() = Some(stranger_info.clone()));
    world.queue(founder, Intent::Approve(request));
    world.run_until_idle();
    SWAP.with(|swap| *swap.borrow_mut() = None);

    for idx in [founder, member] {
        let device = world.device(idx);
        assert!(
            matches!(
                device.ignored(),
                [(_, Ignored::Cipher(CipherError::Invalid(_)))]
            ),
            "{idx} ignored the Commit with the same verdict: {:?}",
            device.ignored()
        );
        assert!(!device.state().roster().contains(&joiner_key));
        assert!(!device.state().roster().contains(&stranger_info.key));
        assert_eq!(
            device.state().request_of(&joiner_key),
            Some(request),
            "the request is still open"
        );
    }
    assert_eq!(
        world.device(founder).group_digest(),
        world.device(member).group_digest()
    );
    assert!(!world.device(stranger).is_member());

    // An honest member approves the same request, and the joiner is in.
    world.queue(member, Intent::Approve(request));
    world.run_until_idle();
    assert!(world.device(joiner).is_member());
    let ping = world.start_ping(founder);
    world.post(joiner, ping, b"thanks");
    world.run_until_idle();
    assert_eq!(
        world
            .device(founder)
            .ping(ping)
            .expect("read")
            .readable()
            .len(),
        1
    );
    assert_eq!(
        world
            .device(stranger)
            .ping(ping)
            .map_or(0, |view| view.readable().len()),
        0,
        "the stranger reads nothing"
    );
}
