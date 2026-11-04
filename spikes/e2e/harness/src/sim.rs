//! The simulator: a world of one server, many devices and a virtual clock.
//!
//! A [`World`] runs on one thread. It holds the [`Server`], the devices and the
//! [`Clock`], and a seeded scheduler decides what happens next. Nothing in it
//! reads the system time or the OS randomness, so a run is a function of its
//! [`Seed`] and of the cipher: two runs with one seed store the same bytes.
//!
//! # Steps
//!
//! A step is one of:
//!
//! - **Sync**: a device reads the logs it follows from where it stopped, checks
//!   and applies them. It runs when the device was woken, when its poll is due,
//!   when it came back online, or when the server restarted.
//! - **Act**: a device does the intent at the front of its queue, from the state
//!   it has. A device acts only after it has synced since its last write or
//!   conflict, so it never writes from a view it knows to be stale.
//! - **Deliver**: a wake that the server queued reaches its device.
//! - **Gossip**: a device tells a random peer its heads, on a channel the server
//!   does not control (only when [`Faults::gossip_every`] is set).
//!
//! Each step of a loop is picked at random, with the seeded generator, among the
//! steps that are ready. A device that synced can be overtaken by another that
//! writes before it acts: that is how two devices both expect one position and
//! one loses (a conflict), with no scripting. When no step is ready the clock
//! jumps to the next time at which one will be.
//!
//! # Faults
//!
//! [`Faults`] sets the weather: a wake is late or lost, a rival wins an append.
//! A test also injects single faults: [`World::set_offline`] takes a device
//! offline for a span of time or for a number of events,
//! [`World::restart_server`] loses what the server keeps in memory, and
//! [`World::restart_device`] closes and opens a device's cipher from its
//! exported state. A lost wake costs only latency, because each device polls.
//!
//! # Verbs
//!
//! [`World::found_group`], [`World::admit`], [`World::add_device_to`] and
//! [`World::start_ping`] run the usual flows to the end (the code, the request,
//! the approval) so that a scenario can build a group in a few lines.
//!
//! ```
//! use e2e_harness::fake::FakeCipher;
//! use e2e_harness::sim::World;
//! use e2e_harness::Seed;
//!
//! let mut world = World::<FakeCipher>::new(Seed(1));
//! let alice = world.found_group();
//! let bob = world.admit(alice);
//! let ping = world.start_ping(alice);
//! world.post(bob, ping, b"on my way");
//! world.run_until_idle();
//!
//! // Both devices read the same group state and the same message.
//! assert_eq!(world.device(alice).group_digest(), world.device(bob).group_digest());
//! assert_eq!(world.device(alice).ping(ping).unwrap().readable().len(), 1);
//! assert_eq!(world.device(alice).ping(ping).unwrap().digest(),
//!            world.device(bob).ping(ping).unwrap().digest());
//! ```

use std::{collections::BTreeMap, fmt};

use crate::{
    cipher::{CipherError, GroupCipher},
    clock::{Clock, Millis},
    device::{Act, Device, Intent},
    ids::{GroupId, InviteCode, PingId, Seq, UserId},
    keys::{DeviceKey, Identity},
    log::{Log, MemoryLog, Wake},
    rng::{Probability, Seed, SimRng},
    roster::DeviceInfo,
    server::Server,
};

/// The most steps that a `run_until` takes before it gives up.
const STEP_BUDGET: u64 = 400_000;

/// A device of a world, by order of creation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceIdx(pub usize);

impl fmt::Display for DeviceIdx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "d{}", self.0)
    }
}

/// The weather of a world.
#[derive(Debug, Clone, PartialEq)]
pub struct Faults {
    /// The chance that a wake never arrives.
    pub wake_loss: Probability,
    /// The least and the most time a wake takes to arrive.
    pub wake_delay: (Millis, Millis),
    /// The chance that, before a device writes, a rival device wins the position.
    pub rival: Probability,
    /// How often a device reads even if no wake came.
    pub poll_every: Millis,
    /// How often a device tells a peer its heads; `None` for never.
    pub gossip_every: Option<Millis>,
}

