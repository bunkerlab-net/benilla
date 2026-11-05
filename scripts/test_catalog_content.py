"""Authored whole-catalog content cases, imported by the exporter CI suite."""
import types
import unittest
from classic_export import catalog_content as c
from classic_export.core import ExportError


class ContentCatalogTests(unittest.TestCase):
    def test_quest_reverse_edges_include_negative_chain_and_breadcrumb(self):
        row = dict(prevquestid=-2,nextquestid=3,nextquestinchain=4,breadcrumbforquestid=5,exclusivegroup=-8)
        self.assertEqual(list(c.quest_edges(1,row)),[
            ["quest_reverse",2,1,"prevquestid",-2],["quest_reverse",3,1,"nextquestid",3],
            ["quest_reverse",4,1,"nextquestinchain",4],["quest_reverse",5,1,"breadcrumbforquestid",5],
            ["quest_exclusive",-8,1]])

    def test_raw_null_empty_text_and_signed_widths_remain_distinct(self):
        self.assertIsNone(c.convert(None,"raw"))
        self.assertEqual(c.convert(b"","raw"),"")
        self.assertEqual(c.convert(b"x\0tail","raw"),"78007461696c")
        self.assertEqual(c.convert(b"x\0tail","text"),"78")
        self.assertEqual(c.convert(b"-1","i32"),-1)
        self.assertEqual(c.convert(b"65537","u16"),1)
        self.assertEqual(c.convert(b"-0","f32"),0x80000000)

    def test_arbitrary_negative_lookup_does_not_become_whole_empty(self):
        views = c.ContentViews.__new__(c.ContentViews)
        views.spec = c.POLICY["families"]["quests"]
        views.catalog = types.SimpleNamespace(relations={"quest_template":{"rows":2}},lookup=lambda t,k:iter([]))
        self.assertEqual(views.lookup_state("quest_template",b"unknown"),
            {"whole_empty":False,"selected_empty":True,"rows":0,"field_states":[]})
        views.catalog.relations["quest_template"]["rows"] = 0
        self.assertTrue(views.lookup_state("quest_template",b"unknown")["whole_empty"])
        with self.assertRaisesRegex(ExportError,"content.unknown_relation"):
            views.lookup_state("not_a_quest_table",b"unknown")

    def test_lookup_retains_sql_null_and_empty_field_states(self):
        views = c.ContentViews.__new__(c.ContentViews)
        views.spec = c.POLICY["families"]["quests"]
        row = types.SimpleNamespace(cells=lambda:(None,b"",b"value"))
        views.catalog = types.SimpleNamespace(relations={"quest_template":{"rows":1}},lookup=lambda t,k:iter([row]))
        self.assertEqual(views.lookup_state("quest_template",b"key")["field_states"],[["null","empty","value"]])

    def test_gameobject_and_creature_endpoints_include_alternatives(self):
        views = c.ContentViews.__new__(c.ContentViews)
        tables = {"creature":[(1,10)],"gameobject":[(2,20)],
                  "creature_spawn_entry":[(3,10)],"gameobject_spawn_entry":[(4,20)]}
        def scan(table):
            return iter(types.SimpleNamespace(ordinal=i,cells=lambda row=row:tuple(str(n).encode() for n in row))
                        for i,row in enumerate(tables[table]))
        views.catalog = types.SimpleNamespace(scan=scan)
        views.schema = {t:{"columns":[{"COLUMN_NAME":"guid"},{"COLUMN_NAME":"entry" if t.endswith("_entry") else "id"}]} for t in tables}
        records = []
        views.endpoint_plans({("creature",10):[["creature_questrelation",0,9]],
                              ("gameobject",20):[["gameobject_involvedrelation",0,9]]},types.SimpleNamespace(add=records.append))
        self.assertEqual([r[2] for r in records],[1,3,2,4])
        self.assertEqual(records[-1][-1],[["gameobject_involvedrelation",0,9]])

    def test_whole_quest_inventory_includes_archive_only_poi(self):
        spec = c.POLICY["families"]["quests"]
        self.assertEqual(len(spec["tables"]),7)
        self.assertEqual(len(spec["tables"]["quest_template"]["fields"]),131)
        self.assertEqual(spec["tables"]["quest_poi"]["status"],"archive-only")
        self.assertIn("content.sql_occurrences_not_F1",c.REFUSALS)
