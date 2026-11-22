// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! CreatureAISelector.cpp:38-90. Selection records an executable, never an AI body.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceCreatureAiScriptObservation {
    RegistryLookup,
    NullFactory,
    BuiltinRegistry,
}

#[derive(Debug, PartialEq, Eq)]
pub enum LoadedCreatureAiScriptFactory<'a> {
    Null,
    Ported(&'a SourceScriptDefinition),
}

/// CreatureAISelector.cpp:40-45 and ScriptDevAIMgr.cpp:363-370 for an ordinary DB creature.
pub fn probe_loaded_creature_ai_script<'a>(
    objects: &SourceObjects,
    object: ObjectHandle,
    scripts: &'a SourceScriptRegistry,
    script_id: u32,
    mut observe: impl FnMut(SourceCreatureAiScriptObservation),
) -> Result<LoadedCreatureAiScriptFactory<'a>> {
    require(
        objects.get(object)?.kind == SourceObjectKind::Creature,
        "execution.ai.loaded_script_creature_kind",
    )?;
    let script = scripts.get_script(script_id)?;
    observe(SourceCreatureAiScriptObservation::RegistryLookup);
    if let Some(script) = script {
        let factory = script
            .slots
            .get("GetAI")
            .ok_or_else(|| refuse("execution.creature.ScriptDevAIMgr_367.GetAI_slot_owner"))?;
        if *factory {
            let name = std::str::from_utf8(&script.name).map_err(|_| {
                refuse("execution.ai.ScriptDevAIMgr_370.script_name_encoding_owner")
            })?;
            SourceScriptedAiKind::for_registered_name(name)?;
            return Ok(LoadedCreatureAiScriptFactory::Ported(script));
        }
    }
    observe(SourceCreatureAiScriptObservation::NullFactory);
    Ok(LoadedCreatureAiScriptFactory::Null)
}

