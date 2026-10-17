#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Execute pinned CMaNGOS assistance source; no Cargo, network, or game assets."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import struct
import subprocess
import sys

sys.dont_write_bytecode = True
import candidate_cases
import peer_help_cases

ROOT = Path(__file__).resolve().parent
REVISION = "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f"
HASHES = {
    "src/game/AI/BaseAI/UnitAI.cpp": "7d8f1ad603a419489759713a5bc0e2bf03ab677f51b64bcb2333807e17048934",
    "src/game/AI/BaseAI/UnitAI.h": "2ae4ffeb2203221637ad3723af933ca512c201b988d49078f5e2de62498c7fa6",
    "src/game/World/World.cpp": "405e2a0def584dc2e4a9a9bc2861aac4aacefcecc8ec2fa08856324e32d866dd",
    "src/game/Entities/Relations.cpp": "06e453c80b33cddf1606004055a5d1407a46007b20fef73c5dc917156256f77b",
    "src/game/Grids/GridNotifiers.cpp": "92235b183e2b2b09e45978b8d978bb7d00b4293fa15aa870b6bf001c8d980e48",
    "src/game/Grids/GridNotifiers.h": "a6295900d39ecd6f723bbb50c0de6ac47a9cc570c49605b7c6c2223a9121328a",
    "src/game/Grids/GridNotifiersImpl.h": "e0d6fb2724572dd35d27b71b35d8464d5d19e3b26bc3a1b197918b659847b254",
    "src/game/Globals/SharedDefines.h": "4832b8e0da45211143acae7c42173fea42732e2acdbf190692725e51b020bfe8",
    "src/game/Entities/Unit.cpp": "03b67a4b6226f1f463e71be3694d3c950fc9b915a6e921407a98e4ff82fc21d0",
    "src/game/Entities/Unit.h": "2d9db8de53e4881a6684eeb2e31bc904019ee5e620aa067ecd68bd5d07c5d8a1",
    "src/game/Entities/Creature.cpp": "27ae75c9e90febd0827fd60f9c9fe27bfeeab34e476d10ac784ab1039d4b8302",
    "src/game/Entities/Creature.h": "a92b0e95e5bf7827389fcca0acc0edaba48a1a083efbf5a01a752a4d171664fc",
    "src/framework/Utilities/EventProcessor.cpp": "164cda6f21f72d60e6c426ad04d39036565eda6e60e5e10fce390c43eb396e2f",
    "src/framework/Utilities/EventProcessor.h": "8e85449a46540a7f999f1ac3a3132c2d2b3256b9c29b41a7e2c29ae78cf812c9",
}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def bits(number):
    return struct.unpack("<I", struct.pack("<f", number))[0]


def full_block(text, signature, trailing_semicolon=False):
    if text.count(signature) != 1:
        raise ValueError(f"source signature not unique: {signature}")
    first = text.index(signature)
    brace = text.index("{", first)
    depth, last = 1, brace + 1
    # Pinned selected functions have balanced braces in comments/literals too.
    while depth:
        depth += (text[last] == "{") - (text[last] == "}")
        last += 1
    if trailing_semicolon:
        if text[last] != ";":
            raise ValueError("expected class terminator")
        last += 1
    return first, last + (text[last:last + 1] == "\n")


