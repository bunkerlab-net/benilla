// SPDX-License-Identifier: GPL-3.0-or-later
// Authored live-slot operands and pinned numeric oracle, not F1 admission.
use super::*;
use benilla_sim::{
    source_formations::{Position, Random},
    source_rng::{DrawCursor, EventKind, Mt19937, RawGenerator, SourceRandom, TraceEvent},
};
use std::fmt::Write;
struct ShapeHost {
    base: Host,
    widths: BTreeMap<ObjectHandle, f32>,
    width_calls: Vec<ObjectHandle>,
    engine: Mt19937,
    cursor: DrawCursor,
    random_calls: Vec<String>,
    deadline: Option<u32>,
    refuse_width: bool,
    refuse_random: bool,
    refuse_follow: bool,
}
impl Default for ShapeHost {
    fn default() -> Self {
        Self {
            base: Host::default(),
            widths: BTreeMap::new(),
            width_calls: vec![],
            engine: Mt19937::from_seed(5489),
            cursor: DrawCursor::default(),
            random_calls: vec![],
            deadline: None,
            refuse_width: false,
            refuse_random: false,
            refuse_follow: false,
        }
    }
}
impl FormationOwner for ShapeHost {
    fn formation_unit(&mut self, o: ObjectHandle) -> Result<FormationUnit> {
        self.base.formation_unit(o)
    }
    fn formation_creature(&mut self, m: MapIdentity, g: u32) -> Result<Option<ObjectHandle>> {
        self.base.formation_creature(m, g)
    }
    fn formation_out_of_combat(&mut self, g: GroupIdentity) -> Result<bool> {
        self.base.formation_out_of_combat(g)
    }
    fn formation_world_millis(&mut self) -> Result<i64> {
        self.base.formation_world_millis()
    }
    fn formation_waypoint(&mut self, o: ObjectHandle) -> Result<(u32, u32)> {
        self.base.formation_waypoint(o)
    }
    fn formation_follow_target(&mut self, o: ObjectHandle) -> Result<Option<ObjectHandle>> {
        self.base.formation_follow_target(o)
    }
    fn formation_motion(&mut self, o: ObjectHandle, m: FormationMotion) -> Result<()> {
        if self.refuse_follow && matches!(m, FormationMotion::Follow { .. }) {
            self.base.calls.push(Call::Motion(o, m));
            return Err(m.missing_owner());
        }
        self.base.formation_motion(o, m)
    }
    fn formation_random_motion(&mut self, o: ObjectHandle, position: [f32; 4]) -> Result<()> {
        self.base.formation_random_motion(o, position)
    }
    fn formation_collision_width(&mut self, o: ObjectHandle) -> Result<f32> {
        self.width_calls.push(o);
        if self.refuse_width {
            return Err(refuse("execution.formation.f6_collision_width_owner"));
        }
        Ok(*self.widths.get(&o).unwrap_or(&1.25))
    }
    fn formation_frand(&mut self, low: f32, high: f32) -> Result<f32> {
        if self.refuse_random {
            return Err(refuse("execution.formation.f3_thread_rng_owner"));
        }
        let calls = &mut self.random_calls;
        let mut sink = |event: TraceEvent| {
            if let EventKind::Raw { word, .. } = event.kind {
                calls.push(format!("W {word:08x}"));
            }
        };
        let value = SourceRandom::new(1, &mut self.engine, &mut self.cursor, &mut sink)
            .unwrap()
            .frand(low, high)
            .map_err(|_| refuse("authored.random"))?;
        self.random_calls.push(format!(
            "R {:08x} {:08x} {:08x}",
            low.to_bits(),
            high.to_bits(),
            value.to_bits()
        ));
        Ok(value)
    }
    fn formation_urand(&mut self, low: u32, high: u32) -> Result<u32> {
        assert_eq!((low, high), (20000, 50000));
        if let Some(delay) = self.deadline {
            struct Word(u64);
            impl RawGenerator for Word {
                const MIN: u64 = 0;
                const MAX: u64 = u32::MAX as u64;
                fn next_word(&mut self) -> u64 {
                    self.0
                }
            }
            let mut word = Word(if delay == 20000 {
                1
            } else {
                u64::from(u32::MAX)
            });
            let mut sink = |_| {};
            let value = SourceRandom::new(1, &mut word, &mut self.cursor, &mut sink)
                .unwrap()
                .urand(low, high)
                .unwrap();
            assert_eq!(value, delay);
            self.random_calls.push(format!("U {low} {high} {value}"));
            return Ok(value);
        }
        let mut sink = |_| {};
        let value = SourceRandom::new(1, &mut self.engine, &mut self.cursor, &mut sink)
            .unwrap()
            .urand(low, high)
            .map_err(|_| refuse("authored.random"))?;
        self.random_calls.push(format!("U {low} {high} {value}"));
        Ok(value)
    }
}
impl Random for ShapeHost {
    type Error = Refusal;
    fn frand(&mut self, low: f32, high: f32) -> Result<f32> {
        self.formation_frand(low, high)
    }
    fn urand(&mut self, low: u32, high: u32) -> Result<u32> {
        self.formation_urand(low, high)
    }
}
fn shapes(count: u32, shape: u32, compact: bool, pattern: u32) -> (SourceFormations, ShapeHost) {
    let (mut f, base) = fixture(count, false, 0);
    let mut h = ShapeHost {
        base,
        ..ShapeHost::default()
    };
    // Shape writes are driven through the real options/shape command, not a success callback.
    for i in 0..count {
        let object = object(u64::from(i + 1));
        let slot = f.state(group()).unwrap().slots[&i].unwrap();
        h.widths.insert(object, 1.25 + i as f32 * 0.125);
        h.base.units.get_mut(&object).unwrap().alive = pattern != 2 || i.is_multiple_of(2);
        if !((pattern == 1 && !i.is_multiple_of(2)) || pattern == 3) {
            f.slot_mut(slot).unwrap().owner = Some(object);
        }
    }
    // The fixture has no bindings/motion. These commands execute only the source numerical pass.
    f.execute(
        group(),
        FormationCommand::Spread(3.25_f32.to_bits()),
        &mut h,
    )
    .unwrap();
    f.execute(group(), FormationCommand::Shape(shape), &mut h)
        .unwrap();
    if compact {
        f.execute(group(), FormationCommand::Compact(true), &mut h)
            .unwrap();
    }
    h.random_calls.clear();
    h.cursor = DrawCursor::default();
    h.engine = Mt19937::from_seed(5489);
    // Reset numerical storage to the source constructor so the oracle sees one FixSlotsPositions.
    for id in f
        .state(group())
        .unwrap()
        .slots
        .values()
        .flatten()
        .copied()
        .collect::<Vec<_>>()
    {
        f.slot_mut(id).unwrap().position = Position::default();
    }
    (f, h)
}
fn random_fixture(count: u32) -> (SourceFormations, Host) {
    let (base, host) = fixture(count, false, 0);
    let mut entry = base.definition(group()).unwrap().entry.clone();
    entry.kind = 0;
    let mut formations = SourceFormations::default();
    formations
        .create(group(), entry, &base.state(group()).unwrap().members, false)
        .unwrap();
    (formations, host)
}
fn emit(
    f: &SourceFormations,
    h: &mut ShapeHost,
    output: &mut String,
    case: u32,
    phase: u32,
    mirror: bool,
) {
    for call in h.random_calls.drain(..) {
        writeln!(output, "{call}").unwrap();
    }
    for (&index, id) in &f.state(group()).unwrap().slots {
        let p = &f.slot(id.unwrap()).unwrap().position;
        write!(output, "S {case} {phase} {index}").unwrap();
        for bits in [
            p.real_angle,
            p.real_distance,
            p.angle,
            p.distance,
            p.max_angle,
            p.max_distance,
            p.destination_angle,
            p.destination_distance,
            p.angle(true, mirror),
        ] {
            write!(output, " {bits:08x}").unwrap();
        }
        output.push('\n');
    }
}
#[test]
fn every_shape_missing_live_dead_compact_and_mirror_matches_pinned_cpp() {
    // SpawnGroup.cpp:1541-1911, exact source numeric projection of 560 authored cases.
    let mut output = String::new();
    for line in
        include_str!("../../../../../benilla-sim/tools/fixtures/source_formations/inputs.txt")
            .lines()
    {
        let v: Vec<u32> = line
            .split_whitespace()
            .map(|x| x.parse().unwrap())
            .collect();
        let (case, shape, count, compact, pattern, mirror) =
            (v[0], v[1], v[2], v[3] != 0, v[4], v[5] != 0);
        let (mut f, mut h) = shapes(count, shape, compact, pattern);
        writeln!(output, "C {case}").unwrap();
        f.fix_positions(group(), &mut h).unwrap();
        emit(&f, &mut h, &mut output, case, 0, mirror);
        for id in f
            .state(group())
            .unwrap()
            .slots
            .values()
            .flatten()
            .copied()
            .collect::<Vec<_>>()
        {
            let unit = f.slot(id).unwrap().owner;
            if unit.is_some() && unit == f.master(group()).unwrap() {
                continue;
            }
            f.slot_mut(id)
                .unwrap()
                .position
                .add_variation(false, &mut h)
                .unwrap();
        }
        emit(&f, &mut h, &mut output, case, 1, mirror);
        for id in f
            .state(group())
            .unwrap()
            .slots
            .values()
            .flatten()
            .copied()
            .collect::<Vec<_>>()
        {
            let unit = f.slot(id).unwrap().owner;
            if unit.is_some() && unit == f.master(group()).unwrap() {
                continue;
            }
            f.slot_mut(id).unwrap().position.update();
        }
        emit(&f, &mut h, &mut output, case, 2, mirror);
    }
    let expected =
        include_str!("../../../../../benilla-sim/tools/fixtures/source_formations/expected.tsv");
    assert_eq!(output.lines().count(), expected.lines().count());
    for (index, (actual, expected)) in output.lines().zip(expected.lines()).enumerate() {
        assert_eq!(actual, expected, "oracle line {index}");
    }
}
#[test]
fn stopped_master_only_smooths_and_strict_deadlines_do_not_draw_at_equality() {
    // SpawnGroup.cpp:1506-1537 uses <, not <=, and smooths while stopped.
    let (mut f, mut h) = shapes(3, 0, false, 0);
    f.fix_positions(group(), &mut h).unwrap();
    h.random_calls.clear();
    h.base.now = 0;
    f.execute(group(), FormationCommand::Update, &mut h)
        .unwrap();
    assert!(h.random_calls.is_empty());
    h.base.now = 1000;
    f.execute(group(), FormationCommand::Update, &mut h)
        .unwrap();
    assert_eq!(f.state(group()).unwrap().next_variation_update_millis, 2000);
    assert_eq!(f.state(group()).unwrap().next_variation_millis, 0);
    assert!(h.random_calls.is_empty());
    h.base.units.get_mut(&object(1)).unwrap().moving = true;
    f.execute(group(), FormationCommand::Update, &mut h)
        .unwrap();
    assert!(h.random_calls[0].starts_with("U 20000 50000 "));
    assert_eq!(h.cursor.raw, 9); // Four initial draws, then deadline and four variation draws.
    let deadline = f.state(group()).unwrap().next_variation_millis;
    assert!((21000..=51000).contains(&deadline));
    h.random_calls.clear();
    h.base.now = deadline;
    f.execute(group(), FormationCommand::Update, &mut h)
        .unwrap();
    assert!(h.random_calls.is_empty());
    h.base.now += 1;
    f.execute(group(), FormationCommand::Update, &mut h)
        .unwrap();
    assert_eq!(h.random_calls.len(), 9);
}
#[test]
fn exact_deadline_endpoints_and_overflow_preserve_earlier_draw() {
    // SpawnGroup.cpp:1510 assigns deadline after urand, before slot variation.
    for delay in [20000, 50000] {
        let (mut f, mut h) = shapes(2, 0, false, 0);
        h.deadline = Some(delay);
        h.base.units.get_mut(&object(1)).unwrap().moving = true;
        h.base.now = i64::MAX - i64::from(delay);
        f.execute(group(), FormationCommand::Update, &mut h)
            .unwrap();
        assert_eq!(f.state(group()).unwrap().next_variation_millis, i64::MAX);
        let (mut f, mut h) = shapes(2, 0, false, 0);
        h.deadline = Some(delay);
        h.base.units.get_mut(&object(1)).unwrap().moving = true;
        h.base.now = i64::MAX - i64::from(delay) + 1;
        assert_eq!(
            f.execute(group(), FormationCommand::Update, &mut h)
                .unwrap_err()
                .id,
            "execution.formation.variation_deadline_overflow_undefined"
        );
        assert_eq!(h.random_calls, vec![format!("U 20000 50000 {delay}")]);
        assert_eq!(h.cursor.raw, 1);
    }
}
#[test]
fn compact_counts_dead_but_places_alive_and_queries_selected_width_twice() {
    // SpawnGroup.cpp:1553 is owner->IsAlive(), unlike :1584's !owner->IsAlive().
    let (mut f, mut h) = shapes(3, 6, true, 0);
    h.width_calls.clear();
    f.fix_positions(group(), &mut h).unwrap();
    assert!(h.width_calls.is_empty());
    assert_eq!(
        f.slot(f.state(group()).unwrap().slots[&1].unwrap())
            .unwrap()
            .position,
        Position::default()
    );
    h.base.units.get_mut(&object(1)).unwrap().alive = false;
    h.base.units.get_mut(&object(3)).unwrap().alive = false;
    f.fix_positions(group(), &mut h).unwrap();
    assert_eq!(
        h.width_calls,
        vec![object(1), object(1), object(3), object(3)]
    );
    assert_eq!(
        f.slot(f.state(group()).unwrap().slots[&1].unwrap())
            .unwrap()
            .position
            .distance,
        3.25_f32.to_bits()
    );
    assert_eq!(
        f.slot(f.state(group()).unwrap().slots[&2].unwrap())
            .unwrap()
            .position,
        Position::default()
    );
}
#[test]
fn initial_variation_precedes_the_missing_master_motion_owner() {
    // Promotion fixes positions before static master motion; SpawnGroup.cpp:965-970.
    let (mut f, base) = random_fixture(3);
    let mut h = ShapeHost {
        base,
        ..ShapeHost::default()
    };
    for id in 1..=3 {
        f.execute(
            group(),
            FormationCommand::Attach(object(id), FormationSlotType::Static),
            &mut h,
        )
        .unwrap();
    }
    h.random_calls.clear();
    h.base.reject_motion = true;
    assert_eq!(
        f.execute(
            group(),
            FormationCommand::TryMaster(Some(object(2))),
            &mut h
        )
        .unwrap_err()
        .id,
        "execution.formation.f5_motion_master_owner"
    );
    assert_eq!(
        h.random_calls
            .iter()
            .filter(|x| x.starts_with("R "))
            .count(),
        4
    );
    assert!(
        f.slot(f.state(group()).unwrap().slots[&1].unwrap())
            .unwrap()
            .position
            .distance
            != 0
    );
}

