#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Compare the actual helper against independently captured pinned source outputs."""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import time
import zlib

ROOT = Path(__file__).resolve().parents[3]
FIXTURES = ROOT / "tools/native-navigation/fixtures"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--helper", required=True, type=Path)
    args = parser.parse_args()
    binary = args.helper.resolve(strict=True)
    measurements = {}
    for case in ("ridge_wmo", "ridge_neighbors_wmo", "ridge_xminus_wmo", "ridge_only", "liquid_full_types_wmo"):
        base = FIXTURES / case
        request = zlib.decompress((base / "helper-request.bin.zlib").read_bytes())
        started = time.monotonic()
        result = subprocess.run([str(binary)], input=request, stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE, timeout=120, check=False)
        elapsed = time.monotonic() - started
        assert result.returncode == 0, (case, result.returncode, result.stderr)
        assert not result.stderr, (case, result.stderr)
        output = result.stdout
        assert output[:8] == b"BNAVOUT1"
        status, mmap_size, tile_size, polygons, peak = struct.unpack_from("<4IQ", output, 8)
        expected = json.loads((base / "expected.json").read_text(encoding="utf-8"))
        assert mmap_size == 28 and len(output) == 32 + mmap_size + tile_size, case
        assert output[32:60] == (base / "000.mmap").read_bytes(), case
        if expected["output"]["emitted"]:
            assert status == 0, case
            assert output[60:] == (base / "0003130.mmtile").read_bytes(), case
        else:
            assert status == 2 and polygons == 625 and tile_size == 0, case
            assert not (base / "0003130.mmtile").exists(), case
        assert polygons == expected["output"]["merged_polygons"], case
        measurements[case] = {
            "status": status,
            "mmap_sha256": hashlib.sha256(output[32:60]).hexdigest(),
            "mmtile_sha256": hashlib.sha256(output[60:]).hexdigest(),
            "mmtile_bytes": tile_size,
            "polygons": polygons,
            "native_peak_bytes": peak,
            "wall_seconds": round(elapsed, 6),
            "solid_vertices": expected["solid_vertices"],
            "solid_triangles": expected["solid_triangles"],
            "liquid_vertices": expected["liquid_vertices"],
            "liquid_triangles": expected["liquid_triangles"],
        }
    print(json.dumps(measurements, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
