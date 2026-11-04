//! The storage of the logs (D7 of #61).
//!
//! [`Log`] is the whole contract between the server and its storage: append an
//! entry at an expected position, read from a position, delete a log, and
//! subscribe to a wake. It is synchronous and has no runtime and no threads, so
//! the simulator drives it one call at a time. [`MemoryLog`] is the backend of
//! the simulator.
//!
//! # The shape fits one object for each event
//!
//! The trait is written so that a store of objects can implement it without
//! state of its own, as D7 proposes:
//!
//! | Method | On an object store (for example GCS) |
//! |---|---|
//! | [`Log::append`] | create the object `<group>/<log>/<seq>` with `ifGenerationMatch=0`; a `412` is a [`Conflict`], and the head comes from the list of the prefix after the failed name |
//! | [`Log::read`] | `GET` the names `<from>`, `<from+1>`, … until a `404`; or list from `startOffset=<from>` |
//! | [`Log::delete`] | delete the objects under the prefix of the log (retention deletes a ping log as a whole, D8) |
//! | [`Log::subscribe`] | register a stream or a push token; see below |
//! | [`Log::restart`] | a new instance of the server: nothing in memory, the objects stay |
//!
//! The conditional create is the only lock: two instances may append to one log,
//! and the loser of a race retries at the next position. An append needs no
//! head call, because a cold writer finds the head by listing and a hint is
//! never the truth (D7); [`Log::head`] is for tests and for wrappers, and the
//! provided method reads the whole log.
//!
//! # A wake carries no payload
//!
//! A [`Wake`] says "this log has a new head". It carries no event and no
//! content (D3 and "Other consequences" of #61), so the server learns nothing
//! from it that it does not know, and a lost wake costs only latency: the device
//! also polls. Wakes are queued in memory and taken with [`Log::drain_wakes`];
//! the simulator delays or loses them on purpose.
//!
//! # Who calls
//!
//! Every call names the `caller`, the device key of its authorization token
//! (D4 b). An honest log ignores it. [`crate::evil::EvilLog`] uses it to show
//! two callers two different logs.
//!
//! ```
//! use e2e_harness::ids::{GroupId, LogId, Seq};
//! use e2e_harness::keys::Identity;
//! use e2e_harness::log::{Conflict, Log, MemoryLog};
//! use e2e_harness::rng::{Seed, SimRng};
//!
//! let me = Identity::generate(&mut SimRng::new(Seed(1))).device_key();
//! let log = LogId::Group(GroupId(1));
//! let mut store = MemoryLog::default();
//!
//! assert_eq!(store.append(&me, &log, Seq(0), b"first".to_vec()), Ok(()));
//! // A rival that also expected position 0 loses and learns the head.
//! assert_eq!(
//!     store.append(&me, &log, Seq(0), b"rival".to_vec()),
//!     Err(Conflict { head: Seq(1) })
//! );
//! assert_eq!(store.read(&me, &log, Seq(0)), vec![b"first".to_vec()]);
//! assert!(store.read(&me, &log, Seq(1)).is_empty());
//! ```

use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

use crate::{
    ids::{LogId, Seq},
    keys::DeviceKey,
};

/// An append lost: the expected position was not the head.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error(
    "the log has head {head}: another append won the position (see the module docs of e2e_harness::log)"
)]
pub struct Conflict {
    /// The position the next append must use.
    pub head: Seq,
}

/// "This log has a new head", for one subscriber. No payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Wake {
    /// The device to tell.
    pub to: DeviceKey,
    /// The log that grew.
    pub log: LogId,
    /// The head of the log when the wake was made.
    pub head: Seq,
}

/// The storage of the logs of a server.
///
/// An entry is opaque bytes: the log never looks inside (it stores
/// [`crate::event::Envelope::to_bytes`]). See the module docs for how an object
/// store implements it.
pub trait Log {
    /// Append `entry` as the event at position `expected` of `log`.
    ///
    /// Succeeds only when `expected` is the head of the log, so that two writers
    /// never fill one position. On success every subscriber of `log` gets a
    /// wake, except `caller`.
    fn append(
        &mut self,
        caller: &DeviceKey,
        log: &LogId,
        expected: Seq,
        entry: Vec<u8>,
    ) -> Result<(), Conflict>;

    /// The entries of `log` from position `from` to the head, in order. Empty
    /// when `from` is the head or past it.
    fn read(&self, caller: &DeviceKey, log: &LogId, from: Seq) -> Vec<Vec<u8>>;

    /// The position the next append needs: the number of events of `log`.
    ///
    /// The provided method reads the whole log. An implementation that can do
    /// better overrides it; on an object store that is the hint and a list from
    /// it (D7).
    fn head(&self, caller: &DeviceKey, log: &LogId) -> Seq {
        Seq(self.read(caller, log, Seq::FIRST).len() as u64)
    }

    /// Delete `log` as a whole (retention, D8). Nothing is truncated.
    fn delete(&mut self, log: &LogId);

    /// Ask for a wake each time `log` grows.
    fn subscribe(&mut self, caller: &DeviceKey, log: &LogId);

