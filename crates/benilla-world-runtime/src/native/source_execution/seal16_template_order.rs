// SPDX-License-Identifier: GPL-3.0-or-later
//! Frozen MyISAM INSERT order from the primary-ordered, hash-checked reset snapshot.
use super::*;

fn entries(raw: &[u8], table: &str) -> Result<Vec<u32>> {
    let (create, insert, values, width): (&[u8], &[u8], &[u8], usize) = match table {
        "gameobject_template" => (
            b"CREATE TABLE `gameobject_template` (",
            b"INSERT INTO `gameobject_template`",
            b"INSERT INTO `gameobject_template` VALUES",
            38,
        ),
        "creature_template" => (
            b"CREATE TABLE `creature_template` (",
            b"INSERT INTO `creature_template`",
            b"INSERT INTO `creature_template` VALUES",
            99,
        ),
        _ => return Err(refuse("execution.templates.snapshot_relation")),
    };
    let mut created = false;
    let mut active = false;
    let mut output = Vec::new();
    for line in raw.split(|byte| *byte == b'\n') {
        let line = line.trim_ascii();
        if line == create {
            created = true;
        }
        let mut rest = if line.starts_with(insert) {
            require(
                created && !active,
                "execution.templates.snapshot_schema_owner",
            )?;
            active = true;
            line.strip_prefix(values)
                .ok_or_else(|| refuse("execution.templates.snapshot_projection"))?
                .trim_ascii_start()
        } else if active {
            line
        } else {
            continue;
        };
        while !rest.is_empty() {
            rest = rest
                .strip_prefix(b"(")
                .ok_or_else(|| refuse("execution.templates.snapshot_tuple"))?;
            let mut entry = None;
            for column in 0..width {
                let (value, next) =
                    crate::native::source_execution::manager_inputs::sql_value(rest)?;
                if column == 0 {
                    entry = Some(
                        std::str::from_utf8(
                            value
                                .as_deref()
                                .ok_or_else(|| refuse("execution.templates.snapshot_entry"))?,
                        )
                        .map_err(|_| refuse("execution.templates.snapshot_entry"))?
                        .parse::<u32>()
                        .map_err(|_| refuse("execution.templates.snapshot_entry"))?,
                    );
                }
                rest = next
                    .strip_prefix(if column + 1 == width { b")" } else { b"," })
                    .ok_or_else(|| refuse("execution.templates.snapshot_columns"))?;
            }
            output.push(entry.unwrap());
            if rest == b";" {
                active = false;
                break;
            }
            rest = rest
                .strip_prefix(b",")
                .ok_or_else(|| refuse("execution.templates.snapshot_tuple"))?
                .trim_ascii_start();
        }
    }
    require(created, "execution.templates.snapshot_schema_owner")?;
    require(!active, "execution.templates.snapshot_truncated")?;
    require(
        output.windows(2).all(|pair| pair[0] < pair[1]),
        "execution.templates.snapshot_primary_order",
    )?;
    Ok(output)
}
pub(super) fn snapshot_order(recording: &Path) -> Result<[Vec<u32>; 2]> {
    let root = recording
        .ancestors()
        .find(|path| path.ends_with("benilla-config/full-server/f1"))
        .ok_or_else(|| refuse("execution.startup.recorded_f1_path_component_missing"))?;
    let reset: serde_json::Value = serde_json::from_slice(
        &std::fs::read(recording.join("reset.json"))
            .map_err(|_| refuse("execution.templates.reset_owner"))?,
    )
    .map_err(|_| refuse("execution.templates.reset_owner"))?;
    let classic = reset["databases"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["database"] == "classicmangos"))
        .ok_or_else(|| refuse("execution.templates.reset_classic_owner"))?;
    let approved =
        Sha256Id::parse("31c558ac2cc3e48ec5b64117c7fbc70bb4a8c9f0e56c11d0b48dfc5a9ed8e403")?;
    require(
        classic["sha256"] == approved.as_str() && classic["exit"] == 0,
        "execution.templates.reset_identity",
    )?;
    let raw = super::super::retained_f1_file(
        root,
        Path::new(
            classic["snapshot"]
                .as_str()
                .ok_or_else(|| refuse("execution.templates.reset_snapshot_owner"))?,
        ),
        &approved,
    )?;
    Ok([
        entries(&raw, "gameobject_template")?,
        entries(&raw, "creature_template")?,
    ])
}
pub(super) fn bind_rows<const N: usize, const S: usize>(
    mut rows: Vec<SqlTemplateRow<N, S>>,
    order: &[u32],
) -> Result<Vec<SqlTemplateRow<N, S>>> {
    // Reference environment: db-snapshot.py:9 --order-by-primary; seal16-full-01/reset.json:5-7 SHA256 31c558ac2cc3e48ec5b64117c7fbc70bb4a8c9f0e56c11d0b48dfc5a9ed8e403; mangos.sql:1325,2116 PRIMARY KEY(entry).
    rows.sort_unstable_by_key(|row| row.words[0]);
    require(
        rows.iter()
            .map(|row| row.words[0])
            .eq(order.iter().copied()),
        "execution.templates.snapshot_catalog_entries",
    )?;
    Ok(rows)
}
#[test]
fn restored_template_reader_refuses_other_relations_columns_and_key_orders() {
    let tuple = |entry| {
        let mut values = vec!["0".to_owned(); 38];
        values[0] = format!("{entry}");
        values[3] = "'name),(with\\\'quote'".into();
        format!("({})", values.join(","))
    };
    let raw = format!("CREATE TABLE `gameobject_template` (\nINSERT INTO `gameobject_template` VALUES\n{},\n{};\n", tuple(2), tuple(4));
    assert_eq!(
        entries(raw.as_bytes(), "gameobject_template").unwrap(),
        [2, 4]
    );
    assert_eq!(
        entries(raw.as_bytes(), "gameobject").unwrap_err().id,
        "execution.templates.snapshot_relation"
    );
    assert_eq!(entries(b"CREATE TABLE `gameobject_template` (\nINSERT INTO `gameobject_template` VALUES (2);\n", "gameobject_template").unwrap_err().id, "execution.templates.snapshot_columns");
    assert_eq!(entries(b"CREATE TABLE `gameobject_template` (\nINSERT INTO `gameobject_template` (`entry`) VALUES (2);\n", "gameobject_template").unwrap_err().id, "execution.templates.snapshot_projection");
    let raw = format!(
        "CREATE TABLE `gameobject_template` (\nINSERT INTO `gameobject_template` VALUES {},{};\n",
        tuple(4),
        tuple(2)
    );
    assert_eq!(
        entries(raw.as_bytes(), "gameobject_template")
            .unwrap_err()
            .id,
        "execution.templates.snapshot_primary_order"
    );
    let raw = format!(
        "CREATE TABLE `creature_template` (\nINSERT INTO `creature_template` VALUES ({});\n",
        std::iter::repeat_n("0", 99).collect::<Vec<_>>().join(",")
    );
    assert_eq!(entries(raw.as_bytes(), "creature_template").unwrap(), [0]);
    let row = |entry| {
        let mut words = [0; 38];
        words[0] = entry;
        LoadedGameObjectInfo {
            ordinal: u64::from(entry),
            words,
            strings: std::array::from_fn(|_| Vec::new()),
        }
    };
    assert_eq!(
        bind_rows(vec![row(4), row(2)], &[2, 4])
            .unwrap()
            .iter()
            .map(|row| row.words[0])
            .collect::<Vec<_>>(),
        [2, 4]
    );
    assert_eq!(
        bind_rows(vec![row(4), row(2)], &[2, 3]).unwrap_err().id,
        "execution.templates.snapshot_catalog_entries"
    );
}

