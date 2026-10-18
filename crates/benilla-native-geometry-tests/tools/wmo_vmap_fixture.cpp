// SPDX-License-Identifier: GPL-3.0-or-later
// Authored numeric WMO bytes; exact-source extraction, assembly and queries.
// Only MPQ acquisition and manager ownership are substituted. No install assets.
#include <algorithm>
#include <cassert>
#include <cfenv>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <iomanip>
#include <iterator>
#include <limits>
#include <map>
#include <memory>
#include <set>
#include <sstream>
#include <stdexcept>
#include <string>
#include <unordered_map>
#include <vector>
#include "wmo.h"
#define protected public
#include "WorldModel.h"
#undef protected
#include "ModelInstance.h"
#include "TileAssembler.h"
#include "VMapDefinitions.h"
#define private public
#include "MapTree.h"
#undef private
using G3D::Vector3;
using G3D::AABox;
namespace fs = std::filesystem;

static void require(bool condition, const char* message) {
    if (!condition) throw std::runtime_error(message);
}

// Exact original declarations, authored acquisition only. The map contains no
// filesystem fallbacks: a missing member behaves as an unavailable archive member.
static std::map<std::string, fs::path> authoredFiles;
static std::vector<std::string> openedFiles;
MPQFile::MPQFile(const char* filename) : eof(true), buffer(nullptr), pointer(0), size(0) {
    openedFiles.emplace_back(filename);
    const auto found = authoredFiles.find(filename);
    if (found == authoredFiles.end()) return;
    std::ifstream input(found->second, std::ios::binary);
    std::vector<char> bytes{std::istreambuf_iterator<char>(input), std::istreambuf_iterator<char>()};
    size = bytes.size();
    buffer = new char[size];
    std::copy(bytes.begin(), bytes.end(), buffer);
    eof = !size;
}
size_t MPQFile::read(void* dest, size_t count) {
    require(pointer >= 0 && pointer + count <= size, "authored MPQ read bounds");
    if (count) std::memcpy(dest, buffer + pointer, count);
    pointer += count;
    eof = pointer >= size;
    return count;
}
void MPQFile::seek(int offset) {
    require(offset >= 0 && offset <= size, "authored MPQ seek bounds");
    pointer = offset; eof = pointer >= size;
}
void MPQFile::seekRelative(int offset) { seek(int(pointer) + offset); }
void MPQFile::close() { delete[] buffer; buffer = nullptr; pointer = size = 0; eof = true; }

// This fixture is deliberately no-doodad. Do not silently fake MODN extraction.
bool ExtractSingleModel(std::string&, std::string&, StringSet&) {
    throw std::runtime_error("MODN extraction is outside this no-doodad oracle");
}

// PINNED_SOURCE_METHODS

