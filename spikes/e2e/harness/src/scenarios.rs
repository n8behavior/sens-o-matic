//! The scenarios: the same story for every candidate.
//!
//! Each scenario is a generic function over [`GroupCipher`] that builds a world
//! ([`crate::sim::World`]), plays a story with the real flows (invite, join
//! request, approval, ping), asserts what must hold for *any* protocol, and
//! returns a [`Report`] with the numbers and the findings that differ between
//! protocols. A candidate calls [`run_all`], or has one `#[test]` for each
//! scenario written by [`scenario_tests!`](crate::scenario_tests).
//!
//! What a scenario asserts is a property of the contract of
//! [`crate::cipher`], never of one protocol. What it only reports is where the
//! protocols may differ: how much a joiner can read of the past, whether a
//! leaver reads the old ping.
//!
//! | Scenario | Story |
//! |---|---|
//! | [`thirty_members_three_devices`] | 30 members with 3 devices each (D10) create a group, start a ping, every member writes, every device reads all; in rough weather |
//! | [`simultaneous_group_changes`] | two members remove someone at once, the conditional append refuses one, the loser retries, everyone ends in one state |
//! | [`offline_device_catches_up`] | a device is offline across more than 5 group changes, its cipher is restored from its export, and it reaches the same state |
//! | [`new_device_mid_ping`] | a member adds a device in the middle of a ping; the report says which events it reads |
//! | [`member_leaves_ping_ends`] | a member leaves, another removes them, the ping ends, a new ping starts; the leaver cannot read it |
//! | [`join_request_waits_hours`] | a join request waits hours of virtual time for a member's device |
//! | [`revoked_code_refused`] | a revoked code gets no one in |
//! | [`second_use_of_code_refused`] | the first request for a single-use code wins, the second is refused |
//! | [`unvouched_device_ignored`] | events of a device that no user vouches for change nothing, on every device |
//! | [`evil_server_drop`], [`evil_server_hold_back`], [`evil_server_reorder`], [`evil_server_fork`] | the four attacks of [`crate::evil`], in a busy and in a quiet group |
//!
//! [`same_seed_twice_gives_identical_logs`] is not a story but a property of
//! all of them: two runs with one seed store the same bytes.

use std::{collections::BTreeMap, fmt};

use crate::{
    cipher::GroupCipher,
    clock::Millis,
    device::{Act, Ignored, Intent},
    event::Envelope,
    evil::{self, Detection, Kind, Shape, Vigilance},
    ids::{LogId, Seq},
    keys::EventHash,
    log::{Log, MemoryLog},
    payload::GroupChange,
    rng::Seed,
    rules::{CodeState, Refusal},
    sim::{DeviceIdx, Faults, Offline, World},
};

/// What a scenario did, for a candidate to print and for the issue to quote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// The name of the scenario.
    pub scenario: &'static str,
    /// The name of the candidate ([`GroupCipher::NAME`]).
    pub cipher: &'static str,
    /// Events the server accepted.
    pub events: u64,
    /// Bytes the server accepted, over all events.
    pub bytes: u64,
    /// Devices in the world (in the largest world, if a scenario has several).
    pub devices: usize,
    /// Findings that differ between protocols, one sentence each.
    pub notes: Vec<String>,
    /// The entries stored in each log, byte for byte, when the scenario ran on a
    /// [`MemoryLog`]; empty otherwise. A test compares two runs with it.
    pub logs: BTreeMap<LogId, Vec<Vec<u8>>>,
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} / {}: {} events, {} bytes, {} devices",
            self.cipher, self.scenario, self.events, self.bytes, self.devices
        )?;
        self.notes
            .iter()
            .try_for_each(|note| write!(f, "\n  - {note}"))
    }
}

/// The seed of [`run_all`].
fn default_seed() -> Seed {
    Seed::default()
}

/// Run every scenario with the default seed, in the order of the table of the
/// module docs.
pub fn run_all<C: GroupCipher>() -> Vec<Report> {
    run_all_with::<C>(default_seed())
}

/// Run every scenario with `seed`.
pub fn run_all_with<C: GroupCipher>(seed: Seed) -> Vec<Report> {
    vec![
        thirty_members_three_devices::<C>(seed),
        simultaneous_group_changes::<C>(seed),
        offline_device_catches_up::<C>(seed),
        new_device_mid_ping::<C>(seed),
        member_leaves_ping_ends::<C>(seed),
        join_request_waits_hours::<C>(seed),
        revoked_code_refused::<C>(seed),
        second_use_of_code_refused::<C>(seed),
        unvouched_device_ignored::<C>(seed),
        evil_server_drop::<C>(seed),
        evil_server_hold_back::<C>(seed),
        evil_server_reorder::<C>(seed),
        evil_server_fork::<C>(seed),
    ]
}

