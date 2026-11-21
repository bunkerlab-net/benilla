// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 Spell.cpp, SpellAuras.cpp, UnitAuraProcHandler.cpp.
use super::*;
impl SpellAuthoritySource {
    pub(super) fn validate_frost(&self) -> Result<(), NativeError> {
        let w = &self.words;
        if w.len() != 173 || !matches!(w[0], 168 | 6136) {
            return Err(NativeError::InvalidProfile);
        }
        self.hooks.validate()?;
        self.hooks
            .spell_script_names
            .empty("Frost Armor script source closure")?;
        let armor = w[0] == 168;
        let expected = [
            (1, 4),
            (4, 1),
            (5, if armor { 0 } else { 11 }),
            (6, if armor { NOT_SHAPESHIFT } else { 0 }),
            (7, if armor { 0 } else { 0x488 }),
            (21, if armor { 8 } else { 0 }),
            (24, if armor { 40 } else { 0 }),
            (25, if armor { 100 } else { 101 }),
            (31, 0),
            (36, if armor { 1 } else { 4 }),
            (58, u32::MAX),
            (59, u32::MAX),
            (61, 6),
            (62, 6),
            (63, 0),
            (82, if armor { 1 } else { 6 }),
            (83, if armor { 1 } else { 6 }),
            (84, 0),
            (91, if armor { 22 } else { 138 }),
            (92, if armor { 42 } else { 33 }),
            (93, 0),
            (106, u32::from(armor)),
            (107, 0),
            (108, 0),
            (109, 0),
            (110, if armor { 6136 } else { 0 }),
            (111, 0),
            (160, 3),
            (161, if armor { 0x208_0000 } else { 0x10_0000 }),
            (162, 0),
            (164, 1),
            (165, 1),
        ];
        if expected.into_iter().any(|(i, v)| w[i] != v)
            || self.server_side != 0
            || self.server_attributes != 0
            || self.bonus_coefficients != [0; 3]
            || self.attack_power_coefficients != [0; 3]
            || self.facing_flags != 0
            || self.category_flags != 0
        {
            return Err(NativeError::UnsupportedMechanic(
                "Frost Armor source morphology",
            ));
        }
        for (i, v) in w.iter().enumerate() {
            if *v != 0
                && !matches!(i,0..=7 | 18..=21 | 24..=25 | 27..=30 | 32 | 36 | 38 |
                58..=59 | 61..=69 | 76..=78 | 82..=84 | 91..=93 | 106..=111 | 115..=158 |
                160..=162 | 164..=169)
            {
                return Err(NativeError::UnsupportedMechanic(
                    "unexecuted Frost Armor numeric field",
                ));
            }
        }
        if w[27] != 0
            || w[28] > 60
            || w[29] > 60
            || w[32] > 1_000_000
            || (armor && w[32] == 0)
            || w[156] != 0
            || (!armor && (w[32] != 0 || w[157] != 0 || w[158] != 0))
            || w[19] != 0
            || w[20] != 0
            || w[2] != 0
            || w[18] != self.cast_time.id
            || self.cast_time.base_ms != 0
            || self.cast_time.per_level_ms != 0
            || self.cast_time.minimum_ms != 0
            || self.cast_time.id == 0
            || w[158] > 3_600_000
            || (w[158] != 0 && w[157] == 0)
            || w[36] != self.range.id
            || self.range.minimum != spell::SpellFloat::ZERO
            || self.range.flags != 0
            || (armor && self.range.maximum != spell::SpellFloat::ZERO)
        {
            return Err(NativeError::UnsupportedMechanic(
                "Frost Armor cast and resource source",
            ));
        }
        let d = self
            .duration
            .ok_or(NativeError::AuthorityUnavailable("Frost Armor duration"))?;
        if d.id != w[30] || d.per_level_ms != 0 || d.maximum_ms != d.base_ms {
            return Err(NativeError::UnsupportedMechanic(
                "Frost Armor finite duration",
            ));
        }
        crate::native::frost_armor::duration(self)?;
        for i in 0..3 {
            if !matches!(w[64 + i], 0 | 1)
                || !matches!(w[67 + i], 0 | 1)
                || (i == 2
                    && (w[64 + i] != 0
                        || w[67 + i] != 0
                        || w[76 + i] != 0
                        || w[82 + i] != 0
                        || w[91 + i] != 0))
                || (armor && i == 1 && (w[64 + i] != 0 || w[67 + i] != 0 || w[76 + i] != 0))
                || !matches!(w[167 + i], 0 | 0x3f80_0000)
            {
                return Err(NativeError::UnsupportedMechanic(
                    "Frost Armor deterministic aura amount",
                ));
            }
        }
        crate::native::frost_armor::spec(self, 1)?;
        Ok(())
    }
}
impl SpellAuthorityCatalog {
    pub(in crate::native) fn frost_source(&self, id: u32) -> Option<&SpellAuthoritySource> {
        self.spells.get(&id)
    }
}
