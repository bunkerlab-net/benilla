# Extracted InitialCast phase and amount oracle

The bundled `.inc` files retain unchanged CMaNGOS Classic code from revision
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`. Each retains the upstream copyright
notice. CMaNGOS Project contributors hold the upstream authorship under
GPL-2.0-or-later. This combined harness is GPL-3.0-or-later; notices and license
texts remain in [../../licenses](../../licenses).

`manifest.json` locks complete original file hashes, each retained excerpt's
hash, and exact extraction boundaries. The Python generator also locks the
manifest's hash. CRLF transport is canonicalized to LF; bare CR and other source
changes fail closed. There is no second handwritten opcode-classification switch
or amount implementation used as the expected result. The committed fixture is
emitted by the compiled original C++ slices, independently of Rust.

## Reproduce

From the repository root, using Python 3 and a supported C++17 compiler:

```sh
# Default: retained excerpts only; no network, installed game, DB, or Cargo
python3 crates/benilla-sim/tools/generate_initial_cast_phase_fixture.py --check
# Also prove byte-identical extraction from a complete pinned upstream checkout
python3 crates/benilla-sim/tools/generate_initial_cast_phase_fixture.py /path/to/mangos-classic --check
# Source verification only; invokes no compiler
python3 crates/benilla-sim/tools/generate_initial_cast_phase_fixture.py /path/to/mangos-classic --verify-source
# Explicitly regenerate only this oracle's JSON fixture
python3 crates/benilla-sim/tools/generate_initial_cast_phase_fixture.py /path/to/mangos-classic --write
python3 -m unittest discover -s crates/benilla-sim/tools -p 'test_initial_cast_phase_oracle.py'
```

`--cxx` selects a compiler. Otherwise the existing native compiler discovery
supports GCC/Clang and MSVC. An absent compiler is an error, never a skip. Strict
binary32 operation ordering is required: no fast math or multiply-add contraction,
round-to-nearest-even. GCC/Clang on non-Windows also use undefined-behavior and
float-to-integer-overflow sanitizers with immediate failure. Windows native
compilers do not have those sanitizers enabled by this harness. Warnings disabled
beyond the shared compiler setup concern unused original local variables and the
original intentional fallthrough. No release client or entire server is built.

## Original code versus authored adapters

| Excerpt | Original content executed |
| --- | --- |
| `TargetDefines.inc`, `EffectDefines.inc` | Target/effect enums and effect-info struct |
| `TargetTables.inc` | Complete target table, effect table, and target-info constructor from `SpellTargets.cpp` |
| `ClassifyEffects.inc` | The first complete per-effect classification loop in `SpellTargetMgr::Initialize`, including UNKNOWN preservation |
| `FillTargetMap.inc` | Complete `Spell::FillTargetMap` method |
| `Targetless.inc` | Complete `Spell::DoAllTargetlessEffects` method |
| `ImmediatePhases.inc` | Contiguous `_handle_immediate_phase` fragment from targetless processing through self execution, before `ProcSpellAuraTriggers` |
| `ExecuteEffects.inc` | Complete `Spell::ExecuteEffects` slot loop |
| `HandleEffect.inc` | Complete `Spell::HandleEffect` and `IsEffectWithImplementedMultiplier` methods |
| `EffectTable.inc` | Complete original effect-handler pointer table |
| `EffectHandlers.inc` | Complete `EffectEmpty`, `EffectProficiency`, and `EffectApplyAura` methods |
| `EffectAmount.inc` | Complete `WorldObject::CalculateSpellEffectValue` method |
| `SimpleValue.inc`, `CacheValue.inc` | Unchanged `SpellEntry::CalculateSimpleValue` definition and spell-constructor cache assignment |
| `HolderCreate.inc` | Contiguous self-hit holder-construction/else/`ExecuteEffects` fragment |
| `RegisterAuras.inc` | Contiguous slot registration loop plus `ApplyAuraModifiers(true, true)` call |
| `AuraMethods.inc` | Complete `SpellAuraHolder::AddAura` and `ApplyAuraModifiers` methods |

The surrounding harness is synthetic and deliberately smaller than the server:

- Input spells are authored slot fields, not actual loaded SpellEntry rows. IDs
  and selected numeric shapes illustrate the public-database audit; unrepresented
  fields, raw passive attributes, duration/cast-time indices, and dependencies are
  not captured by this harness. The 15-cast list is an authored order, not a
  loader/recursive-learning capture. No production admission is expanded
- Every case enters an already-successful, synchronous immediate phase. Explicit
  fixture `resolved_cast_time_ms = 0` and holder `resolved_duration_ms = -1` are
  authored prerequisites, not results from DBC index 1 or 21. Duration lookup,
  SpellStart, CheckCast, Prepare, source event scheduling, cast completion, and
  holder lifetime are not executed or certified
- `SetTargetMap` accepts only caster target1. The synthetic `FillUnitTargets`
  boundary merges the masks of a single nonimmune self target. It does not execute
  source deduplication in the later part of `SpellTargetMgr::Initialize`, target
  filtering, `AddUnitTarget`, hit selection, or duration resolution. Those omissions
  preserve the final singleton mask in these authored cases but do not prove target
  queries, general targeting, positivity, or hit/crit/proc absence in a real cast
- The full original target classifier and `FillTargetMap` choose targetless versus
  self versus no execution. None of that choice is supplied by the Rust planner or
  a synthetic opcode switch. Detect49 with A/B/Targets0 consequently reaches no
  `HandleEffect` call and never queries rank, evaluates scaling, or draws
- Scripts, effect trigger chances other than -1, proc consequences, combo points,
  spell mods, creature scaling, destinations, items, gameobjects, and additional
  handler bodies are excluded. Unexpected adapter branches fail rather than
  supplying guessed outcomes. The source targetless OnHit/proc calls enter neutral
  synthetic boundaries; their real neutrality requires separate complete closure
- `HandleEffect` executes unchanged, including the second binary32 conversion in
  `int32(CalculateSpellEffectValue(...) * DamageMultiplier)` for these effects.
  DamageMultiplier is explicitly 1; no modified scripts/multipliers are admitted
- The amount/draw adapter records source `irand` requests and supplies chosen
  outcomes. It does not emulate upstream MT19937 words or Benilla's deterministic
  seeded generator. All seven Dodge outcomes prove the source request bounds and
  value use. A zero amount still consumes the draw before `EffectEmpty`
- `OnEffectExecute` records the amount and imminent table dispatch. The unchanged
  source then stores `damagePerEffect` and calls the unchanged handler table. Thus
  the dispatch marker is a boundary observation immediately before dispatch, not
  instrumentation inserted into the empty original handler. Successful completion
  proves the table call returned; proficiency masks and aura construction are
  independently observable effects of the invoked original handlers
- `EffectApplyAura` passes the already calculated `damage` to a synthetic
  `CreateAura` constructor. That adapter records/stores it without rerolling and
  emits `aura_construct`. The real Aura constructor, OnAuraValueCalculate,
  holder-insertion checks, and each aura modifier implementation are not executed.
  `aura_register` and `aura_apply` observe the original registration/application
  loops against neutral adapters, not skill/stat/reputation subsystem proof
- Every cast_start/cast_complete marker is a harness boundary. It does not certify
  all source cast-start/end callbacks. Likewise no actual DBC loader order,
  recursion, default skill data, rank maps, SQL migrations, or real-client state is
  inferred from this oracle

## Fixture contract and limits

`tests/fixtures/cmangos_initial_cast_phase_golden.json` has versioned authored inputs
and source-observed outputs in each `cases` entry. Events carry a zero-based
`spell_index` and, where relevant, zero-based `slot`. The ordered kinds are
cast_start, draw, amount, dispatch, holder_create, aura_construct, aura_register,
aura_apply, and cast_complete. Optional event fields are value, low/high, and
self/targetless target. Final proficiency masks, per-cast targetless/self masks,
and rank-query count are separate observations.

The ordinary candidate cases include all seven Dodge81 outcomes, no-dispatch
Detect3050 with real-points-per-level5, proficiency slot ordering/masks, two-slot
Sword and Mace construction, Spirit +5 and Diplomacy +10. The authored 15-cast
projection has 19 dispatched slots (13 targetless and 6 self), one semantic draw,
weapon mask541696, and armor mask3. These are conditional authored-case counts,
not a real Human-genesis completeness certificate.

`mechanism_only` cases exercise supported mechanisms using shapes the public
bounded planner need not accept: sides0/1, mixed targetless/self slot order,
partial proficiency mask overlap, and binary32 constant edges. Existing amount
fixtures are kept intact. `source_only` cases exercise scaled rank0/5/100 and
negative/reversed dice ranges that are deliberately outside the Rust planner.
With nonzero points-per-level, the source actually asks for rank (rank0 is a real
input); without scaling, and for undelivered Detect, it never asks. No rank lookup
is invented to make constant effects pass.

Original signed-i32 constructor cache addition is guarded before execution. The
harness rejects overflow and unsigned base dice beyond INT32_MAX, rather than
claiming undefined/implementation-defined source arithmetic is portable. Five
negative runs require failure for missing/extra/out-of-bounds rolls and these
cache boundaries. Floating overflow is not converted into a made-up expected
value. This is a defined-domain, platform-checked binary32 oracle, not a claim
that every possible source input has defined C++ behavior.
