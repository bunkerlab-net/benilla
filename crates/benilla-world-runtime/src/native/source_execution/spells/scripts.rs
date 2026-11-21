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

impl SourceSpellScript {
    // SpellScript.h:96. This script inherits the duration default.
    pub(super) fn on_duration_calculate(script: Option<Self>, duration: i32) -> i32 {
        match script {
            None | Some(Self::DetectThroughInvisibilityMob) => duration,
        }
    }

    // SpellAuras.cpp:6041-6042, SpellScript.h:88.
    pub(super) fn on_holder_init(self, _holder: &mut SourceAuraHolder) {
        match self {
            Self::DetectThroughInvisibilityMob => (),
        }
    }

    // SpellAuras.cpp:6108-6111, SpellScript.h:92.
    pub(super) fn on_aura_value_calculate(self, value: i32) -> i32 {
        match self {
            Self::DetectThroughInvisibilityMob => value,
        }
    }

    // SpellAuras.cpp:6102-6103, SpellScript.h:90.
    pub(super) fn on_aura_init(self, _aura: &mut SourceAura) {
        match self {
            Self::DetectThroughInvisibilityMob => (),
        }
    }

    // SpellAuras.cpp:6130-6131, spell_scripts.cpp:88-92.
    pub(super) fn on_apply(self, aura: &mut SourceAura, apply: bool) {
        match self {
            Self::DetectThroughInvisibilityMob => {
                if apply {
                    aura.force_periodicity(1000);
                }
            }
        }
    }

    // SpellAuras.cpp:6136-6137, SpellScript.h:105.
    pub(super) fn on_after_apply(self, _aura: &mut SourceAura, _apply: bool) {
        match self {
            Self::DetectThroughInvisibilityMob => (),
        }
    }
}

impl SourceAura {
    // SpellAuras.cpp:6199-6207.
    fn force_periodicity(&mut self, time: u32) {
        self.periodic = time != 0;
        self.periodic_time = time;
        self.periodic_timer = time as i32;
    }
}

#[cfg(test)]
mod tests;
