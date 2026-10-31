// SPDX-License-Identifier: GPL-3.0-or-later
//! Private sequential height capability for Object.cpp:2306-2412 at 8ec338a1.
//! Exhaustion restores first XY while carrying the last successful allowed Z.
use super::super::products::vmap::QueryBudget;
use super::*;
use benilla_sim::near_point::{self, selector};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SequentialGroundPhase {
    Primary,
    Free,
    Final,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SequentialGroundReply {
    pub phase: SequentialGroundPhase,
    pub ground: GroundHeightReply,
    /// The source final fallback has no LOS query or vertical acceptance test.
    pub los_ticket: Option<u64>,
}

/// No caller-supplied phase, predecessor, position or Z can advance this sequence.
/// A failed probe permanently poisons it, rather than becoming iterator exhaustion.
pub(crate) struct EmptySelectorGroundSequence<'a, 'products> {
    bound: BoundGroundHeightQuery<'a, 'products>,
    selector: selector::EmptySelector,
    target_xy: [u32; 2],
    distance: u32,
    base_angle: u32,
    first: [u32; 3],
    completed_probes: usize,
    last_allowed_z: Option<u32>,
    done: bool,
    failure: Option<Failure>,
}

impl<'a, 'products> BoundGroundHeightQuery<'a, 'products> {
    pub(crate) fn empty_selector_sequence(
        self,
    ) -> Result<super::EmptySelectorGroundSequence<'a, 'products>, Failure> {
        let Query::SourceMotion(MotionQuery::NearPoint(near)) = &self.context.query else {
            return Err(Failure::UnsupportedQuery);
        };
        let q = &near.request;
        let (base_angle, first) = initial_candidate::initial_position(q)?;
        let target = q.target.pose.position;
        let target_xy = [target.x.as_f32().to_bits(), target.y.as_f32().to_bits()];
        let distance = q.distance.as_f32().to_bits();
        let selector =
            selector::EmptySelector::new(q.owner_bounding_radius.as_f32().to_bits(), distance)
                .map_err(|_| Failure::Invalid)?;
        Ok(EmptySelectorGroundSequence {
            bound: self,
            selector,
            target_xy,
            distance,
            base_angle,
            first,
            completed_probes: 0,
            last_allowed_z: None,
            done: false,
            failure: None,
        })
    }
}

impl EmptySelectorGroundSequence<'_, '_> {
    /// Height and LOS callers share the supplied absolute deadline and work budget.
    /// Stopping early drops the capability; only successful source-order probes can
    /// reach the one final carried-Z query after the actual selector exhausts.
    pub(crate) fn next_with_budget(
        &mut self,
        budget: &mut QueryBudget,
    ) -> Result<Option<super::SequentialGroundReply>, Failure> {
        if let Some(failure) = self.failure {
            return Err(failure);
        }
        if self.done {
            return Ok(None);
        }
        let result = self.next_ground(budget);
        if let Err(failure) = result {
            self.failure = Some(failure);
        }
        result.map(Some)
    }

    fn next_ground(&mut self, budget: &mut QueryBudget) -> Result<SequentialGroundReply, Failure> {
        let (phase, position) = if self.completed_probes == 0 {
            (SequentialGroundPhase::Primary, self.first)
        } else if let Some(offset) = self.selector.next() {
            if self.completed_probes > selector::MAX_OFFSETS {
                return Err(Failure::Invalid);
            }
            let angle = selector::candidate_angle_bits(self.base_angle, offset)
                .map_err(|_| Failure::Invalid)?;
            let xy = near_point::fallback_candidate_bits(self.target_xy, self.distance, angle)
                .map_err(|_| Failure::Invalid)?;
            (SequentialGroundPhase::Free, [xy[0], xy[1], self.first[2]])
        } else {
            // NextUsedAngle has no entries in this exact source selector. Neither
            // an arithmetic failure nor unavailable coverage can enter this branch.
            let z = self.last_allowed_z.ok_or(Failure::Binding)?;
            (
                SequentialGroundPhase::Final,
                [self.first[0], self.first[1], z],
            )
        };
        let final_probe = phase == SequentialGroundPhase::Final;
        let index = if final_probe {
            selector::MAX_OFFSETS + 1
        } else {
            self.completed_probes
        } as u64;
        let request = GroundHeightRequest {
            snapshot: self.bound.provider.binding(),
            context: self.bound.context.clone(),
            ticket: 2 * index + 1,
            position,
        };
        let ground = if final_probe {
            self.bound.query_ground_with_budget(request, budget)?
        } else {
            self.bound
                .query_initial_candidate_with_budget(request, budget)?
        };
        self.last_allowed_z = Some(ground.allowed_z);
        self.completed_probes += 1;
        self.done = final_probe;
        Ok(SequentialGroundReply {
            phase,
            ground,
            los_ticket: (!final_probe).then_some(2 * index + 2),
        })
    }
}
