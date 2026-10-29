// SPDX-License-Identifier: GPL-3.0-or-later
//! Conditional component integration only: production native entry remains uninhabited.
use super::*;
fn load_config(h: &Harness, text: &str) -> benilla_config::AppConfig {
    fs::write(h.root.join("config.toml"), text).unwrap();
    benilla_config::AppConfig::load_for_executable(&h.root.join("benilla")).unwrap()
}
/// This private test bridge consumes the actual parsed explicit setting before
/// component entry. It is not called by production entry or the private compiler.
fn enter_configured(
    h: &Harness,
    open: bool,
    config: &benilla_config::AppConfig,
) -> Result<NativeController, Error> {
    let selected = config.native_regeneration.ok_or(Error::NativeRuntime(
        n::NativeError::UnsupportedMechanic("explicit component health rate missing"),
    ))?;
    let rate = benilla_sim::classic_spell::SpellFloat::from_bits(selected.health_rate_bits())
        .map_err(|_| Error::NativeRuntime(n::NativeError::InvalidProfile))?;
    let mut data = armor::weapon_profile();
    data.initialize_player_health_regeneration(rate)
        .map_err(Error::NativeRuntime)?;
    NativeController::enter_component_test(
        &h.root,
        &h.profile,
        &h.character,
        if open {
            Entry::Open { world: [8; 16] }
        } else {
            Entry::Create {
                world: [8; 16],
                seed: 7,
            }
        },
        data,
        &StaffCatalog,
        armor::EquipmentAuthority::default(),
    )
}
fn durable_files(h: &Harness) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files: Vec<_> = fs::read_dir(h.path())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.file_name().unwrap() != "world.lock")
        .map(|path| {
            let bytes = fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}
#[test]
fn explicit_config_required_before_component_files_and_rate_is_bound_on_reopen() {
    let h = Harness::new();
    let absent = load_config(&h, "");
    assert!(matches!(
        enter_configured(&h, false, &absent),
        Err(Error::NativeRuntime(n::NativeError::UnsupportedMechanic(
            "explicit component health rate missing"
        )))
    ));
    assert!(!h.path().exists());
    let config = load_config(&h, "[native_regeneration]\nhealth_rate=0\n");
    let c = enter_configured(&h, false, &config).unwrap();
    assert_eq!(
        c.world
            .profile()
            .player_health_regeneration
            .as_ref()
            .unwrap()
            .health_rate()
            .bits(),
        0.0f32.to_bits()
    );
    let checkpoint = c.world.checkpoint().unwrap();
    drop(c);
    let before = durable_files(&h);
    let different = load_config(&h, "[native_regeneration]\nhealth_rate=1\n");
    assert!(enter_configured(&h, true, &different).is_err());
    assert_eq!(durable_files(&h), before);
    assert!(enter_configured(&h, true, &absent).is_err());
    assert_eq!(durable_files(&h), before);
    let c = enter_configured(&h, true, &config).unwrap();
    assert_eq!(c.world.checkpoint().unwrap(), checkpoint);
}
#[test]
fn actual_damage_escape_and_health_pulse_wait_for_checkpoint_then_reopen() {
    let h = Harness::new();
    let config = load_config(&h, "[native_regeneration]\nhealth_rate=1\n");
    let mut c = enter_configured(&h, false, &config).unwrap();
    c.initial_events().unwrap();
    let initial_health = c.world.state().health;
    let target = c.projection.npc_guid(c.world.state().npcs[&2].id).unwrap();
    c.receive(ClientCommand::CastSpell {
        spell_id: 133,
        target: Some(target),
    })
    .unwrap();
    for _ in 0..150 {
        step(&mut c);
        if c.world.state().health < initial_health {
            break;
        }
    }
    assert!(c.world.state().health < initial_health);
    assert!(c.world.state().combat.as_ref().unwrap().in_combat);
    for _ in 0..25 {
        step(&mut c);
    }
    assert!(c.world.state().player_alive());
    c.receive(move_to([100.0, 0.0, 0.0])).unwrap();
    let mut healed = None;
    for _ in 0..400 {
        let before = c.world.state().health;
        let due = c.world.state().resources.regen_elapsed_ms() + c.world.profile().tick_ms >= 2000;
        if due {
            inject(&h);
            let failed = c.pump(c.tick_duration());
            paused(&failed);
            assert!(failed.events.is_empty());
            let pulse = c
                .world
                .last_tick()
                .unwrap()
                .events
                .iter()
                .find_map(|event| match event {
                    n::Event::PlayerHealthRegenerated {
                        before,
                        after,
                        diff_ms,
                    } => Some((*before, *after, *diff_ms)),
                    _ => None,
                });
            let digest = c.world.state_digest();
            repair(&h);
            c.retry_checkpoint().unwrap();
            let out = c.pump(Duration::ZERO);
            assert!(out.pause.is_none());
            if let Some((old, new, diff)) = pulse {
                assert_eq!(old, before);
                assert!(new > old);
                assert!(diff >= 2000);
                assert!(!c.world.state().combat.as_ref().unwrap().in_combat);
                assert_eq!(
                    values(&out.events, c.player_guid()).unit_health(),
                    Some(new)
                );
                assert_eq!(c.world.state_digest(), digest);
                assert!(c.pump(Duration::ZERO).events.is_empty());
                healed = Some((digest, new));
                break;
            }
        } else {
            step(&mut c);
        }
    }
    let (digest, health) =
        healed.expect("actual out-of-combat source cadence must heal the prior NPC damage");
    drop(c);
    let mut c = enter_configured(&h, true, &config).unwrap();
    assert_eq!(c.world.state_digest(), digest);
    assert_eq!(c.world.state().health, health);
    let initial = c.initial_events().unwrap();
    assert_eq!(
        values(&initial, c.player_guid()).unit_health(),
        Some(health)
    );
}
