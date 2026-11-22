use super::threat_tests::{add, handle, setup, value, Host};
use super::*;
impl CombatExecutionOwner for Host {
    fn combat_unit(&mut self, unit: ObjectHandle) -> Result<CombatUnit> {
        self.combat
            .get(&unit)
            .copied()
            .ok_or_else(|| refuse("authored.stale_combat_generation"))
    }
    fn combat_call(&mut self, call: CombatCall, damage: &mut SourceDamage) -> Result<()> {
        self.log.push(format!("{call:?} {}", damage.amount));
        match call {
            CombatCall::DamageDeal => {
                if let Some(amount) = self.dealt_damage {
                    damage.amount = amount;
                }
            }
            CombatCall::DamageTaken => {
                if let Some(amount) = self.taken_damage {
                    damage.amount = amount;
                }
            }
            CombatCall::JustPreventedDeath => {
                if let Some(amount) = self.prevented_damage {
                    damage.amount = amount;
                }
            }
            CombatCall::Kill => {
                if self.reject_kill {
                    return Err(refuse("execution.combat.f5_04b_kill_owner"));
                }
                self.combat.get_mut(&damage.victim).unwrap().health = 0;
            }
            _ => {}
        }
        Ok(())
    }
    fn modify_health(&mut self, unit: ObjectHandle, delta: i32) -> Result<()> {
        self.log.push(format!("health {delta}"));
        let health = &mut self.combat.get_mut(&unit).unwrap().health;
        *health = health.saturating_add_signed(delta);
        Ok(())
    }
    fn damage_threat_multiplier(&mut self, _spell: Option<ThreatSpell>) -> Result<ThreatValue> {
        Ok(value(1))
    }
    fn set_in_combat(&mut self, unit: ObjectHandle, enemy: ObjectHandle) -> Result<()> {
        self.log.push(format!(
            "combat {} {}",
            unit.construction, enemy.construction
        ));
        Ok(())
    }
}
fn damage(
    dealer: ObjectHandle,
    victim: ObjectHandle,
    kind: SourceDamageType,
    amount: u32,
) -> SourceDamage {
    SourceDamage {
        dealer: Some(dealer),
        victim,
        amount,
        kind,
        school: 1,
        spell: None,
        clean_damage: None,
        travel_started_ms: None,
        durability_loss: false,
    }
}
#[test]
fn npc_damage_causes_entry_and_bilateral_combat_without_player_shortcut() {
    let (mut threats, mut host, victim, dealer, _) = setup();
    let mut combat = SourceCombat::default();
    assert_eq!(
        combat
            .deal_damage(
                &mut damage(dealer, victim, SourceDamageType::Direct, 12),
                &mut threats,
                &mut host
            )
            .unwrap(),
        12
    );
    assert_eq!(
        threats.manager(victim).unwrap().threat(2, false).unwrap(),
        value(12)
    );
    assert_eq!(host.combat[&victim].health, 88);
    assert_eq!(
        host.log,
        vec![
            "DealDamagePrelude 12",
            "DealDamageNonzeroPrelude 12",
            "health -12",
            "combat 2 1",
            "combat 1 2",
            "timer 2 1",
            "DamageDealtSuffix 12"
        ]
    );
    assert_eq!(host.linking, vec![(victim, dealer)]);
}
#[test]
fn dot_and_shield_still_add_threat_but_skip_bilateral_combat_and_timer() {
    for kind in [SourceDamageType::Dot, SourceDamageType::Shield] {
        let (mut threats, mut host, victim, dealer, _) = setup();
        SourceCombat::default()
            .deal_damage(
                &mut damage(dealer, victim, kind, 9),
                &mut threats,
                &mut host,
            )
            .unwrap();
        assert_eq!(
            threats.manager(victim).unwrap().threat(2, false).unwrap(),
            value(9)
        );
        assert!(!host
            .log
            .iter()
            .any(|s| s.starts_with("combat") || s.starts_with("timer")));
    }
}
#[test]
fn zero_and_lethal_damage_do_not_create_nonthreatening_alive_branch_entries() {
    for amount in [0, 100, 101] {
        let (mut threats, mut host, victim, dealer, _) = setup();
        SourceCombat::default()
            .deal_damage(
                &mut damage(dealer, victim, SourceDamageType::Direct, amount),
                &mut threats,
                &mut host,
            )
            .unwrap();
        assert!(threats.manager(victim).unwrap().online().is_empty());
        assert_eq!(host.log.len(), if amount == 0 { 1 } else { 3 });
    }
}
#[test]
fn mutable_damage_callbacks_precede_absorb_accounting_in_source_order() {
    let (_, mut host, victim, dealer, _) = setup();
    host.dealt_damage = Some(30);
    host.taken_damage = Some(4);
    let mut d = damage(dealer, victim, SourceDamageType::Direct, 20);
    let mut absorb = 2;
    SourceCombat::default()
        .damage_mods(&mut d, Some(&mut absorb), &mut host)
        .unwrap();
    assert_eq!(d.amount, 4);
    assert_eq!(absorb, 18);
    assert_eq!(host.log, vec!["DamageDeal 20", "DamageTaken 30"]);
    host.taken_damage = Some(50);
    SourceCombat::default()
        .damage_mods(&mut d, Some(&mut absorb), &mut host)
        .unwrap();
    assert_eq!(absorb, 18);
}
#[test]
fn death_prevention_latches_once_mutable_callback_does_not_itself_clamp_damage() {
    let (_, mut host, victim, dealer, _) = setup();
    host.combat.get_mut(&victim).unwrap().unkillable = true;
    let mut combat = SourceCombat::default();
    let mut d = damage(dealer, victim, SourceDamageType::Direct, 100);
    host.prevented_damage = Some(3);
    combat.creature_damage_taken(&mut d, &mut host).unwrap();
    assert_eq!(d.amount, 3);
    assert!(combat.death_prevented(victim));
    d.amount = 101;
    combat.creature_damage_taken(&mut d, &mut host).unwrap();
    assert_eq!(d.amount, 101);
    assert_eq!(host.log.len(), 1);
    combat.reset_death_prevented(victim);
    d.kind = SourceDamageType::Instakill;
    combat.creature_damage_taken(&mut d, &mut host).unwrap();
    assert!(!combat.death_prevented(victim));
    d.kind = SourceDamageType::Direct;
    d.amount = 99;
    combat.creature_damage_taken(&mut d, &mut host).unwrap();
    assert!(!combat.death_prevented(victim));
}
#[test]
fn preventing_death_clamps_only_at_dealdamage_and_threat_uses_reduced_amount() {
    let (mut threats, mut host, victim, dealer, _) = setup();
    host.combat.get_mut(&victim).unwrap().preventing_death = true;
    let mut combat = SourceCombat::default();
    let mut d = damage(dealer, victim, SourceDamageType::Direct, 150);
    assert_eq!(
        combat.deal_damage(&mut d, &mut threats, &mut host).unwrap(),
        99
    );
    assert_eq!(host.combat[&victim].health, 1);
    assert_eq!(
        threats.manager(victim).unwrap().threat(2, false).unwrap(),
        value(99)
    );
    assert_eq!(
        combat.deal_damage(&mut d, &mut threats, &mut host).unwrap(),
        0
    );
    assert_eq!(host.combat[&victim].health, 1);
    d.kind = SourceDamageType::Instakill;
    d.amount = 1;
    combat.deal_damage(&mut d, &mut threats, &mut host).unwrap();
    assert_eq!(host.combat[&victim].health, 0);
}
#[test]
fn dead_taxi_and_evade_mods_zero_damage_before_callbacks_and_wrap_absorb() {
    for mode in 0..3 {
        let (_, mut host, victim, dealer, _) = setup();
        let unit = host.combat.get_mut(&victim).unwrap();
        match mode {
            0 => unit.alive = false,
            1 => unit.taxi = true,
            _ => unit.evade = true,
        }
        let mut d = damage(dealer, victim, SourceDamageType::Direct, 5);
        let mut absorb = u32::MAX;
        SourceCombat::default()
            .damage_mods(&mut d, Some(&mut absorb), &mut host)
            .unwrap();
        assert_eq!(d.amount, 0);
        assert_eq!(absorb, 4);
        assert!(host.log.is_empty());
    }
}
#[test]
fn linking_suicide_bypasses_damage_mods_and_prevention_but_not_missing_kill_owner() {
    let (mut threats, mut host, victim, _, _) = setup();
    host.combat.get_mut(&victim).unwrap().preventing_death = true;
    host.combat.get_mut(&victim).unwrap().unkillable = true;
    SourceCombat::default()
        .linking_action(&mut threats, victim, LinkingAction::Suicide, &mut host)
        .unwrap();
    assert_eq!(host.combat[&victim].health, 0);
    assert_eq!(
        host.log,
        vec![
            "DealDamagePrelude 100",
            "DealDamageNonzeroPrelude 100",
            "Kill 100"
        ]
    );
    host.combat.get_mut(&victim).unwrap().health = 100;
    host.reject_kill = true;
    assert_eq!(
        SourceCombat::default()
            .suicide(victim, &mut threats, &mut host)
            .unwrap_err()
            .id,
        "execution.combat.f5_04b_kill_owner"
    );
    assert_eq!(host.combat[&victim].health, 100);
}
#[test]
fn linking_threat_is_bilateral_for_npcs_and_motion_keeps_its_refusal() {
    let (mut threats, mut host, a, b, _) = setup();
    threats.register(b).unwrap();
    let mut combat = SourceCombat::default();
    combat
        .linking_action(&mut threats, a, LinkingAction::AddThreat(b), &mut host)
        .unwrap();
    combat
        .linking_action(&mut threats, b, LinkingAction::AddThreat(a), &mut host)
        .unwrap();
    assert_eq!(threats.hated_by(a), &[b]);
    assert_eq!(threats.hated_by(b), &[a]);
    assert_eq!(
        combat
            .linking_action(&mut threats, a, LinkingAction::Follow(b), &mut host)
            .unwrap_err()
            .id,
        "execution.linking.f5_motion_follow_owner"
    );
}
#[test]
fn undefined_negation_refuses_at_modifyhealth_with_defined_neighbours() {
    let (mut threats, mut host, victim, dealer, _) = setup();
    let mut combat = SourceCombat::default();
    let mut d = damage(dealer, victim, SourceDamageType::Direct, 0x80000000);
    assert_eq!(
        combat
            .handle_damage_dealt(&mut d, &mut threats, &mut host)
            .unwrap_err()
            .id,
        "execution.combat.negate_int_min_damage_undefined"
    );
    assert!(host.log.is_empty());
    d.dealer = None;
    d.amount = 0x7fffffff;
    combat
        .handle_damage_dealt(&mut d, &mut threats, &mut host)
        .unwrap();
    assert_eq!(host.log[0], "health -2147483647");
    host.log.clear();
    d.amount = 0x80000001;
    combat
        .handle_damage_dealt(&mut d, &mut threats, &mut host)
        .unwrap();
    assert_eq!(host.log[0], "health 2147483647");
}
#[test]
fn no_threat_spell_suppresses_threat_and_combat_but_not_health() {
    let (mut threats, mut host, victim, dealer, _) = setup();
    let mut d = damage(dealer, victim, SourceDamageType::Direct, 5);
    d.spell = Some(ThreatSpell {
        id: 5,
        no_threat: true,
        no_harmful_threat: false,
    });
    SourceCombat::default()
        .deal_damage(&mut d, &mut threats, &mut host)
        .unwrap();
    assert_eq!(host.combat[&victim].health, 95);
    assert!(threats.manager(victim).unwrap().online().is_empty());
    assert_eq!(host.log.len(), 4);
}
#[test]
fn health_zero_prevention_wrap_is_unsigned_not_undefined() {
    let (mut threats, mut host, victim, dealer, _) = setup();
    let unit = host.combat.get_mut(&victim).unwrap();
    unit.health = 0;
    unit.preventing_death = true;
    let mut d = damage(dealer, victim, SourceDamageType::Direct, 1);
    assert_eq!(
        SourceCombat::default()
            .deal_damage(&mut d, &mut threats, &mut host)
            .unwrap(),
        u32::MAX
    );
    assert!(host.log.iter().any(|s| s == "Kill 4294967295"));
}
#[test]
fn clear_preserves_dirty_bit_and_does_not_leave_reverse_links() {
    let (mut threats, mut host, s, a, _) = setup();
    add(&mut threats, &mut host, s, a, 1);
    threats.select(s, &mut host).unwrap();
    assert!(!threats.manager(s).unwrap().dirty());
    threats.clear(s).unwrap();
    assert!(!threats.manager(s).unwrap().dirty());
    assert!(threats.hated_by(a).is_empty());
    assert!(threats.manager(handle(999)).is_err());
}

