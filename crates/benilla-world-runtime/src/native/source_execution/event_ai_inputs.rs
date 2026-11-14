// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! CreatureEventAIMgr.cpp:211-1098. Loader skips are not handler coverage.
use super::*;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventAiRow {
    pub ordinal: u64,
    /// SELECT columns at CreatureEventAIMgr.cpp:211-216, after GetUInt32.
    pub words: [u32; 24],
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EventAiAction {
    pub kind: u16,
    /// Invalid opcodes become NONE before parameters are read (:567-572).
    pub params: Option<[u32; 3]>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EventAiDefinition {
    pub ordinal: u64,
    pub id: u32,
    pub entry: u32,
    pub guid: u32,
    pub kind: u32,
    pub inverse_phase: u32,
    pub chance: u8,
    pub flags: u32,
    pub params: [u32; 6],
    pub actions: [EventAiAction; 3],
    pub friendly_hp_target_self: Option<bool>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum EventAiLookup {
    Creature,
    Guid,
    Condition,
    Quest,
    EmoteText,
    Broadcast,
    Faction,
    Display,
    Sound,
    Emote,
    Map,
    Area,
    Summon,
    Relay,
    RelayTemplate,
    TextTemplate,
    SpellList,
    ImmunitySet,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventAiSpell {
    pub school: u32,
    pub targets: u32,
    pub attributes_ex: u32,
    pub attributes_ex3: u32,
    pub range_index: u32,
    pub effects: [u32; 3],
    pub target_a: [u32; 3],
    pub target_b: [u32; 3],
}
impl EventAiSpell {
    fn only_self(&self) -> bool {
        // SpellMgr.h:924-948 returns early at the first empty effect.
        for i in 0..3 {
            if self.effects[i] == 0 {
                return true;
            }
            if self.target_a[i] != 1 || !matches!(self.target_b[i], 0 | 1) {
                return false;
            }
        }
        true
    }
}
pub trait EventAiLoaderOwner {
    fn contains(&mut self, domain: EventAiLookup, id: u32, entry: u32) -> Result<bool>;
    fn spell(&mut self, id: u32) -> Result<Option<EventAiSpell>>;
    fn quest_exploration(&mut self, id: u32) -> Result<Option<bool>>;
    fn assistance_radius(&mut self) -> Result<u32>;
    fn dev_log_filter(&self) -> bool;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct EventAiNotice {
    /// Pinned CreatureEventAIMgr.cpp logging site; slot is one-based.
    pub line: u32,
    pub slot: Option<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventAiDisposition {
    Loaded(EventAiDefinition),
    Skipped { line: u32 },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventAiLoad {
    pub disposition: EventAiDisposition,
    pub notices: Vec<EventAiNotice>,
}
fn notice(notices: &mut Vec<EventAiNotice>, line: u32, slot: Option<u8>) {
    notices.push(EventAiNotice { line, slot });
}
fn log_if(notices: &mut Vec<EventAiNotice>, condition: bool, line: u32, slot: Option<u8>) {
    if condition {
        notice(notices, line, slot);
    }
}
fn skip(mut notices: Vec<EventAiNotice>, line: u32) -> EventAiLoad {
    notice(&mut notices, line, None);
    EventAiLoad {
        disposition: EventAiDisposition::Skipped { line },
        notices,
    }
}
fn check_target(notices: &mut Vec<EventAiNotice>, event: u32, action: u16, target: u32, slot: u8) {
    // CreatureEventAIMgr.cpp:125-199. Even false only logs, never skips.
    let s = Some(slot);
    match target {
        0 => log_if(notices, matches!(action, 15 | 16 | 26 | 33), 133, s),
        1..=5 | 8 | 9 => {
            log_if(
                notices,
                matches!(target, 4 | 5) && matches!(action, 15 | 16 | 26 | 33),
                140,
                s,
            );
            log_if(notices, matches!(event, 1 | 10 | 21), 152, s);
            log_if(notices, event == 29, 155, s);
        }
        6 | 7 => log_if(
            notices,
            !matches!(event, 4..=6 | 8 | 10 | 14..=16 | 22 | 30 | 34),
            177,
            s,
        ),
        10 => log_if(notices, !matches!(event, 8 | 30 | 34), 183, s),
        11..=18 => {}
        _ => notice(notices, 197, s),
    }
}
/// Row conversion/validation is independent of later executable event/action dispatch.
pub fn load_event_ai_row(
    owner: &mut impl EventAiLoaderOwner,
    row: EventAiRow,
) -> Result<EventAiLoad> {
    use EventAiLookup::*;
    let w = row.words;
    let key = w[1] as i32;
    let mut notices = Vec::new();
    let (entry, guid) = if key > 0 {
        if !owner.contains(Creature, key as u32, 0)? {
            return Ok(skip(notices, 241));
        }
        (key as u32, 0)
    } else {
        let guid = key
            .checked_neg()
            .ok_or_else(|| refuse("execution.event_ai.loader.negate_int_min_undefined"))?
            as u32;
        if !owner.contains(Guid, guid, 0)? {
            return Ok(skip(notices, 251));
        }
        (0, guid)
    };
    if w[2] >= 43 {
        return Ok(skip(notices, 260));
    }
    let mut event = EventAiDefinition {
        ordinal: row.ordinal,
        id: w[0],
        entry,
        guid,
        kind: w[2],
        inverse_phase: w[3],
        chance: benilla_sim::event_ai::loaded_chance(w[4]),
        flags: w[5],
        params: w[6..12].try_into().expect("six event parameters"),
        actions: std::array::from_fn(|_| EventAiAction {
            kind: 0,
            params: None,
        }),
        friendly_hp_target_self: None,
    };
    log_if(&mut notices, w[4] as u8 == 0, 273, None);
    log_if(&mut notices, w[4] as u8 > 100, 277, None);
    let p = &mut event.params;
    let mut repeat = None;
    match event.kind {
        0 | 1 | 29 => {
            log_if(&mut notices, p[1] < p[0], 288, None);
            repeat = Some((p[2], p[3], 290));
        }
        2 | 3 | 12 | 18 | 31 => {
            log_if(&mut notices, p[0] > 100, 298, None);
            log_if(&mut notices, p[0] <= p[1], 301, None);
            if event.flags & 1 != 0 && p[2] == 0 && p[3] == 0 {
                notice(&mut notices, 305, None);
                event.flags &= !1;
            }
        }
        8 | 34 => {
            if p[0] != 0 {
                let Some(spell) = owner.spell(p[0])? else {
                    return Ok(skip(notices, if event.kind == 8 { 315 } else { 532 }));
                };
                // GetSchoolMask is 1 << school, SharedDefines.h. Undefined shifts refuse here.
                let mask = 1u32
                    .checked_shl(spell.school)
                    .ok_or_else(|| refuse("execution.event_ai.loader.school_shift_undefined"))?;
                log_if(
                    &mut notices,
                    p[1] & mask != mask,
                    if event.kind == 8 { 320 } else { 537 },
                    None,
                );
            }
            log_if(
                &mut notices,
                p[1] == 0,
                if event.kind == 8 { 324 } else { 541 },
                None,
            );
            repeat = Some((p[2], p[3], if event.kind == 8 { 327 } else { 544 }));
        }
        9 => {
            log_if(&mut notices, p[1] < p[0], 331, None);
            repeat = Some((p[2], p[3], 333));
        }
        10 => {
            if p[5] != 0 && !owner.contains(Condition, p[5], 0)? {
                notice(&mut notices, 338, None);
                p[5] = 0;
            }
            repeat = Some((p[2], p[3], 343));
        }
        11 => match p[0] {
            0 => {}
            1 => log_if(&mut notices, !owner.contains(Map, p[1], 0)?, 352, None),
            2 => log_if(&mut notices, !owner.contains(Area, p[1], 0)?, 356, None),
            _ => notice(&mut notices, 359, None),
        },
        14 | 15 => repeat = Some((p[2], p[3], if event.kind == 14 { 365 } else { 369 })),
        16 => {
            if owner.spell(p[0])?.is_none() {
                return Ok(skip(notices, 376));
            }
            if p[1] == 0 {
                return Ok(skip(notices, 381));
            }
            repeat = Some((p[2], p[3], 385));
        }
        5 | 13 => repeat = Some((p[0], p[1], if event.kind == 5 { 390 } else { 394 })),
        17 | 25 | 26 => {
            log_if(&mut notices, !owner.contains(Creature, p[0], 0)?, 400, None);
            repeat = Some((p[1], p[2], 402));
        }
        19 | 20 => {
            log_if(&mut notices, !owner.contains(Quest, p[0], 0)?, 407, None);
            return Ok(skip(notices, 408));
        }
        6 | 4 | 7 | 21 => {
            if event.kind == 6 && p[0] != 0 && !owner.contains(Condition, p[0], 0)? {
                notice(&mut notices, 415, None);
                p[0] = 0;
            }
            if event.flags & 1 != 0 {
                notice(&mut notices, 425, None);
                event.flags &= !1;
            }
            if event.flags & 0x400 != 0 {
                notice(&mut notices, 431, None);
                event.flags &= !0x400;
            }
        }
        22 => {
            if !owner.contains(EmoteText, p[0], 0)? {
                return Ok(skip(notices, 442));
            }
            if p[1] != 0 && !owner.contains(Condition, p[1], 0)? {
                p[1] = 0;
                return Ok(skip(notices, 448));
            }
            if event.flags & 1 == 0 {
                notice(&mut notices, 455, None);
                event.flags |= 1;
            }
        }
        23 | 24 | 27 | 28 => {
            if owner.spell(p[0])?.is_none() {
                return Ok(skip(notices, 470));
            }
            if p[1] < 1 {
                return Ok(skip(notices, 475));
            }
            repeat = Some((p[2], p[3], 479));
        }
        30 => {
            if p[1] != 0 && !owner.contains(Creature, p[1], 0)? {
                return Ok(skip(notices, 487));
            }
            if p[0] >= 12 {
                return Ok(skip(notices, 493));
            }
        }
        32 | 33 => {
            if event.kind == 33 && p[0] > 1 {
                return Ok(skip(notices, 512));
            }
            if event.flags & 1 != 0 && p[2] == 0 && p[3] == 0 {
                notice(&mut notices, if event.kind == 32 { 502 } else { 518 }, None);
                event.flags &= !1;
            }
            if event.kind == 33 && event.flags & 1 != 0 {
                repeat = Some((p[2], p[3], 523));
            }
        }
        35 | 36 => {}
        37 => {
            if owner.spell(p[0])?.is_none() {
                return Ok(skip(notices, 555));
            }
        }
        _ => notice(&mut notices, 561, None),
    }
    if let Some((low, high, line)) = repeat {
        log_if(&mut notices, high < low, line, None);
    }
    for (index, action) in event.actions.iter_mut().enumerate() {
        let slot = index as u8 + 1;
        let kind = w[12 + index * 4] as u16;
        if kind >= 66 {
            notice(&mut notices, 570, Some(slot));
            continue;
        }
        action.kind = kind;
        let mut p: [u32; 3] = w[13 + index * 4..16 + index * 4]
            .try_into()
            .expect("three action parameters");
        validate_action(owner, event.kind, entry, kind, &mut p, slot, &mut notices)?;
        action.params = Some(p);
    }
    if event.kind == 14 {
        let mut target_self = true;
        for action in &event.actions {
            if action.kind == 11 {
                let p = action.params.expect("CAST parameters initialized");
                if p[1] == 12 {
                    let spell = owner.spell(p[0])?.ok_or_else(|| {
                        refuse("execution.event_ai.loader.friendly_hp_null_spell_undefined")
                    })?;
                    if spell.attributes_ex & 0x80000 != 0 {
                        target_self = false;
                    }
                }
            }
        }
        event.friendly_hp_target_self = Some(target_self);
    }
    Ok(EventAiLoad {
        disposition: EventAiDisposition::Loaded(event),
        notices,
    })
}

fn validate_action(
    owner: &mut impl EventAiLoaderOwner,
    event: u32,
    entry: u32,
    kind: u16,
    p: &mut [u32; 3],
    slot: u8,
    n: &mut Vec<EventAiNotice>,
) -> Result<()> {
    use EventAiLookup::*;
    let s = Some(slot);
    match kind {
        0 => {}
        1 | 44 => {
            if kind == 44 {
                log_if(n, p[0] == 0, 590, s);
                log_if(n, p[0] >= 100, 592, s);
            }
            // :605's not_set assignment is unreachable. Preserve gaps, do not compact TEXT.
            for value in &mut p[usize::from(kind == 44)..] {
                if *value != 0 && !owner.contains(Broadcast, *value, 0)? {
                    notice(n, 609, s);
                    *value = 0;
                }
            }
        }
        2 => {
            if p[0] != 0 && !owner.contains(Faction, p[0], 0)? {
                notice(n, 619, s);
                p[0] = 0;
            }
        }
        3 | 43 => {
            if p[0] != 0 && !owner.contains(Creature, p[0], 0)? {
                notice(n, if kind == 3 { 628 } else { 872 }, s);
                p[0] = 0;
            }
            if p[1] != 0 {
                if p[0] != 0 {
                    notice(n, if kind == 3 { 636 } else { 880 }, s);
                    p[1] = 0;
                } else if !owner.contains(Display, p[1], 0)? {
                    notice(n, if kind == 3 { 641 } else { 885 }, s);
                    p[1] = 0;
                }
            }
        }
        4 | 5 | 9 | 10 => {
            let domain = if matches!(kind, 4 | 9) { Sound } else { Emote };
            let count = if matches!(kind, 9 | 10) { 3 } else { 1 };
            for (i, &value) in p[..count].iter().enumerate() {
                if (i == 0 || value as i32 >= 0) && !owner.contains(domain, value, 0)? {
                    notice(
                        n,
                        match kind {
                            4 => 649,
                            5 => 653,
                            9 => 657 + i as u32 * 2,
                            _ => 665 + i as u32 * 2,
                        },
                        s,
                    );
                }
            }
        }
        11 => {
            let spell = owner.spell(p[0])?;
            if spell.is_none() {
                notice(n, 675, s);
            }
            if p[2] & 0x10 != 0 {
                p[2] |= 2;
            }
            if let Some(spell) = &spell {
                if spell.targets != 0 && matches!(p[1], 15 | 17) {
                    notice(n, 698, s);
                    p[1] = 1;
                }
                if spell.attributes_ex & 0x80000 != 0 && p[1] == 0 {
                    notice(n, 704, s);
                    p[1] = 15;
                }
            }
            check_target(n, event, kind, p[1], slot);
            if !owner.dev_log_filter() {
                if let Some(spell) = &spell {
                    log_if(
                        n,
                        (spell.only_self() || spell.range_index == 1)
                            && p[1] != 0
                            && p[2] & 0x10 == 0,
                        716,
                        s,
                    );
                    log_if(
                        n,
                        p[1] == 6 && spell.effects.iter().any(|e| matches!(e, 2 | 16)),
                        723,
                        s,
                    );
                    log_if(
                        n,
                        spell.attributes_ex3 & 0x100 != 0 && matches!(p[1], 4..=6),
                        728,
                        s,
                    );
                    if spell.targets != 0 && matches!(p[1], 15 | 17) {
                        notice(n, 732, s);
                        p[1] = 1;
                    }
                }
            }
        }
        12 | 32 | 33 => {
            log_if(
                n,
                !owner.contains(Creature, p[0], 0)?,
                match kind {
                    12 => 740,
                    32 => 832,
                    _ => 839,
                },
                s,
            );
            check_target(n, event, kind, p[1], slot);
            if kind == 32 {
                log_if(n, !owner.contains(Summon, p[2], 0)?, 835, s);
            }
        }
        13 | 14 | 23 => {
            let value = p[0] as i32;
            let abs = value
                .checked_abs()
                .ok_or_else(|| refuse("execution.event_ai.loader.abs_int_min_undefined"))?
                as u32;
            match kind {
                13 => {
                    log_if(n, abs > 100 && p[2] == 0, 746, s);
                    check_target(n, event, kind, p[1], slot);
                }
                14 => log_if(n, abs > 100, 751, s),
                _ => {
                    log_if(n, value == 0, 786, s);
                    log_if(n, abs > 31, 788, s);
                }
            }
        }
        15 | 26 => {
            match owner.quest_exploration(p[0])? {
                Some(false) => notice(n, if kind == 15 { 757 } else { 794 }, s),
                None => notice(n, if kind == 15 { 760 } else { 797 }, s),
                Some(true) => {}
            }
            if kind == 15 {
                check_target(n, event, kind, p[1], slot);
            }
        }
        16 | 27 => {
            log_if(
                n,
                !owner.contains(Creature, p[0], 0)?,
                if kind == 16 { 766 } else { 801 },
                s,
            );
            log_if(
                n,
                owner.spell(p[1])?.is_none(),
                if kind == 16 { 768 } else { 803 },
                s,
            );
            if kind == 16 {
                check_target(n, event, kind, p[2], slot);
            }
        }
        17 => {
            log_if(n, p[0] < 6 || p[0] >= 188, 773, s);
            check_target(n, event, kind, p[2], slot);
        }
        18 | 19 | 35 => check_target(n, event, kind, p[1], slot),
        22 => log_if(n, p[0] >= 32, 782, s),
        28 => {
            log_if(n, owner.spell(p[1])?.is_none(), 807, s);
            check_target(n, event, kind, p[0], slot);
        }
        30 => {
            for (i, &phase) in p.iter().enumerate() {
                log_if(n, phase >= 32, 812 + i as u32 * 2, s);
            }
        }
        31 => {
            log_if(n, p[0] >= 32, 820, s);
            log_if(n, p[0] >= 32, 822, s);
            if p[0] >= p[1] {
                notice(n, 825, s);
                p.swap(0, 1);
            }
        }
        34 => log_if(n, p[1] > 4, 844, s),
        36 => log_if(n, !owner.contains(Creature, p[0], 0)?, 851, s),
        40 => {
            if p[0] >= 3 {
                notice(n, 856, s);
                p[0] = 0;
            }
        }
        42 => {
            if p[0] > 1 {
                notice(n, 863, s);
                p[0] = 1;
            }
        }
        6..=8 => notice(n, 905, s),
        45 => {
            if p[0] >= 12 {
                notice(n, 911, s);
                return Ok(());
            }
            log_if(n, p[1] > 533, 915, s);
            if p[1] == 0 {
                notice(n, 919, s);
                p[1] = owner.assistance_radius()?;
            }
            check_target(n, event, kind, p[2], slot);
        }
        46 => log_if(n, p[0] & !0xfff != 0, 927, s),
        47 => log_if(n, p[0] >= 10, 933, s),
        48 => {
            log_if(n, p[0] >= 3, 939, s);
            if p[2] > 3 {
                notice(n, 943, s);
                p[2] = 0;
            }
        }
        50 => log_if(n, p[0] > 2, 950, s),
        52 => log_if(n, p[0] >= 4, 958, s),
        53 => {
            let id = p[0] as i32;
            if id > 0 {
                log_if(n, !owner.contains(Relay, id as u32, 0)?, 967, s);
            } else {
                let id = id.checked_neg().ok_or_else(|| {
                    refuse("execution.event_ai.loader.negate_relay_int_min_undefined")
                })? as u32;
                log_if(n, !owner.contains(RelayTemplate, id, 0)?, 974, s);
            }
        }
        54 => {
            if p[0] != 0 {
                if !owner.contains(Broadcast, p[0], 0)? {
                    notice(n, 984, s);
                    p[0] = 0;
                }
            } else {
                log_if(n, !owner.contains(TextTemplate, p[2], 0)?, 992, s);
            }
        }
        55 => check_target(n, event, kind, p[0], slot),
        56 => {
            if p[0] != 0 && !owner.contains(Creature, p[0], 0)? {
                notice(n, 1003, s);
                p[0] = 0;
            }
        }
        57 => {
            if p[0] >= 5 {
                notice(n, 1010, s);
                p[0] = 0;
            }
            if p[1] > 200 {
                notice(n, 1015, s);
                p[1] = 30;
            }
        }
        58 => {
            if p[0] > 3 {
                notice(n, 1028, s);
                p[0] = 0;
            }
        }
        59 => {
            if p[1] > 1 {
                notice(n, 1036, s);
                p[1] = 0;
            } else {
                check_target(n, event, kind, p[0], slot);
            }
        }
        60 => {
            if !owner.contains(SpellList, p[0], 0)? {
                notice(n, 1045, s);
                p[0] = 0;
            }
        }
        63 => {
            if entry != 0 && !owner.contains(ImmunitySet, p[0], entry)? {
                notice(n, 1058, s);
                p[0] = 0;
            }
        }
        20 | 21 | 24 | 25 | 29 | 37..=39 | 41 | 51 | 61 | 62 | 64 | 65 => {}
        _ => notice(n, 1068, s),
    }
    Ok(())
}

/// Definitions can be shared. Source holders are copied per physical instance.
#[derive(Clone, Debug, Default, Serialize)]
pub struct EventAiDefinitions {
    entries: BTreeMap<u32, Vec<EventAiDefinition>>,
    guids: BTreeMap<u32, Vec<EventAiDefinition>>,
}
impl EventAiDefinitions {
    pub fn load(
        owner: &mut impl EventAiLoaderOwner,
        mut rows: Vec<EventAiRow>,
    ) -> Result<(Self, Vec<EventAiLoad>)> {
        rows.sort_by_key(|r| r.words[0]); // SQL ORDER BY id, :216.
        let mut this = Self::default();
        let mut results = Vec::with_capacity(rows.len());
        for row in rows {
            let result = load_event_ai_row(owner, row)?;
            if let EventAiDisposition::Loaded(event) = &result.disposition {
                this.insert(event.clone());
            }
            results.push(result);
        }
        Ok((this, results))
    }
    pub fn insert(&mut self, event: EventAiDefinition) {
        if event.entry != 0 {
            self.entries.entry(event.entry).or_default().push(event);
        } else {
            self.guids.entry(event.guid).or_default().push(event);
        }
    }
    pub fn for_spawn(&self, entry: u32, guid: u32) -> impl Iterator<Item = &EventAiDefinition> {
        self.entries
            .get(&entry)
            .into_iter()
            .flatten()
            .chain(self.guids.get(&guid).into_iter().flatten())
    }
}