def extract(core):
    files = {}
    for name, expected in HASHES.items():
        data = (core / name).read_bytes()
        if sha(data) != expected:
            raise ValueError(f"pinned source SHA-256 mismatch: {name}")
        files[name] = data.decode()
    excerpts, records = {}, []

    def add(name, source, first, last, compiled=True):
        text = files[source]
        data = text[first:last].encode()
        excerpts[name] = data
        records.append(dict(name=name, source=source, first_byte=first, last_byte_exclusive=last,
                            first_line=text[:first].count("\n") + 1,
                            last_line=text[:last].count("\n") + (not text[:last].endswith("\n")),
                            sha256=sha(data), compiled=compiled))

    def method(name, source, signature, semicolon=False):
        add(name, source, *full_block(files[source], signature, semicolon))

    def snippet(name, source, needle, anchor=None, compiled=True):
        text = files[source]
        begin = 0 if anchor is None else text.index(anchor)
        first = text.index(needle, begin)
        if text.find(needle, first + len(needle)) != -1 and anchor is None:
            raise ValueError(f"source snippet not unique: {name}")
        add(name, source, first, first + len(needle), compiled)

    unit = "src/game/Entities/Unit.cpp"
    creature = "src/game/Entities/Creature.cpp"
    header = "src/game/Entities/Creature.h"
    for name, signature in [
        ("SetInCombatState.inc", "void Unit::SetInCombatState(bool PvP, Unit* enemy)"),
        ("TriggerAggroLinkingEvent.inc", "void Unit::TriggerAggroLinkingEvent(Unit* enemy)"),
        ("CombatStop.inc", "void Unit::CombatStop(bool includingCast, bool includingCombo)"),
        ("UnitSetDeathState.inc", "void Unit::SetDeathState(DeathState s)"),
    ]:
        method(name, unit, signature)
    for name, signature in [
        ("MarkCallAssistanceOnPull.inc", "std::pair<bool, GuidVector> Creature::MarkCallAssistanceOnPull(Unit* enemy)"),
        ("CallAssistanceOnPull.inc", "void Creature::CallAssistanceOnPull(Unit* enemy, GuidVector const& receiverList)"),
        ("CreatureUpdate.inc", "void Creature::Update(const uint32 diff)"),
        ("CreatureSetDeathState.inc", "void Creature::SetDeathState(DeathState s)"),
        ("RemoveCorpse.inc", "void Creature::RemoveCorpse(bool inPlace)"),
    ]:
        method(name, creature, signature)
    # Separate peer-help executable: exact helper resolution and ordered gates.
    ai = "src/game/AI/BaseAI/UnitAI.cpp"
    ai_header = "src/game/AI/BaseAI/UnitAI.h"
    method("CheckForHelp.inc", ai, "void UnitAI::CheckForHelp(Unit* who, Unit* me, float distance)")
    method("GetAttackerForHelper.inc", "src/game/Entities/Unit.h", "        Unit* getAttackerForHelper()")
    method("HelpAIOrders.inc", ai_header, "enum AIOrders\n", True)
    snippet("HelpGetAIOrder.inc", ai_header,
            "        AIOrders GetAIOrder() const { return m_currentAIOrder; }\n")
    for name, needle in [
        ("HelpGetVictim.inc", "        Unit* GetVictim() const { return m_attacking; }     //< Returns the victim that this unit is currently attacking\n"),
        ("HelpAttackerSet.inc", "        typedef std::set<Unit*> AttackerSet;\n"),
        ("HelpIsConfused.inc", "        inline bool IsConfused() const { return HasFlag(UNIT_FIELD_FLAGS, UNIT_FLAG_CONFUSED); }\n"),
        ("HelpIsStunned.inc", "        inline bool IsStunned() const { return HasFlag(UNIT_FIELD_FLAGS, UNIT_FLAG_STUNNED); }\n"),
        ("HelpUnitCanCall.inc", "        virtual bool CanCallForAssistance() const { return true; }\n"),
        ("HelpUnitCanCheck.inc", "        virtual bool CanCheckForHelp() const { return true; }\n"),
    ]:
        snippet(name, "src/game/Entities/Unit.h", needle)
    snippet("HelpWorldRadiusDefault.inc", "src/game/World/World.cpp",
            '    setConfigPos(CONFIG_FLOAT_CREATURE_CHECK_FOR_HELP_RADIUS,         "CreatureCheckForHelpRadius",     5.0f);\n')
    snippet("HelpSightCaller.inc", ai,
            "    if (who->GetObjectGuid().IsCreature() && who->IsInCombat())\n        CheckForHelp(who, m_unit, sWorld.getConfig(CONFIG_FLOAT_CREATURE_CHECK_FOR_HELP_RADIUS));\n", compiled=False)
    # Separate candidate executable: complete source predicates, not ports of Rust.
    relations = "src/game/Entities/Relations.cpp"
    for name, signature in [
        ("GetControllingPlayer.inc", "Player const* Unit::GetControllingPlayer(bool ignoreCharms"),
        ("FactionTemplateReaction.inc", "static inline ReputationRank GetFactionReaction(FactionTemplateEntry const* thisTemplate, FactionTemplateEntry const* otherTemplate)"),
        ("FactionUnitReaction.inc", "static ReputationRank GetFactionReaction(FactionTemplateEntry const* thisTemplate, Unit const* unit)"),
        ("UnitReaction.inc", "ReputationRank Unit::GetReactionTo(Unit const* unit) const"),
        ("IsEnemy.inc", "bool Unit::IsEnemy(Unit const* unit) const"),
        ("IsFriend.inc", "bool Unit::IsFriend(Unit const* unit) const"),
        ("CanAttack.inc", "bool Unit::CanAttack(const Unit* unit) const"),
        ("CanAssist.inc", "bool Unit::CanAssist(const Unit* unit, bool /*ignoreFlags*/) const"),
        ("CanAttackOnSight.inc", "bool Unit::CanAttackOnSight(Unit const* target) const"),
        ("CanAssistInCombatAgainst.inc", "bool Unit::CanAssistInCombatAgainst(Unit const* who, Unit const* enemy) const"),
        ("CanJoinInAttacking.inc", "bool Unit::CanJoinInAttacking(Unit const* enemy) const"),
    ]:
        method(name, relations, signature)
    method("AnyAssistCreatureInRangeCheck.inc", "src/game/Grids/GridNotifiers.cpp",
           "bool MaNGOS::AnyAssistCreatureInRangeCheck::operator()(Creature* u)")
    method("AnyAssistCreatureInRangeCheckDecl.inc", "src/game/Grids/GridNotifiers.h",
           "class AnyAssistCreatureInRangeCheck", True)
    method("CreatureListSearcherDecl.inc", "src/game/Grids/GridNotifiers.h",
           "template<class Check>\n    struct CreatureListSearcher", True)
    method("CreatureListSearcherVisit.inc", "src/game/Grids/GridNotifiersImpl.h",
           "template<class Check>\nvoid MaNGOS::CreatureListSearcher<Check>::Visit(CreatureMapType& m)")
    method("CandidateUnitFlags.inc", "src/game/Entities/Unit.h", "enum UnitFlags\n", True)
    method("CandidateUnitState.inc", "src/game/Entities/Unit.h", "enum UnitState\n", True)
    snippet("CandidateIsInCombat.inc", "src/game/Entities/Unit.h",
            "        bool IsInCombat() const { return HasFlag(UNIT_FIELD_FLAGS, UNIT_FLAG_IN_COMBAT); }\n")
    snippet("IsFeigningDeathSuccessfully.inc", "src/game/Entities/Unit.h",
            "        bool IsFeigningDeathSuccessfully() const { return hasUnitState(UNIT_STAT_FEIGN_DEATH); }\n")
    method("CandidateReputationRank.inc", "src/game/Globals/SharedDefines.h", "enum ReputationRank\n", True)
    method("CreatureCanInitiateAttack.inc", creature, "bool Creature::CanInitiateAttack() const")
    snippet("CanEnterCombat.inc", "src/game/Entities/Unit.h",
            "        bool CanEnterCombat() const { return m_canEnterCombat && !GetCombatManager().IsEvadingHome(); }\n")
    method("UnitLambdaEvent.inc", "src/game/Entities/Unit.h", "class UnitLambdaEvent : public BasicEvent", True)
    for name, needle in [
        ("SetNoCallAssistance.inc", "        void SetNoCallAssistance(bool val) { m_AlreadyCallAssistance = val; }\n"),
        ("CanCallForAssistance.inc", "        bool CanCallForAssistance() const override { return m_canCallForAssistance; }\n"),
        ("SetCanCallForAssistance.inc", "        void SetCanCallForAssistance(bool state) { m_canCallForAssistance = state; }\n"),
        ("CanCheckForHelp.inc", "        bool CanCheckForHelp() const override { return m_checkForHelp; }\n"),
        ("SetCanCheckForHelp.inc", "        void SetCanCheckForHelp(bool state) { m_checkForHelp = state; }\n"),
        ("NoCallAssistFlag.inc", "    CREATURE_EXTRA_FLAG_NO_CALL_ASSIST         = 0x00000800,       // 2048 creature shouldn't call for assistance on aggro\n"),
    ]:
        snippet(name, header, needle)
    snippet("InitCanCallAssistance.inc", creature,
            "    SetCanCallForAssistance((cinfo->ExtraFlags & CREATURE_EXTRA_FLAG_NO_CALL_ASSIST) == 0);\n")
    for name, needle in [("InitialCheckForHelp.inc", "m_checkForHelp(true)"),
                         ("InitialAlreadyCalled.inc", "m_AlreadyCallAssistance(false)"),
                         ("InitialCanCallAssistance.inc", "m_canCallForAssistance(true)")]:
        snippet(name, creature, needle, "Creature::Creature(CreatureSubtype subtype)")
    snippet("UnitUpdateEventQueue.inc", unit, "    m_events.Update(diff);\n", "void Unit::Update(const uint32 diff)")
    snippet("UnitUpdateWorldGuard.inc", unit, "    if (!IsInWorld())\n        return;\n", "void Unit::Update(const uint32 diff)")
    # Placement evidence verifies hooks precede the help suffix and no event kill
    # occurs in the exact lifecycle bodies. The full entry body is also compiled.
    suffix = "        creature->SetCanCheckForHelp(false);\n"
    snippet("CombatHelpSuffix.inc", unit,
            files[unit][files[unit].index(suffix):files[unit].index("        TriggerAggroLinkingEvent(enemy);", files[unit].index(suffix)) + len("        TriggerAggroLinkingEvent(enemy);\n")],
            compiled=False)
    for source in HASHES:
        if "EventProcessor" in source:
            add(Path(source).name, source, 0, len(files[source]))
    notice = files[unit][:files[unit].index("*/") + 2] + "\n"
    excerpts["COPYING-NOTICE.txt"] = notice.encode()
    manifest = dict(schema_version=1, source_revision=REVISION, source_files=HASHES, excerpts=records)
    return excerpts, manifest


