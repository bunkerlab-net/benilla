// SPDX-License-Identifier: GPL-3.0-or-later
//! ScriptMgr.cpp:2173-2200,2980,2990.
use super::*;
impl SpellStartContext<'_> {
    pub fn db_cast(
        &mut self,
        objects: &SourceObjects,
        source: ObjectHandle,
        target: Option<ObjectHandle>,
        spell: u32,
        flags: u32,
        base: Option<[i32; 3]>,
    ) -> Result<()> {
        let go = matches!(
            objects.get(source)?.kind,
            SourceObjectKind::GameObject | SourceObjectKind::Transport
        );
        let caster = if go {
            target.ok_or_else(|| refuse("execution.spells.DB_GO_unit_target_owner"))?
        } else {
            source
        };
        let Some(t) = self.stores.template(spell) else {
            return if base.is_some() {
                Ok(())
            } else {
                Err(refuse("execution.spells.DB_script_null_template_undefined"))
            };
        };
        // Unit.cpp:1492,1553 constructs before target capture. ScriptMgr.cpp:
        // 2185-2200 builds ordinary DB targets before calling Unit::CastSpell.
        let early = if go || base.is_some() {
            Some(self.db_construct(source, caster, spell, flags, base)?)
        } else {
            None
        };
        let mut targets = SpellTargets::default();
        if let Some(target) = target {
            let o = objects.get(target)?;
            if matches!(
                o.kind,
                SourceObjectKind::Player | SourceObjectKind::Creature | SourceObjectKind::Pet
            ) {
                targets.unit = Some(target);
                targets.mask |= 2;
            } else if matches!(
                o.kind,
                SourceObjectKind::GameObject | SourceObjectKind::Transport
            ) {
                targets.gameobject = Some(target);
                targets.mask |= 0x800;
            }
            if targets.unit.is_some() || t.targets & 0x40 != 0 {
                let pose = o
                    .pose
                    .as_ref()
                    .ok_or_else(|| refuse("execution.spells.WorldObject_GetPosition_owner"))?;
                targets.destination = Some(pose.0[..3].try_into().expect("three coordinates"));
            }
            if t.targets & 0x40 != 0 {
                targets.mask |= 0x40;
            }
        }
        if t.targets & 0x20 != 0 {
            let pose = objects
                .get(caster)?
                .pose
                .as_ref()
                .ok_or_else(|| refuse("execution.spells.WorldObject_GetPosition_owner"))?;
            targets.source = Some(pose.0[..3].try_into().expect("three coordinates"));
            targets.mask |= 0x20;
        }
        let mut value = match early {
            Some(value) => value,
            None => self.db_construct(source, caster, spell, flags, base)?,
        };
        let id = value.id;
        let result = value.start(t, targets, None, self.events, self.owner);
        self.starts.spells.insert(id, value);
        result.map(|_| ())
    }
    fn db_construct(
        &mut self,
        source: ObjectHandle,
        caster: ObjectHandle,
        spell: u32,
        flags: u32,
        base: Option<[i32; 3]>,
    ) -> Result<SourceSpell> {
        let id = self.starts.allocate()?;
        let flags = if source != caster {
            0x41
        } else if base.is_some() {
            flags
        } else {
            flags | 0x40
        };
        let mut value = SourceSpell::construct(
            id,
            self.stores,
            spell,
            caster,
            Some(source),
            SpellTriggerFlags(flags),
            self.owner,
        )?;
        if let Some(base) = base {
            value.base_points = base;
        }
        Ok(value)
    }
}
