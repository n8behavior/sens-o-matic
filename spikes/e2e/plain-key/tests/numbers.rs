//! The numbers of the comment `## Numbers` of #68: the size of an application
//! event, of a join approval and of a rekey, and the time of a rekey, for a group
//! of 30 members with 3 devices each (D10 of #61) and for 200 devices.
//!
//! This test reads the wall clock, so it is not run by `cargo test`. Run it with
//!
//! ```text
//! cargo test --release -- --ignored --nocapture
//! ```
//!
//! The library reads no clock; only this test does, to time what the library does.

use std::time::{Duration, Instant};

use e2e_harness::{
    cipher::GroupCipher,
    device::{Act, Intent},
    event::Envelope,
    ids::{LogId, PingId},
    rng::Seed,
    sim::{DeviceIdx, World},
};
use e2e_plain_key::PlainKey;

type Sim = World<PlainKey>;

/// Run `work` and say how long it took on the wall clock.
#[allow(
    clippy::disallowed_methods,
    reason = "the one place that times the cipher; the library never reads a clock"
)]
fn timed<T>(work: impl FnOnce() -> T) -> (T, Duration) {
    let start = Instant::now();
    let value = work();
    (value, start.elapsed())
}

fn stored_len(world: &Sim, log: LogId, from_end: usize) -> (usize, usize) {
    let entries = world.log().entries(&log);
    let entry = &entries[entries.len() - 1 - from_end];
    let envelope = Envelope::from_bytes(entry).expect("an event");
    (entry.len(), envelope.body().payload.len())
}

/// One removal: the rekey that it wrote and the time it took.
struct Rekey {
    /// The devices that stay, so the wraps in the event.
    wraps: usize,
    stored: usize,
    payload: usize,
    /// The author: make, sign and submit.
    make: Duration,
    /// One other device: fetch, check and apply.
    apply: Duration,
}

struct Numbers {
    devices: usize,
    add_device: (usize, usize),
    group_events: usize,
    group_log_bytes: usize,
    app_event: (usize, usize),
    approval: (usize, usize),
    rekeys: Vec<Rekey>,
    state: usize,
}

fn measure(members: usize, per_member: usize) -> Numbers {
    let mut world = Sim::new(Seed(68));
    let founder = world.found_group();
    let mut users = vec![founder];
    while users.len() < members {
        users.push(world.admit(founder));
    }
    let group = LogId::Group(world.group());
    let mut add_device = (0, 0);
    for user in users.clone() {
        for _ in 1..per_member {
            world.add_device_to(user);
            add_device = stored_len(&world, group, 0);
        }
    }
    world.run_until_idle();
    assert_eq!(world.device_count(), members * per_member);

    let ping: PingId = world.start_ping(founder);
    world.post(users[1], ping, &[b'x'; 100]);
    world.run_until_idle();
    let app_event = stored_len(&world, LogId::Ping(world.group(), ping), 0);

    // Three removals, one member (all their devices) each time.
    let all: Vec<DeviceIdx> = world.devices().map(|(idx, _)| idx).collect();
    let mut rekeys = Vec::new();
    for round in 0..3 {
        let target = users[members - 1 - round];
        let user = world.device(target).user().expect("a member");
        let bystander = *all
            .iter()
            .find(|idx| world.device(**idx).user().is_some_and(|u| u != user) && **idx != founder)
            .expect("another device stays");
        let roster = world.device(founder).state().roster();
        let wraps = roster.device_count() - roster.devices_of(user).count();

        world.queue(founder, Intent::Remove(user));
        let (act, make) = timed(|| world.act(founder));
        assert!(matches!(act, Act::Written(_)), "{act:?}");
        let (applied, apply) = timed(|| world.sync(bystander));
        assert_eq!(
            applied, 1,
            "the bystander applies the rekey and nothing else"
        );
        world.run_until_idle();
        let (stored, payload) = stored_len(&world, group, 0);
        rekeys.push(Rekey {
            wraps,
            stored,
            payload,
            make,
            apply,
        });
    }

    // The approval of one more member: the last event of the group log.
    world.admit(users[1]);
    let approval = stored_len(&world, group, 0);

    Numbers {
        devices: members * per_member,
        add_device,
        group_events: world.log().entries(&group).len(),
        group_log_bytes: world.log().entries(&group).iter().map(Vec::len).sum(),
        app_event,
        approval,
        rekeys,
        state: world.device(founder).cipher().export().len(),
    }
}

fn show(label: &str, n: &Numbers) {
    println!("== {label}: {} devices", n.devices);
    println!(
        "group log at the end: {} events, {} bytes",
        n.group_events, n.group_log_bytes
    );
    println!(
        "application event (100 bytes of text): {} bytes stored, {} bytes of payload",
        n.app_event.0, n.app_event.1
    );
    println!(
        "join approval: {} bytes stored, {} bytes of payload",
        n.approval.0, n.approval.1
    );
    println!(
        "new device (AddDevice with one epoch): {} bytes stored, {} bytes of payload",
        n.add_device.0, n.add_device.1
    );
    for (round, rekey) in n.rekeys.iter().enumerate() {
        println!(
            "rekey {}: {} wraps, {} bytes stored, {} bytes of payload; author {:.1} ms, one other device {:.2} ms",
            round + 1,
            rekey.wraps,
            rekey.stored,
            rekey.payload,
            rekey.make.as_secs_f64() * 1000.0,
            rekey.apply.as_secs_f64() * 1000.0,
        );
    }
    println!("exported state of a device: {} bytes", n.state);
}

#[test]
#[ignore = "reads the wall clock: run with `cargo test --release -- --ignored --nocapture`"]
fn numbers_for_90_and_200_devices() {
    show("30 members with 3 devices", &measure(30, 3));
    show("200 devices (40 members with 5 devices)", &measure(40, 5));
}
