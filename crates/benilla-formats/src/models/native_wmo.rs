//! Strict, bounded WMO source witnesses for client-static coverage. Renderer recovery (skipped
//! groups, partial arrays and unresolved props) is deliberately never a negative-space proof.
use super::{CollisionMesh, WmoDoodad, WmoDoodadSet};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

const MAX_FILE: usize = 64 * 1024 * 1024;
const MAX_GROUPS: usize = 1024;
const MAX_DOODADS: usize = 65_536;
const MAX_GEOMETRY: usize = 2_097_152;

fn word(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}
fn finite(bytes: &[u8]) -> bool {
    bytes.len().is_multiple_of(4)
        && bytes
            .as_chunks::<4>()
            .0
            .iter()
            .all(|b| f32::from_le_bytes(*b).is_finite())
}
fn chunks(bytes: &[u8]) -> Option<BTreeMap<[u8; 4], &[u8]>> {
    let mut out = BTreeMap::new();
    let mut at = 0usize;
    while at < bytes.len() {
        let header = bytes.get(at..at.checked_add(8)?)?;
        let end = at.checked_add(8)?.checked_add(word(header, 4) as usize)?;
        let data = bytes.get(at + 8..end)?;
        let magic = header[..4].try_into().ok()?;
        // Repeated UV/color render streams do not affect collision. Every source table and
        // geometry stream used by this witness is otherwise unique, including unknown chunks.
        if out.insert(magic, data).is_some() && !matches!(&magic, b"VTOM" | b"VCOM") {
            return None;
        }
        if out.len() > 128 {
            return None;
        }
        at = end;
    }
    Some(out)
}
pub(crate) fn native_model_name(names: &[u8], offset: usize, wmo: bool) -> Option<&str> {
    if offset != 0 && names.get(offset - 1) != Some(&0) {
        return None;
    }
    let tail = names.get(offset..)?;
    let end = tail.iter().take(1025).position(|&b| b == 0)?;
    let name = std::str::from_utf8(&tail[..end]).ok()?;
    let lower = name.to_ascii_lowercase().replace('\\', "/");
    if lower.is_empty()
        || !lower.is_ascii()
        || lower.bytes().any(|b| b < 0x20 || b == 0x7f)
        || lower.contains(':')
        || lower
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
        || if wmo {
            !lower.ends_with(".wmo")
        } else {
            !(lower.ends_with(".m2") || lower.ends_with(".mdx") || lower.ends_with(".mdl"))
        }
    {
        return None;
    }
    Some(name)
}

