//! The provider of the crate against the stock provider of OpenMLS: the HPKE of
//! `provider` is written here, so it is checked against `hpke-rs`, which the
//! stock provider uses. The stock provider is a dev-dependency and nothing else.

use e2e_harness::rng::{Seed, SimRng};
use e2e_openmls::provider::SimProvider;
use openmls_rust_crypto::RustCrypto;
use openmls_traits::{
    OpenMlsProvider,
    crypto::OpenMlsCrypto,
    random::OpenMlsRand,
    types::{
        CryptoError, HpkeAeadType, HpkeConfig, HpkeKdfType, HpkeKemType, HpkeKeyPair,
        SignatureScheme,
    },
};

fn config() -> HpkeConfig {
    HpkeConfig(
        HpkeKemType::DhKem25519,
        HpkeKdfType::HkdfSha256,
        HpkeAeadType::AesGcm128,
    )
}

fn pair(crypto: &impl OpenMlsCrypto, ikm: &[u8]) -> HpkeKeyPair {
    crypto
        .derive_hpke_keypair(config(), ikm)
        .expect("a key pair")
}

fn providers(seed: u64) -> (SimProvider, RustCrypto) {
    (
        SimProvider::new(SimRng::new(Seed(seed))),
        RustCrypto::default(),
    )
}

#[test]
fn the_stock_provider_derives_the_same_key_pair() {
    let (mine, stock) = providers(1);
    let (a, b) = (pair(mine.crypto(), b"ikm"), pair(&stock, b"ikm"));
    assert_eq!(
        (a.private.as_ref(), a.public),
        (b.private.as_ref(), b.public)
    );
}

#[test]
fn the_stock_provider_opens_what_this_one_seals() {
    let (mine, stock) = providers(2);
    let receiver = pair(&stock, b"receiver");
    let sealed = mine
        .crypto()
        .hpke_seal(config(), &receiver.public, b"info", b"aad", b"secret")
        .expect("sealed");
    let opened = stock.hpke_open(
        config(),
        &sealed,
        receiver.private.as_ref(),
        b"info",
        b"aad",
    );
    assert_eq!(opened.as_deref(), Ok(&b"secret"[..]));
}

#[test]
fn this_provider_opens_what_the_stock_one_seals() {
    let (mine, stock) = providers(3);
    let receiver = pair(mine.crypto(), b"receiver");
    let sealed = stock
        .hpke_seal(config(), &receiver.public, b"info", b"aad", b"secret")
        .expect("sealed");
    let open = |aad: &[u8]| {
        mine.crypto()
            .hpke_open(config(), &sealed, receiver.private.as_ref(), b"info", aad)
    };
    assert_eq!(open(b"aad").as_deref(), Ok(&b"secret"[..]));
    assert_eq!(open(b"other"), Err(CryptoError::HpkeDecryptionError));
}

#[test]
fn the_exporters_of_both_providers_agree() {
    let (mine, stock) = providers(4);
    let receiver = pair(&stock, b"receiver");
    let (enc, sent) = mine
        .crypto()
        .hpke_setup_sender_and_export(config(), &receiver.public, b"info", b"ctx", 32)
        .expect("sender");
    let received = stock
        .hpke_setup_receiver_and_export(
            config(),
            &enc,
            receiver.private.as_ref(),
            b"info",
            b"ctx",
            32,
        )
        .expect("receiver");
    assert_eq!(&*sent, &*received);
}

/// The ephemeral key of an encapsulation is a function of the seed and of the
/// inputs, not of the order of the calls: OpenMLS seals the path secrets of a
/// Commit on several threads, and two runs with one seed must still agree.
#[test]
fn an_encapsulation_does_not_depend_on_the_order_of_the_calls() {
    let receiver = pair(&RustCrypto::default(), b"receiver");
    let seal = |provider: &SimProvider, text: &[u8]| {
        provider
            .crypto()
            .hpke_seal(config(), &receiver.public, b"info", b"", text)
            .expect("sealed")
    };
    let (a, b) = (
        SimProvider::new(SimRng::new(Seed(5))),
        SimProvider::new(SimRng::new(Seed(5))),
    );
    let (a1, a2) = (seal(&a, b"one"), seal(&a, b"two"));
    let (b2, b1) = (seal(&b, b"two"), seal(&b, b"one"));
    assert_eq!(a1.kem_output.as_slice(), b1.kem_output.as_slice());
    assert_eq!(a2.ciphertext.as_slice(), b2.ciphertext.as_slice());
    assert_ne!(a1.kem_output.as_slice(), a2.kem_output.as_slice());
}

#[test]
fn two_providers_with_one_seed_draw_the_same_bytes_and_a_scratch_follows_its_parent() {
    let (a, b) = (
        SimProvider::new(SimRng::new(Seed(6))),
        SimProvider::new(SimRng::new(Seed(6))),
    );
    let scratch = a.scratch();
    let x = scratch.rand().random_array::<16>().expect("bytes");
    assert_eq!(b.rand().random_array::<16>(), Ok(x));
    // The scratch drew from the generator of `a`, so `a` moved on too.
    assert_ne!(a.rand().random_array::<16>(), Ok(x));
}

#[test]
fn a_signature_verifies_and_a_forged_one_does_not() {
    let provider = SimProvider::new(SimRng::new(Seed(7)));
    let crypto = provider.crypto();
    let (sk, pk) = crypto
        .signature_key_gen(SignatureScheme::ED25519)
        .expect("keys");
    let signature = crypto
        .sign(SignatureScheme::ED25519, b"m", &sk)
        .expect("signed");
    assert_eq!(
        crypto.verify_signature(SignatureScheme::ED25519, b"m", &pk, &signature),
        Ok(())
    );
    assert_eq!(
        crypto.verify_signature(SignatureScheme::ED25519, b"n", &pk, &signature),
        Err(CryptoError::InvalidSignature)
    );
}
