// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::collections::BTreeMap;
fn addon_unit() -> SourceUnitFields {
    SourceUnitFields::creature(&CreatureAiTemplateOperands {
        ordinal: 1,
        entry: 1,
        faction: 14,
        extra_flags: 0,
        unit_flags: 0,
        type_flags: 0,
        static_flags: [0; 4],
        rank: 0,
        inhabit_type: 1,
        stats_multipliers: [1f32.to_bits(); 3],
        civilian: false,
        pursuit_ms: 15000,
        call_for_help: 0,
        creature_type: 0,
        ai_name: String::new(),
        script_name: String::new(),
    })
}
fn row(key: u32, text: &[u8]) -> CreatureAddonRow {
    SqlTemplateRow {
        ordinal: u64::from(key),
        words: [key, 0, 0, 0, 0, 0, 0],
        strings: [text.to_vec()],
    }
}
#[derive(Default)]
struct Owner {
    rows: [Vec<CreatureAddonRow>; 2],
    spells: BTreeMap<u32, SpellTemplate>,
    times: BTreeMap<u32, i32>,
    filtered: bool,
    calls: Vec<String>,
}
impl CreatureAddonLoaderOwner for Owner {
    fn rows(&mut self, table: CreatureAddonTable) -> Result<Vec<CreatureAddonRow>> {
        self.calls.push(table.name().into());
        Ok(std::mem::take(&mut self.rows[table as usize]))
    }
    fn display(&mut self, id: u32) -> Result<bool> {
        self.calls.push(format!("display:{id}"));
        Ok(id == 1)
    }
    fn emote(&mut self, id: u32) -> Result<bool> {
        self.calls.push(format!("emote:{id}"));
        Ok(id == 0)
    }
    fn spell(&mut self, id: u32) -> Result<Option<&SpellTemplate>> {
        self.calls.push(format!("spell:{id}"));
        Ok(self.spells.get(&id))
    }
    fn cast_time(&mut self, id: u32) -> Result<Option<i32>> {
        self.calls.push(format!("time:{id}"));
        Ok(self.times.get(&id).copied())
    }
    fn strict_check_filtered(&mut self) -> Result<bool> {
        self.calls.push("filter".into());
        Ok(self.filtered)
    }
    fn creature_template(&mut self, id: u32) -> Result<bool> {
        self.calls.push(format!("entry:{id}"));
        Ok(id == 1)
    }
    fn creature_guid(&mut self, id: u32) -> Result<bool> {
        self.calls.push(format!("guid:{id}"));
        Ok(id == 7)
    }
}
fn spell(id: u32, effect: u32) -> SpellTemplate {
    SpellTemplate {
        id,
        effect1: effect,
        effectimplicittargeta1: 1,
        ..SpellTemplate::default()
    }
}
#[test]
fn addon_loader_keeps_log_only_rows_mutates_mount_emote_and_compacts_auras() {
    let mut template = row(1, b"99 1 2 3 4 5 5 6");
    template.words[1] = 9;
    template.words[3] = 255;
    template.words[4] = 9;
    let mut owner = Owner {
        rows: [vec![template, row(2, b"")], vec![row(7, b"5")]],
        ..Owner::default()
    };
    owner.spells = BTreeMap::from([
        (1, spell(1, 2)),
        (2, spell(2, 6)),
        (3, spell(3, 6)),
        (4, spell(4, 6)),
        (5, spell(5, 3)),
        (6, spell(6, 77)),
    ]);
    owner.spells.get_mut(&2).unwrap().castingtimeindex = 1;
    owner.spells.get_mut(&3).unwrap().effectimplicittargeta1 = 6;
    owner.spells.get_mut(&4).unwrap().effectimplicittargetb1 = 6;
    owner.times.insert(1, 1);
    let loaded = LoadedCreatureAddons::load(&mut owner).unwrap();
    let addon = loaded.get(HighGuid::Creature, 8, 1).unwrap();
    assert_eq!(addon.mount, 0);
    assert_eq!(addon.emote, 0);
    assert_eq!(addon.sheath_state, 255);
    assert_eq!(addon.auras, CreatureAddonAuras::Spells(vec![5, 6, 0]));
    assert_eq!(
        loaded.notices.iter().map(|n| n.line).collect::<Vec<_>>(),
        [680, 686, 690, 625, 632, 640, 648, 648, 654, 708]
    );
    assert_eq!(loaded.record_counts, [2, 1]);
    let entry_check = owner.calls.iter().position(|c| c == "entry:2").unwrap();
    let guid_load = owner
        .calls
        .iter()
        .position(|c| c == "creature_addon")
        .unwrap();
    assert!(entry_check < guid_load);
    assert_eq!(loaded.get(HighGuid::Creature, 7, 1).unwrap().key, 7);
    assert_eq!(loaded.get(HighGuid::Pet, 7, 1).unwrap().key, 1);
    assert!(loaded.get(HighGuid::Pet, 7, 99).is_none());
}
#[test]
fn addon_source_tokenizer_preserves_tabs_leading_spaces_and_zero_key_bypass() {
    assert_eq!(aura_values(b" 1  2\t3 4 ").unwrap(), [1, 2, 4]);
    assert_eq!(aura_values(b" ").unwrap(), [0]);
    assert_eq!(aura_values(b"1\t2").unwrap(), [1]);
    for whitespace in *b"\t\n\x0b\x0c\r " {
        assert_eq!(aura_values(&[whitespace, b'1']).unwrap(), [1]);
    }
    assert!(aura_values(b"").unwrap().is_empty());
    let mut owner = Owner {
        rows: [vec![row(0, b"2147483648"), row(1, b"")], vec![]],
        ..Owner::default()
    };
    let loaded = LoadedCreatureAddons::load(&mut owner).unwrap();
    assert_eq!(
        loaded.get(HighGuid::Pet, 9, 0).unwrap().auras,
        CreatureAddonAuras::Raw(b"2147483648".to_vec())
    );
    assert_eq!(
        loaded.get(HighGuid::Pet, 9, 1).unwrap().auras,
        CreatureAddonAuras::Empty
    );
    assert!(!owner.calls.iter().any(|c| c == "entry:0"));
}
#[test]
fn addon_atoi_and_effect_enum_refuse_both_undefined_boundaries_only_when_reached() {
    assert_eq!(
        aura_values(b"2147483647 -2147483648").unwrap(),
        [i32::MAX as u32, i32::MIN as u32]
    );
    for text in [b"2147483648".as_slice(), b"-2147483649"] {
        assert_eq!(
            aura_values(text).unwrap_err().id,
            "execution.creature_addons.ObjectMgr_595_600.atoi_int_undefined"
        );
    }
    assert!(!has_effect([255, 0, 0], 3).unwrap());
    assert_eq!(
        has_effect([256, 0, 0], 3).unwrap_err().id,
        "execution.creature_addons.SpellMgr_72.SpellEffects_enum_undefined"
    );
    assert!(has_effect([3, 256, 0], 3).unwrap());
    let mut owner = Owner {
        rows: [vec![row(1, b"1")], vec![]],
        spells: BTreeMap::from([(1, spell(1, 6))]),
        ..Owner::default()
    };
    owner.spells.get_mut(&1).unwrap().effect2 = 256;
    LoadedCreatureAddons::load(&mut owner).unwrap();
    let mut owner = Owner {
        rows: [vec![row(u32::MAX - 1, b"")], vec![]],
        ..Owner::default()
    };
    LoadedCreatureAddons::load(&mut owner).unwrap();
    owner.rows[0] = vec![row(u32::MAX, b"")];
    assert_eq!(
        LoadedCreatureAddons::load(&mut owner).unwrap_err().id,
        "execution.creature_addons.SQLStorageImpl_317.max_entry_index_undefined"
    );
}
#[test]
fn addon_self_targeting_keeps_early_empty_effect_and_filter_short_circuit() {
    assert!(only_self([0, 6, 0], [0, 6, 0], [0; 3]));
    assert!(only_self([6, 6, 6], [1; 3], [0, 1, 0]));
    assert!(!only_self([6, 0, 0], [6, 0, 0], [0; 3]));
    assert!(!only_self([6, 0, 0], [1, 0, 0], [6, 0, 0]));
    let mut s = spell(1, 6);
    s.effectimplicittargeta1 = 6;
    let mut owner = Owner {
        rows: [vec![row(1, b"1")], vec![]],
        filtered: true,
        spells: BTreeMap::from([(1, s)]),
        ..Owner::default()
    };
    assert_eq!(
        LoadedCreatureAddons::load(&mut owner)
            .unwrap()
            .get(HighGuid::Pet, 0, 1)
            .unwrap()
            .auras,
        CreatureAddonAuras::Spells(vec![1, 0])
    );
    assert_eq!(
        owner
            .calls
            .iter()
            .filter(|c| c.as_str() == "filter")
            .count(),
        1
    );
}
#[test]
fn addon_missing_owners_refuse_at_source_calls_not_before_earlier_checks() {
    struct Missing {
        calls: Vec<&'static str>,
    }
    impl CreatureAddonLoaderOwner for Missing {
        fn rows(&mut self, _: CreatureAddonTable) -> Result<Vec<CreatureAddonRow>> {
            self.calls.push("rows");
            Ok(vec![row(1, b"1")])
        }
        fn emote(&mut self, _: u32) -> Result<bool> {
            self.calls.push("emote");
            Ok(false)
        }
    }
    let mut owner = Missing { calls: vec![] };
    assert_eq!(
        LoadedCreatureAddons::load(&mut owner).unwrap_err().id,
        "execution.creature_addons.ObjectMgr_622.spell_template_owner"
    );
    assert_eq!(owner.calls, ["rows", "emote"]);
    struct Empty;
    impl CreatureAddonLoaderOwner for Empty {}
    assert_eq!(
        LoadedCreatureAddons::load(&mut Empty).unwrap_err().id,
        "execution.creature_addons.ObjectMgr_667.creature_template_addon_owner"
    );
    assert_eq!(
        get_creature_addon(None, HighGuid::Creature, 7, 1)
            .unwrap_err()
            .id,
        "execution.creature.loaded_creature_addon_store_owner"
    );
}
#[test]
fn addon_aura_predicates_accept_each_defined_aura_dummy_script_and_trigger_kind() {
    for effect in [3, 6, 27, 35, 64, 77, 119] {
        let mut owner = Owner {
            rows: [vec![row(1, b"1")], vec![]],
            spells: BTreeMap::from([(1, spell(1, effect))]),
            ..Owner::default()
        };
        assert_eq!(
            LoadedCreatureAddons::load(&mut owner)
                .unwrap()
                .get(HighGuid::Pet, 0, 1)
                .unwrap()
                .auras,
            CreatureAddonAuras::Spells(vec![1, 0]),
            "effect {effect}"
        );
    }
}