/// One `#[test]` for each scenario, named as the scenario, and
/// `same_seed_twice_gives_identical_logs`, in a module `$name`, for the cipher
/// `$cipher`.
///
/// ```
/// use e2e_harness::fake::FakeCipher; // your cipher, in your crate
///
/// e2e_harness::scenario_tests!(fake, FakeCipher);
/// # fn main() {}
/// ```
#[macro_export]
macro_rules! scenario_tests {
    ($name:ident, $cipher:ty) => {
        mod $name {
            #[allow(unused_imports)]
            use super::*;

            $crate::scenario_tests!(@each $cipher;
                thirty_members_three_devices
                simultaneous_group_changes
                offline_device_catches_up
                new_device_mid_ping
                member_leaves_ping_ends
                join_request_waits_hours
                revoked_code_refused
                second_use_of_code_refused
                unvouched_device_ignored
                evil_server_drop
                evil_server_hold_back
                evil_server_reorder
                evil_server_fork
            );

            #[test]
            fn same_seed_twice_gives_identical_logs() {
                $crate::scenarios::same_seed_twice_gives_identical_logs::<$cipher>();
            }
        }
    };
    (@each $cipher:ty; $($scenario:ident)*) => {
        $(
            #[test]
            fn $scenario() {
                let report = $crate::scenarios::$scenario::<$cipher>($crate::Seed::default());
                println!("{report}");
            }
        )*
    };
}

// ---- Helpers ----

fn report<C: GroupCipher>(
    scenario: &'static str,
    world: &World<C, MemoryLog>,
    notes: Vec<String>,
) -> Report {
    let mut report = report_of(scenario, world, notes);
    report.logs = world.log().snapshot();
    report
}

fn report_of<C: GroupCipher, L: Log>(
    scenario: &'static str,
    world: &World<C, L>,
    notes: Vec<String>,
) -> Report {
    let (events, bytes) = world.server().totals();
    Report {
        scenario,
        cipher: C::NAME,
        events,
        bytes,
        devices: world.device_count(),
        notes,
        logs: BTreeMap::new(),
    }
}

/// Every device that is online ended in one group state, and none raised an alarm.
fn assert_converged<C: GroupCipher, L: Log>(world: &World<C, L>, scenario: &str) {
    let mut states: BTreeMap<EventHash, Vec<DeviceIdx>> = BTreeMap::new();
    for (idx, device) in world.devices() {
        assert!(
            device.alarms().is_empty(),
            "{scenario}: {idx} raised an alarm on an honest server: {:?} (see the module docs of e2e_harness::scenarios)",
            device.alarms()
        );
        states.entry(device.group_digest()).or_default().push(idx);
    }
    assert_eq!(
        states.len(),
        1,
        "{scenario}: the devices ended in {} different group states: {states:?} (see the module docs of e2e_harness::rules)",
        states.len()
    );
}

fn plaintext_of(member: usize, round: usize) -> Vec<u8> {
    format!("member {member} says {round}").into_bytes()
}

fn members_of<C: GroupCipher>(
    world: &mut World<C>,
    founder: DeviceIdx,
    count: usize,
) -> Vec<DeviceIdx> {
    let mut members = vec![founder];
    while members.len() < count {
        let approver = *world.rng().pick(&members).expect("the group has a founder");
        members.push(world.admit(approver));
    }
    members
}

fn ignored_with<C: GroupCipher, L: Log>(
    world: &World<C, L>,
    idx: DeviceIdx,
    refusal: Refusal,
) -> Vec<Seq> {
    world
        .device(idx)
        .ignored()
        .iter()
        .filter(|(_, ignored)| *ignored == Ignored::Rule(refusal))
        .map(|(seq, _)| *seq)
        .collect()
}

// ---- Scenarios ----

