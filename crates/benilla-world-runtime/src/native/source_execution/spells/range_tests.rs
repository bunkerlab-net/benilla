// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored CheckRange operands, not live stock execution.
use super::*;
use SpellCastResult as R;

pub(super) struct RangeFixture {
    pub objects: SourceObjects,
    pub caster: ObjectHandle,
    pub target: ObjectHandle,
    pub entry: Option<SpellRangeEntry>,
    pub reaches: [f32; 2],
    pub forward: [bool; 2],
    pub walk: [bool; 2],
    pub mod_owner: Option<ObjectHandle>,
    pub facing: u32,
    pub pet: Option<ObjectHandle>,
    pub charm: Option<ObjectHandle>,
    pub trace: Vec<&'static str>,
    pub vmap: SourceVMapManager,
    pub dynamic_models: SourceDynamicModels,
    pub dynamic_map: SourceDynamicMap,
}
impl RangeFixture {
    pub fn new() -> Self {
        let mut objects = SourceObjects::default();
        let caster = objects.construct(1, SourceObjectKind::Creature).unwrap();
        let target = objects.construct(1, SourceObjectKind::Creature).unwrap();
        for object in [caster, target] {
            objects.set_pose(object, SourcePose([0; 4])).unwrap();
        }
        Self {
            objects,
            caster,
            target,
            entry: Some(SpellRangeEntry {
                minimum: SpellFloat(2f32.to_bits()),
                maximum: SpellFloat(10f32.to_bits()),
                flags: 0,
            }),
            reaches: [0.0; 2],
            forward: [false; 2],
            walk: [false; 2],
            mod_owner: None,
            facing: 0,
            pet: None,
            charm: None,
            trace: Vec::new(),
            vmap: SourceVMapManager::default(),
            dynamic_models: SourceDynamicModels::default(),
            dynamic_map: SourceDynamicMap::new(MapIdentity {
                map: 0,
                instance: 0,
                creation_generation: 1,
            }),
        }
    }
    pub fn spell(&self) -> (SpellTemplate, SourceSpell) {
        let t = SpellTemplate {
            id: 7,
            ..Default::default()
        };
        let mut s = SourceSpell::new(1, &t, self.caster, SpellTriggerFlags(0), 0).unwrap();
        s.targets.unit = Some(self.target);
        s.cast_time = 1;
        s.max_range = 123;
        (t, s)
    }
    fn target_at(&mut self, position: [f32; 3]) {
        self.objects
            .set_pose(
                self.target,
                SourcePose([
                    position[0].to_bits(),
                    position[1].to_bits(),
                    position[2].to_bits(),
                    0,
                ]),
            )
            .unwrap();
    }
}
impl SpellExecutionOwner for RangeFixture {
    fn range_objects(&mut self) -> Result<&SourceObjects> {
        Ok(&self.objects)
    }
    fn range_entry(&mut self, _: u32) -> Result<Option<SpellRangeEntry>> {
        self.trace.push("range");
        Ok(self.entry)
    }
    fn combat_reach(&mut self, object: ObjectHandle) -> Result<f32> {
        self.trace.push("reach");
        Ok(self.reaches[usize::from(object == self.target)])
    }
    fn moving_forward(&mut self, object: ObjectHandle) -> Result<bool> {
        self.trace.push("forward");
        Ok(self.forward[usize::from(object == self.target)])
    }
    fn walking(&mut self, object: ObjectHandle) -> Result<bool> {
        self.trace.push("walk");
        Ok(self.walk[usize::from(object == self.target)])
    }
    fn range_mod_owner(&mut self, _: ObjectHandle) -> Result<Option<ObjectHandle>> {
        self.trace.push("mods");
        Ok(self.mod_owner)
    }
    fn range_facing(&mut self, _: u32) -> Result<u32> {
        self.trace.push("facing");
        Ok(self.facing)
    }
    fn range_pet(&mut self, _: ObjectHandle) -> Result<Option<ObjectHandle>> {
        self.trace.push("pet");
        Ok(self.pet)
    }
    fn range_charm(&mut self, _: ObjectHandle) -> Result<Option<ObjectHandle>> {
        self.trace.push("charm");
        Ok(self.charm)
    }
    fn range_collision_height(&mut self, _: ObjectHandle) -> Result<f32> {
        self.trace.push("height");
        Ok(2.0)
    }
    fn range_vmap(&mut self) -> Result<&SourceVMapManager> {
        self.trace.push("vmap");
        Ok(&self.vmap)
    }
    fn range_dynamic_geometry(
        &mut self,
        _: MapIdentity,
    ) -> Result<(
        &mut SourceDynamicMap,
        &SourceDynamicModels,
        &SourceVMapManager,
    )> {
        self.trace.push("dynamic");
        Ok((&mut self.dynamic_map, &self.dynamic_models, &self.vmap))
    }
}
#[test]
fn range_instant_release_precedes_every_owner_and_preserves_cache() {
    struct Missing;
    impl SpellExecutionOwner for Missing {}
    let f = RangeFixture::new();
    let (t, mut s) = f.spell();
    s.cast_time = 0;
    assert_eq!(s.check_range(&t, false, &mut Missing).unwrap(), R::Ok);
    assert_eq!(s.max_range, 123);
    assert_eq!(
        s.check_range(&t, true, &mut Missing).unwrap_err().id,
        "execution.spells.f3_WorldObject_range_owner"
    );
    s.cast_time = 1;
    assert!(s.check_range(&t, false, &mut Missing).is_err());
}
#[test]
fn range_distance_edges_are_inclusive_and_maximum_precedes_minimum() {
    // Spell.cpp:6242-6245. Adjacent binary32 values straddle each edge.
    for (bits, expected) in [
        (2f32.to_bits() - 1, R::TooClose),
        (2f32.to_bits(), R::Ok),
        (2f32.to_bits() + 1, R::Ok),
        (10f32.to_bits() - 1, R::Ok),
        (10f32.to_bits(), R::Ok),
        (10f32.to_bits() + 1, R::OutOfRange),
    ] {
        let mut f = RangeFixture::new();
        let (t, mut s) = f.spell();
        f.target_at([f32::from_bits(bits), 0.0, 0.0]);
        assert_eq!(s.check_range(&t, true, &mut f).unwrap(), expected);
        assert_eq!(
            s.max_range,
            if expected == R::Ok {
                10f32.to_bits()
            } else {
                123
            }
        );
    }
    let mut f = RangeFixture::new();
    let (t, mut s) = f.spell();
    f.entry.as_mut().unwrap().minimum = SpellFloat(20f32.to_bits());
    f.target_at([15.0, 0.0, 0.0]);
    assert_eq!(s.check_range(&t, true, &mut f).unwrap(), R::OutOfRange);
}
#[test]
fn range_replays_all_2418_pinned_ordinary_unit_observations() {
    let mut count = 0;
    for line in include_str!("../../../../tools/fixtures/spell-range.tsv")
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let fields: Vec<_> = line.split('\t').collect();
        let bits = |i| u32::from_str_radix(fields[i], 16).unwrap();
        let mut f = RangeFixture::new();
        let (t, mut s) = f.spell();
        f.objects
            .set_pose(f.caster, SourcePose([bits(2), bits(3), bits(4), 0]))
            .unwrap();
        f.objects
            .set_pose(f.target, SourcePose([bits(5), bits(6), bits(7), 0]))
            .unwrap();
        f.entry = Some(SpellRangeEntry {
            minimum: SpellFloat(bits(8)),
            maximum: SpellFloat(bits(9)),
            flags: 0,
        });
        f.reaches = [f32::from_bits(bits(10)), f32::from_bits(bits(11))];
        s.max_range = bits(16);
        let expected = match fields[15] {
            "255" => R::Ok,
            "89" => R::OutOfRange,
            "118" => R::TooClose,
            value => panic!("{value}"),
        };
        assert_eq!(
            s.check_range(&t, fields[1] == "1", &mut f).unwrap(),
            expected,
            "{}",
            fields[0]
        );
        assert_eq!(s.max_range, bits(17), "{}", fields[0]);
        count += 1;
    }
    assert_eq!(count, 2418);
}
#[test]
fn range_melee_ranged_and_missing_rows_follow_flags_not_damage_class() {
    let mut f = RangeFixture::new();
    let (mut t, s) = f.spell();
    for damage_class in 0..=3 {
        t.dmgclass = damage_class;
        for (flags, expected) in [
            (0, (2.0, 10.0)),
            (1, (0.0, 5.0)),
            (2, (7.0, 10.0)),
            (3, (0.0, 5.0)),
        ] {
            f.entry.as_mut().unwrap().flags = flags;
            assert_eq!(s.min_max_range(&t, true, &mut f).unwrap(), expected);
        }
    }
    f.entry = None;
    assert_eq!(s.min_max_range(&t, true, &mut f).unwrap(), (0.0, 0.0));
}
#[test]
fn range_next_swing_strict_shortcut_precedes_missing_owners() {
    struct Missing;
    impl SpellExecutionOwner for Missing {}
    let f = RangeFixture::new();
    let (mut t, s) = f.spell();
    t.attributes = 4;
    assert_eq!(
        s.min_max_range(&t, true, &mut Missing).unwrap(),
        (0.0, 100.0)
    );
    assert!(s.min_max_range(&t, false, &mut Missing).is_err());
}
#[test]
fn range_combined_reach_clamp_minimum_and_release_tolerance_edges() {
    let mut f = RangeFixture::new();
    let (t, mut s) = f.spell();
    f.entry.as_mut().unwrap().flags = 1;
    for (reach, expected) in [(0.0, 5.0), (3.67, 5.0), (4.0, 5.33)] {
        f.reaches = [reach, 0.0];
        assert_eq!(s.min_max_range(&t, true, &mut f).unwrap().1, expected);
    }
    f.reaches = [0.0; 2];
    f.target_at([5.0, 0.0, 0.0]);
    assert_eq!(s.check_range(&t, false, &mut f).unwrap(), R::Ok);
    assert_eq!(s.max_range, 5f32.to_bits());
    f.entry.as_mut().unwrap().flags = 0;
    for maximum in [10.0f32, 30.0, 40.0] {
        f.entry.as_mut().unwrap().maximum = SpellFloat(maximum.to_bits());
        let bound: f32 = match maximum as u32 {
            10 => 11.0,
            30 => 33.0,
            _ => 43.0,
        };
        for (bits, expected) in [
            (bound.to_bits(), R::Ok),
            (bound.to_bits() + 1, R::OutOfRange),
        ] {
            f.target_at([f32::from_bits(bits), 0.0, 0.0]);
            assert_eq!(s.check_range(&t, false, &mut f).unwrap(), expected);
        }
    }
}
#[test]
fn range_movement_leeway_requires_each_guard_and_melee_or_player_target() {
    let mut f = RangeFixture::new();
    let (t, s) = f.spell();
    f.entry.as_mut().unwrap().flags = 1;
    for (forward, walk, leeway) in [
        ([false, true], [false, false], false),
        ([true, false], [false, false], false),
        ([true, true], [true, false], false),
        ([true, true], [false, true], false),
        ([true, true], [false, false], true),
    ] {
        f.forward = forward;
        f.walk = walk;
        assert_eq!(
            s.min_max_range(&t, true, &mut f).unwrap().1.to_bits(),
            if leeway { 0x40f5_5556 } else { 5f32.to_bits() }
        );
    }
    f.entry.as_mut().unwrap().flags = 0;
    assert_eq!(s.min_max_range(&t, true, &mut f).unwrap().1, 10.0);
    f.objects.get_mut(f.target).unwrap().kind = SourceObjectKind::Player;
    assert_eq!(
        s.min_max_range(&t, true, &mut f).unwrap().1.to_bits(),
        0x414a_aaab
    );
}
#[test]
fn range_pet_and_charm_substitution_happens_after_range_calculation() {
    for pet in [false, true] {
        let mut f = RangeFixture::new();
        let (mut t, mut s) = f.spell();
        t.effectimplicittargeta1 = 5;
        s.targets.unit = None;
        s.client_cast = true;
        f.target_at([5.0, 0.0, 0.0]);
        if pet {
            f.pet = Some(f.target);
        } else {
            f.charm = Some(f.target);
        }
        assert_eq!(s.check_range(&t, true, &mut f).unwrap(), R::Ok);
        let reads: Vec<_> = f
            .trace
            .iter()
            .copied()
            .filter(|x| *x == "pet" || *x == "charm")
            .collect();
        assert_eq!(
            reads,
            if pet {
                vec!["pet", "pet"]
            } else {
                vec!["pet", "charm"]
            }
        );
    }
}
#[test]
fn range_gameobject_substitution_and_self_skip_preserve_original_range_target() {
    let mut f = RangeFixture::new();
    let (mut t, mut s) = f.spell();
    s.targets.unit = None;
    s.targets.gameobject = Some(f.target);
    t.targets = 0x800;
    f.target_at([11.0, 0.0, 0.0]);
    assert_eq!(s.check_range(&t, true, &mut f).unwrap(), R::OutOfRange);
    s.targets.unit = Some(f.caster);
    assert_eq!(s.check_range(&t, true, &mut f).unwrap(), R::Ok);
}
#[test]
fn range_player_front_arc_borders_and_metadata_guard() {
    let mut f = RangeFixture::new();
    let (t, mut s) = f.spell();
    f.objects.get_mut(f.caster).unwrap().kind = SourceObjectKind::Player;
    f.facing = 1;
    for (x, y, expected) in [
        (5.0, 0.0, R::Ok),
        (-5.0, 0.0, R::UnitNotInfront),
        (0.0, 5.0, R::Ok),
        // Object.cpp:1471,1518 loses one ulp across TAU; oracle.tsv:1365.
        (0.0, -5.0, R::UnitNotInfront),
        (0.001, -5.0, R::Ok),
        (-0.001, -5.0, R::UnitNotInfront),
        (0.001, 5.0, R::Ok),
        (-0.001, 5.0, R::UnitNotInfront),
    ] {
        f.target_at([x, y, 0.0]);
        assert_eq!(s.check_range(&t, true, &mut f).unwrap(), expected);
    }
    f.facing = 0;
    f.target_at([-5.0, 0.0, 0.0]);
    assert_eq!(s.check_range(&t, true, &mut f).unwrap(), R::Ok);
}
#[test]
fn range_location_masks_distances_and_los_missing_collision_owner() {
    for mask in [0x20, 0x40] {
        let mut f = RangeFixture::new();
        let (mut t, mut s) = f.spell();
        s.targets.unit = None;
        s.targets.mask = mask;
        for (x, expected) in [
            (f32::from_bits(2f32.to_bits() - 1), R::TooClose),
            (2.0, R::Ok),
            (10.0, R::Ok),
            (f32::from_bits(10f32.to_bits() + 1), R::OutOfRange),
        ] {
            let position = Some([x.to_bits(), 0, 0]);
            s.targets.source = position;
            s.targets.destination = position;
            t.attributesex2 = 4;
            assert_eq!(s.check_range(&t, true, &mut f).unwrap(), expected);
        }
        t.attributesex2 = 0;
        s.targets.source = Some([5f32.to_bits(), 0, 0]);
        s.targets.destination = s.targets.source;
        assert_eq!(
            s.check_range(&t, true, &mut f).unwrap_err().id,
            "execution.spells.WorldObject_GetMap_owner"
        );
        s.targets.mask = mask | 2;
        assert_eq!(s.check_range(&t, true, &mut f).unwrap(), R::Ok);
    }
}
#[test]
fn range_player_modifiers_stop_at_named_source_calls() {
    let mut f = RangeFixture::new();
    let (mut t, s) = f.spell();
    f.objects.get_mut(f.caster).unwrap().kind = SourceObjectKind::Player;
    t.attributes = 2;
    assert_eq!(
        s.min_max_range(&t, true, &mut f).unwrap_err().id,
        "execution.spells.Player_GetWeaponForAttack_RangedModRange_owner"
    );
    t.attributes = 0;
    f.mod_owner = Some(f.caster);
    assert_eq!(
        s.min_max_range(&t, true, &mut f).unwrap_err().id,
        "execution.spells.Player_ApplySpellMod_RANGE_owner"
    );
}

