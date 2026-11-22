"""Pinned quest presentation SQL columns, without world rows.
SPDX-License-Identifier: GPL-3.0-or-later
CMaNGOS 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, sql/base/mangos.sql.
"""
QUEST_TEXT = ("Title", "Details", "Objectives", "EndText", "RequestItemsText", "OfferRewardText",
              *(f"ObjectiveText{i}" for i in range(1, 5)))
COLUMNS = {
    "questgiver_greeting": ["Entry", "Type", "Text", "EmoteId", "EmoteDelay"],
    "locales_questgiver_greeting": ["Entry", "Type", *(f"Text_loc{i}" for i in range(1, 9))],
    "locales_quest": ["entry", *(f"{name}_loc{i}" for name in QUEST_TEXT for i in range(1, 9))],
    "npc_gossip": ["npc_guid", "textid"],
    "npc_text": ["ID", *(name for i in range(8) for name in
        (f"text{i}_0", f"text{i}_1", f"lang{i}", f"prob{i}",
         *(f"em{i}_{j}" for j in range(6))))],
    "locales_npc_text": ["entry", *(f"Text{i}_{j}_loc{k}" for i in range(8) for j in range(2) for k in range(1, 9))],
    "npc_text_broadcast_text": ["Id", *(f"Prob{i}" for i in range(8)), *(f"BroadcastTextId{i}" for i in range(8))],
}
KEYS = {"questgiver_greeting": ("Entry", "Type"), "locales_questgiver_greeting": ("Entry", "Type"),
        "locales_quest": ("entry",), "npc_gossip": ("npc_guid",), "npc_text": ("ID",),
        "locales_npc_text": ("entry",), "npc_text_broadcast_text": ("Id",)}
TEXT = {table: {name for name in names if name.startswith(("text", "Text", *QUEST_TEXT))
              and name not in {"textid"}} for table, names in COLUMNS.items()}
