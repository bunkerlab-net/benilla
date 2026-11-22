// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

fn fixture() -> (SourceSpellStores, SourceAura) {
    let mut stores = SourceSpellStores::default();
    stores.bind_test_dbc(
        "SpellRadius.dbc",
        [(
            10,
            SpellDbcRecord {
                ordinal: 0,
                words: BTreeMap::from([(1, 30.0f32.to_bits())]),
            },
        )],
    );
    let t = SpellTemplate {
        effect1: 35,
        effectimplicittargeta1: 1,
        ..Default::default()
    };
    (
        stores,
        SourceAura::self_spell_effect(&t, 0, 22, 505, 505, 1, 42).unwrap(),
    )
}

#[test]
fn authored_area_constructor_binds_party_and_pet_and_suppresses_totem_effect() {
    let (stores, aura) = fixture();
    for (effect, kind, totem, radius_index, radius_bits, aura_kind) in [
        (
            35,
            SourceAreaAuraKind::Party,
            false,
            10,
            30.0f32.to_bits(),
            22,
        ),
        (119, SourceAreaAuraKind::Pet, true, 11, 0, 0),
    ] {
        let mut aura = aura.clone();
        assert_eq!(SourceAreaAuraKind::for_effect(effect), Some(kind));
        SourceAreaAura::attach(&mut aura, kind, &stores, radius_index, || Ok(None), totem).unwrap();
        let area = aura.area.unwrap();
        assert_eq!(
            (area.kind, area.radius_bits, area.original_rank_spell),
            (kind, radius_bits, None)
        );
        assert_eq!(
            (aura.kind, aura.amount, aura.apply_time),
            (aura_kind, 505, 42)
        );
        assert_eq!(
            area.update(1).unwrap_err().id,
            "execution.spells.AreaAura_410.Update_owner"
        );
    }
}

#[test]
fn authored_area_radius_layout_and_spell_mod_owner_stop_in_source_order() {
    let (mut stores, mut aura) = fixture();
    let caster = ObjectHandle {
        thread: 7,
        construction: 1,
    };
    assert_eq!(
        SourceAreaAura::attach(
            &mut aura,
            SourceAreaAuraKind::Party,
            &stores,
            10,
            || Ok(Some(caster)),
            false
        )
        .unwrap_err()
        .id,
        "execution.spells.AreaAura_309.ApplySpellMod_radius_owner"
    );
    stores = SourceSpellStores::default();
    stores.bind_test_dbc(
        "SpellRadius.dbc",
        [(
            10,
            SpellDbcRecord {
                ordinal: 0,
                words: BTreeMap::new(),
            },
        )],
    );
    assert_eq!(
        SourceAreaAura::attach(
            &mut aura,
            SourceAreaAuraKind::Party,
            &stores,
            10,
            || panic!("radius fails before mod owner"),
            false
        )
        .unwrap_err()
        .id,
        "execution.spells.SpellRadius_dbc_column_owner"
    );
}
