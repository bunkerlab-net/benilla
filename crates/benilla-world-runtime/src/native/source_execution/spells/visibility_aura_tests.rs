// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

struct Target {
    object: ObjectHandle,
    objects: SourceObjects,
    unit: SourceUnitFields,
    map: MapIdentity,
    player: bool,
}

fn operands<'a>(
    target: ObjectHandle,
    objects: &'a SourceObjects,
    control_guids: &'a SourceControlGuids,
    player: bool,
) -> SelfAuraTarget<'a> {
    SelfAuraTarget {
        target,
        level: Some(19),
        alive: true,
        player,
        player_group: Some(false),
        totem: false,
        objects,
        control_guids,
    }
}

impl Target {
    fn new(player: bool, in_world: bool) -> Self {
        let mut objects = SourceObjects::default();
        let (kind, guid) = if player {
            (
                SourceObjectKind::Player,
                SourceGuid::without_entry(HighGuid::Player, 41),
            )
        } else {
            (
                SourceObjectKind::Creature,
                SourceGuid::with_entry(HighGuid::Creature, 901, 41),
            )
        };
        let object = objects.construct(6, kind).unwrap();
        objects.assign_guid(object, 41, guid).unwrap();
        let map = MapIdentity {
            map: 0,
            instance: 0,
            creation_generation: 1,
        };
        objects
            .bind_map(object, map, &SourceCells::new(map).unwrap())
            .unwrap();
        if in_world {
            objects.add_to_world(object).unwrap();
        }
        Self {
            object,
            objects,
            unit: SourceUnitFields::constructed_creature(),
            map,
            player,
        }
    }

    fn holder(&mut self, t: &SpellTemplate, amount: i32) -> SourceAuraHolderId {
        let mut holder = SourceAuraHolder::new(t, self.object, 0, 0, -1).unwrap();
        for (i, effect) in t.effects().iter().enumerate() {
            if effect.effect == 6 {
                holder.effects[i] = Some(
                    SourceAura::self_spell_effect(t, i, effect.aura, 0, amount, 1, 0).unwrap(),
                );
            }
        }
        self.unit
            .auras
            .store_aura_holder(t.id, &mut Some(holder))
            .unwrap()
    }

    fn remove(&mut self, stores: &SourceSpellStores, id: SourceAuraHolderId) -> Result<()> {
        self.unit.auras.remove_self_aura_holder(
            stores,
            id,
            AuraRemoveMode::Default,
            &mut SelfAuraRemoval {
                unit: operands(
                    self.object,
                    &self.objects,
                    &self.unit.control_guids,
                    self.player,
                ),
                visibility: &mut self.unit.visibility,
                damage: DamageAuraTarget::new(
                    &mut self.unit.creature_damage,
                    &self.unit.loaded_creature,
                    self.unit.flags,
                    self.unit.shapeshift_form,
                ),
            },
        )
    }
}

impl SpellExecutionOwner for Target {}
impl SelfAuraApplicationOwner for Target {
    fn self_aura_application(&mut self) -> Result<SelfAuraApplication<'_>> {
        Ok(SelfAuraApplication {
            unit: operands(
                self.object,
                &self.objects,
                &self.unit.control_guids,
                self.player,
            ),
            auras: &mut self.unit.auras,
            visibility: &mut self.unit.visibility,
            resistances: &mut self.unit.resistances,
            stat_modifiers: &mut self.unit.stat_modifiers,
            damage: DamageAuraTarget::new(
                &mut self.unit.creature_damage,
                &self.unit.loaded_creature,
                self.unit.flags,
                self.unit.shapeshift_form,
            ),
            can_modify_stats: self.unit.loaded_creature.can_modify_stats,
            threat_modifier: &mut self.unit.threat_modifier,
            power_fields: &mut self.unit.power_fields,
            power_type: self.unit.power_type.as_mut(),
            vitals: &mut self.unit.vitals,
            form: &mut self.unit.shapeshift_form,
        })
    }
}

