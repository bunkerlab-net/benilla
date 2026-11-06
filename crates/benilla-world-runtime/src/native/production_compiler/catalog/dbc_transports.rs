// SPDX-License-Identifier: GPL-3.0-or-later
// Typed retained inputs for TransportMgr.cpp and DBCStores.cpp:484-508.
use super::*;
use benilla_sim::source_transports::{MoTransport, TaxiNode};
use serde::Serialize;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TransportGameObject {
    pub entry: u32,
    pub kind: u32,
    pub display: u32,
    pub movement: MoTransport,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TransportAnimationRecord {
    pub id: u32,
    pub ordinal: u32,
    pub entry: u32,
    pub time: u32,
    pub position: [u32; 3],
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TransportSqlRow {
    pub ordinal: u64,
    pub entry: u32,
    pub name: Vec<u8>,
    pub period: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TransportIntegrityRow {
    pub ordinal: u64,
    pub guid: u32,
    pub entry: u32,
    pub name: Vec<u8>,
}
pub(crate) struct TransportCatalogInputs {
    pub catalog_sha256: String,
    pub maps: BTreeMap<u32, u32>,
    pub path_count: u32,
    pub paths: BTreeMap<u32, Arc<[Option<TaxiNode>]>>,
    /// Raw SQLStorage operands only. An actual ObjectMgr owner must supply executable definitions.
    pub raw_gameobjects: BTreeMap<u32, TransportGameObject>,
    pub rows: Vec<TransportSqlRow>,
    pub integrity_rows: Vec<TransportIntegrityRow>,
}
fn effective_words(catalog: &mut VerifiedCatalog, file: &str) -> Result<Vec<(u32, Vec<u32>)>> {
    let mut view = catalog.dbc_view(file)?;
    let ids = std::mem::take(&mut view.by_id);
    ids.into_values()
        .map(|ordinal| Ok((ordinal, view.words(ordinal)?)))
        .collect()
}
impl VerifiedCatalog {
    pub(crate) fn execution_transport_animations(
        &mut self,
    ) -> Result<(String, Vec<TransportAnimationRecord>)> {
        let records = effective_words(self, "TransportAnimation.dbc")?
            .into_iter()
            .map(|(ordinal, w)| TransportAnimationRecord {
                id: w[0],
                ordinal,
                entry: w[1],
                time: w[2],
                position: [w[3], w[4], w[5]],
            })
            .collect();
        Ok((
            self.manifest["catalog_sha256"]
                .as_str()
                .ok_or_else(|| fail("catalog.schema"))?
                .into(),
            records,
        ))
    }
    /// Phase0 checked inputs, not sealed F1 SQL or effective-ObjectMgr occurrence acceptance.
    pub(crate) fn execution_transport_inputs(
        &mut self,
        occurrence_root: &Path,
    ) -> Result<TransportCatalogInputs> {
        let maps = effective_words(self, "Map.dbc")?
            .into_iter()
            .map(|(_, w)| (w[0], w[2]))
            .collect();
        let taxi = effective_words(self, "TaxiPath.dbc")?;
        let path_count = taxi
            .iter()
            .map(|(_, w)| w[0])
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| fail("transport.path_count_overflow"))?;
        let mut node_slots = BTreeMap::<u32, BTreeMap<u32, TaxiNode>>::new();
        for (_, w) in effective_words(self, "TaxiPathNode.dbc")? {
            if w[0] == 0 {
                continue;
            }
            check(w[1] < path_count, "transport.node_path_out_of_bounds")?;
            check(w[2] < 100_000, "transport.node_index_budget")?;
            node_slots.entry(w[1]).or_default().insert(
                w[2],
                TaxiNode {
                    id: w[0],
                    path: w[1],
                    index: w[2],
                    map: w[3],
                    position: [w[4], w[5], w[6]],
                    flags: w[7],
                    delay: w[8],
                },
            );
        }
        let mut paths = BTreeMap::new();
        let mut slot_bytes = 0;
        for (path, slots) in node_slots {
            check(path != 0, "transport.source_undefined.path_zero_set")?;
            let count = *slots.last_key_value().unwrap().0 as usize + 1;
            slot_bytes += count * std::mem::size_of::<Option<TaxiNode>>();
            check(slot_bytes <= DECODED, "transport.node_slot_budget")?;
            let mut nodes = vec![None; count];
            for (index, node) in slots {
                nodes[index as usize] = Some(node);
            }
            paths.insert(path, Arc::from(nodes));
        }
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        let columns = spawns::columns(&schema, "gameobject_template")?;
        let positions = ["entry", "type", "displayid", "data0", "data1", "data2"]
            .map(|name| spawns::position(&columns, name))
            .into_iter()
            .collect::<Result<Vec<_>>>()?;
        let mut raw_gameobjects = BTreeMap::new();
        self.scan("gameobject_template", 0, 256, |row| {
            let mut cells = [None; 256];
            spawns::fill(&row, &mut cells)?;
            let mut w = [0; 6];
            for (out, index) in w.iter_mut().zip(&positions) {
                *out = spawns::uint(cells[*index])?;
            }
            raw_gameobjects.insert(
                w[0],
                TransportGameObject {
                    entry: w[0],
                    kind: w[1],
                    display: w[2],
                    movement: MoTransport {
                        entry: w[0],
                        path: w[3],
                        speed: w[4],
                        acceleration: w[5],
                    },
                },
            );
            Ok(())
        })?;
        let report = occurrences::verify(self, &schema, occurrence_root)?;
        let mut rows = Vec::new();
        let mut integrity_rows = Vec::new();
        for stream in 0..2 {
            let identity = &report[stream.to_string()];
            let held =
                reader::Held::open(&occurrence_root.join(format!("{stream}.rows")), MAX_RAW)?;
            let mut buffer = vec![0; FRAME];
            let mut at = 0;
            let mut hash = Sha256::new();
            for ordinal in 0..number(&identity["rows"])? {
                let row =
                    super::super::groups::read_stream_row(&held, &mut buffer, &mut at, ordinal, 3)?;
                hash.update(row.frame);
                let mut cells = [None; 256];
                spawns::fill(&row, &mut cells)?;
                let a = spawns::uint(cells[0])?;
                if stream == 0 {
                    rows.push(TransportSqlRow {
                        ordinal,
                        entry: a,
                        name: cells[1].unwrap_or_default().to_vec(),
                        period: spawns::uint(cells[2])?,
                    });
                } else {
                    integrity_rows.push(TransportIntegrityRow {
                        ordinal,
                        guid: a,
                        entry: spawns::uint(cells[1])?,
                        name: cells[2].unwrap_or_default().to_vec(),
                    });
                }
            }
            held.check()?;
            check(
                at == held.len()
                    && identity["occurrence_sha256"] == format!("{:x}", hash.finalize()),
                "transport.changed_occurrence",
            )?;
        }
        Ok(TransportCatalogInputs {
            catalog_sha256: self.manifest["catalog_sha256"]
                .as_str()
                .ok_or_else(|| fail("catalog.schema"))?
                .into(),
            maps,
            path_count,
            paths,
            raw_gameobjects,
            rows,
            integrity_rows,
        })
    }
}
