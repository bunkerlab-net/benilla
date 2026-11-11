// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! ThreatManager.cpp:34-703 and HostileRefManager.cpp:39-246.
//! Live generation references are component state, not native save/Open authority.
use super::*;
pub use benilla_sim::creature_ai::hostile_reference::ThreatValue;
use benilla_sim::creature_ai::hostile_reference::{add_threat_value, threat_switch};
use std::cmp::Ordering;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug)]
pub struct ThreatUnit {
    pub guid: u64,
    pub player: bool,
    pub gm: bool,
    pub alive: bool,
    pub no_threat: bool,
    pub master: Option<ObjectHandle>,
    pub feign_death: bool,
    pub can_have_threat_list: bool,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct ThreatSpell {
    pub id: u32,
    pub no_threat: bool,
    pub no_harmful_threat: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct HelpfulThreatSpell {
    pub spell: ThreatSpell,
    pub school: u32,
    pub no_initial_threat: bool,
    pub no_helpful_threat: bool,
}
/// Queries belong to live Unit, aura/spell and geometry owners. No default values.
pub trait ThreatExecutionOwner {
    fn can_enter_combat(&mut self, _unit: ObjectHandle) -> Result<bool> {
        Err(refuse("execution.threat.f5_04b_combat_owner"))
    }
    fn crowd_controlled(&mut self, _unit: ObjectHandle) -> Result<bool> {
        Err(refuse("execution.threat.f5_04b_unit_fields_owner"))
    }
    fn threat_unit(&mut self, _unit: ObjectHandle) -> Result<ThreatUnit> {
        Err(refuse("execution.threat.f5_04b_unit_fields_owner"))
    }
    fn threat_lookup(&mut self, _source: ObjectHandle, _guid: u64) -> Result<Option<ObjectHandle>> {
        Err(refuse("execution.threat.object_lookup_owner"))
    }
    fn offline_target(&mut self, _source: ObjectHandle, _target: ObjectHandle) -> Result<bool> {
        Err(refuse("execution.threat.f6_offline_target_owner"))
    }
    fn can_join_attacking(&mut self, _master: ObjectHandle, _source: ObjectHandle) -> Result<bool> {
        Err(refuse("execution.threat.f5_04b_combat_owner"))
    }
    fn ignoring_ranged(&mut self, _source: ObjectHandle) -> Result<bool> {
        Err(refuse("execution.threat.f5_04b_unit_fields_owner"))
    }
    fn melee_reach(&mut self, _source: ObjectHandle, _target: ObjectHandle) -> Result<bool> {
        Err(refuse("execution.threat.f6_melee_owner"))
    }
    fn can_attack(&mut self, _source: ObjectHandle, _target: ObjectHandle) -> Result<bool> {
        Err(refuse("execution.threat.f6_attackability_owner"))
    }
    fn suppressed_target(&mut self, _source: ObjectHandle, _target: ObjectHandle) -> Result<bool> {
        Err(refuse("execution.threat.f5_12_suppression_owner"))
    }
    fn spell_threat_mod(
        &mut self,
        _target: ObjectHandle,
        _spell: u32,
        _value: ThreatValue,
    ) -> Result<ThreatValue> {
        Err(refuse("execution.threat.f5_07_spell_modifier_owner"))
    }
    fn critical_threat_mod(&mut self, _target: ObjectHandle, _school: u32) -> Result<ThreatValue> {
        Err(refuse("execution.threat.f5_12_critical_aura_owner"))
    }
    fn total_threat_mod(
        &mut self,
        _target: ObjectHandle,
        _school: u32,
        _value: ThreatValue,
    ) -> Result<ThreatValue> {
        Err(refuse("execution.threat.f5_12_threat_aura_owner"))
    }
    fn aggro_linking(
        &mut self,
        _state: &mut SourceThreats,
        _source: ObjectHandle,
        _target: ObjectHandle,
    ) -> Result<()> {
        Err(refuse("execution.threat.linking_dispatch_owner"))
    }
    fn combat_timer(&mut self, _source: ObjectHandle, _target: ObjectHandle) -> Result<()> {
        Err(refuse("execution.threat.f5_04b_combat_timer_owner"))
    }
}
pub struct ThreatContext<'a> {
    pub state: &'a mut SourceThreats,
    pub owner: &'a mut dyn ThreatExecutionOwner,
}
#[derive(Clone, Debug)]
pub struct SourceHostileReference {
    pub guid: u64,
    pub target: Option<ObjectHandle>,
    pub threat: ThreatValue,
    pub accessible: bool,
    pub suppressed: bool,
    pub suppressibility_toggle: bool,
    pub taunt: u32,
    pub fade: ThreatValue,
}
#[derive(Clone, Debug, Default)]
pub struct SourceThreatManager {
    online: Vec<SourceHostileReference>,
    offline: Vec<SourceHostileReference>,
    current: Option<u64>,
    dirty: bool,
}
impl SourceThreatManager {
    pub fn online(&self) -> &[SourceHostileReference] {
        &self.online
    }
    pub fn offline(&self) -> &[SourceHostileReference] {
        &self.offline
    }
    pub fn current(&self) -> Option<u64> {
        self.current
    }
    pub fn dirty(&self) -> bool {
        self.dirty
    }
    fn reference(&self, guid: u64) -> Option<&SourceHostileReference> {
        self.online
            .iter()
            .chain(&self.offline)
            .find(|r| r.guid == guid)
    }
    fn reference_mut(&mut self, guid: u64) -> Option<&mut SourceHostileReference> {
        self.online
            .iter_mut()
            .chain(&mut self.offline)
            .find(|r| r.guid == guid)
    }
    fn add(&mut self, guid: u64, amount: ThreatValue) -> bool {
        let current = self.current == Some(guid);
        let Some(reference) = self.reference_mut(guid) else {
            return false;
        };
        let (value, delta) = add_threat_value(reference.threat, amount);
        reference.threat = value;
        let clear = reference.suppressibility_toggle && !delta.is_zero();
        if (current && delta.less(ThreatValue::ZERO))
            || (!current && delta.greater(ThreatValue::ZERO))
        {
            self.dirty = true;
        }
        // Suppression notifications require the live ClearSuppressed owner.
        clear
    }
    pub fn threat(&self, guid: u64, include_offline: bool) -> Result<ThreatValue> {
        let reference = self.online.iter().find(|r| r.guid == guid).or_else(|| {
            include_offline
                .then(|| self.offline.iter().find(|r| r.guid == guid))
                .flatten()
        });
        match reference {
            Some(r) => Ok(r.threat),
            None if include_offline => Err(refuse(
                "execution.threat.get_threat_missing_offline_null_undefined",
            )),
            None => Ok(ThreatValue::ZERO),
        }
    }
    pub fn highest(&self) -> ThreatValue {
        self.online
            .iter()
            .chain(&self.offline)
            .fold(ThreatValue::ZERO, |v, r| {
                if r.threat.greater(v) {
                    r.threat
                } else {
                    v
                }
            })
    }
}
/// Reverse references retain link insertion order, independently of online membership.
#[derive(Clone, Debug, Default)]
pub struct SourceThreats {
    managers: BTreeMap<ObjectHandle, SourceThreatManager>,
    hated_by: BTreeMap<ObjectHandle, Vec<ObjectHandle>>,
}
impl SourceThreats {
    pub const INVENTORY_VERSION: u32 = 1;
    pub fn units(&self) -> impl Iterator<Item = (ObjectHandle, &SourceThreatManager)> {
        self.managers.iter().map(|(unit, manager)| (*unit, manager))
    }
    pub fn register(&mut self, unit: ObjectHandle) -> Result<()> {
        require(
            !self.managers.contains_key(&unit),
            "execution.threat.duplicate_generation",
        )?;
        self.managers.insert(unit, SourceThreatManager::default());
        Ok(())
    }
    pub fn manager(&self, unit: ObjectHandle) -> Result<&SourceThreatManager> {
        self.managers
            .get(&unit)
            .ok_or_else(|| refuse("execution.threat.unit_generation_missing"))
    }
    fn manager_mut(&mut self, unit: ObjectHandle) -> Result<&mut SourceThreatManager> {
        self.managers
            .get_mut(&unit)
            .ok_or_else(|| refuse("execution.threat.unit_generation_missing"))
    }
    pub fn hated_by(&self, unit: ObjectHandle) -> &[ObjectHandle] {
        self.hated_by.get(&unit).map_or(&[], Vec::as_slice)
    }
    /// Unit::AddThreat, Unit.cpp:8653-8661. Manager calls bypass this gate.
    #[allow(clippy::too_many_arguments)]
    pub fn unit_add(
        &mut self,
        source: ObjectHandle,
        target: Option<ObjectHandle>,
        amount: ThreatValue,
        crit: bool,
        school: u32,
        spell: Option<ThreatSpell>,
        owner: &mut (impl ThreatExecutionOwner + ?Sized),
    ) -> Result<()> {
        if spell.is_some_and(|s| s.no_harmful_threat) {
            return Ok(());
        }
        if owner.threat_unit(source)?.can_have_threat_list {
            self.add(source, target, amount, crit, school, spell, owner)?;
        }
        Ok(())
    }
    /// ThreatManager.cpp:446-488. No redirection exists in this pinned body.
    #[allow(clippy::too_many_arguments)]
    pub fn add(
        &mut self,
        source: ObjectHandle,
        target: Option<ObjectHandle>,
        amount: ThreatValue,
        crit: bool,
        school: u32,
        spell: Option<ThreatSpell>,
        owner: &mut (impl ThreatExecutionOwner + ?Sized),
    ) -> Result<()> {
        if target == Some(source) {
            return Ok(());
        }
        let Some(target) = target else {
            return Ok(());
        };
        let unit = owner.threat_unit(target)?;
        if unit.player && unit.gm {
            return Ok(());
        }
        if !unit.alive || !owner.threat_unit(source)?.alive {
            return Ok(());
        }
        let mut value = amount;
        if !value.is_zero() {
            if unit.no_threat || owner.threat_unit(source)?.player {
                value = ThreatValue::ZERO;
            } else {
                if let Some(spell) = spell {
                    value = owner.spell_threat_mod(target, spell.id, value)?;
                    if crit {
                        value = value.multiply(owner.critical_threat_mod(target, school)?);
                    }
                }
                value = owner.total_threat_mod(target, school, value)?;
            }
        }
        self.add_direct(
            source,
            target,
            unit.guid,
            value,
            spell.is_some_and(|s| s.no_threat),
            owner,
        )
    }
    pub fn add_direct(
        &mut self,
        source: ObjectHandle,
        target: ObjectHandle,
        guid: u64,
        value: ThreatValue,
        no_new: bool,
        owner: &mut (impl ThreatExecutionOwner + ?Sized),
    ) -> Result<()> {
        if self.manager(source)?.reference(guid).is_some() {
            self.add_existing(source, guid, value, owner)?;
            return Ok(());
        }
        if no_new {
            return Ok(());
        }
        self.manager_mut(source)?
            .online
            .push(SourceHostileReference {
                guid,
                target: Some(target),
                threat: ThreatValue::ZERO,
                accessible: true,
                suppressed: false,
                suppressibility_toggle: false,
                taunt: 0,
                fade: ThreatValue::ZERO,
            });
        self.hated_by.entry(target).or_default().insert(0, source);
        self.add_existing(source, guid, value, owner)?;
        owner.aggro_linking(self, source, target)?;
        if let Some(master) = owner.threat_unit(target)?.master {
            if owner.threat_unit(master)?.alive
                && owner.can_join_attacking(master, source)?
                && !owner.threat_unit(master)?.feign_death
            {
                self.add(
                    source,
                    Some(master),
                    ThreatValue::ZERO,
                    false,
                    0,
                    None,
                    owner,
                )?;
            }
        }
        if owner.offline_target(source, target)? || {
            let unit = owner.threat_unit(target)?;
            unit.player && unit.gm
        } {
            self.set_online(source, guid, false)?;
        }
        Ok(())
    }
    fn add_existing(
        &mut self,
        source: ObjectHandle,
        guid: u64,
        value: ThreatValue,
        owner: &mut (impl ThreatExecutionOwner + ?Sized),
    ) -> Result<()> {
        if self.manager_mut(source)?.add(guid, value) {
            self.set_suppressed(source, guid, false, owner)?;
        }
        Ok(())
    }
    pub fn set_online(&mut self, source: ObjectHandle, guid: u64, online: bool) -> Result<()> {
        let manager = self.manager_mut(source)?;
        let (from, to) = if online {
            (&mut manager.offline, &mut manager.online)
        } else {
            (&mut manager.online, &mut manager.offline)
        };
        if let Some(index) = from.iter().position(|r| r.guid == guid) {
            let reference = from.remove(index);
            if !online && manager.current == Some(guid) {
                manager.current = None;
                manager.dirty = true;
            }
            if online
                && manager
                    .current
                    .and_then(|id| to.iter().find(|r| r.guid == id))
                    .is_some_and(|r| {
                        reference
                            .threat
                            .greater(ThreatValue::from_bits(0x3f8c_cccd).multiply(r.threat))
                    })
            {
                manager.dirty = true;
            }
            to.push(reference);
        }
        Ok(())
    }
    pub fn update_online(
        &mut self,
        source: ObjectHandle,
        guid: u64,
        owner: &mut (impl ThreatExecutionOwner + ?Sized),
    ) -> Result<()> {
        let target = self
            .manager(source)?
            .reference(guid)
            .ok_or_else(|| refuse("execution.threat.reference_missing"))?
            .target;
        let target = match target {
            Some(target) => Some(target),
            None => owner.threat_lookup(source, guid)?,
        };
        if let Some(target) = target {
            require(
                owner.threat_unit(target)?.guid == guid,
                "execution.threat.rebind_guid_mismatch",
            )?;
            if self
                .manager(source)?
                .reference(guid)
                .unwrap()
                .target
                .is_none()
            {
                self.manager_mut(source)?
                    .reference_mut(guid)
                    .unwrap()
                    .target = Some(target);
                self.hated_by.entry(target).or_default().insert(0, source);
            }
        }
        let online = match target {
            Some(target) => !owner.offline_target(source, target)?,
            None => false,
        };
        self.set_online(source, guid, online)
    }
    pub fn remove(&mut self, source: ObjectHandle, guid: u64) -> Result<()> {
        self.set_online(source, guid, false)?;
        let manager = self.manager_mut(source)?;
        if let Some(index) = manager.offline.iter().position(|r| r.guid == guid) {
            let reference = manager.offline.remove(index);
            if let Some(target) = reference.target {
                if let Some(reverse) = self.hated_by.get_mut(&target) {
                    reverse.retain(|s| *s != source);
                }
            }
        }
        Ok(())
    }
    pub fn clear(&mut self, source: ObjectHandle) -> Result<()> {
        let dirty = self.manager(source)?.dirty;
        while let Some(guid) = self
            .manager(source)?
            .online
            .first()
            .or(self.manager(source)?.offline.first())
            .map(|r| r.guid)
        {
            self.remove(source, guid)?;
        }
        self.manager_mut(source)?.current = None;
        self.manager_mut(source)?.dirty = dirty;
        Ok(())
    }
    pub fn invalidate(&mut self, target: ObjectHandle) -> Result<()> {
        let reverse = self.hated_by.remove(&target).unwrap_or_default();
        for source in reverse {
            let guid = self
                .manager(source)?
                .online
                .iter()
                .chain(&self.manager(source)?.offline)
                .find(|r| r.target == Some(target))
                .ok_or_else(|| refuse("execution.threat.reverse_link_missing"))?
                .guid;
            self.manager_mut(source)?
                .reference_mut(guid)
                .unwrap()
                .target = None;
            self.set_online(source, guid, false)?;
        }
        Ok(())
    }
    pub fn delete_references(&mut self, target: ObjectHandle) -> Result<()> {
        while let Some(source) = self.hated_by(target).first().copied() {
            let guid = self
                .manager(source)?
                .online
                .iter()
                .chain(&self.manager(source)?.offline)
                .find(|r| r.target == Some(target))
                .unwrap()
                .guid;
            self.remove(source, guid)?;
        }
        Ok(())
    }
    /// Manager percentage modification searches ONLY online references, cpp:494-496.
    pub fn modify_percent(
        &mut self,
        source: ObjectHandle,
        guid: u64,
        percent: i32,
        owner: &mut (impl ThreatExecutionOwner + ?Sized),
    ) -> Result<bool> {
        let Some(value) = self
            .manager(source)?
            .online
            .iter()
            .find(|r| r.guid == guid)
            .map(|r| r.threat)
        else {
            return Ok(false);
        };
        if percent < -100 {
            self.remove(source, guid)?;
            return Ok(true);
        }
        self.add_existing(source, guid, value.percent_delta(percent), owner)?;
        Ok(false)
    }
    pub fn modify_all_percent(
        &mut self,
        source: ObjectHandle,
        percent: i32,
        owner: &mut (impl ThreatExecutionOwner + ?Sized),
    ) -> Result<()> {
        let mut index = 0;
        while let Some(guid) = self.manager(source)?.online.get(index).map(|r| r.guid) {
            if !self.modify_percent(source, guid, percent, owner)? {
                index += 1;
            }
        }
        Ok(())
    }
    /// EventAI.cpp:843-846 differs from modifyAllThreatPercent: lookup can fail.
    pub fn event_ai_all_percent(
        &mut self,
        source: ObjectHandle,
        percent: i32,
        owner: &mut (impl ThreatExecutionOwner + ?Sized),
    ) -> Result<()> {
        let mut index = 0;
        while let Some(guid) = self.manager(source)?.online.get(index).map(|r| r.guid) {
            if owner.threat_lookup(source, guid)?.is_some()
                && self.modify_percent(source, guid, percent, owner)?
            {
                return Err(refuse(
                    "execution.threat.event_ai_erased_iterator_undefined",
                ));
            }
            index += 1;
        }
        Ok(())
    }
    pub fn set_suppressed(
        &mut self,
        source: ObjectHandle,
        guid: u64,
        state: bool,
        owner: &mut (impl ThreatExecutionOwner + ?Sized),
    ) -> Result<()> {
        let changed = {
            let reference = self
                .manager_mut(source)?
                .reference_mut(guid)
                .ok_or_else(|| refuse("execution.threat.reference_missing"))?;
            let changed = reference.suppressed != state;
            reference.suppressed = state;
            reference.suppressibility_toggle = false;
            changed
        };
        if changed {
            self.manager_mut(source)?.dirty = true;
            let mut index = 0;
            while let Some(reference) = self.manager(source)?.online.get(index) {
                let (other, target, suppressed) =
                    (reference.guid, reference.target, reference.suppressed);
                if other != guid
                    && suppressed
                    && !owner.suppressed_target(
                        source,
                        target.ok_or_else(|| refuse("execution.threat.online_null_target"))?,
                    )?
                {
                    self.set_suppressed(source, other, false, owner)?;
                }
                index += 1;
            }
        }
        Ok(())
    }
    pub fn suppress(
        &mut self,
        source: ObjectHandle,
        guid: u64,
        owner: &mut (impl ThreatExecutionOwner + ?Sized),
    ) -> Result<()> {
        if self.manager(source)?.online.iter().any(|r| r.guid == guid) {
            self.set_suppressed(source, guid, true, owner)?;
            self.manager_mut(source)?
                .reference_mut(guid)
                .unwrap()
                .suppressibility_toggle = true;
        }
        Ok(())
    }
    pub fn taunt_update(&mut self, source: ObjectHandle, casters: &[u64]) -> Result<()> {
        let manager = self.manager_mut(source)?;
        for reference in &mut manager.online {
            if reference.taunt != u32::MAX {
                reference.taunt = casters
                    .iter()
                    .rposition(|guid| *guid == reference.guid)
                    .map_or(0, |i| (i as u32).wrapping_add(1));
            }
        }
        manager.dirty = true;
        Ok(())
    }
    pub fn fixate(
        &mut self,
        source: ObjectHandle,
        guid: Option<u64>,
        casters: &[u64],
    ) -> Result<()> {
        for reference in &mut self.manager_mut(source)?.online {
            if Some(reference.guid) == guid {
                reference.taunt = u32::MAX;
            } else if reference.taunt == u32::MAX {
                reference.taunt = 0;
            }
        }
        self.taunt_update(source, casters)
    }
    pub fn select(
        &mut self,
        source: ObjectHandle,
        owner: &mut (impl ThreatExecutionOwner + ?Sized),
    ) -> Result<Option<ObjectHandle>> {
        let force = owner.ignoring_ranged(source)?;
        let player = owner.threat_unit(source)?.player;
        let manager = self.manager_mut(source)?;
        if (manager.dirty || force || player) && manager.online.len() > 1 {
            // Refuse a concrete nontransitive equivalence, not every IEEE NaN.
            for nan in manager.online.iter().filter(|r| r.threat.is_nan()) {
                for left in &manager.online {
                    if compare(source, nan, left, force, player, owner)? != Ordering::Equal {
                        continue;
                    }
                    for right in &manager.online {
                        if compare(source, nan, right, force, player, owner)? == Ordering::Equal {
                            require(
                                compare(source, left, right, force, player, owner)?
                                    == Ordering::Equal,
                                "execution.threat.nan_sort_order_undefined",
                            )?;
                        }
                    }
                }
            }
            let mut failure = None;
            manager.online.sort_by(|a, b| {
                if failure.is_some() {
                    return Ordering::Equal;
                }
                match compare(source, a, b, force, player, owner) {
                    Ok(order) => order,
                    Err(error) => {
                        failure = Some(error);
                        Ordering::Equal
                    }
                }
            });
            if let Some(error) = failure {
                return Err(error);
            }
        }
        manager.dirty = false;
        // cpp:340-341 decrements end() even before entering the loop.
        require(
            !manager.online.is_empty(),
            "execution.threat.empty_select_end_decrement_undefined",
        )?;
        let current = manager
            .current
            .and_then(|guid| manager.online.iter().find(|r| r.guid == guid));
        let current_melee = match current {
            Some(r) if force => owner.melee_reach(source, required_target(r)?)?,
            _ => true,
        };
        let mut selected = None;
        for reference in &manager.online {
            let target = required_target(reference)?;
            let melee = owner.melee_reach(source, target)?;
            if let Some(current) = current {
                if current.guid == reference.guid {
                    if force && !current_melee {
                        continue;
                    }
                    selected = Some(reference.guid);
                    break;
                }
                if reference.taunt > current.taunt
                    || (owner.threat_unit(target)?.player
                        && !owner.threat_unit(required_target(current)?)?.player)
                {
                    selected = Some(reference.guid);
                    break;
                }
                if force {
                    if !melee {
                        continue;
                    }
                    if !current_melee {
                        selected = Some(reference.guid);
                        break;
                    }
                }
                if !reference.suppressed && current.suppressed {
                    selected = Some(reference.guid);
                    break;
                }
                let low = ThreatValue::from_bits(0x3f8c_cccd).multiply(current.threat);
                if !reference.threat.greater(low) && !reference.threat.is_nan() && !low.is_nan() {
                    selected = Some(current.guid);
                    break;
                }
                if threat_switch(current.threat, reference.threat, melee) {
                    selected = Some(reference.guid);
                    break;
                }
            } else if !force || melee {
                selected = Some(reference.guid);
                break;
            }
        }
        manager.current = selected;
        Ok(selected
            .and_then(|guid| manager.online.iter().find(|r| r.guid == guid))
            .and_then(|r| r.target))
    }
    /// Safe Unit::SelectHostileTarget call site guard, Unit.cpp:8727-8728.
    pub fn select_if_nonempty(
        &mut self,
        source: ObjectHandle,
        owner: &mut (impl ThreatExecutionOwner + ?Sized),
    ) -> Result<Option<ObjectHandle>> {
        if self.manager(source)?.online.is_empty() {
            return Ok(None);
        }
        self.select(source, owner)
    }
    pub fn set_current(&mut self, source: ObjectHandle, guid: Option<u64>) -> Result<()> {
        let manager = self.manager_mut(source)?;
        manager.current = guid.filter(|id| manager.online.iter().any(|r| r.guid == *id));
        Ok(())
    }
    /// HostileRefManager.cpp:72-99 modifies reverse refs, including offline refs.
    pub fn hostile_percent(
        &mut self,
        target: ObjectHandle,
        percent: i32,
        owner: &mut (impl ThreatExecutionOwner + ?Sized),
    ) -> Result<()> {
        let mut index = 0;
        while let Some(source) = self.hated_by(target).get(index).copied() {
            let guid = owner.threat_unit(target)?.guid;
            let before = self
                .manager(source)?
                .reference(guid)
                .ok_or_else(|| refuse("execution.threat.reverse_link_missing"))?
                .threat;
            self.add_existing(source, guid, before.percent_delta(percent), owner)?;
            index += 1;
        }
        Ok(())
    }
    pub fn temporary_fade(
        &mut self,
        target: ObjectHandle,
        amount: ThreatValue,
        apply: bool,
        owner: &mut (impl ThreatExecutionOwner + ?Sized),
    ) -> Result<()> {
        let mut index = 0;
        while let Some(source) = self.hated_by(target).get(index).copied() {
            let guid = owner.threat_unit(target)?.guid;
            let reference = self
                .manager_mut(source)?
                .reference_mut(guid)
                .ok_or_else(|| refuse("execution.threat.reverse_link_missing"))?;
            let delta = if apply {
                let minimum = reference.threat.negative();
                let delta = if minimum.less(amount) {
                    amount
                } else {
                    minimum
                };
                reference.fade = delta;
                delta
            } else {
                reference.fade.negative()
            };
            self.add_existing(source, guid, delta, owner)?;
            if !apply {
                self.manager_mut(source)?.reference_mut(guid).unwrap().fade = ThreatValue::ZERO;
            }
            index += 1;
        }
        Ok(())
    }
    pub fn hostile_online(
        &mut self,
        target: ObjectHandle,
        online: bool,
        owner: &mut (impl ThreatExecutionOwner + ?Sized),
    ) -> Result<()> {
        let guid = owner.threat_unit(target)?.guid;
        let mut index = 0;
        while let Some(source) = self.hated_by(target).get(index).copied() {
            self.set_online(source, guid, online)?;
            index += 1;
        }
        Ok(())
    }
    /// HostileRefManager.cpp:39-68 captures eligible haters before distributing.
    #[allow(clippy::too_many_arguments)]
    pub fn threat_assist(
        &mut self,
        target: ObjectHandle,
        helper: ObjectHandle,
        amount: ThreatValue,
        spell: Option<HelpfulThreatSpell>,
        single: bool,
        ignore_timer: bool,
        owner: &mut (impl ThreatExecutionOwner + ?Sized),
    ) -> Result<()> {
        let spell = spell.ok_or_else(|| refuse("execution.threat.assist_null_spell_undefined"))?;
        if spell.no_initial_threat
            || spell.spell.no_threat
            || !owner.can_enter_combat(target)?
            || !owner.can_enter_combat(helper)?
        {
            return Ok(());
        }
        if spell.no_helpful_threat {
            return Ok(());
        }
        if self.hated_by(target).is_empty() {
            return Ok(());
        }
        let mut valid = Vec::new();
        for source in self.hated_by(target) {
            if !owner.crowd_controlled(*source)?
                && *source != helper
                && owner.can_attack(*source, helper)?
            {
                valid.push(*source);
            }
        }
        let size = if single { 1 } else { valid.len() as u32 };
        let per_target = amount.divide(ThreatValue::from_u32(size));
        for source in valid {
            self.add(
                source,
                Some(helper),
                per_target,
                false,
                spell.school,
                Some(spell.spell),
                owner,
            )?;
            if !ignore_timer {
                owner.combat_timer(helper, source)?;
            }
        }
        Ok(())
    }
    /// Diagnostic inventory version one. F7 restore deliberately remains unavailable.
    pub fn validate(&self, owner: &mut (impl ThreatExecutionOwner + ?Sized)) -> Result<()> {
        for (source, manager) in &self.managers {
            owner.threat_unit(*source)?;
            let mut guids = std::collections::BTreeSet::new();
            for reference in manager.online.iter().chain(&manager.offline) {
                require(
                    guids.insert(reference.guid),
                    "execution.threat.duplicate_guid",
                )?;
                if let Some(target) = reference.target {
                    require(
                        owner.threat_unit(target)?.guid == reference.guid
                            && self
                                .hated_by(target)
                                .iter()
                                .filter(|s| **s == *source)
                                .count()
                                == 1,
                        "execution.threat.reference_generation_or_reverse",
                    )?;
                }
            }
            require(
                manager.online.iter().all(|r| r.target.is_some()),
                "execution.threat.online_null_target",
            )?;
            require(
                manager
                    .current
                    .is_none_or(|guid| manager.online.iter().any(|r| r.guid == guid)),
                "execution.threat.current_not_online",
            )?;
        }
        for (target, sources) in &self.hated_by {
            for source in sources {
                require(
                    self.manager(*source)?
                        .online
                        .iter()
                        .chain(&self.manager(*source)?.offline)
                        .any(|r| r.target == Some(*target)),
                    "execution.threat.orphan_reverse",
                )?;
            }
        }
        Ok(())
    }
}
fn required_target(reference: &SourceHostileReference) -> Result<ObjectHandle> {
    reference
        .target
        .ok_or_else(|| refuse("execution.threat.online_null_target"))
}
fn compare(
    source: ObjectHandle,
    a: &SourceHostileReference,
    b: &SourceHostileReference,
    force: bool,
    player: bool,
    owner: &mut (impl ThreatExecutionOwner + ?Sized),
) -> Result<Ordering> {
    if player {
        let left = owner.threat_unit(required_target(a)?)?.player;
        let right = owner.threat_unit(required_target(b)?)?.player;
        if left != right {
            return Ok(right.cmp(&left));
        }
        let left = owner.can_attack(source, required_target(a)?)?;
        let right = owner.can_attack(source, required_target(b)?)?;
        if left != right {
            return Ok(right.cmp(&left));
        }
    }
    if a.taunt != b.taunt {
        return Ok(b.taunt.cmp(&a.taunt));
    }
    if force {
        let left = owner.melee_reach(source, required_target(a)?)?;
        let right = owner.melee_reach(source, required_target(b)?)?;
        if left != right {
            return Ok(right.cmp(&left));
        }
    }
    if a.suppressed != b.suppressed {
        return Ok(a.suppressed.cmp(&b.suppressed));
    }
    Ok(if a.threat.greater(b.threat) {
        Ordering::Less
    } else if b.threat.greater(a.threat) {
        Ordering::Greater
    } else {
        Ordering::Equal
    })
}
