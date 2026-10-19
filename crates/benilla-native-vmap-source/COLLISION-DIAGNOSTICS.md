# Read-only collision diagnostics

The player executable can inspect the real source-to-collision path without
starting the window, reading a profile or configuration, or joining a network:

```powershell
.\benilla.exe diagnose-collision --data '.\Data' --map 0 --tile 48,32 --timeout-seconds 120
```

All four options are required. Map IDs are 0–999, tile coordinates are 0–63,
and the cooperative deadline is 1–600 seconds. The command writes one flushed
JSON object per line to standard output and creates no files. A zero exit status
means the report was emitted; check each stage's status for the result.

The collector's optional `-Player` argument runs this check independently of
its terrain helper. It preserves the terrain report even if collision preparation
fails or exceeds its deadline. Supply only a player executable you trust. The
collector gives the cooperative deadline five seconds to flush a final report,
then enforces a separate hard process timeout and bounded stream drain. It also
limits output; the in-process
cooperative deadline cannot interrupt every allocation or synchronous decoder.

## Stages and interpretation

1. Selection validates the command arguments
2. Capture reads the mounted archive chain, global WMO references, Map.dbc order,
   required prefix maps, and the complete selected map through the production reader
3. Placement creates the source-shaped placement records using the explicitly named
   portable arithmetic profile
4. Assembly creates the selected map's tree, tiles, and required model products in memory
5. Validation runs the complete collision-only consumer checks on that bundle
6. Probe performs nine height observations against the validated immutable collision snapshot
7. Freshness rechecks the captured source receipts
8. Complete summarizes the preceding results

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

A passed collision report does not establish terrain, navigation, NPC movement,
or gameplay readiness. The view describes immutable captured bytes. The final
recheck detects observed source changes; it does not lock the installation or
promise that files will remain unchanged afterward.

## Privacy and resource bounds

The JSON contains fixed stage/status/error identifiers, numeric counts, numeric
map/tile selections, probe coordinates, and SHA-256 identities. It excludes Data
paths, archive/model names, asset contents, profile details, and arbitrary error
messages. Review the report before sharing it. Hashes identify the selected source
version and are not proof of an authentic client build.

Output is capped at 32 records, 16 KiB per record and 256 KiB total. Existing
source capture, map assembly, model allocation and collision decoder budgets still
apply. In particular, collision consumer decoding has a 16 MiB aggregate serialized
product cap; a large map may exceed the currently supported envelope. The preceding
assembly record includes tree, tile, model, and total serialized byte counts plus
tile and model counts, so a refusal still provides sizing evidence. That is an
explicit diagnostic limitation, not evidence that the installation is corrupt.

The parent collector drains both process streams and retains only schema-validated
records. Raw stderr and rejected output are not copied into the report. It creates
one new report outside Data and refuses to overwrite an existing report.
