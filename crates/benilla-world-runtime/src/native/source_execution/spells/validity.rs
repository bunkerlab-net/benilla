// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpellValidityNotice {
    pub line: u32,
    pub spell: u32,
    /// Missing item or invalid triggered spell.
    pub operand: u32,
}

impl SourceSpellStores {
    /// SpellMgr.cpp:2004-2075. `msg` controls diagnostic records.
    pub fn is_spell_valid(
        &self,
        spell: Option<&SpellTemplate>,
        items: Option<&SourceStartupItems>,
        msg: bool,
        observe: &mut impl FnMut(SpellValidityNotice),
    ) -> Result<bool> {
        let Some(spell) = spell else { return Ok(false) };
        let mut check_reagents = false;
        for (effect, item, trigger) in [
            (
                spell.effect1,
                spell.effectitemtype1,
                spell.effecttriggerspell1,
            ),
            (
                spell.effect2,
                spell.effectitemtype2,
                spell.effecttriggerspell2,
            ),
            (
                spell.effect3,
                spell.effectitemtype3,
                spell.effecttriggerspell3,
            ),
        ] {
            match effect {
                // SpellEffectDefines.h:50. Check every product before checking reagents.
                24 => {
                    if !has_item(
                        items,
                        item,
                        "execution.spells.SpellMgr_2023.item_template_owner",
                    )? {
                        if msg {
                            observe(SpellValidityNotice {
                                line: 2030,
                                spell: spell.id,
                                operand: item,
                            });
                        }
                        return Ok(false);
                    }
                    check_reagents = true;
                }
                // SpellEffectDefines.h:62. Report a bad child before its parent.
                36 if !self.is_spell_valid(self.template(trigger), items, msg, observe)? => {
                    if msg {
                        observe(SpellValidityNotice {
                            line: 2048,
                            spell: spell.id,
                            operand: trigger,
                        });
                    }
                    return Ok(false);
                }
                _ => (),
            }
        }
        if check_reagents {
            for reagent in [
                spell.reagent1,
                spell.reagent2,
                spell.reagent3,
                spell.reagent4,
                spell.reagent5,
                spell.reagent6,
                spell.reagent7,
                spell.reagent8,
            ] {
                if reagent > 0
                    && !has_item(
                        items,
                        reagent as u32,
                        "execution.spells.SpellMgr_2061.item_template_owner",
                    )?
                {
                    if msg {
                        observe(SpellValidityNotice {
                            line: 2068,
                            spell: spell.id,
                            operand: reagent as u32,
                        });
                    }
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}

fn has_item(items: Option<&SourceStartupItems>, id: u32, refusal: &str) -> Result<bool> {
    items
        .ok_or_else(|| refuse(refusal))?
        .item_prototype(id)
        .map(|item| item.is_some())
        .map_err(|_| refuse(refusal))
}

#[cfg(test)]
#[path = "validity_tests.rs"]
mod tests;
