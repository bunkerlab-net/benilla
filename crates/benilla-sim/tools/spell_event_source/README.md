# Exact SpellEvent scheduler oracle and bounded host

The C++ oracle compiles the **exact source SpellEvent methods and EventProcessor**
against an authored, deliberately small Spell/Unit/AiNotify harness. It does not
compile or claim parity for full `Spell::update`, `Spell::SpellStart`,
`Spell::cast`, `Spell::cancel`, `Spell::handle_delayed`, Unit phases, the nearby
AI visitor, spell math, game effects, or multiplayer. The Rust `spell_event`
helper shares the source scheduler decisions with native callers; it confers no
production admission on a host.

## Provenance and license

Source repository: <https://github.com/cmangos/mangos-classic>, commit
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`.

`source_manifest.json` records full-file SHA-256, source line ranges, fragment
SHA-256, and complete local-file SHA-256. The generator pins the manifest itself
and independently checks every copied source file. `--upstream` additionally
checks the full upstream files and byte-compares each extraction at the recorded
line range. Every `.inc` preserves the original source license header followed
by the verbatim source selection, without instrumentation inside the methods.

Compiled exact source:

- `SpellEvent.methods.inc`: Spell.cpp:7127–7245, constructor, destructor,
  Execute, Abort, and IsDeletable
- `SpellEvent.declaration.inc`: Spell.h:279–294, including the actual
  `MaNGOS::unique_trackable_ptr<Spell>` ownership declaration
- `SpellState.inc`: Spell.h:206–216, exact state values
- `SpellDeletable.inc`: Spell.h:593, exact two-reference predicate
- `UniqueTrackablePtr.h`: complete unmodified upstream ownership implementation
- Existing `../event_processor_source/EventProcessor.cpp` and `.h`, complete
  unmodified source with the existing authored integer type shim

CMaNGOS files retain their GPL-2.0-or-later notices. UniqueTrackablePtr.h retains
its original TrinityCore attribution and GPL-2.0-or-later notice. The existing
[provenance and authorship notices](../../PROVENANCE.md) and GPL texts apply.
The oracle, generator, Rust translation, tests and authored mock are
GPL-3.0-or-later. No game assets, database rows or install content are included.

## Reviewed source lifecycle

These are source observations, separately from the bounded harness's outputs:

1. **Initial event exists before cast validation.** Spell.cpp:2998–3022
   (`SpellStart`) creates a SpellEvent and queues it at `CalculateTime(1)` before
   `PreCastCheck`. A failed check calls `finish(false)` and leaves the event to
   be collected. `Prepare` is then called on success. Spell.cpp:3024–3099 installs
   a current-spell reference for appropriate nontriggered casts and executes
   instant spells synchronously. The fixture models only that queue/finish edge.
2. **Creation tick compares map clock, not event clock.** Spell.cpp:3519–3526
   tests `m_updated` once, sets it true, then returns if creation time equals the
   map's current millisecond time. The constructor captures that map time at
   Spell.cpp:451. The source scheduler calls `update(p_time)` before examining
   state. The queue passes the complete Unit diff, not time since the event's
   deadline. Harness `V`/`Z` traces model these selected conditions; they do not
   prove the many omitted update checks.
3. **Casting uses full Unit diff.** Spell.cpp:3556–3575 subtracts the provided
   diff from the casting timer, clamping at zero, and can call `cast()` in that
   callback. The mock models only this timer transition. Target invalidation,
   movement interrupts, charges, costs, effects, GCD, channel timers and cast
   eligibility are not executed by the fixture.
4. **Travel starts on the first event callback.** Spell.cpp:7212–7220 sets
   delay start to actual `e_time` and queues `e_time + GetDelayMoment()`, preserving
   `m_addTime`. It does not backdate delay start to the cast deadline, nor consume
   an invented remaining slice of the Unit diff. A zero delay re-add is eligible
   again in the same EventProcessor update, after already-enqueued equal keys.
5. **Started nonchanneled travel invokes the delayed handler once.**
   Spell.cpp:7199–7210 passes `e_time - delayStart`. A nonzero returned offset
   queues `delayStart + nextOffset`, including an already-due deadline. A zero
   result falls through to `e_time + 1`, even if the handler just finished the
   spell. The next callback sees FINISHED and decides deletion. The fixture's
   delayed handler returns authored scripted offsets; it proves the scheduler's
   handling of them, not source effect target/travel math.
6. **Finished is not synonymous with deletable.** Spell.h:593 requires neither
   current-spell nor currently-executed reference. SpellEvent retries a finished
   nondeletable spell at `now+1`. Unit.cpp:483–492 dispatches events before
   `_UpdateSpells`; Unit.cpp:4094–4102 clears finished current-spell references
   only in that subsequent phase. Current-reference clearing is authored in the
   harness and separately source-reviewed here. Unit destructor, Unit.cpp:438–449,
   clears current-spell references before member teardown.
7. **Cancel, Abort and destructor are different edges.** SpellEvent Abort and
   destructor each call `cancel()` only when not already FINISHED. A direct
   source `Spell::cancel()` finishes without removing its SpellEvent; full
   cancellation side effects are in Spell.cpp:3101–3158 and are not mocked as
   validated behavior. Queue `KillEvent` deletes without Abort; destructor can
   consequently cancel. Due abort always deletes the event. `KillAll(false)`
   calls Abort before asking IsDeletable and can retain an aborted event. An
   additional KillAll calls Abort again. Unit.cpp:9428–9439 describes cleanup's
   nonforce event sweep after interrupting spells.
8. **Force deletes the event, not unconditionally the Spell.** SpellEvent's
   exact custom deleter deletes the Spell only if it is deletable or
   `World::IsStopped()`. Otherwise it logs a leak and leaves the Spell alive.
   The fixture preserves that leaked mock object in its next snapshot and only
   frees it during quiet test-isolation reset. That cleanup is not a claim the
   source frees it. Native teardown must release source current/executed
   references before force deletion, or reject the unrepresented state.
9. **AI co-due order comes from the exact queue.** Unit.cpp:10923–10978 contains
   a one-shot notify callback and its schedule/abort paths. The fixture's
   AiNotify is an authored one-shot peer with no nearby-object visitor. Exact
   EventProcessor multimap insertion order puts a same-deadline Spell re-add
   after existing peers. Both initial AiNotify/Spell orders are exercised.
10. **Channels are outside the Rust host helper.** Exact C++ traces include
    first travel delay, another-cast cancellation, immediate channel transition
    and CHANNELING retry. The Rust helper explicitly rejects channeled work;
    tests do not mislabel rejection as channel parity.

## Run and check

From the repository root:

```sh
python3 crates/benilla-sim/tools/generate_spell_event_fixture.py --check
python3 crates/benilla-sim/tools/generate_spell_event_fixture.py --check --upstream /path/to/pinned/repo
cargo test --locked --offline -p benilla-sim --test spell_event
```

A compiler is mandatory; absence is an error, never a skip. `--trace-output`
writes a complete readable trace. `--sanitize` uses AddressSanitizer and UBSan.
Where LeakSanitizer cannot run under the executor's ptrace sandbox, use
`ASAN_OPTIONS=detect_leaks=0` for the address/undefined check and report leak
checking as unrun. Reset explicitly frees mock objects left by the exercised
source leak branch. The check uses no network, source download, Rust oracle,
server, private database, credentials, assets, producers, consumer or offline
compiler admission path.

## Fixture command schema

The fixture is `tests/fixtures/spell_event_cmangos.tsv`. It is whitespace-delimited
and comments start with `#`. Every authored command is echoed with `>` and is
followed by the complete ordered callback trace and a stable state dump.

