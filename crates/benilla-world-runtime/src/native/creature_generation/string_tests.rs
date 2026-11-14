// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored string catalogs and real generation owners; no imported roster.
use super::*;

fn source() -> CreatureGenerationSource {
    let mut source = super::super::synthetic_creature_generation_fixture::source(2, 1, 1);
    source.input.template.string_id1 = 31;
    source.input.template.string_id2 = 7;
    source.input.string_ids = init::Rows::Complete(vec![
        init::StringIdRow {
            id: 31,
            name: "same".into(),
        },
        init::StringIdRow {
            id: 7,
            name: "same".into(),
        },
        init::StringIdRow {
            id: 31,
            name: "later duplicate".into(),
        },
    ]);
    source
}
fn catalog(source: CreatureGenerationSource) -> CreatureGenerationCatalog {
    CreatureGenerationCatalog::validate_sources(init::Rows::Complete(vec![source]), vec![2])
        .unwrap()
}
fn initialized(catalog: &CreatureGenerationCatalog) -> CreatureGenerationOwner {
    catalog
        .initial(
            [1; 16],
            [2; 32],
            crate::NpcId {
                spawn: 2,
                generation: 0,
            },
            42,
            &mut 0,
        )
        .unwrap()
        .0
}

#[test]
fn raw_string_catalog_names_order_and_unknown_ids_bind_generation_identity() {
    let original = source();
    let original_owner = initialized(&catalog(original.clone()));
    assert_eq!(original_owner.result().effective_string_ids, [31, 7]);
    for mutation in 0..3 {
        let mut changed = original.clone();
        let init::Rows::Complete(rows) = &mut changed.input.string_ids else {
            unreachable!()
        };
        match mutation {
            0 => rows[0].name = "改名".into(),
            1 => rows.reverse(),
            2 => rows.push(init::StringIdRow {
                id: 99,
                name: String::new(),
            }),
            _ => unreachable!(),
        }
        assert_ne!(changed.identity(), original.identity());
        let owner = initialized(&catalog(changed));
        assert_eq!(owner.result(), original_owner.result());
        assert_ne!(owner.source_identity(), original_owner.source_identity());
    }
    let mut unknown = original.clone();
    unknown.input.template.string_id2 = 77;
    let mut zero = unknown.clone();
    zero.input.template.string_id2 = 0;
    assert_ne!(unknown.identity(), zero.identity());
    assert_eq!(
        initialized(&catalog(unknown)).result(),
        initialized(&catalog(zero)).result()
    );
}

#[test]
fn missing_or_conflicting_complete_cold_string_catalogs_fail_sealing() {
    let mut missing = source();
    missing.input.template.string_id1 = 0;
    missing.input.template.string_id2 = 0;
    missing.input.string_ids = init::Rows::Missing;
    assert!(CreatureGenerationCatalog::validate_sources(
        init::Rows::Complete(vec![missing]),
        vec![2]
    )
    .is_err());

    let first = source();
    let mut second = first.clone();
    second.spawn_id = 3;
    second.input.spawn_id = 3;
    second.spawn_entry += 1;
    second.input.template.entry += 1;
    assert!(CreatureGenerationCatalog::validate_sources(
        init::Rows::Complete(vec![first.clone(), second.clone()]),
        vec![2, 3]
    )
    .is_ok());
    let init::Rows::Complete(rows) = &mut second.input.string_ids else {
        unreachable!()
    };
    // Even equivalent membership is not the same retained cold catalog.
    rows.reverse();
    assert!(CreatureGenerationCatalog::validate_sources(
        init::Rows::Complete(vec![first, second]),
        vec![2, 3]
    )
    .is_err());
}

#[test]
fn changed_raw_string_identity_cannot_restore_an_old_world_checkpoint() {
    use super::super::synthetic_creature_generation_fixture::install;
    let first = source();
    let profile = install(
        synthetic_fixture::initialized_profile(),
        vec![first.clone()],
        vec![2],
    );
    let world = NativeWorld::synthetic_component_test(
        profile.clone(),
        [8; 16],
        synthetic_fixture::player(),
        42,
    )
    .unwrap();
    let checkpoint = world.checkpoint().unwrap();
    assert!(NativeWorld::restore_synthetic_component_test(profile, checkpoint.clone()).is_ok());
    let mut changed = first;
    let init::Rows::Complete(rows) = &mut changed.input.string_ids else {
        unreachable!()
    };
    rows[0].name.push_str(" changed retained content");
    let profile = install(
        synthetic_fixture::initialized_profile(),
        vec![changed],
        vec![2],
    );
    assert!(NativeWorld::restore_synthetic_component_test(profile, checkpoint).is_err());
}