def verify_placement(excerpts):
    entry = excerpts["SetInCombatState.inc"].decode()
    required = ["if (!IsAlive())", "bool notInCombat = !HasFlag", "if (creatureNotInCombat)\n    {",
                "creature->AI()->EnterCombat(enemy);", "creature->SetCanCheckForHelp(false);",
                "creature->m_events.AddEvent(new UnitLambdaEvent", "TriggerAggroLinkingEvent(enemy);"]
    positions = [entry.index(needle) for needle in required]
    if positions != sorted(positions):
        raise ValueError("combat suffix source placement changed")
    mark = excerpts["MarkCallAssistanceOnPull.inc"].decode()
    if "bool stored = m_AlreadyCallAssistance;" not in mark or "return {stored, guids};" not in mark:
        raise ValueError("literal saved assistance boolean changed")
    for name in ["CreatureUpdate.inc", "CreatureSetDeathState.inc", "UnitSetDeathState.inc", "RemoveCorpse.inc", "CombatStop.inc"]:
        if "KillAllEvents" in excerpts[name].decode() or "KillEvent(" in excerpts[name].decode():
            raise ValueError(f"lifecycle unexpectedly kills events: {name}")
    if "Abort(" in excerpts["UnitLambdaEvent.inc"].decode():
        raise ValueError("UnitLambdaEvent gained an Abort override")



