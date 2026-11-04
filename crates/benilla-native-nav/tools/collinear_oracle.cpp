// SPDX-License-Identifier: GPL-3.0-or-later
// Authored redundant corner and fully collinear triangle, not install geometry.
#include "DetourNavMeshBuilder.h"
#include "DetourNavMesh.h"
#include "DetourNavMeshQuery.h"
#include "DetourAlloc.h"
#include <cassert>
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
    for(bool flat : {false,true}) {
        const std::string name=flat?"flat":"corner";
        const unsigned short corner[]={10,0,10, 10,0,40, 10,0,70, 70,0,70, 70,0,10};
        const unsigned short line[]={10,2,10, 14,1,10, 15,0,10};
        const auto* verts=flat?line:corner;const int nv=flat?3:5;
        unsigned short polys[12];for(auto& v:polys)v=0xffff;
        for(int i=0;i<nv;++i)polys[i]=i;
        const unsigned short flags[]={1};const unsigned char areas[]={11};
        float detail[15];const float cs=grid/100;
        for(int i=0;i<nv;++i)for(int j=0;j<3;++j)
            detail[i*3+j]=(j==1?0:-grid)+verts[i*3+j]*(j==1?1:cs);
        const unsigned char cornerTris[]={0,1,4,17, 1,2,3,5, 1,3,4,4};
        const unsigned char lineTris[]={2,0,1,21};
        const int nt=flat?1:3;const unsigned int meshes[]={0,unsigned(nv),0,unsigned(nt)};
        dtNavMeshCreateParams cp={};cp.verts=verts;cp.vertCount=nv;cp.polys=polys;
        cp.polyFlags=flags;cp.polyAreas=areas;cp.polyCount=1;cp.nvp=6;
        cp.detailMeshes=meshes;cp.detailVerts=detail;cp.detailVertsCount=nv;
        cp.detailTris=flat?lineTris:cornerTris;cp.detailTriCount=nt;
        cp.walkableHeight=2;cp.walkableRadius=.5;cp.walkableClimb=.4;
        cp.bmin[0]=cp.bmin[2]=-grid;cp.bmax[1]=4;cp.cs=cs;cp.ch=1;cp.buildBvTree=true;
        unsigned char* raw=nullptr;int size=0;assert(dtCreateNavMeshData(&cp,&raw,&size));
        const uint32_t header[]={0x4d4d4150,7,8,uint32_t(size),1};
        std::vector<unsigned char> file(20+size);memcpy(file.data(),header,20);memcpy(file.data()+20,raw,size);
        write(out+"/"+name+".mmtile",file.data(),file.size());
        std::vector<unsigned char> storage(size+8);
        auto* data=storage.data()+((100+nv*12+32)%8?4:0);
        memcpy(data,raw,size);dtFree(raw);
        dtNavMesh mesh;assert(mesh.init(&params)==DT_SUCCESS);
        assert(mesh.addTile(data,size,0,0,nullptr)==DT_SUCCESS);
        dtNavMeshQuery query;assert(query.init(&mesh,512)==DT_SUCCESS);
        dtQueryFilter filter;filter.setIncludeFlags(1);filter.setExcludeFlags(0);
        FILE* f=fopen((out+"/"+name+".tsv").c_str(),"w");assert(f);
        fprintf(f,"# world_bits status reference nearest_world_bits over\n");
        float points[2][3];
        if(flat) {memcpy(points[0],detail,12);memcpy(points[1],detail+6,12);}
        else {const float p[2][3]={{-grid+20*cs,0,-grid+20*cs},{-grid+60*cs,0,-grid+60*cs}};memcpy(points,p,sizeof p);}
        dtPolyRef refs[2]{};
        for(int i=0;i<2;++i) {
            const float ext[]={5,5,5};float nearest[3]{};bool over=false;
            auto status=query.findNearestPoly(points[i],ext,&filter,&refs[i],nearest,&over);
            assert(status==DT_SUCCESS&&refs[i]);
            fprintf(f,"%08x %08x %08x %u %llu %08x %08x %08x %u\n",
                    bits(points[i][2]),bits(points[i][0]),bits(points[i][1]),status,
                    static_cast<unsigned long long>(refs[i]),bits(nearest[2]),bits(nearest[0]),bits(nearest[1]),over?1:0);
        }
        dtPolyRef path[16];int count=0;
        assert(query.findPath(refs[0],refs[1],points[0],points[1],&filter,path,&count,16)==DT_SUCCESS&&count==1);
        float straight[48];int corners=0;
        assert(query.findStraightPath(points[0],points[1],path,count,straight,nullptr,nullptr,&corners,16)==DT_SUCCESS);
        fprintf(f,"path %d %d\n",count,corners);
        for(int i=0;i<corners;++i)fprintf(f,"%08x %08x %08x\n",bits(straight[i*3+2]),bits(straight[i*3]),bits(straight[i*3+1]));
        float t=0,normal[3];int visited=0;
        assert(query.raycast(refs[0],points[0],points[1],&filter,&t,normal,path,&visited,16)==DT_SUCCESS);
        fprintf(f,"raycast %08x %d\n",bits(t),visited);
        assert(fclose(f)==0);
    }
}
