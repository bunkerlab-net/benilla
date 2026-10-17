// SPDX-License-Identifier: GPL-3.0-or-later
// Authored synthetic cases replay the exact pinned-source distribution outcomes.
// These prove algorithm ordering/results, not MT or std::shuffle word parity.
use benilla_sim::loot::*;
use std::collections::{BTreeMap, VecDeque};

fn nums(text: &str, separator: char) -> Vec<u64> {
    if text.is_empty() {
        Vec::new()
    } else {
        text.split(separator).map(|v| v.parse().unwrap()).collect()
    }
}
struct Replay {
    draws: VecDeque<String>,
}
impl Random for Replay {
    fn inclusive(&mut self, low: u32, high: u32) -> Result<u32, Error> {
        let draw = self.draws.pop_front().expect("missing integer draw");
        let n = nums(draw.strip_prefix("u:").expect("integer draw order"), ':');
        assert_eq!((low, high), (n[0] as u32, n[1] as u32));
        Ok(n[2] as u32)
    }
    fn chance(&mut self) -> Result<f64, Error> {
        let draw = self.draws.pop_front().expect("missing chance draw");
        Ok(f64::from_bits(
            draw.strip_prefix("c:")
                .expect("chance draw order")
                .parse()
                .unwrap(),
        ))
    }
    fn shuffle(&mut self, values: &mut [usize]) -> Result<(), Error> {
        let draw = self.draws.pop_front().expect("missing permutation draw");
        let (before, after) = draw
            .strip_prefix("s:")
            .expect("permutation draw order")
            .split_once(':')
            .unwrap();
        assert_eq!(
            values.iter().map(|&v| v as u64).collect::<Vec<_>>(),
            nums(before, ',')
        );
        let shuffled = nums(after, ',');
        assert_eq!(values.len(), shuffled.len());
        for (value, result) in values.iter_mut().zip(shuffled) {
            *value = result as usize;
        }
        Ok(())
    }
}
fn capture(rows: Vec<Row>, case: u32) -> Capture {
    Capture {
        upstream_commit: UPSTREAM_COMMIT.into(),
        source_fingerprint: [1; 32],
        loot_id: 1,
        complete_selected_projection: true,
        loader_order: LoaderOrder {
            origin: OrderOrigin::WholeTableLoaderResult,
            complete_result_observed: true,
            ordered_result_fingerprint: [2; 32],
            result_row_count: rows.len() as u64,
            selected_row_positions: (0..rows.len() as u64).collect(),
        },
        items: rows
            .iter()
            .filter(|row| row.min_count_or_reference > 0)
            .map(|row| {
                (
                    row.item,
                    ItemLookup::Present(Item {
                        entry: row.item,
                        quality: (row.item - 70001) % 7,
                        display_id: 90000 + row.item,
                        flags: if case.is_multiple_of(2) { 0x800 } else { 0 },
                        start_quest: 0,
                        extra_flags: 0,
                        random_property: 0,
                    }),
                )
            })
            .collect::<BTreeMap<_, _>>(),
        rows,
    }
}
#[test]
fn exact_pinned_loot_process_group_roll_money_and_empty_window() {
    let mut cases = 0;
    let mut cap_cases = 0;
    let mut fake_cases = 0;
    let mut empty_draws = 0;
    for line in include_str!("fixtures/loot_cmangos.tsv").lines() {
        let columns = line.split('\t').collect::<Vec<_>>();
        assert_eq!(columns.len(), 8);
        assert!(columns.iter().all(|field| !field.is_empty()));
        let case: u32 = columns[0].parse().unwrap();
        let loot_id: u32 = columns[1].parse().unwrap();
        let money = nums(columns[2], ':');
        let rules = columns[3].split(':').collect::<Vec<_>>();
        let rates = nums(rules[0], ',');
        let rules = Rules {
            quality_rate_bits: std::array::from_fn(|i| rates[i] as u32),
            referenced_rate_bits: 1f32.to_bits(),
            money_rate_bits: rules[1].parse().unwrap(),
            corpse_empty_loot_show: rules[2] == "1",
        };
        let rows = if columns[4] == "-" {
            vec![]
        } else {
            columns[4]
                .split(';')
                .map(|row| {
                    let n = nums(row, ':');
                    Row {
                        item: n[0] as u32,
                        chance_or_quest_chance_bits: n[1] as u32,
                        group: n[2] as u8,
                        min_count_or_reference: n[3] as i32,
                        max_count: n[4] as u32,
                        condition_id: 0,
                    }
                })
                .collect()
        };
        let catalog = Catalog::new(if loot_id == 0 {
            vec![]
        } else {
            vec![capture(rows, case)]
        })
        .unwrap();
        let mut random = Replay {
            draws: columns[5]
                .split(';')
                .filter(|s| *s != "-")
                .map(str::to_owned)
                .collect(),
        };
        if random.draws.is_empty() {
            empty_draws += 1;
        }
        let generated = catalog
            .generate(
                loot_id,
                money[0] as u32,
                money[1] as u32,
                &rules,
                &mut random,
            )
            .unwrap();
        assert!(random.draws.is_empty(), "unused draws in case {case}");
        let summary = nums(columns[6], ':');
        let items = if columns[7] == "-" {
            vec![]
        } else {
            columns[7]
                .split(';')
                .map(|item| {
                    let n = nums(item, ':');
                    LootItem {
                        slot: n[0] as u8,
                        item: n[1] as u32,
                        count: n[2] as u8,
                        display_id: n[3] as u32,
                        random_property: n[4] as i32,
                    }
                })
                .collect()
        };
        assert_eq!(
            generated,
            Generated {
                items,
                money: summary[0] as u32,
                fake: summary[1] != 0,
            },
            "case {case}"
        );
        cap_cases += usize::from(generated.items.len() == MAX_ITEMS);
        fake_cases += usize::from(generated.fake);
        cases += 1;
    }
    assert_eq!(cases, 1536);
    assert!(cap_cases > 30, "item-cap cases: {cap_cases}");
    assert!(fake_cases > 0);
    assert!(empty_draws > 0);
}

