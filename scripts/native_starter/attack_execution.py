"""Attack 6603 marks the existing mainhand operation, never a cast executor.
SPDX-License-Identifier: GPL-3.0-or-later
CMaNGOS 8ec338a1 CombatHandler.cpp:26-61; SpellEffects.cpp:136.
"""
from classic_export.core import digest
from .dbc import binary32_bits

ADAPTER = "captured_attack_6603_existing_mainhand_operation_v1"
REASONS = {"spell.attribute_bits_execution_required", "spell.effect_execution_required",
           "spell.native_effect_target_scaling_rules_required"}
EXACT = {0: 6603, 6: 0x10, 18: 1, 25: 101, 36: 1, 58: 0xffffffff,
         61: 78, 117: 1, 166: 0xffffffff, 167: 0x3f800000,
         168: 0x3f800000, 169: 0x3f800000}


def supported_plan(bundle, spell):
    plan = {"adapter": ADAPTER, "status": "blocked",
            "classification": "existing_mainhand_operation_marker",
            "effective_learning_proven": False, "runtime_export": None, "refusals": []}
    def refuse(reason):
        plan["refusals"].append("spell_execution.attack_" + reason)
    scope, words = spell["numeric_capture"], spell["server_numeric_words"]
    if scope is None or words is None:
        refuse("complete_effective_numeric_inputs_required")
        return plan
    # Localized string offsets are presentation. Every numeric operand is exact.
    if (spell["id"] != 6603 or len(words) != 173
            or any(value != EXACT.get(i, 0) for i, value in enumerate(words)
                   if not 120 <= i <= 155)):
        refuse("numeric_marker_shape")
    if spell["server_client_differences"]:
        refuse("server_client_mismatch")
    row = scope["row"]
    if (row["IsServerSide"] or row["AttributesServerside"]
            or any(binary32_bits(row[f"{field}{slot}"]["decimal"])
                   for field in ("EffectBonusCoefficient", "EffectBonusCoefficientFromAP")
                   for slot in range(1, 4))):
        refuse("server_fields")
    registry, hooks = spell["script_registry"], spell["initial_hooks"]
    if registry is None or registry["rows"]:
        refuse("selected_registry_absence_required")
    if hooks is None or hooks["spell_area"] or hooks["spell_proc_event"]:
        refuse("selected_hook_absence_required")
    if scope["facing"]:
        refuse("selected_facing_absence_required")
    if ("learning_capture" not in bundle
            or any(6603 in (r["spell_id"], r["prev_spell"], r["first_spell"], r["req_spell"])
                   for r in bundle["tables"].get("spell_chain", []))
            or any(6603 in (r["entry"], r["SpellID"])
                   for r in bundle["tables"].get("spell_learn_spell", []))):
        refuse("neutral_learning_rank_scope_required")
    if (spell["cast_time"] != {"id": 1, "base_ms": 0, "per_level_ms": 0, "minimum_ms": 0}
            or spell["range"] != {"id": 1, "minimum_binary32": 0, "maximum_binary32": 0, "flags": 0}
            or spell["duration"] not in (None, {"id": 0, "base_ms": 0, "per_level_ms": 0, "maximum_ms": 0})):
        refuse("auxiliary_resolution")
    if not plan["refusals"]:
        plan["status"] = "supported_shape"
        plan["input_bindings"] = {"numeric_capture_sha256": scope["coverage_sha256"],
            "effective_words_sha256": digest(words), "registry_sha256": registry["coverage_sha256"],
            "initial_hooks_sha256": hooks["coverage_sha256"]}
    return plan
