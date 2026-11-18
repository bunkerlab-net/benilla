// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! CreatureSettings.cpp:25-49; Unit.cpp:10682-10710.
use super::*;
use std::collections::BTreeSet;
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SourceCreatureSettings {
    pub flags: [u32; 4],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreatureStaticFlagApplication {
    Visibility(SourceVisibilityDistance),
    UnitFlag(u32),
    ImmuneToPlayer,
    ImmuneToNpc,
}
impl CreatureStaticFlagApplication {
    pub(super) fn unit_flag(self) -> u32 {
        match self {
            Self::Visibility(_) => 0,
            Self::UnitFlag(mask) => mask,
            Self::ImmuneToPlayer => 0x100,
            Self::ImmuneToNpc => 0x200,
        }
    }
}
impl SourceCreatureSettings {
    pub fn reset(
        &mut self,
        flags: [u32; 4],
        mut apply: impl FnMut(CreatureStaticFlagApplication) -> Result<()>,
    ) -> Result<()> {
        self.flags = flags;
        for effect in Self::effects(flags) {
            apply(effect)?;
        }
        Ok(())
    }

    /// Creature.cpp:631, CreatureSettings.cpp:31-49. Keep writes on refusal.
    pub fn reset_live(
        live: &mut SourceUnitFields,
        flags: [u32; 4],
        object: ObjectHandle,
        objects: &mut SourceObjects,
    ) -> Result<()> {
        live.static_flags = flags;
        for effect in Self::effects(flags) {
            match effect {
                CreatureStaticFlagApplication::Visibility(distance) => {
                    objects.set_visibility_distance_override(object, distance)?;
                }
                CreatureStaticFlagApplication::UnitFlag(mask) => live.flags |= mask,
                CreatureStaticFlagApplication::ImmuneToPlayer
                | CreatureStaticFlagApplication::ImmuneToNpc => {
                    live.controlled.set_immune(
                        effect.unit_flag(),
                        &mut live.flags,
                        objects.get(object)?.in_world,
                        live.control_guids,
                        |kind, guid| lookup_controlled(objects, object, kind, guid),
                        |_, mask| {
                            Err(refuse(if mask == 0x100 {
                                "execution.creature_settings.Unit_10704.controlled_flags_owner"
                            } else {
                                "execution.creature_settings.Unit_10685.controlled_flags_owner"
                            }))
                        },
                    )?;
                }
            }
        }
        Ok(())
    }

    /// CreatureSettings.cpp:31-49, including ordered visibility precedence.
    pub(super) fn effects(flags: [u32; 4]) -> impl Iterator<Item = CreatureStaticFlagApplication> {
        use CreatureStaticFlagApplication::*;
        [
            (
                flags[0] & 0x8000_0000,
                Visibility(SourceVisibilityDistance::Large),
            ),
            (
                flags[2] & 0x400,
                Visibility(SourceVisibilityDistance::Gigantic),
            ),
            (
                flags[2] & 0x800,
                Visibility(SourceVisibilityDistance::Infinite),
            ),
            (flags[0] & 0x200, UnitFlag(0x0200_0000)),
            (flags[0] & 0x20, ImmuneToPlayer),
            (flags[0] & 0x40, ImmuneToNpc),
        ]
        .into_iter()
        .filter_map(|(set, effect)| (set != 0).then_some(effect))
    }
}
/// Unit.h:1872,2674-2706. Live GUID fields and collections, with no cached handles.
#[derive(Clone, Debug, Default)]
pub struct SourceControlledUnits {
    pub pet: SourceGuid,
    pub guardians: BTreeSet<SourceGuid>,
    pub totems: [SourceGuid; 4],
    pub charm: SourceGuid,
    pub private_charms: BTreeSet<SourceGuid>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ControlledLookup {
    Pet,
    Guardian,
    Totem,
    Charm,
    PrivateCharm,
}
impl SourceControlledUnits {
    fn set_immune(
        &mut self,
        mask: u32,
        flags: &mut u32,
        in_world: bool,
        control_guids: SourceControlGuids,
        mut lookup: impl FnMut(ControlledLookup, SourceGuid) -> Result<Option<ObjectHandle>>,
        mut child: impl FnMut(ObjectHandle, u32) -> Result<()>,
    ) -> Result<()> {
        *flags |= mask;
        if control_guids.has_charmer() {
            return Ok(());
        }
        // Unit.h:2674-2706 excludes mini-pets; actual map lookups stay at their source positions.
        if self.pet.raw() != 0 {
            if let Some(pet) = lookup(ControlledLookup::Pet, self.pet)? {
                child(pet, mask)?;
            } else {
                self.pet = SourceGuid::default(); // Unit.cpp:6763, Unit.h:1872.
            }
        }
        for &guid in &self.guardians {
            if let Some(guardian) = lookup(ControlledLookup::Guardian, guid)? {
                child(guardian, mask)?;
            }
        }
        if in_world {
            for guid in self.totems {
                if guid.raw() != 0 {
                    if let Some(totem) = lookup(ControlledLookup::Totem, guid)? {
                        child(totem, mask)?;
                    }
                }
            }
            // This owner is an ordinary creature, never GetCharm's loading-Pet exception, :6559-6562.
            if self.charm.raw() != 0 {
                if let Some(charm) = lookup(ControlledLookup::Charm, self.charm)? {
                    child(charm, mask)?;
                }
            }
        }
        for &guid in &self.private_charms {
            if let Some(charm) = lookup(ControlledLookup::PrivateCharm, guid)? {
                child(charm, mask)?;
            }
        }
        Ok(())
    }
}

/// Unit.cpp:6555-6562,6750-6763,6850-6854. Look up each GUID in the calling map.
fn lookup_controlled(
    objects: &SourceObjects,
    object: ObjectHandle,
    kind: ControlledLookup,
    guid: SourceGuid,
) -> Result<Option<ObjectHandle>> {
    let Some(map) = objects.get(object)?.map else {
        return Ok(None);
    };
    let found = match kind {
        ControlledLookup::Pet | ControlledLookup::Guardian => objects
            .lookup_world_object(map, guid)
            .filter(|&unit| objects.get(unit).unwrap().kind == SourceObjectKind::Pet),
        _ => objects.lookup_unit(map, guid),
    };
    if kind == ControlledLookup::Totem && found.is_some() {
        return Err(refuse(
            "execution.creature_settings.Unit_6854.totem_fields_owner",
        ));
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn static_settings_assign_all_fields_before_ordered_effects_and_preserve_failure_prefix() {
        let mut settings = SourceCreatureSettings::default();
        let flags = [0x8000_0260, u32::MAX, 0xc00, 7];
        let mut applied = Vec::new();
        settings
            .reset(flags, |effect| {
                applied.push(effect);
                Ok(())
            })
            .unwrap();
        use CreatureStaticFlagApplication::*;
        assert_eq!(settings.flags, flags);
        assert_eq!(
            applied,
            [
                Visibility(SourceVisibilityDistance::Large),
                Visibility(SourceVisibilityDistance::Gigantic),
                Visibility(SourceVisibilityDistance::Infinite),
                UnitFlag(0x0200_0000),
                ImmuneToPlayer,
                ImmuneToNpc
            ]
        );
        let failure = settings
            .reset([0x60, 2, 3, 4], |effect| {
                assert_eq!(effect, ImmuneToPlayer);
                Err(refuse("authored.immune_player"))
            })
            .unwrap_err();
        assert_eq!(failure.id, "authored.immune_player");
        assert_eq!(settings.flags, [0x60, 2, 3, 4]);
        settings
            .reset([0; 4], |_| {
                panic!("unset flags have no effects and do not clear previous Unit flags/overrides")
            })
            .unwrap();
        assert_eq!(settings.flags, [0; 4]);
    }
    #[test]
    fn fresh_control_applies_flag_before_charmer_gate_and_walks_real_collections_in_source_order() {
        let handle = |construction| ObjectHandle {
            thread: 1,
            construction,
        };
        let guid = |id| SourceGuid::with_entry(HighGuid::Pet, 1, id);
        let mut control = SourceControlledUnits {
            pet: guid(1),
            guardians: BTreeSet::from([guid(2)]),
            totems: [
                guid(3),
                SourceGuid::default(),
                SourceGuid::default(),
                SourceGuid::default(),
            ],
            charm: guid(4),
            private_charms: BTreeSet::from([guid(5)]),
        };
        let mut flags = 0x10;
        let mut calls = Vec::new();
        let lookup = |_: ControlledLookup, g: SourceGuid| Ok(Some(handle(u64::from(g.counter()))));
        control
            .set_immune(
                0x100,
                &mut flags,
                true,
                SourceControlGuids::default(),
                lookup,
                |child, mask| {
                    calls.push((child, mask));
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(flags, 0x110);
        assert_eq!(
            calls,
            (1..=5).map(|id| (handle(id), 0x100)).collect::<Vec<_>>()
        );
        let mut calls = Vec::new();
        control
            .set_immune(
                0x200,
                &mut flags,
                false,
                SourceControlGuids::default(),
                lookup,
                |child, _| {
                    calls.push(child);
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(calls, [handle(1), handle(2), handle(5)]);
        let charmed = SourceControlGuids {
            charmer: guid(9),
            creator: SourceGuid::default(),
        };
        control
            .set_immune(
                0x100,
                &mut flags,
                true,
                charmed,
                |_, _| panic!("charm suppresses lookup"),
                |_, _| panic!("charm suppresses propagation"),
            )
            .unwrap();
        SourceControlledUnits::default()
            .set_immune(
                0x100,
                &mut flags,
                false,
                SourceControlGuids::default(),
                |_, _| panic!("constructor GUIDs/sets are empty"),
                |_, _| panic!("constructor collections are empty"),
            )
            .unwrap();
    }
}
