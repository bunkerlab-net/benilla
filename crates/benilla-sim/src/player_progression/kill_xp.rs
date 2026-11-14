// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Project contributors.
// Original GPL-2.0-or-later source; this bounded translation GPL-3.0-or-later.
//! Exact ordinary solo-creature Gain and victim-aware GiveXP. See KILL-XP.md.
use super::{Catalog, Error, Evidence, Host, Result, Snapshot, State};
use rustc_apfloat::{ieee::Single, Float, Round, Status, StatusAnd};
use serde::{Deserialize, Serialize};

pub const POLICY_ID: &str = "cmangos-8ec338a1-solo-creature-kill-xp-softfloat-v1";
const R: Round = Round::NearestTiesToEven;
const ONE: u32 = 0x3f80_0000;
const RESTED: u8 = 1;
const NORMAL: u8 = 2;
fn checked<T>(v: StatusAnd<T>) -> Result<T> {
    if !(v.status & !Status::INEXACT).is_empty() {
        return Err(Error::Numeric("kill/rest binary32 exception"));
    }
    Ok(v.value)
}
fn integer(v: u64) -> Result<Single> {
    checked(Single::from_u128_r(v.into(), R))
}
fn scalar(bits: u32) -> Result<Single> {
    let magnitude = bits & 0x7fff_ffff;
    if (bits >> 31 != 0 && magnitude != 0)
        || magnitude >= 0x7f80_0000
        || (magnitude != 0 && magnitude < 0x0080_0000)
    {
        return Err(Error::Numeric(
            "negative/nonfinite/subnormal kill/rest input",
        ));
    }
    Ok(Single::from_bits(bits.into()))
}
fn add(a: Single, b: Single) -> Result<Single> {
    checked(a.add_r(b, R))
}
fn sub(a: Single, b: Single) -> Result<Single> {
    checked(a.sub_r(b, R))
}
fn mul(a: Single, b: Single) -> Result<Single> {
    checked(a.mul_r(b, R))
}
fn div(a: Single, b: Single) -> Result<Single> {
    checked(a.div_r(b, R))
}
fn u32_cast(v: Single, round: Round) -> Result<u32> {
    u32::try_from(checked(v.to_u128_r(32, round, &mut false))?)
        .map_err(|_| Error::Numeric("kill/rest uint32 conversion"))
}
fn bits(v: Single) -> u32 {
    v.to_bits() as u32
}

/// Explicit authoritative Rate.XP.Kill, not an inferred/default realm config.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    pub kill_rate_bits: u32,
}
impl Rules {
    pub fn validate(self) -> Result<()> {
        scalar(self.kill_rate_bits).map(|_| ())
    }
}

/// Resolved source methods at this death, including dynamic NO_XP and max health.
/// The owner binds the raw rows/flags and actual damage ledger to these values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Victim {
    pub guid: u64,
    pub entry: u32,
    pub level: u32,
    pub experience_multiplier_bits: u32,
    pub max_health: u32,
    pub damage_by_others: u32,
    pub is_totem: bool,
    pub is_pet: bool,
    pub is_no_xp: bool,
    pub is_critter: bool,
    pub is_elite: bool,
    pub player_controlled: bool,
}
impl Victim {
    pub fn validate(self) -> Result<()> {
        if self.guid == 0
            || self.entry == 0
            || !(1..=255).contains(&self.level)
            || self.max_health == 0
        {
            return Err(Error::Invalid("kill victim identity/level/health"));
        }
        let _ = scalar(self.experience_multiplier_bits)?;
        if self.is_totem || self.is_pet || self.is_elite || self.player_controlled {
            return Err(Error::Unsupported(
                "ordinary nonpet nonelite uncontrolled creature kill",
            ));
        }
        Ok(())
    }
}

/// Exact BaseGain binary32 result, not the older rational xp::base_gain helper.
pub fn base_gain_bits(unit_level: u32, mob_level: u32) -> Result<u32> {
    if !(1..=255).contains(&unit_level) || !(1..=255).contains(&mob_level) {
        return Err(Error::Invalid("kill level"));
    }
    let base = integer(unit_level as u64 * 5 + 45)?;
    let one = scalar(ONE)?;
    let value = if mob_level >= unit_level {
        mul(
            base,
            add(
                one,
                mul(
                    scalar(0x3d4c_cccd)?,
                    integer((mob_level - unit_level).min(4).into())?,
                )?,
            )?,
        )?
    } else {
        let unit = crate::xp::Level::new(unit_level as u8).ok_or(Error::Invalid("kill level"))?;
        let mob = crate::xp::Level::new(mob_level as u8).ok_or(Error::Invalid("kill level"))?;
        if crate::xp::is_trivial(unit, mob) {
            Single::ZERO
        } else {
            mul(
                base,
                sub(
                    one,
                    div(
                        integer((unit_level - mob_level).into())?,
                        integer(crate::xp::zero_difference(unit).into())?,
                    )?,
                )?,
            )?
        }
    };
    Ok(bits(value))
}

