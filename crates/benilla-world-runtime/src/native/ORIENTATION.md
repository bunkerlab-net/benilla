# Native player pose boundary

Native format 33 binds an explicit `PlayerProfile.orientation` into the profile
identity. `State.orientation`, checkpoints, journals and state hashes retain it.
There is no default inferred from the selected exploration character on restore.
Prior format-8 state/records require a separate explicit migration; they cannot
silently acquire an orientation. OriginalFixture v2 is unchanged.

`Orientation` normalizes finite binary32 inputs using the mechanism in pinned
CMaNGOS classic commit
[8ec338a1704e7dcb1c0213eb7ed58f9231ade40f](https://github.com/cmangos/mangos-classic/tree/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f),
[MapManager.h:121–132](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Maps/MapManager.h#L121).
Positive whole turns become zero; negative whole turns retain binary32 `2*pi`.
The wire representation is exact unsigned bits in `[+0, 2*pi]`. Signed zero is
canonicalized to +0 as an explicit local encoding rule. Nonfinite input and
noncanonical stored bits are rejected, rather than renormalized during decode.
This small normalization port is GPL-3.0-or-later under the upstream GPL-2.0-or-later
option. CMaNGOS Project contributors retain their attribution and license grant;
upstream notices are preserved in the simulation crate's
[AUTHORS](../../../benilla-sim/licenses/AUTHORS.md),
[COPYRIGHT](../../../benilla-sim/licenses/COPYRIGHT.md) and
[license text](../../../benilla-sim/licenses/LICENSE).
No original game data is included.

`Command::Pose` sequences position and orientation together. Changed position
requires the existing trusted movement authority to accept the whole pose;
blocked movement commits neither field. The old position-only `Command::Move`
preserves current orientation. An unchanged position is a pure turn: it makes no
movement callback, creates no path segment, consumes no extra elapsed movement
budget, does not mark the player moving, and does not interrupt casts through a
fabricated position change. It remains a bounded, authenticated, sequenced input.
Dead players cannot turn through this command. A frame error rolls all inputs back.

The separation between location and orientation follows
[Object.cpp:1198–1225](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Object.cpp#L1198).
`Query::Spell.player_orientation` is the exact current pose at prepare, release or
impact, including earlier inputs in that same tick. It is only a bound witness.
The pose mechanism alone does not implement facing predicates. The separately
selected [facing authority](../../FACING-AUTHORITY.md) derives source `HasInArc`
checks from executing motion/combat/spell requests. Rear melee and daze,
turn-rate limits, transport-local motion and missing movement modes remain
unsupported; existing explicit exclusions stay in force. Production `NativeCapability` is
still uninhabited, and compiler/admission producers are not changed.

`Event::PlayerPose` carries both fields at that input boundary, so projection does
not substitute final-frame orientation for an earlier move or turn. Initial
projection restores the checkpoint pose. The actual app controller queues pose
without mutating selection state and reports only its last delivered projection.
Every pose-bearing tick saves before publishing. A failed save retains the computed
transaction and ordered outbox, while getters and corrections remain at the prior
published pose. Retry saves the exact transaction and publishes once, without
reexecuting its input, cast, RNG or grants.

Authored tests exercise source normalization boundaries, strict decoding, explicit
genesis identity, turn/cast ordering and phase witnesses, movement budget invariance,
blocked/dead/input-error rollback, failed-save/retry/reopen and byte-exact replay.
They establish this transaction contract, not playable Classic or facing parity.
