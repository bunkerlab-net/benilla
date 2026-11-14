// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored requirements at SpellMgr.cpp:2290-2338,2796-2854, not live stock casts.
use super::*;
use SpellCastResult as R;
fn player() -> ObjectHandle {
    ObjectHandle {
        thread: 1,
        construction: 1,
    }
}
fn area() -> SpellArea {
    SpellArea {
        spell: 7,
        area: 0,
        quest_start: 0,
        quest_start_active: false,
        quest_end: 0,
        condition: 0,
        aura: 0,
        race_mask: 0,
        gender: 2,
        autocast: false,
    }
}
fn template(id: u32) -> SpellTemplate {
    SpellTemplate {
        id,
        ..Default::default()
    }
}
fn operands() -> SpellUnitState {
    SpellUnitState {
        player: false,
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
    }
}
fn authored_spell(t: &SpellTemplate, caster: ObjectHandle, flags: u32) -> SourceSpell {
    struct Constructor;
    impl SpellExecutionOwner for Constructor {
        fn unit(&mut self, _: ObjectHandle) -> Result<SpellUnitState> {
            Ok(operands())
        }
        fn map_time(&mut self, _: ObjectHandle) -> Result<u32> {
            Ok(0)
        }
        fn targeting_data(&mut self, _: u32) -> Result<[[u32; 2]; 3]> {
            Ok([[0; 2]; 3])
        }
    }
    let mut stores = SourceSpellStores::default();
    stores.insert_template(t.clone()).unwrap();
    SourceSpell::construct(
        1,
        &stores,
        t.id,
        caster,
        None,
        SpellTriggerFlags(flags),
        &mut Constructor,
    )
    .unwrap()
}
fn fields() -> SourceUnitFields {
    SourceUnitFields::creature(&CreatureAiTemplateOperands {
        ordinal: 0,
        entry: 1,
        faction: 0,
        extra_flags: 0,
        unit_flags: 0,
        type_flags: 0,
        static_flags: [0; 4],
        rank: 0,
        inhabit_type: 1,
        stats_multipliers: [0; 3],
        civilian: false,
        pursuit_ms: 0,
        call_for_help: 0,
        creature_type: 0,
        ai_name: String::new(),
        script_name: String::new(),
    })
}
struct Missing;
impl SpellLocationOwner for Missing {}
impl SpellLoaderOwner for Missing {}
#[derive(Default)]
struct Live {
    yes: bool,
    map: Option<bool>,
    calls: Vec<&'static str>,
}
impl SpellLocationOwner for Live {
    fn condition(&mut self, _: ObjectHandle, _: u16) -> Result<bool> {
        self.calls.push("condition");
        Ok(self.yes)
    }
    fn gender(&mut self, _: ObjectHandle) -> Result<u8> {
        self.calls.push("gender");
        Ok(u8::from(self.yes))
    }
    fn race_mask(&mut self, _: ObjectHandle) -> Result<u32> {
        self.calls.push("race");
        Ok(u32::from(self.yes))
    }
    fn active_quest(&mut self, _: ObjectHandle, _: u32) -> Result<bool> {
        self.calls.push("active");
        Ok(self.yes)
    }
    fn rewarded_quest(&mut self, _: ObjectHandle, _: u32) -> Result<bool> {
        self.calls.push("rewarded");
        Ok(self.yes)
    }
    fn aura(&mut self, _: ObjectHandle, _: u32) -> Result<bool> {
        self.calls.push("aura");
        Ok(self.yes)
    }
    fn in_battleground(&mut self, _: ObjectHandle) -> Result<bool> {
        self.calls.push("in_bg");
        Ok(self.yes)
    }
    fn battleground(&mut self, _: ObjectHandle) -> Result<bool> {
        self.calls.push("bg");
        Ok(self.yes)
    }
    fn battleground_wait_join(&mut self, _: ObjectHandle) -> Result<bool> {
        self.calls.push("wait");
        Ok(self.yes)
    }
    fn map_battleground(&mut self, _: u32) -> Result<Option<bool>> {
        self.calls.push("map");
        Ok(self.map)
    }
}
fn stores(rows: Vec<SpellArea>) -> SourceSpellStores {
    SourceSpellStores {
        areas: BTreeMap::from([(7, rows)]),
        ..Default::default()
    }
}
#[test]
fn condition_replaces_legacy_checks_and_null_player_never_reads_live_state() {
    let row = SpellArea {
        condition: 1,
        gender: 1,
        race_mask: 1,
        quest_start: 1,
        quest_end: 1,
        ..area()
    };
    assert!(!row.fits_location(None, 0, 0, &mut Missing).unwrap());
    for yes in [false, true] {
        let mut live = Live {
            yes,
            ..Default::default()
        };
        assert_eq!(
            row.fits_location(Some(player()), 0, 0, &mut live).unwrap(),
            yes
        );
        assert_eq!(live.calls, ["condition"]);
    }
    assert_eq!(
        row.fits_location(Some(player()), 0, 0, &mut Missing)
            .unwrap_err()
            .id,
        "execution.spells.location.IsConditionSatisfied_owner"
    );
}
#[test]
fn gender_then_race_then_quest_before_area_and_aura() {
    let rows = [
        (
            SpellArea {
                gender: 1,
                ..area()
            },
            "Player_getGender",
        ),
        (
            SpellArea {
                race_mask: 1,
                ..area()
            },
            "Player_getRaceMask",
        ),
        (
            SpellArea {
                quest_start: 1,
                quest_start_active: true,
                ..area()
            },
            "Player_IsActiveQuest",
        ),
        (
            SpellArea {
                quest_start: 1,
                ..area()
            },
            "Player_GetQuestRewardStatus",
        ),
        (
            SpellArea {
                quest_end: 1,
                ..area()
            },
            "Player_GetQuestRewardStatus",
        ),
        (SpellArea { aura: 1, ..area() }, "Player_HasAura"),
    ];
    for (row, read) in rows {
        assert!(!row.fits_location(None, 0, 0, &mut Missing).unwrap());
        assert_eq!(
            row.fits_location(Some(player()), 0, 0, &mut Missing)
                .unwrap_err()
                .id,
            format!("execution.spells.location.{read}_owner")
        );
        for yes in [false, true] {
            let mut live = Live {
                yes,
                ..Default::default()
            };
            let expected = if row.quest_end != 0 { !yes } else { yes };
            assert_eq!(
                row.fits_location(Some(player()), 0, 0, &mut live).unwrap(),
                expected
            );
        }
    }
    let row = SpellArea {
        gender: 1,
        race_mask: 1,
        quest_start: 1,
        area: 99,
        aura: 1,
        ..area()
    };
    let mut live = Live::default();
    assert!(!row.fits_location(Some(player()), 0, 0, &mut live).unwrap());
    assert_eq!(live.calls, ["gender"]);
}
#[test]
fn active_start_short_circuits_reward_and_forbidden_end_has_opposite_polarity() {
    for active in [false, true] {
        for yes in [false, true] {
            let row = SpellArea {
                quest_start: 1,
                quest_start_active: active,
                ..area()
            };
            let mut live = Live {
                yes,
                ..Default::default()
            };
            assert_eq!(
                row.fits_location(Some(player()), 0, 0, &mut live).unwrap(),
                yes
            );
            assert_eq!(
                live.calls,
                match (active, yes) {
                    (false, _) => vec!["rewarded"],
                    (true, true) => vec!["active"],
                    (true, false) => vec!["active", "rewarded"],
                }
            );
        }
    }
}
#[test]
fn area_matches_zone_or_subzone_before_signed_aura_presence() {
    let row = SpellArea { area: 12, ..area() };
    for (zone, sub, expected) in [(12, 87, true), (0, 12, true), (0, 0, false)] {
        assert_eq!(
            row.fits_location(None, zone, sub, &mut Missing).unwrap(),
            expected
        );
    }
    for aura in [-1, 1] {
        let row = SpellArea {
            area: 12,
            aura,
            ..area()
        };
        assert!(!row
            .fits_location(Some(player()), 0, 0, &mut Missing)
            .unwrap());
        for yes in [false, true] {
            assert_eq!(
                row.fits_location(
                    Some(player()),
                    12,
                    0,
                    &mut Live {
                        yes,
                        ..Default::default()
                    }
                )
                .unwrap(),
                if aura > 0 { yes } else { !yes }
            );
        }
    }
    let defined = SpellArea {
        aura: i32::MIN + 1,
        ..area()
    };
    assert_eq!(
        defined
            .fits_location(Some(player()), 0, 0, &mut Missing)
            .unwrap_err()
            .id,
        "execution.spells.location.Player_HasAura_owner"
    );
    let row = SpellArea {
        aura: i32::MIN,
        ..area()
    };
    assert!(!row.fits_location(None, 0, 0, &mut Missing).unwrap());
    assert_eq!(
        row.fits_location(Some(player()), 0, 0, &mut Missing)
            .unwrap_err()
            .id,
        "execution.spells.area_abs_undefined"
    );
}
#[test]
fn area_bounds_override_battleground_and_special_id_checks_in_insertion_order() {
    let mut t = template(7);
    t.attributesex3 = 0x800;
    let s = stores(vec![SpellArea { area: 12, ..area() }, area()]);
    assert_eq!(
        s.allowed_in_location(&t, 0, 0, 0, None, &mut Missing)
            .unwrap(),
        R::Ok
    );
    let s = stores(vec![SpellArea { area: 12, ..area() }]);
    assert_eq!(
        s.allowed_in_location(&t, 0, 0, 0, None, &mut Missing)
            .unwrap(),
        R::RequiresArea
    );
    let s = stores(vec![
        SpellArea {
            condition: 1,
            ..area()
        },
        area(),
    ]);
    assert_eq!(
        s.allowed_in_location(&t, 0, 0, 0, Some(player()), &mut Missing)
            .unwrap_err()
            .id,
        "execution.spells.location.IsConditionSatisfied_owner"
    );
    assert_eq!(
        s.allowed_in_location(&t, 0, 0, 0, None, &mut Missing)
            .unwrap(),
        R::Ok
    );
}
#[test]
fn battleground_attribute_and_waiting_resurrection_short_circuit_null_player() {
    for id in [7, 2584] {
        let mut t = template(id);
        t.attributesex3 = if id == 7 { 0x800 } else { 0 };
        let s = SourceSpellStores::default();
        assert_eq!(
            s.allowed_in_location(&t, 0, 0, 0, None, &mut Missing)
                .unwrap(),
            R::OnlyBattlegrounds
        );
        assert_eq!(
            s.allowed_in_location(&t, 0, 0, 0, Some(player()), &mut Missing)
                .unwrap_err()
                .id,
            "execution.spells.location.Player_InBattleGround_owner"
        );
        for yes in [false, true] {
            assert_eq!(
                s.allowed_in_location(
                    &t,
                    0,
                    0,
                    0,
                    Some(player()),
                    &mut Live {
                        yes,
                        ..Default::default()
                    }
                )
                .unwrap(),
                if yes { R::Ok } else { R::OnlyBattlegrounds }
            );
        }
    }
}
#[test]
fn recall_gets_battleground_before_map_comparison_then_status_only_if_map_and_bg_match() {
    struct Bg {
        present: bool,
        wait: bool,
        calls: Vec<&'static str>,
    }
    impl SpellLocationOwner for Bg {
        fn battleground(&mut self, _: ObjectHandle) -> Result<bool> {
            self.calls.push("bg");
            Ok(self.present)
        }
        fn battleground_wait_join(&mut self, _: ObjectHandle) -> Result<bool> {
            self.calls.push("wait");
            Ok(self.wait)
        }
    }
    let s = SourceSpellStores::default();
    for id in [22563, 22564] {
        let t = template(id);
        assert_eq!(
            s.allowed_in_location(&t, 30, 0, 0, None, &mut Missing)
                .unwrap(),
            R::RequiresArea
        );
        assert_eq!(
            s.allowed_in_location(&t, 0, 0, 0, Some(player()), &mut Missing)
                .unwrap_err()
                .id,
            "execution.spells.location.Player_GetBattleGround_owner"
        );
        for map in [0, 30] {
            for present in [false, true] {
                for wait in [false, true] {
                    let mut bg = Bg {
                        present,
                        wait,
                        calls: vec![],
                    };
                    assert_eq!(
                        s.allowed_in_location(&t, map, 0, 0, Some(player()), &mut bg)
                            .unwrap(),
                        if map == 30 && present && !wait {
                            R::Ok
                        } else {
                            R::RequiresArea
                        }
                    );
                    assert_eq!(
                        bg.calls,
                        if map == 30 && present {
                            vec!["bg", "wait"]
                        } else {
                            vec!["bg"]
                        }
                    );
                }
            }
        }
    }
    struct StatusMissing;
    impl SpellLocationOwner for StatusMissing {
        fn battleground(&mut self, _: ObjectHandle) -> Result<bool> {
            Ok(true)
        }
    }
    assert_eq!(
        s.allowed_in_location(
            &template(22563),
            30,
            0,
            0,
            Some(player()),
            &mut StatusMissing
        )
        .unwrap_err()
        .id,
        "execution.spells.location.BattleGround_GetStatus_owner"
    );
}
#[test]
fn spirit_heal_map_entry_null_nonbg_and_bg_are_distinct_and_reached_without_player() {
    let s = SourceSpellStores::default();
    for id in [22011, 22012, 24171] {
        for player in [None, Some(player())] {
            assert_eq!(
                s.allowed_in_location(&template(id), 0, 0, 0, player, &mut Missing)
                    .unwrap_err()
                    .id,
                "execution.spells.location.MapEntry_owner"
            );
            for (map, result) in [
                (None, R::RequiresArea),
                (Some(false), R::OnlyBattlegrounds),
                (Some(true), R::Ok),
            ] {
                assert_eq!(
                    s.allowed_in_location(
                        &template(id),
                        0,
                        0,
                        0,
                        player,
                        &mut Live {
                            map,
                            ..Default::default()
                        }
                    )
                    .unwrap(),
                    result
                );
            }
        }
    }
    assert_eq!(
        s.allowed_in_location(&template(7), 0, 0, 0, None, &mut Missing)
            .unwrap(),
        R::Ok
    );
}
#[test]
fn beneficiary_uses_live_control_not_cached_beneficiary_and_refuses_unported_master_reads() {
    let mut objects = SourceObjects::default();
    let mut units = SourceUnits::default();
    assert_eq!(
        spell_beneficiary_player(&objects, &units, None).unwrap(),
        None
    );
    for kind in [
        SourceObjectKind::Creature,
        SourceObjectKind::Pet,
        SourceObjectKind::Player,
    ] {
        let caster = objects.construct(1, kind).unwrap();
        assert_eq!(
            spell_beneficiary_player(&objects, &units, Some(caster))
                .unwrap_err()
                .id,
            "execution.unit.fields_generation_missing"
        );
        units.register(&objects, caster, fields()).unwrap();
        units.get_mut(&objects, caster).unwrap().beneficiary = Some(caster);
        assert_eq!(
            spell_beneficiary_player(&objects, &units, Some(caster)).unwrap(),
            (kind == SourceObjectKind::Player).then_some(caster)
        );
        units.get_mut(&objects, caster).unwrap().owner = Some(caster);
        assert_eq!(
            spell_beneficiary_player(&objects, &units, Some(caster))
                .unwrap_err()
                .id,
            "execution.spells.location.Unit_GetOwner_owner"
        );
        units.get_mut(&objects, caster).unwrap().charmer = Some(caster);
        assert_eq!(
            spell_beneficiary_player(&objects, &units, Some(caster))
                .unwrap_err()
                .id,
            "execution.spells.location.Unit_GetCharmer_owner"
        );
    }
}

