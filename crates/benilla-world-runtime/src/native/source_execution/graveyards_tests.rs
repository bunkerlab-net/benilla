// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

fn references() -> GraveyardReferences {
    GraveyardReferences {
        safe_locations: BTreeSet::from([4, 10, 11]),
        areas: BTreeSet::from([12, 0x8000_0001]),
        maps: BTreeSet::from([0, 1]),
    }
}
#[test]
fn validation_order_counts_skips_and_retains_multimap_order() {
    let mut world = SourceGraveyardManager::default();
    let census = world.load_zones(
        Some(&[
            [999, 999, 2, 999], // kind precedes safe-location lookup
            [999, 999, 0, 999], // safe location precedes area
            [4, 999, 0, 999],   // area precedes team
            [4, 999, 1, 999],   // map precedes team
            [4, 12, 0, 1],
            [10, 12, 0, 67],
            [4, 12, 0, 469],
            [10, 12, 0, 0], // same safe location/key, different faction is duplicate
            [4, 0, 1, 0],
            [11, 1, 1, 469],
            [11, 0x8000_0001, 0, 67], // key collision is duplicate, not shift UB
        ]),
        &references(),
    );
    assert_eq!((census.visited, census.inserted), (11, 4));
    assert_eq!(
        census.skipped,
        BTreeMap::from([
            (GraveyardSkip::LinkKind, 1),
            (GraveyardSkip::SafeLocation, 1),
            (GraveyardSkip::Area, 1),
            (GraveyardSkip::Map, 1),
            (GraveyardSkip::Team, 1),
            (GraveyardSkip::Duplicate, 2),
        ])
    );
    assert_eq!(
        world.entries()[&12],
        [
            GraveyardData {
                safe_location: 10,
                team: 67
            },
            GraveyardData {
                safe_location: 4,
                team: 469
            },
        ]
    );
    assert_eq!(
        world.entries().keys().copied().collect::<Vec<_>>(),
        [12, 0x8000_0000, 0x8000_0001]
    );
}
#[test]
fn reload_does_not_clear_and_map_init_copies_all_links_independently() {
    let mut world = SourceGraveyardManager::default();
    let mut map = SourceGraveyardManager::default();
    assert_eq!(
        map.init(&world).unwrap_err().id,
        "execution.world.load_graveyard_zones_owner"
    );
    assert!(map.entries().is_empty());
    let rows = [[4, 12, 0, 0], [10, 1, 1, 67]];
    assert_eq!(world.load_zones(Some(&rows), &references()).inserted, 2);
    assert_eq!(world.load_zones(None, &references()).visited, 0);
    assert_eq!(world.load_zones(Some(&[]), &references()).inserted, 0);
    let reload = world.load_zones(Some(&rows), &references());
    assert_eq!((reload.visited, reload.inserted), (2, 0));
    assert_eq!(
        reload.skipped,
        BTreeMap::from([(GraveyardSkip::Duplicate, 2)])
    );
    map.init(&world).unwrap();
    assert_eq!(map, world);
    world.load_zones(Some(&[[11, 0, 1, 469]]), &references());
    assert_ne!(map, world);
    assert_eq!(map.entries().values().map(Vec::len).sum::<usize>(), 2);
    map.init(&world).unwrap();
    assert_eq!(map, world);
}
#[test]
fn real_catalog_graveyard_census() {
    let Some(root) = std::env::var_os("BENILLA_F2_CATALOG") else {
        eprintln!("SKIP real_catalog_graveyard_census: BENILLA_F2_CATALOG absent");
        return;
    };
    let mut catalog =
        super::super::super::production_compiler::VerifiedCatalog::open(Path::new(&root), false)
            .unwrap();
    let mut world = SourceGraveyardManager::default();
    let census = world.load_catalog(&mut catalog).unwrap();
    println!("GRAVEYARD_CENSUS {census:?}");
    assert_eq!(census.visited, 190);
    assert_eq!(census.inserted, 190);
    assert!(census.skipped.is_empty());
    for map in [0, 1] {
        let mut copied = SourceGraveyardManager::default();
        copied.init(&world).unwrap();
        assert_eq!(
            copied.entries().values().map(Vec::len).sum::<usize>(),
            190,
            "map={map}"
        );
        assert_eq!(copied, world);
    }
}
