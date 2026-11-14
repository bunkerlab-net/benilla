// SPDX-License-Identifier: GPL-3.0-or-later
//! Independently checked first-entry inputs, not a Profile or a runtime receipt.
//! The collision epoch is a declared source binding, not an installation lease.
use super::{dbc, fail, json, ProductionDiagnostic, Result};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest as _, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::Arc,
};
mod binary;
mod collision;
mod grid;

const FORMAT: &str = "benilla-native-starter-location-input-v1";
const POLICY: &str = "cmangos-8ec338a1-first-entry-grid-and-group-exclusion-v1";
const PIN: &str = "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f";
const MAX_FILE: usize = 32 * 1024 * 1024;
const MAX_TOTAL: usize = 256 * 1024 * 1024;
const MAX_FILES: usize = 4096;
const MAX_WORDS: usize = 1_048_576;

#[derive(Debug)]
pub(super) struct CheckedLocation {
    pub(super) report: Value,
    pub(super) area_id: u32,
    pub(super) zone_id: u32,
    pub(super) raw_files: BTreeMap<String, Arc<Vec<u8>>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    format: String,
    policy: String,
    source_sha256: String,
    selection_sha256: String,
    position_bits: [u32; 3],
    capture: String,
    capture_sha256: String,
    collision: String,
    collision_sha256: String,
    catalog: String,
    source_epoch: String,
}
fn error(reason: &str) -> ProductionDiagnostic {
    fail(&format!("source_location.{reason}"))
}
fn require(value: bool, reason: &str) -> Result<()> {
    if value {
        Ok(())
    } else {
        Err(error(reason))
    }
}
fn parse<T: serde::de::DeserializeOwned>(value: Value, reason: &str) -> Result<T> {
    serde_json::from_value(value).map_err(|_| error(reason))
}
fn hash_text(value: &str) -> bool {
    value.len() == 64
        && value.bytes().any(|b| b != b'0')
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn digest_bytes(value: &str) -> Result<[u8; 32]> {
    require(hash_text(value), "digest")?;
    let mut out = [0; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[i * 2..i * 2 + 2], 16).map_err(|_| error("digest"))?;
    }
    Ok(out)
}
fn field(hash: &mut Sha256, value: &[u8]) {
    hash.update((value.len() as u64).to_le_bytes());
    hash.update(value);
}
fn component(value: &str) -> bool {
    !value.is_empty() && !matches!(value, "." | "..") && !value.contains(['/', '\\'])
}
struct Files<'a> {
    root: &'a Path,
    raw: BTreeMap<String, Arc<Vec<u8>>>,
    work: usize,
}
impl<'a> Files<'a> {
    fn new(root: &'a Path) -> Result<Self> {
        require(root.is_dir() && !root.is_symlink(), "input_directory")?;
        Ok(Self {
            root,
            raw: BTreeMap::new(),
            work: 0,
        })
    }
    fn read(&mut self, name: &str, maximum: usize) -> Result<Arc<Vec<u8>>> {
        require(
            !name.is_empty()
                && name.len() <= 512
                && !name.contains('\\')
                && name.split('/').all(component),
            "file_name",
        )?;
        if let Some(data) = self.raw.get(name) {
            require(data.len() <= maximum, "file_limit")?;
            return Ok(Arc::clone(data));
        }
        require(self.raw.len() < MAX_FILES, "file_count")?;
        let mut path = self.root.to_path_buf();
        for part in name.split('/') {
            path.push(part);
            require(!path.is_symlink(), "symlink")?;
        }
        let data = Arc::new(json::read_regular(&path, maximum)?);
        self.work += data.len();
        require(self.work <= MAX_TOTAL, "aggregate_limit")?;
        self.raw.insert(name.to_owned(), Arc::clone(&data));
        Ok(data)
    }
    fn json(&mut self, name: &str) -> Result<Value> {
        json::decode(&self.read(name, 2 * 1024 * 1024)?, 2 * 1024 * 1024)
    }
}

