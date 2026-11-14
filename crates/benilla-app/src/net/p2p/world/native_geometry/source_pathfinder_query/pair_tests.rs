// SPDX-License-Identifier: GPL-3.0-or-later
//! Fault probes invoked by the actual dual-owner component fixture before install.
use super::*;
use benilla_native_nav::{
    ReconstructedSourceChaseFixedCache, ReconstructedSourceWanderTerminal,
    SourceChaseFixedCacheChallenge, SourceWanderReconstructionChallenge,
};

/// Exercise the exact preflight used by install_batch without consuming the
/// certificates needed for its subsequent successful two-entry installation.
pub(in super::super) fn assert_rejections(
    queries: &mut SourcePathfinderQueryProvider,
    plan: &mut ReconstructedConstructorPairPlan,
    wander: &ReconstructedSourceWanderTerminal,
    chase: &ReconstructedSourceChaseFixedCache,
    foreign: &SourcePathfinderQueryProvider,
) {
    assert_eq!(queries.constructor_count(), 0);
    assert_eq!(queries.validate_batch(plan, wander, chase), Ok(()));
    assert_eq!(queries.binding(), foreign.binding());
    assert!(!queries.shares_native_query(foreign));
    let rejected = |queries: &SourcePathfinderQueryProvider,
                    plan: &ReconstructedConstructorPairPlan| {
        assert!(queries.validate_batch(plan, wander, chase).is_err());
        assert_eq!(queries.constructor_count(), 0);
    };

    plan.world[0] ^= 1;
    rejected(queries, plan);
    plan.world[0] ^= 1;
    plan.profile.clock_origin_ms ^= 1;
    rejected(queries, plan);
    plan.profile.clock_origin_ms ^= 1;
    plan.snapshot.policy[0] ^= 1;
    rejected(queries, plan);
    plan.snapshot.policy[0] ^= 1;
    plan.configuration[0] ^= 1;
    rejected(queries, plan);
    plan.configuration[0] ^= 1;
    plan.npc.generation ^= 1;
    rejected(queries, plan);
    plan.npc.generation ^= 1;

    std::mem::swap(&mut plan.wander_construction, &mut plan.chase_construction);
    rejected(queries, plan);
    std::mem::swap(&mut plan.wander_construction, &mut plan.chase_construction);
    std::mem::swap(&mut plan.wander_first, &mut plan.chase_first);
    rejected(queries, plan);
    std::mem::swap(&mut plan.wander_first, &mut plan.chase_first);

    let challenge = std::mem::replace(
        &mut plan.wander_challenge,
        SourceWanderReconstructionChallenge::new(),
    );
    rejected(queries, plan);
    plan.wander_challenge = challenge;
    let challenge = std::mem::replace(
        &mut plan.chase_challenge,
        SourceChaseFixedCacheChallenge::new(),
    );
    rejected(queries, plan);
    plan.chase_challenge = challenge;

    plan.wander_semantic.path_type ^= 8;
    rejected(queries, plan);
    plan.wander_semantic.path_type ^= 8;
    plan.chase_semantic.path_type ^= 8;
    rejected(queries, plan);
    plan.chase_semantic.path_type ^= 8;
    plan.wander_recipe.operation_count ^= 1;
    rejected(queries, plan);
    plan.wander_recipe.operation_count ^= 1;
    plan.chase_recipe.operation_count ^= 1;
    rejected(queries, plan);
    plan.chase_recipe.operation_count ^= 1;
    plan.wander_recipe.operations[0].raw_end[0] ^= 1;
    rejected(queries, plan);
    plan.wander_recipe.operations[0].raw_end[0] ^= 1;
    assert_eq!(plan.chase_recipe.operations.len(), 2);
    plan.chase_recipe.operations.reverse();
    rejected(queries, plan);
    plan.chase_recipe.operations.reverse();

    let original = std::mem::replace(&mut plan.query, foreign.nav_query().retain_default());
    assert_eq!(
        queries.validate_batch(plan, wander, chase),
        Err(ConstructorQueryError::Native(
            benilla_native_nav::Error::Identity
        )),
    );
    assert_eq!(queries.constructor_count(), 0);
    plan.query = original;

    let entry = RegisteredConstructor {
        npc: plan.npc,
        construction: plan.wander_construction,
        first_event: plan.wander_first.clone(),
        reopened: Some(Rc::new(())),
        query: queries.nav_query().retain_default(),
    };
    queries.constructors.push(entry);
    assert_eq!(
        queries.validate_batch(plan, wander, chase),
        Err(ConstructorQueryError::Binding),
    );
    assert_eq!(queries.constructor_count(), 1);
    assert!(queries.constructors.pop().is_some());
    assert_eq!(queries.constructor_count(), 0);
    assert_eq!(queries.validate_batch(plan, wander, chase), Ok(()));
}