#[test]
fn addon_missing_prerequisites_name_each_exact_source_site() {
    struct Missing;
    impl CreatureAddonLoaderOwner for Missing {}
    let mut owner = Missing;
    let results = [
        owner.display(1).unwrap_err().id,
        owner.emote(0).unwrap_err().id,
        owner.spell(1).unwrap_err().id,
        owner.cast_time(0).unwrap_err().id,
        owner.strict_check_filtered().unwrap_err().id,
        owner.creature_template(1).unwrap_err().id,
        owner.creature_guid(1).unwrap_err().id,
        owner.rows(CreatureAddonTable::Guid).unwrap_err().id,
    ];
    assert_eq!(
        results,
        [
            "execution.creature_addons.ObjectMgr_678.creature_display_info_owner",
            "execution.creature_addons.ObjectMgr_688.emotes_owner",
            "execution.creature_addons.ObjectMgr_622.spell_template_owner",
            "execution.creature_addons.ObjectMgr_636.spell_cast_times_owner",
            "execution.creature_addons.ObjectMgr_646.db_strict_log_filter_owner",
            "execution.creature_addons.ObjectMgr_707.loaded_creature_templates_owner",
            "execution.creature_addons.ObjectMgr_715.loaded_creature_data_owner",
            "execution.creature_addons.ObjectMgr_667.creature_addon_owner",
        ]
    );
}

