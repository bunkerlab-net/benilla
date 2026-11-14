# Mage resource dependencies

This is a bounded, engine-independent dependency adapter for the existing
[spell component](SPELLS.md). It does not compile native starter content or
establish retail Classic gameplay parity. The implementation translates selected
CMaNGOS Classic paths at commit `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`.
CMaNGOS Project contributors hold the upstream GPL-2.0-or-later source grant;
2026 Benilla fork translations are GPL-3.0-or-later. See
[PROVENANCE.md](PROVENANCE.md) and preserved notices in [licenses](licenses/AUTHORS.md).
No assets, native DBC rows, SQL rows, or invented starter stats are bundled.

## Authoritative input contract

`mage_resource` accepts effective state; it never substitutes a Human/Mage
constant for missing native data. It contains no `Default` for player state,
modifier observations, skills or admission exclusions. Required dependencies:

- Proven player class Mage, effective level and uint32 intellect/spirit fields
  after authoritative
  race/class/level stats, items, passives and auras. `GetStat` converts the uint32
  field to float. Creating those fields and applying stat modifiers are outside
  this module; initial SQL skill/spell candidates are not live player state
- Every `SkillLineAbility.dbc` mapping for each queried spell, retaining source
  indexed row ID, spell ID and skill ID. Source `LoadSkillLineAbilityMaps` inserts
  in ascending DBC index order; `GetSpellRank` uses the first matching multimap
  entry, even if another entry references a known/higher skill. Do not use the
  client presentation catalog's single-row-per-spell overwrite as this contract
- Live current skill values, signed permanent/temporary bonuses and deleted
  status. Absent/zero/deleted skill is zero. `GetSkillValue` adds both bonuses,
  clamps negative totals to zero and narrows to uint16, including defined wrap.
  Source skill IDs narrow to uint16 at that call boundary. `maxLevel * 5` caps
  the resulting rank; absent mapping returns zero, never `level * 5`
- Explicit school and all-school spell-crit aura totals. Only their zero/zero
  branch is supported; nonzero totals return `Unsupported`. The function returns
  `UpdateSpellCritChance` percent. Victim, spell-family, item, script and other
  `Unit::GetCritChance` dependencies remain the final caller's responsibility
- Explicit `UpdateManaRegen` aggregate spirit multiplier, MP5 aura sum and
  interrupted percentage. Their aggregation order and eligibility must be
  resolved from actual aura/passive/item state. Totals are not a substitute for
  proving unsupported aura effects absent. The interrupt percentage is capped
  only above 100; a negative percentage is preserved when final rates stay valid
- Explicit world mana-rate configuration and real current/maximum mana. Create
  mana and final maximum mana differ and remain separately resolved dependencies
- Target type/totem status: `base_magic_miss` returns source magic base miss 4%
  for ordinary units and zero for creature totems. It is not final hit/resist
  chance or proof of zero resistance. Attribute-based attack classification,
  totem identification, resistance and the spell component's remaining hit
  exclusions must be proven separately

The existing spell component rejects rank zero and has a narrower admitted rank
range than the source rank helper. A missing skill mapping/current value is an
explicit native-compiler blocker; do not replace it with a plausible value.

## Crit coefficients and evidence limit

`GetSpellCritFromIntellect` for mage evaluates
`3.70f + intellect / (14.77f + 0.65f * level)`, then
`UpdateSpellCritChance` adds its zero-initialized accumulator and two explicit
zero aura totals in source order. Upstream marks this coefficient table
`[TZERO] from mangos 3462 for 1.12 MUST BE CHECKED`. These coefficients are
**pinned-CMaNGOS parity only**, not independently verified vanilla accuracy.
A returned percentage outside the downstream combat-die domain must be handled
as unsupported, not silently changed by this dependency helper.

## Numeric and scheduling policy

`MAGE_RESOURCE_POLICY_ID` versions software APFloat nearest-even operations.
The existing exact pinned `rustc_apfloat` dependency supplies binary32 and binary64;
no host floating-point arithmetic or host libm determines Rust outputs. Inputs
use `SpellFloat` binary encodings, finite normals or signed zero, with magnitude
at most 1,000,000. Effective intellect/spirit and mana are bounded to 0..=1,000,000;
level is 1..=60. Aura integer aggregates have absolute value at most 1,000,000.
Negative final regeneration, unsupported encodings, floating exceptions other
than inexact rounding, and undefined float-to-integer conversions are rejected.
Overflowing source integer sums/timers or `maxLevel * 5` are rejected instead of
reproducing unrelated unsigned overflow. These are explicit bounded domains.

