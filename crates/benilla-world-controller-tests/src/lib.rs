#![cfg(test)]
mod net {
    use benilla_protocol::{messages::MoveMode, JumpInfo, TransportPose};
    include!(concat!(env!("OUT_DIR"), "/commands.rs"));
}
#[path = "../../benilla-app/src/net/p2p/world.rs"]
mod world;

mod creature_anim {
    #[allow(dead_code)]
    pub(crate) mod move_flags {
        include!(concat!(env!("OUT_DIR"), "/move_flags.rs"));
    }
}
