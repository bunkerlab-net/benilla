// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored model/list operands; no captured geometric answers.
use super::tests::{deadline, model, present};
use super::*;
fn row(bytes: &mut Vec<u8>, display: u32, name: &[u8], bounds: [[f32; 3]; 2]) {
    bytes.extend(display.to_le_bytes());
    bytes.extend((name.len() as u32).to_le_bytes());
    bytes.extend(name);
    for f in bounds.into_iter().flatten() {
        bytes.extend(f.to_le_bytes());
    }
}
fn fixture(bounds: [[f32; 3]; 2]) -> (Manager, GameObjectModelList, DynamicModels) {
    let mut manager = Manager::new();
    let mut bytes = Vec::new();
    row(&mut bytes, 7, b"test.wmo", bounds);
    present(&mut manager, b"temp_gameobject_models", bytes);
    present(&mut manager, b"test.wmo.vmo", model(&[(1, 0.0)], false));
    let mut list = GameObjectModelList::default();
    list.load(&manager.files).unwrap();
    (manager, list, DynamicModels::default())
}
pub(super) fn ordinary() -> (Manager, GameObjectModelList, DynamicModels) {
    fixture([[-10.0; 3], [10.0; 3]])
}
fn construct(
    manager: &mut Manager,
    list: &GameObjectModelList,
    models: &mut DynamicModels,
    pointer: u64,
    position: Vector,
) -> DynamicModelHandle {
    models
        .construct(
            manager,
            list,
            Some(7),
            DynamicModelPose {
                position,
                orientation: 0.0,
                scale: 1.0,
            },
            Some(pointer),
        )
        .unwrap()
        .unwrap()
}
#[test]
fn dynamic_null_display_missing_list_zero_bounds_and_model_null_are_source_branches() {
    let (mut manager, list, mut models) = ordinary();
    assert_eq!(
        models
            .construct(
                &mut manager,
                &list,
                None,
                DynamicModelPose {
                    position: [0.0; 3],
                    orientation: 0.0,
                    scale: 1.0
                },
                None
            )
            .unwrap(),
        None
    );
    assert_eq!(
        models
            .construct(
                &mut manager,
                &list,
                Some(99),
                DynamicModelPose {
                    position: [0.0; 3],
                    orientation: 0.0,
                    scale: 1.0
                },
                Some(10)
            )
            .unwrap(),
        None
    );
    assert!(models
        .construct(
            &mut manager,
            &list,
            Some(7),
            DynamicModelPose {
                position: [0.0; 3],
                orientation: 0.0,
                scale: 1.0
            },
            None
        )
        .unwrap()
        .is_some());
    let (mut manager, list, mut models) = fixture([[0.0; 3]; 2]);
    assert_eq!(
        models
            .construct(
                &mut manager,
                &list,
                Some(7),
                DynamicModelPose {
                    position: [0.0; 3],
                    orientation: 0.0,
                    scale: 1.0
                },
                Some(10)
            )
            .unwrap(),
        None
    );
    let (mut manager, list, mut models) = ordinary();
    manager.files = ProductFiles::default();
    manager
        .files
        .observe(b"test.wmo.vmo", FileState::Absent)
        .unwrap();
    assert_eq!(
        models
            .construct(
                &mut manager,
                &list,
                Some(7),
                DynamicModelPose {
                    position: [0.0; 3],
                    orientation: 0.0,
                    scale: 1.0
                },
                Some(10)
            )
            .unwrap(),
        None
    );
    manager.files = ProductFiles::default();
    assert_eq!(
        models
            .construct(
                &mut manager,
                &list,
                Some(7),
                DynamicModelPose {
                    position: [0.0; 3],
                    orientation: 0.0,
                    scale: 1.0
                },
                Some(10)
            )
            .unwrap_err(),
        DynamicError::Vmap(Error::ProductUnavailable)
    );
}
#[test]
fn dynamic_model_list_first_wins_and_empty_truncated_reads_refuse() {
    let mut bytes = Vec::new();
    row(&mut bytes, 7, b"one", [[-1.0; 3], [1.0; 3]]);
    row(&mut bytes, 7, b"two", [[-2.0; 3], [2.0; 3]]);
    let mut files = ProductFiles::default();
    files
        .observe(
            b"temp_gameobject_models",
            FileState::Present(bytes.clone().into()),
        )
        .unwrap();
    let mut list = GameObjectModelList::default();
    list.load(&files).unwrap();
    assert_eq!(list.bounds(7), Some([[-1.0; 3], [1.0; 3]]));
    for bytes in [Vec::new(), bytes[..3].to_vec(), bytes[..10].to_vec()] {
        let mut files = ProductFiles::default();
        files
            .observe(b"temp_gameobject_models", FileState::Present(bytes.into()))
            .unwrap();
        assert_eq!(
            GameObjectModelList::default().load(&files),
            Err(DynamicError::ModelListReadUndefined)
        );
    }
    let mut files = ProductFiles::default();
    files
        .observe(
            b"temp_gameobject_models",
            FileState::Present(bytes[..23].to_vec().into()),
        )
        .unwrap();
    let mut partial = GameObjectModelList::default();
    partial.load(&files).unwrap();
    assert_eq!(partial.bounds(7), Some([[-1.0; 3], [0.0; 3]]));
    let mut files = ProductFiles::default();
    files
        .observe(b"temp_gameobject_models", FileState::Absent)
        .unwrap();
    assert!(GameObjectModelList::default().load(&files).is_ok());
    let mut bytes = 7u32.to_le_bytes().to_vec();
    bytes.extend(500u32.to_le_bytes());
    let mut files = ProductFiles::default();
    files
        .observe(b"temp_gameobject_models", FileState::Present(bytes.into()))
        .unwrap();
    assert!(GameObjectModelList::default().load(&files).is_ok());
}
#[test]
fn dynamic_door_blocks_until_disabled_and_shared_model_releases() {
    let (mut manager, list, mut models) = ordinary();
    let handle = construct(&mut manager, &list, &mut models, 10, [0.0; 3]);
    let mut tree = DynamicMapTree::new();
    tree.insert(&models, &handle).unwrap();
    assert!(!tree
        .line_of_sight(
            &models,
            &manager,
            [0.0, 0.0, 10.0],
            [0.0, 0.0, -5.0],
            false,
            deadline()
        )
        .unwrap());
    // DynamicTree.cpp:267-276 has no static-tree 0.1f ray-origin offset.
    assert_eq!(
        tree.height(&models, &manager, [0.0, 0.0, 10.0], 15.0, deadline())
            .unwrap()
            .to_bits(),
        0
    );
    models.enable(&handle, false).unwrap();
    assert!(tree
        .line_of_sight(
            &models,
            &manager,
            [0.0, 0.0, 10.0],
            [0.0, 0.0, -5.0],
            false,
            deadline()
        )
        .unwrap());
    tree.remove(&handle).unwrap();
    models.release(&mut manager, &handle).unwrap();
    assert!(manager.model_inventory().is_empty());
    assert_eq!(models.enable(&handle, true), Err(DynamicError::StaleModel));
}
#[test]
fn dynamic_transport_scale_inverse_and_relocation_retain_original_scale() {
    let (mut manager, list, mut models) = ordinary();
    let handle = models
        .construct(
            &mut manager,
            &list,
            Some(7),
            DynamicModelPose {
                position: [0.0, 0.0, 6.0],
                orientation: 0.0,
                scale: 2.0,
            },
            Some(10),
        )
        .unwrap()
        .unwrap();
    assert_eq!(models.inventory()[0].scale.to_bits(), 2f32.to_bits());
    assert_eq!(
        models.inventory()[0].inverse_scale.to_bits(),
        0.5f32.to_bits()
    );
    let mut tree = DynamicMapTree::new();
    tree.insert(&models, &handle).unwrap();
    assert!(!tree
        .line_of_sight(
            &models,
            &manager,
            [0.0, 0.0, 10.0],
            [0.0, 0.0, 2.0],
            false,
            deadline()
        )
        .unwrap());
    tree.remove(&handle).unwrap();
    assert!(models
        .relocate(&manager, &list, &handle, 7, [0.0, 0.0, 1.0], 0.0)
        .unwrap());
    tree.insert(&models, &handle).unwrap();
    assert!(tree
        .line_of_sight(
            &models,
            &manager,
            [0.0, 0.0, 10.0],
            [0.0, 0.0, 2.0],
            false,
            deadline()
        )
        .unwrap());
    assert_eq!(
        models.inventory()[0].bounds,
        [[-20.0, -20.0, -19.0], [20.0, 20.0, 21.0]]
    );
    assert!(!models
        .relocate(&manager, &list, &handle, 99, [0.0; 3], 0.0)
        .unwrap());
    assert_eq!(models.inventory()[0].position, [0.0, 0.0, 1.0]);
}
#[test]
fn dynamic_nonzero_rotation_requires_bound_dispatch_and_zero_scale_is_ieee() {
    let (mut manager, list, mut models) = ordinary();
    assert_eq!(
        models
            .construct(
                &mut manager,
                &list,
                Some(7),
                DynamicModelPose {
                    position: [3.0, 4.0, 5.0],
                    orientation: 1.0,
                    scale: 2.0
                },
                Some(10)
            )
            .unwrap_err(),
        DynamicError::Vmap(Error::NumericBinding)
    );
    // GameObjectModel.cpp:91-103 retains acquisition and these writes at rotation.
    assert_eq!(manager.model_inventory()[0].references, 1);
    let partial = &models.construction_inventory()[0];
    assert_eq!(partial.shared.name.as_ref(), b"test.wmo");
    assert_eq!(partial.display, 7);
    assert_eq!(partial.model_bounds, list.bounds(7).unwrap());
    assert_eq!(partial.position, [3.0, 4.0, 5.0]);
    assert_eq!(partial.scale.to_bits(), 2f32.to_bits());
    assert_eq!(partial.inverse_scale.to_bits(), 0.5f32.to_bits());
    assert!(partial.enabled);
    assert!(models.inventory().is_empty());
    let (mut manager, list, mut models) = ordinary();
    let handle = models
        .construct(
            &mut manager,
            &list,
            Some(7),
            DynamicModelPose {
                position: [0.0; 3],
                orientation: -0.0,
                scale: 0.0,
            },
            Some(10),
        )
        .unwrap()
        .unwrap();
    assert_eq!(models.inventory()[0].inverse_scale, f32::INFINITY);
    models.release(&mut manager, &handle).unwrap();
}
#[test]
fn dynamic_cell_spanning_duplicates_count_memberships_not_models() {
    let (mut manager, list, mut models) = fixture([[-600.0, -600.0, -10.0], [600.0, 600.0, 10.0]]);
    let handle = construct(&mut manager, &list, &mut models, 10, [0.0; 3]);
    let mut tree = DynamicMapTree::new();
    tree.insert(&models, &handle).unwrap();
    assert_eq!(tree.size(), 16);
    tree.insert(&models, &handle).unwrap();
    assert_eq!(tree.size(), 32);
    assert_eq!(tree.inventory().len(), 16);
    assert!(tree
        .inventory()
        .iter()
        .all(|n| n.pending == [handle.clone()]));
    tree.remove(&handle).unwrap();
    assert_eq!(tree.size(), 0);
    tree.balance(&models).unwrap();
    assert!(tree.inventory().iter().all(|n| n.objects.is_empty()));
}
#[test]
fn dynamic_remove_before_first_balance_and_query_balance_do_not_reset_map_timer() {
    let (mut manager, list, mut models) = ordinary();
    let handle = construct(&mut manager, &list, &mut models, 10, [0.0; 3]);
    let mut tree = DynamicMapTree::new();
    tree.update(&models, u32::MAX).unwrap();
    assert_eq!(tree.timer_ms(), 200);
    tree.insert(&models, &handle).unwrap();
    tree.remove(&handle).unwrap();
    assert!(tree
        .line_of_sight(
            &models,
            &manager,
            [0.0, 0.0, 10.0],
            [0.0, 0.0, -5.0],
            false,
            deadline()
        )
        .unwrap());
    assert_eq!(tree.unbalanced(), 2);
    assert_eq!(tree.timer_ms(), 200);
    tree.insert(&models, &handle).unwrap();
    tree.update(&models, 199).unwrap();
    assert_eq!(tree.timer_ms(), 1);
    assert_eq!(tree.unbalanced(), 3);
    tree.height(&models, &manager, [0.0, 0.0, 10.0], 15.0, deadline())
        .unwrap();
    assert_eq!(tree.unbalanced(), 3);
    // RegularGrid.h:162-173,250-256 balances only the visited BIHWrap.
    let nodes = tree.inventory();
    assert_eq!(
        nodes
            .iter()
            .find(|n| n.cell == [32, 32])
            .unwrap()
            .unbalanced,
        0
    );
    assert_eq!(nodes.iter().filter(|n| n.unbalanced != 0).count(), 3);
    tree.update(&models, 1).unwrap();
    assert_eq!(tree.timer_ms(), 200);
    assert_eq!(tree.unbalanced(), 0);
    tree.remove(&handle).unwrap();
    tree.update(&models, 200).unwrap();
    assert_eq!(tree.timer_ms(), 200);
}
#[test]
fn dynamic_g3d_pointer_bucket_order_resize_and_reuse_are_not_guid_order() {
    let (mut manager, list, mut models) = fixture([[1.0; 3], [10.0; 3]]);
    let mut tree = DynamicMapTree::new();
    let handles: Vec<_> = [10, 20, 30, 40]
        .into_iter()
        .map(|p| {
            let h = construct(&mut manager, &list, &mut models, p, [0.0; 3]);
            tree.insert(&models, &h).unwrap();
            h
        })
        .collect();
    tree.balance(&models).unwrap();
    let n = &tree.inventory()[0];
    assert_eq!(n.buckets, Some(31));
    assert_eq!(
        n.objects
            .iter()
            .map(|h| h.pointer().unwrap())
            .collect::<Vec<_>>(),
        [40, 10, 20, 30]
    );
    // BIHWrap.h:94-95 and G3D Set.h:97-99/Table.h:872-881 replace, not append.
    tree.remove(&handles[1]).unwrap();
    tree.balance(&models).unwrap();
    assert_eq!(
        tree.inventory()[0]
            .objects
            .iter()
            .map(|h| h.pointer().unwrap())
            .collect::<Vec<_>>(),
        [40, 10, 30]
    );
    models.release(&mut manager, &handles[1]).unwrap();
    let reused = construct(&mut manager, &list, &mut models, 20, [0.0; 3]);
    assert_ne!(reused, handles[1]);
    tree.insert(&models, &reused).unwrap();
    tree.balance(&models).unwrap();
    assert_eq!(
        models.enable(&handles[1], false),
        Err(DynamicError::StaleModel)
    );
}
#[test]
fn dynamic_grid_cross_cell_hit_then_miss_overwrites_did_hit() {
    let (mut manager, list, mut models) = fixture([[-600.0, -10.0, -10.0], [600.0, 10.0, 10.0]]);
    let handle = construct(&mut manager, &list, &mut models, 10, [0.0; 3]);
    let mut tree = DynamicMapTree::new();
    tree.insert(&models, &handle).unwrap();
    // The first cell hits the plane. A duplicate in a later cell sees strict t < MaxDist
    // fail, assigning false to did_hit, DynamicTree.cpp:147 and RegularGrid.h:214-235.
    assert!(tree
        .line_of_sight(
            &models,
            &manager,
            [-500.0, 0.0, 10.0],
            [500.0, 0.0, -10.0],
            false,
            deadline()
        )
        .unwrap());
    assert_eq!(
        tree.hit_position(
            &models,
            &manager,
            [-500.0, 0.0, 10.0],
            [500.0, 0.0, -10.0],
            0.0,
            deadline()
        )
        .unwrap(),
        (false, [500.0, 0.0, -10.0])
    );
}
#[test]
fn dynamic_near_far_stop_is_bucket_order_and_parallel_walk_is_source_walk() {
    let (mut manager, list, mut models) = fixture([[1.0, 1.0, -10.0], [10.0, 10.0, 10.0]]);
    let mut tree = DynamicMapTree::new();
    for (pointer, z) in [(10, 0.0), (20, 5.0)] {
        let h = construct(&mut manager, &list, &mut models, pointer, [0.0, 0.0, z]);
        tree.insert(&models, &h).unwrap();
    }
    // Same bucket: second insertion is first. BIHWrap always stops at its first hit.
    assert_eq!(
        tree.height(&models, &manager, [2.0, 2.0, 10.0], 20.0, deadline())
            .unwrap(),
        5.0
    );
    assert!(tree
        .line_of_sight(
            &models,
            &manager,
            [600.0, 0.0, 10.0],
            [-600.0, 0.0, 10.0],
            false,
            deadline()
        )
        .unwrap());
    assert!(tree
        .line_of_sight(
            &models,
            &manager,
            [0.0; 3],
            [0.000001, 0.0, 0.0],
            false,
            deadline()
        )
        .unwrap());
}
#[test]
fn dynamic_cell_undefined_cast_and_index_boundaries_refuse() {
    let (mut manager, list, mut models) = fixture([[1.0; 3], [2.0; 3]]);
    for (pointer, x, expected) in [
        (10, f32::INFINITY, DynamicError::CellConversionUndefined),
        (20, -20000.0, DynamicError::CellIndexUndefined),
    ] {
        let h = construct(&mut manager, &list, &mut models, pointer, [x, 0.0, 0.0]);
        assert_eq!(DynamicMapTree::new().insert(&models, &h), Err(expected));
    }
    let h = construct(&mut manager, &list, &mut models, 30, [16000.0, 0.0, 0.0]);
    assert!(DynamicMapTree::new().insert(&models, &h).is_ok());
}