#[test]
fn reject_unproven_loader_order_and_unsupported_loot_dependencies() {
    let row = Row {
        item: 70001,
        chance_or_quest_chance_bits: 100f32.to_bits(),
        group: 0,
        min_count_or_reference: 1,
        max_count: 1,
        condition_id: 0,
    };
    let c = capture(vec![row], 0);
    assert!(Catalog::new(vec![c.clone()]).is_ok());
    let mut bad = c.clone();
    bad.loader_order.complete_result_observed = false;
    assert!(Catalog::new(vec![bad]).is_err());
    let mut bad = c.clone();
    bad.loader_order.selected_row_positions[0] = 1;
    assert!(Catalog::new(vec![bad]).is_err());
    let mut bad = c.clone();
    bad.rows[0].condition_id = 1;
    assert!(Catalog::new(vec![bad]).is_err());
    let mut bad = c.clone();
    bad.rows[0].min_count_or_reference = -1;
    assert!(Catalog::new(vec![bad]).is_err());
    let mut bad = c.clone();
    bad.rows[0].chance_or_quest_chance_bits = (-50f32).to_bits();
    assert!(Catalog::new(vec![bad]).is_err());
    let mut bad = c;
    if let ItemLookup::Present(item) = bad.items.get_mut(&70001).unwrap() {
        item.random_property = 1;
    }
    assert!(Catalog::new(vec![bad]).is_err());
}

