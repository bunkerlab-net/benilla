// SPDX-License-Identifier: GPL-3.0-or-later
//! World.cpp:1012,1015,1021 orders model, equipment and creature template storage.
use super::*;
#[derive(Default)]
pub(super) struct Evidence {
    queries: Vec<(u64, usize, u64)>,
    pointer: String,
    selected: Option<usize>,
    fields: BTreeMap<usize, Vec<Vec<Vec<u8>>>>,
    last_word: u64,
    before: Option<u64>,
    after: Option<u64>,
}
impl Evidence {
    pub(super) fn observe(&mut self, line: u64, kind: &str, p: &[&str], raw: u64) {
        if kind == "sql_query" {
            let query = unhex(p.last().unwrap());
            if query == b"SELECT * FROM creature_model_info" {
                self.before = Some(line);
            }
            if query == b"SELECT MAX(entry) FROM creature_template" {
                self.after = Some(line);
            }
            self.selected = EQUIPMENT_QUERIES.iter().position(|q| q.as_bytes() == query);
            if let Some(slot) = self.selected {
                self.queries.push((line, slot, raw));
                self.pointer = p[0].into();
                assert!(self.fields.insert(slot, Vec::new()).is_none());
            }
        }
        if kind == "sql_field" && p[0] == self.pointer {
            if let Some(slot) = self.selected {
                let column = p[1].parse::<usize>().unwrap();
                let rows = self.fields.get_mut(&slot).unwrap();
                if column == 0 {
                    rows.push(Vec::new());
                }
                let row = rows.last_mut().unwrap();
                assert_eq!(row.len(), column);
                row.push(templates::sql_storage_field(p));
                self.last_word = raw;
            }
        }
    }
    pub(super) fn load(self, catalog: &mut VerifiedCatalog) -> LoadedEquipmentTemplates {
        assert_eq!(
            self.queries.iter().map(|q| q.1).collect::<Vec<_>>(),
            [0, 1, 2]
        );
        assert!(
            self.before.unwrap() < self.queries[0].0 && self.queries[2].0 < self.after.unwrap()
        );
        assert!(self.queries.iter().all(|q| q.2 == self.last_word));
        let rows = catalog.execution_equipment_rows().unwrap();
        assert_eq!(
            self.fields[&0],
            vec![vec![rows.last().unwrap().words[0].to_string().into_bytes()]]
        );
        assert_eq!(
            self.fields[&1],
            vec![vec![rows.len().to_string().into_bytes()]]
        );
        assert_eq!(self.fields[&2].len(), rows.len());
        for (row, observed) in rows.iter().zip(&self.fields[&2]) {
            assert_eq!(observed.len(), 4);
            assert_eq!(
                row.words
                    .map(|word| word.to_string().into_bytes())
                    .as_slice(),
                observed
            );
        }
        let max_ub = rows.iter().filter(|r| r.words[0] == u32::MAX).count();
        assert_eq!(max_ub, 0);
        let items = catalog.execution_equipment_items().unwrap();
        let item_max_ub = usize::from(items.contains_key(&u32::MAX));
        assert_eq!(item_max_ub, 0);
        let loaded = LoadedEquipmentTemplates::load(rows, items).unwrap();
        let mut notices = BTreeMap::<u32, usize>::new();
        let mut changed = BTreeSet::new();
        let mut missing_item_rows = BTreeSet::new();
        let mut invalid_inventory_rows = BTreeSet::new();
        for &(entry, _, line) in &loaded.notices {
            *notices.entry(line).or_default() += 1;
            changed.insert(entry);
            if line == 1580 {
                missing_item_rows.insert(entry);
            }
            if line == 1596 {
                invalid_inventory_rows.insert(entry);
            }
        }
        println!("F6_EQUIPMENT_SQL matched_sql=3 rows={} skipped_rows=0 fixed_rows={} missing_item_rows={} invalid_inventory_rows={} loader_words=0 query_lines={:?} notices={notices:?} item_fixups={:?} max_index_ub={max_ub} item_max_index_ub={item_max_ub}", loaded.len(), changed.len(), missing_item_rows.len(), invalid_inventory_rows.len(), self.queries.iter().map(|q| q.0).collect::<Vec<_>>(), loaded.item_fixups);
        loaded
    }
}
pub(super) fn census(
    templates: &LoadedCreatureTemplates,
    spawns: &SourceEffectiveSpawns,
    spawn_templates: &LoadedCreatureSpawnTemplates,
    inputs: &EffectiveSpawnInputs,
    groups: &SourceSpawnGroups,
) {
    let mut template_branches = BTreeMap::<CreatureEquipmentBranch, usize>::new();
    for row in templates.rows() {
        let branch = CreatureEquipmentBranch::select(None, None, row.words[93], row.words[93]).0;
        *template_branches.entry(branch).or_default() += 1;
    }
    let mut spawn_branches = BTreeMap::<CreatureEquipmentBranch, usize>::new();
    let mut before_equipment = 0;
    let mut unresolved_guids = BTreeSet::new();
    let mut candidates = BTreeSet::new();
    for (key, data) in spawns.pre_terrain_map() {
        if key.kind != GridSpawnKind::Creature {
            continue;
        }
        if data.row.entry == 0 {
            // Creature.cpp:1637-1650 chooses the live entry before InitEntry equipment.
            before_equipment += 1;
            unresolved_guids.insert(key.guid);
            if let Some(entries) = inputs.conditional.get(&key.guid) {
                candidates.extend(entries.iter().copied().filter(|entry| *entry != 0));
            }
            if let Some((_, entries)) = inputs.random.get(key) {
                candidates.extend(entries.iter().copied().filter(|entry| *entry != 0));
            }
            continue;
        }
        let template = templates.get(data.row.entry).unwrap();
        let spawn = &spawn_templates.rows()[&data.spawn_template.unwrap()];
        let branch = CreatureEquipmentBranch::select(
            None,
            Some(spawn.equipment_id),
            template.words[93],
            template.words[93],
        )
        .0;
        *spawn_branches.entry(branch).or_default() += 1;
    }
    for group in groups.definitions().unwrap().values() {
        if group.kind() == GridSpawnKind::Creature
            && group
                .members
                .iter()
                .any(|member| unresolved_guids.contains(&member.guid))
        {
            candidates.extend(
                group
                    .entries
                    .iter()
                    .map(|entry| entry.entry)
                    .filter(|entry| *entry != 0),
            );
            for squad in &group.squads {
                candidates.extend(
                    squad
                        .entries
                        .iter()
                        .filter(|(guid, entry)| unresolved_guids.contains(guid) && **entry != 0)
                        .map(|(_, entry)| *entry),
                );
            }
        }
    }
    assert_eq!(
        spawn_branches.values().sum::<usize>() + before_equipment,
        spawns
            .pre_terrain_map()
            .keys()
            .filter(|key| key.kind == GridSpawnKind::Creature)
            .count(),
    );
    println!("F6_EQUIPMENT_CENSUS event_override=none difficulty=normal template_branches={template_branches:?} fixed_entry_spawn_branches={spawn_branches:?} entry_chosen_at_load_equipment_path_unknown={before_equipment} distinct_catalog_candidate_templates={} fresh_constructor_can_wield_loot=0", candidates.len());
}
pub(super) fn next_zone_owner(
    catalog: &mut VerifiedCatalog,
    geometry: &SourceTerrainManager,
    map: u32,
    creature_pose: [f32; 3],
    go_pose: [f32; 3],
) {
    let hit = geometry
        .vmap
        .area_info(map, creature_pose)
        .unwrap()
        .unwrap();
    let go_hit = geometry.vmap.area_info(map, go_pose).unwrap();
    let rows = catalog.execution_wmo_area_dbc("WMOAreaTable.dbc").unwrap();
    let matching = rows
        .iter()
        .filter(|row| {
            (row[1] as i32, row[2] as i32, row[3] as i32) == (hit.root, hit.adt, hit.group)
        })
        .collect::<Vec<_>>();
    println!("F6_NEXT_WMO_OWNER diagnostic_only=true map={map} dbc_rows={} fields=[ID,rootId,adtId,groupId,flags,areaId] columns=[0,1,2,3,9,10] creature_pose={creature_pose:?} creature_hit={hit:?} first_go_pose={go_pose:?} first_go_hit={go_hit:?} matching_rows={matching:?} recorded_prefix_first_unbound_lookup=1", rows.len());
}
