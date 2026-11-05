"""Exact retained transport loader cursors, separate from physical SQL indexes.
SPDX-License-Identifier: GPL-3.0-or-later
"""
import hashlib
import os
from pathlib import Path
import struct
from .core import canonical, check
from .catalog_io import FRAME, MAX_RAW, DECODED, Catalog, encode_row, decode_row, key_for
from .catalog_spawns import _open, _regular, uint
from .catalog_schema import policy

FORMAT = "benilla-transport-occurrences-v1"
LOADERS = ("Entities/Transports:39", "Entities/Transports:99")


def specs():
    return {str(i): policy()["loaders"][loader] for i, loader in enumerate(LOADERS)}


def capture_occurrences(source, root):
    root = Path(root)
    root.mkdir()
    streams = {}
    for name, spec in specs().items():
        h, count, size = hashlib.sha256(), 0, 0
        with (root / (name+".rows")).open("xb") as handle:
            for cells in source.query_rows(spec["query"]):
                check(count < 2000000, "dbc.occurrence_limit")
                frame = encode_row(count, cells)
                check(size+len(frame) <= MAX_RAW, "dbc.occurrence_limit")
                handle.write(frame)
                h.update(frame)
                count += 1
                size += len(frame)
            handle.flush()
            os.fsync(handle.fileno())
        streams[name] = {"query": spec["query"], "sha256": h.hexdigest(), "rows": count, "bytes": size}
    result = {"format": FORMAT, "streams": streams, "complete": False,
              "source_snapshot": "phase0-sequential-diagnostic-not-F1"}
    with (root / "occurrences.json").open("xb") as handle:
        handle.write(canonical(result))
        handle.flush()
        os.fsync(handle.fileno())
    return result


def expected(views):
    rows, names, owned = [], {}, 0
    for _, row in views.rows("transports"):
        owned += len(row["name"] or b"")+512
        check(owned <= DECODED, "dbc.occurrence_limit")
        rows.append(hashlib.sha256(key_for([row[col] for col in ("entry", "name", "period")], range(3))).digest())
        if row["entry"] is not None:
            names.setdefault(uint(row["entry"]), []).append(row["name"])
    joined = []
    for _, row in views.rows("gameobject"):
        if row["id"] is None:
            continue
        for name in names.get(uint(row["id"]), []):
            check(owned+(len(joined)+1)*512 <= DECODED, "dbc.occurrence_limit")
            joined.append(hashlib.sha256(key_for([row["guid"], row["id"], name], range(3))).digest())
    return {"0": sorted(rows), "1": sorted(joined)}


def verify_occurrences(views, root):
    root = Path(root)
    meta = views.catalog._json(_regular(root / "occurrences.json", 1048576))
    check(type(meta) is dict and set(meta) == {"format", "streams", "complete", "source_snapshot"}
          and meta["format"] == FORMAT and meta["complete"] is False
          and meta["source_snapshot"] == "phase0-sequential-diagnostic-not-F1"
          and type(meta["streams"]) is dict and set(meta["streams"]) == set(specs()), "dbc.occurrences")
    projected, result = expected(views), {}
    for name, spec in specs().items():
        hashes = projected[name]
        d = meta["streams"][name]
        check(type(d) is dict and set(d) == {"query", "sha256", "rows", "bytes"}
              and d["query"] == spec["query"], "dbc.occurrence_query")
        seen, h = [], hashlib.sha256()
        with _open(root / (name+".rows")) as handle:
            before = os.fstat(handle.fileno())
            for ordinal in range(len(hashes)):
                prefix = handle.read(4)
                check(len(prefix) == 4, "dbc.occurrence_exhausted")
                size, = struct.unpack("<I", prefix)
                check(16 <= size <= FRAME, "catalog.frame_limit")
                frame = prefix+handle.read(size-4)
                check(len(frame) == size, "catalog.truncated")
                cells = decode_row(frame, ordinal, 3)
                h.update(frame)
                seen.append(hashlib.sha256(key_for(cells, range(3))).digest())
            check(not handle.read(1), "dbc.occurrence_trailing")
            check(Catalog._identity(before) == Catalog._identity(os.fstat(handle.fileno())), "catalog.changed")
        check(type(d["rows"]) is int and type(d["bytes"]) is int and d["rows"] == len(seen)
              and d["bytes"] == before.st_size and d["sha256"] == h.hexdigest(), "dbc.occurrence_hash")
        check(sorted(seen) == hashes, "dbc.occurrence_mismatch")
        result[name] = {"source": spec["source"], "query": spec["query"], "rows": len(seen),
                        "occurrence_sha256": h.hexdigest(), "multiset_sha256": hashlib.sha256(b"".join(hashes)).hexdigest()}
    return result
