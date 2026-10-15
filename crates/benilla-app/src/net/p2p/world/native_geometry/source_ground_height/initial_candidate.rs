// SPDX-License-Identifier: GPL-3.0-or-later
//! Only the first candidate and its ground adjustment, not NearPoint acceptance.
use super::*;
use benilla_sim::{facing, near_point, source_trig};

/// A derived candidate is not evidence that collision selection/LOS accepted it.
/// The ground reply retains the complete frozen context, raw XY and target Z,
/// exact product binding, ticket, ground result and adjusted Z.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InitialCandidateReply {
    pub policy: [u8; 32],
    pub angle: u32,
    pub ground: GroundHeightReply,
}

pub(super) fn policy_digest() -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"benilla-source-initial-ground-candidate-v1");
    for policy in [near_point::POLICY, source_trig::SOFT_POLICY, facing::POLICY] {
        h.update((policy.len() as u64).to_le_bytes());
        h.update(policy.as_bytes());
    }
    for hash in near_point::SOURCE_SHA256
        .iter()
        .chain(source_trig::SOURCE_SHA256.iter())
        .chain(facing::SOURCE_SHA256.iter())
    {
        h.update(hash.as_bytes());
    }
    h.finalize().into()
}

impl BoundGroundHeightQuery<'_, '_> {
    /// Resolve the angle from this frozen request, derive source XY, then query
    /// the existing actual product-backed ground path. No caller-supplied XY,
    /// guessed collision configuration or selector/fallback result is accepted.
    pub(crate) fn derive_initial_candidate(
        &self,
        ticket: u64,
        deadline: Instant,
    ) -> Result<InitialCandidateReply, Failure> {
        if ticket == 0 {
            return Err(Failure::Binding);
        }
        let Query::SourceMotion(MotionQuery::NearPoint(near)) = &self.context.query else {
            return Err(Failure::UnsupportedQuery);
        };
        let (angle, position) = initial_position(&near.request)?;
        let ground = self.query_initial_candidate(
            GroundHeightRequest {
                snapshot: self.provider.binding(),
                context: self.context.clone(),
                ticket,
                position,
            },
            deadline,
        )?;
        Ok(InitialCandidateReply {
            policy: policy_digest(),
            angle,
            ground,
        })
    }
}

pub(super) fn initial_position(
    q: &benilla_sim::creature_motion::NearPointRequest,
) -> Result<(u32, [u32; 3]), Failure> {
    let target = q.target.pose.position;
    let owner = q.owner.position;
    let target_xy = [target.x.as_f32().to_bits(), target.y.as_f32().to_bits()];
    let angle = match q.angle {
        NearPointAngle::CurrentTargetToOwner => facing::bearing_bits(
            target_xy,
            [owner.x.as_f32().to_bits(), owner.y.as_f32().to_bits()],
        )
        .map_err(|_| Failure::Invalid)?,
        NearPointAngle::Absolute(angle) => angle.as_f32().to_bits(),
    };
    // Keep the transient angle as raw bits. A valid subnormal source bearing
    // need not fit the runtime's persistent Real representation.
    let xy = near_point::candidate_bits(target_xy, q.distance.as_f32().to_bits(), angle)
        .map_err(|_| Failure::Invalid)?;
    Ok((angle, [xy[0], xy[1], target.z.as_f32().to_bits()]))
}
