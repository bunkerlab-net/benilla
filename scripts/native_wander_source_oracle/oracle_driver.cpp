// SPDX-License-Identifier: GPL-3.0-or-later
// Test-only adapters. Every included source body is extracted and hash recorded.
#include "G3D/Vector3.h"
#include "DetourNavMeshBuilder.h"
#include "DetourNavMesh.h"
#include "DetourNavMeshQuery.h"
#include "DetourCommon.h"
#include "DetourMath.h"
#include <algorithm>
#include <array>
#include <bit>
#include <cfenv>
#include <cfloat>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <limits>
#include <memory>
#include <sstream>
#include <stdexcept>
#include <string>
#include <type_traits>
#include <vector>
#include <xmmintrin.h>
using uint32=std::uint32_t;
using int32=std::int32_t;
using G3D::Vector3;
using PointsArray=std::vector<Vector3>;
static_assert(FLT_EVAL_METHOD==0 && sizeof(float)==4);
static_assert(std::is_same_v<decltype(cos(0.f)),float>);
#include "Pi.inc"
#define DEBUG_FILTER_LOG(...) ((void)0)
#define VERTEX_SIZE 3
#define INVALID_POLYREF 0
#define MAX_POINT_PATH_LENGTH 74
#define SMOOTH_PATH_STEP_SIZE 4.0f
#include "SearchBounds.inc"
#include "PathType.inc"
#include "UnitState.inc"
#include "ShortTimeTracker.inc"
[[noreturn]] static void fail(const char* message) { throw std::runtime_error(message); }
static void require(bool yes,const char* message) { if(!yes) fail(message); }
static uint32 bits(float x) { return std::bit_cast<uint32>(x); }
static std::vector<std::string> events;
static void event(const std::string& text) { events.push_back(text); }
struct Draw { bool normalized; uint32 low,high,value; };
static std::vector<Draw> draws;
static size_t drawIndex=0;
static float rand_norm_f() {
    require(drawIndex<draws.size(),"unexpected normalized RNG request");
    const auto d=draws[drawIndex++];require(d.normalized,"RNG order mismatch");
    const float value=std::bit_cast<float>(d.value);require(value>=0&&value<1,"normalized RNG must be in [0,1)");
    event("rng:normalized:"+std::to_string(d.value));return value;
}
static uint32 urand(uint32 low,uint32 high) {
    require(drawIndex<draws.size(),"unexpected integer RNG request");
    const auto d=draws[drawIndex++];require(!d.normalized&&d.low==low&&d.high==high&&d.value>=low&&d.value<=high,"RNG range/order mismatch");
    event("rng:integer:"+std::to_string(low)+":"+std::to_string(high)+":"+std::to_string(d.value));return d.value;
}
static Draw norm(float value) { return {true,0,0,bits(value)}; }
static Draw integer(uint32 low,uint32 high,uint32 value) { return {false,low,high,value}; }
static std::string fixtureDirectory;
static void write_fixture(const char* name,const void* data,size_t size) {
    const std::string path=fixtureDirectory+"/"+name;
    FILE* out=std::fopen(path.c_str(),"wb");require(out,"open authored fixture");
    require(std::fwrite(data,1,size,out)==size,"write authored fixture");
    require(std::fclose(out)==0,"close authored fixture");
}
static std::vector<unsigned char> read_fixture(const char* path) {
    FILE* input=std::fopen(path,"rb");require(input,"open unchanged fixture");
    require(std::fseek(input,0,SEEK_END)==0,"seek fixture");const long size=std::ftell(input);
    require(size>0&&std::fseek(input,0,SEEK_SET)==0,"fixture size");
    std::vector<unsigned char> bytes(static_cast<size_t>(size));
    require(std::fread(bytes.data(),1,bytes.size(),input)==bytes.size(),"read fixture");
    require(std::fclose(input)==0,"close fixture");return bytes;
}
struct AuthoredMesh {
    dtNavMesh mesh;
    dtNavMeshQuery query;
    void* tileOwner=nullptr;dtTileRef tileRef=0;
    AuthoredMesh() {
        dtNavMeshParams params={};params.tileWidth=64;params.tileHeight=64;params.maxTiles=1;params.maxPolys=4;
        write_fixture("params.test-fixture",&params,sizeof(params));
        require(mesh.init(&params)==DT_SUCCESS,"authored mesh init");
        const unsigned short vertices[]={0,0,0,0,0,40,40,0,40,40,0,0};
        unsigned short polygons[12];std::fill(std::begin(polygons),std::end(polygons),0xffff);
        for(unsigned i=0;i<4;++i)polygons[i]=i;
        const unsigned short flags[]={1};const unsigned char areas[]={11};
        dtNavMeshCreateParams cp={};cp.verts=vertices;cp.vertCount=4;cp.polys=polygons;
        cp.polyFlags=flags;cp.polyAreas=areas;cp.polyCount=1;cp.nvp=6;
        cp.walkableHeight=2;cp.walkableRadius=.5f;cp.walkableClimb=.4f;
        cp.bmax[0]=64;cp.bmax[1]=10;cp.bmax[2]=64;cp.cs=1;cp.ch=1;cp.buildBvTree=true;
        unsigned char* data=nullptr;int size=0;
        require(dtCreateNavMeshData(&cp,&data,&size),"authored tile generation");
        const uint32 frame[]={0x4d4d4150,7,8,uint32(size),1};
        std::vector<unsigned char> framed(sizeof(frame)+size);
        std::memcpy(framed.data(),frame,sizeof(frame));std::memcpy(framed.data()+sizeof(frame),data,size);
        write_fixture("quad.test-fixture",framed.data(),framed.size());
        // DT_POLYREF64 links need eight-byte alignment, while the original format
        // advances arrays by four. Borrow the tile at a base aligned for its links.
        dtMeshHeader header;std::memcpy(&header,data,sizeof(header));
        const size_t links=sizeof(header)+size_t(header.vertCount)*12+size_t(header.polyCount)*32;
        tileOwner=std::malloc(size+8);require(tileOwner,"authored tile allocation");
        auto* aligned=static_cast<unsigned char*>(tileOwner)+(links%8?4:0);
        std::memcpy(aligned,data,size);dtFree(data);
        require(mesh.addTile(aligned,size,0,0,&tileRef)==DT_SUCCESS,"authored tile load");
        require(query.init(&mesh,1024)==DT_SUCCESS,"source 1024 query nodes");
    }
    AuthoredMesh(const char* parameterPath,const char* tilePath) {
        const auto parameterBytes=read_fixture(parameterPath);const auto tileBytes=read_fixture(tilePath);
        dtNavMeshParams params;require(parameterBytes.size()==sizeof(params)&&tileBytes.size()>120,"existing fixture framing");
        std::memcpy(&params,parameterBytes.data(),sizeof(params));
        require(mesh.init(&params)==DT_SUCCESS,"existing authored mesh init");
        dtMeshHeader header;std::memcpy(&header,tileBytes.data()+20,sizeof(header));
        const size_t links=sizeof(header)+size_t(header.vertCount)*12+size_t(header.polyCount)*32;
        tileOwner=std::malloc(tileBytes.size()+8);require(tileOwner,"existing authored allocation");
        auto* aligned=static_cast<unsigned char*>(tileOwner)+(links%8?4:0);
        std::memcpy(aligned,tileBytes.data()+20,tileBytes.size()-20);
        require(mesh.addTile(aligned,int(tileBytes.size()-20),0,0,&tileRef)==DT_SUCCESS,"existing authored tile load");
        require(query.init(&mesh,1024)==DT_SUCCESS,"existing source1024 query");
    }
    ~AuthoredMesh() { mesh.removeTile(tileRef,nullptr,nullptr);std::free(tileOwner); }
};
static AuthoredMesh* geometry;
struct ObservedQuery {
    dtNavMeshQuery* query;
    dtStatus findNearestPoly(const float* p,const float* ext,const dtQueryFilter* filter,dtPolyRef* ref,float* near) const {
        event("detour:nearest:"+std::to_string(int(ext[0])));
        return query->findNearestPoly(p,ext,filter,ref,near);
    }
    dtStatus closestPointOnPoly(dtPolyRef ref,const float* p,float* near,bool* over) const {
        event("detour:closest-retained");return query->closestPointOnPoly(ref,p,near,over);
    }
    dtStatus getPolyHeight(dtPolyRef ref,const float* p,float* height) const {
        event("detour:height");return query->getPolyHeight(ref,p,height);
    }
};
static ObservedQuery observed;
struct GenericTransport {
    void CalculatePassengerPosition(float&,float&,float&) { fail("transport outside admitted scope"); }
    void CalculatePassengerOffset(float&,float&,float&) { fail("transport outside admitted scope"); }
};
struct Terrain {
    bool IsSwimmable(float,float,float) const { return false; }
    bool IsUnderWater(float,float,float) const { return false; }
};
struct Map {
    bool IsDungeon() const { return false; }
    bool IsInLineOfSight(float,float,float,float,float,float,bool) const { event("external:los");return true; }
};
struct Player;
struct MoveSpline { bool finalized=true;bool Finalized() const { return finalized; } };
enum { TYPEID_UNIT=3,TYPEID_PLAYER=4,UNIT_FIELD_FLAGS=0,UNIT_FLAG_PLAYER_CONTROLLED=8,CREATURE_EXTRA_FLAG_RUN_DURING_WANDER=32 };
struct Unit {
    Vector3 position{10,10,0};uint32 state=0;bool alive=true,ai=true,clientControlled=false,playerControlled=false;
    bool water=false,above=false,swim=false,levitate=false,hover=false,slowed=false,noMesh=false,noQuery=false,filtered=false;
    float healthPercent=100;int32 launchDuration=1000;unsigned launches=0;bool lastWalk=true;float combatSlowed=1;
    MoveSpline spline;MoveSpline* movespline=&spline;Player* controller=nullptr;
    mutable Map map;mutable Terrain terrain;
    void addUnitState(uint32 mask) { state|=mask;event("owner:add-state:"+std::to_string(mask)); }
    void clearUnitState(uint32 mask) { state&=~mask;event("owner:clear-state:"+std::to_string(mask)); }
    bool hasUnitState(uint32 mask) const { return (state&mask)!=0; }
    const Player* GetClientControlling() const { return controller; }
    void* AI() const { return ai?(void*)this:nullptr; }
    void SetTarget(std::nullptr_t) { event("owner:clear-target"); }
    Unit* GetVictim() { return nullptr; }
    void MeleeAttackStop(Unit*) { event("owner:stop-melee"); }
    bool IsClientControlled() const { return clientControlled; }
    void StopMoving(bool) { spline.finalized=true;event("owner:stop-moving"); }
    void InterruptMoving() { spline.finalized=true;event("owner:interrupt-moving"); }
    bool IsAlive() const { return alive; }
    bool HasFlag(int,uint32) const { return playerControlled; }
    bool IsSlowedInCombat() const { return slowed; }
    float GetHealthPercent() const { return healthPercent; }
    int GetTypeId() const { return TYPEID_UNIT; }
    GenericTransport* GetTransport() const { return nullptr; }
    void GetPosition(float& x,float& y,float& z,GenericTransport*) const { event("owner:position");x=position.x;y=position.y;z=position.z; }
    void UpdateAllowedPositionZ(float&,float&,float&) const { fail("normalization outside admitted scope"); }
    Map* GetMap() const { return &map; }
    Terrain* GetTerrain() const { return &terrain; }
    bool IsInWater() const { event("owner:water");return water; }
    bool CanSwim() const { return swim; }
    bool IsAboveGround() const { event("owner:above-ground");return above; }
    bool IsLevitating() const { return levitate; }
    bool IsHovering() const { return hover; }
    bool CanFly() const { return false; }
    float GetCollisionHeight() const { return 1; }
};
struct Player { void UpdateClientControl(Unit*,bool enable) const { event(enable?"client:enable":"client:disable"); } };
struct CreatureInfo { bool run=false;bool HasFlag(int) const { return run; } };
struct Creature:Unit {
    CreatureInfo info;Vector3 home{10,10,0};float radius=4;
    const CreatureInfo* GetCreatureInfo() const { return &info; }
    void GetRespawnCoord(float& x,float& y,float& z,void*,float* r) const { x=home.x;y=home.y;z=home.z;*r=radius;event("owner:respawn"); }
    void SetWalk(bool walk,bool) { lastWalk=walk;event("owner:set-walk"); }
};
enum ConfigKey { CONFIG_BOOL_PATH_FIND_NORMALIZE_Z };
struct Config { bool getConfig(ConfigKey) const { return false; } } sWorld;
class PathFinder {
public:
    PointsArray m_pathPoints;PathType m_type=PATHFIND_BLANK;
    bool m_straightLine=false,m_forceDestination=false,m_ignoreNormalization=false;
    uint32 m_pointPathLimit=74;std::vector<dtPolyRef> m_pathPolyRefs=std::vector<dtPolyRef>(74);uint32 m_polyLength=0;
    Vector3 m_startPosition{0,0,0},m_endPosition{0,0,0},m_actualEndPosition{0,0,0};
    const Unit* m_sourceUnit;const dtNavMesh* m_navMesh=nullptr;const ObservedQuery* m_navMeshQuery=nullptr;dtQueryFilter m_filter;
    explicit PathFinder(const Unit* owner):m_sourceUnit(owner) { event("path:construct-cache74"); }
    void updateFilter() { event("external:filter");m_filter.setIncludeFlags(m_sourceUnit->filtered?2:1);m_filter.setExcludeFlags(0); }
    void SetCurrentNavMesh() { event("external:select-nav");m_navMesh=m_sourceUnit->noMesh?nullptr:&geometry->mesh;m_navMeshQuery=m_sourceUnit->noQuery?nullptr:&observed; }
    #include "Clear.inc"
    void setStartPosition(const Vector3& point) { m_startPosition=point; }
    void setEndPosition(const Vector3& point) { m_actualEndPosition=point;m_endPosition=point; }
    void setActualEndPosition(const Vector3& point) { m_actualEndPosition=point; }
    Vector3 getStartPosition() const { return m_startPosition; }
    Vector3 getEndPosition() const { return m_endPosition; }
    Vector3 getActualEndPosition() const { return m_actualEndPosition; }
    PointsArray& getPath() { return m_pathPoints; }
    PathType getPathType() const { return m_type; }
    void setPathLengthLimit(float distance) { m_pointPathLimit=std::min<uint32>(uint32(distance/SMOOTH_PATH_STEP_SIZE*1.25f),MAX_POINT_PATH_LENGTH); }
    dtPolyRef getPathPolyByPosition(const dtPolyRef*,uint32,const float*,float*,const float maxDist=3.f) const;
    dtPolyRef getPolyByLocation(const float*,float*);
    void ComputePathToRandomPoint(const Vector3&,float);
    bool HaveTile(const Vector3&) const;
    void BuildPolyPath(const Vector3&,const Vector3&);
    void BuildShortcut();void NormalizePath();
};
#include "GetPathPolyByPosition.inc"
#include "GetPolyByLocation.inc"
#include "NormalizePath.inc"
#include "BuildShortcut.inc"
#include "HaveTile.inc"
#include "BuildPolyPath.selected.inc"
#include "ComputePathToRandomPoint.patched.inc"
namespace Movement {
struct MoveSplineInit {
    Unit& owner;explicit MoveSplineInit(Unit& unit):owner(unit) { event("spline:init"); }
    void MovebyPath(const PointsArray& path) { require(path.size()>=2,"launch requires path");event("spline:path"); }
    void SetWalk(bool walk) { owner.lastWalk=walk;event(walk?"spline:walk":"spline:run"); }
    void SetCombatSlowed(float factor) { owner.combatSlowed=factor;event("spline:slowed"); }
    int32 Launch() { ++owner.launches;event("external:launch:"+std::to_string(owner.launchDuration));if(owner.launchDuration)owner.spline.finalized=false;return owner.launchDuration; }
};
}
enum MovementGeneratorType { RANDOM_MOTION_TYPE=1 };
struct MovementGenerator {
    virtual ~MovementGenerator()=default;
    virtual void Initialize(Unit&)=0;virtual void Finalize(Unit&)=0;virtual void Interrupt(Unit&)=0;virtual void Reset(Unit&)=0;
    virtual bool Update(Unit&,const uint32&)=0;virtual MovementGeneratorType GetMovementGeneratorType() const=0;
};
#include "RandomDecl.inc"
#include "WanderDecl.inc"
#include "Initialize.inc"
#include "Finalize.inc"
#include "Interrupt.inc"
#include "Reset.inc"
#include "Update.inc"
#include "SetLocation.inc"
#include "WanderConstructor.inc"
#include "WanderFinalize.inc"
#include "WanderInterrupt.inc"
#include "Pause.inc"
struct InspectWander:WanderMovementGenerator {
    using WanderMovementGenerator::WanderMovementGenerator;
    uint32 timer() const { return i_nextMoveTimer.GetExpiry(); }
    uint32 count() const { return i_nextMoveCount; }
    const PathFinder& path() const { return *m_pathFinder; }
};
static void point(const Vector3& p) { std::printf("[%u,%u,%u]",bits(p.x),bits(p.y),bits(p.z)); }
static const char* geometryLabel="original-authored-quad64";
static void snapshot(const char* label,const char* frame,const Creature& owner,const InspectWander& move) {
    const auto& path=move.path();
    std::printf("{\"case\":\"%s\",\"frame\":\"%s\",\"timer\":%u,\"count\":%u,\"state\":%u,\"launches\":%u,\"walk\":%s,\"finalized\":%s,\"combat_slowed_bits\":%u,\"draws\":%zu,\"path_type\":%u,\"active_polys\":%u,\"endpoint_bits\":",label,frame,move.timer(),move.count(),owner.state,owner.launches,owner.lastWalk?"true":"false",owner.spline.finalized?"true":"false",bits(owner.combatSlowed),drawIndex,unsigned(path.m_type),path.m_polyLength);
    point(path.m_endPosition);std::printf(",\"geometry\":\"%s\"",geometryLabel);std::printf(",\"current_bits\":");point(owner.position);std::printf(",\"home_bits\":");point(owner.home);
    std::printf(",\"radius_bits\":%u,\"storage\":[",bits(owner.radius));
    for(size_t i=0;i<path.m_pathPolyRefs.size();++i)std::printf("%s%llu",i?",":"",(unsigned long long)path.m_pathPolyRefs[i]);
    std::printf("],\"points_bits\":[");for(size_t i=0;i<path.m_pathPoints.size();++i){if(i)std::printf(",");point(path.m_pathPoints[i]);}
    std::printf("],\"trace\":[");for(size_t i=0;i<events.size();++i)std::printf("%s\"%s\"",i?",":"",events[i].c_str());std::printf("]}\n");
}
static void begin(std::vector<Draw> supplied) { events.clear();draws=std::move(supplied);drawIndex=0; }
static void finish() { require(drawIndex==draws.size(),"unused supplied RNG draws"); }
static std::vector<Draw> success(float angle=0,float range=.5f,uint32 count=3,uint32 pause=3000) { return {norm(angle),norm(range),integer(1,3,count),integer(3000,10000,pause)}; }

