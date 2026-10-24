// SPDX-License-Identifier: GPL-3.0-or-later
// Test-only unchanged source methods, transparent real-Detour observation, authored geometry.
#include "DetourNavMeshBuilder.h"
#include "DetourNavMesh.h"
#include "DetourNavMeshQuery.h"
#include "DetourCommon.h"
#include "DetourMath.h"
#include "DetourAlloc.h"
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
// PINNED_CONSTANTS
static_assert(MAX_POINT_PATH_LENGTH == 74 && MAX_PATH_LENGTH == 74 && VERTEX_SIZE == 3);
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
#ifdef SMOOTHING_REPAIR_TILE_LIFETIME
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
struct Diagnostics {
    uint32 boundary_calls = 0, steer_calls = 0, move_calls = 0, height_calls = 0, steer_buffer_calls = 0;
    dtStatus boundary_status = 0, steer_status = 0, move_status = 0, height_status = 0;
};
struct ObservedQuery {
    dtNavMeshQuery* actual;
    Diagnostics d;
    std::vector<std::vector<dtPolyRef>> visits;
    uint32 height_failures=0, move_buffer_calls=0;
    dtStatus closestPointOnPolyBoundary(dtPolyRef ref, const float* p, float* out) {
        ++d.boundary_calls; return d.boundary_status = actual->closestPointOnPolyBoundary(ref, p, out);
    }
    dtStatus findStraightPath(const float* a, const float* b, const dtPolyRef* path, int n,
                             float* pts, unsigned char* flags, dtPolyRef* refs, int* count, int cap) {
        ++d.steer_calls; d.steer_status = actual->findStraightPath(a, b, path, n, pts, flags, refs, count, cap);
        if (d.steer_status & DT_BUFFER_TOO_SMALL) ++d.steer_buffer_calls;
        return d.steer_status;
    }
    dtStatus moveAlongSurface(dtPolyRef ref, const float* a, const float* b, const dtQueryFilter* filter,
                              float* out, dtPolyRef* visited, int* count, int cap) {
        ++d.move_calls; d.move_status = actual->moveAlongSurface(ref, a, b, filter, out, visited, count, cap);
        require((d.move_status == DT_SUCCESS || d.move_status == (DT_SUCCESS | DT_BUFFER_TOO_SMALL)) && *count >= 0, "unsupported source move failure");
        if(d.move_status & DT_BUFFER_TOO_SMALL) ++move_buffer_calls;
        visits.emplace_back(visited,visited+*count);
        return d.move_status;
    }
    dtStatus getPolyHeight(dtPolyRef ref, const float* p, float* height) {
        ++d.height_calls; d.height_status = actual->getPolyHeight(ref, p, height);
        if(d.height_status != DT_SUCCESS) ++height_failures;
        // Source ignores this status; successful move already initialized the height.
        return d.height_status;
    }
};
struct PathFinder {
    uint32 m_pointPathLimit = MAX_POINT_PATH_LENGTH;
    std::vector<dtPolyRef> m_smoothPathPolyRefs = std::vector<dtPolyRef>(MAX_POINT_PATH_LENGTH);
    dtNavMesh* m_navMesh;
    ObservedQuery* m_navMeshQuery;
    dtQueryFilter m_filter;
    uint32 fixupCorridor(dtPolyRef*, uint32, uint32, const dtPolyRef*, uint32);
    bool getSteerTarget(const float*, const float*, float, const dtPolyRef*, uint32,
                        float*, unsigned char&, dtPolyRef&) const;
    dtStatus findSmoothPath(const float*, const float*, const dtPolyRef*, uint32, float*, int*, uint32);
    bool inRangeYZX(const float*, const float*, float, float) const;
};
// PINNED_METHODS

