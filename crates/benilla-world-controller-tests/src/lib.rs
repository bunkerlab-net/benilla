#![cfg(test)]
pub(crate) mod native_geometry;
#[path = "../../benilla-app/src/net/p2p/world_setup.rs"]
mod world_setup;
mod net {
    pub(crate) mod p2p {
        pub(crate) use crate::native_geometry;
    }
    use benilla_protocol::{
        messages::{MoveMode, SpeedKind},
        JumpInfo, TransportPose,
    };
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