/// 30 members with 3 devices each (D10 of #61) create a group, start a ping and
/// every member writes an event; every device reads all.
///
/// The weather is rough ([`Faults::rough`]): a fifth of the wakes are lost, the
/// others are late, and one write in ten meets a rival. The group is built with
/// the real flows: 29 members join with a code and an approval by a random
/// member's device, then all 30 members add 2 devices at once, which makes the
/// conditional append refuse most of them at least once.
///
/// Asserts that the 90 devices end in one group state with no event ignored and
/// no alarm, and that each of them reads the event of every member and the same
/// events as every other.
pub fn thirty_members_three_devices<C: GroupCipher>(seed: Seed) -> Report {
    const MEMBERS: usize = 30;
    let mut world = World::<C>::new(seed);
    *world.faults_mut() = Faults::rough();
    let founder = world.found_group();
    let members = members_of(&mut world, founder, MEMBERS);

    let mut devices = members.clone();
    for member in &members {
        for _ in 0..2 {
            let device = world.add_device();
            let info = world.introduction_of(device);
            world.queue(*member, Intent::AddDevice(info));
            devices.push(device);
        }
    }
    world.run_until_idle();

    let roster = world.device(founder).state().roster();
    assert_eq!(
        roster.user_count(),
        MEMBERS,
        "thirty_members_three_devices: 30 users"
    );
    assert_eq!(
        roster.device_count(),
        MEMBERS * 3,
        "thirty_members_three_devices: 90 devices"
    );

    let ping = world.start_ping(founder);
    for (n, member) in members.iter().enumerate() {
        world.post(*member, ping, &plaintext_of(n, 0));
    }
    world.run_until_idle();

    assert_converged(&world, "thirty_members_three_devices");
    let reference = world
        .device(founder)
        .ping(ping)
        .expect("the founder read the ping");
    for (idx, device) in world.devices() {
        assert!(
            device.ignored().is_empty(),
            "thirty_members_three_devices: {idx} ignored {:?} (see the module docs of e2e_harness::rules)",
            device.ignored()
        );
        let view = device.ping(ping).expect("every device read the ping");
        assert!(
            view.is_opened(),
            "thirty_members_three_devices: {idx} could not open the ping"
        );
        assert!(
            view.unreadable().is_empty(),
            "thirty_members_three_devices: {idx} could not read {:?} (see the module docs of e2e_harness::cipher)",
            view.unreadable()
        );
        assert_eq!(
            view.digest(),
            reference.digest(),
            "thirty_members_three_devices: {idx} read other events than the founder"
        );
    }
    for (n, _) in members.iter().enumerate() {
        let wanted = plaintext_of(n, 0);
        assert!(
            reference.readable().values().any(|plain| *plain == wanted),
            "thirty_members_three_devices: the event of member {n} is missing"
        );
    }

    let stats = world.stats();
    let notes = vec![
        format!(
            "{} steps, {} conflicts, {} rivals, {} wakes lost, {} delivered, {} of virtual time",
            stats.steps,
            stats.conflicts,
            stats.rivals,
            stats.wakes_lost,
            stats.wakes_delivered,
            world.now()
        ),
        format!(
            "the group log has {} events and the ping log {}",
            world.device(founder).head(&LogId::Group(world.group())).0,
            world
                .device(founder)
                .head(&world.device(founder).ping_log(ping))
                .0
        ),
    ];
    report("thirty_members_three_devices", &world, notes)
}

/// Two members change the group at once: both read the same head and both remove
/// a user; the conditional append refuses one, the loser retries from the new
/// state, and all devices end in one state.
///
/// Asserts that exactly one append was refused for the race, that both users are
/// gone from every device, that the devices agree, and that the two members who
/// stay can read each other in a ping that starts after the changes.
pub fn simultaneous_group_changes<C: GroupCipher>(seed: Seed) -> Report {
    let mut world = World::<C>::new(seed);
    let (a, b) = {
        let a = world.found_group();
        (a, world.admit(a))
    };
    let (x, y) = (world.admit(a), world.admit(b));
    let (user_x, user_y) = (
        world.device(x).user().expect("x is a member"),
        world.device(y).user().expect("y is a member"),
    );

    let before = world.stats().conflicts;
    world.queue(a, Intent::Remove(user_x));
    world.queue(b, Intent::Remove(user_y));
    world.sync(a);
    world.sync(b);
    let first = world.act(a);
    let second = world.act(b);
    assert!(
        matches!(first, Act::Written(_)),
        "simultaneous_group_changes: the first append of the race was refused: {first:?}"
    );
    assert!(
        matches!(second, Act::Conflict { .. }),
        "simultaneous_group_changes: the second append of the race was not refused: {second:?} (see the module docs of e2e_harness::log)"
    );
    world.run_until_idle();
    assert_eq!(world.stats().conflicts - before, 1);

    assert_converged(&world, "simultaneous_group_changes");
    let roster = world.device(a).state().roster();
    assert!(!roster.contains_user(user_x) && !roster.contains_user(user_y));
    assert_eq!(roster.user_count(), 2);

    let ping = world.start_ping(a);
    world.post(b, ping, b"after the race");
    world.post(a, ping, b"yes");
    world.run_until_idle();
    for member in [a, b] {
        let view = world
            .device(member)
            .ping(ping)
            .expect("a member reads the ping");
        assert_eq!(
            view.readable().len(),
            2,
            "simultaneous_group_changes: {member} reads both events"
        );
    }
    let removed_read: usize = [x, y]
        .iter()
        .map(|idx| {
            world
                .device(*idx)
                .ping(ping)
                .map_or(0, |view| view.readable().len())
        })
        .sum();
    assert_eq!(
        removed_read, 0,
        "simultaneous_group_changes: a removed user reads the new ping"
    );

    let notes = vec![format!(
        "the loser retried and won at the next position; {} conflicts in the whole run",
        world.stats().conflicts
    )];
    report("simultaneous_group_changes", &world, notes)
}