#[test]
fn total_percent_stat_changes_unit_groups_before_the_player_or_health_write() {
    let stores = SourceSpellStores::default();
    let single = [false, false, true, false, false];
    for (player, misc, attributes, vitals, can_modify, changed, error) in [
        (false, -2, 0, None, false, [false; 5], None),
        (false, 5, 0, Some((5, 10)), false, [false; 5], None),
        (
            false,
            0,
            0,
            None,
            false,
            [false; 5],
            Some("execution.spells.SpellAuras_3553.health_owner"),
        ),
        (false, -1, 0x10, Some((5, 10)), false, [true; 5], None),
        (false, 2, 0, Some((5, 10)), false, single, None),
        (false, 2, 0x10, Some((0, 10)), false, single, None),
        (false, 2, 0x10, Some((5, 0)), false, single, None),
        (
            false,
            2,
            0x10,
            Some((5, 10)),
            false,
            single,
            Some("execution.spells.SpellAuras_3571.SetHealth_owner"),
        ),
        (
            true,
            -1,
            0,
            Some((5, 10)),
            true,
            [true, false, false, false, false],
            Some("execution.spells.Unit_9017.UpdateStats_owner"),
        ),
        (
            true,
            -1,
            0,
            Some((5, 10)),
            false,
            [true, false, false, false, false],
            Some("execution.spells.AuraHandler.type_137_owner"),
        ),
    ] {
        let mut target = Target::new(player, false);
        target.unit.loaded_creature.can_modify_stats = can_modify;
        target.unit.vitals = vitals.map(|(health, max_health)| SourceUnitVitals {
            health,
            max_health,
            mana: 0,
            max_mana: 0,
            has_mana: false,
            energy: 0,
            max_energy: 0,
        });
        let before = target.unit.vitals;
        let t = SpellTemplate {
            id: 1,
            attributes,
            effect1: 6,
            effectapplyauraname1: 137,
            effectmiscvalue1: misc,
            ..Default::default()
        };
        let id = target.holder(&t, 25);
        assert_eq!(
            apply_self_aura_holder(&mut target, &stores, &t, id)
                .err()
                .map(|err| err.id),
            error.map(str::to_owned),
        );
        assert_eq!(
            target.unit.stat_modifiers.map(|group| group.total_percent),
            changed.map(|changed| if changed { 1.25 } else { 1.0 }),
        );
        assert_eq!(target.unit.vitals, before);
        let holder = target.unit.auras.first_aura_holder(t.id).unwrap();
        assert_eq!(holder.effects[0].as_ref().unwrap().applied, error.is_none());
    }
}

fn invisibility(stores: &mut SourceSpellStores, id: u32, misc: i32) -> SpellTemplate {
    let t = SpellTemplate {
        id,
        effect1: 6,
        effectapplyauraname1: 18,
        effectmiscvalue1: misc,
        ..Default::default()
    };
    stores.insert_template(t.clone()).unwrap();
    t
}

#[test]
fn invisibility_round_trip_across_indices_preserves_saved_unit_bytes() {
    let mut stores = SourceSpellStores::default();
    let unseen = invisibility(&mut stores, 4986, 0);
    let other = invisibility(&mut stores, 9000, 31);
    let mut target = Target::new(false, false);
    let bytes = serde_json::to_vec(&target.unit).unwrap();
    assert_eq!(target.unit.visibility, SourceUnitVisibility::default());
    let main = target.holder(&unseen, 200);
    apply_self_aura_holder(&mut target, &stores, &unseen, main).unwrap();
    assert_eq!(target.unit.visibility.values[0], 200);
    assert_eq!(target.unit.visibility.mask, 0x0000_0001);
    assert_eq!(
        target.unit.visibility.mode,
        SourceUnitVisibilityMode::GroupInvisibility
    );
    let extra = target.holder(&other, 45);
    apply_self_aura_holder(&mut target, &stores, &other, extra).unwrap();
    assert_eq!(target.unit.visibility.values[31], 45);
    assert_eq!(target.unit.visibility.mask, 0x8000_0001);
    assert_eq!(serde_json::to_vec(&target.unit).unwrap(), bytes);
    target.remove(&stores, main).unwrap();
    assert_eq!(target.unit.visibility.values[0], 0);
    assert_eq!(target.unit.visibility.mask, 0x8000_0000);
    assert_eq!(
        target.unit.visibility.mode,
        SourceUnitVisibilityMode::GroupInvisibility
    );
    target.remove(&stores, extra).unwrap();
    assert_eq!(target.unit.visibility, SourceUnitVisibility::default());
    assert_eq!(serde_json::to_vec(&target.unit).unwrap(), bytes);
    assert!(!target
        .objects
        .client_update_pending(target.map, target.object));
}