#[test]
fn area_loader_drop_rules_and_validation_order() {
    struct Known;
    impl SpellLoaderOwner for Known {
        fn contains(&mut self, _: SpellDependency, id: u32) -> Result<bool> {
            Ok(id != 999)
        }
    }
    let cases: [([u32; 10], &str); 11] = [
        (
            [999, 999, 999, 0, 999, 999, 999, 256, 3, 0],
            "missing_spell",
        ),
        ([7, 999, 999, 0, 999, 999, 999, 256, 3, 0], "missing_area"),
        (
            [7, 0, 999, 0, 999, 999, 999, 256, 3, 0],
            "missing_condition",
        ),
        (
            [7, 0, 999, 0, 999, 0, 999, 256, 3, 0],
            "missing_start_quest",
        ),
        ([7, 0, 1, 0, 999, 0, 999, 256, 3, 0], "missing_end_quest"),
        ([7, 0, 1, 0, 1, 0, 999, 256, 3, 0], "same_quest"),
        ([7, 0, 0, 0, 0, 0, 999, 256, 3, 0], "race_mask"),
        ([7, 0, 0, 0, 0, 0, 999, 0, 3, 0], "gender"),
        ([7, 0, 0, 0, 0, 0, 999, 0, 2, 0], "missing_aura"),
        ([7, 0, 0, 0, 0, 0, 8, 0, 2, 0], "aura_type"),
        ([7, 0, 0, 0, 0, 0, 7, 0, 2, 0], "self_aura"),
    ];
    for (words, reason) in cases {
        let mut s = SourceSpellStores::default();
        s.dbc.insert("AreaTable.dbc".into(), BTreeMap::new());
        let mut t = template(7);
        t.effectapplyauraname1 = 4;
        s.insert_template(t).unwrap();
        s.insert_template(template(8)).unwrap();
        s.load_row(
            "spell_area",
            0,
            &words.map(serde_json::Value::from),
            &mut Known,
        );
        assert_eq!(
            s.loads["spell_area"][0].disposition,
            SpellRowDisposition::Skipped(reason.into())
        );
        assert!(s.areas(7).unwrap().is_empty());
    }
    let mut s = SourceSpellStores::default();
    s.dbc.insert("AreaTable.dbc".into(), BTreeMap::new());
    s.insert_template(template(7)).unwrap();
    for (ordinal, words) in [
        [7, 0, 1, 1, 1, 0, 0, 1, 2, 0],
        [7, 0, 999, 0, 999, 1, 0, 256, 3, 0],
    ]
    .into_iter()
    .enumerate()
    {
        s.load_row(
            "spell_area",
            ordinal as u64,
            &words.map(serde_json::Value::from),
            &mut Known,
        );
    }
    assert_eq!(s.areas(7).unwrap().len(), 2);
}

