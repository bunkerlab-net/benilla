// SPDX-License-Identifier: GPL-3.0-or-later
//! Loaded dry initialization through actual lethal spell, corpse and respawn paths.
//! Authored no-combat-entry and no sight visits isolate this owner; it never asks the
//! deliberately ground-only synthetic route adapter to certify habitat-3 movement.
use super::*;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

struct ExpiredInitialization<'a, 'b> {
    inner: &'a mut InitialAuthority<'b>,
    rejected: Option<AuthorityRequest>,
}
impl Authority for ExpiredInitialization<'_, '_> {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        if matches!(request.query, Query::CreatureInitialization(_)) {
            assert_eq!(
                self.inner.provider.resolve(request, Instant::now()),
                Err(Failure::Deadline)
            );
            self.rejected = Some(request.clone());
            return Err(NativeError::AuthorityUnavailable(
                "expired initialization snapshot query",
            ));
        }
        self.inner.resolve(request)
    }
}

pub(super) fn cold_draws(world: &NativeWorld) -> Vec<Draw> {
    let value = serde_json::to_value(world.checkpoint().unwrap()).unwrap();
    serde_json::from_value(
        value["state"]["npcs"]["2"]["source_generation"]["generations"][0]["draws"].clone(),
    )
    .unwrap()
}
pub(super) fn assert_query_order(request: &AuthorityRequest, draws: &[Draw]) -> u32 {
    let Query::CreatureInitialization(query) = &request.query else {
        unreachable!()
    };
    let level = draws.iter().find(|draw| matches!(draw.purpose, DrawPurpose::CreatureLevel { npc } if Some(npc) == query.binding.npc)).unwrap();
    assert_eq!(query.before_level_counter, level.first_counter);
    let prefix: Vec<_> = draws.iter().filter(|draw| matches!(draw.purpose,
        DrawPurpose::CreatureModel { npc } | DrawPurpose::CreatureGender { npc } if Some(npc) == query.binding.npc)).cloned().collect();
    assert_eq!(prefix.len(), 2);
    assert_eq!(query.model_gender_draws, prefix);
    assert_eq!(query.first_counter, prefix[0].first_counter);
    assert_eq!(query.model_roll, Some(prefix[0].value as u32));
    assert_eq!(query.gender_roll, Some(prefix[1].value as u32));
    assert_eq!(query.classification_height.bits(), 2.03128f32.to_bits());
    assert_ne!(query.collision_height, query.classification_height);
    query.collision_height.bits()
}
pub(super) fn assert_dry_owner(world: &NativeWorld) {
    let npc = &world.state().npcs[&2];
    let witness = npc.creature_capabilities().unwrap().witness();
    assert_eq!(witness.inhabit_type(), 3);
    assert!(witness.capabilities().unwrap().can_swim());
    assert_eq!(witness.swimming_movement_flags(), 0);
    assert_eq!(npc.initialized_creature().unwrap().unit_flags & 0x8000, 0);
    assert_eq!(npc.initialized_creature().unwrap().unit_bytes_2, 0x1001);
    let facts = witness.initialization().unwrap();
    assert_eq!(facts.request.binding.npc, Some(npc.id));
    assert_eq!(facts.request.binding.world, world.state().world);
    let mut context = facts.request.binding.clone();
    context.tick = world.state().tick;
    witness.validate_context(world.profile(), &context).unwrap();
    context.world[0] ^= 1;
    assert!(witness.validate_context(world.profile(), &context).is_err());
}