static uint32 bits(float value) {
    uint32 result; std::memcpy(&result, &value, sizeof(result)); return result;
}
static void hex(std::ostream& out, float value) {
    out << '\t' << std::hex << std::setw(8) << std::setfill('0') << bits(value) << std::dec;
}
static void emitVector(std::ostream& out, const Vector3& v) {
    for (float value : {v.x, v.y, v.z}) hex(out, value);
}
static void write(FILE* file, const void* data, size_t bytes) {
    require(std::fwrite(data, 1, bytes, file) == bytes, "write authored bytes");
}
template<typename T> static void write(FILE* file, const T& value) { write(file, &value, sizeof(value)); }
using Bytes = std::vector<uint8>;
template<typename T> static void append(Bytes& bytes, const T& value) {
    const auto data = reinterpret_cast<const uint8*>(&value);
    bytes.insert(bytes.end(), data, data + sizeof(value));
}
static void authoredChunk(Bytes& bytes, const char* name, const Bytes& payload) {
    for (int i=3; i>=0; --i) bytes.push_back(name[i]);
    append(bytes, uint32(payload.size()));
    bytes.insert(bytes.end(), payload.begin(), payload.end());
}
static void save(const fs::path& path, const Bytes& bytes) {
    FILE* file = std::fopen(path.c_str(), "wb"); require(file, "create authored input");
    write(file, bytes.data(), bytes.size()); require(!std::fclose(file), "close authored input");
}
static Bytes version() { Bytes result; append(result, uint32(17)); return result; }
static Bytes rootBytes() {
    Bytes bytes, header;
    authoredChunk(bytes, "MVER", version());
    for (uint32 value : {0u,6u,0u,0u,0u,0u,0u,0x12345678u,4242u}) append(header,value);
    for (float value : {7.f,11.f,1.f,120.f,120.f,20.f}) append(header,value);
    append(header,uint32(4));
    require(header.size()==64,"MOHD64"); authoredChunk(bytes,"MOHD",header);
    const char names[]="floor\0upper\0empty\0antiportal\0";
    authoredChunk(bytes,"MOGN",Bytes(names,names+sizeof(names)-1));
    return bytes;
}
static Bytes groupBytes(unsigned index) {
    std::vector<Vector3> vertices;
    std::vector<uint16> indices, batches;
    Bytes materials;
    const auto triangle=[&](uint16 a,uint16 b,uint16 c,uint8 mask) {
        indices.insert(indices.end(),{a,b,c}); materials.push_back(mask); materials.push_back(0x7f);
    };
    Vector3 low, high;
    if (index==0) {
        // Unreferenced first/last vertices and descending first use distinguish
        // original ascending-index compaction from first-use deduplication.
        vertices.emplace_back(7,11,-0.f);
        for (unsigned mask=0;mask<256;++mask) {
            const float x=8+4*(mask%16), y=12+4*(mask/16);
            uint16 base=vertices.size();
            vertices.emplace_back(x,y,4); vertices.emplace_back(x+2,y,4); vertices.emplace_back(x,y+2,4);
            triangle(base+2,base+1,base,uint8(mask));
        }
        vertices.emplace_back(999,999,999);
        batches={12,345,65535}; low={7,11,3}; high={73,77,18};
    } else if (index==1) {
        for (unsigned y=0;y<=4;++y) for (unsigned x=0;x<=4;++x)
            vertices.emplace_back(12+x*2,16+y*2,10);
        for (uint16 y=0;y<4;++y) for (uint16 x=0;x<4;++x) {
            const uint16 a=y*5+x,b=a+1,c=a+5,d=c+1;
            triangle(d,b,a,0x0c); triangle(c,d,a,0x20);
        }
        batches={96,17}; low={11,15,9}; high={21,25,18};
    } else {
        const float xy=90+index*4;
        vertices={{xy,xy,2},{xy+2,xy,2},{xy,xy+2,2}};
        triangle(2,1,0,index==2?0x24:0x08);
        batches={3}; low={xy-1,xy-1,1}; high={xy+3,xy+3,8};
    }
    Bytes header;
    append(header,uint32(index==1?6:index==2?12:index==5?18:0));
    append(header,uint32(0));
    append(header,uint32(index==3?0x80:index==4?0x4000000:index==1?0x2000:0x8));
    for (float v:{low.x,low.y,low.z,high.x,high.y,high.z}) append(header,v);
    for (int i=0;i<4;++i) append(header,uint16(0));
    for (uint32 v:{0u,0u,15u,701u+index,0u,0u}) append(header,v);
    require(header.size()==68,"MOGP68");
    Bytes body=header, data;
    authoredChunk(body,"MOPY",materials);
    for (auto v:indices) append(data,v); authoredChunk(body,"MOVI",data); data.clear();
    for (auto v:vertices) { append(data,v.x); append(data,v.y); append(data,v.z); }
    authoredChunk(body,"MOVT",data); data.clear();
    for (auto value:batches) for (unsigned word=0;word<12;++word) append(data,uint16(word==8?value:word+1));
    authoredChunk(body,"MOBA",data);
    Bytes result; authoredChunk(result,"MVER",version()); authoredChunk(result,"MOGP",body); return result;
}
static VMAP::ModelSpawn spawn(uint32 id, Vector3 pos, float scale, Vector3 rotation) {
    VMAP::ModelSpawn result;
    result.flags=VMAP::MOD_HAS_BOUND; result.adtId=id==201?13:29; result.ID=id;
    result.name="Authored.wmo"; result.iPos=pos; result.iRot=rotation; result.iScale=scale;
    // WMO spawn bounds come from authored placements, not calculated by the
    // assembler. Transform all corners with the original ModelPosition helper.
    VMAP::ModelPosition transform; transform.iDir=rotation; transform.iScale=scale; transform.init();
    bool first=true;
    for (float x:{7.f,109.f}) for (float y:{11.f,109.f}) for (float z:{1.f,18.f}) {
        const auto p=transform.transform({x,y,z})+pos;
        if (first) { result.iBound=AABox(p,p); first=false; } else result.iBound.merge(p);
    }
    return result;
}
struct ProbeModel : VMAP::WorldModel {
    using VMAP::WorldModel::groupModels;
    using VMAP::WorldModel::groupTree;
};
static void load(const fs::path& root, VMAP::StaticMapTree& tree, std::vector<std::unique_ptr<ProbeModel>>& models) {
    FILE* file=std::fopen((root/"000.vmtree").c_str(),"rb"); require(file,"open original tree");
    char tag[8]; uint8 tiled;
    require(VMAP::readChunk(file,tag,VMAP::VMAP_MAGIC,8),"tree magic");
    require(std::fread(&tiled,1,1,file)==1 && tiled==1,"tiled tree");
    require(VMAP::readChunk(file,tag,"NODE",4),"tree NODE");
    require(tree.iTree.readFromFile(file),"original BIH reader");
    require(VMAP::readChunk(file,tag,"GOBJ",4),"tree GOBJ");
    require(std::fgetc(file)==EOF && !std::fclose(file),"tree end");
    file=std::fopen((root/"000_32_32.vmtile").c_str(),"rb"); require(file,"open original tile");
    require(VMAP::readChunk(file,tag,VMAP::VMAP_MAGIC,8),"tile magic");
    uint32 count; require(std::fread(&count,4,1,file)==1 && count==2,"tile count");
    tree.iNTreeValues=count; tree.iTreeValues=new VMAP::ModelInstance[count]; tree.iIsTiled=true;
    for (uint32 i=0;i<count;++i) {
        VMAP::ModelSpawn placement;
        require(VMAP::ModelSpawn::readFromFile(file,placement),"original spawn reader");
        uint32 index; require(std::fread(&index,4,1,file)==1 && index==i,"source model index");
        auto model=std::make_unique<ProbeModel>();
        require(model->readFile((root/(placement.name+".vmo")).string()),"original model reader");
        require(model->groupModels.size()==3,"three original retained groups");
        model->setModelFlags(placement.flags);
        tree.iTreeValues[index]=VMAP::ModelInstance(placement,model.get()); models.push_back(std::move(model));
    }
    require(std::fgetc(file)==EOF && !std::fclose(file),"tile end");
}
int main(int argc,char** argv) {
    try {
        require(argc==2,"usage: oracle output-directory");
        require(!std::fesetround(FE_TONEAREST),"round to nearest");
        static_assert(sizeof(float)==4 && std::numeric_limits<float>::is_iec559);
        static_assert(sizeof(Vec3D)==12 && sizeof(Vector3)==12 && sizeof(AABox)==24);
        const uint32 endian=1; require(*reinterpret_cast<const uint8*>(&endian)==1,"little endian");
        const fs::path root(argv[1]); const auto raw=root/".assembly", products=root/"products";
        fs::create_directory(raw); fs::create_directory(products);
        save(root/"authored-wmo.root.test-fixture",rootBytes());
        authoredFiles["Authored.wmo"]=root/"authored-wmo.root.test-fixture";
        for (unsigned i=0;i<6;++i) {
            char name[80], member[80];
            std::snprintf(name,sizeof(name),"authored-wmo_%03u.group.test-fixture",i);
            std::snprintf(member,sizeof(member),"Authored_%03u.wmo",i);
            save(root/name,groupBytes(i)); authoredFiles[member]=root/name;
        }
        require(raw.string().size()<sizeof(szWorkDirWmo),"temporary path bound");
        std::strcpy(szWorkDirWmo,raw.c_str());
        std::string filename="Authored.wmo";
        require(ExtractSingleWmo(filename),"original complete WMO extraction");
        require(openedFiles.size()==7,"all six root-declared groups opened");
        fs::copy_file(raw/"Authored.wmo",root/"authored-wmo.raw.vmap");
        // Safe original-orchestration negative: missing even a skipped group
        // deletes the partial product. No truncated source reaches unsafe reads.
        fs::remove(raw/"Authored.wmo");
        const auto missing=authoredFiles.at("Authored_004.wmo"); authoredFiles.erase("Authored_004.wmo");
        require(ExtractSingleWmo(filename),"original missing group return contract");
        require(!fs::exists(raw/"Authored.wmo"),"missing skipped group deletes output");
        authoredFiles["Authored_004.wmo"]=missing;
        require(ExtractSingleWmo(filename),"restore complete original extraction");
        FILE* directory=std::fopen((raw/"dir_bin").c_str(),"wb"); require(directory,"authored placements");
        for (const auto& placement:{spawn(201,{0,0,0},1,{0,0,0}),spawn(202,{180,20,4},1.75f,{13,37,-9})}) {
            write(directory,uint32(0)); write(directory,uint32(32)); write(directory,uint32(32));
            require(VMAP::ModelSpawn::writeToFile(directory,placement),"original spawn writer");
        }
        require(!std::fclose(directory),"close placements");
        VMAP::TileAssembler assembler(raw.string(),products.string());
        require(assembler.convertWorld2(),"original complete raw assembly"); fs::remove_all(raw);
        VMAP::StaticMapTree tree(0,products.string()); std::vector<std::unique_ptr<ProbeModel>> models;
        load(products,tree,models);
        std::ofstream rays(root/"rays.tsv"),heights(root/"heights.tsv"),locations(root/"locations.tsv");
        rays << "# CMaNGOS 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f; binary32 hex\n"
             << "# scope\tcase\tox\toy\toz\tdx\tdy\tdz\tmaximum\tignore_m2\thit\tdistance\n";
        heights << "# Original StaticMapTree::getHeight; infinity is the original miss\n# case\tpx\tpy\tpz\tmaximum\theight\n";
        locations << "# Original StaticMapTree GetLocationInfo and getAreaInfo; binary32 hex\n"
                  << "# case\twx\twy\twz\tix\tiy\tiz\thit\tfloor\tspawn\troot\tgroup\tflags\tadt\n";
        auto ray=[&](const std::string& scope,const std::string& name,const Vector3& p,const Vector3& d,float limit) {
            for (bool ignore:{false,true}) {
                float distance=limit;
                bool hit=scope=="map" ? tree.getIntersectionTime(G3D::Ray(p,d),distance,false,ignore)
                       : models[scope=="authored"?0:1]->IntersectRay(G3D::Ray(p,d),distance,false,ignore);
                rays << scope << '\t' << name; emitVector(rays,p); emitVector(rays,d); hex(rays,limit);
                rays << '\t' << unsigned(ignore) << '\t' << unsigned(hit); hex(rays,distance); rays << '\n';
            }
        };
        VMAP::VMapManager2 manager;
        auto queries=[&](const std::string& name,const Vector3& desired,float limit) {
            const auto world=manager.convertPositionToInternalRep(desired.x,desired.y,desired.z);
            const auto p=manager.convertPositionToInternalRep(world.x,world.y,world.z);
            heights << name; emitVector(heights,p); hex(heights,limit); hex(heights,tree.getHeight(p,limit)); heights << '\n';
            VMAP::LocationInfo location;
            const bool hit=tree.GetLocationInfo(p,location);
            Vector3 areaPoint=p; uint32 flags=0; int32 adt=0,rootId=0,group=0;
            const bool areaHit=tree.getAreaInfo(areaPoint,flags,adt,rootId,group);
            require(hit==areaHit,"source location/area presence");
            if(hit) require(bits(location.ground_Z)==bits(areaPoint.z) && location.rootId==rootId,"source location/area metadata");
            locations << name; emitVector(locations,world); emitVector(locations,p);
            locations << '\t' << unsigned(hit); hex(locations,location.ground_Z);
            locations << '\t' << (hit?location.hitInstance->ID:0) << '\t' << (hit?rootId:0)
                      << '\t' << (hit?group:0) << '\t' << (hit?flags:0) << '\t' << (hit?adt:0) << '\n';
        };
        for (unsigned i=0;i<2;++i) {
            const auto& placement=tree.iTreeValues[i]; const std::string scope=i==0?"authored":"rotated";
            VMAP::ModelPosition transform; transform.iDir=placement.iRot; transform.iScale=placement.iScale; transform.init();
            auto place=[&](const Vector3& p){ return transform.transform(p)+placement.iPos; };
            auto both=[&](const std::string& name,const Vector3& p,const Vector3& d,float limit) {
                ray(scope,name,p,d,limit);
                const auto world=place(p), delta=place(p+d*20)-world;
                ray("map",scope+"_"+name,world,delta/delta.magnitude(),limit*placement.iScale);
            };
            for (unsigned mask=0;mask<256;++mask) {
                const Vector3 p(8.5f+4*(mask%16),12.5f+4*(mask/16),14);
                const auto name="mask_"+std::to_string(mask);
                both(name,p,{0,0,-1},20); queries(scope+"_"+name,place(p),30);
            }
            for (const Vector3 p:{Vector3(13,17,14),Vector3(20,24,14),Vector3(20.001f,24.001f,14),
                                  Vector3(8,12,14),Vector3(8,12,4),Vector3(8.5f,12.5f,2),
                                  Vector3(98.5f,98.5f,4),Vector3(102.5f,102.5f,4),
                                  Vector3(106.5f,106.5f,4),Vector3(110.5f,110.5f,4),
                                  Vector3(6,10,14),Vector3(74,78,14),Vector3(-0.f,-0.f,-0.f)}) {
                const auto name="point_"+std::to_string(bits(p.x))+"_"+std::to_string(bits(p.y))+"_"+std::to_string(bits(p.z));
                both(name,p,{-0.f,-0.f,-1},30); queries(scope+"_"+name,place(p),30);
            }
            for(float limit:{0.f,3.9999998f,4.f,4.0000005f,10.f,20.f})
                both("endpoint_"+std::to_string(bits(limit)),{13,17,14},{0,0,-1},limit);
            both("up",{13,17,2},{0,0,1},20);
            queries(scope+"_up",place({13,17,2}),-30);
        }
        for(auto* out:{&rays,&heights,&locations}) { out->flush(); require(bool(*out),"write expectations"); }
        std::printf("Original WMO root/group extraction, assembly, reload and queries verified\n");
        return 0;
    } catch(const std::exception& error) { std::fprintf(stderr,"WMO oracle: %s\n",error.what()); return 1; }
}
