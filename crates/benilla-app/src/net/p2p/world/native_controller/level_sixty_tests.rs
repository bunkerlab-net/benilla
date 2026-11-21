// SPDX-License-Identifier: GPL-3.0-or-later
//! Exact app AdminBackend/controller/store seam; authored rows, no playable admission.
use super::*;
use benilla_protocol::ObjectFields;
use benilla_ui::script::admin::{self as ui, AdminBackend};

fn data(h: &Harness) -> n::Profile {
    let (mut profile, input, data) = quests::setup_with_levels(60, 60, true, 1.0f32.to_bits());
    profile.initialize_quest_progression(&input, data).unwrap();
    profile.admin = Some(n::AdminPolicy {
        world: [8; 16],
        world_owner: Selection::from_profile(&h.profile, &h.character)
            .unwrap()
            .player
            .owner,
        allowlist: Default::default(),
        revision: 7,
    });
    profile
}

fn enter(h: &Harness, open: bool) -> NativeController {
    h.enter(
        if open {
            Entry::Open { world: [8; 16] }
        } else {
            Entry::Create {
                world: [8; 16],
                seed: 7,
            }
        },
        data(h),
        quests::QuestAuthority::default(),
    )
    .unwrap()
}

fn request(
    backend: &impl AdminBackend,
    sequence: u64,
    operation: ui::AdminOperation,
) -> ui::AdminRequest {
    let snapshot = backend.snapshot();
    ui::AdminRequest {
        id: ui::AdminRequestId {
            context: snapshot.context.unwrap(),
            sequence,
        },
        catalog_revision: snapshot.revision,
        operation,
    }
}

fn committed(backend: &impl AdminBackend, request: &ui::AdminRequest) -> ui::AdminResult {
    let result = backend
        .poll(&request.id)
        .expect("durable source-backed receipt");
    assert!(
        matches!(&result.outcome, ui::AdminOutcome::Committed { receipt } if !receipt.is_empty())
    );
    result
}

fn player_fields(controller: &NativeController, events: &[SessionEvent]) -> ObjectFields {
    let mut result = ObjectFields::from_pairs(&[]);
    for event in events {
        if let SessionEvent::ObjectValues { guid, fields } = event {
            if *guid == controller.projection.player_guid() {
                result.merge(fields.clone());
            }
        }
    }
    result
}

fn raise_to(
    controller: &mut NativeController,
    backend: &impl AdminBackend,
    sequence: &mut u64,
    level: u8,
) {
    let before = controller.world.player_level();
    let mut requests = Vec::new();
    for _ in before..level {
        let request = request(backend, *sequence, ui::AdminOperation::GainOneLevel);
        *sequence += 1;
        backend.submit(request.clone()).unwrap();
        requests.push(request);
    }
    let output = controller.pump(controller.tick_duration());
    assert!(output.pause.is_none(), "{:?}", output.pause);
    assert_eq!(controller.world.player_level(), level);
    assert_eq!(
        output
            .events
            .iter()
            .filter(|event| matches!(event, SessionEvent::LevelUp(_)))
            .count(),
        usize::from(level - before)
    );
    for request in requests {
        committed(backend, &request);
    }
}

