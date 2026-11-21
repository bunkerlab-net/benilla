// SPDX-License-Identifier: GPL-3.0-or-later
//! Existing original-C++ fixtures supply authored inputs and independent outputs.
use super::*;
use crate::grid_area::AreaData;
use std::collections::BTreeMap;
const WET: &str =
    include_str!("../../../benilla-native-geometry-tests/tools/fixtures/wet-liquid.tsv");
const AREA: &str =
    include_str!("../../../benilla-native-geometry-tests/tools/fixtures/area-liquid.tsv");
struct Owner {
    types: BTreeMap<u32, u32>,
    areas: BTreeMap<u32, Area>,
    area: AreaData,
    ground: f32,
}
impl Lookup for Owner {
    type Error = Error;
    fn liquid_type(&self, id: u32) -> Result<Option<u32>, Error> {
        Ok(self.types.get(&id).copied())
    }
    fn area(&self, id: u32) -> Result<Option<Area>, Error> {
        Ok(self.areas.get(&id).copied())
    }
}
impl GridOwner for Owner {
    fn area_key(&self, x: f32, y: f32) -> Result<u16, Error> {
        self.area
            .get(x, y)
            .ok_or(Error::CoordinateConversionUndefined)
    }
    fn height(&self, _: f32, _: f32) -> Result<f32, Error> {
        Ok(self.ground)
    }
}
fn number(s: &str) -> u32 {
    s.parse().unwrap()
}
fn bits(s: &str) -> u32 {
    u32::from_str_radix(s, 16).unwrap()
}
fn word(b: &mut Vec<u8>, v: u32) {
    b.extend(v.to_le_bytes());
}
fn sentinel() -> LiquidData {
    LiquidData {
        entry: 0xdeadbeef,
        type_flags: 0xdecafbad,
        level: f32::from_bits(0x4a123456),
        depth_level: f32::from_bits(0xca654321),
    }
}
fn verify(liquid: &GridLiquid, owner: &Owner, r: &[&str]) {
    let mut out = sentinel();
    let actual = liquid
        .status(
            [1, 2, 3].map(|i| f32::from_bits(bits(r[i]))),
            number(r[5]) as u8,
            f32::from_bits(bits(r[4])),
            (number(r[7]) != 0).then_some(&mut out),
            owner,
        )
        .unwrap();
    assert_eq!(actual as u32, number(r[8]), "{r:?}");
    assert_eq!(
        (
            out.entry,
            out.type_flags,
            out.level.to_bits(),
            out.depth_level.to_bits()
        ),
        (number(r[9]), number(r[10]), bits(r[11]), bits(r[12])),
        "{r:?}"
    );
}
#[test]
fn original_cpp_wet_liquid_rows_match_shared_zero_copy_arrays() {
    let mut profiles = BTreeMap::new();
    for l in WET.lines().filter(|l| l.starts_with("# profile\t")) {
        let r: Vec<_> = l.split('\t').collect();
        if r[1] == "x_bits" {
            continue;
        }
        let n = |i: usize| number(r[i]);
        let (types, heights, zero) = (n(9) != 0, n(10) != 0, n(11) != 0);
        let mut b = b"MLIQ".to_vec();
        b.extend([u8::from(!types) | (u8::from(!heights) << 1), n(3) as u8]);
        b.extend((n(2) as u16).to_le_bytes());
        for i in 4..=7 {
            b.push(n(i) as u8);
        }
        word(&mut b, bits(r[8]));
        if types {
            for i in 0..256 {
                b.extend([21u16, 22, 23, 24, 77, 0, 20][i % 7].to_le_bytes());
            }
            for i in 0..256 {
                b.push(if zero {
                    0
                } else {
                    [0, 1, 2, 4, 8, 16, 24, 32, 63][i % 9]
                });
            }
        }
        if heights {
            for row in 0..n(7) {
                for col in 0..n(6) {
                    word(
                        &mut b,
                        (10.0 + row as f32 * 0.5 + col as f32 * 0.25).to_bits(),
                    );
                }
            }
        }
        profiles.insert(n(1), GridLiquid::decode(b.into(), 0).unwrap());
    }
    let owner = &mut Owner {
        types: [(21, 3), (22, 1), (23, 0), (24, 2)].into(),
        areas: BTreeMap::new(),
        area: AreaData::default(),
        ground: 0.0,
    };
    let mut count = 0;
    for l in WET.lines().filter(|l| !l.starts_with('#') && !l.is_empty()) {
        let r: Vec<_> = l.split('\t').collect();
        owner.ground = f32::from_bits(bits(r[6]));
        verify(&profiles[&number(r[0])], owner, &r);
        count += 1;
    }
    assert_eq!((profiles.len(), count), (10, 2953));
}
#[test]
fn original_cpp_area_liquid_rows_match_parent_and_raw_id_selection() {
    let mut types: BTreeMap<u32, BTreeMap<u32, u32>> = BTreeMap::new();
    let mut areas: BTreeMap<u32, BTreeMap<u32, Area>> = BTreeMap::new();
    for l in AREA
        .lines()
        .filter(|l| l.starts_with("# liquid_row\t") || l.starts_with("# area_row\t"))
    {
        let r: Vec<_> = l.split('\t').skip(1).map(number).collect();
        if l.starts_with("# liquid_row") {
            types.entry(r[0]).or_default().insert(r[1], r[3]);
        } else {
            areas.entry(r[0]).or_default().insert(
                r[1],
                Area {
                    zone: r[3],
                    liquid_override: r[25],
                },
            );
        }
    }
    let mut profiles = BTreeMap::new();
    let area_cycle = [
        100u16, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 112, 114, 115, 0, 300, 65535,
        200, 201, 202, 500, 9000, 107, 109,
    ];
    for l in AREA.lines().filter(|l| l.starts_with("# profile\t")) {
        let r: Vec<_> = l.split('\t').collect();
        if r[1] == "x_bits" {
            continue;
        }
        let n = |i: usize| number(r[i]);
        let mut a = b"AREA".to_vec();
        a.extend(u16::from(n(6) == 0).to_le_bytes());
        a.extend((n(3) as u16).to_le_bytes());
        if n(6) != 0 {
            for i in 0..256 {
                a.extend(area_cycle[i % 24].to_le_bytes());
            }
        }
        let mut b = b"MLIQ".to_vec();
        b.extend([2 | u8::from(n(7) == 0), n(5) as u8]);
        b.extend((n(4) as u16).to_le_bytes());
        b.extend([0, 0, 128, 128]);
        word(&mut b, 0);
        if n(7) != 0 {
            for i in 0..256 {
                b.extend([0u16, 1, 2, 3, 20, 21, 22, 77][(i / 24) % 8].to_le_bytes());
            }
            for i in 0..256 {
                b.push([0, 1, 16, 32, 63][i % 5]);
            }
        }
        let store = n(2);
        profiles.insert(
            n(1),
            (
                GridLiquid::decode(b.into(), 0).unwrap(),
                Owner {
                    types: types[&store].clone(),
                    areas: areas[&store].clone(),
                    area: AreaData::decode(&a).unwrap(),
                    ground: 0.0,
                },
            ),
        );
    }
    let mut count = 0;
    for l in AREA
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let r: Vec<_> = l.split('\t').collect();
        let (liquid, owner) = profiles.get_mut(&number(r[0])).unwrap();
        owner.ground = f32::from_bits(bits(r[6]));
        assert_eq!(
            u32::from(
                owner
                    .area_key(f32::from_bits(bits(r[1])), f32::from_bits(bits(r[2])))
                    .unwrap()
            ),
            number(r[13])
        );
        verify(liquid, owner, &r);
        count += 1;
    }
    assert_eq!((profiles.len(), count), (35, 1931));
}
