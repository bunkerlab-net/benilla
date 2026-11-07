// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored component scenes only; no raw producer, installed scene or native admission.
use super::*;
use crate::native::{
    synthetic_combat_fixture, synthetic_fixture as f, synthetic_motion_fixture,
    synthetic_spell_fixture, synthetic_two_owner_fixture,
};

fn sources(profile: &Profile) -> EventAiInput {
    let mut input = crate::native::event_ai::tests::input();
    let EventAiRows::Complete(owners) = &mut input.owners else {
        unreachable!()
    };
    let template = owners[0].clone();
    *owners = profile
        .npcs
        .iter()
        .enumerate()
        .map(|(i, npc)| {
            let mut owner = template.clone();
            owner.spawn_id = npc.ai.creature_id;
            owner.entry = npc.entry;
            owner.database_guid = 99001 + i as u32;
            owner.name = npc.name.clone();
            let EventAiRows::Complete(events) = &mut owner.entry_events else {
                unreachable!()
            };
            events[0].id = 91001 + 10 * i as u32;
            events[0].creature_id = npc.entry as i32;
            let mut guid = events[0].clone();
            guid.id = 90001 + 10 * i as u32;
            guid.creature_id = -(owner.database_guid as i32);
            owner.guid_events = EventAiRows::Complete(vec![guid]);
            owner
        })
        .collect();
    input
}
fn configuration(profile: &Profile) -> EventAiRuntimeSource {
    EventAiRuntimeSource {
        session: EventAiSessionPolicy::SoloSelfCamera,
        locale: EventAiLocale::EnUs,
        configured_say_range: f::real(25.0),
        visibility_distance: f::real(100.0),
        owners: EventAiRows::Complete(
            profile
                .npcs
                .iter()
                .map(|npc| EventAiRuntimeOwnerSource {
                    spawn_id: npc.ai.creature_id,
                    entry: npc.entry,
                    civilian: 0,
                    extra_flags: 0,
                    spell_list_id: 0,
                    resolved_spell_list_id: npc.entry * 100,
                    resolved_spells: EventAiRows::Complete(vec![]),
                })
                .collect(),
        ),
    }
}
fn visitors(profile: &Profile) -> SourceVisitorInput {
    let mut input = synthetic_two_owner_fixture::visitor_input(
        profile,
        profile.npcs.iter().map(|npc| npc.ai.creature_id).collect(),
    );
    let SourceVisitorRows::Complete(owners) = &mut input.owners else {
        unreachable!()
    };
    for (i, owner) in owners.iter_mut().enumerate() {
        owner.ai_name = "EventAI".into();
        owner.entry_events = SourceVisitorRows::Complete(vec![91001 + 10 * i as u32]);
        owner.guid_events = SourceVisitorRows::Complete(vec![90001 + 10 * i as u32]);
    }
    input
}
fn selected(profile: &Profile) -> SourceVisitorCatalog {
    SourceVisitorCatalog::validate_sources_with_event_ai(
        visitors(profile),
        EventAiRuntimeCatalog::validate_sources(
            EventAiCatalog::validate_sources(sources(profile)).unwrap(),
            configuration(profile),
        )
        .unwrap(),
    )
    .unwrap()
}
fn select(profile: &mut Profile) {
    if profile.creature_capabilities.is_none() {
        profile.creature_capabilities = Some(
            CreatureCapabilityCatalog::validate_sources(
                synthetic_motion_fixture::creature_capability_sources(profile),
            )
            .unwrap(),
        );
    }
    profile.source_visitors = Some(selected(profile));
}
fn runtime_owner(source: &mut EventAiRuntimeSource) -> &mut EventAiRuntimeOwnerSource {
    let EventAiRows::Complete(owners) = &mut source.owners else {
        unreachable!()
    };
    &mut owners[0]
}
fn visitor_owner(input: &mut SourceVisitorInput) -> &mut SourceVisitorOwner {
    let SourceVisitorRows::Complete(owners) = &mut input.owners else {
        unreachable!()
    };
    &mut owners[0]
}

