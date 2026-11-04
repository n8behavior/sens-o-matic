//! The envelope of an event (D3 and D4 of #61).
//!
//! Every event of every log has the same envelope. The author makes a
//! [`Draft`]: the signed bytes and the signature. The server stamps the draft
//! with the time of its own clock and stores the [`Envelope`]. The stored bytes
//! are the unit of the log (one object for each event, D7), and their SHA-256
//! is the [`EventHash`] that the next event names.
//!
//! ```text
//! stored  = version u8 | signed (u32 length + bytes) | signature [64] | timestamp u64
//! signed  = version u8 | log | seq u64 | prev [32] | author [32] | payload (u32 length + bytes)
//! ```
//!
//! # What the signature covers
//!
//! The author signs `signed`, with a fixed prefix that names this use of the
//! key. The signature covers the log, the position, the hash of the event
//! before, the author and the payload. It cannot cover the timestamp: the
//! server writes that, and the author does not know it. So the timestamp is the
//! server's claim, as D4 says ("inside the window uses the timestamp that the
//! server writes in the envelope"), and the hash chain covers it, because the
//! hash is taken over the stored bytes.
//!
//! The signature covers the position and `prev` so that the server cannot move
//! an event to another place of the log, or to another log, or splice it after
//! another event, without breaking the signature.
//!
//! # Bytes that are never encoded again
//!
//! An [`Envelope`] keeps the exact bytes that were signed and checks the
//! signature over those bytes, never over a fresh encoding of the parsed
//! fields. An old build that drops a field it does not know therefore still
//! checks the signature of a new event. The rule comes from "Rules for the
//! format of an event" in #61; this crate has one format version, `1`.
//!
//! ```
//! use e2e_harness::clock::Millis;
//! use e2e_harness::event::{Draft, Envelope};
//! use e2e_harness::ids::{GroupId, LogId, Seq};
//! use e2e_harness::keys::{EventHash, Identity};
//! use e2e_harness::rng::{Seed, SimRng};
//!
//! let alice = Identity::generate(&mut SimRng::new(Seed(1)));
//! let log = LogId::Group(GroupId(1));
//!
//! // The author signs; the server stamps; a reader checks.
//! let draft = Draft::sign(log, Seq::FIRST, EventHash::GENESIS, b"payload".to_vec(), &alice);
//! let stored = Envelope::stamp(draft, Millis(42)).unwrap();
//! assert!(stored.verify().is_ok());
//! assert_eq!(stored.body().author, alice.device_key());
//!
//! // The bytes of a log entry read back to the same envelope and the same hash.
//! let again = Envelope::from_bytes(&stored.to_bytes()).unwrap();
//! assert_eq!(again.hash(), stored.hash());
//! assert_eq!(again.timestamp(), Millis(42));
//! ```

use thiserror::Error;

use crate::{
    clock::Millis,
    codec::{DecodeError, Reader, Writer},
    ids::{LogId, Seq},
    keys::{DeviceKey, EventHash, Identity, Signature},
};

/// The format version of this build, the first byte of the stored event and of
/// the signed bytes.
pub const FORMAT_VERSION: u8 = 1;

/// The prefix of the signed bytes that names this use of the key.
const SIGNING_DOMAIN: &[u8] = b"e2e-harness/event/v1\0";

/// The signature of an event did not match its signed bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error(
    "the signature does not match the signed bytes of the event (see the module docs of e2e_harness::event)"
)]
pub struct BadSignature;

/// What the author signs: where the event goes, what it follows and what it says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Body {
    /// The log the event belongs to.
    pub log: LogId,
    /// The position the author expects for it: the head the author has read.
    pub seq: Seq,
    /// The hash of the event at `seq - 1`, or [`EventHash::GENESIS`] at the first.
    pub prev: EventHash,
    /// The device that wrote it.
    pub author: DeviceKey,
    /// The bytes of the event: a ciphertext, or a group event of [`crate::payload`].
    pub payload: Vec<u8>,
}

impl Body {
    fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u8(FORMAT_VERSION);
        self.log.encode(&mut w);
        w.u64(self.seq.0)
            .raw(self.prev.as_bytes())
            .raw(self.author.as_bytes())
            .bytes(&self.payload);
        w.finish()
    }

    fn decode(signed: &[u8]) -> Result<Self, DecodeError> {
        let mut r = Reader::new(signed);
        match r.u8()? {
            FORMAT_VERSION => {}
            other => return Err(DecodeError::UnknownVersion(other)),
        }
        let body = Self {
            log: LogId::decode(&mut r)?,
            seq: Seq(r.u64()?),
            prev: EventHash::from_bytes(r.array()?),
            author: DeviceKey::from_bytes(r.array()?),
            payload: r.bytes()?,
        };
        r.finish()?;
        Ok(body)
    }
}

