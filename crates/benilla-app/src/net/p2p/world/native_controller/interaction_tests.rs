// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual controller preview route with source-derived semantics and no spatial answer.
use super::*;

fn source_profile() -> n::Profile {
    let mut profile = quests::profile();
    let control = n::InteractionControlSeed {
        unit_state: 0,
        owner_guid: 0,
        charmer_guid: 0,
        mount_display_id: 0,
        shapeshift_form: 0,
        aura_flags: 0,
    };
    let absent = n::InteractionEvidence::Absent;
    let input = n::InteractionInput {
        player: n::PlayerInteractionSource {
            faction_template: 1,
            unit_flags: 8,
            player_flags: 0,
            control,
        },
        npcs: profile
            .npcs
            .iter()
            .map(|npc| n::NpcInteractionSource {
                entry: npc.entry,
                faction_template: npc.faction_template,
                npc_flags: if npc.entry == 60003 { 2 } else { 0 },
                unit_flags: 0,
                creature_type_flags: 0,
                control,
                spawn_flag_or_faction_overrides: absent,
            })
            .collect(),
        factions: vec![n::InteractionFactionSource {
            template_id: 1,
            faction: 0,
            template_flags: 0,
            group_mask: 1,
            friend_group_mask: 1,
            enemy_group_mask: 0,
            enemies: [0; 4],
            friends: [0; 4],
            reputation_list_id: None,
        }],
        audit: n::InteractionAudit {
            external_state_mutations: absent,
            controlled_or_summoned_units: absent,
            pvp_duel_or_sanctuary: absent,
            forced_reactions: absent,
            temporary_factions: absent,
            immunity_overrides: absent,
            npc_aura_or_script_hooks: absent,
        },
    };
    profile.initialize_interactions(input).unwrap();
    profile
}
struct NoSpatialAnswers {
    derived: Arc<Mutex<Vec<n::AuthorityReply>>>,
}
impl n::Authority for NoSpatialAnswers {
    fn resolve(&mut self, _: &n::AuthorityRequest) -> Result<n::AuthorityReply, n::NativeError> {
        panic!("read-only source interaction reached the spatial service")
    }
    fn observe_derived(&mut self, reply: &n::AuthorityReply) -> Result<(), n::NativeError> {
        self.derived.lock().unwrap().push(reply.clone());
        Ok(())
    }
}
#[test]
fn source_status_and_hello_at_tick_zero_never_require_spatial_body_or_authored_facts() {
    let h = Harness::new();
    let derived = Arc::new(Mutex::new(vec![]));
    let mut c = h
        .enter(
            Entry::Create {
                world: [8; 16],
                seed: 7,
            },
            source_profile(),
            NoSpatialAnswers {
                derived: derived.clone(),
            },
        )
        .unwrap();
    let before = c.world.checkpoint().unwrap();
    let npc = c.projection.npc_guid(c.world.state().npcs[&3].id).unwrap();
    let status = c
        .receive(ClientCommand::QuestgiverStatusQuery { npc })
        .unwrap();
    assert!(
        matches!(&status.events[..], [SessionEvent::QuestGiverStatus { status, .. }] if *status != 0)
    );
    let hello = c.receive(ClientCommand::QuestgiverHello { npc }).unwrap();
    assert!(matches!(
        &hello.events[..],
        [SessionEvent::QuestGreeting(_)]
    ));
    assert_eq!(c.world.checkpoint().unwrap(), before);
    assert!(c.actions.is_empty());
    let replies = derived.lock().unwrap();
    assert_eq!(replies.len(), 2);
    for reply in replies.iter() {
        assert_eq!(reply.request.binding.tick, 0);
        let witness = match &reply.request.query {
            n::Query::QuestStatus { witness, .. } | n::Query::QuestInteraction { witness, .. } => {
                witness
            }
            _ => panic!("unexpected semantic query"),
        };
        assert_eq!(witness.moment, n::InteractionMoment::ReadOnly);
    }
}
