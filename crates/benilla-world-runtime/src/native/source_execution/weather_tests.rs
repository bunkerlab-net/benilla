// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use benilla_sim::{
    source_rng::{DrawCursor, Mt19937, SourceRandom, TraceEvent},
    source_weather::{Context, Random, RAIN},
};
use std::collections::{BTreeMap, VecDeque};
fn map() -> MapIdentity {
    MapIdentity {
        map: 0,
        instance: 0,
        creation_generation: 1,
    }
}
fn definitions(zones: &[u32]) -> SourceWeatherDefinitions {
    let mut d = SourceWeatherDefinitions::default();
    for (ordinal, &zone) in zones.iter().enumerate() {
        d.insert(WeatherChanceRow {
            ordinal: ordinal as u64,
            zone,
            seasons: [[100, 0, 0]; 4],
        });
    }
    d
}
fn player(objects: &mut SourceObjects) -> ObjectHandle {
    let player = objects.construct(1, SourceObjectKind::Player).unwrap();
    objects
        .bind_map(player, map(), &SourceCells::new(map()).unwrap())
        .unwrap();
    objects.link_map_player(player).unwrap();
    player
}
#[derive(Default)]
struct Owner {
    zones: BTreeMap<ObjectHandle, u32>,
    packets: Vec<(ObjectHandle, [u8; 13])>,
    calls: Vec<String>,
    reject: bool,
}
impl WeatherPacketOwner for Owner {
    fn zone_id(&mut self, player: ObjectHandle, _: &SourceObjects) -> Result<u32> {
        self.calls.push("terrain".into());
        self.zones
            .get(&player)
            .copied()
            .ok_or_else(|| refuse("authored.terrain_missing"))
    }
    fn send_weather(&mut self, player: ObjectHandle, payload: [u8; 13]) -> Result<()> {
        self.calls.push("packet".into());
        self.packets.push((player, payload));
        Ok(())
    }
}
impl PlayerZoneWeatherOwner for Owner {
    fn zone_exists(&mut self, zone: u32) -> Result<bool> {
        Ok(zone != 999)
    }
    fn before_zone_weather(&mut self, _: ObjectHandle, old: u32, zone: u32) -> Result<()> {
        self.calls.push(format!("before {old} {zone}"));
        if self.reject {
            Err(refuse("authored.callback"))
        } else {
            Ok(())
        }
    }
    fn after_zone_weather(&mut self, _: ObjectHandle, zone: u32, area: u32) -> Result<()> {
        self.calls.push(format!("after {zone} {area}"));
        Ok(())
    }
}
impl SourceMapEndWork for Owner {
    fn map_end_phase(
        &mut self,
        phase: MapEndPhase,
        _: MapIdentity,
        _: u32,
        _: &mut SourceObjects,
    ) -> Result<()> {
        self.calls.push(format!("{phase:?}"));
        if self.reject {
            Err(refuse("authored.phase"))
        } else {
            Ok(())
        }
    }
}
struct ContextCalls {
    rolls: VecDeque<u32>,
    days: u32,
}
impl Random for ContextCalls {
    type Error = Refusal;
    fn urand(&mut self, _: u32, _: u32) -> Result<u32> {
        Ok(self.rolls.pop_front().expect("unexpected weather draw"))
    }
    fn rand_norm_f(&mut self) -> Result<f32> {
        panic!("unexpected float draw")
    }
}
impl Context for ContextCalls {
    fn year_day(&mut self) -> Result<u32> {
        self.days += 1;
        Ok(78)
    }
}
fn no_draw() -> ContextCalls {
    ContextCalls {
        rolls: VecDeque::new(),
        days: 0,
    }
}

