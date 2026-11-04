//! [`OpenMls`]: MLS (RFC 9420) with OpenMLS behind
//! [`GroupCipher`](e2e_harness::cipher::GroupCipher).
//!
//! # The group
//!
//! One MLS group holds the group of the app, and one leaf is one device: the
//! identity of the basic credential and the signature key of the leaf are the
//! key of the device in the logs ([`crate::identity`]). The group log is the
//! delivery service. It orders the Commits, and the conditional append is the
//! rule "the first valid Commit for an epoch wins" (RFC 9750 section 5.2.1): the
//! loser of a race never sees its Commit applied, and makes a new one from the
//! new state.
//!
//! | Event | What the member's device does | What it puts in the material |
//! |---|---|---|
//! | `Create` | makes the MLS group | the tree hash, to find it again |
//! | `Approve` | one Commit that Adds the key package of the request | the Commit and the Welcome |
//! | `AddDevice` | the same for the new device of a known user | the Commit, the Welcome, and a [`Handover`](crate::wire::Handover) sealed to the new device |
//! | `Remove` | one Commit that removes every device of the user | the Commit |
//! | `Invite`, `Revoke`, `Leave` | nothing | nothing |
//!
//! A request and its Add are one flow (D11): the joiner's `introduction` is a
//! key package, in the clear in the `JoinRequest`; the approver's Add must carry
//! exactly that key package. The key package has a lifetime from 0 to
//! `u64::MAX`, because OpenMLS reads the wall clock when it validates one (see
//! [`crate::provider`] and the `clock` test).
//!
//! # A make method changes nothing
//!
//! The contract of the trait says that a `make` method must not change what
//! [`apply_group_event`](GroupCipher::apply_group_event) yields later. OpenMLS
//! does the opposite: `add_members` stages a *pending commit* in the group, and
//! only one can wait. So a `make` method works on a **scratch** copy of the
//! storage ([`SimProvider::scratch`]), keeps it with the hash of the material it
//! made, and gives it up when the event comes back from the log with that
//! material: then the scratch becomes the state. A rival's event instead goes
//! through `process_message` on the untouched state, and the scratch of the
//! loser is dropped.
//!
//! # What a device checks of a Commit
//!
//! A Commit of the group log is applied only if the devices of the group after
//! it are exactly the devices of the roster after the event ([`check_members`]),
//! each Add carries the key package of the roster, and the Commit has no other
//! proposal. This is the credential check of question 8 of #62, and it is the
//! same function of the log on every device: an `Approve` that Adds somebody
//! else is refused by every device alike, and the request stays open.
//!
//! # The key of a ping (question 1 of #62)
//!
//! Design B. Every epoch gives each device a key, the MLS exporter with the
//! label `e2e-openmls epoch key`. A ping begun in epoch *e* uses a key derived
//! from the key of *e* and the id of the ping, with plain AES-128-GCM. The first
//! event of the ping log names *e*. Design A (one MLS group for each ping) is
//! measured in `tests/numbers.rs`.
//!
//! - Nothing in a ping depends on the epoch of the *reader or the author*: a
//!   reader that is ahead, an author that writes late, a device that replays
//!   from the start all use the key of *e*. The price is that a device keeps 32
//!   bytes for each epoch in which a ping may still begin or be read, and that
//!   the ping events have no forward secrecy of their own: the key is kept so
//!   that the log can be read again (C13).
//! - A removal ends the ping (D10): the pings of the next epochs use keys that
//!   the removed user's devices cannot derive. They keep the keys they had, so
//!   they read the old ping (D10 accepts that).
//! - A new device of a known user gets the keys of its siblings in the material
//!   of `AddDevice`, sealed to its key package. A new user gets nothing.
//! - Every ping event carries, inside the ciphertext, the epoch of its author
//!   and the hash of the Commit that made it: "the hash of the Commit I merged"
//!   that the OpenMLS book suggests, without a new kind of event. A reader that
//!   merged another Commit for that epoch refuses the event (question 3 of #62).

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use e2e_harness::{
    cipher::{CipherError, GroupCipher, GroupEvent},
    ids::{GroupId, InviteCode, Material, PingId, Seq, UserId},
    keys::{DeviceKey, EventHash, Identity},
    payload::GroupChange,
    rng::SimRng,
    roster::{DeviceInfo, Roster},
};
use openmls::prelude::{
    AeadType, GroupId as MlsGroupId, HashType, HpkeAeadType, HpkeCiphertext, HpkeConfig,
    HpkeKdfType, HpkeKemType, KeyPackage, KeyPackageBundle, KeyPackageIn, KeyPackageRef,
    LeafNodeIndex, Lifetime, MlsGroup, MlsGroupCreateConfig, MlsGroupJoinConfig, MlsMessageBodyIn,
    MlsMessageIn, OpenMlsCrypto, OpenMlsProvider, OpenMlsRand, ProcessedMessageContent,
    ProtocolVersion, StagedCommit, StagedWelcome,
};
use openmls_traits::storage::StorageProvider;
use tls_codec::{Deserialize, Serialize};

