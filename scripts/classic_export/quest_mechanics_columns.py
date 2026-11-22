"""Pinned CMaNGOS 8ec338a1 / Classic DB ec4f5961 quest mechanics schemas.
SPDX-License-Identifier: GPL-3.0-or-later
"""
U = "mediumint unsigned"
SCHEMA = {
    "dbscripts_on_quest_end": [
        ("id", U, False), ("delay", "int unsigned", False), ("priority", "int unsigned", False),
        ("command", U, False), *((name, "int unsigned", False) for name in ("datalong", "datalong2", "datalong3")),
        ("buddy_entry", U, False), ("search_radius", U, False), ("data_flags", "int unsigned", False),
        *((name, "int", False) for name in ("dataint", "dataint2", "dataint3", "dataint4")),
        *((name, "float", False) for name in ("datafloat", "x", "y", "z", "o", "speed")),
        ("condition_id", U, False), ("comments", "varchar(255)", False)],
    "gossip_menu": [("entry", "smallint unsigned", False), ("text_id", U, False),
                    ("script_id", U, False), ("condition_id", U, False)],
    "gossip_menu_option": [("menu_id", "smallint unsigned", False), ("id", "smallint unsigned", False),
        ("option_icon", U, False), ("option_text", "text", True), ("option_broadcast_text", "int", False),
        ("option_id", "tinyint unsigned", False), ("npc_option_npcflag", "int unsigned", False),
        ("action_menu_id", "mediumint", False), ("action_poi_id", U, False), ("action_script_id", U, False),
        ("box_coded", "tinyint unsigned", False), ("box_money", "int unsigned", False),
        ("box_text", "text", True), ("box_broadcast_text", "int", False), ("condition_id", U, False)],
    "conditions": [("condition_entry", U, False), ("type", "tinyint", False),
        *((f"value{i}", U, False) for i in range(1, 5)), ("flags", "tinyint unsigned", False),
        ("comments", "varchar(500)", True)],
}
KEYS = {"dbscripts_on_quest_end": (), "gossip_menu": ("entry", "text_id", "script_id"),
        "gossip_menu_option": ("menu_id", "id"), "conditions": ("condition_entry",)}
TEXT = {"dbscripts_on_quest_end": {"comments"}, "conditions": {"comments"},
        "gossip_menu_option": {"option_text", "box_text"}}
COLUMNS = {table: [name for name, _, _ in fields] for table, fields in SCHEMA.items()}
