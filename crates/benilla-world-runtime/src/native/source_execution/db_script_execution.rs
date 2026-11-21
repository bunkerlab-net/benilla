// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! Map.cpp:2166-2276; ScriptMgr.cpp:1309-1764. Component-only, no admission or F7 restore.
use super::*;
use std::collections::BTreeMap;

#[path = "db_script_commands.rs"]
mod commands;
#[path = "db_script_targets.rs"]
mod targets;
pub use commands::*;
pub use targets::*;
#[path = "db_script_map_end.rs"]
mod map_end;
pub use map_end::*;
#[cfg(test)]
#[path = "db_script_execution_census.rs"]
mod census;
#[cfg(test)]
#[path = "db_script_execution_tests.rs"]
mod tests;

pub(super) fn script_missing(owner: &str) -> Refusal {
    refuse(&format!("execution.db_script_executor.{owner}_owner"))
}

/// Map.h:233-240, ScriptMgr.h:626-631. Absent GUIDs are wildcards for uniqueness/termination.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct DbScriptInvocation {
    pub kind: Option<DbScriptType>,
    pub id: u32,
    pub source: Option<SourceGuid>,
    pub target: Option<SourceGuid>,
    pub item_owner: Option<SourceGuid>,
}
impl DbScriptInvocation {
    fn matches(self, pending: Self) -> bool {
        self.kind == pending.kind
            && self.id == pending.id
            && self.source.is_none_or(|g| Some(g) == pending.source)
            && self.target.is_none_or(|g| Some(g) == pending.target)
            && self
                .item_owner
                .is_none_or(|g| Some(g) == pending.item_owner)
    }
}
#[derive(Clone, Debug)]
enum Definition<'a> {
    Loaded(&'a DbScriptRow),
    Internal(std::sync::Arc<DbScriptRow>),
}
#[derive(Clone, Debug)]
struct Step<'a> {
    invocation: DbScriptInvocation,
    definition: Definition<'a>,
}
impl Step<'_> {
    fn row(&self) -> &DbScriptRow {
        match &self.definition {
            Definition::Loaded(row) => row,
            Definition::Internal(row) => row,
        }
    }
}
/// Loaded rows stay borrowed; delayed internal commands retain their own immutable copy.
pub struct SourceDbScriptExecutor<'a> {
    maps: &'a SourceDbScriptMaps,
    map: MapIdentity,
    next: u64,
    queue: BTreeMap<(i64, u64), Step<'a>>,
}
/// Live dependencies, not selected command results. Each missing call refuses where reached.
pub struct DbScriptContext<'a, O> {
    pub objects: &'a mut SourceObjects,
    pub units: &'a mut SourceUnits,
    pub owner: &'a mut O,
}
impl<'a> SourceDbScriptExecutor<'a> {
    pub fn new(map: MapIdentity, maps: &'a SourceDbScriptMaps) -> Self {
        Self {
            maps,
            map,
            next: 0,
            queue: BTreeMap::new(),
        }
    }
    pub fn pending(&self) -> impl Iterator<Item = (i64, DbScriptInvocation, u64)> + '_ {
        self.queue
            .iter()
            .map(|(&(at, _), step)| (at, step.invocation, step.row().ordinal))
    }
    pub fn inventory(&self) -> serde_json::Value {
        serde_json::json!({"version":1,"map":self.map,"next":self.next,
            "pending":self.queue.iter().map(|(&(at,serial),step)|
                serde_json::json!({"deadline":at,"serial":serial,"invocation":step.invocation,
                    "internal_copy":matches!(&step.definition,Definition::Internal(_)),
                    "definition":step.row()})).collect::<Vec<_>>(),
            "live_restore":"execution.db_script_executor.f7_continuation_owner"})
    }
    pub fn validate(&self) -> Result<()> {
        for (&(_, serial), step) in &self.queue {
            require(
                serial < self.next && step.invocation.id == step.row().id,
                "execution.db_script_executor.queue_identity",
            )?;
        }
        Ok(())
    }
    fn invocation<O: DbScriptExecutionOwner>(
        &self,
        kind: Option<DbScriptType>,
        id: u32,
        source: ObjectHandle,
        target: Option<ObjectHandle>,
        context: &mut DbScriptContext<'_, O>,
    ) -> Result<DbScriptInvocation> {
        let object = context.objects.get(source)?;
        let source_guid = object.guid.ok_or_else(|| script_missing("source_guid"))?;
        let item_owner = if matches!(
            object.kind,
            SourceObjectKind::Item | SourceObjectKind::Container
        ) {
            Some(context.owner.item_owner(source)?)
        } else {
            None
        };
        let target = target
            .map(|h| {
                context
                    .objects
                    .get(h)?
                    .guid
                    .ok_or_else(|| script_missing("target_guid"))
            })
            .transpose()?;
        Ok(DbScriptInvocation {
            kind,
            id,
            source: Some(source_guid),
            target,
            item_owner,
        })
    }
    /// Zero delays run synchronously; uniqueness inspects only the pending queue, Map.cpp:2181-2219.
    #[allow(clippy::too_many_arguments)]
    pub fn start<O: DbScriptExecutionOwner>(
        &mut self,
        kind: DbScriptType,
        id: u32,
        source: ObjectHandle,
        target: Option<ObjectHandle>,
        unique: u8,
        context: &mut DbScriptContext<'_, O>,
    ) -> Result<bool> {
        // Non-fixed ScriptExecutionParam has range 0..3, Map.h:233-239.
        require(
            unique <= 3,
            "execution.db_script_executor.uniqueness_enum_undefined",
        )?;
        let Some(rows) = self.maps.map(kind)?.scripts().get(&id) else {
            return Ok(false);
        };
        let invocation = self.invocation(Some(kind), id, source, target, context)?;
        if unique != 0 {
            let search = DbScriptInvocation {
                source: if unique & 1 != 0 {
                    invocation.source
                } else {
                    None
                },
                target: if unique & 2 != 0 {
                    invocation.target
                } else {
                    None
                },
                ..invocation
            };
            if self
                .queue
                .values()
                .any(|step| search.matches(step.invocation))
            {
                return Ok(true);
            }
        }
        for row in rows {
            let step = Step {
                invocation,
                definition: Definition::Loaded(row),
            };
            if row.delay == 0 {
                if self.handle_step(step, context)? {
                    return Ok(true);
                }
            } else {
                self.schedule(row.delay, step, context.owner)?;
            }
        }
        Ok(true)
    }
    /// Delayed internal commands own their copy, Map.cpp:2234, regardless of the stale comment :2227.
    pub fn command_start<O: DbScriptExecutionOwner>(
        &mut self,
        row: &DbScriptRow,
        delay: u32,
        source: ObjectHandle,
        target: Option<ObjectHandle>,
        context: &mut DbScriptContext<'_, O>,
    ) -> Result<()> {
        let invocation = self.invocation(None, row.id, source, target, context)?;
        if delay == 0 {
            self.handle_step(
                Step {
                    invocation,
                    definition: Definition::Loaded(row),
                },
                context,
            )?;
        } else {
            let step = Step {
                invocation,
                definition: Definition::Internal(std::sync::Arc::new(row.clone())),
            };
            self.schedule(delay, step, context.owner)?;
        }
        Ok(())
    }
    fn schedule(
        &mut self,
        delay: u32,
        step: Step<'a>,
        owner: &mut impl DbScriptExecutionOwner,
    ) -> Result<()> {
        let at = owner
            .clock_millis()?
            .checked_add(i64::from(delay))
            .ok_or_else(|| refuse("execution.db_script_executor.clock_add_overflow_undefined"))?;
        let serial = self.next;
        self.next = self
            .next
            .checked_add(1)
            .ok_or_else(|| refuse("execution.db_script_executor.queue_serial_exhausted"))?;
        self.queue.insert((at, serial), step);
        Ok(())
    }
    /// Inclusive deadlines and restart-from-begin, Map.cpp:2251-2275; map phase at :1025.
    /// Keep the executing entry visible to recursive uniqueness checks until its callback returns.
    pub fn process<O: DbScriptExecutionOwner>(
        &mut self,
        context: &mut DbScriptContext<'_, O>,
    ) -> Result<()> {
        while let Some((&key, step)) = self.queue.first_key_value() {
            if key.0 > context.owner.clock_millis()? {
                break;
            }
            let step = step.clone();
            let invocation = step.invocation;
            if self.handle_step(step, context)? {
                self.queue.retain(|_, s| !invocation.matches(s.invocation));
            } else {
                self.queue.remove(&key);
            }
        }
        Ok(())
    }
    fn handle_step<O: DbScriptExecutionOwner>(
        &mut self,
        step: Step<'_>,
        context: &mut DbScriptContext<'_, O>,
    ) -> Result<bool> {
        let Some(targets::ProcessTargets {
            sources,
            targets,
            buddy_found,
            item_source,
        }) = self.targets(&step, context)?
        else {
            return Ok(false);
        };
        let mut terminate = false;
        // Membership is fixed before callbacks, with source-major pairs, ScriptMgr.cpp:1740-1762.
        for source in sources {
            if targets.is_empty() {
                terminate |= self.execute_command(
                    step.row(),
                    source,
                    None,
                    buddy_found,
                    item_source,
                    context,
                )?;
            } else {
                for &target in &targets {
                    terminate |= self.execute_command(
                        step.row(),
                        source,
                        target,
                        buddy_found,
                        item_source,
                        context,
                    )?;
                }
            }
        }
        Ok(terminate)
    }
}