#[test]
fn addon_loader_retains_missing_guid_and_all_dropped_auras_with_terminator() {
    let mut mounted = row(1, b"1 2");
    mounted.words[1] = 1;
    mounted.words[2] = 255;
    mounted.words[3] = 2;
    mounted.words[5] = u32::MAX;
    let mut owner = Owner {
        rows: [vec![mounted], vec![row(99, b"123")]],
        spells: BTreeMap::from([(1, spell(1, 6)), (2, spell(2, 6))]),
        times: BTreeMap::from([(0, -1)]),
        ..Owner::default()
    };
    let loaded = LoadedCreatureAddons::load(&mut owner).unwrap();
    let template = loaded.get(HighGuid::Pet, 99, 1).unwrap();
    assert_eq!(
        (
            template.mount,
            template.stand_state,
            template.sheath_state,
            template.move_flags
        ),
        (1, 255, 2, u32::MAX)
    );
    assert_eq!(template.auras, CreatureAddonAuras::Spells(vec![1, 2, 0]));
    assert_eq!(
        loaded.get(HighGuid::Creature, 99, 1).unwrap().auras,
        CreatureAddonAuras::Spells(vec![0])
    );
    assert_eq!(
        loaded.notices.iter().map(|n| n.line).collect::<Vec<_>>(),
        [625, 716]
    );
}

