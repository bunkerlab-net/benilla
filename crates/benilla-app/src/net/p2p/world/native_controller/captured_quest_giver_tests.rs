// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored short-chain adapters through normal commands and durable controller reopening.
use super::*;
use benilla_world_client::captured_quests::*;
use n::synthetic_quest_giver_fixture as captured_fixture;

struct CapturedCatalog {
    base: LoopCatalog,
    quests: CompiledQuestPresentation,
}
fn presentation(gameplay: &q::CompiledOrdinaryQuests, map: u32) -> CapturedQuestPresentation {
    CapturedQuestPresentation {
        source: gameplay.source(),
        quests: Some(
            gameplay
                .quests()
                .iter()
                .map(|row| CapturedQuestText {
                    quest: row.id,
                    zone_or_sort: 9,
                    point_map: map,
                    point_x_bits: 0,
                    point_y_bits: 0,
                    point_option: 0,
                    text: QuestText {
                        title: format!("Original short-chain quest {}", row.id),
                        details: "Original detail $N$B".into(),
                        objectives: "Original objective".into(),
                        end: String::new(),
                        request: "Original request".into(),
                        offer: "Original offer".into(),
                        objective: std::array::from_fn(|_| String::new()),
                    },
                    presentation: benilla_world_client::NativeQuestPresentation {
                        details_emotes: [(0, 0); 4],
                        offer_emotes: [(0, 0); 4],
                        complete_emote: 1,
                        complete_emote_delay: 0,
                        incomplete_emote: 0,
                        incomplete_emote_delay: 0,
                    },
                })
                .collect(),
        ),
        quest_locales: Some(vec![]),
        greetings: Some(vec![CapturedGreeting {
            entry: 823,
            kind: 0,
            text: "Original giver greeting $N$B".into(),
            emote: 1,
            delay: 0,
        }]),
        greeting_locales: Some(vec![]),
        spawns: Some(
            gameplay
                .givers()
                .iter()
                .map(|g| CapturedGreetingSpawn {
                    spawn: g.spawn,
                    entry: g.entry,
                    generation: g.generation,
                    gender: 0,
                    npc_gossip: Some(None),
                })
                .collect(),
        ),
        npc_text_keys: vec![0xffffff],
        npc_text: Some(vec![]),
        npc_text_locales: Some(vec![]),
        broadcasts: Some(vec![]),
        broadcast_locales: Some(vec![]),
        emotes: Some(vec![0, 1]),
        maps: Some(vec![map]),
        items: Some(vec![]),
        locale_mapping: vec![(-1, n::EventAiLocale::EnUs)],
    }
}
impl CapturedCatalog {
    fn new(profile: &n::Profile, gameplay: &q::CompiledOrdinaryQuests) -> Self {
        Self {
            base: LoopCatalog::new(&loop_fixture::profile()),
            quests: presentation(gameplay, profile.map_id)
                .compile(gameplay, -1)
                .unwrap(),
        }
    }
}
impl ClientCatalog for CapturedCatalog {
    fn build(&self) -> u32 {
        self.base.build()
    }
    fn manifest_digest(&self) -> runtime::Digest {
        self.quests.manifest_digest()
    }
    fn contains_map(&self, id: u32) -> bool {
        self.base.contains_map(id)
    }
    fn contains_creature_display(&self, id: u32) -> bool {
        self.base.contains_creature_display(id)
    }
    fn contains_item_display(&self, id: u32) -> bool {
        self.base.contains_item_display(id)
    }
    fn contains_spell(&self, id: u32) -> bool {
        self.base.contains_spell(id)
    }
    fn player_power_type(&self, id: PlayerId) -> Option<u8> {
        self.base.player_power_type(id)
    }
    fn player_bytes_2(&self, id: PlayerId) -> Option<u32> {
        self.base.player_bytes_2(id)
    }
    fn item_metadata(&self, id: u32) -> Option<benilla_protocol::messages::ItemInfo> {
        self.base.item_metadata(id)
    }
    fn quest_metadata(&self, id: u32) -> Option<benilla_world_client::QuestMetadata> {
        self.quests.quest_metadata(id).cloned()
    }
    fn quest_greeting(&self, id: u32) -> Option<String> {
        self.quests
            .native_quest_greeting(id)
            .map(|row| row.text.clone())
    }
    fn native_quest_greeting(&self, id: u32) -> Option<benilla_world_client::NativeQuestGreeting> {
        self.quests.native_quest_greeting(id).cloned()
    }
    fn native_quest_presentation(
        &self,
        id: u32,
    ) -> Option<benilla_world_client::NativeQuestPresentation> {
        self.quests.native_quest_presentation(id).cloned()
    }
}
fn enter_captured(
    h: &Harness,
    data: &n::Profile,
    catalog: &CapturedCatalog,
    open: bool,
) -> NativeController {
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
        data.clone(),
        catalog,
        loop_fixture::CompleteAuthority::default(),
    )
    .unwrap()
}
fn reopen_captured(
    h: &Harness,
    c: NativeController,
    data: &n::Profile,
    catalog: &CapturedCatalog,
) -> NativeController {
    let checkpoint = c.world.checkpoint().unwrap();
    let state = c.world.state().clone();
    drop(c);
    let mut reopened = enter_captured(h, data, catalog, true);
    assert_eq!(reopened.world.checkpoint().unwrap(), checkpoint);
    assert_eq!(reopened.world.state(), &state);
    no_transient_progress(&reopened.initial_events().unwrap());
    reopened
}
fn accept_id(c: &mut NativeController, spawn: u64, quest: u32) -> Pump {
    c.receive(ClientCommand::QuestgiverAccept {
        npc: guid(c, spawn),
        quest,
    })
    .unwrap();
    step(c)
}
fn reward_id(c: &mut NativeController, quest: u32) -> Pump {
    c.receive(ClientCommand::QuestgiverChooseReward {
        npc: guid(c, 3),
        quest,
        choice: 0,
    })
    .unwrap();
    step(c)
}
#[test]
fn captured_introduction_unlocks_kill_quest_across_controller_reopen_and_reward_retry() {
    let (data, gameplay) = captured_fixture::profile();
    let catalog = CapturedCatalog::new(&data, &gameplay);
    let h = Harness::new();
    let mut c = enter_captured(&h, &data, &catalog, false);
    let mcbride = guid(&c, 3);
    let willem = guid(&c, 4);
    assert!(matches!(
        &c.receive(ClientCommand::QuestgiverStatusQuery { npc: willem })
            .unwrap()
            .events[..],
        [SessionEvent::QuestGiverStatus { status: 5, .. }]
    ));
    assert!(matches!(
        &c.receive(ClientCommand::QuestgiverStatusQuery { npc: mcbride })
            .unwrap()
            .events[..],
        [SessionEvent::QuestGiverStatus { status: 0, .. }]
    ));
    assert!(c
        .receive(ClientCommand::QuestgiverQuery {
            npc: mcbride,
            quest: 7
        })
        .is_err());
    let greeting = c
        .receive(ClientCommand::QuestgiverHello { npc: willem })
        .unwrap();
    assert!(
        matches!(&greeting.events[..], [SessionEvent::QuestGreeting(list)] if list.quests.len() == 1 && list.quests[0].quest_id == 783)
    );
    assert!(
        matches!(&c.receive(ClientCommand::QuestQuery { quest: 783 }).unwrap().events[..], [SessionEvent::QuestTemplate(row)] if row.details == "Original detail $N$B")
    );
    assert!(accept_id(&mut c, 4, 783).pause.is_none());
    c = reopen_captured(&h, c, &data, &catalog);
    assert!(matches!(
        &c.receive(ClientCommand::QuestgiverComplete {
            npc: mcbride,
            quest: 783
        })
        .unwrap()
        .events[..],
        [SessionEvent::QuestOffer(_)]
    ));
    assert!(c
        .receive(ClientCommand::QuestgiverRequestReward {
            npc: mcbride,
            quest: 783
        })
        .is_ok());
    fs::write(
        h.path().join(".world-checkpoint.pending"),
        b"authored reward fault",
    )
    .unwrap();
    c.receive(ClientCommand::QuestgiverChooseReward {
        npc: mcbride,
        quest: 783,
        choice: 0,
    })
    .unwrap();
    let pending = c.pump(c.tick_duration());
    assert!(pending.pause.is_some());
    assert!(pending.events.is_empty());
    let digest = c.world.state_digest();
    let money = progress(&c).money;
    fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
    c.retry_checkpoint().unwrap();
    let published = c.pump(Duration::ZERO);
    assert_eq!(
        published
            .events
            .iter()
            .filter(|e| matches!(e, SessionEvent::QuestComplete(row) if row.quest_id == 783))
            .count(),
        1
    );
    assert_eq!(c.world.state_digest(), digest);
    assert_eq!(progress(&c).money, money);
    c = reopen_captured(&h, c, &data, &catalog);
    assert!(progress(&c).quests.progress(783).unwrap().rewarded);
    assert!(matches!(
        &c.receive(ClientCommand::QuestgiverStatusQuery { npc: mcbride })
            .unwrap()
            .events[..],
        [SessionEvent::QuestGiverStatus { status: 5, .. }]
    ));
    assert!(
        matches!(&c.receive(ClientCommand::QuestgiverHello { npc: mcbride }).unwrap().events[..], [SessionEvent::QuestDetail(detail)] if detail.quest_id == 7)
    );
    assert!(accept_id(&mut c, 3, 7).pause.is_none());
    c = reopen_captured(&h, c, &data, &catalog);
    assert_eq!(progress(&c).quests.progress(7).unwrap().creatures[0], 0);
    let changed = CapturedCatalog {
        base: LoopCatalog::new(&loop_fixture::profile()),
        quests: {
            let mut raw = presentation(&gameplay, data.map_id);
            raw.greetings.as_mut().unwrap()[0].text.push('!');
            raw.compile(&gameplay, -1).unwrap()
        },
    };
    assert_ne!(catalog.manifest_digest(), changed.manifest_digest());
}

