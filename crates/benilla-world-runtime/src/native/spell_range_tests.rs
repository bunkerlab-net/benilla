// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored source-domain fixtures; no native content or entry certification.
use super::{synthetic_fixture as f, synthetic_spell_fixture as fixture, *};

fn world(p: Profile) -> NativeWorld {
    NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 7).unwrap()
}
fn ranges(w: &NativeWorld) -> Vec<(&SourceSpellRequest, SpellRangeResult)> {
    w.records()
        .iter()
        .flat_map(|r| &r.replies)
        .filter_map(|reply| match (&reply.request.query, &reply.facts) {
            (
                Query::SourceSpell(SourceSpellQuery::Range { context, .. }),
                Facts::SourceSpell(SourceSpellFacts::Range(result)),
            ) => Some((context, *result)),
            _ => None,
        })
        .collect()
}

#[test]
fn range_is_derived_only_at_prepare_and_release_and_observed_in_receipt_order() {
    let mut w = world(fixture::profile());
    let mut a = fixture::spatial_only();
    let input = f::cast(&w, 133);
    w.step(1, Some(input), &mut a).unwrap();
    for tick in 2..=40 {
        w.step(tick, None, &mut a).unwrap();
    }
    let range = ranges(&w);
    assert_eq!(range.len(), 2);
    assert_eq!(range[0].0.phase, SourceSpellPhase::Prepare);
    assert_eq!(range[1].0.phase, SourceSpellPhase::ReleaseCheck);
    assert!(range.iter().all(|(_, r)| *r == SpellRangeResult::Within));
    assert!(a
        .queries
        .iter()
        .all(|r| !matches!(r.query, Query::SourceSpell(SourceSpellQuery::Range { .. }))));
    let recorded: Vec<_> = w
        .records()
        .iter()
        .flat_map(|r| &r.replies)
        .filter(|r| {
            matches!(
                r.request.query,
                Query::SourceSpell(SourceSpellQuery::Range { .. })
            )
        })
        .collect();
    let observed: Vec<_> = a
        .derived
        .iter()
        .filter(|r| {
            matches!(
                r.request.query,
                Query::SourceSpell(SourceSpellQuery::Range { .. })
            )
        })
        .collect();
    assert_eq!(recorded, observed);
}

#[test]
fn raw_range_row_and_numeric_policy_are_profile_bound() {
    let (mut p, initial, source) = fixture::inputs();
    p.initialize_spell_authority(&initial, source.clone())
        .unwrap();
    let original = p.digest();
    let value = serde_json::to_value(&p).unwrap();
    assert_eq!(
        value["interactions"]["spell_authority"]["range_policy"]["algorithm"],
        benilla_sim::spell_range::POLICY
    );
    assert_eq!(
        value["interactions"]["spell_authority"]["range_policy"]["source_sha256"],
        serde_json::json!(benilla_sim::spell_range::SOURCE_SHA256)
    );
    for field in 0..3 {
        let mut changed = source.clone();
        match field {
            0 => changed.spells[0].range.minimum = f::float(5.0),
            1 => changed.spells[0].range.maximum = f::float(31.0),
            _ => changed.spells[0].range.flags = 1,
        }
        let mut altered = p.clone();
        if field == 2 {
            assert!(altered
                .initialize_spell_authority(&initial, changed)
                .is_err());
            assert_eq!(altered.digest(), original);
        } else {
            altered
                .initialize_spell_authority(&initial, changed)
                .unwrap();
            assert_ne!(altered.digest(), original);
        }
    }
}

