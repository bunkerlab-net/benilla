"""Pinned global spell/condition input views, never executable handlers or admission.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from __future__ import annotations
from collections import Counter, defaultdict
import hashlib
from pathlib import Path
import re
import struct
import sys

from .core import canonical, check, digest, decode_json, CORE_REVISION
from .catalog_io import DECODED, FRAME, encode_row
from .catalog_spawns import uint, floating
from .catalog_dbc import DbcViews

POLICY = decode_json(Path(__file__).with_name("catalog_spell_policy.json").read_bytes(), 65536)
TABLES = tuple(POLICY["tables"])
CONDITIONS = ("conditions", "unit_condition", "combat_condition", "worldstate_expression", "worldstate_name")
DBC_FILES = tuple(sorted(("AreaTable.dbc", "Faction.dbc", "Map.dbc", "SkillLine.dbc",
    "SkillLineAbility.dbc", "SkillRaceClassInfo.dbc", "SkillTiers.dbc", "Talent.dbc",
    "SpellCastTimes.dbc", "SpellCategory.dbc", "SpellDuration.dbc", "SpellFocusObject.dbc",
    "SpellItemEnchantment.dbc", "SpellRadius.dbc", "SpellRange.dbc", "SpellShapeshiftForm.dbc")))
REFUSALS = ["spells.reference_execution_missing", "spells.sql_occurrences_not_F1",
    "spells.registration_and_handlers_unverified", "spells.external_loaded_dependencies_unverified",
    "spells.unit_condition_flags_unassigned_at_pin", "spells.expression_execution_unimplemented",
    "spells.gameplay_not_admitted"]
SPELL_FIELDS = ("attributes", *(f"{field}{i}" for field in ("effect", "effectmiscvalue",
    "effectbasepoints", "effecttriggerspell", "effectimplicittargeta", "effectimplicittargetb",
    "effectapplyauraname", "effectitemtype") for i in range(1, 4)))
PACK = struct.Struct("<"+"I"*len(SPELL_FIELDS))


def convert(raw, kind):
    if kind in ("ignored", "unassigned"):
        return None
    if kind == "text":
        return (raw or b"").split(b"\0", 1)[0].hex()
    if kind == "f32":
        return struct.unpack("<I", struct.pack("<f", floating(raw)))[0]
    check(raw is None or len(raw) < 128, "spells.numeric_limit")
    if kind == "u64":
        match = re.match(rb"\s*([+-]?[0-9]+)", raw or b"")
        value = int(match[1]) if match else 0
        check(abs(value) <= 0xffffffffffffffff, "spells.uint64_overflow")
        return value & 0xffffffffffffffff
    value = uint(raw)
    if kind == "bool":
        return int((value if value < 1 << 31 else value-(1 << 32)) > 0)
    return value & {"u8":255, "u16":65535, "u32":0xffffffff}[kind]


def signed(value):
    return value if value < 1 << 31 else value-(1 << 32)


def resident(value):
    if isinstance(value, dict):
        return sys.getsizeof(value)+sum(resident(k)+resident(v) for k,v in value.items())
    if isinstance(value, (list, tuple, set)):
        return sys.getsizeof(value)+sum(resident(v) for v in value)
    return sys.getsizeof(value)


def bounded(*values):
    check(sum(map(resident, values)) <= DECODED-4194304, "spells.view_limit")


class Stream:
    def __init__(self, output, name):
        self.handle = (output/(name+".jsonl")).open("xb") if output else None
        self.hash, self.rows, self.size = hashlib.sha256(), 0, 0

    def add(self, record):
        raw = canonical(record)+b"\n"
        check(len(raw) <= FRAME, "spells.record_limit")
        self.hash.update(raw)
        self.rows += 1
        self.size += len(raw)
        if self.handle:
            self.handle.write(raw)

    def finish(self):
        if self.handle:
            self.handle.close()
        return {"rows":self.rows, "bytes":self.size, "sha256":self.hash.hexdigest()}


def chains(spells, talents, abilities, rows):
    """SpellMgr.cpp:1284-1623, including the pre-SQL talent/ability fallback."""
    nodes, talent_pos = {}, {}
    for _, w in sorted(talents.items()):
        for rank, spell in enumerate(w[4:9]):
            if spell:
                talent_pos[spell] = (rank+1, w)
            if w[5] and spell in spells:
                nodes[spell] = [w[3+rank] if rank else 0, w[4], rank+1, 0]
    by_spell = defaultdict(list)
    for _, w in sorted(abilities.items()):
        by_spell[w[2]].append(w)
    previous = {}
    for spell in sorted(by_spell):
        for w in by_spell[spell]:
            forward = 2368 if spell == 2366 else w[8]
            if spell not in spells or not forward or forward not in spells or forward not in by_spell:
                continue
            if forward in nodes:
                check(nodes[forward][0] == spell, "spells.chain_conflict")
            elif forward in previous:
                check(previous[forward] == spell, "spells.chain_conflict")
            elif spell in nodes:
                n = nodes[spell]
                nodes[forward] = [spell,n[1],n[2]+1,0]
            else:
                previous[forward] = spell
    while previous:
        spell = min(previous)
        prev = previous.pop(spell)
        path, seen = [(spell,prev)], {spell}
        while prev not in nodes and prev in previous:
            check(prev not in seen and len(path) <= 30, "spells.chain_cycle")
            seen.add(prev)
            path.append((prev,previous[prev]))
            prev = previous[prev]
        check(prev not in seen, "spells.chain_cycle")
        if prev not in nodes:
            nodes[prev] = [0,prev,1,0]
        for child,parent in reversed(path):
            n = nodes[parent]
            nodes[child] = [parent,n[1],n[2]+1,0]
    fallback = dict((k,list(v)) for k,v in nodes.items())
    dispositions = []
    for ordinal,row in rows:
        spell = row["spell_id"]
        n = [row[k] for k in ("prev_spell","first_spell","rank","req_spell")]
        result = "loaded"
        if spell not in spells:
            result = "skip:missing_spell"
        elif spell in nodes:
            old = nodes[spell]
            if old[:3] != n[:3]:
                result = "skip:dbc_chain_conflict"
            elif n[3]:
                old[3] = n[3]
                result = "loaded:req_update"
            else:
                result = "skip:redundant_dbc_chain"
        elif n[0] and n[0] not in spells:
            result = "skip:missing_previous"
        elif n[1] not in spells:
            result = "skip:missing_first"
        elif (spell == n[1]) != (n[2] <= 1) or (spell == n[1]) != (n[0] == 0) or (n[2] <= 1) != (n[0] == 0):
            result = "skip:incompatible_chain"
        elif n[3] and n[3] not in spells:
            result = "skip:missing_required"
        elif spell in talent_pos and (n[2] != talent_pos[spell][0] or n[1] != talent_pos[spell][1][4]
              or n[2]>1 and n[0] != talent_pos[spell][1][n[2]+2] or n[3] != talent_pos[spell][1][20]):
            result = "skip:talent_conflict"
        else:
            nodes[spell] = n
        dispositions.append([ordinal,result])
    return nodes, fallback, dispositions, talent_pos, by_spell


def rank_inputs(table, rows, spells, nodes, next_ready=True):
    """SpellRankHelper and proc-item fallback, SpellMgr.cpp:513-558,690-805,1049-1086."""
    loaded, origins, dispositions, firsts = {}, {}, [], set()
    for ordinal,row in rows:
        spell = row["entry"]
        first = nodes.get(spell,[0,spell,0,0])[1]
        result = "loaded"
        if spell not in spells:
            result = "skip:missing_spell"
        elif first != spell and (table == "spell_proc_item_enchant" or not
                (row["ppmrate"] & 0x7fffffff if table == "spell_proc_event" else row["threat"])):
            result = "skip:nonfirst_rank"
        else:
            loaded[spell] = row
            origins[spell] = spell
            firsts.add(first)
        dispositions.append([ordinal,result])
    adjacent = defaultdict(list)
    for spell,n in sorted(nodes.items()) if next_ready else ():
        for parent in (n[0],n[3]):
            if parent:
                adjacent[parent].append(spell)
    for first in sorted(firsts):
        if first not in loaded:
            continue
        pending, active, steps = [(s,False) for s in reversed(adjacent[first])], {first}, 0
        while pending:
            spell, exiting = pending.pop()
            if exiting:
                active.remove(spell)
                continue
            check(spell not in active, "spells.rank_cycle")
            active.add(spell)
            steps += 1
            check(steps <= 2_000_000, "spells.view_limit")
            if table == "spell_proc_item_enchant" or spell not in loaded:
                loaded[spell], origins[spell] = loaded[first], first
            pending.append((spell,True))
            pending.extend((s,False) for s in reversed(adjacent[spell]))
    return loaded, origins, dispositions


def pet_inputs(rows, spells):
    loaded, result = set(), {}
    for ordinal,row in rows:
        spell = row["spell"]
        if spell in loaded:
            status = "loaded_input:additional_pet"
        elif spell not in spells:
            status = "skip:missing_spell"
        else:
            r = dict(zip(SPELL_FIELDS,PACK.unpack(spells[spell])))
            if not any(r[f"effect{i}"] == 3 or r[f"effect{i}"] == 6 and r[f"effectapplyauraname{i}"] == 4 for i in range(1,4)):
                status = "skip:pet_dummy"
            elif row["aura"] not in spells:
                status = "skip:missing_aura"
            else:
                status = "loaded_input"
                loaded.add(spell)
        result[ordinal] = status
    return result


class SpellViews:
    def __init__(self, catalog):
        self.catalog = catalog
        check(catalog.manifest["core_revision"] == POLICY["core_revision"] == CORE_REVISION, "spells.source_pin")
        self.schema = catalog._json(catalog.object(catalog.manifest["schema_root"]))["tables"]
        for table,spec in POLICY["tables"].items():
            check(table in catalog.relations, "spells.missing_relation")
            check([c["COLUMN_NAME"].lower() for c in self.schema[table]["columns"]]
                  == [n for n,_ in spec["fields"]], "spells.schema")

    def rows(self, table):
        fields = POLICY["tables"][table]["fields"]
        for row in self.catalog.scan(table):
            yield row.ordinal, {n:convert(raw,kind) for (n,kind),raw in zip(fields,row.cells())}

    def lookup(self, table, cells):
        check(table in TABLES, "spells.unknown_relation")
        return self.catalog.lookup(table, cells)

    def lookup_state(self, table, key):
        count = sum(1 for _ in self.lookup(table,key))
        return {"whole_empty":self.catalog.relations[table]["rows"] == 0,
                "selected_empty":count == 0,"rows":count}

    def report(self, output=None):
        output = Path(output) if output else None
        if output:
            output.mkdir()
        dbcs = DbcViews(self.catalog)
        dependencies, ids, talents, abilities = {}, {}, {}, {}
        for name in DBC_FILES:
            view = dbcs.open(name)
            dependencies[name] = view.report()
            ids[name] = set(view.by_id)
            if name in ("Talent.dbc", "SkillLineAbility.dbc"):
                target = talents if name == "Talent.dbc" else abilities
                for key,ordinal in sorted(view.by_id.items()):
                    target[key] = view.words(ordinal)
            view.close()
        bounded(ids,talents,abilities)
        spells = {}
        for _,r in self.rows("spell_template"):
            spells[r["id"]] = PACK.pack(*(r[k] for k in SPELL_FIELDS))
            check(len(spells)*256 < DECODED-4194304, "spells.view_limit")
        aux = {}
        retained = sum(map(resident,(spells,ids,talents,abilities)))
        for table in TABLES:
            if table == "spell_template":
                continue
            rows = []
            for ordinal,row in self.rows(table):
                retained += resident((ordinal,row))+16
                check(retained <= DECODED-4194304, "spells.view_limit")
                rows.append((ordinal,row))
            aux[table] = rows
        bounded(spells,aux,ids,talents,abilities)
        nodes, fallback, chain_disp, talent_pos, by_spell = chains(spells,talents,abilities,aux["spell_chain"])
        dispositions = {"spell_chain":dict(chain_disp)}
        plans = Stream(output,"plans")
        for spell,node in sorted(fallback.items()):
            plans.add(["chain_fallback",spell,node])
        for spell,node in sorted(nodes.items()):
            plans.add(["chain",spell,node])
        for table in ("spell_proc_event", "spell_threat", "spell_proc_item_enchant"):
            loaded,origins,disp = rank_inputs(table,aux[table],spells,nodes,bool(aux["spell_chain"]))
            dispositions[table] = dict(disp)
            for spell,row in sorted(loaded.items()):
                plans.add([table,spell,origins[spell],row])
        groups = {}
        dispositions["spell_group"] = {}
        for ordinal,row in aux["spell_group"]:
            status = "skip:invalid_rule" if row["rule"] >= 3 else "loaded"
            dispositions["spell_group"][ordinal] = status
            if status == "loaded":
                groups.setdefault(row["id"],row["rule"])
        masks = {}
        dispositions["spell_group_spell"] = {}
        for ordinal,row in aux["spell_group_spell"]:
            group,spell = row["id"],row["spellid"]
            status = "skip:missing_group" if group not in groups else "skip:missing_spell" if spell not in spells else "loaded"
            dispositions["spell_group_spell"][ordinal] = status
            if status == "loaded":
                check(1 <= group <= 64, "spells.stack_shift_undefined")
                mask,rule = masks.get(spell,(0,0))
                masks[spell] = (mask | 1 << (group-1),rule)
        # The source chooses a rule in ascending group ID, not membership cursor order.
        for group,rule in sorted(groups.items()):
            for _,row in aux["spell_group_spell"]:
                if row["id"] == group and row["spellid"] in masks:
                    spell = row["spellid"]
                    mask,old = masks[spell]
                    masks[spell] = (mask,old or rule)
        for spell,value in sorted(masks.items()):
            plans.add(["stack",spell,list(value)])
        learned = defaultdict(list)
        dispositions["spell_learn_spell"] = {}
        for ordinal,row in aux["spell_learn_spell"]:
            a,b = row["entry"],row["spellid"]
            status = "skip:missing_spell" if a not in spells or b not in spells else "skip:talent" if b in talent_pos else "loaded"
            dispositions["spell_learn_spell"][ordinal] = status
            if status == "loaded":
                learned[a].append([b,row["active"],0])
        skills = 0
        # A whole-empty SQL learn table returns before the effect-derived scan.
        for spell,packed in sorted(spells.items()):
            r = dict(zip(SPELL_FIELDS,PACK.unpack(packed)))
            for i in range(1,4):
                effect = r[f"effect{i}"]
                if effect == 118 and r[f"effectmiscvalue{i}"]&65535 and (r[f"effectbasepoints{i}"]+1)&65535:
                    plans.add(["learn_skill",spell,[r[f"effectmiscvalue{i}"]&65535,(r[f"effectbasepoints{i}"]+1)&65535,effect]])
                    skills += 1
                    break
            if aux["spell_learn_spell"]:
                for i in range(1,4):
                    target = r[f"effecttriggerspell{i}"]
                    if r[f"effect{i}"] == 36 and target in spells and not any(n[0] == target for n in learned[spell]):
                        auto = r[f"effectimplicittargeta{i}"] == 5 or spell in talent_pos or r["attributes"]&0x40 != 0 or 44 in [r[f"effect{j}"] for j in range(1,4)]
                        learned[spell].append([target,1,int(auto)])
        for spell,rows in sorted(learned.items()):
            for row in rows:
                plans.add(["learn_spell",spell,row])
        dispositions["spell_pet_auras"] = pet_inputs(aux["spell_pet_auras"],spells)
        condition_status = self.condition_inputs(aux,spells,ids,by_spell,plans)
        dispositions.update(condition_status)
        for _,r in self.rows("spell_template"):
            for file,key in (("SpellCastTimes.dbc",r["castingtimeindex"]),("SpellRange.dbc",r["rangeindex"]),
                    ("SpellDuration.dbc",r["durationindex"]),("SpellCategory.dbc",r["category"]),
                    ("SpellFocusObject.dbc",r["requiresspellfocus"]),
                    *(("SpellRadius.dbc",r[f"effectradiusindex{i}"]) for i in range(1,4))):
                if key or file == "SpellRange.dbc":
                    plans.add(["dbc_lookup",r["id"],file,key,key in ids[file]])
        # Preserve every unknown effect/aura as a blocked input, not a callable handler.
        handlers = Counter()
        for packed in spells.values():
            r = dict(zip(SPELL_FIELDS,PACK.unpack(packed)))
            for i in range(1,4):
                handlers[(r[f"effect{i}"],r[f"effectapplyauraname{i}"])] += 1
        for (effect,aura),count in sorted(handlers.items()):
            plans.add(["handler_unverified",effect,aura,count])
        families = {}
        for table,spec in POLICY["tables"].items():
            raw,stream,counts = hashlib.sha256(),Stream(output,table),Counter()
            for view in self.catalog.scan(table):
                raw.update(encode_row(view.ordinal,view.cells()))
                values = [convert(cell,kind) for (_,kind),cell in zip(spec["fields"],view.cells())]
                row = dict(zip((n for n,_ in spec["fields"]),values))
                status = dispositions.get(table,{}).get(view.ordinal)
                if status is None:
                    status = self.basic_disposition(table,row,spells,nodes,ids)
                counts[status] += 1
                stream.add({"ordinal":view.ordinal,"values":values,"disposition":status})
            families[table] = {"raw_sha256":raw.hexdigest(),"fields":spec["fields"],"source":spec["source"],
                "stream":stream.finish(),"dispositions":dict(counts),"whole_empty":self.catalog.relations[table]["rows"] == 0}
        inventory = dbcs.inventory["Spell.dbc"]
        client = dbcs._open("Spell.dbc")
        client_count = client.count
        client.close()
        result = {"format":"benilla-spell-views-v1","catalog_sha256":self.catalog.manifest["catalog_sha256"],
            "policy_sha256":digest(POLICY),"families":families,"dbc_dependencies":dependencies,
            "client_spell":{"sha256":inventory["sha256"],"bytes":inventory["bytes"],"records":client_count,"server_store":False},
            "plans":plans.finish(),"fallback_chain_rows":len(fallback),"chain_rows":len(nodes),"learn_skills":skills,
            "chain_next_cache_built":bool(aux["spell_chain"]),
            "sql_spell_rows":len(spells),"condition_rows":sum(self.catalog.relations[t]["rows"] for t in CONDITIONS),
            "complete":False,"refusals":REFUSALS}
        result["view_sha256"] = digest(result)
        check(len(canonical(result)) <= 1048576,"spells.report_limit")
        return result

    @staticmethod
    def basic_disposition(table,row,spells,nodes,ids=None):
        if table == "spell_target_position":
            check(ids is not None, "spells.dbc_inputs_missing")
            if row["target_map"] not in ids["Map.dbc"]:
                return "skip:target_map"
            if all(row[f"target_position_{c}"] & 0x7fffffff == 0 for c in "xyz"):
                return "skip:target_coordinates"
            if row["id"] not in spells:
                return "skip:missing_spell"
            r = dict(zip(SPELL_FIELDS,PACK.unpack(spells[row["id"]])))
            if not any(r[f"effectimplicittarget{c}{i}"] == 17 for c in "ab" for i in range(1,4)):
                return "skip:database_target"
            return "loaded_input"
        if table in ("spell_facing","spell_elixir","spell_affect","spell_script_target","spell_target_position","spell_pet_auras","spell_area"):
            spell = row.get("entry",row.get("id",row.get("spell")))
            if spell not in spells:
                return "skip:missing_spell"
        if table == "spell_affect":
            spell = row["entry"]
            if row["effectid"] >= 3:
                return "skip:effect_index"
            if nodes.get(spell,[0,spell])[1] != spell:
                return "skip:nonfirst_rank"
            r = dict(zip(SPELL_FIELDS,PACK.unpack(spells[spell])))
            i = row["effectid"]+1
            if r[f"effect{i}"] != 6 or r[f"effectapplyauraname{i}"] not in (4,107,108,109,112):
                return "skip:affect_aura"
            if r[f"effectitemtype{i}"] and r[f"effectitemtype{i}"] == row["spellfamilymask"]:
                return "skip:redundant_mask"
            return "loaded_input"
        if table in ("spell_area","spell_script_target","spell_target_position","spell_pet_auras"):
            return "retained:cross_loader_validation_required"
        return "loaded_input"

    def condition_inputs(self,aux,spells,ids,by_spell,plans):
        loaded = {}
        statuses = {t:{} for t in CONDITIONS}
        for table in ("worldstate_expression","worldstate_name"):
            for ordinal,row in aux[table]:
                plans.add([table,ordinal,row])
                statuses[table][ordinal] = "loaded_input:execution_blocked" if table == "worldstate_expression" else "loaded_input"
        unit_ids = {row["id"] for _,row in aux["unit_condition"]}
        expr_ids = {row["id"] for _,row in aux["worldstate_expression"]}
        for ordinal,row in aux["unit_condition"]:
            values = []
            for i in range(8):
                values.append([row[f"variable_{i}"] if row[f"variable_{i}"] < 87 else 0,
                    row[f"op_{i}"] if row[f"op_{i}"] < 7 else 0,signed(row[f"value_{i}"])])
            plans.add(["unit_condition",row["id"],values])
            statuses["unit_condition"][ordinal] = "blocked:flags_unassigned_at_pin"
        for ordinal,row in aux["combat_condition"]:
            r = dict(row)
            for key in ("worldstateexpressionid","selfconditionid","targetconditionid","friendconditionid_0","friendconditionid_1","enemyconditionid_0","enemyconditionid_1"):
                if r[key] and r[key] not in (expr_ids if key == "worldstateexpressionid" else unit_ids):
                    r[key] = 0
            for key in ("friendconditionop_0","friendconditionop_1","enemyconditionop_0","enemyconditionop_1"):
                if r[key] >= 7:
                    r[key] = 0
            plans.add(["combat_condition",row["id"],r])
            statuses["combat_condition"][ordinal] = "loaded_input:execution_blocked"
        names = {row["id"] for _,row in aux["worldstate_name"]}
        external, external_count = {}, 0
        for table,key in (("item_template","entry"),("quest_template","entry"),("creature_template","entry"),("game_event","entry")):
            cols = [c["COLUMN_NAME"].lower() for c in self.schema[table]["columns"]]
            external[table] = set()
            index = cols.index(key)
            for view in self.catalog.scan(table):
                value = uint(view.cells()[index])
                if value not in external[table]:
                    external_count += 1
                    check(external_count*96 <= 4194304, "spells.view_limit")
                    external[table].add(value)
        for ordinal,row in sorted(aux["conditions"],key=lambda item:item[1]["condition_entry"]):
            entry,kind,a,b,c,d = (row[k] for k in ("condition_entry","type","value1","value2","value3","value4"))
            kind = signed(kind)
            status = "loaded_input"
            if kind in (-3,-2,-1):
                refs = [a] if kind == -3 else [a,b]+([c] if c else [])+([d] if d else [])
                if any(ref >= entry or ref not in loaded for ref in refs):
                    status = "skip:condition_reference"
                elif any(loaded[ref].startswith("retained:") for ref in refs):
                    status = "retained:condition_dependency_unverified"
            elif kind == 1:
                if a not in spells or b >= 3: status = "skip:aura"
            elif kind in (2,3,23):
                if a not in external["item_template"] or kind != 3 and b < 1: status = "skip:item"
            elif kind == 4:
                if a not in ids["AreaTable.dbc"] or b > 1: status = "skip:area"
            elif kind in (5,30):
                if a not in ids["Faction.dbc"] or b >= 8: status = "skip:reputation"
            elif kind == 6:
                if a not in (469,67): status = "skip:team"
            elif kind in (7,29,15):
                status = "retained:world_config_required"
            elif kind in (8,9,19,22):
                if a not in external["quest_template"]: status = "skip:quest"
            elif kind == 12:
                status = "retained:event_loaded_view_required"
            elif kind == 13:
                if not a and not b: status = "skip:area_flags"
            elif kind == 14:
                if not (a or b) or a and not a&0xff or b and not b&0x5df: status = "skip:race_class"
            elif kind == 17:
                if a not in spells or b > 1: status = "skip:spell"
            elif kind == 28:
                if a not in by_spell or b and b not in external["item_template"]: status = "skip:learnable"
            elif kind in (37,39):
                if a not in external["creature_template"] or kind == 37 and b == 0: status = "skip:creature"
            elif kind == 31:
                status = "retained:encounter_loaded_view_required"
            elif kind == 33:
                if b > 2: status = "skip:waypoint"
            elif kind == 35:
                if a >= 3: status = "skip:gender"
            elif kind == 36:
                if a >= 4: status = "skip:dead_state"
            elif kind == 42:
                if b > 7 or a not in names: status = "skip:worldstate"
            elif kind == 43:
                if a > 1: status = "skip:combat_state"
            elif kind not in (0,10,11,18,26,40):
                status = "skip:unknown_condition_type"
            statuses["conditions"][ordinal] = status
            if not status.startswith("skip:"):
                loaded[entry] = status
            plans.add(["condition",entry,status,row])
        return statuses
