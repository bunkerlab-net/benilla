#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Extract pinned Unit aura selection/restart loops, not the aura-effect lifecycle.

The authored removal leaf erases the selected holder and records its ID/mode.
Real native holder effects are tested separately. No game data or live authority.
CMaNGOS upstream GPL-2.0-or-later; Copyright CMaNGOS contributors. No warranty.
"""
import argparse
import hashlib
from pathlib import Path
import subprocess
import tempfile

PIN = "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f"
HASHES = {
    "src/game/Entities/Unit.cpp": "03b67a4b6226f1f463e71be3694d3c950fc9b915a6e921407a98e4ff82fc21d0",
    "src/game/Entities/Unit.h": "2d9db8de53e4881a6684eeb2e31bc904019ee5e620aa067ecd68bd5d07c5d8a1",
}


def function(source, signature):
    start = source.index(signature)
    body = source.index("{", start)
    depth, end = 1, body + 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[start:end]


HOST = r'''
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <map>
#include <vector>
using uint32 = uint32_t;
struct SpellEntry { uint32 AuraInterruptFlags; };
struct SpellAuraHolder {
    uint32 id;
    SpellEntry spell;
    SpellEntry const* GetSpellProto() const { return &spell; }
};
using SpellAuraHolderMap = std::multimap<uint32, SpellAuraHolder*>;
using SpellAuraHolderBounds = std::pair<SpellAuraHolderMap::iterator, SpellAuraHolderMap::iterator>;
class Unit {
public:
    SpellAuraHolderMap m_spellAuraHolders;
    std::vector<uint32> removed;
    SpellAuraHolderBounds GetSpellAuraHolderBounds(uint32 id) { return m_spellAuraHolders.equal_range(id); }
    void RemoveAurasWithInterruptFlags(uint32 flags);
    void RemoveAurasDueToSpell(uint32 spellId, SpellAuraHolder* except = nullptr, AuraRemoveMode mode = AURA_REMOVE_BY_DEFAULT);
    void RemoveSpellAuraHolder(SpellAuraHolder* holder, AuraRemoveMode mode = AURA_REMOVE_BY_DEFAULT) {
        if (mode != AURA_REMOVE_BY_DEFAULT) std::abort();
        auto bounds = m_spellAuraHolders.equal_range(holder->id);
        for (auto it = bounds.first; it != bounds.second; ++it) {
            if (it->second == holder) {
                removed.push_back(holder->id);
                m_spellAuraHolders.erase(it);
                return;
            }
        }
        std::abort();
    }
};
'''

MAIN = r'''
int main() {
    std::puts("# mask61004 mask61005 operation argument removed_count first_spell second_spell");
    for (uint32 first: {0u, 0x80u, 0x100u, 0x180u}) {
        for (uint32 second: {0u, 0x80u, 0x100u, 0x180u}) {
            for (uint32 operation: {0u, 1u}) {
                for (uint32 argument: operation == 0 ? std::vector<uint32>{0u, 0x80u, 0x100u, 0x180u} : std::vector<uint32>{1604u, 61004u, 61005u}) {
                    Unit unit;
                    SpellAuraHolder a{61004u, {first}}, b{61005u, {second}};
                    unit.m_spellAuraHolders.emplace(b.id, &b);
                    unit.m_spellAuraHolders.emplace(a.id, &a);
                    if (operation == 0) unit.RemoveAurasWithInterruptFlags(argument);
                    else unit.RemoveAurasDueToSpell(argument);
                    auto removed = unit.removed;
                    // Repeat selection on live post-removal state must produce no duplicates.
                    if (operation == 0) unit.RemoveAurasWithInterruptFlags(argument);
                    else unit.RemoveAurasDueToSpell(argument);
                    if (removed != unit.removed) return 2;
                    std::printf("%u %u %u %u %zu %u %u\n", first, second, operation, argument,
                        removed.size(), removed.empty() ? 0 : removed[0], removed.size() < 2 ? 0 : removed[1]);
                }
            }
        }
    }
}
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("upstream", type=Path)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    sources = {}
    for path, expected in HASHES.items():
        data = (args.upstream / path).read_bytes()
        actual = hashlib.sha256(data).hexdigest()
        if actual != expected:
            raise SystemExit(f"pinned {PIN} source mismatch: {path}: {actual}")
        sources[path] = data.decode()
    unit = sources["src/game/Entities/Unit.cpp"]
    header = sources["src/game/Entities/Unit.h"]
    mode_start = header.index("enum AuraRemoveMode")
    mode = header[mode_start:header.index("};", mode_start) + 2]
    host = mode + "\n" + HOST
    host += "\n" + function(unit, "void Unit::RemoveAurasWithInterruptFlags(uint32 flags)")
    host += "\n" + function(unit, "void Unit::RemoveAurasDueToSpell(uint32 spellId, SpellAuraHolder* except, AuraRemoveMode mode)")
    host += MAIN
    with tempfile.TemporaryDirectory(prefix="benilla-water-aura-selection-") as tmp:
        root = Path(tmp)
        (root / "oracle.cpp").write_text(host)
        subprocess.run(["c++", "-std=c++17", "-O0", "-Wall", "-Wextra", "-Werror",
                        "-fsanitize=undefined", "-fno-sanitize-recover=all",
                        str(root / "oracle.cpp"), "-o", str(root / "oracle")], check=True)
        result = subprocess.check_output([str(root / "oracle")])
    fixture = Path(__file__).parent / "fixtures/water-aura-selection.tsv"
    if args.check:
        if fixture.read_bytes() != result:
            raise SystemExit("water aura selection source fixture mismatch")
    else:
        fixture.write_bytes(result)
    print(f"{len(result.splitlines()) - 1} source-extracted selection cases; sha256={hashlib.sha256(result).hexdigest()}")


if __name__ == "__main__":
    main()
