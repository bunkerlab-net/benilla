#!/usr/bin/env python3
"""Authored tests for conservative static anchor classification. No ELF runs."""
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest

SCRIPT = Path(__file__).with_name("observer-manifest.py")
SPEC = importlib.util.spec_from_file_location("observer_manifest", SCRIPT)
MANIFEST = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MANIFEST)


class AnchorClassification(unittest.TestCase):
    def test_dwarf_symbols_and_missing_are_distinct_without_admitting_any_site(self):
        base = Path(os.environ.get("CARGO_TARGET_DIR", Path(__file__).resolve().parents[1] / "target"))
        base.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="observer-manifest-test-", dir=base) as temporary:
            root = Path(temporary)
            def site(chain):
                return {"status":"refused_unpinned","anchors":[] if chain is None else [{"inline_chain_raw":chain}],"reason":"missing exhaustive machine cover"}
            source = {"tools":[{"tool":"fixture","sites":[site(["0x401000","owner()","fixture.c:12"]),site(["0x401001","owner()","??:?"]),site(None)]},{"tool":"ad","sites":[site(["0x402000","guessed()","fixture.c:99"])]}]}
            (root/"manifest.json").write_text(json.dumps(source))
            MANIFEST.classify(root)
            result = json.loads((root/"anchor-classification.json").read_text())
            self.assertEqual(result["anchor_class_counts"]["fixture"], {"a":1,"b":1,"c":1})
            self.assertEqual(result["anchor_class_counts"]["ad"], {"a":0,"b":1,"c":0})
            for tool in result["tools"]:
                for item in tool["sites"]:
                    self.assertEqual(item["status"], "refused_unpinned")
                    self.assertEqual(item["missing_for_exhaustive_coverage"], "missing exhaustive machine cover")
                    if item["anchor_class"] == "c":
                        self.assertIn("missing_anchor", item)
            receipt = json.loads((root/"anchor-classification.sha256.json").read_text())
            self.assertEqual(receipt["sha256"], MANIFEST.digest((root/receipt["path"]).read_bytes()))
            with self.assertRaises(FileExistsError):
                MANIFEST.classify(root)

    def test_retained_imports_do_not_turn_null_symbols_into_names(self):
        base = Path(os.environ.get("CARGO_TARGET_DIR", Path(__file__).resolve().parents[1] / "target"))
        base.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="observer-import-test-", dir=base) as temporary:
            root = Path(temporary)
            binary = root/"authored-not-executed"
            binary.write_bytes(b"authored identity fixture")
            (root/"fixture").mkdir()
            (root/"fixture/elf.txt").write_text("[Requesting program interpreter: /fixture/loader]\n")
            (root/"fixture/dynamic-imports.txt").write_text("Type: DYN (Position-Independent Executable file)\n(FLAGS_1) Flags: PIE\n(NEEDED) Shared library: [libm.so.6]\n0: 0000 0 NOTYPE LOCAL DEFAULT UND \n1: 0000 0 FUNC GLOBAL DEFAULT UND lrint@GLIBC_2.2.5 (2)\n")
            (root/"manifest.json").write_text(json.dumps({"tools":[{"tool":"fixture","path":str(binary),"executable_sha256":MANIFEST.digest(binary.read_bytes())}]}))
            MANIFEST.library_imports(root, retained=True)
            tool = json.loads((root/"dynamic-imports-v2.json").read_text())["tools"][0]
            self.assertEqual(tool["undefined_dynamic_symbols"], ["lrint"])
            self.assertEqual(tool["numeric_import_candidates"], ["lrint"])
            self.assertEqual(tool["dt_needed"], ["libm.so.6"])
            self.assertEqual(tool["requested_interpreter"], "/fixture/loader")
            self.assertIn("unobserved", tool["actual_loaded_libraries"])


if __name__ == "__main__":
    unittest.main()
