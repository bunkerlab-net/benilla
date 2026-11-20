// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! Creature.cpp:2343, Unit.cpp:1453-1512. Fresh Create precedes AI and AddToWorld.
use super::*;

#[path = "seal16_addon_effects.rs"]
mod effects;
use crate::native::source_execution::visibility::SourceBroadcast;
pub(super) use effects::spell_start_packet;
pub(super) struct Execution<'a> {
    pub object: ObjectHandle,
    pub objects: &'a SourceObjects,
    pub terrain: &'a mut SourceTerrainManager,
    pub stores: &'a SourceSpellStores,
    pub targets: &'a SourceSpellTargetMgr,
    pub unit: SpellUnitState,
    pub level: u32,
    pub clocks: &'a mut ActiveClocks,
    pub world_clock: &'a SourceWorldClock,
    pub random: &'a mut dyn CreatureRandom,
    pub creature_spells: Option<&'a CreatureSpellMapData>,
    pub raw: u64,
    pub max_casts: u32,
}
impl CreatureAddonExecutionOwner for Execution<'_> {
    fn cast_spell(
        &mut self,
        state: &mut CreatureAddonState,
        live: &mut SourceUnitFields,
        spell: u32,
    ) -> Result<()> {
        let invisibility = self.object.thread == 6 && spell == 4986 && live.entry == 3094;
        let before = (invisibility || matches!(spell, 9036 | 1032))
            .then(|| serde_json::to_vec(&*live).unwrap());
        if self.object.thread == 7 && self.objects.get(self.object)?.database_guid == 27677 {
            // Catalog04 template11577/ordinal6753 and spawnTemplate0 leave every flag clear.
            assert_eq!(live.flags, 0);
            assert_eq!(live.faction, 91);
            assert_eq!(live.static_flags, [0; 4]);
            assert_eq!(live.persuaded, None);
            assert_eq!(live.control_guids, SourceControlGuids::default());
        }
        let (cast, starts, events) = live.borrow_cast();
        let mut owner = Owner {
            object: self.object,
            objects: self.objects,
            terrain: self.terrain,
            stores: self.stores,
            targets: self.targets,
            flags: cast.flags,
            control_guids: cast.control_guids,
            immunities: cast.immunities,
            auras: cast.auras,
            visibility: cast.visibility,
            resistances: cast.resistances,
            creature_damage: cast.creature_damage,
            loaded_creature: cast.loaded_creature,
            can_modify_stats: cast.can_modify_stats,
            threat_modifier: cast.threat_modifier,
            power_fields: cast.power_fields,
            power_type: cast.power_type.as_mut(),
            vitals: cast.vitals,
            form: cast.shapeshift_form,
            cooldowns: cast.cooldowns,
            creature_spell_list: cast.spell_list.as_ref(),
            entry: cast.entry,
            random: self.random,
            creature_spells: self.creature_spells,
            stand: *cast.stand_state,
            mount: state.mount,
            unit: self.unit,
            level: self.level,
            cast_speed: cast.cast_speed,
            clocks: self.clocks,
            world_clock: self.world_clock,
            max_casts: self.max_casts,
            broadcast: None,
            bounding_radius: *cast.bounding_radius,
        };
        // Creature.cpp:2343 ignores the cast result. A refusal still stops here.
        SpellStartContext {
            stores: self.stores,
            starts,
            events,
            owner: &mut owner,
        }
        .cast_triggered(self.object, Some(self.object), spell, None)?;
        if let Some(before) = before {
            assert_eq!(
                before,
                serde_json::to_vec(&*live).unwrap(),
                "applying {spell} must not change saved SourceUnitFields bytes"
            );
        }
        if invisibility {
            // SQL :433653,:841741; Creature.cpp:932,1738,2343.
            assert!(!self.objects.get(self.object)?.in_world);
            assert_eq!(
                live.visibility.mode,
                SourceUnitVisibilityMode::GroupInvisibility
            );
            assert_eq!(live.visibility.mask, 1);
            assert_eq!(live.visibility.values[0], 200);
            assert_eq!(live.visibility.values[1..], [0; 31]);
            let holder = live.auras.first_aura_holder(spell).unwrap();
            assert!(holder.ready && holder.passive);
            let aura = holder.effects[0].as_ref().unwrap();
            assert_eq!(
                (aura.kind, aura.misc, aura.amount, aura.applied),
                (18, 0, 200, true)
            );
        }
        if self.object.thread == 6 && self.objects.get(self.object)?.database_guid == 40520 {
            assert_eq!(self.raw, 27080);
            assert!(state.duplicate_aura_errors.is_empty());
            let id = if spell == 9036 { 1 } else { 2 };
            assert_eq!(live.spell_starts.spells.len(), id as usize);
            let value = &live.spell_starts.spells[&id];
            assert_eq!(value.spell, if id == 1 { 9036 } else { 10848 });
            assert_eq!(value.state, SourceSpellState::Finished);
            assert_eq!(value.proc_event_lookups, if id == 1 { 1 } else { 4 });
            assert_eq!(value.unique_targets.len(), 1);
            let target = &value.unique_targets[0];
            assert_eq!(
                (target.unit, target.effect_mask, target.hit_mask),
                (self.object, 1, 1)
            );
            assert!(target.processed && !target.missed);
            assert_eq!((target.processed_mask, target.duration), (0, -1));
            assert_eq!(
                value.damage_per_effect,
                if id == 1 { [-20, 0, 0] } else { [0; 3] }
            );
            assert_eq!(value.damage_multipliers, [1f32.to_bits(), 0, 0]);
            assert!(value.spell_go.is_some());
            let queue = live.unit_events.inventory();
            assert_eq!(queue.clock_ms, 0);
            assert!(queue.detached.is_empty());
            assert_eq!(queue.queued.len(), id as usize);
            let event = &queue.queued[id as usize - 1];
            assert_eq!(value.event, Some(event.event_id));
            assert_eq!(event.kind, SourceUnitEvent::Spell { spell: id });
            assert_eq!(
                (event.add_time_ms, event.exec_time_ms, event.to_abort),
                (0, 1, false)
            );
            let aura = live.auras.first_aura_holder(spell).unwrap();
            assert!(aura.permanent && aura.ready && aura.skip_update && !aura.deleted);
            assert_eq!((aura.caster, aura.target), (self.object, self.object));
            assert_eq!(
                (aura.duration, aura.max_duration, aura.charges, aura.stack),
                (-1, -1, 0, 1)
            );
            assert_eq!((aura.apply_time, aura.apply_ms), (1794314190, 0));
            assert_eq!(aura.slot, if id == 1 { 255 } else { 0 });
            assert_eq!(aura.passive, id == 1);
            assert_eq!(aura.death_persistent, id == 1);
            assert!(!aura.removed_on_shape_lost);
            assert_eq!(aura.level, 1);
            let effect = aura.effects[0].as_ref().unwrap();
            assert!(effect.applied);
            assert_eq!(effect.kind, if id == 1 { 95 } else { 4 });
            assert_eq!(
                (effect.amount, effect.base_points),
                if id == 1 { (-20, -20) } else { (0, 0) }
            );
            assert_eq!(
                (
                    effect.periodic_time,
                    effect.periodic_timer,
                    effect.periodic_tick
                ),
                (0, 0, 0)
            );
            assert_eq!(effect.positive, id != 1);
            assert_eq!(effect.apply_time, 1794314190);
            assert!(aura.effects[1..].iter().all(Option::is_none));
            assert_eq!(live.auras.vis_flags, 1);
            assert_eq!(live.auras.cast_counter, 0);
            assert!(!live.auras.spell_procs_happening);
            let t = self.stores.template(spell).unwrap();
            assert!(live.cooldowns.is_empty());
            assert_eq!((t.procflags, t.procchance, t.proccharges), (0, 101, 0));
            assert!(self.stores.proc_event(spell).is_none());
            if id == 2 {
                assert_eq!(
                    live.auras.aura_spell_ids().collect::<Vec<_>>(),
                    [9036, 10848]
                );
                assert_eq!(live.auras.aura_type_ids().collect::<Vec<_>>(), [4, 95]);
                assert_eq!(
                    live.auras.first_aura_holder(9036).unwrap().effects[0]
                        .as_ref()
                        .unwrap()
                        .amount,
                    -20
                );
                assert_eq!(live.auras.aura_fields.spells[0], 10848);
                assert!(live.auras.aura_fields.spells[1..].iter().all(|v| *v == 0));
                assert_eq!(live.auras.aura_fields.flags[0], 5);
                assert_eq!(live.auras.aura_fields.levels[0], self.level);
                assert_eq!(live.auras.aura_fields.applications, [0; 12]);
            }
            println!("F6_ADDON_APPLIED guid=40520 spell={spell} finished=true proc_fields=[0,101,0] proc_table_lookups={} raw={} additional_words=0 additional_sql=0", value.proc_event_lookups, self.raw);
        }
        Ok(())
    }
}
pub(super) struct Owner<'a> {
    pub object: ObjectHandle,
    pub objects: &'a SourceObjects,
    pub terrain: &'a mut SourceTerrainManager,
    pub stores: &'a SourceSpellStores,
    pub targets: &'a SourceSpellTargetMgr,
    pub flags: &'a u32,
    pub control_guids: &'a SourceControlGuids,
    pub immunities: &'a CreatureImmunityState,
    pub auras: &'a mut SourceUnitAuras,
    pub visibility: &'a mut SourceUnitVisibility,
    pub resistances: &'a mut SourceUnitResistances,
    pub creature_damage: &'a mut SourceCreatureDamage,
    pub loaded_creature: &'a SourceLoadedCreatureFields,
    pub can_modify_stats: &'a bool,
    pub threat_modifier: &'a mut [f32; 7],
    pub power_fields: &'a mut Option<SourceUnitPowerFields>,
    pub power_type: Option<&'a mut u8>,
    pub vitals: &'a mut Option<SourceUnitVitals>,
    pub form: &'a mut u32,
    pub cooldowns: &'a mut SourceSpellCooldowns,
    pub stand: u8,
    pub mount: u32,
    pub unit: SpellUnitState,
    pub level: u32,
    pub cast_speed: &'a u32,
    pub creature_spell_list: Option<&'a CreatureSpellList>,
    pub entry: &'a u32,
    pub random: &'a mut dyn CreatureRandom,
    pub creature_spells: Option<&'a CreatureSpellMapData>,
    pub clocks: &'a mut ActiveClocks,
    pub world_clock: &'a SourceWorldClock,
    pub max_casts: u32,
    pub broadcast: Option<(&'a SourceCells, u32)>,
    pub bounding_radius: Option<u32>,
}

