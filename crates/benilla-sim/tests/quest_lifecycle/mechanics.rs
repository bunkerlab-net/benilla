// SPDX-License-Identifier: GPL-3.0-or-later
// Authored projections, not copied database rows.
use super::*;
fn input() -> MechanicsInput {
    MechanicsInput {
        script_keys: vec![71021],
        scripts: vec![
            ScriptRow {
                id: 71021,
                delay: 2000,
                priority: 0,
                position: 0,
                command: 0,
                datalong: [0; 3],
                buddy_entry: 0,
                search_radius: 0,
                data_flags: 0,
                dataint: [72001, 0, 0, 0],
                float_bits: [0; 6],
                condition: 0,
            },
            ScriptRow {
                id: 71021,
                delay: 2000,
                priority: 0,
                position: 1,
                command: 1,
                datalong: [71, 0, 0],
                buddy_entry: 0,
                search_radius: 0,
                data_flags: 0,
                dataint: [0; 4],
                float_bits: [0; 6],
                condition: 0,
            },
        ],
        broadcasts: vec![ScriptBroadcast {
            id: 72001,
            chat_type: 0,
            language: 0,
            sounds: [0; 2],
            emotes: [0; 3],
            delays: [0; 3],
            condition: 0,
            emotes_id: 0,
            flags: 1,
        }],
        emotes: vec![(71, 0)],
        ..MechanicsInput::default()
    }
}
#[test]
fn completion_script_keeps_native_delay_and_canonical_priority_tie_order() {
    let m = Mechanics::compile(input()).unwrap();
    assert_eq!(
        m.script(71021).unwrap(),
        [
            ScriptStep {
                delay_ms: 2000,
                effect: ScriptEffect::Talk { broadcast: 72001 }
            },
            ScriptStep {
                delay_ms: 2000,
                effect: ScriptEffect::Emote { emote: 71 }
            },
        ]
    );
    let mut permuted = input();
    permuted.scripts.reverse();
    assert_eq!(
        Mechanics::compile(permuted).unwrap().script(71021),
        m.script(71021)
    );
    let mut prioritized = input();
    prioritized.scripts[0].priority = 2;
    assert_eq!(
        Mechanics::compile(prioritized)
            .unwrap()
            .script(71021)
            .unwrap()[0]
            .effect,
        ScriptEffect::Emote { emote: 71 }
    );
    let mut delayed = input();
    delayed.scripts[0].priority = 2;
    delayed.scripts[0].delay = 1000;
    let delayed = Mechanics::compile(delayed).unwrap();
    assert_eq!(delayed.script(71021).unwrap()[0].delay_ms, 1000);
    assert_eq!(
        delayed.script(71021).unwrap()[0].effect,
        ScriptEffect::Talk { broadcast: 72001 }
    );
    let mut row = quest(71001);
    row.excluded.complete_script = 71021;
    let c = Catalog::new_with_mechanics(vec![row.clone()], relations(&[row]), audit(), m).unwrap();
    assert_eq!(c.row(71001).unwrap().excluded.complete_script, 71021);
    let mut invalid = input();
    invalid.scripts[0].command = 15;
    assert!(matches!(
        Mechanics::compile(invalid),
        Err(Error::Relationship {
            field: "quest_end_scripts.command",
            target: 15,
            reason: "unsupported_command",
            ..
        })
    ));
    let mut invalid = input();
    invalid.scripts[0].data_flags = 4;
    assert!(matches!(
        Mechanics::compile(invalid),
        Err(Error::Relationship {
            field: "quest_end_scripts.data_flags",
            target: 4,
            ..
        })
    ));
    let mut invalid = input();
    invalid.broadcasts[0].sounds[0] = 1;
    assert!(matches!(
        Mechanics::compile(invalid),
        Err(Error::Relationship {
            field: "broadcast_text",
            reason: "unsupported_speech_effects",
            ..
        })
    ));
}
fn option(condition: u32) -> GossipOptionRow {
    GossipOptionRow {
        menu: 0,
        id: 0,
        option: 2,
        npc_flags: 2,
        condition,
        icon: 0,
        text: "Authored hidden questgiver option".into(),
        broadcast: 0,
        action_menu: 0,
        action_poi: 0,
        action_script: 0,
        box_coded: false,
        box_money: 0,
        box_text: String::new(),
        box_broadcast: 0,
    }
}
#[test]
fn default_gossip_options_suppress_quests_before_npc_flags_and_select_highest_condition() {
    let mut raw = MechanicsInput {
        givers: vec![(900, 44001, 3)],
        menus: vec![
            GossipMenuRow {
                entry: 44001,
                text: 75001,
                script: 0,
                condition: 0,
            },
            GossipMenuRow {
                entry: 44001,
                text: 75002,
                script: 0,
                condition: 4,
            },
        ],
        conditions: vec![GossipConditionRow {
            id: 4,
            kind: 9,
            values: [71001, 2, 0, 0],
            flags: 0,
        }],
        options: vec![option(4)],
        ..MechanicsInput::default()
    };
    let c = Catalog::new_with_mechanics(
        vec![quest(71001)],
        relations(&[quest(71001)]),
        audit(),
        Mechanics::compile(raw.clone()).unwrap(),
    )
    .unwrap();
    let s = State::new(&c);
    assert_eq!(
        c.mechanics().gossip(900, &s.view()).unwrap(),
        Some((75001, false))
    );
    let h = InventoryHost {
        catalog: inventory::Catalog::new(vec![], inventory_audit()).unwrap(),
        state: inventory::State::new(BTreeMap::new(), 1).unwrap(),
        money: 0,
    };
    let accepted = reduce(
        &c,
        &s,
        &h,
        s.next_sequence(),
        Command::Accept {
            quest: 71001,
            giver: 900,
        },
    )
    .unwrap();
    assert_eq!(
        c.mechanics().gossip(900, &accepted.state.view()).unwrap(),
        Some((75002, true))
    );
    raw.options[0].npc_flags = 128;
    let m = Mechanics::compile(raw.clone()).unwrap();
    assert_eq!(m.gossip(900, &s.view()).unwrap(), Some((75001, false)));
    raw.conditions[0].kind = 19;
    assert!(matches!(
        Mechanics::compile(raw),
        Err(Error::Relationship {
            field: "conditions.type",
            target: 19,
            ..
        })
    ));
}
#[test]
fn reachable_vendor_option_refuses_but_unmatched_default_option_is_inert() {
    let mut row = option(0);
    row.option = 3;
    row.npc_flags = 4;
    let mut raw = MechanicsInput {
        givers: vec![(900, 44001, 3)],
        menus: vec![GossipMenuRow {
            entry: 44001,
            text: 75001,
            script: 0,
            condition: 0,
        }],
        options: vec![row],
        ..MechanicsInput::default()
    };
    Mechanics::compile(raw.clone()).unwrap();
    raw.givers[0].2 = 7;
    assert!(matches!(
        Mechanics::compile(raw),
        Err(Error::Relationship {
            field: "gossip_menu_option.option_id",
            target: 3,
            reason: "unsupported_option",
            ..
        })
    ));
}
