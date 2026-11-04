//! A device: what an honest client does with the server and its cipher.
//!
//! A [`Device`] reads the logs it follows, checks them, applies them, and writes
//! when it has something to say. It is a state machine without a thread: the
//! simulator ([`crate::sim`]) calls [`Device::sync`] and [`Device::act`] one at
//! a time, in the order that the seeded scheduler picks.
//!
//! # What a device checks
//!
//! Every entry that the server serves is checked before it is applied:
//!
//! 1. it is an event, with a good signature of its author;
//! 2. it belongs to the log that was asked for;
//! 3. its position is the one the device expects next (a **drop** or a
//!    **reorder** shows here);
//! 4. its `prev` is the hash of the event before, as the device stored it (a
//!    **splice** or a **fork** that reaches the device shows here);
//! 5. its timestamp is not older than the one before.
//!
//! A failed check is an [`Alarm`]. The device stops reading that log (it halts
//! it) and keeps the alarm for the test; a real client would tell the person.
//!
//! Three more checks compare what the device believes with what it hears:
//!
//! - **Its own write.** After an append is acknowledged, the next read must show
//!   that event at that position with the hash the device computed. A server
//!   that acknowledges and discards ([`crate::evil::Attack::Drop`]) is caught
//!   here, by the author only.
//! - **A conflict.** A refused append carries the head of the log. A device
//!   that cannot read up to that head after its next read has been held back.
//! - **Heads from other devices.** [`Device::heads`] and
//!   [`Device::receive_heads`] model a channel that the server does not control
//!   (a message between two phones). A peer whose head is ahead of what the
//!   server shows is a hold-back, unless the device reaches that position and
//!   finds the same hash; a peer whose hash at a position the device has (or
//!   later reaches) is different is a fork. The channel is assumed to be
//!   authentic: a peer that lies about its head can make a device halt a log,
//!   so a real client would sign the heads it tells.
//!
//! What this cannot see is the open risk of #61: a server that keeps two groups
//! of readers on two histories forever, and a hold-back of a reader who never
//! writes and never hears from a peer. See [`crate::evil`] for the numbers.
//!
//! # What a device applies
//!
//! In the group log, an entry that passes the checks goes through the rules
//! ([`crate::rules`]); an event the rules refuse, or the cipher refuses, is kept
//! in [`Device::ignored`] and changes nothing. In a ping log, the first event
//! opens the ping and the others are decrypted; one the cipher cannot read is
//! kept as unreadable. A ping log is read only after the device has been
//! admitted to the group, and from its first event.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use thiserror::Error;

use crate::{
    cipher::{CipherError, GroupCipher, GroupEvent},
    clock::Millis,
    event::{Draft, Envelope},
    ids::{GroupId, InviteCode, LogId, Material, PingId, Seq, UserId},
    keys::{DeviceKey, EventHash, Identity},
    log::Log,
    payload::{GroupChange, GroupPayload, PingOpen},
    rng::SimRng,
    roster::DeviceInfo,
    rules::{GroupState, Refusal},
    server::{Receipt, Server, SubmitError},
};

