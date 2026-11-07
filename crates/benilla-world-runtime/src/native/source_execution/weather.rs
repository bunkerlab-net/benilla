// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! Map-owned weather, actual player membership, and explicit F3 clock/RNG owners.
use super::super::production_compiler::VerifiedCatalog;
use super::identity_table::IdentityTable;
use super::*;
use benilla_sim::{
    source_calendar,
    source_time::ShortIntervalTimer,
    source_weather::{Chances, Context, Random, Weather},
};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WeatherChanceRow {
    pub ordinal: u64,
    pub zone: u32,
    pub seasons: [[u32; 3]; 4],
}
#[derive(Clone, Debug, Default)]
pub struct SourceWeatherDefinitions {
    rows: BTreeMap<u32, (u64, Chances)>,
}
impl SourceWeatherDefinitions {
    pub fn load(catalog: &mut VerifiedCatalog, occurrences: &Path) -> Result<Self> {
        let mut rows = catalog.execution_weather_rows(occurrences)?;
        let mut this = Self::default();
        while let Some(row) = rows.next_weather()? {
            this.insert(row);
        }
        Ok(this)
    }
    /// Last duplicate definition replaces the seasons; Weather.cpp:454-480.
    pub fn insert(&mut self, row: WeatherChanceRow) {
        self.rows
            .insert(row.zone, (row.ordinal, Chances::corrected(row.seasons)));
    }
    pub fn len(&self) -> usize {
        self.rows.len()
    }
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// Borrow an existing logical thread stream and World game-time snapshot owner.
pub struct WeatherExecution<'a, R> {
    pub random: &'a mut R,
    pub world_calendar: &'a Calendar,
}
impl<R: Random<Error = benilla_sim::source_rng::Error>> Random for WeatherExecution<'_, R> {
    type Error = Refusal;
    fn urand(&mut self, low: u32, high: u32) -> Result<u32> {
        self.random
            .urand(low, high)
            .map_err(|_| refuse("execution.weather.random_owner"))
    }
    fn rand_norm_f(&mut self) -> Result<f32> {
        self.random
            .rand_norm_f()
            .map_err(|_| refuse("execution.weather.random_owner"))
    }
}
impl<R: Random<Error = benilla_sim::source_rng::Error>> Context for WeatherExecution<'_, R> {
    fn year_day(&mut self) -> Result<u32> {
        let seconds = self.world_calendar.world_seconds()?;
        let calendar = source_calendar::localtime(seconds)
            .map_err(|_| refuse("execution.weather.localtime_null_undefined"))?;
        Ok(calendar.yday as u32)
    }
}

