// SPDX-License-Identifier: GPL-3.0-or-later
// Independent translation of native_starter/item_plan.py, CMaNGOS 8ec338a1.
use super::*;
pub(in super::super) const ADAPTER: &str = "benilla_sim::inventory::captured::compile_catalog";
pub(in super::super) const REASON: &str = "item.native_inventory_equipment_rules_required";
pub(in super::super) const INVENTORY: &[(&str, &str)] = &[
    ("entry", "entry"),
    ("class", "class"),
    ("InventoryType", "inventory_type"),
    ("displayid", "display_id"),
    ("Quality", "quality"),
    ("Flags", "flags"),
    ("ExtraFlags", "extra_flags"),
    ("stackable", "stackable"),
    ("maxcount", "maximum_count"),
    ("bonding", "bonding"),
    ("MaxDurability", "maximum_durability"),
    ("BagFamily", "bag_family"),
    ("ContainerSlots", "container_slots"),
    ("RandomProperty", "random_property"),
    ("startquest", "start_quest"),
    ("Duration", "duration"),
    ("itemset", "item_set"),
    ("area", "area"),
    ("Map", "map"),
    ("lockid", "lock_id"),
];
const PLACEMENT: &[(&str, &str)] = &[
    ("entry", "id"),
    ("class", "class"),
    ("subclass", "subclass"),
    ("InventoryType", "inventory_type"),
    ("RequiredLevel", "required_level"),
    ("RequiredSkill", "required_skill"),
    ("RequiredSkillRank", "required_skill_rank"),
    ("requiredspell", "required_spell"),
    ("requiredhonorrank", "required_honor_rank"),
    ("RequiredReputationFaction", "required_reputation_faction"),
    ("Flags", "flags"),
    ("maxcount", "unique_max_count"),
    ("stackable", "stackable"),
    ("BagFamily", "bag_family"),
    ("ContainerSlots", "container_slots"),
    ("itemset", "item_set"),
    ("RandomProperty", "random_property"),
    ("Duration", "duration"),
    ("armor", "armor"),
    ("block", "block"),
    ("delay", "delay_ms"),
    ("MaxDurability", "max_durability"),
];
const SPELL: &[(&str, &str)] = &[
    ("spellid", "id"),
    ("spelltrigger", "trigger"),
    ("spellcharges", "charges"),
    ("spellcooldown", "cooldown"),
    ("spellcategory", "category"),
    ("spellcategorycooldown", "category_cooldown"),
];

