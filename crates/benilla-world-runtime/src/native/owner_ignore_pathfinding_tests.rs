// SPDX-License-Identifier: GPL-3.0-or-later
//! Invented source rows exercise real runtime issuance; no raw content admission.
use super::{
    synthetic_combat_fixture as combat, synthetic_fixture as f, synthetic_motion_fixture as motion,
    *,
};
use benilla_sim::creature_motion as m;

fn profile() -> Profile {
    let mut p = combat::profile();
    p.npcs[0].ai.spawn = f::point(8.0);
    p.initialize_creature_capabilities(motion::creature_capability_sources(&p))
        .unwrap();
    p
}
fn world(p: Profile) -> NativeWorld {
    NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 1234).unwrap()
}
fn step(w: &mut NativeWorld, a: &mut impl Authority) {
    w.step(w.state().tick + 1, None, a).unwrap();
}
fn route(q: &AuthorityRequest) -> Option<&CreatureRouteRequest> {
    match &q.query {
        Query::SourceMotion(MotionQuery::Route(r)) => Some(r),
        _ => None,
    }
}
fn captured(p: Profile) -> (NativeWorld, CreatureRouteRequest) {
    let mut w = world(p);
    let mut a = motion::SyntheticMotionAuthority::default();
    loop {
        assert!(w.state().tick < 10, "missing real Route");
        step(&mut w, &mut a);
        if let Some(r) = w
            .last_tick()
            .unwrap()
            .replies
            .iter()
            .find_map(|r| route(&r.request))
        {
            let r = r.clone();
            return (w, r);
        }
    }
}

#[test]
fn exact_source_mask_is_distinct_from_other_unit_bits_and_creature_extra_flags() {
    assert_eq!(UNIT_STAT_IGNORE_PATHFINDING, 0x1000_0000);
    // Unit.h:332/351/352 and :402; Home clears this mask, preserving IGNORE.
    let melee_chase = 0x0000_0001 | 0x0002_0000 | 0x0004_0000;
    let all_dyn_states: u32 = !(0x0100_0000 | 0x0400_0000 | 0x0800_0000 | 0x1000_0000);
    for state in [0, melee_chase, 0x2000, 0x4000, 0x0000_6000] {
        assert_eq!(state & UNIT_STAT_IGNORE_PATHFINDING, 0);
        assert_eq!((state & !all_dyn_states) & UNIT_STAT_IGNORE_PATHFINDING, 0);
        let ignoring = state | UNIT_STAT_IGNORE_PATHFINDING;
        assert_ne!(ignoring & UNIT_STAT_IGNORE_PATHFINDING, 0);
        assert_ne!(
            (ignoring & !all_dyn_states) & UNIT_STAT_IGNORE_PATHFINDING,
            0
        );
    }
    assert_eq!(OWNER_IGNORE_PATHFINDING_SOURCE_SHA256.len(), 6);
}

#[test]
fn runtime_route_receipt_has_source_profile_policy_generation_and_survives_replay_restore() {
    let p = profile();
    let initial = world(p.clone()).checkpoint().unwrap();
    let (w, route) = captured(p);
    let receipt = route.require_owner_ignore_pathfinding(w.profile()).unwrap();
    assert_eq!(receipt.npc(), w.state().npcs[&2].id);
    assert_eq!(receipt.profile_binding(), w.profile().digest());
    assert_eq!(receipt.policy(), OWNER_IGNORE_PATHFINDING_POLICY);
    assert_ne!(receipt.source_binding(), [0; 32]);
    assert_eq!(receipt.projected_unit_state(), 0);
    assert!(!receipt.ignore_pathfinding());
    assert_eq!(route.request.kind, m::RouteKind::ChaseInitial);
    let roundtrip: CreatureRouteRequest =
        serde_json::from_slice(&serde_json::to_vec(&route).unwrap()).unwrap();
    assert_eq!(
        roundtrip
            .require_owner_ignore_pathfinding(w.profile())
            .unwrap(),
        receipt
    );
    let records = decode_records(&encode_records(w.records()).unwrap()).unwrap();
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile().clone(), initial).unwrap();
    replay.replay(&records).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
    let restored = NativeWorld::restore_synthetic_component_test(
        w.profile().clone(),
        Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(restored.state_digest(), w.state_digest());
    // This invariant is independent of endpoint/cache data; outer authority binds them.
    let mut other_goal = route.clone();
    other_goal.request.goal = f::point(19.0);
    assert_eq!(
        other_goal
            .require_owner_ignore_pathfinding(w.profile())
            .unwrap(),
        receipt
    );
}