/// A check that failed: the server, or the log, is not what an honest one would
/// give. See the module docs for the checks.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Alarm {
    /// The entry is not an event.
    #[error(
        "log {log}: the entry at {seq} is not an event (see the module docs of e2e_harness::device)"
    )]
    Malformed {
        /// The log that was read.
        log: LogId,
        /// The position that was due.
        seq: Seq,
    },
    /// The signature does not match.
    #[error(
        "log {log}: the event at {seq} has a bad signature (see the module docs of e2e_harness::device)"
    )]
    BadSignature {
        /// The log that was read.
        log: LogId,
        /// The position that was due.
        seq: Seq,
    },
    /// The event says it belongs to another log.
    #[error(
        "log {log}: the event at {seq} says it belongs to {claimed} (see the module docs of e2e_harness::device)"
    )]
    WrongLog {
        /// The log that was read.
        log: LogId,
        /// The position that was due.
        seq: Seq,
        /// The log the event names.
        claimed: LogId,
    },
    /// The event claims another position than the one due: an event was
    /// dropped (the claim is later) or served again or out of order.
    #[error(
        "log {log}: {expected} was due but the server served the event of {got}: an event was dropped, repeated or reordered (see the module docs of e2e_harness::device)"
    )]
    OutOfPlace {
        /// The log that was read.
        log: LogId,
        /// The position that was due.
        expected: Seq,
        /// The position the event claims.
        got: Seq,
    },
    /// The event names another hash than the event before it.
    #[error(
        "log {log}: the event at {seq} does not follow the event the device stored before it: the log was spliced or forked (see the module docs of e2e_harness::device)"
    )]
    BrokenChain {
        /// The log that was read.
        log: LogId,
        /// The position of the event.
        seq: Seq,
    },
    /// The timestamp is older than the one of the event before.
    #[error(
        "log {log}: the event at {seq} is older than the event before it (see the module docs of e2e_harness::device)"
    )]
    TimeWentBack {
        /// The log that was read.
        log: LogId,
        /// The position of the event.
        seq: Seq,
    },
    /// The event at the position of the device's own acknowledged write is
    /// another event.
    #[error(
        "log {log}: the event at {seq} is not the one this device wrote there (see the module docs of e2e_harness::device)"
    )]
    OwnWriteAltered {
        /// The log that was read.
        log: LogId,
        /// The position of the write.
        seq: Seq,
    },
    /// The server acknowledged the device's write and does not show it.
    #[error(
        "log {log}: the server acknowledged the write at {seq} and does not show it (see the module docs of e2e_harness::device)"
    )]
    OwnWriteMissing {
        /// The log that was read.
        log: LogId,
        /// The position of the write.
        seq: Seq,
    },
    /// A conflict or a peer says the log is longer than the server shows.
    #[error(
        "log {log}: the device can read up to {seen} but knows the head is at least {known}: events are held back (see the module docs of e2e_harness::device)"
    )]
    HeldBack {
        /// The log that was read.
        log: LogId,
        /// The head the device can read.
        seen: Seq,
        /// The head it knows from a conflict or a peer.
        known: Seq,
    },
    /// A peer has another event at a position the device also has.
    #[error(
        "log {log}: a peer has another event at {seq}: the server shows two histories (see the module docs of e2e_harness::device)"
    )]
    Fork {
        /// The log that was compared.
        log: LogId,
        /// The position where the hashes differ.
        seq: Seq,
    },
}

impl Alarm {
    /// The log the alarm is about.
    pub fn log(&self) -> LogId {
        match self {
            Self::Malformed { log, .. }
            | Self::BadSignature { log, .. }
            | Self::WrongLog { log, .. }
            | Self::OutOfPlace { log, .. }
            | Self::BrokenChain { log, .. }
            | Self::TimeWentBack { log, .. }
            | Self::OwnWriteAltered { log, .. }
            | Self::OwnWriteMissing { log, .. }
            | Self::HeldBack { log, .. }
            | Self::Fork { log, .. } => *log,
        }
    }
}

/// Why an accepted-looking event of the group log changed nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ignored {
    /// A rule refused it ([`crate::rules`]).
    Rule(Refusal),
    /// The cipher refused it ([`crate::cipher::GroupCipher::apply_group_event`]).
    Cipher(CipherError),
}

/// What a device means to do next. A test queues intents; the scheduler runs
/// them one at a time when the device acts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Intent {
    /// Make the group; the device becomes the first device of the first user.
    Found,
    /// Issue a single-use code.
    Invite(InviteCode),
    /// Cancel a code that is not used yet.
    Revoke(InviteCode),
    /// Ask to join with a code (the device is not a member).
    Request(InviteCode),
    /// Approve the join request at this position of the group log.
    Approve(Seq),
    /// Vouch for a new device of the same user.
    AddDevice(DeviceInfo),
    /// Say that the user leaves.
    Leave,
    /// Remove another user, who leaves or is removed.
    Remove(UserId),
    /// Start a ping: write the first event of its log.
    StartPing(PingId),
    /// Write an event of a ping.
    Post {
        /// The ping.
        ping: PingId,
        /// What to say.
        text: Vec<u8>,
    },
    /// Write a group event with no checks and no material, as a device that
    /// breaks the rules would. For tests of the rules.
    Forge(GroupChange),
}

impl Intent {
    /// The log this intent writes to.
    pub fn log(&self, group: GroupId) -> LogId {
        match self {
            Self::StartPing(ping) | Self::Post { ping, .. } => LogId::Ping(group, *ping),
            Self::Found
            | Self::Invite(_)
            | Self::Revoke(_)
            | Self::Request(_)
            | Self::Approve(_)
            | Self::AddDevice(_)
            | Self::Leave
            | Self::Remove(_)
            | Self::Forge(_) => LogId::Group(group),
        }
    }
}

