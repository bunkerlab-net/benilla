// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored ground-only composition through real runtime owners and callbacks.
//! These fixtures are not imported creatures or evidence of native admission.
use super::*;
use benilla_sim::creature_motion as motion;

fn addon(sheath_state: u32) -> init::Addon {
    init::Addon {
        mount: 0,
        stand_state: 0,
        sheath_state,
        emote: 0,
        move_flags: 0,
        auras: vec![],
    }
}

fn authored_base() -> Profile {
    let mut p = synthetic_combat_fixture::profile();
    p.npcs[0].ai.spawn = f::point(6.0);
    p
}

fn authored_source(
    spawn_sheath: Option<u32>,
    template_sheath: Option<u32>,
) -> CreatureGenerationSource {
    let mut raw = complete_scene_sources(&authored_base()).remove(0);
    raw.input.spawn_addons = init::Rows::Complete(
        spawn_sheath
            .into_iter()
            .map(|sheath| init::SpawnAddonRow {
                guid: raw.spawn_id,
                addon: addon(sheath),
            })
            .collect(),
    );
    raw.input.template_addons = init::Rows::Complete(
        template_sheath
            .into_iter()
            .map(|sheath| init::TemplateAddonRow {
                entry: raw.spawn_entry,
                addon: addon(sheath),
            })
            .collect(),
    );
    raw
}

fn home(p: &mut Profile) -> &mut HomeAudit {
    &mut p
        .source_motion
        .as_mut()
        .unwrap()
        .npcs
        .get_mut(&2)
        .unwrap()
        .npc_policy
        .home
}

fn capabilities(p: &Profile, raw: &CreatureGenerationSource) -> Vec<CreatureCapabilitySource> {
    let CreatureCapabilityRows::Complete(mut rows) =
        synthetic_motion_fixture::creature_capability_sources(p)
    else {
        unreachable!()
    };
    rows[0].addon_closure = CreatureCapabilityAddonClosure::GenerationSource {
        source_identity: raw.identity(),
    };
    rows
}

fn visitors(p: &Profile, identity: crate::Digest) -> SourceVisitorCatalog {
    let mut input = synthetic_two_owner_fixture::visitor_input(p, vec![2]);
    let SourceVisitorRows::Complete(rows) = &mut input.owners else {
        unreachable!()
    };
    rows[0].addon_closure = SourceVisitorAddonClosure::GenerationSource {
        source_identity: identity,
    };
    SourceVisitorCatalog::validate_sources(input).unwrap()
}

fn composed(raw: CreatureGenerationSource) -> Profile {
    let mut p = authored_base();
    home(&mut p).addon_closure = HomeAddonClosure::GenerationSource {
        source_identity: raw.identity(),
    };
    // Reuse the static exemplar setup, then install both retained owners together.
    // None of these authored profiles is a production admission constructor.
    p = install(p, vec![raw.clone()], vec![2]);
    p.creature_generations = None;
    p.source_visitors = Some(visitors(&p, raw.identity()));
    let capability_rows = capabilities(&p, &raw);
    p.initialize_creature_generation_capabilities(
        init::Rows::Complete(vec![raw]),
        vec![2],
        CreatureCapabilityRows::Complete(capability_rows),
    )
    .unwrap();
    p
}

fn capability(w: &NativeWorld) -> &CreatureCapabilityWitness {
    w.state.npcs[&2].creature_capabilities().unwrap().witness()
}

fn assert_owned(w: &NativeWorld, raw: &CreatureGenerationSource, sheath: u32) {
    assert_eq!(owner(w, 2).source_identity(), raw.identity());
    assert_eq!(owner(w, 2).result().unit_bytes_2, 0x1000 | sheath);
    assert_eq!(
        w.profile
            .creature_capabilities
            .as_ref()
            .unwrap()
            .source(2)
            .unwrap()
            .addon_closure,
        CreatureCapabilityAddonClosure::GenerationSource {
            source_identity: raw.identity()
        }
    );
    assert_eq!(
        w.profile
            .npc_motion(2)
            .unwrap()
            .npc_policy
            .home
            .addon_closure,
        HomeAddonClosure::GenerationSource {
            source_identity: raw.identity()
        }
    );
    assert_eq!(
        w.profile
            .source_visitors
            .as_ref()
            .unwrap()
            .owner(2)
            .unwrap()
            .addon_closure,
        SourceVisitorAddonClosure::GenerationSource {
            source_identity: raw.identity()
        }
    );
    let witness = capability(w);
    assert_eq!(witness.npc(), owner(w, 2).npc_id());
    witness.validate_for(&w.profile, witness.npc()).unwrap();
    assert_eq!(witness.inhabit_type(), 1);
    assert_eq!(witness.capability_movement_flags(), 0);
    assert_eq!(
        witness.allowed_position_z_branch().unwrap(),
        AllowedPositionZBranch::Ground
    );
    assert!(witness.capabilities().unwrap().can_walk());
    assert!(!witness.capabilities().unwrap().can_swim());
    assert!(!witness.capabilities().unwrap().can_fly());
    assert_selection(w, 2);
}