#[test]
fn explicit_runtime_selection_binds_complete_one_and_two_owner_scenes() {
    for mut profile in [
        synthetic_combat_fixture::profile(),
        synthetic_two_owner_fixture::profile(),
    ] {
        let original = profile.digest();
        let input = visitors(&profile);
        assert!(SourceVisitorCatalog::validate_sources(input).is_err());
        select(&mut profile);
        profile.validate().unwrap();
        assert_ne!(original, profile.digest());
        let visitors = profile.source_visitors.as_ref().unwrap();
        let runtime = visitors.event_ai().unwrap();
        assert_eq!(
            runtime.session_policy(),
            EventAiSessionPolicy::SoloSelfCamera
        );
        assert_eq!(runtime.locale(), EventAiLocale::EnUs);
        assert_eq!(runtime.say_range(), f::real(25.0));
        assert_eq!(runtime.visibility_distance(), f::real(100.0));
        assert_eq!(
            runtime.owner_ids().collect::<Vec<_>>(),
            visitors.owner_order()
        );
        assert_eq!(
            runtime
                .catalog()
                .events(2)
                .unwrap()
                .iter()
                .map(|row| row.event_id())
                .collect::<Vec<_>>(),
            vec![91001, 90001]
        );
    }
}

#[test]
fn runtime_configuration_requires_resolved_empty_spell_list_and_no_base_callbacks() {
    let profile = synthetic_combat_fixture::profile();
    let catalog = EventAiCatalog::validate_sources(sources(&profile)).unwrap();
    for case in 0..16 {
        let mut config = configuration(&profile);
        match case {
            0 => config.owners = EventAiRows::Missing,
            1 => config.owners = EventAiRows::Unknown,
            2 => config.owners = EventAiRows::Complete(vec![]),
            3 => runtime_owner(&mut config).spawn_id += 1,
            4 => runtime_owner(&mut config).entry += 1,
            5 => runtime_owner(&mut config).civilian = 1,
            6 => runtime_owner(&mut config).extra_flags = GUARD,
            7 => runtime_owner(&mut config).spell_list_id = 1,
            8 => runtime_owner(&mut config).resolved_spell_list_id = 0,
            9 => runtime_owner(&mut config).resolved_spells = EventAiRows::Missing,
            10 => runtime_owner(&mut config).resolved_spells = EventAiRows::Unknown,
            11 => runtime_owner(&mut config).resolved_spells = EventAiRows::Complete(vec![133]),
            12 => config.configured_say_range = f::real(-1.0),
            13 => config.visibility_distance = f::real(-1.0),
            14 => config.configured_say_range = f::real(500_000_000_000.0),
            _ => {
                let row = runtime_owner(&mut config).clone();
                config.owners = EventAiRows::Complete(vec![row.clone(), row]);
            }
        }
        assert!(
            EventAiRuntimeCatalog::validate_sources(catalog.clone(), config).is_err(),
            "case {case}"
        );
    }
    let mut zero = configuration(&profile);
    zero.configured_say_range = ai::Real::ZERO;
    zero.visibility_distance = ai::Real::ZERO;
    EventAiRuntimeCatalog::validate_sources(catalog, zero).unwrap();
}

#[test]
fn runtime_requires_actual_nonempty_male_default_for_gender_independence() {
    let profile = synthetic_combat_fixture::profile();
    for absent in [None, Some(String::new())] {
        let mut input = sources(&profile);
        let EventAiRows::Complete(broadcasts) = &mut input.broadcasts else {
            unreachable!()
        };
        broadcasts[0].text = absent;
        broadcasts[0].text1 = Some("Female-only authored text".into());
        let catalog = EventAiCatalog::validate_sources(input).unwrap();
        assert!(EventAiRuntimeCatalog::validate_sources(catalog, configuration(&profile)).is_err());
    }
    let mut observed = Vec::new();
    for gender in 0..=2 {
        let mut input = sources(&profile);
        let EventAiRows::Complete(owners) = &mut input.owners else {
            unreachable!()
        };
        owners[0].gender = gender;
        let EventAiRows::Complete(broadcasts) = &mut input.broadcasts else {
            unreachable!()
        };
        broadcasts[0].text1 = Some("Ignored female vector".into());
        let catalog = EventAiCatalog::validate_sources(input).unwrap();
        let runtime =
            EventAiRuntimeCatalog::validate_sources(catalog, configuration(&profile)).unwrap();
        observed.push(
            runtime
                .catalog()
                .localized(2, 71001, EventAiLocale::FrFr)
                .unwrap(),
        );
    }
    assert!(observed.windows(2).all(|pair| pair[0] == pair[1]));
}