use crate::{
    identity::{DeviceSigner, credential_of, device_key_of},
    provider::{CIPHERSUITE, SimProvider},
    wire::{Bundle, EpochEntry, Handover, Saved},
};

const EPOCH_LABEL: &str = "e2e-openmls epoch key";
const PING_INFO: &[u8] = b"e2e-openmls ping key";
const HANDOVER_INFO: &[u8] = b"e2e-openmls handover";

/// How many scratch copies a device keeps for events it has made and not yet
/// read back. Two are enough for a device that writes twice before it reads;
/// the oldest is dropped.
const PREPARED: usize = 4;

fn invalid(error: impl fmt::Display) -> CipherError {
    CipherError::Invalid(error.to_string())
}

fn hpke() -> HpkeConfig {
    HpkeConfig(
        HpkeKemType::DhKem25519,
        HpkeKdfType::HkdfSha256,
        HpkeAeadType::AesGcm128,
    )
}

fn create_config() -> MlsGroupCreateConfig {
    MlsGroupCreateConfig::builder()
        .ciphersuite(CIPHERSUITE)
        .use_ratchet_tree_extension(true)
        .lifetime(forever())
        .build()
}

fn join_config() -> MlsGroupJoinConfig {
    MlsGroupJoinConfig::builder()
        .use_ratchet_tree_extension(true)
        .build()
}

/// The lifetime of every key package and of the leaf of the founder: OpenMLS
/// validates it against the wall clock, so a lifetime that ends would make an old
/// Add fail on replay. The price is that a key package never expires.
fn forever() -> Lifetime {
    Lifetime::init(0, u64::MAX)
}

/// A key and the hash of the Commit of one epoch.
#[derive(Debug, Clone, Copy)]
struct Epoch {
    key: [u8; 32],
    commit: EventHash,
}

/// A state that a `make` method built on a copy, waiting for its event.
struct Prepared {
    id: EventHash,
    provider: SimProvider,
    group: MlsGroup,
}

/// The MLS candidate. See the module docs.
pub struct OpenMls {
    identity: Identity,
    me: DeviceKey,
    provider: SimProvider,
    group: Option<MlsGroup>,
    epochs: BTreeMap<u64, Epoch>,
    pings: BTreeMap<PingId, u64>,
    prepared: Vec<Prepared>,
    forks: usize,
}

impl fmt::Debug for OpenMls {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenMls")
            .field("device", &self.me)
            .field("epoch", &self.epoch())
            .field("epochs kept", &self.epochs.len())
            .field("pings", &self.pings.len())
            .finish_non_exhaustive()
    }
}

impl OpenMls {
    /// The epoch of the MLS group, or `None` before the device has one.
    pub fn epoch(&self) -> Option<u64> {
        self.group.as_ref().map(|group| group.epoch().as_u64())
    }

    /// The epochs whose key the device keeps.
    pub fn epochs_kept(&self) -> usize {
        self.epochs.len()
    }

    /// The members of the MLS group, as the device sees them.
    pub fn members(&self) -> Result<BTreeSet<DeviceKey>, CipherError> {
        self.group()?
            .members()
            .map(|member| device_key_of(&member.credential))
            .collect()
    }

    /// The size of the state of the device in bytes: what `export` writes.
    pub fn state_size(&self) -> usize {
        self.export().len()
    }

    /// How many ping events this device has read whose author had merged
    /// another Commit for the epoch than this device had: the evidence of a fork
    /// or of two states that diverged (see the module docs).
    pub fn forks_seen(&self) -> usize {
        self.forks
    }

    fn group(&self) -> Result<&MlsGroup, CipherError> {
        self.group
            .as_ref()
            .ok_or_else(|| CipherError::State("the device has no MLS group yet".into()))
    }

