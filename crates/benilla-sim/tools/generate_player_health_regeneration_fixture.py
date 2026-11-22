#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Extract pinned standing-mage regen/ModifyHealth; synthetic numeric corpus, no scheduler."""
import argparse
import hashlib
import pathlib
import subprocess
import tempfile
from generate_player_derived_fixture import HASHES, body

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('upstream',type=pathlib.Path)
    parser.add_argument('--check',action='store_true')
    args=parser.parse_args()
    source={}
    for name in ['Player.cpp','Unit.cpp','Unit.h']:
        raw=(args.upstream/name).read_bytes()
        assert hashlib.sha256(raw).hexdigest()==HASHES[name],name
        source[name]=raw.decode()
    assert 'float(GetUInt32Value(static_cast<uint16>(UNIT_FIELD_STAT0) + static_cast<uint16>(stat)))' in source['Unit.h']
    chunks=[body(source['Player.cpp'],'void Player::RegenerateHealth('),
            body(source['Unit.cpp'],'float Unit::OCTRegenHPPerSpirit()'),
            body(source['Unit.cpp'],'int32 Unit::ModifyHealth(')]
    harness=pathlib.Path(__file__).with_name('player_health_regeneration_oracle.cpp').read_text()
    assert harness.count('// PINNED_BODIES\n')==1
    harness=harness.replace('// PINNED_BODIES\n','\n'.join(chunks))
    with tempfile.TemporaryDirectory() as directory:
        tmp=pathlib.Path(directory)
        (tmp/'oracle.cpp').write_text(harness)
        subprocess.run(['g++','-std=c++17','-O0','-Wall','-Wextra','-Werror','-fno-fast-math','-ffp-contract=off','-frounding-math','-fsanitize=undefined,float-cast-overflow',str(tmp/'oracle.cpp'),'-o',str(tmp/'oracle')],check=True)
        data=subprocess.check_output([str(tmp/'oracle')])
    target=pathlib.Path(__file__).resolve().parents[1]/'tests/fixtures/player_health_regeneration_cmangos.tsv'
    if args.check: assert target.read_bytes()==data,'fixture changed'
    else: target.write_bytes(data)
    print(len(data.splitlines()),hashlib.sha256(data).hexdigest())
if __name__=='__main__':main()
