// SPDX-License-Identifier: GPL-3.0-or-later
// Test-only unchanged original warm lookup bodies and scoped storage-transition fragments.
#include "DetourNavMeshBuilder.h"
#include "DetourNavMesh.h"
#include "DetourNavMeshQuery.h"
#include "DetourCommon.h"
#include "DetourMath.h"
#include "DetourAlloc.h"
#include "DetourNode.h"
#include "G3D/Vector3.h"
#include <array>
#include <bit>
#include <cmath>
#include <cfenv>
#include <cfloat>
#include <xmmintrin.h>
#include <type_traits>
#include <cstdint>
#include <cstddef>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <limits>
#include <map>
#include <string>
#include <tuple>
#include <vector>
#ifdef ENABLE_PLAYERBOTS
#error Original non-PLAYERBOTS branch required
#endif
using uint32 = std::uint32_t;
using int32 = std::int32_t;
using uint8 = std::uint8_t;
using G3D::Vector3;
typedef std::vector<Vector3> PointsArray;
static_assert(FLT_EVAL_METHOD == 0);
static_assert(sizeof(float) == 4 && std::numeric_limits<float>::is_iec559);
static_assert(std::endian::native == std::endian::little);
static_assert(sizeof(dtPolyRef) == 8 && sizeof(dtNavMeshParams) == 28 && sizeof(dtMeshHeader) == 100);
static_assert(alignof(dtLink)==8 && alignof(dtMeshHeader)==4 && alignof(dtPoly)==4);
static_assert(alignof(dtPolyDetail)==4 && alignof(dtBVNode)==4);
// Both lanes compare every tile member against the verified original declaration.
// ORIGINAL_TILE_LAYOUT
static_assert(!std::is_default_constructible_v<OriginalTileLayout> && !std::is_aggregate_v<OriginalTileLayout>);
static_assert(sizeof(dtMeshTile)==104 && alignof(dtMeshTile)==8);
static_assert(sizeof(dtMeshTile)==sizeof(OriginalTileLayout) && alignof(dtMeshTile)==alignof(OriginalTileLayout));
static_assert(std::is_standard_layout_v<dtMeshTile> && !std::is_copy_constructible_v<dtMeshTile> && !std::is_copy_assignable_v<dtMeshTile>);
#ifdef STRAIGHT_PATH_REPAIR_TILE_LIFETIME
static_assert(std::is_trivially_default_constructible_v<dtMeshTile> && std::is_trivially_destructible_v<dtMeshTile>);
#else
// Exact original lane retains this known raw-storage lifetime caveat.
static_assert(!std::is_default_constructible_v<dtMeshTile>);
#endif
#define CHECK_TILE_FIELD(member) static_assert(offsetof(dtMeshTile,member)==offsetof(OriginalTileLayout,member))
CHECK_TILE_FIELD(salt);CHECK_TILE_FIELD(linksFreeList);CHECK_TILE_FIELD(header);CHECK_TILE_FIELD(polys);
CHECK_TILE_FIELD(verts);CHECK_TILE_FIELD(links);CHECK_TILE_FIELD(detailMeshes);CHECK_TILE_FIELD(detailVerts);
CHECK_TILE_FIELD(detailTris);CHECK_TILE_FIELD(bvTree);CHECK_TILE_FIELD(offMeshCons);CHECK_TILE_FIELD(data);
CHECK_TILE_FIELD(dataSize);CHECK_TILE_FIELD(flags);CHECK_TILE_FIELD(next);
#undef CHECK_TILE_FIELD
[[noreturn]] static void fail(const char* s) { std::fprintf(stderr, "%s\n", s); std::exit(2); }
static void require(bool b, const char* s) { if (!b) fail(s); }
static std::vector<unsigned char> read(const std::string& p) {
    FILE* f = std::fopen(p.c_str(), "rb"); require(f, "open input");
    require(std::fseek(f, 0, SEEK_END) == 0, "seek input"); long n = std::ftell(f);
    require(n > 0 && std::fseek(f, 0, SEEK_SET) == 0, "size input");
    std::vector<unsigned char> data(static_cast<size_t>(n));
    require(std::fread(data.data(), 1, data.size(), f) == data.size(), "read input"); std::fclose(f); return data;
}
struct Loaded {
    std::vector<void*> owners;
    dtNavMesh mesh;
    dtNavMeshQuery query;
    explicit Loaded(const std::vector<unsigned char>& params, int nodes=1024) {
        dtNavMeshParams p; require(params.size() == sizeof(p), "parameter size"); std::memcpy(&p, params.data(), sizeof(p));
        require(mesh.init(&p) == DT_SUCCESS && query.init(&mesh, nodes) == DT_SUCCESS, "initialize native mesh");
    }
    void add(const std::vector<unsigned char>& file) {
        require(file.size() > 120, "tile size"); const auto* bytes = file.data() + 20;
        dtMeshHeader h; std::memcpy(&h, bytes, sizeof(h));
        const size_t links = sizeof(h) + size_t(h.vertCount) * 12 + size_t(h.polyCount) * 32;
        void* owner = std::malloc(file.size() + 8); require(owner, "tile allocation");
        auto* aligned = static_cast<unsigned char*>(owner) + (links % 8 ? 4 : 0);
        std::memcpy(aligned, bytes, file.size() - 20);
        require(mesh.addTile(aligned, int(file.size() - 20), 0, 0, nullptr) == DT_SUCCESS, "add native tile");
        owners.push_back(owner);
    }
    // Explicit clear before owner free: dtNavMesh destructor has no borrowed live tile.
    ~Loaded() {
        for (int i = 0; i < mesh.getMaxTiles(); ++i) {
            const auto* tile = static_cast<const dtNavMesh&>(mesh).getTile(i);
            if (tile->header) mesh.removeTile(mesh.getTileRef(tile), nullptr, nullptr);
        }
        for (void* owner : owners) std::free(owner);
    }
};