The mage `OCTRegenMPPerSpirit` sequence is important:

1. Convert effective uint32 spirit to binary32 and divide by binary32 4
2. Promote that rounded value to binary64 and add **unsuffixed binary64 12.5**
3. Assign/round back to binary32, then divide by binary32 `2.0f`

`UpdateManaRegen` multiplies by the supplied binary32 spirit multiplier,
computes integer MP5 / `5.0f`, and preserves source multiply/divide/add ordering.
`Player::Regenerate` multiplies selected per-second regen by the explicit world
rate, then by `uint32(float(diff) / 1000)`. The latter is not integer division:
16,777,999 milliseconds becomes 16,778 seconds because of binary32 rounding.
The direct numeric helper covers all uint32 diffs subject to valid output
conversion; positive results truncate toward zero and clamp to maximum mana.
Fractional mana is discarded at every call, with no accumulated fraction.

`MageManaState` shares actual `classic_spell::CasterResources`. Its constructor
requires explicit mana, maximum, regen rates and source timer counters; callers
must not recover the countdown from the spell component's timestamp.
`after_spell_and_aura_events` is an ordered resource adapter:

1. The caller runs `m_events.Update` / spell / aura effects first and supplies
   their resulting spell events exactly once, after their actual mana deduction
2. Any successful positive `ManaSpent` sets the source countdown to 5000ms;
   zero cost, interrupted or failed casts do not reset it. Timestamp equality
   does not discard repeated legitimate mana use
3. Decrement the countdown by the **same frame diff**, saturating at zero
4. If alive after events, accumulate the regen timer. At >=2000ms call regen
   once with `accumulated / 100 * 100`, then retain the sub-100ms remainder
5. The selected regen rate observes the decremented countdown. A cast completed
   in a 5000ms frame can therefore leave no interruption timer at regeneration

Death stops regen-timer accumulation but not the mana-use countdown.
Out-of-world frames are no-ops and cannot contain spell events. Regen remains
allowed in combat; this module does not model health, rage or energy regeneration.
Snapshot regen elapsed time must be below the scheduling threshold and the
mana-use timer at most 5000ms. No wall clock or default world rate is consulted.

The bounded spell component's mana path excludes cast items, aura-triggered
casts and `SPELL_ATTR_EX2_DONT_BLOCK_MANA_REGEN`. `ManaUseExclusions` requires
explicit observations of those branches and other unmodeled mana mutations;
any present branch returns `Unsupported`. The adapter does not deduct spell
costs a second time. Unsupported resource effects, drains, potions, regeneration
channels, auras that change effective stats, mana maximum changes, and character
progression require additional caller implementations or rejection. Rate changes
can use `set_regen` only after those authoritative dependencies are resolved.

Adapter validation failures do not mutate its state. Spell execution has its
own commit boundary; a caller that wants full-frame atomicity must stage casts,
auras and this adapter together. This component is not a persistence format.

## Verification

`tools/generate_mage_resource_fixture.py` checks full-file hashes, inserts
unchanged original `GetSkill`, `GetSpellRank`, `GetSpellCritFromIntellect`,
`UpdateSpellCritChance`, `OCTRegenMPPerSpirit`, `UpdateManaRegen`,
`Regenerate` and magic `GetMissChance` methods into an explicit synthetic C++
state adapter. The source mana timer and regen scheduling blocks are also
extracted unchanged. It verifies source event-before-timer-before-regen order,
map insertion order, the 2000ms/5000ms constants and mana-use attribute guard.
Strict C++ flags disable contraction/fast math and enable undefined-behavior /
float-conversion sanitizers. The fixture contains 2,082 numeric/state probes;
Rust tests compare every row and independently cover unsupported inputs, no
fractional carry, timer boundaries, zero/failed costs, source narrowing, absent
first skills, transactional failures and actual spell-component composition.

Regenerate with:

`python3 crates/benilla-sim/tools/generate_mage_resource_fixture.py /path/to/pinned/files`

Run focused verification with:

`cargo test -p benilla-sim --test mage_resource`

The fixture SHA-256 is
`3d3ab4767ae6ec5cb86bac3ebff64ee2b839aa63b544bb02e3b85ebe49a414b5`.
It contains synthetic outputs only and must be force-added because the generic
repository fixture ignore rule also covers these reviewed algorithm probes.
Tests do not fetch source or require an installation. They prove bounded
pinned-source behavior, not native starter admission, raw row completeness,
retail coefficients, full runtime/app integration, or live playability.
