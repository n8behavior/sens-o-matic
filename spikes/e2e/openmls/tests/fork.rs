//! Question 3 of #62: a fork of the group log, and what MLS adds to the hash
//! chain of the harness.
//!
//! The harness already measures, for any cipher, what the hash chain notices (the
//! `evil_server_*` scenarios; the numbers are in `evil` of the harness). Here the
//! server forks the **group log**: half of the devices see one Commit at a
//! position and the other half another one, so the two halves are in two MLS
//! states of the same epoch number. Nothing in the logs shows it (the chain of
//! each history is intact). The ping log is shared, and its events carry the hash
//! of the Commit their author merged ("the hash of the Commit I merged" of the
//! OpenMLS book, inside every ping event). A reader that merged another Commit
//! for that epoch has read an event whose AEAD passed and whose claim did not:
//! `forks_seen`. Without the claim the same event is read without a sign of the
//! fork, because the key of a ping does not depend on the diverged state.

use e2e_harness::{
    clock::Millis,
    device::Intent,
    evil::{Attack, EvilLog},
    ids::LogId,
    log::MemoryLog,
    rng::Seed,
    sim::{DeviceIdx, World},
};
use e2e_openmls::OpenMls;

type Evil = World<OpenMls, EvilLog<MemoryLog>>;

struct Fork {
    world: Evil,
    devices: Vec<DeviceIdx>,
    /// The devices on the second history.
    branch: Vec<DeviceIdx>,
}

/// Twelve devices (six members with two devices each), a ping with a warm-up
/// event from everyone, then a fork of the group log in which the first device
/// (on the main history) and the second (on the branch) each add a new device.
fn forked() -> (Fork, e2e_harness::ids::PingId) {
    let mut world = Evil::with_log(Seed(21), EvilLog::default());
    let founder = world.found_group();
    let mut devices = vec![founder];
    for _ in 1..6 {
        devices.push(world.admit(founder));
    }
    for member in devices.clone() {
        devices.push(world.add_device_to(member));
    }
    let ping = world.start_ping(founder);
    for device in &devices {
        world.post(*device, ping, b"warm-up");
    }
    world.run_until_idle();

    let group_log = LogId::Group(world.group());
    let at = world.head_of(founder, &group_log);
    let branch: Vec<DeviceIdx> = devices.iter().skip(1).step_by(2).copied().collect();
    let keys = branch.iter().map(|idx| world.device(*idx).key()).collect();
    world.log_mut().arm(Attack::Fork {
        log: group_log,
        at,
        branch: keys,
    });
    // Two Commits at one position, one on each history.
    for committer in [devices[0], branch[0]] {
        let newcomer = world.add_device();
        let info = world.introduction_of(newcomer);
        world.queue(committer, Intent::AddDevice(info));
    }
    world.run_until_idle();
    assert!(
        world
            .devices()
            .all(|(_, device)| device.alarms().is_empty()),
        "the chain of each history is intact: nothing raises an alarm in band"
    );
    (
        Fork {
            world,
            devices,
            branch,
        },
        ping,
    )
}

fn noticed(fork: &Fork) -> Vec<DeviceIdx> {
    fork.devices
        .iter()
        .copied()
        .filter(|idx| fork.world.device(*idx).cipher().forks_seen() > 0)
        .collect()
}

/// A quiet group: one device writes. Every device on the other history notices at
/// the first event; the writer's own half never does, and cannot.
#[test]
fn in_a_quiet_group_the_other_half_sees_the_fork_at_the_first_event() {
    let (mut fork, ping) = forked();
    let writer = fork.devices[0];
    let before = fork.world.writes();
    fork.world.post(writer, ping, b"are you there?");
    fork.world.run_until_idle();
    assert_eq!(fork.world.writes() - before, 1);

    let seen = noticed(&fork);
    assert_eq!(
        seen, fork.branch,
        "exactly the devices of the branch notice, after 1 event"
    );
    for idx in &fork.branch {
        let view = fork.world.device(*idx).ping(ping).expect("read");
        assert!(
            view.unreadable().len() == 1,
            "the event is refused, not read"
        );
    }
    // The main half reads its own history without a sign.
    for idx in fork.devices.iter().filter(|idx| !fork.branch.contains(idx)) {
        assert_eq!(fork.world.device(*idx).cipher().forks_seen(), 0);
    }
}

/// A busy group: every device writes once, in turn. After the second event, one
/// from each history, all twelve devices have noticed.
#[test]
fn in_a_busy_group_every_device_sees_the_fork_after_two_events() {
    let (mut fork, ping) = forked();
    let (first, second) = (fork.devices[0], fork.branch[0]);
    let mut events = 0;
    for writer in [first, second] {
        fork.world.post(writer, ping, b"hello");
        fork.world.run_until_idle();
        events += 1;
        if noticed(&fork).len() == fork.devices.len() {
            break;
        }
    }
    assert_eq!(events, 2);
    assert_eq!(noticed(&fork), fork.devices);
}

/// Without any message of the group the fork is not seen: the clock runs for a
/// day and nobody writes. This is the open risk of #61 and MLS does not close it.
#[test]
fn a_fork_in_a_silent_group_is_not_seen_at_all() {
    let (mut fork, _) = forked();
    fork.world.run_for(Millis::from_hours(24));
    fork.world.run_until_idle();
    assert!(noticed(&fork).is_empty());
    assert!(
        fork.world
            .devices()
            .all(|(_, device)| device.alarms().is_empty())
    );
}
