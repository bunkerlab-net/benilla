// SPDX-License-Identifier: GPL-3.0-or-later
// Authored numeric liquids/geometry; all queries and native serialization execute
// the hash-guarded original CMaNGOS sources, without invoking native parsers.
#include "authored_location.cpp"

static void putFloat(std::ostream& out, float value) {
    out << '\t' << std::hex << std::setw(8) << std::setfill('0') << bits(value) << std::dec;
}

// Wider numeric geometry admits liquid edges and holes while the point remains
// inside collision bounds. Unreferenced corners preserve the volume extents.
static void addLiquidFloor(Geometry& geometry, uint32 id, float z, uint32 flags) {
    std::vector<Vector3> vertices;
    std::vector<MeshTriangle> triangles;
    for (unsigned y = 0; y <= 4; ++y) for (unsigned x = 0; x <= 4; ++x)
        vertices.emplace_back(float(x * 5) - 8.f, float(y * 5) - 8.f, z);
    for (unsigned y = 0; y < 4; ++y) for (unsigned x = 0; x < 4; ++x) {
        const unsigned a = y * 5 + x, b = a + 1, c = a + 5, d = c + 1;
        triangles.emplace_back(a, b, d);
        triangles.emplace_back(a, d, c);
    }
    const AABox bound(Vector3(-8, -8, -2), Vector3(12, 12, 18));
    vertices.push_back(bound.low());
    vertices.push_back(bound.high());
    geometry.raw.groupsArray.push_back({vertices});
    geometry.groups.emplace_back(flags, id, bound);
    geometry.groups.back().setMeshData(vertices, triangles);
}

struct AuthoredLiquid {
    std::string model;
    unsigned groupIndex;
    uint32 groupId;
    uint32 width, height;
    Vector3 corner;
    VMAP::WmoLiquid* liquid;
    // These point into the group vector after construction; swapping it into the
    // original WorldModel preserves element addresses.
    GroupModel* group = nullptr;
};

static AuthoredLiquid addLiquid(Geometry& geometry, const std::string& model,
                               uint32 width, uint32 height, const Vector3& corner,
                               uint32 type, const std::vector<float>& levels,
                               const std::vector<uint8>& flags) {
    require(levels.size() == (width + 1) * (height + 1), "authored liquid height count");
    require(flags.size() == width * height, "authored liquid flag count");
    auto* liquid = new VMAP::WmoLiquid(width, height, corner, type);
    std::copy(levels.begin(), levels.end(), liquid->GetHeightStorage());
    std::copy(flags.begin(), flags.end(), liquid->GetFlagsStorage());
    AuthoredLiquid result{model, unsigned(geometry.groups.size() - 1),
        geometry.groups.back().GetWmoID(), width, height, corner, liquid};
    geometry.groups.back().setLiquidData(liquid);
    require(liquid == nullptr, "original group takes liquid ownership");
    return result;
}

static std::vector<unsigned char> writtenBytes(FILE* file) {
    const auto length = std::ftell(file);
    require(length >= 0, "measure original serialized bytes");
    require(std::fseek(file, 0, SEEK_SET) == 0, "rewind authored temporary serialization");
    std::vector<unsigned char> bytes(static_cast<size_t>(length));
    require(std::fread(bytes.data(), 1, bytes.size(), file) == bytes.size(), "read authored serialization");
    require(std::fclose(file) == 0, "close authored temporary serialization");
    return bytes;
}

static void auditSize(std::ostream& out, const AuthoredLiquid& authored) {
    FILE* file = std::tmpfile();
    require(file != nullptr, "temporary liquid serialization");
    require(authored.liquid->writeToFile(file), "original liquid writer");
    const auto payload = writtenBytes(file);
    const auto declared = authored.liquid->GetFileSize();
    require(payload.size() == size_t(declared) + sizeof(uint32), "source GetFileSize excludes iType");
    file = std::tmpfile();
    require(file != nullptr, "temporary group serialization");
    require(authored.group->writeToFile(file), "original group writer");
    const auto group = writtenBytes(file);
    require(group.size() >= payload.size() + 8, "group contains LIQU header");
    const size_t chunk = group.size() - payload.size() - 8;
    require(std::memcmp(group.data() + chunk, "LIQU", 4) == 0, "LIQU ends serialized nonempty group");
    uint32 groupDeclared;
    std::memcpy(&groupDeclared, group.data() + chunk + 4, sizeof(groupDeclared));
    require(groupDeclared == declared, "group writes undersized source LIQU length");
    require(std::memcmp(group.data() + chunk + 8, payload.data(), payload.size()) == 0,
            "group emits exact original liquid payload including type");
    out << authored.model << '\t' << authored.groupIndex << '\t' << authored.groupId
        << '\t' << authored.width << '\t' << authored.height;
    for (float value : {authored.corner.x, authored.corner.y, authored.corner.z}) putFloat(out, value);
    out << '\t' << authored.liquid->GetType() << '\t' << unsigned(VMAP::GetLiquidMask(authored.liquid->GetType()))
        << '\t' << declared << '\t' << payload.size() << '\t' << group.size() << '\t' << chunk << '\n';
}

