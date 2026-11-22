// SPDX-License-Identifier: GPL-3.0-or-later
use super::EmeraldDragonSummon;
use super::{
    refuse, Result, SourceWorldClock, WorldStartupObservation, WorldStartupSql, WorldStartupWork,
    WorldTailOwner,
};
use std::collections::BTreeSet;

pub type WorldStateRow<'a> = (u32, Option<&'a [u8]>);

// WorldState.cpp:573-578 assigns entries in mask order.
const EMERALD_DRAGONS: [u32; 4] = [14887, 14888, 14889, 14890];

#[derive(Default)]
struct AhnQiraj {
    phase: u32,
    timer_ms: u64,
    counters: [u32; 30],
    phase2_tier: u32,
    killed_bosses: u32,
    spawned_guids: BTreeSet<u32>,
}

struct ScourgeInvasion {
    state: u32,
    timers_ms: [i64; 8],
    battles_won: u32,
    last_attack_zone: Option<u32>,
    remaining: [u32; 6],
}

pub(super) struct SourceWorldState {
    dragon_mask: u8,
    dragon_timer_ms: u32,
    dragon_positions: [u32; 4],
    aq: AhnQiraj,
    love_counters: [u32; 8],
    invasion: ScourgeInvasion,
}

impl Default for SourceWorldState {
    fn default() -> Self {
        // WorldState.cpp:95-97,1058-1060; WorldState.h:204-217,286-292,455.
        Self {
            dragon_mask: 0xF,
            dragon_timer_ms: 0,
            dragon_positions: [0; 4],
            aq: AhnQiraj::default(),
            love_counters: [0; 8],
            invasion: ScourgeInvasion {
                state: 0,
                timers_ms: [0; 8],
                battles_won: 0,
                // CreationPolicy.h:35-38 does not zero fields omitted by WorldState.cpp:1058.
                last_attack_zone: None,
                remaining: [0; 6],
            },
        }
    }
}

