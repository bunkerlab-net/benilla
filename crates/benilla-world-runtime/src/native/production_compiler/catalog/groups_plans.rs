// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::collections::BTreeSet;
fn numbers(
    catalog: &mut VerifiedCatalog,
    schema: &Value,
    table: &str,
    names: &[&str],
    mut visit: impl FnMut(&[u32]),
) -> Result<()> {
    let cols = schema["tables"][table]["columns"]
        .as_array()
        .ok_or_else(|| fail("catalog.schema"))?;
    let indexes = names
        .iter()
        .map(|n| position(cols, n))
        .collect::<Result<Vec<_>>>()?;
    catalog.scan(table, 0, 256, |row| {
        let mut cells = [None; 256];
        for (i, cell) in row.cells().enumerate() {
            cells[i] = cell?;
        }
        let mut values = [0; 5];
        for (i, &index) in indexes.iter().enumerate() {
            values[i] = super::spawns::uint(cells[index])?;
        }
        visit(&values[..indexes.len()]);
        Ok(())
    })
}
fn add(counts: &mut BTreeMap<&str, u64>, key: &'static str, yes: bool) {
    *counts.get_mut(key).expect("fixed plan counter") += u64::from(yes);
}
pub(super) fn plans(catalog: &mut VerifiedCatalog, schema: &Value) -> Result<Value> {
    let mut charge: u64 = [
        "creature",
        "gameobject",
        "creature_template",
        "gameobject_template",
    ]
    .iter()
    .map(|t| catalog.relations[*t].rows * 160)
    .sum();
    for id in &LOADERS[..8] {
        let table = catalog_schema::POLICY["loaders"][id]["relations"][0]
            .as_str()
            .ok_or_else(|| fail("groups.query"))?;
        charge += catalog.relations[table].rows * 160;
    }
    charge += [
        "creature_movement",
        "creature_movement_template",
        "waypoint_path",
    ]
    .iter()
    .map(|t| catalog.relations[*t].rows * 160)
    .max()
    .unwrap_or(0);
    check(charge <= DECODED as u64, "groups.view_limit")?;
    analyze(|table, names, visit| numbers(catalog, schema, table, names, visit))
}
fn analyze(
    mut each: impl FnMut(&str, &[&str], &mut dyn FnMut(&[u32])) -> Result<()>,
) -> Result<Value> {
    let mut spawns = [BTreeMap::<u32, (u32, u32)>::new(), BTreeMap::new()];
    let mut templates = [BTreeSet::new(), BTreeSet::new()];
    for (i, kind) in ["creature", "gameobject"].iter().enumerate() {
        each(kind, &["guid", "id", "map"], &mut |v| {
            spawns[i].insert(v[0], (v[1], v[2]));
        })?;
        each(&format!("{kind}_template"), &["entry"], &mut |v| {
            templates[i].insert(v[0]);
        })?;
    }
    let mut groups = BTreeMap::new();
    let mut formations = BTreeSet::new();
    let mut entries = BTreeSet::new();
    each("spawn_group", &["id", "type"], &mut |v| {
        groups.insert(v[0], v[1]);
    })?;
    each("spawn_group_formation", &["id"], &mut |v| {
        formations.insert(v[0]);
    })?;
    each("spawn_group_entry", &["id"], &mut |v| {
        entries.insert(v[0]);
    })?;
    let mut counts: BTreeMap<_, u64> = [
        "groups_without_formation",
        "invalid_group_type",
        "missing_group",
        "missing_spawn",
        "duplicate_membership",
        "cross_map_groups",
        "missing_entry",
        "linked_missing_group",
        "squad_overrides_entries",
        "link_missing_master",
        "link_missing_slave",
        "link_cross_map",
        "link_invalid_flag",
        "link_self_despawn",
        "link_nonunique_master",
        "waypoint_zero_paths",
        "waypoint_gap_edges",
        "waypoint_missing_owner",
    ]
    .into_iter()
    .map(|s| (s, 0))
    .collect();
    counts.insert(
        "groups_without_formation",
        groups.keys().filter(|k| !formations.contains(k)).count() as u64,
    );
    counts.insert(
        "invalid_group_type",
        groups.values().filter(|&&t| t > 1).count() as u64,
    );
    let mut seen = BTreeSet::new();
    let mut maps = BTreeMap::<u32, BTreeSet<u32>>::new();
    // Separate GUID sets follow ObjectMgr.cpp:1253-1323, not untyped integer identity.
    each("spawn_group_spawn", &["id", "guid"], &mut |v| {
        let Some(&kind) = groups.get(&v[0]) else {
            add(&mut counts, "missing_group", true);
            return;
        };
        if kind > 1 {
            return;
        }
        add(
            &mut counts,
            "duplicate_membership",
            !seen.insert((kind, v[1])),
        );
        match spawns[kind as usize].get(&v[1]) {
            Some((_, map)) => {
                maps.entry(v[0]).or_default().insert(*map);
            }
            None => add(&mut counts, "missing_spawn", true),
        }
    })?;
    counts.insert(
        "cross_map_groups",
        maps.values().filter(|m| m.len() > 1).count() as u64,
    );
    each(
        "spawn_group_entry",
        &["id", "entry"],
        &mut |v| match groups.get(&v[0]) {
            None => add(&mut counts, "missing_group", true),
            Some(&kind) if kind < 2 => add(
                &mut counts,
                "missing_entry",
                !templates[kind as usize].contains(&v[1]),
            ),
            _ => (),
        },
    )?;
    each("spawn_group_linked_group", &["id", "linkedid"], &mut |v| {
        add(
            &mut counts,
            "linked_missing_group",
            !groups.contains_key(&v[0]) || !groups.contains_key(&v[1]),
        );
    })?;
    each("spawn_group_squad", &["id"], &mut |v| {
        add(&mut counts, "missing_group", !groups.contains_key(&v[0]));
        add(
            &mut counts,
            "squad_overrides_entries",
            groups.contains_key(&v[0]) && entries.contains(&v[0]),
        );
    })?;
    let mut masters = BTreeMap::<(u32, u32), u64>::new();
    for &(entry, map) in spawns[0].values() {
        *masters.entry((entry, map)).or_default() += 1;
    }
    for by_entry in [false, true] {
        let (table, names): (&str, &[&str]) = if by_entry {
            (
                "creature_linking_template",
                &["entry", "master_entry", "flag", "map", "search_range"],
            )
        } else {
            ("creature_linking", &["guid", "master_guid", "flag"])
        };
        // Linking rejects cross-map GUID masters and ambiguous entry masters, CreatureLinkingMgr.cpp:211-249.
        each(table, names, &mut |v| {
            let slave = v[0];
            let master = v[1];
            let flag = v[2] & 65535;
            let exists = |id| {
                if by_entry {
                    templates[0].contains(&id)
                } else {
                    spawns[0].contains_key(&id)
                }
            };
            add(&mut counts, "link_missing_slave", !exists(slave));
            add(&mut counts, "link_missing_master", !exists(master));
            if !by_entry {
                if let (Some(s), Some(m)) = (spawns[0].get(&slave), spawns[0].get(&master)) {
                    add(&mut counts, "link_cross_map", s.1 != m.1);
                }
            }
            add(
                &mut counts,
                "link_invalid_flag",
                flag == 0 || flag & 32768 != 0,
            );
            add(
                &mut counts,
                "link_self_despawn",
                flag & 256 != 0 && slave == master,
            );
            if by_entry && v[4] & 65535 == 0 && flag & 3584 != 0 {
                add(
                    &mut counts,
                    "link_nonunique_master",
                    masters.get(&(master, v[3])).copied().unwrap_or(0) != 1,
                );
            }
        })?;
    }
    for (table, owner) in [
        ("creature_movement", "id"),
        ("creature_movement_template", "entry"),
        ("waypoint_path", "pathid"),
    ] {
        let template = table.ends_with("_template");
        let names = if template {
            vec![owner, "point", "pathid"]
        } else {
            vec![owner, "point"]
        };
        let mut paths = BTreeMap::<u32, BTreeSet<u32>>::new();
        each(table, &names, &mut |v| {
            let key = if template {
                v[0].wrapping_shl(8).wrapping_add(v[2])
            } else {
                v[0]
            };
            paths.entry(key).or_default().insert(v[1]);
            if table != "waypoint_path" {
                add(
                    &mut counts,
                    "waypoint_missing_owner",
                    if template {
                        !templates[0].contains(&v[0])
                    } else {
                        !spawns[0].contains_key(&v[0])
                    },
                );
            }
        })?;
        for points in paths.values() {
            add(&mut counts, "waypoint_zero_paths", points.contains(&0));
            let mut prev = None;
            for &p in points {
                if let Some(previous) = prev {
                    add(&mut counts, "waypoint_gap_edges", p != previous + 1);
                }
                prev = Some(p);
            }
        }
    }
    Ok(json!(counts))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typed_members_groups_without_formation_links_and_squads() {
        let result = analyze(|table, names, visit| {
            let rows: &[&[u32]] = match (table, names) {
                ("creature" | "gameobject", _) => &[&[7, 11, 0], &[8, 11, 1]],
                ("creature_template" | "gameobject_template", _) => &[&[11]],
                ("spawn_group", _) => &[&[1, 0], &[2, 1]],
                ("spawn_group_formation", _) => &[&[1]],
                ("spawn_group_spawn", _) => &[&[1, 7], &[2, 7], &[1, 7], &[1, 8]],
                ("spawn_group_entry", ["id"]) => &[&[1]],
                ("spawn_group_entry", _) => &[&[1, 11]],
                ("spawn_group_squad", _) => &[&[1]],
                ("spawn_group_linked_group", _) => &[&[1, 2], &[2, 99]],
                _ => &[],
            };
            for row in rows {
                visit(row);
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(result["duplicate_membership"], 1);
        assert_eq!(result["groups_without_formation"], 1);
        assert_eq!(result["cross_map_groups"], 1);
        assert_eq!(result["squad_overrides_entries"], 1);
        assert_eq!(result["linked_missing_group"], 1);
        assert_eq!(result["missing_entry"], 0);
    }
    #[test]
    fn linking_refusals_and_waypoint_gaps_preserve_source_identity() {
        let result = analyze(|table, _, visit| {
            let rows: &[&[u32]] = match table {
                "creature" => &[&[7, 11, 0], &[8, 11, 1]],
                "creature_template" => &[&[11]],
                "creature_linking" => &[&[7, 8, 1], &[99, 100, 0], &[7, 7, 256]],
                "creature_linking_template" => &[&[11, 11, 512, 2, 0]],
                "creature_movement" => &[&[7, 0], &[7, 2], &[7, 5]],
                "creature_movement_template" => &[&[11, 1, 1], &[11, 1, 2]],
                _ => &[],
            };
            for row in rows {
                visit(row);
            }
            Ok(())
        })
        .unwrap();
        for key in [
            "link_cross_map",
            "link_missing_slave",
            "link_missing_master",
            "link_invalid_flag",
            "link_self_despawn",
            "link_nonunique_master",
            "waypoint_zero_paths",
        ] {
            assert_eq!(result[key], 1, "{key}");
        }
        assert_eq!(result["waypoint_gap_edges"], 2);
        assert_eq!(result["waypoint_missing_owner"], 0);
    }
}
