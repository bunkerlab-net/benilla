// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored callback hosts are not real F5 gameplay or sealed MG/MR evidence.
use super::*;
use std::rc::Rc;

type Action = LinkingAction;
type Event = LinkingEvent;
#[derive(Clone, Debug, PartialEq, Eq)]
enum Effect {
    Action(ObjectHandle, Action),
    Schedule(u32, usize),
    Wall,
}
enum Mutation {
    Reenter(Event, ObjectHandle, Option<ObjectHandle>),
    Append(ObjectHandle, u32),
    Remove(ObjectHandle),
}
struct Host {
    scene: Scene,
    links: Rc<SourceCreatureLinking>,
    creatures: BTreeMap<ObjectHandle, LinkingCreature>,
    effects: Vec<Effect>,
    mutation: Option<Mutation>,
    wall: i64,
    clock_kind: ClockKind,
    refuse: bool,
}
impl Host {
    fn new(
        templates: Vec<LinkingTemplateRow>,
        guids: Vec<LinkingGuidRow>,
        d: &mut Definitions,
    ) -> Self {
        Self {
            scene: Scene::new(),
            links: Rc::new(loaded(templates, guids, d)),
            creatures: BTreeMap::new(),
            effects: vec![],
            mutation: None,
            wall: 100,
            clock_kind: ClockKind::WallSeconds,
            refuse: false,
        }
    }
    fn actor(&mut self, entry: u32, db: u32, x: f32) -> ObjectHandle {
        let object = self.scene.actor(entry, db, db, x, true, false);
        self.creatures.insert(
            object,
            LinkingCreature {
                object,
                entry,
                database: db,
                respawn_xy: [x.to_bits(), 0],
                new_spawning: false,
                death: CreatureDeathState::Alive,
                pet: false,
                controlled: false,
                combat: false,
                respawn_delay: 10,
                respawn_time: 200,
            },
        );
        register_source_creature_linking(
            &self.links,
            &mut self.scene.holder,
            &self.scene.objects,
            object,
            entry,
        )
        .unwrap();
        object
    }
    fn fire(
        &mut self,
        event: Event,
        source: ObjectHandle,
        enemy: Option<ObjectHandle>,
    ) -> Result<()> {
        let links = Rc::clone(&self.links);
        let mut holder = std::mem::replace(
            &mut self.scene.holder,
            SourceCreatureLinkingHolder::new(map()).unwrap(),
        );
        let result = holder.event(&links, self, event, source, enemy);
        self.scene.holder = holder;
        result
    }
    fn follow(&mut self, source: ObjectHandle) -> Result<bool> {
        let links = Rc::clone(&self.links);
        let mut holder = std::mem::replace(
            &mut self.scene.holder,
            SourceCreatureLinkingHolder::new(map()).unwrap(),
        );
        let result = holder.try_follow_master(&links, self, source);
        self.scene.holder = holder;
        result
    }
}
impl LinkingEventOwner for Host {
    fn linking_creature(
        &mut self,
        _: MapIdentity,
        object: ObjectHandle,
    ) -> Result<LinkingCreature> {
        self.creatures
            .get(&object)
            .copied()
            .ok_or_else(|| refuse("authored.creature"))
    }
    fn linking_lookup(
        &mut self,
        map: MapIdentity,
        lookup: LinkingLookup,
    ) -> Result<Option<ObjectHandle>> {
        Ok(lookup.resolve(map, &self.scene.objects))
    }
    fn linking_wall(&mut self) -> Result<ClockSample> {
        self.effects.push(Effect::Wall);
        Ok(ClockSample::authored(self.clock_kind, self.wall))
    }
    fn linking_schedule(
        &mut self,
        request: LinkingScheduleRequest,
        holder: &mut SourceCreatureLinkingHolder,
    ) -> Result<()> {
        let i = holder.inventory();
        let retained = i
            .entries
            .values()
            .chain(i.guids.values())
            .flatten()
            .map(|g| g.slaves.len())
            .sum();
        self.effects
            .push(Effect::Schedule(request.database, retained));
        Ok(())
    }
    fn linking_action(
        &mut self,
        request: LinkingActionRequest,
        holder: &mut SourceCreatureLinkingHolder,
    ) -> Result<()> {
        if self.refuse {
            return Err(request.action.missing_owner());
        }
        self.effects
            .push(Effect::Action(request.object, request.action));
        // This authored host models callback mutations, not production combat/AI.
        match request.action {
            Action::Suicide => {
                self.creatures.get_mut(&request.object).unwrap().death = CreatureDeathState::Corpse
            }
            Action::ForcedDespawn => {
                self.creatures.get_mut(&request.object).unwrap().death = CreatureDeathState::Dead
            }
            Action::Respawn => {
                let c = self.creatures.get_mut(&request.object).unwrap();
                c.death = CreatureDeathState::Dead;
                c.respawn_time = self.wall;
            }
            _ => (),
        }
        if let Some(mutation) = self.mutation.take() {
            match mutation {
                Mutation::Reenter(event, source, enemy) => {
                    let links = Rc::clone(&self.links);
                    holder.event(&links, self, event, source, enemy)?;
                }
                Mutation::Append(object, entry) => {
                    holder.add_slave(&self.links, &self.scene.objects, object, entry)?
                }
                Mutation::Remove(object) => self.scene.objects.remove_from_world(object)?,
            }
        }
        Ok(())
    }
}
fn entry_host(flags: u16, range: u16) -> (Host, ObjectHandle, ObjectHandle) {
    let mut h = Host::new(
        vec![template(7, 0, 8, flags, range)],
        vec![],
        &mut Definitions::default(),
    );
    let source = h.actor(8, 2, 0.0);
    let slave = h.actor(7, 1, 1.0);
    (h, source, slave)
}
#[test]
fn recursive_same_list_guard_does_not_suppress_a_different_list() {
    // Nested traversal skips only the active group, not the whole event; cpp:427-448.
    let mut h = Host::new(
        vec![template(7, 0, 8, 1, 5), template(9, 0, 8, 1, 10)],
        vec![],
        &mut Definitions::default(),
    );
    let source = h.actor(8, 10, 0.0);
    let first = h.actor(7, 11, 1.0);
    let second = h.actor(9, 12, 1.0);
    h.mutation = Some(Mutation::Reenter(Event::Aggro, source, Some(source)));
    h.fire(Event::Aggro, source, Some(source)).unwrap();
    assert_eq!(
        h.effects,
        vec![
            Effect::Action(first, Action::AttackStart(source)),
            Effect::Action(second, Action::AttackStart(source)),
            Effect::Action(second, Action::AttackStart(source))
        ]
    );
    assert!(h.scene.holder.inventory().entries[&8]
        .iter()
        .all(|g| !g.in_use));
}
#[test]
fn entry_then_guid_then_reverse_master_order_and_bilateral_combat() {
    // Entry and GUID lists precede reverse actions, whose combat calls target the enemy; cpp:427-501.
    let mut d = Definitions::default();
    d.spawns.insert(30, spawn(9, 0, 0.0));
    d.spawns.insert(20, spawn(8, 0, 0.0));
    let mut h = Host::new(
        vec![template(7, 0, 8, 1, 0), template(8, 0, 10, 2, 0)],
        vec![guid_row(30, 20, 1)],
        &mut d,
    );
    let master = h.actor(10, 40, 0.0);
    let source = h.actor(8, 20, 0.0);
    let entry = h.actor(7, 10, 0.0);
    let guid = h.actor(9, 30, 0.0);
    h.creatures.get_mut(&master).unwrap().combat = true;
    h.fire(Event::Aggro, source, Some(source)).unwrap();
    assert_eq!(
        h.effects,
        vec![
            Effect::Action(entry, Action::AttackStart(source)),
            Effect::Action(guid, Action::AttackStart(source)),
            Effect::Action(master, Action::AddThreat(source)),
            Effect::Action(source, Action::AddThreat(master)),
            Effect::Action(source, Action::SetInCombatWith(master)),
            Effect::Action(source, Action::TriggerCombatTimer(master))
        ]
    );
}
#[test]
fn flags_and_ranges_have_independent_groups_and_strict_respawn_xy() {
    // Flags and range form distinct lists; range is strict respawn XY, not current pose; cpp:349-362,647-659.
    let mut h = Host::new(
        vec![
            template(7, 0, 8, 1, 5),
            template(9, 0, 8, 1, 10),
            template(10, 0, 8, 0x1000, 5),
        ],
        vec![],
        &mut Definitions::default(),
    );
    let source = h.actor(8, 1, 0.0);
    h.actor(7, 2, 5.0);
    let in_range = h.actor(9, 3, 5.0);
    h.actor(10, 4, 1.0);
    h.fire(Event::Aggro, source, Some(source)).unwrap();
    assert_eq!(
        h.effects,
        vec![Effect::Action(in_range, Action::AttackStart(source))]
    );
    assert_eq!(h.scene.holder.inventory().entries[&8].len(), 3);
}
#[test]
fn new_system_schedules_absent_and_corpse_before_pruning_or_range() {
    // Preprocessing uses the source system and schedules outside range before lazy erase; cpp:520-549.
    let (mut h, source, missing) = entry_host(0x0040, 1);
    let corpse = h.actor(7, 3, 999.0);
    h.creatures.get_mut(&source).unwrap().new_spawning = true;
    h.creatures.get_mut(&corpse).unwrap().death = CreatureDeathState::Corpse;
    h.scene.objects.remove_from_world(missing).unwrap();
    h.fire(Event::Die, source, None).unwrap();
    assert_eq!(
        h.effects,
        vec![Effect::Schedule(1, 2), Effect::Schedule(3, 1)]
    );
    assert_eq!(h.scene.holder.inventory().entries[&8][0].slaves.len(), 1);
}
#[test]
fn new_system_removes_respawn_flag_for_present_dead_slave() {
    // DEAD is not CORPSE, so preprocessing does not schedule and postprocess has no respawn; cpp:522-534.
    let (mut h, source, slave) = entry_host(0x0080, 0);
    h.creatures.get_mut(&source).unwrap().new_spawning = true;
    h.creatures.get_mut(&slave).unwrap().death = CreatureDeathState::Dead;
    h.fire(Event::Respawn, source, None).unwrap();
    assert!(h.effects.is_empty());
}
#[test]
fn zero_database_slave_uses_full_runtime_lookup_and_schedules_zero() {
    // Non-DB slaves resolve full GUID, but preprocessing still passes their zero DB GUID; cpp:529-534.
    let (mut h, source, _) = entry_host(0x0040, 0);
    let object = h.scene.actor(7, 0, 99, 0.0, true, false);
    h.scene
        .holder
        .add_slave(&h.links, &h.scene.objects, object, 7)
        .unwrap();
    h.scene.objects.remove_from_world(object).unwrap();
    h.creatures.get_mut(&source).unwrap().new_spawning = true;
    h.fire(Event::Die, source, None).unwrap();
    assert_eq!(h.effects, vec![Effect::Schedule(0, 2)]);
}
#[test]
fn lazy_stale_pruning_only_occurs_for_nonzero_event_mask_including_follow_die() {
    // FOLLOW enters the DIE mask even though it has no death action; cpp:262,515-516,535-539.
    let mut d = Definitions::default();
    d.unique.insert((8, 0), vec![2]);
    let mut h = Host::new(vec![template(7, 0, 8, 0x0200, 0)], vec![], &mut d);
    let source = h.actor(8, 2, 0.0);
    let stale = h.actor(7, 1, 0.0);
    h.scene.objects.remove_from_world(stale).unwrap();
    h.fire(Event::Evade, source, None).unwrap();
    assert_eq!(h.scene.holder.inventory().entries[&8][0].slaves.len(), 1);
    h.fire(Event::Die, source, None).unwrap();
    assert!(h.scene.holder.inventory().entries[&8][0].slaves.is_empty());
    assert!(h.effects.is_empty());
}
#[test]
fn controlled_source_and_enemyless_aggro_do_not_prune() {
    // Both early exits precede list traversal; cpp:409-413.
    let (mut h, source, stale) = entry_host(1, 0);
    h.scene.objects.remove_from_world(stale).unwrap();
    h.fire(Event::Aggro, source, None).unwrap();
    h.creatures.get_mut(&source).unwrap().controlled = true;
    h.fire(Event::Aggro, source, Some(source)).unwrap();
    assert_eq!(h.scene.holder.inventory().entries[&8][0].slaves.len(), 1);
    assert!(h.effects.is_empty());
}
#[test]
fn controlled_aggro_slave_and_pet_are_skipped_but_controlled_death_is_not() {
    // Pets skip every action, controlled creatures skip only aggro; cpp:544-546,562-563,585-590.
    let (mut h, source, controlled) = entry_host(1 | 0x0020, 0);
    let pet = h.actor(7, 3, 0.0);
    h.creatures.get_mut(&pet).unwrap().pet = true;
    h.creatures.get_mut(&controlled).unwrap().controlled = true;
    h.fire(Event::Aggro, source, Some(source)).unwrap();
    assert!(h.effects.is_empty());
    h.fire(Event::Die, source, None).unwrap();
    assert_eq!(h.effects, vec![Effect::Action(controlled, Action::Suicide)]);
}
#[test]
fn reverse_master_uses_first_in_range_even_if_dead_not_nearest_alive() {
    // Selection stops at the first in-range master without an alive predicate; cpp:459-466,499-501.
    let (mut h, _, slave) = entry_host(0x0200, 10);
    let dead = h.actor(8, 3, 5.0);
    let alive = h.actor(8, 4, 1.0);
    // The original master is first too, so make it out of range.
    let original = ObjectHandle {
        thread: 1,
        construction: 1,
    };
    h.creatures.get_mut(&original).unwrap().respawn_xy[0] = 20.0_f32.to_bits();
    h.creatures.get_mut(&dead).unwrap().death = CreatureDeathState::Dead;
    h.fire(Event::Respawn, slave, None).unwrap();
    assert!(h.effects.is_empty());
    assert_ne!(dead, alive);
}
#[test]
fn follow_master_retains_last_out_of_range_lookup_but_reverse_event_does_not() {
    // TryFollowMaster assigns before range testing, unlike DoCreatureLinkingEvent; cpp:462-465,766-776.
    let (mut h, master, slave) = entry_host(0x0200, 1);
    h.creatures.get_mut(&master).unwrap().respawn_xy[0] = 100.0_f32.to_bits();
    h.fire(Event::Respawn, slave, None).unwrap();
    assert!(h.effects.is_empty());
    assert!(h.follow(slave).unwrap());
    assert_eq!(
        h.effects,
        vec![Effect::Action(slave, Action::Follow(master))]
    );
}
#[test]
fn repeated_respawn_guard_uses_strict_fresh_wall_and_zero_delay_short_circuit() {
    // Repeated Respawn writes now and the strict > check suppresses a second call; cpp:593-597.
    let (mut h, source, slave) = entry_host(0x0080 | 0x0100, 0);
    h.creatures.get_mut(&slave).unwrap().death = CreatureDeathState::Corpse;
    h.fire(Event::Respawn, source, None).unwrap();
    h.fire(Event::Respawn, source, None).unwrap();
    assert_eq!(
        h.effects,
        vec![
            Effect::Wall,
            Effect::Action(slave, Action::Respawn),
            Effect::Wall
        ]
    );
    h.effects.clear();
    h.creatures.get_mut(&slave).unwrap().respawn_delay = 0;
    h.fire(Event::Respawn, source, None).unwrap();
    assert_eq!(h.effects, vec![Effect::Action(slave, Action::Respawn)]);
}
#[test]
fn respawn_flag_wins_over_despawn_and_follow_rechecks_combat() {
    // Respawn/despawn are else-if, follow is a separate alive/noncombat check; cpp:593-603.
    let (mut h, source, slave) = entry_host(0x0080 | 0x0100 | 0x0200, 5);
    h.fire(Event::Respawn, source, None).unwrap();
    assert_eq!(
        h.effects,
        vec![Effect::Action(slave, Action::Follow(source))]
    );
    h.effects.clear();
    h.creatures.get_mut(&slave).unwrap().combat = true;
    h.fire(Event::Respawn, source, None).unwrap();
    assert!(h.effects.is_empty());
}
#[test]
fn suicide_despawn_respawn_combination_rechecks_liveness_after_each_callback() {
    // Suicide makes the slave dead before the independent despawn/respawn predicates; cpp:585-590.
    let (mut h, source, slave) = entry_host(0x0020 | 0x0010 | 0x0040, 0);
    h.fire(Event::Die, source, None).unwrap();
    assert_eq!(
        h.effects,
        vec![
            Effect::Action(slave, Action::Suicide),
            Effect::Action(slave, Action::Respawn)
        ]
    );
}
#[test]
fn evade_and_despawn_recheck_state_in_source_order() {
    // ForcedDespawn precedes respawn, and evade needs the resulting alive state; cpp:577-582,607-608.
    let (mut h, source, slave) = entry_host(0x1000 | 0x0004 | 0x4000 | 0x2000, 0);
    h.fire(Event::Evade, source, None).unwrap();
    assert_eq!(
        h.effects,
        vec![
            Effect::Action(slave, Action::ForcedDespawn),
            Effect::Action(slave, Action::Respawn)
        ]
    );
    h.effects.clear();
    h.fire(Event::Despawn, source, None).unwrap();
    assert!(h.effects.is_empty());
    h.creatures.get_mut(&slave).unwrap().death = CreatureDeathState::Corpse;
    h.fire(Event::Despawn, source, None).unwrap();
    assert_eq!(
        h.effects,
        vec![Effect::Action(slave, Action::ForcedDespawn)]
    );
}
#[test]
fn reverse_new_system_evade_schedules_absent_or_corpse_master() {
    // Reverse scheduling tests source system and uses masterDBGuid, not runtime GUID; cpp:475-497.
    for corpse in [false, true] {
        let mut d = Definitions::default();
        d.spawns.insert(1, spawn(7, 0, 0.0));
        d.spawns.insert(2, spawn(8, 0, 0.0));
        let mut h = Host::new(vec![], vec![guid_row(1, 2, 8)], &mut d);
        let slave = h.actor(7, 1, 0.0);
        h.creatures.get_mut(&slave).unwrap().new_spawning = true;
        if corpse {
            let master = h.actor(8, 2, 0.0);
            h.creatures.get_mut(&master).unwrap().death = CreatureDeathState::Corpse;
        }
        h.fire(Event::Evade, slave, None).unwrap();
        assert_eq!(h.effects, vec![Effect::Schedule(2, 1)]);
    }
}
#[test]
fn callback_append_obeys_preincremented_end_iterator() {
    // An append is visited only if the preincremented iterator was not already end; cpp:542-550.
    for extra in [false, true] {
        let (mut h, source, first) = entry_host(1, 0);
        let second = if extra {
            Some(h.actor(7, 3, 0.0))
        } else {
            None
        };
        h.mutation = Some(Mutation::Append(first, 7));
        h.fire(Event::Aggro, source, Some(source)).unwrap();
        let mut expected = vec![Effect::Action(first, Action::AttackStart(source))];
        if let Some(second) = second {
            expected.push(Effect::Action(second, Action::AttackStart(source)));
            expected.push(Effect::Action(first, Action::AttackStart(source)));
        }
        assert_eq!(h.effects, expected);
    }
}
#[test]
fn callback_removing_next_creature_causes_lazy_pruning_on_next_lookup() {
    // The list advances before callback but resolves the next creature afterwards; cpp:528-542.
    let (mut h, source, first) = entry_host(1, 0);
    let second = h.actor(7, 3, 0.0);
    h.mutation = Some(Mutation::Remove(second));
    h.fire(Event::Aggro, source, Some(source)).unwrap();
    assert_eq!(
        h.effects,
        vec![Effect::Action(first, Action::AttackStart(source))]
    );
    assert_eq!(h.scene.holder.inventory().entries[&8][0].slaves.len(), 1);
}
#[test]
fn missing_f5_actions_refuse_without_clearing_flags_or_retrying() {
    // Reached F5 calls are not flag clearing or successful empty callbacks; cpp:560-603.
    for (flags, event, combat, expected) in [
        (
            1,
            Event::Aggro,
            false,
            "execution.linking.f5_ai_attack_start_owner",
        ),
        (1, Event::Aggro, true, "execution.linking.f5_threat_owner"),
        (
            0x4000,
            Event::Evade,
            false,
            "execution.linking.f5_ai_evade_owner",
        ),
        (
            0x0020,
            Event::Die,
            false,
            "execution.linking.f5_combat_suicide_owner",
        ),
        (
            0x0200,
            Event::Respawn,
            false,
            "execution.linking.f5_motion_follow_owner",
        ),
    ] {
        let (mut h, source, slave) = entry_host(flags, 5);
        h.refuse = true;
        h.creatures.get_mut(&slave).unwrap().combat = combat;
        assert_eq!(
            h.fire(event, source, Some(source)).unwrap_err().id,
            expected
        );
        assert_eq!(h.scene.holder.inventory().entries[&8][0].flags, flags);
        assert!(h.scene.holder.inventory().entries[&8][0].in_use);
        assert_eq!(
            h.fire(event, source, Some(source)).unwrap_err().id,
            "execution.linking.events_stopped"
        );
    }
    assert_eq!(
        Action::TriggerCombatTimer(ObjectHandle {
            thread: 1,
            construction: 1
        })
        .missing_owner()
        .id,
        "execution.linking.f5_combat_owner"
    );
}
#[test]
fn executable_range_overflow_and_wrong_clock_kind_refuse() {
    // Preserve F4.03's undefined range boundary and reject a non-wall loop clock; cpp:596,659.
    let (mut h, source, _) = entry_host(1, 46341);
    assert_eq!(
        h.fire(Event::Aggro, source, Some(source)).unwrap_err().id,
        "execution.linking.range_square_undefined"
    );
    let (mut h, source, slave) = entry_host(0x0080, 0);
    h.creatures.get_mut(&slave).unwrap().death = CreatureDeathState::Dead;
    h.clock_kind = ClockKind::MonotonicNanoseconds;
    assert_eq!(
        h.fire(Event::Respawn, source, None).unwrap_err().id,
        "execution.linking.wall_clock_kind"
    );
}
