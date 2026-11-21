"""Bounded effective SpellMgr skill-training lookups, never client inference.

CMaNGOS 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f:
src/game/Spells/SpellMgr.cpp:1625-1656; Entities/Player.cpp:5642-5698.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from dataclasses import dataclass
from .core import check, digest, integer, canonical

FORMAT = "benilla-classic-skill-training-selection-v1"
MAX_PROBES = 4096
FIELDS = ["Id"] + [f"{field}{slot}" for slot in range(1, 4)
                    for field in ("Effect", "EffectMiscValue", "EffectBasePoints")]


@dataclass(frozen=True)
class Query:
    ids: tuple[int, ...]
    limit: int = MAX_PROBES

    def sql(self):
        validate_ids(list(self.ids))
        check(bool(self.ids) and self.limit == MAX_PROBES, "skill_training.query_limit")
        columns = ",".join(f"`{name}` AS `{name}`" for name in FIELDS)
        return (f"SELECT {columns} FROM `spell_template` WHERE `Id` IN "
                f"({','.join(['%s'] * len(self.ids))}) ORDER BY `Id` LIMIT %s",
                self.ids + (MAX_PROBES + 1,))


def validate_ids(ids):
    check(type(ids) is list and len(ids) <= MAX_PROBES, "skill_training.probe_limit")
    for entry in ids:
        integer(entry, 1, 0xffffffff, "skill_training.spell_id")
    check(ids == sorted(set(ids)), "skill_training.probe_order")


def normalize(rows):
    check(type(rows) is list and len(rows) <= MAX_PROBES, "skill_training.row_limit")
    for row in rows:
        check(type(row) is dict and set(row) == set(FIELDS), "skill_training.row_fields")
        for name in FIELDS:
            integer(row[name], -0x80000000 if name.startswith(("EffectMiscValue", "EffectBasePoints")) else 0,
                    0x7fffffff if name.startswith(("EffectMiscValue", "EffectBasePoints")) else 0xffffffff,
                    "skill_training.field_domain")
    validate_ids([row["Id"] for row in rows])
    return rows


def training(row):
    """First valid SPELL_EFFECT_SKILL, not SkillStep. Values narrow to uint16 in core."""
    for slot in range(1, 4):
        skill, step = row[f"EffectMiscValue{slot}"], row[f"EffectBasePoints{slot}"] + 1
        if row[f"Effect{slot}"] == 118 and skill and step:
            return skill & 0xffff, step & 0xffff
    return None


def evidence(bundle, ids, rows, dbc_sha256):
    validate_ids(ids)
    normalize(rows)
    check({r["Id"] for r in rows} <= set(ids), "skill_training.unselected_row")
    return {"format": FORMAT, "spell_ids": ids, "rows": rows, "dbc_sha256": dbc_sha256,
            "binding": {"selection_sha256": digest(bundle["selection"]),
                        "schema_sha256": bundle["schema_sha256"],
                        "table_sha256": digest(bundle["table_sha256"]),
                        "db_version_sha256": digest(bundle["db_version"]),
                        "consistency": bundle["consistency"],
                        "rows_sha256": digest(rows), "selectors_sha256": digest(ids)}}


def validate(bundle):
    value = bundle["skill_training_capture"]
    check(type(value) is dict, "skill_training.extension")
    hashes = value.get("dbc_sha256")
    check(type(hashes) is dict and 0 < len(hashes) <= 32, "skill_training.dbc_hashes")
    for name, sha in hashes.items():
        check(type(name) is str and name.isascii() and name.isalnum() and len(name) <= 64,
              "skill_training.dbc_name")
        check(type(sha) is str and len(sha) == 64 and all(c in "0123456789abcdef" for c in sha),
              "skill_training.dbc_hash")
    check(canonical(value) == canonical(evidence(bundle, value.get("spell_ids"), value.get("rows"), hashes)),
          "skill_training.coverage_mismatch")
    known = {r["Id"]: r for r in bundle["tables"]["spell_template"]}
    for row in value["rows"]:
        if row["Id"] in known:
            check(all(known[row["Id"]][name] == row[name] for name in FIELDS),
                  "skill_training.effective_row_mismatch")


def capture(source, bundle, ids, dbc_sha256):
    """Call inside the caller's read-only snapshot, preserving its provenance limits."""
    rows = normalize(source.fetch(Query(tuple(ids)))) if ids else []
    result = dict(bundle)
    result["skill_training_capture"] = evidence(bundle, ids, rows, dbc_sha256)
    validate(result)
    return result


def recapture(source, bundle, dbcs, request, quiescent=False):
    """Extend an unchanged operator-quiescent capture without promoting probe rows to roots."""
    from .capture import validate_bundle
    from native_starter.compiler import Compiler
    from native_starter.learning import discover
    validate_bundle(bundle)
    check(quiescent and bundle["consistency"] == "operator_quiescent_snapshot",
          "skill_training.unchanged_quiescent_snapshot_required")
    inventory = source.inspect()
    check(inventory["tables"]["spell_template"] == bundle["schema"]["tables"]["spell_template"],
          "skill_training.schema_changed")
    consistency = source.start_read({inventory["tables"]["spell_template"]["engine"]}, quiescent)
    try:
        check(source.version_notes() == bundle["db_version"], "skill_training.db_version_changed")
        check(consistency == bundle["consistency"], "skill_training.consistency_changed")
        hashes = {name: table.sha256 for name, table in sorted(dbcs.items())}
        result = dict(bundle)
        result.pop("skill_training_capture", None)
        for _ in range(MAX_PROBES + 1):
            compiler = Compiler(result, dbcs, request)
            compiler.outfit()
            closure = discover(compiler)
            ids = closure["training_probe_ids"]
            previous = result.get("skill_training_capture")
            if previous is not None and ids == previous["spell_ids"]:
                validate_bundle(result)
                return result
            result = capture(source, bundle, ids, hashes)
        check(False, "skill_training.fixed_point_limit")
    finally:
        source.finish_read()
