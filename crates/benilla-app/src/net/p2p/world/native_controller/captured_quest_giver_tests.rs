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
        let mut raw = presentation(gameplay, profile.map_id);
        let texts = gameplay
            .catalog()
            .mechanics()
            .gossip_text_ids()
            .collect::<Vec<_>>();
        for id in texts {
            raw.npc_text_keys.push(id);
            raw.npc_text.as_mut().unwrap().push(CapturedNpcText {
                id,
                options: std::array::from_fn(|index| CapturedNpcTextOption {
                    probability_bits: if index == 0 { 1f32.to_bits() } else { 0 },
                    text0: if index == 0 {
                        "Authored ordinary gossip".into()
                    } else {
                        String::new()
                    },
                    text1: String::new(),
                    language: 0,
                    emotes: [(0, 0); 3],
                    broadcast: 0,
                }),
            });
        }
        for id in gameplay.catalog().mechanics().script_broadcast_ids() {
            raw.broadcasts
                .as_mut()
                .unwrap()
                .push(n::EventAiBroadcastSource {
                    id: id as i32,
                    text: Some("Authored completion speech $N".into()),
                    text1: Some(String::new()),
                    chat_type: 0,
                    language: 0,
                    condition_id: 0,
                    emotes_id: 0,
                    flags: 1,
                    sounds: [0; 2],
                    emote_ids: [0; 3],
                    emote_delays: [0; 3],
                    verified_build: 0,
                });
        }
        if gameplay
            .quests()
            .iter()
            .any(|r| r.excluded.source_item == 9571)
        {
            raw.items.as_mut().unwrap().push(9571);
        }
        if gameplay
            .catalog()
            .mechanics()
            .script_broadcast_ids()
            .next()
            .is_some()
        {
            raw.emotes.as_mut().unwrap().push(71);
        }
        Self {
            base: LoopCatalog::new(&loop_fixture::profile()),
            quests: raw.compile(gameplay, -1).unwrap(),
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
        self.base
            .item_metadata(if id == 9571 { loop_fixture::ITEM } else { id })
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
    fn native_npc_text(&self, id: u32) -> Option<Vec<benilla_protocol::messages::NpcTextBlock>> {
        self.quests.native_npc_text(id).map(<[_]>::to_vec)
    }
    fn native_quest_script_text(&self, id: u32) -> Option<String> {
        self.quests.native_quest_script_text(id).map(str::to_owned)
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

#[test]
fn full_authored_giver_closure_executes_scripts_source_items_and_gossip_across_reopen() {
    let (data, gameplay) = captured_fixture::profile_with_mechanics();
    assert_eq!(
        gameplay.catalog().ids().collect::<Vec<_>>(),
        [7, 15, 21, 783, 3104]
    );
    let catalog = CapturedCatalog::new(&data, &gameplay);
    let h = Harness::new();
    let mut c = enter_captured(&h, &data, &catalog, false);
    let npc = guid(&c, 3);
    assert!(
        matches!(&c.receive(ClientCommand::QuestgiverHello { npc }).unwrap().events[..],
        [SessionEvent::GossipMenu { text_id: 75001, options, quests, .. }] if options.is_empty() && quests.is_empty())
    );
    assert!(
        matches!(&c.receive(ClientCommand::NpcTextQuery { text_id: 75001, guid: npc }).unwrap().events[..],
        [SessionEvent::NpcGreeting { text_id: 75001, blocks }] if blocks[0].male == "Authored ordinary gossip")
    );
    assert!(accept_id(&mut c, 4, 783).pause.is_none());
    assert!(reward_id(&mut c, 783).pause.is_none());
    let mut kills = captured_fixture::MECHANIC_TARGETS.into_iter();
    for (quest, count) in [(7, 10), (15, 10), (21, 12)] {
        assert!(accept_id(&mut c, 3, quest).pause.is_none());
        for index in 0..count {
            kill(&mut c, kills.next().unwrap());
            assert_eq!(
                progress(&c).quests.progress(quest).unwrap().creatures[0],
                index + 1
            );
            if index == count / 2 {
                c = reopen_captured(&h, c, &data, &catalog);
            }
        }
        let rewarded = reward_id(&mut c, quest);
        assert!(rewarded.pause.is_none(), "{:?}", rewarded.pause);
        assert!(progress(&c).quests.progress(quest).unwrap().rewarded);
        if quest == 7 {
            let accepted = accept_id(&mut c, 3, 3104);
            assert!(accepted.pause.is_none());
            assert!(accepted.events.iter().any(|event| matches!(event,
                SessionEvent::ItemPushResult(push) if push.item_entry == 9571
                    && push.from_npc && !push.created && push.show_in_chat && push.count == 1)));
            assert_eq!(inventory(&c).item_count(9571).unwrap(), 1);
            c = reopen_captured(&h, c, &data, &catalog);
            let slot = progress(&c)
                .quests
                .view()
                .log
                .iter()
                .position(|id| *id == Some(3104))
                .unwrap();
            c.receive(ClientCommand::QuestlogRemove { slot: slot as u8 })
                .unwrap();
            let abandoned = step(&mut c);
            assert!(abandoned.pause.is_none());
            assert_eq!(inventory(&c).item_count(9571).unwrap(), 0);
            assert!(accept_id(&mut c, 3, 3104).pause.is_none());
            assert_eq!(inventory(&c).item_count(9571).unwrap(), 1);
            assert!(reward_id(&mut c, 3104).pause.is_none());
            assert_eq!(inventory(&c).item_count(9571).unwrap(), 0);
        } else if quest == 21 {
            assert!(!rewarded.events.iter().any(|e| matches!(
                e,
                SessionEvent::Chat(_) | SessionEvent::Emote { emote_id: 71, .. }
            )));
            c = reopen_captured(&h, c, &data, &catalog);
            let mut effects = vec![];
            for _ in 0..25 {
                let out = step(&mut c);
                assert!(out.pause.is_none(), "{:?}", out.pause);
                effects.extend(out.events.into_iter().filter(|e| {
                    matches!(
                        e,
                        SessionEvent::Chat(_) | SessionEvent::Emote { emote_id: 71, .. }
                    )
                }));
            }
            assert!(
                matches!(&effects[..], [SessionEvent::Chat(chat), SessionEvent::Emote { emote_id: 71, .. }]
                if chat.text == "Authored completion speech $N")
            );
            c = reopen_captured(&h, c, &data, &catalog);
            for _ in 0..3 {
                assert!(!step(&mut c).events.iter().any(|e| matches!(
                    e,
                    SessionEvent::Chat(_) | SessionEvent::Emote { emote_id: 71, .. }
                )));
            }
        }
    }
    assert!(kills.next().is_none());
}
