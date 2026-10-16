// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored EventAI receipts through the stock protocol boundary, without assets.
use super::*;
use n::synthetic_event_ai_fixture as fixture;

const SPEAKING_SEED: u64 = 1234;

struct Catalog;
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
        matches!(id, 60001 | 96002)
    }
    fn contains_item_display(&self, _: u32) -> bool {
        false
    }
    fn contains_spell(&self, _: u32) -> bool {
        true
    }
    fn player_power_type(&self, _: PlayerId) -> Option<u8> {
        Some(0)
    }
    fn item_metadata(&self, _: u32) -> Option<p::ItemInfo> {
        None
    }
    fn quest_metadata(&self, _: u32) -> Option<crate::QuestMetadata> {
        None
    }
    fn quest_greeting(&self, _: u32) -> Option<String> {
        None
    }
}

fn chats(events: &[SessionEvent]) -> Vec<&p::ChatMessage> {
    events
        .iter()
        .filter_map(|event| match event {
            SessionEvent::Chat(chat) => Some(chat),
            _ => None,
        })
        .collect()
}

fn speech_boundary() -> (n::NativeWorld, NativeProjection, n::EventAiSpeech) {
    let mut world = fixture::world(SPEAKING_SEED);
    let mut projection = NativeProjection::new(&world, world.state().player, &Catalog).unwrap();
    assert!(chats(&projection.initial_events(&world).unwrap()).is_empty());
    let mut authority = fixture::Authority::default();
    for tick in 1..=10 {
        world.step(tick, None, &mut authority).unwrap();
        if let Some(speech) = world
            .last_tick()
            .unwrap()
            .events
            .iter()
            .find_map(|event| match event {
                n::Event::EventAiSpeech(speech) => Some((**speech).clone()),
                _ => None,
            })
        {
            return (world, projection, speech);
        }
        assert!(chats(&projection.advance(&world).unwrap()).is_empty());
    }
    panic!("authored seed must reach an audible AGGRO callback");
}

fn source_packet(speech: &n::EventAiSpeech, speaker: u64, player: u64) -> Vec<u8> {
    // Pinned Chat.cpp:3616-3668 writes byte counts INCLUDING each UTF-8 NUL.
    let mut body = vec![0x0b];
    body.extend(speech.language.to_le_bytes());
    body.extend(speaker.to_le_bytes());
    body.extend((u32::try_from(speech.speaker_name.len()).unwrap() + 1).to_le_bytes());
    body.extend(speech.speaker_name.as_bytes());
    body.push(0);
    body.extend(player.to_le_bytes());
    body.extend((u32::try_from(speech.text.len()).unwrap() + 1).to_le_bytes());
    body.extend(speech.text.as_bytes());
    body.extend([0, 0]);
    body
}

#[test]
fn event_ai_speech_is_one_stock_monster_say_with_inline_identity_and_no_server_emote() {
    let (world, mut projection, speech) = speech_boundary();
    let speaker = projection.npc_guid(speech.callback.npc).unwrap();
    let packets = projection.advance(&world).unwrap();
    let delivered = chats(&packets);
    assert_eq!(delivered.len(), 1);
    let chat = delivered[0];
    assert_eq!(chat.chat_type, 0x0b);
    assert_eq!(chat.language, speech.language);
    assert_eq!(chat.sender_guid, speaker);
    assert_eq!(chat.target_guid, projection.player_guid());
    assert_eq!(
        chat.sender_name.as_deref(),
        Some(speech.speaker_name.as_str())
    );
    assert_eq!(chat.text, speech.text);
    assert_eq!(chat.channel, None);
    assert_eq!(chat.chat_tag, 0);
    assert!(!packets.iter().any(|event| matches!(
        event,
        SessionEvent::Notification { .. } | SessionEvent::Emote { .. }
    )));
    let (packet, tail) = p::parse_server_with_tail(
        p::opcode::SMSG_MESSAGECHAT,
        &source_packet(&speech, speaker, projection.player_guid()),
    )
    .unwrap();
    assert_eq!(tail, 0);
    let decoded = benilla_protocol::decode(packet);
    assert_eq!(chats(&decoded), delivered);
    assert!(matches!(
        projection.advance(&world),
        Err(Error::StaleProjection)
    ));
}

