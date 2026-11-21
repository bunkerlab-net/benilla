# Independent cold-origin retained-straight observer

This test-only observer composes the existing guarded selected cold-policy
projection with the existing guarded original warm-straight source package.
It pins CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`, original
Detour translation units and G3D headers. It never links the production bridge.
Existing oracle packages, input manifests, geometry and observations are not
modified or reinterpreted as this new sequence's expected output.

## Two actual live sequences

Two separate original 1024-node queries and diagnostic PathFinder objects are
simultaneously live. Each starts from the original constructor's cache-only
initializer projection: limit 74, active zero, 74 zero references, 222 cached
floats and 74 smoothing references. A diagnostic constructor equality check
binds all those storage members. No complete owner constructor or owner query
pointer initialization is claimed.

Coordinates below are Detour YZX operands, not expected results:

- `normal`: selected cold `{5,0,5}` to `{6,0,5}`, followed by original retained
  straight `{5,0,5}` to `{15,0,5}`. The observer requires a real Replacement
  no-hit ray and a NORMAL result.
- `incomplete`: the same selected cold entry, original retained straight
  `{5,0,5}` to `{5,0,25}`, then `{5,0,5}` to `{15,0,5}`. It requires the first
  straight call to produce an actual hit/INCOMPLETE result and the subsequent
  no-hit call to retain that actual INCOMPLETE type.

Both sequences use explicit include filter 13, exclude 0, full collision
width `0.125f`, point limit 74, straight true, useStraightPath false, and
optimize/normalize/ignore-normalization/force false. The owner is nonnull.
Width is a supplied scalar dependency, not a claim about owner-model geometry.
Every call checks the same underlying query allocation; the wrapper never
assigns previous path type, reconstructs a prior cache or creates a new object
between steps. Only diagnostic counters are cleared.

The initial call uses the existing `cmangos-buildpolypath-zero-distance-v1`
policy, initializing just the two caller distances to positive zero. Original
uninitialized cold lookup is never invoked and no original-cold parity is
claimed. Every later call uses the original uninitialized caller declaration
with a required valid nonempty active scan, which defines both distances before
they can be read. The exact retained prefix, far predicate, selected suffix,
complete lookup/point/shortcut methods and disabled normalization are retained.

## Products and evidence

Actual execution creates `fixtures/retained-straight-origin/cases.test-fixture`
and `traces.test-fixture`, plus the new prefix's source-fragments package and
provenance JSON. No expected outputs are synthesized before execution.

The case schema is the ordered metadata columns `label`, `sequence`, `step`,
`oracle`, `query_nodes`, `include_filter`, `exclude_filter`, `point_limit`,
`after_start_storage`, followed by every original straight-case column except
its already included label. `FIELDS` in the generator is authoritative. There
are five rows: `normal_cold`, `normal_no_hit`, `incomplete_cold`,
`incomplete_hit`, `incomplete_no_hit`. Traces use the existing straight trace
schema unchanged. World XYZ points and scalar floats are hexadecimal binary32;
references are decimal; complete inactive storage and ordered tails remain.

Guards check exact schemas, domains, call order, predecessor storage/active/type
adjacency, explicit configuration, lookup append relationships, trace ancestry,
ray/height summaries, and binary32 full-width subtraction. Provenance binds
every original source file, both existing source packages, generator/harness
dependencies, exact source spans, authored geometry, harness and output hashes.
Compiler receipts retain every serial compile/link command, symbols, the copied
observed tile header and the sole audited original unused-variable warning.

The unchanged original dtMeshTile declaration retains its known C++20
raw-storage object-lifetime caveat. Normal/UBSan agreement is empirical, not a
standards-defined lifetime proof. The optional existing constructor-only repair
lane changes only that header declaration and must reproduce the same products.
No complete Calculate/BuildPolyPath, Chase dispatch, smooth fallback, active
world normalization, Route or gameplay admission is established.

## Managed build and reproduction

Use the existing official GCC 14.2.0/x86_64-linux-gnu toolchain and MBX exec.
The generator honors shell-tokenized `CXX` and requires its first executable to
resolve to the existing MBX compiler shim. It does not hardcode `/usr/bin/g++`.
`--build-dir` is mandatory, must be a new subdirectory under this checkout's
existing managed target symlink, and is never removed. Compiler temporary
files also stay there via TMPDIR. Original translation units are compiled
serially with the audited flags, then linked in their original order.

From the checkout root, after the parent has reviewed the generator and
established its serial process-group/free-space guard:

```sh
source ../tooling/env.sh
SOURCE="$(realpath ../cmangos-source)"
TOOLS=crates/benilla-native-nav/tools
python3 -B "$TOOLS/retained_straight_origin_test_generate.py" "$SOURCE"
CXX=g++ "$MBX" exec --project-root "$PWD" python3 -B \
  "$TOOLS/retained_straight_origin_generate.py" "$SOURCE" \
  --build-dir "$PWD/target/native126-oracle/normal"
CXX=g++ "$MBX" exec --project-root "$PWD" python3 -B \
  "$TOOLS/retained_straight_origin_generate.py" "$SOURCE" --check --sanitize \
  --build-dir "$PWD/target/native126-oracle/ubsan"
python3 -B "$TOOLS/retained_straight_origin_test_generate.py" "$SOURCE"
```

The noncompiling guards explicitly skip generated-product tests before the
first source execution; that is not a product-validation pass. Generation
refuses to overwrite existing products; later lanes use `--check` and distinct
managed build directories. Optional `--repair-tile-lifetime` lanes likewise
use fresh managed subdirectories and `--check`.

The explicit run floor is 1.25 GiB (1,342,177,280 bytes), checked before each
compiler/link/execution stage and before publishing products. A parent process
must additionally monitor disk space while a child command is running and
stop its process group if the floor is crossed. The generator does not change
MBX's separate GC configuration, install anything, download sources, create
an unmanaged target or delete scratch. No overlapping build is authorized.
