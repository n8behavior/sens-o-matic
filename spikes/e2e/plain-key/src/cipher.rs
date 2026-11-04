//! `PlainKey`: the plain group key behind [`GroupCipher`].
//!
//! One symmetric key for the group, which every device of the group holds, and a
//! new key after a member leaves or is removed (SPEC section 9). The design is in
//! the comment "Evaluation" of #62, candidate "Plain group key"; the messages are
//! in [`crate::material`], the primitives in [`crate::crypto`].
//!
//! # Keys
//!
//! - A **group key** for each *epoch*. Epoch 0 starts at the `Create`; each
//!   `Remove` starts the next one, with a new random key sealed with HPKE to each
//!   device that stays. A removed device holds the keys up to the removal and
//!   never gets the next.
//! - A **ping key** for each ping, made by the starter and sealed under the group
//!   key of the current epoch in the first event of the ping log. It lives in the
//!   ping log, so any device that holds that epoch can open the ping from the
//!   start of its log, in any order. The events of the ping are sealed under it.
//! - A **device key pair** (X25519) for HPKE. The public half is the
//!   introduction that goes into `Create`, `JoinRequest` and `AddDevice`.
//!
//! # Who gets what
//!
//! | Event | Who learns a key |
//! |---|---|
//! | `Create` | the founder: epoch 0 |
//! | `Approve` | the joiner: the current epoch only (none of the past) |
//! | `AddDevice` | the new device: every epoch its sibling holds |
//! | `Remove` | every device that stays: the next epoch |
//!
//! A joiner can therefore read the pings of the current epoch from their start
//! and no ping of an earlier epoch. A new device of a known user reads what its
//! user could read.
//!
//! # A ping ends with a removal (D10)
//!
//! A cipher that has applied a `Remove` after a ping began refuses to
//! [`encrypt_ping`](GroupCipher::encrypt_ping) for it, because the removed user
//! holds the key of that ping. An author that has not read the removal yet can
//! still write with the old key, and the removed user reads that event: the
//! window is the time between the removal and the moment the author reads it, and
//! D10 accepts it.
//!
//! # What this does not give
//!
//! No forward secrecy and no post-compromise security inside an epoch: a device
//! that is stolen gives up every key it ever held, and the wraps for its public
//! key are in the log for ever. See the weak points in the comment `## Answers`
//! of #68.

use std::{collections::BTreeMap, fmt};

use e2e_harness::{
    cipher::{CipherError, GroupCipher, GroupEvent},
    ids::{GroupId, InviteCode, Material, PingId, Seq, UserId},
    keys::{DeviceKey, Identity},
    payload::GroupChange,
    rng::SimRng,
    roster::{DeviceInfo, Roster},
};

use crate::{
    crypto::{
        Commitment, DevicePublic, DeviceSecret, GroupKey, KEY_LEN, PingKey, Sealed, open_sym,
        open_with, seal_sym, seal_to,
    },
    invalid,
    material::{
        Handover, PingBox, Purpose, Rekey, decode_bundle, encode_bundle, ping_event_aad,
        ping_key_aad, wrap_info,
    },
    ring::{Epoch, KeyRing},
    wire::{Reader, WireError, Writer},
};

/// The version of the exported state.
const STATE_VERSION: u8 = 1;

/// A ping that this device can read and write.
#[derive(Debug, Clone)]
struct OpenPing {
    /// The epoch whose key sealed the key of the ping.
    epoch: Epoch,
    key: PingKey,
}

/// The cipher of one device. See the module docs.
pub struct PlainKey {
    me: DeviceKey,
    rng: SimRng,
    secret: DeviceSecret,
    public: DevicePublic,
    keys: KeyRing,
    pings: BTreeMap<PingId, OpenPing>,
}

