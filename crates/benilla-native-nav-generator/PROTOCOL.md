# Selected-tile generation protocol v1

Little-endian, exact framing, binary32 float bits. One stdin request and one stdout
response; no command-line options, source paths, cache paths, profiles, environment
configuration or network inputs. The parent supplies already cleaned source-order
vertices/triangles, full-map tile-union parameters and effective source configuration.
A parent must reject selected off-mesh connections before this v1 helper: off-mesh
arrays are not representable, and silently omitting configured connections is invalid.

Request, in order:

- 8 bytes `BNAVIN01`
- u32 map_id, tile_x, tile_y, uses_liquids (0 or 1)
- u64 allocation_budget_bytes, 1 through 256 MiB, normally 128 MiB
- source 28-byte dtNavMeshParams: origin[3] f32, tile_width f32, tile_height f32,
  max_tiles u32, max_polys u32. max_polys is the raw source bits 0x80000000
- bmin[3] then bmax[3], f32
- Effective config (12 fields): border_size u32, walkable_slope_angle f32,
  walkable_height u32, walkable_climb u32, walkable_radius u32, max_edge_len u32,
  max_simplification_error f32, min_region_area u32, merge_region_area u32,
  detail_sample_dist f32, detail_sample_max_error f32, liquid_flag_merge_threshold f32
- Solid mesh: u32 vertex_count,triangle_count; f32 vertices[vertex_count][3];
  u32 triangles[triangle_count][3]
- Liquid mesh in the same format
- u8 liquid_areas[liquid_triangle_count] (source water 9 or magma/slime 8)

The min/merge region fields are source JSON values, before squaring. Counts are
triples, not scalar counts. Empty arrays have zero counts. Config merge semantics
belong to the caller: source defaults, map merge patch, then the unpadded decimal
concatenation of tile_x and tile_y as tile key. Integer source JSON conversions
truncate numbers. Grid size 533.33333f and base unit 0.2666666f are source binary32.

Response, in order:

- 8 bytes `BNAVOUT1`
- u32 status: 0 generated, 1 source empty geometry, 2 source exactly-625-polygon skip
- u32 mmap_len (28), mmtile_len (zero for skip), polygon_count
- u64 native_peak_bytes
- mmap bytes, then mmtile bytes including the source 20-byte MmapTileHeader

All errors produce nonzero exit, no response, and bounded stderr. The caller must
require successful exit, decode exact framing, independently validate both source
files, and bind outputs to the exact source/config/off-mesh identity. A helper result
does not establish game-session admission.

Limits: request and response each at most 32 MiB; each mesh at most 500,000 vertices
and 1,000,000 triangles; selected tile in 0..63; map in 0..999; map tiles 1..4096;
finite coordinates within ±100,000; vertical range at most 16,000 world units;
fixed 25×25 subtiles with 80 cells and source config-dependent bounded border.
For an empty mesh only, Y bounds may instead use the exact source sentinel
FLT_MIN/FLT_MAX; these bounds never enter rasterization. Per-field config validation
additionally constrains work to representable ranges.

`native_peak_bytes` measures the live peak of Recast/Detour allocations including
tracking headers, not RSS. Native mesh copies are bounded by the 32 MiB input cap;
Rust's stdin buffer is separately bounded by 32 MiB plus one byte (Vec capacity may
round upwards). Output buffering is bounded by 32 MiB. The wrapper uses a fixed
625-entry tile array and bounded triangle flags. The parent owns a hard wall-clock
process timeout and cancellation/kill/reap. This is a process boundary, not a claim
that an arbitrary native Recast call can be cooperatively interrupted.

Generator compatibility policy stable-BV v1 specifies stable equal-axis minima
ordering at each BV-tree split; it is independent of the unchanged v1 wire framing.
The parent product identity is navigation v2 / stable-BV v1. See README.md for the
explicit libc compatibility difference, scratch memory/work bounds, and failures.
