#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Extract pinned CMaNGOS speed methods; compile a bounded, explicit fixture host.

Upstream GPL-2.0-or-later; Copyright CMaNGOS Project contributors. No warranty.
No game data, database, installation, network or native application authority.
"""
import argparse
import hashlib
from pathlib import Path
import subprocess
import tempfile

PIN = "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f"
HASHES = {
    "src/game/Entities/Unit.cpp": "03b67a4b6226f1f463e71be3694d3c950fc9b915a6e921407a98e4ff82fc21d0",
    "src/game/Spells/SpellAuras.cpp": "5e60510d23b789f8f0fb584da8f05a5be675f1cb4024ce997c989cde00105fde",
}


def function(source, signature):
    start = source.index(signature)
    body = source.index("{", start)
    depth = 1
    end = body + 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[start:end]


HOST = r'''
#include <algorithm>
#include <array>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <vector>
using int32 = int32_t;
using uint32 = uint32_t;
enum UnitMoveType { MOVE_WALK, MOVE_RUN, MOVE_RUN_BACK, MOVE_SWIM, MOVE_SWIM_BACK, MOVE_TURN_RATE, MAX_MOVE_TYPE };
enum AuraType {
 SPELL_AURA_MOD_INCREASE_MOUNTED_SPEED, SPELL_AURA_MOD_MOUNTED_SPEED_ALWAYS,
 SPELL_AURA_MOD_MOUNTED_SPEED_NOT_STACK, SPELL_AURA_MOD_INCREASE_SPEED,
 SPELL_AURA_MOD_SPEED_ALWAYS, SPELL_AURA_MOD_SPEED_NOT_STACK,
 SPELL_AURA_MOD_INCREASE_SWIM_SPEED, SPELL_AURA_USE_NORMAL_MOVEMENT_SPEED,
 SPELL_AURA_MOD_DECREASE_SPEED
};
constexpr int TYPEID_PLAYER = 4, CORPSE = 1;
constexpr int CONFIG_FLOAT_GHOST_RUN_SPEED_BG = 0, CONFIG_FLOAT_GHOST_RUN_SPEED_WORLD = 1;
struct World { float getConfig(int) { return 1.f; } } sWorld;
struct Log { void outError(const char*, ...) {} } sLog;
class Aura;
using AuraList = std::vector<Aura*>;
class Unit {
public:
 AuraList auras;
 std::array<float, MAX_MOVE_TYPE> m_speed_rate{{1.f,1.f,1.f,1.f,1.f,1.f}};
 std::vector<UnitMoveType> updates;
 float m_baseSpeedRun=1.f, m_baseSpeedWalk=1.f;
 bool IsMounted() const { return false; }
 bool IsCreature() const { return false; }
 int GetTypeId() const { return TYPEID_PLAYER; }
 int GetDeathState() const { return 0; }
 int32 GetMaxPositiveAuraModifier(AuraType) const { return 0; }
 float GetTotalAuraMultiplier(AuraType) const { return 1.f; }
 AuraList const& GetAurasByType(AuraType type) const {
   if (type != SPELL_AURA_MOD_DECREASE_SPEED) std::abort();
   return auras;
 }
 int32 GetMaxNegativeAuraModifier(AuraType) const;
 void UpdateSpeed(UnitMoveType, bool, float = 1.f);
 void SetSpeedRate(UnitMoveType, float, bool);
 float GetSpeed(UnitMoveType) const;
};
class Player: public Unit { public: bool InBattleGround() const { return false; } };
struct Modifier { int32 m_amount; };
class Aura {
 Unit* target;
 Modifier modifier;
public:
 Aura(Unit* target_, int32 amount): target(target_), modifier{amount} {}
 Modifier const* GetModifier() const { return &modifier; }
 Unit* GetTarget() const { return target; }
 void HandleAuraModDecreaseSpeed(bool, bool);
};
'''

MAIN = r'''
uint32 bits(float f) { uint32 v; std::memcpy(&v, &f, sizeof(v)); return v; }
int main() {
 std::vector<std::array<int32, 2>> cases;
 for (int32 n = 0; n >= -199; --n) cases.push_back({n, 0});
 for (auto pair : {std::array<int32, 2>{-1000000,0}, {-10000,0}, {-50,-67}, {-67,-50}, {-67,-67}, {-1,-37}, {-37,-1}, {0,0}}) cases.push_back(pair);
 std::puts("# amount1 amount2 run_rate run_back_rate swim_rate run_speed run_back_speed swim_speed walk_speed swim_back_speed");
 for (auto pair: cases) {
   Player player;
   Aura first(&player, pair[0]), second(&player, pair[1]);
   player.auras={&first,&second};
   first.HandleAuraModDecreaseSpeed(true, false);
   if (!player.updates.empty()) return 2;
   first.HandleAuraModDecreaseSpeed(true, true);
   if (player.updates != std::vector<UnitMoveType>{MOVE_RUN,MOVE_RUN_BACK,MOVE_SWIM}) return 3;
   std::printf("%d %d %08x %08x %08x %08x %08x %08x %08x %08x\n", pair[0],pair[1],
     bits(player.m_speed_rate[MOVE_RUN]),bits(player.m_speed_rate[MOVE_RUN_BACK]),bits(player.m_speed_rate[MOVE_SWIM]),
     bits(player.GetSpeed(MOVE_RUN)),bits(player.GetSpeed(MOVE_RUN_BACK)),bits(player.GetSpeed(MOVE_SWIM)),
     bits(player.GetSpeed(MOVE_WALK)),bits(player.GetSpeed(MOVE_SWIM_BACK)));
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
    aura = sources["src/game/Spells/SpellAuras.cpp"]
    base_start = unit.index("float baseMoveSpeed[MAX_MOVE_TYPE]")
    base = unit[base_start:unit.index("};", base_start) + 2]
    setter = function(unit, "void Unit::SetSpeedRate(")
    floor = setter[setter.index("    rate = "):setter.index("    // Update speed only on change")]
    if floor.strip() != "rate = std::max(rate, 0.01f);":
        raise SystemExit("unexpected source SetSpeedRate floor")
    host = HOST + base + "\n" + function(unit, "int32 Unit::GetMaxNegativeAuraModifier(")
    host += "\n" + function(unit, "void Unit::UpdateSpeed(")
    host += "\n" + function(unit, "float Unit::GetSpeed(UnitMoveType")
    host += "\n" + function(aura, "void Aura::HandleAuraModDecreaseSpeed(")
    host += "\nvoid Unit::SetSpeedRate(UnitMoveType mtype, float rate, bool forced) {\n"
    host += floor + " (void)forced; m_speed_rate[mtype]=rate; updates.push_back(mtype);\n}\n" + MAIN
    with tempfile.TemporaryDirectory(prefix="benilla-player-slow-oracle-") as tmp:
        root = Path(tmp)
        (root / "oracle.cpp").write_text(host)
        subprocess.run(["c++", "-std=c++17", "-O0", "-ffp-contract=off", "-fno-fast-math",
                        "-fsanitize=undefined,float-cast-overflow", "-fno-sanitize-recover=all",
                        str(root / "oracle.cpp"), "-o", str(root / "oracle")], check=True)
        result = subprocess.check_output([str(root / "oracle")])
    fixture = Path(__file__).parent / "fixtures/player_slow.tsv"
    if args.check:
        if fixture.read_bytes() != result:
            raise SystemExit("player slow source fixture mismatch")
    else:
        fixture.write_bytes(result)
    print(f"{len(result.splitlines()) - 1} source-extracted speed cases; sha256={hashlib.sha256(result).hexdigest()}")


if __name__ == "__main__":
    main()
