// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

fn subject(in_world: bool) -> (SourceObjects, ObjectHandle, MapIdentity) {
    let map = MapIdentity {
        map: 0,
        instance: 0,
        creation_generation: 1,
    };
    let cells = SourceCells::new(map).unwrap();
    let mut objects = SourceObjects::default();
    let unit = objects.construct(6, SourceObjectKind::Creature).unwrap();
    objects
        .assign_guid(
            unit,
            41,
            SourceGuid::with_entry(HighGuid::Creature, 901, 41),
        )
        .unwrap();
    objects.bind_map(unit, map, &cells).unwrap();
    if in_world {
        objects.add_to_world(unit).unwrap();
    }
    (objects, unit, map)
}

#[test]
fn client_update_aura_field_words_compare_packed_values() {
    const SLOT: usize = 47;
    for (in_world, changed) in [(true, false), (true, true), (false, true)] {
        let (objects, target, map) = subject(in_world);
        let controls = SourceControlGuids::default();
        let operands = SelfAuraTarget {
            target,
            level: Some(22),
            alive: true,
            player: false,
            player_group: None,
            totem: false,
            objects: &objects,
            control_guids: &controls,
        };
        let mut fields = SourceAuraFields::default();
        fields.spells[SLOT] = 10848 * u32::from(!changed);
        fields.flags[5] = 0x0054_3210 | ((8 * u32::from(!changed)) << 28);
        fields.levels[11] = 0x0034_1256 | ((22 * u32::from(!changed)) << 24);
        fields.applications[11] = 0x0078_5634 | ((5 * u32::from(!changed)) << 24);
        fields.set_aura_spell(operands, SLOT, 10848).unwrap();
        fields.set_aura_flags(operands, SLOT, 8).unwrap();
        fields.set_aura_level(operands, SLOT, 22).unwrap();
        fields
            .update_aura_application(&objects, target, SLOT as u8, 3, 2)
            .unwrap();
        assert_eq!(
            (
                fields.spells[SLOT],
                fields.flags[5],
                fields.levels[11],
                fields.applications[11]
            ),
            (10848, 0x8054_3210, 0x1634_1256, 0x0578_5634)
        );
        let object = objects.get(target).unwrap();
        for word in [
            UNIT_FIELD_AURA + 47,
            UNIT_FIELD_AURAFLAGS + 5,
            UNIT_FIELD_AURALEVELS + 11,
            UNIT_FIELD_AURAAPPLICATIONS + 11,
        ] {
            assert_eq!(object.field_changed(word).unwrap(), changed);
            assert!(!object.field_changed(word - 1).unwrap());
        }
        assert_eq!(
            objects.client_update_pending(map, target),
            in_world && changed
        );
    }
}

#[test]
fn client_update_ghost_uses_the_visibility_byte_change_test() {
    for (apply, expected) in [(true, 3), (false, 2)] {
        for (in_world, changed) in [(true, false), (true, true), (false, true)] {
            let (objects, target, map) = subject(in_world);
            let mut auras = SourceUnitAuras::default();
            let vis_flags = &mut auras.vis_flags;
            *vis_flags = expected ^ u8::from(changed);
            auras
                .handle_aura_ghost(&objects, target, apply, false)
                .unwrap();
            assert_eq!(auras.vis_flags, expected);
            assert_eq!(
                objects
                    .get(target)
                    .unwrap()
                    .field_changed(UNIT_FIELD_BYTES_1)
                    .unwrap(),
                changed
            );
            assert_eq!(
                objects.client_update_pending(map, target),
                in_world && changed
            );
        }
    }
}

#[test]
fn client_update_player_ghost_marks_unit_byte_before_player_flag_refusal() {
    let mut objects = SourceObjects::default();
    let player = objects.construct(6, SourceObjectKind::Player).unwrap();
    objects
        .assign_guid(player, 41, SourceGuid::without_entry(HighGuid::Player, 41))
        .unwrap();
    let mut auras = SourceUnitAuras::default();
    for (apply, flags, refusal) in [
        (
            true,
            1,
            "execution.spells.Aura_4411.player_ghost_flag_owner",
        ),
        (
            false,
            0,
            "execution.spells.Aura_4421.player_ghost_flag_owner",
        ),
    ] {
        assert_eq!(
            auras
                .handle_aura_ghost(&objects, player, apply, true)
                .unwrap_err()
                .id,
            refusal
        );
        assert_eq!(auras.vis_flags, flags);
        assert!(objects
            .get(player)
            .unwrap()
            .field_changed(UNIT_FIELD_BYTES_1)
            .unwrap());
    }
}

#[test]
fn client_update_stance_writes_the_rage_form_and_power_type_words() {
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
        id: 2457,
        ..Default::default()
    };
    stores.insert_template(t.clone()).unwrap();
    for (in_world, changed) in [(true, false), (true, true), (false, true)] {
        let (objects, object, map) = subject(in_world);
        let mut unit = SourceUnitFields::constructed_creature();
        unit.power_type = Some(if changed { 0 } else { 1 });
        unit.shapeshift_form = if changed { 0 } else { 17 };
        unit.power_fields = Some(SourceUnitPowerFields {
            level: 19,
            create_health: 100,
            create_mana: 5,
            auxiliary_current: Some([u32::from(changed) * 30, 0, 0]),
            auxiliary_maximum: None,
        });
        let holder = unit
            .auras
            .store_aura_holder(
                t.id,
                &mut Some(SourceAuraHolder::new(&t, object, 0, 0, -1).unwrap()),
            )
            .unwrap();
        let mut live = SelfAuraApplication {
            unit: SelfAuraTarget {
                target: object,
                level: Some(19),
                alive: true,
                player: false,
                player_group: None,
                totem: unit.totem,
                objects: &objects,
                control_guids: &unit.control_guids,
            },
            auras: &mut unit.auras,
            visibility: &mut unit.visibility,
            resistances: &mut unit.resistances,
            damage: DamageAuraTarget::new(
                &mut unit.creature_damage,
                &unit.loaded_creature,
                unit.flags,
                unit.shapeshift_form,
            ),
            can_modify_stats: true,
            threat_modifier: &mut unit.threat_modifier,
            power_fields: &mut unit.power_fields,
            power_type: unit.power_type.as_mut(),
            vitals: &mut unit.vitals,
            form: &mut unit.shapeshift_form,
        };
        assert_eq!(live.apply_shape(&stores, holder, 17).unwrap(), Some(21156));
        assert_eq!(unit.power_type, Some(1));
        assert_eq!(unit.shapeshift_form, 17);
        assert_eq!(
            unit.power_fields.unwrap().auxiliary_current,
            Some([0, 0, 0])
        );
        for word in [UNIT_FIELD_BYTES_0, UNIT_FIELD_BYTES_1, UNIT_FIELD_POWER2] {
            assert_eq!(
                objects.get(object).unwrap().field_changed(word).unwrap(),
                changed
            );
        }
        assert_eq!(
            objects.client_update_pending(map, object),
            in_world && changed
        );
    }
}
