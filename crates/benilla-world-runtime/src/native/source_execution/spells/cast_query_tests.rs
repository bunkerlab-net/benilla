use super::*;

// Authored inputs only. Expected branches come from Unit.cpp:4349-4381.
fn authored(state: SourceSpellState, flags: u32) -> SourceSpell {
    let t = SpellTemplate {
        id: 7,
        ..Default::default()
    };
    let mut s = SourceSpell::new(
        1,
        &t,
        ObjectHandle {
            thread: 1,
            construction: 1,
        },
        SpellTriggerFlags(flags),
        0,
    )
    .unwrap();
    s.state = state;
    s
}

#[test]
fn authored_generic_query_states_and_five_operands() {
    use SourceSpellState::*;
    // :4351 excludes only FINISHED; :4357 excludes TRAVELING, not DELAYED.
    for (state, ordinary, delayed) in [
        (Created, true, true),
        (Targeting, true, true),
        (Casting, true, true),
        (Delayed, true, true),
        (Traveling, false, true),
        (Landing, true, true),
        (Channeling, true, true),
        (Finished, false, false),
    ] {
        for (with_delayed, expected) in [(false, ordinary), (true, delayed)] {
            let spells = BTreeMap::from([(1, authored(state, 0))]);
            let mut stores = SourceSpellStores::default();
            stores
                .insert_template(SpellTemplate {
                    id: 7,
                    interruptflags: 1,
                    ..Default::default()
                })
                .unwrap();
            let slots = SourceSpellSlots {
                current: [None, Some(1), None, None],
            };
            assert_eq!(
                slots
                    .is_non_melee_spell_casted(
                        &spells,
                        &stores,
                        with_delayed,
                        false,
                        true,
                        false,
                        false
                    )
                    .unwrap(),
                expected,
                "{state:?}, withDelayed={with_delayed}"
            );
            assert_eq!(
                slots
                    .is_non_melee_spell_casted(
                        &spells,
                        &stores,
                        with_delayed,
                        false,
                        true,
                        true,
                        false
                    )
                    .unwrap(),
                expected,
                "{state:?}, movement"
            );
        }
    }
}

#[test]
fn authored_generic_movement_and_auto_ignore_short_circuit() {
    // A missing channel identity witnesses fall-through, even when :4381 returns true.
    for (movement, auto_ignore, interrupt, attr, returns_early) in [
        (false, false, 0, 0, true),
        (true, false, 0, 0, false),
        (true, false, 1, 0, true),
        (false, false, 0, 0x20000, true),
        (false, true, 0, 0, true),
        (false, true, 0, 0x20000, false),
        (true, true, 1, 0, true),
        (true, true, 1, 0x20000, false),
    ] {
        let slots = SourceSpellSlots {
            current: [None, Some(1), Some(99), None],
        };
        let spells = BTreeMap::from([(1, authored(SourceSpellState::Casting, 0))]);
        let mut stores = SourceSpellStores::default();
        stores
            .insert_template(SpellTemplate {
                id: 7,
                interruptflags: interrupt,
                attributesex2: attr,
                ..Default::default()
            })
            .unwrap();
        let result = slots.is_non_melee_spell_casted(
            &spells,
            &stores,
            false,
            false,
            true,
            movement,
            auto_ignore,
        );
        if returns_early {
            assert!(result.unwrap());
        } else {
            assert_eq!(
                result.unwrap_err().id,
                "execution.spells.IsNonMeleeSpellCasted_current_spell_owner"
            );
        }
    }
}

#[test]
fn authored_channel_query_both_sides() {
    // :4363,4368-4373. OLD_TRIGGERED is m_IsTriggeredSpell (Spell.cpp:432).
    for (skip, movement, parallel, triggered, finished, expected) in [
        (false, false, false, false, false, true),
        (true, false, false, false, false, false),
        (false, false, true, false, false, false),
        (false, true, true, false, false, true),
        (false, true, false, false, false, true),
        (false, false, false, true, false, false),
        (false, false, false, false, true, false),
    ] {
        let slots = SourceSpellSlots {
            current: [None, None, Some(1), None],
        };
        let spells = BTreeMap::from([(
            1,
            authored(
                if finished {
                    SourceSpellState::Finished
                } else {
                    SourceSpellState::Channeling
                },
                u32::from(triggered),
            ),
        )]);
        let mut stores = SourceSpellStores::default();
        stores
            .insert_template(SpellTemplate {
                id: 7,
                attributesex4: if parallel { 0x80 } else { 0 },
                ..Default::default()
            })
            .unwrap();
        assert_eq!(
            slots
                .is_non_melee_spell_casted(&spells, &stores, false, skip, true, movement, false)
                .unwrap(),
            expected
        );
    }
    // Auto-ignore prevents a channel return. An unknown autorepeat identity is
    // legal here because :4378 reads only its pointer, never its Spell object.
    for attr in [0, 0x20000] {
        let slots = SourceSpellSlots {
            current: [None, None, Some(1), Some(99)],
        };
        let spells = BTreeMap::from([(1, authored(SourceSpellState::Channeling, 0))]);
        let mut stores = SourceSpellStores::default();
        stores
            .insert_template(SpellTemplate {
                id: 7,
                attributesex2: attr,
                ..Default::default()
            })
            .unwrap();
        assert!(slots
            .is_non_melee_spell_casted(&spells, &stores, false, false, false, true, true)
            .unwrap());
    }
}

