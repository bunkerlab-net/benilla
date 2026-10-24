// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f,
// Object.cpp:1252–1279,1361–1367,1818–1847; GridNotifiers.h:486–504;
// CellImpl.h:37–193; ObjectDefines.h:32; Maps/GridDefines.h:38.
// Upstream GPL-2.0-or-later, Copyright CMaNGOS contributors.
//! Bounded, complete loaded world-camera membership and strict SAY distance.
//!
//! Membership is a trusted source traversal fact, like SceneVisitorFacts. The
//! runtime does not recreate cell membership from a disk or a visibility list.

use super::super::{InteractionMoment, NativeError, VisitorParticipant};
use benilla_sim::{creature_ai::movement::Check, creature_ai::Real, creature_motion::Pose};
use serde::{Deserialize, Serialize};

// MAX_VISIBILITY_DISTANCE = SIZE_OF_GRIDS = 533.33333f, not the speaker's AOI.
const MAX_VISIBILITY_DISTANCE_BITS: u32 = 0x4405_5555;

/// A request repeats the immutable session selection; it does not admit it.
/// Only SoloSelfCamera can be selected by the bounded runtime policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventAiAudienceSelection {
    SoloSelfCamera,
    Unknown,
    FarSight,
    AdditionalRecipients,
}

/// The camera body is the server WorldObject, not the rendered camera position.
/// In the admitted domain the owner and body are the same singleton player.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventAiCamera {
    pub owner: crate::PlayerId,
    pub body: VisitorParticipant,
    pub body_pose: Pose,
}

/// Fresh callback-time state supplied by the runtime, never by the authority.
/// All fields are fixed-size. No action, random word, or selected text is exposed
/// for the authority to choose. The opaque digest identifies the live callback.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventAiAudienceRequest {
    pub callback: crate::Digest,
    pub world: crate::WorldId,
    pub tick: u64,
    pub moment: InteractionMoment,
    pub catalog: crate::Digest,
    pub speaker: crate::NpcId,
    pub map: u32,
    pub speaker_pose: Pose,
    pub player: crate::PlayerId,
    pub player_pose: Pose,
    pub camera: EventAiCamera,
    pub speaker_bounding_radius: Real,
    pub speaker_combat_reach: Real,
    pub body_combat_reach: Real,
    /// Configured ListenRange.Say before the speaker's visibility multiplier.
    pub say_range: Real,
    pub visibility_distance: Real,
    pub selection: EventAiAudienceSelection,
}

/// Complete source-loaded camera coverage, including the zero-visited case.
/// Exactly one of world_camera/not_visited must contain the selected camera.
/// Fixed single slots bound decoding and make an extra recipient unrepresentable.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum EventAiAudienceFacts {
    Complete {
        request: Box<EventAiAudienceRequest>,
        /// Attests the full Cell::VisitWorldObjects traversal and absence of
        /// unrepresented cameras, not merely coverage of the selected body.
        loaded_membership: Check,
        #[serde(deserialize_with = "required_camera")]
        world_camera: Option<Box<EventAiCamera>>,
        #[serde(deserialize_with = "required_camera")]
        not_visited: Option<Box<EventAiCamera>>,
    },
    Missing,
    Unknown,
    UnrepresentedParticipants,
}

fn required_camera<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Box<EventAiCamera>>, D::Error> {
    Option::<Box<EventAiCamera>>::deserialize(deserializer)
}

fn nonnegative(value: Real) -> Result<(), NativeError> {
    if value.as_f32() < 0.0 {
        return Err(NativeError::InvalidState);
    }
    Ok(())
}

impl EventAiAudienceRequest {
    /// Object::MonsterText multiplies the configured range in binary32.
    pub fn effective_say_range(&self) -> Result<Real, NativeError> {
        nonnegative(self.say_range)?;
        nonnegative(self.visibility_distance)?;
        let visibility = self.visibility_distance.as_f32();
        let multiplier = if visibility >= 400.0 {
            4.0
        } else if visibility >= 200.0 {
            2.0
        } else {
            1.0
        };
        Ok(Real::new(self.say_range.as_f32() * multiplier)?)
    }

