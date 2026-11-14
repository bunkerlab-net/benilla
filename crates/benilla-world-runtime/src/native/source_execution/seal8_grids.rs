// SPDX-License-Identifier: GPL-3.0-or-later
//! Retained member arguments are component fixtures, never native load triggers.
use super::retained_seal8::{parse, scan_seal8, SEAL8_TRACE_HASH};
use super::*;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Serialize)]
pub struct Seal8GridReport {
    pub adapter: &'static str,
    pub trace_sha256: Sha256Id,
    pub records: u64,
    pub grid_creations_matched: u64,
    pub object_load_branches_matched: u64,
    pub converted_product_requests_matched: u64,
    pub forced_unload_visits_matched: u64,
    pub product_requests_observed: u64,
    pub additional_product_requests: u64,
    pub pending: Vec<GridPending>,
    pub reference_execution_sealed: bool,
    pub whole_execution_accepted: bool,
}
fn compare(expected: GridProjectionOperation, actual: GridProjectionOperation) -> Result<()> {
    DifferentialOracle::compare_grid_projection(expected, actual).map_err(|e| e.reason)
}
pub fn compare_seal8_unsealed_grids(root: &Path) -> Result<Seal8GridReport> {
    let mut maps = BTreeMap::<(u32, u32), SourceGrids>::new();
    let mut awaiting = BTreeMap::<u64, GridIdentity>::new();
    let mut products = BTreeMap::<(u32, u16, u16), MapIdentity>::new();
    let mut observed_products = BTreeSet::new();
    let mut cursors = BTreeMap::<MapIdentity, Option<GridIdentity>>::new();
    let (mut creates, mut loads, mut unloads, mut product_observations) = (0, 0, 0, 0);
    let records = scan_seal8(root, |context, kind, payload| {
        let mut fields = payload.split_whitespace();
        match kind {
            "logical_birth" => {
                let generation = parse(fields.next())?;
                if fields.next() == Some("map") {
                    require(maps.len() < 2, "execution.seal8_grid_map_births")?;
                    // Continents are constructed in map 0, then map 1 order; MapManager.cpp:96-106.
                    let map = MapIdentity {
                        map: maps.len() as u32,
                        instance: 0,
                        creation_generation: generation,
                    };
                    maps.insert((map.map, map.instance), SourceGrids::new(map)?);
                }
            }
            "grid_create" => {
                let key = (parse(fields.next())?, parse(fields.next())?);
                let (x, y) = (parse(fields.next())?, parse(fields.next())?);
                let grids = maps
                    .get_mut(&key)
                    .ok_or_else(|| refuse("execution.seal8_grid_map"))?;
                require(
                    !cursors.contains_key(&grids.map()) && grids.identity(x, y).is_none(),
                    "execution.seal8_grid_creation",
                )?;
                let grid = grids.create(x, y)?;
                require(
                    awaiting.insert(context.thread, grid).is_none(),
                    "execution.seal8_grid_initial_state",
                )?;
                let target = grid.product_coordinates();
                products.insert((grid.map.map, target.x, target.y), grid.map);
            }
            "grid_state" => {
                let _pointer = fields
                    .next()
                    .ok_or_else(|| refuse("execution.seal8_grid_state"))?;
                let (x, y): (u16, u16) = (parse(fields.next())?, parse(fields.next())?);
                let (previous, current): (u8, u8) = (parse(fields.next())?, parse(fields.next())?);
                if previous == 0 {
                    let grid = awaiting
                        .remove(&context.thread)
                        .ok_or_else(|| refuse("execution.seal8_grid_initial_state"))?;
                    let grids = &maps[&(grid.map.map, grid.map.instance)];
                    compare(
                        GridProjectionOperation::InitialState {
                            map: grid.map,
                            x,
                            y,
                            phase: current,
                        },
                        GridProjectionOperation::InitialState {
                            map: grid.map,
                            x: grid.x,
                            y: grid.y,
                            phase: grids.phase(grid)? as u8,
                        },
                    )?;
                    creates += 1;
                }
            }
            "grid_load" => {
                let key = (parse(fields.next())?, parse(fields.next())?);
                let (x, y) = (parse(fields.next())?, parse(fields.next())?);
                let grids = maps
                    .get_mut(&key)
                    .ok_or_else(|| refuse("execution.seal8_grid_map"))?;
                let grid = grids
                    .identity(x, y)
                    .ok_or_else(|| refuse("execution.seal8_grid_not_created"))?;
                compare(
                    GridProjectionOperation::ObjectData {
                        map: grid.map,
                        x,
                        y,
                        loaded_before: false,
                    },
                    GridProjectionOperation::ObjectData {
                        map: grid.map,
                        x,
                        y,
                        loaded_before: grids.object_data_loaded(x, y),
                    },
                )?;
                grids.mark_objects(grid)?;
                loads += 1;
            }
            "vmap_load" => {
                let key = (
                    parse::<u32>(fields.next())?,
                    parse::<u16>(fields.next())?,
                    parse::<u16>(fields.next())?,
                );
                require(
                    key.1 < 64 && key.2 < 64,
                    "execution.seal8_product_coordinates",
                )?;
                observed_products.insert(key);
                product_observations += 1;
            }
            "grid_unload" => {
                let map: u32 = parse(fields.next())?;
                let (x, y) = (parse(fields.next())?, parse(fields.next())?);
                let force: u32 = parse(fields.next())?;
                require(
                    force == 1,
                    "execution.seal8_nonforced_unload_needs_completion",
                )?;
                let grids = maps
                    .get(&(map, 0))
                    .ok_or_else(|| refuse("execution.seal8_grid_map"))?;
                let cursor = cursors
                    .entry(grids.map())
                    .or_insert_with(|| grids.linked().next());
                let actual = cursor.ok_or_else(|| refuse("execution.seal8_grid_unload_cursor"))?;
                compare(
                    GridProjectionOperation::ForcedUnloadVisit {
                        map: grids.map(),
                        x,
                        y,
                    },
                    GridProjectionOperation::ForcedUnloadVisit {
                        map: actual.map,
                        x: actual.x,
                        y: actual.y,
                    },
                )?;
                // UnloadAll advances before calling UnloadGrid; this does not certify deletion; Map.cpp:1322-1326.
                *cursor = grids.next_linked(actual)?;
                unloads += 1;
            }
            _ => {}
        }
        Ok(())
    })?;
    let mut matched_products = 0;
    for (&(map, x, y), &identity) in &products {
        let &(actual_map, actual_x, actual_y) = observed_products
            .get(&(map, x, y))
            .ok_or_else(|| refuse("execution.seal8_grid_product_association"))?;
        require(
            actual_map == identity.map,
            "execution.seal8_grid_product_map",
        )?;
        compare(
            GridProjectionOperation::ProductRequest {
                map: identity,
                x: actual_x,
                y: actual_y,
            },
            GridProjectionOperation::ProductRequest {
                map: identity,
                x,
                y,
            },
        )?;
        matched_products += 1;
    }
    require(
        creates == 34
            && loads == 30
            && unloads == 34
            && matched_products == 34
            && product_observations == 35
            && awaiting.is_empty()
            && cursors.values().all(Option::is_none),
        "execution.seal8_grid_exhaustion",
    )?;
    Ok(Seal8GridReport {
        adapter: "benilla-seal8-08-unsealed-grid-fixtures-v1",
        trace_sha256: Sha256Id::parse(SEAL8_TRACE_HASH)?,
        records,
        grid_creations_matched: creates,
        object_load_branches_matched: loads,
        converted_product_requests_matched: matched_products,
        forced_unload_visits_matched: unloads,
        product_requests_observed: product_observations,
        additional_product_requests: product_observations - matched_products,
        pending: GRID_PENDING.into(),
        reference_execution_sealed: false,
        whole_execution_accepted: false,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retained_seal8_grid_projection_or_clean_absence() {
        let explicit = std::env::var_os("BENILLA_F1_SEAL8_FRAMING");
        let path = explicit
            .clone()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .ancestors()
                    .nth(2)
                    .unwrap()
                    .join(
                        "benilla-config/full-server/f1/sealing/source-choice/seal8-full-framing-08",
                    )
            });
        if explicit.is_none() && !path.try_exists().unwrap() {
            eprintln!("SKIP seal8-08 grid projection: set BENILLA_F1_SEAL8_FRAMING");
            return;
        }
        let report = compare_seal8_unsealed_grids(&path).unwrap();
        assert_eq!(
            (
                report.grid_creations_matched,
                report.object_load_branches_matched,
                report.converted_product_requests_matched,
                report.forced_unload_visits_matched
            ),
            (34, 30, 34, 34)
        );
        assert_eq!(report.additional_product_requests, 1);
        assert!(!report.reference_execution_sealed && !report.whole_execution_accepted);
        println!("{}", serde_json::to_string(&report).unwrap());
    }
    #[test]
    fn grid_projection_preserves_first_field_difference() {
        let map = MapIdentity {
            map: 0,
            instance: 0,
            creation_generation: 7,
        };
        let expected = GridProjectionOperation::ProductRequest { map, x: 63, y: 0 };
        let actual = GridProjectionOperation::ProductRequest { map, x: 0, y: 63 };
        let error = DifferentialOracle::compare_grid_projection(expected, actual).unwrap_err();
        assert_eq!(error.expected, expected);
        assert_eq!(error.actual, actual);
    }
}
