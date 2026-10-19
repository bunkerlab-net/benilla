// SPDX-License-Identifier: GPL-3.0-or-later
// Authored predicate inputs only. Include the complete untouched original
// translation unit so even its inline/static slab helpers run original code.
// These stack edge structs are not serialized MMTiles or navigation evidence.
#include <array>
#include <cfenv>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <fstream>
#include <limits>
#include <memory>
#include <stdexcept>
#include "json.hpp"
#include "DetourNavMesh.cpp"
using json=nlohmann::json;
using Point=std::array<float,3>;
static json float_bits(const float* values,size_t count) {
    json result=json::array();
    for(size_t i=0;i<count;++i) {uint32_t word;std::memcpy(&word,&values[i],4);char text[9];std::snprintf(text,9,"%08x",word);result.push_back(text);}
    return result;
}
int main(int argc,char** argv) {
    if(argc!=2 || std::fesetround(FE_TONEAREST)!=0)return 2;
    std::unique_ptr<dtNavMesh,decltype(&dtFreeNavMesh)> mesh(dtAllocNavMesh(),&dtFreeNavMesh);
    if(!mesh)throw std::bad_alloc();
    dtNavMeshParams params{};params.maxTiles=1;params.maxPolys=1;params.tileWidth=1;params.tileHeight=1;
    if(dtStatusFailed(mesh->init(&params))) {(void)mesh.release();throw std::runtime_error("predicate mesh init failed");}
    json results={{"schema",1},{"scope","Authored edge predicate structs only; no fabricated MMTile or path"},{"cases",json::array()}};
    auto run=[&](const char* name,Point a,Point b,Point c,Point d,float target_climb=1.f,int side=4,bool supported=true) {
        dtMeshHeader header{};header.polyCount=1;header.vertCount=2;header.walkableClimb=target_climb;
        dtPoly poly{};poly.vertCount=2;poly.verts[0]=0;poly.verts[1]=1;poly.neis[0]=DT_EXT_LINK|side;
        float vertices[6];std::memcpy(vertices,c.data(),12);std::memcpy(vertices+3,d.data(),12);
        dtMeshTile* tile=&mesh->m_tiles[0];tile->header=&header;tile->polys=&poly;tile->verts=vertices;
        float amin[2],amax[2],bmin[2],bmax[2];
        calcSlabEndPoints(a.data(),b.data(),amin,amax,side);calcSlabEndPoints(c.data(),d.data(),bmin,bmax,side);
        const bool overlap=overlapSlabs(amin,amax,bmin,bmax,.01f,target_climb);
        dtPolyRef con[4]={};float areas[8]={};
        const int count=mesh->findConnectingPolys(a.data(),b.data(),tile,side,con,areas,4);
        json refs=json::array();for(int i=0;i<count;++i)refs.push_back(con[i]);
        results["cases"].push_back({{"name",name},{"source_a_bits",float_bits(a.data(),3)},{"source_b_bits",float_bits(b.data(),3)},
            {"target_a_bits",float_bits(c.data(),3)},{"target_b_bits",float_bits(d.data(),3)},
            {"target_climb_bits",float_bits(&target_climb,1)[0]},{"side",side},{"bounded_finite_scope",supported},
            {"source_min_bits",float_bits(amin,2)},{"source_max_bits",float_bits(amax,2)},
            {"target_min_bits",float_bits(bmin,2)},{"target_max_bits",float_bits(bmax,2)},
            {"overlap_slabs",overlap},{"connection_count",count},{"connection_refs",refs},{"connection_area_bits",float_bits(areas,2*count)}});
        tile->header=nullptr;tile->polys=nullptr;tile->verts=nullptr;
    };
    const float below=std::nextafter(.01f,0.f),above=std::nextafter(.01f,1.f);
    for(const auto& item:std::array<std::pair<const char*,float>,6>{{{"normal_positive_below",below},{"normal_positive_equal",.01f},{"normal_positive_above",above},{"normal_negative_below",-below},{"normal_negative_equal",-.01f},{"normal_negative_above",-above}}})
        run(item.first,{item.second,0,0},{item.second,0,1},{0,0,0},{0,0,1});
    run("horizontal_touching",{0,0,0},{0,0,1},{0,0,1},{0,0,2});
    for(const auto& item:std::array<std::pair<const char*,float>,3>{{{"horizontal_shrink_below",std::nextafter(.98f,0.f)},{"horizontal_shrink_equal",.98f},{"horizontal_shrink_above",std::nextafter(.98f,1.f)}}})
        run(item.first,{0,0,0},{0,0,1},{0,0,item.second},{0,0,2});
    for(const auto& item:std::array<std::pair<const char*,float>,6>{{{"vertical_positive_below",std::nextafter(2.f,0.f)},{"vertical_positive_equal",2.f},{"vertical_positive_above",std::nextafter(2.f,3.f)},{"vertical_negative_below",-std::nextafter(2.f,0.f)},{"vertical_negative_equal",-2.f},{"vertical_negative_above",-std::nextafter(2.f,3.f)}}})
        run(item.first,{0,0,0},{0,0,1},{0,item.second,0},{0,item.second,1});
    run("target_climb_one_rejects",{0,0,0},{0,0,1},{0,3,0},{0,3,1},1);
    run("target_climb_two_accepts_reverse",{0,3,0},{0,3,1},{0,0,0},{0,0,1},2);
    run("crossing_slopes",{0,-4,0},{0,4,1},{0,4,0},{0,-4,1},0);
    run("reversed_source",{0,4,1},{0,-4,0},{0,4,0},{0,-4,1},0);
    run("reversed_target",{0,-4,0},{0,4,1},{0,-4,1},{0,4,0},0);
    run("reversed_both",{0,4,1},{0,-4,0},{0,-4,1},{0,4,0},0);
    run("side_zero",{0,0,0},{0,0,1},{0,0,0},{0,0,1},1,0);
    run("side_two",{0,0,0},{1,0,0},{0,0,0},{1,0,0},1,2);
    run("side_six_reversed",{1,0,0},{0,0,0},{1,0,0},{0,0,0},1,6);
    const float huge=std::numeric_limits<float>::max();
    run("unsupported_vertical_overflow",{0,-huge,0},{0,huge,1},{0,0,0},{0,0,1},1,4,false);
    run("unsupported_zero_width",{0,0,0},{0,1,0},{0,0,-1},{0,0,1},1,4,false);
    run("unsupported_horizontal_overflow",{0,0,-huge},{0,1,huge},{0,0,-huge},{0,0,huge},1,4,false);
    std::ofstream(argv[1])<<results.dump(2)<<"\n";
}