// Authored triangle at z=0, encoded as the live VMap product format.
fn blocking_vmap(vmap: &mut SourceVMapManager) {
    use benilla_native_vmap::live::FileState;
    fn word(b: &mut Vec<u8>, w: u32) {
        b.extend(w.to_le_bytes());
    }
    fn vector(b: &mut Vec<u8>, v: [f32; 3]) {
        for x in v {
            word(b, x.to_bits());
        }
    }
    fn bounds(b: &mut Vec<u8>, world: bool) {
        let mid = if world { 17066.666 } else { 0.0 };
        vector(b, [mid - 10.0, mid - 10.0, -10.0]);
        vector(b, [mid + 10.0, mid + 10.0, 20.0]);
    }
    fn bih(b: &mut Vec<u8>, world: bool) {
        bounds(b, world);
        for w in [3, 3 << 30, 1, 0, 1, 0] {
            word(b, w);
        }
    }
    let mut tree = b"VMAP_7.0".to_vec();
    tree.push(1);
    tree.extend(b"NODE");
    bih(&mut tree, true);
    tree.extend(b"GOBJ");
    let mut model = b"VMAP_7.0WMOD".to_vec();
    word(&mut model, 8);
    word(&mut model, 17);
    model.extend(b"GMOD");
    word(&mut model, 1);
    bounds(&mut model, false);
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
    bih(&mut model, false);
    model.extend(b"LIQU");
    word(&mut model, 0);
    model.extend(b"GBIH");
    bih(&mut model, false);
    let mut tile = b"VMAP_7.0".to_vec();
    word(&mut tile, 1);
    word(&mut tile, 4);
    tile.extend(2u16.to_le_bytes());
    word(&mut tile, 1);
    vector(&mut tile, [17066.666, 17066.666, 0.0]);
    vector(&mut tile, [0.0; 3]);
    word(&mut tile, 1f32.to_bits());
    bounds(&mut tile, true);
    word(&mut tile, 8);
    tile.extend(b"test.wmo");
    word(&mut tile, 0);
    for (name, bytes) in [
        (&b"000.vmtree"[..], tree),
        (&b"test.wmo.vmo"[..], model),
        (&b"000_02_01.vmtile"[..], tile),
    ] {
        vmap.collision
            .files
            .observe(name, FileState::Present(bytes.into()))
            .unwrap();
    }
    assert_eq!(
        vmap.load_map(0, 1, 2).unwrap(),
        benilla_native_vmap::live::LoadResult::Ok
    );
}
fn bind_map(f: &mut RangeFixture) {
    let map = MapIdentity {
        map: 0,
        instance: 0,
        creation_generation: 1,
    };
    f.objects
        .bind_map(f.caster, map, &SourceCells::new(map).unwrap())
        .unwrap();
}
#[test]
fn range_los_static_block_short_circuits_dynamic_but_clear_consults_tree() {
    let mut f = RangeFixture::new();
    bind_map(&mut f);
    blocking_vmap(&mut f.vmap);
    f.objects
        .set_pose(f.caster, SourcePose([0, 0, 8f32.to_bits(), 0]))
        .unwrap();
    assert!(!f.range_point_los(f.caster, [0.0, 0.0, -5.0]).unwrap());
    assert_eq!(f.trace, ["height", "vmap"]);
    assert!(f.range_point_los(f.caster, [0.0, 0.0, 5.0]).unwrap());
    assert_eq!(f.trace, ["height", "vmap", "height", "vmap", "dynamic"]);
}
#[test]
fn range_dest_and_source_los_failures_and_ignore_attributes() {
    for mask in [0x20, 0x40] {
        let mut f = RangeFixture::new();
        bind_map(&mut f);
        blocking_vmap(&mut f.vmap);
        f.objects
            .set_pose(f.caster, SourcePose([0, 0, 3f32.to_bits(), 0]))
            .unwrap();
        let (mut t, mut s) = f.spell();
        s.targets.unit = None;
        s.targets.mask = mask;
        s.targets.source = Some([0, 0, (-5f32).to_bits()]);
        s.targets.destination = s.targets.source;
        assert_eq!(s.check_range(&t, true, &mut f).unwrap(), R::LineOfSight);
        assert_eq!(s.max_range, 123);
        // z=-1 becomes zero after +1.f. The caster eye starts at z=5.
        s.targets.source = Some([0, 0, 0]);
        s.targets.destination = s.targets.source;
        assert_eq!(s.check_range(&t, true, &mut f).unwrap(), R::Ok);
        for id in [8733, 8735, 24742] {
            t.id = id;
            assert_eq!(s.check_range(&t, true, &mut f).unwrap(), R::Ok);
        }
        t.id = 7;
        t.rangeindex = 13;
        f.trace.clear();
        assert_eq!(s.check_range(&t, true, &mut f).unwrap(), R::Ok);
        assert!(f.trace.ends_with(&["height", "vmap", "dynamic"]));
        t.attributesex2 = 4;
        assert_eq!(s.check_range(&t, true, &mut f).unwrap(), R::Ok);
    }
}

