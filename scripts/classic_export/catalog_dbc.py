"""Pinned server DBC input views, independent of gameplay and SQL spell identity.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from __future__ import annotations
import hashlib
from pathlib import Path
import struct
import weakref

from .core import canonical, check, decode_json, digest
from .catalog_io import FRAME, DECODED, encode_row
from .catalog_spawns import uint
from native_starter.dbc import LAYOUTS, MAX_RECORDS, MAX_STRING_BYTES

CONTRACT = decode_json(Path(__file__).with_name("catalog_dbc_layouts.json").read_bytes(), 65536)
LAYOUT_POLICY = digest(CONTRACT)


class DbcView:
    """One bounded chunk and a sparse last-occurrence index, never a max-ID vector."""
    def __init__(self, read, descriptor, layout=None):
        check(type(descriptor["bytes"]) is int and 0 <= descriptor["bytes"] <= 64*1024*1024, "catalog.overflow")
        self.read = read
        self.descriptor = descriptor
        self.closed = False
        self.layout = layout
        self.cached = None
        self.chunk_hashes = []
        h = hashlib.sha256()
        for at in range(0, descriptor["bytes"], FRAME):
            raw = read(at, min(FRAME, descriptor["bytes"]-at))
            check(len(raw) == min(FRAME, descriptor["bytes"]-at), "catalog.truncated")
            h.update(raw)
            self.chunk_hashes.append(hashlib.sha256(raw).digest())
        check(h.hexdigest() == descriptor["sha256"], "catalog.hash")
        check(descriptor["bytes"] >= 20, "dbc.byte_limit_or_truncated_header")
        magic, self.count, self.fields, self.size, self.string_size = struct.unpack("<4s4I", self.range(0, 20))
        check(magic == b"WDBC", "dbc.magic")
        check(self.count <= MAX_RECORDS, "dbc.record_limit")
        check(0 < self.fields <= 4096 and 0 < self.size <= FRAME, "dbc.layout")
        self.end = 20 + self.count*self.size
        check(self.end+self.string_size == descriptor["bytes"], "dbc.exact_length")
        check(self.string_size > 0 and self.range(self.end, 1) == b"\0"
              and self.range(descriptor["bytes"]-1, 1) == b"\0", "dbc.string_block")
        self.by_id = {}
        self.duplicates = 0
        if layout is not None:
            check((self.fields, self.size) == (len(layout["format"]), layout["record_bytes"]), "dbc.layout")
            fmt = layout["format"]
            check(len(fmt) <= 173 and set(fmt) <= set("nidxsfbX") and sum(c in "nd" for c in fmt) <= 1, "dbc.layout")
            self.offsets = []
            at = 0
            for kind in fmt:
                self.offsets.append(at)
                at += 1 if kind in "bX" else 4
            for ordinal in range(self.count):
                key = self.record(ordinal)["key"]
                self.duplicates += key in self.by_id
                self.by_id[key] = ordinal
                check(len(self.by_id)*160 <= DECODED, "dbc.index_limit")

    def close(self):
        self.closed = True
        self.cached = None
        self.by_id.clear()
        self.chunk_hashes.clear()
        self.read = None

    def range(self, at, size):
        check(not self.closed, "dbc.view_closed")
        check(0 <= size <= FRAME and 0 <= at <= at+size <= self.descriptor["bytes"], "catalog.truncated")
        out = bytearray()
        while size:
            page = at//FRAME
            if self.cached is None or self.cached[0] != page:
                raw = self.read(page*FRAME, min(FRAME, self.descriptor["bytes"]-page*FRAME))
                check(hashlib.sha256(raw).digest() == self.chunk_hashes[page], "catalog.hash")
                self.cached = (page, raw)
            take = min(size, len(self.cached[1])-at%FRAME)
            out += self.cached[1][at%FRAME:at%FRAME+take]
            at += take
            size -= take
        return bytes(out)

    def string(self, offset):
        check(offset < self.string_size, "dbc.string_offset")
        raw = self.range(self.end+offset, min(MAX_STRING_BYTES+1, self.string_size-offset))
        end = raw.find(b"\0")
        check(end >= 0, "dbc.string_termination_or_limit")
        return raw[:end]

    def record(self, ordinal, overlays=()):
        check(not self.closed, "dbc.view_closed")
        check(self.layout is not None, "dbc.layout_required")
        check(0 <= ordinal < self.count, "dbc.ordinal")
        raw = self.range(20+ordinal*self.size, self.size)
        key, values = ordinal, []
        for col, kind in enumerate(self.layout["format"]):
            if kind in "xX":
                continue
            at = self.offsets[col]
            width = 1 if kind == "b" else 4
            check(at+width <= len(raw), "dbc.layout")
            word = int.from_bytes(raw[at:at+width], "little")
            if kind in "nd":
                key = word
            if kind != "d":
                values.append([col, kind, self.string(word).hex() if kind == "s" else word])
        result = {"ordinal": ordinal, "key": key, "values": values}
        for overlay in overlays:
            check(overlay.count == self.count, "dbc.locale_record_count")
            result = fill_locale([result], [overlay.record(ordinal)])[0]
        return result

    def lookup(self, key):
        check(not self.closed, "dbc.view_closed")
        check(self.layout is not None, "dbc.layout_required")
        ordinal = self.by_id.get(key)
        return None if ordinal is None else self.record(ordinal)

    def words(self, ordinal):
        raw = self.range(20+ordinal*self.size, self.size)
        return list(struct.unpack("<"+"I"*(self.size//4), raw))

    def report(self, output=None):
        check(not self.closed, "dbc.view_closed")
        raw, decoded, index = hashlib.sha256(), hashlib.sha256(), hashlib.sha256()
        for ordinal in range(self.count):
            raw.update(self.range(20+ordinal*self.size, self.size))
            if self.layout:
                record = canonical(self.record(ordinal))+b"\n"
                check(len(record) <= FRAME, "dbc.record_output_limit")
                decoded.update(record)
                if output is not None:
                    output.write(record)
        for key, ordinal in sorted(self.by_id.items()):
            index.update(struct.pack("<II", key, ordinal))
        return {"sha256": self.descriptor["sha256"], "bytes": self.descriptor["bytes"],
                "records": self.count, "fields": self.fields, "record_bytes": self.size,
                "physical_sha256": raw.hexdigest(),
                "decoded_sha256": decoded.hexdigest() if self.layout else None,
                "index_sha256": index.hexdigest() if self.layout else None,
                "unique_keys": len(self.by_id) if self.layout else None,
                "duplicate_keys": self.duplicates if self.layout else None,
                "lookup_extent": (max(self.by_id, default=0)+1 if any(c in "nd" for c in self.layout["format"])
                                  else self.count) if self.layout else None}


def fill_locale(base, overlay):
    """DBCFileLoader.cpp:272-298 fills empty string slots in physical row order."""
    check(len(base) == len(overlay), "dbc.locale_record_count")
    result = []
    for left, right in zip(base, overlay):
        check(left["ordinal"] == right["ordinal"] and len(left["values"]) == len(right["values"]), "dbc.locale_layout")
        values = []
        for a, b in zip(left["values"], right["values"]):
            check(a[:2] == b[:2], "dbc.locale_layout")
            values.append(b if a[1] == "s" and not a[2] else a)
        result.append({**left, "values": values})
    return result


def locale_selection(race, configured=0):
    """World.cpp:1462-1504 detects names; World.h:556-559 falls back globally."""
    check(race is not None, "dbc.locale_human_missing")
    names = {column: value for column, kind, value in race["values"] if kind == "s"}
    check(all(i in names for i in range(17, 25)), "dbc.locale_layout")
    mask = sum(1 << i for i in range(8) if names[17+i])
    check(mask != 0, "dbc.locale_undetermined")
    effective = 0 if configured != 255 and configured >= 8 else configured
    default = next(i for i in range(8) if mask & (1 << i))
    if effective < 8 and mask & (1 << effective):
        default = effective
    return {"configured_locale": configured, "effective_config_locale": effective,
            "available_mask": mask, "default_locale": default,
            "fallback_locales": [i if mask & (1 << i) else default for i in range(8)],
            "source": "World.cpp:1462-1504; World.h:556-559"}


class DbcViews:
    def __init__(self, catalog):
        self.catalog = catalog
        self.inventory = catalog._json(catalog.object(catalog.manifest["dbc_root"]))
        check(catalog.manifest["core_revision"] == CONTRACT["core_revision"], "dbc.source_pin")
        self.layouts = {l["file"]: l for l in CONTRACT["loaders"]}
        check(self.layouts.keys() <= self.inventory.keys(), "dbc.mandatory_file_missing")

    def open(self, name):
        check(name in self.layouts, "dbc.not_server_loader")
        return self._open(name, self.layouts[name])

    def _open(self, name, layout=None):
        check(name in self.inventory, "dbc.mandatory_file_missing")
        d = self.inventory[name]
        previous = getattr(self.catalog, "_dbc_active", lambda: None)()
        if previous is not None:
            previous.close()
        view = DbcView(lambda at, size: self.catalog.blob_range(d, at, size), d,
                       self.layouts.get(name) if layout is None else layout)
        self.catalog._dbc_active = weakref.ref(view)
        return view

    def rows(self, table):
        schema = self.catalog._json(self.catalog.object(self.catalog.manifest["schema_root"]))["tables"][table]
        columns = [c["COLUMN_NAME"].lower() for c in schema["columns"]]
        for view in self.catalog.scan(table):
            yield view.ordinal, dict(zip(columns, view.cells()))

    def report(self, occurrences, output=None):
        if output is not None:
            output = Path(output)
            output.mkdir()
        files = {}
        for name, descriptor in sorted(self.inventory.items()):
            if descriptor["bytes"] == 0:
                check(name in CONTRACT["zero_byte_archive"] and name not in self.layouts, "dbc.byte_limit_or_truncated_header")
                files[name] = {**descriptor, "classification": "zero_byte_archive_not_WDBC", "records": None}
                continue
            view = self._open(name)
            handle = (output / (name+".jsonl")).open("xb") if output is not None and name in self.layouts else None
            try:
                files[name] = {**view.report(handle), "classification": "server_loader" if name in self.layouts else "archive_only",
                               "layout": self.layouts.get(name), "source_treatment": CONTRACT["disabled"].get(name, "no_enabled_LoadDBC_call" if name not in self.layouts else "mandatory")}
            finally:
                if handle:
                    handle.close()
        transports = self.transports()
        from .catalog_transports import verify_occurrences
        transports["occurrences"] = verify_occurrences(self, occurrences)
        spell = self._open("Spell.dbc", {"format": "".join("n" if i == 0 else "s" if i in LAYOUTS["Spell"].strings else "i" for i in range(173)), "record_bytes": LAYOUTS["Spell"].size})
        spell_report = spell.report()
        sql_hash, sql_count = hashlib.sha256(), 0
        for ordinal, row in self.rows("spell_template"):
            sql_hash.update(encode_row(ordinal, list(row.values())))
            sql_count += 1
        result = {"format": "benilla-dbc-catalog-views-v1", "catalog_sha256": self.catalog.manifest["catalog_sha256"],
                  "layout_policy_sha256": LAYOUT_POLICY, "files": files, "transports": transports,
                  "spell_identity": {"client_dbc": spell_report, "server_sql_rows": sql_count,
                                     "server_sql_sha256": sql_hash.hexdigest(), "server_store": "spell_template_not_Spell.dbc"},
                  "locale_selection": locale_selection(self.open("ChrRaces.dbc").lookup(1)),
                  "locale_policy": "base_strings_then_first_nonempty_overlay_by_physical_ordinal",
                  "locale_overlays": "not_in_retained_catalog", "complete": False,
                  "blockers": ["dbc.reference_execution_missing", "dbc.locale_occurrences_missing", "dbc.gameplay_not_admitted"]}
        result["view_sha256"] = digest(result)
        self.catalog._checked_identities()
        return result

    def transports(self):
        from native_starter.dbc import parse_table
        from native_starter.transport_paths import coverage
        tables = {}
        for name in ("Map", "TaxiPath", "TaxiPathNode"):
            view = self.open(name+".dbc")
            check(view.descriptor["bytes"] <= FRAME*8, "dbc.transport_input_limit")
            data = b"".join(view.range(at, min(FRAME, view.descriptor["bytes"]-at)) for at in range(0, view.descriptor["bytes"], FRAME))
            tables[name] = parse_table(name, data)
        templates, raw = {}, {}
        for table in ("transports", "gameobject_template"):
            h, count = hashlib.sha256(), 0
            for ordinal, row in self.rows(table):
                h.update(encode_row(ordinal, list(row.values())))
                count += 1
                if table == "gameobject_template" and uint(row["type"]) == 15:
                    templates[uint(row["entry"])] = {k: uint(row[k]) for k in ("entry", "type", "data0", "data1", "data2", "displayid")}
            raw[table] = {"rows": count, "raw_sha256": h.hexdigest()}
        animation = self.open("TransportAnimation.dbc")
        animations = {}
        for key in sorted(animation.by_id):
            words = animation.words(animation.by_id[key])
            group = animations.setdefault(words[1], {})
            group[words[2]] = {"ordinal": animation.by_id[key], "words": words}
        animation_views = {str(entry): {"total_time": max(nodes), "nodes": [nodes[t] for t in sorted(nodes)]} for entry, nodes in sorted(animations.items())}
        display = self.open("GameObjectDisplayInfo.dbc")
        plans = {}
        for entry, template in sorted(templates.items()):
            plans[str(entry)] = {"template": template, "display_present": display.lookup(template["displayid"]) is not None,
                                 "paths": [coverage({"selection": {"map_id": map_id}}, template, tables) for map_id in (0, 1)]}
        rows = []
        for ordinal, row in self.rows("transports"):
            entry = uint(row["entry"])
            rows.append({"ordinal": ordinal, "entry": entry, "name_hex": (row["name"] or b"").hex(),
                         "period": uint(row["period"]), "template_present": entry in templates})
        return {"raw": raw, "rows": rows, "templates": plans, "animations": animation_views,
                "taxi_paths": len(tables["TaxiPath"].rows), "taxi_nodes": len(tables["TaxiPathNode"].rows),
                "animation_records": animation.count, "source": "TransportMgr.cpp:28-32,44-55,76-139,305-311; Transports.cpp:35-95; DBCStores.cpp:482-508"}
