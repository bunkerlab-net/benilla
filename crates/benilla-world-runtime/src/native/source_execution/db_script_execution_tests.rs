use super::*;
use std::collections::{BTreeMap, VecDeque};

pub(super) fn map_id() -> MapIdentity {
    MapIdentity {
        map: 0,
        instance: 0,
        creation_generation: 1,
    }
}
pub(super) struct Fixture {
    pub objects: SourceObjects,
    pub units: SourceUnits,
    pub host: Host,
    pub h: [ObjectHandle; 5],
}
pub(super) fn fixture() -> Fixture {
    let mut objects = SourceObjects::default();
    let mut units = SourceUnits::default();
    let h = std::array::from_fn(|i| {
        let kind = match i {
            3 => SourceObjectKind::Player,
            4 => SourceObjectKind::GameObject,
            _ => SourceObjectKind::Creature,
        };
        let h = objects.construct(1, kind).unwrap();
        let guid = match kind {
            SourceObjectKind::Player => SourceGuid::from_raw(4),
            SourceObjectKind::GameObject => SourceGuid::with_entry(HighGuid::GameObject, 1901, 5),
            _ => SourceGuid::with_entry(HighGuid::Creature, 901, i as u32 + 1),
        };
        objects.assign_guid(h, i as u32 + 1, guid).unwrap();
        objects.get_mut(h).unwrap().map = Some(map_id());
        objects
            .set_pose(h, SourcePose([(i as f32).to_bits(), 0, 0, 0]))
            .unwrap();
        if kind == SourceObjectKind::Player {
            objects.register_player_lookup(h).unwrap();
        }
        objects.add_to_world(h).unwrap();
        if kind != SourceObjectKind::GameObject {
            let mut fields =
                SourceUnitFields::creature(&super::super::motion_master_tests::template());
            fields.has_ai = true;
            units.register(&objects, h, fields).unwrap();
        }
        h
    });
    let entries = h
        .into_iter()
        .enumerate()
        .map(|(i, h)| (h, if i == 4 { 1901 } else { 901 }))
        .collect();
    let position = h
        .into_iter()
        .enumerate()
        .map(|(i, h)| (h, i as f32))
        .collect();
    Fixture {
        objects,
        units,
        h,
        host: Host {
            entries,
            position,
            spells: BTreeMap::from([(1, (false, 0))]),
            go_state: 1,
            ..Host::default()
        },
    }
}

