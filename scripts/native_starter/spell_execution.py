"""Captured execution shapes, not effective learning or production admission.
SPDX-License-Identifier: GPL-3.0-or-later

The initial shapes mirror starter_init/initial_cast_phases::classify. Its source
ports execute targetless initial effects and permanent self aura holders. Active
casts must use spell_authority instead; an opcode match is never sufficient.
"""
from __future__ import annotations

from classic_export.core import digest
from .dbc import binary32_bits, signed

ADAPTER = "captured_initial_spell_existing_phase_handlers_v1"
SHAPES = {
    81: ([(20, 0, 90)], -1, 0xffffffff),
    203: ([(25, 0, 0)], 2, 8192),
    204: ([(26, 0, 0)], -1, 0),
    227: ([(25, 0, 0), (60, 0, 0)], 2, 1024),
    522: ([(37, 0, 0)], -1, 0xffffffff),
    668: ([(39, 0, 7)], -1, 0xffffffff),
    2382: ([(25, 0, 0), (60, 0, 0)], 2, 16384),
    3050: ([(49, 0, 0)], -1, 0xffffffff),
    5009: ([(25, 0, 0), (60, 0, 0)], 2, 524288),
    9078: ([(60, 0, 0)], 4, 2),
    9125: ([(60, 0, 0)], 4, 1),
    20597: ([(6, 98, 43), (6, 98, 55)], -1, 0),
    20598: ([(6, 137, 4)], -1, 0),
    20599: ([(6, 156, 0)], -1, 0),
    20864: ([(6, 98, 54), (6, 98, 160)], -1, 0),
}
REASONS = {"spell.native_effect_target_scaling_rules_required",
           "spell.multiple_or_missing_effects", "spell.aura_or_trigger_dependency",
           "spell.effect_execution_required", "spell.target_selector_execution_required",
           "spell.attribute_bits_execution_required"}
EXTENSIONS = {8690: "item_use_teleport_and_home_bind_lifecycle"}
ONE = 0x3f800000


