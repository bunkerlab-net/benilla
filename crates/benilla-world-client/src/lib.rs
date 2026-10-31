// SPDX-License-Identifier: GPL-3.0-or-later
//! Pure, tested integration seam. No networking, Bevy, asset aliases, world admission,
//! GUI enabling or save-file writes. The original headless fixture lacks client bindings.
#![forbid(unsafe_code)]
pub mod corpse_appearance;
mod interactions;
mod metadata;
pub use metadata::QuestMetadata;

use benilla_protocol::{messages as protocol, EntityKind, ObjectFields, SessionEvent};
use benilla_world_data::QuestRelationKind;
use benilla_world_runtime as runtime;
use runtime::{Digest, Input, NpcId, OwnedInput, OwnerId, PlayerId, World};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

const MAX_ENVELOPE_BYTES: usize = 4096;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    UnboundMap(u32),
    UnboundCreatureDisplay(u32),
    UnboundPlayerDisplay(u32),
    UnboundItemDisplay(u32),
    UnboundSpell(u32),
    InvalidClientManifest,
    Incompatible,
    InvalidMembership,
    UnknownPlayer,
    UnknownTarget,
    GuidCollision,
    Unrepresentable,
    StaleProjection,
    InvalidEnvelope,
    TooLarge,
    WrongOwner,
    WrongCharacter,
    WrongTick,
    InvalidSequence,
    UnsupportedCommand,
    UnsupportedPower,
    MissingMovementOrderState,
    MissingItemMetadata(u32),
    InvalidItemMetadata(u32),
    MissingQuestMetadata(u32),
    InvalidQuestMetadata(u32),
    MissingQuestGreeting(u32),
    OutOfRange,
    UnavailableQuest,
    LootNotOpen,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "world client boundary: {self:?}")
    }
}
impl std::error::Error for Error {}
/// Implement against validated tables read from the user's own client installation.
/// Presence checks establish renderability only, never gameplay parity or data rights.
/// No remapping of fixture ability IDs to unrelated real spells is permitted.
pub trait ClientCatalog {
    fn build(&self) -> u32;
    fn manifest_digest(&self) -> Digest;
    fn contains_map(&self, id: u32) -> bool;
    fn contains_creature_display(&self, id: u32) -> bool;
    fn contains_item_display(&self, id: u32) -> bool;
    fn contains_spell(&self, id: u32) -> bool;
    /// Selected profile class/presentation binding: v1 runtime exposes mana only.
    fn player_power_type(&self, player: PlayerId) -> Option<u8>;
    /// Exact existing PLAYER_BYTES_2, including appearance and purchased-bank bytes.
    /// Native progression replaces only its high rest-state byte.
    fn player_bytes_2(&self, _player: PlayerId) -> Option<u32> {
        None
    }
    fn item_metadata(&self, entry: u32) -> Option<protocol::ItemInfo>;
    /// Exact sSpellTemplate query evidence for item metadata. None is unknown, never absent.
    /// This is distinct from client renderability or admission to cast/use the spell.
    fn native_item_spell_query(&self, _spell: u32) -> Option<NativeItemSpellQuery> {
        None
    }
    fn quest_metadata(&self, entry: u32) -> Option<QuestMetadata>;
    fn quest_greeting(&self, creature_entry: u32) -> Option<String>;
    /// Actual resolved questgiver greeting row or NPC gossip fallback, including
    /// localized text and emotes. None means unknown, never an empty source row.
    fn native_quest_greeting(&self, _creature_entry: u32) -> Option<NativeQuestGreeting> {
        None
    }
    /// Actual raw quest emote columns. The native packet subset currently requires
    /// absent details/offer emotes and zero request delay; unknown rows reject.
    fn native_quest_presentation(&self, _quest: u32) -> Option<NativeQuestPresentation> {
        None
    }
}
/// Source item-query lookup after the selected server spell table is resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeItemSpellQuery {
    Missing,
    Present {
        recovery_time: u32,
        category: u32,
        category_recovery_time: u32,
    },
}
/// Caller-owned source presentation capture, bound by the client manifest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeQuestGreeting {
    /// True only for an actual questgiver_greeting row; false uses the fully
    /// resolved NPC gossip fallback (including a proven absent fallback row).
    pub has_greeting_row: bool,
    pub text: String,
    pub emote_delay: u32,
    pub emote: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeQuestPresentation {
    pub details_emotes: [(u32, u32); 4],
    pub offer_emotes: [(u32, u32); 4],
    pub complete_emote: u32,
    pub complete_emote_delay: u32,
    pub incomplete_emote: u32,
    pub incomplete_emote_delay: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Compatibility {
    pub world: runtime::WorldId,
    pub content_digest: Digest,
    pub rules_digest: Digest,
    pub client_manifest: Digest,
    pub membership_digest: Digest,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub version: u32,
    pub compatibility: Compatibility,
    pub base_revision: u64,
    pub base_state_digest: Digest,
    pub input: Input,
}
impl Envelope {
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_ENVELOPE_BYTES {
            return Err(Error::TooLarge);
        }
        serde_json::from_slice(bytes).map_err(|_| Error::InvalidEnvelope)
    }
    pub fn encode(&self) -> Result<Vec<u8>, Error> {
        let bytes = serde_json::to_vec(self).map_err(|_| Error::InvalidEnvelope)?;
        if bytes.len() > MAX_ENVELOPE_BYTES {
            return Err(Error::TooLarge);
        }
        Ok(bytes)
    }
}
/// Capture a GUI guid into its exact spawn generation when the input is received, before
/// queuing it. A later respawn must not retarget that queued interaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NpcHandle {
    pub id: NpcId,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiRequest {
    MoveDelta([i32; 3]),
    MeleeOnce(NpcHandle),
    CastAtNpc {
        ability: u32,
        target: NpcHandle,
    },
    CastSelf {
        ability: u32,
    },
    AcceptQuest {
        quest: u32,
        giver: NpcHandle,
    },
    TurnInQuest {
        quest: u32,
        giver: NpcHandle,
    },
    ChooseQuestReward {
        quest: u32,
        giver: NpcHandle,
        choice: u32,
    },
    OpenLoot(NpcHandle),
    TakeLootMoney(NpcHandle),
    TakeLootItem {
        target: NpcHandle,
        slot: u8,
    },
    ReleaseLoot(NpcHandle),
    Revive,
}
/// Frozen member set. Transport authentication supplies owners; an Envelope never does.
/// Validation happens before collecting a world tick so a forged peer cannot cancel solo work.
pub struct CommandAdapter {
    selected: PlayerId,
    compatibility: Compatibility,
    members: BTreeMap<OwnerId, PlayerId>,
}
impl CommandAdapter {
    pub fn new(
        world: &World,
        selected: PlayerId,
        catalog: &impl ClientCatalog,
    ) -> Result<Self, Error> {
        validate_catalog(world, catalog)?;
        if catalog.player_power_type(selected) != Some(0) {
            return Err(Error::UnsupportedPower);
        }
        let members: BTreeMap<_, _> = world
            .state()
            .players
            .iter()
            .map(|p| (p.id.owner, p.id))
            .collect();
        if world.player(selected).is_none() {
            return Err(Error::UnknownPlayer);
        }
        let compatibility = Compatibility {
            world: world.state().world,
            content_digest: world.data().content_digest(),
            rules_digest: world.rules().digest(),
            client_manifest: catalog.manifest_digest(),
            membership_digest: membership(world),
        };
        Ok(Self {
            selected,
            compatibility,
            members,
        })
    }
    pub fn compatibility(&self) -> &Compatibility {
        &self.compatibility
    }
    fn check_world(&self, world: &World) -> Result<(), Error> {
        if world.state().world != self.compatibility.world
            || world.data().content_digest() != self.compatibility.content_digest
            || world.rules().digest() != self.compatibility.rules_digest
            || membership(world) != self.compatibility.membership_digest
        {
            return Err(Error::Incompatible);
        }
        Ok(())
    }
    /// Intent translation only; runtime validates range/resources/state. Tick/owner/sequence
    /// are taken from the selected world, never from GUI parameters.
    pub fn local(&self, world: &World, request: UiRequest) -> Result<OwnedInput, Error> {
        self.check_world(world)?;
        let command = match request {
            UiRequest::MoveDelta(delta) => runtime::Command::Move { delta },
            UiRequest::MeleeOnce(target) => runtime::Command::Melee { target: target.id },
            UiRequest::CastAtNpc { ability, target } => runtime::Command::Cast {
                ability,
                target: runtime::Target::Npc { id: target.id },
            },
            UiRequest::CastSelf { ability } => runtime::Command::Cast {
                ability,
                target: runtime::Target::SelfPlayer,
            },
            UiRequest::AcceptQuest { quest, giver } => runtime::Command::AcceptQuest {
                quest,
                giver: giver.id,
            },
            UiRequest::TurnInQuest { quest, giver } => runtime::Command::TurnInQuest {
                quest,
                giver: giver.id,
            },
            UiRequest::ChooseQuestReward { .. } => return Err(Error::UnsupportedCommand),
            UiRequest::OpenLoot(target) => runtime::Command::OpenLoot { target: target.id },
            UiRequest::TakeLootMoney(target) => {
                if self.capture_loot(world)? != target {
                    return Err(Error::LootNotOpen);
                }
                runtime::Command::TakeLootMoney { target: target.id }
            }
            UiRequest::TakeLootItem { target, slot } => {
                if self.capture_loot(world)? != target {
                    return Err(Error::LootNotOpen);
                }
                runtime::Command::TakeLootItem {
                    target: target.id,
                    slot,
                }
            }
            UiRequest::ReleaseLoot(target) => runtime::Command::ReleaseLoot { target: target.id },
            UiRequest::Revive => runtime::Command::Revive,
        };
        let player = world.player(self.selected).ok_or(Error::UnknownPlayer)?;
        Ok(OwnedInput {
            authenticated_owner: self.selected.owner,
            input: Input {
                world: world.state().world,
                tick: world.state().tick.checked_add(1).ok_or(Error::WrongTick)?,
                player: self.selected,
                sequence: player.next_sequence,
                command,
            },
        })
    }
    /// Capture the active corpse generation when a stock slot/money request arrives,
    /// before queuing it. A later window switch must not retarget that request.
    pub fn capture_loot(&self, world: &World) -> Result<NpcHandle, Error> {
        self.check_world(world)?;
        world
            .player(self.selected)
            .and_then(|p| p.active_loot)
            .map(|id| NpcHandle { id })
            .ok_or(Error::LootNotOpen)
    }
    pub fn envelope(&self, world: &World, input: Input) -> Result<Envelope, Error> {
        self.check_world(world)?;
        if matches!(input.command, runtime::Command::Loot { .. }) {
            return Err(Error::UnsupportedCommand);
        }
        Ok(Envelope {
            version: 1,
            compatibility: self.compatibility.clone(),
            base_revision: world.state().revision,
            base_state_digest: world.state_digest(),
            input,
        })
    }
    /// The owner argument MUST come from the already authenticated connection context.
    /// This function does not authenticate a PSK, key, signature or session itself.
    pub fn remote(
        &self,
        world: &World,
        authenticated_owner: OwnerId,
        envelope: Envelope,
    ) -> Result<OwnedInput, Error> {
        self.check_world(world)?;
        if envelope.version != 1
            || envelope.compatibility != self.compatibility
            || envelope.base_revision != world.state().revision
            || envelope.base_state_digest != world.state_digest()
        {
            return Err(Error::Incompatible);
        }
        let input = envelope.input;
        if input.player.owner != authenticated_owner {
            return Err(Error::WrongOwner);
        }
        let member = self
            .members
            .get(&authenticated_owner)
            .ok_or(Error::InvalidMembership)?;
        if *member != input.player {
            return Err(Error::WrongCharacter);
        }
        if input.world != world.state().world {
            return Err(Error::Incompatible);
        }
        if input.tick != world.state().tick + 1 {
            return Err(Error::WrongTick);
        }
        if world
            .player(*member)
            .ok_or(Error::UnknownPlayer)?
            .next_sequence
            != input.sequence
        {
            return Err(Error::InvalidSequence);
        }
        if matches!(input.command, runtime::Command::Loot { .. }) {
            return Err(Error::UnsupportedCommand);
        }
        Ok(OwnedInput {
            authenticated_owner,
            input,
        })
    }
}
fn hash(domain: &[u8], value: &impl Serialize) -> Digest {
    let mut hash = Sha256::new();
    hash.update(domain);
    hash.update(serde_json::to_vec(value).unwrap());
    hash.finalize().into()
}
fn membership(world: &World) -> Digest {
    hash(
        b"benilla-world-membership-v1\0",
        &world
            .state()
            .players
            .iter()
            .map(|p| p.id)
            .collect::<Vec<_>>(),
    )
}
fn validate_catalog(world: &World, catalog: &impl ClientCatalog) -> Result<(), Error> {
    if catalog.build() != 5875 || catalog.manifest_digest() == [0; 32] {
        return Err(Error::InvalidClientManifest);
    }
    if !catalog.contains_map(world.rules().start.map) {
        return Err(Error::UnboundMap(world.rules().start.map));
    }
    for spawn in &world.data().export().spawns {
        if !catalog.contains_map(spawn.map_id) {
            return Err(Error::UnboundMap(spawn.map_id));
        }
    }
    for c in &world.data().export().creatures {
        if c.display_id == 0 || !catalog.contains_creature_display(c.display_id) {
            return Err(Error::UnboundCreatureDisplay(c.entry));
        }
    }
    for i in &world.data().export().items {
        if i.display_id == 0 || !catalog.contains_item_display(i.display_id) {
            return Err(Error::UnboundItemDisplay(i.entry));
        }
    }
    if world.data().export().quests.len() > 20
        || world.rules().inventory_slots > 16
        || world.data().export().quests.iter().any(|q| {
            q.objectives.len() > 4
                || q.reward_items.len() > 4
                || q.objectives.iter().any(|o| o.count > 63)
        })
    {
        return Err(Error::Unrepresentable);
    }
    for ability in &world.data().export().abilities {
        if ability.entry > u16::MAX.into() {
            return Err(Error::Unrepresentable);
        }
        if !catalog.contains_spell(ability.entry) {
            return Err(Error::UnboundSpell(ability.entry));
        }
    }
    metadata::Metadata::load(world, catalog)?;
    Ok(())
}
fn player_guid(id: PlayerId) -> u64 {
    let mut hash = Sha256::new();
    hash.update(b"benilla-character-guid-v1\0");
    hash.update(id.owner);
    hash.update(id.character);
    let mut bytes = [0; 8];
    bytes[..6].copy_from_slice(&hash.finalize()[..6]);
    u64::from_le_bytes(bytes).max(1)
}
fn npc_guid(world: &World, id: NpcId) -> Result<u64, Error> {
    let spawn = world.data().spawn(id.spawn).ok_or(Error::UnknownTarget)?;
    if spawn.creature_entry > 0xff_ffff {
        return Err(Error::Unrepresentable);
    }
    let digest = hash(b"benilla-world-npc-guid-v1\0", &(world.state().world, id));
    let counter = u32::from_le_bytes([digest[0], digest[1], digest[2], 0]).max(1);
    Ok(0xf130_0000_0000_0000 | (u64::from(spawn.creature_entry) << 24) | u64::from(counter))
}
// Imported text is plain text; neutralize the stock UI's hyperlink/color escape prefix.
fn plain_ui_text(value: &str) -> String {
    value.replace('|', "||")
}
fn u32_value(value: u64) -> Result<u32, Error> {
    value.try_into().map_err(|_| Error::Unrepresentable)
}

