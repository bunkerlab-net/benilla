// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 Spell.cpp:4697-4707; SpellAuras.cpp:3617-3646.
use super::*;
impl SpellAuthoritySource {
    pub(super) fn validate_consumable(&self) -> Result<(), NativeError> {
        let w = &self.words;
        if w.len() != 173 || !matches!(w[0], 430 | 433) {
            return Err(NativeError::InvalidProfile);
        }
        let food = w[0] == 433;
        let expected_scripts = if food {
            vec!["spell_food_animation".to_owned()]
        } else {
            vec![]
        };
        if self.hooks.spell_script_names != SpellSourceRows::Complete(expected_scripts) {
            return Err(NativeError::UnsupportedMechanic(
                "consumable script source closure",
            ));
        }
        let mut hooks = self.hooks.clone();
        hooks.spell_script_names = SpellSourceRows::Complete(vec![]);
        hooks.validate()?;
        let expected = [
            (1, 0),
            (2, if food { 11 } else { 59 }),
            (6, 0x1800_0100),
            (18, 1),
            (19, 0),
            (20, 60_000),
            (21, 1),
            (22, 0x40080),
            (25, 101),
            (27, 0),
            (28, 5),
            (29, if food { 0 } else { 5 }),
            (31, 0),
            (32, 0),
            (36, 1),
            (58, u32::MAX),
            (59, if food { 0 } else { u32::MAX }),
            (61, 6),
            (64, 1),
            (67, 1),
            (76, if food { 16 } else { 41 }),
            (82, 1),
            (91, if food { 84 } else { 85 }),
            (97, if food { 0x3f80_0000 } else { 0 }),
            (166, u32::MAX),
            (167, 0x3f80_0000),
            (168, 0x3f80_0000),
            (169, 0x3f80_0000),
        ];
        if expected.into_iter().any(|(i, v)| w[i] != v)
            || w.iter().enumerate().any(|(i, v)| {
                *v != 0
                    && !matches!(i,
                0..=2|6|18..=22|25|28..=30|36|58..=59|61|64|67|76|82|91|97|
                115..=155|166..=169)
            })
            || self.server_side != 0
            || self.server_attributes != 0
            || self.bonus_coefficients != [0; 3]
            || self.attack_power_coefficients != [0; 3]
            || self.facing_flags != 0
            || self.category_flags != 0
            || self.cast_time.id != w[18]
            || self.cast_time.base_ms != 0
            || self.cast_time.per_level_ms != 0
            || self.cast_time.minimum_ms != 0
            || self.range.id != 1
            || self.range.minimum != spell::SpellFloat::ZERO
            || self.range.maximum != spell::SpellFloat::ZERO
            || self.range.flags != 0
        {
            return Err(NativeError::UnsupportedMechanic(
                "food/drink effective source morphology",
            ));
        }
        let duration = self
            .duration
            .ok_or(NativeError::AuthorityUnavailable("consumable duration"))?;
        if duration.id != w[30]
            || duration.base_ms <= 0
            || duration.base_ms > 3_600_000
            || duration.per_level_ms != 0
            || duration.maximum_ms != duration.base_ms
        {
            return Err(NativeError::UnsupportedMechanic(
                "consumable finite duration",
            ));
        }
        Ok(())
    }
}
impl SpellAuthorityCatalog {
    pub(in crate::native) fn consumable_source(&self, id: u32) -> Option<&SpellAuthoritySource> {
        self.spells.get(&id).filter(|_| matches!(id, 430 | 433))
    }
}