#[test]
fn event_ai_utf8_locales_fallback_and_common_match_the_existing_wire_parser() {
    let (world, projection, original) = speech_boundary();
    let catalog = world
        .profile()
        .source_visitors
        .as_ref()
        .unwrap()
        .event_ai()
        .unwrap()
        .catalog();
    let source = catalog
        .events(original.callback.npc.spawn)
        .unwrap()
        .into_iter()
        .find(|event| event.event_id() == original.event_id)
        .unwrap();
    let mut languages = BTreeSet::new();
    let mut saw_utf8 = false;
    for (slot, broadcast) in source
        .text_ids()
        .into_iter()
        .enumerate()
        .filter(|(_, id)| *id > 0)
    {
        assert_eq!(
            catalog
                .localized(
                    original.callback.npc.spawn,
                    broadcast,
                    n::EventAiLocale::DeDe
                )
                .unwrap(),
            catalog
                .localized(
                    original.callback.npc.spawn,
                    broadcast,
                    n::EventAiLocale::EnUs
                )
                .unwrap(),
            "an absent translation retains the default inline text and name",
        );
        for locale in [
            n::EventAiLocale::EnUs,
            n::EventAiLocale::FrFr,
            n::EventAiLocale::DeDe,
        ] {
            let text = catalog
                .localized(original.callback.npc.spawn, broadcast, locale)
                .unwrap();
            let mut speech = original.clone();
            speech.broadcast_id = broadcast as u32;
            speech.text_slot = slot as u8;
            speech.locale = locale;
            speech.speaker_name = text.speaker_name;
            speech.text = text.text;
            speech.language = text.language;
            languages.insert(speech.language);
            saw_utf8 |= !speech.text.is_ascii() || !speech.speaker_name.is_ascii();
            let speaker = projection.npc_guid(speech.callback.npc).unwrap();
            let projected = SessionEvent::Chat(message(&speech, speaker, projection.player_guid()));
            let (parsed, tail) = p::parse_server_with_tail(
                p::opcode::SMSG_MESSAGECHAT,
                &source_packet(&speech, speaker, projection.player_guid()),
            )
            .unwrap();
            assert_eq!(tail, 0);
            let decoded = benilla_protocol::decode(parsed);
            assert_eq!(chats(&[projected]), chats(&decoded));
        }
    }
    assert_eq!(languages, BTreeSet::from([0, 7]));
    assert!(
        saw_utf8,
        "authored locale must exercise byte counts, not character counts"
    );
}

#[test]
fn event_ai_forged_callback_or_localized_content_rejects_before_delivery() {
    let (world, projection, original) = speech_boundary();
    let speaker = projection.npc_guid(original.callback.npc).unwrap();
    let mutations: Vec<n::EventAiSpeech> = (0..13)
        .map(|case| {
            let mut speech = original.clone();
            match case {
                0 => speech.callback.world[0] ^= 1,
                1 => speech.callback.tick += 1,
                2 => speech.callback.now_ms += 1,
                3 => speech.callback.moment = n::InteractionMoment::ReadOnly,
                4 => speech.callback.invoker.owner[0] ^= 1,
                5 => speech.recipient.character[0] ^= 1,
                6 => speech.callback.catalog[0] ^= 1,
                7 => speech.event_id = 0,
                8 => speech.text_slot = 3,
                9 => speech.text.push('!'),
                10 => speech.speaker_name.push('!'),
                11 => speech.language = 33,
                12 => {
                    speech.locale = if speech.locale == n::EventAiLocale::EnUs {
                        n::EventAiLocale::FrFr
                    } else {
                        n::EventAiLocale::EnUs
                    }
                }
                _ => unreachable!(),
            }
            speech
        })
        .collect();
    for speech in mutations {
        assert!(matches!(
            project(&projection, &world, &speech, speaker),
            Err(Error::StaleProjection)
        ));
    }
}

