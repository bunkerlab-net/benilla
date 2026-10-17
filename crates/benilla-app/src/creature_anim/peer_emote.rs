//! Fork deviation: exploration peers share the owner's weighted dance sequence and phase.
//! Snapshots remove frame-rate drift; residual one-way transport delay is not clock-synchronized.
use super::peer_timeline::{Clock, Timeline};
use benilla_assets::{AnimClip, ModelAnimations};
use bevy::prelude::*;

#[derive(Component)]
pub(super) struct PeerEmotePlayback {
    pub(super) emote_id: u32,
    pub(super) clock: Clock,
    seed: u32,
    timeline: Timeline,
    model: Option<(bevy::asset::AssetId<AnimationGraph>, u16)>,
}
impl PeerEmotePlayback {
    pub(super) fn new(
        guid: u64,
        emote_id: u32,
        generation: u64,
        elapsed_ms: u32,
        now: f64,
    ) -> Self {
        let mixed = guid ^ guid.rotate_left(23) ^ generation.wrapping_mul(0x9e3779b97f4a7c15);
        let seed = (mixed ^ (mixed >> 32)) as u32;
        Self {
            emote_id,
            clock: Clock::new(generation, f64::from(elapsed_ms) / 1000.0, now),
            seed,
            timeline: Timeline::new(seed),
            model: None,
        }
    }
    pub(super) fn sample<'a>(
        &mut self,
        anims: &'a ModelAnimations,
        head: &'a AnimClip,
        now: f64,
    ) -> Option<(&'a AnimClip, f32)> {
        let model = (anims.graph.id(), head.anim_id);
        if self.model != Some(model) {
            self.model = Some(model);
            self.timeline = Timeline::new(self.seed);
        }
        let (index, phase) =
            self.timeline
                .sample(self.clock.elapsed(now), |variation, replay| {
                    let c = anims
                        .pick_variation(head.anim_id, variation)
                        .unwrap_or(head);
                    let count = (c.replay.0
                        + ((u64::from(replay) * u64::from(c.replay.1.saturating_sub(c.replay.0)))
                            >> 15) as u32)
                        .max(1);
                    let index = anims
                        .clips
                        .iter()
                        .position(|other| other.node == c.node)
                        .unwrap_or(0);
                    (index, f64::from(c.duration) * f64::from(count))
                })?;
        let c = anims.clips.get(index)?;
        Some((c, (phase % f64::from(c.duration)) as f32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::animation::graph::AnimationNodeIndex;

    fn model() -> ModelAnimations {
        ModelAnimations {
            graph: Handle::default(),
            clips: (0..3)
                .map(|i| AnimClip {
                    anim_id: 69,
                    seq_index: i,
                    node: AnimationNodeIndex::new(i + 1),
                    looping: true,
                    duration: (i + 1) as f32,
                    move_speed: 0.0,
                    blend_time: 0.1,
                    bounds_center: Vec3::ZERO,
                    bounds_radius: 0.0,
                    bounds_min: Vec3::ZERO,
                    bounds_max: Vec3::ZERO,
                    events: Vec::new().into(),
                    arm_nodes: None,
                    upper_node: None,
                    frequency: [8192, 8192, 16384][i],
                    replay: (1, 4),
                    poses_bones: true,
                })
                .collect(),
            hand_close: [None, None],
            playable_animation_lookup: Vec::new(),
            animation_lookup: Vec::new(),
            global_bones: Vec::new(),
            first_seq: None,
            pose: Default::default(),
        }
    }

    #[test]
    fn weighted_m2_variations_and_replay_windows_match_at_different_frame_rates() {
        let model = model();
        let mut a = PeerEmotePlayback::new(42, 10, 1, 0, 0.0);
        let mut b = PeerEmotePlayback::new(42, 10, 1, 0, 100.0);
        let mut seen = std::collections::BTreeSet::new();
        for second in 0..100 {
            for frame in 0..60 {
                a.sample(&model, &model.clips[0], second as f64 + frame as f64 / 60.0);
            }
            let (ca, pa) = a
                .sample(&model, &model.clips[0], second as f64 + 1.0)
                .unwrap();
            let (cb, pb) = b
                .sample(&model, &model.clips[0], second as f64 + 101.0)
                .unwrap();
            assert_eq!((ca.seq_index, pa), (cb.seq_index, pb));
            seen.insert(ca.seq_index);
        }
        assert_eq!(
            seen.len(),
            3,
            "weighted authored variations remain available"
        );
        let mut late = PeerEmotePlayback::new(42, 10, 1, 100_000, 900.0);
        let (c, p) = late.sample(&model, &model.clips[0], 900.0).unwrap();
        let (expected, ep) = a.sample(&model, &model.clips[0], 100.0).unwrap();
        assert_eq!((c.seq_index, p), (expected.seq_index, ep));
    }

    #[test]
    fn repeated_emote_starts_a_fresh_generation_without_consuming_global_randomness() {
        let model = model();
        let mut old = PeerEmotePlayback::new(42, 10, 1, 0, 0.0);
        old.sample(&model, &model.clips[0], 90.0);
        let mut repeat = PeerEmotePlayback::new(42, 10, 2, 0, 90.0);
        let (_, phase) = repeat.sample(&model, &model.clips[0], 90.0).unwrap();
        assert_eq!(phase, 0.0);
        assert_ne!(old.seed, repeat.seed);
    }
}
