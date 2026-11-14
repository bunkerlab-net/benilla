use super::*;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[derive(Default)]
struct Loader {
    missing: BTreeSet<(EventAiLookup, u32)>,
    spells: BTreeMap<u32, EventAiSpell>,
    dev: bool,
}
impl EventAiLoaderOwner for Loader {
    fn contains(&mut self, domain: EventAiLookup, id: u32, _entry: u32) -> Result<bool> {
        Ok(!self.missing.contains(&(domain, id)))
    }
    fn spell(&mut self, id: u32) -> Result<Option<EventAiSpell>> {
        Ok(self.spells.get(&id).cloned())
    }
    fn quest_exploration(&mut self, _id: u32) -> Result<Option<bool>> {
        Ok(Some(true))
    }
    fn assistance_radius(&mut self) -> Result<u32> {
        Ok(10)
    }
    fn dev_log_filter(&self) -> bool {
        self.dev
    }
}
fn row(id: u32, key: i32, kind: u32) -> EventAiRow {
    let mut words = [0; 24];
    words[0] = id;
    words[1] = key as u32;
    words[2] = kind;
    words[4] = 100;
    EventAiRow {
        ordinal: u64::from(id),
        words,
    }
}
fn loaded(loader: &mut Loader, row: EventAiRow) -> EventAiDefinition {
    match load_event_ai_row(loader, row).unwrap().disposition {
        EventAiDisposition::Loaded(e) => e,
        _ => panic!("expected loaded row"),
    }
}
fn definition(id: u32, kind: u32) -> EventAiDefinition {
    let mut loader = Loader::default();
    let mut row = row(id, 901, kind);
    if kind == 16 {
        row.words[6] = 7;
        row.words[7] = 1;
        loader.spells.insert(7, spell());
    }
    loaded(&mut loader, row)
}
fn spell() -> EventAiSpell {
    EventAiSpell {
        school: 1,
        targets: 1,
        attributes_ex: 0,
        attributes_ex3: 0,
        range_index: 2,
        effects: [2, 0, 0],
        target_a: [6, 0, 0],
        target_b: [0; 3],
    }
}
fn selected(objects: &mut SourceObjects, guid: u32) -> SelectedCreatureAi {
    let creature = objects
        .construct(u64::from(guid), SourceObjectKind::Creature)
        .unwrap();
    objects.get_mut(creature).unwrap().database_guid = guid;
    let mut registry = AiRegistryBuilder::builtin();
    registry.load_scripts(Vec::new()).unwrap();
    registry
        .freeze()
        .select(
            objects,
            creature,
            CreatureAiInputs {
                effective_entry: 901,
                ai_name: "EventAI".into(),
                script_name: String::new(),
                control: CreatureAiControl::default(),
                guard: false,
                no_aggro_on_sight: false,
                neutral_to_all: false,
            },
        )
        .unwrap()
}
fn state(
    objects: &mut SourceObjects,
    definitions: Arc<EventAiDefinitions>,
    guid: u32,
    build: EventAiBuild,
) -> SourceEventAi {
    let selected = selected(objects, guid);
    SourceEventAi::construct(selected, objects, definitions, build).unwrap()
}
#[derive(Default)]
struct Host {
    calls: Vec<String>,
    draws: Vec<(u32, u32)>,
    checks: Vec<u32>,
    dispatched: Vec<u32>,
    main: bool,
}
impl CreatureRandom for Host {
    fn creature_irand(&mut self, _min: i32, _max: i32) -> Result<i32> {
        panic!("unexpected irand")
    }
    fn creature_urand(&mut self, min: u32, max: u32) -> Result<u32> {
        self.draws.push((min, max));
        Ok(min)
    }
}
impl EventAiLifecycleOwner for Host {
    fn callback(&mut self, _state: &mut SourceEventAi, call: EventAiCallback) -> Result<()> {
        self.calls.push(format!("{call:?}"));
        if matches!(call, EventAiCallback::CalculateMainSpell(_)) && !self.main {
            return Err(call.missing_owner());
        }
        Ok(()) // Authored callback domain, not executable base AI evidence.
    }
    fn check_and_ready(
        &mut self,
        state: &mut SourceEventAi,
        index: usize,
        _invoker: Option<ObjectHandle>,
    ) -> Result<()> {
        self.checks.push(state.holders[index].event.id);
        state.staged[state.depth].push(index);
        Ok(()) // Authored predicate always ready.
    }
    fn process_event(
        &mut self,
        state: &mut SourceEventAi,
        index: usize,
        _invoker: Option<ObjectHandle>,
    ) -> Result<()> {
        self.calls
            .push(format!("Process({})", state.holders[index].event.id));
        self.dispatched.push(state.holders[index].event.id);
        Ok(()) // No stock action claim.
    }
    fn spawned_conditions(&mut self, _state: &SourceEventAi, _index: usize) -> Result<bool> {
        Ok(true)
    }
}
fn settings() -> EventAiRespawnSettings {
    EventAiRespawnSettings {
        no_aggro_on_sight: false,
        spawn_defensive: false,
        ignore_combat: false,
    }
}

