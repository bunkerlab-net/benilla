# Exact pinned defense-skill source excerpts

The `.inc` files preserve unchanged complete method bodies and original copyright
notices from CMaNGOS Classic revision
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`:

- `Player.cpp.inc`: `src/game/Entities/Player.cpp`, `UpdateCombatSkills`,
  `UpdateDefense`, `UpdateSkill`
- `Util.h.inc`: `src/shared/Util/Util.h`, `roll_chance_f`

CMaNGOS Project contributors hold the upstream authorship, GPL-2.0-or-later.
The combined oracle and Rust translation are GPL-3.0-or-later; original notices
and license texts remain under [../../licenses](../../licenses).
The generator checks these excerpts' SHA-256 values, and with an optional complete
pinned checkout also verifies the full source hashes and byte-identical extraction.
Git CRLF is canonicalized to LF; all other source changes fail closed.

The surrounding C++ harness supplies synthetic state, records the source's final
float, and explicitly supplies semantic random draws. It does not replace these
methods with another translation. No game assets, database records, or runtime
content admission are included.