#[test]
fn default_check_binds_location_owner_and_keeps_missing_binding_reachable() {
    struct Bound<'a> {
        stores: &'a SourceSpellStores,
        objects: &'a SourceObjects,
        units: &'a SourceUnits,
        live: Missing,
    }
    impl SpellExecutionOwner for Bound<'_> {
        fn location_context(&mut self) -> Result<SpellLocationContext<'_>> {
            Ok(SpellLocationContext {
                stores: self.stores,
                objects: self.objects,
                units: self.units,
                owner: &mut self.live,
            })
        }
    }
    struct Unbound;
    impl SpellExecutionOwner for Unbound {}
    let mut objects = SourceObjects::default();
    let caster = objects.construct(1, SourceObjectKind::Creature).unwrap();
    let map = MapIdentity {
        map: 0,
        instance: 0,
        creation_generation: 1,
    };
    let cells = SourceCells::new(map).unwrap();
    objects.bind_map(caster, map, &cells).unwrap();
    let mut units = SourceUnits::default();
    units.register(&objects, caster, fields()).unwrap();
    let s = stores(vec![SpellArea { area: 12, ..area() }]);
    let t = template(7);
    let mut spell = authored_spell(&t, caster, 0);
    let mut bound = Bound {
        stores: &s,
        objects: &objects,
        units: &units,
        live: Missing,
    };
    assert_eq!(
        bound
            .check(&mut spell, &t, SpellCheck::AllowedInLocation([12, 87]))
            .unwrap(),
        R::Ok
    );
    assert_eq!(
        bound
            .check(&mut spell, &t, SpellCheck::AllowedInLocation([0, 87]))
            .unwrap(),
        R::RequiresArea
    );
    assert_eq!(
        Unbound
            .check(&mut spell, &t, SpellCheck::AllowedInLocation([12, 87]))
            .unwrap_err(),
        SpellCheck::AllowedInLocation([12, 87]).missing_owner()
    );
}

