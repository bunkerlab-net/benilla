// SPDX-License-Identifier: GPL-3.0-or-later
// Authored MPQ closures around the separately captured CMaNGOS
// 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f one-candidate records. Exact comparisons
// cover this corpus under the named portable profile, not every native libm.
// Projection-only comparisons omit generated IDs. Complete-record comparisons
// below include source-allocated IDs; multi-child ordering is covered separately.

use super::*;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::PathBuf};

const DP_POLICY: &str = "g3d-portable-libm-0.2.16-v1";
const DP_ROOT: &str = "World\\Wmo\\Temple.wmo";
const DP_GROUP: &str = "World\\Wmo\\Temple_000.wmo";
const DP_SKIPPED_GROUP: &str = "World\\Wmo\\Temple_001.wmo";
const DP_MODEL: &str = "Models\\Solid.m2";
const DP_ADT: &str = "World\\Maps\\Selected\\Selected_12_34.adt";

fn dp_profile() -> DoodadProjectionProfile {
    DoodadProjectionProfile::from_policy_id(DP_POLICY).unwrap()
}

fn dp_fixture(case: &str, name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../benilla-native-geometry-tests/tools/fixtures/wmo_doodad_transform_source")
            .join(case)
            .join(name),
    )
    .unwrap()
}

fn dp_replace(entries: &mut [Entry], name: &str, bytes: Vec<u8>) {
    *entries.iter_mut().find(|entry| entry.name == name).unwrap() = raw_entry(name, bytes);
}

fn dp_group(skip: bool, collision: bool) -> Vec<u8> {
    let mut bytes = wmo_group(skip);
    bytes[12 + 8 + 68 + 8] = if collision { 8 } else { 0 };
    let references = chunk(b"MODR", &[0, 0, 0, 0]);
    let length = word(&bytes, 16) + references.len() as u32;
    set_word(&mut bytes, 16, length);
    bytes.extend(references);
    bytes
}

fn dp_entries(case: &str, collision: bool) -> Vec<Entry> {
    dp_entries_from(
        &dp_fixture(case, "input.modn"),
        &dp_fixture(case, "input.modf"),
        &dp_fixture(case, "input.modd"),
        collision,
    )
}

fn dp_entries_from(modn: &[u8], modf: &[u8], modd: &[u8], collision: bool) -> Vec<Entry> {
    assert_eq!(modf.len(), 64);
    assert_eq!(modd.len(), 40);
    // The standalone source probe has an unused prefix name and uses MODF name
    // index 3. Rebase these file-local indexes into this complete MPQ closure;
    // keep the original model path and every numeric transform byte unchanged.
    let model_offset = (word(modd, 0) & 0x00ff_ffff) as usize;
    let model_name = &modn[model_offset..];
    assert!(matches!(
        model_name,
        b"Models\\Solid.mdx\0" | b"Models\\Solid.mdl\0" | b"Models\\Solid.m2\0"
    ));
    let mut spawn = modd.to_vec();
    set_word(&mut spawn, 0, word(modd, 0) & 0xff00_0000);
    let mut placement = modf.to_vec();
    set_word(&mut placement, 0, 0);

    let mut root = wmo_root(2);
    set_word(&mut root, 20 + 16, 1);
    set_word(&mut root, 20 + 20, 1);
    set_word(&mut root, 20 + 24, 1);
    root.extend(chunk(b"MODN", model_name));
    let mut set = [0; 32];
    set_word(&mut set, 24, 1);
    root.extend(chunk(b"MODS", &set));
    root.extend(chunk(b"MODD", &spawn));
    let mut adt = chunk(b"MVER", &18u32.to_le_bytes());
    let mut root_name = DP_ROOT.as_bytes().to_vec();
    root_name.push(0);
    adt.extend(chunk(b"MWMO", &root_name));
    adt.extend(chunk(b"MODF", &placement));

    let mut entries = wmo_entries(2);
    dp_replace(&mut entries, MAP, maps(&[(7, "Selected")]));
    dp_replace(&mut entries, DP_ROOT, root);
    dp_replace(&mut entries, DP_GROUP, dp_group(false, collision));
    dp_replace(&mut entries, DP_SKIPPED_GROUP, dp_group(true, true));
    entries.push(raw_entry(DP_MODEL, m2(true)));
    entries.push(raw_entry(DP_ADT, adt));
    entries
}

