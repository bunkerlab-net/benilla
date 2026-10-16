// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored native33 quest/combat/loot composition through the real app controller.
//! Spatial observations are synthetic; production admission and native-cache reopening stay closed.
use super::*;
use benilla_sim::{creature_ai as ai, inventory as inv, quest_lifecycle as q};
use n::synthetic_distinct_quest_fixture as loop_fixture;

struct LoopCatalog {
    quest: q::QuestRow,
}
impl LoopCatalog {
    fn new(profile: &n::Profile) -> Self {
        Self {
            quest: profile
                .quest_progression
                .as_ref()
                .unwrap()
                .quest(loop_fixture::QUEST)
                .unwrap()
                .clone(),
        }
    }
}
impl ClientCatalog for LoopCatalog {
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
        matches!(id, 60001 | 96002)
    }
    fn contains_item_display(&self, id: u32) -> bool {
        matches!(id, 70002 | 900002)
    }
    fn contains_spell(&self, id: u32) -> bool {
        Catalog.contains_spell(id)
    }
    fn player_power_type(&self, player: PlayerId) -> Option<u8> {
        Catalog.player_power_type(player)
    }
    fn player_bytes_2(&self, player: PlayerId) -> Option<u32> {
        Catalog.player_bytes_2(player)
    }
    fn item_metadata(&self, entry: u32) -> Option<benilla_protocol::messages::ItemInfo> {
        if !matches!(entry, 70001 | 900001) {
            return None;
        }
        Some(benilla_protocol::messages::ItemInfo {
            class: if entry == 900001 { 4 } else { 15 },
            subclass: 0,
            name: format!("Authored item {entry}"),
            display_info_id: if entry == 900001 { 900002 } else { 70002 },
            quality: 1,
            flags: 0,
            buy_price: 0,
            sell_price: 0,
            inventory_type: if entry == 900001 { 5 } else { 0 },
            allowable_class: -1,
            allowable_race: -1,
            item_level: 0,
            required_level: u32::from(entry == 900001),
            required_skill: 0,
            required_skill_rank: 0,
            required_spell: 0,
            required_honor_rank: 0,
            required_city_rank: 0,
            required_rep_faction: 0,
            required_rep_rank: 0,
            max_count: 0,
            stackable: if entry == 900001 { 1 } else { 5 },
            container_slots: 0,
            stats: if entry == 900001 {
                vec![(5, 3), (6, 2)]
            } else {
                vec![]
            },
            damages: Vec::new(),
            dmg_min: 0.0,
            dmg_max: 0.0,
            dmg_type: 0,
            armor: if entry == 900001 { 7 } else { 0 },
            resistances: [0; 6],
            delay_ms: 0,
            ammo_type: 0,
            ranged_mod_range: 0.0,
            spells: Vec::new(),
            spell_charges_0: 0,
            use_spell: None,
            bonding: 0,
            description: "An original test token.".into(),
            page_text: 0,
            language_id: 0,
            page_material: 0,
            start_quest: 0,
            lock_id: 0,
            material: 0,
            sheath: 0,
            random_property: 0,
            block: 0,
            item_set: 0,
            max_durability: 0,
            area: 0,
            map: 0,
            bag_family: 0,
        })
    }
    fn quest_metadata(&self, id: u32) -> Option<benilla_world_client::QuestMetadata> {
        let mut metadata = Catalog.quest_metadata(id)?;
        for (i, objective) in metadata.template.objectives.iter_mut().enumerate() {
            objective.creature_or_go = self.quest.required_creature[i] as u32;
            objective.required_count = self.quest.required_creature_count[i];
            objective.item_id = self.quest.required_item[i];
            objective.item_count = self.quest.required_item_count[i];
        }
        Some(metadata)
    }
    fn quest_greeting(&self, entry: u32) -> Option<String> {
        (entry == 96002).then(|| "An authored distinct-giver request".into())
    }
    fn native_quest_greeting(
        &self,
        entry: u32,
    ) -> Option<benilla_world_client::NativeQuestGreeting> {
        Some(benilla_world_client::NativeQuestGreeting {
            has_greeting_row: true,
            text: self.quest_greeting(entry)?,
            emote_delay: 0,
            emote: 0,
        })
    }
    fn native_quest_presentation(
        &self,
        id: u32,
    ) -> Option<benilla_world_client::NativeQuestPresentation> {
        Catalog.native_quest_presentation(id)
    }
}

