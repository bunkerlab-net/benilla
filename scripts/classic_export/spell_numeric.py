"""Complete selected effective SQL numeric capture, never authority.
SPDX-License-Identifier: GPL-3.0-or-later
CMaNGOS 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f:
src/game/Server/SQLStorages.cpp:40,55, SpellEntryfmt/sSpellTemplate;
src/game/Server/DBCStructure.h:716-814, SpellEntry;
src/game/Spells/SpellMgr.cpp:2993-3027, SpellMgr::LoadFacingCasterFlags.
"""
from __future__ import annotations
from typing import Any
from .core import MAX_SPELLS, canonical, check, digest, integer, normalize_cell, sql_type

FORMAT = "benilla-classic-spell-numeric-selection-v1"
MODE = "spell_numeric_selectors"
WORD_FIELDS = {'Id': 0, 'School': 1, 'Category': 2, 'Dispel': 4, 'Mechanic': 5, 'Attributes': 6, 'AttributesEx': 7, 'AttributesEx2': 8, 'AttributesEx3': 9, 'AttributesEx4': 10, 'Stances': 11, 'StancesNot': 12, 'Targets': 13, 'TargetCreatureType': 14, 'RequiresSpellFocus': 15, 'CasterAuraState': 16, 'TargetAuraState': 17, 'CastingTimeIndex': 18, 'RecoveryTime': 19, 'CategoryRecoveryTime': 20, 'InterruptFlags': 21, 'AuraInterruptFlags': 22, 'ChannelInterruptFlags': 23, 'procFlags': 24, 'procChance': 25, 'procCharges': 26, 'maxLevel': 27, 'baseLevel': 28, 'spellLevel': 29, 'DurationIndex': 30, 'powerType': 31, 'manaCost': 32, 'manaCostPerlevel': 33, 'manaPerSecond': 34, 'manaPerSecondPerLevel': 35, 'rangeIndex': 36, 'speed': 37, 'StackAmount': 39, 'EquippedItemClass': 58, 'EquippedItemSubClassMask': 59, 'EquippedItemInventoryTypeMask': 60, 'ManaCostPercentage': 156, 'StartRecoveryCategory': 157, 'StartRecoveryTime': 158, 'SpellFamilyName': 160, 'DmgClass': 164, 'PreventionType': 165, 'Effect1': 61, 'Effect2': 62, 'Effect3': 63, 'EffectDieSides1': 64, 'EffectDieSides2': 65, 'EffectDieSides3': 66, 'EffectBaseDice1': 67, 'EffectBaseDice2': 68, 'EffectBaseDice3': 69, 'EffectDicePerLevel1': 70, 'EffectDicePerLevel2': 71, 'EffectDicePerLevel3': 72, 'EffectRealPointsPerLevel1': 73, 'EffectRealPointsPerLevel2': 74, 'EffectRealPointsPerLevel3': 75, 'EffectBasePoints1': 76, 'EffectBasePoints2': 77, 'EffectBasePoints3': 78, 'EffectMechanic1': 79, 'EffectMechanic2': 80, 'EffectMechanic3': 81, 'EffectImplicitTargetA1': 82, 'EffectImplicitTargetA2': 83, 'EffectImplicitTargetA3': 84, 'EffectImplicitTargetB1': 85, 'EffectImplicitTargetB2': 86, 'EffectImplicitTargetB3': 87, 'EffectRadiusIndex1': 88, 'EffectRadiusIndex2': 89, 'EffectRadiusIndex3': 90, 'EffectApplyAuraName1': 91, 'EffectApplyAuraName2': 92, 'EffectApplyAuraName3': 93, 'EffectAmplitude1': 94, 'EffectAmplitude2': 95, 'EffectAmplitude3': 96, 'EffectMultipleValue1': 97, 'EffectMultipleValue2': 98, 'EffectMultipleValue3': 99, 'EffectChainTarget1': 100, 'EffectChainTarget2': 101, 'EffectChainTarget3': 102, 'EffectItemType1': 103, 'EffectItemType2': 104, 'EffectItemType3': 105, 'EffectMiscValue1': 106, 'EffectMiscValue2': 107, 'EffectMiscValue3': 108, 'EffectTriggerSpell1': 109, 'EffectTriggerSpell2': 110, 'EffectTriggerSpell3': 111, 'EffectPointsPerComboPoint1': 112, 'EffectPointsPerComboPoint2': 113, 'EffectPointsPerComboPoint3': 114, 'CastUI': 3, 'ModalNextSpell': 38, 'Totem1': 40, 'Totem2': 41, 'Reagent1': 42, 'Reagent2': 43, 'Reagent3': 44, 'Reagent4': 45, 'Reagent5': 46, 'Reagent6': 47, 'Reagent7': 48, 'Reagent8': 49, 'ReagentCount1': 50, 'ReagentCount2': 51, 'ReagentCount3': 52, 'ReagentCount4': 53, 'ReagentCount5': 54, 'ReagentCount6': 55, 'ReagentCount7': 56, 'ReagentCount8': 57, 'SpellVisual': 115, 'SpellIconID': 117, 'ActiveIconID': 118, 'SpellPriority': 119, 'MaxTargetLevel': 159, 'MaxAffectedTargets': 163, 'StanceBarOrder': 166, 'DmgMultiplier1': 167, 'DmgMultiplier2': 168, 'DmgMultiplier3': 169, 'MinFactionId': 170, 'MinReputation': 171, 'RequiredAuraVision': 172}
FLOAT_FIELDS = ['DmgMultiplier1', 'DmgMultiplier2', 'DmgMultiplier3', 'EffectBonusCoefficient1', 'EffectBonusCoefficient2', 'EffectBonusCoefficient3', 'EffectBonusCoefficientFromAP1', 'EffectBonusCoefficientFromAP2', 'EffectBonusCoefficientFromAP3', 'EffectDicePerLevel1', 'EffectDicePerLevel2', 'EffectDicePerLevel3', 'EffectMultipleValue1', 'EffectMultipleValue2', 'EffectMultipleValue3', 'EffectPointsPerComboPoint1', 'EffectPointsPerComboPoint2', 'EffectPointsPerComboPoint3', 'EffectRealPointsPerLevel1', 'EffectRealPointsPerLevel2', 'EffectRealPointsPerLevel3', 'speed']
SIGNED_FIELDS = ['EffectBasePoints1', 'EffectBasePoints2', 'EffectBasePoints3', 'EffectDieSides1', 'EffectDieSides2', 'EffectDieSides3', 'EffectMiscValue1', 'EffectMiscValue2', 'EffectMiscValue3', 'EquippedItemClass', 'EquippedItemInventoryTypeMask', 'EquippedItemSubClassMask', 'Reagent1', 'Reagent2', 'Reagent3', 'Reagent4', 'Reagent5', 'Reagent6', 'Reagent7', 'Reagent8', 'StanceBarOrder']
COLUMNS = ['Id', 'School', 'Category', 'Dispel', 'Mechanic', 'Attributes', 'AttributesEx', 'AttributesEx2', 'AttributesEx3', 'AttributesEx4', 'Stances', 'StancesNot', 'Targets', 'TargetCreatureType', 'RequiresSpellFocus', 'CasterAuraState', 'TargetAuraState', 'CastingTimeIndex', 'RecoveryTime', 'CategoryRecoveryTime', 'InterruptFlags', 'AuraInterruptFlags', 'ChannelInterruptFlags', 'procFlags', 'procChance', 'procCharges', 'maxLevel', 'baseLevel', 'spellLevel', 'DurationIndex', 'powerType', 'manaCost', 'manaCostPerlevel', 'manaPerSecond', 'manaPerSecondPerLevel', 'rangeIndex', 'speed', 'StackAmount', 'EquippedItemClass', 'EquippedItemSubClassMask', 'EquippedItemInventoryTypeMask', 'ManaCostPercentage', 'StartRecoveryCategory', 'StartRecoveryTime', 'SpellFamilyName', 'DmgClass', 'PreventionType', 'Effect1', 'Effect2', 'Effect3', 'EffectDieSides1', 'EffectDieSides2', 'EffectDieSides3', 'EffectBaseDice1', 'EffectBaseDice2', 'EffectBaseDice3', 'EffectDicePerLevel1', 'EffectDicePerLevel2', 'EffectDicePerLevel3', 'EffectRealPointsPerLevel1', 'EffectRealPointsPerLevel2', 'EffectRealPointsPerLevel3', 'EffectBasePoints1', 'EffectBasePoints2', 'EffectBasePoints3', 'EffectMechanic1', 'EffectMechanic2', 'EffectMechanic3', 'EffectImplicitTargetA1', 'EffectImplicitTargetA2', 'EffectImplicitTargetA3', 'EffectImplicitTargetB1', 'EffectImplicitTargetB2', 'EffectImplicitTargetB3', 'EffectRadiusIndex1', 'EffectRadiusIndex2', 'EffectRadiusIndex3', 'EffectApplyAuraName1', 'EffectApplyAuraName2', 'EffectApplyAuraName3', 'EffectAmplitude1', 'EffectAmplitude2', 'EffectAmplitude3', 'EffectMultipleValue1', 'EffectMultipleValue2', 'EffectMultipleValue3', 'EffectChainTarget1', 'EffectChainTarget2', 'EffectChainTarget3', 'EffectItemType1', 'EffectItemType2', 'EffectItemType3', 'EffectMiscValue1', 'EffectMiscValue2', 'EffectMiscValue3', 'EffectTriggerSpell1', 'EffectTriggerSpell2', 'EffectTriggerSpell3', 'EffectPointsPerComboPoint1', 'EffectPointsPerComboPoint2', 'EffectPointsPerComboPoint3', 'CastUI', 'ModalNextSpell', 'Totem1', 'Totem2', 'Reagent1', 'Reagent2', 'Reagent3', 'Reagent4', 'Reagent5', 'Reagent6', 'Reagent7', 'Reagent8', 'ReagentCount1', 'ReagentCount2', 'ReagentCount3', 'ReagentCount4', 'ReagentCount5', 'ReagentCount6', 'ReagentCount7', 'ReagentCount8', 'SpellVisual', 'SpellIconID', 'ActiveIconID', 'SpellPriority', 'MaxTargetLevel', 'MaxAffectedTargets', 'StanceBarOrder', 'DmgMultiplier1', 'DmgMultiplier2', 'DmgMultiplier3', 'MinFactionId', 'MinReputation', 'RequiredAuraVision', 'SpellFamilyFlags', 'AttributesServerside', 'IsServerSide', 'EffectBonusCoefficient1', 'EffectBonusCoefficient2', 'EffectBonusCoefficient3', 'EffectBonusCoefficientFromAP1', 'EffectBonusCoefficientFromAP2', 'EffectBonusCoefficientFromAP3']
TEXT_FIELDS = ["SpellName"] + [f"SpellName{i}" for i in range(2, 9)] + [f"Rank{i}" for i in range(1, 9)]
FACING_COLUMNS = ["entry", "facingcasterflag"]