class DirectOracle {
    std::ofstream output;
    size_t count = 0, hits = 0;
public:
    explicit DirectOracle(const std::filesystem::path& root) : output(root / "scopedliquid_direct.tsv") {
        output << "# Original WmoLiquid::GetLiquidHeight and GroupModel::GetLiquidLevel; CMaNGOS "
                  "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f\n"
               << "# Authored liquids owned by the exact serialized native GroupModel; level starts 123.25\n"
               << "# model\tgroup_index\tgroup_id\tcase\tx\ty\tz\thit\tlevel\tgroup_hit\tgroup_level\ttype\n";
    }
    void query(const AuthoredLiquid& liquid, const std::string& name, const Vector3& p) {
        float level = 123.25f, groupLevel = 123.25f;
        const bool hit = liquid.liquid->GetLiquidHeight(p, level);
        const bool groupHit = liquid.group->GetLiquidLevel(p, groupLevel);
        require(hit == groupHit && bits(level) == bits(groupLevel), "original group delegates to liquid");
        require(hit || bits(level) == bits(123.25f), "false liquid result leaves output unchanged");
        output << liquid.model << '\t' << liquid.groupIndex << '\t' << liquid.groupId << '\t' << name;
        for (float value : {p.x, p.y, p.z}) putFloat(output, value);
        output << '\t' << unsigned(hit); putFloat(output, level);
        output << '\t' << unsigned(groupHit); putFloat(output, groupLevel);
        output << '\t' << liquid.liquid->GetType() << '\n';
        ++count; hits += hit;
    }
    void samples(const AuthoredLiquid& liquid) {
        const float tile = LIQUID_TILE_SIZE;
        for (uint32 y = 0; y < liquid.height; ++y) for (uint32 x = 0; x < liquid.width; ++x)
            for (float dy : {0.f, .125f, .5f, .875f}) for (float dx : {0.f, .125f, .5f, .875f})
                for (float z : {-50.f, 0.f, 50.f}) {
                    const auto name = "tile_" + std::to_string(x) + "_" + std::to_string(y) + "_" +
                        std::to_string(bits(dx)) + "_" + std::to_string(bits(dy)) + "_" + std::to_string(bits(z));
                    query(liquid, name, {liquid.corner.x + (float(x) + dx) * tile,
                                        liquid.corner.y + (float(y) + dy) * tile, z});
                }
        // Finite positions just below the lower edge truncate to zero before the
        // source's negative check. No undefined out-of-range float-to-u32 casts.
        const float middleX = liquid.corner.x + .5f * tile;
        const float middleY = liquid.corner.y + .5f * tile;
        for (uint32 i = 0; i <= liquid.width; ++i) {
            const float edge = liquid.corner.x + float(i) * tile;
            for (float x : {std::nextafter(edge, -100.f), edge, std::nextafter(edge, 100.f)})
                query(liquid, "x_edge_" + std::to_string(i) + "_" + std::to_string(bits(x)), {x, middleY, 4});
        }
        for (uint32 i = 0; i <= liquid.height; ++i) {
            const float edge = liquid.corner.y + float(i) * tile;
            for (float y : {std::nextafter(edge, -100.f), edge, std::nextafter(edge, 100.f)})
                query(liquid, "y_edge_" + std::to_string(i) + "_" + std::to_string(bits(y)), {middleX, y, 4});
        }
        query(liquid, "lower_corner", liquid.corner);
        query(liquid, "outside_x", {liquid.corner.x - .01f, middleY, 0});
        query(liquid, "outside_y", {middleX, liquid.corner.y - .01f, 0});
        query(liquid, "negative_x_short_circuits_unsafe_y", {liquid.corner.x - .01f, -1e20f, 0});
        query(liquid, "upper_x_short_circuits_unsafe_y", {liquid.corner.x + float(liquid.width) * tile + 1.f, -1e20f, 0});
        query(liquid, "signed_zero_input", {-0.f, -0.f, -0.f});
        query(liquid, "upper_corner", {liquid.corner.x + float(liquid.width) * tile,
                                       liquid.corner.y + float(liquid.height) * tile, 0});
    }
    void finish() {
        output.flush(); require(bool(output), "write direct original liquid expectations");
        std::printf("Exact pinned WmoLiquid/group oracle: %zu queries, %zu wet hits\n", count, hits);
    }
};

