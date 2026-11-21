// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::*;
use QuestLookup::*;

const EXPLORATION: u32 = 0x02;
const DELIVER: u32 = 0x08;
const KILL_SPEAK: u32 = 0x30;

impl SourceQuestTemplate {
    // ObjectMgr.cpp:4309-4788. Log-only checks leave the loaded fields intact.
    pub(super) fn validate(
        &mut self,
        owner: &mut impl QuestLoaderOwner,
        used_mail: &mut BTreeSet<u32>,
        out: &mut Vec<QuestLoadNotice>,
    ) -> Result<()> {
        let mut log = |line, operand| notice(out, line, self.id, operand);
        if self.method >= 3 {
            log(4311, 0);
        }
        if self.special_flags > 3 {
            log(4316, 0);
        } // QuestDef.h:161.
        if self.quest_flags & 0x400 != 0 {
            // QuestDef.h:143.
            for (i, &item) in self.choice_items.iter().enumerate().skip(1) {
                if item != 0 {
                    log(4326, i as u32 + 1);
                }
            }
        }
        if self.zone_or_sort > 0 && !contains(owner, 4336, Area, self.zone_or_sort as u32)? {
            log(4338, 0);
        }
        if self.zone_or_sort < 0 {
            let sort = self.zone_or_sort.checked_neg().ok_or_else(|| {
                refuse("execution.quests.ObjectMgr_4346.zone_or_sort_negation_undefined")
            })? as u32;
            if !contains(owner, 4346, QuestSort, sort)? {
                log(4349, 0);
            }
            if let Some(skill) = skill_by_quest_sort(sort) {
                if self.required_skill != skill {
                    log(4359, skill);
                }
            }
        }
        // SharedDefines.h:35-57,72-96. A mixed mask with any playable bit survives.
        if self.required_classes != 0 && self.required_classes & 0x5df == 0 {
            log(4371, 0);
            self.required_classes = 0;
        }
        if self.required_races != 0 && self.required_races & 0xff == 0 {
            log(4381, 0);
            self.required_races = 0;
        }
        if self.required_skill != 0 && !contains(owner, 4389, SkillLine, self.required_skill)? {
            log(4391, 0);
        }
        if self.required_skill_value != 0
            && self.required_skill_value
                > u32::from(at(owner.max_skill(), 4398, "max_skill_config")?)
        {
            log(4400, 0);
        }
        for (lookup, issue, faction) in [
            (4407, 4409, self.rep_objective_faction),
            (4414, 4416, self.required_min_rep_faction),
            (4421, 4423, self.required_max_rep_faction),
        ] {
            if faction != 0 && !contains(owner, lookup, Faction, faction)? {
                log(issue, 0);
            }
        }
        if self.required_min_rep_value > 42_999 {
            log(4430, 0);
        } // ReputationMgr.h:67.
        if self.required_min_rep_value != 0
            && self.required_max_rep_value != 0
            && self.required_max_rep_value <= self.required_min_rep_value
        {
            log(4437, 0);
        }
        for (line, faction, value) in [
            (4444, self.rep_objective_faction, self.rep_objective_value),
            (
                4451,
                self.required_min_rep_faction,
                self.required_min_rep_value,
            ),
            (
                4458,
                self.required_max_rep_faction,
                self.required_max_rep_value,
            ),
        ] {
            if faction == 0 && value > 0 {
                log(line, 0);
            }
        }
        if self.source_item != 0 {
            if !contains(owner, 4465, Item, self.source_item)? {
                log(4467, 0);
                self.source_item = 0;
            } else if self.source_item_count == 0 {
                log(4473, 0);
                self.source_item_count = 1;
            }
        } else if self.source_item_count != 0 {
            log(4480, 0);
            self.source_item_count = 0;
        }
        if self.source_spell != 0 {
            if at(
                owner.spell_effects(self.source_spell),
                4487,
                "spell_template",
            )?
            .is_none()
            {
                log(4490, 0);
                self.source_spell = 0;
            } else if !at(owner.spell_valid(self.source_spell), 4494, "spell_validity")? {
                log(4496, 0);
                self.source_spell = 0;
            }
        }
        for (i, (&item, count)) in self
            .required_items
            .iter()
            .zip(&mut self.required_item_counts)
            .enumerate()
        {
            let slot = i as u32 + 1;
            if item != 0 {
                if *count == 0 {
                    log(4508, slot);
                }
                self.special_flags |= DELIVER;
                if !contains(owner, 4515, Item, item)? {
                    log(4517, slot);
                    *count = 0;
                }
            } else if *count != 0 {
                log(4524, slot);
                *count = 0;
            }
        }
        for (i, (&item, &count)) in self
            .required_source_items
            .iter()
            .zip(&self.required_source_item_counts)
            .enumerate()
        {
            if item != 0 {
                if !contains(owner, 4534, Item, item)? {
                    log(4536, i as u32 + 1);
                }
            } else if count != 0 {
                log(4545, i as u32 + 1);
            }
        }
        for (i, (&spell, &target)) in self
            .required_spells
            .iter()
            .zip(&self.required_targets)
            .enumerate()
        {
            if spell == 0 {
                continue;
            }
            let Some(effects) = at(owner.spell_effects(spell), 4556, "spell_template")? else {
                log(4559, i as u32 + 1);
                continue;
            };
            if target == 0 {
                if effects
                    .iter()
                    .any(|&(effect, misc)| (effect == 16 && misc as u32 == self.id) || effect == 61)
                {
                    if self.special_flags & EXPLORATION == 0 {
                        log(4581, i as u32 + 1);
                        self.special_flags |= EXPLORATION;
                    }
                } else {
                    log(4589, i as u32 + 1);
                }
            }
        }
        for (i, (target, &count)) in self
            .required_targets
            .iter_mut()
            .zip(&self.required_target_counts)
            .enumerate()
        {
            let slot = i as u32 + 1;
            let original = *target;
            if original < 0 {
                let id = original.checked_neg().ok_or_else(|| {
                    refuse("execution.quests.ObjectMgr_4600.target_negation_undefined")
                })? as u32;
                if at(owner.gameobject_type(id), 4600, "gameobject_template")?.is_none() {
                    log(4602, slot);
                    *target = 0;
                }
            }
            if original > 0 && !contains(owner, 4607, Creature, original as u32)? {
                log(4609, slot);
                *target = 0;
            }
            if original != 0 {
                self.special_flags |= KILL_SPEAK;
                if count == 0 {
                    log(4622, slot);
                }
            } else if count != 0 {
                log(4629, slot);
            }
        }
        let mut choice_found = false;
        for i in (0..self.choice_items.len()).rev() {
            let slot = i as u32 + 1;
            let item = self.choice_items[i];
            if item != 0 {
                if contains(owner, 4640, Item, item)? {
                    choice_found = true;
                } else {
                    log(4642, slot);
                    self.choice_items[i] = 0;
                }
                if self.choice_item_counts[i] == 0 {
                    log(4651, slot);
                }
            } else if choice_found {
                // Only a later present item can set choice_found.
                log(4658, slot);
                self.choice_items[i] = self.choice_items[i + 1];
                self.choice_item_counts[i] = self.choice_item_counts[i + 1];
            } else if self.choice_item_counts[i] != 0 {
                log(4666, slot);
            }
        }
        for (i, (item, &count)) in self
            .reward_items
            .iter_mut()
            .zip(&self.reward_item_counts)
            .enumerate()
        {
            let slot = i as u32 + 1;
            if *item != 0 {
                if !contains(owner, 4676, Item, *item)? {
                    log(4678, slot);
                    *item = 0;
                }
                if count == 0 {
                    log(4685, slot);
                }
            } else if count != 0 {
                log(4692, slot);
            }
        }
        for (i, (faction, &value)) in self
            .reward_factions
            .iter_mut()
            .zip(&self.reward_rep_values)
            .enumerate()
        {
            let slot = i as u32 + 1;
            if *faction != 0 {
                if value == 0 {
                    log(4703, slot);
                }
                if !contains(owner, 4706, Faction, *faction)? {
                    log(4708, slot);
                    *faction = 0;
                }
            } else if value != 0 {
                log(4715, slot);
            }
        }
        for (spell, [lookup, missing, valid, broken, talent, is_talent]) in [
            (&mut self.reward_spell, [4723, 4727, 4731, 4733, 4737, 4739]),
            (
                &mut self.reward_spell_cast,
                [4747, 4751, 4755, 4757, 4761, 4763],
            ),
        ] {
            if *spell == 0 {
                continue;
            }
            let issue = if at(owner.spell_effects(*spell), lookup, "spell_template")?.is_none() {
                missing
            } else if !at(owner.spell_valid(*spell), valid, "spell_validity")? {
                broken
            } else if contains(owner, talent, Talent, *spell)? {
                is_talent
            } else {
                continue;
            };
            log(issue, 0);
            *spell = 0;
        }
        if self.reward_mail_template != 0 {
            let issue = if !contains(owner, 4771, MailTemplate, self.reward_mail_template)? {
                Some(4773)
            } else if !used_mail.insert(self.reward_mail_template) {
                Some(4781)
            } else {
                None
            };
            if let Some(line) = issue {
                log(line, 0);
                self.reward_mail_template = 0;
                self.reward_mail_delay = 0;
            }
        }
        Ok(())
    }
}

// SharedDefines.h:1459-1474. Only profession sorts imply a skill.
fn skill_by_quest_sort(sort: u32) -> Option<u32> {
    Some(match sort {
        24 => 182,
        101 => 356,
        121 => 164,
        181 => 171,
        182 => 165,
        201 => 202,
        264 => 197,
        304 => 185,
        324 => 129,
        _ => return None,
    })
}