    /// Cell::Visit adds the source object's bounding radius before applying its
    /// constant visibility cap. This is a cell-search radius, not a body cutoff.
    /// Source visits the standing cell for zero radius; otherwise it visits the
    /// normalized cell rectangle, standing-first, or VisitCircle's octagonal
    /// footprint when BOTH cell-index spans exceed four. No unloaded cells are
    /// created (VisitWorldObjects defaults dont_load=true).
    pub fn traversal_radius(&self) -> Result<Real, NativeError> {
        nonnegative(self.speaker_bounding_radius)?;
        let radius = self.effective_say_range()?.as_f32() + self.speaker_bounding_radius.as_f32();
        Ok(Real::new(
            radius.min(f32::from_bits(MAX_VISIBILITY_DISTANCE_BITS)),
        )?)
    }

    fn validate_request(&self) -> Result<(), NativeError> {
        if self.selection != EventAiAudienceSelection::SoloSelfCamera {
            return Err(NativeError::UnsupportedMechanic(
                "EventAI audience requires solo self-camera",
            ));
        }
        if !matches!(self.moment, InteractionMoment::Frame { .. })
            || self.speaker.spawn == 0
            || self.camera.owner != self.player
            || self.camera.body != VisitorParticipant::Player(self.player)
            || self.camera.body_pose != self.player_pose
        {
            return Err(NativeError::AuthorityBinding);
        }
        self.speaker_pose.validate()?;
        self.player_pose.validate()?;
        self.camera.body_pose.validate()?;
        nonnegative(self.speaker_combat_reach)?;
        nonnegative(self.body_combat_reach)?;
        self.traversal_radius()?;
        Ok(())
    }

    fn distance_squared(&self) -> f32 {
        let body = self.camera.body_pose.position;
        let speaker = self.speaker_pose.position;
        let dx = body.x.as_f32() - speaker.x.as_f32();
        let dy = body.y.as_f32() - speaker.y.as_f32();
        let dz = body.z.as_f32() - speaker.z.as_f32();
        let xy = dx * dx + dy * dy;
        xy + dz * dz
    }