impl<'a> Owner<'a> {
    pub(super) fn aura_target(&self) -> SelfAuraTarget<'a> {
        SelfAuraTarget {
            target: self.object,
            level: Some(self.level),
            alive: self.unit.alive,
            player: self.unit.player,
            player_group: None,
            totem: self.unit.totem,
            objects: self.objects,
            control_guids: self.control_guids,
        }
    }
}

impl SelfAuraApplicationOwner for Owner<'_> {
    fn self_aura_application(&mut self) -> SelfAuraApplication<'_> {
        SelfAuraApplication {
            unit: self.aura_target(),
            auras: self.auras,
            visibility: self.visibility,
            resistances: self.resistances,
            damage: DamageAuraTarget::new(
                self.creature_damage,
                self.loaded_creature,
                *self.flags,
                *self.form,
            ),
            can_modify_stats: *self.can_modify_stats,
            threat_modifier: self.threat_modifier,
            power_fields: self.power_fields,
            power_type: self.power_type.as_deref_mut(),
            vitals: self.vitals,
            form: self.form,
        }
    }
}

pub(super) fn broadcast(
    objects: &SourceObjects,
    object: ObjectHandle,
    binding: Option<(&SourceCells, u32)>,
    radius: Option<u32>,
) -> Result<SourceBroadcast> {
    if !objects.get(object)?.in_world {
        return Ok(SourceBroadcast::OutOfWorld);
    }
    let (cells, map_distance_bits) =
        binding.ok_or_else(|| refuse("execution.spells.Map_541.broadcast_grid_owner"))?;
    objects.message_broadcast(object, cells, map_distance_bits, radius)
}