struct InvalidDraw {
    chance: f64,
    invalid_integer: bool,
    invalid_permutation: bool,
}
impl Random for InvalidDraw {
    fn inclusive(&mut self, low: u32, high: u32) -> Result<u32, Error> {
        Ok(if self.invalid_integer { high + 1 } else { low })
    }
    fn chance(&mut self) -> Result<f64, Error> {
        Ok(self.chance)
    }
    fn shuffle(&mut self, values: &mut [usize]) -> Result<(), Error> {
        if self.invalid_permutation {
            values.fill(usize::MAX);
        }
        Ok(())
    }
}
#[test]
fn malformed_distribution_outcomes_are_rejected_before_use() {
    let rules = Rules {
        quality_rate_bits: [1f32.to_bits(); 7],
        referenced_rate_bits: 1f32.to_bits(),
        money_rate_bits: 1f32.to_bits(),
        corpse_empty_loot_show: true,
    };
    let row = Row {
        item: 70001,
        chance_or_quest_chance_bits: 50f32.to_bits(),
        group: 0,
        min_count_or_reference: 1,
        max_count: 5,
        condition_id: 0,
    };
    let make = |row: Row| Catalog::new(vec![capture(vec![row], 0)]).unwrap();
    let catalog = make(row.clone());
    for chance in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.01, 100.] {
        let mut random = InvalidDraw {
            chance,
            invalid_integer: false,
            invalid_permutation: false,
        };
        assert_eq!(
            catalog.generate(1, 0, 0, &rules, &mut random),
            Err(Error::Draw)
        );
    }
    // Exercise both count and fake-empty-window integer outcomes.
    for chance in [0., 99.] {
        let mut random = InvalidDraw {
            chance,
            invalid_integer: true,
            invalid_permutation: false,
        };
        assert_eq!(
            catalog.generate(1, 0, 0, &rules, &mut random),
            Err(Error::Draw)
        );
    }
    let mut random = InvalidDraw {
        chance: 0.,
        invalid_integer: true,
        invalid_permutation: false,
    };
    assert_eq!(
        Catalog::new(vec![])
            .unwrap()
            .generate(0, 1, 2, &rules, &mut random),
        Err(Error::Draw)
    );
    // Both explicit and equal groups must reject a forged index before indexing rows.
    for chance in [0f32, 50.] {
        let catalog = make(Row {
            group: 1,
            chance_or_quest_chance_bits: chance.to_bits(),
            ..row.clone()
        });
        let mut random = InvalidDraw {
            chance: 0.,
            invalid_integer: false,
            invalid_permutation: true,
        };
        assert_eq!(
            catalog.generate(1, 0, 0, &rules, &mut random),
            Err(Error::Draw)
        );
    }
}

