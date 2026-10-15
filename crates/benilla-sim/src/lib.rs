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
pub mod creature_regeneration;

pub mod classic_targets;

pub mod classic_spell;

/// Bounded source-derived ordinary creature AI and movement-admission contracts.
pub mod creature_ai;

pub mod mage_resource;

pub mod starter_init;

pub mod npc_melee;

/// Bounded source-derived player death and corpse lifecycle.
pub mod player_death;

/// Source-derived strongest-negative player speed rates and movement-mode selection.
pub mod player_slow;

/// Bounded source-derived player XP and level transitions with an audited host.
pub mod player_progression;

/// Source-derived ordinary solo corpse loot generation.
pub mod loot;

/// Bounded source-derived ordinary backpack storage.
pub mod inventory;

/// Source-native reputation initialization, flags, gains and ordered spillover.
pub mod reputation;

/// Source-derived NPC chase/home spline motion with explicit spatial evidence.
pub mod creature_motion;

/// Pinned portable current-pose bearing and inclusive arc arithmetic.
pub mod facing;

/// Source-derived per-Unit event ordering with bounded transactional dispatch.
pub mod event_processor;

/// Exact bounded SpellEvent callback scheduling decisions; host lifecycle is separate.
pub mod spell_event;

/// Portable ordinary explicit-unit spell range and release tolerance.
pub mod spell_range;
