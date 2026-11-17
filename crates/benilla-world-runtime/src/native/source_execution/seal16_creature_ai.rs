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
}
impl UnitAiExecutionOwner for Owner<'_, '_, '_> {}

#[path = "seal16_ai_cast.rs"]
mod ai_cast;
struct LiveOwner<'a, 'b, 'c, 'd> {
    random: Owner<'a, 'b, 'c>,
    binding: ai_cast::Binding<'d>,
}
impl CreatureRandom for LiveOwner<'_, '_, '_, '_> {
    fn creature_irand(&mut self, min: i32, max: i32) -> Result<i32> {
        self.random.creature_irand(min, max)
    }
    fn creature_urand(&mut self, min: u32, max: u32) -> Result<u32> {
        self.random.creature_urand(min, max)
    }
    fn creature_urand_default(&mut self) -> Result<u32> {
        self.random.creature_urand_default()
    }
}
impl UnitAiExecutionOwner for LiveOwner<'_, '_, '_, '_> {
    fn add_main_spell(
        &mut self,
        _unit: ObjectHandle,
        spell: u32,
        ai: &mut SourceUnitAi,
        spells: &mut std::collections::BTreeSet<u32>,
        units: &mut SourceUnits,
        objects: &SourceObjects,
    ) -> Result<()> {
        ai.add_main_spell(
            spell,
            spells,
            self.binding.stores,
            &mut main_spell::UnitPowerOwner { objects, units },
        )
    }
    fn with_unit_spell_context<T>(
        &mut self,
        units: &mut SourceUnits,
        ais: &SourceCreatureAis,
        objects: &SourceObjects,
        run: impl FnOnce(&mut SpellStartContext<'_>) -> Result<T>,
    ) -> Result<T> {
        self.binding.run(units, ais, objects, run)
    }
}

pub(super) fn initialize<D: GridDefinitions<Row = SpawnKey>>(
    prefix: &mut Prefix<'_>,
    object: ObjectHandle,
    context: &mut GridFactoryContext<'_, D, SourceTerrainManager>,
) -> Result<()> {
    let selected = prefix.selected_ai.as_ref().unwrap();
    if selected.executable().builtin_kind() != Some(BuiltinCreatureAi::Event) {
        return Err(selected.constructor_refusal(context.objects)?);
    }
    let random = RefCell::new(&mut *prefix.random);
    let mut owner = Owner(&random);
    let base = SourceCreatureAi::construct(
        context.objects,
        object,
        &mut prefix.units,
        &prefix.ai_templates[&selected.inputs().effective_entry],
        false,
        0,
        prefix.ai_config,
        &mut owner,
    )?;
    prefix
        .ais
        .install(context.objects, &mut prefix.units, base, &mut prefix.combat)?;
    prefix.lookups.push("creature_base_ai_constructed");
    prefix.event_ais.insert(
        object,
        SourceEventAi::construct(
            selected.clone(),
            context.objects,
            prefix.event_definitions.clone(),
            EventAiBuild::Release,
        )?,
    );
    prefix.lookups.push("creature_event_ai_constructed");
    let event = prefix.event_ais.get_mut(&object).unwrap();
    let fields = prefix.creature_entry.as_ref().unwrap();
    let mut live_owner = LiveOwner {
        random: Owner(&random),
        binding: ai_cast::Binding {
            object,
            stores: prefix.spell_stores,
            targets: &prefix.spell_targets,
            terrain: context.products,
            world_clock: prefix.world_clock,
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
    execution.spell_list_changed(
        event,
        fields.spell_list_disabled,
        fields.spell_list.as_ref(),
    )?;
    prefix.lookups.push("creature_ai_spell_list_changed");
    event.just_respawned(
        context.objects,
        EventAiRespawnSettings {
            no_aggro_on_sight: selected.inputs().no_aggro_on_sight,
            spawn_defensive: fields.settings.flags[1] & 0x4000000 != 0,
            ignore_combat: fields.settings.flags[0] & 0x2000000 != 0,
        },
        &mut Owner(&random),
        &mut EventAiDispatcher(&mut execution),
    )?;
    prefix.lookups.push("creature_ai_just_respawned");
    if let Some(script) = prefix.instance_data.as_mut() {
        script.creature_respawn()?;
    }
    prefix.lookups.push("creature_instance_respawn_callback");
    if fields.settings.flags[0] & 0x2000 != 0 {
        return Err(refuse(
            "execution.creature.Creature_893.SetLootRecipient_owner",
        ));
    }
    prefix.lookups.push("creature_creator_loot_checked");
    prefix.lookups.push("creature_aim_initialize_return");
    Ok(())
}