fn signing_input(signed: &[u8]) -> Vec<u8> {
    [SIGNING_DOMAIN, signed].concat()
}

/// An event as the author sends it to the server: signed, not yet stamped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    signed: Vec<u8>,
    signature: Signature,
}

impl Draft {
    /// Sign an event of `identity`. The author of the body is the key that
    /// signs, so a draft cannot name another author.
    pub fn sign(
        log: LogId,
        seq: Seq,
        prev: EventHash,
        payload: Vec<u8>,
        identity: &Identity,
    ) -> Self {
        let body = Body {
            log,
            seq,
            prev,
            author: identity.device_key(),
            payload,
        };
        let signed = body.encode();
        let signature = identity.sign(&signing_input(&signed));
        Self { signed, signature }
    }

    /// The body, read from the signed bytes.
    pub fn body(&self) -> Result<Body, DecodeError> {
        Body::decode(&self.signed)
    }
}

/// An event as the server stores it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    body: Body,
    signed: Vec<u8>,
    signature: Signature,
    timestamp: Millis,
}

impl Envelope {
    /// Stamp a draft with `timestamp`, the time of the server's clock.
    pub fn stamp(draft: Draft, timestamp: Millis) -> Result<Self, DecodeError> {
        Ok(Self {
            body: draft.body()?,
            signed: draft.signed,
            signature: draft.signature,
            timestamp,
        })
    }

    /// Read a stored event. The bytes must be exactly one canonical event.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DecodeError> {
        let mut r = Reader::new(bytes);
        match r.u8()? {
            FORMAT_VERSION => {}
            other => return Err(DecodeError::UnknownVersion(other)),
        }
        let signed = r.bytes()?;
        let signature = Signature::from_bytes(r.array()?);
        let timestamp = Millis(r.u64()?);
        r.finish()?;
        Ok(Self {
            body: Body::decode(&signed)?,
            signed,
            signature,
            timestamp,
        })
    }

    /// The stored bytes: the entry of the log.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u8(FORMAT_VERSION)
            .bytes(&self.signed)
            .raw(self.signature.as_bytes())
            .u64(self.timestamp.0);
        w.finish()
    }

    /// The hash of the stored bytes: what the next event names as `prev`.
    pub fn hash(&self) -> EventHash {
        EventHash::of(&self.to_bytes())
    }

    /// Check the signature of the author over the signed bytes as they were
    /// stored.
    pub fn verify(&self) -> Result<(), BadSignature> {
        self.body
            .author
            .verify(&signing_input(&self.signed), &self.signature)
            .then_some(())
            .ok_or(BadSignature)
    }

    /// What the author signed.
    pub fn body(&self) -> &Body {
        &self.body
    }

    /// The time that the server wrote.
    pub fn timestamp(&self) -> Millis {
        self.timestamp
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ids::GroupId,
        rng::{Seed, SimRng},
    };

    fn envelope(payload: &[u8]) -> Envelope {
        let identity = Identity::generate(&mut SimRng::new(Seed(5)));
        let draft = Draft::sign(
            LogId::Group(GroupId(1)),
            Seq(4),
            EventHash::of(b"before"),
            payload.to_vec(),
            &identity,
        );
        Envelope::stamp(draft, Millis(9)).unwrap()
    }

    #[test]
    fn a_changed_payload_breaks_the_signature() {
        let stored = envelope(b"payload").to_bytes();
        let at = stored.windows(7).position(|w| w == b"payload").unwrap();
        let mut forged = stored.clone();
        forged[at] ^= 1;
        let forged = Envelope::from_bytes(&forged).unwrap();
        assert_eq!(forged.verify(), Err(BadSignature));
    }

    #[test]
    fn the_timestamp_is_in_the_hash_but_not_in_the_signature() {
        let a = envelope(b"x");
        let mut b = a.clone();
        b.timestamp = Millis(10);
        assert!(b.verify().is_ok());
        assert_ne!(a.hash(), b.hash());
    }

    #[test]
    fn a_stored_event_with_bytes_left_over_is_refused() {
        let mut bytes = envelope(b"x").to_bytes();
        bytes.push(0);
        assert_eq!(Envelope::from_bytes(&bytes), Err(DecodeError::Trailing(1)));
    }

    #[test]
    fn an_unknown_version_is_refused() {
        let mut bytes = envelope(b"x").to_bytes();
        bytes[0] = 9;
        assert_eq!(
            Envelope::from_bytes(&bytes),
            Err(DecodeError::UnknownVersion(9))
        );
    }
}
