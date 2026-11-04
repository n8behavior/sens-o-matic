//! What the cipher checks on its own, with material that an honest device does
//! not make: a rekey that leaves a device out, a wrap for the wrong device, keys
//! that a commitment does not name. The rig drives the ciphers by hand (see
//! `common`), so each test sees the verdict of every device.
//!
//! These are the questions 8 (a device that no user vouches for) and the "C14"
//! of #62 (a malicious member) for the plain key.

mod common;

use common::Rig;
use e2e_harness::{
    cipher::{CipherError, GroupCipher},
    ids::{Material, PingId, Seq},
    payload::GroupChange,
};

/// Bytes of a rekey: the epoch, the commitment, then 80 bytes for each device.
const HEADER: usize = 4 + 32;
const WRAP: usize = 80;

fn verdicts_of(
    rig: &mut Rig,
    author: usize,
    change: GroupChange,
    material: Material,
) -> Vec<Result<(), CipherError>> {
    rig.deliver(author, change, material)
        .expect("the rules accept the event")
        .into_iter()
        .map(|(_, verdict)| verdict)
        .collect()
}

fn invalid(verdict: &Result<(), CipherError>) -> bool {
    matches!(verdict, Err(CipherError::Invalid(_)))
}

#[test]
fn a_rekey_that_leaves_a_device_out_is_refused_by_every_device_alike() {
    let mut rig = Rig::group_of(1, 4);
    let user = rig.user(3);
    let honest = rig.removal(0, user);
    assert_eq!(honest.len(), HEADER + 3 * WRAP, "three devices stay");

    let short = Material::from(&honest.as_slice()[..honest.len() - WRAP]);
    let verdicts = verdicts_of(&mut rig, 0, GroupChange::Remove { user }, short);
    assert_eq!(verdicts.len(), 3);
    assert!(verdicts.iter().all(invalid), "{verdicts:?}");
    for member in 0..3 {
        assert_eq!(rig.members[member].cipher.current_epoch(), Some(0));
    }
}

#[test]
fn a_rekey_with_a_wrap_for_a_device_that_no_user_vouches_for_is_refused_alike() {
    let mut rig = Rig::group_of(2, 4);
    let user = rig.user(3);
    let honest = rig.removal(0, user);

    let long =
        Material::from([honest.as_slice(), &honest.as_slice()[HEADER..HEADER + WRAP]].concat());
    let verdicts = verdicts_of(&mut rig, 0, GroupChange::Remove { user }, long);
    assert_eq!(verdicts.len(), 3);
    assert!(verdicts.iter().all(invalid), "{verdicts:?}");
}

#[test]
fn a_rekey_with_the_wraps_swapped_opens_for_no_one_who_was_swapped() {
    let mut rig = Rig::group_of(3, 4);
    let user = rig.user(3);
    let honest = rig.removal(0, user);

    // Devices 0, 1 and 2 stay, in roster order. Swap the wraps of 1 and 2.
    let mut bytes = honest.as_slice().to_vec();
    let (first, second) = (HEADER + WRAP, HEADER + 2 * WRAP);
    let wrap_one: Vec<u8> = bytes[first..first + WRAP].to_vec();
    let wrap_two: Vec<u8> = bytes[second..second + WRAP].to_vec();
    bytes[first..first + WRAP].copy_from_slice(&wrap_two);
    bytes[second..second + WRAP].copy_from_slice(&wrap_one);

    let verdicts = verdicts_of(
        &mut rig,
        0,
        GroupChange::Remove { user },
        Material::from(bytes),
    );
    assert!(
        verdicts[0].is_ok(),
        "device 0 has its own wrap: {verdicts:?}"
    );
    assert!(
        invalid(&verdicts[1]) && invalid(&verdicts[2]),
        "{verdicts:?}"
    );
}

#[test]
fn a_rekey_that_names_the_wrong_epoch_is_refused_alike() {
    let mut rig = Rig::group_of(4, 3);
    let user = rig.user(2);
    let honest = rig.removal(0, user);
    let mut bytes = honest.as_slice().to_vec();
    bytes[3] = 7; // epoch 7 instead of 1

    let verdicts = verdicts_of(
        &mut rig,
        0,
        GroupChange::Remove { user },
        Material::from(bytes),
    );
    assert!(verdicts.iter().all(invalid), "{verdicts:?}");
}

