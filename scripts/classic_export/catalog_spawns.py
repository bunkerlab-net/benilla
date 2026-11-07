"""Complete diagnostic spawn views over a verified catalog, never gameplay admission.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from __future__ import annotations

import bisect
import hashlib
import itertools
import math
import os
from pathlib import Path
import re
import struct
import sys

from .core import canonical, check, digest, ExportError
from .catalog_io import FRAME, MAX_ROWS, MAX_RAW, DECODED, MAX_INDEX, Catalog, StoreWriter, encode_row, decode_row, key_for
from .catalog_schema import policy
from .catalog_paths import Directory, open_regular

FAMILIES = (
    "creature", "gameobject", "creature_spawn_entry", "gameobject_spawn_entry",
    "creature_template", "gameobject_template", "creature_model_info",
    "creature_equip_template", "creature_template_classlevelstats",
    "creature_template_addon", "creature_addon", "gameobject_addon",
    "gameobject_template_addon", "creature_spawn_data", "creature_spawn_data_template",
    "creature_conditional_spawn", "creature_immunities", "creature_template_spells",
    "creature_spell_targeting", "creature_spell_list_entry", "creature_spell_list",
    "creature_cooldowns",
)
JOINS = {
    "creature": (("game_event_creature", "guid", "event"),
                 ("pool_creature", "guid", "pool_entry"),
                 ("pool_creature_template", "id", "pool_entry"),
                 ("creature_spawn_data", "guid", "id")),
    "gameobject": (("game_event_gameobject", "guid", "event"),
                   ("pool_gameobject", "guid", "pool_entry"),
                   ("pool_gameobject_template", "id", "pool_entry")),
}
PROJECTIONS = {
    "creature": ("guid", "id", "map", "position_x", "position_y", "position_z",
                 "orientation", "spawntimesecsmin", "spawntimesecsmax", "spawndist",
                 "MovementType", "spawnMask"),
    "gameobject": ("guid", "id", "map", "position_x", "position_y", "position_z",
                   "orientation", "rotation0", "rotation1", "rotation2", "rotation3",
                   "spawntimesecsmin", "spawntimesecsmax", "spawnMask"),
}
QUERY_IDS = {"creature": "Globals/ObjectMgr:1877", "gameobject": "Globals/ObjectMgr:2086"}
INDEX = struct.Struct(">IIIIQ")  # map, grid, cell, GUID, source ordinal
_INTEGER_PREFIX = re.compile(rb"[\t\n\v\f\r ]*([+-]?[0-9]+)")
_FLOAT_PREFIX = re.compile(rb"[\t\n\v\f\r ]*[+-]?(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)(?:[eE][+-]?[0-9]+)?")


def uint(value):
    # Field::GetUInt32 wraps atoll, src/shared/Database/Field.h:59.
    check(value is None or len(value) < 128, "spawn.numeric_limit")
    match = _INTEGER_PREFIX.match(b"" if value is None else bytes(value))
    number = int(match[1]) if match else 0
    return min(max(number, -(1 << 63)), (1 << 63)-1) & 0xffffffff


def floating(value):
    # Field::GetFloat rounds atof's double, src/shared/Database/Field.h:53.
    check(value is None or len(value) < 128, "spawn.numeric_limit")
    if value is not None:
        check(not bytes(value).lstrip().lstrip(b"+-").lower().startswith((b"0x", b"nan", b"inf")),
              "spawn.numeric_unsupported")
    match = _FLOAT_PREFIX.match(b"" if value is None else bytes(value))
    number = float(match[0]) if match else 0.0
    try:
        return struct.unpack("<f", struct.pack("<f", number))[0]
    except OverflowError:
        return math.copysign(math.inf, number)


def coordinates(x, y):
    # Compute uses double intermediates with float constants, Maps/GridDefines.h:147-166.
    size = floating(b"533.33333")
    cell_size = floating(str(size / 16).encode())
    cell = [int((v - cell_size / 2) / cell_size + 512 + .5) for v in (x, y)]
    grid = [int((v - size / 2) / size + 32 + .5) for v in (x, y)]
    check(all(0 <= c < 1024 for c in cell) and all(0 <= g < 64 for g in grid),
          "spawn.invalid_coordinates")
    return grid[1] * 64 + grid[0], cell[1] * 1024 + cell[0]


def query(name):
    return policy()["loaders"][QUERY_IDS[name]]["query"]


def capture_occurrences(source, root):
    """Retain actual diagnostic joined cursors, separately from base-table ordering."""
    root = Path(root)
    root.mkdir()
    streams = {}
    for name in JOINS:
        h, count, size = hashlib.sha256(), 0, 0
        with (root / (name + ".rows")).open("xb") as handle:
            for cells in source.query_rows(query(name)):
                check(count < MAX_ROWS, "spawn.row_limit")
                frame = encode_row(count, cells)
                check(size + len(frame) <= MAX_RAW, "spawn.byte_limit")
                handle.write(frame)
                h.update(frame)
                size += len(frame)
                count += 1
            handle.flush()
            os.fsync(handle.fileno())
        streams[name] = {"sha256": h.hexdigest(), "bytes": size, "rows": count,
                         "query": query(name)}
    metadata = {"format": "benilla-spawn-occurrences-v3", "streams": streams,
                "source_snapshot": "phase0-sequential-diagnostic-not-F1", "complete": False}
    with (root / "occurrences.json").open("xb") as handle:
        handle.write(canonical(metadata))
        handle.flush()
        os.fsync(handle.fileno())
    return metadata


class SpawnViews:
    """Compact numeric indexes reference source ordinals, never sorted loader rows."""
    def __init__(self, catalog):
        self.catalog = catalog
        self.schema = catalog._json(catalog.object(catalog.manifest["schema_root"]))["tables"]
        self.indexes, self.counts, self.raw_hashes, self.bits_hashes = {}, {}, {}, {}
        self.references = {"entry_zero": 0, "invalid_coordinates": 0}
        self.index_bytes = 0
        self.index_roots = None
        for name in FAMILIES:
            check(name in catalog.relations, "spawn.missing_relation")
            columns = self.schema[name]["columns"]
            floats = [i for i, c in enumerate(columns)
                      if c["DATA_TYPE"] in ("float", "double", "decimal")]
            raw, bits, count = hashlib.sha256(), hashlib.sha256(), 0
            records = []
            for view in catalog.scan(name):
                cells = view.cells()
                raw.update(encode_row(view.ordinal, cells))
                for col in floats:
                    bits.update(struct.pack("<QII", view.ordinal, col,
                                            struct.unpack("<I", struct.pack("<f", floating(cells[col])))[0]))
                if name in JOINS:
                    d = dict(zip((c["COLUMN_NAME"] for c in columns), cells))
                    map_id, guid = uint(d["map"]), uint(d["guid"])
                    self.counts.setdefault(str(map_id), {"creature": 0, "gameobject": 0})[name] += 1
                    self.references["entry_zero"] += uint(d["id"]) == 0
                    check(self.index_bytes + (len(records)+1)*80 <= DECODED, "spawn.index_limit")
                    try:
                        x, y = floating(d["position_x"]), floating(d["position_y"])
                        check(math.isfinite(x) and math.isfinite(y), "spawn.invalid_coordinates")
                        grid, cell = coordinates(x, y)
                        records.append(INDEX.pack(map_id, grid, cell, guid, view.ordinal))
                    except ExportError as error:
                        if str(error) != "spawn.invalid_coordinates":
                            raise
                        self.references["invalid_coordinates"] += 1
                count += 1
            self.raw_hashes[name], self.bits_hashes[name] = raw.hexdigest(), bits.hexdigest()
            if name in JOINS:
                records.sort()
                self.indexes[name] = records
                self.index_bytes += sys.getsizeof(records) + sum(map(sys.getsizeof, records))
                check(self.index_bytes <= DECODED, "spawn.index_limit")
            self.counts[name] = count

    def columns(self, name):
        return [c["COLUMN_NAME"] for c in self.schema[name]["columns"]]

    def scan(self, name):
        return self.catalog.scan(name)

    def lookup(self, name, *values):
        relation = self.catalog.relations[name]
        check(len(values) == len(relation["key_columns"]), "spawn.key")
        return self.catalog.lookup(name, key_for(list(values), range(len(values))))

    def map_rows(self, name, map_id, grid=None):
        check(name in self.indexes and type(map_id) is int and 0 <= map_id <= 0xffffffff,
              "spawn.key")
        prefix = struct.pack(">I", map_id)
        if grid is not None:
            check(type(grid) is int and 0 <= grid < 4096, "spawn.key")
            prefix += struct.pack(">I", grid)
        records = self.indexes[name]
        at = bisect.bisect_left(records, prefix)
        while at < len(records) and records[at].startswith(prefix):
            ordinal = INDEX.unpack(records[at])[-1]
            yield ordinal, self.catalog.row(name, ordinal)
            at += 1

    def materialize_indexes(self):
        writer = _IndexWriter(self.catalog)
        roots = {name: writer.key_tree(self.catalog.relations[name]["id"],
                 ((r[:16], INDEX.unpack(r)[-1]) for r in records))
                 for name, records in self.indexes.items()}
        value = {"format": "benilla-spawn-indexes-v3", "catalog_sha256": self.catalog.manifest["catalog_sha256"],
                 "indexes": roots, "complete": False}
        raw = canonical(value)
        import secrets
        name = ".spawn-indexes-" + secrets.token_hex(16)
        fd = self.catalog._root.open_file(name, create=True)
        try:
            with os.fdopen(fd, "wb") as handle:
                handle.write(raw)
                handle.flush()
                os.fsync(handle.fileno())
            self.catalog._indexes.sync()
            self.catalog._root.link(name, "spawn-indexes.json")
            self.catalog._root.sync()
        finally:
            self.catalog._root.unlink(name)
        self.verify_indexes()

    def verify_indexes(self):
        fd = self.catalog._file("spawn-indexes.json", self.catalog._root, 1048576)
        value = self.catalog._json(self.catalog._read_fd(fd, 0, os.fstat(fd).st_size))
        check(type(value) is dict and set(value) == {"format", "catalog_sha256", "indexes", "complete"}
              and value["format"] == "benilla-spawn-indexes-v3"
              and value["catalog_sha256"] == self.catalog.manifest["catalog_sha256"]
              and value["complete"] is False and type(value["indexes"]) is dict
              and set(value["indexes"]) == set(self.indexes), "spawn.index")
        for name, records in self.indexes.items():
            root = value["indexes"][name]
            self.catalog._root_descriptor(root, len(records))
            pairs = self.catalog._keys(root, self.catalog.relations[name]["id"], 0, False)
            expected = ((r[:16], INDEX.unpack(r)[-1]) for r in records)
            for actual, wanted in itertools.zip_longest(pairs, expected):
                check(actual == wanted, "spawn.index")
        self.catalog._checked_identities()
        self.index_roots = value["indexes"]

    def expected_join(self, name):
        self._join_charge = self.index_bytes + 2097152 + sum(
            self.catalog.relations[t]["rows"]*256 for t, _, _ in JOINS[name])
        check(self._join_charge <= DECODED, "spawn.join_limit")
        ancillary = []
        for table, key, value in JOINS[name]:
            cols = [c.lower() for c in self.columns(table)]
            index = {}
            for row in self.catalog.scan(table):
                cells = row.cells()
                if cells[cols.index(key)] is not None:
                    index.setdefault(uint(cells[cols.index(key)]), []).append(cells[cols.index(value)])
            ancillary.append(index)
        cols = [c.lower() for c in self.columns(name)]
        positions = [cols.index(c.lower()) for c in PROJECTIONS[name]]
        for row in self.catalog.scan(name):
            cells = row.cells()
            guid, entry = uint(cells[cols.index("guid")]), uint(cells[cols.index("id")])
            choices = [idx.get(entry if spec[1] == "id" else guid, [None])
                       for idx, spec in zip(ancillary, JOINS[name])]
            for tail in itertools.product(*choices):
                yield [cells[i] for i in positions] + list(tail)
    def alternatives(self, kind, guid):
        check(kind in JOINS, "spawn.key")
        name = kind + "_spawn_entry"
        cols = self.columns(name)
        for row in self.catalog.scan(name):
            cells = row.cells()
            if uint(cells[cols.index("guid")]) == guid:
                yield row.ordinal, uint(cells[cols.index("entry")])

    def model_closure(self, entry):
        pending = []
        for ordinal in self.lookup("creature_template", str(entry).encode()):
            cols, cells = self.columns("creature_template"), self.catalog.row("creature_template", ordinal)
            pending.extend(uint(cells[cols.index("DisplayId" + str(i))]) for i in range(1, 5))
        seen, missing = set(), set()
        while pending:
            model = pending.pop()
            if model == 0 or model in seen:
                continue
            seen.add(model)
            check(len(seen) <= self.counts["creature_model_info"] + 4, "spawn.closure_limit")
            found = False
            for ordinal in self.lookup("creature_model_info", str(model).encode()):
                found = True
                cols, cells = self.columns("creature_model_info"), self.catalog.row("creature_model_info", ordinal)
                pending.append(uint(cells[cols.index("modelid_other_gender")]))
                pending.append(uint(cells[cols.index("modelid_alternative")]))
            if not found:
                missing.add(model)
        return {"models": sorted(seen), "missing": sorted(missing)}


    def verify_occurrences(self, root):
        root = Directory(root)
        metadata = self.catalog._json(_regular(root / "occurrences.json", 1048576))
        check(type(metadata) is dict and set(metadata) == {"format", "streams", "source_snapshot", "complete"}
              and metadata["format"] == "benilla-spawn-occurrences-v3"
              and metadata["source_snapshot"] == "phase0-sequential-diagnostic-not-F1"
              and metadata["complete"] is False and type(metadata["streams"]) is dict
              and set(metadata["streams"]) == set(JOINS),
              "spawn.occurrences")
        reports = {}
        for name in JOINS:
            expected = []
            for cells in self.expected_join(name):
                check(len(expected) < MAX_ROWS and self._join_charge + (len(expected)+1)*192 <= DECODED,
                      "spawn.join_limit")
                expected.append(hashlib.sha256(key_for(cells, range(len(cells)))).digest())
            expected.sort()
            descriptor = metadata["streams"][name]
            check(type(descriptor) is dict and set(descriptor) == {"sha256", "bytes", "rows", "query"}
                  and descriptor["query"] == query(name), "spawn.query")
            check(type(descriptor["bytes"]) is int and 0 <= descriptor["bytes"] <= MAX_RAW
                  and type(descriptor["rows"]) is int and 0 <= descriptor["rows"] <= MAX_ROWS, "spawn.hash")
            seen, h = [], hashlib.sha256()
            with _open(root / (name + ".rows")) as handle:
                before = os.fstat(handle.fileno())
                for ordinal in range(len(expected)):
                    size_raw = handle.read(4)
                    check(len(size_raw) == 4, "spawn.exhausted")
                    size, = struct.unpack("<I", size_raw)
                    check(16 <= size <= FRAME, "catalog.frame_limit")
                    frame = size_raw + handle.read(size - 4)
                    check(len(frame) == size, "catalog.truncated")
                    cells = decode_row(frame, ordinal, len(PROJECTIONS[name])+len(JOINS[name]))
                    h.update(frame)
                    seen.append(hashlib.sha256(key_for(cells, range(len(cells)))).digest())
                check(not handle.read(1), "spawn.trailing")
                after = os.fstat(handle.fileno())
                check(Catalog._identity(before) == Catalog._identity(after), "catalog.changed")
            check(before.st_size == descriptor["bytes"] and h.hexdigest() == descriptor["sha256"]
                  and len(seen) == descriptor["rows"], "spawn.hash")
            seen.sort()
            check(seen == expected, "spawn.join_mismatch")
            reports[name] = {"rows": len(seen), "occurrence_sha256": h.hexdigest(),
                             "multiset_sha256": hashlib.sha256(b"".join(seen)).hexdigest()}
        return reports

    def report(self, occurrences):
        check(self.index_roots is not None, "spawn.index_missing")
        result = {"format": "benilla-spawn-views-v3", "catalog_sha256": self.catalog.manifest["catalog_sha256"],
                  "counts": self.counts, "raw_sha256": self.raw_hashes, "float_bits_sha256": self.bits_hashes,
                  "index_pages": self.index_roots,
                  "indexes": {name: hashlib.sha256(b"".join(rows)).hexdigest()
                              for name, rows in self.indexes.items()}, "joined": occurrences,
                  "dispositions": self.references, "complete": False,
                  "refusals": ["spawn.reference_execution_missing", "spawn.gameplay_not_admitted"]}
        result["view_sha256"] = digest(result)
        return result


def _open(path):
    return open_regular(path)


def _regular(path, limit):
    with _open(path) as handle:
        before = os.fstat(handle.fileno())
        raw = handle.read(limit + 1)
        check(len(raw) <= limit, "catalog.raw_limit")
        check(Catalog._identity(before) == Catalog._identity(os.fstat(handle.fileno())), "catalog.changed")
        return raw


class _IndexWriter(StoreWriter):
    def __init__(self, catalog):
        self.catalog, self.written = catalog, 0

    def put(self, data, directory="objects"):
        check(directory == "indexes", "spawn.index")
        h = hashlib.sha256(data).hexdigest()
        descriptor = {"sha256": h, "bytes": len(data)}
        check(self.catalog.index_bytes + self.written + len(data) <= MAX_INDEX, "catalog.index_limit")
        try:
            fd = self.catalog._indexes.open_file(h, create=True)
        except FileExistsError:
            check(self.catalog.object(descriptor, "indexes") == data, "catalog.hash")
        else:
            with os.fdopen(fd, "wb") as handle:
                handle.write(data)
                handle.flush()
                os.fsync(handle.fileno())
            self.written += len(data)
        return descriptor
