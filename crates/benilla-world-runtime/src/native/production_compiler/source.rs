// SPDX-License-Identifier: GPL-3.0-or-later
// classic_export/core.py and capture.py own the source-v2 contract.
use super::{check, fail, json::*, Result};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

pub(super) const MAX_BUNDLE: usize = 64 * 1024 * 1024;
pub(super) const CORE: &str = "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f";
pub(super) const DATABASE: &str = "ec4f596146be6467ea93c57397858e329e2db852";
pub(super) static CONTRACT: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!("schema_contract.json")).expect("reviewed schema metadata")
});

pub(super) fn names(value: &Value) -> BTreeSet<&str> {
    value
        .as_array()
        .expect("contract list")
        .iter()
        .map(|v| v.as_str().expect("contract name"))
        .collect()
}
pub(super) fn contract_names(group: &str) -> BTreeSet<&'static str> {
    names(&CONTRACT[group])
}
pub(super) fn columns(table: &str) -> Vec<&'static str> {
    CONTRACT["columns"][table]
        .as_array()
        .expect("reviewed table")
        .iter()
        .map(|v| v.as_str().expect("column"))
        .collect()
}
pub(super) fn keys(table: &str) -> Vec<&'static str> {
    CONTRACT["keys"][table]
        .as_array()
        .expect("reviewed key")
        .iter()
        .map(|v| v.as_str().expect("key"))
        .collect()
}
pub(super) fn rows<'a>(bundle: &'a Value, table: &str) -> &'a [Value] {
    bundle["tables"][table]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
}
pub(super) fn ids(bundle: &Value, table: &str, field: &str) -> BTreeSet<i128> {
    rows(bundle, table).iter().map(|r| int(&r[field])).collect()
}

