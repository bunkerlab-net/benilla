// SPDX-License-Identifier: GPL-3.0-or-later
//! Complete player mask, locale and service-order input calculations.
use super::*;
pub(super) fn mask_pairs(races: u32, classes: u32) -> Vec<[u32; 2]> {
    if races != 0 && races & 0xff == 0 || classes != 0 && classes & 0x5df == 0 {
        return Vec::new();
    }
    let mut result = Vec::new();
    for race in 1..=8 {
        for class in 1..=11 {
            if (races == 0 || races & (1 << (race - 1)) != 0)
                && (classes == 0 || classes & (1 << (class - 1)) != 0)
            {
                result.push([race, class]);
            }
        }
    }
    result
}
pub(super) fn skill_pairs(
    row: &Value,
    infos: &BTreeMap<u32, Vec<u32>>,
    tiers: &BTreeMap<u32, Vec<u32>>,
) -> Vec<[u32; 2]> {
    let step = row["step"].as_u64().unwrap() as usize;
    if step > 16 {
        return Vec::new();
    }
    mask_pairs(
        row["racemask"].as_u64().unwrap() as u32,
        row["classmask"].as_u64().unwrap() as u32,
    )
    .into_iter()
    .filter(|&[race, class]| {
        infos.values().any(|info| {
            info[1] == row["skill"].as_u64().unwrap() as u32
                && info[2] & (1 << (race - 1)) != 0
                && info[3] & (1 << (class - 1)) != 0
                && (step == 0 || tiers.get(&info[6]).is_some_and(|t| t[16 + step] != 0))
        })
    })
    .collect()
}
pub(super) fn locale_state(raw: Option<&[u8]>) -> &'static str {
    match raw {
        None => "null:fallback",
        Some([]) => "empty:fallback",
        Some([0, ..]) => "nul_prefix:fallback",
        _ => "override_input",
    }
}
pub(super) fn service_order(mut rows: Vec<[u64; 3]>) -> Result<(Vec<[u64; 3]>, Vec<Value>)> {
    check(rows.len() * 128 <= 8_388_608, "content.view_limit")?;
    rows.sort_unstable_by_key(|r| (r[1], r[2]));
    let mut by_key = BTreeMap::<(u64, u64), Vec<u64>>::new();
    for &[entry, slot, ordinal] in &rows {
        by_key.entry((entry, slot)).or_default().push(ordinal);
    }
    let ties = by_key
        .into_iter()
        .filter(|(_, ordinals)| ordinals.len() > 1)
        .map(|((entry, slot), ordinals)| json!([entry, slot, ordinals]))
        .collect();
    Ok((rows, ties))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_masks_and_zero_wildcard_survive() {
        assert_eq!(mask_pairs(0, 0).len(), 88);
        assert_eq!(mask_pairs(0x80, 0x400), vec![[8, 11]]);
        assert!(mask_pairs(0x100, 1).is_empty());
        assert_eq!(mask_pairs(0x101, 1), vec![[1, 1]]);
    }
    #[test]
    fn locale_and_slot_ties_do_not_invent_source_order() {
        assert_eq!(locale_state(None), "null:fallback");
        assert_eq!(locale_state(Some(b"")), "empty:fallback");
        assert_eq!(locale_state(Some(b"\0tail")), "nul_prefix:fallback");
        let (ordered, ties) =
            service_order(vec![[1, 2, 0], [1, 1, 1], [1, 1, 2], [2, 1, 3]]).unwrap();
        assert_eq!(ordered, vec![[1, 1, 1], [1, 1, 2], [2, 1, 3], [1, 2, 0]]);
        assert_eq!(ties, vec![json!([1, 1, [1, 2]])]);
    }
}
