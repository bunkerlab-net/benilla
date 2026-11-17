// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! ObjectMgr.cpp:870-881,945-976; Creature.cpp:2462-2480.
use super::*;
use crate::native::production_compiler::VerifiedCatalog;
use std::collections::BTreeMap;
const IMMUNITY_COUNT: usize = 6; // SharedDefines.h:462-472.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreatureImmunity {
    kind: usize,
    pub value: u32,
}
#[derive(Debug, Default)]
pub struct LoadedCreatureImmunities {
    sets: BTreeMap<(u32, u32), Vec<CreatureImmunity>>,
}
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CreatureImmunityCensus {
    pub visited: usize,
    pub loaded: usize,
    pub missing_entry: usize,
    pub invalid_type: usize,
}
impl LoadedCreatureImmunities {
    pub fn load_catalog(
        catalog: &mut VerifiedCatalog,
        creatures: &LoadedCreatureTemplates,
    ) -> Result<(Self, CreatureImmunityCensus)> {
        let rows = catalog.execution_creature_immunities()?;
        Ok(Self::load(&rows, |entry| creatures.get(entry).is_some()))
    }
    pub fn load(
        rows: &[[u32; 4]],
        mut creature_exists: impl FnMut(u32) -> bool,
    ) -> (Self, CreatureImmunityCensus) {
        let mut store = Self::default();
        let mut census = CreatureImmunityCensus::default();
        for &[entry, set, kind, value] in rows {
            census.visited += 1;
            // Validation order and the two logged-and-skipped branches, ObjectMgr.cpp:957-967.
            if !creature_exists(entry) {
                census.missing_entry += 1;
                continue;
            }
            if kind >= IMMUNITY_COUNT as u32 {
                census.invalid_type += 1;
                continue;
            }
            store
                .sets
                .entry((entry, set))
                .or_default()
                .push(CreatureImmunity {
                    kind: kind as usize,
                    value,
                });
            census.loaded += 1;
        }
        (store, census)
    }
    pub fn get(&self, entry: u32, set: u32) -> Option<&[CreatureImmunity]> {
        self.sets.get(&(entry, set)).map(Vec::as_slice)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SpellImmune {
    aura: Option<u64>,
    value: u32,
}
#[derive(Debug)]
pub struct CreatureImmunityState {
    pub set: u32,
    spells: [Vec<SpellImmune>; IMMUNITY_COUNT],
}
impl Default for CreatureImmunityState {
    fn default() -> Self {
        Self {
            set: u32::MAX,
            spells: std::array::from_fn(|_| Vec::new()),
        } // Creature.cpp:145.
    }
}
impl CreatureImmunityState {
    /// Creature.cpp:1983-1997 skips template masks for self casts.
    /// Unit.cpp:7343-7383 still evaluates positivity before the school loop.
    pub fn immune_to_self_spell(
        &self,
        stores: &SourceSpellStores,
        spell: &SpellTemplate,
        mask: u32,
        unit: ObjectHandle,
        auras: &CreatureAddonState,
    ) -> Result<bool> {
        if self.spells[4]
            .iter()
            .any(|entry| entry.value == spell.dispel)
        {
            return Ok(true);
        }
        let mut positive = positive_unit_spell(stores, spell, mask, Some((unit, unit)))?;
        if aura_apply_effects(spell, mask) && spell.attributes & 0x0400_0000 != 0 {
            positive = false;
        }
        for entry in &self.spells[2] {
            require(
                spell.school < 32,
                "execution.spells.GetSchoolMask_shift_undefined",
            )?;
            if entry.value & (1 << spell.school) == 0 {
                continue;
            }
            if entry.aura.is_some() {
                let _ = positive;
                return Err(refuse("execution.spells.Unit_7365.Aura_IsPositive_owner"));
            }
            // CanPierceImmuneAura(nullptr), SpellMgr.h:1295-1301.
            if spell.attributes & 0x2000_0000 == 0 {
                return Ok(true);
            }
        }
        if spell.mechanic != 0 {
            if self.spells[5]
                .iter()
                .any(|entry| entry.value == spell.mechanic)
            {
                return Ok(true);
            }
            if auras.has_aura_type(147) {
                return Err(refuse("execution.spells.Unit_7377.Aura_GetModifier_owner"));
            }
        }
        Ok(false)
    }
    // Unit::ApplySpellImmune(nullptr,...), Unit.cpp:7680-7708. Only loaded validated kinds enter here.
    fn apply(&mut self, immunity: CreatureImmunity, apply: bool) {
        let list = &mut self.spells[immunity.kind];
        if apply {
            list.retain(|old| old.value != immunity.value);
            list.push(SpellImmune {
                aura: None,
                value: immunity.value,
            });
        } else if let Some(index) = list
            .iter()
            .position(|old| old.aura.is_none() && old.value == immunity.value)
        {
            list.remove(index);
        }
    }
    pub fn update(
        &mut self,
        entry: u32,
        set: u32,
        store: Option<&LoadedCreatureImmunities>,
    ) -> Result<()> {
        if self.set == set {
            return Ok(());
        }
        let store =
            store.ok_or_else(|| refuse("execution.creature.loaded_immunity_set_store_owner"))?;
        let Some(new) = store.get(entry, set) else {
            return Ok(());
        };
        if let Some(old) = store.get(entry, self.set) {
            for &immunity in old {
                self.apply(immunity, false);
            }
        }
        for &immunity in new {
            self.apply(immunity, true);
        }
        self.set = set;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fresh_self_spell_immunity_uses_loaded_sets_and_keeps_aura_dependencies() {
        let stores = SourceSpellStores::default();
        let unit = ObjectHandle {
            thread: 6,
            construction: 1,
        };
        let mut auras = CreatureAddonState::default();
        let mut spell = SpellTemplate {
            effect1: 6,
            effectimplicittargeta1: 1,
            school: 1,
            dispel: 7,
            mechanic: 2,
            ..Default::default()
        };
        let mut state = CreatureImmunityState::default();
        assert!(!state
            .immune_to_self_spell(&stores, &spell, 1, unit, &auras)
            .unwrap());
        let (loaded, _) =
            LoadedCreatureImmunities::load(&[[1, 0, 4, 7], [1, 1, 2, 2], [1, 2, 5, 2]], |_| true);
        for set in 0..3 {
            state.update(1, set, Some(&loaded)).unwrap();
            assert!(state
                .immune_to_self_spell(&stores, &spell, 1, unit, &auras)
                .unwrap());
        }
        state.update(1, 1, Some(&loaded)).unwrap();
        spell.attributes = 0x2000_0000;
        assert!(!state
            .immune_to_self_spell(&stores, &spell, 1, unit, &auras)
            .unwrap());
        spell.school = 32;
        assert_eq!(
            state
                .immune_to_self_spell(&stores, &spell, 1, unit, &auras)
                .unwrap_err()
                .id,
            "execution.spells.GetSchoolMask_shift_undefined"
        );
        spell.school = 31;
        assert!(!state
            .immune_to_self_spell(&stores, &spell, 1, unit, &auras)
            .unwrap());
        auras.aura_types.insert(147, vec![1]);
        assert_eq!(
            state
                .immune_to_self_spell(&stores, &spell, 1, unit, &auras)
                .unwrap_err()
                .id,
            "execution.spells.Unit_7377.Aura_GetModifier_owner"
        );
    }
    #[test]
    fn creature_immunities_validation_keeps_source_order_and_error_precedence() {
        let (store, census) = LoadedCreatureImmunities::load(
            &[
                [9, 0, 99, 1],
                [1, 0, 6, 2],
                [1, 0, 5, u32::MAX],
                [1, 0, 0, 7],
                [1, 0, 0, 7],
            ],
            |entry| entry == 1,
        );
        assert_eq!(
            census,
            CreatureImmunityCensus {
                visited: 5,
                loaded: 3,
                missing_entry: 1,
                invalid_type: 1
            }
        );
        assert!(store.get(9, 0).is_none() && store.get(1, 9).is_none());
        assert_eq!(
            store
                .get(1, 0)
                .unwrap()
                .iter()
                .map(|r| (r.kind, r.value))
                .collect::<Vec<_>>(),
            [(5, u32::MAX), (0, 7), (0, 7)]
        );
        let (empty, census) =
            LoadedCreatureImmunities::load(&[], |_| panic!("empty result has no lookups"));
        assert_eq!(census, CreatureImmunityCensus::default());
        assert!(empty.get(1, 0).is_none());
    }
    #[test]
    fn creature_immunities_same_set_returns_before_lookup_missing_new_preserves_old() {
        let mut state = CreatureImmunityState::default();
        state.update(1, u32::MAX, None).unwrap();
        assert_eq!(
            state.update(1, 0, None).unwrap_err().id,
            "execution.creature.loaded_immunity_set_store_owner"
        );
        let (store, _) = LoadedCreatureImmunities::load(&[[1, 0, 2, 7]], |_| true);
        state.update(1, 0, Some(&store)).unwrap();
        state.update(1, 0, None).unwrap();
        state.update(1, 99, Some(&store)).unwrap();
        assert_eq!(state.set, 0);
        assert_eq!(
            state.spells[2],
            [SpellImmune {
                aura: None,
                value: 7
            }]
        );
    }
    #[test]
    fn creature_immunities_switch_removes_old_then_applies_new_with_null_aura_rules() {
        let (store, _) =
            LoadedCreatureImmunities::load(&[[1, 0, 2, 7], [1, 0, 2, 7], [1, 1, 2, 8]], |_| true);
        let mut state = CreatureImmunityState::default();
        state.spells[2].push(SpellImmune {
            aura: Some(10),
            value: 7,
        });
        state.update(1, 0, Some(&store)).unwrap();
        assert_eq!(
            state.spells[2],
            [SpellImmune {
                aura: None,
                value: 7
            }]
        );
        state.spells[2].push(SpellImmune {
            aura: Some(11),
            value: 7,
        });
        state.update(1, 1, Some(&store)).unwrap();
        assert_eq!(state.set, 1);
        assert_eq!(
            state.spells[2],
            [
                SpellImmune {
                    aura: Some(11),
                    value: 7
                },
                SpellImmune {
                    aura: None,
                    value: 8
                }
            ]
        );
    }
}