    /// Validate exact complete membership, then return delivery to this player.
    /// Missing evidence is an error even when no listeners would pass distance.
    /// The caller must validate against a freshly constructed request, not the
    /// request echoed inside a historical receipt. No LOS/alive filter applies.
    pub fn validate(&self, facts: &EventAiAudienceFacts) -> Result<bool, NativeError> {
        self.validate_request()?;
        let (request, loaded_membership, world_camera, not_visited) = match facts {
            EventAiAudienceFacts::Complete {
                request,
                loaded_membership,
                world_camera,
                not_visited,
            } => (request, loaded_membership, world_camera, not_visited),
            EventAiAudienceFacts::UnrepresentedParticipants => {
                return Err(NativeError::UnsupportedMechanic(
                    "EventAI audience unrepresented cameras",
                ));
            }
            EventAiAudienceFacts::Missing | EventAiAudienceFacts::Unknown => {
                return Err(NativeError::AuthorityUnavailable(
                    "complete EventAI world-camera membership",
                ));
            }
        };
        if request.as_ref() != self {
            return Err(NativeError::AuthorityBinding);
        }
        if *loaded_membership != Check::Passed {
            return Err(NativeError::AuthorityUnavailable(
                "complete EventAI world-camera membership",
            ));
        }
        let visited = match (world_camera.as_deref(), not_visited.as_deref()) {
            (Some(camera), None) if *camera == self.camera => true,
            (None, Some(camera)) if *camera == self.camera => false,
            _ => return Err(NativeError::AuthorityBinding),
        };
        if !visited {
            return Ok(false);
        }

        // Object::GetDistance(DIST_CALC_NONE) and _IsWithinDist preserve these
        // binary32 operation boundaries: no square root, FMA, or reassociation.
        let distance_squared = self.distance_squared();
        let size_factor = self.body_combat_reach.as_f32() + self.speaker_combat_reach.as_f32();
        let maximum = self.effective_say_range()?.as_f32() + size_factor;
        Ok(distance_squared < maximum * maximum)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_sim::creature_ai::Point;

    fn real(value: f32) -> Real {
        Real::new(value).unwrap()
    }

    fn pose(x: f32, y: f32, z: f32) -> Pose {
        Pose {
            position: Point::new(x, y, z).unwrap(),
            orientation: Real::ZERO,
        }
    }

    fn request() -> EventAiAudienceRequest {
        let player = crate::PlayerId {
            owner: [1; 16],
            character: [2; 16],
        };
        EventAiAudienceRequest {
            callback: [3; 32],
            world: [4; 16],
            tick: 17,
            moment: InteractionMoment::Frame { action_ordinal: 2 },
            catalog: [5; 32],
            speaker: crate::NpcId {
                spawn: 8,
                generation: 3,
            },
            map: 0,
            speaker_pose: pose(0.0, 0.0, 0.0),
            player,
            player_pose: pose(10.0, 0.0, 0.0),
            camera: EventAiCamera {
                owner: player,
                body: VisitorParticipant::Player(player),
                body_pose: pose(10.0, 0.0, 0.0),
            },
            speaker_bounding_radius: real(0.5),
            speaker_combat_reach: real(1.5),
            body_combat_reach: real(1.5),
            say_range: real(25.0),
            visibility_distance: real(100.0),
            selection: EventAiAudienceSelection::SoloSelfCamera,
        }
    }

    fn body(request: &mut EventAiAudienceRequest, pose: Pose) {
        request.player_pose = pose;
        request.camera.body_pose = pose;
    }

    fn complete(request: &EventAiAudienceRequest, visited: bool) -> EventAiAudienceFacts {
        EventAiAudienceFacts::Complete {
            request: Box::new(request.clone()),
            loaded_membership: Check::Passed,
            world_camera: visited.then(|| Box::new(request.camera)),
            not_visited: (!visited).then(|| Box::new(request.camera)),
        }
    }

    #[test]
    fn strict_three_dimensional_distance_uses_both_reaches() {
        let mut request = request();
        let threshold = 28.0f32.to_bits();
        for (bits, delivered) in [
            (threshold - 1, true),
            (threshold, false),
            (threshold + 1, false),
        ] {
            body(&mut request, pose(0.0, 0.0, f32::from_bits(bits)));
            assert_eq!(request.validate(&complete(&request, true)), Ok(delivered));
        }
        body(&mut request, pose(26.0, 0.0, 0.0));
        assert!(request.validate(&complete(&request, true)).unwrap());
        // A loaded visited cell can contain a body beyond the traversal radius.
        assert_eq!(request.traversal_radius().unwrap(), real(25.5));
        request.body_combat_reach = Real::ZERO;
        request.speaker_combat_reach = Real::ZERO;
        assert!(!request.validate(&complete(&request, true)).unwrap());
    }

    #[test]
    fn reaches_round_together_before_the_range_and_zero_is_strict() {
        let mut request = request();
        request.body_combat_reach = Real::from_bits(0x3580_0000).unwrap();
        request.speaker_combat_reach = request.body_combat_reach;
        body(&mut request, pose(25.0, 0.0, 0.0));
        // Each reach is half one ULP at 25: adding them separately to 25 loses
        // both, while the source reach sum gives exactly one ULP of extension.
        assert!(request.validate(&complete(&request, true)).unwrap());
        request.say_range = Real::ZERO;
        request.body_combat_reach = Real::ZERO;
        request.speaker_combat_reach = Real::ZERO;
        body(&mut request, pose(0.0, 0.0, 0.0));
        assert!(!request.validate(&complete(&request, true)).unwrap());
    }

    #[test]
    fn visibility_multipliers_and_source_constant_cell_cap_are_distinct() {
        let mut request = request();
        for (visibility, range) in [
            (f32::from_bits(200.0f32.to_bits() - 1), 25.0),
            (200.0, 50.0),
            (f32::from_bits(400.0f32.to_bits() - 1), 50.0),
            (400.0, 100.0),
        ] {
            request.visibility_distance = real(visibility);
            assert_eq!(request.effective_say_range().unwrap(), real(range));
            assert_eq!(request.traversal_radius().unwrap(), real(range + 0.5));
        }
        request.visibility_distance = real(20.0);
        assert_eq!(request.traversal_radius().unwrap(), real(25.5));
        request.say_range = real(600.0);
        assert_eq!(request.effective_say_range().unwrap(), real(600.0));
        assert_eq!(request.traversal_radius().unwrap().bits(), 0x4405_5555);
        // Traversal is capped; the post-visit body-distance check is not.
        body(&mut request, pose(550.0, 0.0, 0.0));
        assert!(request.validate(&complete(&request, true)).unwrap());
    }

    #[test]
    fn zero_visited_requires_complete_explicit_outside_classification() {
        let mut request = request();
        body(&mut request, pose(1000.0, 0.0, 0.0));
        assert!(!request.validate(&complete(&request, false)).unwrap());
        assert!(!request.validate(&complete(&request, true)).unwrap());
        for facts in [
            EventAiAudienceFacts::Missing,
            EventAiAudienceFacts::Unknown,
            EventAiAudienceFacts::UnrepresentedParticipants,
            EventAiAudienceFacts::Complete {
                request: Box::new(request.clone()),
                loaded_membership: Check::Passed,
                world_camera: None,
                not_visited: None,
            },
            EventAiAudienceFacts::Complete {
                request: Box::new(request.clone()),
                loaded_membership: Check::Passed,
                world_camera: Some(Box::new(request.camera)),
                not_visited: Some(Box::new(request.camera)),
            },
        ] {
            assert!(request.validate(&facts).is_err());
        }
        for loaded_membership in [Check::Unknown, Check::Failed] {
            let facts = EventAiAudienceFacts::Complete {
                request: Box::new(request.clone()),
                loaded_membership,
                world_camera: None,
                not_visited: Some(Box::new(request.camera)),
            };
            assert!(request.validate(&facts).is_err());
        }
    }

    #[test]
    fn fresh_callback_and_every_geometry_identity_bind_the_facts() {
        let request = request();
        let facts = complete(&request, true);
        let mutations: &[fn(&mut EventAiAudienceRequest)] = &[
            |r| r.callback[0] ^= 1,
            |r| r.world[0] ^= 1,
            |r| r.tick += 1,
            |r| r.moment = InteractionMoment::Frame { action_ordinal: 3 },
            |r| r.catalog[0] ^= 1,
            |r| r.speaker.generation += 1,
            |r| r.speaker.spawn += 1,
            |r| r.map += 1,
            |r| r.speaker_pose = pose(1.0, 0.0, 0.0),
            |r| body(r, pose(11.0, 0.0, 0.0)),
            |r| r.speaker_bounding_radius = real(0.75),
            |r| r.speaker_combat_reach = real(2.0),
            |r| r.body_combat_reach = real(2.0),
            |r| r.say_range = real(30.0),
            |r| r.visibility_distance = real(200.0),
        ];
        for mutate in mutations {
            let mut fresh = request.clone();
            mutate(&mut fresh);
            assert_eq!(fresh.validate(&facts), Err(NativeError::AuthorityBinding));
        }
    }

    #[test]
    fn unknown_alternate_and_unrepresented_camera_contracts_reject() {
        let mut request = request();
        for selection in [
            EventAiAudienceSelection::Unknown,
            EventAiAudienceSelection::FarSight,
            EventAiAudienceSelection::AdditionalRecipients,
        ] {
            request.selection = selection;
            assert!(request.validate(&complete(&request, true)).is_err());
        }
        request.selection = EventAiAudienceSelection::SoloSelfCamera;
        request.camera.body = VisitorParticipant::Creature(request.speaker);
        assert!(request.validate(&complete(&request, true)).is_err());
        request.camera.body = VisitorParticipant::Player(request.player);
        request.camera.body_pose = pose(11.0, 0.0, 0.0);
        assert!(request.validate(&complete(&request, true)).is_err());
        request.camera.body_pose = request.player_pose;
        request.camera.owner.character[0] ^= 1;
        assert!(request.validate(&complete(&request, true)).is_err());
        request.camera.owner = request.player;
        request.moment = InteractionMoment::ReadOnly;
        assert!(request.validate(&complete(&request, true)).is_err());
    }

    #[test]
    fn membership_cannot_substitute_another_camera_owner_or_body() {
        let request = request();
        for visited in [true, false] {
            let mut camera = request.camera;
            camera.owner.character[0] ^= 1;
            let facts = EventAiAudienceFacts::Complete {
                request: Box::new(request.clone()),
                loaded_membership: Check::Passed,
                world_camera: visited.then(|| Box::new(camera)),
                not_visited: (!visited).then(|| Box::new(camera)),
            };
            assert_eq!(request.validate(&facts), Err(NativeError::AuthorityBinding));
        }
    }

    #[test]
    fn fixed_shape_roundtrips_and_omitted_or_extra_camera_evidence_rejects() {
        let request = request();
        let facts = complete(&request, true);
        let bytes = serde_json::to_vec(&facts).unwrap();
        assert!(bytes.len() < 8192);
        assert_eq!(
            serde_json::from_slice::<EventAiAudienceFacts>(&bytes).unwrap(),
            facts
        );
        for field in ["world_camera", "not_visited"] {
            let mut value = serde_json::to_value(&facts).unwrap();
            value["Complete"].as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<EventAiAudienceFacts>(value).is_err());
        }
        let mut value = serde_json::to_value(&facts).unwrap();
        value["Complete"]["extra_camera"] = serde_json::to_value(request.camera).unwrap();
        assert!(serde_json::from_value::<EventAiAudienceFacts>(value).is_err());
        let mut value = serde_json::to_value(&facts).unwrap();
        value["Complete"]["world_camera"] = serde_json::json!([request.camera, request.camera]);
        assert!(serde_json::from_value::<EventAiAudienceFacts>(value).is_err());
    }