/// Source float->uint32 before damage attribution, then uint32->float before
/// Rate.XP.Kill and nearbyint (FE_TONEAREST). Every conversion is checked.
pub fn gain(unit_level: u32, victim: Victim, rules: Rules) -> Result<u32> {
    victim.validate()?;
    rules.validate()?;
    if victim.is_no_xp || victim.is_critter {
        return Ok(0);
    }
    let base = scalar(base_gain_bits(unit_level, victim.level)?)?;
    if base.is_zero() {
        return Ok(0);
    }
    let mut xp = u32_cast(
        mul(base, scalar(victim.experience_multiplier_bits)?)?,
        Round::TowardZero,
    )?;
    if xp != 0 && victim.damage_by_others != 0 {
        let percentage = div(
            integer(victim.damage_by_others.into())?,
            integer(victim.max_health.into())?,
        )?;
        if percentage >= scalar(ONE)? {
            xp = 0;
        } else if percentage > Single::ZERO {
            xp = u32_cast(
                mul(integer(xp.into())?, sub(scalar(ONE)?, percentage)?)?,
                Round::TowardZero,
            )?;
        }
    }
    u32_cast(mul(integer(xp.into())?, scalar(rules.kill_rate_bits)?)?, R)
}

/// Complete source lookup is required unless the actual no-reputation flag is set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReputationAdmission {
    Unknown,
    NoReputationFlag,
    ProvenAbsentOnKillRow { entry: u32 },
    PresentOnKillRow,
}
/// Bounded production rest contract: a newly created player that has stayed
/// online outside rest areas. Rejoining/offline time is not silently discarded.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OnlineRestContract {
    pub rule: OnlineRestRule,
    pub offline_intervals: Evidence,
    pub entered_rest_area: Evidence,
}
/// Explicit authored P2P continuation rule: a save/reopen pauses the same online
/// simulation; it does not perform a CMaNGOS logout/login or accrue offline rest.
/// The caller must display/bind this rule, never infer it from missing timestamps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OnlineRestRule {
    #[serde(rename = "fresh-online-simulation-continuation-no-offline-rest-v1")]
    SimulationContinuationWithoutSourceLogout,
}
/// Source Unit::Kill tap selection, plus explicit no-effect honor/reputation proof.
/// death_event is an owner-generated identity, never an external kill command.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KillContext {
    pub death_event: u64,
    pub player_guid: u64,
    pub responsible_player_guid: Option<u64>,
    pub original_loot_recipient_guid: Option<u64>,
    pub group_loot_recipient: Evidence,
    pub civilian_for_target: Evidence,
    pub racial_leader: Evidence,
    pub reputation: ReputationAdmission,
    pub clock: OnlineRestContract,
}
impl KillContext {
    pub fn validate(self, victim: Victim, rest: RestState) -> Result<()> {
        if self.death_event == 0
            || self.player_guid == 0
            || self.player_guid == victim.guid
            || self.responsible_player_guid == Some(0)
            || self.original_loot_recipient_guid == Some(0)
            || self
                .original_loot_recipient_guid
                .or(self.responsible_player_guid)
                != Some(self.player_guid)
        {
            return Err(Error::Invalid("source kill tap/death identity"));
        }
        if self.group_loot_recipient != Evidence::Absent
            || self.civilian_for_target != Evidence::Absent
            || self.racial_leader != Evidence::Absent
        {
            return Err(Error::Unsupported("kill group/honor dependencies"));
        }
        match self.reputation {
            ReputationAdmission::NoReputationFlag => (),
            ReputationAdmission::ProvenAbsentOnKillRow { entry } if entry == victim.entry => (),
            _ => return Err(Error::Unsupported("complete kill reputation absence")),
        }
        if self.clock.offline_intervals != Evidence::Absent
            || self.clock.entered_rest_area != Evidence::Absent
            || rest != RestState::fresh_player()
        {
            return Err(Error::Unsupported(
                "fresh continuous online nonresting clock",
            ));
        }
        Ok(())
    }
}