#[test]
fn range_pet_null_unit_dereference_refuses_at_call_not_before() {
    let mut f = RangeFixture::new();
    let (mut t, mut s) = f.spell();
    f.objects.get_mut(f.caster).unwrap().kind = SourceObjectKind::GameObject;
    t.effectimplicittargeta1 = 5;
    s.targets.unit = None;
    // Non-client casts never dereference m_caster here.
    assert_eq!(s.check_range(&t, true, &mut f).unwrap(), R::Ok);
    s.client_cast = true;
    assert_eq!(
        s.check_range(&t, true, &mut f).unwrap_err().id,
        "execution.spells.CheckRange_GetPet_null_caster_undefined"
    );
    // A Unit caster safely reaches the actual pet/charm reads.
    f.objects.get_mut(f.caster).unwrap().kind = SourceObjectKind::Creature;
    assert_eq!(s.check_range(&t, true, &mut f).unwrap(), R::Ok);
}
#[test]
fn range_collision_height_refuses_before_static_or_dynamic_query() {
    struct MissingHeight(SourceObjects);
    impl SpellExecutionOwner for MissingHeight {
        fn range_objects(&mut self) -> Result<&SourceObjects> {
            Ok(&self.0)
        }
    }
    let mut f = RangeFixture::new();
    bind_map(&mut f);
    assert_eq!(
        MissingHeight(f.objects)
            .range_point_los(f.caster, [0.0; 3])
            .unwrap_err()
            .id,
        "execution.spells.WorldObject_GetCollisionHeight_owner"
    );
}
#[test]
fn range_zero_minimum_corpse_and_positive_minimum_reach_guards() {
    let mut f = RangeFixture::new();
    let (t, mut s) = f.spell();
    s.targets.unit = None;
    s.targets.corpse = Some(f.target);
    f.reaches = [1.5; 2];
    for (minimum, expected) in [(0.0, 0.0), (-1.0, -1.0), (1.0, 4.0)] {
        f.entry.as_mut().unwrap().minimum = SpellFloat(f32::to_bits(minimum));
        assert_eq!(s.min_max_range(&t, true, &mut f).unwrap(), (expected, 13.0));
    }
    f.entry.as_mut().unwrap().flags = 2;
    f.entry.as_mut().unwrap().minimum = SpellFloat(1f32.to_bits());
    assert_eq!(s.min_max_range(&t, true, &mut f).unwrap(), (6.0, 13.0));
}

