// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored component rows only; no installed world or runnable source capture.
use super::*;

fn event(id: u32, selector: i32) -> EventAiEventSource {
    EventAiEventSource {
        id,
        creature_id: selector,
        event_type: 4,
        inverse_phase_mask: 0,
        chance: 10,
        flags: 0,
        parameters: [0; 6],
        actions: [
            EventAiActionSource {
                action_type: 1,
                parameters: [71002, 71001, 0],
            },
            EventAiActionSource {
                action_type: 0,
                parameters: [0; 3],
            },
            EventAiActionSource {
                action_type: 0,
                parameters: [0; 3],
            },
        ],
        comment: "Authored random aggro text".into(),
    }
}
pub(super) fn input() -> EventAiInput {
    EventAiInput {
        owners: EventAiRows::Complete(vec![EventAiOwnerSource {
            spawn_id: 2,
            entry: 60001,
            database_guid: 99001,
            gender: 0,
            name: "Authored speaker".into(),
            name_locales: EventAiRows::Complete(vec![EventAiNameLocale {
                locale: EventAiLocale::FrFr,
                name: "Locuteur".into(),
            }]),
            entry_events: EventAiRows::Complete(vec![event(91001, 60001)]),
            guid_events: EventAiRows::Complete(vec![]),
        }]),
        broadcasts: EventAiRows::Complete(
            [71001, 71002]
                .into_iter()
                .map(|id| EventAiBroadcastSource {
                    id,
                    text: Some(format!("Authored line {id}")),
                    text1: Some(String::new()),
                    chat_type: 0,
                    language: if id == 71001 { 7 } else { 0 },
                    condition_id: 0,
                    emotes_id: 0,
                    flags: 1,
                    sounds: [0; 2],
                    emote_ids: [0; 3],
                    emote_delays: [0; 3],
                    verified_build: 31882,
                })
                .collect(),
        ),
        locales: EventAiRows::Complete(vec![EventAiBroadcastLocale {
            id: 71001,
            locale: "frFR".into(),
            text: Some("Une réplique".into()),
            text1: None,
            verified_build: 31882,
        }]),
    }
}
fn owner(input: &mut EventAiInput) -> &mut EventAiOwnerSource {
    let EventAiRows::Complete(owners) = &mut input.owners else {
        panic!()
    };
    &mut owners[0]
}
fn events(input: &mut EventAiInput) -> &mut Vec<EventAiEventSource> {
    let EventAiRows::Complete(events) = &mut owner(input).entry_events else {
        panic!()
    };
    events
}
fn broadcasts(input: &mut EventAiInput) -> &mut Vec<EventAiBroadcastSource> {
    let EventAiRows::Complete(rows) = &mut input.broadcasts else {
        panic!()
    };
    rows
}
#[test]
fn retained_metadata_locales_and_source_order() {
    let mut i = input();
    owner(&mut i).guid_events = EventAiRows::Complete(vec![event(90000, -99001)]);
    let c = EventAiCatalog::validate_sources(i).unwrap();
    let a = c.events(2).unwrap();
    assert_eq!(
        a.iter().map(AggroText::event_id).collect::<Vec<_>>(),
        vec![91001, 90000]
    );
    let encoded = serde_json::to_value(&c).unwrap();
    assert_eq!(encoded["broadcasts"]["71001"]["flags"], 1);
    assert_eq!(encoded["broadcasts"]["71001"]["verified_build"], 31882);
    assert_eq!(
        c.localized(2, 71001, EventAiLocale::FrFr).unwrap(),
        EventAiLocalizedText {
            speaker_name: "Locuteur".into(),
            text: "Une réplique".into(),
            language: 7,
        }
    );
    assert_eq!(
        c.localized(2, 71001, EventAiLocale::DeDe).unwrap().text,
        "Authored line 71001"
    );
    assert_ne!(
        c.digest(),
        EventAiCatalog::validate_sources(input()).unwrap().digest()
    );
}
#[test]
fn unknown_missing_duplicate_or_unselected_dependencies_fail_closed() {
    for rows in [EventAiRows::Unknown, EventAiRows::Missing] {
        let mut i = input();
        i.broadcasts = rows;
        assert!(EventAiCatalog::validate_sources(i).is_err());
    }
    let mut i = input();
    owner(&mut i).guid_events = EventAiRows::Unknown;
    assert!(EventAiCatalog::validate_sources(i).is_err());
    let mut i = input();
    i.locales = EventAiRows::Unknown;
    assert!(EventAiCatalog::validate_sources(i).is_err());
    let mut i = input();
    broadcasts(&mut i).pop();
    assert!(EventAiCatalog::validate_sources(i).is_err());
    let mut i = input();
    let row = broadcasts(&mut i)[0].clone();
    broadcasts(&mut i).push(row);
    assert!(EventAiCatalog::validate_sources(i).is_err());
    let mut i = input();
    broadcasts(&mut i)[0].id = 71003;
    assert!(EventAiCatalog::validate_sources(i).is_err());
    let mut i = input();
    events(&mut i)[0].creature_id = -99001;
    assert!(EventAiCatalog::validate_sources(i).is_err());
    let mut i = input();
    let row = events(&mut i)[0].clone();
    events(&mut i).push(row);
    assert!(EventAiCatalog::validate_sources(i).is_err());
}
#[test]
fn only_reviewed_event_and_broadcast_domain_is_executable() {
    let mut cases = Vec::new();
    let mut i = input();
    events(&mut i)[0].event_type = 0;
    cases.push(i);
    let mut i = input();
    events(&mut i)[0].flags = 1;
    cases.push(i);
    let mut i = input();
    events(&mut i)[0].inverse_phase_mask = 1;
    cases.push(i);
    let mut i = input();
    events(&mut i)[0].parameters[0] = 1;
    cases.push(i);
    let mut i = input();
    events(&mut i)[0].chance = 256;
    cases.push(i);
    let mut i = input();
    events(&mut i)[0].actions[0].parameters = [71002, 0, 71001];
    cases.push(i);
    let mut i = input();
    events(&mut i)[0].actions[1].action_type = 1;
    cases.push(i);
    let mut i = input();
    events(&mut i)[0].actions[0].parameters[0] = -71002;
    cases.push(i);
    let mut i = input();
    broadcasts(&mut i)[0].chat_type = 1;
    cases.push(i);
    let mut i = input();
    broadcasts(&mut i)[0].sounds[0] = 1;
    cases.push(i);
    let mut i = input();
    broadcasts(&mut i)[0].emote_ids[0] = 1;
    cases.push(i);
    let mut i = input();
    broadcasts(&mut i)[0].text = Some("bad\0text".into());
    cases.push(i);
    for i in cases {
        assert!(EventAiCatalog::validate_sources(i).is_err());
    }
}
#[test]
fn male_default_overrides_gender_before_localized_fallback() {
    let mut i = input();
    owner(&mut i).gender = 1;
    broadcasts(&mut i)[0].text1 = Some("Female default".into());
    let EventAiRows::Complete(rows) = &mut i.locales else {
        panic!()
    };
    rows[0].text1 = Some("Réplique féminine".into());
    let c = EventAiCatalog::validate_sources(i.clone()).unwrap();
    assert_eq!(
        c.localized(2, 71001, EventAiLocale::FrFr).unwrap().text,
        "Une réplique"
    );
    broadcasts(&mut i)[0].text = None;
    let c = EventAiCatalog::validate_sources(i.clone()).unwrap();
    assert_eq!(
        c.localized(2, 71001, EventAiLocale::FrFr).unwrap().text,
        "Réplique féminine"
    );
    assert_eq!(
        c.localized(2, 71001, EventAiLocale::DeDe).unwrap().text,
        "Female default"
    );
    let EventAiRows::Complete(rows) = &mut i.locales else {
        panic!()
    };
    rows[0].text1 = Some(String::new());
    let c = EventAiCatalog::validate_sources(i).unwrap();
    assert_eq!(
        c.localized(2, 71001, EventAiLocale::FrFr).unwrap().text,
        "Female default"
    );
}
#[test]
fn newer_raw_locales_survive_but_pinned_loader_ignores_them() {
    let mut i = input();
    let EventAiRows::Complete(rows) = &mut i.locales else {
        panic!()
    };
    for locale in ["esMX", "ptBR", "ruRU", "enGB", "enUS"] {
        rows.push(EventAiBroadcastLocale {
            id: 71001,
            locale: locale.into(),
            text: Some("Ignored newer locale text".into()),
            text1: None,
            verified_build: 31882,
        });
    }
    let c = EventAiCatalog::validate_sources(i).unwrap();
    for locale in ["esMX", "ptBR", "ruRU", "enGB", "enUS"] {
        assert_eq!(EventAiLocale::from_source_name(locale), EventAiLocale::EnUs);
        assert_eq!(
            c.localized(2, 71001, EventAiLocale::from_source_name(locale))
                .unwrap()
                .text,
            "Authored line 71001"
        );
        assert!(serde_json::to_string(&c).unwrap().contains(locale));
    }
}

