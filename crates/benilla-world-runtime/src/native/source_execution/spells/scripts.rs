// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::*;

pub struct SourceSpellScriptRegistry;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceSpellScript {
    DetectThroughInvisibilityMob,
}

impl SpellLoaderOwner for SourceSpellScriptRegistry {
    fn script(&mut self, kind: SpellScriptKind, name: &[u8]) -> Result<Option<SpellScriptBinding>> {
        // spell_scripts.cpp:453. SpellScript.h:171-178 registers each base separately.
        let (script, registration) = match name {
            b"spell_detect_through_invisibility_mob" => {
                (SourceSpellScript::DetectThroughInvisibilityMob, 1)
            }
            _ => {
                return Err(refuse(&format!(
                    "{}.{}",
                    kind.registry_owner(),
                    String::from_utf8_lossy(name)
                )))
            }
        };
        match (script, kind) {
            (SourceSpellScript::DetectThroughInvisibilityMob, SpellScriptKind::Spell) => Ok(None),
            (SourceSpellScript::DetectThroughInvisibilityMob, SpellScriptKind::Aura) => {
                Ok(Some(SpellScriptBinding {
                    name: name.into(),
                    registration,
                    script: Some(script),
                }))
            }
        }
    }
}

#[cfg(test)]
mod tests;