#[test]
fn visitor_runtime_binding_rejects_missing_changed_reordered_or_unrepresented_events() {
    let profile = synthetic_two_owner_fixture::profile();
    let runtime = selected(&profile).event_ai().unwrap().clone();
    for case in 0..13 {
        let mut input = visitors(&profile);
        match case {
            0 => visitor_owner(&mut input).entry_events = SourceVisitorRows::Missing,
            1 => visitor_owner(&mut input).entry_events = SourceVisitorRows::Unknown,
            2 => visitor_owner(&mut input).entry_events = SourceVisitorRows::Complete(vec![]),
            3 => visitor_owner(&mut input).entry_events = SourceVisitorRows::Complete(vec![91002]),
            4 => {
                visitor_owner(&mut input).entry_events =
                    SourceVisitorRows::Complete(vec![91001, 91001])
            }
            5 => visitor_owner(&mut input).guid_events = SourceVisitorRows::Missing,
            6 => visitor_owner(&mut input).guid_events = SourceVisitorRows::Unknown,
            7 => visitor_owner(&mut input).guid_events = SourceVisitorRows::Complete(vec![]),
            8 => visitor_owner(&mut input).guid_events = SourceVisitorRows::Complete(vec![91001]),
            9 => visitor_owner(&mut input).ai_name = "NullAI".into(),
            10 => visitor_owner(&mut input).spawn_id += 10,
            11 => visitor_owner(&mut input).entry += 10,
            _ => {
                let SourceVisitorRows::Complete(owners) = &mut input.owners else {
                    unreachable!()
                };
                owners.pop();
                input.owner_order.pop();
            }
        }
        assert!(
            SourceVisitorCatalog::validate_sources_with_event_ai(input, runtime.clone()).is_err(),
            "case {case}"
        );
    }
    let mut input = visitors(&profile);
    input.owner_order.reverse();
    let reversed = SourceVisitorCatalog::validate_sources_with_event_ai(input, runtime).unwrap();
    assert_eq!(reversed.owner_order(), &[3, 2]);
}

#[test]
fn runtime_scene_flags_names_and_configuration_are_profile_identity_bound() {
    let mut profile = synthetic_combat_fixture::profile();
    select(&mut profile);
    let original = profile.digest();
    for case in 0..5 {
        let mut changed = profile.clone();
        let mut config = configuration(&changed);
        match case {
            0 => config.locale = EventAiLocale::FrFr,
            1 => config.configured_say_range = f::real(30.0),
            2 => config.visibility_distance = f::real(400.0),
            3 => runtime_owner(&mut config).extra_flags = 0x0008_0000,
            _ => changed.npcs[0].name.push('!'),
        }
        let runtime = EventAiRuntimeCatalog::validate_sources(
            EventAiCatalog::validate_sources(sources(&profile)).unwrap(),
            config,
        )
        .unwrap();
        if case < 4 {
            assert_ne!(
                runtime.digest(),
                profile
                    .source_visitors
                    .as_ref()
                    .unwrap()
                    .event_ai()
                    .unwrap()
                    .digest(),
                "case {case}"
            );
        }
        changed.source_visitors = Some(
            SourceVisitorCatalog::validate_sources_with_event_ai(visitors(&changed), runtime)
                .unwrap(),
        );
        assert_ne!(changed.digest(), original);
        if case < 3 {
            changed.validate().unwrap();
        } else {
            assert!(changed.validate().is_err());
        }
    }
}

#[test]
fn spell_authority_accepts_only_exact_selected_ai_name_and_event_vector() {
    let (mut profile, initial, mut source) = synthetic_spell_fixture::inputs();
    select(&mut profile);
    source.npcs[0].ai_name = "EventAI".into();
    source.npcs[0].event_scripts = SpellSourceRows::Complete(vec![91001, 90001]);
    profile
        .initialize_spell_authority(&initial, source.clone())
        .unwrap();
    profile.validate().unwrap();
    for case in 0..9 {
        let mut changed = source.clone();
        match case {
            0 => changed.npcs[0].ai_name.clear(),
            1 => changed.npcs[0].ai_name = "NullAI".into(),
            2 => changed.npcs[0].event_scripts = SpellSourceRows::Unknown,
            3 => changed.npcs[0].event_scripts = SpellSourceRows::Complete(vec![]),
            4 => changed.npcs[0].event_scripts = SpellSourceRows::Complete(vec![91001]),
            5 => changed.npcs[0].event_scripts = SpellSourceRows::Complete(vec![90001, 91001]),
            6 => {
                changed.npcs[0].event_scripts = SpellSourceRows::Complete(vec![91001, 90001, 90001])
            }
            7 => changed.npcs[0].maximum_level += 1,
            _ => changed.npcs[0].script_id = 1,
        }
        let before = profile.digest();
        assert!(
            profile
                .initialize_spell_authority(&initial, changed)
                .is_err(),
            "case {case}"
        );
        assert_eq!(profile.digest(), before);
    }
    let (mut unselected, initial, _) = synthetic_spell_fixture::inputs();
    assert!(unselected
        .initialize_spell_authority(&initial, source)
        .is_err());
}

