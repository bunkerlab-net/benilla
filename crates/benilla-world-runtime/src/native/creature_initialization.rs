// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f,
// Creature.cpp:338-442/508-523/1912-1945, GridMap.cpp:1147-1160.
//! Source-ordered, positively dry InitEntry evidence. No wet locomotion or admission.
use super::*;
use benilla_sim::creature_init as init;
use serde::{Deserialize, Serialize};

pub const CREATURE_INITIALIZATION_POLICY: &str = "cmangos-8ec338a1-native-dry-initialization-v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureInitializationSnapshot {
    pub map: u32,
    pub content: crate::Digest,
    pub source_epoch: crate::Digest,
    pub inventory: crate::Digest,
    pub policy: crate::Digest,
    pub effective_config: crate::Digest,
    pub vmap_height_enabled: bool,
}
/// Raw retained source coordinates and source-query selection, not a dry answer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum CreatureInitializationClosure {
    Ground,
    DryWater {
        map: u32,
        respawn_position: ai::Point,
        snapshot: CreatureInitializationSnapshot,
    },
}
impl CreatureInitializationClosure {
    pub(super) fn validate(
        &self,
        profile: &Profile,
        row: &CreatureCapabilitySource,
        habitat: u32,
    ) -> Result<(), NativeError> {
        match self {
            Self::Ground if habitat == 1 => Ok(()),
            Self::DryWater {
                map,
                respawn_position,
                snapshot,
            } if habitat == 3 => {
                let source = profile
                    .creature_generations
                    .as_ref()
                    .and_then(|catalog| catalog.source(row.spawn_id))
                    .ok_or(NativeError::AuthorityUnavailable(
                        "dry creature generation source",
                    ))?;
                let init::Rows::Complete(spawns) = &source.input.spawn_template else {
                    return Err(NativeError::AuthorityBinding);
                };
                let [spawn] = spawns.as_slice() else {
                    return Err(NativeError::AuthorityBinding);
                };
                if spawn.unit_flags != -1
                    && spawn.unit_flags as u32 & benilla_sim::creature_swimming::UNIT_FLAG_SWIMMING
                        != 0
                {
                    return Err(NativeError::UnsupportedMechanic(
                        "preexisting swimming unit flag",
                    ));
                }
                let npc = profile
                    .npc(row.spawn_id)
                    .ok_or(NativeError::InvalidProfile)?;
                if *map != profile.map_id
                    || snapshot.map != *map
                    || snapshot.content != profile.content_digest
                    || *respawn_position != npc.ai.spawn
                    || [
                        snapshot.content,
                        snapshot.source_epoch,
                        snapshot.inventory,
                        snapshot.policy,
                        snapshot.effective_config,
                    ]
                    .contains(&[0; 32])
                    || source.input.template.extra_flags
                        & benilla_sim::creature_swimming::WALK_IN_WATER
                        != 0
                    || source.input.template.unit_flags
                        & benilla_sim::creature_swimming::UNIT_FLAG_SWIMMING
                        != 0
                {
                    return Err(NativeError::AuthorityBinding);
                }
                let widths = profile.creature_collision_widths.as_ref().ok_or(
                    NativeError::AuthorityUnavailable("dry creature selected model source"),
                )?;
                widths.validate(profile)?;
                profile
                    .creature_collision_heights
                    .as_ref()
                    .ok_or(NativeError::AuthorityUnavailable(
                        "dry creature selected height source",
                    ))?
                    .validate(profile)?;
                Ok(())
            }
            _ => Err(NativeError::UnsupportedMechanic(
                "runtime creature capability habitat domain",
            )),
        }
    }
}

