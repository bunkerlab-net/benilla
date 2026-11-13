// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored aura definitions and live Unit flags, never recorded immunity answers.
use super::*;
use SpellCastResult as R;
struct AuraFixture {
    power: super::power_tests::PowerFixture,
    auras: BTreeMap<u32, Vec<SpellCasterAura>>,
    reads: Vec<u32>,
    missing_type: Option<u32>,
}
impl AuraFixture {
    fn new(flags: u32) -> Self {
        let mut power = super::power_tests::PowerFixture::new();
        power
            .units
            .get_mut(&power.objects, power.caster)
            .unwrap()
            .flags = flags;
        Self {
            power,
            auras: BTreeMap::new(),
            reads: Vec::new(),
            missing_type: None,
        }
    }
    fn spell(&self, t: &SpellTemplate) -> SourceSpell {
        self.power.spell(t)
    }
    fn add(&mut self, ty: u32, mechanic: u32, effect_mechanic: u32) {
        self.auras.entry(ty).or_default().push(SpellCasterAura {
            attributes: 0,
            attributesex2: 0,
            school: 2,
            dispel: 3,
            mechanic,
            effect_mechanic,
            aura_type: ty,
        });
    }
}
impl SpellExecutionOwner for AuraFixture {
    fn power_objects(&mut self) -> Result<&SourceObjects> {
        Ok(&self.power.objects)
    }
    fn power_context(&mut self) -> Result<SpellPowerContext<'_>> {
        self.power.power_context()
    }
    fn caster_auras(&mut self, _: ObjectHandle, ty: u32) -> Result<&[SpellCasterAura]> {
        self.reads.push(ty);
        if self.missing_type == Some(ty) {
            return Err(refuse("authored.aura_list_missing"));
        }
        Ok(self.auras.get(&ty).map_or(&[], Vec::as_slice))
    }
}
fn template() -> SpellTemplate {
    SpellTemplate {
        id: 7,
        ..Default::default()
    }
}
#[test]
fn ignored_caster_aura_state_precedes_even_true_caster_read() {
    struct Missing;
    impl SpellExecutionOwner for Missing {}
    let f = AuraFixture::new(0);
    let t = template();
    let mut s = f.spell(&t);
    s.flags = SpellTriggerFlags(0x10000);
    assert!(s.flags.ignore_caster_aura());
    assert_eq!(s.check_caster_auras(&t, &mut Missing).unwrap(), R::Ok);
}
#[test]
fn nonunit_and_ignore_restrictions_skip_unit_flags() {
    let mut f = AuraFixture::new(u32::MAX);
    let mut t = template();
    let mut s = f.spell(&t);
    f.power.objects.get_mut(f.power.caster).unwrap().kind = SourceObjectKind::GameObject;
    assert_eq!(s.check_caster_auras(&t, &mut f).unwrap(), R::Ok);
    f.power.objects.get_mut(f.power.caster).unwrap().kind = SourceObjectKind::Creature;
    t.attributesex3 = 0x1000_0000;
    assert_eq!(s.check_caster_auras(&t, &mut f).unwrap(), R::Ok);
    assert!(f.power.trace.is_empty());
}
#[test]
fn flag_free_nonclient_reads_flags_but_no_aura_or_charmer_state() {
    let mut f = AuraFixture::new(0);
    let t = template();
    let mut s = f.spell(&t);
    assert_eq!(s.check_caster_auras(&t, &mut f).unwrap(), R::Ok);
    assert_eq!(f.power.trace, ["resources"]);
    assert!(f.reads.is_empty());
}
#[test]
fn stun_silence_pacify_fear_confuse_precedence_uses_actual_aura_lists() {
    let all = 0x0004_0000 | 0x2000 | 0x20000 | 0x800000 | 0x400000;
    for (flags, prevention, ty, expected) in [
        (all, 1, 12, R::Stunned),
        (all & !0x40000, 1, 27, R::Silenced),
        (all & !0x40000, 2, 25, R::Pacified),
        (0xc00000, 0, 7, R::Fleeing),
        (0x400000, 0, 5, R::Confused),
    ] {
        let mut f = AuraFixture::new(flags);
        let t = SpellTemplate {
            preventiontype: prevention,
            ..template()
        };
        f.add(ty, 0, 0);
        let mut s = f.spell(&t);
        assert_eq!(s.check_caster_auras(&t, &mut f).unwrap(), expected);
        assert_eq!(f.reads, [ty]);
    }
}
#[test]
fn prevention_type_mismatch_does_not_read_silence_or_pacify_auras() {
    let mut f = AuraFixture::new(0x22000);
    let t = template();
    let mut s = f.spell(&t);
    assert_eq!(s.check_caster_auras(&t, &mut f).unwrap(), R::Ok);
    assert!(f.reads.is_empty());
}
#[test]
fn empty_flagged_aura_list_cancels_and_continues_the_else_if_chain() {
    let mut f = AuraFixture::new(0x42000);
    let t = SpellTemplate {
        preventiontype: 1,
        ..template()
    };
    f.add(60, 0, 0);
    let mut s = f.spell(&t);
    assert_eq!(s.check_caster_auras(&t, &mut f).unwrap(), R::Silenced);
    assert_eq!(f.reads, [12, 27, 60]);
}
#[test]
fn charm_reads_three_types_in_order_and_control_flags_may_override_charm_failure() {
    let mut f = AuraFixture::new(0);
    let caster = f.power.caster;
    f.power
        .units
        .get_mut(&f.power.objects, caster)
        .unwrap()
        .charmer = Some(caster);
    let t = template();
    let mut s = f.spell(&t);
    s.client_cast = true;
    f.add(2, 0, 0);
    assert_eq!(s.check_caster_auras(&t, &mut f).unwrap(), R::Charmed);
    assert_eq!(f.reads, [6, 177, 2]);
    f.reads.clear();
    f.power
        .units
        .get_mut(&f.power.objects, caster)
        .unwrap()
        .flags = 0x40000;
    f.add(12, 0, 0);
    assert_eq!(s.check_caster_auras(&t, &mut f).unwrap(), R::Stunned);
    assert_eq!(f.reads, [6, 177, 2, 12]);
}
#[test]
fn client_without_charmer_never_reads_charm_aura_lists() {
    let mut f = AuraFixture::new(0);
    let t = template();
    let mut s = f.spell(&t);
    s.client_cast = true;
    assert_eq!(s.check_caster_auras(&t, &mut f).unwrap(), R::Ok);
    assert_eq!(f.power.trace, ["resources", "resources"]);
    assert!(f.reads.is_empty());
}
#[test]
fn mechanic_error_parameter_prefers_effect_then_spell_and_first_uncancelled_aura() {
    for (mechanic, effect, param) in [(3, 4, 4), (3, 0, 3)] {
        let mut f = AuraFixture::new(0x40000);
        let t = template();
        f.add(12, mechanic, effect);
        f.add(12, 8, 9);
        let mut s = f.spell(&t);
        assert_eq!(
            s.check_caster_auras(&t, &mut f).unwrap(),
            R::PreventedByMechanic
        );
        assert_eq!(s.param1, param);
    }
}
#[test]
fn immune_purge_uses_spell_definitions_and_skips_only_matching_auras() {
    let mut f = AuraFixture::new(0x40000);
    let t = SpellTemplate {
        attributesex: 0x8000,
        effect1: 6,
        effectapplyauraname1: 77,
        effectmiscvalue1: 3,
        ..template()
    };
    f.add(12, 3, 0);
    f.add(12, 8, 9);
    let mut s = f.spell(&t);
    assert_eq!(
        s.check_caster_auras(&t, &mut f).unwrap(),
        R::PreventedByMechanic
    );
    assert_eq!(s.param1, 9);
    f.auras.get_mut(&12).unwrap().pop();
    s.param1 = 88;
    assert_eq!(s.check_caster_auras(&t, &mut f).unwrap(), R::Ok);
    assert_eq!(s.param1, 88);
}
#[test]
fn immunity_helpers_follow_attribute_effect_and_school_guards() {
    let mut aura = SpellCasterAura {
        attributes: 0,
        attributesex2: 0,
        school: 2,
        dispel: 3,
        mechanic: 4,
        effect_mechanic: 5,
        aura_type: 12,
    };
    for (ty, misc, expected) in [
        (38, 12, true),
        (38, 11, false),
        (39, 4, true),
        (39, 2, false),
        (41, 3, true),
        (41, 4, false),
        (77, 4, true),
        (77, 5, true),
        (77, 6, false),
        (147, 4, false),
    ] {
        let mut t = SpellTemplate {
            attributesex: 0x8000,
            effect1: 6,
            effectapplyauraname1: ty,
            effectmiscvalue1: misc,
            ..template()
        };
        assert_eq!(spell_cancels_aura_effect(&t, &aura).unwrap(), expected);
        t.effect1 = 27;
        assert!(!spell_cancels_aura_effect(&t, &aura).unwrap());
    }
    let t = SpellTemplate {
        attributesex: 0x8000,
        effect3: 6,
        effectapplyauraname3: 39,
        effectmiscvalue3: 4,
        ..template()
    };
    aura.attributesex2 = 0x04000000;
    assert!(!spell_cancels_aura_effect(&t, &aura).unwrap());
    aura.attributesex2 = 0;
    aura.attributes = 0x20000000;
    assert!(!spell_cancels_aura_effect(&t, &aura).unwrap());
    aura.attributes = 0;
    let t = SpellTemplate {
        attributesex: 0,
        ..t
    };
    assert!(!spell_cancels_aura_effect(&t, &aura).unwrap());
}
#[test]
fn school_shift_refuses_only_reached_undefined_operation_and_accepts_bit31() {
    // SharedDefines.h:185 fixes the mask's underlying type to uint32.
    // Its :235 shift is defined at 31, undefined at 32, not an enum-range UB.
    let mut aura = SpellCasterAura {
        attributes: 0,
        attributesex2: 0,
        school: 31,
        dispel: 0,
        mechanic: 0,
        effect_mechanic: 0,
        aura_type: 12,
    };
    let t = SpellTemplate {
        attributesex: 0x8000,
        effect1: 6,
        effectapplyauraname1: 39,
        effectmiscvalue1: i32::MIN,
        ..template()
    };
    assert!(spell_cancels_aura_effect(&t, &aura).unwrap());
    aura.school = 32;
    assert_eq!(
        spell_cancels_aura_effect(&t, &aura).unwrap_err().id,
        "execution.spells.SpellCancelsAuraEffect_school_shift_undefined"
    );
    aura.attributesex2 = 0x04000000;
    assert!(!spell_cancels_aura_effect(&t, &aura).unwrap());
}
#[test]
fn unavailable_aura_list_refuses_instead_of_synthesizing_empty_or_failure() {
    let mut f = AuraFixture::new(0x40000);
    let t = template();
    let mut s = f.spell(&t);
    f.missing_type = Some(12);
    assert_eq!(
        s.check_caster_auras(&t, &mut f).unwrap_err().id,
        "authored.aura_list_missing"
    );
    assert_eq!(f.reads, [12]);
    assert_eq!(s.param1, 0);
}
#[test]
fn default_aura_owner_preserves_missing_native_flags_and_list_boundaries() {
    struct Native(super::power_tests::PowerFixture);
    impl SpellExecutionOwner for Native {
        fn power_objects(&mut self) -> Result<&SourceObjects> {
            Ok(&self.0.objects)
        }
        fn power_context(&mut self) -> Result<SpellPowerContext<'_>> {
            self.0.power_context()
        }
    }
    let t = template();
    let mut f = Native(super::power_tests::PowerFixture::new());
    let mut s = f.0.spell(&t);
    f.0.units.get_mut(&f.0.objects, f.0.caster).unwrap().flags = 0x40000;
    assert_eq!(
        s.check_caster_auras(&t, &mut f).unwrap_err().id,
        "execution.spells.Unit_GetAurasByType_CheckCasterAuras_owner"
    );
}
