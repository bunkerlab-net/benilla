// SPDX-License-Identifier: GPL-3.0-or-later
// Test-only unchanged original warm lookup bodies and scoped storage-transition fragments.
#include "DetourNavMeshBuilder.h"
#include "DetourNavMesh.h"
#include "DetourNavMeshQuery.h"
#include "DetourCommon.h"
#include "DetourMath.h"
#include "DetourAlloc.h"
#include "DetourNode.h"
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
#ifdef WARM_CACHE_REPAIR_TILE_LIFETIME
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
static void write(const std::string& p, const void* data, size_t n) {
    FILE* f = std::fopen(p.c_str(), "wb"); require(f, "open output");
    require(std::fwrite(data, 1, n, f) == n, "write output"); require(std::fclose(f) == 0, "close output");
}
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
#define VERTEX_SIZE 3
#define INVALID_POLYREF 0
// ORIGINAL_SEARCH_BOUNDS
// The original diagnostic branch is unreachable for the validated active references.
// These aborting diagnostics are not owner/world callbacks or gameplay observations.
struct DiagnosticPoint { std::string toString() const { fail("unreachable diagnostic point"); } };
struct DiagnosticOwner { unsigned GetMapId() const { fail("unreachable diagnostic owner"); } };
struct DiagnosticLog { template<class... T> void outError(T...) { fail("invalid active reference diagnostic"); } };
static DiagnosticLog sLog;

// Transparent instrumentation only. Every primitive answer comes from the complete,
// separately compiled original Detour implementation and its genuinely loaded mesh.
struct ObservedQuery {
    dtNavMeshQuery& query;
    FILE* trace=nullptr;
    const char* label="";
    unsigned endpoint=0,ordinal=0,active=0,seen=0;
    float minimum=std::numeric_limits<float>::max();
    explicit ObservedQuery(dtNavMeshQuery& q):query(q) {}
    void begin(FILE* out,const char* name,unsigned which,unsigned count) {
        trace=out;label=name;endpoint=which;ordinal=0;active=count;seen=0;minimum=std::numeric_limits<float>::max();
    }
    void row(const char* primitive,dtPolyRef ref,dtStatus status,float distance) {
        if(trace)std::fprintf(trace,"%s\t%u\t%u\t%s\t%llu\t%08x\t%08x\n",label,endpoint,ordinal++,primitive,static_cast<unsigned long long>(ref),status,std::bit_cast<uint32>(distance));
    }
    dtStatus closestPointOnPoly(dtPolyRef ref,const float* pos,float* closest,bool* over) {
        auto status=query.closestPointOnPoly(ref,pos,closest,over);
        if(trace) {
            if(seen==active)minimum=std::numeric_limits<float>::max();
            if(dtStatusSucceed(status))minimum=std::min(minimum,dtVdistSqr(pos,closest));
            row(seen<active?"active":"tail",ref,status,dtMathSqrtf(minimum));++seen;
        }
        return status;
    }
    dtStatus findNearestPoly(const float* pos,const float* extents,const dtQueryFilter* filter,dtPolyRef* ref,float* nearest) {
        auto status=query.findNearestPoly(pos,extents,filter,ref,nearest);
        row(extents[0]==5?"nearest5":"nearest10",*ref,status,dtStatusSucceed(status)&&*ref?dtVdist(nearest,pos):0);
        return status;
    }
    dtStatus findPath(dtPolyRef a,dtPolyRef b,const float* p,const float* q,const dtQueryFilter* f,dtPolyRef* out,int* count,int limit) {
        return query.findPath(a,b,p,q,f,out,count,limit);
    }
    dtNodePool* getNodePool() const { return query.getNodePool(); }
};
struct Transition { const char* phase; dtStatus status=0; int nodes=0; };
class PathFinder {
public:
    ObservedQuery observer;
    ObservedQuery* m_navMeshQuery;
    dtQueryFilter m_filter;
    std::vector<dtPolyRef> m_pathPolyRefs;
    uint32 m_polyLength;
    uint32 m_pointPathLimit;
    std::vector<int> m_pathPoints;
    DiagnosticOwner* m_sourceUnit=nullptr;
    PathFinder(Loaded& mesh, std::vector<dtPolyRef> storage, uint32 active, uint32 limit=74)
        : observer(mesh.query), m_navMeshQuery(&observer), m_pathPolyRefs(std::move(storage)), m_polyLength(active), m_pointPathLimit(limit) {
        require(limit>0 && limit<=74 && active<=limit && m_pathPolyRefs.size()>=limit && m_pathPolyRefs.size()<=4096,"cache bounds");
        for (uint32 i=0;i<active;++i) require(mesh.mesh.isValidPolyRef(m_pathPolyRefs[i]),"valid active reference");
        for (auto ref:m_pathPolyRefs) require(!ref || mesh.mesh.isValidPolyRef(ref),"valid retained reference");
        m_filter.setIncludeFlags(1); m_filter.setExcludeFlags(0);
    }
    // ORIGINAL_CLEAR
    // ORIGINAL_LOOKUP_DECLARATIONS
    Transition selected_transition(dtPolyRef startPoly,dtPolyRef endPoly,const float* startPoint,const float* endPoint) {
        require(startPoly && endPoly,"selected endpoints required");
        if(startPoly==endPoly) {
            // Original BuildShortcut's first operation is clear(). This is only its
            // polygon-storage projection, not NormalizePath or a complete shortcut.
            clear();
            // ORIGINAL_SAME_ASSIGNMENT
            return {"same"};
        }
        [[maybe_unused]] DiagnosticPoint startPos,endPos;
        // ORIGINAL_SCAN_AND_CUT
        else {
            // Original replacement clear and direct call, no straight-line lane.
            clear();
            dtStatus dtResult;
            // ORIGINAL_DIRECT_CALL
            return {"replacement",dtResult,m_navMeshQuery->getNodePool()->getNodeCount()};
        }
        return {"cut"};
    }
};
// ORIGINAL_LOOKUP_METHODS

