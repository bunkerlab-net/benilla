// SPDX-License-Identifier: GPL-3.0-or-later
//! AUTHORED geometry/DBC replies exercise enforcement, not actual asset capture.
use super::*;

fn row(spell: u32, area: u32, aura_spell: i32) -> SpellAreaSourceRow {
    SpellAreaSourceRow {
        spell,
        area,
        quest_start: 0,
        quest_start_active: 0,
        quest_end: 0,
        condition_id: 0,
        aura_spell,
        racemask: 0,
        gender: 2,
        autocast: 0,
    }
}
fn profile_with_rows(rows: Vec<SpellAreaSourceRow>) -> Profile {
    let (mut p, initial, mut source) = inputs();
    let rows = SpellAreaSourceRows::CompleteTable(rows);
    source.modifier_hooks.spell_area = rows.clone();
    source.spells[0].hooks.spell_area = rows;
    p.initialize_spell_authority(&initial, source).unwrap();
    fixture::initialize_authored_location(&mut p);
    p
}
fn from_profile(p: Profile) -> NativeWorld {
    NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 1).unwrap()
}
struct LocationAuthority {
    base: fixture::SpatialOnly,
    origin: ai::Point,
    reject_movement: bool,
    destination_flag: u16,
    corrupt: Option<&'static str>,
    locations: usize,
}
impl LocationAuthority {
    fn new(w: &NativeWorld) -> Self {
        Self {
            base: fixture::spatial_only(),
            origin: w.state.position,
            reject_movement: false,
            destination_flag: 7,
            corrupt: None,
            locations: 0,
        }
    }
}
impl Authority for LocationAuthority {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        if let Query::SourceLocation(query) = &request.query {
            self.locations += 1;
            let flag = if query.pose().position == self.origin {
                7
            } else {
                self.destination_flag
            };
            let mut facts =
                SourceLocationFacts::new(query, &fixture::authored_location_catalog(), flag)?;
            if let Some(field) = self.corrupt {
                let mut value = serde_json::to_value(&facts).unwrap();
                match field {
                    "area" => value["area"]["id"] = 148.into(),
                    "tick" => value["query"]["tick"] = 99.into(),
                    "pose" => value["query"]["pose"]["position"]["x"] = f::real(99.0).bits().into(),
                    "source" => value["query"]["source"][0] = 99.into(),
                    _ => unreachable!(),
                }
                facts = serde_json::from_value(value).unwrap();
            }
            return Ok(AuthorityReply {
                request: request.clone(),
                facts: Facts::SourceLocation(facts),
            });
        }
        if self.reject_movement && matches!(request.query, Query::PlayerMovement { .. }) {
            return Ok(AuthorityReply {
                request: request.clone(),
                facts: Facts::PlayerMovement { accepted: false },
            });
        }
        self.base.resolve(request)
    }
    fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
        self.base.observe_derived(reply)
    }
}

#[test]
fn full_raw_without_source_cannot_run_first_frame_or_cast() {
    let mut p = profile();
    p.interactions
        .as_mut()
        .unwrap()
        .spell_authority
        .as_mut()
        .unwrap()
        .source_location = None;
    let mut w = from_profile(p);
    let before = w.checkpoint().unwrap();
    let mut a = fixture::spatial_only();
    assert!(matches!(
        w.step(1, None, &mut a),
        Err(NativeError::AuthorityUnavailable(
            "source-owned current location"
        ))
    ));
    assert_eq!(w.checkpoint().unwrap(), before);
    let cast = f::cast(&w, 133);
    assert!(w.step(1, Some(cast), &mut a).is_err());
    assert_eq!(w.checkpoint().unwrap(), before);
    assert!(a.queries.is_empty());
}

#[test]
fn global_nonempty_table_is_scoped_to_actual_location_and_all_spell_aura_keys() {
    // Source-audited installer row values; geometry and client stores remain authored.
    let mut rows = vec![row(6298, 148, 0)];
    for area in [139, 2057, 2017, 3456] {
        let mut r = row(31906, area, 30238);
        r.condition_id = 1399;
        r.gender = 0;
        r.autocast = 1;
        rows.push(r);
    }
    // Global area0 is absent from the loader's area index, not from its spell index.
    rows.push(row(60099, 0, 0));
    let mut w = from_profile(profile_with_rows(rows));
    let mut a = LocationAuthority::new(&w);
    let cast = f::cast(&w, 133);
    w.step(1, Some(cast), &mut a).unwrap();
    let record = w.last_tick().unwrap();
    assert!(matches!(
        record.replies[0].request.query,
        Query::SourceLocation(_)
    ));
    let facts = w.state.source_location.as_ref().unwrap();
    assert_eq!((facts.zone_id(), facts.area_id()), (12, 9));
    assert_eq!(a.locations, 1);
    for bad in [
        row(133, 999, 0),
        row(60099, 9, 0),
        row(60099, 12, 0),
        row(60099, 999, -133),
        row(133, 0, 0),
    ] {
        let mut w = from_profile(profile_with_rows(vec![bad]));
        let before = w.checkpoint().unwrap();
        let mut a = LocationAuthority::new(&w);
        assert_eq!(
            w.step(1, None, &mut a).err(),
            Some(NativeError::UnsupportedMechanic(
                "applicable spell_area source row"
            ))
        );
        assert_eq!(w.checkpoint().unwrap(), before);
    }
}

