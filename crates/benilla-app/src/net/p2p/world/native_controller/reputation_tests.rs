// SPDX-License-Identifier: GPL-3.0-or-later
//! Exact app controller, durable save and session initialization over authored rows.
use super::*;
use benilla_sim::quest_lifecycle as q;
use n::synthetic_reputation_fixture as rep;
struct ReputationCatalog {
    rows: std::collections::BTreeMap<u32, q::QuestRow>,
    modifier_spell: bool,
}
impl ReputationCatalog {
    fn new(profile: &n::Profile) -> Self {
        let g = profile.quest_progression.as_ref().unwrap();
        Self {
            modifier_spell: g.spell_ids().any(|id| id == rep::MODIFIER_SPELL),
            rows: g
                .quest_ids()
                .map(|id| (id, g.quest(id).unwrap().clone()))
                .collect(),
        }
    }
}
impl ClientCatalog for ReputationCatalog {
    fn build(&self) -> u32 {
        Catalog.build()
    }
    fn manifest_digest(&self) -> runtime::Digest {
        Catalog.manifest_digest()
    }
    fn contains_map(&self, id: u32) -> bool {
        Catalog.contains_map(id)
    }
    fn contains_creature_display(&self, id: u32) -> bool {
        Catalog.contains_creature_display(id)
    }
    fn contains_item_display(&self, id: u32) -> bool {
        Catalog.contains_item_display(id)
    }
    fn contains_spell(&self, id: u32) -> bool {
        Catalog.contains_spell(id) || (self.modifier_spell && id == rep::MODIFIER_SPELL)
    }
    fn player_power_type(&self, id: PlayerId) -> Option<u8> {
        Catalog.player_power_type(id)
    }
    fn player_bytes_2(&self, id: PlayerId) -> Option<u32> {
        Catalog.player_bytes_2(id)
    }
    fn item_metadata(&self, id: u32) -> Option<benilla_protocol::messages::ItemInfo> {
        Catalog.item_metadata(id)
    }
    fn quest_metadata(&self, id: u32) -> Option<benilla_world_client::QuestMetadata> {
        let row = self.rows.get(&id)?;
        let mut out = Catalog.quest_metadata(70001)?;
        out.template.quest_id = id;
        out.template.rep_objective_faction = row.excluded.rep_objective_faction;
        out.template.rep_objective_value = row.excluded.rep_objective_value as u32;
        out.template.money_max_level = row.reward.xp.reward_money_max_level;
        out.template.money = row.reward.reward_or_required_money;
        for (i, o) in out.template.objectives.iter_mut().enumerate() {
            o.creature_or_go = row.required_creature[i] as u32;
            o.required_count = row.required_creature_count[i];
        }
        Some(out)
    }
    fn quest_greeting(&self, id: u32) -> Option<String> {
        Catalog.quest_greeting(id)
    }
    fn native_quest_greeting(&self, id: u32) -> Option<benilla_world_client::NativeQuestGreeting> {
        Catalog.native_quest_greeting(id)
    }
    fn native_quest_presentation(
        &self,
        _: u32,
    ) -> Option<benilla_world_client::NativeQuestPresentation> {
        Catalog.native_quest_presentation(70001)
    }
}
struct FailOnce {
    fail: bool,
    inner: quests::QuestAuthority,
}
impl n::Authority for FailOnce {
    fn resolve(
        &mut self,
        request: &n::AuthorityRequest,
    ) -> Result<n::AuthorityReply, n::NativeError> {
        if self.fail {
            self.fail = false;
            return Err(n::NativeError::AuthorityUnavailable(
                "authored first tick failure",
            ));
        }
        self.inner.resolve(request)
    }
}
fn enter(h: &Harness, open: bool, fail: bool, modifier: bool) -> NativeController {
    let profile = if modifier {
        rep::profile_with_modifier(10)
    } else {
        rep::profile()
    };
    let catalog = ReputationCatalog::new(&profile);
    NativeController::enter_component_test(
        &h.root,
        &h.profile,
        &h.character,
        if open {
            Entry::Open { world: [8; 16] }
        } else {
            Entry::Create {
                world: [8; 16],
                seed: 7,
            }
        },
        profile,
        &catalog,
        FailOnce {
            fail,
            inner: quests::QuestAuthority::default(),
        },
    )
    .unwrap()
}
fn accept(c: &mut NativeController, quest: u32) {
    let npc = c.projection.npc_guid(c.world.state().npcs[&3].id).unwrap();
    c.receive(ClientCommand::QuestgiverAccept { npc, quest })
        .unwrap();
}
fn reward(c: &mut NativeController, quest: u32) {
    let npc = c.projection.npc_guid(c.world.state().npcs[&3].id).unwrap();
    c.receive(ClientCommand::QuestgiverChooseReward {
        npc,
        quest,
        choice: 0,
    })
    .unwrap();
}
fn reputation(c: &NativeController) -> i32 {
    c.world
        .state()
        .quest_progression
        .as_ref()
        .unwrap()
        .reputation(
            c.world.profile().quest_progression.as_ref().unwrap(),
            rep::SPILLOVER,
        )
        .unwrap()
}
#[test]
fn initial_reputation_ack_waits_for_natural_tick_and_survives_authority_save_failure() {
    check_initial_reputation_ack(false);
    check_initial_reputation_ack(true);
}
fn check_initial_reputation_ack(modifier: bool) {
    let gained = if modifier { 240 } else { 200 };
    let h = Harness::new();
    let mut c = enter(&h, false, false, modifier);
    assert_eq!(c.tick(), 0);
    assert_eq!(
        c.initial_events()
            .unwrap()
            .iter()
            .filter(|e| matches!(e, SessionEvent::Reputations { .. }))
            .count(),
        1
    );
    accept(&mut c, rep::OBJECTIVE);
    accept(&mut c, rep::OTHER_OBJECTIVE);
    let out = c.pump(c.tick_duration());
    assert!(out.pause.is_none());
    assert!(c.world.reputation_initial_ack_needed());
    let digest = c.world.state_digest();
    let now = c.world.state().now_ms;
    let tick = c.tick();
    drop(c);
    for _ in 0..8 {
        let mut c = enter(&h, true, false, modifier);
        assert_eq!(c.tick(), tick);
        assert_eq!(c.world.state().now_ms, now);
        assert_eq!(c.world.state_digest(), digest);
        assert!(c.initial_reputation_ack);
        assert_eq!(
            c.initial_events()
                .unwrap()
                .iter()
                .filter(|e| matches!(e, SessionEvent::Reputations { .. }))
                .count(),
            1
        );
        assert_eq!(c.pump(Duration::ZERO).ticks, 0);
    }
    let mut c = enter(&h, true, true, modifier);
    c.initial_events().unwrap();
    accept(&mut c, rep::GRANT);
    reward(&mut c, rep::GRANT);
    let failed = c.pump(c.tick_duration());
    assert!(matches!(failed.pause.unwrap().kind, PauseKind::Authority));
    assert!(failed.events.is_empty());
    assert_eq!(c.world.state_digest(), digest);
    assert_eq!(c.tick(), tick);
    let inputs = &c.pending.as_ref().unwrap().inputs;
    assert_eq!(inputs.len(), 3);
    assert!(matches!(
        inputs[0].input.command,
        n::Command::AcknowledgeInitialReputations
    ));
    assert!(matches!(
        inputs[1].input.command,
        n::Command::AcceptQuest {
            quest: rep::GRANT,
            ..
        }
    ));
    assert!(matches!(
        inputs[2].input.command,
        n::Command::ChooseQuestReward {
            quest: rep::GRANT,
            ..
        }
    ));
    c.retry_tick().unwrap();
    fs::write(
        h.path().join(".world-checkpoint.pending"),
        b"authored save failure",
    )
    .unwrap();
    let failed = c.pump(Duration::ZERO);
    assert!(matches!(
        failed.pause.unwrap().kind,
        PauseKind::Checkpoint {
            requires_reopen: false
        }
    ));
    assert!(failed.events.is_empty());
    assert_eq!(reputation(&c), gained);
    assert!(!c.world.reputation_initial_ack_needed());
    assert_eq!(c.tick(), tick + 1);
    assert_eq!(
        c.world.state().now_ms,
        now + u64::from(c.world.profile().tick_ms)
    );
    let committed = c.world.state_digest();
    fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
    c.retry_checkpoint().unwrap();
    let out = c.pump(Duration::ZERO);
    assert_eq!(out.ticks, 0);
    assert_eq!(
        out.events
            .iter()
            .filter(|e| matches!(e, SessionEvent::ReputationDelta { .. }))
            .count(),
        3
    );
    assert!(!out
        .events
        .iter()
        .any(|e| matches!(e, SessionEvent::Reputations { .. })));
    assert!(out.events.iter().all(|e| !matches!(e, SessionEvent::ReputationDelta { standings } if standings.iter().any(|(index,_)| *index == 11))));
    let rep_index = out
        .events
        .iter()
        .rposition(|e| matches!(e, SessionEvent::ReputationDelta { .. }))
        .unwrap();
    let xp_index = out
        .events
        .iter()
        .position(|e| matches!(e, SessionEvent::XpGain(_)))
        .unwrap();
    assert!(rep_index < xp_index);
    assert!(c.pump(Duration::ZERO).events.is_empty());
    assert_eq!(c.world.state_digest(), committed);
    drop(c);
    let mut c = enter(&h, true, false, modifier);
    assert_eq!(c.world.state_digest(), committed);
    assert!(!c.initial_reputation_ack);
    let initial = c.initial_events().unwrap();
    assert!(initial
        .iter()
        .any(|e| matches!(e,SessionEvent::Reputations{standings} if standings[7]==(1,gained))));
    assert!(!initial.iter().any(|e| matches!(
        e,
        SessionEvent::ReputationDelta { .. } | SessionEvent::XpGain(_)
    )));
    reward(&mut c, rep::GRANT);
    let duplicate = c.pump(c.tick_duration());
    assert!(duplicate.pause.is_none());
    assert!(!duplicate.events.iter().any(|e| matches!(
        e,
        SessionEvent::ReputationDelta { .. } | SessionEvent::XpGain(_)
    )));
    assert_eq!(reputation(&c), gained);
}
#[test]
fn unacknowledged_and_failed_reward_reopen_keeps_last_durable_reputation() {
    check_failed_reputation_reopen(false);
    check_failed_reputation_reopen(true);
}
fn check_failed_reputation_reopen(modifier: bool) {
    let gained = if modifier { 240 } else { 200 };
    let h = Harness::new();
    let mut c = enter(&h, false, false, modifier);
    c.initial_events().unwrap();
    accept(&mut c, rep::OBJECTIVE);
    accept(&mut c, rep::OTHER_OBJECTIVE);
    assert!(c.pump(c.tick_duration()).pause.is_none());
    let digest = c.world.state_digest();
    drop(c);
    let mut c = enter(&h, true, false, modifier);
    c.initial_events().unwrap();
    accept(&mut c, rep::GRANT);
    reward(&mut c, rep::GRANT);
    fs::write(
        h.path().join(".world-checkpoint.pending"),
        b"authored save failure",
    )
    .unwrap();
    assert!(c.pump(c.tick_duration()).events.is_empty());
    assert_eq!(reputation(&c), gained);
    drop(c);
    fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
    let mut c = enter(&h, true, false, modifier);
    assert_eq!(c.world.state_digest(), digest);
    assert_eq!(reputation(&c), 0);
    assert!(c.initial_reputation_ack);
    c.initial_events().unwrap();
    accept(&mut c, rep::GRANT);
    reward(&mut c, rep::GRANT);
    let out = c.pump(c.tick_duration());
    assert!(out.pause.is_none());
    assert_eq!(
        out.events
            .iter()
            .filter(|e| matches!(e, SessionEvent::ReputationDelta { .. }))
            .count(),
        3
    );
    assert_eq!(reputation(&c), gained);
}