def verify_candidate_source(excerpts):
    ordered = {
        "AnyAssistCreatureInRangeCheck.inc": ["if (u == i_funit)",
            "if (!u->CanAssistInCombatAgainst(i_funit, i_enemy))",
            "if (!i_funit->IsWithinDistInMap(u, i_range))", "if (!i_funit->IsWithinLOSInMap(u))"],
        "CanAssistInCombatAgainst.inc": ["if (GetMap()->Instanceable())",
            "return CanJoinInAttacking(enemy);", "if (IsInCombat())",
            "if (CanAssist(who) && CanAttackOnSight(enemy))"],
        "CanAssist.inc": ["if (unit->HasFlag(UNIT_FIELD_FLAGS, UNIT_FLAG_UNINTERACTIBLE))",
            "if (GetReactionTo(unit) < REP_FRIENDLY)"],
        "CanAttackOnSight.inc": ["target->GetCombatManager().IsEvadingHome()",
            "!IsIgnoringFeignDeath() && target->IsFeigningDeathSuccessfully()",
            "static_cast<Pet const*>(target)->HasActionsDisabled()",
            "return (CanAttack(target) && IsEnemy(target));"],
    }
    for name, needles in ordered.items():
        body = excerpts[name].decode()
        if any(needle not in body for needle in needles):
            raise ValueError(f"candidate predicate source shape changed: {name}")
        positions = [body.index(needle) for needle in needles]
        if positions != sorted(positions):
            raise ValueError(f"candidate predicate source order changed: {name}")
        for absent in ["ExtraFlags", "CanAssistTo(", "IsAlive(", "CanAggro("]:
            if absent in body:
                raise ValueError(f"unexpected outdoor candidate predicate: {name}: {absent}")
    if "return (GetReactionTo(unit) < REP_UNFRIENDLY);" not in excerpts["IsEnemy.inc"].decode():
        raise ValueError("candidate IsEnemy threshold changed")
    if "return (IsEnemy(unit) || unit->IsEnemy(this));" not in excerpts["CanAttack.inc"].decode():
        raise ValueError("candidate reciprocal CanAttack changed")


