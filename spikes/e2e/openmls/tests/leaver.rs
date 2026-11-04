//! Question 7 of #62: a leaver while every other member is offline.
//!
//! The rule that this crate uses: the leaver writes `Leave`, which changes no key;
//! the first device of **another** member that reads it writes the `Remove`
//! Commit. In MLS a Commit that removes its own committer is invalid (RFC 9420
//! section 12.2), so the leaver cannot do it, and without a member's device that
//! is online the group key stays the old one. The leaver keeps reading until that
//! Commit is in the group log, and the old ping for ever (D10).

use e2e_harness::{
    clock::Millis,
    device::Intent,
    event::Envelope,
    ids::LogId,
    payload::{GroupChange, GroupPayload},
    rng::Seed,
    sim::{Offline, World},
};
use e2e_openmls::OpenMls;

/// The time between the `Leave` and the `Remove` in the group log, in the
/// timestamps that the server wrote, when every other member is away for `away`.
fn leave_to_remove(away: Millis) -> Millis {
    let mut world = World::<OpenMls>::new(Seed(41));
    let founder = world.found_group();
    let others: Vec<_> = (0..4).map(|_| world.admit(founder)).collect();
    let leaver = world.admit(founder);
    let user = world.device(leaver).user().expect("a member");

    let back = world.now() + away;
    for idx in std::iter::once(founder).chain(others.iter().copied()) {
        world.set_offline(idx, Offline::Until(back));
    }
    world.queue(leaver, Intent::Leave);
    world.run_until_idle();

    // The policy: the first device of another member that has read the Leave
    // writes the Remove.
    world.run_until(|w| {
        w.devices()
            .any(|(idx, device)| idx != leaver && device.state().is_leaving(user))
    });
    let writer = world
        .devices()
        .find(|(idx, device)| *idx != leaver && device.state().is_leaving(user))
        .map(|(idx, _)| idx)
        .expect("a device read the Leave");
    world.queue(writer, Intent::Remove(user));
    world.run_until_idle();

    let group = LogId::Group(world.group());
    let stamp = |wanted: fn(&GroupChange) -> bool| {
        world
            .log()
            .entries(&group)
            .iter()
            .filter_map(|bytes| Envelope::from_bytes(bytes).ok())
            .find(|envelope| {
                GroupPayload::decode(&envelope.body().payload)
                    .is_ok_and(|payload| wanted(&payload.change))
            })
            .map(|envelope| envelope.timestamp())
            .expect("the event is in the log")
    };
    let left = stamp(|change| matches!(change, GroupChange::Leave));
    let removed = stamp(|change| matches!(change, GroupChange::Remove { .. }));

    // The ping that starts after the Remove is closed to the leaver.
    let ping = world.start_ping(founder);
    world.post(founder, ping, b"without you");
    world.run_until_idle();
    assert_eq!(
        world
            .device(leaver)
            .ping(ping)
            .map_or(0, |view| view.readable().len()),
        0
    );
    removed - left
}

#[test]
fn with_the_other_members_online_the_leaver_keeps_reading_for_seconds() {
    let delay = leave_to_remove(Millis(0));
    println!("every other member online: Leave to Remove in {delay}");
    assert!(delay < Millis::from_mins(1), "{delay}");
}

#[test]
fn with_every_other_member_away_the_leaver_keeps_reading_until_one_comes_back() {
    let delay = leave_to_remove(Millis::from_hours(3));
    println!("every other member away for 3 hours: Leave to Remove in {delay}");
    assert!(
        delay >= Millis::from_hours(3) - Millis::from_mins(1),
        "{delay}"
    );
    assert!(
        delay < Millis::from_hours(3) + Millis::from_mins(10),
        "{delay}"
    );
}
