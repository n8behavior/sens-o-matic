//! The server: the process in front of the storage.
//!
//! The server holds a [`Log`] and a [`Clock`]. It does what the server of D4
//! does and nothing more:
//!
//! - it checks that the caller is the author named in the draft (the key of the
//!   token must be the signing key, D4 b) and that the signature is good;
//! - it writes the timestamp from its clock;
//! - it appends the stamped bytes at the position that the author expected, so
//!   the conditional append is the only lock (D7).
//!
//! It cannot read a payload and does not try. It does **not** check that the
//! caller is a member of the group: the rules on replay decide what an event
//! means (D4 c), and a device that no user vouches for is ignored by every
//! device alike. A real server would also rate-limit; the harness has no limit.

use thiserror::Error;

use crate::{
    clock::{Clock, Millis},
    codec::DecodeError,
    event::{BadSignature, Draft, Envelope},
    ids::{LogId, Seq},
    keys::DeviceKey,
    log::{Conflict, Log, Wake},
};

/// What the server tells an author whose append it accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Receipt {
    /// The log that grew.
    pub log: LogId,
    /// The position of the event.
    pub seq: Seq,
    /// The timestamp that the server wrote.
    pub timestamp: Millis,
}

/// Why the server refused a draft.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SubmitError {
    /// The signed bytes are not an event.
    #[error("the draft is not an event: {0}")]
    Malformed(DecodeError),
    /// The caller is not the author named in the draft.
    #[error(
        "the caller {caller} is not the author {author} of the draft (see the module docs of e2e_harness::server)"
    )]
    NotTheAuthor {
        /// The key of the token.
        caller: DeviceKey,
        /// The key named in the draft.
        author: DeviceKey,
    },
    /// The signature does not match.
    #[error(transparent)]
    BadSignature(#[from] BadSignature),
    /// Another append won the position.
    #[error(transparent)]
    Conflict(#[from] Conflict),
}

/// The server of a run.
#[derive(Debug)]
pub struct Server<L> {
    log: L,
    clock: Clock,
    appended: u64,
    bytes: u64,
}

impl<L: Log> Server<L> {
    /// A server over `log` that stamps events with `clock`.
    pub fn new(log: L, clock: Clock) -> Self {
        Self {
            log,
            clock,
            appended: 0,
            bytes: 0,
        }
    }

    /// The time now.
    pub fn now(&self) -> Millis {
        self.clock.now()
    }

    /// Stamp and append a draft of `caller`.
    pub fn submit(&mut self, caller: &DeviceKey, draft: Draft) -> Result<Receipt, SubmitError> {
        let body = draft.body().map_err(SubmitError::Malformed)?;
        if body.author != *caller {
            return Err(SubmitError::NotTheAuthor {
                caller: *caller,
                author: body.author,
            });
        }
        let timestamp = self.clock.now();
        let envelope = Envelope::stamp(draft, timestamp).map_err(SubmitError::Malformed)?;
        envelope.verify()?;
        let entry = envelope.to_bytes();
        let size = entry.len() as u64;
        self.log.append(caller, &body.log, body.seq, entry)?;
        self.appended += 1;
        self.bytes += size;
        Ok(Receipt {
            log: body.log,
            seq: body.seq,
            timestamp,
        })
    }

    /// The entries of `log` from `from`, as the storage gives them.
    pub fn fetch(&self, caller: &DeviceKey, log: &LogId, from: Seq) -> Vec<Vec<u8>> {
        self.log.read(caller, log, from)
    }

    /// Ask for a wake each time `log` grows.
    pub fn subscribe(&mut self, caller: &DeviceKey, log: &LogId) {
        self.log.subscribe(caller, log);
    }

    /// Take the queued wakes.
    pub fn drain_wakes(&mut self) -> Vec<Wake> {
        self.log.drain_wakes()
    }

    /// Delete a log as a whole (retention, D8).
    pub fn delete(&mut self, log: &LogId) {
        self.log.delete(log);
    }

    /// Restart the process: the subscriptions and the queued wakes are lost,
    /// the stored entries are not (see [`Log::restart`]).
    pub fn restart(&mut self) {
        self.log.restart();
    }

    /// The number of events and of bytes that the server accepted.
    pub fn totals(&self) -> (u64, u64) {
        (self.appended, self.bytes)
    }

    /// The storage.
    pub fn log(&self) -> &L {
        &self.log
    }

    /// The storage, to arm an attack of [`crate::evil::EvilLog`].
    pub fn log_mut(&mut self) -> &mut L {
        &mut self.log
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ids::GroupId,
        keys::{EventHash, Identity},
        log::MemoryLog,
        rng::{Seed, SimRng},
    };

    #[test]
    fn the_server_writes_the_time_of_its_clock() {
        let clock = Clock::new();
        let mut server = Server::new(MemoryLog::default(), clock.clone());
        let me = Identity::generate(&mut SimRng::new(Seed(1)));
        let log = LogId::Group(GroupId(1));

        clock.advance(Millis::from_secs(90));
        let draft = Draft::sign(log, Seq::FIRST, EventHash::GENESIS, b"x".to_vec(), &me);
        let receipt = server.submit(&me.device_key(), draft).unwrap();
        assert_eq!(receipt.timestamp, Millis::from_secs(90));

        let stored = server.fetch(&me.device_key(), &log, Seq::FIRST);
        assert_eq!(
            Envelope::from_bytes(&stored[0]).unwrap().timestamp(),
            Millis::from_secs(90)
        );
    }

    #[test]
    fn a_draft_of_another_device_is_refused() {
        let mut rng = SimRng::new(Seed(1));
        let (alice, mallory) = (Identity::generate(&mut rng), Identity::generate(&mut rng));
        let mut server = Server::new(MemoryLog::default(), Clock::new());
        let draft = Draft::sign(
            LogId::Group(GroupId(1)),
            Seq::FIRST,
            EventHash::GENESIS,
            Vec::new(),
            &alice,
        );
        assert!(matches!(
            server.submit(&mallory.device_key(), draft),
            Err(SubmitError::NotTheAuthor { .. })
        ));
    }
}
