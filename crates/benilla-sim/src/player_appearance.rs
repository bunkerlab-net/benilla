// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Project contributors.
// Source grant GPL-2.0-or-later; Rust translation GPL-3.0-or-later.
//! Pinned CMaNGOS 8ec338a1 Player::ValidateAppearance and DBCStores lookups.
//! This stricter loaded-table subset rejects ambiguous/aliased records. It does
//! not replace the client's dial ranges or prove server race/class eligibility.
use benilla_formats::CharCreateCatalog;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Untrusted requested dial values, not evidence that a row exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativePlayerAppearanceChoices {
    pub skin: u8,
    pub face: u8,
    pub hair_style: u8,
    pub hair_color: u8,
    pub facial_hair: u8,
}
/// Only the complete loaded catalog can construct this source-row selection.
/// Serialize is provided for enclosing identity bindings, deliberately not Deserialize.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativePlayerAppearanceSelection {
    policy: &'static str,
    race: u8,
    gender: u8,
    choices: NativePlayerAppearanceChoices,
    sections: Vec<[u32; 10]>,
    facial_style: [u32; 9],
}
const POLICY: &str = "cmangos-8ec338a1-loaded-player-appearance-v1";
fn excludes_facial_section(race: u8, gender: u8) -> bool {
    race == 6 || (gender == 1 && race != 4 && race != 5)
}
impl NativePlayerAppearanceSelection {
    pub fn race(&self) -> u8 {
        self.race
    }
    pub fn gender(&self) -> u8 {
        self.gender
    }
    pub fn choices(&self) -> NativePlayerAppearanceChoices {
        self.choices
    }
    pub fn validate(&self) -> bool {
        if self.policy != POLICY || !(1..=8).contains(&self.race) || self.gender > 1 {
            return false;
        }
        let expected = keys(self.race, self.gender, self.choices);
        if self.sections.len() != expected.len() {
            return false;
        }
        for (row, key) in self.sections.iter().zip(expected) {
            if [row[1], row[2], row[3], row[4], row[5]] != key || row[9] & 1 != 0 {
                return false;
            }
        }
        self.facial_style[..3]
            == [
                u32::from(self.race),
                u32::from(self.gender),
                u32::from(self.choices.facial_hair),
            ]
    }
}
fn keys(race: u8, gender: u8, c: NativePlayerAppearanceChoices) -> Vec<[u32; 5]> {
    let row = |ty, variation, color| {
        [
            u32::from(race),
            u32::from(gender),
            ty,
            u32::from(variation),
            u32::from(color),
        ]
    };
    let mut keys = vec![
        row(0, 0, c.skin),
        row(1, c.face, c.skin),
        row(3, c.hair_style, c.hair_color),
    ];
    if !excludes_facial_section(race, gender) {
        keys.push(row(2, c.facial_hair, c.hair_color));
    }
    keys
}