#[test]
fn capability_and_model_catalogs_without_interaction_sources_do_not_mint_proof() {
    for mut p in [profile(), synthetic_creature_model_fixture::profile()] {
        p.interactions = None;
        let (w, r) = captured(p);
        assert!(r.owner_ignore_pathfinding.is_none());
        assert_eq!(
            r.require_owner_ignore_pathfinding(w.profile()),
            Err(NativeError::AuthorityUnavailable(
                "owned IGNORE_PATHFINDING state"
            ))
        );
        let mut json = serde_json::to_value(&r).unwrap();
        json.as_object_mut()
            .unwrap()
            .remove("owner_ignore_pathfinding");
        assert!(serde_json::from_value::<CreatureRouteRequest>(json).is_err());
    }
}

#[test]
fn invalid_present_bit_and_unknown_or_present_mutation_audits_reject_atomically() {
    for bit in [1, 0x2000, 0x4000, UNIT_STAT_IGNORE_PATHFINDING] {
        let mut p = profile();
        let before = p.clone();
        let mut input = combat::source_rows(&p);
        input.npcs[0].control.unit_state = bit;
        assert_eq!(
            p.initialize_interactions(input),
            Err(NativeError::UnsupportedMechanic(
                "initial interaction control state"
            ))
        );
        assert_eq!(p, before);
    }
    for field in 0..3 {
        for evidence in [InteractionEvidence::Unknown, InteractionEvidence::Present] {
            let mut p = profile();
            let before = p.clone();
            let mut input = combat::source_rows(&p);
            match field {
                0 => input.audit.external_state_mutations = evidence,
                1 => input.audit.controlled_or_summoned_units = evidence,
                _ => input.audit.npc_aura_or_script_hooks = evidence,
            }
            assert!(
                matches!(p.initialize_interactions(input), Err(NativeError::InteractionEvidence { observed, .. }) if observed == evidence)
            );
            assert_eq!(p, before);
        }
    }
    for evidence in [m::Evidence::Unknown, m::Evidence::Present] {
        let mut p = profile();
        p.source_motion
            .as_mut()
            .unwrap()
            .npcs
            .get_mut(&2)
            .unwrap()
            .npc_policy
            .audit
            .combat_scripts = evidence;
        assert!(p.validate().is_err());
    }
    let mut p = profile();
    let CreatureCapabilityRows::Complete(mut rows) = motion::creature_capability_sources(&p) else {
        unreachable!()
    };
    rows[0].external_features = CreatureCapabilityRows::Missing;
    assert!(p
        .initialize_creature_capabilities(CreatureCapabilityRows::Complete(rows))
        .is_err());
}

#[test]
fn closure_uses_staged_ability_set_not_only_initial_profile() {
    let w = world(profile());
    let p = w.profile();
    let npc = &w.state().npcs[&2];
    let base =
        OwnerIgnorePathfindingReceipt::mint(p, w.state(), npc.id, npc.creature_capabilities())
            .unwrap()
            .unwrap();
    let quests = world(synthetic_quest_fixture::profile());
    let mut staged = w.state().clone();
    staged.quest_progression = quests.state().quest_progression.clone();
    let live = &mut staged.quest_progression.as_mut().unwrap().abilities;
    *live = p.abilities.clone();
    live.retain(|a| matches!(a.admission, AbilityAdmission::Available(_)));
    let learned =
        OwnerIgnorePathfindingReceipt::mint(p, &staged, npc.id, npc.creature_capabilities())
            .unwrap()
            .unwrap();
    assert_ne!(learned.source_binding(), base.source_binding());
    if let AbilityAdmission::Available(spec) =
        &mut staged.quest_progression.as_mut().unwrap().abilities[0].admission
    {
        spec.effects[0] = spell::MageEffect::Unsupported {
            opcode: 6,
            aura: 12,
            trigger_spell: 0,
        };
    }
    assert_eq!(
        OwnerIgnorePathfindingReceipt::mint(p, &staged, npc.id, npc.creature_capabilities()),
        Err(NativeError::UnsupportedMechanic(
            "interaction-changing active effect"
        ))
    );
    assert!(p.validate().is_ok(), "initial profile alone remains valid");
}

