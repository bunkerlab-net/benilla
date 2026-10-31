"""Conservative fixed-point dependency discovery, never effective learning.

The two authoritative sources are retained separately: missing SQL overrides
cannot be replaced by client bytes. See PROVENANCE.md for reviewed mechanisms.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from __future__ import annotations
from collections import defaultdict
from typing import Any

from classic_export import learning as server_learning
from classic_export.core import MAX_ITEMS, MAX_SPELLS, check, digest, integer
from .dbc import signed

MAX_SKILLS = 512
MAX_EDGES = 16_384
MAX_ABILITY_ROWS = 4096
MAX_DBC_REFERENCES = 16_384


def discover(c) -> dict[str, Any]:
    """Close skill <-> ability <-> spell effects, items and captured SQL links.

    This deliberately keeps rows with unmatched masks/zero learnOnGetSkill: the
    native initializer, not capture, resolves obtainability and profession repair.
    All indexes are built once; each distinct node and ability row is visited once.
    """
    sql_spells = {r["Id"]: r for r in c.tables["spell_template"]}
    sql_items = {r["entry"]: r for r in c.tables["item_template"]}
    nodes = {"skill": set(), "spell": set(), "item": set()}
    pending: list[tuple[str, int]] = []
    edges: set[tuple[str, int, str, int, str, int]] = set()
    selected_abilities, selected_info = {}, {}
    learn_edges: set[tuple[int, int]] = set()
    rank_edges: set[tuple[int, int]] = set()
    dbc_references: set[tuple[str, int]] = set()

    def lookup(table, entry, columns):
        dbc_references.add((table, entry))
        check(len(dbc_references) <= MAX_DBC_REFERENCES, "learning.dbc_reference_limit")
        return c.lookup(table, entry, columns)

    def add(kind, entry, origin="root", parent=None, role="root", slot=0):
        high = 65535 if kind == "skill" else 0xffffffff
        integer(entry, 1, high, "learning.dependency_id_domain")
        if parent is not None:
            edges.add((origin, parent, kind, entry, role, slot))
            check(len(edges) <= MAX_EDGES, "learning.edge_limit")
        if entry not in nodes[kind]:
            nodes[kind].add(entry)
            check(len(nodes[kind]) <= {"skill": MAX_SKILLS, "spell": MAX_SPELLS, "item": MAX_ITEMS}[kind],
                  f"learning.{kind}_limit")
            pending.append((kind, entry))

    by_skill, by_spell, by_forward, info_by_skill = (defaultdict(list) for _ in range(4))
    table = c.dbcs.get("SkillLineAbility")
    for row in sorted(table.rows, key=lambda r: r.key) if table else []:
        by_skill[row.words[1]].append(row)
        by_spell[row.words[2]].append(row)
        if row.words[8]:
            by_forward[row.words[8]].append(row)
    table = c.dbcs.get("SkillRaceClassInfo")
    for row in sorted(table.rows, key=lambda r: r.key) if table else []:
        info_by_skill[row.words[1]].append(row)
    chains = {r["spell_id"]: r for r in c.tables.get("spell_chain", [])}
    sql_learn = defaultdict(list)
    for row in c.tables.get("spell_learn_spell", []):
        sql_learn[row["entry"]].append(row)

    def ability(row):
        if row.key in selected_abilities:
            return
        selected_abilities[row.key] = row.raw()
        check(len(selected_abilities) <= MAX_ABILITY_ROWS, "learning.ability_row_limit")
        c.use(row, set(range(15)))
        for kind, field, role in (("skill", 1, "ability_skill"), ("spell", 2, "ability_spell"),
                                   ("spell", 8, "ability_forward_spell")):
            if row.words[field]:
                add(kind, row.words[field], "SkillLineAbility", row.words[0], role)

    for row in c.tables["playercreateinfo_skills"]:
        add("skill", row["skill"])
    for entry in sql_spells:
        add("spell", entry)
    for entry in sql_items:
        add("item", entry)
    for table, entry, _ in sorted(c.dependencies):
        if table == "item_template":
            add("item", entry)
    for row in c.tables["quest_template"]:
        if row["RequiredSkill"]:
            add("skill", row["RequiredSkill"], "quest_template", row["entry"], "quest_required_skill")

    def spell_effects(entry, effects, origin):
        for slot, effect, misc, trigger in effects:
            # 44 SkillStep and 118 Skill name a skill in EffectMiscValue; a
            # nonzero misc on any other effect is not a skill reference.
            if effect in {44, 118}:
                if misc > 0:
                    add("skill", misc, origin, entry, "skill_step" if effect == 44 else "skill", slot)
                else:
                    c.issue("spell_template", [entry], "player.learning_skill_effect_invalid", slot=slot,
                            effect_id=effect, raw_misc=misc, source=origin)
            if trigger:
                add("spell", trigger, origin, entry, "learn_spell" if effect == 36 else "raw_trigger_reference", slot)
                if effect == 36:
                    learn_edges.add((entry, trigger))

    while pending:
        kind, entry = pending.pop()
        if kind == "skill":
            lookup("SkillLine", entry, {0, 1})
            for row in info_by_skill[entry]:
                c.use(row, set(range(8)))
                selected_info[row.key] = row.raw()
                if row.words[6]:
                    lookup("SkillTiers", row.words[6], set(range(33)))
            for row in by_skill[entry]:
                ability(row)
        elif kind == "spell":
            source = sql_spells.get(entry)
            if source is None:
                c.need("spell_template", entry, "player.skill_candidate_spell_not_captured")
            else:
                spell_effects(entry, [(i, source[f"Effect{i}"], source[f"EffectMiscValue{i}"],
                                       source[f"EffectTriggerSpell{i}"]) for i in range(1, 4)], "spell_template")
                for table, field in (("SpellCastTimes", "CastingTimeIndex"), ("SpellRange", "rangeIndex"),
                                     ("SpellDuration", "DurationIndex"), ("SpellCategory", "Category"),
                                     ("SpellFocusObject", "RequiresSpellFocus")):
                    if source[field] or table in {"SpellCastTimes", "SpellRange"}:
                        lookup(table, source[field], {0})
                for slot in range(1, 4):
                    if source[f"EffectRadiusIndex{slot}"]:
                        lookup("SpellRadius", source[f"EffectRadiusIndex{slot}"], {0})
            row = lookup("Spell", entry, set(range(58, 115)) | set(range(40, 58)))
            if row:
                # Discover local references even before authoritative SQL rows are
                # recaptured; both origins remain explicit and independently bound.
                for table, field in (("SpellCastTimes", 18), ("SpellRange", 36),
                                     ("SpellDuration", 30), ("SpellCategory", 2),
                                     ("SpellFocusObject", 15)):
                    if row.words[field] or table in {"SpellCastTimes", "SpellRange"}:
                        lookup(table, row.words[field], {0})
                for field in range(88, 91):
                    if row.words[field]:
                        lookup("SpellRadius", row.words[field], {0})
                spell_effects(entry, [(i+1, row.words[61+i], signed(row.words[106+i]), row.words[109+i])
                                      for i in range(3)], "Spell")
                for item in {*row.words[40:42], *[signed(v) for v in row.words[42:50]]}:
                    if item > 0:
                        add("item", item, "Spell", entry, "reagent_or_totem")
            for row in by_spell[entry] + by_forward[entry]:
                ability(row)
            chain = chains.get(entry)
            if chain:
                for field in ("prev_spell", "first_spell", "req_spell"):
                    if chain[field]:
                        add("spell", chain[field], "spell_chain", entry, field)
                if chain["prev_spell"]:
                    rank_edges.add((entry, chain["prev_spell"]))
            for row in sql_learn[entry]:
                add("spell", row["SpellID"], "spell_learn_spell", entry, "learn_spell")
                learn_edges.add((entry, row["SpellID"]))
        else:
            row = sql_items.get(entry)
            if row is None:
                c.need("item_template", entry, "player.learning_item_not_captured")
                continue
            if row["RequiredSkill"]:
                add("skill", row["RequiredSkill"], "item_template", entry, "required_skill")
            for field in ("requiredspell", *(f"spellid_{i}" for i in range(1, 6))):
                if row[field] > 0:
                    add("spell", row[field], "item_template", entry, field)

    def has_cycle(pairs):
        # Kahn elimination is iterative and bounded, including deep graphs.
        adjacent, degrees = defaultdict(set), defaultdict(int)
        for start, end in pairs:
            adjacent[start].add(end)
            degrees.setdefault(start, 0)
            degrees[end] += 1
        ready = [node for node, degree in degrees.items() if degree == 0]
        count = 0
        while ready:
            count += 1
            for end in adjacent[ready.pop()]:
                degrees[end] -= 1
                if degrees[end] == 0:
                    ready.append(end)
        return count != len(degrees)

    for pairs, reason in ((learn_edges, "player.learning_spell_cycle"), (rank_edges, "player.learning_rank_cycle")):
        if has_cycle(pairs):
            c.issue("spell_template", [], reason)
    sql_coverage = c.source.get("learning_capture")
    if sql_coverage is None:
        c.issue("spell_learn_spell", [], "player.server_learning_relationships_not_captured")
    missing_spells = nodes["spell"] - set(sql_spells)
    missing_items = nodes["item"] - set(sql_items)
    dbc_required = {"SkillLine", "SkillRaceClassInfo", "SkillTiers", "SkillLineAbility", "Spell"}
    # A fixed point is a finite discovery result; it is not evidence that missing
    # SQL rows would introduce no additional edges on the next local recapture.
    complete = (not missing_spells and not missing_items and sql_coverage is not None
                and dbc_required <= set(c.dbcs)
                and all(table in c.dbcs and entry in c.dbcs[table].by_id
                        for table, entry in dbc_references))
    return {"format": "benilla-native-learning-dependencies-v1", "fixed_point_reached": True,
            "scope": "skill_ability_effect_item_and_server_learning_dependencies",
            "captured_inputs_complete": complete, "effective_learning_proven": False,
            "skill_ids": sorted(nodes["skill"]), "spell_ids": sorted(nodes["spell"]),
            "item_ids": sorted(nodes["item"]), "sql_relationship_coverage": sql_coverage,
            "dbc_references": [{"table": table, "id": entry} for table, entry in sorted(dbc_references)],
            "server_relationships": {name: c.tables.get(name) for name in server_learning.TABLES},
            "talent_membership": ("raw_table_captured_effective_loader_unproven" if "Talent" in c.dbcs else "not_captured"),
            "external_initial_hooks": "not_proven",
            "derived_learn_spell_scan": ("not_captured" if sql_coverage is None else
                "enabled_by_nonempty_table" if sql_coverage["global_learn_spell_probe"]["rows"] else
                "skipped_for_empty_table"),
            "edges": [{"from_table": a, "from_id": b, "to_kind": d, "to_id": e, "role": f, "slot": g}
                      for a, b, d, e, f, g in sorted(edges)],
            "race_class_candidates": [selected_info[k] for k in sorted(selected_info)],
            "ability_candidates": [selected_abilities[k] for k in sorted(selected_abilities)],
            "dependency_request": {"format": server_learning.REQUEST_FORMAT,
                "core_revision": c.source["core_revision"], "selection": c.source["selection"],
                "source_sha256": digest(c.source),
                "dbc_sha256": {name: table.sha256 for name, table in sorted(c.dbcs.items())},
                "item_ids": sorted(nodes["item"]), "spell_ids": sorted(nodes["spell"])}}