impl SourceWorldState {
    // WorldState.cpp:108-214 visits rows in result order. Field.h:49-52 reads a C string.
    pub(super) fn load_rows(
        &mut self,
        rows: &[WorldStateRow<'_>],
        clock: &SourceWorldClock,
    ) -> Result<()> {
        for &(id, raw) in rows {
            let raw = raw.unwrap_or_default();
            let text = raw.split(|&byte| byte == 0).next().unwrap_or_default();
            if text.is_empty() {
                // WorldState.cpp:127,145,171,183-184,188 leaves only Love reset on empty text.
                if id == 2 {
                    self.love_counters.fill(0);
                }
                continue;
            }
            let mut stream = SaveStream {
                rest: text,
                failed: false,
            };
            match id {
                // WorldState.cpp:123-141, WorldState.h:441-443.
                0 => {
                    let mask = stream.u32().unwrap_or(0xF);
                    let respawn_s = stream.unsigned(u64::MAX);
                    self.dragon_mask = mask as u8;
                    stream.fill(&mut self.dragon_positions);
                    let respawn_s = respawn_s.ok_or_else(|| {
                        refuse("execution.world.WorldState_134.uninitialized_dragon_time")
                    })?;
                    self.dragon_timer_ms = if respawn_s == 0 {
                        0
                    } else {
                        remaining_millis(
                            respawn_s,
                            clock,
                            "execution.world.WorldState_136.dragon_clock_overflow",
                        )? as u32
                    };
                }
                // WorldState.cpp:143-166, WorldState.h:204-217.
                1 => {
                    stream.assign(&mut self.aq.phase);
                    let respawn_s = stream.unsigned(u64::MAX).ok_or_else(|| {
                        refuse("execution.world.WorldState_152.uninitialized_aq_time")
                    })?;
                    if respawn_s != 0 {
                        self.aq.timer_ms = remaining_millis(
                            respawn_s,
                            clock,
                            "execution.world.WorldState_154.aq_clock_overflow",
                        )? as u64;
                    }
                    stream.fill(&mut self.aq.counters);
                    stream.assign(&mut self.aq.phase2_tier);
                    stream.assign(&mut self.aq.killed_bosses);
                }
                // WorldState.cpp:169-185, WorldState.h:306-321.
                2 => stream.fill(&mut self.love_counters),
                // WorldState.cpp:186-210, WorldState.h:222-228,242-264,286-292.
                3 => {
                    self.invasion.state = stream.u32().ok_or_else(|| {
                        refuse("execution.world.WorldState_193.uninitialized_invasion_state")
                    })?;
                    for timer in &mut self.invasion.timers_ms {
                        let time = stream.unsigned(u64::MAX).ok_or_else(|| {
                            refuse("execution.world.WorldState_198.uninitialized_invasion_time")
                        })?;
                        *timer = epoch_millis(
                            time,
                            "execution.world.WorldState_198.invasion_clock_overflow",
                        )?;
                    }
                    stream.assign(&mut self.invasion.battles_won);
                    if let Some(zone) = stream.u32() {
                        self.invasion.last_attack_zone = Some(zone);
                    }
                    stream.fill(&mut self.invasion.remaining);
                }
                _ => (),
            }
        }
        Ok(())
    }

    pub(super) fn war_effort(&self) -> (u32, u32) {
        (self.aq.phase, self.aq.killed_bosses)
    }

    // WorldState.cpp:730-737,848-869 schedules a pass in AQResources enum order.
    pub(super) fn war_effort_pass(&mut self, resource: usize) -> Option<WarEffortPass> {
        if self.aq.phase > 2 {
            return None;
        }
        let &(team, group) = RESOURCES.get(resource)?;
        let (counter, goal) = self.aq.group_counter_and_goal(team, group);
        let computed = resource_tier(counter, goal);
        let tier = if self.aq.phase == 2 && self.aq.phase2_tier != u32::MAX {
            self.aq.phase2_tier.min(5) as usize
        } else {
            computed
        };
        let mut pass = WarEffortPass {
            map: team as u32,
            guids: [0; 10],
            len: 0,
        };
        for brand in &WAR_EFFORT_GUIDS {
            for guids in &brand[..tier] {
                let guid = guids[group];
                // Fixed table entries are unique. Insert all before traversing any maps.
                if self.aq.spawned_guids.insert(guid) {
                    pass.guids[pass.len] = guid;
                    pass.len += 1;
                }
            }
        }
        Some(pass)
    }

    // WorldState.cpp:217-222 resumes the invasion only in STATE_1_ENABLED.
    pub(super) fn resume_scourge_invasion(&self) -> Result<()> {
        if self.invasion.state == 1 {
            return Err(refuse(
                "execution.world.WorldState_219.scourge_invasion_owner",
            ));
        }
        Ok(())
    }

    // WorldState.cpp:595 subtracts in uint32 and shifts a 32-bit int.
    fn is_dragon_spawned(&self, entry: u32) -> Result<bool> {
        let bit = 1_u32
            .checked_shl(entry.wrapping_sub(EMERALD_DRAGONS[0]))
            .ok_or_else(|| refuse("execution.world.WorldState_595.dragon_shift_undefined"))?;
        Ok(bit & u32::from(self.dragon_mask) == 0)
    }

    // WorldState.cpp:232-241 adds the timer to the cached clock before to_time_t.
    fn emerald_dragons_insert(&self, clock_ms: impl FnOnce() -> i64) -> Result<WorldStartupSql> {
        let epoch_seconds = if self.dragon_timer_ms == 0 {
            0
        } else {
            let respawn_ms = clock_ms()
                .checked_add(i64::from(self.dragon_timer_ms))
                .ok_or_else(|| refuse("execution.world.WorldState_235.dragon_clock_overflow"))?;
            // Common.h:58-59 and libstdc++ chrono.h convert through native nanoseconds.
            let respawn_ns = respawn_ms
                .checked_mul(1_000_000)
                .ok_or_else(|| refuse("execution.world.WorldState_236.dragon_clock_overflow"))?;
            (respawn_ns / 1_000_000_000) as u64
        };
        Ok(WorldStartupSql::InsertEmeraldDragons {
            mask: self.dragon_mask,
            epoch_seconds,
            positions: self.dragon_positions,
        })
    }
}

pub(super) fn respawn_emerald_dragons(
    owner: &mut impl WorldTailOwner,
    clock: &SourceWorldClock,
    work: &mut impl WorldStartupWork,
) -> Result<()> {
    // WorldState.cpp:600-611 writes each choice before removing it from the remaining IDs.
    if owner.ready_world()?.world_state.dragon_mask == 0xF {
        let mut ids = EMERALD_DRAGONS;
        for (slot, upper) in (1..ids.len()).rev().enumerate() {
            let pick = owner.world_dragon_urand(upper as u32)? as usize;
            let remaining = &mut ids[..=upper];
            let entry = remaining
                .get(pick)
                .copied()
                .ok_or_else(|| refuse("execution.world.WorldState_606.dragon_roll_out_of_range"))?;
            owner.ready_world()?.world_state.dragon_positions[slot] = entry;
            remaining.copy_within(pick + 1.., pick);
        }
        let state = &mut owner.ready_world()?.world_state;
        state.dragon_positions[3] = ids[0];
        state.dragon_mask = 0;
        let insert = state.emerald_dragons_insert(|| clock.current_ms())?;
        // WorldState.cpp:242-243 sends the delete before the insert.
        work.observe(WorldStartupObservation::Sql(
            WorldStartupSql::DeleteEmeraldDragons,
        ))?;
        work.observe(WorldStartupObservation::Sql(insert))?;
    }

    // WorldState.cpp:613-629 visits map 0 then 1 and checks each live mask bit before summoning.
    let summons = [
        (
            0,
            [
                (0, "execution.world.WorldState_617.dragon_summon_owner"),
                (1, "execution.world.WorldState_621.dragon_summon_owner"),
            ],
        ),
        (
            1,
            [
                (2, "execution.world.WorldState_626.dragon_summon_owner"),
                (3, "execution.world.WorldState_628.dragon_summon_owner"),
            ],
        ),
    ];
    for (map_id, summons) in summons {
        let mut after = None;
        while let Some(map) = owner.event_next_map(Some(map_id), after)? {
            for (slot, site) in summons {
                let state = &owner.ready_world()?.world_state;
                let entry = state.dragon_positions[slot];
                if !state.is_dragon_spawned(entry)? {
                    continue;
                }
                if slot != 0 {
                    return Err(refuse(site));
                }
                // WorldState.cpp:582,590,617 uses the Twilight Grove pose and path.
                if owner
                    .summon_emerald_dragon(
                        map,
                        EmeraldDragonSummon {
                            entry,
                            position: [-10428.8, -392.176, 43.7411, 0.932375],
                            path_id: 1,
                        },
                    )?
                    .is_some()
                {
                    return Err(refuse(
                        "execution.world.WorldState_618.dragon_waypoint_owner",
                    ));
                }
            }
            after = Some(map);
        }
    }
    Ok(())
}

// WorldState.h:139-184 and WorldState.cpp:939-975 map resource ordinals to (map, group).
const RESOURCES: [(usize, usize); 30] = [
    (1, 4),
    (1, 3),
    (1, 2),
    (1, 1),
    (1, 4),
    (1, 0),
    (1, 2),
    (1, 1),
    (1, 0),
    (1, 3),
    (0, 0),
    (0, 1),
    (0, 0),
    (0, 4),
    (0, 3),
    (0, 2),
    (0, 3),
    (0, 1),
    (0, 2),
    (0, 4),
    (0, 2),
    (0, 4),
    (0, 0),
    (0, 3),
    (0, 1),
    (1, 2),
    (1, 4),
    (1, 0),
    (1, 3),
    (1, 1),
];

// WorldState.cpp:765-777 in map order, then AQResourceGroup order.
const GROUP_RESOURCES: [[[usize; 3]; 5]; 2] = [
    [
        [22, 10, 12],
        [11, 17, 24],
        [15, 18, 20],
        [14, 16, 23],
        [13, 19, 21],
    ],
    [[5, 8, 27], [3, 7, 29], [2, 6, 25], [1, 9, 28], [0, 4, 26]],
];

// WorldState.cpp:66-93 gives goals in AQResources order, with five shared goals.
const RESOURCE_GOALS: [u32; 25] = [
    96000, 10000, 22000, 250000, 19000, 60000, 18000, 250000, 60000, 10000, 180000, 800000, 110000,
    33000, 14000, 28000, 20000, 600000, 24000, 20000, 45000, 13000, 40000, 8500, 200000,
];

// WorldState.cpp:759,779-831 lists Alliance first, then Horde, tier and group order.
const WAR_EFFORT_GUIDS: [[[u32; 5]; 5]; 2] = [
    [
        [155010, 155011, 155012, 155013, 155014],
        [155020, 155021, 155022, 155023, 155024],
        [155030, 155031, 155032, 155033, 155034],
        [155040, 155041, 155042, 155043, 155044],
        [155050, 155051, 155052, 155053, 155054],
    ],
    [
        [155510, 155511, 155512, 155513, 155514],
        [155520, 155521, 155522, 155523, 155524],
        [155530, 155531, 155532, 155533, 155534],
        [155540, 155541, 155542, 155543, 155544],
        [155550, 155551, 155552, 155553, 155554],
    ],
];

pub(super) struct WarEffortPass {
    pub(super) map: u32,
    guids: [u32; 10],
    len: usize,
}

impl WarEffortPass {
    pub(super) fn guids(&self) -> &[u32] {
        &self.guids[..self.len]
    }
}

impl AhnQiraj {
    // WorldState.cpp:977-988 sums the three unsigned counters and shifts shared goals.
    fn group_counter_and_goal(&self, team: usize, group: usize) -> (u32, u32) {
        let mut counter = 0_u32;
        let mut goal = 0;
        for resource in GROUP_RESOURCES[team][group] {
            counter = counter.wrapping_add(self.counters[resource]);
            let goal_index = if resource > 24 {
                resource - 5
            } else {
                resource
            };
            goal += RESOURCE_GOALS[goal_index];
        }
        (counter, goal)
    }
}

// WorldState.cpp:833-846 tests strict, integer thresholds from highest to lowest.
fn resource_tier(counter: u32, goal: u32) -> usize {
    for (tier, percent) in [(5, 90), (4, 70), (3, 50), (2, 30), (1, 10)] {
        if counter > goal * percent / 100 {
            return tier;
        }
    }
    0
}

// WorldState.cpp:691-710 dispatches event starts for active war effort phases.
pub(super) fn start_war_effort_event(
    phase: u32,
    killed_bosses: u32,
    mut start: impl FnMut(u16) -> Result<()>,
) -> Result<()> {
    if let 1..=5 = phase {
        start(119 + phase as u16)?;
        if phase == 4 {
            for (bit, id) in [(1, 125), (2, 126), (4, 127)] {
                if killed_bosses & bit != 0 {
                    start(id)?;
                }
            }
        }
    }
    Ok(())
}

fn epoch_millis(seconds: u64, undefined: &'static str) -> Result<i64> {
    // Common.h:58-59 and libstdc++ chrono.h:1257-1261 convert through native nanoseconds.
    (seconds as i64)
        .checked_mul(1_000_000_000)
        .map(|ns| ns / 1_000_000)
        .ok_or_else(|| refuse(undefined))
}

fn remaining_millis(
    seconds: u64,
    clock: &SourceWorldClock,
    undefined: &'static str,
) -> Result<i64> {
    // World.h:581 returns the existing cached clock.
    epoch_millis(seconds, undefined)?
        .checked_sub(clock.current_ms())
        .ok_or_else(|| refuse(undefined))
}

struct SaveStream<'a> {
    rest: &'a [u8],
    failed: bool,
}