#[test]
fn byte_derived_dry_lives_rebind_height_replay_compact_and_reject_partial_initialization() {
    let (products, config) = products::authored_ground_height();
    let participants = NonRenderParticipants::authored_fixture(config.content, config.map);
    let snapshot = products
        .load_map_height(config, Some(&participants))
        .unwrap();
    let mut p = profile(snapshot_binding(&snapshot), [77; 32]);
    p.source_motion
        .as_mut()
        .unwrap()
        .npcs
        .get_mut(&2)
        .unwrap()
        .npc_can_enter_combat = false;
    let AbilityAdmission::Available(spec) = &mut p.abilities[0].admission else {
        unreachable!()
    };
    spec.mana_cost = 0;
    let benilla_sim::classic_spell::MageEffect::SchoolDamage(value) = &mut spec.effects[0] else {
        unreachable!()
    };
    value.base_points = 1000;
    p.validate().unwrap();
    let provider = SourceCreatureInitializationProvider::bind([8; 16], &p, snapshot).unwrap();
    let mut authority = InitialAuthority {
        provider,
        initializations: vec![],
        motion: Default::default(),
    };
    authority.motion.critical_chance = Some(0);
    authority.motion.resist_chance = Some(0);
    authority.motion.visits = Some(false);
    let mut world = NativeWorld::synthetic_component_test_with_initialization(
        p,
        [8; 16],
        f::player(),
        7,
        None,
        &mut authority,
    )
    .unwrap();
    let initial = world.checkpoint().unwrap();
    let mut heights = BTreeSet::from([assert_query_order(
        &authority.initializations[0],
        &cold_draws(&world),
    )]);
    assert_dry_owner(&world);
    let mut completed = false;
    for _ in 0..16 {
        let old = world.state().npcs[&2].id;
        let command = f::cast(&world, 60001);
        world
            .step(world.state().tick + 1, Some(command), &mut authority)
            .unwrap();
        for _ in 0..50 {
            if world.state().npcs[&2].health == 0 {
                break;
            }
            world
                .step(world.state().tick + 1, None, &mut authority)
                .unwrap();
        }
        assert_eq!(world.state().npcs[&2].health, 0, "actual lethal impact");
        assert_dry_owner(&world);
        if old.generation == 0 {
            let mut replay = NativeWorld::restore_synthetic_component_test(
                world.profile().clone(),
                initial.clone(),
            )
            .unwrap();
            replay.replay(world.records()).unwrap();
            assert_eq!(replay.checkpoint().unwrap(), world.checkpoint().unwrap());
        }
        // This is the host compaction boundary after an exact checkpoint commit;
        // old-life initialization evidence must survive without journal records.
        let compact = world.checkpoint().unwrap();
        world.checkpoint_committed(&compact).unwrap();
        assert!(world.records().is_empty());
        world =
            NativeWorld::restore_synthetic_component_test(world.profile().clone(), compact.clone())
                .unwrap();
        let mut saw_expired = false;
        for _ in 0..50 {
            let before = world.checkpoint().unwrap();
            let records = world.records().to_vec();
            let counter = world.state().rng_counter;
            let tick = world.state().tick + 1;
            let mut expired = ExpiredInitialization {
                inner: &mut authority,
                rejected: None,
            };
            let outcome = world.step(tick, None, &mut expired);
            if let Some(rejected) = expired.rejected {
                assert!(matches!(
                    outcome,
                    Err(NativeError::AuthorityUnavailable(
                        "expired initialization snapshot query"
                    ))
                ));
                assert_eq!(world.checkpoint().unwrap(), before);
                assert_eq!(world.records(), records);
                assert_eq!(world.state().rng_counter, counter);
                let record = world.step(tick, None, &mut authority).unwrap().clone();
                let accepted = authority.initializations.last().unwrap();
                assert_eq!(
                    accepted, &rejected,
                    "retry reproduces exact selected model and prefix"
                );
                heights.insert(assert_query_order(accepted, &record.draws));
                let Query::CreatureInitialization(query) = &accepted.query else {
                    unreachable!()
                };
                assert_eq!(query.binding.npc.unwrap().generation, old.generation + 1);
                assert_eq!(query.binding.tick, tick);
                assert_eq!(
                    query.now_ms,
                    world.profile().clock_origin_ms + tick * u64::from(world.profile().tick_ms)
                );
                saw_expired = true;
                break;
            }
            outcome.unwrap();
        }
        assert!(saw_expired, "respawn reached the owned liquid query");
        assert_eq!(world.state().npcs[&2].id.generation, old.generation + 1);
        assert_dry_owner(&world);
        let mut replay =
            NativeWorld::restore_synthetic_component_test(world.profile().clone(), compact)
                .unwrap();
        replay.replay(world.records()).unwrap();
        assert_eq!(replay.checkpoint().unwrap(), world.checkpoint().unwrap());
        let saved = world.checkpoint().unwrap();
        let restored =
            NativeWorld::restore_synthetic_component_test(world.profile().clone(), saved.clone())
                .unwrap();
        assert_eq!(restored.checkpoint().unwrap(), saved);
        if heights.len() == 2 {
            completed = true;
            break;
        }
    }
    assert!(
        completed,
        "real generation draws selected both independently loaded model heights"
    );
    assert_eq!(
        heights,
        BTreeSet::from([(2.875f32 * 1.25).to_bits(), (4.125f32 * 1.25).to_bits()])
    );
    assert!(authority.motion.queries.iter().all(|request| !matches!(
        request.query,
        Query::SourceMotion(MotionQuery::NearPoint(_) | MotionQuery::Route(_))
    )));
    reject_rehashed_history(&world);
    reject_rehashed_model_history(&world);
}

