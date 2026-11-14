// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 TransportMgr.cpp:28-311, Transports.cpp:35-114.
// Upstream GPL-2.0-or-later; Rust translation GPL-3.0-or-later.
use super::super::production_compiler::VerifiedCatalog;
pub use super::super::production_compiler::{
    TransportAnimationRecord, TransportGameObject, TransportIntegrityRow, TransportSqlRow,
};
use super::*;
use benilla_sim::source_transports::{generate_template, Error as NumericError, TransportTemplate};
use std::{collections::BTreeMap, sync::Arc};
#[path = "transports_movement.rs"]
mod movement;
pub use movement::*;
#[path = "transports_passengers.rs"]
mod passengers;
pub use passengers::*;
#[path = "transports_elevators.rs"]
mod elevators;
pub use elevators::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SourceTransportAnimation {
    pub total_time: u32,
    pub nodes: BTreeMap<u32, TransportAnimationRecord>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum TransportTemplateEvent {
    Animation {
        entry: u32,
        time: u32,
        id: u32,
        replaced: Option<u32>,
        total_time: u32,
    },
    Generated {
        entry: u32,
        frames: usize,
        path_time: u32,
    },
    RejectedPath {
        entry: u32,
        path: u32,
    },
    RejectedRow {
        ordinal: u64,
        entry: u32,
        reason: TransportRowRejection,
    },
    AssignedPeriod {
        ordinal: u64,
        entry: u32,
        map: u32,
        period: u32,
    },
    Integrity(TransportIntegrityRow),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum TransportRowRejection {
    MissingGameObject,
    WrongType,
    MissingTemplate,
    MissingStartMap,
}
/// The executed SQLStorage/ObjectMgr owner, not a preparation result or raw-row fallback.
pub trait TransportDefinitionOwner {
    fn gameobject_definitions(&mut self) -> Result<Arc<BTreeMap<u32, TransportGameObject>>>;
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum TransportPending {
    SealedDefinitionAndSqlOccurrences,
    AllocationAndContainerHistory,
    F4LiveTransportLifecycle,
    F6Collision,
    F7Continuation,
}
pub const TRANSPORT_PENDING: &[TransportPending] = &[
    TransportPending::SealedDefinitionAndSqlOccurrences,
    TransportPending::AllocationAndContainerHistory,
    TransportPending::F4LiveTransportLifecycle,
    TransportPending::F6Collision,
    TransportPending::F7Continuation,
];
#[derive(Serialize)]
pub struct TransportTemplateInventory<'a> {
    pub version: u32,
    pub catalog_sha256: Option<&'a str>,
    pub animations_loaded: bool,
    pub templates_loaded: bool,
    pub stopped: bool,
    pub raw_gameobject_rows: usize,
    pub animations: &'a BTreeMap<u32, SourceTransportAnimation>,
    pub templates: &'a BTreeMap<u32, TransportTemplate>,
    /// Source vector order, including repeated references to a period-overridden template.
    pub by_map: &'a BTreeMap<u32, Vec<u32>>,
}
#[derive(Default)]
pub struct SourceTransportTemplates {
    catalog_sha256: Option<String>,
    animations_loaded: bool,
    templates_loaded: bool,
    stopped: bool,
    raw_gameobject_rows: usize,
    animations: BTreeMap<u32, SourceTransportAnimation>,
    templates: BTreeMap<u32, TransportTemplate>,
    by_map: BTreeMap<u32, Vec<u32>>,
}
fn numeric(error: NumericError) -> Refusal {
    let reason = match error {
        NumericError::SourceUndefined(site) => format!("source_undefined.{site}"),
        NumericError::NumericDomain => "numeric_domain".into(),
        NumericError::MissingMap(map) => format!("source_undefined.map_lookup_null.{map}"),
        NumericError::CrossMapInstance => "cross_map_instance_assertion".into(),
    };
    refuse(&format!("execution.transport.{reason}"))
}
impl SourceTransportTemplates {
    pub fn ready(&self) -> Result<()> {
        require(
            self.templates_loaded && !self.stopped,
            "execution.transport.templates_owner",
        )
    }
    pub fn template(&self, entry: u32) -> Result<Option<&TransportTemplate>> {
        self.ready()?;
        Ok(self.templates.get(&entry))
    }
    pub fn animation(&self, entry: u32) -> Result<Option<&SourceTransportAnimation>> {
        require(
            self.animations_loaded && !self.stopped,
            "execution.transport.animation_owner",
        )?;
        Ok(self.animations.get(&entry))
    }
    pub fn by_map(&self) -> Result<&BTreeMap<u32, Vec<u32>>> {
        self.ready()?;
        Ok(&self.by_map)
    }
    pub fn inventory(&self) -> TransportTemplateInventory<'_> {
        TransportTemplateInventory {
            version: 1,
            catalog_sha256: self.catalog_sha256.as_deref(),
            animations_loaded: self.animations_loaded,
            templates_loaded: self.templates_loaded,
            stopped: self.stopped,
            raw_gameobject_rows: self.raw_gameobject_rows,
            animations: &self.animations,
            templates: &self.templates,
            by_map: &self.by_map,
        }
    }
    fn bind_catalog(&mut self, identity: &str) -> Result<()> {
        require(
            self.catalog_sha256
                .as_deref()
                .is_none_or(|old| old == identity),
            "execution.transport.catalog_binding",
        )?;
        self.catalog_sha256 = Some(identity.into());
        Ok(())
    }
    /// World.cpp:970 precedes the separate LoadTransports call at :1127.
    pub fn load_animations_from_catalog(
        &mut self,
        catalog: &mut VerifiedCatalog,
        observe: &mut impl FnMut(TransportTemplateEvent) -> Result<()>,
    ) -> Result<()> {
        let result = (|| {
            let (identity, records) = catalog.execution_transport_animations()?;
            self.bind_catalog(&identity)?;
            self.load_animations(records, observe)
        })();
        if result.is_err() {
            self.stopped = true;
        }
        result
    }
    pub fn load_animations(
        &mut self,
        records: impl IntoIterator<Item = TransportAnimationRecord>,
        observe: &mut impl FnMut(TransportTemplateEvent) -> Result<()>,
    ) -> Result<()> {
        require(
            !self.stopped && !self.animations_loaded,
            "execution.transport.animation_load_state",
        )?;
        let result = (|| {
            // Effective DBC IDs retain the final physical winner, then iterate by ID, not ordinal/time.
            let mut effective = BTreeMap::<u32, TransportAnimationRecord>::new();
            for record in records {
                if let Some(old) = effective.get(&record.id) {
                    require(
                        old.ordinal != record.ordinal || *old == record,
                        "execution.transport.animation_occurrence",
                    )?;
                    if old.ordinal > record.ordinal {
                        continue;
                    }
                }
                effective.insert(record.id, record);
            }
            for record in effective.into_values() {
                let animation = self.animations.entry(record.entry).or_insert_with(|| {
                    SourceTransportAnimation {
                        total_time: 0,
                        nodes: BTreeMap::new(),
                    }
                });
                animation.total_time = animation.total_time.max(record.time);
                let (entry, time, id) = (record.entry, record.time, record.id);
                let replaced = animation.nodes.insert(time, record).map(|old| old.id);
                observe(TransportTemplateEvent::Animation {
                    entry,
                    time,
                    id,
                    replaced,
                    total_time: animation.total_time,
                })?;
            }
            self.animations_loaded = true;
            Ok(())
        })();
        if result.is_err() {
            self.stopped = true;
        }
        result
    }
    /// Checked F2 raw inputs do not replace the mandatory executed definition owner.
    pub fn load_templates_from_catalog(
        &mut self,
        catalog: &mut VerifiedCatalog,
        occurrences: &Path,
        owner: &mut impl TransportDefinitionOwner,
        observe: &mut impl FnMut(TransportTemplateEvent) -> Result<()>,
    ) -> Result<()> {
        require(
            !self.stopped && !self.templates_loaded,
            "execution.transport.template_load_state",
        )?;
        let result = (|| {
            let inputs = catalog.execution_transport_inputs(occurrences)?;
            self.bind_catalog(&inputs.catalog_sha256)?;
            self.raw_gameobject_rows = inputs.raw_gameobjects.len();
            let definitions = owner.gameobject_definitions()?;
            for (&entry, data) in definitions.iter() {
                if entry == 0 || data.kind != 15 {
                    continue;
                }
                require(
                    data.entry == entry && data.movement.entry == entry,
                    "execution.transport.definition_binding",
                )?;
                if data.movement.path >= inputs.path_count {
                    observe(TransportTemplateEvent::RejectedPath {
                        entry,
                        path: data.movement.path,
                    })?;
                    continue;
                }
                let mut nodes = Vec::new();
                if let Some(path) = inputs.paths.get(&data.movement.path) {
                    nodes.reserve_exact(path.len());
                    for node in path.iter() {
                        nodes.push(node.ok_or_else(|| {
                            refuse("execution.transport.source_undefined.null_taxi_node")
                        })?);
                    }
                }
                let template = generate_template(data.movement, Arc::from(nodes), &inputs.maps)
                    .map_err(numeric)?;
                observe(TransportTemplateEvent::Generated {
                    entry,
                    frames: template.key_frames.len(),
                    path_time: template.path_time,
                })?;
                self.templates.insert(entry, template);
            }
            self.assign_periods(
                &definitions,
                &inputs.maps,
                &inputs.rows,
                &inputs.integrity_rows,
                observe,
            )?;
            self.templates_loaded = true;
            Ok(())
        })();
        if result.is_err() {
            self.stopped = true;
        }
        result
    }
    fn assign_periods(
        &mut self,
        definitions: &BTreeMap<u32, TransportGameObject>,
        maps: &BTreeMap<u32, u32>,
        rows: &[TransportSqlRow],
        integrity: &[TransportIntegrityRow],
        observe: &mut impl FnMut(TransportTemplateEvent) -> Result<()>,
    ) -> Result<()> {
        for row in rows {
            let rejection = if let Some(data) = definitions.get(&row.entry) {
                if data.kind != 15 {
                    Some(TransportRowRejection::WrongType)
                } else if !self.templates.contains_key(&row.entry) {
                    Some(TransportRowRejection::MissingTemplate)
                } else {
                    None
                }
            } else {
                Some(TransportRowRejection::MissingGameObject)
            };
            if let Some(reason) = rejection {
                observe(TransportTemplateEvent::RejectedRow {
                    ordinal: row.ordinal,
                    entry: row.entry,
                    reason,
                })?;
                continue;
            }
            let template = self.templates.get_mut(&row.entry).unwrap();
            let map = template.nodes[template.key_frames[0].node].map;
            if !maps.contains_key(&map) {
                observe(TransportTemplateEvent::RejectedRow {
                    ordinal: row.ordinal,
                    entry: row.entry,
                    reason: TransportRowRejection::MissingStartMap,
                })?;
                continue;
            }
            template.override_period(row.period).map_err(numeric)?;
            self.by_map.entry(map).or_default().push(row.entry);
            observe(TransportTemplateEvent::AssignedPeriod {
                ordinal: row.ordinal,
                entry: row.entry,
                map,
                period: row.period,
            })?;
        }
        // Source returns before the integrity query when the first query has no rows.
        if !rows.is_empty() {
            for row in integrity {
                observe(TransportTemplateEvent::Integrity(row.clone()))?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "manager_dispatch_tests.rs"]
mod manager_dispatch_tests;
#[cfg(test)]
#[path = "transports_tests.rs"]
mod tests;
