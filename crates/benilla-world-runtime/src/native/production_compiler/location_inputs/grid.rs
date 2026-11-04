// SPDX-License-Identifier: GPL-3.0-or-later
//! Raw captured MCIN/MCNK area IDs and the existing complete DBC owner.
use super::*;
use crate::native::SourceLocationCatalog;
use binary::{bytes, word};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Member {
    name: String,
    winning_archive: String,
    bytes: usize,
    sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Absent {
    name: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Tile {
    x: u8,
    y: u8,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Grid {
    tile: Tile,
    area_ids: Vec<u32>,
    area_flags: Vec<u16>,
    header_flags: u16,
    grid_area: u16,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Capture {
    format: String,
    source_pin: String,
    source_policy: String,
    policy: String,
    map: u32,
    source_epoch: String,
    terrain_epoch: String,
    catalog_epoch: String,
    terrain_members: Vec<Member>,
    terrain_absent_members: Vec<Absent>,
    catalog_members: Vec<Member>,
    grids: Vec<Grid>,
    #[serde(rename = "areas")]
    _areas: serde::de::IgnoredAny,
    wmo_precedence_required: bool,
    live_install_lease: bool,
    gameplay_admitted: bool,
}
pub(super) struct Context {
    pub(super) map: u32,
    pub(super) source_epoch: String,
    pub(super) catalog: crate::Digest,
    pub(super) area_flag: u16,
    pub(super) area_id: u32,
    pub(super) zone_id: u32,
    pub(super) tile: [u8; 2],
    pub(super) cell: [usize; 2],
}
fn begin_epoch(capture: &Capture, members: usize) -> Sha256 {
    let mut hash = Sha256::new();
    hash.update(b"benilla-native-terrain-source-v1\0");
    field(&mut hash, capture.source_policy.as_bytes());
    field(&mut hash, capture.source_pin.as_bytes());
    hash.update((members as u64).to_le_bytes());
    hash
}
fn member(hash: &mut Sha256, meta: &Member, data: &[u8]) -> Result<()> {
    require(
        meta.bytes == data.len() && json::hash_bytes(data) == meta.sha256,
        "member_binding",
    )?;
    field(hash, meta.name.as_bytes());
    field(hash, meta.winning_archive.as_bytes());
    field(hash, data);
    Ok(())
}

pub(super) fn prepare(
    files: &mut Files<'_>,
    prefix: &str,
    position: [f32; 3],
    dbcs: &dbc::Tables,
) -> Result<Context> {
    let capture: Capture = parse(
        files.json(&format!("{prefix}/location.json"))?,
        "capture_fields",
    )?;
    require(
        capture.format == "benilla-source-location-inputs-v1"
            && capture.source_pin == PIN
            && capture.source_policy == "benilla-chain-and-cmangos-ad-fixed-five-exact-winner-v3"
            && capture.policy == "cmangos-8ec338a1-captured-grid-area-inputs-v1"
            && capture.wmo_precedence_required
            && !capture.live_install_lease
            && !capture.gameplay_admitted,
        "capture_policy",
    )?;
    require(
        capture.terrain_members.len() <= MAX_FILES
            && capture.terrain_absent_members.len() <= MAX_FILES,
        "member_limit",
    )?;
    let mut terrain_hash = begin_epoch(&capture, capture.terrain_members.len());
    let mut source = BTreeMap::new();
    for (i, meta) in capture.terrain_members.iter().enumerate() {
        let data = files.read(&format!("{prefix}/terrain-source-{i:04}.bin"), MAX_FILE)?;
        member(&mut terrain_hash, meta, &data)?;
        require(
            source.insert(meta.name.as_str(), data).is_none(),
            "duplicate_member",
        )?;
    }
    if !capture.terrain_absent_members.is_empty() {
        field(&mut terrain_hash, b"dual-resolver-absent-members-v1");
        terrain_hash.update((capture.terrain_absent_members.len() as u64).to_le_bytes());
        for absent in &capture.terrain_absent_members {
            field(&mut terrain_hash, b"absent");
            field(&mut terrain_hash, absent.name.as_bytes());
        }
    }
    require(
        format!("{:x}", terrain_hash.finalize()) == capture.terrain_epoch,
        "terrain_epoch",
    )?;
    let mut catalogs = Vec::with_capacity(3);
    for name in ["AreaTable.dbc", "WMOAreaTable.dbc", "Map.dbc"] {
        catalogs.push(files.read(&format!("{prefix}/{name}"), MAX_FILE)?);
    }
    for (name, data) in [("AreaTable.dbc", &catalogs[0]), ("Map.dbc", &catalogs[2])] {
        require(
            source.get(format!("DBFilesClient\\{name}").as_str()) == Some(data),
            "catalog_source_binding",
        )?;
    }
    require(
        capture.catalog_members.len() == 1
            && capture.catalog_members[0].name == "DBFilesClient\\WMOAreaTable.dbc",
        "catalog_scope",
    )?;
    let mut catalog_hash = begin_epoch(&capture, 1);
    member(&mut catalog_hash, &capture.catalog_members[0], &catalogs[1])?;
    require(
        format!("{:x}", catalog_hash.finalize()) == capture.catalog_epoch,
        "catalog_epoch",
    )?;
    let mut epoch = Sha256::new();
    epoch.update(capture.policy.as_bytes());
    epoch.update(capture.terrain_epoch.as_bytes());
    epoch.update(capture.catalog_epoch.as_bytes());
    require(
        format!("{:x}", epoch.finalize()) == capture.source_epoch,
        "capture_epoch",
    )?;
    let catalog = SourceLocationCatalog::decode(&catalogs[0], &catalogs[1], &catalogs[2])
        .map_err(|_| error("dbc_layout"))?;
    let maps = dbcs.get("Map").ok_or_else(|| error("catalog_binding"))?;
    require(maps.data == catalogs[2], "catalog_binding")?;
    let map = maps.get(capture.map).ok_or_else(|| error("map_required"))?;
    let directory = map.text(1);
    require(
        !capture.grids.is_empty() && capture.grids.len() <= 4096,
        "grid_count",
    )?;
    let mut seen = BTreeSet::new();
    for grid in &capture.grids {
        let (x, y) = (grid.tile.x, grid.tile.y);
        require(x < 64 && y < 64 && seen.insert((x, y)), "grid_tile")?;
        require(
            grid.area_ids.len() == 256 && grid.area_flags.len() == 256,
            "capture_fields",
        )?;
        let adt = source
            .get(format!("World\\Maps\\{directory}\\{directory}_{y}_{x}.adt").as_str())
            .ok_or_else(|| error("raw_adt_required"))?;
        require(bytes(adt, 84, 4)? == b"NICM", "raw_adt_required")?;
        let mut flags = [0u16; 256];
        for (slot, flag) in flags.iter_mut().enumerate() {
            let at = word(adt, 92 + slot * 16)? as usize;
            require(bytes(adt, at, 4)? == b"KNCM", "raw_mcnk_required")?;
            let id = word(adt, at + 8 + 0x34)?;
            require(id == grid.area_ids[slot], "raw_area_binding")?;
            *flag = if id == 0 {
                u16::MAX
            } else {
                catalog
                    .area(id)
                    .map_or(u16::MAX, |area| area.explore_flag as u16)
            };
        }
        require(flags.as_slice() == grid.area_flags, "area_flags_binding")?;
        let terrain = files.read(
            &format!("{prefix}/{:03}{x:02}{y:02}.map", capture.map),
            MAX_FILE,
        )?;
        let (offset, size) = (word(&terrain, 8)? as usize, word(&terrain, 12)? as usize);
        require(
            bytes(&terrain, offset, 4)? == b"AREA" && offset + size <= terrain.len(),
            "area_section",
        )?;
        let header = u16::from_le_bytes(bytes(&terrain, offset + 4, 2)?.try_into().unwrap());
        let uniform = u16::from_le_bytes(bytes(&terrain, offset + 6, 2)?.try_into().unwrap());
        require(
            header == grid.header_flags
                && uniform == grid.grid_area
                && header == u16::from(flags.iter().all(|v| *v == flags[0]))
                && size == if header == 1 { 8 } else { 520 },
            "area_header",
        )?;
        for (i, flag) in flags.into_iter().enumerate() {
            let stored = if header == 1 {
                uniform
            } else {
                u16::from_le_bytes(bytes(&terrain, offset + 8 + i * 2, 2)?.try_into().unwrap())
            };
            require(stored == flag, "emitted_area_binding")?;
        }
    }
    let grid = [
        32.0f32 - position[0] / (1600.0f32 / 3.0),
        32.0f32 - position[1] / (1600.0f32 / 3.0),
    ];
    require(
        grid.iter().all(|v| (0.0..64.0).contains(v)),
        "position_outside_map",
    )?;
    let tile = grid.map(|v| v.floor() as u8);
    let selected = capture
        .grids
        .iter()
        .find(|g| [g.tile.x, g.tile.y] == tile)
        .ok_or_else(|| error("outside_capture"))?;
    let cell = grid.map(|v| (16.0 * v) as usize & 15);
    let flag = selected.area_flags[cell[0] * 16 + cell[1]];
    let area = catalog.resolve(capture.map, flag);
    let (area_id, zone_id) = area.map_or((0, 0), |area| {
        (area.id, if area.zone == 0 { area.id } else { area.zone })
    });
    let mut parent = area;
    let mut seen = BTreeSet::new();
    while let Some(area) = parent {
        require(seen.insert(area.id), "parent_cycle")?;
        if area.zone == 0 {
            break;
        }
        parent = Some(
            catalog
                .area(area.zone)
                .ok_or_else(|| error("parent_required"))?,
        );
    }
    Ok(Context {
        map: capture.map,
        source_epoch: capture.source_epoch,
        catalog: catalog.digest(),
        area_flag: flag,
        area_id,
        zone_id,
        tile,
        cell,
    })
}
