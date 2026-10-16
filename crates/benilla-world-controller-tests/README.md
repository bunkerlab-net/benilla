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
