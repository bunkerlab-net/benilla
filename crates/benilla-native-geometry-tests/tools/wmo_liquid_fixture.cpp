// SPDX-License-Identifier: GPL-3.0-or-later
// Authored numeric WMO bytes; exact-source extraction, assembly and queries.
// Only acquisition, manager ownership and named two-byte padding initialization
// are substituted. No install assets or unspecified padding parity claim.
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
#include <type_traits>
#include <vector>
#include "wmo.h"
#define protected public
#include "WorldModel.h"
#undef protected
#include "ModelInstance.h"
#include "TileAssembler.h"
#include "VMapDefinitions.h"
#include "Maps/GridMapDefines.h"
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
struct Case {
    const char* name;
    const char* member;
    uint32 rootFlags, groupFlags, liquidType;
    bool holeOnly=false, trailing=false, upper=false;
};
static const std::vector<Case> cases = {
    {"Water","Water.wmo",4,8,1,false,false,true},
    {"Oceanbit","Oceanbit.wmo",4,0x80008,1},
    {"Ocean","Ocean.wmo",4,8,2},
    {"Magma","Magma.wmo",4,8,3},
    {"Slime","Slime.wmo",4,8,4},
    {"Naxx","World\\Stratholme_raid\\Naxx.wmo",4,8,4},
    {"Case","World\\stratholme_raid\\Case.wmo",4,8,4},
    {"Known","Known.wmo",4,8,41},
    {"Signed","Signed.wmo",4,8,32768},
    {"Wrapped","Wrapped.wmo",4,8,65537},
    {"Fallback","Fallback.wmo",4,8,0},
    {"Legacy","Legacy.wmo",0,8,15},
    {"Oldwater","Oldwater.wmo",0,8,0},
    {"Oldslime","Oldslime.wmo",0,8,19},
    {"Empty","Empty.wmo",4,8,0,true},
    {"Reset","Reset.wmo",4,8,3,false,true},
    {"Unknown","Unknown.wmo",4,8,22},
    {"Negative","Negative.wmo",4,8,65535},
    {"Zero","Zero.wmo",4,8,65536},
    {"Overflow","Overflow.wmo",0,8,0xffffffffu},
};
static Bytes rootBytes(const Case& c,unsigned index) {
    Bytes bytes,header;
    authoredChunk(bytes,"MVER",version());
    for (uint32 value:{0u,c.upper?2u:1u,0u,0u,0u,0u,0u,0x12345678u,8100u+index}) append(header,value);
    for (float value:{0.f,0.f,0.f,16.f,16.f,18.f}) append(header,value);
    append(header,c.rootFlags); require(header.size()==64,"MOHD64");
    authoredChunk(bytes,"MOHD",header);
    const char names[]="floor\0upper\0";
    authoredChunk(bytes,"MOGN",Bytes(names,names+sizeof(names)-1));
    return bytes;
}
static Bytes groupBytes(const Case& c,unsigned index,bool upper) {
    Bytes header,body,data,materials;
    append(header,uint32(upper?6:0)); append(header,uint32(0)); append(header,c.groupFlags);
    const float low=upper?4.f:0.f, high=upper?8.f:16.f, z=upper?5.f:1.f;
    for (float v:{low,low,z-1.f,high,high,18.f}) append(header,v);
    for (int i=0;i<4;++i) append(header,uint16(0));
    for (uint32 v:{0u,0u,c.liquidType,8200u+index*2+unsigned(upper),0u,0u}) append(header,v);
    require(header.size()==68,"MOGP68"); body=header;
    materials={0x08,0x7f,0x20,0x7f}; authoredChunk(body,"MOPY",materials);
    for (uint16 v:{0,1,2,0,2,3}) append(data,v); authoredChunk(body,"MOVI",data); data.clear();
    for (const auto& v:std::vector<Vector3>{{low,low,z},{high,low,z},{high,high,z},{low,high,z}})
        for (float f:{v.x,v.y,v.z}) append(data,f);
    authoredChunk(body,"MOVT",data); data.clear();
    for (unsigned word=0;word<12;++word) append(data,uint16(word==8?37:word+1));
    authoredChunk(body,"MOBA",data); data.clear();
    if (!upper) {
        for (int32 v:{3,3,2,2}) append(data,v);
        for (float v:{2.f,3.f,99.f}) append(data,v);
        append(data,uint16(0x6abc)); require(data.size()==30,"MLIQ header defines exactly30 bytes");
        unsigned i=0;
        for (float height:{2.f,6.f,10.f,3.f,9.f,5.f,11.f,4.f,7.f}) {
            append(data,uint16(100+i)); append(data,uint16(200+i++)); append(data,height);
        }
        for (uint8 v:{uint8(0x0f),uint8(0x12),uint8(0xa1),uint8(0x83)}) data.push_back(c.holeOnly?uint8(0xff):v);
        require(data.size()==106,"MLIQ payload authored dimensions"); authoredChunk(body,"MLIQ",data);
        if (c.trailing) authoredChunk(body,"MOTV",Bytes{});
    }
    Bytes result; authoredChunk(result,"MVER",version()); authoredChunk(result,"MOGP",body); return result;
}
static std::string memberGroup(const Case& c,unsigned group) {
    auto name=std::string(c.member); name.resize(name.size()-4);
    char suffix[32]; std::snprintf(suffix,sizeof(suffix),"_%03u.wmo",group); return name+suffix;
}
static Vector3 place(const VMAP::ModelSpawn& spawn,const Vector3& p) {
    VMAP::ModelPosition transform; transform.iDir=spawn.iRot; transform.iScale=spawn.iScale; transform.init();
    return transform.transform(p)+spawn.iPos;
}
static VMAP::ModelSpawn spawn(unsigned index,bool rotated=false) {
    VMAP::ModelSpawn result;
    result.flags=VMAP::MOD_HAS_BOUND; result.adtId=rotated?99:20+index; result.ID=rotated?999:300+index;
    result.name=std::string(cases[index].name)+".wmo";
    result.iPos=rotated?Vector3(0,100,4):Vector3(40*index,0,0);
    result.iRot=rotated?Vector3(13,37,-9):Vector3(0,0,0); result.iScale=rotated?1.75f:1.f;
    bool first=true;
    for (float x:{0.f,16.f}) for (float y:{0.f,16.f}) for (float z:{0.f,18.f}) {
        auto p=place(result,{x,y,z});
        if (first) { result.iBound=AABox(p,p); first=false; } else result.iBound.merge(p);
    }
    return result;
}
struct ProbeModel : VMAP::WorldModel { using VMAP::WorldModel::groupModels; };
static void load(const fs::path& root,VMAP::StaticMapTree& tree,std::vector<std::unique_ptr<ProbeModel>>& models) {
    FILE* file=std::fopen((root/"000.vmtree").c_str(),"rb"); require(file,"open original tree");
    char tag[8]; uint8 tiled;
    require(VMAP::readChunk(file,tag,VMAP::VMAP_MAGIC,8),"tree magic");
    require(std::fread(&tiled,1,1,file)==1 && tiled==1,"tiled tree");
    require(VMAP::readChunk(file,tag,"NODE",4),"tree NODE"); require(tree.iTree.readFromFile(file),"original BIH");
    require(VMAP::readChunk(file,tag,"GOBJ",4),"tree GOBJ"); require(std::fgetc(file)==EOF && !std::fclose(file),"tree end");
    file=std::fopen((root/"000_32_32.vmtile").c_str(),"rb"); require(file,"open original tile");
    require(VMAP::readChunk(file,tag,VMAP::VMAP_MAGIC,8),"tile magic");
    uint32 count; require(std::fread(&count,4,1,file)==1 && count==cases.size()+1,"tile count");
    tree.iNTreeValues=count; tree.iTreeValues=new VMAP::ModelInstance[count]; tree.iIsTiled=true;
    for (uint32 i=0;i<count;++i) {
        VMAP::ModelSpawn placement; require(VMAP::ModelSpawn::readFromFile(file,placement),"spawn reader");
        uint32 index; require(std::fread(&index,4,1,file)==1 && index==i,"source model index");
        auto model=std::make_unique<ProbeModel>(); require(model->readFile((root/(placement.name+".vmo")).string()),"model reader");
        model->setModelFlags(placement.flags); tree.iTreeValues[index]=VMAP::ModelInstance(placement,model.get());
        models.push_back(std::move(model));
    }
    require(std::fgetc(file)==EOF && !std::fclose(file),"tile end");
}
extern "C" void assemblerLiquidLayout(size_t* out);
static void layout(const fs::path& root) {
    using H=WMOLiquidHeader;
    static_assert(sizeof(H)==32 && alignof(H)==4 && offsetof(H,type)==28 && sizeof(H::type)==2);
    static_assert(sizeof(WMOLiquidVert)==8 && offsetof(WMOLiquidVert,height)==4);
    static_assert(std::is_standard_layout<H>::value && std::is_trivially_copyable<H>::value);
    const size_t expected[]={32,4,0,4,8,12,16,20,24,28,2};
    const size_t extractor[]={sizeof(H),alignof(H),offsetof(H,xverts),offsetof(H,yverts),offsetof(H,xtiles),offsetof(H,ytiles),
        offsetof(H,pos_x),offsetof(H,pos_y),offsetof(H,pos_z),offsetof(H,type),sizeof(H::type)};
    size_t assembler[11]; assemblerLiquidLayout(assembler);
    std::ofstream out(root/"layout.tsv");
    out << "# declaration\tsize\talign\txverts\tyverts\txtiles\tytiles\tpos_x\tpos_y\tpos_z\ttype\ttype_size\n";
    for (const auto& row:std::vector<std::pair<const char*,const size_t*>>{{"extractor",extractor},{"assembler",assembler}}) {
        out << row.first;
        for (size_t i=0;i<11;++i) {require(row.second[i]==expected[i],"liquid ABI layout");out << '\t' << row.second[i];}
        out << '\n';
    }
}
static Bytes read(const fs::path& path) {
    std::ifstream input(path,std::ios::binary); return Bytes(std::istreambuf_iterator<char>(input),{});
}
static uint32 word(const Bytes& b,size_t at) { uint32 result; require(at+4<=b.size(),"word bounds"); std::memcpy(&result,b.data()+at,4);return result; }
static size_t liquidOffset(const Bytes& b) {
    size_t at=20+36;
    for (const char* tag:{"GRP ","INDX","VERT"}) {
        require(at+8<=b.size() && !std::memcmp(b.data()+at,tag,4),"raw group chunks"); at+=8+word(b,at+4);
    }
    require(at+8<=b.size() && !std::memcmp(b.data()+at,"LIQU",4),"raw LIQU"); return at;
}
static void paddingProof(const fs::path& root,const fs::path& raw,const fs::path& products) {
    auto original=read(raw/"Water.wmo"); const size_t at=liquidOffset(original),header=at+8;
    require(original[header+30]==0 && original[header+31]==0,"explicit zero tail padding");
    require(word(original,at+4)==108,"advertised32+9*8+4");
    const size_t next=header+32+9*4+4;
    require(word(original,next+4)==8201,"actual32+9*4+4 ends at next group");
    auto changed=original; changed[header+30]=0xa5;changed[header+31]=0x5a;
    save(raw/"Water.wmo",changed);
    const auto alternate=root/".padding-products"; fs::create_directory(alternate);
    VMAP::TileAssembler assembler(raw.string(),alternate.string()); require(assembler.convertRawFile("Water.wmo"),"assemble changed padding");
    require(read(products/"Water.wmo.vmo")==read(alternate/"Water.wmo.vmo"),"assembler ignores both padding bytes");
    save(raw/"Water.wmo",original);fs::remove_all(alternate);
    std::ofstream out(root/"padding.tsv");
    out << "# policy\tpadding_offset\tzero_bytes\tmutated_bytes\tassembled_identical\tdeclared_liqu\tactual_liqu\n"
        << "WMO_LIQUID_HEADER_ZERO_PADDING_V1\t30\t0000\ta55a\t1\t108\t72\n";
}

