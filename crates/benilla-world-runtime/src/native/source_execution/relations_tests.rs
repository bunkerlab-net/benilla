// SPDX-License-Identifier: GPL-3.0-or-later
//! Relations.cpp:100-267,458-510 authored boundaries and static stock operands.
use super::spells::power_tests::PowerFixture;
use super::*;

// FactionTemplate.dbc ordinals0,6,10, read independently from catalog04's raw WDBC.
fn real_template(id: u32) -> SourceFactionTemplate {
    let row = match id {
        1 => [1, 72, 3, 2, 12],
        7 => [7, 0, 0, 0, 0],
        14 => [14, 0, 8, 0, 1],
        _ => panic!("not a retained test row"),
    };
    SourceFactionTemplate {
        faction: row[0],
        flags: row[1],
        group: row[2],
        friends: row[3],
        enemies: row[4],
        enemy_factions: [0; 4],
        friend_factions: [0; 4],
    }
}

#[test]
fn relations_real_faction_templates_preserve_mask_order_without_private_inputs() {
    let human = real_template(1);
    assert_eq!(human.reaction_to(&human), 4);
    assert_eq!(human.reaction_to(&real_template(7)), 3);
    assert_eq!(human.reaction_to(&real_template(14)), 1);
    let mut conflict = human.clone();
    conflict.enemies = 3;
    assert_eq!(conflict.reaction_to(&human), 1);
    conflict.enemies = 0;
    conflict.friends = 0;
    conflict.enemy_factions = [1, 0, 0, 0];
    assert_eq!(conflict.reaction_to(&human), 1);
    conflict.enemy_factions = [0, 1, 0, 0];
    assert_eq!(conflict.reaction_to(&human), 4);
    conflict.group = 0;
    conflict.friend_factions = [1, 0, 0, 0];
    assert_eq!(conflict.reaction_to(&human), 4);
    assert_eq!(human.reaction_to(&conflict), 4);
}
#[test]
fn relations_identity_precedes_all_reaction_reads_but_not_uninteractible() {
    struct Flags(u32);
    impl SourceRelationOwner for Flags {
        fn relation_flags(&self, _: ObjectHandle) -> Result<u32> {
            Ok(self.0)
        }
    }
    let f = PowerFixture::new();
    assert_eq!(unit_reaction_to(f.caster, f.caster, &Flags(8)).unwrap(), 4);
    assert!(unit_can_assist_spell(f.caster, f.caster, &Flags(0)).unwrap());
    assert!(!unit_can_assist_spell(f.caster, f.caster, &Flags(0x02000008)).unwrap());
    assert_eq!(
        unit_can_assist(f.caster, f.caster, &Flags(8))
            .unwrap_err()
            .id,
        "execution.relations.CanAssist_487_GetControllingPlayer_owner"
    );
}

#[test]
fn relations_live_player_control_and_pvp_master_boundaries() {
    struct AuthoredMaster<'a> {
        flags: SourceRelations<'a>,
        master: Option<ObjectHandle>,
    }
    impl SourceRelationOwner for AuthoredMaster<'_> {
        fn relation_flags(&self, unit: ObjectHandle) -> Result<u32> {
            self.flags.relation_flags(unit)
        }
        fn relation_master(&self, _: ObjectHandle) -> Result<Option<ObjectHandle>> {
            Ok(self.master)
        }
    }
    let mut f = PowerFixture::new();
    let target = f.objects.construct(1, SourceObjectKind::Creature).unwrap();
    let mut fields = f.units.get(&f.objects, f.caster).unwrap().clone();
    fields.flags = 0;
    f.units.register(&f.objects, target, fields).unwrap();
    let factions = CreatureAiFactions::default();
    f.units.get_mut(&f.objects, f.caster).unwrap().flags = 8;
    let relations = SourceRelations {
        objects: &f.objects,
        units: &f.units,
        factions: &factions,
    };
    assert_eq!(
        unit_reaction_to(f.caster, target, &relations)
            .unwrap_err()
            .id,
        "execution.relations.GetReactionTo_208_GetControllingPlayer_owner"
    );
    for (master, target_flags, caster_flags, expected) in [
        (None, 0, 0, false),
        (None, 0x1000, 0, true),
        (None, 0x1100, 0, true),
        (Some(f.caster), 0, 0x1000, true),
        (Some(f.caster), 0x100, 0x1000, false),
        (Some(f.caster), 0x1000, 0, false),
    ] {
        f.units.get_mut(&f.objects, target).unwrap().flags = target_flags;
        f.units.get_mut(&f.objects, f.caster).unwrap().flags = caster_flags;
        let relations = AuthoredMaster {
            flags: SourceRelations {
                objects: &f.objects,
                units: &f.units,
                factions: &factions,
            },
            master,
        };
        assert_eq!(unit_is_pvp_ui(target, &relations).unwrap(), expected);
    }
}

