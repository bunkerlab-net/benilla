use super::*;
use std::collections::{BTreeMap, BTreeSet};
pub(super) fn handle(id: u64) -> ObjectHandle {
    ObjectHandle {
        thread: 1,
        construction: id,
    }
}
pub(super) fn value(n: u32) -> ThreatValue {
    ThreatValue::from_u32(n)
}
#[derive(Default)]
pub(super) struct Host {
    pub units: BTreeMap<ObjectHandle, ThreatUnit>,
    pub melee: BTreeSet<ObjectHandle>,
    pub offline: BTreeSet<ObjectHandle>,
    pub rooted: bool,
    pub linking: Vec<(ObjectHandle, ObjectHandle)>,
    pub log: Vec<String>,
    pub combat: BTreeMap<ObjectHandle, CombatUnit>,
    pub prevented_damage: Option<u32>,
    pub dealt_damage: Option<u32>,
    pub taken_damage: Option<u32>,
    pub reject_kill: bool,
}
impl Host {
    pub fn unit(&mut self, id: u64, player: bool) -> ObjectHandle {
        let object = handle(id);
        self.units.insert(
            object,
            ThreatUnit {
                guid: id,
                player,
                gm: false,
                alive: true,
                no_threat: false,
                master: None,
                feign_death: false,
                can_have_threat_list: !player,
            },
        );
        self.combat.insert(
            object,
            CombatUnit {
                health: 100,
                alive: true,
                taxi: false,
                evade: false,
                has_ai: true,
                unkillable: false,
                preventing_death: false,
            },
        );
        self.melee.insert(object);
        object
    }
}
impl ThreatExecutionOwner for Host {
    fn can_enter_combat(&mut self, _unit: ObjectHandle) -> Result<bool> {
        Ok(true)
    }
    fn crowd_controlled(&mut self, _unit: ObjectHandle) -> Result<bool> {
        Ok(false)
    }
    fn threat_unit(&mut self, unit: ObjectHandle) -> Result<ThreatUnit> {
        self.units
            .get(&unit)
            .copied()
            .ok_or_else(|| refuse("authored.stale_generation"))
    }
    fn threat_lookup(&mut self, _source: ObjectHandle, guid: u64) -> Result<Option<ObjectHandle>> {
        Ok(self
            .units
            .iter()
            .find(|(_, u)| u.guid == guid)
            .map(|(h, _)| *h))
    }
    fn offline_target(&mut self, _source: ObjectHandle, target: ObjectHandle) -> Result<bool> {
        Ok(self.offline.contains(&target))
    }
    fn can_join_attacking(&mut self, _master: ObjectHandle, _source: ObjectHandle) -> Result<bool> {
        Ok(true)
    }
    fn ignoring_ranged(&mut self, _source: ObjectHandle) -> Result<bool> {
        Ok(self.rooted)
    }
    fn melee_reach(&mut self, _source: ObjectHandle, target: ObjectHandle) -> Result<bool> {
        Ok(self.melee.contains(&target))
    }
    fn can_attack(&mut self, _source: ObjectHandle, _target: ObjectHandle) -> Result<bool> {
        Ok(true)
    }
    fn suppressed_target(&mut self, _source: ObjectHandle, _target: ObjectHandle) -> Result<bool> {
        Ok(false)
    }
    fn total_threat_mod(
        &mut self,
        _target: ObjectHandle,
        _school: u32,
        value: ThreatValue,
    ) -> Result<ThreatValue> {
        Ok(value)
    }
    fn spell_threat_mod(
        &mut self,
        _target: ObjectHandle,
        _spell: u32,
        value: ThreatValue,
    ) -> Result<ThreatValue> {
        Ok(value)
    }
    fn critical_threat_mod(&mut self, _target: ObjectHandle, _school: u32) -> Result<ThreatValue> {
        Ok(value(2))
    }
    fn aggro_linking(
        &mut self,
        _state: &mut SourceThreats,
        source: ObjectHandle,
        target: ObjectHandle,
    ) -> Result<()> {
        self.linking.push((source, target));
        Ok(())
    }
    fn combat_timer(&mut self, source: ObjectHandle, target: ObjectHandle) -> Result<()> {
        self.log.push(format!(
            "timer {} {}",
            source.construction, target.construction
        ));
        Ok(())
    }
}
pub(super) fn setup() -> (
    SourceThreats,
    Host,
    ObjectHandle,
    ObjectHandle,
    ObjectHandle,
) {
    let mut host = Host::default();
    let source = host.unit(1, false);
    let npc = host.unit(2, false);
    let pet = host.unit(3, false);
    let mut state = SourceThreats::default();
    state.register(source).unwrap();
    (state, host, source, npc, pet)
}
pub(super) fn add(
    state: &mut SourceThreats,
    host: &mut Host,
    source: ObjectHandle,
    target: ObjectHandle,
    amount: u32,
) {
    state
        .add(source, Some(target), value(amount), false, 0, None, host)
        .unwrap();
}
#[test]
fn npc_and_pet_targets_tie_stably_then_switch_above_binary32_threshold() {
    let (mut state, mut host, s, a, b) = setup();
    add(&mut state, &mut host, s, a, 100);
    add(&mut state, &mut host, s, b, 100);
    assert_eq!(state.select(s, &mut host).unwrap(), Some(a));
    add(&mut state, &mut host, s, b, 10);
    assert_eq!(state.select(s, &mut host).unwrap(), Some(a));
    add(&mut state, &mut host, s, b, 1);
    assert_eq!(state.select(s, &mut host).unwrap(), Some(b));
    assert_eq!(state.hated_by(a), &[s]);
    assert_eq!(host.linking, vec![(s, a), (s, b)]);
    state.validate(&mut host).unwrap();
}
#[test]
fn runtime_both_sides_of_melee_and_ranged_switch_thresholds() {
    for (melee, amount, expected) in [
        // ThreatManager.cpp:398, 1.1f * 100.f rounds to exactly 110.f.
        (true, 0x42dc0000, false),
        (true, 0x42dc0001, true),
        (false, 0x43020000, false),
        (false, 0x43020001, true),
    ] {
        let (mut state, mut host, s, a, b) = setup();
        add(&mut state, &mut host, s, a, 100);
        state.select(s, &mut host).unwrap();
        if !melee {
            host.melee.remove(&b);
        }
        state
            .add_direct(s, b, 3, ThreatValue::from_bits(amount), false, &mut host)
            .unwrap();
        assert_eq!(
            state.select(s, &mut host).unwrap(),
            Some(if expected { b } else { a })
        );
    }
}
#[test]
fn ranged_challenger_below_130_does_not_hide_next_melee_challenger() {
    let (mut state, mut host, s, a, b) = setup();
    let c = host.unit(4, false);
    add(&mut state, &mut host, s, a, 100);
    state.select(s, &mut host).unwrap();
    add(&mut state, &mut host, s, b, 120);
    add(&mut state, &mut host, s, c, 115);
    host.melee.remove(&b);
    assert_eq!(state.select(s, &mut host).unwrap(), Some(c));
}
#[test]
fn online_offline_order_and_dirty_quirk_are_not_recomputed_on_add() {
    let (mut state, mut host, s, a, b) = setup();
    add(&mut state, &mut host, s, a, 100);
    state.select(s, &mut host).unwrap();
    host.offline.insert(b);
    add(&mut state, &mut host, s, b, 100);
    assert_eq!(state.manager(s).unwrap().offline().len(), 1);
    host.offline.remove(&b);
    add(&mut state, &mut host, s, b, 20);
    assert_eq!(state.manager(s).unwrap().offline()[0].threat, value(120));
    assert_eq!(state.manager(s).unwrap().online().len(), 1);
    state.update_online(s, 3, &mut host).unwrap();
    assert!(state.manager(s).unwrap().dirty());
    assert_eq!(state.select(s, &mut host).unwrap(), Some(b));
    state.set_online(s, 3, false).unwrap();
    assert_eq!(state.manager(s).unwrap().current(), None);
    assert_eq!(state.select(s, &mut host).unwrap(), Some(a));
}
#[test]
fn offline_return_appends_without_sort_below_110_even_when_more_hated() {
    let (mut state, mut host, s, a, b) = setup();
    add(&mut state, &mut host, s, a, 100);
    state.select(s, &mut host).unwrap();
    add(&mut state, &mut host, s, b, 105);
    state.set_online(s, 3, false).unwrap();
    state.select(s, &mut host).unwrap();
    state.set_online(s, 3, true).unwrap();
    assert!(!state.manager(s).unwrap().dirty());
    assert_eq!(state.select(s, &mut host).unwrap(), Some(a));
    assert_eq!(state.manager(s).unwrap().online()[0].guid, 2);
}
#[test]
fn zero_percent_remove_and_negative_add_keep_reverse_references_consistent() {
    let (mut state, mut host, s, a, b) = setup();
    add(&mut state, &mut host, s, a, 7);
    add(&mut state, &mut host, s, b, 9);
    state.modify_percent(s, 2, -100, &mut host).unwrap();
    assert_eq!(
        state.manager(s).unwrap().threat(2, false).unwrap(),
        value(0)
    );
    assert_eq!(state.hated_by(a), &[s]);
    state.modify_percent(s, 2, -101, &mut host).unwrap();
    assert!(state.hated_by(a).is_empty());
    state
        .add(
            s,
            Some(b),
            ThreatValue::from_i32(-100),
            false,
            0,
            None,
            &mut host,
        )
        .unwrap();
    assert_eq!(
        state.manager(s).unwrap().threat(3, false).unwrap(),
        value(0)
    );
    state.set_online(s, 3, false).unwrap();
    state.modify_all_percent(s, -101, &mut host).unwrap();
    assert_eq!(state.manager(s).unwrap().offline().len(), 1);
    state.clear(s).unwrap();
    state.validate(&mut host).unwrap();
    assert!(state.hated_by(b).is_empty());
}
#[test]
fn pet_master_gets_zero_entry_after_linking_and_before_offline_test() {
    let (mut state, mut host, s, master, pet) = setup();
    host.units.get_mut(&pet).unwrap().master = Some(master);
    add(&mut state, &mut host, s, pet, 12);
    assert_eq!(host.linking, vec![(s, pet), (s, master)]);
    assert_eq!(
        state
            .manager(s)
            .unwrap()
            .online()
            .iter()
            .map(|r| r.guid)
            .collect::<Vec<_>>(),
        vec![3, 2]
    );
    assert_eq!(
        state.manager(s).unwrap().threat(2, false).unwrap(),
        value(0)
    );
    let (mut state, mut host, s, master, pet) = setup();
    host.units.get_mut(&pet).unwrap().master = Some(master);
    host.units.get_mut(&master).unwrap().feign_death = true;
    add(&mut state, &mut host, s, pet, 12);
    assert_eq!(state.manager(s).unwrap().online().len(), 1);
}
#[test]
fn self_null_gm_and_dead_guards_precede_threat_and_zero_no_threat_entries_remain() {
    let (mut state, mut host, s, a, b) = setup();
    state
        .add(s, Some(s), value(5), false, 0, None, &mut host)
        .unwrap();
    state
        .add(s, None, value(5), false, 0, None, &mut host)
        .unwrap();
    host.units.get_mut(&a).unwrap().player = true;
    host.units.get_mut(&a).unwrap().gm = true;
    add(&mut state, &mut host, s, a, 5);
    host.units.get_mut(&b).unwrap().alive = false;
    add(&mut state, &mut host, s, b, 5);
    assert!(state.manager(s).unwrap().online().is_empty());
    host.units.get_mut(&b).unwrap().alive = true;
    host.units.get_mut(&b).unwrap().no_threat = true;
    add(&mut state, &mut host, s, b, 5);
    assert_eq!(
        state.manager(s).unwrap().threat(3, false).unwrap(),
        value(0)
    );
    assert_eq!(state.hated_by(b), &[s]);
}
#[test]
fn no_new_only_blocks_creation_and_unit_add_differs_from_manager_add() {
    let (mut state, mut host, s, a, _) = setup();
    let spell = ThreatSpell {
        id: 7,
        no_threat: true,
        no_harmful_threat: false,
    };
    state
        .add(s, Some(a), value(5), false, 1, Some(spell), &mut host)
        .unwrap();
    assert!(state.manager(s).unwrap().online().is_empty());
    add(&mut state, &mut host, s, a, 5);
    state
        .add(s, Some(a), value(5), true, 1, Some(spell), &mut host)
        .unwrap();
    assert_eq!(
        state.manager(s).unwrap().threat(2, false).unwrap(),
        value(15)
    );
    host.units.get_mut(&s).unwrap().can_have_threat_list = false;
    state
        .unit_add(s, Some(a), value(100), false, 0, None, &mut host)
        .unwrap();
    assert_eq!(
        state.manager(s).unwrap().threat(2, false).unwrap(),
        value(15)
    );
    host.units.get_mut(&s).unwrap().player = true;
    state
        .add(s, Some(a), value(100), false, 0, None, &mut host)
        .unwrap();
    assert_eq!(
        state.manager(s).unwrap().threat(2, false).unwrap(),
        value(15)
    );
}
#[test]
fn player_preference_is_a_selection_quirk_even_for_nonplayer_list_owner() {
    let (mut state, mut host, s, a, b) = setup();
    add(&mut state, &mut host, s, b, 0);
    add(&mut state, &mut host, s, a, 0);
    // ThreatManager.cpp:591-602 sets current without sorting. Increasing current
    // threat does not dirty the list at :617-619, so b remains before a.
    state.set_current(s, Some(2)).unwrap();
    add(&mut state, &mut host, s, a, 100);
    host.units.get_mut(&b).unwrap().player = true;
    // With b before a, the unconditional IsPlayer preference beats lower threat.
    assert_eq!(state.select(s, &mut host).unwrap(), Some(b));
}
#[test]
fn taunt_fixate_suppression_and_root_priority_precede_numeric_switch() {
    let (mut state, mut host, s, a, b) = setup();
    add(&mut state, &mut host, s, a, 100);
    add(&mut state, &mut host, s, b, 1);
    state.select(s, &mut host).unwrap();
    state.taunt_update(s, &[3]).unwrap();
    assert_eq!(state.select(s, &mut host).unwrap(), Some(b));
    state.fixate(s, Some(2), &[3]).unwrap();
    assert_eq!(state.select(s, &mut host).unwrap(), Some(a));
    state.fixate(s, None, &[]).unwrap();
    state.suppress(s, 2, &mut host).unwrap();
    assert_eq!(state.select(s, &mut host).unwrap(), Some(b));
    add(&mut state, &mut host, s, a, 1);
    assert!(
        !state
            .manager(s)
            .unwrap()
            .online()
            .iter()
            .find(|r| r.guid == 2)
            .unwrap()
            .suppressed
    );
    host.rooted = true;
    host.melee.remove(&a);
    assert_eq!(state.select(s, &mut host).unwrap(), Some(b));
    host.melee.remove(&b);
    assert_eq!(state.select(s, &mut host).unwrap(), None);
}
#[test]
fn invalidated_guid_rebinds_to_new_generation_and_stale_live_handle_refuses() {
    let (mut state, mut host, s, a, _) = setup();
    add(&mut state, &mut host, s, a, 8);
    state.invalidate(a).unwrap();
    host.units.remove(&a);
    let new = host.unit(20, false);
    host.units.get_mut(&new).unwrap().guid = 2;
    state.update_online(s, 2, &mut host).unwrap();
    assert_eq!(state.select(s, &mut host).unwrap(), Some(new));
    assert_eq!(state.hated_by(new), &[s]);
    state.validate(&mut host).unwrap();
    host.units.remove(&new);
    assert_eq!(
        state.validate(&mut host).unwrap_err().id,
        "authored.stale_generation"
    );
}
#[test]
fn undefined_empty_selection_and_offline_missing_lookup_refuse_only_when_reached() {
    let (mut state, mut host, s, a, _) = setup();
    assert_eq!(state.select_if_nonempty(s, &mut host).unwrap(), None);
    assert_eq!(
        state.select(s, &mut host).unwrap_err().id,
        "execution.threat.empty_select_end_decrement_undefined"
    );
    assert_eq!(
        state.manager(s).unwrap().threat(2, false).unwrap(),
        value(0)
    );
    assert_eq!(
        state.manager(s).unwrap().threat(2, true).unwrap_err().id,
        "execution.threat.get_threat_missing_offline_null_undefined"
    );
    add(&mut state, &mut host, s, a, 9);
    state.set_online(s, 2, false).unwrap();
    assert_eq!(state.manager(s).unwrap().threat(2, true).unwrap(), value(9));
}
#[test]
fn eventai_all_percent_undefined_erased_iterator_refuses_after_first_actual_removal() {
    let (mut state, mut host, s, a, b) = setup();
    add(&mut state, &mut host, s, a, 2);
    add(&mut state, &mut host, s, b, 4);
    state.event_ai_all_percent(s, -100, &mut host).unwrap();
    assert_eq!(state.manager(s).unwrap().highest(), value(0));
    assert_eq!(
        state
            .event_ai_all_percent(s, -101, &mut host)
            .unwrap_err()
            .id,
        "execution.threat.event_ai_erased_iterator_undefined"
    );
    assert!(state.hated_by(a).is_empty());
    assert_eq!(state.hated_by(b), &[s]);
}
#[test]
fn reverse_reference_order_and_destruction_cover_multiple_haters() {
    let (mut state, mut host, s, a, b) = setup();
    state.register(b).unwrap();
    add(&mut state, &mut host, s, a, 5);
    add(&mut state, &mut host, b, a, 6);
    assert_eq!(state.hated_by(a), &[b, s]);
    state.delete_references(a).unwrap();
    assert!(state.manager(s).unwrap().online().is_empty());
    assert!(state.manager(b).unwrap().online().is_empty());
    state.validate(&mut host).unwrap();
}
#[test]
fn component_missing_owners_never_become_defaults() {
    struct Missing;
    impl ThreatExecutionOwner for Missing {}
    let (mut state, _, s, a, _) = setup();
    assert_eq!(
        state
            .add(s, Some(a), value(1), false, 0, None, &mut Missing)
            .unwrap_err()
            .id,
        "execution.threat.f5_04b_unit_fields_owner"
    );
}