#[test]
fn scripted_invisibility_unapply_keeps_live_periodicity() {
    let mut stores = SourceSpellStores::default();
    let t = invisibility(&mut stores, 16380, 0);
    stores.bind_empty_duration_fixture();
    stores
        .bind_script(
            t.id,
            b"spell_detect_through_invisibility_mob",
            &mut SourceSpellScriptRegistry,
        )
        .unwrap();
    let mut target = Target::new(false, false);
    let saved = serde_json::to_vec(&target.unit).unwrap();
    let mut holder =
        SourceAuraHolder::for_self_spell(&stores, &t, target.object, None, || Ok((0, 0))).unwrap();
    holder.initialize();
    holder.effects[0] = Some(SourceAura::self_spell_effect(&t, 0, 18, 0, 300, 1, 0).unwrap());
    let id = target
        .unit
        .auras
        .store_aura_holder(t.id, &mut Some(holder))
        .unwrap();
    apply_self_aura_holder(&mut target, &stores, &t, id).unwrap();
    let aura = target.unit.auras.aura_holder_mut(id).unwrap().effects[0]
        .as_mut()
        .unwrap();
    assert_eq!(
        (
            aura.applied,
            aura.periodic,
            aura.periodic_time,
            aura.periodic_timer
        ),
        (true, true, 1000, 1000)
    );
    aura.periodic_timer = 871;
    aura.periodic_tick = 3;
    target.remove(&stores, id).unwrap();
    let removed = &target.unit.auras.deferred_auras()[0];
    assert_eq!(
        (
            removed.applied,
            removed.periodic,
            removed.periodic_time,
            removed.periodic_timer,
            removed.periodic_tick
        ),
        (false, true, 1000, 871, 3)
    );
    assert_eq!(target.unit.visibility, SourceUnitVisibility::default());
    assert_eq!(serde_json::to_vec(&target.unit).unwrap(), saved);
}

#[test]
fn invisibility_preserves_gm_off_and_clears_no_detect_only_without_stealth() {
    let mut stores = SourceSpellStores::default();
    let t = invisibility(&mut stores, 4986, 0);
    let stealth = SpellTemplate {
        id: 1784,
        effect1: 6,
        effectapplyauraname1: 16,
        ..Default::default()
    };
    stores.insert_template(stealth.clone()).unwrap();
    for (mode, stealthed, expected) in [
        (
            SourceUnitVisibilityMode::Off,
            false,
            SourceUnitVisibilityMode::Off,
        ),
        (
            SourceUnitVisibilityMode::GroupStealth,
            true,
            SourceUnitVisibilityMode::GroupStealth,
        ),
        (
            SourceUnitVisibilityMode::GroupNoDetect,
            false,
            SourceUnitVisibilityMode::On,
        ),
    ] {
        let mut target = Target::new(false, false);
        target.unit.visibility.mode = mode;
        if stealthed {
            target.holder(&stealth, 1);
        }
        let holder = target.holder(&t, 200);
        apply_self_aura_holder(&mut target, &stores, &t, holder).unwrap();
        assert_eq!(target.unit.visibility.mode, mode);
        target.remove(&stores, holder).unwrap();
        assert_eq!(target.unit.visibility.mask, 0);
        assert_eq!(target.unit.visibility.mode, expected);
    }
}

