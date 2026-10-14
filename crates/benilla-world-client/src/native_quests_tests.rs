// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored projection fixtures. These contain no real quest text or production admission.
use super::*;
use n::synthetic_quest_fixture as fixture;

pub(in crate::native) struct Catalog {
    quests: BTreeMap<u32, crate::QuestMetadata>,
    bytes: Option<u32>,
    greeting: bool,
    greeting_row: bool,
    presentation: Option<crate::NativeQuestPresentation>,
}
impl Catalog {
    pub(in crate::native) fn new(w: &n::NativeWorld) -> Self {
        let g = genesis(w).unwrap();
        let quests = g
            .quest_ids()
            .map(|id| {
                let row = g.quest(id).unwrap();
                (
                    id,
                    crate::QuestMetadata {
                        template: p::QuestTemplate {
                            quest_id: id,
                            method: row.method,
                            level: row.reward.xp.quest_level,
                            zone_or_sort: 0,
                            quest_type: 0,
                            rep_objective_faction: row.excluded.rep_objective_faction,
                            rep_objective_value: row.excluded.rep_objective_value as u32,
                            next_quest_in_chain: row.next_in_chain,
                            money: row.reward.reward_or_required_money,
                            money_max_level: row.reward.xp.reward_money_max_level,
                            reward_spell: 0,
                            src_item_id: 0,
                            flags: u32::from(row.reward.quest_flags),
                            rewards: std::array::from_fn(|i| {
                                (
                                    row.excluded.reward_item[i],
                                    row.excluded.reward_item_count[i],
                                )
                            }),
                            choices: std::array::from_fn(|i| {
                                (
                                    row.excluded.reward_choice_item[i],
                                    row.excluded.reward_choice_count[i],
                                )
                            }),
                            point_map_id: w.profile().map_id,
                            point_x: 0.0,
                            point_y: 0.0,
                            point_opt: 0,
                            title: "Authored test | title".into(),
                            objectives_text: "Authored objective".into(),
                            details: "Authored detail".into(),
                            end_text: "Authored end".into(),
                            objectives: std::array::from_fn(|i| p::QuestObjective {
                                creature_or_go: row.required_creature[i] as u32,
                                required_count: row.required_creature_count[i],
                                item_id: row.required_item[i],
                                item_count: row.required_item_count[i],
                                text: String::new(),
                            }),
                        },
                        request_text: "Authored request".into(),
                        offer_text: "Authored offer".into(),
                    },
                )
            })
            .collect();
        Self {
            quests,
            bytes: Some(0x7fabcdef),
            greeting: true,
            greeting_row: true,
            presentation: Some(crate::NativeQuestPresentation {
                details_emotes: [(0, 0); 4],
                offer_emotes: [(0, 0); 4],
                complete_emote: 0,
                complete_emote_delay: 0,
                incomplete_emote: 0,
                incomplete_emote_delay: 0,
            }),
        }
    }
}
impl ClientCatalog for Catalog {
    fn build(&self) -> u32 {
        5875
    }
    fn manifest_digest(&self) -> Digest {
        [9; 32]
    }
    fn contains_map(&self, id: u32) -> bool {
        id == 7
    }
    fn contains_creature_display(&self, id: u32) -> bool {
        id == 60001
    }
    fn contains_item_display(&self, _: u32) -> bool {
        false
    }
    fn contains_spell(&self, id: u32) -> bool {
        matches!(id, 60001 | 60002 | 60004 | 60005)
    }
    fn player_power_type(&self, _: PlayerId) -> Option<u8> {
        Some(0)
    }
    fn player_bytes_2(&self, _: PlayerId) -> Option<u32> {
        self.bytes
    }
    fn item_metadata(&self, _: u32) -> Option<p::ItemInfo> {
        None
    }
    fn quest_metadata(&self, id: u32) -> Option<crate::QuestMetadata> {
        self.quests.get(&id).cloned()
    }
    fn quest_greeting(&self, _: u32) -> Option<String> {
        self.greeting.then(|| "Authored greeting".into())
    }
    fn native_quest_greeting(&self, _: u32) -> Option<crate::NativeQuestGreeting> {
        self.greeting.then(|| crate::NativeQuestGreeting {
            has_greeting_row: self.greeting_row,
            text: "Authored greeting".into(),
            emote_delay: 13,
            emote: 4,
        })
    }
    fn native_quest_presentation(&self, _: u32) -> Option<crate::NativeQuestPresentation> {
        self.presentation.clone()
    }
}
fn player_fields(events: &[SessionEvent], guid: u64) -> ObjectFields {
    let mut fields = ObjectFields::from_pairs(&[]);
    for e in events {
        if let SessionEvent::ObjectValues {
            guid: id,
            fields: update,
        } = e
        {
            if *id == guid {
                fields.merge(update.clone());
            }
        }
    }
    fields
}
fn step(
    w: &mut n::NativeWorld,
    p: &mut NativeProjection,
    a: &mut fixture::QuestAuthority,
    request: Option<UiRequest>,
) -> Vec<SessionEvent> {
    let input = request.map(|r| p.local(w, r).unwrap());
    w.step(w.state().tick + 1, input, a).unwrap();
    p.advance(w).unwrap()
}
fn giver(w: &n::NativeWorld) -> NpcHandle {
    NpcHandle {
        id: w.state().npcs[&3].id,
    }
}
fn kill(
    w: &mut n::NativeWorld,
    p: &mut NativeProjection,
    a: &mut fixture::QuestAuthority,
) -> Vec<SessionEvent> {
    let mut all = step(
        w,
        p,
        a,
        Some(UiRequest::CastAtNpc {
            ability: 60001,
            target: NpcHandle {
                id: w.state().npcs[&2].id,
            },
        }),
    );
    for _ in 0..20 {
        if w.state().npcs[&2].health == 0 {
            return all;
        }
        all.extend(step(w, p, a, None));
    }
    panic!("authored target remained alive")
}
#[test]
fn native_ui_accept_kill_level_complete_reward_and_resume_project_stock_state() {
    let mut w = fixture::world();
    let mut p = NativeProjection::new(&w, w.state().player, &Catalog::new(&w)).unwrap();
    let mut a = fixture::QuestAuthority::default();
    let giver = giver(&w);
    let guid = p.player_guid();
    let initial = p.initial_events(&w).unwrap();
    assert!(!initial
        .iter()
        .any(|e| matches!(e, SessionEvent::QuestGiverStatus { .. })));
    let fields = player_fields(&initial, guid);
    assert_eq!(fields.player_xp(), Some(0));
    assert_eq!(fields.player_next_level_xp(), Some(40));
    assert_eq!(fields.player_money(), Some(11));
    assert_eq!(fields.player_bytes_2(), Some(0x02abcdef));
    assert_eq!(fields.player_skill(0).unwrap().skill_id, 95); // native slot order, not key order
    assert!(
        matches!(p.quest_greeting(&w,giver).unwrap(),SessionEvent::QuestGreeting(ref q) if q.quests.len()==1&&q.quests[0].quest_id==70001&&q.quests[0].icon==p::dialog_status::AVAILABLE)
    );
    assert!(
        matches!(p.quest_detail(&w,70001,giver).unwrap(),SessionEvent::QuestDetail(ref q) if q.money==7&&q.details=="Authored detail")
    );
    assert_eq!(
        p.quest_offer(&w, 70001, giver).unwrap_err(),
        Error::UnavailableQuest
    );
    let accepted = step(
        &mut w,
        &mut p,
        &mut a,
        Some(UiRequest::AcceptQuest {
            quest: 70001,
            giver,
        }),
    );
    assert!(accepted
        .iter()
        .any(|e| matches!(e, SessionEvent::GossipComplete)));
    let close = accepted
        .iter()
        .position(|e| matches!(e, SessionEvent::GossipComplete))
        .unwrap();
    let accepted_log = accepted.iter().position(|e| matches!(e, SessionEvent::ObjectValues { fields, .. } if fields.player_quest_log(0).is_some_and(|q| q.quest_id == 70001))).unwrap();
    assert!(accepted_log < close);
    assert_eq!(
        player_fields(&accepted, guid)
            .player_quest_log(0)
            .unwrap()
            .quest_id,
        70001
    );
    assert!(
        matches!(p.quest_progress(&w,70001,giver).unwrap(),SessionEvent::QuestProgress(ref q) if !q.is_complete)
    );
    let killed = kill(&mut w, &mut p, &mut a);
    let xp = killed
        .iter()
        .position(|e| matches!(e,SessionEvent::XpGain(x) if x.kill&&x.total==50&&x.base==50))
        .unwrap();
    let credit = killed
        .iter()
        .position(|e| {
            matches!(
                e,
                SessionEvent::QuestObjectiveKill {
                    quest_id: 70001,
                    entry: 60001,
                    count: 1,
                    required: 1
                }
            )
        })
        .unwrap();
    assert!(xp < credit);
    assert!(killed.iter().any(|e|matches!(e,SessionEvent::PartyKillLog(x) if x.killer==guid&&x.victim==p.npc_guid(w.state().npcs[&2].id).unwrap())));
    assert!(killed
        .iter()
        .any(|e| matches!(e,SessionEvent::LevelUp(x) if x.level==2)));
    assert!(killed.iter().any(|e| matches!(e, SessionEvent::ObjectValues { fields, .. } if fields.player_quest_log(0).is_some_and(|q| q.state == 1))));
    assert!(!killed
        .iter()
        .any(|e| matches!(e, SessionEvent::QuestGiverStatus { .. })));
    assert!(matches!(
        p.quest_status(&w, giver).unwrap(),
        SessionEvent::QuestGiverStatus { status: 7, .. }
    ));
    assert!(!killed
        .iter()
        .any(|e| matches!(e, SessionEvent::QuestObjectivesComplete { .. })));
    assert!(
        matches!(p.quest_greeting(&w,giver).unwrap(),SessionEvent::QuestGreeting(ref q) if q.quests[0].icon==p::dialog_status::REWARD_REP)
    );
    assert!(
        matches!(p.quest_offer(&w,70001,giver).unwrap(),SessionEvent::QuestOffer(ref q) if q.money==7)
    );
    let rewarded = step(
        &mut w,
        &mut p,
        &mut a,
        Some(UiRequest::TurnInQuest {
            quest: 70001,
            giver,
        }),
    );
    assert!(rewarded
        .iter()
        .any(|e| matches!(e,SessionEvent::XpGain(x) if !x.kill&&x.total==100&&x.victim==0)));
    assert!(rewarded
        .iter()
        .any(|e| matches!(e,SessionEvent::LevelUp(x) if x.level==3)));
    assert!(rewarded.iter().any(|e|matches!(e,SessionEvent::QuestComplete(x) if x.quest_id==70001&&x.xp==100&&x.money==7&&x.items.is_empty())));
    let fields = player_fields(&rewarded, guid);
    assert_eq!(fields.player_money(), Some(18));
    assert_eq!(fields.player_xp(), Some(30));
    assert_eq!(fields.unit_base_mana(), Some(71));
    assert_eq!(fields.unit_base_health(), Some(5020));
    assert_eq!(fields.player_quest_log(0).unwrap().quest_id, 0);
    assert_eq!(fields.player_skill(0).unwrap().max, 15);
    assert_eq!(
        p.quest_offer(&w, 70001, giver).unwrap_err(),
        Error::UnavailableQuest
    );
    let duplicate = step(
        &mut w,
        &mut p,
        &mut a,
        Some(UiRequest::TurnInQuest {
            quest: 70001,
            giver,
        }),
    );
    assert!(!duplicate
        .iter()
        .any(|e| matches!(e, SessionEvent::QuestComplete(_) | SessionEvent::XpGain(_))));
    // CanRewardQuest silently refuses a completed nonrepeatable quest, then
    // HandleQuestgiverChooseRewardOpcode sends the ordinary offer again.
    assert!(duplicate
        .iter()
        .any(|e| matches!(e, SessionEvent::QuestOffer(offer) if offer.quest_id == 70001)));
    assert!(!duplicate.iter().any(|e| matches!(
        e,
        SessionEvent::InventoryFailure { .. } | SessionEvent::Notification { .. }
    )));
    let restored = n::NativeWorld::restore_synthetic_component_test(
        w.profile().clone(),
        w.checkpoint().unwrap(),
    )
    .unwrap();
    let restored_projection =
        NativeProjection::new(&restored, restored.state().player, &Catalog::new(&restored))
            .unwrap();
    let fields = player_fields(
        &restored_projection.initial_events(&restored).unwrap(),
        guid,
    );
    assert_eq!(fields.player_xp(), Some(30));
    assert_eq!(fields.player_money(), Some(18));
    assert_eq!(fields.player_quest_log(0).unwrap().quest_id, 0);
}
#[test]
fn metadata_is_explicit_and_bound_to_source_ids_and_appearance() {
    let w = fixture::world();
    let mut c = Catalog::new(&w);
    c.bytes = None;
    assert!(matches!(
        NativeProjection::new(&w, w.state().player, &c),
        Err(Error::InvalidClientManifest)
    ));
    c.bytes = Some(0);
    c.quests.get_mut(&70001).unwrap().template.objectives[0].creature_or_go = 42;
    assert!(matches!(
        NativeProjection::new(&w, w.state().player, &c),
        Err(Error::InvalidQuestMetadata(70001))
    ));
    c = Catalog::new(&w);
    c.quests.clear();
    assert!(matches!(
        NativeProjection::new(&w, w.state().player, &c),
        Err(Error::MissingQuestMetadata(70001))
    ));
    c = Catalog::new(&w);
    c.greeting = false;
    assert!(matches!(
        NativeProjection::new(&w, w.state().player, &c),
        Err(Error::MissingQuestGreeting(60003))
    ));
}
#[test]
fn previews_are_generation_and_revision_bound_and_do_not_authorize_acceptance() {
    let mut w = fixture::world();
    let mut p = NativeProjection::new(&w, w.state().player, &Catalog::new(&w)).unwrap();
    let mut a = fixture::QuestAuthority {
        deny: true,
        ..Default::default()
    };
    let giver = giver(&w);
    assert!(p.quest_detail(&w, 70001, giver).is_ok());
    let input = p
        .local(
            &w,
            UiRequest::AcceptQuest {
                quest: 70001,
                giver,
            },
        )
        .unwrap();
    w.step(1, Some(input), &mut a).unwrap();
    assert_eq!(
        p.quest_greeting(&w, giver).unwrap_err(),
        Error::StaleProjection
    );
    let denied = p.advance(&w).unwrap();
    assert!(!denied
        .iter()
        .any(|e| matches!(e, SessionEvent::GossipComplete)));
    assert!(w
        .state()
        .quest_progression
        .as_ref()
        .unwrap()
        .quests
        .progress(70001)
        .is_none());
    let stale = NpcHandle {
        id: NpcId {
            generation: giver.id.generation + 1,
            ..giver.id
        },
    };
    assert_eq!(
        p.quest_detail(&w, 70001, stale).unwrap_err(),
        Error::UnknownTarget
    );
    let command = p
        .local(
            &w,
            UiRequest::AcceptQuest {
                quest: 70001,
                giver: stale,
            },
        )
        .unwrap();
    assert!(matches!(command.input.command,n::Command::AcceptQuest{giver,..} if giver==stale.id));
    assert_eq!(p.advance(&w).unwrap_err(), Error::StaleProjection);
}
#[test]
fn marker_level_hiding_is_distinct_from_menu_eligibility() {
    let w = fixture::world();
    let view = state(&w).unwrap().quests.view();
    assert_eq!(
        marker_icon(&w, &view, 9, 60003, 70001).unwrap(),
        p::dialog_status::CHAT
    );
    assert_eq!(
        menu_icon(&w, &view, 9, 60003, 70001).unwrap(),
        p::dialog_status::AVAILABLE
    );
    let (mut profile, input, mut data) = fixture::setup();
    data.quests[0].eligibility.minimum_level = 3;
    profile.initialize_quest_progression(&input, data).unwrap();
    let w = n::NativeWorld::synthetic_component_test(
        profile,
        [1; 16],
        n::synthetic_fixture::player(),
        42,
    )
    .unwrap();
    let view = state(&w).unwrap().quests.view();
    assert_eq!(
        marker_icon(&w, &view, 1, 60003, 70001).unwrap(),
        p::dialog_status::UNAVAILABLE
    );
    assert_eq!(
        menu_icon(&w, &view, 1, 60003, 70001).unwrap(),
        p::dialog_status::NONE
    );
}