struct EventHost {
    state: SourceThreats,
    host: Host,
}
impl CreatureRandom for EventHost {
    fn creature_urand(&mut self, _min: u32, _max: u32) -> Result<u32> {
        panic!("threat actions must not draw")
    }
    fn creature_irand(&mut self, _min: i32, _max: i32) -> Result<i32> {
        panic!("threat actions must not draw")
    }
}
impl EventAiExecutionOwner for EventHost {
    fn threat_context(&mut self) -> Result<ThreatContext<'_>> {
        Ok(ThreatContext {
            state: &mut self.state,
            owner: &mut self.host,
        })
    }
}
#[test]
fn eventai_single_direct_percent_and_all_actions_call_the_actual_threat_owner() {
    let mut ai = super::event_ai_dispatch_tests::state();
    let (state, host, s, a, b) = setup();
    assert_eq!(ai.selected.object(), s);
    let mut owner = EventHost { state, host };
    let call = EventAiInvocation {
        invoker: Some(a),
        sender: None,
    };
    for (kind, params) in [(13, [100, 6, 1]), (13, [10, 6, 0]), (14, [50, 0, 0])] {
        assert!(ai
            .execute_action(
                &EventAiAction {
                    kind,
                    params: Some(params)
                },
                0,
                call,
                None,
                &mut owner
            )
            .unwrap());
    }
    assert_eq!(
        owner.state.manager(s).unwrap().threat(2, false).unwrap(),
        value(165)
    );
    assert!(owner.state.hated_by(b).is_empty());
    assert_eq!(owner.host.linking, vec![(s, a)]);
    assert!(ai
        .execute_action(
            &EventAiAction {
                kind: 13,
                params: Some([1, 15, 1])
            },
            0,
            call,
            None,
            &mut owner
        )
        .unwrap());
}
#[test]
fn retained_catalog04_threat_dependencies_and_undefined_candidates_or_explicit_absence() {
    let Some(root) = std::env::var_os("BENILLA_F5_AI_CATALOG") else {
        println!("SKIP catalog-04 threat census: set BENILLA_F5_AI_CATALOG and BENILLA_F5_EVENT_AI_CONFIG");
        return;
    };
    let mut catalog =
        super::super::production_compiler::VerifiedCatalog::open(Path::new(&root), false).unwrap();
    assert_eq!(
        catalog.report()["catalog_sha256"],
        "757dcfb85e2b2a0de6571b0fe981c214fa57a4e2e546968ca61171113f379bd6"
    );
    let rows = catalog_event_ai_rows(&mut catalog).unwrap();
    assert_eq!(rows.len(), 11469);
    let config_path =
        std::env::var_os("BENILLA_F5_EVENT_AI_CONFIG").expect("recorded config required");
    let config = EventAiLoaderConfig::recorded(
        Path::new(&config_path),
        &Sha256Id::parse("321f2dc486e38f77037366bf84fb0ce8d09ed823ffa4524082ab01a8b0b79244")
            .unwrap(),
    )
    .unwrap();
    let mut loader = EventAiCatalog::new(&mut catalog, config);
    let mut loaded = 0;
    let mut threat_rows = 0;
    let mut selectors = BTreeSet::new();
    let mut zone_rows = 0;
    let mut iterator_candidates = 0;
    let mut first = BTreeMap::<String, usize>::new();
    let mut raw_rows = 0;
    for row in rows {
        raw_rows += 1;
        iterator_candidates += usize::from((0..3).any(|slot| {
            row.words[12 + 4 * slot] as u16 == 14 && (row.words[13 + 4 * slot] as i32) < -100
        }));
        match load_event_ai_row(&mut loader, row) {
            Err(error) => {
                *first.entry(error.id).or_default() += 1;
            }
            Ok(load) => match load.disposition {
                EventAiDisposition::Skipped { .. } => panic!("unexpected source skip"),
                EventAiDisposition::Loaded(event) => {
                    loaded += 1;
                    if event.actions.iter().any(|a| matches!(a.kind, 13 | 14)) {
                        threat_rows += 1;
                        selectors.insert((event.entry, event.guid));
                    }
                    zone_rows += usize::from(event.actions.iter().any(|a| a.kind == 38));
                    *first
                        .entry(
                            if event.flags & 0x400 != 0 {
                                "execution.event_ai.f5_04_combat_action_owner"
                            } else {
                                "execution.event_ai.f5_04_base_ai_owner"
                            }
                            .into(),
                        )
                        .or_default() += 1;
                }
            },
        }
    }
    assert_eq!(raw_rows, 11469);
    assert_eq!(loaded, 11126);
    assert_eq!(iterator_candidates, 0);
    assert_eq!(threat_rows, 4);
    assert_eq!(zone_rows, 18);
    let template_selectors = selectors
        .iter()
        .filter(|(entry, guid)| *entry != 0 && *guid == 0)
        .count();
    assert_eq!(template_selectors, 4);
    println!(
        "F5_THREAT_CENSUS {}",
        serde_json::json!({"raw_rows":raw_rows,"loaded_rows":loaded,"threat_action_rows":threat_rows,"positive_template_selectors":template_selectors,"selectors":selectors,"zone_combat_rows_still_unowned":zone_rows,"end_to_end_rows":0,"first_refusal":first,"undefined_eventai_erased_iterator_candidates":iterator_candidates,"empty_select_stock_callers":1,"empty_select_stock_unguarded_callers":0,"offline_null_lookup_true_callers":0,"F1_threat_projection":"unavailable; retained EventAI RNG projections do not bind threat or damage arguments","stock_large_damage_int_min_reach":"unavailable from catalog and retained damage bindings"})
    );
}