struct LiveFixture {
    objects: SourceObjects,
    units: SourceUnits,
    threats: SourceThreats,
    combat: SourceCombat,
    spells: SourceSpellStores,
    mage: player::SourcePlayer,
    victim: ObjectHandle,
}

impl LiveFixture {
    fn vermin() -> Self {
        let map = MapIdentity {
            map: 0,
            instance: 0,
            creation_generation: 1,
        };
        let cells = SourceCells::new(map).unwrap();
        let mut objects = SourceObjects::default();
        let mut units = SourceUnits::default();
        let mage = player::SourcePlayer::construct(
            &mut objects,
            &mut units,
            9,
            900_000,
            |min, _| Ok(min),
            || Ok(1),
        )
        .unwrap();
        let dealer = mage.object();
        objects
            .assign_guid(dealer, 0, SourceGuid::from_raw(1))
            .unwrap();
        objects.bind_map(dealer, map, &cells).unwrap();
        objects.add_to_world(dealer).unwrap();
        objects.register_player_lookup(dealer).unwrap();
        let fields = units.get_mut(&objects, dealer).unwrap();
        // northshire-03/a/run/trace.tsv:76108095. The live dealer is a Human Mage.
        fields.power_type = Some(0);
        fields.flags |= 8;

        // pre-start-classicmangos.sql:125149,421114. Entry 6, spawn 79994.
        let victim = objects.construct(9, SourceObjectKind::Creature).unwrap();
        let guid = SourceGuid::with_entry(HighGuid::Creature, 6, 79994);
        objects.assign_guid(victim, 79994, guid).unwrap();
        objects.bind_map(victim, map, &cells).unwrap();
        objects.add_to_world(victim).unwrap();
        let mut fields = SourceUnitFields::constructed_creature();
        fields.entry = 6;
        fields.faction = 25;
        fields.pursuit_ms = 15000;
        // northshire-03/a/run/trace.tsv:76018750. Selected level 2, 55/55 HP.
        fields.vitals = Some(SourceUnitVitals {
            health: 55,
            max_health: 55,
            mana: 0,
            max_mana: 0,
            has_mana: false,
            energy: 100,
            max_energy: 100,
        });
        units.register(&objects, victim, fields).unwrap();
        let mut spells = SourceSpellStores::default();
        // pre-start-classicmangos.sql:837896. Rank 1 direct hit and periodic aura.
        spells
            .insert_template(SpellTemplate {
                id: 133,
                school: 2,
                interruptflags: 15,
                durationindex: 35,
                effect1: 2,
                effect2: 6,
                effectbasedice2: 1,
                effectimplicittargeta1: 6,
                effectimplicittargeta2: 6,
                effectapplyauraname2: 3,
                effectamplitude2: 2000,
                spellfamilyname: 3,
                ..Default::default()
            })
            .unwrap();
        let mut threats = SourceThreats::default();
        threats.register(dealer).unwrap();
        threats.register(victim).unwrap();
        Self {
            objects,
            units,
            threats,
            combat: SourceCombat::default(),
            spells,
            mage,
            victim,
        }
    }

