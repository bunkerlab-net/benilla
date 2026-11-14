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
#ifdef POINT_PATH_REPAIR_TILE_LIFETIME
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
struct DiagnosticLog { template<class... T> void outError(T...) { fail("invalid active reference diagnostic"); } };
static DiagnosticLog sLog;

struct Transition { const char* phase="cut"; dtStatus status=0; };
class PathFinder {
public:
    dtNavMesh* m_navMesh;
    dtNavMeshQuery* m_navMeshQuery;
    dtQueryFilter m_filter;
    std::vector<dtPolyRef> m_pathPolyRefs;
    uint32 m_polyLength;
    uint32 m_pointPathLimit;
    PointsArray m_pathPoints;
    std::vector<float> m_cachedPoints;
    std::vector<dtPolyRef> m_smoothPathPolyRefs;
    PathType m_type=PATHFIND_BLANK;
    bool m_straightLine=false,m_useStraightPath=false,m_forceDestination=false,m_ignoreNormalization=false;
    Vector3 m_startPosition,m_endPosition,m_actualEndPosition;
    DiagnosticOwner owner;
    DiagnosticOwner* m_sourceUnit=&owner;
    Transition transition;
    PathFinder(Loaded& mesh,std::vector<dtPolyRef> storage,uint32 active,uint32 limit=74)
        :m_navMesh(&mesh.mesh),m_navMeshQuery(&mesh.query),m_pathPolyRefs(std::move(storage)),m_polyLength(active),
         m_pointPathLimit(limit),m_cachedPoints(limit*3),m_smoothPathPolyRefs(limit) {
        require(limit>=2&&limit<=74&&active<=limit&&m_pathPolyRefs.size()>=limit&&m_pathPolyRefs.size()<=4096,"cache bounds");
        for(uint32 i=0;i<active;++i)require(mesh.mesh.isValidPolyRef(m_pathPolyRefs[i]),"valid active reference");
        for(auto ref:m_pathPolyRefs)require(!ref||mesh.mesh.isValidPolyRef(ref),"valid retained reference");
        m_filter.setIncludeFlags(1);m_filter.setExcludeFlags(0);
    }
    // ORIGINAL_CLEAR
    // ORIGINAL_GETTERS
    // ORIGINAL_SETTERS
    // ORIGINAL_LOOKUP_DECLARATIONS
    void BuildPointPath(const float*,const float*);
    void BuildShortcut();
    void NormalizePath();
    uint32 fixupCorridor(dtPolyRef*,uint32,uint32,const dtPolyRef*,uint32);
    bool getSteerTarget(const float*,const float*,float,const dtPolyRef*,uint32,float*,unsigned char&,dtPolyRef&) const;
    dtStatus findSmoothPath(const float*,const float*,const dtPolyRef*,uint32,float*,int*,uint32);
    bool inRangeYZX(const float*,const float*,float,float) const;
    bool inRange(const Vector3&,const Vector3&,float,float) const;
    float dist3DSqr(const Vector3&,const Vector3&) const;
    void selected_transition(dtPolyRef startPoly,dtPolyRef endPoly,const float* startPoint,const float* endPoint) {
        const bool farFromPoly=false; // admitted by actual warm distance checks below
        transition.phase=startPoly==endPoly?"same":"cut";
        // ORIGINAL_SAME
        [[maybe_unused]] DiagnosticPoint startPos,endPos;
        // ORIGINAL_SCAN_AND_CUT
        else {
            clear();
            dtStatus dtResult;
            // ORIGINAL_DIRECT_CALL
            transition={"replacement",dtResult};
            require(dtResult==DT_SUCCESS&&m_polyLength>0,"only complete original direct status admitted");
        }
    }
    void classify(dtPolyRef endPoly) {
        // ORIGINAL_CLASSIFICATION
    }
    // Explicit raw postprocessing control. Inputs are authored observations, not
    // scripted Detour answers. Only the two unchanged source spans below are evidence.
    void raw_postprocess(std::vector<float>& raw,dtStatus dtResult) {
        require(raw.size()%3==0&&raw.size()/3<=m_pointPathLimit,"raw point bounds");
        uint32 pointCount=uint32(raw.size()/3);
        float* pathPoints=raw.data();
        // ORIGINAL_RAW_OPTIMIZE_CONVERT
        // ORIGINAL_RAW_TERMINAL
    }
};
// ORIGINAL_METHODS
static uint32 bits(float v) {return std::bit_cast<uint32>(v);}
static Vector3 world(const std::array<float,3>& p) {return {p[2],p[0],p[1]};}
static void point(FILE* out,const Vector3& p) {std::fprintf(out,"%08x,%08x,%08x",bits(p.x),bits(p.y),bits(p.z));}
static void points(FILE* out,const PointsArray& values) {
    if(values.empty())std::fputc('-',out);
    for(size_t i=0;i<values.size();++i){if(i)std::fputc(';',out);point(out,values[i]);}
}
static PointsArray converted(const std::vector<float>& raw) {
    PointsArray result;
    for(size_t i=0;i<raw.size();i+=3)result.emplace_back(raw[i+2],raw[i],raw[i+1]);
    return result;
}
static void refs(FILE* out,const std::vector<dtPolyRef>& values) {
    for(size_t i=0;i<values.size();++i)std::fprintf(out,"%s%llu",i?",":"",static_cast<unsigned long long>(values[i]));
}
static std::vector<dtPolyRef> storage(std::initializer_list<dtPolyRef> active,std::initializer_list<dtPolyRef> tail={}) {
    std::vector<dtPolyRef> result(active);result.resize(74);result.insert(result.end(),tail);return result;
}
static void configure(PathFinder& path,Vector3 from,Vector3 to,bool optimize,bool normalize,bool ignore,bool force,uint32 previous) {
    require(previous<=63&&(!normalize||ignore),"admitted settings only");
    sWorld={optimize,normalize};path.m_ignoreNormalization=ignore;path.m_forceDestination=force;
    path.m_type=PathType(previous);path.setStartPosition(from);path.setEndPosition(to);
}
static void result(FILE* out,const PathFinder& path) {
    std::fprintf(out,"\t%u\t",path.m_polyLength);refs(out,path.m_pathPolyRefs);
    std::fprintf(out,"\t%u\t",unsigned(path.m_type));point(out,path.getActualEndPosition());std::fputc('\t',out);points(out,path.m_pathPoints);std::fputc('\n',out);
}
static void observe(Loaded& mesh,FILE* out,const char* label,const char* geometry,std::vector<dtPolyRef> before,uint32 active,
                    std::array<float,3> from,std::array<float,3> to,bool optimize=false,bool normalize=false,bool ignore=false,bool force=false,uint32 previous=0) {
    require(active>0,"uninitialized cold lookup excluded");
    std::fprintf(stderr,"observe %s\n",label);
    PathFinder path(mesh,before,active);configure(path,world(from),world(to),optimize,normalize,ignore,force,previous);
    float ds,de; // unchanged source active scan initializes both before they are read
    auto startPoly=path.getPolyByLocation(from.data(),&ds),endPoly=path.getPolyByLocation(to.data(),&de);
    require(startPoly&&endPoly&&std::isfinite(ds)&&std::isfinite(de)&&ds<=7&&de<=7,"bounded warm selection");
    path.selected_transition(startPoly,endPoly,from.data(),to.data());
    auto selected=path.m_pathPolyRefs;auto selected_active=path.m_polyLength;
    dtStatus smooth=0;std::vector<float> raw;
    if(std::strcmp(path.transition.phase,"same")!=0) {
        path.classify(endPoly);
        // Independent identical-input original smooth probe records raw outputs without
        // modifying the subsequently executed full BuildPointPath object's storage.
        PathFinder probe(mesh,selected,selected_active);raw.resize(74*3);int count=0;
        smooth=probe.findSmoothPath(from.data(),to.data(),selected.data(),selected_active,raw.data(),&count,74);
        require(count>=0&&count<=74,"original smooth point bounds");raw.resize(size_t(count)*3);
        path.BuildPointPath(from.data(),to.data());
        require(raw.empty()||std::memcmp(raw.data(),path.m_cachedPoints.data(),raw.size()*sizeof(float))==0,"smooth probe agrees bitwise with original BuildPointPath");
    }
    std::fprintf(out,"%s\t%s\t",label,geometry);point(out,world(from));std::fputc('\t',out);point(out,world(to));
    std::fprintf(out,"\t%u\t",active);refs(out,before);
    std::fprintf(out,"\t%u\t%u\t%u\t%u\t%u\t%s\t%u\t",unsigned(optimize),unsigned(normalize),unsigned(ignore),unsigned(force),previous,path.transition.phase,selected_active);refs(out,selected);std::fputc('\t',out);
    if(std::strcmp(path.transition.phase,"same")==0)std::fputc('-',out);else std::fprintf(out,"%08x",smooth);
    std::fputc('\t',out);points(out,converted(raw));result(out,path);
}
static void raw_case(Loaded& mesh,FILE* out,const char* label,PointsArray input,Vector3 from,Vector3 to,bool optimize=false,bool force=false,uint32 previous=1,uint32 limit=74,dtStatus status=DT_SUCCESS,PointsArray prior={}) {
    dtPolyRef A=mesh.mesh.getPolyRefBase(static_cast<const dtNavMesh&>(mesh.mesh).getTile(0));
    auto before=storage({A},{A});PathFinder path(mesh,before,1,limit);configure(path,from,to,optimize,false,false,force,previous);path.m_pathPoints=prior;
    std::vector<float> raw;for(const auto& p:input){raw.push_back(p.y);raw.push_back(p.z);raw.push_back(p.x);}
    path.raw_postprocess(raw,status);
    std::fprintf(out,"%s\t%u\t%u\t0\t0\t%u\t%u\t",label,limit,unsigned(optimize),unsigned(force),previous);point(out,from);std::fputc('\t',out);point(out,to);std::fputc('\t',out);points(out,prior);
    std::fprintf(out,"\t%08x\t",status);points(out,input);std::fprintf(out,"\t1\t");refs(out,before);result(out,path);
}
int main(int argc,char** argv) {
    require(argc==3,"usage: oracle OUTPUT FIXTURES");
    require(std::fegetround()==FE_TONEAREST&&(_mm_getcsr()&0x8040)==0,"unsupported floating environment");
    static_assert(MAX_POINT_PATH_LENGTH==74&&MAX_PATH_LENGTH==74&&SKIP_POINT_LIMIT==6&&VERTEX_SIZE==3);
    require(LINE_FAULT==0.5f,"original line threshold");
    const std::string dir=argv[1],existing=argv[2];
    FILE* out=std::fopen((dir+"/cases.test-fixture").c_str(),"w");require(out,"open cases");
    std::fprintf(out,"# Original loaded warm selection then full original smooth/BuildPointPath; all points world XYZ binary32\n# label\tgeometry\tfrom\tto\tbefore_active\tbefore_storage\toptimize\tnormalize\tignore\tforce\tprevious_type\tphase\tselected_active\tselected_storage\tsmooth_status\tsmooth_points\tafter_active\tafter_storage\tpath_type\tactual_end\tpoints\n");
    Loaded precision(read(existing+"/direct-corridor/params.test-fixture"));precision.add(read(existing+"/warm-cache/precision.test-fixture"));
    const dtPolyRef A=precision.mesh.getPolyRefBase(static_cast<const dtNavMesh&>(precision.mesh).getTile(0)),B=A+1,U=A+2,C=A+3;
    observe(precision,out,"replacement_conversion","precision",storage({B},{C}),1,{5,0,5},{15,0,5});
    observe(precision,out,"cut_conversion","precision",storage({C,A,B},{U}),3,{5,0,5},{15,0,5});
    observe(precision,out,"cut_optimize_mutated_previous","precision",storage({A,B},{U}),2,{5,0,5},{19,0,5},true);
    observe(precision,out,"replacement_optimize","precision",storage({B},{C}),1,{5,0,5},{19,0,5},true);
    observe(precision,out,"prior_incomplete_retained","precision",storage({A,B},{U}),2,{5,0,5},{15,0,5},false,false,false,false,4);
    observe(precision,out,"prior_short_classified_normal","precision",storage({A,B}),2,{5,0,5},{15,0,5},false,false,false,false,32);
    observe(precision,out,"prior_incomplete_force_endpoint","precision",storage({A,B},{U}),2,{5,0,5},{15,0,5},false,false,false,true,4);
    observe(precision,out,"force_far_horizontal_shortcut","precision",storage({A,B},{U}),2,{9,0,.1f},{11,0,-7},false,false,false,true);
    observe(precision,out,"force_near_horizontal_replace_endpoint","precision",storage({A,B},{U}),2,{5,0,5},{15,0,12},false,false,false,true);
    observe(precision,out,"normalize_ignored","precision",storage({A,B}),2,{5,0,5},{15,0,5},true,true,true);
    observe(precision,out,"same_polygon_source_order","precision",storage({A,B,C},{U}),3,{5,0,5},{6,.75f,5},true,false,false,true,4);
    observe(precision,out,"same_start_append_end_tail","precision",storage({C},{U}),1,{5,0,5},{6,0,5});
    observe(precision,out,"no_steer_vertical_cached_cut","precision",storage({A,U}),2,{5,0,5},{5,1.1f,5});
    observe(precision,out,"original_zero_input_raw_observation","precision",storage({A,B}),2,{0,0,0},{15,0,5},true);
    observe(precision,out,"warm_active_before_nearer_floor","precision",storage({A,B},{U}),2,{5,.75f,5},{15,0,5});
    observe(precision,out,"clamped_start_and_end","precision",storage({A,B}),2,{-1,0,5},{21,0,5});
    Loaded ground(read(existing+"/smoothing/params.test-fixture"));ground.add(read(existing+"/smoothing/ground.test-fixture"));
    auto slope=storage({A+1,A+2,A+3,A+4,A+5,A+6,A+7,A+8},{A});
    for(bool optimize:{false,true})observe(ground,out,optimize?"slope_optimize":"slope_raw","ground",slope,8,{-GRID+101,.0625f,-GRID+304},{-GRID+163,3.9375f,-GRID+304},optimize);
    auto snake=storage({A+9,A+10,A+11,A+12,A+13,A+14,A+15,A+16,A+17,A+18,A+19,A+20,A+21,A+22,A+23},{A});
    for(bool optimize:{false,true})observe(ground,out,optimize?"snake_optimize":"snake_raw","ground",snake,15,{-GRID+101,.0625f,-GRID+204},{-GRID+171,4.4375f,-GRID+252},optimize);
    Loaded thin(read(existing+"/smoothing/params.test-fixture"));thin.add(read(existing+"/smoothing/thin.test-fixture"));
    auto thinrefs=storage({A});for(uint32 i=0;i<40;++i)thinrefs[i]=A+i;
    observe(thin,out,"thin_fixup_height_detail","thin",thinrefs,40,{-GRID+125.05f,0,-GRID+202},{-GRID+129.95f,0,-GRID+202});
    Loaded basic(read(existing+"/authored.mmap"));basic.add(read(existing+"/0003232.mmtile"));basic.add(read(existing+"/0003231.mmtile"));
    for(bool optimize:{false,true})observe(basic,out,optimize?"cross_tile_failure_optimized":"cross_tile_failure","existing",storage({A}),1,{-400,0,-400},{400,7,-400},optimize);
    require(std::fclose(out)==0,"close cases");
    FILE* raw=std::fopen((dir+"/postprocess.test-fixture").c_str(),"w");require(raw,"open raw controls");
    std::fprintf(raw,"# Authored raw postprocessing controls; unchanged source spans, NOT loaded smooth or world evidence\n# label\tlimit\toptimize\tnormalize\tignore\tforce\tprevious_type\tfrom\tto\tprior_points\traw_status\traw_points\tbefore_active\tbefore_storage\tafter_active\tafter_storage\tpath_type\tactual_end\tpoints\n");
    raw_case(precision,raw,"raw_zero_count",{}, {1,2,3},{4,5,6});
    raw_case(precision,raw,"raw_one_count",{{7,8,9}}, {1,2,3},{4,5,6});
    raw_case(precision,raw,"raw_failure_precedes_short",{{1,2,3},{4,5,6}}, {1,2,3},{4,5,6},false,false,1,2,DT_FAILURE);
    raw_case(precision,raw,"raw_short_equal_limit",{{1,2,3},{4,5,6}}, {1,2,3},{4,5,6},false,false,1,2);
    for(int delta:{-1,0,1}) {
        float y=std::bit_cast<float>(uint32(int64_t(bits(.25f))+delta));
        raw_case(precision,raw,delta<0?"line_fault_below":delta?"line_fault_above":"line_fault_equal",{{1,0,1},{2,y,1},{3,0,1}}, {1,0,1},{3,0,1},true);
    }
    PointsArray collinear;for(int i=1;i<=17;++i)collinear.emplace_back(float(i),float(i),2);
    raw_case(precision,raw,"skip_limit_six",collinear,collinear.front(),collinear.back(),true);
    PointsArray full;for(int i=1;i<=74;++i)full.emplace_back(float(i),float(i),2);
    raw_case(precision,raw,"raw_optimize_before_limit",full,full.front(),full.back(),true);
    raw_case(precision,raw,"raw_failure_after_optimize",full,full.front(),full.back(),true,false,1,74,DT_FAILURE);
    raw_case(precision,raw,"mutated_previous_not_original_collinearity",{{1,1,1},{2,1,1},{3,1,1},{4,1,1},{5,1,1}}, {1,1,1},{5,1,1},true);
    raw_case(precision,raw,"zero_input_fresh_default",{{0,0,0},{1,1,1},{2,2,2}}, {0,0,0},{2,2,2},true);
    raw_case(precision,raw,"zero_input_prior_survives",{{0,0,0},{1,1,1},{2,2,2}}, {0,0,0},{2,2,2},true,false,1,74,DT_SUCCESS,{{9,8,7},{6,5,4}});
    raw_case(precision,raw,"signed_zero_input_prior_survives",{{-0.0f,0,-0.0f},{1,1,1},{2,2,2}}, {0,0,0},{2,2,2},true,false,1,74,DT_SUCCESS,{{9,8,7},{6,5,4}});
    for(int delta:{-1,0,1}) {
        float q=std::bit_cast<float>(uint32(int64_t(bits(1.0f))+delta*2));
        raw_case(precision,raw,delta<0?"force_ratio_below":delta?"force_ratio_above":"force_ratio_equal",{{3,1,0},{1,1,q}}, {3,1,0},{0,0,0},false,true,4);
    }
    for(int delta:{-1,0,1}) {
        float q=std::bit_cast<float>(uint32(int64_t(bits(1.0f))+delta));
        raw_case(precision,raw,delta<0?"range_radius_below":delta?"range_radius_above":"range_radius_equal",{{3,1,0},{q,0,0}}, {3,1,0},{0,0,0},false,true);
        raw_case(precision,raw,delta<0?"range_height_below":delta?"range_height_above":"range_height_equal",{{3,1,0},{0,0,q}}, {3,1,0},{0,0,0},false,true);
    }
    raw_case(precision,raw,"force_incomplete_even_in_range",{{3,1,0},{.5f,0,0}}, {3,1,0},{0,0,0},false,true,4);
    raw_case(precision,raw,"force_normal_already_in_range",{{3,1,0},{.5f,0,0}}, {3,1,0},{0,0,0},false,true,1);
    require(std::fclose(raw)==0,"close raw controls");
}
