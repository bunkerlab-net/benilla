// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored EventAI/camera component closures, never installed source or admission.
use super::{synthetic_fixture as f, synthetic_two_owner_fixture as visitors, *};

pub const EVENT: u32 = 91001;
pub const TEXT_COMMON: u32 = 71001;
pub const TEXT_UNIVERSAL: u32 = 71002;
/// Default chance100 guarantees an action; use profile_with_chance to test failures.
pub fn profile() -> Profile {
    profile_with_chance(100)
}
pub fn profile_with_chance(chance: u32) -> Profile {
    profile_rows(chance, false)
}
/// Two applicable source rows prove entry-before-negative-GUID iteration.
pub fn profile_with_entry_and_guid() -> Profile {
    profile_rows(100, true)
}
fn profile_rows(chance: u32, with_guid: bool) -> Profile {
    install_rows(visitors::profile(), chance, with_guid)
}
/// Compose authored AGGRO rows with an already owned bounded creature profile.
/// Exact generation addon/ExtraFlags ownership is preserved; no source importer.
pub fn install(profile: Profile, chance: u32) -> Profile {
    install_rows(profile, chance, false)
}
fn install_rows(mut profile: Profile, chance: u32, with_guid: bool) -> Profile {
    let speaker = profile.npcs[0].ai.creature_id;
    let speaker_entry = profile.npcs[0].entry;
    let event = EventAiEventSource {
        id: EVENT,
        creature_id: profile.npcs[0].entry as i32,
        event_type: 4,
        inverse_phase_mask: 0,
        chance,
        flags: 0,
        parameters: [0; 6],
        actions: [
            EventAiActionSource {
                action_type: 1,
                parameters: [TEXT_UNIVERSAL as i32, TEXT_COMMON as i32, 0],
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
        comment: "Authored random combat line".into(),
    };
    let input = EventAiInput {
        owners: EventAiRows::Complete(
            profile
                .npcs
                .iter()
                .map(|n| EventAiOwnerSource {
                    spawn_id: n.ai.creature_id,
                    entry: n.entry,
                    database_guid: 99000 + n.ai.creature_id as u32,
                    gender: 0,
                    name: n.name.clone(),
                    name_locales: EventAiRows::Complete(vec![EventAiNameLocale {
                        locale: EventAiLocale::FrFr,
                        name: "Locuteur rédigé".into(),
                    }]),
                    entry_events: EventAiRows::Complete(if n.entry == speaker_entry {
                        vec![event.clone()]
                    } else {
                        vec![]
                    }),
                    guid_events: EventAiRows::Complete(
                        if with_guid && n.ai.creature_id == speaker {
                            let mut guid = event.clone();
                            guid.id += 1;
                            guid.creature_id = -(99000 + n.ai.creature_id as i32);
                            vec![guid]
                        } else {
                            vec![]
                        },
                    ),
                })
                .collect(),
        ),
        broadcasts: EventAiRows::Complete(
            [TEXT_COMMON, TEXT_UNIVERSAL]
                .into_iter()
                .map(|id| EventAiBroadcastSource {
                    id: id as i32,
                    text: Some(format!("Authored line {id}")),
                    text1: None,
                    chat_type: 0,
                    language: if id == TEXT_COMMON { 7 } else { 0 },
                    condition_id: 0,
                    emotes_id: 0,
                    flags: 1,
                    sounds: [0; 2],
                    emote_ids: [0; 3],
                    emote_delays: [0; 3],
                    verified_build: 0,
                })
                .collect(),
        ),
        locales: EventAiRows::Complete(vec![EventAiBroadcastLocale {
            id: TEXT_COMMON as i32,
            locale: "frFR".into(),
            text: Some("Réplique française".into()),
            text1: None,
            verified_build: 0,
        }]),
    };
    let EventAiRows::Complete(owners) = input.owners else {
        unreachable!()
    };
    let mut selectors = std::collections::BTreeSet::new();
    let mut events = std::collections::BTreeMap::new();
    let owners = owners
        .into_iter()
        .map(|owner| {
            selectors.insert(owner.entry as i32);
            selectors.insert(-(owner.database_guid as i32));
            for rows in [owner.entry_events, owner.guid_events] {
                let EventAiRows::Complete(rows) = rows else {
                    unreachable!()
                };
                for row in rows {
                    events.insert(row.id, row);
                }
            }
            EventAiCapturedOwner {
                spawn_id: owner.spawn_id,
                database_guid: owner.database_guid,
                entry: owner.entry,
                captured_entry: owner.entry,
                gender: owner.gender,
                name: owner.name,
                name_locales: owner.name_locales,
            }
        })
        .collect();
    let captured = EventAiCapturedInput {
        owners: EventAiRows::Complete(owners),
        selectors: EventAiRows::Complete(selectors.into_iter().collect()),
        events: EventAiRows::Complete(events.into_values().collect()),
        broadcasts: input.broadcasts,
        locales: input.locales,
    };
    let catalog = EventAiRuntimeCatalog::validate_sources(
        captured.compile().unwrap(),
        EventAiRuntimeSource {
            session: EventAiSessionPolicy::SoloSelfCamera,
            locale: EventAiLocale::FrFr,
            configured_say_range: f::real(25.0),
            visibility_distance: f::real(90.0),
            owners: EventAiRows::Complete(
                profile
                    .npcs
                    .iter()
                    .map(|n| EventAiRuntimeOwnerSource {
                        spawn_id: n.ai.creature_id,
                        entry: n.entry,
                        civilian: 0,
                        extra_flags: profile
                            .creature_generations
                            .as_ref()
                            .and_then(|catalog| catalog.source(n.ai.creature_id))
                            .map_or(0, |source| source.input.template.extra_flags),
                        spell_list_id: 0,
                        resolved_spell_list_id: n.entry * 100,
                        resolved_spells: EventAiRows::Complete(vec![]),
                    })
                    .collect(),
            ),
        },
    )
    .unwrap();
    let owner_order = profile.source_visitors.as_ref().map_or_else(
        || profile.npcs.iter().map(|npc| npc.ai.creature_id).collect(),
        |catalog| catalog.owner_order().to_vec(),
    );
    let mut input = visitors::visitor_input(&profile, owner_order);
    let SourceVisitorRows::Complete(owners) = &mut input.owners else {
        unreachable!()
    };
    for owner in owners.iter_mut() {
        if let Some(source) = profile
            .creature_generations
            .as_ref()
            .and_then(|catalog| catalog.source(owner.spawn_id))
        {
            owner.addon_closure = SourceVisitorAddonClosure::GenerationSource {
                source_identity: source.identity(),
            };
        }
    }
    for owner in owners
        .iter_mut()
        .filter(|owner| owner.entry == speaker_entry)
    {
        owner.ai_name = "EventAI".into();
        owner.entry_events = SourceVisitorRows::Complete(vec![EVENT]);
    }
    if with_guid {
        owners[0].guid_events = SourceVisitorRows::Complete(vec![EVENT + 1]);
    }
    profile.source_visitors =
        Some(SourceVisitorCatalog::validate_sources_with_event_ai(input, catalog).unwrap());
    profile.validate().unwrap();
    profile
}
pub fn world(seed: u64) -> NativeWorld {
    NativeWorld::synthetic_component_test(profile(), [8; 16], f::player(), seed).unwrap()
}
#[derive(Default)]
pub struct Authority {
    pub base: visitors::CompleteAuthority,
    pub outside: bool,
    pub fail_audience: bool,
    pub audience_override: Option<EventAiAudienceFacts>,
    pub queries: Vec<AuthorityRequest>,
}
impl super::Authority for Authority {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        let Query::EventAiAudience(query) = &request.query else {
            return self.base.resolve(request);
        };
        self.queries.push(request.clone());
        if self.fail_audience {
            return Err(NativeError::AuthorityUnavailable(
                "authored camera membership fault",
            ));
        }
        let facts =
            self.audience_override
                .clone()
                .unwrap_or_else(|| EventAiAudienceFacts::Complete {
                    request: query.clone(),
                    loaded_membership: ai::movement::Check::Passed,
                    world_camera: (!self.outside).then(|| Box::new(query.camera)),
                    not_visited: self.outside.then(|| Box::new(query.camera)),
                });
        Ok(AuthorityReply {
            request: request.clone(),
            facts: Facts::EventAiAudience(facts),
        })
    }
    fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
        self.base.observe_derived(reply)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composed_event_ai_preserves_existing_visitor_execution_order() {
        for order in [[2, 3], [3, 2]] {
            let profile = install(visitors::profile_with_order(order), 10);
            assert_eq!(
                profile.source_visitors.as_ref().unwrap().owner_order(),
                order
            );
            assert_eq!(
                profile
                    .event_ai()
                    .unwrap()
                    .catalog()
                    .events(2)
                    .unwrap()
                    .len(),
                1
            );
            assert!(profile
                .event_ai()
                .unwrap()
                .catalog()
                .events(3)
                .unwrap()
                .is_empty());
        }
    }
}
