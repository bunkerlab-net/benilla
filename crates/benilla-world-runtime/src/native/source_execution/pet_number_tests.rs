// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

fn rows(pet_number: GuidMaximum) -> GuidSeedRows {
    GuidSeedRows {
        player: GuidMaximum::NoResult,
        item: GuidMaximum::NoResult,
        corpse: GuidMaximum::NoResult,
        pet_number,
        creature: GuidMaximum::NoResult,
        gameobject: GuidMaximum::NoResult,
        creature_reserve: 0,
        gameobject_reserve: 0,
    }
}

#[test]
fn pet_number_starts_after_loaded_maximum_or_at_constructor_one() {
    // MAX(character_pet.id)+1 seeds the next return; ObjectMgr.cpp:7278-7284,119-127, ObjectMgr.h:444.
    for (maximum, expected) in [
        (GuidMaximum::NoResult, 1),
        (GuidMaximum::Value(0), 1),
        (GuidMaximum::Value(41), 42),
        (GuidMaximum::Value(0x00ff_ffff), 0x0100_0000),
        (GuidMaximum::Value(u32::MAX), 0),
    ] {
        let globals = GlobalGuidAllocators::from_seed_rows(rows(maximum));
        assert_eq!(
            globals.allocate_pet_number(),
            GuidAllocation {
                counter: expected,
                stop_now: false,
            }
        );
    }
}

#[test]
fn two_maps_share_pet_numbers_but_not_low_guids_in_one_tick() {
    let mut globals = GlobalGuidAllocators::from_seed_rows(rows(GuidMaximum::Value(40)));
    let mut maps = SourceMapManager::new(100);
    for map in [1, 0] {
        maps.register(MapIdentity {
            map,
            instance: 0,
            creation_generation: u64::from(map) + 1,
        })
        .unwrap();
    }
    let jobs = maps.begin(100).unwrap().unwrap();
    assert_eq!(
        jobs.iter().map(|job| job.map.map).collect::<Vec<_>>(),
        [0, 1]
    );
    let mut guids = Vec::new();
    for job in jobs {
        let mut local = globals.map_allocators(job.map).unwrap();
        assert_eq!(local.allocate(job.map, HighGuid::Pet).unwrap().counter, 1);
        for _ in 0..4 {
            let number = globals.allocate_pet_number();
            assert!(!number.stop_now);
            let low = local.allocate(job.map, HighGuid::Pet).unwrap().counter;
            guids.push((
                job.map.map,
                number.counter,
                SourceGuid::with_entry(HighGuid::Pet, number.counter, low),
            ));
        }
    }
    assert_eq!(
        guids
            .iter()
            .map(|(map, number, _)| (*map, *number))
            .collect::<Vec<_>>(),
        [
            (0, 41),
            (0, 42),
            (0, 43),
            (0, 44),
            (1, 45),
            (1, 46),
            (1, 47),
            (1, 48)
        ]
    );
    assert_eq!(guids[0].2.raw(), 0xf140_0000_2900_0002);
    assert_eq!(guids[4].2.raw(), 0xf140_0000_2d00_0002);
    assert_eq!(globals.allocate_pet_number().counter, 49);
    maps.finish().unwrap();
}

#[test]
fn pet_number_counters_belong_to_independent_worlds() {
    let first = GlobalGuidAllocators::from_seed_rows(rows(GuidMaximum::Value(0)));
    let second = GlobalGuidAllocators::from_seed_rows(rows(GuidMaximum::Value(0)));
    assert_eq!(first.allocate_pet_number().counter, 1);
    assert_eq!(first.allocate_pet_number().counter, 2);
    assert_eq!(second.allocate_pet_number().counter, 1);
    assert_eq!(first.allocate_pet_number().counter, 3);
}

#[test]
fn concurrent_map_callers_issue_each_pet_number_once() {
    let globals = GlobalGuidAllocators::from_seed_rows(rows(GuidMaximum::Value(0)));
    let start = std::sync::Barrier::new(2);
    let mut issued = std::thread::scope(|scope| {
        let handles = [0, 1].map(|_| {
            scope.spawn(|| {
                start.wait();
                (0..4096)
                    .map(|_| {
                        let allocation = globals.allocate_pet_number();
                        assert!(!allocation.stop_now);
                        allocation.counter
                    })
                    .collect::<Vec<_>>()
            })
        });
        let mut issued = Vec::new();
        for handle in handles {
            let values = handle.join().unwrap();
            assert!(values.windows(2).all(|pair| pair[0] < pair[1]));
            issued.extend(values);
        }
        issued
    });
    issued.sort_unstable();
    assert_eq!(issued, (1..=8192).collect::<Vec<_>>());
    assert_eq!(globals.allocate_pet_number().counter, 8193);
}

