//! The epochs of the group and the keys a device holds for them.
//!
//! An epoch starts at the `Create` of the group (epoch 0) and at each `Remove`
//! (the next number). A joiner or a new device does not start an epoch: they
//! are handed the keys that exist. A [`KeyRing`] keeps every key its device was
//! given and never drops one (C13 of #62: a device can read a log again from the
//! start, in any order, and pays with having no forward secrecy).

use std::{collections::BTreeMap, fmt};

use crate::crypto::GroupKey;

/// The number of an epoch of the group: how many removals came before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Epoch(u32);

impl Epoch {
    /// The epoch of the `Create` event.
    pub(crate) const GENESIS: Self = Self(0);

    pub(crate) fn from_u32(number: u32) -> Self {
        Self(number)
    }

    pub(crate) fn get(self) -> u32 {
        self.0
    }

    /// The epoch that a `Remove` starts.
    ///
    /// # Panics
    ///
    /// Panics after 2^32 removals, which no group has.
    pub(crate) fn next(self) -> Self {
        Self(
            self.0
                .checked_add(1)
                .expect("a group has fewer than 2^32 removals"),
        )
    }
}

impl fmt::Display for Epoch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "epoch {}", self.0)
    }
}

/// The keys of the epochs that a device holds. The newest one is the current
/// epoch of the group, as far as the device has read.
#[derive(Debug, Clone, Default)]
pub(crate) struct KeyRing {
    keys: BTreeMap<Epoch, GroupKey>,
}

impl KeyRing {
    /// True when the device holds no key: it is in no group.
    pub(crate) fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// The newest epoch and its key.
    pub(crate) fn newest(&self) -> Option<(Epoch, &GroupKey)> {
        self.keys.last_key_value().map(|(epoch, key)| (*epoch, key))
    }

    /// The key of `epoch`, if the device holds it.
    pub(crate) fn get(&self, epoch: Epoch) -> Option<&GroupKey> {
        self.keys.get(&epoch)
    }

    pub(crate) fn insert(&mut self, epoch: Epoch, key: GroupKey) {
        self.keys.insert(epoch, key);
    }

    /// Every epoch and key, oldest first.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (Epoch, &GroupKey)> + '_ {
        self.keys.iter().map(|(epoch, key)| (*epoch, key))
    }
}