impl fmt::Debug for PlainKey {
    /// The device and the epochs it holds: never a key.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PlainKey")
            .field("device", &self.me)
            .field(
                "epochs",
                &self
                    .keys
                    .iter()
                    .map(|(epoch, _)| epoch.get())
                    .collect::<Vec<_>>(),
            )
            .field("pings", &self.pings.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

fn state(what: impl fmt::Display) -> CipherError {
    CipherError::State(format!(
        "{what} (see the module docs of e2e_plain_key::cipher)"
    ))
}

fn no_material(change: &str, material: &Material) -> Result<(), CipherError> {
    if material.is_empty() {
        Ok(())
    } else {
        Err(invalid(format!(
            "{change} carries no material, and this one carries {} bytes",
            material.len()
        )))
    }
}

impl PlainKey {
    /// Seal `plaintext` to `to` as a message of `purpose` for `epoch`.
    fn hand_over(
        &mut self,
        purpose: Purpose,
        to: &DeviceInfo,
        epoch: Epoch,
        commitment: Commitment,
        plaintext: &[u8],
    ) -> Result<Handover, CipherError> {
        let public = DevicePublic::from_introduction(&to.introduction)?;
        let info = wrap_info(purpose, &self.me, &to.key, epoch);
        let sealed = seal_to(
            &public,
            &info,
            commitment.as_bytes(),
            plaintext,
            &mut self.rng,
        )?;
        Ok(Handover {
            epoch,
            commitment,
            sealed,
        })
    }

    /// Open a wrap of one key that `author` made for this device, and check it
    /// against the commitment.
    fn open_key(
        &self,
        purpose: Purpose,
        author: &DeviceKey,
        epoch: Epoch,
        commitment: Commitment,
        sealed: &Sealed,
    ) -> Result<GroupKey, CipherError> {
        let info = wrap_info(purpose, author, &self.me, epoch);
        let plaintext = open_with(&self.secret, &info, commitment.as_bytes(), sealed)?;
        let bytes: [u8; KEY_LEN] = plaintext
            .try_into()
            .map_err(|_| invalid("a wrap holds one key of 32 bytes"))?;
        let key = GroupKey::from_bytes(bytes);
        if Commitment::of(&key) != commitment {
            return Err(invalid(format!(
                "the key in the wrap for {epoch} is not the key that its commitment names: the author sealed different keys for different devices"
            )));
        }
        Ok(key)
    }

    /// Check that a message hands out the epoch the group is at.
    fn expect_current(&self, epoch: Epoch) -> Result<(), CipherError> {
        match self.keys.newest() {
            Some((current, _)) if current == epoch => Ok(()),
            Some((current, _)) => Err(invalid(format!(
                "the message hands out {epoch} and the group is at {current}"
            ))),
            None => Err(state("this device holds no key, so it is in no group")),
        }
    }

    fn apply_create(
        &mut self,
        event: &GroupEvent<'_>,
        introduction: &Material,
    ) -> Result<(), CipherError> {
        if *event.author != self.me {
            return Err(invalid(
                "only the founder applies a Create: nobody else is in the group yet",
            ));
        }
        if *introduction != self.public.to_introduction() {
            return Err(invalid(
                "the Create names another public key than this device's",
            ));
        }
        if !self.keys.is_empty() {
            return Err(state(
                "this device holds keys already, so it cannot apply a Create",
            ));
        }
        let handover = Handover::decode(event.material)?;
        if handover.epoch != Epoch::GENESIS {
            return Err(invalid(format!(
                "a Create hands out epoch 0, not {}",
                handover.epoch
            )));
        }
        let key = self.open_key(
            Purpose::Create,
            event.author,
            handover.epoch,
            handover.commitment,
            &handover.sealed,
        )?;
        self.keys.insert(handover.epoch, key);
        Ok(())
    }

    fn apply_approve(&mut self, event: &GroupEvent<'_>) -> Result<(), CipherError> {
        let joiner = admitted_device(event)?;
        DevicePublic::from_introduction(&joiner.introduction)?;
        let handover = Handover::decode(event.material)?;
        if joiner.key != self.me {
            return self.expect_current(handover.epoch);
        }
        // The joiner's first key. A device that was removed and joins again by a
        // new invite already holds the keys up to its removal: the new one is later.
        if let Some((newest, _)) = self.keys.newest()
            && handover.epoch <= newest
        {
            return Err(invalid(format!(
                "an Approve hands out {} to a device that holds {newest} already",
                handover.epoch
            )));
        }
        let key = self.open_key(
            Purpose::Approve,
            event.author,
            handover.epoch,
            handover.commitment,
            &handover.sealed,
        )?;
        self.keys.insert(handover.epoch, key);
        Ok(())
    }

    fn apply_add_device(
        &mut self,
        event: &GroupEvent<'_>,
        device: &DeviceKey,
        introduction: &Material,
    ) -> Result<(), CipherError> {
        DevicePublic::from_introduction(introduction)?;
        let handover = Handover::decode(event.material)?;
        if *device != self.me {
            return self.expect_current(handover.epoch);
        }
        let info = wrap_info(Purpose::AddDevice, event.author, &self.me, handover.epoch);
        let bundle = decode_bundle(&open_with(
            &self.secret,
            &info,
            handover.commitment.as_bytes(),
            &handover.sealed,
        )?)?;
        let (last, newest) = bundle.last().expect("a bundle holds at least one key");
        if *last != handover.epoch || Commitment::of(newest) != handover.commitment {
            return Err(invalid(format!(
                "the newest key of the bundle is not the {} that the message names",
                handover.epoch
            )));
        }
        for (epoch, key) in &bundle {
            if self
                .keys
                .get(*epoch)
                .is_some_and(|held| held.as_bytes() != key.as_bytes())
            {
                return Err(invalid(format!(
                    "the bundle holds another key for {epoch} than this device holds"
                )));
            }
        }
        for (epoch, key) in bundle {
            self.keys.insert(epoch, key);
        }
        Ok(())
    }

    fn apply_remove(&mut self, event: &GroupEvent<'_>) -> Result<(), CipherError> {
        let rekey = Rekey::decode(event.material)?;
        let (current, _) = self
            .keys
            .newest()
            .ok_or_else(|| state("this device holds no key, so it is in no group"))?;
        if rekey.epoch != current.next() {
            return Err(invalid(format!(
                "a Remove starts {} and the group is at {current}",
                rekey.epoch
            )));
        }
        let stays = event.roster.device_count();
        if rekey.wraps.len() != stays {
            return Err(invalid(format!(
                "the Remove has {} wraps and {stays} devices stay: it leaves a device out or wraps for one that no user vouches for",
                rekey.wraps.len()
            )));
        }
        let index = event
            .roster
            .devices()
            .position(|(_, device)| device.key == self.me)
            .ok_or_else(|| state("a Remove is applied only by a device that stays"))?;
        let key = self.open_key(
            Purpose::Rekey,
            event.author,
            rekey.epoch,
            rekey.commitment,
            &rekey.wraps[index],
        )?;
        self.keys.insert(rekey.epoch, key);
        Ok(())
    }
}

/// The device that an `Approve` admitted: the only device of the user whose id is
/// the position of the event.
fn admitted_device<'a>(event: &GroupEvent<'a>) -> Result<&'a DeviceInfo, CipherError> {
    let roster: &'a Roster = event.roster;
    let user: UserId = roster
        .users()
        .find(|user| user.admitted() == event.seq)
        .ok_or_else(|| invalid("an Approve admits a user and the roster holds none for it"))?;
    let mut devices = roster.devices_of(user);
    match (devices.next(), devices.next()) {
        (Some(device), None) => Ok(device),
        _ => Err(invalid("an Approve admits one device")),
    }
}