#[test]
fn every_sheath_and_guid_precedence_execute_with_all_source_owners_and_derived_witnesses() {
    for sheath in 0..=2 {
        for (spawn, template) in [
            (None, Some(sheath)),
            (Some(sheath), None),
            (Some(sheath), Some((sheath + 1) % 3)),
        ] {
            let raw = authored_source(spawn, template);
            let mut w = world(composed(raw.clone()), 7);
            let initial = w.checkpoint().unwrap();
            let mut authority = synthetic_two_owner_fixture::CompleteAuthority::default();
            for _ in 0..3 {
                step(&mut w, &mut authority);
            }
            assert_owned(&w, &raw, sheath);
            assert!(authority.derived.iter().any(|reply| matches!(
                (&reply.request.query, &reply.facts),
                (Query::SourceMotion(MotionQuery::VisitorSight(sight)),
                 Facts::SourceMotion(MotionFacts::VisitorSight(SightDisposition::PlayerSightApplied)))
                 if sight.observer == VisitorParticipant::Creature(w.state.npcs[&2].id)
                    && sight.target == VisitorParticipant::Player(w.state.player))));
            let near = authority
                .queries
                .iter()
                .find_map(|request| match &request.query {
                    Query::SourceMotion(MotionQuery::NearPoint(near)) => Some((request, near)),
                    _ => None,
                })
                .expect("real source chase requests an owner-bound near point");
            assert_eq!(near.1.searcher, *capability(&w));
            assert_eq!(near.0.binding.npc, Some(owner(&w, 2).npc_id()));
            assert!(authority.queries.iter().all(|request| !matches!(
                request.query,
                Query::SourceMotion(MotionQuery::VisitorSight(_) | MotionQuery::Visitor(_))
            )));
            let mut replay =
                NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
            replay.replay(w.records()).unwrap();
            assert_eq!(replay.state_digest(), w.state_digest());
            assert_eq!(restore(&w).state_digest(), w.state_digest());
        }
    }
}