#[test]
fn range_replay_rejects_result_pose_reach_source_phase_history_and_binding_substitution() {
    let mut w = world(fixture::profile());
    let genesis = w.clone();
    let mut a = fixture::spatial_only();
    let input = f::cast(&w, 133);
    w.step(1, Some(input), &mut a).unwrap();
    for tick in 2..=4 {
        w.step(tick, None, &mut a).unwrap();
    }
    let mut valid = genesis.clone();
    valid.replay(w.records()).unwrap();
    assert_eq!(valid.checkpoint().unwrap(), w.checkpoint().unwrap());
    for phase in [SourceSpellPhase::Prepare, SourceSpellPhase::ReleaseCheck] {
        for field in 0..10 {
            let mut records = w.records().to_vec();
            let reply = records
                .iter_mut()
                .flat_map(|r| &mut r.replies)
                .find(|r| {
                    matches!(&r.request.query,
                    Query::SourceSpell(SourceSpellQuery::Range { context, .. })
                        if context.phase == phase)
                })
                .unwrap();
            let Query::SourceSpell(SourceSpellQuery::Range {
                context,
                source,
                player_reach,
                target_reach,
            }) = &mut reply.request.query
            else {
                unreachable!();
            };
            match field {
                0 => {
                    reply.facts =
                        Facts::SourceSpell(SourceSpellFacts::Range(SpellRangeResult::TooFar))
                }
                1 => context.player_position.z = f::real(1.0),
                2 => context.target_position.z = f::real(1.0),
                3 => *player_reach = f::real(2.0),
                4 => *target_reach = f::real(2.0),
                5 => source.maximum = f::float(31.0),
                6 => context.phase = SourceSpellPhase::Impact,
                7 => context.history[0] ^= 1,
                8 => context.witness.moment = InteractionMoment::ReadOnly,
                _ => reply.request.binding.tick += 1,
            }
            let mut rejected = genesis.clone();
            assert_eq!(
                rejected.replay(&records),
                Err(NativeError::ReplayMismatch),
                "{phase:?} field {field}"
            );
            assert_eq!(
                rejected.checkpoint().unwrap(),
                genesis.checkpoint().unwrap()
            );
        }
    }
}

#[test]
fn range_observation_failure_rolls_back_prepare_or_release_and_retries_identically() {
    for phase in [SourceSpellPhase::Prepare, SourceSpellPhase::ReleaseCheck] {
        let mut w = world(fixture::profile());
        let mut a = fixture::spatial_only();
        let command = if phase == SourceSpellPhase::Prepare {
            Some(f::cast(&w, 133))
        } else {
            let input = f::cast(&w, 133);
            w.step(1, Some(input), &mut a).unwrap();
            w.step(2, None, &mut a).unwrap();
            None
        };
        let before = w.checkpoint().unwrap();
        let records = w.records().to_vec();
        let mut clean = w.clone();
        let tick = w.state.tick + 1;
        a.fail_range_phase = Some(phase);
        assert_eq!(
            w.step(tick, command.clone(), &mut a).map(|_| ()),
            Err(NativeError::AuthorityUnavailable(
                "synthetic range receipt fault"
            ))
        );
        assert_eq!(w.checkpoint().unwrap(), before);
        assert_eq!(w.records(), records);
        a.fail_range_phase = None;
        w.step(tick, command.clone(), &mut a).unwrap();
        clean
            .step(tick, command, &mut fixture::spatial_only())
            .unwrap();
        assert_eq!(w.checkpoint().unwrap(), clean.checkpoint().unwrap());
        assert_eq!(w.last_tick(), clean.last_tick());
    }
}

#[test]
fn old_external_range_format_nineteen_is_incompatible_without_migration() {
    let mut w = world(fixture::profile());
    let initial = w.checkpoint().unwrap();
    let mut value = serde_json::to_value(&initial).unwrap();
    value["version"] = 19.into();
    value["state"]["version"] = 19.into();
    value["state"]["policy"] = "benilla-native-backpack-movement-v19".into();
    let old: Checkpoint = serde_json::from_value(value).unwrap();
    assert!(Checkpoint::decode(&old.encode().unwrap()).is_err());
    assert!(NativeWorld::restore_synthetic_component_test(w.profile.clone(), old).is_err());
    w.step(1, None, &mut fixture::spatial_only()).unwrap();
    let mut records = w.records().to_vec();
    records[0].version = 19;
    assert!(decode_records(&encode_records(&records).unwrap()).is_err());
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial.clone()).unwrap();
    assert!(replay.replay(&records).is_err());
    assert_eq!(replay.checkpoint().unwrap(), initial);
    assert_eq!(NATIVE_FORMAT_VERSION, 74);
    assert_eq!(crate::FORMAT_VERSION, 2);
}

