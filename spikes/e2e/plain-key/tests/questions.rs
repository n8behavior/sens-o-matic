//! The eight questions of "What the spike must answer" (#62) for the plain key,
//! as tests on the simulator of the harness. The scenarios of the harness
//! (`tests/scenarios.rs`) answer part of questions 2, 3, 4 and 8; the tests here
//! add the cases that the issue names: an author that writes late, a device that
//! is offline across many removals, a replay five years later, a leaver while
//! every other member is offline, and a server that holds back the removal.
//!
//! The answers, with numbers, are in the comment `## Answers` of #68.

use std::collections::BTreeMap;

use e2e_harness::{
    clock::Millis,
    device::{Act, Ignored, Intent},
    event::Envelope,
    evil::{Attack, EvilLog},
    ids::{LogId, Seq},
    keys::EventHash,
    log::MemoryLog,
    payload::{GroupChange, GroupPayload},
    rng::Seed,
    rules::Refusal,
    sim::{DeviceIdx, Offline, World},
};
use e2e_plain_key::PlainKey;

type Sim = World<PlainKey>;

fn reads(world: &Sim, device: DeviceIdx, ping: e2e_harness::ids::PingId) -> usize {
    world
        .device(device)
        .ping(ping)
        .map_or(0, |view| view.readable().len())
}

fn said(world: &Sim, device: DeviceIdx, ping: e2e_harness::ids::PingId, text: &[u8]) -> bool {
    world
        .device(device)
        .ping(ping)
        .is_some_and(|view| view.readable().values().any(|plain| plain == text))
}

fn all_agree(world: &Sim) {
    let digests: Vec<EventHash> = world.devices().map(|(_, d)| d.group_digest()).collect();
    assert!(
        digests.windows(2).all(|pair| pair[0] == pair[1]),
        "the devices ended in different group states"
    );
    for (idx, device) in world.devices() {
        assert!(device.alarms().is_empty(), "{idx}: {:?}", device.alarms());
    }
}

// ---- 1. The key of a ping event ----

/// A member removed between event n and event m of a ping, and an author that
/// writes late with the old key.
///
/// Result: the removed member reads the late event (1 of 1), because the key of
/// the ping is the same for everyone who held it; the devices that have read the
/// removal refuse to write more to that ping (D10: start a new one).
#[test]
fn q1_an_author_that_writes_late_with_the_old_key_is_read_by_the_leaver() {
    let mut world = Sim::new(Seed::default());
    let a = world.found_group();
    let (b, c, d) = (world.admit(a), world.admit(a), world.admit(a));
    let leaver = world.device(d).user().expect("d is a member");
    let ping = world.start_ping(a);
    world.post(b, ping, b"before");
    world.run_until_idle();

    // `a` removes the leaver; `c` has not read it and writes with the old key.
    world.queue(a, Intent::Remove(leaver));
    assert!(matches!(world.act(a), Act::Written(_)));
    world.post(c, ping, b"late");
    assert!(matches!(world.act(c), Act::Written(_)));
    world.run_until_idle();
    all_agree(&world);

    assert!(said(&world, d, ping, b"before"));
    assert!(
        said(&world, d, ping, b"late"),
        "the removed member reads the event that was written late with the old key"
    );
    for member in [a, b, c] {
        assert_eq!(world.device(member).ping_ended(ping), Some(true));
        assert_eq!(reads(&world, member, ping), 2);
    }

    // Having read the removal, `c` writes no more to the ended ping.
    world.post(c, ping, b"again");
    world.run_until_idle();
    assert!(
        world
            .skips_of(c)
            .iter()
            .any(|why| why.contains("a removal ended it")),
        "{:?}",
        world.skips_of(c)
    );

    // A ping that starts after the removal is sealed under the new epoch.
    let fresh = world.start_ping(a);
    world.post(b, fresh, b"new group");
    world.run_until_idle();
    assert_eq!(reads(&world, c, fresh), 1);
    assert_eq!(reads(&world, d, fresh), 0, "the leaver reads nothing of it");
}

