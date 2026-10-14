// SPDX-License-Identifier: GPL-3.0-or-later
//! Blocking app worker; no transport configuration or peer readiness enters the solo clock.
use super::*;
use crate::net::ClientCommand;
use crossbeam_channel::{Receiver, Sender};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "Native gameplay retry controls are enabled with its future entry producer"
    )
)]
pub(crate) enum Control {
    RetryCheckpoint,
    RetryTick,
}
pub(crate) struct Exit {
    pub saved: SavedBoundary,
    pub cancelled_inputs: usize,
}
fn emit(events: &Sender<SessionEvent>, event: SessionEvent) -> Result<(), Error> {
    events
        .send_timeout(event, Duration::from_millis(100))
        .map_err(|_| {
            Error::Io(std::io::Error::other(
                "world event channel unavailable; gameplay stopped",
            ))
        })
}
fn emit_all(events: &Sender<SessionEvent>, batch: Vec<SessionEvent>) -> Result<(), Error> {
    for event in batch {
        emit(events, event)?;
    }
    Ok(())
}
/// Ordinary logout retains ownership on final-save failure and keeps accepting explicit retry.
/// Only process stop or an unavailable app channel can end an unconfirmed session; its error
/// states that the last admitted boundary is not durably saved. No logout success is emitted.
pub(crate) fn run(
    controller: &mut dyn Driven,
    events: &Sender<SessionEvent>,
    commands: &Receiver<ClientCommand>,
    control: &Receiver<Control>,
    stop: &AtomicBool,
) -> Result<Exit, Error> {
    match drive(controller, events, commands, control, stop) {
        Ok(exit) => Ok(exit),
        Err(error) => {
            // Output/producer loss cannot leave a running simulation. Attempt its checkpoint;
            // preserve the error even if storage succeeded, since the session channel is lost.
            if !controller.closed() {
                controller.shutdown()?;
            }
            Err(error)
        }
    }
}
fn finish(controller: &mut dyn Driven, events: &Sender<SessionEvent>) -> Result<Exit, Error> {
    let (saved, cancelled_inputs, final_events) = controller.shutdown()?;
    emit_all(events, final_events)?;
    if cancelled_inputs > 0 {
        emit(
            events,
            SessionEvent::Notification {
                text: format!(
                    "World saved. {cancelled_inputs} pending intentions were cancelled on logout."
                ),
            },
        )?;
    }
    Ok(Exit {
        saved,
        cancelled_inputs,
    })
}
fn checkpoint_paused(controller: &dyn Driven) -> bool {
    !controller.closed()
        && matches!(
            controller.pause().map(|p| &p.kind),
            Some(PauseKind::Checkpoint { .. })
        )
}
fn drive(
    controller: &mut dyn Driven,
    events: &Sender<SessionEvent>,
    commands: &Receiver<ClientCommand>,
    control: &Receiver<Control>,
    stop: &AtomicBool,
) -> Result<Exit, Error> {
    emit_all(events, controller.initial_events()?)?;
    let mut last_clock = Instant::now();
    let mut last_pause = None;
    loop {
        if stop.load(Ordering::SeqCst) {
            return finish(controller, events);
        }
        if let Ok(control) = control.try_recv() {
            if controller.closing() {
                match finish(controller, events) {
                    Ok(exit) => return Ok(exit),
                    Err(error) if checkpoint_paused(controller) => {
                        emit(events, SessionEvent::Notification { text: format!("Logout remains paused; unsaved progress is still held in this session: {error}") })?;
                    }
                    Err(error) => return Err(error),
                }
            } else {
                let retry = match control {
                    Control::RetryCheckpoint => controller.retry_checkpoint().map(|_| ()),
                    Control::RetryTick => controller.retry_tick(),
                };
                match retry {
                    Ok(_) => {
                        last_clock = Instant::now();
                        last_pause = None;
                        emit(
                            events,
                            SessionEvent::ClientControl {
                                mover: controller.player_guid(),
                                allow_move: true,
                            },
                        )?;
                        emit(
                            events,
                            SessionEvent::Notification {
                                text: "World retry accepted; gameplay resumed.".into(),
                            },
                        )?;
                    }
                    Err(error) => emit(
                        events,
                        SessionEvent::Notification {
                            text: format!("World remains paused: {error}"),
                        },
                    )?,
                }
            }
        }
        // Bound work under a continuously full app input channel; arrivals cannot manufacture
        // ticks and cannot prevent the fixed clock / stop flag from being checked.
        for _ in 0..MAX_ACTIONS {
            match commands.try_recv() {
                Ok(command) => match controller.receive(command) {
                    Ok(received) => {
                        emit_all(events, received.events)?;
                        if received.logout {
                            match finish(controller, events) {
                                Ok(exit) => return Ok(exit),
                                Err(error) if checkpoint_paused(controller) => {
                                    emit(events, SessionEvent::Notification { text: format!("Logout could not save the world; unsaved progress is still held in this session. Repair storage and retry: {error}") })?;
                                    break;
                                }
                                Err(error) => return Err(error),
                            }
                        }
                    }
                    Err(error) => emit(
                        events,
                        SessionEvent::Notification {
                            text: error.to_string(),
                        },
                    )?,
                },
                Err(crossbeam_channel::TryRecvError::Empty) => break,
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    return finish(controller, events);
                }
            }
        }
        let now = Instant::now();
        let output = controller.pump(now.duration_since(last_clock));
        last_clock = now;
        emit_all(events, output.events)?;
        if output.pause != last_pause {
            if let Some(pause) = &output.pause {
                emit(
                    events,
                    SessionEvent::ClientControl {
                        mover: controller.player_guid(),
                        allow_move: false,
                    },
                )?;
                emit(events, SessionEvent::Notification { text: format!("World paused at tick {}: {}. Unsaved progress remains in this session; no replacement was opened.", controller.tick(), pause.message) })?;
            }
            last_pause = output.pause;
        }
        // Pending catch-up runs in bounded batches without discarding ticks. Paused sessions
        // wait as well, retaining the world/store/queue while accepting an explicit retry.
        if output.pending_ticks == 0 || controller.pause().is_some() {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

/// The same app event/input worker drives each explicitly selected runtime.
pub(crate) trait Driven: Send {
    fn initial_events(&mut self) -> Result<Vec<SessionEvent>, Error>;
    fn initial_player_fields(&self) -> Result<benilla_protocol::ObjectFields, Error>;
    fn selected_player(&self) -> PlayerId;
    fn map_id(&self) -> u32;
    fn position_xyz(&self) -> [f32; 3];
    fn facing(&self) -> f32;
    fn player_guid(&self) -> u64;
    fn tick(&self) -> u64;
    fn pause(&self) -> Option<&Pause>;
    fn closing(&self) -> bool;
    fn closed(&self) -> bool;
    fn receive(&mut self, command: ClientCommand) -> Result<commands::Received, Error>;
    fn pump(&mut self, elapsed: Duration) -> Pump;
    fn retry_checkpoint(&mut self) -> Result<SavedBoundary, Error>;
    fn retry_tick(&mut self) -> Result<(), Error> {
        Err(Error::Paused)
    }
    fn shutdown(&mut self) -> Result<(SavedBoundary, usize, Vec<SessionEvent>), Error>;
}
impl Driven for Controller {
    fn initial_events(&mut self) -> Result<Vec<SessionEvent>, Error> {
        Controller::initial_events(self)
    }
    fn initial_player_fields(&self) -> Result<benilla_protocol::ObjectFields, Error> {
        Controller::initial_player_fields(self)
    }
    fn selected_player(&self) -> PlayerId {
        Controller::selected_player(self)
    }
    fn map_id(&self) -> u32 {
        self.position().map
    }
    fn position_xyz(&self) -> [f32; 3] {
        self.position().xyz.map(|v| v as f32 / 1000.0)
    }
    fn facing(&self) -> f32 {
        Controller::facing(self)
    }
    fn player_guid(&self) -> u64 {
        Controller::player_guid(self)
    }
    fn tick(&self) -> u64 {
        self.world.state().tick
    }
    fn pause(&self) -> Option<&Pause> {
        Controller::pause(self)
    }
    fn closing(&self) -> bool {
        self.closing
    }
    fn closed(&self) -> bool {
        self.closed
    }
    fn receive(&mut self, command: ClientCommand) -> Result<commands::Received, Error> {
        Controller::receive(self, command)
    }
    fn pump(&mut self, elapsed: Duration) -> Pump {
        Controller::pump(self, elapsed)
    }
    fn retry_checkpoint(&mut self) -> Result<SavedBoundary, Error> {
        Controller::retry_checkpoint(self)
    }
    fn shutdown(&mut self) -> Result<(SavedBoundary, usize, Vec<SessionEvent>), Error> {
        Controller::shutdown(self)
    }
}