#[test]
fn chance_narrows_before_cap_and_zero_is_logged_but_retained() {
    // CreatureEventAIMgr.cpp:266-278, including 256 -> 0 and 300 -> 44.
    for (raw, expected, line) in [
        (0, 0, Some(273)),
        (256, 0, Some(273)),
        (300, 44, None),
        (255, 100, Some(277)),
        (356, 100, None),
    ] {
        let mut row = row(1, 901, 4);
        row.words[4] = raw;
        let result = load_event_ai_row(&mut Loader::default(), row).unwrap();
        let EventAiDisposition::Loaded(event) = result.disposition else {
            panic!("zero chance was skipped")
        };
        assert_eq!(event.chance, expected);
        assert_eq!(
            result.notices.iter().map(|n| n.line).collect::<Vec<_>>(),
            line.into_iter().collect::<Vec<_>>()
        );
    }
}
#[test]
fn action_slots_narrow_independently_and_invalid_opcode_is_none_without_parameter_reads() {
    // CreatureEventAIMgr.cpp:565-580, not an unknown-handler refusal.
    let mut row = row(1, 901, 4);
    row.words[12..24].copy_from_slice(&[65537, 1, 0, 3, 66, 99, 98, 97, 65541, 7, 8, 9]);
    let result = load_event_ai_row(&mut Loader::default(), row).unwrap();
    let EventAiDisposition::Loaded(event) = result.disposition else {
        panic!("skip")
    };
    assert_eq!(
        event.actions.map(|a| (a.kind, a.params)),
        [(1, Some([1, 0, 3])), (0, None), (5, Some([7, 8, 9]))]
    );
    assert!(result.notices.contains(&EventAiNotice {
        line: 570,
        slot: Some(2)
    }));
}
#[test]
fn missing_entry_guid_invalid_event_and_unimplemented_quests_have_distinct_loader_skips() {
    let mut loader = Loader::default();
    loader.missing.insert((EventAiLookup::Creature, 999));
    loader.missing.insert((EventAiLookup::Guid, 42));
    for (r, line) in [
        (row(1, 999, 4), 241),
        (row(2, -42, 4), 251),
        (row(3, 901, 43), 260),
        (row(4, 901, 19), 408),
    ] {
        assert_eq!(
            load_event_ai_row(&mut loader, r).unwrap().disposition,
            EventAiDisposition::Skipped { line }
        );
    }
    assert_eq!(
        load_event_ai_row(&mut loader, row(5, i32::MIN, 4))
            .unwrap_err()
            .id,
        "execution.event_ai.loader.negate_int_min_undefined"
    );
    assert_eq!(
        loaded(&mut loader, row(6, i32::MIN + 1, 4)).guid,
        i32::MAX as u32
    );
}
#[test]
fn row_parameter_repairs_preserve_source_flag_and_continue_quirks() {
    // :412-456 death falls through; emote missing condition skips despite logging a repair.
    let mut loader = Loader::default();
    loader.missing.insert((EventAiLookup::Condition, 5));
    let mut death = row(1, 901, 6);
    death.words[5] = 0x401;
    death.words[6] = 5;
    let event = loaded(&mut loader, death);
    assert_eq!((event.flags, event.params[0]), (0, 0));
    let mut emote = row(2, 901, 22);
    emote.words[6] = 1;
    emote.words[7] = 5;
    assert_eq!(
        load_event_ai_row(&mut loader, emote).unwrap().disposition,
        EventAiDisposition::Skipped { line: 448 }
    );
    let mut throw = row(3, 901, 4);
    throw.words[12] = 45;
    throw.words[13] = 12;
    throw.words[14] = 0;
    assert_eq!(
        loaded(&mut loader, throw).actions[0].params,
        Some([12, 0, 0])
    ); // :912 continues action loop, retains invalid type.
}
#[test]
fn cast_validation_uses_sql_metadata_and_friendly_hp_computes_target_self() {
    let mut loader = Loader::default();
    let mut info = spell();
    info.attributes_ex = 0x80000;
    loader.spells.insert(7, info);
    let mut r = row(1, 901, 14);
    r.words[12..16].copy_from_slice(&[11, 7, 12, 0x10]);
    let event = loaded(&mut loader, r);
    assert_eq!(event.friendly_hp_target_self, Some(false));
    assert_eq!(event.actions[0].params, Some([7, 12, 0x12]));
    // :704 sets NONE, then :730-733 changes it again only with the filter off.
    for (dev, expected) in [(true, 15), (false, 1)] {
        loader.dev = dev;
        let mut r = row(2, 901, 4);
        r.words[12..16].copy_from_slice(&[11, 7, 0, 0]);
        assert_eq!(
            loaded(&mut loader, r).actions[0].params,
            Some([7, expected, 0])
        );
    }
    let mut r = row(3, 901, 14);
    r.words[12..16].copy_from_slice(&[11, 8, 12, 0]);
    assert_eq!(
        load_event_ai_row(&mut loader, r).unwrap_err().id,
        "execution.event_ai.loader.friendly_hp_null_spell_undefined"
    );
}
#[test]
fn signed_validation_refuses_only_the_reached_abs_or_negation_site() {
    for kind in [13, 14, 23, 53] {
        let mut r = row(1, 901, 4);
        r.words[12] = kind;
        r.words[13] = i32::MIN as u32;
        let error = load_event_ai_row(&mut Loader::default(), r).unwrap_err();
        assert_eq!(
            error.id,
            if kind == 53 {
                "execution.event_ai.loader.negate_relay_int_min_undefined"
            } else {
                "execution.event_ai.loader.abs_int_min_undefined"
            }
        );
        let mut defined = row(2, 901, 4);
        defined.words[12] = kind;
        defined.words[13] = (i32::MIN + 1) as u32;
        assert_eq!(
            loaded(&mut Loader::default(), defined).actions[0]
                .params
                .unwrap()[0],
            (i32::MIN + 1) as u32
        );
    }
    let mut r = row(1, 901, 8);
    r.words[6] = 7;
    let mut loader = Loader::default();
    let mut s = spell();
    s.school = 31;
    loader.spells.insert(7, s.clone());
    assert_eq!(loaded(&mut loader, r.clone()).kind, 8);
    s.school = 32;
    loader.spells.insert(7, s);
    assert_eq!(
        load_event_ai_row(&mut loader, r).unwrap_err().id,
        "execution.event_ai.loader.school_shift_undefined"
    );
}
#[test]
fn entry_rows_precede_guid_rows_even_when_guid_id_is_lower() {
    let rows = vec![
        row(30, 901, 4),
        row(2, -41, 4),
        row(20, 901, 4),
        row(1, -42, 4),
    ];
    let (defs, _) = EventAiDefinitions::load(&mut Loader::default(), rows).unwrap();
    assert_eq!(
        defs.for_spawn(901, 41).map(|e| e.id).collect::<Vec<_>>(),
        [20, 30, 2]
    );
}
#[test]
fn shared_definitions_have_independent_spawn_holders_and_explicit_debug_build_identity() {
    // CreatureEventAI.cpp:112-162. A Rust debug build is not MANGOS_DEBUG.
    let mut defs = EventAiDefinitions::default();
    defs.insert(definition(1, 4));
    let mut debug = definition(2, 4);
    debug.flags = 0x80;
    defs.insert(debug);
    let defs = Arc::new(defs);
    let mut objects = SourceObjects::default();
    let mut a = state(&mut objects, defs.clone(), 41, EventAiBuild::Release);
    let mut b = state(&mut objects, defs, 42, EventAiBuild::MangosDebug);
    assert!(a.holders.is_empty());
    a.init(&objects, &mut Host::default()).unwrap();
    b.init(&objects, &mut Host::default()).unwrap();
    assert_eq!((a.holders.len(), b.holders.len()), (1, 2));
    a.holders[0].enabled = false;
    a.holders[0].timer = 99;
    assert_eq!(
        (
            b.holders[0].enabled,
            b.holders[0].timer,
            b.holders[0].in_progress,
            b.holders[0].event_target
        ),
        (true, 0, false, None)
    );
}
#[test]
fn main_spell_setup_refuses_but_distance_only_setup_needs_no_missing_spell_owner() {
    for (flags, expected) in [
        (0, Ok(())),
        (0x400, Ok(())),
        (
            0x100,
            Err("execution.event_ai.f5_07_main_spell_state_owner"),
        ),
    ] {
        let mut e = definition(1, 4);
        e.actions[2] = EventAiAction {
            kind: 11,
            params: Some([7, 1, flags]),
        };
        let mut defs = EventAiDefinitions::default();
        defs.insert(e);
        let mut objects = SourceObjects::default();
        let mut state = state(&mut objects, Arc::new(defs), 41, EventAiBuild::Release);
        assert_eq!(
            state.init(&objects, &mut Host::default()).map_err(|e| e.id),
            expected.map_err(str::to_owned)
        );
        assert_eq!(state.distance_spells.contains(&7), flags == 0x400);
        assert_eq!(state.main_spell_id, if flags == 0x100 { 7 } else { 0 });
    }
}
#[test]
fn strict_500_ms_boundary_and_large_diff_apply_one_shared_debt_pass() {
    let mut defs = EventAiDefinitions::default();
    let mut e = definition(1, 1);
    e.params = [10000, 10000, 0, 0, 0, 0];
    defs.insert(e);
    let mut objects = SourceObjects::default();
    let mut s = state(&mut objects, Arc::new(defs), 41, EventAiBuild::Release);
    s.just_respawned(
        &objects,
        settings(),
        &mut Host::default(),
        &mut Host::default(),
    )
    .unwrap();
    let mut host = Host::default();
    s.update_timers(500, &mut host).unwrap();
    assert_eq!(
        (s.holders[0].timer, s.clock.remaining, s.clock.debt),
        (10000, 0, 500)
    );
    assert!(host.checks.is_empty());
    s.update_timers(0, &mut host).unwrap();
    assert_eq!(s.holders[0].timer, 10000);
    s.update_timers(1, &mut host).unwrap();
    assert_eq!(
        (s.holders[0].timer, s.clock.remaining, s.clock.debt),
        (9499, 500, 0)
    );
    s.update_timers(20000, &mut host).unwrap();
    assert_eq!(s.holders[0].timer, 0);
    assert_eq!(host.dispatched, [1]);
    assert_eq!(s.clock.remaining, 500);
}
#[test]
fn timers_freeze_by_phase_even_disabled_ones_decrement_and_unreachable_bypasses_timer() {
    let mut defs = EventAiDefinitions::default();
    for (id, kind) in [(1, 1), (2, 1), (3, 36)] {
        defs.insert(definition(id, kind));
    }
    let mut objects = SourceObjects::default();
    let mut s = state(&mut objects, Arc::new(defs), 41, EventAiBuild::Release);
    s.init(&objects, &mut Host::default()).unwrap();
    s.clock.reset();
    for holder in &mut s.holders {
        holder.timer = 900;
        holder.enabled = false;
    }
    s.holders[0].event.inverse_phase = 1;
    let mut host = Host::default();
    s.update_timers(600, &mut host).unwrap();
    assert_eq!(
        s.holders.iter().map(|h| h.timer).collect::<Vec<_>>(),
        [900, 300, 900]
    );
    assert_eq!(host.checks, [3]);
    s.phase = 31;
    s.clock.reset();
    s.update_timers(501, &mut host).unwrap();
    assert_eq!(s.holders[0].timer, 399);
    s.phase = 32;
    s.clock.reset();
    assert_eq!(
        s.update_timers(501, &mut host).unwrap_err().id,
        "execution.event_ai.phase_shift_undefined"
    );
    for holder in &mut s.holders {
        holder.timer = 0;
    }
    s.clock.reset();
    s.update_timers(501, &mut host).unwrap();
}
#[test]
fn reset_combat_and_respawn_keep_the_source_timer_domains_separate() {
    let mut defs = EventAiDefinitions::default();
    for (id, kind) in [
        (1, 0),
        (2, 1),
        (3, 29),
        (4, 4),
        (5, 14),
        (6, 15),
        (7, 16),
        (8, 32),
    ] {
        let mut e = definition(id, kind);
        e.params = [10, 20, 30, 40, 0, 0];
        defs.insert(e);
    }
    let mut objects = SourceObjects::default();
    let mut s = state(&mut objects, Arc::new(defs), 41, EventAiBuild::Release);
    s.init(&objects, &mut Host::default()).unwrap();
    for h in &mut s.holders {
        h.timer = 99;
        h.enabled = false;
    }
    let mut random = Host::default();
    let mut host = Host::default();
    s.reset(&mut random, &mut host).unwrap();
    assert_eq!(
        s.holders
            .iter()
            .map(|h| (h.timer, h.enabled))
            .collect::<Vec<_>>(),
        [
            (99, false),
            (10, true),
            (99, false),
            (99, false),
            (0, true),
            (0, true),
            (0, true),
            (0, true)
        ]
    );
    assert_eq!(random.draws, [(10, 20)]);
    random.draws.clear();
    s.enter_combat(None, &objects, &mut random, &mut host)
        .unwrap();
    assert_eq!(
        random.draws,
        [(10, 20), (30, 40), (30, 40), (30, 40), (30, 40)]
    );
    assert_eq!(s.holders[2].timer, 99);
    assert_eq!(s.holders[1].timer, 10);
    assert_eq!(host.dispatched, [4]);
}
#[test]
fn respawn_resets_old_ooc_before_copying_new_holders_but_does_not_reconstruct_phase() {
    let mut defs = EventAiDefinitions::default();
    let mut e = definition(1, 1);
    e.params = [10, 20, 30, 40, 0, 0];
    defs.insert(e);
    let defs = Arc::new(defs);
    let mut objects = SourceObjects::default();
    let mut s = state(&mut objects, defs.clone(), 41, EventAiBuild::Release);
    let mut random = Host::default();
    let mut host = Host::default();
    s.just_respawned(&objects, settings(), &mut random, &mut host)
        .unwrap();
    assert_eq!(random.draws, [(10, 20)]);
    s.phase = 7;
    s.throw_mask = 9;
    s.has_ooc_los = true;
    s.holders[0].timer = 99;
    // Reset and InitAI discard this old reference without dereferencing it.
    s.holders[0].event_target = Some(ObjectHandle {
        thread: 999,
        construction: 1,
    });
    assert_eq!(
        s.inventory(&objects).unwrap_err().id,
        "execution.stale_object_handle"
    );
    random.draws.clear();
    s.just_respawned(&objects, settings(), &mut random, &mut host)
        .unwrap();
    assert_eq!(random.draws, [(10, 20), (10, 20)]);
    assert_eq!((s.phase, s.throw_mask, s.has_ooc_los), (7, 9, true));
    assert_eq!(s.holders[0].event_target, None);
    let fresh = state(&mut objects, defs, 42, EventAiBuild::Release);
    assert_eq!(
        (fresh.phase, fresh.throw_mask, fresh.has_ooc_los),
        (0, 0, false)
    );
}
#[test]
fn lifecycle_callbacks_run_before_and_after_timer_changes_in_source_order() {
    let mut defs = EventAiDefinitions::default();
    defs.insert(definition(1, 11));
    defs.insert(definition(2, 21));
    let mut objects = SourceObjects::default();
    let mut s = state(&mut objects, Arc::new(defs), 41, EventAiBuild::Release);
    let mut host = Host::default();
    s.just_respawned(
        &objects,
        EventAiRespawnSettings {
            no_aggro_on_sight: false,
            spawn_defensive: true,
            ignore_combat: true,
        },
        &mut Host::default(),
        &mut host,
    )
    .unwrap();
    assert_eq!(
        host.calls,
        [
            "ResetAllBaseTimers",
            "RestoreRangedModeAndAttackDistance",
            "SetAiOrderNone",
            "ClearCombatScriptStatus",
            "ClearCombatOnlyRoot",
            "SetReactDefensive(false)",
            "SetReactDefensive(true)",
            "Process(1)"
        ]
    );
    assert_eq!(host.dispatched, [1]);
    host.calls.clear();
    s.just_reached_home(&mut Host::default(), &mut host)
        .unwrap();
    assert_eq!(host.dispatched, [1, 2]);
    assert_eq!(
        host.calls,
        [
            "Process(2)",
            "RestoreRangedModeAndAttackDistance",
            "SetAiOrderNone",
            "ClearCombatScriptStatus",
            "ClearCombatOnlyRoot"
        ]
    );
    assert_eq!((s.depth, s.staged[0].len()), (0, 0));
}
#[test]
fn missing_event_handlers_remain_refusals_not_loader_skips() {
    struct Missing;
    impl EventAiLifecycleOwner for Missing {}
    let mut defs = EventAiDefinitions::default();
    defs.insert(definition(1, 38));
    defs.insert(definition(2, 1));
    let mut objects = SourceObjects::default();
    let mut s = state(&mut objects, Arc::new(defs), 41, EventAiBuild::Release);
    s.init(&objects, &mut Missing).unwrap();
    assert_eq!(s.holders.len(), 2);
    assert_eq!(
        s.update_timers(1, &mut Missing).unwrap_err().id,
        "execution.event_ai.f5_03_event_check_owner"
    );
}
#[test]
fn retained_catalog04_eventai_disposition_and_missing_owner_census_or_explicit_absence() {
    let Some(root) = std::env::var_os("BENILLA_F5_AI_CATALOG") else {
        println!("SKIP catalog-04 EventAI loader census: set BENILLA_F5_AI_CATALOG");
        return;
    };
    let mut catalog =
        super::super::production_compiler::VerifiedCatalog::open(Path::new(&root), false).unwrap();
    assert_eq!(
        catalog.report()["catalog_sha256"],
        "757dcfb85e2b2a0de6571b0fe981c214fa57a4e2e546968ca61171113f379bd6"
    );
    let mut rows = catalog_event_ai_rows(&mut catalog).unwrap();
    assert_eq!(rows.len(), 11469);
    rows.sort_by_key(|r| r.words[0]);
    let config_path = std::env::var_os("BENILLA_F5_EVENT_AI_CONFIG")
        .ok_or_else(|| refuse("execution.event_ai.config_filter_missing"))
        .unwrap();
    // seal16-full-01/mangosd.conf:432,1062; full-audit/effective-config-audit.json file identity.
    let config = EventAiLoaderConfig::recorded(
        Path::new(&config_path),
        &Sha256Id::parse("321f2dc486e38f77037366bf84fb0ce8d09ed823ffa4524082ab01a8b0b79244")
            .unwrap(),
    )
    .unwrap();
    assert!(config.dev_log_filter());
    assert_eq!(config.event_ai_dev_line, 432);
    let mut owner = EventAiCatalog::new(&mut catalog, config);
    let mut dispositions = BTreeMap::new();
    let mut reasons = BTreeMap::new();
    let mut notices = BTreeMap::new();
    let mut loaded = 0;
    let mut main = 0;
    let mut distance = 0;
    for row in rows {
        match load_event_ai_row(&mut owner, row) {
            Ok(result) => {
                for notice in result.notices {
                    *notices.entry(notice.line).or_insert(0usize) += 1;
                }
                match result.disposition {
                    EventAiDisposition::Loaded(event) => {
                        loaded += 1;
                        *dispositions.entry("loaded".to_owned()).or_insert(0usize) += 1;
                        for action in event.actions {
                            if action.kind == 11 {
                                let p = action.params.unwrap();
                                main += usize::from(p[2] & 0x100 != 0);
                                distance += usize::from(p[2] & 0x400 != 0);
                            }
                        }
                    }
                    EventAiDisposition::Skipped { line } => {
                        *dispositions.entry(format!("skip:{line}")).or_insert(0) += 1;
                    }
                }
            }
            Err(error) => {
                *dispositions.entry("refused".into()).or_insert(0) += 1;
                *reasons.entry(error.id).or_insert(0usize) += 1;
            }
        }
    }
    assert_eq!(dispositions.values().sum::<usize>(), 11469);
    assert!(loaded > 0);
    println!("catalog-04 EventAI census: dispositions={dispositions:?}; refusal_reasons={reasons:?}; notices={notices:?}; loaded_main_slots={main}; loaded_distance_slots={distance}; raw catalog operands, not sealed F1 acceptance");
}

