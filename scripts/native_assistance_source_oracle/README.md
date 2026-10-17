# Pinned assistance source oracles

This executable oracle compiles unchanged CMaNGOS Classic source at revision
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`. It is an independent source-side
observer, not a translation of Benilla's implementation. No Rust source or
binary supplies its expected outputs. It uses no database, install, network,
raw-data compiler, user assets, or production admission path.

## Run and verify

From the repository root, with the pinned CMaNGOS checkout in `../cmangos-source`:

```sh
python3 scripts/native_assistance_source_oracle/run_oracle.py \
  --core ../cmangos-source --output /tmp/benilla-assistance-oracle --check
python3 scripts/native_assistance_source_oracle/test_oracle.py \
  --core ../cmangos-source --output /tmp/benilla-assistance-oracle -v
```

A C++17 compiler with the requested sanitizers is required; absence is an error,
not a skip. `--compiler` selects it. Each invocation creates a fresh numbered
run directory, retaining the exact compiler and execution commands, version,
stdout/stderr/exit codes, source copies, authored inputs, output vectors and
summary. Failure logs are retained too. Compilation uses:

```text
-std=c++17 -O1 -g -Wall -Wextra -Werror -pedantic
-fno-fast-math -ffp-contract=off
-fsanitize=undefined,float-cast-overflow -fno-sanitize-recover=all
```

`--refresh-excerpts` refreshes bundled source excerpts only after checking all fourteen
pinned whole-file SHA-256 values. `--write-fixture` regenerates all three JSON vector fixtures
strictly from their respective source executables. Neither option reads runtime outputs.
Normal `--check` regenerates observations in scratch and requires byte-for-byte
identity with the bundled fixture. [verification.json](verification.json)
records the exact checked command and result for this fixture; it is reproducible
with the commands above and does not claim full workspace gates.

## Executed source and extraction guards

The full source files and each exact byte interval are recorded in
[source/manifest.json](source/manifest.json). Whole-file hashes are independently
fixed in `run_oracle.py`; a rehashed bundled excerpt/manifest does not bypass the
pinned source comparison. Excerpts retain their original bytes and indentation.

The compiled source includes:

- Full `Unit::SetInCombatState`, including alive/repeated-combat entry guards,
  AI callback placement, the assistance suffix, and `TriggerAggroLinkingEvent`
- Full `Creature::MarkCallAssistanceOnPull` and `CallAssistanceOnPull`, including
  the literal saved `m_AlreadyCallAssistance` boolean and ordered GUID snapshot
- Full `UnitLambdaEvent`, original `EventProcessor.cpp` and `.h`, and the exact
  Unit-update world guard/event-queue-update statements
- Full `Unit::CombatStop`, `Unit::SetDeathState`, `Creature::SetDeathState`,
  `Creature::RemoveCorpse`, and `Creature::Update`
- Exact assistance field constructor initializer tokens, flag accessors,
  `NO_CALL_ASSIST` constant, and `InitEntry` assistance-flag assignment

The separately retained `CombatHelpSuffix.inc` is placement evidence; the suffix
executes through the full unchanged `SetInCombatState.inc`, not a duplicate call.
Additional guards require the source hook/suffix order, literal saved boolean,
absence of event cancellation from the selected lifecycle bodies, and absence
of an overridden `UnitLambdaEvent::Abort`.

## Lifecycle shell and explicit limits

The standalone shell supplies declarations and inert callbacks required to compile
these selected methods. It is not a world server:

- In `oracle_driver.cpp`, `Cell::VisitAllObjects` receives an explicitly authored list of already-selected
  creature pointers. `AnyAssistCreatureInRangeCheck` and `CreatureListSearcher`
  have no geometry/eligibility implementation. Empty controls establish only an
  injected empty result; nonempty controls exercise scalar source branches and
  GUID order. Neither proves real loaded cells, map traversal, receiver
  eligibility, completeness, LOS, range or native content admission
- The source `GetMap()->GetUnit` and `GetAnyTypeCreature` calls resolve authored
  GUID controls. Receiver AI methods record callbacks without implementing AI,
  threat, movement or actual assistance. Nonempty controls are outside the
  initial runtime component's admitted empty-receiver domain
- The source AI `EnterCombat` call is logged, with no EventAI RNG/text execution.
  Raw ExtraFlags `2` is an input to the assistance flag assignment only; this
  oracle does not independently prove Defensive AI selection or sight behavior
- Linking, groups, instance callbacks, dungeon/zone combat, controllers/charm,
  critter flee, addons, loot, pools, created-by-spell effects and the new spawning
  system are absent. Reachable excluded paths fail rather than impersonating
  those systems. Unrelated death/visibility/aura/motion setters are inert
- Full `Creature::Update` runs its real death-state switch. `Unit::Update` runs
  only its exact world guard and event update statements. Unit spell/aura update,
  creature regeneration and map updates are not modeled
- The source `ScheduleAINotify` call during respawn is an explicitly inert hook,
  named `respawn_notify_shim_excluded_from_queue` in observations. Retention of
  assistance events is proven, but complete respawn notification queue
  composition is not. `ResetEntry` repeats only the exact assistance flag
  assignment; no creation/generation/addon claim follows
- `time(nullptr)` is supplied a deterministic authored wall-clock value of 1000.
  `map_clock` is a separate observed numeric input. Queue initial clock is an
  authored `uint64` initial state; subsequent queue advancement and deadlines
  use the original EventProcessor. No wall/map time is substituted for its clock
- Diagnostic IDs, inert enum labels and unrelated Unit-flag storage are shell
  declarations. Only `NO_CALL_ASSIST` is a raw content bit used in the lifecycle proof,
  and that constant is extracted unchanged. Numeric inputs are authored and are
  not claimed as ClassicDB content; no ClassicDB SQL is consumed

An observation wrapper owns each original event, forwards Execute/Abort/
IsDeletable and deletes it. The unchanged EventProcessor operates on that
wrapper to observe execution, abort, deletion and insertion order. Original
`UnitLambdaEvent` callbacks execute unchanged. Their inherited `BasicEvent::Abort`
is unchanged and has no effects. Wrapper event timestamps belong to the actual
source queue; inner lambdas do not inspect those bookkeeping members. Event kind
labels identify the two guarded source insertion sites; monotonic observation
IDs record order and do not substitute for a runtime allocator proof.

The source-only callback-entry case uses an authored event that invokes the
unchanged `SetInCombatState` while the source processor is draining. It proves
source zero-delay reentrant additions drain in that same update. It does not
extend the initial host domain: a host that cannot route those insertions to its
live queue must reject that entry before publication.

## Source observations and regression cases

`vectors.json` schema version 1 contains 23 cases and 86 observations. Each case
has authored `input` and source-executable `observations`. Each observation has:

- `phase`, `map_clock`, `queue_clock`, `death_state`, `owner_in_combat`
- `can_check_for_help`, `already_called`, `can_call_assistance`
- `queued` and cumulative `scheduled`, with monotonic `id`, `kind`, `add_clock`,
  and `deadline`; kinds are `ReenableHelp`, `AggroLinking`, or an authored control
- Cumulative `query_count`, `query_radius_bits`, `enemy_lookup_results`,
  `receiver_ai_events`, and `receiver_assistance_calls`
- Cumulative `executed`, `aborted`, `destroyed_without_execute`,
  `help_after_execute`, and inert lifecycle `hooks`

Source output establishes first-entry flag changes, positive help-radius override,
zero/negative-radius fallback scalar controls, NO_CALL_ASSIST, two source-ordered
equal-deadline lambdas, zero delay, earlier equal-deadline insertion, repeated hit,
queue/map lag, wide queue clock, missing enemy/receiver, literal false/true saved
boolean controls, CombatStop, corpse execution, DEAD freeze, respawn retention,
old captured state after reentry, forced/nonforced teardown and due-event abort.

In particular, saved false still queries and schedules both events. Saved true
can dispatch the authored receiver GUIDs; this is the pinned source's literal
boolean, not a negation. CombatStop resets the current field without replacing
captured payloads or resetting `CanCheckForHelp`. An old saved-true event can run
after respawn/reentry. Destroying or aborting either lambda does not Execute it,
so teardown does not reenable help.

`test_oracle.py` runs all three source executables with fixture equality first, then 53
regression/negative tests. Negative guards cover every pinned whole file, mutated
bundled excerpts, rehashed excerpt+manifest tampering, hook placement, negating
the saved boolean, lifecycle event cancellation and an added lambda Abort.
Runtime payload validation, rollback, persistence/replay, generation binding and
production refusal require separate runtime tests; this oracle does not replace
them.

## Candidate predicates: separate exact-source proof

`candidate_driver.cpp` is a second independently compiled source shell.
`candidate_cases.py` contains 55 authored input cases; `candidate_vectors.json`
contains their source-generated observations (58 candidate evaluations). The
existing 23-case/86-observation lifecycle fixture is unchanged, including its
explicitly injected, already-selected receiver list. The two executables are
not presented as an integrated source-world simulation.

The new executable compiles these complete unchanged excerpts:

- `AnyAssistCreatureInRangeCheck::operator()` from `GridNotifiers.cpp:265`,
  its declaration, and `CreatureListSearcher` declaration and `Visit` loop
- `CanAssistInCombatAgainst` (`Relations.cpp:1420`), `CanAssist` (`:458`),
  `CanAttackOnSight` (`:1144`), `CanAttack` (`:351`), `IsEnemy` and `IsFriend`
- Both static `GetFactionReaction` functions, full unit `GetReactionTo`, and
  `GetControllingPlayer`; faction reactions are computed from authored faction
  template fields rather than injected reaction labels
- `CanJoinInAttacking`, `Creature::CanInitiateAttack`, inline `CanEnterCombat`,
  `IsInCombat` and `IsFeigningDeathSuccessfully`, plus the original `UnitFlags`,
  `UnitState` and `ReputationRank` enums

Every original source file is whole-file SHA-256 pinned before extraction.
Manifest intervals retain exact bytes, and the same guards apply to all new
excerpts. Additional negative tests guard predicate order, the strict IsEnemy
threshold, reciprocal CanAttack, and absence of invented outdoor checks.

Each case specifies `authored_units`, their explicit faction masks and four-slot
friend/enemy arrays, and `authored_candidate_sequence`. The source `Visit` loop
runs over an adapter to that exact sequence. Observations record every attempted
candidate, acceptance, ordered leaf calls and selected GUIDs. Separate direct
source calls record each creature's reactions, CanAssist, CanAttack, IsEnemy and
CanAttackOnSight, including eligible objects omitted from the authored sequence.
Those diagnostic calls occur after collection and are not part of its leaf trace.

### What these candidate controls establish

- Pointer self-exclusion precedes every other predicate; a different object with
  the same authored creature entry is not excluded
- Outside instances, candidate combat state precedes CanAssist; the assisted
  owner's UNINTERACTIBLE flag precedes its reaction; CanAssist short-circuits
  before sight eligibility, range and LOS
- A neutral-to-player NPC can have `CanAttack == true` while `IsEnemy == false`.
  CanAttackOnSight and outdoor assistance therefore deny it before geometry
- Raw candidate ExtraFlags 0, 2 and 0x802 yield identical predicate observations
  for otherwise identical hostile candidates. These methods do not consult
  Defensive selection or NO_CALL_ASSIST; this does not execute AI selection
- Hostility precedes friendliness, reciprocal friendly masks/lists work, and
  slot zero enables or disables scanning all four faction list entries
- The source sight guard order is target evading, feign handling, disabled pet,
  CanAttack, then IsEnemy. Target attackability and bidirectional immunity flags
  can deny a hostile candidate before geometry
- Outdoor predicates do not add IsAlive, CanAggro or initiation-state checks.
  Corresponding controls keep the exact source behavior, rather than importing
  checks from CanAssistTo or the separate dungeon path
- Instanceability selects CanJoinInAttacking before the outdoor combat/assist/
  sight conditions. Dungeon controls cover entry capability, evading, initiation,
  feign/stun states, unattackable aura and CanAttack
- Successful semantic eligibility reaches the authored range outcome first, then
  authored LOS only if range succeeds. Semantic denial never queries either
- The source collection loop preserves the supplied reference order and repeated
  references. It does not invent membership: an eligible omitted authored object
  is not returned. Duplicate input references are a loop control, not a claim
  that a real GridRefManager can contain the same object twice

### Candidate shell boundary

Candidates are uncontrolled NPC creatures. Enemies are ordinary player, NPC or
pet controls. Masters, charms, reputation faction entries, forced reputation,
player ghosts/contested flags, duels, groups and FFA are absent. Source queries
for absent reputation data return authored null pointers and are logged;
unsupported reputation/PVP/controller callbacks fail if reached. Faction IDs,
entry IDs and masks are authored inputs, not ClassicDB or DBC extraction.

Instanceability, CanAggro, combat-manager evasion, ignore-feign capability,
unattackable aura and pet-disabled state are authored leaf states. Unit flag and
state masks use original enums; feign state uses the original inline accessor.
The shell's unrelated type/field selectors are diagnostic declarations.

Range and LOS return explicit authored booleans and record the exact querying
objects and binary32 radius. There is no coordinate, collision, map phase,
loaded-cell, map-membership completeness or LOS implementation here. The sequence
adapter is not source GridRefManager, Cell traversal, grid/world pass ordering,
cell order, or insertion order. No real traversal is faked or claimed.

This proof can support a separately validated runtime slice which rejects each
candidate semantically before range/LOS. It does not authorize admitting a
semantically eligible candidate without independent geometry, traversal,
receiver lifecycle and dispatch proof. Dungeon controls remain source-only
controls; their presence does not broaden an outdoor runtime admission domain.

## Peer help: separate exact-source CheckForHelp proof

`peer_help_driver.cpp` is a third independently compiled source shell.
`peer_help_cases.py` supplies 51 authored cases; `peer_help_vectors.json` retains
57 source-generated observations. The established lifecycle and candidate
fixtures remain byte-for-byte unchanged. No Rust runtime output is consumed.

The executable compiles the unchanged full `UnitAI::CheckForHelp` from
`UnitAI.cpp:495`, inline `Unit::getAttackerForHelper` and `GetVictim`, the source
`AttackerSet` typedef, both live Creature help/call accessors and setters, target
`IsConfused`/`IsStunned`, the source AI-order enum/accessor, and the complete
predicate/reaction source methods already used by the candidate oracle.
`World.cpp:473` supplies the compiled default configuration assignment, 5.0f.
The exact `MoveInLineOfSight` call site is retained as uncompiled placement
and radius-selection evidence; this shell does not execute that caller.
All fourteen relevant original source files are whole-file SHA-256 pinned.

These controls establish the source order, which differs from pull-assistance
candidate collection:

1. Resolve the target's active `GetVictim` before any attacker fallback; return
   if neither exists. An active victim rejected later does not trigger fallback
2. Check observer combat, observer CanCall, target CanCall, observer CanCheck,
   then target CanCheck. Repeated evaluations after exact setter calls read the
   live fields on both creatures
3. Check the target's retreating state, confusion flag, then stunned flag
4. Check observer CanInitiateAttack and CanAttackOnSight of the resolved victim
5. Ask the victim's accessibility and visibility relative to the observer
6. Ask observer-to-target range and LOS, with the source `true` LOS argument
7. Check CanAssistInCombatAgainst, then invoke the observed AttackStart endpoint
8. Only after AttackStart, inspect target AI and interrupt panic for ORDER_FLEEING

Explicit positive accessibility continues through visibility, range, LOS,
CanAssist and AttackStart. Independent negative cases stop at every stage and
prove short-circuit ordering. Late CanAssist denials occur after authored
geometry. Target flag/state-split controls intentionally expose that confusion
and stun read flags here, while observer initiation reads the source state mask.
They are scalar controls, not assertions that every split occurs in a real map.
CanAttackOnSight source controls cover evasion, feign, attackability, immunity,
and neutral-to-victim denial. Flee-order controls cover attack before panic,
non-flee orders, absent AI, and suppression after a preceding denial.

The source config default is 5.0f even with authored template CallForHelp values
31.0f, zero or negative; a separate explicit world-config override reaches the
range query. Template CallForHelp is not this method's radius source.

### Peer-help shell boundary

- Observer and target are authored uncontrolled NPCs; resolved victims are
  authored ordinary players. Existing reaction exclusions remain in force
- Source `AttackerSet` is an actual `std::set<Unit*>`, but inputs are limited to
  zero or one attacker. Active-victim precedence is proven against a distinct
  singleton fallback. No multi-attacker pointer order, source map ordering,
  discovery, completeness or target membership is inferred
- Observed boolean, victim-pointer and AI-order storage wrappers log reads while
  returning their stored authored values. Exact accessors/setters are unchanged.
  The wrappers supply no alternate predicates, geometry or source decisions
- Accessibility, visibility, range and LOS are separately named authored boolean
  leaves. Logs include victim/observer/target GUID roles, visibility arguments,
  binary32 radius and LOS argument. There are no coordinates, real collision,
  map phases, native navigation, loaded-cell checks or geometry implementation
- AttackStart and InterruptPanic record calls only; they do not execute combat,
  threat, motion, AI mutation or linking. Repeated observations clear diagnostic
  logs and preserve authored fields, not a real post-attack combat lifecycle
- The direct method does not check target combat or raw ExtraFlags. One control
  for each proves that boundary only. The retained caller requires an in-combat
  creature, and live CanCall fields may have been initialized elsewhere; these
  controls do not broaden any runtime admission domain
- The sibling ClassicDB checkout was separately verified at
  `ec4f596146be6467ea93c57397858e329e2db852`; no database rows are consumed or bundled.
  This is source-side scalar/ordering evidence, not playable-world proof

Negative guards reject changed victim priority, gate order, target roles,
LOS argument, AI order, config selection/default, template radius substitution,
and any drift in pinned whole files or bundled excerpts. Runtime transactional
refusal, persistence/replay, and admission remain separate obligations.

## License and authorship

The extracted source is by the CMaNGOS contributors and retains its original
GPL-2.0-or-later notice in
[source/COPYING-NOTICE.txt](source/COPYING-NOTICE.txt). Complete EventProcessor
copies also retain that notice in place. The original
[CMaNGOS authorship](../../crates/benilla-sim/licenses/AUTHORS.md),
[GPL version 2](../../crates/benilla-sim/licenses/LICENSE), and
[GPL version 3](../../crates/benilla-sim/licenses/GPL-3.0.txt) are retained by this
repository. The authored observer, scripts and shim are GPL-3.0-or-later; the
combined oracle is distributed under GPL-3.0-or-later. No game assets or database
rows are bundled.