/// A device offline across 5 removals reads a ping from before them and a ping
/// from after them, whose keys belong to epochs that are far apart.
///
/// Result: yes. It holds epoch 0, and it gets each of epochs 1 to 5 from its own
/// wrap in the rekey of each removal, in log order; it needs no old epoch beyond
/// those it already held, and no event of a ping names an epoch except the first.
#[test]
fn q1_a_device_offline_across_five_removals_reads_both_pings() {
    let mut world = Sim::new(Seed(11));
    let a = world.found_group();
    let others: Vec<_> = (0..6).map(|_| world.admit(a)).collect();
    let sleeper = world.admit(a);
    let old = world.start_ping(a);
    world.post(a, old, b"one");
    world.run_until_idle();

    world.set_offline(
        sleeper,
        Offline::Until(world.now() + Millis::from_hours(72)),
    );
    for other in &others[..5] {
        let user = world.device(*other).user().expect("a member");
        world.queue(a, Intent::Remove(user));
    }
    world.run_until_idle();
    let fresh = world.start_ping(a);
    world.post(a, fresh, b"two");
    world.post(others[5], fresh, b"three");
    world.run_until_idle();

    world.run_for(Millis::from_hours(73));
    world.run_until_idle();
    all_agree(&world);
    assert_eq!(reads(&world, sleeper, old), 1);
    assert_eq!(reads(&world, sleeper, fresh), 2);
    assert_eq!(
        world.device(sleeper).cipher().epochs(),
        [0, 1, 2, 3, 4, 5],
        "one key for each epoch, each from its own wrap"
    );
}

// ---- 2. Joining with a code ----

/// An uninvited device asks to join with a code that was never issued, and a
/// member forges the approval: every device ignores both alike, and the stranger
/// reads nothing.
#[test]
fn q2_an_uninvited_joiner_is_ignored_alike_by_every_device() {
    let mut world = Sim::new(Seed(21));
    let a = world.found_group();
    let (b, c) = (world.admit(a), world.admit(a));
    let ping = world.start_ping(a);
    world.post(b, ping, b"members only");
    world.run_until_idle();

    let stranger = world.add_device();
    let code = world.new_code();
    world.queue(stranger, Intent::Request(code));
    world.run_until_idle();
    let request = Seq(world.device(a).head(&LogId::Group(world.group())).0 - 1);
    world.queue(c, Intent::Forge(GroupChange::Approve { request }));
    world.post(stranger, ping, b"let me in");
    world.run_until_idle();
    all_agree(&world);

    for member in [a, b, c] {
        let ignored = world.device(member).ignored();
        assert!(
            ignored.contains(&(request, Ignored::Rule(Refusal::UnknownCode(code)))),
            "{member}: {ignored:?}"
        );
        assert!(
            ignored
                .iter()
                .any(|(_, why)| *why == Ignored::Rule(Refusal::UnknownRequest(request))),
            "{member}: {ignored:?}"
        );
    }
    assert!(!world.device(stranger).is_member());
    assert_eq!(reads(&world, stranger, ping), 0);
}

/// A joiner that a member let in and the others do not want is removed: every
/// device applies the removal at the same position and ends in one state, and
/// the removed joiner reads nothing of the next ping.
#[test]
fn q2_a_joiner_that_the_group_removes_is_removed_alike_on_replay() {
    let mut world = Sim::new(Seed(22));
    let a = world.found_group();
    let (b, c) = (world.admit(a), world.admit(a));
    let intruder = world.admit(c);
    let user = world.device(intruder).user().expect("the intruder is in");
    world.queue(b, Intent::Remove(user));
    world.run_until_idle();
    all_agree(&world);

    let ping = world.start_ping(a);
    world.post(c, ping, b"after the removal");
    world.run_until_idle();
    assert_eq!(reads(&world, a, ping), 1);
    assert_eq!(reads(&world, intruder, ping), 0);
}

// ---- 3. The order of events ----

/// Two members remove someone at once: the conditional append refuses the second,
/// which makes its rekey again from the new epoch.
///
/// Result: the survivors end at epoch 2, each removed user holds the keys of the
/// epochs they were in.
#[test]
fn q3_the_loser_of_a_race_rekeys_again_from_the_new_epoch() {
    let mut world = Sim::new(Seed(31));
    let a = world.found_group();
    let b = world.admit(a);
    let (x, y) = (world.admit(a), world.admit(b));
    let (user_x, user_y) = (
        world.device(x).user().expect("x is in"),
        world.device(y).user().expect("y is in"),
    );

    world.queue(a, Intent::Remove(user_x));
    world.queue(b, Intent::Remove(user_y));
    world.sync(a);
    world.sync(b);
    assert!(matches!(world.act(a), Act::Written(_)));
    assert!(matches!(world.act(b), Act::Conflict { .. }));
    world.run_until_idle();
    all_agree(&world);

    for survivor in [a, b] {
        assert_eq!(world.device(survivor).cipher().current_epoch(), Some(2));
    }
    assert_eq!(world.device(x).cipher().epochs(), [0]);
    assert_eq!(
        world.device(y).cipher().epochs(),
        [0, 1],
        "y stayed in the first removal and left in the second"
    );
}

