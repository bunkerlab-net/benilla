"""Complete raw Talent rank membership, not an effective native learning map.

Pinned CMaNGOS DBCStructure.h TalentEntry:21 words, rank spell IDs at4..8.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from .dbc import DbcTable

def inputs(table: DbcTable | None, spell_ids: set[int]) -> dict:
    if table is None:
        return {"coverage": "not_captured", "table_sha256": None, "row_count": None,
                "selected_spells": sorted(spell_ids), "matches": None}
    matches = []
    # Preserve every matching row and slot. Resolving duplicate membership or loader
    # validation belongs to the Rust consumer; do not silently pick an arbitrary winner.
    for row in sorted(table.rows, key=lambda row: row.words[0]):
        slots = [{"slot": slot, "spell": row.words[4+slot]} for slot in range(5)
                 if row.words[4+slot] and row.words[4+slot] in spell_ids]
        if slots:
            matches.append({"talent_id": row.words[0], "rank_slots": slots, "source": row.raw()})
    return {"coverage": "complete_raw_table", "table_sha256": table.sha256,
            "row_count": len(table.rows), "selected_spells": sorted(spell_ids), "matches": matches}
