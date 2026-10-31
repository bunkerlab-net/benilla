// SPDX-License-Identifier: GPL-3.0-or-later
// Authored numeric geometry. Original pinned VMap height methods are supplied by
// scopedheight_generate.py; the existing source geometry harness supplies writers.
#define main authored_ray_main
#include "authored_vmap.cpp"
#undef main

class HeightOracle {
    VMAP::VMapManager2 manager;
    std::ofstream output;
    size_t count = 0;
public:
    HeightOracle(VMAP::StaticMapTree& tree, const std::filesystem::path& root)
        : output(root / "scopedheight.tsv") {
        manager.iInstanceMapTrees.emplace(0, &tree);
        output << "# Original StaticMapTree::getHeight and VMapManager2::getHeight; CMaNGOS "
                  "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f\n"
               << "# Authored VMap geometry; enabled loaded manager; binary32 hexadecimal bits\n"
               << "# case\twx\twy\twz\tsigned_search\tix\tiy\tiz\theight\n";
    }
    void world(const std::string& name, const Vector3& world, float search) {
        const auto internal = manager.convertPositionToInternalRep(world.x, world.y, world.z);
        const auto height = manager.getHeight(0, world.x, world.y, world.z, search);
        output << name;
        for (float value : {world.x, world.y, world.z, search, internal.x, internal.y, internal.z, height})
            output << '\t' << std::hex << std::setw(8) << std::setfill('0') << bits(value);
        output << '\n';
        ++count;
    }
    void internal(const std::string& name, const Vector3& position, float search) {
        // Author world inputs via the source conversion, then convert again within
        // getHeight. The binary32 world/internal roundtrip is not assumed lossless.
        world(name, manager.convertPositionToInternalRep(position.x, position.y, position.z), search);
    }
    size_t finish() {
        output.flush();
        require(bool(output), "write source height expectations");
        return count;
    }
};

int main(int argc, char** argv) {
    try {
        require(argc == 2, "usage: scopedheight-oracle output-directory");
        require(std::fesetround(FE_TONEAREST) == 0, "round-to-nearest");
        static_assert(sizeof(float) == 4 && std::numeric_limits<float>::is_iec559);
        const std::filesystem::path root(argv[1]);
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
        std::vector<WorldModel*> models = {&ramp.model, &stack.model, &wall.model, &ramp.model, &m2.model};
        VMAP::StaticMapTree tree(0, "");
        tree.iIsTiled = true;
        tree.iNTreeValues = uint32(spawns.size());
        tree.iTreeValues = new ModelInstance[spawns.size()];
        for (size_t i = 0; i < spawns.size(); ++i)
            tree.iTreeValues[i] = ModelInstance(spawns[i], models[i]);
        auto bounds = [](const ModelSpawn& value, AABox& result) { result = value.getBounds(); };
        tree.iTree.build(spawns, bounds);
        serializeTree(root, tree, spawns);
        HeightOracle oracle(tree, root);
        for (size_t index : {size_t(0), size_t(1), size_t(3), size_t(4)}) {
            for (int y = -5; y <= 5; ++y) for (int x = -5; x <= 5; ++x) {
                const auto p = place(spawns[index], {float(x), float(y), 0});
                const std::string name = "model" + std::to_string(index) + "_" +
                    std::to_string(x) + "_" + std::to_string(y);
                oracle.internal(name + "_down", {p.x, p.y, 25}, 50);
                oracle.internal(name + "_up", {p.x, p.y, -10}, -50);
            }
        }
        for (float search : {0.f, -0.f, std::numeric_limits<float>::denorm_min(),
                             -std::numeric_limits<float>::denorm_min(),
                             std::nextafter(10.f, 0.f), 10.f, std::nextafter(10.f, 20.f),
                             4096.f, 5000.f, 10000.f}) {
            const auto name = std::to_string(bits(search));
            oracle.internal("ramp_limit_down_" + name, {0, 0, 10}, search);
            oracle.internal("ramp_limit_up_" + name, {0, 0, -10}, -search);
        }
        oracle.internal("ramp_at_origin_down", {0, 0, 0}, 20);
        oracle.internal("ramp_at_origin_up", {0, 0, 0}, -20);
        oracle.internal("stack_at_floor_down", {20, 0, 6}, 20);
        oracle.internal("stack_at_floor_up", {20, 0, 6}, -20);
        oracle.internal("stack_between_down", {20, 0, 7}, 20);
        oracle.internal("stack_between_up", {20, 0, 7}, -20);
        oracle.internal("stack_endpoint_down", {20, 0, 7}, 1);
        oracle.internal("stack_endpoint_up", {20, 0, 7}, -2);
        oracle.internal("m2_included_down", {100, -15, 10}, 20);
        oracle.internal("m2_included_up", {100, -15, -10}, -20);
        oracle.internal("long_down_10000", {20, 0, 9000}, 10000);
        oracle.internal("long_up_10000", {20, 0, -9000}, -10000);
        oracle.internal("long_strict_endpoint", {0, 0, 10000}, 10000);
        oracle.internal("parallel_vertical_wall", {40, 0, 10}, 20);
        for (float x : {-3.999f, -.0001f, .125f, 4.001f})
            for (float y : {-3.999f, -.0001f, .125f, 4.001f}) {
                const auto name = std::to_string(bits(x)) + "_" + std::to_string(bits(y));
                oracle.internal("rounded_world_" + name, {x, y, 10}, 20);
            }
        std::printf("Exact pinned VMap height oracle: %zu queries, unchanged authored products\n", oracle.finish());
        return 0;
    } catch (const std::exception& error) {
        std::fprintf(stderr, "VMap height oracle failed: %s\n", error.what());
        return 1;
    }
}