#[test]
fn lazy_creation_sends_fine_without_clock_or_random_and_obeys_zone_callback_order() {
    let mut w = SourceMapWeather::new(map(), true, 100).unwrap();
    let mut objects = SourceObjects::default();
    let p = player(&mut objects);
    let mut owner = Owner::default();
    w.player_zone_changed(p, 1, 2, false, &mut objects, &definitions(&[1]), &mut owner)
        .unwrap();
    assert_eq!(owner.calls, ["before 0 1", "packet", "after 1 2"]);
    assert_eq!(owner.packets, [(p, [0; 13])]);
    let inventory = w.inventory();
    let zone = &inventory.zones_in_source_order[0];
    assert_eq!(
        (
            zone.zone,
            zone.kind,
            zone.grade_bits,
            zone.permanent,
            zone.current
        ),
        (1, 0, 0, false, 0)
    );
    assert_eq!(objects.get(p).unwrap().zone_update_id, 1);
    owner.calls.clear();
    w.player_zone_changed(p, 1, 3, false, &mut objects, &definitions(&[1]), &mut owner)
        .unwrap();
    assert_eq!(owner.calls, ["after 1 3"]);
    w.player_zone_changed(p, 1, 3, true, &mut objects, &definitions(&[1]), &mut owner)
        .unwrap();
    assert_eq!(owner.packets.len(), 2);
    w.player_zone_changed(
        p,
        999,
        3,
        true,
        &mut objects,
        &definitions(&[1]),
        &mut owner,
    )
    .unwrap();
    assert_eq!(objects.get(p).unwrap().zone_update_id, 1);
}
#[test]
fn disabled_weather_does_not_create_but_still_updates_player_zone() {
    let mut w = SourceMapWeather::new(map(), false, 100).unwrap();
    let mut objects = SourceObjects::default();
    let p = player(&mut objects);
    let mut owner = Owner::default();
    w.player_zone_changed(p, 1, 1, false, &mut objects, &definitions(&[1]), &mut owner)
        .unwrap();
    assert!(w.inventory().zones_in_source_order.is_empty());
    assert!(owner.packets.is_empty());
    assert_eq!(objects.get(p).unwrap().zone_update_id, 1);
    owner.zones.insert(p, 1);
    // Script SetWeather still creates and updates even when the player weather config is disabled.
    w.set_weather(2, RAIN, 0.5, true, &definitions(&[2]), &objects, &mut owner)
        .unwrap();
    w.update(100, &objects, &mut no_draw(), &mut owner).unwrap();
    assert_eq!(w.inventory().zones_in_source_order[0].zone, 2);
}
#[test]
fn changed_empty_zone_deletes_but_unchanged_and_permanent_empty_zones_retain() {
    let mut w = SourceMapWeather::new(map(), true, 100).unwrap();
    let mut objects = SourceObjects::default();
    let p = player(&mut objects);
    let mut owner = Owner::default();
    for zone in [1, 2, 3] {
        w.player_zone_changed(
            p,
            zone,
            zone,
            false,
            &mut objects,
            &definitions(&[1, 2, 3]),
            &mut owner,
        )
        .unwrap();
    }
    owner.zones.insert(p, 3);
    w.set_weather(3, RAIN, 0.5, true, &definitions(&[]), &objects, &mut owner)
        .unwrap();
    w.set_weather(2, RAIN, 0.5, false, &definitions(&[]), &objects, &mut owner)
        .unwrap();
    objects.unlink_map_player(p).unwrap();
    let mut c = ContextCalls {
        rolls: [29, 29].into(),
        days: 0,
    };
    // Weather.cpp:83,105 retains permanent and unchanged zones even without players.
    w.update(100, &objects, &mut c, &mut owner).unwrap();
    assert_eq!(w.inventory().zones_in_source_order.len(), 3);
    let mut c = ContextCalls {
        rolls: [60, 29].into(),
        days: 0,
    };
    w.update(100, &objects, &mut c, &mut owner).unwrap();
    assert_eq!(
        w.inventory()
            .zones_in_source_order
            .iter()
            .map(|v| v.zone)
            .collect::<Vec<_>>(),
        [3, 1]
    );
    assert_eq!(c.days, 1);
    assert!(c.rolls.is_empty());
}
#[test]
fn send_queries_live_terrain_zone_not_update_zone_cache_and_retains_map_reference_order() {
    let mut w = SourceMapWeather::new(map(), true, 100).unwrap();
    let mut objects = SourceObjects::default();
    let a = player(&mut objects);
    let b = player(&mut objects);
    let mut owner = Owner::default();
    owner.zones.insert(a, 7);
    owner.zones.insert(b, 7);
    assert_eq!(objects.get(a).unwrap().zone_update_id, 0);
    assert!(!objects.get(a).unwrap().in_world);
    w.set_weather(
        7,
        RAIN,
        0.5,
        false,
        &definitions(&[7]),
        &objects,
        &mut owner,
    )
    .unwrap();
    assert_eq!(
        owner.packets.iter().map(|v| v.0).collect::<Vec<_>>(),
        [b, a]
    );
    let mut c = ContextCalls {
        rolls: [60].into(),
        days: 0,
    };
    w.update(100, &objects, &mut c, &mut owner).unwrap();
    assert_eq!(w.inventory().zones_in_source_order.len(), 1);
}
#[test]
fn set_same_values_updates_permanence_without_packet_or_empty_zone_deletion() {
    let mut w = SourceMapWeather::new(map(), true, 100).unwrap();
    let objects = SourceObjects::default();
    let mut owner = Owner::default();
    w.set_weather(
        1,
        RAIN,
        0.5,
        false,
        &definitions(&[1]),
        &objects,
        &mut owner,
    )
    .unwrap();
    w.set_weather(1, RAIN, 0.5, true, &definitions(&[1]), &objects, &mut owner)
        .unwrap();
    w.update(100, &objects, &mut no_draw(), &mut owner).unwrap();
    assert!(w.inventory().zones_in_source_order[0].permanent);
    w.set_weather(
        1,
        RAIN,
        0.5,
        false,
        &definitions(&[1]),
        &objects,
        &mut owner,
    )
    .unwrap();
    assert!(!w.inventory().zones_in_source_order[0].permanent);
    assert!(owner.packets.is_empty());
}
#[test]
fn map_end_runs_field_grid_scripts_instance_then_weather_once_not_per_elapsed_interval() {
    let mut w = SourceMapWeather::new(map(), true, 100).unwrap();
    let mut objects = SourceObjects::default();
    let mut owner = Owner::default();
    w.set_weather(1, RAIN, 0.5, true, &definitions(&[1]), &objects, &mut owner)
        .unwrap();
    update_source_map_end(&mut w, &mut objects, &mut no_draw(), &mut owner, 350).unwrap();
    assert_eq!(
        owner.calls,
        ["ObjectFields", "GridStates", "Scripts", "Instance"]
    );
    assert_eq!(w.inventory().zones_in_source_order[0].current, 250);
    owner.reject = true;
    assert!(update_source_map_end(&mut w, &mut objects, &mut no_draw(), &mut owner, 1).is_err());
    assert_eq!(w.inventory().zones_in_source_order[0].current, 250);
    assert!(w.inventory().stopped);
}
#[test]
fn multiple_zones_consume_one_worker_stream_in_source_unordered_order() {
    let mut w = SourceMapWeather::new(map(), true, 100).unwrap();
    let mut objects = SourceObjects::default();
    let p = player(&mut objects);
    let mut owner = Owner::default();
    for zone in [1, 2] {
        w.player_zone_changed(
            p,
            zone,
            0,
            false,
            &mut objects,
            &definitions(&[1, 2]),
            &mut owner,
        )
        .unwrap();
    }
    assert_eq!(
        w.inventory()
            .zones_in_source_order
            .iter()
            .map(|v| v.zone)
            .collect::<Vec<_>>(),
        [2, 1]
    );
    owner.zones.insert(p, 1);
    let mut engine = Mt19937::from_seed(1234);
    let mut cursor = DrawCursor::default();
    let mut events = Vec::<TraceEvent>::new();
    {
        let mut sink = |e| events.push(e);
        let mut random = SourceRandom::new(17, &mut engine, &mut cursor, &mut sink).unwrap();
        let mut context = WeatherExecution {
            random: &mut random,
            world_calendar: &Calendar::new(CalendarOwner::World, 0),
        };
        w.update(100, &objects, &mut context, &mut owner).unwrap();
    }
    assert_eq!(cursor.raw, 4);
    assert!(events.iter().all(|event| event.stream == 17));
    let inventory = w.inventory();
    assert_eq!(inventory.zones_in_source_order[0].kind, 0);
    assert_eq!(inventory.zones_in_source_order[1].kind, RAIN);
}
#[test]
fn callback_refusal_stops_weather_before_creation_or_later_effects() {
    let mut w = SourceMapWeather::new(map(), true, 100).unwrap();
    let mut objects = SourceObjects::default();
    let p = player(&mut objects);
    let mut owner = Owner {
        reject: true,
        ..Owner::default()
    };
    assert_eq!(
        w.player_zone_changed(p, 1, 0, false, &mut objects, &definitions(&[1]), &mut owner)
            .unwrap_err()
            .id,
        "authored.callback"
    );
    assert!(w.inventory().stopped);
    assert!(w.inventory().zones_in_source_order.is_empty());
    assert_eq!(objects.get(p).unwrap().zone_update_id, 0);
}
#[test]
fn retained_f2_weather_catalog_has_thirty_three_definitions_or_explicit_absence() {
    let Some(root) = std::env::var_os("BENILLA_F2_CATALOG") else {
        println!("SKIP retained weather catalog: set BENILLA_F2_CATALOG and BENILLA_F2_EVENT_OCCURRENCES");
        return;
    };
    let occurrences = std::env::var_os("BENILLA_F2_EVENT_OCCURRENCES")
        .expect("explicit catalog requires occurrences");
    let mut catalog =
        super::super::production_compiler::VerifiedCatalog::open(Path::new(&root), false).unwrap();
    let d = SourceWeatherDefinitions::load(&mut catalog, Path::new(&occurrences)).unwrap();
    assert_eq!(d.len(), 33);
}

