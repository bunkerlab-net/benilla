# Pinned WMO doodad reference-container order

This independently authored C++ oracle observes the actual default
`std::unordered_set<uint16>` used by CMaNGOS Classic
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`. It establishes one explicitly bounded
implementation profile: Debian GCC 14.2.0-19, libstdc++ release 14/date 20250315,
Linux x86-64, C++17, identity integer hash, default equality/allocator, default
maximum load factor 1, fresh-empty composite swap, and single-key insertion only.
It does not infer order from the C++ standard or compare two implementations of a
ported ordering algorithm. Every expected vector comes from traversing the real
standard container. No Rust implementation supplies an expected result.

The ten compact fixture files contain 753 cases and 7,901 literal iteration
checkpoints, approximately 6.02 MiB total. There are no game/install inputs.

## Reproduce

```sh
python3 crates/benilla-native-geometry-tests/tools/wmo_doodad_order_generate.py ../cmangos-source
python3 crates/benilla-native-geometry-tests/tools/wmo_doodad_order_generate.py ../cmangos-source --check
CMANGOS_SOURCE=../cmangos-source python3 -m unittest discover -s crates/benilla-native-geometry-tests/tools -p wmo_doodad_order_test.py -v
```

`--check` regenerates into a temporary directory, checks the complete filename
set, and compares every byte without replacing expectations. The source revision,
selected original sources, all system header dependencies, compiler binaries,
linker tools, and runtime libraries are hash-guarded before compilation. A
preprocessing-only dependency pass also rejects a changed header dependency set.
The installed recognized system compiler compiles only this authored harness;
no downloaded compiler, extractor executable, or Rust build is involved.

The [source manifest](wmo_doodad_order_source_hashes.sha256),
[header manifest](wmo_doodad_order_header_hashes.sha256), and
[compiler/runtime manifest](wmo_doodad_order_toolchain_hashes.sha256) are explicit
pins. A different implementation needs a separately reviewed profile and oracle;
silently updating these guards to accept it would change the contract.

## Original source and implementation boundary

The generated harness uses these exact guarded original declarations/statements:

- `uint16`/`uint32` typedefs from `src/shared/Platform/Define.h`
- The WMO `MODS`/`MODD` definitions and entire `WMODoodadData` structure from
  `wmo.h`, with unchanged original `vec3d.h` included by the compiler
- The `WMODoodadData& doodads = WmoDoodads[plain_name]` lookup and
  `std::swap(doodads, froot.DoodadData)` from `vmapexport.cpp`
- Its single-key `doodads.References.insert(groupReference)` statement
- `model.cpp`'s range-for over `doodadData.References`

[Extracted fragments](fixtures/wmo_doodad_order_source/source-fragments.test-fixture)
retain the original notice. Only input acquisition, a root wrapper containing the
original data member, observation, and output serialization are authored. The
real original composite type is swapped through a real named map entry. Both
initially empty and previously full 65,536-key map entries are exercised. The
displaced old object retains its original size/capacity, while the new collection
is observed empty with one bucket and default load factor.

The implementation mechanism resides in guarded libstdc++ headers:

- `bits/unordered_set.h` selects `_Prime_rehash_policy`, modular bucket indexing,
  and unique-key table traits
- `bits/functional_hash.h` defines integer hashing; identity is also checked at
  runtime for every one of the 65,536 possible keys
- `bits/hashtable.h` defines bucket-front insertion and unique-key rehash list
  relinking, including predecessor updates
- `bits/hashtable_policy.h` declares the prime policy and hash/cache traits

The prime growth decision is **out of line**. Its implementation is not guessed
from a header declaration or a nearby GCC release. The actual executable's
undefined dynamic symbol is verified as
`_ZNKSt8__detail20_Prime_rehash_policy14_M_need_rehashEmmm@GLIBCXX_3.4.18` and the
resolved runtime is verified as
`/usr/lib/x86_64-linux-gnu/libstdc++.so.6.0.33`, SHA-256
`972bb2a18b71140dab0240f8a1f68ab3fb1d56bcd4c4f824a91b70888faf5a00`.
An actual ascending container records bucket count after every distinct insertion
through 65,536. Four independent full permutations then compare every cardinality
against that complete observation, totaling 262,144 checks.

[Dependency provenance](fixtures/wmo_doodad_order_source/dependencies.tsv) records
276 translation-unit dependency paths, including generated source and the
original header, plus source/compiler/runtime byte sizes and hashes. Temporary
and checkout paths are normalized. [Provenance](fixtures/wmo_doodad_order_source/provenance.tsv)
records the compiler version, exact flags, linked prime symbol, source hashes,
runtime hash, and compiled executable hash.
[Container observations](fixtures/wmo_doodad_order_source/container_profile.tsv)
record macros, ABI/debug settings, type/node/policy sizes, noncached fast integer
hash traits, load factor, and measured coverage. The flags are
`-std=c++17 -O0 -fno-strict-aliasing -Wall -Wextra -Werror`; debug and assertions
modes are absent, and the C++11 ABI macro is 1.

## Complete observed bucket schedule

The first column is the distinct cardinality at which the new bucket count first
appears. Between rows, bucket count is unchanged, including through cardinality
65,536 after the last row. These are observations of the linked container.

| Distinct count | Buckets |
| ---: | ---: |
| 0 | 1 |
| 1 | 13 |
| 14 | 29 |
| 30 | 59 |
| 60 | 127 |
| 128 | 257 |
| 258 | 541 |
| 542 | 1,109 |
| 1,110 | 2,357 |
| 2,358 | 5,087 |
| 5,088 | 10,273 |
| 10,274 | 20,753 |
| 20,754 | 42,043 |
| 42,044 | 85,229 |

The largest observed bucket count is 85,229, below the independent bounded
production profile's 131,072-slot capacity. This is a bucket-count observation,
not a claim that the allocating standard container uses less than 1 MiB. A
compact production representation must prove its own working-storage bound.

## Authored cases

- Empty, zero/max singleton, repeated keys, and mixed bucket-front insertions
- Forward/reverse collision-heavy sequences with strides 13, 29, 59, 127, 257;
  every prefix is recorded for these bounded sequences
- All 720 permutations of `[0, 1, 13, 26, 29, 65535]`, with every prefix recorded
- Twelve duplicate-at-capacity cases for old capacities 13 through 42,043. Each
  records initial state, the full old capacity, the same state after a duplicate,
  and the following new unique insertion that actually grows the table
- Populated-root replacement with empty and short new collections, plus a full
  affine case that also begins by displacing an old full collection
- Full ascending, descending, affine, and deterministically shuffled permutations
  of all 65,536 keys. Each has a complete reverse-order duplicate pass, with exact
  final iteration equality checked before/after the pass

The affine order is `(index * 65521 + 12345) mod 65536`. The shuffle is authored
Fisher-Yates from 65,536 down to 2, using xorshift32 seeded by `0x6d2b79f5`, shifts
13/17/5, and modulo the remaining length. Inputs are stored literally, so the Rust
consumer does not need to implement either generator.

Full cases record only zero, before/at/after each measured rehash transition,
65,535, 65,536, and the end of the duplicate pass. Capacity-edge cases record only
their four meaningful checkpoints. The oracle never serializes a full 65,536-key
collection after every insertion; iteration work/output is bounded linearly by
these checkpoints, rather than quadratically.

## Fixture schemas

All TSV files have a `# `-prefixed schema row. Integer fields are decimal. Binary
streams contain only unsigned little-endian 16-bit words, without a header.
Offsets and counts are **u16 element units, not byte offsets**.