fn wide_fixture() -> (Manager, GameObjectModelList, DynamicModels) {
    fn word(b: &mut Vec<u8>, w: u32) {
        b.extend(w.to_le_bytes());
    }
    fn vector(b: &mut Vec<u8>, p: Vector) {
        for f in p {
            word(b, f.to_bits());
        }
    }
    fn bounds(b: &mut Vec<u8>) {
        vector(b, [-1000.0; 3]);
        vector(b, [1000.0; 3]);
    }
    fn bih(b: &mut Vec<u8>) {
        bounds(b);
        for w in [3, 3 << 30, 1, 0, 1, 0] {
            word(b, w);
        }
    }
    let mut bytes = b"VMAP_7.0WMOD".to_vec();
    word(&mut bytes, 8);
    word(&mut bytes, 17);
    bytes.extend(b"GMOD");
    word(&mut bytes, 1);
    bounds(&mut bytes);
    word(&mut bytes, 101);
    word(&mut bytes, 1);
    bytes.extend(b"VERT");
    word(&mut bytes, 40);
    word(&mut bytes, 3);
    for p in [
        [-1000.0, -1000.0, 0.0],
        [1000.0, -1000.0, 0.0],
        [0.0, 1000.0, 0.0],
    ] {
        vector(&mut bytes, p);
    }
    bytes.extend(b"TRIM");
    for w in [16, 1, 0, 1, 2] {
        word(&mut bytes, w);
    }
    bytes.extend(b"MBIH");
    bih(&mut bytes);
    bytes.extend(b"LIQU");
    word(&mut bytes, 0);
    bytes.extend(b"GBIH");
    bih(&mut bytes);
    let mut manager = Manager::new();
    present(&mut manager, b"test.wmo.vmo", bytes);
    let mut bytes = Vec::new();
    row(&mut bytes, 7, b"test.wmo", [[-1000.0; 3], [1000.0; 3]]);
    present(&mut manager, b"temp_gameobject_models", bytes);
    let mut list = GameObjectModelList::default();
    list.load(&manager.files).unwrap();
    (manager, list, DynamicModels::default())
}

