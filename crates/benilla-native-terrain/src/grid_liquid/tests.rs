// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::{cell::RefCell, collections::BTreeMap};

struct Grid {
    liquid: GridLiquid,
    ground: f32,
    types: Option<BTreeMap<u32, u32>>,
    areas: Option<BTreeMap<u32, Area>>,
    calls: RefCell<Vec<(char, u32)>>,
}
impl Grid {
    fn new(entry: u16, flags: u8, level: f32, ground: f32) -> Self {
        let mut bytes = b"MLIQ".to_vec();
        bytes.extend([3, flags]);
        bytes.extend(entry.to_le_bytes());
        bytes.extend([0, 0, 128, 128]);
        bytes.extend(level.to_le_bytes());
        Self {
            liquid: GridLiquid::decode(bytes.into(), 0).unwrap(),
            ground,
            types: Some(BTreeMap::new()),
            areas: Some(BTreeMap::new()),
            calls: RefCell::new(Vec::new()),
        }
    }
    fn status(&self, z: f32, mask: u8, out: &mut LiquidData) -> Result<Status, Error> {
        self.liquid
            .status([0.0, 0.0, z], mask, 2.0, Some(out), self)
    }
}
impl Lookup for Grid {
    type Error = Error;
    fn liquid_type(&self, id: u32) -> Result<Option<u32>, Error> {
        self.calls.borrow_mut().push(('l', id));
        Ok(self
            .types
            .as_ref()
            .ok_or(Error::LiquidTypeStoreUnavailable)?
            .get(&id)
            .copied())
    }
    fn area(&self, id: u32) -> Result<Option<Area>, Error> {
        self.calls.borrow_mut().push(('a', id));
        Ok(self
            .areas
            .as_ref()
            .ok_or(Error::AreaStoreUnavailable)?
            .get(&id)
            .copied())
    }
}
impl GridOwner for Grid {
    fn area_key(&self, _: f32, _: f32) -> Result<u16, Error> {
        Ok(100)
    }
    fn height(&self, _: f32, _: f32) -> Result<f32, Error> {
        Ok(self.ground)
    }
}
fn sentinel() -> LiquidData {
    LiquidData {
        entry: 0xdeadbeef,
        type_flags: 0xdecafbad,
        level: f32::from_bits(0x4a123456),
        depth_level: f32::from_bits(0xca654321),
    }
}
#[test]
fn strict_delta_thresholds_and_ground_guards_keep_output_on_every_exit() {
    let grid = Grid::new(21, 1, 8.0, 0.0);
    // GridMap.cpp:629,632,635: exact and adjacent binary32 Z values.
    for (z, expected) in [
        (f32::from_bits(6.0f32.to_bits() - 1), Status::UnderWater),
        (6.0, Status::InWater),
        (f32::from_bits(6.0f32.to_bits() + 1), Status::InWater),
        (f32::from_bits(8.0f32.to_bits() - 1), Status::InWater),
        (8.0, Status::WaterWalk),
        (f32::from_bits(8.0f32.to_bits() + 1), Status::WaterWalk),
        (f32::from_bits(9.0f32.to_bits() - 1), Status::WaterWalk),
        (9.0, Status::AboveWater),
        (f32::from_bits(9.0f32.to_bits() + 1), Status::AboveWater),
    ] {
        let mut out = sentinel();
        assert_eq!(grid.status(z, 0, &mut out), Ok(expected));
        assert_eq!((out.level, out.depth_level), (8.0, 0.0));
    }
    for (z, expected) in [
        (-2.0000002, Status::NoWater),
        (-2.0, Status::UnderWater),
        (-1.9999999, Status::UnderWater),
    ] {
        let mut out = sentinel();
        assert_eq!(grid.status(z, 0, &mut out), Ok(expected));
        if expected == Status::NoWater {
            assert_eq!(out, sentinel());
        }
    }
    for (level, expected) in [
        (f32::from_bits(1.0f32.to_bits() - 1), Status::NoWater),
        (1.0, Status::WaterWalk),
        (f32::from_bits(1.0f32.to_bits() + 1), Status::InWater),
    ] {
        let grid = Grid::new(21, 1, level, 1.0);
        let mut out = sentinel();
        assert_eq!(grid.status(1.0, 0, &mut out), Ok(expected));
        if expected == Status::NoWater {
            assert_eq!(out, sentinel());
        }
    }
    for (mask, expected) in [
        (0, Status::UnderWater),
        (1, Status::UnderWater),
        (2, Status::NoWater),
    ] {
        let mut out = sentinel();
        assert_eq!(grid.status(0.0, mask, &mut out), Ok(expected));
        if expected == Status::NoWater {
            assert_eq!(out, sentinel());
        }
    }
}
#[test]
fn entry_twenty_one_boundary_parent_reach_deep_flag_and_narrowing_are_source_ordered() {
    for entry in [0, 20, 21] {
        let mut grid = Grid::new(entry, 0x3f, 8.0, 0.0);
        grid.types
            .as_mut()
            .unwrap()
            .extend([(u32::from(entry), 32), (99, 31)]);
        grid.areas.as_mut().unwrap().extend([
            (
                100,
                Area {
                    zone: 200,
                    liquid_override: 0,
                },
            ),
            (
                200,
                Area {
                    zone: 300,
                    liquid_override: 99,
                },
            ),
        ]);
        let mut out = sentinel();
        if entry < 21 {
            assert_eq!(grid.status(0.0, 0, &mut out), Ok(Status::UnderWater));
            assert_eq!((out.entry, out.type_flags), (99, 16));
            assert_eq!(
                *grid.calls.borrow(),
                [('l', u32::from(entry)), ('a', 100), ('a', 200), ('l', 99)]
            );
        } else {
            assert_eq!(
                grid.status(0.0, 0, &mut out),
                Err(Error::TypeShiftUndefined)
            );
            assert_eq!(out, sentinel());
            assert_eq!(*grid.calls.borrow(), [('l', 21)]);
        }
    }
    let mut grid = Grid::new(1, 16, 8.0, 0.0);
    grid.types.as_mut().unwrap().extend([(1, 0), (99, 1)]);
    grid.areas.as_mut().unwrap().insert(
        100,
        Area {
            zone: 200,
            liquid_override: 99,
        },
    );
    let mut out = sentinel();
    grid.status(0.0, 0, &mut out).unwrap();
    assert_eq!(out.type_flags, 18);
    assert_eq!(*grid.calls.borrow(), [('l', 1), ('a', 100), ('l', 99)]);
    assert_eq!(type_flag(31), Ok(0x80000000));
    assert_eq!(type_flag(32), Err(Error::TypeShiftUndefined));
}
#[test]
fn absent_stores_and_short_reads_refuse_only_at_the_reached_operation() {
    let mut grid = Grid::new(1, 1, 8.0, 0.0);
    grid.types = None;
    grid.areas = None;
    let mut out = sentinel();
    assert_eq!(
        grid.status(0.0, 0, &mut out),
        Err(Error::LiquidTypeStoreUnavailable)
    );
    grid.types = Some([(1, 0)].into());
    assert_eq!(
        grid.status(0.0, 0, &mut out),
        Err(Error::AreaStoreUnavailable)
    );
    grid.types = Some(BTreeMap::new());
    assert_eq!(grid.status(0.0, 0, &mut out), Ok(Status::UnderWater));
    let dry = GridLiquid::default();
    assert_eq!(
        dry.status([f32::NAN; 3], 0, 2.0, Some(&mut out), &grid),
        Ok(Status::NoWater)
    );
    assert_eq!(out.level, 8.0);
    let mut bytes = b"MLIQ\0\0\0\0\0\0\x80\x80".to_vec();
    bytes.extend(8.0f32.to_le_bytes());
    let short = GridLiquid::decode(bytes.clone().into(), 0).unwrap();
    assert_eq!(
        short.status([0.0; 3], 0, 2.0, None, &grid),
        Err(Error::FlagsReadUndefined)
    );
    bytes.extend([0; 512]);
    bytes.extend([1; 256]);
    let short = GridLiquid::decode(bytes.clone().into(), 0).unwrap();
    assert_eq!(
        short.status([0.0; 3], 0, 2.0, None, &grid),
        Err(Error::HeightReadUndefined)
    );
    bytes.extend(8.0f32.to_le_bytes());
    let complete = GridLiquid::decode(bytes.into(), 0).unwrap();
    assert_eq!(
        complete.status([0.0; 3], 0, 2.0, None, &grid),
        Ok(Status::UnderWater)
    );
    assert_eq!(
        complete.status([f32::NAN, 0.0, 0.0], 0, 2.0, None, &grid),
        Err(Error::CoordinateConversionUndefined)
    );
}
#[test]
fn swapped_offsets_rectangle_edges_and_direct_level_flat_short_circuit() {
    let grid = Grid::new(21, 1, 8.0, 0.0);
    let mut bytes = b"MLIQ\x01\x01\x15\0\x02\x01\x03\x02".to_vec();
    bytes.extend(8.0f32.to_le_bytes());
    for n in 0..6 {
        bytes.extend((10.0 + n as f32).to_le_bytes());
    }
    let liquid = GridLiquid::decode(bytes.into(), 0).unwrap();
    let coord = |cell: f32| -cell * (grid_height::GRID_SIZE / 128.0);
    for (row, col, expected) in [
        (0.0, 2.0, INVALID_HEIGHT_VALUE),
        (1.0, 1.0, INVALID_HEIGHT_VALUE),
        (1.0, 2.0, 10.0),
        (2.0, 4.0, 15.0),
        (3.0, 4.0, INVALID_HEIGHT_VALUE),
        (2.0, 5.0, INVALID_HEIGHT_VALUE),
    ] {
        assert_eq!(liquid.level(coord(row), coord(col)).unwrap(), expected);
        let mut out = sentinel();
        let status = liquid
            .status([coord(row), coord(col), 0.0], 0, 2.0, Some(&mut out), &grid)
            .unwrap();
        assert_eq!(status == Status::NoWater, expected == INVALID_HEIGHT_VALUE);
        if status == Status::NoWater {
            assert_eq!(out, sentinel());
        }
    }
    assert_eq!(grid.liquid.level(f32::NAN, f32::NAN), Ok(8.0));
}