def profile(inventory: dict[str, Any], table: str) -> dict[str, Any]:
    meta = inventory["tables"].get(table)
    check(type(meta) is dict, f"spell_numeric.schema_missing:{table}")
    names = COLUMNS if table == "spell_template" else FACING_COLUMNS
    columns = {c["name"].casefold(): c for c in meta["columns"]}
    check(meta.get("primary_key") == [columns[names[0].casefold()]["name"]], "spell_numeric.primary_key")
    result = {}
    for name in names:
        check(name.casefold() in columns, f"spell_numeric.missing_column:{name}")
        column = columns[name.casefold()]
        family, _, _, unsigned = sql_type(column["type"])
        if name in FLOAT_FIELDS:
            valid = family == "float" and not unsigned
        elif name == "SpellFamilyFlags":
            valid = family == "bigint" and unsigned
        elif table == "spell_facing" and name == "facingcasterflag":
            valid = family == "tinyint" and not unsigned
        else:
            valid = family == "int" and unsigned == (name not in SIGNED_FIELDS)
        check(valid and column["nullable"] is False, f"spell_numeric.column_type:{name}")
        result[name] = column
    # Text is presentation-only in the positive mage ID domain, never executed.
    for name in TEXT_FIELDS if table == "spell_template" else []:
        check(name.casefold() in columns, f"spell_numeric.missing_text_column:{name}")
        check(sql_type(columns[name.casefold()]["type"])[0] == "text", "spell_numeric.text_type")
    return result


