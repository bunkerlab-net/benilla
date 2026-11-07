"""Pinned event/weather inputs, independent of runtime calendar evaluation.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from __future__ import annotations
import hashlib
import os
from pathlib import Path
import re
import struct

from .core import canonical, check, digest, decode_json
from .catalog_io import Catalog, FRAME, MAX_RAW, MAX_ROWS, DECODED, encode_row, decode_row, key_for
from .catalog_schema import policy
from .catalog_spawns import uint, _open, _regular
from .catalog_paths import Directory

LOADERS = tuple("GameEvents/GameEventMgr:" + str(n) for n in (92,107,196,237,319,410,499,557)) + ("Weather/Weather:436",)
FORMAT = "benilla-event-occurrences-v1"
TABLES = ("game_event", "game_event_time", "game_event_creature", "game_event_gameobject", "game_event_creature_data", "game_event_quest", "game_event_mail", "game_weather")


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
                check(count < MAX_ROWS, "events.row_limit")
                frame = encode_row(count, cells)
                check(size + len(frame) <= MAX_RAW, "events.byte_limit")
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


def initial_state(snapshot, environment):
    """Bind immutable dump bytes, never query a running CharacterDatabase."""
    raw = _regular(Path(snapshot), 1048576)
    start = b"LOCK TABLES `game_event_status` WRITE;\n"
    end = b"UNLOCK TABLES;"
    check(raw.count(start) == 1 and raw.count(b"CREATE TABLE `game_event_status` (") == 1,
          "events.initial_state")
    check(end in raw.split(start)[1], "events.initial_state")
    body = raw.split(start)[1].split(end)[0]
    check(raw.count(b"INSERT INTO `game_event_status`") == body.count(b"INSERT INTO `game_event_status`")
          and b"CREATE TABLE `game_event_status` (\n  `event` smallint(6) unsigned NOT NULL DEFAULT 0,\n  PRIMARY KEY (`event`)\n)" in raw,
          "events.initial_state")
    lines = body.splitlines()
    check(len(lines) >= 2 and lines[0] == b"/*!40000 ALTER TABLE `game_event_status` DISABLE KEYS */;"
          and lines[-1] == b"/*!40000 ALTER TABLE `game_event_status` ENABLE KEYS */;", "events.initial_state")
    values = []
    for line in lines[1:-1]:
        prefix = b"INSERT INTO `game_event_status` VALUES "
        check(line.startswith(prefix) and line.endswith(b";"), "events.initial_state")
        payload = line[len(prefix):-1]
        check(re.fullmatch(rb"\([0-9]+\)(,\([0-9]+\))*", payload) is not None, "events.initial_state")
        values.extend(int(v) for v in re.findall(rb"[0-9]+", payload))
    check(len(values) <= 65536 and all(v <= 65535 for v in values) and len(set(values)) == len(values), "events.initial_state")
    evidence_raw = _regular(Path(environment), 1048576)
    evidence = decode_json(evidence_raw, 1048576)
    check(type(evidence) is dict and type(evidence.get("environment")) is dict, "events.calendar")
    check(evidence.get("environment", {}).get("TZ") == "UTC"
          and evidence.get("environment", {}).get("LC_ALL") == "C", "events.calendar")
    tzdata = evidence.get("tzdata_utc")
    check(type(tzdata) is dict and set(tzdata) == {"path", "sha256", "bytes"}
          and tzdata["bytes"] == 114 and tzdata["sha256"] == "8b85846791ab2c8a5463c83a5be3c043e2570d7448434d41398969ed47e3e6f2",
          "events.calendar")
    calendar = {"policy": "recorded_samples_paused_server_v1", "timezone": "UTC", "locale": "C",
                "calendar": "Gregorian", "tzdata": tzdata,
                "evidence": {"sha256": hashlib.sha256(evidence_raw).hexdigest(), "bytes": len(evidence_raw)},
                "runtime_clock_coverage": False}
    return {"snapshot": {"sha256": hashlib.sha256(raw).hexdigest(), "bytes": len(raw)},
            "database": "characters", "table": "game_event_status", "query": "SELECT event FROM game_event_status",
            "source": "src/game/GameEvents/GameEventMgr.cpp:640", "events": values,
            "status_rows": len(values), "calendar": calendar,
            "runtime_status": "not_evaluated"}


class EventViews:
    def __init__(self, catalog):
        self.catalog = catalog
        self.schema = catalog._json(catalog.object(catalog.manifest["schema_root"]))["tables"]

    def rows(self, table):
        columns = [c["COLUMN_NAME"].lower() for c in self.schema[table]["columns"]]
        for view in self.catalog.scan(table):
            yield view.ordinal, dict(zip(columns, view.cells()))

    def expected(self, spec):
        query, table = spec["query"], spec["relations"][0]
        if "MAX(entry)" in query:
            maximum = max((r["entry"] for _, r in self.rows(table)), key=uint, default=None)
            yield [maximum]
            return
        projection = [s.strip().lower() for s in query[7:].split(" FROM ")[0].split(",")]
        if "LEFT OUTER JOIN" in query:
            target = spec["relations"][1]
            check(self.catalog.relations[target]["rows"] * 192 <= DECODED, "events.view_limit")
            matches = {}
            for _, row in self.rows(target):
                if row["guid"] is not None:
                    matches.setdefault(uint(row["guid"]), []).append(row["guid"])
            for _, row in self.rows(table):
                for guid in matches.get(uint(row["guid"]), [None]):
                    yield [guid if col.startswith(target+".") else row[col.split(".")[-1]] for col in projection]
        else:
            for _, row in self.rows(table):
                yield [row[col] for col in projection]

    def verify_occurrences(self, root):
        root = Directory(root)
        meta = self.catalog._json(_regular(root / "occurrences.json", 1048576))
        check(type(meta) is dict and set(meta) == {"format", "streams", "complete", "source_snapshot"}
              and meta["format"] == FORMAT and meta["complete"] is False
              and meta["source_snapshot"] == "phase0-sequential-diagnostic-not-F1"
              and type(meta["streams"]) is dict and set(meta["streams"]) == set(specs()), "events.occurrences")
        reports = {}
        for name, spec in specs().items():
            columns = len(spec["query"][7:].split(" FROM ")[0].split(","))
            expected = []
            for cells in self.expected(spec):
                check((len(expected)+1)*128 <= DECODED, "events.view_limit")
                expected.append(hashlib.sha256(key_for(cells, range(columns))).digest())
            d = meta["streams"][name]
            check(type(d) is dict and set(d) == {"query", "sha256", "rows", "bytes"} and d["query"] == spec["query"], "events.query")
            check(type(d["rows"]) is int and 0 <= d["rows"] <= MAX_ROWS
                  and type(d["bytes"]) is int and 0 <= d["bytes"] <= MAX_RAW, "events.hash")
            seen, h = [], hashlib.sha256()
            with _open(root / (name + ".rows")) as handle:
                before = os.fstat(handle.fileno())
                for ordinal in range(len(expected)):
                    size_raw = handle.read(4)
                    check(len(size_raw) == 4, "events.exhausted")
                    size, = struct.unpack("<I", size_raw)
                    check(16 <= size <= FRAME, "catalog.frame_limit")
                    frame = size_raw + handle.read(size-4)
                    check(len(frame) == size, "catalog.truncated")
                    cells = decode_row(frame, ordinal, columns)
                    h.update(frame)
                    seen.append(hashlib.sha256(key_for(cells, range(columns))).digest())
                check(not handle.read(1), "events.trailing")
                check(Catalog._identity(before) == Catalog._identity(os.fstat(handle.fileno())), "catalog.changed")
            check(d["rows"] == len(seen) and d["bytes"] == before.st_size and d["sha256"] == h.hexdigest(), "events.hash")
            expected.sort()
            seen.sort()
            check(seen == expected, "events.view_mismatch")
            reports[name] = {"source": spec["source"], "query": spec["query"], "rows": len(seen),
                             "occurrence_sha256": h.hexdigest(), "multiset_sha256": hashlib.sha256(b"".join(seen)).hexdigest()}
        return reports

    def report(self, occurrences, state, records):
        records = Path(records)
        records.mkdir()
        families = {}
        for table in TABLES:
            raw, count = hashlib.sha256(), 0
            for ordinal, row in self.rows(table):
                raw.update(encode_row(ordinal, list(row.values())))
                count += 1
            families[table] = {"rows": count, "raw_sha256": raw.hexdigest()}
        plans = self.plans(records)
        result = {"format": "benilla-event-views-v1", "catalog_sha256": self.catalog.manifest["catalog_sha256"],
                  "families": families, "loaders": occurrences, "initial_state": state, "plans": plans,
                  "weather_calendar": {"source": "src/game/Weather/Weather.cpp:112",
                                       "season_by_yday": [((day-78+365)//91)%4 for day in range(366)]},
                  "complete": False, "refusals": ["events.reference_execution_missing", "events.calendar_execution_missing",
                  "events.pool_execution_missing", "events.gameplay_not_admitted"]}
        result["view_sha256"] = digest(result)
        return result

    def plans(self, records):
        dependencies = {"creature":"guid", "gameobject":"guid", "creature_template":"entry",
                        "creature_equip_template":"entry", "quest_template":"entry", "spell_template":"id"}
        charge = sum(self.catalog.relations[t]["rows"]*128 for t in dependencies)
        charge += self.catalog.relations["game_event"]["rows"]*256
        check(charge <= DECODED, "events.view_limit")
        ids = {t: {uint(r[c]) for _, r in self.rows(t)} for t, c in dependencies.items()}
        dbcs = self.catalog._json(self.catalog.object(self.catalog.manifest["dbc_root"]))
        check("MailTemplate.dbc" in dbcs, "events.mail_dbc")
        blob = self.catalog.object(dbcs["MailTemplate.dbc"])
        check(len(blob) >= 20 and blob[:4] == b"WDBC", "events.mail_dbc")
        count, fields, size, strings = struct.unpack("<IIII", blob[4:20])
        check(fields == 10 and size == 40 and count <= 100000
              and len(blob) == 20+count*size+strings and strings > 0, "events.mail_dbc")
        ids["mail_template"] = {struct.unpack_from("<I", blob, 20+i*size)[0] for i in range(count)}
        events = {uint(r["entry"]) & 65535: (uint(r["schedule_type"]), uint(r["length"]))
                  for _, r in self.rows("game_event")}
        def valid(event):
            return event in events and (events[event][0] == 0 or events[event][1] > 0)
        result = {}
        for table in TABLES:
            h, count, size, diagnostics = hashlib.sha256(), 0, 0, {}
            with (Path(records)/(table+".jsonl")).open("xb") as output:
                for ordinal, row in self.rows(table):
                    record = self.disposition(table, ordinal, row, ids, events, valid)
                    for flag in record["diagnostics"]:
                        diagnostics[flag] = diagnostics.get(flag, 0)+1
                    raw = canonical(record)+b"\n"
                    check(len(raw) <= FRAME and size+len(raw) <= MAX_RAW, "events.byte_limit")
                    output.write(raw)
                    h.update(raw)
                    size += len(raw)
                    count += 1
                output.flush()
                os.fsync(output.fileno())
            result[table] = {"rows":count, "bytes":size, "sha256":h.hexdigest(), "diagnostics":diagnostics}
        return result

    @staticmethod
    def disposition(table, ordinal, row, ids, events, valid):
        values = {k:uint(v) for k,v in row.items() if k not in ("description", "start_time", "end_time")}
        flags = []
        def flag(condition, name):
            if condition:
                flags.append(name)
        if table == "game_event":
            event = values["entry"] & 65535
            flag(event == 0, "reserved_event")
            flag(values["linkedto"] != 0 and not valid(values["linkedto"]), "invalid_link_cleared")
            flag(values["schedule_type"] not in (0,1,2,3,5,6,8,9,11,12,13), "unknown_schedule")
            flag(values["occurence"] == 0, "disabled_occurrence")
            flag(values["length"] == 0 and values["schedule_type"] != 0, "invalid_length")
            flag(values["occurence"] != 0 and values["occurence"] < values["length"], "invalid_occurrence")
            if "disabled_occurrence" in flags:
                values["occurence"] = values["length"]
            if "invalid_link_cleared" in flags:
                values["linkedto"] = 0
        elif table == "game_event_time":
            event = values["entry"] & 65535
            flag(event not in events, "missing_event")
            flag(event in events and events[event][0] not in (1,11), "calendar_ignored")
            values["start_time_hex"] = None if row["start_time"] is None else row["start_time"].hex()
            values["end_time_hex"] = None if row["end_time"] is None else row["end_time"].hex()
        elif table == "game_weather":
            for key, value in list(values.items()):
                if key != "zone" and value > 100:
                    flags.append(key+"_reset")
                    values[key] = 25
            values["seasons"] = [[values[s+"_"+w+"_chance"] for w in ("rain","snow","storm")]
                                 for s in ("spring","summer","fall","winter")]
        else:
            event = values["event"] & 65535
            signed = event-65536 if event >= 32768 else event
            membership = table in ("game_event_creature", "game_event_gameobject")
            checked = abs(signed) if membership else event
            flag(checked == 0, "reserved_event")
            flag(not valid(checked), "missing_event")
            values["event"] = signed if membership else event
            if membership:
                values["active_when"] = "event_inactive" if signed < 0 else "event_active"
                target = "creature" if table.endswith("_creature") else "gameobject"
                flag(values["guid"] not in ids[target], "missing_spawn")
            elif table == "game_event_creature_data":
                flag(values["guid"] not in ids["creature"], "missing_spawn")
                for column, target in (("equipment_id","creature_equip_template"),("entry_id","creature_template"),
                                       ("spell_start","spell_template"),("spell_end","spell_template")):
                    if values[column] and values[column] not in ids[target]:
                        flags.append(column+"_cleared")
                        values[column] = 0
            elif table == "game_event_quest":
                flag(values["quest"] not in ids["quest_template"], "missing_quest")
            elif table == "game_event_mail":
                flag(values["racemask"] & 0xff == 0, "invalid_race_mask")
                flag(values["quest"] != 0 and values["quest"] not in ids["quest_template"], "missing_quest")
                flag(values["senderentry"] not in ids["creature_template"], "missing_sender")
                flag(values["mailtemplateid"] not in ids["mail_template"], "missing_mail_template")
        rejected = {"reserved_event", "missing_event", "missing_spawn", "missing_quest",
                    "missing_sender", "missing_mail_template", "invalid_race_mask", "calendar_ignored"}
        flags.sort()
        return {"ordinal":ordinal, "values":values, "diagnostics":flags,
                "accepted":not any(f in rejected for f in flags)}
