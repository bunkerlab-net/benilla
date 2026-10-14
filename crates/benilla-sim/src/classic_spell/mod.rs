// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f.
// CMaNGOS Project contributors; original GPL-2.0-or-later grant applies.
// Rust translation: Benilla fork contributors, GPL-3.0-or-later, NO WARRANTY.
//! Stateful, bounded mage-projectile execution components, including periodic
//! damage. This is not a native Spell.dbc compiler or the complete CheckCast/
//! damage/AI engine. Native admission remains closed; see ../../SPELLS.md.
//! Every supplied check, stat and modifier-exclusion assertion must be resolved
//! from authoritative state. Never use a default 'true' to admit native content.
mod numeric;
pub use numeric::{
    cast_time, combat_chance, critical_damage, mana_cost, projectile_delay,
    zero_resistance_npc_miss, EffectValue, SpellFloat,
};
use std::collections::BTreeMap;

pub const SPELL_EXECUTION_POLICY_ID: &str = "cmangos-8ec338a1-mage-projectile-periodic-binary32-v3";
const MAX_AMOUNT: u32 = 1_000_000;
const MAX_TIME: u32 = 3_600_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpellError {
    Domain(&'static str),
    Unsupported(&'static str),
    Roll(&'static str),
    State(&'static str),
}
impl std::fmt::Display for SpellError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unsupported/invalid Classic spell execution: {self:?}")
    }
}
impl std::error::Error for SpellError {}

/// Typed effects retain all three source slots. No unknown effect is a no-op.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub enum MageEffect {
    Empty,
    SchoolDamage(EffectValue),
    PeriodicDamage {
        value: EffectValue,
        amplitude_ms: u32,
        duration_ms: u32,
    },
    Unsupported {
        opcode: u32,
        aura: u32,
        trigger_spell: u32,
    },
}
/// Already-resolved unmodified player cast metadata. This describes the bounded
/// algorithm, not proof a raw native row has only these semantics. A native
/// compiler still has to reject every unhandled attribute, target, item, aura,
/// script and source dependency; its existing admission gates are unchanged.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MageProjectileSpec {
    pub spell_id: u32,
    pub effects: [MageEffect; 3],
    pub spell_rank: u16,
    pub cast_ms: u32,
    pub mana_cost: u32,
    pub global_category: u32,
    pub global_recovery_ms: u32,
    pub recovery_ms: u32,
    pub category: u32,
    pub category_recovery_ms: u32,
    pub projectile_speed: SpellFloat,
    pub movement_interrupts: bool,
    pub damage_pushback: bool,
}
impl MageProjectileSpec {
    pub fn validate(&self) -> Result<(), SpellError> {
        if self.spell_id == 0
            || self.spell_rank == 0
            || self.spell_rank > 100
            || self.mana_cost > MAX_AMOUNT
            || [
                self.cast_ms,
                self.global_recovery_ms,
                self.recovery_ms,
                self.category_recovery_ms,
            ]
            .into_iter()
            .any(|v| v > MAX_TIME)
        {
            return Err(SpellError::Domain("spell metadata"));
        }
        projectile_delay(SpellFloat::ZERO, self.projectile_speed)?;
        let mut direct = 0;
        let mut periodic = 0;
        for effect in self.effects {
            match effect {
                MageEffect::Empty => {}
                MageEffect::SchoolDamage(value) => {
                    direct += 1;
                    value.roll_range(self.spell_rank)?;
                }
                MageEffect::PeriodicDamage {
                    value,
                    amplitude_ms,
                    duration_ms,
                } => {
                    periodic += 1;
                    value.roll_range(self.spell_rank)?;
                    if amplitude_ms == 0
                        || amplitude_ms > MAX_TIME
                        || duration_ms == 0
                        || duration_ms > MAX_TIME
                    {
                        return Err(SpellError::Domain("periodic timer"));
                    }
                }
                MageEffect::Unsupported { .. } => {
                    return Err(SpellError::Unsupported("effect/aura/trigger"))
                }
            }
        }
        if direct != 1 || periodic > 1 {
            return Err(SpellError::Unsupported(
                "single direct plus optional periodic morphology",
            ));
        }
        Ok(())
    }
}
/// Current source-authorized skill rank and power cost for the original spell.
/// Spell::CheckPower and launch effects re-evaluate these at release; Prepare's
/// SpellEntry, timer, GCD, pushback and effect metadata stay captured.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MageProjectileReleaseContext {
    pub spell_rank: u16,
    pub mana_cost: u32,
}
impl MageProjectileReleaseContext {
    /// Both recipes must be independently resolved from source-authoritative
    /// context. A changed newly computed cast time is ignored for this cast.
    pub fn from_specs(
        prepared: &MageProjectileSpec,
        current: &MageProjectileSpec,
    ) -> Result<Self, SpellError> {
        prepared.validate()?;
        current.validate()?;
        let mut immutable = current.clone();
        immutable.cast_ms = prepared.cast_ms;
        immutable.spell_rank = prepared.spell_rank;
        immutable.mana_cost = prepared.mana_cost;
        if immutable != *prepared {
            return Err(SpellError::Unsupported(
                "release changes prepared spell metadata",
            ));
        }
        Ok(Self {
            spell_rank: current.spell_rank,
            mana_cost: current.mana_cost,
        })
    }
    fn apply(self, prepared: &MageProjectileSpec) -> Result<MageProjectileSpec, SpellError> {
        let mut current = prepared.clone();
        current.spell_rank = self.spell_rank;
        current.mana_cost = self.mana_cost;
        current.validate()?;
        Ok(current)
    }
    fn unchanged(prepared: &MageProjectileSpec) -> Self {
        Self {
            spell_rank: prepared.spell_rank,
            mana_cost: prepared.mana_cost,
        }
    }
}
/// Isolated mutable state; persistence needs a separate adapter. Milliseconds are simulation
/// map time, not wall clock. A new generic cast may start while an older one flies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CasterResources {
    pub mana: u32,
    pub last_mana_use_ms: Option<u64>,
    global: BTreeMap<u32, u64>,
    spells: BTreeMap<u32, u64>,
    categories: BTreeMap<u32, u64>,
}
impl CasterResources {
    pub fn new(mana: u32) -> Result<Self, SpellError> {
        if mana > MAX_AMOUNT {
            return Err(SpellError::Domain("mana"));
        }
        Ok(Self {
            mana,
            last_mana_use_ms: None,
            global: BTreeMap::new(),
            spells: BTreeMap::new(),
            categories: BTreeMap::new(),
        })
    }
    pub fn global_ready_at(&self, category: u32) -> Option<u64> {
        self.global.get(&category).copied()
    }
    pub fn spell_ready_at(&self, spell: u32) -> Option<u64> {
        self.spells.get(&spell).copied()
    }
    fn ready(&self, p: &MageProjectileSpec, now: u64, strict: bool) -> bool {
        !self.spells.get(&p.spell_id).is_some_and(|t| *t > now)
            && !(p.category != 0 && self.categories.get(&p.category).is_some_and(|t| *t > now))
            && !(strict
                && self
                    .global
                    .get(&p.global_category)
                    .is_some_and(|t| *t > now))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub enum CastFailure {
    NotReady,
    NoPower,
    Interrupted,
    TargetLost,
    TargetDead,
    CasterDead,
    Rejected(u32),
}
/// These gates are independent of time/resource handling. `other_failure` is the
/// result of the remaining source CheckCast predicates, including relationship,
/// LOS, range (strict/non-strict differs), caster auras, items and location.
/// Absence is an audited precondition, not a replacement for those predicates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CastChecks {
    pub caster_alive: bool,
    pub target_exists: bool,
    pub target_alive: bool,
    pub position_changed: bool,
    pub other_failure: Option<u32>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub enum CastState {
    Casting { remaining_ms: u32 },
    Traveling { impact_at_ms: u64 },
    Finished,
    Failed(CastFailure),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub enum SpellEvent {
    Started {
        cast_ms: u32,
    },
    CooldownStarted,
    ManaSpent(u32),
    Launched {
        impact_at_ms: u64,
    },
    Resisted,
    AuraApplied {
        replaced: bool,
    },
    DirectDamage {
        amount: u32,
        effective: u32,
        critical: bool,
    },
    PeriodicDamage {
        amount: u32,
        effective: u32,
        absorb: u32,
        resist: i32,
        tick: u32,
    },
    AuraRemoved,
    Immune,
    Completed,
    Failed(CastFailure),
    Pushback(u32),
}
/// Caller records exact inclusive draws, NOT a seed tied to std RNG. Hit draws
/// are 1..=10000 and compare <= chance; crit draws compare STRICTLY < chance.
/// The arrays are indexed by effect slot; only direct-damage dice exist at launch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchRolls {
    pub hit: u16,
    pub direct: [Option<i32>; 3],
    pub critical: Option<u16>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchContext {
    pub distance_squared: SpellFloat,
    pub resist_chance: u16,
    pub critical_chance: u16,
    pub rolls: LaunchRolls,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Payload {
    damage: u32,
    critical: bool,
    resisted: bool,
}
/// Ordinary unmodified NPC branch: no absorption, resistance, damage mods,
/// threat multipliers, procs, scripts, evade, duel, pets, PvP or death prevention.
/// Those properties need real compiler/runtime checks before using this target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DamageTarget {
    pub id: u64,
    pub health: u32,
    aura: Option<PeriodicDamageAura>,
}
impl DamageTarget {
    pub fn new(id: u64, health: u32) -> Result<Self, SpellError> {
        if id == 0 || health > MAX_AMOUNT {
            return Err(SpellError::Domain("target"));
        }
        Ok(Self {
            id,
            health,
            aura: None,
        })
    }
    pub fn aura(&self) -> Option<&PeriodicDamageAura> {
        self.aura.as_ref()
    }
    /// Aura::Update and SpellAuraHolder::Update ordering: tick first, decrement
    /// duration second. Exactly one tick per update, including at zero; no invented
    /// catch-up loop. A creation-map-tick update is skipped once.
    pub fn update_aura(
        &mut self,
        now_ms: u64,
        diff_ms: u32,
        immune: bool,
    ) -> Result<Vec<SpellEvent>, SpellError> {
        self.update_aura_with_evade(now_ms, diff_ms, immune, false)
    }
    /// Timed evade is DealDamageMods absorption, not school immunity or aura removal.
    /// Existing no-resistance/no-absorb/no-proc/no-interrupt-aura domain still applies.
    pub fn update_aura_with_evade(
        &mut self,
        now_ms: u64,
        diff_ms: u32,
        immune: bool,
        evading: bool,
    ) -> Result<Vec<SpellEvent>, SpellError> {
        let mut next = self.clone();
        let events = next.update_aura_inner(now_ms, diff_ms, immune, evading)?;
        *self = next;
        Ok(events)
    }
    fn update_aura_inner(
        &mut self,
        now_ms: u64,
        diff_ms: u32,
        immune: bool,
        evading: bool,
    ) -> Result<Vec<SpellEvent>, SpellError> {
        if diff_ms > MAX_TIME {
            return Err(SpellError::Domain("aura diff"));
        }
        if self.health > MAX_AMOUNT {
            return Err(SpellError::Domain("target health"));
        }
        let Some(aura) = self.aura.as_mut() else {
            return Ok(Vec::new());
        };
        if now_ms < aura.last_update_ms {
            return Err(SpellError::State("time reversal"));
        }
        let mut events = Vec::new();
        if aura.skip_first_update {
            aura.skip_first_update = false;
            if now_ms == aura.applied_ms {
                return Ok(events);
            }
        }
        if now_ms == aura.last_update_ms {
            return Err(SpellError::State("duplicate aura update"));
        }
        aura.last_update_ms = now_ms;
        aura.timer_ms -= i64::from(diff_ms);
        if aura.timer_ms <= 0 {
            aura.timer_ms += i64::from(aura.amplitude_ms);
            aura.ticks = aura
                .ticks
                .checked_add(1)
                .ok_or(SpellError::Domain("aura ticks"))?;
            if self.health > 0 {
                if immune {
                    events.push(SpellEvent::Immune);
                } else {
                    let damage = periodic_damage_after_evade_mods(aura.amount, 0, 0, evading)?;
                    let effective = self.health.min(damage.amount);
                    self.health -= effective;
                    events.push(SpellEvent::PeriodicDamage {
                        amount: damage.amount,
                        effective,
                        absorb: damage.absorb,
                        resist: damage.resist,
                        tick: aura.ticks,
                    });
                }
            }
        }
        aura.duration_ms = aura.duration_ms.saturating_sub(diff_ms);
        if aura.duration_ms == 0 || self.health == 0 {
            self.aura = None;
            events.push(SpellEvent::AuraRemoved);
        }
        Ok(events)
    }
}

/// Post-CalculateDamageAbsorbAndResist accounting, before periodic log/DealDamage.
/// This arithmetic helper does not resolve resistance, shields, AI hooks or procs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PeriodicDamageAmounts {
    pub amount: u32,
    pub absorb: u32,
    pub resist: i32,
}
/// Unit::DealDamageMods adds the incoming damage to any existing absorb, then
/// zeros damage. SpellAuras.cpp applies negative-resist bonus/positive malus next.
pub fn periodic_damage_after_evade_mods(
    mut amount: u32,
    mut absorb: u32,
    resist: i32,
    evading: bool,
) -> Result<PeriodicDamageAmounts, SpellError> {
    if amount > MAX_AMOUNT || absorb > MAX_AMOUNT || resist.unsigned_abs() > MAX_AMOUNT {
        return Err(SpellError::Domain("periodic damage accounting"));
    }
    if evading {
        absorb += amount;
        amount = 0;
    }
    let bonus = if resist < 0 { resist.unsigned_abs() } else { 0 };
    amount += bonus;
    let malus = absorb + if resist > 0 { resist as u32 } else { 0 };
    amount = amount.saturating_sub(malus);
    Ok(PeriodicDamageAmounts {
        amount,
        absorb,
        resist,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeriodicDamageAura {
    pub spell_id: u32,
    pub caster_id: u64,
    pub effect_slot: u8,
    pub amount: u32,
    pub amplitude_ms: u32,
    pub duration_ms: u32,
    pub timer_ms: i64,
    pub ticks: u32,
    applied_ms: u64,
    last_update_ms: u64,
    skip_first_update: bool,
    effect_roll: Option<i32>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MageProjectileCast {
    spec: MageProjectileSpec,
    release: Option<MageProjectileReleaseContext>,
    caster_id: u64,
    target_id: u64,
    created_ms: u64,
    last_update_ms: u64,
    first_update: bool,
    pushbacks: u64,
    casting_elapsed_ms: u64,
    pushback_added_ms: u64,
    state: CastState,
    payload: Option<Payload>,
    launch: Option<LaunchContext>,
}
impl MageProjectileCast {
    /// Non-triggered, non-item player prepare. Caller's generic-cast-slot manager
    /// must prevent concurrency. Zero latency, unit cast speed, no GCD modifiers.
    /// Returns a failed cast without modifying resources for ordinary cast refusal.
    pub fn prepare(
        spec: MageProjectileSpec,
        caster_id: u64,
        target_id: u64,
        now_ms: u64,
        resources: &mut CasterResources,
        checks: CastChecks,
    ) -> Result<(Self, Vec<SpellEvent>), SpellError> {
        spec.validate()?;
        if resources.mana > MAX_AMOUNT {
            return Err(SpellError::Domain("mana"));
        }
        if caster_id == 0 || target_id == 0 || caster_id == target_id {
            return Err(SpellError::Domain("caster/target IDs"));
        }
        if spec.cast_ms == 0 {
            return Err(SpellError::Unsupported("instant-cast entry path"));
        }
        let fail = Self::check(&spec, resources, checks, now_ms, true);
        let state = match fail {
            Some(f) => CastState::Failed(f),
            None => CastState::Casting {
                remaining_ms: spec.cast_ms,
            },
        };
        let events = if let Some(f) = fail {
            vec![SpellEvent::Failed(f)]
        } else {
            if spec.global_recovery_ms != 0 {
                let until = now_ms
                    .checked_add(u64::from(spec.global_recovery_ms))
                    .ok_or(SpellError::Domain("GCD overflow"))?;
                resources.global.retain(|_, t| *t > now_ms);
                resources
                    .global
                    .entry(spec.global_category)
                    .or_insert(until);
            }
            vec![SpellEvent::Started {
                cast_ms: spec.cast_ms,
            }]
        };
        Ok((
            Self {
                spec,
                release: None,
                caster_id,
                target_id,
                created_ms: now_ms,
                last_update_ms: now_ms,
                first_update: true,
                pushbacks: 0,
                casting_elapsed_ms: 0,
                pushback_added_ms: 0,
                state,
                payload: None,
                launch: None,
            },
            events,
        ))
    }
    pub fn state(&self) -> CastState {
        self.state
    }
    fn check(
        spec: &MageProjectileSpec,
        resources: &CasterResources,
        checks: CastChecks,
        now: u64,
        strict: bool,
    ) -> Option<CastFailure> {
        if !resources.ready(spec, now, strict) {
            return Some(CastFailure::NotReady);
        }
        if !checks.caster_alive {
            return Some(CastFailure::CasterDead);
        }
        if let Some(code) = checks.other_failure {
            return Some(CastFailure::Rejected(code));
        }
        if resources.mana < spec.mana_cost {
            return Some(CastFailure::NoPower);
        }
        if !checks.target_exists {
            return Some(CastFailure::TargetLost);
        }
        if !checks.target_alive {
            return Some(CastFailure::TargetDead);
        }
        None
    }
    /// Cancel before launch resets GCD; cancel after launch never refunds mana or
    /// clears GCD. The latter may belong to a newer cast by the same caster.
    pub fn cancel(
        &mut self,
        resources: &mut CasterResources,
        failure: CastFailure,
    ) -> Vec<SpellEvent> {
        if matches!(self.state, CastState::Finished | CastState::Failed(_)) {
            return Vec::new();
        }
        if matches!(self.state, CastState::Casting { .. }) {
            resources.global.remove(&self.spec.global_category);
        }
        self.state = CastState::Failed(failure);
        vec![SpellEvent::Failed(failure)]
    }
    /// Spell::Delayed with no pushback-resistance auras/mods. Source delays are
    /// 1000,800,600,400,200,200... ms, capped by original cast time.
    pub fn damage_pushback(&mut self, resistance_die: u8) -> Result<Vec<SpellEvent>, SpellError> {
        if resistance_die > 99 {
            return Err(SpellError::Roll("pushback resistance die"));
        }
        let CastState::Casting { remaining_ms } = self.state else {
            return Err(SpellError::Roll("pushback die when not casting"));
        };
        if !self.spec.damage_pushback {
            return Err(SpellError::Roll("pushback die on ineligible spell"));
        }
        let delay = 1000 - self.pushbacks.min(4) as u32 * 200;
        let next_pushbacks = self
            .pushbacks
            .checked_add(1)
            .ok_or(SpellError::Domain("pushback count overflow"))?;
        let next = remaining_ms.saturating_add(delay).min(self.spec.cast_ms);
        let next_added = self
            .pushback_added_ms
            .checked_add(u64::from(next - remaining_ms))
            .ok_or(SpellError::Domain("pushback timer overflow"))?;
        self.pushbacks = next_pushbacks;
        self.pushback_added_ms = next_added;
        self.state = CastState::Casting { remaining_ms: next };
        Ok(vec![SpellEvent::Pushback(next - remaining_ms)])
    }
    /// Advances only the cast/travel clock. At launch it snapshots direct effect
    /// values/crit, starts spell/category cooldowns and spends mana in source order.
    /// Structural/numeric/roll errors are transactional: neither state is changed.
    pub fn update(
        &mut self,
        now_ms: u64,
        diff_ms: u32,
        resources: &mut CasterResources,
        checks: CastChecks,
        launch: Option<LaunchContext>,
    ) -> Result<Vec<SpellEvent>, SpellError> {
        self.update_with_release(now_ms, diff_ms, resources, checks, launch, None)
    }
    /// Bind release-current rank/cost only on a release-due update. Context on
    /// an early, creation, traveling or finished update is an atomic error.
    /// Failed release checks consume no launch dice and retain no context.
    pub fn update_with_release(
        &mut self,
        now_ms: u64,
        diff_ms: u32,
        resources: &mut CasterResources,
        checks: CastChecks,
        launch: Option<LaunchContext>,
        release: Option<MageProjectileReleaseContext>,
    ) -> Result<Vec<SpellEvent>, SpellError> {
        if release.is_some() && !self.release_due(now_ms, diff_ms) {
            return Err(SpellError::State("release context outside release update"));
        }
        self.update_dispatch(now_ms, diff_ms, resources, checks, launch, release, false)
    }
    /// Exact SpellEvent callbacks may re-enter a traveling spell at the same map
    /// time after a zero-delay re-add. Ordinary frame updates remain strict.
    pub fn update_from_event_with_release(
        &mut self,
        now_ms: u64,
        diff_ms: u32,
        resources: &mut CasterResources,
        checks: CastChecks,
        launch: Option<LaunchContext>,
        release: Option<MageProjectileReleaseContext>,
    ) -> Result<Vec<SpellEvent>, SpellError> {
        if release.is_some() && !self.release_due(now_ms, diff_ms) {
            return Err(SpellError::State("release context outside event update"));
        }
        self.update_dispatch(now_ms, diff_ms, resources, checks, launch, release, true)
    }
    #[allow(clippy::too_many_arguments)]
    fn update_dispatch(
        &mut self,
        now_ms: u64,
        diff_ms: u32,
        resources: &mut CasterResources,
        checks: CastChecks,
        launch: Option<LaunchContext>,
        release: Option<MageProjectileReleaseContext>,
        event_dispatch: bool,
    ) -> Result<Vec<SpellEvent>, SpellError> {
        let current = release
            .unwrap_or_else(|| MageProjectileReleaseContext::unchanged(&self.spec))
            .apply(&self.spec)?;
        let mut next = self.clone();
        let mut next_resources = resources.clone();
        let events = next.update_inner(
            now_ms,
            diff_ms,
            &mut next_resources,
            checks,
            launch,
            &current,
            event_dispatch,
        )?;
        *self = next;
        *resources = next_resources;
        Ok(events)
    }
    #[allow(clippy::too_many_arguments)]
    fn update_inner(
        &mut self,
        now_ms: u64,
        diff_ms: u32,
        resources: &mut CasterResources,
        checks: CastChecks,
        launch: Option<LaunchContext>,
        current: &MageProjectileSpec,
        event_dispatch: bool,
    ) -> Result<Vec<SpellEvent>, SpellError> {
        if now_ms < self.last_update_ms || diff_ms > MAX_TIME {
            return Err(SpellError::Domain("cast time progression"));
        }
        if resources.mana > MAX_AMOUNT {
            return Err(SpellError::Domain("mana"));
        }
        if matches!(self.state, CastState::Finished | CastState::Failed(_)) {
            if launch.is_some() {
                return Err(SpellError::Roll("rolls on finished cast"));
            }
            return Ok(Vec::new());
        }
        if self.first_update {
            self.first_update = false;
            if now_ms == self.created_ms {
                if launch.is_some() {
                    return Err(SpellError::Roll("rolls on creation tick"));
                }
                return Ok(Vec::new());
            }
        }
        if now_ms == self.last_update_ms
            && !(event_dispatch && matches!(self.state, CastState::Traveling { .. }))
        {
            return Err(SpellError::State("duplicate cast update"));
        }
        self.last_update_ms = now_ms;
        if !checks.target_exists {
            if launch.is_some() {
                return Err(SpellError::Roll("launch dice on lost target"));
            }
            return Ok(self.cancel(resources, CastFailure::TargetLost));
        }
        let CastState::Casting { remaining_ms } = self.state else {
            if launch.is_some() {
                return Err(SpellError::Roll("launch rolls while traveling"));
            }
            return Ok(Vec::new());
        };
        if checks.position_changed && self.spec.movement_interrupts {
            if launch.is_some() {
                return Err(SpellError::Roll("launch dice on movement cancellation"));
            }
            return Ok(self.cancel(resources, CastFailure::Interrupted));
        }
        if !checks.target_alive {
            if launch.is_some() {
                return Err(SpellError::Roll("launch dice on dead target"));
            }
            return Ok(self.cancel(resources, CastFailure::TargetDead));
        }
        self.casting_elapsed_ms = self
            .casting_elapsed_ms
            .checked_add(u64::from(diff_ms))
            .ok_or(SpellError::Domain("cast elapsed overflow"))?;
        let remaining_ms = remaining_ms.saturating_sub(diff_ms);
        self.state = CastState::Casting { remaining_ms };
        if remaining_ms != 0 {
            if launch.is_some() {
                return Err(SpellError::Roll("early launch rolls"));
            }
            return Ok(Vec::new());
        }
        if let Some(failure) = Self::check(current, resources, checks, now_ms, false) {
            if launch.is_some() {
                return Err(SpellError::Roll("launch dice on failed release check"));
            }
            // StopCast, unlike cancel(), does NOT reset GCD.
            self.state = CastState::Failed(failure);
            return Ok(vec![SpellEvent::Failed(failure)]);
        }
        let launch = launch.ok_or(SpellError::State("launch context required"))?;
        let payload = resolve_payload(current, launch)?;
        let delay = projectile_delay(launch.distance_squared, self.spec.projectile_speed)?;
        let impact_at_ms = now_ms
            .checked_add(u64::from(delay))
            .ok_or(SpellError::Domain("impact timestamp"))?;
        let spell_until = now_ms
            .checked_add(u64::from(self.spec.recovery_ms))
            .ok_or(SpellError::Domain("spell cooldown"))?;
        let category_until = now_ms
            .checked_add(u64::from(self.spec.category_recovery_ms))
            .ok_or(SpellError::Domain("category cooldown"))?;
        if self.spec.recovery_ms != 0 {
            resources.spells.insert(self.spec.spell_id, spell_until);
        }
        if self.spec.category != 0 && self.spec.category_recovery_ms != 0 {
            resources
                .categories
                .insert(self.spec.category, category_until);
        }
        resources.mana -= current.mana_cost;
        if current.mana_cost > 0 {
            resources.last_mana_use_ms = Some(now_ms);
        }
        self.payload = Some(payload);
        self.launch = Some(launch);
        self.release = Some(MageProjectileReleaseContext::unchanged(current));
        self.state = CastState::Traveling { impact_at_ms };
        Ok(vec![
            SpellEvent::CooldownStarted,
            SpellEvent::ManaSpent(current.mana_cost),
            SpellEvent::Launched { impact_at_ms },
        ])
    }
    /// Impact rechecks are supplied from DoSpellHitOnUnit's relationship/visibility/
    /// immunity path. Unsupported mechanics are not modeled by `allowed`.
    /// Apply aura first, then accumulated direct damage; lethal hits remove aura.
    /// Only the same spell+caster can replace an existing nonstacking aura.
    /// Uses release rank, assuming no post-release rank change; otherwise callers
    /// must supply the current source-derived rank to impact_at_rank.
    pub fn impact(
        &mut self,
        now_ms: u64,
        target: &mut DamageTarget,
        allowed: bool,
        immune: bool,
        aura_rolls: [Option<i32>; 3],
    ) -> Result<Vec<SpellEvent>, SpellError> {
        self.impact_at_rank(
            self.release.map_or(self.spec.spell_rank, |r| r.spell_rank),
            now_ms,
            target,
            allowed,
            immune,
            aura_rolls,
        )
    }
    /// Source HandleEffect evaluates periodic base values at impact, while the
    /// already-launched direct payload is immutable. No other spec field changes.
    pub fn impact_at_rank(
        &mut self,
        impact_rank: u16,
        now_ms: u64,
        target: &mut DamageTarget,
        allowed: bool,
        immune: bool,
        aura_rolls: [Option<i32>; 3],
    ) -> Result<Vec<SpellEvent>, SpellError> {
        let mut next = self.clone();
        let prepared_rank = next.spec.spell_rank;
        next.spec.spell_rank = impact_rank;
        next.spec.validate()?;
        let mut next_target = target.clone();
        let events = next.impact_inner(now_ms, &mut next_target, allowed, immune, aura_rolls)?;
        next.spec.spell_rank = prepared_rank;
        *self = next;
        *target = next_target;
        Ok(events)
    }
    fn impact_inner(
        &mut self,
        now_ms: u64,
        target: &mut DamageTarget,
        allowed: bool,
        immune: bool,
        aura_rolls: [Option<i32>; 3],
    ) -> Result<Vec<SpellEvent>, SpellError> {
        let CastState::Traveling { impact_at_ms } = self.state else {
            return Err(SpellError::State("not traveling"));
        };
        if now_ms < impact_at_ms || now_ms < self.last_update_ms {
            return Err(SpellError::State("early impact"));
        }
        if target.id != self.target_id {
            return Err(SpellError::State("wrong target"));
        }
        if target.health > MAX_AMOUNT {
            return Err(SpellError::Domain("target health"));
        }
        let payload = self
            .payload
            .ok_or(SpellError::State("missing launch payload"))?;
        let mut events = Vec::new();
        if payload.resisted || !allowed || immune || target.health == 0 {
            if aura_rolls != [None; 3] {
                return Err(SpellError::Roll("effect dice on rejected impact"));
            }
            if payload.resisted {
                events.push(SpellEvent::Resisted);
            } else if immune {
                events.push(SpellEvent::Immune);
            }
        } else {
            for (index, effect) in self.spec.effects.into_iter().enumerate() {
                match effect {
                    MageEffect::PeriodicDamage {
                        value,
                        amplitude_ms,
                        duration_ms,
                    } => {
                        let amount = value
                            .evaluate(self.spec.spell_rank, aura_rolls[index])?
                            .max(0) as u32;
                        if target.aura.as_ref().is_some_and(|old| {
                            old.spell_id != self.spec.spell_id || old.caster_id != self.caster_id
                        }) {
                            return Err(SpellError::Unsupported("aura stacking/rank/other caster"));
                        }
                        let replaced = target.aura.is_some();
                        target.aura = Some(PeriodicDamageAura {
                            spell_id: self.spec.spell_id,
                            caster_id: self.caster_id,
                            effect_slot: index as u8,
                            amount,
                            amplitude_ms,
                            duration_ms,
                            timer_ms: i64::from(amplitude_ms),
                            ticks: 0,
                            applied_ms: now_ms,
                            last_update_ms: now_ms,
                            skip_first_update: true,
                            effect_roll: aura_rolls[index],
                        });
                        events.push(SpellEvent::AuraApplied { replaced });
                    }
                    _ if aura_rolls[index].is_some() => {
                        return Err(SpellError::Roll("unexpected impact effect die"))
                    }
                    _ => {}
                }
            }
            let amount = if payload.critical {
                critical_damage(payload.damage)?
            } else {
                payload.damage
            };
            if amount > 0 {
                let effective = target.health.min(amount);
                target.health -= effective;
                events.push(SpellEvent::DirectDamage {
                    amount,
                    effective,
                    critical: payload.critical,
                });
            }
            if target.health == 0 && target.aura.take().is_some() {
                events.push(SpellEvent::AuraRemoved);
            }
        }
        self.state = CastState::Finished;
        self.last_update_ms = now_ms;
        events.push(SpellEvent::Completed);
        Ok(events)
    }
}

mod checkpoint;
pub use checkpoint::*;

fn resolve_payload(
    spec: &MageProjectileSpec,
    launch: LaunchContext,
) -> Result<Payload, SpellError> {
    if !(1..=10000).contains(&launch.rolls.hit)
        || launch.resist_chance > 10000
        || launch.critical_chance > 10000
    {
        return Err(SpellError::Roll("combat die"));
    }
    let resisted = launch.rolls.hit <= launch.resist_chance;
    let mut damage = 0u32;
    for (index, effect) in spec.effects.into_iter().enumerate() {
        match effect {
            MageEffect::SchoolDamage(value) if !resisted => {
                damage = value
                    .evaluate(spec.spell_rank, launch.rolls.direct[index])?
                    .max(0) as u32;
            }
            _ if launch.rolls.direct[index].is_some() => {
                return Err(SpellError::Roll("unexpected launch effect die"))
            }
            _ => {}
        }
    }
    let critical = if damage > 0 && launch.critical_chance != 0 {
        let die = launch
            .rolls
            .critical
            .ok_or(SpellError::Roll("missing crit die"))?;
        if !(1..=10000).contains(&die) {
            return Err(SpellError::Roll("crit die"));
        }
        die < launch.critical_chance
    } else {
        if launch.rolls.critical.is_some() {
            return Err(SpellError::Roll("unexpected crit die"));
        }
        false
    };
    // Validate critical result before committing cost; apply its multiplier at impact.
    if critical {
        critical_damage(damage)?;
    }
    Ok(Payload {
        damage,
        critical,
        resisted,
    })
}