#[test]
fn complete_empty_companion_events_and_default_ai_name_are_explicitly_supported() {
    let mut profile = synthetic_two_owner_fixture::profile();
    let mut input = sources(&profile);
    let EventAiRows::Complete(owners) = &mut input.owners else {
        unreachable!()
    };
    owners[1].entry_events = EventAiRows::Complete(vec![]);
    owners[1].guid_events = EventAiRows::Complete(vec![]);
    let mut selected = visitors(&profile);
    let SourceVisitorRows::Complete(owners) = &mut selected.owners else {
        unreachable!()
    };
    owners[1].ai_name.clear();
    owners[1].entry_events = SourceVisitorRows::Complete(vec![]);
    owners[1].guid_events = SourceVisitorRows::Complete(vec![]);
    let runtime = EventAiRuntimeCatalog::validate_sources(
        EventAiCatalog::validate_sources(input).unwrap(),
        configuration(&profile),
    )
    .unwrap();
    profile.source_visitors =
        Some(SourceVisitorCatalog::validate_sources_with_event_ai(selected, runtime).unwrap());
    profile.validate().unwrap();
    let runtime = profile
        .source_visitors
        .as_ref()
        .unwrap()
        .event_ai()
        .unwrap();
    assert_eq!(runtime.catalog().events(2).unwrap().len(), 2);
    assert!(runtime.catalog().events(3).unwrap().is_empty());
}

#[test]
fn bounded_nonempty_event_selection_requires_explicit_ai_name() {
    let profile = synthetic_two_owner_fixture::profile();
    let runtime = selected(&profile).event_ai().unwrap().clone();
    let mut input = visitors(&profile);
    visitor_owner(&mut input).ai_name.clear();
    assert!(SourceVisitorCatalog::validate_sources_with_event_ai(input, runtime).is_err());
}