#[derive(Clone, Debug)]
pub(super) struct Selection {
    pub map: i128,
    pub min: [i128; 3],
    pub max: [i128; 3],
    pub quests: BTreeSet<i128>,
    pub spells: BTreeSet<i128>,
    pub starter: Option<(i128, i128, i128)>,
}
impl Selection {
    pub fn parse(v: &Value) -> Result<Self> {
        object(v, "selection.fields")?;
        let version = integer(&v["format_version"], 1, 2, "selection.version")?;
        let mut expected = vec![
            "format_version",
            "map_id",
            "min_milliunits",
            "max_milliunits",
            "quest_ids",
            "spell_ids",
        ];
        if version == 2 {
            expected.push("starter");
        }
        fields(v, &expected, "selection.fields")?;
        let starter = if version == 2 {
            let s = &v["starter"];
            fields(
                s,
                &["race", "class", "level_cap"],
                "selection.starter_fields",
            )?;
            let race = integer(&s["race"], 1, 8, "selection.starter_race")?;
            let class = integer(&s["class"], 1, 11, "selection.starter_class")?;
            check(
                [1, 2, 3, 4, 5, 7, 8, 9, 11].contains(&class),
                "selection.starter_class",
            )?;
            Some((
                race,
                class,
                integer(&s["level_cap"], 1, 60, "selection.starter_level_cap")?,
            ))
        } else {
            None
        };
        let map = integer(&v["map_id"], 0, 65535, "selection.map")?;
        let coords = |name: &str| -> Result<[i128; 3]> {
            let a = array(&v[name], "selection.coordinates")?;
            check(a.len() == 3, "selection.coordinates")?;
            Ok([
                integer(&a[0], -100_000_000, 100_000_000, "selection.coordinate")?,
                integer(&a[1], -100_000_000, 100_000_000, "selection.coordinate")?,
                integer(&a[2], -100_000_000, 100_000_000, "selection.coordinate")?,
            ])
        };
        let min = coords("min_milliunits")?;
        let max = coords("max_milliunits")?;
        check(
            min.iter().zip(max).all(|(a, b)| *a <= b),
            "selection.reversed_bounds",
        )?;
        let selected = |name: &str| -> Result<BTreeSet<i128>> {
            let a = array(&v[name], "selection.id_limit")?;
            check(a.len() <= 512, "selection.id_limit")?;
            let result = a
                .iter()
                .map(|x| integer(x, 1, 0xffff_ffff, "selection.id"))
                .collect::<Result<BTreeSet<_>>>()?;
            check(result.len() == a.len(), "selection.duplicate_id")?;
            Ok(result)
        };
        Ok(Self {
            map,
            min,
            max,
            quests: selected("quest_ids")?,
            spells: selected("spell_ids")?,
            starter,
        })
    }
    pub fn json(&self) -> Value {
        let mut v = json!({"format_version":if self.starter.is_some(){2}else{1}, "map_id":self.map as i64,"min_milliunits":self.min.map(|v|v as i64),"max_milliunits":self.max.map(|v|v as i64),"quest_ids":self.quests.iter().map(|v|*v as u32).collect::<Vec<_>>(),"spell_ids":self.spells.iter().map(|v|*v as u32).collect::<Vec<_>>()});
        if let Some((race, class, cap)) = self.starter {
            v["starter"] = json!({"race":race as u32,"class":class as u32,"level_cap":cap as u32});
        }
        v
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SqlType {
    pub family: String,
    pub precision: Option<usize>,
    pub scale: Option<usize>,
    pub unsigned: bool,
}
fn integer_family(f: &str) -> bool {
    [
        "tinyint",
        "smallint",
        "mediumint",
        "int",
        "integer",
        "bigint",
        "bit",
    ]
    .contains(&f)
}
fn decimal_family(f: &str) -> bool {
    ["float", "double", "decimal", "numeric", "real"].contains(&f)
}
fn text_family(f: &str) -> bool {
    [
        "char",
        "varchar",
        "text",
        "tinytext",
        "mediumtext",
        "longtext",
    ]
    .contains(&f)
}
pub(super) fn sql_type(raw: &str) -> Result<SqlType> {
    let raw = raw.to_ascii_lowercase();
    let (raw, unsigned) = raw
        .strip_suffix(" unsigned")
        .map_or((raw.as_str(), false), |s| (s, true));
    let (family, precision, scale) = if let Some((f, rest)) = raw.split_once('(') {
        let args = rest
            .strip_suffix(')')
            .ok_or_else(|| fail("schema.sql_type"))?;
        let mut parts = args.split(',');
        let number = |s: &str| -> Result<usize> {
            check(
                !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()),
                "schema.sql_type",
            )?;
            s.parse().map_err(|_| fail("schema.sql_type"))
        };
        let first = number(parts.next().ok_or_else(|| fail("schema.sql_type"))?)?;
        let second = parts.next().map(number).transpose()?;
        check(parts.next().is_none(), "schema.sql_type")?;
        (f, Some(first), second)
    } else {
        (raw, None, None)
    };
    check(
        !family.is_empty()
            && family.bytes().all(|b| b.is_ascii_lowercase())
            && (integer_family(family) || decimal_family(family) || text_family(family)),
        "schema.sql_type",
    )?;
    match family {
        "decimal" | "numeric" => check(
            precision.is_some_and(|p| (1..=65).contains(&p))
                && scale.is_some_and(|s| s <= precision.unwrap().min(30)),
            "schema.decimal_type",
        )?,
        "bit" => check(
            precision.is_some_and(|p| (1..=64).contains(&p)) && scale.is_none(),
            "schema.bit_type",
        )?,
        _ => {
            check(scale.is_none(), "schema.sql_type")?;
            match family {
                "char" | "varchar" => check(
                    precision.is_some_and(|p| (1..=65535).contains(&p)),
                    "schema.string_type",
                )?,
                f if integer_family(f) => check(
                    precision.is_none_or(|p| (1..=255).contains(&p)),
                    "schema.integer_type",
                )?,
                _ => check(precision.is_none(), "schema.sql_type")?,
            }
        }
    }
    Ok(SqlType {
        family: family.into(),
        precision,
        scale,
        unsigned,
    })
}
#[derive(Clone, Debug)]
pub(super) struct Column {
    pub kind: SqlType,
    pub nullable: bool,
}
#[derive(Clone, Debug)]
pub(super) struct Profile {
    pub columns: BTreeMap<String, Column>,
    pub extra: Vec<String>,
    pub engine: String,
}
pub(super) type Profiles = BTreeMap<String, Profile>;

pub(super) fn schema(inventory: &Value, required: &BTreeSet<&str>) -> Result<Profiles> {
    fields(inventory, &["format_version", "tables"], "schema.fields")?;
    integer(&inventory["format_version"], 1, 1, "schema.version")?;
    let tables = object(&inventory["tables"], "schema.tables")?;
    check(
        tables.keys().all(|k| CONTRACT["columns"].get(k).is_some()),
        "schema.unreviewed_table",
    )?;
    check(
        required.iter().all(|k| tables.contains_key(*k)),
        "schema.missing_table",
    )?;
    let mut result = Profiles::new();
    for (table, meta) in tables {
        let mut expected = vec!["engine", "table_type", "columns"];
        let pk_allowed = [
            "registry",
            "hooks",
            "loot",
            "spawn_entry",
            "spawn_context",
            "presentation",
        ]
        .iter()
        .any(|g| contract_names(g).contains(table.as_str()))
            || CONTRACT["event_text"].get(table).is_some()
            || super::extensions::quest_mechanics::is_table(table)
            || super::extensions::participants::is_table(table)
            || ["spell_template", "spell_facing"].contains(&table.as_str());
        if pk_allowed && meta.get("primary_key").is_some() {
            expected.push("primary_key");
        }
        fields(meta, &expected, "schema.table_fields")?;
        for (group, code) in [
            ("registry", "schema.registry_primary_key"),
            ("hooks", "schema.initial_hook_primary_key"),
            ("loot", "schema.loot_primary_key"),
            ("spawn_entry", "schema.spawn_entry_primary_key"),
            ("spawn_context", "schema.spawn_context_primary_key"),
        ] {
            if required.contains(table.as_str()) && contract_names(group).contains(table.as_str()) {
                check(meta["primary_key"] == CONTRACT["keys"][table], code)?;
            }
        }
        check(meta["table_type"] == "BASE TABLE", "schema.views_rejected")?;
        check(
            meta["engine"] == "InnoDB" || meta["engine"] == "MyISAM",
            "schema.engine",
        )?;
        let cols = array(&meta["columns"], "schema.columns")?;
        check(cols.len() <= 512, "schema.columns")?;
        let mut by_name = BTreeMap::new();
        let mut ordinals = BTreeSet::new();
        for col in cols {
            fields(
                col,
                &["name", "type", "nullable", "ordinal"],
                "schema.column_fields",
            )?;
            let name = string(&col["name"], "schema.column_name")?;
            check(
                name.chars().count() <= 64
                    && !name.is_empty()
                    && name
                        .chars()
                        .filter(|c| *c != '_')
                        .all(char::is_alphanumeric)
                    && name.chars().any(|c| c != '_'),
                "schema.column_name",
            )?;
            check(
                by_name.insert(name.to_lowercase(), col).is_none(),
                "schema.duplicate_column",
            )?;
            check(
                string(&col["type"], "schema.column_type")?.chars().count() <= 128,
                "schema.column_type",
            )?;
            check(col["nullable"].is_boolean(), "schema.nullability")?;
            let ordinal = integer(&col["ordinal"], 1, 512, "schema.ordinal")?;
            check(ordinals.insert(ordinal), "schema.duplicate_ordinal")?;
        }
        let wanted: BTreeSet<_> = columns(table)
            .into_iter()
            .map(|s| s.to_lowercase())
            .collect();
        if required.contains(table.as_str()) {
            check(
                wanted.iter().all(|n| by_name.contains_key(n)),
                &format!("schema.missing_columns:{table}"),
            )?;
        }
        for (group, prefix) in [("strict", "starter"), ("event_text", "event_text")] {
            if required.contains(table.as_str()) {
                if let Some(spec) = CONTRACT[group].get(table) {
                    check(
                        wanted == by_name.keys().cloned().collect(),
                        &format!("schema.{prefix}_columns:{table}"),
                    )?;
                    let mut source_names = Map::new();
                    for (idx, entry) in spec.as_array().unwrap().iter().enumerate() {
                        let name = entry[0].as_str().unwrap();
                        let actual = by_name[&name.to_lowercase()];
                        let observed = sql_type(actual["type"].as_str().unwrap())?;
                        let expected = sql_type(entry[1].as_str().unwrap())?;
                        let mut equal = if integer_family(&expected.family) {
                            observed.family == expected.family
                                && observed.unsigned == expected.unsigned
                        } else {
                            observed == expected
                        };
                        if group == "event_text" {
                            let allowed = CONTRACT["event_aliases"][table].get(name);
                            check(
                                allowed.map_or(actual["name"] == name, |a| {
                                    a.as_array().unwrap().contains(&actual["name"])
                                }),
                                &format!("schema.event_text_spelling:{table}:{name}"),
                            )?;
                            if table == "creature_ai_scripts" && name == "id" {
                                equal = observed.family == "int";
                            }
                            source_names.insert(name.into(), actual["name"].clone());
                        }
                        check(
                            equal
                                && actual["nullable"] == entry[2]
                                && int(&actual["ordinal"]) == idx as i128 + 1,
                            &format!("schema.{prefix}_type:{table}:{name}"),
                        )?;
                    }
                    if group == "event_text" {
                        let pk: Vec<_> = CONTRACT["event_primary_keys"][table]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|n| source_names[n.as_str().unwrap()].clone())
                            .collect();
                        check(
                            meta["primary_key"] == json!(pk),
                            &format!("schema.event_text_primary_key:{table}"),
                        )?;
                    }
                }
            }
        }
        let mut checked = BTreeMap::new();
        for name in columns(table) {
            if let Some(col) = by_name.get(&name.to_lowercase()) {
                let kind = sql_type(col["type"].as_str().unwrap())?;
                let text = CONTRACT["text"][table]
                    .as_array()
                    .is_some_and(|a| a.iter().any(|v| v == name));
                check(
                    if text {
                        text_family(&kind.family)
                    } else {
                        integer_family(&kind.family) || decimal_family(&kind.family)
                    },
                    &format!("schema.type_family:{table}:{name}"),
                )?;
                if (table == "creature"
                    && [
                        "position_x",
                        "position_y",
                        "position_z",
                        "orientation",
                        "spawndist",
                    ]
                    .contains(&name))
                    || (table == "creature_model_info"
                        && ["bounding_radius", "combat_reach", "SpeedWalk", "SpeedRun"]
                            .contains(&name))
                {
                    check(
                        decimal_family(&kind.family),
                        &format!("schema.decimal_field:{table}:{name}"),
                    )?;
                }
                checked.insert(
                    name.into(),
                    Column {
                        kind,
                        nullable: col["nullable"].as_bool().unwrap(),
                    },
                );
            }
        }
        result.insert(
            table.clone(),
            Profile {
                columns: checked,
                extra: by_name
                    .keys()
                    .filter(|n| !wanted.contains(*n))
                    .cloned()
                    .collect(),
                engine: meta["engine"].as_str().unwrap().into(),
            },
        );
    }
    Ok(result)
}