#[test]
fn hidden_rewards_and_no_conversion_flags_remain_source_bound() {
    let (mut profile, input, mut data) = fixture::setup();
    data.quests[0].reward.quest_flags =
        (p::quest_flags::HIDDEN_REWARDS as u16) | arithmetic::QUEST_FLAGS_NO_MONEY_FROM_XP;
    profile.initialize_quest_progression(&input, data).unwrap();
    let mut w = n::NativeWorld::synthetic_component_test(
        profile,
        [1; 16],
        n::synthetic_fixture::player(),
        42,
    )
    .unwrap();
    let mut projection = NativeProjection::new(&w, w.state().player, &Catalog::new(&w)).unwrap();
    let giver = giver(&w);
    assert!(
        matches!(projection.quest_template(&w, 70001).unwrap(), SessionEvent::QuestTemplate(t) if t.money == 0 && t.money_max_level == 0)
    );
    assert!(
        matches!(projection.quest_detail(&w, 70001, giver).unwrap(), SessionEvent::QuestDetail(t) if t.money == 0)
    );
    let mut authority = fixture::QuestAuthority::default();
    step(
        &mut w,
        &mut projection,
        &mut authority,
        Some(UiRequest::AcceptQuest {
            quest: 70001,
            giver,
        }),
    );
    kill(&mut w, &mut projection, &mut authority);
    assert!(
        matches!(projection.quest_offer(&w, 70001, giver).unwrap(), SessionEvent::QuestOffer(t) if t.money == 7)
    );
}
#[test]
fn reward_announcement_uses_callback_level_not_final_world_level() {
    let w = fixture::world();
    let projection = NativeProjection::new(&w, w.state().player, &Catalog::new(&w)).unwrap();
    let event = n::QuestProgressionEvent::Quest {
        event: q::Event::RewardAnnouncement {
            quest: 70001,
            xp: 100,
        },
        level: 60,
        money: 18,
    };
    let events = projection
        .quest_progression_event(
            &w,
            &event,
            &mut Frame::new(w.state()),
            &mut loot::Frame::new(w.state()),
        )
        .unwrap();
    assert!(matches!(&events[..], [SessionEvent::QuestComplete(t)] if t.xp == 0 && t.money == 67));
    // The packet is a source display quote; the immutable final currency snapshot is not rewritten.
    assert_eq!(state(&w).unwrap().money, 11);
}
#[test]
fn batch_accept_and_rewards_preserve_every_committed_money_callback() {
    let (mut profile, input, mut data) = fixture::setup();
    data.quests[0].required_creature = [0; 4];
    data.quests[0].required_creature_count = [0; 4];
    data.quests[0].special_flags = 0;
    data.quests[0].reward.xp.reward_money_max_level = 0;
    let mut second = data.quests[0].clone();
    second.id = 70002;
    second.reward.reward_or_required_money = 9;
    data.quests.push(second);
    let relation = q::CreatureQuestRelation {
        creature: 60003,
        quest: 70002,
    };
    data.relations.starters.push(relation.clone());
    data.relations.finishers.push(relation);
    profile.initialize_quest_progression(&input, data).unwrap();
    let mut w = n::NativeWorld::synthetic_component_test(
        profile,
        [1; 16],
        n::synthetic_fixture::player(),
        42,
    )
    .unwrap();
    let mut projection = NativeProjection::new(&w, w.state().player, &Catalog::new(&w)).unwrap();
    let giver = giver(&w);
    let requests = [
        UiRequest::AcceptQuest {
            quest: 70001,
            giver,
        },
        UiRequest::AcceptQuest {
            quest: 70002,
            giver,
        },
        UiRequest::TurnInQuest {
            quest: 70001,
            giver,
        },
        UiRequest::TurnInQuest {
            quest: 70002,
            giver,
        },
    ];
    let inputs = requests
        .into_iter()
        .enumerate()
        .map(|(i, request)| {
            let mut input = projection.local(&w, request).unwrap();
            input.input.sequence += i as u64;
            input
        })
        .collect::<Vec<_>>();
    w.step_batch(1, inputs, &mut fixture::QuestAuthority::default())
        .unwrap();
    let events = projection.advance(&w).unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, SessionEvent::GossipComplete))
            .count(),
        2
    );
    let money = events
        .iter()
        .filter_map(|e| {
            if let SessionEvent::ObjectValues { fields, .. } = e {
                fields.player_money()
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(money, vec![18, 27, 27]);
    let complete = events
        .iter()
        .filter_map(|e| {
            if let SessionEvent::QuestComplete(q) = e {
                Some(q.quest_id)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(complete, vec![70001, 70002]);
    assert_eq!(state(&w).unwrap().money, 27);
}

#[test]
fn world_objects_callbacks_emit_no_creature_status_packets() {
    let w = fixture::world();
    let projection = NativeProjection::new(&w, w.state().player, &Catalog::new(&w)).unwrap();
    let mut view = state(&w).unwrap().quests.view();
    view.records.insert(
        70001,
        q::Progress {
            status: q::Status::Complete,
            rewarded: false,
            items: [0; 4],
            creatures: [1, 0, 0, 0],
        },
    );
    let event = n::QuestProgressionEvent::Quest {
        event: q::Event::WorldObjects { view },
        level: 2,
        money: 11,
    };
    // Pinned Player::UpdateForQuestWorldObjects touches only dynamic GO fields.
    // This creature-only profile must not fabricate an NPC status response.
    assert!(projection
        .quest_progression_event(
            &w,
            &event,
            &mut Frame::new(w.state()),
            &mut loot::Frame::new(w.state())
        )
        .unwrap()
        .is_empty());
}
#[test]
fn generic_status_projection_is_not_an_interaction_distance_check() {
    let (mut profile, input, data) = fixture::setup();
    profile
        .npcs
        .iter_mut()
        .find(|n| n.entry == 60003)
        .unwrap()
        .ai
        .spawn = n::synthetic_fixture::point(100.0);
    profile.initialize_quest_progression(&input, data).unwrap();
    let w = n::NativeWorld::synthetic_component_test(
        profile,
        [1; 16],
        n::synthetic_fixture::player(),
        42,
    )
    .unwrap();
    let projection = NativeProjection::new(&w, w.state().player, &Catalog::new(&w)).unwrap();
    let giver = giver(&w);
    // The controller must first authorize this request through generic CanInteract.
    assert!(matches!(
        projection.quest_status(&w, giver).unwrap(),
        SessionEvent::QuestGiverStatus { status: 5, .. }
    ));
    assert_eq!(
        projection.quest_greeting(&w, giver).unwrap_err(),
        Error::OutOfRange
    );
    let stale = NpcHandle {
        id: NpcId {
            generation: giver.id.generation + 1,
            ..giver.id
        },
    };
    assert_eq!(
        projection.quest_status(&w, stale).unwrap_err(),
        Error::UnknownTarget
    );
}

#[test]
fn generic_status_projection_does_not_apply_immediate_combat_gates() {
    let (mut profile, input, data) = fixture::setup();
    profile
        .npcs
        .iter_mut()
        .find(|n| n.entry == 60003)
        .unwrap()
        .ai
        .reaction = ai::Reaction::Aggressive;
    profile.initialize_quest_progression(&input, data).unwrap();
    let mut w = n::NativeWorld::synthetic_component_test(
        profile,
        [1; 16],
        n::synthetic_fixture::player(),
        42,
    )
    .unwrap();
    let mut projection = NativeProjection::new(&w, w.state().player, &Catalog::new(&w)).unwrap();
    let mut authority = fixture::QuestAuthority::default();
    for _ in 0..20 {
        step(&mut w, &mut projection, &mut authority, None);
        if w.state().npcs[&3].ai.phase() == ai::Phase::Combat {
            break;
        }
    }
    assert_eq!(w.state().npcs[&3].ai.phase(), ai::Phase::Combat);
    let giver = giver(&w);
    let before = w.state_digest();
    assert!(w.preview_quest_status(giver.id, &mut authority).unwrap());
    assert_eq!(before, w.state_digest());
    assert!(matches!(
        projection.quest_status(&w, giver).unwrap(),
        SessionEvent::QuestGiverStatus { status: 5, .. }
    ));
    assert_eq!(
        projection.quest_greeting(&w, giver).unwrap_err(),
        Error::UnknownTarget
    );
}

fn authored_world(
    mut profile: n::Profile,
    input: &benilla_sim::starter_init::StarterInput,
    data: n::QuestProgressionInput,
) -> n::NativeWorld {
    profile.initialize_quest_progression(input, data).unwrap();
    n::NativeWorld::synthetic_component_test(profile, [1; 16], n::synthetic_fixture::player(), 42)
        .unwrap()
}
#[test]
fn greeting_roles_captured_relation_order_and_real_emotes_are_preserved() {
    let (profile, input, mut data) = fixture::setup();
    let mut second = data.quests[0].clone();
    second.id = 70002;
    data.quests.push(second);
    data.relations.finishers = vec![q::CreatureQuestRelation {
        creature: 60003,
        quest: 70002,
    }];
    data.relations.starters.push(q::CreatureQuestRelation {
        creature: 60003,
        quest: 70002,
    });
    let w = authored_world(profile.clone(), &input, data.clone());
    let p = NativeProjection::new(&w, w.state().player, &Catalog::new(&w)).unwrap();
    let SessionEvent::QuestGreeting(menu) = p.quest_greeting(&w, giver(&w)).unwrap() else {
        panic!()
    };
    assert_eq!(
        menu.quests.iter().map(|q| q.quest_id).collect::<Vec<_>>(),
        [70001, 70002]
    );
    assert_eq!((menu.emote_delay, menu.emote), (13, 4));
    data.relations.starters.reverse();
    let reversed = authored_world(profile, &input, data);
    assert_ne!(w.profile_digest(), reversed.profile_digest());
    let p = NativeProjection::new(&reversed, reversed.state().player, &Catalog::new(&reversed))
        .unwrap();
    let SessionEvent::QuestGreeting(menu) = p.quest_greeting(&reversed, giver(&reversed)).unwrap()
    else {
        panic!()
    };
    assert_eq!(
        menu.quests.iter().map(|q| q.quest_id).collect::<Vec<_>>(),
        [70002, 70001]
    );
}
#[test]
fn greeting_single_autoopen_empty_suppression_and_unknown_emotes_fail_closed() {
    let (profile, input, mut data) = fixture::setup();
    data.dialog_rules.always_show_quest_greeting = false;
    let mut w = authored_world(profile, &input, data);
    let mut catalog = Catalog::new(&w);
    let mut p = NativeProjection::new(&w, w.state().player, &catalog).unwrap();
    let giver = giver(&w);
    assert!(matches!(
        &p.quest_greeting_events(&w, giver).unwrap()[..],
        [SessionEvent::QuestDetail(_)]
    ));
    let mut a = fixture::QuestAuthority::default();
    step(
        &mut w,
        &mut p,
        &mut a,
        Some(UiRequest::AcceptQuest {
            quest: 70001,
            giver,
        }),
    );
    assert!(
        matches!(p.quest_greeting(&w,giver).unwrap(),SessionEvent::QuestProgress(q) if q.close_on_cancel==1)
    );
    kill(&mut w, &mut p, &mut a);
    assert!(matches!(
        p.quest_greeting(&w, giver).unwrap(),
        SessionEvent::QuestOffer(_)
    ));
    assert!(matches!(
        p.quest_progress(&w, 70001, giver).unwrap(),
        SessionEvent::QuestOffer(_)
    ));
    step(
        &mut w,
        &mut p,
        &mut a,
        Some(UiRequest::TurnInQuest {
            quest: 70001,
            giver,
        }),
    );
    assert!(p.quest_greeting_events(&w, giver).unwrap().is_empty());
    catalog.presentation = None;
    assert!(matches!(
        NativeProjection::new(&w, w.state().player, &catalog),
        Err(Error::InvalidQuestMetadata(70001))
    ));
    catalog = Catalog::new(&w);
    catalog.presentation.as_mut().unwrap().offer_emotes[0] = (4, 50);
    assert!(matches!(
        NativeProjection::new(&w, w.state().player, &catalog),
        Err(Error::InvalidQuestMetadata(70001))
    ));
    // A proven absent greeting row autoopens even when AlwaysShow is true.
    let w = fixture::world();
    let mut catalog = Catalog::new(&w);
    catalog.greeting_row = false;
    let p = NativeProjection::new(&w, w.state().player, &catalog).unwrap();
    assert!(matches!(
        p.quest_greeting(
            &w,
            NpcHandle {
                id: w.state().npcs[&3].id
            }
        )
        .unwrap(),
        SessionEvent::QuestDetail(_)
    ));
}
#[test]
fn empty_request_text_redirect_is_presentation_only_and_offer_words_are_source_spells() {
    let (profile, input, mut data) = fixture::setup();
    data.quests[0].reward.quest_flags = p::quest_flags::HIDDEN_REWARDS as u16;
    let mut w = authored_world(profile, &input, data);
    let mut catalog = Catalog::new(&w);
    catalog.quests.get_mut(&70001).unwrap().request_text.clear();
    let mut p = NativeProjection::new(&w, w.state().player, &catalog).unwrap();
    let giver = giver(&w);
    let mut a = fixture::QuestAuthority::default();
    step(
        &mut w,
        &mut p,
        &mut a,
        Some(UiRequest::AcceptQuest {
            quest: 70001,
            giver,
        }),
    );
    assert!(
        matches!(p.quest_progress(&w,70001,giver).unwrap(),SessionEvent::QuestOffer(q) if q.quest_flags==0&&q.reward_spell==0)
    );
    assert_eq!(
        p.quest_offer(&w, 70001, giver).unwrap_err(),
        Error::UnavailableQuest
    );
    let events = step(
        &mut w,
        &mut p,
        &mut a,
        Some(UiRequest::TurnInQuest {
            quest: 70001,
            giver,
        }),
    );
    assert!(!events
        .iter()
        .any(|e| matches!(e, SessionEvent::QuestComplete(_))));
    assert_eq!(state(&w).unwrap().money, 11);
}
#[test]
fn chain_details_follow_reward_even_when_next_quest_cannot_be_taken() {
    let (profile, input, mut data) = fixture::setup();
    data.quests[0].required_creature = [0; 4];
    data.quests[0].required_creature_count = [0; 4];
    data.quests[0].special_flags = 0;
    data.quests[0].reward.xp.reward_money_max_level = 0;
    let mut second = data.quests[0].clone();
    second.id = 70002;
    second.eligibility.minimum_level = 60;
    data.quests[0].next_in_chain = 70002;
    data.quests.push(second);
    data.relations.starters.push(q::CreatureQuestRelation {
        creature: 60003,
        quest: 70002,
    });
    let mut w = authored_world(profile, &input, data);
    let mut p = NativeProjection::new(&w, w.state().player, &Catalog::new(&w)).unwrap();
    let giver = giver(&w);
    let mut a = fixture::QuestAuthority::default();
    step(
        &mut w,
        &mut p,
        &mut a,
        Some(UiRequest::AcceptQuest {
            quest: 70001,
            giver,
        }),
    );
    let events = step(
        &mut w,
        &mut p,
        &mut a,
        Some(UiRequest::TurnInQuest {
            quest: 70001,
            giver,
        }),
    );
    let reward = events
        .iter()
        .position(|e| matches!(e,SessionEvent::QuestComplete(q) if q.quest_id==70001))
        .unwrap();
    let details = events
        .iter()
        .position(|e| matches!(e,SessionEvent::QuestDetail(q) if q.quest_id==70002))
        .unwrap();
    assert!(reward < details);
    assert_eq!(
        p.quest_detail(&w, 70002, giver).unwrap_err(),
        Error::UnavailableQuest
    );
}
#[test]
fn learned_packets_follow_callback_order_before_create_stats_and_credit() {
    let (profile, input, data) = fixture::learned_setup();
    let mut w = authored_world(profile, &input, data);
    let genesis = w.checkpoint().unwrap();
    let mut p = NativeProjection::new(&w, w.state().player, &Catalog::new(&w)).unwrap();
    let giver = giver(&w);
    let mut a = fixture::QuestAuthority::default();
    step(
        &mut w,
        &mut p,
        &mut a,
        Some(UiRequest::AcceptQuest {
            quest: 70001,
            giver,
        }),
    );
    let events = kill(&mut w, &mut p, &mut a);
    let learned = events
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            if let SessionEvent::SpellLearned { spell_id: id } = e {
                Some((i, *id))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        learned.iter().map(|(_, id)| *id).collect::<Vec<_>>(),
        [60005, 60004]
    );
    let create=events.iter().position(|e|matches!(e,SessionEvent::ObjectValues{guid,fields} if *guid==p.player_guid()&&fields.raw_fields().any(|(id,value)|id==163&&value==5010))).unwrap();
    assert!(learned[1].0 < create);
    let credit = events
        .iter()
        .position(|e| {
            matches!(
                e,
                SessionEvent::QuestObjectiveKill {
                    quest_id: 70001,
                    ..
                }
            )
        })
        .unwrap();
    assert!(learned[1].0 < credit);
    let mut replay =
        n::NativeWorld::restore_synthetic_component_test(w.profile().clone(), genesis).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
    let p = NativeProjection::new(&replay, replay.state().player, &Catalog::new(&replay)).unwrap();
    assert!(!p
        .initial_events(&replay)
        .unwrap()
        .iter()
        .any(|e| matches!(e, SessionEvent::SpellLearned { .. })));
}

#[test]
fn private_spell_closure_requires_client_rows_without_an_execution_recipe() {
    let (mut profile, mut input, data) = fixture::setup();
    let mut dependency = input.spells[0].clone();
    dependency.id = 60006;
    dependency.words[0] = 60006;
    input.spells.push(dependency);
    profile
        .initialize_player(&input, n::synthetic_fixture::float(100.0), 1)
        .unwrap();
    let w = authored_world(profile, &input, data);
    assert!(w.profile().ability(60006).is_none());
    assert!(matches!(
        NativeProjection::new(&w, w.state().player, &Catalog::new(&w)),
        Err(Error::UnboundSpell(60006))
    ));
}

#[test]
fn reward_during_prepare_projects_original_start_and_delayed_source_release() {
    let mut w = n::NativeWorld::synthetic_component_test(
        fixture::release_profile(false),
        [1; 16],
        n::synthetic_fixture::player(),
        42,
    )
    .unwrap();
    let mut p = NativeProjection::new(&w, w.state().player, &Catalog::new(&w)).unwrap();
    let mut a = fixture::QuestAuthority::default();
    let giver = giver(&w);
    step(
        &mut w,
        &mut p,
        &mut a,
        Some(UiRequest::AcceptQuest {
            quest: 70001,
            giver,
        }),
    );
    kill(&mut w, &mut p, &mut a);
    let target = NpcHandle {
        id: w.state().npcs[&4].id,
    };
    let started = step(
        &mut w,
        &mut p,
        &mut a,
        Some(UiRequest::CastAtNpc {
            ability: 60001,
            target,
        }),
    );
    assert!(started.iter().any(|event| matches!(
        event,
        SessionEvent::SpellStart {
            spell_id: 60001,
            cast_time_ms: 350,
            ..
        }
    )));
    let id = *w.state().casts.keys().next().unwrap();
    let rewarded = step(
        &mut w,
        &mut p,
        &mut a,
        Some(UiRequest::TurnInQuest {
            quest: 70001,
            giver,
        }),
    );
    assert!(rewarded
        .iter()
        .any(|event| matches!(event, SessionEvent::LevelUp(_))));
    assert!(!rewarded.iter().any(|event| matches!(
        event,
        SessionEvent::SpellGo { .. } | SessionEvent::SpellFailedOther { .. }
    )));
    assert_eq!(
        w.state().casts[&id].execution.state(),
        spell::CastState::Casting { remaining_ms: 250 }
    );
    let mut go = 0;
    for _ in 0..10 {
        let events = step(&mut w, &mut p, &mut a, None);
        go += events
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    SessionEvent::SpellGo {
                        spell_id: 60001,
                        ..
                    }
                )
            })
            .count();
        if matches!(
            w.state().casts[&id].execution.state(),
            spell::CastState::Traveling { .. }
        ) {
            break;
        }
    }
    assert_eq!(go, 1);
    assert_eq!(
        w.state().casts[&id].release_binding.as_ref().unwrap().level,
        3
    );
}

#[test]
fn native_reputation_packets_preserve_intermediate_callbacks_and_pre_xp_order() {
    use n::synthetic_reputation_fixture as rep;
    let mut w = rep::world();
    let mut p = NativeProjection::new(&w, w.state().player, &Catalog::new(&w)).unwrap();
    let mut a = fixture::QuestAuthority::default();
    let giver = giver(&w);
    let initial = p.initial_events(&w).unwrap();
    let standings = initial
        .iter()
        .find_map(|e| match e {
            SessionEvent::Reputations { standings } => Some(standings),
            _ => None,
        })
        .unwrap();
    assert_eq!(standings.len(), 64);
    assert_eq!(standings[3], (2, 0));
    assert_eq!(standings[7], (0, 0));
    assert!(standings
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != 3)
        .all(|(_, s)| *s == (0, 0)));
    assert!(
        matches!(p.quest_greeting(&w,giver).unwrap(),SessionEvent::QuestGreeting(menu) if !menu.quests.iter().any(|q|q.quest_id==rep::GATED))
    );
    let accepted = step(
        &mut w,
        &mut p,
        &mut a,
        Some(UiRequest::AcceptQuest {
            quest: rep::OBJECTIVE,
            giver,
        }),
    );
    let visible = accepted
        .iter()
        .position(|e| matches!(e, SessionEvent::ReputationVisible { list_id: 7 }))
        .unwrap();
    let log=accepted.iter().position(|e|matches!(e,SessionEvent::ObjectValues{fields,..} if fields.player_quest_log(0).is_some_and(|q|q.quest_id==rep::OBJECTIVE))).unwrap();
    assert!(visible < log);
    let close = accepted
        .iter()
        .position(|e| matches!(e, SessionEvent::GossipComplete))
        .unwrap();
    assert!(log < close);
    step(
        &mut w,
        &mut p,
        &mut a,
        Some(UiRequest::AcceptQuest {
            quest: rep::GRANT,
            giver,
        }),
    );
    let rewarded = step(
        &mut w,
        &mut p,
        &mut a,
        Some(UiRequest::TurnInQuest {
            quest: rep::GRANT,
            giver,
        }),
    );
    let deltas: Vec<_> = rewarded
        .iter()
        .filter_map(|e| match e {
            SessionEvent::ReputationDelta { standings } => Some(standings.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        deltas,
        vec![
            vec![(3, 200), (7, 200)],
            vec![(3, 100), (7, 100)],
            vec![(3, 200), (7, 200)]
        ]
    );
    let toggles: Vec<_> = rewarded
        .iter()
        .take_while(|e| !matches!(e, SessionEvent::XpGain(_)))
        .filter_map(|e| match e {
            SessionEvent::ObjectValues { fields, .. } => {
                fields.player_quest_log(0).map(|q| q.state)
            }
            _ => None,
        })
        .collect();
    assert_eq!(toggles, [1, 0, 1]);
    let last_delta = rewarded
        .iter()
        .rposition(|e| matches!(e, SessionEvent::ReputationDelta { .. }))
        .unwrap();
    let clear=rewarded.iter().position(|e|matches!(e,SessionEvent::ObjectValues{fields,..} if fields.player_quest_log(1).is_some_and(|q|q.quest_id==0))).unwrap();
    let xp = rewarded
        .iter()
        .position(|e| matches!(e, SessionEvent::XpGain(_)))
        .unwrap();
    assert!(last_delta < clear && clear < xp);
    assert!(
        matches!(p.quest_greeting(&w,giver).unwrap(),SessionEvent::QuestGreeting(menu) if menu.quests.iter().any(|q|q.quest_id==rep::GATED))
    );
    let restored = n::NativeWorld::restore_synthetic_component_test(
        w.profile().clone(),
        w.checkpoint().unwrap(),
    )
    .unwrap();
    let reopened =
        NativeProjection::new(&restored, restored.state().player, &Catalog::new(&restored))
            .unwrap()
            .initial_events(&restored)
            .unwrap();
    assert!(reopened.iter().any(|e|matches!(e,SessionEvent::Reputations{standings} if standings[3]==(1,200)&&standings[7]==(1,200))));
    assert!(!reopened.iter().any(|e| matches!(
        e,
        SessionEvent::ReputationDelta { .. }
            | SessionEvent::XpGain(_)
            | SessionEvent::ReputationVisible { .. }
    )));
}