#[test]
fn release_range_uses_the_source_moving_targets_current_pose() {
    let (mut p, initial, mut source) = fixture::inputs();
    p.npcs[0].ai.spawn = f::point(16.0);
    let AbilityAdmission::Available(spec) = &mut p.abilities[0].admission else {
        unreachable!();
    };
    spec.cast_ms = 700;
    source.spells[0].cast_time.base_ms = 700;
    p.initialize_player(&initial, f::float(100.0), 1).unwrap();
    p.initialize_spell_authority(&initial, source).unwrap();
    let mut w = world(p);
    let mut a = fixture::SpatialOnly::default(); // real source visitor starts the chase
    w.step(1, None, &mut a).unwrap();
    let input = f::cast(&w, 133);
    w.step(2, Some(input), &mut a).unwrap();
    for tick in 3..=8 {
        w.step(tick, None, &mut a).unwrap();
    }
    let release_target = w.state.npcs[&2].ai.position();
    w.step(9, None, &mut a).unwrap();
    let values = ranges(&w);
    assert_eq!(values.len(), 2);
    let (prepare, _) = values[0];
    let (release, _) = values[1];
    assert_ne!(prepare.target_position, release.target_position);
    assert_eq!(prepare.player_position, release.player_position);
    assert_eq!(release.target_position, release_target);
}

#[test]
fn numeric_oracle_matches_exact_source_range_and_squared_distance_bits() {
    use benilla_sim::spell_range::{check_bits, RangeResult};
    let mut cases = 0;
    for line in include_str!("../../tools/fixtures/spell-range.tsv").lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 18, "{line}");
        let bit = |i| u32::from_str_radix(fields[i], 16).unwrap();
        let outcome = check_bits(
            [bit(2), bit(3), bit(4)],
            [bit(5), bit(6), bit(7)],
            bit(8),
            bit(9),
            bit(10),
            bit(11),
            fields[1] == "1",
        )
        .unwrap();
        assert_eq!(outcome.minimum, bit(12), "{} minimum", fields[0]);
        assert_eq!(outcome.maximum, bit(13), "{} maximum", fields[0]);
        assert_eq!(outcome.distance_squared, bit(14), "{} distance", fields[0]);
        let expected = match fields[15] {
            "255" => RangeResult::Within,
            "89" => RangeResult::TooFar,
            "118" => RangeResult::TooClose,
            value => panic!("unknown oracle result {value}"),
        };
        assert_eq!(outcome.result, expected, "{} result", fields[0]);
        assert_eq!(
            bit(17),
            if expected == RangeResult::Within {
                outcome.maximum
            } else {
                bit(16)
            },
            "{} source m_maxRange write",
            fields[0]
        );
        cases += 1;
    }
    assert!(cases > 1000, "oracle corpus unexpectedly small: {cases}");
}

#[test]
fn source_range_selection_keeps_nonordinary_and_incomplete_source_guards_closed() {
    for case in 0..9 {
        let (mut p, initial, mut source) = fixture::inputs();
        let original = p.digest();
        match case {
            0 => source.spells[0].range.flags = 1,
            1 => source.spells[0].range.flags = 2,
            2 => {
                source.spells[0].range.minimum = spell::SpellFloat::from_bits(0x8000_0000).unwrap()
            }
            3 => source.spells[0].range.minimum = f::float(31.0),
            4 => source.spells[0].words[6] = 0x0000_0002, // ranged-slot attribute
            5 => source.spells[0].words[7] = 0x0000_0004, // channeled
            6 => source.spells[0].hooks.spell_affect = SpellSourceRows::Unknown,
            7 => source.modifier_hooks.spell_affect = SpellSourceRows::Complete(vec![133]),
            _ => source.spells[0].range.id = 0,
        }
        assert!(
            p.initialize_spell_authority(&initial, source).is_err(),
            "case {case}"
        );
        assert_eq!(p.digest(), original);
    }
    let (mut p, initial, source) = fixture::inputs();
    let AbilityAdmission::Available(spec) = &mut p.abilities[0].admission else {
        unreachable!();
    };
    spec.cast_ms = 0;
    let before = p.digest();
    assert_eq!(
        p.initialize_spell_authority(&initial, source),
        Err(NativeError::UnsupportedMechanic(
            "instant spell entry lifecycle"
        ))
    );
    assert_eq!(p.digest(), before);
    assert!(spell::SpellFloat::from_bits(1).is_err());
    assert!(ai::Real::from_bits(1).is_err());
}