    fn signer(&self) -> DeviceSigner {
        DeviceSigner(self.identity.clone())
    }

    /// A copy of the state to work on, and the group loaded from it.
    fn scratch(&self) -> Result<(SimProvider, MlsGroup), CipherError> {
        let provider = self.provider.scratch();
        let group = MlsGroup::load(provider.storage(), self.group()?.group_id())
            .map_err(invalid)?
            .ok_or_else(|| CipherError::State("the group is not in the storage".into()))?;
        Ok((provider, group))
    }

    /// Keep a state that a `make` method built, until the event comes back.
    fn prepare(&mut self, material: Vec<u8>, provider: SimProvider, group: MlsGroup) -> Material {
        if self.prepared.len() >= PREPARED {
            self.prepared.remove(0);
        }
        self.prepared.push(Prepared {
            id: EventHash::of(&material),
            provider,
            group,
        });
        material.into()
    }

    /// The state that was made for exactly this material.
    fn take(&mut self, material: &Material) -> Result<Prepared, CipherError> {
        let id = EventHash::of(material.as_slice());
        let at = self
            .prepared
            .iter()
            .position(|prepared| prepared.id == id)
            .ok_or_else(|| {
                CipherError::State(
                    "the device did not make this event, or lost the state it made it from (see the module docs of e2e_openmls::cipher)"
                        .into(),
                )
            })?;
        Ok(self.prepared.remove(at))
    }

    fn adopt(&mut self, prepared: Prepared) {
        self.provider = prepared.provider;
        self.group = Some(prepared.group);
        self.prepared.clear();
    }

    /// Read a key package that a device published, and check that it is the
    /// key package of `device`.
    fn key_package(
        &self,
        introduction: &[u8],
        device: &DeviceKey,
    ) -> Result<KeyPackage, CipherError> {
        let key_package = KeyPackageIn::tls_deserialize_exact(introduction)
            .map_err(invalid)?
            .validate(self.provider.crypto(), ProtocolVersion::Mls10)
            .map_err(invalid)?;
        let named = device_key_of(key_package.leaf_node().credential())?;
        if named != *device {
            return Err(invalid(format!(
                "the key package names {named}, not {device}"
            )));
        }
        Ok(key_package)
    }

    /// Record the key of the epoch the group is in now.
    fn remember_epoch(&mut self, commit: EventHash) -> Result<(), CipherError> {
        let group = self.group()?;
        let key = group
            .export_secret(self.provider.crypto(), EPOCH_LABEL, &[], 32)
            .map_err(invalid)?
            .try_into()
            .map_err(|_| invalid("the exporter gave a key of the wrong length"))?;
        let epoch = group.epoch().as_u64();
        self.epochs.insert(epoch, Epoch { key, commit });
        Ok(())
    }

    fn handover(&self) -> Handover {
        Handover::new(
            self.epochs
                .iter()
                .map(|(epoch, kept)| EpochEntry {
                    epoch: *epoch,
                    key: kept.key,
                    commit: *kept.commit.as_bytes(),
                })
                .collect(),
        )
    }

    /// Seal what this device holds to the init key of a key package.
    fn seal_handover(
        &self,
        provider: &SimProvider,
        key_package: &KeyPackage,
    ) -> Result<Vec<u8>, CipherError> {
        provider
            .crypto()
            .hpke_seal(
                hpke(),
                key_package.hpke_init_key().as_slice(),
                HANDOVER_INFO,
                b"",
                &self.handover().encode(),
            )
            .map_err(invalid)?
            .tls_serialize_detached()
            .map_err(invalid)
    }

    /// Open what a sibling sealed to the key package this device introduced.
    fn open_handover(&self, introduction: &[u8], sealed: &[u8]) -> Result<Handover, CipherError> {
        let key_package = self.key_package(introduction, &self.me)?;
        let reference: KeyPackageRef = key_package
            .hash_ref(self.provider.crypto())
            .map_err(invalid)?;
        let bundle: KeyPackageBundle = self
            .provider
            .storage()
            .key_package(&reference)
            .map_err(invalid)?
            .ok_or_else(|| CipherError::NoKey("the key package of this device is gone".into()))?;
        let sealed = HpkeCiphertext::tls_deserialize_exact(sealed).map_err(invalid)?;
        let plain = self
            .provider
            .crypto()
            .hpke_open(
                hpke(),
                &sealed,
                bundle.init_private_key().as_ref(),
                HANDOVER_INFO,
                b"",
            )
            .map_err(invalid)?;
        Handover::decode(&plain).map_err(invalid)
    }

