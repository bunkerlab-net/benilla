"""Authored complete-store loot input validation and cycle refusals."""
import unittest
from classic_export import catalog_content_loot as l
from classic_export.core import ExportError


class ContentLootTests(unittest.TestCase):
    def row(self, **changes):
        row = dict(entry=1,item=10,chanceorquestchance=0x3f800000,groupid=0,
                   mincountorref=1,maxcount=1,condition_id=0)
        row.update(changes)
        return row

    def test_nine_actual_stores_do_not_admit_spell_loot(self):
        self.assertEqual(len(l.STORES),9)
        self.assertIn("mail_loot_template",l.STORES)
        self.assertIn("disenchant_loot_template",l.STORES)
        self.assertNotIn("spell_loot_template",l.STORES)

    def test_invalid_rows_preserve_pinned_skip_precedence(self):
        self.assertEqual(l.loot_status(self.row(maxcount=256,item=99,condition_id=99),{10},set()),"skip:maxcount")
        self.assertEqual(l.loot_status(self.row(item=99,condition_id=99),{10},set()),"skip:missing_condition")
        self.assertEqual(l.loot_status(self.row(item=99),{10},set()),"skip:missing_item")
        self.assertEqual(l.loot_status(self.row(groupid=128),{10},set()),"skip:group")
        self.assertEqual(l.loot_status(self.row(mincountorref=0),{10},set()),"skip:mincount")
        self.assertEqual(l.loot_status(self.row(mincountorref=2),{10},set()),"skip:count_range")

    def test_zero_group_chance_and_negative_quest_chance(self):
        self.assertEqual(l.loot_status(self.row(chanceorquestchance=0x80000000),{10},set()),"skip:ungrouped_zero")
        self.assertEqual(l.loot_status(self.row(chanceorquestchance=0,groupid=1),{10},set()),"retained:loader_validation_pending")
        self.assertEqual(l.loot_status(self.row(chanceorquestchance=0xbf800000),{10},set()),"retained:loader_validation_pending")
        self.assertEqual(l.loot_status(self.row(chanceorquestchance=1),{10},set()),"skip:low_chance")

    def test_reference_checks_do_not_require_an_item_prototype(self):
        self.assertEqual(l.loot_status(self.row(mincountorref=-2,item=999),set(),set()),"retained:loader_validation_pending")
        self.assertEqual(l.loot_status(self.row(mincountorref=-2,chanceorquestchance=0xbf800000),set(),set()),"skip:negative_reference_chance")
        with self.assertRaisesRegex(ExportError,"content.loot_reference_negation_undefined"):
            l.loot_status(self.row(mincountorref=-2147483648),set(),set())

    def test_missing_condition_is_not_a_fabricated_effective_manager(self):
        self.assertEqual(l.loot_status(self.row(condition_id=7),{10},{7}),"retained:condition_validation_pending")

    def test_cycles_refuse_but_repeated_dag_paths_remain_valid(self):
        l.check_reference_cycles({1:[2,2,3],2:[3],3:[]})
        for edges in ({1:[1]},{1:[2],2:[1]},{1:[2],2:[3],3:[2]}):
            with self.assertRaisesRegex(ExportError,"content.loot_reference_cycle"):
                l.check_reference_cycles(edges)
