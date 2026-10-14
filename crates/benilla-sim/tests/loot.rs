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