/// Why a device gave up an intent without writing.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Skip {
    /// The device is not a member of the group.
    #[error("the device is not a member of the group")]
    NotAMember,
    /// The device is a member already (a join request).
    #[error("the device is a member already")]
    AlreadyMember,
    /// The device has already asked with a code and waits.
    #[error("the device has a join request that waits")]
    AlreadyRequested,
    /// The group log is not empty.
    #[error("the group log is not empty, so it cannot be founded")]
    AlreadyFounded,
    /// No join request waits at that position.
    #[error("no join request waits at {0}")]
    NoSuchRequest(Seq),
    /// No such user in the group.
    #[error("no user {0} in the group")]
    NoSuchUser(UserId),
    /// A user cannot remove themselves.
    #[error("a user cannot remove themselves")]
    SelfRemoval,
    /// The ping log has events: it was started.
    #[error("the log of {0} has events already")]
    PingStarted(PingId),
    /// The device cannot read the key event of the ping.
    #[error("the device has not opened {0}")]
    PingNotOpen(PingId),
    /// The device halted the log after an alarm.
    #[error("the device halted {0} after an alarm")]
    Halted(LogId),
    /// The cipher could not make the event.
    #[error(transparent)]
    Cipher(#[from] CipherError),
}

/// The result of one [`Device::act`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Act {
    /// There was no intent.
    Idle,
    /// The server accepted the event.
    Written(Receipt),
    /// A rival won the position; the intent stays queued and the device must
    /// read before it tries again.
    Conflict {
        /// The log.
        log: LogId,
        /// The head the server reported.
        head: Seq,
    },
    /// The intent no longer makes sense for the state of the device and was
    /// dropped.
    Skipped(Skip),
    /// The server refused the draft for another reason; the intent was dropped.
    Refused(SubmitError),
}

/// The heads of the logs a device has read: what it tells a peer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Heads(BTreeMap<LogId, (Seq, EventHash)>);

/// What a device has read of one ping.
#[derive(Debug, Clone, Default)]
pub struct PingView {
    open: Option<PingOpen>,
    opened: bool,
    readable: BTreeMap<Seq, Vec<u8>>,
    unreadable: BTreeSet<Seq>,
}

impl PingView {
    /// The first event of the ping log, if it was read and understood.
    pub fn open(&self) -> Option<&PingOpen> {
        self.open.as_ref()
    }

    /// True when the cipher opened the key event, so the device can write.
    pub fn is_opened(&self) -> bool {
        self.opened
    }

    /// The events that the cipher decrypted, by position.
    pub fn readable(&self) -> &BTreeMap<Seq, Vec<u8>> {
        &self.readable
    }

    /// The positions of the events that the cipher could not decrypt.
    pub fn unreadable(&self) -> &BTreeSet<Seq> {
        &self.unreadable
    }

    /// A hash of what the device can read: equal for two devices exactly when
    /// they read the same events with the same plaintext.
    pub fn digest(&self) -> EventHash {
        let mut w = crate::codec::Writer::new();
        for (seq, plain) in &self.readable {
            w.u64(seq.0).bytes(plain);
        }
        EventHash::of(&w.finish())
    }
}

#[derive(Debug, Clone)]
struct Cursor {
    next: Seq,
    head: EventHash,
    last_time: Millis,
    hashes: Vec<EventHash>,
}

impl Cursor {
    fn new() -> Self {
        Self {
            next: Seq::FIRST,
            head: EventHash::GENESIS,
            last_time: Millis::ZERO,
            hashes: Vec::new(),
        }
    }
}

/// One device of one user, with its cipher.
#[derive(Debug)]
pub struct Device<C> {
    identity: Identity,
    key: DeviceKey,
    group: GroupId,
    cipher: C,
    state: GroupState,
    cursors: BTreeMap<LogId, Cursor>,
    pings: BTreeMap<PingId, PingView>,
    followed: BTreeSet<PingId>,
    joined: bool,
    intents: VecDeque<Intent>,
    alarms: Vec<(Millis, Alarm)>,
    halted: BTreeSet<LogId>,
    acks: BTreeMap<(LogId, Seq), EventHash>,
    at_least: BTreeMap<LogId, Seq>,
    peer_hashes: BTreeMap<(LogId, Seq), BTreeSet<EventHash>>,
    ignored: Vec<(Seq, Ignored)>,
}

