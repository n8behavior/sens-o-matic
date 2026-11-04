//! The OpenMLS provider of this crate: crypto, randomness and storage with one
//! source of randomness, the generator of the harness (question 5 of #62).
//!
//! The stock provider, `openmls_rust_crypto::RustCrypto`, cannot be seeded: it
//! fills a private `ChaCha20Rng` from the OS in `Default`, and its HPKE goes
//! through `hpke-rs`, whose `prng()` reads the OS again for every `Hpke` it
//! makes. So this module is a provider of its own, for the one cipher suite
//! that the crate uses (`MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519`, 0x0001):
//! the primitives come from the RustCrypto crates, HPKE (RFC 9180, base mode)
//! is written here, and the randomness is the [`SimRng`] that the provider holds.
//! [`SimProvider::scratch`] makes a second provider with a copy of the storage
//! that draws from the same generator, which is how a `make` method of the cipher
//! works without touching its state.
//!
//! A seeded generator is not enough, though. OpenMLS seals the path secrets of a
//! Commit on the rayon pool, so draws made inside `hpke_seal` would happen in the
//! order of the threads, and two runs with one seed would store different bytes
//! (`same_seed_twice_gives_identical_logs` failed on the first try for that
//! reason). The ephemeral key of an encapsulation is therefore a function of the
//! seed and of the inputs of the call (see [`SimCrypto`]), and the generator
//! serves only the calls that are made in order.
//!
//! The storage is the stock `openmls_memory_storage::MemoryStorage`: it draws
//! nothing and reads no clock, and its map is only ever read by key.
//!
//! `tests/provider.rs` opens what the stock provider seals and the other way
//! round, so the HPKE here is checked against `hpke-rs`.
//!
//! ```
//! use e2e_harness::rng::{Seed, SimRng};
//! use e2e_openmls::provider::SimProvider;
//! use openmls_traits::{OpenMlsProvider, random::OpenMlsRand};
//!
//! let a = SimProvider::new(SimRng::new(Seed(1)));
//! let b = SimProvider::new(SimRng::new(Seed(1)));
//! assert_eq!(a.rand().random_array::<8>(), b.rand().random_array::<8>());
//! ```

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, PoisonError, RwLock},
};

use aes_gcm::{
    Aes128Gcm, KeyInit,
    aead::{Aead, Payload},
};
use e2e_harness::rng::SimRng;
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use openmls_memory_storage::MemoryStorage;
use openmls_traits::{
    OpenMlsProvider,
    crypto::OpenMlsCrypto,
    random::OpenMlsRand,
    types::{
        AeadType, Ciphersuite, CryptoError, ExporterSecret, HashType, HpkeAeadType, HpkeCiphertext,
        HpkeConfig, HpkeKdfType, HpkeKemType, HpkeKeyPair, SignatureScheme,
    },
};
use rand::RngCore;
use sha2::{Digest, Sha256};
use thiserror::Error;
use x25519_dalek::{X25519_BASEPOINT_BYTES, x25519};

/// The one cipher suite of this crate.
pub const CIPHERSUITE: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

/// The lock of the generator was poisoned by a panic in another thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error(
    "the generator of the simulation is poisoned (see the module docs of e2e_openmls::provider)"
)]
pub struct Poisoned;

/// Crypto and randomness, from the generator of the harness.
///
/// The ephemeral key of an HPKE encapsulation is **not** drawn from the
/// generator at the time of the call. OpenMLS encrypts the path secrets of an
/// `UpdatePath` on the rayon pool, so the order in which threads draw would
/// decide which ciphertext gets which bytes, and two runs with one seed would
/// differ. Instead the key is a PRF of the inputs of the call under a secret
/// that the generator gave at creation: the same function of the seed on every
/// run, whatever the order of the threads. (Two encapsulations of equal inputs
/// are then equal. The path secrets of MLS are fresh, so this does not occur
/// there; a production provider would draw fresh bytes.)
#[derive(Debug, Clone)]
pub struct SimCrypto {
    rng: Arc<Mutex<SimRng>>,
    kem_seed: [u8; 32],
}

impl SimCrypto {
    fn new(mut rng: SimRng) -> Self {
        let mut kem_seed = [0u8; 32];
        rng.fill_bytes(&mut kem_seed);
        Self {
            rng: Arc::new(Mutex::new(rng)),
            kem_seed,
        }
    }

