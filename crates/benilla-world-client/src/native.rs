// SPDX-License-Identifier: GPL-3.0-or-later
//! Native-component protocol projection. The app controller must explicitly select this
//! adapter; it does not enable native admission or reinterpret OriginalFixture World v2.
use crate::{ClientCatalog, Error, NpcHandle, UiRequest};
use benilla_protocol::{messages as p, EntityKind, ObjectFields, SessionEvent};
use benilla_sim::{classic_spell as spell, creature_ai as ai};
use benilla_world_runtime::{native as n, Digest, NpcId, PlayerId};
use std::collections::{BTreeMap, BTreeSet};
#[path = "native_loot.rs"]
mod loot;
#[path = "native_motion.rs"]
mod motion;
#[path = "native_player_slow.rs"]
mod player_slow;
#[path = "native_quests.rs"]
mod quests;

/// Exclusive local session order-counter state. The controller must share it with all
/// forced movement/speed orders; it is not a world tick or a peer-supplied result.
pub struct NativeMovementOrders {
    next: u32,
}
impl NativeMovementOrders {
    pub fn from_session_next_counter(next: u32) -> Self {
        Self { next }
    }
    pub fn next_counter(&self) -> u32 {
        self.next
    }
}
fn movement_counter(next: &mut Option<u32>) -> Result<u32, Error> {
    let counter = next.ok_or(Error::MissingMovementOrderState)?;
    *next = Some(counter.checked_add(1).ok_or(Error::Unrepresentable)?);
    Ok(counter)
}
fn root_order(guid: u64, next: &mut Option<u32>) -> Result<SessionEvent, Error> {
    let counter = movement_counter(next)?;
    Ok(SessionEvent::MoveMode {
        guid,
        counter,
        mode: p::MoveMode::Root,
        apply: true,
    })
}
pub struct NativeProjection {
    player: PlayerId,
    player_guid: u64,
    profile: Digest,
    world: benilla_world_runtime::WorldId,
    previous: n::State,
    previous_digest: Digest,
    npcs: BTreeMap<NpcId, u64>,
    quest_metadata: quests::Metadata,
    loot_metadata: loot::Metadata,
}
fn point(p: ai::Point) -> [f32; 3] {
    [p.x.as_f32(), p.y.as_f32(), p.z.as_f32()]
}
fn equipment_guid(w: &n::NativeWorld, slot: u8, item: u32) -> u64 {
    let hash = crate::hash(
        b"benilla-native-equipment-guid-v2\0",
        &(w.state().world, w.state().player, slot, item),
    );
    let mut bytes = [0; 8];
    bytes[..6].copy_from_slice(&hash[..6]);
    0x4000_0000_0000_0000 | u64::from_le_bytes(bytes).max(1)
}
fn npc_guid(w: &n::NativeWorld, id: NpcId) -> Result<u64, Error> {
    let p = w.profile().npc(id.spawn).ok_or(Error::UnknownTarget)?;
    let digest = crate::hash(b"benilla-native-npc-guid-v1\0", &(w.state().world, id));
    let low = u32::from_le_bytes([digest[0], digest[1], digest[2], 0]).max(1);
    Ok(0xf130_0000_0000_0000 | (u64::from(p.entry) << 24) | u64::from(low))
}
impl NativeProjection {
    pub fn new(
        w: &n::NativeWorld,
        selected: PlayerId,
        catalog: &impl ClientCatalog,
    ) -> Result<Self, Error> {
        if w.state().player != selected {
            return Err(Error::UnknownPlayer);
        }
        if catalog.build() != 5875 || catalog.manifest_digest() == [0; 32] {
            return Err(Error::InvalidClientManifest);
        }
        if catalog.player_power_type(selected) != Some(0) {
            return Err(Error::UnsupportedPower);
        }
        if !catalog.contains_map(w.profile().map_id) {
            return Err(Error::UnboundMap(w.profile().map_id));
        }
        for npc in &w.profile().npcs {
            if !catalog.contains_creature_display(npc.display_id) {
                return Err(Error::UnboundCreatureDisplay(npc.entry));
            }
        }
        for a in w.abilities() {
            if !catalog.contains_spell(a.id) {
                return Err(Error::UnboundSpell(a.id));
            }
        }
        // Incoming aura renderability is independent of the player's learned spellbook.
        if let Some(slow) = &w.profile().player_slow {
            for id in slow.spell_ids() {
                if !catalog.contains_spell(id) {
                    return Err(Error::UnboundSpell(id));
                }
            }
        }
        let mut out = Self {
            player: selected,
            player_guid: crate::player_guid(selected),
            profile: w.profile_digest(),
            world: w.state().world,
            previous: w.state().clone(),
            previous_digest: w.state_digest(),
            npcs: BTreeMap::new(),
            quest_metadata: quests::Metadata::load(w, catalog)?,
            loot_metadata: loot::Metadata::load(w, catalog)?,
        };
        out.npcs = out.guids(w)?;
        Ok(out)
    }
    fn check(&self, w: &n::NativeWorld) -> Result<(), Error> {
        if self.player != w.state().player
            || self.profile != w.profile_digest()
            || self.world != w.state().world
        {
            return Err(Error::Incompatible);
        }
        Ok(())
    }
    fn guids(&self, w: &n::NativeWorld) -> Result<BTreeMap<NpcId, u64>, Error> {
        let mut seen = BTreeSet::from([self.player_guid]);
        let mut npcs = BTreeMap::new();
        for guid in loot::guids(w).chain(
            w.state()
                .combat
                .iter()
                .flat_map(|c| c.equipment.items())
                .map(|i| equipment_guid(w, i.slot, i.item_id)),
        ) {
            if !seen.insert(guid) {
                return Err(Error::GuidCollision);
            }
        }
        for npc in w
            .state()
            .npcs
            .values()
            .filter(|n| n.ai.phase() != ai::Phase::Despawned)
        {
            let guid = npc_guid(w, npc.id)?;
            if !seen.insert(guid) {
                return Err(Error::GuidCollision);
            }
            npcs.insert(npc.id, guid);
        }
        Ok(npcs)
    }
    pub fn player_guid(&self) -> u64 {
        self.player_guid
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
    /// Preserves stock exact spell IDs, including unavailable learned actives. The runtime
    /// produces the specific unavailable reason; no synthetic spell-ID aliases are introduced.
    pub fn local(&self, w: &n::NativeWorld, request: UiRequest) -> Result<n::OwnedInput, Error> {
        self.check(w)?;
        let command = match request {
            UiRequest::CastAtNpc { ability, target } => n::Command::Cast {
                ability,
                target: target.id,
            },
            UiRequest::AcceptQuest { quest, giver } => n::Command::AcceptQuest {
                quest,
                giver: giver.id,
            },
            UiRequest::TurnInQuest { quest, giver } => n::Command::TurnInQuest {
                quest,
                giver: giver.id,
            },
            UiRequest::ChooseQuestReward {
                quest,
                giver,
                choice,
            } => n::Command::ChooseQuestReward {
                quest,
                giver: giver.id,
                choice,
            },
            UiRequest::OpenLoot(target) => n::Command::OpenLoot { corpse: target.id },
            UiRequest::TakeLootMoney(target) => n::Command::LootMoney { corpse: target.id },
            UiRequest::TakeLootItem { target, slot } => n::Command::LootItem {
                corpse: target.id,
                slot,
            },
            UiRequest::ReleaseLoot(target) => n::Command::ReleaseLoot { corpse: target.id },
            _ => return Err(Error::UnsupportedCommand),
        };
        Ok(self.input(w, command))
    }
    /// Body release is visible but explicitly unavailable until ghost/corpse/graveyard support.
    pub fn release_spirit(&self, w: &n::NativeWorld) -> Result<n::OwnedInput, Error> {
        self.check(w)?;
        Ok(self.input(w, n::Command::ReleaseSpirit))
    }
    pub fn cancel_cast(&self, w: &n::NativeWorld) -> Result<n::OwnedInput, Error> {
        self.check(w)?;
        let target = w
            .state()
            .casts
            .values()
            .find(|cast| matches!(cast.execution.state(), spell::CastState::Casting { .. }))
            .map(|cast| cast.id)
            .unwrap_or(0);
        Ok(self.input(
            w,
            n::Command::CancelCast {
                spell_id: 0,
                target: n::CancelCastTarget::Instance(target),
            },
        ))
    }
    /// Destination still goes through the runtime's bound trusted navigation callback.
    pub fn move_to(
        &self,
        w: &n::NativeWorld,
        destination: ai::Point,
    ) -> Result<n::OwnedInput, Error> {
        self.check(w)?;
        if w.profile().source_motion.is_some() {
            return Err(Error::UnsupportedCommand);
        }
        Ok(self.input(w, n::Command::Move { destination }))
    }
    /// A sequenced pose shares the runtime's atomic movement/turn validation.
    pub fn pose(
        &self,
        w: &n::NativeWorld,
        destination: ai::Point,
        orientation: n::Orientation,
    ) -> Result<n::OwnedInput, Error> {
        self.check(w)?;
        if w.profile().source_motion.is_some() {
            return Err(Error::UnsupportedCommand);
        }
        Ok(self.input(
            w,
            n::Command::Pose {
                destination,
                orientation,
            },
        ))
    }
    /// Source motion requires the actual sequenced ground flags; position deltas
    /// cannot distinguish walking, running, stationary turning, or a stop.
    pub fn movement_pose(
        &self,
        w: &n::NativeWorld,
        destination: ai::Point,
        orientation: n::Orientation,
        movement_flags: benilla_sim::creature_motion::GroundMovementFlags,
    ) -> Result<n::OwnedInput, Error> {
        self.check(w)?;
        if w.profile().source_motion.is_none() {
            return Err(Error::UnsupportedCommand);
        }
        Ok(self.input(
            w,
            n::Command::MovementPose {
                destination,
                orientation,
                movement_flags,
            },
        ))
    }
    fn input(&self, w: &n::NativeWorld, command: n::Command) -> n::OwnedInput {
        n::OwnedInput {
            authenticated_owner: self.player.owner,
            input: n::Input {
                world: w.state().world,
                tick: w.state().tick + 1,
                player: self.player,
                sequence: w.state().next_sequence,
                command,
            },
        }
    }
    fn player_values(&self, w: &n::NativeWorld) -> SessionEvent {
        let mut pairs = vec![
            (22, w.state().health),
            (28, w.player_limits().0),
            (23, w.state().resources.caster.mana),
            (29, w.player_limits().1),
            (34, u32::from(w.player_level())),
        ];
        if let Some(c) = &w.state().combat {
            // Loot::SetPlayerIsLooting/NotLooting maintains UNIT_FLAG_LOOTING.
            let looting = w
                .state()
                .quest_progression
                .as_ref()
                .is_some_and(|q| q.loot_target.is_some());
            pairs.extend([
                (
                    46,
                    8 | if c.in_combat { 0x80000 } else { 0 } | if looting { 0x400 } else { 0 },
                ),
                (125, u32::from(c.defense_reactive_ms > 0)),
                (138, u32::from(!c.standing)),
            ]);
            for (i, value) in c.stats.attributes.iter().enumerate() {
                pairs.push((150 + i as u16, *value as u32));
            }
            for (i, value) in c.stats.resistances.iter().enumerate() {
                pairs.push((155 + i as u16, *value as u32));
            }
            for (slot, skill) in c
                .skills
                .values()
                .enumerate()
                .filter(|_| w.state().quest_progression.is_none())
            {
                let base = 718 + 3 * slot as u16;
                pairs.extend([
                    (base, u32::from(skill.id) | (u32::from(skill.step) << 16)),
                    (
                        base + 1,
                        u32::from(skill.value) | (u32::from(skill.maximum) << 16),
                    ),
                    (
                        base + 2,
                        u32::from(skill.temporary_bonus as u16)
                            | (u32::from(skill.permanent_bonus as u16) << 16),
                    ),
                ]);
            }
            for slot in 0..19u8 {
                let guid = c
                    .equipment
                    .item(slot)
                    .map(|i| equipment_guid(w, slot, i.item_id))
                    .unwrap_or(0);
                pairs.extend([
                    (486 + 2 * u16::from(slot), guid as u32),
                    (487 + 2 * u16::from(slot), (guid >> 32) as u32),
                ]);
            }
        }
        if let Some(d) = w.state().combat.as_ref().and_then(|c| c.death.as_ref()) {
            let state = d.snapshot();
            if state.phase == benilla_sim::player_death::DeathPhase::Corpse {
                pairs.extend([
                    (143, 0),
                    (1222, if state.config.map_instanceable { 0 } else { 8 }),
                ]);
            }
        }
        pairs.extend(quests::player_pairs(w, &self.quest_metadata));
        pairs.extend(loot::player_pairs(w));
        pairs.extend(player_slow::player_pairs(w));
        SessionEvent::ObjectValues {
            guid: self.player_guid,
            fields: ObjectFields::from_pairs(&pairs),
        }
    }
    fn equipment_events(
        &self,
        w: &n::NativeWorld,
        initial: bool,
    ) -> Result<Vec<SessionEvent>, Error> {
        let mut events = Vec::new();
        let mut seen = BTreeSet::new();
        if let Some(c) = &w.state().combat {
            for item in c.equipment.items() {
                let guid = equipment_guid(w, item.slot, item.item_id);
                if !seen.insert(guid) {
                    return Err(Error::GuidCollision);
                }
                if initial {
                    events.push(SessionEvent::ItemCreate {
                        guid,
                        container: false,
                        fields: ObjectFields::from_pairs(&[
                            (0, guid as u32),
                            (1, (guid >> 32) as u32),
                            (2, 3),
                            (3, item.item_id),
                            (4, 1.0f32.to_bits()),
                            (6, self.player_guid as u32),
                            (7, (self.player_guid >> 32) as u32),
                            (8, self.player_guid as u32),
                            (9, (self.player_guid >> 32) as u32),
                            (14, 1),
                            (
                                21,
                                self.loot_metadata
                                    .item(item.item_id)
                                    .map_or(0, |p| u32::from(matches!(p.bonding, 1 | 2 | 4))),
                            ),
                            (46, item.current_durability),
                            (47, item.max_durability),
                        ])
                        .into_created(p::ObjectType::Item),
                    });
                } else if self
                    .previous
                    .combat
                    .as_ref()
                    .and_then(|c| c.equipment.item(item.slot))
                    != Some(item)
                {
                    events.push(SessionEvent::ObjectValues {
                        guid,
                        fields: ObjectFields::from_pairs(&[
                            (46, item.current_durability),
                            (47, item.max_durability),
                        ]),
                    });
                }
            }
        }
        Ok(events)
    }
    fn npc_fields(&self, w: &n::NativeWorld, npc: &n::Npc) -> Result<ObjectFields, Error> {
        let profile = w.profile().npc(npc.id.spawn).ok_or(Error::UnknownTarget)?;
        if npc.auras.len() > 16 {
            return Err(Error::Unrepresentable);
        }
        let target = match npc.ai.target() {
            None => 0,
            Some(n::SOLO_UNIT) => self.player_guid,
            Some(_) => return Err(Error::UnknownTarget),
        };
        let mut pairs = vec![
            (16, target as u32),
            (17, (target >> 32) as u32),
            (3, profile.entry),
            (22, npc.health),
            (28, profile.max_health),
            (34, u32::from(profile.ai.level)),
            (35, profile.faction_template),
            (129, profile.ai.bounding_radius.bits()),
            (130, profile.ai.combat_reach.bits()),
            (131, profile.display_id),
            (132, profile.display_id),
            (
                143,
                if npc.original_loot_recipient == Some(self.player) {
                    0x4 | 0x8
                } else {
                    0
                } | u32::from(
                    npc.corpse_loot
                        .as_ref()
                        .is_some_and(|l| l.owner == self.player && l.lootable()),
                ),
            ),
        ];
        if let Some(flags) = w
            .profile()
            .quest_progression
            .as_ref()
            .and_then(|g| g.npc_flags(profile.entry))
        {
            // Creature::SetDeathState clears NPC flags on death and restores
            // the template on revival.
            pairs.push((147, if npc.health == 0 { 0 } else { flags }));
        }
        // All 16 harmful slots are materialized so expired/replaced auras clear in stock UI.
        let auras = npc.auras.iter().collect::<Vec<_>>();
        let mut flags = [0u32; 2];
        let mut levels = [0u32; 4];
        for offset in 0..16 {
            let slot = 32 + offset;
            let id = if let Some((id, target)) = auras.get(offset) {
                let aura = target.aura().ok_or(Error::Unrepresentable)?;
                flags[offset / 8] |= (1u32 << aura.effect_slot) << ((offset % 8) * 4);
                let binding = npc.aura_bindings.get(id).ok_or(Error::Unrepresentable)?;
                levels[offset / 4] |= u32::from(binding.level) << ((offset % 4) * 8);
                **id
            } else {
                0
            };
            pairs.push((47 + slot as u16, id));
        }
        pairs.extend([(99, flags[0]), (100, flags[1])]);
        for (i, level) in levels.into_iter().enumerate() {
            pairs.push((109 + i as u16, level));
            pairs.push((121 + i as u16, 0));
        }
        Ok(ObjectFields::from_pairs(&pairs))
    }
    fn create(&self, w: &n::NativeWorld, npc: &n::Npc) -> Result<Vec<SessionEvent>, Error> {
        let profile = w.profile().npc(npc.id.spawn).ok_or(Error::UnknownTarget)?;
        let guid = npc_guid(w, npc.id)?;
        let mut fields = self.npc_fields(w, npc)?;
        fields.merge(ObjectFields::from_pairs(&[
            (0, guid as u32),
            (1, (guid >> 32) as u32),
            (2, 9),
            (4, 1f32.to_bits()),
        ]));
        Ok(vec![
            SessionEvent::CreatureName {
                entry: profile.entry,
                name: Some(crate::plain_ui_text(&profile.name)),
                subname: None,
                creature_type: None,
                pet_family: 0,
                rank: 0,
                type_flags: 0,
                display_id: profile.display_id,
                civilian: false,
                racial_leader: false,
            },
            SessionEvent::ObjectCreate {
                guid,
                kind: EntityKind::Unit,
                display_id: Some(profile.display_id),
                position: point(
                    npc.source_motion
                        .as_ref()
                        .map_or(npc.ai.position(), |source| source.motion.pose().position),
                ),
                orientation: npc
                    .source_motion
                    .as_ref()
                    .map_or(0.0, |source| source.motion.pose().orientation.as_f32()),
                scale: 1.0,
                speeds: None,
                mover: None,
                transport_progress: None,
                transport: None,
                spline: motion::create_spline(npc)?,
                fields: fields.into_created(p::ObjectType::Unit),
            },
        ])
    }
    pub fn initial_events(&self, w: &n::NativeWorld) -> Result<Vec<SessionEvent>, Error> {
        self.initial_events_inner(w, &mut None)
    }
    pub fn initial_events_with_movement_orders(
        &self,
        w: &n::NativeWorld,
        orders: &mut NativeMovementOrders,
    ) -> Result<Vec<SessionEvent>, Error> {
        let mut next = Some(orders.next);
        let events = self.initial_events_inner(w, &mut next)?;
        orders.next = next.unwrap();
        Ok(events)
    }
    fn initial_events_inner(
        &self,
        w: &n::NativeWorld,
        orders: &mut Option<u32>,
    ) -> Result<Vec<SessionEvent>, Error> {
        self.check(w)?;
        if self.previous_digest != w.state_digest() {
            return Err(Error::StaleProjection);
        }
        let mut events = vec![];
        for npc in w
            .state()
            .npcs
            .values()
            .filter(|n| n.ai.phase() != ai::Phase::Despawned)
        {
            events.extend(self.create(w, npc)?);
        }
        events.extend(self.equipment_events(w, true)?);
        events.extend(self.initial_inventory(w));
        events.extend(self.initial_reputations(w)?);
        events.push(self.player_values(w));
        events.extend(player_slow::initial(w, self.player_guid, orders)?);
        events.push(SessionEvent::ObjectMove {
            guid: self.player_guid,
            position: point(w.state().position),
            orientation: w.state().orientation.as_f32(),
        });
        events.extend(self.initial_loot_window(w)?);
        if w.state()
            .combat
            .as_ref()
            .and_then(|c| c.death.as_ref())
            .is_some_and(|d| d.phase() == benilla_sim::player_death::DeathPhase::Corpse)
        {
            events.push(root_order(self.player_guid, orders)?);
        }
        events.push(SessionEvent::SpellBook {
            spell_ids: quests::spell_ids(w),
            cooldowns: vec![],
        });
        for c in w.state().casts.values() {
            if let spell::CastState::Casting { remaining_ms } = c.execution.state() {
                events.push(SessionEvent::SpellStart {
                    caster: self.player_guid,
                    spell_id: c.ability,
                    cast_flags: 0,
                    cast_time_ms: remaining_ms,
                    target: self.npcs.get(&c.target).copied(),
                    ammo_display_id: None,
                });
            }
        }
        events.extend(self.cooldowns(w));
        Ok(events)
    }
    fn cooldowns(&self, w: &n::NativeWorld) -> Vec<SessionEvent> {
        let cooldowns = w
            .abilities()
            .iter()
            .filter_map(|a| {
                w.state()
                    .resources
                    .caster
                    .spell_ready_at(a.id)
                    .and_then(|t| t.checked_sub(w.state().now_ms))
                    .filter(|t| *t > 0)
                    .map(|t| (a.id, t as u32))
            })
            .collect::<Vec<_>>();
        if cooldowns.is_empty() {
            vec![]
        } else {
            vec![SessionEvent::SpellCooldowns {
                caster: self.player_guid,
                cooldowns,
            }]
        }
    }
    pub fn advance(&mut self, w: &n::NativeWorld) -> Result<Vec<SessionEvent>, Error> {
        self.advance_inner(w, &mut None)
    }
    pub fn advance_with_movement_orders(
        &mut self,
        w: &n::NativeWorld,
        orders: &mut NativeMovementOrders,
    ) -> Result<Vec<SessionEvent>, Error> {
        let mut next = Some(orders.next);
        let events = self.advance_inner(w, &mut next)?;
        orders.next = next.unwrap();
        Ok(events)
    }
    fn advance_inner(
        &mut self,
        w: &n::NativeWorld,
        orders: &mut Option<u32>,
    ) -> Result<Vec<SessionEvent>, Error> {
        self.check(w)?;
        let record = w.last_tick().ok_or(Error::StaleProjection)?;
        if w.state().tick != self.previous.tick + 1
            || record.before != self.previous_digest
            || record.after != w.state_digest()
        {
            return Err(Error::StaleProjection);
        }
        let next = self.guids(w)?;
        let mut source_frame = if w.profile().source_motion.is_some() {
            Some(motion::Frame::new(&self.npcs, &self.previous, w)?)
        } else {
            None
        };
        let mut events = vec![];
        let mut source_values = vec![];
        if source_frame.is_none() {
            for (id, guid) in &self.npcs {
                if !next.contains_key(id) {
                    events.push(SessionEvent::ObjectDestroyed(*guid));
                }
            }
        }
        for npc in w
            .state()
            .npcs
            .values()
            .filter(|n| n.ai.phase() != ai::Phase::Despawned)
        {
            if let Some(old) = self
                .previous
                .npcs
                .get(&npc.id.spawn)
                .filter(|n| n.id == npc.id && self.npcs.contains_key(&n.id))
            {
                if npc.source_motion.is_none() && old.ai.position() != npc.ai.position() {
                    events.push(SessionEvent::MonsterMove {
                        guid: next[&npc.id],
                        transport: None,
                        start: point(old.ai.position()),
                        spline_id: w.state().tick as u32,
                        path: vec![point(old.ai.position()), point(npc.ai.position())],
                        facing: p::MonsterMoveFacing::None,
                        stop: false,
                        duration_ms: w.profile().tick_ms,
                        flying: false,
                        run_mode: true,
                    });
                }
                if old != npc {
                    let values = SessionEvent::ObjectValues {
                        guid: next[&npc.id],
                        fields: self.npc_fields(w, npc)?,
                    };
                    if source_frame.is_some() {
                        // Map::Update sends accumulated values after creature
                        // callbacks. Only final represented survivors enter here.
                        source_values.push(values);
                    } else {
                        events.push(values);
                    }
                }
            } else if source_frame.is_none() {
                events.extend(self.create(w, npc)?);
            }
        }
        events.extend(self.equipment_events(w, false)?);
        let mut quest_frame = quests::Frame::new(&self.previous);
        let mut loot_frame = loot::Frame::new(&self.previous);
        let mut slow_frame = player_slow::Frame::new(&self.previous);
        // AttackStop(false) already cleared melee-attacking state during death cleanup.
        // The visible target may remain until ordinary evade, which must not emit a
        // second stop in this frame or the next one.
        let mut attack_stopped = self
            .previous
            .npcs
            .values()
            .filter(|npc| npc.ai.victim_detached_at_ms().is_some())
            .map(|npc| npc.id)
            .collect::<BTreeSet<_>>();
        let mut accepted_quest_pending_close = false;
        let mut reward_refusal_presented = false;
        for event in &record.events {
            // HandleQuestgiverAcceptQuest closes gossip after AddQuest and
            // CompleteQuest callbacks, before the next input or world update.
            if accepted_quest_pending_close && !matches!(event, n::Event::QuestProgression(_)) {
                events.push(SessionEvent::GossipComplete);
                accepted_quest_pending_close = false;
            }
            let target_guid = |id: &NpcId| {
                if let Some(frame) = &source_frame {
                    return frame.guid(id);
                }
                next.get(id)
                    .or_else(|| self.npcs.get(id))
                    .copied()
                    .ok_or(Error::UnknownTarget)
            };
            match event {
                n::Event::InputResult { sequence, accepted } => {
                    reward_refusal_presented = false;
                    let input = record.inputs.iter().find(|input| input.sequence == *sequence)
                        .ok_or(Error::StaleProjection)?;
                    accepted_quest_pending_close = *accepted && matches!(input.command, n::Command::AcceptQuest { .. });
                    if let n::Command::Cast { ability, .. } = input.command {
                        events.push(SessionEvent::CastResult {
                            spell_id: ability,
                            success: *accepted,
                            reason: None,
                            arg: None,
                        });
                    }
                }
                n::Event::QuestProgression(event) => {
                    reward_refusal_presented |= matches!(event, n::QuestProgressionEvent::RewardRefused { .. });
                    events.extend(self.quest_progression_event(w, event, &mut quest_frame, &mut loot_frame)?);
                },
                n::Event::Loot(event) => events.extend(self.loot_event(w, event, &mut loot_frame)?),
                n::Event::InventoryStored(stored) => events.extend(self.inventory_stored(w, stored, &mut loot_frame)?),
                n::Event::InventoryMoved(moved) => events.extend(self.inventory_moved(w, moved, &mut loot_frame)?),
                n::Event::ItemAcquired(source) => events.push(self.item_acquired(*source, &mut loot_frame)?),
                n::Event::NpcSpell(_) => {}, // Internal Unit lifetime, not a client cast bar.
                n::Event::PlayerSlow(event) => events.extend(slow_frame.project(event, self.player_guid, orders)?),
                n::Event::Death(benilla_sim::player_death::DeathAction::RootMovement) => events.push(root_order(self.player_guid, orders)?),
                n::Event::Death(benilla_sim::player_death::DeathAction::SendDurabilityDamageDeath) => events.push(SessionEvent::DurabilityDamageDeath),
                n::Event::Death(benilla_sim::player_death::DeathAction::ReleaseUnavailable { .. }) => events.push(SessionEvent::Notification { text: "Spirit release is unavailable: corpse, ghost and graveyard traversal are not implemented".into() }),
                n::Event::Combat(n::CombatEvent::CriticalWound) => {
                    events.push(SessionEvent::Emote {
                        guid: self.player_guid,
                        emote_id: 34,
                    })
                }
                n::Event::Rejected(n::Rejection::Loot { corpse, reason }) => events.push(SessionEvent::LootError { guid: npc_guid(w, *corpse)?, error: *reason }),
                // The movement receipt carries the callback-time item identities and error.
                n::Event::Rejected(n::Rejection::Inventory { .. }) => {},
                // RewardRefused already carries the source error and callback-time offer.
                n::Event::Rejected(n::Rejection::Quest(_)) if reward_refusal_presented => {
                    reward_refusal_presented = false;
                },
                n::Event::Rejected(r) => events.push(SessionEvent::Notification {
                    text: format!("Native action rejected: {r:?}"),
                }),
                n::Event::PlayerPose { position, orientation } => events.push(SessionEvent::ObjectMove {
                    guid: self.player_guid,
                    position: point(*position),
                    orientation: orientation.as_f32(),
                }),
                n::Event::Spell {
                    cast_id,
                    ability,
                    target,
                    event,
                } => {
                    let guid = target_guid(target).ok();
                    let school = w.ability_school(*ability).ok_or(Error::UnboundSpell(*ability))?;
                    match event {
                        spell::SpellEvent::Started { cast_ms } => {
                            events.push(SessionEvent::SpellStart {
                                caster: self.player_guid,
                                spell_id: *ability,
                                cast_flags: 0,
                                cast_time_ms: *cast_ms,
                                target: guid,
                                ammo_display_id: None,
                            })
                        }
                        spell::SpellEvent::Launched { .. } => {
                            let guid = guid.ok_or(Error::UnknownTarget)?;
                            let hit = record
                                .draws
                                .iter()
                                .find(|d| d.cast_id == *cast_id && d.purpose == n::DrawPurpose::Hit)
                                .ok_or(Error::StaleProjection)?
                                .value;
                            let chance = record
                                .replies
                                .iter()
                                .find_map(|r| match (&r.request.query, &r.facts) {
                                    (
                                        n::Query::Spell {
                                            cast_id: id,
                                            ability: spell,
                                            phase: n::CastPhase::Release,
                                            ..
                                        },
                                        n::Facts::Spell(f),
                                    ) if !w.profile().has_source_spell_authority()
                                        && id == cast_id
                                        && spell == ability
                                        && r.request.binding.npc == Some(*target) =>
                                    {
                                        Some(f.resist_chance)
                                    }
                                    (
                                        n::Query::SourceSpell(n::SourceSpellQuery::Semantics(context)),
                                        n::Facts::SourceSpell(n::SourceSpellFacts::Semantics(f)),
                                    ) if w.profile().has_source_spell_authority()
                                        && context.phase == n::SourceSpellPhase::ReleaseHit
                                        && context.cast_id == *cast_id
                                        && context.ability == *ability
                                        && r.request.binding.npc == Some(*target) => Some(f.resist_chance),
                                    _ => None,
                                })
                                .ok_or(Error::StaleProjection)?;
                            let resisted = hit <= i64::from(chance);
                            // SharedDefines.h SPELL_MISS_RESIST=2 in the pinned 1.12 core.
                            events.push(SessionEvent::SpellGo {
                                caster: self.player_guid,
                                spell_id: *ability,
                                cast_flags: 0,
                                hits: if resisted { vec![] } else { vec![guid] },
                                misses: if resisted { vec![(guid, 2)] } else { vec![] },
                                target: Some(guid),
                                go_target: None,
                                dest: None,
                                ammo_display_id: None,
                                item_caster: None,
                            });
                        }
                        spell::SpellEvent::Failed(_) => {
                            events.push(SessionEvent::SpellFailedOther {
                                caster: self.player_guid,
                                spell_id: *ability,
                            })
                        }
                        spell::SpellEvent::Pushback(delay) => {
                            events.push(SessionEvent::SpellDelayed {
                                caster: self.player_guid,
                                delay_ms: *delay,
                            })
                        }
                        spell::SpellEvent::DirectDamage {
                            amount, critical, ..
                        } => events.push(SessionEvent::SpellDamageLog(p::SpellDamageLog {
                            target: guid.ok_or(Error::UnknownTarget)?,
                            attacker: self.player_guid,
                            spell_id: *ability,
                            damage: *amount,
                            school,
                            absorb: 0,
                            resist: 0,
                            periodic: false,
                            blocked: 0,
                            hit_info: if *critical { 2 } else { 0 },
                        })),
                        spell::SpellEvent::PeriodicDamage { amount, absorb, resist, .. } => {
                            events.push(SessionEvent::PeriodicAuraLog(p::PeriodicAuraLog {
                                target: guid.ok_or(Error::UnknownTarget)?,
                                caster: self.player_guid,
                                spell_id: *ability,
                                ticks: vec![p::PeriodicTick::Damage {
                                    amount: *amount,
                                    school: u32::from(school),
                                    absorb: *absorb,
                                    resist: *resist,
                                }],
                            }))
                        }
                        spell::SpellEvent::CooldownStarted => events.extend(self.cooldowns(w)),
                        spell::SpellEvent::Resisted | spell::SpellEvent::Immune => {
                            events.push(SessionEvent::Notification {
                                text: format!("Spell {ability}: {event:?}"),
                            })
                        }
                        _ => {}
                    }
                }
                n::Event::NpcMotion { id, event } => {
                    let frame = source_frame.as_ref().ok_or(Error::StaleProjection)?;
                    events.extend(motion::project(*id, event, self.player_guid, frame.represented())?);
                }
                n::Event::Npc { id, effect } => match effect {
                    ai::Effect::CorpseRemoved if source_frame.is_some() => {
                        events.push(source_frame.as_mut().unwrap().remove(*id)?);
                    }
                    ai::Effect::Respawned { .. } if source_frame.is_some() => {
                        let npc = w.state().npcs.get(&id.spawn)
                            .filter(|npc| npc.id == *id && npc.ai.phase() != ai::Phase::Despawned)
                            .ok_or(Error::StaleProjection)?;
                        let guid = *next.get(id).ok_or(Error::StaleProjection)?;
                        source_frame.as_mut().unwrap().spawn(*id, guid)?;
                        events.extend(self.create(w, npc)?);
                    }
                    ai::Effect::CombatStarted { .. } => {
                        attack_stopped.remove(id);
                        let guid = target_guid(id)?;
                        events.push(SessionEvent::AttackStart {
                            attacker: guid,
                            victim: self.player_guid,
                        });
                        events.push(SessionEvent::AiReaction {
                            unit: guid,
                            reaction: 2,
                        });
                    }
                    ai::Effect::AttackStopped { .. } | ai::Effect::EvadeStarted(_) | ai::Effect::Died
                        if attack_stopped.insert(*id) => {
                        events.push(SessionEvent::AttackStop {
                            attacker: target_guid(id)?,
                            victim: self.player_guid,
                        });
                    }
                    _ => {}
                },
                n::Event::Melee { id, outcome, .. } => {
                    events.push(SessionEvent::AttackerState(p::AttackerState {
                        attacker: target_guid(id)?,
                        victim: self.player_guid,
                        hit_info: outcome.hit_info,
                        damage: outcome.damage,
                        victim_state: outcome.victim_state,
                        absorb: outcome.absorb,
                        resist: outcome.resist,
                        blocked: outcome.blocked,
                        melee_spell_id: 0,
                        school: outcome.school,
                    }))
                }
                _ => {}
            }
        }
        if accepted_quest_pending_close {
            events.push(SessionEvent::GossipComplete);
        }
        if let Some(frame) = &source_frame {
            frame.finish(&next)?;
        }
        quest_frame.finish(w)?;
        loot_frame.validate_final(w)?;
        slow_frame.finish(w)?;
        events.extend(loot_frame.finish(self, w));
        events.extend(source_values);
        events.push(self.player_values(w));
        self.previous = w.state().clone();
        self.previous_digest = record.after;
        self.npcs = next;
        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use n::synthetic_fixture::{world, SyntheticAuthority};
    pub(super) struct Catalog;
    impl ClientCatalog for Catalog {
        fn build(&self) -> u32 {
            5875
        }
        fn manifest_digest(&self) -> Digest {
            [9; 32]
        }
        fn contains_map(&self, id: u32) -> bool {
            id == 7
        }
        fn contains_creature_display(&self, id: u32) -> bool {
            id == 60001
        }
        fn contains_item_display(&self, _: u32) -> bool {
            false
        }
        fn contains_spell(&self, id: u32) -> bool {
            matches!(id, 133 | 60001 | 60002)
        }
        fn player_power_type(&self, _: PlayerId) -> Option<u8> {
            Some(0)
        }
        fn item_metadata(&self, _: u32) -> Option<p::ItemInfo> {
            None
        }
        fn quest_metadata(&self, _: u32) -> Option<crate::QuestMetadata> {
            None
        }
        fn quest_greeting(&self, _: u32) -> Option<String> {
            None
        }
    }
    #[test]
    fn native_projection_reaches_stock_cast_projectile_damage_auras_and_moving_objects() {
        let mut w = world();
        let mut projection = NativeProjection::new(&w, w.state().player, &Catalog).unwrap();
        let initial = projection.initial_events(&w).unwrap();
        assert!(initial.iter().any(
            |e| matches!(e,SessionEvent::SpellBook{spell_ids,..} if spell_ids==&vec![60001,60002])
        ));
        let handle = projection
            .capture_npc(projection.npc_guid(w.state().npcs[&2].id).unwrap())
            .unwrap();
        let input = projection
            .local(
                &w,
                UiRequest::CastAtNpc {
                    ability: 60001,
                    target: handle,
                },
            )
            .unwrap();
        let mut authority = SyntheticAuthority::default();
        w.step(1, Some(input), &mut authority).unwrap();
        let mut all = projection.advance(&w).unwrap();
        for tick in 2..=65 {
            w.step(tick, None, &mut authority).unwrap();
            all.extend(projection.advance(&w).unwrap());
        }
        assert!(all.iter().any(|e| matches!(
            e,
            SessionEvent::SpellStart {
                spell_id: 60001,
                ..
            }
        )));
        assert!(all.iter().any(|e| matches!(
            e,
            SessionEvent::SpellGo {
                spell_id: 60001,
                ..
            }
        )));
        assert!(all.iter().any(|e| matches!(
            e,
            SessionEvent::SpellDamageLog(p::SpellDamageLog {
                spell_id: 60001,
                ..
            })
        )));
        assert!(all.iter().any(|e| matches!(
            e,
            SessionEvent::PeriodicAuraLog(p::PeriodicAuraLog {
                spell_id: 60001,
                ..
            })
        )));
        assert!(all.iter().any(
            |e| matches!(e,SessionEvent::MonsterMove{path,..} if path.len()==2 && path[0]!=path[1])
        ));
        assert!(all
            .iter()
            .any(|e| matches!(e, SessionEvent::AttackStart { .. })));
        assert!(all.iter().any(|e|matches!(e,SessionEvent::ObjectValues{fields,..} if fields.unit_auras().any(|a|a.spell_id==60001 && !a.is_helpful()))));
    }
    #[test]
    fn unsupported_known_active_emits_failure_and_specific_reason() {
        let mut w = world();
        let mut p = NativeProjection::new(&w, w.state().player, &Catalog).unwrap();
        let target = NpcHandle {
            id: w.state().npcs[&2].id,
        };
        let input = p
            .local(
                &w,
                UiRequest::CastAtNpc {
                    ability: 60002,
                    target,
                },
            )
            .unwrap();
        w.step(1, Some(input), &mut SyntheticAuthority::default())
            .unwrap();
        let events = p.advance(&w).unwrap();
        assert!(events.iter().any(|e| matches!(
            e,
            SessionEvent::CastResult {
                spell_id: 60002,
                success: false,
                ..
            }
        )));
        assert!(events
            .iter()
            .any(|e| matches!(e,SessionEvent::Notification{text} if text.contains("77"))));
    }
    #[test]
    fn projection_rejects_skipped_ticks_without_partial_advancement() {
        let mut w = world();
        let mut p = NativeProjection::new(&w, w.state().player, &Catalog).unwrap();
        let mut a = SyntheticAuthority::default();
        w.step(1, None, &mut a).unwrap();
        w.step(2, None, &mut a).unwrap();
        assert!(matches!(p.advance(&w), Err(Error::StaleProjection)));
        assert_eq!(p.previous.tick, 0);
    }
}
#[cfg(test)]
mod stale_target_tests {
    use super::*;
    struct Catalog;
    impl ClientCatalog for Catalog {
        fn build(&self) -> u32 {
            5875
        }
        fn manifest_digest(&self) -> Digest {
            [9; 32]
        }
        fn contains_map(&self, _: u32) -> bool {
            true
        }
        fn contains_creature_display(&self, _: u32) -> bool {
            true
        }
        fn contains_item_display(&self, _: u32) -> bool {
            false
        }
        fn contains_spell(&self, _: u32) -> bool {
            true
        }
        fn player_power_type(&self, _: PlayerId) -> Option<u8> {
            Some(0)
        }
        fn item_metadata(&self, _: u32) -> Option<p::ItemInfo> {
            None
        }
        fn quest_metadata(&self, _: u32) -> Option<crate::QuestMetadata> {
            None
        }
        fn quest_greeting(&self, _: u32) -> Option<String> {
            None
        }
    }
    #[test]
    fn long_flight_failure_after_target_corpse_removal_needs_no_visible_target_guid() {
        let mut profile = n::synthetic_fixture::profile();
        let n::AbilityAdmission::Available(mut spec) = profile.abilities[0].admission.clone()
        else {
            unreachable!()
        };
        spec.spell_id = 60002;
        spec.projectile_speed = n::synthetic_fixture::float(1000.0);
        let spell::MageEffect::SchoolDamage(ref mut value) = spec.effects[0] else {
            unreachable!()
        };
        value.base_points = 2000;
        profile.abilities[1].admission = n::AbilityAdmission::Available(spec);
        let mut w = n::NativeWorld::synthetic_component_test(
            profile,
            [8; 16],
            n::synthetic_fixture::player(),
            1234,
        )
        .unwrap();
        let mut p = NativeProjection::new(&w, w.state().player, &Catalog).unwrap();
        let mut a = n::synthetic_fixture::SyntheticAuthority::default();
        let mut all = vec![];
        for tick in 1..=14 {
            let input = if tick == 1 || tick == 4 {
                Some(
                    p.local(
                        &w,
                        UiRequest::CastAtNpc {
                            ability: if tick == 1 { 60001 } else { 60002 },
                            target: NpcHandle {
                                id: w.state().npcs[&2].id,
                            },
                        },
                    )
                    .unwrap(),
                )
            } else {
                None
            };
            w.step(tick, input, &mut a).unwrap();
            all.extend(p.advance(&w).unwrap());
        }
        assert!(all
            .iter()
            .any(|e| matches!(e, SessionEvent::ObjectDestroyed(_))));
        // Source SpellEvent does not poll a traveling target every map frame.
        // The stale generation is resolved on the original flight deadline.
        assert!(!all.iter().any(|e| matches!(
            e,
            SessionEvent::SpellFailedOther {
                spell_id: 60001,
                ..
            }
        )));
        let spell::CastState::Traveling { impact_at_ms } = w.state().casts[&1].execution.state()
        else {
            panic!()
        };
        while w.state().now_ms < impact_at_ms {
            w.step(w.state().tick + 1, None, &mut a).unwrap();
            all.extend(p.advance(&w).unwrap());
        }
        assert!(all.iter().any(|e| matches!(
            e,
            SessionEvent::SpellFailedOther {
                spell_id: 60001,
                ..
            }
        )));
    }
}
#[cfg(test)]
mod resisted_launch_test {
    use super::*;
    #[test]
    fn committed_resisted_launch_projects_miss_without_hit_or_damage() {
        let mut w = n::synthetic_fixture::world();
        let mut projection =
            NativeProjection::new(&w, w.state().player, &super::tests::Catalog).unwrap();
        let mut authority = n::synthetic_fixture::SyntheticAuthority {
            resist_chance: 10000,
            ..Default::default()
        };
        let mut events = vec![];
        for tick in 1..=44 {
            let input = (tick == 1).then(|| n::synthetic_fixture::cast(&w, 60001));
            w.step(tick, input, &mut authority).unwrap();
            events.extend(projection.advance(&w).unwrap());
        }
        assert!(events.iter().any(|event|matches!(event,SessionEvent::SpellGo{hits,misses,..} if hits.is_empty() && misses.len()==1 && misses[0].1==2)));
        assert!(!events.iter().any(|event| matches!(
            event,
            SessionEvent::SpellDamageLog(_) | SessionEvent::PeriodicAuraLog(_)
        )));
    }
}

#[cfg(test)]
mod melee_projection_tests {
    use super::*;
    use n::synthetic_fixture::{cast, initialized_world, KernelAuthority};
    #[test]
    fn initialized_fields_equipment_and_real_attack_results_reach_session() {
        let mut world = initialized_world(9);
        let mut a = KernelAuthority::default();
        let mut projection =
            NativeProjection::new(&world, world.state().player, &super::tests::Catalog).unwrap();
        let initial = projection.initial_events(&world).unwrap();
        assert!(initial.iter().any(|e|matches!(e,SessionEvent::ItemCreate{fields,..} if fields.item_durability()==Some(1) && fields.item_max_durability()==Some(1))));
        assert!(initial.iter().any(|e|matches!(e,SessionEvent::ObjectValues{fields,..} if fields.unit_stat(1)==Some(23) && fields.player_skill(1).is_some_and(|s|s.skill_id==95 && s.value==1))));
        let command = cast(&world, 60001);
        let mut events = Vec::new();
        world.step(1, Some(command), &mut a).unwrap();
        events.extend(projection.advance(&world).unwrap());
        for tick in 2..=180 {
            world.step(tick, None, &mut a).unwrap();
            events.extend(projection.advance(&world).unwrap());
        }
        assert!(events
            .iter()
            .any(|e| matches!(e,SessionEvent::AttackerState(s) if s.damage>0)));
        assert!(events.iter().any(|e|matches!(e,SessionEvent::ObjectValues{fields,..} if fields.item_durability()==Some(0) && fields.item_max_durability()==Some(1))));
        assert!(events.iter().any(
            |e| matches!(e,SessionEvent::ObjectValues{fields,..} if fields.unit_aura_state()&1!=0)
        ));
        assert!(events.iter().any(|e|matches!(e,SessionEvent::ObjectValues{fields,..} if fields.player_skill(1).is_some_and(|s|s.value>1))));
    }
}

#[cfg(test)]
mod death_projection_tests {
    use super::*;
    use n::synthetic_fixture::*;
    #[test]
    fn death_projects_durability_attack_stop_release_flag_and_session_bound_root() {
        use benilla_sim::player_death as d;
        let mut profile = initialized_profile();
        let c = profile.npcs[0].combat.as_mut().unwrap();
        c.minimum_damage = float(10000.0);
        c.maximum_damage = float(10000.0);
        profile
            .configure_player_death(
                d::DeathConfig {
                    map_instanceable: false,
                    pve_reclaim_delay_enabled: true,
                },
                d::DeathSupport::checked(&d::REQUIRED_ABSENCES.map(|x| (x, Some(false)))).unwrap(),
            )
            .unwrap();
        let mut w =
            n::NativeWorld::synthetic_component_test(profile, [1; 16], player(), 7).unwrap();
        let mut projection = NativeProjection::new(&w, w.state().player, &tests::Catalog).unwrap();
        let mut orders = NativeMovementOrders::from_session_next_counter(37);
        let mut authority = KernelAuthority::default();
        let i = cast(&w, 60001);
        w.step(1, Some(i), &mut authority).unwrap();
        projection.advance(&w).unwrap();
        let mut events = Vec::new();
        while w.state().health > 0 {
            w.step(w.state().tick + 1, None, &mut authority).unwrap();
            events.extend(projection.advance(&w).unwrap());
        }
        assert!(events
            .iter()
            .any(|e| matches!(e, SessionEvent::DurabilityDamageDeath)));
        let npc_guid = projection.npc_guid(w.state().npcs[&2].id).unwrap();
        assert_eq!(w.state().npcs[&2].ai.target(), Some(n::SOLO_UNIT));
        assert_eq!(
            projection
                .npc_fields(&w, &w.state().npcs[&2])
                .unwrap()
                .unit_target(),
            Some(projection.player_guid())
        );
        assert!(events
            .iter()
            .any(|e| matches!(e, SessionEvent::AttackStop { .. })));
        w.step(w.state().tick + 1, None, &mut authority).unwrap();
        assert!(matches!(
            projection.advance(&w),
            Err(Error::MissingMovementOrderState)
        ));
        assert_eq!(orders.next_counter(), 37);
        let events = projection
            .advance_with_movement_orders(&w, &mut orders)
            .unwrap();
        assert!(events.iter().any(|e| matches!(
            e,
            SessionEvent::MoveMode {
                counter: 37,
                mode: p::MoveMode::Root,
                apply: true,
                ..
            }
        )));
        assert!(events.iter().any(|e| matches!(e,SessionEvent::ObjectValues { guid,fields } if *guid==projection.player_guid() && fields.player_release_timer_running() && fields.unit_dynamic_flags()==0)));
        assert_eq!(orders.next_counter(), 38);
        assert_eq!(w.state().npcs[&2].ai.target(), None);
        assert!(events.iter().any(|e| matches!(e, SessionEvent::ObjectValues { guid, fields } if *guid == npc_guid && fields.unit_target().is_none())));
        assert!(matches!(
            projection.advance_with_movement_orders(&w, &mut orders),
            Err(Error::StaleProjection)
        ));
        assert_eq!(orders.next_counter(), 38);
        let restored = n::NativeWorld::restore_synthetic_component_test(
            w.profile().clone(),
            w.checkpoint().unwrap(),
        )
        .unwrap();
        let fresh = NativeProjection::new(&restored, w.state().player, &tests::Catalog).unwrap();
        assert!(matches!(
            fresh.initial_events(&restored),
            Err(Error::MissingMovementOrderState)
        ));
        let mut restored_orders = NativeMovementOrders::from_session_next_counter(50);
        assert!(fresh
            .initial_events_with_movement_orders(&restored, &mut restored_orders)
            .unwrap()
            .iter()
            .any(|e| matches!(e, SessionEvent::MoveMode { counter: 50, .. })));
    }
}
#[cfg(test)]
mod death_stop_tests {
    use super::*;
    use n::synthetic_fixture::*;
    struct Authority(KernelAuthority);
    impl n::Authority for Authority {
        fn resolve(
            &mut self,
            request: &n::AuthorityRequest,
        ) -> Result<n::AuthorityReply, n::NativeError> {
            let mut result = n::Authority::resolve(&mut self.0, request)?;
            if request.binding.npc.is_some_and(|id| id.spawn == 2) {
                if let n::Facts::Npc(facts) = &mut result.facts {
                    facts.can_initiate_attack = request.binding.tick >= 20;
                }
            }
            Ok(result)
        }
    }
    #[test]
    fn detach_followed_by_same_frame_or_next_frame_evade_sends_one_attack_stop() {
        use benilla_sim::player_death as d;
        let mut profile = initialized_profile();
        let a = &mut profile.npcs[0];
        a.ai.reaction = ai::Reaction::Aggressive;
        a.combat.as_mut().unwrap().minimum_damage = float(10000.0);
        a.combat.as_mut().unwrap().maximum_damage = float(10000.0);
        let mut b = a.clone();
        b.ai.creature_id = 3;
        b.ai.reaction = ai::Reaction::Defensive;
        b.combat.as_mut().unwrap().minimum_damage = float(0.0);
        b.combat.as_mut().unwrap().maximum_damage = float(0.0);
        profile.npcs.push(b);
        profile
            .configure_player_death(
                d::DeathConfig {
                    map_instanceable: false,
                    pve_reclaim_delay_enabled: true,
                },
                d::DeathSupport::checked(&d::REQUIRED_ABSENCES.map(|d| (d, Some(false)))).unwrap(),
            )
            .unwrap();
        let mut world =
            n::NativeWorld::synthetic_component_test(profile, [1; 16], player(), 7).unwrap();
        let mut projection = NativeProjection::new(&world, player(), &tests::Catalog).unwrap();
        let a_guid = projection.npc_guid(world.state().npcs[&2].id).unwrap();
        let b_guid = projection.npc_guid(world.state().npcs[&3].id).unwrap();
        let mut command = cast(&world, 60001);
        if let n::Command::Cast { target, .. } = &mut command.input.command {
            *target = world.state().npcs[&3].id;
        }
        let mut authority = Authority(KernelAuthority::default());
        let mut orders = NativeMovementOrders::from_session_next_counter(5);
        let mut stops = BTreeMap::new();
        for tick in 1..=21 {
            world
                .step(
                    tick,
                    if tick == 1 {
                        Some(command.clone())
                    } else {
                        None
                    },
                    &mut authority,
                )
                .unwrap();
            let events = projection
                .advance_with_movement_orders(&world, &mut orders)
                .unwrap();
            for event in events {
                if let SessionEvent::AttackStop { attacker, .. } = event {
                    *stops.entry(attacker).or_insert(0) += 1;
                }
            }
            if tick == 20 {
                assert_eq!(world.state().health, 0);
                assert_eq!(stops.get(&a_guid), Some(&1));
                assert_eq!(stops.get(&b_guid), Some(&1));
            }
        }
        assert_eq!(stops.get(&a_guid), Some(&1));
        assert_eq!(stops.get(&b_guid), Some(&1));
    }
}

#[cfg(test)]
mod pose_projection_tests {
    use super::*;
    use n::synthetic_fixture as fixture;

    #[test]
    fn initial_and_ordered_pose_projection_use_bound_orientation() {
        let mut profile = fixture::profile();
        profile.player.orientation = n::Orientation::new(0.25).unwrap();
        let mut world =
            n::NativeWorld::synthetic_component_test(profile, [8; 16], fixture::player(), 7)
                .unwrap();
        let mut projection =
            NativeProjection::new(&world, world.state().player, &super::tests::Catalog).unwrap();
        assert!(projection.initial_events(&world).unwrap().iter().any(|e| matches!(e,
            SessionEvent::ObjectMove { guid, orientation, .. } if *guid == projection.player_guid() && *orientation == 0.25)));
        let position = world.state().position;
        let first = projection
            .pose(&world, position, n::Orientation::new(1.0).unwrap())
            .unwrap();
        let mut cast = projection
            .local(
                &world,
                UiRequest::CastAtNpc {
                    ability: 60001,
                    target: NpcHandle {
                        id: world.state().npcs[&2].id,
                    },
                },
            )
            .unwrap();
        cast.input.sequence += 1;
        let mut last = projection
            .pose(&world, position, n::Orientation::new(2.0).unwrap())
            .unwrap();
        last.input.sequence += 2;
        world
            .step_batch(
                1,
                vec![first, cast, last],
                &mut fixture::SyntheticAuthority::default(),
            )
            .unwrap();
        let events = projection.advance(&world).unwrap();
        let ordered: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                SessionEvent::ObjectMove {
                    guid, orientation, ..
                } if *guid == projection.player_guid() => Some(*orientation),
                SessionEvent::SpellStart { .. } => Some(-1.0),
                _ => None,
            })
            .collect();
        assert_eq!(ordered, vec![1.0, -1.0, 2.0]);
        let input = projection.move_to(&world, fixture::point(1.0)).unwrap();
        world
            .step(2, Some(input), &mut fixture::SyntheticAuthority::default())
            .unwrap();
        let events = projection.advance(&world).unwrap();
        assert!(events.iter().any(
            |e| matches!(e, SessionEvent::ObjectMove { guid, orientation, .. }
            if *guid == projection.player_guid() && *orientation == 2.0)
        ));
    }
}
#[cfg(test)]
#[path = "native_regeneration_tests.rs"]
mod regeneration_tests;

