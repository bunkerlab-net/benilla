// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

fn proc_row() -> SpellProc {
    SpellProc {
        school_mask: 0,
        family: 0,
        family_masks: [0; 3],
        flags: 0,
        extra: 0,
        ppm: SpellFloat::default(),
        chance: SpellFloat::default(),
        cooldown: 0,
    }
}

#[test]
fn proc_predicate_checks_overlap_periodic_sign_school_family_and_extra() {
    use ProcPredicate::*;
    let spell = SpellTemplate {
        school: 2,
        spellfamilyname: 5,
        ..Default::default()
    };
    let generic = SpellTemplate::default();
    let high_school = SpellTemplate {
        school: 31,
        ..Default::default()
    };
    let bad_school = SpellTemplate {
        school: 32,
        ..Default::default()
    };
    let s = Some(&spell);
    let row = proc_row();
    let fire = SpellProc {
        school_mask: 4,
        ..row
    };
    let normal = SpellProc {
        school_mask: 1,
        ..row
    };
    let family = SpellProc {
        family: 5,
        family_masks: [u64::MAX; 3],
        ..row
    };
    let mask31 = SpellProc {
        school_mask: 1 << 31,
        ..row
    };
    let miss = SpellProc { extra: 4, ..row };
    let no_damage = SpellProc {
        extra: TRIGGER_ON_NO_DAMAGE,
        ..row
    };
    let deal = DEAL_HARMFUL_PERIODIC;
    let take = TAKE_HARMFUL_PERIODIC;
    let hot = NORMAL_HIT | INTERNAL_HOT;
    let hit = NORMAL_HIT;
    let swing = 4;
    let cases = [
        (
            "no overlap before always",
            Some(&fire),
            HEARTBEAT,
            None,
            swing,
            0,
            NoOverlap,
        ),
        (
            "heartbeat bypasses later checks",
            Some(&fire),
            HEARTBEAT,
            None,
            HEARTBEAT,
            0,
            Allowed,
        ),
        (
            "kill bypasses later checks",
            Some(&fire),
            KILL,
            None,
            KILL,
            0,
            Allowed,
        ),
        (
            "trap bypasses later checks",
            Some(&fire),
            ON_TRAP_ACTIVATION,
            None,
            ON_TRAP_ACTIVATION,
            0,
            Allowed,
        ),
        (
            "deal HOT accepts nonzero family",
            None,
            deal,
            s,
            deal,
            hot,
            Allowed,
        ),
        (
            "deal HOT rejects generic family",
            None,
            deal,
            Some(&generic),
            deal,
            hot,
            Rejected,
        ),
        (
            "deal HOT accepts helpful spell",
            None,
            deal | DEAL_HELPFUL_SPELL,
            None,
            deal,
            hot,
            Allowed,
        ),
        (
            "deal HOT accepts helpful ability",
            None,
            deal | DEAL_HELPFUL_ABILITY,
            s,
            deal,
            hot,
            Allowed,
        ),
        (
            "deal HOT rejects wrong sign",
            None,
            deal | DEAL_HARMFUL_SPELL,
            s,
            deal,
            hot,
            Rejected,
        ),
        (
            "deal DOT accepts neutral alone",
            None,
            deal,
            None,
            deal,
            hit,
            Allowed,
        ),
        (
            "deal DOT accepts harmful spell",
            None,
            deal | DEAL_HARMFUL_SPELL,
            s,
            deal,
            hit,
            Allowed,
        ),
        (
            "deal DOT accepts harmful ability",
            None,
            deal | DEAL_HARMFUL_ABILITY,
            s,
            deal,
            hit,
            Allowed,
        ),
        (
            "deal DOT rejects wrong sign",
            None,
            deal | DEAL_HELPFUL_SPELL,
            s,
            deal,
            hit,
            Rejected,
        ),
        (
            "take HOT rejects neutral alone",
            None,
            take,
            s,
            take,
            hot,
            Rejected,
        ),
        (
            "take HOT accepts helpful spell",
            None,
            take | TAKE_HELPFUL_SPELL,
            s,
            take,
            hot,
            Allowed,
        ),
        (
            "take HOT accepts helpful ability",
            None,
            take | TAKE_HELPFUL_ABILITY,
            s,
            take,
            hot,
            Allowed,
        ),
        (
            "take HOT rejects wrong sign",
            None,
            take | TAKE_HARMFUL_SPELL,
            s,
            take,
            hot,
            Rejected,
        ),
        (
            "take DOT accepts neutral alone",
            None,
            take,
            s,
            take,
            hit,
            Allowed,
        ),
        (
            "take DOT accepts harmful spell",
            None,
            take | TAKE_HARMFUL_SPELL,
            s,
            take,
            hit,
            Allowed,
        ),
        (
            "take DOT accepts harmful ability",
            None,
            take | TAKE_HARMFUL_ABILITY,
            s,
            take,
            hit,
            Allowed,
        ),
        (
            "take DOT rejects wrong sign",
            None,
            take | TAKE_HELPFUL_SPELL,
            s,
            take,
            hit,
            Rejected,
        ),
        (
            "requires periodic flag on both",
            None,
            deal | take | swing,
            None,
            swing,
            hot,
            Allowed,
        ),
        (
            "requires periodic flag on event",
            None,
            swing,
            None,
            deal | take | swing,
            hot,
            Allowed,
        ),
        (
            "melee ignores family and mask zero",
            Some(&family),
            swing,
            None,
            swing,
            hit,
            Allowed,
        ),
        (
            "melee normal school",
            Some(&normal),
            swing,
            None,
            swing,
            hit,
            Allowed,
        ),
        (
            "melee wrong school",
            Some(&fire),
            swing,
            None,
            swing,
            hit,
            Rejected,
        ),
        (
            "spell same school",
            Some(&fire),
            swing,
            s,
            swing,
            hit,
            Allowed,
        ),
        (
            "spell different school",
            Some(&normal),
            swing,
            s,
            swing,
            hit,
            Rejected,
        ),
        (
            "spell school shift 31",
            Some(&mask31),
            swing,
            Some(&high_school),
            swing,
            hit,
            Allowed,
        ),
        (
            "zero school mask skips shift",
            Some(&row),
            swing,
            Some(&bad_school),
            swing,
            hit,
            Allowed,
        ),
        (
            "spell same family, mask deferred",
            Some(&family),
            swing,
            s,
            swing,
            hit,
            Allowed,
        ),
        (
            "spell different family",
            Some(&family),
            swing,
            Some(&generic),
            swing,
            hit,
            Rejected,
        ),
        (
            "default allows crit",
            None,
            swing,
            s,
            swing,
            CRITICAL_HIT,
            Allowed,
        ),
        (
            "default allows cast end",
            None,
            swing,
            s,
            swing,
            CAST_END,
            Allowed,
        ),
        ("default rejects miss", None, swing, s, swing, 4, Rejected),
        (
            "custom no damage",
            Some(&no_damage),
            swing,
            s,
            swing,
            0,
            Allowed,
        ),
        ("custom match", Some(&miss), swing, s, swing, 4, Allowed),
        (
            "custom mismatch",
            Some(&miss),
            swing,
            s,
            swing,
            hit,
            Rejected,
        ),
    ];
    for (name, row, event_flags, spell, flags, extra, expected) in cases {
        assert_eq!(
            is_spell_proc_event_can_triggered_by(row, event_flags, spell, flags, extra).unwrap(),
            expected,
            "{name}"
        );
    }
    assert_eq!(
        is_spell_proc_event_can_triggered_by(None, deal, None, deal, hot)
            .unwrap_err()
            .id,
        "execution.spells.SpellMgr_863.hot_null_spell_undefined",
    );
    assert_eq!(
        is_spell_proc_event_can_triggered_by(Some(&mask31), swing, Some(&bad_school), swing, hit)
            .unwrap_err()
            .id,
        "execution.spells.SpellMgr_908.school_shift_undefined",
    );
}