impl CreatureAiInputs {
    /// CreatureAISelector.cpp:40-83 reads these operands from the current creature template and unit.
    pub fn loaded_db_creature(
        template: &LoadedCreatureInfo,
        live: &SourceUnitFields,
        scripts: &SourceScriptRegistry,
        factions: &CreatureAiFactions,
    ) -> Result<Self> {
        require(
            live.entry == template.words[0],
            "execution.ai.effective_entry",
        )?;
        let ai_name = std::str::from_utf8(&template.strings[2])
            .map_err(|_| refuse("execution.creature.GetAIName_encoding_owner"))?;
        let script_name = std::str::from_utf8(
            scripts
                .names()
                .get(template.words[96] as usize)
                .ok_or_else(|| refuse("execution.creature.ScriptName_index_owner"))?,
        )
        .map_err(|_| refuse("execution.creature.ScriptName_encoding_owner"))?;
        Ok(Self {
            effective_entry: live.entry,
            ai_name: ai_name.into(),
            script_name: script_name.into(),
            control: CreatureAiControl {
                pet: live.pet_kind,
                player_controlled: live.flags & 8 != 0,
                temporary_summon: live.temporary_spawn.is_some(),
                totem: live.totem,
            },
            guard: template.words[23] & 0x400 != 0,
            no_aggro_on_sight: template.words[23] & 2 != 0,
            neutral_to_all: factions.neutral_to_all(live.faction),
        })
    }
}
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum BuiltinCreatureAi {
    Null,
    Guard,
    Pet,
    Totem,
    Event,
    Guardian,
    Possessed,
}
impl BuiltinCreatureAi {
    pub fn key(self) -> &'static str {
        match self {
            Self::Null => "NullAI",
            Self::Guard => "GuardAI",
            Self::Pet => "PetAI",
            Self::Totem => "TotemAI",
            Self::Event => "EventAI",
            Self::Guardian => "GuardianAI",
            Self::Possessed => "PossessedAI",
        }
    }
    pub fn constructor_refusal(self) -> Refusal {
        refuse(match self {
            Self::Null => "execution.ai.null_constructor_owner",
            Self::Guard => "execution.ai.CreatureAIImpl_29.GuardAI_factory_context_owner",
            Self::Pet => "execution.ai.pet_constructor_owner",
            Self::Totem => "execution.ai.totem_constructor_owner",
            Self::Event => "execution.ai.event_ai_constructor_owner",
            Self::Guardian => "execution.ai.guardian_constructor_owner",
            Self::Possessed => "execution.ai.possessed_constructor_owner",
        })
    }
    fn permit(self, input: &CreatureAiInputs) -> i32 {
        // UnitAI.h:639-644; each class's Permissible, not event-row presence.
        match self {
            Self::Null => 1,                                 // NullCreatureAI.h:38
            Self::Guard if input.guard => 800,               // GuardAI.cpp:24-29
            Self::Pet if input.control.pet.is_some() => 800, // PetAI.cpp:29-34
            Self::Guardian if input.control.pet == Some(CreaturePetKind::Guardian) => 800,
            Self::Totem if input.control.totem => 200, // TotemAI.cpp:28-33
            Self::Event if input.ai_name == "EventAI" => 800,
            Self::Event if input.no_aggro_on_sight || input.neutral_to_all => 100,
            Self::Event => 200, // CreatureEventAI.cpp:51-62
            _ => -1,            // GuardianAI.cpp:76-81; PossessedAI.h:37
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum CreaturePetKind {
    Controlled,
    Guardian,
    Other,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct CreatureAiControl {
    pub pet: Option<CreaturePetKind>,
    pub player_controlled: bool,
    pub temporary_summon: bool,
    pub totem: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CreatureAiInputs {
    pub effective_entry: u32,
    pub ai_name: String,
    pub script_name: String,
    pub control: CreatureAiControl,
    pub guard: bool,
    pub no_aggro_on_sight: bool,
    pub neutral_to_all: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BuiltinAiBinding {
    pub key: String,
    pub kind: BuiltinCreatureAi,
    pub registration: u64,
}
/// Identity of the selected GetAI slot. No process address or invented return value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ScriptDevAiBinding {
    pub script_id: u32,
    pub name: String,
    pub registration: u64,
    pub operation: String,
    pub source_file: String,
    pub source_line: u32,
    pub source_function: String,
    pub registry_sha256: Sha256Id,
    pub get_ai_nonnull: bool,
}
fn shared<T: Serialize, S: serde::Serializer>(
    value: &Arc<T>,
    s: S,
) -> std::result::Result<S::Ok, S::Error> {
    value.as_ref().serialize(s)
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum AiExecutable {
    Builtin(#[serde(serialize_with = "shared")] Arc<BuiltinAiBinding>),
    ScriptDevGetAi(#[serde(serialize_with = "shared")] Arc<ScriptDevAiBinding>),
    /// Direct new NullCreatureAI, not a failed lookup of the registered "NullAI".
    NullFallback,
}
impl AiExecutable {
    pub fn builtin_kind(&self) -> Option<BuiltinCreatureAi> {
        match self {
            Self::Builtin(binding) => Some(binding.kind),
            Self::NullFallback => Some(BuiltinCreatureAi::Null),
            Self::ScriptDevGetAi(_) => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum AiSelectionBranch {
    ScriptDev,
    ControlledPet,
    Guardian,
    Charm,
    Totem,
    ExplicitName,
    Guard,
    Permit,
}
/// F7 component inventory version 1. Fields are immutable after selection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SelectedCreatureAi {
    version: u32,
    object: ObjectHandle,
    database_guid: u32,
    runtime_guid: Option<SourceGuid>,
    map: Option<MapIdentity>,
    inputs: CreatureAiInputs,
    branch: AiSelectionBranch,
    executable: AiExecutable,
}
impl SelectedCreatureAi {
    fn new(
        source: &SourceObject,
        inputs: CreatureAiInputs,
        branch: AiSelectionBranch,
        executable: AiExecutable,
    ) -> Self {
        Self {
            version: 1,
            object: source.handle,
            database_guid: source.database_guid,
            runtime_guid: source.guid,
            map: source.map,
            inputs,
            branch,
            executable,
        }
    }

    pub fn object(&self) -> ObjectHandle {
        self.object
    }
    pub fn inputs(&self) -> &CreatureAiInputs {
        &self.inputs
    }
    pub fn branch(&self) -> AiSelectionBranch {
        self.branch
    }
    pub fn executable(&self) -> &AiExecutable {
        &self.executable
    }
    /// Validate the selected unit's construction before constructor dispatch.
    pub fn validate(&self, objects: &SourceObjects) -> Result<()> {
        self.validate_object(objects)?;
        validate_control(objects, self.object, &self.inputs.control)
    }
    pub(crate) fn validate_object(&self, objects: &SourceObjects) -> Result<()> {
        let object = objects.get(self.object)?;
        require(
            self.version == 1
                && object.database_guid == self.database_guid
                && object.guid == self.runtime_guid
                && object.map == self.map,
            "execution.ai.spawn_context_changed",
        )?;
        Ok(())
    }
    /// Selection alone cannot execute even NullCreatureAI's CreatureAI base constructor.
    pub fn constructor_refusal(&self, objects: &SourceObjects) -> Result<Refusal> {
        self.validate(objects)?;
        Ok(match &self.executable {
            AiExecutable::Builtin(binding) => binding.kind.constructor_refusal(),
            AiExecutable::NullFallback => BuiltinCreatureAi::Null.constructor_refusal(),
            AiExecutable::ScriptDevGetAi(_) => {
                refuse("execution.ai.scriptdev_get_ai_constructor_owner")
            }
        })
    }
}
fn validate_control(
    objects: &SourceObjects,
    object: ObjectHandle,
    control: &CreatureAiControl,
) -> Result<()> {
    let kind = objects.get(object)?.kind;
    require(
        matches!(kind, SourceObjectKind::Creature | SourceObjectKind::Pet),
        "execution.ai.creature_kind",
    )?;
    require(
        (kind == SourceObjectKind::Pet) == control.pet.is_some(),
        "execution.ai.pet_context",
    )?;
    Ok(())
}
#[derive(Clone, Default)]
pub struct AiRegistryBuilder {
    factories: BTreeMap<String, Arc<BuiltinAiBinding>>,
    scripts: BTreeMap<u32, Arc<ScriptDevAiBinding>>,
    scripts_loaded: bool,
    next_registration: u64,
}
impl AiRegistryBuilder {
    /// ObjectRegistry.h:35,45-57. RegisterSelf rejects duplicates by default.
    pub fn register(
        &mut self,
        key: String,
        kind: BuiltinCreatureAi,
        replace: bool,
    ) -> Result<bool> {
        if self.factories.contains_key(&key) && !replace {
            return Ok(false);
        }
        self.next_registration = self
            .next_registration
            .checked_add(1)
            .ok_or_else(|| refuse("execution.ai.registration_exhausted"))?;
        self.factories.insert(
            key.clone(),
            Arc::new(BuiltinAiBinding {
                key,
                kind,
                registration: self.next_registration,
            }),
        );
        Ok(true)
    }
    /// ScriptDevAIMgr.cpp:426-432 overwrites by ID, unlike ObjectRegistry.
    pub fn load_scripts(
        &mut self,
        bindings: impl IntoIterator<Item = ScriptDevAiBinding>,
    ) -> Result<()> {
        let mut scripts = BTreeMap::new();
        let mut names = BTreeMap::new();
        for binding in bindings {
            if binding.script_id == 0 {
                continue;
            }
            require(!binding.name.is_empty(), "execution.ai.script_name")?;
            if let Some(id) = names.insert(binding.name.clone(), binding.script_id) {
                require(id == binding.script_id, "execution.ai.script_name_id")?;
            }
            if let Some(previous) = scripts.insert(binding.script_id, Arc::new(binding)) {
                require(
                    previous.name == scripts[&previous.script_id].name,
                    "execution.ai.script_name_id",
                )?;
            }
        }
        self.scripts = scripts;
        self.scripts_loaded = true;
        Ok(())
    }
    pub fn freeze(self) -> SourceAiRegistry {
        SourceAiRegistry {
            factories: self.factories,
            scripts: self
                .scripts
                .values()
                .map(|binding| (binding.name.clone(), Arc::clone(binding)))
                .collect(),
            scripts_loaded: self.scripts_loaded,
        }
    }
    pub fn builtin() -> Self {
        let mut registry = Self::default();
        // CreatureAIRegistry.cpp:37-43. Traversal is map-key order, not this order.
        for kind in [
            BuiltinCreatureAi::Null,
            BuiltinCreatureAi::Guard,
            BuiltinCreatureAi::Pet,
            BuiltinCreatureAi::Totem,
            BuiltinCreatureAi::Event,
            BuiltinCreatureAi::Guardian,
            BuiltinCreatureAi::Possessed,
        ] {
            registry
                .register(kind.key().into(), kind, false)
                .expect("seven distinct builtin keys");
        }
        registry
    }
}
/// Frozen registry definitions. A retained ScriptDev registry is still unsealed evidence.
pub struct SourceAiRegistry {
    factories: BTreeMap<String, Arc<BuiltinAiBinding>>,
    scripts: BTreeMap<String, Arc<ScriptDevAiBinding>>,
    scripts_loaded: bool,
}
impl SourceAiRegistry {
    pub fn builtin() -> Self {
        AiRegistryBuilder::builtin().freeze()
    }
    pub fn select(
        &self,
        objects: &SourceObjects,
        object: ObjectHandle,
        input: CreatureAiInputs,
        control_guids: SourceControlGuids,
    ) -> Result<SelectedCreatureAi> {
        self.select_inner(objects, object, input, false, || Ok(control_guids))
    }
    pub(super) fn select_with_live_control(
        &self,
        objects: &SourceObjects,
        object: ObjectHandle,
        input: CreatureAiInputs,
        control_guids: impl Fn() -> Result<SourceControlGuids>,
    ) -> Result<SelectedCreatureAi> {
        self.select_inner(objects, object, input, false, control_guids)
    }
    /// CreatureAISelector.cpp:40-83 and ScriptDevAIMgr.cpp:363-370.
    #[allow(clippy::too_many_arguments)]
    pub fn select_loaded_db_creature(
        &self,
        objects: &SourceObjects,
        unit: ObjectHandle,
        live: &SourceUnitFields,
        template: &LoadedCreatureInfo,
        scripts: &SourceScriptRegistry,
        factions: &CreatureAiFactions,
        mut observe: impl FnMut(SourceCreatureAiScriptObservation),
    ) -> Result<SelectedCreatureAi> {
        let script_id = template.words[96];
        match probe_loaded_creature_ai_script(objects, unit, scripts, script_id, &mut observe)? {
            LoadedCreatureAiScriptFactory::Null => {
                observe(SourceCreatureAiScriptObservation::BuiltinRegistry);
                let inputs =
                    CreatureAiInputs::loaded_db_creature(template, live, scripts, factions)?;
                self.select_inner(objects, unit, inputs, true, || Ok(live.control_guids))
            }
            LoadedCreatureAiScriptFactory::Ported(script) => {
                let inputs =
                    CreatureAiInputs::loaded_db_creature(template, live, scripts, factions)?;
                validate_control(objects, unit, &inputs.control)?;
                let binding = self.bind_loaded_script(script_id, &inputs.script_name, script)?;
                Ok(SelectedCreatureAi::new(
                    objects.get(unit)?,
                    inputs,
                    AiSelectionBranch::ScriptDev,
                    AiExecutable::ScriptDevGetAi(Arc::clone(binding)),
                ))
            }
        }
    }

    fn bind_loaded_script(
        &self,
        id: u32,
        name: &str,
        live: &SourceScriptDefinition,
    ) -> Result<&Arc<ScriptDevAiBinding>> {
        self.scripts
            .get(name)
            .filter(|binding| {
                binding.script_id == id
                    && binding.name.as_bytes() == live.name
                    && binding.get_ai_nonnull
                    && binding.registration == live.registration
                    && binding.operation == live.operation
                    && binding.source_file == live.source_file
                    && binding.source_line == live.source_line
                    && binding.source_function == live.source_function
            })
            .ok_or_else(|| refuse("execution.ai.ScriptDevAIMgr_370.GetAI_binding_owner"))
    }
    fn select_inner(
        &self,
        objects: &SourceObjects,
        object: ObjectHandle,
        input: CreatureAiInputs,
        script_checked: bool,
        control_guids: impl Fn() -> Result<SourceControlGuids>,
    ) -> Result<SelectedCreatureAi> {
        validate_control(objects, object, &input.control)?;
        let source = objects.get(object)?;
        let c = &input.control;
        // ScriptDevAIMgr.cpp:363-370. A nonnull slot must run before its return is known.
        // CreatureAISelector.cpp:41,59 reads HasCharmer without resolving the GUID.
        if !script_checked
            && (c.pet != Some(CreaturePetKind::Controlled) || control_guids()?.has_charmer())
            && !input.script_name.is_empty()
        {
            require(self.scripts_loaded, "execution.ai.scriptdev_registry_owner")?;
            if let Some(binding) = self
                .scripts
                .get(&input.script_name)
                .filter(|b| b.get_ai_nonnull)
            {
                return Ok(SelectedCreatureAi::new(
                    source,
                    input,
                    AiSelectionBranch::ScriptDev,
                    AiExecutable::ScriptDevGetAi(Arc::clone(binding)),
                ));
            }
        }
        let (branch, key) = if c.pet.is_some() {
            if c.player_controlled && c.pet == Some(CreaturePetKind::Controlled) {
                (AiSelectionBranch::ControlledPet, Some("PetAI"))
            } else {
                (AiSelectionBranch::Guardian, Some("GuardianAI"))
            }
        } else if control_guids()?.has_charmer() && !c.temporary_summon {
            (AiSelectionBranch::Charm, Some("PetAI"))
        } else if c.totem {
            (AiSelectionBranch::Totem, Some("TotemAI"))
        } else if !input.ai_name.is_empty() {
            (
                AiSelectionBranch::ExplicitName,
                Some(input.ai_name.as_str()),
            )
        } else if input.guard {
            (AiSelectionBranch::Guard, Some("GuardAI"))
        } else {
            (AiSelectionBranch::Permit, None)
        };
        let factory = if let Some(key) = key {
            self.factories.get(key)
        } else {
            let mut best = -1;
            let mut selected = None;
            for factory in self.factories.values() {
                let permit = factory.kind.permit(&input);
                if permit > best {
                    best = permit;
                    selected = Some(factory);
                }
            }
            selected
        };
        let executable = factory.map_or(AiExecutable::NullFallback, |factory| {
            AiExecutable::Builtin(Arc::clone(factory))
        });
        Ok(SelectedCreatureAi::new(source, input, branch, executable))
    }
}

#[cfg(test)]
mod registry_startup_tests {
    use super::*;

    #[test]
    fn source_registration_and_permit_traversal_orders_differ() {
        let registry = SourceAiRegistry::builtin();
        let mut registration: Vec<_> = registry.factories.values().collect();
        registration.sort_by_key(|factory| factory.registration);
        assert_eq!(
            registration
                .iter()
                .map(|factory| factory.key.as_str())
                .collect::<Vec<_>>(),
            [
                "NullAI",
                "GuardAI",
                "PetAI",
                "TotemAI",
                "EventAI",
                "GuardianAI",
                "PossessedAI"
            ]
        );
        // ObjectRegistry.h:35 uses std::map, including strict-greater tie handling.
        assert_eq!(
            registry
                .factories
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            [
                "EventAI",
                "GuardAI",
                "GuardianAI",
                "NullAI",
                "PetAI",
                "PossessedAI",
                "TotemAI"
            ]
        );
    }

    #[test]
    fn event_permit_obeys_name_and_both_reactive_predicates() {
        for (name, no_aggro, neutral, expected) in [
            ("EventAI", false, false, 800),
            ("EventAI", true, true, 800),
            ("", false, false, 200),
            ("", true, false, 100),
            ("", false, true, 100),
            ("", true, true, 100),
        ] {
            let input = CreatureAiInputs {
                effective_entry: 1,
                ai_name: name.into(),
                script_name: String::new(),
                control: CreatureAiControl::default(),
                guard: false,
                no_aggro_on_sight: no_aggro,
                neutral_to_all: neutral,
            };
            assert_eq!(BuiltinCreatureAi::Event.permit(&input), expected);
        }
    }
}