// Dependency-state equivalence only: direct calls to the unchanged complete
// patched source body, with authored owner inputs and RNG transcripts. This
// neither replays generator clocks nor installs a runtime cache/revision/seal.
enum class WitnessKind { Normal, MinimumDistance, EndpointHeight, EndpointMissing };
struct WitnessOperation { WitnessKind kind;float angle=0;float radius=4; };
using PointBits=std::array<uint32,3>;
static PointBits point_bits(const Vector3& p) { return {bits(p.x),bits(p.y),bits(p.z)}; }
struct WitnessState {
    std::vector<dtPolyRef> storage;
    std::vector<PointBits> points;
    PointBits start,end,actualEnd;
    uint32 type,active,limit;
    unsigned include,exclude;
    bool straight,force;
    bool operator==(const WitnessState&) const=default;
};
static WitnessState witness_state(const PathFinder& path) {
    std::vector<PointBits> points;for(const auto& p:path.m_pathPoints)points.push_back(point_bits(p));
    return {path.m_pathPolyRefs,points,point_bits(path.m_startPosition),point_bits(path.m_endPosition),
        point_bits(path.m_actualEndPosition),unsigned(path.m_type),path.m_polyLength,path.m_pointPathLimit,
        path.m_filter.getIncludeFlags(),path.m_filter.getExcludeFlags(),path.m_straightLine,path.m_forceDestination};
}
static void witness_operation(AuthoredMesh& mesh,Creature& owner,PathFinder& path,const WitnessOperation& op) {
    geometry=&mesh;observed.query=&mesh.query;
    owner.position=Vector3(10,10,0);
    const float radius=op.kind==WitnessKind::EndpointHeight?64:op.kind==WitnessKind::EndpointMissing?100:op.radius;
    begin({norm(op.angle),norm(op.kind==WitnessKind::MinimumDistance?0.f:.5f)});
    path.ComputePathToRandomPoint(Vector3(10,10,0),radius);finish();
    require(path.m_navMesh==&mesh.mesh&&path.m_navMeshQuery==&observed,"witness actual selected allocation");
    require(path.m_filter.getIncludeFlags()==1&&path.m_filter.getExcludeFlags()==0,"witness immutable dry filter");
    require(path.m_type==(op.kind==WitnessKind::Normal?PATHFIND_NORMAL:PATHFIND_NOPATH),"witness source terminal");
    require(path.m_polyLength==(op.kind==WitnessKind::Normal?1u:0u),"witness source active prefix");
    require(path.m_pathPoints.size()==(op.kind==WitnessKind::Normal?2u:0u),"witness source points");
    require(std::all_of(path.m_pathPolyRefs.begin()+1,path.m_pathPolyRefs.begin()+74,[](dtPolyRef ref){return ref==0;}),"witness inactive slots remain zero");
    const auto detourCalls=std::count_if(events.begin(),events.end(),[](const auto& e){return e.starts_with("detour:");});
    require((detourCalls==0)==(op.kind==WitnessKind::MinimumDistance),"witness endpoint writer classification");
    if(op.kind==WitnessKind::MinimumDistance)
        require(std::find(events.begin(),events.end(),"owner:water")==events.end(),"minimum-distance returns before dry tail");
    else require(std::find(events.begin(),events.end(),"owner:above-ground")!=events.end(),"endpoint writer executes dry tail");
}
static void write_bounded_witness() {
    AuthoredMesh originalMesh,freshMesh;
    require(&originalMesh.mesh != &freshMesh.mesh && &originalMesh.query != &freshMesh.query,"witness separate mesh/query allocations");
    Creature originalOwner,freshOwner;
    PathFinder original(&originalOwner),rebuilt(&freshOwner);
    std::vector<WitnessOperation> operations{{WitnessKind::EndpointHeight}};
    while(operations.size()<4093) {
        const size_t index=operations.size();
        const WitnessKind kinds[]={WitnessKind::Normal,WitnessKind::MinimumDistance,WitnessKind::EndpointHeight,WitnessKind::EndpointMissing};
        operations.push_back({kinds[index%4]});
    }
    operations.push_back({WitnessKind::Normal,0,8});
    operations.push_back({WitnessKind::EndpointHeight});
    operations.push_back({WitnessKind::MinimumDistance});
    std::vector<size_t> appenders;
    std::vector<uint32> beforeActive;
    size_t lastNormal=0,lastEndpoint=0;
    for(size_t i=0;i<operations.size();++i) {
        const auto before=original.m_pathPolyRefs;beforeActive.push_back(original.m_polyLength);
        witness_operation(originalMesh,originalOwner,original,operations[i]);
        require(original.m_pathPolyRefs.size()>=before.size(),"witness storage never shrinks");
        require(std::equal(before.begin()+74,before.end(),original.m_pathPolyRefs.begin()+74),"witness ordered tail only appends");
        if(original.m_pathPolyRefs.size()>before.size())appenders.push_back(i);
        if(operations[i].kind==WitnessKind::Normal)lastNormal=i;
        if(operations[i].kind!=WitnessKind::MinimumDistance)lastEndpoint=i;
    }
    std::vector<size_t> selected=appenders;
    selected.insert(selected.end(),{lastNormal,lastEndpoint,operations.size()-1});
    std::sort(selected.begin(),selected.end());selected.erase(std::unique(selected.begin(),selected.end()),selected.end());
    require(appenders==std::vector<size_t>{0}&&selected==std::vector<size_t>({0,4093,4094,4095}),"witness discriminating retained indexes");
    unsigned beforeDifferences=0;
    for(size_t i:selected) {
        beforeDifferences+=rebuilt.m_polyLength!=beforeActive[i];
        witness_operation(freshMesh,freshOwner,rebuilt,operations[i]);
    }
    const auto final=witness_state(original);
    require(witness_state(rebuilt)==final,"bounded witness complete semantic cache equality");
    require(beforeDifferences>0,"witness must distinguish historical before-state from semantic equality");
    for(size_t omitted:{lastNormal,lastEndpoint}) {
        Creature negativeOwner;PathFinder negative(&negativeOwner);
        for(size_t i:selected)if(i!=omitted)witness_operation(freshMesh,negativeOwner,negative,operations[i]);
        const auto wrong=witness_state(negative);
        require(!(wrong==final),"omitting semantic writer must differ");
        if(omitted==lastNormal)require(wrong.storage[0]==0&&final.storage[0]!=0,"NORMAL omission loses slot zero");
        else require(wrong.end!=final.end,"endpoint omission loses retained endpoint");
    }
    // Missing endpoint and near-zero calls cannot recreate an omitted appended
    // ref. This negative uses actual nearest misses, never invented poly refs.
    Creature appendOwner,missingOwner;PathFinder appendPath(&appendOwner),missingPath(&missingOwner);
    const std::vector<WitnessOperation> appendCase={{WitnessKind::EndpointHeight},{WitnessKind::EndpointMissing},{WitnessKind::MinimumDistance}};
    for(size_t i=0;i<appendCase.size();++i) {
        witness_operation(originalMesh,appendOwner,appendPath,appendCase[i]);
        if(i)witness_operation(freshMesh,missingOwner,missingPath,appendCase[i]);
    }
    require(appendPath.m_pathPolyRefs.size()==75&&missingPath.m_pathPolyRefs.size()==74,"append omission loses retained tail");
    require(!(witness_state(appendPath)==witness_state(missingPath)),"append omission must differ");
    const std::vector<WitnessOperation> continuation={{WitnessKind::MinimumDistance},{WitnessKind::Normal,.25f,6},
        {WitnessKind::EndpointHeight},{WitnessKind::EndpointMissing},{WitnessKind::Normal,.125f,8}};
    for(const auto& op:continuation) {
        witness_operation(originalMesh,originalOwner,original,op);const auto trace=events;
        witness_operation(freshMesh,freshOwner,rebuilt,op);
        require(witness_state(original)==witness_state(rebuilt)&&trace==events,"bounded witness actual source continuation equality");
    }
    std::ostringstream json;
    json<<"{\"schema\":1,\"scope\":\"source-native semantic cache equivalence; no runtime restoration\","
        <<"\"operations\":"<<operations.size()<<",\"retained_operations\":"<<selected.size()
        <<",\"appending_operations\":"<<appenders.size()<<",\"retained_indices\":[";
    for(size_t i=0;i<selected.size();++i)json<<(i?",":"")<<selected[i];
    json<<"],\"final_storage\":[";
    for(size_t i=0;i<final.storage.size();++i)json<<(i?",":"")<<final.storage[i];
    json<<"],\"final_endpoint_bits\":["<<final.end[0]<<","<<final.end[1]<<","<<final.end[2]
        <<"],\"final_path_type\":"<<final.type<<",\"final_active_polys\":"<<final.active
        <<",\"final_points\":"<<final.points.size()<<",\"before_active_differences\":"<<beforeDifferences
        <<",\"continuation_operations\":"<<continuation.size()
        <<",\"fresh_query_allocation\":true,\"semantic_state_equal\":true,\"continuation_state_and_trace_equal\":true,"
        <<"\"omit_last_normal_detected\":true,\"omit_endpoint_writer_detected\":true,\"omit_appender_detected\":true}\n";
    const auto data=json.str();write_fixture("bounded-witness.json",data.data(),data.size());
}
int main(int argc,char** argv) {
try {
    require(argc==4,"usage: wander-oracle OUTPUT_DIRECTORY PRECISION_PARAMS PRECISION_TILE");fixtureDirectory=argv[1];
    require(std::fegetround()==FE_TONEAREST&&(_mm_getcsr()&0x8040)==0,"unsupported float environment");
    AuthoredMesh mesh;geometry=&mesh;observed.query=&mesh.query;
    {
        auto sequence=success();sequence.push_back(norm(.25f));sequence.push_back(norm(.5f));sequence.push_back(norm(.5f));sequence.push_back(norm(.5f));
        begin(sequence);Creature npc;InspectWander move(npc);move.Initialize(npc);
        require(drawIndex==0&&npc.launches==0&&move.timer()==0&&move.count()==1,"initialization contract");snapshot("pause_relaunch","initialized",npc,move);
        move.Update(npc,1);require(move.timer()==3000&&move.count()==3&&move.path().m_type==PATHFIND_NORMAL,"first successful leg");snapshot("pause_relaunch","first_launch",npc,move);
        move.Update(npc,5000);require(move.timer()==3000&&drawIndex==4,"moving freezes timer");snapshot("pause_relaunch","moving_timer_frozen",npc,move);
        npc.spline.finalized=true;move.Update(npc,2999);require(move.timer()==1&&npc.launches==1,"pause below boundary");snapshot("pause_relaunch","pause_remaining_one",npc,move);
        move.Update(npc,1);require(move.timer()==0&&move.count()==2&&npc.launches==2,"pause equality relaunch");snapshot("pause_relaunch","second_launch",npc,move);
        npc.spline.finalized=true;move.Update(npc,1);require(move.count()==1&&npc.launches==3&&move.timer()==0,"move count unpaused relaunch");snapshot("pause_relaunch","third_launch",npc,move);finish();
    }
    for(unsigned scenario=0;scenario<4;++scenario) {
        begin({norm(0),norm(0),norm(0),norm(.5f),integer(1,3,1),integer(3000,10000,10000)});
        Creature npc;npc.playerControlled=scenario&1;InspectWander move(npc);move.Initialize(npc);
        const char* label=scenario==0?"failure_retry_500":scenario==1?"failure_retry_100":scenario==2?"blocked_timer": "dead_keeps_state";
        move.Update(npc,0);uint32 delay=npc.playerControlled?100:500;
        require(move.timer()==delay&&move.count()==1&&npc.launches==0,"failure delay and count");snapshot(label,"failure",npc,move);
        if(scenario==2) { npc.state|=UNIT_STAT_ROOT|UNIT_STAT_ROAMING_MOVE;move.Update(npc,delay-1);require(move.timer()==1&&!(npc.state&UNIT_STAT_ROAMING_MOVE),"blocked countdown");snapshot(label,"blocked",npc,move);npc.state&=~UNIT_STAT_ROOT; }
        else if(scenario==3) { npc.alive=false;require(!move.Update(npc,1000)&&move.timer()==delay,"dead early return");snapshot(label,"dead",npc,move);npc.alive=true;move.Update(npc,delay-1); }
        else move.Update(npc,delay-1);
        require(npc.launches==0&&drawIndex==2,"no early retry");move.Update(npc,1);require(npc.launches==1&&move.timer()==10000,"retry equality");snapshot(label,"retry_success",npc,move);finish();
    }
    for(unsigned scenario=0;scenario<2;++scenario) {
        begin({norm(0),norm(.5f),integer(0,99,scenario?15:14),integer(1,3,2),integer(3000,10000,7777)});
        Creature npc;npc.info.run=true;npc.slowed=true;npc.healthPercent=20;InspectWander move(npc);move.Initialize(npc);move.Update(npc,0);
        require(npc.lastWalk==bool(scenario),"walk threshold 15");snapshot(scenario?"run_roll_15_walk":"run_roll_14_run","launch",npc,move);finish();
    }
    {
        begin({norm(0),norm(.5f),integer(0,99,14)});Creature npc;npc.info.run=true;npc.launchDuration=0;InspectWander move(npc);move.Initialize(npc);move.Update(npc,0);
        require(move.timer()==500&&move.count()==1&&!(npc.state&UNIT_STAT_ROAMING_MOVE),"zero launch failure");snapshot("zero_duration","failed_launch",npc,move);finish();
    }
    for(unsigned scenario=0;scenario<5;++scenario) {
        begin(success());Creature npc;npc.noMesh=scenario==0;npc.noQuery=scenario==1;
        if(scenario==2)npc.state|=UNIT_STAT_IGNORE_PATHFINDING;
        if(scenario==3)npc.position=Vector3(70,10,0);
        if(scenario==4)npc.home=Vector3(70,10,0);
        InspectWander move(npc);move.Initialize(npc);move.Update(npc,0);
        require(move.path().m_type==(PATHFIND_NORMAL|PATHFIND_SHORTCUT),"source missing map/tile shortcut");
        const char* names[]={"no_mesh_source_shortcut","no_query_source_shortcut","ignore_pathfinding_source_shortcut","missing_start_tile_source_shortcut","missing_end_tile_source_shortcut"};
        snapshot(names[scenario],"launch",npc,move);finish();
    }
    for(unsigned scenario=0;scenario<4;++scenario) {
        begin({norm(0),norm(.5f)});Creature npc;npc.filtered=scenario==0;
        if(scenario)npc.radius=64;
        if(scenario==3)npc.radius=100;
        if(scenario==2){npc.water=true;npc.swim=true;}
        InspectWander move(npc);move.Initialize(npc);
        // Water LOS fallback returns SHORTCUT without NOPATH and therefore launches.
        if(scenario==2){draws.push_back(integer(1,3,1));draws.push_back(integer(3000,10000,3000));}
        move.Update(npc,0);
        require((scenario==2&&move.path().m_type==PATHFIND_SHORTCUT&&npc.launches==1)||(scenario!=2&&move.path().m_type==PATHFIND_NOPATH&&npc.launches==0),"actual filter/height failures");
        const char* names[]={"actual_filtered_poly_miss","actual_outside_poly_height_failure","water_height_failure_source_shortcut","actual_missing_poly_inside_loaded_tile"};snapshot(names[scenario],"result",npc,move);finish();
    }
    for(unsigned scenario=0;scenario<2;++scenario) {
        begin({norm(0),norm(.5f),norm(0),norm(.125f),integer(1,3,2),integer(3000,10000,4000)});
        Creature npc;npc.radius=scenario?100:64;InspectWander move(npc);move.Initialize(npc);move.Update(npc,1);
        const char* label=scenario?"missing_poly_retry":"height_failure_retry";
        require(move.timer()==500&&move.path().m_type==PATHFIND_NOPATH&&npc.launches==0,"defined geometry failure retry");
        require(move.path().m_pathPolyRefs.size()==(scenario?74u:75u),"failed lookup source cache mutation");snapshot(label,"failure",npc,move);
        move.Update(npc,499);require(drawIndex==2&&move.timer()==1,"geometry failure delay");snapshot(label,"one_ms_remaining",npc,move);
        move.Update(npc,1);require(npc.launches==1&&move.path().m_type==PATHFIND_NORMAL&&move.count()==2&&move.timer()==4000,"geometry failure retry success");snapshot(label,"retry_success",npc,move);finish();
    }
    for(unsigned scenario=0;scenario<2;++scenario) {
        begin(scenario?success(0,.5f):std::vector<Draw>{norm(0),norm(.5f)});Creature npc;npc.home=npc.position=Vector3(0,0,0);npc.radius=scenario?.2f:std::nextafter(.2f,0.f);
        InspectWander move(npc);move.Initialize(npc);move.Update(npc,0);
        require(npc.launches==scenario,"strict 0.01 squared distance threshold");snapshot(scenario?"distance_above_threshold":"distance_below_threshold","result",npc,move);finish();
    }
    for(unsigned scenario=0;scenario<3;++scenario) {
        begin({});Creature npc;Player player;npc.spline.finalized=false;
        if(scenario==1){npc.controller=&player;npc.clientControlled=true;}
        if(scenario==2)npc.ai=false;
        InspectWander move(npc);move.Initialize(npc);snapshot(scenario==0?"initial_prior_spline_ai":scenario==1?"initial_prior_spline_client":"initial_prior_spline_no_ai","initialized",npc,move);
        require(npc.spline.finalized&&drawIndex==0,"initial interrupt no random draw");move.Finalize(npc);snapshot(scenario==0?"initial_prior_spline_ai":scenario==1?"initial_prior_spline_client":"initial_prior_spline_no_ai","finalized",npc,move);finish();
    }
    {
        begin(success());Creature npc;InspectWander move(npc);move.Initialize(npc);move.Update(npc,0);
        move.AddToRandomPauseTime(-4000,false);require(move.timer()==1,"negative pause adjustment clamps to one");snapshot("pause_adjust_reset","clamped_one",npc,move);
        move.AddToRandomPauseTime(0,true);require(move.timer()==0,"force pause zero");
        move.AddToRandomPauseTime(100,false);require(move.timer()==0,"nonforced passed timer unchanged");snapshot("pause_adjust_reset","passed_unchanged",npc,move);
        move.Reset(npc);require(move.timer()==0&&move.count()==3&&npc.spline.finalized&&move.path().m_pathPolyRefs.size()==74,"reset replaces path and preserves move count");snapshot("pause_adjust_reset","reset",npc,move);finish();
    }
    for(unsigned scenario=0;scenario<5;++scenario) {
        const float angles[]={.1f,.125f,.33333334f,.777f,std::nextafter(1.f,0.f)};
        begin(success(angles[scenario],.731f));Creature npc;npc.radius=7.25f;InspectWander move(npc);move.Initialize(npc);move.Update(npc,0);
        std::string label="sample_angle_"+std::to_string(scenario);snapshot(label.c_str(),"launch",npc,move);finish();
    }
    // Separate observer lane: exact pre-existing authored fixture bytes, unmodified.
    AuthoredMesh precision(argv[2],argv[3]);geometry=&precision;observed.query=&precision.query;
    geometryLabel="unchanged-existing-precision-fixture";
    for(unsigned scenario=0;scenario<3;++scenario) {
        begin(scenario?std::vector<Draw>{norm(scenario==1?0.f:.125f),norm(.5f)}:success(0,.5f));
        Creature npc;npc.position=npc.home=Vector3(5,5,0);npc.radius=scenario==0?2:scenario==1?12:150;
        InspectWander move(npc);move.Initialize(npc);move.Update(npc,1);
        require(move.path().m_type==(scenario?PATHFIND_NOPATH:PATHFIND_NORMAL),"unchanged precision fixture source result");
        const char* names[]={"precision_same_polygon","precision_endpoint_height_failure","precision_endpoint_missing"};
        snapshot(names[scenario],"result",npc,move);finish();
    }
    write_bounded_witness();
    return 0;
} catch(const std::exception& error) { std::fprintf(stderr,"%s\n",error.what());return 2; }
}
