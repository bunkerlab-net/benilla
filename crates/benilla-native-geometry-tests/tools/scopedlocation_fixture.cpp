// SPDX-License-Identifier: GPL-3.0-or-later
// Authored numeric geometry only; original pinned point, group, instance, tree,
// manager and native serialization algorithms are compiled by the generator.
#define main authored_ray_main
#include "authored_vmap.cpp"
#undef main

static void populate(VMAP::StaticMapTree& tree, const std::vector<ModelSpawn>& spawns,
                     const std::vector<WorldModel*>& models) {
    tree.iIsTiled = true;
    tree.iNTreeValues = uint32(spawns.size());
    tree.iTreeValues = new ModelInstance[spawns.size()];
    for (size_t i = 0; i < spawns.size(); ++i)
        tree.iTreeValues[i] = ModelInstance(spawns[i], models[i]);
    auto bounds = [](const ModelSpawn& value, AABox& result) { result = value.getBounds(); };
    tree.iTree.build(spawns, bounds);
}

// A numeric floor inside a thick authored bound. Unreferenced corner vertices
// preserve those extents through the original assembler's transformed bounds.
// They are valid model vertices; only the 32 floor triangles enter the mesh BIH.
static void addVolumeFloor(Geometry& geometry, unsigned id, float z, uint32 flags) {
    std::vector<Vector3> vertices;
    std::vector<MeshTriangle> triangles;
    for (int y = 0; y <= 4; ++y) for (int x = 0; x <= 4; ++x)
        vertices.emplace_back(float(x * 2 - 4), float(y * 2 - 4), z);
    for (unsigned y = 0; y < 4; ++y) for (unsigned x = 0; x < 4; ++x) {
        const unsigned a = y * 5 + x, b = a + 1, c = a + 5, d = c + 1;
        triangles.emplace_back(a, b, d);
        triangles.emplace_back(a, d, c);
    }
    const AABox bound(Vector3(-4, -4, -1), Vector3(4, 4, 10));
    vertices.push_back(bound.low());
    vertices.push_back(bound.high());
    geometry.raw.groupsArray.push_back({vertices});
    geometry.groups.emplace_back(flags, id, bound);
    geometry.groups.back().setMeshData(vertices, triangles);
}

static void finishVolume(Geometry& geometry, uint32 flags, uint32 rootId,
                         const std::filesystem::path& path) {
    geometry.model.setRootWmoID(rootId);
    geometry.model.setGroupModels(geometry.groups);
    geometry.model.setModelFlags(flags);
    require(geometry.model.writeFile(path.string()), "write authored volume model");
}

