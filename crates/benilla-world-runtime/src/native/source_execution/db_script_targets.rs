// SPDX-License-Identifier: GPL-3.0-or-later
//! ScriptMgr.cpp:1309-1643, including its asymmetric buddy and origin quirks.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DbScriptVisit {
    GridCreatures,
    WorldCreatures,
    AllCreatures,
    GridGameObjects,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DbScriptMembers {
    Pool(u32),
    Group(u32),
    StringId(u32),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DbScriptDistance {
    Bounding,
    SquaredNoBounding,
    TwoDimensional,
    Combat,
    InMap,
}

pub(super) struct ProcessTargets {
    pub sources: Vec<Option<ObjectHandle>>,
    pub targets: Vec<Option<ObjectHandle>>,
    pub buddy_found: bool,
    pub item_source: Option<ObjectHandle>,
}

impl SourceDbScriptExecutor<'_> {
    fn resolve<O: DbScriptExecutionOwner>(
        &self,
        guid: Option<SourceGuid>,
        owner: Option<SourceGuid>,
        include_item: bool,
        context: &mut DbScriptContext<'_, O>,
    ) -> Result<Option<Option<ObjectHandle>>> {
        let Some(guid) = guid.filter(|g| g.raw() != 0) else {
            return Ok(Some(None));
        };
        let object = match guid.high() {
            0xf130 => context
                .objects
                .lookup(self.map, LiveLookupKind::Creature, guid),
            0xf140 => context.objects.lookup(self.map, LiveLookupKind::Pet, guid),
            0 => context
                .objects
                .lookup(self.map, LiveLookupKind::Player, guid),
            0xf110 => context
                .objects
                .lookup(self.map, LiveLookupKind::GameObject, guid),
            0xf101 => context.owner.global_corpse(guid)?,
            0x1fc0 => context.owner.map_transport(self.map, guid)?,
            0x4000 if include_item => {
                if let Some(player) =
                    owner.and_then(|g| context.objects.lookup(self.map, LiveLookupKind::Player, g))
                {
                    context.owner.player_item(player, guid)?
                } else {
                    None
                }
            }
            // Unsupported GUIDs log and skip this step, not the remaining invocation (:1348,1717).
            _ => return Ok(None),
        };
        Ok(Some(if let Some(h) = object {
            context.objects.get(h)?.in_world.then_some(h)
        } else {
            None
        }))
    }
    pub(super) fn targets<O: DbScriptExecutionOwner>(
        &self,
        step: &Step<'_>,
        context: &mut DbScriptContext<'_, O>,
    ) -> Result<Option<ProcessTargets>> {
        let Some(source) = self.resolve(
            step.invocation.source,
            step.invocation.item_owner,
            true,
            context,
        )?
        else {
            return Ok(None);
        };
        let Some(target) = self.resolve(
            step.invocation.target,
            step.invocation.item_owner,
            false,
            context,
        )?
        else {
            return Ok(None);
        };
        let item_source = if let Some(h) = source {
            matches!(
                context.objects.get(h)?.kind,
                SourceObjectKind::Item | SourceObjectKind::Container
            )
            .then_some(h)
        } else {
            None
        };
        let source = source.filter(|h| Some(*h) != item_source);
        let mut sources: Vec<_> = source.map(Some).into_iter().collect();
        let mut targets: Vec<_> = target.map(Some).into_iter().collect();
        let Some(buddies) = self.buddies(step.row(), source, target, context)? else {
            return Ok(None);
        };
        let found = !buddies.is_empty(); // :1643 narrows size to bool, even for its null buddy quirk.
        if step.row().flags & 1 != 0 {
            targets = buddies;
        } else if found {
            sources = buddies;
        }
        if step.row().flags & 2 != 0 {
            std::mem::swap(&mut sources, &mut targets);
        }
        if step.row().flags & 4 != 0 {
            targets.clone_from(&sources);
        }
        Ok(Some(ProcessTargets {
            sources,
            targets,
            buddy_found: found,
            item_source,
        }))
    }
    /// This expression is intentionally not corrected to prefer a nonplayer target, :1444-1446.
    fn origin<O: DbScriptExecutionOwner>(
        source: Option<ObjectHandle>,
        target: Option<ObjectHandle>,
        context: &DbScriptContext<'_, O>,
    ) -> Result<ObjectHandle> {
        let mut origin = source
            .or(target)
            .ok_or_else(|| refuse("execution.db_script_executor.null_buddy_origin_undefined"))?;
        if context.objects.get(origin)?.kind == SourceObjectKind::Player
            && source.is_some_and(|s| {
                context
                    .objects
                    .get(s)
                    .is_ok_and(|o| o.kind != SourceObjectKind::Player)
            })
        {
            origin = target.ok_or_else(|| {
                refuse("execution.db_script_executor.null_buddy_origin_undefined")
            })?;
        }
        Ok(origin)
    }
    pub(super) fn buddies<O: DbScriptExecutionOwner>(
        &self,
        row: &DbScriptRow,
        source: Option<ObjectHandle>,
        target: Option<ObjectHandle>,
        context: &mut DbScriptContext<'_, O>,
    ) -> Result<Option<Vec<Option<ObjectHandle>>>> {
        let mut buddies = Vec::new();
        if row.buddy == 0 && row.flags & 0x90 == 0 {
            return Ok(Some(buddies));
        }
        let creature = row.creature_buddy();
        let dead = row.flags & 0x40 != 0;
        let all = row.flags & 0x200 != 0;
        if row.flags & 0x10 != 0 {
            let buddy = context.objects.lookup_database(
                self.map,
                if creature {
                    DatabaseLookupKind::Creature
                } else {
                    DatabaseLookupKind::GameObject
                },
                row.radius_or_guid,
            );
            if creature
                && buddy.is_some_and(|h| context.objects.get(h).is_ok_and(|o| o.alive == dead))
                && row.command != 31
            {
                return Ok(None);
            }
            if let Some(buddy) = buddy {
                buddies.push(Some(buddy));
            } else if row.command != 31 {
                return Ok(None);
            }
        } else if row.flags & 0x80 != 0 {
            if creature {
                // Owner supplies equal-chanced then explicitly-chanced source order, :1400-1429.
                for h in context
                    .owner
                    .members(
                        self.map,
                        DbScriptMembers::Pool(row.radius_or_guid as u16 as u32),
                    )?
                    .ok_or_else(|| script_missing("f4_pool_members"))?
                {
                    if context.objects.get(h)?.alive != dead {
                        buddies.push(Some(h));
                        break;
                    }
                }
            }
            if buddies.is_empty() && row.command != 31 {
                return Ok(None);
            }
        } else if row.flags & 0x100 != 0 {
            let origin = Self::origin(source, target, context)?;
            // No life-state/radius filter in the group branch, :1448-1490.
            let members = context
                .owner
                .members(self.map, DbScriptMembers::Group(row.buddy))?
                .ok_or_else(|| {
                    refuse("execution.db_script_executor.null_spawn_group_entry_undefined")
                })?;
            if all {
                buddies.extend(members.into_iter().map(Some));
            } else if let Some(h) =
                Self::closest(origin, &members, DbScriptDistance::Bounding, context)?
            {
                buddies.push(Some(h));
            }
        } else if row.flags & 0x800 != 0 {
            let origin = Self::origin(source, target, context)?;
            let mut members = Vec::new();
            let Some(objects) = context
                .owner
                .members(self.map, DbScriptMembers::StringId(row.buddy))?
            else {
                return Ok(None); // A null map list skips even TERMINATE_SCRIPT, ScriptMgr.cpp:1499-1500.
            };
            for h in objects {
                let o = context.objects.get(h)?;
                if !matches!(o.kind, SourceObjectKind::Creature | SourceObjectKind::Pet)
                    || o.alive != dead
                {
                    members.push(h);
                }
            }
            if all {
                buddies.extend(members.into_iter().map(Some));
            } else if let Some(h) = Self::closest(
                origin,
                &members,
                DbScriptDistance::SquaredNoBounding,
                context,
            )? {
                buddies.push(Some(h));
            }
            if row.radius_or_guid > 0 {
                let mut filtered = Vec::new();
                for h in buddies.into_iter().flatten() {
                    if context.owner.within(
                        origin,
                        h,
                        (row.radius_or_guid as f32).to_bits(),
                        DbScriptDistance::Bounding,
                    )? {
                        filtered.push(Some(h));
                    }
                }
                buddies = filtered;
            }
            if buddies.is_empty() && row.command != 31 {
                return Ok(None);
            }
        } else {
            if source.is_none() && target.is_none() {
                return Ok(None);
            } // :1537-1540 is defined.
            let origin = Self::origin(source, target, context)?;
            let visit = if !creature {
                DbScriptVisit::GridGameObjects
            } else if all {
                DbScriptVisit::AllCreatures
            } else if !dead && row.flags & 0x20 != 0 {
                DbScriptVisit::WorldCreatures
            } else {
                DbScriptVisit::GridCreatures
            };
            let radius = (row.radius_or_guid as f32).to_bits();
            let mut range = if creature && !dead && !all {
                (row.radius_or_guid as f32 * row.radius_or_guid as f32).to_bits()
            } else {
                radius
            };
            let mut last = None;
            for h in DbScriptExecutionOwner::visit(context.owner, origin, radius, visit)? {
                if context.owner.entry(h)? != row.buddy {
                    continue;
                }
                if creature && (all || !dead) {
                    if context.objects.get(h)?.alive == dead {
                        continue;
                    }
                    // All-dead buddies require CORPSE, not merely !IsAlive, GridNotifiers.h:1306.
                    if all && dead && !context.owner.is_corpse(h)? {
                        continue;
                    }
                    if !all && h == origin {
                        continue;
                    }
                }
                let accepts = if creature && !dead && !all {
                    // GridNotifiers.h:1265-1275 compares squared distance to squared radius.
                    let d =
                        context
                            .owner
                            .distance(origin, h, DbScriptDistance::SquaredNoBounding)?;
                    if f32::from_bits(d) < f32::from_bits(range) {
                        range = d;
                        true
                    } else {
                        false
                    }
                } else {
                    let mode = if creature && all {
                        DbScriptDistance::Combat
                    } else if creature {
                        DbScriptDistance::TwoDimensional
                    } else {
                        DbScriptDistance::InMap
                    };
                    context.owner.within(origin, h, range, mode)?
                };
                if accepts {
                    if all {
                        buddies.push(Some(h));
                    } else {
                        last = Some(h);
                        if !creature {
                            range =
                                context
                                    .owner
                                    .distance(origin, h, DbScriptDistance::Bounding)?;
                        }
                    }
                }
            }
            if !all {
                if let Some(h) = last {
                    buddies.push(Some(h));
                } else if creature && context.owner.entry(origin)? == row.buddy {
                    // :1586-1589 pushes nullptr, rather than origin. Commands still run for that pair.
                    buddies.push(None);
                }
            }
            if buddies.is_empty() && row.command != 31 {
                return Ok(None);
            }
        }
        Ok(Some(buddies))
    }
    pub(super) fn termination_entry_found<O: DbScriptExecutionOwner>(
        &self,
        origin: ObjectHandle,
        entry: u32,
        radius: u32,
        creature: bool,
        context: &mut DbScriptContext<'_, O>,
    ) -> Result<bool> {
        let visit = if creature {
            DbScriptVisit::GridCreatures
        } else {
            DbScriptVisit::GridGameObjects
        };
        let mut range = if creature {
            (radius as f32 * radius as f32).to_bits()
        } else {
            (radius as f32).to_bits()
        };
        let mut found = false;
        for h in
            DbScriptExecutionOwner::visit(context.owner, origin, (radius as f32).to_bits(), visit)?
        {
            if context.owner.entry(h)? != entry {
                continue;
            }
            if creature {
                if h == origin || !context.objects.get(h)?.alive {
                    continue;
                }
                let distance =
                    context
                        .owner
                        .distance(origin, h, DbScriptDistance::SquaredNoBounding)?;
                if f32::from_bits(distance) < f32::from_bits(range) {
                    range = distance;
                    found = true;
                }
            } else if context
                .owner
                .within(origin, h, range, DbScriptDistance::InMap)?
            {
                range = context
                    .owner
                    .distance(origin, h, DbScriptDistance::Bounding)?;
                found = true;
            }
        }
        Ok(found)
    }
    fn closest<O: DbScriptExecutionOwner>(
        origin: ObjectHandle,
        members: &[ObjectHandle],
        mode: DbScriptDistance,
        context: &mut DbScriptContext<'_, O>,
    ) -> Result<Option<ObjectHandle>> {
        let mut closest = None;
        for &h in members {
            if let Some(old) = closest {
                if f32::from_bits(context.owner.distance(origin, h, mode)?)
                    < f32::from_bits(context.owner.distance(origin, old, mode)?)
                {
                    closest = Some(h);
                }
            } else {
                closest = Some(h);
            }
        }
        Ok(closest)
    }
}
