# App world-controller source harness

Run `cargo test -p benilla-world-controller-tests` and
`cargo clippy -p benilla-world-controller-tests --all-targets -- -D warnings`.
The harness includes the actual `benilla-app/src/net/p2p/world.rs` module by path.
Its build script extracts the used real `ClientCommand` variant declarations and
`MoveKind` from `net.rs`, preserving all fields. Unsupported renderer/UI-dependent
commands are represented only by the unmatched enum variant. There is no duplicate
controller or manually maintained shadow definition of supported commands.

Bindings and gameplay in the tests are original synthetic fixtures, never copied
native rows or an enabled Classic mode. Full app typechecking and graphical tests
remain separate obligations. See the [controller contract](../benilla-app/src/net/p2p/world/README.md).

The loaded straight/Chase regression fixtures carry the genuine runtime-issued
`OwnerIgnorePathfindingReceipt` through initial and retry dependencies, journal
encoding/replay and controller/checkpoint rollback. Its generation/profile/source binding proves
only the clear `0x10000000` bit from explicit interaction rows and the bounded owner
lifecycle. Missing or edited evidence cannot publish a route or mutate the retained
cache. The initial warm seed remains `SyntheticFixture`; it is not an authenticated
first PathFinder lifecycle, source nav/config proof or native admission.

The [distinct-actor quest loop](../benilla-app/src/net/p2p/world/native_controller/distinct_quest_loop_tests.rs)
composes native49 source-selected quest/combat/spell/loot semantics through that same
controller. Authored hostile A and combatable neutral questgiver B retain independent
full visitors and queues. Tests require both a kill and actual corpse-item collection,
then source-ordered requirement destruction/reward, including late-owner rollback,
durable publication faults and synthetic-spatial reopen boundaries. This composition
does not certify installed world content or open the native cache/production gates.