#[test]
fn authored_empty_melee_autorepeat_and_missing_reads() {
    let stores = SourceSpellStores::default();
    let spells = BTreeMap::new();
    for auto_ignore in [false, true] {
        // Melee is excluded even if its identity has no owner, :4345.
        let slots = SourceSpellSlots {
            current: [Some(99), None, None, None],
        };
        assert_eq!(
            slots
                .is_non_melee_spell_casted(
                    &spells,
                    &stores,
                    false,
                    false,
                    false,
                    false,
                    auto_ignore
                )
                .unwrap(),
            auto_ignore
        );
    }
    for skip in [false, true] {
        let slots = SourceSpellSlots {
            current: [None, None, None, Some(99)],
        };
        assert_eq!(
            slots
                .is_non_melee_spell_casted(&spells, &stores, false, false, skip, false, false)
                .unwrap(),
            !skip
        );
    }
    let slots = SourceSpellSlots {
        current: [None, Some(1), None, None],
    };
    assert_eq!(
        slots
            .is_non_melee_spell_casted(&spells, &stores, false, false, true, false, false)
            .unwrap_err()
            .id,
        "execution.spells.IsNonMeleeSpellCasted_current_spell_owner"
    );
    let spells = BTreeMap::from([(1, authored(SourceSpellState::Casting, 0))]);
    // No template read without movement/auto-ignore, :4354-4356.
    assert!(slots
        .is_non_melee_spell_casted(&spells, &stores, false, false, true, false, false)
        .unwrap());
    assert_eq!(
        slots
            .is_non_melee_spell_casted(&spells, &stores, false, false, true, true, false)
            .unwrap_err()
            .id,
        "execution.spells.IsNonMeleeSpellCasted_spell_info_owner"
    );
    let spells = BTreeMap::from([(1, authored(SourceSpellState::Finished, 0))]);
    assert!(!slots
        .is_non_melee_spell_casted(&spells, &stores, false, false, true, true, false)
        .unwrap());
}

#[test]
fn authored_live_slots_are_excluded_from_serialized_unit_bytes() {
    let template = CreatureAiTemplateOperands {
        ordinal: 1,
        entry: 901,
        faction: 14,
        extra_flags: 0,
        unit_flags: 0,
        type_flags: 0,
        static_flags: [0; 4],
        rank: 0,
        inhabit_type: 1,
        stats_multipliers: [0x3f800000; 3],
        civilian: false,
        pursuit_ms: 15000,
        call_for_help: 0,
        creature_type: 0,
        ai_name: String::new(),
        script_name: String::new(),
    };
    let mut unit = SourceUnitFields::creature(&template);
    let before = serde_json::to_vec(&unit).unwrap();
    for slot in [
        SpellSlot::Melee,
        SpellSlot::Generic,
        SpellSlot::Channel,
        SpellSlot::AutoRepeat,
    ] {
        assert_eq!(unit.spell_slots.current(slot), None); // Unit.cpp:350-351.
    }
    unit.spell_slots.current = [Some(1), Some(2), Some(3), Some(4)];
    assert_eq!(serde_json::to_vec(&unit).unwrap(), before);
    assert!(!String::from_utf8(before).unwrap().contains("spell_slots"));
    assert_eq!(
        unit.spell_slots
            .is_non_melee_spell_casted(
                &(),
                &SourceSpellStores::default(),
                false,
                false,
                true,
                true,
                false,
            )
            .unwrap_err()
            .id,
        "execution.spells.IsNonMeleeSpellCasted_live_spell_owner"
    );
}

#[test]
fn authored_channel_states_and_missing_owner_read_order() {
    use SourceSpellState::*;
    let slots = SourceSpellSlots {
        current: [None, None, Some(1), None],
    };
    let stores = SourceSpellStores::default();
    for (state, expected) in [
        (Created, true),
        (Targeting, true),
        (Casting, true),
        (Delayed, true),
        (Traveling, true),
        (Landing, true),
        (Channeling, true),
        (Finished, false),
    ] {
        let spells = BTreeMap::from([(1, authored(state, 0))]);
        // Movement suppresses the attribute read and auto-ignore is false,
        // so no template is needed, Unit.cpp:4368-4372.
        assert_eq!(
            slots
                .is_non_melee_spell_casted(&spells, &stores, false, false, true, true, false)
                .unwrap(),
            expected
        );
        // Without movement even FINISHED reads the attribute first, :4369.
        assert_eq!(
            slots
                .is_non_melee_spell_casted(&spells, &stores, false, false, true, false, false)
                .unwrap_err()
                .id,
            "execution.spells.IsNonMeleeSpellCasted_spell_info_owner"
        );
    }
    assert_eq!(
        slots
            .is_non_melee_spell_casted(&(), &stores, false, false, true, false, false)
            .unwrap_err()
            .id,
        "execution.spells.IsNonMeleeSpellCasted_live_spell_owner"
    );
    assert!(!slots
        .is_non_melee_spell_casted(&(), &stores, false, true, true, false, false)
        .unwrap());
    // INSTANT_CAST is independent of OLD_TRIGGERED, Spell.cpp:432,473.
    let spells = BTreeMap::from([(1, authored(Channeling, 8))]);
    assert!(slots
        .is_non_melee_spell_casted(&spells, &stores, false, false, true, true, false)
        .unwrap());
    assert_eq!(
        slots
            .is_non_melee_spell_casted(&spells, &stores, false, false, true, true, true)
            .unwrap_err()
            .id,
        "execution.spells.IsNonMeleeSpellCasted_spell_info_owner"
    );
}
