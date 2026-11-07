// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from CMaNGOS src/game/Tools/Formulas.h at
// 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f. See ../PROVENANCE.md.
// Original work: CMaNGOS Project contributors; see ../licenses/AUTHORS.md.
// Upstream grants GPL version 2 or (at your option) any later version.
// This modified Rust translation is distributed under GPL version 3 or later.
// This program is distributed WITHOUT ANY WARRANTY; without even the implied
// warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
//! Level-based XP formulas only. Creature flags, damage attribution, elite/rate
//! modifiers, rested/group XP, quest rewards and level-up tables are not included.

/// A nonzero level, bounded to the source protocol's byte-sized level domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Level(u8);

impl Level {
    pub const fn new(value: u8) -> Option<Self> {
        if value == 0 {
            None
        } else {
            Some(Self(value))
        }
    }

    pub const fn get(self) -> u8 {
        self.0
    }
}

/// Exact nonnegative XP ratio; fields are private to preserve the nonzero denominator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct XpRatio {
    numerator: u32,
    denominator: u32,
}

impl XpRatio {
    pub const fn numerator(self) -> u32 {
        self.numerator
    }
    pub const fn denominator(self) -> u32 {
        self.denominator
    }

    /// Round the exact ratio to nearest integer, with ties to even.
    /// This specifies deterministic behavior; upstream's intermediate f32 rounding
    /// occasionally differs. It is not the complete CMaNGOS Gain function.
    pub const fn round_ties_even(self) -> u32 {
        let quotient = self.numerator / self.denominator;
        let remainder = self.numerator % self.denominator;
        let above_half = remainder * 2 > self.denominator;
        let odd_tie = remainder * 2 == self.denominator && quotient % 2 == 1;
        quotient + (above_half || odd_tie) as u32
    }
}

pub const fn quest_green_range(level: Level) -> u32 {
    match level.0 / 5 {
        0 | 1 => 4,
        2 | 3 => 5,
        4 | 5 => 6,
        6 | 7 => 7,
        8 => 8,
        9 => 9,
        10 => 10,
        11 => 11,
        _ => 12,
    }
}

pub const fn is_trivial(unit: Level, target: Level) -> bool {
    unit.0 > target.0 && (unit.0 - target.0) as u32 > quest_green_range(unit)
}

pub const fn gray_level(level: Level) -> u32 {
    let level = level.0 as u32;
    if level <= 5 {
        0
    } else if level <= 39 {
        level - 5 - level / 10
    } else if level <= 59 {
        level - 1 - level / 5
    } else {
        level - 9
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XpColor {
    Red,
    Orange,
    Yellow,
    Green,
    Gray,
}

/// Preserves source unsigned subtraction, including its level-1 color quirk.
/// The XP eligibility threshold uses `is_trivial`, not this display helper.
pub const fn color_code(player: Level, mob: Level) -> XpColor {
    let player_u32 = player.0 as u32;
    let mob_u32 = mob.0 as u32;
    if mob_u32 >= player_u32 + 5 {
        XpColor::Red
    } else if mob_u32 >= player_u32 + 3 {
        XpColor::Orange
    } else if mob_u32 >= player_u32.wrapping_sub(2) {
        XpColor::Yellow
    } else if mob_u32 > gray_level(player) {
        XpColor::Green
    } else {
        XpColor::Gray
    }
}

pub const fn zero_difference(level: Level) -> u32 {
    match level.0 {
        1..=7 => 5,
        8..=9 => 6,
        10..=11 => 7,
        12..=15 => 8,
        16..=19 => 9,
        20..=29 => 11,
        30..=39 => 12,
        40..=44 => 13,
        45..=49 => 14,
        50..=54 => 15,
        55..=59 => 16,
        _ => 17,
    }
}

/// Exact rational reformulation of CMaNGOS XP::BaseGain, before any modifiers.
/// No floating-point math is used in authoritative calculations.
pub const fn base_gain(unit: Level, mob: Level) -> XpRatio {
    let base = unit.0 as u32 * 5 + 45;
    if mob.0 >= unit.0 {
        let difference = (mob.0 - unit.0) as u32;
        let difference = if difference > 4 { 4 } else { difference };
        XpRatio {
            numerator: base * (20 + difference),
            denominator: 20,
        }
    } else if !is_trivial(unit, mob) {
        let zero = zero_difference(unit);
        let difference = (unit.0 - mob.0) as u32;
        XpRatio {
            numerator: base * (zero - difference),
            denominator: zero,
        }
    } else {
        XpRatio {
            numerator: 0,
            denominator: 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn l(value: u8) -> Level {
        Level::new(value).unwrap()
    }

    #[test]
    fn boundaries_and_ties() {
        assert!(Level::new(0).is_none());
        assert_eq!(base_gain(l(1), l(1)).round_ties_even(), 50);
        assert_eq!(base_gain(l(1), l(2)).round_ties_even(), 52);
        assert_eq!(base_gain(l(1), l(4)).round_ties_even(), 58);
        assert_eq!(base_gain(l(60), l(1)).round_ties_even(), 0);
        assert_eq!(base_gain(l(60), l(64)), base_gain(l(60), l(255)));
        assert_eq!(color_code(l(1), l(1)), XpColor::Green);
    }

    #[test]
    fn complete_byte_domain_is_safe_and_bounded() {
        for unit in 1..=255 {
            for mob in 1..=255 {
                let gain = base_gain(l(unit), l(mob));
                assert!(gain.denominator() > 0);
                assert!(gain.round_ties_even() <= 1584);
                assert_eq!(gain.numerator() == 0, is_trivial(l(unit), l(mob)));
                assert!((color_code(l(unit), l(mob)) as u8) <= XpColor::Gray as u8);
            }
        }
    }
}
