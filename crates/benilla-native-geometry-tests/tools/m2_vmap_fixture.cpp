// SPDX-License-Identifier: GPL-3.0-or-later
// Authored numeric M2 bytes; exact-source extractor, assembler and queries.
// MPQ acquisition alone is substituted by a reader for our authored local file.
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
#include "model.h"
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

// No archive implementation or installation data is involved. This seam gives
// the original Model::open the exact authored bytes, including original offsets.
class MPQFile {
    std::vector<char> bytes;
public:
    explicit MPQFile(const char* path) {
        std::ifstream input(path, std::ios::binary);
        bytes.assign(std::istreambuf_iterator<char>(input), std::istreambuf_iterator<char>());
    }
    bool isEof() const { return bytes.empty(); }
    char* getBuffer() { return bytes.data(); }
    void close() { bytes.clear(); }
};

// PINNED_SOURCE_METHODS

static uint32 bits(float value) {
    uint32 result;
    std::memcpy(&result, &value, sizeof(result));
    return result;
}

static void hex(std::ostream& out, float value) {
    out << '\t' << std::hex << std::setw(8) << std::setfill('0') << bits(value) << std::dec;
}

static void vector(std::ostream& out, const Vector3& v) {
    for (float value : {v.x, v.y, v.z}) hex(out, value);
}

static void write(FILE* file, const void* data, size_t bytes) {
    require(std::fwrite(data, 1, bytes, file) == bytes, "write authored bytes");
}

template<typename T> static void write(FILE* file, const T& value) {
    write(file, &value, sizeof(value));
}

static void author(const fs::path& path, bool zero) {
    std::vector<Vec3D> vertices;
    std::vector<uint16> indices;
    for (int y=0; y<=4; ++y) for (int x=0; x<=4; ++x) {
        vertices.emplace_back(zero ? (x == 0 ? -0.f : float(x*2)) : float(8+x*2),
                              zero ? (y == 0 ? -0.f : float(y*2)) : float(12+y*2),
                              zero ? -0.f : 4.f);
    }
    for (uint16 y=0; y<4; ++y) for (uint16 x=0; x<4; ++x) {
        const uint16 a=y*5+x, b=a+1, c=a+5, d=c+1;
        indices.insert(indices.end(), {a,b,d,a,d,c});
    }
    ModelHeader header{};
    std::memcpy(header.id, "MD20", 4);
    header.version[0] = zero ? 1 : 0;
    header.version[1] = 1;
    header.nBoundingTriangles = uint32(indices.size());
    header.ofsBoundingTriangles = sizeof(header);
    header.nBoundingVertices = uint32(vertices.size());
    header.ofsBoundingVertices = header.ofsBoundingTriangles + uint32(indices.size()*2);
    FILE* file = std::fopen(path.c_str(), "wb");
    require(file, "create authored M2");
    write(file, header);
    write(file, indices.data(), indices.size()*2);
    write(file, vertices.data(), vertices.size()*sizeof(Vec3D));
    require(std::fclose(file)==0, "close authored M2");
}

static VMAP::ModelSpawn spawn(uint32 id, const std::string& name, const Vector3& pos, float scale,
                              Vector3 rotation=Vector3(0,0,0)) {
    VMAP::ModelSpawn result;
    result.flags = VMAP::MOD_M2;
    result.adtId = 0;
    result.ID = id;
    result.name = name;
    result.iPos = pos;
    result.iRot = rotation;
    result.iScale = scale;
    return result;
}

// Access for auditing the original serialized bounds only. Algorithms unchanged.
struct ProbeModel : VMAP::WorldModel {
    using VMAP::WorldModel::groupModels;
    using VMAP::WorldModel::groupTree;
};

static void checkZeroBound(const AABox& bound) {
    for (float value : {bound.low().x,bound.low().y,bound.low().z,
                       bound.high().x,bound.high().y,bound.high().z})
        require(bits(value)==0, "original extractor/assembler bound must stay positive zero");
}

