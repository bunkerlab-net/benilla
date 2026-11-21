// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::player_skills::{CreationSkillRow, PlayerSkillInputs};
use super::*;
use crate::native::production_compiler::{CatalogRowView, ProductionDiagnostic, VerifiedCatalog};
use std::{collections::BTreeMap, str::FromStr};

type Pair = (u8, u8);

#[derive(Clone, Copy, Debug)]
pub struct PlayerCreateStart {
    pub map: u32,
    pub zone: u32,
    pub pose: SourcePose,
}

#[derive(Clone, Copy, Debug)]
pub struct PlayerCreateItem {
    pub item: u32,
    pub amount: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct PlayerCreateAction {
    pub button: u8,
    pub action: u32,
    pub kind: u8,
}

impl PlayerCreateAction {
    // Player.cpp:5920-5936; Player.h:159,183.
    pub(super) fn in_bounds(self) -> bool {
        self.button < 120 && self.action < 0x0100_0000
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PlayerCreateRace {
    pub faction: u32,
    pub display: [u32; 2],
    pub team: u32,
    pub taxi_mask: u32,
}

/// The retained cursor order supplies grants and buttons. These rows do not run startup.
#[derive(Debug, Default)]
pub struct PlayerCreateRows {
    pub(super) skill_inputs: Option<PlayerSkillInputs>,
    skills: BTreeMap<Pair, Vec<PlayerCreateSkill>>,
    pub(super) races: BTreeMap<u8, PlayerCreateRace>,
    pub(super) classes: BTreeMap<u8, u8>,
    pub(super) starts: BTreeMap<Pair, PlayerCreateStart>,
    items: BTreeMap<Pair, Vec<PlayerCreateItem>>,
    spells: BTreeMap<Pair, Vec<u32>>,
    actions: BTreeMap<Pair, Vec<PlayerCreateAction>>,
    pub(super) class_levels: BTreeMap<(u8, u8), [u16; 2]>,
    pub(super) levels: BTreeMap<(u8, u8, u8), [u8; 5]>,
    pub(super) xp: BTreeMap<u8, u32>,
    pub(super) display_scales: BTreeMap<u32, [f32; 2]>,
}

pub(super) fn playable_race(race: u8) -> bool {
    (1..=8).contains(&race)
}

pub(super) fn playable_class(class: u8) -> bool {
    matches!(class, 1..=5 | 7..=9 | 11)
}

impl PlayerCreateRows {
    /// ObjectMgr.h:522-529. Both displays must be present for GetPlayerInfo.
    pub fn start(&self, race: u8, class: u8) -> Option<&PlayerCreateStart> {
        let race_row = self.races.get(&race)?;
        if race_row.display.contains(&0) {
            return None;
        }
        self.starts.get(&(race, class))
    }

    pub fn items(&self, race: u8, class: u8) -> &[PlayerCreateItem] {
        self.items.get(&(race, class)).map_or(&[], Vec::as_slice)
    }

    pub fn spells(&self, race: u8, class: u8) -> &[u32] {
        self.spells.get(&(race, class)).map_or(&[], Vec::as_slice)
    }

    pub fn skills(&self, race: u8, class: u8) -> &[PlayerCreateSkill] {
        self.skills.get(&(race, class)).map_or(&[], Vec::as_slice)
    }

    pub fn actions(&self, race: u8, class: u8) -> &[PlayerCreateAction] {
        self.actions.get(&(race, class)).map_or(&[], Vec::as_slice)
    }

    /// ObjectMgr.cpp:3071-3710, the creation and level-one operands, excluding skill grants.
    pub fn load(catalog: &mut VerifiedCatalog) -> Result<Self> {
        let mut loaded = Self::default();
        {
            let mut races = catalog.dbc_view("ChrRaces.dbc")?;
            for race in 1..=8 {
                if let Some(row) = races.lookup(race)? {
                    loaded.races.insert(
                        race as u8,
                        PlayerCreateRace {
                            faction: dbc_word(&row, 2)?,
                            display: [dbc_word(&row, 4)?, dbc_word(&row, 5)?],
                            team: dbc_word(&row, 8)?,
                            taxi_mask: dbc_word(&row, 14)?,
                        },
                    );
                }
            }
        }
        {
            let mut classes = catalog.dbc_view("ChrClasses.dbc")?;
            for class in (1..=11).filter(|&class| playable_class(class)) {
                if let Some(row) = classes.lookup(u32::from(class))? {
                    loaded.classes.insert(class, dbc_word(&row, 3)? as u8);
                }
            }
        }
        let maps: BTreeMap<_, _> = catalog.execution_map_entries()?.into_iter().collect();
        scan(catalog, "playercreateinfo", |cells| {
            let key = loaded.pair(cells, 3109, 3116)?;
            let map = number(cells)?;
            let zone = number(cells)?;
            let pose = SourcePose([
                coordinate(cells)?,
                coordinate(cells)?,
                coordinate(cells)?,
                coordinate(cells)?,
            ]);
            // MapManager::IsValidMapCoord uses the same four-coordinate check as corpses.
            let &map_type = maps.get(&map).ok_or_else(|| {
                refuse("execution.player.ObjectMgr_3123.playercreateinfo_position")
            })?;
            require(
                valid_corpse_position(pose.0),
                "execution.player.ObjectMgr_3123.playercreateinfo_position",
            )?;
            require(
                !PersistentMapEntry { id: map, map_type }.instanceable(),
                "execution.player.ObjectMgr_3129.playercreateinfo_instance",
            )?;
            loaded
                .starts
                .insert(key, PlayerCreateStart { map, zone, pose });
            Ok(())
        })?;
        require(
            !loaded.starts.is_empty(),
            "execution.player.ObjectMgr_3086.playercreateinfo_empty",
        )?;

        // Validate references after each cursor closes. The retained reader cannot re-enter a scan.
        let mut items = Vec::new();
        scan(catalog, "playercreateinfo_item", |cells| {
            let key = loaded.pair(cells, 3184, 3191)?;
            items.push((
                key,
                PlayerCreateItem {
                    item: number(cells)?,
                    amount: number(cells)?,
                },
            ));
            Ok(())
        })?;
        for (key, item) in items {
            if template_exists(catalog, "item_template", item.item)? && item.amount != 0 {
                loaded.items.entry(key).or_default().push(item);
            }
        }
        let skill_inputs = PlayerSkillInputs::load(catalog)?;
        let mut skill_rows = Vec::new();
        scan(catalog, "playercreateinfo_skills", |cells| {
            skill_rows.push(CreationSkillRow {
                race: number(cells)?,
                class: number(cells)?,
                skill: PlayerCreateSkill {
                    id: number(cells)?,
                    step: number(cells)?,
                },
            });
            Ok(())
        })?;
        loaded.skills = skill_inputs.creation_rows(skill_rows);
        loaded.skill_inputs = Some(skill_inputs);

        let mut spells = Vec::new();
        scan(catalog, "playercreateinfo_spell", |cells| {
            spells.push((loaded.pair(cells, 3362, 3369)?, number(cells)?));
            Ok(())
        })?;
        for (key, spell) in spells {
            if template_exists(catalog, "spell_template", spell)? {
                loaded.spells.entry(key).or_default().push(spell);
            }
        }
        require(
            !loaded.spells.is_empty(),
            "execution.player.ObjectMgr_3346.playercreateinfo_spell_empty",
        )?;

        let mut actions = Vec::new();
        scan(catalog, "playercreateinfo_action", |cells| {
            let key = loaded.pair(cells, 3422, 3429)?;
            actions.push((
                key,
                PlayerCreateAction {
                    button: number(cells)?,
                    action: number(cells)?,
                    kind: number(cells)?,
                },
            ));
            Ok(())
        })?;
        for (key, action) in actions {
            if valid_action(catalog, action)? {
                loaded.actions.entry(key).or_default().push(action);
            }
        }
        require(
            !loaded.actions.is_empty(),
            "execution.player.ObjectMgr_3406.playercreateinfo_action_empty",
        )?;
        loaded.order_grants();

        scan(catalog, "player_classlevelstats", |cells| {
            let class = number(cells)?;
            let level = number(cells)?;
            require(
                class < 12,
                "execution.player.ObjectMgr_3480.class_level_class",
            )?;
            require(
                level != 0,
                "execution.player.ObjectMgr_3487.class_level_zero",
            )?;
            loaded
                .class_levels
                .insert((class, level), [number(cells)?, number(cells)?]);
            Ok(())
        })?;
        for &class in loaded.classes.keys() {
            require(
                loaded
                    .class_levels
                    .get(&(class, 1))
                    .is_some_and(|values| values[0] != 0),
                "execution.player.ObjectMgr_3533.class_level_one_missing",
            )?;
        }
        scan(catalog, "player_levelstats", |cells| {
            let (race, class) = loaded.pair(cells, 3579, 3586)?;
            let level = number(cells)?;
            require(
                level != 0,
                "execution.player.ObjectMgr_3608.level_zero_index_undefined",
            )?;
            loaded.levels.insert(
                (race, class, level),
                [
                    number(cells)?,
                    number(cells)?,
                    number(cells)?,
                    number(cells)?,
                    number(cells)?,
                ],
            );
            Ok(())
        })?;
        for &(race, class) in loaded.starts.keys() {
            if loaded.start(race, class).is_some() {
                require(
                    loaded
                        .levels
                        .get(&(race, class, 1))
                        .is_some_and(|stats| stats[0] != 0),
                    "execution.player.ObjectMgr_3644.player_level_one_missing",
                )?;
            }
        }
        scan(catalog, "player_xp_for_level", |cells| {
            loaded.xp.insert(number(cells)?, number(cells)?);
            Ok(())
        })?;
        require(
            loaded.xp.contains_key(&1),
            "execution.player.ObjectMgr_3678.level_one_xp_missing",
        )?;
        let mut pending = [None; 16];
        {
            let mut displays = catalog.dbc_view("CreatureDisplayInfo.dbc")?;
            let ids = loaded
                .races
                .values()
                .flat_map(|race| race.display)
                .filter(|&id| id != 0);
            for (id, slot) in ids.zip(&mut pending) {
                let display = displays
                    .lookup(id)?
                    .ok_or_else(|| refuse("execution.player.Player_17506.display_info"))?;
                *slot = Some((
                    id,
                    dbc_word(&display, 1)?,
                    f32::from_bits(dbc_word(&display, 4)?),
                ));
            }
        }
        let mut models = catalog.dbc_view("CreatureModelData.dbc")?;
        for (id, model, scale) in pending.into_iter().flatten() {
            let model = models
                .lookup(model)?
                .ok_or_else(|| refuse("execution.player.Player_17508.model_data"))?;
            loaded
                .display_scales
                .insert(id, [scale, f32::from_bits(dbc_word(&model, 4)?)]);
        }
        Ok(loaded)
    }

    // ObjectMgr.cpp:3343-3442. Northshire-01 A proxy-requests.jsonl:536-539
    // binds the MyISAM primary-key scans by spell ID and button number.
    fn order_grants(&mut self) {
        for spells in self.spells.values_mut() {
            spells.sort_unstable();
        }
        for actions in self.actions.values_mut() {
            actions.sort_unstable_by_key(|row| row.button);
        }
    }

    fn pair(&self, cells: &mut Cells<'_>, race_line: u32, class_line: u32) -> Result<Pair> {
        let race = number(cells)?;
        let class = number(cells)?;
        if !playable_race(race) || !self.races.contains_key(&race) {
            return Err(refuse(&format!(
                "execution.player.ObjectMgr_{race_line}.race"
            )));
        }
        if !playable_class(class) || !self.classes.contains_key(&class) {
            return Err(refuse(&format!(
                "execution.player.ObjectMgr_{class_line}.class"
            )));
        }
        Ok((race, class))
    }
}

// Player.cpp:5918-5980. The loader passes nullptr, so it cannot test known spells.
fn valid_action(catalog: &mut VerifiedCatalog, row: PlayerCreateAction) -> Result<bool> {
    if !row.in_bounds() {
        return Ok(false);
    }
    match row.kind {
        0 => template_exists(catalog, "spell_template", row.action),
        128 => template_exists(catalog, "item_template", row.action),
        _ => Ok(true),
    }
}

fn template_exists(catalog: &mut VerifiedCatalog, table: &str, id: u32) -> Result<bool> {
    let mut key = [0; 14];
    let mut tail = &mut key[4..];
    use std::io::Write;
    write!(tail, "{id}").map_err(|_| refuse("execution.player.catalog_key"))?;
    let len = 10 - tail.len();
    key[..4].copy_from_slice(&(len as u32).to_le_bytes());
    let mut found = false;
    catalog.lookup(table, &key[..4 + len], |_| {
        found = true;
        Ok(())
    })?;
    Ok(found)
}

pub(super) fn dbc_word(row: &serde_json::Value, column: u32) -> Result<u32> {
    row["values"]
        .as_array()
        .and_then(|values| values.iter().find(|value| value[0] == column))
        .and_then(|value| value[2].as_u64())
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| refuse("execution.player.catalog_dbc_layout"))
}

type Cells<'a> =
    dyn Iterator<Item = std::result::Result<Option<&'a [u8]>, ProductionDiagnostic>> + 'a;

fn number<T: FromStr>(cells: &mut Cells<'_>) -> Result<T> {
    let bytes = cells
        .next()
        .transpose()?
        .flatten()
        .ok_or_else(|| refuse("execution.player.catalog_column"))?;
    std::str::from_utf8(bytes)
        .ok()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| refuse("execution.player.catalog_number"))
}

// Field.h:53 uses atof before narrowing to float.
fn coordinate(cells: &mut Cells<'_>) -> Result<u32> {
    Ok((number::<f64>(cells)? as f32).to_bits())
}

fn scan(
    catalog: &mut VerifiedCatalog,
    table: &str,
    mut visit: impl FnMut(&mut Cells<'_>) -> Result<()>,
) -> Result<()> {
    // VerifiedCatalog::open checks the full ordered schema before these projections.
    catalog.scan(table, 0, 256, |row: CatalogRowView<'_>| {
        visit(&mut row.cells()).map_err(|error| ProductionDiagnostic { id: error.id })
    })?;
    Ok(())
}

#[cfg(test)]
impl PlayerCreateRows {
    pub(super) fn test_grants(&mut self, spells: Vec<u32>, actions: Vec<PlayerCreateAction>) {
        self.spells.insert((1, 8), spells);
        self.actions.insert((1, 8), actions);
        self.order_grants();
    }
}
