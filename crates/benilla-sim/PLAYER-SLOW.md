# Bounded player decrease-speed arithmetic and source closure

CMaNGOS Project contributors, Classic revision
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`, GPL-2.0-or-later. This Rust translation
and authored oracle host are GPL-3.0-or-later. [Provenance](PROVENANCE.md) and
[upstream notices](licenses/AUTHORS.md) apply. No game assets or database rows
are supplied. These are source-derived bounded mechanisms, not playable native
content admission, proof of a live-client run, or permission to apply an aura.

## Exact numeric branch

- [Unit.cpp:63](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Unit.cpp#L63)
  supplies `baseMoveSpeed`: WALK 2.5f, RUN 7.0f, RUN_BACK 4.5f, SWIM 4.722222f,
  SWIM_BACK 2.5f. Encodings are retained exactly, especially the swim constant
- [GetMaxNegativeAuraModifier](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Unit.cpp#L4529)
  starts at zero and selects the lowest signed amount; effects are neither summed
  nor multiplied together
- [UpdateSpeed](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Unit.cpp#L8337)
  computes `(100.0f + slow) / 100.0f`, multiplies the baseline speed, then the
  admitted ratio1.0. Software APFloat rounds to binary32 nearest-even after each
  operation. [SetSpeedRate](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Unit.cpp#L8492)
  clamps the resulting rate to 0.01f, even for a -100% or stronger modifier.
  [GetSpeed](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Unit.cpp#L8439)
  separately multiplies the stored rate by the movement-type base speed
- [HandleAuraModDecreaseSpeed](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Spells/SpellAuras.cpp#L2912)
  returns without a real apply/remove; real changes update RUN, RUN_BACK and SWIM
  in that order. WALK and SWIM_BACK remain baseline in this no-other-modifier
  domain. [GetXYFlagBasedSpeed](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Unit.cpp#L8454)
  selects swimming before walking, then backward running. The host must prove
  lateral motion independently; a mode alone does not establish displacement

The numeric API accepts at most16 nonpositive amounts in -1,000,000..=0.
The source catalog requires a strictly negative derived amount. The living
player host must exclude mounted, positive speed, normalization, ghost,
nonunit ratio, pet/control and other unrepresented movement modifiers. The
kernel does not infer that closure from an empty vector. `SpeedRates` is a
serializable value, never an authority capability; the runtime validates it
against owned holders.

## Effective source and amount/duration derivation

`PlayerSlowSource` retains all173 effective SpellEntry words after server
overrides, every server-only coefficient/attribute, and linked cast-time, range
and duration rows. Missing rows or Unknown hook queries reject. Complete empty
results are required for spell_area, script names/targets, conditions, proc,
affect/bonus/threat/elixir, group/rank, compiled aura scripts, template scripts,
linked spells and applicable modifier IDs. The existing API field
`spell_script_names` names the pinned source's `spell_scripts` binding query;
it does not assert that a separate source table has that API field's name.
The host must bind these inputs to
actual initial/current units; declared empty source collections are not producer
certification.

`PlayerSlowHostSource` additionally requires complete per-spawn caster rows and
player modifier/script queries. NPC spawn, entry, raw level range and effective
unit flags must match the profile; regeneration policy separately establishes
ordinary nonpet identity. Nonzero owner, charmer, mounted, shapeshift or unit-state
fields reject, as do NPC spell lists, named AI/scripts, addon/spawn-addon auras
and modifier/event-script hooks. This closes the NPC `GetSpellModOwner` path.
The player source initializer's passive metadata is independently checked for
all speed aura types and flat/percent spellmods107/108. Profile construction
does not expose later control, mount or unsupported modifier mutation.

The positive audited ID boundary is1604 plus authored fixture IDs61004/61005.
ID1604 supplies no value, row or stock assertion. Its source rear-daze trigger
remains separately closed. Every accepted row must independently fit the bounded
morphology, including exactly one effect6/aura33, enemy target6, no periodic or
random effect, no passive/death persistence/heartbeat/procs/interruption,
no mechanics/DR, no family/stack/rank/group interaction and no unrepresented
server fields. Unsupported real rows reject rather than being repaired to fit.
Visual/text words are retained; semantic spell priority119 is required zero.

[WorldObject::CalculateSpellEffectValue](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Object.cpp#L2916)
is reused through the exact `EffectValue` softfloat kernel. An ordinary NPC's
[Unit::GetSpellRank](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Unit.cpp#L11876)
starts from its level, not level*5; effect scaling subsequently divides that
rank by5. The complete level1..63 domain is checked for deterministic negative
amounts, and any required effect die rejects. Caster spellmods and aura value
scripts remain unsupported and must be explicitly absent.

[GetSpellDuration/CalculateSpellDuration](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Spells/SpellMgr.cpp#L56)
use abs(Duration[0]), with -1 reserved for permanence. The bounded catalog
admits1..3,600,000ms and excludes permanent/zero duration. Duration[1] is retained
but not consulted; Duration[2] is retained and validated but the NPC branch does
not do player combo-point interpolation.
[SpellAuraHolder construction](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Spells/SpellAuras.cpp#L5185)
sets both current and maximum duration to the calculated duration.

[SpellStacking.cpp](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Spells/SpellStacking.cpp#L312)
marks decrease-speed non-multi-instance. With family/flags zero, distinct IDs
are not multirank and can coexist under empty group/rank closure; only the
strongest amount changes speed. Same-ID/different-caster replacement is outside
this first bounded runtime contract, so the host rejects it transactionally.

## Reproducible source oracle

`python3 tools/generate_player_slow_fixture.py UPSTREAM_ROOT --check` verifies
SHA-256 of both pinned upstream files, extracts the exact complete
GetMaxNegativeAuraModifier, UpdateSpeed, GetSpeed and HandleAuraModDecreaseSpeed
functions and baseMoveSpeed constants, and extracts SetSpeedRate's exact floor
statement. The small authored fixture host explicitly closes all excluded
branches, stores resulting rates, and records handler update order. It does not
model speed packets, client acknowledgments or controlled-unit propagation.
The source-extracted C++ compiles with no fast math/fusion and undefined-behavior
and float-conversion sanitizers. All208 cases are compared bit-for-bit in Rust,
including all amounts0 through-199, extreme negative values, holder order and
duplicates. The fixture hash is
`72d72dc7cedf37466f50a58853d377a41826524e271f5b682e653e09f09a25cc`.

Additional tests cover invalid/forged rate rejection, zero-holder restoration,
movement-mode precedence and non-updated baseline modes. Runtime source tests
cover unknown/missing source rejection, unsupported field mutations, NPC rank
division, nonstock amounts and absolute finite durations whose raw maximum
differs. They do not assert native-player content or rear-trigger admission.
