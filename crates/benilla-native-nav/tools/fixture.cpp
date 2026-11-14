// SPDX-License-Identifier: GPL-3.0-or-later
// Authored numeric geometry only. Uses pinned native writer and direct query primitives.
#include "DetourNavMeshBuilder.h"
#include "DetourNavMesh.h"
#include "DetourNavMeshQuery.h"
#include "DetourAlloc.h"
#include <cstdio>
#include <cstring>
#include <cstdlib>
#include <cstdint>
#include <string>
#include <vector>
#include <cassert>
static void write(const std::string& path,const void* data,size_t n) {
    FILE* f=fopen(path.c_str(),"wb");assert(f);assert(fwrite(data,n,1,f)==1);fclose(f);
}
static uint32_t bits(float f){uint32_t b;memcpy(&b,&f,4);return b;}
static void longCorridor(const std::string& dir,int count){
    const float grid=533.3333f;std::vector<unsigned short> verts((count+1)*6);
    for(int i=0;i<=count;++i){unsigned short row[6]={static_cast<unsigned short>(i),0,0,static_cast<unsigned short>(i),0,100};memcpy(&verts[i*6],row,12);}
    std::vector<unsigned short> polys(count*12,0xffff),flags(count,1);std::vector<unsigned char> areas(count,11);
    std::vector<unsigned int> details(count*4);std::vector<float> dverts(count*12);std::vector<unsigned char> tris(count*8);
    for(int i=0;i<count;++i){
        unsigned short ids[4]={static_cast<unsigned short>(i*2),static_cast<unsigned short>(i*2+1),static_cast<unsigned short>(i*2+3),static_cast<unsigned short>(i*2+2)};
        for(int j=0;j<4;++j)polys[i*12+j]=ids[j];
        if(i>0)polys[i*12+6]=i-1;if(i+1<count)polys[i*12+8]=i+1;
        details[i*4]=i*4;details[i*4+1]=4;details[i*4+2]=i*2;details[i*4+3]=2;
        for(int j=0;j<4;++j)for(int k=0;k<3;++k)dverts[i*12+j*3+k]=(k==1?0:-grid)+verts[ids[j]*3+k]*(k==1?1:grid/count);
        const unsigned char fan[8]={0,1,2,5,0,2,3,20};memcpy(&tris[i*8],fan,8);
    }
    dtNavMeshCreateParams cp={};cp.verts=verts.data();cp.vertCount=(count+1)*2;cp.polys=polys.data();cp.polyFlags=flags.data();cp.polyAreas=areas.data();cp.polyCount=count;cp.nvp=6;
    cp.detailMeshes=details.data();cp.detailVerts=dverts.data();cp.detailVertsCount=count*4;cp.detailTris=tris.data();cp.detailTriCount=count*2;
    cp.walkableHeight=2;cp.walkableRadius=.5;cp.walkableClimb=.4;cp.bmin[0]=cp.bmin[2]=-grid;cp.bmax[1]=2;cp.cs=grid/count;cp.ch=1;cp.buildBvTree=true;
    unsigned char* data=nullptr;int size=0;assert(dtCreateNavMeshData(&cp,&data,&size));
    uint32_t header[]={0x4d4d4150,7,8,uint32_t(size),1};std::vector<unsigned char> file(20+size);
    memcpy(file.data(),header,20);memcpy(file.data()+20,data,size);dtFree(data);
    write(dir+"/long"+std::to_string(count)+".mmtile",file.data(),file.size());
}
int main(int argc,char** argv){
    assert(argc==2);const std::string dir=argv[1];const float grid=533.3333f;
    dtNavMeshParams params={{-grid,0,-grid},grid,grid,4,INT32_MIN};
    write(dir+"/authored.mmap",&params,sizeof params);
    longCorridor(dir,300);longCorridor(dir,600);
    dtNavMesh mesh;assert(mesh.init(&params)==DT_SUCCESS);
    std::vector<void*> owners;
    // Tile 32,32 contains two adjacent quads, a detached island, and a stacked floor.
    // Tile 32,31 shares its x portal. Both are native serialized source products.
    for(int ti=0;ti<2;++ti){
        const unsigned short verts[]={0,0,0, 0,0,60, 50,0,60, 50,0,0, 100,0,60, 100,0,0,
                                      10,0,200, 10,0,220, 30,0,220, 30,0,200,
                                      0,10,0, 0,10,60, 50,10,60, 50,10,0, 25,0,0};
        unsigned short polys[4*12];for(auto &v:polys)v=0xffff;
        const unsigned short indices[4][4]={{0,1,2,3},{3,2,4,5},{6,7,8,9},{10,11,12,13}};
        for(int p=0;p<4;++p){for(int i=0;i<4;++i)polys[p*12+i]=indices[p][i];}
        polys[0*12+6+2]=1;polys[1*12+6+0]=0;
        // Builder side 2 converts to Detour +x; side 0 to Detour -x.
        polys[1*12+6+2]=0x8002;
        if(ti==1)polys[0*12+6+0]=0x8000;
        unsigned short flags[4]={1,1,1,1};unsigned char areas[4]={11,11,11,11};
        dtNavMeshCreateParams cp={};cp.verts=verts;cp.vertCount=14;cp.polys=polys;cp.polyFlags=flags;cp.polyAreas=areas;
        cp.polyCount=4;cp.nvp=6;cp.walkableHeight=2;cp.walkableRadius=.5f;cp.walkableClimb=.4f;
        cp.tileX=ti;cp.tileY=0;cp.bmin[0]=(ti-1)*grid;cp.bmin[1]=0;cp.bmin[2]=-grid;
        cp.bmax[0]=ti*grid;cp.bmax[1]=20;cp.bmax[2]=0;cp.cs=grid/100;cp.ch=1;cp.buildBvTree=true;
        // Only first strip reaches a tile edge; island stays interior by different z data.
        // Use cs=GRID/100; island indices must remain within the tile, so shrink their z.
        unsigned short local[sizeof(verts)/sizeof(*verts)];memcpy(local,verts,sizeof verts);
        for(int i=6;i<10;++i){local[i*3+2]-=130;}
        if(ti==1){for(int i: {2,3})local[i*3+1]=5;for(int i:{4,5})local[i*3+1]=10;}
        cp.verts=local;
        unsigned int detailMeshes[16];float detailVerts[16*3];unsigned char detailTris[8*4];
        for(int p=0;p<4;++p){
            detailMeshes[p*4]=p*4;detailMeshes[p*4+1]=4;detailMeshes[p*4+2]=p*2;detailMeshes[p*4+3]=2;
            for(int j=0;j<4;++j){int v=indices[p][j];for(int k=0;k<3;++k)detailVerts[(p*4+j)*3+k]=cp.bmin[k]+local[v*3+k]*(k==1?cp.ch:cp.cs);}
            const unsigned char ts[8]={0,1,2,5,0,2,3,20};memcpy(detailTris+p*8,ts,8);
        }
        cp.detailMeshes=detailMeshes;cp.detailVerts=detailVerts;cp.detailVertsCount=16;cp.detailTris=detailTris;cp.detailTriCount=8;
        unsigned char* data=nullptr;int size=0;assert(dtCreateNavMeshData(&cp,&data,&size));
        const uint32_t header[]={0x4d4d4150,7,8,uint32_t(size),1};
        std::vector<unsigned char> file(20+size);memcpy(file.data(),header,20);memcpy(file.data()+20,data,size);
        write(dir+(ti==0?"/0003232.mmtile":"/0003231.mmtile"),file.data(),file.size());
        if(ti==0){
            cp.vertCount=15;unsigned char* odd=nullptr;int oddSize=0;assert(dtCreateNavMeshData(&cp,&odd,&oddSize));
            uint32_t oddHeader[]={0x4d4d4150,7,8,uint32_t(oddSize),1};std::vector<unsigned char> oddFile(20+oddSize);
            memcpy(oddFile.data(),oddHeader,20);memcpy(oddFile.data()+20,odd,oddSize);
            write(dir+"/0003232-odd.mmtile",oddFile.data(),oddFile.size());dtFree(odd);
        }
        // Exactly as bridge: correct source serialized dtLink alignment without changing bytes.
        const dtMeshHeader* h=reinterpret_cast<dtMeshHeader*>(data);
        const size_t offset=100+size_t(h->vertCount)*12+size_t(h->polyCount)*32;
        void* owner=malloc(size+8);assert(owner);auto* aligned=static_cast<unsigned char*>(owner)+(offset%8?4:0);
        memcpy(aligned,data,size);dtFree(data);assert(mesh.addTile(aligned,size,0,0,nullptr)==DT_SUCCESS);owners.push_back(owner);
    }
    dtNavMeshQuery query;assert(query.init(&mesh,512)==DT_SUCCESS);dtQueryFilter filter;filter.setIncludeFlags(1);filter.setExcludeFlags(0);
    FILE* out=fopen((dir+"/oracle.tsv").c_str(),"w");assert(out);
    // Include inside, outside, stacked-height, near-boundary, cross-tile and unreachable cases.
    for(int z=5;z<100;z+=15)for(int x=5;x<190;x+=15)for(int y: {0,4,10}){
        float a[3]={-grid+x*(grid/100),float(y),-grid+z*(grid/100)};
        float b[3]={-grid+180*(grid/100),0,-grid+50*(grid/100)};
        const float ext[3]={5,5,5};dtPolyRef ar=0,br=0;float an[3]={},bn[3]={};bool over=false;
        auto as=query.findNearestPoly(a,ext,&filter,&ar,an,&over);auto bs=query.findNearestPoly(b,ext,&filter,&br,bn);
        fprintf(out,"%08x %08x %08x %08x %08x %08x %08x %llu %08x %08x %08x %u",bits(a[2]),bits(a[0]),bits(a[1]),bits(b[2]),bits(b[0]),bits(b[1]),as,(unsigned long long)ar,bits(an[2]),bits(an[0]),bits(an[1]),over?1:0);
        dtPolyRef path[256]={};int n=0;dtStatus status=0;
        if(as==DT_SUCCESS&&bs==DT_SUCCESS&&ar&&br){status=query.findPath(ar,br,an,bn,&filter,path,&n,256);}
        fprintf(out," %08x %d",status,n);for(int i=0;i<n;++i)fprintf(out," %llu",(unsigned long long)path[i]);
        float pts[256*3]={};unsigned char flags[256]={};dtPolyRef refs[256]={};int np=0;dtStatus straight=0;
        if(status==DT_SUCCESS&&n>0){straight=query.findStraightPath(an,bn,path,n,pts,flags,refs,&np,256);}
        fprintf(out," %08x %d",straight,np);for(int i=0;i<np;++i)fprintf(out," %08x %08x %08x %u %llu",bits(pts[i*3+2]),bits(pts[i*3]),bits(pts[i*3+1]),flags[i],(unsigned long long)refs[i]);
        fputc('\n',out);
    }
    fclose(out);
    // mesh must be destroyed before owners; local mesh destructor sees flags=0 and only headers.
    // Owners intentionally live until process exit in this generation-only oracle.
}
