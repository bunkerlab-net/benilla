// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored state through the real power owner, not observed stock casts.
use super::*;
use SpellCastResult as R;

pub(super) struct PowerFixture {
    pub objects: SourceObjects,
    pub units: SourceUnits,
    pub caster: ObjectHandle,
    pub initialized: benilla_sim::creature_init::InitializedCreature,
    pub level: u32,
    pub flat: i32,
    pub multiplier: u32,
    pub mod_owner: Option<ObjectHandle>,
    pub mod_delta: i32,
    pub trace: Vec<String>,
    pub missing: Option<&'static str>,
}
impl PowerFixture {
    pub fn new() -> Self {
        use benilla_sim::creature_init as init;
        let input =
            super::super::super::synthetic_creature_generation_fixture::source(40, 5, 5).input;
        let plan = init::prepare(&input).unwrap();
        let model = plan.model_roll_range().map(|r| r.0);
        let gender = plan.gender_roll_range(model).unwrap().map(|r| r.0);
        let initialized = plan
            .resolve(init::SelectionRolls {
                model,
                gender,
                level: None,
            })
            .unwrap();
        let t = &input.template;
        let fields = CreatureAiTemplateOperands {
            ordinal: 0,
            entry: t.entry,
            faction: t.faction,
            extra_flags: t.extra_flags,
            unit_flags: t.unit_flags,
            type_flags: t.type_flags,
            static_flags: t.static_flags,
            rank: t.rank,
            inhabit_type: 1,
            stats_multipliers: [
                t.health_multiplier.bits(),
                t.power_multiplier.bits(),
                t.armor_multiplier.bits(),
            ],
            civilian: false,
            pursuit_ms: 0,
            call_for_help: 0,
            creature_type: 0,
            ai_name: String::new(),
            script_name: String::new(),
        };
        let mut objects = SourceObjects::default();
        let caster = objects.construct(1, SourceObjectKind::Creature).unwrap();
        let mut units = SourceUnits::default();
        let mut fields = SourceUnitFields::creature(&fields);
        fields.apply_initialized(&initialized);
        units.register(&objects, caster, fields).unwrap();
        Self {
            objects,
            units,
            caster,
            initialized,
            level: 5,
            flat: 0,
            multiplier: 0,
            mod_owner: None,
            mod_delta: 0,
            trace: Vec::new(),
            missing: None,
        }
    }
    pub fn spell(&self, t: &SpellTemplate) -> SourceSpell {
        SourceSpell::new(1, t, self.caster, SpellTriggerFlags(0), 0).unwrap()
    }
    pub fn vitals(&mut self) -> &mut SourceUnitVitals {
        self.units
            .get_mut(&self.objects, self.caster)
            .unwrap()
            .vitals
            .as_mut()
            .unwrap()
    }
    fn read(&mut self, name: &'static str) -> Result<()> {
        self.trace.push(name.into());
        if self.missing == Some(name) {
            Err(refuse(&format!("authored.missing_{name}")))
        } else {
            Ok(())
        }
    }
}
impl SpellExecutionOwner for PowerFixture {
    fn power_objects(&mut self) -> Result<&SourceObjects> {
        Ok(&self.objects)
    }
    fn power_context(&mut self) -> Result<SpellPowerContext<'_>> {
        self.read("resources")?;
        Ok(SpellPowerContext {
            objects: &self.objects,
            units: &self.units,
        })
    }
    fn power_level(&mut self, _: ObjectHandle) -> Result<u32> {
        self.read("level")?;
        Ok(self.level)
    }
    fn power_create_health(&mut self, _: ObjectHandle) -> Result<u32> {
        self.read("create_health")?;
        Ok(self.initialized.stats.create_health)
    }
    fn power_create_mana(&mut self, _: ObjectHandle) -> Result<u32> {
        self.read("create_mana")?;
        Ok(self.initialized.stats.create_mana)
    }
    fn power_flat_modifier(&mut self, _: ObjectHandle, school: u32) -> Result<i32> {
        self.read("flat")?;
        self.trace.push(format!("school={school}"));
        Ok(self.flat)
    }
    fn power_mod_owner(&mut self, _: ObjectHandle) -> Result<Option<ObjectHandle>> {
        self.read("mod_owner")?;
        Ok(self.mod_owner)
    }
    fn apply_cost_mod(
        &mut self,
        _: ObjectHandle,
        _: u32,
        cost: i32,
        final_use: bool,
    ) -> Result<i32> {
        self.read("apply_mod")?;
        self.trace.push(format!("final_use={final_use}"));
        Ok(cost + self.mod_delta)
    }
    fn power_multiplier(&mut self, _: ObjectHandle, _: u32) -> Result<u32> {
        self.read("multiplier")?;
        Ok(self.multiplier)
    }
}
fn template() -> SpellTemplate {
    SpellTemplate {
        id: 7,
        ..Default::default()
    }
}