#[test]
fn dynamic_all_three_query_results_can_depend_on_source_pointer_bucket_order() {
    let (mut manager, list, mut models) = wide_fixture();
    let handles = [
        construct(&mut manager, &list, &mut models, 10, [0.0, 0.0, 5.0]),
        construct(&mut manager, &list, &mut models, 20, [0.0; 3]),
    ];
    let mut heights = Vec::new();
    let mut hits = Vec::new();
    let mut los = Vec::new();
    for order in [[0, 1], [1, 0]] {
        let mut tree = DynamicMapTree::new();
        for index in order {
            tree.insert(&models, &handles[index]).unwrap();
        }
        heights.push(
            tree.height(&models, &manager, [0.0, 0.0, 10.0], 20.0, deadline())
                .unwrap()
                .to_bits(),
        );
        hits.push(
            tree.hit_position(
                &models,
                &manager,
                [0.0, 0.0, 10.0],
                [0.0, 0.0, -5.0],
                0.0,
                deadline(),
            )
            .unwrap()
            .1[2]
                .to_bits(),
        );
        // BIHWrap.h:105 stops at the first hit in EACH cell. DynamicTree.cpp:147
        // overwrites did_hit in the next cell using the already reduced distance.
        los.push(
            tree.line_of_sight(
                &models,
                &manager,
                [-500.0, 0.0, 10.0],
                [500.0, 0.0, -10.0],
                false,
                deadline(),
            )
            .unwrap(),
        );
    }
    assert_eq!(heights, [0, 5f32.to_bits()]);
    assert_eq!(hits, [0, 5f32.to_bits()]);
    assert_eq!(los, [false, true]);
}