def verify_peer_help_source(excerpts):
    ordered = {
        "CheckForHelp.inc": [
            "Unit* victim = who->getAttackerForHelper();", "if (!victim)",
            "me->IsInCombat() || !me->CanCallForAssistance() || !who->CanCallForAssistance() || !me->CanCheckForHelp() || !who->CanCheckForHelp()",
            "who->hasUnitState(UNIT_STAT_RETREATING) || who->IsConfused() || who->IsStunned()",
            "me->CanInitiateAttack() && me->CanAttackOnSight(victim) && victim->isInAccessablePlaceFor(me) && victim->IsVisibleForOrDetect(me, me, false)",
            "me->IsWithinDistInMap(who, distance) && me->IsWithinLOSInMap(who, true)",
            "me->CanAssistInCombatAgainst(who, victim)", "AttackStart(victim);",
            "who->AI() && who->AI()->GetAIOrder() == ORDER_FLEEING", "who->GetMotionMaster()->InterruptPanic();"],
        "GetAttackerForHelper.inc": ["if (GetVictim() != nullptr)", "return GetVictim();",
            "if (!m_attackers.empty())", "return *(m_attackers.begin());", "return nullptr;"],
    }
    for name, needles in ordered.items():
        body = excerpts[name].decode()
        if any(needle not in body for needle in needles):
            raise ValueError(f"peer-help source shape changed: {name}")
        positions = [body.index(needle) for needle in needles]
        if positions != sorted(positions):
            raise ValueError(f"peer-help source order changed: {name}")
    if "CONFIG_FLOAT_CREATURE_CHECK_FOR_HELP_RADIUS" not in excerpts["HelpSightCaller.inc"].decode():
        raise ValueError("peer-help caller no longer uses world config")
    if '"CreatureCheckForHelpRadius",     5.0f' not in excerpts["HelpWorldRadiusDefault.inc"].decode():
        raise ValueError("peer-help default radius changed")
    for name in ["CheckForHelp.inc", "HelpSightCaller.inc"]:
        if "CallForHelp" in excerpts[name].decode():
            raise ValueError("peer-help unexpectedly reads template CallForHelp")


def validate_source(core, source_dir=ROOT / "source", refresh=False):
    excerpts, manifest = extract(core)
    verify_placement(excerpts)
    verify_candidate_source(excerpts)
    verify_peer_help_source(excerpts)
    manifest_bytes = (json.dumps(manifest, indent=2, sort_keys=True) + "\n").encode()
    if refresh:
        for name, data in excerpts.items():
            (source_dir / name).write_bytes(data)
        (source_dir / "manifest.json").write_bytes(manifest_bytes)
    else:
        if (source_dir / "manifest.json").read_bytes() != manifest_bytes:
            raise ValueError("bundled source manifest mismatch")
        for name, data in excerpts.items():
            if (source_dir / name).read_bytes() != data:
                raise ValueError(f"bundled source excerpt mismatch: {name}")
    return manifest