class LocationOracle {
    VMAP::VMapManager2 manager;
    VMAP::StaticMapTree* tree = nullptr;
    std::ofstream output;
    std::string dataset;
    size_t count = 0;
    size_t hits = 0;
public:
    explicit LocationOracle(const std::filesystem::path& root)
        : output(root / "scopedlocation.tsv") {
        output << "# Original StaticMapTree::GetLocationInfo and VMapManager2::GetLiquidLevel; CMaNGOS "
                  "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f\n"
               << "# Authored dry VMap bytes; base products unchanged; floats are binary32 hexadecimal bits\n"
               << "# level starts 123.25, floor starts -100000, type starts 0x12345678; height probe is z+2/search=50\n"
               << "# dataset\tcase\twx\twy\twz\tix\tiy\tiz\thit\tground\tspawn_id\troot_id\tgroup_id\tgroup_flags\tadt_id\treq_mask\tliquid_result\tlevel\tfloor\ttype\theight_z_plus_2\n";
    }
    void select(const std::string& name, VMAP::StaticMapTree& value) {
        dataset = name;
        tree = &value;
        manager.iInstanceMapTrees[0] = tree;
    }
    void world(const std::string& name, const Vector3& world, uint8 mask = 0) {
        const auto internal = manager.convertPositionToInternalRep(world.x, world.y, world.z);
        VMAP::LocationInfo info;
        const bool hit = tree->GetLocationInfo(internal, info);
        float level = 123.25f, floor = -100000.f;
        uint32 type = 0x12345678;
        const bool liquid = manager.GetLiquidLevel(0, world.x, world.y, world.z, mask, level, floor, type);
        const auto height = manager.getHeight(0, world.x, world.y, world.z + 2.f, 50.f);
        require(!liquid && bits(level) == bits(123.25f), "dry liquid result leaves level unchanged");
        require(hit ? (bits(floor) == bits(info.ground_Z) && type == 0)
                    : (bits(floor) == bits(-100000.f) && type == 0x12345678),
                "dry liquid floor/type mutation follows location hit");
        output << dataset << '\t' << name;
        for (float value : {world.x, world.y, world.z, internal.x, internal.y, internal.z})
            output << '\t' << std::hex << std::setw(8) << std::setfill('0') << bits(value);
        output << '\t' << std::dec << unsigned(hit) << '\t'
               << std::hex << std::setw(8) << std::setfill('0') << bits(info.ground_Z) << std::dec
               << '\t' << (info.hitInstance ? info.hitInstance->ID : 0)
               << '\t' << info.rootId
               << '\t' << (info.hitModel ? info.hitModel->GetWmoID() : 0)
               << '\t' << (info.hitModel ? info.hitModel->GetMogpFlags() : 0)
               << '\t' << (info.hitInstance ? info.hitInstance->adtId : 0)
               << '\t' << unsigned(mask) << '\t' << unsigned(liquid);
        for (float value : {level, floor})
            output << '\t' << std::hex << std::setw(8) << std::setfill('0') << bits(value);
        output << '\t' << std::dec << type << '\t'
               << std::hex << std::setw(8) << std::setfill('0') << bits(height) << '\n';
        ++count;
        hits += hit;
    }
    void internal(const std::string& name, const Vector3& position, uint8 mask = 0) {
        // Use exact source world conversion twice, keeping its binary32 rounding.
        world(name, manager.convertPositionToInternalRep(position.x, position.y, position.z), mask);
    }
    void local(const std::string& name, const ModelSpawn& spawn, const Vector3& position, uint8 mask = 0) {
        internal(name, place(spawn, position), mask);
    }
    void finish() {
        output.flush();
        require(bool(output), "write source location expectations");
        std::printf("Exact pinned VMap location oracle: %zu queries, %zu location hits, all dry results false\n", count, hits);
    }
};