    fn hit(&mut self, mut damage: SourceDamage) -> Result<u32> {
        self.combat.deal_damage(
            &mut damage,
            &mut self.threats,
            &mut SourceLiveDamage {
                objects: &self.objects,
                units: &mut self.units,
                spells: &self.spells,
                players: &[&self.mage],
            },
        )
    }

    fn fireball(&self) -> SourceDamage {
        // northshire-03/a/run/trace.tsv:76057589. The landed input is 22 fire damage.
        let mut hit = damage(
            self.mage.object(),
            self.victim,
            SourceDamageType::SpellDirect,
            22,
        );
        hit.school = 1 << self.spells.template(133).unwrap().school;
        hit.spell = Some(ThreatSpell {
            id: 133,
            ..Default::default()
        });
        hit.clean_damage = Some(SourceCleanDamage {
            damage: hit.amount,
            attack: SourceDamageAttack::MainHand,
            outcome: SourceDamageOutcome::Normal,
            taken_or_absorbed: true,
        });
        hit.travel_started_ms = Some(1);
        hit.durability_loss = true;
        hit
    }

    fn victim(&self) -> &SourceUnitFields {
        self.units.get(&self.objects, self.victim).unwrap()
    }

    fn victim_mut(&mut self) -> &mut SourceUnitFields {
        self.units.get_mut(&self.objects, self.victim).unwrap()
    }
}

