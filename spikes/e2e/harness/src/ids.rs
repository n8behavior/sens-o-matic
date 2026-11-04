//! The names of things: groups, pings, users, logs, positions and codes.
//!
//! Each is a newtype, so that a ping cannot be passed where a group is
//! expected. A [`UserId`] is not chosen by anyone: it is the position in the
//! group log of the event that admitted the user, so two users cannot get one
//! id and a joiner cannot claim an id that is taken.

use std::fmt;

use crate::{
    codec::{DecodeError, Reader, Writer},
    rng::SimRng,
};

/// A group. A server may hold many; the harness runs one at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GroupId(pub u32);

impl fmt::Display for GroupId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "g{}", self.0)
    }
}

/// A ping (a hangout), which has a log of its own (D3 of #61).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PingId(pub u32);

impl fmt::Display for PingId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "p{}", self.0)
    }
}

/// The position of an event in its log, counted from zero.
///
/// The head of a log is the position that the next event gets, so an empty log
/// has head [`Seq::FIRST`] and a log of three events has head `Seq(3)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Seq(pub u64);

impl Seq {
    /// The position of the first event of a log.
    pub const FIRST: Self = Self(0);

    /// The position after this one.
    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }

    /// The position as an index of a list of events.
    pub fn index(self) -> usize {
        usize::try_from(self.0).expect("a position of a simulated log fits in usize")
    }
}

impl fmt::Display for Seq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// A user: the set of devices that the group log vouches for as one person.
///
/// The id is the position of the group-log event that admitted the user (the
/// `Create` of the founder, or the `Approve` of a joiner). Only the rules
/// ([`crate::rules`]) make one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UserId(Seq);

impl UserId {
    pub(crate) fn admitted_at(seq: Seq) -> Self {
        Self(seq)
    }

    /// The position in the group log of the event that admitted the user.
    pub fn admitted(self) -> Seq {
        self.0
    }

    pub(crate) fn encode(self, w: &mut Writer) {
        w.u64(self.0.0);
    }

    pub(crate) fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self(Seq(r.u64()?)))
    }
}

impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "u{}", (self.0).0)
    }
}

/// One log of a group: the group log, or the log of one ping (D3 of #61).
///
/// On an object store this is the prefix of the objects of the log, with one
/// object for each event named by its position (D7 of #61).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LogId {
    /// Membership, devices and keys.
    Group(GroupId),
    /// The events of one ping.
    Ping(GroupId, PingId),
}

impl LogId {
    /// The group the log belongs to.
    pub fn group(self) -> GroupId {
        match self {
            Self::Group(group) | Self::Ping(group, _) => group,
        }
    }

    pub(crate) fn encode(self, w: &mut Writer) {
        match self {
            Self::Group(group) => {
                w.u8(0).u32(group.0);
            }
            Self::Ping(group, ping) => {
                w.u8(1).u32(group.0).u32(ping.0);
            }
        }
    }

    pub(crate) fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        match r.u8()? {
            0 => Ok(Self::Group(GroupId(r.u32()?))),
            1 => Ok(Self::Ping(GroupId(r.u32()?), PingId(r.u32()?))),
            tag => Err(DecodeError::UnknownTag { what: "log", tag }),
        }
    }
}

impl fmt::Display for LogId {
    /// The prefix of the objects: `g1/group` or `g1/ping-3`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Group(group) => write!(f, "{group}/group"),
            Self::Ping(group, ping) => write!(f, "{group}/ping-{}", ping.0),
        }
    }
}

/// The code of an invite (SPEC section 9, "Invite Codes"; D11 of #61).
///
/// Eight lowercase letters and digits. The code is a lookup name and never a
/// key: whoever holds it can ask to join, and a member's device decides. It is
/// single use and has no expiry.
///
/// ```
/// use e2e_harness::ids::InviteCode;
/// use e2e_harness::rng::{Seed, SimRng};
///
/// let code = InviteCode::generate(&mut SimRng::new(Seed(1)));
/// assert_eq!(code.as_str().len(), InviteCode::LEN);
/// assert_eq!(InviteCode::parse(code.as_str()), Some(code));
/// assert_eq!(InviteCode::parse("TOO-SHORT"), None);
/// ```
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InviteCode([u8; InviteCode::LEN]);

impl InviteCode {
    /// The number of characters of a code.
    pub const LEN: usize = 8;
    const ALPHABET: &'static [u8; 36] = b"abcdefghijklmnopqrstuvwxyz0123456789";

    /// A code from `rng`.
    pub fn generate(rng: &mut SimRng) -> Self {
        let mut code = [0u8; Self::LEN];
        for byte in &mut code {
            *byte = Self::ALPHABET[rng.below(Self::ALPHABET.len())];
        }
        Self(code)
    }

    /// The code written as `text`, or `None` when `text` is not a code.
    pub fn parse(text: &str) -> Option<Self> {
        let bytes: [u8; Self::LEN] = text.as_bytes().try_into().ok()?;
        bytes
            .iter()
            .all(|byte| Self::ALPHABET.contains(byte))
            .then_some(Self(bytes))
    }

    /// The code as text.
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).expect("a code holds only ASCII letters and digits")
    }

    pub(crate) fn encode(self, w: &mut Writer) {
        w.raw(&self.0);
    }

    pub(crate) fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let bytes = r.array::<{ Self::LEN }>()?;
        bytes
            .iter()
            .all(|byte| Self::ALPHABET.contains(byte))
            .then_some(Self(bytes))
            .ok_or(DecodeError::Invalid(
                "an invite code holds only lowercase letters and digits",
            ))
    }
}

impl fmt::Display for InviteCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for InviteCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "code:{self}")
    }
}

/// Bytes that belong to a candidate: key material, a Welcome, a Commit.
///
/// The harness stores them in an event and hands them back; it never looks
/// inside.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Material(Vec<u8>);

impl Material {
    /// No bytes.
    pub fn empty() -> Self {
        Self::default()
    }

    /// The bytes.
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }

    /// The number of bytes.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// True when there are no bytes.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<Vec<u8>> for Material {
    fn from(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

impl From<&[u8]> for Material {
    fn from(bytes: &[u8]) -> Self {
        Self(bytes.to_vec())
    }
}

impl AsRef<[u8]> for Material {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for Material {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Material({} bytes)", self.0.len())
    }
}

impl Material {
    pub(crate) fn encode(&self, w: &mut Writer) {
        w.bytes(&self.0);
    }

    pub(crate) fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        r.bytes().map(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_log_id_round_trips() {
        for log in [LogId::Group(GroupId(3)), LogId::Ping(GroupId(3), PingId(9))] {
            let mut w = Writer::new();
            log.encode(&mut w);
            let bytes = w.finish();
            let mut r = Reader::new(&bytes);
            assert_eq!(LogId::decode(&mut r), Ok(log));
            assert_eq!(r.finish(), Ok(()));
        }
    }

    #[test]
    fn a_log_id_names_the_prefix_of_its_objects() {
        assert_eq!(LogId::Group(GroupId(1)).to_string(), "g1/group");
        assert_eq!(LogId::Ping(GroupId(1), PingId(3)).to_string(), "g1/ping-3");
    }

    #[test]
    fn a_code_outside_the_alphabet_is_refused() {
        assert_eq!(InviteCode::parse("ABCDEFGH"), None);
        assert_eq!(InviteCode::parse("abcdefg"), None);
        assert!(InviteCode::parse("abcd0123").is_some());
    }
}
