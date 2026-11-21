// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::{objects::*, *};
use std::collections::BTreeMap;

#[path = "player_create_rows.rs"]
mod player_rows;
use player_rows::{playable_class, playable_race, PlayerCreateRace};
pub use player_rows::{PlayerCreateAction, PlayerCreateItem, PlayerCreateRows, PlayerCreateStart};

#[path = "player_skills.rs"]
mod player_skills;
pub use player_skills::{PlayerCreateSkill, SourcePlayerSkill};

#[path = "player_spellbook.rs"]
mod player_spellbook;
use player_spellbook::PlayerLearning;
pub use player_spellbook::{
    SourcePlayerAction, SourcePlayerActionState, SourcePlayerSpell, SourcePlayerSpellState,
};
#[path = "player_cast.rs"]
mod player_cast;
pub(super) use player_cast::{spell_mod_player, MOD_JUMP_TARGETS, MOD_RADIUS};
pub use player_cast::{SourcePlayerAuraClock, SourcePlayerCastInputs};
#[path = "player_defense.rs"]
mod player_defense;

// UpdateFields.h:27,77-88,114-120,133,140-151,180-186,215,218,228,243,263.
const OBJECT_FIELD_TYPE: usize = 0x02;
const UNIT_FIELD_POWER1: usize = 0x17;
const UNIT_FIELD_MAXHEALTH: usize = 0x1c;
const UNIT_FIELD_MAXPOWER1: usize = 0x1d;
const UNIT_FIELD_LEVEL: usize = 0x22;
const UNIT_FIELD_BASEATTACKTIME: usize = 0x7e;
const UNIT_FIELD_BOUNDINGRADIUS: usize = 0x81;
const UNIT_FIELD_COMBATREACH: usize = 0x82;
const UNIT_FIELD_DISPLAYID: usize = 0x83;
const UNIT_FIELD_NATIVEDISPLAYID: usize = 0x84;
const UNIT_MOD_CAST_SPEED: usize = 0x91;
const UNIT_FIELD_STAT2: usize = 0x98;
const UNIT_FIELD_BASE_MANA: usize = 0xa2;
const UNIT_FIELD_BASE_HEALTH: usize = 0xa3;
const PLAYER_GUILDID: usize = 0xbf;
const PLAYER_GUILDRANK: usize = 0xc0;
const PLAYER_BYTES: usize = 0xc1;
const PLAYER_BYTES_2: usize = 0xc2;
const PLAYER_BYTES_3: usize = 0xc3;
const PLAYER_GUILD_TIMESTAMP: usize = 0xc5;
const PLAYER_NEXT_LEVEL_XP: usize = 0x2cd;
const PLAYER_CHARACTER_POINTS2: usize = 0x44f;
const PLAYER_FIELD_COINAGE: usize = 0x498;
const PLAYER_FIELD_POSSTAT0: usize = 0x499;
const PLAYER_FIELD_NEGSTAT0: usize = 0x49e;
const PLAYER_FIELD_MOD_DAMAGE_DONE_PCT: usize = 0x4bf;
const PLAYER_FIELD_WATCHED_FACTION_INDEX: usize = 0x4ed;

/// CharacterHandler owns name and admission policy before constructing a Player.
pub struct SourcePlayerCreation {
    pub guid_low: u32,
    pub name: String,
    pub race: u8,
    pub class: u8,
    pub gender: u8,
    /// Skin, face, hair style, hair color and facial hair in packet order.
    pub appearance: [u8; 5],
}

impl SourcePlayerCreation {
    /// Check the captured appearance before consuming the character GUID or constructor inputs.
    pub fn check_appearance(race: u8, class: u8, gender: u8, appearance: [u8; 5]) -> Result<()> {
        require(
            (race, class, gender, appearance) == (1, 8, 0, [0; 5]),
            "execution.player.create_appearance_domain",
        )
    }
}

