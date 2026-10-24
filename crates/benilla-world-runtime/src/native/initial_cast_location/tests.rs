// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored DBC/pose evidence; public SQL rows are not a captured client location.
use super::*;
fn dbc<const N: usize>(rows: &[[u32; N]]) -> Vec<u8> {
    let mut b = b"WDBC".to_vec();
    for n in [rows.len() as u32, N as u32, (N * 4) as u32, 1] {
        b.extend(n.to_le_bytes());
    }
    for row in rows {
        for n in row {
            b.extend(n.to_le_bytes());
        }
    }
    b.push(0);
    b
}
fn catalog(map: u32) -> SourceLocationCatalog {
    let mut area = [0; 25];
    area[0] = 9;
    area[1] = map;
    area[2] = 12;
    area[3] = 7;
    let mut m = [0; 42];
    m[0] = map;
    m[19] = 9;
    SourceLocationCatalog::decode(&dbc(&[area]), &dbc::<20>(&[]), &dbc(&[m])).unwrap()
}
fn public_rows() -> Vec<SpellAreaSourceRow> {
    // Pinned ClassicDB base + default-install audit, all ten raw columns retained.
    let mut rows = vec![SpellAreaSourceRow {
        spell: 6298,
        area: 148,
        quest_start: 0,
        quest_start_active: 0,
        quest_end: 0,
        condition_id: 0,
        aura_spell: 0,
        racemask: 0,
        gender: 2,
        autocast: 0,
    }];
    rows.extend(
        [139, 2057, 2017, 3456]
            .into_iter()
            .map(|area| SpellAreaSourceRow {
                spell: 31906,
                area,
                quest_start: 0,
                quest_start_active: 0,
                quest_end: 0,
                condition_id: 1399,
                aura_spell: 30238,
                racemask: 0,
                gender: 0,
                autocast: 1,
            }),
    );
    rows
}
fn profile(rows: Vec<SpellAreaSourceRow>) -> Profile {
    let (mut p, input, prepared, mut source) = synthetic_initial_cast_fixture::inputs();
    source.hooks.spell_area = SpellAreaSourceRows::CompleteTable(rows);
    let c = catalog(p.map_id);
    source.entry_location = Some(
        InitialCastEntryLocationSource::new(
            SourceLocationSnapshotBinding {
                map: p.map_id,
                content: p.content_digest,
                source_epoch: [31; 32],
                inventory: [32; 32],
                policy: [33; 32],
                catalog: c.digest(),
            },
            c,
        )
        .unwrap(),
    );
    let mut numeric = prepared.dependencies().clone();
    numeric.hook_scope_identity = source.hook_scope_identity();
    let prepared =
        benilla_sim::starter_init::prepare_starter(&input, &numeric, prepared.order()).unwrap();
    p.initialize_player_prepared(&input, &prepared, synthetic_fixture::float(100.), 1)
        .unwrap();
    p.initialize_initial_cast_source(&input, source).unwrap();
    p
}
fn facts(p: &Profile) -> InitialCastEntryLocationFacts {
    let q = InitialCastEntryLocationQuery::new(p, [8; 16], synthetic_fixture::player()).unwrap();
    InitialCastEntryLocationFacts::new(&q, &catalog(p.map_id), 7).unwrap()
}
#[test]
fn public_nonempty_table_is_entry_scoped_and_restore_preserves_draw_baseline() {
    let p = profile(public_rows());
    let facts = facts(&p);
    assert_eq!((facts.zone_id(), facts.area_id()), (12, 9));
    assert!(NativeWorld::synthetic_component_test(
        p.clone(),
        [8; 16],
        synthetic_fixture::player(),
        7
    )
    .is_err());
    let world = NativeWorld::synthetic_component_test_with_initial_location(
        p.clone(),
        [8; 16],
        synthetic_fixture::player(),
        7,
        Some(facts),
    )
    .unwrap();
    assert_eq!(world.state().rng_counter, 1);
    let checkpoint = world.checkpoint().unwrap();
    let restored = NativeWorld::restore_synthetic_component_test(
        p,
        Checkpoint::decode(&checkpoint.encode().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(restored.checkpoint().unwrap(), checkpoint);
}
#[test]
fn any_reachable_raw_area_spell_or_aura_row_remains_closed() {
    for (spell, area, aura) in [(6298, 12, 0), (6298, 9, 0), (81, 148, 0), (6298, 148, -81)] {
        let mut rows = public_rows();
        rows[0].spell = spell;
        rows[0].area = area;
        rows[0].aura_spell = aura;
        let p = profile(rows);
        let facts = facts(&p);
        assert!(NativeWorld::synthetic_component_test_with_initial_location(
            p,
            [8; 16],
            synthetic_fixture::player(),
            7,
            Some(facts)
        )
        .is_err());
    }
    let mut rows = public_rows();
    rows[0].area = 0;
    let p = profile(rows);
    let f = facts(&p);
    NativeWorld::synthetic_component_test_with_initial_location(
        p,
        [8; 16],
        synthetic_fixture::player(),
        7,
        Some(f),
    )
    .unwrap();
}
#[test]
fn entry_witness_cannot_move_across_pose_player_world_scope_or_source() {
    let p = profile(public_rows());
    let f = facts(&p);
    for field in 0..8 {
        let mut changed = f.clone();
        match field {
            0 => changed.query.world[0] ^= 1,
            1 => changed.query.player.owner[0] ^= 1,
            2 => changed.query.profile[0] ^= 1,
            3 => changed.query.pose.position.x = ai::Real::new(1.).unwrap(),
            4 => changed.query.scope[0] ^= 1,
            5 => changed.query.source[0] ^= 1,
            6 => changed.query.snapshot.inventory[0] ^= 1,
            _ => changed.area.as_mut().unwrap().zone = 13,
        }
        assert!(NativeWorld::synthetic_component_test_with_initial_location(
            p.clone(),
            [8; 16],
            synthetic_fixture::player(),
            7,
            Some(changed)
        )
        .is_err());
    }
    let json = serde_json::to_value(&f).unwrap();
    let mut missing = json;
    missing.as_object_mut().unwrap().remove("area");
    assert!(serde_json::from_value::<InitialCastEntryLocationFacts>(missing).is_err());
}

#[test]
fn later_location_catalog_must_share_the_frozen_entry_snapshot() {
    let (p, input, mut spells) = synthetic_spell_fixture::inputs();
    let (mut p, input, _, mut source) = synthetic_initial_cast_fixture::prepare(p, input);
    let c = catalog(p.map_id);
    let snapshot = SourceLocationSnapshotBinding {
        map: p.map_id,
        content: p.content_digest,
        source_epoch: [31; 32],
        inventory: [32; 32],
        policy: [33; 32],
        catalog: c.digest(),
    };
    source.entry_location = Some(InitialCastEntryLocationSource::new(snapshot, c.clone()).unwrap());
    spells.initial_cast_dependencies = source.dependencies.clone();
    spells.modifier_spell_ids = SpellSourceRows::Complete(vec![81, 133, 3050]);
    p.initialize_initial_cast_source(&input, source).unwrap();
    p.initialize_spell_authority(&input, spells).unwrap();
    let original = p.clone();
    for field in 0..3 {
        let mut wrong = snapshot;
        match field {
            0 => wrong.source_epoch[0] ^= 1,
            1 => wrong.inventory[0] ^= 1,
            _ => wrong.policy[0] ^= 1,
        }
        assert_eq!(
            p.initialize_source_location(wrong, c.clone()),
            Err(NativeError::AuthorityBinding)
        );
        assert_eq!(p, original);
    }
    p.initialize_source_location(snapshot, c).unwrap();
    let f = facts(&p);
    let w = NativeWorld::synthetic_component_test_with_initial_location(
        p,
        [8; 16],
        synthetic_fixture::player(),
        7,
        Some(f),
    )
    .unwrap();
    NativeWorld::restore_synthetic_component_test(w.profile().clone(), w.checkpoint().unwrap())
        .unwrap();
}
