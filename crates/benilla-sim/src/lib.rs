// SPDX-License-Identifier: GPL-3.0-or-later
//! Engine-independent simulation primitives. This is a partial port, not a game server.
//! No game assets, database contents, networking, clock, or renderer dependencies.
#![forbid(unsafe_code)]

pub mod xp;

pub mod encounter;

pub mod classic;

pub mod quest;

pub mod classic_targets;

pub mod classic_spell;

/// Bounded source-derived ordinary creature AI and movement-admission contracts.
pub mod creature_ai;

pub mod mage_resource;
