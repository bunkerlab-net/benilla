// SPDX-License-Identifier: GPL-3.0-or-later
// Authored numeric geometry only. The generator compiles pinned original VMap/G3D
// sources, injecting source-exact methods at the marker below. No native input
// loader is called. CMaNGOS and G3D grants are retained beside this harness.
#include <cfenv>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <iomanip>
#include <limits>
#include <map>
#include <memory>
#include <sstream>
#include <stdexcept>
#include <string>
#include <unordered_map>
#include <vector>
#include "WorldModel.h"
#include "ModelInstance.h"
#include "TileAssembler.h"
#include "VMapDefinitions.h"
// Fixture construction needs direct access to populate the original tree without
// invoking any parser. This changes access only, not the class data or algorithms.
#define private public
#include "MapTree.h"
#undef private

using G3D::Vector3;
using G3D::AABox;
using VMAP::ModelSpawn;
using VMAP::ModelInstance;
using VMAP::WorldModel;
using VMAP::GroupModel;
using VMAP::MeshTriangle;

// PINNED_SOURCE_METHODS

static void require(bool condition, const char* message) {
    if (!condition) throw std::runtime_error(message);
}

static uint32_t bits(float value) {
    uint32_t result;
    std::memcpy(&result, &value, sizeof(result));
    return result;
}

struct Geometry {
    AuthoredRaw raw;
    std::vector<GroupModel> groups;
    WorldModel model;
};

// A regular 4x4 grid produces 32 original triangles and nontrivial native BIHs.
static void addSurface(Geometry& geometry, unsigned id, float z, bool wall, bool ramp) {
    std::vector<Vector3> vertices;
    std::vector<MeshTriangle> triangles;
    for (int y = 0; y <= 4; ++y) {
        for (int x = 0; x <= 4; ++x) {
            const float a = float(x * 2 - 4), b = float(y * 2 - 4);
            vertices.emplace_back(wall ? 0.f : a, b, wall ? a + 4.f : z + (ramp ? .5f*a + .25f*b : 0.f));
        }
    }
    for (unsigned y = 0; y < 4; ++y) {
        for (unsigned x = 0; x < 4; ++x) {
            const unsigned a = y*5 + x, b = a+1, c = a+5, d = c+1;
            triangles.emplace_back(a,b,d);
            triangles.emplace_back(a,d,c);
        }
    }
    AABox bound(vertices.front(), vertices.front());
    for (const auto& vertex : vertices) bound.merge(vertex);
    geometry.raw.groupsArray.push_back({vertices});
    geometry.groups.emplace_back(8, id, bound);
    geometry.groups.back().setMeshData(vertices, triangles);
}

static void finish(Geometry& geometry, uint32 flags, const std::filesystem::path& path) {
    geometry.model.setRootWmoID(17);
    geometry.model.setGroupModels(geometry.groups);
    geometry.model.setModelFlags(flags);
    require(geometry.model.writeFile(path.string()), "write authored model");
}

static ModelSpawn spawn(Geometry& geometry, uint32 id, const std::string& name,
                        const Vector3& position, const Vector3& rotation, float scale, bool m2) {
    ModelSpawn result;
    result.flags = VMAP::MOD_HAS_BOUND | (m2 ? VMAP::MOD_M2 : 0);
    result.adtId = 7;
    result.ID = id;
    result.iPos = position;
    result.iRot = rotation;
    result.iScale = scale;
    result.name = name;
    require(authoredBound(result, geometry.raw), "calculate authored bound");
    return result;
}

static void writeBytes(FILE* file, const void* bytes, size_t count) {
    require(std::fwrite(bytes, 1, count, file) == count, "write authored product");
}

template <typename T> static void writeValue(FILE* file, const T& value) {
    writeBytes(file, &value, sizeof(value));
}