#[test]
fn live_fireball_taps_and_changes_the_health_field_before_attackability() {
    let mut f = LiveFixture::vermin();
    let dealer = f.mage.object();
    let template = f.spells.template(133).unwrap();
    let mut holder = SourceAuraHolder::new(template, f.victim, 0, 0, 8000).unwrap();
    holder.caster = dealer;
    holder.effects[1] = Some(SourceAura::self_spell_effect(template, 1, 3, 0, 0, 1, 0).unwrap());
    f.victim_mut()
        .auras
        .store_aura_holder(133, &mut Some(holder))
        .unwrap();
    let hit = f.fireball();
    assert_eq!(
        f.hit(hit).unwrap_err().id,
        "execution.combat.Unit_1211.CanAttack_owner"
    );
    let fields = f.victim();
    let object = f.objects.get(f.victim).unwrap();
    // northshire-03/a/run/trace.tsv:76080344,76108095. Health is absolute in the live Unit.
    assert_eq!(fields.vitals.unwrap().health, 33);
    assert_eq!(fields.loot_recipient_guid.raw(), 1);
    assert_eq!(fields.loot_group_recipient_id, 0);
    assert_eq!(fields.damage_by_others, 0);
    assert!(fields.auras.first_aura_holder(133).is_some());
    assert!(object.field_changed(UNIT_FIELD_HEALTH).unwrap());
    assert!(object.field_changed(143).unwrap());
    assert!(object.alive);
    assert!(f.threats.manager(f.victim).unwrap().online().is_empty());
}

#[test]
fn live_damage_keeps_first_tap_and_counts_other_damage() {
    let mut f = LiveFixture::vermin();
    let hit = f.fireball();
    f.hit(hit).unwrap_err();
    let mut other = hit;
    other.dealer = Some(f.victim);
    other.amount = 2;
    assert_eq!(
        f.hit(other).unwrap_err().id,
        "execution.combat.Unit_1211.CanAttack_owner"
    );
    assert_eq!(f.victim().loot_recipient_guid.raw(), 1);
    assert_eq!(f.victim().damage_by_others, 2);
    assert_eq!(f.victim().vitals.unwrap().health, 31);

    f.objects.remove_player_lookup(f.mage.object()).unwrap();
    assert_eq!(
        f.hit(other).unwrap_err().id,
        "execution.combat.Creature_1103.FindPlayer_owner"
    );
    assert_eq!(f.victim().damage_by_others, 2);
    assert_eq!(f.victim().vitals.unwrap().health, 31);
}

#[test]
fn live_damage_excludes_untapped_npc_and_multitap_hits() {
    for (dealer_is_player, multitap) in [(false, false), (true, true)] {
        let mut f = LiveFixture::vermin();
        if multitap {
            f.victim_mut().static_flags[2] |= 0x0080_0000; // CreatureDefines.h:128.
        }
        let mut hit = f.fireball();
        if !dealer_is_player {
            hit.dealer = Some(f.victim);
        }
        assert_eq!(
            f.hit(hit).unwrap_err().id,
            "execution.combat.Unit_1211.CanAttack_owner"
        );
        assert_eq!(f.victim().loot_recipient_guid.raw(), 0);
        assert!(!f.objects.get(f.victim).unwrap().field_changed(143).unwrap());
        assert_eq!(f.victim().vitals.unwrap().health, 33);
    }
}

#[test]
fn live_group_tap_refuses_at_the_group_read_after_writing_the_guid() {
    let mut f = LiveFixture::vermin();
    f.mage.group = Some(7);
    let hit = f.fireball();
    assert_eq!(
        f.hit(hit).unwrap_err().id,
        "execution.combat.Creature_1176.loot_group_owner"
    );
    assert_eq!(f.victim().loot_recipient_guid.raw(), 1);
    assert_eq!(f.victim().vitals.unwrap().health, 55);
    assert!(!f.objects.get(f.victim).unwrap().field_changed(143).unwrap());

    f.victim_mut().loot_group_recipient_id = 7;
    assert_eq!(
        f.hit(hit).unwrap_err().id,
        "execution.combat.Creature_1112.GetGroupById_owner"
    );
    assert_eq!(f.victim().vitals.unwrap().health, 55);
}

#[test]
fn live_no_dealer_damage_leaves_tap_untouched_and_keeps_death_dispatch_separate() {
    let mut f = LiveFixture::vermin();
    let mut hit = f.fireball();
    hit.dealer = None;
    hit.kind = SourceDamageType::Dot;
    assert_eq!(f.hit(hit).unwrap(), 22);
    assert_eq!(f.victim().vitals.unwrap().health, 33);
    assert_eq!(f.victim().loot_recipient_guid.raw(), 0);
    assert!(!f.objects.get(f.victim).unwrap().field_changed(143).unwrap());

    hit.amount = 33;
    assert_eq!(
        f.hit(hit).unwrap_err().id,
        "execution.combat.Unit_1010.Kill_owner"
    );
    assert_eq!(f.victim().vitals.unwrap().health, 33);
    f.victim_mut().static_flags[0] |= 8; // Creature.cpp:3006-3009.
    assert_eq!(f.hit(hit).unwrap(), 32);
    assert_eq!(f.victim().vitals.unwrap().health, 1);
    assert!(f.objects.get(f.victim).unwrap().alive);
    assert!(f.threats.manager(f.victim).unwrap().online().is_empty());
}

