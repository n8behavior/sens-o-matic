//! Questions 4 of #62 and C13: what a device must keep, what a device that lost
//! its state can do, what a new device gets from a sibling, and whether a log
//! can be read twice.

mod support;

use e2e_harness::{
    cipher::{CipherError, GroupCipher},
    ids::{PingId, Seq},
    rng::{Seed, SimRng},
};
use e2e_openmls::{
    OpenMls,
    identity::{DeviceSigner, credential_of},
    provider::{CIPHERSUITE, SimProvider},
};
use openmls::prelude::{
    GroupId, KeyPackage, MlsGroup, MlsGroupCreateConfig, MlsGroupJoinConfig, MlsMessageBodyIn,
    MlsMessageIn, ProcessedMessageContent, StagedWelcome,
};
use support::Group;
use tls_codec::{Deserialize, Serialize};

const PING: PingId = PingId(1);

/// A ping event can be decrypted again and again, by the device that read it, by
/// a restored copy of that device, and by a sibling: design B keeps the key of
/// the epoch, so the log can be replayed from the start. The price is that the
/// key is kept.
#[test]
fn a_ping_log_can_be_read_again_from_the_start_with_the_key_of_its_epoch() {
    let mut group = Group::new(1);
    let (a, b) = (0, group.join(0, true));
    let opening = group.start_ping(a, PING);
    let ciphertext = group.devices[a]
        .cipher
        .encrypt_ping(PING, b"on my way")
        .expect("encrypted");

    for _ in 0..2 {
        let plain = group.devices[b]
            .cipher
            .decrypt_ping(PING, Seq(1), &ciphertext);
        assert_eq!(plain.as_deref(), Ok(&b"on my way"[..]));
    }

    // The device is closed and opened again from its saved state: still readable.
    let saved = group.devices[b].cipher.export();
    let identity = group.devices[b].identity.clone();
    let mut restored = OpenMls::restore(&identity, SimRng::new(Seed(5)), &saved).expect("restored");
    restored
        .open_ping(PING, Seq(1), &opening)
        .expect("the restored device opens the ping from its first event");
    assert_eq!(
        restored.decrypt_ping(PING, Seq(1), &ciphertext).as_deref(),
        Ok(&b"on my way"[..])
    );
}

/// What native MLS gives instead: an application message is a one-time read. The
/// key of its generation is deleted when it has decrypted, which is the forward
/// secrecy of MLS, and a second read is refused. This is the "no" of C13 for
/// ping events that are MLS application messages.
#[test]
fn an_mls_application_message_can_be_decrypted_once() {
    let (pa, pb) = (
        SimProvider::new(SimRng::new(Seed(1))),
        SimProvider::new(SimRng::new(Seed(2))),
    );
    let ids = [
        e2e_harness::keys::Identity::generate(&mut SimRng::new(Seed(3))),
        e2e_harness::keys::Identity::generate(&mut SimRng::new(Seed(4))),
    ];
    let signers = [DeviceSigner(ids[0].clone()), DeviceSigner(ids[1].clone())];
    let config = MlsGroupCreateConfig::builder()
        .ciphersuite(CIPHERSUITE)
        .use_ratchet_tree_extension(true)
        .build();
    let mut alice = MlsGroup::new_with_group_id(
        &pa,
        &signers[0],
        &config,
        GroupId::from_slice(b"g"),
        credential_of(&ids[0].device_key()),
    )
    .expect("group");
    let bob_package = KeyPackage::builder()
        .build(
            CIPHERSUITE,
            &pb,
            &signers[1],
            credential_of(&ids[1].device_key()),
        )
        .expect("key package");
    let (_, welcome, _) = alice
        .add_members(&pa, &signers[0], &[bob_package.key_package().clone()])
        .expect("add");
    alice.merge_pending_commit(&pa).expect("merged");
    let MlsMessageBodyIn::Welcome(welcome) =
        MlsMessageIn::tls_deserialize_exact(welcome.tls_serialize_detached().expect("serialized"))
            .expect("read")
            .extract()
    else {
        panic!("not a Welcome");
    };
    let mut bob =
        StagedWelcome::new_from_welcome(&pb, &MlsGroupJoinConfig::default(), welcome, None)
            .and_then(|staged| staged.into_group(&pb))
            .expect("bob joins");

    let message = alice
        .create_message(&pa, &signers[0], b"on my way")
        .expect("message")
        .tls_serialize_detached()
        .expect("serialized");
    let read = |bob: &mut MlsGroup| {
        let protocol = MlsMessageIn::tls_deserialize_exact(&message)
            .expect("read")
            .try_into_protocol_message()
            .expect("protocol message");
        bob.process_message(&pb, protocol)
            .map(|processed| processed.into_content())
    };
    assert!(matches!(
        read(&mut bob),
        Ok(ProcessedMessageContent::ApplicationMessage(_))
    ));
    assert!(
        read(&mut bob).is_err(),
        "the second read of an MLS application message must fail"
    );
}

