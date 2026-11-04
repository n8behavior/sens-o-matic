//! The primitives behind the cipher. Each is a well-known crate and none is
//! written here:
//!
//! | Use | Primitive | Crate |
//! |---|---|---|
//! | Wrap a key for one device | HPKE base mode, DHKEM(X25519, HKDF-SHA256), HKDF-SHA256, ChaCha20-Poly1305 (RFC 9180) | `hpke` |
//! | Wrap a ping key under a group key, seal a ping event | XChaCha20-Poly1305 | `chacha20poly1305` |
//! | Commit to a group key | SHA-256 | `sha2` |
//!
//! All randomness comes from the [`SimRng`] that the cipher was given: the keys,
//! the nonces of XChaCha20-Poly1305 and the ephemeral keys of HPKE (through
//! `HpkeRng`, which hands the generator to the version of `rand_core` that
//! `hpke` asks for). The crates are built without default features, so the OS
//! randomness is not even compiled in.

use std::{convert::Infallible, fmt};

use chacha20poly1305::{
    KeyInit, XChaCha20Poly1305,
    aead::{Aead, Payload},
};
use e2e_harness::{cipher::CipherError, ids::Material, rng::SimRng};
use hpke::{
    Deserializable, Kem as _, OpModeR, OpModeS, Serializable, aead::ChaCha20Poly1305,
    kdf::HkdfSha256, kem::X25519HkdfSha256, single_shot_open, single_shot_seal_with_rng,
};
use rand::RngCore;
use rand_core10::{TryCryptoRng, TryRng};
use sha2::{Digest, Sha256};

use crate::invalid;

/// The width of every key, of the encapsulated key of HPKE and of a commitment.
pub(crate) const KEY_LEN: usize = 32;
/// The width of the nonce of XChaCha20-Poly1305.
pub(crate) const NONCE_LEN: usize = 24;
/// The width of the tag that both AEADs append.
pub(crate) const TAG_LEN: usize = 16;

/// The generator of the harness, seen by `hpke`.
///
/// `hpke` asks for the traits of `rand_core` 0.10 and [`SimRng`] has those of
/// 0.9: this forwards the three calls.
struct HpkeRng<'a>(&'a mut SimRng);

impl TryRng for HpkeRng<'_> {
    type Error = Infallible;

    fn try_next_u32(&mut self) -> Result<u32, Infallible> {
        Ok(self.0.next_u32())
    }

    fn try_next_u64(&mut self) -> Result<u64, Infallible> {
        Ok(self.0.next_u64())
    }

    fn try_fill_bytes(&mut self, dst: &mut [u8]) -> Result<(), Infallible> {
        self.0.fill_bytes(dst);
        Ok(())
    }
}

impl TryCryptoRng for HpkeRng<'_> {}

macro_rules! secret_32 {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone)]
        pub(crate) struct $name([u8; KEY_LEN]);

        impl $name {
            /// A new key from `rng`.
            pub(crate) fn generate(rng: &mut SimRng) -> Self {
                let mut bytes = [0u8; KEY_LEN];
                rng.fill_bytes(&mut bytes);
                Self(bytes)
            }

            pub(crate) fn from_bytes(bytes: [u8; KEY_LEN]) -> Self {
                Self(bytes)
            }

            pub(crate) fn as_bytes(&self) -> &[u8; KEY_LEN] {
                &self.0
            }
        }

        impl fmt::Debug for $name {
            /// The name only: a secret key must not reach a log.
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!(stringify!($name), "(..)"))
            }
        }
    };
}

secret_32! {
    /// The key of one epoch of the group, held by every device of the group.
    GroupKey
}

secret_32! {
    /// The key of one ping. It is made by the starter of the ping and travels in
    /// the first event of the ping log, sealed under a `GroupKey`.
    PingKey
}

/// A hash that names a `GroupKey` without giving it away.
///
/// A message that hands out a key carries its commitment, so a device that opens
/// a wrap checks that it got the key that everyone else got. Without it, a
/// member could wrap different keys for different devices and split the group
/// while every device accepts the event.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Commitment([u8; KEY_LEN]);

impl Commitment {
    const DOMAIN: &'static [u8] = b"sens-o-matic/plain-key/v1/key-commitment";