#[cfg(test)]
mod source_spell_tests {
    use super::*;
    #[test]
    fn selected_source_release_hit_projects_both_hit_and_resist_without_legacy_facts() {
        let mut outcomes = [false; 2];
        for seed in 0..200 {
            let mut world = n::NativeWorld::synthetic_component_test(
                n::synthetic_spell_fixture::profile(),
                [8; 16],
                n::synthetic_fixture::player(),
                seed,
            )
            .unwrap();
            let mut projection =
                NativeProjection::new(&world, world.state().player, &super::tests::Catalog)
                    .unwrap();
            projection.initial_events(&world).unwrap();
            let mut authority = n::synthetic_spell_fixture::spatial_only();
            for tick in 1..=3 {
                let input = (tick == 1).then(|| n::synthetic_fixture::cast(&world, 133));
                world.step(tick, input, &mut authority).unwrap();
                let events = projection.advance(&world).unwrap();
                for event in events {
                    if let SessionEvent::SpellGo {
                        spell_id,
                        hits,
                        misses,
                        ..
                    } = event
                    {
                        assert_eq!(spell_id, 133);
                        let record = world.last_tick().unwrap();
                        assert!(!record
                            .replies
                            .iter()
                            .any(|r| matches!(r.request.query, n::Query::Spell { .. })));
                        let chance = record
                            .replies
                            .iter()
                            .find_map(|r| match (&r.request.query, &r.facts) {
                                (
                                    n::Query::SourceSpell(n::SourceSpellQuery::Semantics(q)),
                                    n::Facts::SourceSpell(n::SourceSpellFacts::Semantics(f)),
                                ) if q.phase == n::SourceSpellPhase::ReleaseHit => {
                                    Some(f.resist_chance)
                                }
                                _ => None,
                            })
                            .unwrap();
                        let hit = record
                            .draws
                            .iter()
                            .find(|d| d.purpose == n::DrawPurpose::Hit)
                            .unwrap()
                            .value;
                        let resisted = hit <= i64::from(chance);
                        outcomes[usize::from(resisted)] = true;
                        assert_eq!(hits.len(), usize::from(!resisted));
                        assert_eq!(misses.len(), usize::from(resisted));
                        if resisted {
                            assert_eq!(misses[0].1, 2);
                        }
                    }
                }
            }
            if outcomes == [true, true] {
                break;
            }
        }
        assert_eq!(outcomes, [true, true]);
    }
}

#[cfg(test)]
#[path = "native_player_slow_tests.rs"]
mod player_slow_tests;

#[cfg(test)]
#[path = "native_level_sixty_tests.rs"]
mod level_sixty_tests;
