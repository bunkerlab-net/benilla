"""Pinned group/link/waypoint loader inputs, not executable gameplay managers.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from __future__ import annotations
import hashlib
import os
from pathlib import Path
import struct

from .core import canonical, check, digest
from .catalog_io import Catalog, FRAME, MAX_RAW, MAX_ROWS, DECODED, encode_row, decode_row, key_for
from .catalog_schema import policy
from .catalog_spawns import uint, floating, _open, _regular

# Each query and source location is pinned by the reviewed coverage policy.
LOADERS = (
    "Globals/ObjectMgr:1125", "Globals/ObjectMgr:1192", "Globals/ObjectMgr:1250",
    "Globals/ObjectMgr:1355", "Globals/ObjectMgr:1398", "Globals/ObjectMgr:1426",
    "Entities/CreatureLinkingMgr:75", "Entities/CreatureLinkingMgr:120",
    "MotionGenerators/WaypointManager:173", "MotionGenerators/WaypointManager:198",
    "MotionGenerators/WaypointManager:302", "MotionGenerators/WaypointManager:329",
    "MotionGenerators/WaypointManager:400", "MotionGenerators/WaypointManager:427",
    "MotionGenerators/WaypointManager:441",
)
FORMAT = "benilla-group-occurrences-v1"


def specs():
    return {str(i): policy()["loaders"][loader] for i, loader in enumerate(LOADERS)}


def capture_occurrences(source, root):
    root = Path(root)
    root.mkdir()
    streams = {}
    for name, spec in specs().items():
        h, count, size = hashlib.sha256(), 0, 0
        with (root / (name + ".rows")).open("xb") as handle:
            for cells in source.query_rows(spec["query"]):
                check(count < MAX_ROWS, "groups.row_limit")
                frame = encode_row(count, cells)
                check(size + len(frame) <= MAX_RAW, "groups.byte_limit")
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


class GroupViews:
    def __init__(self, catalog):
        self.catalog = catalog
        self.schema = catalog._json(catalog.object(catalog.manifest["schema_root"]))["tables"]

    def rows(self, table):
        columns = [c["COLUMN_NAME"].lower() for c in self.schema[table]["columns"]]
        for view in self.catalog.scan(table):
            yield view.ordinal, dict(zip(columns, view.cells()))

    def expected(self, spec):
        query = spec["query"]
        table = spec["relations"][0]
        projection = query.split(" FROM ")[0][7:].lower().split(", ")
        if "GROUP BY" in query:
            counts = {}
            for _, row in self.rows(table):
                key = row[projection[0]]
                counts[key] = counts.get(key, 0) + (row["point"] is not None)
                check(len(counts)*256 <= DECODED, "groups.view_limit")
            for key, count in counts.items():
                yield [key, str(count).encode()]
        else:
            for _, row in self.rows(table):
                yield [row[col] for col in projection]

    def verify_occurrences(self, root):
        root = Path(root)
        meta = self.catalog._json(_regular(root / "occurrences.json", 1048576))
        check(type(meta) is dict and set(meta) == {"format", "streams", "complete", "source_snapshot"}
              and meta["format"] == FORMAT and meta["complete"] is False
              and meta["source_snapshot"] == "phase0-sequential-diagnostic-not-F1"
              and type(meta["streams"]) is dict and set(meta["streams"]) == set(specs()), "groups.occurrences")
        reports = {}
        for name, spec in specs().items():
            expected = []
            columns = None
            for cells in self.expected(spec):
                columns = len(cells)
                check((len(expected)+1)*128 <= DECODED, "groups.view_limit")
                expected.append(hashlib.sha256(key_for(cells, range(len(cells)))).digest())
            if columns is None:
                columns = len(spec["query"].split(" FROM ")[0][7:].split(", "))
            d = meta["streams"][name]
            check(type(d) is dict and set(d) == {"query", "sha256", "rows", "bytes"}
                  and d["query"] == spec["query"], "groups.query")
            check(type(d["rows"]) is int and 0 <= d["rows"] <= MAX_ROWS
                  and type(d["bytes"]) is int and 0 <= d["bytes"] <= MAX_RAW, "groups.hash")
            seen, h = [], hashlib.sha256()
            with _open(root / (name + ".rows")) as handle:
                before = os.fstat(handle.fileno())
                for ordinal in range(len(expected)):
                    size_raw = handle.read(4)
                    check(len(size_raw) == 4, "groups.exhausted")
                    size, = struct.unpack("<I", size_raw)
                    check(16 <= size <= FRAME, "catalog.frame_limit")
                    frame = size_raw + handle.read(size-4)
                    check(len(frame) == size, "catalog.truncated")
                    cells = decode_row(frame, ordinal, columns)
                    h.update(frame)
                    seen.append(hashlib.sha256(key_for(cells, range(columns))).digest())
                check(not handle.read(1), "groups.trailing")
                check(Catalog._identity(before) == Catalog._identity(os.fstat(handle.fileno())), "catalog.changed")
            check(d["rows"] == len(seen) and d["bytes"] == before.st_size and d["sha256"] == h.hexdigest(), "groups.hash")
            expected.sort()
            seen.sort()
            check(seen == expected, "groups.view_mismatch")
            reports[name] = {"source": spec["source"], "query": spec["query"], "rows": len(seen),
                             "occurrence_sha256": h.hexdigest(),
                             "multiset_sha256": hashlib.sha256(b"".join(seen)).hexdigest()}
        return reports

    def report(self, occurrences):
        tables = sorted({s["relations"][0] for s in specs().values()})
        families = {}
        for table in tables:
            raw, bits, count = hashlib.sha256(), hashlib.sha256(), 0
            floats = [(i, c["COLUMN_NAME"].lower()) for i, c in enumerate(self.schema[table]["columns"])
                      if c["DATA_TYPE"] in ("float", "double", "decimal")]
            for ordinal, row in self.rows(table):
                raw.update(encode_row(ordinal, list(row.values())))
                for col, name in floats:
                    bits.update(struct.pack("<QII", ordinal, col, struct.unpack("<I", struct.pack("<f", floating(row[name])))[0]))
                count += 1
            families[table] = {"rows": count, "raw_sha256": raw.hexdigest(), "float_bits_sha256": bits.hexdigest()}
        result = {"format": "benilla-group-views-v1", "catalog_sha256": self.catalog.manifest["catalog_sha256"],
                  "families": families, "loaders": occurrences, "plans": self.plans(), "complete": False,
                  "refusals": ["groups.reference_execution_missing", "groups.gameplay_not_admitted"]}
        result["view_sha256"] = digest(result)
        return result

    def plans(self):
        # Numeric references retain raw ordinals; ObjectMgr.cpp:1273-1323 types GUIDs.
        charge = sum(self.catalog.relations[t]["rows"]*160 for t in
                     ("creature", "gameobject", "creature_template", "gameobject_template"))
        charge += sum(self.catalog.relations[s["relations"][0]]["rows"]*160
                      for s in list(specs().values())[:8])
        charge += max(self.catalog.relations[t]["rows"] for t in
                      ("creature_movement", "creature_movement_template", "waypoint_path"))*160
        check(charge <= DECODED, "groups.view_limit")
        spawns = {}
        templates = {}
        for kind in ("creature", "gameobject"):
            spawns[kind] = {uint(r["guid"]): (uint(r["id"]), uint(r["map"]))
                            for _, r in self.rows(kind)}
            templates[kind] = {uint(r["entry"]) for _, r in self.rows(kind + "_template")}
        groups = {uint(r["id"]): uint(r["type"]) for _, r in self.rows("spawn_group")}
        formations = {uint(r["id"]) for _, r in self.rows("spawn_group_formation")}
        entries = {uint(r["id"]) for _, r in self.rows("spawn_group_entry")}
        result = {key: 0 for key in (
            "groups_without_formation", "invalid_group_type", "missing_group", "missing_spawn",
            "duplicate_membership", "cross_map_groups", "missing_entry", "linked_missing_group",
            "squad_overrides_entries", "link_missing_master", "link_missing_slave",
            "link_cross_map", "link_invalid_flag", "link_self_despawn", "link_nonunique_master",
            "waypoint_zero_paths", "waypoint_gap_edges", "waypoint_missing_owner")}
        result["groups_without_formation"] = len(groups.keys() - formations)
        result["invalid_group_type"] = sum(t > 1 for t in groups.values())
        seen, maps = set(), {}
        for _, r in self.rows("spawn_group_spawn"):
            group, guid = uint(r["id"]), uint(r["guid"])
            if group not in groups:
                result["missing_group"] += 1
                continue
            if groups[group] > 1:
                continue
            kind = ("creature", "gameobject")[groups[group]]
            key = kind, guid
            result["duplicate_membership"] += key in seen
            seen.add(key)
            if guid not in spawns[kind]:
                result["missing_spawn"] += 1
            else:
                maps.setdefault(group, set()).add(spawns[kind][guid][1])
        result["cross_map_groups"] = sum(len(m) > 1 for m in maps.values())
        for _, r in self.rows("spawn_group_entry"):
            group = uint(r["id"])
            if group not in groups:
                result["missing_group"] += 1
            elif groups[group] < 2:
                result["missing_entry"] += uint(r["entry"]) not in templates[("creature", "gameobject")[groups[group]]]
        for _, r in self.rows("spawn_group_linked_group"):
            result["linked_missing_group"] += uint(r["id"]) not in groups or uint(r["linkedid"]) not in groups
        for _, r in self.rows("spawn_group_squad"):
            group = uint(r["id"])
            result["missing_group"] += group not in groups
            result["squad_overrides_entries"] += group in groups and group in entries
        # Flags narrow to uint16; unique-master check is CreatureLinkingMgr.cpp:218-249.
        masters = {}
        for entry, map_id in spawns["creature"].values():
            key = entry, map_id
            masters[key] = masters.get(key, 0) + 1
        for table in ("creature_linking", "creature_linking_template"):
            by_entry = table.endswith("_template")
            for _, r in self.rows(table):
                slave = uint(r["entry" if by_entry else "guid"])
                master = uint(r["master_entry" if by_entry else "master_guid"])
                domain = templates["creature"] if by_entry else spawns["creature"]
                result["link_missing_slave"] += slave not in domain
                result["link_missing_master"] += master not in domain
                if not by_entry and slave in domain and master in domain:
                    result["link_cross_map"] += domain[slave][1] != domain[master][1]
                flag = uint(r["flag"]) & 65535
                result["link_invalid_flag"] += flag == 0 or bool(flag & 32768)
                result["link_self_despawn"] += bool(flag & 256) and slave == master
                if by_entry and (uint(r["search_range"]) & 65535) == 0 and flag & 3584:
                    result["link_nonunique_master"] += masters.get((master, uint(r["map"])), 0) != 1
        # Point zero blacklists the path; gaps remain gaps, WaypointManager.cpp:270-272,388-390.
        for table, owner in (("creature_movement", "id"), ("creature_movement_template", "entry"), ("waypoint_path", "pathid")):
            paths = {}
            for _, r in self.rows(table):
                identity = uint(r[owner])
                key = ((identity << 8) + uint(r["pathid"])) & 0xffffffff if table.endswith("_template") else identity
                paths.setdefault(key, set()).add(uint(r["point"]))
                if table != "waypoint_path":
                    domain = templates["creature"] if table.endswith("_template") else spawns["creature"]
                    result["waypoint_missing_owner"] += identity not in domain
            for points in paths.values():
                result["waypoint_zero_paths"] += 0 in points
                ordered = sorted(points)
                result["waypoint_gap_edges"] += sum(b != a+1 for a, b in zip(ordered, ordered[1:]))
        return result
