use super::*;

#[test]
fn delayed_forced_despawn_reaches_real_death_corpse_and_health_at_deadline() {
    // Creature.cpp:96-102,1962-1980, not an authored successful event callback.
    let mut f = Fixture::new(false);
    f.apply(|l, c, h| l.forced_despawn_after(25, false, c, h))
        .unwrap();
    assert_eq!(f.life.death_state, CreatureDeathState::Alive);
    f.apply(|l, c, h| update_source_creature_events(l, 24, c, h))
        .unwrap();
    assert_eq!(f.life.death_state, CreatureDeathState::Alive);
    f.apply(|l, c, h| update_source_creature_events(l, 1, c, h))
        .unwrap();
    assert_eq!(f.life.death_state, CreatureDeathState::Dead);
    assert_eq!(f.host.factory.health[&f.life.object()], 0);
    assert!(f.host.calls.contains(&Call::Phase(Phase::SetHealthZero)));
    assert!(f.life.unit_events.inventory().queued.is_empty());
}
#[test]
fn delayed_only_alive_checks_death_at_execute_not_at_schedule() {
    for only_alive in [false, true] {
        let mut f = Fixture::new(false);
        f.apply(|l, c, h| l.forced_despawn_after(25, only_alive, c, h))
            .unwrap();
        f.die();
        assert_eq!(f.life.death_state, CreatureDeathState::Corpse);
        f.apply(|l, c, h| update_source_creature_events(l, 25, c, h))
            .unwrap();
        assert_eq!(
            f.life.death_state,
            if only_alive {
                CreatureDeathState::Corpse
            } else {
                CreatureDeathState::Dead
            }
        );
        assert_eq!(
            f.host.calls.contains(&Call::Phase(Phase::SetHealthZero)),
            !only_alive
        );
        assert!(f.life.unit_events.inventory().queued.is_empty());
    }
}
#[test]
fn immediate_only_alive_is_ignored_for_corpse_like_the_source() {
    // onlyAlive is inside the delayed event only; Creature.cpp:98,1962-1980.
    let mut f = Fixture::new(false);
    f.die();
    f.apply(|l, c, h| l.forced_despawn_after(0, true, c, h))
        .unwrap();
    assert_eq!(f.life.death_state, CreatureDeathState::Dead);
    assert!(f.host.calls.contains(&Call::Phase(Phase::SetHealthZero)));
    assert!(f.life.unit_events.inventory().queued.is_empty());
}
#[test]
fn delayed_event_cancel_and_cleanup_leave_creature_alive() {
    for mode in 0..3 {
        let mut f = Fixture::new(false);
        f.apply(|l, c, h| l.forced_despawn_after(25, false, c, h))
            .unwrap();
        let id = f.life.unit_events.inventory().queued[0].event_id;
        match mode {
            0 => f.life.unit_events.cancel(id).unwrap(),
            1 => f.life.unit_events.kill_all(false).unwrap(),
            _ => f.life.unit_events.kill_all(true).unwrap(),
        }
        f.apply(|l, c, h| update_source_creature_events(l, 25, c, h))
            .unwrap();
        assert_eq!(f.life.death_state, CreatureDeathState::Alive);
        assert!(f.life.unit_events.inventory().queued.is_empty());
        assert!(!f.host.calls.contains(&Call::Phase(Phase::UnitJustDied)));
    }
}
#[test]
fn delayed_event_refuses_at_reached_lifecycle_owner_before_health_zero() {
    let mut f = Fixture::new(false);
    f.host.refuse = Some(Phase::UnitJustDied);
    f.apply(|l, c, h| l.forced_despawn_after(1, false, c, h))
        .unwrap();
    assert_eq!(
        f.apply(|l, c, h| update_source_creature_events(l, 1, c, h))
            .unwrap_err()
            .id,
        "execution.creature_lifecycle.f5_unit_effects_pending"
    );
    assert!(!f.host.calls.contains(&Call::Phase(Phase::SetHealthZero)));
    assert_eq!(
        f.apply(|l, c, h| update_source_creature_events(l, 1, c, h))
            .unwrap_err()
            .id,
        "execution.unit_events.stopped"
    );
}
#[test]
fn unit_update_reaches_spell_owner_before_queue_or_heartbeat() {
    let mut f = Fixture::new(false);
    f.apply(|l, c, h| l.forced_despawn_after(1, false, c, h))
        .unwrap();
    assert_eq!(
        f.apply(|l, c, h| update_source_creature_unit(l, 1, c, h))
            .unwrap_err()
            .id,
        "execution.unit_update.f5_08_spell_owner"
    );
    assert!(f.life.unit_update.spell_update_happening);
    assert_eq!(f.life.unit_events.clock_millis(), 0);
    assert_eq!(f.life.unit_events.inventory().queued.len(), 1);
    assert_eq!(f.life.unit_update.heartbeat_expiry(), 0);
}

#[test]
fn out_of_world_unit_update_leaves_pending_events_and_timer_untouched() {
    // Unit.cpp:465-466, contrasted with the in-world refusal immediately above.
    let mut f = Fixture::new(false);
    f.apply(|l, c, h| l.forced_despawn_after(1, false, c, h))
        .unwrap();
    f.world.objects.remove_from_world(f.life.object()).unwrap();
    f.apply(|l, c, h| update_source_creature_unit(l, 10000, c, h))
        .unwrap();
    assert!(!f.life.unit_update.spell_update_happening);
    assert_eq!(f.life.unit_events.clock_millis(), 0);
    assert_eq!(f.life.unit_events.inventory().queued.len(), 1);
    assert_eq!(f.life.unit_update.heartbeat_expiry(), 0);
    assert_eq!(f.life.death_state, CreatureDeathState::Alive);
}

#[test]
fn delayed_pet_virtual_call_refuses_unsummon_but_only_alive_can_skip_it() {
    // Pet.cpp:2284-2287 schedules the base event; Execute invokes the Pet override.
    for only_alive in [false, true] {
        let mut f = Fixture::new(false);
        f.apply(|l, c, h| l.forced_despawn_after(1, only_alive, c, h))
            .unwrap();
        f.die();
        // Authored virtual-class tag, not a substitute for F5.15 pet construction.
        f.world.objects.get_mut(f.life.object()).unwrap().kind = SourceObjectKind::Pet;
        let result = f.apply(|l, c, h| update_source_creature_events(l, 1, c, h));
        if only_alive {
            result.unwrap();
            assert!(f.life.unit_events.inventory().queued.is_empty());
        } else {
            assert_eq!(
                result.unwrap_err().id,
                "execution.unit_events.f5_15b_pet_forced_despawn_owner"
            );
        }
        assert_eq!(f.life.death_state, CreatureDeathState::Corpse);
        assert!(!f.host.calls.contains(&Call::Phase(Phase::SetHealthZero)));
    }
}

#[test]
fn immediate_pet_virtual_call_refuses_before_creature_corpse_removal() {
    let mut f = Fixture::new(false);
    f.world.objects.get_mut(f.life.object()).unwrap().kind = SourceObjectKind::Pet;
    assert_eq!(
        f.apply(|l, c, h| l.forced_despawn_after(0, false, c, h))
            .unwrap_err()
            .id,
        "execution.unit_events.f5_15b_pet_forced_despawn_owner"
    );
    assert_eq!(f.life.death_state, CreatureDeathState::Alive);
    assert!(!f.host.calls.contains(&Call::Phase(Phase::UnitJustDied)));
}
