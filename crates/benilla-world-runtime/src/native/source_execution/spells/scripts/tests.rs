// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::native::production_compiler::VerifiedCatalog;
use std::collections::BTreeSet;
use std::path::Path;

const NAME: &[u8] = b"spell_detect_through_invisibility_mob";

#[test]
fn production_registry_checks_each_base_independently() {
    let mut owner = SourceSpellScriptRegistry;
    assert_eq!(owner.script(SpellScriptKind::Spell, NAME).unwrap(), None);
    let binding = owner.script(SpellScriptKind::Aura, NAME).unwrap().unwrap();
    assert_eq!(binding.name.as_ref(), NAME);
    assert_eq!(binding.registration, 1);
    assert_eq!(
        binding.script,
        Some(SourceSpellScript::DetectThroughInvisibilityMob)
    );
    for kind in [SpellScriptKind::Spell, SpellScriptKind::Aura] {
        assert_eq!(
            owner.script(kind, b"unported_test_name").unwrap_err().id,
            format!("{}.unported_test_name", kind.registry_owner())
        );
    }
}

#[test]
fn stock_spell_script_loader_census() {
    let Some(root) = std::env::var_os("BENILLA_F5_AI_CATALOG") else {
        println!("SKIP stock spell script loader census: set BENILLA_F5_AI_CATALOG");
        return;
    };
    let mut catalog = VerifiedCatalog::open(Path::new(&root), false).unwrap();
    let stores = SourceSpellStores::load(&mut catalog, &mut SourceSpellScriptRegistry).unwrap();
    let rows = &stores.loads()["spell_scripts"];
    let mut visited = 0;
    let mut names = BTreeSet::new();
    let mut typed = 0;
    let mut refused = 0;
    catalog
        .execution_spell_script_scan(|ordinal, id, name| {
            let row = &rows[visited];
            visited += 1;
            assert_eq!((row.ordinal, row.key), (ordinal, id));
            names.insert(name.to_vec());
            match &row.disposition {
                SpellRowDisposition::Loaded => {
                    assert_eq!((id, name), (16380, NAME)); // initial-classicmangos.sql:836881.
                    assert!(stores
                        .script_binding(SpellScriptKind::Spell, id)
                        .unwrap()
                        .is_none());
                    let binding = stores
                        .script_binding(SpellScriptKind::Aura, id)
                        .unwrap()
                        .unwrap();
                    assert_eq!(binding.name.as_ref(), name);
                    assert_eq!(
                        binding.script,
                        Some(SourceSpellScript::DetectThroughInvisibilityMob)
                    );
                    typed += 1;
                }
                SpellRowDisposition::Refused(error) => {
                    assert_eq!(
                        error.id,
                        format!(
                            "execution.spells.spell_script_registry_owner.{}",
                            std::str::from_utf8(name).unwrap()
                        )
                    );
                    refused += 1;
                }
                SpellRowDisposition::Skipped(reason) => panic!("unexpected script skip: {reason}"),
            }
            Ok(())
        })
        .unwrap();
    // initial-classicmangos.sql:836694-837271; trace :60094985.
    assert_eq!(
        (rows.len(), visited, names.len(), typed, refused),
        (578, 578, 241, 1, 577)
    );
}

struct UntypedBinding(SpellScriptKind);
impl SpellLoaderOwner for UntypedBinding {
    fn script(&mut self, kind: SpellScriptKind, name: &[u8]) -> Result<Option<SpellScriptBinding>> {
        Ok((kind == self.0).then(|| SpellScriptBinding {
            name: name.into(),
            registration: 7,
            script: None,
        }))
    }
}

fn untyped_fixture(kind: SpellScriptKind) -> (SourceSpellStores, SpellTemplate, ObjectHandle) {
    let mut stores = SourceSpellStores::default();
    let t = SpellTemplate {
        id: 16380,
        ..Default::default()
    };
    stores.insert_template(t.clone()).unwrap();
    assert_eq!(
        stores
            .bind_script(t.id, NAME, &mut UntypedBinding(kind))
            .unwrap(),
        None
    );
    (
        stores,
        t,
        ObjectHandle {
            thread: 6,
            construction: 1,
        },
    )
}

#[test]
fn spell_constructor_refuses_a_binding_without_a_ported_kind() {
    struct Unbound;
    impl SpellExecutionOwner for Unbound {}
    for kind in [SpellScriptKind::Spell, SpellScriptKind::Aura] {
        let (stores, t, caster) = untyped_fixture(kind);
        let error = SourceSpell::construct(
            1,
            &stores,
            t.id,
            caster,
            None,
            SpellTriggerFlags(0),
            &mut Unbound,
        )
        .unwrap_err();
        assert_eq!(error.id, "execution.spells.Spell_388.script_binding_owner");
    }
}

#[test]
fn aura_holder_constructor_refuses_an_untyped_binding_before_its_clock() {
    let (stores, t, target) = untyped_fixture(SpellScriptKind::Aura);
    assert_eq!(
        SourceAuraHolder::for_self_spell(&stores, &t, target, None, || panic!(
            "holder clock was read"
        ))
        .unwrap_err()
        .id,
        "execution.spells.SpellAuras_5164.script_binding_owner"
    );
}
