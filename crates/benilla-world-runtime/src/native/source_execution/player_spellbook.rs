// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::{player_skills::PlayerSkillInputs, *};

// SpellDefines.h:36,37,97,121; SpellEffectDefines.h:70,144.
const PASSIVE: u32 = 0x40;
const DO_NOT_DISPLAY: u32 = 0x80;
const CAST_WHEN_LEARNED: u32 = 0x8000_0000;
const ALLOW_WHILE_NOT_SHAPESHIFTED: u32 = 0x0008_0000;
const SKILL_STEP: u32 = 44;
const SKILL: u32 = 118;

// Player.h:106-120; Player.cpp:3148,3313-3317 initialize each inserted row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourcePlayerSpellState {
    Unchanged,
    Changed,
    New,
    Removed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourcePlayerSpell {
    pub state: SourcePlayerSpellState,
    pub active: bool,
    pub dependent: bool,
    pub disabled: bool,
}

// Player.h:140-146,161-166; ActionButton() starts at packed zero and NEW.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourcePlayerActionState {
    Unchanged,
    Changed,
    New,
    Deleted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourcePlayerAction {
    pub packed_data: u32,
    pub state: SourcePlayerActionState,
}

impl Default for SourcePlayerAction {
    fn default() -> Self {
        Self {
            packed_data: 0,
            state: SourcePlayerActionState::New,
        }
    }
}

pub(super) struct PlayerLearning<'a, 'cast> {
    pub objects: &'a SourceObjects,
    pub units: &'a mut SourceUnits,
    pub cast: Option<SourcePlayerCastInputs<'cast>>,
    pub skills: &'a PlayerSkillInputs,
    pub spells: &'a SourceSpellStores,
    pub items: &'a SourceStartupItems,
    pub class: u8,
}

impl SourcePlayer {
    /// Spell IDs and flags. Iteration does not reproduce the login packet's hash order.
    pub fn spellbook(&self) -> &BTreeMap<u32, SourcePlayerSpell> {
        &self.spells
    }

    /// Stored action words keyed by button number.
    pub fn action_buttons(&self) -> &BTreeMap<u8, SourcePlayerAction> {
        &self.action_buttons
    }

    // Player.cpp:3935-3943. Inactive ranks still count as known.
    pub(super) fn has_spell(&self, id: u32) -> bool {
        self.spells
            .get(&id)
            .is_some_and(|spell| spell.state != SourcePlayerSpellState::Removed && !spell.disabled)
    }

    // Player.cpp:18281-18294.
    pub(super) fn learn_default_spells(
        &mut self,
        live: &mut PlayerLearning<'_, '_>,
        ids: &[u32],
    ) -> Result<()> {
        for &id in ids {
            self.learn_granted_spell(live, id)?;
        }
        Ok(())
    }

    // Player.cpp:5693-5696,18289-18292. Detached grants do not send a learn packet.
    pub(super) fn learn_granted_spell(
        &mut self,
        live: &mut PlayerLearning<'_, '_>,
        id: u32,
    ) -> Result<()> {
        if live.objects.get(self.object())?.in_world {
            self.learn_spell(live, id, true, false)
        } else {
            self.add_spell(live, id, true, true, true, false)?;
            Ok(())
        }
    }