- `P name`: reset queue/map time, objects and mock flags; cleanup is quiet
- `N id timer travel current failed created`: create a spell/event; event due
  `now+1`, cast timer is uint32 ms, travel=-1 finishes immediately on release,
  travel>=0 enters TRAVELING with that delay; failed precheck creates a finished
  unreferenced spell; created is an independent map-clock timestamp
- `U diff map clear`: dispatch queue with full uint32 diff at the supplied map
  clock; clear=1 then removes current references from finished mock spells
- `I id delay`: add an authored one-shot AiNotify peer at `now+delay`
- `R id current executed`: set the two IsDeletable reference flags
- `C id`: direct mock cancel to FINISHED, retaining the queued event
- `B id`: set queue event `to_Abort`
- `X force stopped`: set mocked World::IsStopped and source KillAllEvents
- `K id`: source KillEvent, which skips Abort but runs the real destructor
- `G id next`: next mock delayed-handler return offset; consumed once
- `Y id other_cast`: mark the mock SpellInfo channeled and set other-cast check

Fixture inputs are original bounded synthetic values. The driver is not a public
untrusted-input parser and does not justify expanding runtime admission limits.

## Trace and snapshot rows

- `E id now diff add exec`: entry into exact SpellEvent Execute
- `I id now diff add exec`: authored AiNotify callback
- `A id now add exec`: entry into exact SpellEvent Abort (or mock peer Abort)
- `D id`: event destructor **entry**, before base SpellEvent destructor effects
- `F id`: mock Spell destructor invoked by the exact source custom deleter
- `V id diff old_state old_timer`: entry into mock Spell update
- `Z id`: mock creation-tick skip; m_updated has already become true
- `L id`: mock release; `T id start`: source called SetDelayStart
- `G id elapsed next`: exact source requested delayed handler; mock return
- `M id`: mock handle_immediate entered for source channeled branch
- `C id`: mock cancellation was invoked; `J id`: post-queue reference clear
- `W id`: exact source deleter refused deletion and logged the leak
- `S now map queued live_events live_spells aborting`: stable owner counts
- `Q id kind deadline add exec aborted`: queue order, kind S=Spell or I=AiNotify
- `H id state timer start travel current executed updated`: mock spell, ID order

Exact source enum values: CREATED=0, TARGETING=1, CASTING=2, DELAYED=3,
TRAVELING=4, LANDING=5, CHANNELING=6, FINISHED=7.

The normalized row is:

`O id now dispatch_state deletable start_before travel next_offset channeled other_cast delete start_after next_deadline`

`dispatch_state` is the state after mock `update` returns, before exact scheduler
handling. `next_offset` is the next scripted delayed-handler result (unused on
other branches). `next_deadline` is zero for Delete; `start_after` preserves the
source member even for a finished spell. The authored update captures its state
at function exit; the exact SpellEvent code is unchanged.

## Independent Rust comparison

`tests/spell_event.rs` reads each normalized `O` row. It calls the same
`plan_after_update` and `plan_after_delayed` helpers used by native callers,
checks delayed elapsed time and compares delete/re-add result, source delay
start and exact deadline. It explicitly checks unsupported channel rejection
and separately tests checked arithmetic, future delay-start rejection, the zero
sentinel, already-due offsets and immediate zero-delay recurrence.

This is independent of Rust-produced expected values: fixture generation calls
only the pinned C++ methods and authored input driver. The full lifecycle rows
remain readable diagnostic evidence, but are not all replayed by the helper
unit test. Runtime tests must additionally verify their own actual host state,
full-diff and creation-clock inputs, current-reference phase, cancellation and
teardown, complete event/Spell ownership, AiNotify ordering, transaction rollback,
checkpoint reopen, generation binding, queue limits and fail-closed admission.
