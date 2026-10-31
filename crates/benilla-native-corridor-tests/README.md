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

`home_component_tests` composes actual Chase, fresh HomeForceDestination
same-polygon routing, arrival/finalization, retained default Reset and subsequent
Wander. It also checks the distinct source-origin and corrected launch segment
against real collision geometry, with transactional rejection and retry.
`home_mailbox_tests` exercises the same loaded scene through the original 2 ms
service slices and 500 ms lease, cancellation, rollback and commit acknowledgement.
See [the selected Home contract](../benilla-world-runtime/HOME.md) for source
order, constructor lifetime, compatibility and remaining unsupported branches.
These authored tests do not establish normal populated-world startup.

```sh
mbx test -p benilla-native-corridor-tests home_ -- --test-threads=1
```

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

`reconstruction_component_tests` separately executes a
[bounded reconstruction recipe](../benilla-world-store/CACHE-RECONSTRUCTION.md)
through fresh actual providers. It retains accepted NOPATH before a normal leg,
saves mid-Wander, destroys the original mesh/provider, compares resumed arrival
and relaunch against uninterrupted execution, then saves and reconstructs again.
Stored replies alone fail concrete candidate acknowledgement. Version-2 recipes
can re-anchor only to an already durably saved, globally cache-free checkpoint.
Additional tests use actual death/respawn and Home Reset, then fresh queries and
exact continuation comparison. The death test uses a supported authored 1-ms
tick to cross 2048 total ticks without changing source lifecycle delays. The
2048-tick/8-MiB cap remains per retained history; active cache compaction,
indefinite arbitrary play and normal startup integration remain unsupported.
Ordinary direct loaded-checkpoint restore remains rejected.

Resident result transport probes use the original 120-second lease for native
preparation, independently of the two-second service slice. A preparation
deadline refusal retries the same live Work without counting as an injected
failure. The rejection hook runs only after concrete certification completes.
The post-mint expiry probe advances a job-local test clock to the original
deadline after certification. It does not cancel the job during preparation.
The probes still require genuine Facts and verify that rejected or expired
candidates publish neither staging authority nor a commit acknowledgement.

Source-location assertions compare native `Path` components. Windows backslashes
and forward slashes therefore identify the same expected source suffix, without
changing the recorded source location or its line and column checks.