#[test]
fn actual_lua_reputation_controls_wait_for_save_then_reopen_without_invented_ack() {
    use benilla_sim::reputation as r;
    use benilla_ui::script::{FactionEntry, ReputationSend, ReputationState, UiScript};
    let h = Harness::new();
    let mut c = enter(&h, false, false, false);
    c.initial_events().unwrap();
    accept(&mut c, rep::OBJECTIVE);
    let out = c.pump(c.tick_duration());
    assert!(out.pause.is_none());
    let flags = c
        .world
        .state()
        .quest_progression
        .as_ref()
        .unwrap()
        .reputation
        .as_ref()
        .unwrap()
        .view()
        .factions()[&7]
        .flags;
    assert_ne!(flags & r::VISIBLE, 0);
    assert_eq!(flags & (r::AT_WAR | r::INACTIVE), 0);
    let mut ui = UiScript::new().unwrap();
    ui.set_reputation(ReputationState {
        entries: vec![FactionEntry {
            faction_id: rep::SPILLOVER,
            rep_list_id: 7,
            parent_id: 0,
            name: "Authored visible faction".into(),
            description: String::new(),
            standing: 0,
            standing_id: 4,
            bar_min: 0,
            bar_max: 3000,
            visible: flags & r::VISIBLE != 0,
            is_header: false,
            at_war: flags & r::AT_WAR != 0,
            can_toggle_at_war: true,
            inactive: flags & r::INACTIVE != 0,
        }],
        watched: None,
    });
    ui.run("for i=1,GetNumFactions() do local name=GetFactionInfo(i); if name=='Authored visible faction' then FactionToggleAtWar(i); SetFactionInactive(i); break end end").unwrap();
    let sends = ui.take_reputation_sends();
    assert_eq!(
        sends,
        vec![
            ReputationSend::AtWar {
                rep_list_id: 7,
                at_war: true
            },
            ReputationSend::Inactive {
                rep_list_id: 7,
                inactive: true
            }
        ]
    );
    for send in sends {
        c.receive(match send {
            ReputationSend::AtWar {
                rep_list_id,
                at_war,
            } => ClientCommand::SetFactionAtWar {
                rep_list_id,
                at_war,
            },
            ReputationSend::Inactive {
                rep_list_id,
                inactive,
            } => ClientCommand::SetFactionInactive {
                rep_list_id,
                inactive,
            },
            ReputationSend::Watch(_) => unreachable!(),
        })
        .unwrap();
    }
    let sequence = c.world.state().next_sequence;
    fs::write(
        h.path().join(".world-checkpoint.pending"),
        b"authored save failure",
    )
    .unwrap();
    let failed = c.pump(c.tick_duration());
    assert!(matches!(
        failed.pause.unwrap().kind,
        PauseKind::Checkpoint {
            requires_reopen: false
        }
    ));
    assert!(failed.events.is_empty());
    let state = c
        .world
        .state()
        .quest_progression
        .as_ref()
        .unwrap()
        .reputation
        .as_ref()
        .unwrap()
        .view();
    assert_eq!(state.factions()[&7].flags, flags | r::AT_WAR | r::INACTIVE);
    assert!(state.factions()[&7].need_send && state.factions()[&7].need_save);
    assert_eq!(c.world.state().next_sequence, sequence + 2);
    let digest = c.world.state_digest();
    fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
    c.retry_checkpoint().unwrap();
    let flushed = c.pump(Duration::ZERO);
    assert!(flushed.pause.is_none());
    assert!(!flushed.events.iter().any(|e| matches!(
        e,
        SessionEvent::Reputations { .. } | SessionEvent::ReputationDelta { .. }
    )));
    assert!(c.pump(Duration::ZERO).events.is_empty());
    assert_eq!(c.world.state_digest(), digest);
    assert!(
        !c.initial_reputation_ack,
        "no synthetic bootstrap ack after toggle"
    );
    drop(c);
    let mut c = enter(&h, true, false, false);
    assert_eq!(c.world.state_digest(), digest);
    let events = c.initial_events().unwrap();
    assert!(events.iter().any(|e| matches!(e, SessionEvent::Reputations { standings } if standings[7].0 as u32 == (flags | r::AT_WAR | r::INACTIVE))));
    assert!(c.initial_reputation_ack);
    c.receive(ClientCommand::SetFactionAtWar {
        rep_list_id: 7,
        at_war: false,
    })
    .unwrap();
    c.receive(ClientCommand::SetFactionInactive {
        rep_list_id: 7,
        inactive: false,
    })
    .unwrap();
    let out = c.pump(c.tick_duration());
    assert!(out.pause.is_none());
    assert!(!out.events.iter().any(|e| matches!(
        e,
        SessionEvent::Reputations { .. } | SessionEvent::ReputationDelta { .. }
    )));
    assert_eq!(
        c.world
            .state()
            .quest_progression
            .as_ref()
            .unwrap()
            .reputation
            .as_ref()
            .unwrap()
            .view()
            .factions()[&7]
            .flags,
        flags
    );
    let digest = c.world.state_digest();
    drop(c);
    let c = enter(&h, true, false, false);
    assert_eq!(c.world.state_digest(), digest);
}

