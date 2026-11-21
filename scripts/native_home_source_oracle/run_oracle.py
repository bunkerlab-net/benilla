#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Independent selected-source Home lifecycle oracle; no Rust or game data."""
import argparse
import difflib
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

HERE=Path(__file__).resolve().parent
PIN='8ec338a1704e7dcb1c0213eb7ed58f9231ade40f'
CLASSIC_DB_PIN='ec4f596146be6467ea93c57397858e329e2db852'
SOURCE_MANIFEST_SHA256='a2f3ef44a3b820e72d910952ecf8248ef7ef3912462697c34a1faf3b446775ca'
POLICY='cmangos-buildpolypath-zero-distance-v1'
PATH='src/game/MotionGenerators/PathFinder.cpp'
HEADER='src/game/MotionGenerators/PathFinder.h'
RANDOM='src/game/MotionGenerators/RandomMovementGenerator.cpp'
RANDOM_HEADER='src/game/MotionGenerators/RandomMovementGenerator.h'
HOME='src/game/MotionGenerators/HomeMovementGenerator.cpp'
MOTION='src/game/MotionGenerators/MotionMaster.cpp'
DETOUR=('DetourAlloc','DetourAssert','DetourCommon','DetourNavMesh','DetourNavMeshBuilder','DetourNavMeshQuery','DetourNode')
def sha(data): return hashlib.sha256(data).hexdigest()
def exact(text,signature,semicolon=False):
    if text.count(signature)!=1: raise ValueError('nonunique source signature: '+signature)
    first=text.index(signature); last=text.index('{',first); depth=0
    while True:
        depth+=(text[last]=='{')-(text[last]=='}');last+=1
        if not depth: break
    if semicolon:
        assert text[last]==';';last+=1
    return first,last