// Mirror the retained evidence's exact wire order to recompute a self-consistent
// width witness. These controls must pass the older independent width check;
// only crossbinding to the actually observed initializer should reject them.
#[derive(serde::Serialize, serde::Deserialize)]
struct ModelEvidence {
    world: [u8; 16],
    npc: benilla_world_runtime::NpcId,
    profile: [u8; 32],
    source: [u8; 32],
    created_tick: u64,
    first_counter: u64,
    end_counter: u64,
    draws: Vec<Draw>,
}
fn rehash_width(mut value: serde_json::Value, source: [u8; 32]) -> CreatureCollisionWidthWitness {
    let evidence: ModelEvidence =
        serde_json::from_value(value["generation_selection"].clone()).unwrap();
    let mut hash = Sha256::new();
    hash.update(b"benilla-generation-model-width-owner-v1\0");
    hash.update(serde_json::to_vec(&(source, evidence)).unwrap());
    let digest: [u8; 32] = hash.finalize().into();
    value["source_binding"] = serde_json::json!(digest);
    serde_json::from_value(value).unwrap()
}
fn reject_rehashed_model_history(world: &NativeWorld) {
    let profile = world.profile();
    let npc = &world.state().npcs[&2];
    assert!(npc.id.generation > 0);
    let width = npc.creature_collision_width().unwrap().witness();
    let facts = npc
        .creature_capabilities()
        .unwrap()
        .witness()
        .initialization()
        .unwrap();
    width
        .validated_model_selection(profile, npc.id, Some(facts))
        .unwrap();
    assert!(width
        .validated_model_selection(profile, npc.id, None)
        .is_err());
    let source = profile
        .creature_collision_heights
        .as_ref()
        .unwrap()
        .source(npc.id.spawn)
        .unwrap()
        .width_source_binding();
    assert_eq!(
        rehash_width(serde_json::to_value(width).unwrap(), source),
        *width
    );
    for mutation in 0..6 {
        let mut value = serde_json::to_value(width).unwrap();
        let mut evidence: ModelEvidence =
            serde_json::from_value(value["generation_selection"].clone()).unwrap();
        match mutation {
            0 => evidence.world[0] ^= 1,
            1 => evidence.created_tick += 1,
            2 => {
                evidence.first_counter += 1;
                evidence.end_counter += 1;
                for draw in &mut evidence.draws {
                    draw.first_counter += 1;
                }
            }
            3 => {
                let draw = &mut evidence.draws[0];
                assert!(matches!(draw.purpose, DrawPurpose::CreatureModel { .. }));
                assert_eq!((draw.low, draw.high), (0, 1));
                // Both authored model roll values select the sole root display.
                draw.value = 1 - draw.value;
            }
            4 | 5 => {
                let index = mutation - 4;
                assert!(matches!(
                    evidence.draws[index].purpose,
                    DrawPurpose::CreatureModel { .. } | DrawPurpose::CreatureGender { .. }
                ));
                evidence.draws[index].consumed += 1;
                evidence.end_counter += 1;
                for draw in evidence.draws.iter_mut().skip(index + 1) {
                    draw.first_counter += 1;
                }
            }
            _ => unreachable!(),
        }
        value["generation_selection"] = serde_json::to_value(evidence).unwrap();
        let forged = rehash_width(value, source);
        forged.validate_for(profile, npc.id).unwrap();
        assert!(
            forged
                .validated_model_selection(profile, npc.id, Some(facts))
                .is_err(),
            "model history case {mutation}"
        );
    }
    // Deliberately authored negative transcripts, not live RNG or dry proof:
    // actual fixture draws each consume one word. Give the two independent
    // transcripts the same enlarged endpoints, but redistribute the extra word
    // between model and gender. Scalar and aggregate-prefix checks alone pass.
    let mut value = serde_json::to_value(width).unwrap();
    let mut evidence: ModelEvidence =
        serde_json::from_value(value["generation_selection"].clone()).unwrap();
    evidence.draws[0].consumed += 1;
    evidence.end_counter += 1;
    for draw in evidence.draws.iter_mut().skip(1) {
        draw.first_counter += 1;
    }
    value["generation_selection"] = serde_json::to_value(&evidence).unwrap();
    let forged = rehash_width(value, source);
    forged.validate_for(profile, npc.id).unwrap();
    let mut foreign_facts = facts.clone();
    foreign_facts.request.model_gender_draws[1].consumed += 1;
    foreign_facts.request.before_level_counter += 1;
    foreign_facts.request.validate_for(profile).unwrap();
    assert_eq!(foreign_facts.request.first_counter, evidence.first_counter);
    assert_eq!(
        foreign_facts.request.before_level_counter,
        evidence.draws[1].first_counter + u64::from(evidence.draws[1].consumed)
    );
    assert!(forged
        .validated_model_selection(profile, npc.id, Some(&foreign_facts))
        .is_err());
}

