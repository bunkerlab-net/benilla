# Exact-source Player terrain-environment reducer

This independent oracle executes CMaNGOS Classic
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f` environment state and 100 ms gate logic
against **authored bound liquid facts**. It proves reducer/timer results and the
ordered dependency calls made by that source for the declared inputs. It does
not prove geometry, a native geometry feed, production admission, gameplay
integration, or the implementation of those dependencies.

The harness and generator are GPL-3.0-or-later. Extracted CMaNGOS code remains
GPL-2.0-or-later under its original notice, which is retained in the temporary
translation unit. The combined source oracle is GPL-3.0-or-later. See the root
[GPL license](../../../LICENSE-GPL-3.0) and existing
[CMaNGOS provenance](../../benilla-sim/PROVENANCE.md).

## Run

From the repository root, with the complete pinned source checkout available:

```sh
python3 crates/benilla-world-runtime/tools/generate_environment_state_fixture.py /path/to/mangos-classic
python3 crates/benilla-world-runtime/tools/generate_environment_state_fixture.py /path/to/mangos-classic --check
python3 crates/benilla-world-runtime/tools/generate_environment_state_fixture.py /path/to/mangos-classic --check --sanitize
python3 crates/benilla-world-runtime/tools/test_environment_state_oracle.py /path/to/mangos-classic
```

A compiler is required; its absence fails rather than skips. The tested profile
is little-endian IEEE binary32 with C++17, no fast-math or FP contraction, and
no strict-aliasing optimization. `--sanitize` enables undefined-behavior and
float-to-integer-cast checks with no recovery. `CXX` may select the compiler.
The program and all temporary binary products use `.test-fixture` suffixes.
Generation and checking do not read an installation, use the network, run the
native input compiler, build a player, or alter the pinned source checkout.

## Source boundary

[The source hash manifest](environment_state_source_hashes.sha256) guards all
ten consumed upstream files byte-for-byte before extraction or compilation.
No reducer or lookup answer is reimplemented in Python or authored C++.
[The generator](generate_environment_state_fixture.py) extracts:

- Complete `Player::SetEnvironmentFlags`, `Player.cpp:1105–1140`
- Complete `Player::UpdateTerainEnvironmentFlags`, `Player.cpp:19113–19175`
- The exact position-status countdown block from `Player::Update`,
  `Player.cpp:1483–1489`
- The exact post-relocation timer gate, reset to 100, and terrain-environment
  call from `Player::SetPosition`, within `Player.cpp:6015–6078`
- Original `EnvironmentFlags`, mirror-timer type enum, aura interrupt and
  triggered-cast flag enums, `LiquidTypeEntry`, and `LiquidTypefmt`

The full original `GridMapDefines.h`, `DBCStore.h`, `DBCFileLoader.h`,
`DBCFileLoader.cpp`, and its `ByteConverter.h` dependency are compiled through
their normal includes. A small platform header only supplies fixed-width
integer aliases and the tested endianness constants. DBC row indexing,
absence, row-zero presence, and full 32-bit ID lookup execute the original
loader and storage implementation, including original `LookupEntry`.

The wrapper around the source `SetPosition` slice returns true after that
slice. Coordinate validation, relocation, movement/turning aura interruption,
group/trade/zone work, outdoor/exploration checks, and the other portions of
`Player::Update` are deliberately outside the boundary. The TSV `pose` labels
identify the caller situation; they do not execute or prove those preceding
operations. The extracted gate is independent of displacement/orientation:
calling it after the timer expires can refresh even when the body pose is
unchanged. Timer expiry alone never calls the terrain reducer.

## Explicit dependency inputs and intent audit

The harness's `TerrainInfo::getLiquidStatus` adapter returns the authored
status and complete liquid-data tuple. It checks that the original reducer
passes `MAP_ALL_LIQUIDS` and the supplied collision height, and counts calls.
It leaves liquid output untouched on `NoWater`, as the source reducer does not
read the tuple on that branch. This is a reducer oracle, **not** evidence that
any terrain query, native reader, loaded snapshot, or movement binding produced
those facts. Those mechanisms need their own evidence.

`HasAura` and `GetWaterBreathingInterval` return explicit per-step dependency
answers and record that they were queried. The real aura registry, water
breathing configuration/multiplier conversion, spells, and mirror timers are
not substituted with purported implementations. The latter getter is invoked
only where source short-circuit evaluation actually reaches it.

Threat refresh, interrupt-aura removal, spell-aura removal, self-casting, and
mirror scale setters only record dependency **intents** with their exact
arguments and order. No callback updates a simulated aura registry or mirror
state. In particular, an authored `HasAura` answer can remain true after a
same-spell-ID removal intent; this records the original reducer's control flow
for that supplied answer and does not claim realistic callback feedback or
state consistency. Cast success, aura application/removal, threat results,
mirror duration/ticking/packets, breath loss, fatigue, and environmental damage
remain unproven.

## Products and schema

- [environment-state.tsv](fixtures/environment-state.tsv): 1,324 steps across
  1,261 independently initialized cases; 1,263 position-gate calls, 61 timer
  updates, and 1,259 actual reducer queries
- [environment-state-dbc.tsv](fixtures/environment-state-dbc.tsv): 20 source
  lookup observations including row zero, ordinary absence, sparse high IDs,
  and all four original `LiquidTypeEntry` fields
- [store 0](fixtures/environment-state/store0/LiquidType.dbc.test-fixture) and
  [store 1](fixtures/environment-state/store1/LiquidType.dbc.test-fixture): two
  complete authored WDBC files, with unsorted four-word rows, exact headers,
  and a one-byte NUL string table. Store 0 has a spell-bearing row zero;
  store 1 genuinely lacks it

All binary inputs are numeric test data, not game assets or copied game rows.
Their `.test-fixture` suffix is intentional. Packaging must retain these exact
source fixtures while continuing to reject ordinary installation `.dbc` files;
this oracle does not relax the source archive's asset policy.

The main TSV has 22 fields, in this order:

1. `case`, `step`, `store`, `phase`, `pose`
2. `diff_ms`, `status`, `entry`, `type_flags`
3. `level_bits`, `depth_bits`, `collision_bits`
4. `breathing_interval`, `has_aura`
5. `before_flags`, `before_timer_ms`, `before_last_entry`
6. `after_flags`, `after_timer_ms`, `after_last_entry`
7. `query_count`, `callbacks`

Float words are hexadecimal binary32. Other numbers are decimal. `-` denotes
a null last-liquid pointer or no callbacks. `phase` is `position` or `update`.
Every step records its before/after state; consecutive steps in a case preserve
that state, including the original source pointer to the loaded last row.
`query_count` counts only this step's authored terrain dependency invocation.

Callbacks are a comma-separated ordered sequence:

- `threat`
- `interrupt:<mask>`
- `fatigue:<scale>`, `breath:<scale>`, `environmental:<scale>`
- `interval:<returned_interval>`
- `has_aura:<spell_id>:<returned_boolean>`
- `remove_spell:<spell_id>`
- `cast_spell:<spell_id>:<trigger_flags>`; the harness also verifies self-target

## Exercised source distinctions

Sequences cover first refresh, 99/100 ms boundaries, several position calls
within one interval, unchanged/turning caller labels, stationary expiry,
water→magma→slime→dry, repeated dry, and retained last-liquid state. The complete
raw source environment byte is exercised for dry clearing, including preservation
of the unused high bit. The Rust-facing profile may reject unsupported state
or facts separately; a rejection must not be presented as a CMaNGOS outcome.

The authored matrix exercises all five source liquid statuses, ordinary and
mixed type masks, deep-water/WMO bits, spell/no-spell/missing/zero/high-ID rows,
zero/nonzero breathing intervals, and present/absent aura answers. Both zero-row
stores are used in sequences. An explicit initial row-zero pointer separately
shows the difference between real DBC lookup and the reducer's `entry != 0`
gate. A row with `SpellId == 0` can survive a subsequent entry-zero fact;
a prior spell-bearing row is cleared. Replacement by another entry sharing the
same spell still requests old-spell removal before `HasAura`.

Selective type checks preserve old water/magma/slime bits whose types are not
queried. A dry aggregate clear uses the requested mask for callbacks, even if
only one of its bits was previously present. Callback order comes directly from
the original methods, not from a sorted event set.

Seventy-two float profiles surround the strict high-liquid threshold. The
unsuffixed original `0.75` promotes collision multiplication and the depth sum
to double precision. The corpus contains cases where a binary32-only expression
would disagree. Timer profiles include zero, 1, 99, 100, and `u32::MAX`, with
zero, boundary, and maximal differences.

Malformed WDBC, malformed/unsupported liquid facts, invalid bindings, missing
native geometry, stale replies, and atomic rollback are consumer contract tests;
they are not run through an unsafe source parser or mislabeled as source
results. [The oracle source checks](test_environment_state_oracle.py) verify
verbatim extraction, rejection of a changed byte in every dependency before
compilation, complete WDBC consistency with original-reader observations,
sequence continuity, and a meaningful double-versus-float threshold distinction.