impl NativePlayerAppearanceSelection {
    /// Only the exact loaded DBC record owner supplies source rows. Request bytes
    /// and this receipt do not authenticate who selected a character.
    pub fn select(
        catalog: &CharCreateCatalog,
        race: u8,
        gender: u8,
        choices: NativePlayerAppearanceChoices,
    ) -> Option<Self> {
        let rows = catalog.native_appearance_rows()?;
        select_rows(rows.sections(), rows.facial_styles(), race, gender, choices)
    }
}
fn select_rows(
    raw_sections: &[[u32; 10]],
    facial: &[[u32; 9]],
    race: u8,
    gender: u8,
    choices: NativePlayerAppearanceChoices,
) -> Option<NativePlayerAppearanceSelection> {
    // Explicit supported subset before source packed-key narrowing/OR. Unknown
    // aliases and duplicate indexed records cannot silently supply a winner.
    if !(1..=8).contains(&race)
        || gender > 1
        || raw_sections
            .iter()
            .any(|r| r[1] > 32 || r[2..=5].iter().any(|v| *v > 255))
        || facial
            .iter()
            .any(|r| r[0] > 32 || r[1..=2].iter().any(|v| *v > 255))
    {
        return None;
    }
    let mut ids = HashSet::new();
    if raw_sections.iter().any(|r| !ids.insert(r[0])) {
        return None;
    }
    let mut sections = Vec::new();
    for key in keys(race, gender, choices) {
        let mut matching = raw_sections
            .iter()
            .filter(|r| [r[1], r[2], r[3], r[4], r[5]] == key);
        let row = *matching.next()?;
        if matching.next().is_some() || row[9] & 1 != 0 {
            return None;
        }
        sections.push(row);
    }
    let key = [
        u32::from(race),
        u32::from(gender),
        u32::from(choices.facial_hair),
    ];
    let mut matching = facial.iter().filter(|r| r[..3] == key);
    let facial_style = *matching.next()?;
    if matching.next().is_some() {
        return None;
    }
    let selected = NativePlayerAppearanceSelection {
        policy: POLICY,
        race,
        gender,
        choices,
        sections,
        facial_style,
    };
    selected.validate().then_some(selected)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn choices() -> NativePlayerAppearanceChoices {
        NativePlayerAppearanceChoices {
            skin: 1,
            face: 2,
            hair_style: 3,
            hair_color: 4,
            facial_hair: 5,
        }
    }
    fn rows(race: u8, gender: u8) -> (Vec<[u32; 10]>, Vec<[u32; 9]>) {
        let sections = keys(race, gender, choices())
            .into_iter()
            .enumerate()
            .map(|(i, k)| [(i + 1) as u32, k[0], k[1], k[2], k[3], k[4], 0, 0, 0, 0])
            .collect();
        (
            sections,
            vec![[u32::from(race), u32::from(gender), 5, 0, 0, 0, 0, 0, 0]],
        )
    }
    #[test]
    fn exact_coupled_source_rows_preserve_all_requested_bytes() {
        let (sections, facial) = rows(1, 0);
        let proof = select_rows(&sections, &facial, 1, 0, choices()).unwrap();
        assert!(proof.validate());
        assert_eq!(proof.choices(), choices());
        for row in 0..sections.len() {
            let mut bad = sections.clone();
            bad[row][5] ^= 1;
            assert!(select_rows(&bad, &facial, 1, 0, choices()).is_none());
        }
        let mut sparse = choices();
        sparse.face = 1;
        assert!(select_rows(&sections, &facial, 1, 0, sparse).is_none());
    }
    #[test]
    fn facial_section_exceptions_never_remove_style_requirement() {
        for race in 1..=8 {
            for gender in 0..=1 {
                let (sections, facial) = rows(race, gender);
                assert_eq!(
                    sections.len(),
                    if excludes_facial_section(race, gender) {
                        3
                    } else {
                        4
                    }
                );
                assert!(select_rows(&sections, &facial, race, gender, choices()).is_some());
                assert!(select_rows(&sections, &[], race, gender, choices()).is_none());
                let nofacial: Vec<_> = sections.iter().copied().filter(|r| r[3] != 2).collect();
                assert_eq!(
                    select_rows(&nofacial, &facial, race, gender, choices()).is_some(),
                    excludes_facial_section(race, gender)
                );
            }
        }
    }
    #[test]
    fn unavailable_duplicate_and_fullwidth_alias_rows_fail_closed() {
        let (sections, facial) = rows(1, 0);
        for column in 1..=5 {
            let mut bad = sections.clone();
            bad[0][column] += 256;
            assert!(select_rows(&bad, &facial, 1, 0, choices()).is_none());
        }
        let mut bad = sections.clone();
        bad[0][9] = 1;
        assert!(select_rows(&bad, &facial, 1, 0, choices()).is_none());
        let mut bad = sections.clone();
        bad[1][0] = bad[0][0];
        assert!(select_rows(&bad, &facial, 1, 0, choices()).is_none());
        let mut bad = sections.clone();
        let mut extra = bad[0];
        extra[0] = 99;
        bad.push(extra);
        assert!(select_rows(&bad, &facial, 1, 0, choices()).is_none());
        let mut bad = facial.clone();
        bad.push(bad[0]);
        assert!(select_rows(&sections, &bad, 1, 0, choices()).is_none());
        let mut bad = facial.clone();
        bad[0][2] += 256;
        assert!(select_rows(&sections, &bad, 1, 0, choices()).is_none());
    }
}
