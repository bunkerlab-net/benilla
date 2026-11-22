// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! ObjectMgr.cpp:400-575,6926-7194, immutable SQLStorage products.
use super::*;
use std::collections::{BTreeMap, HashMap};

/// SQLStorages.cpp:21-22,29-30. Float words retain their f32 bits; strings retain C-string bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlTemplateRow<const N: usize, const S: usize> {
    pub ordinal: u64,
    pub words: [u32; N],
    pub strings: [Vec<u8>; S],
}
pub type LoadedCreatureInfo = SqlTemplateRow<99, 3>;
pub type LoadedGameObjectInfo = SqlTemplateRow<38, 2>;
impl LoadedGameObjectInfo {
    pub fn template(&self) -> GameObjectTemplate {
        GameObjectTemplate {
            entry: self.words[0],
            kind: self.words[1],
            display: self.words[2],
            faction: self.words[5],
            flags: self.words[6],
            extra_flags: self.words[7],
            scale_bits: self.words[8],
            data: self.words[9..33].try_into().unwrap(),
            string_id: self.words[36],
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TemplateLookup {
    Faction,
    Model,
    CreatureType,
    Family,
    PetSpell,
    Equipment,
    StringId,
    Lock,
    SpellFocus,
    PageText,
    TaxiPath,
    Spell,
}
pub trait TemplateValidationOwner {
    fn contains(&mut self, domain: TemplateLookup, id: u32) -> Result<bool>;
    fn display_scale(&mut self, id: u32) -> Result<Option<u32>>;
    fn class_stats(&mut self, level: u32, class: u32) -> Result<bool>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum TemplateNoticeKind {
    Fixup,
    Diagnostic,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TemplateNotice {
    pub entry: u32,
    pub line: u32,
    pub field: usize,
    pub kind: TemplateNoticeKind,
}
fn notice(out: &mut Vec<TemplateNotice>, entry: u32, line: u32, field: usize, fixed: bool) {
    out.push(TemplateNotice {
        entry,
        line,
        field,
        kind: if fixed {
            TemplateNoticeKind::Fixup
        } else {
            TemplateNoticeKind::Diagnostic
        },
    });
}
fn lookup(
    owner: &mut impl TemplateValidationOwner,
    domain: TemplateLookup,
    id: u32,
    line: u32,
) -> Result<bool> {
    owner.contains(domain, id).map_err(|error| {
        refuse(&format!(
            "execution.templates.ObjectMgr_{line}.{domain:?}_owner.{error}"
        ))
    })
}
#[derive(Debug)]
pub struct LoadedCreatureTemplates {
    rows: Vec<LoadedCreatureInfo>,
    index: Vec<Option<usize>>,
    notices: Vec<TemplateNotice>,
}
impl LoadedCreatureTemplates {
    pub fn load(
        rows: Vec<LoadedCreatureInfo>,
        owner: &mut impl TemplateValidationOwner,
    ) -> Result<Self> {
        let max = rows.iter().map(|row| row.words[0]).max().unwrap_or(0);
        require(
            max != u32::MAX,
            "execution.templates.SQLStorageImpl_317.max_entry_index_undefined",
        )?;
        require(
            max < 16_777_216,
            "execution.templates.creature_index_budget",
        )?;
        let mut this = Self {
            index: vec![None; max as usize + 1],
            rows,
            notices: Vec::new(),
        };
        for (slot, row) in this.rows.iter().enumerate() {
            this.index[row.words[0] as usize] = Some(slot);
        }
        // SQLStorage indexes overwrite duplicate IDs; entry zero is not validated, ObjectMgr.cpp:406.
        for entry in 1..this.index.len() {
            let Some(slot) = this.index[entry] else {
                continue;
            };
            let w = &mut this.rows[slot].words;
            let entry = w[0];
            if !lookup(owner, TemplateLookup::Faction, w[13], 412)? {
                notice(&mut this.notices, entry, 414, 13, false);
            }
            for (offset, credit) in w[68..70].iter_mut().enumerate() {
                if *credit != 0
                    && this
                        .index
                        .get(*credit as usize)
                        .copied()
                        .flatten()
                        .is_none()
                {
                    *credit = 0;
                    notice(&mut this.notices, entry, 423, 68 + offset, true);
                }
            }
            let mut scale = None;
            for (offset, display) in w[5..9].iter_mut().enumerate() {
                let field = 5 + offset;
                if *display == 0 {
                    continue;
                }
                match owner.display_scale(*display).map_err(|error| {
                    refuse(&format!(
                        "execution.templates.ObjectMgr_435.CreatureDisplayInfo_owner.{error}"
                    ))
                })? {
                    None => {
                        *display = 0;
                        notice(&mut this.notices, entry, 439, field, true);
                    }
                    Some(value) => {
                        if scale.is_none() {
                            scale = Some(value);
                        }
                    }
                }
                if !lookup(owner, TemplateLookup::Model, *display, 444)? {
                    notice(&mut this.notices, entry, 446, field, false);
                }
            }
            if scale.is_none() {
                notice(&mut this.notices, entry, 451, 5, false);
            }
            if w[3] == 0 {
                w[3] = 1;
                notice(&mut this.notices, entry, 456, 3, true);
            }
            if w[3] > w[4] {
                w[4] = w[3];
                notice(&mut this.notices, entry, 462, 4, true);
            }
            for (field, line) in [(3, 468), (4, 474)] {
                if w[field] > 70 {
                    w[field] = 70;
                    notice(&mut this.notices, entry, line, field, true);
                }
            }
            // Zero short-circuits the source shift; nonzero values above 32 are undefined, :477.
            require(
                w[36] <= 32,
                "execution.templates.ObjectMgr_477.UnitClass_shift_undefined",
            )?;
            if !matches!(w[36], 1 | 2 | 8) {
                w[36] = 1;
                notice(&mut this.notices, entry, 480, 36, true);
            }
            for level in w[3]..=w[4] {
                if !owner.class_stats(level, w[36]).map_err(|error| {
                    refuse(&format!(
                        "execution.templates.ObjectMgr_485.creature_classlevelstats_owner.{error}"
                    ))
                })? {
                    notice(&mut this.notices, entry, 487, 36, false);
                    break;
                }
            }
            if w[62] >= 7 {
                w[62] = 0;
                notice(&mut this.notices, entry, 495, 62, true);
            }
            for (field, line) in [(60, 499), (61, 502)] {
                if w[field] == 0 {
                    w[field] = 2000;
                    notice(&mut this.notices, entry, line, field, true);
                }
            }
            if w[20] & 0x10 != 0 && w[80] >= 4 {
                notice(&mut this.notices, entry, 505, 80, false);
            }
            if w[16] != 0 && !lookup(owner, TemplateLookup::CreatureType, w[16], 507)? {
                w[16] = 7;
                notice(&mut this.notices, entry, 510, 16, true);
            }
            if w[15] != 0 && !lookup(owner, TemplateLookup::Family, w[15], 514)? && w[15] != 10 {
                w[15] = 0;
                notice(&mut this.notices, entry, 517, 15, true);
            }
            if !(1..=7).contains(&w[17]) {
                w[17] = 7;
                notice(&mut this.notices, entry, 523, 17, true);
            }
            if w[78] != 0 && !lookup(owner, TemplateLookup::PetSpell, w[78], 528)? {
                notice(&mut this.notices, entry, 530, 78, false);
            }
            if w[79] >= 5 {
                w[79] = 0;
                notice(&mut this.notices, entry, 536, 79, true);
            }
            if w[93] > 0 && !lookup(owner, TemplateLookup::Equipment, w[93], 541)? {
                w[93] = 0;
                notice(&mut this.notices, entry, 544, 93, true);
            }
            if w[85] > 0 && w[20] & 0x80 == 0 {
                notice(&mut this.notices, entry, 551, 85, false);
            }
            if f32::from_bits(w[14]) <= 0.0 {
                w[14] = scale.unwrap_or(1.0f32.to_bits());
                notice(
                    &mut this.notices,
                    entry,
                    if scale.is_some() { 558 } else { 560 },
                    14,
                    true,
                );
            }
            for (field, check, line) in [(91, 563, 566), (92, 569, 572)] {
                if w[field] != 0 && !lookup(owner, TemplateLookup::StringId, w[field], check)? {
                    w[field] = 0;
                    notice(&mut this.notices, entry, line, field, true);
                }
            }
            if w[25..29].iter().any(|v| *v != 0) {
                let old = w[24];
                w[24] = type_flags(old, w[25..29].try_into().unwrap());
                if w[24] != old {
                    notice(&mut this.notices, entry, 575, 24, true);
                }
            }
        }
        Ok(this)
    }
    pub fn get(&self, entry: u32) -> Option<&LoadedCreatureInfo> {
        self.index
            .get(entry as usize)
            .copied()
            .flatten()
            .map(|slot| &self.rows[slot])
    }
    pub fn rows(&self) -> &[LoadedCreatureInfo] {
        &self.rows
    }
    pub fn notices(&self) -> &[TemplateNotice] {
        &self.notices
    }
    pub fn class_shift_ub_reach(rows: &[LoadedCreatureInfo]) -> usize {
        rows.iter()
            .filter(|row| row.words[0] != 0 && row.words[36] > 32)
            .count()
    }
}
/// Every flag enum has fixed uint32 underlying type, CreatureDefines.h:27,65,103,141,282.
fn type_flags(mut flags: u32, words: [u32; 4]) -> u32 {
    const MAPPING: &[(usize, u32, u32)] = &[
        (0, 0x10, 1),
        (0, 0x10000, 4),
        (0, 0x200000, 2),
        (0, 0x1000000, 0x10),
        (0, 0x800000, 8),
        (0, 0x40000000, 0x20),
        (1, 8, 0x40),
        (1, 0x800, 0x80),
        (1, 0x4000, 0x100),
        (1, 0x8000, 0x200),
        (1, 0x40000, 0x800),
        (1, 0x100000, 0x400),
        (1, 0x20000000, 0x1000),
        (1, 0x80000000, 0x2000),
        (2, 8, 0x4000),
        (2, 0x10, 0x8000),
        (2, 0x200, 0x10000),
        (2, 0x2000, 0x100000),
        (2, 0x10000, 0x20000),
        (2, 0x40000, 0x40000),
        (2, 0x400000, 0x80000),
        (2, 0x1000000, 0x200000),
        (2, 0x20000000, 0x400000),
        (3, 8, 0x800000),
        (3, 0x10, 0x1000000),
        (3, 0x20, 0x2000000),
        (3, 0x8000, 0x4000000),
        (3, 0x80000, 0x8000000),
        (3, 0x200000, 0x10000000),
        (3, 0x20000000, 0x20000000),
        (3, 0x40000000, 0x40000000),
        (3, 0x80000000, 0x80000000),
    ];
    for &(word, input, output) in MAPPING {
        if words[word] & input != 0 {
            flags |= output;
        }
    }
    flags
}
#[derive(Debug)]
pub struct LoadedGameObjectTemplates {
    rows: Vec<LoadedGameObjectInfo>,
    index: HashMap<u32, usize>,
    notices: Vec<TemplateNotice>,
    transport_displays: Vec<u32>,
}
impl LoadedGameObjectTemplates {
    pub fn load(
        mut rows: Vec<LoadedGameObjectInfo>,
        owner: &mut impl TemplateValidationOwner,
    ) -> Result<Self> {
        let index: HashMap<_, _> = rows
            .iter()
            .enumerate()
            .map(|(slot, row)| (row.words[0], slot))
            .collect();
        let kinds: Vec<_> = rows.iter().map(|row| row.words[1]).collect();
        let mut notices = Vec::new();
        let mut transport_displays = Vec::new();
        // Hash lookup is separate from raw stored row traversal, ObjectMgr.cpp:7019.
        for row in &mut rows {
            let w = &mut row.words;
            let entry = w[0];
            if f32::from_bits(w[8]) <= 0.0 {
                w[8] = 1.0f32.to_bits();
                notice(&mut notices, entry, 7027, 8, true);
            }
            if w[36] != 0 && !lookup(owner, TemplateLookup::StringId, w[36], 7030)? {
                w[36] = 0;
                notice(&mut notices, entry, 7033, 36, true);
            }
            let kind = w[1];
            let lock = match kind {
                0 | 1 => Some(1),
                2 | 3 | 6 | 10 | 12 | 13 | 24 | 26 => Some(0),
                25 => Some(4),
                _ => None,
            };
            if let Some(n) = lock {
                if w[9 + n] != 0 && !lookup(owner, TemplateLookup::Lock, w[9 + n], 6937)? {
                    notice(&mut notices, entry, 6940, 9 + n, false);
                }
            }
            let trap = match kind {
                1 => Some(3),
                3 => Some(7),
                8 => Some(2),
                10 => Some(12),
                _ => None,
            };
            // Keep each type's diagnostic order, ObjectMgr.cpp:7040-7188.
            if (kind == 3 || kind == 10) && w[12] > 1 {
                notice(&mut notices, entry, 6995, 12, false);
            }
            if kind == 7 && w[10] > 2 {
                w[10] = 0;
                notice(&mut notices, entry, 6976, 10, true);
            }
            if kind == 8 && w[9] != 0 && !lookup(owner, TemplateLookup::SpellFocus, w[9], 7091)? {
                notice(&mut notices, entry, 7092, 9, false);
            }
            if kind == 10 && w[16] != 0 && !lookup(owner, TemplateLookup::PageText, w[16], 7109)? {
                notice(&mut notices, entry, 7110, 16, false);
            }
            let boolean = match kind {
                0 => Some(3),
                1 => Some(4),
                2 | 24 => Some(5),
                10 => Some(11),
                26 => Some(3),
                _ => None,
            };
            if kind == 10 && w[20] > 1 {
                notice(&mut notices, entry, 6985, 20, false);
            }
            if let Some(n) = trap.filter(|n| w[9 + n] != 0) {
                let trap_kind = index.get(&w[9 + n]).map(|&slot| kinds[slot]);
                if trap_kind != Some(6) {
                    notice(
                        &mut notices,
                        entry,
                        if trap_kind.is_some() { 6949 } else { 6954 },
                        9 + n,
                        false,
                    );
                }
            }
            if kind != 10 {
                if let Some(n) = boolean {
                    if w[9 + n] > 1 {
                        notice(&mut notices, entry, 6985, 9 + n, false);
                    }
                }
            }
            if kind == 11 {
                transport_displays.push(w[2]);
            }
            if kind == 15 {
                if w[9] != 0 && !lookup(owner, TemplateLookup::TaxiPath, w[9], 7141)? {
                    notice(&mut notices, entry, 7142, 9, false);
                }
                if !matches!(w[2], 462 | 562) {
                    transport_displays.push(w[2]);
                }
            }
            if kind == 22 && !lookup(owner, TemplateLookup::Spell, w[9], 6960)? {
                notice(&mut notices, entry, 6963, 9, false);
            }
            if kind == 29 && w[25] == 0 {
                w[25] = 1;
                notice(&mut notices, entry, 7008, 25, true);
            }
        }
        Ok(Self {
            rows,
            index,
            notices,
            transport_displays,
        })
    }
    pub fn get(&self, entry: u32) -> Option<&LoadedGameObjectInfo> {
        self.index.get(&entry).map(|&slot| &self.rows[slot])
    }
    pub fn rows(&self) -> &[LoadedGameObjectInfo] {
        &self.rows
    }
    pub fn notices(&self) -> &[TemplateNotice] {
        &self.notices
    }
    pub fn transport_displays(&self) -> &[u32] {
        &self.transport_displays
    }
}
mod catalog;
pub use catalog::TemplateCatalog;
#[cfg(test)]
mod tests;
