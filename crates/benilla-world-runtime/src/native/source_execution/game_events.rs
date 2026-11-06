// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1, GameEventMgr.cpp:89-765,1162-1346.
// Final spawn, substitution, quest, mail and instance effects belong to F4.11.
use super::super::production_compiler::{EventCatalogRows, VerifiedCatalog};
use super::*;
use benilla_sim::{source_calendar, source_time::IntervalTimer};
use std::collections::{BTreeMap, BTreeSet};
const FAR_FUTURE: i64 = 4_102_444_800;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct GameEventDefinition {
    // Deviation: Empty/never-assigned slots are never SERVERSIDE because native does not reproduce undefined scheduleType reads in CMaNGOS GameEventMgr.h:53-55,63-64 and GameEventMgr.cpp:184-186,1328-1330.
    pub schedule: Option<u32>,
    pub start: i64,
    pub end: i64,
    pub occurrence: u32,
    pub length: u32,
    pub holiday: u32,
    pub linked: u32,
    pub description: Vec<u8>,
}
impl GameEventDefinition {
    fn hole() -> Self {
        Self {
            start: 1,
            ..Self::default()
        }
    }
    fn valid(&self) -> bool {
        self.schedule == Some(0) || self.length > 0
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameEventDefinitionRow {
    pub ordinal: u64,
    pub id: u16,
    pub schedule: u32,
    pub occurrence: u32,
    pub length: u32,
    pub holiday: u32,
    pub linked: u32,
    pub description: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameEventDateRow {
    pub ordinal: u64,
    pub id: u16,
    pub start: Vec<u8>,
    pub end: Vec<u8>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum EventSpawnKind {
    Creature,
    GameObject,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameEventMemberRow {
    pub ordinal: u64,
    pub exists: bool,
    pub guid: u32,
    pub event: i16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct GameEventSubstitution {
    pub guid: u32,
    pub entry: u32,
    pub model: u32,
    pub equipment: u32,
    pub start_spell: u32,
    pub end_spell: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameEventSubstitutionRow {
    pub ordinal: u64,
    pub exists: bool,
    pub event: u16,
    pub data: GameEventSubstitution,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct GameEventMail {
    pub race_mask: u32,
    pub quest: u32,
    pub template: u32,
    pub sender: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameEventMailRow {
    pub ordinal: u64,
    pub event: u16,
    pub mail: GameEventMail,
}

pub trait GameEventRows {
    fn maximum(&mut self) -> Result<Option<u16>>;
    fn definition(&mut self) -> Result<Option<GameEventDefinitionRow>>;
    fn date(&mut self) -> Result<Option<GameEventDateRow>>;
    fn member(&mut self, kind: EventSpawnKind) -> Result<Option<GameEventMemberRow>>;
    fn substitution(&mut self) -> Result<Option<GameEventSubstitutionRow>>;
    fn quest(&mut self) -> Result<Option<(u64, u32, u16)>>;
    fn mail(&mut self) -> Result<Option<GameEventMailRow>>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum EventClockSite {
    LoadSchedule(u16),
    LoadYearly(u16),
    Update,
    NextCheck(u16),
    OverwriteStart(u16),
    OverwriteStop(u16),
    Weekly(u16),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum EventReference {
    Equipment,
    CreatureTemplate,
    Spell,
    Quest,
    MailTemplate,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum GameEventEffect {
    TruncateStatus,
    InsertStatus(u16),
    DeleteStatus(u16),
    Announce(u16),
    Restart {
        seconds: u32,
    },
    Spawn(i16),
    Unspawn(i16),
    Substitutions {
        event: u16,
        active: bool,
    },
    Quests {
        event: u16,
        active: bool,
    },
    Mail(i16),
    Notify {
        event: u16,
        active: bool,
        resume: bool,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum GameEventObservation {
    Definition {
        ordinal: u64,
        event: u16,
    },
    Date {
        ordinal: u64,
        event: u16,
        start: i64,
        end: i64,
        start_parsed: bool,
        end_parsed: bool,
    },
    Calendar {
        event: u16,
        today: i64,
        start: i64,
        end: i64,
    },
    Member {
        kind: EventSpawnKind,
        ordinal: u64,
        event: i16,
        guid: u32,
        pool: u16,
    },
    Decision {
        slot: u8,
        ordinal: u64,
        event: u16,
        reason: &'static str,
    },
    Active {
        event: u16,
        active: bool,
    },
    NextDelay(u32),
    Timer {
        interval: i64,
        current: i64,
    },
}
/// Each reached lookup/effect executes its owner, never a recorded answer.
pub trait GameEventOwner {
    fn clock(&mut self, site: EventClockSite) -> Result<ClockSample>;
    fn lunar_schedule(&mut self, start: i64, end: i64, today: i64) -> Result<(i64, i64)>;
    fn reference(&mut self, kind: EventReference, id: u32) -> Result<bool>;
    fn set_quest_active(&mut self, quest: u32, active: bool) -> Result<()>;
    fn check_pool_event(
        &mut self,
        pool: u16,
        event: i16,
        creatures: &BTreeMap<u32, i16>,
        gameobjects: &BTreeMap<u32, i16>,
    ) -> Result<()>;
    fn effect(
        &mut self,
        manager: &mut SourceGameEventManager,
        effect: GameEventEffect,
    ) -> Result<()>;
    fn observe(&mut self, observation: GameEventObservation) -> Result<()>;
}
fn wall(owner: &mut impl GameEventOwner, site: EventClockSite) -> Result<i64> {
    let sample = owner.clock(site)?;
    require(
        sample.kind() == ClockKind::WallSeconds,
        "execution.event.wall_clock_kind",
    )?;
    Ok(sample.value())
}
fn numeric<T>(result: source_calendar::Result<T>) -> Result<T> {
    result.map_err(|error| {
        refuse(match error {
            source_calendar::Error::Overflow => "execution.event.calendar_overflow",
            source_calendar::Error::YearRange => "execution.event.calendar_year_range",
            source_calendar::Error::LunarOwner => "execution.event.lunar_owner",
        })
    })
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct GameEventInventory {
    pub definitions: Vec<GameEventDefinition>,
    pub active: BTreeSet<u16>,
    pub initial_status: BTreeSet<u16>,
    pub initial_binding: Option<serde_json::Value>,
    pub creatures: Vec<Vec<u32>>,
    pub gameobjects: Vec<Vec<u32>>,
    pub pools: Vec<Vec<u16>>,
    pub substitutions: Vec<Vec<GameEventSubstitution>>,
    pub substitution_events: BTreeMap<u32, Vec<u16>>,
    pub quests: Vec<Vec<u32>>,
    pub quest_active: BTreeMap<u32, bool>,
    pub mails: Vec<Vec<GameEventMail>>,
    pub initialized: bool,
    pub loaded: bool,
    pub stopped: bool,
}
#[derive(Clone, Debug, Default)]
pub struct SourceGameEventManager {
    state: GameEventInventory,
}
impl SourceGameEventManager {
    pub fn inventory(&self) -> &GameEventInventory {
        &self.state
    }
    pub fn load_catalog(
        &mut self,
        catalog: &mut VerifiedCatalog,
        occurrences: &Path,
        snapshot: &Path,
        environment: &Path,
        pools: &mut SourcePoolManager,
        owner: &mut impl GameEventOwner,
    ) -> Result<()> {
        self.run(|this| {
            let binding = VerifiedCatalog::execution_event_initial(snapshot, environment)?;
            let mut rows: EventCatalogRows = catalog.execution_event_rows(occurrences)?;
            this.state.initial_binding = Some(binding);
            this.load(&mut rows, pools, owner)
        })
    }
    pub fn initialize_bound(
        &mut self,
        announce: bool,
        owner: &mut impl GameEventOwner,
    ) -> Result<u32> {
        self.run(|this| {
            let binding = this
                .state
                .initial_binding
                .as_ref()
                .ok_or_else(|| refuse("execution.event.initial_binding_owner"))?;
            let status = binding["events"]
                .as_array()
                .ok_or_else(|| refuse("execution.event.initial_binding_owner"))?
                .iter()
                .map(|n| {
                    n.as_u64()
                        .and_then(|v| u16::try_from(v).ok())
                        .ok_or_else(|| refuse("execution.event.initial_binding_owner"))
                })
                .collect::<Result<Vec<_>>>()?;
            this.initialize(
                (!status.is_empty()).then_some(status.as_slice()),
                announce,
                owner,
            )
        })
    }
    pub fn active_substitution(&self, guid: u32) -> Result<Option<&GameEventSubstitution>> {
        self.ready()?;
        let Some(event) = self
            .state
            .substitution_events
            .get(&guid)
            .and_then(|events| {
                events
                    .iter()
                    .find(|event| self.state.active.contains(event))
            })
        else {
            return Ok(None);
        };
        Ok(self.state.substitutions[usize::from(*event)]
            .iter()
            .find(|data| data.guid == guid))
    }
    pub fn member_event(&self, kind: EventSpawnKind, guid: u32) -> Result<i16> {
        self.ready()?;
        let members = if kind == EventSpawnKind::Creature {
            &self.state.creatures
        } else {
            &self.state.gameobjects
        };
        require(
            members.len() <= usize::from(u16::MAX),
            "execution.event.member_scan_unbounded",
        )?;
        Ok(members
            .iter()
            .position(|list| list.contains(&guid))
            .map_or(0, |slot| {
                (slot as i64 + 1 - self.state.definitions.len() as i64) as i16
            }))
    }
    pub fn pool_event(&self, pool: u16) -> Result<i16> {
        self.ready()?;
        Ok(self
            .state
            .pools
            .iter()
            .position(|list| list.contains(&pool))
            .map_or(0, |slot| slot as i16))
    }
    pub fn start_event(
        &mut self,
        id: u16,
        overwrite: bool,
        resume: bool,
        announce: bool,
        owner: &mut impl GameEventOwner,
    ) -> Result<()> {
        self.run(|this| {
            this.ready()?;
            this.start_inner(id, resume, announce, owner)?;
            if overwrite {
                let start = wall(owner, EventClockSite::OverwriteStart(id))?;
                let data = &mut this.state.definitions[usize::from(id)];
                data.start = start;
                if data.end <= start {
                    data.end = start
                        .checked_add(i64::from(data.length))
                        .ok_or_else(|| refuse("execution.event.time_add_undefined"))?;
                }
            }
            Ok(())
        })
    }
    pub fn stop_event(
        &mut self,
        id: u16,
        overwrite: bool,
        owner: &mut impl GameEventOwner,
    ) -> Result<()> {
        self.run(|this| {
            this.ready()?;
            this.stop_inner(id, owner)?;
            if overwrite {
                let now = wall(owner, EventClockSite::OverwriteStop(id))?;
                let data = &mut this.state.definitions[usize::from(id)];
                data.start = now
                    .checked_sub(i64::from(data.length.wrapping_mul(60)))
                    .ok_or_else(|| refuse("execution.event.time_subtract_undefined"))?;
                if data.end <= data.start {
                    data.end = data
                        .start
                        .checked_add(i64::from(data.length))
                        .ok_or_else(|| refuse("execution.event.time_add_undefined"))?;
                }
            }
            Ok(())
        })
    }
    fn ready(&self) -> Result<()> {
        require(
            self.state.loaded && !self.state.stopped,
            "execution.event.not_loaded_or_stopped",
        )
    }
    fn run<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        require(!self.state.stopped, "execution.event.stopped")?;
        let result = f(self);
        if result.is_err() {
            self.state.stopped = true;
        }
        result
    }
    fn definition(&self, id: u32) -> Result<&GameEventDefinition> {
        self.state
            .definitions
            .get(id as usize)
            .ok_or_else(|| refuse("execution.event.definition_index_undefined"))
    }
    pub fn is_valid(&self, id: u32) -> Result<bool> {
        Ok(self
            .state
            .definitions
            .get(id as usize)
            .is_some_and(GameEventDefinition::valid))
    }
    pub fn active(&self, id: u32) -> bool {
        u16::try_from(id).is_ok_and(|id| self.state.active.contains(&id))
    }
    fn compute(
        &mut self,
        id: u16,
        schedule: u32,
        today: i64,
        owner: &mut impl GameEventOwner,
    ) -> Result<()> {
        let definition = self.definition(u32::from(id))?;
        let (start, end) = if schedule == 12 {
            owner.lunar_schedule(definition.start, definition.end, today)?
        } else {
            numeric(source_calendar::compute_schedule(
                schedule,
                definition.start,
                definition.end,
                today,
            ))?
        };
        let definition = &mut self.state.definitions[id as usize];
        definition.start = start;
        definition.end = end;
        owner.observe(GameEventObservation::Calendar {
            event: id,
            today,
            start,
            end,
        })
    }
    fn decision(
        owner: &mut impl GameEventOwner,
        slot: u8,
        ordinal: u64,
        event: u16,
        reason: &'static str,
    ) -> Result<()> {
        owner.observe(GameEventObservation::Decision {
            slot,
            ordinal,
            event,
            reason,
        })
    }
    pub fn load(
        &mut self,
        rows: &mut impl GameEventRows,
        pools: &mut SourcePoolManager,
        owner: &mut impl GameEventOwner,
    ) -> Result<()> {
        self.run(|this| {
            this.state.loaded = false;
            let Some(maximum) = rows.maximum()? else {
                this.state.loaded = true;
                return Ok(());
            };
            this.state
                .definitions
                .resize_with(usize::from(maximum) + 1, GameEventDefinition::hole);
            let mut any = false;
            while let Some(row) = rows.definition()? {
                any = true;
                if row.id == 0 {
                    Self::decision(owner, 1, row.ordinal, 0, "reserved_event")?;
                    continue;
                }
                require(
                    row.schedule <= 15,
                    "execution.event.schedule_cast_undefined",
                )?;
                let slot = this
                    .state
                    .definitions
                    .get_mut(usize::from(row.id))
                    .ok_or_else(|| refuse("execution.event.definition_index_undefined"))?;
                slot.schedule = Some(row.schedule);
                slot.occurrence = row.occurrence;
                slot.length = row.length;
                slot.holiday = row.holiday;
                slot.linked = row.linked;
                slot.description = row.description;
                match row.schedule {
                    0 => {
                        slot.start = FAR_FUTURE;
                        slot.end = FAR_FUTURE;
                    }
                    2 | 3 | 5 | 6 | 8 | 9 | 12 | 13 => {
                        let today = wall(owner, EventClockSite::LoadSchedule(row.id))?;
                        this.compute(row.id, row.schedule, today, owner)?;
                    }
                    _ => {}
                }
                let slot = &mut this.state.definitions[usize::from(row.id)];
                if slot.occurrence == 0 {
                    slot.start = FAR_FUTURE;
                    slot.occurrence = slot.length;
                }
                if (slot.length == 0 && row.schedule != 0) || slot.occurrence < slot.length {
                    slot.start = FAR_FUTURE;
                }
                owner.observe(GameEventObservation::Definition {
                    ordinal: row.ordinal,
                    event: row.id,
                })?;
            }
            if !any {
                this.state.definitions.clear();
                this.state.loaded = true;
                return Ok(());
            }
            require(
                this.state.definitions.len() <= u16::MAX as usize,
                "execution.event.uint16_scan_unbounded",
            )?;
            for id in 1..this.state.definitions.len() {
                if this.state.definitions[id].valid() {
                    let linked = this.state.definitions[id].linked;
                    if linked != 0 && !this.is_valid(linked)? {
                        this.state.definitions[id].linked = 0;
                    }
                }
            }
            while let Some(row) = rows.date()? {
                let slot = this.definition(u32::from(row.id))?;
                if !matches!(slot.schedule, Some(1 | 11)) {
                    Self::decision(owner, 2, row.ordinal, row.id, "date_schedule_ignored")?;
                    continue;
                }
                if slot.start == FAR_FUTURE {
                    continue;
                }
                let start = numeric(source_calendar::field_time(&row.start))?;
                let end = numeric(source_calendar::field_time(&row.end))?;
                let slot = &mut this.state.definitions[usize::from(row.id)];
                slot.start = start.seconds;
                slot.end = end.seconds;
                owner.observe(GameEventObservation::Date {
                    ordinal: row.ordinal,
                    event: row.id,
                    start: start.seconds,
                    end: end.seconds,
                    start_parsed: start.parsed,
                    end_parsed: end.parsed,
                })?;
                if slot.schedule == Some(11) {
                    let today = wall(owner, EventClockSite::LoadYearly(row.id))?;
                    this.compute(row.id, 11, today, owner)?;
                }
            }
            let count = this.state.definitions.len();
            this.state.pools.resize_with(count, Vec::new);
            this.state.creatures.resize_with(count * 2 - 1, Vec::new);
            this.state.gameobjects.resize_with(count * 2 - 1, Vec::new);
            let mut pool_events = BTreeMap::new();
            let mut creature_events = BTreeMap::new();
            let mut object_events = BTreeMap::new();
            for kind in [EventSpawnKind::Creature, EventSpawnKind::GameObject] {
                while let Some(row) = rows.member(kind)? {
                    if !row.exists
                        || row.event == 0
                        || !this.is_valid(u32::from(row.event.unsigned_abs()))?
                    {
                        Self::decision(
                            owner,
                            if kind == EventSpawnKind::Creature {
                                3
                            } else {
                                4
                            },
                            row.ordinal,
                            row.event as u16,
                            "invalid_member",
                        )?;
                        continue;
                    }
                    let mut pool = 0;
                    if row.event > 0 {
                        (if kind == EventSpawnKind::Creature {
                            &mut creature_events
                        } else {
                            &mut object_events
                        })
                        .insert(row.guid, row.event);
                        pool = pools.top_pool(
                            if kind == EventSpawnKind::Creature {
                                PoolKind::Creature
                            } else {
                                PoolKind::GameObject
                            },
                            row.guid,
                        )?;
                        if pool != 0 {
                            if let std::collections::btree_map::Entry::Vacant(entry) =
                                pool_events.entry(pool)
                            {
                                entry.insert(row.event);
                                this.state.pools[row.event as usize].push(pool);
                                pools.remove_auto_spawn(pool)?;
                            }
                        }
                    }
                    if pool == 0 {
                        let slot = (count as i64 + i64::from(row.event) - 1) as usize;
                        (if kind == EventSpawnKind::Creature {
                            &mut this.state.creatures
                        } else {
                            &mut this.state.gameobjects
                        })[slot]
                            .push(row.guid);
                    }
                    owner.observe(GameEventObservation::Member {
                        kind,
                        ordinal: row.ordinal,
                        event: row.event,
                        guid: row.guid,
                        pool,
                    })?;
                }
            }
            for (pool, event) in pool_events {
                owner.check_pool_event(pool, event, &creature_events, &object_events)?;
            }
            this.state.substitutions.resize_with(count, Vec::new);
            while let Some(row) = rows.substitution()? {
                if !row.exists || row.event == 0 || !this.is_valid(u32::from(row.event))? {
                    Self::decision(owner, 5, row.ordinal, row.event, "invalid_substitution")?;
                    continue;
                }
                let mut data = row.data;
                for (kind, id) in [
                    (EventReference::Equipment, &mut data.equipment),
                    (EventReference::CreatureTemplate, &mut data.entry),
                    (EventReference::Spell, &mut data.start_spell),
                    (EventReference::Spell, &mut data.end_spell),
                ] {
                    if *id != 0 && !owner.reference(kind, *id)? {
                        *id = 0;
                    }
                }
                this.state.substitutions[row.event as usize].push(data);
                this.state
                    .substitution_events
                    .entry(data.guid)
                    .or_default()
                    .push(row.event);
            }
            this.state.quests.resize_with(count, Vec::new);
            while let Some((ordinal, quest, event)) = rows.quest()? {
                if event == 0
                    || !this.is_valid(u32::from(event))?
                    || !owner.reference(EventReference::Quest, quest)?
                {
                    Self::decision(owner, 6, ordinal, event, "invalid_quest")?;
                    continue;
                }
                owner.set_quest_active(quest, false)?;
                this.state.quest_active.insert(quest, false);
                this.state.quests[event as usize].push(quest);
            }
            this.state.mails.resize_with(count * 2 - 1, Vec::new);
            while let Some(row) = rows.mail()? {
                let mail = row.mail;
                if row.event == 0
                    || !this.is_valid(u32::from(row.event))?
                    || mail.race_mask & 0xff == 0
                    || (mail.quest != 0 && !owner.reference(EventReference::Quest, mail.quest)?)
                    || !owner.reference(EventReference::MailTemplate, mail.template)?
                    || !owner.reference(EventReference::CreatureTemplate, mail.sender)?
                {
                    Self::decision(owner, 7, row.ordinal, row.event, "invalid_mail")?;
                    continue;
                }
                this.state.mails[count + usize::from(row.event) - 1].push(mail);
            }
            this.state.loaded = true;
            Ok(())
        })
    }
    fn window(&self, id: u16, now: i64) -> Result<bool> {
        let data = self.definition(u32::from(id))?;
        if !(data.start < now && now < data.end) {
            return Ok(false);
        }
        let modulus = data.occurrence.wrapping_mul(60);
        require(modulus != 0, "execution.event.modulo_zero_undefined")?;
        let elapsed = now
            .checked_sub(data.start)
            .ok_or_else(|| refuse("execution.event.time_subtract_undefined"))?;
        Ok(elapsed % i64::from(modulus) < i64::from(data.length.wrapping_mul(60)))
    }
    pub fn check_one(&self, id: u16, now: i64) -> Result<bool> {
        self.ready()?;
        self.window(id, now)
    }
    pub fn next_check(&mut self, id: u16, owner: &mut impl GameEventOwner) -> Result<u32> {
        self.run(|this| this.next_check_inner(id, owner))
    }
    fn next_check_inner(&self, id: u16, owner: &mut impl GameEventOwner) -> Result<u32> {
        let now = wall(owner, EventClockSite::NextCheck(id))?;
        let data = self.definition(u32::from(id))?;
        if now > data.end {
            return Ok(86400);
        }
        if data.start > now {
            return Ok(data
                .start
                .checked_sub(now)
                .ok_or_else(|| refuse("execution.event.time_subtract_undefined"))?
                as u32);
        }
        let modulus = data.occurrence.wrapping_mul(60);
        require(modulus != 0, "execution.event.modulo_zero_undefined")?;
        let elapsed = now
            .checked_sub(data.start)
            .ok_or_else(|| refuse("execution.event.time_subtract_undefined"))?;
        let remainder = elapsed % i64::from(modulus);
        let length = data.length.wrapping_mul(60);
        let delay = if remainder < i64::from(length) {
            i64::from(length) - remainder
        } else {
            i64::from(modulus) - remainder
        } as u32;
        if data.end
            < now
                .checked_add(i64::from(delay))
                .ok_or_else(|| refuse("execution.event.time_add_undefined"))?
        {
            return Ok(data
                .end
                .checked_sub(now)
                .ok_or_else(|| refuse("execution.event.time_subtract_undefined"))?
                as u32);
        }
        Ok(delay)
    }
    fn signed(id: u16, negative: bool) -> i16 {
        if negative {
            (-(i32::from(id))) as i16
        } else {
            id as i16
        }
    }
    fn start_inner(
        &mut self,
        id: u16,
        resume: bool,
        announce: bool,
        owner: &mut impl GameEventOwner,
    ) -> Result<()> {
        self.definition(u32::from(id))?;
        self.state.active.insert(id);
        owner.observe(GameEventObservation::Active {
            event: id,
            active: true,
        })?;
        if id == 987 {
            let seconds = self.state.definitions[id as usize].length.wrapping_mul(60);
            return owner.effect(self, GameEventEffect::Restart { seconds });
        }
        owner.effect(self, GameEventEffect::InsertStatus(id))?;
        if announce {
            owner.effect(self, GameEventEffect::Announce(id))?;
        }
        for effect in [
            GameEventEffect::Spawn(Self::signed(id, false)),
            GameEventEffect::Unspawn(Self::signed(id, true)),
            GameEventEffect::Substitutions {
                event: id,
                active: true,
            },
            GameEventEffect::Quests {
                event: id,
                active: true,
            },
        ] {
            owner.effect(self, effect)?;
        }
        for quest in &self.state.quests[id as usize] {
            self.state.quest_active.insert(*quest, true);
        }
        if !resume {
            owner.effect(self, GameEventEffect::Mail(Self::signed(id, false)))?;
        }
        owner.effect(
            self,
            GameEventEffect::Notify {
                event: id,
                active: true,
                resume,
            },
        )
    }
    fn stop_inner(&mut self, id: u16, owner: &mut impl GameEventOwner) -> Result<()> {
        self.definition(u32::from(id))?;
        self.state.active.remove(&id);
        owner.observe(GameEventObservation::Active {
            event: id,
            active: false,
        })?;
        for effect in [
            GameEventEffect::DeleteStatus(id),
            GameEventEffect::Unspawn(Self::signed(id, false)),
            GameEventEffect::Spawn(Self::signed(id, true)),
            GameEventEffect::Substitutions {
                event: id,
                active: false,
            },
            GameEventEffect::Quests {
                event: id,
                active: false,
            },
        ] {
            owner.effect(self, effect)?;
        }
        for quest in &self.state.quests[id as usize] {
            self.state.quest_active.insert(*quest, false);
        }
        owner.effect(self, GameEventEffect::Mail(Self::signed(id, true)))?;
        owner.effect(
            self,
            GameEventEffect::Notify {
                event: id,
                active: false,
                resume: false,
            },
        )?;
        Ok(())
    }
    fn update_inner(
        &mut self,
        resume: Option<&BTreeSet<u16>>,
        announce: bool,
        owner: &mut impl GameEventOwner,
    ) -> Result<u32> {
        self.ready()?;
        let now = wall(owner, EventClockSite::Update)?;
        let mut delay = 86400;
        for id in 1..self.state.definitions.len() {
            let id = id as u16;
            let data = self.definition(u32::from(id))?;
            let serverside = data.schedule == Some(0);
            if (data.occurrence == 0 && !serverside) || (serverside && self.state.initialized) {
                continue;
            }
            if self.window(id, now)? {
                if !self.active(u32::from(id)) && (data.linked == 0 || self.active(data.linked)) {
                    self.start_inner(id, resume.is_some_and(|r| r.contains(&id)), announce, owner)?;
                }
            } else if self.active(u32::from(id)) {
                self.stop_inner(id, owner)?;
            } else if !self.state.initialized {
                owner.effect(self, GameEventEffect::Spawn(Self::signed(id, true)))?;
            }
            delay = delay.min(self.next_check(id, owner)?);
        }
        let delay = (delay + 1) * 1000;
        owner.observe(GameEventObservation::NextDelay(delay))?;
        Ok(delay)
    }
    pub fn initialize(
        &mut self,
        status: Option<&[u16]>,
        announce: bool,
        owner: &mut impl GameEventOwner,
    ) -> Result<u32> {
        self.run(|this| {
            this.ready()?;
            this.state.active.clear();
            require(
                status.is_none_or(|rows| !rows.is_empty()),
                "execution.event.empty_query_result",
            )?;
            let status: BTreeSet<_> = status.into_iter().flatten().copied().collect();
            this.state.initial_status = status.clone();
            if !status.is_empty() {
                owner.effect(this, GameEventEffect::TruncateStatus)?;
            }
            let delay = this.update_inner(Some(&status), announce, owner)?;
            this.state.initialized = true;
            Ok(delay)
        })
    }
    pub fn update(&mut self, announce: bool, owner: &mut impl GameEventOwner) -> Result<u32> {
        self.run(|this| this.update_inner(None, announce, owner))
    }
    pub fn update_world_timer(
        &mut self,
        timer: &mut IntervalTimer,
        announce: bool,
        owner: &mut impl GameEventOwner,
    ) -> Result<bool> {
        self.run(|this| {
            if !timer.passed() {
                return Ok(false);
            }
            timer
                .reset()
                .map_err(|_| refuse("execution.event.timer_overflow"))?;
            owner.observe(GameEventObservation::Timer {
                interval: timer.get_interval(),
                current: timer.get_current(),
            })?;
            let delay = this.update_inner(None, announce, owner)?;
            timer.set_interval(i64::from(delay));
            timer
                .reset()
                .map_err(|_| refuse("execution.event.timer_overflow"))?;
            owner.observe(GameEventObservation::Timer {
                interval: timer.get_interval(),
                current: timer.get_current(),
            })?;
            Ok(true)
        })
    }
    pub fn weekly_recalculate(&mut self, owner: &mut impl GameEventOwner) -> Result<()> {
        self.run(|this| {
            this.ready()?;
            for id in 0..this.state.definitions.len() {
                if let Some(schedule @ (2 | 3 | 5 | 6 | 8 | 9 | 11 | 12 | 13)) =
                    this.state.definitions[id].schedule
                {
                    let today = wall(owner, EventClockSite::Weekly(id as u16))?;
                    this.compute(id as u16, schedule, today, owner)?;
                }
            }
            Ok(())
        })
    }
}