    pub(crate) fn of(key: &GroupKey) -> Self {
        Self(
            Sha256::new_with_prefix(Self::DOMAIN)
                .chain_update(key.as_bytes())
                .finalize()
                .into(),
        )
    }

    pub(crate) fn from_bytes(bytes: [u8; KEY_LEN]) -> Self {
        Self(bytes)
    }

    pub(crate) fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }
}

impl fmt::Debug for Commitment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0[..4]
            .iter()
            .try_for_each(|byte| write!(f, "{byte:02x}"))
    }
}

type Kem = X25519HkdfSha256;
type PrivateKey = <Kem as hpke::Kem>::PrivateKey;
type PublicKey = <Kem as hpke::Kem>::PublicKey;
type EncappedKey = <Kem as hpke::Kem>::EncappedKey;

/// The private half of the key pair of a device. HPKE opens a wrap with it.
#[derive(Clone)]
pub(crate) struct DeviceSecret(PrivateKey);

impl DeviceSecret {
    /// A new key pair from `rng`. The public half is the introduction of the
    /// device (see `DevicePublic`).
    pub(crate) fn generate(rng: &mut SimRng) -> (Self, DevicePublic) {
        let mut ikm = [0u8; KEY_LEN];
        rng.fill_bytes(&mut ikm);
        let (secret, public) = Kem::derive_keypair(&ikm);
        (Self(secret), DevicePublic(public))
    }

    pub(crate) fn from_bytes(bytes: &[u8; KEY_LEN]) -> Result<Self, CipherError> {
        PrivateKey::from_bytes(bytes)
            .map(Self)
            .map_err(|_| invalid("the exported state holds no private key of a device"))
    }

    pub(crate) fn to_bytes(&self) -> [u8; KEY_LEN] {
        self.0.to_bytes().into()
    }

    pub(crate) fn public(&self) -> DevicePublic {
        DevicePublic(Kem::sk_to_pk(&self.0))
    }
}

impl fmt::Debug for DeviceSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DeviceSecret(..)")
    }
}

/// The public half of the key pair of a device: its introduction (D6, D11).
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct DevicePublic(PublicKey);

impl DevicePublic {
    /// Read an introduction, which travels in the clear in `Create`,
    /// `JoinRequest` and `AddDevice`.
    pub(crate) fn from_introduction(introduction: &Material) -> Result<Self, CipherError> {
        PublicKey::from_bytes(introduction.as_slice())
            .map(Self)
            .map_err(|_| {
                invalid(format!(
                    "an introduction is {KEY_LEN} bytes, a public key of HPKE; got {}",
                    introduction.len()
                ))
            })
    }

    pub(crate) fn to_introduction(&self) -> Material {
        Material::from(self.0.to_bytes().to_vec())
    }
}

impl fmt::Debug for DevicePublic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DevicePublic(..)")
    }
}

/// An HPKE message to one device: the encapsulated key and the ciphertext.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Sealed {
    pub(crate) enc: [u8; KEY_LEN],
    pub(crate) ct: Vec<u8>,
}

impl Sealed {
    /// The width of a sealed key: the encapsulated key and the key with its tag.
    pub(crate) const KEY_WIDTH: usize = KEY_LEN + KEY_LEN + TAG_LEN;
}

/// Seal `plaintext` to `to`. `info` says what the message is for and who sends
/// it to whom; `aad` is bound to the ciphertext.
pub(crate) fn seal_to(
    to: &DevicePublic,
    info: &[u8],
    aad: &[u8],
    plaintext: &[u8],
    rng: &mut SimRng,
) -> Result<Sealed, CipherError> {
    let (enc, ct) = single_shot_seal_with_rng::<ChaCha20Poly1305, HkdfSha256, Kem>(
        &OpModeS::Base,
        &to.0,
        info,
        plaintext,
        aad,
        &mut HpkeRng(rng),
    )
    .map_err(|error| invalid(format!("HPKE could not seal to this device ({error:?})")))?;
    Ok(Sealed {
        enc: enc.to_bytes().into(),
        ct,
    })
}

