// SPDX-License-Identifier: GPL-3.0-or-later
//! Loaded authored Map.dbc owner through the actual controller/store; no release admission.
use super::*;
use n::{synthetic_map_classification_fixture as maps, synthetic_source_spline_fixture as splines};

fn profile() -> n::Profile {
    let mut data = splines::surviving_profile();
    data.initialize_map_classification(&maps::catalog(data.map_id, 0))
        .unwrap();
    data
}
fn enter(h: &Harness, data: n::Profile, open: bool) -> NativeController {
    NativeController::enter_component_test(
        &h.root,
        &h.profile,
        &h.character,
        if open {
            Entry::Open { world: [8; 16] }
        } else {
            Entry::Create {
                world: [8; 16],
                seed: 42,
            }
        },
        data,
        &MeleeCatalog,
        MeleeAuthority::default(),
    )
    .unwrap()
}
#[test]
fn selected_source_map_binding_survives_actual_tick_checkpoint_retry_and_reopen() {
    let h = Harness::new();
    let data = profile();
    let selected = data.map_classification.as_ref().unwrap().clone();
    assert_eq!(selected.map_id(), data.map_id);
    assert_eq!(selected.kind(), n::MapKind::Common);
    assert!(!selected.instanceable());
    assert!(!selected.is_battleground());
    let profile_digest = data.digest();
    let binding = selected.binding();
    let mut c = enter(&h, data.clone(), false);
    let mut replay = c.world.clone();
    c.receive(move_to([0.25, 0.0, 0.0])).unwrap();
    fs::write(
        h.path().join(".world-checkpoint.pending"),
        b"authored recoverable fault",
    )
    .unwrap();
    let withheld = c.pump(c.tick_duration());
    assert_eq!(withheld.ticks, 1);
    assert!(withheld.events.is_empty());
    assert!(matches!(
        withheld.pause.unwrap().kind,
        PauseKind::Checkpoint {
            requires_reopen: false
        }
    ));
    assert_eq!(c.world.state().next_sequence, 1);
    replay
        .replay(std::slice::from_ref(c.world.last_tick().unwrap()))
        .unwrap();
    let checkpoint = c.world.checkpoint().unwrap();
    assert_eq!(replay.checkpoint().unwrap(), checkpoint);
    assert_eq!(c.world.profile().digest(), profile_digest);
    assert_eq!(
        c.world
            .profile()
            .map_classification
            .as_ref()
            .unwrap()
            .binding(),
        binding
    );
    fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
    c.retry_checkpoint().unwrap();
    let out = c.pump(Duration::ZERO);
    assert!(out.pause.is_none());
    assert!(!out.events.is_empty());
    assert_eq!(c.position_xyz(), [0.25, 0.0, 0.0]);
    assert!(c.pump(Duration::ZERO).events.is_empty());
    assert_eq!(c.world.checkpoint().unwrap(), checkpoint);
    drop(c);
    let reopened = enter(&h, data, true);
    assert_eq!(reopened.world.checkpoint().unwrap(), checkpoint);
    assert_eq!(reopened.world.profile().digest(), profile_digest);
    assert_eq!(
        reopened.world.profile().map_classification.as_ref(),
        Some(&selected)
    );
    assert_eq!(
        reopened
            .world
            .profile()
            .map_classification
            .as_ref()
            .unwrap()
            .binding(),
        binding
    );
}
#[test]
fn source_instance_or_battleground_cannot_relabel_existing_ordinary_death_config() {
    let h = Harness::new();
    for kind in [1, 2, 3] {
        let mut data = splines::surviving_profile();
        let before = data.clone();
        let result = data.initialize_map_classification(&maps::catalog(data.map_id, kind));
        if kind == 3 {
            assert!(matches!(
                result,
                Err(n::NativeError::UnsupportedMechanic(
                    "source battleground player death"
                ))
            ));
        } else {
            assert!(matches!(result, Err(n::NativeError::AuthorityBinding)));
        }
        assert_eq!(data, before);
        assert!(data.map_classification.is_none());
        assert!(!h.path().parent().unwrap().exists());
    }
}
