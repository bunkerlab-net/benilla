// SPDX-License-Identifier: GPL-3.0-or-later
// Authored executable manager owners. These are not sealed F1 execution.
use super::*;
use benilla_sim::source_time::IntervalTimer;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
#[path = "game_event_effect_tests.rs"]
mod effects;
#[derive(Default)]
struct Rows {
    maximum: Option<u16>,
    definitions: VecDeque<GameEventDefinitionRow>,
    dates: VecDeque<GameEventDateRow>,
    creatures: VecDeque<GameEventMemberRow>,
    objects: VecDeque<GameEventMemberRow>,
    substitutions: VecDeque<GameEventSubstitutionRow>,
    quests: VecDeque<(u64, u32, u16)>,
    mails: VecDeque<GameEventMailRow>,
}
impl GameEventRows for Rows {
    fn maximum(&mut self) -> Result<Option<u16>> {
        Ok(self.maximum.take())
    }
    fn definition(&mut self) -> Result<Option<GameEventDefinitionRow>> {
        Ok(self.definitions.pop_front())
    }
    fn date(&mut self) -> Result<Option<GameEventDateRow>> {
        Ok(self.dates.pop_front())
    }
    fn member(&mut self, kind: EventSpawnKind) -> Result<Option<GameEventMemberRow>> {
        Ok(if kind == EventSpawnKind::Creature {
            self.creatures.pop_front()
        } else {
            self.objects.pop_front()
        })
    }
    fn substitution(&mut self) -> Result<Option<GameEventSubstitutionRow>> {
        Ok(self.substitutions.pop_front())
    }
    fn quest(&mut self) -> Result<Option<(u64, u32, u16)>> {
        Ok(self.quests.pop_front())
    }
    fn mail(&mut self) -> Result<Option<GameEventMailRow>> {
        Ok(self.mails.pop_front())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EventCall {
    Clock(EventClockSite),
    Effect(GameEventEffect),
}
#[derive(Default)]
struct Owner<'a> {
    clocks: VecDeque<(EventClockSite, ClockSample)>,
    effects: Vec<GameEventEffect>,
    observations: Vec<GameEventObservation>,
    missing: BTreeSet<(u8, u32)>,
    quest_flags: BTreeMap<u32, bool>,
    pool_checks: Vec<(u16, i16, usize, usize)>,
    references: Vec<(EventReference, u32)>,
    reject: Option<GameEventEffect>,
    lunar: Option<LunarEventBindings<'a>>,
    reference_lunar: Option<ReferenceLunarEventOwner<'a>>,
    timeline: Vec<EventCall>,
}
impl Owner<'_> {
    fn wall(&mut self, site: EventClockSite, seconds: i64) {
        self.clocks
            .push_back((site, ClockSample::authored(ClockKind::WallSeconds, seconds)));
    }
    fn update_times(&mut self, now: i64, checks: &[(u16, i64)]) {
        self.wall(EventClockSite::Update, now);
        for (id, seconds) in checks {
            self.wall(EventClockSite::NextCheck(*id), *seconds);
        }
    }
}
impl GameEventOwner for Owner<'_> {
    fn clock(&mut self, site: EventClockSite) -> Result<ClockSample> {
        self.timeline.push(EventCall::Clock(site));
        let (expected, sample) = self.clocks.pop_front().expect("missing authored clock");
        assert_eq!(site, expected);
        Ok(sample)
    }
    fn lunar_bindings(&mut self) -> Result<LunarEventBindings<'_>> {
        if self.lunar.is_none() {
            return self
                .reference_lunar
                .as_mut()
                .map(ReferenceLunarEventOwner::bindings)
                .ok_or_else(|| refuse("execution.event.lunar_reference_dispatch_unbound"));
        }
        let bindings = self.lunar.as_mut().unwrap();
        Ok(LunarEventBindings {
            calendar: &mut *bindings.calendar,
            sine: &mut *bindings.sine,
            trace: &mut *bindings.trace,
        })
    }
    fn reference(&mut self, kind: EventReference, id: u32) -> Result<bool> {
        self.references.push((kind, id));
        Ok(!self.missing.contains(&(kind as u8, id)))
    }
    fn set_quest_active(&mut self, quest: u32, active: bool) -> Result<()> {
        self.quest_flags.insert(quest, active);
        Ok(())
    }
    fn check_pool_event(
        &mut self,
        _: &SourcePoolManager,
        pool: u16,
        event: i16,
        creatures: &BTreeMap<u32, i16>,
        gameobjects: &BTreeMap<u32, i16>,
    ) -> Result<()> {
        self.pool_checks
            .push((pool, event, creatures.len(), gameobjects.len()));
        Ok(())
    }
    fn effect(
        &mut self,
        manager: &mut SourceGameEventManager,
        effect: GameEventEffect,
    ) -> Result<()> {
        self.timeline.push(EventCall::Effect(effect));
        self.effects.push(effect);
        if self.reject == Some(effect) {
            return Err(refuse("execution.event.authored_effect_refusal"));
        }
        if let GameEventEffect::Quests { event, active } = effect {
            for quest in &manager.inventory().quests[event as usize] {
                self.quest_flags.insert(*quest, active);
            }
        }
        if let GameEventEffect::Notify { event, active, .. } = effect {
            assert_eq!(manager.active(u32::from(event)), active);
            for quest in &manager.inventory().quests[event as usize] {
                assert_eq!(manager.inventory().quest_active[quest], active);
                assert_eq!(self.quest_flags[quest], active);
            }
        }
        Ok(())
    }
    fn observe(&mut self, observation: GameEventObservation) -> Result<()> {
        self.observations.push(observation);
        Ok(())
    }
}
#[derive(Default)]
struct PoolRowsFixture {
    templates: VecDeque<PoolTemplateRow>,
    members: VecDeque<PoolMembershipRow>,
}
impl PoolRows for PoolRowsFixture {
    fn maximum(&mut self) -> Result<Option<u16>> {
        Ok(Some(self.templates.iter().map(|r| r.id).max().unwrap_or(0)))
    }
    fn template(&mut self) -> Result<Option<PoolTemplateRow>> {
        Ok(self.templates.pop_front())
    }
    fn member(&mut self, slot: usize) -> Result<Option<PoolMembershipRow>> {
        Ok(if slot == 2 {
            self.members.pop_front()
        } else {
            None
        })
    }
}
struct PoolOwner;
impl PoolLoadOwner for PoolOwner {
    fn spawn_map(&mut self, _: PoolKind, _: u32) -> Result<Option<u32>> {
        Ok(Some(0))
    }
    fn map_exists(&mut self, _: u32) -> Result<bool> {
        Ok(true)
    }
}
fn pools(pooled: bool) -> SourcePoolManager {
    let mut rows = PoolRowsFixture::default();
    if pooled {
        rows.templates.push_back(PoolTemplateRow {
            ordinal: 0,
            id: 1,
            limit: 1,
            description: vec![],
        });
        rows.members.push_back(PoolMembershipRow {
            ordinal: 0,
            guid: 7,
            pool: 1,
            chance: 0.0,
        });
    }
    let mut pools = SourcePoolManager::default();
    pools.load(&mut rows, &mut PoolOwner).unwrap();
    pools
}
fn definition(id: u16) -> GameEventDefinitionRow {
    GameEventDefinitionRow {
        ordinal: u64::from(id),
        id,
        schedule: 1,
        occurrence: 10,
        length: 1,
        holiday: 0,
        linked: 0,
        description: vec![],
    }
}
fn rows(count: u16) -> Rows {
    Rows {
        maximum: Some(count),
        definitions: (1..=count).map(definition).collect(),
        dates: (1..=count)
            .map(|id| GameEventDateRow {
                ordinal: u64::from(id),
                id,
                start: b"1970-01-01 00:01:40".to_vec(),
                end: b"1970-01-01 00:16:40".to_vec(),
            })
            .collect(),
        ..Rows::default()
    }
}
fn manager(rows: &mut Rows, owner: &mut Owner<'_>) -> SourceGameEventManager {
    let mut manager = SourceGameEventManager::default();
    manager.load(rows, &mut pools(false), owner).unwrap();
    manager
}
#[test]
fn strict_edges_modulo_and_separate_next_check_sample() {
    let mut owner = Owner::default();
    let mut manager = manager(&mut rows(1), &mut owner);
    for (now, active) in [
        (100, false),
        (101, true),
        (159, true),
        (160, false),
        (700, true),
        (760, false),
        (1000, false),
    ] {
        assert_eq!(manager.check_one(1, now).unwrap(), active);
    }
    owner.update_times(130, &[(1, 500)]);
    assert_eq!(
        manager.initialize(None, false, &mut owner).unwrap(),
        201_000
    );
    assert!(manager.active(1));
    assert!(owner.clocks.is_empty());
    owner.wall(EventClockSite::NextCheck(1), 1000);
    assert_eq!(manager.next_check(1, &mut owner).unwrap(), 0);
}
#[test]
fn ascending_link_scan_is_not_a_fixed_point() {
    let mut rows = rows(2);
    rows.definitions[0].linked = 2;
    let mut owner = Owner::default();
    let mut manager = manager(&mut rows, &mut owner);
    owner.update_times(130, &[(1, 130), (2, 130)]);
    manager.initialize(None, false, &mut owner).unwrap();
    assert_eq!(manager.inventory().active, BTreeSet::from([2]));
    owner.update_times(130, &[(1, 130), (2, 130)]);
    manager.update(false, &mut owner).unwrap();
    assert_eq!(manager.inventory().active, BTreeSet::from([1, 2]));
}
#[test]
fn startup_resume_and_stop_preserve_status_and_signed_effect_order() {
    let mut owner = Owner::default();
    let mut manager = manager(&mut rows(1), &mut owner);
    owner.update_times(130, &[(1, 130)]);
    manager.initialize(Some(&[1]), true, &mut owner).unwrap();
    assert_eq!(
        owner.effects,
        vec![
            GameEventEffect::TruncateStatus,
            GameEventEffect::InsertStatus(1),
            GameEventEffect::Announce(1),
            GameEventEffect::Spawn(1),
            GameEventEffect::Unspawn(-1),
            GameEventEffect::Substitutions {
                event: 1,
                active: true
            },
            GameEventEffect::Quests {
                event: 1,
                active: true
            },
            GameEventEffect::Notify {
                event: 1,
                active: true,
                resume: true
            }
        ]
    );
    owner.effects.clear();
    owner.update_times(160, &[(1, 160)]);
    manager.update(false, &mut owner).unwrap();
    assert_eq!(
        owner.effects,
        vec![
            GameEventEffect::DeleteStatus(1),
            GameEventEffect::Unspawn(1),
            GameEventEffect::Spawn(-1),
            GameEventEffect::Substitutions {
                event: 1,
                active: false
            },
            GameEventEffect::Quests {
                event: 1,
                active: false
            },
            GameEventEffect::Mail(-1),
            GameEventEffect::Notify {
                event: 1,
                active: false,
                resume: false
            }
        ]
    );
}
#[test]
fn two_timer_resets_subtract_old_then_new_interval() {
    let mut owner = Owner::default();
    let mut manager = manager(&mut rows(1), &mut owner);
    owner.update_times(130, &[(1, 130)]);
    manager.initialize(None, false, &mut owner).unwrap();
    let mut timer = IntervalTimer::default();
    timer.set_interval(1000);
    timer.set_current(40_000);
    owner.update_times(130, &[(1, 130)]);
    assert!(manager
        .update_world_timer(&mut timer, false, &mut owner)
        .unwrap());
    assert_eq!((timer.get_interval(), timer.get_current()), (31_000, 8_000));
    let timers: Vec<_> = owner
        .observations
        .iter()
        .filter_map(|v| {
            if let GameEventObservation::Timer { interval, current } = v {
                Some((*interval, *current))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(timers, vec![(1000, 39_000), (31_000, 8_000)]);
}
#[test]
fn never_assigned_slots_follow_recorded_zero_and_initial_clock_order() {
    let mut data = rows(3);
    data.definitions.remove(1);
    data.definitions[0].linked = 2;
    let mut owner = Owner::default();
    let mut manager = manager(&mut data, &mut owner);
    assert_eq!(manager.inventory().definitions[1].linked, 2);
    for id in [0, 2] {
        assert_eq!(manager.inventory().definitions[id].schedule, 0);
        assert!(manager.is_valid(id as u32).unwrap());
        assert_eq!(manager.inventory().definitions[id].start, 1);
        assert_eq!(manager.inventory().definitions[id].end, 0);
        assert!(!manager.check_one(id as u16, 130).unwrap());
    }
    owner.update_times(130, &[(1, 131), (2, 132), (3, 133)]);
    manager.initialize(None, false, &mut owner).unwrap();
    assert_eq!(manager.inventory().active, BTreeSet::from([3]));
    assert!(owner.effects.contains(&GameEventEffect::Spawn(-2)));
    assert!(owner.timeline.windows(2).any(|pair| pair
        == [
            EventCall::Effect(GameEventEffect::Spawn(-2)),
            EventCall::Clock(EventClockSite::NextCheck(2)),
        ]));
    assert!(!owner.effects.contains(&GameEventEffect::Spawn(0)));
    assert!(manager.inventory().creatures[1].is_empty());
    assert!(manager.inventory().gameobjects[1].is_empty());
    owner.update_times(130, &[(1, 130), (3, 130)]);
    manager.update(false, &mut owner).unwrap();
    manager.weekly_recalculate(&mut owner).unwrap();
    assert!(owner.clocks.is_empty());
    assert!(!manager.inventory().stopped);
}

#[test]
fn uint16_scan_overflow_still_refuses() {
    let mut owner = Owner::default();
    let mut data = rows(1);
    data.maximum = Some(u16::MAX);
    let mut manager = SourceGameEventManager::default();
    assert_eq!(
        manager
            .load(&mut data, &mut pools(false), &mut owner)
            .unwrap_err()
            .id,
        "execution.event.uint16_scan_unbounded"
    );
}
#[test]
fn disabled_linked_and_serverside_corrections_follow_source_order() {
    let mut data = rows(3);
    data.definitions[0].occurrence = 0;
    data.definitions[1].linked = 99;
    data.definitions[2].schedule = 0;
    data.definitions[2].length = 0;
    data.definitions[2].occurrence = 0;
    let mut owner = Owner::default();
    let mut manager = manager(&mut data, &mut owner);
    assert_eq!(manager.inventory().definitions[1].occurrence, 1);
    assert_eq!(manager.inventory().definitions[1].start, 4_102_444_800);
    assert_eq!(manager.inventory().definitions[2].linked, 0);
    assert_eq!(manager.inventory().definitions[3].end, 4_102_444_800);
    owner.update_times(130, &[(1, 130), (2, 130), (3, 130)]);
    manager.initialize(None, false, &mut owner).unwrap();
    owner.update_times(130, &[(1, 130), (2, 130)]);
    manager.update(false, &mut owner).unwrap();
    assert!(owner.clocks.is_empty());
}
#[test]
fn signed_memberships_pool_association_and_substitution_first_active_winner() {
    let mut data = rows(2);
    data.creatures = VecDeque::from([
        GameEventMemberRow {
            ordinal: 0,
            exists: true,
            guid: 7,
            event: 1,
        },
        GameEventMemberRow {
            ordinal: 1,
            exists: true,
            guid: 7,
            event: -1,
        },
        GameEventMemberRow {
            ordinal: 2,
            exists: true,
            guid: 8,
            event: 1,
        },
        GameEventMemberRow {
            ordinal: 3,
            exists: true,
            guid: 8,
            event: 1,
        },
    ]);
    let mut owner = Owner::default();
    let mut manager = SourceGameEventManager::default();
    let mut pools = pools(true);
    manager.load(&mut data, &mut pools, &mut owner).unwrap();
    assert_eq!(manager.pool_event(1).unwrap(), 1);
    assert_eq!(
        manager.member_event(EventSpawnKind::Creature, 7).unwrap(),
        -1
    );
    assert_eq!(
        manager.inventory().creatures[manager.inventory().definitions.len()].as_slice(),
        [8, 8]
    );
    assert_eq!(manager.inventory().pools[1], [1]);
    assert!(!pools.inventory().templates[1].auto_spawn);
}
#[test]
fn failed_reached_effect_stops_without_clearing_mutated_active_state() {
    let mut owner = Owner::default();
    let mut manager = manager(&mut rows(1), &mut owner);
    owner.reject = Some(GameEventEffect::InsertStatus(1));
    owner.update_times(130, &[(1, 130)]);
    assert_eq!(
        manager.initialize(None, false, &mut owner).unwrap_err().id,
        "execution.event.authored_effect_refusal"
    );
    assert!(manager.active(1));
    assert!(manager.inventory().stopped);
    assert_eq!(
        manager.update(false, &mut owner).unwrap_err().id,
        "execution.event.stopped"
    );
}

#[test]
fn substitution_corrections_and_equal_key_first_active_winner() {
    let mut data = rows(2);
    let first = GameEventSubstitution {
        guid: 9,
        entry: 22,
        model: 123,
        equipment: 21,
        start_spell: 23,
        end_spell: 24,
    };
    data.substitutions = VecDeque::from([
        GameEventSubstitutionRow {
            ordinal: 0,
            exists: true,
            event: 2,
            data: first,
        },
        GameEventSubstitutionRow {
            ordinal: 1,
            exists: true,
            event: 2,
            data: GameEventSubstitution { entry: 88, ..first },
        },
        GameEventSubstitutionRow {
            ordinal: 2,
            exists: true,
            event: 1,
            data: GameEventSubstitution { entry: 77, ..first },
        },
    ]);
    data.quests = VecDeque::from([(0, 31, 1), (1, 32, 1)]);
    let mut owner = Owner::default();
    owner.missing.extend([
        (EventReference::Equipment as u8, 21),
        (EventReference::CreatureTemplate as u8, 22),
        (EventReference::Spell as u8, 23),
        (EventReference::Quest as u8, 32),
    ]);
    let mut manager = manager(&mut data, &mut owner);
    assert_eq!(
        manager.inventory().substitutions[2][0],
        GameEventSubstitution {
            entry: 0,
            equipment: 0,
            start_spell: 0,
            ..first
        }
    );
    assert_eq!(manager.inventory().quests[1], [31]);
    assert!(!owner.quest_flags[&31]);
    owner.update_times(130, &[(1, 130), (2, 130)]);
    manager.initialize(None, false, &mut owner).unwrap();
    assert_eq!(manager.active_substitution(9).unwrap().unwrap().entry, 0);
    assert_eq!(manager.inventory().substitution_events[&9], [2, 2, 1]);
    manager.stop_event(2, false, &mut owner).unwrap();
    assert_eq!(manager.active_substitution(9).unwrap().unwrap().entry, 77);
}

#[test]
fn unsigned_mail_ids_and_playable_race_checks_do_not_admit_goblins() {
    let mut data = rows(1);
    let mail = GameEventMail {
        race_mask: 1,
        quest: 0,
        template: 7,
        sender: 8,
    };
    data.mails = VecDeque::from([
        GameEventMailRow {
            ordinal: 0,
            event: 1,
            mail,
        },
        GameEventMailRow {
            ordinal: 1,
            event: 1,
            mail: GameEventMail {
                race_mask: 1 << 8,
                ..mail
            },
        },
        GameEventMailRow {
            ordinal: 2,
            event: u16::MAX,
            mail,
        },
    ]);
    let mut owner = Owner::default();
    let manager = manager(&mut data, &mut owner);
    assert_eq!(
        manager.inventory().mails[manager.inventory().definitions.len()],
        [mail]
    );
    assert_eq!(
        owner.references,
        [
            (EventReference::MailTemplate, 7),
            (EventReference::CreatureTemplate, 8)
        ]
    );
}

#[test]
fn overwrite_retains_source_seconds_and_weekly_zero_is_inert() {
    let mut owner = Owner::default();
    let mut manager = manager(&mut rows(1), &mut owner);
    owner.wall(EventClockSite::OverwriteStart(1), 10_000);
    manager
        .start_event(1, true, false, false, &mut owner)
        .unwrap();
    assert_eq!(
        (
            manager.inventory().definitions[1].start,
            manager.inventory().definitions[1].end
        ),
        (10_000, 10_001)
    );
    owner.wall(EventClockSite::OverwriteStop(1), 20_000);
    manager.stop_event(1, true, &mut owner).unwrap();
    assert_eq!(
        (
            manager.inventory().definitions[1].start,
            manager.inventory().definitions[1].end
        ),
        (19_940, 19_941)
    );
    manager.weekly_recalculate(&mut owner).unwrap();
    assert_eq!(manager.inventory().definitions[0].schedule, 0);
    assert!(!manager.inventory().stopped);
    assert!(owner.clocks.is_empty());
}

#[test]
fn slot_zero_manual_effects_and_next_check_keep_source_boundaries() {
    let mut owner = Owner::default();
    let mut manager = manager(&mut rows(1), &mut owner);
    for (now, expected) in [(-1, 2), (0, 1), (1, 86400), (130, 86400)] {
        owner.wall(EventClockSite::NextCheck(0), now);
        assert_eq!(manager.next_check(0, &mut owner).unwrap(), expected);
    }
    manager
        .start_event(0, false, false, false, &mut owner)
        .unwrap();
    assert!(manager.active(0));
    assert_eq!(
        &owner.effects[..3],
        &[
            GameEventEffect::InsertStatus(0),
            GameEventEffect::Spawn(0),
            GameEventEffect::Unspawn(0),
        ]
    );
    owner.effects.clear();
    manager.stop_event(0, false, &mut owner).unwrap();
    assert!(!manager.active(0));
    assert_eq!(
        &owner.effects[..3],
        &[
            GameEventEffect::DeleteStatus(0),
            GameEventEffect::Unspawn(0),
            GameEventEffect::Spawn(0),
        ]
    );
    assert!(owner.clocks.is_empty());
}

#[test]
fn weekly_scan_includes_zero_but_only_calendar_schedules_sample_time() {
    let mut data = rows(3);
    data.definitions.remove(1);
    data.definitions[0].schedule = 11;
    let mut owner = Owner::default();
    owner.wall(EventClockSite::LoadYearly(1), 130);
    let mut manager = manager(&mut data, &mut owner);
    let zero = manager.inventory().definitions[0].clone();
    let hole = manager.inventory().definitions[2].clone();
    // ResetWeeklyQuests reaches this scan after gameTime > reset; World.cpp:1560-1561,2337.
    owner.wall(EventClockSite::Weekly(1), 31_536_130);
    manager.weekly_recalculate(&mut owner).unwrap();
    let scanned: Vec<_> = owner
        .observations
        .iter()
        .filter_map(|observation| {
            if let GameEventObservation::WeeklySchedule { event, schedule } = observation {
                Some((*event, *schedule))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(scanned, [(0, 0), (1, 11), (2, 0), (3, 1)]);
    assert_eq!(manager.inventory().definitions[0], zero);
    assert_eq!(manager.inventory().definitions[2], hole);
    assert_eq!(manager.inventory().definitions[1].start, 31_536_100);
    assert!(owner.clocks.is_empty());
}

#[test]
fn recorded_empty_slot_projection_or_explicit_absence() {
    let Ok(path) = std::env::var("BENILLA_F4_EVENT_ZERO") else {
        println!("SKIP round-15 empty-slot projection: BENILLA_F4_EVENT_ZERO absent");
        return;
    };
    let evidence: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(evidence["observed"]["all_raw_values_u32"], 0);
    assert_eq!(evidence["observed"]["slot_count"], 1025);
    assert_eq!(evidence["observed"]["reached_ID_holes"], 957);
    assert_eq!(evidence["observed"]["schedule_reads"], 2871);
    assert_eq!(evidence["observed"]["slot_zero_reads"], 0);
    assert_eq!(evidence["observed"]["weekly_reads"], 0);
    let projection_path = std::path::Path::new(&path)
        .parent()
        .unwrap()
        .join("seal15-input-comparison-diagnostic.full.json");
    if !projection_path.exists() {
        println!("SKIP round-15 raw hole-ID projection: retained diagnostic absent");
        return;
    }
    let projection: serde_json::Value =
        serde_json::from_slice(&std::fs::read(projection_path).unwrap()).unwrap();
    assert_eq!(projection["value_counts_u32"]["0"], 2871);
    assert_eq!(projection["read_site_counts"]["10064"], 957);
    assert_eq!(projection["read_site_counts"]["206761"], 957);
    assert_eq!(projection["read_site_counts"]["206762"], 957);
    let holes: BTreeSet<u16> = projection["empty_slot_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| u16::try_from(id.as_u64().unwrap()).unwrap())
        .collect();
    assert_eq!(holes.len(), 958);
    assert!(holes.contains(&0) && holes.contains(&3));
    let mut data = rows(1024);
    data.definitions.retain(|row| !holes.contains(&row.id));
    data.dates.clear();
    let mut owner = Owner::default();
    let mut manager = manager(&mut data, &mut owner);
    let native_holes: BTreeSet<u16> = manager
        .inventory()
        .definitions
        .iter()
        .enumerate()
        .filter_map(|(id, d)| (d.schedule == 0).then_some(id as u16))
        .collect();
    assert_eq!(native_holes, holes);
    owner.update_times(130, &(1..=1024).map(|id| (id, 130)).collect::<Vec<_>>());
    assert_eq!(
        manager.initialize(None, false, &mut owner).unwrap(),
        86_401_000
    );
    assert_eq!(owner.effects.len(), 1024);
    assert!(owner
        .effects
        .iter()
        .all(|effect| matches!(effect, GameEventEffect::Spawn(id) if *id < 0)));
    assert!(owner.clocks.is_empty());
    assert!(manager.inventory().active.is_empty());
    println!("Unsealed round-15 projection: 957 holes, 2871 recorded zero reads; authored 957 hole NextCheck samples and empty negative spawns matched source consequences; no event-transition or whole-execution acceptance");
}
#[test]
fn restart_event_only_activates_and_requests_shutdown() {
    let mut owner = Owner::default();
    let mut manager = manager(&mut rows(987), &mut owner);
    manager
        .start_event(987, false, false, true, &mut owner)
        .unwrap();
    assert!(manager.active(987));
    assert_eq!(owner.effects, [GameEventEffect::Restart { seconds: 60 }]);
}

#[test]
fn zero_modulus_and_wrong_clock_kind_refuse_at_the_reached_operation() {
    let mut data = rows(1);
    data.definitions[0].schedule = 0;
    data.definitions[0].length = 0;
    data.definitions[0].occurrence = 0;
    let mut owner = Owner::default();
    let mut manager = manager(&mut data, &mut owner);
    owner.wall(EventClockSite::NextCheck(1), 4_102_444_800);
    assert_eq!(
        manager.next_check(1, &mut owner).unwrap_err().id,
        "execution.event.modulo_zero_undefined"
    );
    assert!(manager.inventory().stopped);
    let mut manager = self::manager(&mut rows(1), &mut owner);
    owner.clocks.push_back((
        EventClockSite::Update,
        ClockSample::authored(ClockKind::MonotonicNanoseconds, 1),
    ));
    assert_eq!(
        manager.initialize(None, false, &mut owner).unwrap_err().id,
        "execution.event.wall_clock_kind"
    );
}

#[test]
fn retained_raw_event_query_streams_or_explicit_absence() {
    let Some(occurrences) = std::env::var_os("BENILLA_F2_EVENT_OCCURRENCES") else {
        println!(
            "SKIP raw event query fixture: set BENILLA_F2_EVENT_OCCURRENCES and BENILLA_F2_CATALOG"
        );
        return;
    };
    let catalog =
        std::env::var_os("BENILLA_F2_CATALOG").expect("explicit event occurrences require catalog");
    let mut catalog =
        super::super::production_compiler::VerifiedCatalog::open(Path::new(&catalog), false)
            .unwrap();
    let mut rows = catalog
        .execution_event_rows(Path::new(&occurrences))
        .unwrap();
    assert_eq!(rows.maximum().unwrap(), Some(1024));
    assert_eq!(rows.maximum().unwrap(), None);
    let mut ids = BTreeSet::new();
    let mut count = [1, 0, 0, 0, 0, 0, 0, 0];
    while let Some(row) = rows.definition().unwrap() {
        count[1] += 1;
        assert!(row.schedule <= 15);
        ids.insert(row.id);
    }
    while rows.date().unwrap().is_some() {
        count[2] += 1;
    }
    while rows.member(EventSpawnKind::Creature).unwrap().is_some() {
        count[3] += 1;
    }
    while rows.member(EventSpawnKind::GameObject).unwrap().is_some() {
        count[4] += 1;
    }
    while rows.substitution().unwrap().is_some() {
        count[5] += 1;
    }
    while rows.quest().unwrap().is_some() {
        count[6] += 1;
    }
    while rows.mail().unwrap().is_some() {
        count[7] += 1;
    }
    assert_eq!(count, [1, 67, 38, 3239, 11204, 1025, 61, 1]);
    let holes: Vec<_> = (1..=1024).filter(|id| !ids.contains(id)).collect();
    assert_eq!(holes.len(), 957);
    assert_eq!(holes[0], 3);
    println!("Unsealed raw event query fixture: 15636 operands; 957 never-assigned positive schedule slots, first ID 3; zero effective-loader or F1 acceptance");
}

#[cfg(test)]
mod lunar {
    use super::*;
    use benilla_sim::source_moon::{self as moon, Calendar};
    use serde_json::{json, Value};

    const OWNER: moon::CallOwner = moon::CallOwner { thread: 1, call: 1 };

    fn lunar_rows() -> Rows {
        let mut data = rows(1);
        data.definitions[0].schedule = 12;
        data
    }

    #[test]
    fn missing_lunar_dispatch_refuses_without_publishing_boundaries() {
        let mut owner = Owner::default();
        owner.wall(EventClockSite::LoadSchedule(1), 0);
        let mut manager = SourceGameEventManager::default();
        assert_eq!(
            manager
                .load(&mut lunar_rows(), &mut pools(false), &mut owner)
                .unwrap_err()
                .id,
            "execution.event.lunar_reference_dispatch_unbound",
        );
        assert!(manager.inventory().stopped);
        assert!(!owner
            .observations
            .iter()
            .any(|record| matches!(record, GameEventObservation::Calendar { .. })));

        let mut calendar = moon::RecordedCalendar::new(OWNER, &[]);
        let mut sine = moon::UnboundSine;
        let mut trace = Vec::new();
        let mut owner = Owner {
            lunar: Some(LunarEventBindings {
                calendar: &mut calendar,
                sine: &mut sine,
                trace: &mut trace,
            }),
            ..Owner::default()
        };
        owner.wall(EventClockSite::LoadSchedule(1), 0);
        let mut manager = SourceGameEventManager::default();
        assert_eq!(
            manager
                .load(&mut lunar_rows(), &mut pools(false), &mut owner)
                .unwrap_err()
                .id,
            "execution.event.lunar_reference_dispatch_unbound",
        );
        assert!(manager.inventory().stopped);
        assert!(trace.is_empty());
    }

    fn integer(value: &Value) -> i64 {
        value.as_i64().unwrap()
    }
    fn int(value: &Value) -> i32 {
        i32::try_from(integer(value)).unwrap()
    }
    fn tm(value: &Value) -> benilla_sim::source_calendar::Tm {
        assert_eq!(value.as_array().unwrap().len(), 9);
        benilla_sim::source_calendar::Tm {
            sec: int(&value[0]),
            min: int(&value[1]),
            hour: int(&value[2]),
            mday: int(&value[3]),
            mon: int(&value[4]),
            year: int(&value[5]),
            isdst: int(&value[6]),
            wday: int(&value[7]),
            yday: int(&value[8]),
        }
    }

    // Inject external libc answers while checking all manager-generated requests, per source_moon/FORMAT.md.
    struct ExternalCalendar(VecDeque<Value>);
    impl Calendar for ExternalCalendar {
        fn owner(&self) -> moon::CallOwner {
            OWNER
        }
        fn now(&mut self) -> moon::Result<i64> {
            let record = self.0.pop_front().ok_or(moon::Error::MissingObservation)?;
            assert_eq!(record[0], "time");
            Ok(integer(&record[1]))
        }
        fn local(&mut self, seconds: i64) -> moon::Result<benilla_sim::source_calendar::Tm> {
            let record = self.0.pop_front().ok_or(moon::Error::MissingObservation)?;
            assert_eq!(record[0], "local");
            assert_eq!(integer(&record[1]), seconds);
            Ok(tm(&record[2]))
        }
        fn epoch(
            &mut self,
            input: moon::CalendarInput,
        ) -> moon::Result<(i64, benilla_sim::source_calendar::Tm)> {
            let record = self.0.pop_front().ok_or(moon::Error::MissingObservation)?;
            assert_eq!(record[0], "make");
            let fields = match input {
                moon::CalendarInput::Retained(value) => json!([
                    value.sec,
                    value.min,
                    value.hour,
                    value.mday,
                    value.mon,
                    value.year,
                    value.isdst
                ]),
                moon::CalendarInput::AnchorProbe(date) => json!([
                    0,
                    0,
                    3,
                    date.day,
                    date.month.checked_sub(1).unwrap(),
                    date.year.checked_sub(1900).unwrap(),
                    -1
                ]),
            };
            assert_eq!(record[1], fields);
            Ok((integer(&record[2]), tm(&record[3])))
        }
        fn finish(&self) -> moon::Result<()> {
            if self.0.is_empty() {
                Ok(())
            } else {
                Err(moon::Error::SurplusObservation)
            }
        }
    }

    #[test]
    fn manager_computes_all_authored_lunar_event_boundaries_conditionally() {
        compare_lunar_event_boundaries(false, None);
    }

    #[test]
    fn manager_computes_all_authored_lunar_event_boundaries_with_real_sine() {
        compare_lunar_event_boundaries(true, None);
    }

    fn compare_lunar_event_boundaries(real_sine: bool, stock: Option<&GameEventDefinitionRow>) {
        let root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../benilla-sim/tools/fixtures/source_moon");
        let load = |name: &str| match std::fs::read_to_string(root.join(name)) {
            Ok(value) => Some(value),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                println!("SKIP authored lunar component fixture absent: {name}");
                None
            }
            Err(error) => panic!("lunar fixture {name}: {error}"),
        };
        let Some(cases) = load("cases.json") else {
            return;
        };
        let Some(expected) = load("expected.jsonl") else {
            return;
        };
        let cases: Value = serde_json::from_str(&cases).unwrap();
        let mut count = 0;
        for (case, line) in cases["cases"]
            .as_array()
            .unwrap()
            .iter()
            .zip(expected.lines())
        {
            if case["kind"] != "event" {
                continue;
            }
            count += 1;
            let expected: Value = serde_json::from_str(line).unwrap();
            assert_eq!(expected["case"], case["case"]);
            let mut calendar = VecDeque::new();
            let mut samples = Vec::new();
            let mut clocks = Vec::new();
            let mut cycle = 0;
            let mut position = 0;
            let mut boundaries = None;
            for record in expected["records"].as_array().unwrap() {
                match record[0].as_str().unwrap() {
                    "time" | "local" | "make" => {
                        if record[0] == "time" {
                            clocks.push(integer(&record[1]));
                        }
                        calendar.push_back(record.clone());
                    }
                    "candidate_begin" => {
                        cycle = int(&record[1]);
                        position = 0;
                    }
                    "sine" => {
                        samples.push(moon::SineSample {
                            owner: OWNER,
                            cycle,
                            anomaly: if position == 0 {
                                moon::Anomaly::Solar
                            } else {
                                moon::Anomaly::Lunar
                            },
                            operand: u64::from_str_radix(record[1].as_str().unwrap(), 16).unwrap(),
                            result: u64::from_str_radix(record[2].as_str().unwrap(), 16).unwrap(),
                        });
                        position += 1;
                    }
                    "result" => boundaries = Some((integer(&record[2][0]), integer(&record[2][1]))),
                    _ => {}
                }
            }
            assert_eq!(json!(clocks), case["clocks"]);
            let mut calendar = ExternalCalendar(calendar);
            let mut sine =
                moon::RecordedSine::new(OWNER, Some(moon::SineOrder::SolarFirst), &samples);
            let mut trace = Vec::new();
            let mut owner = if real_sine {
                let text = std::env::var("BENILLA_LUNAR_PROCESS_RECEIPT").map_or_else(
                    |_| include_str!("../../../../benilla-sim/tools/fixtures/source_sine/process-environment.json").to_owned(),
                    |path| std::fs::read_to_string(path).unwrap(),
                );
                let receipt: benilla_sim::source_sine::ProcessReceipt =
                    serde_json::from_str(&text).unwrap();
                let binding = benilla_sim::source_sine::DispatchBinding::from_process_receipt(
                    14,
                    &receipt.source_acquisition_bindings,
                    &receipt,
                )
                .unwrap();
                Owner {
                    reference_lunar: Some(ReferenceLunarEventOwner::new(
                        &mut calendar,
                        &mut trace,
                        moon::SineOrder::SolarFirst,
                        binding,
                    )),
                    ..Owner::default()
                }
            } else {
                Owner {
                    lunar: Some(LunarEventBindings {
                        calendar: &mut calendar,
                        sine: &mut sine,
                        trace: &mut trace,
                    }),
                    ..Owner::default()
                }
            };
            let today = integer(&case["epoch"]);
            let mut data = lunar_rows();
            if let Some(row) = stock {
                data.maximum = Some(row.id);
                data.definitions = VecDeque::from([row.clone()]);
                data.dates.clear();
            }
            let id = stock.map_or(1, |row| row.id);
            owner.wall(EventClockSite::LoadSchedule(id), today);
            let manager = manager(&mut data, &mut owner);
            let definition = &manager.inventory().definitions[usize::from(id)];
            assert_eq!(
                (definition.start, definition.end),
                boundaries.unwrap(),
                "case {}",
                case["case"]
            );
            assert!(owner
                .observations
                .contains(&GameEventObservation::Calendar {
                    event: id,
                    today,
                    start: definition.start,
                    end: definition.end,
                }));
            drop(owner);
            let counts = trace.iter().fold([0; 3], |mut counts, record| {
                match record {
                    moon::Evidence::Calendar(moon::CalendarCall::Now { .. }) => counts[0] += 1,
                    moon::Evidence::Calendar(moon::CalendarCall::Local { .. }) => counts[1] += 1,
                    moon::Evidence::Calendar(moon::CalendarCall::Epoch { .. }) => counts[2] += 1,
                    _ => {}
                }
                counts
            });
            assert_eq!(counts, [4, 10, 6]);
        }
        assert_eq!(count, 204);
    }

    #[test]
    fn retained_stock_lunar_rows_compute_with_bound_real_sine() {
        let Some(catalog) = std::env::var_os("BENILLA_F2_CATALOG") else {
            println!("SKIP unsealed stock lunar rows: BENILLA_F2_CATALOG absent");
            return;
        };
        let Some(occurrences) = std::env::var_os("BENILLA_F2_EVENT_OCCURRENCES") else {
            println!("SKIP unsealed stock lunar rows: BENILLA_F2_EVENT_OCCURRENCES absent");
            return;
        };
        if std::env::var_os("BENILLA_LUNAR_PROCESS_RECEIPT").is_none() {
            println!("SKIP unsealed stock lunar rows: actual process receipt absent");
            return;
        }
        let mut catalog = super::super::super::production_compiler::VerifiedCatalog::open(
            Path::new(&catalog),
            false,
        )
        .unwrap();
        let mut rows = catalog
            .execution_event_rows(Path::new(&occurrences))
            .unwrap();
        assert_eq!(rows.maximum().unwrap(), Some(1024));
        assert_eq!(rows.maximum().unwrap(), None);
        let mut ids = Vec::new();
        while let Some(row) = rows.definition().unwrap() {
            if row.schedule != 12 {
                continue;
            }
            compare_lunar_event_boundaries(true, Some(&row));
            ids.push(row.id);
        }
        assert_eq!(ids, [7]);
        println!("Unsealed catalog-04 lunar row 7 computes all 204 authored boundaries with real sine and bound process receipt; calendar responses and clocks remain authored component inputs, not production admission");
    }
}