#[test]
fn actual_lua_watched_faction_waits_for_descriptor_commit_and_retains_raw_signed_values() {
    use benilla_ui::script::{FactionEntry, ReputationSend, ReputationState, UiScript};
    let h = Harness::new();
    let mut c = enter(&h, false, false, false);
    let initial = c.initial_events().unwrap();
    let field = |events: &[SessionEvent]| {
        events.iter().find_map(|event| match event {
            SessionEvent::ObjectValues { fields, .. } => fields.player_watched_faction(),
            _ => None,
        })
    };
    assert_eq!(field(&initial), Some(-1));
    accept(&mut c, rep::OBJECTIVE);
    assert!(c.pump(c.tick_duration()).pause.is_none());
    let entry = FactionEntry {
        faction_id: rep::SPILLOVER,
        rep_list_id: 7,
        parent_id: 0,
        name: "Authored watched faction".into(),
        description: String::new(),
        standing: 0,
        standing_id: 4,
        bar_min: 0,
        bar_max: 3000,
        visible: true,
        is_header: false,
        at_war: false,
        can_toggle_at_war: true,
        inactive: false,
    };
    let mut ui = UiScript::new().unwrap();
    ui.set_reputation(ReputationState {
        entries: vec![entry.clone()],
        watched: None,
    });
    ui.run("for i=1,GetNumFactions() do local name=GetFactionInfo(i); if name=='Authored watched faction' then SetWatchedFactionIndex(i); break end end").unwrap();
    assert!(
        ui.eval::<bool>("return GetWatchedFactionInfo()==nil")
            .unwrap(),
        "stock UI waits for server descriptor, no local watch mirror"
    );
    assert_eq!(
        ui.take_reputation_sends(),
        vec![ReputationSend::Watch(Some(7))]
    );
    c.receive(ClientCommand::SetWatchedFaction { rep_list_id: 7 })
        .unwrap();
    let reputation_before = c
        .world
        .state()
        .quest_progression
        .as_ref()
        .unwrap()
        .reputation
        .as_ref()
        .unwrap()
        .clone();
    fs::write(
        h.path().join(".world-checkpoint.pending"),
        b"authored save failure",
    )
    .unwrap();
    let failed = c.pump(c.tick_duration());
    assert!(matches!(
        failed.pause.unwrap().kind,
        PauseKind::Checkpoint {
            requires_reopen: false
        }
    ));
    assert!(failed.events.is_empty());
    assert_eq!(c.world.state().watched_faction(), 7);
    assert_eq!(
        c.world
            .state()
            .quest_progression
            .as_ref()
            .unwrap()
            .reputation
            .as_ref()
            .unwrap(),
        &reputation_before
    );
    let sequence = c.world.state().next_sequence;
    fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
    c.retry_checkpoint().unwrap();
    let out = c.pump(Duration::ZERO);
    assert!(out.pause.is_none());
    let watched = field(&out.events).unwrap();
    assert_eq!(watched, 7);
    ui.set_reputation(ReputationState {
        entries: vec![entry.clone()],
        watched: u32::try_from(watched).ok(),
    });
    assert_eq!(
        ui.eval::<String>("return GetWatchedFactionInfo()").unwrap(),
        entry.name
    );
    assert!(c.pump(Duration::ZERO).events.is_empty());
    assert_eq!(c.world.state().next_sequence, sequence);
    let digest = c.world.state_digest();
    drop(c);
    let mut c = enter(&h, true, false, false);
    assert_eq!(c.world.state_digest(), digest);
    assert_eq!(field(&c.initial_events().unwrap()), Some(7));
    ui.run("SetWatchedFactionIndex(0)").unwrap();
    assert_eq!(
        ui.take_reputation_sends(),
        vec![ReputationSend::Watch(None)]
    );
    c.receive(ClientCommand::SetWatchedFaction { rep_list_id: -1 })
        .unwrap();
    let out = c.pump(c.tick_duration());
    assert!(out.pause.is_none());
    assert_eq!(field(&out.events), Some(-1));
    for raw in [0, -2, i32::MIN, i32::MAX, 64, 123456] {
        c.receive(ClientCommand::SetWatchedFaction { rep_list_id: raw })
            .unwrap();
        let out = c.pump(c.tick_duration());
        assert!(out.pause.is_none());
        assert_eq!(field(&out.events), Some(raw));
        assert_eq!(c.world.state().watched_faction(), raw);
        let digest = c.world.state_digest();
        drop(c);
        c = enter(&h, true, false, false);
        assert_eq!(c.world.state_digest(), digest);
        assert_eq!(field(&c.initial_events().unwrap()), Some(raw));
    }
}