impl<C: GroupCipher> Device<C> {
    /// A device of `group` with a fresh cipher. It knows no event yet.
    pub fn new(identity: Identity, group: GroupId, rng: SimRng) -> Self {
        let cipher = C::new(&identity, rng);
        Self {
            key: identity.device_key(),
            identity,
            group,
            cipher,
            state: GroupState::default(),
            cursors: BTreeMap::new(),
            pings: BTreeMap::new(),
            followed: BTreeSet::new(),
            joined: false,
            intents: VecDeque::new(),
            alarms: Vec::new(),
            halted: BTreeSet::new(),
            acks: BTreeMap::new(),
            at_least: BTreeMap::new(),
            peer_hashes: BTreeMap::new(),
            ignored: Vec::new(),
        }
    }

    /// The public key of the device.
    pub fn key(&self) -> DeviceKey {
        self.key
    }

    /// The group log of the device's group.
    pub fn group_log(&self) -> LogId {
        LogId::Group(self.group)
    }

    /// The log of one ping of the device's group.
    pub fn ping_log(&self, ping: PingId) -> LogId {
        LogId::Ping(self.group, ping)
    }

    /// What the device derived from the group log it has read.
    pub fn state(&self) -> &GroupState {
        &self.state
    }

    /// True when some user vouches for this device.
    pub fn is_member(&self) -> bool {
        self.state.roster().contains(&self.key)
    }

    /// The user that vouches for this device.
    pub fn user(&self) -> Option<UserId> {
        self.state.roster().user_of(&self.key)
    }

    /// True once the device has been a member, even if it was removed since:
    /// such a device still reads the logs, and what it can read is up to the
    /// cipher.
    pub fn has_joined(&self) -> bool {
        self.joined
    }

    /// The introduction of this device, made by its cipher.
    pub fn introduction(&mut self) -> Material {
        self.cipher.introduction()
    }

    /// Read the ping logs too, once the device has joined.
    pub fn follow(&mut self, ping: PingId) {
        self.followed.insert(ping);
    }

    /// What the device has read of `ping`.
    pub fn ping(&self, ping: PingId) -> Option<&PingView> {
        self.pings.get(&ping)
    }

    /// True when the device has applied a removal after `ping` began (D10).
    /// `None` when the device has not read the first event of the ping.
    pub fn ping_ended(&self, ping: PingId) -> Option<bool> {
        let open = self.pings.get(&ping)?.open()?;
        Some(self.state.ping_ended(open))
    }

    /// The position the device will read next in `log`: its view of the head.
    pub fn head(&self, log: &LogId) -> Seq {
        self.cursors
            .get(log)
            .map_or(Seq::FIRST, |cursor| cursor.next)
    }

    /// The alarms the device raised, with the time of each.
    pub fn alarms(&self) -> &[(Millis, Alarm)] {
        &self.alarms
    }

    /// The group events that changed nothing, with the reason.
    pub fn ignored(&self) -> &[(Seq, Ignored)] {
        &self.ignored
    }

    /// True when the device stopped reading `log` after an alarm.
    pub fn is_halted(&self, log: &LogId) -> bool {
        self.halted.contains(log)
    }

    /// A hash of the group state and of the events that were ignored: equal for
    /// two devices exactly when they applied the same group log alike.
    pub fn group_digest(&self) -> EventHash {
        let mut w = crate::codec::Writer::new();
        w.raw(self.state.digest().as_bytes());
        for (seq, ignored) in &self.ignored {
            w.u64(seq.0);
            match ignored {
                Ignored::Rule(refusal) => w.u8(0).bytes(format!("{refusal:?}").as_bytes()),
                Ignored::Cipher(_) => w.u8(1),
            };
        }
        EventHash::of(&w.finish())
    }

    /// The cipher, to ask it something a test needs.
    pub fn cipher(&self) -> &C {
        &self.cipher
    }

    /// Stop and start the cipher from its exported state, as an app does when it
    /// is closed and opened again.
    pub fn restart(&mut self, rng: SimRng) -> Result<(), CipherError> {
        let state = self.cipher.export();
        self.cipher = C::restore(&self.identity, rng, &state)?;
        Ok(())
    }