    /// The ephemeral key of an encapsulation of `parts`.
    fn ephemeral(&self, parts: &[&[u8]]) -> Result<[u8; 32], CryptoError> {
        let info: Vec<u8> = parts
            .iter()
            .flat_map(|part| [&(part.len() as u64).to_be_bytes()[..], part].concat())
            .collect();
        let mut sk = [0u8; 32];
        Hkdf::<Sha256>::from_prk(&self.kem_seed)
            .map_err(|_| CryptoError::InsufficientRandomness)?
            .expand(&info, &mut sk)
            .map_err(|_| CryptoError::InsufficientRandomness)?;
        Ok(sk)
    }

    fn fill(&self, out: &mut [u8]) -> Result<(), Poisoned> {
        self.rng
            .lock()
            .map_err(|_: PoisonError<_>| Poisoned)?
            .fill_bytes(out);
        Ok(())
    }
}

impl OpenMlsRand for SimCrypto {
    type Error = Poisoned;

    fn random_array<const N: usize>(&self) -> Result<[u8; N], Poisoned> {
        let mut out = [0u8; N];
        self.fill(&mut out)?;
        Ok(out)
    }

    fn random_vec(&self, len: usize) -> Result<Vec<u8>, Poisoned> {
        let mut out = vec![0u8; len];
        self.fill(&mut out)?;
        Ok(out)
    }
}

/// An OpenMLS provider whose only source of randomness is a [`SimRng`].
#[derive(Debug)]
pub struct SimProvider {
    crypto: SimCrypto,
    storage: MemoryStorage,
}

impl SimProvider {
    /// A provider with an empty storage that draws from `rng`.
    pub fn new(rng: SimRng) -> Self {
        Self {
            crypto: SimCrypto::new(rng),
            storage: MemoryStorage::default(),
        }
    }

    /// A provider with a copy of this storage that draws from the same
    /// generator: what it does leaves this one as it was.
    pub fn scratch(&self) -> Self {
        Self {
            crypto: self.crypto.clone(),
            storage: MemoryStorage {
                values: RwLock::new(self.entries().clone()),
            },
        }
    }

    /// A provider that draws from `rng`, with the entries of `storage`.
    pub fn restore(rng: SimRng, storage: HashMap<Vec<u8>, Vec<u8>>) -> Self {
        Self {
            crypto: SimCrypto::new(rng),
            storage: MemoryStorage {
                values: RwLock::new(storage),
            },
        }
    }

