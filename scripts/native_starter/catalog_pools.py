"""Pinned pool validation candidates, without random selection or admission.
SPDX-License-Identifier: GPL-3.0-or-later
"""
import struct

from classic_export.core import check
from classic_export.catalog_io import MAX_ROWS, DECODED
from classic_export.catalog_spawns import uint, floating


def f32(value):
    return struct.unpack("<f", struct.pack("<f", value))[0]


class PoolState:
    """Source validation candidates retain dependencies on ObjectMgr loaded spawns."""
    def __init__(self, maximum, maps, spawn):
        self.maximum, self.maps, self.spawn = maximum, maps, spawn
        self.templates, self.pool_maps, self.search, self.parents, self.groups = {}, {}, {}, {}, {}
        self.records = []
        self.nested = set()
        self.budget = DECODED

    def emit(self, *record):
        check(len(self.records) < MAX_ROWS, "pool.row_limit")
        check((len(self.records) + 1) * 640 <= self.budget, "pool.decoded_limit")
        self.records.append(list(record))

    def map_check(self, pool, map_id):
        if map_id not in self.maps:
            return "missing_map"
        if pool in self.pool_maps and self.pool_maps[pool] != map_id:
            return "cross_map"
        self.pool_maps[pool] = map_id
        return None

    def add(self, kind, pool, guid, chance):
        # Nonzero chance is explicit only for MaxLimit=1, PoolManager.cpp:139-159.
        self.groups.setdefault((kind, pool), []).append((guid, chance,
            chance != 0 and self.templates.get(pool, 0) == 1))

    def apply(self, name, ordinal, cells):
        if name == "maximum":
            return
        if name == "pool_template":
            pool = uint(cells[0]) & 65535
            check(pool <= self.maximum, "pool.maximum_range")
            self.templates[pool] = uint(cells[1])
            self.emit(name, ordinal, "template", pool, uint(cells[1]))
            return
        guid, pool = uint(cells[0]), uint(cells[1]) & 65535
        chance = floating(cells[2])
        bits = struct.unpack("<I", struct.pack("<f", chance))[0]
        status = None
        if name == "pool_pool":
            guid &= 65535
            if pool > self.maximum:
                status = "mother_out_of_range"
            elif guid > self.maximum:
                status = "child_out_of_range"
            elif guid == pool:
                status = "self_cycle"
        else:
            kind = "creature" if "creature" in name else "gameobject"
            map_id = self.spawn(kind, guid)
            if map_id is None:
                status = "missing_spawn"
            elif pool > self.maximum:
                status = "pool_out_of_range"
        if status is None and (chance < 0 or chance > 100):
            status = "invalid_chance"
        if name != "pool_pool" and status is None:
            if name.endswith("_template") and self.search.get((kind, guid), 0):
                status = "duplicate_guid"
            else:
                status = self.map_check(pool, map_id)
        if status is None:
            kind = "pool" if name == "pool_pool" else kind
            self.add(kind, pool, guid, chance)
            if kind == "pool":
                self.parents.setdefault(guid, pool)
                self.nested.add(guid)
            else:
                self.search.setdefault((kind, guid), pool)
            status = "candidate_accepted"
        self.emit(name, ordinal, status, guid, pool, bits)

    def finish(self):
        # Traverse i<MAX and erase the closing relation, PoolManager.cpp:988-1021.
        for start in range(self.maximum):
            checked, child = set(), start
            while child in self.parents:
                parent = self.parents[child]
                status = self.map_check(parent, self.pool_maps[child]) if child in self.pool_maps else None
                checked.add(child)
                if status is None and parent in checked:
                    status = "cycle_removed"
                if status is not None:
                    self.emit("hierarchy", start, status, child, parent)
                    self.parents.pop(child)
                    group = self.groups[("pool", parent)]
                    # Remove first match from each chance list, PoolManager.cpp:348-365.
                    for explicit in (True, False):
                        match = next((i for i, r in enumerate(group) if r[0] == child and r[2] == explicit), None)
                        if match is not None:
                            group.pop(match)
                    break
                child = parent
        disabled = []
        for pool in sorted(self.templates):
            if pool in self.nested:
                continue
            for kind in ("gameobject", "creature", "pool"):
                group = self.groups.get((kind, pool), [])
                total = 0.0
                if any(not r[2] for r in group):
                    continue
                for _, chance, _ in group:
                    total = f32(total + chance)
                if total not in (0, 100):
                    disabled.append(pool)
                    self.emit("integrity", pool, "autospawn_disabled", kind,
                              struct.unpack("<I", struct.pack("<f", total))[0])
                    break
        return {"parents": [[c, p] for c, p in sorted(self.parents.items())],
                "maps": [[p, m] for p, m in sorted(self.pool_maps.items())],
                "autospawn_disabled": disabled}