impl SourceRelationOwner for Owner<'_> {
    fn relation_flags(&self, unit: ObjectHandle) -> Result<u32> {
        require(
            unit == self.object,
            "execution.relations.addon_other_unit_flags_owner",
        )?;
        Ok(*self.flags)
    }
    fn relation_master(&self, unit: ObjectHandle) -> Result<Option<ObjectHandle>> {
        require(
            unit == self.object,
            "execution.relations.Unit_6582.addon_other_unit_fields_owner",
        )?;
        Ok(self
            .control_guids
            .get_master(self.objects, unit, None, unbound_pet_loading)?
            .unit())
    }
}
impl SpellLocationOwner for Owner<'_> {}
impl SpellExecutionOwner for Owner<'_> {
    fn unit(&mut self, unit: ObjectHandle) -> Result<SpellUnitState> {
        require(
            unit == self.object,
            "execution.creature_addon.other_unit_fields_owner",
        )?;
        self.objects.get(unit)?;
        let mut value = self.unit;
        value.standing = self.stand == 0;
        value.mounted = self.mount != 0;
        value.form = *self.form;
        value.stealth = self.auras.has_aura_type(16);
        Ok(value)
    }
    fn map_time(&mut self, caster: ObjectHandle) -> Result<u32> {
        self.unit(caster)?;
        // Map.cpp:78-80; World.cpp:107,1389,1511. Startup precedes World::Update.
        Ok(0)
    }
    fn positive_definition(&mut self, t: &SpellTemplate) -> Result<bool> {
        positive_unit_spell(self.stores, t, 7, None)
    }
    fn targeting_data(&mut self, spell: u32) -> Result<[[u32; 2]; 3]> {
        self.targets
            .get_spell_targeting_data(spell)
            .map(|data| {
                data.filtering_scheme
                    .map(|row| row.map(|scheme| scheme as u32))
            })
            .map_err(|error| refuse(&format!("execution.spells.targeting.{error}")))
    }
    fn terrain_geometry(&mut self) -> Result<(&SourceObjects, &mut SourceTerrainManager)> {
        Ok((self.objects, self.terrain))
    }
    fn casting_source(&mut self, spell: &SourceSpell) -> Result<Option<[u32; 3]>> {
        let position = self.unit(spell.caster)?.position;
        Ok(Some(position[..3].try_into().unwrap()))
    }
    fn check(
        &mut self,
        spell: &mut SourceSpell,
        t: &SpellTemplate,
        check: SpellCheck,
    ) -> Result<SpellCastResult> {
        use SpellCastResult as R;
        match check {
            SpellCheck::TargetFilter(index) => {
                let result =
                    spell.check_target_filter(t, usize::from(index), self.objects, self)?;
                if self.object.thread == 7
                    && self.objects.get(self.object)?.database_guid == 27677
                    && t.id == 18148
                {
                    assert_eq!(result, R::Ok);
                    assert_eq!(spell.targets.unit, Some(self.object));
                    assert_eq!(
                        (spell.caster, spell.original_caster),
                        (self.object, self.object)
                    );
                    assert_eq!(*self.flags, 0);
                    assert_eq!(unit_reaction_to(self.object, self.object, self)?, 4);
                    println!("F6_RELATIONS guid=27677 flags=0 identity_exit=Relations.cpp:204 assist_exit=Relations.cpp:504 raw={} relation_words=0 relation_sql=0 relation_clock=0", self.random.creature_raw_position()?);
                }
                Ok(result)
            }
            SpellCheck::PositiveImmunity(mask) => {
                // Spell.cpp:4926 evaluates IsPositiveSpell even for an empty mask.
                let positive =
                    positive_unit_spell(self.stores, t, 7, Some((spell.caster, self.object)))?;
                Ok(
                    if positive
                        && mask != 0
                        && self.immunities.immune_to_self_spell(
                            self.stores,
                            t,
                            mask,
                            self.object,
                            self.auras,
                        )?
                    {
                        R::TargetAuraState
                    } else {
                        R::Ok
                    },
                )
            }
            SpellCheck::SpiritOfRedemption => Ok(
                if self.auras.has_aura_type(176) && t.attributesex3 & 0x0010_0000 == 0 {
                    R::BadTargets
                } else {
                    R::Ok
                },
            ),
            SpellCheck::StrongerAura(_) if !self.auras.has_aura_holders() => Ok(R::Ok),
            SpellCheck::AllowedInLocation(location) => {
                let player =
                    spell_beneficiary_from_guids(self.objects, self.object, *self.control_guids)?;
                let map = self
                    .objects
                    .get(self.object)?
                    .map
                    .ok_or_else(|| refuse("execution.spells.location.WorldObject_GetMapId_owner"))?
                    .map;
                self.stores
                    .allowed_in_location(t, map, location[0], location[1], player, self)
            }
            SpellCheck::Items => spell.check_items(t, false, self),
            _ => Err(check.missing_owner()),
        }
    }
    fn call(&mut self, spell: &mut SourceSpell, call: SpellCall) -> Result<()> {
        if call != SpellCall::PrepareTriggerData {
            return Err(call.missing_owner());
        }
        spell.prepare_trigger_data(self.stores)
    }
    fn cast_time(&mut self, spell: &mut SourceSpell, t: &SpellTemplate) -> Result<i32> {
        let mod_owner = world_object_spell_mod_owner(self.objects, spell.caster, self.unit.totem)?;
        spell.creature_cast_time(
            t,
            self.stores,
            mod_owner,
            || Ok(self.level),
            || Ok(*self.cast_speed),
        )
    }
    fn cast_scheduled(
        &mut self,
        spell: &mut SourceSpell,
        skip: bool,
        starts: &mut SourceSpellStarts,
        events: &mut SourceUnitEvents,
    ) -> Result<()> {
        self.assert_cast_entry(spell)?;
        spell.cast_self(
            skip,
            SelfCastCaller::TriggeredUnit,
            self.stores,
            starts,
            events,
            self,
        )
    }
}