#[test]
fn follower_restart_precedes_variation_draw_and_smoothing_overflow_refusal() {
    // SpawnGroup.cpp:1494-1519 restarts followers before drawing a new variation.
    let (mut f, mut h) = shapes(3, 0, false, 0);
    f.execute(group(), FormationCommand::StopFollowers, &mut h)
        .unwrap();
    h.base.now = 6001;
    h.base.units.get_mut(&object(1)).unwrap().moving = true;
    h.refuse_random = true;
    assert_eq!(
        f.execute(group(), FormationCommand::Update, &mut h)
            .unwrap_err()
            .id,
        "execution.formation.f3_thread_rng_owner"
    );
    assert!(!f.state(group()).unwrap().followers_stopped);
    assert!(f
        .state(group())
        .unwrap()
        .slots
        .values()
        .flatten()
        .all(|id| f.slot(*id).unwrap().can_follow));
    assert!(h.random_calls[0].starts_with("U "));
    for (now, undefined) in [(i64::MAX - 1000, false), (i64::MAX - 999, true)] {
        let (mut f, mut h) = shapes(2, 0, false, 0);
        h.base.now = now;
        let result = f.execute(group(), FormationCommand::Update, &mut h);
        if undefined {
            assert_eq!(
                result.unwrap_err().id,
                "execution.formation.variation_update_deadline_overflow_undefined"
            );
        } else {
            result.unwrap();
            assert_eq!(
                f.state(group()).unwrap().next_variation_update_millis,
                i64::MAX
            );
        }
    }
}