#[test]
fn forged_receipt_fields_and_request_identity_are_refused() {
    let (w, r) = captured(profile());
    for field in [
        "policy",
        "npc",
        "profile_binding",
        "source_binding",
        "capability_binding",
        "projected_unit_state",
        "live_abilities",
    ] {
        let mut json = serde_json::to_value(&r).unwrap();
        let receipt = &mut json["owner_ignore_pathfinding"];
        match field {
            "policy" => receipt[field] = "invented-owner".into(),
            "npc" => receipt[field]["generation"] = 1.into(),
            "projected_unit_state" => receipt[field] = UNIT_STAT_IGNORE_PATHFINDING.into(),
            "live_abilities" => receipt[field] = serde_json::json!([]),
            _ => receipt[field][0] = ((receipt[field][0].as_u64().unwrap() + 1) % 256).into(),
        }
        let forged: CreatureRouteRequest = serde_json::from_value(json).unwrap();
        assert_eq!(
            forged.require_owner_ignore_pathfinding(w.profile()),
            Err(NativeError::AuthorityBinding),
            "{field}"
        );
    }
    for field in 0..5 {
        let mut forged = r.clone();
        match field {
            0 => forged.request.binding.identity.creature += 1,
            1 => forged.request.binding.identity.map += 1,
            2 => forged.request.binding.identity.content.0[0] ^= 1,
            3 => forged.request.binding.identity.geometry.0[0] ^= 1,
            _ => forged.owner_position = f::point(99.0),
        }
        assert_eq!(
            forged.require_owner_ignore_pathfinding(w.profile()),
            Err(NativeError::AuthorityBinding)
        );
    }
}

#[test]
fn chase_and_home_preserve_the_bit_without_claiming_full_unit_state_zero() {
    let mut p = profile();
    p.npcs[0].ai.hard_leash_radius = Some(f::real(1.0));
    let mut w = world(p);
    let mut a = combat::SpatialOnly::default();
    let mut chase = None;
    loop {
        assert!(w.state().tick < 40, "missing Home");
        step(&mut w, &mut a);
        for r in w
            .last_tick()
            .unwrap()
            .replies
            .iter()
            .filter_map(|r| route(&r.request))
        {
            let receipt = r.require_owner_ignore_pathfinding(w.profile()).unwrap();
            assert!(!receipt.ignore_pathfinding());
            match r.request.kind {
                m::RouteKind::ChaseInitial => chase = Some(receipt.clone()),
                m::RouteKind::HomeForceDestination => {
                    assert_eq!(Some(receipt), chase.as_ref());
                    assert_eq!(r.owner_position, r.request.origin);
                    assert_ne!(r.owner_position, r.request.launch_origin);
                    return;
                }
                _ => {}
            }
        }
    }
}

#[test]
fn actual_melee_then_replan_keeps_the_single_bit_receipt() {
    let (mut w, initial) = captured(profile());
    let receipt = initial
        .require_owner_ignore_pathfinding(w.profile())
        .unwrap()
        .clone();
    let mut a = combat::SpatialOnly::default();
    loop {
        assert!(w.state().tick < 80, "missing actual melee");
        step(&mut w, &mut a);
        if w.last_tick()
            .unwrap()
            .events
            .iter()
            .any(|e| matches!(e, Event::Melee { .. }))
        {
            break;
        }
    }
    let input = OwnedInput {
        authenticated_owner: w.state().player.owner,
        input: Input {
            world: w.state().world,
            tick: w.state().tick + 1,
            player: w.state().player,
            sequence: w.state().next_sequence,
            command: Command::MovementPose {
                destination: f::point(-8.0),
                orientation: Orientation::ZERO,
                movement_flags: m::GroundMovementFlags::STOPPED,
            },
        },
    };
    w.step(w.state().tick + 1, Some(input), &mut a).unwrap();
    loop {
        if let Some(r) = w
            .last_tick()
            .unwrap()
            .replies
            .iter()
            .find_map(|r| route(&r.request))
        {
            assert_eq!(
                r.require_owner_ignore_pathfinding(w.profile()).unwrap(),
                &receipt
            );
            assert!(!receipt.ignore_pathfinding());
            break;
        }
        assert!(w.state().tick < 100, "missing post-melee replan");
        step(&mut w, &mut a);
    }
}

