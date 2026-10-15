# Pinned ordinary-unit spell range oracle

This is numeric, headless evidence for the existing Fireball 133 ordinary-NPC
explicit-unit target domain. It is not a full `CheckCast`, LOS, collision,
movement, server, database or playable-world oracle. Inputs are authored probes;
no WoW install rows or third-party world database data are bundled.

## Source and bounded host

Pinned CMaNGOS Classic revision:
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`.

The generator verifies every full-file SHA-256 in
[spell_range_source_hashes.sha256](spell_range_source_hashes.sha256), allowing
only CRLF-to-LF checkout normalization, then inserts complete unchanged bodies:

- `Spell.cpp:6157–6280`: `Spell::GetMinMaxRange` and `Spell::CheckRange`
- `Object.cpp:1252–1281`: `WorldObject::GetDistance(const WorldObject*, bool, DistanceCalculation)`
- `SpellMgr.h`: `GetSpellMinRange`, `GetSpellMaxRange`, `IsNextMeleeSwingSpell`
- `Player.h`: the entire `Player::ApplySpellMod` template and `GetSpellModOwner`
- Actual position/type/reach/attribute getters, `SpellRangeEntry`, enums and the
  source `MELEE_LEEWAY` declaration needed by these bodies

All 15 files are guarded before compiler invocation. Extraction preserves
unsupported branches, source arithmetic and original copyright/license notices.
The upstream code is GPL-2.0-or-later; the authored harness/tools are
GPL-3.0-or-later, consistent with this derived crate.

The authored host supplies a player caster, distinct ordinary NPC unit target,
existing range row with zero flags, nonzero cast time, explicit unit target mask,
no next-swing/ranged-slot attributes, zero facing metadata, and empty range
modifier collections. The actual player modifier-owner getter returns the
player; the actual empty-collection `ApplySpellMod` body executes and performs no
assignment. Both units move forward without walking, deliberately exercising
the fact that a non-melee ordinary NPC target receives no moving-player leeway.
Positions, combat-reach storage, spell/range lookup, empty collections and neutral
metadata are bounded host inputs, not whole-server implementations. Unsupported
services fail loudly rather than silently returning a plausible result.

Each generation also verifies rejection before source execution for melee,
ranged, next-swing, ranged-slot, instant-release, player/self/corpse/gameobject
and source/destination-location targets, facing flags, nonempty modifiers and
missing range rows. These are exclusions, not parity claims. Other spell IDs,
channels, pets, area targets, modified weapons and modifier effects are not
admitted by this work.

## Corpus and observations

[fixtures/spell-range.tsv](fixtures/spell-range.tsv) contains 2,418 rows, each with
18 tab-separated fields, in this exact order:

1. Case name
2. Strict flag, `1` for begin, `0` for non-strict release
3. Origin x/y/z bits (three fields)
4. Target x/y/z bits (three fields)
5. Row minimum/maximum bits (two fields)
6. Caster/target combat-reach bits (two fields)
7. Observed minimum/effective maximum/squared distance bits (three fields)
8. Source result: `255` success, `89` out of range, `118` too close
9. Source `m_maxRange` before/after bits (two fields)

All float fields use eight lowercase hexadecimal binary32 bits. The maximum
includes release tolerance. To observe the local effective maximum on a failing
cast without altering `CheckRange` or duplicating its formula, the harness makes
a second source call with the same host/row/reaches and a target exactly at the
source minimum. Its successful `m_maxRange` write exposes the value. The original
cast's before/after state is independently preserved: failure leaves the sentinel
unchanged; success writes the effective maximum. The pure Rust numeric helper
reports the computed maximum on either result, and its consumer checks this
source-side-effect distinction explicitly. Native non-channel casts do not
claim ownership of the separate server channel cache.

The corpus covers strict/release minimum and maximum edges, `nextafter` neighbors
on all three axes, zero and nonzero minimum, multiple reaches, tolerance below and
at/above its three-unit cap, signed zero, bounded extremes, gradual underflow,
source operation ordering, and 512 deterministic nontrivial triples. A named
probe distinguishes squared comparison from rounded square root; other rows
distinguish reassociated reach/distance sums and fused products. The source uses
`dx*dx + dy*dy` followed by adding `dz*dz`; it does not take a square root for
`DIST_CALC_NONE`. Maximum rejection precedes minimum rejection; both endpoints
are inclusive.

The raw helper admits finite coordinates of magnitude at most 32760, row ranges
from zero through 1,000,000 with minimum no greater than maximum, and reaches from
zero through binary32(1e12). These bounds preclude overflow. Raw-helper probes may
contain finite subnormal inputs. The existing native typed APIs are narrower:
their scalar inputs remain normal-or-zero, while valid normal inputs can still
produce subnormal intermediate differences/products/tolerance. Those defined
intermediates are retained with gradual underflow, not flushed or rejected.

[fixtures/spell-range-excluded.tsv](fixtures/spell-range-excluded.tsv) separately
records 12 actual source observations outside these bounds: overflow in
subtraction/squaring/reach addition, overbound finite coordinates and a nonfinite
coordinate. They are not runtime parity/admission fixtures. They expose source
infinity/NaN comparisons instead of pretending that the source saturates or
validates these inputs. NaN sign/payload observations are host-specific; no
cross-architecture NaN-payload guarantee is made. All admitted outputs are finite.

## Reproduce and verify

From the repository root, set `SOURCE` to the pinned CMaNGOS checkout and run:

```sh
python3 crates/benilla-world-runtime/tools/generate_spell_range_fixture.py "$SOURCE" --check
python3 crates/benilla-world-runtime/tools/generate_spell_range_fixture.py "$SOURCE" --check --sanitize
python3 crates/benilla-world-runtime/tools/test_spell_range_oracle.py "$SOURCE"
```

Omit `--check` only to regenerate the committed fixtures. `CXX` selects the
compiler (default `g++`); a missing compiler or source is a hard error, not a skip.
No full-server/application build is required. Output comparison normalizes only
CRLF to LF. Each run prints the exact compile command and fixture hashes.

Compilation uses C++17, `-O0 -fno-fast-math -ffp-contract=off -frounding-math`,
strict warnings and no reassociation/FMA. The executable checks IEEE binary32,
round-to-nearest ties-to-even, tie-even neighbors and gradual-underflow behavior
before emitting observations. Sanitizer mode enables undefined-behavior and
float-cast-overflow checks with recovery disabled. Floating-point overflow itself
is defined IEEE behavior and is intentionally observed only in the excluded file.
The Python suite checks every verbatim body, each file's tamper rejection before
compiler invocation, CRLF portability, missing-compiler failure, corpus bounds,
edge/results coverage, subnormal distinctions and side effects.
