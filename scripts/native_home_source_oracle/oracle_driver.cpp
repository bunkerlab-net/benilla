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
    const dtNavMesh* getAttachedNavMesh() const { return query->getAttachedNavMesh(); }
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
#include "Position.inc"
namespace Movement {
#include "Location.inc"
}
struct GenericTransport {
    void CalculatePassengerPosition(float&,float&,float&) { fail("transport outside selected scope"); }
    void CalculatePassengerOffset(float&,float&,float&,float* =nullptr) { fail("transport outside selected scope"); }
    unsigned GetDisplayId()const{return 0;}
};
struct Creature;struct Unit;struct MotionMaster;
struct AILeaves { void SummonedJustReachedHome(Creature*){event("callback:summoned-home");} };
struct Guid { bool creature=false;bool IsCreatureOrPet()const{return creature;} };
struct Map {
    bool dungeon=false;bool IsDungeon()const{return dungeon;}
    Creature* GetAnyTypeCreature(Guid)const{fail("summoner outside selected scope");}
};
struct Terrain { bool IsSwimmable(float,float,float)const{return false;}bool IsUnderWater(float,float,float)const{return false;} };
struct MoveSpline {
    bool finalized=true;Movement::Location computed{10,10,0,0};
    bool Finalized()const{return finalized;}
    Movement::Location ComputePosition()const{event("external:spline-compute-position");return computed;}
    void _Interrupt(){finalized=true;event("spline:interrupt");}
};
struct CombatManager { int evade=0;void SetEvadeState(int e){evade=e;event("owner:evade:"+std::to_string(e));} };
enum {TYPEID_UNIT=3,TYPEID_PLAYER=4,UNIT_FIELD_FLAGS=0,UNIT_FLAG_PLAYER_CONTROLLED=8,CREATURE_EXTRA_FLAG_RUN_DURING_WANDER=32,DIST_CALC_NONE=0,EVADE_NONE=0,EVADE_HOME=1,TEMPFACTION_RESTORE_REACH_HOME=1};
struct Player;
struct Unit {
    Position position{10,10,0};MoveSpline spline;MoveSpline* movespline=&spline;
    uint32 state=0;bool active=false,inWorld=true,temporary=false,ai=true;unsigned temporaryFaction=0;
    mutable Map map;mutable Terrain terrain;CombatManager combat;MotionMaster* motion=nullptr;AILeaves aiObject;
    unsigned launches=0;bool lastWalk=false;float velocity=0,facing=0;mutable float distanceSquared=0;PointsArray launchPath;int32 launchDuration=1000;
    void addUnitState(uint32 mask){state|=mask;event("owner:add-state:"+std::to_string(mask));}
    void clearUnitState(uint32 mask){state&=~mask;event("owner:clear-state:"+std::to_string(mask));}
    bool hasUnitState(uint32 mask)const{return (state&mask)!=0;}
    bool isActiveObject()const{return active;}void SetActiveObjectState(bool v){active=v;event(v?"owner:active":"owner:inactive");}
    CombatManager& GetCombatManager(){return combat;}
    MotionMaster* GetMotionMaster(){return motion;}
    Map* GetMap()const{return &map;}Terrain* GetTerrain()const{return &terrain;}
    bool IsInWorld()const{return inWorld;}bool IsTemporarySummon()const{return temporary;}
    uint32 GetTemporaryFactionFlags()const{return temporaryFaction;}
    void ClearTemporaryFaction(){temporaryFaction=0;event("owner:restore-faction");}
    void LoadCreatureAddon(bool reload){require(reload,"addon reload required");event("callback:addon-reload");}
    void TriggerHomeEvents(){event("callback:home-events");}
    void SetRespawnDelay(unsigned delay,bool){event("owner:respawn-delay:"+std::to_string(delay));}
    void ForcedDespawn(unsigned delay){event("owner:despawn:"+std::to_string(delay));}
    Guid GetSpawnerGuid()const{return {};}
    AILeaves* AI(){return ai?&aiObject:nullptr;}
    GenericTransport* GetTransport()const{return nullptr;}
    Position GetPosition(GenericTransport* =nullptr)const{event("owner:map-position");return position;}
    float GetPositionX()const{return position.x;}float GetPositionY()const{return position.y;}float GetPositionZ()const{return position.z;}float GetOrientation()const{return position.o;}
    // Authored no-transport leaf; pinned GetDistance.context.inc retains the
    // original DIST_CALC_NONE squared-distance expression and default return.
    float GetDistance(float x,float y,float z,int mode,GenericTransport* transport)const{require(mode==DIST_CALC_NONE&&!transport,"only no-transport DIST_CALC_NONE");event("owner:distance-check");float a=position.x-x,b=position.y-y,c=position.z-z;distanceSquared=a*a+b*b+c;return distanceSquared;}
    std::string GetGuidStr()const{return "authored-owner";}unsigned GetMapId()const{return 0;}unsigned GetInstanceId()const{return 0;}
    int GetTypeId()const{return TYPEID_UNIT;}bool IsCreature()const{return true;}bool CanSwim()const{return false;}bool CanFly()const{return false;}
    const Player* GetClientControlling()const{return nullptr;}
    void SetTarget(std::nullptr_t){event("owner:clear-target");}Unit* GetVictim(){return nullptr;}
    void MeleeAttackStop(Unit*){event("owner:stop-melee");}bool IsClientControlled()const{return false;}
    void UpdateAllowedPositionZ(float&,float&,float&)const{fail("normalization outside selected scope");}
    void UpdateSplinePosition(bool forced){require(forced,"forced interrupt relocation");const auto p=spline.ComputePosition();position=Position(p.x,p.y,p.z,p.orientation);event("owner:forced-relocation");}
    void StopMoving(bool =false){spline.finalized=true;event("owner:stop-moving");}
    void InterruptMoving(bool forceSendStop=false);
};
struct Player {void UpdateClientControl(Unit*,bool)const{fail("controlled owner outside selected scope");}};
struct CreatureInfo {bool HasFlag(int)const{return false;}};
struct Creature:Unit {
    Position combatStart;Position respawn{8,8,0,.25f};CreatureInfo info;float radius=4;
    void GetCombatStartPosition(Position& p)const{p=combatStart;event("owner:combat-start-read");}
    const Position& GetCombatStartPosition()const{return combatStart;}
    void SetCombatStartPosition(const Position& p){combatStart=p;event(p.IsEmpty()?"owner:clear-combat-start":"owner:capture-combat-start");}
    void GetRespawnCoord(float& x,float& y,float& z,float* o=nullptr,float* r=nullptr)const{x=respawn.x;y=respawn.y;z=respawn.z;if(o)*o=respawn.o;if(r)*r=radius;event("owner:respawn-read");}
    const CreatureInfo* GetCreatureInfo()const{return &info;}
    void SetWalk(bool walk,bool){lastWalk=walk;event("owner:set-walk");}
    void CaptureCombatStart(){Creature* creature=this;
#include "CombatCapture.inc"
    }
};
#include "InterruptMoving.inc"
enum ConfigKey {CONFIG_BOOL_PATH_FIND_NORMALIZE_Z};
struct Config {bool getConfig(ConfigKey)const{return false;}}sWorld;
namespace MaNGOS {static bool IsValidMapCoord(float x,float y,float z){return std::isfinite(x)&&std::isfinite(y)&&std::isfinite(z)&&std::abs(x)<17067&&std::abs(y)<17067&&std::abs(z)<17067;}}
namespace MMAP {
struct MMapManager {const ObservedQuery* GetNavMeshQuery(unsigned,unsigned){event("constructor:source1024-query");return &observed;}const ObservedQuery* GetModelNavMeshQuery(unsigned){fail("transport query outside selected scope");}};
struct MMapFactory {static bool IsPathfindingEnabled(unsigned,const Unit*){return true;}static MMapManager* createOrGetMMapManager(){static MMapManager m;return &m;}};
}
static unsigned pathConstructed=0;
struct CapturedPath {bool force=false,straight=false;unsigned type=0,active=0;Vector3 start,end;std::vector<dtPolyRef> storage;PointsArray points;} captured;
class PathFinder {
public:
    PointsArray m_pathPoints;PathType m_type;bool m_useStraightPath,m_forceDestination,m_straightLine;
    uint32 m_pointPathLimit;std::vector<float> m_cachedPoints;std::vector<dtPolyRef> m_pathPolyRefs;uint32 m_polyLength;std::vector<dtPolyRef>m_smoothPathPolyRefs;
    Vector3 m_startPosition,m_endPosition,m_actualEndPosition;const Unit* const m_sourceUnit;
    const dtNavMesh* m_navMesh;const ObservedQuery* m_navMeshQuery;const ObservedQuery* m_defaultNavMeshQuery;uint32 m_defaultMapId;bool m_ignoreNormalization;dtQueryFilter m_filter;
    explicit PathFinder(const Unit* owner,bool ignoreNormalization=false);
    ~PathFinder(){event("path:destroy");}
    void createFilter(){++pathConstructed;event("constructor:filter");m_filter.setIncludeFlags(1);m_filter.setExcludeFlags(0);}
    void updateFilter(){event("calculate:filter");}
    void SetCurrentNavMesh();
    bool calculate(const Vector3&,const Vector3&,bool forceDest=false,bool straightLine=false);
#include "Clear.inc"
    void setStartPosition(const Vector3& p){m_startPosition=p;}void setEndPosition(const Vector3& p){m_endPosition=p;m_actualEndPosition=p;}void setActualEndPosition(const Vector3& p){m_actualEndPosition=p;}
    Vector3 getStartPosition()const{return m_startPosition;}Vector3 getEndPosition()const{return m_endPosition;}Vector3 getActualEndPosition()const{return m_actualEndPosition;}
    PointsArray& getPath(){captured={m_forceDestination,m_straightLine,unsigned(m_type),m_polyLength,m_startPosition,m_endPosition,m_pathPolyRefs,m_pathPoints};return m_pathPoints;}
    PathType getPathType()const{return m_type;}
    dtPolyRef getPathPolyByPosition(const dtPolyRef*,uint32,const float*,float*,const float maxDist=3.f)const;dtPolyRef getPolyByLocation(const float*,float*);
    bool HaveTile(const Vector3&)const;void BuildPolyPath(const Vector3&,const Vector3&);void BuildShortcut();void NormalizePath();
};
#include "PathConstructor.inc"
#include "SelectNav.inc"
#include "Calculate.inc"
#include "GetPathPolyByPosition.inc"
#include "GetPolyByLocation.inc"
#include "NormalizePath.inc"
#include "BuildShortcut.inc"
#include "HaveTile.inc"
#include "BuildPolyPath.selected.inc"
namespace Movement {
using MoveSpline=::MoveSpline;
struct MoveSplineInit {
    Unit& unit;struct Args{PointsArray path;}args;
    explicit MoveSplineInit(Unit& u):unit(u){event("spline:init");}
    void MovebyPath(const PointsArray& p){args.path=p;event("spline:path");}
    void MoveTo(const Location& p){args.path={Vector3(p.x,p.y,p.z),Vector3(p.x,p.y,p.z)};}
    void SetWalk(bool walk){unit.lastWalk=walk;event(walk?"spline:walk":"spline:run");}
    void SetFacing(float f){unit.facing=f;event("spline:facing");}void SetVelocity(float v){unit.velocity=v;event("spline:override-velocity");}
    int32 Launch(){
#include "LaunchStart.prefix.inc"
        require(!pathEmpty,"Home supplies a real path");unit.launchPath=args.path;++unit.launches;unit.spline.finalized=false;event("external:spline-launch");return unit.launchDuration;
    }
};
}
enum MovementGeneratorType {IDLE_MOTION_TYPE,RANDOM_MOTION_TYPE,CHASE_MOTION_TYPE,HOME_MOTION_TYPE,EFFECT_MOTION_TYPE,DISTRACT_MOTION_TYPE};
struct MovementGenerator {
    virtual ~MovementGenerator()=default;
    virtual void Initialize(Unit&){}virtual void Finalize(Unit&){}virtual void Interrupt(Unit&){}virtual void Reset(Unit&){}
    virtual bool Update(Unit&,const uint32&){return true;}
    virtual MovementGeneratorType GetMovementGeneratorType()const{return IDLE_MOTION_TYPE;}
    virtual bool GetResetPosition(Unit&,float&,float&,float&,float&)const{return false;}
    virtual bool IsRemovedOnExpire()const{return false;}
};
#include "MediumDecl.inc"
#include "RandomDecl.inc"
#include "WanderDecl.inc"
template<class T>class HomeMovementGenerator;
#include "HomeDecl.inc"
struct InspectWander:WanderMovementGenerator {
    using WanderMovementGenerator::WanderMovementGenerator;
    void SetCount(unsigned count){i_nextMoveCount=count;}void SetTimer(unsigned timer){i_nextMoveTimer.Reset(timer);}
    unsigned Count()const{return i_nextMoveCount;}unsigned Timer()const{return i_nextMoveTimer.GetExpiry();}
};
MovementGenerator si_idleMovement;
static bool isStatic(MovementGenerator* p){return p==&si_idleMovement;}
#define MANGOS_ASSERT(value) require(bool(value),"MotionMaster assertion")
struct Log {void outError(const char*,...){fail("unsupported owner type");}}sLog;
enum {MMCF_UPDATE=1,MMCF_RESET=2};
struct MotionMaster {
    Unit* m_owner;std::vector<MovementGenerator*> stack;using ExpireList=std::vector<MovementGenerator*>;ExpireList* m_expList=nullptr;unsigned m_cleanFlag=0;
    explicit MotionMaster(Unit& o):m_owner(&o){o.motion=this;}
    ~MotionMaster(){for(auto* p:stack)if(!isStatic(p))delete p;if(m_expList){for(auto*p:*m_expList)delete p;delete m_expList;}}
    bool empty()const{return stack.empty();}size_t size()const{return stack.size();}MovementGenerator* top()const{return stack.back();}void push(MovementGenerator*p){stack.push_back(p);}void pop(){stack.pop_back();}
    void Initialize(){fail("default reselect outside selected scope");}
    void DirectClean(bool,bool);void DelayedClean(bool,bool);void DirectExpire(bool);void DelayedExpire(bool);
#include "ClearMotion.inc"
#include "ExpireMotion.inc"
    void Mutate(MovementGenerator*);void MoveTargetedHome(bool runHome=true);void UpdateMotion(uint32);
};
#include "RandomInitialize.inc"
#include "RandomFinalize.inc"
#include "RandomInterrupt.inc"
#include "RandomReset.inc"
#include "WanderConstructor.inc"
#include "WanderInterrupt.inc"
#include "WanderFinalize.inc"
bool AbstractRandomMovementGenerator::Update(Unit&,const uint32&){fail("random sampling outside Home oracle scope");}
int32 AbstractRandomMovementGenerator::_setLocation(Unit&){fail("random sampling outside Home oracle scope");}
#include "HomeInitialize.inc"
#include "HomeReset.inc"
#include "HomeSetTarget.inc"
#include "HomeUpdate.inc"
#include "HomeFinalize.inc"
#include "DirectClean.inc"
#include "DelayedClean.inc"
#include "DirectExpire.inc"
#include "DelayedExpire.inc"
#include "Mutate.inc"
#include "MoveHome.inc"
#include "UpdateMotion.inc"
struct Chase:MovementGenerator {MovementGeneratorType GetMovementGeneratorType()const override{return CHASE_MOTION_TYPE;}void Finalize(Unit&)override{event("chase:finalize");}};
static size_t at(const char* e){auto p=std::find(events.begin(),events.end(),e);require(p!=events.end(),e);return size_t(p-events.begin());}
static bool has(const char* e){return std::find(events.begin(),events.end(),e)!=events.end();}
static void point(const Vector3&p){std::printf("[%u,%u,%u]",bits(p.x),bits(p.y),bits(p.z));}
static void snapshot(const char* name,const Creature&o,const MotionMaster&m){
    std::printf("{\"case\":\"%s\",\"launches\":%u,\"active\":%s,\"evade\":%d,\"constructors\":%u,\"stack\":%zu,\"force\":%s,\"straight\":%s,\"path_type\":%u,\"active_polys\":%u,\"query_origin_bits\":",name,o.launches,o.active?"true":"false",o.combat.evade,pathConstructed,m.size(),captured.force?"true":"false",captured.straight?"true":"false",captured.type,captured.active);point(captured.start);std::printf(",\"goal_bits\":");point(captured.end);std::printf(",\"launched_points_bits\":[");for(size_t i=0;i<o.launchPath.size();++i){if(i)std::printf(",");point(o.launchPath[i]);}std::printf("],\"storage\":[");for(size_t i=0;i<captured.storage.size();++i)std::printf("%s%llu",i?",":"",(unsigned long long)captured.storage[i]);
    std::printf("],\"state\":%u,\"walk\":%s,\"velocity_bits\":%u,\"facing_bits\":%u,\"distance_squared_bits\":%u,\"combat_start_bits\":",o.state,o.lastWalk?"true":"false",bits(o.velocity),bits(o.facing),bits(o.distanceSquared));point(Vector3(o.combatStart.x,o.combatStart.y,o.combatStart.z));
    if(!m.empty())if(auto*w=dynamic_cast<const InspectWander*>(m.top()))std::printf(",\"retained_wander_count\":%u,\"retained_wander_timer\":%u",w->Count(),w->Timer());
    std::printf(",\"trace\":[");for(size_t i=0;i<events.size();++i)std::printf("%s\"%s\"",i?",":"",events[i].c_str());std::printf("]}\n");
}
static void begin(){events.clear();pathConstructed=0;captured={};}
static void normal(const Creature&o){require(captured.force&&!captured.straight&&captured.type==PATHFIND_NORMAL&&captured.active==1,"Home true/false same-polygon terminal");require(captured.storage.size()==75&&captured.storage[0]==captured.storage[74],"cold zero policy retained lookup");require(o.velocity==0&&o.launches==1,"ordinary speed rather than 400 fallback");}
int main(int argc,char**argv){try{
    require(argc==2,"output directory required");fixtureDirectory=argv[1];std::fesetround(FE_TONEAREST);_MM_SET_FLUSH_ZERO_MODE(_MM_FLUSH_ZERO_OFF);_mm_setcsr(_mm_getcsr()&~0x40u);AuthoredMesh mesh;geometry=&mesh;observed.query=&mesh.query;
    {begin();Creature o;MotionMaster m(o);m.push(&si_idleMovement);o.position={14,15,0,.75f};o.CaptureCombatStart();o.position={20,20,0,1};o.CaptureCombatStart();m.MoveTargetedHome();normal(o);require(captured.end==Vector3(14,15,0)&&o.facing==.75f,"first combat origin retained");require(!has("owner:respawn-read"),"nonempty origin must not respawn");snapshot("combat-origin-preserved",o,m);}
    {begin();Creature o;MotionMaster m(o);m.push(&si_idleMovement);o.combatStart={0,0,0,1};m.MoveTargetedHome(false);normal(o);require(captured.end==Vector3(8,8,0)&&o.facing==.25f&&o.lastWalk,"zero xyz respawn fallback");snapshot("zero-origin-respawn-fallback",o,m);}
    {begin();Creature o;MotionMaster m(o);m.push(&si_idleMovement);m.push(new Chase);o.combatStart={14,15,0,.75f};o.position={20,20,0};o.spline.finalized=false;o.spline.computed={21,20,0,0};m.MoveTargetedHome();normal(o);require(captured.start==Vector3(20,20,0)&&o.launchPath[0]==Vector3(21,20,0),"Idle stale launch correction");require(!has("owner:forced-relocation"),"Idle Interrupt is empty");snapshot("idle-default-stale-launch",o,m);}
    {begin();Creature o;MotionMaster m(o);auto*w=new InspectWander(o);m.push(w);w->Initialize(o);w->SetCount(2);w->SetTimer(5000);o.position={14,15,0,.75f};o.CaptureCombatStart();m.Mutate(new Chase);o.position={20,20,0};o.spline.finalized=false;o.spline.computed={21,20,0,0};events.clear();m.MoveTargetedHome();normal(o);require(captured.start==Vector3(21,20,0)&&o.launchPath[0]==captured.start,"default Wander second interrupt");require(at("chase:finalize")<at("owner:forced-relocation")&&at("owner:forced-relocation")<at("owner:active"),"Clear, default Interrupt, Home Initialize order");require(captured.end==Vector3(14,15,0),"Wander combat-origin home");m.UpdateMotion(50);require(!has("callback:addon-reload"),"not finalized no arrival");o.spline.finalized=true;m.UpdateMotion(50);require(m.size()==1&&m.top()==w&&w->Count()==2&&w->Timer()==0,"retained default count and zero timer");require(pathConstructed==3,"fresh Home then fresh reset Wander constructor");require(at("callback:addon-reload")<at("callback:home-events")&&at("callback:home-events")<at("owner:clear-combat-start"),"Home callback order");require(o.combatStart.IsEmpty()&&!o.active&&o.combat.evade==0,"Home finalized state");snapshot("wander-chase-home-arrival-reset",o,m);}
    {begin();Creature o;MotionMaster m(o);m.push(&si_idleMovement);o.combatStart={14,15,0};m.MoveTargetedHome();m.Mutate(new Chase);require(!has("callback:addon-reload")&&!o.combatStart.IsEmpty()&&o.combat.evade==0,"interrupted Home no arrival callbacks");snapshot("home-interrupted-before-arrival",o,m);}
    {begin();Creature o;MotionMaster m(o);m.push(&si_idleMovement);o.active=true;o.temporaryFaction=TEMPFACTION_RESTORE_REACH_HOME;o.combatStart={14,15,0};m.MoveTargetedHome();normal(o);o.spline.finalized=true;m.UpdateMotion(50);require(o.active&&!has("owner:inactive")&&o.combatStart.IsEmpty(),"already active owner remains active after arrival");require(at("owner:restore-faction")<at("callback:addon-reload"),"temporary faction restored before addon reload");snapshot("already-active-arrival-faction-restore",o,m);}
    {begin();Creature o;MotionMaster m(o);m.push(&si_idleMovement);o.state=UNIT_STAT_LOST_CONTROL;m.MoveTargetedHome();require(o.launches==0&&pathConstructed==0&&m.size()==1,"lost control blocks Home");snapshot("lost-control-refusal",o,m);}
    {begin();Creature o;MotionMaster m(o);m.push(&si_idleMovement);o.state=UNIT_STAT_NOT_MOVE;HomeMovementGenerator<Creature> h;h.Initialize(o);require(o.launches==0&&pathConstructed==0&&o.active&&o.combat.evade==1,"not-move target early return");snapshot("not-move-no-path",o,m);}
    {begin();Creature o;MotionMaster m(o);m.push(&si_idleMovement);o.combatStart={500,10,0};m.MoveTargetedHome();require(o.launches==0&&pathConstructed==0&&has("owner:respawn-delay:5")&&has("owner:despawn:1"),"outdoor long-distance source branch");require(at("callback:home-events")<at("owner:respawn-delay:5"),"long-distance finalize before despawn");snapshot("outdoor-long-distance-excluded",o,m);}
    {begin();Creature o;MotionMaster m(o);m.push(&si_idleMovement);o.combatStart={160,10,0};m.MoveTargetedHome();require(o.distanceSquared==150.f*150.f&&o.launches==1&&pathConstructed==1&&!has("owner:despawn:1"),"far comparison is strict at equality");require(captured.type==(PATHFIND_NORMAL|PATHFIND_NOT_USING_PATH)&&o.velocity==0,"distance-boundary missing-tile shortcut remains excluded");snapshot("outdoor-distance-equality-excluded",o,m);}
    {begin();Creature o;MotionMaster m(o);m.push(&si_idleMovement);o.combatStart={std::nextafter(160.f,std::numeric_limits<float>::infinity()),10,0};m.MoveTargetedHome();require(o.distanceSquared>150.f*150.f&&o.launches==0&&pathConstructed==0&&has("owner:despawn:1"),"adjacent float above far comparison");snapshot("outdoor-distance-next-float-excluded",o,m);}
    {begin();Creature o;MotionMaster m(o);auto*w=new InspectWander(o);m.push(w);w->Initialize(o);m.Mutate(new Chase);o.combatStart={10,10,0};o.position={159,10,0};o.spline.finalized=false;o.spline.computed={161,10,0,0};events.clear();m.MoveTargetedHome();require(o.distanceSquared==151.f*151.f&&o.launches==0&&pathConstructed==1&&has("owner:despawn:1"),"relocated 151 is far although stale pose was 149");require(at("owner:forced-relocation")<at("owner:distance-check")&&at("owner:distance-check")<at("callback:home-events"),"Wander interruption precedes distance and far finalize");snapshot("wander-interrupt-crosses-far-boundary-excluded",o,m);}
    {begin();Creature o;MotionMaster m(o);auto*w=new InspectWander(o);m.push(w);w->Initialize(o);m.Mutate(new Chase);o.combatStart={10,10,0};o.position={161,10,0};o.spline.finalized=false;o.spline.computed={159,10,0,0};events.clear();m.MoveTargetedHome();require(o.distanceSquared==149.f*149.f&&o.launches==1&&pathConstructed==2&&!has("owner:despawn:1"),"relocated 149 is not far although stale pose was 151");require(captured.start==Vector3(159,10,0)&&captured.type==(PATHFIND_NORMAL|PATHFIND_NOT_USING_PATH),"inverse boundary is an excluded missing-tile shortcut");require(at("owner:forced-relocation")<at("owner:distance-check")&&at("owner:distance-check")<at("constructor:filter"),"Wander interruption precedes distance and fresh Home path");snapshot("wander-interrupt-crosses-near-boundary-excluded",o,m);}
    {begin();Creature o;MotionMaster m(o);m.push(&si_idleMovement);o.state=UNIT_STAT_IGNORE_PATHFINDING;o.combatStart={14,15,0};m.MoveTargetedHome();require(captured.type==(PATHFIND_NORMAL|PATHFIND_NOT_USING_PATH)&&o.velocity==0,"ignore shortcut source flags");snapshot("ignore-pathfinding-excluded",o,m);}
    {begin();Creature o;MotionMaster m(o);m.push(&si_idleMovement);o.combatStart={60,10,0};m.MoveTargetedHome();require(captured.type==PATHFIND_NOPATH&&o.velocity==400,"missing polygon 400 source fallback");snapshot("missing-polygon-400-excluded",o,m);}
    return 0;
}catch(const std::exception&e){std::fprintf(stderr,"Home oracle: %s\n",e.what());return 1;}}