    // Player.cpp:3117-3428. The learned row is visible before casting and training.
    pub(super) fn add_spell(
        &mut self,
        live: &mut PlayerLearning<'_, '_>,
        id: u32,
        mut active: bool,
        learning: bool,
        dependent: bool,
        disabled: bool,
    ) -> Result<bool> {
        use SourcePlayerSpellState::{Changed, New, Removed, Unchanged};
        let in_world = live.objects.get(self.object())?.in_world;
        let Some(template) = live.spells.template(id) else {
            require(
                in_world || learning,
                "execution.player.Player_3126.character_spell_delete_owner",
            )?;
            return Ok(false);
        };
        // Player.cpp:3134 validates before the duplicate check.
        if !live
            .spells
            .is_spell_valid(Some(template), Some(live.items), false, &mut |_| {})?
        {
            require(
                in_world || learning,
                "execution.player.Player_3140.character_spell_delete_owner",
            )?;
            return Ok(false);
        }

        let mut state = if learning { New } else { Unchanged };
        let mut reenabled = false;
        let mut superseded = false;
        if self.spells.contains_key(&id) {
            // Player.cpp:3160-3167. Resolving the successor can change the request.
            let successor = live
                .skills
                .spellbook_successor(id)
                .filter(|&next| self.has_spell(next));
            if successor.is_some() {
                active = false;
            }
            if let Some(old) = self.spells.get_mut(&id) {
                let known = old.state != Removed;
                if known
                    && old.active == active
                    && old.dependent == dependent
                    && old.disabled == disabled
                {
                    if !in_world && !learning {
                        old.state = Unchanged;
                    }
                    return Ok(false);
                }

                let dependent_set = known && !old.dependent && dependent;
                if dependent_set {
                    old.dependent = true;
                    if old.state != New {
                        old.state = Changed;
                    }
                }

                if known && old.active != active && !old.disabled {
                    old.active = active;
                    if !in_world && !learning && !dependent_set {
                        old.state = Unchanged;
                    } else if old.state != New {
                        old.state = Changed;
                    }
                    if active
                        && needs_cast_at_learn(
                            template,
                            live.units.get(live.objects, self.object())?,
                        )?
                    {
                        return Err(refuse("execution.player.Player_3204.passive_cast_owner"));
                    }
                    if !active && in_world {
                        return Err(refuse(if successor.is_some() {
                            "execution.player.Player_3211.send_superseded_spell_owner"
                        } else {
                            "execution.player.Player_3214.send_removed_spell_owner"
                        }));
                    }
                    return Ok(active);
                }

                if known && old.disabled != disabled {
                    if old.state != New {
                        old.state = Changed;
                    }
                    old.disabled = disabled;
                    if disabled {
                        return Ok(false);
                    }
                    reenabled = true;
                } else if old.state == Removed {
                    self.spells.remove(&id);
                    state = Changed;
                } else {
                    if !in_world && !learning && !dependent_set {
                        old.state = Unchanged;
                    }
                    return Ok(false);
                }
            }
        }

        // Player.cpp:3252-3310. Talent removal and the lower-rank tab gate are separate owners.
        let talent = live.spells.is_talent(id);
        if !reenabled {
            require(
                !talent,
                "execution.player.Player_3259.talent_rank_removal_owner",
            )?;
            require(
                live.spells
                    .chain(id)
                    .is_none_or(|chain| chain.previous == 0),
                "execution.player.Player_3284.talent_tab_owner",
            )?;
            if active && !disabled {
                // Player.cpp:3322-3350. Detached replacement only updates the old row.
                for (&old_id, old) in &mut self.spells {
                    if old.state != Removed
                        && old.active
                        && live.skills.spellbook_successor(old_id) == Some(id)
                    {
                        require(
                            !in_world,
                            "execution.player.Player_3339.send_superseded_spell_owner",
                        )?;
                        old.active = false;
                        if old.state != New {
                            old.state = Changed;
                        }
                        superseded = true;
                    }
                }
            }
            self.spells.insert(
                id,
                SourcePlayerSpell {
                    state,
                    active,
                    dependent,
                    disabled,
                },
            );
            if disabled {
                return Ok(false);
            }
        }
        require(
            !talent,
            "execution.player.Player_3379.used_talent_points_owner",
        )?;

        // Player.cpp:3383-3387; SpellMgr.cpp:30-39,1131-1148; Player.h:1493.
        let free = self.free_primary_professions;
        if free != 0
            && template.effect1 == SKILL
            && live
                .skills
                .is_primary_profession(template.effectmiscvalue1 as u32)
            && live.spells.chain(id).is_some_and(|chain| chain.rank == 1)
        {
            live.objects.set_update_value(
                self.object(),
                PLAYER_CHARACTER_POINTS2,
                &mut self.free_primary_professions,
                u32::from((free - 1) as u16),
            )?;
        }

        if needs_cast_at_learn(template, live.units.get(live.objects, self.object())?)? {
            self.cast_granted_passive(live, id)?;
        }
        require(
            !template
                .effects()
                .iter()
                .any(|effect| effect.effect == SKILL_STEP),
            "execution.player.Player_3404.skill_step_cast_owner",
        )?;

        // Player.cpp:3409-3424. Insertion also bounds recursion through learned spell links.
        self.update_spell_trained_skills(live, id)?;
        for row in live.spells.learned_spells(id) {
            if row.auto_learned {
                continue;
            }
            if !in_world || !row.active {
                self.add_spell(live, row.spell, row.active, true, true, false)?;
            } else {
                self.learn_spell(live, row.spell, true, false)?;
            }
        }
        Ok(active && !disabled && !superseded)
    }

