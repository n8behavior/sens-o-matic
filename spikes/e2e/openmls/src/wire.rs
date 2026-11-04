//! The bytes of the material of a group event and of the saved state of a
//! cipher, in the TLS presentation language of RFC 9420 (the codec of OpenMLS).

use std::collections::BTreeMap;

use e2e_harness::ids::PingId;
use thiserror::Error;
use tls_codec::{
    Deserialize, Serialize, TlsDeserialize, TlsSerialize, TlsSize, TlsVecU32, VLBytes,
};

/// The bytes do not read as what they should be.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("cannot read {what}: {reason} (see the module docs of e2e_openmls::wire)")]
pub struct WireError {
    what: &'static str,
    reason: String,
}

fn read<T: Deserialize>(what: &'static str, bytes: &[u8]) -> Result<T, WireError> {
    T::tls_deserialize_exact(bytes).map_err(|error| WireError {
        what,
        reason: error.to_string(),
    })
}

fn write<T: Serialize>(value: &T) -> Vec<u8> {
    value
        .tls_serialize_detached()
        .expect("a value of this module fits its length prefix")
}

/// The material of an `Approve`, an `AddDevice` or a `Remove`: the Commit, the
/// Welcome for the device that is added, and what the user's other devices hand
/// over to a new device (see [`Handover`]). A field that the event has no use
/// for is empty.
#[derive(Debug, Clone, PartialEq, Eq, TlsSerialize, TlsDeserialize, TlsSize)]
pub struct Bundle {
    /// The MLS Commit, as an `MlsMessageOut`.
    pub commit: VLBytes,
    /// The MLS Welcome, as an `MlsMessageOut`; empty for a removal.
    pub welcome: VLBytes,
    /// The HPKE ciphertext of a [`Handover`]; empty unless a device is added.
    pub handover: VLBytes,
}

impl Bundle {
    /// The bytes of the bundle.
    pub fn encode(&self) -> Vec<u8> {
        write(self)
    }

    /// Read a bundle.
    pub fn decode(bytes: &[u8]) -> Result<Self, WireError> {
        read("the material of a group event", bytes)
    }
}

/// An epoch of the group and what a device keeps for it.
#[derive(Debug, Clone, PartialEq, Eq, TlsSerialize, TlsDeserialize, TlsSize)]
pub struct EpochEntry {
    /// The number of the epoch.
    pub epoch: u64,
    /// The key that the epoch gives to the pings that start in it.
    pub key: [u8; 32],
    /// The hash of the Commit that made the epoch.
    pub commit: [u8; 32],
}

/// What a device hands to a new device of the same user: the keys of the epochs
/// it holds, so that the new device reads the pings that began before it came.
#[derive(Debug, Clone, PartialEq, Eq, TlsSerialize, TlsDeserialize, TlsSize)]
pub struct Handover {
    epochs: TlsVecU32<EpochEntry>,
}

impl Handover {
    /// A handover of `epochs`.
    pub fn new(epochs: Vec<EpochEntry>) -> Self {
        Self {
            epochs: epochs.into(),
        }
    }

    /// The bytes, to seal.
    pub fn encode(&self) -> Vec<u8> {
        write(self)
    }

    /// Read what was sealed.
    pub fn decode(bytes: &[u8]) -> Result<Self, WireError> {
        read("a handover", bytes)
    }

    /// The epochs.
    pub fn epochs(&self) -> &[EpochEntry] {
        self.epochs.as_slice()
    }
}

/// A key and a value of the storage of OpenMLS.
#[derive(Debug, Clone, PartialEq, Eq, TlsSerialize, TlsDeserialize, TlsSize)]
pub struct StorageEntry {
    key: VLBytes,
    value: VLBytes,
}

/// A ping and the epoch whose key it uses.
#[derive(Debug, Clone, PartialEq, Eq, TlsSerialize, TlsDeserialize, TlsSize)]
pub struct PingEntry {
    ping: u32,
    epoch: u64,
}