impl Faults {
    /// Wakes arrive within 200 ms, none is lost, no rival; poll every 5 minutes.
    pub fn calm() -> Self {
        Self {
            wake_loss: Probability::NEVER,
            wake_delay: (Millis(0), Millis(200)),
            rival: Probability::NEVER,
            poll_every: Millis::from_mins(5),
            gossip_every: None,
        }
    }

    /// A fifth of the wakes are lost, the others take up to 30 s, and one write
    /// in ten meets a rival.
    pub fn rough() -> Self {
        Self {
            wake_loss: Probability::new(0.2).expect("0.2 is a chance"),
            wake_delay: (Millis(0), Millis::from_secs(30)),
            rival: Probability::new(0.1).expect("0.1 is a chance"),
            ..Self::calm()
        }
    }
}

impl Default for Faults {
    fn default() -> Self {
        Self::calm()
    }
}

/// Why a device is not reading or writing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Offline {
    /// Until the clock reaches this time.
    Until(Millis),
    /// Until the world has accepted this many events in all.
    UntilEvents(u64),
}

/// What happened in a world, for the tests that look.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stats {
    /// Steps taken by the scheduler.
    pub steps: u64,
    /// Appends that lost the position.
    pub conflicts: u64,
    /// Rivals that [`Faults::rival`] injected.
    pub rivals: u64,
    /// Wakes that were lost.
    pub wakes_lost: u64,
    /// Wakes that reached an online device.
    pub wakes_delivered: u64,
    /// Restarts of the server.
    pub server_restarts: u64,
    /// Intents that a device dropped, with the device and the reason.
    pub skipped: Vec<(DeviceIdx, String)>,
}

#[derive(Debug)]
struct Slot<C> {
    device: Device<C>,
    offline: Option<Offline>,
    dirty: bool,
    blocked: bool,
    needs_subscribe: bool,
    next_poll: Millis,
    next_gossip: Millis,
    synced_at: u64,
}

#[derive(Debug, Clone, Copy)]
struct InFlight {
    due: Millis,
    wake: Wake,
}

#[derive(Debug, Clone, Copy)]
enum Step {
    Deliver(usize),
    Sync(DeviceIdx),
    Act(DeviceIdx),
    Gossip(DeviceIdx),
}

/// One run: the server, the devices, the clock and the scheduler.
#[derive(Debug)]
pub struct World<C, L = MemoryLog> {
    group: GroupId,
    clock: Clock,
    rng: SimRng,
    server: Server<L>,
    slots: Vec<Slot<C>>,
    by_key: BTreeMap<DeviceKey, DeviceIdx>,
    faults: Faults,
    in_flight: Vec<InFlight>,
    writes: u64,
    pings: Vec<PingId>,
    stats: Stats,
    first_alarm: BTreeMap<DeviceIdx, u64>,
}

impl<C: GroupCipher> World<C, MemoryLog> {
    /// A world over a [`MemoryLog`], with calm weather.
    pub fn new(seed: Seed) -> Self {
        Self::with_log(seed, MemoryLog::default())
    }
}

impl<C: GroupCipher, L: Log> World<C, L> {
    /// A world over `log`, with calm weather.
    pub fn with_log(seed: Seed, log: L) -> Self {
        let clock = Clock::new();
        Self {
            group: GroupId(1),
            server: Server::new(log, clock.clone()),
            clock,
            rng: SimRng::new(seed),
            slots: Vec::new(),
            by_key: BTreeMap::new(),
            faults: Faults::calm(),
            in_flight: Vec::new(),
            writes: 0,
            pings: Vec::new(),
            stats: Stats::default(),
            first_alarm: BTreeMap::new(),
        }
    }

    // ---- Look ----

    /// The time now.
    pub fn now(&self) -> Millis {
        self.clock.now()
    }

    /// The group the world runs.
    pub fn group(&self) -> GroupId {
        self.group
    }

    /// The server.
    pub fn server(&self) -> &Server<L> {
        &self.server
    }

