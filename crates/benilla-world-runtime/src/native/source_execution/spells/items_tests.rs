use super::*;
use SpellCastResult as R;

fn handle() -> ObjectHandle {
    ObjectHandle {
        thread: 1,
        construction: 1,
    }
}
fn template() -> SpellTemplate {
    SpellTemplate {
        id: 7,
        equippeditemclass: -1,
        ..Default::default()
    }
}
struct Owner {
    player: bool,
    reads: usize,
}
impl SpellExecutionOwner for Owner {
    fn unit(&mut self, _: ObjectHandle) -> Result<SpellUnitState> {
        self.reads += 1;
        Ok(SpellUnitState {
            player: self.player,
            gameobject: false,
            alive: true,
            standing: true,
            player_controlled: false,
            combat: false,
            battleground_wait_leave: false,
            gm: false,
            moving: false,
            falling_far: false,
            stealth: false,
            form: 0,
            class: 1,
            combo_target: None,
            mounted: false,
            taxi: false,
            totem: false,
            untargetable: false,
            has_ai: false,
            position: [0; 4],
        })
    }
}
fn spell(t: &SpellTemplate, flags: u32) -> SourceSpell {
    SourceSpell::new(1, t, handle(), SpellTriggerFlags(flags), 0).unwrap()
}
fn check(s: &mut SourceSpell, t: &SpellTemplate) -> Result<R> {
    Owner {
        player: true,
        reads: 0,
    }
    .check(s, t, SpellCheck::Items)
}
fn refusal(s: &mut SourceSpell, t: &SpellTemplate, name: &str) {
    assert_eq!(
        check(s, t).unwrap_err().id,
        format!("execution.spells.{name}_owner")
    );
}

#[test]
fn nonplayers_skip_items_before_target_cast_item_equipment_focus_and_reagents() {
    // Spell.cpp:6438-6439, before every other operand.
    let t = SpellTemplate {
        requiresspellfocus: 1,
        reagent1: 1,
        totem1: 1,
        equippeditemclass: 2,
        effect1: 53,
        ..template()
    };
    let mut s = spell(&t, 0);
    s.item_cast = true;
    s.targets.unit = Some(handle());
    s.targets.item_guid = 1;
    s.targets.item = Some(handle());
    let mut owner = Owner {
        player: false,
        reads: 0,
    };
    assert_eq!(owner.check(&mut s, &t, SpellCheck::Items).unwrap(), R::Ok);
    assert_eq!(owner.reads, 1);
    refusal(&mut s, &t, "f5_25c_Item_IsInTrade");
}

#[test]
fn player_target_type_read_precedes_cast_item_read() {
    // Spell.cpp:6443-6450.
    let t = template();
    let mut s = spell(&t, 0);
    let mut owner = Owner {
        player: true,
        reads: 0,
    };
    assert_eq!(owner.check(&mut s, &t, SpellCheck::Items).unwrap(), R::Ok);
    assert_eq!(owner.reads, 1);
    s.targets.unit = Some(handle());
    s.item_cast = true;
    assert!(owner
        .check(&mut s, &t, SpellCheck::Items)
        .unwrap_err()
        .id
        .ends_with("Item_IsInTrade_owner"));
    assert_eq!(owner.reads, 3);
}

#[test]
fn unresolved_target_item_reports_source_trigger_trade_order() {
    // Spell.cpp:6525-6527. Ignore costs does not skip target checks.
    let t = template();
    for (flags, mask, expected) in [
        (0, 0, R::ItemGone),
        (1, 0, R::DontReport),
        (1, 0x1000, R::ItemGone),
        (0x400, 0, R::ItemGone),
        (8, 0, R::ItemGone),
        (1 | 0x20000, 0, R::DontReport),
    ] {
        let mut s = spell(&t, flags);
        s.targets.item_guid = 1;
        s.targets.mask = mask;
        assert_eq!(check(&mut s, &t).unwrap(), expected);
        s.targets.item = Some(handle());
        refusal(&mut s, &t, "f5_25a_Item_IsFitToSpellRequirements_GetProto");
    }
    assert_eq!(check(&mut spell(&t, 0), &t).unwrap(), R::Ok);
}

#[test]
fn negative_equipment_classes_skip_hand_requirements() {
    // Player.cpp:18645 precedes :18648/:18655, not just class == -1.
    for class in [-2, -1] {
        let t = SpellTemplate {
            equippeditemclass: class,
            attributesex3: 0x01000400,
            ..template()
        };
        assert_eq!(check(&mut spell(&t, 0), &t).unwrap(), R::Ok);
    }
}

