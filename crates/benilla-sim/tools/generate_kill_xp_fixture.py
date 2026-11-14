#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Hash-checked extracted CMaNGOS Classic 8ec338a1 kill/rest XP, synthetic only.
Usage: generate_kill_xp_fixture.py UPSTREAM_DIR [--check]
GiveLevel is a labeled trace adapter; the existing player progression oracle
separately extracts its complete body. No assets/database/native captures.
"""
import argparse
import hashlib
import pathlib
import subprocess
import tempfile
HASHES = {
    'Formulas.h': 'ca8b55fd022b0c0191255bc457574ccafce5eeb2f99a447466ba75be4717652c',
    'Player.cpp': 'd02d693f2575b0f47be405fe6e81e2a4e833fca8d149354d9cb8d3b458527c7e',
    'Unit.cpp': '03b67a4b6226f1f463e71be3694d3c950fc9b915a6e921407a98e4ff82fc21d0',
}
def body(text, signature):
    start = text.index(signature)
    brace = text.index('{', start)
    depth = 1
    end = brace + 1
    while depth:
        depth += (text[end] == '{') - (text[end] == '}')
        end += 1
    return text[start:end] + '\n'
def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('upstream', type=pathlib.Path)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    sources = {}
    for name, digest in HASHES.items():
        raw = (args.upstream / name).read_bytes()
        if hashlib.sha256(raw).hexdigest() != digest:
            raise SystemExit('Wrong upstream revision: ' + name)
        sources[name] = raw.decode()
    replacements = {
        'DAMAGE': ('Unit.cpp', 'uint32 Unit::GetModifierXpBasedOnDamageReceived('),
        'TRIVIAL': ('Formulas.h', 'inline bool IsTrivialLevelDifference('),
        'ZERO': ('Formulas.h', 'inline uint32 GetZeroDifference('),
        'BASE': ('Formulas.h', 'inline float BaseGain('),
        'GAIN': ('Formulas.h', 'inline uint32 Gain('),
        'REST_BONUS': ('Player.cpp', 'uint32 Player::GetXPRestBonus('),
        'SET_REST': ('Player.cpp', 'void Player::SetRestBonus('),
        'LOG': ('Player.cpp', 'void Player::SendLogXPGain('),
        'GIVE_XP': ('Player.cpp', 'void Player::GiveXP('),
        'COMPUTE_REST': ('Player.cpp', 'float Player::ComputeRest('),
    }
    harness = pathlib.Path(__file__).with_name('kill_xp_oracle.cpp').read_text()
    for key, (name, sig) in replacements.items():
        marker = '// PINNED_' + key + '\n'
        assert harness.count(marker) == 1
        harness = harness.replace(marker, body(sources[name], sig))
    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp)
        (root/'oracle.cpp').write_text(harness)
        subprocess.run(['g++','-std=c++17','-O0','-Wall','-Wextra','-Werror','-fno-fast-math','-ffp-contract=off','-frounding-math','-fsanitize=undefined,float-cast-overflow','-fno-sanitize-recover=all',str(root/'oracle.cpp'),'-o',str(root/'oracle')],check=True)
        data = subprocess.check_output([str(root/'oracle')])
    dest = pathlib.Path(__file__).resolve().parents[1]/'tests/fixtures/kill_xp_cmangos.tsv'
    if args.check:
        if dest.read_bytes()!=data: raise SystemExit('Fixture differs')
    else: dest.write_bytes(data)
    print(f'{len(data.splitlines())-1} rows {len(data)} bytes sha256={hashlib.sha256(data).hexdigest()}')
if __name__ == '__main__': main()