class LiquidOracle {
    VMAP::VMapManager2 manager;
    VMAP::StaticMapTree& tree;
    std::ofstream output;
    size_t count = 0, locations = 0, hits = 0;
public:
    LiquidOracle(const std::filesystem::path& root, VMAP::StaticMapTree& tree) : tree(tree), output(root / "scopedliquid.tsv") {
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
        output << "scopedliquid\t" << name;
        for (float value : {world.x, world.y, world.z, p.x, p.y, p.z}) putFloat(output, value);
        output << '\t' << unsigned(found); putFloat(output, info.ground_Z);
        output << '\t' << (info.hitInstance ? info.hitInstance->ID : 0) << '\t' << info.rootId
               << '\t' << (info.hitModel ? info.hitModel->GetWmoID() : 0)
               << '\t' << (info.hitModel ? info.hitModel->GetMogpFlags() : 0)
               << '\t' << (info.hitInstance ? info.hitInstance->adtId : 0)
               << '\t' << unsigned(mask) << '\t' << unsigned(hit);
        putFloat(output, level); putFloat(output, floor);
        output << '\t' << type << '\t' << unsigned(instanceHit); putFloat(output, instanceLevel);
        output << '\t' << unsigned(found && evaluateInstance) << '\n';
        ++count; locations += found; hits += hit;
    }
    void internal(const std::string& name, const Vector3& p, uint8 mask = 0, bool evaluateInstance = true) {
        world(name, manager.convertPositionToInternalRep(p.x, p.y, p.z), mask, evaluateInstance);
    }
    void local(const std::string& name, const ModelSpawn& spawn, const Vector3& p, uint8 mask = 0, bool evaluateInstance = true) {
        internal(name, place(spawn, p), mask, evaluateInstance);
    }
    void finish() {
        output.flush(); require(bool(output), "write original manager liquid expectations");
        std::printf("Exact pinned wet manager oracle: %zu queries, %zu location hits, %zu wet hits\n", count, locations, hits);
    }
};

