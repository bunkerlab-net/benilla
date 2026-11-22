# Read-only terrain, collision and navigation diagnostics

The player executable can inspect the real source-to-collision path without
starting the window, reading a profile or configuration, or joining a network:

```powershell
.\benilla.exe diagnose-collision --data '.\Data' --map 0 --tile 48,32 --timeout-seconds 120
```

All four options are required. Map IDs are 0–999, tile coordinates are 0–63,
and the requested cooperative deadline is 1–600 seconds. The command writes one flushed
JSON object per line to standard output and creates no files. A zero exit status
means the report was emitted; check each stage's status for the result.

The collector's optional `-Player` argument runs this check independently of
its terrain helper. It preserves the terrain report even if collision preparation
fails or exceeds its deadline. Supply only a player executable you trust. The
collector gives the cooperative deadline five seconds to flush a final report,
then enforces a separate hard process timeout and bounded stream drain. It also
limits output; the in-process
cooperative deadline cannot interrupt every allocation or synchronous decoder. Terrain preparation and freshness rereads are synchronous and cannot be interrupted by that soft deadline; the parent hard timeout also bounds those phases.

## Stages and interpretation

1. Selection validates the command arguments
2. Terrain capture independently prepares the selected genuine terrain tile
3. Capture reads the mounted archive chain, global WMO references, Map.dbc order,
   required prefix maps, and the complete selected map through the production reader
4. Placement and assembly create the selected map's tree, tiles, and model products
5. Validation checks the whole placement index and loads selected collision residency
6. Probe performs nine collision-height observations
7. Composition compares actual terrain/collision shared receipts and creates a
   historical static-height view using the original selection/retry arithmetic
8. Static probe performs nine composed terrain/collision height observations
9. Composition freshness rechecks its source receipts
10. Four navigation-neighbor stages independently attempt cardinal terrain extraction;
    a WDT-absent neighbor is reported explicitly, while malformed present input fails
11. Navigation terrain and input bind successful extraction products and report counts
12. Navigation generation invokes the packaged helper and records bounded output,
    polygon count and tracked native allocation peak after exact map-parameter
    binding and count/liquid metadata checks
13. Navigation validation uses the ordinary production Detour reader
14. Navigation probe performs up to nine nearest-ground observations from successful
    static-height results, using the original five-unit query extent
15. Navigation freshness and collision freshness recheck their exact consumed sources
16. Complete summarizes independent and composed results

This is format `benilla-collision-diagnostics-v3`. Use the matching collector;
older strict collectors reject the new version rather than misinterpret stages.
The command spelling is unchanged. The current collector accepts only version 3.

A failed prerequisite makes later dependent stages unavailable. Capture failures
include a fixed stage identifier and bounded work counters where available. The
report does not skip a required global or prefix source dependency to make the
selected map appear valid. A source recheck is performed whenever capture succeeded,
including after an assembly or consumer error.

The nine probe positions are at offsets 0.25, 0.5, and 0.75 across each axis of the
requested terrain grid tile. Public CMaNGOS world coordinates use
`x = (32 - tile_x - offset_x) * (1600 / 3)` and the same formula for y,
with binary32 arithmetic. Each probe starts at z=2000 and searches down 10,000
world units. Coordinates and results are reported as unsigned IEEE binary32 bit
patterns. A collision-height miss is the source sentinel -200000, not an error.
The observations are not assertions that buildings or NPCs ought to exist there.

A passed report supplies selected terrain, collision and generated navigation observations. It does not establish complete-map navigation, NPC movement policy, body support or gameplay readiness. The view describes immutable captured bytes. The final
recheck detects observed source changes; it does not lock the installation or
promise that files will remain unchanged afterward. Terrain and collision rechecks are sequential observations, not an atomic whole-install snapshot. Exact shared winning receipts establish compatible consumed inputs; each consumer retains its own source identity.

## Privacy and resource bounds

The JSON contains fixed stage/status/error identifiers, numeric counts, numeric
map/tile selections, probe coordinates, and SHA-256 identities. It excludes Data
paths, archive/model names, asset contents, profile details, and arbitrary error
messages. Review the report before sharing it. Hashes identify the selected source
version and are not proof of an authentic client build.

Output is capped at 64 records, 16 KiB per record and 256 KiB total. Existing
source capture, map assembly, model allocation and collision decoder budgets still
apply. The tile-resident consumer validates the complete bounded placement index but decodes only selected geometry, retaining its 16 MiB resident serialized-input cap. Required unloaded geometry returns a missing-data error, never a clear ray. Full-map producer memory and whole-index limits still apply; residency does not remove those costs. The preceding
assembly record includes tree, tile, model, and total serialized byte counts plus
tile and model counts, so a refusal still provides sizing evidence. That is an
explicit diagnostic limitation, not evidence that the installation is corrupt.

The parent collector drains both process streams and retains only schema-validated
records. Raw stderr and rejected output are not copied into the report. It creates
one new report outside Data and refuses to overwrite an existing report.

## Navigation helper and limitations

The command resolves benilla-native-navigation beside its own executable. There is
no arbitrary helper path option. Keep the matching helper from the same player
package beside the player. Missing or incompatible helpers produce fixed safe codes
while earlier terrain/collision results remain in the report. The helper receives
only bounded mesh/configuration bytes and does not read the installation itself.
Its response protocol and exact requested map parameters are checked before any
metadata is reported; successful response framing does not mean production
validation passed. Those stages are separate.

The generated map origin is relative to the explicit successful terrain extraction
inventory plus the complete collision-tile union. Neighbor absence is not a claim
that a WDT-present tile could be silently omitted. Changing that inventory requires
regeneration and changes its binding. All selected source receipts are checked
again; the checks are sequential, not an atomic lock on the installation.

The helper has a tracked native allocation budget of 128 MiB, with separately bounded
input/output/adapter allocations. Its peak counter is not total RSS. Generation
executes in a bounded subprocess; the collector also enforces the enclosing hard
deadline. Terrain extraction and freshness rereads have synchronous sections.
Exact original empty and 625-polygon skip outcomes are reported as unavailable
validation, rather than fabricated traversable space. Unsupported quantized terrain,
liquid navigation classes, off-mesh connections or geometry outside the validated
reader envelope remain explicit failures. Real-install sizing and cross-platform
results require local/CI verification. No generated cache or gameplay state is saved.

Nearest-query hits include the exact output coordinate bits and split 32-bit polygon
reference words. A valid no-ground result is a passed observation with result miss.
When static height is missing, the corresponding navigation point is not invented;
the report remains incomplete unless all nine observations are available.

The v3 diagnostic executes only the matching trusted navigation helper beside the
player executable. Before execution, it checks an absolute path, regular leaf,
and each ancestor with no-follow metadata, rejecting symlinks and Windows reparse
points. Windows UNC/device prefixes and remote/unknown drives are refused. These
are observational checks, not a sandbox or race-free executable attestation; use
a trusted package in a directory that untrusted processes cannot replace. An
unsupported native ABI reports `navigation.unsupported` after retaining input
sizing instead of attempting the unsupported helper. Windows junction/drive
behavior still requires the native Windows test gate.
