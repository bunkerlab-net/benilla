// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1; WaypointManager.cpp:157-510.
use super::*;
use crate::native::production_compiler::VerifiedCatalog;
use benilla_sim::source_rng::{RawGenerator, SourceRandom, TraceEvent};

pub const WAYPOINT_MANAGER_QUERIES: [&str; 7] = [
    "SELECT Id, COUNT(Point) FROM creature_movement GROUP BY Id",
    "SELECT Id, Point, PositionX, PositionY, PositionZ, Orientation, WaitTime, ScriptId FROM creature_movement",
    "SELECT Entry, COUNT(Point) FROM creature_movement_template GROUP BY Entry",
    "SELECT Entry, PathId, Point, PositionX, PositionY, PositionZ, Orientation, WaitTime, ScriptId FROM creature_movement_template",
    "SELECT PathId, COUNT(Point) FROM waypoint_path GROUP BY PathId",
    "SELECT PathId FROM waypoint_path_name",
    "SELECT PathId, Point, PositionX, PositionY, PositionZ, Orientation, WaitTime, ScriptId FROM waypoint_path",
];

/// Error-log branches retain their source line and operands in execution order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WaypointLoadNotice {
    pub line: u32,
    pub owner: u32,
    pub point: u32,
    pub path: u32,
    pub script: u32,
}
#[derive(Default, Debug)]
pub struct WaypointLoadReport {
    pub statements: Vec<String>,
    pub notices: Vec<WaypointLoadNotice>,
    pub rows: [usize; 3],
    pub missing_owner: [usize; 3],
    pub blacklisted_nodes: [usize; 3],
    pub paths: [usize; 3],
    pub nodes: [usize; 3],
}
impl WaypointLoadReport {
    pub(crate) fn notice(&mut self, line: u32, row: SourceWaypointRow) {
        self.notices.push(WaypointLoadNotice {
            line,
            owner: row.owner,
            point: row.point,
            path: row.path,
            script: row.node.script,
        });
    }
}
/// ObjectMgr's loaded containers, plus the F6 repair calls made only for bad XYZ.
pub trait WaypointLoadOwner {
    fn creature(&mut self, guid: u32) -> Result<Option<(u32, u32)>>;
    fn template(&mut self, entry: u32) -> Result<Option<u32>>;
    fn repair_guid(
        &mut self,
        _: SourceWaypointRow,
        _: &mut SourceWaypointNode,
        _: &mut WaypointLoadReport,
    ) -> Result<()> {
        Err(refuse("execution.waypoint.f6_coordinate_repair_owner"))
    }
}
struct Owner<'a, 'b, E, S> {
    catalog: &'a mut VerifiedCatalog,
    spawns: &'a SourceEffectiveSpawns,
    templates: &'a LoadedCreatureTemplates,
    geometry: &'a mut SourceTerrainManager,
    random: &'a mut SourceRandom<'b, E, S>,
    repair_spawns: Option<BTreeMap<u32, [u32; 2]>>,
}
impl<E: RawGenerator, S: FnMut(TraceEvent)> WaypointLoadOwner for Owner<'_, '_, E, S> {
    fn creature(&mut self, guid: u32) -> Result<Option<(u32, u32)>> {
        // GetCreatureData returns the ObjectMgr container, including post-insert skips.
        Ok(self
            .spawns
            .pre_terrain_data(SpawnKey {
                kind: GridSpawnKind::Creature,
                guid,
            })?
            .map(|data| (data.row.entry, u32::from(data.row.movement))))
    }
    fn template(&mut self, entry: u32) -> Result<Option<u32>> {
        Ok(self.templates.get(entry).map(|info| info.words[79]))
    }
    fn repair_guid(
        &mut self,
        row: SourceWaypointRow,
        node: &mut SourceWaypointNode,
        report: &mut WaypointLoadReport,
    ) -> Result<()> {
        // WaypointManager.cpp:246 uses raw SQL, not ObjectMgr's effective entry.
        report.statements.push(format!(
            "SELECT id, map FROM creature WHERE guid = '{}'",
            row.owner
        ));
        if self.repair_spawns.is_none() {
            self.repair_spawns = Some(self.catalog.execution_waypoint_repair_spawns()?);
        }
        let spawn = self
            .repair_spawns
            .as_ref()
            .unwrap()
            .get(&row.owner)
            .copied();
        report.notice(if spawn.is_some() { 248 } else { 251 }, row);
        normalize_waypoint_xy(node);
        if let Some([_, map]) = spawn {
            self.geometry.load_terrain(map, self.random)?;
            node.position[2] = self.geometry.height_static(map, node.position, true, 10.)?;
        }
        report.statements.push(format!("UPDATE creature_movement SET PositionX = '{}', PositionY = '{}', PositionZ = '{}' WHERE Id = '{}' AND Point = '{}'", WaypointSqlFloat(node.position[0]), WaypointSqlFloat(node.position[1]), WaypointSqlFloat(node.position[2]), row.owner, row.point));
        Ok(())
    }
}
impl SourceWaypointPaths {
    /// World.cpp:1248 binds the complete loaded ObjectMgr and ScriptMgr owners.
    pub fn load_startup_catalog<E: RawGenerator, S: FnMut(TraceEvent)>(
        catalog: &mut VerifiedCatalog,
        spawns: &SourceEffectiveSpawns,
        templates: &LoadedCreatureTemplates,
        scripts: &SourceDbScriptMaps,
        geometry: &mut SourceTerrainManager,
        random: &mut SourceRandom<'_, E, S>,
    ) -> Result<Self> {
        let projection = scripts.waypoint_maps()?;
        let mut rows = [
            catalog.execution_waypoint_rows("creature_movement")?,
            catalog.execution_waypoint_rows("creature_movement_template")?,
            catalog.execution_waypoint_rows("waypoint_path")?,
        ];
        // Owner's primary-key rule; pre-start-classicmangos.sql:335044,388541,861620.
        for rows in &mut rows {
            rows.sort_unstable_by_key(|row| (row.owner, row.path, row.point));
        }
        let names = if rows[2].is_empty() {
            BTreeSet::new()
        } else {
            catalog.execution_waypoint_names()?
        };
        let mut owner = Owner {
            catalog,
            spawns,
            templates,
            geometry,
            random,
            repair_spawns: None,
        };
        Self::load_manager(rows, names, &mut owner, Some(&projection))
    }
    /// Rows arrive in the snapshot primary-key order. Point-zero blacklists
    /// apply after a whole table, including nodes already loaded for that key.
    pub fn load_manager(
        rows: [Vec<SourceWaypointRow>; 3],
        mut names: BTreeSet<u32>,
        owner: &mut impl WaypointLoadOwner,
        scripts: Option<&WaypointScriptMaps>,
    ) -> Result<Self> {
        let mut result = Self::default();
        let mut unused = scripts
            .map(|s| s.movement.keys().copied().collect::<BTreeSet<_>>())
            .unwrap_or_default();
        for (family, rows) in rows.into_iter().enumerate() {
            let report = &mut result.report;
            report.rows[family] = rows.len();
            report
                .statements
                .push(WAYPOINT_MANAGER_QUERIES[family * 2].into());
            if rows.is_empty() {
                continue;
            }
            if family == 2 {
                report.statements.push(WAYPOINT_MANAGER_QUERIES[5].into());
            }
            report.statements.push(
                WAYPOINT_MANAGER_QUERIES[if family == 2 { 6 } else { family * 2 + 1 }].into(),
            );
            let mut paths = BTreeMap::<u32, SourceWaypointPath>::new();
            let mut blacklist = BTreeSet::new();
            let mut wrong_motion = BTreeSet::new();
            for row in rows {
                let key = if family == 1 {
                    row.owner.wrapping_shl(8).wrapping_add(row.path)
                } else {
                    row.owner
                };
                let template = if family == 1 {
                    Some(owner.template(row.owner)?)
                } else {
                    None
                };
                if row.point == 0 {
                    blacklist.insert(key);
                    report.notice([218, 348, 457][family], row);
                }
                if family == 0 {
                    let Some((_, motion)) = owner.creature(row.owner)? else {
                        report.missing_owner[family] += 1;
                        report.notice(225, row);
                        continue;
                    };
                    if motion != 2 && motion != 4 {
                        wrong_motion.insert(row.owner);
                    }
                } else if family == 1 && template == Some(None) {
                    report.missing_owner[family] += 1;
                    report.notice(353, row);
                    continue;
                }
                if family == 2 && names.insert(row.owner) {
                    report.notice(463, row);
                }
                let mut node = row.node;
                if !valid_waypoint_coord(node) {
                    if family == 0 {
                        owner.repair_guid(row, &mut node, report)?;
                    } else {
                        report.notice(if family == 1 { 371 } else { 480 }, row);
                        normalize_waypoint_xy(&mut node);
                        report.notice(if family == 1 { 377 } else { 486 }, row);
                        report.statements.push(if family == 1 {
                            format!("UPDATE creature_movement_template SET PositionX = '{}', PositionY = '{}' WHERE Entry = {} AND Point = {} AND PathId = {}", WaypointSqlFloat(node.position[0]), WaypointSqlFloat(node.position[1]), row.owner, row.point, row.path)
                        } else {
                            format!("UPDATE waypoint_path SET PositionX = '{}', PositionY = '{}' WHERE PathId = {} AND Point = {}", WaypointSqlFloat(node.position[0]), WaypointSqlFloat(node.position[1]), row.owner, row.point)
                        });
                    }
                }
                if node.script != 0 {
                    let scripts = scripts.ok_or_else(|| {
                        refuse("execution.waypoint.scriptmgr_dbscript_loaded_maps_owner")
                    })?;
                    let notice = scripts.check_with_notice(&mut node)?;
                    if notice != Some(54) {
                        unused.remove(&node.script);
                    }
                    if let Some(line) = notice {
                        report.notice(line, row);
                    }
                }
                paths.entry(key).or_default().insert(row.point, node);
            }
            for key in blacklist {
                if family == 2 {
                    // Source erases the wrong container, cpp:496-498.
                    if let Some(path) = result.entry.remove(&key) {
                        report.blacklisted_nodes[1] += path.len();
                    }
                } else if let Some(path) = paths.remove(&key) {
                    report.blacklisted_nodes[family] += path.len();
                }
            }
            for guid in wrong_motion {
                let (entry, _) = owner.creature(guid)?.ok_or_else(|| {
                    refuse("execution.waypoint.WaypointManager_279.null_creature_data_undefined")
                })?;
                let row = SourceWaypointRow {
                    ordinal: 0,
                    owner: guid,
                    path: entry,
                    point: 0,
                    node: SourceWaypointNode {
                        position: [0.; 3],
                        orientation: None,
                        delay: 0,
                        script: 0,
                    },
                };
                match owner.template(entry)? {
                    None => report.notice(283, row),
                    Some(motion) => {
                        report.notice(287, row);
                        if motion == 2 || motion == 4 {
                            report.notice(290, row);
                        }
                    }
                }
            }
            let paths = paths.into_iter().map(|(k, p)| (k, Rc::new(p))).collect();
            match family {
                0 => result.guid = paths,
                1 => result.entry = paths,
                _ => result.waypoint = paths,
            }
        }
        for script in unused {
            result.report.notices.push(WaypointLoadNotice {
                line: 507,
                owner: script,
                point: 0,
                path: 0,
                script,
            });
        }
        for (family, paths) in [&result.guid, &result.entry, &result.waypoint]
            .into_iter()
            .enumerate()
        {
            result.report.paths[family] = paths.len();
            result.report.nodes[family] = paths.values().map(|p| p.len()).sum();
        }
        Ok(result)
    }
}
pub(crate) fn normalize_waypoint_xy(node: &mut SourceWaypointNode) {
    // GridDefines.h:169-175. NaN survives both comparisons.
    let limit = (533.333_3_f32 * 64. / 2.) as f64 - 0.5;
    for value in &mut node.position[..2] {
        if f64::from(*value) > limit {
            *value = limit as f32;
        } else if f64::from(*value) < -limit {
            *value = -limit as f32;
        }
    }
}

/// PExecute's `%f` promotes float to double and retains signed lowercase NaN.
pub(crate) struct WaypointSqlFloat(pub f32);
impl std::fmt::Display for WaypointSqlFloat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0.is_nan() {
            f.write_str(if self.0.is_sign_negative() {
                "-nan"
            } else {
                "nan"
            })
        } else {
            write!(f, "{:.6}", f64::from(self.0))
        }
    }
}

#[cfg(test)]
#[path = "waypoint_manager_tests.rs"]
mod tests;