pub(super) fn normalize_decimal(raw: &str, kind: &SqlType) -> Result<String> {
    check(raw.chars().count() <= 96, "row.decimal_text_limit")?;
    let cleaned = raw.trim().replace('_', "");
    let (negative, s) = cleaned
        .strip_prefix('-')
        .map_or((false, cleaned.as_str()), |s| (true, s));
    let s = if negative {
        s
    } else {
        s.strip_prefix('+').unwrap_or(s)
    };
    let (mantissa, exponent) = s.split_once(['e', 'E']).map_or(Ok((s, 0i32)), |(m, e)| {
        Ok((m, e.parse::<i32>().map_err(|_| fail("row.decimal"))?))
    })?;
    check(!mantissa.is_empty(), "row.decimal")?;
    if ["nan", "snan", "inf", "infinity"]
        .iter()
        .any(|n| mantissa.eq_ignore_ascii_case(n))
    {
        return Err(fail("row.decimal_range"));
    }
    let mut dot = false;
    let mut fraction = 0i32;
    let mut digits = String::new();
    for b in mantissa.bytes() {
        if b == b'.' && !dot {
            dot = true;
        } else {
            check(b.is_ascii_digit(), "row.decimal")?;
            digits.push(b as char);
            if dot {
                fraction += 1;
            }
        }
    }
    check(!digits.is_empty(), "row.decimal")?;
    let digits = digits.trim_start_matches('0');
    let exponent = exponent
        .checked_sub(fraction)
        .ok_or_else(|| fail("row.decimal_precision"))?;
    let decimal_position = digits.len() as i64 + exponent as i64;
    if !digits.is_empty() {
        check(
            decimal_position <= 31
                && (decimal_position < 31
                    || digits == "1"
                    || digits.starts_with('1') && digits[1..].bytes().all(|b| b == b'0')),
            "row.decimal_range",
        )?;
    }
    check(
        exponent >= -60 && digits.len().max(1) <= 64,
        "row.decimal_precision",
    )?;
    let mut result = if digits.is_empty() {
        check(
            exponent <= if negative { 94 } else { 95 },
            "row.decimal_precision",
        )?;
        "0".to_string()
    } else {
        check(
            (-96..=96).contains(&decimal_position),
            "row.decimal_precision",
        )?;
        let mut value = if decimal_position <= 0 {
            format!("0.{}{}", "0".repeat((-decimal_position) as usize), digits)
        } else if decimal_position >= digits.len() as i64 {
            format!(
                "{}{}",
                digits,
                "0".repeat(decimal_position as usize - digits.len())
            )
        } else {
            format!(
                "{}.{}",
                &digits[..decimal_position as usize],
                &digits[decimal_position as usize..]
            )
        };
        if value.contains('.') {
            value = value.trim_end_matches('0').trim_end_matches('.').into();
        }
        if negative {
            value.insert(0, '-');
        }
        value
    };
    check(result.len() <= 96, "row.decimal_precision")?;
    if kind.family == "decimal" || kind.family == "numeric" {
        let (whole, frac) = result
            .trim_start_matches('-')
            .split_once('.')
            .unwrap_or((result.trim_start_matches('-'), ""));
        check(
            whole.trim_start_matches('0').len() <= kind.precision.unwrap() - kind.scale.unwrap()
                && frac.len() <= kind.scale.unwrap(),
            "row.sql_decimal_precision",
        )?;
    }
    check(
        !kind.unsigned || !negative || result == "0",
        "row.sql_unsigned_decimal",
    )?;
    if result == "-0" {
        result = "0".into();
    }
    Ok(result)
}
pub(super) fn cell(value: &Value, column: &Column) -> Result<Value> {
    if value.is_null() {
        check(column.nullable, "row.unexpected_null")?;
        return Ok(Value::Null);
    }
    let kind = &column.kind;
    if text_family(&kind.family) {
        let text = string(value, "row.text_limit")?;
        check(text.len() <= 16384, "row.text_limit")?;
        check(!text.contains('\0'), "row.nul_text")?;
        check(
            !["char", "varchar"].contains(&kind.family.as_str())
                || text.chars().count() <= kind.precision.unwrap(),
            "row.sql_string_length",
        )?;
        return Ok(value.clone());
    }
    if integer_family(&kind.family) {
        let v = integer(value, i64::MIN as i128, u64::MAX as i128, "row.integer")?;
        let unsigned = kind.unsigned || kind.family == "bit";
        let bits = match kind.family.as_str() {
            "tinyint" => 8,
            "smallint" => 16,
            "mediumint" => 24,
            "int" | "integer" => 32,
            "bigint" => 64,
            "bit" => kind.precision.unwrap(),
            _ => unreachable!(),
        };
        check(
            v >= if unsigned { 0 } else { -(1i128 << (bits - 1)) }
                && v <= if unsigned {
                    (1i128 << bits) - 1
                } else {
                    (1i128 << (bits - 1)) - 1
                },
            "row.sql_integer_range",
        )?;
        return Ok(value.clone());
    }
    let raw = if let Some(obj) = value.as_object() {
        check(
            obj.len() == 1 && obj.contains_key("decimal"),
            "row.decimal_type",
        )?;
        string(&value["decimal"], "row.decimal_type")?.to_string()
    } else if value.is_number() {
        value.to_string()
    } else {
        string(value, "row.decimal_type")?.to_string()
    };
    Ok(json!({"decimal":normalize_decimal(&raw,kind)?}))
}
fn key_min(table: &str, key: &str) -> i128 {
    match (table, key) {
        ("game_event_creature", "event") => -32768,
        ("spell_area", "aura_spell") => -0x800000,
        ("creature_spawn_entry", "entry")
        | ("creature_linking_template", "map")
        | ("game_event_creature_data", "event")
        | ("playercreateinfo_action", "button")
        | ("playercreateinfo_skills", "raceMask" | "classMask")
        | ("creature_spawn_data_template", "Entry")
        | ("creature_loot_template" | "reference_loot_template", "item")
        | ("spell_area", "area" | "quest_start" | "quest_start_active" | "racemask" | "gender")
        | ("questgiver_greeting" | "locales_questgiver_greeting", "Type") => 0,
        _ => 1,
    }
}
pub(super) fn row_limit(table: &str) -> usize {
    match table {
        "quest_template"
        | "spell_template"
        | "playercreateinfo_spell"
        | "playercreateinfo_skills" => 512,
        "item_template" | "playercreateinfo_item" => 2048,
        "playercreateinfo" => 1,
        "playercreateinfo_action" => 120,
        "player_classlevelstats" | "player_levelstats" => 60,
        "player_xp_for_level" => 59,
        _ => 4096,
    }
}
pub(super) fn validate_rows(
    table: &str,
    value: &Value,
    profile: &Profile,
    limit: usize,
) -> Result<()> {
    let rows = array(value, "bundle.rows")?;
    check(rows.len() <= limit, &format!("rows.limit:{table}"))?;
    let cols = columns(table);
    check(
        profile.columns.len() == cols.len(),
        &format!("schema.missing_columns:{table}"),
    )?;
    let mut seen = BTreeSet::new();
    let mut previous: Option<Vec<Key>> = None;
    let mut total = 0;
    for row in rows {
        let obj = object(row, "bundle.row_type")?;
        check(
            obj.len() == cols.len() && cols.iter().all(|c| obj.contains_key(*c)),
            &format!("row.fields:{table}"),
        )?;
        for (name, col) in &profile.columns {
            check(
                cell(&row[name], col)? == row[name],
                "bundle.table_hash_or_order",
            )?;
        }
        if contract_names("presentation").contains(table) {
            check(
                profile.extra.is_empty(),
                &format!("quest_presentation.unreviewed_columns:{table}"),
            )?;
        }
        let mut key = Vec::new();
        for name in keys(table) {
            if table == "broadcast_text_locale" && name == "Locale" {
                let s = string(&row[name], &format!("row.key:{table}"))?;
                check(
                    !s.is_empty() && s.chars().count() <= 10,
                    &format!("row.key:{table}"),
                )?;
                key.push(Key::Text(s.into()));
            } else {
                key.push(Key::Integer(integer(
                    &row[name],
                    key_min(table, name),
                    u64::MAX as i128,
                    &format!("row.key:{table}"),
                )?));
            }
        }
        check(
            seen.insert(key.clone()),
            &format!("row.duplicate_key:{table}"),
        )?;
        check(
            previous.as_ref().is_none_or(|p| p < &key),
            "bundle.table_hash_or_order",
        )?;
        previous = Some(key);
        let size = bounded_len(row, 65536, &format!("row.byte_limit:{table}"))?;
        total += size;
        check(
            total <= 16 * 1024 * 1024,
            &format!("table.byte_limit:{table}"),
        )?;
    }
    Ok(())
}
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Integer(i128),
    Text(String),
}

