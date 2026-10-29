// SPDX-License-Identifier: GPL-3.0-or-later
//! Source-qualified static identity and committed-frame pose-cell observations.
//! Not CurrentCell, grid loading, registration, lookup, or movement authority.
use super::*;
use crate::{NpcId, PlayerId, WorldId};
use benilla_sim::resident_map::{compute_cell_pair, CellPair};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
const POLICY: &str = "cmangos-8ec338a1-creature-pose-cell-index-v1";

/// Static GridLoader/LoadFromDB identity candidate, not proof of AddToWorld.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceStaticCreatureIdentity {
    database_guid: u32,
    entry: u32,
    guid: u64,
}
impl SourceStaticCreatureIdentity {
    pub fn database_guid(self) -> u32 {
        self.database_guid
    }
    pub fn entry(self) -> u32 {
        self.entry
    }
    pub fn guid(self) -> u64 {
        self.guid
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CreaturePoseCellSource {
    policy: &'static str,
    map: crate::Digest,
    sources: BTreeMap<u64, (crate::Digest, SourceStaticCreatureIdentity)>,
}
impl CreaturePoseCellSource {
    fn build(p: &Profile) -> Result<Self, NativeError> {
        let map = p
            .map_classification
            .as_ref()
            .ok_or(NativeError::UnsupportedMechanic(
                "pose-cell loaded map classification",
            ))?;
        map.validate(p)?;
        if matches!(p.map_id, 533 | 543)
            || map.kind() != MapKind::Common
            || p.source_motion.is_none()
        {
            return Err(NativeError::UnsupportedMechanic(
                "static ordinary source pose-cell domain",
            ));
        }
        let generations =
            p.creature_generations
                .as_ref()
                .ok_or(NativeError::UnsupportedMechanic(
                    "pose-cell source generations",
                ))?;
        let mut sources = BTreeMap::new();
        let mut guids = BTreeSet::new();
        for npc in &p.npcs {
            let spawn = npc.ai.creature_id;
            let source = generations
                .source(spawn)
                .ok_or(NativeError::InvalidProfile)?;
            let entry = source.input.template.entry;
            // GridLoader's ordinary non-event caller supplies dbGuid as newGuid.
            // Existing complete external-feature contract excludes event/group
            // substitutions. No new absence flags or private consumer are added.
            if spawn == 0
                || spawn > 0xff_ffff
                || entry == 0
                || entry > 0xff_ffff
                || source.spawn_id != spawn
                || source.input.spawn_id != spawn
                || source.spawn_entry != entry
                || npc.entry != entry
                || source.input.template.extra_flags & 0x0010_0000 != 0
                || !matches!(&source.input.external_features, benilla_sim::creature_init::Rows::Complete(rows) if rows.is_empty())
            {
                return Err(NativeError::UnsupportedMechanic(
                    "static source creature identity",
                ));
            }
            if let Some(owner) = p
                .event_ai()
                .and_then(|catalog| catalog.catalog().owner(spawn))
            {
                if u64::from(owner.database_guid) != spawn || owner.entry != entry {
                    return Err(NativeError::UnsupportedMechanic(
                        "conflicting source creature database identity",
                    ));
                }
            }
            let identity = SourceStaticCreatureIdentity {
                database_guid: spawn as u32,
                entry,
                guid: 0xf130_0000_0000_0000 | (u64::from(entry) << 24) | spawn,
            };
            if !guids.insert(identity.guid)
                || sources
                    .insert(spawn, (source.identity(), identity))
                    .is_some()
            {
                return Err(NativeError::InvalidProfile);
            }
            cell(npc.ai.spawn)?;
        }
        Ok(Self {
            policy: POLICY,
            map: map.binding(),
            sources,
        })
    }
    pub(super) fn validate(&self, p: &Profile) -> Result<(), NativeError> {
        if self != &Self::build(p)? {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(())
    }
}
impl Profile {
    /// Opt into observations derived from already validated source owners only.
    /// This neither executes Map::Add nor enables lookup or native admission.
    pub fn initialize_creature_pose_cells(&mut self) -> Result<(), NativeError> {
        self.validate()?;
        let source = CreaturePoseCellSource::build(self)?;
        if self
            .creature_pose_cells
            .as_ref()
            .is_some_and(|old| old != &source)
        {
            return Err(NativeError::InvalidProfile);
        }
        let mut next = self.clone();
        next.creature_pose_cells = Some(source);
        next.validate()?;
        *self = next;
        Ok(())
    }
}
fn cell(position: ai::Point) -> Result<CellPair, NativeError> {
    compute_cell_pair(position.x.bits(), position.y.bits())
        .map_err(|_| NativeError::UnsupportedMechanic("source pose-cell coordinate domain"))
}
fn key(cell: CellPair) -> u32 {
    u32::from(cell.x()) * 1024 + u32::from(cell.y())
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreaturePoseCell {
    identity: SourceStaticCreatureIdentity,
    npc: NpcId,
    cell: CellPair,
}
impl CreaturePoseCell {
    pub fn identity(&self) -> SourceStaticCreatureIdentity {
        self.identity
    }
    pub fn npc(&self) -> NpcId {
        self.npc
    }
    pub fn cell(&self) -> CellPair {
        self.cell
    }
}
/// Derived committed-frame index. No registered/in-world/loaded state exists here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreaturePoseCellIndex {
    world: WorldId,
    player: PlayerId,
    profile: crate::Digest,
    entries: BTreeMap<u64, CreaturePoseCell>,
    cells: BTreeMap<u32, BTreeSet<u64>>,
}
impl CreaturePoseCellIndex {
    pub fn entries(&self) -> impl Iterator<Item = (&u64, &CreaturePoseCell)> {
        self.entries.iter()
    }
    pub fn in_cell(&self, cell: CellPair) -> impl Iterator<Item = u64> + '_ {
        self.cells
            .get(&key(cell))
            .filter(|_| cell.validate())
            .into_iter()
            .flat_map(|ids| ids.iter().copied())
    }
    pub(super) fn capture(p: &Profile, s: &State) -> Result<Option<Self>, NativeError> {
        let Some(source) = &p.creature_pose_cells else {
            return Ok(None);
        };
        source.validate(p)?;
        if source.sources.len() != s.npcs.len() {
            return Err(NativeError::InvalidState);
        }
        let mut entries = BTreeMap::new();
        let mut cells: BTreeMap<u32, BTreeSet<u64>> = BTreeMap::new();
        for (&spawn, npc) in &s.npcs {
            let (source_identity, identity) = source
                .sources
                .get(&spawn)
                .ok_or(NativeError::InvalidState)?;
            let generation = npc
                .source_generation
                .as_ref()
                .ok_or(NativeError::InvalidState)?;
            let input = p
                .creature_generations
                .as_ref()
                .and_then(|g| g.source(spawn))
                .ok_or(NativeError::InvalidState)?;
            if input.identity() != *source_identity
                || npc.id.spawn != spawn
                || generation.npc_id() != npc.id
                || generation.result().entry != identity.entry
            {
                return Err(NativeError::InvalidState);
            }
            let cell = cell(npc.ai.position())?;
            entries.insert(
                spawn,
                CreaturePoseCell {
                    identity: *identity,
                    npc: npc.id,
                    cell,
                },
            );
            cells.entry(key(cell)).or_default().insert(spawn);
        }
        Ok(Some(Self {
            world: s.world,
            player: s.player,
            profile: p.digest(),
            entries,
            cells,
        }))
    }
    pub(super) fn validate(p: &Profile, s: &State) -> Result<(), NativeError> {
        if s.creature_pose_cells != Self::capture(p, s)? {
            return Err(NativeError::InvalidCheckpoint);
        }
        Ok(())
    }
}
impl State {
    pub fn creature_pose_cells(&self) -> Option<&CreaturePoseCellIndex> {
        self.creature_pose_cells.as_ref()
    }
}
