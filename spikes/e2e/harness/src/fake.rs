//! `FakeCipher`: a cipher with no cryptography, to test the harness.
//!
//! **This is not encryption.** The ciphertext of a ping event is the plaintext
//! with the number of the key epoch in front, and a device that lacks that
//! epoch gets an error. Anyone can read it. It exists only so that the
//! scenarios, the simulator and the evil server can be tested without a
//! candidate, and its model of keys is the smallest that makes the scenarios
//! mean something:
//!
//! - the group has a current *epoch*; a removal makes the next one;
//! - an approved joiner gets the current epoch only;
//! - a new device of a known user gets every epoch its sibling holds;
//! - a ping is started under the current epoch and its events carry that epoch.
//!
//! The material of an event names the devices it is addressed to, and a device
//! learns an epoch from material only if it is named: that is how a removed
//! member is kept from the new epoch.
//!
//! ```
//! use e2e_harness::fake::FakeCipher;
//! use e2e_harness::cipher::{CipherError, GroupCipher};
//! use e2e_harness::ids::PingId;
//! use e2e_harness::keys::Identity;
//! use e2e_harness::rng::{Seed, SimRng};
//!
//! let mut rng = SimRng::new(Seed(1));
//! let mut cipher = FakeCipher::new(&Identity::generate(&mut rng), rng.fork());
//! // A device with no epoch cannot write or read a ping.
//! assert!(matches!(
//!     cipher.encrypt_ping(PingId(1), b"hi"),
//!     Err(CipherError::State(_))
//! ));
//! ```

use std::collections::{BTreeMap, BTreeSet};

use rand::RngCore;

use crate::{
    cipher::{CipherError, GroupCipher, GroupEvent},
    codec::{DecodeError, Reader, Writer},
    ids::{GroupId, InviteCode, Material, PingId, Seq, UserId},
    keys::{DeviceKey, Identity},
    payload::GroupChange,
    rng::SimRng,
    roster::{DeviceInfo, Roster},
};

/// A cipher with no cryptography. See the module docs.
#[derive(Debug)]
pub struct FakeCipher {
    me: DeviceKey,
    rng: SimRng,
    known: BTreeSet<u32>,
    current: Option<u32>,
    pings: BTreeMap<PingId, u32>,
}

fn invalid(error: DecodeError) -> CipherError {
    CipherError::Invalid(error.to_string())
}

impl FakeCipher {
    fn current(&self) -> Result<u32, CipherError> {
        self.current
            .ok_or_else(|| CipherError::State("the device has no group epoch".into()))
    }

    fn learn(&mut self, epochs: impl IntoIterator<Item = u32>) {
        for epoch in epochs {
            self.known.insert(epoch);
            self.current = self.current.max(Some(epoch));
        }
    }
}

impl GroupCipher for FakeCipher {
    const NAME: &'static str = "fake";

    fn new(identity: &Identity, rng: SimRng) -> Self {
        Self {
            me: identity.device_key(),
            rng,
            known: BTreeSet::new(),
            current: None,
            pings: BTreeMap::new(),
        }
    }

    fn export(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u64(self.current.map_or(u64::MAX, u64::from));
        w.u32(self.known.len() as u32);
        self.known.iter().for_each(|epoch| {
            w.u32(*epoch);
        });
        w.u32(self.pings.len() as u32);
        self.pings.iter().for_each(|(ping, epoch)| {
            w.u32(ping.0).u32(*epoch);
        });
        w.finish()
    }

