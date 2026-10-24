# Bounded Classic solo-creature kill experience

`player_progression::kill_xp` ports the pinned CMaNGOS Classic
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f` ordinary creature `XP::Gain`,
victim-aware `Player::GiveXP`, `GetXPRestBonus`, `SetRestBonus`, and
`ComputeRest` arithmetic. Its policy is
`cmangos-8ec338a1-solo-creature-kill-xp-softfloat-v1`.

This supports positive XP from ordinary starter creatures. It does not label a
zero-XP-only fixture as ordinary kill support. The production `award` uses the
[existing progression reducer](PLAYER-PROGRESSION.md) for every actual level
transition; it does not duplicate or approximate `GiveLevel`, synthesize level
rows, or install a production no-op host. The native compiler, actual capture,
world adapter, source rest clock, persistence transaction, and capability gate
remain separate required integration work. No real database rows or game assets
are bundled and no native capability is enabled by this module.

## Source inputs and bounded admission

`Victim` holds the actual creature GUID, entry, selected current level, template
`ExperienceMultiplier` binary32 bits, current maximum health, and source
`m_damageByOthers`. Its flags must resolve the source methods at death, including
actual dynamic `CreatureStaticFlags::NO_XP`, creature type CRITTER, `IsPet`,
`IsTotem`, `IsElite`, and player-controlled status. In this scope, pets, totems,
elites, and player-controlled targets reject. Source `IsElite` is false for
normal and rare ranks; this must not be guessed from the display name. Critters
and NO_XP targets produce zero XP and remain distinct from unsupported admission.
`Rules` requires the actual selected Rate.XP.Kill float bits; no rate defaults
are inferred. The current progression scope is solo mage levels one through nine.
The standalone exact BaseGain/Gain arithmetic admits byte-sized levels 1–255.

`KillContext` binds an owner-generated death event and the source tap selection:
original loot recipient if present, otherwise the responsible player (killer's
beneficiary). It must equal the local player and group loot must be proven
absent. The owner derives a kill from the real staged combat health transition;
a client cannot submit a victim-shaped DTO to request XP. The owner deduplicates
that death identity atomically with rewards. Neither a GUID nor a hash is proof
that the kill happened. Absence of other players does not justify omitting the
source damage ledger; zero damage-by-others needs a proven solo combat envelope.
The source applies no extra tap-damage percentage eligibility cutoff here.

`RewardSinglePlayerAtKill` calls honor, then reputation, Gain/GiveXP, pet XP,
and finally KilledMonster. The admitted no-effect branches require:

- `IsCivilianForTarget` and `IsRacialLeader` both proven false. The former is a
  source relation result: it depends on PvP, civilian flags, triviality, and enemy
  relation. For example an actual non-civilian flag proves that branch false;
  arbitrary false defaults do not. The Honorless Target aura is not required
  when both ordinary-creature branches are already excluded
- Actual `IsNoReputation` true, or a complete selected
  `creature_onkill_reputation` lookup proving no row for this exact entry.
  Unknown lookups and present rows reject. A supplied empty list without the
  complete query and source identity is not absence evidence
- No pet or controlled units under the progression audit

A NO_XP flag does not prove either honor or reputation absence. This module
rejects unsupported honor/reputation consequences before changing staged state;
it never acknowledges a side effect without implementing it. The mandatory
`KillHost::preflight_kill` must recheck the real native rows, flags, tap, death
identity, and rest contract on every call. It is not a certification marker.
Actual creature reputation capture is still required for a production adapter;
synthetic test absence is not native source evidence.

After `award` succeeds, the owner applies ordinary entry and KillCredit alias
quest credit in source order, even if XP was zero, the player was dead, or the
player was capped. Kill credit is not emitted by this module because its real
quest reducer is owned by the adapter. Both operations, combat death, spell and
stat host changes, deduplication, inventory/loot state, and event outbox must
commit in one transaction. Publishing a packet before that commit is forbidden.

## Exact numeric path

The module uses pinned `rustc_apfloat` software binary32 and nearest-even
rounding. Only exact integer threshold helpers from `xp.rs` are reused; its
rational BaseGain approximation is never called. Native host floats do not
participate in authoritative calculations.

The source Gain path is unusually sensitive to conversions:

1. BaseGain evaluates each original binary32 operation, including 0.05f times
   the capped higher-level difference, or lower-level division and subtraction
2. Multiply by the actual ExperienceMultiplier
3. Convert to uint32 on entry to GetModifierXpBasedOnDamageReceived, truncating
   before that helper even when other-player damage is zero
4. When damage-by-others is positive, convert it and max health to binary32,
   divide, zero XP at a proportion at least one, or multiply by one minus the
   fraction and truncate back to uint32
5. Convert that uint32 back to binary32, multiply by the actual kill rate, then
   nearbyint using FE_TONEAREST and convert to uint32

For example level one against level two at multiplier/rate one yields 52 XP:
BaseGain's 52.5 is truncated by the helper parameter. The final nearbyint is not
applied to the original BaseGain. Conversion overflow, NaN/infinity, subnormal
inputs/results that raise exceptions, and negative rates/multipliers reject
rather than choose a platform-dependent C++ conversion. Both signed zeros are
accepted. Inexact finite rounding is retained; all out-of-range float-to-integer
conversions are rejected before committing any effects.

Victim-aware GiveXP preserves zero/dead/cap early returns. Accepted grants run
GetXPRestBonus/SetRestBonus before the packet, then perform source wrapping
uint32 addition of old XP, base XP, and rested bonus before any threshold test.
It repeatedly calls the same progression level reducer, keeping old PLAYER_XP
visible during every intermediate callback, and writes the final remainder once.
Cap surplus is retained. The kill packet is GUID, wrapping total XP, type byte
zero, base XP, and the solo group rate float 1.0 bits. It is never projected as
non-kill XP or awarded twice from an event.

`SetRestBonus` updates internal float bonus first, sets the rest byte to RESTED
above ten or NORMAL at or below one (preserving it between those thresholds),
then writes the uint32 rest-experience field. `Observation` snapshots preserve
that field ordering, including the old experience during the earlier byte write.
Both rest and progression callbacks execute only on a deep in-memory host clone;
any callback failure discards the complete staged transaction.

## Explicit rest and clock boundary

The public production award currently requires `RestState::fresh_player()` and
an explicit `OnlineRestContract`. This matches a newly created player with zero
rest who has remained online outside resting areas. Unknown or present offline
intervals or entered rest areas reject. The caller must select and bind the rule
`fresh-online-simulation-continuation-no-offline-rest-v1`, represented by
`OnlineRestRule::SimulationContinuationWithoutSourceLogout`. There is no default.

This is an authored P2P pause/continue rule: saving and reopening resumes the
same online simulation without performing source logout/login. It permits
checkpoint continuation without inventing elapsed wall time. It is not CMaNGOS
offline-rest parity. The adapter must not apply it automatically to an imported
realm character, overwrite positive earned rest, reinterpret an actual offline
interval as absent, or silently coerce an unsupported saved character to zero
rest. A save with unsupported rest remains intact until a compatible clock
adapter or an explicitly selected supported world contract can continue it.

The numerical `consume_rest`, `set_rest_bonus`, and `compute_rest_bits` helpers
support and source-test positive rest. They are not an area/clock adapter.
ComputeRest requires authoritative nonnegative signed-time_t seconds, actual
next-level XP, and explicit in-game/offline-in-rest-place/offline-wilderness rate
bits. It preserves source uint32-to-float and time_t-to-float conversions and
operation order, including wilderness rate divided by four before multiplication.
SetRestBonus clamps negative input and caps to next XP times 1.5f divided by 2.
A full source-compatible adapter must additionally implement and persist:

- Source resting flags, time_inn_enter, area/city/inn transitions, actual epoch
  and time_t policy, and any associated rest-type PvP changes
- Player::Update accumulation only while resting, inn-enter greater than zero,
  and elapsed inn time at least ten seconds, then UpdateInnerTime(now)
- Offline accumulation once for positive logout interval after InitStatsForLevel,
  using the recorded rest-place status and actual configured rate
- Atomic save/restore of bonus bits, rest byte, rest-experience field and clock;
  no inferred timestamps or duplicated offline interval on replay

## Evidence, licensing, and verification

CMaNGOS Project contributors' pinned source is GPL-2.0-or-later. This Rust
translation, synthetic harness and generator are GPL-3.0-or-later, 2026 Benilla
fork contributors, without warranty. The [upstream authors](licenses/AUTHORS.md),
[copyright](licenses/COPYRIGHT.md), [source grant](licenses/LICENSE),
[GPLv3](licenses/GPL-3.0.txt), and [combined-artifact provenance](PROVENANCE.md)
apply. Source links are pinned, not moving main-branch assumptions:

- [Formulas.h](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Tools/Formulas.h): BaseGain 321 and Gain 339
- [Player.cpp](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Player.cpp): resting update 1470, SendLogXPGain 2654, GiveXP 2668, RewardReputation 6384, RewardHonor 6627, GetXPRestBonus 7765, load rest 14440, SetRestBonus 17010, RewardSinglePlayerAtKill 18804, SetRestType 19936, ComputeRest 20079
- [Unit.cpp](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Unit.cpp): damage attribution 986, tap/reward selection 1011, GetModifierXpBasedOnDamageReceived 12072
- [Relations.cpp](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Relations.cpp): IsCivilianForTarget 692

The generator checks whole-source SHA-256 and extracts unchanged BaseGain/Gain,
trivial/zero-difference helpers, damage-based XP helper, GiveXP, packet writer,
rest consumption/setter, and rest computation. It compiles C++17 with
FE_TONEAREST, no fast math or contraction, warnings as errors and undefined/
float-cast-overflow sanitizers. Its GiveLevel is an explicit tracing adapter;
the [separate progression oracle](PLAYER-PROGRESSION.md) checks the complete
unchanged GiveLevel body. This compositional boundary is not hidden inside a
claim that this harness reimplements every native dependency.

The committed fixture has 29,995 synthetic rows: 26,649 exact BaseGain/Gain
rows, 2,594 victim-aware GiveXP/rest state/packet/ordered trace rows, 576
ComputeRest rows, and 176 SetRestBonus rows. Other tests require positive XP and
an actual level transition, reject missing/changed source proofs, exercise
source zero-XP and clock admission, require serde's explicit rest rule, and
inject failure after every callback in a multilevel kill transaction.

Reproduce without game data:

```
python3 crates/benilla-sim/tools/generate_kill_xp_fixture.py /path/to/pinned/sources --check
cargo test -p benilla-sim --lib kill_xp
cargo test -p benilla-sim --release --lib kill_xp
cargo test -p benilla-sim --test player_progression
cargo clippy -p benilla-sim --lib --tests -- -D warnings
```

Linux amd64 is the verified host. These are headless component tests, not a
Windows/macOS build, real source capture, graphical run, full workspace gate,
native compiler admission, or end-to-end proof of a playable Classic starter.
