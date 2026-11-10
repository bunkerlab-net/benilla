#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Count raw catalog elevator candidates, not effective ObjectMgr or F1 occurrences."""
import argparse
import collections
import json
from pathlib import Path
import struct
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[3]/'scripts'))
from classic_export.catalog_io import Catalog
from classic_export.catalog_dbc import DbcViews


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('catalog')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    catalog = Catalog(args.catalog)
    views = DbcViews(catalog)
    templates = {int(row['entry']): row for _, row in views.rows('gameobject_template') if int(row['type']) == 11}
    dbc = views.open('TransportAnimation.dbc')
    animations = {}
    overwritten = 0
    for key in sorted(dbc.by_id):
        w = dbc.words(dbc.by_id[key])
        timeline = animations.setdefault(w[1], {})
        overwritten += w[2] in timeline
        timeline[w[2]] = w
    rows = []
    for ordinal, row in views.rows('gameobject'):
        entry = int(row['id'])
        if entry not in templates:
            continue
        timeline = animations.get(entry)
        template = templates[entry]
        bits = lambda value: struct.unpack('<I', struct.pack('<f', float(value)))[0]
        extra = next(key for key in template if key.replace('_', '') == 'extraflags')
        rows.append({'ordinal': ordinal, 'guid': int(row['guid']), 'entry': entry, 'map': int(row['map']),
                     'animation': timeline is not None, 'total_time': max(timeline) if timeline else None,
                     'display': int(templates[entry]['displayid']),
                     'position_bits': [bits(row[key]) for key in ['position_x', 'position_y', 'position_z', 'orientation']],
                     'rotation_bits': [bits(row['rotation'+str(i)]) for i in range(4)],
                     'template': {'entry': entry, 'kind': 11, 'display': int(template['displayid']),
                                  'faction': int(template['faction']), 'flags': int(template['flags']),
                                  'extra_flags': int(template[extra]), 'scale_bits': bits(template['size']),
                                  'data': [int(template['data'+str(i)]) for i in range(24)]},
                     'first_refusal': 'execution.dynamic_collision_owner_unavailable',
                     'conditional_update_refusal': 'execution.elevator.period_modulo_zero_undefined' if timeline and max(timeline) == 0 else None})
    result = {'catalog_sha256': catalog.manifest['catalog_sha256'], 'source_snapshot': catalog.manifest['source_snapshot'],
              'animation_dbc_sha256': dbc.descriptor['sha256'], 'animation_records': dbc.count,
              'animation_time_overwrites': overwritten, 'templates': len(templates),
              'templates_with_animation': sum(entry in animations for entry in templates),
              'zero_time_template_entries': [entry for entry in templates if entry in animations and max(animations[entry]) == 0],
              'spawns': len(rows), 'spawns_with_animation': sum(row['animation'] for row in rows),
              'by_map': {}, 'rows': rows, 'sealed_equivalence': False}
    for map_id in sorted({row['map'] for row in rows}):
        selected = [r for r in rows if r['map'] == map_id]
        result['by_map'][str(map_id)] = {'spawns': len(selected), 'entries': sorted({r['entry'] for r in selected}),
            'spawns_with_animation': sum(r['animation'] for r in selected),
            'templates_with_animation': len({r['entry'] for r in selected if r['animation']}),
            'refusals': dict(collections.Counter(r['first_refusal'] for r in selected))}
    args.output.write_text(json.dumps(result, indent=2)+'\n')
    print(json.dumps({k: v for k, v in result.items() if k != 'rows'}, sort_keys=True))
    catalog.close()


if __name__ == '__main__':
    main()