/// The state of a cipher between two events, for `export` and `restore`.
#[derive(Debug, Clone, PartialEq, Eq, TlsSerialize, TlsDeserialize, TlsSize)]
pub struct Saved {
    group: VLBytes,
    storage: TlsVecU32<StorageEntry>,
    epochs: TlsVecU32<EpochEntry>,
    pings: TlsVecU32<PingEntry>,
}

impl Saved {
    /// The state from its parts: the id of the MLS group (none before the device
    /// has a group), the storage of OpenMLS, the epochs and the pings. The
    /// storage is written in the order of its keys, so that one state has one
    /// encoding.
    pub fn new(
        group: Option<&[u8]>,
        storage: BTreeMap<Vec<u8>, Vec<u8>>,
        epochs: Vec<EpochEntry>,
        pings: &BTreeMap<PingId, u64>,
    ) -> Self {
        Self {
            group: group.unwrap_or_default().to_vec().into(),
            storage: storage
                .into_iter()
                .map(|(key, value)| StorageEntry {
                    key: key.into(),
                    value: value.into(),
                })
                .collect::<Vec<_>>()
                .into(),
            epochs: epochs.into(),
            pings: pings
                .iter()
                .map(|(ping, epoch)| PingEntry {
                    ping: ping.0,
                    epoch: *epoch,
                })
                .collect::<Vec<_>>()
                .into(),
        }
    }

    /// The bytes.
    pub fn encode(&self) -> Vec<u8> {
        write(self)
    }

    /// Read a state.
    pub fn decode(bytes: &[u8]) -> Result<Self, WireError> {
        read("the saved state of a cipher", bytes)
    }

    /// The id of the MLS group, if the device had one.
    pub fn group(&self) -> Option<&[u8]> {
        Some(self.group.as_slice()).filter(|id| !id.is_empty())
    }

    /// The entries of the storage of OpenMLS.
    pub fn storage(&self) -> impl Iterator<Item = (Vec<u8>, Vec<u8>)> + '_ {
        self.storage.as_slice().iter().map(|entry| {
            (
                entry.key.as_slice().to_vec(),
                entry.value.as_slice().to_vec(),
            )
        })
    }

    /// The epochs the device holds.
    pub fn epochs(&self) -> &[EpochEntry] {
        self.epochs.as_slice()
    }

    /// The pings the device has opened, with the epoch of each.
    pub fn pings(&self) -> impl Iterator<Item = (PingId, u64)> + '_ {
        self.pings
            .as_slice()
            .iter()
            .map(|entry| (PingId(entry.ping), entry.epoch))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bundle_round_trips_and_trailing_bytes_are_refused() {
        let bundle = Bundle {
            commit: b"commit".to_vec().into(),
            welcome: VLBytes::new(Vec::new()),
            handover: b"sealed".to_vec().into(),
        };
        let bytes = bundle.encode();
        assert_eq!(Bundle::decode(&bytes), Ok(bundle));
        assert!(Bundle::decode(&[bytes, vec![0]].concat()).is_err());
        assert!(Bundle::decode(&[]).is_err());
    }

    #[test]
    fn a_saved_state_has_one_encoding() {
        let storage: BTreeMap<_, _> = [
            (b"b".to_vec(), b"2".to_vec()),
            (b"a".to_vec(), b"1".to_vec()),
        ]
        .into();
        let pings: BTreeMap<_, _> = [(PingId(3), 7u64)].into();
        let saved = Saved::new(Some(b"g"), storage, vec![], &pings);
        assert_eq!(saved.group(), Some(&b"g"[..]));
        assert_eq!(Saved::decode(&saved.encode()), Ok(saved.clone()));
        assert_eq!(
            saved.storage().collect::<Vec<_>>(),
            vec![
                (b"a".to_vec(), b"1".to_vec()),
                (b"b".to_vec(), b"2".to_vec())
            ]
        );
    }
}