/// Full immutable source call. The classification height is the nested method's
/// default; collision_height is solely IsSwimmable's selected-model depth threshold.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureInitializationRequest {
    pub binding: Binding,
    pub snapshot: CreatureInitializationSnapshot,
    pub capability_source: crate::Digest,
    pub generation_source: crate::Digest,
    pub model_source: crate::Digest,
    pub position: ai::Point,
    pub native_display_id: u32,
    pub model_id: u32,
    pub object_scale: spell::SpellFloat,
    pub raw_height: spell::SpellFloat,
    pub collision_height: spell::SpellFloat,
    pub classification_height: spell::SpellFloat,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    pub model_roll: Option<u32>,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    pub gender_roll: Option<u32>,
    #[serde(deserialize_with = "model_gender_draws")]
    pub model_gender_draws: Vec<Draw>,
    pub first_counter: u64,
    pub before_level_counter: u64,
    pub now_ms: u64,
    pub ticket: u64,
    pub required_type: u8,
}
impl CreatureInitializationRequest {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        profile: &Profile,
        world: crate::WorldId,
        npc: crate::NpcId,
        tick: u64,
        first_counter: u64,
        before_level_counter: u64,
        model: &init::ModelPlan<'_>,
        model_gender_draws: &[Draw],
    ) -> Result<Option<Self>, NativeError> {
        let Some(capability) = profile
            .creature_capabilities
            .as_ref()
            .and_then(|catalog| catalog.source(npc.spawn))
        else {
            return Ok(None);
        };
        let CreatureInitializationClosure::DryWater {
            respawn_position,
            snapshot,
            ..
        } = &capability.initialization
        else {
            return Ok(None);
        };
        let source = profile
            .creature_generations
            .as_ref()
            .and_then(|catalog| catalog.source(npc.spawn))
            .ok_or(NativeError::AuthorityUnavailable(
                "dry creature generation source",
            ))?;
        if model_gender_draws.len() > 2 {
            return Err(NativeError::AuthorityBinding);
        }
        let plan = init::prepare(&source.input)?;
        let decisions = [
            (
                DrawPurpose::CreatureModel { npc },
                plan.model_roll_range(),
                model.model_roll(),
            ),
            (
                DrawPurpose::CreatureGender { npc },
                plan.gender_roll_range(model.model_roll())?,
                model.gender_roll(),
            ),
        ];
        let mut draws = model_gender_draws.iter();
        let mut cursor = first_counter;
        for (purpose, range, value) in decisions {
            let Some((low, high)) = range else {
                if value.is_some() {
                    return Err(NativeError::AuthorityBinding);
                }
                continue;
            };
            let draw = draws.next().ok_or(NativeError::AuthorityBinding)?;
            if draw.cast_id != 0
                || draw.purpose != purpose
                || draw.first_counter != cursor
                || draw.consumed == 0
                || draw.consumed > 64
                || draw.low != i64::from(low)
                || draw.high != i64::from(high)
                || value.map(i64::from) != Some(draw.value)
                || !(draw.low..=draw.high).contains(&draw.value)
            {
                return Err(NativeError::AuthorityBinding);
            }
            cursor = cursor
                .checked_add(u64::from(draw.consumed))
                .ok_or(NativeError::Limit)?;
        }
        if draws.next().is_some() || cursor != before_level_counter {
            return Err(NativeError::AuthorityBinding);
        }
        let width = profile
            .creature_collision_widths
            .as_ref()
            .and_then(|catalog| catalog.source(npc.spawn))
            .ok_or(NativeError::AuthorityUnavailable(
                "dry creature model source",
            ))?;
        let height = width.selected_height_before_level(model)?;
        let now_ms = tick
            .checked_mul(u64::from(profile.tick_ms))
            .and_then(|n| profile.clock_origin_ms.checked_add(n))
            .ok_or(NativeError::Limit)?;
        Ok(Some(Self {
            binding: Binding {
                world,
                tick,
                profile: profile.digest(),
                authority_policy: profile.authority_policy,
                geometry: profile.geometry_revision,
                npc: Some(npc),
            },
            snapshot: *snapshot,
            capability_source: capability.binding(),
            generation_source: source.identity(),
            model_source: width.binding(),
            position: *respawn_position,
            native_display_id: height.native_display_id(),
            model_id: height.model_id(),
            object_scale: height.object_scale(),
            raw_height: height.raw_height(),
            collision_height: height.height(),
            classification_height: spell::SpellFloat::from_bits(
                benilla_sim::creature_swimming::LIQUID_COLLISION_HEIGHT_BITS,
            )?,
            model_roll: model.model_roll(),
            gender_roll: model.gender_roll(),
            model_gender_draws: model_gender_draws.to_vec(),
            first_counter,
            before_level_counter,
            now_ms,
            ticket: npc.generation.checked_add(1).ok_or(NativeError::Limit)?,
            required_type: benilla_sim::creature_swimming::LIQUID_MASK,
        }))
    }
    /// Transcript consistency only; the running generation owner proves draw and
    /// clock history. The trusted geometry adapter must query its exact byte snapshot.
    pub fn validate_for(&self, profile: &Profile) -> Result<(), NativeError> {
        profile.validate()?;
        let npc = self.binding.npc.ok_or(NativeError::AuthorityBinding)?;
        let source = profile
            .creature_generations
            .as_ref()
            .and_then(|catalog| catalog.source(npc.spawn))
            .ok_or(NativeError::AuthorityUnavailable(
                "dry creature generation source",
            ))?;
        let model =
            init::prepare(&source.input)?.select_model(self.model_roll, self.gender_roll)?;
        let expected = Self::new(
            profile,
            self.binding.world,
            npc,
            self.binding.tick,
            self.first_counter,
            self.before_level_counter,
            &model,
            &self.model_gender_draws,
        )?
        .ok_or(NativeError::AuthorityBinding)?;
        if self != &expected
            || self.binding.world == [0; 16]
            || self.before_level_counter < self.first_counter
            || self.before_level_counter - self.first_counter > 128
        {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(())
    }
}

fn model_gender_draws<'de, D>(deserializer: D) -> Result<Vec<Draw>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct Prefix;
    impl<'de> serde::de::Visitor<'de> for Prefix {
        type Value = Vec<Draw>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("at most two exact model/gender draw receipts")
        }
        fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            if sequence.size_hint().is_some_and(|size| size > 2) {
                return Err(serde::de::Error::custom("model/gender draw bound"));
            }
            let mut values = Vec::with_capacity(2);
            while let Some(value) = sequence.next_element()? {
                if values.len() == 2 {
                    return Err(serde::de::Error::custom("model/gender draw bound"));
                }
                values.push(value);
            }
            Ok(values)
        }
    }
    deserializer.deserialize_seq(Prefix)
}

/// Authority transcript, never a constructor for live movement state. A successful
/// adapter obtains this exact NO_WATER observation from loaded product bytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureInitializationFacts {
    pub request: CreatureInitializationRequest,
    pub status: environment::LiquidStatus,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    pub data: Option<environment::LiquidData>,
}
impl CreatureInitializationFacts {
    pub fn validate_for(
        &self,
        expected: &CreatureInitializationRequest,
    ) -> Result<(), NativeError> {
        if &self.request != expected {
            return Err(NativeError::AuthorityBinding);
        }
        if self.status != environment::LiquidStatus::NoWater || self.data.is_some() {
            return Err(NativeError::UnsupportedMechanic(
                "wet creature initialization",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
pub(super) fn unavailable_initialization(
    _: &CreatureInitializationRequest,
) -> Result<CreatureInitializationFacts, NativeError> {
    Err(NativeError::AuthorityUnavailable(
        "creature initialization liquid observation",
    ))
}

#[cfg(test)]
mod tests;
