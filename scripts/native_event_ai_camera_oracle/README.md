# Pinned camera arithmetic oracle

This numeric-only oracle extracts unchanged GetDistance, _IsWithinDist and the
SAY visibility multiplier statements from CMaNGOS Classic
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`, `src/game/Entities/Object.cpp`.
The complete source file must have SHA256
`410904520fe2847a855c396d9fe442065268fd4ff5bb510633c3aadb5c54665b`.
No game database, client assets or loaded-cell membership are inferred.

Run `python3 scripts/native_event_ai_camera_oracle/run_oracle.py
--core /path/to/pinned-checkout
--runtime-source crates/benilla-world-runtime/src/native/event_ai/audience.rs
--output /path/to/scratch/oracle-runs` (one shell line). The compiler must support
C++17, strict IEEE float flags and the requested undefined/float-cast sanitizers.
Outputs retain source/excerpt hashes, compiler commands, inputs, source outputs
and independent binary32 comparison results. Failed runs retain their logs.

The runtime fixture `crates/benilla-world-runtime/tests/data/event_ai_camera.json`
contains the 44 source-generated edge cases. Its Rust test exercises the actual
camera validator, effective range and distance operation order. The broader
2,048 seeded cases are reproducible audit output, not bundled world content.
The membership fact in this numeric test is expressly authored; it does not prove
an actual map's loaded world-container traversal or runtime admission.