pub(super) fn max_casts(config: &[u8]) -> Result<u32> {
    let (_, value) = super::super::super::super::super::spawn_loader_config::assignment(
        config,
        "MaxSpellCastsInChain",
    )?;
    value
        .parse()
        .map_err(|_| refuse("execution.spells.MaxSpellCastsInChain_config"))
}

struct CooldownBinding<'a> {
    object: ObjectHandle,
    objects: &'a SourceObjects,
    control_guids: &'a SourceControlGuids,
    entry: u32,
    list: Option<&'a CreatureSpellList>,
    definitions: Option<&'a CreatureSpellMapData>,
    stores: &'a SourceSpellStores,
    random: &'a mut dyn CreatureRandom,
    clock: &'a SourceWorldClock,
    totem: bool,
    observation: &'a mut CreatureCooldownObservation,
}
impl CreatureCooldownOwner for CooldownBinding<'_> {
    fn caster_kind(&self) -> Result<SourceObjectKind> {
        Ok(self.objects.get(self.object)?.kind)
    }
    fn spell_cooldown(&mut self, spell: u32) -> Result<CreatureCooldownMatch> {
        self.observation.searches += 1;
        let result = creature_spell_cooldown(self.list, spell, self.random)?;
        self.observation.result = Some(result);
        Ok(result)
    }
    fn creature_cooldown(&mut self, spell: u32) -> Result<Option<u32>> {
        let result = creature_cooldown(self.definitions, self.entry, spell, self.random)?;
        self.observation.fallback = Some(result);
        Ok(result)
    }
    fn spell_mod_owner(&self) -> Result<Option<ObjectHandle>> {
        world_object_spell_mod_owner(self.objects, self.object, self.totem)
    }
    fn clock(&self) -> Result<i64> {
        self.objects
            .get(self.object)?
            .map
            .ok_or_else(|| refuse("execution.spells.Creature_3138.map_clock_owner"))?;
        Ok(self.clock.current_ms())
    }
    fn category_flags(&self, category: u32) -> Result<Option<u32>> {
        Ok(self
            .stores
            .dbc("SpellCategory.dbc", category)?
            .map(|row| row.words[&1]))
    }
    fn master(&mut self) -> Result<Option<ObjectHandle>> {
        let lookup =
            self.control_guids
                .get_master(self.objects, self.object, None, unbound_pet_loading)?;
        self.observation.master = Some(lookup);
        Ok(lookup.unit())
    }
    fn is_player(&self, unit: ObjectHandle) -> Result<bool> {
        Ok(self.objects.get(unit)?.kind == SourceObjectKind::Player)
    }
}

impl Owner<'_> {
    fn send_spell_cooldown(&mut self, spell: &mut SourceSpell, t: &SpellTemplate) -> Result<()> {
        self.cooldowns.send_spell_cooldown(
            t,
            spell.flags.channel_only(),
            &mut CooldownBinding {
                object: self.object,
                objects: self.objects,
                control_guids: self.control_guids,
                entry: *self.entry,
                list: self.creature_spell_list,
                definitions: self.creature_spells,
                stores: self.stores,
                random: self.random,
                clock: self.world_clock,
                totem: self.unit.totem,
                observation: &mut spell.creature_cooldown,
            },
        )
    }
}