def extract(core):
    if sha((HERE/'source_hashes.sha256').read_bytes()) != SOURCE_MANIFEST_SHA256:
        raise ValueError('pinned source manifest hash mismatch')
    sources={}
    for line in (HERE/'source_hashes.sha256').read_text().splitlines():
        if not line or line.startswith('#'): continue
        digest,name=line.split();raw=(core/name).read_bytes().replace(b'\r\n',b'\n')
        if sha(raw)!=digest: raise ValueError('pinned source hash mismatch: '+name)
        sources[name]=raw.decode('latin1')
    records=[];excerpts={}
    def interval(name,file,first,last,compiled=True):
        text=sources[file];data=text[first:last];excerpts[name]=data
        records.append(dict(name=name,source=file,first_byte=first,last_byte_exclusive=last,first_line=text[:first].count('\n')+1,last_line=text[:last].count('\n')+(not text[:last].endswith('\n')),sha256=sha(data.encode('latin1')),compiled=compiled))
        return data
    def method(name,file,sig,semicolon=False,compiled=True):
        return interval(name,file,*exact(sources[file],sig,semicolon),compiled=compiled)
    method('PathType.inc',HEADER,'enum PathType\n',True)
    method('UnitState.inc','src/game/Entities/Unit.h','enum UnitState\n',True)
    method('ShortTimeTracker.inc','src/shared/Util/Timer.h','struct ShortTimeTracker\n',True)
    method('RandomDecl.inc',RANDOM_HEADER,'class AbstractRandomMovementGenerator :',True)
    method('WanderDecl.inc',RANDOM_HEADER,'class WanderMovementGenerator :',True)
    method('HomeDecl.inc','src/game/MotionGenerators/HomeMovementGenerator.h','template <>\nclass HomeMovementGenerator',True)
    method('MediumDecl.inc','src/game/MotionGenerators/MovementGenerator.h','template<class T, class D>\nclass MovementGeneratorMedium',True)
    for name,sig in [('RandomInitialize','void AbstractRandomMovementGenerator::Initialize('),('RandomFinalize','void AbstractRandomMovementGenerator::Finalize('),('RandomInterrupt','void AbstractRandomMovementGenerator::Interrupt('),('RandomReset','void AbstractRandomMovementGenerator::Reset('),('WanderConstructor','WanderMovementGenerator::WanderMovementGenerator(const Creature& npc)'),('WanderInterrupt','void WanderMovementGenerator::Interrupt('),('WanderFinalize','void WanderMovementGenerator::Finalize(')]: method(name+'.inc',RANDOM,sig)
    for name,sig in [('HomeInitialize','void HomeMovementGenerator<Creature>::Initialize('),('HomeReset','void HomeMovementGenerator<Creature>::Reset('),('HomeSetTarget','void HomeMovementGenerator<Creature>::_setTargetLocation('),('HomeUpdate','bool HomeMovementGenerator<Creature>::Update('),('HomeFinalize','void HomeMovementGenerator<Creature>::Finalize(')]: method(name+'.inc',HOME,sig)
    for name,sig in [('DirectClean','void MotionMaster::DirectClean('),('DelayedClean','void MotionMaster::DelayedClean('),('DirectExpire','void MotionMaster::DirectExpire('),('DelayedExpire','void MotionMaster::DelayedExpire('),('Mutate','void MotionMaster::Mutate('),('MoveHome','void MotionMaster::MoveTargetedHome('),('UpdateMotion','void MotionMaster::UpdateMotion(')]: method(name+'.inc',MOTION,sig)
    for name,sig in [('ClearMotion','        void Clear(bool reset'),('ExpireMotion','        void MovementExpired(bool reset')]:method(name+'.inc','src/game/MotionGenerators/MotionMaster.h',sig)
    for name,sig in [('PathConstructor','PathFinder::PathFinder(const Unit* owner,'),('SelectNav','void PathFinder::SetCurrentNavMesh('),('Calculate','bool PathFinder::calculate(Vector3 const& start,'),('GetPathPolyByPosition','dtPolyRef PathFinder::getPathPolyByPosition('),('GetPolyByLocation','dtPolyRef PathFinder::getPolyByLocation('),('BuildShortcut','void PathFinder::BuildShortcut('),('NormalizePath','void PathFinder::NormalizePath('),('HaveTile','bool PathFinder::HaveTile(')]:method(name+'.inc',PATH,sig)
    method('BuildPolyPath.context.inc',PATH,'void PathFinder::BuildPolyPath(',compiled=False)
    method('BuildPointPath.context.inc',PATH,'void PathFinder::BuildPointPath(',compiled=False)
    text=sources[PATH]
    original=interval('BuildPolyPath.original-prefix.inc',PATH,text.index('void PathFinder::BuildPolyPath('),text.index('    // look for startPoly/endPoly in current path'),False)
    needle='    float distToStartPoly, distToEndPoly;'
    assert original.count(needle)==1
    patched=original.replace(needle,'    float distToStartPoly = 0.0f, distToEndPoly = 0.0f;')
    excerpts['BuildPolyPath.selected.inc']=patched+'    throw std::runtime_error("outside same-polygon oracle scope");\n}\n'
    patch=''.join(difflib.unified_diff(original.splitlines(True),patched.splitlines(True),fromfile='BuildPolyPath.original-prefix.inc',tofile='BuildPolyPath.compatibility-prefix.inc'))
    constants=sources[HEADER]
    interval('SearchBounds.inc',HEADER,constants.index('static float NearPolySearchBound'),constants.index('\nenum PathType'))
    common=sources['src/shared/Common.h'];start=common.index('#ifndef M_PI_F');interval('Pi.inc','src/shared/Common.h',start,common.index('#endif',start)+6)
    method('Clear.inc',HEADER,'        void clear()')
    method('InterruptMoving.inc','src/game/Entities/Unit.cpp','void Unit::InterruptMoving(')
    launch='src/game/Movement/MoveSplineInit.cpp';text=sources[launch]
    start=text.index('        MoveSpline& move_spline = *unit.movespline;')
    end=text.index('        args.flags.enter_cycle = args.flags.cyclic;',start)
    interval('LaunchStart.prefix.inc',launch,start,end)
    unit=sources['src/game/Entities/Unit.cpp'];start=unit.index('        if (creature->GetCombatStartPosition().IsEmpty())')
    interval('CombatCapture.inc','src/game/Entities/Unit.cpp',start,unit.index('\n\n',start))
    method('Position.inc','src/game/Entities/Object.h','struct Position\n',True)
    method('Location.inc','src/game/Movement/MoveSpline.h','    struct Location : public Vector3',True)
    method('GetDistance.context.inc','src/game/Entities/Object.cpp','float WorldObject::GetDistance(float x,',compiled=False)
    verify_contract(excerpts)
    return sources,excerpts,records,patch