/// Exact raw root tables; private construction prevents renderer defaults proving absence.
#[derive(Clone, Debug)]
pub struct NativeWmoRoot {
    source: [u8; 32],
    group_flags: Vec<u32>,
    doodads: Vec<WmoDoodad>,
    sets: Vec<WmoDoodadSet>,
}
impl NativeWmoRoot {
    pub fn read(bytes: &[u8]) -> Option<Self> {
        if bytes.len() > MAX_FILE {
            return None;
        }
        let c = chunks(bytes)?;
        if *c.get(b"REVM")? != 17u32.to_le_bytes() || c.contains_key(b"PGOM") {
            return None;
        }
        let h = *c.get(b"DHOM")?;
        if h.len() != 64
            || !finite(&h[36..60])
            || (0..3)
                .any(|i| f32::from_bits(word(h, 36 + i * 4)) > f32::from_bits(word(h, 48 + i * 4)))
        {
            return None;
        }
        let (groups, doodads, sets) = (
            word(h, 4) as usize,
            word(h, 20) as usize,
            word(h, 24) as usize,
        );
        if groups == 0 || groups > MAX_GROUPS || doodads > MAX_DOODADS || sets > MAX_DOODADS {
            return None;
        }
        let gi = *c.get(b"IGOM")?;
        let dd = c.get(b"DDOM").copied().unwrap_or_default();
        let ds = c.get(b"SDOM").copied().unwrap_or_default();
        let names = c.get(b"NDOM").copied().unwrap_or_default();
        if gi.len() != groups * 32 || dd.len() != doodads * 40 || ds.len() != sets * 32 {
            return None;
        }
        for g in gi.as_chunks::<32>().0 {
            if !finite(&g[4..28])
                || (0..3).any(|i| {
                    f32::from_bits(word(g, 4 + i * 4)) > f32::from_bits(word(g, 16 + i * 4))
                })
            {
                return None;
            }
        }
        for d in dd.as_chunks::<40>().0 {
            if word(d, 0) >> 24 != 0 || !finite(&d[4..36]) || f32::from_bits(word(d, 32)) <= 0.0 {
                return None;
            }
            native_model_name(names, word(d, 0) as usize, false)?;
            let norm: f64 = (16..32)
                .step_by(4)
                .map(|i| f64::from(f32::from_bits(word(d, i))).powi(2))
                .sum();
            // Normalization must remain finite in the same f32 arithmetic as the producer.
            if !(1e-12..=1e12).contains(&norm) {
                return None;
            }
        }
        let mut covered = vec![false; doodads];
        for s in ds.as_chunks::<32>().0 {
            let start = word(s, 20) as usize;
            let end = start.checked_add(word(s, 24) as usize)?;
            for slot in covered.get_mut(start..end)? {
                *slot = true;
            }
        }
        if covered.iter().any(|v| !v) {
            return None;
        }
        Some(Self {
            source: Sha256::digest(bytes).into(),
            group_flags: gi.as_chunks::<32>().0.iter().map(|g| word(g, 0)).collect(),
            doodads: dd
                .as_chunks::<40>()
                .0
                .iter()
                .map(|d| WmoDoodad {
                    model: native_model_name(names, word(d, 0) as usize, false)
                        .unwrap()
                        .into(),
                    position: std::array::from_fn(|i| f32::from_bits(word(d, 4 + i * 4))),
                    orientation: std::array::from_fn(|i| f32::from_bits(word(d, 16 + i * 4))),
                    scale: f32::from_bits(word(d, 32)),
                    color: [d[38], d[37], d[36], d[39]],
                })
                .collect(),
            sets: ds
                .as_chunks::<32>()
                .0
                .iter()
                .map(|s| WmoDoodadSet {
                    start: word(s, 20),
                    count: word(s, 24),
                })
                .collect(),
        })
    }
    pub fn source_digest(&self) -> [u8; 32] {
        self.source
    }
    pub fn group_count(&self) -> usize {
        self.group_flags.len()
    }
    pub fn doodads(&self) -> &[WmoDoodad] {
        &self.doodads
    }
    pub fn doodad_sets(&self) -> &[WmoDoodadSet] {
        &self.sets
    }
    /// Exact renderer order, set zero then selected set. Overlap rejects instead of accepting
    /// duplicate spawned props; a missing selected set never means an empty set.
    pub fn selected_doodads(&self, selected: u16) -> Option<Vec<usize>> {
        if self.sets.is_empty() {
            return (selected == 0 && self.doodads.is_empty()).then(Vec::new);
        }
        let mut out = Vec::new();
        let mut seen = BTreeSet::new();
        for i in [0, usize::from(selected)]
            .into_iter()
            .take(if selected == 0 { 1 } else { 2 })
        {
            let s = self.sets.get(i)?;
            for d in s.start as usize..(s.start + s.count) as usize {
                if !seen.insert(d) {
                    return None;
                }
                out.push(d);
            }
        }
        Some(out)
    }
}