#[test]
fn check_cast_continues_to_items_and_only_source_rejections_become_dont_report() {
    struct Bound<'a> {
        stores: &'a SourceSpellStores,
        objects: &'a SourceObjects,
        units: &'a SourceUnits,
        live: Missing,
        unit_reads: usize,
        checks: Vec<SpellCheck>,
    }
    impl SpellExecutionOwner for Bound<'_> {
        fn check(
            &mut self,
            spell: &mut SourceSpell,
            t: &SpellTemplate,
            check: SpellCheck,
        ) -> Result<R> {
            self.checks.push(check);
            match check {
                SpellCheck::Items => {
                    let player = self.unit(spell.caster)?.player;
                    spell.check_items(t, player, self)
                }
                SpellCheck::AllowedInLocation(location) => {
                    self.location_context()?.check(t, spell.caster, location)
                }
                _ => Err(check.missing_owner()),
            }
        }
        fn unit(&mut self, _: ObjectHandle) -> Result<SpellUnitState> {
            self.unit_reads += 1;
            Ok(operands())
        }
        fn zone_area(&mut self, _: ObjectHandle) -> Result<[u32; 2]> {
            Ok([12, 87])
        }
        fn location_context(&mut self) -> Result<SpellLocationContext<'_>> {
            Ok(SpellLocationContext {
                stores: self.stores,
                objects: self.objects,
                units: self.units,
                owner: &mut self.live,
            })
        }
    }
    let mut objects = SourceObjects::default();
    let caster = objects.construct(1, SourceObjectKind::Creature).unwrap();
    let map = MapIdentity {
        map: 0,
        instance: 0,
        creation_generation: 1,
    };
    let cells = SourceCells::new(map).unwrap();
    objects.bind_map(caster, map, &cells).unwrap();
    let mut units = SourceUnits::default();
    units.register(&objects, caster, fields()).unwrap();
    let t = template(7);
    for flags in [1, 0x800] {
        for area_id in [12, 99] {
            let s = stores(vec![SpellArea {
                area: area_id,
                ..area()
            }]);
            let mut owner = Bound {
                stores: &s,
                objects: &objects,
                units: &units,
                live: Missing,
                unit_reads: 0,
                checks: Vec::new(),
            };
            let mut spell = authored_spell(&t, caster, flags);
            if area_id == 12 {
                if flags == 1 {
                    assert_eq!(spell.check_cast(&t, false, &mut owner).unwrap(), R::Ok);
                    assert_eq!(
                        owner.checks,
                        [SpellCheck::AllowedInLocation([12, 87]), SpellCheck::Items]
                    );
                } else {
                    // Spell.cpp:6219-6220 skips range for strict=false, cast time 0.
                    assert_eq!(
                        spell.check_cast(&t, false, &mut owner).unwrap_err(),
                        SpellCheck::Power(false).missing_owner()
                    );
                    assert_eq!(
                        owner.checks,
                        [
                            SpellCheck::AllowedInLocation([12, 87]),
                            SpellCheck::Items,
                            SpellCheck::Power(false),
                        ]
                    );
                }
                // One read in CheckCast, then the non-player CheckItems branch.
                assert_eq!(owner.unit_reads, 2);
            } else {
                assert_eq!(
                    spell.check_cast(&t, false, &mut owner).unwrap(),
                    if flags == 1 {
                        R::DontReport
                    } else {
                        R::RequiresArea
                    }
                );
            }
        }
    }
    let s = SourceSpellStores::default();
    let t = template(22011);
    let mut owner = Bound {
        stores: &s,
        objects: &objects,
        units: &units,
        live: Missing,
        unit_reads: 0,
        checks: Vec::new(),
    };
    let mut spell = authored_spell(&t, caster, 1);
    assert_eq!(
        spell.check_cast(&t, false, &mut owner).unwrap_err().id,
        "execution.spells.location.MapEntry_owner"
    );
}