#[test]
fn real_waypoint_keys_drive_mirror_callbacks_and_both_undefined_boundaries() {
    // WaypointMovementGenerator.cpp:617-646 uses map iterators, not key arithmetic.
    let (mut f, mut h) = fixture(2, false, 0);
    attach(&mut f, &mut h, 1);
    let follower = attach(&mut f, &mut h, 2);
    f.slot_mut(follower)
        .unwrap()
        .position
        .set(1_f32.to_bits(), 3_f32.to_bits());
    let path = BTreeMap::from([(2, ()), (7, ()), (99, ())]);
    let (mut current, mut returning) = (7, false);
    f.linear_waypoint_next(object(1), &path, &mut current, &mut returning)
        .unwrap();
    assert_eq!((current, returning), (99, false));
    f.linear_waypoint_next(object(1), &path, &mut current, &mut returning)
        .unwrap();
    assert_eq!((current, returning), (7, true));
    assert_eq!(
        f.slot_offset(follower, true).unwrap()[0],
        (std::f32::consts::TAU - 1.0).to_bits()
    );
    f.linear_waypoint_next(object(1), &path, &mut current, &mut returning)
        .unwrap();
    f.linear_waypoint_next(object(1), &path, &mut current, &mut returning)
        .unwrap();
    assert_eq!((current, returning), (7, false));
    assert_eq!(f.slot_offset(follower, true).unwrap()[0], 1_f32.to_bits());
    let path = BTreeMap::from([(7, ())]);
    assert_eq!(
        f.linear_waypoint_next(object(1), &path, &mut current, &mut returning)
            .unwrap_err()
            .id,
        "execution.formation.waypoint_previous_begin_undefined"
    );
    assert!(!returning);
    returning = true;
    f.execute(group(), FormationCommand::WaypointEnd, &mut h)
        .unwrap();
    assert_eq!(
        f.linear_waypoint_next(object(1), &path, &mut current, &mut returning)
            .unwrap_err()
            .id,
        "execution.formation.waypoint_next_end_undefined"
    );
    assert!(!returning);
    assert!(!f.state(group()).unwrap().mirror);
}

