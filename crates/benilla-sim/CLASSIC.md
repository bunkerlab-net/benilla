# Bounded Classic formulas

`classic` contains a small callable port of pinned CMaNGOS algorithms. It contains
no game rows, client files, racial/class starting values, credentials or database
connection. It does not make a native world runnable. All test inputs are original
arithmetic probes; all oracle outputs are numeric algorithm results.

## Authority and numerical contract

`NUMERIC_POLICY_VERSION = 1` defines the bounds and rational armor policy. A runtime
using it must version/hash that policy with its rules; changing it invalidates the
meaning of old replay/snapshot hashes unless an explicit migration is provided.

`armor_reduced_damage(level, armor, penetration, damage)` is integer-only:

- Level: 1 through 63; signed armor/penetration: -1,000,000,000 through
  1,000,000,000; incoming integer damage: 0 through 1,000,000,000
- Effective armor is `max(0, armor - penetration)` using signed 64-bit subtraction
- Let `K = 85 * level + 400`. If armor is at least `3 * K`, remaining damage is
  `floor(damage / 4)`; otherwise it is `floor(damage * K / (armor + K))`
- Return at least one, preserving the source helper's minimum even for damage zero
- All products fit `u64` under the checked domain. No float operation or implicit
  float-to-integer conversion appears on this authoritative path

This is the exact rational reformulation, followed by floor, of the source
formula. It deliberately does not reproduce intermediate binary32 rounding.
`armor_rounding_differences.tsv` enumerates every difference over levels 1..10,
armor 0..2000, damage 0..64, penetration zero: 1,300,650 cases, 168 differences.
For example, level 1 / armor 388 / damage 9 yields 5 under this policy and 4 under
the source float path. This is a documented fork rule, not exact Classic parity.
Outside that enumerated domain differences can be larger, particularly above
binary32's exact-integer range; no universal one-point error bound is claimed.

The caller must decide whether the hit exists, is physical, is immune, or is
otherwise eligible before calling. This helper does not implement hit rolls,
block, absorb, crit, resistance, school selection, damage variance, weapon
skills, or attack timing. A runtime may use it for its authored fixture's
physical melee without asserting that its native creature, player or spell
pipeline is implemented. The reference/current source gates must remain closed
until those dependencies are supported.

## Source-exact software binary32 armor

For a future native-authoritative profile, use `software_armor_reduced_damage`
(or `software_armor_reduction` to inspect both result bits and integer damage),
with the **same bounded integer inputs**. Its policy identity is
`SOFTWARE_ARMOR_POLICY_ID = "cmangos-8ec338a1-armor-binary32-rne-v1"`. A runtime
must explicitly select/hash this policy; it must not reinterpret a rational-policy
save or journal as software-binary32 without a deliberate version transition.

This path uses the official `rust-lang/rustc_apfloat` LLVM APFloat port's `Single`
type. All arithmetic executes on integer limbs, with explicit nearest-ties-even
rounding after every source operation and toward-zero integer conversion at the
melee assignment. Constants are binary32 bit patterns, not host float expressions.
Armor and penetration are converted separately before subtraction. No host
`f32`/`f64` arithmetic, global rounding mode, string conversion, allocation for
arithmetic, or handwritten general floating-point engine is used. Invalid,
overflow, underflow and divide-by-zero statuses fail; ordinary INEXACT rounding
is expected. This preserves the native source result at the 168 rational-policy
differences rather than silently treating that drift as Classic behavior.

The source-exact path matches the 5,376 independently compiled C++ float-bit and
integer-cast probes and every integer result in the exhaustive 1,300,650-case
comparison domain. Those checks do not establish full combat parity or prove
an architecture that has not run them. A small release-only development probe
processed one million bounded calls in about 0.44 seconds on the cloud Linux
machine; it is a feasibility result, not an application/per-platform benchmark.
The broader stat helpers below remain reference/import-validation-only.

## Reference/import-validation helpers

These helpers preserve the pinned source's binary32 operation sequence and are
**not integer-only runtime authority**:

- `cmangos_armor_reduction`: source float bits and the subsequent physical-melee
  `uint32` truncation. Same armor domain as the rational path
- `health_bonus_from_stamina` and `mana_bonus_from_intellect`: the first 20 points
  count one each; subsequent points count ten or fifteen, respectively
- `player_max_health`: resolved UnitMods slots, create health and player stamina;
  source max-one clamp and halfway-away-from-zero rounding
- `maximum_mana`: corresponding POWER_MANA path, including suppression of the
  intellect bonus when create mana is zero
- `creature_max_health`: creature stamina contributes its bonus difference from
  the floating-point create stamina; the health multiplier affects both create
  health and that difference. It does not incorrectly add full player stamina

Create health/mana are bounded unsigned integers through 1e9. Resolved stats are
signed integers within +/-1e6. Create stamina is finite 0..1e6. Flat modifiers
are finite +/-1e9; percentage factors and the resolved creature health multiplier
are finite 0..1000. NaN, infinity, subnormal inputs and out-of-domain values are
errors. Negative raw output mana and output over 1e9 are errors before casting;
health's minimum-one clamp is the upstream rule. These bounds describe supported
formula inputs, not what an authentic race/class can obtain.

Every multiplication, addition, subtraction and division is a distinct source
binary32 operation, with no `mul_add`, fast-math or algebraic reassociation. The
C++ oracle uses FE_TONEAREST, `-fno-fast-math`, `-ffp-contract=off`. Committed exact
bit/value vectors are intended to run on Linux amd64/arm64, macOS arm64 and Windows
amd64; a Linux-only development pass does not establish the other targets passed.

## Local-data prerequisite and remaining blockers

Player create health/mana must come from the matching local
`player_classlevelstats` row; attributes from `player_levelstats`, selected by the
validated race/class/level. Starting equipment, learned spells/passives and
modifier slots need their own audited data/dependency path. A default modifier
value is valid only when its absence is actually established, never because a
capture omitted the dependency. Full `Player::Create`/`InitStatsForLevel`, attack
power, regeneration, faction, skill, spell and level-up lifecycle remain unported.

Creature create health/stamina and the resolved health multiplier need the
matching `creature_template_classlevelstats` row and `Creature::SelectLevel`
semantics. Rank health-rate application, negative-multiplier legacy fallbacks,
random levels, power-type selection, equipment, auras and spawn overrides remain
caller/compiler prerequisites. This module does not perform `SelectLevel` or
pretend that raw HealthMultiplier zero always means zero effective health: that
source initializer selects a multiplier of one for zero. The stat helper receives
an already-resolved multiplier. Native faction/DBC dependencies are separate.

A future compiler must validate those inputs, refuse every required unsupported
field/flag, retain native source IDs/provenance and emit no runnable profile until
all required mechanics have a supported runtime representation. Reading the
player's own MPQs cannot supply missing server-side world tables. No local native
schema/data or actual installed client was used to verify this module.

## Reproduce

With the exact public source files named in `PROVENANCE.md`, run:

```sh
python3 crates/benilla-sim/tools/generate_classic_fixture.py /path/to/upstream/src/game/Entities
cargo test -p benilla-sim
cargo clippy -p benilla-sim --all-targets -- -D warnings
```

The generator checks full-file SHA-256 values before extracting original C++
function bodies. The mock objects supply only resolved inputs/accessors, capture
output fields, and dispatch the actual source methods; formulas are not rewritten
in Python. It produces 5,376 armor bit/cast vectors, 1,011 stat vectors and the
exhaustive difference ledger. Tests compare both software-binary32 and host
reference results against the same independent source oracle. Re-running it must leave all fixtures unchanged.