fn dp_tree(path: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, path: &Path, result: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let relative = path.strip_prefix(root).unwrap().to_path_buf();
            if entry.file_type().unwrap().is_dir() {
                result.insert(relative, Vec::new());
                visit(root, &path, result);
            } else {
                result.insert(relative, fs::read(path).unwrap());
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(path, path, &mut result);
    result
}

fn dp_assert_source_record(case: &str, projection: &DoodadProjection) {
    let expected = dp_fixture(case, "child.dir");
    assert!(expected.len() > 54, "{case}");
    assert_eq!(
        projection.prefix_before_generated_id().as_slice(),
        &expected[..18],
        "{case} prefix"
    );
    assert_eq!(
        projection.suffix_after_generated_id(),
        &expected[22..],
        "{case} suffix"
    );
    assert_eq!(
        projection.prefix_before_generated_id().len()
            + projection.suffix_after_generated_id().len(),
        expected.len() - 4,
        "{case} excludes exactly the generated-ID slot"
    );
    for (index, value) in projection
        .position()
        .into_iter()
        .chain(projection.rotation())
        .chain([projection.scale()])
        .enumerate()
    {
        assert_eq!(value.to_bits(), word(&expected, 22 + index * 4), "{case}");
    }
}

#[test]
fn complete_authored_mpq_closures_project_original_records_without_generated_ids() {
    for (case, disposition) in [
        ("portable_mdx", PlacementDisposition::Emits),
        ("portable_mdl", PlacementDisposition::Emits),
        ("portable_m2", PlacementDisposition::Emits),
        ("portable_nonunit", PlacementDisposition::Emits),
        ("portable_parent_rotation", PlacementDisposition::Emits),
        ("portable_composed", PlacementDisposition::Emits),
        ("portable_scale_zero", PlacementDisposition::Emits),
        ("portable_scale_negative", PlacementDisposition::Emits),
        ("portable_scale_signed_zero", PlacementDisposition::Emits),
        ("portable_zero_origin", PlacementDisposition::Emits),
        ("portable_signed_zero_origin", PlacementDisposition::Emits),
        (
            "portable_destructible_origin",
            PlacementDisposition::Destructible,
        ),
        (
            "portable_destructible_nonzero",
            PlacementDisposition::Destructible,
        ),
        (
            "portable_destructible_signed_zero",
            PlacementDisposition::Destructible,
        ),
        (
            "portable_no_parent_collision_origin",
            PlacementDisposition::NoCollision,
        ),
        (
            "portable_no_parent_collision_nonzero",
            PlacementDisposition::NoCollision,
        ),
        (
            "portable_no_parent_collision_signed_zero",
            PlacementDisposition::NoCollision,
        ),
    ] {
        let temp = Temp::new();
        let data = temp.data();
        let entries = dp_entries(case, disposition != PlacementDisposition::NoCollision);
        save(&data, "wmo.MPQ", &entries);
        let before = dp_tree(&temp.0);
        let prepared = prepare(&data, 7, || false).unwrap();
        assert_eq!(prepared.summary().state, MapDiscoveryState::Scanned);
        assert_eq!(prepared.summary().adt_attempts, 4096);
        assert_eq!(prepared.summary().adts_present, 1);
        assert_eq!(prepared.summary().global_root_events, 1);
        assert_eq!(prepared.summary().m2_raw_products, 1);
        assert_eq!(prepared.summary().wmo_raw_products, 1);
        assert_eq!(prepared.placements().len(), 1);
        assert_eq!(prepared.doodad_candidates().len(), 1);
        assert_eq!(
            prepared.placements()[0].disposition(),
            disposition,
            "{case}"
        );
        assert_eq!(prepared.placements()[0].tile(), Tile { x: 12, y: 34 });
        let root = prepared
            .products()
            .iter()
            .find(|product| product.source_member() == DP_ROOT)
            .unwrap();
        assert_eq!(root.wmo_group_counts(), Some((2, 1)));
        let original_placement = prepared.placements()[0].clone();
        let original_candidate = prepared.doodad_candidates()[0].clone();
        let projection = prepared.project_doodad(0, dp_profile()).unwrap();
        dp_assert_source_record(case, &projection);
        assert_eq!(projection.profile().policy_id(), DP_POLICY);
        assert_eq!(projection.require_policy(DP_POLICY), Ok(()));
        assert_eq!(prepared.placements()[0], original_placement);
        assert_eq!(prepared.doodad_candidates()[0], original_candidate);
        assert_eq!(
            projection.scale().to_bits(),
            original_candidate.spawn.scale.to_bits(),
            "{case} never composes MODF or MODD scale into position"
        );
        let suffix = projection.suffix_after_generated_id();
        assert_eq!(word(suffix, 28), original_candidate.serialized_name_len);
        assert_eq!(suffix.len(), 32 + word(suffix, 28) as usize);
        assert_eq!(
            &suffix[32..],
            if case == "portable_m2" {
                b"Solid.m2".as_slice()
            } else {
                b"Solid.m2\0".as_slice()
            }
        );
        assert!(!format!("{projection:?}").contains("Solid"));
        prepared.verify_unchanged(|| false).unwrap();
        assert_eq!(dp_tree(&temp.0), before, "{case} creates no output files");
    }
}

#[test]
fn captured_projection_rejects_invalid_indexes_and_incompatible_policy_claims() {
    let temp = Temp::new();
    let data = temp.data();
    save(&data, "wmo.MPQ", &dp_entries("portable_mdx", true));
    let before = dp_tree(&temp.0);
    let prepared = prepare(&data, 7, || false).unwrap();
    let profile = dp_profile();
    let projection = prepared.project_doodad(0, profile).unwrap();
    assert_eq!(projection, prepared.project_doodad(0, profile).unwrap());
    for index in [1, usize::MAX] {
        assert_eq!(
            prepared.project_doodad(index, profile),
            Err(DoodadProjectionError::CandidateIndex)
        );
    }
    for other in [
        "",
        "native",
        "g3d-native-linux",
        "g3d-portable-libm-0.2.17-v1",
        "g3d-portable-libm-0.2.16-v2",
        "g3d-portable-libm-0.2.16-v1 ",
    ] {
        assert_eq!(
            DoodadProjectionProfile::from_policy_id(other),
            Err(DoodadProjectionError::PolicyMismatch)
        );
        assert_eq!(
            profile.require_match(other),
            Err(DoodadProjectionError::PolicyMismatch)
        );
        assert_eq!(
            projection.require_policy(other),
            Err(DoodadProjectionError::PolicyMismatch)
        );
    }
    // Publicly copied raw candidates cannot replace the privately retained one.
    let mut detached = prepared.doodad_candidates()[0].clone();
    detached.spawn.position = [f32::NAN; 3];
    detached.spawn.scale = f32::INFINITY;
    detached.serialized_name_len = u32::MAX;
    assert_ne!(
        detached.serialized_name_len,
        prepared.doodad_candidates()[0].serialized_name_len
    );
    assert_eq!(projection, prepared.project_doodad(0, profile).unwrap());
    prepared.verify_unchanged(|| false).unwrap();
    assert_eq!(dp_tree(&temp.0), before);
}

#[test]
fn projection_binding_names_the_pin_policy_and_full_capture_epoch_and_is_not_a_lease() {
    for changed_member in [DP_ROOT, DP_MODEL, DP_SKIPPED_GROUP, DP_ADT, MAP] {
        let temp = Temp::new();
        let data = temp.data();
        let mut entries = dp_entries("portable_mdx", true);
        save(&data, "wmo.MPQ", &entries);
        let prepared = prepare(&data, 7, || false).unwrap();
        prepared.verify_unchanged(|| false).unwrap();
        let projection = prepared.project_doodad(0, dp_profile()).unwrap();
        let mut digest = Sha256::new();
        digest.update(b"benilla-wmo-doodad-projection-binding-v3\0");
        digest.update(SOURCE_PIN.as_bytes());
        digest.update([0]);
        digest.update(DP_POLICY.as_bytes());
        digest.update([0]);
        digest.update(b"gcc14-libstdcxx-u16-identity-prime-load1-fresh-insert-v1");
        digest.update([0]);
        digest.update(b"cmangos-vmaps05-encounter-ledger-v5");
        digest.update([0]);
        digest.update(b"cmangos-client-u32-child-u16-first-encounter-v1");
        digest.update([0]);
        digest.update(prepared.source_epoch().as_bytes());
        let expected: [u8; 32] = digest.finalize().into();
        assert_eq!(projection.preprocessing_binding(), &expected);

        let changed = entries
            .iter_mut()
            .find(|entry| entry.name == changed_member)
            .unwrap();
        let at = match changed_member {
            DP_ROOT => changed.bytes.len() - 4, // MODD color, not a transform.
            DP_MODEL => changed.bytes.len() - 4, // Collision vertex only.
            DP_SKIPPED_GROUP => 12 + 8,         // Ignored skipped-group name offset.
            DP_ADT => changed.bytes.len() - 64 + 4, // Raw MODF unique ID.
            MAP => 20 + 8, // Unconsumed field within the physical selected row.
            _ => unreachable!(),
        };
        changed.bytes[at] ^= 1;
        save(&data, "wmo.MPQ", &entries);
        let after_caller_write = dp_tree(&temp.0);
        assert!(matches!(
            prepared.verify_unchanged(|| false),
            Err(Error::Source(SourceError::Changed))
        ));
        // Frozen projection remains deterministic, but must not be mistaken for
        // a fresh-chain verification or content/gameplay admission.
        assert_eq!(
            projection,
            prepared.project_doodad(0, dp_profile()).unwrap()
        );
        let fresh = prepare(&data, 7, || false).unwrap();
        fresh.verify_unchanged(|| false).unwrap();
        let fresh_projection = fresh.project_doodad(0, dp_profile()).unwrap();
        assert_ne!(
            fresh.source_epoch(),
            prepared.source_epoch(),
            "{changed_member}"
        );
        assert_ne!(
            fresh_projection.preprocessing_binding(),
            projection.preprocessing_binding(),
            "{changed_member}"
        );
        assert_eq!(
            fresh_projection.prefix_before_generated_id(),
            projection.prefix_before_generated_id()
        );
        assert_eq!(
            fresh_projection.suffix_after_generated_id(),
            projection.suffix_after_generated_id()
        );
        assert_eq!(dp_tree(&temp.0), after_caller_write);
    }
}

#[test]
fn finite_source_mutations_fail_the_projection_numeric_envelope_without_writes() {
    for case in [
        "zero_norm",
        "signed_zero_norm",
        "underflow_norm",
        "overflow_norm",
        "parent_angle",
        "position_overflow",
    ] {
        let temp = Temp::new();
        let data = temp.data();
        let modn = dp_fixture("portable_mdx", "input.modn");
        let mut modf = dp_fixture("portable_mdx", "input.modf");
        let mut modd = dp_fixture("portable_mdx", "input.modd");
        match case {
            "zero_norm" | "signed_zero_norm" | "underflow_norm" | "overflow_norm" => {
                let value = match case {
                    "zero_norm" => 0.0_f32,
                    "signed_zero_norm" => -0.0,
                    "underflow_norm" => f32::from_bits(1),
                    "overflow_norm" => f32::MAX,
                    _ => unreachable!(),
                };
                for index in 0..4 {
                    set_word(&mut modd, 16 + index * 4, value.to_bits());
                }
            }
            "parent_angle" => set_word(
                &mut modf,
                20,
                f32::from_bits(360.0_f32.to_bits() + 1).to_bits(),
            ),
            "position_overflow" => {
                for index in 0..3 {
                    set_word(&mut modf, 8 + index * 4, f32::MAX.to_bits());
                    set_word(&mut modd, 4 + index * 4, f32::MAX.to_bits());
                }
            }
            _ => unreachable!(),
        }
        save(
            &data,
            "wmo.MPQ",
            &dp_entries_from(&modn, &modf, &modd, true),
        );
        let before = dp_tree(&temp.0);
        let prepared = prepare(&data, 7, || false).unwrap();
        assert_eq!(prepared.doodad_candidates().len(), 1, "{case}");
        assert_eq!(
            prepared.project_doodad(0, dp_profile()),
            Err(DoodadProjectionError::NumericDomain),
            "{case}"
        );
        prepared.verify_unchanged(|| false).unwrap();
        assert_eq!(dp_tree(&temp.0), before, "{case}");
    }
}

#[test]
fn nonfinite_source_mutations_never_produce_a_prepared_projection() {
    for (is_parent, at) in [(true, 8), (true, 20), (false, 4), (false, 16), (false, 32)] {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let temp = Temp::new();
            let data = temp.data();
            let modn = dp_fixture("portable_mdx", "input.modn");
            let mut modf = dp_fixture("portable_mdx", "input.modf");
            let mut modd = dp_fixture("portable_mdx", "input.modd");
            set_word(
                if is_parent { &mut modf } else { &mut modd },
                at,
                value.to_bits(),
            );
            save(
                &data,
                "wmo.MPQ",
                &dp_entries_from(&modn, &modf, &modd, true),
            );
            let before = dp_tree(&temp.0);
            let error = prepare(&data, 7, || false).unwrap_err();
            assert!(matches!(
                error.category(),
                Error::Discovery(_) | Error::Doodad(_)
            ));
            assert!(!format!("{error:?}").contains("Solid"));
            assert_eq!(dp_tree(&temp.0), before);
        }
    }
}