#[test]
fn composed_sheath_owner_survives_home_death_respawn_save_replay_and_compaction() {
    for sheath in 0..=2 {
        let raw = authored_source(Some(sheath), Some((sheath + 1) % 3));
        let mut p = composed(raw.clone());
        p.npcs[0].ai.hard_leash_radius = Some(f::real(1.0));
        let mut w = world(p, 7);
        let initial = w.checkpoint().unwrap();
        let history = owner(&w, 2).history().to_vec();
        let mut authority = synthetic_two_owner_fixture::CompleteAuthority::default();
        let mut reached = false;
        for _ in 0..100 {
            step(&mut w, &mut authority);
            assert_owned(&w, &raw, sheath);
            assert_eq!(owner(&w, 2).history(), history);
            reached = w.last_tick().unwrap().events.iter().any(|event| {
                matches!(
                    event,
                    Event::NpcMotion {
                        event: motion::Event::HomeReached { .. },
                        ..
                    }
                )
            });
            if reached {
                break;
            }
        }
        assert!(
            reached,
            "the addon owner must execute the real Home callback"
        );
        let mut replay =
            NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
        replay.replay(w.records()).unwrap();
        assert_eq!(replay.state_digest(), w.state_digest());
        assert_eq!(restore(&w).state_digest(), w.state_digest());

        // The lethal cast uses the authored one-health spawn before Home heals it.
        let mut w = world(composed(raw.clone()), 7);
        let initial = w.checkpoint().unwrap();
        let old_id = owner(&w, 2).npc_id();
        let old_capability = capability(&w).clone();
        let mut authority = synthetic_two_owner_fixture::CompleteAuthority::default();
        authority.base.base.critical_chance = Some(0);
        authority.base.base.resist_chance = Some(0);
        kill(&mut w, &mut authority);
        assert_owned(&w, &raw, sheath);
        let mut corpse_resume = restore(&w);
        respawn(&mut w, &mut authority);
        while corpse_resume.state.tick < w.state.tick {
            step(&mut corpse_resume, &mut authority);
        }
        assert_eq!(corpse_resume.state_digest(), w.state_digest());
        assert_owned(&w, &raw, sheath);
        assert_eq!(owner(&w, 2).history().len(), 2);
        assert_eq!(
            owner(&w, 2)
                .historical(old_id)
                .unwrap()
                .result()
                .unit_bytes_2,
            0x1000 | sheath
        );
        assert_eq!(
            old_capability.source_binding(),
            capability(&w).source_binding()
        );
        assert!(old_capability
            .validate_for(&w.profile, w.state.npcs[&2].id)
            .is_err());
        let mut replay =
            NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
        replay.replay(w.records()).unwrap();
        assert_eq!(replay.state_digest(), w.state_digest());
        let checkpoint = w.checkpoint().unwrap();
        w.checkpoint_committed(&checkpoint).unwrap();
        assert!(w.records().is_empty());
        let reopened = restore(&w);
        assert_eq!(reopened.state_digest(), w.state_digest());
        assert_owned(&reopened, &raw, sheath);
    }
}

#[test]
fn ignored_moveflags_change_every_source_binding_without_changing_results_or_draws() {
    let raw = authored_source(Some(1), Some(2));
    let mut old = world(composed(raw.clone()), 7);
    let mut authority = synthetic_two_owner_fixture::CompleteAuthority::default();
    for _ in 0..3 {
        step(&mut old, &mut authority);
    }
    for guid in [false, true] {
        let mut changed = raw.clone();
        if guid {
            let init::Rows::Complete(rows) = &mut changed.input.spawn_addons else {
                unreachable!()
            };
            rows[0].addon.move_flags = u32::MAX;
        } else {
            let init::Rows::Complete(rows) = &mut changed.input.template_addons else {
                unreachable!()
            };
            rows[0].addon.move_flags = u32::MAX;
        }
        let mut fresh = world(composed(changed.clone()), 7);
        assert_ne!(changed.identity(), raw.identity());
        assert_ne!(fresh.profile_digest(), old.profile_digest());
        assert_ne!(
            capability(&fresh).source_binding(),
            capability(&old).source_binding()
        );
        assert_eq!(owner(&fresh, 2).result(), owner(&old, 2).result());
        assert_eq!(
            owner(&fresh, 2).genesis_draws(),
            owner(&old, 2).genesis_draws()
        );
        assert_owned(&fresh, &changed, 1);
        assert!(capability(&old)
            .validate_for(&fresh.profile, fresh.state.npcs[&2].id)
            .is_err());
        let before = fresh.checkpoint().unwrap();
        assert!(fresh.replay(old.records()).is_err());
        assert_eq!(fresh.checkpoint().unwrap(), before);
        assert!(fresh.records().is_empty());
        assert!(NativeWorld::restore_synthetic_component_test(
            fresh.profile.clone(),
            old.checkpoint().unwrap()
        )
        .is_err());
    }
}

