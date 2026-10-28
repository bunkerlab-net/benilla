// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use benilla_sim::quest_lifecycle as q;
fn abandon(c: &mut NativeController, slot: u8) -> Pump {
    c.receive(ClientCommand::QuestlogRemove { slot }).unwrap();
    let out = c.pump(c.tick_duration());
    assert!(out.pause.is_none(), "{:?}", out.pause);
    out
}
#[test]
fn actual_lua_abandon_waits_for_checkpoint_then_reopens_and_reoffers_reset_quest() {
    use benilla_ui::script::{QuestLogEntryView, QuestLogState, UiScript};
    let h = Harness::new();
    let mut c = ready(&h);
    let prior = progress(&c).quests.progress(70001).unwrap().clone();
    assert_eq!(prior.status, q::Status::Complete);
    let mut ui = UiScript::new().unwrap();
    ui.set_quest_log(QuestLogState {
        num_quests: 1,
        entries: vec![QuestLogEntryView {
            quest_id: 70001,
            title: "Authored quest".into(),
            level: 1,
            complete: 1,
            ..Default::default()
        }],
        ..Default::default()
    });
    ui.run("SelectQuestLogEntry(1); SetAbandonQuest(); AbandonQuest()")
        .unwrap();
    let ids = ui.take_quest_log_abandons();
    assert_eq!(ids, vec![70001]);
    let slot = progress(&c)
        .quests
        .view()
        .log
        .iter()
        .position(|id| *id == Some(ids[0]))
        .unwrap() as u8;
    c.receive(ClientCommand::QuestlogRemove { slot }).unwrap();
    inject(&h);
    let out = c.pump(c.tick_duration());
    paused(&out);
    let mut expected = prior;
    expected.status = q::Status::None;
    assert_eq!(progress(&c).quests.progress(70001), Some(&expected));
    assert_eq!(progress(&c).quests.view().log[0], None);
    let staged = format!("{:?}", c.outbox.as_ref().unwrap().events);
    assert_eq!(
        c.world
            .last_tick()
            .unwrap()
            .events
            .iter()
            .filter(|e| matches!(
                e,
                n::Event::QuestProgression(n::QuestProgressionEvent::Quest {
                    event: q::Event::LogSet {
                        slot: 0,
                        quest: None
                    },
                    ..
                })
            ))
            .count(),
        1
    );
    let digest = c.world.state_digest();
    let sequence = c.world.state().next_sequence;
    assert!(c.retry_checkpoint().is_err());
    repair(&h);
    c.retry_checkpoint().unwrap();
    let out = c.pump(Duration::ZERO);
    no_rewards(&out.events);
    // Existing semantic projection also sends a consolidated player field view;
    // the durable outbox is flushed exactly once, not one invented wire packet.
    assert_eq!(format!("{:?}", out.events), staged);
    assert!(out.events.iter().any(|e|matches!(e,SessionEvent::ObjectValues{guid,fields} if *guid==c.player_guid()&&fields.player_quest_log(0).is_some_and(|s|s.quest_id==0&&s.state==0&&s.counters==[0;4]&&s.timer==0))));
    assert!(c.pump(Duration::ZERO).events.is_empty());
    assert_eq!(c.world.state().next_sequence, sequence);
    drop(c);
    let mut c = enter(&h, true);
    assert_eq!(c.world.state_digest(), digest);
    assert_eq!(progress(&c).quests.progress(70001), Some(&expected));
    let npc = giver(&c);
    assert!(matches!(
        &c.receive(ClientCommand::QuestgiverStatusQuery { npc })
            .unwrap()
            .events[..],
        [SessionEvent::QuestGiverStatus { status: 5, .. }]
    ));
    assert!(matches!(
        &c.receive(ClientCommand::QuestgiverHello { npc })
            .unwrap()
            .events[..],
        [SessionEvent::QuestGreeting(_)]
    ));
    assert!(accept(&mut c).pause.is_none());
    let p = progress(&c).quests.progress(70001).unwrap();
    assert_eq!(p.status, q::Status::Incomplete);
    assert_eq!(p.creatures, [0; 4]);
    assert!(!p.rewarded);
    let digest = c.world.state_digest();
    drop(c);
    assert_eq!(enter(&h, true).world.state_digest(), digest);
}
#[test]
fn invalid_empty_and_duplicate_abandons_preserve_rewarded_history() {
    let h = Harness::new();
    let mut c = ready(&h);
    assert!(reward(&mut c).pause.is_none());
    let prior = progress(&c).quests.progress(70001).unwrap().clone();
    assert!(prior.rewarded);
    let before = c.world.state_digest();
    let sequence = c.world.state().next_sequence;
    for slot in [20, 25, 255] {
        assert!(c
            .receive(ClientCommand::QuestlogRemove { slot })
            .unwrap()
            .events
            .is_empty());
    }
    assert!(c.actions.is_empty());
    assert_eq!(c.world.state_digest(), before);
    assert_eq!(c.world.state().next_sequence, sequence);
    for slot in [0, 0, 19] {
        let out = abandon(&mut c, slot);
        no_rewards(&out.events);
        assert_eq!(progress(&c).quests.progress(70001), Some(&prior));
    }
    let digest = c.world.state_digest();
    drop(c);
    assert_eq!(enter(&h, true).world.state_digest(), digest);
}
#[test]
fn incomplete_quest_abandon_has_no_alive_guard_after_real_death() {
    use benilla_sim::player_death as d;
    let h = Harness::new();
    let mut data = quests::profile();
    data.npcs[0].max_health = 1000;
    let npc = data.npcs[0].combat.as_mut().unwrap();
    npc.minimum_damage = fixture::float(10000.);
    npc.maximum_damage = fixture::float(10000.);
    data.configure_player_death(
        d::DeathConfig {
            map_instanceable: false,
            pve_reclaim_delay_enabled: true,
        },
        d::DeathSupport::checked(&d::REQUIRED_ABSENCES.map(|x| (x, Some(false)))).unwrap(),
    )
    .unwrap();
    let mut c = h
        .enter(
            Entry::Create {
                world: [8; 16],
                seed: 7,
            },
            data,
            quests::QuestAuthority::default(),
        )
        .unwrap();
    assert!(accept(&mut c).pause.is_none());
    c.receive(cast(&c)).unwrap();
    for _ in 0..200 {
        let out = c.pump(c.tick_duration());
        assert!(out.pause.is_none());
        if !c.world.state().player_alive() {
            break;
        }
    }
    assert!(!c.world.state().player_alive());
    let out = abandon(&mut c, 0);
    no_rewards(&out.events);
    assert_eq!(
        progress(&c).quests.progress(70001).unwrap().status,
        q::Status::None
    );
    assert_eq!(c.world.state().health, 0);
}