struct Loaded {
    std::vector<void*> owners;
    dtNavMesh mesh;
    dtNavMeshQuery query;
    explicit Loaded(const std::vector<unsigned char>& params) {
        dtNavMeshParams p; require(params.size() == sizeof(p), "parameter size"); std::memcpy(&p, params.data(), sizeof(p));
        require(mesh.init(&p) == DT_SUCCESS && query.init(&mesh, 512) == DT_SUCCESS, "initialize native mesh");
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
static void make_geometry(const std::string& dir, bool thin=false) {
    dtNavMeshParams params = {{-GRID, 0, -GRID}, GRID, GRID, 4, INT32_MIN};
    write(dir + "/params.test-fixture", &params, sizeof(params));
    using V = std::array<unsigned short, 3>;
    std::vector<V> vertices; std::vector<std::array<unsigned short, 4>> cells;
    std::map<V, unsigned short> ids;
    auto vertex = [&](int x, int z, int height) {
        V v = {static_cast<unsigned short>(x), static_cast<unsigned short>(height), static_cast<unsigned short>(z)};
        auto found = ids.find(v); if (found != ids.end()) return found->second;
        const auto id = static_cast<unsigned short>(vertices.size()); ids.emplace(v, id); vertices.push_back(v); return id;
    };
    auto rect = [&](int x0, int z0, int x1, int z1, int h0, int h1) {
        cells.push_back({vertex(x0,z0,h0),vertex(x0,z1,h0),vertex(x1,z1,h1),vertex(x1,z0,h1)});
    };
    if(thin) {
        for(int i=0;i<40;++i) rect(1000+i,1600,1001+i,1664,0,0);
    } else {
    rect(100,100,132,132,0,0); // flat interior and clamp controls
    for (int i=0; i<8; ++i) rect(100+i*8,300,108+i*8,308,i*2,(i+1)*2); // gentle planar slope
    const std::array<std::array<int,2>,15> snake = {{{0,0},{1,0},{2,0},{2,1},{2,2},{3,2},{4,2},{4,3},{4,4},{5,4},{6,4},{6,5},{6,6},{7,6},{8,6}}};
    for (auto c:snake) rect(100+c[0]*8,200+c[1]*8,108+c[0]*8,208+c[1]*8,c[0]*2,(c[0]+1)*2);
    rect(100,400,500,408,0,0); // 400-unit point-limit control
    }
    const int n = int(cells.size());
    std::vector<unsigned short> polys(n*12,0xffff), flags(n,1);
    std::vector<unsigned char> areas(n,11), tris(n*8);
    std::vector<unsigned int> details(n*4);
    std::vector<float> detail_vertices(n*12);
    for (int i=0; i<n; ++i) {
        for(int j=0;j<4;++j) {
            polys[i*12+j]=cells[i][j];
            for(int k=0;k<3;++k) detail_vertices[i*12+j*3+k]=(k==1?0:-GRID)+vertices[cells[i][j]][k]*(k==1?.25f:(thin?.125f:1.0f));
            for(int other=0;other<n;++other) if(other!=i) for(int edge=0;edge<4;++edge)
                if(cells[i][j]==cells[other][(edge+1)%4] && cells[i][(j+1)%4]==cells[other][edge]) polys[i*12+6+j]=static_cast<unsigned short>(other);
        }
        details[i*4]=i*4; details[i*4+1]=4; details[i*4+2]=i*2; details[i*4+3]=2;
        const unsigned char fan[8]={0,1,2,5,0,2,3,20}; std::memcpy(&tris[i*8],fan,8);
    }
    dtNavMeshCreateParams cp={}; cp.verts=vertices[0].data(); cp.vertCount=int(vertices.size());
    cp.polys=polys.data(); cp.polyFlags=flags.data(); cp.polyAreas=areas.data(); cp.polyCount=n; cp.nvp=6;
    cp.detailMeshes=details.data(); cp.detailVerts=detail_vertices.data(); cp.detailVertsCount=n*4;
    cp.detailTris=tris.data(); cp.detailTriCount=n*2; cp.walkableHeight=2;cp.walkableRadius=.5f;cp.walkableClimb=.4f;
    cp.bmin[0]=cp.bmin[2]=-GRID; cp.bmax[1]=20;cp.cs=thin?.125f:1.0f;cp.ch=.25f;cp.buildBvTree=true;
    unsigned char* data=nullptr;int size=0;require(dtCreateNavMeshData(&cp,&data,&size),"create authored native mesh");
    uint32 header[]={0x4d4d4150,7,8,uint32(size),1};std::vector<unsigned char> file(20+size);
    std::memcpy(file.data(),header,20);std::memcpy(file.data()+20,data,size);dtFree(data);
    write(dir+(thin?"/thin.test-fixture":"/ground.test-fixture"),file.data(),file.size());
}
static void point(FILE* out, const float* p) {
    std::fprintf(out,"%08x,%08x,%08x",std::bit_cast<uint32>(p[2]),std::bit_cast<uint32>(p[0]),std::bit_cast<uint32>(p[1]));
}
static void refs(FILE* out, const dtPolyRef* p, int n) {
    if(!n) std::fprintf(out,"-");
    for(int i=0;i<n;++i) std::fprintf(out,"%s%llu",i?",":"",static_cast<unsigned long long>(p[i]));
}
struct Near {dtStatus status=0;dtPolyRef ref=0;float point[3]={};bool over=false;unsigned attempts=0;};
static Near nearest(dtNavMeshQuery& q, const float* p, const dtQueryFilter& f) {
    Near out;
    for(float size:{5.0f,10.0f}) {
        const float ext[3]={size,size,size}; ++out.attempts;
        out.status=q.findNearestPoly(p,ext,&f,&out.ref,out.point,&out.over);
        if(dtStatusSucceed(out.status)&&out.ref) break;
    }
    return out;
}
static void observe(Loaded& loaded, FILE* out, const char* label, const char* geometry,
                    std::array<float,3> a, std::array<float,3> b, uint32 limit=74) {
    std::fprintf(stderr,"observe %s\n",label);
    require(limit>=2&&limit<=74,"point limit domain");
    dtQueryFilter filter;filter.setIncludeFlags(1);filter.setExcludeFlags(0);
    auto from=nearest(loaded.query,a.data(),filter), to=nearest(loaded.query,b.data(),filter);
    require(from.ref&&to.ref,"case must have nearest ground refs");
    dtPolyRef corridor[74]={}; int nc=0;
    const dtStatus cs=loaded.query.findPath(from.ref,to.ref,a.data(),b.data(),&filter,corridor,&nc,int(limit));
    ObservedQuery oq{&loaded.query,{}, {}, 0, 0};PathFinder path;path.m_navMesh=&loaded.mesh;path.m_navMeshQuery=&oq;
    path.m_filter=filter;path.m_pointPathLimit=limit;
    std::array<float,74*3> pts={};int np=0;dtStatus status=0;
    float bounded_from[3]={}, bounded_to[3]={};
    dtStatus probe_status=0;int probe_count=0;dtStatus full_status=0;int full_count=0;
    const char* stop="corridor_rejected";
    if(cs==DT_SUCCESS&&nc>0) {
        require(loaded.query.closestPointOnPolyBoundary(corridor[0],a.data(),bounded_from)==DT_SUCCESS,"initial boundary");
        require(loaded.query.closestPointOnPolyBoundary(corridor[nc-1],b.data(),bounded_to)==DT_SUCCESS,"target boundary");
        float steer[9]={};unsigned char flags[3]={};dtPolyRef sp[3]={};
        probe_status=loaded.query.findStraightPath(bounded_from,bounded_to,corridor,nc,steer,flags,sp,&probe_count,3);
        float all[74*3]={};unsigned char all_flags[74]={};dtPolyRef all_refs[74]={};
        full_status=loaded.query.findStraightPath(bounded_from,bounded_to,corridor,nc,all,all_flags,all_refs,&full_count,74);
        status=path.findSmoothPath(a.data(),b.data(),corridor,nc,pts.data(),&np,limit);
        if(np>1&&std::memcmp(pts.data()+(np-1)*3,bounded_to,12)==0) stop="target_reached";
        else if(np>=int(limit)) stop="point_limit";
        else stop="no_steer_target";
    }
    std::fprintf(out,"%s\t%s\t",label,geometry);point(out,a.data());std::fputc('\t',out);point(out,b.data());
    std::fprintf(out,"\t%u\t%08x\t%llu\t",limit,from.status,static_cast<unsigned long long>(from.ref));point(out,from.point);
    std::fprintf(out,"\t%u\t%u\t%08x\t%llu\t",unsigned(from.over),from.attempts,to.status,static_cast<unsigned long long>(to.ref));point(out,to.point);
    std::fprintf(out,"\t%u\t%u\t%08x\t",unsigned(to.over),to.attempts,cs);refs(out,corridor,nc);
    std::fputc('\t',out);point(out,bounded_from);std::fputc('\t',out);point(out,bounded_to);
    std::fprintf(out,"\t%08x\t%d\t%08x\t%s\t",probe_status,probe_count,status,stop);
    if(!np) std::fputc('-',out);
    for(int i=0;i<np;++i){if(i)std::fputc(';',out);point(out,pts.data()+i*3);}
    auto d=oq.d;
    std::fprintf(out,"\t%u\t%u\t%u\t%u\t%u\t%08x\t%08x\t%08x\t%08x",d.boundary_calls,d.steer_calls,d.move_calls,d.height_calls,d.steer_buffer_calls,d.boundary_status,d.steer_status,d.move_status,d.height_status);
    std::fprintf(out,"\t%08x\t%d\t",full_status,full_count);refs(out,path.m_smoothPathPolyRefs.data(),74);
    dtPolyRef replay[74]={};std::memcpy(replay,corridor,size_t(nc)*sizeof(dtPolyRef));uint32 replay_count=uint32(nc);
    for(const auto& visited:oq.visits) replay_count=path.fixupCorridor(replay,replay_count,limit,visited.data(),uint32(visited.size()));
    if(status) require(std::memcmp(replay,path.m_smoothPathPolyRefs.data(),size_t(replay_count)*sizeof(dtPolyRef))==0,"source final corridor replay differs");
    std::fputc('\t',out);refs(out,replay,int(replay_count));std::fprintf(out,"\t%u\t%u\n",oq.height_failures,oq.move_buffer_calls);
}
static void fixups(FILE* out) {
    PathFinder p;
    auto row=[&](const char* label,std::vector<dtPolyRef> original,std::vector<dtPolyRef> visited,uint32 cap){
        auto path=original;path.resize(74);auto count=p.fixupCorridor(path.data(),uint32(original.size()),cap,visited.data(),uint32(visited.size()));
        std::fprintf(out,"%s\t%u\t",label,cap);refs(out,original.data(),int(original.size()));std::fputc('\t',out);
        refs(out,visited.data(),int(visited.size()));std::fputc('\t',out);refs(out,path.data(),int(count));std::fputc('\n',out);
    };
    row("no_intersection",{1,2,3},{7,8},74);
    row("reverse_visited",{1,2,3,4},{7,2,8,9},74);
    row("earliest_duplicate_visited",{1,2,3,4},{2,7,2,8},74);
    row("furthest_duplicate_path",{1,2,3,2,4},{2,8},74);
    row("truncate_tail",{1,2,3,4,5},{8,2,9,10},4);
    row("req_equals_limit",{1,2,3,4},{2,8,9},3);
    row("req_exceeds_limit",{1,2,3,4},{2,8,9,10},3);
    row("empty_visited",{1,2,3},{},74);
    row("single_common",{1},{1},2);
}
static void ranges(FILE* out) {
    PathFinder p;const std::array<float,3> zero={0,0,0};
    auto row=[&](const char* label,std::array<float,3> b,float r,float h){
        std::fprintf(out,"%s\t",label);point(out,zero.data());std::fputc('\t',out);point(out,b.data());
        std::fprintf(out,"\t%08x\t%08x\t%u\n",std::bit_cast<uint32>(r),std::bit_cast<uint32>(h),unsigned(p.inRangeYZX(zero.data(),b.data(),r,h)));
    };
    row("horizontal_below_slop",{std::nextafter(.3f,0.f),0,0},.3f,1);
    row("horizontal_equal_slop",{.3f,0,0},.3f,1);
    row("horizontal_above_slop",{std::nextafter(.3f,1.f),0,0},.3f,1);
    row("vertical_below_height",{0,std::nextafter(1.f,0.f),0},.3f,1);
    row("vertical_equal_height",{0,1,0},.3f,1);
    row("vertical_negative_equal",{0,-1,0},.3f,1);
    row("steer_height_below1000",{0,std::nextafter(1000.f,0.f),0},.3f,1000);
    row("steer_height_equal1000",{0,1000,0},.3f,1000);
    row("both_horizontal_axes",{.2f,0,.2f},.3f,1);
    row("diagonal_outside",{.25f,0,.25f},.3f,1);
    row("zero_radius",{0,0,0},0,1);
    row("signed_zero",{-0.f,-0.f,-0.f},.3f,1);
}
int main(int argc,char** argv) {
    require(std::fegetround()==FE_TONEAREST && (_mm_getcsr() & 0x8040)==0,"unsupported native floating environment");
    require(argc==3,"usage: oracle OUTPUT EXISTING_FIXTURES");const std::string dir=argv[1],existing=argv[2];
    make_geometry(dir);make_geometry(dir,true);
    Loaded authored(read(dir+"/params.test-fixture"));authored.add(read(dir+"/ground.test-fixture"));
    Loaded thin(read(dir+"/params.test-fixture"));thin.add(read(dir+"/thin.test-fixture"));
    Loaded cross(read(existing+"/authored.mmap"));cross.add(read(existing+"/0003232.mmtile"));cross.add(read(existing+"/0003231.mmtile"));
    FILE* out=std::fopen((dir+"/cases.test-fixture").c_str(),"w");require(out,"open cases");
    std::fprintf(out,"# Unchanged original smoothing on loaded authored Detour bytes; all point triples are WORLD XYZ hex binary32.\n");
    std::fprintf(out,"# label\tgeometry\tfrom\tto\tlimit\tfrom_status\tfrom_ref\tfrom_nearest\tfrom_over\tfrom_attempts\tto_status\tto_ref\tto_nearest\tto_over\tto_attempts\tcorridor_status\tcorridor\tbounded_from\tbounded_to\tprobe_steer_status\tprobe_steer_count\tsmooth_status\tstop\tpoints\tboundary_calls\tsteer_calls\tmove_calls\theight_calls\tsteer_buffer_calls\tboundary_status\tsteer_status\tmove_status\theight_status\tfull_steer_status\tfull_steer_count\tfinal_storage\tfinal_corridor\theight_failures\tmove_buffer_calls\n");
    auto local=[](float x,float z,float y=0){return std::array<float,3>{-GRID+x,y,-GRID+z};};
    observe(authored,out,"flat_interior","ground",local(102,104),local(128,125));
    observe(authored,out,"flat_vertical_offsets","ground",local(102,104,2),local(128,125,3));
    observe(authored,out,"flat_target_boundary_clamp","ground",local(102,104,2),local(135,125,3));
    observe(authored,out,"flat_target_lower_boundary_clamp","ground",local(128,104,2),local(97,125,3));
    observe(authored,out,"flat_start_boundary_clamp","ground",local(97,104,2),local(128,125,3));
    observe(authored,out,"far_lookup","ground",local(93,104,0),local(128,125,0));
    observe(authored,out,"vertical_only_early_stop","ground",local(110,110,2),local(110,110,4));
    observe(authored,out,"coincident_early_stop","ground",local(110,110,2),local(110,110,2));
    observe(authored,out,"slop_early_stop","ground",local(110,110,0),local(110.1f,110.1f,3));
    observe(authored,out,"flat_limit2","ground",local(102,104),local(128,125),2);
    observe(authored,out,"short_reaches_at_limit2","ground",local(110,110),local(111,111),2);
    observe(authored,out,"slope_multi_portal","ground",local(102,304,.125f),local(162,304,3.875f));
    observe(authored,out,"slope_vertical_offsets","ground",local(102,304,2.125f),local(162,304,6.875f));
    observe(authored,out,"slope_reverse","ground",local(162,304,3.875f),local(102,304,.125f));
    observe(authored,out,"bend_many_corners","ground",local(102,202,.125f),local(170,254,4.375f));
    observe(authored,out,"bend_reverse","ground",local(170,254,4.375f),local(102,202,.125f));
    observe(authored,out,"bend_corridor_limit2","ground",local(102,202,.125f),local(170,254,4.375f),2);
    observe(authored,out,"bend_limit16","ground",local(102,202,.125f),local(170,254,4.375f),16);
    observe(authored,out,"long_limit74","ground",local(102,404),local(498,404));
    observe(authored,out,"long_limit10","ground",local(102,404),local(498,404),10);
    observe(authored,out,"unreachable_partial_corridor","ground",local(110,110),local(110,304,.625f));
    observe(cross,out,"exact_fit_two_polygons_limit2","existing",{-400,0,-400},{-200,0,-400},2);
    observe(cross,out,"cross_tile_slope","existing",{-10,0,-400},{50,0.9375f,-400});
    observe(cross,out,"cross_tile_unsnapped_vertical","existing",{-10,2,-400},{50,3.9375f,-400});
    observe(cross,out,"cross_tile_reverse","existing",{50,0.9375f,-400},{-10,0,-400});
    observe(thin,out,"thin_visited_buffer","thin",local(125.0625f,204),local(129.9375f,204));
    observe(thin,out,"thin_visited_buffer_reverse","thin",local(129.9375f,204),local(125.0625f,204));
    std::fclose(out);
    out=std::fopen((dir+"/fixup.test-fixture").c_str(),"w");require(out,"open fixup");
    std::fprintf(out,"# label\tlimit\tpath\tvisited\tresult\n");fixups(out);std::fclose(out);
    out=std::fopen((dir+"/range.test-fixture").c_str(),"w");require(out,"open range");
    std::fprintf(out,"# label\tfrom\tto\tradius\theight\tresult\n");ranges(out);std::fclose(out);
}