fn expand_same_entry(profile: &mut Profile) {
    let template = profile.npcs[0].clone();
    let motion = profile.source_motion.as_ref().unwrap().npcs[&template.ai.creature_id].clone();
    profile.npcs = (2..9)
        .map(|spawn| {
            let mut npc = template.clone();
            npc.ai.creature_id = spawn;
            npc
        })
        .collect();
    profile.source_motion.as_mut().unwrap().npcs =
        (2..9).map(|spawn| (spawn, motion.clone())).collect();
    profile.creature_capabilities = Some(
        CreatureCapabilityCatalog::validate_sources(
            synthetic_motion_fixture::creature_capability_sources(profile),
        )
        .unwrap(),
    );
    profile.source_visitors = Some(
        SourceVisitorCatalog::validate_sources(synthetic_two_owner_fixture::visitor_input(
            profile,
            vec![8, 3, 5, 2, 7, 4, 6],
        ))
        .unwrap(),
    );
}
fn shared_selection(profile: &Profile, guid_events: bool) -> SourceVisitorCatalog {
    let mut input = sources(profile);
    let EventAiRows::Complete(owners) = &mut input.owners else {
        unreachable!()
    };
    let shared = owners[0].entry_events.clone();
    for owner in owners {
        owner.entry_events = shared.clone();
        if !guid_events {
            owner.guid_events = EventAiRows::Complete(vec![]);
        }
    }
    let runtime = EventAiRuntimeCatalog::validate_sources(
        EventAiCatalog::validate_sources(input).unwrap(),
        configuration(profile),
    )
    .unwrap();
    let mut input = visitors(profile);
    input.owner_order = vec![8, 3, 5, 2, 7, 4, 6];
    let SourceVisitorRows::Complete(owners) = &mut input.owners else {
        unreachable!()
    };
    for owner in owners {
        owner.entry_events = SourceVisitorRows::Complete(vec![91001]);
        if !guid_events {
            owner.guid_events = SourceVisitorRows::Complete(vec![]);
        }
    }
    SourceVisitorCatalog::validate_sources_with_event_ai(input, runtime).unwrap()
}
#[test]
fn seven_same_entry_runtime_owners_have_independent_generation_state() {
    let mut profile = synthetic_combat_fixture::profile();
    expand_same_entry(&mut profile);
    profile.source_visitors = Some(shared_selection(&profile, true));
    profile.validate().unwrap();
    let runtime = profile.event_ai().unwrap();
    for case in 0..4 {
        let mut config = configuration(&profile);
        let EventAiRows::Complete(owners) = &mut config.owners else {
            unreachable!()
        };
        match case {
            0 => owners[6].extra_flags = 0x0008_0000,
            1 => owners[6].spawn_id = 9,
            2 => owners[6].spawn_id = 2,
            _ => {
                owners.pop();
            }
        }
        assert!(
            EventAiRuntimeCatalog::validate_sources(runtime.catalog().clone(), config).is_err(),
            "case {case}"
        );
    }
    let mut state = EventAiState::new(&profile).unwrap().unwrap();
    let companion = state.owners[&3].clone();
    assert_eq!(state.owners[&2].holders[0], companion.holders[0]);
    assert_ne!(
        state.owners[&2].holders[1].event_id,
        companion.holders[1].event_id
    );
    state.owners.get_mut(&2).unwrap().next_occurrence = 5;
    assert_eq!(state.owners[&3], companion);
    state
        .respawn(
            &profile,
            crate::NpcId {
                spawn: 2,
                generation: 1,
            },
            1,
        )
        .unwrap();
    assert_eq!(state.owners[&2].id.generation, 1);
    assert_eq!(state.owners[&2].next_occurrence, 0);
    assert_eq!(state.owners[&3], companion);
    for spawn in 3..9 {
        assert_eq!(state.owners[&spawn].id.generation, 0);
        assert_eq!(state.owners[&spawn].next_occurrence, 0);
    }
}
#[test]
fn combat_and_spell_catalogs_cover_distinct_entries_and_each_spawn_closure() {
    let (mut profile, initial, mut source) = synthetic_spell_fixture::inputs();
    expand_same_entry(&mut profile);
    profile.source_visitors = Some(shared_selection(&profile, false));
    source.npcs[0].ai_name = "EventAI".into();
    source.npcs[0].event_scripts = SpellSourceRows::Complete(vec![91001]);
    profile
        .initialize_spell_authority(&initial, source.clone())
        .unwrap();
    profile.validate().unwrap();
    let row = NpcCombatAuthoritySource {
        entry: profile.npcs[0].entry,
        extra_flags: 0,
        static_flags1: 0,
        static_flags2: 0,
        static_flags3: 0,
        static_flags4: 0,
    };
    profile
        .initialize_combat_authority(vec![row.clone()])
        .unwrap();
    for case in 0..3 {
        let mut combat_rows = vec![row.clone()];
        let mut spell_rows = source.clone();
        match case {
            0 => {
                combat_rows.clear();
                spell_rows.npcs.clear();
            }
            1 => {
                combat_rows[0].entry += 1;
                spell_rows.npcs[0].entry += 1;
            }
            _ => {
                let mut foreign = row.clone();
                foreign.entry += 1;
                combat_rows.push(foreign);
                let mut foreign = spell_rows.npcs[0].clone();
                foreign.entry += 1;
                spell_rows.npcs.push(foreign);
            }
        }
        let before = profile.digest();
        assert!(
            profile.initialize_combat_authority(combat_rows).is_err(),
            "case {case}"
        );
        assert!(
            profile
                .initialize_spell_authority(&initial, spell_rows)
                .is_err(),
            "case {case}"
        );
        assert_eq!(profile.digest(), before);
    }
    let mut contradictory = profile.clone();
    contradictory.npcs[6].ai.level += 1;
    assert!(contradictory.validate().is_err());

    // The entry-keyed spell source cannot represent different GUID closures.
    // Matching the first owner must never silently select it for all owners.
    profile.source_visitors = Some(shared_selection(&profile, true));
    source.npcs[0].event_scripts = SpellSourceRows::Complete(vec![91001, 90001]);
    assert!(profile
        .initialize_spell_authority(&initial, source)
        .is_err());
}