    /// Queue an intent.
    pub fn queue(&mut self, intent: Intent) {
        self.intents.push_back(intent);
    }

    /// The number of intents that wait.
    pub fn pending(&self) -> usize {
        self.intents.len()
    }

    /// The log that the next intent writes to.
    pub fn next_log(&self) -> Option<LogId> {
        self.intents.front().map(|intent| intent.log(self.group))
    }

    /// Ask the server for a wake on every log the device reads.
    pub fn subscribe<L: Log>(&self, server: &mut Server<L>) {
        server.subscribe(&self.key, &self.group_log());
        for ping in &self.followed {
            server.subscribe(&self.key, &self.ping_log(*ping));
        }
    }

    /// The heads of the logs the device has read, to tell a peer.
    pub fn heads(&self) -> Heads {
        Heads(
            self.cursors
                .iter()
                .filter(|(_, cursor)| cursor.next > Seq::FIRST)
                .map(|(log, cursor)| (*log, (cursor.next, cursor.head)))
                .collect(),
        )
    }

    /// Compare the heads of a peer with what this device has read.
    pub fn receive_heads(&mut self, now: Millis, heads: &Heads) {
        for (log, (peer_next, peer_head)) in &heads.0 {
            let Some(last) = peer_next.0.checked_sub(1).map(Seq) else {
                continue;
            };
            if self.halted.contains(log) {
                continue;
            }
            let cursor = self.cursors.get(log);
            if last < cursor.map_or(Seq::FIRST, |cursor| cursor.next) {
                let differs =
                    cursor.is_some_and(|cursor| cursor.hashes[last.index()] != *peer_head);
                if differs {
                    self.raise(
                        now,
                        Alarm::Fork {
                            log: *log,
                            seq: last,
                        },
                    );
                }
            } else {
                let known = self.at_least.entry(*log).or_insert(*peer_next);
                *known = (*known).max(*peer_next);
                self.peer_hashes
                    .entry((*log, last))
                    .or_default()
                    .insert(*peer_head);
            }
        }
    }

    /// Read every log the device follows from where it stopped, check each entry
    /// and apply it. Returns the number of events applied.
    pub fn sync<L: Log>(&mut self, server: &Server<L>) -> usize {
        let mut applied = self.sync_log(server, self.group_log());
        if self.joined {
            let pings: Vec<PingId> = self.followed.iter().copied().collect();
            for ping in pings {
                applied += self.sync_log(server, self.ping_log(ping));
            }
        }
        applied
    }

    fn sync_log<L: Log>(&mut self, server: &Server<L>, log: LogId) -> usize {
        if self.halted.contains(&log) {
            return 0;
        }
        let now = server.now();
        let from = self.head(&log);
        let mut applied = 0;
        for bytes in server.fetch(&self.key, &log, from) {
            match self.accept(&log, &bytes) {
                Ok(envelope) => {
                    self.apply(&log, &envelope);
                    applied += 1;
                }
                Err(alarm) => {
                    self.raise(now, alarm);
                    return applied;
                }
            }
        }
        let next = self.head(&log);
        if let Some(known) = self
            .at_least
            .get(&log)
            .copied()
            .filter(|known| next < *known)
        {
            self.raise(
                now,
                Alarm::HeldBack {
                    log,
                    seen: next,
                    known,
                },
            );
        } else if let Some(((_, seq), _)) = self
            .acks
            .range((log, Seq::FIRST)..=(log, Seq(u64::MAX)))
            .next()
        {
            let seq = *seq;
            self.raise(now, Alarm::OwnWriteMissing { log, seq });
        }
        applied
    }

    fn raise(&mut self, now: Millis, alarm: Alarm) {
        self.halted.insert(alarm.log());
        self.alarms.push((now, alarm));
    }

