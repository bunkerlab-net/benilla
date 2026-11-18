// SPDX-License-Identifier: GPL-3.0-or-later
//! CreatureAIImpl.h:26-29; Creature.cpp:883-895 uses the live Unit and shared AI owners.
use super::*;
use std::cell::RefCell;

struct Owner<'a, 'b, 'c>(&'a RefCell<&'b mut (dyn CreatureRandom + 'c)>);
impl CreatureRandom for Owner<'_, '_, '_> {
    fn creature_irand(&mut self, min: i32, max: i32) -> Result<i32> {
        self.0.borrow_mut().creature_irand(min, max)
    }
    fn creature_urand(&mut self, min: u32, max: u32) -> Result<u32> {
        self.0.borrow_mut().creature_urand(min, max)
    }
    fn creature_urand_default(&mut self) -> Result<u32> {
        // CreatureEventAI.cpp:607,618 retains the map thread's full-width draw.
        self.0.borrow_mut().creature_urand_default()
    }
    fn creature_raw_position(&self) -> Result<u64> {
        self.0.borrow().creature_raw_position()
    }
}
impl UnitAiExecutionOwner for Owner<'_, '_, '_> {}

#[path = "seal16_ai_cast.rs"]
mod ai_cast;
struct LiveOwner<'a> {
    binding: ai_cast::Binding<'a>,
}
impl CreatureRandom for LiveOwner<'_> {
    fn creature_irand(&mut self, min: i32, max: i32) -> Result<i32> {
        self.binding.random.creature_irand(min, max)
    }
    fn creature_urand(&mut self, min: u32, max: u32) -> Result<u32> {
        self.binding.random.creature_urand(min, max)
    }
    fn creature_urand_default(&mut self) -> Result<u32> {
        self.binding.random.creature_urand_default()
    }
}
impl UnitAiExecutionOwner for LiveOwner<'_> {
    fn add_main_spell(
        &mut self,
        _unit: ObjectHandle,
        spell: u32,
        ai: &mut SourceUnitAi,
        spells: &mut std::collections::BTreeSet<u32>,
        units: &SourceUnits,
        objects: &SourceObjects,
    ) -> Result<()> {
        ai.add_main_spell(
            spell,
            spells,
            self.binding.stores,
            &mut main_spell::UnitPowerOwner {
                objects,
                units: main_spell::UnitPowerSource::Units(units),
            },
        )
    }
    fn with_unit_spell_context<T>(
        &mut self,
        units: &mut SourceUnits,
        ais: &mut SourceCreatureAis,
        objects: &SourceObjects,
        run: impl FnOnce(&mut SpellStartContext<'_>) -> Result<T>,
    ) -> Result<T> {
        self.binding.run(units, ais, objects, run)
    }
}

pub(super) fn construct(
    prefix: &mut Prefix<'_>,
    state: &mut CreatureConstruction,
    objects: &SourceObjects,
) -> Result<()> {
    let selected = state
        .selected_ai()
        .ok_or_else(|| refuse("execution.ai.selection_absent"))?;
    let object = state.object;
    prefix.lookups.push("creature_ai_selected");
    eprintln!(
        "F6_CREATURE_AI map={} entry={} input={:?} branch={:?} executable={:?}",
        prefix.map_entry.id,
        state.template.entry,
        selected.inputs(),
        selected.branch(),
        selected.executable()
    );
    prefix.selected_ai = Some(selected.clone());
    state.construct_event_ai_with_base(
        objects,
        prefix.event_definitions.clone(),
        EventAiBuild::Release,
        |selected| {
            let template = prefix
                .ai_templates
                .get(&selected.inputs().effective_entry)
                .ok_or_else(|| refuse("execution.unit_ai.effective_entry"))?;
            let random = RefCell::new(&mut *prefix.random);
            prefix.ais.install_loaded_event_ai_base(
                objects,
                &mut prefix.units,
                &mut prefix.combat,
                selected,
                template,
                prefix.ai_config,
                &mut Owner(&random),
            )?;
            prefix.lookups.push("creature_base_ai_constructed");
            Ok(())
        },
    )?;
    prefix.event_ais.insert(
        object,
        state
            .take_event_ai()
            .ok_or_else(|| refuse("execution.event_ai.generation_missing"))?,
    );
    prefix.lookups.push("creature_event_ai_constructed");
    Ok(())
}

pub(super) fn callback<D: GridDefinitions<Row = SpawnKey>>(
    prefix: &mut Prefix<'_>,
    phase: CreaturePhase,
    object: ObjectHandle,
    context: &mut GridFactoryContext<'_, D, SourceTerrainManager>,
) -> Result<()> {
    let event = prefix
        .event_ais
        .get_mut(&object)
        .ok_or_else(|| refuse("execution.event_ai.generation_missing"))?;
    let random = RefCell::new(&mut *prefix.random);
    let mut live_owner = LiveOwner {
        binding: ai_cast::Binding {
            object,
            stores: prefix.spell_stores,
            targets: &prefix.spell_targets,
            terrain: context.products,
            world_clock: prefix.world_clock,
            clocks: prefix.clocks,
            random: &mut Owner(&random),
            creature_spells: prefix.creature_spells,
            max_casts: super::addon_cast::max_casts(prefix.config)?,
            broadcast: Some((context.cells, prefix.map_visibility_bits)),
        },
    };
    let mut execution = SourceAiExecution {
        queries: SourceUnitQueries {
            objects: context.objects,
            units: &mut prefix.units,
        },
        ais: &mut prefix.ais,
        threats: &mut prefix.threats,
        owner: &mut live_owner,
    };
    match phase {
        CreaturePhase::AiSpellListChanged => {
            execution.spell_list_changed(event)?;
            prefix.lookups.push("creature_ai_spell_list_changed");
        }
        CreaturePhase::AiJustRespawned => {
            execution.event_ai_just_respawned(event, &mut Owner(&random))?;
            prefix.lookups.push("creature_ai_just_respawned");
        }
        _ => return Err(phase.missing_owner()),
    }
    Ok(())
}

pub(super) fn finish(
    prefix: &mut Prefix<'_>,
    object: ObjectHandle,
    objects: &SourceObjects,
) -> Result<()> {
    finish_source_creature_ai_initialization(
        objects,
        &prefix.units,
        object,
        prefix.instance_data.as_deref_mut(),
        |notice| {
            prefix.lookups.push(match notice {
                CreatureAiInitializeObservation::InstanceRespawn => {
                    "creature_instance_respawn_callback"
                }
                CreatureAiInitializeObservation::CreatorLoot => "creature_creator_loot_checked",
            })
        },
        |_| {
            Err(refuse(
                "execution.creature.Creature_893.SetLootRecipient_owner",
            ))
        },
    )?;
    prefix.lookups.push("creature_aim_initialize_return");
    Ok(())
}
