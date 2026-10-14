// SPDX-License-Identifier: GPL-3.0-or-later
//! Exact app controller, durable save and session initialization over authored rows.
use super::*;
use benilla_sim::quest_lifecycle as q;
use n::synthetic_reputation_fixture as rep;
struct ReputationCatalog {
    rows: std::collections::BTreeMap<u32, q::QuestRow>,
}
impl ReputationCatalog {
    fn new(profile: &n::Profile) -> Self {
        let g = profile.quest_progression.as_ref().unwrap();
        Self {
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
        Catalog.contains_spell(id)
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
fn enter(h: &Harness, open: bool, fail: bool) -> NativeController {
    let profile = rep::profile();
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
    let h = Harness::new();
    let mut c = enter(&h, false, false);
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
        let mut c = enter(&h, true, false);
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
    let mut c = enter(&h, true, true);
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
    assert_eq!(reputation(&c), 200);
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
    let mut c = enter(&h, true, false);
    assert_eq!(c.world.state_digest(), committed);
    assert!(!c.initial_reputation_ack);
    let initial = c.initial_events().unwrap();
    assert!(initial
        .iter()
        .any(|e| matches!(e,SessionEvent::Reputations{standings} if standings[7]==(1,200))));
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
    assert_eq!(reputation(&c), 200);
}
#[test]
fn unacknowledged_and_failed_reward_reopen_keeps_last_durable_reputation() {
    let h = Harness::new();
    let mut c = enter(&h, false, false);
    c.initial_events().unwrap();
    accept(&mut c, rep::OBJECTIVE);
    accept(&mut c, rep::OTHER_OBJECTIVE);
    assert!(c.pump(c.tick_duration()).pause.is_none());
    let digest = c.world.state_digest();
    drop(c);
    let mut c = enter(&h, true, false);
    c.initial_events().unwrap();
    accept(&mut c, rep::GRANT);
    reward(&mut c, rep::GRANT);
    fs::write(
        h.path().join(".world-checkpoint.pending"),
        b"authored save failure",
    )
    .unwrap();
    assert!(c.pump(c.tick_duration()).events.is_empty());
    assert_eq!(reputation(&c), 200);
    drop(c);
    fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
    let mut c = enter(&h, true, false);
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
    assert_eq!(reputation(&c), 200);
}