def sql(query) -> tuple[str, tuple[Any, ...]]:
    check(query.table in {"spell_template", "spell_facing"} and query.selection is None,
          "spell_numeric.query_table")
    check(not query.source_names, "spell_numeric.query_source_names")
    check(0 < len(query.ids) <= MAX_SPELLS and query.limit == MAX_SPELLS,
          "spell_numeric.query_limit")
    check(len(set(query.ids)) == len(query.ids), "spell_numeric.duplicate_selector")
    for value in query.ids:
        integer(value, 1, 0x7fffffff, "spell_numeric.selector_domain")
    names = COLUMNS if query.table == "spell_template" else FACING_COLUMNS
    key = names[0]
    columns = ",".join(f"`{name}` AS `{name}`" for name in names)
    return (f"SELECT {columns} FROM `{query.table}` WHERE `{key}` IN ({','.join(['%s']*len(query.ids))}) ORDER BY `{key}` LIMIT %s",
            query.ids + (query.limit + 1,))


def normalize(rows, columns):
    check(type(rows) is list and len(rows) <= MAX_SPELLS, "spell_numeric.row_limit")
    out = []
    for row in rows:
        check(type(row) is dict and set(row) == set(columns), "spell_numeric.row_fields")
        out.append({name: normalize_cell(row[name], column) for name, column in columns.items()})
    key = next(iter(columns))
    check(len({r[key] for r in out}) == len(out), "spell_numeric.duplicate_key")
    return sorted(out, key=lambda r: r[key])


