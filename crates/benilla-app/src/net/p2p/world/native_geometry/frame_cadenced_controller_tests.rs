// SPDX-License-Identifier: GPL-3.0-or-later
//! Frame-cadenced mailbox component only: the permit is scheduled, service is not.
//! Borrowed authored providers are serviced immediately after App::update returns,
//! not inside production process. This does not establish gameplay admission.
use super::{
    core::FrameBudget,
    resident_result_controller_tests::{run, Mode},
};
use avian3d::prelude::PhysicsSystems;
use bevy::{
    diagnostic::FrameCount, prelude::*, time::TimeUpdateStrategy, transform::TransformSystems,
};
use std::time::Duration;

#[derive(Resource, Default)]
struct Permit {
    budget: FrameBudget,
    ready: Option<(u32, usize)>,
    fixed_steps: u64,
}

pub(super) fn install(app: &mut App) {
    app.init_resource::<Permit>();
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
        16,
    )));
    app.add_systems(
        FixedPostUpdate,
        (|frame: Res<FrameCount>, mut permit: ResMut<Permit>| {
            permit.fixed_steps += 1;
            let allowance = permit.budget.remaining(frame.0);
            if allowance > 0 {
                // Reserve the whole allowance: catch-up fixed ticks cannot mint
                // additional passes in the same render frame.
                permit.budget.spend(allowance);
                assert!(permit.ready.is_none());
                permit.ready = Some((frame.0, allowance));
            }
        })
        .after(PhysicsSystems::Last)
        .after(TransformSystems::Propagate),
    );
}

pub(super) fn take_permit(app: &mut App) -> Option<(u32, usize, u64)> {
    let mut permit = app.world_mut().resource_mut::<Permit>();
    permit
        .ready
        .take()
        .map(|(frame, allowance)| (frame, allowance, permit.fixed_steps))
}

#[test]
#[ignore = "runner-sensitive default Route budget; run explicitly in release profile"]
fn frame_cadenced_mailbox_saves_cold_and_first_retained() {
    run(Mode::FrameCadenced);
}

#[test]
#[ignore = "runner-sensitive default Route budget; run explicitly in release profile"]
fn frame_cadenced_mailbox_missed_frames_refuse_without_publication() {
    run(Mode::MissedFrame);
}