#[test]
fn complete_selected_record_stream_matches_original_parent_and_child_bytes() {
    for case in [
        "portable_mdx",
        "portable_mdl",
        "portable_m2",
        "portable_nonunit",
        "portable_parent_rotation",
        "portable_composed",
        "portable_scale_zero",
        "portable_scale_negative",
        "portable_scale_signed_zero",
        "portable_zero_origin",
        "portable_signed_zero_origin",
        "portable_destructible_origin",
        "portable_destructible_nonzero",
        "portable_destructible_signed_zero",
        "portable_no_parent_collision_origin",
        "portable_no_parent_collision_nonzero",
        "portable_no_parent_collision_signed_zero",
    ] {
        let temp = Temp::new();
        let data = temp.data();
        let mut entries = dp_entries(case, !case.contains("no_parent_collision"));
        // Use the original probe's Parent.wmo basename in this authored MPQ,
        // including its listfile, group names and ADT reference. Equal length.
        for entry in &mut entries {
            entry.name = entry.name.replace("Temple", "Parent");
            let mut index = 0;
            while index + 6 <= entry.bytes.len() {
                if &entry.bytes[index..index + 6] == b"Temple" {
                    entry.bytes[index..index + 6].copy_from_slice(b"Parent");
                    index += 6;
                } else {
                    index += 1;
                }
            }
        }
        save(&data, "wmo.MPQ", &entries);
        let before = dp_tree(&temp.0);
        let prepared = prepare(&data, 7, || false).unwrap();
        let records = prepared.placement_records(dp_profile(), || false).unwrap();
        let parent = dp_fixture(case, "parent.dir");
        let child = dp_fixture(case, "child.dir");
        let expected = [parent.as_slice(), child.as_slice()].concat();
        assert_eq!(records.bytes(), expected, "{case}");
        assert_eq!(
            records.record_count(),
            usize::from(!parent.is_empty()) + usize::from(!child.is_empty())
        );
        assert_eq!(before, dp_tree(&temp.0));
        assert!(!format!("{records:?}").contains("Parent"));
    }
}

