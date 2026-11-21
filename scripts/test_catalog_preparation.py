"""Authored whole-catalog aggregation refusal cases, imported by exporter CI."""
import copy
import hashlib
from pathlib import Path
import tempfile
import unittest
from classic_export import catalog_preparation as p
from classic_export.core import ExportError, canonical, digest
from classic_export.catalog_io import encode_row

CASES = p.decode_json(Path(p.__file__).with_name("catalog_preparation_cases.json").read_bytes(),1048576)


class CatalogPreparationTests(unittest.TestCase):
    def request(self):
        return {"format":"benilla-catalog-preparation-input-v1","catalog_sha256":"catalog",
            "source_snapshot":"snapshot","reference_execution":copy.deepcopy(p.POLICY["pending_reference"]),
            "occurrences":{name:{} for name in p.OCCURRENCES},"registration":{},"initial_snapshot":{},"environment":{}}

    def family(self, name="quests"):
        spec = p.POLICY["families"][name]
        tables = {t:{"rows":1,"raw_sha256":"raw"} for t in spec["tables"]}
        report = {"format":spec["format"],"catalog_sha256":"catalog","complete":False,
            "families":copy.deepcopy(tables),"refusals":sorted(spec["refusals"])}
        report["view_sha256"] = digest(report)
        return report,{"catalog_sha256":"catalog","tables":tables}

    def test_pending_execution_is_typed_and_has_no_fake_hash(self):
        request = self.request()
        p.validate_input(request,"catalog","snapshot")
        for value in ({"sha256":"0"*64},None,{"status":"sealed"},"pending"):
            request["reference_execution"] = value
            with self.assertRaisesRegex(ExportError,"preparation.execution_pending"):
                p.validate_input(request,"catalog","snapshot")

    def test_wrong_snapshot_and_missing_or_surplus_occurrences_refuse(self):
        with self.assertRaisesRegex(ExportError,"preparation.snapshot"):
            p.validate_input(self.request(),"wrong","snapshot")
        for change in (lambda v:v.pop("spawn"),lambda v:v.update(extra={})):
            request = self.request()
            change(request["occurrences"])
            with self.assertRaisesRegex(ExportError,"preparation.occurrences"):
                p.validate_input(request,"catalog","snapshot")

    def test_cross_family_omission_surplus_and_rehashed_wrong_rows_refuse(self):
        report,source = self.family()
        p.check_family("quests",report,source)
        table = next(iter(report["families"]))
        for change in (lambda r:r["families"].pop(table),
                       lambda r:r["families"].update(extra={}),
                       lambda r:r["families"][table].update(rows=0),
                       lambda r:r["families"][table].update(raw_sha256="forged")):
            forged = copy.deepcopy(report)
            change(forged)
            forged.pop("view_sha256")
            forged["view_sha256"] = digest(forged)
            with self.assertRaisesRegex(ExportError,"preparation.(family_tables|cross_family)"):
                p.check_family("quests",forged,source)

    def test_unknown_family_complete_flag_and_cleared_obligations_refuse(self):
        report,source = self.family()
        with self.assertRaisesRegex(ExportError,"preparation.unknown_family"):
            p.check_family("unknown",report,source)
        report["complete"] = True
        with self.assertRaisesRegex(ExportError,"preparation.complete"):
            p.check_family("quests",report,source)
        report["complete"] = False
        report["refusals"] = []
        with self.assertRaisesRegex(ExportError,"preparation.obligations"):
            p.check_family("quests",report,source)

    def test_wrong_build_descriptor_does_not_accept_a_new_claimed_hash(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder)/"build"
            path.write_bytes(b"actual")
            valid = {"path":str(path),"bytes":6,"sha256":hashlib.sha256(b"actual").hexdigest()}
            self.assertEqual(p.bind_descriptor(valid),{k:valid[k] for k in ("bytes","sha256")})
            with self.assertRaisesRegex(ExportError,"preparation.input_hash"):
                p.bind_descriptor({"path":str(path),"bytes":6,"sha256":hashlib.sha256(b"forged").hexdigest()})

    def test_binary_source_ordinals_and_stream_exhaustion_refuse(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder)/"rows"
            path.write_bytes(encode_row(0,[None,b"",b"tail"]))
            result = p.index_stream(path,"frames",3)
            self.assertEqual((result["rows"],len(result["pages"])),(1,1))
            path.write_bytes(encode_row(1,[b"x"]))
            with self.assertRaisesRegex(ExportError,"catalog.duplicate_row"):
                p.index_stream(path,"frames",1)
            path.write_bytes(encode_row(0,[b"x"])[:-1])
            with self.assertRaisesRegex(ExportError,"preparation.exhausted"):
                p.index_stream(path,"frames",1)

    def test_pages_bound_bytes_and_records_without_clipping(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder)/"rows.jsonl"
            raw = b"[0]\n"*4097
            path.write_bytes(raw)
            result = p.index_stream(path,"jsonl")
            self.assertEqual([v["rows"] for v in result["pages"]],[4096,1])
            self.assertEqual(result["sha256"],hashlib.sha256(raw).hexdigest())
            self.assertEqual(result["bytes"],len(raw))

    def seal_candidate(self, root, manifest):
        manifest.pop("preparation_sha256",None)
        manifest["preparation_sha256"] = hashlib.sha256(p.DOMAIN+canonical(manifest)).hexdigest()
        identity = {"format":"benilla-production-input-identity-v2","preparation_sha256":manifest["preparation_sha256"],
                    "runtime_admitted":False}
        identity["identity_sha256"] = digest(identity)
        (root/"preparation.json").write_bytes(canonical(manifest))
        (root/"identity.json").write_bytes(canonical(identity))

    def candidate(self, folder):
        actual,expected = Path(folder)/"actual",Path(folder)/"expected"
        for root in (actual,expected):
            root.mkdir()
            (root/"families").mkdir()
            for name in p.FAMILIES:
                (root/"families"/(name+".json")).write_bytes(b"{}")
            (root/"rows.jsonl").write_bytes(b"".join(canonical(r)+b"\n" for r in CASES["original"]))
        index = p.index_stream(actual/"rows.jsonl","jsonl")
        manifest = {"format":"benilla-native-catalog-preparation-v2","complete":False,
            "catalog_sha256":"a"*64,"reference_execution":copy.deepcopy(p.POLICY["pending_reference"]),
            "input_bindings":{"registration":{"bytes":1,"sha256":"a"*64}},"streams":{"rows.jsonl":index}}
        for root in (actual,expected):
            self.seal_candidate(root,manifest)
        return actual,expected,manifest

    def test_false_negative_missing_surplus_and_out_of_order_records_refuse(self):
        for case in CASES["cases"]:
            raw = b"".join(canonical(r)+b"\n" for r in case["records"])
            with self.subTest(case=case["name"]),tempfile.TemporaryDirectory() as folder:
                actual,expected,manifest = self.candidate(folder)
                (expected/"rows.jsonl").write_bytes(raw)
                with self.assertRaisesRegex(ExportError,case["diagnostic"]):
                    p.compare_output(expected,actual,manifest)

    def test_rehashing_supplied_root_does_not_replace_source_comparison(self):
        with tempfile.TemporaryDirectory() as folder:
            actual,expected,manifest = self.candidate(folder)
            (expected/"rows.jsonl").write_bytes(b"[false]\n[true]\n")
            forged = copy.deepcopy(manifest)
            forged["streams"]["rows.jsonl"] = p.index_stream(expected/"rows.jsonl","jsonl")
            self.seal_candidate(expected,forged)
            with self.assertRaisesRegex(ExportError,"preparation.manifest_mismatch"):
                p.compare_output(expected,actual,manifest)

    def test_identity_family_file_surplus_and_complete_forgery_refuse(self):
        for changed,diagnostic in (("identity","identity_mismatch"),("family","family_mismatch"),
                                   ("surplus","surplus"),("complete","complete")):
            with self.subTest(changed=changed),tempfile.TemporaryDirectory() as folder:
                actual,expected,manifest = self.candidate(folder)
                if changed == "identity":
                    (expected/"identity.json").write_bytes(b"{\"forged\":true}")
                elif changed == "family":
                    (expected/"families/quests.json").write_bytes(b"{\"complete\":true}")
                elif changed == "surplus":
                    (expected/"extra").write_bytes(b"x")
                else:
                    forged = dict(manifest,complete=True)
                    (expected/"preparation.json").write_bytes(canonical(forged))
                with self.assertRaisesRegex(ExportError,"preparation."+diagnostic):
                    p.compare_output(expected,actual,manifest)

    def test_every_obligation_has_one_explicit_kind_and_stays_pending(self):
        reports = {name:{"refusals":list(spec["refusals"])} for name,spec in p.POLICY["families"].items()}
        coverage = {"loaders":{"source:1":{}},"tables":{"outside":{"status":"consumed"}}}
        result = p.obligations(reports,coverage)
        self.assertEqual(set(result),set(p.POLICY["obligation_kinds"]))
        ids = [r["id"] for rows in result.values() for r in rows]
        self.assertEqual(len(ids),len(set(ids)))
        self.assertTrue(all(r["status"] == "pending" for rows in result.values() for r in rows))
        self.assertIn("view.outside",ids)
        self.assertIn("binding.initial_snapshot",ids)

    def test_observed_cursor_order_must_match_bound_occurrence_metadata(self):
        for name in p.OCCURRENCES:
            observed = {"0":{"rows":2,"occurrence_sha256":"recorded"}}
            report = ({"joined":observed} if name == "spawn" else {"streams":observed} if name == "pool"
                      else {"transports":{"occurrences":observed}} if name == "dbc" else {"loaders":observed})
            metadata = {"streams":{"0":{"rows":2,"sha256":"recorded"}}}
            p.check_occurrences(name,report,metadata)
            observed["0"]["occurrence_sha256"] = "same_rows_other_order"
            with self.assertRaisesRegex(ExportError,"preparation.occurrence_binding"):
                p.check_occurrences(name,report,metadata)

    def test_rehashed_wrong_snapshot_execution_and_build_refuse(self):
        for kind in ("snapshot","execution","build"):
            with self.subTest(kind=kind),tempfile.TemporaryDirectory() as folder:
                actual,expected,manifest = self.candidate(folder)
                forged = copy.deepcopy(manifest)
                if kind == "snapshot":
                    forged["catalog_sha256"] = "b"*64
                elif kind == "execution":
                    forged["reference_execution"] = {"status":"sealed","sha256":"b"*64}
                else:
                    forged["input_bindings"]["registration"]["sha256"] = "b"*64
                self.seal_candidate(expected,forged)
                with self.assertRaisesRegex(ExportError,"preparation.manifest_mismatch"):
                    p.compare_output(expected,actual,manifest)

    def test_exact_canonical_root_bytes_are_required(self):
        with tempfile.TemporaryDirectory() as folder:
            actual,expected,manifest = self.candidate(folder)
            p.compare_output(expected,actual,manifest)
            (expected/"preparation.json").write_bytes(b" "+canonical(manifest))
            with self.assertRaisesRegex(ExportError,"catalog.json"):
                p.compare_output(expected,actual,manifest)