    /// The storage, to read it or to arm an attack.
    pub fn log(&self) -> &L {
        self.server.log()
    }

    /// The storage, to arm an attack of [`crate::evil::EvilLog`].
    pub fn log_mut(&mut self) -> &mut L {
        self.server.log_mut()
    }

    /// The generator of the scheduler, for a scenario that needs a choice.
    pub fn rng(&mut self) -> &mut SimRng {
        &mut self.rng
    }

    /// The weather.
    pub fn faults(&self) -> &Faults {
        &self.faults
    }

    /// Change the weather.
    pub fn faults_mut(&mut self) -> &mut Faults {
        &mut self.faults
    }

    /// What happened so far.
    pub fn stats(&self) -> &Stats {
        &self.stats
    }

    /// The number of events the server has accepted.
    pub fn writes(&self) -> u64 {
        self.writes
    }

    /// The device `idx`.
    pub fn device(&self, idx: DeviceIdx) -> &Device<C> {
        &self.slots[idx.0].device
    }

    /// The number of devices.
    pub fn device_count(&self) -> usize {
        self.slots.len()
    }

    /// Every device, in the order of creation.
    pub fn devices(&self) -> impl Iterator<Item = (DeviceIdx, &Device<C>)> + '_ {
        self.slots
            .iter()
            .enumerate()
            .map(|(i, slot)| (DeviceIdx(i), &slot.device))
    }

    /// The device that has `key`.
    pub fn index_of(&self, key: &DeviceKey) -> Option<DeviceIdx> {
        self.by_key.get(key).copied()
    }

    /// The number of events the server had accepted when the device raised its
    /// first alarm; `None` if it has raised none.
    pub fn first_alarm(&self, idx: DeviceIdx) -> Option<u64> {
        self.first_alarm.get(&idx).copied()
    }

    /// True when the device is offline now.
    pub fn is_offline(&self, idx: DeviceIdx) -> bool {
        self.slots[idx.0]
            .offline
            .is_some_and(|offline| !self.offline_over(offline))
    }

    // ---- Make ----

    /// A new device that no user vouches for yet.
    pub fn add_device(&mut self) -> DeviceIdx {
        let identity = Identity::generate(&mut self.rng.fork());
        let mut device = Device::new(identity, self.group, self.rng.fork());
        for ping in &self.pings {
            device.follow(*ping);
        }
        let idx = DeviceIdx(self.slots.len());
        self.by_key.insert(device.key(), idx);
        let poll_in = self.rng.between(0, self.faults.poll_every.0);
        let gossip_in = self
            .faults
            .gossip_every
            .map_or(0, |every| self.rng.between(0, every.0));
        self.slots.push(Slot {
            device,
            offline: None,
            dirty: true,
            blocked: false,
            needs_subscribe: true,
            next_poll: self.now() + Millis(poll_in),
            next_gossip: self.now() + Millis(gossip_in),
            synced_at: 0,
        });
        idx
    }

    /// A code for an invite, from the generator of the world.
    pub fn new_code(&mut self) -> InviteCode {
        InviteCode::generate(&mut self.rng)
    }

    /// A new ping, announced to every device, now and to come: a device that has
    /// joined reads its log. The ping has no event until a device starts it.
    pub fn new_ping(&mut self) -> PingId {
        let ping = PingId(self.pings.len() as u32 + 1);
        self.pings.push(ping);
        for slot in &mut self.slots {
            slot.device.follow(ping);
            slot.needs_subscribe = true;
        }
        ping
    }

    /// The key and the introduction of `idx`, to hand to a device that vouches
    /// for it (a QR code between two phones).
    pub fn introduction_of(&mut self, idx: DeviceIdx) -> DeviceInfo {
        let device = &mut self.slots[idx.0].device;
        DeviceInfo {
            key: device.key(),
            introduction: device.introduction(),
        }
    }

    /// Queue an intent for a device.
    pub fn queue(&mut self, idx: DeviceIdx, intent: Intent) {
        self.slots[idx.0].device.queue(intent);
    }

    /// Queue an event of a ping for a device.
    pub fn post(&mut self, idx: DeviceIdx, ping: PingId, text: &[u8]) {
        self.queue(
            idx,
            Intent::Post {
                ping,
                text: text.to_vec(),
            },
        );
    }

    // ---- Break ----

    /// Take a device offline: it neither reads nor writes, and a wake for it is
    /// dropped. When it comes back it reads from where it stopped.
    pub fn set_offline(&mut self, idx: DeviceIdx, offline: Offline) {
        self.slots[idx.0].offline = Some(offline);
    }

    /// Restart the server: the subscriptions and the queued wakes are lost, the
    /// stored events are not. Every device subscribes again and reads.
    pub fn restart_server(&mut self) {
        self.server.restart();
        self.in_flight.clear();
        self.stats.server_restarts += 1;
        for slot in &mut self.slots {
            slot.needs_subscribe = true;
            slot.dirty = true;
        }
    }

    /// Close and open the cipher of a device from its exported state.
    pub fn restart_device(&mut self, idx: DeviceIdx) -> Result<(), CipherError> {
        let rng = self.rng.fork();
        self.slots[idx.0].device.restart(rng)
    }

    // ---- Run ----

    /// Make the device read now, whatever the scheduler would do.
    pub fn sync(&mut self, idx: DeviceIdx) -> usize {
        let now = self.now();
        let slot = &mut self.slots[idx.0];
        if slot.needs_subscribe {
            slot.device.subscribe(&mut self.server);
            slot.needs_subscribe = false;
        }
        let applied = slot.device.sync(&self.server);
        slot.dirty = false;
        slot.blocked = false;
        slot.next_poll = now + self.faults.poll_every;
        slot.synced_at = self.writes;
        self.note_alarms(idx);
        applied
    }

    /// Make the device do the intent at the front of its queue now, whatever the
    /// scheduler would do.
    pub fn act(&mut self, idx: DeviceIdx) -> Act {
        if let Some(log) = self.slots[idx.0].device.next_log()
            && self.rng.chance(self.faults.rival)
        {
            self.inject_rival(idx, log);
        }
        let act = self.slots[idx.0].device.act(&mut self.server);
        self.record(idx, &act);
        act
    }

    fn record(&mut self, idx: DeviceIdx, act: &Act) {
        let slot = &mut self.slots[idx.0];
        match act {
            Act::Written(_) => {
                self.writes += 1;
                slot.blocked = true;
                slot.dirty = true;
            }
            Act::Conflict { .. } => {
                self.stats.conflicts += 1;
                slot.blocked = true;
                slot.dirty = true;
            }
            Act::Skipped(skip) => self.stats.skipped.push((idx, skip.to_string())),
            Act::Refused(error) => self.stats.skipped.push((idx, error.to_string())),
            Act::Idle => {}
        }
    }

    /// Let another member write to `log` just before `idx` does, so that `idx`
    /// loses the position.
    fn inject_rival(&mut self, idx: DeviceIdx, log: crate::ids::LogId) {
        let candidates: Vec<DeviceIdx> = self
            .devices()
            .filter(|(other, device)| {
                *other != idx && device.is_member() && !device.is_halted(&log)
            })
            .map(|(other, _)| other)
            .filter(|other| !self.is_offline(*other))
            .collect();
        let Some(&rival) = self.rng.pick(&candidates) else {
            return;
        };
        let filler = match log {
            crate::ids::LogId::Group(_) => Intent::Invite(self.new_code()),
            crate::ids::LogId::Ping(_, ping) => Intent::Post {
                ping,
                text: b"rival".to_vec(),
            },
        };
        self.sync(rival);
        let act = self.slots[rival.0].device.run(filler, &mut self.server);
        if let Act::Written(_) = act {
            self.stats.rivals += 1;
        }
        self.record(rival, &act);
    }

    fn offline_over(&self, offline: Offline) -> bool {
        match offline {
            Offline::Until(time) => self.now() >= time,
            Offline::UntilEvents(events) => self.writes >= events,
        }
    }

    fn note_alarms(&mut self, idx: DeviceIdx) {
        if !self.slots[idx.0].device.alarms().is_empty() {
            self.first_alarm.entry(idx).or_insert(self.writes);
        }
    }

    /// Bring back the devices whose offline span is over.
    fn refresh(&mut self) {
        for i in 0..self.slots.len() {
            if self.slots[i]
                .offline
                .is_some_and(|offline| self.offline_over(offline))
            {
                let slot = &mut self.slots[i];
                slot.offline = None;
                slot.dirty = true;
                slot.needs_subscribe = true;
            }
        }
    }

    fn collect_wakes(&mut self) {
        let now = self.now();
        for wake in self.server.drain_wakes() {
            if self.rng.chance(self.faults.wake_loss) {
                self.stats.wakes_lost += 1;
                continue;
            }
            let (least, most) = self.faults.wake_delay;
            let delay = Millis(self.rng.between(least.0, most.0));
            self.in_flight.push(InFlight {
                due: now + delay,
                wake,
            });
        }
    }

    fn ready_steps(&self) -> Vec<Step> {
        let now = self.now();
        let mut ready = Vec::new();
        for (i, flight) in self.in_flight.iter().enumerate() {
            if flight.due <= now {
                ready.push(Step::Deliver(i));
            }
        }
        for (i, slot) in self.slots.iter().enumerate() {
            if slot.offline.is_some() {
                continue;
            }
            let idx = DeviceIdx(i);
            if slot.dirty || slot.needs_subscribe || slot.next_poll <= now {
                ready.push(Step::Sync(idx));
            }
            if !slot.blocked && slot.device.pending() > 0 {
                ready.push(Step::Act(idx));
            }
            if self.faults.gossip_every.is_some() && slot.next_gossip <= now {
                ready.push(Step::Gossip(idx));
            }
        }
        ready
    }

    /// Move the clock to the next time at which something is ready.
    fn advance_clock(&mut self) {
        let mut times: Vec<Millis> = self.in_flight.iter().map(|flight| flight.due).collect();
        for slot in &self.slots {
            match slot.offline {
                None => {
                    times.push(slot.next_poll);
                    if self.faults.gossip_every.is_some() {
                        times.push(slot.next_gossip);
                    }
                }
                Some(Offline::Until(time)) => times.push(time),
                Some(Offline::UntilEvents(_)) => {}
            }
        }
        match times.into_iter().min() {
            Some(target) if target > self.now() => self.clock.advance_to(target),
            Some(_) => {}
            None => self.clock.advance(self.faults.poll_every),
        }
    }

    /// One step of the scheduler.
    pub fn step(&mut self) {
        self.stats.steps += 1;
        self.refresh();
        self.collect_wakes();
        let ready = self.ready_steps();
        let Some(&step) = self.rng.pick(&ready) else {
            self.advance_clock();
            return;
        };
        match step {
            Step::Deliver(i) => {
                let flight = self.in_flight.remove(i);
                if let Some(idx) = self.index_of(&flight.wake.to)
                    && self.slots[idx.0].offline.is_none()
                {
                    self.slots[idx.0].dirty = true;
                    self.stats.wakes_delivered += 1;
                }
            }
            Step::Sync(idx) => {
                self.sync(idx);
            }
            Step::Act(idx) => {
                self.act(idx);
            }
            Step::Gossip(idx) => self.gossip(idx),
        }
    }

    fn gossip(&mut self, idx: DeviceIdx) {
        let now = self.now();
        let every = self.faults.gossip_every.unwrap_or(self.faults.poll_every);
        self.slots[idx.0].next_gossip = now + every;
        let peers: Vec<DeviceIdx> = (0..self.slots.len())
            .map(DeviceIdx)
            .filter(|peer| *peer != idx && self.slots[peer.0].offline.is_none())
            .collect();
        let Some(&peer) = self.rng.pick(&peers) else {
            return;
        };
        let heads = self.slots[idx.0].device.heads();
        self.slots[peer.0].device.receive_heads(now, &heads);
        self.slots[peer.0].dirty = true;
        self.note_alarms(peer);
    }

    /// Take steps until `done` is true.
    ///
    /// # Panics
    ///
    /// Panics when the world takes more steps than the budget without `done`
    /// becoming true: a device that waits for an event that never comes, or an
    /// intent that conflicts for ever. The message says what was waiting.
    pub fn run_until(&mut self, mut done: impl FnMut(&Self) -> bool) {
        for _ in 0..STEP_BUDGET {
            if done(self) {
                return;
            }
            self.step();
        }
        let waiting: Vec<String> = self
            .devices()
            .filter(|(_, device)| device.pending() > 0)
            .map(|(idx, device)| format!("{idx} has {} intents", device.pending()))
            .collect();
        panic!(
            "the world did not reach its goal in {STEP_BUDGET} steps at {}: {} events written, \
             {} conflicts, waiting: [{}] (see the module docs of e2e_harness::sim)",
            self.now(),
            self.writes,
            self.stats.conflicts,
            waiting.join(", ")
        );
    }

    /// True when every online device has no intent, has read everything the
    /// server accepted, and has no read to do.
    pub fn is_idle(&self) -> bool {
        self.slots.iter().all(|slot| match slot.offline {
            Some(offline) => !self.offline_over(offline),
            None => {
                slot.device.pending() == 0
                    && !slot.dirty
                    && !slot.blocked
                    && slot.synced_at == self.writes
            }
        })
    }

    /// Take steps until the world is idle (see [`World::is_idle`]).
    pub fn run_until_idle(&mut self) {
        self.run_until(Self::is_idle);
    }

    /// Take steps until `span` of virtual time has passed.
    pub fn run_for(&mut self, span: Millis) {
        let end = self.now() + span;
        self.run_until(|world| world.now() >= end);
    }

    // ---- Verbs ----

    /// A new device founds the group and the world runs until idle.
    pub fn found_group(&mut self) -> DeviceIdx {
        let founder = self.add_device();
        self.queue(founder, Intent::Found);
        self.run_until_idle();
        founder
    }

    /// A new device joins with a code that `approver` issues, and `approver`
    /// approves it (D11). Returns the new device.
    pub fn admit(&mut self, approver: DeviceIdx) -> DeviceIdx {
        let joiner = self.add_device();
        self.admit_device(approver, joiner);
        joiner
    }

    /// The device `joiner` joins with a code that `approver` issues, and
    /// `approver` approves it. Returns the new user.
    ///
    /// # Panics
    ///
    /// Panics when the join does not end with the joiner in the group, for
    /// example because `approver` is offline.
    pub fn admit_device(&mut self, approver: DeviceIdx, joiner: DeviceIdx) -> UserId {
        let code = self.new_code();
        self.queue(approver, Intent::Invite(code));
        self.run_until_idle();

        self.queue(joiner, Intent::Request(code));
        let key = self.device(joiner).key();
        self.run_until(|world| world.device(approver).state().request_of(&key).is_some());
        let request = self
            .device(approver)
            .state()
            .request_of(&key)
            .expect("the request reached the approver");

        self.queue(approver, Intent::Approve(request));
        self.run_until_idle();
        self.device(approver)
            .state()
            .roster()
            .user_of(&key)
            .unwrap_or_else(|| panic!("{joiner} is not in the group after its join (code {code})"))
    }

    /// A new device of the user of `owner`, vouched for by `owner`.
    pub fn add_device_to(&mut self, owner: DeviceIdx) -> DeviceIdx {
        let device = self.add_device();
        let info = self.introduction_of(device);
        self.queue(owner, Intent::AddDevice(info));
        self.run_until_idle();
        device
    }

    /// A new ping that `by` starts; the world runs until idle.
    pub fn start_ping(&mut self, by: DeviceIdx) -> PingId {
        let ping = self.new_ping();
        self.queue(by, Intent::StartPing(ping));
        self.run_until_idle();
        ping
    }

    /// The position of the next event of `log` as `idx` sees it.
    pub fn head_of(&self, idx: DeviceIdx, log: &crate::ids::LogId) -> Seq {
        self.device(idx).head(log)
    }

    /// The reasons for which intents were dropped (see [`Stats::skipped`]).
    pub fn skips_of(&self, idx: DeviceIdx) -> Vec<&str> {
        self.stats
            .skipped
            .iter()
            .filter(|(who, _)| *who == idx)
            .map(|(_, why)| why.as_str())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakeCipher;

    #[test]
    fn two_devices_that_read_the_same_head_race_and_the_loser_retries() {
        let mut world = World::<FakeCipher>::new(Seed(3));
        let alice = world.found_group();
        let bob = world.admit(alice);
        let carol = world.admit(alice);
        let (code_a, code_b) = (world.new_code(), world.new_code());
        world.queue(alice, Intent::Invite(code_a));
        world.queue(bob, Intent::Invite(code_b));
        world.run_until_idle();

        // Alice and Bob read the same head and both write: one loses.
        let before = world.stats().conflicts;
        let (code_c, code_d) = (world.new_code(), world.new_code());
        world.queue(alice, Intent::Invite(code_c));
        world.queue(bob, Intent::Invite(code_d));
        world.sync(alice);
        world.sync(bob);
        assert!(matches!(world.act(alice), Act::Written(_)));
        assert!(matches!(world.act(bob), Act::Conflict { .. }));
        world.run_until_idle();

        // Bob's retry went through and everyone read both.
        let digests: Vec<_> = [alice, bob, carol]
            .map(|idx| world.device(idx).group_digest())
            .into();
        assert!(digests.windows(2).all(|pair| pair[0] == pair[1]));
        assert!(world.device(carol).state().code(&code_d).is_some());
        assert_eq!(world.stats().conflicts - before, 1);
    }

    #[test]
    fn a_device_that_is_offline_catches_up_when_it_returns() {
        let mut world = World::<FakeCipher>::new(Seed(4));
        let alice = world.found_group();
        let bob = world.admit(alice);
        world.set_offline(bob, Offline::Until(world.now() + Millis::from_hours(6)));
        let code = world.new_code();
        world.queue(alice, Intent::Invite(code));
        world.run_until_idle();
        assert!(world.device(bob).state().code(&code).is_none());

        world.run_for(Millis::from_hours(7));
        world.run_until_idle();
        assert!(world.device(bob).state().code(&code).is_some());
    }

    #[test]
    fn a_lost_wake_costs_only_latency() {
        let mut world = World::<FakeCipher>::new(Seed(5));
        world.faults_mut().wake_loss = Probability::ALWAYS;
        let alice = world.found_group();
        let bob = world.admit(alice);
        let start = world.now();
        let code = world.new_code();
        world.queue(alice, Intent::Invite(code));
        world.run_until_idle();
        assert!(world.device(bob).state().code(&code).is_some());
        assert!(world.stats().wakes_lost > 0);
        assert_eq!(world.stats().wakes_delivered, 0);
        assert!(world.now() > start, "the poll, not the wake, woke Bob");
    }

    #[test]
    fn a_restart_of_the_server_is_survived() {
        let mut world = World::<FakeCipher>::new(Seed(6));
        let alice = world.found_group();
        let bob = world.admit(alice);
        world.restart_server();
        let code = world.new_code();
        world.queue(alice, Intent::Invite(code));
        world.run_until_idle();
        assert!(world.device(bob).state().code(&code).is_some());
    }

    #[test]
    fn a_device_that_is_offline_for_events_returns_after_that_many() {
        let mut world = World::<FakeCipher>::new(Seed(7));
        let alice = world.found_group();
        let bob = world.admit(alice);
        let until = world.writes() + 3;
        world.set_offline(bob, Offline::UntilEvents(until));
        for _ in 0..3 {
            let code = world.new_code();
            world.queue(alice, Intent::Invite(code));
        }
        world.run_until_idle();
        assert!(!world.is_offline(bob));
        assert_eq!(
            world.device(bob).state().digest(),
            world.device(alice).state().digest()
        );
    }
}