def verify_contract(excerpts):
    """Discriminating placement guards, in addition to whole-source hashes."""
    def ordered(name, *tokens):
        text = excerpts[name]
        offset = 0
        for token in tokens:
            found = text.find(token, offset)
            if found < 0:
                raise ValueError('source contract changed: '+name+': '+token)
            offset = found + len(token)
    ordered('HomeSetTarget.inc',
        'if (owner.hasUnitState(UNIT_STAT_NOT_MOVE))',
        'top()->GetResetPosition(owner, pos.x, pos.y, pos.z, pos.o)',
        'owner.GetCombatStartPosition(pos);', 'if (pos.IsEmpty())',
        'owner.GetRespawnCoord(pos.x, pos.y, pos.z, &pos.o);',
        'owner.GetDistance(pos.GetPositionX(), pos.GetPositionY(), pos.GetPositionZ(), DIST_CALC_NONE, owner.GetTransport()) > 150.f * 150.f',
        'if (!owner.IsInWorld() || !owner.GetMap()->IsDungeon())',
        'arrived = true;', 'Finalize(owner);', 'owner.SetRespawnDelay(5, true);',
        'owner.ForcedDespawn(1);', 'PathFinder path(&owner);',
        'Position curPos = owner.GetPosition(owner.GetTransport());',
        'path.calculate(G3D::Vector3(curPos.GetPositionX(), curPos.GetPositionY(), curPos.GetPositionZ()), G3D::Vector3(pos.GetPositionX(), pos.GetPositionY(), pos.GetPositionZ()), true);',
        'init.SetWalk(!runHome);', 'init.SetFacing(pos.GetPositionO());',
        'if (path.getPathType() & (PATHFIND_NOPATH | PATHFIND_SHORTCUT))',
        'init.SetVelocity(400.f);', 'init.Launch();', 'arrived = false;',
        'owner.clearUnitState(static_cast<uint32>(UNIT_STAT_ALL_DYN_STATES));')
    ordered('HomeFinalize.inc', 'SetEvadeState(EVADE_NONE);', 'if (arrived)',
        'owner.LoadCreatureAddon(true);', 'owner.TriggerHomeEvents();',
        'if (!wasActive && owner.IsInWorld())', 'owner.SetActiveObjectState(false);',
        'owner.SetCombatStartPosition(Position());')
    ordered('MoveHome.inc', 'UNIT_STAT_LOST_CONTROL', 'Clear(false);',
        'Mutate(new HomeMovementGenerator<Creature>(runHome));')
    ordered('Mutate.inc', 'MovementExpired(false);', 'top()->Interrupt(*m_owner);',
        'm->Initialize(*m_owner);', 'push(m);')
    ordered('DirectExpire.inc', 'MovementGenerator* nowTop = empty() ? nullptr : top();',
        'curr->Finalize(*m_owner);', 'if (reset && top() == nowTop)', 'top()->Reset(*m_owner);')
    ordered('RandomReset.inc', 'i_nextMoveTimer.Reset(0);', 'Initialize(owner);')
    ordered('WanderInterrupt.inc', 'AbstractRandomMovementGenerator::Interrupt(owner);',
        'SetWalk(!owner.hasUnitState(UNIT_STAT_RUNNING_STATE), false);')
    ordered('InterruptMoving.inc', 'if (!movespline->Finalized())',
        'UpdateSplinePosition(true);', 'movespline->_Interrupt();', 'StopMoving(forceSendStop || isMoving);')
    ordered('LaunchStart.prefix.inc', 'if (!move_spline.Finalized())',
        'real_position = move_spline.ComputePosition();', 'args.path[0] = real_position;')
    ordered('Calculate.inc', 'm_forceDestination = forceDest;',
        'm_straightLine = straightLine;', 'BuildPolyPath(start, dest);')
    ordered('BuildPolyPath.original-prefix.inc', 'if (startPoly == endPoly)',
        'm_type = farFromPoly ? PATHFIND_INCOMPLETE : PATHFIND_NORMAL;', 'return;')
    ordered('BuildPolyPath.context.inc', 'if (startPoly == endPoly)',
        '// look for startPoly/endPoly in current path', 'BuildPointPath(')
    ordered('BuildPointPath.context.inc', 'if (m_forceDestination &&',
        'm_type = PathType(PATHFIND_NORMAL | PATHFIND_NOT_USING_PATH);')
    ordered('GetDistance.context.inc', 'float dist = dx * dx + dy * dy + dz * dz;', 'default: return dist;')
    original = excerpts['BuildPolyPath.original-prefix.inc']
    needle = '    float distToStartPoly, distToEndPoly;'
    if original.count(needle) != 1:
        raise ValueError('source contract changed: original cold distance locals')
    selected = original.replace(needle, '    float distToStartPoly = 0.0f, distToEndPoly = 0.0f;')
    selected += '    throw std::runtime_error("outside same-polygon oracle scope");\n}\n'
    if excerpts['BuildPolyPath.selected.inc'] != selected:
        raise ValueError('source contract changed: sole cold-distance compatibility patch')

