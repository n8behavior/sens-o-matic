//! The evil server: a [`Log`] that lies, and how long honest devices take to notice.
//!
//! The server orders the log, so it can do four things that a signature does not
//! catch (the section "Risks" of #61). [`EvilLog`] wraps a log and does one of
//! them on demand ([`Attack`]); [`measure`] arms an attack in a group of twelve
//! devices and counts the events that pass before the affected devices raise an
//! [`Alarm`](crate::device::Alarm).
//!
//! | Attack | What the wrapper does | What notices |
//! |---|---|---|
//! | [`Attack::Drop`] | acknowledges the next append and stores nothing | the author only: its own write is missing or altered |
//! | [`Attack::HoldBack`] | hides every event from a position on from one reader | that reader, when it writes (the conflict carries the head), or when a peer tells it the head |
//! | [`Attack::Reorder`] | withholds an event from all readers but its author until the next exists, then serves the two swapped | every other reader, at once: the position of the event is signed |
//! | [`Attack::Fork`] | gives a set of devices a second history from a position on, in which their appends land | nobody, until two devices on different histories compare heads |
//!
//! # What the numbers say
//!
//! [`measure`] runs each attack in a **busy** group (every device writes in every
//! round) and a **quiet** group (one device writes), with devices that look only
//! at the log (**in band**) and with devices that also tell a random peer their
//! heads each round (**gossip**, a channel that the server does not control).
//! The scenarios `evil_server_drop`, `evil_server_hold_back`,
//! `evil_server_reorder` and `evil_server_fork` assert the results below and
//! print every row in their [`Report`](crate::scenarios::Report).
//!
//! - **Drop** is caught by the author alone, at its next read, in every
//!   configuration. No other device can notice: the chain has no gap, because the
//!   next writer simply takes the position. A dropped event is a delay that only
//!   its author sees.
//! - **Reorder** is caught by every affected reader, as soon as the second event
//!   exists, because the author signed the position. The attack cannot be done
//!   to a reader that is up to date.
//! - **Hold back** is caught in a busy group, in band, as soon as the victim
//!   writes. In a **quiet group** the victim reads, writes nothing and hears
//!   nobody: no alarm in any number of events. This is the open risk of #61.
//! - **Fork** is not caught in band, busy or quiet: each device sees a log whose
//!   chain is intact, and no event is ever shown to both sides. With gossip every
//!   device in a busy group raises an alarm within a few rounds. In a quiet
//!   group only the half that falls behind learns (it hears a head it cannot
//!   read); the writer's half cannot tell a fork from a reader that is slow.
//!
//! So the hash chain closes drop-in-the-middle and reorder, but not hold-back of
//! a silent reader and not a fork. For those, devices must compare heads outside
//! the log, or someone must write regularly in a quiet group.
//!
//! The attacks are the smallest that a server can do without a signature of its
//! own. The wrapper does not lie in the answer of a conflict, which would make
//! the victim retry at the same position for ever; a client would give up after
//! a few tries, and the harness does not model that.

use std::{cell::RefCell, collections::BTreeSet, fmt};

use crate::{
    cipher::GroupCipher,
    clock::Millis,
    event::Envelope,
    ids::{LogId, Seq},
    keys::DeviceKey,
    log::{Conflict, Log, MemoryLog, Wake},
    rng::Seed,
    sim::{DeviceIdx, World},
};

/// One thing that the evil server does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Attack {
    /// Acknowledge the next valid append to `log` and store nothing.
    Drop {
        /// The log.
        log: LogId,
    },
    /// Show `victim` only the events of `log` before position `from`.
    HoldBack {
        /// The log.
        log: LogId,
        /// The reader that is kept behind.
        victim: DeviceKey,
        /// The first position that the victim does not see.
        from: Seq,
    },
    /// Swap the events at `at` and `at + 1` of `log` for every reader but the
    /// author of the first, who always sees its own event.
    Reorder {
        /// The log.
        log: LogId,
        /// The position of the first of the two events.
        at: Seq,
    },
    /// From position `at`, give the devices of `branch` a second history of
    /// `log`: their reads and their appends go to it, and the others keep the
    /// first.
    Fork {
        /// The log.
        log: LogId,
        /// The position where the two histories part.
        at: Seq,
        /// The devices on the second history.
        branch: BTreeSet<DeviceKey>,
    },
}