#[test]
fn restored_primary_order_drives_the_raw_transport_display_vector() {
    struct NoDependencies;
    impl TemplateValidationOwner for NoDependencies {
        fn contains(&mut self, _: TemplateLookup, _: u32) -> Result<bool> {
            Err(refuse("authored.unexpected_validation_dependency"))
        }
        fn display_scale(&mut self, _: u32) -> Result<Option<u32>> {
            Err(refuse("authored.unexpected_display_dependency"))
        }
        fn class_stats(&mut self, _: u32, _: u32) -> Result<bool> {
            Err(refuse("authored.unexpected_class_dependency"))
        }
    }
    let row = |entry| {
        let mut words = [0; 38];
        words[0] = entry;
        words[1] = 11;
        words[2] = entry * 10;
        words[8] = 1.0f32.to_bits();
        LoadedGameObjectInfo {
            ordinal: u64::from(entry),
            words,
            strings: std::array::from_fn(|_| Vec::new()),
        }
    };
    let raw = LoadedGameObjectTemplates::load(vec![row(4), row(2)], &mut NoDependencies).unwrap();
    assert_eq!(raw.transport_displays(), [40, 20]);
    let restored = bind_rows(vec![row(4), row(2)], &[2, 4]).unwrap();
    let loaded = LoadedGameObjectTemplates::load(restored, &mut NoDependencies).unwrap();
    assert_eq!(loaded.transport_displays(), [20, 40]);
}