    /// An Add of `device`, with the handover of this device's keys if it is a
    /// new device of a known user.
    fn add(&mut self, device: &DeviceInfo, hand_over: bool) -> Result<Material, CipherError> {
        let key_package = self.key_package(device.introduction.as_slice(), &device.key)?;
        let (provider, mut group) = self.scratch()?;
        let (commit, welcome, _) = group
            .add_members(
                &provider,
                &self.signer(),
                std::slice::from_ref(&key_package),
            )
            .map_err(invalid)?;
        let handover = if hand_over {
            self.seal_handover(&provider, &key_package)?
        } else {
            Vec::new()
        };
        let material = Bundle {
            commit: commit.tls_serialize_detached().map_err(invalid)?.into(),
            welcome: welcome.tls_serialize_detached().map_err(invalid)?.into(),
            handover: handover.into(),
        }
        .encode();
        Ok(self.prepare(material, provider, group))
    }

    /// The device is the one this event adds, and another device adds it.
    fn is_added_by_another(&self, event: &GroupEvent<'_>) -> bool {
        if *event.author == self.me {
            return false;
        }
        match event.change {
            GroupChange::AddDevice { device, .. } => *device == self.me,
            GroupChange::Approve { .. } => event
                .roster
                .users()
                .filter(|user| user.admitted() == event.seq)
                .flat_map(|user| event.roster.devices_of(user))
                .any(|device| device.key == self.me),
            GroupChange::Create { .. }
            | GroupChange::Invite { .. }
            | GroupChange::Revoke { .. }
            | GroupChange::JoinRequest { .. }
            | GroupChange::Leave
            | GroupChange::Remove { .. } => false,
        }
    }

    fn apply_create(&mut self, event: &GroupEvent<'_>) -> Result<(), CipherError> {
        if *event.author != self.me {
            return Err(invalid("only the founder makes the group"));
        }
        let prepared = self.take(event.material)?;
        self.adopt(prepared);
        self.remember_epoch(EventHash::of(event.material.as_slice()))
    }

    fn apply_commit(&mut self, event: &GroupEvent<'_>) -> Result<(), CipherError> {
        let bundle = Bundle::decode(event.material.as_slice()).map_err(invalid)?;
        let commit_hash = EventHash::of(bundle.commit.as_slice());
        if *event.author == self.me {
            self.apply_own_commit(event, commit_hash)
        } else if self.is_added_by_another(event) {
            self.apply_welcome(event, &bundle, commit_hash)
        } else {
            self.apply_foreign_commit(event, &bundle, commit_hash)
        }
    }

    /// The Commit this device made: its scratch becomes the state.
    fn apply_own_commit(
        &mut self,
        event: &GroupEvent<'_>,
        commit_hash: EventHash,
    ) -> Result<(), CipherError> {
        let mut prepared = self.take(event.material)?;
        let staged = prepared
            .group
            .pending_commit()
            .ok_or_else(|| invalid("the state made for this event has no pending Commit"))?;
        check_members(&prepared.group, staged, event.roster)?;
        prepared
            .group
            .merge_pending_commit(&prepared.provider)
            .map_err(invalid)?;
        self.adopt(prepared);
        self.remember_epoch(commit_hash)
    }

    /// The Commit of another member, on the state the device has.
    fn apply_foreign_commit(
        &mut self,
        event: &GroupEvent<'_>,
        bundle: &Bundle,
        commit_hash: EventHash,
    ) -> Result<(), CipherError> {
        let group = self
            .group
            .as_mut()
            .ok_or_else(|| CipherError::State("the device has no MLS group yet".into()))?;
        let message = MlsMessageIn::tls_deserialize_exact(bundle.commit.as_slice())
            .map_err(invalid)?
            .try_into_protocol_message()
            .map_err(invalid)?;
        let processed = group
            .process_message(&self.provider, message)
            .map_err(invalid)?;
        let committer = device_key_of(processed.credential())?;
        if committer != *event.author {
            return Err(invalid(format!(
                "the Commit is signed by {committer}, not by the author {} of the event",
                event.author
            )));
        }
        let ProcessedMessageContent::StagedCommitMessage(staged) = processed.into_content() else {
            return Err(invalid("the message of the event is not a Commit"));
        };
        check_members(group, &staged, event.roster)?;
        group
            .merge_staged_commit(&self.provider, *staged)
            .map_err(invalid)?;
        self.remember_epoch(commit_hash)
    }

