#![cfg(test)]
mod net {
    use benilla_protocol::{JumpInfo, TransportPose};
    include!(concat!(env!("OUT_DIR"), "/commands.rs"));
}
#[path = "../../benilla-app/src/net/p2p/world.rs"]
mod world;
