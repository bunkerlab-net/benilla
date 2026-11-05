"""Complete catalog preparation and pending execution ledger, never admission.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from __future__ import annotations
import gc
import hashlib
import os
from pathlib import Path
import struct
import time

from .core import canonical, check, decode_json, digest
from .catalog_io import Catalog, CHUNK, FRAME, encode_row, decode_row, rss_kib
from .catalog_spawns import SpawnViews, _open, _regular
from .catalog_groups import GroupViews
from .catalog_pools import PoolViews
from .catalog_events import EventViews, initial_state
from .catalog_dbc import DbcViews
from .catalog_scripts import ScriptViews, registration_input
from .catalog_spells import SpellViews
from .catalog_content import ContentViews
from .catalog_schema import policy as coverage_policy

POLICY = decode_json(Path(__file__).with_name("catalog_preparation_policy.json").read_bytes(), CHUNK)
FAMILIES = tuple(sorted(POLICY["families"]))
OCCURRENCES = ("dbc","event","group","pool","script","spawn")
DOMAIN = b"benilla-native-catalog-preparation-v2\0"


def _write(path, value):
    raw = canonical(value)
    check(len(raw) <= CHUNK,"preparation.metadata_limit")
    with Path(path).open("xb") as handle:
        handle.write(raw)
        handle.flush()
        os.fsync(handle.fileno())


def _fields(value, names, diagnostic):
    check(type(value) is dict and set(value) == set(names),diagnostic)


def checked_bytes(value):
    _fields(value,("path","bytes","sha256"),"preparation.input_descriptor")
    check(type(value["path"]) is str and type(value["bytes"]) is int
          and 0 <= value["bytes"] <= CHUNK,"preparation.input_descriptor")
    raw = _regular(Path(value["path"]),CHUNK)
    check(len(raw) == value["bytes"] and hashlib.sha256(raw).hexdigest() == value["sha256"],
          "preparation.input_hash")
    return raw


def bind_descriptor(value):
    checked_bytes(value)
    return {k:value[k] for k in ("bytes","sha256")}


def validate_input(value, catalog_sha256, snapshot):
    _fields(value,("format","catalog_sha256","source_snapshot","reference_execution",
        "occurrences","registration","initial_snapshot","environment"),"preparation.inputs")
    check(value["format"] == "benilla-catalog-preparation-input-v1","preparation.version")
    check(value["catalog_sha256"] == catalog_sha256 and value["source_snapshot"] == snapshot,
          "preparation.snapshot")
    check(value["reference_execution"] == POLICY["pending_reference"],"preparation.execution_pending")
    _fields(value["occurrences"],OCCURRENCES,"preparation.occurrences")


def check_occurrences(name, report, metadata):
    observed = (report["joined"] if name == "spawn" else report["streams"] if name == "pool"
                else report["transports"]["occurrences"] if name == "dbc" else report["loaders"])
    check(set(observed) == set(metadata["streams"]),"preparation.occurrence_binding")
    for key, stream in metadata["streams"].items():
        check(observed[key]["rows"] == stream["rows"]
              and observed[key]["occurrence_sha256"] == stream["sha256"],"preparation.occurrence_binding")


def index_stream(path, kind, columns=0):
    """Index complete source frames or canonical records without an eager file read."""
    pages, whole, page = [], hashlib.sha256(), hashlib.sha256()
    offset, size, rows, total, count = 0, 0, 0, 0, 0
    def flush():
        nonlocal offset,size,rows,page
        if size:
            pages.append({"offset":offset,"bytes":size,"rows":rows,"sha256":page.hexdigest()})
            offset += size
            size, rows, page = 0,0,hashlib.sha256()
    with _open(Path(path)) as handle:
        before = os.fstat(handle.fileno())
        check(before.st_size <= POLICY["limits"]["stream_bytes"],"preparation.stream_limit")
        while True:
            if kind == "frames":
                head = handle.read(4)
                if not head:
                    break
                check(len(head) == 4,"preparation.exhausted")
                length = struct.unpack("<I",head)[0]
                check(16 <= length <= FRAME,"preparation.record_limit")
                record = head+handle.read(length-4)
                check(len(record) == length,"preparation.exhausted")
                decode_row(record,count,columns)
            elif kind == "jsonl":
                record = handle.readline(FRAME+1)
                if not record:
                    break
                check(len(record) <= FRAME and record.endswith(b"\n"),"preparation.record_limit")
                check(canonical(decode_json(record[:-1],FRAME)) == record[:-1],"preparation.record_json")
            else:
                record = handle.read(CHUNK+1)
                if not record:
                    break
                check(count == 0 and len(record) <= CHUNK,"preparation.record_limit")
                check(canonical(decode_json(record,CHUNK)) == record,"preparation.record_json")
            if rows == 4096 or size+len(record) > CHUNK:
                flush()
            whole.update(record)
            page.update(record)
            size += len(record)
            total += len(record)
            rows += 1
            count += 1
        check(Catalog._identity(before) == Catalog._identity(os.fstat(handle.fileno())),"catalog.changed")
    flush()
    return {"kind":kind,"bytes":total,"rows":count,"sha256":whole.hexdigest(),"pages":pages}


def check_family(name, report, source):
    check(name in POLICY["families"],"preparation.unknown_family")
    spec = POLICY["families"][name]
    check(report.get("complete") is False,"preparation.complete")
    check(report.get("format") == spec["format"],"preparation.family_format")
    check(report.get("catalog_sha256") == source["catalog_sha256"],"preparation.snapshot")
    raw = report.get("raw_sha256") if name in ("spawn","pool") else None
    tables = report["transports"]["raw"] if name == "dbc" else report.get("families",raw)
    check(type(tables) is dict and set(tables) == set(spec["tables"]),"preparation.family_tables")
    check(set(report.get("refusals",report.get("blockers",[]))) == set(spec["refusals"]),
          "preparation.obligations")
    for table in spec["tables"]:
        row = tables[table]
        raw_hash = raw[table] if raw is not None else row["raw_sha256"]
        count = report["counts"][table] if raw is not None else row.get("rows",row.get("stream",{}).get("rows"))
        check(raw_hash == source["tables"][table]["raw_sha256"] and count == source["tables"][table]["rows"],
              "preparation.cross_family")
    own = dict(report)
    provided = own.pop("view_sha256",None)
    check(digest(own) == provided,"preparation.family_hash")


def obligations(reports, coverage):
    result = {kind:{} for kind in POLICY["obligation_kinds"]}
    for requirement in POLICY["pending_reference"]["required"]:
        result["F1_binding"]["binding."+requirement] = ["reference_execution"]
    for family,report in sorted(reports.items()):
        for diagnostic in report.get("refusals",report.get("blockers",[])):
            kind = POLICY["families"][family]["refusals"][diagnostic]
            result[kind].setdefault(diagnostic,[]).append(family)
    for loader in sorted(coverage["loaders"]):
        result["loader_execution"]["loader."+loader] = ["source_loader"]
    covered = {table for spec in POLICY["families"].values() for table in spec["tables"]}
    for table,spec in sorted(coverage["tables"].items()):
        if spec["status"] != "archive-only" and table not in covered:
            result["loader_execution"]["view."+table] = ["raw_catalog_only"]
    result["configuration"]["configuration.effective_world_inputs_unbound"] = ["reference_execution"]
    return {kind:[{"id":key,"status":"pending","owners":sorted(owners)}
                  for key,owners in sorted(values.items())] for kind,values in result.items()}


def compare_output(expected, actual, manifest):
    """Rehashed supplied metadata never replaces freshly recomputed source outputs."""
    expected,actual = Path(expected),Path(actual)
    raw = _regular(expected/"preparation.json",CHUNK)
    supplied = decode_json(raw,CHUNK)
    check(canonical(supplied) == raw,"catalog.json")
    check(supplied.get("complete") is False,"preparation.complete")
    check(supplied == manifest,"preparation.manifest_mismatch")
    expected_files = {"preparation.json","identity.json"}
    for relative,stream in manifest["streams"].items():
        expected_files.add(relative)
        with _open(expected/relative) as left, _open(actual/relative) as right:
            a,b = os.fstat(left.fileno()),os.fstat(right.fileno())
            check(a.st_size == b.st_size == stream["bytes"],"preparation.record_mismatch")
            while True:
                first,second = left.read(FRAME),right.read(FRAME)
                check(first == second,"preparation.record_mismatch")
                if not first:
                    break
            check(Catalog._identity(a) == Catalog._identity(os.fstat(left.fileno()))
                  and Catalog._identity(b) == Catalog._identity(os.fstat(right.fileno())),"catalog.changed")
    for name in FAMILIES:
        relative = f"families/{name}.json"
        expected_files.add(relative)
        check(_regular(expected/relative,CHUNK) == _regular(actual/relative,CHUNK),"preparation.family_mismatch")
    check(_regular(expected/"identity.json",CHUNK) == _regular(actual/"identity.json",CHUNK),"preparation.identity_mismatch")
    allowed = expected_files | {"resources.json"}
    allowed |= {str(Path(f).parent) for f in expected_files if str(Path(f).parent) != "."}
    for path in expected.rglob("*"):
        check(path.relative_to(expected).as_posix() in allowed,"preparation.surplus")


def prepare(root, inputs, output, expected=None):
    started = time.perf_counter()
    output = Path(output)
    output.mkdir()
    (output/"families").mkdir()
    (output/"source").mkdir()
    request = decode_json(_regular(Path(inputs),CHUNK),CHUNK)
    with Catalog(root) as catalog:
        validate_input(request,catalog.manifest["catalog_sha256"],catalog.manifest["source_snapshot"])
        bindings = {name:bind_descriptor(request[name]) for name in ("registration","initial_snapshot","environment")}
        occurrence_meta = {name:decode_json(checked_bytes(request["occurrences"][name]),CHUNK) for name in OCCURRENCES}
        bindings["occurrences"] = {name:{k:request["occurrences"][name][k] for k in ("bytes","sha256")} for name in OCCURRENCES}
        occurrences = {name:Path(request["occurrences"][name]["path"]).parent for name in OCCURRENCES}
        coverage = coverage_policy()
        check(set(catalog.relations) == set(coverage["tables"]),"preparation.source_tables")
        source = {"catalog_sha256":catalog.manifest["catalog_sha256"],"tables":{}}
        streams, reports = {}, {}
        for table,relation in sorted(catalog.relations.items()):
            relative = "source/"+table+".frames"
            with (output/relative).open("xb") as handle:
                for view in catalog.scan(table):
                    handle.write(encode_row(view.ordinal,view.cells()))
            index = index_stream(output/relative,"frames",relation["columns"])
            check(index["rows"] == relation["rows"],"preparation.source_rows")
            streams[relative] = index
            source["tables"][table] = {"rows":relation["rows"],"raw_sha256":index["sha256"],
                "ordinal_index":relation["ordinal"],"key_index":relation["key"],
                "classification":coverage["tables"][table]["status"],
                "field_disposition":"retained_raw_archive" if coverage["tables"][table]["status"] == "archive-only" else "retained_raw_loader_input",
                "columns":relation["columns"],"key_columns":relation["key_columns"],
                "views":sorted(name for name,spec in POLICY["families"].items() if table in spec["tables"])}
        spawns = SpawnViews(catalog)
        spawns.verify_indexes()
        reports["spawn"] = spawns.report(spawns.verify_occurrences(occurrences["spawn"]))
        views = GroupViews(catalog)
        reports["group"] = views.report(views.verify_occurrences(occurrences["group"]))
        reports["pool"] = PoolViews(catalog,spawns).verify_occurrences(occurrences["pool"],output/"pool")
        del spawns,views
        gc.collect()
        views = EventViews(catalog)
        state = initial_state(request["initial_snapshot"]["path"],request["environment"]["path"])
        check(state["snapshot"] == bindings["initial_snapshot"]
              and state["calendar"]["evidence"] == bindings["environment"],"preparation.snapshot")
        reports["event"] = views.report(views.verify_occurrences(occurrences["event"]),state,output/"event")
        reports["dbc"] = DbcViews(catalog).report(occurrences["dbc"],output/"dbc")
        registration = registration_input(Path(request["registration"]["path"]))
        declared = decode_json(checked_bytes(request["registration"]),CHUNK)
        check(registration["input"] == {n:{k:declared[n][k] for k in ("bytes","sha256")}
            for n in ("manifest","trace","build","definitions")},"preparation.build")
        views = ScriptViews(catalog)
        reports["script"] = views.report(views.verify_occurrences(occurrences["script"]),registration)
        del views,registration
        gc.collect()
        reports["spell"] = SpellViews(catalog).report(output/"spell")
        for family in ("quests","items","loot","globals"):
            reports[family] = ContentViews(catalog,family).report(output/family)
            gc.collect()
        check(set(reports) == set(FAMILIES),"preparation.families")
        for family,report in sorted(reports.items()):
            check_family(family,report,source)
            if family in OCCURRENCES:
                check_occurrences(family,report,occurrence_meta[family])
            _write(output/"families"/(family+".json"),report)
            folder = output/family
            if folder.is_dir():
                for path in sorted(folder.iterdir()):
                    relative = path.relative_to(output).as_posix()
                    streams[relative] = index_stream(path,"jsonl" if path.suffix == ".jsonl" else "json")
        pending = obligations(reports,coverage)
        family_ledger = {name:{"status":"input_views_verified_execution_pending","view_sha256":r["view_sha256"],
            "rows":sum(source["tables"][t]["rows"] for t in POLICY["families"][name]["tables"]),
            "report":{"bytes":len(canonical(r)),"sha256":digest(r)},
            "refusals":r.get("refusals",r.get("blockers",[]))} for name,r in sorted(reports.items())}
        manifest = {"format":"benilla-native-catalog-preparation-v2","catalog_sha256":catalog.manifest["catalog_sha256"],
            "core_revision":catalog.manifest["core_revision"],"database_revision":catalog.manifest["database_revision"],
            "source_snapshot":catalog.manifest["source_snapshot"],"reference_execution":request["reference_execution"],
            "input_bindings":bindings,"policy_sha256":digest(POLICY),"source":source["tables"],
            "families":family_ledger,"streams":streams,"obligations":pending,
            "obligation_counts":{k:len(v) for k,v in pending.items()},"complete":False}
        manifest["preparation_sha256"] = hashlib.sha256(DOMAIN+canonical(manifest)).hexdigest()
        identity = {"format":"benilla-production-input-identity-v2","catalog_sha256":catalog.manifest["catalog_sha256"],
            "preparation_sha256":manifest["preparation_sha256"],"reference_execution":request["reference_execution"],
            "input_bindings":bindings,"runtime_admitted":False}
        identity["identity_sha256"] = digest(identity)
        _write(output/"preparation.json",manifest)
        _write(output/"identity.json",identity)
        if expected:
            compare_output(expected,output,manifest)
        resources = catalog.report()
        resources.update({"phase":"whole_aggregation","output_bytes":sum(v["bytes"] for v in streams.values()),
            "output_pages":sum(len(v["pages"]) for v in streams.values()),"output_streams":len(streams),
            "output_buffer_limit":CHUNK+FRAME,"rss_kib":rss_kib(),"elapsed_seconds":time.perf_counter()-started})
        _write(output/"resources.json",resources)
        return {"preparation":manifest,"identity":identity}
