"""Original authored group/link/waypoint cases, imported by the CI exporter suite."""
import copy
import hashlib
import json
from pathlib import Path
import tempfile
import types
import unittest
from classic_export import catalog_groups as g
from classic_export.catalog_io import encode_row
from classic_export.core import ExportError, canonical


class GroupCatalogTests(unittest.TestCase):
    def setUp(self):
        self.rows = {}
        self.columns = {}
        for spec in g.specs().values():
            table = spec['relations'][0]
            if 'COUNT(' not in spec['query']:
                self.columns[table] = spec['query'][7:].split(' FROM ')[0].lower().split(', ')
                self.rows[table] = []
        for kind in ('creature', 'gameobject'):
            self.columns[kind] = ['guid', 'id', 'map']
            self.columns[kind+'_template'] = ['entry']
            self.rows[kind] = [[b'7', b'11', b'0'], [b'8', b'11', b'1']]
            self.rows[kind+'_template'] = [[b'11']]
        self.schema = {'tables': {t: {'columns': [{'COLUMN_NAME':c, 'DATA_TYPE':'int'} for c in cols]}
                                  for t, cols in self.columns.items()}}
        def scan(table):
            for ordinal, cells in enumerate(self.rows[table]):
                yield types.SimpleNamespace(ordinal=ordinal, cells=lambda c=cells: c)
        self.catalog = types.SimpleNamespace(
            scan=scan, manifest={'schema_root':None,'catalog_sha256':'a'*64},
            object=lambda _: canonical(self.schema), _json=json.loads,
            relations={t:{'rows':len(rows)} for t,rows in self.rows.items()})
        self.views = g.GroupViews(self.catalog)
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)/'streams'

    def put(self, table, **values):
        self.rows[table].append([str(values.get(c, 0)).encode() for c in self.columns[table]])

    def capture(self):
        # The source returns deliberately reversed SQL cursors, not catalog order.
        def query(sql):
            spec = next(s for s in g.specs().values() if s['query']==sql)
            return iter(reversed(list(self.views.expected(spec))))
        g.capture_occurrences(types.SimpleNamespace(query_rows=query), self.root)

    def test_typed_membership_formation_links_squads_and_alternatives(self):
        self.put('spawn_group', id=1, type=0)
        self.put('spawn_group', id=2, type=1)
        self.put('spawn_group_formation', id=1)
        for group in (1,2,1):
            self.put('spawn_group_spawn', id=group, guid=7)
        self.put('spawn_group_spawn', id=1, guid=8)
        self.put('spawn_group_entry', id=1, entry=11)
        self.put('spawn_group_squad', id=1, squadid=2, guid=7, entry=11)
        self.put('spawn_group_linked_group', id=1, linkedid=2)
        self.put('spawn_group_linked_group', id=2, linkedid=99)
        plans = self.views.plans()
        self.assertEqual(plans['duplicate_membership'],1)
        self.assertEqual(plans['groups_without_formation'],1)
        self.assertEqual(plans['cross_map_groups'],1)
        self.assertEqual(plans['squad_overrides_entries'],1)
        self.assertEqual(plans['linked_missing_group'],1)
        self.assertEqual(plans['missing_entry'],0)
        self.capture()
        result = self.views.report(self.views.verify_occurrences(self.root))
        self.assertFalse(result['complete'])
        self.assertEqual(result['loaders']['2']['rows'],4)
        self.assertEqual(len(result['families']),12)

    def test_waypoint_gaps_zero_and_grouped_count_not_max_point(self):
        for point in (0,2,5):
            self.put('creature_movement', id=7, point=point)
        self.put('creature_movement_template', entry=11, pathid=1, point=1)
        self.put('creature_movement_template', entry=11, pathid=2, point=1)
        self.put('waypoint_path', pathid=50, point=3)
        self.assertEqual(list(self.views.expected(g.specs()['8'])),[[b'7',b'3']])
        self.assertEqual(list(self.views.expected(g.specs()['10'])),[[b'11',b'2']])
        plans = self.views.plans()
        self.assertEqual(plans['waypoint_gap_edges'],2)
        self.assertEqual(plans['waypoint_zero_paths'],1)
        self.capture()
        self.assertEqual(self.views.verify_occurrences(self.root)['8']['rows'],1)

    def test_cross_map_missing_and_ambiguous_masters(self):
        self.put('creature_linking', guid=7, master_guid=8, flag=1)
        self.put('creature_linking', guid=99, master_guid=100, flag=0)
        self.put('creature_linking', guid=7, master_guid=7, flag=256)
        self.put('creature_linking_template', entry=11, master_entry=11, map=2, flag=512)
        plans = self.views.plans()
        for key in ('link_cross_map','link_missing_slave','link_missing_master','link_invalid_flag','link_self_despawn','link_nonunique_master'):
            self.assertEqual(plans[key],1,key)

    def test_occurrence_permutation_preserves_multiset_not_order(self):
        self.put('spawn_group_linked_group',id=1,linkedid=2)
        self.put('spawn_group_linked_group',id=2,linkedid=1)
        self.capture()
        first = self.views.verify_occurrences(self.root)['4']
        raw = b''.join(encode_row(i,c) for i,c in enumerate(self.rows['spawn_group_linked_group']))
        self.rewrite('4',raw,2)
        second = self.views.verify_occurrences(self.root)['4']
        self.assertEqual(first['multiset_sha256'],second['multiset_sha256'])
        self.assertNotEqual(first['occurrence_sha256'],second['occurrence_sha256'])

    def rewrite(self,key,raw,rows):
        (self.root/(key+'.rows')).write_bytes(raw)
        meta = json.loads((self.root/'occurrences.json').read_bytes())
        meta['streams'][key].update(rows=rows,bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest())
        (self.root/'occurrences.json').write_bytes(canonical(meta))

    def test_refuse_rehashed_forgery_exhaustion_trailing_and_count(self):
        self.put('spawn_group_linked_group',id=1,linkedid=2)
        self.capture()
        path = self.root/'4.rows'
        original = path.read_bytes()
        for raw,rows,error in ((b'',0,'groups.exhausted'),
            (original+b'x',1,'groups.trailing'),
            (encode_row(0,[b'1',b'99']),1,'groups.view_mismatch'),
            (encode_row(2,[b'1',b'2']),1,'catalog.duplicate_row')):
            self.rewrite('4',raw,rows)
            with self.assertRaises(ExportError) as caught:
                self.views.verify_occurrences(self.root)
            self.assertEqual(error,str(caught.exception))
        self.rewrite('4',original,2)
        with self.assertRaisesRegex(ExportError,'groups.hash'):
            self.views.verify_occurrences(self.root)

    def test_empty_catalog_and_unknown_stream_refuse(self):
        self.capture()
        self.assertEqual(self.views.verify_occurrences(self.root)['0']['rows'],0)
        meta = json.loads((self.root/'occurrences.json').read_bytes())
        meta['streams']['unknown'] = copy.deepcopy(meta['streams']['0'])
        (self.root/'occurrences.json').write_bytes(canonical(meta))
        with self.assertRaisesRegex(ExportError,'groups.occurrences'):
            self.views.verify_occurrences(self.root)

    def test_more_than_4096_waypoints_and_null_count(self):
        for point in range(1,4102):
            self.put('waypoint_path',pathid=7,point=point)
        self.rows['waypoint_path'][0][1] = None
        self.assertEqual(list(self.views.expected(g.specs()['12'])),[[b'7',b'4100']])
        self.capture()
        self.assertEqual(self.views.verify_occurrences(self.root)['14']['rows'],4101)

    def test_forged_grouped_count_query_identity_refuses(self):
        self.put('creature_movement',id=7,point=9)
        self.capture()
        self.rewrite('8',encode_row(0,[b'7',b'9']),1)
        with self.assertRaisesRegex(ExportError,'groups.view_mismatch'):
            self.views.verify_occurrences(self.root)
        self.rewrite('8',encode_row(0,[b'7',b'1']),1)
        meta = json.loads((self.root/'occurrences.json').read_bytes())
        meta['streams']['8']['query'] = 'SELECT Id, MAX(Point) FROM creature_movement GROUP BY Id'
        (self.root/'occurrences.json').write_bytes(canonical(meta))
        with self.assertRaisesRegex(ExportError,'groups.query'):
            self.views.verify_occurrences(self.root)

    def test_plan_residency_limit_refuses_without_clipping(self):
        self.catalog.relations['creature']['rows'] = 2_000_000
        with self.assertRaisesRegex(ExportError,'groups.view_limit'):
            self.views.plans()