static void serializeTree(const std::filesystem::path& root, VMAP::StaticMapTree& tree,
                          const std::vector<ModelSpawn>& spawns) {
    FILE* file = std::fopen((root / "000.vmtree").c_str(), "wb");
    require(file, "open authored tree");
    writeBytes(file, VMAP::VMAP_MAGIC, 8);
    const uint8 tiled = 1;
    writeValue(file, tiled);
    writeBytes(file, "NODE", 4);
    require(tree.iTree.writeToFile(file), "serialize exact BIH");
    writeBytes(file, "GOBJ", 4);
    require(std::fclose(file) == 0, "close authored tree");

    file = std::fopen((root / "000_32_32.vmtile").c_str(), "wb");
    require(file, "open authored tile");
    writeBytes(file, VMAP::VMAP_MAGIC, 8);
    writeValue(file, uint32(spawns.size()));
    for (uint32 index = 0; index < spawns.size(); ++index) {
        require(ModelSpawn::writeToFile(file, spawns[index]), "serialize exact spawn");
        writeValue(file, index);
    }
    require(std::fclose(file) == 0, "close authored tile");
}

static Vector3 place(const ModelSpawn& spawn, const Vector3& local) {
    VMAP::ModelPosition transform;
    transform.iDir = spawn.iRot;
    transform.iScale = spawn.iScale;
    transform.init();
    return transform.transform(local) + spawn.iPos;
}

class Oracle {
    VMAP::StaticMapTree& tree;
    std::ofstream output;
    std::ofstream segments;
    std::ofstream worldSegments;
    size_t count = 0;
public:
    Oracle(VMAP::StaticMapTree& tree, const std::filesystem::path& path) : tree(tree), output(path),
        segments(path.parent_path()/"segments.tsv"), worldSegments(path.parent_path()/"world-segments.tsv") {
        output << "# Authored VMap ray oracle; CMaNGOS 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f\n"
               << "# Binary32 values are hexadecimal bits; VMap internal coordinates; nearest hit (stopFirst=false)\n"
               << "# case\tox\toy\toz\tdx\tdy\tdz\tmax_distance\tignore_m2\thit\tresult_distance\n";
        segments << "# Exact binary32 endpoint normalization; internal VMap coordinates; nearest hit\n"
                 << "# case\tax\tay\taz\tbx\tby\tbz\tdx\tdy\tdz\tmax_distance\tignore_m2\thit\tresult_distance\n";
        worldSegments << "# Original VMapManager2 conversion, then normalized nearest ray; exact binary32 bits\n"
                      << "# case\twax\tway\twaz\twbx\twby\twbz\tax\tay\taz\tbx\tby\tbz\tdx\tdy\tdz\tmax_distance\tignore_m2\thit\tresult_distance\n";
    }
    void ray(const std::string& name, const Vector3& origin, const Vector3& direction, float maximum) {
        require(direction.magnitude() > 0.f && std::isfinite(maximum) && maximum >= 0.f, "valid authored ray");
        for (bool ignore : {false, true}) {
            float result = maximum;
            const bool hit = tree.getIntersectionTime(G3D::Ray(origin, direction), result, false, ignore);
            output << name;
            for (float value : {origin.x, origin.y, origin.z, direction.x, direction.y, direction.z, maximum})
                output << '\t' << std::hex << std::setw(8) << std::setfill('0') << bits(value);
            output << std::dec << '\t' << unsigned(ignore) << '\t' << unsigned(hit) << '\t'
                   << std::hex << std::setw(8) << std::setfill('0') << bits(result) << std::dec << '\n';
            ++count;
        }
    }
    void segment(const std::string& name, const Vector3& a, const Vector3& b) {
        // Exact normalization expression used by StaticMapTree's public segment API.
        const float maximum = (b - a).magnitude();
        require(maximum > 1e-10f, "nonzero authored segment");
        ray(name, a, (b - a) / maximum, maximum);
        endpointRow(segments,name,a,b,{},false);
    }
    void endpointRow(std::ofstream& destination, const std::string& name, const Vector3& a,
                     const Vector3& b, const std::vector<float>& prefix, bool hasPrefix) {
        const float maximum = (b-a).magnitude();
        const Vector3 direction = (b-a)/maximum;
        for (bool ignore : {false,true}) {
            float result=maximum;
            const bool hit=tree.getIntersectionTime(G3D::Ray(a,direction),result,false,ignore);
            destination << name;
            if (hasPrefix) for (float value : prefix)
                destination << '\t' << std::hex << std::setw(8) << std::setfill('0') << bits(value);
            for (float value : {a.x,a.y,a.z,b.x,b.y,b.z,direction.x,direction.y,direction.z,maximum})
                destination << '\t' << std::hex << std::setw(8) << std::setfill('0') << bits(value);
            destination << std::dec << '\t' << unsigned(ignore) << '\t' << unsigned(hit) << '\t'
                        << std::hex << std::setw(8) << std::setfill('0') << bits(result) << std::dec << '\n';
        }
    }
    void world(const std::string& name, const Vector3& internalA, const Vector3& internalB) {
        VMAP::VMapManager2 manager;
        // This symmetric source transform creates the authored world inputs. Convert
        // those exact input bits again for the oracle; never assume lossless roundtrip.
        const auto wa=manager.convertPositionToInternalRep(internalA.x,internalA.y,internalA.z);
        const auto wb=manager.convertPositionToInternalRep(internalB.x,internalB.y,internalB.z);
        const auto a=manager.convertPositionToInternalRep(wa.x,wa.y,wa.z);
        const auto b=manager.convertPositionToInternalRep(wb.x,wb.y,wb.z);
        endpointRow(worldSegments,name,a,b,{wa.x,wa.y,wa.z,wb.x,wb.y,wb.z},true);
    }
    void local(const std::string& name, const ModelSpawn& model, const Vector3& a, const Vector3& b) {
        segment(name, place(model, a), place(model, b));
    }
    size_t finish() {
        output.flush(); segments.flush(); worldSegments.flush();
        require(bool(output) && bool(segments) && bool(worldSegments), "write authored expectations");
        return count;
    }
};

