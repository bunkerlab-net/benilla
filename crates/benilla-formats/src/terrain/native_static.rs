//! Raw ADT completeness for dry native client-static coverage. Tolerant display parsing must
//! never erase a placement, duplicate declaration, unresolved name, or dangling MCRF reference.
use super::native_subchunk;
use std::collections::BTreeSet;
pub(super) const MAX_PLACEMENTS: usize = 4096;
pub(super) fn complete(bytes: &[u8]) -> bool {
    complete_with_liquids(bytes, false)
}
pub(super) fn complete_with_liquids(bytes: &[u8], allow_liquids: bool) -> bool {
    if bytes.len() > 64 * 1024 * 1024 {
        return false;
    }
    let mut cursor = 0usize;
    let mut terrain = 0usize;
    let mut seen = BTreeSet::new();
    let (mut names, mut offsets, mut placements) = (&[][..], &[][..], &[][..]);
    let (mut wnames, mut woffsets, mut wplacements) = (&[][..], &[][..], &[][..]);
    let mut references = Vec::new();
    let mut wreferences = Vec::new();
    while cursor < bytes.len() {
        let Some(header) = bytes.get(cursor..cursor.saturating_add(8)) else {
            return false;
        };
        let size = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
        let Some(end) = cursor.checked_add(8).and_then(|n| n.checked_add(size)) else {
            return false;
        };
        let Some(data) = bytes.get(cursor + 8..end) else {
            return false;
        };
        let magic = &header[..4];
        if matches!(
            magic,
            b"XDMM" | b"DIMM" | b"FDDM" | b"OMWM" | b"DIWM" | b"FDOM"
        ) && !seen.insert(magic)
        {
            return false;
        }
        match magic {
            b"XDMM" => names = data,
            b"DIMM" => offsets = data,
            b"FDDM" => placements = data,
            b"OMWM" => wnames = data,
            b"DIWM" => woffsets = data,
            b"FDOM" => wplacements = data,
            b"O2HM" if !allow_liquids => return false,
            b"KNCM" => {
                terrain += 1;
                if terrain > 256 || data.len() < 128 {
                    return false;
                }
                let word = |at| u32::from_le_bytes(data[at..at + 4].try_into().unwrap());
                if (!allow_liquids && (word(0) & 0x3c != 0 || word(0x60) != 0 || word(0x64) != 0))
                    || native_subchunk(data, word(0x14), b"TVCM").is_none_or(|h| h.len() != 580)
                {
                    return false;
                }
                let (count, wcount) = (word(0x10) as usize, word(0x38) as usize);
                if count + wcount > MAX_PLACEMENTS {
                    return false;
                }
                if word(0x20) == 0 {
                    if count + wcount != 0 {
                        return false;
                    }
                } else {
                    let Some(raw) = native_subchunk(data, word(0x20), b"FRCM") else {
                        return false;
                    };
                    if raw.len() != (count + wcount) * 4 {
                        return false;
                    }
                    for (slice, dst) in [
                        (&raw[..count * 4], &mut references),
                        (&raw[count * 4..], &mut wreferences),
                    ] {
                        let mut local = BTreeSet::new();
                        for r in slice.as_chunks::<4>().0 {
                            let r = u32::from_le_bytes(*r) as usize;
                            if !local.insert(r) {
                                return false;
                            }
                            dst.push(r);
                        }
                    }
                }
            }
            _ => {}
        }
        cursor = end;
    }
    if terrain != 256 || placements.len() / 36 + wplacements.len() / 64 > MAX_PLACEMENTS {
        return false;
    }
    let mut ids = BTreeSet::new();
    for (names, offsets, placements, stride, refs, wmo) in [
        (names, offsets, placements, 36, references, false),
        (wnames, woffsets, wplacements, 64, wreferences, true),
    ] {
        if !offsets.len().is_multiple_of(4)
            || !placements.len().is_multiple_of(stride)
            || offsets.len() / 4 > MAX_PLACEMENTS
        {
            return false;
        }
        for offset in offsets.as_chunks::<4>().0 {
            if crate::models::native_model_name(names, u32::from_le_bytes(*offset) as usize, wmo)
                .is_none()
            {
                return false;
            }
        }
        for p in placements.chunks_exact(stride) {
            let word = |at| u32::from_le_bytes(p[at..at + 4].try_into().unwrap());
            if word(0) as usize >= offsets.len() / 4
                || !ids.insert(word(4))
                || (8..if wmo { 56 } else { 32 })
                    .step_by(4)
                    .any(|at| !f32::from_bits(word(at)).is_finite())
            {
                return false;
            }
            if wmo {
                // Vanilla MODF has no supported flags/scale in this subset; raw AABBs must be
                // well formed, but never substitute for transformed source vertices.
                if u16::from_le_bytes(p[56..58].try_into().unwrap()) != 0
                    || u16::from_le_bytes(p[62..64].try_into().unwrap()) != 0
                    || (0..3).any(|i| {
                        f32::from_bits(word(32 + i * 4)) > f32::from_bits(word(44 + i * 4))
                    })
                {
                    return false;
                }
            } else if u16::from_le_bytes(p[32..34].try_into().unwrap()) == 0
                || u16::from_le_bytes(p[34..36].try_into().unwrap()) != 0
            {
                return false;
            }
        }
        let mut referenced = vec![false; placements.len() / stride];
        for r in refs {
            let Some(slot) = referenced.get_mut(r) else {
                return false;
            };
            *slot = true;
        }
        if referenced.into_iter().any(|r| !r) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::super::{
        adt_to_tile_mesh,
        native_coverage_tests::{append_first_chunk, chunk, source},
    };
    fn placed() -> Vec<u8> {
        let mut b = source();
        b.extend(chunk(b"XDMM", b"World\\Trees\\Tree.m2\0"));
        b.extend(chunk(b"DIMM", &0u32.to_le_bytes()));
        let mut p = [0u8; 36];
        p[4..8].copy_from_slice(&42u32.to_le_bytes());
        p[32..34].copy_from_slice(&1024u16.to_le_bytes());
        b.extend(chunk(b"FDDM", &p));
        let offset = append_first_chunk(&mut b, &chunk(b"FRCM", &0u32.to_le_bytes()));
        b[8 + 0x10..8 + 0x14].copy_from_slice(&1u32.to_le_bytes());
        b[8 + 0x20..8 + 0x24].copy_from_slice(&offset.to_le_bytes());
        b
    }
    #[test]
    fn native_static_coverage_accepts_complete_m2_and_keeps_legacy_subset_closed() {
        let b = placed();
        let t = adt_to_tile_mesh(&b).unwrap();
        assert!(t.native_static_coverage_safe);
        assert!(!t.native_coverage_safe);
        assert_eq!(t.doodads.len(), 1);
        assert_eq!(t.doodads[0].unique_id, 42);
        assert!(
            adt_to_tile_mesh(&source())
                .unwrap()
                .native_static_coverage_safe
        );
    }
    #[test]
    fn native_static_coverage_rejects_filtered_duplicate_and_dangling_sources() {
        let b = placed();
        for magic in [b"XDMM", b"DIMM", b"FDDM"] {
            let mut bad = b.clone();
            bad.extend(chunk(magic, &[]));
            assert!(!adt_to_tile_mesh(&bad).unwrap().native_static_coverage_safe);
        }
        let mut bad = b.clone();
        let at = bad.windows(4).rposition(|v| v == b"FDDM").unwrap() + 8;
        bad[at..at + 4].copy_from_slice(&1u32.to_le_bytes());
        assert!(adt_to_tile_mesh(&bad).unwrap().doodads.is_empty());
        assert!(!adt_to_tile_mesh(&bad).unwrap().native_static_coverage_safe);
        let mut bad = b.clone();
        let at = bad.windows(4).position(|v| v == b"FRCM").unwrap() + 8;
        bad[at..at + 4].copy_from_slice(&1u32.to_le_bytes());
        assert!(!adt_to_tile_mesh(&bad).unwrap().native_static_coverage_safe);
    }
    #[test]
    fn native_static_coverage_rejects_unsupported_or_malformed_inputs() {
        for (at, value) in [(0x10, 0u32), (0x38, 1), (0x60, 1)] {
            let mut bad = placed();
            bad[8 + at..8 + at + 4].copy_from_slice(&value.to_le_bytes());
            assert!(!adt_to_tile_mesh(&bad).unwrap().native_static_coverage_safe);
        }
        for suffix in [chunk(b"FDOM", &[0; 64]), chunk(b"O2HM", &[])] {
            let mut bad = placed();
            bad.extend(suffix);
            assert!(!adt_to_tile_mesh(&bad).unwrap().native_static_coverage_safe);
        }
        let mut bad = placed();
        let at = bad.windows(4).rposition(|v| v == b"DIMM").unwrap() + 8;
        bad[at..at + 4].copy_from_slice(&1u32.to_le_bytes());
        assert!(!adt_to_tile_mesh(&bad).unwrap().native_static_coverage_safe);
        let mut bad = placed();
        bad.push(0);
        assert!(!super::complete(&bad));
    }
}