/// A device is offline across more than 5 group changes, its cipher is closed and
/// opened from its export while it waits, and when it returns it reaches the same
/// state as the others and reads a ping that starts after it is back.
///
/// The group changes are 3 joins, 2 removals and a new device of another member.
/// The report says whether the device reads the ping that started while it was
/// offline.
pub fn offline_device_catches_up<C: GroupCipher>(seed: Seed) -> Report {
    let mut world = World::<C>::new(seed);
    let founder = world.found_group();
    let m1 = world.admit(founder);
    let m2 = world.admit(founder);
    let sleeper = world.add_device_to(m1);
    let group_log = LogId::Group(world.group());

    let head_before = world.device(founder).head(&group_log);
    world.set_offline(
        sleeper,
        Offline::Until(world.now() + Millis::from_hours(48)),
    );
    let joiners = [
        world.admit(founder),
        world.admit(founder),
        world.admit(founder),
    ];
    for joiner in &joiners[..2] {
        let user = world.device(*joiner).user().expect("a joiner is a member");
        world.queue(founder, Intent::Remove(user));
    }
    world.run_until_idle();
    let extra = world.add_device_to(m2);
    let changed = world.device(founder).head(&group_log).0 - head_before.0;
    assert!(
        changed >= 5,
        "offline_device_catches_up: the sleeper missed only {changed} events, fewer than 5"
    );
    assert!(
        world.device(sleeper).state().roster().user_count()
            < world.device(founder).state().roster().user_count()
    );

    let during = world.start_ping(founder);
    world.post(m2, during, b"while you slept");
    world.post(extra, during, b"hello");
    world.run_until_idle();

    world.restart_device(sleeper).unwrap_or_else(|error| {
        panic!("offline_device_catches_up: the cipher did not restore from its export: {error} (see the module docs of e2e_harness::cipher)")
    });
    world.run_for(Millis::from_hours(49));
    world.run_until_idle();
    assert_converged(&world, "offline_device_catches_up");

    let after = world.start_ping(founder);
    world.post(founder, after, b"welcome back");
    world.post(sleeper, after, b"I am here");
    world.run_until_idle();
    for member in [founder, sleeper] {
        let view = world.device(member).ping(after).expect("the ping is read");
        assert_eq!(
            view.readable().len(),
            2,
            "offline_device_catches_up: {member} should read both events of the ping after the return"
        );
    }

    let missed = world
        .device(sleeper)
        .ping(during)
        .map_or(0, |view| view.readable().len());
    let notes = vec![
        format!(
            "the sleeper missed {changed} events of the group log and caught up after 48 hours"
        ),
        format!("it reads {missed} of 2 events of the ping that started while it was offline"),
    ];
    report("offline_device_catches_up", &world, notes)
}

/// A member adds a device in the middle of a ping. The report states which events
/// the new device can read.
///
/// Asserts that the new device reads every event written after it was added, that
/// the old devices read everything, and that nobody raises an alarm.
pub fn new_device_mid_ping<C: GroupCipher>(seed: Seed) -> Report {
    const POSTS: usize = 2;
    let mut world = World::<C>::new(seed);
    let founder = world.found_group();
    let members = members_of(&mut world, founder, 4);
    let ping = world.start_ping(founder);
    let ping_log = world.device(founder).ping_log(ping);

    for round in 0..POSTS {
        for (n, member) in members.iter().enumerate() {
            world.post(*member, ping, &plaintext_of(n, round));
        }
    }
    world.run_until_idle();
    let before = world.device(founder).head(&ping_log);

    let newcomer = world.add_device_to(members[1]);
    for round in POSTS..2 * POSTS {
        for (n, member) in members.iter().enumerate() {
            world.post(*member, ping, &plaintext_of(n, round));
        }
    }
    world.run_until_idle();
    assert_converged(&world, "new_device_mid_ping");

    let total = world.device(founder).head(&ping_log);
    let view = world
        .device(newcomer)
        .ping(ping)
        .expect("the new device read the ping");
    let readable: Vec<Seq> = view.readable().keys().copied().collect();
    for seq in (before.0..total.0).map(Seq) {
        assert!(
            readable.contains(&seq),
            "new_device_mid_ping: the new device cannot read {seq}, written after it was added (see the module docs of e2e_harness::cipher)"
        );
    }
    let old = world
        .device(members[1])
        .ping(ping)
        .expect("the old device read the ping");
    assert_eq!(
        old.readable().len(),
        total.0 as usize - 1,
        "new_device_mid_ping: the old device reads everything"
    );

    let earlier = readable.iter().filter(|seq| **seq < before).count();
    let notes = vec![format!(
        "the new device reads {} of {} events: all {} written after it was added and {} of the {} before",
        readable.len(),
        total.0 - 1,
        total.0 - before.0,
        earlier,
        before.0 - 1,
    )];
    report("new_device_mid_ping", &world, notes)
}