    /// The device is added: it joins from the Welcome, and reads the handover.
    fn apply_welcome(
        &mut self,
        event: &GroupEvent<'_>,
        bundle: &Bundle,
        commit_hash: EventHash,
    ) -> Result<(), CipherError> {
        let own = event
            .roster
            .devices()
            .find(|(_, device)| device.key == self.me)
            .map(|(_, device)| device)
            .ok_or_else(|| invalid("the device is not in the roster of its own Welcome"))?;
        let handover = (!bundle.handover.as_slice().is_empty())
            .then(|| self.open_handover(own.introduction.as_slice(), bundle.handover.as_slice()))
            .transpose()?;
        let welcome = match MlsMessageIn::tls_deserialize_exact(bundle.welcome.as_slice())
            .map_err(invalid)?
            .extract()
        {
            MlsMessageBodyIn::Welcome(welcome) => welcome,
            MlsMessageBodyIn::PublicMessage(_)
            | MlsMessageBodyIn::PrivateMessage(_)
            | MlsMessageBodyIn::GroupInfo(_)
            | MlsMessageBodyIn::KeyPackage(_) => {
                return Err(invalid("the Welcome of the event is not a Welcome"));
            }
        };
        let staged = StagedWelcome::new_from_welcome(&self.provider, &join_config(), welcome, None)
            .map_err(invalid)?;
        let members = staged
            .members()
            .map(|member| device_key_of(&member.credential))
            .collect::<Result<BTreeSet<_>, _>>()?;
        if members != roster_devices(event.roster) {
            return Err(invalid(
                "the devices in the Welcome are not the devices of the roster",
            ));
        }
        let group = staged.into_group(&self.provider).map_err(invalid)?;
        self.group = Some(group);
        self.prepared.clear();
        self.remember_epoch(commit_hash)?;
        for entry in handover.iter().flat_map(Handover::epochs) {
            self.epochs.entry(entry.epoch).or_insert(Epoch {
                key: entry.key,
                commit: EventHash::from_bytes(entry.commit),
            });
        }
        Ok(())
    }

    /// The key of a ping, from the key of its epoch.
    fn ping_key(&self, ping: PingId) -> Result<[u8; 16], CipherError> {
        let epoch = self
            .pings
            .get(&ping)
            .ok_or_else(|| CipherError::State(format!("{ping} is not open")))?;
        let kept = self
            .epochs
            .get(epoch)
            .ok_or_else(|| CipherError::NoKey(format!("epoch {epoch} of {ping}")))?;
        self.provider
            .crypto()
            .hkdf_expand(
                HashType::Sha2_256,
                &kept.key,
                &[PING_INFO, &ping.0.to_be_bytes()].concat(),
                16,
            )
            .map_err(invalid)?
            .as_slice()
            .try_into()
            .map_err(|_| invalid("the key of the ping has the wrong length"))
    }
}

fn roster_devices(roster: &Roster) -> BTreeSet<DeviceKey> {
    roster.devices().map(|(_, device)| device.key).collect()
}

/// The check of a Commit of the group log against the roster: see the module
/// docs. `group` is the state before the Commit.
fn check_members(
    group: &MlsGroup,
    staged: &StagedCommit,
    roster: &Roster,
) -> Result<(), CipherError> {
    let adds = staged.add_proposals().count();
    let removes = staged.remove_proposals().count();
    if staged.queued_proposals().count() != adds + removes {
        return Err(invalid(
            "a Commit of the group log carries Add and Remove proposals only",
        ));
    }
    let removed: BTreeSet<LeafNodeIndex> = staged
        .remove_proposals()
        .map(|proposal| proposal.remove_proposal().removed())
        .collect();
    let mut after = BTreeSet::new();
    for member in group.members() {
        if !removed.contains(&member.index) {
            after.insert(device_key_of(&member.credential)?);
        }
    }
    for add in staged.add_proposals() {
        let key_package = add.add_proposal().key_package();
        let device = device_key_of(key_package.leaf_node().credential())?;
        let introduced = roster
            .devices()
            .find(|(_, info)| info.key == device)
            .is_some_and(|(_, info)| {
                key_package
                    .tls_serialize_detached()
                    .is_ok_and(|bytes| bytes == info.introduction.as_slice())
            });
        if !introduced {
            return Err(invalid(format!(
                "the Commit adds a key package of {device} that the roster does not hold"
            )));
        }
        after.insert(device);
    }
    if after != roster_devices(roster) {
        return Err(invalid(
            "the devices of the group after the Commit are not the devices of the roster",
        ));
    }
    Ok(())
}