- `cases.tsv`: `case`, `input_offset`, `input_count`, `prepopulate_count`
  - The literal input slice belongs to `insertions.u16le`
  - Old map entries receive ascending keys `[0, prepopulate_count)` before the
    original composite empty-swap; old values are not part of the new insertion
    stream
- `checkpoints.tsv`: `case`, `after_inserts`, `distinct_count`, `bucket_count`,
  `iteration_offset`, `iteration_count`
  - `after_inserts` counts all attempted insertions, including duplicates
  - The literal expected traversal slice belongs to `iterations.u16le`
  - `iteration_count` equals `distinct_count`; a zero-length slice is valid
- `bucket_schedule.tsv`: `distinct_count`, `bucket_count`
  - The complete change-point encoding of all 65,537 observed cardinalities
- `swap_checks.tsv`: `case`, `prior_distinct`, `prior_buckets`, `reset_distinct`,
  `reset_buckets`, `displaced_distinct`, `displaced_buckets`
- `container_profile.tsv` and `provenance.tsv`: `item`, `value`
- `dependencies.tsv`: `kind`, `path`, `bytes`, `sha256`
- `source-fragments.test-fixture`: exact original data declarations and relevant
  collection statements, with original copyright/license notice

There are 613,908 literal input words and 2,378,875 literal expected iteration
words. All slices are contiguous, exhaust their corresponding file, and are
checked for cardinality, membership, bounds, and duplicate preservation. The
Python integration tests execute the source oracle again and byte-compare the
committed fixtures; the production-side fixture replay is a separate consumer.

## Scope and notices

This profile establishes only the source collection's order under the documented
lifecycle and toolchain. It does not establish a portable `unordered_set` order,
another standard library/compiler version, another hash/allocator/load factor,
reserve/erase/explicit-rehash behavior, filtering correctness, archive/listfile
acquisition order, transforms, extraction-wide placement IDs, directory assembly,
native installation compatibility, or gameplay admission.

Authored harness/generator/tests are GPL-3.0-or-later. Extracted CMaNGOS fragments
retain GPL-2.0-or-later notices; see the existing
[CMaNGOS notice](../../benilla-app/src/net/p2p/world/native_geometry/products/CMANGOS-NOTICE.txt).
The system libstdc++ headers/runtime remain installed system dependencies; their
implementation is not copied into the fixtures.