#[test]
fn live_zero_hit_stands_ordinary_npc_and_leaves_stunned_or_periodic_npc_down() {
    for (kind, flags, stand) in [
        (SourceDamageType::Direct, 0, 0),
        (SourceDamageType::Direct, 0x0004_0000, 1),
        (SourceDamageType::Dot, 0, 1),
        (SourceDamageType::Instakill, 0, 1),
    ] {
        let mut f = LiveFixture::vermin();
        let fields = f.victim_mut();
        fields.stand_state = 1;
        fields.flags = flags;
        let mut hit = f.fireball();
        hit.kind = kind;
        hit.amount = 0;
        assert_eq!(f.hit(hit).unwrap(), 0);
        assert_eq!(f.victim().stand_state, stand);
        assert_eq!(f.victim().vitals.unwrap().health, 55);
        assert_eq!(f.victim().loot_recipient_guid.raw(), 0);
        let object = f.objects.get(f.victim).unwrap();
        assert_eq!(
            object.field_changed(UNIT_FIELD_BYTES_1).unwrap(),
            stand == 0
        );
        assert!(!object.field_changed(UNIT_FIELD_HEALTH).unwrap());
    }
}

#[test]
fn live_absorbed_direct_hit_keeps_newer_aura_but_refuses_old_interrupting_aura() {
    let mut f = LiveFixture::vermin();
    let template = SpellTemplate {
        id: 70000,
        aurainterruptflags: 0x0100_0000, // SpellDefines.h:386.
        ..Default::default()
    };
    let mut holder = SourceAuraHolder::new(&template, f.victim, 0, 20, -1).unwrap();
    holder.effects[0] = Some(SourceAura::self_spell_effect(&template, 0, 3, 0, 0, 1, 0).unwrap());
    f.spells.insert_template(template).unwrap();
    f.victim_mut()
        .auras
        .store_aura_holder(70000, &mut Some(holder))
        .unwrap();
    let mut hit = f.fireball();
    hit.amount = 0;
    hit.travel_started_ms = Some(20);
    assert_eq!(f.hit(hit).unwrap(), 0);
    hit.travel_started_ms = Some(21);
    hit.kind = SourceDamageType::Dot;
    assert_eq!(f.hit(hit).unwrap(), 0);
    hit.kind = SourceDamageType::SpellDirect;
    let mut same_spell = hit;
    same_spell.spell = Some(ThreatSpell {
        id: 70000,
        ..Default::default()
    });
    assert_eq!(f.hit(same_spell).unwrap(), 0);
    assert_eq!(
        f.hit(hit).unwrap_err().id,
        "execution.combat.Unit_950.RemoveAurasDueToSpell_owner"
    );
    assert!(f.victim().auras.first_aura_holder(70000).is_some());
    assert_eq!(f.victim().vitals.unwrap().health, 55);
}

#[test]
fn live_damage_channels_refuse_after_the_holder_walk_and_before_health() {
    let mut f = LiveFixture::vermin();
    let template = SpellTemplate {
        id: 70000,
        channelinterruptflags: 2, // SpellDefines.h:363.
        ..Default::default()
    };
    let spell = SourceSpell::new(1, &template, f.victim, SpellTriggerFlags(0), 0).unwrap();
    f.spells.insert_template(template).unwrap();
    let v = f.victim_mut();
    v.spell_starts.spells.insert(1, spell);
    v.spell_slots
        .replace_slot(SpellSlot::Channel, 1, &mut v.spell_starts.spells)
        .unwrap();
    let hit = f.fireball();
    assert_eq!(
        f.hit(hit).unwrap_err().id,
        "execution.combat.Unit_954.InterruptSpell_owner"
    );
    assert_eq!(f.victim().spell_slots.current(SpellSlot::Channel), Some(1));
    assert_eq!(f.victim().vitals.unwrap().health, 55);
    assert_eq!(f.victim().loot_recipient_guid.raw(), 0);

    f.victim_mut().spell_starts.spells.remove(&1);
    assert_eq!(
        f.hit(hit).unwrap_err().id,
        "execution.combat.Unit_953.channel_spell_owner"
    );
}

#[test]
fn live_damage_reaches_duel_and_rage_before_health_and_durability_after() {
    let mut f = LiveFixture::vermin();
    let mage = f.mage.object();
    let mut hit = f.fireball();
    hit.victim = mage;
    f.units.get_mut(&f.objects, mage).unwrap().vitals = f.victim().vitals;
    f.mage.duel_opponent = Some(f.victim);
    hit.amount = 54;
    assert_eq!(
        f.hit(hit).unwrap_err().id,
        "execution.combat.Unit_980.duel_opponent_owner"
    );
    f.mage.duel_opponent = None;

    f.units.get_mut(&f.objects, mage).unwrap().power_type = Some(1);
    hit.dealer = None;
    hit.school = 1;
    hit.amount = 0;
    assert_eq!(
        f.hit(hit).unwrap_err().id,
        "execution.combat.Unit_962.RewardRage_owner"
    );
    hit.amount = 22;
    assert_eq!(
        f.hit(hit).unwrap_err().id,
        "execution.combat.Unit_1239.RewardRage_owner"
    );
    assert_eq!(
        f.units
            .get(&f.objects, mage)
            .unwrap()
            .vitals
            .unwrap()
            .health,
        33
    );

    f.units.get_mut(&f.objects, mage).unwrap().power_type = Some(0);
    assert_eq!(
        f.hit(hit).unwrap_err().id,
        "execution.combat.Unit_1242.durability_roll_owner"
    );
    assert_eq!(
        f.units
            .get(&f.objects, mage)
            .unwrap()
            .vitals
            .unwrap()
            .health,
        11
    );

    let mut hit = f.fireball();
    hit.kind = SourceDamageType::Direct;
    f.units.get_mut(&f.objects, mage).unwrap().power_type = Some(1);
    assert_eq!(
        f.hit(hit).unwrap_err().id,
        "execution.combat.Unit_916.RewardRage_owner"
    );
    assert_eq!(f.victim().vitals.unwrap().health, 55);
}