/// A member leaves, another member removes them, the ping ends and a new ping
/// starts for the new group. The leaver cannot read the new ping and may still
/// read the old one, as D10 accepts.
///
/// The leaver cannot remove themselves: a forged `Remove` of their own user is
/// ignored by every device. The report says how long the leaver kept reading
/// between the `Leave` and the `Remove` (question 7 of #62), and how much of the
/// old ping they read.
pub fn member_leaves_ping_ends<C: GroupCipher>(seed: Seed) -> Report {
    let mut world = World::<C>::new(seed);
    let founder = world.found_group();
    let members = members_of(&mut world, founder, 4);
    let leaver = members[3];
    let leaver_user = world.device(leaver).user().expect("the leaver is a member");

    let old = world.start_ping(founder);
    for (n, member) in members.iter().enumerate() {
        world.post(*member, old, &plaintext_of(n, 0));
    }
    world.run_until_idle();

    let left_at = world.now();
    world.queue(leaver, Intent::Leave);
    world.run_until_idle();
    assert!(world.device(founder).state().is_leaving(leaver_user));

    // A user cannot remove themselves: every device ignores the forged event.
    world.queue(
        leaver,
        Intent::Forge(GroupChange::Remove { user: leaver_user }),
    );
    world.run_until_idle();
    assert_eq!(
        ignored_with(&world, founder, Refusal::SelfRemoval).len(),
        1,
        "member_leaves_ping_ends: the removal of oneself was not ignored (see rule 6 in the module docs of e2e_harness::rules)"
    );

    // Until another member writes the new key, the leaver keeps reading.
    world.post(founder, old, b"still here?");
    world.run_until_idle();
    let read_while_leaving = world
        .device(leaver)
        .ping(old)
        .map_or(0, |view| view.readable().len());
    world.run_for(Millis::from_mins(30));
    let removed_at = world.now();
    world.queue(founder, Intent::Remove(leaver_user));
    world.run_until_idle();
    assert_converged(&world, "member_leaves_ping_ends");
    for (idx, device) in world.devices() {
        assert_eq!(
            device.ping_ended(old),
            Some(true),
            "member_leaves_ping_ends: {idx} did not end the old ping after the removal (D10; see the module docs of e2e_harness::rules)"
        );
    }

    let new = world.start_ping(founder);
    for (n, member) in members.iter().take(3).enumerate() {
        world.post(*member, new, &plaintext_of(n, 1));
    }
    world.run_until_idle();
    assert_eq!(world.device(founder).ping_ended(new), Some(false));
    for member in &members[..3] {
        let view = world
            .device(*member)
            .ping(new)
            .expect("a member reads the new ping");
        assert_eq!(
            view.readable().len(),
            3,
            "member_leaves_ping_ends: {member} should read the three events of the new ping"
        );
    }
    let leaver_view = world.device(leaver).ping(new);
    let leaver_reads_new = leaver_view.map_or(0, |view| view.readable().len());
    assert_eq!(
        leaver_reads_new, 0,
        "member_leaves_ping_ends: the leaver reads {leaver_reads_new} events of the ping that started after the removal (see the module docs of e2e_harness::cipher)"
    );

    let old_total = world
        .device(founder)
        .ping(old)
        .map_or(0, |view| view.readable().len());
    let leaver_old = world
        .device(leaver)
        .ping(old)
        .map_or(0, |view| view.readable().len());
    let notes = vec![
        format!(
            "the leaver kept reading for {} between the Leave and the Remove, and read {read_while_leaving} events of the old ping by then",
            removed_at - left_at
        ),
        format!(
            "after the removal the leaver reads {leaver_old} of {old_total} events of the old ping and 0 of the new one"
        ),
    ];
    report("member_leaves_ping_ends", &world, notes)
}

/// A join request waits hours of virtual time until a member's device approves.
///
/// Every member goes offline after the invite; the joiner asks; three hours pass;
/// the members come back, see the request and approve. Asserts that the approval
/// is at least three hours after the request in the timestamps that the server
/// wrote, and that the joiner then reads a ping that starts after the join.
pub fn join_request_waits_hours<C: GroupCipher>(seed: Seed) -> Report {
    let mut world = World::<C>::new(seed);
    let founder = world.found_group();
    let member = world.admit(founder);
    let group_log = LogId::Group(world.group());

    let code = world.new_code();
    world.queue(founder, Intent::Invite(code));
    world.run_until_idle();

    let back = world.now() + Millis::from_hours(3);
    world.set_offline(founder, Offline::Until(back));
    world.set_offline(member, Offline::Until(back));
    let joiner = world.add_device();
    world.queue(joiner, Intent::Request(code));
    world.run_until_idle();
    let key = world.device(joiner).key();
    assert!(!world.device(joiner).is_member());

    world.run_for(Millis::from_hours(3));
    world.run_until(|w| w.device(founder).state().request_of(&key).is_some());
    let request = world
        .device(founder)
        .state()
        .request_of(&key)
        .expect("the request reached the founder");
    world.queue(founder, Intent::Approve(request));
    world.run_until_idle();
    assert_converged(&world, "join_request_waits_hours");
    assert!(
        world.device(joiner).is_member(),
        "join_request_waits_hours: the joiner is not in the group after the approval"
    );

    let stamps = |seq: Seq| {
        Envelope::from_bytes(&world.log().entries(&group_log)[seq.index()])
            .expect("the log holds events")
            .timestamp()
    };
    let approval = Seq(world.device(founder).head(&group_log).0 - 1);
    let waited = stamps(approval) - stamps(request);
    assert!(
        waited >= Millis::from_hours(3),
        "join_request_waits_hours: the approval came after {waited}, not after the hours the members were away"
    );

    let ping = world.start_ping(founder);
    world.post(founder, ping, b"welcome");
    world.post(joiner, ping, b"thanks");
    world.run_until_idle();
    for idx in [founder, member, joiner] {
        let view = world.device(idx).ping(ping).expect("the ping is read");
        assert_eq!(
            view.readable().len(),
            2,
            "join_request_waits_hours: {idx} should read both events of the ping"
        );
    }

    let notes = vec![format!("the join request waited {waited} for an approval")];
    report("join_request_waits_hours", &world, notes)
}