/// Settings read for normal-security, level-one creation.
#[derive(Clone, Copy)]
pub struct SourcePlayerCreateConfig {
    pub save_interval: u32,
    pub start_level: u32,
    pub start_money: u32,
    pub max_primary_professions: u16,
}

/// Player fields for one live object handle. Object and shared Unit fields stay in their registries.
/// Creation is detached. A refusal leaves no accessor, map-player list, or grid registration.
pub struct SourcePlayer {
    object: ObjectHandle,
    object_type: u32,
    pub name: String,
    pub extra_flags: u32,
    pub next_save_ms: u32,
    pub login_time: i64,
    pub last_tick: i64,
    pub played_time: [u32; 2],
    pub items: [Option<ObjectHandle>; 118],
    pub race: u8,
    pub gender: u8,
    pub team: u32,
    pub spell_family: u32,
    pub native_display: u32,
    pub display: u32,
    pub scale: f32,
    pub base_walk_speed: f32,
    pub base_run_speed: f32,
    pub pet_loyalty: u8,
    pub misc_flags: u8,
    pub bytes: [u8; 4],
    pub bytes_2: [u8; 4],
    pub bytes_3: u32,
    pub guild_id: u32,
    pub guild_rank: u32,
    pub guild_timestamp: u32,
    pub watched_faction: i32,
    pub money: u32,
    pub create_stats: [u8; 5],
    /// Current stamina, intellect and spirit. Strength and agility have shared Unit owners.
    pub mental_and_stamina: [u32; 3],
    pub next_level_xp: u32,
    pub attack_times: [f32; 3],
    pub damage_done_percent: [f32; 7],
    // Object.cpp:105-108 zeros the positive and negative stat update words.
    positive_stats: [f32; 5],
    negative_stats: [f32; 5],
    // Object.cpp:105-108 and Player.cpp:2883-2887 start the display words at zero.
    block_percentage: f32,
    parry_percentage: f32,
    dodge_percentage: f32,
    // Player.cpp:579-580; Player.h:2420-2421.
    weapon_proficiency: u32,
    armor_proficiency: u32,
    pub taxi_mask: [u32; 8],
    pub free_primary_professions: u32,
    skills: Vec<SourcePlayerSkill>,
    // Player.h:2377,2379; Player.cpp:471. Both maps default to empty.
    spells: BTreeMap<u32, SourcePlayerSpell>,
    action_buttons: BTreeMap<u8, SourcePlayerAction>,
    // Player.h:2385. Default construction leaves every operation's list empty.
    spell_mods: BTreeMap<u32, Vec<player_cast::PlayerSpellModifier>>,
    // Player.h:2556-2557; Player.cpp:640-641. One identity names the in-flight Spell and its charges.
    active_mod_spell: Option<u64>,
}

impl SourcePlayer {
    pub fn object(&self) -> ObjectHandle {
        self.object
    }