#[test]
fn shared_entry_domain_rejects_contradictory_entry_events_or_database_guid() {
    for same_entry in [false, true] {
        let mut i = input();
        let mut other = owner(&mut i).clone();
        other.spawn_id = 3;
        if same_entry {
            other.database_guid += 1;
        } else {
            other.entry += 1;
        }
        other.entry_events = EventAiRows::Complete(vec![event(91002, other.entry as i32)]);
        let EventAiRows::Complete(owners) = &mut i.owners else {
            panic!()
        };
        owners.push(other);
        assert!(EventAiCatalog::validate_sources(i).is_err());
    }
}

#[test]
fn aggregate_serialized_bytes_are_bounded_before_cloning_rows() {
    // Each string fits its raw per-text bound; JSON escaping exceeds the total.
    let mut i = input();
    let EventAiRows::Complete(rows) = &mut i.locales else {
        panic!()
    };
    for index in 0..100 {
        rows.push(EventAiBroadcastLocale {
            id: 71001,
            locale: format!("x{index}"),
            text: Some("\u{1}".repeat(16 * 1024)),
            text1: None,
            verified_build: 0,
        });
    }
    assert_eq!(
        EventAiCatalog::validate_sources(i),
        Err(NativeError::TooLarge)
    );
}
#[test]
fn another_owners_broadcast_cannot_be_projected_for_this_owner() {
    let mut i = input();
    let mut other = owner(&mut i).clone();
    other.spawn_id = 3;
    other.entry = 60002;
    other.database_guid = 99002;
    let mut e = event(91002, 60002);
    e.actions[0].parameters = [71003, 0, 0];
    other.entry_events = EventAiRows::Complete(vec![e]);
    let EventAiRows::Complete(owners) = &mut i.owners else {
        panic!()
    };
    owners.push(other);
    let mut row = broadcasts(&mut i)[0].clone();
    row.id = 71003;
    broadcasts(&mut i).push(row);
    let c = EventAiCatalog::validate_sources(i).unwrap();
    assert_eq!(
        c.localized(2, 71003, EventAiLocale::EnUs),
        Err(NativeError::AuthorityBinding)
    );
    assert!(c.localized(3, 71003, EventAiLocale::EnUs).is_ok());
}