/// A member revokes a code that nobody used; a request with it is refused, and a
/// member's approval of that request is ignored too.
///
/// Asserts that every device ignored the request as `CodeRevoked` and the forged
/// approval as `UnknownRequest`, and that the joiner is in no roster and reads
/// nothing.
pub fn revoked_code_refused<C: GroupCipher>(seed: Seed) -> Report {
    let mut world = World::<C>::new(seed);
    let founder = world.found_group();
    let member = world.admit(founder);
    let group_log = LogId::Group(world.group());

    let code = world.new_code();
    world.queue(founder, Intent::Invite(code));
    world.run_until_idle();
    world.queue(member, Intent::Revoke(code));
    world.run_until_idle();
    assert_eq!(
        world.device(founder).state().code(&code),
        Some(CodeState::Revoked)
    );

    let joiner = world.add_device();
    world.queue(joiner, Intent::Request(code));
    world.run_until_idle();
    let request = Seq(world.device(founder).head(&group_log).0 - 1);

    world.queue(founder, Intent::Forge(GroupChange::Approve { request }));
    world.run_until_idle();
    assert_converged(&world, "revoked_code_refused");

    for (idx, device) in world.devices() {
        assert!(
            !device
                .state()
                .roster()
                .contains(&world.device(joiner).key()),
            "revoked_code_refused: {idx} let the joiner in on a revoked code (see rule 3 in the module docs of e2e_harness::rules)"
        );
        assert_eq!(
            ignored_with(&world, idx, Refusal::CodeRevoked(code)),
            vec![request],
            "revoked_code_refused: {idx} did not refuse the request"
        );
        assert_eq!(
            ignored_with(&world, idx, Refusal::UnknownRequest(request)),
            vec![Seq(request.0 + 1)],
            "revoked_code_refused: {idx} did not refuse the approval"
        );
    }

    let ping = world.start_ping(founder);
    world.post(member, ping, b"members only");
    world.run_until_idle();
    let joiner_reads = world
        .device(joiner)
        .ping(ping)
        .map_or(0, |view| view.readable().len());
    assert_eq!(
        joiner_reads, 0,
        "revoked_code_refused: the joiner reads a ping"
    );

    let notes =
        vec!["the request and the forged approval were ignored by all devices alike".to_owned()];
    report("revoked_code_refused", &world, notes)
}

/// Two devices ask with one single-use code: the first request in the log holds
/// it, and the second is refused, however the members act.
///
/// Both requests are queued together, so the log decides which comes first. The
/// winner is approved. A forged approval of the loser is ignored. Asserts that
/// the loser is in no roster and that every device ignored the same two events.
pub fn second_use_of_code_refused<C: GroupCipher>(seed: Seed) -> Report {
    let mut world = World::<C>::new(seed);
    let founder = world.found_group();
    let member = world.admit(founder);

    let code = world.new_code();
    world.queue(founder, Intent::Invite(code));
    world.run_until_idle();
    let (first, second) = (world.add_device(), world.add_device());
    world.queue(first, Intent::Request(code));
    world.queue(second, Intent::Request(code));
    world.run_until_idle();

    let state = world.device(member).state();
    let (winner, loser) = {
        let holds = |idx: DeviceIdx| state.request_of(&world.device(idx).key());
        match (holds(first), holds(second)) {
            (Some(_), None) => (first, second),
            (None, Some(_)) => (second, first),
            (won_first, won_second) => panic!(
                "second_use_of_code_refused: exactly one request holds a single-use code, but {won_first:?} and {won_second:?} do (see rule 3 in the module docs of e2e_harness::rules)"
            ),
        }
    };
    let won = state
        .request_of(&world.device(winner).key())
        .expect("the winner holds the code");
    let lost = ignored_with(&world, member, Refusal::CodeUsed(code));
    assert_eq!(
        lost.len(),
        1,
        "second_use_of_code_refused: the second request was not refused"
    );

    world.queue(member, Intent::Approve(won));
    world.queue(
        founder,
        Intent::Forge(GroupChange::Approve { request: lost[0] }),
    );
    world.run_until_idle();
    assert_converged(&world, "second_use_of_code_refused");

    for (idx, device) in world.devices() {
        let roster = device.state().roster();
        assert!(
            roster.contains(&world.device(winner).key())
                && !roster.contains(&world.device(loser).key()),
            "second_use_of_code_refused: {idx} did not admit exactly the first request"
        );
        assert_eq!(ignored_with(&world, idx, Refusal::CodeUsed(code)), lost);
    }

    let ping = world.start_ping(founder);
    world.post(winner, ping, b"I am in");
    world.run_until_idle();
    let reads = |idx: DeviceIdx| {
        world
            .device(idx)
            .ping(ping)
            .map_or(0, |view| view.readable().len())
    };
    assert_eq!((reads(founder), reads(winner), reads(loser)), (1, 1, 0));

    let notes = vec!["the first request in the log held the code; the second and a forged approval of it were ignored by all devices".to_owned()];
    report("second_use_of_code_refused", &world, notes)
}