#[test]
fn a_changed_commitment_is_refused_by_the_device_that_opens_the_wrap() {
    let mut rig = Rig::group_of(5, 3);
    let user = rig.user(2);
    let honest = rig.removal(0, user);
    let mut bytes = honest.as_slice().to_vec();
    bytes[4] ^= 1; // the first byte of the commitment

    let verdicts = verdicts_of(
        &mut rig,
        0,
        GroupChange::Remove { user },
        Material::from(bytes),
    );
    assert!(verdicts.iter().all(invalid), "{verdicts:?}");
    assert_eq!(rig.members[0].cipher.current_epoch(), Some(0));
}

#[test]
fn an_approval_that_is_sealed_for_another_device_does_not_open() {
    let mut rig = Rig::group_of(6, 2);
    let (joiner, bystander) = (rig.add_device(), rig.add_device());
    let code = e2e_harness::ids::InviteCode::parse("abcd1234").unwrap();
    let invite = rig.members[0].cipher.invite(&code).unwrap();
    rig.deliver_ok(0, GroupChange::Invite { code }, invite);
    let info = rig.info(joiner);
    rig.deliver_ok(
        joiner,
        GroupChange::JoinRequest {
            code,
            introduction: info.introduction.clone(),
        },
        Material::empty(),
    );
    let request = Seq(rig.next.0 - 1);

    // The approver seals the key for the bystander and approves the joiner.
    let other = rig.info(bystander);
    let material = rig.members[0]
        .cipher
        .approve_join(rig.state.roster(), &other)
        .unwrap();
    let verdicts = verdicts_of(&mut rig, 0, GroupChange::Approve { request }, material);
    let joiner_verdict = verdicts.last().unwrap();
    assert!(invalid(joiner_verdict), "{verdicts:?}");
    assert_eq!(rig.members[joiner].cipher.current_epoch(), None);
}

#[test]
fn an_event_that_carries_no_keys_carries_no_material() {
    let mut rig = Rig::group_of(7, 3);
    let code = e2e_harness::ids::InviteCode::parse("zzzz9999").unwrap();
    let verdicts = verdicts_of(
        &mut rig,
        0,
        GroupChange::Invite { code },
        Material::from(&b"smuggled"[..]),
    );
    assert_eq!(verdicts.len(), 3);
    assert!(verdicts.iter().all(invalid), "{verdicts:?}");
}

#[test]
fn a_join_request_with_no_public_key_is_ignored_by_every_member() {
    let mut rig = Rig::group_of(8, 3);
    let stranger = rig.add_device();
    let code = e2e_harness::ids::InviteCode::parse("qwer1234").unwrap();
    let invite = rig.members[0].cipher.invite(&code).unwrap();
    rig.deliver_ok(0, GroupChange::Invite { code }, invite);
    let verdicts = verdicts_of(
        &mut rig,
        stranger,
        GroupChange::JoinRequest {
            code,
            introduction: Material::from(&b"not a key"[..]),
        },
        Material::empty(),
    );
    assert_eq!(verdicts.len(), 3, "the three members judge it");
    assert!(verdicts.iter().all(invalid), "{verdicts:?}");
}

#[test]
fn a_joiner_gets_the_current_epoch_and_none_of_the_past() {
    let mut rig = Rig::group_of(9, 3);
    let leaver = rig.user(2);
    rig.remove(0, leaver);
    let joiner = rig.add_device();
    rig.admit(0, joiner);

    assert_eq!(rig.members[0].cipher.epochs(), [0, 1]);
    assert_eq!(rig.members[joiner].cipher.epochs(), [1]);
}

#[test]
fn a_new_device_gets_every_epoch_its_sibling_holds() {
    let mut rig = Rig::group_of(10, 3);
    let leaver = rig.user(2);
    rig.remove(0, leaver);
    let sibling = rig.add_device();
    rig.add_device_to(1, sibling);

    assert_eq!(rig.members[1].cipher.epochs(), [0, 1]);
    assert_eq!(rig.members[sibling].cipher.epochs(), [0, 1]);
}

