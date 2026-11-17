use super::*;

#[derive(Default)]
struct Clock(Vec<&'static str>);
impl SpellCooldownClock for Clock {
    fn map_clock(&mut self) -> Result<i64> {
        self.0.push("map");
        Ok(100)
    }
    fn world_clock(&mut self) -> Result<i64> {
        self.0.push("world");
        Ok(200)
    }
}
fn template() -> SpellTemplate {
    SpellTemplate {
        id: 42,
        category: 7,
        school: 2,
        preventiontype: 1,
        ..SpellTemplate::default()
    }
}
#[test]
fn empty_cooldowns_use_the_selected_cached_clock_and_are_ready() {
    let cd = SourceSpellCooldowns::default();
    let mut clock = Clock::default();
    assert!(cd
        .is_spell_ready(&template(), false, true, &mut clock)
        .unwrap());
    assert!(cd
        .is_spell_ready(&template(), false, false, &mut clock)
        .unwrap());
    assert_eq!(clock.0, ["map", "world"]);
    assert!(cd.is_empty());
}
#[test]
fn cooldown_boundaries_and_permanent_entries_follow_object_h() {
    let mut cd = SourceSpellCooldowns::default();
    cd.add(50, &template(), 50, 80, false, |_| Ok(Some(0)))
        .unwrap();
    assert!(!cd.ready_at(&template(), false, 99).unwrap());
    assert!(!cd.ready_at(&template(), false, 100).unwrap());
    assert!(cd.ready_at(&template(), false, 130).unwrap());
    let mut other = template();
    other.category = 0;
    assert!(!cd.ready_at(&other, false, 99).unwrap());
    assert!(cd.ready_at(&other, false, 100).unwrap());
    cd.add(50, &template(), 0, 0, true, |_| {
        panic!("no category lookup")
    })
    .unwrap();
    assert!(!cd.ready_at(&template(), false, i64::MAX).unwrap());
}
#[test]
fn global_cooldown_silence_and_item_reads_preserve_source_order() {
    let mut cd = SourceSpellCooldowns::default();
    cd.add(50, &template(), 0, 50, false, |_| Ok(Some(2)))
        .unwrap();
    assert!(!cd.ready_at(&template(), true, 99).unwrap());
    assert_eq!(
        cd.ready_at(&template(), true, 100).unwrap_err().id,
        "execution.spells.Object_2718.item_prototype_owner"
    );
    assert!(cd.ready_at(&template(), false, 100).unwrap());
    cd.lockouts.insert(2, 110);
    assert!(!cd.ready_at(&template(), false, 109).unwrap());
    assert!(cd.ready_at(&template(), false, 110).unwrap());
    let mut unprevented = template();
    unprevented.preventiontype = 0;
    assert!(cd.ready_at(&unprevented, false, 109).unwrap());
}
#[test]
fn zero_duration_world_add_writes_nothing_and_never_reads_a_clock() {
    let mut cd = SourceSpellCooldowns::default();
    cd.add_cooldown(
        &template(),
        0,
        false,
        || panic!("no clock"),
        |_| panic!("no category"),
    )
    .unwrap();
    assert!(cd.is_empty());
}
#[test]
fn category_null_refuses_after_the_spell_insertion() {
    let mut cd = SourceSpellCooldowns::default();
    assert_eq!(
        cd.add(0, &template(), 1, 1, false, |_| Ok(None))
            .unwrap_err()
            .id,
        "execution.spells.Object_3110.category_null_undefined"
    );
    assert_eq!(cd.spells.len(), 1);
    assert!(cd.categories.is_empty());
    cd.add(0, &template(), 1, 1, false, |_| Ok(Some(0)))
        .unwrap();
    assert_eq!(cd.categories.get(&7), Some(&42));
}
#[test]
fn category_reuse_retains_original_owner_and_permanent_priority() {
    let mut cd = SourceSpellCooldowns::default();
    cd.add(0, &template(), 2, 10, true, |_| Ok(Some(0)))
        .unwrap();
    let mut other = template();
    other.id = 43;
    cd.add(0, &other, 2, 20, true, |_| Ok(Some(0))).unwrap();
    assert_eq!(cd.categories[&7], 42);
    assert_eq!(cd.spells[&43].category, 0);
    cd.add(0, &other, 2, 30, false, |_| Ok(Some(0))).unwrap();
    assert_eq!(cd.categories[&7], 42);
    assert!(!cd.spells[&42].permanent);
    assert!(!cd.ready_at(&template(), false, 29).unwrap());
    assert!(cd.ready_at(&template(), false, 30).unwrap());
    cd.update(30);
    assert!(cd.spells.is_empty() && cd.categories.is_empty());
}
#[test]
fn category_gcd_emplace_and_exact_expiry_are_independent_of_container_global() {
    let mut cd = SourceSpellCooldowns::default();
    let mut t = template();
    t.startrecoverytime = 50;
    t.startrecoverycategory = 3;
    cd.add_gcd(&t, 0, || Ok(50)).unwrap();
    cd.add_gcd(&t, 200, || Ok(50)).unwrap();
    assert!(cd.gcd.has(3, 99));
    assert!(!cd.gcd.has(3, 100));
    assert!(cd.ready_at(&t, false, 99).unwrap());
}
#[test]
fn undefined_clock_and_school_boundaries_refuse_at_the_read() {
    assert_eq!(expiry(i64::MAX - 1, 1).unwrap(), i64::MAX);
    assert_eq!(
        expiry(i64::MAX, 1).unwrap_err().id,
        "execution.spells.cooldown_clock_add_undefined"
    );
    let mut cd = SourceSpellCooldowns::default();
    let mut t = template();
    t.school = 31;
    cd.lockouts.insert(31, 10);
    assert!(!cd.ready_at(&t, false, 9).unwrap());
    assert!(cd.ready_at(&t, false, 10).unwrap());
    t.school = 32;
    assert_eq!(
        cd.ready_at(&t, false, 10).unwrap_err().id,
        "execution.spells.Object_2742.school_shift_undefined"
    );
    t.school = 0;
    cd.lockouts.insert(32, 10);
    assert_eq!(
        cd.ready_at(&t, false, 10).unwrap_err().id,
        "execution.spells.Object_2669.lockout_shift_undefined"
    );
}

#[test]
fn writers_preserve_forced_duration_ignore_category_and_send_exits() {
    let mut cd = SourceSpellCooldowns::default();
    let mut t = template();
    t.recoverytime = 10;
    t.categoryrecoverytime = 20;
    cd.add_cooldown(&t, 30, true, || Ok(100), |_| unreachable!())
        .unwrap();
    assert!(cd.categories.is_empty());
    assert_eq!(cd.spells[&42].spell_expiry, 130);
    assert!(!cd.ready_at(&t, false, 129).unwrap());
    assert!(cd.ready_at(&t, false, 130).unwrap());
    cd.remove_spell(42);
    t.attributes = 0x40;
    cd.send_world_spell_cooldown(&t, false, || unreachable!(), |_| unreachable!())
        .unwrap();
    t.attributes = 0;
    cd.send_world_spell_cooldown(&t, true, || unreachable!(), |_| unreachable!())
        .unwrap();
    assert!(cd.is_empty());
}

#[test]
fn category_expiry_detaches_its_index_before_the_spell_expires() {
    let mut cd = SourceSpellCooldowns::default();
    cd.add(0, &template(), 20, 10, false, |_| Ok(Some(0)))
        .unwrap();
    cd.update(10);
    assert_eq!(cd.spells[&42].category, 0);
    assert!(cd.categories.is_empty());
    assert!(!cd.ready_at(&template(), false, 19).unwrap());
    cd.update(20);
    assert!(cd.is_empty());
}
