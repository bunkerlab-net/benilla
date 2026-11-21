// SPDX-License-Identifier: GPL-3.0-or-later
//! Static stock operands, not an asserted execution of future combat/list transitions.
use super::*;
fn default_key(template: &LoadedCreatureInfo) -> u32 {
    if template.words[89] != 0 {
        template.words[89]
    } else {
        template.words[0].wrapping_mul(100)
    }
}
fn unwritten_auto(list: &CreatureSpellList) -> usize {
    list.spells
        .values()
        .filter(|s| !s.disabled_for_ai && !s.has_written_combat_condition())
        .count()
}
pub(super) fn census(
    catalog: &mut VerifiedCatalog,
    templates: &LoadedCreatureTemplates,
    spawns: &SourceEffectiveSpawns,
    lists: &LoadedCreatureSpellLists,
) {
    // UnitAI.cpp:58,104-110,1155-1189. Disabled does NOT gate UpdateSpellLists.
    let cold_reach = lists
        .lists
        .values()
        .filter(|l| l.ai_has_spells())
        .map(unwritten_auto)
        .sum::<usize>();
    let unwritten_auto_spells = lists.lists.values().map(unwritten_auto).sum::<usize>();
    let fallback_pairs = templates
        .rows()
        .iter()
        .filter_map(|t| {
            if t.words[89] == 0 {
                return None;
            }
            let modern = lists.get(t.words[89])?;
            if !modern.ai_has_spells() {
                return None;
            }
            let fallback_key = t.words[0].wrapping_mul(100);
            let fallback = lists.get(fallback_key)?;
            fallback.disabled.then_some((
                t.words[0],
                t.words[89],
                fallback_key,
                unwritten_auto(fallback),
            ))
        })
        .collect::<Vec<_>>();
    let vulnerable_fallback_pairs = fallback_pairs.iter().filter(|p| p.3 != 0).count();
    let charm_legacy = templates
        .rows()
        .iter()
        .filter_map(|t| {
            lists
                .get(t.words[90])
                .filter(|l| l.disabled)
                .map(|l| (t.words[0], t.words[90], unwritten_auto(l)))
        })
        .collect::<Vec<_>>();
    let mut setters = Vec::new();
    let mut modern_owners = templates
        .rows()
        .iter()
        .filter(|t| {
            lists
                .get(default_key(t))
                .is_some_and(CreatureSpellList::ai_has_spells)
        })
        .map(|t| t.words[0])
        .collect::<BTreeSet<_>>();
    let mut spell_set_actions = 0;
    let mut entry_change_actions = 0;
    for row in catalog_event_ai_rows(catalog).unwrap() {
        for offset in [12, 16, 20] {
            let kind = row.words[offset];
            if !matches!(kind, 36 | 60) {
                continue;
            }
            let actor = row.words[1] as i32;
            let actor_entry = if actor < 0 {
                spawns
                    .complete_data(SpawnKey {
                        kind: GridSpawnKind::Creature,
                        guid: actor.unsigned_abs(),
                    })
                    .unwrap()
                    .expect("stock EventAI GUID binding")
                    .row
                    .entry
            } else {
                actor as u32
            };
            let target = row.words[offset + 1];
            let key = if kind == 36 {
                entry_change_actions += 1;
                default_key(templates.get(target).expect("stock UpdateEntry target"))
            } else {
                spell_set_actions += 1;
                target
            };
            if let Some(list) = lists.get(key) {
                if list.ai_has_spells() {
                    modern_owners.insert(actor_entry);
                }
                setters.push((row.words[0], actor_entry, kind, target, key));
            }
        }
    }
    let eai_legacy_targets = setters
        .iter()
        .filter(|s| lists.get(s.4).unwrap().disabled)
        .copied()
        .collect::<Vec<_>>();
    let eai_modern_to_legacy = eai_legacy_targets
        .iter()
        .filter(|s| modern_owners.contains(&s.1) && unwritten_auto(lists.get(s.4).unwrap()) != 0)
        .copied()
        .collect::<Vec<_>>();
    assert_eq!(cold_reach, 0);
    assert_eq!(unwritten_auto_spells, 4277);
    assert_eq!((fallback_pairs.len(), vulnerable_fallback_pairs), (66, 65));
    assert_eq!(
        fallback_pairs.iter().map(|p| p.0).collect::<Vec<_>>(),
        [
            589, 595, 619, 785, 1181, 1183, 1538, 1539, 2180, 2181, 3272, 4286, 4287, 4288, 4289,
            4290, 4291, 4294, 4296, 4297, 4298, 4299, 4300, 4302, 4303, 4540, 4543, 4803, 4809,
            4820, 4852, 6222, 6223, 6227, 7439, 8717, 9449, 11347, 11382, 11793, 11794, 14507,
            14509, 14510, 14682, 14834, 14987, 15623, 15928, 15929, 15930, 15936, 15952, 16028,
            16050, 16051, 16052, 16054, 16058, 16062, 16063, 16064, 16150, 16299, 16380, 16438,
        ]
    );
    assert!(charm_legacy.is_empty());
    assert_eq!((spell_set_actions, entry_change_actions), (0, 21));
    assert!(eai_modern_to_legacy.is_empty());
    println!("F6_SPELL_LIST_REACH cold_start_UnitAI_1213_reach={cold_reach} unwritten_autocastable_spells={unwritten_auto_spells} modern_template_legacy_fallback_pairs={} vulnerable_pairs={vulnerable_fallback_pairs} fallback_targets_by_entry={fallback_pairs:?} charm_legacy_targets={charm_legacy:?} eai_spell_set_actions={spell_set_actions} eai_entry_change_actions={entry_change_actions} eai_legacy_targets={eai_legacy_targets:?} eai_modern_to_legacy_candidates={eai_modern_to_legacy:?} disabled_is_not_an_UpdateSpellLists_guard=true", fallback_pairs.len());
}