    /// A copy of the entries of the storage, for [`SimProvider::restore`].
    pub fn entries(&self) -> HashMap<Vec<u8>, Vec<u8>> {
        self.storage
            .values
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl OpenMlsProvider for SimProvider {
    type CryptoProvider = SimCrypto;
    type RandProvider = SimCrypto;
    type StorageProvider = MemoryStorage;

    fn storage(&self) -> &MemoryStorage {
        &self.storage
    }

    fn crypto(&self) -> &SimCrypto {
        &self.crypto
    }

    fn rand(&self) -> &SimCrypto {
        &self.crypto
    }
}

// ---- HPKE (RFC 9180, base mode, DHKEM(X25519, HKDF-SHA256), HKDF-SHA256, AES-128-GCM) ----

const KEM_ID: [u8; 2] = 0x0020u16.to_be_bytes();
const KDF_ID: [u8; 2] = 0x0001u16.to_be_bytes();
const AEAD_ID: [u8; 2] = 0x0001u16.to_be_bytes();

fn kem_suite() -> Vec<u8> {
    [b"KEM".as_slice(), &KEM_ID].concat()
}

fn hpke_suite() -> Vec<u8> {
    [b"HPKE".as_slice(), &KEM_ID, &KDF_ID, &AEAD_ID].concat()
}

/// `LabeledExtract`: the pseudorandom key, as bytes.
fn labeled_extract(suite: &[u8], salt: &[u8], label: &[u8], ikm: &[u8]) -> [u8; 32] {
    Hkdf::<Sha256>::extract(Some(salt), &[b"HPKE-v1", suite, label, ikm].concat())
        .0
        .into()
}

/// `LabeledExpand` of `L` bytes.
fn labeled_expand<const L: usize>(
    prk: &[u8; 32],
    suite: &[u8],
    label: &[u8],
    info: &[u8],
) -> Result<[u8; L], CryptoError> {
    let length = u16::try_from(L).map_err(|_| CryptoError::HkdfOutputLengthInvalid)?;
    let info = [
        &length.to_be_bytes(),
        b"HPKE-v1".as_slice(),
        suite,
        label,
        info,
    ]
    .concat();
    let mut out = [0u8; L];
    Hkdf::<Sha256>::from_prk(prk)
        .map_err(|_| CryptoError::HkdfOutputLengthInvalid)?
        .expand(&info, &mut out)
        .map_err(|_| CryptoError::HkdfOutputLengthInvalid)?;
    Ok(out)
}

/// The DHKEM shared secret from the DH output and the two public keys.
fn shared_secret(dh: &[u8; 32], enc: &[u8], pk_r: &[u8]) -> Result<[u8; 32], CryptoError> {
    if *dh == [0u8; 32] {
        return Err(CryptoError::InvalidPublicKey);
    }
    let suite = kem_suite();
    let prk = labeled_extract(&suite, b"", b"eae_prk", dh);
    labeled_expand(&prk, &suite, b"shared_secret", &[enc, pk_r].concat())
}

/// The key, the base nonce and the exporter secret of HPKE's key schedule.
struct Schedule {
    key: [u8; 16],
    nonce: [u8; 12],
    exporter: [u8; 32],
}

fn key_schedule(shared_secret: &[u8; 32], info: &[u8]) -> Result<Schedule, CryptoError> {
    let suite = hpke_suite();
    let context = [
        [0u8].as_slice(),
        &labeled_extract(&suite, b"", b"psk_id_hash", b""),
        &labeled_extract(&suite, b"", b"info_hash", info),
    ]
    .concat();
    let secret = labeled_extract(&suite, shared_secret, b"secret", b"");
    Ok(Schedule {
        key: labeled_expand(&secret, &suite, b"key", &context)?,
        nonce: labeled_expand(&secret, &suite, b"base_nonce", &context)?,
        exporter: labeled_expand(&secret, &suite, b"exp", &context)?,
    })
}

fn aead(
    key: &[u8],
    nonce: &[u8],
    seal: bool,
    data: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let cipher = Aes128Gcm::new_from_slice(key).map_err(|_| CryptoError::InvalidLength)?;
    let nonce = nonce.try_into().map_err(|_| CryptoError::InvalidLength)?;
    let payload = Payload { msg: data, aad };
    if seal {
        cipher
            .encrypt(nonce, payload)
            .map_err(|_| CryptoError::CryptoLibraryError)
    } else {
        cipher
            .decrypt(nonce, payload)
            .map_err(|_| CryptoError::AeadDecryptionError)
    }
}

fn require_suite(config: &HpkeConfig) -> Result<(), CryptoError> {
    match config {
        HpkeConfig(HpkeKemType::DhKem25519, HpkeKdfType::HkdfSha256, HpkeAeadType::AesGcm128) => {
            Ok(())
        }
        HpkeConfig(..) => Err(CryptoError::UnsupportedCiphersuite),
    }
}

fn array32(bytes: &[u8]) -> Result<[u8; 32], CryptoError> {
    bytes.try_into().map_err(|_| CryptoError::InvalidLength)
}

impl SimCrypto {
    /// `Encap(pk_r)`; `context` is what the ephemeral key is derived from.
    fn encap(&self, pk_r: &[u8], context: &[&[u8]]) -> Result<(Vec<u8>, [u8; 32]), CryptoError> {
        let pk = array32(pk_r)?;
        let sk_e = self.ephemeral(&[&[pk_r], context].concat())?;
        let enc = x25519(sk_e, X25519_BASEPOINT_BYTES);
        let secret = shared_secret(&x25519(sk_e, pk), &enc, &pk)?;
        Ok((enc.to_vec(), secret))
    }

    /// `Decap(enc, sk_r)`.
    fn decap(enc: &[u8], sk_r: &[u8]) -> Result<[u8; 32], CryptoError> {
        let sk = array32(sk_r)?;
        let pk_r = x25519(sk, X25519_BASEPOINT_BYTES);
        shared_secret(&x25519(sk, array32(enc)?), enc, &pk_r)
    }
}

impl OpenMlsCrypto for SimCrypto {
    fn supports(&self, ciphersuite: Ciphersuite) -> Result<(), CryptoError> {
        (ciphersuite == CIPHERSUITE)
            .then_some(())
            .ok_or(CryptoError::UnsupportedCiphersuite)
    }

    fn supported_ciphersuites(&self) -> Vec<Ciphersuite> {
        vec![CIPHERSUITE]
    }

    fn hkdf_extract(
        &self,
        hash_type: HashType,
        salt: &[u8],
        ikm: &[u8],
    ) -> Result<tls_codec::SecretVLBytes, CryptoError> {
        require_sha256(hash_type)?;
        Ok(Hkdf::<Sha256>::extract(Some(salt), ikm)
            .0
            .as_slice()
            .to_vec()
            .into())
    }

    fn hmac(
        &self,
        hash_type: HashType,
        key: &[u8],
        message: &[u8],
    ) -> Result<tls_codec::SecretVLBytes, CryptoError> {
        require_sha256(hash_type)?;
        let mut mac =
            Hmac::<Sha256>::new_from_slice(key).map_err(|_| CryptoError::InvalidLength)?;
        mac.update(message);
        Ok(mac.finalize().into_bytes().as_slice().to_vec().into())
    }

    fn hkdf_expand(
        &self,
        hash_type: HashType,
        prk: &[u8],
        info: &[u8],
        okm_len: usize,
    ) -> Result<tls_codec::SecretVLBytes, CryptoError> {
        require_sha256(hash_type)?;
        let hkdf =
            Hkdf::<Sha256>::from_prk(prk).map_err(|_| CryptoError::HkdfOutputLengthInvalid)?;
        let mut okm = vec![0u8; okm_len];
        hkdf.expand(info, &mut okm)
            .map_err(|_| CryptoError::HkdfOutputLengthInvalid)?;
        Ok(okm.into())
    }

    fn hash(&self, hash_type: HashType, data: &[u8]) -> Result<Vec<u8>, CryptoError> {
        require_sha256(hash_type)?;
        Ok(Sha256::digest(data).as_slice().to_vec())
    }

    fn aead_encrypt(
        &self,
        alg: AeadType,
        key: &[u8],
        data: &[u8],
        nonce: &[u8],
        aad: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        require_aes128(alg)?;
        aead(key, nonce, true, data, aad)
    }

    fn aead_decrypt(
        &self,
        alg: AeadType,
        key: &[u8],
        ct_tag: &[u8],
        nonce: &[u8],
        aad: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        require_aes128(alg)?;
        aead(key, nonce, false, ct_tag, aad)
    }

    fn signature_key_gen(&self, alg: SignatureScheme) -> Result<(Vec<u8>, Vec<u8>), CryptoError> {
        require_ed25519(alg)?;
        let mut seed = [0u8; 32];
        self.fill(&mut seed)
            .map_err(|_| CryptoError::InsufficientRandomness)?;
        let key = ed25519_dalek::SigningKey::from_bytes(&seed);
        Ok((seed.to_vec(), key.verifying_key().to_bytes().to_vec()))
    }

    fn verify_signature(
        &self,
        alg: SignatureScheme,
        data: &[u8],
        pk: &[u8],
        signature: &[u8],
    ) -> Result<(), CryptoError> {
        require_ed25519(alg)?;
        let key = ed25519_dalek::VerifyingKey::from_bytes(&array32(pk)?)
            .map_err(|_| CryptoError::InvalidPublicKey)?;
        let signature = ed25519_dalek::Signature::from_bytes(
            signature
                .try_into()
                .map_err(|_| CryptoError::SignatureDecodingError)?,
        );
        key.verify_strict(data, &signature)
            .map_err(|_| CryptoError::InvalidSignature)
    }

    fn sign(&self, alg: SignatureScheme, data: &[u8], key: &[u8]) -> Result<Vec<u8>, CryptoError> {
        require_ed25519(alg)?;
        let key = ed25519_dalek::SigningKey::from_bytes(&array32(key)?);
        Ok(ed25519_dalek::Signer::sign(&key, data).to_bytes().to_vec())
    }

    fn hpke_seal(
        &self,
        config: HpkeConfig,
        pk_r: &[u8],
        info: &[u8],
        aad: &[u8],
        ptxt: &[u8],
    ) -> Result<HpkeCiphertext, CryptoError> {
        require_suite(&config)?;
        let (enc, secret) = self.encap(pk_r, &[info, aad, ptxt])?;
        let schedule = key_schedule(&secret, info)?;
        Ok(HpkeCiphertext {
            kem_output: enc.into(),
            ciphertext: aead(&schedule.key, &schedule.nonce, true, ptxt, aad)?.into(),
        })
    }

    fn hpke_open(
        &self,
        config: HpkeConfig,
        input: &HpkeCiphertext,
        sk_r: &[u8],
        info: &[u8],
        aad: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        require_suite(&config)?;
        let secret = Self::decap(input.kem_output.as_slice(), sk_r)?;
        let schedule = key_schedule(&secret, info)?;
        aead(
            &schedule.key,
            &schedule.nonce,
            false,
            input.ciphertext.as_slice(),
            aad,
        )
        .map_err(|_| CryptoError::HpkeDecryptionError)
    }

    fn hpke_setup_sender_and_export(
        &self,
        config: HpkeConfig,
        pk_r: &[u8],
        info: &[u8],
        exporter_context: &[u8],
        exporter_length: usize,
    ) -> Result<(Vec<u8>, ExporterSecret), CryptoError> {
        require_suite(&config)?;
        let (enc, secret) = self.encap(pk_r, &[info, exporter_context])?;
        let exported = export(
            &key_schedule(&secret, info)?,
            exporter_context,
            exporter_length,
        )?;
        Ok((enc, exported.into()))
    }

    fn hpke_setup_receiver_and_export(
        &self,
        config: HpkeConfig,
        enc: &[u8],
        sk_r: &[u8],
        info: &[u8],
        exporter_context: &[u8],
        exporter_length: usize,
    ) -> Result<ExporterSecret, CryptoError> {
        require_suite(&config)?;
        let secret = Self::decap(enc, sk_r)?;
        let exported = export(
            &key_schedule(&secret, info)?,
            exporter_context,
            exporter_length,
        )?;
        Ok(exported.into())
    }

    fn derive_hpke_keypair(
        &self,
        config: HpkeConfig,
        ikm: &[u8],
    ) -> Result<HpkeKeyPair, CryptoError> {
        require_suite(&config)?;
        let suite = kem_suite();
        let prk = labeled_extract(&suite, b"", b"dkp_prk", ikm);
        let sk = labeled_expand::<32>(&prk, &suite, b"sk", b"")?;
        Ok(HpkeKeyPair {
            private: sk.to_vec().into(),
            public: x25519(sk, X25519_BASEPOINT_BYTES).to_vec(),
        })
    }
}

/// `Export(exporter_context, length)` of the key schedule.
fn export(schedule: &Schedule, context: &[u8], length: usize) -> Result<Vec<u8>, CryptoError> {
    let info = [
        &u16::try_from(length)
            .map_err(|_| CryptoError::HkdfOutputLengthInvalid)?
            .to_be_bytes(),
        b"HPKE-v1".as_slice(),
        &hpke_suite(),
        b"sec",
        context,
    ]
    .concat();
    let mut out = vec![0u8; length];
    Hkdf::<Sha256>::from_prk(&schedule.exporter)
        .map_err(|_| CryptoError::ExporterError)?
        .expand(&info, &mut out)
        .map_err(|_| CryptoError::ExporterError)?;
    Ok(out)
}

fn require_sha256(hash: HashType) -> Result<(), CryptoError> {
    match hash {
        HashType::Sha2_256 => Ok(()),
        HashType::Sha2_384 | HashType::Sha2_512 => Err(CryptoError::UnsupportedHashAlgorithm),
    }
}

fn require_aes128(alg: AeadType) -> Result<(), CryptoError> {
    match alg {
        AeadType::Aes128Gcm => Ok(()),
        AeadType::Aes256Gcm | AeadType::ChaCha20Poly1305 => {
            Err(CryptoError::UnsupportedAeadAlgorithm)
        }
    }
}

fn require_ed25519(alg: SignatureScheme) -> Result<(), CryptoError> {
    if alg == SignatureScheme::ED25519 {
        Ok(())
    } else {
        Err(CryptoError::UnsupportedSignatureScheme)
    }
}
