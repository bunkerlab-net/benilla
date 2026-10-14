// SPDX-License-Identifier: GPL-3.0-or-later
//! Native death-only solo reward admission, followed by source KilledMonster aliases.
use super::*;
use benilla_sim::player_progression::kill_xp as kill;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KillCreatureRow {
    pub entry: u32,
    pub experience_multiplier_bits: u32,
    pub is_totem: bool,
    pub is_pet: bool,
    /// Resolved Creature::IsNoXp from actual static/dynamic settings, not an inferred bit.
    pub is_no_xp: bool,
    pub is_critter: bool,
    pub is_elite: bool,
    pub player_controlled: bool,
    pub civilian_for_target: pg::Evidence,
    pub racial_leader: pg::Evidence,
    pub reputation: kill::ReputationAdmission,
    pub kill_credit: [u32; 2],
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KillDependencyAudit {
    pub kill_procs: pg::Evidence,
    pub kill_scripts: pg::Evidence,
    pub other_damage_sources: pg::Evidence,
    /// Includes CREATOR_LOOT, CAN_BE_MULTITAPPED, preexisting/foreign taps,
    /// and any callback that replaces the ordinary first-player tap.
    pub tap_overrides: pg::Evidence,
    /// Full loot/reference-loot closure and money fields must be inert until
    /// the native inventory/loot host is implemented.
    pub native_loot_or_money: pg::Evidence,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeKillPolicy {
    pub rules: kill::Rules,
    pub clock: kill::OnlineRestContract,
    pub audit: KillDependencyAudit,
    pub creatures: Vec<KillCreatureRow>,
}
impl NativeKillPolicy {
    pub(in crate::native) fn validate(
        &self,
        profile: &Profile,
        loot_present: bool,
    ) -> Result<(), NativeError> {
        self.rules.validate()?;
        if self.audit.native_loot_or_money == pg::Evidence::Unknown
            || (!loot_present && self.audit.native_loot_or_money != pg::Evidence::Absent)
        {
            return Err(NativeError::UnsupportedMechanic(
                "native loot/money host required",
            ));
        }
        if self.creatures.len() > 4096
            || self.creatures.windows(2).any(|w| w[0].entry >= w[1].entry)
            || self.clock.offline_intervals != pg::Evidence::Absent
            || self.clock.entered_rest_area != pg::Evidence::Absent
            || [
                &self.audit.kill_procs,
                &self.audit.kill_scripts,
                &self.audit.other_damage_sources,
                &self.audit.tap_overrides,
            ]
            .iter()
            .any(|e| **e != pg::Evidence::Absent)
        {
            return Err(NativeError::UnsupportedMechanic(
                "native kill dependency/loot/rest closure",
            ));
        }
        let entries = profile
            .npcs
            .iter()
            .map(|p| p.entry)
            .collect::<BTreeSet<_>>();
        if entries != self.creatures.iter().map(|p| p.entry).collect() {
            return Err(NativeError::InvalidProfile);
        }
        for npc in &profile.npcs {
            let row = self.row(npc.entry)?;
            if row.kill_credit.iter().any(|v| *v > 0xff_ffff) {
                return Err(NativeError::InvalidProfile);
            }
            let victim = row.victim(2, npc);
            victim.validate()?;
            kill::KillContext {
                death_event: 1,
                player_guid: 1,
                responsible_player_guid: Some(1),
                original_loot_recipient_guid: Some(1),
                group_loot_recipient: pg::Evidence::Absent,
                civilian_for_target: row.civilian_for_target,
                racial_leader: row.racial_leader,
                reputation: row.reputation,
                clock: self.clock,
            }
            .validate(victim, kill::RestState::fresh_player())?;
        }
        Ok(())
    }
    fn row(&self, entry: u32) -> Result<&KillCreatureRow, NativeError> {
        self.creatures
            .binary_search_by_key(&entry, |r| r.entry)
            .ok()
            .map(|i| &self.creatures[i])
            .ok_or(NativeError::InvalidProfile)
    }
}
impl KillCreatureRow {
    fn victim(&self, guid: u64, profile: &NpcProfile) -> kill::Victim {
        kill::Victim {
            guid,
            entry: self.entry,
            level: profile.ai.level.into(),
            experience_multiplier_bits: self.experience_multiplier_bits,
            max_health: profile.max_health,
            damage_by_others: 0,
            is_totem: self.is_totem,
            is_pet: self.is_pet,
            is_no_xp: self.is_no_xp,
            is_critter: self.is_critter,
            is_elite: self.is_elite,
            player_controlled: self.player_controlled,
        }
    }
}
#[derive(Clone)]
struct KillHost {
    progression: ProgressionHost,
    expected: (kill::Victim, kill::Rules, kill::KillContext),
    rest: kill::RestState,
}
impl pg::Host for KillHost {
    fn preflight(&self, catalog: &pg::Catalog, view: &pg::Snapshot) -> pg::Result<()> {
        self.progression.preflight(catalog, view)
    }
    fn apply(&mut self, event: &pg::Event, view: &pg::Snapshot) -> pg::Result<()> {
        self.progression.apply(event, view)
    }
}
impl kill::KillHost for KillHost {
    fn preflight_kill(
        &self,
        victim: &kill::Victim,
        rules: &kill::Rules,
        context: &kill::KillContext,
        rest: &kill::RestState,
    ) -> pg::Result<()> {
        if (*victim, *rules, *context) != self.expected || *rest != self.rest {
            return Err(pg::Error::Host("owner death/tap binding"));
        }
        context.validate(*victim, *rest)
    }
    fn apply_kill(
        &mut self,
        _: &kill::Event,
        _: &pg::Snapshot,
        rest: &kill::RestState,
    ) -> pg::Result<()> {
        self.rest = *rest; // The ordered packet/field observations join the committed owner outbox.
        Ok(())
    }
}
impl QuestProgressionState {
    pub(in crate::native) fn killed(
        &self,
        g: &QuestProgressionGenesis,
        combat: &PlayerCombatState,
        world: crate::WorldId,
        profile: &NpcProfile,
        id: crate::NpcId,
    ) -> Result<(Self, Vec<QuestProgressionEvent>), NativeError> {
        if self
            .credited_generations
            .get(&id.spawn)
            .is_some_and(|generation| *generation >= id.generation)
        {
            return Err(NativeError::InvalidState);
        }
        let row = g.kill.row(profile.entry)?;
        let guid = creature_guid(world, id, profile.entry);
        let victim = row.victim(guid, profile);
        let context = kill::KillContext {
            death_event: self.next_death_event,
            player_guid: SOLO_UNIT,
            responsible_player_guid: Some(SOLO_UNIT),
            original_loot_recipient_guid: Some(SOLO_UNIT),
            group_loot_recipient: pg::Evidence::Absent,
            civilian_for_target: row.civilian_for_target,
            racial_leader: row.racial_leader,
            reputation: row.reputation,
            clock: g.kill.clock,
        };
        let training = init::LiveTraining::restore_authoritative(
            &g.training,
            &self.training,
            self.player.view(),
        )?;
        let host = KillHost {
            progression: ProgressionHost {
                training,
                derived: self.derived.clone(),
                derived_calls: self.derived_calls.clone(),
                inputs: g.derived_inputs(combat),
                audit: g.initial.view().audit,
            },
            expected: (victim, g.kill.rules, context),
            rest: self.rest,
        };
        let tx = kill::award(
            &g.levels,
            &self.player,
            self.rest,
            &host,
            g.kill.rules,
            victim,
            context,
        )?;
        let abilities = g.resolve_abilities(&tx.host.progression.training, tx.state.view())?;
        let mut next = Self {
            inventory: self.inventory.clone(),
            inventory_receipts: self.inventory_receipts.clone(),
            loot_target: self.loot_target,
            quests: self.quests.clone(),
            player: tx.state,
            training: tx.host.progression.training.snapshot(),
            derived: tx.host.progression.derived,
            derived_calls: tx.host.progression.derived_calls,
            abilities,
            money: self.money,
            genesis: self.genesis,
            rest: tx.rest,
            next_death_event: self
                .next_death_event
                .checked_add(1)
                .ok_or(NativeError::Limit)?,
            credited_generations: self.credited_generations.clone(),
            played_clock_seconds: self.played_clock_seconds,
            played_time_total: self.played_time_total,
        };
        next.credited_generations.insert(id.spawn, id.generation);
        let mut events = vec![QuestProgressionEvent::PartyKill { victim: id }];
        events.extend(
            tx.events
                .into_iter()
                .map(|event| QuestProgressionEvent::KillExperience(Box::new(event))),
        );
        // Player::KilledMonster: normal template first, then both aliases in native
        // array order. This bounded runtime forbids transformations/template swaps.
        for entry in std::iter::once(profile.entry)
            .chain(row.kill_credit)
            .filter(|id| *id != 0)
        {
            let (updated, credit) =
                next.reduce(g, combat, q::Command::KillCredit { entry, guid })?;
            next = updated;
            events.extend(credit);
        }
        Ok((next, events))
    }
}
/// Same stable native-guid policy as the protocol adapter, bound to world/spawn generation.
pub fn creature_guid(world: crate::WorldId, id: crate::NpcId, entry: u32) -> u64 {
    let digest = crate::digest(b"benilla-native-npc-guid-v1\0", &(world, id));
    let low = u32::from_le_bytes([digest[0], digest[1], digest[2], 0]).max(1);
    0xf130_0000_0000_0000 | (u64::from(entry) << 24) | u64::from(low)
}
