#!/usr/bin/env python3
"""Independent captured-location diagnostic. Oracle results are not capabilities."""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import sys

# The helper's tests run this tool from the sealed distribution source tree.
sys.dont_write_bytecode = True

sys.path.insert(0, str(Path(__file__).resolve().parents[3] / 'scripts'))
from native_starter.location_inputs import CollisionIndex, Files, verify_grid_capture


def verify_wmo_exclusion(root, position, oracle=None):
    """Retain the original-source oracle as an explicit diagnostic-only path."""
    root = Path(root)
    index = CollisionIndex(Files(root.parent), root.name)
    result = index.summary(position)
    if result['wmo_bounds_containing_position']:
        if oracle is None:
            raise ValueError('location.wmo_candidate_requires_geometry')
        run = subprocess.run([str(oracle), str(root), *map(str, position)],
                             check=True, text=True, capture_output=True)
        observed = json.loads(run.stdout)
        assert observed['position_bits'] == [struct.unpack('<I', struct.pack('<f', n))[0] for n in position]
        assert observed['slots'] == index.count
        assert observed['wmo_candidates'] == result['wmo_bounds_containing_position']
        if observed['raw_hit'] is not False:
            raise ValueError('location.wmo_override_requires_resolution')
        result['original_source'] = observed
        result['oracle_sha256'] = hashlib.sha256(oracle.read_bytes()).hexdigest()
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('capture', type=Path)
    parser.add_argument('--position', nargs=3, type=float, required=True)
    parser.add_argument('--collision-index', type=Path)
    parser.add_argument('--wmo-oracle', type=Path)
    args = parser.parse_args()
    result = verify_grid_capture(args.capture, args.position)
    if args.collision_index:
        result['wmo_exclusion'] = verify_wmo_exclusion(args.collision_index, args.position, args.wmo_oracle)
        result['wmo_precedence_required'] = False
        result['combined_source_epoch'] = hashlib.sha256(
            b'benilla-genuine-resident-source-location-v1' +
            result['source_epoch'].encode() +
            bytes.fromhex(result['wmo_exclusion']['collision_epoch'])
        ).hexdigest()
    print(json.dumps(result, indent=2))
