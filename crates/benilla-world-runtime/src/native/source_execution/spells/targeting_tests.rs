// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

struct Relations(u32);
impl SourceRelationOwner for Relations {
    fn relation_flags(&self, _: ObjectHandle) -> Result<u32> {
        Ok(self.0)
    }
    fn relation_master(&self, _: ObjectHandle) -> Result<Option<ObjectHandle>> {
        Ok(None)
    }
}

fn area_spell(a: u32, b: u32) -> SpellTemplate {
    SpellTemplate {
        id: 1032,
        effect1: 35,
        effectapplyauraname1: 22,
        effectimplicittargeta1: a,
        effectimplicittargetb1: b,
        ..Default::default()
    }
}

fn with_fixture<T>(
    t: &SpellTemplate,
    flags: u32,
    kind: SourceObjectKind,
    run: impl FnOnce(&SelfSpellTargeting<'_>, &mut SourceSpell) -> Result<T>,
) -> Result<T> {
    let mut stores = SourceSpellStores::default();
    // Authored loaded tables with no rows; no private inputs are needed.
    stores.bind_test_dbc("SpellRange.dbc", []);
    stores.bind_test_dbc("SpellDuration.dbc", []);
    stores.bind_test_dbc("SpellRadius.dbc", []);
    stores.insert_template(t.clone())?;
    let mut objects = SourceObjects::default();
    let caster = objects.construct(7, kind)?;
    let mut s = SourceSpell::new(1, t, caster, SpellTriggerFlags(1), 0)?;
    s.affective_caster = Some(caster);
    s.targets.unit = Some(caster);
    let unit = SpellUnitState {
        alive: true,
        standing: true,
        player: false,
        gameobject: false,
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
    };
    let targets = SourceSpellTargetMgr::initialize(&stores);
    let live = SelfSpellTargeting {
        stores: &stores,
        targets: &targets,
        objects: &objects,
        unit: &unit,
        level: 40,
        immunities: &CreatureImmunityState::default(),
        auras: &SourceUnitAuras::default(),
        relations: &Relations(flags),
    };
    run(&live, &mut s)
}

fn select(t: SpellTemplate, ignored: [bool; 2], flags: u32) -> Result<SelfTargetSelection> {
    with_fixture(&t, flags, SourceObjectKind::Creature, |live, s| {
        let (selected, units) =
            live.select(s, &t, 0, SpellTargetImplicitType::SpecialUnit, ignored)?;
        assert_eq!(units.map(|unit| unit.is_some()), selected.has_candidate);
        Ok(selected)
    })
}

#[test]
fn area_target_uses_explicit_single_unit_and_skips_aoe_b() {
    let selected = select(area_spell(1, 20), [false; 2], 0).unwrap();
    assert_eq!(selected.modes, [1, 20]);
    assert_eq!(selected.dispatched, [true, false]);
    assert!(selected.had_single && !selected.caster_default);
    assert_eq!(selected.has_candidate, [true, false]);
}

#[test]
fn area_target_defaults_to_caster_when_aoe_b_is_the_only_mode() {
    let selected = select(area_spell(0, 20), [false; 2], 0).unwrap();
    assert_eq!(selected.dispatched, [false; 2]);
    assert!(!selected.had_single && selected.caster_default);
    assert_eq!(selected.has_candidate, [true, false]);
}

#[test]
fn area_target_examines_nonignored_b_and_never_replaces_an_empty_single() {
    let t = area_spell(1, 21);
    for (flags, has_candidate) in [(0, true), (0x0200_0008, false)] {
        let selected = select(t.clone(), [true, false], flags).unwrap();
        assert_eq!(selected.dispatched, [false, true]);
        assert!(selected.had_single && !selected.caster_default);
        assert_eq!(selected.has_candidate, [false, has_candidate]);
    }
    let selected = select(t, [true, true], 0).unwrap();
    assert_eq!(selected.dispatched, [false; 2]);
    assert!(!selected.had_single && selected.caster_default);
}

#[test]
fn area_target_a_calls_the_party_owner_before_considering_b_or_default() {
    assert_eq!(
        select(area_spell(20, 1), [false; 2], 0).unwrap_err().id,
        "execution.spells.Spell_2389.FillRaidOrPartyTargets_owner"
    );
}

#[test]
fn friendly_target_mode_honours_exclude_caster_but_caster_mode_does_not() {
    for (mode, candidate) in [(1, true), (21, false), (57, false)] {
        let mut t = area_spell(mode, 0);
        t.attributesex = 0x0008_0000;
        let selected = select(t, [false; 2], 0).unwrap();
        assert_eq!(selected.has_candidate, [candidate, false]);
        assert!(selected.had_single && !selected.caster_default);
    }
}

#[test]
fn existing_target_table_bounds_are_enforced_at_the_live_read() {
    assert_eq!(
        select(area_spell(63, 0), [false; 2], 0).unwrap_err().id,
        "execution.spells.Spell_1785.SetTargetMap_owner"
    );
    assert_eq!(
        select(area_spell(64, 0), [false; 2], 0).unwrap_err().id,
        "execution.spells.targeting.TargetTableIndexUndefined { spell: 1032, effect: 0, right: 0, value: 64 }"
    );
}

#[test]
fn ordinary_unit_no_targets_stops_at_the_entry_mask_or_default_read() {
    let mut t = area_spell(0, 0);
    t.effect1 = 6;
    assert_eq!(
        with_fixture(&t, 0, SourceObjectKind::Creature, |live, s| live.fill(
            s,
            &t,
            |_| {}
        ))
        .unwrap_err()
        .id,
        "execution.spells.Spell_581.FillFromTargetFlags_or_default_owner"
    );
}

#[test]
fn targetless_effect_has_no_unit_hit_and_refuses_explicit_target_map() {
    let mut t = SpellTemplate {
        id: 204,
        effect1: 26,
        // SpellTargets.cpp:469-484. A unit entry mask cannot fill a targetless effect.
        targets: 2,
        ..SpellTemplate::default()
    };
    with_fixture(&t, 0, SourceObjectKind::Creature, |live, s| {
        live.fill(s, &t, |_| panic!("targetless effect selected a unit"))?;
        assert_eq!(s.targetless_mask, 1);
        assert!(s.unique_targets.is_empty());
        Ok(())
    })
    .unwrap();
    t.effectimplicittargeta1 = 1;
    assert_eq!(
        with_fixture(&t, 0, SourceObjectKind::Creature, |live, s| live.fill(
            s,
            &t,
            |_| {}
        ))
        .unwrap_err()
        .id,
        "execution.spells.Spell_589.SetTargetMap_owner"
    );
    // SpellTargets.cpp:219. Effect 118 is targetless with a nonzero default.
    t.effectimplicittargeta1 = 0;
    t.effect1 = 118;
    assert_eq!(
        with_fixture(&t, 0, SourceObjectKind::Creature, |live, s| live.fill(
            s,
            &t,
            |_| {}
        ))
        .unwrap_err()
        .id,
        "execution.spells.Spell_584.SetTargetMap_default_owner"
    );
}

#[test]
fn unknown_effect_without_default_selects_no_hit() {
    // Stock SQL :840044; SpellTargets.cpp:150; Spell.cpp:578-584,680-681.
    let t = SpellTemplate {
        id: 3050,
        effect1: 49,
        ..SpellTemplate::default()
    };
    with_fixture(&t, 0, SourceObjectKind::Creature, |live, spell| {
        live.fill(spell, &t, |_| panic!("Detect selected a unit"))?;
        assert_eq!(spell.targetless_mask, 0);
        assert!(spell.unique_targets.is_empty());
        Ok(())
    })
    .unwrap();
}

#[test]
fn radius_callback_gates_precede_caster_arm() {
    let t = area_spell(1, 0);
    for (kind, scripted, owner) in [
        (
            SourceObjectKind::Player,
            true,
            "execution.spells.Spell_7551.ApplySpellMod_radius_owner",
        ),
        (
            SourceObjectKind::Creature,
            true,
            "execution.spells.Spell_7583.OnRadiusCalculate_owner",
        ),
    ] {
        assert_eq!(
            with_fixture(&t, 0, kind, |live, s| {
                s.spell_script =
                    scripted.then_some(SourceSpellScript::DetectThroughInvisibilityMob);
                live.fill(s, &t, |_| panic!("dispatch must not run"))
            })
            .unwrap_err()
            .id,
            owner
        );
    }
}

#[test]
fn caster_default_passes_the_same_live_unit_fill_as_explicit_caster() {
    for modes in [(0, 0), (0, 20), (1, 0)] {
        let t = area_spell(modes.0, modes.1);
        with_fixture(&t, 0, SourceObjectKind::Creature, |live, s| {
            live.fill(s, &t, |_| {})?;
            let target = s.unique_targets.first().unwrap();
            assert_eq!(
                (target.unit, target.effect_mask, target.hit_mask),
                (s.caster, 1, 1)
            );
            assert!(!target.missed);
            Ok(())
        })
        .unwrap();
    }
}

#[test]
fn fallback_does_not_skip_the_creature_type_or_required_empty_target_read() {
    let mut t = area_spell(0, 0);
    t.targetcreaturetype = 1;
    assert_eq!(
        with_fixture(&t, 0, SourceObjectKind::Creature, |live, s| live.fill(
            s,
            &t,
            |_| {}
        ))
        .unwrap_err()
        .id,
        "execution.spells.Spell_6918.GetCreatureTypeMask_owner"
    );
    t.effectimplicittargeta1 = 21;
    t.attributesex = 0x0400_0000;
    assert_eq!(
        with_fixture(&t, 0x0200_0008, SourceObjectKind::Creature, |live, s| live
            .fill(s, &t, |_| {}))
        .unwrap_err()
        .id,
        "execution.spells.Spell_773.SendCastResult_owner"
    );
}

#[test]
fn fill_names_unbound_filters_and_player_pvp_reads_before_add_unit_target() {
    let t = area_spell(1, 0);
    with_fixture(&t, 0, SourceObjectKind::Creature, |live, s| {
        let mut data = SpellTargetingData::default();
        data.filtering_scheme[0][0] = SpellTargetFilterScheme::HighestHp;
        assert_eq!(
            live.fill_unit_targets(s, &t, 0, &data, [Some(s.caster), None])
                .unwrap_err()
                .id,
            "execution.spells.Spell_749.FilterTargetMap_owner"
        );
        data.filtering_scheme[0][0] = SpellTargetFilterScheme::Random;
        let mut unit = *live.unit;
        unit.player = true;
        let player = SelfSpellTargeting {
            unit: &unit,
            ..*live
        };
        assert_eq!(
            player
                .fill_unit_targets(s, &t, 0, &data, [Some(s.caster), None])
                .unwrap_err()
                .id,
            "execution.spells.Spell_756.GetBeneficiaryPlayer_owner"
        );
        assert!(s.unique_targets.is_empty());
        Ok(())
    })
    .unwrap();
}

#[test]
fn authored_area_effect_inserts_a_living_target_and_skips_a_dead_target() {
    struct Hit<'a> {
        unit: SelfAuraTarget<'a>,
        auras: SourceUnitAuras,
        visibility: SourceUnitVisibility,
        damage: SourceCreatureDamage,
        clocks: Vec<i64>,
    }
    impl SpellAuraHitOwner for Hit<'_> {
        fn aura_target(&self, _: ObjectHandle) -> Result<SelfAuraTarget<'_>> {
            Ok(self.unit)
        }
        fn aura_holder_seconds(&mut self, _: &SourceSpell) -> Result<i64> {
            self.clocks.push(42);
            Ok(42)
        }
        fn aura_effect_seconds(&mut self, _: &SourceSpell) -> Result<i64> {
            self.clocks.push(43);
            Ok(43)
        }
        fn aura_hit_target(&mut self, _: ObjectHandle) -> Result<SelfAuraHitTarget<'_>> {
            Ok(SelfAuraHitTarget {
                unit: self.unit,
                stand: 0,
                auras: &mut self.auras,
                visibility: &mut self.visibility,
                damage: DamageAuraTarget::unmodified(&mut self.damage),
            })
        }
        fn apply_inserted_aura_holder(
            &mut self,
            _: &SpellTemplate,
            holder: SourceAuraHolderId,
        ) -> Result<()> {
            let effect = self.auras.aura_holder(holder).unwrap().effects[0]
                .as_ref()
                .unwrap();
            assert_eq!((effect.apply_time, effect.script_value), (43, 71));
            assert_eq!(effect.area.unwrap().kind, SourceAreaAuraKind::Party);
            Ok(())
        }
        fn proc_aura_hit(&mut self, _: &mut SourceSpell) -> Result<()> {
            Ok(())
        }
    }
    let mut t = area_spell(1, 0);
    t.effectbasepoints1 = 504;
    t.effectbasedice1 = 1;
    t.effectdiesides1 = 1;
    for alive in [false, true] {
        with_fixture(&t, 0, SourceObjectKind::Creature, |live, s| {
            live.fill(s, &t, |_| {})?;
            let caster = s.caster;
            s.script_value = 71;
            let controls = SourceControlGuids::default();
            let mut hit = Hit {
                unit: SelfAuraTarget {
                    target: caster,
                    level: Some(live.level),
                    alive,
                    player: false,
                    player_group: None,
                    totem: false,
                    objects: live.objects,
                    control_guids: &controls,
                },
                auras: SourceUnitAuras::default(),
                visibility: SourceUnitVisibility::default(),
                damage: SourceCreatureDamage::default(),
                clocks: Vec::new(),
            };
            s.hit_self_auras(&t, live.stores, &mut hit)?;
            assert!(s.pending_aura.is_none());
            assert_eq!(hit.auras.has_aura_holders(), alive);
            assert_eq!(hit.clocks, if alive { &[42, 43][..] } else { &[42][..] });
            if let Some(holder) = hit.auras.first_aura_holder(t.id) {
                assert_eq!((holder.apply_time, holder.diminish_group), (42, 0));
                assert_eq!(holder.effects[0].as_ref().unwrap().amount, 505);
                assert!(holder.ready && holder.skip_update);
            }
            assert!(s.unique_targets[0].processed);
            assert_eq!(s.damage_per_effect, [505, 0, 0]);
            Ok(())
        })
        .unwrap();
    }
}

#[test]
fn authored_caster_default_refuses_to_overwrite_an_existing_candidate() {
    let caster = ObjectHandle {
        thread: 7,
        construction: 1,
    };
    let other = ObjectHandle {
        thread: 7,
        construction: 2,
    };
    let mut side_a = None;
    add_default_caster(&mut side_a, caster).unwrap();
    assert_eq!(side_a, Some(caster));
    side_a = Some(other);
    assert_eq!(
        add_default_caster(&mut side_a, caster).unwrap_err().id,
        "execution.spells.Spell_572.caster_default_append_owner"
    );
    assert_eq!(side_a, Some(other));
}
