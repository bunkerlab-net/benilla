use super::*;
use std::{collections::BTreeMap, sync::Arc};

fn template(name: &str) -> CreatureAiTemplateOperands {
    CreatureAiTemplateOperands {
        ordinal: 0,
        entry: 901,
        faction: 14,
        extra_flags: 0,
        unit_flags: 0,
        type_flags: 0,
        static_flags: [0; 4],
        rank: 0,
        inhabit_type: 1,
        stats_multipliers: [1f32.to_bits(); 3],
        civilian: false,
        pursuit_ms: 15000,
        ai_name: name.into(),
        script_name: String::new(),
    }
}
fn config() -> EventAiLoaderConfig {
    let bytes = b"LogFilter_EventAiDev=1\nGuarderSight=50\n";
    EventAiLoaderConfig::parse(bytes, Sha256Id::of(bytes)).unwrap()
}
fn event(kind: u32) -> EventAiDefinition {
    EventAiDefinition {
        ordinal: 0,
        id: 1,
        entry: 901,
        guid: 0,
        kind,
        inverse_phase: 0,
        chance: 100,
        flags: 1,
        params: [0; 6],
        actions: std::array::from_fn(|_| EventAiAction {
            kind: 0,
            params: Some([0; 3]),
        }),
        friendly_hp_target_self: None,
    }
}
#[derive(Default)]
struct Unbound;
impl UnitAiExecutionOwner for Unbound {}
impl BuiltinAiExecutionOwner for Unbound {}
impl HostileTargetExecutionOwner for Unbound {}
impl CreatureRandom for Unbound {
    fn creature_irand(&mut self, _: i32, _: i32) -> Result<i32> {
        panic!("unexpected draw")
    }
    fn creature_urand(&mut self, _: u32, _: u32) -> Result<u32> {
        panic!("unexpected draw")
    }
}
impl SummonedAiExecutionOwner for Unbound {}
struct Host {
    calls: Vec<String>,
    kind: u32,
    spell: bool,
    ready: bool,
    range: f32,
    target: Option<ObjectHandle>,
    queries: Vec<bool>,
    helper: Option<ObjectHandle>,
}
impl Default for Host {
    fn default() -> Self {
        Self {
            calls: Vec::new(),
            kind: 1,
            spell: true,
            ready: true,
            range: 0.0,
            target: None,
            queries: Vec::new(),
            helper: None,
        }
    }
}
impl UnitAiExecutionOwner for Host {
    fn call(
        &mut self,
        _: ObjectHandle,
        call: UnitAiCall,
        _: &mut SourceUnits,
        _: &SourceObjects,
    ) -> Result<()> {
        self.calls.push(format!("{call:?}"));
        match call {
            UnitAiCall::Home | UnitAiCall::Chase { .. } => Err(call.missing_owner()),
            _ => Ok(()),
        }
    }
    fn has_unattackable_aura(&mut self, _: ObjectHandle) -> Result<bool> {
        Ok(false)
    }
}
impl BuiltinAiExecutionOwner for Host {
    fn helper_attacker(&mut self, _: ObjectHandle) -> Result<Option<ObjectHandle>> {
        self.calls.push("helper".into());
        Ok(self.helper)
    }
}
impl HostileTargetExecutionOwner for Host {
    fn set_in_front(&mut self, _: ObjectHandle, _: ObjectHandle) -> Result<()> {
        self.calls.push("face".into());
        Ok(())
    }
}
impl CreatureRandom for Host {
    fn creature_irand(&mut self, _: i32, _: i32) -> Result<i32> {
        panic!("unexpected signed draw")
    }
    fn creature_urand(&mut self, min: u32, _: u32) -> Result<u32> {
        self.calls.push("draw".into());
        Ok(min)
    }
    fn creature_urand_default(&mut self) -> Result<u32> {
        self.calls.push("word".into());
        Ok(0)
    }
}
impl SummonedAiExecutionOwner for Host {
    fn totem_type(&mut self, _: ObjectHandle) -> Result<u32> {
        self.calls.push("type".into());
        Ok(self.kind)
    }
    fn totem_spell(&mut self, _: ObjectHandle) -> Result<u32> {
        self.calls.push("spell-id".into());
        Ok(7)
    }
    fn spell(&mut self, id: u32) -> Result<Option<TotemSpell>> {
        self.calls.push("spell".into());
        Ok(self.spell.then_some(TotemSpell { id, range_index: 1 }))
    }
    fn spell_ready(&mut self, _: ObjectHandle, _: TotemSpell) -> Result<bool> {
        self.calls.push("ready".into());
        Ok(self.ready)
    }
    fn spell_max_range(&mut self, _: u32) -> Result<f32> {
        self.calls.push("range".into());
        Ok(self.range)
    }
    fn nearest_totem_target(
        &mut self,
        _: ObjectHandle,
        _: Option<ObjectHandle>,
        _: f32,
    ) -> Result<Option<ObjectHandle>> {
        self.calls.push("search".into());
        Ok(self.target)
    }
    fn totem_query(&mut self, _: ObjectHandle, _: ObjectHandle, q: TotemQuery) -> Result<bool> {
        self.calls.push(format!("{q:?}"));
        Ok(self.queries.remove(0))
    }
    fn cast_totem_spell(&mut self, _: ObjectHandle, _: ObjectHandle, spell: u32) -> Result<()> {
        assert_eq!(spell, 7);
        self.calls.push("cast".into());
        Ok(())
    }
    fn follow_data(&mut self, _: ObjectHandle, _: ObjectHandle) -> Result<(f32, f32)> {
        self.calls.push("follow-data".into());
        Ok((0.75, 2.5))
    }
    fn move_follow(&mut self, _: ObjectHandle, _: ObjectHandle, d: f32, a: f32) -> Result<()> {
        assert_eq!((d, a), (2.5, 0.75));
        self.calls.push("follow".into());
        Ok(())
    }
    fn relinquish_follow(&mut self, _: ObjectHandle, _: ObjectHandle) -> Result<()> {
        self.calls.push("relinquish".into());
        Ok(())
    }
    fn guardian_sight(
        &mut self,
        _: ObjectHandle,
        _: ObjectHandle,
        q: GuardianSightQuery,
    ) -> Result<bool> {
        self.calls.push(format!("{q:?}"));
        Ok(self.queries.remove(0))
    }
}
struct Fixture {
    objects: SourceObjects,
    units: SourceUnits,
    ais: SourceCreatureAis,
    threats: SourceThreats,
    state: SourceSummonedAi,
    unit: ObjectHandle,
    owner: ObjectHandle,
    target: ObjectHandle,
}
impl Fixture {
    fn run<O: SummonedAiExecutionOwner, T>(
        &mut self,
        host: &mut O,
        f: impl FnOnce(&mut SourceSummonedAi, &mut SourceAiExecution<'_, O>) -> T,
    ) -> T {
        f(
            &mut self.state,
            &mut SourceAiExecution {
                queries: SourceUnitQueries {
                    objects: &self.objects,
                    units: &mut self.units,
                },
                ais: &mut self.ais,
                threats: &mut self.threats,
                owner: host,
            },
        )
    }
}
fn fixture(name: &str, definition: Option<EventAiDefinition>) -> Fixture {
    let mut objects = SourceObjects::default();
    let unit = objects.construct(1, SourceObjectKind::Creature).unwrap();
    let owner = objects.construct(1, SourceObjectKind::Player).unwrap();
    let target = objects.construct(1, SourceObjectKind::Creature).unwrap();
    let map = MapIdentity {
        map: 0,
        instance: 0,
        creation_generation: 1,
    };
    for (h, high, entry, counter) in [
        (unit, HighGuid::Creature, 901, 1),
        (owner, HighGuid::Player, 0, 1),
        (target, HighGuid::Creature, 902, 2),
    ] {
        objects
            .assign_guid(h, 0, SourceGuid::with_entry(high, entry, counter))
            .unwrap();
        objects.get_mut(h).unwrap().map = Some(map);
        objects.get_mut(h).unwrap().alive = true;
        objects.set_pose(h, SourcePose([0; 4])).unwrap();
    }
    objects.register_world_lookup(target).unwrap();
    objects.mark_in_world(target).unwrap();
    objects.register_player_lookup(owner).unwrap();
    objects.mark_in_world(owner).unwrap();
    let t = template(name);
    let mut units = SourceUnits::default();
    for h in [unit, owner, target] {
        units
            .register(&objects, h, SourceUnitFields::creature(&t))
            .unwrap();
        units.get_mut(&objects, h).unwrap().vitals = Some(SourceUnitVitals {
            health: 10,
            max_health: 10,
            mana: 0,
            max_mana: 0,
            has_mana: false,
            energy: 0,
            max_energy: 0,
        });
    }
    let control = CreatureAiControl {
        owner: Some(owner),
        totem: name == "TotemAI",
        ..Default::default()
    };
    let selected = SourceAiRegistry::builtin()
        .select(&objects, unit, t.clone().inputs(control, false))
        .unwrap();
    let base = SourceCreatureAi::construct(
        &objects,
        unit,
        &mut units,
        &t,
        false,
        0,
        &config(),
        &mut Unbound,
    )
    .unwrap();
    let mut ais = SourceCreatureAis::default();
    ais.install(&objects, &mut units, base, &mut SourceCombat::default())
        .unwrap();
    let mut defs = EventAiDefinitions::default();
    if let Some(event) = definition {
        defs.insert(event);
    }
    let mut state = SourceSummonedAi::construct(
        selected,
        &objects,
        &mut units,
        &mut ais,
        Arc::new(defs),
        EventAiBuild::Release,
        Some(11939),
    )
    .unwrap();
    state
        .event
        .init(&objects, &mut EventAiDispatcher(&mut Init))
        .unwrap();
    let mut threats = SourceThreats::default();
    threats.register(unit).unwrap();
    Fixture {
        objects,
        units,
        ais,
        threats,
        state,
        unit,
        owner,
        target,
    }
}
struct Init;
impl CreatureRandom for Init {
    fn creature_irand(&mut self, _: i32, _: i32) -> Result<i32> {
        panic!("unexpected draw")
    }
    fn creature_urand(&mut self, _: u32, _: u32) -> Result<u32> {
        panic!("unexpected draw")
    }
}
impl EventAiExecutionOwner for Init {}
fn settings() -> EventAiRespawnSettings {
    EventAiRespawnSettings {
        no_aggro_on_sight: false,
        spawn_defensive: false,
        ignore_combat: false,
    }
}

#[test]
fn summoned_construction_retains_eventai_loader_and_derived_identity_only() {
    for name in ["TotemAI", "GuardianAI"] {
        let f = fixture(name, Some(event(4)));
        assert_eq!(f.state.event.holders.len(), 1);
        assert_eq!(f.state.event.selected.inputs().ai_name, name);
        assert!(SourceEventAi::construct(
            f.state.event.selected.clone(),
            &f.objects,
            Arc::default(),
            EventAiBuild::Release
        )
        .is_err());
        if name == "GuardianAI" {
            let base = f.ais.get(&f.objects, f.unit).unwrap();
            assert_eq!(
                (base.follow_angle, base.follow_distance),
                (std::f32::consts::FRAC_PI_2, 3.0)
            );
            assert!(f.state.follow_default);
        }
        assert_eq!(f.state.inventory(&f.objects, &f.ais).unwrap()["version"], 1);
    }
    let mut f = fixture("TotemAI", None);
    for name in ["EventAI", "NullAI", "GuardAI", "PossessedAI", "PetAI"] {
        let selected = SourceAiRegistry::builtin()
            .select(
                &f.objects,
                f.unit,
                template(name).inputs(CreatureAiControl::default(), false),
            )
            .unwrap();
        assert_eq!(
            SourceEventAi::construct_inherited(
                selected,
                &f.objects,
                Arc::default(),
                EventAiBuild::Release
            )
            .unwrap_err()
            .id,
            "execution.event_ai.derived_executable_identity"
        );
    }
    let selected = SourceAiRegistry::builtin()
        .select(
            &f.objects,
            f.unit,
            template("GuardianAI").inputs(CreatureAiControl::default(), false),
        )
        .unwrap();
    assert_eq!(
        SourceSummonedAi::construct(
            selected,
            &f.objects,
            &mut f.units,
            &mut f.ais,
            Arc::default(),
            EventAiBuild::Release,
            Some(0)
        )
        .unwrap_err()
        .id,
        "execution.guardian_ai.f5_15_required_owner"
    );
    let mut input = template("GuardianAI").inputs(
        CreatureAiControl {
            owner: Some(f.owner),
            ..Default::default()
        },
        false,
    );
    let selected = SourceAiRegistry::builtin()
        .select(&f.objects, f.unit, input.clone())
        .unwrap();
    assert_eq!(
        SourceSummonedAi::construct(
            selected,
            &f.objects,
            &mut f.units,
            &mut f.ais,
            Arc::default(),
            EventAiBuild::Release,
            None
        )
        .unwrap_err()
        .id,
        "execution.guardian_ai.f5_15_created_by_spell_owner"
    );
    // GuardianAI.cpp:29-33 mutates the inherited defaults before reading the missing field.
    let base = f.ais.get(&f.objects, f.unit).unwrap();
    assert_eq!(
        (base.follow_angle, base.follow_distance),
        (std::f32::consts::FRAC_PI_2, 1.5)
    );
    input.ai_name.clear();
    for (control, kind, branch) in [
        (
            CreatureAiControl {
                totem: true,
                ..Default::default()
            },
            BuiltinCreatureAi::Totem,
            AiSelectionBranch::Totem,
        ),
        (
            CreatureAiControl {
                pet: Some(CreaturePetKind::Guardian),
                owner: Some(f.owner),
                ..Default::default()
            },
            BuiltinCreatureAi::Guardian,
            AiSelectionBranch::Guardian,
        ),
        (
            CreatureAiControl::default(),
            BuiltinCreatureAi::Event,
            AiSelectionBranch::Permit,
        ),
    ] {
        let object = if control.pet.is_some() {
            f.objects.construct(1, SourceObjectKind::Pet).unwrap()
        } else {
            f.unit
        };
        input.control = control;
        let selected = SourceAiRegistry::builtin()
            .select(&f.objects, object, input.clone())
            .unwrap();
        assert_eq!(
            (selected.executable().builtin_kind(), selected.branch()),
            (Some(kind), branch)
        );
    }
}
#[test]
fn totem_updates_event_timers_before_subtype_and_skips_inherited_unit_timers() {
    let mut f = fixture("TotemAI", Some(event(0)));
    f.state.event.holders[0].timer = 1000;
    let timer = &mut f
        .ais
        .get_mut(&f.objects, f.unit)
        .unwrap()
        .base
        .generic_timers[0];
    timer.disabled = false;
    timer.remaining = 5;
    assert_eq!(
        f.run(&mut Unbound, |s, e| s.update(501, e)).unwrap_err().id,
        "execution.totem_ai.f5_15_totem_type_owner"
    );
    assert_eq!(f.state.event.holders[0].timer, 499);
    assert_eq!(
        f.ais.get(&f.objects, f.unit).unwrap().base.generic_timers[0].remaining,
        5
    );
    f.units.get_mut(&f.objects, f.unit).unwrap().totem = false;
    assert_eq!(
        f.run(&mut Unbound, |s, e| s.update(1, e)).unwrap_err().id,
        "execution.totem_ai.non_totem_downcast_undefined"
    );
    f.units.get_mut(&f.objects, f.unit).unwrap().totem = true;
    let mut host = Host {
        kind: 0,
        ..Default::default()
    };
    f.run(&mut host, |s, e| s.update(1, e)).unwrap();
    assert_eq!(host.calls, ["type"]);
    host.kind = 1;
    host.calls.clear();
    f.objects.get_mut(f.unit).unwrap().alive = false;
    f.run(&mut host, |s, e| s.update(1, e)).unwrap();
    assert_eq!(host.calls, ["type"]);
}
#[test]
fn totem_spell_cooldown_search_facing_cast_and_victim_revalidation_order() {
    let mut f = fixture("TotemAI", None);
    let mut host = Host::default();
    f.run(&mut host, |s, e| s.update(1, e)).unwrap();
    assert_eq!(
        host.calls,
        ["type", "spell-id", "spell", "ready", "range", "face", "cast"]
    );
    host.calls.clear();
    host.range = 30.0;
    host.target = Some(f.target);
    // Previous self target is not registered in Map::GetUnit; search is required.
    f.run(&mut host, |s, e| s.update(1, e)).unwrap();
    assert_eq!(
        host.calls,
        ["type", "spell-id", "spell", "ready", "range", "search", "face", "cast"]
    );
    host.calls.clear();
    host.queries = vec![true, true, false, true];
    f.run(&mut host, |s, e| s.update(1, e)).unwrap();
    assert_eq!(
        &host.calls[5..],
        [
            "CanAttackOnSight",
            "WithinDistance(1106247680)",
            "CanAssist",
            "VisibleForOrDetect",
            "face",
            "cast"
        ]
    );
    host.calls.clear();
    host.queries = vec![false];
    host.target = None;
    f.run(&mut host, |s, e| s.update(1, e)).unwrap();
    assert_eq!(host.calls.last().unwrap(), "search");
    host.calls.clear();
    host.spell = false;
    f.run(&mut host, |s, e| s.update(1, e)).unwrap();
    assert_eq!(host.calls, ["type", "spell-id", "spell"]);
    host.calls.clear();
    host.spell = true;
    host.ready = false;
    f.run(&mut host, |s, e| s.update(1, e)).unwrap();
    assert_eq!(host.calls, ["type", "spell-id", "spell", "ready"]);
    host.calls.clear();
    f.units
        .get_mut(&f.objects, f.unit)
        .unwrap()
        .casting_non_melee = true;
    f.run(&mut host, |s, e| s.update(1, e)).unwrap();
    assert_eq!(host.calls, ["type"]);
    assert!(!f.state.is_visible().unwrap());
    let target = f.target;
    f.run(&mut Unbound, |s, e| s.attack_start(Some(target), e))
        .unwrap();
    f.run(&mut Unbound, |s, e| s.attacked_by(Some(target), e))
        .unwrap();
    f.run(&mut Unbound, |s, e| {
        s.move_in_line_of_sight(target, &config(), e)
    })
    .unwrap();
}
#[test]
fn guardian_assistance_precedes_inherited_timers_and_event_update() {
    let mut f = fixture("GuardianAI", Some(event(0)));
    f.state.event.holders[0].timer = 1000;
    f.units.get_mut(&f.objects, f.owner).unwrap().flags |= UNIT_FLAG_COMBAT;
    let timer = &mut f
        .ais
        .get_mut(&f.objects, f.unit)
        .unwrap()
        .base
        .generic_timers[0];
    timer.disabled = false;
    timer.remaining = 5;
    assert_eq!(
        f.run(&mut Unbound, |s, e| s.update(501, e)).unwrap_err().id,
        "execution.builtin_ai.f5_06_helper_attacker_owner"
    );
    assert_eq!(
        f.ais.get(&f.objects, f.unit).unwrap().base.generic_timers[0].remaining,
        5
    );
    assert_eq!(f.state.event.holders[0].timer, 1000);
    let mut host = Host::default();
    f.run(&mut host, |s, e| s.update(501, e)).unwrap();
    assert_eq!(host.calls, ["helper"]);
    assert_eq!(f.state.event.holders[0].timer, 499);
    assert_eq!(
        f.ais.get(&f.objects, f.unit).unwrap().base.generic_timers[0].remaining,
        0
    );
    f.units.get_mut(&f.objects, f.unit).unwrap().owner = None;
    f.run(&mut Unbound, |s, e| s.update(1, e)).unwrap();
    assert_eq!(
        f.state.is_visible().unwrap_err().id,
        "execution.builtin_ai.f6_visibility_owner"
    );
}
#[test]
fn guardian_respawn_follow_owner_loss_death_and_unsummon() {
    let mut f = fixture("GuardianAI", Some(event(4)));
    f.state.event.phase = 9;
    f.state.event.clock.remaining = 123;
    f.units.get_mut(&f.objects, f.unit).unwrap().owner = None;
    f.run(&mut Unbound, |s, e| s.just_respawned(settings(), e))
        .unwrap();
    assert_eq!(f.state.event.clock.remaining, 123);
    f.units.get_mut(&f.objects, f.unit).unwrap().owner = Some(f.owner);
    assert_eq!(
        f.run(&mut Unbound, |s, e| s.just_respawned(settings(), e))
            .unwrap_err()
            .id,
        "execution.guardian_ai.f5_15_player_follow_data_owner"
    );
    let mut host = Host::default();
    f.run(&mut host, |s, e| s.just_respawned(settings(), e))
        .unwrap();
    assert_eq!(host.calls, ["follow-data", "follow"]);
    assert_eq!(f.state.event.clock.remaining, 500);
    assert_eq!(f.state.event.phase, 9);
    assert_eq!(
        f.run(&mut Unbound, |s, e| s.on_unsummon(e)).unwrap_err().id,
        "execution.guardian_ai.f5_15_relinquish_follow_owner"
    );
    host.calls.clear();
    f.run(&mut host, |s, e| s.just_died(None, e)).unwrap();
    assert_eq!(f.state.event.phase, 0);
    assert_eq!(host.calls, ["relinquish"]);
    f.state.event.throw_mask = 1;
    assert_eq!(
        f.run(&mut host, |s, e| s.just_died(None, e))
            .unwrap_err()
            .id,
        "execution.summoned_ai.f5_06_death_event_broadcast_owner"
    );
    f.state.event.throw_mask = 0;
    f.state.event.despawn_mask = 4;
    f.run(&mut host, |s, e| s.just_died(None, e)).unwrap();
    f.state
        .event
        .despawn_guids
        .push(f.objects.get(f.target).unwrap().guid.unwrap());
    assert_eq!(
        f.run(&mut host, |s, e| s.just_died(None, e))
            .unwrap_err()
            .id,
        "execution.summoned_ai.f5_15_death_aggregation_owner"
    );
}
#[test]
fn summoned_evade_overrides_do_not_run_eventai_aggregation_tail() {
    for name in ["TotemAI", "GuardianAI"] {
        let mut f = fixture(name, Some(event(7)));
        f.state.event.despawn_mask = 2;
        let mut host = Host::default();
        if name == "GuardianAI" {
            assert_eq!(
                f.run(&mut Unbound, |s, e| s.enter_evade(e)).unwrap_err().id,
                "execution.unit_ai.f5_12_evade_aura_owner"
            );
            f.ais.get_mut(&f.objects, f.unit).unwrap().base.react_state = 0;
        } else {
            assert_eq!(
                f.run(&mut Unbound, |s, e| s.enter_evade(e)).unwrap_err().id,
                "execution.unit_ai.combat_stop_owner"
            );
        }
        f.run(&mut host, |s, e| s.enter_evade(e)).unwrap();
        if name == "TotemAI" {
            assert_eq!(host.calls, ["CombatStopWithPets", "word", "word"]);
        } else {
            assert_eq!(
                host.calls,
                [
                    "RemoveAurasOnEvade",
                    "CombatStopWithPets",
                    "TriggerEvade",
                    "word",
                    "word"
                ]
            );
        }
    }
}
#[test]
fn guardian_los_runs_event_filter_before_inherited_sight() {
    let mut definition = event(10);
    definition.params[0] = 1;
    definition.params[1] = 30;
    definition.params[5] = 1;
    let mut f = fixture("GuardianAI", Some(definition));
    let who = f.owner;
    assert_eq!(
        f.run(&mut Unbound, |s, e| s.move_in_line_of_sight(
            who,
            &config(),
            e
        ))
        .unwrap_err()
        .id,
        "execution.guardian_ai.f5_04_ooc_los_reaction_owner"
    );
    let mut host = Host {
        queries: vec![false, true, true],
        ..Default::default()
    };
    f.ais.get_mut(&f.objects, f.unit).unwrap().base.react_state = 0;
    f.run(&mut host, |s, e| s.move_in_line_of_sight(who, &config(), e))
        .unwrap();
    assert_eq!(
        host.calls,
        [
            "Enemy",
            "WithinDistance(1106247680)",
            "LineOfSight",
            "word",
            "word"
        ]
    );
}
#[test]
fn summoned_unbound_dependencies_name_the_actual_owner() {
    let f = fixture("TotemAI", None);
    let u = f.unit;
    let t = f.target;
    let mut o = Unbound;
    let spell = TotemSpell {
        id: 7,
        range_index: 1,
    };
    let errors = [
        o.totem_spell(u).unwrap_err(),
        o.spell(7).unwrap_err(),
        o.spell_ready(u, spell).unwrap_err(),
        o.spell_max_range(1).unwrap_err(),
        o.totem_query(u, t, TotemQuery::CanAttackOnSight)
            .unwrap_err(),
        o.totem_query(u, t, TotemQuery::WithinDistance(0))
            .unwrap_err(),
        o.nearest_totem_target(u, None, 30.0).unwrap_err(),
        o.set_in_front(u, t).unwrap_err(),
        o.cast_totem_spell(u, t, 7).unwrap_err(),
        o.move_follow(u, t, 1.5, 0.0).unwrap_err(),
        o.mini_pet(u).unwrap_err(),
        o.guardian_sight(u, t, GuardianSightQuery::LineOfSight)
            .unwrap_err(),
    ];
    assert_eq!(
        errors.map(|e| e.id),
        [
            "execution.totem_ai.f5_15_totem_spell_owner",
            "execution.totem_ai.f5_07_spell_template_owner",
            "execution.totem_ai.f5_08_spell_cooldown_owner",
            "execution.totem_ai.f5_07_spell_range_owner",
            "execution.totem_ai.f5_04_reaction_owner",
            "execution.totem_ai.f6_target_geometry_owner",
            "execution.totem_ai.f6_nearest_attackable_visitor_owner",
            "execution.select_hostile_target.f6_set_in_front_owner",
            "execution.totem_ai.f5_08_nontriggered_cast_owner",
            "execution.guardian_ai.f5_05_follow_motion_owner",
            "execution.guardian_ai.f5_15_pet_subtype_owner",
            "execution.guardian_ai.f6_ooc_los_geometry_owner",
        ]
    );
}

#[test]
fn summoned_catalog04_constructor_and_first_update_census_or_explicit_absence() {
    let Some(root) = std::env::var_os("BENILLA_F5_AI_CATALOG") else {
        println!("SKIP summoned catalog-04 census: set BENILLA_F5_AI_CATALOG, BENILLA_F5_SCRIPT_REPORT and BENILLA_F5_EVENT_AI_CONFIG");
        return;
    };
    let report = std::env::var_os("BENILLA_F5_SCRIPT_REPORT").expect("ScriptDev report required");
    let config_path =
        std::env::var_os("BENILLA_F5_EVENT_AI_CONFIG").expect("recorded config required");
    let config = EventAiLoaderConfig::recorded(
        Path::new(&config_path),
        &Sha256Id::parse("321f2dc486e38f77037366bf84fb0ce8d09ed823ffa4524082ab01a8b0b79244")
            .unwrap(),
    )
    .unwrap();
    let mut catalog =
        super::super::super::production_compiler::VerifiedCatalog::open(Path::new(&root), false)
            .unwrap();
    let mut builder = AiRegistryBuilder::builtin();
    builder
        .load_catalog_scripts(&catalog, Path::new(&report))
        .unwrap();
    let registry = builder.freeze();
    let factions = CreatureAiFactions::load_catalog(&mut catalog).unwrap();
    let templates = catalog_creature_ai_census_inputs(&mut catalog).unwrap();
    let mut spawn_counts = BTreeMap::<u32, usize>::new();
    for (_, w) in catalog
        .execution_event_ai_operands("creature", ["guid", "id"])
        .unwrap()
    {
        *spawn_counts.entry(w[1]).or_default() += 1;
    }
    let rows = catalog_event_ai_rows(&mut catalog).unwrap();
    let mut selected_counts =
        BTreeMap::from([("TotemAI", (0usize, 0usize)), ("GuardianAI", (0, 0))]);
    let mut outcomes = BTreeMap::<String, (usize, usize)>::new();
    let mut event_rows = 0;
    let mut constructed = 0;
    let mut initialized = 0;
    let mut undefined_rows = Vec::new();
    for t in templates {
        let mut objects = SourceObjects::default();
        let unit = objects.construct(1, SourceObjectKind::Creature).unwrap();
        let selected = registry
            .select(
                &objects,
                unit,
                t.clone().inputs(
                    CreatureAiControl::default(),
                    factions.neutral_to_all(t.faction),
                ),
            )
            .unwrap();
        let kind = selected.executable().builtin_kind();
        if !matches!(
            kind,
            Some(BuiltinCreatureAi::Totem | BuiltinCreatureAi::Guardian)
        ) {
            continue;
        }
        let name = kind.unwrap().key();
        let spawns = spawn_counts.get(&t.entry).copied().unwrap_or(0);
        let count = selected_counts.get_mut(name).unwrap();
        count.0 += 1;
        count.1 += spawns;
        let mut units = SourceUnits::default();
        units
            .register(&objects, unit, SourceUnitFields::creature(&t))
            .unwrap();
        let mut ais = SourceCreatureAis::default();
        let mut threats = SourceThreats::default();
        threats.register(unit).unwrap();
        let result = (|| {
            let relevant = rows
                .iter()
                .filter(|row| row.words[1] == t.entry)
                .cloned()
                .collect();
            let (defs, _) = EventAiDefinitions::load(
                &mut EventAiCatalog::new(&mut catalog, config.clone()),
                relevant,
            )?;
            let base = SourceCreatureAi::construct(
                &objects,
                unit,
                &mut units,
                &t,
                false,
                0,
                &config,
                &mut Unbound,
            )?;
            ais.install(&objects, &mut units, base, &mut SourceCombat::default())?;
            let mut state = SourceSummonedAi::construct(
                selected,
                &objects,
                &mut units,
                &mut ais,
                Arc::new(defs),
                EventAiBuild::Release,
                None,
            )?;
            constructed += 1;
            state
                .event
                .init(&objects, &mut EventAiDispatcher(&mut Init))?;
            event_rows += state.event.holders.len();
            initialized += 1;
            state.update(
                1,
                &mut SourceAiExecution {
                    queries: SourceUnitQueries {
                        objects: &objects,
                        units: &mut units,
                    },
                    ais: &mut ais,
                    threats: &mut threats,
                    owner: &mut Unbound,
                },
            )
        })();
        let reason = result.map_or_else(|e| e.id, |_| "updated".into());
        if reason == "execution.totem_ai.non_totem_downcast_undefined" && spawns != 0 {
            undefined_rows.push((t.entry, spawns));
        }
        let count = outcomes.entry(format!("{name}:{reason}")).or_default();
        count.0 += 1;
        count.1 += spawns;
    }
    assert_eq!(selected_counts["TotemAI"].0, 93);
    println!("summoned catalog-04 ordinary uncontrolled templates/raw-spawn rows={selected_counts:?}; constructor/init/first-update outcomes={outcomes:?}; constructed={constructed}; initialized={initialized}; copied EventAI rows={event_rows}; non-Totem static downcast stock rows={undefined_rows:?}; summon-time class/control and live owner state are not inferable from raw templates");
}

#[test]
fn destroyed_owner_skips_guardian_assistance_and_respawn_but_not_eventai_validation() {
    let mut f = fixture("GuardianAI", Some(event(0)));
    let selected = SourceAiRegistry::builtin()
        .select(
            &f.objects,
            f.unit,
            template("EventAI").inputs(
                CreatureAiControl {
                    owner: Some(f.owner),
                    ..Default::default()
                },
                false,
            ),
        )
        .unwrap();
    let ordinary =
        SourceEventAi::construct(selected, &f.objects, Arc::default(), EventAiBuild::Release)
            .unwrap();
    f.objects.remove_from_world(f.owner).unwrap();
    f.objects.remove_player_lookup(f.owner).unwrap();
    let map = f.objects.get(f.unit).unwrap().map.unwrap();
    f.objects
        .destroy(f.owner, &mut SourceCells::new(map).unwrap())
        .unwrap();
    // SourceUnitFields::owner is the live GetOwner result, not the retained OwnerGuid.
    f.units.get_mut(&f.objects, f.unit).unwrap().owner = None;
    assert_eq!(
        ordinary.validate(&f.objects).unwrap_err().id,
        "execution.stale_object_handle"
    );
    assert_eq!(
        ordinary.validate_inherited(&f.objects).unwrap_err().id,
        "execution.event_ai.derived_executable_identity"
    );
    f.state.event.clock.remaining = 123;
    f.run(&mut Unbound, |s, e| s.just_respawned(settings(), e))
        .unwrap();
    assert_eq!(f.state.event.clock.remaining, 123); // GuardianAI.cpp:45 returns before base reset.
    f.run(&mut Unbound, |s, e| s.update(1, e)).unwrap(); // :62 skips assistance, not inherited update.
    assert_eq!(f.state.event.clock.remaining, 122);
    f.state.inventory(&f.objects, &f.ais).unwrap();
}

#[test]
fn guardian_ordinary_constructor_distance_and_nonfollow_respawn_are_exact() {
    let mut f = fixture("GuardianAI", None);
    let selected = f.state.event.selected.clone();
    let other = SourceSummonedAi::construct(
        selected,
        &f.objects,
        &mut f.units,
        &mut f.ais,
        Arc::default(),
        EventAiBuild::Release,
        Some(0),
    )
    .unwrap();
    assert!(other.follow_default);
    assert_eq!(f.ais.get(&f.objects, f.unit).unwrap().follow_distance, 1.5);
    f.state.follow_default = false;
    f.run(&mut Unbound, |s, e| s.just_respawned(settings(), e))
        .unwrap();
}

#[test]
fn totem_cached_target_failure_short_circuits_before_search_and_clears_guid() {
    for (queries, expected) in [
        (vec![false], vec!["CanAttackOnSight"]),
        (
            vec![true, false],
            vec!["CanAttackOnSight", "WithinDistance(1106247680)"],
        ),
        (
            vec![true, true, true],
            vec![
                "CanAttackOnSight",
                "WithinDistance(1106247680)",
                "CanAssist",
            ],
        ),
        (
            vec![true, true, false, false],
            vec![
                "CanAttackOnSight",
                "WithinDistance(1106247680)",
                "CanAssist",
                "VisibleForOrDetect",
            ],
        ),
    ] {
        let mut f = fixture("TotemAI", None);
        let mut host = Host {
            range: 30.0,
            target: Some(f.target),
            ..Default::default()
        };
        f.run(&mut host, |s, e| s.update(1, e)).unwrap();
        host.calls.clear();
        host.queries = queries;
        host.target = None;
        f.run(&mut host, |s, e| s.update(1, e)).unwrap();
        assert_eq!(&host.calls[5..host.calls.len() - 1], expected);
        assert_eq!(host.calls.last().unwrap(), "search");
        host.calls.clear();
        f.run(&mut host, |s, e| s.update(1, e)).unwrap();
        assert_eq!(&host.calls[5..], ["search"]);
    }
}

#[test]
fn totem_cached_creature_uses_map_storage_before_attackability_checks() {
    // TotemAI.cpp:84-89; Map.cpp:2298-2310,2371-2376 has no creature in-world filter.
    let mut f = fixture("TotemAI", None);
    let mut host = Host {
        range: 30.0,
        target: Some(f.target),
        ..Default::default()
    };
    f.run(&mut host, |s, e| s.update(1, e)).unwrap();
    f.objects.get_mut(f.target).unwrap().in_world = false;
    host.calls.clear();
    host.queries = vec![true, true, false, true];
    f.run(&mut host, |s, e| s.update(1, e)).unwrap();
    assert_eq!(
        &host.calls[5..],
        [
            "CanAttackOnSight",
            "WithinDistance(1106247680)",
            "CanAssist",
            "VisibleForOrDetect",
            "face",
            "cast",
        ]
    );
}