#[test]
fn checked_raw_stock_shape_offsets_and_first_missing_model_owner_are_counted_separately() {
    let Some(path) = std::env::var_os("BENILLA_F4_FORMATION_ROWS") else {
        println!("SKIP catalog-04 formation shape reach: set BENILLA_F4_FORMATION_ROWS");
        return;
    };
    let (held, raw) = metadata(Path::new(&path), 2_097_152).unwrap();
    assert_eq!(
        Sha256Id::of(&raw).as_str(),
        "c804df0569e963a661470eaa1cf26758761b1bb483822961e3f5dbf06ba09f67"
    );
    let rows: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    let number =
        |row: &serde_json::Value, key: &str| row[key].as_str().unwrap().parse::<u32>().unwrap();
    let mut numeric = [0_u32; 7];
    let mut refused = [0_u32; 7];
    for row in rows["formations"].as_array().unwrap() {
        let group_id = number(row, "id");
        let shape = number(row, "formationtype");
        let members: Vec<_> = rows["members"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| number(m, "id") == group_id)
            .enumerate()
            .map(|(ordinal, m)| GroupMember {
                group: group_id,
                guid: number(m, "guid"),
                slot: m["slotid"].as_str().unwrap().parse().unwrap(),
                chance: 0,
                ordinal: ordinal as u64,
                own_entry: 1,
                random_entry: false,
            })
            .collect();
        if members.is_empty() {
            continue;
        }
        let group = GroupIdentity {
            handle: SpawnGroupHandle {
                id: group_id,
                generation: 1,
            },
            ..group()
        };
        let entry = Formation {
            group: group_id,
            kind: shape,
            spread_bits: row["formationspread"]
                .as_str()
                .unwrap()
                .parse::<f32>()
                .unwrap()
                .to_bits(),
            options: number(row, "formationoptions"),
            path: number(row, "pathid"),
            movement: number(row, "movementtype"),
            comment: vec![],
        };
        let mut formations = SourceFormations::default();
        let mut host = ShapeHost::default();
        formations
            .create(group, entry.clone(), &members, false)
            .unwrap();
        // A valid explicit source shape call with no live owners needs no model stand-ins.
        formations
            .execute(
                group,
                FormationCommand::Spread(entry.spread_bits),
                &mut host,
            )
            .unwrap();
        numeric[shape as usize] += 1;
        assert!(formations
            .state(group)
            .unwrap()
            .slots
            .values()
            .flatten()
            .any(|id| formations.slot(*id).unwrap().position.distance != 0));
        let mut formations = SourceFormations::default();
        formations
            .create(group, entry.clone(), &members, false)
            .unwrap();
        let master = members.iter().find(|m| m.slot == 0).unwrap();
        host.base.units.insert(object(1), unit(master.guid, false));
        host.refuse_width = true;
        host.base.reject_motion = true;
        assert_eq!(
            formations
                .execute(
                    group,
                    FormationCommand::Attach(object(1), FormationSlotType::Static),
                    &mut host
                )
                .unwrap_err()
                .id,
            "execution.formation.f5_motion_master_owner"
        );
        let mut formations = SourceFormations::default();
        formations.create(group, entry, &members, false).unwrap();
        host.base.reject_motion = false;
        assert_eq!(
            formations
                .execute(
                    group,
                    FormationCommand::Attach(object(1), FormationSlotType::Static),
                    &mut host
                )
                .unwrap_err()
                .id,
            "execution.formation.f6_collision_width_owner"
        );
        refused[shape as usize] += 1;
    }
    assert_eq!(numeric, [17, 30, 65, 45, 39, 4, 27]);
    assert_eq!(refused, numeric);
    println!("Raw stock conditional empty-owner numeric groups={numeric:?}; default initial F5 master refusals={refused:?}; conditional post-master missing-width refusals={refused:?}; undefined reaches=0. Not effective ObjectMgr acceptance.");
    held.check().unwrap();
}

