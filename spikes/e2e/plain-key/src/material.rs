//! The messages of the plain group key: what goes in the `Material` of each
//! event of the group log and in the first event of a ping log.
//!
//! The harness carries the material and never reads it. Everything here is in
//! the clear except what is sealed; each message says what it is for, and every
//! device that applies the event checks the same things, so that every device
//! reaches the same verdict (D4 c of #61).
//!
//! # The messages
//!
//! | Event | Message | Sealed |
//! |---|---|---|
//! | `Create` | `Handover` for epoch 0, to the founder's own device | the key of epoch 0 |
//! | `Approve` | `Handover` for the current epoch, to the joiner | the current key |
//! | `AddDevice` | `Handover` for the current epoch, to the new device | every key the sibling device holds |
//! | `Remove` | `Rekey`: the next epoch, one wrap for each device that stays | the new key, once for each device |
//! | first event of a ping log | `PingBox` | the key of the ping, under the key of an epoch |
//! | other events of a ping log | the nonce and the ciphertext of XChaCha20-Poly1305 | the event, under the key of the ping |
//! | `Invite`, `Revoke`, `JoinRequest`, `Leave` | none | |
//!
//! ```text
//! Handover = epoch u32 | commitment [32] | enc [32] | ciphertext (to the end)
//! Rekey    = epoch u32 | commitment [32] | wrap [80] ...         (one for each device, in roster order)
//! wrap     = enc [32] | ciphertext [48]
//! PingBox  = epoch u32 | nonce [24] | ciphertext (to the end)
//! ```
//!
//! The wraps of a `Rekey` have no label: wrap *i* is for the *i*-th device of
//! the roster after the removal, in the order of
//! [`Roster::devices`](e2e_harness::roster::Roster::devices) (users by id,
//! devices of a user as they were vouched for). That order is a function of the
//! group log, so every device finds its own wrap, and a rekey that leaves a
//! device out or adds one that no user vouches for has the wrong number of
//! wraps and is refused by every device alike (question 8 of #62).
//!
//! # What every device checks
//!
//! - A message is well formed: the right widths, nothing missing, nothing left.
//! - The epoch it hands out is the epoch it must be: 0 for `Create`, the current
//!   one for `Approve` and `AddDevice` (the author wrote it from the head it
//!   had read, so it is the epoch at that position), the next one for `Remove`.
//! - The introduction of a device that the event vouches for is a public key.
//! - A `Rekey` covers exactly the devices that the roster holds after the
//!   removal.
//! - The device that is addressed opens its wrap and finds the key that the
//!   commitment names.
//!
//! What no device can check is that the wrap of *another* device opens. A member
//! who seals garbage for one device locks that device out, and the others cannot
//! tell: see the weak points in the comment `## Answers` of #68.
//!
//! # Domain separation
//!
//! Each use of a key has its own label (`Purpose`). An HPKE wrap is bound to
//! its purpose, its author, its recipient and its epoch through `info`, and to
//! its commitment through the associated data, so a wrap cannot be moved to
//! another device, another event kind or another epoch. The sealed key of a ping
//! is bound to the ping, to the group head it was made under and to the epoch.

use e2e_harness::{
    cipher::CipherError,
    ids::{Material, PingId, Seq},
    keys::DeviceKey,
};

use crate::{
    crypto::{Commitment, GroupKey, KEY_LEN, NONCE_LEN, Sealed, TAG_LEN},
    invalid,
    ring::{Epoch, KeyRing},
    wire::{Reader, WireError, Writer},
};

const DOMAIN: &[u8] = b"sens-o-matic/plain-key/v1/";

/// What a key is used for. A label is never shared between two uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Purpose {
    /// The founder's key of epoch 0, sealed to the founder.
    Create,
    /// The current key, sealed to a joiner.
    Approve,
    /// Every key of a device, sealed to its new sibling.
    AddDevice,
    /// The key of a new epoch, sealed to a device that stays.
    Rekey,
    /// The key of a ping, sealed under a key of the group.
    PingKey,
    /// An event of a ping, sealed under the key of the ping.
    PingEvent,
}

impl Purpose {
    fn label(self) -> &'static [u8] {
        match self {
            Self::Create => b"create",
            Self::Approve => b"approve",
            Self::AddDevice => b"add-device",
            Self::Rekey => b"rekey",
            Self::PingKey => b"ping-key",
            Self::PingEvent => b"ping-event",
        }
    }
}

/// The `info` of the HPKE wrap of `purpose` from `author` to `recipient`.
pub(crate) fn wrap_info(
    purpose: Purpose,
    author: &DeviceKey,
    recipient: &DeviceKey,
    epoch: Epoch,
) -> Vec<u8> {
    let mut w = Writer::new();
    w.raw(DOMAIN)
        .raw(purpose.label())
        .raw(author.as_bytes())
        .raw(recipient.as_bytes())
        .u32(epoch.get());
    w.finish()
}

