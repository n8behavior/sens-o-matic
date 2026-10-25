//! The data of the server, held in memory.
//!
//! [`AppState`] holds one [`InMemoryStore`] for each kind of entity: users,
//! groups and pings. A ping owns its answers and its hangout, so they have no
//! store of their own. The state is shared between all the requests, and
//! everything in it is lost when the process stops.
//!
//! # Concurrency
//!
//! A store is a map behind a read-write lock. Each operation of a store is
//! atomic, but a sequence of them is not: a handler that reads an entity, checks
//! a rule and then writes it back does so in separate steps, and another
//! request can change the entity in between. The write closure of
//! [`InMemoryStore::update`] runs under the lock and sees the current value;
//! put in it whatever must not be raced.
//!
//! The stores hand out copies, never references, so a caller never holds a
//! lock between two calls.
//!
//! # Order
//!
//! The stores are hash maps, and the lists they return ([`InMemoryStore::filter`],
//! [`AppState::get_user_groups`], [`AppState::get_group_pings`]) come in no
//! particular order, which can differ from one call to the next.

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use std::hash::Hash;

use crate::models::{Group, GroupId, InviteCode, Ping, PingId, User, UserId};

/// A map from id to item, safe to share between threads.
///
/// The key type `K` is the id type of the items, such as [`UserId`], so that a
/// store of users cannot be asked for the id of a group.
///
/// # Panics
///
/// Every method takes the lock and panics if it is poisoned, that is, if a
/// closure panicked while it held the write lock. After that the store is
/// unusable, and so is every request that touches it.
#[derive(Debug)]
pub struct InMemoryStore<K, V> {
    data: RwLock<HashMap<K, V>>,
}

impl<K, V> Default for InMemoryStore<K, V> {
    fn default() -> Self {
        Self {
            data: RwLock::default(),
        }
    }
}

impl<K: Eq + Hash, V: Clone> InMemoryStore<K, V> {
    /// Creates an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores an item under the id. An item that is stored under the same id is
    /// replaced.
    ///
    /// # Panics
    ///
    /// Panics if the lock is poisoned; see [`InMemoryStore`].
    pub fn insert(&self, id: K, item: V) {
        self.data.write().unwrap().insert(id, item);
    }

    /// Returns a copy of the item, or `None` if there is none under the id.
    ///
    /// # Panics
    ///
    /// Panics if the lock is poisoned; see [`InMemoryStore`].
    pub fn get(&self, id: &K) -> Option<V> {
        self.data.read().unwrap().get(id).cloned()
    }

    /// Changes the item in place with `f`, and returns a copy of the result, or
    /// `None` if there is no item under the id (and `f` does not run).
    ///
    /// `f` runs while the store holds its write lock. It must not call the
    /// same store, which would wait for itself for ever.
    ///
    /// # Panics
    ///
    /// Panics if the lock is poisoned; see [`InMemoryStore`].
    ///
    /// # Examples
    ///
    /// ```
    /// use sens_o_matic::models::UserId;
    /// use sens_o_matic::state::InMemoryStore;
    ///
    /// let store = InMemoryStore::new();
    /// let id = UserId::random();
    /// store.insert(id, vec![1, 2]);
    ///
    /// let updated = store.update(&id, |numbers| numbers.push(3));
    /// assert_eq!(updated, Some(vec![1, 2, 3]));
    /// assert_eq!(store.get(&id), Some(vec![1, 2, 3]));
    ///
    /// // An unknown id changes nothing.
    /// assert_eq!(store.update(&UserId::random(), |numbers| numbers.clear()), None);
    /// ```
    pub fn update<F>(&self, id: &K, f: F) -> Option<V>
    where
        F: FnOnce(&mut V),
    {
        let mut data = self.data.write().unwrap();
        if let Some(item) = data.get_mut(id) {
            f(item);
            Some(item.clone())
        } else {
            None
        }
    }

    /// Removes the item and returns it, or returns `None` if there is none
    /// under the id.
    ///
    /// # Panics
    ///
    /// Panics if the lock is poisoned; see [`InMemoryStore`].
    pub fn remove(&self, id: &K) -> Option<V> {
        self.data.write().unwrap().remove(id)
    }

    /// Tells whether there is an item under the id.
    ///
    /// # Panics
    ///
    /// Panics if the lock is poisoned; see [`InMemoryStore`].
    pub fn exists(&self, id: &K) -> bool {
        self.data.read().unwrap().contains_key(id)
    }