#[derive(Debug)]
struct Branch<L> {
    log: LogId,
    members: BTreeSet<DeviceKey>,
    store: L,
}

/// A [`Log`] that lies. Arm an attack with [`EvilLog::arm`].
#[derive(Debug, Default)]
pub struct EvilLog<L = MemoryLog> {
    main: L,
    attacks: Vec<Attack>,
    branch: Option<Branch<L>>,
    subscriptions: BTreeSet<(DeviceKey, LogId)>,
    affected: RefCell<BTreeSet<DeviceKey>>,
}

impl<L: Log + Default> EvilLog<L> {
    /// An honest log, until an attack is armed.
    pub fn new(main: L) -> Self {
        Self {
            main,
            attacks: Vec::new(),
            branch: None,
            subscriptions: BTreeSet::new(),
            affected: RefCell::default(),
        }
    }

    /// Start an attack. It acts on the calls that follow.
    ///
    /// A fork copies the events before its position into the second history.
    pub fn arm(&mut self, attack: Attack) {
        if let Attack::Fork { log, at, branch } = attack {
            let mut store = L::default();
            let first = *branch
                .iter()
                .next()
                .expect("a fork needs a device on the second history");
            for (index, entry) in self
                .main
                .read(&first, &log, Seq::FIRST)
                .into_iter()
                .enumerate()
            {
                if index >= at.index() {
                    break;
                }
                store
                    .append(&first, &log, Seq(index as u64), entry)
                    .expect("the copy fills the positions in order");
            }
            for (device, subscribed) in &self.subscriptions {
                if *subscribed == log && branch.contains(device) {
                    store.subscribe(device, &log);
                }
            }
            self.branch = Some(Branch {
                log,
                members: branch,
                store,
            });
        } else {
            self.attacks.push(attack);
        }
    }

    /// The devices that the attacks changed what they see or what they wrote:
    /// the ones that a honest log would have served differently.
    pub fn affected(&self) -> BTreeSet<DeviceKey> {
        self.affected.borrow().clone()
    }

    /// The honest log underneath.
    pub fn inner(&self) -> &L {
        &self.main
    }

    fn on_branch(&self, caller: &DeviceKey, log: &LogId) -> bool {
        self.branch
            .as_ref()
            .is_some_and(|branch| branch.log == *log && branch.members.contains(caller))
    }

    fn touches_fork(&self, log: &LogId) -> bool {
        self.branch
            .as_ref()
            .is_some_and(|branch| branch.log == *log)
    }

    fn head(&self, caller: &DeviceKey, log: &LogId) -> Seq {
        Seq(self.route(caller, log).read(caller, log, Seq::FIRST).len() as u64)
    }

    fn route(&self, caller: &DeviceKey, log: &LogId) -> &L {
        match self.branch.as_ref().filter(|_| self.on_branch(caller, log)) {
            Some(branch) => &branch.store,
            None => &self.main,
        }
    }

    fn route_mut(&mut self, caller: &DeviceKey, log: &LogId) -> &mut L {
        if self.on_branch(caller, log)
            && let Some(branch) = self.branch.as_mut()
        {
            &mut branch.store
        } else {
            &mut self.main
        }
    }
}

impl<L: Log + Default> Log for EvilLog<L> {
    fn append(
        &mut self,
        caller: &DeviceKey,
        log: &LogId,
        expected: Seq,
        entry: Vec<u8>,
    ) -> Result<(), Conflict> {
        if self.touches_fork(log) {
            self.affected.borrow_mut().insert(*caller);
        }
        let dropping = self
            .attacks
            .iter()
            .position(|attack| matches!(attack, Attack::Drop { log: target } if target == log));
        if let Some(position) = dropping
            && expected == self.head(caller, log)
        {
            self.attacks.remove(position);
            self.affected.borrow_mut().insert(*caller);
            return Ok(());
        }
        self.route_mut(caller, log)
            .append(caller, log, expected, entry)
    }