int main(int argc, char** argv) {
    try {
        require(argc == 2, "usage: scopedliquid-oracle output-directory");
        require(authored_location_main(argc, argv) == 0, "reconstruct unchanged base and dry-location fixtures");
        const std::filesystem::path root(argv[1]), directory = root / "scopedliquid";
        std::filesystem::create_directory(directory);
        std::vector<std::unique_ptr<Geometry>> geometries;
        std::vector<AuthoredLiquid> liquids;
        auto geometry = [&]() -> Geometry& {
            geometries.emplace_back(std::make_unique<Geometry>());
            geometries.back()->groups.reserve(4);
            return *geometries.back();
        };
        auto attach = [&](Geometry& target, const std::string& name, uint32 type, bool hole = false) {
            liquids.push_back(addLiquid(target, name, 1, 1, {-2, -1, 500}, type,
                                       {2, 6, 10, 3}, {uint8(hole ? 0x1f : 0x08)}));
        };
        auto finishWet = [&](Geometry& target, const std::string& name, uint32 rootId, uint32 flags = 0) {
            for (auto& liquid : liquids) if (liquid.model == name) liquid.group = &target.groups[liquid.groupIndex];
            finishVolume(target, flags, rootId, directory / (name + ".vmo"));
        };
        auto& wet = geometry();
        addLiquidFloor(wet, 0xe2345678u, 0, 0x2008);
        liquids.push_back(addLiquid(wet, "wet.wmo", 3, 2, {-4, -3, 77}, 1,
            {2, 4, 8, -2, 3, 9, 5, 6, -1, 12, 0, 10}, {0x00, 0x0f, 0x10, 0xff, 0x08, 0xf0}));
        finishWet(wet, "wet.wmo", 0xf1234567u);
        auto& dryOccluder = geometry();
        addLiquidFloor(dryOccluder, 502, 0, 8); attach(dryOccluder, "dry-occluder.wmo", 1);
        addLiquidFloor(dryOccluder, 503, 6, 0x2000);
        finishWet(dryOccluder, "dry-occluder.wmo", 52);
        auto& holeOccluder = geometry();
        addLiquidFloor(holeOccluder, 504, 0, 8); attach(holeOccluder, "hole-occluder.wmo", 1);
        addLiquidFloor(holeOccluder, 505, 6, 0x2000); attach(holeOccluder, "hole-occluder.wmo", 2, true);
        finishWet(holeOccluder, "hole-occluder.wmo", 53);
        auto& dry = geometry();
        addLiquidFloor(dry, 506, 6, 0x2000); finishWet(dry, "dry.wmo", 54);
        auto& m2 = geometry();
        addLiquidFloor(m2, 507, 6, 8); attach(m2, "wet.m2", 2);
        finishWet(m2, "wet.m2", 55, VMAP::MOD_M2);
        std::vector<ModelSpawn> spawns;
        std::vector<WorldModel*> models;
        auto addSpawn = [&](Geometry& g, const std::string& name, const Vector3& p,
                            const Vector3& r = Vector3(), float scale = 1.f, bool m2Flag = false) {
            spawns.push_back(spawn(g, uint32(501 + spawns.size()), name, p, r, scale, m2Flag));
            spawns.back().adtId = uint16(0x8100 + spawns.size());
            models.push_back(&g.model);
        };
        addSpawn(wet, "wet.wmo", {0, 0, 0});
        addSpawn(wet, "wet.wmo", {100, 20, 4}, {0, 37, 0});
        addSpawn(wet, "wet.wmo", {200, 5, -3}, {0, 0, 0}, .5f);
        addSpawn(wet, "wet.wmo", {300, 20, 4}, {13, 37, -9}, 1.75f);
        addSpawn(dryOccluder, "dry-occluder.wmo", {400, 0, 0});
        addSpawn(holeOccluder, "hole-occluder.wmo", {500, 0, 0});
        addSpawn(wet, "wet.wmo", {600, 0, 0});
        addSpawn(dry, "dry.wmo", {600, 0, 0});
        addSpawn(m2, "wet.m2", {700, 0, 0}, {}, 1, true);
        addSpawn(wet, "wet.wmo", {800, 0, 0});
        addSpawn(m2, "wet.m2", {800, 0, 0}, {}, 1, true);
        const std::vector<uint32> types = {0, 1, 2, 3, 4, 21, 41, 61, 999, 0x12345678u};
        std::vector<size_t> typeSpawns;
        for (uint32 type : types) {
            auto& g = geometry();
            const auto name = "type-" + std::to_string(type) + ".wmo";
            addLiquidFloor(g, 600 + uint32(typeSpawns.size()), 0, 8); attach(g, name, type);
            finishWet(g, name, 60 + uint32(typeSpawns.size()));
            typeSpawns.push_back(spawns.size());
            addSpawn(g, name, {1000.f + 100.f * float(typeSpawns.size()), 0, 0});
        }
        // Zero-cell liquids retain a payload and type, but every query is false.
        for (const auto dimension : {std::pair<uint32, uint32>(0, 2), {2, 0}, {0, 0}}) {
            auto& g = geometry();
            const auto name = "empty-" + std::to_string(dimension.first) + "x" + std::to_string(dimension.second) + ".wmo";
            addLiquidFloor(g, 701 + uint32(spawns.size()), 0, 8);
            liquids.push_back(addLiquid(g, name, dimension.first, dimension.second, {-2, -1, 0}, 41,
                std::vector<float>((dimension.first + 1) * (dimension.second + 1), 5.f), {}));
            finishWet(g, name, 90 + uint32(spawns.size()));
            addSpawn(g, name, {2100.f + 100.f * float(spawns.size() - 21), 0, 0});
        }
        auto& zeroCorner = geometry();
        addLiquidFloor(zeroCorner, 750, 0, 8);
        liquids.push_back(addLiquid(zeroCorner, "zero-corner.wmo", 1, 1, {0, 0, -999}, 3,
                                   {2, 6, 10, 3}, {0x00}));
        finishWet(zeroCorner, "zero-corner.wmo", 123);
        addSpawn(zeroCorner, "zero-corner.wmo", {2500, 0, 0});
        VMAP::StaticMapTree tree(0, ""); populate(tree, spawns, models); serializeTree(directory, tree, spawns);
        DirectOracle direct(root);
        std::ofstream sizes(root / "scopedliquid_sizes.tsv");
        sizes << "# Original WmoLiquid::GetFileSize versus actual original liquid/group writeToFile bytes\n"
              << "# model\tgroup_index\tgroup_id\ttiles_x\ttiles_y\tcorner_x\tcorner_y\tcorner_z\ttype\tsource_mask\tdeclared_bytes\tactual_bytes\tgroup_bytes\tliqu_header_offset\n";
        for (const auto& liquid : liquids) { auditSize(sizes, liquid); direct.samples(liquid); }
        sizes.flush(); require(bool(sizes), "write original serialization size audit"); direct.finish();
        LiquidOracle oracle(root, tree);
        const float tile = LIQUID_TILE_SIZE;
        for (size_t index : {size_t(0), size_t(1), size_t(2), size_t(3)})
            for (uint32 ty = 0; ty < 2; ++ty) for (uint32 tx = 0; tx < 3; ++tx)
                for (const auto fraction : {std::pair<float, float>(.75f, .25f), {.25f, .75f}, {.5f, .5f}})
                    for (float z : {-1.f, 0.f, 1.f, 4.f, 10.f, 18.f, 19.f})
                        for (uint8 mask : {uint8(0), uint8(1), uint8(2), uint8(15), uint8(255)}) {
                            const auto name = "model" + std::to_string(index) + "_tile" + std::to_string(tx) + "_" +
                                std::to_string(ty) + "_" + std::to_string(bits(fraction.first)) + "_" +
                                std::to_string(bits(fraction.second)) + "_z" + std::to_string(bits(z)) + "_mask" + std::to_string(mask);
                            oracle.local(name, spawns[index], {-4.f + (float(tx) + fraction.first) * tile,
                                                               -3.f + (float(ty) + fraction.second) * tile, z}, mask);
                        }
        for (size_t i = 0; i < types.size(); ++i) for (unsigned mask = 0; mask < 256; ++mask)
            oracle.local("type" + std::to_string(types[i]) + "_mask" + std::to_string(mask),
                         spawns[typeSpawns[i]], {-.5f, .5f, 4}, uint8(mask));
        for (size_t i : {size_t(4), size_t(5), size_t(6), size_t(8), size_t(9), size_t(21), size_t(22), size_t(23)})
            for (float z : {-.101f, -.1f, -.099f, 0.f, 5.f, 5.899f, 5.9f, 5.901f, 6.f, 7.f, 18.f, 19.f})
                for (uint8 mask : {uint8(0), uint8(1), uint8(2), uint8(15)})
                    oracle.local("selection" + std::to_string(i) + "_z" + std::to_string(bits(z)) + "_mask" + std::to_string(mask),
                                 spawns[i], {-.5f, .5f, z}, mask);
        for (float edge : {-4.f, -4.f + tile, -4.f + 2.f * tile, -4.f + 3.f * tile})
            for (float x : {edge - .002f, std::nextafter(edge, -100.f), edge, std::nextafter(edge, 100.f), edge + .002f})
                oracle.internal("world_x_edge_" + std::to_string(bits(x)), {x, -2.f, 4});
        for (float edge : {-3.f, -3.f + tile, -3.f + 2.f * tile})
            for (float y : {edge - .002f, std::nextafter(edge, -100.f), edge, std::nextafter(edge, 100.f), edge + .002f})
                oracle.internal("world_y_edge_" + std::to_string(bits(y)), {-3.f, y, 4});
        oracle.internal("nearest_dry_group_occludes_wet", {399.5f, .5f, 7});
        oracle.internal("wet_group_below_dry_floor", {399.5f, .5f, 5});
        oracle.internal("nearest_hole_group_occludes_wet", {499.5f, .5f, 7});
        oracle.internal("nearest_dry_instance_occludes_wet", {599.5f, .5f, 7});
        oracle.internal("wet_m2_excluded", {699.5f, .5f, 7});
        oracle.internal("wet_m2_does_not_occlude_wmo", {799.5f, .5f, 7});
        oracle.internal("wet_level_below_floor_still_true", {-4.f + 2.9375f * tile, -3.f + .0625f * tile, 4});
        oracle.local("mask_rejects_unsafe_x_before_instance", spawns[typeSpawns[1]], {-8, .5f, 4}, 2, false);
        oracle.local("mask_rejects_unsafe_y_before_instance", spawns[typeSpawns[1]], {-.5f, -8, 4}, 2, false);
        oracle.local("unknown_type_mask_rejects_unsafe_xy", spawns[typeSpawns[8]], {-8, -8, 4}, 255, false);
        oracle.internal("map_gap", {50, 0, 4});
        oracle.internal("outside_map_root", {-500, -500, 4});
        oracle.finish();
        return 0;
    } catch (const std::exception& error) {
        std::fprintf(stderr, "VMap wet liquid oracle failed: %s\n", error.what());
        return 1;
    }
}