fn project(item: &Value, fields: &[(&str, &str)]) -> Map<String, Value> {
    fields
        .iter()
        .map(|(source, target)| ((*target).into(), item[*source].clone()))
        .collect()
}
fn bits(value: &Value) -> Result<u32> {
    if let Some(decimal) = value.get("decimal").and_then(Value::as_str) {
        dbc::binary32(decimal)
    } else {
        dbc::binary32(&value.to_string())
    }
}
pub(super) fn equip_slots(item: &Value) -> Option<&'static [u32]> {
    let class = int(&item["class"]);
    let sub = int(&item["subclass"]);
    let kind = int(&item["InventoryType"]);
    let supported = class == 0 && (0..=8).contains(&sub) && kind == 0
        || class == 15 && sub == 0 && kind == 0
        || class == 4
            && (0..=4).contains(&sub)
            && ((0..13).contains(&kind) || [16, 19, 20].contains(&kind))
        || class == 2
            && [0, 1, 4, 5, 6, 7, 8, 10, 13, 15, 20].contains(&sub)
            && [0, 13, 17, 21].contains(&kind);
    if !supported {
        return None;
    }
    const SINGLE_SLOTS: [[u32; 1]; 10] = [[0], [1], [2], [3], [4], [5], [6], [7], [8], [9]];
    Some(match kind {
        0 => &[],
        1..=10 => &SINGLE_SLOTS[kind as usize - 1],
        11 => &[10, 11],
        12 => &[12, 13],
        13 | 17 | 21 => &[15],
        16 => &[14],
        19 => &[18],
        20 => &[4],
        _ => unreachable!(),
    })
}
pub(in super::super) fn item_plan(item: &Value) -> Value {
    let entry = &item["entry"];
    let mut refusals = Vec::new();
    let mut refuse = |field: &str, reason: &str| {
        refusals.push(json!({"table":"item_template","key":int(entry).to_string(),"field":field,"reason":format!("item.{reason}")}));
    };
    let class = int(&item["class"]);
    let sub = int(&item["subclass"]);
    const SUBCLASSES: [i128; 16] = [9, 8, 21, 1, 10, 1, 5, 14, 1, 11, 1, 4, 1, 2, 1, 6];
    if !(0..16).contains(&class) || !(0..SUBCLASSES[class as usize]).contains(&sub) {
        refuse("class/subclass", "prototype_loader_repair_required");
    }
    for (field, low, high) in [
        ("entry", 1, 0xffffffff),
        ("displayid", 1, 0xffffffff),
        ("Quality", 0, 6),
        ("InventoryType", 0, 28),
        ("stackable", 1, 255),
        ("maxcount", 0, 0x7fffffff),
        ("bonding", 0, 4),
        ("MaxDurability", 0, 0xffffffff),
        ("RequiredSkill", 0, 762),
    ] {
        if !(low..=high).contains(&int(&item[field])) {
            refuse(field, "prototype_domain");
        }
    }
    for field in [
        "startquest",
        "RandomProperty",
        "ScriptName",
        "Duration",
        "area",
        "Map",
        "BagFamily",
        "itemset",
        "lockid",
        "ExtraFlags",
    ] {
        if nonzero(&item[field]) {
            refuse(field, "carried_dependency_unsupported");
        }
    }
    if int(&item["Flags"]) & !0x800 != 0 {
        refuse("Flags", "carried_dependency_unsupported");
    }
    let bag = class == 1
        && int(&item["InventoryType"]) == 18
        && (1..=36).contains(&int(&item["ContainerSlots"]))
        && item["stackable"] == 1;
    if class == 11
        || item["InventoryType"] == 27
        || !bag && (class == 1 || item["InventoryType"] == 18 || nonzero(&item["ContainerSlots"]))
    {
        refuse(
            "ContainerSlots/InventoryType",
            "carried_container_unsupported",
        );
    }
    let mut spells = Vec::new();
    for slot in 1..=5 {
        let mut spell: Map<String, Value> = SPELL
            .iter()
            .map(|(source, target)| ((*target).into(), item[format!("{source}_{slot}")].clone()))
            .collect();
        if ["id", "trigger", "category"]
            .iter()
            .any(|n| !(0..=0xffffffff).contains(&int(&spell[*n])))
            || ["charges", "cooldown", "category_cooldown"]
                .iter()
                .any(|n| !(i32::MIN as i128..=i32::MAX as i128).contains(&int(&spell[*n])))
            || int(&spell["trigger"]) >= 6
        {
            refuse(&format!("spell_{slot}"), "prototype_spell_domain");
        }
        if nonzero(&spell["id"]) && nonzero(&spell["trigger"]) {
            refuse(
                &format!("spelltrigger_{slot}"),
                "carried_equip_proc_spell_unsupported",
            );
        }
        match bits(&item[format!("spellppmRate_{slot}")]) {
            Ok(bits) => {
                spell.insert("ppm_rate_bits".into(), json!(bits));
            }
            Err(_) => refuse(&format!("spellppmRate_{slot}"), "prototype_spell_domain"),
        }
        spells.push(Value::Object(spell));
    }
    let stats: Vec<_> = (1..=10).map(|i| json!({"kind":item[format!("stat_type{i}")],"value":item[format!("stat_value{i}")]})).collect();
    if stats
        .iter()
        .any(|s| nonzero(&s["value"]) && !(0..8).contains(&int(&s["kind"])))
    {
        refuse("stats", "prototype_loader_repair_required");
    }
    let mut damage = Vec::new();
    for slot in 1..=5 {
        match (
            bits(&item[format!("dmg_min{slot}")]),
            bits(&item[format!("dmg_max{slot}")]),
        ) {
            (Ok(min), Ok(max)) => damage.push(
                json!({"min_bits":min,"max_bits":max,"school":item[format!("dmg_type{slot}")]}),
            ),
            _ => refuse(&format!("damage_{slot}"), "prototype_damage_domain"),
        }
        if !(0..7).contains(&int(&item[format!("dmg_type{slot}")])) {
            refuse(
                &format!("dmg_type{slot}"),
                "prototype_loader_repair_required",
            );
        }
    }
    let mut inventory = project(item, INVENTORY);
    inventory.insert("spells".into(), json!(spells));
    inventory.insert("script_id".into(), json!(0));
    let mut placement = project(item, PLACEMENT);
    placement.extend([
        (
            "allowable_class".into(),
            num(int(&item["AllowableClass"]) & 0xffffffff),
        ),
        (
            "allowable_race".into(),
            num(int(&item["AllowableRace"]) & 0xffffffff),
        ),
        ("scripted".into(), json!(nonzero(&item["ScriptName"]))),
        ("stats".into(), json!(stats)),
        ("damage".into(), json!(damage)),
        (
            "spells".into(),
            json!(spells
                .iter()
                .map(|s| json!({"id":s["id"],"trigger":s["trigger"]}))
                .collect::<Vec<_>>()),
        ),
        (
            "resistances".into(),
            json!(["holy", "fire", "nature", "frost", "shadow", "arcane"]
                .map(|f| item[format!("{f}_res")].clone())),
        ),
    ]);
    let mut starting = Vec::new();
    if !(1..=10000).contains(&int(&item["BuyCount"])) {
        starting.push("item.outfit_quantity_domain");
    }
    if equip_slots(item).is_none() {
        starting.push("item.starting_placement_unsupported");
    }
    if [
        "Flags",
        "block",
        "requiredhonorrank",
        "RequiredReputationFaction",
        "RequiredReputationRank",
        "RequiredCityRank",
    ]
    .iter()
    .any(|f| nonzero(&item[*f]))
    {
        starting.push("item.starting_dependency_unsupported");
    }
    json!({"entry":entry,"status":if refusals.is_empty(){"supported_shape"}else{"blocked"},
        "item_template_sha256":digest(item),
        "captured_input":if refusals.is_empty(){json!({"inventory":inventory,"placement":placement,"buy_count":item["BuyCount"]})}else{Value::Null},
        "carried_operation":if bag{"empty_carried_bag"}else{"backpack_item"},
        "starter_placement":{"status":if refusals.is_empty()&&starting.is_empty(){"supported_shape"}else{"blocked"},"refusals":starting,"eligibility":"initializer_resolves_actual_skills_spells_masks_level_and_capacity"},
        "unavailable_operations":{"equip_newly_looted":"item.carried_item_equipment_not_implemented","use":"item.carried_item_use_not_implemented"},"refusals":refusals})
}
pub(in super::super) fn supported(source: &Value, source_sha: &str) -> Value {
    let items: Vec<_> = rows(source, "item_template")
        .iter()
        .map(item_plan)
        .collect();
    let refusals: Vec<_> = items
        .iter()
        .flat_map(|i| i["refusals"].as_array().unwrap().iter())
        .collect();
    json!({"adapter":ADAPTER,"runtime_admitted":false,"items":items,"source_sha256":source_sha,
        "remaining_dependencies":["independent_inventory_audit","client_item_query_metadata","actual_starter_initializer_and_genesis_equipment_custody"],"refusals":refusals})
}
pub(in super::super) fn discharges(issue: &Value, plan: &Value) -> bool {
    issue["table"] == "item_template"
        && issue["reason"] == REASON
        && plan["items"].as_array().unwrap().iter().any(|i| {
            int(&i["entry"]).to_string() == issue["key"].as_str().unwrap_or("")
                && i["status"] == "supported_shape"
        })
}
