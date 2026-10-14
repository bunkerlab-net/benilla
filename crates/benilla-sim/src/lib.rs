// SPDX-License-Identifier: GPL-3.0-or-later
//! Engine-independent simulation primitives. This is a partial port, not a game server.
//! No game assets, database contents, networking, clock, or renderer dependencies.
#![forbid(unsafe_code)]

pub mod xp;

pub mod encounter;

pub mod classic;

pub mod quest;
pub mod quest_lifecycle;

/// Bounded source-derived ordinary creature initialization from explicit local rows.
pub mod creature_init;

pub mod classic_targets;

pub mod classic_spell;

/// Bounded source-derived ordinary creature AI and movement-admission contracts.
pub mod creature_ai;

pub mod mage_resource;

pub mod starter_init;

pub mod npc_melee;

/// Bounded source-derived player death and corpse lifecycle.
pub mod player_death;

/// Bounded source-derived player XP and level transitions with an audited host.
pub mod player_progression;