// Bind the original serialized BIH and ModelSpawn records to the original loaded
// WorldModel objects. This replaces manager ownership only; no query arithmetic.
static void load(const fs::path& root, VMAP::StaticMapTree& tree,
                 std::vector<std::unique_ptr<ProbeModel>>& models) {
    FILE* file = std::fopen((root/"000.vmtree").c_str(), "rb");
    require(file, "open original map tree");
    char chunk[8];
    uint8 tiled;
    require(VMAP::readChunk(file,chunk,VMAP::VMAP_MAGIC,8), "tree magic");
    require(std::fread(&tiled,1,1,file)==1 && tiled==1, "tiled source tree");
    require(VMAP::readChunk(file,chunk,"NODE",4), "tree NODE");
    require(tree.iTree.readFromFile(file), "original BIH reader");
    require(VMAP::readChunk(file,chunk,"GOBJ",4), "tree GOBJ");
    require(std::fgetc(file)==EOF, "no global spawns");
    require(std::fclose(file)==0, "close original tree");
    file=std::fopen((root/"000_32_32.vmtile").c_str(), "rb");
    require(file, "open original tile");
    require(VMAP::readChunk(file,chunk,VMAP::VMAP_MAGIC,8), "tile magic");
    uint32 count;
    require(std::fread(&count,4,1,file)==1 && count==3, "tile count");
    tree.iNTreeValues=count;
    tree.iTreeValues=new VMAP::ModelInstance[count];
    tree.iIsTiled=true;
    for (uint32 i=0; i<count; ++i) {
        VMAP::ModelSpawn placement;
        require(VMAP::ModelSpawn::readFromFile(file,placement), "original spawn reader");
        uint32 index;
        require(std::fread(&index,4,1,file)==1 && index==i, "source model index");
        auto model=std::make_unique<ProbeModel>();
        require(model->readFile((root/(placement.name+".vmo")).string()), "original world model reader");
        require(model->groupModels.size()==1, "source M2 has one group");
        checkZeroBound(model->groupModels.front().GetBound());
        checkZeroBound(model->groupTree.bounds);
        // The original StaticMapTree loader sets flags from this spawn record.
        model->setModelFlags(placement.flags);
        tree.iTreeValues[index]=VMAP::ModelInstance(placement,model.get());
        models.push_back(std::move(model));
    }
    require(std::fgetc(file)==EOF, "tile end");
    require(std::fclose(file)==0, "close original tile");
}