#[test]
fn a_device_that_was_removed_and_joins_again_holds_the_old_and_the_new_epochs() {
    let mut rig = Rig::group_of(11, 3);
    let user = rig.user(2);
    rig.remove(0, user);
    let other = rig.user(1);
    rig.remove(0, other); // epoch 2: device 2 holds epoch 0 only
    assert_eq!(rig.members[2].cipher.epochs(), [0]);

    rig.admit(0, 2);
    assert_eq!(rig.members[2].cipher.epochs(), [0, 2]);
    assert_eq!(rig.members[0].cipher.current_epoch(), Some(2));
}

#[test]
fn a_ping_ended_by_a_removal_is_read_by_the_removed_and_not_written_by_the_rest() {
    let mut rig = Rig::group_of(12, 3);
    let ping = PingId(1);
    let head = rig.next;
    let open = rig.members[0].cipher.start_ping(ping, head).unwrap();
    for member in &mut rig.members {
        member.cipher.open_ping(ping, head, &open).unwrap();
    }
    let early = rig.members[1].cipher.encrypt_ping(ping, b"before").unwrap();
    // Member 1 has not read the removal yet and writes with the old key.
    let late = rig.members[1].cipher.encrypt_ping(ping, b"late").unwrap();

    let leaver = rig.user(2);
    rig.remove(0, leaver);

    // The removed device reads both, and the late one too: D10 accepts it.
    for ciphertext in [&early, &late] {
        assert!(
            rig.members[2]
                .cipher
                .decrypt_ping(ping, Seq(1), ciphertext)
                .is_ok()
        );
    }
    // After the removal the devices that stay refuse to write to the ended ping.
    let refused = rig.members[1].cipher.encrypt_ping(ping, b"again");
    assert!(matches!(refused, Err(CipherError::State(_))), "{refused:?}");
    // A ping that starts now cannot be opened by the removed device.
    let head = rig.next;
    let fresh = rig.members[0].cipher.start_ping(PingId(2), head).unwrap();
    rig.members[1]
        .cipher
        .open_ping(PingId(2), head, &fresh)
        .unwrap();
    let denied = rig.members[2].cipher.open_ping(PingId(2), head, &fresh);
    assert!(matches!(denied, Err(CipherError::NoKey(_))), "{denied:?}");
}

#[test]
fn the_exported_state_restores_a_cipher_that_reads_what_the_old_one_read() {
    let mut rig = Rig::group_of(13, 3);
    let ping = PingId(1);
    let head = rig.next;
    let open = rig.members[0].cipher.start_ping(ping, head).unwrap();
    for member in &mut rig.members {
        member.cipher.open_ping(ping, head, &open).unwrap();
    }
    let sealed = rig.members[0].cipher.encrypt_ping(ping, b"hello").unwrap();

    let state = rig.members[1].cipher.export();
    let identity = rig.members[1].identity.clone();
    let mut restored = PlainKey::restore(
        &identity,
        e2e_harness::rng::SimRng::new(e2e_harness::Seed(99)),
        &state,
    )
    .unwrap();
    assert_eq!(restored.epochs(), rig.members[1].cipher.epochs());
    assert_eq!(
        restored.decrypt_ping(ping, Seq(1), &sealed).unwrap(),
        b"hello"
    );
    assert_eq!(
        restored.introduction(),
        rig.members[1].cipher.introduction()
    );

    let bad = PlainKey::restore(
        &identity,
        e2e_harness::rng::SimRng::new(e2e_harness::Seed(1)),
        &state[..10],
    );
    assert!(matches!(bad, Err(CipherError::Invalid(_))));
    let mut other_version = state.clone();
    other_version[0] = 9;
    let bad = PlainKey::restore(
        &identity,
        e2e_harness::rng::SimRng::new(e2e_harness::Seed(1)),
        &other_version,
    );
    assert!(matches!(bad, Err(CipherError::Invalid(_))));
}

use e2e_plain_key::PlainKey;
