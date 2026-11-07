# Initial player model scale and collision height

Policy: `cmangos-8ec338a1-initial-player-model-softfloat-v1`.

`player_model::initial_collision_height(InitialPlayerModelInputs)` evaluates
already-selected native player display/model fields. Its immutable
`InitialPlayerCollisionHeight` is Serialize-only, with no scalar constructor,
Deserialize, public fields or mutation methods. Getters retain native display
ID, model ID, raw display scale, raw model scale, raw collision height, raw scale
product, stored object scale, effective getter scale and final height.

These public raw inputs and receipt establish arithmetic consistency only. A
runtime must privately bind the selected display and model to genuine loaded
DBC rows and its initial player identity, catalog and generation. IDs alone
establish none of that. This primitive does not select race/class/gender, load
DBC data, supply game rows, certify current body state, or lift native-world
admission. An ordinary NPC selection receipt proves a different scale law and
cannot substitute for this initial player computation.

## Exact source sequence

Pinned CMaNGOS Classic revision
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`:

1. `Player.cpp:17491–17526`, `Player::InitDisplayIds`, selects the male or female
   display from PlayerInfo, looks up its CreatureDisplayInfo and ModelData,
   multiplies `displayInfo->scale * modelData->Scale` in binary32 and passes it
   to `SetObjectScale`. It sets both native and current display IDs. Display
   scale is raw CreatureDisplayInfo field 4; model scale is raw ModelData field 4.
   `Player.cpp:2262–2266`, `GetNativeScale`, returns the same raw product.
2. Fresh Create and login initialize field storage through `Object::_Create`
   and `_InitValues` (`Object.cpp:105–124`) before `InitDisplayIds`.
   `_InitValues` zero-fills the fields. `SetObjectScale` (`Object.cpp:128–131`)
   calls `SetFloatValue` (`Object.cpp:872–882`), whose assignment happens only
   when old float value differs from new float value. Therefore a raw -0 scale
   product stays -0 in the receipt's `raw_scale_product`, but either zero
   product leaves the fresh stored scale at +0. A nonzero product is stored.
   Fresh player paths call `_Create` at `Player.cpp:750` / `14079`, then
   `InitDisplayIds` at `794` / `14131`, before `InitStatsForLevel` at `835` /
   `14433`; no preceding path writes object scale. This is an explicitly fresh
   initialization scope, not arbitrary reinitialization of an existing object.
3. `Object.h:364–367`, `GetObjectScale`, returns the stored value if nonzero,
   otherwise `DEFAULT_OBJECT_SCALE = 1.0f`. Both stored +0 and -0 select one.
   The fresh receipt stores +0 for zero products; the oracle separately checks
   direct getter inputs with both stored zero signs.
4. `Unit.cpp:11930–11958`, `GetCollisionHeight`, loads the native display's model,
   then computes a second binary32 product: effective object scale times raw
   ModelData.CollisionHeight (field 15). Only an exact signed-zero product
   selects `DEFAULT_COLLISION_HEIGHT = 2.03128f` (`0x4002007e`), from
   `ObjectDefines.h:47`. Negative nonzero heights remain negative; this is not
   a positive-body-domain validator and does not clamp heights to the default.

The two products round independently, nearest ties to even. Reassociation or
one rounding of the exact triple product is incorrect. For authored input bits
`0x3e1dc131`, `0x40b448dc`, `0xbf1a4e8b`, the stored scale is `0x3f5e3179`
and height is `0xbf05edef`; reassociation or a single final rounding gives
`0xbf05edee`.

The active height function's mount branch is commented out. Current display,
model collision width, render sizing and NPC template scale do not participate.
Initial player model scale participates once, through initial object scale;
it is not multiplied into height again. Later aura/form/current-scale changes,
GM gender/reset reinitialization, load replacement and ownership transitions
require a new appropriate live-state witness and invalidate this initial one.

## Bounded software arithmetic

The port uses `SpellFloat` for all raw fields and
`rustc_apfloat::ieee::Single` for both products, with explicit nearest-ties-even
rounding and no host floating-point operations. The accepted input/result domain
is either signed zero or finite normal binary32 with absolute value at most
1,000,000. Nonfinite, subnormal and larger raw encodings are refused; missing
zero IDs are refused as inconsistent identities, not native loader repairs.

Each intermediate must remain in that bounded domain. Arithmetic exceptions
other than inexact, exact subnormal results, and results outside the bound are
refused. In particular native underflow-to-zero could trigger the scale getter
or height default; the bounded port refuses that underflow instead of claiming
parity there. An out-of-bound stored scale is refused even if multiplication by
small raw height would later bring the final result back into range. Native
exceptions and results outside this domain are documented source behavior,
not admitted Rust results. Negative finite values and signed-zero inputs are
accepted for source arithmetic; a consuming runtime may require positive body
sizes under its separate admission contract.

## Source, provenance and licenses

Source copyright: CMaNGOS Project contributors, GPL-2.0-or-later. Rust
translation and authored host: 2026 Benilla fork contributors,
GPL-3.0-or-later. The preserved [authors](licenses/AUTHORS.md),
[copyright](licenses/COPYRIGHT.md), upstream [GPLv2](licenses/LICENSE),
[GPLv3](licenses/GPL-3.0.txt) and crate [provenance](PROVENANCE.md) apply.

Before extraction, the generator verifies complete LF-normalized source
SHA-256 hashes (CRLF normalization preserves source content):

- [Player.cpp](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Player.cpp):
  `d02d693f2575b0f47be405fe6e81e2a4e833fca8d149354d9cb8d3b458527c7e`
- [Unit.cpp](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Unit.cpp):
  `03b67a4b6226f1f463e71be3694d3c950fc9b915a6e921407a98e4ff82fc21d0`
- [Object.cpp](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Object.cpp):
  `410904520fe2847a855c396d9fe442065268fd4ff5bb510633c3aadb5c54665b`
- [Object.h](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Object.h):
  `259cdb1a061258d6a8c3e977c743c8c83848c763353b504ca3e617580f58c128`
- [ObjectDefines.h](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/ObjectDefines.h):
  `8a5433945cf1a8016afa7ce9618cd0b2140721961acf05c44dee2b47cbff3491`
- [DBCStructure.h](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Server/DBCStructure.h):
  `354013c7508d7891f19a1856cd7bbafda19f18388bc20671f5692efa57ebdf50`
- [DBCfmt.h](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Server/DBCfmt.h):
  `8232159a33cd63fe473457f19ae9343aaf2c81a0e9a0c3ba0c3294e1637358e4`
- [SharedDefines.h](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Globals/SharedDefines.h):
  `4832b8e0da45211143acae7c42173fea42732e2acdbf190692725e51b020bfe8`

## Independent extracted-source oracle

Run `python3 tools/generate_player_model_fixture.py UPSTREAM_ROOT --check`
from this crate. Omit `--check` only to regenerate the numeric fixture; add
`--sanitize` for undefined-behavior and float-cast-overflow sanitizers with no
recovery. `python3 tools/test_player_model_oracle.py` checks extraction and
fail-closed guards without compiling. No downloads, game-install access or
Rust expected-output generator are involved.

The generator inserts complete, unchanged `Player::InitDisplayIds`,
`Player::GetNativeScale`, `Object::_InitValues`, `Object::SetFloatValue`,
`Object::SetObjectScale`, `Object::GetObjectScale`, `Unit::GetCollisionHeight`,
original backing-storage declarations, display/model structures, Gender enum,
DBC formats and default constants. This is an authored lookup/storage host,
not the original translation units, full object layout, native DBC loader,
full Create/login sequence or live lifecycle. The source `_InitValues` and
setters actually execute; the authored constructor chooses the verified fresh
initialization boundary. Authored current-display ID differs from native ID
before height evaluation. Unused collision widths differ from heights.

Audited execution profile: GCC 14.2.0, x86_64-linux-gnu, C++17, IEEE binary32,
FLT_EVAL_METHOD=0, FE_TONEAREST, FTZ=DAZ=0 with gradual underflow. Flags:
`-O0 -Wall -Wextra -Werror -march=x86-64 -mno-fma -fno-fast-math
-ffp-contract=off -frounding-math`. Negative FTZ, DAZ and rounding controls must
refuse before source execution. Runs print compiler identity, invocation,
licenses, full source/extraction/generated-harness hashes and fixture hash.
This profile alone is not cross-platform native floating-point parity.

The 6,796 numeric fixture rows contain decimal u32 values in this order:
gender, native display ID, model ID, raw display scale bits, raw model scale
bits, raw height bits, raw scale product bits, stored scale bits, effective
getter scale bits, raw height product bits, final height bits. All rows are
authored, with no game data. Both genders, both zero signs in all raw operands,
non-unit/negative scales, negative heights, independent rounding, near-bound
results, subnormal/zero underflow, invalid raw fields and 4,096 deterministic
rounding probes are covered. Rust compares 6,230 admitted rows bit-for-bit;
48 invalid raw rows, 244 unsupported scale products and 274 unsupported height
products fail closed. Admitted rows include 440 negative-zero raw scale
products, 436 exact-zero height defaults and 2,580 negative final heights.

Numeric fixture SHA-256:
`ddb156c1a44a422b757ccc33a21f9d50f5b4b2686cba540249132ce6389e994d`.
Generated harness SHA-256:
`b3cfe0166f28382db05e5932aa8b49d8c06477241eb1ed6202abe1c90494e2b4`.
The only generated repository artifact is
`tests/fixtures/player_model_cmangos.test-fixture`. Source-extraction tests
cover changed hashes, LF normalization, missing source/constants/formats,
missing/repeated insertion markers, exact insertion and corrupt/truncated
fixture refusal. Rust compile-fail examples enforce receipt opacity.
