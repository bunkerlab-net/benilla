// SPDX-License-Identifier: GPL-3.0-or-later
use super::{synthetic_fixture as f, synthetic_motion_fixture as fixture, *};
use benilla_sim::{creature_init::ExternalFeature, creature_motion as m};

fn rows(p: &Profile) -> Vec<CreatureCapabilitySource> {
    let CreatureCapabilityRows::Complete(rows) = fixture::creature_capability_sources(p) else {
        unreachable!()
    };
    rows
}
fn template(row: &mut CreatureCapabilitySource) -> &mut CreatureCapabilityTemplateSource {
    let CreatureCapabilityRows::Complete(rows) = &mut row.templates else {
        unreachable!()
    };
    &mut rows[0]
}
fn spawn(row: &mut CreatureCapabilitySource) -> &mut CreatureCapabilitySpawnTemplateSource {
    let CreatureCapabilityRows::Complete(rows) = &mut row.spawn_templates else {
        unreachable!()
    };
    &mut rows[0]
}
fn profile() -> Profile {
    let mut p = fixture::profile();
    p.initialize_creature_capabilities(fixture::creature_capability_sources(&p))
        .unwrap();
    p
}
fn world() -> NativeWorld {
    NativeWorld::synthetic_component_test(profile(), [8; 16], f::player(), 1234).unwrap()
}

#[test]
fn provider_consistency_validator_accepts_actual_respawn_witness_without_stale_state() {
    let mut p = profile();
    p.npcs[0].ai.corpse_decay_ms = 100;
    p.npcs[0].ai.respawn_delay_seconds = 1;
    let mut w = NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 1234).unwrap();
    let old = w.state().npcs[&2]
        .creature_capabilities()
        .unwrap()
        .witness()
        .clone();
    old.validate_for(w.profile(), old.npc()).unwrap();
    // Authored external death enters the real source host's corpse/respawn path.
    w.state.npcs.get_mut(&2).unwrap().health = 0;
    let mut a = fixture::SyntheticMotionAuthority {
        visits: Some(false),
        ..Default::default()
    };
    while w.state().npcs[&2].id.generation == 0 {
        assert!(w.state().tick < 25);
        step(&mut w, &mut a);
    }
    let current = w.state().npcs[&2]
        .creature_capabilities()
        .unwrap()
        .witness();
    assert_eq!(current.npc().generation, 1);
    current.validate_for(w.profile(), current.npc()).unwrap();
    assert!(old.validate_for(w.profile(), current.npc()).is_err());
    assert!(current.validate_for(w.profile(), old.npc()).is_err());
    assert_eq!(old.source_binding(), current.source_binding());
    let mut changed = w.profile().clone();
    changed.npcs[0].entry += 1;
    assert!(current.validate_for(&changed, current.npc()).is_err());
}
fn step(w: &mut NativeWorld, authority: &mut impl Authority) {
    w.step(w.state().tick + 1, None, authority).unwrap();
}

#[test]
fn raw_catalog_retains_all_valid_habitats_but_runtime_selects_only_actual_ground() {
    let mut p = fixture::profile();
    for habitat in 1..=7 {
        let mut input = rows(&p);
        template(&mut input[0]).inhabit_type = habitat;
        let catalog = CreatureCapabilityCatalog::validate_sources(
            CreatureCapabilityRows::Complete(input.clone()),
        )
        .unwrap();
        assert_eq!(catalog.source(2).unwrap(), &input[0]);
        let before = p.digest();
        let result = p.initialize_creature_capabilities(CreatureCapabilityRows::Complete(input));
        if habitat == 1 {
            assert!(result.is_ok());
        } else {
            assert!(matches!(result, Err(NativeError::UnsupportedMechanic(_))));
            assert_eq!(p.digest(), before);
        }
    }
    for invalid in [0, 8, 255, u32::MAX] {
        let mut input = rows(&p);
        template(&mut input[0]).inhabit_type = invalid;
        assert!(
            CreatureCapabilityCatalog::validate_sources(CreatureCapabilityRows::Complete(input))
                .is_err()
        );
    }
}

