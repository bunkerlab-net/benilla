# Simulation provenance and license boundary

This crate is a deliberately partial server-simulation port for this fork. It is not
part of upstream Benilla's client-only scope. Its GPL-3.0-or-later package license is
explicit and does not inherit the workspace's MIT OR Apache-2.0 default. Existing
Benilla source retains its original notices. A distributed combined derivative that
links this crate must satisfy the GPL's conditions; the combined artifact cannot be
represented as MIT/Apache-only. Private use is not a guarantee about other rights.

## Source

- Project: CMaNGOS Classic, CMaNGOS Project contributors
- Repository: https://github.com/cmangos/mangos-classic
- Commit: `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`
- File: `src/game/Tools/Formulas.h`, namespace `MaNGOS::XP`
- SHA-256: `ca8b55fd022b0c0191255bc457574ccafce5eeb2f99a447466ba75be4717652c`
- Source grant: GNU GPL version 2, or at the recipient's option any later version
- Rust modifications/translation: 2026, Benilla fork contributors, GPL version 3 or later
- Upstream authorship, copyright notice and GPLv2 text are preserved under `licenses/`
- The applicable GPLv3 text is `licenses/GPL-3.0.txt`

## Implemented surface

`is_trivial`, `quest_green_range`, `gray_level`, `color_code`, `zero_difference`
translate the corresponding integer helpers. `base_gain` reformulates `BaseGain`
using exact integer ratios; `round_ties_even` explicitly defines rounding.

The accepted input domain is level 1..=255. Source uint32 overflow outside that domain
is not reproduced. `color_code` preserves the source's unsigned subtraction at level
1, even though its equal-level result is green. XP eligibility uses `is_trivial`, not
`gray_level` or display color, because those source thresholds are not interchangeable.

Deterministic deviation: rational arithmetic eliminates upstream float32 intermediate
rounding. The fixture `tests/fixtures/rounding_differences.tsv` exhaustively enumerates
integer-rounding differences for level pairs 1..=63, with both results. These are
intentional differences, not claims of exact `Gain` parity. Changing this policy needs
a simulation-rules version change once these primitives are integrated in multiplayer.

## Verification

`tools/generate_xp_fixture.py` checks the pinned source's hash, compiles the original
helper bodies using g++ C++17, no fast math, and FE_TONEAREST, and writes the oracle
fixture. It does not use the Rust implementation. Tests compare all 3,969 level pairs,
assert exact integer helper parity, bound float32 approximation error, and check every
rounded-value deviation against the committed list. Unit tests exercise all nonzero
byte-sized level pairs, zero rejection, caps, and tie rounding. Cross-platform end-to-end
replicated simulation has not been tested by these formula tests.

## Exclusions

No `XP::Gain` creature filtering, elite/damage/rate modifiers, group allocation,
rested XP, level-up tables, quests, production AI/combat, or durable persistence is provided.
`encounter` is an original synthetic headless harness using these XP primitives; its
fixed damage and simple killer-only rewards are not translations of CMaNGOS combat.
See `ENCOUNTER.md` for its deliberately narrow deterministic contract.
No Classic-DB rows, Playerbots code, client assets, FrameXML, or game-install files are
included. This crate includes algorithmic source and numeric test outputs only.
