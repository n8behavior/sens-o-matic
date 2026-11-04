//! The numbers of the issue: bytes and time of create, add, remove and a Commit,
//! for 90 devices (30 members with 3 devices, D10) and for 200.
//!
//! ```text
//! cargo test --release --manifest-path spikes/e2e/openmls/Cargo.toml -- --ignored --nocapture
//! ```
//!
//! The test is `#[ignore]` because it reads the wall clock, which `clippy.toml`
//! refuses in `src/` and everywhere else; it is the only measure in the crate.
//! A group is built the way the app builds it: one user at a time by the flow of
//! D11, then two more devices for each user. Only four devices are live, which
//! keeps a group of 200 cheap to build; the others hold a key package and a leaf.

#![allow(clippy::disallowed_methods)]

#[path = "support/mod.rs"]
mod support;

use std::time::{Duration, Instant};

use e2e_harness::{
    cipher::GroupCipher,
    ids::{InviteCode, Material, PingId, Seq},
    payload::GroupChange,
    rng::{Seed, SimRng},
    roster::DeviceInfo,
};
use e2e_openmls::{
    identity::{DeviceSigner, credential_of},
    provider::{CIPHERSUITE, SimProvider},
    wire::Bundle,
};
use openmls::prelude::{
    KeyPackage, LeafNodeParameters, MlsGroup, MlsGroupCreateConfig, MlsGroupJoinConfig,
    MlsMessageBodyIn, MlsMessageIn, StagedWelcome,
};
use support::Group;
use tls_codec::{Deserialize, Serialize};

const LIVE: usize = 4;

fn ms(duration: Duration) -> String {
    format!("{:.1} ms", duration.as_secs_f64() * 1000.0)
}

fn kb(bytes: usize) -> String {
    format!("{bytes} B ({:.1} KiB)", bytes as f64 / 1024.0)
}

/// Apply an event on every live device that is a member after it, and say how
/// long the author and a receiver took.
fn timed_push(
    group: &mut Group,
    author: usize,
    change: GroupChange,
    material: Material,
) -> (Duration, Duration) {
    let seq = Seq(group.next);
    let key = group.devices[author].key;
    group
        .state
        .apply(seq, &key, &change)
        .expect("the rules accept");
    group.next += 1;
    let (mut own, mut other) = (Duration::ZERO, Duration::ZERO);
    for (idx, device) in group.devices.iter_mut().enumerate() {
        if device.live && group.state.roster().contains(&device.key) {
            let event = e2e_harness::cipher::GroupEvent {
                seq,
                author: &key,
                change: &change,
                material: &material,
                roster: group.state.roster(),
            };
            let started = Instant::now();
            device.cipher.apply_group_event(&event).expect("applied");
            let took = started.elapsed();
            if idx == author {
                own = took;
            } else {
                other = other.max(took);
            }
        }
    }
    (own, other)
}

/// A group of `devices` devices: users of three devices each, device 0 first.
/// A user's two extra devices are added right after the user joins, while its
/// first device is current. Returns the group, the time, and the first device of
/// each user.
fn build(devices: usize) -> (Group, Duration, Vec<usize>) {
    let started = Instant::now();
    let mut group = Group::new(devices as u64);
    let mut owners = vec![0];
    for n in 0..devices / 3 {
        let owner = if n == 0 { 0 } else { group.join(0, true) };
        for _ in 0..2 {
            group.add_device(owner, false);
        }
        if n >= LIVE {
            group.devices[owner].live = false;
        }
        if n > 0 {
            owners.push(owner);
        }
    }
    while group.devices.len() < devices {
        group.add_device(0, false);
    }
    (group, started.elapsed(), owners)
}

/// One run of an operation: the time to make it, to apply it by its author and
/// by a member, and by the joiner when there is one; the bytes of its parts.
#[derive(Clone, Copy, Default)]
struct Sample {
    make: Duration,
    own: Duration,
    other: Duration,
    joiner: Duration,
    commit: usize,
    welcome: usize,
}

fn median<T: Ord + Copy>(samples: &[Sample], key: impl Fn(&Sample) -> T) -> T {
    let mut values: Vec<T> = samples.iter().map(key).collect();
    values.sort();
    values[values.len() / 2]
}