#[test]
fn range_negative_y_arc_source_rounding_trace() {
    use benilla_sim::facing::{atan2_bits, bearing_bits, normalize_bits};
    // Object.cpp:1467-1471,1509-1522. Pinned facing/oracle.tsv:1365
    // independently records false for (0,-1), orientation 0, arc PI.
    let atan = atan2_bits((-5f32).to_bits(), 0).unwrap();
    let bearing = bearing_bits([0, 0], [0, (-5f32).to_bits()]).unwrap();
    let arc = normalize_bits(0x4049_0fdb).unwrap();
    let relative = f32::from_bits(bearing) - 0.0;
    let normalized = normalize_bits(relative.to_bits()).unwrap();
    let folded = f32::from_bits(normalized) - f32::from_bits(0x40c9_0fdb);
    let half = f32::from_bits(arc) / 2.0;
    let lower = -half;
    println!("negative-Y atan2={atan:08x} bearing={bearing:08x} arc={arc:08x} relative={:08x} normalized={normalized:08x} folded={:08x} lower={:08x} upper={:08x} lower_inclusive={} upper_inclusive={}",
        relative.to_bits(), folded.to_bits(), lower.to_bits(), half.to_bits(),
        folded >= lower, folded <= half);
    assert_eq!(
        [
            atan,
            bearing,
            arc,
            relative.to_bits(),
            normalized,
            folded.to_bits(),
            lower.to_bits(),
            half.to_bits()
        ],
        [
            0xbfc9_0fdb,
            0x4096_cbe4,
            0x4049_0fdb,
            0x4096_cbe4,
            0x4096_cbe4,
            0xbfc9_0fdc,
            0xbfc9_0fdb,
            0x3fc9_0fdb
        ]
    );
    assert!(!(folded >= lower && folded <= half));
}