int main(int argc, char** argv) {
    try {
        require(argc == 2, "usage: oracle output-directory");
        require(std::fesetround(FE_TONEAREST) == 0, "round-to-nearest");
        static_assert(sizeof(float)==4 && std::numeric_limits<float>::is_iec559);
        static_assert(sizeof(Vector3)==12 && sizeof(AABox)==24 && sizeof(MeshTriangle)==12);
        const uint32 endian = 1;
        require(*reinterpret_cast<const unsigned char*>(&endian) == 1, "little endian");
        const std::filesystem::path root(argv[1]);
        Geometry ramp, stack, wall, m2;
        addSurface(ramp,1,0,false,true);
        for (unsigned floor=0; floor<4; ++floor) addSurface(stack,10+floor,float(3*floor),false,false);
        addSurface(wall,20,0,true,false);
        addSurface(m2,30,0,false,true);
        finish(ramp,0,root/"ramp.wmo.vmo");
        finish(stack,0,root/"stack.wmo.vmo");
        finish(wall,0,root/"wall.wmo.vmo");
        finish(m2,VMAP::MOD_M2,root/"ramp.m2.vmo");
        std::vector<ModelSpawn> spawns = {
            spawn(ramp,101,"ramp.wmo",{0,0,0},{0,0,0},1,false),
            spawn(stack,102,"stack.wmo",{20,0,0},{0,0,0},1,false),
            spawn(wall,103,"wall.wmo",{40,0,0},{0,0,0},1,false),
            spawn(ramp,104,"ramp.wmo",{70,20,4},{13,37,-9},1.75f,false),
            spawn(m2,105,"ramp.m2",{100,-15,2},{0,0,0},.5f,true)
        };
        std::vector<WorldModel*> models = {&ramp.model,&stack.model,&wall.model,&ramp.model,&m2.model};
        VMAP::StaticMapTree tree(0, "");
        tree.iIsTiled = true;
        tree.iNTreeValues = uint32(spawns.size());
        tree.iTreeValues = new ModelInstance[spawns.size()];
        for (size_t i=0; i<spawns.size(); ++i) tree.iTreeValues[i] = ModelInstance(spawns[i],models[i]);
        auto bounds = [](const ModelSpawn& spawn, AABox& result) { result=spawn.getBounds(); };
        tree.iTree.build(spawns,bounds);
        serializeTree(root,tree,spawns);
        Oracle oracle(tree,root/"rays.tsv");
        for (size_t index : {size_t(0),size_t(1),size_t(3),size_t(4)}) {
            // Interior, shared edge, exact corner and outside-box rays, in both directions.
            for (int y=-5; y<=5; ++y) for (int x=-5; x<=5; ++x) {
                const std::string name="model"+std::to_string(index)+"_grid_"+std::to_string(x)+"_"+std::to_string(y);
                oracle.local(name+"_down",spawns[index],{float(x),float(y),15},{float(x),float(y),-5});
                oracle.local(name+"_up",spawns[index],{float(x),float(y),-5},{float(x),float(y),15});
            }
        }
        for (int z=-1; z<=10; ++z) for (int y=-5; y<=5; ++y) {
            const auto name="wall_"+std::to_string(y)+"_"+std::to_string(z);
            oracle.local(name+"_forward",spawns[2],{-5,float(y),float(z)},{5,float(y),float(z)});
            oracle.local(name+"_back",spawns[2],{5,float(y),float(z)},{-5,float(y),float(z)});
        }
        // Source strict t > 0 and t < maximum behavior, including adjacent float endpoints.
        for (float limit : {0.f, 9.999999f, 10.f, 10.000001f, 20.f}) {
            const std::string suffix=std::to_string(bits(limit));
            oracle.ray("ramp_endpoint_"+suffix,{0,0,10},{0,0,-1},limit);
            oracle.ray("wall_endpoint_"+suffix,{30,0,4},{1,0,0},limit);
        }
        oracle.ray("ramp_origin_on_surface",{0,0,0},{0,0,-1},10);
        oracle.ray("wall_origin_on_surface",{40,0,4},{1,0,0},10);
        oracle.ray("stack_surface_to_next_floor",{20,0,6},{0,0,-1},20);
        oracle.ray("stack_between_floors_down",{20,0,7},{0,0,-1},20);
        oracle.ray("stack_between_floors_up",{20,0,7},{0,0,1},20);
        oracle.ray("stack_exact_lower_endpoint",{20,0,7},{0,0,-1},1);
        oracle.ray("coplanar_ramp_ray",{0,0,0},{0,1,0},10);
        oracle.ray("parallel_above_all",{-10,0,20},{1,0,0},150);
        oracle.ray("signed_zero_axis",{0,0,10},{-0.f,0.f,-1},20);
        oracle.segment("diagonal_across_wall",{35,-3,7},{45,3,1});
        oracle.segment("clear_above_wall",{35,-3,12},{45,3,12});
        oracle.segment("across_multiple_models",{-10,0,1},{110,0,1});
        for (size_t index : {size_t(0),size_t(1),size_t(3),size_t(4)}) {
            for (float x : {-3.999f,.125f,4.001f}) for (float y : {-3.999f,.125f,4.001f}) {
                oracle.world("world_model"+std::to_string(index)+"_"+std::to_string(bits(x))+"_"+std::to_string(bits(y)),
                             place(spawns[index],{x,y,15}),place(spawns[index],{x,y,-5}));
            }
        }
        for (float z : {-1.f,4.f,9.f})
            oracle.world("world_wall_"+std::to_string(bits(z)),{35,-1,z},{45,1,z});
        const auto count=oracle.finish();
        std::printf("Exact pinned VMap oracle: %zu rays, 5 instances, 4 authored models\n",count);
        return 0;
    } catch (const std::exception& error) {
        std::fprintf(stderr,"VMap oracle failed: %s\n",error.what());
        return 1;
    }
}