    /// Run the checks of the module docs on one entry and, if they pass, move
    /// the cursor past it.
    fn accept(&mut self, log: &LogId, bytes: &[u8]) -> Result<Envelope, Alarm> {
        let cursor = self.cursors.entry(*log).or_insert_with(Cursor::new);
        let seq = cursor.next;
        let envelope =
            Envelope::from_bytes(bytes).map_err(|_| Alarm::Malformed { log: *log, seq })?;
        envelope
            .verify()
            .map_err(|_| Alarm::BadSignature { log: *log, seq })?;
        let body = envelope.body();
        if body.log != *log {
            return Err(Alarm::WrongLog {
                log: *log,
                seq,
                claimed: body.log,
            });
        }
        if body.seq != seq {
            return Err(Alarm::OutOfPlace {
                log: *log,
                expected: seq,
                got: body.seq,
            });
        }
        if body.prev != cursor.head {
            return Err(Alarm::BrokenChain { log: *log, seq });
        }
        if envelope.timestamp() < cursor.last_time {
            return Err(Alarm::TimeWentBack { log: *log, seq });
        }
        let hash = envelope.hash();
        if let Some(wrote) = self.acks.remove(&(*log, seq))
            && wrote != hash
        {
            return Err(Alarm::OwnWriteAltered { log: *log, seq });
        }
        if let Some(told) = self.peer_hashes.remove(&(*log, seq))
            && told.iter().any(|peer| *peer != hash)
        {
            return Err(Alarm::Fork { log: *log, seq });
        }
        let cursor = self
            .cursors
            .get_mut(log)
            .expect("the cursor was made above");
        cursor.next = seq.next();
        cursor.head = hash;
        cursor.last_time = envelope.timestamp();
        cursor.hashes.push(hash);
        Ok(envelope)
    }

    fn apply(&mut self, log: &LogId, envelope: &Envelope) {
        let body = envelope.body();
        match log {
            LogId::Group(_) => self.apply_group(body.seq, &body.author, &body.payload),
            LogId::Ping(_, ping) => self.apply_ping(*ping, body.seq, &body.payload),
        }
    }

    fn apply_group(&mut self, seq: Seq, author: &DeviceKey, payload: &[u8]) {
        let Ok(payload) = GroupPayload::decode(payload) else {
            self.ignored.push((seq, Ignored::Rule(Refusal::Malformed)));
            return;
        };
        let mut next = self.state.clone();
        if let Err(refusal) = next.apply(seq, author, &payload.change) {
            self.ignored.push((seq, Ignored::Rule(refusal)));
            return;
        }
        if next.roster().contains(&self.key) {
            let event = GroupEvent {
                seq,
                author,
                change: &payload.change,
                material: &payload.material,
                roster: next.roster(),
            };
            if let Err(error) = self.cipher.apply_group_event(&event) {
                self.ignored.push((seq, Ignored::Cipher(error)));
                return;
            }
            self.joined = true;
        }
        self.state = next;
    }

    fn apply_ping(&mut self, ping: PingId, seq: Seq, payload: &[u8]) {
        if seq == Seq::FIRST {
            let view = self.pings.entry(ping).or_default();
            if let Ok(open) = PingOpen::decode(payload) {
                view.opened = self
                    .cipher
                    .open_ping(ping, open.group_head, &open.material)
                    .is_ok();
                view.open = Some(open);
            }
            return;
        }
        let result = self.cipher.decrypt_ping(ping, seq, payload);
        let view = self.pings.entry(ping).or_default();
        match result {
            Ok(plaintext) => {
                view.readable.insert(seq, plaintext);
            }
            Err(_) => {
                view.unreadable.insert(seq);
            }
        }
    }

    /// Do the intent at the front of the queue, from the state the device has now.
    ///
    /// The device does not read first: the scheduler decides when, so that two
    /// devices can read the same head and then both write (a real race). A
    /// conflict leaves the intent queued; the device must [`sync`](Self::sync)
    /// before it acts again, or it would lose again.
    pub fn act<L: Log>(&mut self, server: &mut Server<L>) -> Act {
        let Some(intent) = self.intents.pop_front() else {
            return Act::Idle;
        };
        let act = self.run(intent.clone(), server);
        if let Act::Conflict { .. } = act {
            self.intents.push_front(intent);
        }
        act
    }

