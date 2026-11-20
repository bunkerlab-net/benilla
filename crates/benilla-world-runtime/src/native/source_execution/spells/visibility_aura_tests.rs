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
            },
        )
    }
}

impl SpellExecutionOwner for Target {}
impl SelfAuraApplicationOwner for Target {
    fn self_aura_application(&mut self) -> SelfAuraApplication<'_> {
        SelfAuraApplication {
            unit: operands(
                self.object,
                &self.objects,
                &self.unit.control_guids,
                self.player,
            ),
            auras: &mut self.unit.auras,
            visibility: &mut self.unit.visibility,
            resistances: &mut self.unit.resistances,
            can_modify_stats: false,
            threat_modifier: &mut self.unit.threat_modifier,
            power_fields: &mut self.unit.power_fields,
            power_type: self.unit.power_type.as_mut(),
            vitals: &mut self.unit.vitals,
            form: &mut self.unit.shapeshift_form,
        }
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
    apply_self_aura_holder(&mut target, &stores, &unseen, main, None).unwrap();
    assert_eq!(target.unit.visibility.values[0], 200);
    assert_eq!(target.unit.visibility.mask, 0x0000_0001);
    assert_eq!(
        target.unit.visibility.mode,
        SourceUnitVisibilityMode::GroupInvisibility
    );
    let extra = target.holder(&other, 45);
    apply_self_aura_holder(&mut target, &stores, &other, extra, None).unwrap();
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
        apply_self_aura_holder(&mut target, &stores, &t, holder, None).unwrap();
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
    apply_self_aura_holder(&mut target, &stores, &t, first, None).unwrap();
    assert!(target.unit.auras.aura_holder(removed).is_none());
    let second = target.holder(&another, 20);
    apply_self_aura_holder(&mut target, &stores, &another, second, None).unwrap();
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
        apply_self_aura_holder(&mut target, &stores, &t, holder, None)
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
        apply_self_aura_holder(&mut target, &stores, &t, holder, None)
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
            apply_self_aura_holder(&mut target, &stores, &t, holder, None)
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
    apply_self_aura_holder(&mut target, &stores, &a, max, None).unwrap();
    let overflow = target.holder(&b, 1);
    let before = target.unit.visibility.clone();
    assert_eq!(
        apply_self_aura_holder(&mut target, &stores, &b, overflow, None)
            .unwrap_err()
            .id,
        "execution.spells.ObjectVisibility_71.invisibility_addition_undefined"
    );
    assert_eq!(target.unit.visibility, before);
    let mut target = Target::new(false, false);
    let min = target.holder(&a, i32::MIN);
    apply_self_aura_holder(&mut target, &stores, &a, min, None).unwrap();
    assert_eq!(target.unit.visibility.values[0], i32::MIN);
    assert_eq!(target.unit.visibility.mask, 0);
    assert_eq!(
        target.remove(&stores, min).unwrap_err().id,
        "execution.spells.Aura_2635.invisibility_amount_negation_undefined"
    );
    assert_eq!(target.unit.visibility.values[0], i32::MIN);
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
        effectapplyauraname1: 16,
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
                "execution.spells.AuraHandler.type_16_unapply_owner"
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
    apply_self_aura_holder(&mut target, &stores, &t, holder, None).unwrap();
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
