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

mod ui_unit {
    include!(concat!(env!("OUT_DIR"), "/race_team.rs"));
}

mod whisper_support {
    use anyhow::Result;
    use benilla_profile::Character;
    use benilla_protocol::SessionEvent;
    use crossbeam_channel::Sender;
    use sha2::{Digest, Sha256};
    include!(concat!(env!("OUT_DIR"), "/whisper_support.rs"));
}

#[path = "../../benilla-app/src/net/p2p/whispers.rs"]
mod whispers;
use whisper_support::{character_guid, character_name, chat, emit, MAX_CHAT};

#[path = "../../benilla-app/src/net/p2p/speech.rs"]
mod speech;

mod ui_items {
    use crate::net::ClientCommand;
    use benilla_protocol::messages::{BAG_PLAYER_INVENTORY, SLOT_BAG_FIRST, SLOT_PACK_FIRST};
    use benilla_ui::script::EQUIPMENT_BAG;
    include!(concat!(env!("OUT_DIR"), "/item_positions.rs"));
    include!(concat!(env!("OUT_DIR"), "/equipment_sender.rs"));
}