impl GroupCipher for PlainKey {
    const NAME: &'static str = "plain-key";

    fn new(identity: &Identity, mut rng: SimRng) -> Self {
        let (secret, public) = DeviceSecret::generate(&mut rng);
        Self {
            me: identity.device_key(),
            rng,
            secret,
            public,
            keys: KeyRing::default(),
            pings: BTreeMap::new(),
        }
    }

    /// `version u8 | private key [32] | keys: count u32, (epoch u32, key [32])... |
    /// pings: count u32, (ping u32, epoch u32, key [32])...`
    fn export(&self) -> Vec<u8> {
        let count = |len: usize| u32::try_from(len).expect("a device holds fewer than 2^32 keys");
        let mut w = Writer::new();
        w.u8(STATE_VERSION).raw(&self.secret.to_bytes());
        w.u32(count(self.keys.iter().count()));
        for (epoch, key) in self.keys.iter() {
            w.u32(epoch.get()).raw(key.as_bytes());
        }
        w.u32(count(self.pings.len()));
        for (ping, open) in &self.pings {
            w.u32(ping.0).u32(open.epoch.get()).raw(open.key.as_bytes());
        }
        w.finish()
    }

    fn restore(identity: &Identity, rng: SimRng, state_bytes: &[u8]) -> Result<Self, CipherError> {
        let malformed =
            |error: WireError| invalid(format!("the exported state is malformed: {error}"));
        let mut r = Reader::new(state_bytes);
        match r.u8().map_err(malformed)? {
            STATE_VERSION => {}
            other => {
                return Err(invalid(format!(
                    "the exported state has version {other} and this build reads version {STATE_VERSION}"
                )));
            }
        }
        let secret = DeviceSecret::from_bytes(&r.array().map_err(malformed)?)?;
        let mut keys = KeyRing::default();
        for _ in 0..r.u32().map_err(malformed)? {
            let epoch = Epoch::from_u32(r.u32().map_err(malformed)?);
            keys.insert(epoch, GroupKey::from_bytes(r.array().map_err(malformed)?));
        }
        let mut pings = BTreeMap::new();
        for _ in 0..r.u32().map_err(malformed)? {
            let ping = PingId(r.u32().map_err(malformed)?);
            let epoch = Epoch::from_u32(r.u32().map_err(malformed)?);
            let key = PingKey::from_bytes(r.array().map_err(malformed)?);
            if keys.get(epoch).is_none() {
                return Err(invalid(format!(
                    "the exported state opens {ping} under {epoch}, which it holds no key for"
                )));
            }
            pings.insert(ping, OpenPing { epoch, key });
        }
        r.finish().map_err(malformed)?;
        Ok(Self {
            me: identity.device_key(),
            rng,
            public: secret.public(),
            secret,
            keys,
            pings,
        })
    }

