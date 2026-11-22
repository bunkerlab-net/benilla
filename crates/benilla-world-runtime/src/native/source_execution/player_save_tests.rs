// SPDX-License-Identifier: GPL-3.0-or-later
use super::source_rows::*;
use super::*;
use serde_json::Value;
use std::fmt::Write as _;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::Path;

#[derive(Default)]
struct Work(Vec<i64>);
impl WorldStartupWork for Work {
    fn clock(&mut self, site: WorldStartupClockSite) -> Result<ClockSample> {
        panic!("unexpected world clock {site:?}");
    }

    fn observe(&mut self, observation: WorldStartupObservation) -> Result<()> {
        let WorldStartupObservation::LocalTime { seconds, .. } = observation else {
            panic!("unexpected world observation {observation:?}");
        };
        self.0.push(seconds);
        Ok(())
    }
}

pub(super) fn assert_recorded_create_save(
    player: &mut SourcePlayer,
    objects: &mut SourceObjects,
    units: &mut SourceUnits,
    items: &mut SourceItems,
    spells: &SourceSpellStores,
    session: &mut SourceWorldSession,
    trace: &Path,
) -> [u32; 8] {
    // Northshire-01 A mangosd.conf:825 and proxy-requests.jsonl:1498.
    const SKIP_CINEMATICS: u32 = 0;
    let world = SourceWorld::new(12, 200).unwrap();
    let (game_seconds, save_seconds) = save_clocks(trace);
    let c = trace.parent().unwrap().parent().unwrap();
    let a = trace
        .ancestors()
        .nth(5)
        .unwrap()
        .join("northshire-03/source-choice/a");
    let (columns, create) = recording(c, 1511, 1514);
    let (a_columns, logout) = recording(&a, 1517, 1833);
    let (_, other_create) = recording(&a, 1517, 1520);
    assert_eq!(a_columns, columns);
    let mut work = Work::default();
    let mut read_wall = [save_seconds].into_iter();
    let mut inputs = SourcePlayerSaveInputs {
        objects,
        units,
        items,
        session,
        quests: &SourceQuestMap::default(),
        spells,
        clock: &SourceWorldClock::default(),
        world: &world,
        calendar: Calendar::new(CalendarOwner::World, game_seconds),
        maintenance_weekday: 3, // Northshire-01 A mangosd.conf:832.
        save_interval: 900_000,
        world_work: &mut work,
        wall_seconds: &mut || Ok(read_wall.next().unwrap()),
    };
    player.set_creation_flags(false, SKIP_CINEMATICS);
    let staged = player.stage_save(&mut inputs).unwrap();
    assert_eq!(read_wall.next(), None);
    assert_eq!(work.0, [game_seconds; 3]);
    assert_eq!(staged.rows.len(), 37); // C:p1502-1607, not counting BEGIN and COMMIT.
    let [RowOperation::Delete(RowDelete::Character(guid)), RowOperation::Insert(RowInsert::Character(row)), inventory @ ..] =
        staged.rows.as_slice()
    else {
        panic!("unexpected save batch {:?}", staged.rows);
    };
    assert_eq!((staged.guid, *guid), (row.guid, row.guid));
    let projected = bind_words(row);
    assert_eq!(projected.len(), 56);
    for ((name, projected), observed) in columns.iter().zip(&projected).zip(&create) {
        assert_eq!(projected, observed, "C:p1514 {name}");
    }

    assert_eq!(create[17].bytes.as_slice(), b"2 0 0 0 0 0 0 0 ");
    assert!(create[37].bytes.is_empty());
    assert_eq!(create[51].bytes.len(), 128);
    assert_eq!(create[52].bytes.len(), 89);

    // The logout-only values need progress and an in-world zone owner.
    let changed = [
        7, 8, 9, 14, 15, 16, 17, 19, 20, 21, 22, 23, 24, 35, 36, 46, 47, 52,
    ];
    for (index, ((name, projected), observed)) in
        columns.iter().zip(&projected).zip(&logout).enumerate()
    {
        assert_eq!(projected.tag, observed.tag, "A:p1833 {name} type");
        assert_eq!(
            projected == observed,
            !changed.contains(&(index + 1)),
            "A:p1833 {name}"
        );
    }
    assert_eq!(other_create[23].bytes, 1795250266u64.to_le_bytes());
    for (index, (create, other)) in create.iter().zip(&other_create).enumerate() {
        assert_eq!(create == other, index != 23, "A:p1520 {}", columns[index]);
    }
    assert!(!row.online);
    assert_eq!(logout[18].bytes, 1u32.to_le_bytes()); // Live logout saves before detach.
    assert_eq!(logout[23].bytes, 1795250531u64.to_le_bytes());
    let (inventory, learning) = inventory.split_at(16); // C:p1519-1552.
    super::player_items::save_inventory::tests::assert_create_recording(c, inventory);
    let tutorial =
        super::player_save_learning::tests::assert_create_learning_recording(c, *guid, learning);
    assert_eq!(
        tutorial,
        TutorialRow {
            account: u64::from(row.account),
            words: [0; 8],
        }
    ); // C:p1605.
    tutorial.words
}