#[test]
fn all_hook_copies_must_retain_the_same_complete_table() {
    let (mut p, initial, mut source) = inputs();
    source.modifier_hooks.spell_area = SpellAreaSourceRows::Unknown;
    assert!(matches!(
        p.initialize_spell_authority(&initial, source),
        Err(NativeError::AuthorityUnavailable(_))
    ));
    let (mut p, initial, mut source) = inputs();
    source.modifier_hooks.spell_area = SpellAreaSourceRows::CompleteTable(vec![row(6298, 148, 0)]);
    assert_eq!(
        p.initialize_spell_authority(&initial, source),
        Err(NativeError::AuthorityBinding)
    );
}

#[test]
fn uncovered_proposed_pose_is_atomic_and_blocked_movement_keeps_current_witness() {
    let mut w = from_profile(profile_with_rows(vec![row(6298, 148, 0)]));
    let mut a = LocationAuthority::new(&w);
    advance(&mut w, &mut a);
    let before = w.checkpoint().unwrap();
    let input = |w: &NativeWorld| {
        let mut i = f::cast(w, 133);
        let mut destination = w.state.position;
        destination.x = f::real(destination.x.as_f32() + 0.1);
        i.input.command = Command::MovementPose {
            destination,
            orientation: w.state.orientation,
            movement_flags: w.state.player_movement_flags.unwrap(),
        };
        i
    };
    a.destination_flag = 9; // Authored area148 is outside this admitted source scope.
    let movement = input(&w);
    assert_eq!(
        w.step(2, Some(movement), &mut a).err(),
        Some(NativeError::UnsupportedMechanic(
            "applicable spell_area source row"
        ))
    );
    assert_eq!(w.checkpoint().unwrap(), before);
    let queried = a.locations;
    a.reject_movement = true;
    let movement = input(&w);
    w.step(2, Some(movement), &mut a).unwrap();
    assert_eq!(a.locations, queried + 1); // Current pose only; rejected endpoint has no effects.
    assert_eq!(w.state.position, a.origin);
    assert_eq!(w.state.source_location.as_ref().unwrap().area_id(), 9);
    w.checkpoint().unwrap();
}

#[test]
fn exact_query_and_catalog_reject_stale_or_forged_facts_atomically() {
    for field in ["area", "tick", "pose", "source"] {
        let mut w = world(1);
        let before = w.checkpoint().unwrap();
        let mut a = LocationAuthority::new(&w);
        a.corrupt = Some(field);
        assert!(w.step(1, None, &mut a).is_err(), "{field}");
        assert_eq!(w.checkpoint().unwrap(), before);
    }
}

#[test]
fn checkpoint_requires_current_witness_and_replay_requeries_exact_binding() {
    let mut w = world(1);
    let genesis = w.checkpoint().unwrap();
    let mut a = LocationAuthority::new(&w);
    advance(&mut w, &mut a);
    let cp = w.checkpoint().unwrap();
    for field in [
        "absent", "null", "tick", "position", "area", "source", "scope",
    ] {
        let mut value = serde_json::to_value(&cp).unwrap();
        match field {
            "absent" => {
                value["state"]
                    .as_object_mut()
                    .unwrap()
                    .remove("source_location");
            }
            "null" => value["state"]["source_location"] = serde_json::Value::Null,
            "tick" => value["state"]["source_location"]["query"]["tick"] = 2.into(),
            "position" => {
                value["state"]["source_location"]["query"]["pose"]["position"]["x"] =
                    f::real(99.0).bits().into()
            }
            "area" => value["state"]["source_location"]["area"]["id"] = 148.into(),
            "source" => value["state"]["source_location"]["query"]["source"][0] = 99.into(),
            "scope" => {
                value["state"]["source_location"]["query"]["spell_area_scope"][0] = 99.into()
            }
            _ => unreachable!(),
        }
        if field == "absent" {
            assert!(serde_json::from_value::<Checkpoint>(value).is_err());
            continue;
        }
        assert!(
            NativeWorld::restore_synthetic_component_test(w.profile.clone(), rehash(value))
                .is_err(),
            "{field}"
        );
    }
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), genesis).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
    let mut restored =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), cp).unwrap();
    let mut missing = MissingLocation;
    let before = restored.checkpoint().unwrap();
    assert!(restored.step(2, None, &mut missing).is_err());
    assert_eq!(restored.checkpoint().unwrap(), before);
}
struct MissingLocation;
impl Authority for MissingLocation {
    fn resolve(&mut self, _: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        Err(NativeError::AuthorityUnavailable(
            "authored missing location",
        ))
    }
}
