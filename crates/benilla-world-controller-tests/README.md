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