pub(super) fn compare_decimal(left: &str, right: &str) -> std::cmp::Ordering {
    fn parts(value: &str) -> (bool, &str, &str) {
        let negative = value.starts_with('-');
        let value = value.strip_prefix('-').unwrap_or(value);
        let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
        let whole = whole.trim_start_matches('0');
        let fraction = fraction.trim_end_matches('0');
        (
            negative && (!whole.is_empty() || !fraction.is_empty()),
            whole,
            fraction,
        )
    }
    let a = parts(left);
    let b = parts(right);
    if a.0 != b.0 {
        return b.0.cmp(&a.0);
    }
    let order =
        a.1.len()
            .cmp(&b.1.len())
            .then_with(|| a.1.cmp(b.1))
            .then_with(|| a.2.cmp(b.2));
    if a.0 {
        order.reverse()
    } else {
        order
    }
}

pub(super) fn inside(value: &Value, low: i128, high: i128) -> bool {
    let s = value["decimal"].as_str().expect("verified decimal");
    // SQL decimals are exponent-free after row verification.
    let coordinate = |v: i128| {
        format!(
            "{}{}.{:03}",
            if v < 0 { "-" } else { "" },
            v.abs() / 1000,
            v.abs() % 1000
        )
    };
    compare_decimal(s, &coordinate(low)) != std::cmp::Ordering::Less
        && compare_decimal(s, &coordinate(high)) != std::cmp::Ordering::Greater
}

