// SPDX-License-Identifier: GPL-3.0-or-later
//! Framing checks for the captured VMAP_7.0 tree and group bytes.
use super::*;

pub(super) fn bytes(data: &[u8], at: usize, count: usize) -> Result<&[u8]> {
    data.get(
        at..at
            .checked_add(count)
            .ok_or_else(|| error("binary_truncated"))?,
    )
    .ok_or_else(|| error("binary_truncated"))
}
pub(super) fn word(data: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(bytes(data, at, 4)?.try_into().unwrap()))
}
pub(super) fn vector<const N: usize>(data: &[u8], at: usize) -> Result<[f32; N]> {
    let mut out = [0.0; N];
    for (i, value) in out.iter_mut().enumerate() {
        *value = f32::from_bits(word(data, at + i * 4)?);
        require(value.is_finite(), "nonfinite")?;
    }
    Ok(out)
}
pub(super) fn bound(data: &[u8], at: usize) -> Result<[f32; 6]> {
    let out = vector(data, at)?;
    require((0..3).all(|i| out[i] <= out[i + 3]), "bounds")?;
    Ok(out)
}
pub(super) fn contains(bound: [f32; 6], point: [f32; 3]) -> bool {
    (0..3).all(|i| bound[i] <= point[i] && point[i] <= bound[i + 3])
}

pub(super) fn bih(data: &[u8], mut at: usize) -> Result<(usize, usize)> {
    bound(data, at)?;
    at += 24;
    let count = word(data, at)? as usize;
    require(
        (3..=MAX_WORDS).contains(&count) && count.is_multiple_of(3),
        "bih_count",
    )?;
    at += 4;
    let tree = bytes(data, at, count * 4)?;
    at += tree.len();
    let count = word(data, at)? as usize;
    require(count <= MAX_WORDS, "bih_count")?;
    at += 4;
    let objects = bytes(data, at, count * 4)?;
    at += objects.len();
    let mut slots = vec![false; count];
    for object in objects.as_chunks::<4>().0 {
        let slot = u32::from_le_bytes(*object) as usize;
        require(slot < count && !slots[slot], "bih_objects")?;
        slots[slot] = true;
    }
    slots.fill(false);
    let mut nodes = vec![false; tree.len() / 12];
    let mut pending = vec![(0usize, 0usize)];
    while let Some((node, depth)) = pending.pop() {
        require(
            depth <= 63 && node.is_multiple_of(3) && node + 2 < tree.len() / 4 && !nodes[node / 3],
            "bih_structure",
        )?;
        nodes[node / 3] = true;
        let value = word(tree, node * 4)?;
        let axis = value >> 30;
        let offset = (value & 0x1fff_ffff) as usize;
        let low = word(tree, (node + 1) * 4)?;
        let high = word(tree, (node + 2) * 4)?;
        if axis == 3 {
            let end = offset + low as usize;
            require(
                value & (1 << 29) == 0 && high == 0 && end <= count,
                "bih_leaf",
            )?;
            for slot in &mut slots[offset..end] {
                require(!*slot, "bih_leaf")?;
                *slot = true;
            }
        } else {
            let (low, high) = (f32::from_bits(low), f32::from_bits(high));
            if value & (1 << 29) != 0 {
                require(
                    low.is_finite() && high.is_finite() && low <= high,
                    "bih_clip",
                )?;
                pending.push((offset, depth + 1));
            } else {
                let (left, right) = (low != f32::NEG_INFINITY, high != f32::INFINITY);
                require(
                    (left || right) && (!left || low.is_finite()) && (!right || high.is_finite()),
                    "bih_clip",
                )?;
                if right {
                    pending.push((offset + 3, depth + 1));
                }
                if left {
                    pending.push((offset, depth + 1));
                }
            }
        }
    }
    require(
        nodes.iter().all(|v| *v) && slots.iter().all(|v| *v),
        "bih_coverage",
    )?;
    Ok((at, count))
}