#[test]
fn modify_health_refuses_signed_overflow_at_the_source_operation() {
    let mut f = LiveFixture::vermin();
    let v = f.units.get_mut(&f.objects, f.victim).unwrap();
    v.vitals.as_mut().unwrap().max_health = i32::MAX as u32 + 1;
    v.vitals.as_mut().unwrap().health = i32::MAX as u32;
    assert_eq!(
        v.modify_health(&f.objects, f.victim, 1, &[])
            .unwrap_err()
            .id,
        "execution.unit.Unit_8069.health_sum_undefined"
    );
    assert_eq!(v.vitals.unwrap().health, i32::MAX as u32);
    v.vitals.as_mut().unwrap().health = i32::MAX as u32 + 1;
    assert_eq!(
        v.modify_health(&f.objects, f.victim, 1, &[])
            .unwrap_err()
            .id,
        "execution.unit.Unit_8073.health_gain_undefined"
    );
    assert_eq!(v.vitals.unwrap().health, 0);
    v.vitals.as_mut().unwrap().health = 1;
    assert_eq!(
        v.modify_health(&f.objects, f.victim, 1, &[])
            .unwrap_err()
            .id,
        "execution.unit.Unit_8087.health_gain_undefined"
    );
    assert_eq!(v.vitals.unwrap().health, i32::MAX as u32 + 1);
    v.vitals.as_mut().unwrap().max_health = 55;
    v.vitals.as_mut().unwrap().health = 54;
    assert_eq!(v.modify_health(&f.objects, f.victim, 9, &[]).unwrap(), 1);
    assert_eq!(v.vitals.unwrap().health, 55);
}

#[test]
fn live_damage_rejects_a_stale_actor_and_a_cross_map_pair_before_standing() {
    let mut f = LiveFixture::vermin();
    f.victim_mut().stand_state = 1;
    let mut hit = f.fireball();
    hit.dealer = Some(handle(999));
    assert_eq!(f.hit(hit).unwrap_err().id, "execution.stale_object_handle");
    let dealer = f.objects.construct(9, SourceObjectKind::Creature).unwrap();
    hit.dealer = Some(dealer);
    assert_eq!(
        f.hit(hit).unwrap_err().id,
        "execution.combat.Unit_902.cross_map_damage_owner"
    );
    assert_eq!(f.victim().stand_state, 1);
    assert_eq!(f.victim().vitals.unwrap().health, 55);
}

#[test]
fn live_damage_stand_refusal_keeps_the_byte_write_order() {
    let mut f = LiveFixture::vermin();
    let mage = f.mage.object();
    let mut hit = f.fireball();
    hit.victim = mage;
    hit.amount = 0;
    let v = f.units.get_mut(&f.objects, mage).unwrap();
    v.stand_state = 1;
    v.flags |= 0x0004_0000; // Unit.h:484.
    assert_eq!(
        f.hit(hit).unwrap_err().id,
        "execution.unit.Unit_10190.SendPacket_owner"
    );
    let v = f.units.get(&f.objects, mage).unwrap();
    assert_eq!(v.stand_state, 0);

    let template = f.spells.template(133).unwrap();
    let mut holder = SourceAuraHolder::new(template, f.victim, 0, 0, 8000).unwrap();
    holder.effects[1] = Some(SourceAura::self_spell_effect(template, 1, 3, 0, 0, 1, 0).unwrap());
    let v = f.victim_mut();
    v.stand_state = 1;
    v.auras.store_aura_holder(133, &mut Some(holder)).unwrap();
    hit.victim = f.victim;
    assert_eq!(
        f.hit(hit).unwrap_err().id,
        "execution.unit.Unit_10184.RemoveAurasWithInterruptFlags_owner"
    );
    assert_eq!(f.victim().stand_state, 0);
    assert!(f.victim().auras.first_aura_holder(133).is_some());
}

#[test]
fn live_health_group_writes_refuse_after_health_for_player_and_charmed_unit() {
    let mut f = LiveFixture::vermin();
    let mage = f.mage.object();
    f.units.get_mut(&f.objects, mage).unwrap().vitals = f.victim().vitals;
    let mut hit = f.fireball();
    hit.dealer = None;
    f.mage.group = Some(7);
    hit.victim = mage;
    assert_eq!(
        f.hit(hit).unwrap_err().id,
        "execution.unit.Unit_9236.SetGroupUpdateFlag_owner"
    );
    assert_eq!(
        f.units
            .get(&f.objects, mage)
            .unwrap()
            .vitals
            .unwrap()
            .health,
        33
    );

    hit.victim = f.victim;
    f.victim_mut().control_guids.charmer = SourceGuid::from_raw(1);
    assert_eq!(
        f.hit(hit).unwrap_err().id,
        "execution.unit.Unit_9242.SetGroupUpdateFlag_owner"
    );
    assert_eq!(f.victim().vitals.unwrap().health, 33);
}

