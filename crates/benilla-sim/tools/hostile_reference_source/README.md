# Exact-source owned hostile-reference oracle

This bounded oracle executes unchanged LF-normalized CMaNGOS Classic source
from revision `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`, with authored linked-unit
inputs and actual upstream online/offline `std::list` containers and target-side
intrusive reverse-reference lists. It does not substitute a translated threat
formula or a standalone eligibility predicate for owned reference transitions.

Canonical source: [CMaNGOS Classic at the pinned revision](https://github.com/cmangos/mangos-classic/tree/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f).
The retained files preserve their original GPL-2.0-or-later notices and CMaNGOS
contributor attribution. The authored host, generator and Rust differential
reader are GPL-3.0-or-later, Copyright 2026 Benilla fork contributors. See the
crate's [provenance](../../PROVENANCE.md) and [licenses](../../licenses).
No game installation, database contents, network, terrain or proprietary data
are used.

## Retained source and host boundary

`source_manifest.json` pins full upstream file, excerpt-body and retained-file
SHA-256 hashes. `generate.py` independently pins the manifest hash. Retained
bytes must match exactly. Optional `--upstream` verifies every retained body
and notice against complete hash-pinned upstream files, normalizing only CRLF
to LF. Normal checks require no upstream checkout or network. Whole files are
retained for `ThreatManager.h`, `HostileRefManager.h`, `UnitEvents.h` and the
`LinkedList`/`Reference`/`RefManager` implementation.

Executed source includes:

- `ThreatManager.cpp:63–204`: construction/linking, target callbacks, clamped
  addition, invalid-reference lookup/rebind, online and accessibility changes,
  removal and source lookup
- `ThreatManager.cpp:210–246,288–325`: actual container clear/find/add and update
- `ThreatManager.cpp:331–424,430–442,470–490,506–517,591–602,608–666`: explicit
  selection, manager construction/clear, direct addition, current victim and
  status-event handling
- `HostileRefManager.cpp:25–32,104–152,209–212`: target-side manager ownership,
  refresh callbacks, deletion and list traversal
- `Unit.cpp:8944–8950,8665–8669`: exact offline predicate and `DeleteThreatList`
- `Creature.cpp:2962–2965`, `CreatureSettings.cpp:105–108`,
  `Unit.h:334,1305,2085–2090,2298`, `CreatureDefines.h:75`: exact raw-state/flag
  predicates, flag constants and actual target reverse-link insertion

The authored host supplies primitive types, opaque identities, explicit object
lookup storage, tiny Unit storage, and diagnostics. It fixes the executed domain
to one ordinary unowned non-GM player target, normal priority and melee reach. Its
out-of-domain spell-threat, master/pet, combat-state and suppression hooks
throw instead of pretending to implement those systems. Public/private access
macros apply only while compiling the unchanged source headers and expose the
existing containers for observation; they do not change transition logic.
A derived recording event inherits the original `UnitEvents.h` constructors and
storage. A preprocessor alias substitutes that event type in the retained
ThreatManager declarations and method bodies. Its `setThreatManager` first
forwards the original setter, then records the event at actual source dispatch;
no transition function body is edited. `CreatureStaticFlags2` bitwise storage
glue is authored; the original
`CreatureSettings::HasFlag` and `Creature::IsIgnoringFeignDeath` functions run.

## Evidence and exclusions

The fixture records 611 observations: 83 lifecycle/ownership snapshots and 528
raw bit-predicate observations (512 distinct state/flag pairs). It covers:

- Constructor/link defaults, add-zero creation, repeated addition reusing one
  reference, negative deltas/clamping and actual aggro-callback counts
- Explicit selection clearing dirty, and absence of automatic current-victim
  assignment on addition, online return or rebind
- Successful-feign offline transitions, ignored-feign return, no status refresh
  until called, and preservation of reference identity and threat while offline
- Invalid references keeping source/GUID/threat, failed lookup remaining offline,
  same-GUID lookup rebind, target reverse-link removal/recreation and removal
- Target deletion via exact `Unit::DeleteThreatList`, owner cleanup, online and
  offline clears, repeated clears and fresh zero-threat references after cleanup
- Accessibility toggling without becoming an online predicate or a terrain
  accessibility claim
- Every individual bit and complement in authored raw unit-state/static-flag
  words; native runtime tests compare their private evaluator to the source

The Rust manager deliberately supports one reference. No multi-target sorting,
110/130 percent switching, player priority, taunt, suppression, pet ownership,
full threat calculation, aura success determination, ordinary hostility checks,
damage/death execution, respawn engine or full combat parity is established.
The source functions contain broader branches, but the corpus does not execute
or claim them. `D` is authored lifecycle dispatch to exact `DeleteThreatList`,
not a simulation of the death call chain. A fresh post-cleanup addition proves
reference reinitialization only.

Selection is called only with a nonempty online container, as required by the
retained source's iterator usage. Unresolved references are explicitly removed
before source clear/unlink: original `unlink()` calls the target callback even
when invalid. Safe Rust clearing of an invalid reference is tested separately,
not presented as a sanitizer-clean execution of that C++ path. Ordered event
traces observe actual `processThreatEvent` dispatches and compare
Rust's `ThreatChanged`, `OnlineStatus` and `Removed` return values. Accessibility
events are recorded separately and checked only as out-of-domain observations;
there is no Rust terrain-accessibility receipt or provider. This is not proof
of every runtime combat receipt.

## Fixture and checks

Each row is:

`command | offline_predicate current_guid dirty online_count offline_count aggro_callbacks [guid:threat_bits:valid:online:accessible:reverse_links ...] | dispatched_events`

Threat is the observed binary32 bit pattern. A zero current GUID means none.
Events are comma-separated `T` (threat change), `O0`/`O1` (offline/online), `R`
(removal), or `X0`/`X1` (accessibility change); `-` means none. Event order is the
actual source dispatch order, including `O0,R` for removal of an online ref.
`P guid raw_unit_state raw_static_flags2` executes only the source offline
predicate and records its output in the first result field. Other commands:
`N` new host, `A guid threat_bits`, `F guid raw_state`, `G raw_static_flags2`,
`U guid` reference refresh, `T guid` target-side refresh, `V guid` invalidate
and unresolve, `L guid` restore lookup, `X guid accessible`, `R guid` remove,
`C` clear, `D guid` target deletion (`1000` is the owner), and `S` explicit
selection. No source output is copied from Rust.

From the repository root:

```
python3 crates/benilla-sim/tools/hostile_reference_source/generate.py --check --sanitize --upstream /path/to/pinned/mangos-classic
python3 crates/benilla-sim/tools/test_hostile_reference_oracle.py
cargo test -p benilla-sim --test hostile_reference_oracle
```

Use the repository's required managed build wrapper where applicable. Drop
`--check` only to regenerate the authored numeric fixture. Missing compilers,
modified retained source/manifest, incorrect upstream hashes, source undefined
behavior in the exercised corpus and fixture mismatches fail rather than skip.
The exact upstream mixed `&&`/`||` expression is retained unchanged; only that
optional compiler parentheses warning is disabled.