def evidence(bundle, rows, facing):
    ids = sorted(r["Id"] for r in bundle["tables"]["spell_template"])
    return {"format": FORMAT, "selected_ids": ids, "fields": COLUMNS,
            "rows": rows, "facing": facing, "row_limit": MAX_SPELLS,
            "text_disposition": {name: "presentation_only_positive_mage_domain" for name in TEXT_FIELDS},
            "snapshot": {"consistency": bundle["consistency"], "schema_sha256": bundle["schema_sha256"],
                         "selection_sha256": digest(bundle["selection"]), "db_version_sha256": digest(bundle["db_version"]),
                         "legacy_rows_sha256": digest(bundle["tables"]["spell_template"]),
                         "numeric_rows_sha256": digest(rows), "facing_rows_sha256": digest(facing)}}


def validate(bundle):
    extension = bundle["spell_numeric_capture"]
    check(type(extension) is dict, "spell_numeric.extension")
    for table, key in (("spell_template", "rows"), ("spell_facing", "facing")):
        columns = profile(bundle["schema"], table)
        rows = extension.get(key)
        check(type(rows) is list and len(rows) <= MAX_SPELLS, "spell_numeric.row_limit")
        decoded = [{k: v["decimal"] if type(v) is dict and set(v) == {"decimal"} else v
                    for k, v in row.items()} for row in rows]
        check(normalize(decoded, columns) == rows, "spell_numeric.row_normalization")
    check(canonical(extension) == canonical(evidence(bundle, extension["rows"], extension["facing"])),
          "spell_numeric.coverage_mismatch")
    legacy = {r["Id"]: r for r in bundle["tables"]["spell_template"]}
    check([r["Id"] for r in extension["rows"]] == sorted(legacy), "spell_numeric.exact_selected_rows")
    for row in extension["rows"]:
        check(all(row[name] == value for name, value in legacy[row["Id"]].items()), "spell_numeric.legacy_row_mismatch")
    check(all(row["entry"] in legacy for row in extension["facing"]), "spell_numeric.unselected_facing")


def scope(bundle, spell):
    if "spell_numeric_capture" not in bundle:
        return None
    extension = bundle["spell_numeric_capture"]
    row = next((r for r in extension["rows"] if r["Id"] == spell), None)
    if row is None:
        return None
    return {"status": "complete", "row": row,
            "facing": [r for r in extension["facing"] if r["entry"] == spell],
            "coverage_sha256": digest(extension)}
