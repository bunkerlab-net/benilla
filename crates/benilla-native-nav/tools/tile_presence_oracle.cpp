// SPDX-License-Identifier: GPL-3.0-or-later
// Test-only complete unchanged original HaveTile and IsValidMapCoord methods.
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
#ifdef TILE_PRESENCE_REPAIR_TILE_LIFETIME
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

// The only owner/vector stand-ins are authored and deliberately nontransport.
// No owner exists: the unmodified HaveTile body short-circuits before GetTransport.
struct Unit { void* GetTransport() const { fail("unexpected owner callback"); } };
struct Vector3 { float x,y,z; };
#define VERTEX_SIZE 3
// ORIGINAL_COORDINATE_DEFINES
namespace MaNGOS {
// ORIGINAL_COORDINATE_METHODS
}
struct PathFinder {
    const Unit* m_sourceUnit = nullptr;
    const dtNavMesh* m_navMesh;
    bool HaveTile(const Vector3& p) const;
};
// ORIGINAL_HAVE_TILE

static uint32 bits(float v) { return std::bit_cast<uint32>(v); }
static float word(uint32 v) { return std::bit_cast<float>(v); }
static void point(FILE* out,Vector3 p) { std::fprintf(out,"%08x,%08x,%08x",bits(p.x),bits(p.y),bits(p.z)); }
static bool calc_safe(const Loaded& loaded,Vector3 p) {
    if(!std::isfinite(p.x)||!std::isfinite(p.y)||!std::isfinite(p.z))return false;
    const auto* params=loaded.mesh.getParams();
    const float a=std::floor((p.y-params->orig[0])/params->tileWidth);
    const float b=std::floor((p.x-params->orig[2])/params->tileHeight);
    return std::isfinite(a)&&std::isfinite(b)&&double(a)>=INT32_MIN&&double(a)<=INT32_MAX&&double(b)>=INT32_MIN&&double(b)<=INT32_MAX;
}
struct Endpoint { bool checked=false;int tx=0,ty=0;bool layer0=false,have=false; };
static Endpoint observe_endpoint(Loaded& loaded,Vector3 p) {
    require(calc_safe(loaded,p),"unsafe original float-to-int domain");
    Endpoint e;e.checked=true;float native[]={p.y,p.z,p.x};
    // Independent observations call actual original primitives. HaveTile itself is
    // also executed unchanged; this wrapper supplies no query or geometry answer.
    loaded.mesh.calcTileLoc(native,&e.tx,&e.ty);
    e.layer0=loaded.mesh.getTileAt(e.tx,e.ty,0)!=nullptr;
    e.have=PathFinder{nullptr,&loaded.mesh}.HaveTile(p);return e;
}
static void endpoint(FILE* out,Endpoint e) {
    if(!e.checked)std::fprintf(out,"0\t-\t-\t-\t-");
    else std::fprintf(out,"1\t%d\t%d\t%u\t%u",e.tx,e.ty,unsigned(e.layer0),unsigned(e.have));
}
static void observe(Loaded& loaded,FILE* out,const std::string& label,const char* geometry,Vector3 from,Vector3 to,const char* scope="runtime",bool nearest=false) {
    const bool dest_valid=MaNGOS::IsValidMapCoord(to.x,to.y,to.z);
    const bool start_valid=dest_valid&&MaNGOS::IsValidMapCoord(from.x,from.y,from.z);
    const bool pair_valid=dest_valid&&start_valid;
    const bool safe=calc_safe(loaded,from)&&calc_safe(loaded,to)&&std::fabs(from.z)<=100000&&std::fabs(to.z)<=100000;
    Endpoint a,b;
    if(pair_valid&&(safe||std::strcmp(scope,"raw")==0)) {
        a=observe_endpoint(loaded,from);
        if(a.have)b=observe_endpoint(loaded,to);
    }
    std::fprintf(out,"%s\t%s\t%s\t",label.c_str(),geometry,scope);point(out,from);std::fputc('\t',out);point(out,to);
    std::fprintf(out,"\t%u\t",unsigned(dest_valid));if(dest_valid)std::fprintf(out,"%u",unsigned(start_valid));else std::fputc('-',out);
    std::fprintf(out,"\t%u\t%u\t",unsigned(pair_valid),unsigned(safe));endpoint(out,a);std::fputc('\t',out);endpoint(out,b);
    std::fprintf(out,"\t%u\t",unsigned(a.have&&b.have));
    if(nearest) {
        float pos[]={from.y,from.z,from.x},ext[]={5,5,5},closest[3]={};dtPolyRef ref=0;dtQueryFilter filter;
        filter.setIncludeFlags(1);filter.setExcludeFlags(0);
        dtStatus status=loaded.query.findNearestPoly(pos,ext,&filter,&ref,closest);
        std::fprintf(out,"%08x\t%llu",status,static_cast<unsigned long long>(ref));
        require(dtStatusSucceed(status)&&ref&&!a.have,"nearby polygon is not tile presence");
    } else std::fprintf(out,"-\t-");
    std::fputc('\n',out);
}
static void environment_guard() {
    require(std::fegetround()==FE_TONEAREST,"unsupported floating environment: rounding");
    require((_mm_getcsr()&0xe040u)==0,"unsupported floating environment: FTZ/DAZ/rounding");
}
int main(int argc,char** argv) {
    require(argc==3||argc==4,"arguments");
    if(argc==4) {
        std::string mode=argv[3];
        if(mode=="round-down")require(std::fesetround(FE_DOWNWARD)==0,"set rounding");
        else if(mode=="ftz")_mm_setcsr(_mm_getcsr()|0x8000u);
        else if(mode=="daz")_mm_setcsr(_mm_getcsr()|0x0040u);
        else if(mode=="sse-round")_mm_setcsr((_mm_getcsr()&~0x6000u)|0x4000u);
        else fail("unknown environment mutation");
    }
    environment_guard();
    const std::string dir=argv[1],inputs=argv[2];
    auto params=read(inputs+"/authored.mmap"),left=read(inputs+"/0003232.mmtile"),right=read(inputs+"/0003231.mmtile");
    Loaded both(params),only_left(params),only_right(params),empty(params);
    both.add(left);both.add(right);only_left.add(left);only_right.add(right);
    const float grid=533.3333f;const Vector3 a={-400,-400,0},b={-400,100,0},outside={100,-400,0};
    FILE* out=std::fopen((dir+"/cases.test-fixture").c_str(),"wb");require(out,"open cases");
    std::fprintf(out,"# Exact original HaveTile and coordinate validity; authored loaded meshes; not calculate or owner/gameplay evidence\n");
    std::fprintf(out,"# label\tgeometry\tscope\tfrom\tto\tdestination_valid\tstart_valid\tpair_valid\tnative_safe\tstart_checked\tstart_tx\tstart_ty\tstart_layer0\tstart_have\tend_checked\tend_tx\tend_ty\tend_layer0\tend_have\tpair_have\tnearest_status\tnearest_ref\n");
    observe(both,out,"both_present","both",a,b);
    observe(only_left,out,"left_only_end_missing","left",a,b);
    observe(only_right,out,"right_only_start_missing","right",a,b);
    observe(empty,out,"empty_start_missing","empty",a,b,"raw");
    observe(both,out,"start_missing_end_present","both",outside,a);
    observe(both,out,"start_present_end_missing","both",a,outside);
    observe(both,out,"axes_distinct","both",{-400,100,7},a);
    observe(both,out,"axes_permuted","both",{100,-400,7},a);
    observe(only_left,out,"nearby_poly_tile_absent","left",{-400,1,0},a,"runtime",true);
    for(float z:{-100000.0f,100000.0f,100001.0f,std::numeric_limits<float>::max()}) {
        std::string name="height_"+std::to_string(bits(z));
        observe(both,out,name,"both",{-400,-400,z},{-400,100,-z},std::fabs(z)>100000?"raw":"runtime");
    }
    // Native boundary at -GRID and cancellation/rounding near world y=0.
    for(int axis=0;axis<2;++axis) {
        const float centers[]={-grid,0,grid};
        for(int i=0;i<3;++i)for(int d=-1;d<=1;++d) {
            float v=d<0?std::nextafter(centers[i],-INFINITY):d>0?std::nextafter(centers[i],INFINITY):centers[i];
            Vector3 p=a;if(axis==0)p.x=v;else p.y=v;
            observe(both,out,"tile_edge_"+std::to_string(axis)+"_"+std::to_string(i)+"_"+std::to_string(d+1),"both",p,a);
        }
    }
    for(uint32 w:{0xb8000001u,0xb8000000u,0xb7ffffffu,0x80000001u,0x80000000u,0u,1u}) {
        observe(both,out,"rounding_y_"+std::to_string(w),"both",{-400,word(w),0},a);
        observe(both,out,"rounding_x_"+std::to_string(w),"both",{word(w),-400,0},a);
    }
    const float edge=float(MAP_HALFSIZE-0.5);
    for(int axis=0;axis<2;++axis)for(int sign:{-1,1})for(int d=-1;d<=1;++d) {
        float v=sign*edge;v=d<0?std::nextafter(v,-INFINITY):d>0?std::nextafter(v,INFINITY):v;
        Vector3 p=a;if(axis==0)p.x=v;else p.y=v;
        const std::string name="map_edge_"+std::to_string(axis)+"_"+std::to_string(sign+1)+"_"+std::to_string(d+1);
        observe(both,out,name+"_start","both",p,a);observe(both,out,name+"_end","both",a,p);
    }
    for(int axis=0;axis<3;++axis)for(uint32 w:{0x7f800000u,0xff800000u,0x7fc00000u}) {
        Vector3 p=a;if(axis==0)p.x=word(w);else if(axis==1)p.y=word(w);else p.z=word(w);
        const std::string name="invalid_"+std::to_string(axis)+"_"+std::to_string(w);
        observe(both,out,name+"_start","both",p,a);observe(both,out,name+"_end","both",a,p);
    }
    observe(both,out,"invalid_destination_first","both",{NAN,-400,0},{-400,INFINITY,0});
    // Explicitly authored header/origin variants of source-writer-generated bytes.
    for(int index:{1,2}) {
        auto shifted=params,tile=left;dtNavMeshParams p;std::memcpy(&p,params.data(),sizeof(p));p.orig[0]=index*grid;
        std::memcpy(shifted.data(),&p,sizeof(p));dtMeshHeader h;std::memcpy(&h,tile.data()+20,sizeof(h));h.x=-index;
        std::memcpy(tile.data()+20,&h,sizeof(h));const std::string suffix=index==1?"minus-one":"minus-two";
        write(dir+"/params-"+suffix+".test-fixture",shifted.data(),shifted.size());write(dir+"/tile-"+suffix+".test-fixture",tile.data(),tile.size());
        Loaded loaded(shifted);loaded.add(tile);
        observe(loaded,out,index==1?"minus_one_present_rejected":"minus_two_present_allowed",index==1?"minus_one":"minus_two",{-400,1,0},{-400,1,0});
    }
    auto layer=left;dtMeshHeader h;std::memcpy(&h,layer.data()+20,sizeof(h));h.layer=1;std::memcpy(layer.data()+20,&h,sizeof(h));
    write(dir+"/tile-layer-one.test-fixture",layer.data(),layer.size());Loaded layer_one(params);layer_one.add(layer);
    require(layer_one.mesh.getTileAt(0,0,1)!=nullptr,"layer one actually loaded");
    observe(layer_one,out,"layer_one_only","layer_one",a,a,"raw");
    require(std::fclose(out)==0,"close cases");
}