    fn restore(identity: &Identity, rng: SimRng, state: &[u8]) -> Result<Self, CipherError> {
        let mut r = Reader::new(state);
        let read = |r: &mut Reader<'_>| -> Result<_, DecodeError> {
            let current = match r.u64()? {
                u64::MAX => None,
                epoch => Some(u32::try_from(epoch).map_err(|_| DecodeError::Invalid("epoch"))?),
            };
            let known = (0..r.u32()?).map(|_| r.u32()).collect::<Result<_, _>>()?;
            let pings = (0..r.u32()?)
                .map(|_| Ok((PingId(r.u32()?), r.u32()?)))
                .collect::<Result<_, DecodeError>>()?;
            Ok((current, known, pings))
        };
        let (current, known, pings) = read(&mut r).map_err(invalid)?;
        r.finish().map_err(invalid)?;
        Ok(Self {
            me: identity.device_key(),
            rng,
            known,
            current,
            pings,
        })
    }

    fn introduction(&mut self) -> Material {
        Material::from(self.rng.next_u64().to_be_bytes().to_vec())
    }

    fn create_group(&mut self, _group: GroupId) -> Result<Material, CipherError> {
        let mut w = Writer::new();
        w.u32(0);
        Ok(w.finish().into())
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
        let mut w = Writer::new();
        w.u32(1)
            .raw(joiner.key.as_bytes())
            .u32(1)
            .u32(self.current()?);
        Ok(w.finish().into())
    }

    fn add_device(
        &mut self,
        _roster: &Roster,
        device: &DeviceInfo,
    ) -> Result<Material, CipherError> {
        self.current()?;
        let mut w = Writer::new();
        w.u32(1)
            .raw(device.key.as_bytes())
            .u32(self.known.len() as u32);
        self.known.iter().for_each(|epoch| {
            w.u32(*epoch);
        });
        Ok(w.finish().into())
    }

    fn remove_member(&mut self, roster: &Roster, user: UserId) -> Result<Material, CipherError> {
        let epoch = self.current()? + 1;
        let stays: Vec<_> = roster
            .devices()
            .filter(|(owner, _)| *owner != user)
            .map(|(_, device)| device.key)
            .collect();
        let mut w = Writer::new();
        w.u32(stays.len() as u32);
        stays.iter().for_each(|key| {
            w.raw(key.as_bytes());
        });
        w.u32(1).u32(epoch);
        Ok(w.finish().into())
    }

    fn apply_group_event(&mut self, event: &GroupEvent<'_>) -> Result<(), CipherError> {
        // Every addressed event has the same layout: the devices it is for, then
        // the epochs it hands them. A `Create` has no addressees.
        let mut r = Reader::new(event.material.as_slice());
        let mut read = || -> Result<(Vec<DeviceKey>, Vec<u32>), DecodeError> {
            let addressees = (0..r.u32()?)
                .map(|_| r.array().map(DeviceKey::from_bytes))
                .collect::<Result<_, _>>()?;
            let epochs = (0..r.u32()?).map(|_| r.u32()).collect::<Result<_, _>>()?;
            Ok((addressees, epochs))
        };
        match event.change {
            GroupChange::Create { .. } => {
                let mut r = Reader::new(event.material.as_slice());
                let epoch = r.u32().map_err(invalid)?;
                self.learn([epoch]);
            }
            GroupChange::Approve { .. }
            | GroupChange::AddDevice { .. }
            | GroupChange::Remove { .. } => {
                let (addressees, epochs) = read().map_err(invalid)?;
                if addressees.contains(&self.me) {
                    self.learn(epochs);
                }
            }
            GroupChange::Invite { .. }
            | GroupChange::Revoke { .. }
            | GroupChange::JoinRequest { .. }
            | GroupChange::Leave => {}
        }
        Ok(())
    }

    fn start_ping(&mut self, _ping: PingId, _group_head: Seq) -> Result<Material, CipherError> {
        let mut w = Writer::new();
        w.u32(self.current()?);
        Ok(w.finish().into())
    }

    fn open_ping(
        &mut self,
        ping: PingId,
        _group_head: Seq,
        material: &Material,
    ) -> Result<(), CipherError> {
        let epoch = Reader::new(material.as_slice()).u32().map_err(invalid)?;
        if !self.known.contains(&epoch) {
            return Err(CipherError::NoKey(format!("epoch {epoch} of {ping}")));
        }
        self.pings.insert(ping, epoch);
        Ok(())
    }

    fn encrypt_ping(&mut self, ping: PingId, plaintext: &[u8]) -> Result<Vec<u8>, CipherError> {
        let epoch = *self
            .pings
            .get(&ping)
            .ok_or_else(|| CipherError::State(format!("{ping} is not open")))?;
        let mut w = Writer::new();
        w.u32(epoch).raw(plaintext);
        Ok(w.finish())
    }

    fn decrypt_ping(
        &mut self,
        ping: PingId,
        _seq: Seq,
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, CipherError> {
        let mut r = Reader::new(ciphertext);
        let epoch = r.u32().map_err(invalid)?;
        if !self.known.contains(&epoch) {
            return Err(CipherError::NoKey(format!("epoch {epoch} of {ping}")));
        }
        Ok(ciphertext[4..].to_vec())
    }
}
