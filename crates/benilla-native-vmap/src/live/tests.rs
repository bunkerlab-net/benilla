// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored products only. No install assets or recorded answers.
use super::*;
use std::time::Duration;

#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
#[test]
fn default_arithmetic_environment_matches_source_profile() {
    assert_eq!(arithmetic_environment(), Ok(()));
}

#[test]
fn liquid_interpolation_nan_is_native_on_x86_and_refused_on_aarch64() {
    for overflow in [false, true] {
        let mut m = Manager::new();
        present(&mut m, b"000.vmtree", tree(true, 1, &[]));
        let mut bytes = model(&[(1, 0.0)], true);
        if overflow {
            let liquid = bytes.windows(4).position(|v| v == b"LIQU").unwrap();
            for (i, height) in [-f32::MAX, -f32::MAX, f32::MAX, -f32::MAX]
                .into_iter()
                .enumerate()
            {
                let start = liquid + 32 + i * 4;
                bytes[start..start + 4].copy_from_slice(&height.to_le_bytes());
            }
        }
        present(&mut m, b"test.wmo.vmo", bytes);
        load(&mut m, 1, 2, &[spawn(0, 4, 1, 2, 0.0)]);
        let mut out = LiquidOutput {
            level: 99.0,
            floor: 98.0,
            kind: 97,
        };
        let result = m.liquid_level(0, [0.0, 0.0, 10.0], 0, &mut out, deadline());
        if overflow {
            #[cfg(target_arch = "x86_64")]
            {
                assert_eq!(result, Ok(true));
                assert!(out.level.is_nan());
            }
            #[cfg(target_arch = "aarch64")]
            {
                assert_eq!(result, Err(Error::LiquidInterpolationNaN));
                assert_eq!(out.level, 99.0);
            }
        } else {
            assert_eq!(result, Ok(true));
            assert_eq!(out.level, 3.0);
        }
    }
}