pub(super) fn validate(bundle: &Value) -> Result<(Selection, Profiles)> {
    let mut expected = vec![
        "format",
        "core_revision",
        "database_revision",
        "revision_claims_verified_against_installation",
        "client_build",
        "selection",
        "consistency",
        "schema",
        "schema_sha256",
        "db_version",
        "tables",
        "table_sha256",
        "required_ids",
        "report",
    ];
    let obj = object(bundle, "bundle.fields")?;
    if bundle["format"] == "benilla-classic-source-slice-v2" {
        expected.push("starter_required_keys");
        for k in [
            "event_ai_capture",
            "event_text_capture",
            "learning_capture",
            "native_spawn_capture",
            "spell_script_capture",
            "initial_hook_capture",
            "loot_capture",
            "loot_loader_capture",
            "spell_numeric_capture",
            "quest_giver_presentation_capture",
            "quest_mechanics_capture",
            "quest_endpoint_capture",
        ] {
            if obj.contains_key(k) {
                expected.push(k);
            }
        }
    }
    for k in [
        "quest_relationship_capture",
        "spawn_entry_capture",
        "spawn_context_capture",
        "skill_training_capture",
        "participants_capture",
    ] {
        if obj.contains_key(k) {
            expected.push(k);
        }
    }
    fields(bundle, &expected, "bundle.fields")?;
    check(
        [
            "benilla-classic-source-slice-v1",
            "benilla-classic-source-slice-v2",
        ]
        .iter()
        .any(|s| bundle["format"] == *s)
            && bundle["core_revision"] == CORE
            && bundle["database_revision"] == DATABASE
            && integer(&bundle["client_build"], 5875, 5875, "bundle.profile").is_ok(),
        "bundle.profile",
    )?;
    check(
        bundle["revision_claims_verified_against_installation"] == false,
        "bundle.unverified_provenance",
    )?;
    let selection = Selection::parse(&bundle["selection"])?;
    check(
        bundle["format"]
            == if selection.starter.is_some() {
                "benilla-classic-source-slice-v2"
            } else {
                "benilla-classic-source-slice-v1"
            },
        "bundle.selection_version",
    )?;
    check(
        bundle["consistency"] == "transactional_snapshot"
            || bundle["consistency"] == "operator_quiescent_snapshot",
        "bundle.consistency",
    )?;
    let tables = object(&bundle["tables"], "bundle.tables")?;
    check(
        tables.keys().all(|n| CONTRACT["keys"].get(n).is_some()),
        "bundle.tables",
    )?;
    check(
        !tables.contains_key("spell_facing"),
        "spell_numeric.facing_rows_require_extension",
    )?;
    check(
        selection.starter.is_some()
            || contract_names("starter")
                .iter()
                .all(|n| !tables.contains_key(*n)),
        "bundle.unselected_starter_tables",
    )?;
    let mut mandatory: BTreeSet<_> = [
        "creature",
        "creature_template",
        "creature_model_info",
        "creature_template_classlevelstats",
        "creature_questrelation",
        "creature_involvedrelation",
        "quest_template",
        "item_template",
    ]
    .into_iter()
    .collect();
    if selection.starter.is_some() {
        mandatory.extend(contract_names("starter"));
        mandatory.insert("spell_template");
    }
    check(
        mandatory.iter().all(|n| tables.contains_key(*n)),
        "bundle.missing_tables",
    )?;
    bounded_len(
        &bundle["schema"],
        2 * 1024 * 1024,
        "bundle.schema_byte_limit",
    )?;
    let mut required: BTreeSet<_> = tables.keys().map(String::as_str).collect();
    required.insert("db_version");
    let profiles = schema(&bundle["schema"], &required)?;
    check(
        bundle["consistency"] == "operator_quiescent_snapshot"
            || profiles.iter().all(|(table, p)| {
                ((table == "spell_facing" && !obj.contains_key("spell_numeric_capture"))
                    || (contract_names("presentation").contains(table.as_str())
                        && !obj.contains_key("quest_giver_presentation_capture"))
                    || (super::extensions::quest_mechanics::is_table(table)
                        && !obj.contains_key("quest_mechanics_capture")
                        && !(obj.contains_key("participants_capture")
                            && super::extensions::participants::is_table(table)))
                    || (super::extensions::participants::is_table(table)
                        && !super::extensions::quest_mechanics::is_table(table)
                        && !obj.contains_key("participants_capture")))
                    || p.engine == "InnoDB"
            }),
        "bundle.myisam_consistency",
    )?;
    check(
        bundle["schema_sha256"] == digest(&bundle["schema"]),
        "bundle.schema_hash",
    )?;
    let hashes = object(&bundle["table_sha256"], "bundle.table_hashes")?;
    check(hashes.keys().eq(tables.keys()), "bundle.table_hashes")?;
    for (table, rs) in tables {
        validate_rows(table, rs, &profiles[table], row_limit(table))?;
        check(hashes[table] == digest(rs), "bundle.table_hash_or_order")?;
    }
    if let Some(s) = selection.starter {
        validate_starter(bundle, s)?;
    }
    super::extensions::validate(bundle, &selection, &profiles)?;
    references(bundle, &selection)?;
    let notes = array(&bundle["db_version"], "schema.db_version_singleton")?;
    check(
        notes.len() == 1 && notes[0].is_object(),
        "schema.db_version_singleton",
    )?;
    let cols = &profiles["db_version"].columns;
    check(
        cols.len() == 2
            && object(&notes[0], "schema.version_fields")?
                .keys()
                .eq(cols.keys()),
        "schema.version_fields",
    )?;
    for (name, col) in cols {
        check(
            cell(&notes[0][name], col)? == notes[0][name],
            "bundle.db_version",
        )?;
        check(
            notes[0][name].is_null() || notes[0][name].as_str().unwrap().len() <= 1024,
            "schema.version_value",
        )?;
    }
    super::extensions::validate_late(bundle, &profiles)?;
    let status = &bundle["report"];
    fields(
        status,
        &["runnable", "capture_complete", "scope", "unsupported"],
        "bundle.report",
    )?;
    check(
        status["runnable"] == false && status["capture_complete"] == false,
        "bundle.not_runnable",
    )?;
    check(
        string(&status["scope"], "bundle.report_scope")?
            .chars()
            .count()
            <= 256,
        "bundle.report_scope",
    )?;
    let findings = array(&status["unsupported"], "bundle.report_limit")?;
    check(findings.len() <= 4096 * 8, "bundle.report_limit")?;
    for issue in findings {
        fields(
            issue,
            &["table", "key", "status", "reason"],
            "bundle.issue_fields",
        )?;
        check(
            issue["table"]
                .as_str()
                .is_some_and(|n| CONTRACT["columns"].get(n).is_some())
                && issue["status"] == "blocked",
            "bundle.issue",
        )?;
        check(
            string(&issue["key"], "bundle.issue_key")?.chars().count() <= 128,
            "bundle.issue_key",
        )?;
        check(
            string(&issue["reason"], "bundle.issue_reason")?
                .chars()
                .count()
                <= 128,
            "bundle.issue_reason",
        )?;
    }
    check(
        status["unsupported"] == full_report(bundle, &profiles, &selection)?,
        "bundle.missing_or_inconsistent_blocker",
    )?;
    bounded_len(bundle, MAX_BUNDLE, "bundle.byte_limit")?;
    Ok((selection, profiles))
}
fn validate_starter(bundle: &Value, (race, class, cap): (i128, i128, i128)) -> Result<()> {
    let mut expected = Map::new();
    for table in contract_names("starter") {
        let rs = rows(bundle, table);
        for row in rs {
            if row.get("race").is_some() {
                check(int(&row["race"]) == race, "starter.unselected_race")?;
            }
            if row.get("class").is_some() {
                check(int(&row["class"]) == class, "starter.unselected_class")?;
            }
        }
        expected.insert(
            table.into(),
            json!(rs
                .iter()
                .map(|r| keys(table)
                    .iter()
                    .map(|n| r[*n].clone())
                    .collect::<Vec<_>>())
                .collect::<Vec<_>>()),
        );
    }
    check(
        rows(bundle, "playercreateinfo").len() == 1,
        "starter.missing_creation",
    )?;
    for table in ["player_classlevelstats", "player_levelstats"] {
        check(
            ids(bundle, table, "level") == (1..=cap).collect(),
            &format!("starter.missing_or_unexpected_levels:{table}"),
        )?;
    }
    check(
        ids(bundle, "player_xp_for_level", "lvl") == (1..cap).collect(),
        "starter.missing_or_unexpected_xp_levels",
    )?;
    for row in rows(bundle, "player_classlevelstats") {
        integer(&row["basehp"], 1, 65535, "starter.basehp")?;
        integer(&row["basemana"], 0, 65535, "starter.basemana")?;
    }
    for row in rows(bundle, "player_levelstats") {
        for name in ["str", "agi", "sta", "inte", "spi"] {
            integer(
                &row[name],
                if name == "str" { 1 } else { 0 },
                255,
                &format!("starter.{name}"),
            )?;
        }
    }
    for row in rows(bundle, "player_xp_for_level") {
        integer(&row["xp_for_next_level"], 1, 0xffff_ffff, "starter.xp")?;
    }
    for row in rows(bundle, "playercreateinfo_item") {
        integer(&row["amount"], 1, 255, "starter.item_amount")?;
    }
    for row in rows(bundle, "playercreateinfo_action") {
        integer(&row["button"], 0, 119, "starter.action_button")?;
        integer(&row["action"], 0, 0xffffff, "starter.action_value")?;
        integer(&row["type"], 0, 255, "starter.action_type")?;
    }
    for row in rows(bundle, "playercreateinfo_skills") {
        integer(&row["step"], 0, 16, "starter.skill_step")?;
        let r = int(&row["raceMask"]);
        let c = int(&row["classMask"]);
        check(
            (r == 0 || r & (1 << (race - 1)) != 0) && (c == 0 || c & (1 << (class - 1)) != 0),
            "starter.unselected_skill_mask",
        )?;
    }
    starter_refs(bundle)?;
    check(
        bundle["starter_required_keys"] == Value::Object(expected),
        "bundle.starter_keys",
    )
}
pub(super) fn starter_refs(bundle: &Value) -> Result<(BTreeSet<i128>, BTreeSet<i128>)> {
    let mut items = ids(bundle, "playercreateinfo_item", "itemid");
    let mut spells = ids(bundle, "playercreateinfo_spell", "Spell");
    for row in rows(bundle, "playercreateinfo_action") {
        match int(&row["type"]) {
            128 => {
                items.insert(integer(&row["action"], 1, 0xffffff, "starter.action_item")?);
            }
            0 => {
                spells.insert(integer(
                    &row["action"],
                    1,
                    0xffffff,
                    "starter.action_spell",
                )?);
            }
            _ => {}
        }
    }
    Ok((items, spells))
}
fn references(bundle: &Value, s: &Selection) -> Result<()> {
    let required = object(&bundle["required_ids"], "bundle.references")?;
    let tables = bundle["tables"].as_object().unwrap();
    check(
        required.keys().all(|k| tables.contains_key(k)),
        "bundle.references",
    )?;
    let mut reference_tables: BTreeSet<_> = [
        "creature_template",
        "creature_model_info",
        "quest_template",
        "item_template",
    ]
    .into_iter()
    .collect();
    if tables.contains_key("spell_template") {
        reference_tables.insert("spell_template");
    }
    check(
        required.keys().map(String::as_str).collect::<BTreeSet<_>>() == reference_tables,
        "bundle.required_reference_tables",
    )?;
    for (table, values) in required {
        let a = array(values, "bundle.reference_ids")?;
        let mut selected = BTreeSet::new();
        for v in a {
            check(
                selected.insert(integer(v, 1, 0xffff_ffff, "bundle.reference_id")?),
                "bundle.reference_ids",
            )?;
        }
        check(
            selected == ids(bundle, table, keys(table)[0]),
            &format!("reference.missing_or_unexpected:{table}"),
        )?;
    }
    let creatures = ids(bundle, "creature_template", "Entry");
    let quests = ids(bundle, "quest_template", "entry");
    let items = ids(bundle, "item_template", "entry");
    let spells = ids(bundle, "spell_template", "Id");
    let models = ids(bundle, "creature_model_info", "modelid");
    let (si, ss) = starter_refs(bundle)?;
    check(si.is_subset(&items), "bundle.starter_item_reference")?;
    check(ss.is_subset(&spells), "bundle.starter_spell_reference")?;
    check(
        s.quests.is_subset(&quests) && s.spells.is_subset(&spells),
        "bundle.selected_ids_missing",
    )?;
    for row in rows(bundle, "creature") {
        let id = int(&row["id"]);
        check(id == 0 || creatures.contains(&id), "bundle.spawn_reference")?;
        check(int(&row["map"]) == s.map, "bundle.map")?;
        for (idx, name) in ["position_x", "position_y", "position_z"]
            .iter()
            .enumerate()
        {
            check(
                inside(&row[*name], s.min[idx], s.max[idx]),
                "bundle.outside_region",
            )?;
        }
    }
    for table in ["creature_questrelation", "creature_involvedrelation"] {
        check(
            rows(bundle, table)
                .iter()
                .all(|r| creatures.contains(&int(&r["id"])) && quests.contains(&int(&r["quest"]))),
            "bundle.quest_relation_reference",
        )?;
    }
    for row in rows(bundle, "quest_template") {
        check(
            (1..=4).all(|i| {
                let id = int(&row[format!("ReqCreatureOrGOId{i}")]);
                id <= 0 || creatures.contains(&id)
            }),
            "bundle.objective_reference",
        )?;
        for name in columns("quest_template") {
            if name == "SrcItemId"
                || ["ReqItemId", "ReqSourceId", "RewItemId", "RewChoiceItemId"]
                    .iter()
                    .any(|p| name.starts_with(p))
            {
                let id = int(&row[name]);
                if id != 0 {
                    integer(&row[name], 1, 0xffff_ffff, "reference.item_id")?;
                    check(items.contains(&id), "bundle.quest_item_reference")?;
                }
            }
        }
        check(
            ["SrcSpell", "RewSpell", "RewSpellCast"].iter().all(|n| {
                let id = int(&row[*n]);
                id == 0 || spells.contains(&id)
            }),
            "bundle.quest_spell_reference",
        )?;
    }
    for row in rows(bundle, "creature_template") {
        let low = integer(&row["MinLevel"], 1, 65, "creature.min_level")?;
        integer(&row["MaxLevel"], low, 65, "creature.max_level")?;
        check(
            (1..=4).all(|i| {
                let id = int(&row[format!("DisplayId{i}")]);
                id == 0 || models.contains(&id)
            }),
            "bundle.model_reference",
        )?;
        check(
            ["KillCredit1", "KillCredit2"].iter().all(|n| {
                let id = int(&row[*n]);
                id == 0 || creatures.contains(&id)
            }),
            "bundle.kill_credit_reference",
        )?;
    }
    for row in rows(bundle, "item_template") {
        if s.starter.is_some() {
            let id = int(&row["requiredspell"]);
            check(
                id == 0 || spells.contains(&id),
                "bundle.item_required_spell_reference",
            )?;
        }
        check(
            (1..=5).all(|i| {
                let id = int(&row[format!("spellid_{i}")]);
                id <= 0 || spells.contains(&id)
            }),
            "bundle.item_spell_reference",
        )?;
    }
    for row in rows(bundle, "spell_template") {
        check(
            (1..=3).all(|i| {
                let id = int(&row[format!("EffectTriggerSpell{i}")]);
                id == 0 || spells.contains(&id)
            }),
            "bundle.trigger_reference",
        )?;
    }
    Ok(())
}