#[test]
fn equipment_reads_preserve_mainhand_offhand_scan_order() {
    // Player.cpp:18648-18688. Triggering cannot turn an unknown live read into success.
    for flags in [0, 1, 0x400, 0x8000] {
        for class in [0, 2, 4] {
            let mut t = SpellTemplate {
                equippeditemclass: class,
                attributesex3: 0x01000400,
                ..template()
            };
            refusal(
                &mut spell(&t, flags),
                &t,
                "f5_25b_Player_hasMainhandWeaponForAttack",
            );
            t.attributesex3 = 0x01000000;
            refusal(
                &mut spell(&t, flags),
                &t,
                "f5_25b_Player_hasOffhandWeaponForAttack",
            );
            t.attributesex3 = 0;
            if matches!(class, 2 | 4) {
                refusal(
                    &mut spell(&t, flags),
                    &t,
                    "f5_25b_Player_GetItemByPos_equipment",
                );
            } else {
                assert_eq!(
                    check(&mut spell(&t, flags), &t).unwrap(),
                    if flags == 1 {
                        R::DontReport
                    } else {
                        R::EquippedItemClass
                    }
                );
            }
        }
    }
}

#[test]
fn unsupported_equipment_classes_reject_before_focus_or_inventory() {
    // Player.cpp:18694-18699; Spell.cpp:6542.
    for class in [0, 1, 3, 5, i32::MAX] {
        let t = SpellTemplate {
            equippeditemclass: class,
            requiresspellfocus: 1,
            reagent1: 1,
            ..template()
        };
        assert_eq!(check(&mut spell(&t, 0), &t).unwrap(), R::EquippedItemClass);
        assert_eq!(check(&mut spell(&t, 1), &t).unwrap(), R::DontReport);
    }
}

#[test]
fn focus_precedes_reagents_even_when_costs_are_ignored() {
    // Spell.cpp:6546-6560.
    let t = SpellTemplate {
        requiresspellfocus: 1,
        reagent1: 1,
        ..template()
    };
    for flags in [0, 1, 0x400, 0x8000] {
        assert_eq!(
            check(&mut spell(&t, flags), &t).unwrap_err(),
            SpellCheck::Focus.missing_owner()
        );
    }
    assert_eq!(
        check(&mut spell(&template(), 0), &template()).unwrap(),
        R::Ok
    );
}

#[test]
fn reagents_scan_all_eight_slots_skip_nonpositive_and_do_not_skip_zero_counts() {
    // Spell.cpp:6564-6590; Player.cpp:8462-8464.
    let mut t = SpellTemplate {
        reagent1: -1,
        reagent2: i32::MIN,
        ..template()
    };
    assert_eq!(check(&mut spell(&t, 0), &t).unwrap(), R::Ok);
    for slot in 0..8 {
        let mut values = [0; 8];
        values[slot] = 1;
        [
            t.reagent1, t.reagent2, t.reagent3, t.reagent4, t.reagent5, t.reagent6, t.reagent7,
            t.reagent8,
        ] = values;
        refusal(&mut spell(&t, 0), &t, "f5_25a_Player_HasItemCount_reagent");
    }
}

#[test]
fn reagents_precede_totems_and_totems_scan_both_slots() {
    // Spell.cpp:6590 precedes :6604.
    let mut t = SpellTemplate {
        reagent8: 1,
        totem1: 1,
        totem2: 1,
        ..template()
    };
    refusal(&mut spell(&t, 0), &t, "f5_25a_Player_HasItemCount_reagent");
    t.reagent8 = 0;
    refusal(&mut spell(&t, 0), &t, "f5_25a_Player_HasItemCount_totem");
    t.totem1 = 0;
    refusal(&mut spell(&t, 0), &t, "f5_25a_Player_HasItemCount_totem");
    t.totem2 = 0;
    assert_eq!(check(&mut spell(&t, 0), &t).unwrap(), R::Ok);
}

