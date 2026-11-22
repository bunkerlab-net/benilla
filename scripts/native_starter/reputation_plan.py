"""SQL policy resolution only. Actual initialized aura holders remain unbound.
SPDX-License-Identifier: GPL-3.0-or-later
CMaNGOS 8ec338a1 ObjectMgr.cpp:7413-7664; Player.cpp:6304-6428.
"""
from classic_export import reputation as capture
from classic_export.core import digest
from .dbc import binary32_bits, signed

FORMAT = "cmangos-8ec338a1-reputation-policy-inputs-v1"
RANKS = ("Hated", "Hostile", "Unfriendly", "Neutral", "Friendly", "Honored", "Revered", "Exalted")


def supported_plan(source, table, rules):
    value = source.get("reputation_policy_capture")
    if value is None:
        return "require_independent_sql_resolution"
    plan = {"format": FORMAT, "status": "blocked", "capture_binding": digest(value),
            "configuration": rules["reputation"] if rules else None,
            "configuration_binding": digest(rules) if rules else None,
            "aura_binding": "requires_actual_initialized_aura_list", "consumer_certified": False,
            "policies": None, "kill_sources": [], "loader_dispositions": [], "refusals": []}
    factions = {r.words[0]: r.words for r in table.rows} if table else {}
    if table is None or value["faction_sha256"] != table.sha256 or value["faction_ids"] != sorted(factions):
        plan["refusals"].append("reputation.complete_faction_binding_required")
        return plan
    if rules is None:
        plan["refusals"].append("reputation.explicit_configuration_required")
    lookups = value["lookups"]
    rates, spills = {}, {}
    dispositions = plan["loader_dispositions"]
    for record in lookups[capture.RATE]:
        entry, row = record["id"], record["row"]
        status, result = "absent", "Absent"
        if row is not None:
            bits = [binary32_bits(row[n]["decimal"]) for n in capture.FIELDS[capture.RATE][1:]]
            negative = any(b & 0x80000000 and b & 0x7fffffff for b in bits)
            status = "skipped_negative_rate" if negative else "accepted"
            if not negative:
                result = {"Present": dict(zip(("quest", "creature", "spell"), bits))}
        rates[entry] = result
        dispositions.append({"table": capture.RATE, "id": entry, "status": status})
    for record in lookups[capture.SPILL]:
        entry, row = record["id"], record["row"]
        status, result, warnings = "absent", "Absent", []
        if row is not None:
            targets = [row[f"faction{i}"] for i in range(1, 5)]
            for i, target in enumerate(targets, 1):
                if factions[entry][18] == 0:
                    break
                if target and target not in factions:
                    warnings.append({"slot": i - 1, "reason": "missing_faction"})
                elif target and signed(factions[target][1]) < 0:
                    warnings.append({"slot": i - 1, "reason": "unlisted_faction"})
                elif target and row[f"rank_{i}"] >= 8:
                    warnings.append({"slot": i - 1, "reason": "invalid_rank"})
            status = ("skipped_no_team" if factions[entry][18] == 0 else
                      "skipped_missing_target" if any(t and t not in factions for t in targets) else "accepted")
            if status == "accepted":
                slots = []
                for i, target in enumerate(targets, 1):
                    rank = row[f"rank_{i}"]
                    if target and rank >= 8:
                        plan["refusals"].append("reputation.spillover_rank_unrepresentable")
                    slots.append({"faction": target, "rate": binary32_bits(row[f"rate_{i}"]["decimal"]),
                                  "maximum_rank": RANKS[rank] if rank < 8 else "Hated"})
                result = {"Present": slots}
        spills[entry] = result
        dispositions.append({"table": capture.SPILL, "id": entry, "status": status, "warnings": warnings})
    for record in lookups[capture.KILL]:
        entry, row = record["id"], record["row"]
        status, converted = "absent", None
        if row is not None:
            converted = {n: (v > 0 if n in {"IsTeamAward1", "IsTeamAward2", "TeamDependent"}
                             else v if n.startswith("RewOnKillRepValue") else v & 0xffffffff)
                         for n, v in row.items()}
            status = "accepted" if all(not converted[n] or converted[n] in factions for n in
                                      ("RewOnKillRepFaction1", "RewOnKillRepFaction2")) else "skipped_missing_faction"
            if status != "accepted":
                converted = None
        plan["kill_sources"].append({"entry": entry, "row": converted,
            "admission": "PresentOnKillRow" if converted is not None else "ProvenAbsentOnKillRow"})
        dispositions.append({"table": capture.KILL, "id": entry, "status": status})
    # The existing adapter rejects unsupported binary32 encodings, never clamps them.
    for result in [*rates.values(), *spills.values()]:
        if result == "Absent":
            continue
        present = result["Present"]
        words = present.values() if type(present) is dict else (s["rate"] for s in present)
        for bits in words:
            magnitude = bits & 0x7fffffff
            if magnitude > 0x49742400 or 0 < magnitude < 0x00800000:
                plan["refusals"].append("reputation.binary32_adapter_domain")
    plan["refusals"] = sorted(set(plan["refusals"]))
    if not plan["refusals"]:
        plan["status"] = "complete_inputs"
        plan["policies"] = [{"faction": entry, "reward_rates": rates[entry], "spillover": spills[entry]}
                            for entry in sorted(factions)]
    return plan
