//! What the harness writes in the payload of an event.
//!
//! A ping event is a ciphertext that belongs to the candidate, except the first
//! one of a ping log, which is a [`PingOpen`]. A group-log event is a
//! [`GroupPayload`]: a [`GroupChange`] that the harness defines and reads, and
//! the [`Material`] that the candidate makes for it.
//!
//! # Why a group change is in the clear
//!
//! The rules on replay (D4 c, [`crate::rules`]) are the same for every
//! candidate, so they read a vocabulary that the harness owns: which code was
//! used, which device asks to join, who is removed. That vocabulary is public
//! data: the user-to-group mapping and the device keys are on the list of what
//! the server may read (D1 of #61), and the invite code is only a lookup name
//! (D11), never a key. Everything secret is in the `material`, which only the
//! candidate reads.
//!
//! A candidate whose protocol puts its own proposals inside its messages (an
//! MLS Commit names the members it adds) must check that the material agrees
//! with the change, and refuse it with a [`crate::cipher::CipherError`] when it
//! does not: every device refuses the same event alike.
//!
//! # Unknown kinds
//!
//! A tag that this build does not know decodes to an error, and the device
//! ignores the event with [`crate::rules::Refusal::Malformed`]: "an unknown kind
//! is skipped" (Rules for the format of an event, #61).

use crate::{
    codec::{DecodeError, Reader, Writer},
    ids::{InviteCode, Material, Seq, UserId},
    keys::DeviceKey,
};

/// What a group-log event does, in the clear.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupChange {
    /// The first event of the log: the author is the founder, and the group
    /// exists from here.
    Create,
    /// A member makes a single-use code that a person can pass on (D11).
    Invite {
        /// The lookup name of the invite.
        code: InviteCode,
    },
    /// A member cancels a code that nobody used yet. Any member may.
    Revoke {
        /// The code to cancel.
        code: InviteCode,
    },
    /// A device that no user vouches for yet asks to join with a code. The
    /// first valid request for a code wins; a later one is refused (D11).
    JoinRequest {
        /// The code of the invite.
        code: InviteCode,
        /// What the protocol needs to hand the requester keys.
        introduction: Material,
    },
    /// A member's device approves the join request at `request`: the requester
    /// becomes a new user, and the material gives it the keys.
    Approve {
        /// The position of the `JoinRequest` in the group log.
        request: Seq,
    },
    /// A device of a member vouches for a new device of the same user (D6). The
    /// new device passes its introduction on outside the log, as a QR code does.
    AddDevice {
        /// The key of the new device.
        device: DeviceKey,
        /// What the protocol needs to hand the new device keys.
        introduction: Material,
    },
    /// A member says that they leave. It changes no key: another member must
    /// write the `Remove` that does (question 7 of #62).
    Leave,
    /// Another member removes a user, who leaves or is removed. The material
    /// carries the new key to everyone who stays.
    Remove {
        /// The user that goes.
        user: UserId,
    },
}

impl GroupChange {
    fn encode(&self, w: &mut Writer) {
        match self {
            Self::Create => {
                w.u8(0);
            }
            Self::Invite { code } => {
                w.u8(1);
                code.encode(w);
            }
            Self::Revoke { code } => {
                w.u8(2);
                code.encode(w);
            }
            Self::JoinRequest { code, introduction } => {
                w.u8(3);
                code.encode(w);
                introduction.encode(w);
            }
            Self::Approve { request } => {
                w.u8(4).u64(request.0);
            }
            Self::AddDevice {
                device,
                introduction,
            } => {
                w.u8(5).raw(device.as_bytes());
                introduction.encode(w);
            }
            Self::Leave => {
                w.u8(6);
            }
            Self::Remove { user } => {
                w.u8(7);
                user.encode(w);
            }
        }
    }

    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(match r.u8()? {
            0 => Self::Create,
            1 => Self::Invite {
                code: InviteCode::decode(r)?,
            },
            2 => Self::Revoke {
                code: InviteCode::decode(r)?,
            },
            3 => Self::JoinRequest {
                code: InviteCode::decode(r)?,
                introduction: Material::decode(r)?,
            },
            4 => Self::Approve {
                request: Seq(r.u64()?),
            },
            5 => Self::AddDevice {
                device: DeviceKey::from_bytes(r.array()?),
                introduction: Material::decode(r)?,
            },
            6 => Self::Leave,
            7 => Self::Remove {
                user: UserId::decode(r)?,
            },
            tag => {
                return Err(DecodeError::UnknownTag {
                    what: "group change",
                    tag,
                });
            }
        })
    }
}

