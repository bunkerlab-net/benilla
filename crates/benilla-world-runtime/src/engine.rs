// SPDX-License-Identifier: GPL-3.0-or-later
use crate::*;
use benilla_sim::xp::{base_gain, Level};
use benilla_world_data::{CreatureBehavior, DirectEffect, ItemStack, QuestRelationKind};

pub(crate) fn advance(
    state: &mut State,
    data: &WorldData,
    rules: &Rules,
    tick: u64,
    inputs: &[Input],
) -> Result<TickResult, Error> {
    state.tick = tick;
    state.revision += 1;
    let mut engine = Engine {
        state,
        data,
        rules,
        events: Vec::new(),
    };
    engine.respawn()?;
    engine.finish_casts();
    let mut outcomes = Vec::new();
    for input in inputs {
        let pi = engine.player_index(input.player).unwrap();
        let rejection = engine.command(pi, &input.command).err();
        engine.state.players[pi].next_sequence += 1;
        outcomes.push(Outcome {
            player: input.player,
            sequence: input.sequence,
            rejection,
        });
    }
    engine.retaliate();
    Ok(TickResult {
        outcomes,
        events: engine.events,
    })
}
struct Engine<'a> {
    state: &'a mut State,
    data: &'a WorldData,
    rules: &'a Rules,
    events: Vec<Event>,
}
impl Engine<'_> {
    fn player_index(&self, id: PlayerId) -> Option<usize> {
        self.state.players.binary_search_by_key(&id, |p| p.id).ok()
    }
    fn npc_index(&self, id: NpcId) -> Result<usize, Rejection> {
        let i = self
            .state
            .npcs
            .binary_search_by_key(&id.spawn, |n| n.id.spawn)
            .map_err(|_| Rejection::UnknownTarget)?;
        if self.state.npcs[i].id != id {
            return Err(Rejection::StaleTarget);
        }
        Ok(i)
    }
    fn npc_position(&self, ni: usize) -> Position {
        let s = self.data.spawn(self.state.npcs[ni].id.spawn).unwrap();
        Position {
            map: s.map_id,
            xyz: s.position_milliunits,
        }
    }
    fn creature(&self, ni: usize) -> &benilla_world_data::CreatureTemplate {
        self.data
            .creature(
                self.data
                    .spawn(self.state.npcs[ni].id.spawn)
                    .unwrap()
                    .creature_entry,
            )
            .unwrap()
    }
    fn command(&mut self, pi: usize, command: &Command) -> Result<(), Rejection> {
        if matches!(command, Command::Revive) {
            return self.revive(pi);
        }
        // Closing UI state remains safe after death, expiry, or movement out of range.
        if let Command::ReleaseLoot { target } = *command {
            if self.state.players[pi].active_loot == Some(target) {
                self.release_loot(pi);
            }
            return Ok(());
        }
        if self.state.players[pi].health == 0 {
            return Err(Rejection::Dead);
        }
        match *command {
            Command::Move { delta } => {
                if self.state.players[pi].casting.is_some() {
                    return Err(Rejection::BusyCasting);
                }
                // Bound before squaring: adversarial i32::MIN cannot overflow.
                if delta
                    .iter()
                    .any(|x| x.unsigned_abs() > self.rules.movement_per_tick)
                    || delta
                        .iter()
                        .map(|x| i64::from(*x).pow(2) as u64)
                        .sum::<u64>()
                        > u64::from(self.rules.movement_per_tick).pow(2)
                {
                    return Err(Rejection::InvalidMove);
                }
                let old = self.state.players[pi].position.xyz;
                let mut next = [0; 3];
                for i in 0..3 {
                    next[i] = old[i].checked_add(delta[i]).ok_or(Rejection::InvalidMove)?;
                    if next[i].unsigned_abs() > MAX_COORD as u32 {
                        return Err(Rejection::InvalidMove);
                    }
                }
                self.state.players[pi].position.xyz = next;
                if let Some(target) = self.state.players[pi].active_loot {
                    let ni = self.npc_index(target)?;
                    if !self.state.players[pi].position.in_range(
                        self.npc_position(ni),
                        0,
                        self.rules.interact_range,
                    ) {
                        self.release_loot(pi);
                    }
                }
                Ok(())
            }
            Command::Melee { target } => {
                if self.state.players[pi].casting.is_some() {
                    return Err(Rejection::BusyCasting);
                }
                if self.state.players[pi].melee_ready_at > self.state.tick {
                    return Err(Rejection::NotReady);
                }
                let ni = self.npc_index(target)?;
                // Only this original physical-melee path uses the integer armor port.
                // Direct abilities lack native school semantics and must not be routed here.
                let damage = benilla_sim::classic::armor_reduced_damage(
                    self.state.players[pi].level,
                    self.creature(ni).armor as i32,
                    0,
                    self.rules.melee_damage,
                )
                .map_err(|_| Rejection::UnsupportedMechanic)?;
                self.damage(pi, target, damage, 0, self.rules.melee_range)?;
                self.state.players[pi].melee_ready_at =
                    self.state.tick + self.rules.ticks(self.rules.melee_period_ms);
                Ok(())
            }
            Command::Cast { ability, target } => self.cast(pi, ability, target),
            Command::AcceptQuest { quest, giver } => self.accept(pi, quest, giver),
            Command::TurnInQuest { quest, giver } => self.turn_in(pi, quest, giver),
            Command::OpenLoot { target } => self.open_loot(pi, target),
            Command::TakeLootMoney { target } => self.take_loot_money(pi, target),
            Command::TakeLootItem { target, slot } => self.take_loot_item(pi, target, slot),
            Command::Loot { target } => self.loot(pi, target),
            Command::Revive | Command::ReleaseLoot { .. } => unreachable!(),
        }
    }
    fn validate_effect(&self, pi: usize, ability: u32, target: Target) -> Result<(), Rejection> {
        let a = self
            .data
            .ability(ability)
            .ok_or(Rejection::UnknownAbility)?;
        match (a.effect, target) {
            (DirectEffect::Damage, Target::Npc { id }) => {
                let ni = self.npc_index(id)?;
                if self.state.npcs[ni].health == 0 {
                    return Err(Rejection::TargetDead);
                }
                if self.creature(ni).behavior == CreatureBehavior::Passive {
                    return Err(Rejection::PassiveTarget);
                }
                if !self.state.players[pi].position.in_range(
                    self.npc_position(ni),
                    a.min_range_milliunits,
                    a.max_range_milliunits,
                ) {
                    return Err(Rejection::OutOfRange);
                }
            }
            (DirectEffect::Heal, Target::SelfPlayer) => {
                if a.min_range_milliunits > 0 {
                    return Err(Rejection::OutOfRange);
                }
            }
            _ => return Err(Rejection::InvalidTarget),
        }
        Ok(())
    }
    fn cast(&mut self, pi: usize, ability: u32, target: Target) -> Result<(), Rejection> {
        let p = &self.state.players[pi];
        if !p.abilities.contains(&ability) {
            return Err(Rejection::UnknownAbility);
        }
        let a = self
            .data
            .ability(ability)
            .ok_or(Rejection::UnknownAbility)?;
        if p.casting.is_some() {
            return Err(Rejection::BusyCasting);
        }
        if p.global_ready_at > self.state.tick
            || p.cooldowns
                .get(&ability)
                .is_some_and(|x| *x > self.state.tick)
        {
            return Err(Rejection::NotReady);
        }
        if p.mana < a.mana_cost {
            return Err(Rejection::InsufficientMana);
        }
        self.validate_effect(pi, ability, target)?;
        let p = &mut self.state.players[pi];
        p.mana -= a.mana_cost;
        p.global_ready_at = self.state.tick + self.rules.ticks(a.global_cooldown_ms);
        p.cooldowns
            .insert(ability, self.state.tick + self.rules.ticks(a.cooldown_ms));
        p.casting = Some(CastState {
            ability,
            target,
            completes_at: self.state.tick + self.rules.ticks(a.cast_time_ms),
        });
        self.events.push(Event::CastStarted {
            player: p.id,
            ability,
            target,
            completes_at: self.state.tick + self.rules.ticks(a.cast_time_ms),
        });
        // Zero-time abilities resolve in the command phase; positive durations resolve before
        // commands at their completion tick, after respawns and in canonical player order.
        if a.cast_time_ms == 0 {
            self.finish_cast(pi);
        }
        Ok(())
    }
    fn finish_casts(&mut self) {
        for pi in 0..self.state.players.len() {
            if self.state.players[pi]
                .casting
                .as_ref()
                .is_some_and(|c| c.completes_at <= self.state.tick)
            {
                self.finish_cast(pi);
            }
        }
    }
    fn finish_cast(&mut self, pi: usize) {
        let cast = self.state.players[pi].casting.take().unwrap();
        let result = self.apply_effect(pi, cast.ability, cast.target);
        if let Err(reason) = result {
            self.events.push(Event::CastFizzled {
                player: self.state.players[pi].id,
                ability: cast.ability,
                reason,
            });
        } else {
            self.events.push(Event::CastFinished {
                player: self.state.players[pi].id,
                ability: cast.ability,
                target: cast.target,
            });
        }
    }
    fn apply_effect(&mut self, pi: usize, ability: u32, target: Target) -> Result<(), Rejection> {
        if self.state.players[pi].health == 0 {
            return Err(Rejection::Dead);
        }
        self.validate_effect(pi, ability, target)?;
        let a = self.data.ability(ability).unwrap();
        match target {
            Target::Npc { id } => self.damage(
                pi,
                id,
                a.amount,
                a.min_range_milliunits,
                a.max_range_milliunits,
            ),
            Target::SelfPlayer => {
                let p = &mut self.state.players[pi];
                let amount = a.amount.min(self.rules.player_health - p.health);
                p.health += amount;
                self.events.push(Event::Healed {
                    player: p.id,
                    amount,
                });
                Ok(())
            }
        }
    }
    fn damage(
        &mut self,
        pi: usize,
        target: NpcId,
        damage: u32,
        min: u32,
        max: u32,
    ) -> Result<(), Rejection> {
        let ni = self.npc_index(target)?;
        let npc = &self.state.npcs[ni];
        if npc.health == 0 {
            return Err(Rejection::TargetDead);
        }
        let creature = self.creature(ni);
        if creature.behavior == CreatureBehavior::Passive {
            return Err(Rejection::PassiveTarget);
        }
        if !self.state.players[pi]
            .position
            .in_range(self.npc_position(ni), min, max)
        {
            return Err(Rejection::OutOfRange);
        }
        let owner = npc.tagged_by.unwrap_or(self.state.players[pi].id);
        let owner_i = self.player_index(owner).unwrap();
        let amount = damage.min(npc.health);
        let died = amount == npc.health;
        let creature_entry = creature.entry;
        let gain = if died {
            self.rules.rated_xp(u64::from(
                base_gain(
                    Level::new(self.state.players[owner_i].level).unwrap(),
                    Level::new(creature.level).unwrap(),
                )
                .round_ties_even(),
            ))
        } else {
            0
        };
        if self.state.players[owner_i].xp + gain > MAX_PROGRESSION {
            return Err(Rejection::ProgressionLimit);
        }
        let drop = if died {
            self.rules
                .loot
                .iter()
                .find(|drop| drop.creature == creature_entry)
        } else {
            None
        };
        let npc = &mut self.state.npcs[ni];
        npc.tagged_by = Some(owner);
        npc.health -= amount;
        self.events.push(Event::DamageNpc {
            id: target,
            player: self.state.players[pi].id,
            amount,
        });
        if died {
            let spawn = self.data.spawn(target.spawn).unwrap();
            npc.respawn_at = Some(self.state.tick + self.rules.ticks(spawn.respawn_ms));
            npc.loot_money = drop.map_or(0, |x| x.money);
            npc.loot_items =
                drop.map_or_else(Vec::new, |x| x.items.iter().cloned().map(Some).collect());
            npc.looted = npc.loot_money == 0 && npc.loot_items.iter().all(Option::is_none);
            self.add_xp(owner_i, gain);
            let p = &mut self.state.players[owner_i];
            for (quest, progress) in &mut p.quests {
                if progress.turned_in {
                    continue;
                }
                for (i, objective) in self
                    .data
                    .quest(*quest)
                    .unwrap()
                    .objectives
                    .iter()
                    .enumerate()
                {
                    if objective.creature_entry == creature_entry {
                        progress.counts[i] = (progress.counts[i] + 1).min(objective.count);
                    }
                }
            }
            self.events.push(Event::NpcDied {
                id: target,
                owner,
                xp: gain,
            });
        }
        Ok(())
    }
    fn add_xp(&mut self, pi: usize, xp: u64) {
        let p = &mut self.state.players[pi];
        p.xp += xp;
        p.level = self.rules.level(p.xp);
    }
    fn quest_giver(
        &self,
        pi: usize,
        quest: u32,
        giver: NpcId,
        kind: QuestRelationKind,
    ) -> Result<(), Rejection> {
        self.data.quest(quest).ok_or(Rejection::UnknownQuest)?;
        let ni = self.npc_index(giver)?;
        if self.state.npcs[ni].health == 0 {
            return Err(Rejection::TargetDead);
        }
        if !self.data.export().quest_relations.iter().any(|r| {
            r.creature_entry == self.creature(ni).entry && r.quest_entry == quest && r.kind == kind
        }) {
            return Err(Rejection::WrongQuestGiver);
        }
        if !self.state.players[pi].position.in_range(
            self.npc_position(ni),
            0,
            self.rules.interact_range,
        ) {
            return Err(Rejection::OutOfRange);
        }
        Ok(())
    }
    fn accept(&mut self, pi: usize, quest: u32, giver: NpcId) -> Result<(), Rejection> {
        self.quest_giver(pi, quest, giver, QuestRelationKind::Starts)?;
        let q = self.data.quest(quest).unwrap();
        let level = self.state.players[pi].level;
        if level < q.min_level || level > q.max_level {
            return Err(Rejection::QuestLevel);
        }
        let p = &mut self.state.players[pi];
        if let Some(progress) = p.quests.get(&quest) {
            return Err(if progress.turned_in {
                Rejection::AlreadyTurnedIn
            } else {
                Rejection::AlreadyAccepted
            });
        }
        if p.quests.len() >= MAX_QUESTS {
            return Err(Rejection::QuestLogFull);
        }
        p.quests.insert(
            quest,
            QuestProgress {
                counts: vec![0; self.data.quest(quest).unwrap().objectives.len()],
                turned_in: false,
            },
        );
        Ok(())
    }
    fn turn_in(&mut self, pi: usize, quest: u32, giver: NpcId) -> Result<(), Rejection> {
        self.quest_giver(pi, quest, giver, QuestRelationKind::Finishes)?;
        let q = self.data.quest(quest).unwrap();
        let p = &self.state.players[pi];
        let progress = p.quests.get(&quest).ok_or(Rejection::QuestNotAccepted)?;
        if progress.turned_in {
            return Err(Rejection::AlreadyTurnedIn);
        }
        if q.objectives
            .iter()
            .zip(&progress.counts)
            .any(|(o, count)| *count < o.count)
        {
            return Err(Rejection::QuestIncomplete);
        }
        let xp = self.rules.rated_xp(u64::from(q.reward_xp));
        if p.xp + xp > MAX_PROGRESSION || p.money + u64::from(q.reward_money) > MAX_PROGRESSION {
            return Err(Rejection::ProgressionLimit);
        }
        let (inventory, next_item_instance) = self.add_items(pi, &q.reward_items)?;
        self.add_xp(pi, xp);
        let p = &mut self.state.players[pi];
        p.inventory = inventory;
        p.next_item_instance = next_item_instance;
        p.money += u64::from(q.reward_money);
        p.quests.get_mut(&quest).unwrap().turned_in = true;
        self.events.push(Event::QuestRewarded {
            player: p.id,
            quest,
        });
        Ok(())
    }
    /// Build the complete prospective bag before mutating any reward or loot state.
    fn add_items(
        &self,
        pi: usize,
        items: &[ItemStack],
    ) -> Result<(Vec<InventoryItem>, u64), Rejection> {
        let p = &self.state.players[pi];
        let mut bag = p.inventory.clone();
        let mut next_instance = p.next_item_instance;
        for item in items {
            let max = self.data.item(item.item_entry).unwrap().max_stack;
            let mut remaining = item.count;
            // Slot order is canonical; existing stack identities survive each merge.
            for stack in bag.iter_mut().filter(|x| x.item_entry == item.item_entry) {
                let added = remaining.min(max - stack.count);
                stack.count += added;
                remaining -= added;
                if remaining == 0 {
                    break;
                }
            }
            while remaining > 0 {
                let slot = (0..self.rules.inventory_slots as u16)
                    .find(|slot| bag.iter().all(|stack| stack.slot != *slot))
                    .ok_or(Rejection::InventoryFull)?;
                let instance = next_instance;
                next_instance = next_instance
                    .checked_add(1)
                    .ok_or(Rejection::ProgressionLimit)?;
                let count = remaining.min(max);
                let index = bag.partition_point(|stack| stack.slot < slot);
                bag.insert(
                    index,
                    InventoryItem {
                        instance,
                        slot,
                        item_entry: item.item_entry,
                        count,
                    },
                );
                remaining -= count;
            }
        }
        Ok((bag, next_instance))
    }
    fn validate_loot(&self, pi: usize, target: NpcId) -> Result<usize, Rejection> {
        let ni = self.npc_index(target)?;
        let npc = &self.state.npcs[ni];
        if npc.health > 0 {
            return Err(Rejection::Alive);
        }
        if npc.tagged_by != Some(self.state.players[pi].id) {
            return Err(Rejection::LootNotOwned);
        }
        if !self.state.players[pi].position.in_range(
            self.npc_position(ni),
            0,
            self.rules.interact_range,
        ) {
            return Err(Rejection::OutOfRange);
        }
        Ok(ni)
    }
    fn active_loot(&self, pi: usize, target: NpcId) -> Result<usize, Rejection> {
        let ni = self.validate_loot(pi, target)?;
        if self.state.players[pi].active_loot != Some(target) {
            return Err(Rejection::LootNotOpen);
        }
        Ok(ni)
    }
    // CMaNGOS LootHandler separates show/take-slot/take-gold/release; see README provenance.
    // These checks and fixed original rewards are not native Classic loot parity.
    fn open_loot(&mut self, pi: usize, target: NpcId) -> Result<(), Rejection> {
        self.validate_loot(pi, target)?;
        if self.state.players[pi].active_loot != Some(target) {
            self.release_loot(pi);
            self.state.players[pi].active_loot = Some(target);
        }
        self.events.push(Event::LootOpened {
            player: self.state.players[pi].id,
            id: target,
        });
        Ok(())
    }
    fn release_loot(&mut self, pi: usize) {
        let p = &mut self.state.players[pi];
        if let Some(id) = p.active_loot.take() {
            self.events.push(Event::LootReleased { player: p.id, id });
        }
    }
    fn update_looted(&mut self, ni: usize) {
        let n = &mut self.state.npcs[ni];
        n.looted = n.health == 0 && n.loot_money == 0 && n.loot_items.iter().all(Option::is_none);
    }
    fn take_loot_money(&mut self, pi: usize, target: NpcId) -> Result<(), Rejection> {
        let ni = self.active_loot(pi, target)?;
        let amount = self.state.npcs[ni].loot_money;
        if amount == 0 {
            return Err(Rejection::LootMoneyEmpty);
        }
        if self.state.players[pi].money + u64::from(amount) > MAX_PROGRESSION {
            return Err(Rejection::ProgressionLimit);
        }
        self.state.players[pi].money += u64::from(amount);
        self.state.npcs[ni].loot_money = 0;
        self.update_looted(ni);
        self.events.push(Event::LootMoneyTaken {
            player: self.state.players[pi].id,
            id: target,
            amount,
        });
        Ok(())
    }
    fn take_loot_item(&mut self, pi: usize, target: NpcId, slot: u8) -> Result<(), Rejection> {
        let ni = self.active_loot(pi, target)?;
        let item = self.state.npcs[ni]
            .loot_items
            .get(usize::from(slot))
            .ok_or(Rejection::InvalidLootSlot)?
            .as_ref()
            .ok_or(Rejection::LootSlotEmpty)?;
        let (inventory, next_item_instance) = self.add_items(pi, std::slice::from_ref(item))?;
        let p = &mut self.state.players[pi];
        p.inventory = inventory;
        p.next_item_instance = next_item_instance;
        self.state.npcs[ni].loot_items[usize::from(slot)] = None;
        self.update_looted(ni);
        self.events.push(Event::LootItemTaken {
            player: self.state.players[pi].id,
            id: target,
            slot,
        });
        Ok(())
    }
    fn loot(&mut self, pi: usize, target: NpcId) -> Result<(), Rejection> {
        let ni = self.validate_loot(pi, target)?;
        let npc = &self.state.npcs[ni];
        if npc.looted {
            return Err(Rejection::AlreadyLooted);
        }
        let money = u64::from(npc.loot_money);
        if self.state.players[pi].money + money > MAX_PROGRESSION {
            return Err(Rejection::ProgressionLimit);
        }
        let remaining: Vec<ItemStack> = npc.loot_items.iter().flatten().cloned().collect();
        let (inventory, next_item_instance) = self.add_items(pi, &remaining)?;
        let p = &mut self.state.players[pi];
        p.money += money;
        p.inventory = inventory;
        p.next_item_instance = next_item_instance;
        let n = &mut self.state.npcs[ni];
        n.loot_money = 0;
        n.loot_items.fill(None);
        self.update_looted(ni);
        self.events.push(Event::Looted {
            player: self.state.players[pi].id,
            id: target,
        });
        Ok(())
    }
    fn respawn(&mut self) -> Result<(), Error> {
        for ni in 0..self.state.npcs.len() {
            if self.state.npcs[ni]
                .respawn_at
                .is_some_and(|tick| tick <= self.state.tick)
            {
                let max_health = self.creature(ni).max_health;
                let expired = self.state.npcs[ni].id;
                for pi in 0..self.state.players.len() {
                    if self.state.players[pi].active_loot == Some(expired) {
                        self.release_loot(pi);
                    }
                }
                let n = &mut self.state.npcs[ni];
                n.id.generation = n.id.generation.checked_add(1).ok_or(Error::LimitReached)?;
                n.health = max_health;
                n.tagged_by = None;
                n.melee_ready_at = 0;
                n.respawn_at = None;
                n.looted = false;
                n.loot_money = 0;
                n.loot_items.clear();
                self.events.push(Event::Respawned { id: n.id });
            }
        }
        Ok(())
    }
    fn retaliate(&mut self) {
        for ni in 0..self.state.npcs.len() {
            let n = &self.state.npcs[ni];
            if n.health == 0 || n.melee_ready_at > self.state.tick {
                continue;
            }
            let Some(owner) = n.tagged_by else {
                continue;
            };
            let pi = self.player_index(owner).unwrap();
            let p = &self.state.players[pi];
            let c = self.creature(ni);
            if p.health == 0
                || c.behavior != CreatureBehavior::StationaryMelee
                || !p
                    .position
                    .in_range(self.npc_position(ni), 0, c.combat_reach_milliunits)
            {
                continue;
            }
            let amount = c.melee_damage.min(p.health);
            let period = self.rules.ticks(c.melee_period_ms);
            self.state.npcs[ni].melee_ready_at = self.state.tick + period;
            let p = &mut self.state.players[pi];
            p.health -= amount;
            self.events.push(Event::DamagePlayer {
                player: p.id,
                amount,
            });
            if p.health == 0 {
                p.casting = None;
                p.revive_at = Some(self.state.tick + self.rules.ticks(self.rules.revive_delay_ms));
                self.events.push(Event::PlayerDied { player: p.id });
                self.release_loot(pi);
            }
        }
    }
    fn revive(&mut self, pi: usize) -> Result<(), Rejection> {
        let p = &mut self.state.players[pi];
        if p.health > 0 {
            return Err(Rejection::Alive);
        }
        if p.revive_at.is_none_or(|tick| tick > self.state.tick) {
            return Err(Rejection::NotReady);
        }
        p.position = self.rules.start;
        p.health = self.rules.player_health;
        p.mana = self.rules.player_mana;
        p.revive_at = None;
        self.events.push(Event::Revived { player: p.id });
        Ok(())
    }
}
