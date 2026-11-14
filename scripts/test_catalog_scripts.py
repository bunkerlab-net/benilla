"""Authored script catalog and registration input cases for the CI suite."""
import hashlib
import json
from pathlib import Path
import tempfile
import types
import unittest
from classic_export import catalog_scripts as s
from classic_export.core import ExportError, canonical


class ScriptCatalogTests(unittest.TestCase):
    def test_commands_zero_one_and_ties_are_not_discarded(self):
        rows = [{"id": b"7", "command": b"0", "priority": b"2"},
                {"id": b"7", "command": b"1", "priority": b"2"}]
        result = s.db_summary(iter(enumerate(rows)))
        self.assertEqual(result, {"commands": {"0": 1, "1": 1}, "equal_priority_groups": 1,
                                  "equal_priority_rows": 2})

    def test_signed_eventai_selectors_and_all_action_slots(self):
        row = {"id": b"1", "creature_id": b"-7"}
        for action in range(1, 4):
            row[f"action{action}_type"] = b"1"
            for parameter in range(1, 4):
                row[f"action{action}_param{parameter}"] = str(parameter-2).encode()
        result = s.event_summary([(0, row)])
        self.assertEqual(result["guid_selectors"], 1)
        self.assertEqual(result["positive_text_parameters"], 3)
        self.assertEqual(result["negative_text_parameters"], 3)
        self.assertEqual(s.signed(b"-2147483648"), -2147483648)
        self.assertEqual(s.signed(b"4294967295"), -1)

    def test_every_callback_slot_and_factory_kind_is_explicit(self):
        self.assertEqual(len(s.SLOTS), 22)
        self.assertEqual(s.REGISTRIES, ("scriptdev", "creature_ai", "motion", "spell", "aura"))
        with self.assertRaisesRegex(ExportError, "scripts.registry_slots"):
            s.Registry().finish()

    def trace(self, attempts):
        lines = []
        accepted = 0
        def add(kind, payload):
            lines.append(f"1\t{len(lines)+1}\t{kind}\t{payload}\n".encode())
        for index, (name, script_id, old) in enumerate(attempts):
            operation = index+1
            add("script_registration", name+" 1")
            add("script_registration_site", f"{operation} /reference/source/src/game/test.cpp 7 void AddSC_test()")
            for slot in s.SLOTS:
                add("script_callback_binding", f"{operation} {slot} 0")
            if script_id:
                add("script_binding", f"{script_id} {name}")
                add("script_registry_store", f"{script_id} {old} 0x{index+16:x} {accepted}")
                accepted += 1
        return lines

    def registry(self, attempts):
        registry = s.Registry()
        for line in self.trace(attempts):
            registry.feed(line)
        return registry.finish()

    def test_absent_name_outcome_is_unobserved_not_inferred_rejection(self):
        result = self.registry([("present", 7, "(nil)"), ("absent", 0, "(nil)"),
                                ("present", 7, "0x10")])
        self.assertEqual(result["attempts"], 3)
        self.assertEqual(result["unresolved_attempts"], 1)
        self.assertEqual(result["records"][1]["outcome"], "unobserved_binding_outcome")
        self.assertEqual(result["accepted_overrides"], 1)
        self.assertEqual(result["final_bindings"], 1)
        self.assertEqual(result["callback_slots"], 66)

    def test_duplicate_slot_missing_store_and_wrong_previous_binding_refuse(self):
        for mutation, reason in (("slot", "scripts.registry_slots"),
                                 ("store", "scripts.registry_store"),
                                 ("old", "scripts.registry_store")):
            lines = self.trace([("present", 7, "(nil)")])
            if mutation == "slot":
                lines[3] = lines[3].replace(s.SLOTS[1].encode(), s.SLOTS[0].encode())
            elif mutation == "store":
                lines.pop()
            else:
                lines[-1] = lines[-1].replace(b"(nil)", b"0xff")
            registry = s.Registry()
            with self.assertRaisesRegex(ExportError, reason):
                for line in lines:
                    registry.feed(line)
                registry.finish()

    def test_factory_duplicates_rejected_and_explicit_overrides(self):
        for kind in s.REGISTRIES[1:]:
            self.assertEqual(s.registry_outcome(kind, False, False), "accepted")
            self.assertEqual(s.registry_outcome(kind, True, False), "rejected_duplicate")
            if kind in ("spell", "aura"):
                with self.assertRaisesRegex(ExportError, "scripts.registry_replace_not_supported"):
                    s.registry_outcome(kind, True, True)
            else:
                self.assertEqual(s.registry_outcome(kind, True, True), "accepted_override")
        self.assertEqual(s.registry_outcome("scriptdev", True, False), "accepted_override")
        with self.assertRaisesRegex(ExportError, "scripts.registry_kind"):
            s.registry_outcome("unknown", False, False)

    def test_forged_linked_source_and_manifest_refuse(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root/"manifest.json"
            path.write_bytes(canonical({"core_revision":"wrong"}))
            with self.assertRaisesRegex(ExportError, "scripts.linked_source"):
                s.reference_manifest(path)

    def test_ordered_loader_multiset_preserves_ties(self):
        expected = [(1, b"a"), (1, b"b"), (2, b"c")]
        self.assertTrue(s.ordered_groups(expected, [(1,b"b"),(1,b"a"),(2,b"c")]))
        self.assertFalse(s.ordered_groups(expected, [(2,b"c"),(1,b"a"),(1,b"b")]))
        self.assertFalse(s.ordered_groups(expected, [(1,b"a"),(1,b"a"),(2,b"c")]))

    def fixture(self, directory, count=2):
        columns = {}
        for spec in s.specs().values():
            if "UNION" in spec["query"]:
                for table in spec["relations"]:
                    columns.setdefault(table, set()).add("scriptname")
            else:
                table = spec["relations"][0]
                names = spec["query"][7:].split(" FROM ")[0].split(",")
                columns.setdefault(table, set()).update(n.strip().lower() for n in names if "COUNT(" not in n)
                if "COUNT(" in spec["query"]:
                    columns[table].add("entry")
                if "ORDER BY" in spec["query"]:
                    columns[table].update(n.strip().lower() for n in spec["query"].split(" ORDER BY ")[1].split(","))
        columns = {t:sorted(names) for t,names in columns.items()}
        rows = {t:[] for t in columns}
        table = s.DB_TABLES[0]
        for index in range(count):
            rows[table].append({c:str(index if c=="id" else index%2 if c=="command" else 1 if c=="priority" else 0).encode() for c in columns[table]})
        schema = {"tables":{t:{"columns":[{"COLUMN_NAME":c,"DATA_TYPE":"int"} for c in names]} for t,names in columns.items()}}
        def scan(table):
            for ordinal,row in enumerate(rows[table]):
                yield types.SimpleNamespace(ordinal=ordinal,cells=lambda r=row:list(r.values()))
        catalog = types.SimpleNamespace(scan=scan,manifest={"schema_root":"schema"},object=lambda _:canonical(schema),_json=json.loads)
        views = s.ScriptViews(catalog)
        def query(sql):
            spec = next(v for v in s.specs().values() if v["query"]==sql)
            return (cells for _,cells in sorted(views.expected(spec),key=lambda pair:pair[0]))
        root = Path(directory)/"occurrences"
        s.capture_occurrences(types.SimpleNamespace(query_rows=query),root)
        return root,views

    def test_complete_streams_exceed_old_row_limit_without_clipping(self):
        with tempfile.TemporaryDirectory() as directory:
            root,views = self.fixture(directory,4100)
            report = views.verify_occurrences(root)
            self.assertEqual(report["0"]["rows"],4100)
            self.assertEqual(sum(r["rows"] for r in report.values()),4100)

    def test_rehashed_forgery_exhaustion_trailing_and_query_drift_refuse(self):
        for mutation,reason in (("forged","scripts.view_mismatch"),("empty","scripts.exhausted"),
                                ("extra","scripts.trailing"),("query","scripts.query"),
                                ("ordinal","catalog.duplicate_row")):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as directory:
                root,views = self.fixture(directory)
                path = root/"0.rows"
                raw = path.read_bytes()
                if mutation=="forged":
                    raw = raw.replace(b"0",b"9")
                elif mutation=="empty":
                    raw = b""
                elif mutation=="extra":
                    raw += b"x"
                elif mutation=="ordinal":
                    raw = raw[:4]+(9).to_bytes(8,"little")+raw[12:]
                path.write_bytes(raw)
                meta_path = root/"occurrences.json"
                meta = json.loads(meta_path.read_bytes())
                meta["streams"]["0"].update(bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest())
                if mutation=="query":
                    meta["streams"]["0"]["query"] += " LIMIT 1"
                meta_path.write_bytes(canonical(meta))
                with self.assertRaisesRegex(ExportError,reason):
                    views.verify_occurrences(root)

    def test_input_descriptor_binds_actual_trace_and_linked_binary_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            def descriptor(path, relative=None):
                raw = path.read_bytes()
                result = {"path":str(path),"bytes":len(raw),"sha256":hashlib.sha256(raw).hexdigest()}
                if relative is not None:
                    result["relative"] = relative
                return result
            binary = root/"mangosd"
            observer = root/"transport-observer.so"
            binary.write_bytes(b"authored-executable")
            observer.write_bytes(b"authored-observer")
            trace = root/"trace.tsv"
            trace.write_bytes(b"".join(self.trace([("present",7,"(nil)")]))
                             + b"1\t99\tunrelated\tretained and hashed\n")
            definitions = root/"definitions.json"
            definitions.write_bytes(b"{}")
            build = root/"build.json"
            build.write_bytes(canonical({"files":[{**descriptor(binary),"path":"/recorded/mangosd"},
                                                 {**descriptor(observer),"path":"/recorded/transport-observer.so"}]}))
            manifest = root/"manifest.json"
            manifest.write_bytes(canonical({"core_revision":s.CORE_REVISION,"classic_db_revision":s.DATABASE_REVISION,
                "reference_execution_sealed":False,"files":[{"path":name,**{k:descriptor(path)[k] for k in ("bytes","sha256")}}
                    for name,path in (("trace",trace),("build",build),("definitions",definitions))]}))
            value = {"format":"benilla-registration-input-v1","manifest":descriptor(manifest),
                     "trace":descriptor(trace,"trace"),"build":descriptor(build,"build"),
                     "definitions":descriptor(definitions,"definitions"),
                     "linked":{"executable":descriptor(binary),"observer":descriptor(observer)}}
            input_path = root/"input.json"
            input_path.write_bytes(canonical(value))
            result = s.registration_input(input_path)
            self.assertEqual(result["registry"]["attempts"],1)
            self.assertEqual(result["trace_records"],27)
            self.assertEqual(result["input"]["trace"]["sha256"],value["trace"]["sha256"])
            binary.write_bytes(b"changed-executable")
            with self.assertRaisesRegex(ExportError,"scripts.linked_source"):
                s.registration_input(input_path)
            binary.write_bytes(b"authored-executable")
            trace.write_bytes(trace.read_bytes().replace(b"unrelated",b"tampered!"))
            with self.assertRaisesRegex(ExportError,"scripts.input_hash"):
                s.registration_input(input_path)
