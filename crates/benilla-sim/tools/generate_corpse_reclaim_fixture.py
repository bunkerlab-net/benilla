#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Hash-checked pinned CMaNGOS reclaim handler oracle; synthetic inputs only.

Usage: generate_corpse_reclaim_fixture.py PINNED_SOURCE_DIRECTORY
CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Project contributors.
Extracted original source is GPL-2.0-or-later; this harness is GPL-3.0-or-later.
No database, geometry service, release transition, or native admission is used.
"""
import hashlib
import itertools
import pathlib
import struct
import subprocess
import sys
import tempfile

HASHES = {
    'MiscHandler.cpp': '5ecb0556049a2658faccad0f9f97c3dcbcd39975e50dea203393f47b22caeb62',
    'Player.cpp': 'd02d693f2575b0f47be405fe6e81e2a4e833fca8d149354d9cb8d3b458527c7e',
    'Player.h': 'acd3ff726fafecd582673ec918157584d71fde6e3a1f59e26f0374b379cdffbb',
    'Object.cpp': '410904520fe2847a855c396d9fe442065268fd4ff5bb510633c3aadb5c54665b',
    'Object.h': '259cdb1a061258d6a8c3e977c743c8c83848c763353b504ca3e617580f58c128',
    'Corpse.h': 'ad58d5ae78d9fe40ee0a0ca0267c1dae5ac7f18d1dcc469881fb987d95443651',
}
HEADER = 'alive ghost present enabled expire delay_now compare_now ghost_time player_world corpse_world player_map corpse_map px_bits py_bits pz_bits cx_bits cy_bits cz_bits reach_bits accepted actions'


def body(source, signature):
    start = source.index(signature)
    end = source.index('{', start) + 1
    depth = 1
    while depth:
        depth += (source[end] == '{') - (source[end] == '}')
        end += 1
    return source[start:end] + '\n'


def bits(value):
    return struct.unpack('<I', struct.pack('<f', value))[0]


def cases():
    base = [0, 1, 1, 1, 1000, 1001, 1001, 900, 1, 1, 1, 1,
            bits(0), bits(0), bits(0), bits(0), bits(0), bits(0), bits(1.5)]
    rows = set()

    def add(**updates):
        row = base.copy()
        fields = HEADER.split()[:-2]
        for key, value in updates.items():
            row[fields.index(key)] = value
        rows.add(tuple(row))

    for alive, ghost, present, enabled in itertools.product(range(2), repeat=4):
        add(alive=alive, ghost=ghost, present=present, enabled=enabled)
    # Dynamic expiration steps, distinct clock samples, and unsigned narrowing
    # BEFORE division. All timestamps fit signed 64-bit time_t without overflow.
    for enabled in range(2):
        for delta in [0, 1, 299, 300, 301, 599, 600, 601, 900,
                      2**32-1, 2**32, 2**32+299, 2**32+300]:
            for elapsed in [29, 30, 31, 59, 60, 61, 119, 120, 121]:
                add(enabled=enabled, expire=1000+delta, delay_now=1000,
                    compare_now=1000+elapsed, ghost_time=1000)
    for delay_now in [999, 1000, 1001, 1299, 1300, 1301]:
        for compare_now in [1029, 1030, 1059, 1060, 1119, 1120]:
            add(expire=1300, delay_now=delay_now, compare_now=compare_now, ghost_time=1000)
    for pw, cw, pm, cm in itertools.product(range(2), range(2), [1, 2], [1, 2]):
        # Map tokens stand for exact map-instance pointers, not map IDs.
        add(player_world=pw, corpse_world=cw, player_map=pm, corpse_map=cm)
    for reach in [0.0, 1.5, 2.0, 100.0]:
        edge = bits(39.0 + reach)
        for coordinate in [edge-1, edge, edge+1]:
            for axis in ['px_bits', 'py_bits', 'pz_bits']:
                add(reach_bits=bits(reach), **{axis: coordinate})
    for xyz in [(23.4, 31.2, 0), (23.4, 0, 31.2), (20, 20, 20),
                (30, 30, 0), (-39, 0, 0), (1, 2, 3)]:
        for reach in [0, 1.5]:
            add(px_bits=bits(xyz[0]), py_bits=bits(xyz[1]), pz_bits=bits(xyz[2]), reach_bits=bits(reach))
            add(cx_bits=bits(xyz[0]), cy_bits=bits(xyz[1]), cz_bits=bits(xyz[2]), reach_bits=bits(reach))
    return sorted(rows)


HARNESS = r'''
#include <cassert>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <ctime>
#include <iostream>
#include <limits>
#include <string>
using uint32 = std::uint32_t;
static_assert(sizeof(time_t) == 8 && std::numeric_limits<time_t>::is_signed);
static_assert(sizeof(float) == 4 && std::numeric_limits<float>::is_iec559);
static time_t delay_now, compare_now;
static unsigned clock_reads;
static time_t oracle_time(std::nullptr_t) { return clock_reads++ == 0 ? delay_now : compare_now; }
#define time oracle_time
#define DETAIL_LOG(...) ((void)0)
constexpr int PLAYER_FLAGS=1, PLAYER_FLAGS_GHOST=2;
constexpr int CORPSE_RESURRECTABLE_PVP=2;
constexpr int CONFIG_BOOL_DEATH_CORPSE_RECLAIM_DELAY_PVP=1;
constexpr int CONFIG_BOOL_DEATH_CORPSE_RECLAIM_DELAY_PVE=2;
constexpr uint32 DEATH_EXPIRE_STEP=300, MAX_DEATH_COUNT=3;
static const uint32 corpseReclaimDelay[MAX_DEATH_COUNT]={30,60,120};
constexpr float CORPSE_RECLAIM_RADIUS=39;
struct World { bool enabled; bool getConfig(int id) const { assert(id==CONFIG_BOOL_DEATH_CORPSE_RECLAIM_DELAY_PVE); assert(clock_reads==1); return enabled; } } sWorld;
enum DistanceCalculation { DIST_CALC_NONE, DIST_CALC_BOUNDING_RADIUS, DIST_CALC_COMBAT_REACH, DIST_CALC_COMBAT_REACH_WITH_MELEE };
struct WorldObject {
    bool world; int map; float x,y,z,reach;
    float GetPositionX() const { return x; }
    float GetPositionY() const { return y; }
    float GetPositionZ() const { return z; }
    float GetCombatReach() const { return reach; }
    bool IsInWorld() const { return world; }
    int GetMap() const { return map; }
    float GetObjectBoundingRadius() const { assert(false); return 0; }
    float GetCombinedCombatReach(const WorldObject*, bool) const { assert(false); return 0; }
    float GetDistance(const WorldObject*, bool, DistanceCalculation) const;
    bool _IsWithinDist(const WorldObject*, float, bool) const;
    // INLINE_METHODS
};
struct Corpse : WorldObject {
    time_t ghost_time;
    time_t GetGhostTime() const { return ghost_time; }
    int GetType() const { return 1; }
};
static std::string actions;
struct Player : WorldObject {
    bool alive, ghost; Corpse* corpse; time_t m_deathExpireTime;
    bool IsAlive() const { return alive; }
    bool HasFlag(int field, int flag) const { assert(field==PLAYER_FLAGS && flag==PLAYER_FLAGS_GHOST); return ghost; }
    Corpse* GetCorpse() const { return corpse; }
    bool InBattleGround() const { return false; }
    uint32 getCorpseReclaimDelayHelper(time_t, time_t, bool) const;
    uint32 GetCorpseReclaimDelay(bool) const;
    void ResurrectPlayer(float percent, bool sickness=false) { assert(percent==0.5f && !sickness && actions.empty()); actions="resurrect_half_no_sickness"; }
    void SpawnCorpseBones() { assert(actions=="resurrect_half_no_sickness"); actions+=",bones"; }
};
struct ObjectGuid { uint32 token=0; };
struct WorldPacket { void operator>>(ObjectGuid& guid) { guid.token=777; } };
struct WorldSession {
    Player* player;
    Player* GetPlayer() const { return player; }
    void HandleReclaimCorpseOpcode(WorldPacket&);
};
// METHODS
static float from_bits(uint32 bits) { float value; std::memcpy(&value,&bits,4); assert(std::isfinite(value)); return value; }
int main() {
    std::uint64_t v[19];
    while (std::cin >> v[0]) {
        for (int i=1;i<19;++i) { std::cin >> v[i]; assert(std::cin); }
        Player p{}; Corpse c{};
        p.alive=v[0]; p.ghost=v[1]; p.corpse=v[2]?&c:nullptr; sWorld.enabled=v[3];
        p.m_deathExpireTime=v[4]; delay_now=v[5]; compare_now=v[6]; c.ghost_time=v[7];
        p.world=v[8]; c.world=v[9]; p.map=v[10]; c.map=v[11];
        p.x=from_bits(v[12]); p.y=from_bits(v[13]); p.z=from_bits(v[14]);
        c.x=from_bits(v[15]); c.y=from_bits(v[16]); c.z=from_bits(v[17]);
        p.reach=from_bits(v[18]); c.reach=0;
        clock_reads=0; actions.clear(); WorldSession session{&p}; WorldPacket packet;
        session.HandleReclaimCorpseOpcode(packet);
        assert(clock_reads==((!p.alive && p.ghost && p.corpse)?2u:0u));
        for (auto value:v) std::cout << value << '\t';
        std::cout << (!actions.empty()) << '\t' << (actions.empty()?"-":actions) << '\n';
    }
}
'''


def main():
    sources = {}
    for name, digest in HASHES.items():
        raw = pathlib.Path(sys.argv[1], name).read_bytes()
        if hashlib.sha256(raw).hexdigest() != digest:
            raise SystemExit('Wrong upstream revision: ' + name)
        sources[name] = raw.decode()
    assert 'void ResurrectPlayer(float restore_percent, bool applySickness = false);' in sources['Player.h']
    assert '#define CORPSE_RECLAIM_RADIUS 39' in sources['Corpse.h']
    assert '#define DEATH_EXPIRE_STEP (5*MINUTE)' in sources['Player.cpp']
    assert '#define MAX_DEATH_COUNT 3' in sources['Player.cpp']
    assert 'corpseReclaimDelay[MAX_DEATH_COUNT] = {30, 60, 120};' in sources['Player.cpp']
    assert 'virtual float GetCombatReach() const { return 0.f; }' in sources['Object.h']
    methods = ''.join(body(sources[name], signature) for name, signature in [
        ('MiscHandler.cpp', 'void WorldSession::HandleReclaimCorpseOpcode('),
        ('Player.cpp', 'uint32 Player::getCorpseReclaimDelayHelper('),
        ('Player.cpp', 'uint32 Player::GetCorpseReclaimDelay('),
        ('Object.cpp', 'float WorldObject::GetDistance(const WorldObject* obj,'),
        ('Object.cpp', 'bool WorldObject::_IsWithinDist('),
    ])
    inline = ''.join(body(sources['Object.h'], signature) for signature in [
        'bool IsInMap(const WorldObject* obj) const',
        'bool IsWithinDistInMap(WorldObject const* obj,',
    ])
    harness = HARNESS.replace('// INLINE_METHODS', inline).replace('// METHODS', methods)
    rows = cases()
    inputs = ''.join(' '.join(map(str, row))+'\n' for row in rows)
    with tempfile.TemporaryDirectory() as temp:
        root = pathlib.Path(temp)
        (root/'oracle.cpp').write_text(harness)
        subprocess.run(['g++', '-std=c++17', '-O0', '-Wall', '-Wextra', '-Werror',
                        '-ffp-contract=off', '-fsanitize=undefined', '-fno-sanitize-recover=all',
                        str(root/'oracle.cpp'), '-o', str(root/'oracle')], check=True)
        result = subprocess.check_output([str(root/'oracle')], input=inputs.encode()).decode()
    assert len(result.splitlines()) == len(rows)
    destination = pathlib.Path(__file__).resolve().parents[1]/'tests/fixtures/corpse_reclaim_cmangos.tsv'
    destination.write_text(HEADER.replace(' ', '\t')+'\n'+result)
    print(f'{len(rows)} rows -> {destination}')


if __name__ == '__main__':
    main()