def source_package(sources, excerpts):
    package='// SPDX-License-Identifier: GPL-3.0-or-later\n// CMaNGOS '+PIN+'\n'+sources[PATH][:sources[PATH].index('*/')+2]+'\n'
    for name,text in excerpts.items():package+='\n// EXCERPT '+name+'\n'+text+'\n'
    return package

def verify_bundle(sources, excerpts, records, patch, bundle=HERE):
    """Reject bundled tampering against the pinned checkout before compiling."""
    for name, expected in [('source_excerpts.test-fixture', source_package(sources, excerpts)),
                           ('cold-distance-zero.patch', patch)]:
        if (bundle/name).read_text() != expected:
            raise ValueError('bundled source mismatch: '+name)
    metadata = json.loads((bundle/'source_intervals.json').read_text())
    if (metadata['source_intervals'] != records or metadata['source_revision'] != PIN
            or metadata['source_manifest_sha256'] != SOURCE_MANIFEST_SHA256):
        raise ValueError('bundled source metadata mismatch')

def command(run,name,args):
    (run/(name+'.command.json')).write_text(json.dumps(args,indent=2)+'\n')
    result=subprocess.run(args,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
    (run/(name+'.stdout')).write_bytes(result.stdout);(run/(name+'.stderr')).write_bytes(result.stderr)
    (run/(name+'.returncode')).write_text(str(result.returncode)+'\n')
    if result.returncode: raise RuntimeError(f'{name} failed ({result.returncode}); see {run}')
    return result.stdout

def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--core',type=Path,required=True);parser.add_argument('--output',type=Path,required=True)
    mode=parser.add_mutually_exclusive_group();mode.add_argument('--write-bundled',action='store_true');mode.add_argument('--check-bundled',action='store_true');args=parser.parse_args()
    args.output.mkdir(parents=True,exist_ok=True);number=1
    while (args.output/f'{number:03}').exists():number+=1
    run=(args.output/f'{number:03}').resolve();run.mkdir()
    try:
        sources,excerpts,records,patch=extract(args.core)
        if args.check_bundled:verify_bundle(sources,excerpts,records,patch)
        compiler=shutil.which('g++')
        if not compiler:raise RuntimeError('required g++ absent')
        version=command(run,'compiler-version',[compiler,'-dumpfullversion']).decode().strip();target=command(run,'compiler-target',[compiler,'-dumpmachine']).decode().strip()
        if (version,target)!=('14.2.0','x86_64-linux-gnu'):raise RuntimeError('unsupported audited compiler profile')
        for name,text in excerpts.items():(run/name).write_text(text,encoding='latin1')
        for name,text in sources.items():
            if name.startswith(('dep/recastnavigation/','dep/g3dlite/')):
                dest=run/name;dest.parent.mkdir(parents=True,exist_ok=True);dest.write_text(text,encoding='latin1')
        shutil.copyfile(HERE/'oracle_driver.cpp',run/'oracle_driver.cpp');detour=run/'dep/recastnavigation/Detour'
        flags=['-std=c++20','-O2','-fno-inline','-march=x86-64','-mno-fma','-fno-fast-math','-ffp-contract=off','-frounding-math','-DNDEBUG','-DDT_POLYREF64=1','-Wall','-Wextra','-Werror','-Wno-class-memaccess','-Wno-deprecated-copy','-Wno-unused-variable','-Wno-unused-function','-Wno-implicit-fallthrough','-fsanitize=undefined,float-cast-overflow','-fno-sanitize-recover=all']
        command(run,'compile',[compiler,*flags,'-I'+str(detour/'Include'),'-I'+str(run/'dep/g3dlite'),str(run/'oracle_driver.cpp'),*[str(detour/f'Source/{n}.cpp') for n in DETOUR],'-o',str(run/'home-oracle')])
        symbols=command(run,'symbols',['nm','-C',str(run/'home-oracle')]).decode()
        for signature in ['PathFinder::calculate(','HomeMovementGenerator<Creature>::_setTargetLocation(','MotionMaster::Mutate(','MotionMaster::DirectExpire(','AbstractRandomMovementGenerator::Reset(','dtNavMeshQuery::findNearestPoly(','dtCreateNavMeshData(']:
            if signature not in symbols:raise RuntimeError('missing source symbol: '+signature)
        output=command(run,'execute',[str(run/'home-oracle'),str(run)]);rows=[json.loads(line) for line in output.decode().splitlines()]
        vector=dict(schema=1,source_revision=PIN,compatibility_policy=POLICY,snapshots=rows)
        metadata=dict(source_revision=PIN,classic_db_revision=CLASSIC_DB_PIN,classic_db_rows_consumed=False,compatibility_policy=POLICY,original_cold_execution=False,wander_endpoint_repair_applied=False,compiler=version,target=target,source_intervals=records,source_manifest_sha256=sha((HERE/'source_hashes.sha256').read_bytes()),geometry_fixtures_sha256={name:sha((run/name).read_bytes()) for name in ['params.test-fixture','quad.test-fixture']},tile_lifetime_profile='unchanged-original-empirical; existing dtMeshTile raw-storage lifetime caveat',external_leaves=['owner flags, positions, squared DIST_CALC_NONE distance, lifecycle callbacks','constructor-enabled source query selection and filter','spline ComputePosition and Launch duration','Home event and addon callbacks','collision and real world membership not claimed'],excluded=['production admission','world data compiler','actual world membership','full spline interpolation or packet output','cross-polygon routes','original undefined cold parity'])
        package=source_package(sources,excerpts)
        bundled={'vectors.json':json.dumps(vector,indent=2)+'\n','source_intervals.json':json.dumps(metadata,indent=2)+'\n','source_excerpts.test-fixture':package,'cold-distance-zero.patch':patch}
        for name,text in bundled.items():
            (run/name).write_text(text)
            if args.write_bundled:(HERE/name).write_text(text)
            if args.check_bundled and (HERE/name).read_text()!=text:raise RuntimeError('bundled differs: '+name)
        report=dict(cases=len(rows),result='passed exact selected-source assertions',sanitizer_stderr_bytes=(run/'execute.stderr').stat().st_size,run=str(run))
        (run/'summary.json').write_text(json.dumps(report,indent=2)+'\n');(run/'manifest.json').write_text(json.dumps({p.relative_to(run).as_posix():sha(p.read_bytes()) for p in sorted(run.rglob('*')) if p.is_file()},indent=2)+'\n');print(json.dumps(report,indent=2))
    except Exception as error:
        (run/'FAILED.txt').write_text(str(error)+'\n');raise
if __name__=='__main__':main()
