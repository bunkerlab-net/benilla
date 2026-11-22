// SPDX-License-Identifier: GPL-3.0-or-later
//! SpellMgr.cpp:389-431; DBCStores.cpp:616-635. Definition checks with live form.
use super::*;
impl SourceSpellStores {
    pub fn shapeshift_error(&self, t: &SpellTemplate, form: u32) -> Result<SpellCastResult> {
        use SpellCastResult as R;
        // DBCStores.cpp:625-635 returns rank+1 for every Talent rank membership.
        if self.is_talent(t.id) && t.effects().iter().any(|e| e.effect == 36) {
            return Ok(R::Ok);
        }
        require(
            form <= 32,
            "execution.spells.SpellMgr_397.form_shift_undefined",
        )?;
        let mask = if form != 0 { 1u32 << (form - 1) } else { 0 };
        if mask & t.stancesnot != 0 {
            return Ok(R::NotShapeshift);
        }
        if mask & t.stances != 0 {
            return Ok(R::Ok);
        }
        let shifted = if form > 0 {
            let Some(row) = self.dbc("SpellShapeshiftForm.dbc", form)? else {
                return Ok(R::Ok);
            };
            row.words
                .get(&11)
                .copied()
                .ok_or_else(|| refuse("execution.spells.shapeshift_dbc_layout"))?
                & 1
                == 0
        } else {
            false
        };
        if shifted {
            if t.attributes & 0x10000 != 0 {
                return Ok(R::NotShapeshift);
            }
            if t.stances != 0 {
                return Ok(R::OnlyShapeshift);
            }
        } else if t.attributesex2 & 0x80000 == 0 && t.stances != 0 {
            return Ok(R::OnlyShapeshift);
        }
        Ok(R::Ok)
    }
}
#[test]
fn form_stance_boundaries_do_not_read_unused_dbc() {
    let s = SourceSpellStores::default();
    let mut t = SpellTemplate::default();
    assert_eq!(s.shapeshift_error(&t, 0).unwrap(), SpellCastResult::Ok);
    t.stances = 1;
    assert_eq!(
        s.shapeshift_error(&t, 0).unwrap(),
        SpellCastResult::OnlyShapeshift
    );
    assert_eq!(s.shapeshift_error(&t, 1).unwrap(), SpellCastResult::Ok);
    t.stancesnot = 1;
    assert_eq!(
        s.shapeshift_error(&t, 1).unwrap(),
        SpellCastResult::NotShapeshift
    );
    t.stances = 0x80000000;
    t.stancesnot = 0;
    assert_eq!(s.shapeshift_error(&t, 32).unwrap(), SpellCastResult::Ok);
    assert_eq!(
        s.shapeshift_error(&t, 33).unwrap_err().id,
        "execution.spells.SpellMgr_397.form_shift_undefined"
    );
}