impl GroupCipher for OpenMls {
    const NAME: &'static str = "openmls";

    fn new(identity: &Identity, rng: SimRng) -> Self {
        Self {
            identity: identity.clone(),
            me: identity.device_key(),
            provider: SimProvider::new(rng),
            group: None,
            epochs: BTreeMap::new(),
            pings: BTreeMap::new(),
            prepared: Vec::new(),
            forks: 0,
        }
    }

    fn export(&self) -> Vec<u8> {
        Saved::new(
            self.group.as_ref().map(|group| group.group_id().as_slice()),
            self.provider.entries().into_iter().collect(),
            self.epochs
                .iter()
                .map(|(epoch, kept)| EpochEntry {
                    epoch: *epoch,
                    key: kept.key,
                    commit: *kept.commit.as_bytes(),
                })
                .collect(),
            &self.pings,
        )
        .encode()
    }

    fn restore(identity: &Identity, rng: SimRng, state: &[u8]) -> Result<Self, CipherError> {
        let saved = Saved::decode(state).map_err(invalid)?;
        let provider = SimProvider::restore(rng, saved.storage().collect());
        let group = saved
            .group()
            .map(|id| {
                MlsGroup::load(provider.storage(), &MlsGroupId::from_slice(id))
                    .map_err(invalid)?
                    .ok_or_else(|| invalid("the saved state names a group that it does not hold"))
            })
            .transpose()?;
        Ok(Self {
            identity: identity.clone(),
            me: identity.device_key(),
            provider,
            group,
            epochs: saved
                .epochs()
                .iter()
                .map(|entry| {
                    (
                        entry.epoch,
                        Epoch {
                            key: entry.key,
                            commit: EventHash::from_bytes(entry.commit),
                        },
                    )
                })
                .collect(),
            pings: saved.pings().collect(),
            prepared: Vec::new(),
            forks: 0,
        })
    }

    /// # Panics
    ///
    /// Panics when OpenMLS cannot build a key package, which it does not for a
    /// fresh key; the trait has no way to return the error.
    fn introduction(&mut self) -> Material {
        let bundle = openmls::prelude::KeyPackage::builder()
            .key_package_lifetime(forever())
            .build(
                CIPHERSUITE,
                &self.provider,
                &self.signer(),
                credential_of(&self.me),
            )
            .unwrap_or_else(|error| {
                panic!("OpenMLS cannot build a key package: {error} (see the module docs of e2e_openmls::cipher)")
            });
        bundle
            .key_package()
            .tls_serialize_detached()
            .expect("a key package fits its length prefix")
            .into()
    }

    fn create_group(&mut self, group: GroupId) -> Result<Material, CipherError> {
        if self.group.is_some() {
            return Err(CipherError::State("the device has a group already".into()));
        }
        let provider = self.provider.scratch();
        let mls = MlsGroup::new_with_group_id(
            &provider,
            &self.signer(),
            &create_config(),
            MlsGroupId::from_slice(&group.0.to_be_bytes()),
            credential_of(&self.me),
        )
        .map_err(invalid)?;
        let material = mls.public_group().group_context().tree_hash().to_vec();
        Ok(self.prepare(material, provider, mls))
    }

    fn invite(&mut self, _code: &InviteCode) -> Result<Material, CipherError> {
        Ok(Material::empty())
    }

    fn revoke_code(&mut self, _code: &InviteCode) -> Result<Material, CipherError> {
        Ok(Material::empty())
    }

    fn approve_join(
        &mut self,
        _roster: &Roster,
        joiner: &DeviceInfo,
    ) -> Result<Material, CipherError> {
        self.add(joiner, false)
    }

    fn add_device(
        &mut self,
        _roster: &Roster,
        device: &DeviceInfo,
    ) -> Result<Material, CipherError> {
        self.add(device, true)
    }

