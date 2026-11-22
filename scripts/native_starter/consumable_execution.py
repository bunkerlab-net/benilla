"""Source-bound Food/Drink execution, not learning or production admission.
SPDX-License-Identifier: GPL-3.0-or-later
CMaNGOS 8ec338a1 Player.cpp:7463-7502; Spell.cpp:4314-4375;
SpellAuras.cpp:3617-3646,4953-4990; spell_scripts.cpp:154-173.
"""
from classic_export.core import digest
from .dbc import binary32_bits

ADAPTER = "captured_food_drink_existing_item_spell_authority_v1"


def supported_plan(bundle, spell):
    entry = spell["id"]
    food = entry == 433
    plan = {"adapter": ADAPTER, "status": "blocked", "classification": "existing_spell_authority",
            "effective_learning_proven": False, "runtime_export": None, "refusals": []}
    reasons = plan["refusals"]
    def refuse(reason):
        reasons.append("spell_execution." + reason)
    scope, w = spell["numeric_capture"], spell["server_numeric_words"]
    if scope is None or w is None:
        refuse("complete_effective_numeric_inputs_required")
        return plan
    if spell["server_client_differences"]:
        refuse("server_client_mismatch")
    registry, hooks = spell["script_registry"], spell["initial_hooks"]
    scripts = [{"Id": 433, "ScriptName": "spell_food_animation"}] if food else []
    if registry is None or registry["rows"] != scripts:
        refuse("consumable_script_source_closure")
    if hooks is None or hooks["spell_area"] or hooks["spell_proc_event"]:
        refuse("selected_hook_absence_required")
    expected = {0: entry, 1: 0, 2: 11 if food else 59, 6: 0x18000100, 18: 1, 19: 0,
        20: 60000, 21: 1, 22: 0x40080, 25: 101, 27: 0, 28: 5, 29: 0 if food else 5,
        31: 0, 32: 0, 36: 1, 58: 0xffffffff, 59: 0 if food else 0xffffffff,
        61: 6, 64: 1, 67: 1, 76: 16 if food else 41, 82: 1, 91: 84 if food else 85,
        97: 0x3f800000 if food else 0, 166: 0xffffffff, 167: 0x3f800000,
        168: 0x3f800000, 169: 0x3f800000}
    allowed = {*range(3), 6, *range(18, 23), 25, *range(28, 31), 36, 58, 59,
               61, 64, 67, 76, 82, 91, 97, *range(115, 156), *range(166, 170)}
    r = scope["row"]
    cast, duration, spell_range = spell["cast_time"], spell["duration"], spell["range"]
    if (any(w[i] != value for i, value in expected.items())
            or any(value and i not in allowed for i, value in enumerate(w))
            or r["IsServerSide"] or r["AttributesServerside"]
            or any(binary32_bits(r[f"EffectBonusCoefficient{s}"]["decimal"])
                   or binary32_bits(r[f"EffectBonusCoefficientFromAP{s}"]["decimal"]) for s in range(1, 4))
            or any(row["facingcasterflag"] for row in scope["facing"])
            or cast is None or cast["id"] != 1 or any(cast[k] for k in ("base_ms", "per_level_ms", "minimum_ms"))
            or spell_range is None or spell_range["id"] != 1
            or any(spell_range[k] for k in ("minimum_binary32", "maximum_binary32", "flags"))):
        refuse("consumable_effective_source_morphology")
    if (duration is None or duration["id"] != w[30] or not 0 < duration["base_ms"] <= 3_600_000
            or duration["per_level_ms"] or duration["maximum_ms"] != duration["base_ms"]):
        refuse("consumable_finite_duration")
    item = next((row for row in bundle["tables"].get("item_template", [])
                 if row["entry"] == (2070 if food else 159)), None)
    if (item is None or item["class"] != 0 or item["subclass"] != 0 or item["stackable"] != 20
            or item["spellid_1"] != entry or item["spelltrigger_1"] != 0 or item["spellcharges_1"] != -1
            or item["spellcategory_1"] != w[2] or item["RequiredLevel"] > 1
            or item["AllowableClass"] & 128 == 0 or item["AllowableRace"] & 1 == 0
            or any(item[k] for k in ("RequiredSkill", "RequiredSkillRank", "requiredspell", "requiredhonorrank", "RequiredReputationFaction"))
            or any(item[f"spellid_{i}"] for i in range(2, 6))):
        refuse("starting_consumable_item_source")
    if not reasons:
        plan["status"] = "supported_shape"
        plan["input_bindings"] = {"numeric_capture_sha256": scope["coverage_sha256"],
            "effective_words_sha256": digest(w), "registry_sha256": registry["coverage_sha256"],
            "initial_hooks_sha256": hooks["coverage_sha256"], "item_template_sha256": digest(item)}
    return plan