#[test]
fn dynamic_unbound_singleton_cells_and_cross_cell_overwrite_ignore_insert_order() {
    let (mut manager, list, mut models) = ordinary();
    let handles: Vec<_> = [-1000.0, 0.0, 1000.0]
        .into_iter()
        .map(|x| {
            models
                .construct(
                    &mut manager,
                    &list,
                    Some(7),
                    DynamicModelPose {
                        position: [x, 0.0, 0.0],
                        orientation: 0.0,
                        scale: 1.0,
                    },
                    None,
                )
                .unwrap()
                .unwrap()
        })
        .collect();
    let mut answers = Vec::new();
    for order in [[0, 1, 2], [2, 1, 0], [1, 0, 2]] {
        let mut tree = DynamicMapTree::new();
        for index in order {
            tree.insert(&models, &handles[index]).unwrap();
        }
        tree.update(&models, 200).unwrap();
        assert!(tree
            .inventory()
            .iter()
            .all(|n| n.pending.len() == 1 && !n.pointer_order_bound));
        let mut bits = Vec::new();
        for x in [-1000.0, 0.0, 1000.0] {
            bits.push(
                tree.height(&models, &manager, [x, 0.0, 10.0], 20.0, deadline())
                    .unwrap()
                    .to_bits(),
            );
            let (hit, p) = tree
                .hit_position(
                    &models,
                    &manager,
                    [x, 0.0, 10.0],
                    [x, 0.0, -5.0],
                    0.0,
                    deadline(),
                )
                .unwrap();
            assert!(hit);
            bits.push(p[2].to_bits());
        }
        // RegularGrid.h:162-235 fixes cell order independently of allocation.
        assert!(tree
            .line_of_sight(
                &models,
                &manager,
                [-1500.0, 0.0, 10.0],
                [1500.0, 0.0, -10.0],
                false,
                deadline()
            )
            .unwrap());
        assert_eq!(bits, [0; 6]);
        answers.push(bits);
    }
    assert!(answers.windows(2).all(|pair| pair[0] == pair[1]));
}

