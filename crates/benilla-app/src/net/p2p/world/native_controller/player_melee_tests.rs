// SPDX-License-Identifier: GPL-3.0-or-later
//! Exact app controller + filesystem, authored source-shaped fixtures only.
//! This does not open native entry or establish a graphical playable world.
use super::*;
use benilla_world_runtime::native::{
    synthetic_player_melee_fixture as melee_fixture, synthetic_spell_fixture,
    synthetic_two_owner_fixture::CompleteAuthority, Authority,
};

#[derive(Default)]
struct MeleeAuthority {
    scene: CompleteAuthority,
    spell: synthetic_spell_fixture::SpatialOnly,
}
impl Authority for MeleeAuthority {
    fn resolve(
        &mut self,
        request: &n::AuthorityRequest,
    ) -> Result<n::AuthorityReply, n::NativeError> {
        if matches!(
            request.query,
            n::Query::SourceSpell(_) | n::Query::SourceLocation(_)
        ) {
            self.spell.resolve(request)
        } else {
            self.scene.resolve(request)
        }
    }
    fn observe_derived(&mut self, reply: &n::AuthorityReply) -> Result<(), n::NativeError> {
        if matches!(reply.request.query, n::Query::SourceSpell(_)) {
            self.spell.observe_derived(reply)
        } else {
            self.scene.observe_derived(reply)
        }
    }
}
struct MeleeCatalog;
impl ClientCatalog for MeleeCatalog {
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
        matches!(id, 31 | 34 | 131 | 134)
    }
    fn contains_item_display(&self, id: u32) -> bool {
        id == 900002
    }
    fn item_metadata(&self, id: u32) -> Option<benilla_protocol::messages::ItemInfo> {
        Catalog.item_metadata(id)
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
    fn quest_metadata(&self, id: u32) -> Option<benilla_world_client::QuestMetadata> {
        Catalog.quest_metadata(id)
    }
    fn quest_greeting(&self, entry: u32) -> Option<String> {
        Catalog.quest_greeting(entry)
    }
    fn native_quest_greeting(
        &self,
        entry: u32,
    ) -> Option<benilla_world_client::NativeQuestGreeting> {
        Catalog.native_quest_greeting(entry)
    }
    fn native_quest_presentation(
        &self,
        id: u32,
    ) -> Option<benilla_world_client::NativeQuestPresentation> {
        Catalog.native_quest_presentation(id)
    }
}
fn enter(h: &Harness, open: bool, health: u32) -> NativeController {
    NativeController::enter_component_test(
        &h.root,
        &h.profile,
        &h.character,
        if open {
            Entry::Open { world: [8; 16] }
        } else {
            Entry::Create {
                world: [8; 16],
                seed: 42,
            }
        },
        melee_fixture::source_fireball_profile(health),
        &MeleeCatalog,
        MeleeAuthority::default(),
    )
    .unwrap()
}
fn guid(c: &NativeController, spawn: u64) -> u64 {
    c.projection
        .npc_guid(c.world.state().npcs[&spawn].id)
        .unwrap()
}
fn step(c: &mut NativeController) -> Pump {
    let out = c.pump(c.tick_duration());
    assert!(out.pause.is_none(), "{:?}", out.pause);
    assert_eq!(out.ticks, 1);
    assert_eq!(out.saved.as_ref().unwrap().tick, c.tick());
    out
}
fn reopen(h: &Harness, c: NativeController, health: u32) -> NativeController {
    let before = c.world.state_digest();
    drop(c);
    let next = enter(h, true, health);
    assert_eq!(next.world.state_digest(), before);
    next
}
#[test]
fn selected_mainhand_and_cast_stock_commands_survive_checkpoint_retry_reopen_and_kill_credit() {
    let h = Harness::new();
    let mut c = enter(&h, false, 100);
    assert!(c.world.profile().player_melee.is_some());
    c.receive(ClientCommand::QuestgiverAccept {
        npc: guid(&c, 3),
        quest: 70001,
    })
    .unwrap();
    step(&mut c);
    c.receive(ClientCommand::AttackSwing { guid: guid(&c, 2) })
        .unwrap();
    c.receive(ClientCommand::CastSpell {
        spell_id: 133,
        target: Some(guid(&c, 2)),
    })
    .unwrap();
    fs::write(
        h.path().join(".world-checkpoint.pending"),
        b"authored recoverable fault",
    )
    .unwrap();
    let paused = c.pump(c.tick_duration());
    assert_eq!(paused.ticks, 1);
    assert!(paused.events.is_empty());
    assert!(matches!(
        paused.pause.unwrap().kind,
        PauseKind::Checkpoint {
            requires_reopen: false
        }
    ));
    let committed = c.world.state_digest();
    let sequence = c.world.state().next_sequence;
    fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
    c.retry_checkpoint().unwrap();
    let published = c.pump(Duration::ZERO);
    assert!(published.pause.is_none());
    assert_eq!(c.world.state_digest(), committed);
    assert_eq!(c.world.state().next_sequence, sequence);
    assert_eq!(published.events.iter().filter(|e| matches!(e, SessionEvent::AttackStart { attacker, .. } if *attacker == c.player_guid())).count(), 1);
    assert!(!published
        .events
        .iter()
        .any(|e| matches!(e, SessionEvent::AttackerState(a) if a.attacker == c.player_guid())));
    assert_eq!(
        published
            .events
            .iter()
            .filter(|e| matches!(e, SessionEvent::SpellStart { .. }))
            .count(),
        1
    );
    c = reopen(&h, c, 100);
    let mut melee = false;
    let mut melee_with_aura = false;
    let mut kill_xp = 0;
    // This authored low-damage staff is not guaranteed to kill within thirty seconds.
    // Bound the encounter at one thousand real fixed steps, without advancing timers by hand.
    for _ in 0..1000 {
        let aura_before = c.world.state().npcs[&2].auras.contains_key(&133);
        let out = step(&mut c);
        let actual_melee = out
            .events
            .iter()
            .any(|e| matches!(e, SessionEvent::AttackerState(a) if a.attacker == c.player_guid()));
        melee |= actual_melee;
        melee_with_aura |=
            actual_melee && aura_before && c.world.state().npcs[&2].auras.contains_key(&133);
        kill_xp += out
            .events
            .iter()
            .filter(|e| matches!(e, SessionEvent::XpGain(x) if x.kill))
            .count();
        if c.world.state().npcs[&2].health == 0 {
            break;
        }
    }
    assert!(melee && melee_with_aura);
    assert_eq!(
        c.world.state().npcs[&2].health,
        0,
        "{:?}",
        c.world.state().npcs[&2]
    );
    assert_eq!(kill_xp, 1);
    assert_eq!(
        c.world
            .state()
            .quest_progression
            .as_ref()
            .unwrap()
            .quests
            .progress(70001)
            .unwrap()
            .creatures[0],
        1
    );
    c = reopen(&h, c, 100);
    assert_eq!(c.world.state().npcs[&2].health, 0);
    c.receive(ClientCommand::QuestgiverChooseReward {
        npc: guid(&c, 3),
        quest: 70001,
        choice: 0,
    })
    .unwrap();
    let reward = step(&mut c);
    assert_eq!(
        reward
            .events
            .iter()
            .filter(|e| matches!(e, SessionEvent::QuestComplete(q) if q.quest_id == 70001))
            .count(),
        1
    );
    assert!(
        c.world
            .state()
            .quest_progression
            .as_ref()
            .unwrap()
            .quests
            .progress(70001)
            .unwrap()
            .rewarded
    );
    assert!(c.world.player_level() > 1);
    c = reopen(&h, c, 100);
    assert!(
        c.world
            .state()
            .quest_progression
            .as_ref()
            .unwrap()
            .quests
            .progress(70001)
            .unwrap()
            .rewarded
    );
    c.receive(ClientCommand::AttackStop).unwrap();
    let out = step(&mut c);
    assert!(!out
        .events
        .iter()
        .any(|e| matches!(e, SessionEvent::XpGain(x) if x.kill)));
}

#[path = "source_spline_controller_tests.rs"]
mod source_spline_controller_tests;

#[path = "player_hide_flags_tests.rs"]
mod player_hide_flags_tests;

#[path = "player_facial_hair_tests.rs"]
mod player_facial_hair_tests;

#[path = "player_map_classification_tests.rs"]
mod player_map_classification_tests;
