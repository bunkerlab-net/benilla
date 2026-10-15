// SPDX-License-Identifier: GPL-3.0-or-later
// Test-only full unchanged original Detour, fresh direct/sliced queries, authored geometry.
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
#ifdef DIRECT_CORRIDOR_REPAIR_TILE_LIFETIME
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
static void make_geometry(const std::string& dir, const char* name, int count, int tx=0, int tz=0, bool square=false) {
    using V=std::array<unsigned short,3>;
    std::vector<V> vertices;
    std::vector<std::array<unsigned short,4>> cells;
    for(int i=0;i<=count;++i) {
        vertices.push_back({static_cast<unsigned short>(i),0,0});
        vertices.push_back({static_cast<unsigned short>(i),0,static_cast<unsigned short>(square?1:100)});
    }
    for(int i=0;i<count;++i) cells.push_back({static_cast<unsigned short>(i*2),static_cast<unsigned short>(i*2+1),static_cast<unsigned short>(i*2+3),static_cast<unsigned short>(i*2+2)});
    std::vector<unsigned short> polys(count*12,0xffff), flags(count,1);
    std::vector<unsigned char> areas(count,11), tris(count*8);
    std::vector<unsigned int> details(count*4);
    std::vector<float> detail_vertices(count*12);
    const float cs=GRID/float(count);
    const float minx=float(tx-1)*GRID,minz=float(tz-1)*GRID;
    for(int i=0;i<count;++i) {
        for(int j=0;j<4;++j) {
            polys[i*12+j]=cells[i][j];
            for(int k=0;k<3;++k) detail_vertices[i*12+j*3+k]=(k==0?minx:k==2?minz:0)+vertices[cells[i][j]][k]*(k==1?1:cs);
        }
        if(i>0) polys[i*12+6]=static_cast<unsigned short>(i-1);
        if(i+1<count) polys[i*12+8]=static_cast<unsigned short>(i+1);
        if(square) { // Source builder sides 0:-x, 1:+z, 2:+x, 3:-z.
            polys[i*12+6]=0x8000; polys[i*12+7]=0x8001;
            polys[i*12+8]=0x8002; polys[i*12+9]=0x8003;
        }
        details[i*4]=i*4;details[i*4+1]=4;details[i*4+2]=i*2;details[i*4+3]=2;
        const unsigned char fan[8]={0,1,2,5,0,2,3,20};std::memcpy(&tris[i*8],fan,8);
    }
    dtNavMeshCreateParams cp={};cp.verts=vertices[0].data();cp.vertCount=int(vertices.size());
    cp.polys=polys.data();cp.polyFlags=flags.data();cp.polyAreas=areas.data();cp.polyCount=count;cp.nvp=6;
    cp.detailMeshes=details.data();cp.detailVerts=detail_vertices.data();cp.detailVertsCount=count*4;
    cp.detailTris=tris.data();cp.detailTriCount=count*2;cp.walkableHeight=2;cp.walkableRadius=.5f;cp.walkableClimb=.4f;
    cp.tileX=tx;cp.tileY=tz;cp.bmin[0]=minx;cp.bmin[2]=minz;cp.bmax[0]=float(tx)*GRID;cp.bmax[2]=float(tz)*GRID;cp.bmax[1]=20;
    cp.cs=cs;cp.ch=1;cp.buildBvTree=true;
    unsigned char* data=nullptr;int size=0;require(dtCreateNavMeshData(&cp,&data,&size),"create authored native mesh");
    uint32 header[]={0x4d4d4150,7,8,uint32(size),1};std::vector<unsigned char> file(20+size);
    std::memcpy(file.data(),header,20);std::memcpy(file.data()+20,data,size);dtFree(data);
    write(dir+"/"+name,file.data(),file.size());
}
static void point(FILE* out,const float* p) {
    std::fprintf(out,"%08x,%08x,%08x",std::bit_cast<uint32>(p[2]),std::bit_cast<uint32>(p[0]),std::bit_cast<uint32>(p[1]));
}
static void refs(FILE* out,const std::vector<dtPolyRef>& path) {
    if(path.empty()) std::fputc('-',out);
    for(size_t i=0;i<path.size();++i) std::fprintf(out,"%s%llu",i?",":"",static_cast<unsigned long long>(path[i]));
}
struct Near {dtStatus status=0;dtPolyRef ref=0;float point[3]={};bool over=false;};
static Near nearest(dtNavMeshQuery& q,const float* p,const dtQueryFilter& f) {
    Near out;const float ext[3]={5,5,5};out.status=q.findNearestPoly(p,ext,&f,&out.ref,out.point,&out.over);return out;
}
struct Result {dtStatus status=0;std::vector<dtPolyRef> path;int nodes=0,states=0,iterations=0;};
static Result query(Loaded& loaded,const Near& a,const Near& b,const float* from,const float* to,int capacity,int nodes,bool sliced) {
    dtNavMeshQuery q;require(q.init(&loaded.mesh,nodes)==DT_SUCCESS,"initialize observer query");
    dtQueryFilter filter;filter.setIncludeFlags(1);filter.setExcludeFlags(0);
    Result r;r.path.resize(size_t(capacity));int count=0;
    if(!sliced) r.status=q.findPath(a.ref,b.ref,from,to,&filter,r.path.data(),&count,capacity);
    else {
        r.status=q.initSlicedFindPath(a.ref,b.ref,from,to,&filter,0);
        while(dtStatusInProgress(r.status)) {
            int done=0;r.status=q.updateSlicedFindPath(1,&done);r.iterations+=done;
            require(done<=1 && r.iterations<100000,"bounded sliced query");
        }
        require(dtStatusSucceed(r.status),"sliced query failed");
        r.status=q.finalizeSlicedFindPath(r.path.data(),&count,capacity);
    }
    require(count>=0 && count<=capacity,"query count domain");r.path.resize(size_t(count));
    for(auto ref:r.path) require(loaded.mesh.isValidPolyRef(ref),"corridor contains unloaded reference");
    auto* pool=q.getNodePool();r.nodes=pool->getNodeCount();
    for(int i=1;i<=r.nodes;++i) r.states+=pool->getNodeAtIdx(static_cast<unsigned int>(i))->state!=0;
    return r;
}
static void observe(Loaded& loaded,FILE* out,const char* label,const char* geometry,std::array<float,3> a,std::array<float,3> b,int capacity=74,int nodes=1024) {
    dtQueryFilter filter;filter.setIncludeFlags(1);filter.setExcludeFlags(0);
    auto from=nearest(loaded.query,a.data(),filter),to=nearest(loaded.query,b.data(),filter);
    require(from.status==DT_SUCCESS && to.status==DT_SUCCESS && from.ref && to.ref,"loaded endpoint lookup");
    auto direct=query(loaded,from,to,a.data(),b.data(),capacity,nodes,false);
    auto sliced=query(loaded,from,to,a.data(),b.data(),capacity,nodes,true);
    std::fprintf(out,"%s\t%s\t",label,geometry);point(out,a.data());std::fputc('\t',out);point(out,b.data());
    std::fprintf(out,"\t%d\t%d\t%08x\t%llu\t",capacity,nodes,from.status,static_cast<unsigned long long>(from.ref));point(out,from.point);
    std::fprintf(out,"\t%u\t%08x\t%llu\t",unsigned(from.over),to.status,static_cast<unsigned long long>(to.ref));point(out,to.point);
    std::fprintf(out,"\t%u\t%08x\t",unsigned(to.over),direct.status);refs(out,direct.path);
    std::fprintf(out,"\t%d\t%d\t%08x\t",direct.nodes,direct.states,sliced.status);refs(out,sliced.path);
    std::fprintf(out,"\t%d\t%d\n",sliced.nodes,sliced.iterations);
    std::fprintf(stderr,"%s direct=%08x/%zu sliced=%08x/%zu nodes=%d/%d states=%d\n",label,direct.status,direct.path.size(),sliced.status,sliced.path.size(),direct.nodes,sliced.nodes,direct.states);
}
int main(int argc,char** argv) {
    require(argc==3,"usage: oracle OUTPUT EXISTING_FIXTURES");
    require(std::fegetround()==FE_TONEAREST && (_mm_getcsr()&0x8040)==0,"unsupported floating environment");
    const std::string dir=argv[1],existing=argv[2];
    dtNavMeshParams params={{-GRID,0,-GRID},GRID,GRID,4,INT32_MIN};write(dir+"/params.test-fixture",&params,sizeof(params));
    make_geometry(dir,"long600.test-fixture",600);make_geometry(dir,"long1100.test-fixture",1100);
    make_geometry(dir,"grid00.test-fixture",1,0,0,true);make_geometry(dir,"grid10.test-fixture",1,1,0,true);
    make_geometry(dir,"grid01.test-fixture",1,0,1,true);make_geometry(dir,"grid11.test-fixture",1,1,1,true);
    FILE* out=std::fopen((dir+"/cases.test-fixture").c_str(),"w");require(out,"open cases");
    std::fprintf(out,"# Original Detour direct and zero-option sliced; authored loaded ground meshes; world XYZ binary32 words\n");
    std::fprintf(out,"# label\tgeometry\tfrom\tto\tcapacity\tnodes\tfrom_status\tfrom_ref\tfrom_nearest\tfrom_over\tto_status\tto_ref\tto_nearest\tto_over\tdirect_status\tdirect_corridor\tdirect_nodes\tdirect_state_nodes\tsliced_status\tsliced_corridor\tsliced_nodes\tsliced_iterations\n");
    Loaded basic(read(existing+"/authored.mmap"));basic.add(read(existing+"/0003232.mmtile"));basic.add(read(existing+"/0003231.mmtile"));
    observe(basic,out,"same_polygon","existing",{-400,2,-400},{-300,3,-300});
    observe(basic,out,"two_polygons","existing",{-400,0,-400},{-100,0,-400});
    observe(basic,out,"two_exact_capacity","existing",{-400,0,-400},{-100,0,-400},2);
    observe(basic,out,"two_truncated","existing",{-400,0,-400},{-100,0,-400},1);
    observe(basic,out,"cross_tile_slope","existing",{-100,0,-400},{100,2,-400});
    observe(basic,out,"cross_tile_reverse","existing",{400,7,-400},{-400,0,-400});
    observe(basic,out,"disconnected_partial","existing",{-400,0,-400},{-400,0,-100});
    Loaded grid(read(dir+"/params.test-fixture"));
    for(const char* name:{"grid00.test-fixture","grid10.test-fixture","grid01.test-fixture","grid11.test-fixture"}) grid.add(read(dir+"/"+name));
    const std::array<float,3> a={-GRID+GRID*.40f,0,-GRID+GRID*.90f}, b={-GRID+GRID*1.95f,0,-GRID+GRID*1.05f};
    observe(grid,out,"cross_side_route_diverges","grid",a,b);
    observe(grid,out,"cross_side_exact_capacity","grid",a,b,3);
    observe(grid,out,"cross_side_truncated","grid",a,b,2);
    observe(grid,out,"cross_side_reverse","grid",b,a);
    for(int count:{600,1100}) {
        const auto name="long"+std::to_string(count);Loaded loaded(read(dir+"/params.test-fixture"));loaded.add(read(dir+"/"+name+".test-fixture"));
        const float cs=GRID/float(count);const std::array<float,3> start={-GRID+cs*.5f,0,-GRID+cs*50};
        const std::array<float,3> end={-GRID+cs*(float(count)-.5f),0,-GRID+cs*50};
        observe(loaded,out,(name+"_1024").c_str(),name.c_str(),start,end);
        observe(loaded,out,(name+"_512").c_str(),name.c_str(),start,end,74,512);
        observe(loaded,out,(name+"_full").c_str(),name.c_str(),start,end,count);
        if(count==1100) {
            observe(loaded,out,"long1100_outnodes_capacity1024",name.c_str(),start,end,1024);
            const std::array<float,3> exact_pool={-GRID+cs*1023.5f,0,start[2]};
            observe(loaded,out,"long1100_exact_pool1024",name.c_str(),start,exact_pool);
            observe(loaded,out,"long1100_exact_pool1024_full",name.c_str(),start,exact_pool,1024);
            const std::array<float,3> exact_capacity={-GRID+cs*73.5f,0,start[2]};
            const std::array<float,3> over_capacity={-GRID+cs*74.5f,0,start[2]};
            observe(loaded,out,"seventyfour_exact_capacity",name.c_str(),start,exact_capacity);
            observe(loaded,out,"seventyfive_truncated",name.c_str(),start,over_capacity);
        }
    }
    require(std::fclose(out)==0,"close cases");
}