#[test]
fn missing_chance_changed_empty_weather_deletes_without_random_or_calendar() {
    let mut weather = SourceMapWeather::new(map(), true, 100).unwrap();
    let objects = SourceObjects::default();
    let mut owner = Owner::default();
    weather
        .set_weather(7, RAIN, 0.5, false, &definitions(&[]), &objects, &mut owner)
        .unwrap();
    let mut context = no_draw();
    weather
        .update(100, &objects, &mut context, &mut owner)
        .unwrap();
    assert!(weather.inventory().zones_in_source_order.is_empty());
    assert_eq!(context.days, 0);
}

#[test]
fn invalid_localtime_refuses_after_initial_draw_without_inventing_calendar_values() {
    let mut weather = SourceMapWeather::new(map(), true, 100).unwrap();
    let objects = SourceObjects::default();
    let mut owner = Owner::default();
    weather
        .set_weather(
            1,
            RAIN,
            0.5,
            false,
            &definitions(&[1]),
            &objects,
            &mut owner,
        )
        .unwrap();
    let mut engine = Mt19937::from_seed(5489);
    let mut cursor = DrawCursor::default();
    let mut sink = |_| {};
    {
        let mut random = SourceRandom::new(1, &mut engine, &mut cursor, &mut sink).unwrap();
        let mut context = WeatherExecution {
            random: &mut random,
            world_calendar: &Calendar::new(CalendarOwner::World, i64::MAX),
        };
        assert_eq!(
            weather
                .update(100, &objects, &mut context, &mut owner)
                .unwrap_err()
                .id,
            "execution.weather.localtime_null_undefined"
        );
    }
    assert_eq!(cursor.raw, 1);
    assert!(weather.inventory().stopped);
    assert_eq!(
        weather.inventory().zones_in_source_order[0].grade_bits,
        0x3f000000
    );
}