    /// Player.cpp:471-649. Callers supply the actual session thread's draw and wall clock.
    pub fn construct(
        objects: &mut SourceObjects,
        units: &mut SourceUnits,
        thread: u64,
        save_interval: u32,
        mut urand: impl FnMut(u32, u32) -> Result<u32>,
        mut wall_seconds: impl FnMut() -> Result<i64>,
    ) -> Result<Self> {
        let object = objects.construct(thread, SourceObjectKind::Player)?;
        let result = (|| {
            let mut fields = SourceUnitFields::constructed_creature();
            // The Player constructor has no Creature respawn, alert or detector defaults.
            fields.can_aggro = None;
            fields.respawn_delay = 0;
            fields.loaded_creature = Default::default();
            // Player.cpp:581-582 overrides the Unit defaults.
            fields.can_parry = false;
            fields.can_block = false;
            units.register(objects, object, fields)?;

            // uint32 multiplication wraps before division at Player.cpp:515.
            let low = save_interval / 2;
            let high = save_interval.wrapping_mul(3) / 2;
            require(
                low <= high,
                "execution.player.Player_515.save_interval_bounds",
            )?;
            let next_save_ms = urand(low, high)?;
            let login_time = wall_seconds()?;
            // Object.cpp:105-108 clears update fields on _Create. These are the zero words.
            Ok(Self {
                object,
                object_type: 0,
                name: String::new(),
                extra_flags: 4, // Player.cpp:490-496; normal security accepts whispers.
                next_save_ms,
                login_time,
                last_tick: login_time,
                played_time: [0; 2],
                items: [None; 118], // Player.cpp:521; Player.h:550-553.
                race: 0,
                gender: 0,
                team: 0,
                spell_family: 0,
                native_display: 0,
                display: 0,
                scale: 0.0,
                base_walk_speed: 1.0, // Unit.cpp:429-430.
                base_run_speed: 1.0,
                pet_loyalty: 0,
                misc_flags: 0,
                bytes: [0; 4],
                bytes_2: [0; 4],
                bytes_3: 0,
                guild_id: 0,
                guild_rank: 0,
                guild_timestamp: 0,
                watched_faction: 0,
                money: 0,
                create_stats: [0; 5],
                mental_and_stamina: [0; 3],
                next_level_xp: 0,
                attack_times: [0.0; 3],
                damage_done_percent: [0.0; 7],
                positive_stats: [0.0; 5],
                negative_stats: [0.0; 5],
                block_percentage: 0.0,
                parry_percentage: 0.0,
                dodge_percentage: 0.0,
                weapon_proficiency: 0,
                armor_proficiency: 0,
                taxi_mask: [0; 8],
                free_primary_professions: 0,
                skills: Vec::new(),
                spells: BTreeMap::new(),
                action_buttons: BTreeMap::new(),
                spell_mods: BTreeMap::new(),
                active_mod_spell: None,
            })
        })();
        if result.is_err() {
            objects.destroy_unbound(object)?;
            units.discard_construction(object);
        }
        result
    }