/// GetZoneId is a fresh terrain query, not m_zoneUpdateId; Object.cpp:1227-1229.
pub trait WeatherPacketOwner {
    fn zone_id(&mut self, player: ObjectHandle, objects: &SourceObjects) -> Result<u32>;
    fn send_weather(&mut self, player: ObjectHandle, payload: [u8; 13]) -> Result<()>;
}
/// Surround the weather point with the real Player::UpdateZone owners; Player.cpp:6833-6858.
pub trait PlayerZoneWeatherOwner: WeatherPacketOwner {
    fn zone_exists(&mut self, zone: u32) -> Result<bool>;
    fn before_zone_weather(&mut self, player: ObjectHandle, old: u32, zone: u32) -> Result<()>;
    fn after_zone_weather(&mut self, player: ObjectHandle, zone: u32, area: u32) -> Result<()>;
}
#[derive(Clone, Debug)]
struct ZoneWeather {
    state: Weather,
    timer: ShortIntervalTimer,
    definition: Option<(u64, Chances)>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WeatherZoneInventory {
    pub zone: u32,
    pub kind: u32,
    pub grade_bits: u32,
    pub permanent: bool,
    pub interval: u32,
    pub current: u32,
    pub definition_ordinal: Option<u64>,
    pub chances: Option<[[u32; 3]; 4]>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WeatherInventory {
    pub map: MapIdentity,
    pub enabled: bool,
    pub interval: u32,
    pub buckets: usize,
    pub stopped: bool,
    pub zones_in_source_order: Vec<WeatherZoneInventory>,
}
pub struct SourceMapWeather {
    map: MapIdentity,
    enabled: bool,
    interval: u32,
    zones: IdentityTable<ZoneWeather>,
    pub(super) stopped: bool,
}
impl SourceMapWeather {
    pub fn new(map: MapIdentity, enabled: bool, interval: u32) -> Result<Self> {
        require(
            map.creation_generation > 0,
            "execution.weather.map_generation",
        )?;
        Ok(Self {
            map,
            enabled,
            interval,
            zones: IdentityTable::default(),
            stopped: false,
        })
    }
    pub fn map(&self) -> MapIdentity {
        self.map
    }
    pub fn inventory(&self) -> WeatherInventory {
        WeatherInventory {
            map: self.map,
            enabled: self.enabled,
            interval: self.interval,
            buckets: self.zones.bucket_count(),
            stopped: self.stopped,
            zones_in_source_order: self
                .zones
                .iter()
                .map(|(zone, v)| WeatherZoneInventory {
                    zone: zone as u32,
                    kind: v.state.kind,
                    grade_bits: v.state.grade_bits,
                    permanent: v.state.permanent,
                    interval: v.timer.get_interval(),
                    current: v.timer.get_current(),
                    definition_ordinal: v.definition.map(|v| v.0),
                    chances: v.definition.map(|v| v.1 .0),
                })
                .collect(),
        }
    }
    fn find_or_create(
        &mut self,
        zone: u32,
        definitions: &SourceWeatherDefinitions,
    ) -> Result<&mut ZoneWeather> {
        require(!self.stopped, "execution.weather.stopped")?;
        if self.zones.get(u64::from(zone)).is_none() {
            let mut timer = ShortIntervalTimer::default();
            timer.set_interval(self.interval);
            // Construction does not regenerate or draw; Weather.cpp:49-57,358-367.
            self.zones.insert(
                u64::from(zone),
                ZoneWeather {
                    state: Weather::default(),
                    timer,
                    definition: definitions.rows.get(&zone).copied(),
                },
                100_000,
            )?;
        }
        self.zones
            .get_mut(u64::from(zone))
            .ok_or_else(|| refuse("execution.weather.zone_owner"))
    }
    /// Run at the source UpdateZone call, including disabled-weather zone bookkeeping.
    #[allow(clippy::too_many_arguments)]
    pub fn player_zone_changed(
        &mut self,
        player: ObjectHandle,
        zone: u32,
        area: u32,
        force: bool,
        objects: &mut SourceObjects,
        definitions: &SourceWeatherDefinitions,
        owner: &mut impl PlayerZoneWeatherOwner,
    ) -> Result<()> {
        let result = (|| {
            require(!self.stopped, "execution.weather.stopped")?;
            let object = objects.get(player)?;
            require(
                object.kind == SourceObjectKind::Player && object.map == Some(self.map),
                "execution.weather.player_map",
            )?;
            if !owner.zone_exists(zone)? {
                return Ok(());
            }
            if object.zone_update_id != zone || force {
                owner.before_zone_weather(player, object.zone_update_id, zone)?;
                if self.enabled {
                    let payload = self.find_or_create(zone, definitions)?.state.packet();
                    owner.send_weather(player, payload)?;
                }
            }
            objects.get_mut(player)?.zone_update_id = zone;
            owner.after_zone_weather(player, zone, area)
        })();
        self.stopped |= result.is_err();
        result
    }
    /// Script calls do not depend on CONFIG_BOOL_WEATHER; Weather.cpp:241-250.
    #[allow(clippy::too_many_arguments)]
    pub fn set_weather(
        &mut self,
        zone: u32,
        kind: u32,
        grade: f32,
        permanent: bool,
        definitions: &SourceWeatherDefinitions,
        objects: &SourceObjects,
        owner: &mut impl WeatherPacketOwner,
    ) -> Result<()> {
        let result = (|| {
            let weather = self.find_or_create(zone, definitions)?;
            if weather.state.set(kind, grade, permanent) {
                let payload = weather.state.packet();
                Self::send_zone(self.map, zone, payload, objects, owner)?;
            }
            Ok(())
        })();
        self.stopped |= result.is_err();
        result
    }
    fn send_zone(
        map: MapIdentity,
        zone: u32,
        payload: [u8; 13],
        objects: &SourceObjects,
        owner: &mut impl WeatherPacketOwner,
    ) -> Result<bool> {
        let mut found = false;
        // Iterate map references without an IsInWorld filter; Map.cpp:1527-1538.
        for player in objects.map_players(map) {
            if owner.zone_id(player, objects)? == zone {
                owner.send_weather(player, payload)?;
                found = true;
            }
        }
        Ok(found)
    }
    /// Map-end dispatch does not test enabled again; Map.cpp:1030; Weather.cpp:61-77,371-385.
    pub fn update(
        &mut self,
        diff: u32,
        objects: &SourceObjects,
        context: &mut impl Context<Error = Refusal>,
        owner: &mut impl WeatherPacketOwner,
    ) -> Result<()> {
        let result = (|| {
            require(!self.stopped, "execution.weather.stopped")?;
            // Erase invalidates only the current node; Weather.cpp:374-384.
            let mut current = self.zones.first_key();
            while let Some(zone) = current {
                current = self.zones.next_key(zone);
                let weather = self
                    .zones
                    .get_mut(zone)
                    .ok_or_else(|| refuse("execution.weather.zone_owner"))?;
                weather.timer.update(diff);
                if weather.timer.passed() {
                    weather.timer.reset();
                    if weather
                        .state
                        .regenerate(weather.definition.as_ref().map(|v| &v.1), context)?
                    {
                        let payload = weather.state.packet();
                        if !Self::send_zone(self.map, zone as u32, payload, objects, owner)? {
                            self.zones.erase(zone);
                        }
                    }
                }
            }
            Ok(())
        })();
        self.stopped |= result.is_err();
        result
    }
}