/// A new user joins by the flow of D11 and device 0 approves. The joiner is live
/// for its own turn and not after.
fn add_user(group: &mut Group) -> Sample {
    let joiner = group.spawn(false);
    let code = InviteCode::generate(&mut group.rng);
    let invite = group.devices[0].cipher.invite(&code).expect("invite");
    group.push_ok(0, GroupChange::Invite { code }, invite);
    let introduction = group.devices[joiner].cipher.introduction();
    group.push_ok(
        joiner,
        GroupChange::JoinRequest {
            code,
            introduction: introduction.clone(),
        },
        Material::empty(),
    );
    let request = Seq(group.next - 1);
    let info = DeviceInfo {
        key: group.devices[joiner].key,
        introduction,
    };
    let roster = group.state.roster().clone();
    let started = Instant::now();
    let material = group.devices[0]
        .cipher
        .approve_join(&roster, &info)
        .expect("approve");
    let make = started.elapsed();
    let bundle = Bundle::decode(material.as_slice()).expect("a bundle");
    let (commit, welcome) = (
        bundle.commit.as_slice().len(),
        bundle.welcome.as_slice().len(),
    );
    let (own, other) = timed_push(group, 0, GroupChange::Approve { request }, material.clone());

    let roster = group.state.roster().clone();
    let author = group.devices[0].key;
    let started = Instant::now();
    group.devices[joiner]
        .cipher
        .apply_group_event(&e2e_harness::cipher::GroupEvent {
            seq: Seq(group.next - 1),
            author: &author,
            change: &GroupChange::Approve { request },
            material: &material,
            roster: &roster,
        })
        .expect("the joiner reads the Welcome");
    Sample {
        make,
        own,
        other,
        joiner: started.elapsed(),
        commit,
        welcome,
    }
}

/// Device 0 removes the user of `victim`.
fn remove_user(group: &mut Group, victim: usize) -> Sample {
    let user = group.user_of(victim);
    let roster = group.state.roster().clone();
    let started = Instant::now();
    let material = group.devices[0]
        .cipher
        .remove_member(&roster, user)
        .expect("remove");
    let make = started.elapsed();
    let commit = Bundle::decode(material.as_slice())
        .expect("a bundle")
        .commit
        .as_slice()
        .len();
    let (own, other) = timed_push(group, 0, GroupChange::Remove { user }, material);
    Sample {
        make,
        own,
        other,
        commit,
        ..Sample::default()
    }
}

/// Every figure is the median of five runs (three for a removal), each on the
/// group as the run before left it.
fn measure(devices: usize) {
    let (mut group, build_time, owners) = build(devices);
    assert_eq!(group.state.roster().device_count(), devices);
    println!(
        "build the group, one user at a time, 4 live devices: {}",
        ms(build_time)
    );
    println!(
        "state of one device (device 0): {}",
        kb(group.devices[0].cipher.state_size())
    );

    let adds: Vec<Sample> = (0..5).map(|_| add_user(&mut group)).collect();
    println!(
        "add a user (approve a join request) to a group of {devices}: make {}, apply by the author {}, by a member {}, by the joiner (Welcome) {}",
        ms(median(&adds, |s| s.make)),
        ms(median(&adds, |s| s.own)),
        ms(median(&adds, |s| s.other)),
        ms(median(&adds, |s| s.joiner)),
    );
    println!(
        "  Commit {}, Welcome {} (with the ratchet tree), material {}",
        kb(median(&adds, |s| s.commit)),
        kb(median(&adds, |s| s.welcome)),
        kb(median(&adds, |s| s.commit + s.welcome)),
    );

    // Add a device of a known user: the Commit, the Welcome and the handover.
    let peer = owners[1];
    let device = group.spawn(false);
    let info = DeviceInfo {
        key: group.devices[device].key,
        introduction: group.devices[device].cipher.introduction(),
    };
    let roster = group.state.roster().clone();
    let material = group.devices[peer]
        .cipher
        .add_device(&roster, &info)
        .expect("add device");
    println!(
        "add a device of a known user: material {} (Commit, Welcome, handover)",
        kb(material.len())
    );
    timed_push(
        &mut group,
        peer,
        GroupChange::AddDevice {
            device: info.key,
            introduction: info.introduction,
        },
        material,
    );

    // A ping event: what the cipher adds to the plaintext.
    let ping = PingId(1);
    group.start_ping(0, ping);
    let overhead = group.devices[0]
        .cipher
        .encrypt_ping(ping, b"")
        .expect("encrypted")
        .len();
    println!(
        "a ping event adds {overhead} bytes to its plaintext (nonce, tag, epoch, hash of the Commit)"
    );

    // Remove users with three devices, the last users that joined.
    let removes: Vec<Sample> = owners
        .iter()
        .rev()
        .take(3)
        .map(|victim| remove_user(&mut group, *victim))
        .collect();
    println!(
        "remove a user with 3 devices: make {}, apply by the author {}, by a member {}; Commit {}",
        ms(median(&removes, |s| s.make)),
        ms(median(&removes, |s| s.own)),
        ms(median(&removes, |s| s.other)),
        kb(median(&removes, |s| s.commit)),
    );

    let (commit, welcome, add_all, join, update, update_make, update_receive) = raw(devices);
    println!(
        "create a group of {devices} in one Commit (= design A for one ping): make {}, Commit {}, Welcome {}, a device joins from the Welcome in {}",
        ms(add_all),
        kb(commit),
        kb(welcome),
        ms(join)
    );
    println!(
        "an update Commit of a group of {devices}: make {}, receive {}, {}",
        ms(update_make),
        ms(update_receive),
        kb(update)
    );
}