/// Shared SQL entry rows, with an independently selected GUID vector per owner.
pub(super) fn same_entry_input(count: usize) -> EventAiInput {
    let mut input = input();
    let template = owner(&mut input).clone();
    input.owners = EventAiRows::Complete(
        (0..count)
            .map(|index| {
                let mut owner = template.clone();
                owner.spawn_id = index as u64 + 2;
                owner.database_guid += index as u32;
                owner.gender = (index % 3) as u8;
                owner.guid_events = EventAiRows::Complete(vec![event(
                    92001 + index as u32,
                    -(owner.database_guid as i32),
                )]);
                owner
            })
            .collect(),
    );
    input
}
#[test]
fn seven_same_entry_owners_copy_shared_entry_then_independent_guid_vectors() {
    let catalog = EventAiCatalog::validate_sources(same_entry_input(7)).unwrap();
    for spawn in 2..9 {
        assert_eq!(
            catalog
                .events(spawn)
                .unwrap()
                .iter()
                .map(AggroText::event_id)
                .collect::<Vec<_>>(),
            vec![91001, 92001 + (spawn - 2) as u32]
        );
        assert_eq!(
            catalog.owner(spawn).unwrap().database_guid,
            99001 + (spawn - 2) as u32
        );
    }
}
#[test]
fn shared_entry_fields_and_global_event_id_collisions_fail_closed() {
    for case in 0..10 {
        let mut input = same_entry_input(7);
        let EventAiRows::Complete(owners) = &mut input.owners else {
            unreachable!()
        };
        let first = owners[0].clone();
        let other = &mut owners[6];
        match case {
            0 => other.name.push('!'),
            1 => other.name_locales = EventAiRows::Complete(vec![]),
            2 => other.entry_events = EventAiRows::Complete(vec![]),
            3 => {
                let EventAiRows::Complete(events) = &mut other.entry_events else {
                    unreachable!()
                };
                events[0].comment.push('!');
            }
            4 => {
                let EventAiRows::Complete(events) = &mut other.entry_events else {
                    unreachable!()
                };
                events[0].chance += 1;
            }
            5 => other.guid_events = first.guid_events,
            6 => {
                other.entry += 1;
                other.entry_events = EventAiRows::Complete(vec![event(91001, other.entry as i32)]);
            }
            7 => {
                other.guid_events =
                    EventAiRows::Complete(vec![event(91001, -(other.database_guid as i32))])
            }
            8 => other.database_guid = first.database_guid,
            _ => other.spawn_id = first.spawn_id,
        }
        assert!(
            EventAiCatalog::validate_sources(input).is_err(),
            "case {case}"
        );
    }
}
#[test]
fn shared_entry_vectors_keep_per_owner_event_and_aggregate_source_bounds() {
    let mut input = same_entry_input(7);
    let EventAiRows::Complete(owners) = &mut input.owners else {
        unreachable!()
    };
    for owner in owners {
        owner.entry_events = EventAiRows::Complete(
            (0..31)
                .map(|index| event(91001 + index, owner.entry as i32))
                .collect(),
        );
    }
    EventAiCatalog::validate_sources(input.clone()).unwrap();
    let EventAiRows::Complete(owners) = &mut input.owners else {
        unreachable!()
    };
    for owner in owners {
        let EventAiRows::Complete(events) = &mut owner.entry_events else {
            unreachable!()
        };
        events.push(event(91032, owner.entry as i32));
    }
    assert_eq!(
        EventAiCatalog::validate_sources(input),
        Err(NativeError::TooLarge)
    );

    let mut input = same_entry_input(256);
    let EventAiRows::Complete(owners) = &mut input.owners else {
        unreachable!()
    };
    for owner in owners {
        owner.entry_events = EventAiRows::Complete(
            (0..31)
                .map(|index| {
                    let mut row = event(91001 + index, owner.entry as i32);
                    row.comment = "𐀀".repeat(255);
                    row
                })
                .collect(),
        );
    }
    assert_eq!(
        EventAiCatalog::validate_sources(input),
        Err(NativeError::TooLarge)
    );
}
#[test]
fn event_ai_owner_bound_uses_the_world_limit() {
    let input = same_entry_input(crate::MAX_NPCS);
    EventAiCatalog::validate_sources(input).unwrap();
    assert_eq!(
        EventAiCatalog::validate_sources(same_entry_input(crate::MAX_NPCS + 1)),
        Err(NativeError::TooLarge)
    );
}