#[test]
fn respawn_reissues_current_generation_and_old_receipt_cannot_bind_new_owner() {
    let mut p = profile();
    p.npcs[0].ai.corpse_decay_ms = 100;
    p.npcs[0].ai.respawn_delay_seconds = 1;
    let (mut w, old_route) = captured(p);
    w.state.npcs.get_mut(&2).unwrap().health = 0;
    let mut a = combat::SpatialOnly::default();
    a.base.visits = Some(false);
    while w.state().npcs[&2].id.generation == 0 {
        assert!(w.state().tick < 30);
        step(&mut w, &mut a);
    }
    a.base.visits = Some(true);
    loop {
        assert!(w.state().tick < 40);
        step(&mut w, &mut a);
        if let Some(r) = w
            .last_tick()
            .unwrap()
            .replies
            .iter()
            .find_map(|r| route(&r.request))
        {
            let receipt = r.require_owner_ignore_pathfinding(w.profile()).unwrap();
            assert_eq!(receipt.npc().generation, 1);
            let mut forged = r.clone();
            forged.owner_ignore_pathfinding = old_route.owner_ignore_pathfinding;
            assert_eq!(
                forged.require_owner_ignore_pathfinding(w.profile()),
                Err(NativeError::AuthorityBinding)
            );
            break;
        }
    }
}

#[test]
fn forged_echo_and_replay_receipt_roll_back_and_retry_exactly() {
    struct Corrupt {
        base: combat::SpatialOnly,
    }
    impl Authority for Corrupt {
        fn resolve(&mut self, q: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
            let mut reply = self.base.resolve(q)?;
            if let Query::SourceMotion(MotionQuery::Route(r)) = &mut reply.request.query {
                r.owner_ignore_pathfinding = None;
            }
            Ok(reply)
        }
    }
    let mut w = world(profile());
    let mut good = combat::SpatialOnly::default();
    step(&mut w, &mut good);
    let before = w.checkpoint().unwrap();
    assert!(w
        .step(
            2,
            None,
            &mut Corrupt {
                base: Default::default()
            }
        )
        .is_err());
    assert_eq!(w.checkpoint().unwrap(), before);
    step(&mut w, &mut good);
    let mut record = w.last_tick().unwrap().clone();
    let reply = record
        .replies
        .iter_mut()
        .find(|r| route(&r.request).is_some())
        .unwrap();
    let Query::SourceMotion(MotionQuery::Route(r)) = &mut reply.request.query else {
        unreachable!()
    };
    r.owner_ignore_pathfinding = None;
    let mut restored =
        NativeWorld::restore_synthetic_component_test(w.profile().clone(), before.clone()).unwrap();
    assert!(restored.replay(&[record]).is_err());
    assert_eq!(restored.checkpoint().unwrap(), before);
    step(&mut restored, &mut combat::SpatialOnly::default());
    assert_eq!(restored.last_tick(), w.last_tick());
}

#[test]
fn pre_receipt_native28_checkpoint_record_and_replay_are_rejected() {
    let (w, _) = captured(profile());
    let cp = w.checkpoint().unwrap();
    let mut old = serde_json::to_value(&cp).unwrap();
    old["version"] = 28.into();
    old["state"]["version"] = 28.into();
    old["state"]["policy"] = "benilla-native-surface-player-environment-v28".into();
    let old: Checkpoint = serde_json::from_value(old).unwrap();
    assert!(Checkpoint::decode(&old.encode().unwrap()).is_err());
    assert!(NativeWorld::restore_synthetic_component_test(w.profile().clone(), old).is_err());
    let mut records = w.records().to_vec();
    records[0].version = 28;
    assert!(decode_records(&encode_records(&records).unwrap()).is_err());
    let mut replay = world(w.profile().clone());
    assert!(replay.replay(&records).is_err());
    assert_eq!(NATIVE_FORMAT_VERSION, 39);
    assert_eq!(NATIVE_POLICY, "benilla-native-creature-equipment-v39");
    assert_eq!(crate::FORMAT_VERSION, 2);
}

