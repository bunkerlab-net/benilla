// SPDX-License-Identifier: GPL-3.0-or-later
//! Checked numeric starter draft. No hook seal, runtime profile or capability.
use super::*;
use benilla_sim::{reputation as rep, starter_init as init};

#[derive(Debug)]
pub(super) enum StarterDraftError {
    Input(super::super::ProductionDiagnostic),
    Reducer(init::InitError),
    Reputation(rep::Error),
}
impl From<super::super::ProductionDiagnostic> for StarterDraftError {
    fn from(error: super::super::ProductionDiagnostic) -> Self {
        Self::Input(error)
    }
}
impl std::fmt::Display for StarterDraftError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Input(error) => error.fmt(f),
            Self::Reducer(error) => error.fmt(f),
            Self::Reputation(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for StarterDraftError {}
type DraftResult<T> = std::result::Result<T, StarterDraftError>;
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum StarterDraftRefusal {
    PendingReachedHookBinding {
        learned_spell: u32,
        casts: bool,
        retained_holder: bool,
    },
    PendingSourceSeal,
}

/// The builder retains the exact guarded input for rank 2 to rebuild and seal.
/// Projected state is conditional, never an effective runtime character.
#[derive(Clone, Debug)]
pub(super) struct StarterDraft {
    pub input: init::StarterInput,
    pub projection: init::UnsealedStarterDraft,
    pub initial_reputation: Vec<rep::FactionState>,
    pub refusals: Vec<StarterDraftRefusal>,
}

fn words<const N: usize>(row: Row<'_>) -> [u32; N] {
    std::array::from_fn(|i| row.word(i))
}
fn require_table<'a>(c: &'a Compiler<'_>, name: &str) -> Result<&'a dbc::Table> {
    c.dbcs
        .get(name)
        .ok_or_else(|| fail("starter_draft.dbc_required"))
}
fn sim<T>(result: init::Result<T>) -> DraftResult<T> {
    result.map_err(StarterDraftError::Reducer)
}

pub(super) fn build(
    c: &Compiler<'_>,
    candidate: &init::CapturedStarterLearning,
) -> DraftResult<StarterDraft> {
    check(c.source_sha.len() == 64, "starter_draft.source_identity")?;
    let mut identity = [0; 32];
    for (i, byte) in identity.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&c.source_sha[i * 2..i * 2 + 2], 16)
            .map_err(|_| fail("starter_draft.source_identity"))?;
    }
    check(
        candidate.source_identity == identity,
        "starter_draft.candidate_source_binding",
    )?;
    let learning = sim(candidate.compile(c.request.race, c.request.class))?;
    let attributes = rows(c.source, "player_levelstats")
        .iter()
        .find(|r| int(&r["level"]) == 1)
        .ok_or_else(|| fail("starter_draft.level_one_attributes_required"))?;
    let class = rows(c.source, "player_classlevelstats")
        .iter()
        .find(|r| int(&r["level"]) == 1)
        .ok_or_else(|| fail("starter_draft.level_one_resources_required"))?;
    let class_dbc = require_table(c, "ChrClasses")?
        .get(u32::from(c.request.class))
        .ok_or_else(|| fail("starter_draft.class_required"))?;
    let input = init::StarterInput {
        race: c.request.race,
        class: c.request.class,
        gender: c.request.gender,
        level: 1,
        class_power_type: class_dbc.word(3),
        create_attributes: init::Attributes(
            ["str", "agi", "sta", "inte", "spi"].map(|key| int(&attributes[key]) as i32),
        ),
        create_health: int(&class["basehp"]) as u32,
        create_mana: int(&class["basemana"]) as u32,
        default_skills: init::Rows::Missing,
        default_spells: init::Rows::Missing,
        skill_lines: vec![],
        skill_race_class: init::Rows::Missing,
        skill_tiers: vec![],
        skill_abilities: init::Rows::Missing,
        spells: vec![],
        // Availability remains separate from learning and initial casting. No
        // executor is admitted by this internal draft, including Fireball.
        active_support: vec![],
        item_grants: init::Rows::Missing,
        items: vec![],
        item_creation_hooks: false,
    };
    let input = sim(learning.populate(input, sim(learning.pending_spell_rows())?))?;
    let input = starting_items(c, input)?;
    let numeric = input
        .spells
        .iter()
        .map(|row| dependency(c, row))
        .collect::<DraftResult<Vec<_>>>()?;
    let stores: BTreeMap<_, _> = c
        .dbcs
        .iter()
        .map(|(name, table)| (name, &table.sha256))
        .collect();
    let store_identity = crate::digest(b"benilla-unsealed-starter-dbc-stores-v1\0", &stores);
    let projection = sim(init::draft_starter(&input, store_identity, numeric))?;
    let reached: BTreeSet<_> = projection
        .reached_dependencies()
        .iter()
        .map(|row| row.spell_id)
        .collect();
    let holders: BTreeSet<_> = projection
        .projected_creation()
        .initial_auras
        .iter()
        .map(|aura| aura.spell_id)
        .collect();
    let mut refusals: Vec<_> = projection
        .pending_learning_hooks()
        .iter()
        .map(|&id| StarterDraftRefusal::PendingReachedHookBinding {
            learned_spell: id,
            casts: reached.contains(&id),
            retained_holder: holders.contains(&id),
        })
        .collect();
    refusals.push(StarterDraftRefusal::PendingSourceSeal);
    let initial_reputation = reputation(c)?;
    Ok(StarterDraft {
        input,
        projection,
        initial_reputation,
        refusals,
    })
}