static constexpr float GRID = 533.3333f;
#define DEBUG_FILTER_LOG(...) ((void)0)
// ORIGINAL_CONSTANTS
// ORIGINAL_PATH_TYPE
// ORIGINAL_SEARCH_BOUNDS
// The original diagnostic branch is unreachable for the validated active references.
// These aborting diagnostics are not owner/world callbacks or gameplay observations.
struct DiagnosticPoint { std::string toString() const { fail("unreachable diagnostic point"); } };
struct GenericTransport {
    void CalculatePassengerPosition(float&,float&,float&) { fail("unadmitted transport normalization"); }
    void CalculatePassengerOffset(float&,float&,float&) { fail("unadmitted transport normalization"); }
};
struct DiagnosticOwner {
    float width=0; uint32 width_calls=0;
    float GetCollisionWidth() { ++width_calls; return width; }
    unsigned GetMapId() const { fail("unreachable diagnostic owner"); }
    GenericTransport* GetTransport() const { fail("unadmitted world normalization"); }
    void UpdateAllowedPositionZ(float&,float&,float&) const { fail("unadmitted world normalization"); }
};
// Explicit external configuration snapshot. It supplies no terrain or owner answers.
enum ConfigKey { CONFIG_BOOL_PATH_FIND_OPTIMIZE, CONFIG_BOOL_PATH_FIND_NORMALIZE_Z };
struct ConfigSnapshot {
    bool optimize=false, normalize=false;
    bool getConfig(ConfigKey key) const { return key==CONFIG_BOOL_PATH_FIND_OPTIMIZE?optimize:normalize; }
} sWorld;
struct DiagnosticLog { [[noreturn]] void unreachable() { fail("unadmitted source diagnostic"); } };
// Diagnostic arguments are never evaluated: active references and statuses are guarded.
#define outError(...) unreachable()
static DiagnosticLog sLog;

