// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Spell.cpp,
// SpellMgr.h, Unit.cpp, Relations.cpp, Creature.cpp and SpellAuras.cpp.
// Upstream GPL-2.0-or-later; Copyright CMaNGOS contributors. No warranty.
//! Immutable source closure and borrowed current-state spell semantics.
use super::*;
use benilla_sim::starter_init as init;
use std::collections::BTreeSet;

pub const SPELL_AUTHORITY_POLICY: &str = "cmangos-8ec338a1-native-spell-authority-v1";
const CANT_CRIT: u32 = 0x2000_0000;

/// Complete applicable source rows, including an observed empty result. Unknown
/// is never absence. These data inputs are not a raw-producer certification.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum SpellSourceRows<T> {
    Unknown,
    Complete(Vec<T>),
}
impl<T> SpellSourceRows<T> {
    fn complete(&self, dependency: &'static str) -> Result<(), NativeError> {
        match self {
            Self::Complete(rows) if rows.len() <= 4096 => Ok(()),
            Self::Complete(_) => Err(NativeError::TooLarge),
            Self::Unknown => Err(NativeError::AuthorityUnavailable(dependency)),
        }
    }
    fn empty(&self, dependency: &'static str) -> Result<(), NativeError> {
        match self {
            Self::Complete(rows) if rows.is_empty() => Ok(()),
            Self::Complete(_) => Err(NativeError::UnsupportedMechanic(dependency)),
            Self::Unknown => Err(NativeError::AuthorityUnavailable(dependency)),
        }
    }
}
/// Raw rows, retained even though a single-ID holder domain never compares
/// different spells. This is not a precomputed stackability verdict.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SpellGroupSource {
    pub id: u32,
    pub rule: u32,
    pub name: String,
    pub spell_ids: Vec<u32>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SpellRankSource {
    pub spell_id: u32,
    pub previous_spell: u32,
    pub first_spell: u32,
    pub rank: u32,
    pub required_spell: u32,
}
/// Applicable hook keys (nonempty hook sets reject) and complete raw stacking
/// rows, retained even though the singleton holder domain never compares them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SpellHookSources {
    pub spell_area: SpellSourceRows<u32>,
    pub spell_script_names: SpellSourceRows<String>,
    pub conditions: SpellSourceRows<u32>,
    pub spell_proc_event: SpellSourceRows<u32>,
    pub spell_affect: SpellSourceRows<u32>,
    pub spell_bonus_data: SpellSourceRows<u32>,
    pub spell_threat: SpellSourceRows<u32>,
    pub spell_elixir: SpellSourceRows<u32>,
    /// Complete applicable spell_group and spell_group_spell rows.
    pub spell_group_membership: SpellSourceRows<SpellGroupSource>,
    /// Complete applicable spell_chain rows (ancestors and descendants).
    pub spell_rank_chain: SpellSourceRows<SpellRankSource>,
}
impl SpellHookSources {
    fn validate(&self) -> Result<(), NativeError> {
        self.spell_area.empty("spell_area source closure")?;
        self.spell_script_names
            .empty("spell script source closure")?;
        self.conditions.empty("spell condition source closure")?;
        self.spell_proc_event
            .empty("spell_proc_event source closure")?;
        self.spell_affect.empty("spell_affect source closure")?;
        self.spell_bonus_data
            .empty("spell_bonus_data source closure")?;
        self.spell_threat.empty("spell_threat source closure")?;
        self.spell_elixir.empty("spell_elixir source closure")?;
        // Source AddSpellAuraHolder removes the sole same-ID/same-caster
        // holder before the cross-ID group/rank loop. Complete raw rows are
        // retained even when nonempty; the sole active-ID boundary proves the
        // comparison unreachable. Unknown is still not absence.
        self.spell_group_membership
            .complete("spell group source closure")?;
        self.spell_rank_chain
            .complete("spell rank-chain source closure")?;
        if let SpellSourceRows::Complete(rows) = &self.spell_group_membership {
            let mut ids = BTreeSet::new();
            for row in rows {
                if !(1..=64).contains(&row.id)
                    || !(1..=2).contains(&row.rule)
                    || row.name.len() > 256
                    || row.spell_ids.len() > 4096
                    || !ids.insert(row.id)
                    || row
                        .spell_ids
                        .iter()
                        .any(|id| *id == 0 || *id > u16::MAX.into())
                    || row.spell_ids.iter().collect::<BTreeSet<_>>().len() != row.spell_ids.len()
                {
                    return Err(NativeError::InvalidProfile);
                }
            }
        }
        if let SpellSourceRows::Complete(rows) = &self.spell_rank_chain {
            let mut ids = BTreeSet::new();
            for row in rows {
                if row.spell_id == 0
                    || row.first_spell == 0
                    || !(1..=255).contains(&row.rank)
                    || row.previous_spell == row.spell_id
                    || !ids.insert(row.spell_id)
                    || (row.rank == 1) != (row.previous_spell == 0)
                    || (row.rank == 1 && row.first_spell != row.spell_id)
                    || (row.spell_id == 133 && (row.rank != 1 || row.first_spell != 133))
                {
                    return Err(NativeError::InvalidProfile);
                }
            }
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpellRangeSource {
    pub id: u32,
    pub minimum: spell::SpellFloat,
    pub maximum: spell::SpellFloat,
    pub flags: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct SpellCastTimeSource {
    pub id: u32,
    pub base_ms: i32,
    pub per_level_ms: i32,
    pub minimum_ms: i32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct SpellDurationSource {
    pub id: u32,
    pub base_ms: i32,
    pub per_level_ms: i32,
    pub maximum_ms: i32,
}
/// Numeric SpellEntry AFTER all spell_template overrides, not a client-only row.
/// Retains all 173 DBC words and the server-only semantic fields separately.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SpellAuthoritySource {
    pub words: Vec<u32>,
    pub server_side: u32,
    pub server_attributes: u32,
    pub bonus_coefficients: [u32; 3],
    pub attack_power_coefficients: [u32; 3],
    pub cast_time: SpellCastTimeSource,
    pub duration: Option<SpellDurationSource>,
    pub range: SpellRangeSource,
    pub category_flags: u32,
    pub facing_flags: u32,
    pub hooks: SpellHookSources,
}
/// Creature template/creation and addon data, not an asserted resistance result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct NpcSpellSource {
    pub entry: u32,
    pub resistances: [i32; 6],
    pub mechanic_immune_mask: u32,
    pub school_immune_mask: u32,
    pub minimum_level: u8,
    pub maximum_level: u8,
    pub rank: u32,
    pub subtype: u32,
    pub creature_type: u32,
    pub spell_list_id: u32,
    pub script_id: u32,
    pub ai_name: String,
    pub addon_auras: SpellSourceRows<u32>,
    pub spawn_addon_auras: SpellSourceRows<u32>,
    pub immunity_set: SpellSourceRows<u32>,
    pub event_scripts: SpellSourceRows<u32>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SpellSessionSource {
    pub player_stand_state: u8,
    pub player_gm_mode: bool,
    pub latency_ms: u32,
    pub session_scripts: SpellSourceRows<String>,
}
#[derive(Clone, Debug)]
pub struct SpellAuthorityInput {
    pub spells: Vec<SpellAuthoritySource>,
    pub npcs: Vec<NpcSpellSource>,
    pub session: SpellSessionSource,
    pub modifier_hooks: SpellHookSources,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct InitialModifierSource {
    spell_words: Vec<Vec<u32>>,
    // Item::ApplyEnchantment is excluded by the initializer. Retain every
    // equipped item's spell triggers, set/random/script fields as raw evidence.
    equipped_items: Vec<InitialItemModifierSource>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct InitialItemModifierSource {
    id: u32,
    item_set: u32,
    random_property: u32,
    scripted: bool,
    spells: [(u32, u32); 5],
}
/// No deserializer and no public construction. Catalog ownership stays with
/// Profile; current state and phase history can only be borrowed by the frame.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SpellAuthorityCatalog {
    policy: &'static str,
    spells: BTreeMap<u32, SpellAuthoritySource>,
    npcs: BTreeMap<u32, NpcSpellSource>,
    session: SpellSessionSource,
    modifier_hooks: SpellHookSources,
    initial_modifiers: InitialModifierSource,
}
impl Profile {
    /// Read-only projection contract selector; this does not grant content admission.
    pub fn has_source_spell_authority(&self) -> bool {
        selected(self)
    }
    pub fn initialize_spell_authority(
        &mut self,
        initial: &init::StarterInput,
        input: SpellAuthorityInput,
    ) -> Result<(), NativeError> {
        if initial.spells.iter().any(|row| row.words[22] != 0) {
            return Err(NativeError::UnsupportedMechanic(
                "initial spell aura-interrupt lifecycle",
            ));
        }
        if self
            .abilities
            .iter()
            .any(|a| matches!(&a.admission, AbilityAdmission::Available(spec) if spec.cast_ms == 0))
        {
            return Err(NativeError::UnsupportedMechanic(
                "instant spell entry lifecycle",
            ));
        }
        let combat = self.combat.as_ref().ok_or(NativeError::InvalidProfile)?;
        // The typed availability list alone does not prove modifier exclusions.
        // Re-run the raw initializer and compare its owned genesis with this profile.
        let mut reconstructed = self.clone();
        reconstructed.initialize_player(
            initial,
            combat.durability_hit_chance,
            combat.defense_gain,
        )?;
        if reconstructed.combat != self.combat || reconstructed.player != self.player {
            return Err(NativeError::InvalidProfile);
        }
        if input.spells.len() > 1024 || input.npcs.len() > 4096 {
            return Err(NativeError::TooLarge);
        }
        let mut spells = BTreeMap::new();
        for row in input.spells {
            if row.words.len() != 173 || spells.insert(row.words[0], row).is_some() {
                return Err(NativeError::InvalidProfile);
            }
        }
        let mut npcs = BTreeMap::new();
        for row in input.npcs {
            if npcs.insert(row.entry, row).is_some() {
                return Err(NativeError::InvalidProfile);
            }
        }
        // Initial passives are restricted by initialize_mage to stat/resistance/
        // skill changes. All spell/item proc and modifier sources stay represented.
        let mut spell_words = Vec::new();
        for row in &initial.spells {
            if row.external_initial_hooks {
                return Err(NativeError::UnsupportedMechanic("initial spell hooks"));
            }
            if let Some(active) = spells.get(&row.id) {
                if active.words != row.words {
                    return Err(NativeError::InvalidProfile);
                }
            }
            spell_words.push(row.words.to_vec());
        }
        let mut equipped_items = Vec::new();
        for item in &combat.equipment.items {
            let raw = initial
                .items
                .iter()
                .find(|i| i.id == item.item_id)
                .ok_or(NativeError::InvalidProfile)?;
            equipped_items.push(InitialItemModifierSource {
                id: raw.id,
                item_set: raw.item_set,
                random_property: raw.random_property,
                scripted: raw.scripted,
                spells: raw.spells.map(|s| (s.id, s.trigger)),
            });
        }
        let mut next = self.clone();
        next.interactions
            .as_mut()
            .ok_or(NativeError::UnsupportedMechanic(
                "spell authority requires source interaction catalog",
            ))?
            .spell_authority = Some(SpellAuthorityCatalog {
            policy: SPELL_AUTHORITY_POLICY,
            spells,
            npcs,
            session: input.session,
            modifier_hooks: input.modifier_hooks,
            initial_modifiers: InitialModifierSource {
                spell_words,
                equipped_items,
            },
        });
        next.validate()?;
        *self = next;
        Ok(())
    }
}
impl SpellAuthorityCatalog {
    pub(super) fn validate(&self, profile: &Profile) -> Result<(), NativeError> {
        let interactions = profile
            .interactions
            .as_ref()
            .ok_or(NativeError::InvalidProfile)?;
        if interactions.combat_authority.is_none() {
            return Err(NativeError::UnsupportedMechanic(
                "spell authority requires source combat host",
            ));
        }
        if self.session.player_stand_state != 0
            || self.session.player_gm_mode
            || self.session.latency_ms != 0
        {
            return Err(NativeError::UnsupportedMechanic("spell session state"));
        }
        self.modifier_hooks.validate()?;
        self.session
            .session_scripts
            .empty("spell session script source closure")?;
        if !(1..=60).contains(&profile.player.level) {
            return Err(NativeError::UnsupportedMechanic(
                "spell zero-resistance level domain",
            ));
        }
        if self.spells.len() != 1 || !self.spells.contains_key(&133) {
            return Err(NativeError::UnsupportedMechanic(
                "single audited active spell",
            ));
        }
        self.spells[&133].validate()?;
        if profile.quest_progression.is_none()
            && profile
                .combat
                .as_ref()
                .is_none_or(|c| c.equipment.items.iter().any(|i| i.max_durability != 0))
        {
            return Err(NativeError::UnsupportedMechanic(
                "mutable spell stats require progression history",
            ));
        }
        if self.npcs.len() != profile.npcs.len() {
            return Err(NativeError::InvalidProfile);
        }
        for n in &profile.npcs {
            let raw = self.npcs.get(&n.entry).ok_or(NativeError::InvalidProfile)?;
            // Creature.cpp:585 and 1983; subtype and world-boss level override
            // must be proven independently of the AI's copied level.
            if raw.resistances != [0; 6]
                || raw.school_immune_mask != 0
                || raw.mechanic_immune_mask != 0
            {
                return Err(NativeError::UnsupportedMechanic(
                    "spell NPC resistance or immunity source",
                ));
            }
            if raw.minimum_level != n.ai.level
                || raw.maximum_level != n.ai.level
                || n.combat.as_ref().is_none_or(|c| c.creature_is_critter)
                || raw.rank != 0
                || raw.subtype != 0
                || !(1..=63).contains(&n.ai.level)
                || !(1..=12).contains(&raw.creature_type)
                || raw.creature_type == 8
                || raw.spell_list_id != 0
                || raw.script_id != 0
                || !raw.ai_name.is_empty()
            {
                return Err(NativeError::UnsupportedMechanic(
                    "spell NPC kind, level or script source",
                ));
            }
            raw.addon_auras
                .empty("NPC template addon aura source closure")?;
            raw.spawn_addon_auras
                .empty("NPC spawn addon aura source closure")?;
            raw.immunity_set.empty("NPC immunity set source closure")?;
            raw.event_scripts.empty("NPC event script source closure")?;
        }
        for row in self.spells.values() {
            row.validate()?;
        }
        for a in &profile.abilities {
            if let AbilityAdmission::Available(spec) = &a.admission {
                let row = self
                    .spells
                    .get(&a.id)
                    .ok_or(NativeError::AuthorityUnavailable(
                        "raw spell source closure",
                    ))?;
                row.matches_spec(a.school, spec)?;
                row.matches_cost(
                    spec,
                    profile
                        .combat
                        .as_ref()
                        .ok_or(NativeError::InvalidProfile)?
                        .recipe
                        .snapshot()
                        .modifiers
                        .create_mana,
                )?;
                let hit = profile
                    .source_motion
                    .as_ref()
                    .and_then(|m| m.spell_hits.get(&a.id))
                    .ok_or(NativeError::InvalidProfile)?;
                if hit.attributes != row.words[6..11]
                    || hit.aura_interrupt_flags != row.words[22]
                    || hit.implicit_target_a != row.words[82..85]
                    || hit.implicit_target_b != row.words[85..88]
                {
                    return Err(NativeError::InvalidProfile);
                }
            }
        }
        Ok(())
    }
}
impl SpellAuthoritySource {
    fn validate(&self) -> Result<(), NativeError> {
        let w = &self.words;
        if w.len() != 173 || w[0] == 0 || w[0] > u16::MAX.into() || !(1..=6).contains(&w[1]) {
            return Err(NativeError::InvalidProfile);
        }
        self.hooks.validate()?;
        // Ordinary mage MAGIC, explicit enemy target. Unknown bits stay closed.
        // CANT_CRIT is admitted deliberately and overrides the player's crit stat.
        if w[6] != 0
            || w[7] != 0
            || w[8] & !CANT_CRIT != 0
            || w[9] != 0
            || w[10] != 0
            || w[160] != 3
            || !matches!(w[161], 0 | 1)
            || w[162] != 0
            || w[164] != 1
            || self.server_side != 0
            || self.server_attributes != 0
            || self.bonus_coefficients != [0; 3]
            || self.attack_power_coefficients != [0; 3]
        {
            return Err(NativeError::UnsupportedMechanic(
                "raw spell attributes, family or damage class",
            ));
        }
        // All remaining semantic fields outside the admitted numeric/effect inputs
        // are checked; unused visual/text fields 3,38,115..155,166 are retained.
        for index in [
            4, 5, 11, 12, 14, 15, 16, 17, 22, 23, 24, 25, 26, 31, 34, 35, 39, 59, 60, 159, 163,
            170, 171, 172,
        ] {
            if w[index] != 0 {
                return Err(NativeError::UnsupportedMechanic(
                    "raw spell constraint or modifier",
                ));
            }
        }
        if !matches!(w[13], 0 | 2)
            || w[21] & !0x0a != 0
            || w[58] != u32::MAX
            || w[40..58].iter().any(|x| *x != 0)
            || w[165] > 1
            || self.category_flags != 0
            || self.facing_flags & !1 != 0
        {
            return Err(NativeError::UnsupportedMechanic(
                "raw spell item, interrupt or targeting constraint",
            ));
        }
        // Positive source-audited ID boundary, not an incomplete denylist of
        // hardcoded CheckCast/LOS/hit/aura/remove/stack helper exceptions.
        // Values are still checked after all server overrides; this does not
        // claim admission of an unmodified stock Fireball row.
        if w[0] != 133 || w[1] != 2 {
            return Err(NativeError::UnsupportedMechanic(
                "audited spell ID/school domain",
            ));
        }
        if self.cast_time.id == 0
            || self.cast_time.id != w[18]
            || self.range.id != w[36]
            || self.range.id == 0
            || self.range.flags != 0
            || self.range.minimum.bits() & 0x8000_0000 != 0
            || self.range.maximum.bits() & 0x8000_0000 != 0
            || self.range.minimum.bits() > self.range.maximum.bits()
        {
            return Err(NativeError::UnsupportedMechanic(
                "raw spell cast-time or range row",
            ));
        }
        if let Some(d) = self.duration {
            if d.id == 0
                || d.id != w[30]
                || d.per_level_ms != 0
                || d.base_ms <= 0
                || d.maximum_ms != d.base_ms
            {
                return Err(NativeError::UnsupportedMechanic("raw spell duration row"));
            }
        } else if w[30] != 0 {
            return Err(NativeError::AuthorityUnavailable("raw spell duration row"));
        }
        let mut direct = 0;
        let mut periodic = 0;
        for i in 0..3 {
            if w[61 + i] == 0 {
                // Empty slots may not hide semantic effects or targets.
                if (64..=112).step_by(3).any(|base| w[base + i] != 0) || w[167 + i] != 0 {
                    return Err(NativeError::UnsupportedMechanic(
                        "nonempty raw empty effect slot",
                    ));
                }
                continue;
            }
            if w[82 + i] != 6
                || w[85 + i] != 0
                || w[79 + i] != 0
                || w[88 + i] != 0
                || w[97 + i] != 0
                || w[100 + i] > 1
                || w[103 + i] != 0
                || w[106 + i] != 0
                || w[109 + i] != 0
                || w[112 + i] != 0
                || !matches!(w[167 + i], 0 | 0x3f80_0000)
            {
                return Err(NativeError::UnsupportedMechanic(
                    "raw spell effect target, mechanic or modifier",
                ));
            }
            match (w[61 + i], w[91 + i], w[94 + i]) {
                (2, 0, 0) => direct += 1,
                (6, 3, amplitude) if amplitude > 0 && self.duration.is_some() => periodic += 1,
                _ => {
                    return Err(NativeError::UnsupportedMechanic(
                        "raw spell effect morphology",
                    ))
                }
            }
        }
        if direct != 1 || periodic > 1 || (periodic == 0 && self.duration.is_some()) {
            return Err(NativeError::UnsupportedMechanic(
                "raw mage projectile morphology",
            ));
        }
        Ok(())
    }
    fn matches_cost(
        &self,
        spec: &spell::MageProjectileSpec,
        create_mana: u32,
    ) -> Result<(), NativeError> {
        let w = &self.words;
        if spell::mana_cost(
            w[32],
            w[33],
            w[156],
            create_mana,
            spec.spell_rank,
            w[28] as u8,
        )? != spec.mana_cost
        {
            return Err(NativeError::InvalidProfile);
        }
        Ok(())
    }
    fn matches_spec(
        &self,
        school: u8,
        spec: &spell::MageProjectileSpec,
    ) -> Result<(), NativeError> {
        self.validate()?;
        spec.validate()?;
        if spec.cast_ms == 0 {
            return Err(NativeError::UnsupportedMechanic(
                "instant spell entry lifecycle",
            ));
        }
        let w = &self.words;
        if w[0] != spec.spell_id
            || w[1] != u32::from(school)
            || w[2] != spec.category
            || w[19] != spec.recovery_ms
            || w[20] != spec.category_recovery_ms
            || w[37] != spec.projectile_speed.bits()
            || w[157] != spec.global_category
            || w[158] != spec.global_recovery_ms
            || (w[21] & 8 != 0) != spec.movement_interrupts
            || (w[21] & 2 != 0) != spec.damage_pushback
            || spell::cast_time(
                self.cast_time.base_ms,
                self.cast_time.per_level_ms,
                self.cast_time.minimum_ms,
                spec.spell_rank,
                u8::try_from(w[28]).map_err(|_| NativeError::InvalidProfile)?,
            )? != spec.cast_ms
        {
            return Err(NativeError::InvalidProfile);
        }
        for i in 0..3 {
            let value = spell::EffectValue {
                base_points: w[76 + i] as i32,
                base_dice: w[67 + i] as i32,
                die_sides: w[64 + i] as i32,
                points_per_level: spell::SpellFloat::from_bits(w[73 + i])?,
                dice_per_level: spell::SpellFloat::from_bits(w[70 + i])?,
                base_level: u8::try_from(w[28]).map_err(|_| NativeError::InvalidProfile)?,
                spell_level: u8::try_from(w[29]).map_err(|_| NativeError::InvalidProfile)?,
                max_level: u8::try_from(w[27]).map_err(|_| NativeError::InvalidProfile)?,
            };
            let effect = match w[61 + i] {
                0 => spell::MageEffect::Empty,
                2 => spell::MageEffect::SchoolDamage(value),
                6 => spell::MageEffect::PeriodicDamage {
                    value,
                    amplitude_ms: w[94 + i],
                    duration_ms: self.duration.ok_or(NativeError::InvalidProfile)?.base_ms as u32,
                },
                _ => return Err(NativeError::InvalidProfile),
            };
            if spec.effects[i] != effect {
                return Err(NativeError::InvalidProfile);
            }
        }
        Ok(())
    }
}

pub(in crate::native) fn selected(profile: &Profile) -> bool {
    profile
        .interactions
        .as_ref()
        .is_some_and(|c| c.spell_authority.is_some())
}
/// Private live view; no public constructor or deserializable authority snapshot.
pub(in crate::native) struct CurrentSpell<'a> {
    view: CurrentView<'a>,
    raw: &'a SpellAuthoritySource,
    target: &'a Npc,
}
impl<'a> CurrentSpell<'a> {
    pub(in crate::native) fn new(
        profile: &'a Profile,
        state: &'a State,
        target: crate::NpcId,
        ability: u32,
    ) -> Result<Self, NativeError> {
        let catalog = profile
            .interactions
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        let source = catalog
            .spell_authority
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        if state.npcs.len() != profile.npcs.len() {
            return Err(NativeError::AuthorityBinding);
        }
        let target = state
            .npcs
            .get(&target.spawn)
            .filter(|n| n.id == target)
            .ok_or(NativeError::UnknownTarget)?;
        if target.auras.keys().any(|id| *id != 133)
            || target
                .aura_bindings
                .iter()
                .any(|(id, b)| *id != 133 || b.ability.id != 133)
        {
            return Err(NativeError::UnsupportedMechanic(
                "single audited aura holder",
            ));
        }
        let p = profile
            .npc(target.id.spawn)
            .ok_or(NativeError::InvalidState)?;
        let raw = source
            .spells
            .get(&ability)
            .ok_or(NativeError::AuthorityUnavailable(
                "raw spell source closure",
            ))?;
        if spell::zero_resistance_npc_miss(
            state.level(profile),
            p.ai.level,
            spell::SpellFloat::from_bits(0x4080_0000)?,
        )
        .is_err()
        {
            return Err(NativeError::UnsupportedMechanic(
                "spell zero-resistance level domain",
            ));
        }
        Ok(Self {
            view: CurrentView {
                profile,
                state,
                catalog,
                npc: CurrentNpc::from(target),
                source: &catalog.npcs[&p.entry],
            },
            raw,
            target,
        })
    }
    pub(in crate::native) fn range(&self) -> SpellRangeSource {
        self.raw.range
    }
    pub(in crate::native) fn needs_facing(&self) -> bool {
        self.raw.facing_flags & 1 != 0
    }
    pub(in crate::native) fn untargetable(&self) -> bool {
        self.view.npc_unit_flags() & unit_flags::UNTARGETABLE != 0
    }
    pub(in crate::native) fn can_attack_spell(&self) -> Result<bool, NativeError> {
        // Relations.cpp:1065: ordinary non-AOE player -> non-PC creature.
        if self.target.health == 0 {
            return Ok(false);
        }
        self.view.with_relation_input(|input| {
            match relations::check_can_attack_in_combat(input, relations::Side::Player) {
                relations::TargetDecision::Allowed => Ok(true),
                relations::TargetDecision::Denied(_) => Ok(false),
                relations::TargetDecision::Unsupported(e) => {
                    Err(NativeError::InteractionRelation(e))
                }
            }
        })
    }
    pub(in crate::native) fn validate_context(
        &self,
        binding: &ExecutionBinding,
    ) -> Result<(), NativeError> {
        self.raw
            .matches_spec(binding.ability.school, binding.spec()?)?;
        let create_mana = self
            .view
            .state
            .combat
            .as_ref()
            .ok_or(NativeError::InvalidState)?
            .recipe
            .snapshot()
            .modifiers
            .create_mana;
        self.raw.matches_cost(binding.spec()?, create_mana)
    }
    pub(in crate::native) fn launch_chances(&self) -> Result<(u16, u16), NativeError> {
        if self.target.ai.is_evading() {
            return Err(NativeError::UnsupportedMechanic(
                "spell release EVADE hit result",
            ));
        }
        let p = self
            .view
            .profile
            .npc(self.target.id.spawn)
            .ok_or(NativeError::InvalidState)?;
        numeric_chances(
            self.raw,
            self.view.state.level(self.view.profile),
            p.ai.level,
            self.view
                .state
                .combat
                .as_ref()
                .ok_or(NativeError::InvalidState)?
                .stats
                .crit_percent,
        )
    }
}

fn numeric_chances(
    raw: &SpellAuthoritySource,
    level: u8,
    target_level: u8,
    chance: spell::SpellFloat,
) -> Result<(u16, u16), NativeError> {
    let miss = spell::zero_resistance_npc_miss(
        level,
        target_level,
        spell::SpellFloat::from_bits(0x4080_0000)?,
    )?;
    let crit = if raw.words[8] & CANT_CRIT != 0
        || chance.bits() & 0x8000_0000 != 0
        || chance.bits() < 0x3ba3_d70a
    {
        0
    } else if chance.bits() > 0x42c8_0000 {
        10000
    } else {
        spell::combat_chance(chance)?
    };
    Ok((miss, crit))
}
/// Called only after ExecutionBinding's historical recipe/training/callback
/// prefix has been source-validated. Restore never substitutes current stats.
#[cfg(any(test, feature = "synthetic-native-tests"))]
pub(in crate::native) fn validate_historical_binding(
    profile: &Profile,
    binding: &ExecutionBinding,
) -> Result<(), NativeError> {
    let Some(catalog) = profile
        .interactions
        .as_ref()
        .and_then(|c| c.spell_authority.as_ref())
    else {
        return Ok(());
    };
    let raw = catalog
        .spells
        .get(&binding.ability.id)
        .ok_or(NativeError::InvalidCheckpoint)?;
    raw.matches_spec(binding.ability.school, binding.spec()?)?;
    raw.matches_cost(binding.spec()?, binding.historical_create_mana(profile)?)?;
    if !(1..=60).contains(&binding.level) {
        return Err(NativeError::InvalidCheckpoint);
    }
    Ok(())
}
#[cfg(any(test, feature = "synthetic-native-tests"))]
pub(in crate::native) fn validate_historical_launch(
    profile: &Profile,
    binding: &ExecutionBinding,
    target: NpcId,
    launch: &spell::LaunchContext,
) -> Result<(), NativeError> {
    let Some(catalog) = profile
        .interactions
        .as_ref()
        .and_then(|c| c.spell_authority.as_ref())
    else {
        return Ok(());
    };
    let raw = catalog
        .spells
        .get(&binding.ability.id)
        .ok_or(NativeError::InvalidCheckpoint)?;
    let level = profile
        .npc(target.spawn)
        .ok_or(NativeError::InvalidCheckpoint)?
        .ai
        .level;
    let chances = numeric_chances(
        raw,
        binding.level,
        level,
        binding.historical_spell_crit(profile)?,
    )?;
    if chances != (launch.resist_chance, launch.critical_chance) {
        return Err(NativeError::InvalidCheckpoint);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceSpellPhase {
    Prepare,
    ReleaseCheck,
    ReleaseHit,
    Impact,
    Aura,
}
/// Receipt witnesses bind the complete staged state (including scheduler) plus
/// the actual preparing/temporarily removed cast or currently installed holder.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceSpellRequest {
    pub witness: InteractionWitness,
    /// Actual executing queue/dispatch when the frame temporarily owns it outside
    /// State. Prepare/Aura use the queue already present in the state witness.
    pub scheduler: Option<benilla_sim::event_processor::EventCallbackSnapshot>,
    pub history: crate::Digest,
    pub cast_id: u64,
    pub ability: u32,
    pub phase: SourceSpellPhase,
    pub player_orientation: Orientation,
    pub player_position: ai::Point,
    pub target_position: ai::Point,
    pub player_level: u8,
    pub target_level: u8,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum SourceSpellQuery {
    Semantics(SourceSpellRequest),
    /// Spell::CheckCast's caster -> explicit target LOS, before target semantics.
    LineOfSight(SourceSpellRequest),
    /// Spell::GetMinMaxRange/CheckRange. Raw flags are zero in this host;
    /// include combat reaches, and apply non-strict tolerance only at release.
    Range {
        context: SourceSpellRequest,
        source: SpellRangeSource,
        player_reach: ai::Real,
        target_reach: ai::Real,
    },
    Facing(SourceSpellRequest),
    /// DoSpellHitOnUnit's target.IsVisibleForOrDetect(player,...,true,true).
    /// This is not the reverse hostile-hit callback or geometric segment LOS.
    ImpactVisibility(SourceSpellRequest),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpellRangeResult {
    Within,
    TooFar,
    TooClose,
    Unknown,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum SourceSpellFacts {
    Semantics(SpellFacts),
    LineOfSight(ai::movement::Check),
    Range(SpellRangeResult),
    Facing(ai::movement::Check),
    ImpactVisibility(ai::movement::Check),
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn positive_id_domain_refuses_hardcoded_helpers_and_other_ordinary_morphology() {
        let (_, _, input) = crate::native::synthetic_spell_fixture::inputs();
        for id in [
            8733, 8735, 24742, 19968, 23155, 24575, 24659, 24662, 26464, 12654, 5019, 11984, 15506,
            23255, 30074, 143, 60001,
        ] {
            let mut raw = input.spells[0].clone();
            raw.words[0] = id;
            assert!(
                matches!(
                    raw.validate(),
                    Err(NativeError::UnsupportedMechanic(
                        "audited spell ID/school domain"
                    ))
                ),
                "{id}"
            );
        }
        let mut raw = input.spells[0].clone();
        raw.validate().unwrap();
        raw.words[161] = 2;
        assert!(raw.validate().is_err());
        raw = input.spells[0].clone();
        raw.words[18] = 0;
        raw.cast_time.id = 0;
        assert!(raw.validate().is_err());
        raw = input.spells[0].clone();
        raw.words[30] = 0;
        raw.duration.as_mut().unwrap().id = 0;
        assert!(raw.validate().is_err());
    }
}