fn ordinary(item: u32, chance: f32, group: u8) -> Row {
    Row {
        item,
        chance_or_quest_chance_bits: chance.to_bits(),
        group,
        min_count_or_reference: 1,
        max_count: 1,
        condition_id: 0,
    }
}
fn reference(id: i32, chance: f32, repetitions: u32) -> Row {
    Row {
        item: 0,
        min_count_or_reference: -id,
        max_count: repetitions,
        ..ordinary(0, chance, 0)
    }
}
fn unit_rules() -> Rules {
    Rules {
        quality_rate_bits: [1f32.to_bits(); 7],
        referenced_rate_bits: 1f32.to_bits(),
        money_rate_bits: 1f32.to_bits(),
        corpse_empty_loot_show: false,
    }
}
/// Authored synthetic witnesses describe the test adapter order, not a database query.
fn namespace(tables: Vec<(u32, Vec<Row>)>, case: u32) -> Vec<Capture> {
    let total = tables.iter().map(|(_, rows)| rows.len() as u64).sum();
    let mut next = 0;
    tables
        .into_iter()
        .map(|(id, rows)| {
            let mut c = capture(rows, case);
            c.loot_id = id;
            c.loader_order.result_row_count = total;
            c.loader_order.selected_row_positions = (next..next + c.rows.len() as u64).collect();
            next += c.rows.len() as u64;
            c
        })
        .collect()
}
fn reference_catalog(body: Vec<Row>, children: Vec<(u32, Vec<Row>)>) -> Result<Catalog, Error> {
    Catalog::new_with_references(namespace(vec![(1, body)], 0), namespace(children, 0))
}
fn parse_rows(text: &str) -> Vec<Row> {
    if text == "-" {
        return vec![];
    }
    text.split(';')
        .map(|row| {
            let fields = row
                .split(':')
                .map(|n| n.parse::<i64>().unwrap())
                .collect::<Vec<_>>();
            assert_eq!(fields.len(), 5);
            Row {
                item: fields[0].try_into().unwrap(),
                chance_or_quest_chance_bits: fields[1].try_into().unwrap(),
                group: fields[2].try_into().unwrap(),
                min_count_or_reference: fields[3].try_into().unwrap(),
                max_count: fields[4].try_into().unwrap(),
                condition_id: 0,
            }
        })
        .collect()
}
#[test]
fn exact_pinned_one_hop_reference_process_and_draw_order() {
    let mut cases = 0;
    let mut cap_cases = 0;
    for line in include_str!("fixtures/loot_reference_cmangos.tsv").lines() {
        let columns = line.split('\t').collect::<Vec<_>>();
        assert_eq!(columns.len(), 9);
        assert!(columns.iter().all(|field| !field.is_empty()));
        let case: u32 = columns[0].parse().unwrap();
        let loot_id: u32 = columns[1].parse().unwrap();
        let money = nums(columns[2], ':');
        let rules = columns[3].split(':').collect::<Vec<_>>();
        assert_eq!(rules.len(), 4);
        let quality = nums(rules[0], ',');
        let rules = Rules {
            quality_rate_bits: std::array::from_fn(|i| quality[i] as u32),
            referenced_rate_bits: rules[1].parse().unwrap(),
            money_rate_bits: rules[2].parse().unwrap(),
            corpse_empty_loot_show: rules[3] == "1",
        };
        let body = if loot_id == 0 {
            vec![]
        } else {
            namespace(vec![(loot_id, parse_rows(columns[4]))], case)
        };
        let children = if columns[5] == "-" {
            vec![]
        } else {
            namespace(
                columns[5]
                    .split('|')
                    .map(|table| {
                        let (id, rows) = table.split_once('=').unwrap();
                        (id.parse().unwrap(), parse_rows(rows))
                    })
                    .collect(),
                case,
            )
        };
        let catalog = Catalog::new_with_references(body, children)
            .unwrap_or_else(|error| panic!("case {case}: {error}"));
        let mut random = Replay {
            draws: columns[6]
                .split(';')
                .filter(|draw| *draw != "-")
                .map(str::to_owned)
                .collect(),
        };
        let generated = catalog
            .generate(
                loot_id,
                money[0] as u32,
                money[1] as u32,
                &rules,
                &mut random,
            )
            .unwrap();
        assert!(random.draws.is_empty(), "unused draws in case {case}");
        let summary = nums(columns[7], ':');
        let items = if columns[8] == "-" {
            vec![]
        } else {
            columns[8]
                .split(';')
                .map(|item| {
                    let n = nums(item, ':');
                    LootItem {
                        slot: n[0] as u8,
                        item: n[1] as u32,
                        count: n[2] as u8,
                        display_id: n[3] as u32,
                        random_property: n[4] as i32,
                    }
                })
                .collect()
        };
        assert_eq!(
            generated,
            Generated {
                items,
                money: summary[0] as u32,
                fake: summary[1] != 0
            },
            "case {case}"
        );
        cap_cases += usize::from(generated.items.len() == MAX_ITEMS);
        cases += 1;
    }
    assert_eq!(cases, 1024);
    assert!(cap_cases > 0);
}

#[test]
fn reference_namespace_is_distinct_and_placeholder_is_not_a_leaf_item() {
    // The parent and child share ID 1; item 0 has no ItemPrototype dependency.
    let catalog = reference_catalog(
        vec![reference(1, 100., 1)],
        vec![(1, vec![ordinary(70001, 100., 0)])],
    )
    .unwrap();
    assert!(catalog.captures().next().unwrap().items.is_empty());
    assert_eq!(catalog.reference_captures().next().unwrap().loot_id, 1);
    let mut random = Replay {
        draws: VecDeque::from(["u:1:1:1".into()]),
    };
    let generated = catalog
        .generate(1, 0, 0, &unit_rules(), &mut random)
        .unwrap();
    assert_eq!(generated.items[0].item, 70001);
    assert!(random.draws.is_empty());
    assert!(!catalog.contains(2));
    let child_only = Catalog::new_with_references(
        vec![],
        namespace(vec![(1, vec![ordinary(70001, 100., 0)])], 0),
    );
    assert!(child_only.is_err());
}

