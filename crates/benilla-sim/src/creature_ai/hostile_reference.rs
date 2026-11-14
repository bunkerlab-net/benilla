// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 ThreatManager.cpp:63-160, 288-326, 466-489, 511-518, 608-652.
//! Owned single-player HostileReference and ThreatManager container projection.
//! Membership is the actual online/offline container; there is no parallel scalar threat.
use super::{AiError, Real, Unsupported};
use serde::{Deserialize, Deserializer, Serialize};

fn required_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::deserialize(deserializer)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Membership {
    Online,
    Offline,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostileReference {
    target_guid: u64,
    valid: bool,
    membership: Membership,
    threat: Real,
}
impl HostileReference {
    pub fn target_guid(&self) -> u64 {
        self.target_guid
    }
    pub fn valid(&self) -> bool {
        self.valid
    }
    pub fn online(&self) -> bool {
        self.membership == Membership::Online
    }
    pub fn membership(&self) -> Membership {
        self.membership
    }
    pub fn threat(&self) -> Real {
        self.threat
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreatEvent {
    ThreatChanged,
    OnlineStatus(bool),
    Removed,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostileReferences {
    #[serde(deserialize_with = "required_option")]
    reference: Option<HostileReference>,
    #[serde(deserialize_with = "required_option")]
    current_victim: Option<u64>,
    dirty: bool,
}
impl Default for HostileReferences {
    fn default() -> Self {
        Self::new()
    }
}
impl HostileReferences {
    pub const fn new() -> Self {
        Self {
            reference: None,
            current_victim: None,
            dirty: false,
        }
    }
    pub fn reference(&self) -> Option<&HostileReference> {
        self.reference.as_ref()
    }
    pub fn current_victim(&self) -> Option<u64> {
        self.current_victim
    }
    pub fn dirty(&self) -> bool {
        self.dirty
    }
    fn check_target(&self, target: u64) -> Result<(), AiError> {
        if target == 0 {
            return Err(AiError::Invalid("hostile reference target"));
        }
        if self
            .reference
            .as_ref()
            .is_some_and(|r| r.target_guid != target)
        {
            return Err(AiError::Unsupported(Unsupported::MultipleThreatTargets));
        }
        Ok(())
    }
    pub fn add_threat(
        &mut self,
        target: u64,
        amount: Real,
        offline: bool,
    ) -> Result<Vec<ThreatEvent>, AiError> {
        self.check_target(target)?;
        let created = self.reference.is_none();
        let before = self.reference.as_ref().map_or(Real::ZERO, |r| r.threat);
        let tentative = before.add(amount)?;
        let amount = if tentative.less(Real::ZERO) {
            Real::ZERO.sub(before)?
        } else {
            amount
        };
        let threat = before.add(amount)?;
        let reference = self.reference.get_or_insert(HostileReference {
            target_guid: target,
            valid: true,
            membership: Membership::Online,
            threat: Real::ZERO,
        });
        reference.threat = threat;
        let mut events = Vec::new();
        if amount != Real::ZERO {
            if (self.current_victim == Some(target) && amount.less(Real::ZERO))
                || (self.current_victim != Some(target) && amount.greater(Real::ZERO))
            {
                self.dirty = true;
            }
            events.push(ThreatEvent::ThreatChanged);
        }
        // Existing online OR offline references are reused without re-evaluating status.
        if created && offline {
            self.set_online(false, &mut events);
        }
        Ok(events)
    }
    fn set_online(&mut self, online: bool, events: &mut Vec<ThreatEvent>) {
        let Some(reference) = self.reference.as_mut() else {
            return;
        };
        if reference.online() == online {
            return;
        }
        reference.membership = if online {
            Membership::Online
        } else {
            Membership::Offline
        };
        if !online && self.current_victim == Some(reference.target_guid) {
            self.current_victim = None;
            self.dirty = true;
        }
        // With one target there cannot be a different current victim to outrank.
        events.push(ThreatEvent::OnlineStatus(online));
    }
    pub fn update_online_status(
        &mut self,
        target: u64,
        rebound_target: Option<u64>,
        offline: bool,
    ) -> Result<Vec<ThreatEvent>, AiError> {
        self.check_target(target)?;
        if rebound_target.is_some_and(|id| id != target) {
            return Err(AiError::Invalid("hostile reference rebind identity"));
        }
        let Some(reference) = self.reference.as_mut() else {
            return Ok(vec![]);
        };
        if !reference.valid && rebound_target == Some(reference.target_guid) {
            reference.valid = true;
        }
        let online = reference.valid && !offline;
        let mut events = Vec::new();
        self.set_online(online, &mut events);
        Ok(events)
    }
    pub fn invalidate_target(&mut self, target: u64) -> Result<Vec<ThreatEvent>, AiError> {
        self.check_target(target)?;
        if let Some(reference) = self.reference.as_mut() {
            reference.valid = false;
        }
        let mut events = Vec::new();
        self.set_online(false, &mut events);
        Ok(events)
    }
    pub fn remove(&mut self, target: u64) -> Result<Vec<ThreatEvent>, AiError> {
        self.check_target(target)?;
        if self.reference.is_none() {
            return Ok(vec![]);
        }
        // removeReference invalidates its reverse link before REMOVE_FROM_LIST.
        let mut events = self.invalidate_target(target)?;
        self.reference = None;
        if self.current_victim == Some(target) {
            self.current_victim = None;
            self.dirty = true;
        }
        events.push(ThreatEvent::Removed);
        Ok(events)
    }
    /// clearReferences unlinks both containers and clears current without resetting dirty.
    pub fn clear(&mut self) {
        self.reference = None;
        self.current_victim = None;
    }
    /// UpdateContainers runs before getHostileTarget; even a single-element list clears dirty.
    pub fn select_current_victim(&mut self) -> Option<u64> {
        self.dirty = false;
        self.current_victim = self
            .reference
            .as_ref()
            .filter(|r| r.valid && r.online())
            .map(|r| r.target_guid);
        self.current_victim
    }
    pub(super) fn validate(&self, target: u64) -> Result<(), AiError> {
        if let Some(reference) = &self.reference {
            reference.threat.nonnegative("checkpoint hostile threat")?;
            if reference.target_guid != target || (!reference.valid && reference.online()) {
                return Err(AiError::Invalid("checkpoint hostile reference"));
            }
        }
        if self.current_victim.is_some_and(|id| {
            id != target
                || !self
                    .reference
                    .as_ref()
                    .is_some_and(|r| r.valid && r.online())
        }) {
            return Err(AiError::Invalid("checkpoint current hostile victim"));
        }
        Ok(())
    }
}

/// Unrestricted IEEE binary32 threat arithmetic, rounded after each source operation.
/// Unlike the bounded solo `Real`, this also retains subnormals and infinities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThreatValue(u32);
impl ThreatValue {
    pub const ZERO: Self = Self(0);
    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }
    pub const fn bits(self) -> u32 {
        self.0
    }
    fn value(self) -> rustc_apfloat::ieee::Single {
        use rustc_apfloat::Float;
        rustc_apfloat::ieee::Single::from_bits(u128::from(self.0))
    }
    pub fn from_u32(value: u32) -> Self {
        use rustc_apfloat::Float;
        Self(
            rustc_apfloat::ieee::Single::from_u128(u128::from(value))
                .value
                .to_bits() as u32,
        )
    }
    pub fn from_i32(value: i32) -> Self {
        use rustc_apfloat::Float;
        Self(
            rustc_apfloat::ieee::Single::from_i128(i128::from(value))
                .value
                .to_bits() as u32,
        )
    }
    pub fn add_value(self, other: Self) -> Self {
        use rustc_apfloat::Float;
        Self((self.value() + other.value()).value.to_bits() as u32)
    }
    pub fn multiply(self, other: Self) -> Self {
        use rustc_apfloat::Float;
        Self((self.value() * other.value()).value.to_bits() as u32)
    }
    pub fn divide(self, other: Self) -> Self {
        use rustc_apfloat::Float;
        Self((self.value() / other.value()).value.to_bits() as u32)
    }
    pub fn negative(self) -> Self {
        Self(self.0 ^ 0x8000_0000)
    }
    pub fn greater(self, other: Self) -> bool {
        self.value() > other.value()
    }
    pub fn less(self, other: Self) -> bool {
        self.value() < other.value()
    }
    pub fn is_zero(self) -> bool {
        self.0 & 0x7fff_ffff == 0
    }
    pub fn is_nan(self) -> bool {
        self.0 & 0x7fff_ffff > 0x7f80_0000
    }
    /// ThreatManager.h:68-72, multiply before divide, with exact -100 cancellation.
    pub fn percent_delta(self, percent: i32) -> Self {
        if percent == -100 {
            self.negative()
        } else {
            self.multiply(Self::from_i32(percent))
                .divide(Self::from_u32(100))
        }
    }
}

/// HostileReference::addThreat, ThreatManager.cpp:108-123.
/// Returns both the resulting amount and the clamped event delta.
pub fn add_threat_value(before: ThreatValue, mut delta: ThreatValue) -> (ThreatValue, ThreatValue) {
    if delta.add_value(before).less(ThreatValue::ZERO) {
        delta = before.negative();
    }
    (before.add_value(delta), delta)
}

/// ThreatManager.cpp:398-406, strict binary32 110%/130% comparisons.
pub fn threat_switch(current: ThreatValue, challenger: ThreatValue, melee: bool) -> bool {
    let low = ThreatValue::from_bits(0x3f8c_cccd).multiply(current);
    if !challenger.greater(low) {
        return false;
    }
    challenger.greater(ThreatValue::from_bits(0x3fa6_6666).multiply(current)) || melee
}

#[cfg(test)]
mod threat_tests {
    use super::*;

    #[test]
    fn source_binary32_switch_boundaries_are_strict() {
        let current = ThreatValue::from_u32(100);
        // ThreatManager.cpp:398, binary32 1.1f * 100.f rounds to 110.f.
        let low = ThreatValue::from_bits(0x42dc_0000);
        assert!(!threat_switch(current, low, true));
        assert!(threat_switch(
            current,
            ThreatValue::from_bits(0x42dc_0001),
            true
        ));
        assert!(!threat_switch(current, ThreatValue::from_u32(130), false));
        assert!(threat_switch(
            current,
            ThreatValue::from_bits(0x4302_0001),
            false
        ));
    }

    #[test]
    fn source_percent_preserves_operation_order_exact_cancel_and_ieee_values() {
        let before = ThreatValue::from_u32(7);
        let (zero, delta) = add_threat_value(before, before.percent_delta(-100));
        assert!(zero.is_zero());
        assert_eq!(delta.bits(), 0xc0e00000);
        let (clamped, delta) = add_threat_value(before, ThreatValue::from_i32(-9));
        assert!(clamped.is_zero());
        assert_eq!(delta.bits(), 0xc0e00000);
        assert_eq!(
            ThreatValue::from_bits(1)
                .add_value(ThreatValue::from_bits(1))
                .bits(),
            2
        );
        assert_eq!(
            ThreatValue::from_bits(0x7f7fffff)
                .multiply(ThreatValue::from_u32(2))
                .bits(),
            0x7f800000
        );
        assert_eq!(
            ThreatValue::from_u32(100).percent_delta(10).bits(),
            0x41200000
        );
    }
}