/// The server holds the removal back from one member, who goes on writing with
/// the old key: the removed member reads it. The protocol does nothing against
/// this; the hash chain of the harness is what would catch it.
#[test]
fn q3_a_server_that_holds_back_the_removal_lets_the_leaver_read_on() {
    let mut world =
        World::<PlainKey, EvilLog>::with_log(Seed(32), EvilLog::new(MemoryLog::default()));
    let a = world.found_group();
    let (b, victim, d) = (world.admit(a), world.admit(a), world.admit(a));
    let leaver = world.device(d).user().expect("d is in");
    let ping = world.start_ping(a);
    world.post(b, ping, b"before");
    world.run_until_idle();

    let log = LogId::Group(world.group());
    let from = world.device(a).head(&log);
    let victim_key = world.device(victim).key();
    world.log_mut().arm(Attack::HoldBack {
        log,
        victim: victim_key,
        from,
    });
    world.queue(a, Intent::Remove(leaver));
    world.run_until_idle();
    world.post(victim, ping, b"still the old key");
    world.run_until_idle();

    let read = |idx: DeviceIdx, text: &[u8]| {
        world
            .device(idx)
            .ping(ping)
            .is_some_and(|view| view.readable().values().any(|plain| plain == text))
    };
    assert!(
        read(d, b"still the old key"),
        "the leaver reads the victim's event"
    );
    assert_eq!(world.device(victim).ping_ended(ping), Some(false));
    assert_eq!(world.device(a).ping_ended(ping), Some(true));
}

// ---- 4. Cold start and a new device ----

/// A user whose device lost its state gets a new device from a sibling. The new
/// device reads the pings of every epoch its sibling held, from the logs, with no
/// plaintext handed over; a joiner after the removal reads only the ping of the
/// current epoch.
#[test]
fn q4_a_new_device_from_a_sibling_reads_every_ping_its_sibling_could() {
    let mut world = Sim::new(Seed(41));
    let a = world.found_group();
    let u1 = world.admit(a);
    let u2 = world.add_device_to(u1);
    let w = world.admit(a);
    let user_w = world.device(w).user().expect("w is in");

    let first = world.start_ping(a);
    world.post(a, first, b"epoch zero");
    world.run_until_idle();
    world.queue(a, Intent::Remove(user_w));
    world.run_until_idle();
    let second = world.start_ping(a);
    world.post(a, second, b"epoch one");
    world.run_until_idle();

    // u2 is lost; u1 vouches for a replacement.
    let u3 = world.add_device_to(u1);
    let joiner = world.admit(a);
    world.post(a, second, b"after both joined");
    world.run_until_idle();
    all_agree(&world);

    assert_eq!(world.device(u2).cipher().epochs(), [0, 1]);
    assert_eq!(world.device(u3).cipher().epochs(), [0, 1]);
    assert_eq!(reads(&world, u3, first), reads(&world, a, first));
    assert_eq!(reads(&world, u3, second), reads(&world, a, second));

    assert_eq!(world.device(joiner).cipher().epochs(), [1]);
    assert_eq!(reads(&world, joiner, first), 0, "none of the past epoch");
    assert_eq!(
        reads(&world, joiner, second),
        reads(&world, a, second),
        "the whole ping of the current epoch, also what came before the join"
    );
}

// ---- 5. Determinism ----

struct Story {
    payloads: BTreeMap<LogId, Vec<Vec<u8>>>,
    group_states: Vec<EventHash>,
    plaintexts: Vec<Vec<Vec<u8>>>,
    first_stamp: Millis,
}