#[test]
fn atomic_composition_rejects_stale_missing_wrong_owner_and_each_false_absence() {
    let raw = authored_source(Some(0), Some(2));
    let good = composed(raw.clone());
    let mut other = raw.clone();
    other.spawn_id = 7;
    other.input.spawn_id = 7;
    let init::Rows::Complete(rows) = &mut other.input.spawn_addons else {
        unreachable!()
    };
    rows[0].guid = 7;
    for case in 0..10 {
        let mut p = good.clone();
        let mut raw = raw.clone();
        let mut rows = capabilities(&p, &raw);
        match case {
            0 => rows[0].addon_closure = CreatureCapabilityAddonClosure::Absent,
            1 => {
                rows[0].addon_closure = CreatureCapabilityAddonClosure::GenerationSource {
                    source_identity: [0; 32],
                }
            }
            2 => {
                rows[0].addon_closure = CreatureCapabilityAddonClosure::GenerationSource {
                    source_identity: other.identity(),
                }
            }
            3 => home(&mut p).addon_closure = HomeAddonClosure::Absent,
            4 => {
                home(&mut p).addon_closure = HomeAddonClosure::GenerationSource {
                    source_identity: other.identity(),
                }
            }
            5 => {
                p.source_visitors = Some(
                    SourceVisitorCatalog::validate_sources(
                        synthetic_two_owner_fixture::visitor_input(&p, vec![2]),
                    )
                    .unwrap(),
                )
            }
            6 => p.source_visitors = Some(visitors(&p, other.identity())),
            7 => raw.input.spawn_addons = init::Rows::Missing,
            8 => raw.input.template_addons = init::Rows::Missing,
            _ => {
                let init::Rows::Complete(addons) = &mut raw.input.spawn_addons else {
                    unreachable!()
                };
                addons[0].addon.move_flags = 1;
            }
        }
        let before = p.clone();
        assert!(
            p.initialize_creature_generation_capabilities(
                init::Rows::Complete(vec![raw]),
                vec![2],
                CreatureCapabilityRows::Complete(rows)
            )
            .is_err(),
            "binding case {case}"
        );
        assert_eq!(
            p, before,
            "both retained catalogs roll back together, case {case}"
        );
    }
    let mut missing = good.clone();
    missing.creature_generations = None;
    assert!(missing.validate().is_err());
    assert!(capability(&world(good, 7))
        .validate_for(
            &missing,
            crate::NpcId {
                spawn: 2,
                generation: 0
            }
        )
        .is_err());
}

#[test]
fn composed_derived_callback_failure_rolls_back_owner_rng_and_journal_before_retry() {
    let raw = authored_source(Some(2), Some(0));
    let mut w = world(composed(raw.clone()), 7);
    step(
        &mut w,
        &mut synthetic_two_owner_fixture::CompleteAuthority::default(),
    );
    let before = w.checkpoint().unwrap();
    let records = w.records().to_vec();
    let mut expected = w.clone();
    step(
        &mut expected,
        &mut synthetic_two_owner_fixture::CompleteAuthority::default(),
    );
    let mut failed = synthetic_two_owner_fixture::CompleteAuthority {
        fail_derived_owner: Some(2),
        ..Default::default()
    };
    assert_eq!(
        w.step(w.state.tick + 1, None, &mut failed),
        Err(NativeError::AuthorityUnavailable(
            "authored late owner derived receipt fault"
        ))
    );
    assert_eq!(w.checkpoint().unwrap(), before);
    assert_eq!(w.records(), records);
    assert_owned(&w, &raw, 2);
    failed.fail_derived_owner = None;
    step(&mut w, &mut failed);
    assert_eq!(w.checkpoint().unwrap(), expected.checkpoint().unwrap());
    assert_eq!(w.records(), expected.records());
}

#[test]
fn pinned_classicdb_entry_six_habitat_three_and_zero_spawn_flags_remain_rejected() {
    // Genuine pinned ClassicDB capability fields, not a ground-only replacement
    // for entry 6. Other initializer rows remain expressly authored test inputs.
    let mut p = synthetic_motion_fixture::profile();
    p.npcs[0].entry = 6;
    let mut raw = complete_scene_sources(&p).remove(0);
    let init::Rows::Complete(spawn) = &mut raw.input.spawn_template else {
        unreachable!()
    };
    spawn[0].spawn_flags = 0;
    raw.input.template_addons = init::Rows::Complete(vec![init::TemplateAddonRow {
        entry: 6,
        addon: addon(1),
    }]);
    home(&mut p).addon_closure = HomeAddonClosure::GenerationSource {
        source_identity: raw.identity(),
    };
    p = install(p, vec![raw.clone()], vec![2]);
    let mut rows = capabilities(&p, &raw);
    let CreatureCapabilityRows::Complete(template) = &mut rows[0].templates else {
        unreachable!()
    };
    template[0].inhabit_type = 3;
    let CreatureCapabilityRows::Complete(spawn) = &mut rows[0].spawn_templates else {
        unreachable!()
    };
    spawn[0].spawn_flags = 0;
    assert_eq!(rows[0].spawn_entry, 6);
    assert_eq!(rows[0].requested_spawn_template_entry, 0);
    let catalog =
        CreatureCapabilityCatalog::validate_sources(CreatureCapabilityRows::Complete(rows.clone()))
            .unwrap();
    assert_eq!(catalog.source(2), Some(&rows[0]));
    let before = p.clone();
    assert_eq!(
        p.initialize_creature_generation_capabilities(
            init::Rows::Complete(vec![raw]),
            vec![2],
            CreatureCapabilityRows::Complete(rows)
        ),
        Err(NativeError::UnsupportedMechanic(
            "runtime creature capability habitat domain"
        ))
    );
    assert_eq!(p, before);
}

