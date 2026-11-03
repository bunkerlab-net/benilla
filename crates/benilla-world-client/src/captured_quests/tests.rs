// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
fn authored_text(value: &str) -> QuestText {
    QuestText {
        title: value.into(),
        details: "Original detail $N$B".into(),
        objectives: "Original objective".into(),
        end: String::new(),
        request: "Original request".into(),
        offer: "Original offer".into(),
        objective: std::array::from_fn(|_| String::new()),
    }
}
fn source(gameplay: &q::CompiledOrdinaryQuests, map: u32) -> CapturedQuestPresentation {
    CapturedQuestPresentation {
        source: gameplay.source(),
        quests: Some(
            gameplay
                .quests()
                .iter()
                .map(|q| CapturedQuestText {
                    quest: q.id,
                    zone_or_sort: 9,
                    point_map: map,
                    point_x_bits: 0,
                    point_y_bits: 0,
                    point_option: 0,
                    text: authored_text(&format!("Original quest {}", q.id)),
                    presentation: NativeQuestPresentation {
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
            text: "Original greeting $N$B".into(),
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
        locale_mapping: vec![(-1, EventAiLocale::EnUs), (0, EventAiLocale::FrFr)],
    }
}
fn npc_option() -> CapturedNpcTextOption {
    CapturedNpcTextOption {
        probability_bits: 0,
        text0: String::new(),
        text1: String::new(),
        language: 0,
        emotes: [(0, 0); 3],
        broadcast: 0,
    }
}
#[test]
fn captured_text_and_greetings_bind_exactly_and_empty_locale_falls_back() {
    let (profile, gameplay) = n::synthetic_quest_giver_fixture::profile();
    let mut raw = source(&gameplay, profile.map_id);
    let mut localized = authored_text("");
    localized.details = "Texte original $N$B".into();
    raw.quest_locales
        .as_mut()
        .unwrap()
        .push(CapturedQuestLocale {
            quest: 783,
            locale: EventAiLocale::FrFr,
            text: localized,
        });
    raw.greeting_locales
        .as_mut()
        .unwrap()
        .push(CapturedGreetingLocale {
            entry: 823,
            kind: 0,
            locale: EventAiLocale::FrFr,
            text: String::new(),
        });
    let out = raw.compile(&gameplay, 0).unwrap();
    assert_eq!(
        out.quest_metadata(783).unwrap().template.title,
        "Original quest 783"
    );
    assert_eq!(
        out.quest_metadata(783).unwrap().template.details,
        "Texte original $N$B"
    );
    assert_eq!(
        out.native_quest_greeting(823).unwrap().text,
        "Original greeting $N$B"
    );
    assert!(!out.native_quest_greeting(197).unwrap().has_greeting_row);
    assert_eq!(out.native_quest_greeting(197).unwrap().text, "");
    assert_eq!(
        out.native_quest_presentation(783).unwrap().complete_emote,
        1
    );
    assert_ne!(
        out.manifest_digest(),
        raw.compile(&gameplay, -1).unwrap().manifest_digest()
    );
}
#[test]
fn captured_fallback_uses_guid_text0_then_text1_and_broadcast_gender() {
    let (profile, gameplay) = n::synthetic_quest_giver_fixture::profile();
    let mut raw = source(&gameplay, profile.map_id);
    raw.spawns.as_mut().unwrap()[0].npc_gossip = Some(Some(123));
    raw.npc_text_keys.push(123);
    let mut options = std::array::from_fn(|_| npc_option());
    options[0].text1 = "Original alternate".into();
    options[0].emotes[0] = (40, 1);
    raw.npc_text = Some(vec![CapturedNpcText { id: 123, options }]);
    let out = raw.compile(&gameplay, -1).unwrap();
    assert_eq!(
        out.native_quest_greeting(197).unwrap().text,
        "Original alternate"
    );
    assert_eq!(out.native_quest_greeting(197).unwrap().emote_delay, 40);
    raw.npc_text.as_mut().unwrap()[0].options[0].broadcast = 456;
    raw.spawns.as_mut().unwrap()[0].gender = 1;
    raw.broadcasts = Some(vec![n::EventAiBroadcastSource {
        id: 456,
        text: Some("Original male".into()),
        text1: Some("Original female".into()),
        chat_type: 0,
        language: 0,
        condition_id: 0,
        emotes_id: 0,
        flags: 0,
        sounds: [0; 2],
        emote_ids: [0; 3],
        emote_delays: [0; 3],
        verified_build: 0,
    }]);
    assert_eq!(
        raw.compile(&gameplay, -1)
            .unwrap()
            .native_quest_greeting(197)
            .unwrap()
            .text,
        "Original female"
    );
}
#[test]
fn captured_unknown_lookups_emotes_delays_and_changed_binding_refuse() {
    let (profile, gameplay) = n::synthetic_quest_giver_fixture::profile();
    let raw = source(&gameplay, profile.map_id);
    let mut bad = raw.clone();
    bad.emotes = None;
    assert_eq!(bad.compile(&gameplay, -1).unwrap_err().field, "Emotes");
    let mut bad = raw.clone();
    bad.emotes = Some(vec![0]);
    assert_eq!(
        bad.compile(&gameplay, -1).unwrap_err().field,
        "questgiver_greeting"
    );
    let mut bad = raw.clone();
    bad.spawns.as_mut().unwrap()[0].npc_gossip = None;
    assert_eq!(bad.compile(&gameplay, -1).unwrap_err().field, "npc_gossip");
    let mut bad = raw.clone();
    bad.source = [0; 32];
    assert_eq!(bad.compile(&gameplay, -1).unwrap_err().field, "source");
    let mut bad = raw.clone();
    bad.quest_locales = None;
    assert_eq!(
        bad.compile(&gameplay, -1).unwrap_err().field,
        "locales_quest"
    );
    let mut bad = raw.clone();
    bad.quests.as_mut().unwrap()[0]
        .presentation
        .complete_emote_delay = 1;
    assert_eq!(
        bad.compile(&gameplay, -1).unwrap_err().reason,
        "unsupported_packet_shape"
    );
    let mut bad = raw.clone();
    bad.quests.as_mut().unwrap()[0].text.title.push('\0');
    assert_eq!(bad.compile(&gameplay, -1).unwrap_err().field, "text");
    assert_eq!(raw.compile(&gameplay, 2).unwrap_err().field, "locale");
}

#[test]
fn differing_same_entry_guid_fallbacks_refuse_entry_keyed_projection() {
    let (_, gameplay) = n::synthetic_quest_giver_fixture::profile();
    let mut givers = gameplay.givers().to_vec();
    let mut extra = givers[0].clone();
    extra.spawn = 5;
    extra.database_guid = 5;
    givers.push(extra);
    let input = q::CapturedOrdinaryQuests {
        source: gameplay.source(),
        quests: Some(gameplay.quests().to_vec()),
        starters: Some(gameplay.relations().starters.clone()),
        finishers: Some(gameplay.relations().finishers.clone()),
        givers: Some(givers),
    };
    let dependencies = q::QuestDependencySource {
        source: gameplay.source(),
        mechanics: None,
        scripts: Some(vec![]),
        gossip: Some(
            [197, 823]
                .map(|entry| q::CapturedGossipScope {
                    entry,
                    menu: 0,
                    options: vec![],
                })
                .to_vec(),
        ),
        allowed_areas: vec![9],
        global_spell_areas: Some(vec![]),
        item_keys: vec![],
        item_deletion_effects: Some(vec![]),
    }
    .resolve(&input)
    .unwrap();
    let gameplay = input
        .compile(gameplay.relationships(), &dependencies)
        .unwrap();
    let mut raw = source(&gameplay, 7);
    raw.spawns.as_mut().unwrap()[2].npc_gossip = Some(Some(123));
    raw.npc_text_keys.push(123);
    let mut options = std::array::from_fn(|_| npc_option());
    options[0].text0 = "Different GUID fallback".into();
    raw.npc_text = Some(vec![CapturedNpcText { id: 123, options }]);
    assert_eq!(
        raw.compile(&gameplay, -1).unwrap_err().reason,
        "incompatible_spawn_projection"
    );
}