    fn introduction(&mut self) -> Material {
        self.public.to_introduction()
    }

    fn create_group(&mut self, _group: GroupId) -> Result<Material, CipherError> {
        if !self.keys.is_empty() {
            return Err(state("this device is in a group already"));
        }
        let key = GroupKey::generate(&mut self.rng);
        let me = DeviceInfo {
            key: self.me,
            introduction: self.public.to_introduction(),
        };
        let handover = self.hand_over(
            Purpose::Create,
            &me,
            Epoch::GENESIS,
            Commitment::of(&key),
            key.as_bytes(),
        )?;
        Ok(handover.encode())
    }

    fn invite(&mut self, _code: &InviteCode) -> Result<Material, CipherError> {
        Ok(Material::empty())
    }

    fn revoke_code(&mut self, _code: &InviteCode) -> Result<Material, CipherError> {
        Ok(Material::empty())
    }

    fn approve_join(
        &mut self,
        _roster: &Roster,
        joiner: &DeviceInfo,
    ) -> Result<Material, CipherError> {
        let (epoch, key) = self
            .keys
            .newest()
            .map(|(epoch, key)| (epoch, key.clone()))
            .ok_or_else(|| state("this device holds no key, so it cannot approve a join"))?;
        let handover = self.hand_over(
            Purpose::Approve,
            joiner,
            epoch,
            Commitment::of(&key),
            key.as_bytes(),
        )?;
        Ok(handover.encode())
    }

    fn add_device(
        &mut self,
        _roster: &Roster,
        device: &DeviceInfo,
    ) -> Result<Material, CipherError> {
        let (epoch, commitment) = self
            .keys
            .newest()
            .map(|(epoch, key)| (epoch, Commitment::of(key)))
            .ok_or_else(|| state("this device holds no key, so it cannot add a device"))?;
        let bundle = encode_bundle(&self.keys);
        let handover = self.hand_over(Purpose::AddDevice, device, epoch, commitment, &bundle)?;
        Ok(handover.encode())
    }