#[test]
fn dynamic_unbound_exact_ties_have_bit_identical_answers_in_every_permutation() {
    let (mut manager, list, mut models) = ordinary();
    let a = models
        .construct(
            &mut manager,
            &list,
            Some(7),
            DynamicModelPose {
                position: [0.0; 3],
                orientation: 0.0,
                scale: 1.0,
            },
            None,
        )
        .unwrap()
        .unwrap();
    let b = models
        .construct(
            &mut manager,
            &list,
            Some(7),
            DynamicModelPose {
                position: [0.0; 3],
                orientation: 0.0,
                scale: 1.0,
            },
            None,
        )
        .unwrap()
        .unwrap();
    let c = models
        .construct(
            &mut manager,
            &list,
            Some(7),
            DynamicModelPose {
                position: [0.0; 3],
                orientation: 0.0,
                scale: 1.0,
            },
            None,
        )
        .unwrap()
        .unwrap();
    let d = models
        .construct(
            &mut manager,
            &list,
            Some(7),
            DynamicModelPose {
                position: [0.0; 3],
                orientation: 0.0,
                scale: 1.0,
            },
            None,
        )
        .unwrap()
        .unwrap();
    let mut answers = Vec::new();
    for order in [
        [&a, &b, &c, &d],
        [&d, &c, &b, &a],
        [&c, &a, &d, &b],
        [&a, &a, &a, &a],
    ] {
        let mut tree = DynamicMapTree::new();
        for h in order {
            tree.insert(&models, h).unwrap();
        }
        let height = tree
            .height(&models, &manager, [0.0, 0.0, 10.0], 20.0, deadline())
            .unwrap()
            .to_bits();
        let hit = tree
            .hit_position(
                &models,
                &manager,
                [0.0, 0.0, 10.0],
                [0.0, 0.0, -5.0],
                0.0,
                deadline(),
            )
            .unwrap();
        let los = tree
            .line_of_sight(
                &models,
                &manager,
                [0.0, 0.0, 10.0],
                [0.0, 0.0, -5.0],
                false,
                deadline(),
            )
            .unwrap();
        assert_eq!(
            (height, hit.0, hit.1[2].to_bits(), los),
            (0, true, 0, false)
        );
        answers.push((height, hit.1.map(f32::to_bits), los));
    }
    assert!(answers.windows(2).all(|pair| pair[0] == pair[1]));
}

