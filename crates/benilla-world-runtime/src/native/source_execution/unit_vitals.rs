// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::{player::SourcePlayer, *};

pub(super) const UNIT_STAND_STATE_STAND: u8 = 0; // Unit.h:97.
const UNIT_STAND_STATE_SLEEP: u8 = 3; // Unit.h:100.

pub(super) fn live_player<'a>(
    players: &[&'a SourcePlayer],
    unit: ObjectHandle,
    missing_owner: &'static str,
) -> Result<&'a SourcePlayer> {
    players
        .iter()
        .copied()
        .find(|player| player.object() == unit)
        .ok_or_else(|| refuse(missing_owner))
}

impl SourceUnitFields {
    // Unit.cpp:10155-10168; Unit.h:97-105.
    pub(super) fn is_standing(&self) -> bool {
        !matches!(self.stand_state, 1..=6 | 8)
    }

    // Unit.cpp:10170-10190. Update the byte before aura removal and the player packet.
    pub(super) fn set_stand_state(
        &mut self,
        objects: &SourceObjects,
        unit: ObjectHandle,
        state: u8,
        acknowledge: bool,
    ) -> Result<()> {
        if !objects.set_update_value(unit, UNIT_FIELD_BYTES_1, &mut self.stand_state, state)? {
            return Ok(());
        }
        if matches!(state, UNIT_STAND_STATE_STAND | UNIT_STAND_STATE_SLEEP) {
            require(
                !self.auras.has_aura_holders(),
                "execution.unit.Unit_10184.RemoveAurasWithInterruptFlags_owner",
            )?;
        }
        if !acknowledge && objects.get(unit)?.kind == SourceObjectKind::Player {
            return Err(refuse("execution.unit.Unit_10190.SendPacket_owner"));
        }
        Ok(())
    }

    // Unit.cpp:9224-9244. Run group checks after the field write, including on equality.
    pub(super) fn set_health(
        &mut self,
        objects: &SourceObjects,
        unit: ObjectHandle,
        value: u32,
        players: &[&SourcePlayer],
    ) -> Result<()> {
        let vitals = self
            .vitals
            .as_mut()
            .ok_or_else(|| refuse("execution.unit.Unit_9226.GetMaxHealth_owner"))?;
        objects.set_update_value(
            unit,
            UNIT_FIELD_HEALTH,
            &mut vitals.health,
            vitals.max_health.min(value),
        )?;
        if objects.get(unit)?.kind == SourceObjectKind::Player {
            let player = live_player(players, unit, "execution.unit.Unit_9235.GetGroup_owner")?;
            return require(
                player.group.is_none(),
                "execution.unit.Unit_9236.SetGroupUpdateFlag_owner",
            );
        }
        if self.control_guids.has_charmer() {
            let charmer = self
                .control_guids
                .get_charmer(objects, unit, None, unbound_pet_loading)?
                .unit();
            if let Some(charmer) = charmer {
                if objects.get(charmer)?.kind == SourceObjectKind::Player {
                    let player =
                        live_player(players, charmer, "execution.unit.Unit_9241.GetGroup_owner")?;
                    require(
                        player.group.is_none(),
                        "execution.unit.Unit_9242.SetGroupUpdateFlag_owner",
                    )?;
                }
            }
        }
        Ok(())
    }

    // Unit.cpp:8062-8090. The casts are signed reinterpretations on this source target.
    pub(super) fn modify_health(
        &mut self,
        objects: &SourceObjects,
        unit: ObjectHandle,
        delta: i32,
        players: &[&SourcePlayer],
    ) -> Result<i32> {
        if delta == 0 {
            return Ok(0);
        }
        let vitals = self
            .vitals
            .as_ref()
            .ok_or_else(|| refuse("execution.unit.Unit_8067.GetHealth_owner"))?;
        let before = vitals.health as i32;
        let after = delta
            .checked_add(before)
            .ok_or_else(|| refuse("execution.unit.Unit_8069.health_sum_undefined"))?;
        if after <= 0 {
            self.set_health(objects, unit, 0, players)?;
            return before
                .checked_neg()
                .ok_or_else(|| refuse("execution.unit.Unit_8073.health_gain_undefined"));
        }
        let maximum = vitals.max_health as i32;
        if after < maximum {
            self.set_health(objects, unit, after as u32, players)?;
            Ok(delta)
        } else {
            self.set_health(objects, unit, maximum as u32, players)?;
            maximum
                .checked_sub(before)
                .ok_or_else(|| refuse("execution.unit.Unit_8087.health_gain_undefined"))
        }
    }
}
