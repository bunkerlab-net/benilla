// SPDX-License-Identifier: GPL-3.0-or-later
//! ScriptMgr.cpp:1767-3346. Owner calls are effects, never a declaration that a row is supported.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DbScriptSoundKind {
    Music,
    Distance,
    Map,
    Direct,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DbScriptSoundAudience {
    Set,
    Target(ObjectHandle),
    Zone(u32),
    Area(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DbScriptCall {
    DisplayText(i32),
    Emote(u32),
    WriteField {
        field: u32,
        value: u32,
    },
    SetWalk(bool),
    Stand(u32),
    Gossip(u32),
    WorldState(u32, u32),
    Sheathe(u32),
    StringId(u32, bool),
    Active(bool),
    Faction(u32, u32),
    Morph(u32),
    Mount(u32),
    MountEntry(u32),
    Hover(bool),
    EquipmentDefault,
    VirtualItem(u8, u32),
    UpdateEntry(u32),
    Teleport,
    QuestExplored {
        quest: u32,
        fail: bool,
    },
    KillCredit {
        entry: u32,
        group: bool,
    },
    RespawnGo,
    SummonCreature,
    Door {
        open: bool,
        reset: u32,
        alternate: bool,
    },
    GoUse,
    GoAnimation(u32),
    RemoveAura,
    CastSpell(u32),
    Sound {
        id: u32,
        kind: DbScriptSoundKind,
        audience: DbScriptSoundAudience,
    },
    CreateItem,
    Despawn(u32),
    Movement,
    MoveTo,
    AttackStart,
    Taxi,
    WaypointPause(bool),
    WaypointTerminationPause(i32),
    Facing(u32),
    NearTeleport,
    MotionClear(bool),
    GroupMoveHome,
    FailQuestGroup(u32),
    FacingReset,
    FacingTarget,
    MoveDynamic([u32; 3]),
    Mail,
    DespawnGo(u32),
    Respawn,
    ResetGo,
    CustomSpell,
    Interrupt(u32),
    InstanceData(u32, u64),
    ZoneCombat,
    Formation,
    FormationMovement(u32, u32),
    SpawnGroup,
    MeetingStone,
}
impl DbScriptCall {
    pub fn missing_owner(self) -> Refusal {
        script_missing(match self {
            Self::DisplayText(_) => "presentation_text",
            Self::Emote(_) => "presentation_emote",
            Self::Sound { .. } => "presentation_sound",
            Self::WriteField { .. }
            | Self::Stand(_)
            | Self::Sheathe(_)
            | Self::SetWalk(_)
            | Self::StringId(..)
            | Self::Morph(_)
            | Self::Mount(_) => "unit_fields",
            Self::Gossip(_) => "f5_25_gossip",
            Self::WorldState(..) => "worldstate_variables",
            Self::Active(_) => "f4_active_object",
            Self::Faction(..) => "temporary_faction",
            Self::MountEntry(_) | Self::Hover(_) => "f5_05_movement_flags",
            Self::EquipmentDefault | Self::VirtualItem(..) => "equipment",
            Self::UpdateEntry(_) => "f4_update_entry",
            Self::Teleport => "f5_24_player_teleport",
            Self::QuestExplored { .. } | Self::KillCredit { .. } | Self::FailQuestGroup(_) => {
                "f5_24_quest"
            }
            Self::RespawnGo
            | Self::Door { .. }
            | Self::GoUse
            | Self::GoAnimation(_)
            | Self::DespawnGo(_)
            | Self::ResetGo => "f4_gameobject_lifecycle",
            Self::SummonCreature => "f5_15_summon",
            Self::RemoveAura => "f5_12_aura",
            Self::CastSpell(_) | Self::CustomSpell | Self::Interrupt(_) => "f5_08_spell",
            Self::CreateItem => "f5_26_inventory",
            Self::Despawn(_) | Self::Respawn => "f4_creature_lifecycle",
            Self::Movement
            | Self::MoveTo
            | Self::WaypointPause(_)
            | Self::WaypointTerminationPause(_)
            | Self::FacingReset => "f5_05_motion",
            Self::Facing(_) => "f5_05_facing",
            Self::NearTeleport => "f4_unit_relocation",
            Self::MotionClear(_) => "f5_05_motion",
            Self::FacingTarget => "f6_geometry",
            Self::MoveDynamic(_) => "f5_05_motion",
            Self::AttackStart | Self::ZoneCombat => "f5_04_combat",
            Self::Taxi => "f5_24_player_taxi",
            Self::Mail => "f5_25_mail",
            Self::InstanceData(..) => "instance_script",
            Self::Formation | Self::FormationMovement(..) | Self::GroupMoveHome => "f4_formation",
            Self::SpawnGroup => "f4_spawn_group",
            Self::MeetingStone => "f5_25_lfg",
        })
    }
}
pub trait DbScriptExecutionOwner: AssistanceExecutionOwner {
    fn clock_millis(&mut self) -> Result<i64> {
        Err(script_missing("map_clock"))
    }
    fn zone_id(&mut self, _source: ObjectHandle) -> Result<u32> {
        Err(script_missing("f6_zone"))
    }
    fn area_id(&mut self, _source: ObjectHandle) -> Result<u32> {
        Err(script_missing("f6_area"))
    }
    fn urand(&mut self, _min: u32, _max: u32) -> Result<u32> {
        Err(script_missing("thread_rng"))
    }
    fn chance(&mut self) -> Result<f64> {
        Err(script_missing("thread_rng"))
    }
    fn item_owner(&mut self, _item: ObjectHandle) -> Result<SourceGuid> {
        Err(script_missing("f5_26_item"))
    }
    fn player_item(
        &mut self,
        _player: ObjectHandle,
        _guid: SourceGuid,
    ) -> Result<Option<ObjectHandle>> {
        Err(script_missing("f5_26_item"))
    }
    fn global_corpse(&mut self, _guid: SourceGuid) -> Result<Option<ObjectHandle>> {
        Err(script_missing("f4_global_corpse_lookup"))
    }
    fn map_transport(
        &mut self,
        _map: MapIdentity,
        _guid: SourceGuid,
    ) -> Result<Option<ObjectHandle>> {
        Err(script_missing("f4_transport_lookup"))
    }
    fn entry(&mut self, _object: ObjectHandle) -> Result<u32> {
        Err(script_missing("effective_entry"))
    }
    /// Membership and traversal only. The executor applies the source predicates.
    fn members(
        &mut self,
        _map: MapIdentity,
        kind: DbScriptMembers,
    ) -> Result<Option<Vec<ObjectHandle>>> {
        Err(script_missing(match kind {
            DbScriptMembers::Pool(_) => "f4_pool_members",
            DbScriptMembers::Group(_) => "f4_group_members",
            DbScriptMembers::StringId(_) => "string_id_members",
        }))
    }
    fn visit(
        &mut self,
        _origin: ObjectHandle,
        _radius: u32,
        _kind: DbScriptVisit,
    ) -> Result<Vec<ObjectHandle>> {
        Err(script_missing("f6_grid_visitor"))
    }
    fn distance(
        &mut self,
        _origin: ObjectHandle,
        _target: ObjectHandle,
        _mode: DbScriptDistance,
    ) -> Result<u32> {
        Err(script_missing("f6_distance"))
    }
    fn within(
        &mut self,
        _origin: ObjectHandle,
        _target: ObjectHandle,
        _radius: u32,
        _mode: DbScriptDistance,
    ) -> Result<bool> {
        Err(script_missing("f6_distance"))
    }
    fn condition(
        &mut self,
        _id: u32,
        _target: Option<ObjectHandle>,
        _source: Option<ObjectHandle>,
    ) -> Result<bool> {
        Err(script_missing("f5_22_condition"))
    }
    fn values_count(&mut self, _object: ObjectHandle) -> Result<u32> {
        Err(script_missing("object_values"))
    }
    fn field(&mut self, _object: ObjectHandle, _field: u32) -> Result<u32> {
        Err(script_missing("object_values"))
    }
    fn walking(&mut self, _creature: ObjectHandle) -> Result<bool> {
        Err(script_missing("f5_05_movement_flags"))
    }
    fn near_script_point(&mut self, _unit: ObjectHandle, _point: [u32; 3]) -> Result<bool> {
        Err(script_missing("f6_point_distance"))
    }
    fn dynamic_point(
        &mut self,
        _row: &DbScriptRow,
        _source: ObjectHandle,
        _target: ObjectHandle,
    ) -> Result<[u32; 3]> {
        Err(script_missing("f6_geometry"))
    }
    fn creature_group(&mut self, _unit: ObjectHandle) -> Result<bool> {
        Err(script_missing("f4_creature_group"))
    }
    fn can_attack_now(&mut self, _source: ObjectHandle, _target: ObjectHandle) -> Result<bool> {
        Err(script_missing("f5_04_attack_eligibility"))
    }
    /// SpellEntry attribute EXCLUDE_CASTER and Targets, not an expected cast result.
    fn spell_targets(&mut self, _id: u32) -> Result<(bool, u32)> {
        Err(script_missing("f5_07_spell_template"))
    }
    fn choose_display(&mut self, _entry: u32) -> Result<u32> {
        Err(script_missing("f4_creature_display"))
    }
    fn has_instance(&mut self, _source: ObjectHandle) -> Result<bool> {
        Err(script_missing("instance_binding"))
    }
    fn go_type(&mut self, _go: ObjectHandle) -> Result<u32> {
        Err(script_missing("gameobject_type"))
    }
    fn go_state(&mut self, _go: ObjectHandle) -> Result<u32> {
        Err(script_missing("gameobject_state"))
    }
    fn go_spawned(&mut self, _go: ObjectHandle) -> Result<bool> {
        Err(script_missing("gameobject_spawned"))
    }
    fn formation_member(&mut self, _creature: ObjectHandle) -> Result<Option<bool>> {
        Err(script_missing("f4_formation_membership"))
    }
    fn is_corpse(&mut self, _creature: ObjectHandle) -> Result<bool> {
        Err(script_missing("f4_creature_death_state"))
    }
    fn current_motion(&mut self, _creature: ObjectHandle) -> Result<u32> {
        Err(script_missing("f5_05_motion"))
    }
    /// Called at the source function call, after opcode-specific guards and RNG.
    fn call(
        &mut self,
        call: DbScriptCall,
        _row: &DbScriptRow,
        _source: Option<ObjectHandle>,
        _target: Option<ObjectHandle>,
        _objects: &mut SourceObjects,
        _units: &mut SourceUnits,
    ) -> Result<()> {
        Err(call.missing_owner())
    }
}

impl SourceDbScriptExecutor<'_> {
    fn is(
        context: &DbScriptContext<'_, impl DbScriptExecutionOwner>,
        h: Option<ObjectHandle>,
        kind: u8,
    ) -> Result<bool> {
        let Some(h) = h else { return Ok(false) };
        let k = context.objects.get(h)?.kind;
        Ok(match kind {
            0 => matches!(k, SourceObjectKind::Creature | SourceObjectKind::Pet),
            1 => matches!(
                k,
                SourceObjectKind::Creature | SourceObjectKind::Pet | SourceObjectKind::Player
            ),
            2 => matches!(
                k,
                SourceObjectKind::GameObject | SourceObjectKind::Transport
            ),
            _ => k == SourceObjectKind::Player,
        })
    }
    fn player(
        context: &DbScriptContext<'_, impl DbScriptExecutionOwner>,
        source: Option<ObjectHandle>,
        target: Option<ObjectHandle>,
    ) -> Result<Option<ObjectHandle>> {
        Ok(if Self::is(context, target, 3)? {
            target
        } else if Self::is(context, source, 3)? {
            source
        } else {
            None
        })
    }
    fn call(
        context: &mut DbScriptContext<'_, impl DbScriptExecutionOwner>,
        call: DbScriptCall,
        row: &DbScriptRow,
        source: Option<ObjectHandle>,
        target: Option<ObjectHandle>,
    ) -> Result<()> {
        context
            .owner
            .call(call, row, source, target, context.objects, context.units)
    }
    fn template_id(
        &self,
        id: u32,
        relay: bool,
        owner: &mut impl DbScriptExecutionOwner,
    ) -> Result<i32> {
        let Some(rows) = (if relay {
            self.maps.templates().relay(id)
        } else {
            self.maps.templates().string(id)
        }) else {
            return Ok(0);
        };
        if rows.iter().any(|&(_, chance)| chance != 0) {
            let random = owner.chance()? as f32;
            let mut cumulative = 0u32;
            for &(target, chance) in rows.iter().filter(|&&(_, chance)| chance != 0) {
                cumulative = cumulative.wrapping_add(chance);
                if cumulative as f32 >= random {
                    return Ok(target);
                }
            }
        }
        let count = rows.iter().filter(|&&(_, chance)| chance == 0).count();
        if count != 0 {
            let chosen = owner.urand(0, count as u32 - 1)?;
            return Ok(rows
                .iter()
                .filter(|&&(_, chance)| chance == 0)
                .nth(chosen as usize)
                .ok_or_else(|| refuse("execution.db_script_executor.rng_out_of_range"))?
                .0);
        }
        Ok(0)
    }
    pub(super) fn execute_command<O: DbScriptExecutionOwner>(
        &mut self,
        row: &DbScriptRow,
        source: Option<ObjectHandle>,
        mut target: Option<ObjectHandle>,
        buddy_found: bool,
        item_source: Option<ObjectHandle>,
        context: &mut DbScriptContext<'_, O>,
    ) -> Result<bool> {
        if row.condition != 0 && !context.owner.condition(row.condition, target, source)? {
            return Ok(false);
        }
        let [a, b, c] = row.data;
        let additional = row.flags & 8 != 0;
        match row.command {
            0 => {
                if source.is_none() {
                    return Ok(false);
                }
                let text = if a != 0 {
                    self.template_id(a, false, context.owner)?
                } else if row.text[1] != 0 {
                    let count = row.text.iter().skip(2).take_while(|&&v| v != 0).count() + 2;
                    row.text[context.owner.urand(0, count as u32 - 1)? as usize]
                } else {
                    row.text[0]
                };
                if a == 0 || text != 0 {
                    let unit_target = if Self::is(context, target, 1)? {
                        target
                    } else {
                        None
                    };
                    Self::call(
                        context,
                        DbScriptCall::DisplayText(text),
                        row,
                        source,
                        unit_target,
                    )?;
                }
            }
            1 => {
                if !Self::is(context, source, 1)? {
                    return Ok(false);
                }
                let count = row.text.iter().take_while(|&&v| v != 0).count();
                // Always draws, including the singleton, ScriptMgr.cpp:1826.
                let index = context.owner.urand(0, count as u32)?;
                let emote = if index == 0 {
                    a
                } else {
                    row.text[index as usize - 1] as u32
                };
                Self::call(context, DbScriptCall::Emote(emote), row, source, target)?;
            }
            2 | 4 | 5 => {
                let Some(h) = source.or(item_source) else {
                    return Ok(false);
                };
                if a <= 3 || a >= context.owner.values_count(h)? {
                    return Ok(false);
                }
                let value = match row.command {
                    4 => context.owner.field(h, a)? | b,
                    5 => context.owner.field(h, a)? & !b,
                    _ => b,
                };
                Self::call(
                    context,
                    DbScriptCall::WriteField { field: a, value },
                    row,
                    Some(h),
                    target,
                )?;
            }
            3 => {
                if !Self::is(context, source, 1)? {
                    return Ok(false);
                }
                // The non-derived static downcast itself is undefined, ScriptMgr.cpp:1849.
                if !Self::is(context, source, 0)? {
                    return Err(refuse(
                        "execution.db_script_executor.move_to_creature_downcast_undefined",
                    ));
                }
                if row.text[0] != 0 {
                    match row.text[0] {
                        1 => {
                            require(
                                c <= 3,
                                "execution.db_script_executor.forced_movement_enum_undefined",
                            )?;
                            Self::call(context, DbScriptCall::MoveTo, row, source, target)?;
                        }
                        2 if context.owner.creature_group(source.unwrap())? => {
                            Self::call(context, DbScriptCall::GroupMoveHome, row, source, target)?
                        }
                        2 => {
                            require(
                                c <= 3,
                                "execution.db_script_executor.forced_movement_enum_undefined",
                            )?;
                            Self::call(context, DbScriptCall::MoveTo, row, source, target)?;
                        }
                        _ => {}
                    }
                    return Ok(false);
                }
                let point = [row.floats[1], row.floats[2], row.floats[3]];
                if point.iter().all(|&v| f32::from_bits(v) == 0.)
                    || context.owner.near_script_point(source.unwrap(), point)?
                {
                    Self::call(
                        context,
                        DbScriptCall::Facing(row.floats[4]),
                        row,
                        source,
                        target,
                    )?;
                    return Ok(false);
                }
                if f32::from_bits(point[0]) == 0.
                    && f32::from_bits(point[1]) == 0.
                    && f32::from_bits(point[2]) != 0.
                {
                    require(
                        c <= 3,
                        "execution.db_script_executor.forced_movement_enum_undefined",
                    )?;
                    Self::call(context, DbScriptCall::MoveTo, row, source, target)?;
                } else if additional {
                    Self::call(context, DbScriptCall::NearTeleport, row, source, target)?;
                } else {
                    Self::call(
                        context,
                        DbScriptCall::MotionClear(b & 2 != 0),
                        row,
                        source,
                        target,
                    )?;
                    require(
                        c <= 3,
                        "execution.db_script_executor.forced_movement_enum_undefined",
                    )?;
                    Self::call(context, DbScriptCall::MoveTo, row, source, target)?;
                }
            }
            6 | 7 | 8 | 17 | 30 => {
                let Some(player) = Self::player(context, source, target)? else {
                    return Ok(false);
                };
                if row.command == 7 {
                    let object = if Self::is(context, source, 0)? || Self::is(context, source, 2)? {
                        source
                    } else if Self::is(context, target, 0)? || Self::is(context, target, 2)? {
                        target
                    } else {
                        None
                    };
                    if b != 0 && object.is_none() {
                        return Ok(false);
                    }
                    let fail = if Self::is(context, object, 0)?
                        && !context.objects.get(object.unwrap())?.alive
                    {
                        true
                    } else {
                        b != 0
                            && !context.owner.within(
                                object.unwrap(),
                                player,
                                (b as f32).to_bits(),
                                DbScriptDistance::InMap,
                            )?
                    };
                    Self::call(
                        context,
                        DbScriptCall::QuestExplored { quest: a, fail },
                        row,
                        Some(player),
                        object,
                    )?;
                } else if row.command == 8 {
                    let reward = if Self::is(context, source, 0)? {
                        source
                    } else if Self::is(context, target, 0)? {
                        target
                    } else {
                        None
                    };
                    let entry = if a != 0 {
                        a
                    } else if let Some(reward) = reward {
                        context.owner.entry(reward)?
                    } else {
                        return Ok(false);
                    };
                    let searcher = if b != 0 {
                        reward.or(source).or(target)
                    } else {
                        reward
                    };
                    Self::call(
                        context,
                        DbScriptCall::KillCredit {
                            entry,
                            group: b != 0,
                        },
                        row,
                        Some(player),
                        searcher,
                    )?;
                } else {
                    let call = match row.command {
                        6 => DbScriptCall::Teleport,
                        17 => DbScriptCall::CreateItem,
                        _ => DbScriptCall::Taxi,
                    };
                    Self::call(context, call, row, Some(player), target)?;
                }
            }
            9 | 11 | 12 => {
                let go = if a != 0 {
                    context
                        .objects
                        .lookup_database(self.map, DatabaseLookupKind::GameObject, a)
                } else if Self::is(context, source, 2)? {
                    source
                } else {
                    None
                };
                let Some(go) = go else { return Ok(false) };
                let kind = context.owner.go_type(go)?;
                if row.command == 9 {
                    if kind == 17 || kind == 0 || context.owner.go_spawned(go)? {
                        return Ok(false);
                    }
                    Self::call(context, DbScriptCall::RespawnGo, row, Some(go), target)?;
                } else {
                    if kind != 0 && kind != 1 {
                        return Ok(false);
                    }
                    let ready = context.owner.go_state(go)? == 1;
                    if (row.command == 11) != ready {
                        return Ok(false);
                    }
                    let call = DbScriptCall::Door {
                        open: row.command == 11,
                        reset: b.max(15),
                        alternate: c != 0,
                    };
                    Self::call(context, call, row, Some(go), None)?;
                    if let Some(button) = target {
                        if Self::is(context, Some(button), 2)?
                            && context.owner.go_type(button)? == 1
                        {
                            Self::call(context, call, row, Some(button), None)?;
                        }
                    }
                }
            }
            10 => {
                if source.is_some() {
                    Self::call(context, DbScriptCall::SummonCreature, row, source, target)?;
                }
            }
            13 => {
                if !Self::is(context, source, 1)? || !Self::is(context, target, 2)? {
                    return Ok(false);
                }
                Self::call(
                    context,
                    if additional {
                        DbScriptCall::GoAnimation(a)
                    } else {
                        DbScriptCall::GoUse
                    },
                    row,
                    source,
                    target,
                )?;
            }
            14 => {
                if !Self::is(context, source, 1)?
                    || b == 0 && additional && !Self::is(context, target, 1)?
                    || b > 2
                {
                    return Ok(false);
                }
                Self::call(context, DbScriptCall::RemoveAura, row, source, target)?;
            }
            15 => {
                let count = row.text.iter().take_while(|&&v| v != 0).count();
                let chosen = if count > 0 {
                    context.owner.urand(0, count as u32)?
                } else {
                    0
                };
                let spell = if chosen > 0 {
                    row.text[chosen as usize - 1] as u32
                } else {
                    a
                };
                let (exclude_caster, _) = context.owner.spell_targets(spell)?;
                if exclude_caster && source == target || additional {
                    target = None;
                }
                if Self::is(context, source, 2)? {
                    if !Self::is(context, target, 1)? {
                        return Ok(false);
                    }
                } else if !Self::is(context, source, 1)? {
                    return Ok(false);
                }
                Self::call(context, DbScriptCall::CastSpell(spell), row, source, target)?;
            }
            16 => {
                let Some(source) = source else {
                    return Ok(false);
                };
                let mut audience = DbScriptSoundAudience::Set;
                if b & 1 != 0 {
                    let Some(player) = Self::player(context, Some(source), target)? else {
                        return Ok(false);
                    };
                    audience = DbScriptSoundAudience::Target(player);
                }
                if b & 8 != 0 {
                    audience = DbScriptSoundAudience::Zone(if c != 0 {
                        c
                    } else {
                        context.owner.zone_id(source)?
                    });
                }
                if b & 16 != 0 {
                    audience = DbScriptSoundAudience::Area(if c != 0 {
                        c
                    } else {
                        context.owner.area_id(source)?
                    });
                }
                let kind = if additional {
                    DbScriptSoundKind::Music
                } else if b & 2 != 0 {
                    DbScriptSoundKind::Distance
                } else if b & 4 != 0 {
                    DbScriptSoundKind::Map
                } else {
                    DbScriptSoundKind::Direct
                };
                Self::call(
                    context,
                    DbScriptCall::Sound {
                        id: a,
                        kind,
                        audience,
                    },
                    row,
                    Some(source),
                    target,
                )?;
            }
            18 => {
                // Compatibility quirk is only applied when a noncreature target actually exists (:2260).
                if target.is_some()
                    && !Self::is(context, target, 0)?
                    && Self::is(context, source, 0)?
                {
                    target = source;
                }
                if Self::is(context, target, 0)? {
                    Self::call(context, DbScriptCall::Despawn(a), row, source, target)?;
                }
            }
            19 | 33 => {} // Genuine skipped/unsupported source cases, :2273-2275,2715-2718.
            20 => {
                if !Self::is(context, source, 0)? {
                    return Ok(false);
                }
                let h = source.unwrap();
                if context.units.get(context.objects, h)?.in_combat() {
                    return Ok(false);
                }
                require(
                    (0..=3).contains(&row.text[0]),
                    "execution.db_script_executor.forced_movement_enum_undefined",
                )?;
                match context.owner.formation_member(h)? {
                    Some(false) => return Ok(false),
                    Some(true) => {
                        // Non-fixed enum range is 0..31, MotionMaster.h:49-79; cast at ScriptMgr.cpp:2313.
                        require(
                            a <= 31,
                            "execution.db_script_executor.movement_type_enum_undefined",
                        )?;
                        Self::call(
                            context,
                            DbScriptCall::FormationMovement(a, b),
                            row,
                            source,
                            target,
                        )?;
                    }
                    None => {}
                }
                if matches!(a, 2 | 3) && target.is_none() && c & 1 != 0 {
                    return Ok(false);
                }
                if matches!(a, 0 | 1 | 2 | 3 | 4 | 18) {
                    Self::call(context, DbScriptCall::Movement, row, source, target)?;
                }
            }
            21 => {
                let h = source.ok_or_else(|| {
                    refuse("execution.db_script_executor.null_active_source_undefined")
                })?;
                if !Self::is(context, Some(h), 3)? {
                    Self::call(context, DbScriptCall::Active(a != 0), row, source, target)?;
                }
            }
            22..=25 | 28 | 29 | 39 | 42 | 44 | 48 => {
                if !Self::is(context, source, 0)? {
                    return Ok(false);
                }
                let h = source.unwrap();
                let call = match row.command {
                    22 => DbScriptCall::Faction(a, b),
                    23 | 24 => {
                        if row.command == 24 && b != 0 {
                            DbScriptCall::MountEntry(a)
                        } else {
                            let display = if a == 0 || additional {
                                a
                            } else {
                                context.owner.choose_display(a)?
                            };
                            if row.command == 23 {
                                DbScriptCall::Morph(display)
                            } else {
                                DbScriptCall::Mount(display)
                            }
                        }
                    }
                    25 => {
                        if context.owner.walking(h)? == (a == 0) {
                            return Ok(false);
                        }
                        DbScriptCall::SetWalk(a == 0)
                    }
                    28 => DbScriptCall::Stand(u32::from(a as u8)),
                    29 | 48 => {
                        let field = if row.command == 29 { 0x93 } else { 0x2e };
                        let old = context.owner.field(h, field)?;
                        let value = match b {
                            0 => old & !a,
                            1 => old | a,
                            2 if old & a != 0 => old & !a,
                            2 => old | a,
                            _ => return Ok(false),
                        };
                        DbScriptCall::WriteField { field, value }
                    }
                    39 => DbScriptCall::Hover(a != 0),
                    42 => {
                        if a != 0 {
                            DbScriptCall::EquipmentDefault
                        } else {
                            for slot in 0..3 {
                                if row.text[slot] >= 0 {
                                    Self::call(
                                        context,
                                        DbScriptCall::VirtualItem(
                                            slot as u8,
                                            row.text[slot] as u32,
                                        ),
                                        row,
                                        source,
                                        target,
                                    )?;
                                }
                            }
                            return Ok(false);
                        }
                    }
                    44 => {
                        if context.owner.entry(h)? == a {
                            return Ok(false);
                        }
                        DbScriptCall::UpdateEntry(a)
                    }
                    _ => unreachable!(),
                };
                Self::call(context, call, row, source, target)?;
            }
            26 => {
                if Self::is(context, source, 0)?
                    && Self::is(context, target, 1)?
                    && context
                        .owner
                        .can_attack_now(source.unwrap(), target.unwrap())?
                {
                    Self::call(context, DbScriptCall::AttackStart, row, source, target)?;
                }
            }
            27 => {
                if !Self::is(context, source, 2)? {
                    return Ok(false);
                }
                let h = source.unwrap();
                let mut flags = context.owner.field(h, 9)?;
                if a & 1 != 0 {
                    flags |= 2;
                } else if a & 2 != 0 {
                    flags &= !2;
                }
                if a & 4 != 0 {
                    flags |= 0x10;
                } else if a & 8 != 0 {
                    flags &= !0x10;
                }
                Self::call(
                    context,
                    DbScriptCall::WriteField {
                        field: 9,
                        value: flags,
                    },
                    row,
                    source,
                    target,
                )?;
            }
            31 => {
                if source.is_none() && target.is_none() && row.flags & 0x10 == 0 {
                    return Ok(true);
                }
                let found = if a != 0 || c != 0 || row.flags & 0x10 != 0 {
                    let Some(mut origin) = source.or(target) else {
                        return Ok(source.is_none() && target.is_none() && row.flags & 0x10 == 0);
                    };
                    if Self::is(context, Some(origin), 3)? {
                        origin = match target {
                            Some(candidate) if !Self::is(context, Some(candidate), 3)? => candidate,
                            _ => origin,
                        };
                    }
                    if row.flags & 0x10 != 0 {
                        target.is_some_and(|h| {
                            context
                                .objects
                                .get(h)
                                .is_ok_and(|o| o.database_guid == row.radius_or_guid)
                        }) && Self::is(context, target, if row.creature_buddy() { 1 } else { 2 })?
                    } else if a != 0 {
                        self.termination_entry_found(origin, a, b, row.creature_buddy(), context)?
                    } else {
                        let mut found = false;
                        for h in context
                            .owner
                            .members(self.map, DbScriptMembers::Pool(c as u16 as u32))?
                            .ok_or_else(|| script_missing("f4_pool_members"))?
                        {
                            if context.objects.get(h)?.alive
                                && (b == 0
                                    || context.owner.within(
                                        origin,
                                        h,
                                        (b as f32).to_bits(),
                                        DbScriptDistance::Bounding,
                                    )?)
                            {
                                found = true;
                                break;
                            }
                        }
                        found
                    }
                } else {
                    buddy_found
                };
                if found == additional {
                    if row.text[0] != 0
                        && Self::is(context, source, 0)?
                        && context.owner.current_motion(source.unwrap())? == 2
                    {
                        Self::call(
                            context,
                            DbScriptCall::WaypointTerminationPause(row.text[0]),
                            row,
                            source,
                            target,
                        )?;
                    }
                    return Ok(true);
                }
            }
            32 => {
                if Self::is(context, source, 0)? {
                    Self::call(
                        context,
                        DbScriptCall::WaypointPause(a != 0),
                        row,
                        source,
                        target,
                    )?;
                }
            }
            34 => {
                let player = Self::player(context, source, target)?;
                let second = if Self::is(context, target, 3)? {
                    source
                } else if Self::is(context, source, 3)? {
                    target
                } else {
                    source
                };
                let terminate = context.owner.condition(a, player, second)? != additional;
                if terminate && b != 0 && player.is_some() {
                    Self::call(
                        context,
                        DbScriptCall::FailQuestGroup(b),
                        row,
                        source,
                        player,
                    )?;
                }
                return Ok(terminate);
            }
            35 => {
                if !Self::is(context, source, 0)? || !Self::is(context, target, 1)? {
                    return Ok(false);
                }
                let source = source.unwrap();
                let target = target.unwrap();
                // ScriptMgr.cpp:2768-2773 dereferences source AI before delivery.
                require(
                    context.units.get(context.objects, source)?.has_ai,
                    "execution.db_script_executor.source_ai_null_undefined",
                )?;
                let event = SourceAiEventType::from(a); // Raw-u32 Deviation is owned by assistance.rs:8.
                if b != 0 {
                    SourceAssistance::send_immediate(
                        source,
                        Some(target),
                        event,
                        c,
                        (b as f32).to_bits(),
                        context.units,
                        context.objects,
                        context.owner,
                    )?;
                } else {
                    let (receiver, sender, invoker) = if Self::is(context, Some(target), 0)? {
                        (target, source, None)
                    } else {
                        (source, target, Some(target))
                    };
                    require(
                        context.units.get(context.objects, receiver)?.has_ai,
                        "execution.db_script_executor.receiver_ai_null_undefined",
                    )?;
                    context.owner.receive_ai_event(
                        receiver,
                        sender,
                        invoker,
                        event,
                        c,
                        context.units,
                        context.objects,
                    )?;
                }
            }
            36 | 37 => {
                if !Self::is(context, source, 0)? || target.is_none() {
                    return Ok(false);
                }
                let call = if row.command == 36 {
                    if a != 0 {
                        DbScriptCall::FacingReset
                    } else {
                        DbScriptCall::FacingTarget
                    }
                } else {
                    if context
                        .units
                        .get(context.objects, source.unwrap())?
                        .in_combat()
                        || a == 0 && source == target
                    {
                        return Ok(false);
                    }
                    if a == 0 && additional {
                        // :2826 leaves z uninitialized; Object.cpp:1639 reads it as a height input.
                        return Err(refuse(
                            "execution.db_script_executor.move_dynamic_uninitialized_z_undefined",
                        ));
                    }
                    let point =
                        context
                            .owner
                            .dynamic_point(row, source.unwrap(), target.unwrap())?;
                    if row.text[2] & 1 != 0 {
                        Self::call(
                            context,
                            DbScriptCall::MotionClear(true),
                            row,
                            source,
                            target,
                        )?;
                    }
                    require(
                        (0..=3).contains(&row.text[0]),
                        "execution.db_script_executor.forced_movement_enum_undefined",
                    )?;
                    DbScriptCall::MoveDynamic(point)
                };
                Self::call(context, call, row, source, target)?;
            }
            38 => {
                if Self::is(context, target, 3)? && (b != 0 || Self::is(context, source, 0)?) {
                    Self::call(context, DbScriptCall::Mail, row, source, target)?;
                }
            }
            40 | 41 | 43 | 52 | 200 => {
                if !Self::is(
                    context,
                    target,
                    match row.command {
                        41 | 52 => 0,
                        200 => 3,
                        _ => 2,
                    },
                )? {
                    return Ok(false);
                }
                let call = match row.command {
                    40 => DbScriptCall::DespawnGo(a),
                    41 => DbScriptCall::Respawn,
                    43 => {
                        if !matches!(context.owner.go_type(target.unwrap())?, 0 | 1) {
                            return Ok(false);
                        }
                        DbScriptCall::ResetGo
                    }
                    52 => DbScriptCall::Gossip(a),
                    _ => DbScriptCall::MeetingStone,
                };
                Self::call(context, call, row, source, target)?;
            }
            45 => {
                let chosen = if b != 0 {
                    self.template_id(b, true, context.owner)? as u32
                } else {
                    a
                };
                if chosen != 0 {
                    let h = source.ok_or_else(|| {
                        refuse("execution.db_script_executor.null_relay_source_undefined")
                    })?;
                    // The declaration's default is NONE, not the definition's stale comment, Map.h:240.
                    self.start(DbScriptType::Relay, chosen, h, target, 0, context)?;
                }
            }
            46 | 47 | 50 | 54 => {
                if !Self::is(context, source, if row.command == 50 { 0 } else { 1 })?
                    || row.command == 46 && !Self::is(context, target, 1)?
                {
                    return Ok(false);
                }
                if row.command == 47 {
                    // CurrentSpellTypes has range 0..7; m_currentSpells has four slots.
                    require(
                        a < 4,
                        "execution.db_script_executor.interrupt_spell_index_undefined",
                    )?;
                }
                if row.command == 54 {
                    // Unit.h:130-136 includes MAX_SHEATH_STATE=3; cast at ScriptMgr.cpp:3297.
                    require(
                        a <= 3,
                        "execution.db_script_executor.sheathe_enum_undefined",
                    )?;
                }
                let call = match row.command {
                    46 => DbScriptCall::CustomSpell,
                    47 => DbScriptCall::Interrupt(a),
                    50 => DbScriptCall::ZoneCombat,
                    _ => DbScriptCall::Sheathe(a),
                };
                Self::call(context, call, row, source, target)?;
            }
            49 => {
                let h = source.ok_or_else(|| {
                    refuse("execution.db_script_executor.null_instance_source_undefined")
                })?;
                if context.owner.has_instance(h)? {
                    Self::call(
                        context,
                        DbScriptCall::InstanceData(a, u64::from(b)),
                        row,
                        source,
                        target,
                    )?;
                }
            }
            51 => {
                let eligible = match a {
                    150 | 151 => Self::is(context, target.or(source), 0)?,
                    100..=102 => Self::is(context, target, 0)?,
                    _ => false,
                };
                if eligible {
                    Self::call(context, DbScriptCall::Formation, row, source, target)?;
                }
            }
            53 => {
                Self::call(
                    context,
                    DbScriptCall::WorldState(row.text[0] as u32, row.text[1] as u32),
                    row,
                    source,
                    target,
                )?;
            }
            55 | 58 => {
                require(
                    source.is_some(),
                    if row.command == 55 {
                        "execution.db_script_executor.null_string_source_undefined"
                    } else {
                        "execution.db_script_executor.null_group_source_undefined"
                    },
                )?;
                Self::call(
                    context,
                    if row.command == 55 {
                        DbScriptCall::StringId(a, b != 0)
                    } else {
                        DbScriptCall::SpawnGroup
                    },
                    row,
                    source,
                    target,
                )?;
            }
            _ => {} // Source default logs unknown opcodes and continues (:3317-3320).
        }
        Ok(false)
    }
}
