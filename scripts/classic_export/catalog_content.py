"""Whole-world content inputs, independently checked and never gameplay admission.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from __future__ import annotations
from collections import Counter, defaultdict
import hashlib
from pathlib import Path

from .core import canonical, check, decode_json, digest, CORE_REVISION
from .catalog_io import DECODED, encode_row
from .catalog_spawns import uint
from .catalog_spells import convert as spell_convert, resident, Stream
from .catalog_dbc import DbcViews
from .catalog_content_loot import STORES, loot_status, check_reference_cycles
from .catalog_content_globals import skill_pairs, locale_state, service_order

POLICY = decode_json(Path(__file__).with_name("catalog_content_policy.json").read_bytes(), 262144)
REFUSALS = ["content.reference_execution_missing", "content.sql_occurrences_not_F1",
    "content.loaded_manager_validation_pending", "content.gameplay_not_admitted"]
RELATIONS = ("creature_questrelation", "creature_involvedrelation",
             "gameobject_questrelation", "gameobject_involvedrelation")


def convert(raw, kind):
    if kind == "raw":
        return None if raw is None else raw.hex()
    if kind == "bool8":
        return int(bool(uint(raw) & 255))
    if kind == "i32":
        n = uint(raw)
        return n if n < 0x80000000 else n-0x100000000
    return spell_convert(raw, kind)


def quest_edges(quest, row):
    """ObjectMgr.cpp:4790-4875, complete inbound inputs, not validated managers."""
    for field in ("prevquestid", "nextquestid", "nextquestinchain", "breadcrumbforquestid"):
        value = row[field]
        if value:
            yield ["quest_reverse", abs(value), quest, field, value]
    if row["exclusivegroup"]:
        yield ["quest_exclusive", row["exclusivegroup"], quest]


def item_plans(row):
    yield ["item_masks",row["entry"],row["allowablerace"],row["allowableclass"]]
    for slot in range(1,6):
        yield ["item_spell_slot",row["entry"],slot,[row[f"{name}_{slot}"] for name in
            ("spellid","spelltrigger","spellcharges","spellppmrate","spellcooldown","spellcategory","spellcategorycooldown")]]


class ContentViews:
    def __init__(self, catalog, family):
        check(family in POLICY["families"], "content.unknown_family")
        check(catalog.manifest["core_revision"] == POLICY["core_revision"] == CORE_REVISION,
              "content.source_pin")
        self.catalog, self.family = catalog, family
        self.spec = POLICY["families"][family]
        self.schema = catalog._json(catalog.object(catalog.manifest["schema_root"]))["tables"]
        for table,spec in self.spec["tables"].items():
            check(table in catalog.relations, "content.missing_relation")
            check([c["COLUMN_NAME"].lower() for c in self.schema[table]["columns"]]
                  == [n for n,_ in spec["fields"]], "content.schema")

    def rows(self, table):
        fields = self.spec["tables"][table]["fields"]
        for view in self.catalog.scan(table):
            yield view.ordinal, {n:convert(cell,kind) for (n,kind),cell in zip(fields,view.cells())}

    def lookup_state(self, table, key):
        check(table in self.spec["tables"], "content.unknown_relation")
        count, fields = 0, []
        for view in self.catalog.lookup(table,key):
            count += 1
            fields.append(["null" if c is None else "empty" if not c else "value" for c in view.cells()])
            check(resident(fields) < DECODED-4194304, "content.view_limit")
        return {"whole_empty":self.catalog.relations[table]["rows"] == 0,
                "selected_empty":count == 0,"rows":count,"field_states":fields}

    def dependencies(self):
        targets = {target for spec in self.spec["tables"].values()
                   for _,target,_ in spec["references"]}
        targets.update(target for _,_,target in self.spec.get("external_references",[]))
        ids, reports, self.words, self.outfit = {}, {}, {}, []
        dbcs = DbcViews(self.catalog) if self.spec["dbc"] else None
        for target in sorted(set(targets) | set(self.spec["dbc"])):
            if target.endswith(".dbc"):
                view = dbcs.open(target)
                reports[target] = view.report()
                ids[target] = set(view.by_id)
                if self.family == "globals":
                    for key,ordinal in sorted(view.by_id.items()):
                        if target in ("SkillRaceClassInfo.dbc","SkillTiers.dbc"):
                            self.words.setdefault(target,{})[key] = view.words(ordinal)
                        elif target == "CharStartOutfit.dbc" and key:
                            self.outfit.extend((key,col,value) for col,_,value in view.record(ordinal)["values"]
                                if 5 <= col <= 16 and 0 < value < 0x80000000)
                view.close()
            else:
                index = self.catalog.relations[target]["key_columns"][0]
                ids[target] = {uint(view.cells()[index]) for view in self.catalog.scan(target)}
            check(resident((ids,self.words,self.outfit)) < DECODED-8388608, "content.view_limit")
        return ids,reports

    def report(self, output=None):
        output = Path(output) if output else None
        if output:
            output.mkdir()
        plans, lookups = Stream(output,"plans"), Stream(output,"lookups")
        (ids,dbc_reports), families = self.dependencies(), {}
        endpoints, reference_edges = defaultdict(list), defaultdict(list)
        for table,spec in sorted(self.spec["tables"].items()):
            raw, stream = hashlib.sha256(), Stream(output,table)
            counts = Counter()
            slots = Counter()
            service_rows = []
            for view in self.catalog.scan(table):
                cells = view.cells()
                raw.update(encode_row(view.ordinal,cells))
                values = [convert(c,k) for c,(_,k) in zip(cells,spec["fields"])]
                row = dict(zip((n for n,_ in spec["fields"]),values))
                status = "archive-only" if spec["status"] == "archive-only" else "retained:loader_validation_pending"
                if table in STORES:
                    status = loot_status(row,ids["item_template"],ids["conditions"])
                for field,target,mode in spec["references"]:
                    value = row[field]
                    if mode == "negative" and value >= 0 or mode == "loot_item" and row["mincountorref"] <= 0:
                        continue
                    if value:
                        key = abs(value) if mode in ("abs","negative") else value
                        present = (key & 0xffffffff) in ids[target]
                        lookups.add([table,view.ordinal,field,target,key,present])
                        if table in RELATIONS and field == "quest" and not present:
                            status = "skip:missing_quest"
                if table == "quest_template":
                    for record in quest_edges(row["entry"],row):
                        plans.add(record)
                    plans.add(["quest_masks",row["entry"],row["requiredraces"],row["requiredclasses"]])
                    for i in range(1,5):
                        value = row[f"reqcreatureorgoid{i}"]
                        if value:
                            target = "creature_template" if value > 0 else "gameobject_template"
                            # Objective identities are independent of physical spawn availability.
                            found = abs(value) in ids[target]
                            lookups.add([table,view.ordinal,f"reqcreatureorgoid{i}",target,abs(value),found])
                elif table in RELATIONS:
                    plans.add(["quest_relation",table,view.ordinal,row["id"],row["quest"],status])
                    endpoints[("creature" if table.startswith("creature") else "gameobject",row["id"])].append([table,view.ordinal,row["quest"]])
                    check(view.ordinal*512 < DECODED-8388608, "content.view_limit")
                elif table == "item_template":
                    for record in item_plans(row):
                        plans.add(record)
                elif table == "page_text":
                    plans.add(["page_next_input",row["entry"],row["next_page"]])
                elif table in STORES and not status.startswith("skip:"):
                    slots[row["entry"]] += 1
                    check(len(slots)*128 < 4194304,"content.view_limit")
                    lane = "entries" if not row["groupid"] else "explicit" if row["chanceorquestchance"] & 0x7fffffff else "equal"
                    plans.add(["loot_slot_input",table,row["entry"],view.ordinal,slots[row["entry"]],
                        row["groupid"],lane,row["mincountorref"],row["maxcount"]])
                    if table == "reference_loot_template" and row["mincountorref"] < 0:
                        reference_edges[row["entry"]].append(-row["mincountorref"])
                        check(view.ordinal*64 < 4194304,"content.view_limit")
                if self.family == "globals" and spec["status"] == "input":
                    self.global_plans(table,row,view.ordinal,cells,spec,ids,plans,lookups,service_rows)
                counts[status] += 1
                stream.add({"ordinal":view.ordinal,"values":values,
                    "nulls":[i for i,c in enumerate(cells) if c is None],"disposition":status})
            if table.startswith("npc_vendor"):
                ordered,ties = service_order(service_rows)
                for entry,slot,ordinal in ordered:
                    plans.add(["service_slot_input",table,entry,slot,ordinal])
                for tie in ties:
                    plans.add(["service_slot_tie_unproven",table,*tie])
            elif table.startswith("npc_trainer") or table == "gossip_menu_option":
                for a,b,ordinal in sorted(service_rows):
                    plans.add(["service_key_order",table,a,b,ordinal])
            families[table] = {"raw_sha256":raw.hexdigest(),"fields":spec["fields"],
                "source":spec["source"],"stream":stream.finish(),"dispositions":dict(counts),
                "whole_empty":self.catalog.relations[table]["rows"] == 0}
        if self.family == "quests":
            self.endpoint_plans(endpoints,plans)
        if self.family == "loot":
            check_reference_cycles(reference_edges)
            for entry,targets in sorted(reference_edges.items()):
                plans.add(["loot_reference_edges",entry,targets])
        if self.family == "globals":
            for table,field,target in self.spec["external_references"]:
                cols = [c["COLUMN_NAME"].lower() for c in self.schema[table]["columns"]]
                index = cols.index(field)
                for view in self.catalog.scan(table):
                    key = uint(view.cells()[index])
                    if key:
                        lookups.add([table,view.ordinal,field,target,key,key in ids[target]])
            for key,column,item in self.outfit:
                lookups.add(["CharStartOutfit.dbc",key,column,"item_template",item,item in ids["item_template"]])
        result = {"format":"benilla-content-views-v1","family":self.family,
            "catalog_sha256":self.catalog.manifest["catalog_sha256"],
            "policy_sha256":digest(self.spec),"families":families,"plans":plans.finish(),
            "lookups":lookups.finish(),"rows":sum(v["stream"]["rows"] for v in families.values()),
            "order":"catalog_ordinal_not_F1_cursor","complete":False,
            "refusals":REFUSALS+self.spec.get("refusals",[])}
        if self.spec["dbc"]:
            result["dbc_dependencies"] = dbc_reports
        result["view_sha256"] = digest(result)
        check(len(canonical(result)) <= 1048576,"content.report_limit")
        return result

    def global_plans(self,table,row,ordinal,cells,spec,ids,plans,lookups,service_rows):
        if table == "playercreateinfo_skills":
            pairs = skill_pairs(row,self.words["SkillRaceClassInfo.dbc"].values(),self.words["SkillTiers.dbc"])
            plans.add(["player_skill_input",ordinal,row["racemask"],row["classmask"],row["skill"],row["step"],pairs])
        elif table.startswith("player") and "race" in row and "class" in row:
            plans.add(["player_pair_input",table,ordinal,row["race"],row["class"]])
        if table == "playercreateinfo_action" and row["type"] in (0,128):
            target = "spell_template" if row["type"] == 0 else "item_template"
            lookups.add([table,ordinal,"action",target,row["action"],row["action"] in ids[target]])
        if table == "petcreateinfo_spell":
            for slot in range(1,5):
                plans.add(["pet_spell_slot",row["entry"],slot,row[f"spell{slot}"]])
        elif table == "pet_name_generation":
            plans.add(["pet_name_part",row["entry"],row["half"],ordinal,row["word"]])
        if table.startswith("locales_") or table == "broadcast_text_locale":
            for (name,kind),raw in zip(spec["fields"],cells):
                if kind == "text" and name != "locale":
                    plans.add(["locale_field",table,ordinal,name,locale_state(raw)])
        if table.startswith("npc_vendor"):
            service_rows.append([row["entry"],row["slot"],ordinal])
        elif table.startswith("npc_trainer"):
            service_rows.append([row["entry"],row["spell"],ordinal])
        elif table == "gossip_menu_option":
            service_rows.append([row["menu_id"],row["id"],ordinal])
        check(len(service_rows)*128 <= 8388608,"content.view_limit")

    def endpoint_plans(self, endpoints, plans):
        for namespace in ("creature","gameobject"):
            for table in (namespace,namespace+"_spawn_entry"):
                cols = [c["COLUMN_NAME"].lower() for c in self.schema[table]["columns"]]
                gi,ei = cols.index("guid"),cols.index("id" if table == namespace else "entry")
                for view in self.catalog.scan(table):
                    cells = view.cells()
                    guid,entry = uint(cells[gi]),uint(cells[ei])
                    if (namespace,entry) in endpoints:
                        plans.add(["quest_endpoint",namespace,guid,entry,table,view.ordinal,endpoints[(namespace,entry)]])