int main(int argc, char** argv) {
    try {
        require(argc == 2, "usage: scopedlocation-oracle output-directory");
        require(std::fesetround(FE_TONEAREST) == 0, "round-to-nearest");
        static_assert(sizeof(float) == 4 && std::numeric_limits<float>::is_iec559);
        const std::filesystem::path root(argv[1]);
        // Rebuild the original dataset byte-for-byte. The generator compares all
        // six products against existing fixtures and never rewrites any of them.
        Geometry ramp, stack, wall, m2;
        addSurface(ramp, 1, 0, false, true);
        for (unsigned floor = 0; floor < 4; ++floor)
            addSurface(stack, 10 + floor, float(3 * floor), false, false);
        addSurface(wall, 20, 0, true, false);
        addSurface(m2, 30, 0, false, true);
        finish(ramp, 0, root / "ramp.wmo.vmo");
        finish(stack, 0, root / "stack.wmo.vmo");
        finish(wall, 0, root / "wall.wmo.vmo");
        finish(m2, VMAP::MOD_M2, root / "ramp.m2.vmo");
        std::vector<ModelSpawn> spawns = {
            spawn(ramp, 101, "ramp.wmo", {0, 0, 0}, {0, 0, 0}, 1, false),
            spawn(stack, 102, "stack.wmo", {20, 0, 0}, {0, 0, 0}, 1, false),
            spawn(wall, 103, "wall.wmo", {40, 0, 0}, {0, 0, 0}, 1, false),
            spawn(ramp, 104, "ramp.wmo", {70, 20, 4}, {13, 37, -9}, 1.75f, false),
            spawn(m2, 105, "ramp.m2", {100, -15, 2}, {0, 0, 0}, .5f, true)
        };
        VMAP::StaticMapTree baseTree(0, "");
        populate(baseTree, spawns, {&ramp.model, &stack.model, &wall.model, &ramp.model, &m2.model});
        serializeTree(root, baseTree, spawns);
        LocationOracle oracle(root);
        oracle.select("base", baseTree);
        for (size_t index = 0; index < spawns.size(); ++index)
            for (float y : {-4.f, -2.f, 0.f, 2.f, 4.f})
                for (float x : {-4.f, -2.f, 0.f, 2.f, 4.f})
                    for (float z : {-3.f, 0.f, 3.f, 6.f, 9.f}) {
                        const auto name = "model" + std::to_string(index) + "_" +
                            std::to_string(bits(x)) + "_" + std::to_string(bits(y)) + "_" + std::to_string(bits(z));
                        oracle.local(name, spawns[index], {x, y, z});
                    }
        for (float z : {-.101f, -.1f, -.099f, -0.f, 0.f, .099f, .1f, .101f,
                        std::nextafter(3.f, 0.f), 3.f, std::nextafter(3.f, 4.f)})
            for (uint8 mask : {uint8(0), uint8(15)}) {
                const auto name = "ramp_bias_" + std::to_string(bits(z)) + "_" + std::to_string(mask);
                oracle.internal(name, {0, 0, z}, mask);
            }
        oracle.internal("stack_between_zero_thickness_groups", {20, 0, 7});
        oracle.internal("outside_all_bounds", {-100, -100, 0});
        oracle.internal("m2_excluded", {100, -15, 2});
        for (float x : {-4.001f, -3.999f, -.0001f, .125f, 4.001f})
            for (float y : {-4.001f, -3.999f, -.0001f, .125f, 4.001f})
                oracle.internal("rounded_world_" + std::to_string(bits(x)) + "_" + std::to_string(bits(y)), {x, y, 1});

        const auto extra = root / "scopedlocation";
        std::filesystem::create_directory(extra);
        Geometry volume, raised, volumeM2;
        for (unsigned i = 0; i < 4; ++i)
            addVolumeFloor(volume, 201 + i, i < 3 ? float(3 * i) : 6.f, i == 3 ? 0x2000 : 8);
        // A genuine empty serialized group still participates in group point BIH.
        volume.groups.emplace_back(8, 205, AABox(Vector3(-4, -4, -1), Vector3(4, 4, 10)));
        addVolumeFloor(raised, 0xe2345678u, 4, 0x2008);
        addVolumeFloor(volumeM2, 401, 2, 8);
        finishVolume(volume, 0, 21, extra / "volume.wmo.vmo");
        finishVolume(raised, 0, 0xf1234567u, extra / "raised.wmo.vmo");
        finishVolume(volumeM2, VMAP::MOD_M2, 23, extra / "volume.m2.vmo");
        std::vector<ModelSpawn> volumes = {
            spawn(volume, 201, "volume.wmo", {0, 0, 0}, {0, 0, 0}, 1, false),
            spawn(raised, 202, "raised.wmo", {0, 0, 0}, {0, 0, 0}, 1, false),
            spawn(volume, 203, "volume.wmo", {0, 0, 0}, {0, 0, 0}, 1, false),
            spawn(volume, 204, "volume.wmo", {30, 0, 0}, {0, 0, 0}, 1, false),
            spawn(volume, 205, "volume.wmo", {70, 20, 4}, {13, 37, -9}, 1.75f, false),
            spawn(volumeM2, 206, "volume.m2", {100, -15, 2}, {0, 0, 0}, .5f, true),
            spawn(raised, 207, "raised.wmo", {30, 0, 0}, {0, 0, 0}, 1, false),
            spawn(volumeM2, 208, "volume.m2", {0, 0, 5}, {0, 0, 0}, 1, true),
            spawn(volume, 209, "volume.wmo", {130, 5, -3}, {0, 0, 0}, .5f, false)
        };
        for (size_t i = 0; i < volumes.size(); ++i) volumes[i].adtId = uint16(11 + i);
        VMAP::StaticMapTree volumeTree(0, "");
        populate(volumeTree, volumes, {&volume.model, &raised.model, &volume.model, &volume.model,
                                     &volume.model, &volumeM2.model, &raised.model, &volumeM2.model, &volume.model});
        serializeTree(extra, volumeTree, volumes);
        oracle.select("scopedlocation", volumeTree);
        for (size_t index : {size_t(0), size_t(3), size_t(4), size_t(5), size_t(8)})
            for (float y : {-4.f, -2.f, 0.f, 2.f, 4.f})
                for (float x : {-4.f, -2.f, 0.f, 2.f, 4.f})
                    for (float z : {-1.f, 0.f, 2.f, 3.f, 3.95f, 4.f, 5.f, 6.f, 7.f, 10.f}) {
                        const auto name = "model" + std::to_string(index) + "_" +
                            std::to_string(bits(x)) + "_" + std::to_string(bits(y)) + "_" + std::to_string(bits(z));
                        oracle.local(name, volumes[index], {x, y, z});
                    }
        for (float floor : {0.f, 3.f, 4.f, 6.f}) {
            const float edge = floor - .1f;
            for (float z : {std::nextafter(edge, -100.f), edge, std::nextafter(edge, 100.f),
                            floor - .099f, std::nextafter(floor, -100.f), floor,
                            std::nextafter(floor, 100.f), floor + .1f})
                for (uint8 mask : {uint8(0), uint8(15)})
                    oracle.internal("floor_bias_" + std::to_string(bits(floor)) + "_" +
                                    std::to_string(bits(z)) + "_" + std::to_string(mask), {0, 0, z}, mask);
        }
        for (float coordinate : {-4.002f, -4.f, -3.998f, 0.f, 3.998f, 4.f, 4.002f}) {
            oracle.internal("bound_x_" + std::to_string(bits(coordinate)), {coordinate, 0, 5});
            oracle.internal("bound_y_" + std::to_string(bits(coordinate)), {0, coordinate, 5});
        }
        for (float z : {std::nextafter(-1.f, -2.f), -1.f, std::nextafter(-1.f, 0.f),
                        std::nextafter(10.f, 0.f), 10.f, std::nextafter(10.f, 20.f)})
            oracle.internal("bound_z_" + std::to_string(bits(z)), {0, 0, z});
        oracle.internal("location4_height6", {0, 0, 5});
        oracle.internal("nearest_group_and_equal_instance_tie", {0, 0, 7});
        oracle.internal("raised_wins_later", {0, 0, 5});
        oracle.internal("raised_wins_other_order", {30, 0, 5});
        oracle.internal("m2_thick_excluded", {100, -15, 4});
        oracle.internal("point_outside_but_height_available", {0, 0, 11});
        oracle.internal("point_below_all_floors_inside_bounds", {0, 0, -.5f});
        oracle.internal("m2_above_wmo_excluded", {0, 0, 8});
        oracle.internal("map_gap_between_instances", {15, 0, 5});
        oracle.internal("instance_gap_inside_root", {50, 10, 5});
        oracle.internal("signed_zero_z", {0, 0, -0.f});
        for (float z : {5.899f, 5.9f, 5.901f, 5.95f, 6.f, 6.05f}) {
            oracle.local("rotated_local_bias_" + std::to_string(bits(z)), volumes[4], {0, 0, z});
            oracle.local("half_scale_local_bias_" + std::to_string(bits(z)), volumes[8], {0, 0, z});
        }
        oracle.finish();
        return 0;
    } catch (const std::exception& error) {
        std::fprintf(stderr, "VMap location oracle failed: %s\n", error.what());
        return 1;
    }
}