#[test]
fn range_los_closed_door_blocks_and_open_door_allows_the_same_spell() {
    let mut f = RangeFixture::new();
    bind_map(&mut f);
    let (models, map, vmap) = dynamic_models::tests::fixture();
    f.dynamic_models = models;
    f.dynamic_map = map;
    f.vmap = vmap;
    let door = f
        .objects
        .construct(1, SourceObjectKind::GameObject)
        .unwrap();
    f.dynamic_models.bind_allocation(door, 0x1000, 128).unwrap();
    f.dynamic_models
        .operation(
            &mut f.dynamic_map,
            &mut f.vmap,
            dynamic_models::tests::object(door),
            GameObjectModelOperation::ConstructAndInsert,
        )
        .unwrap();
    f.objects
        .set_pose(f.caster, SourcePose([0, 0, 3f32.to_bits(), 0]))
        .unwrap();
    let (t, mut s) = f.spell();
    s.targets.unit = None;
    s.targets.mask = 0x40;
    s.targets.destination = Some([0, 0, (-5f32).to_bits()]);
    assert_eq!(s.check_range(&t, true, &mut f).unwrap(), R::LineOfSight);
    assert_eq!(s.max_range, 123);
    assert!(f.trace.ends_with(&["height", "vmap", "dynamic"]));
    f.dynamic_models
        .operation(
            &mut f.dynamic_map,
            &mut f.vmap,
            dynamic_models::tests::object(door),
            GameObjectModelOperation::Enable(false),
        )
        .unwrap();
    assert_eq!(s.check_range(&t, true, &mut f).unwrap(), R::Ok);
    assert_eq!(s.max_range, 10f32.to_bits());
}