#[test]
fn invisibility_interrupts_matching_holders_even_on_positive_unapply() {
    let mut stores = SourceSpellStores::default();
    let t = invisibility(&mut stores, 4986, 0);
    let another = invisibility(&mut stores, 4987, 0);
    let canceled = SpellTemplate {
        id: 11,
        aurainterruptflags: 0x0010_0000,
        effect1: 6,
        effectapplyauraname1: 42,
        ..Default::default()
    };
    let kept = SpellTemplate {
        id: 12,
        aurainterruptflags: 0x8000,
        ..canceled.clone()
    };
    stores.insert_template(canceled.clone()).unwrap();
    stores.insert_template(kept.clone()).unwrap();
    let mut target = Target::new(false, false);
    let removed = target.holder(&canceled, 1);
    let surviving = target.holder(&kept, 1);
    let first = target.holder(&t, 200);
    apply_self_aura_holder(&mut target, &stores, &t, first).unwrap();
    assert!(target.unit.auras.aura_holder(removed).is_none());
    let second = target.holder(&another, 20);
    apply_self_aura_holder(&mut target, &stores, &another, second).unwrap();
    let removed_again = target.holder(&canceled, 1);
    target.remove(&stores, second).unwrap();
    assert_eq!(target.unit.visibility.values[0], 200);
    assert_eq!(target.unit.visibility.mask, 1);
    assert!(target.unit.auras.aura_holder(surviving).is_some());
    assert!(target.unit.auras.aura_holder(removed_again).is_none());
    assert_eq!(
        target
            .unit
            .auras
            .deferred_aura_holders()
            .map(|(id, _)| id)
            .collect::<Vec<_>>(),
        [removed, removed_again, second]
    );
}

#[test]
fn invisibility_refuses_player_glow_after_updating_values() {
    let mut stores = SourceSpellStores::default();
    let t = invisibility(&mut stores, 4986, 0);
    let mut target = Target::new(true, false);
    let holder = target.holder(&t, 200);
    assert_eq!(
        apply_self_aura_holder(&mut target, &stores, &t, holder)
            .unwrap_err()
            .id,
        "execution.spells.Aura_2647.GetMover_owner"
    );
    assert_eq!(
        (
            target.unit.visibility.mask,
            target.unit.visibility.values[0]
        ),
        (1, 200)
    );
    assert_eq!(target.unit.visibility.mode, SourceUnitVisibilityMode::On);
    assert_eq!(
        target.remove(&stores, holder).unwrap_err().id,
        "execution.spells.Aura_2666.RemoveByteFlag_owner"
    );
    assert_eq!(
        (
            target.unit.visibility.mask,
            target.unit.visibility.values[0]
        ),
        (0, 0)
    );
}

#[test]
fn invisibility_refuses_world_refresh_after_writing_live_state() {
    let mut stores = SourceSpellStores::default();
    let t = invisibility(&mut stores, 4986, 0);
    let mut target = Target::new(false, true);
    let holder = target.holder(&t, 200);
    let refresh = "execution.spells.Aura_2679.UpdateVisibilityAndView_owner";
    assert_eq!(
        apply_self_aura_holder(&mut target, &stores, &t, holder)
            .unwrap_err()
            .id,
        refresh
    );
    assert_eq!(
        target.unit.visibility.mode,
        SourceUnitVisibilityMode::GroupInvisibility
    );
    assert_eq!(target.remove(&stores, holder).unwrap_err().id, refresh);
    assert_eq!(target.unit.visibility.mode, SourceUnitVisibilityMode::On);
    assert!(!target
        .objects
        .client_update_pending(target.map, target.object));
}

#[test]
fn invisibility_refuses_undefined_indices_and_arithmetic_before_write() {
    let mut stores = SourceSpellStores::default();
    for (id, index) in [(9001, -1), (9002, 32)] {
        let t = invisibility(&mut stores, id, index);
        let mut target = Target::new(false, false);
        let holder = target.holder(&t, 1);
        assert_eq!(
            apply_self_aura_holder(&mut target, &stores, &t, holder)
                .unwrap_err()
                .id,
            "execution.spells.ObjectVisibility_71.invisibility_index_undefined"
        );
        assert_eq!(target.unit.visibility, SourceUnitVisibility::default());
    }
    let a = invisibility(&mut stores, 9003, 0);
    let b = invisibility(&mut stores, 9004, 0);
    let mut target = Target::new(false, false);
    let max = target.holder(&a, i32::MAX);
    apply_self_aura_holder(&mut target, &stores, &a, max).unwrap();
    let overflow = target.holder(&b, 1);
    let before = target.unit.visibility.clone();
    assert_eq!(
        apply_self_aura_holder(&mut target, &stores, &b, overflow)
            .unwrap_err()
            .id,
        "execution.spells.ObjectVisibility_71.invisibility_addition_undefined"
    );
    assert_eq!(target.unit.visibility, before);
    let mut target = Target::new(false, false);
    let min = target.holder(&a, i32::MIN);
    apply_self_aura_holder(&mut target, &stores, &a, min).unwrap();
    assert_eq!(target.unit.visibility.values[0], i32::MIN);
    assert_eq!(target.unit.visibility.mask, 0);
    assert_eq!(
        target.remove(&stores, min).unwrap_err().id,
        "execution.spells.Aura_2635.invisibility_amount_negation_undefined"
    );
    assert_eq!(target.unit.visibility.values[0], i32::MIN);
}