    fn read(&self, caller: &DeviceKey, log: &LogId, from: Seq) -> Vec<Vec<u8>> {
        if self.touches_fork(log) {
            self.affected.borrow_mut().insert(*caller);
        }
        let mut entries = self.route(caller, log).read(caller, log, from);
        for attack in &self.attacks {
            match attack {
                Attack::HoldBack {
                    log: target,
                    victim,
                    from: cut,
                } if target == log && victim == caller => {
                    let visible = cut.0.saturating_sub(from.0) as usize;
                    if entries.len() > visible {
                        entries.truncate(visible);
                        self.affected.borrow_mut().insert(*caller);
                    }
                }
                Attack::Reorder { log: target, at } if target == log && from <= *at => {
                    let first = (at.0 - from.0) as usize;
                    let Some(entry) = entries.get(first) else {
                        continue;
                    };
                    let author = Envelope::from_bytes(entry)
                        .ok()
                        .map(|envelope| envelope.body().author);
                    if author.as_ref() == Some(caller) {
                        continue;
                    }
                    if entries.len() > first + 1 {
                        entries.swap(first, first + 1);
                        self.affected.borrow_mut().insert(*caller);
                    } else {
                        entries.truncate(first);
                        self.affected.borrow_mut().insert(*caller);
                    }
                }
                Attack::HoldBack { .. }
                | Attack::Reorder { .. }
                | Attack::Drop { .. }
                | Attack::Fork { .. } => {}
            }
        }
        entries
    }

    fn delete(&mut self, log: &LogId) {
        self.main.delete(log);
        if let Some(branch) = self.branch.as_mut().filter(|branch| branch.log == *log) {
            branch.store.delete(log);
        }
    }

    fn subscribe(&mut self, caller: &DeviceKey, log: &LogId) {
        self.subscriptions.insert((*caller, *log));
        self.main.subscribe(caller, log);
        if self.on_branch(caller, log)
            && let Some(branch) = self.branch.as_mut()
        {
            branch.store.subscribe(caller, log);
        }
    }

    fn drain_wakes(&mut self) -> Vec<Wake> {
        let mut wakes = self.main.drain_wakes();
        if let Some(branch) = self.branch.as_mut() {
            wakes.extend(branch.store.drain_wakes());
        }
        wakes
    }

    fn restart(&mut self) {
        self.subscriptions.clear();
        self.main.restart();
        if let Some(branch) = self.branch.as_mut() {
            branch.store.restart();
        }
    }
}

/// Which attack to measure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// [`Attack::Drop`].
    Drop,
    /// [`Attack::HoldBack`].
    HoldBack,
    /// [`Attack::Reorder`].
    Reorder,
    /// [`Attack::Fork`].
    Fork,
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Drop => "drop",
            Self::HoldBack => "hold back",
            Self::Reorder => "reorder",
            Self::Fork => "fork",
        })
    }
}

/// How much the group writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Shape {
    /// Every device writes an event in every round.
    Busy,
    /// One device writes an event in every round.
    Quiet,
}

impl fmt::Display for Shape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Busy => "busy group",
            Self::Quiet => "quiet group",
        })
    }
}

/// What the devices look at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Vigilance {
    /// Only the log: the chain, their own writes, the heads in a conflict.
    InBand,
    /// Also the heads that peers tell them, once in every round.
    Gossip,
}

impl fmt::Display for Vigilance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InBand => "in band",
            Self::Gossip => "with gossip",
        })
    }
}

/// How many devices noticed one attack, and after how many events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detection {
    /// The attack.
    pub kind: Kind,
    /// How much the group wrote.
    pub shape: Shape,
    /// What the devices looked at.
    pub vigilance: Vigilance,
    /// The devices in the group.
    pub devices: usize,
    /// The devices that the attack changed what they see or write.
    pub affected: usize,
    /// Of those, the devices that raised an alarm.
    pub alarmed: usize,
    /// Events the server accepted in the whole run, the group's building included.
    pub events: u64,
    /// Bytes the server accepted in the whole run.
    pub bytes: u64,
    /// Events accepted since the attack began, when the measure ended.
    pub events_passed: u64,
    /// Events accepted until the first alarm.
    pub events_to_first: Option<u64>,
    /// Events accepted until every affected device had raised an alarm; `None`
    /// if some did not within the rounds of the measure.
    pub events_to_all: Option<u64>,
}

impl Detection {
    /// True when at least one device was affected and every one noticed.
    pub fn all_noticed(&self) -> bool {
        self.affected > 0 && self.alarmed == self.affected
    }
}

