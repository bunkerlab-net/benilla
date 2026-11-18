// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

fn unit(construction: u64) -> ObjectHandle {
    ObjectHandle {
        thread: 6,
        construction,
    }
}

fn removal<'a>(
    objects: &'a SourceObjects,
    controls: &'a SourceControlGuids,
) -> SelfAuraRemoval<'a> {
    SelfAuraRemoval {
        target: unit(1),
        level: Some(19),
        alive: true,
        player: false,
        player_group: None,
        totem: false,
        objects,
        control_guids: controls,
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
    let objects = SourceObjects::default();
    let controls = SourceControlGuids::default();
    hit(&t, 0)
        .hit_self_auras(
            &t,
            &SourceSpellStores::default(),
            removal(&objects, &controls),
            0,
            None,
            &mut owner,
        )
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
        let objects = SourceObjects::default();
        let controls = SourceControlGuids::default();
        let result = hit(&t, 1).hit_self_auras(
            &t,
            &stores,
            removal(&objects, &controls),
            0,
            None,
            &mut owner,
        );
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

#[test]
fn aura_removal_unlinks_by_identity_and_defers_in_effect_order() {
    let t = SpellTemplate {
        id: 10848,
        procflags: 1,
        ..Default::default()
    };
    let mut stores = SourceSpellStores::default();
    stores.insert_template(t.clone()).unwrap();
    let objects = SourceObjects::default();
    let controls = SourceControlGuids::default();
    let live = removal(&objects, &controls);
    let mut auras = SourceUnitAuras::default();
    let mut old = authored_effect_holder(&t, live.target, 95);
    old.effects[2] = authored_effect_holder(&t, live.target, 4).effects[0].clone();
    old.effects
        .iter_mut()
        .flatten()
        .for_each(|aura| aura.applied = true);
    old.slot = 1;
    old.charges = 3;
    old.stack = 2;
    auras.aura_fields.spells[1] = t.id;
    auras.aura_fields.flags[0] = 0xabcd_ef51;
    auras.aura_fields.levels[0] = 0x3412_0956;
    auras.aura_fields.applications[0] = 0x3412_0556;
    auras.vis_flags = 3;
    auras.heartbeat_proc_counter = 2;
    let old = auras.store_aura_holder(t.id, &mut Some(old)).unwrap();
    let other = auras
        .store_aura_holder(t.id, &mut Some(authored_effect_holder(&t, live.target, 4)))
        .unwrap();
    auras.aura_update_next = Some(old);
    auras
        .remove_self_aura_holder(&stores, old, AuraRemoveMode::Stack, live)
        .unwrap();
    assert_eq!(auras.aura_update_next, Some(other));
    assert!(auras.aura_holder(old).is_none());
    assert_eq!(
        auras.aura_holders().map(|(id, _)| id).collect::<Vec<_>>(),
        [other]
    );
    assert!(!auras.has_aura_type(95));
    assert_eq!(
        auras.auras_of_type(4),
        &[SourceAuraEffectRef {
            holder: other,
            effect: 0
        }]
    );
    assert_eq!(auras.vis_flags, 2);
    assert_eq!(auras.heartbeat_proc_counter, 1);
    assert_eq!(auras.aura_fields.spells[1], 0);
    assert_eq!(auras.aura_fields.flags[0], 0xabcd_ef01);
    assert_eq!(auras.aura_fields.levels[0], 0x3412_1356);
    assert_eq!(auras.aura_fields.applications[0], 0x3412_0056);
    let (deferred_id, deferred) = auras.deferred_aura_holders().next().unwrap();
    assert_eq!(deferred_id, old);
    assert!(deferred.deleted && deferred.effects.iter().all(Option::is_none));
    assert_eq!(
        (deferred.remove_mode, deferred.charges, deferred.stack),
        (1, 0, 1)
    );
    assert_eq!(
        auras
            .deferred_auras()
            .iter()
            .map(|effect| (effect.kind, effect.remove_mode, effect.applied))
            .collect::<Vec<_>>(),
        [(95, 1, false), (4, 1, false)]
    );
    auras.cleanup_deleted_auras();
    assert_eq!(auras.deferred_aura_holders().len(), 0);
    assert!(auras.deferred_auras().is_empty());
    assert!(auras.aura_holder(other).is_some());
}

#[test]
fn cast_removal_restarts_at_the_first_live_holder() {
    let mut stores = SourceSpellStores::default();
    let objects = SourceObjects::default();
    let controls = SourceControlGuids::default();
    let live = removal(&objects, &controls);
    let mut auras = SourceUnitAuras::default();
    let mut ids = Vec::new();
    for (id, flags) in [(1, 0), (2, 4), (3, 4)] {
        let t = SpellTemplate {
            id,
            aurainterruptflags: flags,
            ..Default::default()
        };
        let mut old = authored_effect_holder(&t, live.target, 4);
        old.slot = 255;
        let holder = auras.store_aura_holder(id, &mut Some(old)).unwrap();
        stores.insert_template(t).unwrap();
        ids.push(holder);
    }
    let cast = SpellTemplate {
        id: 4,
        ..Default::default()
    };
    let mut lookups = 0;
    check_cast_aura_removals(&stores, &mut auras, &cast, live, 4, || lookups += 1).unwrap();
    // Visits [1,2], then [1,3], then [1].
    assert_eq!(lookups, 5);
    assert_eq!(
        auras.aura_holders().map(|(id, _)| id).collect::<Vec<_>>(),
        ids[..1]
    );
    assert_eq!(
        auras
            .deferred_aura_holders()
            .map(|(id, _)| id)
            .collect::<Vec<_>>(),
        ids[1..]
    );
}

#[test]
fn aura_removal_refuses_first_unbound_source_operand() {
    let objects = SourceObjects::default();
    let controls = SourceControlGuids::default();
    for (tracked, diminish_group, attributesex, player, mode, site, slot_cleared, deleted) in [
        (
            true,
            0,
            0,
            false,
            AuraRemoveMode::Stack,
            Some("execution.spells.SpellAuraHolder_5915.GetTrackedAuraTargets_owner"),
            false,
            false,
        ),
        (
            false,
            1,
            0,
            false,
            AuraRemoveMode::Stack,
            Some("execution.spells.SpellAuraHolder_5337.ApplyDiminishingAura_owner"),
            false,
            false,
        ),
        (
            false,
            0,
            4,
            false,
            AuraRemoveMode::Stack,
            Some("execution.spells.Unit_5381.GetChannelObjectGuid_owner"),
            true,
            true,
        ),
        (
            false,
            0,
            0,
            true,
            AuraRemoveMode::Stack,
            Some("execution.spells.Unit_10506.GetGroup_owner"),
            true,
            false,
        ),
        (false, 0, 0, true, AuraRemoveMode::Delete, None, true, true),
    ] {
        let t = SpellTemplate {
            id: 10848,
            attributesex,
            ..Default::default()
        };
        let mut stores = SourceSpellStores::default();
        stores.insert_template(t.clone()).unwrap();
        let live = SelfAuraRemoval {
            player,
            ..removal(&objects, &controls)
        };
        let mut auras = SourceUnitAuras::default();
        let mut old = authored_effect_holder(&t, live.target, 95);
        if mode != AuraRemoveMode::Delete && player {
            old.effects = [None, None, None];
        }
        old.tracked = tracked;
        old.diminish_group = diminish_group;
        old.slot = 0;
        auras.vis_flags = 1;
        auras.aura_fields.spells[0] = t.id;
        let old = auras.store_aura_holder(t.id, &mut Some(old)).unwrap();
        assert_eq!(
            auras
                .remove_self_aura_holder(&stores, old, mode, live)
                .err()
                .map(|err| err.id),
            site.map(str::to_owned)
        );
        assert!(auras.aura_holder(old).is_none());
        assert_eq!(auras.aura_fields.spells[0] == 0, slot_cleared);
        assert_eq!(
            auras.deferred_aura_holders().next().unwrap().1.deleted,
            deleted
        );
        if mode == AuraRemoveMode::Delete {
            assert_eq!(auras.vis_flags, 1);
        }
    }
}

#[test]
fn replacement_slot_refuses_player_duration_packet_after_slot_write() {
    let t = SpellTemplate {
        id: 10848,
        effect1: 6,
        effectapplyauraname1: 4,
        ..Default::default()
    };
    let mut stores = SourceSpellStores::default();
    stores.bind_empty_duration_fixture();
    let mut owner = HitOwner::default();
    let objects = SourceObjects::default();
    let controls = SourceControlGuids::default();
    let live = SelfAuraRemoval {
        player: true,
        player_group: Some(false),
        ..removal(&objects, &controls)
    };
    let mut spell = hit(&t, 1);
    assert_eq!(
        spell
            .hit_self_auras(&t, &stores, live, 0, None, &mut owner)
            .unwrap_err()
            .id,
        "execution.spells.SpellAuraHolder_5986.aura_duration_session_owner"
    );
    assert_eq!(owner.auras.aura_fields.spells[0], t.id);
    assert!(!owner.auras.has_aura(t.id));
    assert_eq!(owner.applied, None);
}

#[test]
fn aura_removal_refuses_the_missing_boost_template_dereference() {
    let t = SpellTemplate {
        id: 20594,
        ..Default::default()
    };
    let mut stores = SourceSpellStores::default();
    stores.insert_template(t.clone()).unwrap();
    let objects = SourceObjects::default();
    let controls = SourceControlGuids::default();
    let live = removal(&objects, &controls);
    let mut auras = SourceUnitAuras::default();
    let mut holder = Some(authored_effect_holder(&t, live.target, 95));
    let id = auras.store_aura_holder(t.id, &mut holder).unwrap();
    assert_eq!(
        auras
            .remove_self_aura_holder(&stores, id, AuraRemoveMode::Stack, live)
            .unwrap_err()
            .id,
        "execution.spells.SpellAuraHolder_5633.boost_template_null_undefined"
    );
    assert!(auras.aura_holder(id).is_none());
    assert_eq!(auras.deferred_auras().len(), 1);
    assert!(!auras.deferred_aura_holders().next().unwrap().1.deleted);
}