/// The same story, begun `years` after the start of the clock.
fn story(years: u64) -> Story {
    let mut world = Sim::new(Seed(51));
    for _ in 0..years {
        world.run_for(Millis::from_hours(24 * 365));
    }
    let a = world.found_group();
    let (b, c) = (world.admit(a), world.admit(a));
    let ping = world.start_ping(a);
    world.post(b, ping, b"one");
    world.post(c, ping, b"two");
    world.run_until_idle();
    let user_c = world.device(c).user().expect("c is in");
    world.queue(a, Intent::Remove(user_c));
    world.run_until_idle();
    let next = world.start_ping(a);
    world.post(b, next, b"three");
    world.run_until_idle();
    all_agree(&world);

    let entries = world.log().snapshot();
    let first_stamp = entries
        .values()
        .flatten()
        .map(|entry| Envelope::from_bytes(entry).expect("an event").timestamp())
        .min()
        .expect("the log has events");
    let payloads = entries
        .into_iter()
        .map(|(log, entries)| {
            let payloads = entries
                .iter()
                .map(|entry| {
                    Envelope::from_bytes(entry)
                        .expect("an event")
                        .body()
                        .payload
                        .clone()
                })
                .collect();
            (log, payloads)
        })
        .collect();
    Story {
        payloads,
        group_states: world.devices().map(|(_, d)| d.group_digest()).collect(),
        plaintexts: world
            .devices()
            .map(|(_, d)| {
                [ping, next]
                    .iter()
                    .flat_map(|p| d.ping(*p).into_iter())
                    .flat_map(|view| view.readable().values().cloned())
                    .collect()
            })
            .collect(),
        first_stamp,
    }
}

/// The same story with the clock five years later: the same ciphertexts, the same
/// group states, the same plaintexts. Nothing in the cipher reads a clock, so
/// nothing can expire: an Add of five years ago is still an Add.
#[test]
fn q5_a_replay_with_the_clock_five_years_later_gives_the_same_bytes() {
    let now = story(0);
    let later = story(5);
    assert!(later.first_stamp >= Millis::from_hours(24 * 365 * 5));
    assert!(now.first_stamp < Millis::from_hours(24));
    assert_eq!(now.payloads, later.payloads, "payloads differ");
    assert_eq!(now.group_states, later.group_states);
    assert_eq!(now.plaintexts, later.plaintexts);
}

// ---- 7. A leaver while every other member is offline ----

fn stamp_of(world: &Sim, wanted: impl Fn(&GroupChange) -> bool) -> Millis {
    world
        .log()
        .entries(&LogId::Group(world.group()))
        .iter()
        .map(|entry| Envelope::from_bytes(entry).expect("an event"))
        .find(|envelope| {
            GroupPayload::decode(&envelope.body().payload)
                .is_ok_and(|payload| wanted(&payload.change))
        })
        .expect("the log holds the event")
        .timestamp()
}

/// The leaver says `Leave` while every other member is offline for 6 hours. No
/// key changes until a member's device comes back, reads the `Leave` and writes
/// the `Remove`: that member writes the new key, at once, and the leaver reads
/// the old ping for as long as the members are away.
#[test]
fn q7_a_leaver_keeps_reading_until_the_first_other_member_comes_back() {
    let mut world = Sim::new(Seed(71));
    let a = world.found_group();
    let (b, c, leaver_device) = (world.admit(a), world.admit(a), world.admit(a));
    let leaver = world.device(leaver_device).user().expect("a member");
    let ping = world.start_ping(a);
    world.post(a, ping, b"hello");
    world.run_until_idle();

    let back = world.now() + Millis::from_hours(6);
    for member in [a, b, c] {
        world.set_offline(member, Offline::Until(back));
    }
    world.queue(leaver_device, Intent::Leave);
    world.run_until_idle();
    world.post(leaver_device, ping, b"is anyone there?");
    world.run_until_idle();

    // The members return; the first whose device has read the `Leave` removes.
    world.run_for(Millis::from_hours(6));
    world.run_until(|w| {
        [a, b, c]
            .iter()
            .any(|m| w.device(*m).state().is_leaving(leaver))
    });
    let remover = [a, b, c]
        .into_iter()
        .find(|m| world.device(*m).state().is_leaving(leaver))
        .expect("a member read the Leave");
    world.queue(remover, Intent::Remove(leaver));
    world.run_until_idle();
    all_agree(&world);

    let left = stamp_of(&world, |change| matches!(change, GroupChange::Leave));
    let removed = stamp_of(&world, |change| {
        matches!(change, GroupChange::Remove { .. })
    });
    let window = removed - left;
    println!("the leaver kept the old key for {window} after the Leave");
    assert!(window >= Millis::from_hours(6));
    assert!(window < Millis::from_hours(7));

    // After the removal, a new ping is closed to the leaver.
    let fresh = world.start_ping(a);
    world.post(b, fresh, b"new group");
    world.run_until_idle();
    assert_eq!(reads(&world, leaver_device, fresh), 0);
    assert_eq!(reads(&world, b, fresh), 1);
}
