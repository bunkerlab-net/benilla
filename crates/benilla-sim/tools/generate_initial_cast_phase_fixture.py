#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Execute unchanged pinned InitialCast source slices on authored adapters.

Default --check reproduces the committed fixture without changing it. --write
replaces only this oracle's fixture. --verify-source checks hashes/extraction
without compiling. No game data, Rust implementation, or production admission.
"""
import argparse
import copy
import hashlib
import json
import pathlib
import re
import struct
import subprocess
import tempfile

from generate_defense_skill_fixture import canonical_text, checked_text, native_compiler

ROOT = pathlib.Path(__file__).resolve().parents[1]
SOURCE = ROOT / "tools/initial_cast_phase_source"
FIXTURE = ROOT / "tests/fixtures/cmangos_initial_cast_phase_golden.json"
PIN = "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f"
MANIFEST_SHA256 = "4237fb93a91b19d2cccff099a8bb54562b146fcc3edd1e0eeb4492ba40c4e23e"


def body(text, signature):
    start = text.index(signature)
    end = text.index("{", start) + 1
    depth = 1
    while depth:
        depth += (text[end] == "{") - (text[end] == "}")
        end += 1
    return text[start:end] + "\n"


def verify_source(source_root=None):
    manifest = json.loads(checked_text(SOURCE / "manifest.json", MANIFEST_SHA256))
    if manifest["source_pin"] != PIN:
        raise SystemExit("Unexpected source pin")
    snippets = {name: checked_text(SOURCE / name, spec["sha256"])
                for name, spec in manifest["snippets"].items()}
    if source_root is not None:
        originals = {name: checked_text(source_root / name, digest)
                     for name, digest in manifest["sources"].items()}
        for name, spec in manifest["snippets"].items():
            result = ""
            for part in spec["parts"]:
                text = originals[part["path"]]
                if not result:
                    result = text[:text.index("*/") + 2] + "\n\n"
                if "method" in part:
                    result += body(text, part["method"])
                else:
                    start = text.index(part["start"])
                    end = text.index(part["end"], start + len(part["start"]))
                    result += text[start:end]
            if result != snippets[name]:
                raise SystemExit("Bundled source differs from unchanged extraction: " + name)
    return manifest, snippets


def assemble(snippets):
    harness = (ROOT / "tools/initial_cast_phase_oracle.cpp").read_text()
    handlers = sorted(set(re.findall(r"&Spell::(\w+)", snippets["EffectTable.inc"])))
    implemented = {"EffectEmpty", "EffectProficiency", "EffectApplyAura"}
    stubs = "\n".join(
        f'    void {name}(SpellEffectIndex) {{ throw std::runtime_error("unreached handler {name}"); }}'
        for name in handlers if name not in implemented)
    harness = harness.replace("// GENERATED_UNREACHED_HANDLERS", stubs)
    for name, text in snippets.items():
        marker = "// PINNED_" + name.removesuffix(".inc") + "\n"
        if harness.count(marker) != 1:
            raise SystemExit("Non-unique or missing extraction marker: " + marker)
        harness = harness.replace(marker, text + "\n")
    if "// PINNED_" in harness:
        raise SystemExit("Unfilled original-source marker")
    return harness


def fbits(value):
    return struct.unpack("<I", struct.pack("<f", value))[0]


def effect(opcode=0, *, aura=0, misc=0, base_points=0, base_dice=0,
           die_sides=0, target_a=0, target_b=0, points_per_level=0.,
           dice_per_level=0., combo_points=0.):
    return dict(opcode=opcode, aura=aura, misc=misc, base_points=base_points,
                base_dice=base_dice, die_sides=die_sides, target_a=target_a,
                target_b=target_b, points_per_level_bits=fbits(points_per_level),
                dice_per_level_bits=fbits(dice_per_level), combo_points_bits=fbits(combo_points))


def spell(spell_id, effects, *, item_class=0, item_mask=0, spell_rank=0,
          max_level=0, base_level=0, spell_level=0):
    assert len(effects) <= 3
    return dict(spell_id=spell_id, effects=effects + [effect() for _ in range(3 - len(effects))],
                item_class=item_class, item_mask=item_mask, spell_rank=spell_rank,
                max_level=max_level, base_level=base_level, spell_level=spell_level,
                resolved_cast_time_ms=0,
                resolved_duration_ms=-1 if any(e["opcode"] == 6 for e in effects) else 0)


def case(name, spells, rolls=(), *, initial_weapon_mask=0, initial_armor_mask=0,
         mechanism_only=False, source_only=False):
    return dict(name=name, spells=spells, rolls=list(rolls),
                initial_weapon_mask=initial_weapon_mask, initial_armor_mask=initial_armor_mask,
                mechanism_only=mechanism_only, source_only=source_only)


def authored_cases():
    dodge = spell(81, [effect(20, misc=90, die_sides=6)], item_class=-1, item_mask=0xffffffff)
    detect = spell(3050, [effect(49, base_points=-1, base_dice=1, die_sides=1, points_per_level=5.)], item_class=-1, item_mask=0xffffffff)
    def empty(spell_id, opcode, **kwargs):
        if spell_id in (204, 522):
            kwargs.setdefault("item_class", -1)
            kwargs.setdefault("item_mask", 0 if spell_id == 204 else 0xffffffff)
        return spell(spell_id, [effect(opcode)], **kwargs)
    def proficiency(spell_id, mask, armor=False):
        effects = [effect(60)] if armor else [effect(25), effect(60)]
        return spell(spell_id, effects, item_class=4 if armor else 2, item_mask=mask)
    def racial(spell_id, aura, misc, base_points=4):
        return spell(spell_id, [effect(6, aura=aura, misc=m, base_points=base_points,
                                      base_dice=1, die_sides=1, target_a=1) for m in misc], item_class=-1)
    staves, generic_weapon, wands = [proficiency(i, m) for i, m in ((227, 1024), (2382, 16384), (5009, 524288))]
    cloth, generic_armor = proficiency(9078, 2, True), proficiency(9125, 1, True)
    sword, mace = racial(20597, 98, [43, 55]), racial(20864, 98, [54, 160])
    spirit, diplomacy = racial(20598, 137, [4]), racial(20599, 156, [0], 9)
    rows = [case(f"dodge81_roll_{roll}", [dodge], [roll]) for roll in range(7)]
    for sides in (0, 1):
        rows.append(case(f"dodge81_die_{sides}_no_draw", [spell(81, [effect(20, misc=90, die_sides=sides)])], mechanism_only=True))
    rows += [case("detect3050_no_dispatch", [detect]),
             case("proficiency_slot_order_and_masks", [staves, generic_weapon, wands, cloth, generic_armor]),
             case("proficiency_already_known", [staves, cloth], initial_weapon_mask=1024, initial_armor_mask=2),
             case("sword20597_two_auras", [sword]), case("mace20864_two_auras", [mace]),
             case("spirit20598_self_aura", [spirit]), case("diplomacy20599_amount10", [diplomacy])]
    direct = [dodge, empty(203, 25, item_class=2, item_mask=8192), empty(204, 26), staves,
              empty(522, 37), spell(668, [effect(39, misc=7)], item_class=-1, item_mask=0xffffffff), generic_weapon, detect,
              wands, cloth, generic_armor, sword, spirit, diplomacy, mace]
    rows.append(case("authored_direct_passive_projection", direct, [4]))
    rows.append(case("targetless_before_self_despite_slot_order", [spell(900001, [
        effect(6, aura=98, misc=43, base_points=4, base_dice=1, die_sides=1, target_a=1),
        effect(20, die_sides=6), effect(26)])], [3], mechanism_only=True))
    rows.append(case("proficiency_partial_mask_overlap", [proficiency(900002, 1024 | 16384)],
                     initial_weapon_mask=1024, mechanism_only=True))
    for n, (points, dice) in enumerate(((-16777217, 16777218), (-16777219, 16777220),
                                      (-16777216, 16777217), (-16777215, 16777216),
                                      (-2147483647, 2147483647), (16777217, 0), (16777217, 1),
                                      (-32769, 1), (32766, 1), (32767, 1), (-1, 0), (0, 1))):
        for sides in (0, 1):
            rows.append(case(f"binary32_constant_{n}_die_{sides}", [spell(900100 + n, [
                effect(20, base_points=points, base_dice=dice, die_sides=sides)])], mechanism_only=True))
    rows.append(case("rank_zero_no_scaling_no_rank_query", [empty(204, 26)]))
    for rank in (0, 5, 100):
        rows.append(case(f"source_only_scaling_rank_{rank}", [spell(900200, [
            effect(20, base_points=4, base_dice=1, die_sides=1, points_per_level=5.)],
            spell_rank=rank, spell_level=1)], source_only=True))
    rows.append(case("source_only_negative_die_range", [spell(900201, [effect(20, base_dice=1, die_sides=-3)])], [-2], source_only=True))
    rows.append(case("source_only_reversed_positive_die_range", [spell(900202, [effect(20, base_dice=6, die_sides=2)])], [4], source_only=True))
    return rows


EFFECT_FIELDS = ("opcode", "aura", "misc", "base_points", "base_dice", "die_sides", "target_a", "target_b",
                 "points_per_level_bits", "dice_per_level_bits", "combo_points_bits")
SPELL_FIELDS = ("spell_id", "item_class", "item_mask", "spell_rank", "max_level", "base_level", "spell_level")


def input_bytes(cases):
    words = [str(len(cases))]
    for row in cases:
        words += [str(len(row["spells"])), str(len(row["rolls"])), str(row["initial_weapon_mask"]), str(row["initial_armor_mask"])]
        words += list(map(str, row["rolls"]))
        for entry in row["spells"]:
            words += [str(entry[key]) for key in SPELL_FIELDS]
            for slot in entry["effects"]:
                words += [str(slot[key]) for key in EFFECT_FIELDS]
    return (" ".join(words) + "\n").encode()


def validate_observations(rows):
    """Check extraction/adapters are exercising the intended boundaries, not Rust parity."""
    by_name = {row["name"]: row for row in rows}
    for roll in range(7):
        row = by_name[f"dodge81_roll_{roll}"]
        assert [e["kind"] for e in row["events"]] == ["cast_start", "draw", "amount", "dispatch", "cast_complete"]
        assert row["events"][1] == dict(kind="draw", spell_index=0, slot=0, low=0, high=6, value=roll)
        assert row["events"][2]["value"] == roll
        assert row["targetless_masks"] == [1] and row["self_masks"] == [0]
    detect = by_name["detect3050_no_dispatch"]
    assert [e["kind"] for e in detect["events"]] == ["cast_start", "cast_complete"]
    assert detect["rank_queries"] == 0 and detect["targetless_masks"] == detect["self_masks"] == [0]
    for sides in (0, 1):
        assert not any(e["kind"] == "draw" for e in by_name[f"dodge81_die_{sides}_no_draw"]["events"])
    direct = by_name["authored_direct_passive_projection"]
    assert sum(e["kind"] == "dispatch" for e in direct["events"]) == 19
    assert sum(e["kind"] == "draw" for e in direct["events"]) == 1
    assert direct["final_weapon_mask"] == 541696 and direct["final_armor_mask"] == 3
    for name in ("sword20597_two_auras", "mace20864_two_auras"):
        row = by_name[name]
        assert [e["kind"] for e in row["events"]] == [
            "cast_start", "holder_create", "amount", "dispatch", "aura_construct",
            "amount", "dispatch", "aura_construct", "aura_register", "aura_register", "aura_apply", "aura_apply", "cast_complete"]
        assert [e["value"] for e in row["events"] if e["kind"] == "aura_construct"] == [5, 5]
    assert by_name["rank_zero_no_scaling_no_rank_query"]["rank_queries"] == 0
    assert all(by_name[f"source_only_scaling_rank_{rank}"]["rank_queries"] == 1 for rank in (0, 5, 100))


def negative_probes(exe, env):
    base = case("guard", [spell(81, [effect(20, die_sides=6)])], [3])
    rows = []
    missing = copy.deepcopy(base); missing["rolls"] = []; rows.append((missing, "missing semantic irand"))
    extra = copy.deepcopy(base); extra["rolls"] = [3, 4]; rows.append((extra, "unused semantic irand"))
    outside = copy.deepcopy(base); outside["rolls"] = [7]; rows.append((outside, "outside source bounds"))
    overflow = copy.deepcopy(base); overflow["spells"][0]["effects"][0].update(base_points=2147483647, base_dice=1)
    rows.append((overflow, "source cache arithmetic outside defined signed-i32 domain"))
    unsigned = copy.deepcopy(base); unsigned["spells"][0]["effects"][0]["base_dice"] = 2147483648
    rows.append((unsigned, "source cache arithmetic outside defined signed-i32 domain"))
    for row, expected in rows:
        result = subprocess.run([str(exe)], input=input_bytes([row]), env=env, capture_output=True)
        if result.returncode == 0 or expected.encode() not in result.stderr:
            raise SystemExit("Source adapter negative probe did not fail closed: " + expected)
    return len(rows)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source_root", type=pathlib.Path, nargs="?")
    parser.add_argument("--cxx")
    action = parser.add_mutually_exclusive_group()
    action.add_argument("--write", action="store_true")
    action.add_argument("--check", action="store_true")
    action.add_argument("--verify-source", action="store_true")
    args = parser.parse_args()
    manifest, snippets = verify_source(args.source_root)
    harness = assemble(snippets)
    if args.verify_source:
        print(f"Verified {len(snippets)} unchanged source excerpts; manifest {MANIFEST_SHA256}; no compiler invoked")
        return
    cases = authored_cases()
    with tempfile.TemporaryDirectory(prefix="initial-cast-phase-oracle-") as temp:
        root = pathlib.Path(temp)
        compiler, flags, env, msvc = native_compiler(args.cxx, root)
        # Original FillTargetMap retains unused locals in this pinned revision.
        flags += ["/wd4101", "/wd4189"] if msvc else ["-Wno-unused-variable", "-Wno-unused-but-set-variable", "-Wno-implicit-fallthrough"]
        cpp, exe = root / "oracle.cpp", root / ("oracle.exe" if msvc else "oracle")
        cpp.write_text(harness)
        outputs = [str(cpp), "/Fe" + str(exe), "/Fo" + str(root / "oracle.obj")] if msvc else [str(cpp), "-o", str(exe)]
        subprocess.run(compiler + flags + outputs, cwd=root, env=env, check=True)
        output = subprocess.check_output([str(exe)], input=input_bytes(cases), env=env)
        observations = [json.loads(line) for line in output.splitlines()]
        if len(observations) != len(cases):
            raise SystemExit("Incomplete source execution output")
        for row, observation in zip(cases, observations):
            row.update(observation)
        validate_observations(cases)
        negatives = negative_probes(exe, env)
    result = dict(schema_version=1, source_pin=PIN, manifest_sha256=MANIFEST_SHA256,
                  provenance="authored synthetic inputs and resolved phase prerequisites; not SQL/DBC capture, loader order, full spell engine or gameplay admission",
                  cases=cases)
    encoded = (json.dumps(result, indent=2) + "\n").encode()
    if args.write:
        FIXTURE.write_bytes(encoded)
    elif canonical_text(FIXTURE) != encoded:
        raise SystemExit("InitialCast source fixture divergence; do not loosen comparison or silently rewrite")
    print(f"Verified {len(cases)} authored cases, {negatives} negative adapter probes; fixture SHA-256 {hashlib.sha256(encoded).hexdigest()}")
    print("Compiler:", " ".join(compiler), "; sanitized:", not msvc)
    print("Source pin:", manifest["source_pin"], "; source execution remains a bounded extracted oracle, not a full server build")


if __name__ == "__main__":
    main()
