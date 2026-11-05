"""Authored all-race/class, locale fallback and service order input cases."""
import unittest
from classic_export import catalog_content_globals as g
from classic_export.core import ExportError


class ContentGlobalTests(unittest.TestCase):
    def test_zero_masks_match_all_source_indices(self):
        self.assertEqual(len(g.mask_pairs(0,0)),88)
        self.assertIn([8,11],g.mask_pairs(0,0))
        self.assertEqual(g.mask_pairs(0x80,0x400),[[8,11]])

    def test_nonplayable_masks_skip_but_extra_bits_are_not_erased(self):
        self.assertEqual(g.mask_pairs(0x100,1),[])
        self.assertEqual(g.mask_pairs(1,0x20),[])
        self.assertEqual(g.mask_pairs(0x101,1),[[1,1]])
        self.assertEqual(g.mask_pairs(1,0x21),[[1,1],[1,6]])

    def test_skill_obtainability_requires_matching_dbc_masks_and_tier(self):
        row = dict(racemask=0,classmask=0,skill=10,step=1)
        infos = [[1,10,0x80,0x400,0,0,7,0],[2,10,1,1,0,0,99,0]]
        tier = [0]*33
        tier[17] = 75
        self.assertEqual(g.skill_pairs(row,infos,{7:tier}),[[8,11]])
        self.assertEqual(g.skill_pairs(dict(row,step=0),infos,{}),[[1,1],[8,11]])
        self.assertEqual(g.skill_pairs(dict(row,step=17),infos,{7:tier}),[])
        tier[17] = 0
        self.assertEqual(g.skill_pairs(row,infos,{7:tier}),[])

    def test_locale_null_empty_nul_prefix_and_override_are_distinct(self):
        self.assertEqual([g.locale_state(v) for v in (None,b"",b"\0tail",b"translated")],
            ["null:fallback","empty:fallback","nul_prefix:fallback","override_input"])

    def test_service_order_uses_numeric_slot_not_item_or_catalog_key(self):
        ordered,ties = g.service_order([[1,2,0],[1,1,1],[1,1,2],[2,1,3]])
        self.assertEqual(ordered,[[1,1,1],[1,1,2],[2,1,3],[1,2,0]])
        self.assertEqual(ties,[[1,1,[1,2]]])
        self.assertEqual(g.service_order([[1,1,0],[2,1,1]])[1],[])

    def test_service_retention_bound_refuses_without_clipping(self):
        with self.assertRaisesRegex(ExportError,"content.view_limit"):
            g.service_order([[1,1,1]]*65537)

    def test_complete_global_inventory_keeps_archival_and_ignored_fields(self):
        from classic_export.catalog_content import POLICY
        spec = POLICY["families"]["globals"]
        self.assertEqual(len(spec["tables"]),47)
        self.assertEqual(spec["tables"]["pet_familystats"]["status"],"archive-only")
        self.assertEqual(dict(spec["tables"]["broadcast_text"]["fields"])["verifiedbuild"],"raw")
        self.assertEqual(dict(spec["tables"]["gossip_menu_option"]["fields"])["box_coded"],"bool8")
        self.assertEqual(dict(spec["tables"]["npc_trainer"]["fields"])["reqlevel"],"u32")