#[test]
fn record_stream_cancellation_and_changed_capture_return_no_partial_product() {
    let temp = Temp::new();
    let data = temp.data();
    let mut entries = dp_entries("portable_mdx", true);
    save(&data, "wmo.MPQ", &entries);
    let prepared = prepare(&data, 7, || false).unwrap();
    let total = std::cell::Cell::new(0);
    prepared
        .placement_records(dp_profile(), || {
            total.set(total.get() + 1);
            false
        })
        .unwrap();
    assert!(
        total.get() > 4,
        "must include source recheck and output hashing"
    );
    for cutoff in 0..total.get() {
        let polls = std::cell::Cell::new(0);
        let error = prepared
            .placement_records(dp_profile(), || {
                let n = polls.get();
                polls.set(n + 1);
                n >= cutoff
            })
            .unwrap_err();
        assert!(
            matches!(
                error,
                PlacementRecordError::Cancelled
                    | PlacementRecordError::Source(SourceError::Cancelled)
            ),
            "cutoff={cutoff}: {error:?}"
        );
    }
    let original = prepared.placement_records(dp_profile(), || false).unwrap();
    let changed = entries.iter_mut().find(|e| e.name == DP_MODEL).unwrap();
    // An ignored authored model-header byte still belongs to the captured source.
    changed.bytes[8] ^= 1;
    save(&data, "wmo.MPQ", &entries);
    assert!(matches!(
        prepared.placement_records(dp_profile(), || false),
        Err(PlacementRecordError::Source(_))
    ));
    assert!(!original.bytes().is_empty());
}
