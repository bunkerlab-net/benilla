// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

fn row(id: u32, gender: u32, other: u32, alternative: u32) -> LoadedCreatureModelInfo {
    SqlTemplateRow {
        ordinal: 0,
        words: [
            id,
            0,
            1.5f32.to_bits(),
            1f32.to_bits(),
            1.14286f32.to_bits(),
            gender,
            other,
            alternative,
        ],
        strings: [],
    }
}
#[test]
fn model_loader_keeps_log_only_rows_and_applies_gender_and_reference_fixups() {
    // ObjectMgr.cpp:1646-1679 logs missing display but fixes gender and invalid links.
    let models = LoadedCreatureModels::load(
        vec![row(10, 3, 10, 99), row(11, 2, 12, 10)],
        &BTreeSet::from([11, 12]),
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(models.get(10).unwrap().words[5..], [0, 0, 0]);
    assert_eq!(models.get(11).unwrap().words[5..], [2, 12, 0]);
    assert_eq!(
        models.notices,
        vec![(10, 1647), (10, 1652), (10, 1660), (10, 1679), (11, 1679)]
    );
}
#[test]
fn model_loader_race_checks_fix_only_nonpositive_radius_in_female_then_male_order() {
    // ObjectMgr.cpp:1694-1726 leaves wrong gender/other model unchanged and defaults radius.
    let mut female = row(10, 0, 0, 0);
    female.words[1] = (-1f32).to_bits();
    let mut male = row(11, 1, 0, 0);
    male.words[1] = f32::NAN.to_bits();
    let models = LoadedCreatureModels::load(
        vec![female, male],
        &BTreeSet::from([10, 11]),
        &BTreeMap::from([(1, [11, 10])]),
    )
    .unwrap();
    assert_eq!(models.get(10).unwrap().words[1], 0.389f32.to_bits());
    assert!(f32::from_bits(models.get(11).unwrap().words[1]).is_nan());
    assert_eq!(
        models.notices,
        vec![(10, 1697), (10, 1700), (10, 1705), (11, 1714), (11, 1717)]
    );
}
#[test]
fn model_loader_guards_source_index_and_race_shift_on_both_sides() {
    // SQLStorageImpl.h:317 wraps MAX(modelid)+1; ObjectMgr.cpp:1691 shifts race-1.
    LoadedCreatureModels::load(
        vec![row(u32::MAX - 1, 0, 0, 0)],
        &BTreeSet::new(),
        &BTreeMap::from([(32, [0, 0])]),
    )
    .unwrap();
    assert_eq!(
        LoadedCreatureModels::load(
            vec![row(u32::MAX, 0, 0, 0)],
            &BTreeSet::new(),
            &BTreeMap::new()
        )
        .unwrap_err()
        .id,
        "execution.creature_models.SQLStorageImpl_317.max_entry_index_undefined"
    );
    assert_eq!(
        LoadedCreatureModels::load(
            Vec::new(),
            &BTreeSet::new(),
            &BTreeMap::from([(33, [0, 0])])
        )
        .unwrap_err()
        .id,
        "execution.creature_models.ObjectMgr_1691.race_shift_undefined"
    );
}
#[test]
fn random_gender_uses_calling_stream_and_keeps_original_when_alternate_row_is_absent() {
    // ObjectMgr.cpp:1617-1632 draws only with a nonzero other gender, absent alternate is nonfatal.
    use benilla_sim::source_rng::{DrawCursor, Mt19937, SourceRandom};
    let models = LoadedCreatureModels::load(
        vec![row(10, 0, 11, 0)],
        &BTreeSet::from([10, 11]),
        &BTreeMap::new(),
    )
    .unwrap();
    let mut engine = Mt19937::from_seed(1);
    let mut cursor = DrawCursor::default();
    let mut sink = |_| {};
    let mut random = SourceRandom::new(7, &mut engine, &mut cursor, &mut sink).unwrap();
    assert_eq!(models.random_gender(0, &mut random).unwrap(), None);
    assert_eq!(random.cursor().raw, 0);
    assert_eq!(
        models
            .random_gender(10, &mut random)
            .unwrap()
            .unwrap()
            .words[0],
        10
    );
    assert_eq!(random.cursor().raw, 1);
}

#[test]
fn random_gender_switches_on_zero_keeps_on_one_and_does_not_draw_without_alternate() {
    // ObjectMgr.cpp:1622-1632 uses exactly urand(0,1)==0, with one nonrecursive lookup.
    use benilla_sim::source_rng::{DrawCursor, Mt19937, SourceRandom};
    let models = LoadedCreatureModels::load(
        vec![row(10, 0, 11, 0), row(11, 1, 0, 0)],
        &BTreeSet::from([10, 11]),
        &BTreeMap::new(),
    )
    .unwrap();
    let mut engine = Mt19937::from_seed(1);
    let mut cursor = DrawCursor::default();
    let mut sink = |_| {};
    let mut random = SourceRandom::new(7, &mut engine, &mut cursor, &mut sink).unwrap();
    assert_eq!(
        models
            .random_gender(10, &mut random)
            .unwrap()
            .unwrap()
            .words[0],
        11
    );
    assert_eq!(
        models
            .random_gender(10, &mut random)
            .unwrap()
            .unwrap()
            .words[0],
        10
    );
    assert_eq!(random.cursor().raw, 2);
    assert_eq!(
        models
            .random_gender(11, &mut random)
            .unwrap()
            .unwrap()
            .words[0],
        11
    );
    assert_eq!(random.cursor().raw, 2);
}
