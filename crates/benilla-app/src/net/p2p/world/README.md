# Solo-world app boundary

`p2p::run_character` routes an explicit `Some(world::Request)` through its native
entry constructor and this controller's real app command driver. `None` selects
existing exploration. A failed gameplay request never selects exploration, realm
login, another world, or new-world creation. The current character browser passes
`None`; there is no supported native compiler request producer yet.

`Request::new` captures exact content, rules and an immutable `ClientCatalog`;
it cannot supply an arbitrary controller. Entry checks the already-open profile's
owner, persisted selected character, explicit WorldId, native mechanics profile,
runtime rules and catalog. Original fixtures are rejected by the production entry;
native entry currently returns `NativeMechanicsUnavailable`. Native source audit
reports do not constitute a playable mechanics capability. A future profile must
distinguish known-but-unavailable active abilities from absent/invalid IDs, preserving
spellbook visibility and explicit unavailability. Unresolved passive/stat/item effects
still block entry. Only tests call the
private lower-level constructor with authored fixture bindings.

## Ownership and durability

The controller owns one `World`, `Projection` and exclusively locked `WorldStore`.
The app resolves its root through `local_state::home`; the fixed leaf is
`p2p-worlds/<32-hex-character-world-id>`. The full restored roster must contain
exactly the selected profile owner/character. Profile appearance/name still drive
the avatar; authoritative world fields are merged before its first ObjectCreate,
and subsequent projection updates supply progression fields. Gameplay entry clears
any exploration ticket and never starts its network. The capability remains solo
until an authenticated join-boundary and membership protocol exists.
No world checkpoint, movement or gameplay field is written to `p2p-profile`.

Create and Open are separate actions. Catalog/runtime/identity validation precedes
world directory creation. Open validates retained history and fails on absent,
corrupt, incompatible, foreign-roster or locked state; it never creates or recovers
implicitly. The store's typed commit receipt is required before acknowledging the
journal. Simulation is paused during its synchronous prepare/commit/ack sequence.

Save every ten simulated seconds and when `checkpoint_due()` warns first. Disk
failure pauses at the current boundary, preserving the world, journal and pending
intentions. An explicit retry can resume after a definitely pre-publication error.
An ambiguous/external-modification result remains blocked for explicit recovery;
retry does not replace the in-memory world or switch saves. A paused wall-clock
interval does not become simulation debt.

Orderly logout cancels unadmitted intentions and commits every admitted tick. It
never manufactures future ticks to drain a burst of input. A failed ordinary logout save
keeps the controller, journal and store lock alive in a closing/paused driver until
explicit repair/retry succeeds; no success receipt or logout confirmation is
emitted. Process stop or app-channel loss attempts a final checkpoint and reports
any failure, but cannot promise to retain memory after the application exits. Process termination can still lose progress since
the last successful checkpoint. Normal periodic loss is up to ten simulated
seconds; after a save failure, that unconfirmed boundary remains in memory only.
Store platform guarantees are preserved: file and directory sync on Unix, file
sync with directory best-effort on Windows. Hardware/filesystem must honor sync.

## Input and presentation

The actual `ClientCommand` mapper handles direct self/NPC spells, explicit quest
accept/fixed reward, independent query panels, and generation-captured loot
open/money/slot/release. Queries cannot grant rewards. Unsupported forms return
errors, including ground/item/corpse casts, nonzero reward choice and the stock
corpse-release command. The runtime's original revive shortcut is never substituted.

A maximum of 64 FIFO actions is retained. Movement replaces one latest floating
pose, validates finiteness, then quantizes at admission and produces bounded integer
deltas each tick. Casting/death blocks discard the predicted goal and send a
self-mover correction immediately, including a goal queued before the blocking
tick. Movement and eligible melee alternate fairly with queued actions;
the single owner still admits at most one command per tick. NPC handles capture
the exact generation on receipt. Loot money/slot commands capture the then-active
window, so opening another corpse cannot retarget a queued click.

Autoattack schedules individual eligible melee intentions and emits start/stop
state; a stop, refusal, dead player or removed/dead target cancels it. This does not
synthesize native melee combat logs or claim complete Classic combat animation.
Every committed tick runs `Projection::tick`, including empty ticks. Selected-player
correction is `UnitMove`, routed by the app's existing self-mover pipeline. Facing
is presentation-only and is not persisted as authoritative runtime state. Native
collision, corpse-run and resurrection semantics remain gated.

The host monotonic clock determines due ticks only. A pump runs at most 32 ticks,
retains all catch-up debt and yields for input/stop checks. Input count cannot add
game time; configured/offline peers do not participate in a solo quorum. Event
channel failure stops execution and attempts a final durable checkpoint.

## Source-level tests

`cargo test -p benilla-world-controller-tests` compiles this exact controller,
mapper, driver and explicit dispatcher without linking Bevy. Its build script
extracts the relevant actual app command variant declarations, including fields;
it does not maintain a second handwritten input enum. The tests exercise original
synthetic bindings, profile identity checks, checkpoint faults, more than 10,000
ticks, partial loot/reload, query/reward separation and channel-driven pause/retry.
This harness is not graphical/native parity evidence. The production app still
requires its normal full build/lint and the user-local data/rendering tests.
