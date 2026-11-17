// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::native::production_compiler::{ObjectMgrSpawnRows, VerifiedCatalog};
use std::collections::{BTreeMap, BTreeSet};
#[test]
fn catalog04_creature_addon_branch_census_and_4692_lookup() {
    let (Some(root), Some(occurrences), Some(config)) = (
        std::env::var_os("BENILLA_F2_CATALOG"),
        std::env::var_os("BENILLA_F2_SPAWN_OCCURRENCES"),
        std::env::var_os("BENILLA_F5_EVENT_AI_CONFIG"),
    ) else {
        println!("SKIP creature addon census: bind catalog, spawn occurrences and recorded configuration");
        return;
    };
    let mut catalog = VerifiedCatalog::open(Path::new(&root), false).unwrap();
    let templates = LoadedCreatureTemplates::catalog_diagnostic(&mut catalog).unwrap();
    let spawns =
        SourceEffectiveSpawns::inspect_catalog_pre_terrain(&mut catalog, Path::new(&occurrences))
            .unwrap();
    struct Missing;
    impl SpellLoaderOwner for Missing {}
    let spells = SourceSpellStores::load(&mut catalog, &mut Missing).unwrap();
    let config = std::fs::read_to_string(config).unwrap();
    let settings: Vec<_> = config
        .lines()
        .filter_map(|l| l.split_once('='))
        .filter(|(k, _)| k.trim() == "LogFilter_DbStrictedCheck")
        .collect();
    assert_eq!(settings.len(), 1);
    let value = settings[0].1.trim();
    let filtered =
        value == "1" || value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("yes");
    assert!(filtered, "seal16-full-01/mangosd.conf:429");
    let raw = [
        catalog
            .execution_creature_addon_rows(CreatureAddonTable::Template)
            .unwrap(),
        catalog
            .execution_creature_addon_rows(CreatureAddonTable::Guid)
            .unwrap(),
    ];
    let mut owner = CreatureAddonCatalog::new(&mut catalog);
    owner.templates = Some(&templates);
    owner.spawns = Some(&spawns);
    owner.spells = Some(&spells);
    owner.strict_check_filtered = Some(filtered);
    let mut loaded = LoadedCreatureAddons::empty();
    for table in [CreatureAddonTable::Template, CreatureAddonTable::Guid] {
        let slot = table as usize;
        let mut undefined =
            BTreeMap::<&str, usize>::from([("max_index", 0), ("atoi", 0), ("effect_enum", 0)]);
        for row in &raw[slot] {
            *undefined.get_mut("max_index").unwrap() += usize::from(row.words[0] == u32::MAX);
            if row.words[0] != 0 {
                match aura_values(&row.strings[0]) {
                    Err(_) => *undefined.get_mut("atoi").unwrap() += 1,
                    Ok(values) => {
                        let mut reaches = false;
                        for id in values {
                            if let Some(s) = spells.template(id) {
                                let e = [s.effect1, s.effect2, s.effect3];
                                if !e.iter().any(|e| matches!(e, 6 | 27 | 35 | 119)) {
                                    reaches |= (|| -> Result<bool> {
                                        Ok(has_effect(e, 3)?
                                            || has_effect(e, 77)?
                                            || has_effect(e, 64)?)
                                    })()
                                    .is_err();
                                }
                            }
                        }
                        *undefined.get_mut("effect_enum").unwrap() += usize::from(reaches);
                    }
                }
            }
        }
        assert!(undefined.values().all(|n| *n == 0), "{undefined:?}");
        loaded.load_table(table, &mut owner).unwrap();
        let mut existence = [0usize; 3];
        let mut refused = BTreeMap::new();
        for row in loaded.table_rows(table).iter().filter(|r| r.key != 0) {
            match if table == CreatureAddonTable::Template {
                owner.creature_template(row.key)
            } else {
                owner.creature_guid(row.key)
            } {
                Ok(true) => existence[0] += 1,
                Ok(false) => existence[1] += 1,
                Err(e) => {
                    existence[2] += 1;
                    *refused.entry(e.id).or_insert(0usize) += 1;
                }
            }
        }
        let branches: BTreeMap<_, _> = [680, 686, 690, 625, 632, 640, 648, 654]
            .into_iter()
            .map(|line| {
                let notices: Vec<_> = loaded
                    .notices
                    .iter()
                    .filter(|n| n.table == table && n.line == line)
                    .collect();
                let rows = notices.iter().map(|n| n.key).collect::<BTreeSet<_>>().len();
                let mutated = notices
                    .iter()
                    .filter(|n| n.mutated)
                    .map(|n| n.key)
                    .collect::<BTreeSet<_>>()
                    .len();
                (line, (rows, mutated, notices.len()))
            })
            .collect();
        let empty = loaded
            .table_rows(table)
            .iter()
            .filter(|r| r.auras == CreatureAddonAuras::Empty)
            .count();
        let all_dropped = loaded
            .table_rows(table)
            .iter()
            .filter(|r| r.auras == CreatureAddonAuras::Spells(vec![0]))
            .count();
        let zero_keys = raw[slot].iter().filter(|r| r.words[0] == 0).count();
        let zero_tokens = raw[slot]
            .iter()
            .filter(|r| r.words[0] != 0)
            .flat_map(|r| aura_values(&r.strings[0]).unwrap())
            .filter(|id| *id == 0)
            .count();
        assert_eq!(loaded.record_counts[slot], [7474, 1789][slot]);
        assert_eq!(empty, [6746, 1327][slot]);
        assert!(branches.values().all(|count| *count == (0, 0, 0)));
        assert_eq!((all_dropped, zero_keys, zero_tokens), (0, 0, 0));
        assert_eq!(existence, [[7474, 0, 0], [1672, 0, 117]][slot]);
        println!("CREATURE_ADDON_CENSUS table={} loaded={} branches_rows_mutated_hits={branches:?} empty_auras={empty} all_auras_dropped={all_dropped} row_dropped=0 zero_keys={zero_keys} zero_aura_tokens={zero_tokens} existence_valid_missing_refused={existence:?} refused={refused:?} stock_ub={undefined:?}", table.name(), loaded.record_counts[slot]);
    }
    let mut aura_spells = BTreeMap::<u32, [usize; 2]>::new();
    for table in [CreatureAddonTable::Template, CreatureAddonTable::Guid] {
        let mut rows_with_auras = 0;
        for row in loaded.table_rows(table) {
            if let CreatureAddonAuras::Spells(auras) = &row.auras {
                rows_with_auras += usize::from(auras.first().is_some_and(|spell| *spell != 0));
                for &spell in auras.iter().take_while(|&&spell| spell != 0) {
                    aura_spells.entry(spell).or_default()[table as usize] += 1;
                }
            }
        }
        assert_eq!(rows_with_auras, [728, 462][table as usize]);
    }
    let mut effect_types = BTreeMap::<u32, usize>::new();
    let mut aura_types = BTreeMap::<u32, usize>::new();
    let mut effect_aura_slots = BTreeMap::<(u32, u32), usize>::new();
    let mut static_covered = 0;
    for (&id, references) in &aura_spells {
        let spell = spells.template(id).unwrap();
        let static_coverage = spell.effects().iter().all(|effect| {
            effect.effect == 0 || (effect.effect == 6 && matches!(effect.aura, 4 | 95))
        });
        static_covered += usize::from(static_coverage);
        for effect in spell.effects().iter().filter(|effect| effect.effect != 0) {
            *effect_aura_slots
                .entry((effect.effect, effect.aura))
                .or_default() += 1;
        }
        let effects = spell.effects().map(|effect| effect.effect);
        for &effect in effects.iter().filter(|&&effect| effect != 0) {
            *effect_types.entry(effect).or_default() += 1;
        }
        let auras = spell.effects().map(|effect| effect.aura);
        for &aura in auras.iter().filter(|&&aura| aura != 0) {
            *aura_types.entry(aura).or_default() += 1;
        }
        let reached = if matches!(id, 9036 | 10848) {
            "GUID40520_completion_asserted_in_seal16"
        } else {
            "execution_not_asserted_by_GUID40520_test"
        };
        println!("CREATURE_ADDON_AURA spell={id} references_template_guid={references:?} effects={effects:?} auras={auras:?} static_coverage={static_coverage} reached={reached}");
    }
    assert_eq!(aura_spells.len(), 262);
    assert_eq!(static_covered, 31);
    assert_eq!(aura_types.len(), 43);
    assert_eq!(aura_types.values().sum::<usize>(), 292);
    assert_eq!(effect_aura_slots.values().sum::<usize>(), 299);
    assert_eq!(effect_aura_slots[&(6, 95)], 1);
    assert_eq!(effect_aura_slots[&(6, 4)], 33);
    assert_eq!(
        effect_types,
        BTreeMap::from([(3, 5), (6, 281), (27, 1), (35, 10), (119, 2)])
    );
    println!("CREATURE_ADDON_AURA_CENSUS rows_template_guid=[728,462] distinct={} effect_slots={effect_types:?}", aura_spells.len());
    println!("CREATURE_ADDON_AURA_TYPES {aura_types:?}");
    println!("CREATURE_ADDON_STATIC_COVERAGE label='static coverage, not execution' only_owned_effect_and_aura_types={static_covered} unported_effect_or_aura_type={} effect_aura_slots={effect_aura_slots:?} specific_callbacks_not_qualified=true", aura_spells.len() - static_covered);
    drop(owner);
    let mut rows = ObjectMgrSpawnRows::open(&mut catalog, Path::new(&occurrences)).unwrap();
    let mut found = 0;
    while let Some(row) = rows.next("creature").unwrap() {
        if row.entry == 4692 && row.map == 1 {
            found += 1;
            let guid = loaded.table_get(CreatureAddonTable::Guid, row.guid);
            let template = loaded.table_get(CreatureAddonTable::Template, 4692);
            println!("CREATURE_ADDON_4692 guid={} guid_addon={guid:?} template_addon={template:?} selected={:?}", row.guid, loaded.get(HighGuid::Creature, row.guid, 4692));
        }
    }
    assert!(found > 0);
    assert!(loaded.table_get(CreatureAddonTable::Guid, 28044).is_none());
    let addon = loaded.get(HighGuid::Creature, 28044, 4692).unwrap();
    assert_eq!(
        (
            addon.key,
            addon.mount,
            addon.stand_state,
            addon.sheath_state,
            addon.emote,
            addon.move_flags
        ),
        (4692, 0, 0, 1, 0, 0)
    );
    assert_eq!(addon.auras, CreatureAddonAuras::Empty);
}

pub(crate) fn assert_loaded_guid_census(
    catalog: &mut VerifiedCatalog,
    spawns: &SourceEffectiveSpawns,
) {
    let rows = catalog
        .execution_creature_addon_rows(CreatureAddonTable::Guid)
        .unwrap();
    let mut counts = [0usize; 3];
    for row in rows.iter().filter(|r| r.words[0] != 0) {
        let key = SpawnKey {
            kind: GridSpawnKind::Creature,
            guid: row.words[0],
        };
        match spawns.pre_terrain_data(key) {
            Ok(Some(_)) => counts[0] += 1,
            Ok(None) => counts[1] += 1,
            Err(_) => counts[2] += 1,
        }
    }
    let out_error_db = counts[1];
    assert_eq!(rows.len(), 1789);
    assert_eq!(counts, [1789, 0, 0]);
    assert_eq!(out_error_db, 0);
    println!("CREATURE_ADDON_LOADED_GUIDS present_missing_unresolved={counts:?} outErrorDb={out_error_db}");
}
