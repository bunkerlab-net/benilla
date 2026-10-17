// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f,
// Creature.h:403-409,643-646 and Object.h:701-737,763.
// Copyright CMaNGOS contributors; upstream GPL-2.0-or-later.
//! Source creature movement capabilities from explicit habitat and current flags.
//! This predicate does not initialize flags, infer terrain, or admit movement.

pub const POLICY: &str = "cmangos-8ec338a1-creature-capabilities-v1";
/// LF-normalized Creature.h, Object.h and ObjectMgr.cpp, respectively.
pub const SOURCE_SHA256: [&str; 3] = [
    "a92b0e95e5bf7827389fcca0acc0edaba48a1a083efbf5a01a752a4d171664fc",
    "259cdb1a061258d6a8c3e977c743c8c83848c763353b504ca3e617580f58c128",
    "dd27915ca363f81885d20783fc4f6c0392df20c4e3dfa031faa1a30df7a4b78b",
];

pub const MOVEFLAG_WALK_MODE: u32 = 0x0000_0100;
pub const MOVEFLAG_LEVITATING: u32 = 0x0000_0400;
pub const MOVEFLAG_FLYING: u32 = 0x0000_0800;
pub const MOVEFLAG_SWIMMING: u32 = 0x0020_0000;
pub const MOVEFLAG_CAN_FLY: u32 = 0x0080_0000;
pub const MOVEFLAG_HOVER: u32 = 0x4000_0000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidInhabitType(u32),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInhabitType(raw) => {
                write!(f, "source creature InhabitType outside 1..=7: {raw}")
            }
        }
    }
}

impl std::error::Error for Error {}

/// Validated raw source habitat. Invalid database values reject here rather than
/// applying ObjectMgr.cpp:520-524's load-time repair to INHABIT_ANYWHERE (7).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(transparent)]
pub struct InhabitType(u32);

impl InhabitType {
    pub const fn try_from_raw(raw: u32) -> Result<Self, Error> {
        if raw == 0 || raw > 7 {
            Err(Error::InvalidInhabitType(raw))
        } else {
            Ok(Self(raw))
        }
    }

    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// Only the source predicate can construct these derived capabilities.
/// Serialization is for evidence; deserialization cannot grant capabilities.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct MovementCapabilities {
    can_walk: bool,
    can_swim: bool,
    can_fly: bool,
}

impl MovementCapabilities {
    pub const fn can_walk(self) -> bool {
        self.can_walk
    }

    pub const fn can_swim(self) -> bool {
        self.can_swim
    }

    pub const fn can_fly(self) -> bool {
        self.can_fly
    }
}

/// Creature::CanWalk/CanSwim/CanFly for every valid source habitat and every
/// current u32 movement-flags word. Unknown/unrelated bits are ignored exactly
/// as in source. This general predicate is broader than runtime admission:
/// the runtime must derive and own these inputs within its separate profile.
/// WALK_MODE, SWIMMING and FLYING alone do not confer flight capability.
pub const fn evaluate(inhabit_type: InhabitType, movement_flags: u32) -> MovementCapabilities {
    MovementCapabilities {
        can_walk: inhabit_type.raw() & 1 != 0,
        can_swim: inhabit_type.raw() & 2 != 0,
        can_fly: inhabit_type.raw() & 4 != 0
            || movement_flags & (MOVEFLAG_LEVITATING | MOVEFLAG_HOVER | MOVEFLAG_CAN_FLY) != 0,
    }
}