#[test]
fn composed_rehashed_owner_sources_and_route_receipts_reject_stale_substitution() {
    let raw = authored_source(Some(1), Some(2));
    let mut changed = raw.clone();
    let init::Rows::Complete(rows) = &mut changed.input.spawn_addons else {
        unreachable!()
    };
    rows[0].addon.move_flags = u32::MAX;
    let mut old = world(composed(raw), 7);
    let mut fresh = world(composed(changed), 7);
    for w in [&mut old, &mut fresh] {
        let mut authority = synthetic_two_owner_fixture::CompleteAuthority::default();
        for _ in 0..3 {
            step(w, &mut authority);
        }
    }
    let saved = serde_json::to_value(fresh.checkpoint().unwrap()).unwrap();
    for (path, stale) in [
        (
            "/state/npcs/2/source_generation/source",
            owner(&old, 2).source_identity(),
        ),
        (
            "/state/npcs/2/source_capabilities/witness/source_binding",
            capability(&old).source_binding(),
        ),
    ] {
        let mut forged = saved.clone();
        let value = forged
            .pointer_mut(path)
            .expect("retained owner source field");
        assert_ne!(*value, serde_json::to_value(stale).unwrap());
        *value = serde_json::to_value(stale).unwrap();
        let forged = resign(forged);
        // The envelope and current profile stay valid. The retained owner itself
        // must reject the stale source even after both outer hashes are repaired.
        Checkpoint::decode(&forged.encode().unwrap()).unwrap();
        assert!(
            NativeWorld::restore_synthetic_component_test(fresh.profile.clone(), forged).is_err()
        );
    }
    let first_route = |w: &NativeWorld| {
        w.records()
            .iter()
            .flat_map(|record| &record.replies)
            .find_map(|reply| match &reply.request.query {
                Query::SourceMotion(MotionQuery::Route(route)) => Some((**route).clone()),
                _ => None,
            })
            .expect("composed source chase issues a route")
    };
    let old_route = first_route(&old);
    let fresh_route = first_route(&fresh);
    assert_eq!(fresh_route.owner, *capability(&fresh));
    let old_receipt = old_route
        .require_owner_ignore_pathfinding(&old.profile)
        .unwrap();
    let receipt = fresh_route
        .require_owner_ignore_pathfinding(&fresh.profile)
        .unwrap();
    assert_eq!(receipt.npc(), owner(&fresh, 2).npc_id());
    assert_eq!(receipt.profile_binding(), fresh.profile_digest());
    assert_eq!(receipt.policy(), OWNER_IGNORE_PATHFINDING_POLICY);
    assert_eq!(receipt.projected_unit_state(), 0);
    assert!(!receipt.ignore_pathfinding());
    assert_ne!(receipt.source_binding(), old_receipt.source_binding());
    let stale = serde_json::to_value(&old_route).unwrap();
    for field in ["source_binding", "capability_binding"] {
        let mut forged = serde_json::to_value(&fresh_route).unwrap();
        assert_ne!(
            forged["owner_ignore_pathfinding"][field],
            stale["owner_ignore_pathfinding"][field]
        );
        forged["owner_ignore_pathfinding"][field] =
            stale["owner_ignore_pathfinding"][field].clone();
        let forged: CreatureRouteRequest = serde_json::from_value(forged).unwrap();
        // Current profile, owner and request identity remain untouched; each
        // downstream source link independently refuses the earlier addon owner.
        assert_eq!(
            forged.require_owner_ignore_pathfinding(&fresh.profile),
            Err(NativeError::AuthorityBinding),
            "{field}"
        );
    }
}