static uint32 bits(float v) {return std::bit_cast<uint32>(v);}
static float word(uint32 v) {return std::bit_cast<float>(v);}
static Vector3 world(const std::array<float,3>& p) {return {p[2],p[0],p[1]};}
static Vector3 world(const float* p) {return {p[2],p[0],p[1]};}
static void point(FILE* out,const Vector3& p) {std::fprintf(out,"%08x,%08x,%08x",bits(p.x),bits(p.y),bits(p.z));}
static void points(FILE* out,const PointsArray& values) {
    if(values.empty())std::fputc('-',out);
    for(size_t i=0;i<values.size();++i){if(i)std::fputc(';',out);point(out,values[i]);}
}
static void refs(FILE* out,const std::vector<dtPolyRef>& values) {
    if(values.empty())std::fputc('-',out);
    for(size_t i=0;i<values.size();++i)std::fprintf(out,"%s%llu",i?",":"",static_cast<unsigned long long>(values[i]));
}
static std::vector<dtPolyRef> storage(std::initializer_list<dtPolyRef> active,std::initializer_list<dtPolyRef> tail={}) {
    std::vector<dtPolyRef> result(active);result.resize(74);result.insert(result.end(),tail);return result;
}
struct Event {
    const char* primitive;dtPolyRef ref=0,target=0;dtStatus status=0;
    PointsArray input,output;float scalar=0;std::vector<dtPolyRef> path;
};
struct ObservedQuery {
    dtNavMeshQuery& query;std::vector<dtPolyRef>* cache=nullptr;uint32* active=nullptr;
    std::vector<dtPolyRef> smooth_storage;uint32 smooth_active=0;
    std::vector<Event> events;bool has_ray=false;dtStatus ray_status=0,direct_status=0;
    float hit=0;std::array<float,3> normal={};int ray_active=0;
    std::vector<dtPolyRef> ray_storage;uint32 height_calls=0,height_failures=0;
    explicit ObservedQuery(dtNavMeshQuery& q):query(q) {}
    void reset() {events.clear();has_ray=false;ray_status=direct_status=0;hit=0;normal={};ray_active=0;ray_storage.clear();smooth_storage.clear();smooth_active=0;height_calls=height_failures=0;}
    dtStatus closestPointOnPoly(dtPolyRef ref,const float* p,float* out,bool* over) {
        auto s=query.closestPointOnPoly(ref,p,out,over);
        events.push_back({"closest",ref,0,s,{world(p)},dtStatusSucceed(s)?PointsArray{world(out)}:PointsArray{},0,{}});return s;
    }
    dtStatus findNearestPoly(const float* p,const float* ext,const dtQueryFilter* f,dtPolyRef* ref,float* out) {
        auto s=query.findNearestPoly(p,ext,f,ref,out);
        events.push_back({ext[0]==5?"nearest5":"nearest10",*ref,0,s,{world(p)},dtStatusSucceed(s)&&*ref?PointsArray{world(out)}:PointsArray{},0,{}});return s;
    }
    dtStatus raycast(dtPolyRef ref,const float* a,const float* b,const dtQueryFilter* f,float* t,float* n,dtPolyRef* path,int* count,int cap) {
        // Avoid the original convenience overload's uninitialized hitNormal failure domain.
        require(query.isValidPolyRef(ref,f)&&dtVisfinite(a)&&dtVisfinite(b)&&cap==74&&f,"ray input domain");
        auto s=query.raycast(ref,a,b,f,t,n,path,count,cap);has_ray=true;ray_status=s;hit=*t;
        require(*count>=0&&*count<=cap,"ray output bounds");
        std::copy(n,n+3,normal.begin());ray_active=*count;ray_storage=*cache;
        events.push_back({"raycast",ref,0,s,{world(a),world(b)},{world(n)},*t,std::vector<dtPolyRef>(path,path+*count)});return s;
    }
    dtStatus getPolyHeight(dtPolyRef ref,const float* p,float* height) {
        auto input=world(p);auto s=query.getPolyHeight(ref,p,height);++height_calls;if(dtStatusFailed(s))++height_failures;
        auto output=input;output.z=*height;events.push_back({"height",ref,0,s,{input},{output},*height,{}});return s;
    }
    dtStatus findPath(dtPolyRef a,dtPolyRef b,const float* p,const float* q,const dtQueryFilter* f,dtPolyRef* out,int* count,int cap) {
        auto s=query.findPath(a,b,p,q,f,out,count,cap);direct_status=s;
        require(*count>=0&&*count<=cap,"direct output bounds");
        events.push_back({"direct",a,b,s,{world(p),world(q)},{},0,std::vector<dtPolyRef>(out,out+*count)});return s;
    }
    dtStatus closestPointOnPolyBoundary(dtPolyRef ref,const float* p,float* out) {
        if(smooth_storage.empty()){smooth_storage=*cache;smooth_active=*active;}
        auto s=query.closestPointOnPolyBoundary(ref,p,out);
        events.push_back({"boundary",ref,0,s,{world(p)},dtStatusSucceed(s)?PointsArray{world(out)}:PointsArray{},0,{}});return s;
    }
    dtStatus findStraightPath(const float* a,const float* b,const dtPolyRef* path,int n,float* out,unsigned char* flags,dtPolyRef* ids,int* count,int cap) {
        auto s=query.findStraightPath(a,b,path,n,out,flags,ids,count,cap);
        require(*count>=0&&*count<=cap,"steer output bounds");PointsArray output;
        for(int i=0;i<*count;++i)output.push_back(world(out+i*3));
        events.push_back({"steer",path[0],path[n-1],s,{world(a),world(b)},output,0,{}});return s;
    }
    dtStatus moveAlongSurface(dtPolyRef ref,const float* a,const float* b,const dtQueryFilter* f,float* out,dtPolyRef* visited,int* count,int cap) {
        auto s=query.moveAlongSurface(ref,a,b,f,out,visited,count,cap);
        require((s==DT_SUCCESS||s==(DT_SUCCESS|DT_BUFFER_TOO_SMALL))&&*count>0&&*count<=cap,"unadmitted original move failure");
        events.push_back({"move",ref,0,s,{world(a),world(b)},{world(out)},0,std::vector<dtPolyRef>(visited,visited+*count)});return s;
    }
};
class PathFinder {
public:
    dtNavMesh* m_navMesh;ObservedQuery observer;ObservedQuery* m_navMeshQuery;
    dtQueryFilter m_filter;std::vector<dtPolyRef> m_pathPolyRefs;uint32 m_polyLength,m_pointPathLimit=74;
    PointsArray m_pathPoints;std::vector<float> m_cachedPoints;std::vector<dtPolyRef> m_smoothPathPolyRefs;
    PathType m_type=PATHFIND_BLANK;
    bool m_straightLine=true,m_useStraightPath=false,m_forceDestination=false,m_ignoreNormalization=false;
    Vector3 m_startPosition,m_endPosition,m_actualEndPosition;
    DiagnosticOwner owner;DiagnosticOwner* m_sourceUnit=&owner;
    PathFinder(Loaded& loaded,std::vector<dtPolyRef> before,uint32 active)
        :m_navMesh(&loaded.mesh),observer(loaded.query),m_navMeshQuery(&observer),m_pathPolyRefs(std::move(before)),m_polyLength(active),m_cachedPoints(74*3),m_smoothPathPolyRefs(74) {
        observer.cache=&m_pathPolyRefs;observer.active=&m_polyLength;m_filter.setIncludeFlags(1);m_filter.setExcludeFlags(0);
    }
    // ORIGINAL_CLEAR
    // ORIGINAL_GETTERS
    // ORIGINAL_SETTERS
    // ORIGINAL_LOOKUP_DECLARATIONS
    void BuildPointPath(const float*,const float*);
    void BuildShortcut();void NormalizePath();
    uint32 fixupCorridor(dtPolyRef*,uint32,uint32,const dtPolyRef*,uint32);
    bool getSteerTarget(const float*,const float*,float,const dtPolyRef*,uint32,float*,unsigned char&,dtPolyRef&) const;
    dtStatus findSmoothPath(const float*,const float*,const dtPolyRef*,uint32,float*,int*,uint32);
    bool inRangeYZX(const float*,const float*,float,float) const;
    bool inRange(const Vector3&,const Vector3&,float,float) const;
    float dist3DSqr(const Vector3&,const Vector3&) const;
    void selected_transition(dtPolyRef startPoly,dtPolyRef endPoly,const float* startPoint,const float* endPoint) {
        const bool farFromPoly=false;
        const Vector3 startPos=getStartPosition();
        dtStatus dtResult;
        // ORIGINAL_SELECTED_SUFFIX
    }
};
// ORIGINAL_METHODS
static const char* admissible(const Loaded& loaded,const std::vector<dtPolyRef>& before,uint32 active,const Vector3& from,const Vector3& to,float width,bool normalize=false,bool ignore=false,uint32 previous=0) {
    if(active>74||before.size()<74||before.size()>4096||active>before.size())return "active_bounds";
    for(size_t i=0;i<before.size();++i)if((i<active&&!before[i])||(before[i]&&!loaded.mesh.isValidPolyRef(before[i])))return "invalid_reference";
    if(!active&&before.size()<=74)return "cold_uninitialized";
    for(float v:{from.x,from.y,from.z,to.x,to.y,to.z})if(!std::isfinite(v)||std::fabs(v)>100000)return "coordinate_domain";
    if(!std::isfinite(width)||width<0||width>100000)return "width_domain";
    if(normalize&&!ignore)return "normalization_unsupported";
    if(previous>63)return "type_domain";
    const float length=(to-from).magnitude();
    if(!std::isfinite(length)||length>4096)return "sample_bound";
    return nullptr;
}
static const char* phase(const PathFinder& p,dtPolyRef a,dtPolyRef b) {
    if(a==b)return "same";
    uint32 i=0;for(;i<p.m_polyLength&&p.m_pathPolyRefs[i]!=a;++i){}
    if(i==p.m_polyLength)return "replacement";
    for(uint32 j=p.m_polyLength-1;j>i;--j)if(p.m_pathPolyRefs[j]==b)return "cut";
    return "replacement";
}
static void configure(PathFinder& path,Vector3 from,Vector3 to,float width,bool optimize,bool normalize,bool ignore,bool force,uint32 previous) {
    sWorld={optimize,normalize};path.m_ignoreNormalization=ignore;path.m_forceDestination=force;
    path.owner.width=width;path.owner.width_calls=0;path.m_type=PathType(previous);path.setStartPosition(from);path.setEndPosition(to);
}
static void result(FILE* out,const PathFinder& p) {
    std::fprintf(out,"\t%u\t",p.m_polyLength);refs(out,p.m_pathPolyRefs);
    std::fprintf(out,"\t%u\t",unsigned(p.m_type));point(out,p.getEndPosition());std::fputc('\t',out);point(out,p.getActualEndPosition());std::fputc('\t',out);points(out,p.m_pathPoints);
}
static void trace(FILE* out,const char* label,const ObservedQuery& q) {
    for(size_t i=0;i<q.events.size();++i){const auto& e=q.events[i];
        std::fprintf(out,"%s_%04zu\t%zu\t%s\t%llu\t%llu\t%08x\t",label,i,i,e.primitive,(unsigned long long)e.ref,(unsigned long long)e.target,e.status);
        points(out,e.input);std::fputc('\t',out);points(out,e.output);std::fprintf(out,"\t%08x\t%zu\t",bits(e.scalar),e.path.size());refs(out,e.path);std::fputc('\n',out);
    }
}
static void fallback(Loaded& mesh,PathFinder& path,FILE* out,FILE* traces,const char* label,std::array<float,3> from,std::array<float,3> to,bool optimize) {
    require((path.m_type&(PATHFIND_NOPATH|PATHFIND_INCOMPLETE))!=0,"original fallback predicate");
    require(!admissible(mesh,path.m_pathPolyRefs,path.m_polyLength,world(from),world(to),path.owner.width),"fallback safe warm domain");
    const auto before=path.m_pathPolyRefs;const uint32 active=path.m_polyLength,previous=path.m_type;
    // Original calculate updates endpoint fields and flags, but never clears prior path type/cache.
    path.setStartPosition(world(from));path.setEndPosition(world(to));path.m_straightLine=false;path.m_forceDestination=false;
    sWorld.optimize=optimize;path.observer.reset();path.owner.width_calls=0;
    float ds,de;const auto a=path.getPolyByLocation(from.data(),&ds),b=path.getPolyByLocation(to.data(),&de);
    require(a&&b&&std::isfinite(ds)&&std::isfinite(de)&&ds<=7&&de<=7,"fallback warm endpoints");
    const auto selected=path.m_pathPolyRefs;const char* selected_phase=phase(path,a,b);
    path.selected_transition(a,b,from.data(),to.data());
    dtStatus smooth=0;PointsArray raw;
    // The source smoother's original cached output is the raw stream; observe count from
    // its final returned stream via an independent same-input probe when a corridor exists.
    if(std::strcmp(selected_phase,"same")!=0) {
        const auto corridor=path.observer.smooth_storage;const uint32 nc=path.observer.smooth_active;
        require(nc>0&&nc<=74&&corridor.size()>=74,"actual original pre-smooth corridor");
        if(path.observer.direct_status)require(path.observer.direct_status==DT_SUCCESS,"complete fallback direct status");
        PathFinder rawprobe(mesh,corridor,nc);std::vector<float> data(74*3);int count=0;
        smooth=rawprobe.findSmoothPath(from.data(),to.data(),corridor.data(),nc,data.data(),&count,74);
        require(count>=0&&count<=74,"smooth output bounds");
        require(!count||std::memcmp(data.data(),path.m_cachedPoints.data(),size_t(count)*3*sizeof(float))==0,"same-object raw smooth agrees with independent original probe");
        for(int i=0;i<count;++i)raw.push_back(world(data.data()+i*3));
    }
    const std::string name=std::string(label)+"_smooth";
    std::fprintf(out,"%s\t%s\t",name.c_str(),label);point(out,world(from));std::fputc('\t',out);point(out,world(to));
    std::fprintf(out,"\t%u\t%u\t",unsigned(optimize),active);refs(out,before);
    std::fprintf(out,"\t%u\t%llu\t%08x\t%llu\t%08x\t",previous,(unsigned long long)a,bits(ds),(unsigned long long)b,bits(de));refs(out,selected);
    std::fprintf(out,"\t%s\t",selected_phase);
    if(path.observer.direct_status)std::fprintf(out,"%08x",path.observer.direct_status);else std::fputc('-',out);
    std::fputc('\t',out);if(smooth)std::fprintf(out,"%08x",smooth);else std::fputc('-',out);
    std::fputc('\t',out);points(out,raw);result(out,path);std::fputc('\n',out);trace(traces,name.c_str(),path.observer);
}
static void observe(Loaded& mesh,FILE* out,FILE* sequences,FILE* traces,const char* label,const char* geometry,std::vector<dtPolyRef> before,uint32 active,
    std::array<float,3> from,std::array<float,3> to,float width=0,bool optimize=false,bool normalize=false,bool ignore=false,bool force=false,uint32 previous=0,bool helper=false,bool sequence=false) {
    require(!admissible(mesh,before,active,world(from),world(to),width,normalize,ignore,previous),"case input domain");
    std::fprintf(stderr,"observe %s\n",label);
    PathFinder path(mesh,before,active);configure(path,world(from),world(to),width,optimize,normalize,ignore,force,previous);
    float ds=0,de=0;dtPolyRef a=0,b=0;const char* selected_phase="sample";
    if(!helper){
        // Warm validation guarantees original nonempty active/tail writes each distance.
        float startDistance,endDistance;
        a=path.getPolyByLocation(from.data(),&startDistance);b=path.getPolyByLocation(to.data(),&endDistance);
        ds=startDistance;de=endDistance;
        require(a&&b&&std::isfinite(ds)&&std::isfinite(de)&&ds<=7&&de<=7,"loaded warm endpoint domain");selected_phase=phase(path,a,b);
    }
    const auto lookup=path.m_pathPolyRefs;
    if(helper)path.BuildPointPath(from.data(),to.data());else path.selected_transition(a,b,from.data(),to.data());
    require(path.m_pathPoints.size()<=1026,"sample output observer bound");
    std::fprintf(out,"%s\t%s\t%s\t",label,geometry,helper?"helper":"loaded");point(out,world(from));std::fputc('\t',out);point(out,world(to));
    std::fprintf(out,"\t%08x\t%u\t%u\t%u\t%u\t%u\t%u\t",bits(width),unsigned(optimize),unsigned(normalize),unsigned(ignore),unsigned(force),previous,active);refs(out,before);
    std::fprintf(out,"\t%llu\t%08x\t%llu\t%08x\t",(unsigned long long)a,bits(ds),(unsigned long long)b,bits(de));refs(out,lookup);std::fprintf(out,"\t%s\t",selected_phase);
    if(path.observer.has_ray){const auto& q=path.observer;
        std::fprintf(out,"%08x\t%08x\t",q.ray_status,bits(q.hit));point(out,world(q.normal));std::fprintf(out,"\t%d\t",q.ray_active);refs(out,q.ray_storage);std::fputc('\t',out);
        if(path.owner.width_calls)std::fprintf(out,"%08x",bits(q.hit-width));else std::fputc('-',out);
    }else std::fprintf(out,"-\t-\t-\t-\t-\t-");
    std::fprintf(out,"\t%u",path.owner.width_calls);result(out,path);
    std::fprintf(out,"\t%u\t%u\n",path.observer.height_calls,path.observer.height_failures);trace(traces,label,path.observer);
    if(sequence)fallback(mesh,path,sequences,traces,label,from,to,optimize);
}
int main(int argc,char** argv) {
    require(argc==3,"usage: oracle OUTPUT FIXTURES");
    require(std::fegetround()==FE_TONEAREST&&(_mm_getcsr()&0x8040)==0,"unsupported floating environment");
    static_assert(MAX_POINT_PATH_LENGTH==74&&MAX_PATH_LENGTH==74&&VERTEX_SIZE==3);
    require(SMOOTH_PATH_STEP_SIZE==4.0f,"original sample step");
    const std::string dir=argv[1],existing=argv[2];
    FILE* out=std::fopen((dir+"/cases.test-fixture").c_str(),"w");
    FILE* sequences=std::fopen((dir+"/sequences.test-fixture").c_str(),"w");
    FILE* traces=std::fopen((dir+"/traces.test-fixture").c_str(),"w");
    FILE* domains=std::fopen((dir+"/domains.test-fixture").c_str(),"w");
    require(out&&sequences&&traces&&domains,"open outputs");
    std::fprintf(out,"# label\tgeometry\tscope\tfrom\tto\tcollision_width\toptimize\tnormalize\tignore\tforce\tprevious_type\tbefore_active\tbefore_storage\tstart_ref\tstart_distance\tend_ref\tend_distance\tlookup_storage\tphase\tray_status\tray_hit\tray_normal\tray_active\tray_storage\tadjusted_hit\twidth_calls\tafter_active\tafter_storage\tpath_type\tend\tactual_end\tpoints\theight_calls\theight_failures\n");
    std::fprintf(sequences,"# label\tfirst_case\tfrom\tto\toptimize\tbefore_active\tbefore_storage\tprevious_type\tstart_ref\tstart_distance\tend_ref\tend_distance\tlookup_storage\tphase\tdirect_status\tsmooth_status\tsmooth_points\tafter_active\tafter_storage\tpath_type\tend\tactual_end\tpoints\n");
    std::fprintf(traces,"# label\tordinal\tprimitive\tref\ttarget\tstatus\tinput\toutput\tscalar\tcount\trefs\n");
    std::fprintf(domains,"# label\tclassification\tinvoked\n");
    Loaded precision(read(existing+"/direct-corridor/params.test-fixture"));precision.add(read(existing+"/warm-cache/precision.test-fixture"));
    const dtPolyRef A=precision.mesh.getPolyRefBase(static_cast<const dtNavMesh&>(precision.mesh).getTile(0)),B=A+1,U=A+2,C=A+3;
    observe(precision,out,sequences,traces,"replacement_no_hit","precision",storage({B},{C}),1,{5,0,5},{15,0,5});
    observe(precision,out,sequences,traces,"replacement_no_hit_prior_incomplete","precision",storage({B},{C}),1,{5,0,5},{15,0,5},0,false,false,false,false,4,false,true);
    observe(precision,out,sequences,traces,"cut_no_raycast","precision",storage({C,A,B},{U}),3,{5,0,5},{15,0,5},1.5f);
    observe(precision,out,sequences,traces,"same_no_sampling","precision",storage({A,B,C},{U}),3,{1,0,5},{5,0,5},1.5f,true,false,false,true,4);
    observe(precision,out,sequences,traces,"same_zero_length","precision",storage({A,B}),2,{5,0,5},{5,0,5});
    observe(precision,out,sequences,traces,"hit_boundary_ignored_height","precision",storage({A}),1,{5,0,5},{5,2,25});
    observe(precision,out,sequences,traces,"hit_full_width","precision",storage({A}),1,{5,0,5},{5,0,25},.125f);
    observe(precision,out,sequences,traces,"hit_full_width_force_clipped_end","precision",storage({A}),1,{5,0,5},{5,0,25},.125f,false,false,false,true,4);
    observe(precision,out,sequences,traces,"hit_width_reject","precision",storage({A}),1,{5,0,5},{5,0,25},.5f);
    for(auto [name,width]:std::array<std::pair<const char*,uint32>,3>{{{"hit_adjusted_above",0x3cccccca},{"hit_adjusted_equal",0x3ccccccd},{"hit_adjusted_below",0x3cccccce}}})
        observe(precision,out,sequences,traces,name,"precision",storage({A}),1,{5,0,9},{5,0,17},word(width));
    observe(precision,out,sequences,traces,"ray_no_hit_other_layer","precision",storage({U,A}),2,{1,0,5},{9,1,5},0,false,false,false,false,0,false,true);
    observe(precision,out,sequences,traces,"ignored_normalization","precision",storage({A,B}),2,{5,0,5},{15,0,5},0,true,true,true);
    observe(precision,out,sequences,traces,"sample_zero_length","precision",storage({A}),1,{0,0,5},{0,0,5},0,false,false,false,false,1,true);
    for(auto [name,length]:std::array<std::pair<const char*,uint32>,3>{{{"sample_length_below",0x3dcccccc},{"sample_length_equal",0x3dcccccd},{"sample_length_above",0x3dccccce}}})
        observe(precision,out,sequences,traces,name,"precision",storage({A}),1,{0,0,5},{word(length),0,5},0,false,false,false,false,1,true);
    observe(precision,out,sequences,traces,"sample_four_duplicate_endpoint","precision",storage({A}),1,{1,0,5},{5,0,5},0,false,false,false,false,1,true);
    observe(precision,out,sequences,traces,"sample_eight_merge","precision",storage({A}),1,{1,0,5},{9,0,5},0,false,false,false,false,1,true);
    for(auto [name,length]:std::array<std::pair<const char*,uint32>,2>{{{"sample_remainder_below",0x41133333},{"sample_remainder_above",0x41133334}}})
        observe(precision,out,sequences,traces,name,"precision",storage({A}),1,{0,0,5},{word(length),0,5},0,false,false,false,false,1,true);
    // End selects the upper floor while ray follows lower A. Interior clipping then
    // successfully resets height to zero, isolating original squared-distance > 0.01.
    for(auto [name,start]:std::array<std::pair<const char*,float>,3>{{{"hit_distance_below",9.8f},{"hit_distance_above",std::nextafter(9.8f,0.0f)},{"hit_distance_small",9.9f}}})
        observe(precision,out,sequences,traces,name,"precision",storage({U,A}),2,{5,0,start},{5,2.5f,20-start},.25f);
    Loaded ground(read(existing+"/smoothing/params.test-fixture"));ground.add(read(existing+"/smoothing/ground.test-fixture"));
    auto snake=storage({A+9,A+10,A+11,A+12,A+13},{A});
    const std::array<float,3> sa={-GRID+101,.0625f,-GRID+204},sb={-GRID+123,1.4375f,-GRID+220};
    observe(ground,out,sequences,traces,"snake_cut_bypasses_wall","ground",snake,5,sa,sb,.125f);
    observe(ground,out,sequences,traces,"snake_hit_then_smooth_incomplete","ground",storage({A+13},{A}),1,sa,sb,.125f,false,false,false,false,0,false,true);
    observe(ground,out,sequences,traces,"snake_reject_then_smooth_normal","ground",storage({A+13},{A}),1,sa,sb,.5f,false,false,false,false,0,false,true);
    observe(ground,out,sequences,traces,"snake_hit_then_smooth_optimized","ground",storage({A+13},{A}),1,sa,sb,.125f,true,false,false,false,0,false,true);
    auto slope=storage({A+1,A+2,A+3,A+4,A+5,A+6,A+7,A+8},{A});
    observe(ground,out,sequences,traces,"slope_sample_mutable_height","ground",slope,8,{-GRID+101,2,-GRID+304},{-GRID+163,3,-GRID+304});
    for(auto [name,length]:std::array<std::pair<const char*,float>,3>{{{"sample_count_73",288},{"sample_count_74",292},{"sample_count_75",296}}})
        observe(ground,out,sequences,traces,name,"ground",storage({A+24}),1,{-GRID+101,0,-GRID+404},{-GRID+101+length,0,-GRID+404},0,false,false,false,false,1,true);
    Loaded thin(read(existing+"/smoothing/params.test-fixture"));thin.add(read(existing+"/smoothing/thin.test-fixture"));
    observe(thin,out,sequences,traces,"thin_cached_early_height_failure","thin",storage({A+39}),1,{-GRID+125.05f,0,-GRID+202},{-GRID+129.95f,0,-GRID+202});
    Loaded longmesh(read(existing+"/direct-corridor/params.test-fixture"));longmesh.add(read(existing+"/direct-corridor/long600.test-fixture"));
    const float cell=GRID/600;
    observe(longmesh,out,sequences,traces,"ray_buffer_no_hit_sampling_appends","long600",storage({A+80}),1,{-GRID+cell*.5f,0,-GRID+cell*50},{-GRID+cell*80.5f,0,-GRID+cell*50});
    auto domain=[&](const char* label,const char* expected,std::vector<dtPolyRef> cache,uint32 active,Vector3 from,Vector3 to,float width=0,bool normalize=false,bool ignore=false,uint32 previous=0){
        const char* found=admissible(precision,cache,active,from,to,width,normalize,ignore,previous);
        require(found&&std::strcmp(found,expected)==0,"domain classifier mismatch");std::fprintf(domains,"%s\t%s\t0\n",label,found);
    };
    domain("cold_no_tail","cold_uninitialized",storage({}),0,{5,5,0},{5,15,0});
    domain("active_zero_reference","invalid_reference",storage({}),1,{5,5,0},{5,15,0});
    domain("invalid_retained_reference","invalid_reference",storage({A},{1}),1,{5,5,0},{5,15,0});
    domain("active_over_capacity","active_bounds",storage({A}),75,{5,5,0},{5,15,0});
    domain("nonfinite_endpoint","coordinate_domain",storage({A}),1,{word(0x7f800000),5,0},{5,15,0});
    domain("nan_width","width_domain",storage({A}),1,{5,5,0},{5,15,0},word(0x7fc00000));
    domain("negative_width","width_domain",storage({A}),1,{5,5,0},{5,15,0},-1);
    domain("active_normalization","normalization_unsupported",storage({A}),1,{5,5,0},{5,15,0},0,true);
    domain("invalid_type","type_domain",storage({A}),1,{5,5,0},{5,15,0},0,false,false,64);
    domain("sample_operational_bound","sample_bound",storage({A}),1,{5,5,0},{5000,15,0});
    require(std::fclose(out)==0&&std::fclose(sequences)==0&&std::fclose(traces)==0&&std::fclose(domains)==0,"close outputs");
}