pub(super) fn issue(table: &str, key: impl ToString, reason: &str) -> Value {
    json!({"table":table,"key":key.to_string(),"status":"blocked","reason":reason})
}
pub(super) fn full_report(bundle: &Value, profiles: &Profiles, s: &Selection) -> Result<Value> {
    let mut out = Vec::new();
    let tables = bundle["tables"].as_object().unwrap();
    let body = bundle.get("loot_capture").is_some();
    for (table, rs) in tables {
        if !profiles[table].extra.is_empty() {
            out.push(issue(table, "schema", "schema.unreviewed_columns"));
        }
        for row in rs.as_array().unwrap() {
            let key = keys(table)
                .iter()
                .map(|n| {
                    if let Some(s) = row[*n].as_str() {
                        s.into()
                    } else {
                        row[*n].to_string()
                    }
                })
                .collect::<Vec<String>>()
                .join(":");
            let mut add = |reason: &str| out.push(issue(table, &key, reason));
            let nonzero = |n: &str| int(&row[n]) != 0;
            match table.as_str() {
                "creature" => {
                    if !nonzero("id") {
                        add(if tables.contains_key("creature_spawn_entry") {
                            "spawn.entry_substitution_unresolved"
                        } else {
                            "spawn.entry_substitution_not_captured"
                        });
                    }
                    if !contract_names("spawn")
                        .iter()
                        .all(|n| tables.contains_key(*n))
                    {
                        add("spawn.overrides_pools_events_not_captured");
                    }
                    if nonzero("MovementType") || row["spawndist"] != json!({"decimal":"0"}) {
                        add("movement.native_rules_required");
                    }
                }
                "creature_template" => {
                    add("creature.native_stats_combat_faction_required");
                    if row["MinLevel"] != row["MaxLevel"] {
                        add("creature.variable_level");
                    }
                    if row["AIName"] != ""
                        || row["ScriptName"] != ""
                        || nonzero("SpellList")
                        || nonzero("CharmedSpellList")
                    {
                        add("creature.ai_or_spell_dependency");
                    }
                    if !body
                        && ["LootId", "SkinningLootId", "PickpocketLootId"]
                            .iter()
                            .any(|n| nonzero(n))
                    {
                        add("creature.loot_dependency_not_captured");
                    }
                }
                "quest_template" => {
                    add("quest.native_eligibility_credit_reward_required");
                    if nonzero("RewMoneyMaxLevel") {
                        add("quest.dynamic_xp");
                    }
                    if ["RequiredCondition", "StartScript", "CompleteScript"]
                        .iter()
                        .any(|n| nonzero(n))
                    {
                        add("quest.condition_or_script");
                    }
                    if [
                        "PrevQuestId",
                        "NextQuestId",
                        "NextQuestInChain",
                        "ExclusiveGroup",
                        "BreadcrumbForQuestId",
                    ]
                    .iter()
                    .any(|n| nonzero(n))
                    {
                        add("quest.chain");
                    }
                    if (1..=4).any(|i| {
                        int(&row[format!("ReqCreatureOrGOId{i}")]) < 0
                            || nonzero(&format!("ReqSpellCast{i}"))
                            || nonzero(&format!("ReqItemId{i}"))
                    }) {
                        add("quest.nonkill_objective");
                    }
                }
                "spell_template" => {
                    add("spell.native_effect_target_scaling_rules_required");
                    if (1..=3).filter(|i| nonzero(&format!("Effect{i}"))).count() != 1 {
                        add("spell.multiple_or_missing_effects");
                    }
                    if (1..=3).any(|i| {
                        nonzero(&format!("EffectApplyAuraName{i}"))
                            || nonzero(&format!("EffectTriggerSpell{i}"))
                    }) {
                        add("spell.aura_or_trigger_dependency");
                    }
                }
                "item_template" => add("item.native_inventory_equipment_rules_required"),
                "spell_scripts" => add("spell.registry_row_requires_named_handler_semantics"),
                "creature_ai_scripts" => add("creature.event_ai_matching_row_requires_execution"),
                _ => {}
            }
        }
    }
    if let Some((race, class, _)) = s.starter {
        let key = format!("{race}:{class}");
        for reason in [
            "player.native_creation_progression_rules_required",
            "player.chr_races_classes_map_dbc_not_captured",
            "player.faction_reputation_dbc_not_captured",
            "player.char_start_outfit_dbc_items_not_captured",
            "player.start_configuration_not_captured",
        ] {
            out.push(issue("playercreateinfo", &key, reason));
        }
        for (table, reason) in [
            (
                "player_classlevelstats",
                "player.equipment_aura_stat_modifiers_required",
            ),
            (
                "playercreateinfo_skills",
                "player.skill_line_race_class_tiers_ability_dbc_not_captured",
            ),
            (
                "playercreateinfo_spell",
                "player.spell_learning_passive_dependencies_not_captured",
            ),
        ] {
            out.push(issue(table, &key, reason));
        }
        let creation = &rows(bundle, "playercreateinfo")[0];
        if int(&creation["map"]) != s.map
            || ["position_x", "position_y", "position_z"]
                .iter()
                .enumerate()
                .any(|(i, n)| !inside(&creation[*n], s.min[i], s.max[i]))
        {
            out.push(issue(
                "playercreateinfo",
                &key,
                "player.start_outside_selection",
            ));
        }
        let learned = ids(bundle, "playercreateinfo_spell", "Spell");
        for row in rows(bundle, "playercreateinfo_action") {
            let reason = if ![0, 128].contains(&int(&row["type"])) {
                Some("player.action_type_not_supported")
            } else if int(&row["type"]) == 0 && !learned.contains(&int(&row["action"])) {
                Some("player.action_spell_learning_not_resolved")
            } else {
                None
            };
            if let Some(reason) = reason {
                out.push(issue(
                    "playercreateinfo_action",
                    format!("{key}:{}", row["button"]),
                    reason,
                ));
            }
        }
        for row in rows(bundle, "item_template") {
            if int(&row["RequiredSkill"]) != 0 || int(&row["RequiredReputationFaction"]) != 0 {
                out.push(issue(
                    "item_template",
                    &row["entry"],
                    "item.skill_faction_dbc_not_captured",
                ));
            }
            if int(&row["startquest"]) != 0 {
                out.push(issue(
                    "item_template",
                    &row["entry"],
                    "item.startquest_dependency_not_captured",
                ));
            }
        }
    }
    for row in rows(bundle, "creature_template") {
        let low = integer(&row["MinLevel"], 1, 65, "creature.min_level")?;
        let high = integer(&row["MaxLevel"], low, 65, "creature.max_level")?;
        let class = integer(&row["UnitClass"], 0, 8, "creature.unit_class")?;
        for level in low..=high {
            if !rows(bundle, "creature_template_classlevelstats")
                .iter()
                .any(|r| int(&r["Class"]) == class && int(&r["Level"]) == level)
            {
                out.push(issue(
                    "creature_template_classlevelstats",
                    format!("{class}:{level}"),
                    "creature.missing_classlevelstats",
                ));
            }
        }
    }
    // Python computes a set of missing class-level pairs, not one finding per creature.
    let mut stat_seen = BTreeSet::new();
    out.retain(|v| {
        v["reason"] != "creature.missing_classlevelstats" || stat_seen.insert(v.to_string())
    });
    let spawned = ids(bundle, "creature", "id");
    for quest in rows(bundle, "quest_template") {
        if (1..=4).any(|i| {
            let id = int(&quest[format!("ReqCreatureOrGOId{i}")]);
            id > 0 && !spawned.contains(&id)
        }) {
            out.push(issue(
                "quest_template",
                &quest["entry"],
                "quest.objective_outside_selection",
            ));
        }
        for table in ["creature_questrelation", "creature_involvedrelation"] {
            if !rows(bundle, table)
                .iter()
                .any(|r| r["quest"] == quest["entry"] && spawned.contains(&int(&r["id"])))
            {
                out.push(issue(
                    table,
                    &quest["entry"],
                    "quest.relation_missing_or_outside_selection",
                ));
            }
        }
    }
    out.extend(super::extensions::blockers(bundle)?);
    out.sort_by(|a, b| {
        a["table"]
            .as_str()
            .cmp(&b["table"].as_str())
            .then_with(|| a["key"].as_str().cmp(&b["key"].as_str()))
            .then_with(|| a["reason"].as_str().cmp(&b["reason"].as_str()))
    });
    Ok(json!(out))
}
