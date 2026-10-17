# Exact-source swimming-capable allowed Z

This scalar oracle executes the complete pinned CMaNGOS Classic
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f` bodies of
`Map::GetHeight`, `Unit::UpdateAllowedPositionZ`, and
`TerrainInfo::GetWaterOrGroundLevel`. Static height, liquid classification and
empty-dynamic results are explicitly scripted observation boundaries. It does
not query loaded geometry, establish wet locomotion, or admit native content.
The existing ground-height oracle separately executes the actual empty dynamic
tree, while application tests cover actual decoded authored dry products.

The generator retains all ground-height/VMap/G3D hash guards, adds complete-file
hash guards for GridMap.cpp, GridMap.h, GridMapDefines.h and IVMapManager.h, and
extracts complete methods, source constants, liquid types and the exact
getLiquidStatus declaration including its default `2.03128f` argument. Original
source notices appear in both the generated compilation unit and checked-in
source package. Unrecognized/missing source, extraction/marker drift and changed
fixtures fail rather than skip.

The 865 rows cover no water, every individual nonzero status and combined status,
selected collision heights distinct from default classification height, strict
depth equality and adjacent binary32 values, water-walk controls, disabled liquid
checking, invalid ground threshold neighbors, raw -200000 ground, signed zeros,
clamps and exact ordered static/dynamic/liquid call arguments. Liquid callbacks
supply an unrelated depth_level to detect substitution for groundZ. Nonzero,
water-walk and disabled-check rows are scalar controls only. The loaded reader
continues to require complete dry bytes, enabled VMap height, exact inventory and
configuration, covered coordinates and independent empty-dynamic proof.

Each tab-separated row contains:

```
id z ground level status selected_height water_walk can_check search
 dynamic_z dynamic_range liquid_count liquid_z mask classification_height max_z allowed_z
```

Float columns are eight-digit binary32 hexadecimal. Identifiers, status, booleans,
counts and mask are decimal. Missing liquid calls have zeroed observation columns.
The harness fixes CanSwim=true and CanFly=false, executes Unit against its own
Map, and checks the 50-yard static search even when final dry Z equals ground-only
arithmetic. The exact TerrainInfo method is invoked independently a second time
solely to record intermediate maxZ.

Run offline:

```sh
python3 crates/benilla-native-geometry-tests/tools/swimming_allowed_z_generate.py /path/to/mangos-classic
python3 crates/benilla-native-geometry-tests/tools/swimming_allowed_z_generate.py /path/to/mangos-classic --check --sanitize
python3 crates/benilla-native-geometry-tests/tools/test_swimming_allowed_z_generate.py /path/to/mangos-classic
```

The compile profile is C++17, `-O0 -fno-fast-math -ffp-contract=off`, binary32,
little-endian and FE_TONEAREST. Sanitizing adds undefined-behavior and
float-cast-overflow traps. Normal and sanitized fixtures must be byte-identical.
No Rust build, package installation, network, raw-content compiler or game assets
are involved. Source attribution is covered by the adjacent existing CMaNGOS
notice and the original notices in the verbatim package.