    fn remove_member(&mut self, roster: &Roster, user: UserId) -> Result<Material, CipherError> {
        let (provider, mut group) = self.scratch()?;
        let leaves = roster
            .devices_of(user)
            .map(|device| {
                group
                    .member_leaf_index(&credential_of(&device.key).credential)
                    .ok_or_else(|| {
                        invalid(format!(
                            "{} is in the roster and not in the MLS group",
                            device.key
                        ))
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let (commit, _, _) = group
            .remove_members(&provider, &self.signer(), &leaves)
            .map_err(invalid)?;
        let material = Bundle {
            commit: commit.tls_serialize_detached().map_err(invalid)?.into(),
            welcome: Vec::new().into(),
            handover: Vec::new().into(),
        }
        .encode();
        Ok(self.prepare(material, provider, group))
    }

    fn apply_group_event(&mut self, event: &GroupEvent<'_>) -> Result<(), CipherError> {
        match event.change {
            GroupChange::Create { .. } => self.apply_create(event),
            GroupChange::Approve { .. }
            | GroupChange::AddDevice { .. }
            | GroupChange::Remove { .. } => self.apply_commit(event),
            GroupChange::Invite { .. }
            | GroupChange::Revoke { .. }
            | GroupChange::JoinRequest { .. }
            | GroupChange::Leave => Ok(()),
        }
    }

    fn start_ping(&mut self, _ping: PingId, _group_head: Seq) -> Result<Material, CipherError> {
        let epoch = self.group()?.epoch().as_u64();
        Ok(epoch.to_be_bytes().to_vec().into())
    }

    fn open_ping(
        &mut self,
        ping: PingId,
        _group_head: Seq,
        material: &Material,
    ) -> Result<(), CipherError> {
        let epoch = u64::from_be_bytes(
            material
                .as_slice()
                .try_into()
                .map_err(|_| invalid("the first event of a ping names an epoch of 8 bytes"))?,
        );
        if !self.epochs.contains_key(&epoch) {
            return Err(CipherError::NoKey(format!("epoch {epoch} of {ping}")));
        }
        self.pings.insert(ping, epoch);
        Ok(())
    }

    fn encrypt_ping(&mut self, ping: PingId, plaintext: &[u8]) -> Result<Vec<u8>, CipherError> {
        let key = self.ping_key(ping)?;
        let epoch = self.group()?.epoch().as_u64();
        let claim = self
            .epochs
            .get(&epoch)
            .ok_or_else(|| CipherError::State(format!("the key of epoch {epoch} is not kept")))?
            .commit;
        let nonce = self.provider.rand().random_array::<12>().map_err(invalid)?;
        let body = [epoch.to_be_bytes().as_slice(), claim.as_bytes(), plaintext].concat();
        let sealed = self
            .provider
            .crypto()
            .aead_encrypt(
                AeadType::Aes128Gcm,
                &key,
                &body,
                &nonce,
                &ping.0.to_be_bytes(),
            )
            .map_err(invalid)?;
        Ok([nonce.as_slice(), &sealed].concat())
    }

    fn decrypt_ping(
        &mut self,
        ping: PingId,
        _seq: Seq,
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, CipherError> {
        let key = self.ping_key(ping)?;
        let (nonce, sealed) = ciphertext
            .split_at_checked(12)
            .ok_or_else(|| invalid("a ping event is shorter than its nonce"))?;
        let body = self
            .provider
            .crypto()
            .aead_decrypt(
                AeadType::Aes128Gcm,
                &key,
                sealed,
                nonce,
                &ping.0.to_be_bytes(),
            )
            .map_err(|error| CipherError::NoKey(format!("{ping}: {error}")))?;
        let (epoch, rest) = body
            .split_first_chunk::<8>()
            .ok_or_else(|| invalid("a ping event has no epoch"))?;
        let (claimed, plaintext) = rest
            .split_first_chunk::<32>()
            .ok_or_else(|| invalid("a ping event has no hash of a Commit"))?;
        let epoch = u64::from_be_bytes(*epoch);
        if let Some(kept) = self.epochs.get(&epoch)
            && kept.commit.as_bytes() != claimed
        {
            self.forks += 1;
            return Err(invalid(format!(
                "the author merged another Commit for epoch {epoch} than this device did: the group log was forked, or the two states diverged (see the module docs of e2e_openmls::cipher)"
            )));
        }
        Ok(plaintext.to_vec())
    }
}