#[test]
fn proc_event_nonzero_flags_replace_template_and_zero_flags_keep_it() {
    let aura = SpellTemplate {
        procflags: 0x14,
        ..Default::default()
    };
    let row = proc_row();
    assert_eq!(proc_event_flags(None, &aura), 0x14);
    assert_eq!(proc_event_flags(Some(&row), &aura), 0x14);
    assert_eq!(
        proc_event_flags(
            Some(&SpellProc {
                flags: 0x400,
                ..row
            }),
            &aura
        ),
        0x400
    );
}

#[test]
fn proc_trigger_excludes_self_cast_unless_periodic_and_checks_player_kill_first() {
    let stores = SourceSpellStores::default();
    let objects = SourceObjects::default();
    let mut aura = SpellTemplate {
        id: 18943,
        procflags: 0x14,
        ..Default::default()
    };
    let target = ObjectHandle {
        thread: 6,
        construction: 1,
    };
    let holder = SourceAuraHolder::new(&aura, target, 0, 0, -1).unwrap();
    let unit = ProcUnit {
        objects: &objects,
        stand: 0,
        player: false,
        victim: false,
    };
    let check = |aura: &SpellTemplate, flags, extra, unit| {
        is_triggered_at_spell_proc_event(&stores, aura, &holder, Some(aura), flags, extra, unit)
            .unwrap()
    };
    assert_eq!(
        check(&aura, 0x10, CAST_END, unit),
        ProcTriggerExit::SelfTrigger
    );
    aura.procflags |= TAKE_HARMFUL_PERIODIC;
    assert_eq!(
        check(&aura, 0x10, CAST_END, unit).refusal().unwrap().id,
        "execution.spells.UnitAuraProcHandler_592.proc_chance_owner"
    );
    aura.procflags = TAKE_HARMFUL_PERIODIC;
    assert_eq!(
        check(&aura, TAKE_HARMFUL_PERIODIC, NORMAL_HIT, unit)
            .refusal()
            .unwrap()
            .id,
        "execution.spells.UnitAuraProcHandler_592.proc_chance_owner"
    );
    aura.procflags = KILL;
    assert_eq!(
        check(&aura, KILL, CAST_END, unit),
        ProcTriggerExit::SelfTrigger
    );
    let player = check(
        &aura,
        KILL,
        CAST_END,
        ProcUnit {
            player: true,
            ..unit
        },
    );
    assert_eq!(player, ProcTriggerExit::KillPlayer);
    assert_eq!(
        player.refusal().unwrap().id,
        "execution.spells.UnitAuraProcHandler_538.isHonorOrXPTarget_owner"
    );
}