    fn remove_member(&mut self, roster: &Roster, user: UserId) -> Result<Material, CipherError> {
        let (current, _) = self
            .keys
            .newest()
            .ok_or_else(|| state("this device holds no key, so it cannot remove a member"))?;
        let epoch = current.next();
        let key = GroupKey::generate(&mut self.rng);
        let commitment = Commitment::of(&key);
        let wraps = roster
            .devices()
            .filter(|(owner, _)| *owner != user)
            .map(|(_, device)| {
                self.hand_over(Purpose::Rekey, device, epoch, commitment, key.as_bytes())
                    .map(|handover| handover.sealed)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Rekey {
            epoch,
            commitment,
            wraps,
        }
        .encode())
    }

    fn apply_group_event(&mut self, event: &GroupEvent<'_>) -> Result<(), CipherError> {
        match event.change {
            GroupChange::Create { introduction } => self.apply_create(event, introduction),
            GroupChange::Approve { .. } => self.apply_approve(event),
            GroupChange::AddDevice {
                device,
                introduction,
            } => self.apply_add_device(event, device, introduction),
            GroupChange::Remove { .. } => self.apply_remove(event),
            GroupChange::JoinRequest { introduction, .. } => {
                DevicePublic::from_introduction(introduction)?;
                no_material("a JoinRequest", event.material)
            }
            GroupChange::Invite { .. } => no_material("an Invite", event.material),
            GroupChange::Revoke { .. } => no_material("a Revoke", event.material),
            GroupChange::Leave => no_material("a Leave", event.material),
        }
    }

    fn start_ping(&mut self, ping: PingId, group_head: Seq) -> Result<Material, CipherError> {
        let Self { keys, rng, .. } = self;
        let (epoch, group_key) = keys
            .newest()
            .ok_or_else(|| state("this device holds no key, so it cannot start a ping"))?;
        let key = PingKey::generate(rng);
        let sealed = seal_sym(
            group_key.as_bytes(),
            &ping_key_aad(ping, group_head, epoch),
            key.as_bytes(),
            rng,
        );
        Ok(PingBox { epoch, sealed }.encode())
    }

    fn open_ping(
        &mut self,
        ping: PingId,
        group_head: Seq,
        material: &Material,
    ) -> Result<(), CipherError> {
        let ping_box = PingBox::decode(material)?;
        let group_key = self.keys.get(ping_box.epoch).ok_or_else(|| {
            CipherError::NoKey(format!(
                "{ping} is sealed under {}, which this device holds no key for",
                ping_box.epoch
            ))
        })?;
        let plaintext = open_sym(
            group_key.as_bytes(),
            &ping_key_aad(ping, group_head, ping_box.epoch),
            &ping_box.sealed,
        )?;
        let key = PingKey::from_bytes(
            plaintext
                .try_into()
                .map_err(|_| invalid("the key of a ping is 32 bytes"))?,
        );
        self.pings.insert(
            ping,
            OpenPing {
                epoch: ping_box.epoch,
                key,
            },
        );
        Ok(())
    }

    fn encrypt_ping(&mut self, ping: PingId, plaintext: &[u8]) -> Result<Vec<u8>, CipherError> {
        let open = self
            .pings
            .get(&ping)
            .ok_or_else(|| state(format!("{ping} is not open")))?;
        if let Some((current, _)) = self.keys.newest()
            && open.epoch < current
        {
            return Err(state(format!(
                "{ping} began in {} and the group is at {current}: a removal ended it (D10), and the removed user holds its key; start a new ping",
                open.epoch
            )));
        }
        Ok(seal_sym(
            open.key.as_bytes(),
            &ping_event_aad(ping),
            plaintext,
            &mut self.rng,
        ))
    }

    fn decrypt_ping(
        &mut self,
        ping: PingId,
        _seq: Seq,
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, CipherError> {
        let open = self
            .pings
            .get(&ping)
            .ok_or_else(|| CipherError::NoKey(format!("{ping} is not open on this device")))?;
        open_sym(open.key.as_bytes(), &ping_event_aad(ping), ciphertext)
    }
}

impl PlainKey {
    /// The epochs whose keys this device holds, oldest first.
    pub fn epochs(&self) -> Vec<u32> {
        self.keys.iter().map(|(epoch, _)| epoch.get()).collect()
    }

    /// The epoch of the newest key, or `None` when the device is in no group.
    pub fn current_epoch(&self) -> Option<u32> {
        self.keys.newest().map(|(epoch, _)| epoch.get())
    }
}
