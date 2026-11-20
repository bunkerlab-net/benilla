// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use benilla_sim::creature_init::SourceLevelStats;

const INITIAL: [[u32; 2]; 3] = [
    [0x4000_0000, 0x4040_0000],
    [0x3f00_0000; 2],
    [0x4040_0000, 0x4080_0000],
];
const REDUCED: [[u32; 2]; 3] = [
    [0x3fe6_6666, 0x402c_cccc],
    [0x3ee6_6666; 2],
    [0x402c_cccc, 0x4066_6666],
];
const FIELDS: [usize; 6] = [0x86, 0x87, 0x88, 0x89, 0xab, 0xac];

struct Target {
    objects: SourceObjects,
    object: ObjectHandle,
    live: SourceUnitFields,
    map: MapIdentity,
}

impl Target {
    fn new() -> Self {
        let mut objects = SourceObjects::default();
        let object = objects.construct(6, SourceObjectKind::Creature).unwrap();
        objects
            .assign_guid(
                object,
                41,
                SourceGuid::with_entry(HighGuid::Creature, 901, 41),
            )
            .unwrap();
        let map = MapIdentity {
            map: 0,
            instance: 0,
            creation_generation: 1,
        };
        objects
            .bind_map(object, map, &SourceCells::new(map).unwrap())
            .unwrap();
        let mut live = SourceUnitFields::constructed_creature();
        live.apply_source_level(
            &objects,
            object,
            &SourceLevelStats {
                level: 19,
                create_attributes_bits: [0; 5],
                attributes: [0; 5],
                create_health: 1,
                create_mana: 0,
                maximum_health: 1,
                health: 1,
                maximum_power: [0; 5],
                power: [0; 5],
                armor_base_bits: 0,
                armor: 0,
                attack_power: [14, 28],
                base_attack_power_bits: [14.0f32.to_bits(), 28.0f32.to_bits()],
                weapon_damage_bits: [[1.0f32.to_bits(), 2.0f32.to_bits()]; 3],
                damage_multiplier_bits: 1.0f32.to_bits(),
            },
            1,
        )
        .unwrap();
        live.loaded_creature.can_modify_stats = true;
        objects.add_to_world(object).unwrap();
        Self {
            objects,
            object,
            live,
            map,
        }
    }

    fn apply(
        &mut self,
        stores: &SourceSpellStores,
        spell: &SpellTemplate,
        amount: i32,
    ) -> SourceAuraHolderId {
        let mut holder = SourceAuraHolder::new(spell, self.object, 0, 0, -1).unwrap();
        holder.effects[0] =
            Some(SourceAura::self_spell_effect(spell, 0, 79, 0, amount, 1, 0).unwrap());
        let id = self
            .live
            .auras
            .store_aura_holder(spell.id, &mut Some(holder))
            .unwrap();
        apply_self_aura_holder(self, stores, spell, id, None).unwrap();
        id
    }

    fn remove(
        &mut self,
        stores: &SourceSpellStores,
        id: SourceAuraHolderId,
        mode: AuraRemoveMode,
    ) -> Result<()> {
        self.live.auras.remove_self_aura_holder(
            stores,
            id,
            mode,
            &mut SelfAuraRemoval {
                unit: operands(&self.objects, self.object, &self.live.control_guids),
                visibility: &mut self.live.visibility,
                damage: DamageAuraTarget::new(
                    &mut self.live.creature_damage,
                    &self.live.loaded_creature,
                    0,
                    0,
                ),
            },
        )
    }

    fn expect(&self, bits: [[u32; 2]; 3], marks: bool) {
        assert_eq!(self.live.creature_damage.damage_bits(), bits);
        assert_eq!(
            self.objects.client_update_pending(self.map, self.object),
            marks
        );
        for field in FIELDS {
            assert_eq!(
                self.objects
                    .get(self.object)
                    .unwrap()
                    .field_changed(field)
                    .unwrap(),
                marks
            );
        }
    }
}

fn operands<'a>(
    objects: &'a SourceObjects,
    target: ObjectHandle,
    control_guids: &'a SourceControlGuids,
) -> SelfAuraTarget<'a> {
    SelfAuraTarget {
        target,
        level: Some(19),
        alive: true,
        player: false,
        player_group: None,
        totem: false,
        objects,
        control_guids,
    }
}

impl SpellExecutionOwner for Target {}