impl SaveStream<'_> {
    // WorldState.cpp:120 uses a classic-locale istringstream. A failed extraction may assign
    // zero or saturate once; later extractions leave their destination unchanged.
    fn unsigned(&mut self, max: u64) -> Option<u64> {
        if self.failed {
            return None;
        }
        while let Some((&(b' ' | b'\t'..=b'\r'), tail)) = self.rest.split_first() {
            self.rest = tail;
        }
        let (&first, digits) = self.rest.split_first()?;
        let (negative, digits) = match first {
            b'-' => (true, digits),
            b'+' => (false, digits),
            _ => (false, self.rest),
        };
        let mut value = 0u64;
        let mut len = 0;
        for &byte in digits.iter().take_while(|b| b.is_ascii_digit()) {
            len += 1;
            let digit = u64::from(byte - b'0');
            if value > (max - digit) / 10 {
                self.failed = true;
                return Some(max);
            }
            value = value * 10 + digit;
        }
        self.rest = &digits[len..];
        self.failed = len == 0;
        Some(if negative {
            value.wrapping_neg() & max
        } else {
            value
        })
    }

    fn u32(&mut self) -> Option<u32> {
        self.unsigned(u64::from(u32::MAX)).map(|value| value as u32)
    }

    fn assign(&mut self, target: &mut u32) {
        if let Some(value) = self.u32() {
            *target = value;
        }
    }

    fn fill(&mut self, fields: &mut [u32]) {
        for field in fields {
            self.assign(field);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emerald_dragons_mask_accepts_the_largest_defined_shift() {
        let world = SourceWorldState::default();
        // WorldState.cpp:595 checks the low four bits and permits a shift of 31.
        assert!(!world.is_dragon_spawned(14887).unwrap());
        assert!(world.is_dragon_spawned(14918).unwrap());
    }

    #[test]
    fn emerald_dragons_mask_refuses_undefined_shifts() {
        let world = SourceWorldState::default();
        // WorldState.cpp:595 subtracts in uint32 before shifting a 32-bit int.
        for entry in [14919, 14886] {
            assert_eq!(
                world.is_dragon_spawned(entry).unwrap_err().id,
                "execution.world.WorldState_595.dragon_shift_undefined"
            );
        }
    }

    #[test]
    fn emerald_dragons_save_uses_loaded_timer_and_positions() {
        let mut world = SourceWorldState::default();
        let clock = SourceWorldClock::default();
        // WorldState.cpp:232-241 uses the cached clock and the stored timer.
        world
            .load_rows(&[(0, Some(b"2 17 14887 14888 14889 14890"))], &clock)
            .unwrap();
        assert_eq!(
            world.emerald_dragons_insert(|| clock.current_ms()).unwrap(),
            WorldStartupSql::InsertEmeraldDragons {
                mask: 2,
                epoch_seconds: 17,
                positions: [14887, 14888, 14889, 14890],
            }
        );
    }

    #[test]
    fn emerald_dragons_save_refuses_overflow_before_writing() {
        let mut world = SourceWorldState::default();
        let clock = SourceWorldClock::default();
        world
            .load_rows(&[(0, Some(b"15 1 0 0 0 0"))], &clock)
            .unwrap();
        // WorldState.cpp:235-236 adds 1000 ms then casts through native nanoseconds.
        let last_ms = i64::MAX / 1_000_000;
        assert!(world.emerald_dragons_insert(|| last_ms - 1000).is_ok());
        assert_eq!(
            world
                .emerald_dragons_insert(|| last_ms - 999)
                .unwrap_err()
                .id,
            "execution.world.WorldState_236.dragon_clock_overflow"
        );
        assert_eq!(
            world.emerald_dragons_insert(|| i64::MAX).unwrap_err().id,
            "execution.world.WorldState_235.dragon_clock_overflow"
        );
    }

    #[test]
    fn world_state_load_restores_all_four_saved_row_types() {
        let mut world = SourceWorldState::default();
        let clock = SourceWorldClock::default();
        assert_eq!(world.dragon_mask, 0xF);
        assert_eq!(world.dragon_timer_ms, 0);
        assert_eq!(world.dragon_positions, [0; 4]);
        assert_eq!(world.war_effort(), (0, 0));
        assert_eq!(world.invasion.last_attack_zone, None);
        world
            .load_rows(
                &[
                    (0, Some(b"256 4294968 14890 14888 14887 14889")),
                    (
                        1,
                        Some(b"4 2 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 5 7"),
                    ),
                    (2, Some(b"10 20 30 40 50 60 70 80\0discard")),
                    (3, Some(b"1 0 1 2 3 4 5 6 7 9 139 1 2 3 4 5 6")),
                    (4, Some(b"ignored")),
                ],
                &clock,
            )
            .unwrap();
        assert_eq!(world.dragon_mask, 0);
        assert_eq!(world.dragon_timer_ms, 704);
        assert_eq!(world.dragon_positions, [14890, 14888, 14887, 14889]);
        assert_eq!(world.war_effort(), (4, 7));
        assert_eq!(world.aq.timer_ms, 2_000);
        assert_eq!(world.aq.counters, core::array::from_fn(|i| i as u32 + 1));
        assert_eq!(world.aq.phase2_tier, 5);
        assert_eq!(world.love_counters, [10, 20, 30, 40, 50, 60, 70, 80]);
        assert_eq!(world.invasion.state, 1);
        assert_eq!(
            world.invasion.timers_ms,
            [0, 1000, 2000, 3000, 4000, 5000, 6000, 7000]
        );
        assert_eq!(world.invasion.battles_won, 9);
        assert_eq!(world.invasion.last_attack_zone, Some(139));
        assert_eq!(world.invasion.remaining, [1, 2, 3, 4, 5, 6]);

        world
            .load_rows(
                &[(0, None), (1, Some(b"4 0")), (2, None), (3, Some(b""))],
                &clock,
            )
            .unwrap();
        assert_eq!(world.dragon_timer_ms, 704);
        assert_eq!(world.aq.timer_ms, 2_000);
        assert_eq!(world.love_counters, [0; 8]);
        assert_eq!(world.invasion.state, 1);
    }

    #[test]
    fn world_state_load_keeps_defined_partial_extraction() {
        let mut world = SourceWorldState::default();
        let clock = SourceWorldClock::default();
        world
            .load_rows(&[(2, Some(b"1 2 3 4 5 6 7 8"))], &clock)
            .unwrap();
        world
            .load_rows(&[(2, Some(b"-1\x0b+9 nope 88"))], &clock)
            .unwrap();
        assert_eq!(world.love_counters, [u32::MAX, 9, 0, 4, 5, 6, 7, 8]);
        world
            .load_rows(&[(2, Some(b"4 4294967296 6"))], &clock)
            .unwrap();
        assert_eq!(world.love_counters, [4, u32::MAX, 0, 4, 5, 6, 7, 8]);
        world
            .load_rows(&[(3, Some(b"1 0 1 2 3 4 5 6 7 9"))], &clock)
            .unwrap();
        assert_eq!(world.invasion.battles_won, 9);
        assert_eq!(world.invasion.last_attack_zone, None);
        world.load_rows(&[(0, Some(b"7 x 99"))], &clock).unwrap();
        assert_eq!(
            (
                world.dragon_mask,
                world.dragon_timer_ms,
                world.dragon_positions
            ),
            (7, 0, [0; 4])
        );
    }

    #[test]
    fn world_state_load_refuses_undefined_times_at_the_source_site() {
        let mut world = SourceWorldState::default();
        let clock = SourceWorldClock::default();
        world
            .load_rows(&[(0, Some(b"7 9223372036"))], &clock)
            .unwrap();
        for (id, text, refusal) in [
            (
                0,
                &b"7"[..],
                "execution.world.WorldState_134.uninitialized_dragon_time",
            ),
            (
                0,
                &b"7 9223372037"[..],
                "execution.world.WorldState_136.dragon_clock_overflow",
            ),
            (
                1,
                &b"4 "[..],
                "execution.world.WorldState_152.uninitialized_aq_time",
            ),
            (
                1,
                &b"4 9223372037"[..],
                "execution.world.WorldState_154.aq_clock_overflow",
            ),
            (
                3,
                &b" "[..],
                "execution.world.WorldState_193.uninitialized_invasion_state",
            ),
            (
                3,
                &b"1 0"[..],
                "execution.world.WorldState_198.uninitialized_invasion_time",
            ),
            (
                3,
                &b"1 9223372037"[..],
                "execution.world.WorldState_198.invasion_clock_overflow",
            ),
        ] {
            assert_eq!(
                world.load_rows(&[(id, Some(text))], &clock).unwrap_err().id,
                refusal
            );
        }
    }

    #[test]
    fn war_effort_passes_visit_both_teams_with_empty_counters() {
        let mut world = SourceWorldState::default();
        let mut maps = [0; 2];
        for pass in (0..).map_while(|resource| world.war_effort_pass(resource)) {
            maps[pass.map as usize] += 1;
            assert!(pass.guids().is_empty());
        }
        assert_eq!(maps, [15, 15]);
        world.aq.phase = 3;
        assert!(world.war_effort_pass(0).is_none());
    }

    #[test]
    fn war_effort_queues_both_brands_and_deduplicates_before_visiting() {
        let mut world = SourceWorldState::default();
        // Horde herbs: 96,000 + 19,000 + the shared 13,000.
        world.aq.counters[0] = 12_800;
        assert!(world.war_effort_pass(0).unwrap().guids().is_empty());
        world.aq.counters[0] += 1;
        let first = world.war_effort_pass(0).unwrap();
        assert_eq!(first.map, 1);
        assert_eq!(first.guids(), [155014, 155514]);
        assert!(world.aq.spawned_guids.contains(&155014));
        assert!(world.aq.spawned_guids.contains(&155514));
        // Alliance herbs could now reach tier one, but share the live dedup set.
        world.aq.counters[13] = 6601;
        let next = world.war_effort_pass(13).unwrap();
        assert_eq!(next.map, 0);
        assert!(next.guids().is_empty());
        world.aq.counters[0] = 12_801;
        world.aq.counters[4] = u32::MAX;
        let wrap = world.war_effort_pass(4).unwrap();
        assert!(wrap.guids().is_empty());
        assert_eq!(world.aq.spawned_guids.len(), 2);
    }

    #[test]
    fn war_effort_transport_uses_forced_tier_except_the_minus_one_sentinel() {
        let mut world = SourceWorldState::default();
        world.aq.phase = 2;
        world.aq.phase2_tier = 2;
        let pass = world.war_effort_pass(0).unwrap();
        assert_eq!(pass.guids(), [155014, 155024, 155514, 155524]);
        world.aq.phase2_tier = u32::MAX;
        assert!(world.war_effort_pass(1).unwrap().guids().is_empty());
        world.aq.counters[1] = 28_501;
        let pass = world.war_effort_pass(1).unwrap();
        assert_eq!(
            pass.guids(),
            [155013, 155023, 155033, 155043, 155053, 155513, 155523, 155533, 155543, 155553]
        );
    }

    #[test]
    fn world_state_starts_war_effort_events_in_source_order() {
        for (phase, bosses, expected) in [
            (0, 7, &[][..]),
            (1, 7, &[120][..]),
            (2, 7, &[121][..]),
            (3, 7, &[122][..]),
            (4, 0, &[123][..]),
            (4, 7, &[123, 125, 126, 127][..]),
            (5, 7, &[124][..]),
            (6, 7, &[][..]),
        ] {
            let mut calls = Vec::new();
            start_war_effort_event(phase, bosses, |id| {
                calls.push(id);
                Ok(())
            })
            .unwrap();
            assert_eq!(calls, expected);
        }
    }
}