#[test]
fn hit_position_nan_is_native_on_x86_and_refused_on_aarch64() {
    let mut m = fixture(1, &[(1, 0.0)], false);
    load(&mut m, 1, 2, &[spawn(0, 4, 1, 2, 0.0)]);
    for modify in [f32::NAN, f32::INFINITY] {
        let result = m.hit_position(0, [0.0, 0.0, 10.0], [0.0, 0.0, -5.0], modify, deadline());
        #[cfg(target_arch = "x86_64")]
        {
            let (hit, position) = result.unwrap();
            assert!(hit);
            assert!(position[0].is_nan());
            assert!(position[1].is_nan());
        }
        #[cfg(target_arch = "aarch64")]
        assert_eq!(result, Err(Error::HitPositionNaN));
        assert_eq!(
            m.hit_position(0, [0.0, 0.0, 10.0], [0.0, 0.0, 5.0], modify, deadline()),
            Ok((false, [0.0, 0.0, 5.0]))
        );
    }
}
fn word(b: &mut Vec<u8>, w: u32) {
    b.extend_from_slice(&w.to_le_bytes());
}
fn vector(b: &mut Vec<u8>, v: Vector) {
    for f in v {
        word(b, f.to_bits());
    }
}
fn box_bytes(b: &mut Vec<u8>, lo: Vector, hi: Vector) {
    vector(b, lo);
    vector(b, hi);
}
fn bih(b: &mut Vec<u8>, lo: Vector, hi: Vector, objects: &[u32]) {
    box_bytes(b, lo, hi);
    word(b, 3);
    word(b, 3 << 30);
    word(b, objects.len() as u32);
    word(b, 0);
    word(b, objects.len() as u32);
    for &v in objects {
        word(b, v);
    }
}
fn local_bounds() -> (Vector, Vector) {
    ([-10.0, -10.0, -10.0], [10.0, 10.0, 20.0])
}
fn world_bounds() -> (Vector, Vector) {
    let mid = to_internal([0.0; 3]);
    (
        [mid[0] - 10.0, mid[1] - 10.0, -10.0],
        [mid[0] + 10.0, mid[1] + 10.0, 20.0],
    )
}
fn model(groups: &[(u32, f32)], wet: bool) -> Vec<u8> {
    let mut b = b"VMAP_7.0WMOD".to_vec();
    word(&mut b, 8);
    word(&mut b, 17);
    b.extend(b"GMOD");
    word(&mut b, groups.len() as u32);
    let (lo, hi) = local_bounds();
    for &(id, z) in groups {
        box_bytes(&mut b, lo, hi);
        word(&mut b, id + 100);
        word(&mut b, id);
        b.extend(b"VERT");
        word(&mut b, 40);
        word(&mut b, 3);
        for p in [[-10.0, -10.0, z], [10.0, -10.0, z], [0.0, 10.0, z]] {
            vector(&mut b, p);
        }
        b.extend(b"TRIM");
        word(&mut b, 16);
        word(&mut b, 1);
        for i in 0..3 {
            word(&mut b, i);
        }
        b.extend(b"MBIH");
        bih(&mut b, lo, hi, &[0]);
        b.extend(b"LIQU");
        if wet {
            word(&mut b, 41);
            word(&mut b, 1);
            word(&mut b, 1);
            vector(&mut b, [-1.0, -1.0, 0.0]);
            word(&mut b, 1);
            for _ in 0..4 {
                word(&mut b, 3.0f32.to_bits());
            }
            b.push(0);
        } else {
            word(&mut b, 0);
        }
    }
    b.extend(b"GBIH");
    bih(
        &mut b,
        lo,
        hi,
        &(0..groups.len() as u32).collect::<Vec<_>>(),
    );
    b
}
fn spawn(slot: u32, flags: u32, id: u32, adt: u16, angle: f32) -> Vec<u8> {
    let mut b = Vec::new();
    word(&mut b, flags);
    b.extend(adt.to_le_bytes());
    word(&mut b, id);
    vector(&mut b, to_internal([0.0; 3]));
    vector(&mut b, [angle, 0.0, 0.0]);
    word(&mut b, 1.0f32.to_bits());
    if flags & 4 != 0 {
        let (lo, hi) = world_bounds();
        box_bytes(&mut b, lo, hi);
    }
    word(&mut b, 8);
    b.extend(b"test.wmo");
    word(&mut b, slot);
    b
}
fn tree(tiled: bool, slots: u32, globals: &[Vec<u8>]) -> Vec<u8> {
    let mut b = b"VMAP_7.0".to_vec();
    b.push(u8::from(tiled));
    b.extend(b"NODE");
    let (lo, hi) = world_bounds();
    bih(&mut b, lo, hi, &(0..slots).collect::<Vec<_>>());
    b.extend(b"GOBJ");
    for s in globals {
        b.extend(s);
    }
    b
}
fn tile(spawns: &[Vec<u8>]) -> Vec<u8> {
    let mut b = b"VMAP_7.0".to_vec();
    word(&mut b, spawns.len() as u32);
    for s in spawns {
        b.extend(s);
    }
    b
}
fn present(m: &mut Manager, name: impl AsRef<[u8]>, bytes: Vec<u8>) {
    m.files
        .observe(name, FileState::Present(bytes.into()))
        .unwrap();
}
fn fixture(slots: u32, groups: &[(u32, f32)], wet: bool) -> Manager {
    let mut m = Manager::new();
    present(&mut m, b"000.vmtree", tree(true, slots, &[]));
    present(&mut m, b"test.wmo.vmo", model(groups, wet));
    m
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(30)
}
fn load(m: &mut Manager, x: u32, y: u32, spawns: &[Vec<u8>]) {
    present(m, Manager::tile_filename(0, x, y), tile(spawns));
    assert_eq!(m.load_map(0, x, y).unwrap(), LoadResult::Ok);
}
fn blocked(m: &Manager, ignore: bool) -> bool {
    !m.line_of_sight(0, [0.0, 0.0, 10.0], [0.0, 0.0, -5.0], ignore, deadline())
        .unwrap()
}
#[test]
fn asymmetric_filenames_absent_and_empty_tiles_are_distinct() {
    // MapTree.cpp:89-96,365-437.
    assert_eq!(Manager::tile_filename(1, 7, 12), "001_12_07.vmtile");
    let mut m = fixture(0, &[(1, 0.0)], false);
    m.files
        .observe(b"000_12_07.vmtile", FileState::Absent)
        .unwrap();
    assert_eq!(m.load_map(0, 7, 12).unwrap(), LoadResult::Ok);
    load(&mut m, 12, 7, &[]);
    assert_eq!(
        m.tree_inventory()[0].tiles,
        vec![((7, 12), false), ((12, 7), true)]
    );
    assert!(m.is_tile_loaded(0, 7, 12));
    assert!(m.is_tile_loaded(0, 12, 7));
    m.unload_tile(0, 7, 12).unwrap();
    assert!(m.has_tree(0));
    m.unload_tile(0, 12, 7).unwrap();
    assert!(!m.has_tree(0));
}
#[test]
fn global_non_tiled_model_uses_logical_tiles_without_tile_files() {
    // MapTree.cpp:298-341,367-372; last logical tile deletes the tree without UnloadMap.
    let mut m = fixture(1, &[(1, 0.0)], false);
    let mut global = Manager::new();
    present(
        &mut global,
        b"000.vmtree",
        tree(false, 1, &[spawn(0, 6, 1, 2, 0.0)]),
    );
    present(&mut global, b"test.wmo.vmo", model(&[(1, 0.0)], false));
    assert_eq!(global.load_map(0, 7, 12).unwrap(), LoadResult::Ok);
    assert!(blocked(&global, false));
    global.load_map(0, 12, 7).unwrap();
    assert_eq!(global.model_inventory()[0].references, 1);
    global.unload_tile(0, 7, 12).unwrap();
    assert!(blocked(&global, false));
    global.unload_map(0);
    assert!(global.model_inventory().is_empty());
    assert!(!global.has_tree(0));
    // A separate tiled owner remains independent.
    load(&mut m, 1, 2, &[spawn(0, 4, 1, 2, 0.0)]);
    assert!(blocked(&m, false));
}
#[test]
fn shared_spawn_retains_first_slot_and_two_actual_model_references() {
    // MapTree.cpp:399-429 acquires and mutates flags even for an existing slot.
    let mut m = fixture(1, &[(1, 0.0)], false);
    load(&mut m, 1, 2, &[spawn(0, 4, 1, 2, 0.0)]);
    load(&mut m, 2, 2, &[spawn(0, 5, 99, 9, 0.0)]);
    let state = m.tree_inventory();
    assert_eq!(state[0].slots[0].1, 2);
    assert_eq!(state[0].slots[0].2, 1);
    assert_eq!(m.model_inventory()[0].references, 2);
    assert_eq!(m.model_inventory()[0].flags, 5);
    assert!(blocked(&m, false));
    assert!(!blocked(&m, true));
    m.unload_tile(0, 2, 2).unwrap();
    assert!(blocked(&m, false));
    assert!(!blocked(&m, true));
    assert_eq!(m.model_inventory()[0].references, 1);
}
#[test]
fn shared_m2_wmo_flag_order_is_last_acquisition_not_spawn_classification() {
    // WorldModel.cpp:428-431, MapTree.cpp:399-401.
    for reverse in [false, true] {
        let mut m = fixture(2, &[(1, 0.0)], false);
        let flags = if reverse { [5, 4] } else { [4, 5] };
        load(&mut m, 1, 2, &[spawn(0, flags[0], 1, 2, 0.0)]);
        load(&mut m, 2, 2, &[spawn(1, flags[1], 2, 3, 0.0)]);
        assert_eq!(blocked(&m, true), reverse);
    }
}
#[test]
fn final_reference_releases_model_and_reload_creates_new_generation() {
    // MapTree.cpp:472-485; VMapManager2.cpp:341-345.
    let mut m = fixture(1, &[(1, 0.0)], false);
    load(&mut m, 1, 2, &[spawn(0, 4, 1, 2, 0.0)]);
    load(&mut m, 2, 2, &[spawn(0, 4, 1, 2, 0.0)]);
    let old = m.model_inventory()[0].owner.clone();
    let selected = m
        .location(0, [0.0, 0.0, 10.0], deadline())
        .unwrap()
        .unwrap();
    m.unload_tile(0, 1, 2).unwrap();
    m.validate_model(&old).unwrap();
    assert!(blocked(&m, false));
    m.unload_tile(0, 2, 2).unwrap();
    assert_eq!(m.validate_model(&old), Err(Error::StaleGeneration));
    assert!(!m.has_tree(0));
    m.load_map(0, 1, 2).unwrap();
    assert_ne!(m.model_inventory()[0].owner.generation, old.generation);
    let mut out = LiquidOutput {
        level: 99.0,
        floor: 98.0,
        kind: 97,
    };
    assert_eq!(
        m.liquid_at(&selected, 0, &mut out),
        Err(Error::StaleGeneration)
    );
    assert_eq!(out.level, 99.0);
}
#[test]
fn stale_instance_cannot_query_model_retained_by_another_map() {
    let mut m = fixture(1, &[(1, 0.0)], false);
    present(&mut m, b"001.vmtree", tree(true, 1, &[]));
    present(&mut m, b"001_02_01.vmtile", tile(&[spawn(0, 4, 1, 2, 0.0)]));
    load(&mut m, 1, 2, &[spawn(0, 4, 1, 2, 0.0)]);
    m.load_map(1, 1, 2).unwrap();
    let selected = m
        .location(0, [0.0, 0.0, 10.0], deadline())
        .unwrap()
        .unwrap();
    m.unload_tile(0, 1, 2).unwrap();
    m.load_map(0, 1, 2).unwrap();
    m.validate_model(&selected.model).unwrap();
    assert_eq!(
        m.liquid_at(
            &selected,
            0,
            &mut LiquidOutput {
                level: 0.0,
                floor: 0.0,
                kind: 0
            }
        ),
        Err(Error::StaleGeneration)
    );
}
#[test]
fn positive_los_height_parallel_tiny_and_zero_rays_follow_source_branches() {
    // VMapManager2.cpp:187-202,237-253; MapTree.cpp:155-165,213-230.
    let mut m = fixture(1, &[(1, 0.0)], false);
    load(&mut m, 1, 2, &[spawn(0, 4, 1, 2, 0.0)]);
    assert!(blocked(&m, false));
    assert_eq!(
        m.height(0, [0.0, 0.0, 10.0], 20.0, deadline())
            .unwrap()
            .to_bits(),
        0
    );
    assert_eq!(
        m.height(0, [0.0, 0.0, -5.0], -20.0, deadline())
            .unwrap()
            .to_bits(),
        0
    );
    assert_eq!(
        m.height(0, [0.0, 0.0, -5.0], 20.0, deadline()).unwrap(),
        -200000.0
    );
    for z in [0.0, 1e-11] {
        assert!(m
            .line_of_sight(0, [0.0, 0.0, 5.0], [0.0, 0.0, 5.0 + z], false, deadline())
            .unwrap());
    }
    assert!(m
        .line_of_sight(0, [-1.0, 0.0, 5.0], [1.0, 0.0, 5.0], false, deadline())
        .unwrap());
    // Tiny representable segment at the origin in Z.
    assert!(m
        .line_of_sight(0, [0.0, 0.0, 0.0], [0.0, 0.0, 1e-11], false, deadline())
        .unwrap());
}
#[test]
fn equal_group_ties_keep_first_group_but_area_metadata_is_last_instance_hit() {
    // WorldModel.cpp:464; ModelInstance.cpp:91,126.
    let mut m = fixture(2, &[(7, 0.0), (8, 0.0)], false);
    load(
        &mut m,
        1,
        2,
        &[spawn(0, 4, 1, 2, 0.0), spawn(1, 4, 2, 3, 0.0)],
    );
    let area: AreaInfo = m
        .area_info(0, [0.0, 0.0, 10.0], deadline())
        .unwrap()
        .unwrap();
    assert_eq!((area.group, area.adt, area.flags), (7, 2, 107));
    let location: LocationInfo = m
        .location(0, [0.0, 0.0, 10.0], deadline())
        .unwrap()
        .unwrap();
    assert_eq!(location.slot, 0);
    assert_eq!(location.group, 7);
}
#[test]
fn liquid_floor_and_type_writes_precede_mask_and_level_success() {
    // VMapManager2.cpp:295-302; ModelInstance.cpp:150.
    let mut m = fixture(1, &[(1, 0.0)], true);
    load(&mut m, 1, 2, &[spawn(0, 4, 1, 2, 0.0)]);
    let mut out = LiquidOutput {
        level: 99.0,
        floor: 98.0,
        kind: 97,
    };
    assert!(!m
        .liquid_level(0, [0.0, 0.0, 10.0], 4, &mut out, deadline())
        .unwrap());
    // WorldModel.cpp:384-389: rounded 1/400 gives dist 0x41219999, then
    // dist-0.1 is 0x411fffff, so ModelInstance.cpp:121-125 returns 2^-20.
    assert_eq!(
        (out.level.to_bits(), out.floor.to_bits(), out.kind),
        (99.0f32.to_bits(), 0x35800000, 1)
    );
    assert!(m
        .liquid_level(0, [0.0, 0.0, 10.0], 0, &mut out, deadline())
        .unwrap());
    assert_eq!(out.level, 3.0);
}
#[test]
fn cold_disabled_ignored_error_and_uncaptured_are_not_conflated() {
    let mut m = Manager::new();
    assert!(m
        .line_of_sight(0, [f32::NAN; 3], [0.0; 3], false, deadline())
        .unwrap());
    assert_eq!(m.load_map(0, 1, 2), Err(Error::ProductUnavailable));
    m.files.observe(b"000.vmtree", FileState::Absent).unwrap();
    assert_eq!(m.load_map(0, 1, 2).unwrap(), LoadResult::Error);
    m.configure(false, false);
    assert_eq!(m.load_map(99, 1, 2).unwrap(), LoadResult::Ignored);
    assert_eq!(m.height(0, [0.0; 3], 10.0, deadline()).unwrap(), -200000.0);
    m.configure(true, true);
    m.initialize_thread_unsafe([0]);
    assert_eq!(m.load_map(99, 1, 2), Err(Error::InvalidMap));
    let mut malformed = Manager::new();
    present(&mut malformed, b"000.vmtree", vec![0; 20]);
    assert_eq!(malformed.load_map(0, 1, 2).unwrap(), LoadResult::Error);
}
#[test]
fn missing_model_is_source_null_not_native_unavailability() {
    let mut m = Manager::new();
    present(&mut m, b"000.vmtree", tree(true, 1, &[]));
    present(&mut m, b"000_02_01.vmtile", tile(&[spawn(0, 4, 1, 2, 0.0)]));
    assert_eq!(m.load_map(0, 1, 2), Err(Error::ProductUnavailable));
    m.files.observe(b"test.wmo.vmo", FileState::Absent).unwrap();
    assert_eq!(m.load_map(0, 1, 2).unwrap(), LoadResult::Ok);
    assert!(!blocked(&m, false));
    assert!(m.model_inventory().is_empty());
}
#[test]
fn source_coordinate_conversion_preserves_roundtrip_bits_not_ideal_coordinates() {
    assert_eq!(
        to_internal([0.0, 1.0, -0.0]).map(f32::to_bits),
        [0x46855555, 0x46855355, 0x80000000]
    );
    assert_eq!(
        to_internal(to_internal([1e-5, -1e-5, -0.0])).map(f32::to_bits),
        [0, 0, 0x80000000]
    );
}
#[test]
fn undefined_slot_boundary_and_unbound_rotations_refuse_at_the_actual_operation() {
    // MapTree.cpp:411 accepts equality and writes out of bounds at :417.
    for (slot, expected) in [
        (0, Ok(LoadResult::Ok)),
        (1, Err(Error::UndefinedSlot)),
        (2, Ok(LoadResult::Ok)),
    ] {
        let mut m = fixture(1, &[(1, 0.0)], false);
        present(
            &mut m,
            b"000_02_01.vmtile",
            tile(&[spawn(slot, 4, 1, 2, 0.0)]),
        );
        assert_eq!(m.load_map(0, 1, 2), expected);
    }
    for angle in [-0.0, 0.0, 1.0, 1800.0, 1801.0] {
        let mut m = fixture(1, &[(1, 0.0)], false);
        present(
            &mut m,
            b"000_02_01.vmtile",
            tile(&[spawn(0, 4, 1, 2, angle)]),
        );
        let result = m.load_map(0, 1, 2);
        if angle == 0.0 {
            assert_eq!(result, Ok(LoadResult::Ok));
        } else if angle > 1800.0 {
            assert_eq!(result, Err(Error::RotationDomain));
        } else {
            assert_eq!(result, Err(Error::NumericBinding));
        }
    }
}
#[test]
fn immutable_source_cannot_be_replaced_after_capture() {
    let mut files = ProductFiles::default();
    files.observe(b"x", FileState::Absent).unwrap();
    assert_eq!(
        files.observe(b"x", FileState::Present(Arc::from([1u8]))),
        Err(Error::ImmutableSource)
    );
}
#[test]
fn source_reference_read_refuses_only_after_acquiring_the_model() {
    // MapTree.cpp:399-408.
    let mut m = fixture(1, &[(1, 0.0)], false);
    let mut bytes = tile(&[spawn(0, 4, 1, 2, 0.0)]);
    bytes.truncate(bytes.len() - 4);
    present(&mut m, b"000_02_01.vmtile", bytes);
    assert_eq!(m.load_map(0, 1, 2), Err(Error::UndefinedRead));
    assert_eq!(m.model_inventory()[0].references, 1);
}
#[test]
fn source_missing_bounds_refuse_only_when_the_instance_is_queried() {
    // G3D AABox.h:42-43; ModelInstance.cpp:44 reads those undefined fields.
    let mut m = fixture(1, &[(1, 0.0)], false);
    load(&mut m, 1, 2, &[spawn(0, 0, 1, 2, 0.0)]);
    assert_eq!(
        m.line_of_sight(0, [0.0, 0.0, 10.0], [0.0, 0.0, -5.0], false, deadline()),
        Err(Error::UndefinedRead)
    );
    m.unload_map(0);
    assert!(m.model_inventory().is_empty());
}
#[test]
fn scaled_translated_instance_and_hit_position_keep_source_operation_order() {
    // ModelInstance.cpp:53-60; MapTree.cpp:187-205.
    let mut m = fixture(1, &[(1, 2.0)], false);
    let mut s = spawn(0, 4, 1, 2, 0.0);
    s[18..22].copy_from_slice(&3.0f32.to_le_bytes());
    s[34..38].copy_from_slice(&2.0f32.to_le_bytes());
    load(&mut m, 1, 2, &[s]);
    assert_eq!(
        m.height(0, [0.0, 0.0, 10.0], 20.0, deadline())
            .unwrap()
            .to_bits(),
        7.0f32.to_bits()
    );
    for (modify, z) in [(0.0f32, 7.0f32), (1.0, 6.0), (-1.0, 8.0), (-20.0, 10.0)] {
        let (hit, p) = m
            .hit_position(0, [0.0, 0.0, 10.0], [0.0, 0.0, -5.0], modify, deadline())
            .unwrap();
        assert!(hit);
        assert_eq!(p.map(f32::to_bits), [0, 0, z.to_bits()]);
    }
}
#[test]
fn unloaded_slots_retain_instance_storage_until_tree_destruction() {
    // ModelInstance.h:69 only clears the model pointer, not spawn data.
    let mut m = fixture(1, &[(1, 0.0)], false);
    load(&mut m, 1, 2, &[spawn(0, 4, 1, 2, 0.0)]);
    m.files
        .observe(b"000_03_01.vmtile", FileState::Absent)
        .unwrap();
    m.load_map(0, 1, 3).unwrap();
    m.unload_tile(0, 1, 2).unwrap();
    assert!(!blocked(&m, false));
    assert_eq!(m.tree_inventory()[0].slots[0], (0, 0, 1, None));
    assert!(m.model_inventory().is_empty());
    m.load_map(0, 1, 2).unwrap();
    assert!(blocked(&m, false));
}
#[test]
fn global_last_logical_tile_preserves_source_model_reference_leak() {
    // VMapManager2.cpp:176-180 deletes StaticMapTree without UnloadMap;
    // MapTree.cpp:133-136 deletes only the instance array.
    let mut m = Manager::new();
    present(
        &mut m,
        b"000.vmtree",
        tree(false, 1, &[spawn(0, 6, 1, 2, 0.0)]),
    );
    present(&mut m, b"test.wmo.vmo", model(&[(1, 0.0)], false));
    m.load_map(0, 1, 2).unwrap();
    let old = m.model_inventory()[0].owner.clone();
    m.unload_tile(0, 1, 2).unwrap();
    assert!(!m.has_tree(0));
    assert_eq!(m.model_inventory()[0].references, 1);
    m.load_map(0, 1, 2).unwrap();
    assert_eq!(m.model_inventory()[0].references, 2);
    assert_eq!(m.model_inventory()[0].owner, old);
    m.unload_map(0);
    assert_eq!(m.model_inventory()[0].references, 1);
}
#[test]
fn native_model_validation_failure_never_becomes_a_source_null_model() {
    let mut m = Manager::new();
    present(&mut m, b"000.vmtree", tree(true, 1, &[]));
    present(&mut m, b"000_02_01.vmtile", tile(&[spawn(0, 4, 1, 2, 0.0)]));
    let mut bytes = model(&[(1, 0.0)], false);
    let mesh = bytes.windows(4).position(|v| v == b"MBIH").unwrap();
    bytes[mesh + 32..mesh + 36].copy_from_slice(&((3u32 << 30) | 7).to_le_bytes());
    present(&mut m, b"test.wmo.vmo", bytes);
    assert_eq!(m.load_map(0, 1, 2), Err(Error::Unsupported));
    assert!(m.tree_inventory()[0].slots.is_empty());
}
#[test]
fn liquid_float_to_unsigned_refuses_undefined_conversion_before_negative_check() {
    // WorldModel.cpp:158-161 casts before checking tx_f < 0.
    let mut m = fixture(1, &[(1, 0.0)], true);
    load(&mut m, 1, 2, &[spawn(0, 4, 1, 2, 0.0)]);
    let mut out = LiquidOutput {
        level: 99.0,
        floor: 98.0,
        kind: 97,
    };
    assert!(!m
        .liquid_level(0, [5.0, 2.0, 10.0], 0, &mut out, deadline())
        .unwrap());
    assert_eq!(
        m.liquid_level(0, [5.25, 2.0, 10.0], 0, &mut out, deadline()),
        Err(Error::UndefinedRead)
    );
}
#[test]
fn source_model_read_failure_and_ignored_gmod_chunk_are_distinct() {
    // WorldModel.cpp:558-578 returns false on the main header, but ignores GMOD failure.
    for empty_model in [false, true] {
        let mut m = Manager::new();
        present(&mut m, b"000.vmtree", tree(true, 1, &[]));
        present(&mut m, b"000_02_01.vmtile", tile(&[spawn(0, 4, 1, 2, 0.0)]));
        let bytes = if empty_model {
            let mut b = b"VMAP_7.0WMOD".to_vec();
            word(&mut b, 8);
            word(&mut b, 17);
            b.extend(b"JUNK");
            b
        } else {
            vec![0; 20]
        };
        present(&mut m, b"test.wmo.vmo", bytes);
        assert_eq!(m.load_map(0, 1, 2).unwrap(), LoadResult::Ok);
        assert!(!blocked(&m, false));
        assert_eq!(m.model_inventory().len(), usize::from(empty_model));
    }
}
#[test]
fn equal_counter_values_in_another_manager_do_not_reconstitute_a_model_handle() {
    let mut first = fixture(1, &[(1, 0.0)], true);
    let mut second = fixture(1, &[(1, 0.0)], true);
    load(&mut first, 1, 2, &[spawn(0, 4, 1, 2, 0.0)]);
    load(&mut second, 1, 2, &[spawn(0, 4, 1, 2, 0.0)]);
    let selected = first
        .location(0, [0.0, 0.0, 10.0], deadline())
        .unwrap()
        .unwrap();
    first.validate_model(&selected.model).unwrap();
    assert_eq!(
        first.model_inventory()[0].owner.generation,
        second.model_inventory()[0].owner.generation
    );
    assert_eq!(
        second.validate_model(&selected.model),
        Err(Error::StaleGeneration)
    );
    let mut output = LiquidOutput {
        level: 99.0,
        floor: 98.0,
        kind: 97,
    };
    assert_eq!(
        second.liquid_at(&selected, 0, &mut output),
        Err(Error::StaleGeneration)
    );
    assert_eq!((output.level, output.floor, output.kind), (99.0, 98.0, 97));
}