fn update_template() -> CreatureAiTemplateOperands {
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
        stats_multipliers: [0x3f800000; 3],
        civilian: false,
        pursuit_ms: 15000,
        call_for_help: 0,
        creature_type: 0,
        ai_name: "EventAI".into(),
        script_name: String::new(),
    }
}

impl UnitAiExecutionOwner for Host {}
impl BuiltinAiExecutionOwner for Host {}
impl HostileTargetExecutionOwner for Host {}

#[test]
fn inherited_eventai_update_runs_timers_selection_event_timers_then_ooc_return() {
    let mut objects = SourceObjects::default();
    let mut defs = EventAiDefinitions::default();
    // Combat-only timer expires but does not read missing vitals out of combat.
    defs.insert(definition(1, 0));
    let mut state = state(&mut objects, Arc::new(defs), 41, EventAiBuild::Release);
    state.init(&objects, &mut Host::default()).unwrap();
    state.holders[0].timer = 5;
    let unit = state.selected.object();
    objects.get_mut(unit).unwrap().alive = true;
    let mut units = SourceUnits::default();
    units
        .register(
            &objects,
            unit,
            SourceUnitFields::creature(&update_template()),
        )
        .unwrap();
    let bytes = b"LogFilter_EventAiDev=1\nGuarderSight=50\n";
    let config = EventAiLoaderConfig::parse(bytes, Sha256Id::of(bytes)).unwrap();
    let base = SourceCreatureAi::construct(
        &objects,
        unit,
        &mut units,
        &update_template(),
        false,
        0,
        &config,
        &mut Host::default(),
    )
    .unwrap();
    assert!(base.base.spell_list_cooldown); // UnitAI.cpp:58.
    let mut ais = SourceCreatureAis::default();
    ais.install(&objects, &mut units, base, &mut SourceCombat::default())
        .unwrap();
    let timer = &mut ais.get_mut(&objects, unit).unwrap().base.generic_timers[0];
    timer.remaining = 5;
    timer.disabled = false;
    ais.get_mut(&objects, unit)
        .unwrap()
        .base
        .distancing_cooldown = true;
    let mut threats = SourceThreats::default();
    // Selection fails after the base timer, before EventAI timers.
    let mut host = Host::default();
    let mut execution = SourceAiExecution {
        queries: SourceUnitQueries {
            objects: &objects,
            units: &mut units,
        },
        ais: &mut ais,
        threats: &mut threats,
        owner: &mut host,
    };
    assert!(state.update(501, &mut execution).is_err());
    assert!(
        !execution
            .ais
            .get(&objects, unit)
            .unwrap()
            .base
            .distancing_cooldown
    );
    assert_eq!(state.holders[0].timer, 5);
    execution.threats.register(unit).unwrap();
    state.update(501, &mut execution).unwrap();
    assert_eq!(state.holders[0].timer, 0);
    assert_eq!(
        state.clock,
        benilla_sim::event_ai::EventClock {
            remaining: 500,
            debt: 0
        }
    );
    // UnitAI.cpp:1088 can return true while casting with an empty threat list.
    // Event timers still execute before ExecuteActions and UpdateSpellLists.
    let fields = execution.queries.units.get_mut(&objects, unit).unwrap();
    fields.casting_non_melee = true;
    fields.flags |= UNIT_FLAG_COMBAT;
    // UnitAI.cpp:104-109 combat activation, before unavailable initial spell cooldowns.
    execution
        .ais
        .get_mut(&objects, unit)
        .unwrap()
        .base
        .spell_list_cooldown = false;
    state.holders[0].event.kind = 1;
    state.holders[0].timer = 5;
    assert_eq!(
        state.update(501, &mut execution).unwrap_err().id,
        "execution.builtin_ai.f5_08_update_spell_lists_owner"
    );
    assert_eq!(state.holders[0].timer, 0);
    assert_eq!(state.clock.debt, 0);
    // CreatureEventAI.cpp:1950 has undefined shift at 32, not at 31.
    execution
        .queries
        .units
        .get_mut(&objects, unit)
        .unwrap()
        .flags &= !UNIT_FLAG_COMBAT;
    execution
        .queries
        .units
        .get_mut(&objects, unit)
        .unwrap()
        .casting_non_melee = false;
    state.phase = 31;
    state.holders[0].timer = 1000;
    state.update(501, &mut execution).unwrap();
    assert_eq!(state.holders[0].timer, 499);
    state.phase = 32;
    assert_eq!(
        state.update(501, &mut execution).unwrap_err().id,
        "execution.event_ai.phase_shift_undefined"
    );
    assert_eq!(state.holders[0].timer, 499);
    assert!(host.draws.is_empty());
}

