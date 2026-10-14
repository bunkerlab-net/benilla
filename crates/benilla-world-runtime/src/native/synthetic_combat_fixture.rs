// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored source-shaped rows and spatial witnesses, never content admission.
use super::*;

pub fn source_rows(profile: &Profile) -> InteractionInput {
    let control = InteractionControlSeed {
        unit_state: 0,
        owner_guid: 0,
        charmer_guid: 0,
        mount_display_id: 0,
        shapeshift_form: 0,
        aura_flags: 0,
    };
    let absent = InteractionEvidence::Absent;
    InteractionInput {
        player: PlayerInteractionSource {
            faction_template: 1,
            unit_flags: 8,
            player_flags: 0,
            control,
        },
        npcs: profile
            .npcs
            .iter()
            .map(|n| NpcInteractionSource {
                entry: n.entry,
                faction_template: n.faction_template,
                npc_flags: 0,
                unit_flags: n.health_regeneration.unit_flags,
                creature_type_flags: n.health_regeneration.creature_type_flags,
                control,
                spawn_flag_or_faction_overrides: absent,
            })
            .collect(),
        factions: [1, 2]
            .map(|id| InteractionFactionSource {
                template_id: id,
                faction: if id == 2 { 96002 } else { 0 },
                template_flags: 0,
                group_mask: id,
                friend_group_mask: id,
                enemy_group_mask: if id == 1 { 2 } else { 1 },
                enemies: [0; 4],
                friends: [0; 4],
                reputation_list_id: if id == 2 { Some(-1) } else { None },
            })
            .into(),
        audit: InteractionAudit {
            external_state_mutations: absent,
            controlled_or_summoned_units: absent,
            pvp_duel_or_sanctuary: absent,
            forced_reactions: absent,
            temporary_factions: absent,
            immunity_overrides: absent,
            npc_aura_or_script_hooks: absent,
        },
    }
}
pub fn install(profile: &mut Profile) {
    profile
        .initialize_interactions(source_rows(profile))
        .unwrap();
    profile
        .initialize_combat_authority(
            profile
                .npcs
                .iter()
                .map(|n| NpcCombatAuthoritySource {
                    entry: n.entry,
                    extra_flags: 0,
                    static_flags1: 0,
                    static_flags3: 0,
                    static_flags4: 0,
                    static_flags2: 0,
                })
                .collect(),
        )
        .unwrap();
}
pub fn profile() -> Profile {
    let mut p = synthetic_fixture::initialized_profile();
    p.npcs[0].faction_template = 2;
    p.npcs[0].ai.reaction = ai::Reaction::Aggressive;
    p.source_motion = Some(synthetic_motion_fixture::genesis(&p));
    install(&mut p);
    p
}
#[derive(Default)]
pub struct SpatialOnly {
    pub base: synthetic_motion_fixture::SyntheticMotionAuthority,
    pub derived: Vec<AuthorityReply>,
    pub queries: Vec<AuthorityRequest>,
    pub visibility: Option<ai::movement::Check>,
    pub accessibility: Option<ai::movement::Check>,
    pub line_of_sight: Option<ai::movement::Check>,
    pub front: Option<ai::movement::Check>,
    pub fail_post_damage: bool,
    pub stale_spatial: bool,
    pub wrong_spatial_kind: bool,
}
impl Authority for SpatialOnly {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        self.queries.push(request.clone());
        assert!(
            !matches!(request.query, Query::Npc { .. } | Query::NativeMelee { .. }),
            "semantic facts escaped source host"
        );
        let mut reply = self.base.resolve(request)?;
        if let Facts::SourceMotion(MotionFacts::Combat(facts)) = &mut reply.facts {
            match facts {
                CombatFacts::NpcVisibility(value) => *value = self.visibility.unwrap_or(*value),
                CombatFacts::NpcAccessibility(value) => {
                    *value = self.accessibility.unwrap_or(*value)
                }
                CombatFacts::NpcLineOfSight(value) => *value = self.line_of_sight.unwrap_or(*value),
                CombatFacts::MeleeFront(value) => *value = self.front.unwrap_or(*value),
                _ => panic!("semantic facts escaped source host"),
            }
            if self.stale_spatial {
                reply.request.binding.tick += 1;
            }
            if self.wrong_spatial_kind {
                *facts = CombatFacts::MeleeFront(ai::movement::Check::Passed);
            }
        }
        Ok(reply)
    }
    fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
        self.derived.push(reply.clone());
        if self.fail_post_damage
            && matches!(
                reply.facts,
                Facts::SourceMotion(MotionFacts::Combat(CombatFacts::MeleeDamage(_)))
            )
        {
            return Err(NativeError::AuthorityUnavailable(
                "authored post-health receipt fault",
            ));
        }
        Ok(())
    }
}