#[test]
fn references_keep_source_chance_shortcut_zero_repetitions_and_tiny_chances() {
    let child = vec![(1, vec![ordinary(70001, 100., 0)])];
    let mut rules = unit_rules();
    rules.referenced_rate_bits = 0f32.to_bits();
    let catalog = reference_catalog(vec![reference(1, 100., 1)], child.clone()).unwrap();
    let mut random = Replay {
        draws: VecDeque::from(["u:1:1:1".into()]),
    };
    assert_eq!(
        catalog
            .generate(1, 0, 0, &rules, &mut random)
            .unwrap()
            .items
            .len(),
        1
    );
    assert!(random.draws.is_empty());
    let catalog = reference_catalog(vec![reference(1, 1e-8, 0)], child.clone()).unwrap();
    let mut random = Replay {
        draws: VecDeque::from([format!("c:{}", 0f64.to_bits())]),
    };
    assert!(catalog
        .generate(1, 0, 0, &unit_rules(), &mut random)
        .unwrap()
        .items
        .is_empty());
    assert!(random.draws.is_empty());
    assert!(reference_catalog(vec![reference(1, 100., 0)], vec![]).is_err());
    for rate in [f32::NAN, f32::INFINITY, -1., 1001.] {
        rules.referenced_rate_bits = rate.to_bits();
        let mut random = Replay {
            draws: VecDeque::new(),
        };
        assert_eq!(
            catalog.generate(1, 0, 0, &rules, &mut random),
            Err(Error::Invalid("loot rate"))
        );
    }
}

#[test]
fn rejects_nested_grouped_quest_conditioned_and_malformed_references() {
    let child = vec![(1, vec![ordinary(70001, 100., 0)])];
    let base = reference(1, 50., 1);
    for changed in [
        Row {
            group: 1,
            ..base.clone()
        },
        Row {
            condition_id: 1,
            ..base.clone()
        },
        Row {
            chance_or_quest_chance_bits: (-1f32).to_bits(),
            ..base.clone()
        },
        Row {
            chance_or_quest_chance_bits: f32::NAN.to_bits(),
            ..base.clone()
        },
        Row {
            chance_or_quest_chance_bits: 0f32.to_bits(),
            ..base.clone()
        },
        Row {
            min_count_or_reference: i32::MIN,
            ..base.clone()
        },
        Row {
            min_count_or_reference: 0,
            ..base.clone()
        },
        Row {
            max_count: 256,
            ..base.clone()
        },
        Row {
            min_count_or_reference: -2,
            ..base.clone()
        },
    ] {
        assert!(reference_catalog(vec![changed], child.clone()).is_err());
    }
    for nested in [reference(1, 100., 1), reference(2, 100., 1)] {
        assert!(reference_catalog(vec![base.clone()], vec![(1, vec![nested])]).is_err());
    }
    for changed in [
        Row {
            condition_id: 1,
            ..ordinary(70001, 100., 0)
        },
        ordinary(70001, -100., 0),
        Row {
            min_count_or_reference: 0,
            ..ordinary(70001, 100., 0)
        },
    ] {
        assert!(reference_catalog(vec![base.clone()], vec![(1, vec![changed])]).is_err());
    }
    assert!(reference_catalog(
        vec![base],
        vec![
            (1, vec![ordinary(70001, 100., 0)]),
            (2, vec![ordinary(70002, 100., 0)])
        ]
    )
    .is_err());
}

#[test]
fn references_require_exact_leaf_items_and_coherent_order_evidence() {
    let body = namespace(
        vec![(1, vec![reference(1, 100., 1), reference(2, 100., 0)])],
        0,
    );
    let refs = namespace(
        vec![
            (1, vec![ordinary(70001, 100., 0)]),
            (2, vec![ordinary(70002, 100., 0)]),
        ],
        0,
    );
    assert!(Catalog::new_with_references(body.clone(), refs.clone()).is_ok());
    for mutation in 0..8 {
        let mut changed = refs.clone();
        match mutation {
            0 => changed[0].items.clear(),
            1 => {
                changed[0].items.insert(70001, ItemLookup::Absent);
            }
            2 => changed[0].complete_selected_projection = false,
            3 => changed[0].loader_order.complete_result_observed = false,
            4 => changed[1].loader_order.ordered_result_fingerprint = [3; 32],
            5 => changed[1].loader_order.result_row_count += 1,
            6 => changed[1].loader_order.selected_row_positions[0] = 0,
            7 => changed[1].source_fingerprint = [4; 32],
            _ => unreachable!(),
        }
        assert!(
            Catalog::new_with_references(body.clone(), changed).is_err(),
            "mutation {mutation}"
        );
    }
    let mut changed_body = body.clone();
    changed_body[0].items.insert(0, ItemLookup::Absent);
    assert!(Catalog::new_with_references(changed_body, refs.clone()).is_err());
    let mut extra = refs.clone();
    let extra_item = extra[1].items[&70002].clone();
    extra[0].items.insert(70002, extra_item);
    assert!(Catalog::new_with_references(body.clone(), extra).is_err());
    let mut missing = refs;
    missing.pop();
    assert!(Catalog::new_with_references(body, missing).is_err());
}

