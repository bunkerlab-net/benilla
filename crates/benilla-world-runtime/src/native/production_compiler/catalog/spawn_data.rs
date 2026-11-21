// SPDX-License-Identifier: GPL-3.0-or-later
//! Fixed loader projections, ObjectMgr.cpp:1762,2252.
use super::*;
use crate::native::source_execution::CreatureSpawnTemplateRow;
const CREATURE: &str = "Entry NpcFlags UnitFlags Faction ModelId EquipmentId CurHealth CurMana SpawnFlags RelayId StringId Name";
const GO: &str = "guid animprogress state stringId";
fn projection(table: &str, names: &[&str]) -> Result<()> {
    let columns = match table {
        "creature_spawn_data_template" => CREATURE,
        "gameobject_addon" => GO,
        _ => return Err(fail("spawn_data.relation")),
    };
    check(
        names.iter().copied().eq(columns.split_whitespace()),
        "spawn_data.projection",
    )
}
fn uint64(raw: &[u8]) -> Result<u64> {
    let raw = raw.trim_ascii_start();
    let negative = raw.first() == Some(&b'-');
    let start = usize::from(matches!(raw.first(), Some(b'+' | b'-')));
    let mut value = 0u64;
    for &b in &raw[start..] {
        if !b.is_ascii_digit() {
            break;
        }
        value = value
            .checked_mul(10)
            .and_then(|n| n.checked_add(u64::from(b - b'0')))
            .ok_or_else(|| fail("spawn_data.uint64_scan_overflow_undefined"))?;
    }
    Ok(if negative {
        value.wrapping_neg()
    } else {
        value
    })
}
fn creature(fields: &[Vec<u8>]) -> Result<CreatureSpawnTemplateRow> {
    check(fields.len() == 12, "spawn_data.creature_columns")?;
    let mut values = [0; 11];
    for (column, value) in values.iter_mut().enumerate() {
        *value = if column == 2 {
            uint64(&fields[column])?
        } else {
            u64::from(spawns::uint(Some(&fields[column]))?)
        };
    }
    Ok(CreatureSpawnTemplateRow {
        values,
        name: fields[11]
            .split(|b| *b == 0)
            .next()
            .unwrap_or_default()
            .to_vec(),
    })
}
fn addon(fields: &[Vec<u8>]) -> Result<[u32; 4]> {
    check(fields.len() == 4, "spawn_data.addon_columns")?;
    let mut values = [0; 4];
    for (value, bytes) in values.iter_mut().zip(fields) {
        *value = spawns::uint(Some(bytes))?;
    }
    Ok(values)
}
fn primary_key(row: &CreatureSpawnTemplateRow) -> (u32, i32, i64, u32, i32, u32, u32, u32) {
    let v = &row.values;
    (
        v[0] as u32,
        v[1] as u32 as i32,
        v[2] as i64,
        v[4] as u32,
        v[5] as u32 as i32,
        v[6] as u32,
        v[7] as u32,
        v[8] as u32,
    )
}
fn order_creature_rows(rows: &mut [CreatureSpawnTemplateRow]) -> Result<()> {
    rows.sort_by_key(primary_key);
    check(
        rows.windows(2)
            .all(|pair| primary_key(&pair[0]) < primary_key(&pair[1])),
        "spawn_data.duplicate_key",
    )
}
impl VerifiedCatalog {
    fn spawn_data_fields(&mut self, table: &str, names: &[&str]) -> Result<Vec<Vec<Vec<u8>>>> {
        projection(table, names)?;
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        let columns = spawns::columns(&schema, table)?;
        let positions = names
            .iter()
            .map(|name| spawns::position(&columns, name))
            .collect::<Result<Vec<_>>>()?;
        let mut rows = Vec::new();
        self.scan(table, 0, 256, |row| {
            let mut cells = [None; 256];
            spawns::fill(&row, &mut cells)?;
            rows.push(
                positions
                    .iter()
                    .map(|&p| cells[p].unwrap_or_default().to_vec())
                    .collect::<Vec<_>>(),
            );
            check(
                rows.len() <= DECODED / (names.len() * std::mem::size_of::<Vec<u8>>()),
                "spawn_data.view_limit",
            )
        })?;
        Ok(rows)
    }
    /// Restored primary-key order is reference-environment behaviour, not an SQL guarantee.
    pub(crate) fn execution_creature_spawn_templates(
        &mut self,
    ) -> Result<Vec<CreatureSpawnTemplateRow>> {
        let mut rows = self
            .spawn_data_fields(
                "creature_spawn_data_template",
                &CREATURE.split_whitespace().collect::<Vec<_>>(),
            )?
            .iter()
            .map(|fields| creature(fields))
            .collect::<Result<Vec<_>>>()?;
        order_creature_rows(&mut rows)?;
        Ok(rows)
    }
    pub(crate) fn execution_gameobject_addons(&mut self) -> Result<Vec<[u32; 4]>> {
        let mut rows = self
            .spawn_data_fields(
                "gameobject_addon",
                &GO.split_whitespace().collect::<Vec<_>>(),
            )?
            .iter()
            .map(|fields| addon(fields))
            .collect::<Result<Vec<_>>>()?;
        rows.sort_by_key(|row| row[0]);
        check(
            rows.windows(2).all(|pair| pair[0][0] < pair[1][0]),
            "spawn_data.duplicate_key",
        )?;
        Ok(rows)
    }
    #[cfg(test)]
    pub(crate) fn execution_check_spawn_data_trace(
        &mut self,
        rows: &[CreatureSpawnTemplateRow],
        addons: &[[u32; 4]],
        trace: &[Vec<Vec<Vec<u8>>>; 2],
    ) -> Result<()> {
        check(
            trace[0]
                .iter()
                .map(|fields| creature(fields))
                .collect::<Result<Vec<_>>>()?
                == rows,
            "spawn_data.template_trace_order_or_fields",
        )?;
        check(
            trace[1]
                .iter()
                .map(|fields| addon(fields))
                .collect::<Result<Vec<_>>>()?
                == addons,
            "spawn_data.addon_trace_order_or_fields",
        )
    }
}
#[test]
fn spawn_data_readers_refuse_unlisted_relations_and_columns() {
    for (table, columns) in [
        ("creature_spawn_data_template", CREATURE),
        ("gameobject_addon", GO),
    ] {
        let names = columns.split_whitespace().collect::<Vec<_>>();
        projection(table, &names).unwrap();
        assert_eq!(
            projection(table, &names[..1]).unwrap_err().id,
            "spawn_data.projection"
        );
        assert_eq!(
            projection(table, &["wrong"]).unwrap_err().id,
            "spawn_data.projection"
        );
    }
    for table in [
        "creature",
        "creature_template",
        "gameobject_template",
        "string_id",
    ] {
        assert_eq!(
            projection(table, &["Entry"]).unwrap_err().id,
            "spawn_data.relation"
        );
    }
}
#[test]
fn spawn_data_unsigned_scan_preserves_signed_bits_and_refuses_overflow() {
    assert_eq!(uint64(b"18446744073709551615").unwrap(), u64::MAX);
    assert_eq!(uint64(b"-1").unwrap(), u64::MAX);
    assert_eq!(uint64(b"0").unwrap(), 0);
    assert_eq!(
        uint64(b"18446744073709551616").unwrap_err().id,
        "spawn_data.uint64_scan_overflow_undefined"
    );
}

#[test]
fn spawn_data_primary_order_uses_the_complete_signed_composite_key() {
    let row = |npc, unit, model, equipment| CreatureSpawnTemplateRow {
        values: [7, npc, unit, 0, model, equipment, 0, 0, 0, 0, 0],
        name: b"named".to_vec(),
    };
    let first = row(u32::MAX.into(), u64::MAX, 0, u32::MAX.into());
    let second = row(u32::MAX.into(), 0, 0, u32::MAX.into());
    let third = row(u32::MAX.into(), 0, 0, 0);
    let fourth = row(0, u64::MAX, 0, u32::MAX.into());
    let mut rows = [fourth.clone(), third.clone(), first.clone(), second.clone()];
    order_creature_rows(&mut rows).unwrap();
    assert_eq!(rows, [first.clone(), second, third, fourth]);
    assert_eq!(
        order_creature_rows(&mut [first.clone(), first])
            .unwrap_err()
            .id,
        "spawn_data.duplicate_key"
    );
}