/// Native m_rest_bonus, PLAYER_BYTES_2 byte3 and PLAYER_REST_STATE_EXPERIENCE.
/// This DTO is not provenance. Capture/restore and clock application are owned by
/// the caller. The high-level award currently admits the fresh-online case only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestState {
    pub bonus_bits: u32,
    pub client_state: u8,
    pub experience: u32,
}
impl RestState {
    pub const fn fresh_player() -> Self {
        Self {
            bonus_bits: 0,
            client_state: NORMAL,
            experience: 0,
        }
    }
    pub fn validate(self) -> Result<()> {
        if ![RESTED, NORMAL].contains(&self.client_state)
            || u32_cast(scalar(self.bonus_bits)?, Round::TowardZero)? != self.experience
        {
            return Err(Error::Invalid("rest checkpoint"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    RestStateByte(u8),
    RestExperience(u32),
    /// SMSG_LOG_XPGAIN: guid, total_xp, type0, base_xp, group_rate float bits.
    KillXpLog {
        victim_guid: u64,
        total_xp: u32,
        base_xp: u32,
        group_rate_bits: u32,
    },
    Progression(super::Event),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub event: Event,
    pub view: Snapshot,
    pub rest: RestState,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transaction<H> {
    pub state: State,
    pub rest: RestState,
    pub host: H,
    pub events: Vec<Observation>,
}

/// Required native owner audit and real side effects on a deep in-memory clone.
/// `preflight_kill` rechecks actual tap, death-once, flags, rows and source clock;
/// callers cannot turn an unverified assertion into proof by hashing it.
/// `apply_kill` records the rest fields and real packet outbox. It must not publish
/// or persist during staging. Progression callbacks still use super::Host::apply.
pub trait KillHost: Host {
    fn preflight_kill(
        &self,
        victim: &Victim,
        rules: &Rules,
        context: &KillContext,
        rest: &RestState,
    ) -> Result<()>;
    fn apply_kill(&mut self, event: &Event, view: &Snapshot, rest: &RestState) -> Result<()>;
}

/// Pure source SetRestBonus. Source emits byte writes even if the byte is equal;
/// 1<bonus<=10 preserves the previous byte. XP field is always written.
pub fn set_rest_bonus(
    previous: RestState,
    bonus_bits: u32,
    level: u32,
    max_level: u32,
    next_xp: u32,
) -> Result<(RestState, Vec<Event>)> {
    previous.validate()?;
    let magnitude = bonus_bits & 0x7fff_ffff;
    if magnitude >= 0x7f80_0000 || (magnitude != 0 && magnitude < 0x0080_0000) {
        return Err(Error::Numeric("nonfinite/subnormal rest input"));
    }
    let mut value = Single::from_bits(bonus_bits.into());
    if value < Single::ZERO {
        value = Single::ZERO;
    }
    if level >= max_level {
        value = Single::ZERO;
    }
    let maximum = div(
        mul(integer(next_xp.into())?, scalar(0x3fc0_0000)?)?,
        integer(2)?,
    )?;
    if value > maximum {
        value = maximum;
    }
    let mut out = RestState {
        bonus_bits: bits(value),
        client_state: previous.client_state,
        experience: u32_cast(value, Round::TowardZero)?,
    };
    let mut events = Vec::new();
    if value > integer(10)? {
        out.client_state = RESTED;
        events.push(Event::RestStateByte(RESTED));
    } else if value <= integer(1)? {
        out.client_state = NORMAL;
        events.push(Event::RestStateByte(NORMAL));
    }
    events.push(Event::RestExperience(out.experience));
    Ok((out, events))
}
/// Source GetXPRestBonus and SetRestBonus, including binary32 uint32 subtraction.
pub fn consume_rest(
    previous: RestState,
    xp: u32,
    level: u32,
    max_level: u32,
    next_xp: u32,
) -> Result<(RestState, u32, Vec<Event>)> {
    previous.validate()?;
    let bonus = previous.experience.min(xp);
    let remainder = sub(scalar(previous.bonus_bits)?, integer(bonus.into())?)?;
    let (state, events) = set_rest_bonus(previous, bits(remainder), level, max_level, next_xp)?;
    Ok((state, bonus, events))
}

/// Explicit source ComputeRest rules. Clock/location selection is not inferred.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestRules {
    pub ingame_bits: u32,
    pub offline_rest_place_bits: u32,
    pub offline_wilderness_bits: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RestInterval {
    Online,
    OfflineRestPlace,
    OfflineWilderness,
}
/// Source time_t seconds must be nonnegative and explicitly supplied by the
/// authoritative clock. Online caller applies only when resting and time_inn>=10;
/// offline caller applies only once for positive time_diff after InitStatsForLevel.
pub fn compute_rest_bits(
    seconds: u64,
    next_xp: u32,
    interval: RestInterval,
    rules: RestRules,
) -> Result<u32> {
    if seconds > i64::MAX as u64 {
        return Err(Error::Invalid("rest source time_t interval"));
    }
    for b in [
        rules.ingame_bits,
        rules.offline_rest_place_bits,
        rules.offline_wilderness_bits,
    ] {
        let _ = scalar(b)?;
    }
    let base = mul(
        integer(seconds)?,
        div(integer(next_xp.into())?, integer(1_152_000)?)?,
    )?;
    let rate = match interval {
        RestInterval::Online => scalar(rules.ingame_bits)?,
        RestInterval::OfflineRestPlace => scalar(rules.offline_rest_place_bits)?,
        RestInterval::OfflineWilderness => {
            div(scalar(rules.offline_wilderness_bits)?, integer(4)?)?
        }
    };
    Ok(bits(mul(base, rate)?))
}

/// RewardSinglePlayerAtKill's ordinary no-honor/no-reputation branch, through
/// GiveXP, before KilledMonster/quest credit. The owner applies credit afterward
/// in the SAME atomic death transaction, including zero-XP/dead/capped cases.
pub fn award<H: KillHost>(
    catalog: &Catalog,
    state: &State,
    rest: RestState,
    host: &H,
    rules: Rules,
    victim: Victim,
    context: KillContext,
) -> Result<Transaction<H>> {
    victim.validate()?;
    rules.validate()?;
    rest.validate()?;
    context.validate(victim, rest)?;
    host.preflight_kill(&victim, &rules, &context, &rest)?;
    let xp = gain(state.view().level, victim, rules)?;
    give_xp_inner(catalog, state, rest, host, xp, victim.guid)
}

fn give_xp_inner<H: KillHost>(
    catalog: &Catalog,
    state: &State,
    mut rest: RestState,
    host: &H,
    xp: u32,
    victim_guid: u64,
) -> Result<Transaction<H>> {
    rest.validate()?;
    let mut staged = super::stage(catalog, state, host)?;
    let mut events = Vec::new();
    if xp != 0 && staged.state.0.alive && staged.state.0.level < catalog.rules.max_player_level {
        let (next_rest, bonus, rest_events) = consume_rest(
            rest,
            xp,
            staged.state.0.level,
            catalog.rules.max_player_level,
            staged.state.0.next_level_xp,
        )?;
        rest.bonus_bits = next_rest.bonus_bits;
        for event in rest_events
            .into_iter()
            .chain(std::iter::once(Event::KillXpLog {
                victim_guid,
                total_xp: xp.wrapping_add(bonus),
                base_xp: xp,
                group_rate_bits: ONE,
            }))
        {
            match event {
                Event::RestStateByte(value) => rest.client_state = value,
                Event::RestExperience(value) => rest.experience = value,
                _ => (),
            }
            staged.host.apply_kill(&event, &staged.state.0, &rest)?;
            events.push(Observation {
                event,
                view: staged.state.snapshot(),
                rest,
            });
        }
        let mut new_xp = staged.state.0.xp.wrapping_add(xp).wrapping_add(bonus);
        while new_xp >= staged.state.0.next_level_xp
            && staged.state.0.level < catalog.rules.max_player_level
        {
            new_xp -= staged.state.0.next_level_xp;
            staged.level(staged.state.0.level + 1)?;
        }
        staged.state.0.xp = new_xp;
        staged.emit(super::Event::Xp(new_xp))?;
        events.extend(staged.events.into_iter().map(|observation| Observation {
            event: Event::Progression(observation.event),
            view: observation.view,
            rest,
        }));
    }
    Ok(Transaction {
        state: staged.state,
        rest,
        host: staged.host,
        events,
    })
}

#[cfg(test)]
#[path = "kill_xp_tests.rs"]
mod tests;
