#!/usr/bin/env python3
"""Extract unchanged pinned calendar bodies and compile in the approved F0 builder."""
# SPDX-License-Identifier: GPL-3.0-or-later
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

PIN = '8ec338a1704e7dcb1c0213eb7ed58f9231ade40f'
IMAGE = 'sha256:7f8acd0cd855b63046cfdf49f21ed319ae6b981758f5fafe2100aed8b4ef21e8'
HERE = Path(__file__).resolve().parent


def body(source, signature):
    assert source.count(signature) == 1, signature
    start = source.index(signature)
    depth = 0
    for end in range(source.index('{', start), len(source)):
        depth += (source[end] == '{') - (source[end] == '}')
        if depth == 0:
            return source[start:end+1]
    raise ValueError(signature)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path)
    parser.add_argument('evidence', type=Path)
    parser.add_argument('--private-cases', type=Path)
    args = parser.parse_args()
    assert subprocess.check_output(['git', '-C', str(args.source), 'rev-parse', 'HEAD'], text=True).strip() == PIN
    work = args.evidence.resolve()
    work.mkdir(parents=True, exist_ok=True)
    manager_path = args.source/'src/game/GameEvents/GameEventMgr.cpp'
    manager = manager_path.read_text()
    header = (args.source/'src/game/GameEvents/GameEventMgr.h').read_text()
    definitions = body(header, 'enum GameEventScheduleType')+';\n'+body(header, 'struct GameEventData')+';\n'
    # The oracle includes the original Moon only from the user's read-only checkout.
    # Its source is never copied into public fixtures or translated by this script.
    prefix = '#include <cmath>\n#include <ctime>\n#include <tuple>\n#include <sstream>\n#include <iomanip>\n#include <iostream>\n#include <cstdint>\n#include "moon.h"\nusing uint32=uint32_t; using HolidayIds=uint32_t; const uint32_t HOLIDAY_NONE=0; using std::tm;\n'
    source = prefix+definitions+'class GameEventMgr { public: void ComputeEventStartAndEndTime(GameEventData&,time_t); };\n'
    source += body(manager, 'std::tuple<int, int, int> gaussEaster(int Y)')+'\n'
    source += body(manager, 'void GameEventMgr::ComputeEventStartAndEndTime(GameEventData& data, time_t today)')+'\n'
    source += r'''
static void fields(const tm& t) { std::cout<<' '<<t.tm_sec<<' '<<t.tm_min<<' '<<t.tm_hour<<' '<<t.tm_mday<<' '<<t.tm_mon<<' '<<t.tm_year<<' '<<t.tm_isdst<<' '<<t.tm_wday<<' '<<t.tm_yday; }
int main() {
    char kind;
    while(std::cin>>kind) {
        if(kind=='F') {
            std::string hex,s; std::cin>>hex;
            if(hex!="-") for(size_t i=0;i<hex.size();i+=2) s.push_back(char(std::stoul(hex.substr(i,2),nullptr,16)));
            s=std::string(s.c_str());
            tm value={}; std::stringstream stream(s); stream>>std::get_time(&value,"%Y-%m-%d %H:%M:%S");
            bool valid=!stream.fail(); size_t consumed=s.size()-stream.rdbuf()->in_avail();
            tm before=value; time_t result=std::mktime(&value);
            std::cout<<"F "<<result<<' '<<valid<<' '<<consumed; fields(before); fields(value); std::cout<<'\n';
        } else if(kind=='E') {
            int y,m,d; std::cin>>y; std::tie(y,m,d)=gaussEaster(y); std::cout<<"E "<<y<<' '<<m<<' '<<d<<'\n';
        } else if(kind=='S') {
            GameEventData value; unsigned schedule; time_t today;
            std::cin>>schedule>>value.start>>value.end>>today; value.scheduleType=GameEventScheduleType(schedule);
            GameEventMgr().ComputeEventStartAndEndTime(value,today); std::cout<<"S "<<value.start<<' '<<value.end<<'\n';
        } else return 2;
    }
}
'''
    (work/'oracle.cpp').write_text(source)
    dates = ['', 'bad', '2024', '2024-', '2024-02', '2024-02-', '2024-02-29', '2024-02-29 ', '2024-02-29 12', '2024-02-29 12:', '2024-02-29 12:34', '2024-02-29 12:34:', '2024-02-29 12:34:56', '2024-02-30 12:34:56 trailing', '2024-13-01 00:00:00', '2024-02-00 00:00:00', '2024-02-29 24:00:00', '2024-02-29 12:60:00', '2024-02-29 12:34:60', '2024-02-29 12:34:61', ' 2024-02-29 12:34:56', '2024- 2- 9 1:2:3', '0-1-1 0:0:0', '9999-12-31 23:59:59']
    dates.extend(['2024-02-29 12:34:56'+chr(0)+'ignored', '2024-02'+chr(0)+'-29 12:34:56'])
    cases = [['F', s] for s in dates] + [['E', y] for y in range(1583,2501)]
    # January/February/leap/year-boundary epochs, independently authored operands.
    for today in [0,1704067200,1706659200,1709164800,1735603200]:
        for schedule in [0,1,2,3,4,5,6,7,8,9,10,11,13,14,15]:
            cases.append(['S',schedule,1582945445,1583031845,today])
    if args.private_cases:
        cases = json.loads(args.private_cases.read_text())
    lines = [('F '+(c[1].encode().hex() or '-')) if c[0]=='F' else ' '.join(map(str,c)) for c in cases]
    (work/'inputs.txt').write_text('\n'.join(lines)+'\n')
    compile_args=['g++','-std=c++17','-O3','-DNDEBUG','-ffp-contract=off','-fno-fast-math','-fno-omit-frame-pointer','-g','-I/source/src/game/GameEvents','/evidence/oracle.cpp','/source/src/game/GameEvents/moon.cpp','-o','/evidence/oracle']
    base=['docker','run','--rm','-i','--network=none','--user',f'{__import__("os").getuid()}:{__import__("os").getgid()}','--env','TZ=UTC','--env','LC_ALL=C','--mount',f'type=bind,src={args.source.resolve()},dst=/source,readonly','--mount',f'type=bind,src={work},dst=/evidence',IMAGE]
    subprocess.run(base+compile_args,check=True)
    result=subprocess.check_output(base+['/evidence/oracle'],input=(work/'inputs.txt').read_bytes())
    assert len(result.splitlines())==len(cases)
    fixtures=work if args.private_cases else HERE/'fixtures/source_calendar'
    fixtures.mkdir(parents=True,exist_ok=True)
    (fixtures/'cases.json').write_text(json.dumps(cases,separators=(',',':'))+'\n')
    (fixtures/'expected.tsv').write_bytes(result)
    receipt={'image':IMAGE,'core_revision':PIN,'compile_command':compile_args,'cases':len(cases),'source_sha256':{str(p.relative_to(args.source)):hashlib.sha256(p.read_bytes()).hexdigest() for p in [manager_path,args.source/'src/game/GameEvents/GameEventMgr.h',args.source/'src/shared/Database/Field.cpp',args.source/'src/shared/Database/Field.h',args.source/'src/game/GameEvents/moon.cpp',args.source/'src/game/GameEvents/moon.h']},'oracle_sha256':hashlib.sha256(source.encode()).hexdigest(),'output_sha256':hashlib.sha256(result).hexdigest(),'scope':'Authored calendar component fixtures, not historical dispatch or sealed execution'}
    if args.private_cases:
        receipt['scope']='Unsealed raw stock calendar component fixtures, not effective definitions or F1 acceptance'
    (fixtures/'provenance.json').write_text(json.dumps(receipt,indent=2)+'\n')
    (work/'oracle-receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
    print(json.dumps(receipt))


if __name__=='__main__':
    main()
