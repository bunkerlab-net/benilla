// SPDX-License-Identifier: GPL-3.0-or-later
// Authored driver. All generation and query algorithms are linked original source.
#include <cfenv>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <limits>
#include <fstream>
#include <sstream>
#include <string>
#include <vector>
#include "MapBuilder.h"
#include "DetourNavMeshQuery.h"
#include "TileAssembler.h"
#include "MapTree.h"
#include "DetourAlloc.h"

static int merged_polygons=-1;
extern "C" bool __real__Z17rcMergePolyMeshesP9rcContextPP10rcPolyMeshiRS1_(rcContext*,rcPolyMesh**,int,rcPolyMesh&);
extern "C" bool __wrap__Z17rcMergePolyMeshesP9rcContextPP10rcPolyMeshiRS1_(rcContext* c,rcPolyMesh** p,int n,rcPolyMesh& out) {
    bool ok=__real__Z17rcMergePolyMeshesP9rcContextPP10rcPolyMeshiRS1_(c,p,n,out);
    merged_polygons=out.npolys; return ok;
}
static bool trace_files=false;
static std::vector<json> file_events;
extern "C" FILE* __real_fopen(const char*,const char*);
extern "C" FILE* __wrap_fopen(const char* path,const char* mode) {
    FILE* file=__real_fopen(path,mode);
    if(trace_files) {
        const char* base=std::strrchr(path,'/'); base=base?base+1:path;
        file_events.push_back({{"basename",base},{"mode",mode},{"opened",file!=nullptr}});
    }
    return file;
}
static uint32_t bits(float value) { uint32_t word; std::memcpy(&word, &value, 4); return word; }
static json float_bits(const float* values, size_t count) {
    json out=json::array();
    for(size_t i=0; i<count; ++i) { char buf[9]; std::snprintf(buf,9,"%08x",bits(values[i])); out.push_back(buf); }
    return out;
}
static void write_u32(FILE* out, uint32_t value) { if(fwrite(&value,4,1,out)!=1) std::abort(); }
static void write_array(FILE* out, const void* data, size_t item_size, size_t count) {
    if(count && fwrite(data,item_size,count,out)!=count) std::abort();
}
static void dump_mesh(const std::string& path, const MMAP::MeshData& data) {
    FILE* arrays=std::fopen(path.c_str(),"wb");
    for(bool liquid:{false,true}) {
        const auto& vertices=liquid?data.liquidVerts:data.solidVerts;
        const auto& triangles=liquid?data.liquidTris:data.solidTris;
        write_u32(arrays,vertices.size()/3); write_u32(arrays,triangles.size()/3);
        write_array(arrays,vertices.getCArray(),4,vertices.size()); write_array(arrays,triangles.getCArray(),4,triangles.size());
        if(liquid) write_array(arrays,data.liquidType.getCArray(),1,data.liquidType.size());
    }
    fclose(arrays);
}
static std::vector<unsigned char> read_file(const std::string& path) {
    std::ifstream input(path,std::ios::binary); return {std::istreambuf_iterator<char>(input),{}};
}
static json queries(const std::string& base, uint32_t map, uint32_t x, uint32_t y) {
    char path[1024]; std::snprintf(path,sizeof(path),"%s/mmaps/%03u.mmap",base.c_str(),map);
    auto mmap=read_file(path); if(mmap.size()!=sizeof(dtNavMeshParams)) throw std::runtime_error("missing mmap");
    dtNavMeshParams params; memcpy(&params,mmap.data(),sizeof(params));
    json result; result["params_origin_bits"]=float_bits(params.orig,3);
    result["max_tiles"]=params.maxTiles; result["max_polys"]=params.maxPolys;
    std::snprintf(path,sizeof(path),"%s/mmaps/%03u%02u%02u.mmtile",base.c_str(),map,y,x);
    auto bytes=read_file(path); result["emitted"]=!bytes.empty(); if(bytes.empty()) return result;
    MmapTileHeader framing; memcpy(&framing,bytes.data(),sizeof(framing));
    if(framing.size+sizeof(framing)!=bytes.size()) throw std::runtime_error("bad original framing");
    dtNavMesh* mesh=dtAllocNavMesh(); if(dtStatusFailed(mesh->init(&params))) throw std::runtime_error("init failed");
    dtTileRef tile_ref=0;
    auto status=mesh->addTile(bytes.data()+sizeof(framing),framing.size,0,0,&tile_ref);
    if(dtStatusFailed(status)) throw std::runtime_error("addTile failed");
    const dtMeshTile* tile=mesh->getTileByRef(tile_ref); const dtMeshHeader* header=tile->header;
    result["poly_count"]=header->polyCount; result["vert_count"]=header->vertCount;
    result["detail_vert_count"]=header->detailVertCount; result["detail_tri_count"]=header->detailTriCount;
    result["bv_node_count"]=header->bvNodeCount; result["tile_x"]=header->x; result["tile_y"]=header->y;
    result["tile_bmin_bits"]=float_bits(header->bmin,3); result["tile_bmax_bits"]=float_bits(header->bmax,3);
    json areas=json::object(); for(int i=0;i<header->polyCount;++i) {auto key=std::to_string(tile->polys[i].getArea()); areas[key]=areas.value(key,0)+1;}
    result["areas"]=areas;
    dtNavMeshQuery* query=dtAllocNavMeshQuery(); if(dtStatusFailed(query->init(mesh,32768))) throw std::runtime_error("query init failed");
    dtQueryFilter filter; filter.setIncludeFlags(NAV_GROUND); filter.setExcludeFlags(0);
    const float extents[3]={5.f,10000.f,5.f};
    result["queries"]=json::array(); std::vector<dtPolyRef> refs; std::vector<std::array<float,3>> points;
    for(int row=1;row<=5;++row) for(int col=1;col<=5;++col) {
        float request[3]={header->bmin[0]+(header->bmax[0]-header->bmin[0])*col/6.f,
                          header->bmax[1]+1.f,
                          header->bmin[2]+(header->bmax[2]-header->bmin[2])*row/6.f};
        float nearest[3]={}; dtPolyRef ref=0; auto nearest_status=query->findNearestPoly(request,extents,&filter,&ref,nearest);
        json item={{"point_bits",float_bits(request,3)},{"extents_bits",float_bits(extents,3)},
                   {"status",nearest_status},{"poly_ref",ref},{"nearest_bits",float_bits(nearest,3)}};
        if(ref) { float height=0; auto hs=query->getPolyHeight(ref,nearest,&height); item["height_status"]=hs; item["height_bits"]=float_bits(&height,1)[0];
            refs.push_back(ref); points.push_back({nearest[0],nearest[1],nearest[2]}); }
        result["queries"].push_back(item);
    }
    result["paths"]=json::array();
    if(refs.size()>1) for(size_t end=1;end<refs.size();++end) {
        dtPolyRef corridor[65536]; int count=0; auto ps=query->findPath(refs[0],refs[end],points[0].data(),points[end].data(),&filter,corridor,&count,65536);
        json item={{"start_ref",refs[0]},{"end_ref",refs[end]},{"start_bits",float_bits(points[0].data(),3)},
                   {"end_bits",float_bits(points[end].data(),3)},{"status",ps},{"corridor",json::array()}};
        for(int i=0;i<count;++i) item["corridor"].push_back(corridor[i]);
        float straight[4096*3]; unsigned char flags[4096]; dtPolyRef straight_refs[4096]; int straight_count=0;
        auto ss=query->findStraightPath(points[0].data(),points[end].data(),corridor,count,straight,flags,straight_refs,&straight_count,4096);
        item["straight_status"]=ss; item["straight_bits"]=float_bits(straight,straight_count*3);
        result["paths"].push_back(item);
    }
    dtFreeNavMeshQuery(query); dtFreeNavMesh(mesh); return result;
}
static int scalar_nearest_queries(const char* base,const char* source_points,const char* destination) {
    std::ifstream original_points(source_points);json fixture=json::parse(original_points);
    auto mmap=read_file(std::string(base)+"/mmaps/000.mmap");
    auto bytes=read_file(std::string(base)+"/mmaps/0003130.mmtile");
    if(mmap.size()!=sizeof(dtNavMeshParams) || bytes.size()<sizeof(MmapTileHeader)) throw std::runtime_error("missing original scalar-query input");
    dtNavMeshParams params;std::memcpy(&params,mmap.data(),sizeof(params));
    MmapTileHeader framing;std::memcpy(&framing,bytes.data(),sizeof(framing));
    if(framing.size+sizeof(framing)!=bytes.size())throw std::runtime_error("bad original scalar-query framing");
    dtNavMesh* mesh=dtAllocNavMesh();if(dtStatusFailed(mesh->init(&params)))throw std::runtime_error("scalar query mesh init failed");
    dtTileRef tile_ref=0;auto add_status=mesh->addTile(bytes.data()+sizeof(framing),framing.size,0,0,&tile_ref);
    if(dtStatusFailed(add_status))throw std::runtime_error("scalar query tile load failed");
    dtNavMeshQuery* query=dtAllocNavMeshQuery();if(dtStatusFailed(query->init(mesh,32768)))throw std::runtime_error("scalar query init failed");
    dtQueryFilter filter;filter.setIncludeFlags(NAV_GROUND);filter.setExcludeFlags(0);
    const float extents[3]={5.f,5.f,5.f};
    json result={{"schema",1},{"include_flags",NAV_GROUND},{"exclude_flags",0},{"extents_bits",float_bits(extents,3)},{"queries",json::array()}};
    unsigned index=0;
    for(const auto& original:fixture["output"]["queries"]) {
        float point[3];for(unsigned axis=0;axis<3;++axis){uint32_t word=std::stoul(original["nearest_bits"][axis].get<std::string>(),nullptr,16);std::memcpy(&point[axis],&word,4);}
        float nearest[3]={};dtPolyRef ref=0;bool over=false;
        auto status=query->findNearestPoly(point,extents,&filter,&ref,nearest,&over);
        json item={{"source_query_index",index++},{"source_poly_ref",original["poly_ref"]},{"point_bits",float_bits(point,3)},
                   {"status",status},{"poly_ref",ref},{"nearest_bits",float_bits(nearest,3)},{"is_over_poly",over}};
        if(ref){float input_height=0,nearest_height=0;
            auto input_status=query->getPolyHeight(ref,point,&input_height);auto nearest_status=query->getPolyHeight(ref,nearest,&nearest_height);
            item["input_height_status"]=input_status;item["input_height_bits"]=float_bits(&input_height,1)[0];
            item["nearest_height_status"]=nearest_status;item["nearest_height_bits"]=float_bits(&nearest_height,1)[0];
        }
        result["queries"].push_back(item);
    }
    dtFreeNavMeshQuery(query);dtFreeNavMesh(mesh);std::ofstream(destination)<<result.dump(2)<<"\n";return 0;
}
void from_json(const json&,rcConfig&);
static int config_queries(const char* base,const char* config_path) {
    MMAP::MapBuilder builder(config_path,1,false,false,false,false,false,nullptr,base);
    json results=json::array();
    for(const auto& key:std::vector<std::array<uint32_t,3>>{{0,30,31},{0,36,53},{429,30,32},{545,0,0},{562,0,0},{572,28,29},{1,1,23},{1,12,3}}) {
        json values=builder.getTileConfig(key[0],key[1],key[2]); rcConfig config{};from_json(values,config);
        results.push_back({{"map_id",key[0]},{"tile_x",key[1]},{"tile_y",key[2]},{"merged_json",values},
                           {"walkable_radius",config.walkableRadius},{"walkable_climb",config.walkableClimb},
                           {"min_region_area_squared",config.minRegionArea},{"merge_region_area_squared",config.mergeRegionArea}});
    }
    std::ofstream(std::string(base)+"/config-result.json")<<results.dump(2)<<"\n";return 0;
}
int main(int argc,char** argv) {
    static_assert(sizeof(float)==4 && sizeof(dtNavMeshParams)==28 && sizeof(MmapTileHeader)==20);
    if(std::fesetround(FE_TONEAREST)!=0) return 1;
    if(argc==5 && std::string(argv[1])=="scalar_queries") return scalar_nearest_queries(argv[2],argv[3],argv[4]);
    if(argc==4 && std::string(argv[1])=="configs") return config_queries(argv[2],argv[3]);
    if(argc==4 && std::string(argv[1])=="assemble") { VMAP::TileAssembler assembler(argv[2],argv[3]); return assembler.convertWorld2()?0:2; }
    if(argc!=7) { std::fprintf(stderr,"usage: oracle DIR MAP X Y CONFIG OFFMESH\n"); return 2; }
    const std::string base=argv[1]; uint32_t map=std::stoul(argv[2]),x=std::stoul(argv[3]),y=std::stoul(argv[4]);
    MMAP::MapBuilder builder(argv[5],1,false,false,false,false,false,argv[6],base.c_str());
    MMAP::MeshData data; trace_files=true; builder.m_terrainBuilder->loadMap(map,x,y,data); trace_files=false;
    dump_mesh(base+"/source-terrain.bin",data);
    json result; result["terrain_file_events"]=file_events; result["terrain_vertices_before_clean"]=data.solidVerts.size()/3;
    result["terrain_triangles"]=data.solidTris.size()/3;
    result["vmap_loaded"]=builder.m_terrainBuilder->loadVMap(map,y,x,data);
    dump_mesh(base+"/source-combined.bin",data);
    result["combined_vertices_before_clean"]=data.solidVerts.size()/3;
    result["combined_triangles"]=data.solidTris.size()/3;
    MMAP::TerrainBuilder::cleanVertices(data.solidVerts,data.solidTris);
    MMAP::TerrainBuilder::cleanVertices(data.liquidVerts,data.liquidTris);
    G3D::Array<float> all; all.append(data.liquidVerts); all.append(data.solidVerts);
    float bmin[3],bmax[3]; builder.getTileBounds(x,y,all.getCArray(),all.size()/3,bmin,bmax);
    const json config=builder.getTileConfig(map,x,y); result["config"]=config;
    result["bounds_min_bits"]=float_bits(bmin,3); result["bounds_max_bits"]=float_bits(bmax,3);
    result["solid_vertices"]=data.solidVerts.size()/3; result["solid_triangles"]=data.solidTris.size()/3;
    result["liquid_vertices"]=data.liquidVerts.size()/3; result["liquid_triangles"]=data.liquidTris.size()/3;
    result["tile_union"]=json::array(); for(auto tile:builder.getTileList(map)) { uint32_t tx,ty; VMAP::StaticMapTree::unpackTileID(tile,tx,ty); result["tile_union"].push_back({tx,ty}); }
    if(!std::getenv("BENILLA_ORACLE_COLLECT_ONLY")) {
        builder.buildSingleTile(map,x,y);
        result["output"]=queries(base,map,x,y);
        result["output"]["merged_polygons"]=merged_polygons;
    }
    dump_mesh(base+"/source-mesh.bin",data);
    std::ofstream(base+"/source-result.json")<<result.dump(2)<<"\n";
}