#[test]
fn level_sixty_boundary_saves_withhold_stock_points_until_retry_and_reopen_without_duplicates() {
    let h = Harness::new();
    let mut controller = enter(&h, false);
    let backend = controller.admin_backend();
    assert_eq!(
        backend.snapshot().level,
        Some(ui::AdminLevel {
            current: 1,
            maximum: 60
        })
    );
    let initial = controller.initial_events().unwrap();
    assert_eq!(
        player_fields(&controller, &initial).player_talent_points(),
        Some(0)
    );

    controller.receive(cast(&controller)).unwrap();
    for _ in 0..25 {
        let output = controller.pump(controller.tick_duration());
        assert!(output.pause.is_none(), "{:?}", output.pause);
        if controller.world.state().npcs[&2].health == 0 {
            break;
        }
    }
    assert_eq!(controller.world.player_level(), 2);
    assert_eq!(
        controller
            .world
            .state()
            .quest_progression
            .as_ref()
            .unwrap()
            .player
            .view()
            .xp,
        10
    );
    let off = request(
        &backend,
        1,
        ui::AdminOperation::SetXpGain { enabled: false },
    );
    backend.submit(off.clone()).unwrap();
    assert!(controller.pump(controller.tick_duration()).pause.is_none());
    committed(&backend, &off);
    let mut sequence = 2;

    for (before, after, points) in [(9, 10, 1), (59, 60, 51)] {
        raise_to(&mut controller, &backend, &mut sequence, before);
        let request = request(&backend, sequence, ui::AdminOperation::GainOneLevel);
        sequence += 1;
        backend.submit(request.clone()).unwrap();
        fs::write(
            h.path().join(".world-checkpoint.pending"),
            b"authored recoverable save fault",
        )
        .unwrap();
        let failed = controller.pump(controller.tick_duration());
        assert!(matches!(
            failed.pause.unwrap().kind,
            PauseKind::Checkpoint {
                requires_reopen: false
            }
        ));
        assert!(failed.events.is_empty());
        assert!(backend.poll(&request.id).is_none());
        assert_eq!(backend.snapshot().level.unwrap().current, before);
        assert_eq!(controller.world.player_level(), after);
        let digest = controller.world.state_digest();
        let next_sequence = controller.world.state().next_sequence;
        backend.submit(request.clone()).unwrap();
        fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
        controller.retry_checkpoint().unwrap();
        assert!(backend.poll(&request.id).is_none());
        let saved = controller.pump(Duration::ZERO);
        assert!(saved.pause.is_none());
        assert_eq!(
            saved
                .events
                .iter()
                .filter(|event| matches!(event, SessionEvent::LevelUp(_)))
                .count(),
            1
        );
        assert!(!saved
            .events
            .iter()
            .any(|event| matches!(event, SessionEvent::XpGain(_))));
        let fields = player_fields(&controller, &saved.events);
        assert_eq!(fields.unit_level(), Some(u32::from(after)));
        assert_eq!(fields.player_talent_points(), Some(points));
        assert!(fields.raw_fields().any(|pair| pair == (1102, points)));
        assert_eq!(fields.player_xp(), Some(10));
        assert_eq!(fields.unit_health(), Some(controller.world.state().health));
        assert_eq!(
            fields.unit_max_health(),
            Some(controller.world.player_limits().0)
        );
        assert_eq!(
            fields.unit_power(0),
            Some(controller.world.state().resources.caster.mana)
        );
        assert_eq!(
            fields.unit_max_power(0),
            Some(controller.world.player_limits().1)
        );
        for (index, stat) in controller
            .world
            .state()
            .combat
            .as_ref()
            .unwrap()
            .stats
            .attributes
            .iter()
            .enumerate()
        {
            assert_eq!(fields.unit_stat(index as u8), Some(*stat as u32));
        }
        assert_eq!(backend.snapshot().level.unwrap().current, after);
        assert_eq!(backend.snapshot().xp_enabled, Some(false));
        let receipt = committed(&backend, &request);
        assert_eq!(controller.world.state_digest(), digest);
        assert_eq!(controller.world.state().next_sequence, next_sequence);
        backend.submit(request.clone()).unwrap();
        assert!(controller.pump(Duration::ZERO).events.is_empty());
        assert_eq!(backend.poll(&request.id), Some(receipt));
        assert_eq!(controller.world.state().next_sequence, next_sequence);
    }

    let checkpoint = controller.world.checkpoint().unwrap();
    drop(controller);
    let mut reopened = enter(&h, true);
    assert_eq!(reopened.world.checkpoint().unwrap(), checkpoint);
    assert_eq!(h.profile.selected_character(), Some(&h.character));
    let initial = reopened.initial_events().unwrap();
    let fields = player_fields(&reopened, &initial);
    assert_eq!(fields.unit_level(), Some(60));
    assert_eq!(fields.player_talent_points(), Some(51));
    assert_eq!(fields.player_xp(), Some(10));
    assert_eq!(fields.player_next_level_xp(), Some(0));
    assert!(!initial
        .iter()
        .any(|event| matches!(event, SessionEvent::LevelUp(_) | SessionEvent::XpGain(_))));
    let backend = reopened.admin_backend();
    assert_eq!(backend.snapshot().xp_enabled, Some(false));
    assert!(matches!(
        &backend.snapshot().availability[ui::AdminAction::Level as usize],
        ui::AdminAvailability::Unavailable(_)
    ));
    assert!(reopened.pump(Duration::ZERO).events.is_empty());
}
