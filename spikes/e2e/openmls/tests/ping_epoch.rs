//! Question 1 of #62, the key epoch of a ping event, for the cases that D10 keeps:
//! a change of members ends the ping (`member_leaves_ping_ends`), a new device of
//! a known user joins in the middle of a ping (`new_device_mid_ping`), and the
//! case that only this file tests: an author who writes late, under an old epoch.

use e2e_harness::{
    device::Act,
    device::Intent,
    evil::{Attack, EvilLog},
    ids::LogId,
    log::MemoryLog,
    rng::Seed,
    sim::World,
};
use e2e_openmls::OpenMls;

/// A member whose device has not read the removal writes into the old ping after
/// a member who is up to date has written. The server holds the group log back
/// from that one device, which is how a device stays at epoch *e* while the ping
/// log moves on. The ping log then holds events whose authors were in epoch *e*,
/// *e + 1* and *e* again.
///
/// Every device that holds the key of the ping reads every event, in any order of
/// epochs, because the key of a ping comes from the epoch where it began and not
/// from the epoch of the reader or of the author. The removed member reads the
/// events written after the removal too: D10 accepts it. What an honest client
/// needs is the rule of D10, `ping_ended`: after it has applied a removal that
/// follows the start of the ping, it neither writes to the ping nor counts later
/// events as part of the plan.
#[test]
fn an_author_who_writes_late_under_the_old_epoch_is_read_by_everyone_who_holds_the_key() {
    let mut world = World::<OpenMls, EvilLog<MemoryLog>>::with_log(Seed(11), EvilLog::default());
    let founder = world.found_group();
    let (member, leaver, stale) = (
        world.admit(founder),
        world.admit(founder),
        world.admit(founder),
    );
    let leaver_user = world.device(leaver).user().expect("a member");
    let group_log = LogId::Group(world.group());
    let ping = world.start_ping(founder);
    world.post(founder, ping, b"before the removal");
    world.run_until_idle();

    // The group log is held back from `stale` from here on: it never reads the removal.
    let from = world.head_of(founder, &group_log);
    let key = world.device(stale).key();
    world.log_mut().arm(Attack::HoldBack {
        log: group_log,
        victim: key,
        from,
    });
    world.queue(founder, Intent::Remove(leaver_user));
    world.run_until_idle();

    // The founder writes in epoch e + 1; then the stale device reads it and writes
    // in epoch e.
    world.post(founder, ping, b"after, epoch e + 1");
    world.run_until_idle();
    world.sync(stale);
    world.post(stale, ping, b"late, epoch e");
    assert!(
        matches!(world.act(stale), Act::Written(_)),
        "the stale device writes"
    );
    world.post(member, ping, b"after, epoch e + 1 again");
    world.run_until_idle();

    let log = world.device(founder).ping_log(ping);
    assert_eq!(
        world.device(founder).head(&log).0 - 1,
        4,
        "four events after the first"
    );
    for (name, idx) in [("founder", founder), ("member", member), ("stale", stale)] {
        let view = world.device(idx).ping(ping).expect("the ping is read");
        assert_eq!(view.readable().len(), 4, "{name} reads all four events");
        assert_eq!(
            world.device(idx).cipher().forks_seen(),
            0,
            "{name} sees no fork"
        );
    }
    assert_eq!(world.device(founder).ping_ended(ping), Some(true));
    assert_eq!(world.device(member).ping_ended(ping), Some(true));
    assert_eq!(
        world.device(stale).ping_ended(ping),
        Some(false),
        "the stale device does not know that the ping ended"
    );
    // The removed member read the three events written after the removal.
    let leaver_view = world
        .device(leaver)
        .ping(ping)
        .expect("the leaver still reads the log");
    assert_eq!(leaver_view.readable().len(), 4);

    // A ping that begins after the removal is closed to the removed member.
    let next = world.start_ping(founder);
    world.post(founder, next, b"new ping");
    world.run_until_idle();
    assert_eq!(
        world
            .device(member)
            .ping(next)
            .expect("read")
            .readable()
            .len(),
        1
    );
    assert_eq!(
        world
            .device(leaver)
            .ping(next)
            .map_or(0, |view| view.readable().len()),
        0
    );
}