def cases():
    out = []
    def add(name, operations, flags=2, help_radius=5., fallback=10., delay=5,
            queue=0, map_clock=0, already=False, receivers=()):
        out.append(dict(name=name, input=dict(raw_extra_flags=flags,
            call_for_help_bits=bits(help_radius), fallback_radius_bits=bits(fallback),
            delay_ms=delay, initial_queue_clock=queue, initial_map_clock=map_clock,
            initial_already_called=already, authored_receivers=list(receivers), operations=operations)))
    add("first_entry_positive_radius", ["entry", "update 4", "update 1"])
    add("repeated_hit_no_new_pair", ["entry", "entry", "update 5"])
    add("saved_false_authored_receivers", ["entry", "update 5"], receivers=(101, 102))
    add("saved_true_authored_receivers", ["entry", "update 5"], already=True, receivers=(101, 102))
    add("saved_true_empty_receivers", ["entry", "update 5"], already=True)
    add("no_call_assist_saved_false", ["entry", "update 5"], flags=0x802, receivers=(101,))
    add("no_call_assist_saved_true", ["entry", "update 5"], flags=0x802, already=True, receivers=(101,))
    add("zero_help_uses_fallback", ["entry", "update 5"], help_radius=0.)
    add("negative_help_uses_fallback", ["entry", "update 5"], help_radius=-1.)
    add("zero_delay_waits_for_update", ["entry", "update 0"], delay=0)
    add("callback_entry_zero_delay_source_only", ["callback_entry", "update 7"], delay=0)
    add("captured_receivers_missing_at_execution", ["entry", "receivers_present 0", "update 5"], already=True, receivers=(101,))
    add("prior_equal_deadline_keeps_order", ["sentinel", "entry", "update 5"])
    add("queue_clock_lags_map_clock", ["entry", "map_advance 100", "update 4", "update 1"], queue=17, map_clock=1000)
    add("missing_enemy_at_execution", ["entry", "enemy_present 0", "update 5"], already=True, receivers=(101,))
    add("combat_stop_before_execution", ["entry", "combat_stop", "update 5"], already=True, receivers=(101,))
    add("stop_reentry_old_saved_true_survives", ["entry", "combat_stop", "entry", "update 5"], already=True, receivers=(101,))
    add("corpse_advances_events", ["entry", "die", "update 5"], already=True, receivers=(101,))
    add("dead_freezes_respawn_retains", ["entry", "die", "remove_corpse", "update 100", "respawn", "entry", "update 5"], already=True, receivers=(101,))
    add("teardown_force_aborts_without_execute", ["entry", "teardown 1"])
    add("teardown_normal_aborts_without_execute", ["entry", "teardown 0"])
    add("due_abort_does_not_execute", ["entry", "mark_abort", "update 5"])
    add("wide_queue_clock", ["entry", "update 5"], queue=4294967296, map_clock=9000000000)
    return out


def command_stream(rows):
    result = []
    for row in rows:
        x = row["input"]
        result.append("case " + " ".join(map(str, [row["name"], x["raw_extra_flags"], x["call_for_help_bits"],
            x["fallback_radius_bits"], x["delay_ms"], x["initial_queue_clock"], x["initial_map_clock"],
            int(x["initial_already_called"]), len(x["authored_receivers"]), *x["authored_receivers"]])))
        result.extend(x["operations"])
    return ("\n".join(result) + "\n").encode()


