// SPDX-License-Identifier: GPL-3.0-or-later
//! Source range failure messages from authored rows, without native admission.
use super::*;
use n::{synthetic_fixture as f, synthetic_spell_fixture as source};

fn world(distance: f32) -> n::NativeWorld {
    let (mut profile, mut initial, mut rows) = source::inputs();
    profile.npcs[0].ai.spawn = f::point(distance);
    rows.spells[0].range.minimum = f::float(5.0);
    rows.spells[0].words[21] &= !1;
    initial.spells[0].words[21] = rows.spells[0].words[21];
    let n::AbilityAdmission::Available(spec) = &mut profile.abilities[0].admission else {
        unreachable!()
    };
    spec.movement_interrupts = false;
    profile
        .initialize_player(&initial, f::float(100.0), 1)
        .unwrap();
    profile.initialize_spell_authority(&initial, rows).unwrap();
    n::NativeWorld::synthetic_component_test(profile, [8; 16], f::player(), 7).unwrap()
}

fn feedback(events: &[SessionEvent]) -> Vec<(&'static str, Option<u8>)> {
    events
        .iter()
        .filter_map(|event| match event {
            SessionEvent::CastResult {
                success,
                reason,
                arg,
                ..
            } => {
                assert_eq!(*arg, None);
                Some((if *success { "accepted" } else { "failed" }, *reason))
            }
            SessionEvent::SpellFailedOther { .. } => Some(("interrupted", None)),
            SessionEvent::Notification { .. } => Some(("notification", None)),
            _ => None,
        })
        .collect()
}

#[test]
fn source_prepare_range_failure_is_one_precise_cast_result_without_interrupt_or_notification() {
    for (distance, code) in [(36.0, 0x59), (7.0, 0x76)] {
        let mut w = world(distance);
        let mut projection = NativeProjection::new(&w, w.state().player, &tests::Catalog).unwrap();
        let input = f::cast(&w, 133);
        w.step(1, Some(input), &mut source::spatial_only()).unwrap();
        let events = projection.advance(&w).unwrap();
        assert_eq!(feedback(&events), vec![("failed", Some(code))]);
        assert!(!events.iter().any(|event| matches!(
            event,
            SessionEvent::SpellStart { .. }
                | SessionEvent::SpellGo { .. }
                | SessionEvent::SpellDamageLog(_)
                | SessionEvent::PeriodicAuraLog(_)
        )));
    }
}

#[test]
fn source_release_range_failure_reports_reason_before_interrupt_without_another_ack() {
    for (distance, destination, code) in [(33.0, -3.000_004, 0x59), (10.0, 2.000_001, 0x76)] {
        let mut w = world(distance);
        let mut projection = NativeProjection::new(&w, w.state().player, &tests::Catalog).unwrap();
        let mut authority = source::spatial_only();
        let input = f::cast(&w, 133);
        w.step(1, Some(input), &mut authority).unwrap();
        // The existing input acceptance ack is outside this failure-message slice.
        assert_eq!(
            feedback(&projection.advance(&w).unwrap()),
            vec![("accepted", None)]
        );
        w.step(2, None, &mut authority).unwrap();
        projection.advance(&w).unwrap();
        let input = projection
            .movement_pose(
                &w,
                f::point(destination),
                w.state().orientation,
                benilla_sim::creature_motion::GroundMovementFlags::STOPPED,
            )
            .unwrap();
        w.step(3, Some(input), &mut authority).unwrap();
        let events = projection.advance(&w).unwrap();
        assert_eq!(
            feedback(&events),
            vec![("failed", Some(code)), ("interrupted", None)]
        );
        assert!(!events.iter().any(|event| matches!(
            event,
            SessionEvent::SpellGo { .. }
                | SessionEvent::SpellDamageLog(_)
                | SessionEvent::PeriodicAuraLog(_)
        )));
    }
}

#[test]
fn non_range_prepare_failure_retains_its_existing_projection() {
    let mut w = world(36.0);
    let mut projection = NativeProjection::new(&w, w.state().player, &tests::Catalog).unwrap();
    let input = f::cast(&w, 133);
    let mut authority = source::spatial_only();
    authority.line_of_sight = Some(ai::movement::Check::Failed);
    w.step(1, Some(input), &mut authority).unwrap();
    let events = projection.advance(&w).unwrap();
    assert_eq!(
        feedback(&events),
        vec![
            ("failed", None),
            ("interrupted", None),
            ("notification", None)
        ]
    );
}

#[test]
fn prepare_range_correlation_requires_this_inputs_exact_callback_and_rejection() {
    let w = world(36.0);
    let input = f::cast(&w, 133).input;
    let n::Command::Cast { target, .. } = input.command else {
        unreachable!()
    };
    let failed = n::Event::Spell {
        cast_id: 7,
        ability: 133,
        target,
        event: spell::SpellEvent::Failed(spell::CastFailure::Rejected(0x59)),
    };
    let rejected = n::Event::Rejected(n::Rejection::CastFailed(spell::CastFailure::Rejected(0x59)));
    let events = [failed.clone(), rejected.clone()];
    assert_eq!(prepare_range_failure(&events, &input), Some((7, 0x59)));
    for changed in [
        n::Command::Cast {
            ability: 60001,
            target,
        },
        n::Command::Cast {
            ability: 133,
            target: NpcId {
                generation: target.generation + 1,
                ..target
            },
        },
        n::Command::Move {
            destination: f::point(0.0),
        },
    ] {
        let mut changed_input = input.clone();
        changed_input.command = changed;
        assert_eq!(prepare_range_failure(&events, &changed_input), None);
    }
    for changed in [
        n::Event::InputResult {
            sequence: input.sequence + 1,
            accepted: false,
        },
        n::Event::Rejected(n::Rejection::UnknownTarget),
        n::Event::Rejected(n::Rejection::CastFailed(spell::CastFailure::Rejected(0x76))),
    ] {
        assert_eq!(
            prepare_range_failure(&[failed.clone(), changed.clone(), rejected.clone()], &input),
            None
        );
        assert_eq!(
            prepare_range_failure(&[changed, failed.clone(), rejected.clone()], &input),
            None
        );
    }
}

#[test]
fn legacy_external_range_code_keeps_generic_fixture_projection() {
    struct ExternalRange(n::synthetic_fixture::SyntheticAuthority);
    impl n::Authority for ExternalRange {
        fn resolve(
            &mut self,
            request: &n::AuthorityRequest,
        ) -> Result<n::AuthorityReply, n::NativeError> {
            let mut reply = n::Authority::resolve(&mut self.0, request)?;
            if let n::Facts::Spell(facts) = &mut reply.facts {
                facts.other_failure = Some(0x59);
            }
            Ok(reply)
        }
    }
    let mut w = f::world();
    let mut projection = NativeProjection::new(&w, w.state().player, &tests::Catalog).unwrap();
    let input = f::cast(&w, 60001);
    w.step(1, Some(input), &mut ExternalRange(Default::default()))
        .unwrap();
    assert_eq!(
        feedback(&projection.advance(&w).unwrap()),
        vec![
            ("failed", None),
            ("interrupted", None),
            ("notification", None)
        ]
    );
}