#[test]
fn dynamic_unbound_distinct_callbacks_refuse_at_query_after_actual_balance() {
    let (mut manager, list, mut models) = ordinary();
    let mut tree = DynamicMapTree::new();
    for z in [0.0, 5.0] {
        let h = models
            .construct(
                &mut manager,
                &list,
                Some(7),
                DynamicModelPose {
                    position: [0.0, 0.0, z],
                    orientation: 0.0,
                    scale: 1.0,
                },
                None,
            )
            .unwrap()
            .unwrap();
        tree.insert(&models, &h).unwrap();
    }
    assert_eq!(
        tree.height(&models, &manager, [0.0, 0.0, 10.0], 20.0, deadline()),
        Err(DynamicError::ModelAllocationUnbound)
    );
    let node = tree
        .inventory()
        .into_iter()
        .find(|n| n.cell == [32, 32])
        .unwrap();
    assert_eq!(node.unbalanced, 0);
    assert_eq!(node.objects.len(), 2);
    assert_eq!(
        tree.line_of_sight(
            &models,
            &manager,
            [0.0, 0.0, 10.0],
            [0.0, 0.0, -5.0],
            false,
            deadline()
        ),
        Err(DynamicError::ModelAllocationUnbound)
    );
    assert_eq!(
        tree.hit_position(
            &models,
            &manager,
            [0.0, 0.0, 10.0],
            [0.0, 0.0, -5.0],
            0.0,
            deadline()
        ),
        Err(DynamicError::ModelAllocationUnbound)
    );
    tree.update(&models, 200).unwrap();
    assert_eq!(tree.unbalanced(), 0);
    assert_eq!(tree.timer_ms(), 200);
}