#[derive(Default)]
pub(super) struct Host {
    pub now: i64,
    pub effects: Vec<(DbScriptCall, Option<ObjectHandle>, Option<ObjectHandle>)>,
    pub calls: Vec<DbScriptCall>,
    pub fields: BTreeMap<(ObjectHandle, u32), u32>,
    pub entries: BTreeMap<ObjectHandle, u32>,
    pub members: Vec<ObjectHandle>,
    pub visitor: Vec<ObjectHandle>,
    pub draws: Vec<(u32, u32)>,
    pub words: VecDeque<u32>,
    pub received: Vec<(ObjectHandle, ObjectHandle, Option<ObjectHandle>, u32, u32)>,
    pub condition: Option<bool>,
    pub geometry: bool,
    pub position: BTreeMap<ObjectHandle, f32>,
    pub walk: bool,
    pub event_ai: Option<SourceEventAi>,
    pub event_words: u32,
    pub spells: BTreeMap<u32, (bool, u32)>,
    pub go_state: u32,
    pub formation: Option<bool>,
    pub corpses: std::collections::BTreeSet<ObjectHandle>,
}
impl AssistanceExecutionOwner for Host {
    fn receive_ai_event(
        &mut self,
        receiver: ObjectHandle,
        sender: ObjectHandle,
        enemy: Option<ObjectHandle>,
        event: SourceAiEventType,
        misc: u32,
        _: &mut SourceUnits,
        _: &SourceObjects,
    ) -> Result<()> {
        self.received
            .push((receiver, sender, enemy, event.raw(), misc));
        if let Some(mut ai) = self.event_ai.take() {
            ai.receive_ai_event(event.raw(), sender, enemy, self)?;
            self.event_ai = Some(ai);
        }
        Ok(())
    }
    fn visit(&mut self, _: ObjectHandle, _: u32, _: AssistanceVisit) -> Result<Vec<ObjectHandle>> {
        if !self.geometry {
            return Err(script_missing("f6_grid_visitor"));
        }
        Ok(self.visitor.clone())
    }
    fn query(
        &mut self,
        source: ObjectHandle,
        target: ObjectHandle,
        query: AssistanceQuery,
    ) -> Result<bool> {
        match query {
            AssistanceQuery::Distance(radius) => DbScriptExecutionOwner::within(
                self,
                source,
                target,
                radius,
                DbScriptDistance::Bounding,
            ),
            _ => Err(script_missing("f5_unit_relationship")),
        }
    }
    fn assistance_ai_base(&mut self, _: ObjectHandle) -> Result<AssistanceAiBase> {
        Ok(AssistanceAiBase::Unit)
    }
}
impl CreatureRandom for Host {
    fn creature_irand(&mut self, min: i32, _: i32) -> Result<i32> {
        Ok(min)
    }
    fn creature_urand(&mut self, min: u32, max: u32) -> Result<u32> {
        DbScriptExecutionOwner::urand(self, min, max)
    }
    fn creature_urand_default(&mut self) -> Result<u32> {
        self.event_words += 1;
        Ok(0)
    }
}
impl EventAiExecutionOwner for Host {
    fn current_ranged_mode(&mut self, _: ObjectHandle) -> Result<bool> {
        Ok(false)
    }
    fn can_execute_combat_action(&mut self, _: ObjectHandle) -> Result<bool> {
        Ok(true)
    }
}
impl DbScriptExecutionOwner for Host {
    fn clock_millis(&mut self) -> Result<i64> {
        Ok(self.now)
    }
    fn urand(&mut self, min: u32, max: u32) -> Result<u32> {
        self.draws.push((min, max));
        let n = self.words.pop_front().unwrap_or(min);
        assert!((min..=max).contains(&n));
        Ok(n)
    }
    fn chance(&mut self) -> Result<f64> {
        Ok(75.)
    }
    fn entry(&mut self, object: ObjectHandle) -> Result<u32> {
        Ok(self.entries[&object])
    }
    fn values_count(&mut self, _: ObjectHandle) -> Result<u32> {
        Ok(160)
    }
    fn field(&mut self, object: ObjectHandle, field: u32) -> Result<u32> {
        Ok(*self.fields.get(&(object, field)).unwrap_or(&0))
    }
    fn walking(&mut self, _: ObjectHandle) -> Result<bool> {
        Ok(self.walk)
    }
    fn condition(
        &mut self,
        _: u32,
        _: Option<ObjectHandle>,
        _: Option<ObjectHandle>,
    ) -> Result<bool> {
        self.condition
            .ok_or_else(|| script_missing("f5_22_condition"))
    }
    fn spell_targets(&mut self, id: u32) -> Result<(bool, u32)> {
        self.spells
            .get(&id)
            .copied()
            .ok_or_else(|| script_missing("f5_07_spell_template"))
    }
    fn members(&mut self, _: MapIdentity, _: DbScriptMembers) -> Result<Option<Vec<ObjectHandle>>> {
        Ok(Some(self.members.clone()))
    }
    fn visit(&mut self, _: ObjectHandle, _: u32, _: DbScriptVisit) -> Result<Vec<ObjectHandle>> {
        if !self.geometry {
            return Err(script_missing("f6_grid_visitor"));
        }
        Ok(self.visitor.clone())
    }
    fn distance(
        &mut self,
        source: ObjectHandle,
        target: ObjectHandle,
        mode: DbScriptDistance,
    ) -> Result<u32> {
        if !self.geometry {
            return Err(script_missing("f6_distance"));
        }
        let distance = (self.position[&source] - self.position[&target]).abs();
        Ok((if mode == DbScriptDistance::SquaredNoBounding {
            distance * distance
        } else {
            distance
        })
        .to_bits())
    }
    fn within(
        &mut self,
        source: ObjectHandle,
        target: ObjectHandle,
        range: u32,
        _: DbScriptDistance,
    ) -> Result<bool> {
        if !self.geometry {
            return Err(script_missing("f6_distance"));
        }
        Ok((self.position[&source] - self.position[&target]).abs() < f32::from_bits(range))
    }
    fn formation_member(&mut self, _: ObjectHandle) -> Result<Option<bool>> {
        Ok(self.formation)
    }
    fn is_corpse(&mut self, creature: ObjectHandle) -> Result<bool> {
        Ok(self.corpses.contains(&creature))
    }
    fn dynamic_point(
        &mut self,
        row: &DbScriptRow,
        _: ObjectHandle,
        target: ObjectHandle,
    ) -> Result<[u32; 3]> {
        if !self.geometry || row.data[0] != 0 || row.data[2] != 0 {
            return Err(script_missing("f6_geometry"));
        }
        // Authored zero-radius objects on a flat floor, Object.h:915-918.
        Ok([self.position[&target].to_bits(), 0, 0])
    }
    fn current_motion(&mut self, _: ObjectHandle) -> Result<u32> {
        Ok(0)
    }
    fn has_instance(&mut self, _: ObjectHandle) -> Result<bool> {
        Ok(false)
    }
    fn go_type(&mut self, _: ObjectHandle) -> Result<u32> {
        Ok(1)
    }
    fn go_state(&mut self, _: ObjectHandle) -> Result<u32> {
        Ok(self.go_state)
    }
    fn go_spawned(&mut self, _: ObjectHandle) -> Result<bool> {
        Ok(false)
    }
    fn call(
        &mut self,
        call: DbScriptCall,
        _: &DbScriptRow,
        source: Option<ObjectHandle>,
        target: Option<ObjectHandle>,
        objects: &mut SourceObjects,
        units: &mut SourceUnits,
    ) -> Result<()> {
        self.calls.push(call);
        match call {
            DbScriptCall::WriteField { field, value } => {
                self.fields.insert((source.unwrap(), field), value);
                if field == 0x2e {
                    units.get_mut(objects, source.unwrap())?.flags = value;
                }
            }
            DbScriptCall::SetWalk(walk) => self.walk = walk,
            DbScriptCall::Emote(_)
            | DbScriptCall::DisplayText(_)
            | DbScriptCall::Stand(_)
            | DbScriptCall::Sheathe(_)
            | DbScriptCall::StringId(..)
            | DbScriptCall::WorldState(..) => {}
            _ => return Err(call.missing_owner()),
        }
        // These are the authored host's observable presentation/field notifications.
        self.effects.push((call, source, target));
        Ok(())
    }
}
pub(super) struct Loader;
impl DbScriptLoaderOwner for Loader {
    fn contains(&mut self, _: DbScriptLookup, _: u32) -> Result<bool> {
        Ok(true)
    }
    fn quest_exploration(&mut self, _: u32) -> Result<Option<bool>> {
        Ok(Some(true))
    }
    fn gameobject_entry(&mut self, _: u32) -> Result<Option<u32>> {
        Ok(Some(1901))
    }
    fn gameobject_type(&mut self, _: u32) -> Result<Option<u32>> {
        Ok(Some(1))
    }
    fn max_pool_id(&mut self) -> Result<u32> {
        Ok(65535)
    }
}
pub(super) fn maps(rows: Vec<DbScriptRow>) -> SourceDbScriptMaps {
    SourceDbScriptMaps::load(
        vec![(DbScriptType::Relay, rows)],
        SourceDbScriptTemplates::default(),
        &mut Loader,
    )
    .unwrap()
}
pub(super) fn row(command: u32, a: u32, b: u32) -> DbScriptRow {
    DbScriptRow {
        id: 100,
        command,
        data: [a, b, 0],
        ..DbScriptRow::default()
    }
}
fn run(
    executor: &mut SourceDbScriptExecutor<'_>,
    row: &DbScriptRow,
    source: Option<ObjectHandle>,
    target: Option<ObjectHandle>,
    f: &mut Fixture,
) -> Result<bool> {
    executor.execute_command(
        row,
        source,
        target,
        false,
        None,
        &mut DbScriptContext {
            objects: &mut f.objects,
            units: &mut f.units,
            owner: &mut f.host,
        },
    )
}
fn start(
    executor: &mut SourceDbScriptExecutor<'_>,
    source: ObjectHandle,
    target: Option<ObjectHandle>,
    unique: u8,
    f: &mut Fixture,
) -> Result<bool> {
    executor.start(
        DbScriptType::Relay,
        100,
        source,
        target,
        unique,
        &mut DbScriptContext {
            objects: &mut f.objects,
            units: &mut f.units,
            owner: &mut f.host,
        },
    )
}
fn process(executor: &mut SourceDbScriptExecutor<'_>, f: &mut Fixture) -> Result<()> {
    executor.process(&mut DbScriptContext {
        objects: &mut f.objects,
        units: &mut f.units,
        owner: &mut f.host,
    })
}
#[test]
fn zero_delay_is_synchronous_and_due_queue_is_inclusive_stable_and_clock_based() {
    let mut rows = vec![row(1, 11, 0), row(1, 22, 0), row(1, 33, 0), row(1, 44, 0)];
    for (i, r) in rows.iter_mut().enumerate() {
        r.ordinal = i as u64;
        r.delay = [20, 0, 10, 10][i];
    }
    let maps = maps(rows);
    let mut executor = SourceDbScriptExecutor::new(map_id(), &maps);
    let mut f = fixture();
    f.host.now = 700;
    start(&mut executor, f.h[0], None, 3, &mut f).unwrap();
    assert_eq!(
        f.host.effects.iter().map(|e| e.0).collect::<Vec<_>>(),
        [DbScriptCall::Emote(22)]
    );
    assert_eq!(
        executor.pending().map(|p| (p.0, p.2)).collect::<Vec<_>>(),
        [(710, 2), (710, 3), (720, 0)]
    );
    f.host.now = 709;
    process(&mut executor, &mut f).unwrap();
    assert_eq!(f.host.effects.len(), 1);
    f.host.now = 710;
    process(&mut executor, &mut f).unwrap();
    assert_eq!(
        f.host.effects.iter().map(|e| e.0).collect::<Vec<_>>(),
        [
            DbScriptCall::Emote(22),
            DbScriptCall::Emote(33),
            DbScriptCall::Emote(44)
        ]
    );
    f.host.now = 900;
    process(&mut executor, &mut f).unwrap();
    assert_eq!(f.host.effects.last().unwrap().0, DbScriptCall::Emote(11));
    assert!(executor.pending().next().is_none());
    executor.validate().unwrap();
}
#[test]
fn uniqueness_uses_pending_source_target_wildcards_and_termination_erases_matching_invocations() {
    let mut stop = row(31, 0, 0);
    stop.delay = 10;
    let mut later = row(1, 1, 0);
    later.delay = 20;
    let maps = maps(vec![stop, later]);
    let mut e = SourceDbScriptExecutor::new(map_id(), &maps);
    let mut f = fixture();
    start(&mut e, f.h[0], Some(f.h[1]), 3, &mut f).unwrap();
    start(&mut e, f.h[0], Some(f.h[2]), 1, &mut f).unwrap();
    assert_eq!(e.pending().count(), 2);
    start(&mut e, f.h[0], Some(f.h[2]), 2, &mut f).unwrap();
    assert_eq!(e.pending().count(), 4);
    f.host.now = 10;
    process(&mut e, &mut f).unwrap();
    assert_eq!(e.pending().count(), 0);
    assert!(f.host.effects.is_empty());
}
#[test]
fn zero_delay_termination_prevents_delayed_publication_and_relay_is_synchronous() {
    let maps = maps(vec![
        row(31, 0, 0),
        DbScriptRow {
            delay: 1,
            ..row(1, 1, 0)
        },
    ]);
    let mut e = SourceDbScriptExecutor::new(map_id(), &maps);
    let mut f = fixture();
    start(&mut e, f.h[0], None, 3, &mut f).unwrap();
    assert_eq!(e.pending().count(), 0);
    let relay = SourceDbScriptMaps::load(
        vec![(
            DbScriptType::Relay,
            vec![DbScriptRow {
                id: 101,
                ..row(1, 7, 0)
            }],
        )],
        SourceDbScriptTemplates::default(),
        &mut Loader,
    )
    .unwrap();
    let mut e = SourceDbScriptExecutor::new(map_id(), &relay);
    run(&mut e, &row(45, 101, 0), Some(f.h[0]), None, &mut f).unwrap();
    assert_eq!(f.host.effects.last().unwrap().0, DbScriptCall::Emote(7));
    assert!(!e
        .start(
            DbScriptType::Relay,
            999,
            f.h[0],
            None,
            3,
            &mut DbScriptContext {
                objects: &mut f.objects,
                units: &mut f.units,
                owner: &mut f.host
            }
        )
        .unwrap());
}
#[test]
fn delayed_lookup_uses_live_membership_and_not_the_original_handle_generation() {
    let maps = maps(vec![DbScriptRow {
        delay: 10,
        ..row(1, 7, 0)
    }]);
    let mut e = SourceDbScriptExecutor::new(map_id(), &maps);
    let mut f = fixture();
    start(&mut e, f.h[0], None, 0, &mut f).unwrap();
    f.objects.remove_from_world(f.h[0]).unwrap();
    f.host.now = 10;
    process(&mut e, &mut f).unwrap();
    assert!(f.host.effects.is_empty());
}
#[test]
fn buddy_guid_direction_self_and_missing_life_state_reproduce_source() {
    let r = DbScriptRow {
        buddy: 901,
        radius_or_guid: 2,
        flags: 0x10 | 1 | 2 | 4,
        ..row(1, 7, 0)
    };
    let maps = maps(vec![r]);
    let mut e = SourceDbScriptExecutor::new(map_id(), &maps);
    let mut f = fixture();
    start(&mut e, f.h[0], Some(f.h[3]), 0, &mut f).unwrap();
    assert_eq!(
        f.host.effects[0],
        (DbScriptCall::Emote(7), Some(f.h[1]), Some(f.h[1]))
    );
    f.host.effects.clear();
    f.objects.mark_dead(f.h[1]).unwrap();
    start(&mut e, f.h[0], None, 0, &mut f).unwrap();
    assert!(f.host.effects.is_empty());
}
#[test]
fn entry_buddies_use_nearest_strict_squared_distance_and_all_buddies_keep_visitor_order() {
    let r = DbScriptRow {
        buddy: 902,
        radius_or_guid: 10,
        ..row(1, 7, 0)
    };
    let definitions = maps(vec![r]);
    let mut e = SourceDbScriptExecutor::new(map_id(), &definitions);
    let mut f = fixture();
    f.host.geometry = true;
    f.host.visitor = vec![f.h[2], f.h[1]];
    f.host.entries.insert(f.h[2], 902);
    f.host.entries.insert(f.h[1], 902);
    start(&mut e, f.h[0], None, 0, &mut f).unwrap();
    assert_eq!(f.host.effects[0].1, Some(f.h[1]));
    f.host.effects.clear();
    let r = DbScriptRow {
        buddy: 902,
        radius_or_guid: 10,
        flags: 0x200,
        ..row(1, 9, 0)
    };
    let definitions = maps(vec![r]);
    let mut e = SourceDbScriptExecutor::new(map_id(), &definitions);
    start(&mut e, f.h[0], None, 0, &mut f).unwrap();
    assert_eq!(
        f.host
            .effects
            .iter()
            .map(|e| e.1.unwrap())
            .collect::<Vec<_>>(),
        [f.h[2], f.h[1]]
    );
}

#[path = "db_script_command_tests.rs"]
mod command_tests;

#[path = "db_script_target_tests.rs"]
mod target_tests;