int main(int argc,char** argv) {
    try {
        require(argc==2,"usage: oracle output-directory");
        require(std::fesetround(FE_TONEAREST)==0,"round to nearest");
        static_assert(sizeof(float)==4 && std::numeric_limits<float>::is_iec559);
        static_assert(sizeof(ModelHeader)==324 && offsetof(ModelHeader,nBoundingTriangles)==236);
        static_assert(sizeof(Vec3D)==12 && sizeof(Vector3)==12 && sizeof(AABox)==24);
        const uint32 endian=1;
        require(*reinterpret_cast<const uint8*>(&endian)==1,"little endian");
        const fs::path root(argv[1]);
        const auto raw=root/".assembly";
        const auto products=root/"products";
        fs::create_directory(raw);
        fs::create_directory(products);
        for (bool zero : {false,true}) {
            const std::string stem=zero ? "signed-zero" : "authored";
            const auto input=root/(stem+"-m2.m2.test-fixture");
            author(input,zero);
            std::string filename=input.string();
            Model model(filename);
            StringSet failed;
            require(model.open(failed) && failed.empty(),"original Model::open authored input");
            require(model.ConvertToVMAPModel((raw/(stem+".m2")).c_str()),"original extractor writer");
            fs::copy_file(raw/(stem+".m2"),root/(stem+"-m2.raw.vmap"));
        }
        FILE* directory=std::fopen((raw/"dir_bin").c_str(),"wb");
        require(directory,"create authored placements");
        const std::vector<VMAP::ModelSpawn> spawns={
            spawn(101,"authored.m2",{0,0,0},1.f),
            spawn(102,"signed-zero.m2",{40,0,2},.5f),
            spawn(103,"authored.m2",{70,20,4},1.75f,{13,37,-9})
        };
        for (const auto& placement : spawns) {
            write(directory,uint32(0)); write(directory,uint32(32)); write(directory,uint32(32));
            require(VMAP::ModelSpawn::writeToFile(directory,placement),"original spawn writer");
        }
        require(std::fclose(directory)==0,"close placements");
        VMAP::TileAssembler assembler(raw.string(),products.string());
        require(assembler.convertWorld2(),"original raw reader and complete tile/model assembly");
        fs::remove_all(raw);
        VMAP::StaticMapTree tree(0,products.string());
        std::vector<std::unique_ptr<ProbeModel>> models;
        load(products,tree,models);
        std::ofstream rays(root/"rays.tsv"), points(root/"points.tsv"), heights(root/"heights.tsv");
        rays << "# CMaNGOS " << "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f" << "; binary32 hex; nearest hit\n"
             << "# scope\tcase\tox\toy\toz\tdx\tdy\tdz\tmaximum\tignore_m2\thit\tdistance\n";
        points << "# Original WorldModel point/location and instance area/location; binary32 hex\n"
               << "# scope\tcase\tpx\tpy\tpz\tarea_hit\tarea_distance\tarea_root\tarea_group\tarea_flags\tlocation_hit\tlocation_distance\tlocation_root\tgroup_hit\tgroup_distance\tworld_px\tworld_py\tworld_pz\tinstance_area_hit\tinstance_location_hit\n";
        heights << "# Original StaticMapTree::getHeight; infinity is the original miss result\n"
                << "# case\tpx\tpy\tpz\tmaximum\theight\n";
        auto ray=[&](const std::string& scope,const std::string& name,const Vector3& p,const Vector3& d,float limit) {
            for (bool ignore : {false,true}) {
                float distance=limit;
                const G3D::Ray r(p,d);
                bool hit=scope=="map" ? tree.getIntersectionTime(r,distance,false,ignore)
                          : models[scope=="authored" ? 0 : scope=="signed-zero" ? 1 : 2]->IntersectRay(r,distance,false,ignore);
                rays << scope << '\t' << name; vector(rays,p); vector(rays,d); hex(rays,limit);
                rays << '\t' << unsigned(ignore) << '\t' << unsigned(hit); hex(rays,distance); rays << '\n';
            }
        };
        auto height=[&](const std::string& name,const Vector3& p,float limit) {
            // Public world coordinates round at this scale. Generate the exact
            // source-reachable internal query, without assuming lossless inversion.
            VMAP::VMapManager2 manager;
            const auto world=manager.convertPositionToInternalRep(p.x,p.y,p.z);
            const auto internal=manager.convertPositionToInternalRep(world.x,world.y,world.z);
            heights << name; vector(heights,internal); hex(heights,limit);
            hex(heights,tree.getHeight(internal,limit)); heights << '\n';
        };
        for (size_t i=0;i<3;++i) {
            const auto& placement=tree.iTreeValues[i];
            const std::string scope=i==0 ? "authored" : i==1 ? "signed-zero" : "rotated";
            VMAP::ModelPosition transform;
            transform.iDir=placement.iRot;
            transform.iScale=placement.iScale;
            transform.init();
            auto place=[&](const Vector3& p) { return transform.transform(p)+placement.iPos; };
            auto direction=[&](const Vector3& p) {
                const auto delta=place(p-Vector3(0,0,20))-place(p);
                return delta/delta.magnitude();
            };
            for (int y=-1;y<=9;++y) for (int x=-1;x<=9;++x) {
                Vector3 p(float(x)+(i!=1?8.f:0.f),float(y)+(i!=1?12.f:0.f),i!=1?14.f:10.f);
                const auto name="grid_"+std::to_string(x)+"_"+std::to_string(y);
                ray(scope,name,p,{0,0,-1},20);
                const auto world=place(p);
                if (i==2) {
                    const auto delta=place(p-Vector3(0,0,20))-world;
                    ray("map",scope+"_"+name,world,delta/delta.magnitude(),delta.magnitude());
                } else ray("map",scope+"_"+name,world,{0,0,-1},20);
                height(scope+"_"+name,world,20);
            }
            const Vector3 center=i!=1 ? Vector3(10,14,14) : Vector3(2,2,10);
            for (float limit : {0.f,9.999999f,10.f,10.000001f,20.f}) {
                const auto name="endpoint_"+std::to_string(bits(limit));
                ray(scope,name,center,{0,0,-1},limit);
                ray("map",scope+"_"+name,place(center),
                    i==2 ? direction(center) : Vector3(0,0,-1),limit*placement.iScale);
            }
            ray(scope,"signed_zero_direction",center,{-0.f,-0.f,-1},20);
            ray(scope,"on_surface",{center.x,center.y,center.z-10.f},{0,0,-1},20);
            ray(scope,"up",{center.x,center.y,center.z-20.f},{0,0,1},20);
            const Vector3 world=place(center);
            height(scope+"_up",world-Vector3(0,0,20),-30);
            for (const Vector3 p : {Vector3(0,0,0),Vector3(-0.f,-0.f,-0.f),
                                   Vector3(2,2,0),Vector3(2,2,.1f),Vector3(10,14,4),Vector3(10,14,5)}) {
                VMAP::AreaInfo area;
                area.rootId=123; area.groupId=456; area.flags=789;
                float distance=99.f, locationDistance=88.f;
                VMAP::GroupLocationInfo location;
                location.rootId=321;
                bool hit=models[i]->IntersectPoint(p,{0,0,-1},distance,area);
                bool locationHit=models[i]->GetLocationInfo(p,{0,0,-1},locationDistance,location);
                points << scope << '\t' << "point"; vector(points,p);
                points << '\t' << unsigned(hit); hex(points,distance);
                points << '\t' << area.rootId << '\t' << area.groupId << '\t' << area.flags << '\t' << unsigned(locationHit);
                hex(points,locationDistance); points << '\t' << location.rootId;
                float groupDistance=77.f;
                const bool groupHit=models[i]->groupModels[0].IsInsideObject(p,{0,0,-1},groupDistance);
                points << '\t' << unsigned(groupHit); hex(points,groupDistance);
                VMAP::AreaInfo instanceArea;
                VMAP::LocationInfo instanceLocation;
                const auto placed=place(p);
                placement.intersectPoint(placed,instanceArea);
                const bool located=placement.GetLocationInfo(placed,instanceLocation);
                require(!instanceArea.result && !located,"M2 instances never provide area/location");
                vector(points,placed);
                points << '\t' << unsigned(instanceArea.result) << '\t' << unsigned(located) << '\n';
            }
        }
        for (auto* out : {&rays,&points,&heights}) { out->flush(); require(bool(*out),"write expectations"); }
        std::printf("Original M2 extraction, assembly, reload and queries verified\n");
        return 0;
    } catch (const std::exception& error) {
        std::fprintf(stderr,"M2 oracle: %s\n",error.what());
        return 1;
    }
}
