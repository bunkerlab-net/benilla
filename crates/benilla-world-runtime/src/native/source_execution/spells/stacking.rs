// SPDX-License-Identifier: GPL-3.0-or-later
//! SpellStacking.cpp:371-387. Attribute exits precede the aura-effect stacker.
use super::*;
pub fn spell_stackable(
    first: Option<&SpellTemplate>,
    second: Option<&SpellTemplate>,
) -> Result<bool> {
    let (Some(first), Some(second)) = (first, second) else {
        return Ok(true);
    };
    if std::ptr::eq(first, second) && first.attributesex & 0x800 != 0 {
        return Ok(false);
    }
    if (first.attributes | second.attributes) & 0x80000000 != 0 {
        return Ok(true);
    }
    if (first.attributes ^ second.attributes) & 0x40 != 0 {
        return Ok(true);
    }
    Err(refuse(
        "execution.spells.SpellStacking_387.IsStackableSpell_owner",
    ))
}
#[test]
fn stacker_attribute_exits_precede_unowned_effect_rules() {
    let mut t = SpellTemplate::default();
    assert!(spell_stackable(None, Some(&t)).unwrap());
    assert!(spell_stackable(Some(&t), None).unwrap());
    assert_eq!(
        spell_stackable(Some(&t), Some(&t)).unwrap_err().id,
        "execution.spells.SpellStacking_387.IsStackableSpell_owner"
    );
    t.attributes = 0x80000000;
    assert!(spell_stackable(Some(&t), Some(&t)).unwrap());
    t.attributesex = 0x800;
    assert!(!spell_stackable(Some(&t), Some(&t)).unwrap());
    let passive = SpellTemplate {
        attributes: 0x40,
        ..SpellTemplate::default()
    };
    t.attributes = 0;
    t.attributesex = 0;
    assert!(spell_stackable(Some(&t), Some(&passive)).unwrap());
}
