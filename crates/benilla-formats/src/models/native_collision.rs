//! Strict collision-only source witness. Rendering may omit a failed or malformed hull;
//! native authority must distinguish that failure from an authored model with no hull.
use sha2::{Digest, Sha256};

use super::CollisionMesh;

/// Complete vanilla M2 collision arrays, bound to the exact loaded bytes. Private fields
/// prevent a render-side `None` or a caller's digest from manufacturing a no-collision proof.
#[derive(Debug, Clone)]
pub struct NativeM2Collision {
    source: [u8; 32],
    hull: CollisionMesh,
}
impl NativeM2Collision {
    pub fn source_digest(&self) -> [u8; 32] {
        self.source
    }
    pub fn hull(&self) -> &CollisionMesh {
        &self.hull
    }
    /// Proves that render-side collision still represents the strictly checked source.
    pub fn matches(&self, hull: Option<&CollisionMesh>) -> bool {
        match hull {
            None => self.hull.positions.is_empty() && self.hull.indices.is_empty(),
            Some(hull) => {
                hull.positions == self.hull.positions && hull.indices == self.hull.indices
            }
        }
    }
}

/// MD20 256/257 hull arrays at 0xec/0xf4/0xfc, the same pre-Wrath header walk as
/// `benilla_m2::parse_m2`. This intentionally admits no later layout or malformed-empty hull.
/// A model with nonzero triangle count must carry every finite vertex, normal and index.
pub fn native_m2_collision(bytes: &[u8]) -> Option<NativeM2Collision> {
    const HEADER: usize = 0x144;
    const MAX_VERTICES: usize = 65_536;
    const MAX_TRIANGLES: usize = 65_536;
    if bytes.len() < HEADER || &bytes[..4] != b"MD20" {
        return None;
    }
    let word = |at| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
    if !matches!(word(4), 256 | 257) {
        return None;
    }
    let array = |at, width, max| {
        let (count, offset) = (word(at), word(at + 4));
        if count > max || (count != 0 && offset < HEADER) {
            return None;
        }
        let end = offset.checked_add(count.checked_mul(width)?)?;
        if count == 0 {
            // Zero count is authoritative; a nonzero stale offset is not dereferenced.
            Some(&bytes[0..0])
        } else {
            bytes.get(offset..end)
        }
    };
    let triangles = array(0xec, 2, MAX_TRIANGLES * 3)?;
    let vertices = array(0xf4, 12, MAX_VERTICES)?;
    let normals = array(0xfc, 12, MAX_TRIANGLES)?;
    if !triangles.len().is_multiple_of(6)
        || (triangles.is_empty() != vertices.is_empty())
        || normals.len() != triangles.len() / 6 * 12
    {
        return None;
    }
    let vectors = |data: &[u8]| -> Option<Vec<[f32; 3]>> {
        data.as_chunks::<12>()
            .0
            .iter()
            .map(|v| {
                let p = std::array::from_fn(|i| {
                    f32::from_le_bytes(v[i * 4..i * 4 + 4].try_into().unwrap())
                });
                p.into_iter().all(f32::is_finite).then_some(p)
            })
            .collect()
    };
    let positions = vectors(vertices)?;
    vectors(normals)?;
    let indices: Vec<_> = triangles
        .as_chunks::<2>()
        .0
        .iter()
        .map(|v| u32::from(u16::from_le_bytes(*v)))
        .collect();
    if indices.iter().any(|&i| i as usize >= positions.len()) {
        return None;
    }
    Some(NativeM2Collision {
        source: Sha256::digest(bytes).into(),
        hull: CollisionMesh { positions, indices },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source() -> Vec<u8> {
        let mut b = vec![0; 0x144];
        b[..4].copy_from_slice(b"MD20");
        b[4..8].copy_from_slice(&256u32.to_le_bytes());
        for (at, count, offset) in [(0xec, 3u32, 0x144u32), (0xf4, 3, 0x14a), (0xfc, 1, 0x16e)] {
            b[at..at + 4].copy_from_slice(&count.to_le_bytes());
            b[at + 4..at + 8].copy_from_slice(&offset.to_le_bytes());
        }
        for i in [0u16, 1, 2] {
            b.extend(i.to_le_bytes());
        }
        for p in [
            [0.0f32, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ] {
            for v in p {
                b.extend(v.to_le_bytes());
            }
        }
        b
    }
    #[test]
    fn native_collision_exact_source_and_render_match() {
        let b = source();
        let c = native_m2_collision(&b).unwrap();
        assert_eq!(c.source_digest(), <[u8; 32]>::from(Sha256::digest(&b)));
        let hull = super::super::parse_m2_collision_hull(&b).unwrap();
        assert!(c.matches(Some(&hull)));
        assert!(!c.matches(None));
        let mut other = hull.clone();
        other.positions[0][0] = 0.1;
        assert!(!c.matches(Some(&other)));
        let mut changed = b.clone();
        changed[0x140] = 1;
        assert_ne!(
            c.source_digest(),
            native_m2_collision(&changed).unwrap().source_digest()
        );
    }
    #[test]
    fn native_collision_only_authored_empty_is_empty() {
        let mut b = source();
        b[0xec..0x104].fill(0);
        let c = native_m2_collision(&b).unwrap();
        assert!(c.matches(None));
        for (offset, value) in [(0xf4, 3u32), (0xfc, 1)] {
            let mut bad = b.clone();
            bad[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            assert!(native_m2_collision(&bad).is_none());
        }
    }
    #[test]
    fn native_collision_malformed_is_not_absent() {
        let b = source();
        for (offset, value) in [
            (4, 258u32),
            (0xec, 2),
            (0xf0, 1),
            (0xf4, 65537),
            (0xf8, u32::MAX),
            (0xfc, 0),
        ] {
            let mut bad = b.clone();
            bad[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            assert!(native_m2_collision(&bad).is_none(), "{offset:x}");
        }
        let mut bad = b.clone();
        bad[0x144..0x146].copy_from_slice(&3u16.to_le_bytes());
        assert!(super::super::parse_m2_collision_hull(&bad)
            .unwrap()
            .is_empty());
        assert!(native_m2_collision(&bad).is_none());
        let mut bad = b.clone();
        bad[0x14a..0x14e].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(native_m2_collision(&bad).is_none());
        assert!(native_m2_collision(&b[..b.len() - 1]).is_none());
    }
}