fn starting_items(c: &Compiler<'_>, input: init::StarterInput) -> DraftResult<init::StarterInput> {
    let packed =
        u32::from(input.race) | u32::from(input.class) << 8 | u32::from(input.gender) << 16;
    let outfits: Vec<_> = require_table(c, "CharStartOutfit")?
        .by_id
        .keys()
        .filter_map(|id| c.dbcs["CharStartOutfit"].get(*id))
        .filter(|row| row.word(0) != 0 && row.word(1) == packed)
        .map(|row| init::CharStartOutfitRow {
            id: row.word(0),
            race_class_gender: row.word(1),
            items: std::array::from_fn(|i| row.word(2 + i) as i32),
        })
        .collect();
    let custom: Vec<_> = canonical_order::collection(c.source, "playercreateinfo_item")
        .into_iter()
        .map(|row| init::CustomStartingItem {
            race: int(&row["race"]) as u8,
            class: int(&row["class"]) as u8,
            item: int(&row["itemid"]) as u32,
            amount: int(&row["amount"]) as u32,
        })
        .collect();
    let selected: BTreeSet<_> = outfits
        .iter()
        .flat_map(|row| row.items)
        .filter(|id| *id > 0)
        .map(|id| id as u32)
        .chain(custom.iter().map(|row| row.item))
        .collect();
    let items = selected
        .into_iter()
        .map(|id| {
            let raw = rows(c.source, "item_template")
                .iter()
                .find(|row| int(&row["entry"]) == i128::from(id))
                .ok_or_else(|| fail("starter_draft.starting_item_required"))?;
            let plan = plans::item::item_plan(raw);
            check(
                plan["starter_placement"]["status"] == "supported_shape",
                "starter_draft.starting_item_unsupported",
            )?;
            let item: init::ItemRow =
                serde_json::from_value(plan["captured_input"]["placement"].clone())
                    .map_err(|_| fail("starter_draft.item_adapter"))?;
            Ok(init::StartingItemRow {
                item,
                buy_count: int(&raw["BuyCount"]) as u32,
                first_spell_category: int(&raw["spellcategory_1"]) as u32,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    sim(init::CapturedStarterItems {
        outfits: init::Rows::Complete(outfits),
        custom: init::Rows::Complete(custom),
        items,
    }
    .compile(input))
}
fn dependency(
    c: &Compiler<'_>,
    row: &init::SpellRow,
) -> DraftResult<init::StarterInitialDependency> {
    let raw = c.source["spell_numeric_capture"]["rows"]
        .as_array()
        .and_then(|rows| {
            rows.iter()
                .find(|raw| int(&raw["Id"]) == i128::from(row.id))
        })
        .ok_or_else(|| fail("starter_draft.numeric_row_required"))?;
    let cast_time =
        c.dbcs
            .get("SpellCastTimes")
            .map_or(init::InitialSpellLookup::Unknown, |table| {
                table.get(row.words[18]).map_or(
                    init::InitialSpellLookup::Missing {
                        index: row.words[18],
                    },
                    |r| {
                        init::InitialSpellLookup::Found(init::SpellCastTimeSource {
                            id: r.word(0),
                            base_ms: r.word(1) as i32,
                            per_level_ms: r.word(2) as i32,
                            minimum_ms: r.word(3) as i32,
                        })
                    },
                )
            });
    let duration = c
        .dbcs
        .get("SpellDuration")
        .map_or(init::InitialSpellLookup::Unknown, |table| {
            table.get(row.words[30]).map_or(
                init::InitialSpellLookup::Missing {
                    index: row.words[30],
                },
                |r| {
                    init::InitialSpellLookup::Found(init::SpellDurationSource {
                        id: r.word(0),
                        base_ms: r.word(1) as i32,
                        per_level_ms: r.word(2) as i32,
                        maximum_ms: r.word(3) as i32,
                    })
                },
            )
        });
    let coefficients = |fields: [&str; 3]| -> Result<[u32; 3]> {
        let mut values = [0; 3];
        for (value, field) in values.iter_mut().zip(fields) {
            *value = dbc::binary32(raw[field]["decimal"].as_str().unwrap())?;
        }
        Ok(values)
    };
    Ok(init::StarterInitialDependency {
        spell_id: row.id,
        source_hash: row.source_hash(),
        cast_time,
        duration,
        server_side: int(&raw["IsServerSide"]) as u32,
        server_attributes: int(&raw["AttributesServerside"]) as u32,
        bonus_coefficients: coefficients([
            "EffectBonusCoefficient1",
            "EffectBonusCoefficient2",
            "EffectBonusCoefficient3",
        ])?,
        attack_power_coefficients: coefficients([
            "EffectBonusCoefficientFromAP1",
            "EffectBonusCoefficientFromAP2",
            "EffectBonusCoefficientFromAP3",
        ])?,
    })
}
fn reputation(c: &Compiler<'_>) -> DraftResult<Vec<rep::FactionState>> {
    let race = require_table(c, "ChrRaces")?
        .get(u32::from(c.request.race))
        .ok_or_else(|| fail("starter_draft.race_required"))?;
    let required: BTreeSet<_> = ids(c.source, "creature_template", "Faction")
        .into_iter()
        .map(|id| id as u32)
        .chain([race.word(2)])
        .collect();
    let template = require_table(c, "FactionTemplate")?;
    let capture = rep::InitialFactionCapture {
        factions: Some(
            require_table(c, "Faction")?
                .by_id
                .keys()
                .filter_map(|id| c.dbcs["Faction"].get(*id))
                .map(words)
                .collect(),
        ),
        templates: required
            .iter()
            .filter_map(|id| template.get(*id))
            .map(words)
            .collect(),
        required_templates: required.into_iter().collect(),
        required_factions: vec![],
        race: c.request.race,
        class: c.request.class,
    };
    capture.initialize().map_err(StarterDraftError::Reputation)
}

#[cfg(test)]
#[path = "starter_draft_tests.rs"]
mod tests;