fn stealth(stores: &mut SourceSpellStores, id: u32, misc: i32) -> SpellTemplate {
    let t = SpellTemplate {
        id,
        effect1: 6,
        effectapplyauraname1: 16,
        effectmiscvalue1: misc,
        ..Default::default()
    };
    stores.insert_template(t.clone()).unwrap();
    t
}

#[test]
fn stealth_last_removal_restores_invisibility_or_on() {
    use SourceUnitVisibilityMode::*;
    let mut stores = SourceSpellStores::default();
    let hide = stealth(&mut stores, 6920, 0);
    let trap = stealth(&mut stores, 9005, 1);
    let unseen = invisibility(&mut stores, 4986, 0);
    for prior in [On, GroupInvisibility, Off] {
        let mut target = Target::new(false, false);
        let saved = serde_json::to_vec(&target.unit).unwrap();
        if prior == GroupInvisibility {
            let holder = target.holder(&unseen, 10);
            apply_self_aura_holder(&mut target, &stores, &unseen, holder).unwrap();
        } else {
            target.unit.visibility.mode = prior;
        }
        let first = target.holder(&hide, -1);
        let second = target.holder(&trap, 10);
        assert_eq!(target.unit.visibility.stealth_strength, [0, 0]);
        apply_self_aura_holder(&mut target, &stores, &hide, first).unwrap();
        apply_self_aura_holder(&mut target, &stores, &trap, second).unwrap();
        assert_eq!(target.unit.visibility.stealth_strength, [u32::MAX, 10]);
        assert_eq!(target.unit.auras.vis_flags, 2);
        let during = if prior == Off { Off } else { GroupStealth };
        assert_eq!(target.unit.visibility.mode, during);
        target.remove(&stores, first).unwrap();
        assert_eq!(target.unit.visibility.stealth_strength, [0, 10]);
        assert_eq!(target.unit.visibility.mode, during);
        assert_eq!(target.unit.auras.vis_flags, 2);
        target.remove(&stores, second).unwrap();
        assert_eq!(target.unit.visibility.stealth_strength, [0, 0]);
        assert_eq!(target.unit.visibility.mode, prior);
        assert_eq!(
            target.unit.auras.vis_flags,
            if prior == Off { 2 } else { 0 }
        );
        assert_eq!(serde_json::to_vec(&target.unit).unwrap(), saved);
    }
}

#[test]
fn stealth_non_real_apply_interrupts_matching_auras_without_creep() {
    let mut stores = SourceSpellStores::default();
    let interrupted = SpellTemplate {
        id: 11,
        aurainterruptflags: 0x0010_0000,
        effect1: 6,
        effectapplyauraname1: 15,
        ..Default::default()
    };
    let kept = SpellTemplate {
        id: 12,
        aurainterruptflags: 0x8000,
        ..interrupted.clone()
    };
    let next = SpellTemplate {
        id: 13,
        ..interrupted.clone()
    };
    for t in [&interrupted, &kept, &next] {
        stores.insert_template(t.clone()).unwrap();
    }
    let mut target = Target::new(false, false);
    let first = target.holder(&interrupted, 1);
    let keep = target.holder(&kept, 1);
    let second = target.holder(&next, 1);
    let live = target.self_aura_application().unwrap();
    let mut removal = SelfAuraRemoval {
        unit: live.unit,
        visibility: live.visibility,
        damage: live.damage,
    };
    live.auras
        .handle_stealth(&stores, &mut removal, 0, 7, true, false)
        .unwrap();
    assert!(live.auras.aura_holder(first).is_none());
    assert!(live.auras.aura_holder(second).is_none());
    assert!(live.auras.aura_holder(keep).is_some());
    assert_eq!(removal.visibility.stealth_strength, [7, 0]);
    assert_eq!(removal.visibility.mode, SourceUnitVisibilityMode::On);
    assert_eq!(live.auras.vis_flags, 0);
    live.auras
        .handle_stealth(&stores, &mut removal, 0, 7, false, false)
        .unwrap();
    assert_eq!(*removal.visibility, SourceUnitVisibility::default());
    assert_eq!(live.auras.vis_flags, 0);
}

