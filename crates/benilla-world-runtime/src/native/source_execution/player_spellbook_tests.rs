// SPDX-License-Identifier: GPL-3.0-or-later
use super::player_skills::PlayerSkillInputs;
use super::player_spellbook::PlayerLearning;
use super::*;

fn with_spells(
    templates: impl IntoIterator<Item = SpellTemplate>,
    in_world: bool,
    form: u32,
    run: impl FnOnce(&mut SourcePlayer, &PlayerLearning<'_>),
) {
    let mut stores = SourceSpellStores::default();
    for template in templates {
        stores.insert_template(template).unwrap();
    }
    stores.bind_test_learning();
    let items = super::tests::test_items([]);
    let skills = PlayerSkillInputs::default();
    let mut objects = SourceObjects::default();
    let mut units = SourceUnits::default();
    let mut player = super::tests::construct(&mut objects, &mut units);
    let handle = player.object();
    let unit = units.get_mut(&objects, handle).unwrap();
    unit.shapeshift_form = form;
    objects.get_mut(handle).unwrap().in_world = in_world;
    run(
        &mut player,
        &PlayerLearning {
            objects: &objects,
            unit,
            skills: &skills,
            spells: &stores,
            items: &items,
            class: 8,
        },
    );
}

fn spell(id: u32) -> SpellTemplate {
    SpellTemplate {
        id,
        ..Default::default()
    }
}

fn spell_with_effect(id: u32, effect: u32, trigger: u32) -> SpellTemplate {
    SpellTemplate {
        id,
        effect1: effect,
        effecttriggerspell1: trigger,
        ..Default::default()
    }
}

fn row(active: bool, dependent: bool, disabled: bool) -> SourcePlayerSpell {
    SourcePlayerSpell {
        state: SourcePlayerSpellState::New,
        active,
        dependent,
        disabled,
    }
}

#[test]
fn validity_precedes_insert_and_database_cleanup_has_an_owner() {
    with_spells([spell_with_effect(2, 36, 999)], false, 0, |p, live| {
        for id in [1, 2] {
            assert!(!p.add_spell(live, id, true, true, true, false).unwrap());
        }
        for (id, reason) in [
            (
                1,
                "execution.player.Player_3126.character_spell_delete_owner",
            ),
            (
                2,
                "execution.player.Player_3140.character_spell_delete_owner",
            ),
        ] {
            assert_eq!(
                p.add_spell(live, id, true, false, true, false)
                    .unwrap_err()
                    .id,
                reason
            );
        }
        assert!(p.spellbook().is_empty());
    });
}

#[test]
fn duplicate_passive_grant_returns_before_a_second_cast() {
    let template = SpellTemplate {
        attributes: 0x40,
        ..spell(1)
    };
    with_spells([template], false, 0, |p, live| {
        assert_eq!(
            p.add_spell(live, 1, true, true, true, false)
                .unwrap_err()
                .id,
            "execution.player.Player_3400.passive_cast_owner"
        );
        assert_eq!(p.spellbook()[&1], row(true, true, false));
        assert!(!p.add_spell(live, 1, true, true, true, false).unwrap());
        assert_eq!(p.spellbook()[&1], row(true, true, false));
        assert!(!p.add_spell(live, 1, true, false, true, false).unwrap());
        assert_eq!(p.spellbook()[&1].state, SourcePlayerSpellState::Unchanged);
    });
}

#[test]
fn activation_mutates_before_passive_cast_and_disabling_does_not_cast() {
    let template = SpellTemplate {
        attributes: 0x40,
        stances: 0,
        ..spell(1)
    };
    with_spells([template], false, 0, |p, live| {
        p.spells.insert(1, row(false, false, false));
        assert_eq!(
            p.add_spell(live, 1, true, true, true, false)
                .unwrap_err()
                .id,
            "execution.player.Player_3204.passive_cast_owner"
        );
        assert_eq!(p.spellbook()[&1], row(true, true, false));
        assert!(!p.add_spell(live, 1, true, true, true, true).unwrap());
        assert_eq!(p.spellbook()[&1], row(true, true, true));
    });
}

#[test]
fn skill_step_and_aura_state_each_stop_at_their_source_call() {
    let caster_state = SpellTemplate {
        attributes: 0x40,
        casteraurastate: 1,
        ..spell(2)
    };
    with_spells(
        [spell_with_effect(1, 44, 0), caster_state],
        false,
        0,
        |p, live| {
            for (id, reason) in [
                (1, "execution.player.Player_3404.skill_step_cast_owner"),
                (2, "execution.player.Player_3448.caster_aura_state_owner"),
            ] {
                assert_eq!(
                    p.add_spell(live, id, true, true, true, false)
                        .unwrap_err()
                        .id,
                    reason
                );
                assert_eq!(p.spellbook()[&id], row(true, true, false));
            }
        },
    );
}

#[test]
fn nonzero_form_refuses_the_form_apply_predicate() {
    with_spells(
        [SpellTemplate {
            attributes: 0x80,
            ..spell(1)
        }],
        false,
        1,
        |p, live| {
            assert_eq!(
                p.add_spell(live, 1, true, true, true, false)
                    .unwrap_err()
                    .id,
                "execution.player.SpellMgr_1383.form_apply_owner"
            );
            assert_eq!(p.spellbook()[&1], row(true, true, false));
        },
    );
}

#[test]
fn in_world_learning_and_removal_refuse_the_session_send() {
    with_spells([spell(1)], true, 0, |p, live| {
        assert_eq!(
            p.learn_spell(live, 1, false, false).unwrap_err().id,
            "execution.player.Player_3465.send_learned_spell_owner"
        );
        assert_eq!(p.spellbook()[&1], row(true, false, false));
        assert_eq!(
            p.add_spell(live, 1, false, true, false, false)
                .unwrap_err()
                .id,
            "execution.player.Player_3214.send_removed_spell_owner"
        );
        assert_eq!(p.spellbook()[&1], row(false, false, false));
    });
}

#[test]
fn button_accepts_only_known_nonpassives_and_packs_its_kind() {
    let templates = [
        spell(1),
        SpellTemplate {
            attributes: 0x40,
            ..spell(2)
        },
        spell(3),
    ];
    with_spells(templates, false, 0, |p, live| {
        p.spells.insert(1, row(false, true, false));
        p.spells.insert(2, row(true, true, false));
        for (button, action, kind) in [
            (0, 1, 0),
            (1, 2, 0),
            (2, 3, 0),
            (3, 999, 0),
            (4, 999, 128),
            (5, 23, 64),
            (120, 23, 64),
            (6, 0x0100_0000, 64),
        ] {
            p.add_action_button(
                live,
                PlayerCreateAction {
                    button,
                    action,
                    kind,
                },
            )
            .unwrap();
        }
        let packed = |packed_data| SourcePlayerAction {
            packed_data,
            state: SourcePlayerActionState::New,
        };
        assert_eq!(
            p.action_buttons(),
            &BTreeMap::from([(0, packed(1)), (5, packed(0x4000_0017))])
        );
    });
}

#[test]
fn detached_dependency_chain_inserts_each_spell_once() {
    with_spells(
        [
            spell_with_effect(1, 36, 2),
            spell_with_effect(2, 36, 3),
            spell(3),
        ],
        false,
        0,
        |p, live| {
            assert!(p.add_spell(live, 1, true, true, false, false).unwrap());
            assert_eq!(
                p.spellbook(),
                &BTreeMap::from([
                    (1, row(true, false, false)),
                    (2, row(true, true, false)),
                    (3, row(true, true, false)),
                ])
            );
            assert!(!p.add_spell(live, 1, true, true, false, false).unwrap());
        },
    );
}