#[test]
fn timer_based_classifier_preserves_callback_and_non_timer_domains() {
    // CreatureEventAI.cpp:209-241. Callback timers are not periodic checks.
    let expected = [
        0, 1, 2, 3, 5, 8, 9, 10, 12, 13, 14, 15, 16, 17, 18, 23, 24, 25, 26, 27, 28, 29, 31, 32,
        33, 34,
    ];
    for kind in 0..=40 {
        assert_eq!(
            benilla_sim::event_ai::timer_based_event(kind),
            expected.contains(&kind),
            "event {kind}"
        );
    }
    assert!(!benilla_sim::event_ai::timer_based_event(u32::MAX));
    for kind in [5, 8, 10, 17, 25, 26, 34] {
        assert!(!benilla_sim::event_ai::timer_executed_event(kind));
    }
}

#[test]
fn retained_catalog04_inherited_eventai_update_census_or_explicit_absence() {
    let Some(root) = std::env::var_os("BENILLA_F5_AI_CATALOG") else {
        println!("SKIP EventAI inherited update census: set BENILLA_F5_AI_CATALOG, BENILLA_F5_SCRIPT_REPORT and BENILLA_F5_EVENT_AI_CONFIG");
        return;
    };
    let report = std::env::var_os("BENILLA_F5_SCRIPT_REPORT").unwrap();
    let config_path = std::env::var_os("BENILLA_F5_EVENT_AI_CONFIG").unwrap();
    let config = EventAiLoaderConfig::recorded(
        Path::new(&config_path),
        &Sha256Id::parse("321f2dc486e38f77037366bf84fb0ce8d09ed823ffa4524082ab01a8b0b79244")
            .unwrap(),
    )
    .unwrap();
    let mut catalog =
        super::super::production_compiler::VerifiedCatalog::open(Path::new(&root), false).unwrap();
    assert_eq!(
        catalog.report()["catalog_sha256"],
        "757dcfb85e2b2a0de6571b0fe981c214fa57a4e2e546968ca61171113f379bd6"
    );
    let mut builder = AiRegistryBuilder::builtin();
    builder
        .load_catalog_scripts(&catalog, Path::new(&report))
        .unwrap();
    let registry = builder.freeze();
    let factions = CreatureAiFactions::load_catalog(&mut catalog).unwrap();
    let mut spawns = BTreeMap::<u32, Vec<u32>>::new();
    for (_, row) in catalog
        .execution_event_ai_operands("creature", ["guid", "id"])
        .unwrap()
    {
        spawns.entry(row[1]).or_default().push(row[0]);
    }
    let rows = catalog_event_ai_rows(&mut catalog).unwrap();
    let mut definitions = EventAiDefinitions::default();
    let mut unavailable = BTreeMap::<i32, String>::new();
    let mut loader = EventAiCatalog::new(&mut catalog, config.clone());
    for row in rows {
        let key = row.words[1] as i32;
        match load_event_ai_row(&mut loader, row) {
            Ok(EventAiLoad {
                disposition: EventAiDisposition::Loaded(event),
                ..
            }) => definitions.insert(event),
            Ok(_) => {}
            Err(error) => {
                unavailable.entry(key).or_insert(error.id);
            }
        }
    }
    let definitions = Arc::new(definitions);
    let mut counts = BTreeMap::<String, [usize; 2]>::new();
    let mut totals = [0; 2];
    for t in catalog_creature_ai_census_inputs(&mut catalog).unwrap() {
        // Templates use GUID zero; each raw spawn is also probed with its own GUID rows.
        for (column, guid) in std::iter::once((0, 0)).chain(
            spawns
                .get(&t.entry)
                .into_iter()
                .flatten()
                .copied()
                .map(|guid| (1, guid)),
        ) {
            let mut objects = SourceObjects::default();
            let unit = objects.construct(1, SourceObjectKind::Creature).unwrap();
            objects.get_mut(unit).unwrap().database_guid = guid;
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
            if selected.executable().builtin_kind() != Some(BuiltinCreatureAi::Event) {
                continue;
            }
            totals[column] += 1;
            let mut units = SourceUnits::default();
            units
                .register(&objects, unit, SourceUnitFields::creature(&t))
                .unwrap();
            let mut mt = benilla_sim::source_rng::Mt19937::from_seed(5489);
            let mut cursor = benilla_sim::source_rng::DrawCursor::default();
            let mut sink = |_| {};
            let mut host =
                benilla_sim::source_rng::SourceRandom::new(1, &mut mt, &mut cursor, &mut sink)
                    .unwrap();
            let result = (|| -> Result<()> {
                let base = SourceCreatureAi::construct(
                    &objects, unit, &mut units, &t, false, 0, &config, &mut host,
                )?;
                let mut ais = SourceCreatureAis::default();
                ais.install(&objects, &mut units, base, &mut SourceCombat::default())?;
                units.bind_ai_selection(&objects, &selected)?;
                if let Some(reason) = unavailable
                    .get(&(t.entry as i32))
                    .or_else(|| unavailable.get(&-(guid as i32)))
                {
                    return Err(refuse(reason));
                }
                let mut state = SourceEventAi::construct(
                    selected,
                    &objects,
                    Arc::clone(&definitions),
                    EventAiBuild::Release,
                )?;
                let mut threats = SourceThreats::default();
                threats.register(unit)?;
                let mut execution = SourceAiExecution {
                    queries: SourceUnitQueries {
                        objects: &objects,
                        units: &mut units,
                    },
                    ais: &mut ais,
                    threats: &mut threats,
                    owner: &mut host,
                };
                state.init(&objects, &mut EventAiDispatcher(&mut execution))?;
                state.update(100, &mut execution)
            })()
            .map_or_else(|error| error.id, |_| "completed".into());
            counts.entry(result).or_default()[column] += 1;
        }
    }
    assert_eq!(totals, [9865, 60209]);
    assert_eq!(counts.values().map(|n| n[0]).sum::<usize>(), totals[0]);
    assert_eq!(counts.values().map(|n| n[1]).sum::<usize>(), totals[1]);
    println!(
        "F5_EVENTAI_UPDATE_CENSUS {}",
        serde_json::json!({
            "scope":"ordinary uncontrolled template and independent raw-spawn alternatives; copied loaded holders, constructed clock, diff100, absent vitals; not respawn, initialized world, F1 or admission",
            "columns":["templates","raw_spawn_rows"], "totals":totals, "first_stop":counts,
            "format":91, "capability":"uninhabited"
        })
    );
}

impl<E: benilla_sim::source_rng::RawGenerator, S: FnMut(benilla_sim::source_rng::TraceEvent)>
    UnitAiExecutionOwner for benilla_sim::source_rng::SourceRandom<'_, E, S>
{
}
impl<E: benilla_sim::source_rng::RawGenerator, S: FnMut(benilla_sim::source_rng::TraceEvent)>
    BuiltinAiExecutionOwner for benilla_sim::source_rng::SourceRandom<'_, E, S>
{
}
impl<E: benilla_sim::source_rng::RawGenerator, S: FnMut(benilla_sim::source_rng::TraceEvent)>
    HostileTargetExecutionOwner for benilla_sim::source_rng::SourceRandom<'_, E, S>
{
}