#[test]
fn identity_commits_to_reference_rows_items_order_and_namespace() {
    let body = namespace(vec![(1, vec![reference(1, 100., 1)])], 0);
    let children = namespace(
        vec![(1, vec![ordinary(70001, 100., 0), ordinary(70002, 100., 0)])],
        0,
    );
    let identity = Catalog::new_with_references(body.clone(), children.clone())
        .unwrap()
        .identity();
    for mutation in 0..9 {
        let mut parent = body.clone();
        let mut child = children.clone();
        match mutation {
            0 => parent[0].rows[0].item = u32::MAX,
            1 => parent[0].rows[0].max_count = 2,
            2 => child[0].rows[0].chance_or_quest_chance_bits = 50f32.to_bits(),
            3 => child[0].rows.swap(0, 1),
            4 => child[0].source_fingerprint = [3; 32],
            5 => child[0].loader_order.ordered_result_fingerprint = [4; 32],
            6 => child[0].loader_order.result_row_count += 1,
            7 => {
                child[0].loader_order.result_row_count += 1;
                child[0].loader_order.selected_row_positions[1] += 1;
            }
            8 => {
                let ItemLookup::Present(item) = child[0].items.get_mut(&70001).unwrap() else {
                    unreachable!()
                };
                item.display_id += 1;
            }
            _ => unreachable!(),
        }
        assert_ne!(
            Catalog::new_with_references(parent, child)
                .unwrap()
                .identity(),
            identity,
            "mutation {mutation}"
        );
    }
    let ordinary_body = Catalog::new(children).unwrap();
    assert_ne!(ordinary_body.identity(), identity);
    assert_ne!(Catalog::new(vec![]).unwrap().identity(), [0; 32]);
}

#[test]
fn repeated_leaf_metadata_must_agree_across_both_namespaces() {
    let mut body = namespace(
        vec![(1, vec![ordinary(70001, 100., 0), reference(1, 100., 1)])],
        0,
    );
    let refs = namespace(vec![(1, vec![ordinary(70001, 100., 0)])], 0);
    assert!(Catalog::new_with_references(body.clone(), refs.clone()).is_ok());
    let ItemLookup::Present(item) = body[0].items.get_mut(&70001).unwrap() else {
        unreachable!()
    };
    item.quality += 1;
    assert_eq!(
        Catalog::new_with_references(body, refs),
        Err(Error::Invalid("inconsistent item closure"))
    );
}

#[test]
fn admission_bounds_draws_and_work_before_the_output_cap_can_hide_repetition() {
    let child = (0..16)
        .map(|i| ordinary(70001 + i, 50., 0))
        .collect::<Vec<_>>();
    assert!(reference_catalog(vec![reference(1, 50., 127)], vec![(1, child.clone())]).is_ok());
    assert_eq!(
        reference_catalog(vec![reference(1, 50., 128)], vec![(1, child)]),
        Err(Error::Invalid("loot processing budget"))
    );
    let child = (0..255)
        .map(|i| ordinary(70001 + i, 100., 1))
        .collect::<Vec<_>>();
    assert!(reference_catalog(vec![reference(1, 100., 7)], vec![(1, child.clone())]).is_ok());
    assert_eq!(
        reference_catalog(vec![reference(1, 100., 8)], vec![(1, child)]),
        Err(Error::Invalid("loot processing budget"))
    );
    assert!(reference_catalog(
        vec![reference(1, 100., u32::MAX)],
        vec![(1, vec![ordinary(70001, 100., 0)])]
    )
    .is_err());
    let rows = vec![ordinary(70001, 100., 0); 1024];
    let captures = namespace((1..=4).map(|id| (id, rows.clone())).collect(), 0);
    assert!(Catalog::new(captures).is_ok());
    let captures = namespace((1..=5).map(|id| (id, rows.clone())).collect(), 0);
    assert_eq!(
        Catalog::new(captures),
        Err(Error::Invalid("retained loot closure size"))
    );
}