#[test]
fn live_damage_mods_refuse_at_the_installed_ai_callback() {
    let mut f = LiveFixture::vermin();
    let mut hit = f.fireball();
    let mut absorb = 0;
    f.victim_mut().has_ai = true;
    f.units.get_mut(&f.objects, f.mage.object()).unwrap().vitals = f.victim().vitals;
    let mut owner = SourceLiveDamage {
        objects: &f.objects,
        units: &mut f.units,
        spells: &f.spells,
        players: &[&f.mage],
    };
    assert_eq!(
        f.combat
            .damage_mods(&mut hit, Some(&mut absorb), &mut owner)
            .unwrap_err()
            .id,
        "execution.combat.Unit_885.DamageTaken_owner"
    );
    hit.dealer = Some(f.victim);
    assert_eq!(
        f.combat
            .damage_mods(&mut hit, Some(&mut absorb), &mut owner)
            .unwrap_err()
            .id,
        "execution.combat.Unit_881.DamageDeal_owner"
    );
    owner
        .units
        .get_mut(owner.objects, f.victim)
        .unwrap()
        .static_flags[0] |= 8;
    hit.amount = 55;
    assert_eq!(
        f.combat
            .creature_damage_taken(&mut hit, &mut owner)
            .unwrap_err()
            .id,
        "execution.combat.CreatureAI_111.JustPreventedDeath_owner"
    );
    assert_eq!(absorb, 0);
    assert_eq!(f.victim().vitals.unwrap().health, 55);
}

#[test]
fn live_damage_names_missing_power_and_health_at_the_read() {
    let mut f = LiveFixture::vermin();
    let mage = f.mage.object();
    let mut hit = f.fireball();
    hit.kind = SourceDamageType::Direct;
    f.units.get_mut(&f.objects, mage).unwrap().power_type = None;
    assert_eq!(
        f.hit(hit).unwrap_err().id,
        "execution.combat.Unit_915.GetPowerType_owner"
    );

    hit.victim = mage;
    hit.dealer = None;
    hit.school = 1;
    hit.amount = 0;
    assert_eq!(
        f.hit(hit).unwrap_err().id,
        "execution.combat.Unit_960.GetPowerType_owner"
    );
    hit.amount = 1;
    assert_eq!(
        f.hit(hit).unwrap_err().id,
        "execution.combat.Unit_969.GetHealth_owner"
    );
    f.units.get_mut(&f.objects, mage).unwrap().vitals = f.victim().vitals;
    assert_eq!(
        f.hit(hit).unwrap_err().id,
        "execution.combat.Unit_1237.GetPowerType_owner"
    );
    assert_eq!(
        f.units
            .get(&f.objects, mage)
            .unwrap()
            .vitals
            .unwrap()
            .health,
        54
    );
}

#[test]
fn live_damage_refuses_missing_player_state_at_the_source_read() {
    let mut f = LiveFixture::vermin();
    let mage = f.mage.object();
    let mut hit = f.fireball();
    f.units.get_mut(&f.objects, mage).unwrap().vitals = f.victim().vitals;
    let mut owner = SourceLiveDamage {
        objects: &f.objects,
        units: &mut f.units,
        spells: &f.spells,
        players: &[],
    };
    let mut deal = |mut hit| {
        f.combat
            .deal_damage(&mut hit, &mut f.threats, &mut owner)
            .unwrap_err()
            .id
    };
    assert_eq!(deal(hit), "execution.combat.Creature_1176.GetGroup_owner");
    hit.victim = mage;
    assert_eq!(deal(hit), "execution.combat.Unit_976.duel_owner");
    hit.dealer = None;
    assert_eq!(
        deal(hit),
        "execution.combat.Unit_1006.IsPreventingDeath_owner"
    );
    assert_eq!(
        owner
            .units
            .get(&f.objects, mage)
            .unwrap()
            .vitals
            .unwrap()
            .health,
        55
    );
}

#[test]
fn live_damage_aura_template_must_be_bound_before_the_time_filter() {
    let mut f = LiveFixture::vermin();
    let template = SpellTemplate {
        id: 70000,
        ..Default::default()
    };
    let mut holder = SourceAuraHolder::new(&template, f.victim, 0, 20, -1).unwrap();
    holder.effects[0] = Some(SourceAura::self_spell_effect(&template, 0, 3, 0, 0, 1, 0).unwrap());
    f.victim_mut()
        .auras
        .store_aura_holder(70000, &mut Some(holder))
        .unwrap();
    let hit = f.fireball();
    assert_eq!(
        f.hit(hit).unwrap_err().id,
        "execution.combat.Unit_935.GetSpellProto_owner"
    );
    assert!(f.victim().auras.first_aura_holder(70000).is_some());
    assert_eq!(f.victim().loot_recipient_guid.raw(), 0);
    assert_eq!(f.victim().vitals.unwrap().health, 55);
}

#[test]
fn unit_health_refuses_missing_vitals_or_player_group_after_the_source_reads() {
    let mut f = LiveFixture::vermin();
    let mage = f.mage.object();
    let fields = f.units.get_mut(&f.objects, mage).unwrap();
    assert_eq!(
        fields.set_health(&f.objects, mage, 0, &[]).unwrap_err().id,
        "execution.unit.Unit_9226.GetMaxHealth_owner"
    );
    assert_eq!(
        fields
            .modify_health(&f.objects, mage, 1, &[])
            .unwrap_err()
            .id,
        "execution.unit.Unit_8067.GetHealth_owner"
    );
    fields.vitals = Some(SourceUnitVitals {
        health: 51,
        max_health: 51,
        mana: 0,
        max_mana: 0,
        has_mana: true,
        energy: 0,
        max_energy: 0,
    });
    assert_eq!(
        fields.set_health(&f.objects, mage, 50, &[]).unwrap_err().id,
        "execution.unit.Unit_9235.GetGroup_owner"
    );
    assert_eq!(fields.vitals.unwrap().health, 50);
    let fields = f.units.get_mut(&f.objects, f.victim).unwrap();
    fields.control_guids.charmer = SourceGuid::from_raw(1);
    assert_eq!(
        fields
            .set_health(&f.objects, f.victim, 50, &[])
            .unwrap_err()
            .id,
        "execution.unit.Unit_9241.GetGroup_owner"
    );
    assert_eq!(fields.vitals.unwrap().health, 50);
}