#[test]
fn ignore_flags_and_trigger_master_reagent_preserve_source_quirks() {
    // Spell.cpp:6406,6421-6433. Ordinary triggering sets ignore costs;
    // FORCE_COSTS exposes the master-spell first-slot test.
    let t = SpellTemplate {
        reagent1: 1,
        totem1: 1,
        ..template()
    };
    for flags in [1, 0x400, 0x8000, 1 | 0x20000 | 0x400] {
        let mut s = spell(&t, flags);
        s.triggered_by_spell_reagent = Some(0);
        assert_eq!(check(&mut s, &t).unwrap(), R::Ok);
    }
    for flags in [0, 8, 0x20000] {
        refusal(
            &mut spell(&t, flags),
            &t,
            "f5_25a_Player_HasItemCount_reagent",
        );
    }
    for id in [7, 46546] {
        let mut t = t.clone();
        t.id = id;
        let flags = if id == 7 { 1 | 0x20000 } else { 0 };
        for reagent in [None, Some(-1), Some(1), Some(0)] {
            let mut s = spell(&t, flags);
            s.triggered_by_spell_reagent = reagent;
            if reagent == Some(0) {
                refusal(&mut s, &t, "f5_25a_Player_HasItemCount_reagent");
            } else {
                assert_eq!(check(&mut s, &t).unwrap(), R::Ok);
            }
        }
    }
}

#[test]
fn triggered_trade_owner_read_is_not_replaced_with_a_default() {
    // Spell.cpp:6424-6426, after early ignore flags, before master reagent.
    let t = template();
    for flags in [1 | 0x20000, 0] {
        let mut t = t.clone();
        if flags == 0 {
            t.id = 46546;
        }
        let mut s = spell(&t, flags);
        s.targets.item = Some(handle());
        refusal(&mut s, &t, "f5_25a_Item_GetOwnerGuid_ignore_requirements");
        s.flags = SpellTriggerFlags(flags | 0x400);
        assert_eq!(check(&mut s, &t).unwrap(), R::Ok);
    }
}

#[test]
fn create_item_checks_only_after_targeting_for_nontriggered_nonzero_items() {
    // Spell.cpp:6642-6650.
    for state in [SourceSpellState::Targeting, SourceSpellState::Casting] {
        for flags in [0, 1, 0x400] {
            for item in [0, 1] {
                let t = SpellTemplate {
                    effect1: 24,
                    effectitemtype1: item,
                    ..template()
                };
                let mut s = spell(&t, flags);
                s.state = state;
                if state != SourceSpellState::Targeting && flags != 1 && item != 0 {
                    refusal(&mut s, &t, "f5_25a_ObjectMgr_GetItemPrototype");
                } else {
                    assert_eq!(check(&mut s, &t).unwrap(), R::Ok);
                }
            }
        }
    }
}

#[test]
fn enchant_and_disenchant_distinguish_null_pointer_from_unbound_item_state() {
    // Spell.cpp:6667-6671,6690-6694,6713-6717. Held enchants do nothing here.
    for (effect, expected, name) in [
        (53, R::ItemGone, "f5_25a_Item_GetProto_enchant"),
        (54, R::ItemGone, "f5_25a_Item_GetOwner_temporary_enchant"),
        (
            99,
            R::CantBeDisenchanted,
            "f5_25a_Item_GetOwnerGuid_disenchant",
        ),
    ] {
        for flags in [0, 1, 0x400] {
            let t = SpellTemplate {
                effect1: effect,
                ..template()
            };
            let mut s = spell(&t, flags);
            assert_eq!(check(&mut s, &t).unwrap(), expected);
            s.targets.item = Some(handle());
            refusal(&mut s, &t, name);
        }
    }
    let t = SpellTemplate {
        effect1: 92,
        ..template()
    };
    assert_eq!(check(&mut spell(&t, 0), &t).unwrap(), R::Ok);
}

#[test]
fn weapon_checks_request_ranged_equipment_only_for_exact_source_effects() {
    // Spell.cpp:6729-6735. Percent and normalized weapon damage are not cases.
    for effect in [17, 58, 31, 121] {
        let t = SpellTemplate {
            effect1: effect,
            ..template()
        };
        for attack in [0, 1, 2] {
            let mut s = spell(&t, 0);
            s.attack_type = attack;
            if matches!(effect, 17 | 58) && attack == 2 {
                refusal(&mut s, &t, "f5_25b_Player_GetWeaponForAttack_ranged");
            } else {
                assert_eq!(check(&mut s, &t).unwrap(), R::Ok);
            }
        }
    }
}

#[test]
fn effect_slots_preserve_first_failure_and_do_not_skip_later_slots() {
    // Spell.cpp:6638.
    let mut t = SpellTemplate {
        effect1: 99,
        effect2: 53,
        effect3: 24,
        effectitemtype3: 1,
        ..template()
    };
    assert_eq!(check(&mut spell(&t, 0), &t).unwrap(), R::CantBeDisenchanted);
    t.effect1 = 0;
    assert_eq!(check(&mut spell(&t, 0), &t).unwrap(), R::ItemGone);
    t.effect2 = 0;
    refusal(&mut spell(&t, 0), &t, "f5_25a_ObjectMgr_GetItemPrototype");
}