/// A device that lost its state (a new cipher with the same key) can do nothing:
/// it has no group, so it cannot read a ping and cannot apply a Commit. It must
/// be replaced by a new device that a sibling adds, or join again by invite.
#[test]
fn a_device_that_lost_its_state_cannot_read_or_apply_anything() {
    let mut group = Group::new(2);
    let (a, b) = (0, group.join(0, true));
    group.start_ping(a, PING);
    let ciphertext = group.devices[a]
        .cipher
        .encrypt_ping(PING, b"hi")
        .expect("encrypted");

    let identity = group.devices[b].identity.clone();
    group.devices[b].cipher = OpenMls::new(&identity, SimRng::new(Seed(6)));
    assert!(matches!(
        group.devices[b]
            .cipher
            .decrypt_ping(PING, Seq(1), &ciphertext),
        Err(CipherError::State(_))
    ));
    group.tolerate.push(b);
    group.join(a, true);
    // The Commit of the join did not apply on the device without state.
    assert!(matches!(group.refused.as_slice(), [(device, CipherError::State(_))] if *device == b));
    assert!(group.devices[b].cipher.epoch().is_none());
}

/// A new device of a known user gets the keys of its sibling and reads the ping
/// that began before it came; a new user of the group gets none of them.
#[test]
fn a_new_device_reads_the_ping_that_began_before_it_and_a_new_user_does_not() {
    let mut group = Group::new(3);
    let (a, b) = (0, group.join(0, true));
    let opening = group.start_ping(a, PING);
    let before = group.devices[a]
        .cipher
        .encrypt_ping(PING, b"before")
        .expect("encrypted");

    let sibling = group.add_device(b, true);
    let stranger = group.join(a, true);
    let head = group.head();
    assert_eq!(
        group.devices[sibling]
            .cipher
            .open_ping(PING, head, &opening),
        Ok(())
    );
    assert_eq!(
        group.devices[sibling]
            .cipher
            .decrypt_ping(PING, Seq(1), &before)
            .as_deref(),
        Ok(&b"before"[..])
    );
    assert!(matches!(
        group.devices[stranger]
            .cipher
            .open_ping(PING, head, &opening),
        Err(CipherError::NoKey(_))
    ));
    assert!(
        group.devices[stranger]
            .cipher
            .decrypt_ping(PING, Seq(1), &before)
            .is_err()
    );
}

/// The MLS group of every live member has exactly the devices of the roster, after
/// joins, a new device and a removal, and a device keeps one key for each epoch it
/// was in: 32 bytes and the hash of a Commit (72 bytes in the saved state).
#[test]
fn the_mls_group_follows_the_roster_and_a_device_keeps_a_key_for_each_epoch() {
    let mut group = Group::new(4);
    let a = 0;
    let b = group.join(a, true);
    let c = group.join(a, true);
    group.add_device(b, true);
    group.remove(a, c);

    let roster: std::collections::BTreeSet<_> = group
        .state
        .roster()
        .devices()
        .map(|(_, device)| device.key)
        .collect();
    assert_eq!(roster.len(), 3);
    for idx in [a, b] {
        assert_eq!(group.devices[idx].cipher.members().as_ref(), Ok(&roster));
    }
    // Create, two joins, a new device and a removal: epochs 0 to 4 for the founder.
    assert_eq!(group.devices[a].cipher.epoch(), Some(4));
    assert_eq!(group.devices[a].cipher.epochs_kept(), 5);
    // The second member joined into epoch 1 and keeps epochs 1 to 4.
    assert_eq!(group.devices[b].cipher.epochs_kept(), 4);
}
