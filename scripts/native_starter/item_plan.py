"""Captured item projection for existing carried and starter operations only.
SPDX-License-Identifier: GPL-3.0-or-later
CMaNGOS 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f,
Globals/ObjectMgr.cpp:2557-2815, Entities/Player.cpp:857-985,8822-9195,9432-9555.
"""
from classic_export.core import ExportError, digest
from .dbc import binary32_bits
from .outfit_plan import equip_slots

ADAPTER = "benilla_sim::inventory::captured::compile_catalog"
REASON = "item.native_inventory_equipment_rules_required"
SUBCLASSES = (9, 8, 21, 1, 10, 1, 5, 14, 1, 11, 1, 4, 1, 2, 1, 6)
INVENTORY_FIELDS = {
    "entry": "entry", "class": "class", "InventoryType": "inventory_type",
    "displayid": "display_id", "Quality": "quality", "Flags": "flags",
    "ExtraFlags": "extra_flags", "stackable": "stackable", "maxcount": "maximum_count",
    "bonding": "bonding", "MaxDurability": "maximum_durability", "BagFamily": "bag_family",
    "ContainerSlots": "container_slots", "RandomProperty": "random_property",
    "startquest": "start_quest", "Duration": "duration", "itemset": "item_set",
    "area": "area", "Map": "map", "lockid": "lock_id",
}
PLACEMENT_FIELDS = {
    "entry": "id", "class": "class", "subclass": "subclass", "InventoryType": "inventory_type",
    "RequiredLevel": "required_level", "RequiredSkill": "required_skill",
    "RequiredSkillRank": "required_skill_rank", "requiredspell": "required_spell",
    "requiredhonorrank": "required_honor_rank", "RequiredReputationFaction": "required_reputation_faction",
    "Flags": "flags", "maxcount": "unique_max_count", "stackable": "stackable",
    "BagFamily": "bag_family", "ContainerSlots": "container_slots", "itemset": "item_set",
    "RandomProperty": "random_property", "Duration": "duration", "armor": "armor", "block": "block",
    "delay": "delay_ms", "MaxDurability": "max_durability",
}
SPELL_FIELDS = {"spellid": "id", "spelltrigger": "trigger", "spellcharges": "charges",
                "spellcooldown": "cooldown", "spellcategory": "category",
                "spellcategorycooldown": "category_cooldown"}


def bits(value):
    return binary32_bits(value["decimal"] if type(value) is dict else str(value))


