"""Complete pinned PoolManager input views, not sampled pool outcomes.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from __future__ import annotations

import hashlib
import os
from pathlib import Path
import struct

from .core import canonical, check, digest
from .catalog_io import Catalog, FRAME, MAX_ROWS, MAX_RAW, DECODED, encode_row, decode_row, key_for
from .catalog_schema import policy
from .catalog_spawns import uint, _open, _regular
from native_starter import catalog_pools as plans
from .catalog_paths import Directory

TABLES = ("pool_template", "pool_creature", "pool_creature_template",
          "pool_gameobject", "pool_gameobject_template", "pool_pool")
STREAMS = ("maximum",) + TABLES
LINES = (606, 618, 656, 716, 792, 853, 927)
COLS = (("maximum",), ("entry", "max_limit", "description"),
        ("guid", "pool_entry", "chance"), ("guid", "pool_entry", "chance", "id"),
        ("guid", "pool_entry", "chance"), ("guid", "pool_entry", "chance", "id"),
        ("pool_id", "mother_pool", "chance"))


def query(name):
    return policy()["loaders"]["Pools/PoolManager:" + str(LINES[STREAMS.index(name)])]["query"]


def capture_occurrences(source, root):
    root = Path(root)
    root.mkdir()
    streams = {}
    for name in STREAMS:
        h, count, size = hashlib.sha256(), 0, 0
        with (root / (name + ".rows")).open("xb") as handle:
            for cells in source.query_rows(query(name)):
                check(count < MAX_ROWS, "pool.row_limit")
                frame = encode_row(count, cells)
                check(size + len(frame) <= MAX_RAW, "pool.byte_limit")
                handle.write(frame)
                h.update(frame)
                size += len(frame)
                count += 1
            handle.flush()
            os.fsync(handle.fileno())
        streams[name] = {"sha256": h.hexdigest(), "bytes": size, "rows": count, "query": query(name)}
    result = {"format": "benilla-pool-occurrences-v3", "streams": streams,
              "source_snapshot": "phase0-sequential-diagnostic-not-F1", "complete": False}
    with (root / "occurrences.json").open("xb") as handle:
        handle.write(canonical(result))
        handle.flush()
        os.fsync(handle.fileno())
    return result


def cell_hash(cells):
    return hashlib.sha256(key_for(cells, range(len(cells)))).digest()


class PoolViews:
    def __init__(self, catalog, spawns):
        self.catalog, self.spawns = catalog, spawns
        self.counts, self.raw = {}, {}
        check(sum(catalog.relations[n]["rows"] for n in TABLES) * 768 + spawns.index_bytes <= DECODED,
              "pool.decoded_limit")
        for name in TABLES:
            h, count = hashlib.sha256(), 0
            for row in spawns.scan(name):
                h.update(encode_row(row.ordinal, row.cells()))
                count += 1
            self.counts[name], self.raw[name] = count, h.hexdigest()

    def projected(self, name):
        cols = [c.lower() for c in self.spawns.columns(name)]
        positions = [cols.index(c) for c in COLS[STREAMS.index(name)]]
        for row in self.spawns.scan(name):
            cells = row.cells()
            yield [cells[i] for i in positions]

    def expected(self, name):
        if name == "maximum":
            values = [uint(c[0]) for c in self.projected("pool_template")]
            yield [str(max(values)).encode() if values else None]
        elif name.endswith("_template") and name != "pool_template":
            kind = "creature" if "creature" in name else "gameobject"
            cols = [c.lower() for c in self.spawns.columns(name)]
            by_entry = {}
            for row in self.spawns.scan(name):
                c = row.cells()
                check(all(c[cols.index(n)] is None or len(c[cols.index(n)]) < 128
                          for n in ("id", "pool_entry", "chance")), "spawn.numeric_limit")
                by_entry.setdefault(uint(c[cols.index("id")]), []).append(
                    [c[cols.index("pool_entry")], c[cols.index("chance")], c[cols.index("id")]])
            spawn_cols = [c.lower() for c in self.spawns.columns(kind)]
            matched = set()
            for row in self.spawns.scan(kind):
                c = row.cells()
                entry = uint(c[spawn_cols.index("id")])
                for tail in by_entry.get(entry, []):
                    matched.add(entry)
                    yield [c[spawn_cols.index("guid")]] + tail
            for entry, tails in by_entry.items():
                if entry not in matched:
                    for tail in tails:
                        yield [None] + tail
        else:
            yield from self.projected(name)

    def spawn_map(self, kind, guid):
        found = None
        cols = [c.lower() for c in self.spawns.columns(kind)]
        for ordinal in self.spawns.lookup(kind, str(guid).encode()):
            found = uint(self.catalog.row(kind, ordinal)[cols.index("map")])
        return found

    def maps(self):
        dbcs = self.catalog._json(self.catalog.object(self.catalog.manifest["dbc_root"]))
        check("Map.dbc" in dbcs, "pool.map_dbc_missing")
        raw = self.catalog.object(dbcs["Map.dbc"])
        check(len(raw) >= 20 and raw[:4] == b"WDBC", "pool.map_dbc")
        count, fields, width, strings = struct.unpack_from("<IIII", raw, 4)
        check(fields >= 1 and width == fields * 4 and 20 + count * width + strings == len(raw), "pool.map_dbc")
        return {struct.unpack_from("<I", raw, 20 + i * width)[0] for i in range(count)}

    def verify_occurrences(self, root, records=None):
        root = Directory(root)
        meta = self.catalog._json(_regular(root / "occurrences.json", 1048576))
        check(type(meta) is dict and set(meta) == {"format", "streams", "source_snapshot", "complete"}
              and meta["format"] == "benilla-pool-occurrences-v3" and meta["complete"] is False
              and meta["source_snapshot"] == "phase0-sequential-diagnostic-not-F1"
              and type(meta["streams"]) is dict and set(meta["streams"]) == set(STREAMS), "pool.occurrences")
        maximum = max((uint(c[0]) for c in self.projected("pool_template")), default=0) & 65535
        state = plans.PoolState(maximum, self.maps(), self.spawn_map)
        streams = {}
        disposition_hash = hashlib.sha256()
        dispositions = {}
        for name in STREAMS:
            expected = []
            for cells in self.expected(name):
                check(len(expected) < MAX_ROWS, "pool.join_limit")
                check((len(expected)+1)*256 + len(state.records)*640
                      + self.spawns.index_bytes + 2097152 <= DECODED, "pool.decoded_limit")
                expected.append(cell_hash(cells))
            expected.sort()
            state.budget = DECODED - self.spawns.index_bytes - len(expected)*256 - 2097152
            d = meta["streams"][name]
            check(type(d) is dict and set(d) == {"sha256", "bytes", "rows", "query"}
                  and d["query"] == query(name), "pool.query")
            check(type(d["rows"]) is int and 0 <= d["rows"] <= MAX_ROWS
                  and type(d["bytes"]) is int and 0 <= d["bytes"] <= MAX_RAW, "pool.hash")
            seen, h = [], hashlib.sha256()
            with _open(root / (name + ".rows")) as handle:
                before = os.fstat(handle.fileno())
                for ordinal in range(len(expected)):
                    raw = handle.read(4)
                    check(len(raw) == 4, "pool.exhausted")
                    size, = struct.unpack("<I", raw)
                    check(16 <= size <= FRAME, "catalog.frame_limit")
                    raw += handle.read(size - 4)
                    check(len(raw) == size, "catalog.truncated")
                    cells = decode_row(raw, ordinal, len(COLS[STREAMS.index(name)]))
                    seen.append(cell_hash(cells))
                    h.update(raw)
                    state.apply(name, ordinal, cells)
                check(not handle.read(1), "pool.trailing")
                check(Catalog._identity(before) == Catalog._identity(os.fstat(handle.fileno())), "catalog.changed")
            check(before.st_size == d["bytes"] and h.hexdigest() == d["sha256"] and len(seen) == d["rows"], "pool.hash")
            seen.sort()
            check(seen == expected, "pool.join_mismatch")
            streams[name] = {"rows": len(seen), "occurrence_sha256": h.hexdigest(),
                             "multiset_sha256": hashlib.sha256(b"".join(seen)).hexdigest()}
        hierarchy = state.finish()
        for record in state.records:
            disposition_hash.update(canonical(record) + b"\n")
            dispositions[record[2]] = dispositions.get(record[2], 0) + 1
        report = {"format": "benilla-pool-views-v3", "catalog_sha256": self.catalog.manifest["catalog_sha256"],
                  "counts": self.counts, "raw_sha256": self.raw, "streams": streams,
                  "dispositions": dispositions, "disposition_sha256": disposition_hash.hexdigest(),
                  "hierarchy_sha256": digest(hierarchy), "hierarchy_counts": {k: len(v) for k, v in hierarchy.items()},
                  "complete": False, "refusals": ["pool.reference_execution_missing", "pool.gameplay_not_admitted",
                  "pool.objectmgr_loaded_spawn_validation_pending"]}
        report["view_sha256"] = digest(report)
        self.records, self.hierarchy = state.records, hierarchy
        if records is not None:
            records = Path(records)
            records.mkdir()
            with (records / "dispositions.jsonl").open("xb") as handle:
                for record in state.records:
                    handle.write(canonical(record) + b"\n")
                handle.flush()
                os.fsync(handle.fileno())
            with (records / "hierarchy.json").open("xb") as handle:
                handle.write(canonical(hierarchy))
                handle.flush()
                os.fsync(handle.fileno())
        return report
