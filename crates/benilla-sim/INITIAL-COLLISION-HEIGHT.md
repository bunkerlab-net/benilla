# Initial native-model collision height

Policy: `cmangos-8ec338a1-initial-collision-height-softfloat-v1`.

`ModelSelectionReceipt::collision_height(display_id, model_id, raw_height)`
returns an immutable, Serialize-only `InitialCollisionHeight`. It is minted
from the selection receipt retained by a fully validated initialization Plan.
There is no scalar constructor or Deserialize implementation. Its getters expose
policy, native display ID, model ID, positive effective object scale, raw height
and final height. IDs must match the receipt's selected native display and a
nonzero model ID. They establish consistency, not loaded-data provenance.

## Source arithmetic and boundary

Pinned CMaNGOS Classic `Unit.cpp:11930–11958`, `Unit::GetCollisionHeight`, loads
CreatureDisplayInfo by `GetNativeDisplayId()`, then CreatureModelData by its
ModelId. Its active computation is one binary32 multiply of `GetObjectScale()`
and CreatureModelData.CollisionHeight (raw DBC field 15). Only a product equal
to either signed zero becomes `DEFAULT_COLLISION_HEIGHT`, defined in
`ObjectDefines.h:47` as `2.03128f`, bits `0x4002007e`. The result is not clamped
to that default: finite negative nonzero heights stay negative here.

The Rust translation uses pinned `rustc_apfloat::ieee::Single`, nearest-ties-even
at the multiply. The existing SpellFloat domain admits signed zero and finite
normal binary32 with magnitude at most one million. Raw nonfinite, subnormal
or larger values are rejected before this API; underflow/arithmetic exceptions,
subnormal results and products outside the result bound are refused. Source
underflow-to-zero can select the native default, but this bounded translation
refuses the underflow instead of certifying it. Valid bounded inputs cannot
overflow binary32 to infinity; their product can still exceed the supported
one-million result bound. The oracle includes finite out-of-input-domain
heights that do overflow native binary32, and Rust refuses these as inputs.

`Object.h:364–367` defines `GetObjectScale()` as stored scale when nonzero,
otherwise `DEFAULT_OBJECT_SCALE` (`1.0f`). Both stored +0 and -0 therefore
produce effective 1.0. This does not widen the initialization receipt: `prepare`
requires all captured display scales to be positive, and selects positive
template scale or the first actual template display's positive scale when raw
template scale is nonpositive. Consequently every minted receipt has a positive
effective initial scale. Raw template ±0 is a template/DBC fallback, not a stored
zero object-scale witness. The oracle checks the extracted getter's ±0 repair
separately; all recorded fixture scales are positive effective initial scales.

The mount branch in the pinned function is commented out. Current display,
CreatureModelData.Scale, render/display scaling, SQL bounding radius/combat reach
and collision width are not height factors in the active branch. The oracle
sets irrelevant model Scale/CollisionWidth to distinct non-unit values and its
authored current-display getter differs from native display. Neither forms,
mount lifecycle nor later current object-scale/native-model changes are modeled
by this receipt. Respawn/reset or other substitutions require the caller to
invalidate/rebind its initial receipt as described in [CREATURE-INIT.md](CREATURE-INIT.md).

A consumer must bind the raw height and ModelId to its actual loaded DBC row,
compare retained display-scale observations, and retain selected content and
owner generation in its own private state. This arithmetic receipt alone is
neither a live-state witness nor native-world admission.

## Source, hashes and license

CMaNGOS Classic, CMaNGOS Project contributors, revision
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`, GPL-2.0-or-later.
Rust translation and authored synthetic oracle host: 2026 Benilla fork
contributors, GPL-3.0-or-later. The crate's preserved [authors](licenses/AUTHORS.md),
[copyright](licenses/COPYRIGHT.md), upstream [GPLv2](licenses/LICENSE),
[GPLv3](licenses/GPL-3.0.txt) and [provenance](PROVENANCE.md) apply.

The generator verifies these complete LF-normalized source SHA-256 values before
extracting anything. CRLF checkout normalization does not change source content.

- [Unit.cpp](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Unit.cpp):
  `03b67a4b6226f1f463e71be3694d3c950fc9b915a6e921407a98e4ff82fc21d0`
- [ObjectDefines.h](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/ObjectDefines.h):
  `8a5433945cf1a8016afa7ce9618cd0b2140721961acf05c44dee2b47cbff3491`
- [Object.h](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Object.h):
  `259cdb1a061258d6a8c3e977c743c8c83848c763353b504ca3e617580f58c128`
- [DBCStructure.h](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Server/DBCStructure.h):
  `354013c7508d7891f19a1856cd7bbafda19f18388bc20671f5692efa57ebdf50`
- [DBCfmt.h](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Server/DBCfmt.h):
  `8232159a33cd63fe473457f19ae9343aaf2c81a0e9a0c3ba0c3294e1637358e4`

The unchanged model structure and `CreatureModelDatafmt` retain field 14 width
and field 15 height as distinct native float columns. This oracle does not parse
a DBC file or validate a caller's loaded-reader binding.

## Independent oracle

Run `python3 tools/generate_creature_collision_height_fixture.py UPSTREAM_ROOT`
from this crate, or append `--check` to compare without overwriting. Add
`--sanitize` for undefined-behavior and float-cast-overflow sanitizers with no
recovery. No Rust expected-output generator, download or game-install access is
used. The only generated repository artifact is the numeric-only
`tests/fixtures/creature_collision_height_cmangos.test-fixture`.

The generator inserts the original height function, including its commented
mount branch, original object-scale getter, model structure, DBC format and two
default declarations unchanged into an authored host. Lookup stores, row values,
Object storage and Unit context are authored substitutes. This is not the original
translation unit or proof of its entire class layout, overload environment,
initialization, lifecycle, DBC loader or live execution. The existing creature
initialization oracle supplies separate model-selection/scale source evidence.

The audited profile is GCC 14.2.0, x86_64-linux-gnu, C++17, IEEE binary32,
FLT_EVAL_METHOD=0, FE_TONEAREST, FTZ=DAZ=0 and gradual underflow. Flags include
`-O0 -Wall -Wextra -Werror -march=x86-64 -mno-fma -fno-fast-math
-ffp-contract=off -frounding-math`. Negative FTZ, DAZ and rounding controls must
refuse before source height execution. Every run prints compiler identity,
full command, license/revision, complete source hashes, unchanged method/getter/
structure hashes, generated-harness hash and output hash. This is narrower than
cross-platform floating-point parity.

The 2,408 fixture rows record decimal u32 bits for effective scale, raw height,
raw native product and final native result. They cover signed-zero defaults,
negative heights, adjacent default and one-million result thresholds, tiny normal
inputs, subnormal and zero underflow, large invalid raw heights/native infinity,
and 2,048 deterministic synthetic rounding probes. Rust compares admitted rows
bit-for-bit, rejects 91 invalid raw-height inputs and rejects unsupported products.
Source-extraction tests cover modified hashes, LF normalization, unchanged
extraction, missing source/default/format and damaged/truncated fixtures.

Numeric fixture SHA-256:
`829d06885fa500e026d3ac4559d2b6cc0bdf4f23b688af61af2923e218472871`.
No game rows, DBC files, original full source files or assets are distributed.
