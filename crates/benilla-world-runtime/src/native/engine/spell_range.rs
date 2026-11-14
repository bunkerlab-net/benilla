// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 Spell.cpp::GetMinMaxRange/CheckRange and
// Object.cpp::GetDistance; upstream GPL-2.0-or-later. See SPELL-RANGE.md.
use super::*;
use benilla_sim::spell_range;

impl<A: Authority> Frame<'_, A> {
    pub(super) fn spell_range_facts(
        &self,
        request: &AuthorityRequest,
    ) -> Result<Option<Facts>, NativeError> {
        if !interactions::spell_authority::selected(self.profile) {
            return Ok(None);
        }
        let Query::SourceSpell(SourceSpellQuery::Range {
            context,
            source,
            player_reach,
            target_reach,
        }) = &request.query
        else {
            return Ok(None);
        };
        // Only Frame::ask calls this function, with its executing request. No
        // external receipt/request can grant current state or phase authority.
        let strict = match context.phase {
            SourceSpellPhase::Prepare => true,
            SourceSpellPhase::ReleaseCheck => false,
            _ => return Err(NativeError::AuthorityBinding),
        };
        if source.flags != 0 {
            return Err(NativeError::UnsupportedMechanic("spell range flags"));
        }
        let outcome = spell_range::check_bits(
            xyz(context.player_position),
            xyz(context.target_position),
            source.minimum.bits(),
            source.maximum.bits(),
            player_reach.bits(),
            target_reach.bits(),
            strict,
        )?;
        // Source writes m_maxRange only on success. Its sole read is a channel
        // check; channel spells are excluded by the source catalog, so this
        // bounded projectile/holder host has no consumer or persistent field.
        let result = match outcome.result {
            spell_range::RangeResult::Within => SpellRangeResult::Within,
            spell_range::RangeResult::TooFar => SpellRangeResult::TooFar,
            spell_range::RangeResult::TooClose => SpellRangeResult::TooClose,
        };
        Ok(Some(Facts::SourceSpell(SourceSpellFacts::Range(result))))
    }
}
fn xyz(point: ai::Point) -> [u32; 3] {
    [point.x.bits(), point.y.bits(), point.z.bits()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::{synthetic_fixture as f, synthetic_spell_fixture as fixture};

    #[test]
    fn frame_range_guard_refuses_noncheck_phases_and_nonzero_flags_before_observation() {
        let mut world =
            NativeWorld::synthetic_component_test(fixture::profile(), [8; 16], f::player(), 7)
                .unwrap();
        let input = f::cast(&world, 133);
        world
            .step(1, Some(input), &mut fixture::spatial_only())
            .unwrap();
        let original = world
            .last_tick()
            .unwrap()
            .replies
            .iter()
            .find(|r| {
                matches!(
                    r.request.query,
                    Query::SourceSpell(SourceSpellQuery::Range { .. })
                )
            })
            .unwrap()
            .request
            .clone();
        let before = world.checkpoint().unwrap();
        for case in 0..4 {
            let mut request = original.clone();
            let Query::SourceSpell(SourceSpellQuery::Range {
                context, source, ..
            }) = &mut request.query
            else {
                unreachable!();
            };
            match case {
                0 => context.phase = SourceSpellPhase::ReleaseHit,
                1 => context.phase = SourceSpellPhase::Impact,
                2 => context.phase = SourceSpellPhase::Aura,
                _ => source.flags = 1,
            }
            let mut next = world.state.clone();
            let mut authority = fixture::spatial_only();
            let mut frame = Frame {
                read_only: false,
                profile: &world.profile,
                profile_digest: world.profile_digest,
                state: &mut next,
                authority: &mut authority,
                replies: vec![],
                events: vec![],
                draws: vec![],
                receipts: BTreeMap::new(),
                source_alive_unit: None,
                moved: false,
                cast_inputs: BTreeMap::new(),
                movement_origin: world.state.position,
                movement_path: vec![],
                player_auras_updated: false,
                player_melee_unit_updated: false,
                pending_player_melee_death: None,
            };
            let expected = if case == 3 {
                NativeError::UnsupportedMechanic("spell range flags")
            } else {
                NativeError::AuthorityBinding
            };
            assert_eq!(frame.ask(request.binding.npc, request.query), Err(expected));
            assert!(frame.replies.is_empty());
            assert!(authority.derived.is_empty());
            assert!(authority.queries.is_empty());
            assert_eq!(world.checkpoint().unwrap(), before);
        }
    }
}