#[test]
fn unbound_dealer_suffix_does_not_skip_durability_or_pushback() {
    let mut f = LiveFixture::vermin();
    let mut hit = f.fireball();
    let mut owner = SourceLiveDamage {
        objects: &f.objects,
        units: &mut f.units,
        spells: &f.spells,
        players: &[&f.mage],
    };
    hit.durability_loss = false;
    assert_eq!(
        owner
            .combat_call(CombatCall::DamageDealtSuffix, &mut hit)
            .unwrap_err()
            .id,
        "execution.combat.Unit_1226.durability_roll_owner"
    );
    hit.dealer = Some(f.victim);
    assert_eq!(
        owner
            .combat_call(CombatCall::DamageDealtSuffix, &mut hit)
            .unwrap_err()
            .id,
        "execution.combat.Unit_1250.InterruptOrDelaySpell_owner"
    );
    hit.kind = SourceDamageType::Shield;
    owner
        .combat_call(CombatCall::DamageDealtSuffix, &mut hit)
        .unwrap();
}

#[test]
fn live_tap_requires_a_player_guid_and_resolves_group_reads_in_order() {
    let mut f = LiveFixture::vermin();
    let hit = f.fireball();
    let mage = f.mage.object();
    f.objects.get_mut(mage).unwrap().guid = None;
    assert_eq!(
        f.hit(hit).unwrap_err().id,
        "execution.combat.Creature_1173.GetObjectGuid_owner"
    );
    assert_eq!(f.victim().loot_recipient_guid.raw(), 0);
    f.objects.get_mut(mage).unwrap().guid = Some(SourceGuid::from_raw(1));
    assert_eq!(
        f.hit(hit).unwrap_err().id,
        "execution.combat.Unit_1211.CanAttack_owner"
    );

    let mut npc = hit;
    npc.dealer = Some(f.victim);
    f.units.get_mut(&f.objects, mage).unwrap().flags &= !8;
    assert_eq!(
        f.hit(npc).unwrap_err().id,
        "execution.combat.Relations_916.IsFriend_owner"
    );
    f.units.get_mut(&f.objects, mage).unwrap().flags |= 8;

    let other = player::SourcePlayer::construct(
        &mut f.objects,
        &mut f.units,
        9,
        900_000,
        |min, _| Ok(min),
        || Ok(1),
    )
    .unwrap();
    let object = other.object();
    let map = f.objects.get(f.victim).unwrap().map.unwrap();
    let cells = SourceCells::new(map).unwrap();
    f.objects
        .assign_guid(object, 0, SourceGuid::from_raw(2))
        .unwrap();
    f.objects.bind_map(object, map, &cells).unwrap();
    f.objects.add_to_world(object).unwrap();
    f.objects.register_player_lookup(object).unwrap();
    f.units.get_mut(&f.objects, object).unwrap().flags |= 8;
    let mut other_hit = hit;
    other_hit.dealer = Some(object);
    assert_eq!(
        f.hit(other_hit).unwrap_err().id,
        "execution.combat.Relations_728.GetControllingPlayer_owner"
    );
    f.victim_mut().loot_recipient_guid = SourceGuid::from_raw(2);
    assert_eq!(
        f.hit(hit).unwrap_err().id,
        "execution.combat.Relations_730.GetControllingPlayer_owner"
    );

    f.mage.group = Some(7);
    let mut hit = hit;
    let mut owner = SourceLiveDamage {
        objects: &f.objects,
        units: &mut f.units,
        spells: &f.spells,
        players: &[&f.mage, &other],
    };
    assert_eq!(
        f.combat
            .deal_damage(&mut hit, &mut f.threats, &mut owner)
            .unwrap_err()
            .id,
        "execution.combat.Relations_733.group_owner"
    );
    assert_eq!(f.victim().vitals.unwrap().health, 33);
    assert_eq!(f.victim().damage_by_others, 0);
}

#[test]
fn live_player_damage_cannot_give_a_pet_its_first_tap() {
    let mut f = LiveFixture::vermin();
    let map = f.objects.get(f.victim).unwrap().map.unwrap();
    let cells = SourceCells::new(map).unwrap();
    let pet = f.objects.construct(9, SourceObjectKind::Pet).unwrap();
    f.objects
        .assign_guid(pet, 0, SourceGuid::with_entry(HighGuid::Pet, 6, 1))
        .unwrap();
    f.objects.bind_map(pet, map, &cells).unwrap();
    f.objects.add_to_world(pet).unwrap();
    let mut fields = SourceUnitFields::constructed_creature();
    fields.vitals = f.victim().vitals;
    f.units.register(&f.objects, pet, fields).unwrap();
    let mut hit = f.fireball();
    hit.victim = pet;
    assert_eq!(
        f.hit(hit).unwrap_err().id,
        "execution.combat.Unit_1211.CanAttack_owner"
    );
    let fields = f.units.get(&f.objects, pet).unwrap();
    assert_eq!(fields.loot_recipient_guid.raw(), 0);
    assert_eq!(fields.vitals.unwrap().health, 33);
    assert!(!f.objects.get(pet).unwrap().field_changed(143).unwrap());
}
