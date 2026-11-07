// SPDX-License-Identifier: GPL-3.0-or-later
//! Pinned GUID arithmetic and allocation, without catalog or execution admission.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum HighGuid {
    Player = 0x0000,
    Item = 0x4000,
    GameObject = 0xf110,
    Transport = 0xf120,
    Creature = 0xf130,
    Pet = 0xf140,
    DynamicObject = 0xf100,
    Corpse = 0xf101,
    MoTransport = 0x1fc0,
}
fn has_entry(high: u16) -> bool {
    !matches!(high, 0x4000 | 0x0000 | 0xf100 | 0xf101 | 0x1fc0)
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct SourceGuid(u64);
impl SourceGuid {
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }
    // Preserve unmasked entry/counter overlap and zero counter; ObjectGuid.h:87.
    pub fn with_entry(high: HighGuid, entry: u32, counter: u32) -> Self {
        Self(if counter == 0 {
            0
        } else {
            u64::from(counter) | (u64::from(entry) << 24) | ((high as u64) << 48)
        })
    }
    // The two-argument constructor has no entry field; ObjectGuid.h:88.
    pub fn without_entry(high: HighGuid, counter: u32) -> Self {
        Self(if counter == 0 {
            0
        } else {
            u64::from(counter) | ((high as u64) << 48)
        })
    }
    pub fn raw(self) -> u64 {
        self.0
    }
    pub fn high(self) -> u16 {
        (self.0 >> 48) as u16
    }
    pub fn entry(self) -> u32 {
        if has_entry(self.high()) {
            ((self.0 >> 24) & 0xff_ffff) as u32
        } else {
            0
        }
    }
    pub fn counter(self) -> u32 {
        (self.0
            & if has_entry(self.high()) {
                0xff_ffff
            } else {
                0xffff_ffff
            }) as u32
    }
    pub fn max_counter(high: HighGuid) -> u32 {
        if has_entry(high as u16) {
            0xff_ffff
        } else {
            u32::MAX
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GuidAllocation {
    pub counter: u32,
    pub stop_now: bool,
}
pub struct GuidGenerator {
    high: HighGuid,
    next: u32,
}
impl GuidGenerator {
    pub fn new(high: HighGuid, start: u32) -> Self {
        Self { high, next: start }
    }
    pub fn next_counter(&self) -> u32 {
        self.next
    }
    // StopNow is signalled before returning the old value, even near overflow; ObjectGuid.cpp:64-71.
    pub fn generate(&mut self) -> GuidAllocation {
        let result = GuidAllocation {
            counter: self.next,
            stop_now: self.next >= SourceGuid::max_counter(self.high) - 1000,
        };
        self.next = self.next.wrapping_add(1);
        result
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuidMaximum {
    NoResult,
    Value(u32),
}
impl GuidMaximum {
    // Missing query results retain the constructor's one; ObjectMgr.cpp:6783-6835, ObjectGuid.h:225.
    fn next(self) -> u32 {
        match self {
            Self::NoResult => 1,
            Self::Value(max) => max.wrapping_add(1),
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct GuidSeedRows {
    pub player: GuidMaximum,
    pub item: GuidMaximum,
    pub corpse: GuidMaximum,
    pub creature: GuidMaximum,
    pub gameobject: GuidMaximum,
    pub creature_reserve: u32,
    pub gameobject_reserve: u32,
}
pub struct GlobalGuidAllocators {
    players: GuidGenerator,
    items: GuidGenerator,
    corpses: GuidGenerator,
    static_creatures: GuidGenerator,
    static_gameobjects: GuidGenerator,
    temporary_creature: u32,
    temporary_gameobject: u32,
    created_maps: std::collections::BTreeSet<MapIdentity>,
}
impl GlobalGuidAllocators {
    // Source MAX queries and reserved ranges determine starts, not live-set counts; ObjectMgr.cpp:6781-6855.
    pub fn from_seed_rows(rows: GuidSeedRows) -> Self {
        let creature = rows.creature.next();
        let gameobject = rows.gameobject.next();
        Self {
            players: GuidGenerator::new(HighGuid::Player, rows.player.next()),
            items: GuidGenerator::new(HighGuid::Item, rows.item.next()),
            corpses: GuidGenerator::new(HighGuid::Corpse, rows.corpse.next()),
            static_creatures: GuidGenerator::new(HighGuid::Creature, creature),
            static_gameobjects: GuidGenerator::new(HighGuid::GameObject, gameobject),
            temporary_creature: creature.wrapping_add(rows.creature_reserve),
            temporary_gameobject: gameobject.wrapping_add(rows.gameobject_reserve),
            created_maps: std::collections::BTreeSet::new(),
        }
    }
    pub fn temporary_starts(&self) -> (u32, u32) {
        (self.temporary_creature, self.temporary_gameobject)
    }
    pub fn allocate(&mut self, high: HighGuid) -> Result<GuidAllocation> {
        Ok(match high {
            HighGuid::Player => self.players.generate(),
            HighGuid::Item => self.items.generate(),
            HighGuid::Corpse => self.corpses.generate(),
            _ => return Err(refuse("execution.guid_global_family")),
        })
    }
    // Exhausting the reserved static range returns source zero, without advancing; ObjectMgr.h:814-815.
    pub fn allocate_static(&mut self, high: HighGuid) -> Result<Option<GuidAllocation>> {
        let (generator, limit) = match high {
            HighGuid::Creature => (&mut self.static_creatures, self.temporary_creature),
            HighGuid::GameObject => (&mut self.static_gameobjects, self.temporary_gameobject),
            _ => return Err(refuse("execution.guid_static_family")),
        };
        Ok((generator.next_counter() < limit).then(|| generator.generate()))
    }
    // Each new map starts creature/GO at temporary bases, other families at one; Map.cpp:172-173, Map.h:473-477, ObjectGuid.h:225.
    pub fn map_allocators(&mut self, map: MapIdentity) -> Result<MapGuidAllocators> {
        require(
            map.creation_generation > 0 && self.created_maps.insert(map),
            "execution.guid_map_generation",
        )?;
        Ok(MapGuidAllocators {
            map,
            creatures: GuidGenerator::new(HighGuid::Creature, self.temporary_creature),
            gameobjects: GuidGenerator::new(HighGuid::GameObject, self.temporary_gameobject),
            dynamic_objects: GuidGenerator::new(HighGuid::DynamicObject, 1),
            pets: GuidGenerator::new(HighGuid::Pet, 1),
        })
    }
}
pub struct MapGuidAllocators {
    map: MapIdentity,
    creatures: GuidGenerator,
    gameobjects: GuidGenerator,
    dynamic_objects: GuidGenerator,
    pets: GuidGenerator,
}
impl MapGuidAllocators {
    pub fn map(&self) -> MapIdentity {
        self.map
    }
    // Transports share GO counters, but pets and dynamic objects do not; Map.cpp:2546-2563.
    pub fn allocate(&mut self, map: MapIdentity, high: HighGuid) -> Result<GuidAllocation> {
        require(map == self.map, "execution.guid_map_generation")?;
        Ok(match high {
            HighGuid::Creature => self.creatures.generate(),
            HighGuid::GameObject | HighGuid::Transport => self.gameobjects.generate(),
            HighGuid::DynamicObject => self.dynamic_objects.generate(),
            HighGuid::Pet => self.pets.generate(),
            _ => return Err(refuse("execution.guid_map_family")),
        })
    }
    // Forced maps and event/group dynguid branches allocate only at this seam; Creature.cpp:1662-1672, GameObject.cpp:910-921.
    pub fn spawn_counter(&mut self, request: SpawnGuidRequest) -> Result<GuidAllocation> {
        require(
            matches!(
                request.high,
                HighGuid::Creature | HighGuid::Pet | HighGuid::GameObject | HighGuid::Transport
            ),
            "execution.guid_spawn_family",
        )?;
        if request.force_dynamic
            || request.requested == 0
            || ((request.extra_dynamic || request.group) && request.database == request.requested)
        {
            self.allocate(self.map, request.high)
        } else {
            Ok(GuidAllocation {
                counter: request.requested,
                stop_now: false,
            })
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct SpawnGuidRequest {
    pub high: HighGuid,
    pub database: u32,
    pub requested: u32,
    pub force_dynamic: bool,
    pub extra_dynamic: bool,
    pub group: bool,
}
#[cfg(test)]
mod tests {
    use super::*;
    fn rows() -> GuidSeedRows {
        GuidSeedRows {
            player: GuidMaximum::Value(40),
            item: GuidMaximum::Value(50),
            corpse: GuidMaximum::NoResult,
            creature: GuidMaximum::Value(99),
            gameobject: GuidMaximum::Value(199),
            creature_reserve: 2,
            gameobject_reserve: 3,
        }
    }
    fn map() -> MapIdentity {
        MapIdentity {
            map: 0,
            instance: 0,
            creation_generation: 1,
        }
    }
    #[test]
    fn full_guid_bits_preserve_both_constructors_and_zero() {
        assert_eq!(
            SourceGuid::with_entry(HighGuid::Creature, 0x123456, 0xabcdef).raw(),
            0xf130_1234_56ab_cdef
        );
        assert_eq!(
            SourceGuid::with_entry(HighGuid::Creature, u32::MAX, 0).raw(),
            0
        );
        assert_eq!(SourceGuid::without_entry(HighGuid::Item, 0).raw(), 0);
        let player = SourceGuid::without_entry(HighGuid::Player, 0xfedcba98);
        assert_eq!(player.raw(), 0xfedcba98);
        assert_eq!(player.counter(), 0xfedcba98);
        assert_eq!(player.entry(), 0);
        let overlap = SourceGuid::with_entry(HighGuid::Creature, 0xff000001, 0xff000002);
        assert_eq!(overlap.raw(), 0xf1ff_0000_ff00_0002);
        assert_eq!(
            SourceGuid::from_raw(0x1234_0123_45ab_cdef).counter(),
            0xabcdef
        );
        for high in [
            HighGuid::Player,
            HighGuid::Item,
            HighGuid::DynamicObject,
            HighGuid::Corpse,
            HighGuid::MoTransport,
        ] {
            assert_eq!(SourceGuid::max_counter(high), u32::MAX);
        }
    }
    #[test]
    fn query_maxima_reserved_ranges_and_shared_map_families() {
        let mut globals = GlobalGuidAllocators::from_seed_rows(rows());
        assert_eq!(globals.allocate(HighGuid::Player).unwrap().counter, 41);
        assert_eq!(globals.allocate(HighGuid::Item).unwrap().counter, 51);
        assert_eq!(globals.allocate(HighGuid::Corpse).unwrap().counter, 1);
        assert_eq!(globals.temporary_starts(), (102, 203));
        assert_eq!(
            globals
                .allocate_static(HighGuid::Creature)
                .unwrap()
                .unwrap()
                .counter,
            100
        );
        assert_eq!(
            globals
                .allocate_static(HighGuid::Creature)
                .unwrap()
                .unwrap()
                .counter,
            101
        );
        assert!(globals
            .allocate_static(HighGuid::Creature)
            .unwrap()
            .is_none());
        let mut local = globals.map_allocators(map()).unwrap();
        assert_eq!(
            local.allocate(map(), HighGuid::GameObject).unwrap().counter,
            203
        );
        assert_eq!(
            local.allocate(map(), HighGuid::Transport).unwrap().counter,
            204
        );
        assert_eq!(local.allocate(map(), HighGuid::Pet).unwrap().counter, 1);
        assert_eq!(
            local
                .allocate(map(), HighGuid::DynamicObject)
                .unwrap()
                .counter,
            1
        );
        assert!(local
            .allocate(
                MapIdentity {
                    creation_generation: 2,
                    ..map()
                },
                HighGuid::Creature
            )
            .is_err());
        assert_eq!(
            local.allocate(map(), HighGuid::Creature).unwrap().counter,
            102
        );
    }
    #[test]
    fn source_stop_signal_does_not_suppress_return_or_increment() {
        let threshold = 0xff_ffff - 1000;
        let mut allocator = GuidGenerator::new(HighGuid::Creature, threshold - 1);
        assert_eq!(
            allocator.generate(),
            GuidAllocation {
                counter: threshold - 1,
                stop_now: false
            }
        );
        assert_eq!(
            allocator.generate(),
            GuidAllocation {
                counter: threshold,
                stop_now: true
            }
        );
        assert_eq!(allocator.next_counter(), threshold + 1);
        let mut full = GuidGenerator::new(HighGuid::Player, u32::MAX);
        assert_eq!(
            full.generate(),
            GuidAllocation {
                counter: u32::MAX,
                stop_now: true
            }
        );
        assert_eq!(full.next_counter(), 0);
    }
    #[test]
    fn static_and_dynamic_selection_do_not_reset_map_allocators() {
        let mut globals = GlobalGuidAllocators::from_seed_rows(rows());
        let mut local = globals.map_allocators(map()).unwrap();
        assert!(globals.map_allocators(map()).is_err());
        let request = SpawnGuidRequest {
            high: HighGuid::Creature,
            database: 7,
            requested: 7,
            force_dynamic: false,
            extra_dynamic: false,
            group: false,
        };
        assert_eq!(local.spawn_counter(request).unwrap().counter, 7);
        assert_eq!(
            local
                .spawn_counter(SpawnGuidRequest {
                    group: true,
                    ..request
                })
                .unwrap()
                .counter,
            102
        );
        assert_eq!(
            local
                .spawn_counter(SpawnGuidRequest {
                    requested: 42,
                    group: true,
                    ..request
                })
                .unwrap()
                .counter,
            42
        );
        assert_eq!(
            local
                .spawn_counter(SpawnGuidRequest {
                    force_dynamic: true,
                    ..request
                })
                .unwrap()
                .counter,
            103
        );
        assert_eq!(
            local
                .spawn_counter(SpawnGuidRequest {
                    requested: 0,
                    ..request
                })
                .unwrap()
                .counter,
            104
        );
        assert_eq!(
            local
                .spawn_counter(SpawnGuidRequest {
                    extra_dynamic: true,
                    ..request
                })
                .unwrap()
                .counter,
            105
        );
    }
}
