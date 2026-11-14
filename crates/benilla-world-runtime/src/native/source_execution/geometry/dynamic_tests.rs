// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored products through the source model, spell and transport interfaces.
use super::*;
use benilla_native_vmap::live::FileState;
pub(crate) fn map() -> MapIdentity {
    MapIdentity {
        map: 0,
        instance: 0,
        creation_generation: 1,
    }
}
fn word(b: &mut Vec<u8>, w: u32) {
    b.extend(w.to_le_bytes());
}
fn vector(b: &mut Vec<u8>, p: [f32; 3]) {
    for x in p {
        word(b, x.to_bits());
    }
}
fn bounds(b: &mut Vec<u8>) {
    vector(b, [-10.0; 3]);
    vector(b, [10.0; 3]);
}
fn bih(b: &mut Vec<u8>) {
    bounds(b);
    for w in [3, 3 << 30, 1, 0, 1, 0] {
        word(b, w);
    }
}
pub(crate) fn fixture() -> (SourceDynamicModels, SourceDynamicMap, SourceVMapManager) {
    let mut vmap = SourceVMapManager::default();
    let mut model = b"VMAP_7.0WMOD".to_vec();
    word(&mut model, 8);
    word(&mut model, 17);
    model.extend(b"GMOD");
    word(&mut model, 1);
    bounds(&mut model);
    word(&mut model, 101);
    word(&mut model, 1);
    model.extend(b"VERT");
    word(&mut model, 40);
    word(&mut model, 3);
    for p in [[-10.0, -10.0, 0.0], [10.0, -10.0, 0.0], [0.0, 10.0, 0.0]] {
        vector(&mut model, p);
    }
    model.extend(b"TRIM");
    for w in [16, 1, 0, 1, 2] {
        word(&mut model, w);
    }
    model.extend(b"MBIH");
    bih(&mut model);
    model.extend(b"LIQU");
    word(&mut model, 0);
    model.extend(b"GBIH");
    bih(&mut model);
    let mut list = Vec::new();
    for display in [123, 124] {
        word(&mut list, display);
        word(&mut list, 8);
        list.extend(b"test.wmo");
        bounds(&mut list);
    }
    for (name, bytes) in [
        (b"test.wmo.vmo".as_slice(), model),
        (b"temp_gameobject_models".as_slice(), list),
    ] {
        vmap.collision
            .files
            .observe(name, FileState::Present(bytes.into()))
            .unwrap();
    }
    let mut models = SourceDynamicModels::default();
    models.bind_displays([123, 124]).unwrap();
    models.load_model_list(&vmap).unwrap();
    (models, SourceDynamicMap::new(map()), vmap)
}
pub(crate) fn object(handle: ObjectHandle) -> DynamicGameObject {
    DynamicGameObject {
        handle,
        display: 123,
        pose: SourcePose([0; 4]),
        scale_bits: 1f32.to_bits(),
    }
}
#[test]
fn dynamic_runtime_model_dependencies_and_allocation_overlap_are_named() {
    let (mut models, mut map, mut vmap) = fixture();
    let h = ObjectHandle {
        thread: 1,
        construction: 1,
    };
    let op = GameObjectModelOperation::ConstructAndInsert;
    assert!(models
        .operation(&mut map, &mut vmap, object(h), op)
        .unwrap());
    let bound = ObjectHandle {
        construction: 2,
        ..h
    };
    models.bind_allocation(bound, 0x1000, 128).unwrap();
    assert!(models
        .operation(&mut map, &mut vmap, object(bound), op)
        .unwrap());
    let h2 = ObjectHandle {
        construction: 3,
        ..h
    };
    models.bind_allocation(h2, 0x1040, 128).unwrap();
    assert_eq!(
        models
            .operation(&mut map, &mut vmap, object(h2), op)
            .unwrap_err()
            .id,
        "execution.dynamic_collision.model_allocation_overlap"
    );
    assert_eq!(models.models.inventory().len(), 2);
}
#[test]
fn dynamic_transport_default_constructs_inserts_and_relocates_the_real_model() {
    struct Owner {
        models: SourceDynamicModels,
        map: SourceDynamicMap,
        vmap: SourceVMapManager,
    }
    impl MoTransportOwner for Owner {
        fn transport_template(&mut self, _: u32) -> Result<Option<GameObjectTemplate>> {
            Err(refuse("authored.unexpected_template"))
        }
        fn transport_allocation(&mut self, _: ObjectHandle, _: &mut SourceObjects) -> Result<()> {
            Err(refuse("authored.unexpected_allocation"))
        }
        fn transport_geometry(
            &mut self,
            _: ObjectHandle,
        ) -> Result<(
            &mut SourceDynamicModels,
            &mut SourceDynamicMap,
            &mut SourceVMapManager,
        )> {
            Ok((&mut self.models, &mut self.map, &mut self.vmap))
        }
    }
    let (models, map, vmap) = fixture();
    let mut owner = Owner { models, map, vmap };
    let h = ObjectHandle {
        thread: 1,
        construction: 1,
    };
    owner.models.bind_allocation(h, 0x1000, 128).unwrap();
    let mut transport = SourceMoTransport {
        handle: h,
        template: GameObjectTemplate {
            entry: 100,
            kind: 15,
            display: 123,
            faction: 0,
            flags: 0,
            extra_flags: 0,
            scale_bits: 1f32.to_bits(),
            data: [0; 24],
            string_id: 0,
        },
        motion: benilla_sim::source_transports::TransportMotion::new(0),
        position: SourcePose([0; 4]),
        display: 123,
        state: 0,
        kind: 0,
        animation: 0,
        has_model: false,
    };
    transport.has_model = owner
        .transport_model(&transport, GameObjectModelOperation::ConstructAndInsert)
        .unwrap();
    assert!(transport.has_model);
    assert_eq!(owner.map.tree.size(), 4);
    assert_eq!(owner.models.models.inventory().len(), 1);
    assert_eq!(owner.vmap.collision.model_inventory()[0].references, 1);
    assert!(!owner
        .map
        .line_of_sight(
            &owner.models,
            &owner.vmap,
            [0.0, 0.0, 10.0],
            [0.0, 0.0, -5.0],
            false
        )
        .unwrap());
    transport.position.0[2] = 20f32.to_bits();
    for op in [
        GameObjectModelOperation::Remove,
        GameObjectModelOperation::Relocate,
        GameObjectModelOperation::Insert,
    ] {
        assert!(owner.transport_model(&transport, op).unwrap());
    }
    assert!(owner
        .map
        .line_of_sight(
            &owner.models,
            &owner.vmap,
            [0.0, 0.0, 10.0],
            [0.0, 0.0, -5.0],
            false
        )
        .unwrap());
    assert_eq!(
        owner.models.models.inventory()[0].position,
        [0.0, 0.0, 20.0]
    );
}