// Preserve typed struct field order while rehashing. Parsing to Value first sorts
// object keys and would make these tests fail at an unrelated envelope hash.
fn canonical_state_bytes(checkpoint: &Checkpoint) -> Vec<u8> {
    let bytes = serde_json::to_vec(checkpoint).unwrap();
    let marker = b"\"state\":";
    let start = bytes
        .windows(marker.len())
        .position(|w| w == marker)
        .unwrap()
        + marker.len();
    let mut depth = 0usize;
    let mut quoted = false;
    let mut escape = false;
    for (offset, &byte) in bytes[start..].iter().enumerate() {
        if quoted {
            if escape {
                escape = false;
            } else if byte == b'\\' {
                escape = true;
            } else if byte == b'"' {
                quoted = false;
            }
        } else {
            match byte {
                b'"' => quoted = true,
                b'{' | b'[' => depth += 1,
                b'}' | b']' => {
                    depth -= 1;
                    if depth == 0 {
                        return bytes[start..=start + offset].to_vec();
                    }
                }
                _ => {}
            }
        }
    }
    panic!("typed checkpoint state boundary")
}
pub(super) fn rehash(mut value: serde_json::Value) -> Checkpoint {
    let typed: Checkpoint = serde_json::from_value(value.clone()).unwrap();
    let state = canonical_state_bytes(&typed);
    let mut hash = Sha256::new();
    hash.update(b"benilla-native-state-v67\0");
    hash.update(&state);
    let digest: [u8; 32] = hash.finalize().into();
    value["digest"] = serde_json::json!(digest);
    let mut tuple = vec![b'['];
    tuple.extend(&state);
    tuple.push(b',');
    tuple.extend(serde_json::to_vec(&digest).unwrap());
    tuple.push(b',');
    tuple.extend(serde_json::to_vec(&value["history_tip"]).unwrap());
    tuple.push(b']');
    let mut hash = Sha256::new();
    hash.update(b"benilla-native-checkpoint-v67\0");
    hash.update(tuple);
    let checksum: [u8; 32] = hash.finalize().into();
    value["checksum"] = serde_json::json!(checksum);
    serde_json::from_value(value).unwrap()
}
fn reject_rehashed_history(world: &NativeWorld) {
    let original = world.checkpoint().unwrap();
    assert_eq!(rehash(serde_json::to_value(&original).unwrap()), original);
    for mutation in 0..7 {
        let mut value = serde_json::to_value(&original).unwrap();
        let old = &mut value["state"]["npcs"]["2"]["source_generation"]["generations"][0]
            ["initialization"];
        assert!(!old.is_null());
        match mutation {
            0 => *old = serde_json::Value::Null,
            1 => {
                let byte = old["request"]["snapshot"]["source_epoch"][0]
                    .as_u64()
                    .unwrap();
                old["request"]["snapshot"]["source_epoch"][0] = serde_json::json!(byte ^ 1);
            }
            2 => old["request"]["binding"]["world"][0] = serde_json::json!(255),
            3 => old["request"]["before_level_counter"] = serde_json::json!(999),
            4 => old["request"]["model_id"] = serde_json::json!(999),
            5 => old["request"]["now_ms"] = serde_json::json!(999),
            6 => {
                old["status"] = serde_json::json!("UnderWater");
                old["data"] = serde_json::json!({ "entry":1,"type_flags":1,"level":20f32.to_bits(),"depth_level":0f32.to_bits() });
            }
            _ => unreachable!(),
        }
        let forged = rehash(value);
        assert!(
            NativeWorld::restore_synthetic_component_test(world.profile().clone(), forged).is_err(),
            "historical case {mutation}"
        );
    }
}
