// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Project contributors.
// Creature.cpp:2296-2345; ObjectMgr.cpp:589-719. Upstream GPL-2.0-or-later;
// Rust translation GPL-3.0-or-later. See ../../CREATURE-INIT.md.
use super::*;

/// Typed raw addon payload, before native loader repairs. Aura IDs retain input
/// order and duplicates; any nonempty list is unsupported, including zero IDs.
/// Parsing the SQL text and retaining capture completeness belong to the owner.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Addon {
    pub mount: u32,
    pub stand_state: u32,
    pub sheath_state: u32,
    pub emote: u32,
    /// Retained source column; pinned LoadCreatureAddon never reads it.
    pub move_flags: u32,
    pub auras: Vec<u32>,
}
/// Raw creature_addon row from the exact database-GUID query.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct SpawnAddonRow {
    pub guid: u64,
    pub addon: Addon,
}
/// Raw creature_template_addon row from the exact creature-entry query.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct TemplateAddonRow {
    pub entry: u32,
    pub addon: Addon,
}

/// Called only after aggregate outer row counts have passed the retention cap.
/// Reading each vector length does not traverse any unbounded aura payload.
pub(super) fn retained_aura_ids(input: &Input) -> Result<usize> {
    let spawn = match &input.spawn_addons {
        Rows::Missing => &[][..],
        Rows::Complete(rows) => rows.as_slice(),
    };
    let template = match &input.template_addons {
        Rows::Missing => &[][..],
        Rows::Complete(rows) => rows.as_slice(),
    };
    spawn
        .iter()
        .map(|row| row.addon.auras.len())
        .chain(template.iter().map(|row| row.addon.auras.len()))
        .try_fold(0usize, usize::checked_add)
        .ok_or(InitError::Invalid("source retention row bound"))
}

pub(super) fn prepare(input: &Input) -> Result<Option<u8>> {
    let guid = u32::try_from(input.spawn_id)
        .ok()
        .filter(|guid| *guid != 0)
        .ok_or(InitError::Invalid("addon spawn database GUID"))?;
    let spawn = input
        .spawn_addons
        .require("creature_addon exact GUID", guid)?;
    let template = input
        .template_addons
        .require("creature_template_addon exact entry", input.template.entry)?;
    if spawn.len() > 1 || template.len() > 1 {
        return Err(InitError::Invalid("duplicate addon row"));
    }
    for row in spawn {
        if row.guid != input.spawn_id {
            return Err(InitError::Invalid("addon spawn row identity"));
        }
        validate(&row.addon)?;
    }
    for row in template {
        if row.entry != input.template.entry {
            return Err(InitError::Invalid("addon template row identity"));
        }
        // ObjectMgr loads both tables before GetCreatureAddon selects a row.
        validate(&row.addon)?;
    }
    // GetCreatureAddon gives an existing spawn row whole-row precedence, even
    // when all its payload fields are zero. There is no per-field fallback.
    Ok(spawn
        .first()
        .map(|row| &row.addon)
        .or_else(|| template.first().map(|row| &row.addon))
        .map(|addon| addon.sheath_state as u8))
}
fn validate(addon: &Addon) -> Result<()> {
    if addon.mount != 0 {
        return Err(InitError::Unsupported("initial addon mount"));
    }
    if addon.stand_state != 0 {
        return Err(InitError::Unsupported("initial addon stand state"));
    }
    if addon.sheath_state > 2 {
        return Err(InitError::Unsupported("initial addon sheath state"));
    }
    if addon.emote != 0 {
        return Err(InitError::Unsupported("initial addon emote"));
    }
    if !addon.auras.is_empty() {
        return Err(InitError::Unsupported("initial addon auras"));
    }
    Ok(())
}

pub(super) fn apply_sheath(unit_bytes_2: u32, sheath: Option<u8>) -> u32 {
    // LoadCreatureAddon writes offset zero only; absence does not call the setter.
    sheath.map_or(unit_bytes_2, |sheath| {
        (unit_bytes_2 & !0xff) | u32::from(sheath)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_setter_preserves_upper_bytes_against_exact_source_oracle() {
        let mut count = 0;
        for line in include_str!("../../tests/fixtures/creature_init_cmangos.tsv").lines() {
            let Some(values) = line.strip_prefix("a ") else {
                continue;
            };
            let n: Vec<u32> = values
                .split_whitespace()
                .map(|v| v.parse().unwrap())
                .collect();
            assert_eq!(n.len(), 11);
            let sheath = if n[3] != 0 {
                assert_eq!(n[5], 2);
                Some(n[4] as u8)
            } else if n[1] != 0 {
                assert_eq!(n[5], 1);
                Some(n[2] as u8)
            } else {
                assert_eq!(n[5], 0);
                None
            };
            assert_eq!(apply_sheath(n[0], sheath), n[6]);
            assert_eq!(n[6] & !0xff, n[0] & !0xff);
            assert_eq!(n[7], u32::from(sheath.is_some()));
            assert_eq!(n[8], 0);
            count += 1;
        }
        assert_eq!(count, 448);
    }
}
