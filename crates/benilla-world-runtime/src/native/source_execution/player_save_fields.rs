// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::source_rows::{CharacterRow, FloatWord};
use super::*;

// Player.h:374 and Taxi.h:67-71.
const PLAYER_FLAGS_RESTING: u32 = 0x20;
const TRACKER_FLIGHT: u8 = 3;
// UpdateFields.h:179 and Player.h:370-378.
const PLAYER_FLAGS: usize = 0xbe;
const STATS_RESET_FLAGS: u32 = 0x02 | 0x04 | 0x08 | 0x10 | 0x80 | 0x0200;

/// Live Player fields introduced with their constructor owners.
pub(super) struct PlayerSaveFields {
    // Object.cpp:105-108 clears the Player update words.
    xp: u32,
    flags: u32,
    explored_zones: [u32; 64],
    ammo: u32,
    // Player.cpp:535,537-538,550.
    pub(super) at_login: u32,
    pub(super) teleport_near: bool,
    pub(super) teleport_far: bool,
    pub(super) cinematic: u32,
    // Player.cpp:563,593,601-602,608,645.
    death_expire_time: i64,
    rest_bonus: f32,
    reset_talents_cost: u32,
    reset_talents_time: i64,
    stable_slots: u8,
    fishing_steps: u8,
    // Object.h:288,753-754 and Object.cpp:1170.
    transport_pose: [f32; 4],
    transport: Option<ObjectHandle>,
    // Player.cpp:471, Taxi.h:67-79 starts the tracker at TRACKER_EMPTY.
    pub(super) taxi_state: u8,
    // Player.cpp:597 and Player.h:791-792.
    pub(super) mail_dirty: bool,
    pub(super) bg_dirty: bool,
}

impl Default for PlayerSaveFields {
    fn default() -> Self {
        Self {
            xp: 0,
            flags: 0,
            explored_zones: [0; 64],
            ammo: 0,
            at_login: 0,
            teleport_near: false,
            teleport_far: false,
            cinematic: 0,
            death_expire_time: 0,
            rest_bonus: 0.0,
            reset_talents_cost: 0,
            reset_talents_time: 0,
            stable_slots: 0,
            fishing_steps: 0,
            transport_pose: [0.0; 4],
            transport: None,
            taxi_state: 0,
            mail_dirty: false,
            bg_dirty: false,
        }
    }
}

impl SourcePlayer {
    // Relations.cpp:168,358 both read the live PLAYER_FLAGS word.
    pub(in crate::native::source_execution) fn player_flags(&self) -> u32 {
        self.save_fields.flags
    }

    // Player.cpp:2932 clears transient flags during stat initialization.
    pub(super) fn clear_stat_player_flags(&mut self, objects: &SourceObjects) -> Result<()> {
        objects.update_flags(
            self.object,
            PLAYER_FLAGS,
            &mut self.save_fields.flags,
            STATS_RESET_FLAGS,
            false,
        )
    }

