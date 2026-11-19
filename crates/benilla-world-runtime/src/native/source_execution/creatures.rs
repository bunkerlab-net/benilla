// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! Creature LoadFromDB executes at the grid factory, with mandatory reached owners.
use super::*;
use benilla_sim::creature_init::DisplayChoice;
use benilla_sim::resident_map::compute_cell_pair;
use benilla_sim::source_rng::{RawGenerator, SourceRandom, TraceEvent};

#[derive(Clone, Debug)]
pub struct CreatureSpawn {
    pub database_guid: u32,
    pub entry: u32,
    pub position: [f32; 4],
    pub model: u32,
    pub respawn_seconds: [u32; 2],
    pub respawn_radius: f32,
    pub relay: u32,
}

impl CreatureSpawn {
    /// Creature.h:313, including the draw for equal bounds.
    pub fn random_respawn_time(&self, random: &mut dyn CreatureRandom) -> Result<u32> {
        let [min, max] = self.respawn_seconds;
        random.creature_urand(min, max)
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CreatureEventData {
    pub entry: u32,
    pub model: u32,
    pub equipment: u32,
    pub start_spell: u32,
    pub end_spell: u32,
}
#[derive(Clone, Debug)]
pub struct CreatureTemplate {
    pub entry: u32,
    pub dynamic_guid: bool,
    pub active: bool,
    pub count_spawns: bool,
    pub displays: [DisplayChoice; 4],
    pub levels: [u32; 2],
    pub corpse_seconds: u32,
    pub can_fly: bool,
}

impl From<&LoadedCreatureInfo> for CreatureTemplate {
    fn from(row: &LoadedCreatureInfo) -> Self {
        let w = &row.words;
        Self {
            entry: w[0],
            dynamic_guid: w[23] & 0x10_0000 != 0,
            active: w[23] & 0x1000 != 0,
            count_spawns: w[23] & 0x20_0000 != 0,
            displays: std::array::from_fn(|i| DisplayChoice {
                id: w[5 + i],
                probability: w[9 + i],
            }),
            levels: [w[3], w[4]],
            corpse_seconds: w[88],
            can_fly: w[17] & 4 != 0,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreatureModel {
    pub id: u32,
    pub gender: u8,
    pub other_gender: u32,
}

impl From<&LoadedCreatureModelInfo> for CreatureModel {
    fn from(row: &LoadedCreatureModelInfo) -> Self {
        Self {
            id: row.words[0],
            gender: row.words[5] as u8,
            other_gender: row.words[6],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MissingCreatureModelGender {
    pub display: u32,
    pub alternate: u32,
}

pub struct CreatureModelSelection<T> {
    pub model: Option<T>,
    pub roll: Option<u32>,
    pub missing_gender: Option<MissingCreatureModelGender>,
}

/// ObjectMgr.cpp:1615-1633. Both storage bindings use the same lookup and draw order.
pub(super) fn select_source_creature_model<O: ?Sized, T>(
    owner: &mut O,
    display: u32,
    mut lookup: impl FnMut(&mut O, u32) -> Result<Option<T>>,
    other_gender: impl FnOnce(&T) -> u32,
    draw: impl FnOnce(&mut O) -> Result<u32>,
) -> Result<CreatureModelSelection<T>> {
    let mut selection = CreatureModelSelection {
        model: lookup(owner, display)?,
        roll: None,
        missing_gender: None,
    };
    if let Some(model) = selection.model.as_ref() {
        let alternate = other_gender(model);
        if alternate != 0 {
            let roll = draw(owner)?;
            selection.roll = Some(roll);
            if roll == 0 {
                if let Some(model) = lookup(owner, alternate)? {
                    selection.model = Some(model);
                } else {
                    selection.missing_gender =
                        Some(MissingCreatureModelGender { display, alternate });
                }
            }
        }
    }
    Ok(selection)
}

/// The definitions selected for a fresh ordinary DB creature's entry.
pub struct LoadedCreatureEntry<'a> {
    pub original: &'a LoadedCreatureInfo,
    pub effective: &'a LoadedCreatureInfo,
    pub spawn: &'a LoadedCreatureSpawnTemplate,
    pub movement: u32,
    pub map: u32,
}

#[derive(Clone, Debug, Default)]
pub struct SourceLoadedCreatureFields {
    pub scale: Option<u32>,
    pub race: u8,
    pub native_display: u32,
    pub display: u32,
    pub gender: u8,
    pub base_speeds: [u32; 2],
    pub model_run_speed: u32,
    pub equipment: SourceCreatureEquipment,
    pub name: Vec<u8>,
    pub default_movement: u32,
    pub damage_school: u32,
    pub can_parry: bool,
    pub can_block: bool,
    pub can_dual_wield: bool,
    pub no_reputation: bool,
    pub detection: u32,
    pub interaction_pause: u32,
    pub misc_flags: u8,
    pub gossip: u32,
    pub npc_flags: u32,
    pub attack_times: [u32; 3],
    pub dynamic_flags: u32,
    pub invisible: bool,
    pub ignore_mmap: bool,
    pub count_spawns: bool,
    pub can_modify_stats: bool,
    pub pvp: bool,
    pub chain_immune: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreatureEntryConfig {
    NormalCorpseDecay,
    EliteCorpseDecay,
    RareEliteCorpseDecay,
    WorldBossCorpseDecay,
    RareCorpseDecay,
    InteractionPause,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreatureGroup {
    pub id: u32,
    pub string_id: u32,
    pub has_formation: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreatureEntryOwner {
    Group(CreatureGroup),
    Conditional,
    Random { lookup_database_guid: u32 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreaturePhase {
    InitEntryBase,
    ModelAndDisplay,
    SetDeadAndZeroHealth,
    InitEntryEquipmentAndFlags,
    SelectLevelStats,
    UpdateEntryFlagsAndSpells,
    ApplyEventSpells,
    SelectFinalPointAndRelocate,
    PvpAndInstanceCreate,
    LinkingRegistration,
    LoadAddon,
    AttachGroup,
    RespawnDefinition,
    MapAddToWorld,
    AddToSpawnCount,
    MapAddViewpoint,
    MapAddVisibility,
    MotionInitialize,
    SelectAi,
    AiSpellListChanged,
    AiJustRespawned,
    InstanceRespawnAndCreatorLoot,
    Relay(u32),
    LinkingRespawn,
    FormationSlot,
    GroupRespawn,
    RabbitDay,
}
impl CreaturePhase {
    pub fn missing_owner(self) -> Refusal {
        refuse(match self {
            Self::InitEntryBase
            | Self::ModelAndDisplay
            | Self::InitEntryEquipmentAndFlags
            | Self::SelectLevelStats
            | Self::UpdateEntryFlagsAndSpells => "execution.creature.unit_initialization_owner",
            Self::SetDeadAndZeroHealth => "execution.creature.dead_health_owner",
            Self::ApplyEventSpells => "execution.event.f5_spell_owner",
            Self::SelectFinalPointAndRelocate => "execution.creature.geometry_position_owner",
            Self::PvpAndInstanceCreate => "execution.creature.create_script_owner",
            Self::LinkingRegistration | Self::LinkingRespawn => "execution.creature.linking_owner",
            Self::LoadAddon => "execution.creature.addon_owner",
            Self::AttachGroup | Self::FormationSlot | Self::GroupRespawn => {
                "execution.creature.group_owner"
            }
            Self::RespawnDefinition => "execution.creature.Creature_1688.respawn_radius_owner",
            Self::MapAddToWorld => "execution.creature.add_to_world_owner",
            Self::AddToSpawnCount => "execution.creature.Map_2995.AddToSpawnCount_owner",
            Self::MapAddViewpoint => "execution.creature.viewpoint_owner",
            Self::MapAddVisibility => "execution.creature.visibility_owner",
            Self::MotionInitialize => "execution.creature.motion_owner",
            Self::SelectAi | Self::AiSpellListChanged | Self::AiJustRespawned => {
                "execution.creature.ai_owner"
            }
            Self::InstanceRespawnAndCreatorLoot => "execution.creature.instance_respawn_owner",
            Self::Relay(_) => "execution.creature.relay_owner",
            Self::RabbitDay => "execution.creature.Creature_1761.localtime_owner",
        })
    }
}
#[derive(Clone, Debug)]
pub struct CreatureConstruction {
    pub object: ObjectHandle,
    pub spawn: CreatureSpawn,
    pub original_entry: u32,
    pub template: CreatureTemplate,
    pub event: Option<CreatureEventData>,
    pub group: Option<CreatureGroup>,
    pub position: [f32; 4],
    pub respawn_position: [f32; 4],
    pub model: Option<CreatureModel>,
    pub rolls: benilla_sim::creature_init::SelectionRolls,
    pub level: u32,
    pub respawn_delay: u32,
    pub corpse_delay: u32,
    pub respawn_time: i64,
    pub linking_trigger: bool,
    pub spawning_linked: bool,
    selected_ai: Option<SelectedCreatureAi>,
    event_ai: Option<SourceEventAi>,
    builtin_ai: Option<SourceBuiltinAi>,
    summoned_ai: Option<SourceSummonedAi>,
}
impl CreatureConstruction {
    fn new(
        object: ObjectHandle,
        spawn: CreatureSpawn,
        original_entry: u32,
        template: CreatureTemplate,
        event: Option<CreatureEventData>,
        group: Option<CreatureGroup>,
    ) -> Self {
        let corpse_delay = template.corpse_seconds;
        let position = spawn.position;
        Self {
            object,
            spawn,
            original_entry,
            template,
            event,
            group,
            position,
            respawn_position: position,
            model: None,
            rolls: benilla_sim::creature_init::SelectionRolls {
                model: None,
                gender: None,
                level: None,
            },
            level: 0,
            respawn_delay: 25,
            corpse_delay,
            respawn_time: 0,
            linking_trigger: false,
            spawning_linked: false,
            selected_ai: None,
            event_ai: None,
            builtin_ai: None,
            summoned_ai: None,
        }
    }

    /// Unit.cpp:419 and Creature.cpp:155 initialize the fresh loaded unit.
    pub fn construct_loaded_candidate(
        object: ObjectHandle,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        passengers: &mut SourcePassengers,
        motions: &mut std::collections::BTreeMap<ObjectHandle, SourceMotionMaster>,
    ) -> Result<()> {
        units.register(objects, object, Self::construct_loaded_unit())?;
        passengers.construct(object, objects)?;
        motions.insert(
            object,
            SourceMotionMaster::new(
                object,
                MotionDefault {
                    movement_type: 0,
                    default_path: 0,
                },
            ),
        );
        Ok(())
    }

    /// ObjectGridLoader.cpp:138-142 deletes a failed construction before world insertion.
    pub fn discard_loaded_candidate(
        object: ObjectHandle,
        units: &mut SourceUnits,
        passengers: &mut SourcePassengers,
        motions: &mut std::collections::BTreeMap<ObjectHandle, SourceMotionMaster>,
    ) -> Result<()> {
        require(
            motions
                .get(&object)
                .is_some_and(SourceMotionMaster::is_empty),
            "execution.creature.failed_construction_motion_owner",
        )?;
        passengers.discard_construction(object)?;
        units.discard_construction(object);
        motions.remove(&object);
        Ok(())
    }

    /// Object.cpp:105-118; Creature.cpp:1608 clears the unit's value array.
    pub fn reset_loaded_proto_fields(live: &mut SourceUnitFields) {
        live.combat_reach = Some(0);
        live.cast_speed = 0;
    }

    /// Creature.cpp:1705,1726; Unit.cpp:9224-9243 writes health before group updates.
    pub fn clear_loaded_health(live: &mut SourceUnitFields) -> Result<()> {
        let vitals = live
            .vitals
            .as_mut()
            .ok_or_else(|| refuse("execution.creature.Creature_1705.SetHealth_owner"))?;
        vitals.health = 0;
        require(
            !live.control_guids.has_charmer(),
            "execution.creature.Unit_9240.health_charmer_owner",
        )
    }

    pub fn construct_loaded_unit() -> SourceUnitFields {
        let mut live = SourceUnitFields::constructed_creature();
        live.linking_event_trigger = Some(false);
        live.movement = Some(SourceCreatureMovement::construct());
        live
    }

    /// Creature.cpp:335-344, before ChooseDisplayId. The template is already selected.
    pub fn init_loaded_entry_base(live: &mut SourceUnitFields, template: &LoadedCreatureInfo) {
        let w = &template.words;
        live.entry = w[0];
        live.loaded_creature.scale = Some(w[14]);
        live.loaded_creature.race = 0;
        live.unit_class = Some(u32::from(w[36] as u8));
        live.apply_creature_properties(w[23], w[17], w[33], w[32]);
    }

    /// Creature.cpp:360-385,2888-2918; Unit.cpp:10260-10275.
    pub fn apply_loaded_model(
        live: &mut SourceUnitFields,
        template: &LoadedCreatureInfo,
        model: [u32; 8],
    ) -> Result<CreatureModel> {
        let current_scale = f32::from_bits(
            live.loaded_creature
                .scale
                .ok_or_else(|| refuse("execution.creature.initial_object_scale_binding"))?,
        );
        let native_scale = f32::from_bits(template.words[14]);
        let normalized = if native_scale != 0.0 {
            current_scale / native_scale
        } else {
            1.0
        };
        let entry = &mut live.loaded_creature;
        entry.native_display = model[0];
        entry.display = model[0];
        live.bounding_radius = Some((normalized * f32::from_bits(model[1])).to_bits());
        live.combat_reach = Some((normalized * f32::from_bits(model[2])).to_bits());
        let movement = live
            .movement
            .get_or_insert_with(SourceCreatureMovement::construct);
        for (kind, (speed, override_bits)) in
            model[3..5].iter().zip(&template.words[29..31]).enumerate()
        {
            let bits = if f32::from_bits(*override_bits) != 0.0 {
                *override_bits
            } else {
                *speed
            };
            entry.base_speeds[kind] = bits;
            movement.set_speed_rate(kind, f32::from_bits(bits));
        }
        entry.model_run_speed = model[4];
        entry.gender = model[5] as u8;
        match template.words[36] {
            1 => live.power_type = Some(1),
            2 | 8 => live.power_type = Some(0),
            4 => live.power_type = Some(3),
            _ => (), // Creature.cpp:383-385 does not write PowerType.
        }
        Ok(CreatureModel {
            id: model[0],
            gender: entry.gender,
            other_gender: model[6],
        })
    }

    /// Creature.cpp:388-433. Equipment may refuse before the name and cast-speed writes.
    pub fn load_entry_equipment(
        &self,
        live: &mut SourceUnitFields,
        entry: &LoadedCreatureEntry<'_>,
        equipment: &LoadedEquipmentTemplates,
    ) -> Result<()> {
        live.loaded_creature.equipment.init_entry(
            self.event.map(|event| (event.equipment, event.entry)),
            Some(entry.spawn.equipment_id),
            entry.original.words[93],
            entry.effective.words[93],
            live.static_flags[0] & 0x80 != 0,
            equipment,
        )?;
        live.loaded_creature
            .name
            .clone_from(&entry.original.strings[0]);
        live.cast_speed = 1f32.to_bits();
        Ok(())
    }

    /// Creature.cpp:435-501, on the fresh ordinary DB path.
    pub fn finish_loaded_init_entry(
        &mut self,
        live: &mut SourceUnitFields,
        entry: &LoadedCreatureEntry<'_>,
        encounters: &LoadedInstanceEncounters,
        mut config: impl FnMut(CreatureEntryConfig) -> Result<u32>,
        swimmable: impl FnOnce(u32, f32) -> Result<bool>,
    ) -> Result<()> {
        let w = &entry.effective.words;
        let fields = &mut live.loaded_creature;
        live.movement
            .as_mut()
            .ok_or_else(|| refuse("execution.movement.flags_owner"))?
            .init_entry_flags_from_bits(
                [w[17], w[23]],
                false,
                false,
                Ok(Some(entry.spawn)),
                || {
                    let scale = fields
                        .scale
                        .ok_or_else(|| refuse("execution.creature.initial_object_scale_binding"))?;
                    swimmable(fields.native_display, f32::from_bits(scale))
                },
            )?;
        fields.default_movement = movement_enum(entry.movement)?;
        fields.damage_school = school_enum(w[62])?;
        fields.can_parry = w[23] & 4 == 0;
        fields.can_block = w[23] & 0x10 == 0;
        fields.can_dual_wield = w[23] & 0x800000 != 0;
        fields.no_reputation = false;
        fields.detection = (w[31] as f32).to_bits();
        live.apply_creature_capabilities(w[23]);
        self.corpse_delay = if w[88] != 0 {
            w[88]
        } else if encounters
            .by_credit()
            .get(&self.template.entry)
            .is_some_and(|rows| {
                rows.iter()
                    .any(|row| row.credit_type == 0 && row.entry.map == entry.map)
            })
        {
            3600
        } else {
            config(match w[37] {
                4 => CreatureEntryConfig::RareCorpseDecay,
                1 => CreatureEntryConfig::EliteCorpseDecay,
                2 => CreatureEntryConfig::RareEliteCorpseDecay,
                3 => CreatureEntryConfig::WorldBossCorpseDecay,
                _ => CreatureEntryConfig::NormalCorpseDecay,
            })?
        };
        live.loaded_creature.interaction_pause = if w[87] as i32 != -1 {
            w[87]
        } else {
            config(CreatureEntryConfig::InteractionPause)?
        };
        Ok(())
    }

    /// Creature.cpp:511-514 precedes the SelectLevel draw.
    pub fn begin_loaded_update_entry(live: &mut SourceUnitFields) {
        // Creature.cpp:512 writes the sheath byte through Unit.h:1359.
        if live.sheath_state != 1 {
            live.sheath_state = 1;
        }
        live.loaded_creature.misc_flags = 0x10;
    }

    pub fn select_level(&mut self, random: Option<&mut dyn CreatureRandom>) -> Result<()> {
        let [min, max] = self.template.levels;
        self.level = draw_source_creature_level([min, max], random)?;
        self.rolls.level = (min != max).then_some(self.level);
        Ok(())
    }

    /// Creature.cpp:523-538,1426-1475. Apply the spawn resources after SelectLevel.
    pub fn apply_loaded_level(
        &self,
        live: &mut SourceUnitFields,
        entry: &LoadedCreatureEntry<'_>,
        definitions: &CreatureLevelDefinitions,
        config: &CreatureLevelConfig,
    ) -> Result<benilla_sim::creature_init::SourceLevelStats> {
        let mut stats = definitions.stats(self.template.entry, self.level, false, config)?;
        let class = entry.effective.words[36];
        stats.apply_spawn(
            entry.spawn.cur_health,
            entry.spawn.cur_mana,
            true,
            class,
            entry.effective.words[18],
        );
        live.apply_source_level(&stats, class)?;
        Ok(stats)
    }

    /// Creature.cpp:541-607 on fresh DB construction, before spell-list selection.
    pub fn update_loaded_entry_flags(
        live: &mut SourceUnitFields,
        entry: &LoadedCreatureEntry<'_>,
        faction_flags: &std::collections::BTreeMap<u32, u32>,
    ) -> Result<()> {
        let w = &entry.effective.words;
        let spawn = entry.spawn;
        live.faction = if spawn.faction != 0 {
            spawn.faction
        } else {
            w[13]
        };
        let fields = &mut live.loaded_creature;
        fields.gossip = w[86];
        fields.npc_flags = if spawn.npc_flags == -1 {
            w[20]
        } else {
            spawn.npc_flags as u32
        };
        fields.attack_times = [w[60], w[60], w[61]];
        live.flags = if spawn.unit_flags == -1 {
            w[21]
        } else {
            spawn.unit_flags as u32
        };
        fields.dynamic_flags = w[22];
        live.resistances
            .set_creature_resistances(std::array::from_fn(|i| w[72 + i]));
        fields.invisible = w[23] & 0x80 != 0;
        fields.ignore_mmap = w[23] & 0x4000 != 0;
        fields.count_spawns = w[23] & 0x200000 != 0;
        live.immunities
            .apply_world_boss(live.pet_kind.is_some(), w[37]);
        fields.can_modify_stats = true;
        live.resistances.update_armor()?; // Creature.cpp:600; StatSystem.cpp:639-649.
        fields.pvp = faction_flags
            .get(&live.faction)
            .is_some_and(|flags| flags & 0x1000 != 0);
        Ok(())
    }

    /// Creature.cpp:613-615. Update the live immunity set before critter chain immunity.
    pub fn update_loaded_entry_immunities(
        live: &mut SourceUnitFields,
        template: &LoadedCreatureInfo,
        store: Option<&LoadedCreatureImmunities>,
    ) -> Result<()> {
        live.immunities.update(live.entry, 0, store)?;
        live.set_creature_type(template.words[16]);
        if live.critter {
            live.loaded_creature.chain_immune = true;
        }
        Ok(())
    }

    /// Creature.cpp:617-628. These reached calls still require owners.
    pub fn require_loaded_entry_callbacks(&self, entry: &LoadedCreatureEntry<'_>) -> Result<()> {
        require(
            self.event.is_none(),
            "execution.creature.loaded_game_event_spells_owner",
        )?;
        require(
            entry.effective.words[91] == 0
                && entry.effective.words[92] == 0
                && entry.spawn.string_id == 0,
            "execution.creature.loaded_string_id_owner",
        )
    }

    pub fn selected_ai(&self) -> Option<&SelectedCreatureAi> {
        self.selected_ai.as_ref()
    }
    pub fn event_ai(&self) -> Option<&SourceEventAi> {
        self.event_ai.as_ref()
    }
    pub fn event_ai_mut(&mut self) -> Option<&mut SourceEventAi> {
        self.event_ai.as_mut()
    }
    /// Transfer the constructed AI to its map-local runtime owner.
    pub fn take_event_ai(&mut self) -> Option<SourceEventAi> {
        self.event_ai.take()
    }
    pub fn builtin_ai(&self) -> Option<&SourceBuiltinAi> {
        self.builtin_ai.as_ref()
    }
    pub fn builtin_ai_mut(&mut self) -> Option<&mut SourceBuiltinAi> {
        self.builtin_ai.as_mut()
    }
    pub fn summoned_ai(&self) -> Option<&SourceSummonedAi> {
        self.summoned_ai.as_ref()
    }
    pub fn summoned_ai_mut(&mut self) -> Option<&mut SourceSummonedAi> {
        self.summoned_ai.as_mut()
    }
    pub fn construct_summoned_ai(
        &mut self,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        ais: &mut SourceCreatureAis,
        event: SourceEventAi,
        created_by_spell: Option<u32>,
    ) -> Result<()> {
        require(
            self.summoned_ai.is_none(),
            "execution.summoned_ai.already_constructed",
        )?;
        require(
            self.selected_ai() == Some(&event.selected),
            "execution.summoned_ai.selection_mismatch",
        )?;
        self.summoned_ai = Some(SourceSummonedAi::from_event(
            event,
            objects,
            units,
            ais,
            created_by_spell,
        )?);
        Ok(())
    }
    pub fn construct_builtin_ai(
        &mut self,
        objects: &SourceObjects,
        owners: BuiltinAiFactoryOwners<'_>,
    ) -> Result<()> {
        require(
            self.builtin_ai.is_none(),
            "execution.builtin_ai.already_constructed",
        )?;
        let selected = self
            .selected_ai()
            .ok_or_else(|| refuse("execution.ai.selection_absent"))?;
        self.builtin_ai = Some(SourceCreatureAis::construct_loaded_builtin(
            objects, selected, owners,
        )?);
        Ok(())
    }

    /// The grid replay drops its construction after Creature.cpp:1768. Guard and Null
    /// must have their exact identity live. A script must already have its full derived state live.
    pub fn check_builtin_ai_discard(
        &self,
        objects: &SourceObjects,
        ais: &SourceCreatureAis,
    ) -> Result<()> {
        if let Some(ai) = self.builtin_ai() {
            let selected = ai.selected();
            require(
                matches!(
                    selected.executable().builtin_kind(),
                    Some(BuiltinCreatureAi::Guard | BuiltinCreatureAi::Null)
                ),
                "execution.creature.Creature_1768.builtin_ai_live_retention_owner",
            )?;
            require(
                matches!(
                    &ais.get(objects, self.object)?.loaded_class,
                    LoadedCreatureAiClass::Builtin(executable) if executable == selected.executable()
                ),
                "execution.creature.Creature_1768.builtin_ai_identity_owner",
            )?;
        }
        if let Some(AiExecutable::ScriptDevGetAi(binding)) =
            self.selected_ai().map(SelectedCreatureAi::executable)
        {
            require(
                matches!(
                    &ais.get(objects, self.object)?.loaded_class,
                    LoadedCreatureAiClass::Scripted(script) if script.binding() == binding.as_ref()
                ),
                "execution.creature.Creature_1768.scripted_ai_identity_owner",
            )?;
        }
        Ok(())
    }
    pub fn construct_pet_ai(
        &mut self,
        objects: &SourceObjects,
        owners: PetAiConstructionOwners<'_>,
    ) -> Result<()> {
        require(
            self.builtin_ai.is_none(),
            "execution.builtin_ai.already_constructed",
        )?;
        let selected = self
            .selected_ai()
            .ok_or_else(|| refuse("execution.ai.selection_absent"))?;
        self.builtin_ai = Some(SourceBuiltinAi::construct_pet(
            selected.clone(),
            objects,
            owners,
        )?);
        Ok(())
    }
    pub fn construct_event_ai(
        &mut self,
        objects: &SourceObjects,
        definitions: std::sync::Arc<EventAiDefinitions>,
        build: EventAiBuild,
        base_ais: &SourceCreatureAis,
    ) -> Result<()> {
        self.construct_event_ai_with_base(objects, definitions, build, |selected| {
            base_ais.get(objects, selected.object())?;
            Ok(())
        })
    }

    /// CreatureEventAI.cpp:87 constructs CreatureAI before reading EventAI definitions.
    pub fn construct_event_ai_with_base(
        &mut self,
        objects: &SourceObjects,
        definitions: std::sync::Arc<EventAiDefinitions>,
        build: EventAiBuild,
        bind_base: impl FnOnce(&SelectedCreatureAi) -> Result<()>,
    ) -> Result<()> {
        require(
            self.event_ai.is_none(),
            "execution.event_ai.already_constructed",
        )?;
        let selected = self
            .selected_ai()
            .ok_or_else(|| refuse("execution.ai.selection_absent"))?;
        selected.validate(objects)?;
        if selected.executable().builtin_kind() != Some(BuiltinCreatureAi::Event) {
            return Err(selected.constructor_refusal(objects)?);
        }
        bind_base(selected)?;
        self.event_ai = Some(SourceEventAi::construct(
            selected.clone(),
            objects,
            definitions,
            build,
        )?);
        Ok(())
    }
    /// Retain the constructed object across lives; Creature.cpp:717-750,1912-1944.
    pub fn into_lifecycle(
        self,
        settings: CreatureLifecycleSettings,
        objects: &SourceObjects,
    ) -> Result<SourceCreatureLifecycle> {
        SourceCreatureLifecycle::new(self, settings, objects)
    }
}

// MotionMaster.h:49-79 and SharedDefines.h:169-180 pin the C++ enum value ranges.
pub(super) fn movement_enum(value: u32) -> Result<u32> {
    require(
        value <= 31,
        "execution.creature.Creature_453.movement_enum_undefined",
    )?;
    Ok(value)
}

pub(super) fn school_enum(value: u32) -> Result<u32> {
    require(
        value <= 7,
        "execution.creature.Creature_458.school_enum_undefined",
    )?;
    Ok(value)
}

#[cfg(test)]
#[path = "creature_entry_tests.rs"]
mod entry_tests;

/// Borrow the calling engine without result inputs; Util.cpp:68-96.
pub trait CreatureRandom {
    fn creature_stream(&self) -> Result<u64> {
        Err(refuse("execution.creature.SelectLevel_urand_thread_owner"))
    }
    fn creature_raw_position(&self) -> Result<u64> {
        Err(refuse("execution.creature.raw_position_owner"))
    }
    fn creature_irand(&mut self, min: i32, max: i32) -> Result<i32>;
    fn creature_urand(&mut self, min: u32, max: u32) -> Result<u32>;
    fn creature_urand_default(&mut self) -> Result<u32> {
        Err(refuse("execution.creature.urand_default_owner"))
    }
}
impl<E: RawGenerator, S: FnMut(TraceEvent)> CreatureRandom for SourceRandom<'_, E, S> {
    fn creature_stream(&self) -> Result<u64> {
        Ok(self.stream())
    }
    fn creature_raw_position(&self) -> Result<u64> {
        Ok(self.cursor().raw)
    }
    fn creature_irand(&mut self, min: i32, max: i32) -> Result<i32> {
        self.irand(min, max)
            .map_err(|_| refuse("execution.creature.irand"))
    }
    fn creature_urand(&mut self, min: u32, max: u32) -> Result<u32> {
        self.urand(min, max)
            .map_err(|_| refuse("execution.creature.urand"))
    }
    fn creature_urand_default(&mut self) -> Result<u32> {
        self.urand_default()
            .map_err(|_| refuse("execution.creature.urand_default"))
    }
}

/// Creature.cpp:670-690 retains uint32 accumulation and signed comparison/subtraction.
pub fn draw_source_creature_display(
    choices: [DisplayChoice; 4],
    random: &mut dyn CreatureRandom,
) -> Result<(u32, i32)> {
    draw_source_creature_display_with_chance(
        choices,
        source_creature_display_chance(&choices),
        random,
    )
}

fn source_creature_display_chance(choices: &[DisplayChoice; 4]) -> u32 {
    choices
        .iter()
        .filter(|choice| choice.id != 0)
        .fold(0u32, |total, choice| total.wrapping_add(choice.probability))
}

fn draw_source_creature_display_with_chance(
    choices: [DisplayChoice; 4],
    total: u32,
    random: &mut dyn CreatureRandom,
) -> Result<(u32, i32)> {
    let upper = (total as i32)
        .checked_sub(1)
        .ok_or_else(|| refuse("execution.creature.display_signed_overflow"))?
        .max(0);
    let initial = random.creature_irand(0, upper)?;
    let mut roll = initial;
    for choice in choices {
        if choice.id != 0 {
            if roll < choice.probability as i32 {
                return Ok((choice.id, initial));
            }
            roll = (roll as u32).wrapping_sub(choice.probability) as i32;
        }
    }
    Ok((0, initial))
}

#[cfg(test)]
#[path = "creature_display_tests.rs"]
mod display_tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreatureFactoryObservation {
    ModelSelected,
    NoExistingCreature,
    ConditionalSkipped,
    RandomSkipped,
    OriginalTemplate(u32),
    GuidAllocated,
    CreateFromProto,
    DisplayChance(u32),
    DisplayDrawn,
    DisplayFallback,
    MissingModelGender(MissingCreatureModelGender),
    FinalPositionRelocated,
    RespawnDelay(u32),
    RespawnTime { time: i64, corpse_delay: u32 },
    MapGridLinked,
    WorldLookupRegistered,
    BaseAddedToWorld,
    AiNotifyEnqueued(u32),
    ForceActiveChecked,
    SpawnCountChecked,
    MapActiveChecked,
}

pub trait CreatureFactoryConfig {
    /// Unit.cpp:9394 reads CONFIG_UINT32_CREATURE_RESPAWN_AGGRO_DELAY.
    fn creature_respawn_aggro_config(&self) -> Result<i32> {
        Err(refuse("execution.creature.loaded_notify_delay_config"))
    }

    /// Creature.cpp:1758 reads CONFIG_UINT32_RABBIT_DAY.
    fn creature_rabbit_day(&self) -> Result<u32> {
        Err(refuse("execution.creature.rabbit_day_config"))
    }
}
/// Execute reached owners through the reentrant factory; Creature.cpp:898-934,1738-1765.
pub trait CreatureFactoryOwner<D: GridDefinitions, P: GridProductOwner>:
    GridObjectFactory<D, P> + CreatureFactoryConfig
{
    fn creature_thread(&self) -> u64;
    fn creature_construct_candidate(
        &mut self,
        object: ObjectHandle,
        context: &mut GridFactoryContext<'_, D, P>,
    ) -> Result<()>;
    fn creature_spawn(
        &mut self,
        attempt: GridLoadAttempt,
        row: D::Row,
    ) -> Result<Option<CreatureSpawn>>;
    /// Object.cpp:2193-2203 lookup and spawnMask/Map::CanSpawn gate, before construction.
    fn creature_dynamic_spawn_allowed(&mut self, _database: u32, _row: &D::Row) -> Result<bool> {
        Err(refuse("execution.creature.dynamic_spawn_gate_owner"))
    }
    fn creature_event_guid(&mut self, database: u32) -> Result<bool>;
    fn creature_force_dynamic(&mut self) -> Result<bool>;
    fn creature_force_load_map(&mut self) -> Result<bool>;
    fn creature_group(&mut self, database: u32) -> Result<Option<CreatureGroup>>;
    /// Re-read the live group after relay/link callbacks; Creature.cpp:1749-1753.
    fn creature_current_group(
        &mut self,
        object: ObjectHandle,
        context: &GridFactoryContext<'_, D, P>,
    ) -> Result<Option<CreatureGroup>>;
    fn creature_entry(&mut self, database: u32, owner: CreatureEntryOwner) -> Result<u32>;
    fn creature_event(&mut self, database: u32) -> Result<Option<CreatureEventData>>;
    fn creature_template(&mut self, entry: u32) -> Result<Option<CreatureTemplate>>;
    fn creature_model(&mut self, display: u32) -> Result<Option<CreatureModel>>;
    fn creature_spawn_model(&mut self, spawn: &CreatureSpawn) -> Result<u32> {
        Ok(spawn.model)
    }
    fn observe_creature_factory(&mut self, _observation: CreatureFactoryObservation) {}
    fn creature_guids(&mut self) -> &mut MapGuidAllocators;
    fn creature_random(&mut self) -> Result<&mut dyn CreatureRandom>;
    fn creature_units(&self) -> &SourceUnits;
    fn creature_units_mut(&mut self) -> &mut SourceUnits;
    fn creature_spawn_relay(&mut self, state: &CreatureConstruction) -> Result<u32> {
        Ok(state.spawn.relay)
    }
    fn creature_respawn_time(
        &mut self,
        database: u32,
        context: &mut GridFactoryContext<'_, D, P>,
    ) -> Result<i64>;
    fn creature_save_respawn_time(
        &mut self,
        database: u32,
        time: i64,
        context: &mut GridFactoryContext<'_, D, P>,
    ) -> Result<()>;
    fn creature_wall_time(&mut self, object: ObjectHandle) -> Result<ClockSample>;
    fn creature_spawned_by_linked_mob(
        &mut self,
        state: &CreatureConstruction,
        context: &GridFactoryContext<'_, D, P>,
    ) -> Result<bool>;
    fn creature_linked_can_spawn(
        &mut self,
        state: &CreatureConstruction,
        context: &mut GridFactoryContext<'_, D, P>,
    ) -> Result<bool>;
    fn creature_dead_flying_height(
        &mut self,
        state: &CreatureConstruction,
        context: &mut GridFactoryContext<'_, D, P>,
    ) -> Result<f32>;
    fn creature_select_ai(
        &mut self,
        state: &CreatureConstruction,
        objects: &SourceObjects,
    ) -> Result<SelectedCreatureAi> {
        let inputs = self.creature_ai_inputs(state)?;
        require(
            inputs.effective_entry == state.template.entry,
            "execution.ai.effective_entry",
        )?;
        self.creature_ai_registry()?
            .select_with_live_control(objects, state.object, inputs, || {
                self.creature_ai_control_guids(objects, state.object)
            })
    }
    /// Live effective-template/control operands, not a selected AI answer.
    fn creature_ai_inputs(&mut self, _state: &CreatureConstruction) -> Result<CreatureAiInputs> {
        Err(refuse("execution.ai.live_inputs_owner"))
    }
    fn creature_ai_control_guids(
        &self,
        _objects: &SourceObjects,
        _unit: ObjectHandle,
    ) -> Result<SourceControlGuids> {
        Err(refuse(
            "execution.ai.CreatureAISelector_41.HasCharmer_owner",
        ))
    }
    fn creature_ai_registry(&self) -> Result<&SourceAiRegistry> {
        Err(refuse("execution.ai.registry_owner"))
    }
    fn creature_event_ai_definitions(&self) -> Result<std::sync::Arc<EventAiDefinitions>> {
        Err(refuse("execution.event_ai.loaded_definitions_owner"))
    }
    fn creature_event_ai_build(&self) -> Result<EventAiBuild> {
        Err(refuse("execution.event_ai.source_build_identity_owner"))
    }
    fn creature_base_ai_instances(&self) -> Result<&SourceCreatureAis> {
        Err(refuse("execution.unit_ai.component_registry_binding_owner"))
    }
    /// Split the live registries and thread RNG into disjoint borrows.
    fn creature_builtin_ai_owners(
        &mut self,
        _selected: &SelectedCreatureAi,
    ) -> Result<BuiltinAiFactoryOwners<'_>> {
        Err(refuse(
            "execution.builtin_ai.live_unit_and_base_binding_owner",
        ))
    }
    /// None is explicitly unwritten UNIT_CREATED_BY_SPELL, never an inferred spell zero.
    fn creature_pet_ai_owners(
        &mut self,
        _selected: &SelectedCreatureAi,
    ) -> Result<PetAiConstructionOwners<'_>> {
        Err(refuse("execution.pet_ai.live_unit_and_base_binding_owner"))
    }
    /// Bind the live Unit/base registries and UNIT_CREATED_BY_SPELL, not a default spell.
    fn creature_summoned_ai_owners(
        &mut self,
        _event: &SourceEventAi,
    ) -> Result<(&mut SourceUnits, &mut SourceCreatureAis, Option<u32>)> {
        Err(refuse(
            "execution.summoned_ai.live_unit_and_base_binding_owner",
        ))
    }
    /// Must execute the selected constructor before SpellListChanged/JustRespawned.
    fn creature_ai_construct(
        &mut self,
        state: &mut CreatureConstruction,
        context: &mut GridFactoryContext<'_, D, P>,
    ) -> Result<()> {
        construct_source_creature_ai(self, state, context)
    }
    fn creature_phase(
        &mut self,
        phase: CreaturePhase,
        _state: &mut CreatureConstruction,
        _context: &mut GridFactoryContext<'_, D, P>,
    ) -> Result<()> {
        Err(phase.missing_owner())
    }
    fn creature_created(
        &mut self,
        state: CreatureConstruction,
        context: &mut GridFactoryContext<'_, D, P>,
    ) -> Result<()>;
    fn creature_destroy_candidate(
        &mut self,
        object: ObjectHandle,
        context: &mut GridFactoryContext<'_, D, P>,
    ) -> Result<()>;
}
pub(super) fn failed<D, P, F>(
    factory: &mut F,
    object: ObjectHandle,
    context: &mut GridFactoryContext<'_, D, P>,
) -> Result<GridCandidateOutcome>
where
    D: GridDefinitions,
    P: GridProductOwner,
    F: CreatureFactoryOwner<D, P>,
{
    factory.creature_destroy_candidate(object, context)?;
    context.objects.destroy(object, context.cells)?;
    Ok(GridCandidateOutcome::FailedAndDestroyed(object))
}
// Construction precedes LoadFromDB, including duplicate skips; ObjectGridLoader.cpp:133-140.
pub fn load_source_creature<D, P, F>(
    factory: &mut F,
    attempt: GridLoadAttempt,
    row: D::Row,
    context: &mut GridFactoryContext<'_, D, P>,
) -> Result<GridCandidateOutcome>
where
    D: GridDefinitions,
    P: GridProductOwner,
    F: CreatureFactoryOwner<D, P>,
{
    load_source_creature_inner(factory, attempt, row, None, 0, context)
}

/// Object.cpp:2191-2212 enters LoadFromDB with requested GUID zero, not the grid-loader GUID.
pub fn spawn_source_creature<D, P, F>(
    factory: &mut F,
    attempt: GridLoadAttempt,
    row: D::Row,
    forced_entry: u32,
    context: &mut GridFactoryContext<'_, D, P>,
) -> Result<Option<GridCandidateOutcome>>
where
    D: GridDefinitions,
    P: GridProductOwner,
    F: CreatureFactoryOwner<D, P>,
{
    require(
        attempt.kind == GridSpawnKind::Creature,
        "execution.creature.factory_kind",
    )?;
    if !factory.creature_dynamic_spawn_allowed(attempt.guid, &row)? {
        return Ok(None);
    }
    load_source_creature_inner(factory, attempt, row, Some(0), forced_entry, context).map(Some)
}

/// Pool leaves pass their database GUID directly; PoolManager.cpp:452-454.
pub fn load_pool_source_creature<D, P, F>(
    factory: &mut F,
    attempt: GridLoadAttempt,
    row: D::Row,
    context: &mut GridFactoryContext<'_, D, P>,
) -> Result<GridCandidateOutcome>
where
    D: GridDefinitions,
    P: GridProductOwner,
    F: CreatureFactoryOwner<D, P>,
{
    load_source_creature_inner(factory, attempt, row, Some(attempt.guid), 0, context)
}

fn load_source_creature_inner<D, P, F>(
    factory: &mut F,
    attempt: GridLoadAttempt,
    row: D::Row,
    requested_guid: Option<u32>,
    forced_entry: u32,
    context: &mut GridFactoryContext<'_, D, P>,
) -> Result<GridCandidateOutcome>
where
    D: GridDefinitions,
    P: GridProductOwner,
    F: CreatureFactoryOwner<D, P>,
{
    let (object, loaded) =
        prepare_source_creature(factory, attempt, row, requested_guid, forced_entry, context)?;
    let Some(mut state) = loaded else {
        return failed(factory, object, context);
    };
    let group = state.group;
    // UpdateEntry applies active spells before positioning; Creature.cpp:617-619.
    if state
        .event
        .is_some_and(|event| event.start_spell != 0 || event.end_spell != 0)
    {
        factory.creature_phase(CreaturePhase::ApplyEventSpells, &mut state, context)?;
    }
    factory.creature_phase(
        CreaturePhase::SelectFinalPointAndRelocate,
        &mut state,
        context,
    )?;
    if !relocate_source_creature_for_create(context.objects, object, state.position)? {
        return failed(factory, object, context);
    }
    factory.observe_creature_factory(CreatureFactoryObservation::FinalPositionRelocated);
    for phase in [
        CreaturePhase::PvpAndInstanceCreate,
        CreaturePhase::LinkingRegistration,
        CreaturePhase::LoadAddon,
    ] {
        factory.creature_phase(phase, &mut state, context)?;
    }
    if group.is_some() {
        factory.creature_phase(CreaturePhase::AttachGroup, &mut state, context)?;
    }
    state.respawn_position = state.position;
    context.objects.set_respawn_definition(
        object,
        SourcePose(state.respawn_position.map(f32::to_bits)),
        true,
    )?;
    factory.creature_phase(CreaturePhase::RespawnDefinition, &mut state, context)?;
    state.respawn_delay = state
        .spawn
        .random_respawn_time(factory.creature_random()?)?;
    factory
        .creature_units_mut()
        .get_mut(context.objects, object)?
        .respawn_delay = state.respawn_delay;
    factory.observe_creature_factory(CreatureFactoryObservation::RespawnDelay(
        state.respawn_delay,
    ));
    let runtime_counter = context
        .objects
        .get(object)?
        .guid
        .ok_or_else(|| refuse("execution.creature.guid_absent"))?
        .counter();
    let new_spawning = source_creature_new_spawning(attempt.guid, runtime_counter);
    if !new_spawning {
        state.corpse_delay = state
            .corpse_delay
            .min(state.respawn_delay.wrapping_mul(9) / 10);
    }
    context.objects.get_mut(object)?.alive = true;
    state.respawn_time = factory.creature_respawn_time(attempt.guid, context)?;
    factory
        .creature_units_mut()
        .get_mut(context.objects, object)?
        .respawn_time = state.respawn_time;
    factory.observe_creature_factory(CreatureFactoryObservation::RespawnTime {
        time: state.respawn_time,
        corpse_delay: state.corpse_delay,
    });
    let now = factory.creature_wall_time(object)?;
    require(
        now.kind() == ClockKind::WallSeconds,
        "execution.creature.respawn_clock",
    )?;
    if state.respawn_time > now.value() {
        if new_spawning && group.is_none() {
            return failed(factory, object, context);
        }
        dead(factory, &mut state, context)?;
    } else if state.respawn_time != 0 {
        state.respawn_time = 0;
        factory
            .creature_units_mut()
            .get_mut(context.objects, object)?
            .respawn_time = 0;
        factory.observe_creature_factory(CreatureFactoryObservation::RespawnTime {
            time: 0,
            corpse_delay: state.corpse_delay,
        });
        factory.creature_save_respawn_time(attempt.guid, 0, context)?;
    }
    state.spawning_linked = factory.creature_spawned_by_linked_mob(&state, context)?;
    if state.spawning_linked
        && context.objects.get(object)?.alive
        && !factory.creature_linked_can_spawn(&state, context)?
    {
        dead(factory, &mut state, context)?;
    }
    add_source_creature(factory, &mut state, context)?;
    factory.creature_phase(CreaturePhase::MotionInitialize, &mut state, context)?;
    select_source_creature_ai(factory, &mut state, context)?;
    for phase in [
        CreaturePhase::AiSpellListChanged,
        CreaturePhase::AiJustRespawned,
        CreaturePhase::InstanceRespawnAndCreatorLoot,
    ] {
        factory.creature_phase(phase, &mut state, context)?;
    }
    let relay = factory.creature_spawn_relay(&state)?;
    if relay != 0 {
        factory.creature_phase(CreaturePhase::Relay(relay), &mut state, context)?;
    }
    if state.linking_trigger && context.objects.get(object)?.alive {
        factory.creature_phase(CreaturePhase::LinkingRespawn, &mut state, context)?;
    }
    state.group = factory.creature_current_group(object, context)?;
    if let Some(group) = state.group {
        if group.has_formation {
            factory.creature_phase(CreaturePhase::FormationSlot, &mut state, context)?;
        }
        factory.creature_phase(CreaturePhase::GroupRespawn, &mut state, context)?;
    }
    if context.objects.get(object)?.alive && factory.creature_rabbit_day()? != 0 {
        factory.creature_phase(CreaturePhase::RabbitDay, &mut state, context)?;
    }
    factory.creature_created(state, context)?;
    Ok(GridCandidateOutcome::Loaded(object))
}
pub(super) fn prepare_source_creature<D, P, F>(
    factory: &mut F,
    attempt: GridLoadAttempt,
    row: D::Row,
    requested_guid: Option<u32>,
    forced_entry: u32,
    context: &mut GridFactoryContext<'_, D, P>,
) -> Result<(ObjectHandle, Option<CreatureConstruction>)>
where
    D: GridDefinitions,
    P: GridProductOwner,
    F: CreatureFactoryOwner<D, P>,
{
    require(
        attempt.kind == GridSpawnKind::Creature,
        "execution.creature.factory_kind",
    )?;
    let object = context
        .objects
        .construct(factory.creature_thread(), SourceObjectKind::Creature)?;
    factory.creature_construct_candidate(object, context)?;
    // An unsupported owner is a stopped execution, not a source LoadFromDB false/delete.
    let requested = if let Some(guid) = requested_guid {
        guid
    } else if factory.creature_event_guid(attempt.guid)? {
        0
    } else {
        attempt.guid
    };
    let Some(spawn) = factory.creature_spawn(attempt, row)? else {
        return Ok((object, None));
    };
    require(
        spawn.database_guid == attempt.guid,
        "execution.creature.definition_binding",
    )?;
    if !context.objects.candidate_allowed(
        attempt.grid.map,
        DatabaseLookupKind::Creature,
        attempt.guid,
    ) {
        return Ok((object, None));
    }
    factory.observe_creature_factory(CreatureFactoryObservation::NoExistingCreature);
    let group = factory.creature_group(attempt.guid)?;
    let mut entry = if forced_entry != 0 {
        forced_entry
    } else {
        spawn.entry
    };
    // Group, conditional, random and event fallback are ordered; Creature.cpp:1630-1653.
    if entry == 0 {
        if let Some(group) = group {
            entry = factory.creature_entry(attempt.guid, CreatureEntryOwner::Group(group))?;
        }
    }
    if entry == 0 {
        entry = factory.creature_entry(attempt.guid, CreatureEntryOwner::Conditional)?;
    } else {
        factory.observe_creature_factory(CreatureFactoryObservation::ConditionalSkipped);
    }
    // GetDbGuid is still zero before _Create, not the LoadFromDB argument; Creature.cpp:1646,1608.
    if entry == 0 {
        entry = factory.creature_entry(
            attempt.guid,
            CreatureEntryOwner::Random {
                lookup_database_guid: 0,
            },
        )?;
    } else {
        factory.observe_creature_factory(CreatureFactoryObservation::RandomSkipped);
    }
    let event = factory.creature_event(attempt.guid)?;
    if entry == 0 {
        entry = event.map_or(0, |event| event.entry);
    }
    if entry == 0 {
        return Ok((object, None));
    }
    factory.observe_creature_factory(CreatureFactoryObservation::OriginalTemplate(entry));
    let Some(original) = factory.creature_template(entry)? else {
        return Ok((object, None));
    };
    require(
        original.entry == entry,
        "execution.creature.template_binding",
    )?;
    let force_dynamic = factory.creature_force_dynamic()?;
    require(
        factory.creature_guids().map() == attempt.grid.map,
        "execution.creature.allocator_map",
    )?;
    let allocation = factory.creature_guids().spawn_counter(SpawnGuidRequest {
        high: HighGuid::Creature,
        database: attempt.guid,
        requested,
        force_dynamic,
        extra_dynamic: original.dynamic_guid,
        group: group.is_some(),
    })?;
    require(!allocation.stop_now, "execution.creature.guid_stop_now")?;
    factory.observe_creature_factory(CreatureFactoryObservation::GuidAllocated);
    context
        .objects
        .bind_map(object, attempt.grid.map, context.cells)?;
    context.objects.set_respawn_definition(
        object,
        SourcePose(spawn.position.map(f32::to_bits)),
        true,
    )?;
    // GUID uses the original entry before event InitEntry override; Creature.cpp:1604-1610,322-324.
    context.objects.assign_guid(
        object,
        attempt.guid,
        SourceGuid::with_entry(HighGuid::Creature, entry, allocation.counter),
    )?;
    factory.observe_creature_factory(CreatureFactoryObservation::CreateFromProto);
    let effective = event.filter(|e| e.entry != 0).map_or(entry, |e| e.entry);
    let template = if effective == entry {
        original
    } else {
        let Some(template) = factory.creature_template(effective)? else {
            return Ok((object, None));
        };
        require(
            template.entry == effective,
            "execution.creature.template_binding",
        )?;
        template
    };
    let mut state = CreatureConstruction::new(object, spawn, entry, template, event, group);
    factory.creature_phase(CreaturePhase::InitEntryBase, &mut state, context)?;
    let mut display = event.map_or(0, |event| event.model);
    if display == 0 {
        display = factory.creature_spawn_model(&state.spawn)?;
    }
    if display == 0 {
        let choices = state.template.displays;
        let total = source_creature_display_chance(&choices);
        factory.observe_creature_factory(CreatureFactoryObservation::DisplayChance(total));
        let (chosen, roll) =
            draw_source_creature_display_with_chance(choices, total, factory.creature_random()?)?;
        factory.observe_creature_factory(CreatureFactoryObservation::DisplayDrawn);
        display = chosen;
        state.rolls.model = Some(roll as u32);
        if display == 0 {
            display = factory
                .creature_template(1)?
                .map_or(0, |t| t.displays[0].id);
            factory.observe_creature_factory(CreatureFactoryObservation::DisplayFallback);
        }
    }
    if display == 0 {
        return Ok((object, None));
    }
    let selection = select_source_creature_model(
        factory,
        display,
        F::creature_model,
        |model| model.other_gender,
        |factory| factory.creature_random()?.creature_urand(0, 1),
    )?;
    if let Some(notice) = selection.missing_gender {
        factory.observe_creature_factory(CreatureFactoryObservation::MissingModelGender(notice));
    }
    let Some(model) = selection.model else {
        return Ok((object, None));
    };
    state.rolls.gender = selection.roll;
    factory.observe_creature_factory(CreatureFactoryObservation::ModelSelected);
    state.model = Some(model);
    factory.creature_phase(CreaturePhase::ModelAndDisplay, &mut state, context)?;
    factory.creature_phase(
        CreaturePhase::InitEntryEquipmentAndFlags,
        &mut state,
        context,
    )?;
    let [min, max] = state.template.levels;
    if min == max {
        state.select_level(None)?;
    } else {
        let thread = factory.creature_thread();
        let random = factory.creature_random()?;
        require(
            random.creature_stream()? == thread,
            "execution.creature.SelectLevel_urand_thread_mismatch",
        )?;
        state.select_level(Some(random))?;
    }
    factory.creature_phase(CreaturePhase::SelectLevelStats, &mut state, context)?;
    factory.creature_phase(
        CreaturePhase::UpdateEntryFlagsAndSpells,
        &mut state,
        context,
    )?;
    Ok((object, Some(state)))
}

/// Selected construction only. EventAI keeps its existing definition/build/base owner path.
pub fn construct_source_creature_ai<D, P, F>(
    factory: &mut F,
    state: &mut CreatureConstruction,
    context: &mut GridFactoryContext<'_, D, P>,
) -> Result<()>
where
    D: GridDefinitions,
    P: GridProductOwner,
    F: CreatureFactoryOwner<D, P>,
{
    let selected = state
        .selected_ai()
        .ok_or_else(|| refuse("execution.ai.selection_absent"))?;
    selected.validate(context.objects)?;
    if let AiExecutable::ScriptDevGetAi(binding) = selected.executable() {
        let kind = SourceScriptedAiKind::for_registered_name(&binding.name)?;
        let owners = factory.creature_builtin_ai_owners(selected)?;
        return SourceCreatureAis::construct_loaded_script(
            context.objects,
            selected,
            kind,
            binding,
            owners,
        );
    }
    if selected.executable().builtin_kind() == Some(BuiltinCreatureAi::Pet) {
        let owners = factory.creature_pet_ai_owners(selected)?;
        return state.construct_pet_ai(context.objects, owners);
    }
    if matches!(
        selected.executable().builtin_kind(),
        Some(BuiltinCreatureAi::Null | BuiltinCreatureAi::Guard | BuiltinCreatureAi::Possessed)
    ) {
        let owners = factory.creature_builtin_ai_owners(selected)?;
        return state.construct_builtin_ai(context.objects, owners);
    }
    if matches!(
        selected.executable().builtin_kind(),
        Some(BuiltinCreatureAi::Totem | BuiltinCreatureAi::Guardian)
    ) {
        let definitions = factory.creature_event_ai_definitions()?;
        let build = factory.creature_event_ai_build()?;
        let event = SourceEventAi::construct_inherited(
            selected.clone(),
            context.objects,
            definitions,
            build,
        )?;
        let (units, ais, spell) = factory.creature_summoned_ai_owners(&event)?;
        state.construct_summoned_ai(context.objects, units, ais, event, spell)?;
        return Ok(());
    }
    if selected.executable().builtin_kind() != Some(BuiltinCreatureAi::Event) {
        return Err(selected.constructor_refusal(context.objects)?);
    }
    state.construct_event_ai(
        context.objects,
        factory.creature_event_ai_definitions()?,
        factory.creature_event_ai_build()?,
        factory.creature_base_ai_instances()?,
    )
}

/// Creature.cpp:880-887 calls selection after motion initialization, before AI callbacks.
pub fn select_source_creature_ai<D, P, F>(
    factory: &mut F,
    state: &mut CreatureConstruction,
    context: &mut GridFactoryContext<'_, D, P>,
) -> Result<()>
where
    D: GridDefinitions,
    P: GridProductOwner,
    F: CreatureFactoryOwner<D, P>,
{
    require(state.selected_ai.is_none(), "execution.ai.already_selected")?;
    state.selected_ai = Some(factory.creature_select_ai(state, context.objects)?);
    factory.creature_ai_construct(state, context)
}

/// Re-add a retained construction without LoadFromDB or AI initialization; Map.cpp:471-504, Creature.cpp:173-209.
pub fn add_source_creature<D, P, F>(
    factory: &mut F,
    state: &mut CreatureConstruction,
    context: &mut GridFactoryContext<'_, D, P>,
) -> Result<()>
where
    D: GridDefinitions,
    P: GridProductOwner,
    F: CreatureFactoryOwner<D, P>,
{
    let object = state.object;
    state.position = context.objects.pose(object)?.0.map(f32::from_bits);
    let cell = compute_cell_pair(state.position[0].to_bits(), state.position[1].to_bits())
        .map_err(|_| refuse("execution.creature.map_add_coordinates"))?;
    if context.objects.get(object)?.active {
        context.load_grid(cell, GridLoadTrigger::Enter, factory)?;
    } else {
        context.load_grid(cell, GridLoadTrigger::TerrainOnly, factory)?;
    }
    let cell = context.cells.cell(cell)?;
    context.objects.link_cell(object, cell, context.cells)?;
    factory.observe_creature_factory(CreatureFactoryObservation::MapGridLinked);
    context.objects.register_world_lookup(object)?;
    factory.observe_creature_factory(CreatureFactoryObservation::WorldLookupRegistered);
    context.objects.mark_in_world(object)?;
    factory.observe_creature_factory(CreatureFactoryObservation::BaseAddedToWorld);
    let delay = source_add_to_world_notify_delay(
        context.objects.get(object)?.kind,
        factory.creature_units().get(context.objects, object)?,
        || factory.creature_respawn_aggro_config(),
    )?;
    factory
        .creature_units_mut()
        .get_mut(context.objects, object)?
        .unit_events
        .schedule_ai_notify(delay, false)?;
    factory.observe_creature_factory(CreatureFactoryObservation::AiNotifyEnqueued(delay));
    factory.creature_phase(CreaturePhase::MapAddToWorld, state, context)?;
    if factory.creature_force_load_map()? || state.template.active {
        context.set_active_object_state(object, true, factory)?;
    }
    factory.observe_creature_factory(CreatureFactoryObservation::ForceActiveChecked);
    if factory
        .creature_units()
        .get(context.objects, object)?
        .loaded_creature
        .count_spawns
    {
        factory.creature_phase(CreaturePhase::AddToSpawnCount, state, context)?;
    }
    factory.observe_creature_factory(CreatureFactoryObservation::SpawnCountChecked);
    if context.objects.get(object)?.active {
        context.add_to_active(object, factory)?;
    }
    factory.observe_creature_factory(CreatureFactoryObservation::MapActiveChecked);
    factory.creature_phase(CreaturePhase::MapAddViewpoint, state, context)?;
    factory.creature_phase(CreaturePhase::MapAddVisibility, state, context)
}
fn valid_position(position: [f32; 4]) -> bool {
    // Position validity has a half-yard margin, unlike cell conversion; GridDefines.h:177-194.
    const LIMIT: f64 = (533.333_3_f32 * 64.0 / 2.0) as f64 - 0.5;
    position.iter().all(|v| v.is_finite())
        && f64::from(position[0].abs()) <= LIMIT
        && f64::from(position[1].abs()) <= LIMIT
}
/// Creature.h:646 short-circuits the movement flags if the creature can inhabit air.
fn loaded_creature_can_fly(live: &SourceUnitFields) -> Result<bool> {
    use benilla_sim::creature_capabilities::{
        MOVEFLAG_CAN_FLY, MOVEFLAG_HOVER, MOVEFLAG_LEVITATING,
    };
    if live.can_fly {
        return Ok(true);
    }
    let flags = live
        .movement
        .as_ref()
        .and_then(|movement| movement.flags)
        .ok_or_else(|| refuse("execution.creature.Creature_1706.CanFly_movement_flags_owner"))?;
    Ok(flags & (MOVEFLAG_LEVITATING | MOVEFLAG_HOVER | MOVEFLAG_CAN_FLY) != 0)
}
fn dead<D, P, F>(
    factory: &mut F,
    state: &mut CreatureConstruction,
    context: &mut GridFactoryContext<'_, D, P>,
) -> Result<()>
where
    D: GridDefinitions,
    P: GridProductOwner,
    F: CreatureFactoryOwner<D, P>,
{
    context.objects.mark_dead(state.object)?;
    factory.creature_phase(CreaturePhase::SetDeadAndZeroHealth, state, context)?;
    if loaded_creature_can_fly(
        factory
            .creature_units()
            .get(context.objects, state.object)?,
    )? {
        let height = factory.creature_dead_flying_height(state, context)?;
        let [x, y, z, orientation] = state.spawn.position;
        if f64::from(z - height) > 0.1 {
            state.position = [x, y, height, orientation];
            context
                .objects
                .set_pose(state.object, SourcePose(state.position.map(f32::to_bits)))?;
        }
    }
    Ok(())
}

/// Resolve arithmetic from consumed rolls at SelectLevelStats; Creature.cpp:523,1273-1279.
pub fn resolve_supported_creature_kernel(
    input: &benilla_sim::creature_init::Input,
    state: &CreatureConstruction,
) -> Result<benilla_sim::creature_init::InitializedCreature> {
    use benilla_sim::creature_init as init;
    require(
        state.event.is_none() && state.group.is_none() && !state.template.can_fly,
        "execution.creature.kernel_domain_owner",
    )?;
    require(
        input.spawn_id == u64::from(state.spawn.database_guid)
            && input.template.entry == state.template.entry
            && input.template.displays == state.template.displays
            && [
                u32::from(input.template.minimum_level),
                u32::from(input.template.maximum_level),
            ] == state.template.levels,
        "execution.creature.kernel_operands",
    )?;
    let init::Rows::Complete(spawns) = &input.spawn_template else {
        return Err(refuse("execution.creature.kernel_spawn_template_owner"));
    };
    require(
        spawns.len() == 1 && spawns[0].model_id == state.spawn.model,
        "execution.creature.kernel_spawn_template_owner",
    )?;
    let value = init::prepare(input)
        .and_then(|plan| plan.resolve(state.rolls))
        .map_err(|error| Refusal {
            id: format!("execution.creature.kernel_owner:{error}"),
        })?;
    let model = state
        .model
        .ok_or_else(|| refuse("execution.creature.kernel_model_prefix"))?;
    require(
        value.model.display_id == model.id
            && value.model.gender == model.gender
            && u32::from(value.level) == state.level,
        "execution.creature.kernel_model_prefix",
    )?;
    Ok(value)
}

/// Read the current entry field, not the entry bits retained in a runtime GUID.
pub fn lookup_source_creature_linking(
    links: &SourceCreatureLinking,
    objects: &SourceObjects,
    object: ObjectHandle,
    entry: u32,
) -> Result<Option<LinkingInfo>> {
    links.ensure_loaded()?;
    let value = objects.get(object)?;
    let map = value
        .map
        .ok_or_else(|| refuse("execution.linking.object_map_kind"))?;
    require(
        matches!(
            value.kind,
            SourceObjectKind::Creature | SourceObjectKind::Pet
        ),
        "execution.linking.object_map_kind",
    )?;
    Ok(links.lookup(entry, value.database_guid, map.map))
}

/// Creature.cpp:923-929. The phase owner sets its trigger flag on a true result.
pub fn register_source_creature_linking(
    links: &SourceCreatureLinking,
    holder: &mut SourceCreatureLinkingHolder,
    objects: &SourceObjects,
    object: ObjectHandle,
    entry: u32,
) -> Result<bool> {
    register_source_creature_linking_inner(links, holder, objects, object, entry, |_| {}, || {})
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreatureCreateObservation {
    Battleground(bool),
    Zone([u32; 2]),
    OutdoorScriptLookup,
    InstanceDataLookup,
    InstanceScriptCalled,
}

/// Creature.cpp:911-921 queries outdoor PvP only on the non-battleground path.
#[allow(clippy::too_many_arguments)]
pub fn notify_loaded_creature_create(
    objects: &SourceObjects,
    units: &SourceUnits,
    object: ObjectHandle,
    map: PersistentMapEntry,
    outdoor: &SourceOutdoorPvp,
    instance: Option<&mut SourceWorldScript>,
    zone_and_area: impl FnOnce([f32; 3]) -> Result<[u32; 2]>,
    mut observe: impl FnMut(CreatureCreateObservation),
) -> Result<()> {
    let battleground = map.battleground();
    observe(CreatureCreateObservation::Battleground(battleground));
    if battleground {
        return Err(refuse(
            "execution.creature.loaded_battleground_create_owner",
        ));
    }
    let value = objects.get(object)?;
    require(
        value.map.is_some_and(|identity| identity.map == map.id),
        "execution.creature.map_binding",
    )?;
    let [x, y, z, _] = objects.pose(object)?.0.map(f32::from_bits);
    let location = zone_and_area([x, y, z])?;
    observe(CreatureCreateObservation::Zone(location));
    observe(CreatureCreateObservation::OutdoorScriptLookup);
    if outdoor.get_script(location[0])?.is_some() {
        return Err(refuse("execution.creature.loaded_outdoor_pvp_create_owner"));
    }
    observe(CreatureCreateObservation::InstanceDataLookup);
    if let Some(script) = instance {
        observe(CreatureCreateObservation::InstanceScriptCalled);
        let guid = value
            .guid
            .ok_or_else(|| refuse("execution.object_guid_absent"))?;
        script.creature_create(units.get(objects, object)?.entry, guid.raw());
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreatureLinkingRegistrationObservation {
    SlaveLookup,
    TriggerLookup,
}

/// Creature.cpp:923-929 sets the Unit flag before calling AddMasterToHolder.
pub fn register_loaded_creature_linking(
    state: &mut CreatureConstruction,
    links: &SourceCreatureLinking,
    holder: &mut SourceCreatureLinkingHolder,
    objects: &SourceObjects,
    units: &mut SourceUnits,
    observe: impl FnMut(CreatureLinkingRegistrationObservation),
) -> Result<()> {
    let live = units.get_mut(objects, state.object)?;
    register_source_creature_linking_inner(
        links,
        holder,
        objects,
        state.object,
        live.entry,
        observe,
        || {
            live.linking_event_trigger = Some(true);
            state.linking_trigger = true;
        },
    )?;
    Ok(())
}

fn register_source_creature_linking_inner(
    links: &SourceCreatureLinking,
    holder: &mut SourceCreatureLinkingHolder,
    objects: &SourceObjects,
    object: ObjectHandle,
    entry: u32,
    mut observe: impl FnMut(CreatureLinkingRegistrationObservation),
    set_trigger: impl FnOnce(),
) -> Result<bool> {
    observe(CreatureLinkingRegistrationObservation::SlaveLookup);
    let info = lookup_source_creature_linking(links, objects, object, entry)?;
    let value = objects.get(object)?;
    require(
        value.map == Some(holder.map()),
        "execution.linking.object_map_kind",
    )?;
    if info.is_some() {
        holder.add_slave(links, objects, object, entry)?;
    }
    observe(CreatureLinkingRegistrationObservation::TriggerLookup);
    let trigger = links.event_trigger(entry, value.database_guid, holder.map().map);
    if trigger {
        set_trigger();
        holder.add_master(links, objects, object, entry)?;
    }
    Ok(trigger)
}

/// Creature.cpp:908-909 and :123-134 relocate before checking validity.
pub fn relocate_source_creature_for_create(
    objects: &mut SourceObjects,
    object: ObjectHandle,
    position: [f32; 4],
) -> Result<bool> {
    objects.set_pose(object, SourcePose(position.map(f32::to_bits)))?;
    Ok(valid_position(position))
}

/// CreatureLinkingMgr.cpp:294-298 queries the live effective entry again at Creature.cpp:1720.
pub fn source_creature_link_info(
    links: &SourceCreatureLinking,
    objects: &SourceObjects,
    units: &SourceUnits,
    object: ObjectHandle,
) -> Result<Option<LinkingInfo>> {
    lookup_source_creature_linking(links, objects, object, units.get(objects, object)?.entry)
}
