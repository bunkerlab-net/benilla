// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

fn unit(construction: u64) -> ObjectHandle {
    ObjectHandle {
        thread: 6,
        construction,
    }
}

pub(in crate::native::source_execution) fn authored_effect_holder(
    t: &SpellTemplate,
    target: ObjectHandle,
    kind: u32,
) -> SourceAuraHolder {
    let mut holder = SourceAuraHolder::new(t, target, 0, 0, -1).unwrap();
    holder.effects[0] = Some(SourceAura::self_spell_effect(t, 0, kind, 0, 0, 1, 0).unwrap());
    holder
}

#[test]
fn aura_holders_iterate_by_spell_then_insertion_and_keep_identity() {
    let mut auras = SourceUnitAuras::default();
    let spell = SpellTemplate {
        id: 90,
        ..Default::default()
    };
    let lower = SpellTemplate {
        id: 10,
        ..Default::default()
    };
    let mut one = authored_effect_holder(&spell, unit(1), 95);
    one.effects[2] = one.effects[0].clone();
    let first = auras.store_aura_holder(90, &mut Some(one)).unwrap();
    let mut two = authored_effect_holder(&spell, unit(1), 95);
    two.caster = unit(2);
    let second = auras.store_aura_holder(90, &mut Some(two)).unwrap();
    let early = auras
        .store_aura_holder(10, &mut Some(authored_effect_holder(&lower, unit(1), 95)))
        .unwrap();
    assert_ne!(first, second);
    assert_eq!(
        auras
            .aura_holders()
            .map(|(id, holder)| (id, id.spell(), holder.caster))
            .collect::<Vec<_>>(),
        [
            (early, 10, unit(1)),
            (first, 90, unit(1)),
            (second, 90, unit(2))
        ]
    );
    assert_eq!(auras.first_aura_holder(90).unwrap().caster, unit(1));
    assert_eq!(auras.aura_holder(first).unwrap().caster, unit(1));
    assert_eq!(auras.aura_holder(second).unwrap().caster, unit(2));
    assert_eq!(
        auras.auras_of_type(95),
        &[
            SourceAuraEffectRef {
                holder: first,
                effect: 0,
            },
            SourceAuraEffectRef {
                holder: first,
                effect: 2,
            },
            SourceAuraEffectRef {
                holder: second,
                effect: 0,
            },
            SourceAuraEffectRef {
                holder: early,
                effect: 0,
            },
        ]
    );
}

#[test]
fn aura_holder_ordinal_wrap_preserves_pending_holder_and_type_lists() {
    let mut auras = SourceUnitAuras {
        last_holder_ordinal: u64::MAX - 1,
        ..SourceUnitAuras::default()
    };
    let t = SpellTemplate {
        id: 90,
        ..Default::default()
    };
    let holder = authored_effect_holder(&t, unit(1), 95);
    let last = auras
        .store_aura_holder(t.id, &mut Some(holder.clone()))
        .unwrap();
    assert_eq!(last.ordinal, u64::MAX);
    let mut pending = Some(holder.clone());
    assert_eq!(
        auras.store_aura_holder(t.id, &mut pending).unwrap_err().id,
        "execution.spells.Unit_4849.aura_holder_identity_exhausted"
    );
    assert_eq!(pending.as_ref(), Some(&holder));
    assert_eq!(auras.aura_holders().len(), 1);
    assert_eq!(
        auras.auras_of_type(95),
        &[SourceAuraEffectRef {
            holder: last,
            effect: 0,
        }]
    );
    assert_eq!(auras.last_holder_ordinal, u64::MAX);
}

#[derive(Default)]
struct HitOwner {
    auras: SourceUnitAuras,
    applied: Option<SourceAuraHolderId>,
    reentrant: Option<SourceAuraHolderId>,
}

impl SpellAuraHitOwner for HitOwner {
    fn aura_holder_seconds(&mut self, _: &SourceSpell) -> Result<i64> {
        Ok(17)
    }

    fn aura_effect_seconds(&mut self, _: &SourceSpell) -> Result<i64> {
        Ok(20)
    }

    fn aura_hit_target(&mut self, target: ObjectHandle) -> Result<&mut SourceUnitAuras> {
        assert_eq!(target, unit(1));
        Ok(&mut self.auras)
    }

    fn apply_inserted_aura_holder(
        &mut self,
        t: &SpellTemplate,
        holder: SourceAuraHolderId,
        _: Option<(&mut SourceSpellStarts, &mut SourceUnitEvents)>,
    ) -> Result<()> {
        self.applied = Some(holder);
        // Supply a second holder during the unported modifier continuation.
        let mut another = authored_effect_holder(t, unit(1), 95);
        another.caster = unit(2);
        self.reentrant = Some(self.auras.store_aura_holder(t.id, &mut Some(another))?);
        Ok(())
    }

    fn proc_aura_hit(&mut self, _: &mut SourceSpell) -> Result<()> {
        assert!(self.auras.aura_holders().all(|(_, holder)| !holder.ready));
        Ok(())
    }
}

fn hit(t: &SpellTemplate, hit_mask: u8) -> SourceSpell {
    let mut s = SourceSpell::new(1, t, unit(1), SpellTriggerFlags(0), 0).unwrap();
    s.unique_targets.push(SourceSpellTarget {
        unit: unit(1),
        effect_mask: 1,
        hit_mask,
        processed_mask: 0,
        processed: false,
        duration: 0,
        missed: false,
    });
    s
}

#[test]
fn aura_hit_without_an_insert_leaves_the_incumbent_unready() {
    let mut owner = HitOwner::default();
    let t = SpellTemplate {
        id: 90,
        effect1: 6,
        effectapplyauraname1: 95,
        ..Default::default()
    };
    let old = owner
        .auras
        .store_aura_holder(t.id, &mut Some(authored_effect_holder(&t, unit(1), 95)))
        .unwrap();
    hit(&t, 0)
        .hit_self_auras(&t, &SourceSpellStores::default(), 1, 0, None, &mut owner)
        .unwrap();
    assert_eq!(owner.applied, None);
    assert!(!owner.auras.aura_holder(old).unwrap().ready);
}

#[test]
fn aura_hit_marks_only_its_inserted_holder_ready_outside_procs() {
    let mut stores = SourceSpellStores::default();
    stores.bind_empty_duration_fixture();
    let t = SpellTemplate {
        id: 90,
        effect1: 6,
        effectapplyauraname1: 95,
        ..Default::default()
    };
    for proccing in [false, true] {
        let mut owner = HitOwner::default();
        owner.auras.spell_procs_happening = proccing;
        let result = hit(&t, 1).hit_self_auras(&t, &stores, 1, 0, None, &mut owner);
        if proccing {
            assert_eq!(
                result.unwrap_err().id,
                "execution.spells.Spell_1356.AddDelayedHolderDueToProc_owner"
            );
        } else {
            result.unwrap();
        }
        let own = owner.auras.aura_holder(owner.applied.unwrap()).unwrap();
        let other = owner.auras.aura_holder(owner.reentrant.unwrap()).unwrap();
        assert_eq!(own.ready, !proccing);
        assert!(!other.ready);
        assert_eq!(own.apply_time, 17);
        assert_eq!(own.effects[0].as_ref().unwrap().apply_time, 20);
    }
}