/// Projection emits existing protocol events; it cannot make unknown client IDs render.
/// A new projection starts at one validated boundary; incremental events require every tick.
pub struct Projection {
    adapter: CommandAdapter,
    previous: runtime::State,
    previous_digest: Digest,
    npcs: BTreeMap<NpcId, u64>,
    players: BTreeMap<PlayerId, u64>,
    metadata: metadata::Metadata,
}
impl Projection {
    pub fn new(
        world: &World,
        selected: PlayerId,
        catalog: &impl ClientCatalog,
    ) -> Result<Self, Error> {
        let adapter = CommandAdapter::new(world, selected, catalog)?;
        let players = world
            .state()
            .players
            .iter()
            .map(|p| (p.id, player_guid(p.id)))
            .collect::<BTreeMap<_, _>>();
        let mut out = Self {
            adapter,
            previous: world.state().clone(),
            previous_digest: world.state_digest(),
            npcs: BTreeMap::new(),
            players,
            metadata: metadata::Metadata::load(world, catalog)?,
        };
        out.npcs = out.npc_guids(world)?;
        out.player_values(world)?;
        out.item_events(world, true)?;
        Ok(out)
    }
    pub fn commands(&self) -> &CommandAdapter {
        &self.adapter
    }
    pub fn player_guid(&self, id: PlayerId) -> Option<u64> {
        self.players.get(&id).copied()
    }
    pub fn npc_guid(&self, id: NpcId) -> Option<u64> {
        self.npcs.get(&id).copied()
    }
    pub fn capture_npc(&self, guid: u64) -> Result<NpcHandle, Error> {
        self.npcs
            .iter()
            .find(|(_, g)| **g == guid)
            .map(|(id, _)| NpcHandle { id: *id })
            .ok_or(Error::UnknownTarget)
    }
    fn npc_guids(&self, world: &World) -> Result<BTreeMap<NpcId, u64>, Error> {
        let mut seen: BTreeSet<_> = self.players.values().copied().collect();
        if seen.len() != self.players.len() {
            return Err(Error::GuidCollision);
        }
        let mut out = BTreeMap::new();
        for npc in &world.state().npcs {
            let guid = npc_guid(world, npc.id)?;
            if !seen.insert(guid) {
                return Err(Error::GuidCollision);
            }
            out.insert(npc.id, guid);
        }
        Ok(out)
    }
    pub fn initial_events(&self, world: &World) -> Result<Vec<SessionEvent>, Error> {
        self.adapter.check_world(world)?;
        if self.previous != *world.state() {
            return Err(Error::StaleProjection);
        }
        let mut events = Vec::new();
        for npc in &world.state().npcs {
            events.extend(self.npc_create(world, npc)?);
        }
        events.extend(self.item_events(world, true)?);
        events.push(self.player_values(world)?);
        for ability in &world.player(self.adapter.selected).unwrap().abilities {
            // Catalog validation is exact-ID; there are no aliases to actual Classic spells.
            if *ability > u16::MAX.into() {
                return Err(Error::Unrepresentable);
            }
        }
        events.push(SessionEvent::SpellBook {
            spell_ids: world
                .player(self.adapter.selected)
                .unwrap()
                .abilities
                .iter()
                .copied()
                .collect(),
            cooldowns: Vec::new(),
        });
        let cooldowns = world
            .player(self.adapter.selected)
            .unwrap()
            .cooldowns
            .iter()
            .filter_map(|(id, ready)| {
                ready
                    .checked_sub(world.state().tick)
                    .filter(|ticks| *ticks > 0)
                    .map(|ticks| (*id, ticks))
            })
            .map(|(id, ticks)| Ok((id, u32_value(ticks * u64::from(world.rules().tick_ms))?)))
            .collect::<Result<Vec<_>, Error>>()?;
        if !cooldowns.is_empty() {
            events.push(SessionEvent::SpellCooldowns {
                caster: self.players[&self.adapter.selected],
                cooldowns,
            });
        }
        if let Some(cast) = &world.player(self.adapter.selected).unwrap().casting {
            events.push(SessionEvent::SpellStart {
                caster: self.players[&self.adapter.selected],
                spell_id: cast.ability,
                cast_flags: 0,
                cast_time_ms: u32_value(
                    (cast.completes_at - world.state().tick) * u64::from(world.rules().tick_ms),
                )?,
                target: self
                    .target_guid(cast.target, self.adapter.selected, &self.npcs)
                    .ok(),
                ammo_display_id: None,
            });
        }
        if let Some(id) = world.player(self.adapter.selected).unwrap().active_loot {
            events.push(self.loot_response(world, id)?);
        }
        events.extend(self.quest_status_events(world)?);
        Ok(events)
    }
    fn npc_fields(&self, world: &World, npc: &runtime::Npc) -> ObjectFields {
        let spawn = world.data().spawn(npc.id.spawn).unwrap();
        let c = world.data().creature(spawn.creature_entry).unwrap();
        let lootable =
            npc.health == 0 && !npc.looted && npc.tagged_by == Some(self.adapter.selected);
        let quest_giver = world
            .data()
            .export()
            .quest_relations
            .iter()
            .any(|r| r.creature_entry == c.entry);
        ObjectFields::from_pairs(&[
            (3, c.entry),
            (22, npc.health),
            (28, c.max_health),
            (34, u32::from(c.level)),
            (35, c.faction_template_id),
            (131, c.display_id),
            (132, c.display_id),
            (130, (c.combat_reach_milliunits as f32 / 1000.0).to_bits()),
            (143, u32::from(lootable)),
            (147, if quest_giver { 2 } else { 0 }),
        ])
    }
    fn npc_create(&self, world: &World, npc: &runtime::Npc) -> Result<Vec<SessionEvent>, Error> {
        let spawn = world.data().spawn(npc.id.spawn).unwrap();
        let c = world.data().creature(spawn.creature_entry).unwrap();
        let guid = npc_guid(world, npc.id)?;
        let mut fields = self.npc_fields(world, npc);
        fields.merge(ObjectFields::from_pairs(&[
            (0, guid as u32),
            (1, (guid >> 32) as u32),
            (2, 9),
            (4, 1.0f32.to_bits()),
        ]));
        Ok(vec![
            SessionEvent::CreatureName {
                entry: c.entry,
                name: Some(plain_ui_text(&c.name)),
                subname: None,
                creature_type: None,
                pet_family: 0,
                pet_spell_list: 0, // Native/fixture profiles do not expose tamed-pet spells.
                rank: 0,
                type_flags: 0,
                display_id: c.display_id,
                civilian: false,
                racial_leader: false,
            },
            SessionEvent::ObjectCreate {
                guid,
                kind: EntityKind::Unit,
                display_id: Some(c.display_id),
                position: spawn.position_milliunits.map(|x| x as f32 / 1000.0),
                orientation: f32::from(spawn.orientation_milliradians) / 1000.0,
                scale: 1.0,
                speeds: None,
                mover: None,
                transport_progress: None,
                transport: None,
                spline: None,
                fields: fields.into_created(protocol::ObjectType::Unit),
            },
        ])
    }
    fn player_values(&self, world: &World) -> Result<SessionEvent, Error> {
        let p = world
            .player(self.adapter.selected)
            .ok_or(Error::UnknownPlayer)?;
        let rules = world.rules();
        let index = usize::from(p.level) - 1;
        let (xp, next) = if let Some(next) = rules.level_thresholds.get(index + 1) {
            (
                p.xp - rules.level_thresholds[index],
                next - rules.level_thresholds[index],
            )
        } else {
            (0, 0)
        };
        let mut pairs = vec![
            (22, p.health),
            (28, rules.player_health),
            (23, p.mana),
            (29, rules.player_mana),
            (34, u32::from(p.level)),
            (716, u32_value(xp)?),
            (717, u32_value(next)?),
            (1176, u32_value(p.money)?),
        ];
        pairs.extend(self.pack_pairs(world)?);
        let active: Vec<_> = p.quests.iter().filter(|(_, q)| !q.turned_in).collect();
        if active.len() > 20 || p.inventory.len() > 16 {
            return Err(Error::Unrepresentable);
        }
        for slot in 0..20 {
            let base = 198 + slot * 3;
            let (id, counters) = if let Some((id, q)) = active.get(slot as usize) {
                if q.counts.iter().any(|x| *x > 63) {
                    return Err(Error::Unrepresentable);
                }
                let mut counters = q
                    .counts
                    .iter()
                    .enumerate()
                    .fold(0, |v, (i, c)| v | (u32::from(*c) << (6 * i)));
                if q.counts
                    .iter()
                    .zip(&world.data().quest(**id).unwrap().objectives)
                    .all(|(c, o)| *c == o.count)
                {
                    counters |= 1 << 24;
                }
                (**id, counters)
            } else {
                (0, 0)
            };
            pairs.extend([(base, id), (base + 1, counters), (base + 2, 0)]);
        }
        Ok(SessionEvent::ObjectValues {
            guid: self.players[&p.id],
            fields: ObjectFields::from_pairs(&pairs),
        })
    }
    /// Requires the exact result of the immediately following tick. All validation and
    /// event construction completes before advancing this projection's boundary.
    pub fn tick(&mut self, world: &World) -> Result<Vec<SessionEvent>, Error> {
        self.adapter.check_world(world)?;
        if world.state().tick != self.previous.tick + 1
            || world.state().revision != self.previous.revision + 1
        {
            return Err(Error::StaleProjection);
        }
        let record = world.last_tick().ok_or(Error::StaleProjection)?;
        if record.before_digest != self.previous_digest
            || record.after_digest != world.state_digest()
        {
            return Err(Error::StaleProjection);
        }
        let result = &record.result;
        let next_npcs = self.npc_guids(world)?;
        let mut events = Vec::new();
        for old in &self.previous.npcs {
            if !next_npcs.contains_key(&old.id) {
                events.push(SessionEvent::ObjectDestroyed(self.npcs[&old.id]));
            }
        }
        for npc in &world.state().npcs {
            match self.previous.npcs.iter().find(|n| n.id == npc.id) {
                None => events.extend(self.npc_create(world, npc)?),
                Some(old) if old != npc => events.push(SessionEvent::ObjectValues {
                    guid: next_npcs[&npc.id],
                    fields: self.npc_fields(world, npc),
                }),
                _ => {}
            }
        }
        events.extend(self.item_events(world, false)?);
        events.push(self.player_values(world)?);
        let selected = self.adapter.selected;
        let before = self
            .previous
            .players
            .iter()
            .find(|p| p.id == selected)
            .unwrap();
        let after = world.player(selected).unwrap();
        for (quest, q) in &after.quests {
            for (i, count) in q.counts.iter().enumerate() {
                let old = before.quests.get(quest).map_or(0, |old| old.counts[i]);
                if *count > old {
                    let objective = &world.data().quest(*quest).unwrap().objectives[i];
                    events.push(SessionEvent::QuestObjectiveKill {
                        quest_id: *quest,
                        entry: objective.creature_entry,
                        count: u32::from(*count),
                        required: u32::from(objective.count),
                    });
                }
            }
        }
        for (quest, progress) in &after.quests {
            let definition = world.data().quest(*quest).unwrap();
            let complete = progress
                .counts
                .iter()
                .zip(&definition.objectives)
                .all(|(count, obj)| *count == obj.count);
            let was_complete = before.quests.get(quest).is_some_and(|old| {
                old.counts
                    .iter()
                    .zip(&definition.objectives)
                    .all(|(count, obj)| *count == obj.count)
            });
            if complete && !was_complete {
                events.push(SessionEvent::QuestObjectivesComplete { quest_id: *quest });
            }
        }
        for (input, outcome) in record.inputs.iter().zip(&result.outcomes) {
            if input.player != selected {
                continue;
            }
            if let runtime::Command::Cast { ability, .. } = input.command {
                events.push(SessionEvent::CastResult {
                    spell_id: ability,
                    success: outcome.rejection.is_none(),
                    reason: None,
                    arg: None,
                });
            }
            if outcome.rejection.is_none()
                && matches!(input.command, runtime::Command::AcceptQuest { .. })
            {
                events.push(SessionEvent::GossipComplete);
            }
            if let Some(reason) = outcome.rejection {
                events.push(SessionEvent::Notification {
                    text: format!("World action rejected: {reason:?}"),
                });
            }
        }
        for event in &result.events {
            match *event {
                runtime::Event::LootOpened { player, id } if player == selected => {
                    if after.active_loot == Some(id) {
                        events.push(self.loot_response(world, id)?);
                    }
                }
                runtime::Event::LootMoneyTaken { player, amount, .. } if player == selected => {
                    events.push(SessionEvent::LootMoneyNotify { amount });
                    events.push(SessionEvent::LootClearMoney);
                }
                runtime::Event::LootItemTaken { player, slot, .. } if player == selected => {
                    events.push(SessionEvent::LootRemoved { slot })
                }
                runtime::Event::LootReleased { player, id } if player == selected => {
                    let guid = *self
                        .npcs
                        .get(&id)
                        .or_else(|| next_npcs.get(&id))
                        .ok_or(Error::UnknownTarget)?;
                    events.push(SessionEvent::LootReleaseResponse { guid, result: 1 });
                }
                runtime::Event::NpcDied { id, owner, xp } if owner == selected => {
                    let guid = *self
                        .npcs
                        .get(&id)
                        .or_else(|| next_npcs.get(&id))
                        .ok_or(Error::UnknownTarget)?;
                    events.push(SessionEvent::XpGain(protocol::XpGain {
                        victim: guid,
                        total: u32_value(xp)?,
                        base: u32_value(xp)?,
                        kill: true,
                        group: 1.0, // Fixture solo award, no group multiplier.
                    }));
                }
                runtime::Event::QuestRewarded { player, quest } if player == selected => {
                    let q = world.data().quest(quest).ok_or(Error::UnknownTarget)?;
                    let xp = u64::from(q.reward_xp) * u64::from(world.rules().xp_rate_numerator)
                        / u64::from(world.rules().xp_rate_denominator);
                    events.push(SessionEvent::GossipComplete);
                    events.push(SessionEvent::QuestComplete(protocol::QuestComplete {
                        quest_id: quest,
                        xp: u32_value(xp)?,
                        money: q.reward_money,
                        items: q
                            .reward_items
                            .iter()
                            .map(|i| (i.item_entry, u32::from(i.count)))
                            .collect(),
                    }));
                }
                runtime::Event::CastStarted {
                    player,
                    ability,
                    target,
                    completes_at,
                } => {
                    let target = self.target_guid(target, player, &next_npcs)?;
                    events.push(SessionEvent::SpellStart {
                        caster: self.players[&player],
                        spell_id: ability,
                        cast_flags: 0,
                        cast_time_ms: u32_value(
                            (completes_at - world.state().tick) * u64::from(world.rules().tick_ms),
                        )?,
                        target: Some(target),
                        ammo_display_id: None,
                    });
                }
                runtime::Event::CastFinished {
                    player,
                    ability,
                    target,
                } => {
                    let target = self.target_guid(target, player, &next_npcs)?;
                    events.push(SessionEvent::SpellGo {
                        unit_caster: self.players[&player],
                        target_mask: 0x0002, // TARGET_FLAG_UNIT
                        source: None,
                        caster: self.players[&player],
                        spell_id: ability,
                        cast_flags: 0,
                        hits: vec![target],
                        misses: vec![],
                        target: Some(target),
                        go_target: None,
                        dest: None,
                        ammo_display_id: None,
                        item_caster: None,
                    });
                }
                runtime::Event::CastFizzled {
                    player, ability, ..
                } => events.push(SessionEvent::SpellFailedOther {
                    caster: self.players[&player],
                    spell_id: ability,
                }),
                _ => {}
            }
        }
        if before.quests != after.quests
            || before.level != after.level
            || (before.health == 0) != (after.health == 0)
            || self.previous.npcs != world.state().npcs
        {
            events.extend(self.quest_status_events(world)?);
        }
        self.previous = world.state().clone();
        self.previous_digest = record.after_digest;
        self.npcs = next_npcs;
        Ok(events)
    }
    fn target_guid(
        &self,
        target: runtime::Target,
        player: PlayerId,
        next: &BTreeMap<NpcId, u64>,
    ) -> Result<u64, Error> {
        match target {
            runtime::Target::SelfPlayer => self
                .players
                .get(&player)
                .copied()
                .ok_or(Error::UnknownPlayer),
            runtime::Target::Npc { id } => next
                .get(&id)
                .or_else(|| self.npcs.get(&id))
                .copied()
                .ok_or(Error::UnknownTarget),
        }
    }
}

pub mod native;