#[test]
fn missing_or_non_singleton_source_queries_never_become_empty_proofs() {
    let mut p = profile();
    for case in 0..11 {
        let mut input = rows(&p);
        match case {
            0 => input[0].templates = CreatureCapabilityRows::Missing,
            1 => input[0].templates = CreatureCapabilityRows::Complete(vec![]),
            2 => input[0].spawn_templates = CreatureCapabilityRows::Missing,
            3 => input[0].spawn_templates = CreatureCapabilityRows::Complete(vec![]),
            4 => input[0].external_features = CreatureCapabilityRows::Missing,
            5 => input[0].spawn_id = 3,
            6 => input[0].spawn_entry += 1,
            7 => input[0].requested_spawn_template_entry = 17,
            8 => {
                let t = template(&mut input[0]).clone();
                input[0].templates = CreatureCapabilityRows::Complete(vec![t.clone(), t]);
            }
            9 => {
                let t = spawn(&mut input[0]).clone();
                input[0].spawn_templates = CreatureCapabilityRows::Complete(vec![t.clone(), t]);
            }
            10 => input.push(input[0].clone()),
            _ => unreachable!(),
        }
        let before = p.digest();
        assert!(
            p.initialize_creature_capabilities(CreatureCapabilityRows::Complete(input))
                .is_err(),
            "case {case}"
        );
        assert_eq!(p.digest(), before);
    }
    assert!(p
        .initialize_creature_capabilities(CreatureCapabilityRows::Missing)
        .is_err());
    assert!(p
        .initialize_creature_capabilities(CreatureCapabilityRows::Complete(vec![]))
        .is_err());
    let mut unselected = f::profile();
    assert!(unselected
        .initialize_creature_capabilities(fixture::creature_capability_sources(&unselected))
        .is_err());
}

#[test]
fn a_generation_source_reference_without_its_owner_cannot_install_or_validate_a_witness() {
    let w = world();
    let witness = w.state().npcs[&2]
        .creature_capabilities()
        .unwrap()
        .witness();
    for source_identity in [[0; 32], [7; 32]] {
        let mut p = w.profile().clone();
        let before = p.clone();
        let mut input = rows(&p);
        input[0].addon_closure =
            CreatureCapabilityAddonClosure::GenerationSource { source_identity };
        assert_eq!(
            p.initialize_creature_capabilities(CreatureCapabilityRows::Complete(input.clone())),
            Err(NativeError::AuthorityUnavailable(
                "creature capability generation source"
            ))
        );
        assert_eq!(p, before);

        // A structurally valid catalog is not a profile-bound owner. Providers
        // must reject the missing dependency even without Profile::validate.
        p.creature_capabilities = Some(
            CreatureCapabilityCatalog::validate_sources(CreatureCapabilityRows::Complete(input))
                .unwrap(),
        );
        assert_eq!(
            witness.validate_for(&p, witness.npc()),
            Err(NativeError::AuthorityUnavailable(
                "creature capability generation source"
            ))
        );
    }
}

#[test]
fn actual_spawn_rows_are_required_and_only_the_run_bit_is_supported() {
    let mut p = fixture::profile();
    for flags in [0, 1] {
        for entry in [0, 17] {
            let mut input = rows(&p);
            input[0].requested_spawn_template_entry = entry;
            let spawn = spawn(&mut input[0]);
            spawn.entry = entry;
            spawn.spawn_flags = flags;
            p.initialize_creature_capabilities(CreatureCapabilityRows::Complete(input))
                .unwrap();
            let w =
                NativeWorld::synthetic_component_test(p.clone(), [8; 16], f::player(), 1).unwrap();
            assert_eq!(
                w.state().npcs[&2]
                    .creature_capabilities()
                    .unwrap()
                    .witness()
                    .capability_movement_flags(),
                0
            );
        }
    }
    for flags in [2, 4, 8, 16, 0x4000_0000, u32::MAX] {
        let mut input = rows(&p);
        spawn(&mut input[0]).spawn_flags = flags;
        assert!(p
            .initialize_creature_capabilities(CreatureCapabilityRows::Complete(input))
            .is_err());
    }
    for field in 0..2 {
        let mut input = rows(&p);
        if field == 0 {
            spawn(&mut input[0]).relay_id = 1;
        } else {
            spawn(&mut input[0]).string_id = 1;
        }
        assert!(p
            .initialize_creature_capabilities(CreatureCapabilityRows::Complete(input))
            .is_err());
    }
}