/// One Commit that Adds `n - 1` devices to a group of one, and a plain update
/// Commit of a group of `n`, with raw OpenMLS: the cost of making a whole group
/// in one step, which is also the cost of design A for each ping (one MLS group
/// for each ping, with every device of the group in it).
fn raw(n: usize) -> (usize, usize, Duration, Duration, usize, Duration, Duration) {
    let mut rng = SimRng::new(Seed(n as u64));
    let ids: Vec<_> = (0..n)
        .map(|_| e2e_harness::keys::Identity::generate(&mut rng))
        .collect();
    let (pa, pb, pk) = (
        SimProvider::new(rng.fork()),
        SimProvider::new(rng.fork()),
        SimProvider::new(rng.fork()),
    );
    let signer = |i: usize| DeviceSigner(ids[i].clone());
    let packages: Vec<KeyPackage> = (1..n)
        .map(|i| {
            let provider = if i == 1 { &pb } else { &pk };
            KeyPackage::builder()
                .key_package_lifetime(openmls::prelude::Lifetime::init(0, u64::MAX))
                .build(
                    CIPHERSUITE,
                    provider,
                    &signer(i),
                    credential_of(&ids[i].device_key()),
                )
                .expect("key package")
                .key_package()
                .clone()
        })
        .collect();
    let config = MlsGroupCreateConfig::builder()
        .ciphersuite(CIPHERSUITE)
        .use_ratchet_tree_extension(true)
        .lifetime(openmls::prelude::Lifetime::init(0, u64::MAX))
        .build();
    let started = Instant::now();
    let mut alice = MlsGroup::new_with_group_id(
        &pa,
        &signer(0),
        &config,
        openmls::prelude::GroupId::from_slice(b"ping"),
        credential_of(&ids[0].device_key()),
    )
    .expect("group");
    let create = started.elapsed();
    let started = Instant::now();
    let (commit, welcome, _) = alice
        .add_members(&pa, &signer(0), &packages)
        .expect("add all");
    alice.merge_pending_commit(&pa).expect("merged");
    let add_all = started.elapsed();
    let commit_bytes = commit.tls_serialize_detached().expect("serialized").len();
    let welcome_bytes = welcome.tls_serialize_detached().expect("serialized");
    let MlsMessageBodyIn::Welcome(welcome_in) = MlsMessageIn::tls_deserialize_exact(&welcome_bytes)
        .expect("read")
        .extract()
    else {
        panic!("not a Welcome");
    };
    let started = Instant::now();
    let mut bob =
        StagedWelcome::new_from_welcome(&pb, &MlsGroupJoinConfig::default(), welcome_in, None)
            .and_then(|staged| staged.into_group(&pb))
            .expect("bob joins");
    let join = started.elapsed();

    let started = Instant::now();
    let update = alice
        .self_update(&pa, &signer(0), LeafNodeParameters::default())
        .expect("update");
    alice.merge_pending_commit(&pa).expect("merged");
    let update_make = started.elapsed();
    let update_bytes = update
        .commit()
        .tls_serialize_detached()
        .expect("serialized");
    let message = MlsMessageIn::tls_deserialize_exact(&update_bytes)
        .expect("read")
        .try_into_protocol_message()
        .expect("protocol message");
    let started = Instant::now();
    let processed = bob.process_message(&pb, message).expect("processed");
    if let openmls::prelude::ProcessedMessageContent::StagedCommitMessage(staged) =
        processed.into_content()
    {
        bob.merge_staged_commit(&pb, *staged).expect("merged");
    }
    let update_receive = started.elapsed();
    let _ = (create, join);
    (
        commit_bytes,
        welcome_bytes.len(),
        add_all,
        join,
        update_bytes.len(),
        update_make,
        update_receive,
    )
}

#[test]
#[ignore = "reads the wall clock; run with --release -- --ignored --nocapture"]
fn numbers_for_90_and_200_devices() {
    for devices in [90, 200] {
        println!("\n== {devices} devices ==");
        measure(devices);
    }
}
