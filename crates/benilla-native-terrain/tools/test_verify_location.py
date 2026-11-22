#!/usr/bin/env python3
"""Original authored bounds and source-layout records, never install fixtures."""
import hashlib
import json
from pathlib import Path
import struct
import tempfile
import unittest
from verify_location import verify_wmo_exclusion
from classic_export.core import ExportError


class WmoExclusionTests(unittest.TestCase):
    def capture(self, root, flags=4, bounds=(1.,1.,1.,2.,2.,2.), include_slot=True):
        tree = b'VMAP_7.0\x01NODE' + struct.pack('<6f',0.,0.,0.,3.,3.,3.)
        tree += struct.pack('<6I',3,0xc0000000,1,0,1,0) + b'GOBJ'
        tile = b'VMAP_7.0' + struct.pack('<I',int(include_slot))
        if include_slot:
            tile += struct.pack('<IHI7f6fI',flags,0,7,0.,0.,0.,0.,0.,0.,1.,*bounds,5)
            tile += b'a.wmo' + struct.pack('<I',0)
        model = b'VMAP_7.0WMOD' + struct.pack('<2I',8,0)
        models = struct.pack('<2I',1,5) + b'a.wmo' + struct.pack('<I',5) + b'a.vmo'
        files = []
        for name, data in [('000.vmtree',tree),('000_00_00.vmtile',tile),('models.index',models),('a.vmo',model)]:
            (root/name).write_bytes(data)
            files.append({'file':name,'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()})
        (root/'index.json').write_text(json.dumps({'format':'benilla-source-location-wmo-index-v1','map':0,'collision_epoch':'11'*32,'files':files,'gameplay_admitted':False}))

    def test_all_wmo_bounds_excluded(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            self.capture(root)
            proof=verify_wmo_exclusion(root,[0.,0.,0.])
            self.assertEqual((proof['slots'],proof['wmo_spawns'],proof['wmo_bounds_containing_position']),(1,1,0))

    def test_missing_slot_refuses(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            self.capture(root,include_slot=False)
            with self.assertRaisesRegex(ExportError, 'source_location.complete_collision_slots'):
                verify_wmo_exclusion(root,[0.,0.,0.])

    def test_candidate_wmo_requires_actual_geometry(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            self.capture(root,bounds=(-100000.,-100000.,-100000.,100000.,100000.,100000.))
            with self.assertRaisesRegex(ValueError,'wmo_candidate_requires_geometry'):
                verify_wmo_exclusion(root,[0.,0.,0.])
            self.capture(root,flags=5,bounds=(-100000.,-100000.,-100000.,100000.,100000.,100000.))
            self.assertEqual(verify_wmo_exclusion(root,[0.,0.,0.])['wmo_spawns'],0)


if __name__ == '__main__':
    unittest.main()