#[test]
fn follow_owner_refusal_keeps_initial_draws_slot_and_preceding_clear() {
    // SpawnGroup.cpp:1425-1429,798-800 fixes positions before Clear then MoveInFormation.
    let (mut f, base) = random_fixture(3);
    let mut h = ShapeHost {
        base,
        ..ShapeHost::default()
    };
    f.execute(
        group(),
        FormationCommand::Attach(object(1), FormationSlotType::Static),
        &mut h,
    )
    .unwrap();
    h.refuse_follow = true;
    h.random_calls.clear();
    h.base.calls.clear();
    assert_eq!(
        f.execute(
            group(),
            FormationCommand::Attach(object(2), FormationSlotType::Static),
            &mut h
        )
        .unwrap_err()
        .id,
        "execution.formation.f5_motion_follow_owner"
    );
    assert_eq!(
        h.random_calls
            .iter()
            .filter(|x| x.starts_with("R "))
            .count(),
        4
    );
    let slot = f.binding(object(2)).unwrap();
    assert_eq!(f.slot(slot).unwrap().owner, Some(object(2)));
    assert_eq!(h.base.targets.get(&object(2)), None);
    assert_eq!(
        h.base.calls,
        vec![
            Call::Motion(
                object(2),
                FormationMotion::Clear {
                    reset: false,
                    all: true
                }
            ),
            Call::Motion(
                object(2),
                FormationMotion::Follow {
                    slot,
                    master: object(1)
                }
            )
        ]
    );
}

