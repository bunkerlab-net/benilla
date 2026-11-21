// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

#[derive(Default)]
struct Inputs {
    rows: Vec<BattleGroundRow>,
    safe: BTreeMap<u32, SourceWorldSafeLoc>,
    notices: Vec<BattleGroundLoadNotice>,
}

impl BattleGroundInputs for Inputs {
    fn templates(&mut self, sql: &str) -> Result<Vec<BattleGroundRow>> {
        assert_eq!(sql, BATTLEGROUND_TEMPLATE_SQL);
        Ok(self.rows.clone())
    }

    fn safe_locations(&self) -> Option<&BTreeMap<u32, SourceWorldSafeLoc>> {
        Some(&self.safe)
    }

    fn map_name(&mut self, map: u32) -> Result<Option<Box<[u8]>>> {
        Ok(match map {
            30 => Some(Box::from(&b"Alterac Valley"[..])),
            489 => Some(Box::from(&b"Warsong Gulch"[..])),
            529 => Some(Box::from(&b"Arathi Basin"[..])),
            _ => None,
        })
    }

    fn notice(&mut self, notice: BattleGroundLoadNotice) {
        self.notices.push(notice);
    }
}

fn row(type_id: u32, skin: u32) -> BattleGroundRow {
    BattleGroundRow {
        type_id,
        min_per_team: 5,
        max_per_team: 10,
        min_level: 10,
        max_level: 60,
        alliance_start: 11,
        horde_start: 12,
        max_start_distance: 75.0,
        player_skin_refloot: skin,
    }
}

fn inputs(rows: Vec<BattleGroundRow>) -> Inputs {
    Inputs {
        rows,
        safe: BTreeMap::from([
            (
                11,
                SourceWorldSafeLoc {
                    map_id: 30,
                    position: [1.0, 2.0, 3.0, 4.0],
                },
            ),
            (
                12,
                SourceWorldSafeLoc {
                    map_id: 30,
                    position: [5.0, 6.0, 7.0, 8.0],
                },
            ),
        ]),
        notices: Vec::new(),
    }
}

fn load_loot() -> SourceReferenceLootStore {
    struct Reference;
    impl ReferenceLootInputs for Reference {
        fn rows(&mut self, _sql: &str) -> Result<Vec<ReferenceLootRow>> {
            Ok(vec![ReferenceLootRow {
                entry: 1,
                item: 27,
                chance: 100.0,
                group: 0,
                min_count_or_ref: 1,
                max_count: 1,
                condition: 0,
            }])
        }

        fn item_exists(&mut self, id: u32) -> Result<bool> {
            Ok(id == 27)
        }
    }
    let mut store = SourceReferenceLootStore::default();
    store.load(&mut Reference).unwrap();
    store
}

#[test]
fn creates_three_templates_and_validates_skin_reference_ids() {
    let mut mgr = SourceBattleGroundMgr::default();
    let mut input = inputs(vec![row(1, 1), row(2, 99), row(3, 0)]);
    let loot = load_loot();
    assert_eq!(
        mgr.create_initial_battlegrounds(Some(&loot), &mut input)
            .unwrap(),
        3
    );
    assert_eq!(
        input.notices,
        [BattleGroundLoadNotice::MissingReferenceLoot { type_id: 2, id: 99 }]
    );
    assert_eq!(mgr.used_refloot(), &BTreeSet::from([1]));
    for (id, map, messages, skin) in [
        (1, 30, [0, 784, 785, 786], 1),
        (2, 489, [0, 602, 603, 604], 0),
        (3, 529, [0, 661, 662, 663], 0),
    ] {
        let template = mgr.template(id).unwrap();
        assert_eq!(template.map_id, map);
        assert_eq!(template.instance_id(), 0);
        assert!(template.map.is_none());
        assert_eq!(template.bracket_id, -1);
        assert_eq!((template.status, template.winner), (0, 2));
        assert_eq!(template.start_message_ids, messages);
        assert_eq!(template.start_delay_times_ms, [120_000, 60_000, 30_000, 0]);
        assert_eq!(
            template.start_locations,
            [[1.0, 2.0, 3.0, 4.0], [5.0, 6.0, 7.0, 8.0]]
        );
        assert_eq!((template.min_players, template.max_players), (10, 20));
        assert_eq!(template.player_skin_refloot, skin);
    }
    assert!(matches!(
        mgr.template(1).unwrap().mode,
        BattleGroundMode::AlteracValley {
            mine_yell_ms: 25_000,
            ..
        }
    ));
    assert!(matches!(
        mgr.template(2).unwrap().mode,
        BattleGroundMode::WarsongGulch { .. }
    ));
    assert!(matches!(
        mgr.template(3).unwrap().mode,
        BattleGroundMode::ArathiBasin { .. }
    ));
}

#[test]
fn stops_at_skin_lookup_only_when_a_nonzero_id_needs_an_unbound_store() {
    let mut mgr = SourceBattleGroundMgr::default();
    assert_eq!(
        mgr.create_initial_battlegrounds(None, &mut inputs(vec![row(1, 1)]))
            .unwrap_err()
            .id,
        "execution.battlegrounds.BattleGroundMgr_512.reference_loot_owner"
    );
    assert!(mgr.template(1).is_none());
    assert_eq!(
        mgr.create_initial_battlegrounds(None, &mut inputs(vec![row(1, 0)]))
            .unwrap(),
        1
    );
    assert_eq!(mgr.template(1).unwrap().player_skin_refloot, 0);
}

#[test]
fn skips_missing_locations_and_map_in_source_order() {
    let mut mgr = SourceBattleGroundMgr::default();
    let mut input = inputs(vec![
        BattleGroundRow {
            alliance_start: 40,
            horde_start: 41,
            ..row(1, 1)
        },
        BattleGroundRow {
            horde_start: 41,
            ..row(2, 1)
        },
        row(0, 1),
    ]);
    assert_eq!(
        mgr.create_initial_battlegrounds(None, &mut input).unwrap(),
        0
    );
    assert_eq!(
        input.notices,
        [
            BattleGroundLoadNotice::MissingAllianceStart { type_id: 1, id: 40 },
            BattleGroundLoadNotice::MissingHordeStart { type_id: 2, id: 41 },
            BattleGroundLoadNotice::MissingMap { type_id: 0, id: 0 },
        ]
    );
}

#[test]
fn empty_rows_report_empty_and_out_of_range_type_refuses() {
    let mut mgr = SourceBattleGroundMgr::default();
    let mut input = Inputs::default();
    assert_eq!(
        mgr.create_initial_battlegrounds(None, &mut input).unwrap(),
        0
    );
    assert_eq!(input.notices, [BattleGroundLoadNotice::EmptyTemplates]);
    assert_eq!(
        mgr.create_initial_battlegrounds(None, &mut inputs(vec![row(4, 0)]))
            .unwrap_err()
            .id,
        "execution.battlegrounds.BattleGroundMgr_457.type_range"
    );
}
