// SPDX-License-Identifier: GPL-3.0-or-later
// native_starter/dbc.py: packed records, complete v4 extraction and exact text bounds.
use super::{check, fail, json::*, Result, COMPILER_CONTRACT};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path};
pub(super) const MAX_FILE: usize = 64 * 1024 * 1024;
pub(super) type Tables = BTreeMap<String, Table>;
pub(super) struct Table {
    pub name: String,
    pub data: Vec<u8>,
    pub count: usize,
    pub fields: usize,
    pub size: usize,
    pub by_id: BTreeMap<u32, usize>,
    pub text: BTreeMap<u32, String>,
    pub sha256: String,
}
#[derive(Clone, Copy)]
pub(super) struct Row<'a> {
    pub table: &'a Table,
    pub index: usize,
}
impl Table {
    pub fn parse(name: &str, data: Vec<u8>) -> Result<Self> {
        let layout = COMPILER_CONTRACT["dbc_layouts"]
            .get(name)
            .ok_or_else(|| fail("dbc.unknown_table"))?;
        check(
            (20..=MAX_FILE).contains(&data.len()),
            "dbc.byte_limit_or_truncated_header",
        )?;
        check(&data[..4] == b"WDBC", "dbc.magic")?;
        let word = |i| u32::from_le_bytes(data[i..i + 4].try_into().unwrap()) as usize;
        let count = word(4);
        let fields = word(8);
        let size = word(12);
        let strings = word(16);
        check(
            fields == layout["fields"].as_u64().unwrap() as usize
                && size == layout["size"].as_u64().unwrap() as usize,
            "dbc.layout",
        )?;
        check(count <= 100_000, "dbc.record_limit")?;
        let end = 20 + count * size;
        check(
            end.checked_add(strings) == Some(data.len()),
            "dbc.exact_length",
        )?;
        check(
            strings > 0 && data[end] == 0 && data.last() == Some(&0),
            "dbc.string_block",
        )?;
        let mut by_id = BTreeMap::new();
        let mut text = BTreeMap::new();
        let mut decoded_bytes = 0;
        for index in 0..count {
            let record = &data[20 + index * size..20 + (index + 1) * size];
            let key = if name == "CharBaseInfo" {
                ((record[0] as u32) << 8) | record[1] as u32
            } else {
                u32::from_le_bytes(record[..4].try_into().unwrap())
            };
            check(by_id.insert(key, index).is_none(), "dbc.duplicate_key")?;
            for c in layout["strings"].as_array().unwrap() {
                let c = c.as_u64().unwrap() as usize;
                let offset = u32::from_le_bytes(record[c * 4..c * 4 + 4].try_into().unwrap());
                if text.contains_key(&offset) {
                    continue;
                }
                let offset_usize = offset as usize;
                check(offset_usize < strings, "dbc.string_offset")?;
                let max = (offset_usize + 16385).min(strings);
                let bytes = &data[end + offset_usize..end + max];
                let len = bytes
                    .iter()
                    .position(|b| *b == 0)
                    .ok_or_else(|| fail("dbc.string_termination_or_limit"))?;
                decoded_bytes += len;
                check(
                    decoded_bytes <= 16 * 1024 * 1024 && text.len() < 262144,
                    "dbc.decoded_text_limit",
                )?;
                let decoded =
                    std::str::from_utf8(&bytes[..len]).map_err(|_| fail("dbc.string_encoding"))?;
                text.insert(offset, decoded.into());
            }
        }
        let sha256 = hash_bytes(&data);
        Ok(Self {
            name: name.into(),
            data,
            count,
            fields,
            size,
            by_id,
            text,
            sha256,
        })
    }
    pub fn row(&self, index: usize) -> Row<'_> {
        Row { table: self, index }
    }
    pub fn get(&self, id: u32) -> Option<Row<'_>> {
        self.by_id.get(&id).map(|i| self.row(*i))
    }
    pub fn manifest(&self) -> Value {
        json!({"file":format!("{}.dbc",self.name),"sha256":self.sha256,"bytes":self.data.len(),"records":self.count,"fields":self.fields,"record_bytes":self.size})
    }
}
impl<'a> Row<'a> {
    pub fn word(&self, i: usize) -> u32 {
        let start = 20 + self.index * self.table.size;
        if self.table.name == "CharBaseInfo" {
            self.table.data[start + i] as u32
        } else {
            let start = start + i * 4;
            u32::from_le_bytes(self.table.data[start..start + 4].try_into().unwrap())
        }
    }
    pub fn key(&self) -> Vec<u32> {
        if self.table.name == "CharBaseInfo" {
            vec![self.word(0), self.word(1)]
        } else {
            vec![self.word(0)]
        }
    }
    pub fn len(&self) -> usize {
        if self.table.name == "CharBaseInfo" {
            2
        } else {
            self.table.size / 4
        }
    }
    pub fn text(&self, column: usize) -> &'a str {
        &self.table.text[&self.word(column)]
    }
    pub fn raw(&self) -> Value {
        let strings = COMPILER_CONTRACT["dbc_layouts"][&self.table.name]["strings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                let c = v.as_u64().unwrap() as usize;
                (c.to_string(), self.text(c))
            })
            .collect::<BTreeMap<_, _>>();
        let start = 20 + self.index * self.table.size;
        json!({"key":self.key(),"words":(0..self.len()).map(|i|self.word(i)).collect::<Vec<_>>(),"strings":strings,"record_sha256":hash_bytes(&self.table.data[start..start+self.table.size])})
    }
}
pub(super) fn load(directory: &Path) -> Result<(Tables, Value)> {
    let mut tables = Tables::new();
    let mut aggregate = 0;
    for name in COMPILER_CONTRACT["dbc_layouts"].as_object().unwrap().keys() {
        let data = read_regular(&directory.join(format!("{name}.dbc")), MAX_FILE).map_err(|e| {
            if e.id == "json.byte_limit" {
                fail("dbc.input_byte_limit")
            } else {
                e
            }
        })?;
        aggregate += data.len();
        check(aggregate <= 192 * 1024 * 1024, "dbc.aggregate_byte_limit")?;
        tables.insert(name.clone(), Table::parse(name, data)?);
    }
    let bytes = read_regular(&directory.join("extraction.json"), 65536)?;
    let manifest = decode(&bytes, 65536)?;
    fields(
        &manifest,
        &[
            "format",
            "chain_policy",
            "client_build_claim",
            "client_build_authenticity_verified",
            "files",
        ],
        "extraction.manifest_fields",
    )?;
    check(
        manifest["format"] != "benilla-native-dbc-extraction-v1"
            && manifest["format"] != "benilla-native-dbc-extraction-v2",
        "extraction.legacy_manifest_reextract_v3_required",
    )?;
    check(
        (manifest["format"] == "benilla-native-dbc-extraction-v3"
            || manifest["format"] == "benilla-native-dbc-extraction-v4")
            && manifest["chain_policy"] == "benilla-formats-vanilla-chain"
            && integer(
                &manifest["client_build_claim"],
                5875,
                5875,
                "extraction.manifest_profile",
            )
            .is_ok()
            && manifest["client_build_authenticity_verified"] == false,
        "extraction.manifest_profile",
    )?;
    // The production target requires Emotes. v3 remains valid only for its older diagnostic CLI.
    check(
        manifest["format"] == "benilla-native-dbc-extraction-v4",
        "extraction.complete_valid_dbc_set_required",
    )?;
    verify_manifest(&manifest, &tables)?;
    let binding = json!({"file":"extraction.json","sha256":hash_bytes(&bytes),"bytes":bytes.len(),"integrity_verified":true,"authenticity_verified":false,"metadata":manifest});
    Ok((tables, binding))
}
pub(super) fn verify_manifest(v: &Value, tables: &Tables) -> Result<()> {
    let files = array(&v["files"], "extraction.manifest_file_set")?;
    check(files.len() == tables.len(), "extraction.manifest_file_set")?;
    let mut seen = std::collections::BTreeSet::new();
    for row in files {
        fields(
            row,
            &[
                "file",
                "winning_archive",
                "sha256",
                "bytes",
                "records",
                "fields",
                "record_bytes",
            ],
            "extraction.manifest_file_fields",
        )?;
        let name = string(&row["file"], "extraction.manifest_file_set")?;
        let table = name
            .strip_suffix(".dbc")
            .and_then(|name| tables.get(name))
            .ok_or_else(|| fail("extraction.manifest_file_set"))?;
        check(seen.insert(name), "extraction.manifest_file_set")?;
        let label = string(&row["winning_archive"], "extraction.archive_label")?;
        check(
            !label.is_empty()
                && label.chars().count() <= 255
                && !matches!(label, "." | "..")
                && !label.contains(['/', '\\', '\0']),
            "extraction.archive_label",
        )?;
        let mut observed = row.as_object().unwrap().clone();
        observed.remove("winning_archive");
        check(
            canonical(&Value::Object(observed)) == canonical(&table.manifest()),
            "extraction.dbc_manifest_mismatch",
        )?;
    }
    Ok(())
}
/// Rust's correctly rounded decimal-to-f32 parser implements IEEE round-to-nearest/ties-to-even.
/// Parsing directly as f32 avoids the double rounding introduced by an intermediate f64.
pub(super) fn binary32(decimal: &str) -> Result<u32> {
    check(decimal.chars().count() <= 128, "numeric.decimal_domain")?;
    let normalized = decimal.trim();
    let exponent = normalized
        .split_once(['e', 'E'])
        .map_or(Ok(0i32), |(_, e)| {
            e.parse::<i32>().map_err(|_| fail("numeric.decimal_domain"))
        })?;
    let fraction = normalized
        .split_once(['e', 'E'])
        .map_or(normalized, |(m, _)| m)
        .split_once('.')
        .map_or(0, |(_, f)| f.len() as i32);
    check(
        (-200..=200).contains(&(exponent - fraction)),
        "numeric.decimal_domain",
    )?;
    check(
        !["nan", "snan", "inf", "infinity"].iter().any(|name| {
            normalized
                .trim_start_matches(['-', '+'])
                .eq_ignore_ascii_case(name)
        }),
        "numeric.decimal_domain",
    )?;
    let value = normalized
        .parse::<f32>()
        .map_err(|_| fail("numeric.decimal_domain"))?;
    check(value.is_finite(), "numeric.binary32_overflow")?;
    if value == 0.0 && !normalized.starts_with('-') {
        return Ok(0);
    }
    if value == 0.0
        && normalized
            .trim_start_matches(['-', '+'])
            .split(['e', 'E'])
            .next()
            .unwrap()
            .chars()
            .all(|c| c == '0' || c == '.')
    {
        return Ok(0);
    }
    Ok(value.to_bits())
}
pub(super) fn finite(word: u32, code: &str) -> Result<u32> {
    check(word & 0x7f800000 != 0x7f800000, code)?;
    Ok(word)
}