/// Events of a device that no user vouches for change nothing, on every device:
/// a stranger's, and those of a user after their removal.
///
/// The stranger writes an invite, an approval, a new device, a removal and a
/// revoke into the group log. Asserts that every device ignored each as
/// `Unvouched`, and that the roster and the codes are as they were.
pub fn unvouched_device_ignored<C: GroupCipher>(seed: Seed) -> Report {
    let mut world = World::<C>::new(seed);
    let founder = world.found_group();
    let member = world.admit(founder);
    let user = world.device(member).user().expect("a member");

    let stranger = world.add_device();
    let stranger_key = world.device(stranger).key();
    let (code, other) = (world.new_code(), world.new_code());
    world.queue(founder, Intent::Invite(other));
    world.run_until_idle();
    let roster_before = world.device(founder).state().roster().clone();

    let forged = [
        GroupChange::Invite { code },
        GroupChange::Approve { request: Seq(1) },
        GroupChange::AddDevice {
            device: stranger_key,
            introduction: Default::default(),
        },
        GroupChange::Remove { user },
        GroupChange::Revoke { code: other },
        GroupChange::Leave,
    ];
    for change in &forged {
        world.queue(stranger, Intent::Forge(change.clone()));
    }
    world.run_until_idle();
    assert_converged(&world, "unvouched_device_ignored");

    for (idx, device) in world.devices() {
        assert_eq!(
            ignored_with(&world, idx, Refusal::Unvouched).len(),
            forged.len(),
            "unvouched_device_ignored: {idx} did not ignore all {} events of the stranger (see rule 2 in the module docs of e2e_harness::rules)",
            forged.len()
        );
        assert_eq!(
            *device.state().roster(),
            roster_before,
            "unvouched_device_ignored: {idx} changed its roster"
        );
        assert!(device.state().code(&code).is_none());
        assert!(matches!(
            device.state().code(&other),
            Some(CodeState::Issued { .. })
        ));
    }

    // A removed user is unvouched from the removal on.
    world.queue(founder, Intent::Remove(user));
    world.run_until_idle();
    let removed = ignored_with(&world, founder, Refusal::Unvouched).len();
    let forged_code = world.new_code();
    world.queue(
        member,
        Intent::Forge(GroupChange::Invite { code: forged_code }),
    );
    world.run_until_idle();
    assert_eq!(
        ignored_with(&world, founder, Refusal::Unvouched).len(),
        removed + 1,
        "unvouched_device_ignored: the event of a removed user was not ignored"
    );
    assert_converged(&world, "unvouched_device_ignored");

    let notes = vec![format!(
        "{} events of a stranger and 1 of a removed user were ignored by all {} devices alike",
        forged.len(),
        world.device_count()
    )];
    report("unvouched_device_ignored", &world, notes)
}

fn evil_report<C: GroupCipher>(
    scenario: &'static str,
    seed: Seed,
    kind: Kind,
    check: impl Fn(&Detection),
) -> Report {
    let mut notes = Vec::new();
    let (mut events, mut bytes, mut devices) = (0, 0, 0);
    for shape in [Shape::Busy, Shape::Quiet] {
        for vigilance in [Vigilance::InBand, Vigilance::Gossip] {
            let detection = evil::measure::<C>(seed, kind, shape, vigilance);
            check(&detection);
            events += detection.events;
            bytes += detection.bytes;
            devices = devices.max(detection.devices);
            notes.push(detection.to_string());
        }
    }
    Report {
        scenario,
        cipher: C::NAME,
        events,
        bytes,
        devices,
        notes,
        logs: BTreeMap::new(),
    }
}