pub(super) fn load(path: &Path, source: &Value, dbcs: &dbc::Tables) -> Result<CheckedLocation> {
    let root = path.parent().ok_or_else(|| error("input_directory"))?;
    let mut files = Files::new(root)?;
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| error("file_name"))?;
    let raw = files.json(name)?;
    let input_sha = json::digest(&raw);
    let header: Header = parse(raw, "input_fields")?;
    require(
        header.format == FORMAT && header.policy == POLICY,
        "input_policy",
    )?;
    let source_sha = json::digest(source);
    let selection = &source["selection"];
    let selection_sha = json::digest(selection);
    require(
        header.source_sha256 == source_sha && header.selection_sha256 == selection_sha,
        "source_binding",
    )?;
    let start = &source["tables"]["playercreateinfo"][0];
    let mut position = [0.0f32; 3];
    for (i, key) in ["position_x", "position_y", "position_z"]
        .into_iter()
        .enumerate()
    {
        let text = start[key]["decimal"]
            .as_str()
            .ok_or_else(|| error("position_binding"))?;
        position[i] = f32::from_bits(dbc::binary32(text)?);
    }
    require(
        header.position_bits == position.map(f32::to_bits) && start["map"] == selection["map_id"],
        "position_binding",
    )?;
    require(
        (0..3).all(|i| {
            let milli = f64::from(position[i]) * 1000.0;
            milli >= json::int(&selection["min_milliunits"][i]) as f64
                && milli <= json::int(&selection["max_milliunits"][i]) as f64
        }),
        "position_outside_selection",
    )?;
    for (prefix, name, expected) in [
        (&header.capture, "location.json", &header.capture_sha256),
        (&header.collision, "index.json", &header.collision_sha256),
    ] {
        require(component(prefix), "input_path")?;
        require(
            json::hash_bytes(&files.read(&format!("{prefix}/{name}"), 2 * 1024 * 1024)?)
                == *expected,
            "input_manifest_binding",
        )?;
    }
    let context = grid::prepare(&mut files, &header.capture, position, dbcs)?;
    require(
        context.map == json::int(&start["map"]) as u32
            && context.catalog == digest_bytes(&header.catalog)?,
        "catalog_binding",
    )?;
    let exclusion = collision::exclude(&mut files, &header.collision, position, context.map)?;
    let mut combined = Sha256::new();
    combined.update(b"benilla-genuine-resident-source-location-v1");
    combined.update(context.source_epoch.as_bytes());
    combined.update(digest_bytes(
        exclusion["collision_epoch"].as_str().unwrap(),
    )?);
    let combined = format!("{:x}", combined.finalize());
    require(combined == header.source_epoch, "source_epoch_binding")?;
    Ok(CheckedLocation {
        area_id: context.area_id,
        zone_id: context.zone_id,
        raw_files: files.raw,
        report: json!({"format":FORMAT,"policy":POLICY,"status":"checked_first_entry_context",
            "source_sha256":source_sha,"selection_sha256":selection_sha,"input_sha256":input_sha,
            "source_epoch":combined,"catalog":header.catalog,"map":context.map,"position_bits":header.position_bits,
            "area_flag":context.area_flag,"area_id":context.area_id,"zone_id":context.zone_id,
            "tile":context.tile,"cell":context.cell,"wmo_exclusion":exclusion,"runtime_export":null}),
    })
}

#[cfg(test)]
mod retained_tests {
    use super::*;

    #[test]
    fn retained_file_is_read_once_and_keeps_each_read_bound() {
        let root =
            std::env::temp_dir().join(format!("benilla-location-retained-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("member"), b"checked source").unwrap();
        let mut files = Files::new(&root).unwrap();
        let first = files.read("member", 32).unwrap();
        std::fs::write(root.join("member"), b"replacement").unwrap();
        let second = files.read("member", 32).unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(second.as_slice(), b"checked source");
        assert_eq!(files.work, first.len());
        assert_eq!(
            files.read("member", 1).unwrap_err().id,
            "source_location.file_limit"
        );
        assert_eq!(
            files.read("../member", 32).unwrap_err().id,
            "source_location.file_name"
        );
        std::fs::remove_dir_all(root).unwrap();
        assert_eq!(first.as_slice(), b"checked source");
    }
}