#[test]
fn relations_live_master_lookup_refuses_without_guid_accessor_owner() {
    let mut f = PowerFixture::new();
    let fields = f.units.get_mut(&f.objects, f.caster).unwrap();
    fields.owner = Some(f.caster);
    fields.charmer = Some(f.caster);
    let factions = CreatureAiFactions::default();
    let relations = SourceRelations {
        objects: &f.objects,
        units: &f.units,
        factions: &factions,
    };
    assert_eq!(
        unit_is_pvp_ui(f.caster, &relations).unwrap_err().id,
        "execution.relations.GetMaster_6716_owner"
    );
}

#[test]
fn relations_persuaded_state_is_live_and_restore_still_refuses() {
    let mut f = PowerFixture::new();
    // Object.cpp:105-108 zeros both UNIT_FIELD_PERSUADED words read by Unit.h:2578.
    assert_eq!(f.units.get(&f.objects, f.caster).unwrap().persuaded, None);
    let before = serde_json::to_vec(f.units.get(&f.objects, f.caster).unwrap()).unwrap();
    f.units.get_mut(&f.objects, f.caster).unwrap().persuaded = Some(f.caster);
    assert_eq!(
        serde_json::to_vec(f.units.get(&f.objects, f.caster).unwrap()).unwrap(),
        before
    );
    assert_eq!(
        f.units.inventory(&f.objects).unwrap()["restore"],
        "execution.unit.f7_restore_owner"
    );
}

#[test]
fn relations_catalog_faction_rows_have_friendly_neutral_hostile_paths() {
    let Some(root) = std::env::var_os("BENILLA_F5_AI_CATALOG") else {
        return;
    };
    let mut catalog =
        super::super::production_compiler::VerifiedCatalog::open(Path::new(&root), false).unwrap();
    let template = catalog_creature_ai_template(&mut catalog, 11577)
        .unwrap()
        .unwrap();
    assert_eq!(
        (
            template.ordinal,
            template.faction,
            template.unit_flags,
            template.inhabit_type
        ),
        (6753, 91, 0, 1)
    );
    assert_eq!(template.static_flags, [0; 4]);
    let addon = catalog
        .execution_creature_addon_rows(CreatureAddonTable::Template)
        .unwrap()
        .into_iter()
        .find(|row| row.words[0] == 11577)
        .unwrap();
    assert_eq!(addon.ordinal, 5522);
    assert_eq!(addon.strings[0], b"18148");
    assert!(!catalog
        .execution_creature_addon_rows(CreatureAddonTable::Guid)
        .unwrap()
        .iter()
        .any(|row| row.words[0] == 27677));
    let factions = CreatureAiFactions::load_catalog(&mut catalog).unwrap();
    for id in [1, 7, 14] {
        assert_eq!(factions.relation_template(id), Some(&real_template(id)));
    }
    assert_eq!(factions.relation_template(91).unwrap().flags, 0);
    // FactionTemplate.dbc retained rows1,2,7,14 are independent source operands.
    for id in [1, 2, 7, 14] {
        println!(
            "RELATIONS_STATIC faction_template={id} row={:?}",
            factions.relation_template(id).unwrap()
        );
    }
    let mut f = PowerFixture::new();
    let target = f.objects.construct(1, SourceObjectKind::Creature).unwrap();
    let mut fields = f.units.get(&f.objects, f.caster).unwrap().clone();
    fields.flags = 0;
    f.units.register(&f.objects, target, fields).unwrap();
    for (a, b, expected) in [(1, 1, 4), (1, 7, 3), (1, 14, 1)] {
        f.units.get_mut(&f.objects, f.caster).unwrap().flags = 0;
        f.units.get_mut(&f.objects, f.caster).unwrap().faction = a;
        f.units.get_mut(&f.objects, target).unwrap().faction = b;
        let relations = SourceRelations {
            objects: &f.objects,
            units: &f.units,
            factions: &factions,
        };
        assert_eq!(
            unit_reaction_to(f.caster, target, &relations).unwrap(),
            expected
        );
        assert_eq!(
            unit_can_assist(f.caster, target, &relations).unwrap(),
            expected >= 4
        );
    }
    f.units.get_mut(&f.objects, target).unwrap().flags = 8;
    f.units.get_mut(&f.objects, target).unwrap().faction = 1;
    let relations = SourceRelations {
        objects: &f.objects,
        units: &f.units,
        factions: &factions,
    };
    assert_eq!(
        unit_reaction_to(f.caster, target, &relations)
            .unwrap_err()
            .id,
        "execution.relations.GetFactionReaction_166_GetControllingPlayer_owner"
    );
}
