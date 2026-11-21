"""Authored DBC catalog cases, no installation or database required.
SPDX-License-Identifier: GPL-3.0-or-later
"""
import hashlib
import struct
import unittest
from unittest.mock import Mock
from .catalog_dbc import CONTRACT, DbcView, DbcViews, fill_locale
from .core import ExportError


def fixture(rows, fmt="ni", strings=b"\0", size=None, fields=None):
    size = size or len(rows[0]) if rows else size or 8
    return struct.pack("<4s4I", b"WDBC", len(rows), fields or len(fmt), size, len(strings))+b"".join(rows)+strings


def view(raw, fmt="ni", size=8):
    return DbcView(lambda at, count: raw[at:at+count], {"sha256": hashlib.sha256(raw).hexdigest(), "bytes": len(raw)},
                   {"format": fmt, "record_bytes": size})


class CatalogDbcTests(unittest.TestCase):
    def test_sparse_duplicate_last_physical_wins_and_absence(self):
        table = view(fixture([struct.pack("<II", 9, 17), struct.pack("<II", 2, 3), struct.pack("<II", 9, 99)]))
        self.assertEqual(table.lookup(9), {"ordinal": 2, "key": 9, "values": [[0,"n",9],[1,"i",99]]})
        self.assertEqual(table.record(0)["values"][1][2], 17)
        self.assertIsNone(table.lookup(3))
        self.assertEqual(table.report()["lookup_extent"], 10)
        self.assertEqual(table.report()["duplicate_keys"], 1)

    def test_unindexed_rows_use_ordinal_not_first_word(self):
        table = view(fixture([struct.pack("<II", 5, 10), struct.pack("<II", 5, 11)], "ii"), "ii")
        self.assertEqual(table.lookup(1)["values"], [[0,"i",5],[1,"i",11]])
        self.assertIsNone(table.lookup(5))

    def test_packed_bytes_and_pinned_outfit_word(self):
        table = view(fixture([b"\x09\x02\x03\x04"], "bbbb", size=4), "bbbb", 4)
        self.assertEqual(table.lookup(0)["values"], [[0,"b",9],[1,"b",2],[2,"b",3],[3,"b",4]])
        layout = next(l for l in CONTRACT["loaders"] if l["file"] == "CharStartOutfit.dbc")
        raw = fixture([struct.pack("<II", 7, 0x00010801)+bytes(144)], layout["format"], size=152)
        table = view(raw, layout["format"], 152)
        self.assertEqual(table.lookup(7)["values"][0], [1,"i",0x00010801])
        self.assertEqual(table.fields, 41)

    def test_strings_preserve_bytes_offsets_and_float_bits(self):
        table = view(fixture([struct.pack("<III", 2, 1, 0x80000000)], "nsf", b"\0\xff\0"), "nsf", 12)
        self.assertEqual(table.lookup(2)["values"], [[0,"n",2],[1,"s","ff"],[2,"f",0x80000000]])
        for offset, strings, reason in [(4,b"\0a\0","dbc.string_offset"), (1,b"\0"+b"a"*16385+b"\0","dbc.string_termination_or_limit")]:
            with self.subTest(reason=reason), self.assertRaisesRegex(ExportError, "^"+reason+"$"):
                view(fixture([struct.pack("<II", 2, offset)], "ns", strings), "ns")

    def test_zero_truncated_wrong_layout_and_surplus_refuse(self):
        good = fixture([struct.pack("<II", 2, 3)])
        cases = [(b"", "dbc.byte_limit_or_truncated_header"), (good[:-1], "dbc.exact_length"),
                 (good+b"\0", "dbc.exact_length"), (b"ABCD"+good[4:], "dbc.magic"),
                 (fixture([struct.pack("<II",2,3)], fields=3), "dbc.layout")]
        for raw, reason in cases:
            with self.subTest(reason=reason), self.assertRaisesRegex(ExportError, "^"+reason+"$"):
                view(raw)

    def test_missing_mandatory_is_not_empty_or_archive(self):
        catalog = Mock()
        catalog.manifest = {"dbc_root": {}, "core_revision": CONTRACT["core_revision"]}
        catalog._json.return_value = {"Spell.dbc": {"sha256":"0"*64,"bytes":1}}
        with self.assertRaisesRegex(ExportError, "^dbc.mandatory_file_missing$"):
            DbcViews(catalog)
        self.assertNotIn("Spell.dbc", {l["file"] for l in CONTRACT["loaders"]})
        self.assertEqual(len(CONTRACT["loaders"]), 61)
        self.assertEqual(len(CONTRACT["zero_byte_archive"]), 4)

    def test_locale_fills_empty_slots_by_physical_ordinal(self):
        base = [{"ordinal":0,"key":9,"values":[[0,"n",9],[1,"s",""],[2,"s","62617365"]]}]
        overlay = [{"ordinal":0,"key":2,"values":[[0,"n",2],[1,"s","6c6f63616c"],[2,"s","6f74686572"]]}]
        result = fill_locale(base, overlay)
        self.assertEqual(result[0], {"ordinal":0,"key":9,"values":[[0,"n",9],[1,"s","6c6f63616c"],[2,"s","62617365"]]})
        self.assertEqual(fill_locale(result, base), result)
        with self.assertRaisesRegex(ExportError, "^dbc.locale_record_count$"):
            fill_locale(base, [])

    def test_bad_hash_and_changed_chunk_never_expose_rows(self):
        raw = bytearray(fixture([struct.pack("<II", 2,3)]))
        with self.assertRaisesRegex(ExportError, "^catalog.hash$"):
            DbcView(lambda at,count: bytes(raw[at:at+count]), {"sha256":"0"*64,"bytes":len(raw)})
        table = view(bytes(raw))
        table.cached = None
        table.read = lambda at,count: bytes(raw[at:at+count])
        raw[24] = 99
        with self.assertRaisesRegex(ExportError, "^catalog.hash$"):
            table.lookup(2)

    def test_physical_permutation_changes_both_record_and_index_roots(self):
        records = [struct.pack("<II", 10, 5), struct.pack("<II", 2, 7)]
        a, b = view(fixture(records)).report(), view(fixture(records[::-1])).report()
        self.assertNotEqual(a["physical_sha256"], b["physical_sha256"])
        self.assertNotEqual(a["decoded_sha256"], b["decoded_sha256"])
        self.assertNotEqual(a["index_sha256"], b["index_sha256"])

    def test_output_record_limit_refuses_without_clipping_strings(self):
        raw = fixture([struct.pack("<5I", 2, 1, 1, 1, 1)], "nssss", b"\0"+b"a"*16384+b"\0")
        table = view(raw, "nssss", 20)
        with self.assertRaisesRegex(ExportError, "^dbc.record_output_limit$"):
            table.report()

    def test_locale_reader_applies_first_nonempty_physical_overlay(self):
        base = view(fixture([struct.pack("<III",9,0,1)], "nss", b"\0base\0"), "nss", 12)
        overlay = view(fixture([struct.pack("<III",2,1,7)], "nss", b"\0local\0other\0"), "nss", 12)
        self.assertEqual(base.record(0, [overlay])["values"], [[0,"n",9],[1,"s","6c6f63616c"],[2,"s","62617365"]])

    def test_transport_views_keep_map_transfers_and_animation_id_order(self):
        layouts = {l["file"]:l for l in CONTRACT["loaders"]}
        rows = {}
        def add(name, words):
            layout = layouts[name+".dbc"]
            records = [struct.pack("<"+"I"*len(w), *w) for w in words]
            rows[name+".dbc"] = fixture(records, layout["format"], size=layout["record_bytes"])
        add("Map", [[0]*42, [1]+[0]*41])
        add("TaxiPath", [[900,0,1,0]])
        add("TaxiPathNode", [[100+i,900,i,1 if i<4 else 0,0,0,0,0,0] for i in range(10)])
        add("TransportAnimation", [[10,52,100,0,0,0,0],[2,52,100,99,0,0,0],[7,52,200,0,0,0,0]])
        add("GameObjectDisplayInfo", [[5]+[0]*11])
        class AuthoredViews(DbcViews):
            def __init__(self):
                pass
            def open(self, name, layout=None):
                return view(rows[name], layouts[name]["format"], layouts[name]["record_bytes"])
            def rows(self, table):
                if table == "transports":
                    yield 0, {"entry":b"52","name":b"boat","period":b"5000"}
                else:
                    yield 0, dict(zip(("entry","type","data0","data1","data2","displayid"), (b"52",b"15",b"900",b"30",b"1",b"5")))
        report = AuthoredViews().transports()
        path = report["templates"]["52"]["paths"][0]
        self.assertEqual(path["maps"], [0,1])
        self.assertEqual(path["spline_legs"], [[1,2],[5,6,7,8]])
        self.assertEqual(path["reason"], "interpolation_parameter_unproved")
        self.assertEqual(report["animations"]["52"]["total_time"], 200)
        self.assertEqual(report["animations"]["52"]["nodes"][0]["ordinal"], 0)
        self.assertEqual(report["animations"]["52"]["nodes"][0]["words"][3], 0)

    def test_transport_cursors_keep_order_but_verify_complete_multisets(self):
        import tempfile
        from pathlib import Path
        from .catalog_transports import capture_occurrences, specs, verify_occurrences
        from .core import canonical, decode_json
        transport = [{"entry":b"52","name":b"boat","period":b"99"}, {"entry":b"53","name":b"ship","period":b"100"}]
        gameobjects = [{"guid":b"9","id":b"52"}, {"guid":b"10","id":b"52"}]
        class Views:
            catalog = Mock()
            catalog._json.side_effect = lambda raw: decode_json(raw, 1048576)
            def rows(self, name):
                return enumerate(transport if name == "transports" else gameobjects)
        queries = specs()
        cursors = {queries["0"]["query"]: [[r["entry"],r["name"],r["period"]] for r in transport],
                   queries["1"]["query"]: [[r["guid"],r["id"],b"boat"] for r in gameobjects]}
        source = Mock()
        source.query_rows.side_effect = lambda query: iter(reversed(cursors[query]))
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)/"cursors"
            meta = capture_occurrences(source, root)
            report = verify_occurrences(Views(), root)
            self.assertEqual([report[str(i)]["rows"] for i in range(2)], [2,2])
            original = (root/"0.rows").read_bytes()
            for raw, reason in [(original[:-1],"catalog.truncated"), (original+b"x","dbc.occurrence_trailing")]:
                (root/"0.rows").write_bytes(raw)
                with self.subTest(reason=reason), self.assertRaisesRegex(ExportError, "^"+reason+"$"):
                    verify_occurrences(Views(), root)
            changed = original.replace(b"100",b"101")
            (root/"0.rows").write_bytes(changed)
            meta["streams"]["0"]["sha256"] = hashlib.sha256(changed).hexdigest()
            (root/"occurrences.json").write_bytes(canonical(meta))
            with self.assertRaisesRegex(ExportError, "^dbc.occurrence_mismatch$"):
                verify_occurrences(Views(), root)
            (root/"0.rows").write_bytes(original)
            meta["streams"]["0"]["query"] += " ORDER BY entry"
            (root/"occurrences.json").write_bytes(canonical(meta))
            with self.assertRaisesRegex(ExportError, "^dbc.occurrence_query$"):
                verify_occurrences(Views(), root)

    def test_catalog_owns_only_one_active_dbc_view(self):
        from types import SimpleNamespace
        raw = fixture([struct.pack("<II",2,3)])
        descriptor = {"sha256":hashlib.sha256(raw).hexdigest(),"bytes":len(raw)}
        inventory = {l["file"]: descriptor for l in CONTRACT["loaders"]}
        catalog = SimpleNamespace(manifest={"dbc_root":{},"core_revision":CONTRACT["core_revision"]},
                                  object=lambda _:b"", _json=lambda _:inventory,
                                  blob_range=lambda d,at,n:raw[at:at+n])
        first = DbcViews(catalog).open("SpellCategory.dbc")
        second = DbcViews(catalog).open("StableSlotPrices.dbc")
        self.assertEqual(second.lookup(2)["values"], [[0,"n",2],[1,"i",3]])
        with self.assertRaisesRegex(ExportError, "^dbc.not_server_loader$"):
            DbcViews(catalog).open("Spell.dbc")
        self.assertEqual(len(first.by_id), 0)
        with self.assertRaisesRegex(ExportError, "^dbc.view_closed$"):
            first.lookup(999)

    def test_global_locale_falls_back_to_available_human_race_names(self):
        from .catalog_dbc import locale_selection
        race = {"values": [[17+i,"s","78" if i in (2,5) else ""] for i in range(8)]}
        self.assertEqual(locale_selection(race, 0)["fallback_locales"], [2,2,2,2,2,5,2,2])
        self.assertEqual(locale_selection(race, 5)["default_locale"], 5)
        self.assertEqual(locale_selection(race, 255)["default_locale"], 2)
        self.assertEqual(locale_selection(race, 8)["effective_config_locale"], 0)
        with self.assertRaisesRegex(ExportError, "^dbc.locale_human_missing$"):
            locale_selection(None)
        with self.assertRaisesRegex(ExportError, "^dbc.locale_undetermined$"):
            locale_selection({"values":[[17+i,"s",""] for i in range(8)]})

    def test_archive_without_layout_cannot_prove_lookup_absence(self):
        raw = fixture([struct.pack("<II",2,3)])
        table = DbcView(lambda at,n:raw[at:at+n], {"sha256":hashlib.sha256(raw).hexdigest(),"bytes":len(raw)})
        with self.assertRaisesRegex(ExportError, "^dbc.layout_required$"):
            table.lookup(2)
        with self.assertRaisesRegex(ExportError, "^dbc.layout_required$"):
            table.record(0)
