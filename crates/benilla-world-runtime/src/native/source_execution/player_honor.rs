// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::*;
use crate::native::source_execution::world::{date_today, last_maintenance_day};

// UpdateFields.h:244,252-262.
const PLAYER_FIELD_BYTES: usize = 0x4c6;
const PLAYER_FIELD_SESSION_KILLS: usize = 0x4e2;
const PLAYER_FIELD_YESTERDAY_KILLS: usize = 0x4e3;
const PLAYER_FIELD_LAST_WEEK_KILLS: usize = 0x4e4;
const PLAYER_FIELD_THIS_WEEK_KILLS: usize = 0x4e5;
const PLAYER_FIELD_THIS_WEEK_CONTRIBUTION: usize = 0x4e6;
const PLAYER_FIELD_LIFETIME_HONORABLE_KILLS: usize = 0x4e7;
const PLAYER_FIELD_LIFETIME_DISHONORABLE_KILLS: usize = 0x4e8;
const PLAYER_FIELD_YESTERDAY_CONTRIBUTION: usize = 0x4e9;
const PLAYER_FIELD_LAST_WEEK_CONTRIBUTION: usize = 0x4ea;
const PLAYER_FIELD_LAST_WEEK_RANK: usize = 0x4eb;
const PLAYER_FIELD_BYTES2: usize = 0x4ec;

// Player.h:326-360.
const HONORABLE: u8 = 1;
const DISHONORABLE: u8 = 2;
const HK_NEW: u8 = 0;
const HK_OLD: u8 = 1;
const HK_DELETED: u8 = 2;
struct HonorCp {
    date: u32,
    points: f32,
    kind: u8,
    state: u8,
    is_kill: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct HonorRank {
    rank: u8,
    visual_rank: i8,
    max: f32,
    min: f32,
    positive: bool,
}

impl Default for HonorRank {
    // Formulas.h:43-50.
    fn default() -> Self {
        Self {
            rank: 0,
            visual_rank: 0,
            max: 2000.0,
            min: 0.0,
            positive: true,
        }
    }
}

// Player.cpp:529,6584-6592 initializes the honor state. Object.cpp:105-108 clears the update words.
#[derive(Default)]
pub(super) struct PlayerHonor {
    cp: Vec<HonorCp>,
    rank: HonorRank,
    highest: HonorRank,
    rank_points: Option<f32>,
    pub(super) stored: f32,
    pub(super) stored_kills: [u32; 2], // Dishonorable, honorable.
    pub(super) standing: i32,
    standing_word: u32,
    bar: u8,
    today: [u16; 2], // Honorable, dishonorable.
    kills: [u32; 3], // Yesterday, this week, last week.
    contribution: [u32; 3],
    lifetime: [u32; 2], // Honorable, dishonorable.
}

impl PlayerHonor {
    // Player.cpp:15716 binds the internal rank ID.
    pub(super) fn highest_rank(&self) -> u32 {
        u32::from(self.highest.rank)
    }
}

impl SourcePlayer {
    // Player.cpp:15997-16026. Only NEW contributions write rows.
    pub(super) fn save_honor_cp(&self) -> Result<()> {
        require(
            !self.honor.cp.iter().any(|cp| cp.state == HK_NEW),
            "execution.player_save.Player_16010.honor_cp_owner",
        )
    }

