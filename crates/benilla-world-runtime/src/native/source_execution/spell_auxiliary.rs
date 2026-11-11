// SPDX-License-Identifier: GPL-3.0-or-later
//! Additional SpellMgr tables and first-insertion script bindings.
use super::*;
impl SourceSpellStores {
    pub fn elixir_mask(&self, id: u32) -> u8 {
        self.elixirs.get(&id).copied().unwrap_or(0)
    }
    pub fn affect_mask(&self, id: u32, effect: usize) -> Result<u64> {
        // SpellMgr.h:1951-1958. The fallback indexes SpellEntry only if found.
        if let Some(mask) = self
            .affects
            .get(&id.wrapping_shl(8).wrapping_add(effect as u32))
        {
            return Ok(*mask);
        }
        let Some(s) = self.template(id) else {
            return Ok(0);
        };
        [s.effectitemtype1, s.effectitemtype2, s.effectitemtype3]
            .get(effect)
            .copied()
            .map(u64::from)
            .ok_or_else(|| refuse("execution.spells.affect_effect_index_undefined"))
    }
    pub fn learned_spells(&self, id: u32) -> &[SpellLearned] {
        self.learned.get(&id).map_or(&[], Vec::as_slice)
    }
    pub fn learned_skill(&self, id: u32) -> Option<(u16, u16, u32)> {
        self.skills.get(&id).copied()
    }
    pub fn pet_aura(&self, id: u32) -> Option<&SpellPetAura> {
        self.pet_auras.get(&(id as u16))
    }
    pub fn script_binding(
        &self,
        kind: SpellScriptKind,
        id: u32,
    ) -> Result<Option<&SpellScriptBinding>> {
        self.check_key("spell_scripts", id)?;
        Ok(match kind {
            SpellScriptKind::Spell => self.spell_scripts.get(&id),
            SpellScriptKind::Aura => self.aura_scripts.get(&id),
        })
    }
    pub(super) fn is_talent(&self, id: u32) -> bool {
        self.talent_spells.contains(&id)
    }
    pub(super) fn load_affect(&mut self, v: &[serde_json::Value]) -> Result<Option<&'static str>> {
        let id = v[0].as_u64().unwrap() as u32;
        let effect = v[1].as_u64().unwrap() as usize;
        if effect >= 3 {
            return Ok(Some("effect_index"));
        }
        if self.first_rank(id) != id {
            return Ok(Some("nonfirst_rank"));
        }
        let s = self.template(id).expect("parent checked spell");
        let e = s.effects()[effect];
        if e.effect != 6 || ![4, 107, 108, 109, 112].contains(&e.aura) {
            return Ok(Some("affect_aura"));
        }
        let mask = v[2].as_u64().unwrap();
        let item = [s.effectitemtype1, s.effectitemtype2, s.effectitemtype3][effect];
        if item != 0 && u64::from(item) == mask {
            return Ok(Some("redundant_mask"));
        }
        // SpellMgr.cpp:2956-2959. Insert, not overwrite, including high ranks.
        self.affects
            .entry(id.wrapping_shl(8).wrapping_add(effect as u32))
            .or_insert(mask);
        for child in self.higher(id)? {
            self.affects
                .entry(child.wrapping_shl(8).wrapping_add(effect as u32))
                .or_insert(mask);
        }
        Ok(None)
    }
    pub(super) fn add_template_learning(&mut self) -> Result<()> {
        // SpellMgr.cpp:1721-1763, skipped entirely when the SQL relation is empty.
        for (&id, s) in &self.templates {
            for e in s.effects() {
                if e.effect != 36 || self.template(e.trigger).is_none() {
                    continue;
                }
                let auto = e.target_a == 5
                    || self.is_talent(id)
                    || s.attributes & 0x40 != 0
                    || s.effects().iter().any(|e| e.effect == 44);
                let list = self.learned.entry(id).or_default();
                if !list.iter().any(|n| n.spell == e.trigger) {
                    list.push(SpellLearned {
                        spell: e.trigger,
                        active: true,
                        auto_learned: auto,
                    });
                }
            }
        }
        Ok(())
    }
    pub(super) fn load_learn_skills(&mut self) -> Result<()> {
        // SpellMgr.cpp:1642-1653; CalculateSimpleValue at DBCStructure.h:817.
        for (&id, s) in &self.templates {
            let effects = s.effects();
            let misc = [s.effectmiscvalue1, s.effectmiscvalue2, s.effectmiscvalue3];
            let points = [
                s.effectbasepoints1,
                s.effectbasepoints2,
                s.effectbasepoints3,
            ];
            let dice = [s.effectbasedice1, s.effectbasedice2, s.effectbasedice3];
            for i in 0..3 {
                if effects[i].effect != 118 {
                    continue;
                }
                let skill = misc[i] as u16;
                // Evaluation occurs even when the narrowed skill is zero.
                let step = points[i]
                    .checked_add(dice[i] as i32)
                    .ok_or_else(|| refuse("execution.spells.simple_value_overflow_undefined"))?
                    as u16;
                if skill != 0 && step != 0 {
                    self.skills.insert(id, (skill, step, 118));
                    break;
                }
            }
        }
        Ok(())
    }
    pub(super) fn load_pet_aura(
        &mut self,
        v: &[serde_json::Value],
    ) -> Result<Option<&'static str>> {
        let n = |i: usize| v[i].as_u64().unwrap() as u32;
        let id = n(0);
        let key = id as u16;
        // SpellMgr.cpp:1958 uses map<uint16,...>::find before validation.
        if let Some(pa) = self.pet_auras.get_mut(&key) {
            pa.auras.insert(n(1), n(2));
            return Ok(None);
        }
        let Some(s) = self.template(id) else {
            return Ok(Some("missing_spell"));
        };
        let Some(i) = s
            .effects()
            .iter()
            .position(|e| e.effect == 3 || e.effect == 6 && e.aura == 4)
        else {
            return Ok(Some("pet_dummy"));
        };
        if self.template(n(2)).is_none() {
            return Ok(Some("missing_aura"));
        }
        let points = [
            s.effectbasepoints1,
            s.effectbasepoints2,
            s.effectbasepoints3,
        ][i];
        let dice = [s.effectbasedice1, s.effectbasedice2, s.effectbasedice3][i];
        let damage = points
            .checked_add(dice as i32)
            .ok_or_else(|| refuse("execution.spells.simple_value_overflow_undefined"))?;
        let remove = s.effects()[i].target_a == 5;
        self.pet_auras.insert(
            key,
            SpellPetAura {
                auras: BTreeMap::from([(n(1), n(2))]),
                remove_on_change_pet: remove,
                damage,
            },
        );
        Ok(None)
    }
    pub(super) fn bind_script(
        &mut self,
        id: u32,
        name: &[u8],
        owner: &mut impl SpellLoaderOwner,
    ) -> Result<Option<&'static str>> {
        // SpellScript.cpp:87-104,124-132. Each registry is independent and first wins.
        if self.template(id).is_none() {
            return Ok(Some("missing_spell"));
        }
        let spell = owner.script(SpellScriptKind::Spell, name)?;
        let found = spell.is_some();
        if let Some(binding) = spell {
            self.spell_scripts.entry(id).or_insert(binding);
        }
        let aura = owner.script(SpellScriptKind::Aura, name)?;
        if !found && aura.is_none() {
            return Ok(Some("missing_script"));
        }
        if let Some(binding) = aura {
            self.aura_scripts.entry(id).or_insert(binding);
        }
        Ok(None)
    }
}
