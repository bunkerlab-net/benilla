# Bounded VMap source acquisition

`prepare(data_path, map_id, cancelled)` reads a local Classic MPQ chain and returns
private, complete raw model candidates, ordered raw placement inputs and selected
doodad membership candidates for a selected map. It writes nothing. This library is not a player CLI and does not
assemble a map, grant collision-clearance authority, or open gameplay admission.

Preparation performs the required LiquidType table check, the global per-archive
listfile/WMO pass, and the physical Map.dbc prefix through the selected record.
Each present WDT causes all 4096 ADT attempts in x-major order, regardless of MAIN.
Earlier maps are processed because they can seed the source's flat model cache.
Only selected-map placements are retained. Complete M2 and bounded WMO dependencies,
including ordered MODN extraction,
feed the existing raw writers; unsupported roots stop preparation without exposing
partial products. See [source scope](PROVENANCE.md).

The returned object owns immutable raw model outputs and ordered lookup receipts.
Archive buffers are dropped after parsing; bounded final MODN Paths remain for
source-timed doodad lookups. `verify_unchanged` reopens the chain and checks consumed
inputs, including missing members and blocking dummy winners. It detects changed
inputs; it is not an atomic filesystem snapshot, a live lease, or protection
against a file being changed and restored between reads. Diagnostic summaries
cannot construct a preparation object.

`summary()` and `Error::diagnostic()` contain fixed categories, counts, numeric
identifiers, source revision/policy labels and a digest, with no asset text or
personal paths. Model names and raw placement values are accessible only through
the explicit product/placement interface, not these diagnostic serializers.

## Supported boundary

- Numeric patches 1 through 99, reverse mount winner order and blocking members
  of at most one byte
- Bounded legacy MPQ headers and version-one headers with zero extended offsets
- Complete MD20 collision products and WMO roots with every declared group, including validated MLIQ
- Source-order name events, direct placement-name ordinals and raw client IDs
- Cooperative cancellation, checked bounds and changed-input rejection

[Doodad dependency closure](DOODADS.md) retains raw transforms and selected-set
membership without claiming final emission order or G3D placement arithmetic.
Malformed or unsupported MLIQ dimensions, incompatible emitted/lookup model basenames, nonempty global-WMO
placement, flat-name collisions between distinct canonical members, empty ADT/WDT
name-table entries and other
unsupported input profiles fail explicitly. These restrictions can stop an actual
install during its global WMO pass before the selected map is reached. Real asset
acceptance has not been established for this acquisition library.

Map assembly still requires exact global encounter IDs, all map placements,
transformed doodad instances and validated map-wide
BIHs. The later
GameObjectDisplayInfo extraction phase is outside this selected-prefix result.
The independent terrain helper's successful real Northshire conversion does not
establish those remaining collision prerequisites.

WMO liquid production uses the witnessed root member for the source
case-sensitive type override and identifies `wmo-liquid-header-zero-padding-v1`
in its summary. See [the raw liquid contract](../benilla-native-vmap/WMO-LIQUID.md).

[Per-candidate doodad projection](DOODAD-PROJECTION.md) is available only through
explicit selection of its pinned portable numerical compatibility profile. Its
record spans deliberately omit the generated-ID word and do not establish map
assembly or portable equality with every native C++ libm.