#[test]
fn proc_trigger_checks_equipment_outdoors_caster_and_sitting_before_chance() {
    let stores = SourceSpellStores::default();
    let mut objects = SourceObjects::default();
    let target = objects.construct(6, SourceObjectKind::Creature).unwrap();
    let caster = objects.construct(6, SourceObjectKind::Creature).unwrap();
    objects.get_mut(target).unwrap().guid = Some(SourceGuid::from_raw(1));
    objects.get_mut(caster).unwrap().guid = Some(SourceGuid::from_raw(2));
    let mut aura = SpellTemplate {
        id: 18943,
        procflags: 0x14,
        ..Default::default()
    };
    let mut holder = SourceAuraHolder::new(&aura, target, 0, 0, -1).unwrap();
    holder.caster = caster;
    let unit = ProcUnit {
        objects: &objects,
        stand: 1,
        player: false,
        victim: false,
    };
    let check = |aura: &SpellTemplate, holder: &SourceAuraHolder, unit| {
        is_triggered_at_spell_proc_event(&stores, aura, holder, None, 4, NORMAL_HIT, unit).unwrap()
    };
    aura.attributesex3 = ONLY_PROC_OUTDOORS | ONLY_PROC_ON_CASTER;
    let player = ProcUnit {
        player: true,
        ..unit
    };
    assert_eq!(
        check(&aura, &holder, player).refusal().unwrap().id,
        "execution.spells.UnitAuraProcHandler_548.proc_equipment_owner"
    );
    for actor in [
        unit,
        ProcUnit {
            victim: true,
            ..player
        },
    ] {
        assert_eq!(
            check(&aura, &holder, actor).refusal().unwrap().id,
            "execution.spells.UnitAuraProcHandler_582.IsOutdoors_owner"
        );
    }
    aura.attributesex3 = ONLY_PROC_ON_CASTER | NO_PROC_EQUIP_REQUIREMENT;
    assert_eq!(check(&aura, &holder, player), ProcTriggerExit::OtherCaster);
    holder.caster = target;
    for stand in [1, 2, 4, 5, 6] {
        assert_eq!(
            check(&aura, &holder, ProcUnit { stand, ..unit }),
            ProcTriggerExit::Sitting
        );
    }
    for stand in [0, 3, 7, 8, 9] {
        assert_eq!(
            check(&aura, &holder, ProcUnit { stand, ..unit })
                .refusal()
                .unwrap()
                .id,
            "execution.spells.UnitAuraProcHandler_592.proc_chance_owner"
        );
    }
    aura.attributes = ALLOW_WHILE_SITTING;
    assert_eq!(
        check(&aura, &holder, unit).refusal().unwrap().id,
        "execution.spells.UnitAuraProcHandler_592.proc_chance_owner"
    );
}
