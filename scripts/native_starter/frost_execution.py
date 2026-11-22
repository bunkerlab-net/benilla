"""Frost Armor execution through the bounded native spell authority, not learning.
SPDX-License-Identifier: GPL-3.0-or-later

CMaNGOS Classic 8ec338a1 SpellAuras.cpp:3367-3386,3934-3941,
UnitAuraProcHandler.cpp:343-406 and Unit.cpp:8337-8431,10474-10490.
"""
from classic_export.core import digest
from .dbc import binary32_bits, signed

ADAPTER = "captured_frost_armor_existing_spell_authority_v1"


def refusals(spell):
    entry = spell["id"]
    armor = entry == 168
    scope, w = spell["numeric_capture"], spell["server_numeric_words"]
    if scope is None or w is None:
        return ["complete_effective_numeric_inputs_required"]
    reasons = []
    r = scope["row"]
    registry, hooks = spell["script_registry"], spell["initial_hooks"]
    if spell["server_client_differences"]:
        reasons.append("server_client_mismatch")
    if registry is None or registry["rows"]:
        reasons.append("selected_registry_absence_required")
    if hooks is None or hooks["spell_area"] or hooks["spell_proc_event"]:
        reasons.append("selected_hook_absence_required")
    expected = {0: entry, 1: 4, 4: 1, 5: 0 if armor else 11,
        6: 0x10000 if armor else 0, 7: 0 if armor else 0x488,
        21: 8 if armor else 0, 24: 40 if armor else 0, 25: 100 if armor else 101,
        31: 0, 36: 1 if armor else 4, 58: 0xffffffff, 59: 0xffffffff,
        61: 6, 62: 6, 63: 0, 82: 1 if armor else 6, 83: 1 if armor else 6, 84: 0,
        91: 22 if armor else 138, 92: 42 if armor else 33, 93: 0,
        106: int(armor), 107: 0, 108: 0, 109: 0, 110: 6136 if armor else 0, 111: 0,
        160: 3, 161: 0x2080000 if armor else 0x100000, 162: 0, 164: 1, 165: 1}
    if any(w[i] != value for i, value in expected.items()):
        reasons.append("frost_armor_source_morphology")
    allowed = {*range(8), *range(18, 22), 24, 25, *range(27, 31), 32, 36, 38,
        58, 59, *range(61, 70), *range(76, 79), *range(82, 85), *range(91, 94),
        *range(106, 112), *range(115, 159), *range(160, 163), *range(164, 170)}
    if any(value and i not in allowed for i, value in enumerate(w)):
        reasons.append("unexecuted_frost_armor_numeric_field")
    if (r["IsServerSide"] or r["AttributesServerside"] or any(binary32_bits(r[f"EffectBonusCoefficient{s}"]["decimal"])
            or binary32_bits(r[f"EffectBonusCoefficientFromAP{s}"]["decimal"]) for s in range(1, 4))
            or any(row["facingcasterflag"] for row in scope["facing"])):
        reasons.append("frost_armor_source_modifier")
    cast, duration, spell_range = spell["cast_time"], spell["duration"], spell["range"]
    if (w[27] != 0 or w[28] > 60 or w[29] > 60 or w[32] > 1_000_000
            or armor and w[32] == 0
            or w[156] != 0 or not armor and any(w[i] for i in (32, 157, 158))
            or any(w[i] for i in (2, 19, 20)) or w[158] > 3_600_000 or w[158] and not w[157]
            or cast is None or cast["id"] != w[18] or not w[18]
            or any(cast[n] for n in ("base_ms", "per_level_ms", "minimum_ms"))
            or spell_range is None or spell_range["id"] != w[36] or spell_range["minimum_binary32"] != 0
            or spell_range["flags"] != 0 or armor and spell_range["maximum_binary32"] != 0):
        reasons.append("frost_armor_cast_and_resource_source")
    if (duration is None or duration["id"] != w[30] or not 0 < duration["base_ms"] <= 3_600_000
            or duration["per_level_ms"] or duration["maximum_ms"] != duration["base_ms"]):
        reasons.append("frost_armor_finite_duration")
    for slot in range(3):
        values = tuple(w[i+slot] for i in (64, 67, 76))
        amount = signed(values[2]) + values[1]
        if (values[0] not in (0, 1) or values[1] not in (0, 1)
                or slot == 2 and values != (0, 0, 0)
                or armor and slot == 1 and values != (0, 0, 0)
                or w[167+slot] not in (0, 0x3f800000)
                or armor and slot == 0 and not 0 <= amount <= 1_000_000
                or not armor and slot == 0 and not -1000 <= amount <= 0
                or not armor and slot == 1 and not -100 <= amount <= 0):
            reasons.append(f"frost_armor_effect_{slot+1}_amount")
    return reasons


def supported_plan(spell, spells):
    plan = {"adapter": ADAPTER, "status": "blocked", "classification": "existing_spell_authority",
        "effective_learning_proven": False, "runtime_export": None, "refusals": []}
    reasons = refusals(spell)
    if spell["id"] == 168:
        chilled = next((s for s in spells if s["id"] == 6136), None)
        if chilled is None or refusals(chilled):
            reasons.append("triggered_chilled_source_closure_required")
    else:
        armor = next((s for s in spells if s["id"] == 168), None)
        if armor is None or refusals(armor):
            reasons.append("frost_armor_proc_host_required")
    plan["refusals"] = ["spell_execution." + reason for reason in reasons]
    if not reasons:
        plan["status"] = "supported_shape"
        plan["input_bindings"] = {"numeric_capture_sha256": spell["numeric_capture"]["coverage_sha256"],
            "effective_words_sha256": digest(spell["server_numeric_words"]),
            "registry_sha256": spell["script_registry"]["coverage_sha256"],
            "initial_hooks_sha256": spell["initial_hooks"]["coverage_sha256"]}
    return plan
