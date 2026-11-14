// SPDX-License-Identifier: GPL-3.0-or-later
//! ModelInstance::intersectPoint and GroupModel::IsInsideObject exclusions.
//! A candidate group is a precise refusal, never a replacement ray interpreter.
use super::*;
use binary::{bih, bound, bytes, contains, vector, word};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileRow {
    file: String,
    bytes: usize,
    sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Index {
    format: String,
    map: u32,
    collision_epoch: String,
    files: Vec<FileRow>,
    gameplay_admitted: bool,
}
#[derive(Clone, Copy)]
struct Spawn {
    record: crate::Digest,
    flags: u32,
    bound: [f32; 6],
    model: usize,
    position: [f32; 3],
    rotation: [f32; 3],
    scale: f32,
}
fn read(
    files: &mut Files<'_>,
    prefix: &str,
    name: &str,
    rows: &BTreeMap<&str, &FileRow>,
) -> Result<Arc<Vec<u8>>> {
    let row = rows
        .get(name)
        .ok_or_else(|| error("collision_file_required"))?;
    let data = files.read(&format!("{prefix}/{name}"), MAX_FILE)?;
    require(
        data.len() == row.bytes && json::hash_bytes(&data) == row.sha256,
        "collision_file_binding",
    )?;
    Ok(data)
}
fn model_point(point: [f32; 3], spawn: Spawn) -> Result<[f32; 3]> {
    require(
        spawn.rotation[0] == 0.0
            && spawn.rotation[2] == 0.0
            && (0.001..=1000.0).contains(&spawn.scale),
        "wmo_transform_requires_owner",
    )?;
    require(
        (-1800.0..=1800.0).contains(&spawn.rotation[1]),
        "rotation_domain",
    )?;
    let radians =
        (f64::from_bits(0x4009_21fb_5444_2d28) * f64::from(spawn.rotation[1]) / 180.0) as f32;
    let [sine, cosine] =
        benilla_sim::source_trig_fma::sin_cos_euler_radians_bits_native(radians.to_bits())
            .map_err(|_| error("rotation_domain"))?
            .map(f32::from_bits);
    let determinant = cosine * cosine - -sine * sine;
    let inverse = (1.0f64 / f64::from(determinant)) as f32;
    let (c, s) = (cosine * inverse, sine * inverse);
    let p: [f32; 3] = std::array::from_fn(|i| point[i] - spawn.position[i]);
    let inverse_scale = 1.0 / spawn.scale;
    let out = [
        (c * p[0] + s * p[1]) * inverse_scale,
        (-s * p[0] + c * p[1]) * inverse_scale,
        ((determinant * inverse) * p[2]) * inverse_scale,
    ];
    require(
        out.iter().all(|v| v.is_finite()),
        "wmo_transform_requires_owner",
    )?;
    Ok(out)
}
fn exclude_groups(data: &[u8], point: [f32; 3]) -> Result<()> {
    require(
        bytes(data, 0, 12)? == b"VMAP_7.0WMOD" && word(data, 12)? == 8,
        "world_model",
    )?;
    if data.len() == 20 {
        return Ok(());
    }
    require(bytes(data, 20, 4)? == b"GMOD", "world_model")?;
    let groups = word(data, 24)? as usize;
    require(groups <= 262144, "group_count")?;
    let mut at = 28;
    for _ in 0..groups {
        let bound = bound(data, at)?;
        at += 32;
        require(bytes(data, at, 4)? == b"VERT", "model_vertices")?;
        let (size, count) = (word(data, at + 4)? as usize, word(data, at + 8)? as usize);
        require(
            count <= 262144 && size == 4 + count * 12 && at + 8 + size <= data.len(),
            "model_vertices",
        )?;
        at += 8 + size;
        if count == 0 {
            continue;
        }
        require(bytes(data, at, 4)? == b"TRIM", "model_triangles")?;
        let (size, triangles) = (word(data, at + 4)? as usize, word(data, at + 8)? as usize);
        require(
            triangles <= 262144 && size == 4 + triangles * 12 && at + 8 + size <= data.len(),
            "model_triangles",
        )?;
        at += 8 + size;
        require(bytes(data, at, 4)? == b"MBIH", "model_bih")?;
        let (end, count) = bih(data, at + 4)?;
        at = end;
        require(
            count == triangles && bytes(data, at, 4)? == b"LIQU",
            "model_bih",
        )?;
        require(word(data, at + 4)? == 0, "wmo_liquid_requires_owner")?;
        at += 8;
        require(
            triangles == 0 || !contains(bound, point),
            "wmo_group_requires_owner",
        )?;
    }
    require(bytes(data, at, 4)? == b"GBIH", "group_bih")?;
    let (end, count) = bih(data, at + 4)?;
    require(count == groups && end == data.len(), "group_bih")
}

pub(super) fn exclude(
    files: &mut Files<'_>,
    prefix: &str,
    position: [f32; 3],
    map: u32,
) -> Result<Value> {
    let index: Index = parse(
        files.json(&format!("{prefix}/index.json"))?,
        "collision_fields",
    )?;
    require(
        index.format == "benilla-source-location-wmo-index-v1" && !index.gameplay_admitted,
        "collision_policy",
    )?;
    require(hash_text(&index.collision_epoch), "collision_epoch")?;
    require(index.map == map, "collision_map")?;
    require(index.files.len() <= MAX_FILES, "collision_file_count")?;
    let mut rows = BTreeMap::new();
    for row in &index.files {
        require(
            component(&row.file)
                && row.bytes <= MAX_FILE
                && hash_text(&row.sha256)
                && rows.insert(row.file.as_str(), row).is_none(),
            "collision_file",
        )?;
    }
    let tree_name = format!("{map:03}.vmtree");
    let tree = read(files, prefix, &tree_name, &rows)?;
    require(
        bytes(&tree, 0, 13)? == b"VMAP_7.0\x01NODE",
        "collision_tree",
    )?;
    let (at, count) = bih(&tree, 13)?;
    require(tree.get(at..) == Some(b"GOBJ".as_slice()), "collision_tree")?;
    let mut spawns = vec![None; count];
    let models = read(files, prefix, "models.index", &rows)?;
    let model_count = word(&models, 0)? as usize;
    require(model_count <= MAX_FILES, "model_count")?;
    let mut at = 4;
    let mut names = BTreeMap::new();
    let mut model_files = Vec::with_capacity(model_count);
    for i in 0..model_count {
        let size = word(&models, at)? as usize;
        at += 4;
        require(size <= 500, "model_name")?;
        let name = bytes(&models, at, size)?;
        at += size;
        let size = word(&models, at)? as usize;
        at += 4;
        require(size <= 500, "model_name")?;
        let file =
            std::str::from_utf8(bytes(&models, at, size)?).map_err(|_| error("model_name"))?;
        at += size;
        require(
            names.insert(name, i).is_none() && rows.contains_key(file) && file.ends_with(".vmo"),
            "model_binding",
        )?;
        model_files.push(file);
    }
    require(at == models.len(), "model_index")?;
    for row in &index.files {
        let name = row.file.as_str();
        if name == tree_name || name == "models.index" {
            continue;
        }
        let data = read(files, prefix, name, &rows)?;
        if name.ends_with(".vmo") {
            continue;
        }
        require(
            name.ends_with(".vmtile") && bytes(&data, 0, 8)? == b"VMAP_7.0",
            "collision_tile",
        )?;
        let records = word(&data, 8)? as usize;
        require(records <= MAX_WORDS, "spawn_count")?;
        let mut at = 12;
        for _ in 0..records {
            let start = at;
            let flags = word(&data, at)?;
            require(
                flags & !7 == 0 && flags & 4 != 0 && flags & 2 == 0,
                "spawn_flags",
            )?;
            let position = vector(&data, at + 10)?;
            let rotation = vector(&data, at + 22)?;
            let scale = vector::<1>(&data, at + 34)?[0];
            let bound = bound(&data, at + 38)?;
            let size = word(&data, at + 62)? as usize;
            require(size <= 500, "spawn_name")?;
            let name = bytes(&data, at + 66, size)?;
            at += 66 + size;
            let slot = word(&data, at)? as usize;
            at += 4;
            let model = *names.get(name).ok_or_else(|| error("spawn_slot"))?;
            require(slot < count, "spawn_slot")?;
            let record: crate::Digest = Sha256::digest(bytes(&data, start, at - start)?).into();
            if let Some(previous) = &spawns[slot] {
                let previous: &Spawn = previous;
                require(previous.record == record, "duplicate_spawn_binding")?;
            } else {
                spawns[slot] = Some(Spawn {
                    record,
                    flags,
                    bound,
                    model,
                    position,
                    rotation,
                    scale,
                });
            }
        }
        require(at == data.len(), "collision_tile")?;
    }
    require(
        spawns.iter().all(Option::is_some),
        "complete_collision_slots",
    )?;
    let mid = (0.5f32 * 64.0) * f32::from_bits(0x4405_5555);
    let point = [mid - position[0], mid - position[1], position[2]];
    let (mut wmos, mut candidates) = (0, 0);
    for spawn in spawns.into_iter().flatten().filter(|s| s.flags & 1 == 0) {
        wmos += 1;
        if contains(spawn.bound, point) {
            candidates += 1;
            let local = model_point(point, spawn)?;
            exclude_groups(
                &read(files, prefix, model_files[spawn.model], &rows)?,
                local,
            )?;
        }
    }
    Ok(
        json!({"collision_epoch":index.collision_epoch,"slots":count,"wmo_spawns":wmos,"wmo_bounds_containing_position":candidates}),
    )
}
