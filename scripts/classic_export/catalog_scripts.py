"""Complete pinned script inputs; missing execution evidence always refuses admission.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from __future__ import annotations
from collections import Counter, defaultdict
import hashlib
import os
from pathlib import Path
import struct

from .core import canonical, check, digest, decode_json, CORE_REVISION, DATABASE_REVISION
from .catalog_io import Catalog, FRAME, MAX_RAW, MAX_ROWS, DECODED, encode_row, decode_row, key_for
from .catalog_schema import policy
from .catalog_spawns import uint, floating, _open, _regular
from .event_text import references
from .catalog_paths import Directory
from .global_hooks import CONTRACT

DB_TABLES = tuple(sorted(t for t in policy()["tables"] if t.startswith("dbscripts_on_")))
TABLES = tuple(sorted((*DB_TABLES, "dbscript_random_templates", "dbscript_string", "string_id",
    "creature_ai_scripts", "creature_ai_summons", "creature_ai_texts", "script_texts", "custom_texts",
    "gossip_texts", "script_waypoint", "scripted_areatrigger", "scripted_event_id", "spell_scripts",
    "broadcast_text", "broadcast_text_locale", "world_template", "instance_template")))
LOADERS = tuple("dbscript:"+t for t in DB_TABLES) + (
    "DBScripts/ScriptMgr:1164", "DBScripts/ScriptMgr:1230", "AI/EventAI/CreatureEventAIMgr:211",
    "AI/EventAI/CreatureEventAIMgr:41", "AI/ScriptDevAI/ScriptDevAIMgr:480",
    "AI/ScriptDevAI/ScriptDevAIMgr:550", "AI/ScriptDevAI/ScriptDevAIMgr:593",
    "AI/ScriptDevAI/system/system:68", "AI/ScriptDevAI/system/system:75", "spell-scripts",
    "Globals/ObjectMgr:7786", "Globals/ObjectMgr:7834")
SLOTS = ("pGossipHello", "pGossipHelloGO", "pGossipSelect", "pGossipSelectGO", "pGossipSelectWithCode",
    "pGossipSelectGOWithCode", "pDialogStatusNPC", "pDialogStatusGO", "pQuestAcceptNPC", "pQuestAcceptGO",
    "pQuestAcceptItem", "pQuestRewardedNPC", "pQuestRewardedGO", "pGOUse", "pItemUse", "pItemLoot",
    "pAreaTrigger", "pProcessEventId", "pTrapSearching", "GetGameObjectAI", "GetAI", "GetInstanceData")
REGISTRIES = ("scriptdev", "creature_ai", "motion", "spell", "aura")
KINDS = ("script_registration", "script_registration_site", "script_callback_binding", "script_binding", "script_registry_store")
FORMAT = "benilla-script-occurrences-v1"
REFUSALS = ["scripts.reference_execution_unsealed", "scripts.sql_occurrences_not_F1",
    "scripts.equal_priority_F1_order_missing", "scripts.creature_ai_registry_missing", "scripts.motion_registry_missing",
    "scripts.spell_registry_missing", "scripts.aura_registry_missing", "scripts.callback_dispatch_coverage_incomplete",
    "scripts.rejected_attempt_outcome_unobserved",
    "scripts.linked_source_proof_incomplete", "scripts.loader_validation_execution_pending", "scripts.gameplay_not_admitted"]


def specs():
    return {str(i): policy()["loaders"][loader] for i, loader in enumerate(LOADERS)}


def signed(raw):
    n = uint(raw)
    return n-(1 << 32) if n >= 1 << 31 else n


def db_summary(rows):
    commands, priorities = Counter(), Counter()
    for _, row in rows:
        commands[str(uint(row["command"]))] += 1
        priorities[uint(row["priority"])] += 1
        check(len(priorities)*128 <= DECODED, "scripts.view_limit")
    return {"commands": dict(commands), "equal_priority_groups": sum(n>1 for n in priorities.values()),
            "equal_priority_rows": sum(n for n in priorities.values() if n>1)}


def event_summary(rows):
    result = dict.fromkeys(("entry_selectors", "guid_selectors", "zero_selectors", "extreme_selectors",
                           "positive_text_parameters", "negative_text_parameters", "other_action_slots"), 0)
    for _, row in rows:
        selector = signed(row["creature_id"])
        result["entry_selectors" if selector>0 else "guid_selectors" if selector<0 else "zero_selectors"] += 1
        result["extreme_selectors"] += selector == -(1 << 31)
        converted = {k: signed(v) if "_param" in k else uint(v) for k,v in row.items() if k != "comment"}
        positive, negative, other = references({"creature_ai_scripts":[converted]})
        result["positive_text_parameters"] += len(positive)
        result["negative_text_parameters"] += len(negative)
        result["other_action_slots"] += len(other)
    return result


def ordered_groups(expected, seen):
    return sorted(expected) == sorted(seen) and all(a[0] <= b[0] for a,b in zip(seen,seen[1:]))


def capture_occurrences(source, root):
    root = Path(root)
    root.mkdir()
    streams = {}
    for name,spec in specs().items():
        h, count, size = hashlib.sha256(), 0, 0
        with (root/(name+".rows")).open("xb") as handle:
            for cells in source.query_rows(spec["query"]):
                check(count < MAX_ROWS, "scripts.row_limit")
                frame = encode_row(count,cells)
                check(size+len(frame) <= MAX_RAW, "scripts.byte_limit")
                handle.write(frame)
                h.update(frame)
                count += 1
                size += len(frame)
            handle.flush()
            os.fsync(handle.fileno())
        streams[name] = {"query":spec["query"], "sha256":h.hexdigest(), "rows":count, "bytes":size}
    result = {"format":FORMAT, "streams":streams, "complete":False, "source_snapshot":"phase0-sequential-diagnostic-not-F1"}
    with (root/"occurrences.json").open("xb") as handle:
        handle.write(canonical(result))
        handle.flush()
        os.fsync(handle.fileno())
    return result


class ScriptViews:
    def __init__(self, catalog):
        self.catalog = catalog
        self.schema = catalog._json(catalog.object(catalog.manifest["schema_root"]))["tables"]

    def rows(self, table):
        columns = [c["COLUMN_NAME"].lower() for c in self.schema[table]["columns"]]
        for row in self.catalog.scan(table):
            yield row.ordinal, dict(zip(columns,row.cells()))

    def expected(self, spec):
        query, table = spec["query"], spec["relations"][0]
        if "UNION" in query:
            names = set()
            for t in spec["relations"]:
                for _, row in self.rows(t):
                    name = row["scriptname"]
                    if name:
                        check(name.isascii(), "scripts.name_collation")
                        names.add(name)
                        check(len(names)*256 <= DECODED, "scripts.view_limit")
            check(len({n.lower().rstrip(b' ') for n in names}) == len(names), "scripts.name_collation")
            for name in sorted(names):
                yield (), [name]
        elif "COUNT(Entry)" in query:
            counts = Counter(row["entry"] for _,row in self.rows(table))
            check(len(counts)*256 <= DECODED, "scripts.view_limit")
            for count in counts.values():
                yield (), [str(count).encode()]
        else:
            projection = [n.strip().lower() for n in query[7:].split(" FROM ")[0].split(',')]
            ordering = [n.strip().lower() for n in query.split(" ORDER BY ")[1].split(',')] if " ORDER BY " in query else []
            for _,row in self.rows(table):
                yield tuple(uint(row[c]) for c in ordering), [row[c] for c in projection]

    def verify_occurrences(self, root):
        root = Directory(root)
        meta = self.catalog._json(_regular(root/"occurrences.json",1048576))
        check(type(meta) is dict and set(meta)=={"format","streams","complete","source_snapshot"}
              and meta["format"]==FORMAT and meta["complete"] is False
              and meta["source_snapshot"]=="phase0-sequential-diagnostic-not-F1"
              and type(meta["streams"]) is dict and set(meta["streams"])==set(specs()), "scripts.occurrences")
        reports = {}
        for name,spec in specs().items():
            expected, keys = [], defaultdict(list)
            ordered = " ORDER BY " in spec["query"]
            columns = 1 if "UNION" in spec["query"] or "COUNT(" in spec["query"] else len(spec["query"][7:].split(" FROM ")[0].split(','))
            for ordering,cells in self.expected(spec):
                check((len(expected)+1)*(768 if ordered else 320)+4194304 <= DECODED, "scripts.view_limit")
                h = hashlib.sha256(key_for(cells,range(columns))).digest()
                expected.append((ordering,h))
                if ordered:
                    keys[h].append(ordering)
            for values in keys.values():
                values.sort(reverse=True)
            d = meta["streams"][name]
            check(type(d) is dict and set(d)=={"query","rows","bytes","sha256"} and d["query"]==spec["query"], "scripts.query")
            check(type(d["rows"]) is int and 0<=d["rows"]<=MAX_ROWS and type(d["bytes"]) is int and 0<=d["bytes"]<=MAX_RAW, "scripts.hash")
            seen, raw = [], hashlib.sha256()
            with _open(root/(name+".rows")) as handle:
                before = os.fstat(handle.fileno())
                for ordinal in range(len(expected)):
                    prefix = handle.read(4)
                    check(len(prefix)==4, "scripts.exhausted")
                    size, = struct.unpack('<I',prefix)
                    check(16<=size<=FRAME, "catalog.frame_limit")
                    frame = prefix+handle.read(size-4)
                    check(len(frame)==size, "catalog.truncated")
                    cells = decode_row(frame,ordinal,columns)
                    raw.update(frame)
                    h = hashlib.sha256(key_for(cells,range(columns))).digest()
                    if ordered:
                        check(bool(keys[h]), "scripts.view_mismatch")
                    seen.append((keys[h].pop() if ordered else (),h))
                check(not handle.read(1), "scripts.trailing")
                check(Catalog._identity(before)==Catalog._identity(os.fstat(handle.fileno())), "catalog.changed")
            check(d["rows"]==len(seen) and d["bytes"]==before.st_size and d["sha256"]==raw.hexdigest(), "scripts.hash")
            check(ordered_groups(expected,seen), "scripts.order")
            reports[name] = {"query":spec["query"], "source":spec["source"], "rows":len(seen),
                "occurrence_sha256":raw.hexdigest(), "multiset_sha256":hashlib.sha256(b''.join(sorted(h for _,h in seen))).hexdigest()}
        return reports

    def report(self, occurrences, registration):
        families = {}
        for table in TABLES:
            raw, bits, count = hashlib.sha256(), hashlib.sha256(), 0
            floats = [(i,c["COLUMN_NAME"].lower()) for i,c in enumerate(self.schema[table]["columns"]) if c["DATA_TYPE"] in ("float","double","decimal")]
            for ordinal,row in self.rows(table):
                raw.update(encode_row(ordinal,list(row.values())))
                for index,name in floats:
                    bits.update(struct.pack('<QII',ordinal,index,struct.unpack('<I',struct.pack('<f',floating(row[name])))[0]))
                count += 1
            families[table] = {"rows":count, "raw_sha256":raw.hexdigest(), "float_bits_sha256":bits.hexdigest()}
        names = [cells[0].decode('ascii') for _,cells in self.expected(specs()[str(len(DB_TABLES)+4)])]
        name_ids = {name:i+1 for i,name in enumerate(sorted(names))}
        check(all(r["id"] is None or r["id"]==name_ids.get(r["name"],0) for r in registration["registry"]["records"]),
              "scripts.registration_names_drift")
        registration["sql_name_count"] = len(names)
        registration["unbound_absent_catalog_names"] = sum(r["id"] is None and r["name"] not in name_ids
                                                         for r in registration["registry"]["records"])
        result = {"format":"benilla-script-views-v1", "catalog_sha256":self.catalog.manifest["catalog_sha256"],
            "families":families, "loaders":occurrences, "db_scripts":{t:db_summary(self.rows(t)) for t in DB_TABLES},
            "event_ai":event_summary(self.rows("creature_ai_scripts")), "registrations":registration,
            "legacy_global_hooks_contract_sha256":digest(CONTRACT), "complete":False, "refusals":REFUSALS}
        result["view_sha256"] = digest(result)
        check(len(canonical(result)) <= 1048576, "scripts.report_limit")
        return result


def reference_manifest(path):
    raw = _regular(Path(path),2097152)
    value = decode_json(raw,2097152)
    check(value.get("core_revision")==CORE_REVISION and value.get("classic_db_revision")==DATABASE_REVISION,
          "scripts.linked_source")
    check(type(value.get("files")) is list and value.get("reference_execution_sealed") is False, "scripts.reference_manifest")
    return raw,value


def file_hash(path, limit):
    with _open(Path(path)) as handle:
        before = os.fstat(handle.fileno())
        check(before.st_size <= limit, "scripts.input_limit")
        h = hashlib.sha256()
        while data := handle.read(65536):
            h.update(data)
        check(Catalog._identity(before)==Catalog._identity(os.fstat(handle.fileno())), "catalog.changed")
    return {"bytes":before.st_size, "sha256":h.hexdigest()}


def registry_outcome(kind, duplicate, replace):
    """ScriptDev replaces; ObjectRegistry.h:45-57 requires explicit replace."""
    check(kind in REGISTRIES, "scripts.registry_kind")
    check(not replace or kind not in ("spell","aura"), "scripts.registry_replace_not_supported")
    if duplicate and kind != "scriptdev" and not replace:
        return "rejected_duplicate"
    return "accepted_override" if duplicate else "accepted"


class Registry:
    def __init__(self):
        self.current = None
        self.bindings = {}
        self.counts = Counter()
        self.slots = Counter()
        self.raw = hashlib.sha256()
        self.records = []
        self.charge = 0

    def close(self):
        if self.current is not None:
            c = self.current
            check(set(c["slots"])==set(SLOTS), "scripts.registry_slots")
            check(c["stored"] == c["bound"], "scripts.registry_store")
            self.counts["unresolved_attempts"] += not c["bound"]
            record = {k:c[k] for k in ("name","id","operation","source","slots","report_error")}
            record["outcome"] = c["outcome"] if c["stored"] else "unobserved_binding_outcome"
            self.charge += 4096+4*len(canonical(record))
            check(self.charge <= DECODED, "scripts.view_limit")
            self.records.append(record)
            self.current = None

    def feed(self, raw):
        fields = raw.rstrip(b'\n').split(b'\t',3)
        check(len(fields)==4 and raw.endswith(b'\n'), "scripts.registry_frame")
        kind = fields[2].decode('ascii')
        if kind not in KINDS:
            return
        self.raw.update(raw)
        words = fields[3].decode('utf-8').split(' ')
        if kind=="script_registration":
            self.close()
            check(len(words)==2 and words[1] in ('0','1'), "scripts.registry_frame")
            self.current = {"name":words[0], "thread":fields[0], "slots":{}, "site":False, "bound":False,
                            "stored":False, "id":None, "report_error":words[1]=='1'}
            self.counts["attempts"] += 1
        else:
            c = self.current
            check(c is not None and c["thread"]==fields[0], "scripts.registry_frame")
            if kind=="script_registration_site":
                check(not c["site"] and len(words)>=4 and words[1].startswith('/reference/source/src/'), "scripts.registry_site")
                c["site"] = True
                c["operation"] = words[0]
                c["source"] = {"file":words[1], "line":int(words[2]), "function":' '.join(words[3:])}
            elif kind=="script_callback_binding":
                check(c["site"] and len(words)==3 and words[0]==c["operation"] and words[1] in SLOTS and words[1] not in c["slots"], "scripts.registry_slots")
                c["slots"][words[1]] = format(int(words[2],16),'x')
                self.slots[words[1]] += 1
                self.counts["callback_slots"] += 1
                self.counts["nonnull_callbacks"] += int(words[2],16)!=0
            elif kind=="script_binding":
                check(len(words)==2 and not c["bound"] and words[1]==c["name"], "scripts.registry_store")
                c["bound"] = True
                c["id"] = int(words[0])
                check(0<c["id"]<=0xffffffff, "scripts.registry_store")
            elif kind=="script_registry_store":
                check(len(words)==4 and c["bound"] and not c["stored"] and int(words[0])==c["id"] and c["id"]!=0, "scripts.registry_store")
                old = 0 if words[1]=='(nil)' else int(words[1],16)
                check(old==self.bindings.get(c["id"],0), "scripts.registry_store")
                check(int(words[3])==self.counts["accepted"], "scripts.registry_store")
                check(int(words[2],16)!=0, "scripts.registry_store")
                self.bindings[c["id"]] = int(words[2],16)
                c["stored"] = True
                c["outcome"] = registry_outcome("scriptdev", bool(old), True)
                self.counts["accepted_overrides"] += old!=0
                self.counts["accepted"] += 1

    def finish(self):
        self.close()
        check(self.counts["attempts"]>0, "scripts.registry_slots")
        return {k:self.counts[k] for k in ("attempts","accepted","accepted_overrides","unresolved_attempts","callback_slots","nonnull_callbacks")} | {
            "final_bindings":len(self.bindings), "slots":dict(self.slots), "raw_registry_sha256":self.raw.hexdigest(),
            "observed_registries":["scriptdev"], "missing_registries":list(REGISTRIES[1:]), "records":self.records}


def registration_input(path):
    value = decode_json(_regular(Path(path),1048576),1048576)
    check(type(value) is dict and set(value)=={"format","manifest","trace","build","definitions","linked"}
          and value["format"]=="benilla-registration-input-v1", "scripts.registration_input")
    check(type(value["manifest"]) is dict and set(value["manifest"])=={"path","bytes","sha256"},
          "scripts.registration_input")
    raw,manifest = reference_manifest(value["manifest"]["path"])
    check(len(raw)==value["manifest"]["bytes"] and hashlib.sha256(raw).hexdigest()==value["manifest"]["sha256"],
          "scripts.input_hash")
    for name in ("trace","build","definitions"):
        d = value[name]
        check(type(d) is dict and set(d)=={"path","relative","bytes","sha256"}, "scripts.registration_input")
        found = [f for f in manifest["files"] if f["path"]==d["relative"]]
        check(len(found)==1 and all(found[0][k]==d[k] for k in ("bytes","sha256")), "scripts.input_hash")
    check(file_hash(value["definitions"]["path"],2097152)=={k:value["definitions"][k] for k in ("bytes","sha256")},
          "scripts.linked_source")
    build_raw = _regular(Path(value["build"]["path"]),1048576)
    check(len(build_raw)==value["build"]["bytes"] and hashlib.sha256(build_raw).hexdigest()==value["build"]["sha256"],
          "scripts.linked_source")
    build = decode_json(build_raw,1048576)
    check(type(value["linked"]) is dict and set(value["linked"])=={"executable","observer"},
          "scripts.registration_input")
    linked = []
    for d in build["files"]:
        if d["path"].endswith(('/mangosd','/transport-observer.so')):
            local = value["linked"]["executable" if d["path"].endswith("/mangosd") else "observer"]
            check(type(local) is dict and set(local)=={"path","bytes","sha256"}, "scripts.registration_input")
            check(all(local[k]==d[k] for k in ("bytes","sha256"))
                  and file_hash(local["path"],67108864)=={k:d[k] for k in ("bytes","sha256")}, "scripts.linked_source")
            linked.append({k:d[k] for k in ("bytes","sha256")})
    check(len(linked)==2, "scripts.linked_source")
    registry = Registry()
    h,size,lines = hashlib.sha256(),0,0
    with _open(Path(value["trace"]["path"])) as handle:
        before = os.fstat(handle.fileno())
        check(before.st_size==value["trace"]["bytes"] and before.st_size<=8589934592, "scripts.input_limit")
        while line := handle.readline(FRAME+1):
            check(len(line)<=FRAME and line.endswith(b'\n'), "scripts.registry_frame")
            h.update(line)
            size += len(line)
            lines += 1
            if b'\tscript_' in line:
                registry.feed(line)
        check(Catalog._identity(before)==Catalog._identity(os.fstat(handle.fileno())), "catalog.changed")
    check(size==value["trace"]["bytes"] and h.hexdigest()==value["trace"]["sha256"], "scripts.input_hash")
    return {"input":{k:{f:d[f] for f in ("bytes","sha256")} for k,d in value.items() if k not in ("format","linked")},
            "linked_binaries":linked, "trace_records":lines, "registry":registry.finish(),
            "definitions_status":"bound_F1_receipt_not_native_callback_execution"}