#[test]
fn pet_number_stop_signal_and_unsigned_wrap_match_id_generator() {
    // IdGenerator stops at MAX-1 but returns and postincrements, including unsigned wrap; ObjectMgr.cpp:119-127.
    let globals = GlobalGuidAllocators::from_seed_rows(rows(GuidMaximum::Value(u32::MAX - 3)));
    for (counter, stop_now) in [
        (u32::MAX - 2, false),
        (u32::MAX - 1, true),
        (u32::MAX, true),
        (0, false),
        (1, false),
    ] {
        assert_eq!(
            globals.allocate_pet_number(),
            GuidAllocation { counter, stop_now }
        );
    }
}

#[derive(Deserialize)]
struct RecordedPetCounter {
    initial: u32,
    r#final: [u64; 3],
    reached_increment_count: usize,
    actual_reached_increment_set: Vec<[u64; 8]>,
}

fn compare_recorded_pet_counter(recorded: &RecordedPetCounter) -> Result<usize> {
    require(
        recorded.initial == 1
            && recorded.reached_increment_count == recorded.actual_reached_increment_set.len()
            && recorded.r#final
                == [
                    1,
                    recorded.reached_increment_count as u64 + 1,
                    recorded.reached_increment_count as u64,
                ],
        "execution.pet_number_projection.boundary",
    )?;
    let globals = GlobalGuidAllocators::from_seed_rows(rows(GuidMaximum::Value(0)));
    for row in &recorded.actual_reached_increment_set {
        let native = globals.allocate_pet_number();
        require(
            !native.stop_now
                && u64::from(native.counter) == row[6]
                && u64::from(native.counter.wrapping_add(1)) == row[7],
            "execution.pet_number_projection.increment",
        )?;
    }
    Ok(recorded.reached_increment_count)
}

#[test]
fn unsealed_pet_projection_rejects_duplicate_skipped_and_changed_next_values() {
    let mut recorded = RecordedPetCounter {
        initial: 1,
        r#final: [1, 3, 2],
        reached_increment_count: 2,
        actual_reached_increment_set: vec![[0, 0, 0, 0, 0, 0, 1, 2], [0, 0, 0, 0, 0, 0, 2, 3]],
    };
    assert_eq!(compare_recorded_pet_counter(&recorded).unwrap(), 2);
    for (value, next) in [(1, 2), (3, 4), (2, 4)] {
        recorded.actual_reached_increment_set[1][6] = value;
        recorded.actual_reached_increment_set[1][7] = next;
        assert_eq!(
            compare_recorded_pet_counter(&recorded).unwrap_err().id,
            "execution.pet_number_projection.increment"
        );
    }
}

#[test]
fn retained_round12_pet_numbers_or_clean_absence() {
    let explicit = std::env::var_os("BENILLA_F1_PET_COUNTER_PROJECTION");
    let path = explicit.as_ref().map(std::path::PathBuf::from).unwrap_or_else(|| {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../benilla-config/full-server/f1/sealing/source-choice/seal12-pet-counter-02.json")
    });
    if explicit.is_none() && !path.try_exists().unwrap() {
        eprintln!(
            "SKIP unsealed round-12 pet-number projection: set BENILLA_F1_PET_COUNTER_PROJECTION"
        );
        return;
    }
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        "4f41adb649991a270aa57198072c59d792290431c00b7b6afd3ee07466c40ae4"
    );
    #[derive(Deserialize)]
    struct Projection {
        accepted_exact_reached_pet_increment_sets: bool,
        full: RecordedPetCounter,
        shared: RecordedPetCounter,
        #[serde(rename = "expected_pet_numbers_or_GUIDs_injected")]
        expected_pet_numbers_or_guids_injected: bool,
        reference_execution_sealed: bool,
    }
    let recorded: Projection = serde_json::from_slice(&bytes).unwrap();
    assert!(recorded.accepted_exact_reached_pet_increment_sets);
    assert!(!recorded.expected_pet_numbers_or_guids_injected);
    assert!(!recorded.reference_execution_sealed);
    assert_eq!(compare_recorded_pet_counter(&recorded.full).unwrap(), 8);
    assert_eq!(compare_recorded_pet_counter(&recorded.shared).unwrap(), 8);
    println!("Unsealed round-12 pet-number projection: full=8 shared=8, values=1..8; observed increment order only, not independently reached F5 callers or whole-execution acceptance");
}