// Northshire-01 A trace :68134692-68134694 and :68134746-68134747.
fn save_clocks(path: &Path) -> (i64, i64) {
    let mut reader = BufReader::new(File::open(path).unwrap());
    // Player constructor seed starts on :68133280.
    reader.seek(SeekFrom::Start(4_371_307_944)).unwrap();
    let lines = reader.lines().zip(68_133_280..);
    let mut time = None;
    for (line, number) in lines {
        let line = line.unwrap();
        if (68_134_692..=68_134_694).contains(&number) {
            let event = 15_009 + (number - 68_134_692);
            let prefix = format!("12\t{event}\tclock_localtime\t");
            let value = line
                .strip_prefix(&prefix)
                .unwrap()
                .split_ascii_whitespace()
                .next()
                .unwrap()
                .parse()
                .unwrap();
            assert_eq!(*time.get_or_insert(value), value);
        } else if number == 68_134_746 {
            assert!(line.starts_with("12\t15037\tclock_callsite\t1 3678789157770927477 5781581 "));
        } else if number == 68_134_747 {
            let wall = line
                .strip_prefix("12\t15038\tclock_time\t")
                .unwrap()
                .split_ascii_whitespace()
                .next()
                .unwrap()
                .parse()
                .unwrap();
            return (time.unwrap(), wall);
        }
    }
    panic!("missing save clocks");
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Operand {
    pub(super) tag: [u8; 2],
    pub(super) bytes: Vec<u8>,
}

fn recording(root: &Path, prepare: usize, execute: usize) -> (Vec<String>, Vec<Operand>) {
    let mut rows = BufReader::new(File::open(root.join("proxy-requests.jsonl")).unwrap()).lines();
    let row =
        |line: std::io::Result<String>| serde_json::from_str::<Value>(&line.unwrap()).unwrap();
    let prepare_row = row(rows.nth(prepare - 1).unwrap());
    let bytes = payload(&prepare_row);
    assert_eq!(bytes[0], 22);
    let sql = std::str::from_utf8(&bytes[1..]).unwrap();
    let columns = sql
        .strip_prefix("INSERT INTO characters (")
        .unwrap()
        .split_once(')')
        .unwrap()
        .0
        .split(',')
        .map(|name| name.trim().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(columns.len(), 56);
    let identity = row(rows.next().unwrap());
    assert_eq!(identity["kind"], "prepared_identity");
    assert_eq!(identity["request"], prepare_row["request"]);
    assert_eq!(identity["connection"], 4);
    let execute_row = row(rows.nth(execute - prepare - 2).unwrap());
    let bytes = payload(&execute_row);
    let id = identity["native_statement"]
        .as_u64()
        .unwrap()
        .try_into()
        .unwrap();
    (columns, prepared_operands(&bytes, id, 56))
}

pub(super) fn prepared_operands(bytes: &[u8], id: u32, count: usize) -> Vec<Operand> {
    assert_eq!(bytes[0], 23);
    assert_eq!(u32::from_le_bytes(bytes[1..5].try_into().unwrap()), id);
    let null_end = 10 + count.div_ceil(8);
    assert!(bytes[10..null_end].iter().all(|&byte| byte == 0));
    assert_eq!(bytes[null_end], 1);
    let tag_end = null_end + 1 + 2 * count;
    let mut data = &bytes[tag_end..];
    let (tags, remainder) = bytes[null_end + 1..tag_end].as_chunks::<2>();
    assert!(remainder.is_empty());
    let words = tags
        .iter()
        .map(|tag| {
            let len = match tag[0] {
                1 => 1,
                2 => 2,
                3 | 4 => 4,
                8 => 8,
                254 => {
                    let start = data[0];
                    data = &data[1..];
                    if start == 252 {
                        let len = u16::from_le_bytes(data[..2].try_into().unwrap());
                        data = &data[2..];
                        usize::from(len)
                    } else {
                        assert!(start < 251);
                        usize::from(start)
                    }
                }
                t => panic!("unexpected bind type {t}"),
            };
            let (word, remaining) = data.split_at(len);
            data = remaining;
            Operand {
                tag: *tag,
                bytes: word.to_vec(),
            }
        })
        .collect();
    assert!(data.is_empty());
    words
}

pub(super) fn payload(row: &Value) -> Vec<u8> {
    assert_eq!(row["kind"], "request");
    assert_eq!(row["connection"], 4);
    let hex = row["private_payload_hex"].as_str().unwrap();
    let (pairs, remainder) = hex.as_bytes().as_chunks::<2>();
    assert!(remainder.is_empty());
    pairs
        .iter()
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

fn bind_words(row: &CharacterRow) -> Vec<Operand> {
    let byte = |value: u8| unsigned(1, value.to_le_bytes());
    let short = |value: u16| unsigned(2, value.to_le_bytes());
    let word = |value: u32| unsigned(3, value.to_le_bytes());
    let long = |value: u64| unsigned(8, value.to_le_bytes());
    let float = |word: FloatWord| Operand {
        tag: [4, 0],
        bytes: word.as_float().to_bits().to_le_bytes().to_vec(),
    };
    let text = |bytes| Operand {
        tag: [254, 0],
        bytes,
    };
    let [x, y, z] = row.xyz;
    let [tx, ty, tz, to] = row.transport_pose;
    let [p1, p2, p3, p4, p5] = row.powers;
    vec![
        word(row.guid),
        word(row.account),
        text(row.name.as_bytes().to_vec()),
        byte(row.race),
        byte(row.class),
        byte(row.gender),
        word(u32::from(row.level)),
        word(row.xp),
        word(row.money),
        word(row.player_bytes),
        word(row.player_bytes2),
        word(row.player_flags),
        word(row.map),
        float(x),
        float(y),
        float(z),
        float(row.orientation),
        text(words(row.taxi_mask)),
        word(u32::from(row.online)),
        word(u32::from(row.cinematic)),
        word(row.total_time),
        word(row.level_time),
        float(row.rest_bonus),
        long(row.logout_time),
        word(u32::from(row.is_logout_resting)),
        word(row.reset_talents_cost),
        long(row.reset_talents_time),
        float(tx),
        float(ty),
        float(tz),
        float(to),
        word(u32::try_from(row.transport_guid).unwrap()),
        word(row.extra_flags),
        word(u32::from(row.stable_slots)),
        word(row.at_login),
        word(row.zone),
        long(row.death_expire_time),
        text(
            row.taxi_path
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join("#")
                .into_bytes(),
        ),
        word(row.honor_highest_rank),
        Operand {
            tag: [3, 0],
            bytes: row.honor_standing.to_le_bytes().to_vec(),
        },
        float(row.stored_honor_rating),
        word(row.stored_dishonorable_kills as u32),
        word(row.stored_honorable_kills as u32),
        word(row.watched_faction),
        short(row.drunk),
        word(row.health),
        word(p1),
        word(p2),
        word(p3),
        word(p4),
        word(p5),
        text(words(row.explored_zones)),
        text(words(
            row.equipment_cache.into_iter().flat_map(|(a, b)| [a, b]),
        )),
        word(row.ammo_id),
        word(u32::from(row.action_bars)),
        byte(row.fishing_steps),
    ]
}

pub(super) fn unsigned<const N: usize>(kind: u8, bytes: [u8; N]) -> Operand {
    Operand {
        tag: [kind, 128],
        bytes: bytes.to_vec(),
    }
}

fn words(values: impl IntoIterator<Item = u32>) -> Vec<u8> {
    let mut text = String::new();
    for word in values {
        write!(text, "{word} ").unwrap();
    }
    text.into_bytes()
}

#[test]
fn create_applies_cinematics_and_first_login_before_save() {
    let mut objects = SourceObjects::default();
    let mut units = SourceUnits::default();
    let world = SourceWorld::new(12, 200).unwrap();
    let mut session = SourceWorldSession::construct(97, false);
    let mut items = SourceItems::default();
    for (same_race, config, expected) in [
        (true, 0, 0),
        (false, 1, 0),
        (true, 1, 1),
        (false, 2, 1),
        (true, 3, 0),
    ] {
        let mut player = super::tests::construct(&mut objects, &mut units);
        player.save_fields.teleport_far = true;
        player.save_fields.at_login = 1;
        let mut inputs = SourcePlayerSaveInputs {
            objects: &mut objects,
            units: &mut units,
            items: &mut items,
            session: &mut session,
            quests: &SourceQuestMap::default(),
            spells: &SourceSpellStores::default(),
            clock: &SourceWorldClock::default(),
            world: &world,
            calendar: Calendar::new(CalendarOwner::World, 10),
            maintenance_weekday: 3,
            save_interval: 300_000,
            world_work: &mut Work::default(),
            wall_seconds: &mut || panic!("far teleport returns before wall time"),
        };
        player.set_creation_flags(same_race, config);
        let stop = player
            .project_save(&mut inputs, |_| {
                panic!("far teleport returns before transaction")
            })
            .unwrap_err();
        assert_eq!(
            stop.id,
            "execution.player_save.Player_15607.delayed_save_owner"
        );
        assert_eq!(player.next_save_ms, 300_000);
        assert_eq!(player.save_fields.cinematic, expected);
        assert_eq!(player.save_fields.at_login, 0x21);
    }
}

#[test]
fn save_refuses_in_source_order_without_a_commit() {
    use super::objects::SourceObject;

    let mut objects = SourceObjects::default();
    let mut units = SourceUnits::default();
    let mut player = super::tests::construct(&mut objects, &mut units);
    let world = SourceWorld::new(12, 200).unwrap();
    let mut session = SourceWorldSession::construct(97, false);
    let mut items = SourceItems::default();
    let handle = player.object();
    objects
        .assign_guid(handle, 5, SourceGuid::from_raw(5))
        .unwrap();
    let object: &mut SourceObject = objects.get_mut(handle).unwrap();
    object.pose = Some(SourcePose([0; 4]));
    object.map = Some(MapIdentity {
        map: 0,
        instance: 0,
        creation_generation: 1,
    });
    object.zone_update_id = 26;
    let unit = units.get_mut(&objects, handle).unwrap();
    unit.unit_class = Some(8);
    unit.power_fields = Some(SourceUnitPowerFields {
        level: 1,
        create_health: 51,
        create_mana: 165,
        auxiliary_current: Some([7, 8, 9]),
        auxiliary_maximum: Some([7, 8, 9]),
    });
    unit.vitals = Some(SourceUnitVitals {
        health: 51,
        max_health: 51,
        mana: 165,
        max_mana: 165,
        has_mana: true,
        energy: 100,
        max_energy: 100,
    });
    for (near, in_world, taxi_state, mail, bg, reason, has_row, reads_wall) in [
        (
            true,
            false,
            0,
            false,
            false,
            "Player_15666.teleport_pose_owner",
            false,
            false,
        ),
        (
            false,
            true,
            0,
            false,
            false,
            "Player_15709.in_world_zone_owner",
            false,
            true,
        ),
        (
            false,
            false,
            3,
            false,
            false,
            "Taxi_46.taxi_route_owner",
            false,
            true,
        ),
        (
            false,
            false,
            0,
            true,
            true,
            "Player_15763.mail_owner",
            true,
            true,
        ),
        (
            false,
            false,
            0,
            false,
            true,
            "Player_19840.battleground_save_owner",
            true,
            true,
        ),
        (false, false, 0, false, false, "", true, true),
    ] {
        objects.get_mut(handle).unwrap().in_world = in_world;
        player.save_fields.teleport_near = near;
        player.save_fields.taxi_state = taxi_state;
        player.save_fields.mail_dirty = mail;
        player.save_fields.bg_dirty = bg;
        let mut wall_count = 0;
        let mut work = Work::default();
        let mut inputs = SourcePlayerSaveInputs {
            objects: &mut objects,
            units: &mut units,
            items: &mut items,
            session: &mut session,
            quests: &SourceQuestMap::default(),
            spells: &SourceSpellStores::default(),
            clock: &SourceWorldClock::default(),
            world: &world,
            calendar: Calendar::new(CalendarOwner::World, 10),
            maintenance_weekday: 3,
            save_interval: 300_000,
            world_work: &mut work,
            wall_seconds: &mut || {
                wall_count += 1;
                Ok(101)
            },
        };
        let mut effects = Vec::new();
        let result = player.project_save(&mut inputs, |effect| {
            effects.push(effect);
            Ok(())
        });
        if reason.is_empty() {
            assert_eq!(result.unwrap(), 5);
            assert_eq!(
                effects[3..],
                [
                    SourcePlayerSaveObservation::Row(RowOperation::Delete(
                        RowDelete::CooldownsByGuid(5)
                    )),
                    SourcePlayerSaveObservation::Row(RowOperation::Delete(RowDelete::AurasByGuid(
                        5
                    ))),
                    SourcePlayerSaveObservation::Row(RowOperation::Delete(
                        RowDelete::EnteredInstancesByAccount(97)
                    )),
                ]
            );
        } else {
            assert_eq!(
                result.unwrap_err().id,
                format!("execution.player_save.{reason}")
            );
        }
        assert_eq!(wall_count, i32::from(reads_wall));
        assert_eq!(work.0, [10; 3]);
        assert_eq!(effects[0], SourcePlayerSaveObservation::Begin);
        assert_eq!(
            effects[1],
            SourcePlayerSaveObservation::Row(RowOperation::Delete(RowDelete::Character(5)))
        );
        assert_eq!(
            effects.len(),
            if reason.is_empty() {
                6
            } else if has_row {
                3
            } else {
                2
            }
        );
        if has_row {
            let SourcePlayerSaveObservation::Row(RowOperation::Insert(RowInsert::Character(row))) =
                &effects[2]
            else {
                panic!("expected character insert");
            };
            assert_eq!(
                (row.account, row.zone, row.online, row.logout_time),
                (97, 26, false, 101)
            );
            assert_eq!(row.powers, [165, 7, 8, 100, 9]);
        }
    }
}