#[test]
fn all_existing_external_feature_exclusions_remain_closed() {
    let mut p = fixture::profile();
    for feature in [
        ExternalFeature::PetSummonOrControl,
        ExternalFeature::InstanceOrBattleground,
        ExternalFeature::FlightSwimmingTransportOrMount,
        ExternalFeature::ExistingStateOrForcedLevel,
        ExternalFeature::GameEventOrPoolSubstitution,
        ExternalFeature::ConditionalOrGroupedSpawn,
        ExternalFeature::LinkingOrAssistance,
        ExternalFeature::Addon,
        ExternalFeature::InitialAuraOrUnitModifier,
        ExternalFeature::ScriptOrAiSubstitution,
        ExternalFeature::SpellList,
        ExternalFeature::MapOrOutdoorPvpCreationHook,
        ExternalFeature::EquipmentOrLootWeapon,
        ExternalFeature::EncounterCorpseOverride,
    ] {
        let mut input = rows(&p);
        input[0].external_features = CreatureCapabilityRows::Complete(vec![feature]);
        assert!(
            p.initialize_creature_capabilities(CreatureCapabilityRows::Complete(input))
                .is_err(),
            "{feature:?}"
        );
    }
}

#[test]
fn repeated_template_and_spawn_template_identities_cannot_disagree() {
    let p = fixture::profile();
    for mismatch in 0..2 {
        let mut input = rows(&p);
        let mut other = input[0].clone();
        other.spawn_id = 3;
        if mismatch == 0 {
            template(&mut other).inhabit_type = 3;
        } else {
            spawn(&mut other).spawn_flags = 0;
        }
        input.push(other);
        assert!(
            CreatureCapabilityCatalog::validate_sources(CreatureCapabilityRows::Complete(input))
                .is_err()
        );
    }
}

#[test]
fn real_chase_request_contains_owned_raw_witness_and_replays_exactly() {
    let mut w = world();
    let initial = w.checkpoint().unwrap();
    let mut a = fixture::SyntheticMotionAuthority::default();
    step(&mut w, &mut a);
    step(&mut w, &mut a);
    let request = a
        .queries
        .iter()
        .find(|q| matches!(&q.query, Query::SourceMotion(MotionQuery::NearPoint(_))))
        .unwrap();
    let Query::SourceMotion(MotionQuery::NearPoint(near)) = &request.query else {
        unreachable!()
    };
    assert_eq!(
        near.searcher,
        *w.state().npcs[&2]
            .creature_capabilities()
            .unwrap()
            .witness()
    );
    assert_eq!(request.binding.npc, Some(near.searcher.npc()));
    assert_eq!(
        near.request.binding.now_ms,
        w.profile().clock_origin_ms + request.binding.tick * u64::from(w.profile().tick_ms)
    );
    assert_eq!(
        near.request.binding.identity.creature,
        near.searcher.npc().spawn
    );
    assert_eq!(near.searcher.inhabit_type(), 1);
    assert_eq!(near.searcher.capability_movement_flags(), 0);
    let caps = near.searcher.capabilities().unwrap();
    assert!(caps.can_walk());
    assert!(!caps.can_swim());
    assert!(!caps.can_fly());
    assert_eq!(
        near.searcher.allowed_position_z_branch().unwrap(),
        AllowedPositionZBranch::Ground
    );
    assert!(!a.queries.iter().any(|q| matches!(
        &q.query,
        Query::SourceMotion(MotionQuery::Spatial(m::Query::NearPoint(_)))
    )));
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile().clone(), initial).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(w.state_digest(), replay.state_digest());
    let reopened =
        NativeWorld::restore_synthetic_component_test(w.profile().clone(), w.checkpoint().unwrap())
            .unwrap();
    assert_eq!(reopened.state_digest(), w.state_digest());
}

#[test]
fn unselected_component_near_point_remains_unwrapped() {
    let mut w = fixture::world();
    let mut a = fixture::SyntheticMotionAuthority::default();
    step(&mut w, &mut a);
    step(&mut w, &mut a);
    assert!(a.queries.iter().any(|q| matches!(
        &q.query,
        Query::SourceMotion(MotionQuery::Spatial(m::Query::NearPoint(_)))
    )));
    assert!(!a
        .queries
        .iter()
        .any(|q| matches!(&q.query, Query::SourceMotion(MotionQuery::NearPoint(_)))));
    assert!(w.state().npcs[&2].creature_capabilities().is_none());
}