impl SelfAuraApplicationOwner for Target {
    fn self_aura_application(&mut self) -> SelfAuraApplication<'_> {
        let live = &mut self.live;
        SelfAuraApplication {
            unit: operands(&self.objects, self.object, &live.control_guids),
            auras: &mut live.auras,
            visibility: &mut live.visibility,
            resistances: &mut live.resistances,
            damage: DamageAuraTarget::new(
                &mut live.creature_damage,
                &live.loaded_creature,
                live.flags,
                live.shapeshift_form,
            ),
            can_modify_stats: live.loaded_creature.can_modify_stats,
            threat_modifier: &mut live.threat_modifier,
            power_fields: &mut live.power_fields,
            power_type: live.power_type.as_mut(),
            vitals: &mut live.vitals,
            form: &mut live.shapeshift_form,
        }
    }
}

fn spell(stores: &mut SourceSpellStores, id: u32, mask: i32, item_mask: i32) -> SpellTemplate {
    let spell = SpellTemplate {
        id,
        effect1: 6,
        effectapplyauraname1: 79,
        effectmiscvalue1: mask,
        equippeditemclass: -1,
        equippediteminventorytypemask: item_mask,
        ..Default::default()
    };
    stores.insert_template(spell.clone()).unwrap();
    spell
}

#[test]
fn physical_damage_aura_apply_and_unapply_preserve_source_float_steps() {
    let mut stores = SourceSpellStores::default();
    let physical = spell(&mut stores, 7376, 127, 0);
    for (amount, applied, removed) in [
        (-10, REDUCED, INITIAL),
        // Unit.cpp:9000-9001: 0x3f570a3d * 0x3f986186 rounds to 0x3f7fffff.
        (
            -16,
            [
                [0x3fd7_0a3d, 0x4021_47ae],
                [0x3ed7_0a3d; 2],
                [0x4021_47ae, 0x4057_0a3d],
            ],
            [
                [0x3fff_ffff, 0x403f_ffff],
                [0x3eff_ffff; 2],
                [0x403f_ffff, 0x407f_ffff],
            ],
        ),
        // Unit.cpp:8997,9057: raw factor -1 reads back as zero until unapply.
        (-100, [[0; 2]; 3], INITIAL),
        (-120, [[0; 2]; 3], INITIAL),
    ] {
        let mut subject = Target::new();
        let saved = serde_json::to_vec(&subject.live).unwrap();
        subject.expect(INITIAL, false);
        let id = subject.apply(&stores, &physical, amount);
        subject.expect(applied, true);
        assert_eq!(serde_json::to_vec(&subject.live).unwrap(), saved);
        subject
            .remove(&stores, id, AuraRemoveMode::Default)
            .unwrap();
        subject.expect(removed, true);
        assert_eq!(serde_json::to_vec(&subject.live).unwrap(), saved);
    }
}

#[test]
fn physical_damage_stat_gate_preserves_the_modifier_write() {
    let mut stores = SourceSpellStores::default();
    let physical = spell(&mut stores, 7376, 1, 0);
    let zero = spell(&mut stores, 7377, 1, 0);
    let mut subject = Target::new();
    subject.live.loaded_creature.can_modify_stats = false;
    let id = subject.apply(&stores, &physical, -10);
    subject.expect(INITIAL, false);
    subject.live.loaded_creature.can_modify_stats = true;
    subject.apply(&stores, &zero, 0);
    subject.expect(REDUCED, true);
    subject
        .remove(&stores, id, AuraRemoveMode::Default)
        .unwrap();
    subject.expect(INITIAL, true);
}

#[test]
fn creature_magic_damage_aura_obeys_expire_and_item_mask_order() {
    for (mask, items, mode, refusal) in [
        (0, 0, AuraRemoveMode::Expire, None),
        (1, 0, AuraRemoveMode::Expire, None),
        (2, 0, AuraRemoveMode::Default, None),
        (
            2,
            0,
            AuraRemoveMode::Expire,
            Some("execution.spells.Aura_4177.CastSpell_38639_owner"),
        ),
        (2, 1, AuraRemoveMode::Expire, None),
    ] {
        let mut stores = SourceSpellStores::default();
        let t = spell(&mut stores, 30423, mask, items);
        let mut subject = Target::new();
        let id = subject.apply(&stores, &t, -10);
        subject.expect(if mask == 1 { REDUCED } else { INITIAL }, mask == 1);
        assert_eq!(
            subject.remove(&stores, id, mode).err().map(|err| err.id),
            refusal.map(str::to_owned)
        );
        subject.expect(INITIAL, mask == 1);
    }
}
