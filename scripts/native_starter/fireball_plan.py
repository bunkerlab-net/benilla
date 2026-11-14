"""Exact captured Fireball shape plan, never learning or world admission.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from __future__ import annotations
from classic_export import spell_numeric
from classic_export.core import digest
from .dbc import binary32_bits

ADAPTER = "captured_fireball_133_existing_spell_authority_v1"
REASONS = {
    "spell.multiple_or_missing_effects", "spell.aura_or_trigger_dependency",
    "spell.effect_execution_required", "spell.target_selector_execution_required",
    "spell.attribute_bits_execution_required", "spell.projectile_timing_execution_required",
}
EXACT_WORDS = {0: 133, 1: 2, 2: 0, 4: 0, 5: 0, 6: 65536, 7: 0, 8: 0, 9: 0, 10: 0, 11: 0, 12: 0, 13: 0, 14: 0, 15: 0, 16: 0, 17: 0, 18: 16, 19: 0, 20: 0, 21: 15, 22: 0, 23: 0, 24: 0, 25: 101, 26: 0, 27: 5, 28: 1, 29: 1, 30: 35, 31: 0, 32: 30, 33: 0, 34: 0, 35: 0, 36: 35, 37: 1103101952, 39: 0, 40: 0, 41: 0, 42: 0, 43: 0, 44: 0, 45: 0, 46: 0, 47: 0, 48: 0, 49: 0, 50: 0, 51: 0, 52: 0, 53: 0, 54: 0, 55: 0, 56: 0, 57: 0, 58: 4294967295, 59: 4294967295, 60: 0, 61: 2, 62: 6, 63: 0, 64: 9, 65: 1, 66: 0, 67: 1, 68: 1, 69: 0, 70: 0, 71: 0, 72: 0, 73: 1058642330, 74: 0, 75: 0, 76: 13, 77: 0, 78: 0, 79: 0, 80: 0, 81: 0, 82: 6, 83: 6, 84: 0, 85: 0, 86: 0, 87: 0, 88: 0, 89: 0, 90: 0, 91: 0, 92: 3, 93: 0, 94: 0, 95: 2000, 96: 0, 97: 0, 98: 0, 99: 0, 100: 0, 101: 0, 102: 0, 103: 0, 104: 0, 105: 0, 106: 0, 107: 0, 108: 0, 109: 0, 110: 0, 111: 0, 112: 0, 113: 0, 114: 0, 156: 0, 157: 133, 158: 1500, 159: 0, 160: 3, 161: 1073741825, 162: 0, 163: 0, 164: 1, 165: 1, 167: 1065353216, 168: 1065353216, 169: 1065353216, 170: 0, 171: 0, 172: 0}


def projection(row):
    words = {}
    for name, index in spell_numeric.WORD_FIELDS.items():
        value = row[name]
        words[index] = binary32_bits(value["decimal"]) if name in spell_numeric.FLOAT_FIELDS else value & 0xffffffff
    words[161] = row["SpellFamilyFlags"] & 0xffffffff
    words[162] = row["SpellFamilyFlags"] >> 32
    return words


def supported_plan(bundle, entry, dbc_words, cast, range_row, duration):
    if entry != 133:
        return None
    scope = spell_numeric.scope(bundle, entry)
    plan = {"adapter": ADAPTER, "status": "blocked", "effective_learning_proven": False,
            "runtime_export": None, "numeric_capture": scope, "refusals": []}
    if scope is None:
        plan["refusals"].append("spell_numeric.complete_effective_overlay_unknown")
        return plan
    row = scope["row"]
    words = projection(row)
    mismatches = [name for name, index in spell_numeric.WORD_FIELDS.items()
                  if dbc_words is not None and words[index] != dbc_words[index]
                  and not (name in spell_numeric.FLOAT_FIELDS and words[index] & 0x7fffffff == 0
                           and dbc_words[index] & 0x7fffffff == 0)]
    if dbc_words is not None and any(words[i] != dbc_words[i] for i in (161, 162)):
        mismatches.append("SpellFamilyFlags")
    plan["server_client_differences"] = mismatches
    if dbc_words is None or mismatches:
        plan["refusals"].append("spell.server_client_field_mismatch" if mismatches else "Spell.133.missing")
    names = {index: name for name, index in spell_numeric.WORD_FIELDS.items()}
    names.update({161: "SpellFamilyFlags.low", 162: "SpellFamilyFlags.high"})
    plan["shape_differences"] = [
        {"table": "spell_template", "key": [133], "field": names[i],
         "word": i, "observed": words[i], "required": expected}
        for i, expected in sorted(EXACT_WORDS.items()) if words[i] != expected]
    if plan["shape_differences"]:
        plan["refusals"].append("spell_numeric.exact_shape")
    coefficients = [binary32_bits(row[f"{name}{i}"]["decimal"])
                    for name in ("EffectBonusCoefficient", "EffectBonusCoefficientFromAP") for i in range(1, 4)]
    coefficient_names = [f"{name}{i}" for name in ("EffectBonusCoefficient", "EffectBonusCoefficientFromAP")
                         for i in range(1, 4)]
    expected_coefficients = [0x3dfbe76d, 0, 0, 0, 0, 0]
    plan["server_differences"] = [
        {"field": name, "observed_binary32": actual, "required_binary32": expected}
        for name, actual, expected in zip(coefficient_names, coefficients, expected_coefficients)
        if actual != expected]
    plan["server_differences"].extend(
        {"field": name, "observed": row[name], "required": 0}
        for name in ("IsServerSide", "AttributesServerside") if row[name] != 0)
    if plan["server_differences"]:
        plan["refusals"].append("spell_numeric.server_shape")
    if (cast != {"id": 16, "base_ms": 1500, "per_level_ms": 0, "minimum_ms": 1500}
            or range_row != {"id": 35, "minimum_binary32": 0, "maximum_binary32": 0x420c0000, "flags": 0}
            or duration != {"id": 35, "base_ms": 4000, "per_level_ms": 0, "maximum_ms": 4000}):
        plan["refusals"].append("spell_numeric.auxiliary_shape")
    if any(row["facingcasterflag"] not in (0, 1) for row in scope["facing"]):
        plan["refusals"].append("spell_numeric.facing_bits")
    if not plan["refusals"]:
        plan["status"] = "supported_shape"
        plan["input_bindings"] = {"numeric_capture_sha256": scope["coverage_sha256"],
                                  "Spell.133.sha256": digest(dbc_words),
                                  "cast_time": cast, "range": range_row, "duration": duration}
    return plan


def discharges(issue, plan):
    if not plan or plan["status"] != "supported_shape" or issue["table"] != "spell_template":
        return False
    if issue["key"] not in ([133], "133") or issue["reason"] not in REASONS:
        return False
    reason = issue["reason"]
    if reason in {"spell.effect_execution_required", "spell.target_selector_execution_required"}:
        return issue.get("slot") in (1, 2)
    if reason == "spell.attribute_bits_execution_required":
        return issue.get("field") == "Attributes" and issue.get("bits") == 65536
    return True