#[test]
fn event_ai_late_projection_failure_discards_staged_chat_and_retries_once() {
    let (world, mut projection, speech) = speech_boundary();
    let previous = projection.previous.clone();
    let digest = projection.previous_digest;
    let represented = projection.npcs.clone();
    let other = *represented
        .keys()
        .find(|id| **id != speech.callback.npc)
        .unwrap();
    projection.npcs.remove(&other);
    let broken = projection.npcs.clone();
    let mut delivered = Vec::new();
    // Speaker mapping succeeds; the missing neutral owner is caught by finish
    // after speech was staged locally. Neither local cursor nor caller queue commits.
    let error = projection.advance(&world).unwrap_err();
    assert!(matches!(error, Error::StaleProjection));
    assert_eq!(projection.previous, previous);
    assert_eq!(projection.previous_digest, digest);
    assert_eq!(projection.npcs, broken);
    assert!(chats(&delivered).is_empty());
    projection.npcs = represented;
    delivered.extend(projection.advance(&world).unwrap());
    assert_eq!(chats(&delivered).len(), 1);
    assert!(matches!(
        projection.advance(&world),
        Err(Error::StaleProjection)
    ));
    assert_eq!(chats(&delivered).len(), 1);
}

#[test]
fn event_ai_historical_replay_and_checkpoint_bootstrap_do_not_emit_old_chat() {
    let (world, _, _) = speech_boundary();
    let mut replay = fixture::world(SPEAKING_SEED);
    replay.replay(world.records()).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), world.checkpoint().unwrap());
    for restored in [
        replay,
        n::NativeWorld::restore_synthetic_component_test(
            fixture::profile(),
            world.checkpoint().unwrap(),
        )
        .unwrap(),
    ] {
        let mut projection =
            NativeProjection::new(&restored, restored.state().player, &Catalog).unwrap();
        assert!(chats(&projection.initial_events(&restored).unwrap()).is_empty());
        assert!(matches!(
            projection.advance(&restored),
            Err(Error::StaleProjection)
        ));
    }
}

#[test]
fn event_ai_speaker_mapping_never_relabels_an_old_generation_after_respawn() {
    let (world, projection, speech) = speech_boundary();
    let old = speech.callback.npc;
    let new = NpcId {
        generation: old.generation + 1,
        ..old
    };
    let mut frame = motion::Frame::new(&projection.npcs, &projection.previous, &world).unwrap();
    let old_guid = frame.guid(&old).unwrap();
    assert!(matches!(
        project(&projection, &world, &speech, old_guid).unwrap(),
        SessionEvent::Chat(_)
    ));
    assert!(
        matches!(frame.remove(old).unwrap(), SessionEvent::ObjectDestroyed(guid) if guid == old_guid)
    );
    let new_guid = npc_guid(&world, new).unwrap();
    frame.spawn(new, new_guid).unwrap();
    assert_ne!(new_guid, old_guid);
    assert!(matches!(frame.guid(&old), Err(Error::StaleProjection)));
    assert_eq!(frame.guid(&new).unwrap(), new_guid);
}