#[test]
fn stealth_refusals_follow_strength_and_creep_writes() {
    use SourceUnitVisibilityMode::*;
    let mut stores = SourceSpellStores::default();
    let t = stealth(&mut stores, 6920, 0);
    let world_refresh = "execution.spells.Unit_8334.UpdateVisibilityAndView_owner";
    for (player, world, applied, removed, before) in [
        (
            true,
            false,
            "execution.spells.Aura_2585.SetByteFlag_owner",
            "execution.spells.Aura_2606.RemoveByteFlag_owner",
            On,
        ),
        (false, true, world_refresh, world_refresh, GroupNoDetect),
    ] {
        let mut target = Target::new(player, world);
        let holder = target.holder(&t, 11);
        assert_eq!(
            apply_self_aura_holder(&mut target, &stores, &t, holder)
                .unwrap_err()
                .id,
            applied
        );
        assert_eq!(target.unit.visibility.stealth_strength, [11, 0]);
        assert_eq!(target.unit.visibility.mode, before);
        assert_eq!(target.unit.auras.vis_flags, 2);
        assert!(target
            .objects
            .get(target.object)
            .unwrap()
            .field_changed(UNIT_FIELD_BYTES_1)
            .unwrap());
        assert_eq!(
            target
                .objects
                .client_update_pending(target.map, target.object),
            world
        );
        assert_eq!(target.remove(&stores, holder).unwrap_err().id, removed);
        assert_eq!(target.unit.visibility.stealth_strength, [0, 0]);
        assert_eq!(target.unit.visibility.mode, On);
        assert_eq!(target.unit.auras.vis_flags, 0);
    }
}

#[test]
fn stealth_refuses_invalid_indices_and_minimum_removal_before_state_write() {
    let mut stores = SourceSpellStores::default();
    for (id, index) in [(9001, -1), (9002, 2)] {
        let t = stealth(&mut stores, id, index);
        let mut target = Target::new(false, false);
        let holder = target.holder(&t, 1);
        let refusal = "execution.spells.ObjectVisibility_79.stealth_index_undefined";
        assert_eq!(
            apply_self_aura_holder(&mut target, &stores, &t, holder)
                .unwrap_err()
                .id,
            refusal
        );
        assert_eq!(target.remove(&stores, holder).unwrap_err().id, refusal);
        assert_eq!(target.unit.visibility, SourceUnitVisibility::default());
        assert_eq!(target.unit.auras.vis_flags, 0);
    }
    for (id, amount) in [(9003, i32::MIN), (9004, i32::MIN + 1)] {
        let t = stealth(&mut stores, id, 0);
        let mut target = Target::new(false, false);
        let holder = target.holder(&t, amount);
        apply_self_aura_holder(&mut target, &stores, &t, holder).unwrap();
        assert_eq!(target.unit.visibility.stealth_strength, [amount as u32, 0]);
        let removal = target.remove(&stores, holder);
        if amount == i32::MIN {
            assert_eq!(
                removal.unwrap_err().id,
                "execution.spells.Aura_2572.stealth_amount_negation_undefined"
            );
            assert_eq!(target.unit.visibility.stealth_strength, [1 << 31, 0]);
            assert_eq!(
                target.unit.visibility.mode,
                SourceUnitVisibilityMode::GroupStealth
            );
            assert_eq!(target.unit.auras.vis_flags, 2);
        } else {
            removal.unwrap();
            assert_eq!(target.unit.visibility, SourceUnitVisibility::default());
            assert_eq!(target.unit.auras.vis_flags, 0);
        }
    }
}