    /// Do `intent` now, from the state the device has, without the queue. A
    /// conflict is reported and the intent is not kept.
    pub fn run<L: Log>(&mut self, intent: Intent, server: &mut Server<L>) -> Act {
        let (log, payload) = match self.plan(&intent) {
            Ok(planned) => planned,
            Err(skip) => return Act::Skipped(skip),
        };
        let cursor = self.cursors.entry(log).or_insert_with(Cursor::new);
        let draft = Draft::sign(log, cursor.next, cursor.head, payload, &self.identity);
        match server.submit(&self.key, draft.clone()) {
            Ok(receipt) => {
                let stored = Envelope::stamp(draft, receipt.timestamp)
                    .expect("the draft was made by this device, so it reads back");
                self.acks.insert((log, receipt.seq), stored.hash());
                Act::Written(receipt)
            }
            Err(SubmitError::Conflict(conflict)) => {
                let known = self.at_least.entry(log).or_insert(conflict.head);
                *known = (*known).max(conflict.head);
                Act::Conflict {
                    log,
                    head: conflict.head,
                }
            }
            Err(error) => Act::Refused(error),
        }
    }

    fn plan(&mut self, intent: &Intent) -> Result<(LogId, Vec<u8>), Skip> {
        let log = intent.log(self.group);
        if self.halted.contains(&log) {
            return Err(Skip::Halted(log));
        }
        let member = self.is_member();
        let group_head = self.head(&self.group_log());
        let group = |change: GroupChange, material: Material| {
            Ok((log, GroupPayload { change, material }.encode()))
        };
        match intent {
            Intent::Found => {
                if group_head != Seq::FIRST {
                    return Err(Skip::AlreadyFounded);
                }
                let introduction = self.cipher.introduction();
                let material = self.cipher.create_group(self.group)?;
                group(GroupChange::Create { introduction }, material)
            }
            Intent::Invite(code) => {
                if !member {
                    return Err(Skip::NotAMember);
                }
                let material = self.cipher.invite(code)?;
                group(GroupChange::Invite { code: *code }, material)
            }
            Intent::Revoke(code) => {
                if !member {
                    return Err(Skip::NotAMember);
                }
                let material = self.cipher.revoke_code(code)?;
                group(GroupChange::Revoke { code: *code }, material)
            }
            Intent::Request(code) => {
                if member {
                    return Err(Skip::AlreadyMember);
                }
                if self.state.request_of(&self.key).is_some() {
                    return Err(Skip::AlreadyRequested);
                }
                let introduction = self.cipher.introduction();
                group(
                    GroupChange::JoinRequest {
                        code: *code,
                        introduction,
                    },
                    Material::empty(),
                )
            }
            Intent::Approve(request) => {
                if !member {
                    return Err(Skip::NotAMember);
                }
                let joiner = self
                    .state
                    .request(*request)
                    .ok_or(Skip::NoSuchRequest(*request))?
                    .device
                    .clone();
                let material = self.cipher.approve_join(self.state.roster(), &joiner)?;
                group(GroupChange::Approve { request: *request }, material)
            }
            Intent::AddDevice(device) => {
                if !member {
                    return Err(Skip::NotAMember);
                }
                if self.state.roster().contains(&device.key) {
                    return Err(Skip::AlreadyMember);
                }
                let material = self.cipher.add_device(self.state.roster(), device)?;
                group(
                    GroupChange::AddDevice {
                        device: device.key,
                        introduction: device.introduction.clone(),
                    },
                    material,
                )
            }
            Intent::Leave => {
                if !member {
                    return Err(Skip::NotAMember);
                }
                group(GroupChange::Leave, Material::empty())
            }
            Intent::Remove(user) => {
                let me = self.user().ok_or(Skip::NotAMember)?;
                if !self.state.roster().contains_user(*user) {
                    return Err(Skip::NoSuchUser(*user));
                }
                if *user == me {
                    return Err(Skip::SelfRemoval);
                }
                let material = self.cipher.remove_member(self.state.roster(), *user)?;
                group(GroupChange::Remove { user: *user }, material)
            }
            Intent::StartPing(ping) => {
                if !member {
                    return Err(Skip::NotAMember);
                }
                if self.head(&log) != Seq::FIRST {
                    return Err(Skip::PingStarted(*ping));
                }
                let material = self.cipher.start_ping(*ping, group_head)?;
                let open = PingOpen {
                    group_head,
                    material,
                };
                Ok((log, open.encode()))
            }
            Intent::Post { ping, text } => {
                if !member {
                    return Err(Skip::NotAMember);
                }
                if !self.pings.get(ping).is_some_and(PingView::is_opened) {
                    return Err(Skip::PingNotOpen(*ping));
                }
                Ok((log, self.cipher.encrypt_ping(*ping, text)?))
            }
            Intent::Forge(change) => group(change.clone(), Material::empty()),
        }
    }
}
