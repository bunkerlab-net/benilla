#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Extract exact pinned source bodies, compile independently, save synthetic numeric outputs."""
import hashlib
import pathlib
import subprocess
import sys
import tempfile
HASHES = {
    'Unit.cpp': '03b67a4b6226f1f463e71be3694d3c950fc9b915a6e921407a98e4ff82fc21d0',
    'Object.cpp': '410904520fe2847a855c396d9fe442065268fd4ff5bb510633c3aadb5c54665b',
    'ThreatManager.cpp': 'f3619f175b38b1fa7e3ece0cf662dc7804cf1d6f70756e31225242c833002460',
}
def body(text, signature):
    start = text.index(signature)
    end = text.index('{', start) + 1
    depth = 1
    while depth:
        depth += (text[end] == '{') - (text[end] == '}')
        end += 1
    return text[start:end] + '\n'
def main():
    sources = {}
    for name, digest in HASHES.items():
        data = pathlib.Path(sys.argv[1], name).read_bytes()
        if hashlib.sha256(data).hexdigest() != digest:
            raise SystemExit('Wrong upstream revision: ' + name)
        sources[name] = data.decode()
    methods = ''.join(body(sources[file], signature) for file, signature in [
        ('Unit.cpp', 'float Unit::GetAttackDistance(Unit const* target) const'),
        ('Unit.cpp', 'bool Unit::CanReachWithMeleeAttack(Unit const* pVictim, float flat_mod /*= 0.0f*/) const'),
        ('Unit.cpp', 'bool Unit::UpdateMeleeAttackingState()'),
        ('Object.cpp', 'float WorldObject::GetCombinedCombatReach(WorldObject const* pVictim, bool forMeleeRange, float flat_mod) const'),
        ('Object.cpp', 'float WorldObject::GetCombinedCombatReach(bool forMeleeRange, float flat_mod) const'),
        ('Object.cpp', 'float WorldObject::GetDistance2d(float x, float y, DistanceCalculation distcalc) const'),
        ('ThreatManager.cpp', 'HostileReference* ThreatContainer::selectNextVictim(Unit* attacker, HostileReference* currentVictim)'),
    ])
    harness = pathlib.Path(__file__).with_name('creature_ai_oracle.cpp').read_text()
    harness = harness.replace('// PINNED_METHODS', methods).replace('// PINNED_ENUM', body(sources['Unit.cpp'], 'enum SwingErrors') + ';')
    with tempfile.TemporaryDirectory() as temp:
        root = pathlib.Path(temp)
        (root/'oracle.cpp').write_text(harness)
        subprocess.run(['g++', '-std=c++17', '-O0', '-fno-fast-math', '-ffp-contract=off', '-frounding-math', '-fsanitize=undefined,float-cast-overflow', '-fno-sanitize-recover=all', str(root/'oracle.cpp'), '-o', str(root/'oracle')], check=True)
        data = subprocess.check_output([str(root/'oracle')])
    target = pathlib.Path(__file__).resolve().parents[1]/'tests/fixtures/creature_ai_cmangos.tsv'
    target.write_bytes(data)
    print(len(data.splitlines()), hashlib.sha256(data).hexdigest())
if __name__ == '__main__':
    main()