impl fmt::Display for Detection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}, {}, {}: {} of {} affected devices of {} raised an alarm",
            self.kind, self.shape, self.vigilance, self.alarmed, self.affected, self.devices
        )?;
        match (self.events_to_first, self.events_to_all) {
            (Some(first), Some(all)) => write!(
                f,
                " (first after {}, all after {})",
                events(first),
                events(all)
            ),
            (Some(first), None) => {
                write!(
                    f,
                    " (first after {}, the rest not within {})",
                    events(first),
                    events(self.events_passed)
                )
            }
            (None, _) => write!(f, " (none within {})", events(self.events_passed)),
        }
    }
}

fn events(count: u64) -> String {
    match count {
        1 => "1 event".to_owned(),
        count => format!("{count} events"),
    }
}

const MEMBERS: usize = 6;
const ROUNDS: usize = 12;
const ROUND: Millis = Millis::from_mins(10);

/// Arm `kind` in a group of twelve devices (six members with two devices each)
/// and count the events that pass before the affected devices notice.
///
/// The group is built with the real flows (invite, request, approval), a ping is
/// started, and every device writes one event. Then the attack is armed at the
/// head of the ping log and the group goes on for at most twelve rounds of ten
/// minutes, in `shape`. The measure stops early when every affected device has
/// raised an alarm.
///
/// # Panics
///
/// Panics when a device raises an alarm before the attack: an honest server must
/// never trip the checks.
pub fn measure<C: GroupCipher>(
    seed: Seed,
    kind: Kind,
    shape: Shape,
    vigilance: Vigilance,
) -> Detection {
    let mut world = World::<C, EvilLog<MemoryLog>>::with_log(seed, EvilLog::default());
    if vigilance == Vigilance::Gossip {
        world.faults_mut().gossip_every = Some(ROUND);
    }
    let founder = world.found_group();
    let mut devices = vec![founder];
    for _ in 1..MEMBERS {
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
    assert!(
        world
            .devices()
            .all(|(_, device)| device.alarms().is_empty()),
        "an honest server raised an alarm before the attack (see the module docs of e2e_harness::evil)"
    );

    let log = world.device(founder).ping_log(ping);
    let head = world.head_of(founder, &log);
    let victim = *devices.last().expect("the group has devices");
    let key_of = |idx: &DeviceIdx| world.device(*idx).key();
    let attack = match kind {
        Kind::Drop => Attack::Drop { log },
        Kind::HoldBack => Attack::HoldBack {
            log,
            victim: key_of(&victim),
            from: head,
        },
        Kind::Reorder => Attack::Reorder { log, at: head },
        Kind::Fork => Attack::Fork {
            log,
            at: head,
            branch: devices.iter().skip(1).step_by(2).map(key_of).collect(),
        },
    };
    world.log_mut().arm(attack);

    let base = world.writes();
    let affected_alarmed = |world: &World<C, EvilLog<MemoryLog>>| {
        let affected: Vec<DeviceIdx> = world
            .log()
            .affected()
            .iter()
            .filter_map(|key| world.index_of(key))
            .collect();
        let alarmed: Vec<DeviceIdx> = affected
            .iter()
            .copied()
            .filter(|idx| !world.device(*idx).alarms().is_empty())
            .collect();
        (affected, alarmed)
    };
    for round in 0..ROUNDS {
        let writers = match shape {
            Shape::Busy => &devices[..],
            Shape::Quiet => &devices[..1],
        };
        for writer in writers {
            world.post(*writer, ping, format!("round {round}").as_bytes());
        }
        world.run_for(ROUND);
        let (affected, alarmed) = affected_alarmed(&world);
        if !affected.is_empty() && affected.len() == alarmed.len() {
            break;
        }
    }

    let (affected, alarmed) = affected_alarmed(&world);
    let firsts: Vec<u64> = alarmed
        .iter()
        .filter_map(|idx| world.first_alarm(*idx))
        .map(|writes| writes - base)
        .collect();
    let everyone = !affected.is_empty() && affected.len() == alarmed.len();
    Detection {
        kind,
        shape,
        vigilance,
        devices: devices.len(),
        affected: affected.len(),
        alarmed: alarmed.len(),
        events: world.server().totals().0,
        bytes: world.server().totals().1,
        events_passed: world.writes() - base,
        events_to_first: firsts.iter().copied().min(),
        events_to_all: everyone.then(|| firsts.iter().copied().max()).flatten(),
    }
}