#[test]
fn actual_lua_action_bar_toggles_wait_for_saved_descriptor_and_reopen_raw_byte() {
    use benilla_ui::script::UiScript;
    let h = Harness::new();
    let mut c = enter(&h, false, false, false);
    assert!(c.world.profile().supports_action_bar_toggles());
    let field = |events: &[SessionEvent]| {
        events.iter().find_map(|event| match event {
            SessionEvent::ObjectValues { fields, .. } => fields.player_action_bar_toggles(),
            _ => None,
        })
    };
    let initial = c.initial_events().unwrap();
    assert_eq!(field(&initial), Some(0));
    let mut ui = UiScript::new().unwrap();
    ui.set_action_bar_toggles(field(&initial).unwrap());
    ui.run("SetActionBarToggles(1, nil, 1, nil)").unwrap();
    assert_eq!(ui.take_action_bar_toggle_sends(), vec![5]);
    assert_eq!(
        ui.action_bar_toggles(),
        Some(0),
        "Lua setter does not invent a local descriptor echo"
    );
    c.receive(ClientCommand::SetActionBarToggles { toggles: 5 })
        .unwrap();
    fs::write(
        h.path().join(".world-checkpoint.pending"),
        b"authored save failure",
    )
    .unwrap();
    let failed = c.pump(c.tick_duration());
    assert!(matches!(
        failed.pause.unwrap().kind,
        PauseKind::Checkpoint {
            requires_reopen: false
        }
    ));
    assert!(failed.events.is_empty());
    assert_eq!(c.world.state().action_bar_toggles(), 5);
    let sequence = c.world.state().next_sequence;
    fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
    c.retry_checkpoint().unwrap();
    let out = c.pump(Duration::ZERO);
    assert!(out.pause.is_none());
    assert_eq!(field(&out.events), Some(5));
    ui.set_action_bar_toggles(field(&out.events).unwrap());
    assert!(ui
        .eval::<bool>(
            "local a,b,c,d=GetActionBarToggles(); return a==1 and b==nil and c==1 and d==nil"
        )
        .unwrap());
    assert!(c.pump(Duration::ZERO).events.is_empty());
    assert_eq!(c.world.state().next_sequence, sequence);
    let digest = c.world.state_digest();
    drop(c);
    let mut c = enter(&h, true, false, false);
    assert_eq!(c.world.state_digest(), digest);
    assert_eq!(field(&c.initial_events().unwrap()), Some(5));
    for raw in [0xff, 0xf5, 0x80, 0] {
        c.receive(ClientCommand::SetActionBarToggles { toggles: raw })
            .unwrap();
        let out = c.pump(c.tick_duration());
        assert!(out.pause.is_none());
        assert_eq!(field(&out.events), Some(raw));
        assert_eq!(c.world.state().action_bar_toggles(), raw);
        let digest = c.world.state_digest();
        drop(c);
        c = enter(&h, true, false, false);
        assert_eq!(c.world.state_digest(), digest);
        assert_eq!(field(&c.initial_events().unwrap()), Some(raw));
    }
}
