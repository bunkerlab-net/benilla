// SPDX-License-Identifier: GPL-3.0-or-later
//! Engine-independent simulation primitives. This is a partial port, not a game server.
//! No game assets, database contents, networking, clock, or renderer dependencies.
#![forbid(unsafe_code)]

pub mod xp;

pub mod encounter;