fn rehashed(w: &NativeWorld, change: impl FnOnce(&mut serde_json::Value)) -> Checkpoint {
    let mut c = w.checkpoint().unwrap();
    let mut state = serde_json::to_value(&c.state).unwrap();
    change(&mut state);
    c.state = serde_json::from_value(state).unwrap();
    c.digest = crate::digest(b"benilla-native-state-v82\0", &c.state);
    c.checksum = crate::digest(
        b"benilla-native-checkpoint-v82\0",
        &(&c.state, c.digest, c.history_tip),
    );
    c
}
#[test]
fn restore_rejects_rehashed_source_binding_habitat_flags_and_generation() {
    let w = world();
    for field in 0..7 {
        let c = rehashed(&w, |state| {
            let witness = &mut state["npcs"]["2"]["source_capabilities"]["witness"];
            match field {
                0 => witness["inhabit_type"] = 7.into(),
                1 => witness["capability_movement_flags"] = 0x400_u32.into(),
                2 => witness["capability_movement_flags"] = 0x4000_0000_u32.into(),
                3 => witness["capability_movement_flags"] = 1.into(),
                4 => {
                    witness["source_binding"][0] =
                        ((witness["source_binding"][0].as_u64().unwrap() + 1) % 256).into()
                }
                5 => witness["npc"]["generation"] = 1.into(),
                6 => witness["npc"]["spawn"] = 3.into(),
                _ => unreachable!(),
            }
        });
        assert!(
            NativeWorld::restore_synthetic_component_test(w.profile().clone(), c).is_err(),
            "field {field}"
        );
    }
    let c = rehashed(&w, |state| {
        state["npcs"]["2"]["source_capabilities"] = serde_json::Value::Null
    });
    assert!(NativeWorld::restore_synthetic_component_test(w.profile().clone(), c).is_err());
    let mut changed = w.profile().clone();
    let mut input = rows(&changed);
    spawn(&mut input[0]).spawn_flags = 0;
    changed
        .initialize_creature_capabilities(CreatureCapabilityRows::Complete(input))
        .unwrap();
    assert!(
        NativeWorld::restore_synthetic_component_test(changed, w.checkpoint().unwrap()).is_err()
    );
}

#[test]
fn missing_or_tampered_near_point_fails_without_committing_the_frame() {
    struct Missing {
        synthetic: fixture::SyntheticMotionAuthority,
        tamper: bool,
    }
    impl Authority for Missing {
        fn resolve(&mut self, q: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
            let mut answer = self.synthetic.resolve(q)?;
            if matches!(&q.query, Query::SourceMotion(MotionQuery::NearPoint(_))) {
                if !self.tamper {
                    return Err(NativeError::AuthorityUnavailable(
                        "actual near-point provider",
                    ));
                }
                let Query::SourceMotion(MotionQuery::NearPoint(near)) = &mut answer.request.query
                else {
                    unreachable!()
                };
                near.request.owner.position = f::point(99.0);
            }
            Ok(answer)
        }
    }
    for tamper in [false, true] {
        let mut w = world();
        let mut a = Missing {
            synthetic: Default::default(),
            tamper,
        };
        step(&mut w, &mut a);
        let before = w.checkpoint().unwrap();
        assert!(w.step(2, None, &mut a).is_err());
        assert_eq!(w.checkpoint().unwrap(), before);
    }
}

#[test]
fn replay_rejects_altered_capability_context_without_committing() {
    let mut w = world();
    let mut a = fixture::SyntheticMotionAuthority::default();
    step(&mut w, &mut a);
    let checkpoint = w.checkpoint().unwrap();
    step(&mut w, &mut a);
    let record = w.last_tick().unwrap();
    for field in ["inhabit_type", "capability_movement_flags"] {
        let mut forged = record.clone();
        let reply = forged
            .replies
            .iter_mut()
            .find(|r| {
                matches!(
                    &r.request.query,
                    Query::SourceMotion(MotionQuery::NearPoint(_))
                )
            })
            .unwrap();
        let Query::SourceMotion(MotionQuery::NearPoint(near)) = &mut reply.request.query else {
            unreachable!()
        };
        let mut witness = serde_json::to_value(&near.searcher).unwrap();
        witness[field] = 7.into();
        near.searcher = serde_json::from_value(witness).unwrap();
        let mut replay =
            NativeWorld::restore_synthetic_component_test(w.profile().clone(), checkpoint.clone())
                .unwrap();
        let before = replay.checkpoint().unwrap();
        assert!(replay.replay(&[forged]).is_err());
        assert_eq!(replay.checkpoint().unwrap(), before);
    }
}