class LiquidOracle {
    VMAP::VMapManager2 manager;
    VMAP::StaticMapTree& tree;
    std::ofstream output;
    size_t count = 0, locations = 0, hits = 0;
public:
    LiquidOracle(const std::filesystem::path& root, VMAP::StaticMapTree& tree) : tree(tree), output(root / "liquid.tsv") {
        manager.iInstanceMapTrees[0] = &tree;
        output << "# Original StaticMapTree::GetLocationInfo, ModelInstance::GetLiquidLevel, VMapManager2::GetLiquidLevel; CMaNGOS "
                  "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f\n"
               << "# Authored wet VMap bytes; binary32 hexadecimal floats; level123.25 floor-100000 type0x12345678\n"
               << "# dataset\tcase\twx\twy\twz\tix\tiy\tiz\thit\tground\tspawn_id\troot_id\tgroup_id\tgroup_flags\tadt_id\treq_mask\tliquid_result\tlevel\tfloor\ttype\tinstance_result\tinstance_level\tinstance_evaluated\n";
    }
    void world(const std::string& name, const Vector3& world, uint8 mask = 0, bool evaluateInstance = true) {
        const auto p = manager.convertPositionToInternalRep(world.x, world.y, world.z);
        VMAP::LocationInfo info;
        const bool found = tree.GetLocationInfo(p, info);
        float level = 123.25f, floor = -100000.f, instanceLevel = 123.25f;
        uint32 type = 0x12345678;
        const bool hit = manager.GetLiquidLevel(0, world.x, world.y, world.z, mask, level, floor, type);
        const bool instanceHit = found && evaluateInstance && info.hitInstance->GetLiquidLevel(p, info, instanceLevel);
        require(found ? bits(floor) == bits(info.ground_Z) && type == info.hitModel->GetLiquidType()
                      : bits(floor) == bits(-100000.f) && type == 0x12345678,
                "manager floor/type writes occur exactly on location hit");
        require(hit || bits(level) == bits(123.25f), "false manager result preserves level");
        require(instanceHit || bits(instanceLevel) == bits(123.25f), "false instance result preserves level");
        require(!evaluateInstance || hit == (instanceHit && (!mask || (VMAP::GetLiquidMask(type) & mask))), "source mask gate");
        require(evaluateInstance || (found && mask && !(VMAP::GetLiquidMask(type) & mask) && !hit),
                "unsafe instance probes only omitted behind proven source mask rejection");
        require(!hit || bits(level) == bits(instanceLevel), "manager uses selected original instance level");
        output << "wmo_liquid\t" << name;
        for (float value : {world.x, world.y, world.z, p.x, p.y, p.z}) hex(output, value);
        output << '\t' << unsigned(found); hex(output, info.ground_Z);
        output << '\t' << (info.hitInstance ? info.hitInstance->ID : 0) << '\t' << info.rootId
               << '\t' << (info.hitModel ? info.hitModel->GetWmoID() : 0)
               << '\t' << (info.hitModel ? info.hitModel->GetMogpFlags() : 0)
               << '\t' << (info.hitInstance ? info.hitInstance->adtId : 0)
               << '\t' << unsigned(mask) << '\t' << unsigned(hit);
        hex(output, level); hex(output, floor);
        output << '\t' << type << '\t' << unsigned(instanceHit); hex(output, instanceLevel);
        output << '\t' << unsigned(found && evaluateInstance) << '\n';
        ++count; locations += found; hits += hit;
    }
    void internal(const std::string& name, const Vector3& p, uint8 mask = 0, bool evaluateInstance = true) {
        world(name, manager.convertPositionToInternalRep(p.x, p.y, p.z), mask, evaluateInstance);
    }
    void local(const std::string& name, const VMAP::ModelSpawn& spawn, const Vector3& p, uint8 mask = 0, bool evaluateInstance = true) {
        internal(name, place(spawn, p), mask, evaluateInstance);
    }
    void finish() {
        output.flush(); require(bool(output), "write original manager liquid expectations");
        std::printf("Exact pinned wet manager oracle: %zu queries, %zu location hits, %zu wet hits\n", count, locations, hits);
    }
};
int main(int argc,char** argv) {
    try {
        require(argc==2,"usage: oracle output-directory"); require(!std::fesetround(FE_TONEAREST),"round to nearest");
        static_assert(sizeof(float)==4 && std::numeric_limits<float>::is_iec559);
        const uint32 endian=1;require(*reinterpret_cast<const uint8*>(&endian)==1,"little endian");
        const fs::path root(argv[1]),raw=root/".assembly",products=root/"products",inputs=root/"inputs",rawOutputs=root/"raw";
        for (const auto& p:{raw,products,inputs,rawOutputs}) fs::create_directory(p);
        layout(root);
        std::ofstream manifest(root/"cases.tsv");
        manifest << "# case\troot_member\troot_flags\tgroup_flags\tliquid_type\thole_only\ttrailing_chunk\tupper_group\toutput_name\traw_type_bits\tassembled_type\troot_input\tgroup_inputs\traw_output\tgroup_count\ttriangle_count\tliquid_group_count\n";
        require(raw.string().size()<sizeof(szWorkDirWmo),"temporary path bound");std::strcpy(szWorkDirWmo,raw.c_str());
        for (unsigned i=0;i<cases.size();++i) {
            const auto& c=cases[i];const auto name=std::string(c.name);
            const auto path=inputs/(name+".root.test-fixture");save(path,rootBytes(c,i));authoredFiles[c.member]=path;
            for (unsigned g=0;g<(c.upper?2u:1u);++g) {
                char suffix[60];std::snprintf(suffix,sizeof(suffix),"_%03u.group.test-fixture",g);
                const auto path=inputs/(name+suffix);save(path,groupBytes(c,i,g));authoredFiles[memberGroup(c,g)]=path;
            }
            auto filename=std::string(c.member);require(ExtractSingleWmo(filename),"original WMO extraction");
            const auto product=raw/(name+".wmo");auto bytes=read(product);require(!bytes.empty(),"source output exists");
            save(rawOutputs/(name+".raw.vmap"),bytes);
            uint16 typeBits=0;uint32 promoted=0;
            if (!c.trailing) {
                const size_t at=liquidOffset(bytes);std::memcpy(&typeBits,bytes.data()+at+8+28,2);
                VMAP::WorldModel_Raw model;require(model.Read(product.c_str()),"original raw reader");
                require(model.groupsArray[0].liquid,"raw reader keeps liquid");promoted=model.groupsArray[0].liquid->GetType();
                require(promoted==uint32(int32(int16(typeBits))),"signed short promotion");
            }
            manifest << name << '\t' << c.member << '\t' << c.rootFlags << '\t' << c.groupFlags << '\t' << c.liquidType
                << '\t' << c.holeOnly << '\t' << c.trailing << '\t' << c.upper << '\t' << name << ".wmo\t" << typeBits << '\t' << promoted << '\t' << "inputs/" << name << ".root.test-fixture\tinputs/" << name << "_000.group.test-fixture"
                << (c.upper?";inputs/Water_001.group.test-fixture":"") << "\traw/" << name << ".raw.vmap\t"
                << (c.upper?2:1) << '\t' << (c.upper?4:2) << '\t' << (!c.trailing) << '\n';
        }
        FILE* directory=std::fopen((raw/"dir_bin").c_str(),"wb");require(directory,"authored placements");
        std::vector<VMAP::ModelSpawn> placements;
        for (unsigned i=0;i<cases.size();++i) placements.push_back(spawn(i));placements.push_back(spawn(0,true));
        for (const auto& p:placements) {
            write(directory,uint32(0));write(directory,uint32(32));write(directory,uint32(32));
            require(VMAP::ModelSpawn::writeToFile(directory,p),"original spawn writer");
        }
        require(!std::fclose(directory),"close placements");
        VMAP::TileAssembler assembler(raw.string(),products.string());require(assembler.convertWorld2(),"original complete assembler");
        paddingProof(root,raw,products);fs::remove_all(raw);
        VMAP::StaticMapTree tree(0,products.string());std::vector<std::unique_ptr<ProbeModel>> models;load(products,tree,models);
        LiquidOracle oracle(root,tree);
        for (unsigned i=0;i<placements.size();++i) {
            const auto& p=placements[i];const std::string base=i==cases.size()?"Rotated":cases[i].name;
            const std::vector<std::pair<const char*,Vector3>> samples={
                {"disabled",{3,4,4}}, {"wet_a",{8,4,4}}, {"wet_b",{8,5.5f,4}},
                {"wet_diagonal",{8,9,4}}, {"high_flags",{3,9,4}}, {"dry_upper",{7,7,7}},
                {"below_upper",{7,7,4}}, {"outside_liquid_x",{1.99f,7,4}},
                {"outside_liquid_y",{7,2.99f,4}}, {"upper_x",{2+2*LIQUID_TILE_SIZE,8,4}},
                {"upper_y",{8,3+2*LIQUID_TILE_SIZE,4}}, {"floorless",{8,8,-1}},
                {"outside_mesh",{20,20,4}}, {"above_liquid",{8,4,17}}, {"signed_zero",{-0.f,-0.f,4}}};
            for (const auto& sample:samples) for (uint8 mask:{0,1,2,4,8,15,16})
                oracle.local(base+"_"+sample.first+"_mask"+std::to_string(mask),p,sample.second,mask);
        }
        oracle.finish();
        std::ofstream direct(root/"direct.tsv");direct << "# model\tcase\tx\ty\tz\thit\tlevel\ttype\n";
        const auto& group=models[0]->groupModels[0];unsigned count=0;
        for (unsigned y=0;y<2;++y) for (unsigned x=0;x<2;++x)
            for (float dy:{0.f,.125f,.5f,.875f}) for (float dx:{0.f,.125f,.5f,.875f}) {
                Vector3 p(2+(x+dx)*LIQUID_TILE_SIZE,3+(y+dy)*LIQUID_TILE_SIZE,4);
                float level=123.25f;bool hit=group.GetLiquidLevel(p,level);
                direct << "Water.wmo\t" << count++;emitVector(direct,p);direct << '\t' << hit;hex(direct,level);direct << '\t' << group.GetLiquidType() << '\n';
            }
        std::puts("Original liquid header layout, tail-padding independence and signed type promotion verified");return 0;
    } catch(const std::exception& e) {std::fprintf(stderr,"%s\n",e.what());return 1;}
}