/// One complete dry MOGP, including exact raw MODR membership and both client collision masks.
#[derive(Clone, Debug)]
pub struct NativeWmoGroup {
    source: [u8; 32],
    root: [u8; 32],
    index: usize,
    walking: CollisionMesh,
    camera: CollisionMesh,
    doodad_refs: Vec<u16>,
}
impl NativeWmoGroup {
    pub fn read(bytes: &[u8], root: &NativeWmoRoot, index: usize) -> Option<Self> {
        if bytes.len() > MAX_FILE {
            return None;
        }
        let outer = chunks(bytes)?;
        if *outer.get(b"REVM")? != 17u32.to_le_bytes() || outer.contains_key(b"DHOM") {
            return None;
        }
        let g = *outer.get(b"PGOM")?;
        if g.len() < 68
            || word(g, 8) != *root.group_flags.get(index)?
            || word(g, 8) & 0x1000 != 0
            || word(g, 0x34) != 0xf
            || !finite(&g[12..36])
            || (0..3)
                .any(|i| f32::from_bits(word(g, 12 + i * 4)) > f32::from_bits(word(g, 24 + i * 4)))
        {
            return None;
        }
        let c = chunks(&g[68..])?;
        if c.contains_key(b"QILM") {
            return None;
        }
        let (v, i, f) = (*c.get(b"TVOM")?, *c.get(b"IVOM")?, *c.get(b"YPOM")?);
        if !v.len().is_multiple_of(12)
            || v.len() / 12 > 65_536
            || !finite(v)
            || !i.len().is_multiple_of(6)
            || i.len() / 6 > 262_144
            || f.len() != i.len() / 3
            || i.as_chunks::<2>()
                .0
                .iter()
                .any(|idx| usize::from(u16::from_le_bytes(*idx)) >= v.len() / 12)
            || f.as_chunks::<2>().0.iter().any(|f| f[0] & 0x80 != 0)
        {
            return None;
        }
        let refs = c.get(b"RDOM").copied().unwrap_or_default();
        if !refs.len().is_multiple_of(2) || refs.len() / 2 > MAX_DOODADS {
            return None;
        }
        let doodad_refs: Vec<_> = refs
            .as_chunks::<2>()
            .0
            .iter()
            .map(|r| u16::from_le_bytes(*r))
            .collect();
        if doodad_refs
            .iter()
            .any(|&d| usize::from(d) >= root.doodads.len())
        {
            return None;
        }
        // Same first-use vertex remapping and face order as the renderer's gather. Build only
        // collision arrays here, without allocating unrelated render/UV/material streams.
        let gather = |reject: u8| {
            let mut hull = CollisionMesh::default();
            let mut remap = vec![u32::MAX; v.len() / 12];
            for (tri, flags) in i.as_chunks::<6>().0.iter().zip(f.as_chunks::<2>().0) {
                if flags[0] & reject != 0 {
                    continue;
                }
                for index in tri.as_chunks::<2>().0 {
                    let index = usize::from(u16::from_le_bytes(*index));
                    if remap[index] == u32::MAX {
                        remap[index] = hull.positions.len() as u32;
                        hull.positions.push(std::array::from_fn(|axis| {
                            f32::from_bits(word(v, index * 12 + axis * 4))
                        }));
                    }
                    hull.indices.push(remap[index]);
                }
            }
            hull
        };
        let (walking, camera) = (gather(0x04), gather(0x02));
        Some(Self {
            source: Sha256::digest(bytes).into(),
            root: root.source,
            index,
            walking,
            camera,
            doodad_refs,
        })
    }
    pub fn source_digest(&self) -> [u8; 32] {
        self.source
    }
    pub fn walking(&self) -> &CollisionMesh {
        &self.walking
    }
    pub fn camera(&self) -> &CollisionMesh {
        &self.camera
    }
    pub fn doodad_refs(&self) -> &[u16] {
        &self.doodad_refs
    }
}

