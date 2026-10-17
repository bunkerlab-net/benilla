// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit synthetic test backend. Reuses chat display, never fake WoW item/state updates.
use super::*;

pub(super) fn run(
    settings: P2pSettings,
    events: &Sender<SessionEvent>,
    commands: &Receiver<ClientCommand>,
    stop: &AtomicBool,
) -> Result<()> {
    let id = settings
        .0
        .encounter
        .as_ref()
        .context("missing encounter config")?
        .local_id;
    let mut controller = benilla_encounter::Controller::new(settings.0, settings.1)?;
    emit(
        events,
        SessionEvent::Connected {
            self_guid: guid(id),
            name: name(id),
            billing_time_rested: 0,
            tutorial_flags: None,
            addon_info: Some(Vec::new()),
        },
    )?;
    emit(
        events,
        SessionEvent::Worldport {
            map_id: 0,
            position: SPAWN,
            orientation: 0.0,
            needs_ack: false,
        },
    )?;
    // The ordinary avatar is only a local camera anchor. No simulation HP/XP/item fields
    // enter the realm ECS: the test encounter is deliberately non-spatial and text-only.
    emit(events, avatar(id, SPAWN, 0.0))?;
    emit(
        events,
        SessionEvent::TimeSpeed {
            hours: 12,
            minutes: 0,
            day_serial: 0,
            timescale: 0.01666667,
        },
    )?;
    for line in controller.lobby() {
        emit(events, chat(0, line))?;
    }
    loop {
        if stop.load(Ordering::SeqCst) {
            return Ok(());
        }
        for line in controller.poll() {
            emit(events, chat(0, line))?;
        }
        match commands.recv_timeout(Duration::from_millis(10)) {
            Ok(ClientCommand::Chat {
                kind: ChatKind::Say,
                text,
                ..
            }) => {
                for line in controller.command(&text) {
                    emit(events, chat(0, line))?;
                }
            }
            Ok(ClientCommand::Chat { .. }) => {
                emit(events, chat(0, "Synthetic encounter controls use /say !status (or !ready, !attack, !stop, !pause, !resume, !leave).".into()))?;
            }
            Ok(ClientCommand::NameQuery { guid: query }) if query == guid(id) => {
                emit(events, player_name(id))?;
            }
            Ok(ClientCommand::Logout | ClientCommand::ForceLogout) => {
                controller.command("!leave");
                emit(
                    events,
                    SessionEvent::Disconnected {
                        reason: "Synthetic encounter closed. Restart with a fresh epoch.".into(),
                        end: SessionEnd::Lost,
                    },
                )?;
                return Ok(());
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                controller.command("!leave");
                return Ok(());
            }
            _ => (), // Movement/target/action bars have no synthetic combat semantics.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encounter_backend_opens_local_chat_lobby_without_opening_transport() {
        let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = reservation.local_addr().unwrap();
        let config = format!(
            r#"
[p2p]
psk = "test-only synthetic encounter phrase"
listen = "{endpoint}"
[p2p.encounter]
local_id = "01010101010101010101010101010101"
epoch = "0101010101010101010101010101010101010101010101010101010101010101"
roster = [{{id="01010101010101010101010101010101", address="{endpoint}"}}, {{id="02020202020202020202020202020202", address="127.0.0.1:65534"}}]
"#
        );
        let config =
            benilla_config::AppConfig::parse(&config, std::path::Path::new("/tmp")).unwrap();
        let (handles, mut start) = prepare(
            P2pSettings(config.p2p.unwrap(), config.ruleset),
            P2pEndpointTicket::default(),
        );
        start.start(None, None).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut connected = false;
        loop {
            let event = handles
                .events
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap();
            match event {
                SessionEvent::Connected { .. } => connected = true,
                SessionEvent::Chat(m) if m.text.contains("SYNTHETIC TEST ENCOUNTER") => break,
                SessionEvent::Disconnected { reason, .. } => panic!("{reason}"),
                _ => (),
            }
        }
        assert!(connected);
        // Endpoint remains occupied: lobby must not bind before explicit ready.
        handles
            .commands
            .send(ClientCommand::Chat {
                kind: ChatKind::Say,
                target: None,
                text: "!ready".into(),
                language: None,
            })
            .unwrap();
        loop {
            let event = handles.events.recv_timeout(Duration::from_secs(3)).unwrap();
            if let SessionEvent::Chat(m) = event {
                if m.text.contains("Cannot start encounter") {
                    break;
                }
            }
        }
    }
}