#[test]
fn dynamic_bound_radian_rotation_uses_the_shared_fma_dispatch() {
    let (mut manager, list, mut models) = ordinary();
    manager.bind_rotation_dispatch(super::tests::rotation_binding());
    let h = models
        .construct(
            &mut manager,
            &list,
            Some(7),
            DynamicModelPose {
                position: [0.0; 3],
                orientation: 1.0,
                scale: 1.0,
            },
            None,
        )
        .unwrap()
        .unwrap();
    let mut tree = DynamicMapTree::new();
    tree.insert(&models, &h).unwrap();
    assert!(!tree
        .line_of_sight(
            &models,
            &manager,
            [0.0, 0.0, 10.0],
            [0.0, 0.0, -5.0],
            false,
            deadline()
        )
        .unwrap());
    // G3D's cofactor determinant is 3f7fffff, not exactly one. Matrix3.cpp:399-434.
    assert_eq!(models.inventory()[0].inverse[0][0].to_bits(), 0x3f0a5141);
    assert_eq!(models.inventory()[0].inverse[0][1].to_bits(), 0x3f576aa6);
}

#[test]
fn dynamic_resource_acquisition_retains_existing_static_world_model_flags() {
    let (mut manager, list, mut models) = ordinary();
    let name: Arc<[u8]> = Arc::from(b"test.wmo".as_slice());
    let shared = manager.acquire(&name, 1).unwrap().unwrap();
    let h = construct(&mut manager, &list, &mut models, 10, [0.0; 3]);
    assert_eq!(manager.model_inventory()[0].references, 2);
    assert_eq!(manager.model_inventory()[0].flags, 1);
    let mut tree = DynamicMapTree::new();
    tree.insert(&models, &h).unwrap();
    assert!(tree
        .line_of_sight(
            &models,
            &manager,
            [0.0, 0.0, 10.0],
            [0.0, 0.0, -5.0],
            true,
            deadline()
        )
        .unwrap());
    assert!(!tree
        .line_of_sight(
            &models,
            &manager,
            [0.0, 0.0, 10.0],
            [0.0, 0.0, -5.0],
            false,
            deadline()
        )
        .unwrap());
    tree.remove(&h).unwrap();
    models.release(&mut manager, &h).unwrap();
    assert_eq!(manager.model_inventory()[0].references, 1);
    manager.release_model(&shared).unwrap();
    assert!(manager.model_inventory().is_empty());
}

