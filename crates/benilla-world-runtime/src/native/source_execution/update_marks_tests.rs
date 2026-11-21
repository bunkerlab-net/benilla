// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

#[test]
fn client_update_refuses_each_type_end_before_writing() {
    // UpdateFields.h:31,176,266,287. Padding below these ends still has a changed bit.
    let mut objects = SourceObjects::default();
    for (kind, count) in [
        (SourceObjectKind::Creature, 188),
        (SourceObjectKind::Pet, 188),
        (SourceObjectKind::Player, 1282),
        (SourceObjectKind::GameObject, 26),
        (SourceObjectKind::Transport, 26),
    ] {
        let object = objects.construct(1, kind).unwrap();
        let mut field = 0u32;
        assert!(objects
            .set_update_value(object, count - 1, &mut field, 1)
            .unwrap());
        let mask = objects.get(object).unwrap();
        assert!(mask.field_changed(count - 1).unwrap());
        assert!(!mask.field_changed(count - 2).unwrap());
        for value in [1, 2] {
            assert_eq!(
                objects
                    .set_update_value(object, count, &mut field, value)
                    .unwrap_err()
                    .id,
                "execution.object.Object_849.update_field_index_assert"
            );
            assert_eq!(field, 1);
        }
    }
}

#[test]
fn client_update_refuses_without_a_bound_field_layout() {
    let mut objects = SourceObjects::default();
    for kind in [
        // Item.cpp:189, Bag.cpp:29, Corpse.cpp:36, DynamicObject.cpp:35.
        SourceObjectKind::Item,
        SourceObjectKind::Container,
        SourceObjectKind::Corpse,
        SourceObjectKind::Bones,
        SourceObjectKind::DynamicObject,
        SourceObjectKind::Camera,
    ] {
        let object = objects.construct(1, kind).unwrap();
        assert_eq!(
            objects.force_update_field(object, 0).unwrap_err().id,
            "execution.object.Object_105.update_values_count_owner"
        );
    }
}
