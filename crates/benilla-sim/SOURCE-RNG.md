# Source RNG engines and lifetimes

`source_rng` implements seed-sequence mixing, MT19937, process C RNG state,
and WSE random helpers from explicit inputs. It does not replace native RNG,
read entropy or clocks, use host libc RNG, start threads, serialize saves, or
admit a native world. Distributions, conversions, wrappers, and shuffle have
a separate [arithmetic and tracing contract](SOURCE-RNG-DISTRIBUTIONS.md).
Gameplay draw chronology remains outside both contracts.

## Source identities

CMaNGOS Classic is pinned to
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`.

- `src/shared/Util/Util.cpp:29-39` constructs a thread-local MT with
  `std::seed_seq { size_t(time(nullptr)), size_t(clock()) }`.
- `src/shared/Util/TSS.h:29-54` constructs only on the first get. Its Boost
  thread-specific pointer owns the engine. The captured Boost 1.74 header
  `boost/thread/tss.hpp:37-40,59-62,82-89` supplies deletion and reset cleanup.
- `src/game/World/World.cpp:845-849` seeds the separate process C stream.
- `src/game/World/WorldStateExpression.cpp:65-69,269-277` distinguishes
  ordinary calling-thread RANDOM from a fresh local MERSENNE_RANDOM engine.

The retained F1 build receipts identify Debian GCC/libstdc++
`12.2.0-14+deb12u1`, glibc `2.36-9+deb12u14`, and LP64 Linux. Algorithm
citations to `bits/random.tcc`, `bits/random.h`, and `bits/uniform_int_dist.h`
use the actual headers read from `benilla-cmangos-reference:bookworm`, not
line numbers from a different GCC installation. Full hashes and upstream
licences are in [PROVENANCE.md](PROVENANCE.md#source-rng-engine-families).

The glibc arithmetic basis is the exact Debian `2.36-9+deb12u14`
[source package](https://sources.debian.org/src/glibc/2.36-9%2Bdeb12u14/stdlib/).
The reviewed `random_r.c`, `random.c`, and `rand.c` bytes match the
[glibc-2.36 source tag](https://github.com/bminor/glibc/tree/glibc-2.36/stdlib).

## Seed sequence and MT19937

`seed_seq_generate(&[u32], &mut [u32])` writes the caller's output without
allocation. `bits/random.tcc:3262-3349` fills with `0x8b8b8b8b`, chooses
mixing offsets at lengths 7, 39, 68, and 623, and performs the source's
modulo-2^32 additions, subtractions, and multiplications. Empty output does
nothing. Seed material longer than output still participates in full.

`SeedObservations` carries signed LP64 time and process-clock observations.
Its two seed words preserve the source's size_t conversion followed by
seed_seq's low-32-bit reduction, including negative inputs. No observation
supplier runs before a lazy thread engine's first use.

`Mt19937::from_seed(u64)` narrows the scalar seed and applies the recurrence
in `bits/random.tcc:328-343`. `from_seed_seq` generates all 624 words and
applies only the seed-path zero-state repair in `random.tcc:354-389`.
`next_u32` preserves the in-place twist order and tempering in
`random.tcc:399-424,455-469`.

The captured C++ `result_type` is eight-byte `uint_fast32_t`, but this
engine's mathematical word size is 32, as declared in
`bits/random.h:1575-1582`. Every generated state word and raw result fits
`u32`. `MtState` stores all 624 mathematical words and a `u64` index.
This is a canonical arithmetic state, not the C++ object's memory layout.
Index 624 means exhausted, so the next draw twists. State restore refuses
indices above 624 and does not reseed or apply seed-only zero repair.
`state` borrows the full state, and `into_state` moves it without copying.

## Logical threads and process C state

A `ThreadRng` belongs to one caller-assigned logical thread birth, not a map,
NPC, OS thread number, or world seed. `get_or_init` calls its observation
supplier once, then returns that same engine on later gets. An unused
thread remains uninitialized. The caller must retain one owner for each live
logical thread and drop it at destruction. A later birth gets a new owner,
even if the OS reuses a thread number. `from_state` and `into_state` retain
both complete initialized state and the distinct uninitialized case.
There is no registry of OS threads or native scheduling implementation.

`ProcessCRand` represents the one process-wide stream. All logical callers
must share that owner in source call order. It implements only glibc TYPE_3,
with 31 words and front/rear separation three. Default state is the literal
pre-srand table from `stdlib/random.c:138-158`, equivalent to `srand(1)`.
`srand(u32)` maps zero to one, expands with the signed int32 seed and LP64
quotient/remainder arithmetic, then discards 310 words, following
`stdlib/random_r.c:162-207`. High-bit seeds must become signed before the
initial recurrence; unsigned modular exponentiation is not equivalent.
The source's `rand` forwards to `__random`, `stdlib/rand.c:23-28`.

`next_i32` adds state words modulo 2^32 and discards the low bit, following
`random_r.c:357-395`. Results are in `0..=INT32_MAX`. `CRandState` contains
all 31 word bit patterns and the rear index; front is always three slots
ahead. Restore rejects rear indices outside `0..31`. No other glibc RNG
type, global `srand`, host `rand`, host TLS, or implicit process reseed exists.

## WSE helpers

`wse_mersenne_random(arg1, arg2)` returns 1 immediately for `arg1 == 1`,
without constructing or drawing an engine. Otherwise it constructs a fresh
MT from `arg2`, substituting seed 1 when `arg2 == 0`, draws once, and returns
`raw % arg1 + 1`. The final `i32` cast retains the pinned compiler's signed
conversion, including negative results for large unsigned values.
Zero modulus refuses as `Error::ZeroModulus`, rather than executing the
source-undefined modulo. No thread or process stream can advance here.

`wse_random` implements only the ordinary WSE integer helper, using the
calling `ThreadRng`. It orders the uint32 arguments before the pinned int32
conversion. An inverted signed range refuses before initialization or a
draw. Accepted ranges use the shared distribution implementation described
in [the distribution contract](SOURCE-RNG-DISTRIBUTIONS.md), with a continuing
draw cursor and trace sink. Equal bounds still consume one raw word.

## Authored and private evidence

`tools/source_rng_oracle.cpp` uses the captured C++ headers independently
of Rust. `tests/fixtures/source_rng_gcc12.txt` has 251 rows, SHA-256
`0a8b73ec05901c05237dbc2eec6cc8fbac2b7f02cc16061adae58def09939499`.
It includes 48 complete seed_seq vectors, eight complete initial MT states,
120 selected raw words across twist boundaries, 20 local WSE results, and
50 ordinary WSE results plus five complete post-engine states. It asserts
the captured C++ integer storage widths. The default MT's 10000th output is
4123659995. Short known-answer seed_seq examples include empty material
with output length one yielding 3344521480 and `[1,2,3,4]` yielding
4146227409. These follow the cited seed_seq source, not Rust-generated
expected values. C tests check the known `srand(1)` sequence beginning
1804289383, 846930886, 1681692777, and source-derived high-bit seed vectors.

The optional private test reads `BENILLA_F1_RNG_TRACE`, or the original
prefix under this checkout's `benilla-config/full-server/f1/run/`. An absent
default file prints an explicit SKIP and performs no comparison. An explicit
unreadable path fails. Keep this skip separate from arithmetic-test passes.
With data present, it hashes the whole prefix, checks its 15371848 rows,
restores eight complete states, requires consecutive per-stream positions,
and recomputes every captured raw word. It accepts only original-prefix
SHA-256 `574813534352216ffbc4f7ca884db032bb3cfa08e292489438d60e1d9a8a836b`,
not another run or a mixture of captures. Expected private thread counts
are 1:8759, 11:35791, 12:15775, 13:1401, 14:2147, 15:381, 17:5331, 21:1.
The private addresses and kernel thread numbers are comparison-only keys,
not production logical identities. No private raw words enter the repository.

That 69586-word comparison is an engine projection of an unsealed execution.
It does not establish initialization causality, source consumer chronology,
whole-server replay, or C/WSE branch coverage in F1. It does not change
format `NATIVE_FORMAT_VERSION`, Profile, native State, production RNG selection, or capability.
