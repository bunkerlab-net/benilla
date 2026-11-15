// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! ObjectMgr.cpp:582-717 and Creature.cpp:2296-2303. No aura application or admission.
use super::*;
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CreatureAddonTable {
    Template,
    Guid,
}
impl CreatureAddonTable {
    pub fn name(self) -> &'static str {
        match self {
            Self::Template => "creature_template_addon",
            Self::Guid => "creature_addon",
        }
    }
    /// SQLStorageImpl.h:309-329, in submission order.
    pub fn queries(self) -> [&'static str; 3] {
        match self {
            Self::Template => [
                "SELECT MAX(entry) FROM creature_template_addon",
                "SELECT COUNT(*) FROM creature_template_addon",
                "SELECT * FROM creature_template_addon",
            ],
            Self::Guid => [
                "SELECT MAX(guid) FROM creature_addon",
                "SELECT COUNT(*) FROM creature_addon",
                "SELECT * FROM creature_addon",
            ],
        }
    }
}
pub type CreatureAddonRow = SqlTemplateRow<7, 1>;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CreatureAddonAuras {
    /// Slot zero bypasses ConvertCreatureAddonAuras, ObjectMgr.cpp:670.
    Raw(Vec<u8>),
    Empty,
    /// Includes the source's zero terminator, ObjectMgr.cpp:662.
    Spells(Vec<u32>),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadedCreatureAddon {
    pub key: u32,
    pub mount: u32,
    pub stand_state: u8,
    pub sheath_state: u8,
    pub emote: u32,
    pub move_flags: u32,
    pub auras: CreatureAddonAuras,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreatureAddonNotice {
    pub table: CreatureAddonTable,
    pub key: u32,
    pub line: u32,
    pub mutated: bool,
    pub spell: Option<u32>,
}
pub trait CreatureAddonLoaderOwner {
    fn rows(&mut self, table: CreatureAddonTable) -> Result<Vec<CreatureAddonRow>> {
        Err(refuse(match table {
            CreatureAddonTable::Template => {
                "execution.creature_addons.ObjectMgr_667.creature_template_addon_owner"
            }
            CreatureAddonTable::Guid => {
                "execution.creature_addons.ObjectMgr_667.creature_addon_owner"
            }
        }))
    }
    fn display(&mut self, _id: u32) -> Result<bool> {
        Err(refuse(
            "execution.creature_addons.ObjectMgr_678.creature_display_info_owner",
        ))
    }
    fn emote(&mut self, _id: u32) -> Result<bool> {
        Err(refuse(
            "execution.creature_addons.ObjectMgr_688.emotes_owner",
        ))
    }
    fn spell(&mut self, _id: u32) -> Result<Option<&SpellTemplate>> {
        Err(refuse(
            "execution.creature_addons.ObjectMgr_622.spell_template_owner",
        ))
    }
    fn cast_time(&mut self, _id: u32) -> Result<Option<i32>> {
        Err(refuse(
            "execution.creature_addons.ObjectMgr_636.spell_cast_times_owner",
        ))
    }
    fn strict_check_filtered(&mut self) -> Result<bool> {
        Err(refuse(
            "execution.creature_addons.ObjectMgr_646.db_strict_log_filter_owner",
        ))
    }
    fn creature_template(&mut self, _id: u32) -> Result<bool> {
        Err(refuse(
            "execution.creature_addons.ObjectMgr_707.loaded_creature_templates_owner",
        ))
    }
    fn creature_guid(&mut self, _id: u32) -> Result<bool> {
        Err(refuse(
            "execution.creature_addons.ObjectMgr_715.loaded_creature_data_owner",
        ))
    }
}
#[derive(Debug)]
pub struct LoadedCreatureAddons {
    rows: [Vec<LoadedCreatureAddon>; 2],
    index: [BTreeMap<u32, usize>; 2],
    pub record_counts: [usize; 2],
    pub notices: Vec<CreatureAddonNotice>,
}
impl LoadedCreatureAddons {
    fn empty() -> Self {
        Self {
            rows: std::array::from_fn(|_| Vec::new()),
            index: std::array::from_fn(|_| BTreeMap::new()),
            record_counts: [0; 2],
            notices: Vec::new(),
        }
    }
    pub fn load(owner: &mut impl CreatureAddonLoaderOwner) -> Result<Self> {
        let mut this = Self::empty();
        for table in [CreatureAddonTable::Template, CreatureAddonTable::Guid] {
            this.load_table(table, owner)?;
            let slot = table as usize;
            for (&key, &index) in this.index[slot].range(1..) {
                let exists = match table {
                    CreatureAddonTable::Template => owner.creature_template(key)?,
                    CreatureAddonTable::Guid => owner.creature_guid(key)?,
                };
                if !exists {
                    this.notices.push(CreatureAddonNotice {
                        table,
                        key: this.rows[slot][index].key,
                        line: if table == CreatureAddonTable::Template {
                            708
                        } else {
                            716
                        },
                        mutated: false,
                        spell: None,
                    });
                }
            }
        }
        Ok(this)
    }
    fn load_table(
        &mut self,
        table: CreatureAddonTable,
        owner: &mut impl CreatureAddonLoaderOwner,
    ) -> Result<()> {
        let rows = owner.rows(table)?;
        // MAX(id)+1 wraps to zero, then createRecord indexes an empty allocation, SQLStorageImpl.h:317,394.
        require(
            !rows.iter().any(|r| r.words[0] == u32::MAX),
            "execution.creature_addons.SQLStorageImpl_317.max_entry_index_undefined",
        )?;
        let slot = table as usize;
        self.record_counts[slot] = rows.len();
        for row in rows {
            let w = row.words;
            self.index[slot].insert(w[0], self.rows[slot].len());
            self.rows[slot].push(LoadedCreatureAddon {
                key: w[0],
                mount: w[1],
                stand_state: w[2] as u8,
                sheath_state: w[3] as u8,
                emote: w[4],
                move_flags: w[5],
                auras: CreatureAddonAuras::Raw(row.strings.into_iter().next().unwrap()),
            });
        }
        for (&key, &index) in self.index[slot].range(1..) {
            let row = &mut self.rows[slot][index];
            let mut notice = |line, mutated, spell| {
                self.notices.push(CreatureAddonNotice {
                    table,
                    key,
                    line,
                    mutated,
                    spell,
                })
            };
            if row.mount != 0 && !owner.display(row.mount)? {
                row.mount = 0;
                notice(680, true, None);
            }
            if row.sheath_state > 2 {
                notice(686, false, None);
            }
            if !owner.emote(row.emote)? {
                let mutated = row.emote != 0;
                row.emote = 0;
                notice(690, mutated, None);
            }
            let CreatureAddonAuras::Raw(raw) = &row.auras else {
                unreachable!()
            };
            let values = aura_values(raw)?;
            if values.is_empty() {
                row.auras = CreatureAddonAuras::Empty;
                continue;
            }
            let mut auras = Vec::with_capacity(values.len() + 1);
            for id in values {
                let Some(spell) = owner.spell(id)? else {
                    notice(625, true, Some(id));
                    continue;
                };
                let effects = [spell.effect1, spell.effect2, spell.effect3];
                let target_a = [
                    spell.effectimplicittargeta1,
                    spell.effectimplicittargeta2,
                    spell.effectimplicittargeta3,
                ];
                let target_b = [
                    spell.effectimplicittargetb1,
                    spell.effectimplicittargetb2,
                    spell.effectimplicittargetb3,
                ];
                let time = spell.castingtimeindex;
                // SpellMgr.h:77-86,111-119,69-74. Preserve every short circuit of :630.
                if !effects.iter().any(|e| matches!(e, 6 | 27 | 35 | 119))
                    && !has_effect(effects, 3)?
                    && !has_effect(effects, 77)?
                    && !has_effect(effects, 64)?
                {
                    notice(632, true, Some(id));
                    continue;
                }
                if owner.cast_time(time)?.is_some_and(|time| time > 0) {
                    notice(640, true, Some(id));
                    continue;
                }
                if !owner.strict_check_filtered()? && !only_self(effects, target_a, target_b) {
                    notice(648, true, Some(id));
                    continue;
                }
                if auras.contains(&id) {
                    notice(654, true, Some(id));
                    continue;
                }
                auras.push(id);
            }
            auras.push(0);
            row.auras = CreatureAddonAuras::Spells(auras);
        }
        Ok(())
    }
    pub fn table_rows(&self, table: CreatureAddonTable) -> &[LoadedCreatureAddon] {
        &self.rows[table as usize]
    }
    pub fn table_get(&self, table: CreatureAddonTable, key: u32) -> Option<&LoadedCreatureAddon> {
        let slot = table as usize;
        self.index[slot].get(&key).map(|&i| &self.rows[slot][i])
    }
    /// Creature.cpp:2298-2303 bypasses GUID storage for pets, then falls back to the template.
    pub fn get(
        &self,
        high: HighGuid,
        database_guid: u32,
        entry: u32,
    ) -> Option<&LoadedCreatureAddon> {
        if high != HighGuid::Pet {
            if let Some(addon) = self.table_get(CreatureAddonTable::Guid, database_guid) {
                return Some(addon);
            }
        }
        self.table_get(CreatureAddonTable::Template, entry)
    }
}
pub fn get_creature_addon(
    owner: Option<&LoadedCreatureAddons>,
    high: HighGuid,
    database_guid: u32,
    entry: u32,
) -> Result<Option<&LoadedCreatureAddon>> {
    Ok(owner
        .ok_or_else(|| refuse("execution.creature.loaded_creature_addon_store_owner"))?
        .get(high, database_guid, entry))
}
fn atoi(text: &[u8]) -> Result<u32> {
    let first = text
        .iter()
        .position(|b| !matches!(b, b' ' | b'\t'..=b'\r'))
        .unwrap_or(text.len());
    let text = &text[first..];
    let negative = text.first() == Some(&b'-');
    let start = usize::from(matches!(text.first(), Some(b'+' | b'-')));
    let limit = if negative {
        1u64 << 31
    } else {
        i32::MAX as u64
    };
    let mut value = 0u64;
    for &digit in &text[start..] {
        if !digit.is_ascii_digit() {
            break;
        }
        value = value * 10 + u64::from(digit - b'0');
        require(
            value <= limit,
            "execution.creature_addons.ObjectMgr_595_600.atoi_int_undefined",
        )?;
    }
    Ok(if negative {
        0u32.wrapping_sub(value as u32)
    } else {
        value as u32
    })
}
fn aura_values(raw: &[u8]) -> Result<Vec<u32>> {
    let raw = raw.split(|b| *b == 0).next().unwrap_or_default();
    let mut values = Vec::new();
    let mut p = 0;
    let mut s = 0;
    // ObjectMgr.cpp:590-600 increments before testing a literal space. Do not use split_whitespace.
    while p < raw.len() {
        p += 1;
        if raw.get(p) == Some(&b' ') {
            values.push(atoi(&raw[s..])?);
            p += 1;
            s = p;
        }
    }
    if p != s {
        values.push(atoi(&raw[s..])?);
    }
    Ok(values)
}
fn has_effect(effects: [u32; 3], wanted: u32) -> Result<bool> {
    for effect in effects {
        // SpellEffectDefines.h:24-156 has a non-fixed enum range of 0..=255.
        require(
            effect <= 255,
            "execution.creature_addons.SpellMgr_72.SpellEffects_enum_undefined",
        )?;
        if effect == wanted {
            return Ok(true);
        }
    }
    Ok(false)
}
fn only_self(effects: [u32; 3], a: [u32; 3], b: [u32; 3]) -> bool {
    // SpellMgr.h:924-948 returns true at the first empty slot, even if later slots target others.
    for i in 0..3 {
        if effects[i] == 0 {
            return true;
        }
        if a[i] != 1 || !matches!(b[i], 0 | 1) {
            return false;
        }
    }
    true
}
#[path = "creature_addons_catalog.rs"]
mod catalog;
pub use catalog::*;
#[cfg(test)]
#[path = "creature_addons_census.rs"]
pub(super) mod census;
#[cfg(test)]
#[path = "creature_addons_recording_tests.rs"]
mod recording_tests;
#[cfg(test)]
#[path = "creature_addons_tests.rs"]
mod tests;

#[path = "creature_addon_application.rs"]
mod application;
pub use application::*;