/// Open what [`seal_to`] sealed to the device of `secret`.
pub(crate) fn open_with(
    secret: &DeviceSecret,
    info: &[u8],
    aad: &[u8],
    sealed: &Sealed,
) -> Result<Vec<u8>, CipherError> {
    let enc = EncappedKey::from_bytes(&sealed.enc)
        .map_err(|_| invalid("the encapsulated key of a wrap is not a key"))?;
    single_shot_open::<ChaCha20Poly1305, HkdfSha256, Kem>(
        &OpModeR::Base,
        &secret.0,
        &enc,
        info,
        &sealed.ct,
        aad,
    )
    .map_err(|_| {
        invalid("a wrap does not open with the key of this device: it was not made for it, or it was altered")
    })
}

/// Seal `plaintext` under a 32-byte key with XChaCha20-Poly1305: a random nonce
/// from `rng` in front, then the ciphertext and the tag.
pub(crate) fn seal_sym(
    key: &[u8; KEY_LEN],
    aad: &[u8],
    plaintext: &[u8],
    rng: &mut SimRng,
) -> Vec<u8> {
    let mut nonce = [0u8; NONCE_LEN];
    rng.fill_bytes(&mut nonce);
    let ct = XChaCha20Poly1305::new(&(*key).into())
        .encrypt(
            &nonce.into(),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .expect("XChaCha20-Poly1305 seals any message that fits in memory");
    [nonce.as_slice(), &ct].concat()
}

/// Open what [`seal_sym`] sealed.
pub(crate) fn open_sym(
    key: &[u8; KEY_LEN],
    aad: &[u8],
    sealed: &[u8],
) -> Result<Vec<u8>, CipherError> {
    let (nonce, ct) = sealed
        .split_first_chunk::<NONCE_LEN>()
        .ok_or_else(|| invalid("a sealed message is shorter than its nonce"))?;
    XChaCha20Poly1305::new(&(*key).into())
        .decrypt(&(*nonce).into(), Payload { msg: ct, aad })
        .map_err(|_| invalid("a message does not open with this key: wrong key, or altered"))
}

#[cfg(test)]
mod tests {
    use e2e_harness::rng::Seed;

    use super::*;

    #[test]
    fn a_wrap_opens_for_its_device_and_for_no_other() {
        let mut rng = SimRng::new(Seed(1));
        let (alice, alice_pk) = DeviceSecret::generate(&mut rng);
        let (bob, _) = DeviceSecret::generate(&mut rng);
        let key = GroupKey::generate(&mut rng);

        let sealed = seal_to(&alice_pk, b"info", b"aad", key.as_bytes(), &mut rng).unwrap();
        assert_eq!(sealed.enc.len() + sealed.ct.len(), Sealed::KEY_WIDTH);
        assert_eq!(
            open_with(&alice, b"info", b"aad", &sealed).unwrap(),
            key.as_bytes()
        );
        assert!(open_with(&bob, b"info", b"aad", &sealed).is_err());
        assert!(open_with(&alice, b"other", b"aad", &sealed).is_err());
        assert!(open_with(&alice, b"info", b"other", &sealed).is_err());
    }

    #[test]
    fn a_sealed_message_opens_with_its_key_only() {
        let mut rng = SimRng::new(Seed(2));
        let key = PingKey::generate(&mut rng);
        let other = PingKey::generate(&mut rng);
        let sealed = seal_sym(key.as_bytes(), b"ad", b"hello", &mut rng);
        assert_eq!(sealed.len(), NONCE_LEN + 5 + TAG_LEN);
        assert_eq!(open_sym(key.as_bytes(), b"ad", &sealed).unwrap(), b"hello");
        assert!(open_sym(other.as_bytes(), b"ad", &sealed).is_err());
        assert!(open_sym(key.as_bytes(), b"ad2", &sealed).is_err());
    }

    #[test]
    fn the_same_seed_gives_the_same_bytes() {
        let run = || {
            let mut rng = SimRng::new(Seed(3));
            let (_, pk) = DeviceSecret::generate(&mut rng);
            let key = GroupKey::generate(&mut rng);
            let sealed = seal_to(&pk, b"i", b"a", key.as_bytes(), &mut rng).unwrap();
            (sealed, seal_sym(key.as_bytes(), b"a", b"x", &mut rng))
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn a_secret_is_not_in_the_debug_output() {
        let key = GroupKey::from_bytes([0xAB; KEY_LEN]);
        assert_eq!(format!("{key:?}"), "GroupKey(..)");
    }
}
