# Concrete native corridor component tests

This test-only crate includes the application's actual `LocalCorridor`, loaded
native route adapters and parsed authored terrain fixture. Its small Bevy host
runs real terrain residency, collider membership, Avian body/hull checks and LOS
without compiling the complete application test unit. It does not substitute
planar support for body clearance.

`wander_component_tests` composes those dependencies with the native runtime's
selected random default generator. The authored entry-6-shaped scene retains
MovementType 1, dry habitat 3 / CanSwim, filter 13, immutable model/addon owners
and source-owned RNG. Its inherited reaction/faction profile is authored
Aggressive/faction 2, not the real entry-6 neutral reaction. Map positions,
dimensions, content, visibility and visitor membership are authored inputs. No Classic installation or production native
admission is established by these tests.

The exact source and deterministic endpoint compatibility rule are documented in
[the runtime contract](../benilla-world-runtime/WANDER.md). The test fixture's
streamer setup feature is a development dependency only, not a player capability.
Normal player dependency graphs do not enable it.

Run the selected loop with the repository's build wrapper:

```sh
mbx test -p benilla-native-corridor-tests wander_component_tests
```

Concrete HomeForceDestination routing remains outside the loaded provider's
supported domain. Motion-owner Home Reset tests establish retained move-count
and replaced PathFinder behavior separately; they do not establish a concrete
fight/return-home/Wander-resume loop.

The visibility interruption scene uses an authored 400 ms initial notification
delay to exercise actual aggressive AI combat between spline progress and the
400 ms map-position publication. It checks exact AI/source relocation, suspended
Wander ownership, transactional rollback and replay.

Initialize records semantic constructor identity; first Prepare materializes
constructor dependencies from current owner inputs and an already-loaded query.
Historical initialization-time constructor observations are not established.

Loaded native-cache checkpoints round-trip through encoding but intentionally
reject direct restore and saved-session reopen. Replay tests restore cache-free
genesis, replay to each intermediate checkpoint, compare it exactly, then replay
the remaining receipts. This does not establish native allocation rehydration.
