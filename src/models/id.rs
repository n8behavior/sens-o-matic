//! The id of an entity, one type for each kind of entity.
//!
//! [`Id<T>`] is a [`Uuid`] that is tagged with the entity it names, so that an
//! id of one kind cannot stand where an id of another kind is expected. The
//! entity types are the tags, and the four aliases [`UserId`], [`GroupId`],
//! [`PingId`] and [`ResponseId`] are the types that the rest of the crate
//! uses.
//!
//! The tag exists only in the type system. On the wire an id is what a
//! [`Uuid`] is: a string such as `"67e55044-10b1-426f-9247-bb680e5fe0c8"`, in
//! JSON, in a path, and in the OpenAPI document (`type: string`,
//! `format: uuid`).
//!
//! The server is the only one that makes ids: [`Id::random`] is the one place
//! where a fresh id comes from, and a client cannot choose one. An id that
//! comes from outside is parsed with [`FromStr`] or deserialized, and it names
//! an entity only if the store has it.
//!
//! # In the OpenAPI document
//!
//! [`utoipa`] knows the name `Uuid` and nothing else about ids, so a field that
//! holds an id says `#[schema(value_type = String, format = "uuid")]` (or
//! `Vec<String>`), and a path parameter says `inline(UserId)`; both give the
//! document the same schema as before the ids had types. [`Id`] implements
//! [`PartialSchema`] for the parameters and the keys of maps, and does not
//! implement [`ToSchema`](utoipa::ToSchema) on purpose: a field that lacks the
//! attribute then does not compile, instead of becoming a reference to a
//! component that the document does not have. The test `ids_are_uuid_strings`
//! in [`crate::router`] keeps the attribute and the schema in step.
//!
//! # Why one generic type
//!
//! Four newtypes made by a macro would give the same API four times. One
//! `Id<T>` has the impls once, and the tag costs nothing. The traits are
//! written by hand, not derived, because a derive would add the bound
//! `T: Clone` (and `T: Eq`, `T: Hash`, ...) to each impl, and an entity such as
//! [`User`] is none of them.
//!
//! # Examples
//!
//! ```
//! use sens_o_matic::models::{GroupId, UserId};
//!
//! let user: UserId = UserId::random();
//! let same: UserId = user.to_string().parse().unwrap();
//! assert_eq!(user, same);
//! assert_ne!(user, UserId::random());
//!
//! // On the wire an id is a string.
//! let json = serde_json::to_string(&user).unwrap();
//! assert_eq!(json, format!("\"{user}\""));
//! assert_eq!(serde_json::from_str::<UserId>(&json).unwrap(), user);
//!
//! let group = GroupId::random();
//! assert_ne!(group.to_string(), user.to_string());
//! ```
//!
//! An id of one kind is not an id of another kind. This does not compile:
//!
//! ```compile_fail,E0308
//! use sens_o_matic::models::{GroupId, UserId};
//! use sens_o_matic::state::InMemoryStore;
//!
//! fn is_member(users: &InMemoryStore<UserId, String>, group: GroupId) -> bool {
//!     users.exists(&group)
//! }
//! ```
//!
//! Nor does this:
//!
//! ```compile_fail,E0308
//! use sens_o_matic::models::{GroupId, UserId};
//!
//! let user = UserId::random();
//! let group: GroupId = user;
//! ```

use std::{fmt, hash, marker::PhantomData, str::FromStr};

use serde::{Deserialize, Serialize};
use utoipa::{
    PartialSchema,
    openapi::{KnownFormat, ObjectBuilder, RefOr, Schema, SchemaFormat, Type},
};
use uuid::Uuid;

use super::{Group, Ping, Response, User};

/// The id of a [`User`].
pub type UserId = Id<User>;
/// The id of a [`Group`].
pub type GroupId = Id<Group>;
/// The id of a [`Ping`].
pub type PingId = Id<Ping>;
/// The id of a [`Response`].
pub type ResponseId = Id<Response>;

/// A [`Uuid`] that names an entity of kind `T`. See the [module](self).
#[derive(Serialize, Deserialize)]
#[serde(transparent, bound = "")]
pub struct Id<T> {
    uuid: Uuid,
    // `fn() -> T` makes the id `Send`, `Sync` and covariant whatever `T` is,
    // and says that the id does not own a `T`.
    #[serde(skip)]
    kind: PhantomData<fn() -> T>,
}

impl<T> Id<T> {
    /// Makes a fresh id: a random UUID v4. It is the only way to make an id
    /// out of nothing; every other id comes from a parse.
    pub fn random() -> Self {
        Self {
            uuid: Uuid::new_v4(),
            kind: PhantomData,
        }
    }
}

impl<T> Clone for Id<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Id<T> {}

impl<T> PartialEq for Id<T> {
    fn eq(&self, other: &Self) -> bool {
        self.uuid == other.uuid
    }
}

impl<T> Eq for Id<T> {}

impl<T> hash::Hash for Id<T> {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.uuid.hash(state);
    }
}

impl<T> fmt::Debug for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Id").field(&self.uuid).finish()
    }
}

/// The hyphenated lower-case form, `67e55044-10b1-426f-9247-bb680e5fe0c8`.
impl<T> fmt::Display for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.uuid.fmt(f)
    }
}

/// Parses what [`Uuid`] parses. A parsed id names an entity only if the store
/// has one.
impl<T> FromStr for Id<T> {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self {
            uuid: s.parse()?,
            kind: PhantomData,
        })
    }
}

/// The schema of an id is the one of a [`Uuid`]: a string with the format
/// `uuid`. See the [module](self) for where it is used.
impl<T> PartialSchema for Id<T> {
    fn schema() -> RefOr<Schema> {
        ObjectBuilder::new()
            .schema_type(Type::String)
            .format(Some(SchemaFormat::KnownFormat(KnownFormat::Uuid)))
            .into()
    }
}