#[test]
fn consistent_earlier_ability_receipt_cannot_replace_current_frame_or_replay_receipt() {
    // Authored progression rows with no questgiver or quest hooks. The public
    // initializer and checkpoint restore validate this state before either mint.
    let (mut p, input, mut rows) = synthetic_quest_fixture::setup();
    p.npcs.truncate(1);
    p.npcs[0].max_health = 3000;
    p.npcs[0].ai.spawn = f::point(8.0);
    p.npcs[0].faction_template = 2;
    p.npcs[0].ai.reaction = ai::Reaction::Aggressive;
    rows.quests.clear();
    rows.relations.starters.clear();
    rows.relations.finishers.clear();
    rows.npc_rows.clear();
    rows.kill.creatures.retain(|r| r.entry == p.npcs[0].entry);
    p.initialize_quest_progression(&input, rows).unwrap();
    p.source_motion = Some(motion::genesis(&p, 2));
    combat::install(&mut p);
    p.initialize_creature_capabilities(motion::creature_capability_sources(&p))
        .unwrap();
    p.admin = Some(AdminPolicy {
        world: [8; 16],
        world_owner: f::player().owner,
        allowlist: Default::default(),
        revision: 7,
    });
    p.validate().unwrap();
    let mut w = world(p);
    w = NativeWorld::restore_synthetic_component_test(w.profile().clone(), w.checkpoint().unwrap())
        .unwrap();
    let npc = &w.state().npcs[&2];
    let earlier = OwnerIgnorePathfindingReceipt::mint(
        w.profile(),
        w.state(),
        npc.id,
        npc.creature_capabilities(),
    )
    .unwrap()
    .unwrap();
    step(&mut w, &mut combat::SpatialOnly::default());
    let before = w.checkpoint().unwrap();
    let input = synthetic_quest_fixture::command(
        &w,
        Command::Admin(AdminRequest {
            key: [1; 16],
            world: w.state().world,
            player: w.state().player,
            policy_revision: 7,
            operation: AdminOperation::GainOneLevel,
        }),
    );
    struct Earlier {
        receipt: OwnerIgnorePathfindingReceipt,
        profile: Profile,
        base: combat::SpatialOnly,
    }
    impl Authority for Earlier {
        fn resolve(&mut self, q: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
            let mut reply = self.base.resolve(q)?;
            if let Query::SourceMotion(MotionQuery::Route(r)) = &mut reply.request.query {
                let current = r.require_owner_ignore_pathfinding(&self.profile).unwrap();
                assert_eq!(current.npc(), self.receipt.npc());
                assert_eq!(current.profile_binding(), self.receipt.profile_binding());
                assert_ne!(current.source_binding(), self.receipt.source_binding());
                r.owner_ignore_pathfinding = Some(self.receipt.clone());
                // It is independently valid historical evidence, not a broken hash.
                r.require_owner_ignore_pathfinding(&self.profile).unwrap();
            }
            Ok(reply)
        }
    }
    let mut corrupt = Earlier {
        receipt: earlier.clone(),
        profile: w.profile().clone(),
        base: Default::default(),
    };
    assert!(w.step(2, Some(input.clone()), &mut corrupt).is_err());
    assert_eq!(w.checkpoint().unwrap(), before);
    w.step(2, Some(input), &mut combat::SpatialOnly::default())
        .unwrap();
    assert_eq!(w.player_level(), 2);
    let mut record = w.last_tick().unwrap().clone();
    let reply = record
        .replies
        .iter_mut()
        .find(|r| route(&r.request).is_some())
        .unwrap();
    let Query::SourceMotion(MotionQuery::Route(r)) = &mut reply.request.query else {
        unreachable!()
    };
    assert_ne!(
        r.owner_ignore_pathfinding
            .as_ref()
            .unwrap()
            .source_binding(),
        earlier.source_binding()
    );
    r.owner_ignore_pathfinding = Some(earlier);
    r.require_owner_ignore_pathfinding(w.profile()).unwrap();
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile().clone(), before.clone()).unwrap();
    assert!(replay.replay(&[record]).is_err());
    assert_eq!(replay.checkpoint().unwrap(), before);
}