def supported_plan(bundle, spell, spells):
    entry = spell["id"]
    if entry == 6603:
        from . import attack_execution
        return attack_execution.supported_plan(bundle, spell)
    if entry in (430, 433):
        from . import consumable_execution
        return consumable_execution.supported_plan(bundle, spell)
    if entry in (168, 6136):
        from . import frost_execution
        return frost_execution.supported_plan(spell, spells)
    plan = {"adapter": ADAPTER, "status": "blocked", "classification": "out_of_scope",
            "effective_learning_proven": False, "runtime_export": None, "refusals": []}
    if entry == 133:
        plan["classification"] = "existing_spell_authority"
        plan["refusals"] = ["spell_execution.use_captured_fireball_plan"]
        return plan
    if entry not in SHAPES:
        plan["classification"] = "bounded_extension" if entry in EXTENSIONS else "out_of_scope"
        plan["refusals"] = ["spell_execution." + EXTENSIONS.get(entry, "handler_not_implemented")]
        return plan
    plan["classification"] = "existing_initial_phase_handlers"
    scope = spell["numeric_capture"]
    w = spell["server_numeric_words"]
    def refuse(reason):
        plan["refusals"].append("spell_execution." + reason)
    if scope is None or w is None:
        refuse("complete_effective_numeric_inputs_required")
        return plan
    r = scope["row"]
    if spell["server_client_differences"]:
        refuse("server_client_mismatch")
    registry, hooks = spell["script_registry"], spell["initial_hooks"]
    if registry is None or registry["rows"]:
        refuse("selected_registry_absence_required")
    if hooks is None or hooks["spell_area"] or hooks["spell_proc_event"]:
        refuse("selected_hook_absence_required")
    if ("learning_capture" not in bundle
            or any(entry in (row["spell_id"], row["prev_spell"], row["first_spell"], row["req_spell"])
                   for row in bundle["tables"].get("spell_chain", []))
            or any(entry in (row["entry"], row["SpellID"])
                   for row in bundle["tables"].get("spell_learn_spell", []))):
        refuse("neutral_learning_rank_scope_required")
    if any(row["facingcasterflag"] for row in scope["facing"]):
        refuse("facing_predicate_not_initial")
    effects, item_class, item_mask = SHAPES[entry]
    holder = effects[0][0] == 6
    weapon = entry in {81, 203, 204, 227, 522, 668, 2382, 5009}
    expected = {0: entry, 1: 0, 6: 0x50 if holder or entry == 81 else 0xc0,
                7: 0x02000000 if entry == 3050 else 0, 25: 101, 36: 1,
                58: item_class & 0xffffffff, 59: item_mask,
                164: int(weapon), 165: int(weapon), 166: 0 if holder else 0xffffffff}
    if any(w[i] != value for i, value in expected.items()):
        refuse("initial_morphology")
    allowed = {0, 3, 6, 7, 18, 25, 27, 28, 29, 30, 36, 38, 58, 59,
               *range(61, 70), *range(73, 79), *range(82, 85), *range(91, 94),
               *range(97, 100), *range(106, 109), *range(115, 156), *range(164, 170)}
    if any(value and i not in allowed for i, value in enumerate(w)):
        refuse("unexecuted_numeric_field")
    cast, duration = spell["cast_time"], spell["duration"]
    # SpellMgr.cpp:56-73 returns zero for a missing indexed duration row.
    if duration is None and w[30] == 0:
        duration = {"id": 0, "base_ms": 0, "per_level_ms": 0, "maximum_ms": 0}
    if (cast is None or cast["id"] != w[18] or any(cast[n] for n in ("base_ms", "per_level_ms", "minimum_ms"))
            or duration is None or duration["id"] != w[30] or duration["base_ms"] not in (-1, 0)
            or duration["per_level_ms"] != 0 or duration["maximum_ms"] != duration["base_ms"]
            or holder and not (duration["base_ms"] == -1 or w[30] == 0)):
        refuse("initial_auxiliary_resolution")
    if spell["range"] != {"id": 1, "minimum_binary32": 0, "maximum_binary32": 0, "flags": 0}:
        refuse("initial_self_range")
    if r["IsServerSide"] or r["AttributesServerside"] or any(
            binary32_bits(r[f"EffectBonusCoefficientFromAP{s}"]["decimal"]) for s in range(1, 4)):
        refuse("server_fields")
    for slot in range(3):
        effect, aura, misc = effects[slot] if slot < len(effects) else (0, 0, 0)
        coefficient = ONE if (holder or entry == 81) and effect else 0
        if ((w[61+slot], w[91+slot], signed(w[106+slot])) != (effect, aura, misc)
                or w[82+slot] != int(holder and effect != 0)
                or w[97+slot] != (ONE if entry == 81 and slot == 0 else 0)
                or w[167+slot] != (ONE if not holder or effect else 0)
                or binary32_bits(r[f"EffectBonusCoefficient{slot+1}"]["decimal"]) != coefficient):
            refuse(f"effect_{slot+1}_morphology")
        dice = w[67+slot]
        if dice > 0x7fffffff or not -0x80000000 <= signed(w[76+slot]) + dice <= 0x7fffffff:
            refuse(f"effect_{slot+1}_cached_amount")
        values = tuple(w[b+slot] for b in (64, 67, 73, 76))
        if not effect:
            valid = values == (0, 0, 0, 0)
        elif entry == 3050:
            valid = values == (1, 1, 0x40a00000, 0xffffffff)
        elif entry == 81:
            valid = values == (6, 0, 0, 0)
        else:
            amount = signed(w[76+slot]) + dice
            valid = values[0] in (0, 1) and values[2] == 0 and (aura != 98 or -32768 <= amount <= 32767)
            valid = valid and (not holder or abs(amount) <= 1_000_000)
        if not valid:
            refuse(f"effect_{slot+1}_amount")
    if not plan["refusals"]:
        plan["status"] = "supported_shape"
        plan["input_bindings"] = {"numeric_capture_sha256": scope["coverage_sha256"],
                                  "effective_words_sha256": digest(w),
                                  "registry_sha256": registry["coverage_sha256"],
                                  "initial_hooks_sha256": hooks["coverage_sha256"]}
    return plan


def discharges(issue, spells):
    if issue["table"] == "spell_scripts":
        if issue["reason"] not in {
                "spell.registry_row_requires_named_handler_semantics", "runtime_script_required"}:
            return False
        return any(issue["key"] in ([433], "433") and s["id"] == 433
                   and s["execution_plan"]["status"] == "supported_shape"
                   and s["execution_plan"]["adapter"] ==
                   "captured_food_drink_existing_item_spell_authority_v1" for s in spells)
    if issue["table"] != "spell_template" or issue["reason"] not in REASONS:
        return False
    from . import attack_execution
    return any(issue["key"] in ([s["id"]], str(s["id"]))
               and s["execution_plan"]["status"] == "supported_shape"
               and (s["id"] != 6603 or issue["reason"] in attack_execution.REASONS)
               for s in spells)
