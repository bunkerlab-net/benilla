# Selected private terrain production

This original local adapter ports the **dry selected-tile** branch of CMaNGOS
Classic `ad -f 0` at `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`.
It reads the player's own Data directory, owns exact raw input buffers, writes
`MAPS` / `z1.4` terrain, and has a separate app adapter into the existing
`TerrainTile` reader. It does not generate VMaps/MMaps, import world-content rows,
construct gameplay capabilities, install providers, or activate NPC startup.

## Local command

The output parent must exist. Choose a fresh directory outside the entire client
installation. Terrain grid coordinates are explicit: `32,31` reads ADT
`<directory>_31_32.adt` and writes `maps/0003231.map` for map zero.
The numbers below are a command example, not a claim of supported real terrain.

```sh
cargo run -p benilla-native-terrain -- \
  --data "/path/to/WoW/Data" --map 0 --tile 32,31 \
  --output benilla-config/private-terrain
```

Repeat `--tile` for an explicit closure of at most nine tiles. No tile or neighbor
is silently added, skipped or truncated. A terrain collection is not a corridor
coverage certificate. Source members, generated files and manifests stay private;
never commit or upload them. The command has no network or asset download path.

The CLI creates generation artifacts. The separate app `SourceTerrainProducts`
consumer validates them through the existing terrain reader before retaining the
selected decoded tiles; neither path constitutes movement/support authority.
The full rendered collision and generated VMap/MMap closures remain separate.

## Inputs and honest source identity

The fixed DBC inputs are complete `Map.dbc` (42 fields), `AreaTable.dbc` (25),
and `LiquidType.dbc` (4), followed by the selected map's WDT and ADT members.
Map selection and WDT MAIN selection derive filenames; no arbitrary MPQ path
or imported audit declaration determines output identities.

Benilla's normal patch chain and pinned `ad` are **not identical resolvers**.
This adapter accepts only the conservative intersection: every consumed member
must have the same exact winning archive basename and complete bytes under
Benilla's bounded chain and the pinned `ad` fixed order
`model.MPQ`, `dbc.MPQ`, `terrain.MPQ`, `patch.MPQ`, `patch-2.MPQ`.
The accepted archives also require the canonical version-zero header at offset zero,
nonzero power-of-two hash table, and source-compatible selected-member framing.
Absent fixed archives are allowed as upstream does; ambiguous casing, tombstones,
missing/dummy members and different winners fail. An extra winning patch is not
silently ignored even if its bytes match. The policy is versioned in the manifest
and source-epoch digest. No external extractor executable is invoked.

All complete member bytes, winning identities and policy are retained immutably.
Discovery captures must match the final capture. Fresh-chain rereads before and
after generation and before manifest publication reject observed changes. This
is an immutable **consumed-buffer epoch**, not an atomic filesystem snapshot,
lock, hostile-ABA defense, proof of client build authenticity, or live installation
lease. The manifest explicitly says `live_install_lease: false` and
`gameplay_admitted: false`. A later native session still needs a genuine source
lifecycle contract; hashes alone cannot hold an installation fixed.

## Writer fidelity and restrictions

The source references are `contrib/extractor/System.cpp`: Map loading 189–208,
AreaTable loading 211–233, LiquidType loading 235–254, `ConvertADT` 291–837,
and WDT selection/filename ordering 840–879. Raw framing follows `adt.h` and
`wdt.cpp` at the same pin. The independently extracted source oracle records
full source hashes and extraction intervals under `tools/fixtures`.

- MCIN slot row/column controls placement, not the MCNK's advisory ix/iy fields
- Row-major later cells overwrite shared V9 seams, resetting base before adding
- Binary32 height addition, V8-then-V9 extrema scans, source +/-20000 initial
  extrema, -500 minimum clamp, and exact-flat omission are preserved
- `-f 0` never applies the near-flat threshold or integer height packing
- AreaTable physical row order, duplicate last-wins, uint16 narrowing, unknown
  area `0xffff`, uniform/full AREA layout, V9-before-V8 output, and the complete
  512-byte hole block are preserved

The bounded parser requires canonical MCIN placement, all 256 distinct referenced
MCNKs, source-relative MCVT when present, finite supported heights and strict framing.
A genuine zero MCVT offset preserves the source base-only height branch; dangling
references reject. It rejects all liquid-bearing
or later-client terrain representations, global-WMO maps, incomplete tiles,
unknown structural chunks and resource overflow. Normal render-model placement
records may coexist: their presence does not prove their collision dependency
closure. Dry terrain generation does not prove absence of dynamic participants.
The exact restrictions and diagnostics are executable tests, not fallback flags.

## Publication and limits

At most nine selected tiles; 64 MiB per raw member, 192 MiB aggregate captured
members, 64 bounded archives and bounded MPQ indexing/decompression. Output paths
are derived only from checked numeric map/tile IDs. Files are private on Unix,
created without overwrite and synced. Products are reread and compared before a
complete, synced manifest is atomically exposed with a no-replace hard link.
Failure/crash can leave an incomplete fresh directory; retry with a new output,
never infer completion from file presence alone. Directory sync is Unix-only;
Windows file sync does not imply directory durability. A failure after the complete
marker is linked reports `output.published_but_durability_unconfirmed`; the complete
marker may already be visible and no rollback or successful durability is claimed.
No previous export is
replaced, and no output is automatically activated.

## Evidence and remaining acceptance

Tests use only original authored MPQ/DBC/WDT/ADT data. The source oracle compiles
isolated pinned source routines/readers with authored local-file transport; it
neither builds nor runs the external `ad` executable. Byte-level comparisons,
source-change and publication failures, and actual `TerrainTile` decoding are
separate gates. No player installation was available for this checkpoint.
A player-local run against a real selected tile is still required; it may report
unsupported source geometry. Passing authored tests is not Northshire acceptance.

This is a marked Rust adaptation by Benilla fork contributors in 2026, licensed
GPL-3.0-or-later. Preserve [the upstream notice](CMANGOS-NOTICE.txt), the unchanged
upstream authorship/license files under [benilla-sim/licenses](../benilla-sim/licenses/),
and the repository `LICENSE-GPL-3.0` when distributing source or binaries.