#[test]
fn item_and_nonunit_power_shortcuts_preserve_cached_cost_without_resource_reads() {
    struct Missing;
    impl SpellExecutionOwner for Missing {}
    let mut f = PowerFixture::new();
    let t = template();
    let mut s = f.spell(&t);
    s.power_cost = 81;
    s.item_cast = true;
    assert_eq!(s.check_power(&t, true, &mut Missing).unwrap(), R::Ok);
    assert_eq!(s.calculate_power_cost(&t, false, &mut Missing).unwrap(), 0);
    s.item_cast = false;
    f.objects.get_mut(f.caster).unwrap().kind = SourceObjectKind::GameObject;
    assert_eq!(s.check_power(&t, true, &mut f).unwrap(), R::Ok);
    assert_eq!(s.power_cost, 81);
    assert!(f.trace.is_empty());
}
#[test]
fn current_power_equality_passes_and_one_less_returns_no_power_after_cost_assignment() {
    let mut f = PowerFixture::new();
    let t = SpellTemplate {
        manacost: 20,
        ..template()
    };
    let mut s = f.spell(&t);
    for (mana, expected) in [(19, R::NoPower), (20, R::Ok), (21, R::Ok)] {
        f.vitals().mana = mana;
        assert_eq!(s.check_power(&t, true, &mut f).unwrap(), expected);
        assert_eq!(s.power_cost, 20);
    }
}
#[test]
fn health_cost_equality_fails_but_one_more_passes() {
    let mut f = PowerFixture::new();
    let t = SpellTemplate {
        powertype: SPELL_POWER_HEALTH,
        manacost: 20,
        ..template()
    };
    let mut s = f.spell(&t);
    for (health, expected) in [
        (19, R::CasterAuraState),
        (20, R::CasterAuraState),
        (21, R::Ok),
    ] {
        f.vitals().health = health;
        assert_eq!(s.check_power(&t, false, &mut f).unwrap(), expected);
    }
}
#[test]
fn drain_all_precedes_rank_modifiers_and_health_equality_always_fails() {
    let mut f = PowerFixture::new();
    let mut t = SpellTemplate {
        attributesex: 2,
        ..template()
    };
    f.vitals().mana = 17;
    let mut s = f.spell(&t);
    assert_eq!(s.check_power(&t, true, &mut f).unwrap(), R::Ok);
    assert_eq!(s.power_cost, 17);
    assert_eq!(f.trace, ["resources", "resources"]);
    t.powertype = SPELL_POWER_HEALTH;
    f.vitals().health = 17;
    assert_eq!(s.check_power(&t, true, &mut f).unwrap(), R::CasterAuraState);
    assert_eq!(s.power_cost, 17);
}
#[test]
fn invalid_power_returns_unknown_only_after_cost_calculation() {
    let mut f = PowerFixture::new();
    for (all, percent, expected_cost) in [(0, 0, 27), (2, 0, 0), (0, 10, 0)] {
        let t = SpellTemplate {
            powertype: 5,
            attributesex: all,
            manacost: 27,
            manacostpercentage: percent,
            ..template()
        };
        let mut s = f.spell(&t);
        assert_eq!(s.check_power(&t, true, &mut f).unwrap(), R::Unknown);
        assert_eq!(s.power_cost, expected_cost);
    }
    let t = SpellTemplate {
        powertype: u32::MAX,
        ..template()
    };
    let mut s = f.spell(&t);
    f.missing = Some("flat");
    assert_eq!(
        s.check_power(&t, true, &mut f).unwrap_err().id,
        "authored.missing_flat"
    );
}
#[test]
fn rank_uses_creature_level_without_times_five_and_caps_with_unsigned_product() {
    let t = SpellTemplate {
        maxlevel: 3,
        ..template()
    };
    assert_eq!(creature_power_rank(14, &t), 14);
    assert_eq!(creature_power_rank(15, &t), 15);
    assert_eq!(creature_power_rank(16, &t), 15);
    let t = SpellTemplate {
        maxlevel: 0x3333_3334,
        ..t
    };
    assert_eq!(creature_power_rank(21, &t), 4);
}
#[test]
fn unsigned_per_level_and_percentage_wraps_are_not_signed_undefined_behavior() {
    let mut f = PowerFixture::new();
    let t = SpellTemplate {
        manacost: 5,
        manacostperlevel: 10,
        baselevel: 2,
        ..template()
    };
    let s = f.spell(&t);
    // rank 5 / 5 - base 2 wraps, cost is -5, and only the final clamp makes zero.
    assert_eq!(s.calculate_power_cost(&t, false, &mut f).unwrap(), 0);
    f.initialized.stats.create_mana = 2;
    let t = SpellTemplate {
        manacostpercentage: u32::MAX,
        ..template()
    };
    assert_eq!(
        s.calculate_power_cost(&t, false, &mut f).unwrap(),
        42_949_672
    );
}
#[test]
fn percent_health_and_mana_use_create_values_not_current_or_maximum() {
    let mut f = PowerFixture::new();
    f.initialized.stats.create_mana = 123;
    f.initialized.stats.create_health = 234;
    f.vitals().max_mana = 999;
    f.vitals().health = 1;
    for (ty, cost) in [(0, 24), (SPELL_POWER_HEALTH, 46)] {
        let t = SpellTemplate {
            powertype: ty,
            manacostpercentage: 20,
            ..template()
        };
        let s = f.spell(&t);
        assert_eq!(s.calculate_power_cost(&t, true, &mut f).unwrap(), cost);
    }
}
#[test]
fn energy_percentage_uses_native_maximum_and_missing_other_types_refuse() {
    let mut f = PowerFixture::new();
    f.vitals().max_energy = 90;
    let mut t = SpellTemplate {
        powertype: 3,
        manacostpercentage: 25,
        ..template()
    };
    let s = f.spell(&t);
    assert_eq!(s.calculate_power_cost(&t, false, &mut f).unwrap(), 22);
    for ty in [1, 2, 4] {
        t.powertype = ty;
        assert_eq!(
            s.calculate_power_cost(&t, false, &mut f).unwrap_err().id,
            "execution.spells.Unit_GetMaxPower_type_owner"
        );
        t.manacostpercentage = 0;
        assert_eq!(
            f.power_current(f.caster, ty).unwrap_err().id,
            "execution.spells.Unit_GetPower_type_owner"
        );
        t.manacostpercentage = 25;
    }
}
#[test]
fn cost_modifier_order_and_strict_final_use_are_preserved() {
    let mut f = PowerFixture::new();
    let t = SpellTemplate {
        manacost: 10,
        ..template()
    };
    let mut s = f.spell(&t);
    f.flat = 2;
    f.mod_owner = Some(f.caster);
    f.mod_delta = 3;
    f.multiplier = 0.5f32.to_bits();
    f.vitals().mana = 100;
    for strict in [true, false] {
        f.trace.clear();
        assert_eq!(s.check_power(&t, strict, &mut f).unwrap(), R::Ok);
        assert_eq!(s.power_cost, 22);
        assert_eq!(
            f.trace,
            [
                "level",
                "flat",
                "school=0",
                "mod_owner",
                "apply_mod",
                if strict {
                    "final_use=false"
                } else {
                    "final_use=true"
                },
                "multiplier",
                "resources"
            ]
        );
    }
}
#[test]
fn first_school_in_mask_and_empty_mask_match_source() {
    let mut f = PowerFixture::new();
    let t = template();
    let mut s = f.spell(&t);
    for (mask, school) in [(0, 0), (0x8000_0000, 0), (0x48, 3)] {
        s.school_mask = mask;
        f.trace.clear();
        s.calculate_power_cost(&t, false, &mut f).unwrap();
        assert!(f.trace.contains(&format!("school={school}")));
    }
}
#[test]
fn flat_signed_add_refuses_exactly_above_boundary_after_read_and_before_mod_owner() {
    let mut f = PowerFixture::new();
    let t = SpellTemplate {
        manacost: i32::MAX as u32,
        ..template()
    };
    let s = f.spell(&t);
    f.flat = 0;
    // The defined int32 sum still reaches the later binary32 conversion UB.
    assert_eq!(
        s.calculate_power_cost(&t, false, &mut f).unwrap_err().id,
        "execution.spells.CalculatePowerCost_multiplier_conversion_undefined"
    );
    f.flat = 1;
    f.trace.clear();
    assert_eq!(
        s.calculate_power_cost(&t, false, &mut f).unwrap_err().id,
        "execution.spells.CalculatePowerCost_flat_add_undefined"
    );
    assert_eq!(f.trace, ["level", "flat", "school=0"]);
    let t = SpellTemplate {
        manacost: i32::MIN as u32,
        ..template()
    };
    f.flat = 0;
    assert_eq!(s.calculate_power_cost(&t, false, &mut f).unwrap(), 0);
    f.flat = -1;
    assert_eq!(
        s.calculate_power_cost(&t, false, &mut f).unwrap_err().id,
        "execution.spells.CalculatePowerCost_flat_add_undefined"
    );
}
#[test]
fn final_binary32_conversion_accepts_neighbor_then_refuses_positive_overflow() {
    let mut f = PowerFixture::new();
    for (raw, accepted) in [(2_147_483_583, true), (2_147_483_584, false)] {
        let t = SpellTemplate {
            manacost: raw,
            ..template()
        };
        let s = f.spell(&t);
        if accepted {
            assert_eq!(
                s.calculate_power_cost(&t, false, &mut f).unwrap(),
                2_147_483_520
            );
        } else {
            assert_eq!(
                s.calculate_power_cost(&t, false, &mut f).unwrap_err().id,
                "execution.spells.CalculatePowerCost_multiplier_conversion_undefined"
            );
        }
    }
}
#[test]
fn nan_infinity_and_negative_cost_clamp_happen_at_final_conversion() {
    let mut f = PowerFixture::new();
    let t = SpellTemplate {
        manacost: 2,
        ..template()
    };
    let s = f.spell(&t);
    for bits in [
        f32::NAN.to_bits(),
        f32::INFINITY.to_bits(),
        f32::NEG_INFINITY.to_bits(),
    ] {
        f.multiplier = bits;
        assert_eq!(
            s.calculate_power_cost(&t, false, &mut f).unwrap_err().id,
            "execution.spells.CalculatePowerCost_multiplier_conversion_undefined"
        );
    }
    f.multiplier = (-2f32).to_bits();
    assert_eq!(s.calculate_power_cost(&t, false, &mut f).unwrap(), 0);
}
#[test]
fn creature_scaling_preserves_binary32_order_and_refuses_undefined_conversion() {
    let mut f = PowerFixture::new();
    let t = SpellTemplate {
        attributes: 0x80000,
        manacost: 100,
        spelllevel: 5,
        ..template()
    };
    let s = f.spell(&t);
    assert_eq!(s.calculate_power_cost(&t, false, &mut f).unwrap(), 101);
    // level zero makes the denominator +infinity when spellLevel is positive.
    f.level = 0;
    assert_eq!(s.calculate_power_cost(&t, false, &mut f).unwrap(), 0);
    let t = SpellTemplate { spelllevel: 0, ..t };
    assert_eq!(
        s.calculate_power_cost(&t, false, &mut f).unwrap_err().id,
        "execution.spells.CalculatePowerCost_scaling_conversion_undefined"
    );
}
#[test]
fn player_rank_requires_actual_skill_owner_not_creature_level() {
    let mut f = PowerFixture::new();
    f.objects.get_mut(f.caster).unwrap().kind = SourceObjectKind::Player;
    let t = template();
    let mut s = f.spell(&t);
    assert_eq!(
        s.check_power(&t, true, &mut f).unwrap_err().id,
        "execution.spells.Player_GetSpellRank_skill_owner"
    );
    assert!(f.trace.is_empty());
}
#[test]
fn native_current_resources_are_not_reconstructed_from_initial_maxima() {
    let mut f = PowerFixture::new();
    let initial = f.initialized.stats.resources;
    assert_eq!(f.power_current(f.caster, 0).unwrap(), initial.mana);
    assert_eq!(f.power_maximum(f.caster, 0).unwrap(), initial.max_mana);
    f.vitals().mana = 0;
    assert_eq!(f.power_current(f.caster, 0).unwrap(), 0);
    f.units.get_mut(&f.objects, f.caster).unwrap().vitals = None;
    assert_eq!(
        f.power_current(f.caster, 0).unwrap_err().id,
        "execution.spells.CreatureStats_current_resources_owner"
    );
}
#[test]
fn power_missing_reads_do_not_move_past_earlier_unavailable_operands() {
    let mut f = PowerFixture::new();
    let t = template();
    let mut s = f.spell(&t);
    for (missing, trace) in [
        ("level", vec!["level"]),
        ("flat", vec!["level", "flat"]),
        ("mod_owner", vec!["level", "flat", "school=0", "mod_owner"]),
        (
            "multiplier",
            vec!["level", "flat", "school=0", "mod_owner", "multiplier"],
        ),
    ] {
        f.trace.clear();
        f.missing = Some(missing);
        assert_eq!(
            s.check_power(&t, true, &mut f).unwrap_err().id,
            format!("authored.missing_{missing}")
        );
        assert_eq!(f.trace, trace);
    }
}

