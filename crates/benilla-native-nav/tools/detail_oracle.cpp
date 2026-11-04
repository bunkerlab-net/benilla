// SPDX-License-Identifier: GPL-3.0-or-later
// Authored diagonal boundary sample, one float outside the coarse polygon.
#include "DetourNavMeshBuilder.h"
#include "DetourNavMesh.h"
#include "DetourNavMeshQuery.h"
#include "DetourAlloc.h"
#include <cassert>
#include <cmath>
#include <cstdio>
#include <cstdint>
#include <cstring>
#include <string>
#include <vector>
static void write(const std::string& path,const void* data,size_t size) {
    FILE* f=fopen(path.c_str(),"wb");assert(f);
    assert(fwrite(data,size,1,f)==1);assert(fclose(f)==0);
}
static unsigned bits(float v) {unsigned b;memcpy(&b,&v,4);return b;}
int main(int argc,char** argv) {
    assert(argc==2);const std::string out=argv[1];const float grid=533.3333f;
    dtNavMeshParams params={{-grid,0,-grid},grid,grid,4,INT32_MIN};
    write(out+"/params.mmap",&params,sizeof params);
    const unsigned short verts[]={10,0,10, 10,0,70, 70,0,80, 70,0,10};
    const unsigned short polys[]={0,1,2,3,0xffff,0xffff, 0xffff,0xffff,0xffff,0xffff,0xffff,0xffff};
    const unsigned short flags[]={1};const unsigned char areas[]={11};
    float detail[15];
    const float cs=grid/100;
    for(int i=0;i<4;++i)for(int j=0;j<3;++j)
        detail[i*3+j]=(j==1?0:-grid)+verts[i*3+j]*(j==1?1:cs);
    detail[12]=(detail[3]+detail[6])*0.5f;
    detail[13]=2;
    detail[14]=std::nextafter((detail[5]+detail[8])*0.5f,INFINITY);
    const double cross=(double(detail[6])-detail[3])*(double(detail[14])-detail[5])-
                       (double(detail[8])-detail[5])*(double(detail[12])-detail[3]);
    assert(cross>0);
    const unsigned int meshes[]={0,5,0,3};
    const unsigned char tris[]={0,1,4,5, 0,4,3,16, 4,2,3,5};
    dtNavMeshCreateParams cp={};cp.verts=verts;cp.vertCount=4;cp.polys=polys;
    cp.polyFlags=flags;cp.polyAreas=areas;cp.polyCount=1;cp.nvp=6;
    cp.detailMeshes=meshes;cp.detailVerts=detail;cp.detailVertsCount=5;
    cp.detailTris=tris;cp.detailTriCount=3;
    cp.walkableHeight=2;cp.walkableRadius=.5;cp.walkableClimb=.4;
    cp.bmin[0]=cp.bmin[2]=-grid;cp.bmax[1]=4;cp.cs=cs;cp.ch=1;cp.buildBvTree=true;
    unsigned char* raw=nullptr;int size=0;assert(dtCreateNavMeshData(&cp,&raw,&size));
    const uint32_t header[]={0x4d4d4150,7,8,uint32_t(size),1};
    std::vector<unsigned char> file(20+size);memcpy(file.data(),header,20);memcpy(file.data()+20,raw,size);
    write(out+"/near-edge.mmtile",file.data(),file.size());
    // The bridge's alignment adjustment, without changing any serialized bytes.
    std::vector<unsigned char> storage(size+8);
    auto* data=storage.data()+((100+4*12+32)%8?4:0);
    memcpy(data,raw,size);dtFree(raw);
    dtNavMesh mesh;assert(mesh.init(&params)==DT_SUCCESS);
    assert(mesh.addTile(data,size,0,0,nullptr)==DT_SUCCESS);
    dtNavMeshQuery query;assert(query.init(&mesh,512)==DT_SUCCESS);
    dtQueryFilter filter;filter.setIncludeFlags(1);filter.setExcludeFlags(0);
    const float inside[]={-grid+40*cs,0,-grid+40*cs};
    const float outside[]={detail[12],detail[13],detail[14]};
    FILE* f=fopen((out+"/queries.tsv").c_str(),"w");assert(f);
    fprintf(f,"# label world_bits status reference nearest_world_bits over\n");
    for(int i=0;i<2;++i) {
        const float* p=i?outside:inside;const float ext[]={5,5,5};
        dtPolyRef ref=0;float nearest[3]{};bool over=false;
        const dtStatus status=query.findNearestPoly(p,ext,&filter,&ref,nearest,&over);
        assert(status==DT_SUCCESS&&ref);
        fprintf(f,"%s %08x %08x %08x %u %llu %08x %08x %08x %u\n",
                i?"outside":"inside",bits(p[2]),bits(p[0]),bits(p[1]),status,
                static_cast<unsigned long long>(ref),bits(nearest[2]),bits(nearest[0]),bits(nearest[1]),over?1:0);
    }
    assert(fclose(f)==0);
}
