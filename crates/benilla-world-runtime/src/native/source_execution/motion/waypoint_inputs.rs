// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1; WaypointManager.cpp:47-155,157-510,513-528.
use super::super::*;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum WaypointOrigin {
    #[default]
    None,
    Guid,
    Entry,
    External,
    WaypointPath,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceWaypointNode {
    pub position: [f32; 3],
    pub orientation: Option<f32>,
    pub delay: u32,
    pub script: u32,
}
pub type SourceWaypointPath = BTreeMap<u32, SourceWaypointNode>;
#[derive(Clone, Copy, Debug)]
pub struct SourceWaypointRow {
    pub ordinal: u64,
    pub owner: u32,
    pub path: u32,
    pub point: u32,
    pub node: SourceWaypointNode,
}
/// Loaded ScriptMgr maps, not unvalidated SQL command rows.
#[derive(Default)]
pub struct WaypointScriptMaps {
    pub movement: BTreeMap<u32, Vec<WaypointScriptStep>>,
    pub relay: BTreeSet<u32>,
    pub relay_templates: RefCell<BTreeMap<u32, Vec<u32>>>,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct WaypointScriptStep {
    pub delay: u32,
    pub command: u32,
    pub despawn_delay: u32,
    pub relay: u32,
    pub relay_template: u32,
}
impl WaypointScriptMaps {
    /// CheckDbscript preserves the outer-script reference in both relay loops.
    /// WaypointManager.cpp:102-115,130-141 never inspect the relay step itself.
    pub fn check(&self, node: &mut SourceWaypointNode) -> Result<()> {
        let Some(steps) = self.movement.get(&node.script) else {
            return Ok(()); // cpp:51-55 logs but does not clear script_id.
        };
        if node.delay != 0 {
            return Ok(());
        }
        for step in steps {
            if step.delay != 0 {
                break;
            }
            if (step.command == 18 && step.despawn_delay == 0)
                || step.command == 20
                || step.command == 25
            {
                node.delay = 1000;
                break;
            }
            if step.command == 45 && !self.relay.contains(&step.relay) && step.relay_template != 0 {
                // ScriptMgr.h:684 operator[] inserts an empty missing template.
                // The inner loops still inspect command 45 from the outer row,
                // so neither loop can set delay (cpp:102-115,130-141).
                self.relay_templates
                    .borrow_mut()
                    .entry(step.relay_template)
                    .or_default();
            }
        }
        Ok(())
    }
}
#[derive(Default)]
pub struct SourceWaypointPaths {
    guid: BTreeMap<u32, Rc<SourceWaypointPath>>,
    entry: BTreeMap<u32, Rc<SourceWaypointPath>>,
    waypoint: BTreeMap<u32, Rc<SourceWaypointPath>>,
    external: Option<BTreeMap<u32, Rc<SourceWaypointPath>>>,
}
impl SourceWaypointPaths {
    #[cfg(test)]
    pub(super) fn insert_empty_guid(&mut self, guid: u32) {
        self.guid.insert(guid, Rc::new(SourceWaypointPath::new()));
    }
    pub fn load_catalog(
        catalog: &mut crate::native::production_compiler::VerifiedCatalog,
        creatures: &BTreeSet<u32>,
        templates: &BTreeSet<u32>,
        scripts: Option<&WaypointScriptMaps>,
    ) -> Result<Self> {
        let rows = [
            catalog.execution_waypoint_rows("creature_movement")?,
            catalog.execution_waypoint_rows("creature_movement_template")?,
            catalog.execution_waypoint_rows("waypoint_path")?,
        ];
        let mut paths = Self::load(rows, creatures, templates, scripts)?;
        paths.load_scriptdev(
            catalog.execution_waypoint_rows("script_waypoint")?,
            templates,
        );
        Ok(paths)
    }
    /// SystemMgr::LoadScriptWaypoints, system.cpp:75-114. Point zero
    /// blacklists subsequent nodes, without deleting earlier registered nodes.
    pub fn load_scriptdev(&mut self, mut rows: Vec<SourceWaypointRow>, templates: &BTreeSet<u32>) {
        rows.sort_by_key(|row| (row.owner, row.path, row.point));
        let mut blacklist = BTreeSet::new();
        self.load_external(rows.into_iter().filter(|row| {
            let key = row.owner.wrapping_shl(8).wrapping_add(row.path);
            if row.point == 0 {
                blacklist.insert(key);
            }
            templates.contains(&row.owner) && !blacklist.contains(&key)
        }));
    }
    /// The three vectors are consumed in source loader order, not path order.
    /// cpp:198,329,441; waypoint_path_name only produces diagnostic messages.
    pub fn load(
        rows: [Vec<SourceWaypointRow>; 3],
        creatures: &BTreeSet<u32>,
        templates: &BTreeSet<u32>,
        scripts: Option<&WaypointScriptMaps>,
    ) -> Result<Self> {
        let mut result = Self::default();
        for (family, rows) in rows.into_iter().enumerate() {
            let mut paths = BTreeMap::<u32, SourceWaypointPath>::new();
            let mut blacklist = BTreeSet::new();
            for row in rows {
                let key = if family == 1 {
                    row.owner.wrapping_shl(8).wrapping_add(row.path)
                } else {
                    row.owner
                };
                if row.point == 0 {
                    blacklist.insert(key);
                }
                if (family == 0 && !creatures.contains(&row.owner))
                    || (family == 1 && !templates.contains(&row.owner))
                {
                    continue;
                }
                let mut node = row.node;
                // Coordinate repair can query terrain and write SQL, cpp:243-262.
                require(
                    valid_waypoint_coord(node),
                    "execution.waypoint.f6_coordinate_repair_owner",
                )?;
                if node.script != 0 {
                    scripts
                        .ok_or_else(|| {
                            refuse("execution.waypoint.scriptmgr_dbscript_loaded_maps_owner")
                        })?
                        .check(&mut node)?;
                }
                paths.entry(key).or_default().insert(row.point, node);
            }
            if family == 2 {
                // Source quirk: this erases ENTRY paths, not waypoint_path paths.
                // WaypointManager.cpp:496-498.
                for key in blacklist {
                    result.entry.remove(&key);
                }
            } else {
                for key in blacklist {
                    paths.remove(&key);
                }
            }
            let paths = paths
                .into_iter()
                .map(|(key, path)| (key, Rc::new(path)))
                .collect();
            match family {
                0 => result.guid = paths,
                1 => result.entry = paths,
                _ => result.waypoint = paths,
            }
        }
        Ok(result)
    }
    /// Called only by the owner of ScriptDev's external node registrations.
    /// External orientation 100 is retained, unlike the SQL sentinel conversion.
    pub fn load_external(&mut self, rows: impl IntoIterator<Item = SourceWaypointRow>) {
        let mut paths = BTreeMap::<u32, SourceWaypointPath>::new();
        for row in rows {
            if row.path >= 255 || !valid_waypoint_coord(row.node) {
                continue;
            }
            paths
                .entry(row.owner.wrapping_shl(8).wrapping_add(row.path))
                .or_default()
                .insert(row.point, row.node);
        }
        self.external = Some(paths.into_iter().map(|(k, p)| (k, Rc::new(p))).collect());
    }
    /// WaypointManager.h:69-127,169-178. A present empty GUID path wins.
    pub fn select(
        &self,
        entry: u32,
        guid: u32,
        path: i32,
        origin: WaypointOrigin,
    ) -> Result<(WaypointOrigin, Option<Rc<SourceWaypointPath>>)> {
        if origin == WaypointOrigin::None && path == 0 {
            for (origin, map, key) in [
                (WaypointOrigin::Guid, &self.guid, guid),
                (WaypointOrigin::Entry, &self.entry, entry.wrapping_shl(8)),
                (
                    WaypointOrigin::WaypointPath,
                    &self.waypoint,
                    entry.wrapping_shl(8),
                ),
            ] {
                if let Some(p) = map.get(&key) {
                    return Ok((origin, Some(p.clone())));
                }
            }
            return Ok((WaypointOrigin::None, None));
        }
        let origin = if origin == WaypointOrigin::None {
            WaypointOrigin::Entry
        } else {
            origin
        };
        let key = entry.wrapping_shl(8).wrapping_add(path as u32);
        let p = match origin {
            WaypointOrigin::Guid => self.guid.get(&guid),
            WaypointOrigin::Entry | WaypointOrigin::External if !(0..255).contains(&path) => None,
            WaypointOrigin::Entry => self.entry.get(&key),
            WaypointOrigin::External => self
                .external
                .as_ref()
                .ok_or_else(|| refuse("execution.waypoint.scriptdev_external_paths_owner"))?
                .get(&key),
            WaypointOrigin::WaypointPath => self.waypoint.get(&(path as u32)),
            WaypointOrigin::None => None,
        };
        Ok((origin, p.cloned()))
    }
}
fn valid_waypoint_coord(node: SourceWaypointNode) -> bool {
    // GridDefines.h:177-205 checks finite XYZ and optional orientation.
    let limit = (533.333_3_f32 * 64. / 2.) as f64 - 0.5;
    node.position[..2]
        .iter()
        .all(|v| v.is_finite() && f64::from(v.abs()) <= limit)
        && node.position[2].is_finite()
        && node.orientation.is_none_or(f32::is_finite)
}