/// The payload of a group-log event.
///
/// ```
/// use e2e_harness::ids::Material;
/// use e2e_harness::payload::{GroupChange, GroupPayload};
///
/// let payload = GroupPayload { change: GroupChange::Leave, material: Material::from(&b"keys"[..]) };
/// assert_eq!(GroupPayload::decode(&payload.encode()), Ok(payload));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupPayload {
    /// What the event does.
    pub change: GroupChange,
    /// What the candidate made for it.
    pub material: Material,
}

impl GroupPayload {
    /// The bytes for the payload of an event.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        self.change.encode(&mut w);
        self.material.encode(&mut w);
        w.finish()
    }

    /// Read the payload of an event.
    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        let mut r = Reader::new(bytes);
        let payload = Self {
            change: GroupChange::decode(&mut r)?,
            material: Material::decode(&mut r)?,
        };
        r.finish()?;
        Ok(payload)
    }
}

/// The first event of a ping log: the key of the ping (D3 and question 1 of #62).
///
/// It names the head of the group log that the starter had applied, the "key
/// epoch" that D3 asks a ping event to name. A removal at or after that head
/// ends the ping (D10): see [`crate::rules::GroupState::ping_ended`]. The
/// `material` is the candidate's: a wrapped ping key, a Welcome, a label for an
/// exporter.
///
/// ```
/// use e2e_harness::ids::{Material, Seq};
/// use e2e_harness::payload::PingOpen;
///
/// let open = PingOpen { group_head: Seq(12), material: Material::empty() };
/// assert_eq!(PingOpen::decode(&open.encode()), Ok(open));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PingOpen {
    /// The head of the group log that the starter had applied.
    pub group_head: Seq,
    /// What the candidate made in
    /// [`crate::cipher::GroupCipher::start_ping`].
    pub material: Material,
}

impl PingOpen {
    /// The bytes for the payload of the first event of a ping log.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u64(self.group_head.0);
        self.material.encode(&mut w);
        w.finish()
    }

    /// Read the payload of the first event of a ping log.
    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        let mut r = Reader::new(bytes);
        let open = Self {
            group_head: Seq(r.u64()?),
            material: Material::decode(&mut r)?,
        };
        r.finish()?;
        Ok(open)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::{Seed, SimRng};

    #[test]
    fn every_change_round_trips() {
        let mut rng = SimRng::new(Seed(3));
        let code = InviteCode::generate(&mut rng);
        let key = DeviceKey::from_bytes([7; 32]);
        let changes = [
            GroupChange::Create,
            GroupChange::Invite { code },
            GroupChange::Revoke { code },
            GroupChange::JoinRequest {
                code,
                introduction: Material::from(&b"kp"[..]),
            },
            GroupChange::Approve { request: Seq(5) },
            GroupChange::AddDevice {
                device: key,
                introduction: Material::from(&b"kp"[..]),
            },
            GroupChange::Leave,
            GroupChange::Remove {
                user: UserId::admitted_at(Seq(2)),
            },
        ];
        for change in changes {
            let payload = GroupPayload {
                change,
                material: Material::from(&b"m"[..]),
            };
            assert_eq!(GroupPayload::decode(&payload.encode()), Ok(payload));
        }
    }

    #[test]
    fn an_unknown_kind_is_an_error() {
        assert_eq!(
            GroupPayload::decode(&[200, 0, 0, 0, 0]),
            Err(DecodeError::UnknownTag {
                what: "group change",
                tag: 200
            })
        );
    }
}