/// The associated data of the key of a ping, sealed in its first event.
pub(crate) fn ping_key_aad(ping: PingId, group_head: Seq, epoch: Epoch) -> Vec<u8> {
    let mut w = Writer::new();
    w.raw(DOMAIN)
        .raw(Purpose::PingKey.label())
        .u32(ping.0)
        .u64(group_head.0)
        .u32(epoch.get());
    w.finish()
}

/// The associated data of an event of a ping.
pub(crate) fn ping_event_aad(ping: PingId) -> Vec<u8> {
    let mut w = Writer::new();
    w.raw(DOMAIN).raw(Purpose::PingEvent.label()).u32(ping.0);
    w.finish()
}

fn malformed(what: &'static str) -> impl FnOnce(WireError) -> CipherError {
    move |error| invalid(format!("the material of {what} is malformed: {error}"))
}

/// A message that hands keys to one device: the material of `Create`, `Approve`
/// and `AddDevice`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Handover {
    /// The newest epoch whose key the message hands out.
    pub(crate) epoch: Epoch,
    /// The commitment to the key of that epoch.
    pub(crate) commitment: Commitment,
    /// The keys, sealed to the device.
    pub(crate) sealed: Sealed,
}

impl Handover {
    pub(crate) fn encode(&self) -> Material {
        let mut w = Writer::new();
        w.u32(self.epoch.get())
            .raw(self.commitment.as_bytes())
            .raw(&self.sealed.enc)
            .raw(&self.sealed.ct);
        Material::from(w.finish())
    }

    pub(crate) fn decode(material: &Material) -> Result<Self, CipherError> {
        let mut r = Reader::new(material.as_slice());
        let read = |r: &mut Reader<'_>| -> Result<_, WireError> {
            Ok((
                Epoch::from_u32(r.u32()?),
                Commitment::from_bytes(r.array()?),
                r.array::<KEY_LEN>()?,
                r.rest().to_vec(),
            ))
        };
        let (epoch, commitment, enc, ct) = read(&mut r).map_err(malformed("a handover"))?;
        if ct.len() < TAG_LEN {
            return Err(invalid("the material of a handover is shorter than a tag"));
        }
        Ok(Self {
            epoch,
            commitment,
            sealed: Sealed { enc, ct },
        })
    }
}

/// The material of `Remove`: the key of the next epoch, sealed to each device
/// that stays.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Rekey {
    /// The epoch that the removal starts.
    pub(crate) epoch: Epoch,
    /// The commitment to its key.
    pub(crate) commitment: Commitment,
    /// One wrap for each device that stays, in roster order.
    pub(crate) wraps: Vec<Sealed>,
}

impl Rekey {
    pub(crate) fn encode(&self) -> Material {
        let mut w = Writer::new();
        w.u32(self.epoch.get()).raw(self.commitment.as_bytes());
        for wrap in &self.wraps {
            debug_assert_eq!(wrap.enc.len() + wrap.ct.len(), Sealed::KEY_WIDTH);
            w.raw(&wrap.enc).raw(&wrap.ct);
        }
        Material::from(w.finish())
    }

    pub(crate) fn decode(material: &Material) -> Result<Self, CipherError> {
        let mut r = Reader::new(material.as_slice());
        let read = |r: &mut Reader<'_>| -> Result<_, WireError> {
            Ok((
                Epoch::from_u32(r.u32()?),
                Commitment::from_bytes(r.array()?),
            ))
        };
        let (epoch, commitment) = read(&mut r).map_err(malformed("a rekey"))?;
        let rest = r.rest();
        let (chunks, left_over) = rest.as_chunks::<{ Sealed::KEY_WIDTH }>();
        if !left_over.is_empty() {
            return Err(invalid(format!(
                "the wraps of a rekey are {} bytes each, and {} bytes are not a whole number of them",
                Sealed::KEY_WIDTH,
                rest.len()
            )));
        }
        let wraps = chunks
            .iter()
            .map(|chunk| {
                let (enc, ct) = chunk
                    .split_first_chunk::<KEY_LEN>()
                    .expect("a wrap is wider than a key");
                Sealed {
                    enc: *enc,
                    ct: ct.to_vec(),
                }
            })
            .collect();
        Ok(Self {
            epoch,
            commitment,
            wraps,
        })
    }
}

/// The material of the first event of a ping log: the key of the ping, sealed
/// under the key of an epoch of the group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PingBox {
    /// The epoch whose key seals the key of the ping.
    pub(crate) epoch: Epoch,
    /// The nonce and the ciphertext.
    pub(crate) sealed: Vec<u8>,
}

impl PingBox {
    pub(crate) fn encode(&self) -> Material {
        let mut w = Writer::new();
        w.u32(self.epoch.get()).raw(&self.sealed);
        Material::from(w.finish())
    }