    /// Take the wakes that are queued, oldest log first. Several wakes for one
    /// subscriber and one log are one wake with the newest head.
    fn drain_wakes(&mut self) -> Vec<Wake>;

    /// Lose what a process keeps in memory: the subscriptions and the queued
    /// wakes. The entries stay, as objects do.
    fn restart(&mut self);
}

/// The backend of the simulator: the entries of each log in a list.
#[derive(Debug, Clone, Default)]
pub struct MemoryLog {
    entries: BTreeMap<LogId, Vec<Vec<u8>>>,
    subscribers: BTreeSet<(LogId, DeviceKey)>,
    wakes: BTreeMap<(DeviceKey, LogId), Seq>,
}

impl MemoryLog {
    /// The entries stored for `log`, as they were appended.
    pub fn entries(&self, log: &LogId) -> &[Vec<u8>] {
        self.entries.get(log).map_or(&[], Vec::as_slice)
    }

    /// Every log that has an entry, with its entries: what a test compares
    /// byte for byte.
    pub fn snapshot(&self) -> BTreeMap<LogId, Vec<Vec<u8>>> {
        self.entries.clone()
    }

    /// The number of entries and of bytes stored, over all logs.
    pub fn totals(&self) -> (u64, u64) {
        self.entries
            .values()
            .flatten()
            .fold((0, 0), |(events, bytes), entry| {
                (events + 1, bytes + entry.len() as u64)
            })
    }
}

impl Log for MemoryLog {
    fn append(
        &mut self,
        caller: &DeviceKey,
        log: &LogId,
        expected: Seq,
        entry: Vec<u8>,
    ) -> Result<(), Conflict> {
        let entries = self.entries.entry(*log).or_default();
        let head = Seq(entries.len() as u64);
        if expected != head {
            return Err(Conflict { head });
        }
        entries.push(entry);
        let new_head = head.next();
        let lowest = DeviceKey::from_bytes([0; 32]);
        for (_, subscriber) in self
            .subscribers
            .range((*log, lowest)..)
            .take_while(|(subscribed, _)| subscribed == log)
        {
            if subscriber != caller {
                self.wakes.insert((*subscriber, *log), new_head);
            }
        }
        Ok(())
    }

    fn read(&self, _caller: &DeviceKey, log: &LogId, from: Seq) -> Vec<Vec<u8>> {
        self.entries(log)
            .get(from.index()..)
            .map(<[_]>::to_vec)
            .unwrap_or_default()
    }

    fn head(&self, _caller: &DeviceKey, log: &LogId) -> Seq {
        Seq(self.entries(log).len() as u64)
    }

    fn delete(&mut self, log: &LogId) {
        self.entries.remove(log);
    }

    fn subscribe(&mut self, caller: &DeviceKey, log: &LogId) {
        self.subscribers.insert((*log, *caller));
    }

    fn drain_wakes(&mut self) -> Vec<Wake> {
        std::mem::take(&mut self.wakes)
            .into_iter()
            .map(|((to, log), head)| Wake { to, log, head })
            .collect()
    }

    fn restart(&mut self) {
        self.subscribers.clear();
        self.wakes.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ids::GroupId,
        keys::Identity,
        rng::{Seed, SimRng},
    };

    fn keys() -> (DeviceKey, DeviceKey) {
        let mut rng = SimRng::new(Seed(2));
        (
            Identity::generate(&mut rng).device_key(),
            Identity::generate(&mut rng).device_key(),
        )
    }

    #[test]
    fn wakes_for_one_log_coalesce_and_skip_the_author() {
        let (writer, reader) = keys();
        let log = LogId::Group(GroupId(1));
        let other = LogId::Group(GroupId(2));
        let mut store = MemoryLog::default();
        store.subscribe(&writer, &log);
        store.subscribe(&reader, &log);
        store.subscribe(&reader, &other);

        store.append(&writer, &log, Seq(0), vec![1]).unwrap();
        store.append(&writer, &log, Seq(1), vec![2]).unwrap();
        assert_eq!(
            store.drain_wakes(),
            vec![Wake {
                to: reader,
                log,
                head: Seq(2)
            }]
        );
        assert!(store.drain_wakes().is_empty());
    }

    #[test]
    fn a_restart_loses_the_subscriptions_but_not_the_entries() {
        let (writer, reader) = keys();
        let log = LogId::Group(GroupId(1));
        let mut store = MemoryLog::default();
        store.subscribe(&reader, &log);
        store.restart();
        store.append(&writer, &log, Seq(0), vec![1]).unwrap();
        assert!(store.drain_wakes().is_empty());
        assert_eq!(store.read(&reader, &log, Seq(0)), vec![vec![1]]);
    }

    #[test]
    fn a_deleted_log_starts_again_from_the_first_position() {
        let (writer, _) = keys();
        let log = LogId::Group(GroupId(1));
        let mut store = MemoryLog::default();
        store.append(&writer, &log, Seq(0), vec![1]).unwrap();
        store.delete(&log);
        assert!(store.read(&writer, &log, Seq(0)).is_empty());
        assert_eq!(store.append(&writer, &log, Seq(0), vec![2]), Ok(()));
        assert_eq!(store.totals(), (1, 1));
    }
}
