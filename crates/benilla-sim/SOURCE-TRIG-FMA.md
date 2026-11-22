# glibc 2.36 FMA binary32 sine and cosine

`source_trig_fma::sin_cos_bits` returns `[sinf, cosf]` result bits for all
binary32 encodings. Software round-to-nearest, ties-to-even arithmetic uses
`rustc_apfloat`, including explicit binary64 fused multiply-add.
`sin_cos_bits_native` instantiates the same kernel with native arithmetic and
explicit `f64::mul_add`. Native callers must retain their existing IEEE RNE,
gradual-underflow guard. Neither entry point calls host trigonometry.

The contract covers finite values, signed zeros, subnormals, both infinities,
and every NaN sign and payload. NaNs become quiet without changing their
other bits. Both infinities return `0xffc00000`. The API returns result bits,
not errno, exception flags, or alternate rounding-mode behaviour.
Bounded wrappers preserve the existing TAU, fallback, and Euler caller refusals.
Full-domain component computation does not grant historical execution admission.
Actual-process, acquisition-bound FMA dispatch remains a separate requirement.
Earlier recordings and separately built offline tools do not inherit that binding.

## Source and fused operations

The source is [GNU libc, tag glibc-2.36](https://github.com/bminor/glibc/tree/glibc-2.36/sysdeps/ieee754/flt-32).
`SOURCE_SHA256` lists these full-file identities in this order:

| File | SHA-256 |
| --- | --- |
| `s_cosf.c` | `2e60b691f1e4826da0f10057daecad243b3aaf2a8538ac7caee5c55bb763110a` |
| `s_sincosf.h` | `8155a4a1a21e947cae204802af76343686d8ede76963353b8192d8b3c4be4186` |
| `s_sincosf_data.c` | `0feab148a73db57ecbfbf6ae6ef4810ce1b2f0fd9788857ae6253636bc26a355` |
| `s_sinf.c` | `f0d27b880bc10eab4e27f2f79f19c2af1aaa1bc4cce2e1675c999a1f1f2d698d` |
| `sincosf_poly.h` | `df0bea6550598a828872c53d72f9f45473741de51f8e5e5617c1a6e27b86070e` |

The pinned libm SHA-256 is
`7f2ca87f652f56b094462474b076749e90e689d0ecb9cb63c7679820b271b4e7`.
Disassembly at cosine offset `0x73280` and sine offset `0x73720` determines
which operations fuse. The fast reducer uses one negative multiply-add at
`0x7331c` and `0x737b4`. Its quadrant conversion still truncates the scaled
product before integer addition and signed shift.
Each sine polynomial has three fused operations. Each cosine polynomial has
four. The large reducer retains the source's wrapping integer products,
192-bit 4/PI table, signed integer-to-double conversion, and separate PI63 product.
No polynomial coefficient or exceptional result comes from a host approximation.

The translated core retains LGPL-2.1-or-later and Copyright 2018-2022 Free Software
Foundation, Inc. The [original notice](src/source_trig_fma/GLIBC-NOTICE.txt) and
[complete licence](src/source_trig/GLIBC-LICENSE.txt) accompany it. Authored
adapters are GPL-3.0-or-later. `PROVENANCE.md` and the player packaging notice
list include this port. No code enters a permissive crate.

## Reproduce the gate

Run the gate from the worktree:

```sh
python3 crates/benilla-sim/tools/source_trig_fma_generate.py rustc-1.100/full-domain
```

The generator verifies the exact approved image and libm hashes before execution.
It requires four GiB of free disk space and never pulls or rebuilds the image.
The C++ oracle runs in throwaway read-only containers without network access.
Only the assigned evidence directory is writable. Rust compiles the production
kernel and native adapter with explicit FMA support. The oracle resolves the
image's actual `sinf` and `cosf`, not recompiled source or this Rust port.

Each exhaustive run evaluates all 4,294,967,296 inputs with both functions.
The default process must resolve offsets `0x73720` and `0x73280` and have zero
differences. The process with `GLIBC_TUNABLES=glibc.cpu.hwcaps=-FMA,-AVX2` must
resolve `0x72810` and `0x72250` and distinguish the port.
Each run hashes its complete ordered 68,719,476,736-byte result stream without
retaining it. Each input contributes four little-endian words, oracle sine,
oracle cosine, Rust sine, and Rust cosine. The receipt also hashes all retained
outputs and records both compile commands, source hashes, and resolved offsets.
The repository carries only authored vectors and the compact receipt.

The 83 authored vectors cover tiny branches, reduction thresholds, all ten
known assembler-domain differences, large finite inputs, and exceptional classes.
Tests compare both arithmetic adapters and retain the old offline profile's
exact ten differing cosine expectations. A separate strided test compares both
adapters across the full encoding range. That software-adapter sample is not
an exhaustive software-arithmetic proof.

The exhaustive FMA comparison has zero sine and zero cosine differences.
The masked comparison has 12 sine and 22 cosine differences. Both cover all
4,294,967,296 encodings. Their complete ordered result-stream SHA-256 values are:

- FMA: `a25d780e4bb26e24ff393b2fde06781a8d8928a98e04e61c9cab4c43743e2e47`.
- Masked: `d403a466db137e59cfeb555fd325d1b39a1b733bfa8fc3be65dd814e27f0a348`.

## Caller classification

The CMaNGOS revision is `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`.
These reference-process counterparts select FMA and retain every existing
angle refusal:

| Rust caller | CMaNGOS counterpart |
| --- | --- |
| `creature_motion/protocol.rs:125`, `WanderRequest::sampled_point` | `src/game/MotionGenerators/PathFinder.cpp:1367-1372` |
| `near_point/mod.rs:83,109`, initial and fallback candidates | `src/game/Entities/Object.cpp:2297-2304` |
| `source_transports/passengers.rs:84-88`, passenger position and offset | `src/game/Entities/Transports.cpp:567-588` |
| Runtime `production_compiler/location_inputs/collision.rs:49-65`, `model_point` | `src/game/vmap/ModelInstance.cpp:29-34` |
| App `products/vmap/trig.rs:10-20`, ModelInstance transforms through shared `live/math.rs` | `src/game/vmap/ModelInstance.cpp:31` |
| `benilla-native-vmap/src/live/trig.rs`, acquisition-bound live transforms | `src/game/vmap/ModelInstance.cpp:31` |

These separately built offline counterparts retain their existing unfused
profile. A reference-process receipt does not establish their libm dispatch:

| Rust caller | Offline counterpart |
| --- | --- |
| `benilla-native-vmap-source/src/assembled_spawn_bounds.rs:232-241` | `src/game/vmap/TileAssembler.h:48`, assembler |
| `benilla-native-vmap-source/src/doodad_transform_math.rs:4-12` | `contrib/vmap_extractor/vmapextract/model.cpp:223`, `Doodad::ExtractSet`, extractor |
| App `products/vmap/nav_mesh.rs:1-6,292-301` | `contrib/mmap/src/TerrainBuilder.cpp:559-770`, MoveMapGen |

The app ModelInstance wrapper retains its degree and assembler-radian bounds.
The live manager shares the lunar sine owner's actual-process receipt
validator. A matching round-14-or-later acquisition must record both FMA
`sinf` and `cosf` offsets in the pinned libm before gameplay. The runtime
authenticates the receipt bytes before binding its collision manager.
Without this capability the original signed-zero-only and domain refusals
remain. With it, full-domain reduction follows ModelInstance.cpp:31 and
G3D Matrix3.cpp:1655-1671 without a source-absent angle limit.
The shared matrix helper accepts the selected trig adapter; the app still
uses its unchanged bounded wrapper. No cached product identity changes.

The app updates the static VMap, location, and liquid policies, plus numeric
identities for initial ground candidates, empty-selector near points, resident
ground and dry-swimming near points, and same-polygon Chase/Home/Wander routes.
Location bindings now include the parent VMap numeric policy. Liquid bindings
already include both parent policies. This invalidates cached snapshots, replies,
route jobs, and derived policies carrying the old digests.
Existing fingerprint tests derive their identities rather than asserting fixed
digest literals. Their assertions stay unchanged. A new location regression
changes only the parent numeric policy and requires the old query to refuse.

The independent pinned-image degree oracle retains all 244 authored boundary
expectations and adds 14 signed degree vectors covering every known discriminator.
The existing 4,251,072-sample degree-grid test changes its SHA-256 expectation from
`c4a91f378dea062d0159217e6910b374c0ceacf90f644fe05d1a2ac465841998` to
`19a2791f8c35c9b147a8753bb2cd1e32f523b9028a55db830d10dce34d61e4be`.
Only degree inputs `0x44777ff0` and `0xc4777ff0` differ in that grid. They become
radians `0x418a3add` and `0xc18a3add`, among the ten known cosine discriminators.
Their cosine changes from `0xb794f76f` to `0xb794f770`. Sine is unchanged.
The old unfused degree digest remains with its separate offline fixtures.

Run `tools/source_trig_fma_degrees_generate.py rustc-1.100/degrees`
to reproduce the degree receipt, vectors, and streamed grid hashes.

Both generators create a new relative child of the assigned
`benilla-config/full-server/f6/trig-fma` directory. They refuse absolute paths,
parent traversal, every symlink component, and existing output directories.
Receipts from previous runs remain unchanged. Choose an unused run name when
repeating either command. Run
`python3 -B crates/benilla-sim/tools/test_source_trig_fma_evidence.py`
to check these output guards without starting an oracle.

