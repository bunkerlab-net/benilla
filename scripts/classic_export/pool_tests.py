"""Authored pool-view tests, included by the CI exporter suite."""
import hashlib
import json
from pathlib import Path
import struct
import tempfile
import types
import unittest

from . import catalog_pools as pool
from .core import ExportError, canonical


class CatalogPoolTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name) / "occurrences"
        self.rows = {n: [] for n in pool.TABLES}
        self.rows.update(pool_template=[(b"1", b"1", b"one"), (b"2", b"2", b"two"), (b"3", b"1", b"three")],
                         pool_creature=[(b"7", b"1", b"0")],
                         pool_creature_template=[(b"10", b"1", b"25"), (b"99", b"1", b"-1")],
                         pool_pool=[(b"2", b"3", b"0")],
                         creature=[(b"7", b"10", b"0"), (b"8", b"10", b"1")], gameobject=[])
        self.cols = dict(zip(pool.TABLES, [list(pool.COLS[1]),list(pool.COLS[2]),
            ["id", "pool_entry", "chance"],list(pool.COLS[4]),["id", "pool_entry", "chance"],list(pool.COLS[6])]))
        self.cols.update(creature=["guid", "id", "map"], gameobject=["guid", "id", "map"])
        def scan(name):
            for ordinal, cells in enumerate(self.rows[name]):
                yield types.SimpleNamespace(ordinal=ordinal, cells=lambda c=cells: list(c))
        self.spawns = types.SimpleNamespace(index_bytes=0, scan=scan, columns=lambda n:self.cols[n],
            lookup=lambda n, key: (i for i,r in enumerate(self.rows[n]) if r[0] == key))
        dbc = b"WDBC" + struct.pack("<IIII", 2, 1, 4, 1) + struct.pack("<II", 0, 1) + b"\0"
        self.catalog = types.SimpleNamespace(relations={n:{"rows":len(self.rows[n])} for n in pool.TABLES},
            manifest={"catalog_sha256":"a"*64,"dbc_root":"dbcs"}, _json=json.loads,
            object=lambda d: canonical({"Map.dbc":"map"}) if d == "dbcs" else dbc,
            row=lambda n,i:self.rows[n][i])
        self.views = pool.PoolViews(self.catalog,self.spawns)
        self.sql = {n:list(self.views.expected(n)) for n in pool.STREAMS}
        self.source = types.SimpleNamespace(query_rows=lambda q:iter(self.sql[next(n for n in pool.STREAMS if pool.query(n)==q)]))
        pool.capture_occurrences(self.source,self.root)

    def test_complete_expansion_nulls_duplicate_guid_and_cross_map(self):
        records = Path(self.tmp.name) / "records"
        report = self.views.verify_occurrences(self.root, records)
        self.assertEqual(report["streams"]["pool_creature_template"]["rows"],3)
        self.assertEqual(report["dispositions"]["duplicate_guid"],1)
        self.assertEqual(report["dispositions"]["cross_map"],1)
        self.assertEqual(report["dispositions"]["missing_spawn"],1)
        self.assertEqual(self.views.hierarchy["parents"],[[2,3]])
        self.assertFalse(report["complete"])
        self.assertIn("pool.objectmgr_loaded_spawn_validation_pending",report["refusals"])
        self.assertEqual(hashlib.sha256((records / "dispositions.jsonl").read_bytes()).hexdigest(),
                         report["disposition_sha256"])
        self.assertEqual(hashlib.sha256((records / "hierarchy.json").read_bytes()).hexdigest(),
                         report["hierarchy_sha256"])
        with self.assertRaises(FileExistsError):
            self.views.verify_occurrences(self.root, records)

    def test_missing_max_row_is_not_sql_null(self):
        (self.root / "maximum.rows").write_bytes(b"")
        with self.assertRaisesRegex(ExportError,"pool.exhausted"):
            self.views.verify_occurrences(self.root)

    def test_empty_table_maximum_is_one_null_occurrence(self):
        self.rows["pool_template"] = []
        self.assertEqual(list(self.views.expected("maximum")),[[None]])

    def test_join_duplicate_loss_rehashed_forgery_refuses(self):
        self.sql["pool_creature_template"][0] = self.sql["pool_creature_template"][1]
        other=Path(self.tmp.name)/"forged"
        pool.capture_occurrences(self.source,other)
        with self.assertRaisesRegex(ExportError,"pool.join_mismatch"):
            self.views.verify_occurrences(other)

    def test_query_stream_order_and_trailing_refusals(self):
        self.sql["pool_creature_template"].reverse()
        other=Path(self.tmp.name)/"reordered"
        pool.capture_occurrences(self.source,other)
        a=self.views.verify_occurrences(self.root)
        b=self.views.verify_occurrences(other)
        self.assertEqual(a["streams"]["pool_creature_template"]["multiset_sha256"],b["streams"]["pool_creature_template"]["multiset_sha256"])
        self.assertNotEqual(a["streams"]["pool_creature_template"]["occurrence_sha256"],b["streams"]["pool_creature_template"]["occurrence_sha256"])
        path=self.root/"pool_pool.rows"
        path.write_bytes(path.read_bytes()+b"x")
        with self.assertRaisesRegex(ExportError,"pool.trailing"):
            self.views.verify_occurrences(self.root)

    def test_nested_cycle_negative_zero_overfull_and_missing_map(self):
        state=pool.plans.PoolState(4,{0,1},lambda kind,guid:0 if guid else None)
        state.apply("pool_template",0,[b"1",b"1",b"one"])
        for i in range(2):
            state.apply("pool_creature",i,[str(i+1).encode(),b"1",b"70"])
        state.apply("pool_creature",2,[b"3",b"1",b"-1"])
        self.assertEqual(state.records[-1][2],"invalid_chance")
        state.apply("pool_creature",3,[b"4",b"1",b"101"])
        self.assertEqual(state.records[-1][2],"invalid_chance")
        for child,parent in [(2,3),(3,2)]:
            state.apply("pool_pool",child,[str(child).encode(),str(parent).encode(),b"0"])
        state.apply("pool_pool",4,[b"4",b"4",b"0"])
        self.assertEqual(state.records[-1][2],"self_cycle")
        h=state.finish()
        self.assertEqual(h["parents"],[[2,3]])
        self.assertEqual(h["autospawn_disabled"],[1])
        self.assertTrue(any(r[2]=="cycle_removed" for r in state.records))
        self.assertEqual(state.map_check(4,99),"missing_map")

    def test_duplicate_join_occurrences_and_left_join_null_retained(self):
        self.rows["pool_creature_template"].append((b"10",b"1",b"25"))
        expanded=list(self.views.expected("pool_creature_template"))
        self.assertEqual(expanded.count([b"7",b"1",b"25",b"10"]),2)
        self.assertIn([None,b"1",b"-1",b"99"],expanded)

    def test_capture_refuses_row_quota_without_metadata(self):
        from unittest.mock import patch
        root=Path(self.tmp.name)/"quota"
        with patch.object(pool,"MAX_ROWS",0), self.assertRaisesRegex(ExportError,"pool.row_limit"):
            pool.capture_occurrences(self.source,root)
        self.assertFalse((root/"occurrences.json").exists())

    def test_equal_chances_holes_and_hierarchy_removal_keep_source_flags(self):
        state = pool.plans.PoolState(4, {0, 1}, lambda kind, guid: 0)
        for entry, limit in [(1, 1), (2, 2), (3, 1)]:
            state.apply("pool_template", entry, [str(entry).encode(), str(limit).encode(), b"pool"])
        state.apply("pool_creature", 0, [b"7", b"1", b"0"])
        state.apply("pool_creature", 1, [b"8", b"2", b"70"])
        state.apply("pool_creature", 2, [b"9", b"2", b"70"])
        state.apply("pool_creature", 3, [b"10", b"4", b"-0"])
        self.assertTrue(all(not r[2] for g in state.groups.values() for r in g))
        state.pool_maps[3] = 1
        state.apply("pool_pool", 0, [b"1", b"3", b"50"])
        state.apply("pool_pool", 1, [b"1", b"3", b"0"])
        hierarchy = state.finish()
        self.assertEqual(hierarchy["parents"], [])
        self.assertEqual(state.groups[("pool", 3)], [])
        self.assertIn(1, state.nested)
        self.assertEqual(hierarchy["autospawn_disabled"], [])
        self.assertTrue(any(r[0] == "hierarchy" and r[2] == "cross_map" for r in state.records))

    def test_shared_decoded_budget_refuses_instead_of_clipping(self):
        from unittest.mock import patch
        state = pool.plans.PoolState(4, {0}, lambda kind, guid: 0)
        state.budget = 1
        with self.assertRaisesRegex(ExportError, "pool.decoded_limit"):
            state.apply("pool_template", 0, [b"1", b"1", b"pool"])
        with patch.object(pool, "DECODED", 1), self.assertRaisesRegex(ExportError, "pool.decoded_limit"):
            self.views.verify_occurrences(self.root)

    def test_expansion_refuses_oversized_numeric_cells_before_retention(self):
        self.rows["pool_creature_template"] = [(b"10", b"1", b"0" * 128)]
        with self.assertRaisesRegex(ExportError, "spawn.numeric_limit"):
            list(self.views.expected("pool_creature_template"))