#[test]
fn negative_conversion_boundary_refuses_before_clamp_and_preserves_cached_cost() {
    let mut f = PowerFixture::new();
    let t = SpellTemplate {
        manacost: i32::MIN as u32,
        ..template()
    };
    let mut s = f.spell(&t);
    assert_eq!(s.calculate_power_cost(&t, false, &mut f).unwrap(), 0);
    f.multiplier = (1f32 / 8_388_608f32).to_bits();
    s.power_cost = 91;
    assert_eq!(
        s.check_power(&t, true, &mut f).unwrap_err().id,
        "execution.spells.CalculatePowerCost_multiplier_conversion_undefined"
    );
    assert_eq!(s.power_cost, 91);
}

#[test]
fn check_cast_runs_real_power_then_aura_flags_before_channel_only_success() {
    struct Bound(PowerFixture);
    impl SpellExecutionOwner for Bound {
        fn power_objects(&mut self) -> Result<&SourceObjects> {
            Ok(&self.0.objects)
        }
        fn power_level(&mut self, caster: ObjectHandle) -> Result<u32> {
            self.0.power_level(caster)
        }
        fn power_context(&mut self) -> Result<SpellPowerContext<'_>> {
            self.0.power_context()
        }
        fn power_flat_modifier(&mut self, c: ObjectHandle, school: u32) -> Result<i32> {
            self.0.power_flat_modifier(c, school)
        }
        fn power_mod_owner(&mut self, c: ObjectHandle) -> Result<Option<ObjectHandle>> {
            self.0.power_mod_owner(c)
        }
        fn power_multiplier(&mut self, c: ObjectHandle, school: u32) -> Result<u32> {
            self.0.power_multiplier(c, school)
        }
    }
    let mut f = Bound(PowerFixture::new());
    f.0.units.get_mut(&f.0.objects, f.0.caster).unwrap().flags = 0;
    let t = SpellTemplate {
        attributes: 0x40,
        ..template()
    };
    let mut s = f.0.spell(&t);
    s.flags = SpellTriggerFlags(0x8000);
    // Instant release skips range, passive skips CheckItems, but neither skips
    // power or caster auras, Spell.cpp:5128,5140-5167.
    assert_eq!(
        s.check_cast_location_suffix(&t, false, false, &mut f)
            .unwrap(),
        R::Ok
    );
    assert_eq!(s.partial_application_mask, 7);
    assert_eq!(
        f.0.trace,
        [
            "level",
            "flat",
            "school=0",
            "mod_owner",
            "multiplier",
            "resources",
            "resources"
        ]
    );
    f.0.units.get_mut(&f.0.objects, f.0.caster).unwrap().flags = 0x40000;
    f.0.trace.clear();
    s.partial_application_mask = 0;
    assert_eq!(
        s.check_cast_location_suffix(&t, false, false, &mut f)
            .unwrap_err()
            .id,
        "execution.spells.Unit_GetAurasByType_CheckCasterAuras_owner"
    );
    assert_eq!(s.partial_application_mask, 0);
    assert_eq!(
        f.0.trace,
        [
            "level",
            "flat",
            "school=0",
            "mod_owner",
            "multiplier",
            "resources",
            "resources"
        ]
    );
}