#[derive(Clone, Default)]
struct Controls {
    calls: Arc<AtomicU64>,
    queries: Arc<Mutex<Vec<n::AuthorityRequest>>>,
    derived: Arc<Mutex<Vec<n::AuthorityReply>>>,
    fail_owner: Arc<AtomicU64>,
}
struct ObservedAuthority {
    controls: Controls,
    inner: loop_fixture::CompleteAuthority,
}
impl n::Authority for ObservedAuthority {
    fn resolve(
        &mut self,
        request: &n::AuthorityRequest,
    ) -> Result<n::AuthorityReply, n::NativeError> {
        self.controls.calls.fetch_add(1, Ordering::SeqCst);
        self.controls.queries.lock().unwrap().push(request.clone());
        self.inner.resolve(request)
    }
    fn observe_derived(&mut self, reply: &n::AuthorityReply) -> Result<(), n::NativeError> {
        self.controls.calls.fetch_add(1, Ordering::SeqCst);
        self.controls.derived.lock().unwrap().push(reply.clone());
        self.inner.observe_derived(reply)?;
        if matches!(&reply.request.query,
            n::Query::SourceMotion(n::MotionQuery::VisitorSight(sight))
            if matches!(sight.notification_owner, n::VisitorParticipant::Creature(id)
                if id.spawn == self.controls.fail_owner.load(Ordering::SeqCst)))
        {
            return Err(n::NativeError::AuthorityUnavailable(
                "authored late giver receipt fault",
            ));
        }
        Ok(())
    }
}
fn enter(h: &Harness, profile: n::Profile, open: bool, controls: Controls) -> NativeController {
    let catalog = LoopCatalog::new(&profile);
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
        profile,
        &catalog,
        ObservedAuthority {
            controls,
            inner: Default::default(),
        },
    )
    .unwrap()
}
fn guid(c: &NativeController, spawn: u64) -> u64 {
    c.projection
        .npc_guid(c.world.state().npcs[&spawn].id)
        .unwrap()
}
fn progress(c: &NativeController) -> &n::QuestProgressionState {
    c.world.state().quest_progression.as_ref().unwrap()
}
fn inventory(c: &NativeController) -> &inv::State {
    progress(c).inventory.as_ref().unwrap()
}
fn step(c: &mut NativeController) -> Pump {
    let out = c.pump(c.tick_duration());
    assert!(out.pause.is_none(), "{:?}", out.pause);
    assert_eq!(out.ticks, 1);
    assert_eq!(out.saved.as_ref().unwrap().tick, c.tick());
    out
}
fn accept(c: &mut NativeController, giver: u64) -> Pump {
    c.receive(ClientCommand::QuestgiverAccept {
        npc: guid(c, giver),
        quest: loop_fixture::QUEST,
    })
    .unwrap();
    step(c)
}
fn begin_cast(c: &mut NativeController, hostile: u64) -> Pump {
    c.receive(ClientCommand::CastSpell {
        spell_id: loop_fixture::SPELL,
        target: Some(guid(c, hostile)),
    })
    .unwrap();
    step(c)
}
fn kill(c: &mut NativeController, hostile: u64) -> Vec<SessionEvent> {
    let mut events = begin_cast(c, hostile).events;
    for _ in 0..100 {
        if c.world.state().npcs[&hostile].health == 0 {
            return events;
        }
        events.extend(step(c).events);
    }
    panic!("source-selected authored spell did not kill the target");
}
fn open_loot(c: &mut NativeController, hostile: u64) -> Pump {
    c.receive(ClientCommand::Loot {
        guid: guid(c, hostile),
    })
    .unwrap();
    step(c)
}
fn collect(c: &mut NativeController) -> Pump {
    c.receive(ClientCommand::LootMoney).unwrap();
    c.receive(ClientCommand::AutostoreLootItem { slot: 0 })
        .unwrap();
    step(c)
}
fn choose_reward(c: &mut NativeController, giver: u64) {
    c.receive(ClientCommand::QuestgiverChooseReward {
        npc: guid(c, giver),
        quest: loop_fixture::QUEST,
        choice: 0,
    })
    .unwrap();
}
fn no_transient_progress(events: &[SessionEvent]) {
    assert!(!events.iter().any(|event| matches!(
        event,
        SessionEvent::XpGain(_)
            | SessionEvent::LevelUp(_)
            | SessionEvent::QuestComplete(_)
            | SessionEvent::QuestObjectiveKill { .. }
            | SessionEvent::QuestObjectiveItem { .. }
            | SessionEvent::ItemPushResult(_)
            | SessionEvent::LootClearMoney
    )));
}
fn reopen(
    h: &Harness,
    c: NativeController,
    data: &n::Profile,
    controls: &Controls,
) -> NativeController {
    let checkpoint = c.world.checkpoint().unwrap();
    let state = c.world.state().clone();
    drop(c); // Process loss after the durable receipt, without orderly shutdown.
    let mut c = enter(h, data.clone(), true, controls.clone());
    assert_eq!(c.world.checkpoint().unwrap(), checkpoint);
    assert_eq!(c.world.state(), &state);
    no_transient_progress(&c.initial_events().unwrap());
    assert!(c.loot_window.is_none());
    c
}
fn assert_source_hosts(controls: &Controls) {
    let queries = controls.queries.lock().unwrap();
    assert!(!queries.iter().any(|request| matches!(
        request.query,
        n::Query::QuestStatus { .. }
            | n::Query::QuestInteraction { .. }
            | n::Query::LootInteraction { .. }
            | n::Query::Npc { .. }
            | n::Query::NativeMelee { .. }
            | n::Query::Spell { .. }
    )));
    let derived = controls.derived.lock().unwrap();
    for phase in [
        n::SourceSpellPhase::Prepare,
        n::SourceSpellPhase::ReleaseHit,
        n::SourceSpellPhase::Impact,
    ] {
        assert!(derived.iter().any(|reply| matches!(&reply.request.query,
            n::Query::SourceSpell(n::SourceSpellQuery::Semantics(context)) if context.phase == phase
        )), "missing selected spell phase {phase:?}");
    }
    assert!(derived
        .iter()
        .any(|reply| matches!(reply.request.query, n::Query::QuestInteraction { .. })));
    assert!(derived
        .iter()
        .any(|reply| matches!(reply.request.query, n::Query::LootInteraction { .. })));
}

#[path = "distinct_quest_loop_fault_tests.rs"]
mod fault_tests;
#[path = "distinct_quest_loop_progress_tests.rs"]
mod progress_tests;
