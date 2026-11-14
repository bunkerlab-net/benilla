// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored SpellStart fixture, not F1 acceptance or complete spell execution.
use super::*;
use benilla_sim::source_rng::{DrawCursor, Mt19937, SourceRandom};
struct Authored<'a> {
    objects: &'a SourceObjects,
    terrain: &'a mut SourceTerrainManager,
    seen: Vec<[u32; 2]>,
}
impl SpellExecutionOwner for Authored<'_> {
    fn unit(&mut self, _: ObjectHandle) -> Result<SpellUnitState> {
        Ok(SpellUnitState {
            player: false,
            gameobject: false,
            alive: true,
            standing: true,
            player_controlled: false,
            combat: false,
            battleground_wait_leave: false,
            gm: false,
            moving: false,
            falling_far: false,
            stealth: false,
            form: 0,
            class: 1,
            combo_target: None,
            mounted: false,
            taxi: false,
            totem: false,
            untargetable: false,
            has_ai: false,
            position: [0; 4],
        })
    }
    fn terrain_geometry(&mut self) -> Result<(&SourceObjects, &mut SourceTerrainManager)> {
        Ok((self.objects, self.terrain))
    }
    fn check(
        &mut self,
        _: &mut SourceSpell,
        _: &SpellTemplate,
        check: SpellCheck,
    ) -> Result<SpellCastResult> {
        if let SpellCheck::AllowedInLocation(location) = check {
            self.seen.push(location);
        }
        Err(check.missing_owner())
    }
}
#[test]
fn authored_spell_start_uses_live_caster_and_computed_zone_before_next_missing_owner() {
    // Spell.cpp:3007-3009,5096-5101; Object.cpp:1242-1244.
    let map = MapIdentity {
        map: 0,
        instance: 7,
        creation_generation: 3,
    };
    let cells = SourceCells::new(map).unwrap();
    let mut objects = SourceObjects::default();
    let caster = objects.construct(1, SourceObjectKind::Creature).unwrap();
    objects.bind_map(caster, map, &cells).unwrap();
    objects
        .set_pose(caster, SourcePose([0, 0, 10.0f32.to_bits(), 0]))
        .unwrap();
    let mut terrain = SourceTerrainManager::default();
    terrain.files.retain_source_absence(0, 32, 32);
    terrain.dbc.areas.insert(
        87,
        SourceAreaEntry {
            id: 87,
            map: 0,
            zone: 12,
            flag: 0,
        },
    );
    terrain.dbc.linked_zones.insert(0, 87);
    let mut mt = Mt19937::from_seed(1);
    let mut cursor = DrawCursor::default();
    let mut sink = |_| {};
    let mut random = SourceRandom::new(1, &mut mt, &mut cursor, &mut sink).unwrap();
    terrain.load_terrain(0, &mut random).unwrap();
    let t = SpellTemplate {
        id: 7,
        equippeditemclass: -1,
        ..Default::default()
    };
    let mut spell = SourceSpell::new(1, &t, caster, SpellTriggerFlags(1), 10).unwrap();
    let mut queue = SourceUnitEvents::default();
    let mut owner = Authored {
        objects: &objects,
        terrain: &mut terrain,
        seen: Vec::new(),
    };
    let error = spell
        .start(&t, SpellTargets::default(), None, &mut queue, &mut owner)
        .unwrap_err();
    assert_eq!(owner.seen, [[12, 87]]);
    assert_eq!(
        error,
        SpellCheck::AllowedInLocation([12, 87]).missing_owner()
    );
    assert_eq!(spell.state, SourceSpellState::Targeting);
    assert_eq!(queue.inventory().queued.len(), 1);
    assert!(owner
        .terrain
        .terrain(0)
        .unwrap()
        .residency
        .load_attempted(32, 32)
        .unwrap());
    // Mutating only the live object position changes the reached terrain dependency.
    drop(owner);
    objects
        .set_pose(
            caster,
            SourcePose([600.0f32.to_bits(), 0, 10.0f32.to_bits(), 0]),
        )
        .unwrap();
    let mut owner = Authored {
        objects: &objects,
        terrain: &mut terrain,
        seen: Vec::new(),
    };
    assert_eq!(
        owner.zone_area(caster).unwrap_err().id,
        "execution.terrain.map_product_unavailable"
    );
    assert!(owner.seen.is_empty());
    let stale = ObjectHandle {
        construction: caster.construction + 1,
        ..caster
    };
    assert!(owner.zone_area(stale).is_err());
}
