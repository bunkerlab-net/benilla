//! Attach the persistent setup mailbox to each in-game UI VM without performing disk work.
use crate::net::P2pEndpointTicket;
use crate::ui_script::VmMemo;
use benilla_ui::script::UiScript;
use bevy::prelude::*;

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        attach
            .in_set(benilla_world::schedule::WorldStage::Present)
            .run_if(crate::ui_script::ingame_ui_up),
    );
}
fn attach(
    script: Option<NonSend<UiScript>>,
    ticket: Res<P2pEndpointTicket>,
    mut memo: Local<VmMemo<bool>>,
) {
    let Some(script) = script else {
        return;
    };
    let attached = memo.get(&script);
    if *attached {
        return;
    }
    // Stock-only/disabled-layer VMs do not acquire fork-specific Lua globals.
    if !script.world_setup_interface_installed() {
        *attached = true;
        return;
    }
    match script.set_world_setup_mailbox(ticket.world_setup()) {
        Ok(()) => *attached = true,
        Err(error) => warn!("world setup interface: {error}"),
    }
}