#[test]
fn dynamic_relocation_rotation_refusal_retains_new_position_and_old_transform() {
    let (mut manager, list, mut models) = ordinary();
    let h = construct(&mut manager, &list, &mut models, 10, [0.0; 3]);
    let before = models.inventory().remove(0);
    assert_eq!(
        models.relocate(&manager, &list, &h, 7, [3.0, 4.0, 5.0], 1.0),
        Err(DynamicError::Vmap(Error::NumericBinding))
    );
    // GameObjectModel.cpp:183 writes iPos before the rotation call at :185.
    let after = models.inventory().remove(0);
    assert_eq!(after.position, [3.0, 4.0, 5.0]);
    assert_eq!(
        after.inverse.map(|v| v.map(f32::to_bits)),
        before.inverse.map(|v| v.map(f32::to_bits))
    );
    assert_eq!(
        after.bounds.map(|v| v.map(f32::to_bits)),
        before.bounds.map(|v| v.map(f32::to_bits))
    );
    assert_eq!(after.shared, before.shared);
    assert_eq!(manager.model_inventory()[0].references, 1);
}

#[test]
fn dynamic_bound_membership_transitions_to_unbound_without_losing_models() {
    let (mut manager, list, mut models) = ordinary();
    let bound = construct(&mut manager, &list, &mut models, 10, [0.0; 3]);
    let unbound = models
        .construct(
            &mut manager,
            &list,
            Some(7),
            DynamicModelPose {
                position: [0.0; 3],
                orientation: 0.0,
                scale: 1.0,
            },
            None,
        )
        .unwrap()
        .unwrap();
    let mut tree = DynamicMapTree::new();
    tree.insert(&models, &bound).unwrap();
    tree.insert(&models, &unbound).unwrap();
    assert!(tree
        .inventory()
        .iter()
        .all(|n| !n.pointer_order_bound && n.pending.len() == 2));
    assert!(!tree
        .line_of_sight(
            &models,
            &manager,
            [0.0, 0.0, 10.0],
            [0.0, 0.0, -5.0],
            false,
            deadline()
        )
        .unwrap());
    tree.remove(&bound).unwrap();
    assert!(tree
        .inventory()
        .iter()
        .all(|n| n.pending == [unbound.clone()]));
    assert!(!tree
        .line_of_sight(
            &models,
            &manager,
            [0.0, 0.0, 10.0],
            [0.0, 0.0, -5.0],
            false,
            deadline()
        )
        .unwrap());
}