/// All raw groups in root order. An omitted, repeated or foreign group cannot be assembled.
#[derive(Clone, Debug)]
pub struct NativeWmoCollision {
    source: [u8; 32],
    root: NativeWmoRoot,
    walking: CollisionMesh,
    camera: CollisionMesh,
}
impl NativeWmoCollision {
    pub fn from_parts(root: NativeWmoRoot, groups: Vec<NativeWmoGroup>) -> Option<Self> {
        if groups.len() != root.group_count() {
            return None;
        }
        let mut referenced = vec![false; root.doodads.len()];
        let mut h = Sha256::new();
        h.update(b"benilla-native-wmo-client-v1\0");
        h.update(root.source);
        let (mut walking, mut camera) = (CollisionMesh::default(), CollisionMesh::default());
        for (index, g) in groups.into_iter().enumerate() {
            if g.index != index || g.root != root.source {
                return None;
            }
            h.update(g.source);
            for d in g.doodad_refs {
                referenced[usize::from(d)] = true;
            }
            for (dst, src) in [(&mut walking, g.walking), (&mut camera, g.camera)] {
                if dst.positions.len() + src.positions.len() > MAX_GEOMETRY
                    || (dst.indices.len() + src.indices.len()) / 3 > MAX_GEOMETRY
                {
                    return None;
                }
                let base = dst.positions.len() as u32;
                dst.positions.extend(src.positions);
                dst.indices
                    .extend(src.indices.into_iter().map(|i| i + base));
            }
        }
        // The renderer draws otherwise-unowned MODD records as a deviation; that is outside
        // this reference-bound subset, rather than a reason to omit them from the inventory.
        if referenced.iter().any(|v| !v) {
            return None;
        }
        Some(Self {
            source: h.finalize().into(),
            root,
            walking,
            camera,
        })
    }
    pub fn source_digest(&self) -> [u8; 32] {
        self.source
    }
    pub fn root(&self) -> &NativeWmoRoot {
        &self.root
    }
    pub fn walking(&self) -> &CollisionMesh {
        &self.walking
    }
    pub fn camera(&self) -> &CollisionMesh {
        &self.camera
    }
    pub fn matches(&self, walking: Option<&CollisionMesh>, camera: Option<&CollisionMesh>) -> bool {
        [(&self.walking, walking), (&self.camera, camera)]
            .into_iter()
            .all(|(a, b)| match b {
                Some(b) => a.positions == b.positions && a.indices == b.indices,
                None => a.positions.is_empty() && a.indices.is_empty(),
            })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn chunk(magic: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut b = magic.to_vec();
        b.extend((data.len() as u32).to_le_bytes());
        b.extend(data);
        b
    }
    pub(crate) fn root(prop: bool) -> Vec<u8> {
        let mut b = chunk(b"REVM", &17u32.to_le_bytes());
        let mut h = vec![0; 64];
        h[4..8].copy_from_slice(&1u32.to_le_bytes());
        h[20..24].copy_from_slice(&u32::from(prop).to_le_bytes());
        h[24..28].copy_from_slice(&u32::from(prop).to_le_bytes());
        b.extend(chunk(b"DHOM", &h));
        let mut gi = vec![0; 32];
        gi[..4].copy_from_slice(&8u32.to_le_bytes());
        b.extend(chunk(b"IGOM", &gi));
        if prop {
            b.extend(chunk(b"NDOM", b"World\\Chair.mdx\0"));
            let mut d = vec![0; 40];
            d[28..32].copy_from_slice(&1f32.to_le_bytes());
            d[32..36].copy_from_slice(&1f32.to_le_bytes());
            b.extend(chunk(b"DDOM", &d));
            let mut set = vec![0; 32];
            set[24..28].copy_from_slice(&1u32.to_le_bytes());
            b.extend(chunk(b"SDOM", &set));
        }
        b
    }
    pub(crate) fn group(prop: bool) -> Vec<u8> {
        let mut h = vec![0; 68];
        h[8..12].copy_from_slice(&8u32.to_le_bytes());
        h[0x34..0x38].copy_from_slice(&15u32.to_le_bytes());
        let mut v = Vec::new();
        for p in [[0f32, 0., 0.], [1., 0., 0.], [0., 1., 0.]] {
            for f in p {
                v.extend(f.to_le_bytes());
            }
        }
        h.extend(chunk(b"TVOM", &v));
        let indices: Vec<_> = [0u16, 1, 2, 0, 1, 2, 0, 1, 2]
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect();
        h.extend(chunk(b"IVOM", &indices));
        h.extend(chunk(b"YPOM", &[0, 0, 4, 0, 2, 0]));
        if prop {
            h.extend(chunk(b"RDOM", &0u16.to_le_bytes()));
        }
        let mut b = chunk(b"REVM", &17u32.to_le_bytes());
        b.extend(chunk(b"PGOM", &h));
        b
    }
    fn at(b: &[u8], magic: &[u8; 4]) -> usize {
        b.windows(4).position(|w| w == magic).unwrap() + 8
    }
    #[test]
    fn native_wmo_exact_walk_camera_and_raw_source_closure() {
        let r = NativeWmoRoot::read(&root(true)).unwrap();
        let g = NativeWmoGroup::read(&group(true), &r, 0).unwrap();
        let (mut walk, mut camera) = (CollisionMesh::default(), CollisionMesh::default());
        crate::accumulate_wmo_group_collision(&group(true), &mut walk.positions, &mut walk.indices);
        crate::accumulate_wmo_group_camera_collision(
            &group(true),
            &mut camera.positions,
            &mut camera.indices,
        );
        assert_eq!(walk.positions, g.walking().positions);
        assert_eq!(walk.indices, g.walking().indices);
        assert_eq!(camera.positions, g.camera().positions);
        assert_eq!(camera.indices, g.camera().indices);
        assert_eq!(g.walking().indices.len(), 6);
        assert_eq!(g.camera().indices.len(), 6);
        assert_eq!(g.doodad_refs(), [0]);
        assert_eq!(r.selected_doodads(0), Some(vec![0]));
        assert!(r.selected_doodads(1).is_none());
        let c = NativeWmoCollision::from_parts(r.clone(), vec![g.clone()]).unwrap();
        assert!(c.matches(Some(c.walking()), Some(c.camera())));
        assert!(!c.matches(None, None));
        assert!(NativeWmoCollision::from_parts(r.clone(), vec![]).is_none());
        assert!(NativeWmoCollision::from_parts(r, vec![g.clone(), g]).is_none());
        let mut changed = root(true);
        let offset = at(&changed, b"DHOM");
        changed[offset + 60] = 1;
        let other = NativeWmoRoot::read(&changed).unwrap();
        assert!(NativeWmoCollision::from_parts(
            other,
            vec![NativeWmoGroup::read(&group(true), c.root(), 0).unwrap()]
        )
        .is_none());
    }
    #[test]
    fn native_wmo_rejects_missing_duplicate_malformed_and_liquid_groups() {
        let r = NativeWmoRoot::read(&root(true)).unwrap();
        let b = group(true);
        assert!(NativeWmoGroup::read(&b, &r, 1).is_none());
        for length in 0..b.len() {
            assert!(
                NativeWmoGroup::read(&b[..length], &r, 0).is_none(),
                "{length}"
            );
        }
        for (magic, offset, value) in [
            (b"PGOM", 0x34, 0u32),
            (b"TVOM", 0, f32::NAN.to_bits()),
            (b"IVOM", 0, 0xffff),
            (b"RDOM", 0, 1),
        ] {
            let mut bad = b.clone();
            let pos = at(&bad, magic) + offset;
            // MODR has one u16, all other mutated fields have at least four bytes.
            let n = if magic == b"RDOM" { 2 } else { 4 };
            bad[pos..pos + n].copy_from_slice(&value.to_le_bytes()[..n]);
            assert!(NativeWmoGroup::read(&bad, &r, 0).is_none());
        }
        let gp = at(&b, b"PGOM");
        for suffix in [
            chunk(b"TVOM", &[]),
            chunk(b"IVOM", &[]),
            chunk(b"YPOM", &[]),
            chunk(b"RDOM", &[]),
            chunk(b"QILM", &[]),
        ] {
            let mut bad = b.clone();
            bad.extend(suffix);
            let len = (bad.len() - gp) as u32;
            bad[gp - 4..gp].copy_from_slice(&len.to_le_bytes());
            assert!(NativeWmoGroup::read(&bad, &r, 0).is_none());
        }
        let unowned = NativeWmoGroup::read(&group(false), &r, 0).unwrap();
        assert!(NativeWmoCollision::from_parts(r, vec![unowned]).is_none());
    }
    #[test]
    fn native_wmo_root_rejects_filtered_doodads_and_unbounded_counts() {
        let b = root(true);
        for (magic, offset, value) in [
            (b"DHOM", 4, 1025u32),
            (b"DHOM", 20, 65537),
            (b"DDOM", 0, 1),
            (b"DDOM", 28, 0),
            (b"DDOM", 32, 0),
            (b"SDOM", 20, u32::MAX),
        ] {
            let mut bad = b.clone();
            let pos = at(&bad, magic) + offset;
            bad[pos..pos + 4].copy_from_slice(&value.to_le_bytes());
            assert!(NativeWmoRoot::read(&bad).is_none());
        }
        for magic in [b"DHOM", b"IGOM", b"NDOM", b"DDOM", b"SDOM"] {
            let mut bad = b.clone();
            bad.extend(chunk(magic, &[]));
            assert!(NativeWmoRoot::read(&bad).is_none());
        }
        let mut bad = b.clone();
        let pos = at(&bad, b"NDOM");
        bad[pos] = b'/';
        assert!(NativeWmoRoot::read(&bad).is_none());
        let mut bad = b;
        bad.push(0);
        assert!(NativeWmoRoot::read(&bad).is_none());
    }
    #[test]
    fn native_wmo_authored_empty_group_is_distinct_from_missing_geometry_arrays() {
        let r = NativeWmoRoot::read(&root(false)).unwrap();
        let b = group(false);
        let gp = at(&b, b"PGOM");
        let mut h = b[gp..gp + 68].to_vec();
        for magic in [b"TVOM", b"IVOM", b"YPOM"] {
            h.extend(chunk(magic, &[]));
        }
        let mut empty = chunk(b"REVM", &17u32.to_le_bytes());
        empty.extend(chunk(b"PGOM", &h));
        let g = NativeWmoGroup::read(&empty, &r, 0).unwrap();
        assert!(NativeWmoCollision::from_parts(r.clone(), vec![g])
            .unwrap()
            .matches(None, None));
        let mut missing = chunk(b"REVM", &17u32.to_le_bytes());
        missing.extend(chunk(b"PGOM", &h[..68]));
        assert!(NativeWmoGroup::read(&missing, &r, 0).is_none());
    }
}