    /// Player.cpp:747-945. The caller owns destruction if this partial port refuses.
    #[allow(clippy::too_many_arguments)]
    pub fn create<'a>(
        &mut self,
        objects: &'a mut SourceObjects,
        units: &'a mut SourceUnits,
        request: SourcePlayerCreation,
        rows: &'a PlayerCreateRows,
        spells: &'a SourceSpellStores,
        items: &'a SourceStartupItems,
        models: &LoadedCreatureModels,
        cells: &SourceCells,
        config: SourcePlayerCreateConfig,
        cast: Option<SourcePlayerCastInputs<'a>>,
        mut wall_seconds: impl FnMut() -> Result<i64>,
    ) -> Result<()> {
        let object = self.object;
        require(
            objects.get(object)?.guid.is_none(),
            "execution.player.create_already_started",
        )?;
        // Object.cpp:115-125. The player high type and zero entry preserve the raw low GUID.
        objects.assign_guid(
            object,
            request.guid_low,
            SourceGuid::with_entry(HighGuid::Player, 0, request.guid_low),
        )?;
        objects.set_update_value(object, OBJECT_FIELD_TYPE, &mut self.object_type, 0x19)?;
        self.name = request.name;
        let SourcePlayerCreation {
            race,
            class,
            gender,
            appearance,
            ..
        } = request;
        require(
            playable_race(race) && rows.races.contains_key(&race),
            "execution.player.Player_755.race",
        )?;
        require(playable_class(class), "execution.player.Player_755.class")?;
        let start = rows
            .start(race, class)
            .ok_or_else(|| refuse("execution.player.Player_755.playercreateinfo"))?;
        let &power_type = rows
            .classes
            .get(&class)
            .ok_or_else(|| refuse("execution.player.Player_762.class_dbc"))?;
        require(gender < 2, "execution.player.Player_770.gender")?;
        SourcePlayerCreation::check_appearance(race, class, gender, appearance)?;
        require(
            power_type == 0,
            "execution.player.Player_784.power_type_domain",
        )?;

        self.items.fill(None);
        objects.set_pose(object, start.pose)?;
        // MapManager.cpp:130-146 reuses a registered continent. A missing map needs its owner.
        let map = cells.map();
        require(
            map.map == start.map && map.instance == 0,
            "execution.player.Player_782.create_map_owner",
        )?;
        objects.bind_map(object, map, cells)?;
        let unit = units.get_mut(objects, object)?;
        let race_row = rows
            .races
            .get(&race)
            .ok_or_else(|| refuse("execution.player.Player_786.race_dbc"))?;
        self.set_faction_for_race(objects, unit, race_row)?;
        objects.set_update_value(object, UNIT_FIELD_BYTES_0, &mut self.race, race)?;
        objects.set_update_value(
            object,
            UNIT_FIELD_BYTES_0,
            &mut unit.unit_class,
            Some(class.into()),
        )?;
        self.spell_family = 3; // Player.cpp:16957; DBCEnums.h:162.
        objects.set_update_value(object, UNIT_FIELD_BYTES_0, &mut self.gender, gender)?;
        objects.set_update_value(
            object,
            UNIT_FIELD_BYTES_0,
            &mut unit.power_type,
            Some(power_type),
        )?;
        self.init_display_ids(objects, unit, race_row, rows, models)?;

        objects.set_update_value(object, UNIT_FIELD_BYTES_1, &mut self.pet_loyalty, 0xee)?;
        objects.set_update_value(object, UNIT_FIELD_BYTES_2, &mut self.misc_flags, 0x28)?;
        objects.set_update_value(object, UNIT_FIELD_FLAGS, &mut unit.flags, 8)?;
        objects.set_update_value(
            object,
            UNIT_MOD_CAST_SPEED,
            &mut unit.cast_speed,
            1f32.to_bits(),
        )?;
        objects.set_update_value(
            object,
            PLAYER_FIELD_WATCHED_FACTION_INDEX,
            &mut self.watched_faction,
            -1,
        )?;
        for (dst, byte) in self.bytes.iter_mut().zip(appearance) {
            objects.set_update_value(object, PLAYER_BYTES, dst, byte)?;
        }
        objects.set_update_value(object, PLAYER_BYTES_2, &mut self.bytes_2[0], appearance[4])?;
        objects.set_update_value(object, PLAYER_BYTES_2, &mut self.bytes_2[3], 2)?;
        let gender_word = (self.bytes_3 & 0xffff_0000) | u32::from(gender);
        objects.set_update_value(object, PLAYER_BYTES_3, &mut self.bytes_3, gender_word)?;
        objects.set_update_value(
            object,
            PLAYER_BYTES_3,
            &mut self.bytes_3,
            gender_word & 0x00ff_ffff,
        )?;
        objects.set_update_value(object, PLAYER_GUILDID, &mut self.guild_id, 0)?;
        objects.set_update_value(object, PLAYER_GUILDRANK, &mut self.guild_rank, 0)?;
        objects.set_update_value(object, PLAYER_GUILD_TIMESTAMP, &mut self.guild_timestamp, 0)?;
        let power = unit.power_fields.insert(SourceUnitPowerFields {
            level: 0,
            create_health: 0,
            create_mana: 0,
            auxiliary_current: Some([0; 3]),
            auxiliary_maximum: Some([0; 3]),
        });
        require(
            config.start_level == 1,
            "execution.player.Player_825.start_level_domain",
        )?;
        objects.set_update_value(
            object,
            UNIT_FIELD_LEVEL,
            &mut power.level,
            config.start_level,
        )?;
        objects.set_update_value(
            object,
            PLAYER_FIELD_COINAGE,
            &mut self.money,
            config.start_money,
        )?;
        self.last_tick = wall_seconds()?;
        self.played_time = [0; 2];

        self.init_stats_for_level(objects, unit, rows, class)?;
        self.init_taxi_nodes(race_row);
        // Player.cpp:2775-2784 leaves a fresh level-one player's zero talent points alone.
        self.init_primary_professions(objects, config.max_primary_professions)?;
        self.fill_health_and_mana(objects, unit)?;

        // Player.cpp:850-855 grants skills, spells, then buttons.
        let skills = rows
            .skill_inputs
            .as_ref()
            .ok_or_else(|| refuse("execution.player.Player_850.learn_default_skills_owner"))?;
        let mut live = PlayerLearning {
            objects,
            units,
            cast,
            skills,
            spells,
            items,
            class,
        };
        self.learn_default_skills(&mut live, rows.skills(race, class))?;
        self.learn_default_spells(&mut live, rows.spells(race, class))?;
        for action in rows.actions(race, class) {
            self.add_action_button(&live, *action)?;
        }
        Err(refuse("execution.player.Player_861.outfit_owner"))
    }

    /// CharacterHandler.cpp:498-500 deletes the detached Player when Create refuses.
    pub fn discard(
        self,
        objects: &mut SourceObjects,
        units: &mut SourceUnits,
        cells: &mut SourceCells,
    ) -> Result<()> {
        objects.destroy(self.object, cells)?;
        units.discard_construction(self.object);
        Ok(())
    }

    // Player.cpp:6260-6294. DBC uses base language codes for team identity.
    fn set_faction_for_race(
        &mut self,
        objects: &SourceObjects,
        unit: &mut SourceUnitFields,
        race: &PlayerCreateRace,
    ) -> Result<()> {
        self.team = match race.team {
            7 => 469,
            1 => 67,
            _ => 0,
        };
        unit.set_faction_template(objects, self.object, race.faction)
    }

    // Player.cpp:17491-17527 and Unit.cpp:10199-10206,10260-10275.
    fn init_display_ids(
        &mut self,
        objects: &SourceObjects,
        unit: &mut SourceUnitFields,
        race: &PlayerCreateRace,
        rows: &PlayerCreateRows,
        models: &LoadedCreatureModels,
    ) -> Result<()> {
        let id = race.display[usize::from(self.gender)];
        let &[display_scale, model_scale] = rows
            .display_scales
            .get(&id)
            .ok_or_else(|| refuse("execution.player.Player_17521.display_scale_owner"))?;
        let scale = display_scale * model_scale;
        let object = self.object;
        objects.set_update_value(object, OBJECT_FIELD_SCALE_X, &mut self.scale, scale)?;
        objects.set_update_value(
            object,
            UNIT_FIELD_NATIVEDISPLAYID,
            &mut self.native_display,
            id,
        )?;
        objects.set_update_value(object, UNIT_FIELD_DISPLAYID, &mut self.display, id)?;
        if let Some(model) = models.get(id) {
            let normalized = if scale != 0.0 {
                self.scale / scale
            } else {
                1.0
            };
            let radius = normalized * f32::from_bits(model.words[1]);
            let reach = normalized * f32::from_bits(model.words[2]);
            objects.set_update_value(
                object,
                UNIT_FIELD_BOUNDINGRADIUS,
                &mut unit.bounding_radius,
                Some(radius.to_bits()),
            )?;
            objects.set_update_value(
                object,
                UNIT_FIELD_COMBATREACH,
                &mut unit.combat_reach,
                Some(reach.to_bits()),
            )?;
            self.base_walk_speed = f32::from_bits(model.words[3]);
            self.base_run_speed = f32::from_bits(model.words[4]);
        }
        Ok(())
    }

    // Player.cpp:2808-2952, fresh level one with no auras, skills, equipment or group.
    fn init_stats_for_level(
        &mut self,
        objects: &SourceObjects,
        unit: &mut SourceUnitFields,
        rows: &PlayerCreateRows,
        class: u8,
    ) -> Result<()> {
        let object = self.object;
        let &[create_health, create_mana] = rows
            .class_levels
            .get(&(class, 1))
            .ok_or_else(|| refuse("execution.player.Player_2817.player_classlevelstats"))?;
        let &stats = rows
            .levels
            .get(&(self.race, class, 1))
            .ok_or_else(|| refuse("execution.player.Player_2820.player_levelstats"))?;
        let &xp = rows
            .xp
            .get(&1)
            .ok_or_else(|| refuse("execution.player.Player_2822.player_xp_for_level"))?;
        objects.set_update_value(object, PLAYER_NEXT_LEVEL_XP, &mut self.next_level_xp, xp)?;
        // Player.cpp:2824-2827 initializes an already clear aura state and visits no skills.
        objects.set_update_value(
            object,
            UNIT_MOD_CAST_SPEED,
            &mut unit.cast_speed,
            1f32.to_bits(),
        )?;
        self.create_stats = stats;
        unit.creature_damage
            .set_strength(objects, object, i32::from(stats[0]))?;
        unit.resistances
            .set_agility(objects, object, i32::from(stats[1]))?;
        for (i, stat) in self.mental_and_stamina.iter_mut().enumerate() {
            objects.set_update_value(
                object,
                UNIT_FIELD_STAT2 + i,
                stat,
                u32::from(stats[2 + i]),
            )?;
        }
        let power = unit
            .power_fields
            .as_mut()
            .ok_or_else(|| refuse("execution.player.Player_2815.level_owner"))?;
        objects.set_update_value(
            object,
            UNIT_FIELD_BASE_HEALTH,
            &mut power.create_health,
            u32::from(create_health),
        )?;
        objects.set_update_value(
            object,
            UNIT_FIELD_BASE_MANA,
            &mut power.create_mana,
            u32::from(create_mana),
        )?;
        unit.resistances.update_armor(objects, object)?;
        for (i, percent) in self.damage_done_percent.iter_mut().enumerate() {
            objects.set_update_value(object, PLAYER_FIELD_MOD_DAMAGE_DONE_PCT + i, percent, 1.0)?;
        }
        for (i, time) in self.attack_times.iter_mut().enumerate() {
            objects.set_update_value(object, UNIT_FIELD_BASEATTACKTIME + i, time, 2000.0)?;
        }
        // Player.cpp:2861-2887 resets the still-zero damage, AP and crit words.
        unit.resistances.update_armor(objects, object)?;

        // Player.cpp:17446-17488 stays in form zero. No weapons, items or item sets to update.
        let attack = PlayerAttackPowerInputs {
            class: class.into(),
            level: power.level,
            agility: unit.resistances.agility(),
            form: unit.shapeshift_form,
            form_agility_bonus: None,
            attack_times: self.attack_times,
            unarmed: true,
        };
        unit.creature_damage
            .update_player_attack_power_and_damage(objects, object, attack, false)?;
        unit.creature_damage
            .update_player_attack_power_and_damage(objects, object, attack, true)?;

        let vitals = unit.vitals.insert(SourceUnitVitals {
            health: 0,
            max_health: 0,
            mana: 0,
            max_mana: 0,
            has_mana: true,
            energy: 0,
            max_energy: 0,
        });
        let auxiliary = power
            .auxiliary_maximum
            .as_mut()
            .ok_or_else(|| refuse("execution.player.Player_2913.power_owner"))?;
        objects.set_update_value(
            object,
            UNIT_FIELD_MAXPOWER1,
            &mut vitals.max_mana,
            power.create_mana,
        )?;
        objects.set_update_value(object, UNIT_FIELD_MAXPOWER1 + 1, &mut auxiliary[0], 1000)?;
        objects.set_update_value(object, UNIT_FIELD_MAXPOWER1 + 2, &mut auxiliary[1], 0)?;
        objects.set_update_value(
            object,
            UNIT_FIELD_MAXPOWER1 + 3,
            &mut vitals.max_energy,
            100,
        )?;
        objects.set_update_value(object, UNIT_FIELD_MAXPOWER1 + 4, &mut auxiliary[2], 0)?;
        objects.set_update_value(
            object,
            UNIT_FIELD_MAXHEALTH,
            &mut vitals.max_health,
            power.create_health,
        )?;
        // Player.cpp:2917-2938 clears already-zero flags and preserves player control.
        let (health, mana, energy) = (vitals.max_health, vitals.max_mana, vitals.max_energy);
        objects.set_update_value(object, UNIT_FIELD_HEALTH, &mut vitals.health, health)?;
        objects.set_update_value(object, UNIT_FIELD_POWER1, &mut vitals.mana, mana)?;
        objects.set_update_value(object, UNIT_FIELD_POWER1 + 3, &mut vitals.energy, energy)?;
        Ok(())
    }

    // Player.cpp:295-301. Only the first word receives the race's starting nodes.
    fn init_taxi_nodes(&mut self, race: &PlayerCreateRace) {
        self.taxi_mask.fill(0);
        self.taxi_mask[0] = race.taxi_mask;
    }

    // Player.cpp:18021-18025, normal-security branch.
    fn init_primary_professions(&mut self, objects: &SourceObjects, count: u16) -> Result<()> {
        objects.set_update_value(
            self.object,
            PLAYER_CHARACTER_POINTS2,
            &mut self.free_primary_professions,
            count.into(),
        )?;
        Ok(())
    }

    // Player.h:1824-1828 and Object.h:456-459.
    fn apply_stat_percent_buff_mod(
        &mut self,
        objects: &SourceObjects,
        stat: usize,
        amount: f32,
        apply: bool,
    ) -> Result<()> {
        require(
            stat < self.positive_stats.len(),
            "execution.player.Player_1824.stat_buff_index",
        )?;
        let amount = if amount == -100.0 { -99.9 } else { amount };
        let factor = if apply {
            (100.0 + amount) / 100.0
        } else {
            100.0 / (100.0 + amount)
        };
        let positive = &mut self.positive_stats[stat];
        let negative = &mut self.negative_stats[stat];
        objects.set_update_value(
            self.object,
            PLAYER_FIELD_POSSTAT0 + stat,
            positive,
            *positive * factor,
        )?;
        objects.set_update_value(
            self.object,
            PLAYER_FIELD_NEGSTAT0 + stat,
            negative,
            *negative * factor,
        )?;
        Ok(())
    }

    // StatSystem.cpp:144-200, with the constructor's neutral health and mana modifiers.
    fn fill_health_and_mana(
        &mut self,
        objects: &SourceObjects,
        unit: &mut SourceUnitFields,
    ) -> Result<()> {
        let power = unit
            .power_fields
            .as_ref()
            .ok_or_else(|| refuse("execution.player.Player_841.create_power_owner"))?;
        let vitals = unit
            .vitals
            .as_mut()
            .ok_or_else(|| refuse("execution.player.Player_841.vitals_owner"))?;
        let stamina = self.mental_and_stamina[0] as f32;
        let base = stamina.min(20.0);
        let bonus = base + (stamina - base) * 10.0;
        let health = (power.create_health as f32 + bonus).max(1.0).round() as u32;
        objects.set_update_value(
            self.object,
            UNIT_FIELD_MAXHEALTH,
            &mut vitals.max_health,
            health,
        )?;
        objects.set_update_value(self.object, UNIT_FIELD_HEALTH, &mut vitals.health, health)?;
        let intellect = self.mental_and_stamina[1] as f32;
        let base = intellect.min(20.0);
        let bonus = if power.create_mana > 0 {
            base + (intellect - base) * 15.0
        } else {
            0.0
        };
        let mana = (power.create_mana as f32 + bonus).round() as u32;
        objects.set_update_value(
            self.object,
            UNIT_FIELD_MAXPOWER1,
            &mut vitals.max_mana,
            mana,
        )?;
        objects.set_update_value(self.object, UNIT_FIELD_POWER1, &mut vitals.mana, mana)?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "player_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "player_spellbook_tests.rs"]
mod spellbook_tests;