/// The server acknowledges an event and stores nothing ([`crate::evil::Attack::Drop`]).
///
/// Asserts that the author, and only the author, raises an alarm, in a busy and
/// in a quiet group, with and without gossip. No other device can notice: the
/// chain has no gap (see [`crate::evil`]).
pub fn evil_server_drop<C: GroupCipher>(seed: Seed) -> Report {
    evil_report::<C>("evil_server_drop", seed, Kind::Drop, |d| {
        assert!(
            d.all_noticed() && d.affected == 1,
            "evil_server_drop: the author alone should notice: {d} (see the module docs of e2e_harness::evil)"
        );
    })
}

/// The server hides the newest events from one reader ([`crate::evil::Attack::HoldBack`]).
///
/// Asserts the open risk of #61: in a busy group the victim notices as soon as it
/// writes, because the conflict carries the head; in a **quiet** group, looking at
/// the log alone, the victim never notices. With gossip it notices in both.
pub fn evil_server_hold_back<C: GroupCipher>(seed: Seed) -> Report {
    evil_report::<C>("evil_server_hold_back", seed, Kind::HoldBack, |d| {
        let expected = match (d.shape, d.vigilance) {
            (Shape::Quiet, Vigilance::InBand) => d.alarmed == 0 && d.affected == 1,
            (Shape::Busy, _) | (Shape::Quiet, Vigilance::Gossip) => d.all_noticed(),
        };
        assert!(
            expected,
            "evil_server_hold_back: not the result that the model of the open risk of #61 predicts: {d} (see the module docs of e2e_harness::evil)"
        );
    })
}

/// The server serves two events in the other order ([`crate::evil::Attack::Reorder`]).
///
/// Asserts that every reader that was served the swapped pair raises an alarm
/// at once, in a busy and in a quiet group: the position of an event is signed.
pub fn evil_server_reorder<C: GroupCipher>(seed: Seed) -> Report {
    evil_report::<C>("evil_server_reorder", seed, Kind::Reorder, |d| {
        assert!(
            d.all_noticed(),
            "evil_server_reorder: every reader of the swapped pair should notice: {d} (see the module docs of e2e_harness::evil)"
        );
    })
}

/// The server shows two sets of devices two histories from one position
/// ([`crate::evil::Attack::Fork`]).
///
/// Asserts the finding of [`crate::evil`]: looking at the log alone nobody
/// notices, in a busy or a quiet group; with gossip every device of a busy group
/// does, and in a quiet group the half that falls behind (the writer's half is
/// not affected: it sees what an honest server would show).
pub fn evil_server_fork<C: GroupCipher>(seed: Seed) -> Report {
    evil_report::<C>("evil_server_fork", seed, Kind::Fork, |d| {
        let expected = match (d.shape, d.vigilance) {
            (_, Vigilance::InBand) => d.alarmed == 0 && d.affected > 0,
            (_, Vigilance::Gossip) => d.all_noticed(),
        };
        assert!(
            expected,
            "evil_server_fork: not the result that the model of the open risk of #61 predicts: {d} (see the module docs of e2e_harness::evil)"
        );
    })
}

/// Two runs of [`thirty_members_three_devices`] with one seed store the same
/// bytes in every log, and a run with another seed does not.
///
/// This is the property the spike asks of each candidate (question 5 of #62): it
/// draws all its randomness from the generator it is given and reads no clock.
///
/// # Panics
///
/// Panics, naming the first log and position that differ, when two runs with one
/// seed differ, or when a run with another seed is the same.
pub fn same_seed_twice_gives_identical_logs<C: GroupCipher>() {
    let seed = default_seed();
    let first = thirty_members_three_devices::<C>(seed);
    let second = thirty_members_three_devices::<C>(seed);
    assert_eq!(
        first.logs.keys().collect::<Vec<_>>(),
        second.logs.keys().collect::<Vec<_>>(),
        "same_seed_twice_gives_identical_logs: two runs with seed {seed:?} stored different logs"
    );
    for (log, entries) in &first.logs {
        let again = &second.logs[log];
        assert_eq!(
            entries.len(),
            again.len(),
            "same_seed_twice_gives_identical_logs: the log {log} has {} events in one run and {} in the other (see the module docs of e2e_harness::rng)",
            entries.len(),
            again.len()
        );
        for (position, (a, b)) in entries.iter().zip(again).enumerate() {
            assert_eq!(
                a, b,
                "same_seed_twice_gives_identical_logs: the event {position} of {log} differs between two runs with seed {seed:?}: the cipher or the harness reads something that the seed does not fix (see the module docs of e2e_harness::rng)"
            );
        }
    }
    let other = thirty_members_three_devices::<C>(Seed(seed.0.wrapping_add(1)));
    assert_ne!(
        first.logs, other.logs,
        "same_seed_twice_gives_identical_logs: a run with another seed stored the same bytes: the seed does not reach the keys"
    );
}
