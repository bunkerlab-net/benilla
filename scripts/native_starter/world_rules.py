"""Explicit CMaNGOS start/progression policy, never an inferred realm default.
SPDX-License-Identifier: GPL-3.0-or-later
CMaNGOS 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, World.cpp:411-427,456,548-553,653,686.
"""
import copy
import math
import struct
from classic_export.core import check, digest

PRESET = "cmangos-8ec338a1-stock-progression-v1"
ONE = 0x3f800000


def stock_preset():
    return {"version": 1, "start_player_level": 1, "start_player_money": 0,
            "progression": {"max_player_level": 60, "always_max_skill_for_level": False,
                            "global_skill_max": 300, "talent_rate_bits": ONE},
            "kill": {"kill_rate_bits": ONE}, "quest_xp_rate_bits": ONE,
            "reputation": {"gain_rate": ONE, "low_level_kill_rate": 0x3e4ccccd,
                           "low_level_quest_rate": 0x3e4ccccd},
            "loot": {"quality_rate_bits": [ONE] * 7,
                     "quest_drop_rate": {"multiplier_bits": ONE},
                     "referenced_rate_bits": ONE, "money_rate_bits": ONE,
                     "corpse_empty_loot_show": True}}


def validate(value):
    def fields(obj, expected):
        check(type(obj) is dict and set(obj) == set(expected), "world_rules.unsupported_or_missing_key")
    fields(value, stock_preset())
    check(type(value["version"]) is int and value["version"] == 1, "world_rules.version")
    check(type(value["start_player_level"]) is int and value["start_player_level"] == 1,
          "world_rules.StartPlayerLevel_other_than_one")
    check(type(value["start_player_money"]) is int and 0 <= value["start_player_money"] <= 0x7ffffffe,
          "world_rules.StartPlayerMoney")
    p = value["progression"]
    fields(p, stock_preset()["progression"])
    check(type(p["max_player_level"]) is int and 1 <= p["max_player_level"] <= 60,
          "world_rules.MaxPlayerLevel_outside_1_60")
    check(type(p["always_max_skill_for_level"]) is bool, "world_rules.AlwaysMaxSkillForLevel")
    check(type(p["global_skill_max"]) is int and p["global_skill_max"] == p["max_player_level"] * 5,
          "world_rules.global_skill_max")
    fields(value["kill"], ["kill_rate_bits"])
    fields(value["reputation"], stock_preset()["reputation"])
    loot = value["loot"]
    fields(loot, stock_preset()["loot"])
    fields(loot["quest_drop_rate"], ["multiplier_bits"])
    check(type(loot["corpse_empty_loot_show"]) is bool, "world_rules.Corpse.EmptyLootShow")
    check(type(loot["quality_rate_bits"]) is list and len(loot["quality_rate_bits"]) == 7,
          "world_rules.quality_rate_bits")
    rates = [p["talent_rate_bits"], value["kill"]["kill_rate_bits"], value["quest_xp_rate_bits"],
             *value["reputation"].values(), *loot["quality_rate_bits"],
             loot["quest_drop_rate"]["multiplier_bits"], loot["referenced_rate_bits"], loot["money_rate_bits"]]
    for bits in rates:
        check(type(bits) is int and (bits == 0 or 0x00800000 <= bits <= 0x447a0000),
              "world_rules.rate_outside_bounded_binary32_domain")
    return copy.deepcopy(value)


def binary32(value):
    return struct.unpack("!f", struct.pack("!f", value))[0]


def supported_plan(source, value):
    if value is None:
        return {"status": "not_supplied", "rules": None, "input_binding": None,
                "quest_xp_inputs": [], "refusals": []}
    rules = validate(value)
    rate = struct.unpack("!f", struct.pack("!I", rules["quest_xp_rate_bits"]))[0]
    money_rate = struct.unpack("!f", struct.pack("!I", rules["loot"]["money_rate_bits"]))[0]
    quests, refusals = [], []
    for row in source["tables"]["quest_template"]:
        level, money = row["QuestLevel"], row["RewMoneyMaxLevel"]
        # Conservative conversion preflight; the Rust reducer owns exact binary32 rounding and grants.
        xp_upper = math.ceil(binary32(binary32(money) / binary32(0.6)))
        if not (0 <= level <= 255 and 0 <= money <= 0xffffffff
                and xp_upper < 2**32 and binary32(xp_upper * rate) < 2**32
                and binary32(binary32(money) * money_rate) < 2**31):
            refusals.append({"quest": row["entry"], "reason": "quest_xp_numeric_domain"})
        else:
            quests.append({"quest": row["entry"], "quest_level": level,
                           "reward_money_max_level": money})
    return {"status": "explicit_policy", "rules": rules, "input_binding": digest(rules),
            "quest_xp_inputs": quests, "refusals": refusals,
            "independent_obligations": ["consecutive_level_stat_and_xp_rows", "effective_initialization",
                "kill_source_tap_and_rest", "reputation_sql_reward_rates_spillover_and_auras",
                "loot_loader_item_and_source_closure", "production_admission"]}


def discharges(issue, plan):
    if plan["status"] != "explicit_policy":
        return False
    if issue["reason"] == "player.start_configuration_not_captured":
        return issue["table"] == "playercreateinfo"
    return (issue["reason"] == "quest.dynamic_xp" and issue["table"] == "quest_template"
            and any(issue["key"] in ([q["quest"]], str(q["quest"]))
                    for q in plan["quest_xp_inputs"]))