#[test]
fn event_ai_lethal_first_hit_keeps_create_speech_damage_destroy_and_respawn_guids_ordered() {
    for corpse_decay_ms in [0, 5000] {
        let mut profile = fixture::profile();
        profile.npcs[0].max_health = 1;
        profile.npcs[0].ai.corpse_decay_ms = corpse_decay_ms;
        let mut world = n::NativeWorld::synthetic_component_test(
            profile,
            [8; 16],
            n::synthetic_fixture::player(),
            SPEAKING_SEED,
        )
        .unwrap();
        let mut projection = NativeProjection::new(&world, world.state().player, &Catalog).unwrap();
        let mut all = projection.initial_events(&world).unwrap();
        let mut authority = fixture::Authority::default();
        authority.base.visibility = Some(ai::movement::Check::Failed);
        authority.base.base.base.critical_chance = Some(0);
        authority.base.base.base.resist_chance = Some(0);
        let mut attacked = BTreeSet::new();
        let mut speakers = Vec::new();
        for tick in 1..=100 {
            let current = &world.state().npcs[&2];
            let input =
                (tick >= 3 && current.ai.phase() == ai::Phase::Idle && attacked.insert(current.id))
                    .then(|| n::synthetic_fixture::cast(&world, 60001));
            world.step(tick, input, &mut authority).unwrap();
            let died = world.last_tick().unwrap().events.iter().any(|event| {
                matches!(
                    event,
                    n::Event::Npc {
                        effect: ai::Effect::Died,
                        ..
                    }
                )
            });
            let packets = projection.advance(&world).unwrap();
            if died {
                let chat = chats(&packets);
                assert_eq!(chat.len(), 1);
                let speaker = chat[0].sender_guid;
                assert!(!speakers.contains(&speaker));
                speakers.push(speaker);
                let speech_at = packets
                    .iter()
                    .position(|event| matches!(event, SessionEvent::Chat(_)))
                    .unwrap();
                let damage_at = packets
                    .iter()
                    .position(|event| matches!(event, SessionEvent::SpellDamageLog(_)))
                    .unwrap();
                assert!(speech_at < damage_at);
                let create_at = all
                    .iter()
                    .position(|event| {
                        matches!(event,
                    SessionEvent::ObjectCreate { guid, .. } if *guid == speaker)
                    })
                    .unwrap();
                assert!(create_at < all.len() + speech_at);
                if corpse_decay_ms == 0 {
                    let destroy_at = packets
                        .iter()
                        .position(|event| {
                            matches!(event,
                        SessionEvent::ObjectDestroyed(guid) if *guid == speaker)
                        })
                        .unwrap();
                    assert!(damage_at < destroy_at);
                    assert_eq!(projection.npc_guid(world.state().npcs[&2].id), None);
                } else {
                    assert!(!packets
                        .iter()
                        .any(|event| matches!(event, SessionEvent::ObjectDestroyed(_))));
                    assert_eq!(
                        projection.npc_guid(world.state().npcs[&2].id),
                        Some(speaker)
                    );
                }
            } else {
                assert!(chats(&packets).is_empty());
            }
            all.extend(packets);
            if speakers.len() == if corpse_decay_ms == 0 { 2 } else { 1 } {
                break;
            }
        }
        assert_eq!(speakers.len(), if corpse_decay_ms == 0 { 2 } else { 1 });
        assert_eq!(chats(&all).len(), speakers.len());
    }
}

#[test]
fn event_ai_evaluation_without_a_listener_never_becomes_system_or_monster_chat() {
    let mut world = fixture::world(SPEAKING_SEED);
    let mut projection = NativeProjection::new(&world, world.state().player, &Catalog).unwrap();
    let mut authority = fixture::Authority {
        outside: true,
        ..Default::default()
    };
    let mut evaluated = false;
    for tick in 1..=4 {
        world.step(tick, None, &mut authority).unwrap();
        let record = world.last_tick().unwrap();
        evaluated |= record
            .events
            .iter()
            .any(|event| matches!(event, n::Event::EventAiEvaluation(_)));
        assert!(!record
            .events
            .iter()
            .any(|event| matches!(event, n::Event::EventAiSpeech(_))));
        let events = projection.advance(&world).unwrap();
        assert!(chats(&events).is_empty());
        assert!(!events
            .iter()
            .any(|event| matches!(event, SessionEvent::Notification { .. })));
    }
    assert!(evaluated);
    assert!(world.state().rng_counter >= 2);
}