#[test]
fn dynamic_runtime_inventory_keeps_the_in_flight_constructor_not_a_null_model() {
    let (mut models, mut map, mut vmap) = fixture();
    let h = ObjectHandle {
        thread: 1,
        construction: 1,
    };
    models.bind_allocation(h, 0x1000, 128).unwrap();
    let mut go = object(h);
    go.pose = SourcePose([
        3f32.to_bits(),
        4f32.to_bits(),
        5f32.to_bits(),
        1f32.to_bits(),
    ]);
    go.scale_bits = 2f32.to_bits();
    assert_eq!(
        models
            .operation(
                &mut map,
                &mut vmap,
                go,
                GameObjectModelOperation::ConstructAndInsert
            )
            .unwrap_err()
            .id,
        "execution.vmap.glibc236_fma_rotation_binding"
    );
    let state = models.inventory();
    assert!(state.instances.is_empty());
    assert_eq!(state.constructing.len(), 1);
    assert_eq!(state.constructing[0].0, h);
    assert_eq!(state.construction_fields[0].position, [3.0, 4.0, 5.0]);
    assert_eq!(
        state.construction_fields[0].shared.name.as_ref(),
        b"test.wmo"
    );
    assert_eq!(state.construction_fields[0].scale.to_bits(), 2f32.to_bits());
    assert_eq!(
        state.construction_fields[0].inverse_scale.to_bits(),
        0.5f32.to_bits()
    );
    assert!(state.construction_fields[0].enabled);
    assert_eq!(state.live_allocations, [(0x1000, 0x1080, h)]);
    assert_eq!(vmap.collision.model_inventory()[0].references, 1);
    assert_eq!(map.tree.size(), 0);
}

#[test]
fn dynamic_runtime_map_height_hit_and_explicit_balance_reach_the_real_tree() {
    let (mut models, mut map, mut vmap) = fixture();
    let h = ObjectHandle {
        thread: 1,
        construction: 1,
    };
    models
        .operation(
            &mut map,
            &mut vmap,
            object(h),
            GameObjectModelOperation::ConstructAndInsert,
        )
        .unwrap();
    assert_eq!(
        map.height(&models, &vmap, [0.0, 0.0, 10.0], 20.0)
            .unwrap()
            .to_bits(),
        0
    );
    let (hit, position) = map
        .hit_position(&models, &vmap, [0.0, 0.0, 10.0], [0.0, 0.0, -5.0], 0.0)
        .unwrap();
    assert!(hit);
    assert_eq!(position.map(f32::to_bits), [0; 3]);
    assert_eq!(map.tree.unbalanced(), 1);
    map.balance(&models).unwrap();
    assert_eq!(map.tree.unbalanced(), 0);
    assert_eq!(map.tree.timer_ms(), 200);
}

#[test]
fn dynamic_model_list_refusal_retains_prior_inserts_but_not_a_completed_owner() {
    let mut vmap = SourceVMapManager::default();
    let mut bytes = Vec::new();
    word(&mut bytes, 123);
    word(&mut bytes, 8);
    bytes.extend(b"test.wmo");
    bounds(&mut bytes);
    word(&mut bytes, 124);
    word(&mut bytes, 499);
    vmap.collision
        .files
        .observe(b"temp_gameobject_models", FileState::Present(bytes.into()))
        .unwrap();
    let mut models = SourceDynamicModels::default();
    models.bind_displays([123]).unwrap();
    assert_eq!(
        models.load_model_list(&vmap).unwrap_err().id,
        "execution.dynamic_collision.model_list_read_undefined"
    );
    // GameObjectModel.cpp:50-66 inserts each earlier row before the later read.
    assert_eq!(
        models.list.as_ref().unwrap().bounds(123),
        Some([[-10.0; 3], [10.0; 3]])
    );
    assert!(models.inventory().model_list_attempted);
    assert!(!models.inventory().model_list_loaded);
    let h = ObjectHandle {
        thread: 1,
        construction: 1,
    };
    assert_eq!(
        models
            .operation(
                &mut SourceDynamicMap::new(map()),
                &mut vmap,
                object(h),
                GameObjectModelOperation::ConstructAndInsert
            )
            .unwrap_err()
            .id,
        "execution.dynamic_collision.LoadGameObjectModelList_owner"
    );
}