struct NoSpellExecution;
impl CreatureAddonExecutionOwner for NoSpellExecution {}

fn addon_subject(in_world: bool) -> (SourceObjects, ObjectHandle, MapIdentity) {
    let map = MapIdentity {
        map: 0,
        instance: 0,
        creation_generation: 1,
    };
    let cells = SourceCells::new(map).unwrap();
    let mut objects = SourceObjects::default();
    let object = objects.construct(1, SourceObjectKind::Creature).unwrap();
    objects
        .assign_guid(object, 7, SourceGuid::with_entry(HighGuid::Creature, 1, 7))
        .unwrap();
    objects.bind_map(object, map, &cells).unwrap();
    if in_world {
        objects.add_to_world(object).unwrap();
    }
    (objects, object, map)
}

#[test]
fn client_update_creature_addon_marks_the_changed_byte_words_and_emote() {
    let addon = LoadedCreatureAddon {
        key: 1,
        mount: 0,
        stand_state: 3,
        sheath_state: 2,
        emote: 7,
        move_flags: 0,
        auras: CreatureAddonAuras::Empty,
    };
    for (in_world, changed) in [(true, true), (true, false), (false, true)] {
        let (objects, object, map) = addon_subject(in_world);
        let mut unit = addon_unit();
        if !changed {
            unit.stand_state = 3;
            unit.sheath_state = 2;
            unit.npc_emote_state = 7;
        }
        CreatureAddonState::default()
            .load(
                Some(&addon),
                true,
                &objects,
                object,
                &mut unit,
                &mut NoSpellExecution,
            )
            .unwrap();
        for field in [UNIT_FIELD_BYTES_1, UNIT_FIELD_BYTES_2, UNIT_NPC_EMOTESTATE] {
            assert_eq!(
                objects.get(object).unwrap().field_changed(field).unwrap(),
                changed
            );
        }
        assert_eq!(
            objects.client_update_pending(map, object),
            in_world && changed
        );
    }
}

#[test]
fn addon_application_guid_template_and_absent_preserve_conditional_writes() {
    let mut template = row(1, b"");
    template.words[2] = 3;
    template.words[3] = 2;
    let mut guid = row(7, b"");
    guid.words[3] = 0;
    let loaded = LoadedCreatureAddons::load(&mut Owner {
        rows: [vec![template], vec![guid]],
        ..Owner::default()
    })
    .unwrap();
    let mut state = CreatureAddonState::default();
    let (objects, object, _) = addon_subject(false);
    let mut unit = addon_unit();
    unit.stand_state = 8;
    unit.sheath_state = 1;
    unit.npc_emote_state = 9;
    assert!(state
        .load(
            loaded.get(HighGuid::Creature, 7, 1),
            false,
            &objects,
            object,
            &mut unit,
            &mut NoSpellExecution
        )
        .unwrap());
    assert_eq!(
        (unit.stand_state, unit.sheath_state, unit.npc_emote_state),
        (8, 0, 9)
    );
    assert!(state
        .load(
            loaded.get(HighGuid::Creature, 8, 1),
            false,
            &objects,
            object,
            &mut unit,
            &mut NoSpellExecution
        )
        .unwrap());
    assert_eq!(
        (unit.stand_state, unit.sheath_state, unit.npc_emote_state),
        (3, 2, 9)
    );
    assert!(!state
        .load(
            loaded.get(HighGuid::Creature, 8, 2),
            false,
            &objects,
            object,
            &mut unit,
            &mut NoSpellExecution
        )
        .unwrap());
    assert_eq!(
        (unit.stand_state, unit.sheath_state, unit.npc_emote_state),
        (3, 2, 9)
    );
    assert!(state
        .load(
            loaded.get(HighGuid::Pet, 7, 1),
            false,
            &objects,
            object,
            &mut unit,
            &mut NoSpellExecution
        )
        .unwrap());
    assert_eq!(unit.sheath_state, 2);
}

