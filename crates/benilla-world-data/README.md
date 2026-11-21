# Local world-data boundary

This crate validates an explicit local JSON model. It is **not a SQL importer, a
CMaNGOS port, an NPC runtime, or a complete playable world**. It does not fetch,
execute, or redistribute source data. The original fixture contains no Blizzard
assets, quest text, or database rows. Its names, IDs, numbers and coordinates were
authored for these tests; display IDs are zero placeholders.

## Use

```sh
cargo run -p benilla-world-data --bin benilla-world-data-check -- \
  crates/benilla-world-data/tests/fixtures/original-camp.json
cargo test -p benilla-world-data
```

`load_json(bytes, Limits::default())` and `load_reader(reader, limits)` return an
immutable `WorldData`. Lookup methods return borrowed rows. `export()` exposes a
read-only, sorted normalized model; `content_digest()` returns its SHA-256.

The JSON shape is defined by `src/model.rs`, with a complete original example in
`tests/fixtures/original-camp.json`. `CAPABILITIES.json` is a machine-readable
scope manifest. All fields are required; unknown fields, duplicate IDs, dangling
references, unsupported source revisions, and nonempty `unsupported_features`
are errors. The schema has no operational SQL, script, download or credential fields.
Imported strings are inert text, not rendered/executed here; a downstream UI
must also treat them as plain text. Serde also accepts positional arrays for
record structs; validation is identical and canonical output uses objects.

Hard bounds are 8 MiB, 100,000 combined top-level rows, 16 KiB per multiline text
and 256 bytes per label. Callers can lower bounds. The bounded reader consumes at
most the byte limit plus one byte. Deserialization precedes row/string checks,
so these are bounded-input limits, not a claim of constant memory use. The JSON
parser's recursion limit remains enabled. Error messages contain field names and
IDs, not the imported text or private paths.

## Exact represented attributes

- Creature: numeric entry, name, display ID, faction-template ID, one level,
  health, armor, fixed melee damage/period/reach and passive/stationary behavior
- Spawn: stable numeric ID, creature reference, map ID, three integer coordinates,
  integer orientation and one fixed respawn interval
- Quest: entry/title/details, minimum/maximum levels, one to four ordered creature
  kill objectives, fixed XP/money and up to four plain item rewards
- Relation: creature/quest references and start or finish role; each quest must
  have both roles
- Item: entry/name/display ID and stack bound; no stats, equipment or use effect
- Ability: entry/name, one direct damage or heal amount, mana cost, cast time,
  cooldown/global cooldown and minimum/maximum range

Coordinates/reach/range are thousandths of a WoW world unit, **not millimetres**.
Angles are milliradians in 0..=6283. Quantization is a deliberate model choice,
not evidence of Classic numeric parity. Durations are milliseconds. The runtime
must define tick rounding before executing them. This crate does not check
whether map/display/faction IDs exist in the player's install.

## Provenance and compatibility

`original_fixture` records an authored fixture revision. `local_cmangos_classic`
requires the exact reviewed core/database commit IDs and build 5875 listed in
`CAPABILITIES.json`. It records a lower-case SHA-256 schema fingerprint for audit.
These metadata are supplied by the producer, **not authenticated or proof of
rights**. A matching hash or revision string cannot make unreviewed data safe,
correct, compatible, or licensed. The source variant identifies intended source
provenance; it does not assert a working converter or gameplay fidelity.

The local SQL adapter remains a separate task. It must inspect and explicitly
report unsupported mechanics; it must not project complex rows into this small
model while silently dropping their meaning. Many Classic rows cannot be
represented, including level-dependent quest XP, variable creature stats, AI
scripts, faction rules, or multi-effect spells. Fixed damage/reward models in the
fixture do not establish CMaNGOS parity.

V1 hashes include source metadata, all records and ordered nested objective/reward
slots. Top-level tables and relations are sorted by keys before hashing; JSON
whitespace and row order do not matter. Integers avoid floating-point encoding
ambiguity. The hash domain is `benilla-world-data-v1` followed by a NUL byte and
compact serde JSON. There is no claim of compatibility with arbitrary external
JSON canonicalization standards. A field or normalization change requires a new
format version. Local assets need a separate content compatibility manifest.

GPL-3.0-or-later applies to this original source. No external database content is
included or granted a license by this crate. See the repository distribution
notice and the separate [import design](../../docs/CLASSIC-WORLD-IMPORT.md).