    // Player.cpp:15643-15758. SqlPreparedStatement.h:255 clears the stream after each text bind.
    pub(super) fn character_row(
        &self,
        inputs: &mut SourcePlayerSaveInputs<'_, impl WorldStartupWork>,
        object: &SourceObject,
        guid: u32,
    ) -> Result<CharacterRow> {
        let live = &self.save_fields;
        let account = inputs.session.account_id(); // Player.cpp:15644, WorldSession.h:244.
        let unit = inputs.units.get(inputs.objects, self.object)?;
        let class = unit
            .unit_class
            .and_then(|v| u8::try_from(v).ok())
            .ok_or_else(|| refuse("execution.player_save.Player_15647.class_owner"))?;
        let power = unit
            .power_fields
            .as_ref()
            .ok_or_else(|| refuse("execution.player_save.Player_15649.level_owner"))?;
        let level = u8::try_from(power.level)
            .map_err(|_| refuse("execution.player_save.Player_15649.level_width"))?;
        require(
            !live.teleport_near,
            "execution.player_save.Player_15666.teleport_pose_owner",
        )?;
        let map = object
            .map
            .ok_or_else(|| refuse("execution.player_save.Player_15658.map_owner"))?
            .map;
        let pose = object
            .pose
            .ok_or_else(|| refuse("execution.player_save.Player_15659.pose_owner"))?;
        let [x, y, z, orientation] = pose.0.map(f32::from_bits).map(finite_word);
        let online = object.in_world; // Player.cpp:15677.
        let cinematic = u8::try_from(live.cinematic)
            .map_err(|_| refuse("execution.player_save.Player_15679.cinematic_width"))?;
        let logout_time = (inputs.wall_seconds)()? as u64; // Player.cpp:15685.
        let transport_guid = match live.transport {
            None => 0,
            Some(handle) => u64::from(
                inputs
                    .objects
                    .get(handle)?
                    .guid
                    .ok_or_else(|| {
                        refuse("execution.player_save.Player_15699.transport_guid_owner")
                    })?
                    .counter(),
            ),
        };
        require(
            !online,
            "execution.player_save.Player_15709.in_world_zone_owner",
        )?;
        let zone = object.zone_update_id;
        // Taxi.cpp:42-46 only saves an active flight, whose route and spline owners are absent.
        require(
            live.taxi_state < TRACKER_FLIGHT,
            "execution.player_save.Taxi_46.taxi_route_owner",
        )?;
        let vitals = unit
            .vitals
            .as_ref()
            .ok_or_else(|| refuse("execution.player_save.Player_15727.vitals_owner"))?;
        let [rage, focus, happiness] = power
            .auxiliary_current
            .ok_or_else(|| refuse("execution.player_save.Player_15730.power_owner"))?;
        Ok(CharacterRow {
            guid,
            account,
            name: self.name.clone(),
            race: self.race,
            class,
            gender: self.gender,
            level,
            xp: live.xp,
            money: self.money,
            player_bytes: u32::from_le_bytes(self.bytes),
            player_bytes2: u32::from_le_bytes(self.bytes_2),
            player_flags: live.flags,
            map,
            xyz: [x, y, z],
            orientation,
            taxi_mask: self.taxi_mask,
            online,
            cinematic,
            total_time: self.played_time[0],
            level_time: self.played_time[1],
            rest_bonus: finite_word(live.rest_bonus),
            logout_time,
            is_logout_resting: live.flags & PLAYER_FLAGS_RESTING != 0,
            reset_talents_cost: live.reset_talents_cost,
            reset_talents_time: live.reset_talents_time as u64,
            transport_pose: live.transport_pose.map(finite_word),
            transport_guid,
            extra_flags: self.extra_flags,
            stable_slots: live.stable_slots,
            at_login: live.at_login,
            zone,
            death_expire_time: live.death_expire_time as u64,
            taxi_path: Vec::new(),
            honor_highest_rank: self.honor.highest_rank(),
            honor_standing: self.honor.standing as u32,
            stored_honor_rating: finite_word(self.honor.stored),
            stored_dishonorable_kills: self.honor.stored_kills[0] as i32,
            stored_honorable_kills: self.honor.stored_kills[1] as i32,
            watched_faction: self.watched_faction as u32,
            drunk: (self.bytes_3 & 0xfffe) as u16,
            health: vitals.health,
            powers: [vitals.mana, rage, focus, vitals.energy, happiness],
            explored_zones: live.explored_zones,
            equipment_cache: self.equipment_cache(inputs.items)?,
            ammo_id: live.ammo,
            action_bars: self.field_bytes[2],
            fishing_steps: live.fishing_steps,
        })
    }

    // Player.cpp:15738-15752. SetVisibleItemSlot (10173-10187) copies the placed item's entry and enchant IDs.
    fn equipment_cache(&self, items: &SourceItems) -> Result<[(u32, u32); 20]> {
        let mut cache = [(0, 0); 20];
        for (slot, (value, item)) in cache.iter_mut().zip(&self.items).enumerate() {
            if let Some(handle) = *item {
                let item = items.get(handle)?;
                let enchants = if slot < 19 {
                    u32::from(item.enchantment_id(0)? as u16)
                        | (u32::from(item.enchantment_id(1)? as u16) << 16)
                } else {
                    0 // Player.cpp:15749-15750 writes no enchantments for the first bag.
                };
                *value = (item.entry, enchants);
            }
        }
        Ok(cache)
    }
}

// Common.h:96 implements finiteAlways for FLOAT binds.
fn finite_word(value: f32) -> FloatWord {
    FloatWord::from_float(if value.is_finite() { value } else { 0.0 })
}