static uint32 bits(float v) { return std::bit_cast<uint32>(v); }
static void point(FILE* out,const float* p) { std::fprintf(out,"%08x,%08x,%08x",bits(p[2]),bits(p[0]),bits(p[1])); }
static void refs(FILE* out,const std::vector<dtPolyRef>& values) {
    if(values.empty()) std::fputc('-',out);
    for(size_t i=0;i<values.size();++i) std::fprintf(out,"%s%llu",i?",":"",static_cast<unsigned long long>(values[i]));
}
static std::vector<dtPolyRef> storage(std::initializer_list<dtPolyRef> active,std::initializer_list<dtPolyRef> tail={},uint32 limit=74) {
    std::vector<dtPolyRef> result(active);result.resize(limit);result.insert(result.end(),tail);return result;
}
struct Lookup {
    dtPolyRef selected=0,active=0,tail=0,near5=0,near10=0;
    float distance=0,active_distance=0,tail_distance=0;
    dtStatus near5_status=0,near10_status=0;
    const char* origin="excluded";
    bool appended=false,tail_checked=false,near5_checked=false,near10_checked=false;
};
static Lookup lookup(PathFinder& path,const float* p,FILE* trace=nullptr,const char* label="",unsigned endpoint=0) {
    Lookup result;
    // Never invoke the original helper with uninitialized distance: either a nonempty
    // active scan or an eligible nonempty tail must write distance before nearest runs.
    if(path.m_polyLength==0 && path.m_pathPolyRefs.size()<=std::max(path.m_polyLength,path.m_pointPathLimit)) return result;
    float observed_distance=std::numeric_limits<float>::quiet_NaN();
    result.active=path.getPathPolyByPosition(path.m_pathPolyRefs.data(),path.m_polyLength,p,&observed_distance);
    result.active_distance=path.m_polyLength?observed_distance:0;
    if(result.active) result.origin="active";
    else if(path.m_pathPolyRefs.size()>std::max(path.m_polyLength,path.m_pointPathLimit)) {
        result.tail_checked=true;
        result.tail=path.getPathPolyByPosition(&path.m_pathPolyRefs[path.m_pointPathLimit],uint32(path.m_pathPolyRefs.size()-path.m_pointPathLimit),p,&observed_distance,7.0f);
        result.tail_distance=observed_distance;
        if(result.tail) result.origin="tail";
    }
    if(!result.active && !result.tail) {
        float near[3]={};result.near5_checked=true;
        result.near5_status=path.m_navMeshQuery->findNearestPoly(p,NearPolySearchBound,&path.m_filter,&result.near5,near);
        if(dtStatusSucceed(result.near5_status) && result.near5) result.origin="nearest5";
        else {
            result.near10_checked=true;
            result.near10_status=path.m_navMeshQuery->findNearestPoly(p,FarPolySearchBound,&path.m_filter,&result.near10,near);
            result.origin=dtStatusSucceed(result.near10_status) && result.near10?"nearest10":"miss";
        }
    }
    require(std::isfinite(observed_distance),"warm scan writes finite distance");
    float distance; // Deliberately not initialized; the admitted original warm scan writes it.
    auto before=path.m_pathPolyRefs.size();
    path.observer.begin(trace,label,endpoint,path.m_polyLength);
    result.selected=path.getPolyByLocation(p,&distance);
    path.observer.trace=nullptr;
    require(std::isfinite(distance),"original warm lookup writes finite distance");
    result.distance=distance;result.appended=path.m_pathPolyRefs.size()>before;
    require(path.m_pathPolyRefs.size()<=4096,"storage operational cap");
    return result;
}
static void print_lookup(FILE* out,const Lookup& l) {
    std::fprintf(out,"%llu\t%08x\t%s\t%u",static_cast<unsigned long long>(l.selected),bits(l.distance),l.origin,unsigned(l.appended));
}
static void observe_lookup(Loaded& mesh,FILE* out,const char* label,const char* geometry,const char* scope,
                           std::vector<dtPolyRef> before,uint32 active,std::array<float,3> p,uint32 limit=74) {
    PathFinder path(mesh,before,active,limit); auto l=lookup(path,p.data());
    std::fprintf(out,"%s\t%s\t%s\t",label,geometry,scope);point(out,p.data());std::fprintf(out,"\t%u\t%u\t",limit,active);refs(out,before);std::fputc('\t',out);print_lookup(out,l);
    std::fprintf(out,"\t%llu\t%08x\t%u\t%llu\t%08x\t%u\t%08x\t%llu\t%u\t%08x\t%llu\t",static_cast<unsigned long long>(l.active),bits(l.active_distance),unsigned(l.tail_checked),static_cast<unsigned long long>(l.tail),bits(l.tail_distance),unsigned(l.near5_checked),l.near5_status,static_cast<unsigned long long>(l.near5),unsigned(l.near10_checked),l.near10_status,static_cast<unsigned long long>(l.near10));refs(out,path.m_pathPolyRefs);std::fputc('\n',out);
}
static FILE* case_traces=nullptr;
static void observe_case(Loaded& mesh,FILE* out,const char* label,const char* geometry,const char* scope,
                         std::vector<dtPolyRef> before,uint32 active,std::array<float,3> a,std::array<float,3> b,uint32 limit=74) {
    PathFinder path(mesh,before,active,limit);
    auto start=lookup(path,a.data(),case_traces,label,0),end=lookup(path,b.data(),case_traces,label,1);
    Transition t={"excluded"};
    float distToStartPoly=start.distance,distToEndPoly=end.distance;
    // ORIGINAL_FAR_PREDICATE
    if(start.selected && end.selected && !farFromPoly) t=path.selected_transition(start.selected,end.selected,a.data(),b.data());
    std::fprintf(out,"%s\t%s\t%s\t",label,geometry,scope);point(out,a.data());std::fputc('\t',out);point(out,b.data());std::fprintf(out,"\t%u\t%u\t",limit,active);refs(out,before);std::fputc('\t',out);print_lookup(out,start);std::fputc('\t',out);print_lookup(out,end);
    std::fprintf(out,"\t%s\t",t.phase);if(std::strcmp(t.phase,"replacement")==0) std::fprintf(out,"%08x",t.status);else std::fputc('-',out);
    std::fprintf(out,"\t%d\t%u\t",t.nodes,path.m_polyLength);refs(out,path.m_pathPolyRefs);std::fputc('\n',out);
}
static void observe_scan(Loaded& mesh,FILE* out,const char* label,const char* geometry,std::vector<dtPolyRef> path,std::array<float,3> p,float maxDist) {
    PathFinder finder(mesh,storage({path.front()}),1);float distance=-1;
    auto selected=finder.getPathPolyByPosition(path.data(),uint32(path.size()),p.data(),&distance,maxDist);
    std::fprintf(out,"%s\t%s\t",label,geometry);point(out,p.data());std::fprintf(out,"\t%08x\t",bits(maxDist));refs(out,path);
    std::fprintf(out,"\t%llu\t%08x\t",static_cast<unsigned long long>(selected),bits(distance));
    for(size_t i=0;i<path.size();++i) {
        float closest[3];auto status=mesh.query.closestPointOnPoly(path[i],p.data(),closest,nullptr);
        if(i)std::fputc(',',out);
        if(dtStatusFailed(status))std::fputc('-',out);else std::fprintf(out,"%08x",bits(dtVdistSqr(p.data(),closest)));
    }
    std::fputc('\n',out);
}
static void make_precision(const std::string& dir) {
    // Four authored quads: two adjacent at y=0, one overlapping y=1 floor, and
    // a detached y=0 island. All coordinates are small exactly representable integers.
    const unsigned short vertices[]={0,0,0,0,0,10,10,0,10,10,0,0,20,0,10,20,0,0,
        0,1,0,0,1,10,10,1,10,10,1,0,0,0,20,0,0,30,10,0,30,10,0,20};
    const unsigned short cells[4][4]={{0,1,2,3},{3,2,4,5},{6,7,8,9},{10,11,12,13}};
    unsigned short polys[48],flags[4]={1,1,1,1};for(auto& value:polys)value=0xffff;
    unsigned char areas[4]={11,11,11,11},tris[32];unsigned int details[16];float dv[48];
    for(int i=0;i<4;++i) {
        for(int j=0;j<4;++j) {polys[i*12+j]=cells[i][j];for(int k=0;k<3;++k)dv[i*12+j*3+k]=float(vertices[cells[i][j]*3+k]);}
        details[i*4]=i*4;details[i*4+1]=4;details[i*4+2]=i*2;details[i*4+3]=2;
        const unsigned char fan[8]={0,1,2,5,0,2,3,20};std::memcpy(&tris[i*8],fan,8);
    }
    polys[8]=1;polys[18]=0;
    dtNavMeshCreateParams cp={};cp.verts=vertices;cp.vertCount=14;cp.polys=polys;cp.polyFlags=flags;cp.polyAreas=areas;cp.polyCount=4;cp.nvp=6;
    cp.detailMeshes=details;cp.detailVerts=dv;cp.detailVertsCount=16;cp.detailTris=tris;cp.detailTriCount=8;
    cp.walkableHeight=2;cp.walkableRadius=.5f;cp.walkableClimb=.4f;cp.tileX=1;cp.tileY=1;cp.bmax[0]=cp.bmax[2]=GRID;cp.bmax[1]=20;cp.cs=1;cp.ch=1;cp.buildBvTree=true;
    unsigned char* data=nullptr;int size=0;require(dtCreateNavMeshData(&cp,&data,&size),"create precision mesh");
    uint32 h[]={0x4d4d4150,7,8,uint32(size),1};std::vector<unsigned char> file(20+size);std::memcpy(file.data(),h,20);std::memcpy(file.data()+20,data,size);dtFree(data);
    write(dir+"/precision.test-fixture",file.data(),file.size());
}
static std::array<float,3> exact_squared_point(Loaded& mesh,dtPolyRef ref,float target,float nominal) {
    // Find authored input coordinates whose actual loaded closestPoint computation
    // yields the requested binary32 squared distance, including adjacent float words.
    const std::array<float,3> base=nominal==3?std::array<float,3>{-1,1,-1}:std::array<float,3>{-2,1,-std::sqrt(2.0f)};
    for(int yi=-8;yi<=8;++yi)for(int zi=-8;zi<=8;++zi) {
        auto p=base;p[1]=std::bit_cast<float>(uint32(int64_t(bits(base[1]))+yi));p[2]=std::bit_cast<float>(uint32(int64_t(bits(base[2]))+zi));
        float closest[3];require(dtStatusSucceed(mesh.query.closestPointOnPoly(ref,p.data(),closest,nullptr)),"threshold closest");
        if(bits(dtVdistSqr(p.data(),closest))==bits(target))return p;
    }
    fail("exact adjacent squared-distance coordinate unavailable");
}
int main(int argc,char** argv) {
    require(argc==3,"usage: oracle OUTPUT FIXTURES");
    require(std::fegetround()==FE_TONEAREST && (_mm_getcsr()&0x8040)==0,"unsupported floating environment");
    const std::string dir=argv[1],existing=argv[2],direct=existing+"/direct-corridor";
    make_precision(dir);
    case_traces=std::fopen((dir+"/traces.test-fixture").c_str(),"w");require(case_traces,"open traces");
    std::fprintf(case_traces,"# Actual original lookup primitive calls via transparent original Detour forwarding observer\n# label\tendpoint\tordinal\tprimitive\tpolygon\tstatus\tdistance_bits\n");
    FILE* scans=std::fopen((dir+"/scans.test-fixture").c_str(),"w");require(scans,"open scans");
    FILE* lookups=std::fopen((dir+"/lookups.test-fixture").c_str(),"w");require(lookups,"open lookups");
    FILE* cases=std::fopen((dir+"/cases.test-fixture").c_str(),"w");require(cases,"open cases");
    std::fprintf(scans,"# Unchanged source getPathPolyByPosition, actual original Detour closestPointOnPoly; world XYZ bits\n# label\tgeometry\tpoint\tmax_distance\trefs\tselected_ref\tdistance\tsquared_distances\n");
    std::fprintf(lookups,"# Unchanged source getPolyByLocation, actual original loaded Detour; independent observation of consulted stages\n# label\tgeometry\tscope\tpoint\tlimit\tbefore_active\tbefore_storage\tselected\tdistance\torigin\tappended\tactive_ref\tactive_distance\ttail_checked\ttail_ref\ttail_distance\tnear5_checked\tnear5_status\tnear5_ref\tnear10_checked\tnear10_status\tnear10_ref\tafter_storage\n");
    std::fprintf(cases,"# Unchanged lookup bodies then scoped unchanged selected-reference storage fragments, stopping before BuildPointPath\n# label\tgeometry\tscope\tfrom\tto\tlimit\tbefore_active\tbefore_storage\tstart_ref\tstart_distance\tstart_origin\tstart_appended\tend_ref\tend_distance\tend_origin\tend_appended\tphase\tdirect_status\tdirect_nodes\tafter_active\tafter_storage\n");
    Loaded precision(read(direct+"/params.test-fixture"));precision.add(read(dir+"/precision.test-fixture"));
    const dtPolyRef A=precision.mesh.getPolyRefBase(static_cast<const dtNavMesh&>(precision.mesh).getTile(0)),B=A+1,U=A+2,C=A+3;
    for(auto [suffix,y]:std::vector<std::pair<const char*,float>>{{"below",std::nextafter(1.0f,0.0f)},{"at",1},{"above",std::nextafter(1.0f,2.0f)}})
        {
        float x=y<1?-0x1p-12f:y>1?-std::sqrt(0x1p-23f):0;
        if(y>1)y=1;
        observe_scan(precision,scans,(std::string("squared1_")+suffix).c_str(),"precision",{A,U},{x,y,5},3);
    }
    for(float nominal:{3.0f,7.0f})for(int delta:{-1,0,1}) {
        const float target=std::bit_cast<float>(uint32(int64_t(bits(nominal))+delta));
        auto p=exact_squared_point(precision,A,target,nominal);
        const auto label="squared"+std::to_string(int(nominal))+(delta<0?"_below":delta>0?"_above":"_at");
        observe_scan(precision,scans,label.c_str(),"precision",{A},p,nominal);
        if(nominal==3)observe_lookup(precision,lookups,label.c_str(),"precision","runtime",storage({A}),1,p);
        else observe_lookup(precision,lookups,label.c_str(),"precision","runtime",storage({C},{A}),1,p);
    }
    observe_scan(precision,scans,"ordered_tie_ab","precision",{A,B},{10,1,5},3);
    observe_scan(precision,scans,"ordered_tie_ba","precision",{B,A},{10,1,5},3);
    observe_scan(precision,scans,"early_break_before_closer","precision",{A,U},{5,.75f,5},3);
    observe_scan(precision,scans,"closer_order_reversed","precision",{U,A},{5,.75f,5},3);
    observe_lookup(precision,lookups,"active_before_tail","precision","runtime",storage({A},{U}),1,{5,.75f,5});
    observe_lookup(precision,lookups,"warm_append_smaller","precision","runtime",storage({B}),1,{5,0,5});
    observe_lookup(precision,lookups,"nearest_equal_no_append","precision","runtime",storage({A}),1,{5,-2,5});
    observe_lookup(precision,lookups,"tail_overwrites_prevents_append","precision","runtime",storage({B},{A}),1,{5,-3,5});
    observe_lookup(precision,lookups,"tail_overwrites_appends_duplicate","precision","runtime",storage({A},{C}),1,{5,-3,5});
    observe_lookup(precision,lookups,"nearest5_boundary","precision","runtime",storage({C}),1,{-5,0,5});
    observe_lookup(precision,lookups,"nearest10_fallback","precision","runtime",storage({C}),1,{-6,0,5});
    observe_lookup(precision,lookups,"nearest10_boundary","precision","raw",storage({C}),1,{-10,0,5});
    observe_lookup(precision,lookups,"nearest10_miss","precision","raw",storage({C}),1,{-11,0,5});
    observe_lookup(precision,lookups,"zero_inactive_tail_skipped","precision","runtime",storage({C},{0,A}),1,{5,0,5});
    auto inactive=storage({C});inactive[20]=A;
    observe_lookup(precision,lookups,"inactive_gap_not_scanned","precision","runtime",inactive,1,{5,0,5});
    observe_lookup(precision,lookups,"cold_without_tail_excluded","precision","excluded",storage({}),0,{5,0,5});
    observe_lookup(precision,lookups,"zero_active_eligible_tail_helper_only","precision","helper",storage({},{A}),0,{5,0,5});
    auto lower=storage({C});lower[73]=A;
    observe_lookup(precision,lookups,"lower_limit_exposes_zero_gap_helper_only","precision","helper",lower,1,{5,0,5},2);
    observe_case(precision,cases,"active_before_tail","precision","runtime",storage({A,B},{U}),2,{5,.75f,5},{15,0,5});
    observe_case(precision,cases,"start_append_then_end_tail_hit","precision","runtime",inactive,1,{5,0,5},{6,0,5});
    observe_case(precision,cases,"replacement_retains_tail","precision","runtime",storage({B},{C}),1,{5,0,5},{15,0,5});
    auto duplicate=storage({C,A,B,A,B,C,B},{U,A});duplicate[20]=C;
    observe_case(precision,cases,"cut_first_start_last_end_duplicates","precision","runtime",duplicate,7,{5,0,5},{15,0,5});
    auto same=storage({A,B,C},{B,A});same[20]=C;
    observe_case(precision,cases,"same_polygon_clear_retains_storage","precision","runtime",same,3,{5,0,5},{6,0,5});
    observe_case(precision,cases,"reverse_order_replaces","precision","runtime",storage({A,B},{C}),2,{15,0,5},{5,0,5});
    observe_case(precision,cases,"nearest_equal_no_append","precision","runtime",storage({A,B}),2,{5,-2,5},{15,0,5});
    observe_case(precision,cases,"tail_overwrite_duplicate_append","precision","runtime",storage({A,B},{C}),2,{5,-3,5},{15,0,5});
    for(auto [suffix,distance]:std::vector<std::pair<const char*,float>>{{"below",std::nextafter(7.0f,0.0f)},{"at",7},{"above",std::nextafter(7.0f,8.0f)}}) {
        const auto label=std::string("far_distance7_")+suffix;
        observe_lookup(precision,lookups,label.c_str(),"precision",distance>7?"raw":"runtime",storage({C}),1,{-distance,0,5});
        observe_case(precision,cases,label.c_str(),"precision",distance>7?"excluded":"runtime",storage({C}),1,{-distance,0,5},{15,0,5});
    }
    observe_case(precision,cases,"nearest10_fallback","precision","runtime",storage({C}),1,{-6,0,5},{15,0,5});
    observe_case(precision,cases,"disconnected_partial","precision","raw",storage({A}),1,{5,0,5},{5,0,25});
    observe_case(precision,cases,"nearest_miss_excluded","precision","excluded",storage({C}),1,{-11,0,5},{15,0,5});
    observe_case(precision,cases,"cold_without_tail_excluded","precision","excluded",storage({}),0,{5,0,5},{15,0,5});
    Loaded basic(read(existing+"/authored.mmap"));basic.add(read(existing+"/0003232.mmtile"));basic.add(read(existing+"/0003231.mmtile"));
    observe_case(basic,cases,"cross_tile_replacement","existing","runtime",storage({A}),1,{-400,0,-400},{400,7,-400});
    Loaded grid(read(direct+"/params.test-fixture"));
    for(const char* name:{"grid00.test-fixture","grid10.test-fixture","grid01.test-fixture","grid11.test-fixture"})grid.add(read(direct+"/"+name));
    const std::array<float,3> ga={-GRID+GRID*.40f,0,-GRID+GRID*.90f},gb={-GRID+GRID*1.95f,0,-GRID+GRID*1.05f};
    observe_case(grid,cases,"cross_side_original_direct_order","grid","runtime",storage({A}),1,ga,gb);
    for(int n:{600,1100}) {
        const auto name="long"+std::to_string(n);Loaded mesh(read(direct+"/params.test-fixture"));mesh.add(read(direct+"/"+name+".test-fixture"));
        const float cs=GRID/float(n);const std::array<float,3> start={-GRID+cs*.5f,0,-GRID+cs*50},end={-GRID+cs*(float(n)-.5f),0,start[2]};
        observe_case(mesh,cases,(name+(n==600?"_truncated":"_out_of_nodes")).c_str(),name.c_str(),"raw",storage({A}),1,start,end);
        if(n==1100) {
            observe_case(mesh,cases,"seventyfour_exact_capacity","long1100","runtime",storage({A}),1,start,{-GRID+cs*73.5f,0,start[2]});
            observe_case(mesh,cases,"seventyfive_truncated","long1100","raw",storage({A}),1,start,{-GRID+cs*74.5f,0,start[2]});
        }
    }
    require(std::fclose(scans)==0 && std::fclose(lookups)==0 && std::fclose(cases)==0 && std::fclose(case_traces)==0,"close observations");
}
