// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::creature_capabilities::{self, InhabitType};

const FIXTURE: &str = include_str!(
    "../../../benilla-native-geometry-tests/tools/fixtures/route_filter/cases.test-fixture"
);

fn capabilities(habitat: u32, flags: u32) -> MovementCapabilities {
    creature_capabilities::evaluate(InhabitType::try_from_raw(habitat).unwrap(), flags)
}

#[test]
fn dry_filter_and_unsupported_controls_match_unchanged_source_observations() {
    let mut rows = 0;
    let mut accepted = 0;
    let mut wet_rejections = 0;
    for line in FIXTURE.lines().filter(|line| !line.starts_with('#')) {
        let row: Vec<&str> = line.split('\t').collect();
        assert_eq!(row.len(), 26, "{line}");
        let number = |index: usize| row[index].parse::<u16>().unwrap();
        let walk = number(2) != 0;
        let swim = number(3) != 0;
        let habitat = u32::from(walk) | (u32::from(swim) << 1);
        let caps = capabilities(if habitat == 0 { 4 } else { habitat }, 0);
        rows += 1;
        if number(4) == 0 && number(5) == 0 {
            // Source can skip both liquid reads with no grid. The host must reject
            // missing loaded coverage rather than turn that path into a dry proof.
            assert_eq!(row[24], "-");
            assert_eq!(number(23), 0);
            continue;
        }
        let statuses = [number(9) as u8, number(11) as u8];
        let mut calls = Vec::new();
        let result = create_ground_filter(caps, |mask| {
            let index = calls.len();
            calls.push(mask);
            assert_eq!(
                mask,
                [LiquidMask::AllLiquids, LiquidMask::WaterOrOcean][index],
                "{line}"
            );
            NoWater::try_from_status(statuses[index])
        });
        if !walk || swim {
            assert_eq!(result, Err(Error::UnsupportedCapabilities), "{line}");
            assert!(calls.is_empty());
        } else if let Some(index) = statuses.iter().position(|status| *status != 0) {
            assert_eq!(
                result,
                Err(Error::Liquid(UnsupportedLiquidStatus {
                    status: statuses[index]
                })),
                "{line}"
            );
            assert_eq!(calls.len(), index + 1, "{line}");
            wet_rejections += 1;
        } else {
            let filter = result.unwrap();
            assert_eq!(filter.include_flags(), number(15), "{line}");
            assert_eq!(filter.exclude_flags(), number(16), "{line}");
            assert_eq!(filter.branch(), Branch::GroundDry);
            assert_eq!(number(17), filter.include_flags());
            assert_eq!(number(18), 1, "no updateFilter write on dry path");
            assert_eq!(number(20), 0, "no getNavTerrain/filter read on dry path");
            assert_eq!(number(23), 2);
            assert_eq!(row[24], "15,10");
            assert_eq!(calls.len(), 2);
            accepted += 1;
        }
    }
    assert_eq!(rows, 87);
    assert_eq!(accepted, 4);
    assert_eq!(wet_rejections, 77);
}

#[test]
fn only_exact_no_water_is_supported_including_unknown_statuses() {
    assert!(NoWater::try_from_status(0).is_ok());
    for status in 1..=u8::MAX {
        assert_eq!(
            NoWater::try_from_status(status),
            Err(UnsupportedLiquidStatus { status })
        );
    }
}

#[test]
fn capability_rejection_precedes_liquid_queries() {
    for habitat in 1..=7 {
        for flags in [
            0,
            creature_capabilities::MOVEFLAG_LEVITATING,
            creature_capabilities::MOVEFLAG_HOVER,
            creature_capabilities::MOVEFLAG_CAN_FLY,
            u32::MAX,
        ] {
            let caps = capabilities(habitat, flags);
            if caps.can_walk() && !caps.can_swim() && !caps.can_fly() {
                continue;
            }
            assert_eq!(
                create_ground_filter::<()>(caps, |_| panic!("unsupported domain queried liquid")),
                Err(Error::UnsupportedCapabilities)
            );
        }
    }
}

#[test]
fn current_modes_do_not_invent_capability_and_callback_errors_stop() {
    let caps = capabilities(
        1,
        creature_capabilities::MOVEFLAG_WALK_MODE
            | creature_capabilities::MOVEFLAG_SWIMMING
            | creature_capabilities::MOVEFLAG_FLYING,
    );
    let mut calls = Vec::new();
    assert_eq!(
        create_ground_filter(caps, |mask| {
            calls.push(mask);
            Err("unproved loaded coverage")
        }),
        Err(Error::Liquid("unproved loaded coverage"))
    );
    assert_eq!(calls, [LiquidMask::AllLiquids]);
    calls.clear();
    assert_eq!(
        create_ground_filter(caps, |mask| {
            calls.push(mask);
            if mask == LiquidMask::AllLiquids {
                Ok(NoWater::try_from_status(0).unwrap())
            } else {
                Err("second observation missing")
            }
        }),
        Err(Error::Liquid("second observation missing"))
    );
    assert_eq!(calls, [LiquidMask::AllLiquids, LiquidMask::WaterOrOcean]);
}

#[test]
fn original_masks_and_flags_and_full_source_hashes_are_pinned() {
    assert_eq!(LiquidMask::AllLiquids.bits(), 15);
    assert_eq!(LiquidMask::WaterOrOcean.bits(), 10);
    assert_eq!(
        [NAV_GROUND, NAV_GROUND_STEEP, NAV_WATER, NAV_MAGMA_SLIME],
        [1, 2, 4, 8]
    );
    let manifest = include_str!(
        "../../../benilla-native-geometry-tests/tools/route_filter_source_hashes.sha256"
    );
    let hashes: Vec<&str> = manifest
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| line.split_whitespace().next().unwrap())
        .collect();
    assert_eq!(hashes, SOURCE_SHA256);
}