    /// Returns a copy of an item for which `predicate` is true, or `None`.
    /// With several such items, which one is returned is not specified.
    ///
    /// `predicate` runs while the store holds its read lock. It must not write
    /// to the same store.
    ///
    /// # Panics
    ///
    /// Panics if the lock is poisoned; see [`InMemoryStore`].
    pub fn find<F>(&self, predicate: F) -> Option<V>
    where
        F: Fn(&V) -> bool,
    {
        self.data
            .read()
            .unwrap()
            .values()
            .find(|item| predicate(item))
            .cloned()
    }

    /// Returns copies of all the items for which `predicate` is true, in no
    /// particular order.
    ///
    /// `predicate` runs while the store holds its read lock. It must not write
    /// to the same store.
    ///
    /// # Panics
    ///
    /// Panics if the lock is poisoned; see [`InMemoryStore`].
    ///
    /// # Examples
    ///
    /// ```
    /// use sens_o_matic::models::UserId;
    /// use sens_o_matic::state::InMemoryStore;
    ///
    /// let store = InMemoryStore::new();
    /// for n in 1..=4 {
    ///     store.insert(UserId::random(), n);
    /// }
    ///
    /// let mut even = store.filter(|n| n % 2 == 0);
    /// even.sort();
    /// assert_eq!(even, vec![2, 4]);
    /// ```
    pub fn filter<F>(&self, predicate: F) -> Vec<V>
    where
        F: Fn(&V) -> bool,
    {
        self.data
            .read()
            .unwrap()
            .values()
            .filter(|item| predicate(item))
            .cloned()
            .collect()
    }
}

/// The shared state of the server. Cloning it is cheap and gives a handle to
/// the same stores, which is how each request gets at them.
#[derive(Clone)]
pub struct AppState {
    /// All the users, by id.
    pub users: Arc<InMemoryStore<UserId, User>>,
    /// All the groups, by id.
    pub groups: Arc<InMemoryStore<GroupId, Group>>,
    /// All the pings, by id, with their answers and hangouts.
    pub pings: Arc<InMemoryStore<PingId, Ping>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    /// Creates a state with empty stores.
    pub fn new() -> Self {
        Self {
            users: Arc::new(InMemoryStore::new()),
            groups: Arc::new(InMemoryStore::new()),
            pings: Arc::new(InMemoryStore::new()),
        }
    }

    /// Returns the groups that the user is a member of, in no particular order.
    /// The user does not have to exist: a stranger is in no group.
    ///
    /// # Panics
    ///
    /// Panics if the lock of the group store is poisoned; see [`InMemoryStore`].
    pub fn get_user_groups(&self, user_id: UserId) -> Vec<Group> {
        self.groups.filter(|g| g.is_member(user_id))
    }

    /// Returns the group that has this invite code, or `None`. The match is
    /// exact, so the case matters.
    ///
    /// The groups are not kept unique by code; if two had the same code, which
    /// one is returned is not specified.
    ///
    /// # Panics
    ///
    /// Panics if the lock of the group store is poisoned; see [`InMemoryStore`].
    ///
    /// # Examples
    ///
    /// ```
    /// use sens_o_matic::AppState;
    /// use sens_o_matic::models::{CreateGroupRequest, Group, UserId};
    ///
    /// let state = AppState::new();
    /// let group = Group::new(CreateGroupRequest {
    ///     name: "Friday crew".parse().unwrap(),
    ///     creator_id: UserId::random(),
    /// });
    /// state.groups.insert(group.id, group.clone());
    ///
    /// let found = state.find_group_by_invite_code(&group.invite_code);
    /// assert_eq!(found.map(|g| g.id), Some(group.id));
    /// assert!(state.find_group_by_invite_code(&"nope12".parse().unwrap()).is_none());
    /// ```
    pub fn find_group_by_invite_code(&self, code: &InviteCode) -> Option<Group> {
        self.groups.find(|g| g.invite_code == *code)
    }

    /// Returns the pings that were sent to the group, in every state, in no
    /// particular order. The group does not have to exist.
    ///
    /// # Panics
    ///
    /// Panics if the lock of the ping store is poisoned; see [`InMemoryStore`].
    pub fn get_group_pings(&self, group_id: GroupId) -> Vec<Ping> {
        self.pings.filter(|p| p.group == group_id)
    }
}
