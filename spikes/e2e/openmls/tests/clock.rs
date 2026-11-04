//! Question 5 of #62, the clock: OpenMLS reads the wall clock when it validates a
//! key package, and has no way to be told the time. The crate gives every key
//! package the lifetime 0 to `u64::MAX`, so the check cannot fail at any time.
//! These tests show what the check does, and that a replay five years later still
//! accepts the Adds of the log.
//!
//! They read the wall clock, which `clippy.toml` refuses everywhere else: this is
//! the one file where it is allowed, because the wall clock is what is tested.

#![allow(clippy::disallowed_methods)]

mod support;

use std::time::{Duration, SystemTime};

use e2e_harness::rng::{Seed, SimRng};
use e2e_openmls::{
    identity::{DeviceSigner, credential_of},
    provider::{CIPHERSUITE, SimProvider},
};
use openmls::prelude::{KeyPackage, KeyPackageIn, Lifetime, ProtocolVersion};
use openmls_traits::OpenMlsProvider;
use support::Group;
use tls_codec::{Deserialize, Serialize};

const FIVE_YEARS: Duration = Duration::from_secs(5 * 365 * 24 * 3600);

fn key_package(lifetime: Lifetime) -> (KeyPackage, SimProvider) {
    let mut rng = SimRng::new(Seed(1));
    let identity = e2e_harness::keys::Identity::generate(&mut rng);
    let provider = SimProvider::new(rng);
    let bundle = KeyPackage::builder()
        .key_package_lifetime(lifetime)
        .build(
            CIPHERSUITE,
            &provider,
            &DeviceSigner(identity.clone()),
            credential_of(&identity.device_key()),
        )
        .expect("a key package");
    (bundle.key_package().clone(), provider)
}

/// OpenMLS validates a key package against `SystemTime::now()` and nothing else:
/// a lifetime that is over is refused by `KeyPackageIn::validate`, the call that
/// the cipher makes for every Add.
#[test]
fn openmls_checks_the_lifetime_of_a_key_package_against_the_wall_clock() {
    let validate = |lifetime| {
        let (key_package, provider) = key_package(lifetime);
        let bytes = key_package.tls_serialize_detached().expect("serialized");
        KeyPackageIn::tls_deserialize_exact(&bytes)
            .expect("read")
            .validate(provider.crypto(), ProtocolVersion::Mls10)
            .map(|_| ())
    };
    assert!(
        validate(Lifetime::default()).is_ok(),
        "the default lifetime is valid now"
    );
    assert!(
        validate(Lifetime::init(0, 1)).is_err(),
        "a lifetime that ended in 1970 is refused"
    );
    assert!(validate(Lifetime::init(0, u64::MAX)).is_ok());
}

/// With the default lifetime (about three months) a key package is expired five
/// years later, so a replay would refuse the old Add. With the lifetime of this
/// crate it is valid at any time.
#[test]
fn five_years_later_the_default_lifetime_has_ended_and_the_lifetime_of_this_crate_has_not() {
    let later = SystemTime::now() + FIVE_YEARS;
    let (default, _) = key_package(Lifetime::default());
    assert!(
        default
            .life_time()
            .validate_with_time(SystemTime::now())
            .is_ok()
    );
    assert!(default.life_time().validate_with_time(later).is_err());

    let (forever, _) = key_package(Lifetime::init(0, u64::MAX));
    assert!(forever.life_time().validate_with_time(later).is_ok());
    let far = SystemTime::UNIX_EPOCH + Duration::from_secs(4_000_000_000);
    assert!(forever.life_time().validate_with_time(far).is_ok());
}

/// Every Add of a real group, read back from the roster, is valid now and five
/// years later.
#[test]
fn every_add_of_a_group_replays_five_years_later() {
    let mut group = Group::new(7);
    for _ in 0..4 {
        group.join(0, true);
    }
    group.add_device(1, true);
    let crypto = SimProvider::new(SimRng::new(Seed(9)));
    let later = SystemTime::now() + FIVE_YEARS;
    let mut checked = 0;
    for (_, device) in group.state.roster().devices() {
        if device.introduction.is_empty() {
            continue; // an entry without a key package has no lifetime to check
        }
        let key_package = KeyPackageIn::tls_deserialize_exact(device.introduction.as_slice())
            .expect("read")
            .validate(crypto.crypto(), ProtocolVersion::Mls10)
            .expect("valid now");
        key_package
            .life_time()
            .validate_with_time(later)
            .expect("valid five years later");
        checked += 1;
    }
    // The four joiners and the new device, and the founder when its Create carries a key package.
    assert!(checked >= 5, "{checked} key packages checked");
}

/// The clock reads that remain in OpenMLS do not reach a message: at every epoch
/// change it stamps the message secrets with `SystemTime::now()` (`added_at`, for
/// the policies that delete past epochs by age). The stamp is part of the saved
/// state, so two runs with one seed send the same bytes and save different ones.
#[test]
fn the_saved_state_holds_a_wall_clock_stamp_that_no_message_does() {
    use e2e_harness::cipher::GroupCipher;

    let run = || {
        let mut group = Group::new(3);
        group.join(0, true);
        let ciphertext = {
            group.start_ping(0, e2e_harness::ids::PingId(1));
            group.devices[0]
                .cipher
                .encrypt_ping(e2e_harness::ids::PingId(1), b"x")
                .expect("encrypted")
        };
        (group.devices[0].cipher.export(), ciphertext)
    };
    let ((state_a, ping_a), (state_b, ping_b)) = (run(), run());
    assert_eq!(ping_a, ping_b, "what is sent is a function of the seed");
    assert_ne!(state_a, state_b, "what is saved holds the time of the run");
}