#[test]
fn shapeshift_writes_form_then_removes_holders_or_names_their_boundary() {
    let mut stores = SourceSpellStores::default();
    stores.bind_test_dbc(
        "SpellShapeshiftForm.dbc",
        [(
            17,
            SpellDbcRecord {
                ordinal: 0,
                words: Default::default(),
            },
        )],
    );
    let t = SpellTemplate {
        id: 7165,
        effect1: 6,
        effectapplyauraname1: 36,
        effectmiscvalue1: 17,
        ..Default::default()
    };
    let cancel = SpellTemplate {
        id: 9000,
        aurainterruptflags: 0x8000,
        effect1: 6,
        effectapplyauraname1: 95,
        effect2: 6,
        effectapplyauraname2: 42,
        ..Default::default()
    };
    let surviving = SpellTemplate {
        id: 9001,
        aurainterruptflags: 0x0010_0000,
        ..cancel.clone()
    };
    let unported = SpellTemplate {
        id: 9002,
        effectapplyauraname1: 33, // SpellAuras.cpp:94.
        ..cancel.clone()
    };
    for spell in [&t, &cancel, &surviving, &unported] {
        stores.insert_template(spell.clone()).unwrap();
    }
    for refuses in [false, true] {
        let mut target = Target::new(false, false);
        target.unit.power_type = Some(1);
        target.unit.power_fields = Some(SourceUnitPowerFields {
            level: 19,
            create_health: 100,
            create_mana: 0,
            auxiliary_current: Some([0, 0, 0]),
            auxiliary_maximum: None,
        });
        let stance = target.holder(&t, 1);
        let ignored = target.holder(&surviving, 1);
        let canceled = target.holder(&cancel, 1);
        let canceled_holder = target.unit.auras.aura_holder_mut(canceled).unwrap();
        for aura in canceled_holder.effects.iter_mut().flatten() {
            aura.applied = true;
        }
        let later = refuses.then(|| target.holder(&unported, 1));
        let result = target
            .self_aura_application()
            .unwrap()
            .apply_shape(&stores, stance, 17);
        assert_eq!(target.unit.shapeshift_form, 17);
        assert!(target.unit.auras.aura_holder(ignored).is_some());
        assert!(target.unit.auras.aura_holder(canceled).is_none());
        assert_eq!(
            target.unit.auras.deferred_aura_holders().next().unwrap().0,
            canceled
        );
        assert_eq!(
            target.unit.auras.deferred_auras()[..2]
                .iter()
                .map(|aura| (aura.kind, aura.applied))
                .collect::<Vec<_>>(),
            [(95, false), (42, false)]
        );
        if let Some(later) = later {
            assert_eq!(
                result.unwrap_err().id,
                "execution.spells.AuraHandler.type_33_unapply_owner"
            );
            assert!(target.unit.auras.aura_holder(later).is_none());
            assert!(
                !target
                    .unit
                    .auras
                    .deferred_aura_holders()
                    .nth(1)
                    .unwrap()
                    .1
                    .deleted
            );
        } else {
            assert_eq!(result.unwrap(), Some(21156));
        }
    }
}

#[test]
fn damage_taken_aura_has_no_immediate_apply_or_remove_effect() {
    let t = SpellTemplate {
        id: 7376,
        effect1: 6,
        effectapplyauraname1: 87,
        effectmiscvalue1: 127,
        ..Default::default()
    };
    let mut stores = SourceSpellStores::default();
    stores.insert_template(t.clone()).unwrap();
    let mut target = Target::new(false, true);
    let before = serde_json::to_vec(&target.unit).unwrap();
    let holder = target.holder(&t, -10);
    apply_self_aura_holder(&mut target, &stores, &t, holder).unwrap();
    assert!(target.unit.auras.has_aura_type(87));
    assert!(
        target.unit.auras.aura_holder(holder).unwrap().effects[0]
            .as_ref()
            .unwrap()
            .applied
    );
    target.remove(&stores, holder).unwrap();
    assert!(!target.unit.auras.has_aura_type(87));
    assert_eq!(serde_json::to_vec(&target.unit).unwrap(), before);
    assert!(!target
        .objects
        .client_update_pending(target.map, target.object));
}