    pub(crate) fn decode(material: &Material) -> Result<Self, CipherError> {
        let mut r = Reader::new(material.as_slice());
        let epoch = Epoch::from_u32(r.u32().map_err(malformed("a ping"))?);
        let sealed = r.rest().to_vec();
        if sealed.len() != NONCE_LEN + KEY_LEN + TAG_LEN {
            return Err(invalid(format!(
                "the key of a ping is sealed in {} bytes, not {}",
                NONCE_LEN + KEY_LEN + TAG_LEN,
                sealed.len()
            )));
        }
        Ok(Self { epoch, sealed })
    }
}

/// The plaintext of an `AddDevice` handover: every key of the sibling, oldest
/// first, each as its epoch and its 32 bytes.
pub(crate) fn encode_bundle(ring: &KeyRing) -> Vec<u8> {
    let mut w = Writer::new();
    for (epoch, key) in ring.iter() {
        w.u32(epoch.get()).raw(key.as_bytes());
    }
    w.finish()
}

/// Read what [`encode_bundle`] wrote: at least one key, epochs in strictly
/// increasing order.
pub(crate) fn decode_bundle(bytes: &[u8]) -> Result<Vec<(Epoch, GroupKey)>, CipherError> {
    const ENTRY: usize = 4 + KEY_LEN;
    let (entries, left_over) = bytes.as_chunks::<ENTRY>();
    if entries.is_empty() || !left_over.is_empty() {
        return Err(invalid(format!(
            "a bundle of keys is a whole number of {ENTRY}-byte entries, at least one; got {} bytes",
            bytes.len()
        )));
    }
    let mut keys: Vec<(Epoch, GroupKey)> = Vec::with_capacity(entries.len());
    for entry in entries {
        let mut r = Reader::new(entry);
        let epoch = Epoch::from_u32(r.u32().map_err(malformed("a bundle"))?);
        let key = GroupKey::from_bytes(r.array().map_err(malformed("a bundle"))?);
        if keys.last().is_some_and(|(last, _)| *last >= epoch) {
            return Err(invalid(
                "the epochs of a bundle of keys are not in increasing order",
            ));
        }
        keys.push((epoch, key));
    }
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use e2e_harness::rng::{Seed, SimRng};

    use super::*;

    fn sealed(byte: u8, len: usize) -> Sealed {
        Sealed {
            enc: [byte; KEY_LEN],
            ct: vec![byte; len],
        }
    }

    #[test]
    fn a_handover_round_trips() {
        let handover = Handover {
            epoch: Epoch::from_u32(3),
            commitment: Commitment::from_bytes([1; KEY_LEN]),
            sealed: sealed(2, 48),
        };
        assert_eq!(Handover::decode(&handover.encode()).unwrap(), handover);
        assert!(Handover::decode(&Material::from(&[0u8; 40][..])).is_err());
    }

    #[test]
    fn a_rekey_round_trips_and_refuses_a_partial_wrap() {
        let rekey = Rekey {
            epoch: Epoch::from_u32(1),
            commitment: Commitment::from_bytes([1; KEY_LEN]),
            wraps: vec![sealed(1, 48), sealed(2, 48), sealed(3, 48)],
        };
        let material = rekey.encode();
        assert_eq!(material.len(), 4 + KEY_LEN + 3 * Sealed::KEY_WIDTH);
        assert_eq!(Rekey::decode(&material).unwrap(), rekey);

        let cut = Material::from(&material.as_slice()[..material.len() - 1]);
        assert!(Rekey::decode(&cut).is_err());
    }

    #[test]
    fn a_bundle_keeps_its_order_and_refuses_a_repeat() {
        let mut rng = SimRng::new(Seed(5));
        let mut ring = KeyRing::default();
        for number in [0, 2, 5] {
            ring.insert(Epoch::from_u32(number), GroupKey::generate(&mut rng));
        }
        let bundle = decode_bundle(&encode_bundle(&ring)).unwrap();
        let epochs: Vec<_> = bundle.iter().map(|(epoch, _)| epoch.get()).collect();
        assert_eq!(epochs, [0, 2, 5]);

        let mut twice = encode_bundle(&ring);
        twice.extend_from_slice(&encode_bundle(&ring));
        assert!(decode_bundle(&twice).is_err());
        assert!(decode_bundle(&[]).is_err());
    }

    #[test]
    fn two_uses_of_a_key_never_share_a_label() {
        let (a, b) = (
            DeviceKey::from_bytes([1; 32]),
            DeviceKey::from_bytes([2; 32]),
        );
        let epoch = Epoch::GENESIS;
        let infos: Vec<_> = [
            Purpose::Create,
            Purpose::Approve,
            Purpose::AddDevice,
            Purpose::Rekey,
        ]
        .into_iter()
        .map(|purpose| wrap_info(purpose, &a, &b, epoch))
        .collect();
        for (i, info) in infos.iter().enumerate() {
            assert!(infos.iter().skip(i + 1).all(|other| other != info));
        }
        assert_ne!(
            wrap_info(Purpose::Rekey, &a, &b, epoch),
            wrap_info(Purpose::Rekey, &b, &a, epoch)
        );
    }
}
