// SPDX-License-Identifier: GPL-3.0-or-later
//! Source-derived character/combat state. Raw-content admission remains separate.
use super::*;
use benilla_sim::{npc_melee as melee, starter_init as init};
use serde::{Deserialize, Serialize};

mod melee_timers;
pub use melee_timers::PlayerMeleeTimers;

pub const DEFENSE_SKILL: u16 = 95;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillState {
    pub id: u16,
    pub value: u16,
    pub maximum: u16,
    pub step: u16,
    pub permanent_bonus: i16,
    pub temporary_bonus: i16,
}
impl SkillState {
    pub fn current_value(&self) -> u16 {
        (i32::from(self.value) + i32::from(self.permanent_bonus) + i32::from(self.temporary_bonus))
            .max(0) as u16
    }
}
impl From<init::InitializedSkill> for SkillState {
    fn from(s: init::InitializedSkill) -> Self {
        Self {
            id: s.id,
            value: s.value,
            maximum: s.maximum,
            step: s.step,
            permanent_bonus: s.permanent_bonus,
            temporary_bonus: s.temporary_bonus,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterStats {
    pub attributes: [i32; 5],
    pub resistances: [i32; 7],
    pub max_health: u32,
    pub max_mana: u32,
    pub normal_regen: spell::SpellFloat,
    pub interrupted_regen: spell::SpellFloat,
    pub crit_percent: spell::SpellFloat,
    pub disabled_weapon_slots: Vec<u8>,
}
impl CharacterStats {
    pub(in crate::native) fn from_result(x: &init::EquipmentRecompute) -> Self {
        Self {
            attributes: x.effective_attributes.0,
            resistances: x.resistances,
            max_health: x.resources.max_health,
            max_mana: x.resources.max_mana,
            normal_regen: x.mana_regeneration.normal_per_second,
            interrupted_regen: x.mana_regeneration.interrupted_per_second,
            crit_percent: x.spell_crit_percent,
            disabled_weapon_slots: x.disabled_weapon_slots.iter().copied().collect(),
        }
    }
    pub fn regen(&self) -> mana::ManaRegen {
        mana::ManaRegen {
            normal_per_second: self.normal_regen,
            interrupted_per_second: self.interrupted_regen,
        }
    }
}
fn serialize_recipe<S: serde::Serializer>(r: &init::StatRecipe, s: S) -> Result<S::Ok, S::Error> {
    r.snapshot().serialize(s)
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(super) struct InitialWeapon {
    pub(in crate::native) slot: u8,
    pub(in crate::native) item_id: u32,
    pub(in crate::native) class: u32,
    pub(in crate::native) subclass: u32,
    pub(in crate::native) inventory_type: u32,
    pub(in crate::native) skill: u16,
    pub(in crate::native) delay_ms: u32,
    pub(in crate::native) damage: Vec<(u32, u32, u32)>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InitialAuraMetadata {
    pub spell_id: u32,
    pub effect_index: u8,
    pub aura_type: u32,
    pub misc_value: i32,
    pub amount: i32,
    pub attributes: u32,
    pub attributes_ex3: u32,
    pub caster_aura_state: u32,
    pub aura_interrupt_flags: u32,
}
impl InitialAuraMetadata {
    pub(super) fn from_source(aura: &init::InitialAura, raw: &init::SpellRow) -> Self {
        Self {
            spell_id: aura.spell_id,
            effect_index: aura.effect_index,
            aura_type: aura.aura_type,
            misc_value: aura.misc_value,
            amount: aura.amount,
            attributes: raw.words[6],
            attributes_ex3: raw.words[9],
            caster_aura_state: raw.words[16],
            aura_interrupt_flags: raw.words[22],
        }
    }
}
/// Only the source initializer can construct this value. It is not an admission token.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CombatGenesis {
    pub(super) input_identity: crate::Digest,
    pub(super) prepared: Option<init::PreparedStarter>,
    pub(super) initial_cast_source: Option<interactions::spell_authority::InitialCastSourceSeal>,
    pub(super) starter_identity: PlayerStarterIdentity,
    pub(super) stats: CharacterStats,
    pub(super) initial_health: u32,
    pub(super) initial_mana: u32,
    pub(super) admitted_abilities: Vec<Ability>,
    pub(super) skills: BTreeMap<u16, SkillState>,
    pub(super) equipment: equipment::EquipmentSnapshot,
    #[serde(serialize_with = "serialize_recipe")]
    pub(super) recipe: init::StatRecipe,
    pub(super) initial_auras: Vec<InitialAuraMetadata>,
    pub(super) initial_inventory: Vec<(u32, u32)>,
    pub(super) equipment_weapons: Vec<InitialWeapon>,
    pub(super) melee_timers: PlayerMeleeTimers,
    pub(super) proficiencies: (u32, u32),
    pub(super) durability_hit_chance: spell::SpellFloat,
    pub(super) defense_gain: u16,
    pub(super) damage_cancels: Vec<u32>,
    pub(super) death: Option<DeathPolicy>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureCombatProfile {
    pub minimum_damage: spell::SpellFloat,
    pub maximum_damage: spell::SpellFloat,
    pub no_crushing_blows: bool,
    pub creature_is_critter: bool,
}
impl CreatureCombatProfile {
    pub(super) fn attacker(&self, level: u8) -> melee::CreatureAttack {
        melee::CreatureAttack {
            level,
            minimum_damage: self.minimum_damage,
            maximum_damage: self.maximum_damage,
            no_crushing_blows: self.no_crushing_blows,
            creature_is_critter: self.creature_is_critter,
        }
    }
}
impl Profile {
    /// Reduce complete captured starter rows, including first-entry skill normalization.
    /// A future raw compiler must also certify capture completeness and callback exclusions.
    pub fn initialize_player(
        &mut self,
        input: &init::StarterInput,
        durability_hit_chance: spell::SpellFloat,
        defense_gain: u16,
    ) -> Result<(), NativeError> {
        if self.player_collision_height.is_some() {
            self.validate()?;
        }
        let mut next = self.clone();
        next.initialize_player_inner(input, None, durability_hit_chance, defense_gain)?;
        *self = next;
        Ok(())
    }
    /// Seedless draft projection. A complete source seal is mandatory before execution.
    pub fn initialize_player_prepared(
        &mut self,
        input: &init::StarterInput,
        prepared: &init::PreparedStarter,
        durability_hit_chance: spell::SpellFloat,
        defense_gain: u16,
    ) -> Result<(), NativeError> {
        interactions::spell_authority::InitialCastProof::check_bound(input)?;
        prepared.validate_against(input)?;
        let mut next = self.clone();
        next.initialize_player_inner(input, Some(prepared), durability_hit_chance, defense_gain)?;
        *self = next;
        Ok(())
    }

    pub(in crate::native) fn reinitialize_player(
        &mut self,
        input: &init::StarterInput,
        genesis: &CombatGenesis,
    ) -> Result<(), NativeError> {
        self.initialize_player_inner(
            input,
            genesis.prepared.as_ref(),
            genesis.durability_hit_chance,
            genesis.defense_gain,
        )?;
        self.combat
            .as_mut()
            .ok_or(NativeError::InvalidProfile)?
            .initial_cast_source = genesis.initial_cast_source.clone();
        Ok(())
    }
    fn initialize_player_inner(
        &mut self,
        input: &init::StarterInput,
        prepared: Option<&init::PreparedStarter>,
        durability_hit_chance: spell::SpellFloat,
        defense_gain: u16,
    ) -> Result<(), NativeError> {
        let retained_seal = self
            .combat
            .as_ref()
            .filter(|g| {
                g.input_identity == input.source_identity() && g.prepared.as_ref() == prepared
            })
            .and_then(|g| g.initial_cast_source.clone());
        let mage = match prepared {
            Some(prepared) => {
                interactions::spell_authority::InitialCastProof::check_bound(input)?;
                prepared.validate_against(input)?;
                prepared.created().clone()
            }
            None => init::initialize_mage(input)?,
        };
        let skills = match prepared {
            Some(prepared) => prepared.first_entry().skills.clone(),
            None => init::first_entry_skills(input, &mage)?,
        };
        if !mage.can_dodge || mage.can_parry || mage.can_block {
            return Err(NativeError::UnsupportedMechanic(
                "starter avoidance abilities",
            ));
        }
        // Validate rate via the source percent comparator, including zero and 100.
        if durability_hit_chance.bits() & 0x80000000 != 0
            || durability_hit_chance.bits() > 0x42c80000
        {
            return Err(NativeError::InvalidProfile);
        }
        let mappings = match &input.skill_abilities {
            init::Rows::Complete(rows) => rows,
            _ => return Err(NativeError::InvalidProfile),
        };
        let abilities = mana::SkillLineAbilities::new(
            mappings
                .iter()
                .map(|r| mana::SkillLineAbilityRow {
                    row_id: r.row_id,
                    spell_id: r.spell,
                    skill_id: u32::from(r.skill),
                })
                .collect(),
        )?;
        let current = mana::CurrentSkills::new(
            skills
                .values()
                .map(|s| mana::CurrentSkill {
                    id: s.id,
                    value: s.value,
                    permanent_bonus: s.permanent_bonus,
                    temporary_bonus: s.temporary_bonus,
                    deleted: false,
                })
                .collect(),
        )?;
        let mut active_count = 0;
        let mut damage_cancels = Vec::new();
        for known in mage.abilities.values().filter(|a| {
            a.active
                && matches!(
                    a.availability,
                    init::AbilityAvailability::Available
                        | init::AbilityAvailability::Unavailable(_)
                )
        }) {
            active_count += 1;
            let ability = self
                .ability(known.spell_id)
                .ok_or(NativeError::InvalidProfile)?;
            match (&known.availability, &ability.admission) {
                (init::AbilityAvailability::Available, AbilityAdmission::Available(spec)) => {
                    let raw = input
                        .spells
                        .iter()
                        .find(|s| s.id == known.spell_id)
                        .ok_or(NativeError::InvalidProfile)?;
                    if raw.words[21] & 0x10 != 0 {
                        damage_cancels.push(known.spell_id);
                    }
                    if spec.damage_pushback != (raw.words[21] & 2 != 0) {
                        return Err(NativeError::InvalidProfile);
                    }
                    if u32::from(spec.spell_rank)
                        != mana::spell_rank(known.spell_id, raw.words[27], &abilities, &current)?
                    {
                        return Err(NativeError::InvalidProfile);
                    }
                }
                (
                    init::AbilityAvailability::Unavailable(a),
                    AbilityAdmission::Unavailable { reason: b },
                ) if a == b => {}
                _ => return Err(NativeError::InvalidProfile),
            }
        }
        if active_count != self.abilities.len() {
            return Err(NativeError::InvalidProfile);
        }
        let stats = CharacterStats::from_result(&mage.stat_recipe.current(mage.resources)?);
        let equipment = equipment::EquipmentSnapshot {
            version: equipment::EQUIPMENT_FORMAT_VERSION,
            items: mage
                .equipped
                .values()
                .map(|i| equipment::EquippedItem {
                    origin: equipment::EquipmentOrigin {
                        genesis_slot: i.slot,
                        item_id: i.item,
                    },
                    slot: i.slot,
                    item_id: i.item,
                    max_durability: i.max_durability,
                    current_durability: i.durability,
                })
                .collect(),
        };
        equipment::EquipmentState::from_snapshot(equipment.clone())?;
        let equipment_weapons = mage
            .equipped
            .values()
            .map(|i| {
                let raw = input
                    .items
                    .iter()
                    .find(|r| r.id == i.item)
                    .ok_or(NativeError::InvalidProfile)?;
                Ok(InitialWeapon {
                    slot: i.slot,
                    item_id: i.item,
                    class: raw.class,
                    subclass: raw.subclass,
                    inventory_type: raw.inventory_type,
                    skill: i.weapon_skill,
                    delay_ms: i.delay_ms,
                    damage: i
                        .damage
                        .iter()
                        .map(|d| (d.min_bits, d.max_bits, d.school))
                        .collect(),
                })
            })
            .collect::<Result<Vec<_>, NativeError>>()?;
        let melee_timers = PlayerMeleeTimers::from_initializer(
            &equipment_weapons,
            &equipment,
            &mage.stat_recipe.snapshot(),
        )?;
        self.player.level = input.level;
        self.player.max_health = stats.max_health;
        self.player.max_mana = stats.max_mana;
        self.player.current_health = mage.resources.health;
        self.player.current_mana = mage.resources.mana;
        self.player.normal_mana_per_second = stats.normal_regen;
        self.player.interrupted_mana_per_second = stats.interrupted_regen;
        self.combat = Some(CombatGenesis {
            input_identity: input.source_identity(),
            prepared: prepared.cloned(),
            initial_cast_source: retained_seal,
            starter_identity: PlayerStarterIdentity {
                race: input.race,
                class: input.class,
                gender: input.gender,
            },
            stats,
            initial_health: mage.resources.health,
            initial_mana: mage.resources.mana,
            admitted_abilities: self.abilities.clone(),
            skills: skills.into_iter().map(|(id, s)| (id, s.into())).collect(),
            equipment,
            recipe: mage.stat_recipe,
            initial_auras: mage
                .initial_auras
                .iter()
                .map(|a| {
                    let raw = input
                        .spells
                        .iter()
                        .find(|s| s.id == a.spell_id)
                        .expect("initializer checked aura spell");
                    InitialAuraMetadata::from_source(a, raw)
                })
                .collect(),
            initial_inventory: mage.inventory.iter().map(|i| (i.item, i.count)).collect(),
            equipment_weapons,
            melee_timers,
            proficiencies: (mage.weapon_proficiency, mage.armor_proficiency),
            durability_hit_chance,
            defense_gain,
            damage_cancels,
            death: None,
        });
        self.validate()
    }
}
impl CombatGenesis {
    pub(in crate::native) fn initialized_projection(
        &self,
        input: &init::StarterInput,
    ) -> Result<init::InitializedMage, NativeError> {
        if self.input_identity != input.source_identity() {
            return Err(NativeError::InvalidProfile);
        }
        match &self.prepared {
            Some(prepared) => {
                interactions::spell_authority::InitialCastProof::check_bound(input)?;
                prepared.validate_against(input)?;
                Ok(prepared.created().clone())
            }
            None => Ok(init::initialize_mage(input)?),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerCombatState {
    pub stats: CharacterStats,
    pub skills: BTreeMap<u16, SkillState>,
    pub equipment: equipment::EquipmentState,
    pub(super) recipe: init::StatRecipe,
    pub melee_timers: PlayerMeleeTimers,
    pub(in crate::native) melee_reset: Option<crate::native::melee_reset::MeleeResetWitness>,
    pub standing: bool,
    pub has_channeled_spell: bool,
    pub has_auto_repeat_spell: bool,
    pub defense_reactive_started_ms: Option<u64>,
    pub defense_reactive_ms: u32,
    pub in_combat: bool,
    pub combat_tick_ms: u32,
    pub next_action: u64,
    pub action_tip: crate::Digest,
    pub death: Option<benilla_sim::player_death::PlayerDeathState>,
    pub death_killer: Option<crate::NpcId>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CombatSnapshot {
    pub(super) stats: CharacterStats,
    skills: BTreeMap<u16, SkillState>,
    equipment: equipment::EquipmentSnapshot,
    recipe: init::StatRecipeSnapshot,
    melee_timers: PlayerMeleeTimers,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    pub(in crate::native) melee_reset: Option<crate::native::melee_reset::MeleeResetWitness>,
    standing: bool,
    has_channeled_spell: bool,
    has_auto_repeat_spell: bool,
    defense_reactive_started_ms: Option<u64>,
    defense_reactive_ms: u32,
    in_combat: bool,
    combat_tick_ms: u32,
    next_action: u64,
    action_tip: crate::Digest,
    death: Option<benilla_sim::player_death::PlayerDeathSnapshot>,
    death_killer: Option<crate::NpcId>,
}
impl PlayerCombatState {
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(super) fn new(g: &CombatGenesis, origin: u64) -> Result<Self, NativeError> {
        g.validate_melee_genesis()?;
        Ok(Self {
            stats: g.stats.clone(),
            skills: g.skills.clone(),
            equipment: equipment::EquipmentState::from_snapshot(g.equipment.clone())?,
            recipe: g.recipe.clone(),
            melee_timers: g.melee_timers.clone(),
            melee_reset: None,
            standing: true,
            has_channeled_spell: false,
            has_auto_repeat_spell: false,
            defense_reactive_started_ms: None,
            defense_reactive_ms: 0,
            in_combat: false,
            combat_tick_ms: 1200,
            next_action: 1,
            action_tip: [0; 32],
            death: g
                .death
                .as_ref()
                .map(|p| {
                    benilla_sim::player_death::PlayerDeathState::new(
                        SOLO_UNIT,
                        p.config,
                        0,
                        origin / 1000,
                    )
                })
                .transpose()?,
            death_killer: None,
        })
    }
    pub(super) fn snapshot(&self) -> CombatSnapshot {
        CombatSnapshot {
            stats: self.stats.clone(),
            skills: self.skills.clone(),
            equipment: self.equipment.snapshot(),
            recipe: self.recipe.snapshot(),
            melee_timers: self.melee_timers.clone(),
            melee_reset: self.melee_reset.clone(),
            standing: self.standing,
            has_channeled_spell: self.has_channeled_spell,
            has_auto_repeat_spell: self.has_auto_repeat_spell,
            defense_reactive_started_ms: self.defense_reactive_started_ms,
            defense_reactive_ms: self.defense_reactive_ms,
            in_combat: self.in_combat,
            combat_tick_ms: self.combat_tick_ms,
            next_action: self.next_action,
            action_tip: self.action_tip,
            death: self.death.as_ref().map(|d| d.snapshot()),
            death_killer: self.death_killer,
        }
    }
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(super) fn restore(
        g: &CombatGenesis,
        s: CombatSnapshot,
        health: u32,
        mana: u32,
        now: u64,
        tick: u64,
        origin: u64,
        diff: u32,
    ) -> Result<Self, NativeError> {
        Self::restore_inner(g, s, health, mana, now, tick, origin, diff, false, None)
    }
    /// Provisional component reconstruction for an explicitly selected profile.
    /// The containing checkpoint must validate the complete player melee proof.
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(super) fn restore_selected_fields(
        g: &CombatGenesis,
        s: CombatSnapshot,
        health: u32,
        mana: u32,
        now: u64,
        tick: u64,
        origin: u64,
        diff: u32,
    ) -> Result<Self, NativeError> {
        Self::restore_inner(g, s, health, mana, now, tick, origin, diff, true, None)
    }
    /// The containing quest owner independently replays the complete source
    /// journal before calling this entry. Persisted membership is not authority.
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(super) fn restore_replayed_fields(
        g: &CombatGenesis,
        s: CombatSnapshot,
        recipe: init::StatRecipe,
        health: u32,
        mana: u32,
        now: u64,
        tick: u64,
        origin: u64,
        diff: u32,
        selected_melee: bool,
    ) -> Result<Self, NativeError> {
        Self::restore_inner(
            g,
            s,
            health,
            mana,
            now,
            tick,
            origin,
            diff,
            selected_melee,
            Some(recipe),
        )
    }
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    fn restore_inner(
        g: &CombatGenesis,
        s: CombatSnapshot,
        health: u32,
        mana: u32,
        now: u64,
        tick: u64,
        origin: u64,
        diff: u32,
        selected_melee: bool,
        replayed_recipe: Option<init::StatRecipe>,
    ) -> Result<Self, NativeError> {
        let death = match (&g.death, s.death) {
            (Some(p), Some(d)) => Some(
                benilla_sim::player_death::PlayerDeathState::restore_native_fixed_step(
                    d,
                    SOLO_UNIT,
                    p.config,
                    benilla_sim::player_death::NativeDeathClock {
                        clock_origin_ms: origin,
                        tick_ms: diff,
                        current_tick: tick,
                    },
                )?,
            ),
            (None, None) => None,
            _ => return Err(NativeError::InvalidCheckpoint),
        };
        if let Some(d) = &death {
            if d.is_alive() != (health > 0)
                || d.is_alive() != s.death_killer.is_none()
                || (!d.is_alive() && (s.in_combat || s.defense_reactive_ms != 0))
            {
                return Err(NativeError::InvalidCheckpoint);
            }
        } else if health == 0 || s.death_killer.is_some() {
            return Err(NativeError::InvalidCheckpoint);
        }
        let (recipe, equipment) = match replayed_recipe {
            Some(recipe) => {
                if recipe.snapshot() != s.recipe {
                    return Err(NativeError::InvalidCheckpoint);
                }
                let equipment = quest_progression::equipment_movement::restore_equipment(
                    g,
                    s.equipment.clone(),
                    &recipe,
                )?;
                (recipe, equipment)
            }
            None => (
                init::StatRecipe::restore(&g.recipe, &s.recipe)?,
                equipment::EquipmentState::restore(
                    s.equipment.clone(),
                    &equipment::EquipmentState::from_snapshot(g.equipment.clone())?,
                )?,
            ),
        };
        let live = init::Resources {
            health,
            max_health: s.stats.max_health,
            mana,
            max_mana: s.stats.max_mana,
        };
        let expected = CharacterStats::from_result(&recipe.current(live)?);
        let broken = s
            .equipment
            .items
            .iter()
            .filter(|i| i.is_broken())
            .map(|i| i.slot)
            .collect::<Vec<_>>();
        if expected != s.stats
            || broken != s.recipe.broken_slots
            || s.recipe
                .inactive_slots
                .iter()
                .any(|slot| s.recipe.broken_slots.contains(slot))
            || s.skills.len() != g.skills.len()
            || s.next_action == 0
            || s.next_action > tick.saturating_mul(4096).saturating_add(1)
            || (s.next_action == 1) != (s.action_tip == [0; 32])
            || !s.standing
            || s.has_channeled_spell
            || s.has_auto_repeat_spell
            || s.combat_tick_ms > 1200
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        for (id, skill) in &s.skills {
            let initial = g.skills.get(id).ok_or(NativeError::InvalidCheckpoint)?;
            let mut expected = initial.clone();
            if *id == DEFENSE_SKILL {
                if skill.value < initial.value || skill.value > skill.maximum {
                    return Err(NativeError::InvalidCheckpoint);
                }
                expected.value = skill.value;
            }
            if &expected != skill {
                return Err(NativeError::InvalidCheckpoint);
            }
        }
        let expected_timer = match s.defense_reactive_started_ms {
            None => 0,
            Some(at) => {
                if at < origin || at > now || !(at - origin).is_multiple_of(u64::from(diff)) {
                    return Err(NativeError::InvalidCheckpoint);
                }
                4000u64.saturating_sub(now - at) as u32
            }
        };
        if expected_timer != s.defense_reactive_ms
            || (s.defense_reactive_ms == 0 && s.defense_reactive_started_ms.is_some())
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        let restored = Self {
            stats: s.stats,
            skills: s.skills,
            equipment,
            recipe,
            melee_timers: s.melee_timers,
            melee_reset: s.melee_reset,
            standing: s.standing,
            has_channeled_spell: s.has_channeled_spell,
            has_auto_repeat_spell: s.has_auto_repeat_spell,
            defense_reactive_started_ms: s.defense_reactive_started_ms,
            defense_reactive_ms: s.defense_reactive_ms,
            in_combat: s.in_combat,
            combat_tick_ms: s.combat_tick_ms,
            next_action: s.next_action,
            action_tip: s.action_tip,
            death,
            death_killer: s.death_killer,
        };
        if selected_melee {
            restored.validate_melee_timer_fields(g)
        } else {
            restored.validate_melee_timers(g)
        }
        .map_err(|_| NativeError::InvalidCheckpoint)?;
        Ok(restored)
    }
    pub(super) fn apply_item_break(
        &mut self,
        slot: u8,
        live: init::Resources,
    ) -> Result<init::EquipmentRecompute, NativeError> {
        let mut recipe = self.recipe.clone();
        let result = recipe.set_item_broken(slot, true, live)?;
        // Player::_ApplyItemBonuses(false) runs before durability reaches zero.
        // It updates the attack-time field, never the running attack countdown.
        self.melee_timers.item_broken(slot, &recipe.snapshot())?;
        self.recipe = recipe;
        self.stats = CharacterStats::from_result(&result);
        Ok(result)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeleeFacts {
    pub front_facing: bool,
    pub player_can_attack_npc: bool,
    pub player_can_enter_combat: bool,
    pub npc_can_enter_combat: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum CombatEvent {
    CriticalWound,
    StoodUp,
    EnteredCombat,
    LeftCombat,
    DefenseSkill {
        before: u16,
        after: u16,
    },
    DefenseReactive {
        remaining_ms: u32,
    },
    Durability(equipment::DurabilityChanges),
    ItemStats {
        slot: u8,
        stats: CharacterStats,
        resources: Vec<(u32, u32, u32, u32)>,
    },
    ActionCommitted {
        action: u64,
        binding: crate::Digest,
        tip: crate::Digest,
    },
}
impl NativeWorld {
    pub fn player_limits(&self) -> (u32, u32) {
        self.state
            .combat
            .as_ref()
            .map(|c| (c.stats.max_health, c.stats.max_mana))
            .unwrap_or((self.profile.player.max_health, self.profile.player.max_mana))
    }
}

fn serialize_death_support<S: serde::Serializer>(
    _: &benilla_sim::player_death::DeathSupport,
    s: S,
) -> Result<S::Ok, S::Error> {
    (
        benilla_sim::player_death::PLAYER_DEATH_POLICY,
        benilla_sim::player_death::REQUIRED_ABSENCES,
    )
        .serialize(s)
}
/// Captured map/config and checked bounded callback closure. This is not raw-data admission.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DeathPolicy {
    pub config: benilla_sim::player_death::DeathConfig,
    #[serde(serialize_with = "serialize_death_support")]
    pub(super) support: benilla_sim::player_death::DeathSupport,
}
impl Profile {
    pub fn configure_player_death(
        &mut self,
        config: benilla_sim::player_death::DeathConfig,
        support: benilla_sim::player_death::DeathSupport,
    ) -> Result<(), NativeError> {
        if let Some(source) = &self.map_classification {
            source.validate(self)?;
            source.validate_death(config)?;
        }
        let g = self.combat.as_mut().ok_or(NativeError::InvalidProfile)?;
        // Current source initializer admits only passive, unconditional holder modifiers.
        if g.initial_auras
            .iter()
            .any(|a| a.attributes & 0x40 == 0 || a.caster_aura_state != 0)
        {
            return Err(NativeError::UnsupportedMechanic(
                "death removal of active/conditional player aura modifiers",
            ));
        }
        g.death = Some(DeathPolicy { config, support });
        Ok(())
    }
}