fn run_short_route(
    h: &Harness,
    data: &n::Profile,
    catalog: &CapturedCatalog,
    restart: bool,
) -> (n::Checkpoint, Vec<SessionEvent>) {
    let mut c = enter_captured(h, data, catalog, false);
    let mut events = accept_id(&mut c, 4, 783).events;
    if restart {
        c = reopen_captured(h, c, data, catalog);
    }
    events.extend(reward_id(&mut c, 783).events);
    if restart {
        c = reopen_captured(h, c, data, catalog);
    }
    events.extend(accept_id(&mut c, 3, 7).events);
    for (index, spawn) in captured_fixture::TARGETS.into_iter().enumerate() {
        events.extend(kill(&mut c, spawn));
        assert_eq!(
            progress(&c).quests.progress(7).unwrap().creatures[0],
            index as u32 + 1
        );
        let opened = open_loot(&mut c, spawn);
        assert!(
            opened.events.iter().any(|event| matches!(event,
                SessionEvent::LootResponse { gold: 3, items, .. } if items.len() == 1
            )),
            "{:?}",
            opened.events
        );
        events.extend(opened.events);
        events.extend(collect(&mut c).events);
        assert_eq!(
            inventory(&c).item_count(loop_fixture::ITEM).unwrap(),
            index as u32 + 1
        );
        if restart && matches!(index, 0 | 8 | 9) {
            c = reopen_captured(h, c, data, catalog);
        }
    }
    let npc = guid(&c, 3);
    assert!(matches!(
        &c.receive(ClientCommand::QuestgiverComplete { npc, quest: 7 })
            .unwrap()
            .events[..],
        [SessionEvent::QuestOffer(_)]
    ));
    events.extend(reward_id(&mut c, 7).events);
    events.extend(accept_id(&mut c, 3, 15).events);
    if restart {
        c = reopen_captured(h, c, data, catalog);
    }
    assert!(progress(&c).quests.progress(7).unwrap().rewarded);
    assert_eq!(progress(&c).quests.progress(15).unwrap().creatures[0], 0);
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, SessionEvent::QuestComplete(row) if row.quest_id == 7))
            .count(),
        1
    );
    let checkpoint = c.world.checkpoint().unwrap();
    (checkpoint, events)
}

#[test]
fn captured_ten_kills_loot_rewards_and_followup_match_uninterrupted_checkpoint_and_transcript() {
    let (data, gameplay) = captured_fixture::profile();
    let catalog = CapturedCatalog::new(&data, &gameplay);
    let h = Harness::new();
    let uninterrupted = run_short_route(&h, &data, &catalog, false);
    fs::remove_dir_all(h.path()).unwrap();
    let resumed = run_short_route(&h, &data, &catalog, true);
    assert_eq!(uninterrupted.0, resumed.0);
    assert_eq!(format!("{:?}", uninterrupted.1), format!("{:?}", resumed.1));
}
