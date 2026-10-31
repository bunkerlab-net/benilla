// SPDX-License-Identifier: GPL-3.0-or-later
//! Checked persistence DTOs. Deserializing these does not construct executable state.
use super::*;
use serde::{Deserialize, Serialize};

pub const SPELL_CHECKPOINT_VERSION: u32 = 1;
pub const CAST_CHECKPOINT_VERSION: u32 = 3;
pub const AURA_CHECKPOINT_VERSION: u32 = 2;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceCheckpoint {
    version: u32,
    mana: u32,
    last_mana_use_ms: Option<u64>,
    global: BTreeMap<u32, u64>,
    spells: BTreeMap<u32, u64>,
    categories: BTreeMap<u32, u64>,
}
impl CasterResources {
    pub fn checkpoint(&self) -> ResourceCheckpoint {
        ResourceCheckpoint {
            version: SPELL_CHECKPOINT_VERSION,
            mana: self.mana,
            last_mana_use_ms: self.last_mana_use_ms,
            global: self.global.clone(),
            spells: self.spells.clone(),
            categories: self.categories.clone(),
        }
    }
    pub fn restore(
        s: ResourceCheckpoint,
        now_ms: u64,
        specs: &[MageProjectileSpec],
    ) -> Result<Self, SpellError> {
        if s.version != SPELL_CHECKPOINT_VERSION
            || s.mana > MAX_AMOUNT
            || s.last_mana_use_ms.is_some_and(|t| t > now_ms)
            || s.global.len() > specs.len()
            || s.spells.len() > specs.len()
            || s.categories.len() > specs.len()
        {
            return Err(SpellError::State("resource checkpoint bounds/version"));
        }
        for (map, kind) in [(&s.global, 0), (&s.spells, 1), (&s.categories, 2)] {
            for (&id, &until) in map {
                let duration = specs
                    .iter()
                    .filter_map(|p| match kind {
                        0 if p.global_category == id => Some(p.global_recovery_ms),
                        1 if p.spell_id == id => Some(p.recovery_ms),
                        2 if p.category == id && id != 0 => Some(p.category_recovery_ms),
                        _ => None,
                    })
                    .max()
                    .ok_or(SpellError::State("unknown cooldown key"))?;
                if duration == 0 || until > now_ms.saturating_add(u64::from(duration)) {
                    return Err(SpellError::State("cooldown timestamp"));
                }
            }
        }
        Ok(Self {
            mana: s.mana,
            last_mana_use_ms: s.last_mana_use_ms,
            global: s.global,
            spells: s.spells,
            categories: s.categories,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CastCheckpoint {
    version: u32,
    policy: String,
    prepared_spec: MageProjectileSpec,
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
    launch: Option<LaunchContext>,
}
impl MageProjectileCast {
    pub fn checkpoint(&self) -> CastCheckpoint {
        CastCheckpoint {
            version: CAST_CHECKPOINT_VERSION,
            policy: SPELL_EXECUTION_POLICY_ID.into(),
            prepared_spec: self.spec.clone(),
            release: self.release,
            caster_id: self.caster_id,
            target_id: self.target_id,
            created_ms: self.created_ms,
            last_update_ms: self.last_update_ms,
            first_update: self.first_update,
            pushbacks: self.pushbacks,
            casting_elapsed_ms: self.casting_elapsed_ms,
            pushback_added_ms: self.pushback_added_ms,
            state: self.state,
            launch: self.launch,
        }
    }
    /// Restores unchanged-context live casts. Changed release contexts require
    /// an independent source binding through restore_with_release.
    pub fn restore(
        s: CastCheckpoint,
        spec: MageProjectileSpec,
        now_ms: u64,
    ) -> Result<Self, SpellError> {
        let release = matches!(s.state, CastState::Traveling { .. })
            .then(|| MageProjectileReleaseContext::unchanged(&spec));
        Self::restore_with_release(s, spec, release, now_ms)
    }
    /// Deserialized witnesses are not authority. The caller independently derives
    /// the exact prepare recipe and, for flight, the release context from source.
    pub fn restore_with_release(
        s: CastCheckpoint,
        spec: MageProjectileSpec,
        release: Option<MageProjectileReleaseContext>,
        now_ms: u64,
    ) -> Result<Self, SpellError> {
        spec.validate()?;
        if s.prepared_spec != spec || s.release != release {
            return Err(SpellError::State("cast checkpoint source bindings"));
        }
        let current = release.map(|context| context.apply(&spec)).transpose()?;
        if s.version != CAST_CHECKPOINT_VERSION
            || s.policy != SPELL_EXECUTION_POLICY_ID
            || s.caster_id == 0
            || s.target_id == 0
            || s.caster_id == s.target_id
            || s.created_ms > s.last_update_ms
            || s.last_update_ms != now_ms
            || (s.first_update
                && (s.created_ms != s.last_update_ms
                    || s.pushbacks != 0
                    || s.casting_elapsed_ms != 0
                    || s.pushback_added_ms != 0))
            || (s.pushbacks == 0 && s.pushback_added_ms != 0)
            || s.pushback_added_ms > s.casting_elapsed_ms
        {
            return Err(SpellError::State("cast checkpoint identity/clock/version"));
        }
        let maximum_added = match s.pushbacks {
            0 => 0,
            1 => 1000,
            2 => 1800,
            3 => 2400,
            n => 2800u64.saturating_add(n.saturating_sub(4).saturating_mul(200)),
        };
        if (!spec.damage_pushback && (s.pushbacks != 0 || s.pushback_added_ms != 0))
            || s.pushback_added_ms > maximum_added
        {
            return Err(SpellError::State("checkpoint pushback schedule"));
        }
        let payload = match s.state {
            CastState::Casting { remaining_ms }
                if remaining_ms > 0
                    && remaining_ms <= spec.cast_ms
                    && s.launch.is_none()
                    && release.is_none()
                    && s.casting_elapsed_ms == now_ms - s.created_ms
                    && u64::from(remaining_ms)
                        == u64::from(spec.cast_ms)
                            .checked_add(s.pushback_added_ms)
                            .and_then(|v| v.checked_sub(s.casting_elapsed_ms))
                            .ok_or(SpellError::State("cast checkpoint remaining timer"))? =>
            {
                None
            }
            CastState::Traveling { impact_at_ms } => {
                let launch = s
                    .launch
                    .ok_or(SpellError::State("missing checkpoint launch"))?;
                let delay = u64::from(projectile_delay(
                    launch.distance_squared,
                    spec.projectile_speed,
                )?);
                let released = impact_at_ms
                    .checked_sub(delay)
                    .ok_or(SpellError::State("impact clock"))?;
                if s.first_update
                    || released < s.created_ms
                    || released > s.last_update_ms
                    || impact_at_ms <= now_ms
                    || s.casting_elapsed_ms != released - s.created_ms
                    || s.casting_elapsed_ms
                        < u64::from(spec.cast_ms).saturating_add(s.pushback_added_ms)
                {
                    return Err(SpellError::State("travel checkpoint clock"));
                }
                Some(resolve_payload(
                    current
                        .as_ref()
                        .ok_or(SpellError::State("missing checkpoint release"))?,
                    launch,
                )?)
            }
            _ => return Err(SpellError::State("non-live/invalid cast checkpoint")),
        };
        Ok(Self {
            spec,
            release,
            caster_id: s.caster_id,
            target_id: s.target_id,
            created_ms: s.created_ms,
            last_update_ms: s.last_update_ms,
            first_update: s.first_update,
            pushbacks: s.pushbacks,
            state: s.state,
            payload,
            launch: s.launch,
            casting_elapsed_ms: s.casting_elapsed_ms,
            pushback_added_ms: s.pushback_added_ms,
        })
    }
    pub fn validate_resources(&self, resources: &CasterResources) -> Result<(), SpellError> {
        match self.state {
            CastState::Casting { .. } if self.spec.global_recovery_ms != 0 => {
                let until = self
                    .created_ms
                    .checked_add(u64::from(self.spec.global_recovery_ms))
                    .ok_or(SpellError::State("GCD overflow"))?;
                if resources.global.get(&self.spec.global_category) != Some(&until) {
                    return Err(SpellError::State("casting checkpoint GCD"));
                }
            }
            CastState::Traveling { impact_at_ms } => {
                let launch = self.launch.ok_or(SpellError::State("travel launch"))?;
                let released = impact_at_ms
                    .checked_sub(u64::from(projectile_delay(
                        launch.distance_squared,
                        self.spec.projectile_speed,
                    )?))
                    .ok_or(SpellError::State("release time"))?;
                let release = self.release.ok_or(SpellError::State("travel release"))?;
                if release.mana_cost > 0
                    && !resources.last_mana_use_ms.is_some_and(|t| t >= released)
                {
                    return Err(SpellError::State("travel mana marker"));
                }
                if self.spec.recovery_ms > 0
                    && !resources.spells.get(&self.spec.spell_id).is_some_and(|t| {
                        *t >= released.saturating_add(u64::from(self.spec.recovery_ms))
                    })
                {
                    return Err(SpellError::State("travel spell cooldown"));
                }
                if self.spec.category != 0
                    && self.spec.category_recovery_ms > 0
                    && !resources
                        .categories
                        .get(&self.spec.category)
                        .is_some_and(|t| {
                            *t >= released.saturating_add(u64::from(self.spec.category_recovery_ms))
                        })
                {
                    return Err(SpellError::State("travel category cooldown"));
                }
            }
            _ => {}
        }
        Ok(())
    }
    pub fn release_failure(
        &self,
        resources: &CasterResources,
        checks: CastChecks,
        now_ms: u64,
    ) -> Option<CastFailure> {
        Self::check(&self.spec, resources, checks, now_ms, false)
    }
    /// Check release-current power before drawing launch dice. This does not
    /// bind context or modify state; the host still gates on release_due.
    pub fn release_failure_with_context(
        &self,
        resources: &CasterResources,
        checks: CastChecks,
        now_ms: u64,
        context: MageProjectileReleaseContext,
    ) -> Result<Option<CastFailure>, SpellError> {
        if !matches!(self.state, CastState::Casting { .. }) {
            return Err(SpellError::State("release check when not casting"));
        }
        if resources.mana > MAX_AMOUNT {
            return Err(SpellError::Domain("mana"));
        }
        let current = context.apply(&self.spec)?;
        Ok(Self::check(&current, resources, checks, now_ms, false))
    }
    pub fn release_context(&self) -> Option<MageProjectileReleaseContext> {
        self.release
    }
    /// Command-phase capture after prepare, before the first owning SpellEvent.
    /// This does not satisfy the stronger completed-frame validation below.
    pub fn validate_prepared_fixed_step(
        &self,
        origin: u64,
        tick_ms: u32,
    ) -> Result<(), SpellError> {
        if tick_ms == 0
            || self.created_ms < origin
            || !(self.created_ms - origin).is_multiple_of(u64::from(tick_ms))
            || !self.first_update
            || self.last_update_ms != self.created_ms
            || !matches!(self.state, CastState::Casting { .. })
        {
            return Err(SpellError::State("fixed-step prepared cast origin"));
        }
        Ok(())
    }
    /// Stronger end-of-frame checks for continuously updated fixed-step worlds.
    pub fn validate_fixed_step(&self, origin: u64, tick_ms: u32) -> Result<(), SpellError> {
        if tick_ms == 0
            || self.created_ms < origin
            || !(self.created_ms - origin).is_multiple_of(u64::from(tick_ms))
            || self.first_update
        {
            return Err(SpellError::State("fixed-step cast origin"));
        }
        if let CastState::Traveling { impact_at_ms } = self.state {
            let launch = self
                .launch
                .ok_or(SpellError::State("fixed-step missing launch"))?;
            let released = impact_at_ms
                .checked_sub(u64::from(projectile_delay(
                    launch.distance_squared,
                    self.spec.projectile_speed,
                )?))
                .ok_or(SpellError::State("release clock"))?;
            let threshold = u64::from(self.spec.cast_ms)
                .checked_add(self.pushback_added_ms)
                .ok_or(SpellError::State("release threshold"))?;
            if released < origin
                || !(released - origin).is_multiple_of(u64::from(tick_ms))
                || self.casting_elapsed_ms < threshold
                || self.casting_elapsed_ms >= threshold.saturating_add(u64::from(tick_ms))
            {
                return Err(SpellError::State("delayed fixed-step release"));
            }
        }
        Ok(())
    }
    /// Scheduled restore binds the observed callback clock independently from map
    /// time. The host must cross-check it with the actual owning event/lifetime.
    /// This does not admit finished or detached spell objects as executable casts.
    pub fn restore_scheduled_with_release(
        s: CastCheckpoint,
        spec: MageProjectileSpec,
        release: Option<MageProjectileReleaseContext>,
        now_ms: u64,
        callback_ms: u64,
    ) -> Result<Self, SpellError> {
        if callback_ms > now_ms || s.last_update_ms != callback_ms {
            return Err(SpellError::State("scheduled callback clock"));
        }
        let cast = Self::restore_with_release(s, spec, release, callback_ms)?;
        match cast.state {
            CastState::Casting { .. } if callback_ms == now_ms => {}
            CastState::Traveling { impact_at_ms } if impact_at_ms > now_ms => {}
            _ => return Err(SpellError::State("scheduled live state/deadline")),
        }
        Ok(cast)
    }
    pub fn last_update_ms(&self) -> u64 {
        self.last_update_ms
    }
    pub fn created_ms(&self) -> u64 {
        self.created_ms
    }
    pub fn caster_id(&self) -> u64 {
        self.caster_id
    }
    pub fn target_id(&self) -> u64 {
        self.target_id
    }
    pub fn launch_context(&self) -> Option<LaunchContext> {
        self.launch
    }
    /// Check the exact structural transition of two independently restored
    /// checkpoints. This binds preserved cast state, not the provenance of RNG.
    pub fn validate_release_transition(
        &self,
        before: &Self,
        diff_ms: u32,
    ) -> Result<(), SpellError> {
        if !before.release_due(self.last_update_ms, diff_ms)
            || before.last_update_ms.checked_add(u64::from(diff_ms)) != Some(self.last_update_ms)
            || before.release.is_some()
            || before.launch.is_some()
            || self.release.is_none()
        {
            return Err(SpellError::State("release checkpoint transition"));
        }
        let launch = self
            .launch
            .ok_or(SpellError::State("release checkpoint launch"))?;
        let impact_at_ms = self
            .last_update_ms
            .checked_add(u64::from(projectile_delay(
                launch.distance_squared,
                before.spec.projectile_speed,
            )?))
            .ok_or(SpellError::State("release checkpoint impact"))?;
        let mut expected = before.checkpoint();
        expected.last_update_ms = self.last_update_ms;
        expected.first_update = false;
        expected.casting_elapsed_ms = expected
            .casting_elapsed_ms
            .checked_add(u64::from(diff_ms))
            .ok_or(SpellError::State("release checkpoint elapsed"))?;
        expected.state = CastState::Traveling { impact_at_ms };
        expected.launch = self.launch;
        expected.release = self.release;
        if expected != self.checkpoint() {
            return Err(SpellError::State("release checkpoint preserved fields"));
        }
        Ok(())
    }
    /// Whether release checks must be run this frame; excludes the creation-tick skip.
    pub fn release_due(&self, now_ms: u64, diff_ms: u32) -> bool {
        !(self.first_update && now_ms == self.created_ms)
            && matches!(self.state,CastState::Casting{remaining_ms} if remaining_ms<=diff_ms)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuraCheckpoint {
    version: u32,
    spell_id: u32,
    caster_id: u64,
    effect_slot: u8,
    amount: u32,
    amplitude_ms: u32,
    duration_ms: u32,
    timer_ms: i64,
    ticks: u32,
    applied_ms: u64,
    last_update_ms: u64,
    skip_first_update: bool,
    effect_roll: Option<i32>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DamageTargetCheckpoint {
    version: u32,
    policy: String,
    id: u64,
    health: u32,
    aura: Option<AuraCheckpoint>,
}
impl DamageTarget {
    pub fn checkpoint(&self) -> DamageTargetCheckpoint {
        DamageTargetCheckpoint {
            version: AURA_CHECKPOINT_VERSION,
            policy: SPELL_EXECUTION_POLICY_ID.into(),
            id: self.id,
            health: self.health,
            aura: self.aura.as_ref().map(|a| AuraCheckpoint {
                version: AURA_CHECKPOINT_VERSION,
                spell_id: a.spell_id,
                caster_id: a.caster_id,
                effect_slot: a.effect_slot,
                amount: a.amount,
                amplitude_ms: a.amplitude_ms,
                duration_ms: a.duration_ms,
                timer_ms: a.timer_ms,
                ticks: a.ticks,
                applied_ms: a.applied_ms,
                last_update_ms: a.last_update_ms,
                skip_first_update: a.skip_first_update,
                effect_roll: a.effect_roll,
            }),
        }
    }
    pub fn restore(
        s: DamageTargetCheckpoint,
        spec: &MageProjectileSpec,
        caster_id: u64,
        now_ms: u64,
    ) -> Result<Self, SpellError> {
        spec.validate()?;
        if s.version != AURA_CHECKPOINT_VERSION || s.policy != SPELL_EXECUTION_POLICY_ID {
            return Err(SpellError::State("target checkpoint version"));
        }
        let mut target = Self::new(s.id, s.health)?;
        if let Some(a) = s.aura {
            let Some(MageEffect::PeriodicDamage {
                value,
                amplitude_ms,
                duration_ms,
            }) = spec.effects.get(usize::from(a.effect_slot)).copied()
            else {
                return Err(SpellError::State("unknown checkpoint aura effect"));
            };
            if a.version != AURA_CHECKPOINT_VERSION
                || a.spell_id != spec.spell_id
                || a.caster_id != caster_id
                || caster_id == 0
                || s.health == 0
                || a.amount != value.evaluate(spec.spell_rank, a.effect_roll)?.max(0) as u32
                || a.amplitude_ms != amplitude_ms
                || a.duration_ms == 0
                || a.duration_ms > duration_ms
                || a.timer_ms > i64::from(amplitude_ms)
                || a.timer_ms < -i64::from(MAX_TIME)
                || a.applied_ms > a.last_update_ms
                || a.last_update_ms != now_ms
                || u64::from(duration_ms).checked_sub(a.last_update_ms.saturating_sub(a.applied_ms))
                    != Some(u64::from(a.duration_ms))
                || i128::from(a.timer_ms)
                    != i128::from(amplitude_ms) * (i128::from(a.ticks) + 1)
                        - i128::from(a.last_update_ms.saturating_sub(a.applied_ms))
                || (a.skip_first_update
                    && (a.applied_ms != a.last_update_ms
                        || a.ticks != 0
                        || a.duration_ms != duration_ms))
                || u64::from(a.ticks) > a.last_update_ms.saturating_sub(a.applied_ms)
            {
                return Err(SpellError::State("aura checkpoint invariants"));
            }
            target.aura = Some(PeriodicDamageAura {
                spell_id: a.spell_id,
                caster_id: a.caster_id,
                effect_slot: a.effect_slot,
                amount: a.amount,
                amplitude_ms: a.amplitude_ms,
                duration_ms: a.duration_ms,
                timer_ms: a.timer_ms,
                ticks: a.ticks,
                applied_ms: a.applied_ms,
                last_update_ms: a.last_update_ms,
                skip_first_update: a.skip_first_update,
                effect_roll: a.effect_roll,
            });
        }
        Ok(target)
    }
    pub fn clear_aura(&mut self) {
        self.aura = None;
    }
}

impl PeriodicDamageAura {
    pub fn applied_ms(&self) -> u64 {
        self.applied_ms
    }
    pub fn last_update_ms(&self) -> u64 {
        self.last_update_ms
    }
}