    // Player.cpp:3451-3479. The in-world notification belongs to WorldSession.
    pub(super) fn learn_spell(
        &mut self,
        live: &mut PlayerLearning<'_, '_>,
        id: u32,
        dependent: bool,
        talent: bool,
    ) -> Result<()> {
        let disabled = self.spells.get(&id).filter(|spell| spell.disabled);
        let active = disabled.is_none_or(|spell| spell.active);
        let was_disabled = disabled.is_some();
        let learned = self.add_spell(live, id, active, true, dependent, false)?;
        require(
            !learned || !live.objects.get(self.object())?.in_world,
            "execution.player.Player_3465.send_learned_spell_owner",
        )?;
        if was_disabled || talent {
            for &child in live.spells.next_ranks(id) {
                if self.spells.get(&child).is_some_and(|spell| spell.disabled) {
                    self.learn_spell(live, child, false, false)?;
                }
            }
        }
        Ok(())
    }

    // Player.cpp:5918-5999. At Create the rows already passed ObjectMgr's bounds check.
    pub(super) fn add_action_button(
        &mut self,
        live: &PlayerLearning<'_, '_>,
        row: PlayerCreateAction,
    ) -> Result<()> {
        if !row.in_bounds() {
            return Ok(());
        }
        match row.kind {
            0 if !live.spells.template(row.action).is_some_and(|template| {
                self.has_spell(row.action) && template.attributes & PASSIVE == 0
            }) =>
            {
                return Ok(())
            }
            128 if live.items.item_prototype(row.action)?.is_none() => return Ok(()),
            _ => {}
        }
        // Player.h:171-179 changes the state only on a write or revival.
        let button = self.action_buttons.entry(row.button).or_default();
        let packed_data = row.action | (u32::from(row.kind) << 24);
        if button.packed_data != packed_data || button.state == SourcePlayerActionState::Deleted {
            button.packed_data = packed_data;
            if button.state != SourcePlayerActionState::New {
                button.state = SourcePlayerActionState::Changed;
            }
        }
        Ok(())
    }
}

// Player.cpp:3430-3448; SpellMgr.h:1376-1385.
fn needs_cast_at_learn(template: &SpellTemplate, unit: &SourceUnitFields) -> Result<bool> {
    if template.attributesex & CAST_WHEN_LEARNED != 0 {
        return Ok(true);
    }
    let form = unit.shapeshift_form;
    require(
        form == 0 || template.attributes & (PASSIVE | DO_NOT_DISPLAY) == 0,
        "execution.player.SpellMgr_1383.form_apply_owner",
    )?;
    if template.attributes & PASSIVE == 0 {
        return Ok(false);
    }
    let need_cast = template.stances == 0
        || (form == 0 && template.attributesex2 & ALLOW_WHILE_NOT_SHAPESHIFTED != 0);
    if need_cast {
        require(
            template.casteraurastate == 0,
            "execution.player.Player_3448.caster_aura_state_owner",
        )?;
    }
    Ok(need_cast)
}