def record_command(run, name, command, **kwargs):
    (run / f"{name}.command.json").write_text(json.dumps(command, indent=2) + "\n")
    result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, **kwargs)
    (run / f"{name}.stdout").write_bytes(result.stdout)
    (run / f"{name}.stderr").write_bytes(result.stderr)
    (run / f"{name}.returncode").write_text(f"{result.returncode}\n")
    if result.returncode:
        raise RuntimeError(f"{name} failed ({result.returncode}); logs: {run}")
    return result.stdout


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--core", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--compiler", default="g++")
    parser.add_argument("--refresh-excerpts", action="store_true")
    parser.add_argument("--write-fixture", action="store_true")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    number = 1
    while (args.output / f"run-{number:03}").exists():
        number += 1
    run = (args.output / f"run-{number:03}").resolve()
    run.mkdir()
    try:
        manifest = validate_source(args.core.resolve(), refresh=args.refresh_excerpts)
        shutil.copytree(ROOT / "source", run / "source")
        shutil.copyfile(ROOT / "oracle_driver.cpp", run / "oracle_driver.cpp")
        shutil.copyfile(Path(__file__), run / "run_oracle.py")
        record_command(run, "compiler-version", [args.compiler, "--version"])
        flags = ["-std=c++17", "-O1", "-g", "-Wall", "-Wextra", "-Werror", "-pedantic",
                 "-fno-fast-math", "-ffp-contract=off", "-fsanitize=undefined,float-cast-overflow", "-fno-sanitize-recover=all"]
        record_command(run, "compile", [args.compiler, *flags, "-I" + str(run / "source"),
            str(run / "oracle_driver.cpp"), str(run / "source/EventProcessor.cpp"), "-o", str(run / "oracle")])
        rows = cases()
        inputs = command_stream(rows)
        (run / "inputs.txt").write_bytes(inputs)
        outputs = record_command(run, "execute", [str(run / "oracle")], input=inputs)
        observed = [json.loads(line) for line in outputs.splitlines()]
        cursor = 0
        for row in rows:
            count = 1 + len(row["input"]["operations"])
            row["observations"] = observed[cursor:cursor + count]
            if len(row["observations"]) != count or any(x.pop("case") != row["name"] for x in row["observations"]):
                raise ValueError("source observation identity/count mismatch")
            cursor += count
        if cursor != len(observed):
            raise ValueError("unexpected source observations")
        fixture = dict(schema_version=1, source_revision=REVISION,
            receiver_coverage="explicitly authored selected GUIDs; no real Cell traversal, eligibility, geometry, or production admission proof",
            cases=rows)
        fixture_bytes = (json.dumps(fixture, indent=2) + "\n").encode()
        (run / "vectors.json").write_bytes(fixture_bytes)
        if args.check and (ROOT / "vectors.json").read_bytes() != fixture_bytes:
            raise ValueError("source-generated fixture differs; inspect source outputs")
        if args.write_fixture:
            (ROOT / "vectors.json").write_bytes(fixture_bytes)
        # Independent candidate predicates have a separate authored input/fixture
        # and executable; the lifecycle selected-receiver shim is unchanged.
        for name in ["candidate_driver.cpp", "candidate_cases.py"]:
            shutil.copyfile(ROOT / name, run / name)
        record_command(run, "candidate-compile", [args.compiler, *flags,
            str(run / "candidate_driver.cpp"), "-o", str(run / "candidate-oracle")])
        candidates = candidate_cases.cases()
        candidate_inputs = candidate_cases.command_stream(candidates)
        (run / "candidate-inputs.txt").write_bytes(candidate_inputs)
        candidate_outputs = record_command(run, "candidate-execute", [str(run / "candidate-oracle")], input=candidate_inputs)
        candidate_observed = [json.loads(line) for line in candidate_outputs.splitlines()]
        if len(candidate_observed) != len(candidates):
            raise ValueError("candidate observation count mismatch")
        for row, observation in zip(candidates, candidate_observed):
            if observation.pop("case") != row["name"]:
                raise ValueError("candidate observation identity mismatch")
            row["observation"] = observation
        candidate_fixture = dict(schema_version=1, source_revision=REVISION,
            coverage="unchanged source predicates and collection loop over explicitly authored candidate references; no real Cell traversal, world-membership completeness, geometry, LOS, database, or production admission proof",
            leaf_domain="uncontrolled NPC candidates; ordinary player/NPC/pet enemies; absent masters, forced reputation and reputation factions; authored range/LOS outcomes and instanceability",
            cases=candidates)
        candidate_bytes = (json.dumps(candidate_fixture, indent=2) + "\n").encode()
        (run / "candidate_vectors.json").write_bytes(candidate_bytes)
        if args.check and (ROOT / "candidate_vectors.json").read_bytes() != candidate_bytes:
            raise ValueError("source-generated candidate fixture differs; inspect source outputs")
        if args.write_fixture:
            (ROOT / "candidate_vectors.json").write_bytes(candidate_bytes)
        # Third independent executable; no previous fixture is repurposed.
        for name in ["peer_help_driver.cpp", "peer_help_cases.py"]:
            shutil.copyfile(ROOT / name, run / name)
        record_command(run, "peer-help-compile", [args.compiler, *flags,
            str(run / "peer_help_driver.cpp"), "-o", str(run / "peer-help-oracle")])
        helpers = peer_help_cases.cases()
        helper_inputs = peer_help_cases.command_stream(helpers)
        (run / "peer-help-inputs.txt").write_bytes(helper_inputs)
        helper_outputs = record_command(run, "peer-help-execute", [str(run / "peer-help-oracle")], input=helper_inputs)
        helper_observed = [json.loads(line) for line in helper_outputs.splitlines()]
        cursor = 0
        for row in helpers:
            count = sum(op.startswith("observe ") for op in row["input"]["operations"])
            row["observations"] = helper_observed[cursor:cursor + count]
            if len(row["observations"]) != count or any(value.pop("case") != row["name"] for value in row["observations"]):
                raise ValueError("peer-help observation identity/count mismatch")
            cursor += count
        if cursor != len(helper_observed):
            raise ValueError("unexpected peer-help observations")
        helper_fixture = dict(schema_version=1, source_revision=REVISION,
            coverage="unchanged CheckForHelp, helper resolution and predicate sources; authored accessibility, visibility, range and LOS leaves; no real map, Cell traversal, geometry, AI dispatch effects or production admission proof",
            leaf_domain="uncontrolled NPC observer and target; authored active victim and zero/singleton attacker set; ordinary player victims; source config default 5.0f or explicitly authored override",
            cases=helpers)
        helper_bytes = (json.dumps(helper_fixture, indent=2) + "\n").encode()
        (run / "peer_help_vectors.json").write_bytes(helper_bytes)
        if args.check and (ROOT / "peer_help_vectors.json").read_bytes() != helper_bytes:
            raise ValueError("source-generated peer-help fixture differs; inspect source outputs")
        if args.write_fixture:
            (ROOT / "peer_help_vectors.json").write_bytes(helper_bytes)
        summary = dict(status="passed", cases=len(rows), observations=len(observed),
            peer_help_cases=len(helpers), peer_help_observations=len(helper_observed),
            peer_help_fixture_sha256=sha(helper_bytes),
            peer_help_driver_sha256=sha((ROOT / "peer_help_driver.cpp").read_bytes()),
            peer_help_inputs_sha256=sha((ROOT / "peer_help_cases.py").read_bytes()),
            candidate_cases=len(candidates),
            candidate_evaluations=sum(len(row["observation"]["evaluations"]) for row in candidates),
            candidate_fixture_sha256=sha(candidate_bytes),
            candidate_driver_sha256=sha((ROOT / "candidate_driver.cpp").read_bytes()),
            candidate_inputs_sha256=sha((ROOT / "candidate_cases.py").read_bytes()),
            source_revision=REVISION, fixture_sha256=sha(fixture_bytes),
            driver_sha256=sha((ROOT / "oracle_driver.cpp").read_bytes()),
            source_manifest_sha256=sha(json.dumps(manifest, sort_keys=True).encode()),
            command=" ".join(sys.argv), fixture_checked=args.check)
        (run / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
        print(json.dumps(dict(**summary, output=str(run)), indent=2))
    except Exception as exc:
        (run / "failure.txt").write_text(str(exc) + "\n")
        raise


if __name__ == "__main__":
    main()