#[test]
fn catalog_retains_scalar_policy_and_constructor_lifecycle_source_hashes() {
    let p = profile();
    let catalog = serde_json::to_value(p.creature_capabilities.as_ref().unwrap()).unwrap();
    assert_eq!(
        CREATURE_CAPABILITY_POLICY,
        "cmangos-8ec338a1-native-creature-capability-v3"
    );
    assert_eq!(catalog["policy"], CREATURE_CAPABILITY_POLICY);
    assert_eq!(catalog["spawns"]["2"]["addon_closure"], "Absent");
    assert_eq!(
        catalog["source_policy"]["predicate"],
        benilla_sim::creature_capabilities::POLICY
    );
    assert_eq!(
        catalog["source_policy"]["predicate_sources"],
        serde_json::json!(benilla_sim::creature_capabilities::SOURCE_SHA256)
    );
    assert_eq!(
        catalog["source_policy"]["runtime_sources"],
        serde_json::json!(CREATURE_CAPABILITY_RUNTIME_SOURCE_SHA256)
    );
    assert_eq!(NATIVE_FORMAT_VERSION, 82);
}

#[test]
fn the_v3_witness_binding_rejects_an_exact_legacy_v1_source_hash() {
    #[derive(serde::Serialize)]
    struct Policy {
        predicate: &'static str,
        predicate_sources: [&'static str; 3],
        runtime_sources: [&'static str; 2],
    }
    // The prior source schema had no addon closure. Preserve its field order to
    // reproduce the actual old wire hash rather than only changing its domain.
    #[derive(serde::Serialize)]
    struct LegacySource<'a> {
        spawn_id: u64,
        spawn_entry: u32,
        requested_spawn_template_entry: u32,
        templates: &'a CreatureCapabilityRows<CreatureCapabilityTemplateSource>,
        spawn_templates: &'a CreatureCapabilityRows<CreatureCapabilitySpawnTemplateSource>,
        external_features: &'a CreatureCapabilityRows<ExternalFeature>,
    }
    let policy = Policy {
        predicate: benilla_sim::creature_capabilities::POLICY,
        predicate_sources: benilla_sim::creature_capabilities::SOURCE_SHA256,
        runtime_sources: CREATURE_CAPABILITY_RUNTIME_SOURCE_SHA256,
    };
    let w = world();
    let row = w
        .profile()
        .creature_capabilities
        .as_ref()
        .unwrap()
        .source(2)
        .unwrap();
    let legacy = LegacySource {
        spawn_id: row.spawn_id,
        spawn_entry: row.spawn_entry,
        requested_spawn_template_entry: row.requested_spawn_template_entry,
        templates: &row.templates,
        spawn_templates: &row.spawn_templates,
        external_features: &row.external_features,
    };
    let old_binding = crate::digest(
        b"benilla-creature-capability-source-v1\0",
        &(
            "cmangos-8ec338a1-native-creature-capability-v1",
            &policy,
            legacy,
        ),
    );
    let witness = w.state().npcs[&2]
        .creature_capabilities()
        .unwrap()
        .witness();
    assert_eq!(
        witness.source_binding(),
        crate::digest(
            b"benilla-creature-capability-source-v3\0",
            &(
                CREATURE_CAPABILITY_POLICY,
                CREATURE_INITIALIZATION_POLICY,
                benilla_sim::creature_swimming::POLICY,
                benilla_sim::creature_swimming::SOURCE_SHA256,
                policy,
                row,
            ),
        )
    );
    assert_ne!(witness.source_binding(), old_binding);
    let mut stale = serde_json::to_value(witness).unwrap();
    stale["source_binding"] = serde_json::to_value(old_binding).unwrap();
    let stale: CreatureCapabilityWitness = serde_json::from_value(stale).unwrap();
    assert_eq!(
        stale.validate_for(w.profile(), witness.npc()),
        Err(NativeError::AuthorityBinding)
    );
}