mod motion;

#[test]
fn retained_shape_and_variation_projection_or_explicit_absence() {
    let Some(root) = std::env::var_os("BENILLA_F1_FORMATION_FRAMING") else {
        println!("SKIP unsealed MG/source-numeric formation comparison and variation/F3-word comparison: no bound framing");
        return;
    };
    let (held, raw) = metadata(&Path::new(&root).join("source-sites.json"), 2_097_152).unwrap();
    let sites: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    let direct = sites
        .as_object()
        .unwrap()
        .values()
        .filter(|site| {
            let function = site["callsite"]["function"].as_str().unwrap_or("");
            function.contains("FormationData::")
                || function.contains("FormationSlotData::")
                || function.contains("FormationMovementGenerator::")
        })
        .count();
    held.check().unwrap();
    println!("Unsealed formation numeric availability: dictionary_sha256={}, sites={direct}, independently_matched_numeric=0, attributed_variation_draws=0",Sha256Id::of(&raw).as_str());
    println!(
        "SKIP unsealed MG/source-numeric formation comparison: no independent numeric projection"
    );
    println!("SKIP unsealed formation variation/F3-word comparison: no caller-bound variation spans; helper words alone cannot identify formation draws");
}

#[test]
fn checked_stock_linear_paths_do_not_reach_single_node_iterator_undefined() {
    let Some(path) = std::env::var_os("BENILLA_F4_FORMATION_WAYPOINTS") else {
        println!("SKIP checked catalog-04 linear waypoint boundary census");
        return;
    };
    let (held, raw) = metadata(Path::new(&path), 131072).unwrap();
    let counts: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    assert_eq!(
        counts["catalog_sha256"],
        "757dcfb85e2b2a0de6571b0fe981c214fa57a4e2e546968ca61171113f379bd6"
    );
    assert_eq!(counts["counts"].as_object().unwrap().len(), 244);
    assert!(counts["counts"]
        .as_object()
        .unwrap()
        .values()
        .all(|count| count.as_u64().unwrap() >= 2));
    assert!(counts["point_zero"].as_array().unwrap().is_empty());
    held.check().unwrap();
    println!("Checked catalog-04 waypoint paths=244, single-node paths=0, point-zero paths=0; formation linear definitions=104, all have at least two nodes");
}

#[test]
fn initial_master_motion_refusal_precedes_shape_draws() {
    // Initial attachment moves the master first, unlike promotion; SpawnGroup.cpp:1421-1429.
    let (mut f, base) = random_fixture(3);
    let mut h = ShapeHost {
        base,
        ..ShapeHost::default()
    };
    h.base.reject_motion = true;
    assert_eq!(
        f.execute(
            group(),
            FormationCommand::Attach(object(1), FormationSlotType::Static),
            &mut h
        )
        .unwrap_err()
        .id,
        "execution.formation.f5_motion_master_owner"
    );
    assert!(h.random_calls.is_empty());
    assert_eq!(f.master(group()).unwrap(), Some(object(1)));
    assert!(f
        .state(group())
        .unwrap()
        .slots
        .values()
        .flatten()
        .all(|id| f.slot(*id).unwrap().position == Position::default()));
}