def item_plan(item):
    """One row, precise refusals. No loader repair or inferred source dependency absence."""
    entry = item["entry"]
    refusals = []

    def refuse(field, reason):
        refusals.append({"table": "item_template", "key": str(entry), "field": field,
                         "reason": "item." + reason})

    cls, sub = item["class"], item["subclass"]
    if not 0 <= cls < 16 or not 0 <= sub < SUBCLASSES[cls]:
        refuse("class/subclass", "prototype_loader_repair_required")
    for field, low, high in (("entry", 1, 0xffffffff), ("displayid", 1, 0xffffffff),
                             ("Quality", 0, 6), ("InventoryType", 0, 28),
                             ("stackable", 1, 255), ("maxcount", 0, 0x7fffffff),
                             ("bonding", 0, 4),
                             ("MaxDurability", 0, 0xffffffff), ("RequiredSkill", 0, 762)):
        if not low <= item[field] <= high:
            refuse(field, "prototype_domain")
    for field in ("startquest", "RandomProperty", "ScriptName", "Duration", "area", "Map",
                  "BagFamily", "itemset", "lockid", "ExtraFlags"):
        if item[field]:
            refuse(field, "carried_dependency_unsupported")
    if item["Flags"] & ~0x800:
        refuse("Flags", "carried_dependency_unsupported")
    bag = cls == 1 and item["InventoryType"] == 18 and 1 <= item["ContainerSlots"] <= 36 and item["stackable"] == 1
    if cls == 11 or item["InventoryType"] == 27 or not bag and (
            cls == 1 or item["InventoryType"] == 18 or item["ContainerSlots"]):
        refuse("ContainerSlots/InventoryType", "carried_container_unsupported")
    spells = []
    for slot in range(1, 6):
        spell = {target: item[f"{source}_{slot}"] for source, target in SPELL_FIELDS.items()}
        if (any(not 0 <= spell[name] <= 0xffffffff for name in ("id", "trigger", "category"))
                or any(not -0x80000000 <= spell[name] <= 0x7fffffff
                       for name in ("charges", "cooldown", "category_cooldown"))
                or spell["trigger"] >= 6):
            refuse(f"spell_{slot}", "prototype_spell_domain")
        if spell["id"] and spell["trigger"]:
            refuse(f"spelltrigger_{slot}", "carried_equip_proc_spell_unsupported")
        try:
            spell["ppm_rate_bits"] = bits(item[f"spellppmRate_{slot}"])
        except ExportError:
            refuse(f"spellppmRate_{slot}", "prototype_spell_domain")
        spells.append(spell)
    stats = [{"kind": item[f"stat_type{i}"], "value": item[f"stat_value{i}"]} for i in range(1, 11)]
    if any(s["value"] and not 0 <= s["kind"] < 8 for s in stats):
        refuse("stats", "prototype_loader_repair_required")
    damage = []
    for slot in range(1, 6):
        try:
            damage.append({"min_bits": bits(item[f"dmg_min{slot}"]),
                           "max_bits": bits(item[f"dmg_max{slot}"]), "school": item[f"dmg_type{slot}"]})
        except ExportError:
            refuse(f"damage_{slot}", "prototype_damage_domain")
        if not 0 <= item[f"dmg_type{slot}"] < 7:
            refuse(f"dmg_type{slot}", "prototype_loader_repair_required")
    inventory = {target: item[source] for source, target in INVENTORY_FIELDS.items()}
    inventory.update(spells=spells, script_id=0)
    placement = {target: item[source] for source, target in PLACEMENT_FIELDS.items()}
    placement.update(allowable_class=item["AllowableClass"] & 0xffffffff,
                     allowable_race=item["AllowableRace"] & 0xffffffff,
                     scripted=bool(item["ScriptName"]), stats=stats, damage=damage,
                     spells=[{"id": s["id"], "trigger": s["trigger"]} for s in spells],
                     resistances=[item[f + "_res"] for f in ("holy", "fire", "nature", "frost", "shadow", "arcane")])
    starting_refusals = []
    if not 1 <= item["BuyCount"] <= 10_000:
        starting_refusals.append("item.outfit_quantity_domain")
    if equip_slots(item) is None:
        starting_refusals.append("item.starting_placement_unsupported")
    if any(item[f] for f in ("Flags", "block", "requiredhonorrank", "RequiredReputationFaction",
                             "RequiredReputationRank", "RequiredCityRank")):
        starting_refusals.append("item.starting_dependency_unsupported")
    # Skill/spell dependencies must be bound by the existing initializer, never inferred here.
    return {"entry": entry, "status": "blocked" if refusals else "supported_shape",
            "item_template_sha256": digest(item),
            "captured_input": None if refusals else {"inventory": inventory, "placement": placement,
                                                       "buy_count": item["BuyCount"]},
            "carried_operation": "empty_carried_bag" if bag else "backpack_item",
            "starter_placement": {"status": "blocked" if refusals or starting_refusals else "supported_shape",
                                  "refusals": starting_refusals,
                                  "eligibility": "initializer_resolves_actual_skills_spells_masks_level_and_capacity"},
            "unavailable_operations": {"equip_newly_looted": "item.carried_item_equipment_not_implemented",
                                       "use": "item.carried_item_use_not_implemented"},
            "refusals": refusals}


def supported_plan(source):
    items = [item_plan(row) for row in source["tables"]["item_template"]]
    return {"adapter": ADAPTER, "runtime_admitted": False, "items": items,
            "source_sha256": digest(source),
            "remaining_dependencies": ["independent_inventory_audit", "client_item_query_metadata",
                                       "actual_starter_initializer_and_genesis_equipment_custody"],
            "refusals": [r for item in items for r in item["refusals"]]}


def discharges(issue, plan):
    return (issue["table"] == "item_template" and issue["reason"] == REASON
            and any(str(item["entry"]) == str(issue["key"]) and item["status"] == "supported_shape"
                    for item in plan["items"]))