#[test]
fn addon_application_aura_membership_reload_and_cast_refusal_keep_source_order() {
    let (objects, object, _) = addon_subject(false);
    let holder = || {
        SourceAuraHolder::new(
            &SpellTemplate {
                id: 10,
                ..Default::default()
            },
            ObjectHandle {
                thread: 1,
                construction: 1,
            },
            0,
            0,
            -1,
        )
        .unwrap()
    };
    let mut addon = LoadedCreatureAddon {
        key: 1,
        mount: 0,
        stand_state: 3,
        sheath_state: 2,
        emote: 7,
        move_flags: u32::MAX,
        auras: CreatureAddonAuras::Spells(vec![10, 11, 0]),
    };
    for reload in [false, true] {
        let mut state = CreatureAddonState::default();
        let mut unit = addon_unit();
        unit.auras
            .store_aura_holder(10, &mut Some(holder()))
            .unwrap();
        assert_eq!(
            state
                .load(
                    Some(&addon),
                    reload,
                    &objects,
                    object,
                    &mut unit,
                    &mut NoSpellExecution
                )
                .unwrap_err()
                .id,
            "execution.creature_addon.Creature_2343.CastSpell_owner"
        );
        assert_eq!(
            (unit.stand_state, unit.sheath_state, unit.npc_emote_state),
            (3, 2, 7)
        );
        assert_eq!(
            state.duplicate_aura_errors,
            if reload { vec![] } else { vec![10] }
        );
        assert_eq!(unit.auras.aura_spell_ids().collect::<Vec<_>>(), [10]);
    }
    addon.auras = CreatureAddonAuras::Spells(vec![10, 0, 11]);
    let mut state = CreatureAddonState::default();
    let mut unit = addon_unit();
    unit.auras
        .store_aura_holder(10, &mut Some(holder()))
        .unwrap();
    assert!(state
        .load(
            Some(&addon),
            true,
            &objects,
            object,
            &mut unit,
            &mut NoSpellExecution
        )
        .unwrap());
    assert!(state.duplicate_aura_errors.is_empty());
}

#[test]
fn addon_application_mount_refuses_before_later_field_writes() {
    let mut addon = LoadedCreatureAddon {
        key: 1,
        mount: 1,
        stand_state: 3,
        sheath_state: 2,
        emote: 7,
        move_flags: 0,
        auras: CreatureAddonAuras::Empty,
    };
    let mut state = CreatureAddonState::default();
    let (objects, object, _) = addon_subject(false);
    let mut unit = addon_unit();
    assert_eq!(
        state
            .load(
                Some(&addon),
                false,
                &objects,
                object,
                &mut unit,
                &mut NoSpellExecution
            )
            .unwrap_err()
            .id,
        "execution.creature_addon.Creature_2313.Mount_owner"
    );
    assert_eq!(state, CreatureAddonState::default());
    assert_eq!(unit.mount_display(), 0);
    assert!(!objects
        .get(object)
        .unwrap()
        .field_changed(UNIT_FIELD_MOUNTDISPLAYID)
        .unwrap());
    assert_eq!(
        (unit.stand_state, unit.sheath_state, unit.npc_emote_state),
        (0, 0, 0)
    );
    addon.mount = 0;
    assert!(state
        .load(
            Some(&addon),
            false,
            &objects,
            object,
            &mut unit,
            &mut NoSpellExecution
        )
        .unwrap());
    assert_eq!(
        (unit.stand_state, unit.sheath_state, unit.npc_emote_state),
        (3, 2, 7)
    );
}

#[test]
fn addon_application_checks_both_sides_of_aura_terminator_boundary() {
    let mut addon = LoadedCreatureAddon {
        key: 1,
        mount: 0,
        stand_state: 3,
        sheath_state: 2,
        emote: 7,
        move_flags: 0,
        auras: CreatureAddonAuras::Spells(vec![]),
    };
    let mut state = CreatureAddonState::default();
    let (objects, object, _) = addon_subject(false);
    let mut unit = addon_unit();
    assert_eq!(
        state
            .load(
                Some(&addon),
                false,
                &objects,
                object,
                &mut unit,
                &mut NoSpellExecution
            )
            .unwrap_err()
            .id,
        "execution.creature_addon.Creature_2333.aura_terminator_out_of_bounds_undefined"
    );
    assert_eq!(
        (unit.stand_state, unit.sheath_state, unit.npc_emote_state),
        (3, 2, 7)
    );
    addon.auras = CreatureAddonAuras::Spells(vec![0]);
    assert!(state
        .load(
            Some(&addon),
            false,
            &objects,
            object,
            &mut unit,
            &mut NoSpellExecution
        )
        .unwrap());
}

#[test]
fn addon_operands_do_not_change_serialized_unit_bytes() {
    let mut unit = addon_unit();
    let before = serde_json::to_vec(&unit).unwrap();
    unit.stand_state = 3;
    unit.sheath_state = 2;
    unit.npc_emote_state = 7;
    assert_eq!(serde_json::to_vec(&unit).unwrap(), before);
}
