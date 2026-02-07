use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use uuid::Uuid;

use crate::models::{Group, Hangout, MatchResults, Ping, User};

#[derive(Debug, Default)]
pub struct InMemoryStore<T> {
    data: RwLock<HashMap<Uuid, T>>,
}

impl<T: Clone> InMemoryStore<T> {
    pub fn new() -> Self {
        Self {
            data: RwLock::new(HashMap::new()),
        }
    }

    pub fn insert(&self, id: Uuid, item: T) {
        self.data.write().unwrap().insert(id, item);
    }

    pub fn get(&self, id: &Uuid) -> Option<T> {
        self.data.read().unwrap().get(id).cloned()
    }

    pub fn update<F>(&self, id: &Uuid, f: F) -> Option<T>
    where
        F: FnOnce(&mut T),
    {
        let mut data = self.data.write().unwrap();
        if let Some(item) = data.get_mut(id) {
            f(item);
            Some(item.clone())
        } else {
            None
        }
    }

    pub fn remove(&self, id: &Uuid) -> Option<T> {
        self.data.write().unwrap().remove(id)
    }

    pub fn exists(&self, id: &Uuid) -> bool {
        self.data.read().unwrap().contains_key(id)
    }

    pub fn find<F>(&self, predicate: F) -> Option<T>
    where
        F: Fn(&T) -> bool,
    {
        self.data
            .read()
            .unwrap()
            .values()
            .find(|item| predicate(item))
            .cloned()
    }

    pub fn filter<F>(&self, predicate: F) -> Vec<T>
    where
        F: Fn(&T) -> bool,
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

#[derive(Clone)]
pub struct AppState {
    pub users: Arc<InMemoryStore<User>>,
    pub groups: Arc<InMemoryStore<Group>>,
    pub pings: Arc<InMemoryStore<Ping>>,
    pub hangouts: Arc<InMemoryStore<Hangout>>,
    pub match_results: Arc<InMemoryStore<MatchResults>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        Self {
            users: Arc::new(InMemoryStore::new()),
            groups: Arc::new(InMemoryStore::new()),
            pings: Arc::new(InMemoryStore::new()),
            hangouts: Arc::new(InMemoryStore::new()),
            match_results: Arc::new(InMemoryStore::new()),
        }
    }

    pub fn get_user_groups(&self, user_id: Uuid) -> Vec<Group> {
        self.groups.filter(|g| g.is_member(user_id))
    }

    pub fn find_group_by_invite_code(&self, code: &str) -> Option<Group> {
        self.groups.find(|g| g.invite_code == code)
    }

    pub fn get_group_pings(&self, group_id: Uuid) -> Vec<Ping> {
        self.pings.filter(|p| p.group == group_id)
    }
}
