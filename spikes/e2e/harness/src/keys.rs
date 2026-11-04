//! Device keys, signatures and hashes.
//!
//! A user is a set of device keys (D6 of #61). A device holds an [`Identity`],
//! the Ed25519 key pair that signs its events; the public half is its
//! [`DeviceKey`], the name of the device in every log. The keys come from the
//! seeded generator, and Ed25519 signs without randomness, so a run with one
//! seed makes the same bytes.
//!
//! An [`EventHash`] is the SHA-256 of a stored event; each event names the hash
//! of the event before it (see [`crate::event`]).
//!
//! ```
//! use e2e_harness::keys::Identity;
//! use e2e_harness::rng::{Seed, SimRng};
//!
//! let mut rng = SimRng::new(Seed(1));
//! let alice = Identity::generate(&mut rng);
//! let signature = alice.sign(b"hello");
//! assert!(alice.device_key().verify(b"hello", &signature));
//! assert!(!alice.device_key().verify(b"hullo", &signature));
//! ```

use std::fmt;

use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use rand::RngCore;
use sha2::{Digest, Sha256};

use crate::rng::SimRng;

/// The public key of a device: its name in the logs.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceKey([u8; 32]);

impl DeviceKey {
    /// The key from its 32 bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// The 32 bytes of the key.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// True when `signature` is this device's signature of `message`.
    pub fn verify(&self, message: &[u8], signature: &Signature) -> bool {
        let Ok(key) = VerifyingKey::from_bytes(&self.0) else {
            return false;
        };
        key.verify(message, &ed25519_dalek::Signature::from_bytes(&signature.0))
            .is_ok()
    }
}

impl fmt::Display for DeviceKey {
    /// `dev:` and the first four bytes in hex.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "dev:")?;
        self.0[..4]
            .iter()
            .try_for_each(|byte| write!(f, "{byte:02x}"))
    }
}

impl fmt::Debug for DeviceKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// An Ed25519 signature.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Signature([u8; 64]);

impl Signature {
    /// The signature from its 64 bytes.
    pub fn from_bytes(bytes: [u8; 64]) -> Self {
        Self(bytes)
    }

    /// The 64 bytes of the signature.
    pub fn as_bytes(&self) -> &[u8; 64] {
        &self.0
    }
}

impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "sig:")?;
        self.0[..4]
            .iter()
            .try_for_each(|byte| write!(f, "{byte:02x}"))
    }
}

/// The key pair of a device.
///
/// A cipher gets a reference to its device's identity, so that it can sign what
/// its protocol needs signed (for example the credential of an MLS member).
#[derive(Clone)]
pub struct Identity {
    signing: SigningKey,
}

impl Identity {
    /// A new key pair, from the 32 bytes that `rng` draws.
    pub fn generate(rng: &mut SimRng) -> Self {
        let mut seed = [0u8; 32];
        rng.fill_bytes(&mut seed);
        Self {
            signing: SigningKey::from_bytes(&seed),
        }
    }

    /// The public key of the device.
    pub fn device_key(&self) -> DeviceKey {
        DeviceKey(self.signing.verifying_key().to_bytes())
    }

    /// Sign `message`.
    pub fn sign(&self, message: &[u8]) -> Signature {
        Signature(self.signing.sign(message).to_bytes())
    }
}

impl fmt::Debug for Identity {
    /// The public key only: a secret key must not reach a log.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Identity({})", self.device_key())
    }
}

/// The SHA-256 of a stored event, the link of the hash chain.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventHash([u8; 32]);

impl EventHash {
    /// The `prev` of the first event of a log: there is no event before it.
    pub const GENESIS: Self = Self([0; 32]);

    /// The hash of `bytes`.
    pub fn of(bytes: &[u8]) -> Self {
        Self(Sha256::digest(bytes).into())
    }

    /// The hash from its 32 bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// The 32 bytes of the hash.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for EventHash {
    /// The first four bytes in hex.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0[..4]
            .iter()
            .try_for_each(|byte| write!(f, "{byte:02x}"))
    }
}

impl fmt::Debug for EventHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "hash:{self}")
    }
}
