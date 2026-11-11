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
        critical: false,
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
