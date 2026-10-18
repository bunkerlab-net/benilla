// SPDX-License-Identifier: GPL-3.0-or-later
//! Capture-layer probe only; altered inputs never become live dispatch or ACK authority.
use super::*;
use std::time::Duration;

pub(in super::super::super) fn assert_changed_native_successor_disables_capture(
    provider: &SourceSamePolygonRouteProvider,
    npc: NpcId,
    construction: motion::QueryBinding,
) {
    let entry = provider
        .candidates
        .iter()
        .find(|entry| entry.npc == npc && entry.construction == construction)
        .unwrap();
    let head = entry.committed.as_ref().unwrap();
    assert!(!head.dispatch.first_construction());
    let original = entry.chase_reconstruction.as_ref().unwrap();
    let before = original.view().unwrap();
    let query = provider.queries.nav_query();
    let mut owner = query.retain_default();
    let from = [5., 15., 0.].map(f32::to_bits);
    let to = [5., 16., 0.].map(f32::to_bits);
    let selected = owner
        .select_for_calculate(before.native.slot, from, to)
        .unwrap();
    let changed = selected
        .loaded_retained_same_polygon(
            head.terminal.chase().unwrap(),
            before.native.config,
            provider.binding.configuration.retained_budget,
            Instant::now() + Duration::from_secs(10),
        )
        .unwrap();
    assert_ne!(changed.compatibility().storage(), head.terminal.storage());
    assert_eq!(
        changed.compatibility().before_storage(),
        head.terminal.storage()
    );
    let mut witness = original.native.clone();
    assert!(witness.record(&changed).is_err());
    assert!(
        witness.is_disabled(),
        "genuine successor changed the complete native cache"
    );

    let mut context = head.context.clone();
    let Query::SourceMotion(MotionQuery::Route(route)) = &mut context.query else {
        unreachable!()
    };
    route.request.origin = super::super::super::source_near_point::exact_point(from).unwrap();
    route.request.launch_origin = route.request.origin;
    route.request.goal = super::super::super::source_near_point::exact_point(to).unwrap();
    route.request.prior_cache = Some(Box::new(head.after_cache.clone()));
    let mut after_cache = head.after_cache.clone();
    after_cache.revision += 1;
    after_cache.poly_refs = changed.compatibility().storage().to_vec();
    after_cache.requested_end = route.request.goal;
    let candidate = SamePolygonCandidate {
        context,
        dispatch: head.dispatch.clone(),
        terminal: SamePolygonTerminal::Chase(changed),
        wander_tail: None,
        wander_prefix: None,
        after_cache,
    };
    let mut staged = Some(original.clone());
    ChaseCapture::accept(&mut staged, &candidate);
    assert!(staged.is_none());
    ChaseCapture::accept(&mut staged, &candidate);
    assert!(
        staged.is_none(),
        "later accepted input cannot restart a disabled capture"
    );

    let constructor = before.constructor.clone();
    let Query::SourceMotion(MotionQuery::Route(route)) = &constructor.query else {
        unreachable!()
    };
    let selected = owner
        .select_for_calculate(
            before.native.slot,
            bits(route.request.origin),
            bits(route.request.goal),
        )
        .unwrap();
    let cold = selected
        .loaded_cold_same_polygon_with_filter(
            before.native.compatibility_version,
            before.native.filter,
            before.native.config,
            provider.binding.configuration.cold_budget,
            Instant::now() + Duration::from_secs(10),
        )
        .unwrap();
    let mut after_cache = head.after_cache.clone();
    after_cache.revision = 1;
    after_cache.poly_refs = cold.compatibility().storage().to_vec();
    after_cache.requested_end = route.request.goal;
    let cold = SamePolygonCandidate {
        dispatch: provider
            .registered_constructor_for(npc, construction)
            .unwrap()
            .1,
        context: constructor,
        terminal: SamePolygonTerminal::Chase(cold),
        wander_tail: None,
        wander_prefix: None,
        after_cache,
    };
    ChaseCapture::accept(&mut staged, &cold);
    assert!(
        staged.is_none(),
        "even cold-shaped input cannot revive the disabled owner"
    );
    let mut fresh = Some(ChaseCapture::default());
    ChaseCapture::accept(&mut fresh, &cold);
    assert_eq!(fresh.unwrap().view().unwrap().native.operation_count, 1);
    assert_eq!(
        original.view().unwrap(),
        before,
        "the actual committed owner was never changed"
    );
}