    #[test]
    fn negative_ranges_or_reaches_and_invalid_poses_reject() {
        let mutations: &[fn(&mut EventAiAudienceRequest)] = &[
            |r| r.speaker_bounding_radius = real(-1.0),
            |r| r.speaker_combat_reach = real(-1.0),
            |r| r.body_combat_reach = real(-1.0),
            |r| r.say_range = real(-1.0),
            |r| r.visibility_distance = real(-1.0),
            |r| r.speaker_pose.orientation = real(7.0),
            |r| r.speaker_pose.position.x = real(32761.0),
        ];
        for mutate in mutations {
            let mut request = request();
            mutate(&mut request);
            assert!(request.validate(&complete(&request, true)).is_err());
        }
    }
    #[test]
    fn pinned_original_cpp_camera_vectors_match_actual_rust_validator() {
        #[derive(Deserialize)]
        struct Vectors {
            schema: u32,
            source_revision: String,
            source_sha256: String,
            cases: Vec<Vector>,
        }
        #[derive(Deserialize)]
        struct Vector {
            name: String,
            body_position_bits: [u32; 3],
            speaker_position_bits: [u32; 3],
            body_reach_bits: u32,
            speaker_reach_bits: u32,
            say_range_bits: u32,
            visibility_bits: u32,
            expected_effective_range_bits: u32,
            expected_distance_squared_bits: u32,
            expected_delivered: bool,
        }
        let vectors: Vectors =
            serde_json::from_str(include_str!("../../../tests/data/event_ai_camera.json")).unwrap();
        assert_eq!(vectors.schema, 1);
        assert_eq!(
            vectors.source_revision,
            "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f"
        );
        assert_eq!(
            vectors.source_sha256,
            "410904520fe2847a855c396d9fe442065268fd4ff5bb510633c3aadb5c54665b"
        );
        assert_eq!(vectors.cases.len(), 44);
        for vector in vectors.cases {
            let mut request = request();
            let as_pose = |bits: [u32; 3]| {
                pose(
                    f32::from_bits(bits[0]),
                    f32::from_bits(bits[1]),
                    f32::from_bits(bits[2]),
                )
            };
            request.player_pose = as_pose(vector.body_position_bits);
            request.camera.body_pose = request.player_pose;
            request.speaker_pose = as_pose(vector.speaker_position_bits);
            request.body_combat_reach = Real::from_bits(vector.body_reach_bits).unwrap();
            request.speaker_combat_reach = Real::from_bits(vector.speaker_reach_bits).unwrap();
            request.say_range = Real::from_bits(vector.say_range_bits).unwrap();
            request.visibility_distance = Real::from_bits(vector.visibility_bits).unwrap();
            let facts = EventAiAudienceFacts::Complete {
                request: Box::new(request.clone()),
                loaded_membership: Check::Passed,
                world_camera: Some(Box::new(request.camera)),
                not_visited: None,
            };
            assert_eq!(
                request.effective_say_range().unwrap().bits(),
                vector.expected_effective_range_bits,
                "{} effective range",
                vector.name
            );
            assert_eq!(
                request.distance_squared().to_bits(),
                vector.expected_distance_squared_bits,
                "{} squared distance",
                vector.name
            );
            assert_eq!(
                request.validate(&facts).unwrap(),
                vector.expected_delivered,
                "{} delivery",
                vector.name
            );
        }
    }
}
