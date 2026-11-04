//! The members of a group and their devices, as the rules derive them.
//!
//! A [`Roster`] is built by [`crate::rules::GroupState`] from the group log and
//! nowhere else, so every device that has applied the same prefix of the log has
//! the same roster. A candidate reads it to know whom to give keys to.

use std::{collections::BTreeMap, sync::Arc};

use crate::{
    codec::Writer,
    ids::{Material, UserId},
    keys::DeviceKey,
};

/// A device that the group log vouches for, or one that asks to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    /// The name of the device in the logs.
    pub key: DeviceKey,
    /// What the protocol needs to hand this device keys: an MLS key package, a
    /// public key for HPKE. The device made it with
    /// [`crate::cipher::GroupCipher::introduction`]; the harness only carries it.
    pub introduction: Material,
}

/// The users of a group and the devices of each.
///
/// ```
/// use e2e_harness::roster::Roster;
///
/// assert_eq!(Roster::default().device_count(), 0);
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Roster {
    users: BTreeMap<UserId, Vec<Arc<DeviceInfo>>>,
    owners: BTreeMap<DeviceKey, UserId>,
}

impl Roster {
    /// True when no user has been admitted: the group does not exist yet.
    pub fn is_empty(&self) -> bool {
        self.users.is_empty()
    }

    /// The users, in the order of their ids.
    pub fn users(&self) -> impl Iterator<Item = UserId> + '_ {
        self.users.keys().copied()
    }

    /// The number of users.
    pub fn user_count(&self) -> usize {
        self.users.len()
    }

    /// The number of devices of all users.
    pub fn device_count(&self) -> usize {
        self.owners.len()
    }

    /// Every device with its user, users in the order of their ids and the
    /// devices of a user in the order they were vouched for.
    pub fn devices(&self) -> impl Iterator<Item = (UserId, &DeviceInfo)> + '_ {
        self.users
            .iter()
            .flat_map(|(user, devices)| devices.iter().map(move |device| (*user, device.as_ref())))
    }

    /// The devices of `user`; none when the user is not in the group.
    pub fn devices_of(&self, user: UserId) -> impl Iterator<Item = &DeviceInfo> + '_ {
        self.users
            .get(&user)
            .into_iter()
            .flat_map(|devices| devices.iter().map(Arc::as_ref))
    }

    /// The user that vouches for `key`.
    pub fn user_of(&self, key: &DeviceKey) -> Option<UserId> {
        self.owners.get(key).copied()
    }

    /// True when some user vouches for `key`.
    pub fn contains(&self, key: &DeviceKey) -> bool {
        self.owners.contains_key(key)
    }

    /// True when `user` is in the group.
    pub fn contains_user(&self, user: UserId) -> bool {
        self.users.contains_key(&user)
    }

    /// Add a device to a user, who may be new.
    pub(crate) fn vouch(&mut self, user: UserId, device: Arc<DeviceInfo>) {
        self.owners.insert(device.key, user);
        self.users.entry(user).or_default().push(device);
    }

    /// Remove a user and every device of the user.
    pub(crate) fn remove(&mut self, user: UserId) {
        for device in self.users.remove(&user).into_iter().flatten() {
            self.owners.remove(&device.key);
        }
    }

    pub(crate) fn encode(&self, w: &mut Writer) {
        w.u32(u32::try_from(self.user_count()).expect("a group has fewer than 2^32 users"));
        for (user, devices) in &self.users {
            user.encode(w);
            w.u32(u32::try_from(devices.len()).expect("a user has fewer than 2^32 devices"));
            for device in devices {
                w.raw(device.key.as_bytes());
                device.introduction.encode(w);
            }
        }
    }
}