#[test]
fn hostile_percent_fade_and_online_changes_include_offline_refs() {
    let (mut state, mut host, s, a, b) = setup();
    state.register(b).unwrap();
    add(&mut state, &mut host, s, a, 10);
    add(&mut state, &mut host, b, a, 20);
    state.hostile_online(a, false, &mut host).unwrap();
    state.hostile_percent(a, -50, &mut host).unwrap();
    assert_eq!(state.manager(s).unwrap().threat(2, true).unwrap(), value(5));
    assert_eq!(
        state.manager(b).unwrap().threat(2, true).unwrap(),
        value(10)
    );
    state
        .temporary_fade(a, ThreatValue::from_i32(-7), true, &mut host)
        .unwrap();
    assert_eq!(state.manager(s).unwrap().highest(), value(0));
    assert_eq!(state.manager(b).unwrap().highest(), value(3));
    state.temporary_fade(a, value(0), false, &mut host).unwrap();
    assert_eq!(state.manager(s).unwrap().highest(), value(5));
    assert_eq!(state.manager(b).unwrap().highest(), value(10));
    state.hostile_online(a, true, &mut host).unwrap();
    state.validate(&mut host).unwrap();
}
#[test]
fn helpful_threat_captures_haters_and_splits_in_reverse_link_order() {
    let (mut state, mut host, s, a, b) = setup();
    let helper = host.unit(4, false);
    state.register(b).unwrap();
    add(&mut state, &mut host, s, a, 10);
    add(&mut state, &mut host, b, a, 20);
    let spell = HelpfulThreatSpell {
        spell: ThreatSpell {
            id: 7,
            no_threat: false,
            no_harmful_threat: false,
        },
        school: 1,
        no_initial_threat: false,
        no_helpful_threat: false,
    };
    state
        .threat_assist(a, helper, value(11), Some(spell), false, false, &mut host)
        .unwrap();
    assert_eq!(
        state.manager(s).unwrap().threat(4, false).unwrap().bits(),
        0x40b00000
    );
    assert_eq!(
        state.manager(b).unwrap().threat(4, false).unwrap().bits(),
        0x40b00000
    );
    assert_eq!(host.log, vec!["timer 4 3", "timer 4 1"]);
    state
        .threat_assist(a, helper, value(11), Some(spell), true, true, &mut host)
        .unwrap();
    assert_eq!(
        state.manager(s).unwrap().threat(4, false).unwrap().bits(),
        0x41840000
    );
    assert_eq!(
        state
            .threat_assist(a, helper, value(1), None, false, false, &mut host)
            .unwrap_err()
            .id,
        "execution.threat.assist_null_spell_undefined"
    );
}
#[test]
fn nan_only_refuses_concrete_nontransitive_sort_and_infinity_is_retained() {
    let (mut state, mut host, s, a, b) = setup();
    state
        .add_direct(
            s,
            a,
            2,
            ThreatValue::from_bits(0x7fc00000),
            false,
            &mut host,
        )
        .unwrap();
    add(&mut state, &mut host, s, b, 1);
    assert_eq!(state.select(s, &mut host).unwrap(), Some(a));
    let c = host.unit(4, false);
    add(&mut state, &mut host, s, c, 2);
    assert_eq!(
        state.select(s, &mut host).unwrap_err().id,
        "execution.threat.nan_sort_order_undefined"
    );
    state.remove(s, 2).unwrap();
    state
        .add_direct(
            s,
            b,
            3,
            ThreatValue::from_bits(0x7f800000),
            false,
            &mut host,
        )
        .unwrap();
    assert_eq!(state.select(s, &mut host).unwrap(), Some(b));
}
