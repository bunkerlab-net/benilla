# Exact-source MirrorTimer oracle

This independent oracle executes CMaNGOS Classic
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f` timer code against authored sequential
operations. It proves the bounded timer state machine and its numeric behavior.
It does not execute Player environment callbacks, timer ownership/lifecycle,
world updates, network packets, breath/fatigue gameplay, or environmental damage.

The harness and generator are GPL-3.0-or-later. Extracted upstream material
remains GPL-2.0-or-later under its original notices; all four notices are retained
in the temporary translation unit. The combined oracle is GPL-3.0-or-later. See
the root [GPL license](../../../LICENSE-GPL-3.0) and existing
[CMaNGOS provenance](../../benilla-sim/PROVENANCE.md).

## Run

From the repository root, with the complete pinned source available:

```sh
python3 crates/benilla-world-runtime/tools/generate_mirror_timer_fixture.py /path/to/mangos-classic
python3 crates/benilla-world-runtime/tools/generate_mirror_timer_fixture.py /path/to/mangos-classic --check
python3 crates/benilla-world-runtime/tools/generate_mirror_timer_fixture.py /path/to/mangos-classic --check --sanitize
python3 crates/benilla-world-runtime/tools/test_mirror_timer_oracle.py /path/to/mangos-classic
```

A missing compiler fails rather than skips. `CXX` may select a compiler; the
tested integer profile is C++17 with 32-bit `int` and exact-width `uint32_t` and
`int32_t`. `--sanitize` enables undefined-behavior checks with no recovery.
Unsigned wrapping is defined C++ behavior and is intentionally retained, with
no saturating arithmetic, signed-wrap flags, or widened timer calculations.
The executable uses the `.test-fixture` suffix and all compiler products are
temporary. No installation, native input compiler, game assets, world engine,
network, or Rust build is used by these tools.

## Source boundary

[The hash manifest](mirror_timer_source_hashes.sha256) guards every consumed
upstream file byte-for-byte **before extraction or compilation**:

- `Player.cpp`: all nine complete `MirrorTimer` method definitions, lines 154–285
- `Player.h`: the entire `MirrorTimer` class, lines 410–473, including both enums,
  constructor, every inline getter, overload declarations, and all state fields
- `shared/Util/Timer.h`: the entire original `ShortIntervalTimer` class, including
  its zero-initializing constructor and actual update, passed, reset, setter,
  and getter bodies
- `shared/Common.h`: the complete `TimeConstants` enum that supplies the pulse
  interval's original `IN_MILLISECONDS` constant

[The generator](generate_mirror_timer_fixture.py) substitutes these verbatim
blocks into [the harness](mirror_timer_oracle.cpp); it does not reproduce timer
answers in Python or authored C++. For read-only full-state observation, the
original MirrorTimer declaration is surrounded by `#define private public` and
`#undef private`. This changes access checking only. No source constructor,
getter, operation, private field, or helper body is rewritten; the actual
ShortIntervalTimer fields remain private and are read through its source
getters. No timer fields are directly assigned by the harness.

Each case constructs the source `MirrorTimer(Type)` and preserves its state
between operations. The corpus includes four timer types, targeted sequences,
an 11-duration × 10-scale × 11-difference matrix, an 11-duration × 11-remaining
matrix, and four deterministic 256-operation sequences. Operation selection is
independent of observed timer output. The output is never generated from Rust.

## Defined domain and undefined `abs(INT_MIN)`

The source stores every `int32` scale, including `INT_MIN`. Inactive and
effectively frozen `Update` calls return before `std::abs`, so those calls are
defined even at `INT_MIN` and execute the original body here.

An active, effectively unfrozen update at `INT_MIN` would evaluate an absolute
value that cannot be represented by the source signed type. The harness rejects
**only that call**, including a zero-difference call, before executing the body.
Its `return` field is `undefined_abs`; before/after state is unchanged. This is
an explicit harness domain classification and atomic rejection observation,
**not a return value or state transition obtained by running undefined C++**.
The prior `SetScale(INT_MIN)`, getters, status fetches, setters, stopping,
starting, and inactive/frozen updates remain within the source domain.

## Product and schema

[mirror-timer.tsv](fixtures/mirror-timer.tsv) contains 9,479 steps in 1,346
independently constructed cases. There are 9,230 executed defined observations
and 249 explicitly rejected undefined calls. All inputs are authored numeric
test data, not game data.

The 32 tab-separated columns are:

1. `case`, `step`, `type`, `op`, `arg1`, `arg2`, `arg3`, `return`
2. Before-state: `scale`, `spell_id`, `status`, `active`, `frozen_raw`,
   `tracker_interval`, `tracker_current`, `pulse_interval`, `pulse_current`,
   `remaining`, `is_frozen`, `is_regenerating`
3. After-state: the same twelve fields in the same order

All numeric fields are decimal, booleans are `0`/`1`, status values are the
original `UNCHANGED=0`, `FULL_UPDATE=1`, `STATUS_UPDATE=2`, and timer types are
`FATIGUE=0`, `BREATH=1`, `FEIGNDEATH=2`, `ENVIRONMENTAL=3`. Void operations and
`inspect` have return `-`; `update` returns `0`/`1` unless domain-rejected;
`fetch_status` returns a source status and resets stored status to zero.

Operations and arguments:

- `inspect`: observe only, unused arguments zero
- `start`: interval, spell ID, zero
- `start_remaining`: current remaining, maximum duration, spell ID
- `stop` and `fetch_status`: unused arguments zero
- `set_remaining`, `set_duration`, `update`: duration or difference, zero, zero
- `set_scale`: signed scale, zero, zero
- `set_frozen`: requested boolean, zero, zero

## Exercised distinctions

- Both ShortIntervalTimer accumulators begin at zero, as do their intervals;
  the MirrorTimer constructor starts inactive, unfrozen, scale `-1`, spell `0`
- Exact expiry produces an immediate false result, while subsequent expired
  updates pulse at 2,000 ms; the first expiration's overshoot is discarded
- A large pulse subtracts its interval only once. Residual backlog can produce
  further false results even on zero-difference calls
- Tracker addition, pulse addition, remaining subtraction, `max-current`, and
  scale multiplication wrap at 32 bits, including updates of expired timers
- Two-argument `Start` preserves raw frozen state; the remaining-value overload
  thaws and can overwrite `FULL_UPDATE` with `STATUS_UPDATE`
- Positive scale overrides raw frozen state. `SetScale(0)` requests freeze but
  does not store zero, and therefore does not freeze a regenerating timer
- `SetFrozen` compares against effective frozen state: repeatedly requesting
  true while regenerating can repeatedly emit status, while clearing a raw
  freeze during regeneration need not emit status
- Setters overwrite, rather than priority-merge, status; equal-value setters
  preserve status; `FetchStatus` consumes it; expiration alone does not set it
- Regeneration clears a pulse accumulator or stops at the exact boundary;
  Stop clears both currents but retains intervals, scale, spell and raw freeze
- Zero duration starts, zero setters, remaining greater than maximum,
  `u32::MAX`, high spell IDs, both signed extremes, and zero-difference updates

[The Python checks](test_mirror_timer_oracle.py) verify complete verbatim source
extraction, reject a changed byte in each dependency before a compiler probe
can run, fail on a missing compiler, inspect case continuity and constructor
state, and assert the corpus actually contains the distinguishing observations
above. Rust consumer tests compare their full state and operation results to
this independent source fixture; rejected calls are a separate consumer-domain
contract and must not be described as source C++ outcomes.