    // Player.cpp:6453-6573.
    pub fn update_honor(
        &mut self,
        objects: &SourceObjects,
        world: &SourceWorld,
        calendar: Calendar,
        maintenance_weekday: i32,
        work: &mut impl WorldStartupWork,
    ) -> Result<()> {
        let seconds = calendar.world_seconds()?;
        let today = date_today(seconds, work)?;
        let yesterday = today.wrapping_sub(1);
        let this_week = last_maintenance_day(seconds, maintenance_weekday, work)?;
        let this_week_end = this_week.wrapping_add(7);
        let last_week = this_week.wrapping_sub(7);
        let last_week_end = last_week.wrapping_add(7);
        let honor = &mut self.honor;
        let [mut total_dishonorable, mut total_honorable] = honor.stored_kills;
        let mut today_honorable = 0u32;
        let mut today_dishonorable = 0u32;
        let mut kills = [0u32; 3];
        let mut contribution = [0.0; 3];
        for cp in &mut honor.cp {
            if cp.state == HK_DELETED {
                continue;
            }
            if cp.kind == HONORABLE {
                total_honorable = total_honorable.wrapping_add(u32::from(cp.is_kill));
                if cp.is_kill && cp.date == today {
                    today_honorable = today_honorable.wrapping_add(1);
                }
                for (index, counted) in [
                    cp.date == yesterday,
                    (this_week..=this_week_end).contains(&cp.date),
                    (last_week..last_week_end).contains(&cp.date),
                ]
                .into_iter()
                .enumerate()
                {
                    if counted {
                        kills[index] = kills[index].wrapping_add(u32::from(cp.is_kill));
                        contribution[index] += cp.points;
                    }
                }
            } else if cp.is_kill && cp.kind == DISHONORABLE {
                total_dishonorable = total_dishonorable.wrapping_add(1);
                if cp.date == today {
                    today_dishonorable = today_dishonorable.wrapping_add(1);
                }
                if cp.date > today {
                    cp.state = HK_OLD;
                }
            }
        }

        let guid = objects
            .get(self.object)?
            .guid
            .ok_or_else(|| refuse("execution.player_save.Player_6525.honor_guid_owner"))?
            .counter();
        // ObjectMgr.cpp:3894-3913,3931-3944 looks up the first matching team entry.
        let (alliance, horde) = world.honor_standings();
        let team = if self.team == 67 { horde } else { alliance };
        let entry = team.iter().enumerate().find(|(_, row)| row.guid == guid);
        honor.standing = entry.map_or(0, |(index, _)| (index as i32).wrapping_add(1));
        let mut rank_points = honor.stored;
        if let Some((_, entry)) = entry {
            rank_points += entry.rp_earning;
        }
        let rank_points = *honor.rank_points.insert(rank_points);
        honor.rank = calculate_rank(rank_points)?;
        let rank = honor.rank;
        if rank.visual_rank > 0 && rank.visual_rank > honor.highest.visual_rank {
            honor.highest = rank;
        }
        let points = checked_u32(
            rank_points.abs(),
            "execution.player_save.Player_6544.honor_rank_points_cast_undefined",
        )?;
        let sign = if rank.positive { 255.0 } else { -255.0 };
        let bar = ((points as f32 - rank.min) / (rank.max - rank.min) * sign).trunc();
        require(
            (-128.0..=127.0).contains(&bar),
            "execution.player_save.Player_6545.honor_bar_cast_undefined",
        )?;
        let object = self.object;
        objects.set_update_value(object, PLAYER_FIELD_BYTES2, &mut honor.bar, bar as i8 as u8)?;
        let patent = (self.bytes_3 & 0x00ff_ffff) | (u32::from(rank.rank) << 24);
        objects.set_update_value(object, PLAYER_BYTES_3, &mut self.bytes_3, patent)?;
        objects.set_update_value(
            object,
            PLAYER_FIELD_SESSION_KILLS,
            &mut honor.today[0],
            today_honorable as u16,
        )?;
        objects.set_update_value(
            object,
            PLAYER_FIELD_SESSION_KILLS,
            &mut honor.today[1],
            today_dishonorable as u16,
        )?;
        for (index, (kill_field, cp_field, overflow)) in [
            (
                PLAYER_FIELD_YESTERDAY_KILLS,
                PLAYER_FIELD_YESTERDAY_CONTRIBUTION,
                "execution.player_save.Player_6558.honor_points_cast_undefined",
            ),
            (
                PLAYER_FIELD_THIS_WEEK_KILLS,
                PLAYER_FIELD_THIS_WEEK_CONTRIBUTION,
                "execution.player_save.Player_6561.honor_points_cast_undefined",
            ),
            (
                PLAYER_FIELD_LAST_WEEK_KILLS,
                PLAYER_FIELD_LAST_WEEK_CONTRIBUTION,
                "execution.player_save.Player_6564.honor_points_cast_undefined",
            ),
        ]
        .into_iter()
        .enumerate()
        {
            objects.set_update_value(object, kill_field, &mut honor.kills[index], kills[index])?;
            let points = checked_u32(contribution[index].max(0.0), overflow)?;
            objects.set_update_value(object, cp_field, &mut honor.contribution[index], points)?;
        }
        objects.set_update_value(
            object,
            PLAYER_FIELD_LAST_WEEK_RANK,
            &mut honor.standing_word,
            honor.standing as u32,
        )?;
        objects.set_update_value(
            object,
            PLAYER_FIELD_LIFETIME_HONORABLE_KILLS,
            &mut honor.lifetime[0],
            total_honorable,
        )?;
        objects.set_update_value(
            object,
            PLAYER_FIELD_LIFETIME_DISHONORABLE_KILLS,
            &mut honor.lifetime[1],
            total_dishonorable,
        )?;
        objects.set_update_value(
            object,
            PLAYER_FIELD_BYTES,
            &mut self.field_bytes[3],
            honor.highest.rank,
        )?;
        Ok(())
    }
}

// Formulas.h:52-103. Player.h:362-364 defines the rank counts.
fn calculate_rank(points: f32) -> Result<HonorRank> {
    if points == 0.0 {
        return Ok(HonorRank::default());
    }
    let positive = points > 0.0;
    let points = points.abs();
    let rank = if points < 2000.0 {
        if positive {
            5
        } else {
            4
        }
    } else if points > if positive { 60_000.0 } else { 15_000.0 } {
        if positive {
            18
        } else {
            1
        }
    } else {
        let steps = checked_u32(
            points / 5000.0,
            "execution.player_save.Formulas_95.honor_rank_cast_undefined",
        )? as u8;
        if positive {
            steps + 6
        } else {
            3 - steps
        }
    };
    if rank == 0 {
        return Ok(HonorRank::default());
    }
    let offset = f32::from(rank) - if positive { 5.0 } else { 4.0 };
    let max = (offset * 5000.0).abs();
    let min = if max > 5000.0 { max - 5000.0 } else { 2000.0 };
    let visual_rank = if rank > 4 {
        rank as i8 - 4
    } else {
        -(rank as i8)
    };
    Ok(HonorRank {
        rank,
        visual_rank,
        max,
        min,
        positive,
    })
}

fn checked_u32(value: f32, id: &'static str) -> Result<u32> {
    let value = value.trunc();
    require((0.0..4_294_967_296.0).contains(&value), id)?;
    Ok(value as u32)
}

#[cfg(test)]
#[path = "player_honor_tests.rs"]
mod tests;
