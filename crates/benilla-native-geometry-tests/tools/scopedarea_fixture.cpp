// SPDX-License-Identifier: GPL-3.0-or-later
// Authored numeric fixtures only. Every raw area result comes from the original
// hash-guarded VMapManager2::getAreaInfo and its unchanged source callees.
#include "authored_liquid.cpp"

struct AreaObservation {
    bool hit;
    float z;
    uint32 flags;
    int32 adt, root, group;
};

static std::ofstream areaOutput;
static std::ofstream traceOutput;
static size_t areaCount = 0, areaHits = 0, metadataSplits = 0, missPreserved = 0;
static constexpr uint32 INITIAL_FLAGS = 0xfedcba98u;
static constexpr int32 INITIAL_ADT = -1234567, INITIAL_ROOT = -2345678, INITIAL_GROUP = -3456789;

static AreaObservation areaQuery(VMAP::StaticMapTree& tree, const std::string& dataset,
                                 const std::string& name, const Vector3& world, uint32 mapId = 0) {
    VMAP::VMapManager2 manager;
    manager.iInstanceMapTrees[0] = &tree;
    AreaObservation result{false, world.z, INITIAL_FLAGS, INITIAL_ADT, INITIAL_ROOT, INITIAL_GROUP};
    result.hit = manager.getAreaInfo(mapId, world.x, world.y, result.z, result.flags,
                                    result.adt, result.root, result.group);
    if (!result.hit) {
        require(bits(result.z) == bits(world.z) && result.flags == INITIAL_FLAGS &&
                result.adt == INITIAL_ADT && result.root == INITIAL_ROOT && result.group == INITIAL_GROUP,
                "raw area miss preserves all caller outputs bit-for-bit");
        ++missPreserved;
    }
    // This is only an independent distinction audit. Location never supplies
    // any raw area expectation, and raw area is invoked before this comparison.
    VMAP::LocationInfo location;
    const auto p = manager.convertPositionToInternalRep(world.x, world.y, world.z);
    const bool located = mapId == 0 && tree.GetLocationInfo(p, location);
    require(located == result.hit, "authored area/location hit agreement");
    if (located) {
        require(bits(result.z) == bits(location.ground_Z) && result.adt == location.hitInstance->adtId,
                "authored area/location floor and ADT agreement");
        metadataSplits += uint32(result.root) != uint32(location.rootId) ||
                          uint32(result.group) != location.hitModel->GetWmoID() ||
                          result.flags != location.hitModel->GetMogpFlags();
    }
    areaOutput << dataset << '\t' << name << '\t' << mapId;
    for (float value : {world.x, world.y, world.z}) putFloat(areaOutput, value);
    areaOutput << '\t' << INITIAL_FLAGS << '\t' << uint32(INITIAL_ADT) << '\t' << uint32(INITIAL_ROOT)
               << '\t' << uint32(INITIAL_GROUP) << '\t' << unsigned(result.hit);
    putFloat(areaOutput, result.z);
    areaOutput << '\t' << result.flags << '\t' << uint32(result.adt) << '\t' << uint32(result.root)
               << '\t' << uint32(result.group) << '\n';
    ++areaCount;
    areaHits += result.hit;
    return result;
}

static void recordArea(VMAP::StaticMapTree& tree, const std::string& dataset,
                       const std::string& name, const Vector3& world) {
    areaQuery(tree, dataset, name, world);
}

static Vector3 worldPoint(const Vector3& p) {
    VMAP::VMapManager2 manager;
    return manager.convertPositionToInternalRep(p.x, p.y, p.z);
}

// The source builder is asked for one valid leaf. Only the authored object's
// permutation changes; all instance-array positions, spawn IDs, tile records,
// bounds, geometry and original query/writer algorithms remain identical.
class AreaLeaf : public BIH {
public:
    AreaLeaf(const std::vector<ModelSpawn>& spawns, bool reverse) {
        auto getBounds = [](const ModelSpawn& value, AABox& result) { result = value.getBounds(); };
        build(spawns, getBounds, uint32(spawns.size()));
        require(tree.size() == 3, "authored area order fixture is a single source-built leaf");
        if (reverse) std::reverse(objects.begin(), objects.end());
    }
};

struct AreaTrace {
    VMAP::StaticMapTree& tree;
    std::vector<uint32> callbacks, successful, adts, roots, groups, flags;
    std::vector<float> floors;
    VMAP::AreaInfo accumulated;
    void operator()(const Vector3& p, uint32 entry) {
        callbacks.push_back(tree.iTreeValues[entry].ID);
        VMAP::AreaInfo isolated;
        tree.iTreeValues[entry].intersectPoint(p, isolated);
        tree.iTreeValues[entry].intersectPoint(p, accumulated);
        if (!isolated.result) return;
        successful.push_back(tree.iTreeValues[entry].ID);
        floors.push_back(isolated.ground_Z);
        adts.push_back(uint32(isolated.adtId));
        roots.push_back(uint32(isolated.rootId));
        groups.push_back(uint32(isolated.groupId));
        flags.push_back(isolated.flags);
    }
};

static void decimalList(std::ostream& output, const std::vector<uint32>& values) {
    if (values.empty()) output << '-';
    for (size_t i = 0; i < values.size(); ++i) {
        if (i) output << ',';
        output << values[i];
    }
}

static AreaObservation traced(VMAP::StaticMapTree& tree, const std::string& dataset,
                              const std::string& name, const Vector3& internal) {
    const Vector3 world = worldPoint(internal), actual = worldPoint(world);
    const auto result = areaQuery(tree, dataset, name, world);
    AreaTrace trace{tree, {}, {}, {}, {}, {}, {}, {}, {}};
    tree.iTree.intersectPoint(actual, trace);
    require(result.hit == trace.accumulated.result, "original instance trace agrees with raw area result");
    if (result.hit) {
        require(bits(result.z) == bits(trace.accumulated.ground_Z) && result.adt == trace.accumulated.adtId &&
                result.root == trace.accumulated.rootId && result.group == trace.accumulated.groupId &&
                result.flags == trace.accumulated.flags, "original instance trace agrees with manager outputs");
    }
    traceOutput << dataset << '\t' << name;
    for (float value : {actual.x, actual.y, actual.z}) putFloat(traceOutput, value);
    for (const auto* values : {&trace.callbacks, &trace.successful}) {
        traceOutput << '\t'; decimalList(traceOutput, *values);
    }
    traceOutput << '\t';
    if (trace.floors.empty()) traceOutput << '-';
    for (size_t i = 0; i < trace.floors.size(); ++i) {
        if (i) traceOutput << ',';
        traceOutput << std::hex << std::setw(8) << std::setfill('0') << bits(trace.floors[i]) << std::dec;
    }
    for (const auto* values : {&trace.adts, &trace.roots, &trace.groups, &trace.flags}) {
        traceOutput << '\t'; decimalList(traceOutput, *values);
    }
    traceOutput << '\n';
    return result;
}

static bool sameArea(const AreaObservation& left, const AreaObservation& right) {
    return left.hit == right.hit && bits(left.z) == bits(right.z) && left.flags == right.flags &&
           left.adt == right.adt && left.root == right.root && left.group == right.group;
}

int main(int argc, char** argv) {
    try {
        require(argc == 2, "usage: scopedarea-oracle output-directory");
        const std::filesystem::path root(argv[1]);
        areaOutput.open(root / "scopedarea.tsv");
        traceOutput.open(root / "scopedarea_trace.tsv");
        areaOutput << "# Original VMapManager2::getAreaInfo; CMaNGOS 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f\n"
                   << "# Raw manager result only; floats binary32 hex; IDs/flags decimal uint32 raw bits\n"
                   << "# kind\tcase\tmap_id\tx\ty\tz\tflags_in\tadt_id_in\troot_id_in\tgroup_id_in\tresult\tz_out\tflags_out\tadt_id_out\troot_id_out\tgroup_id_out\n";
        traceOutput << "# Original BIH::intersectPoint plus isolated/shared ModelInstance::intersectPoint calls\n"
                    << "# Actual source-converted internal coordinates; callback/successful IDs retain traversal order\n"
                    << "# kind\tcase\tix\tiy\tiz\tcallback_spawn_ids\tsuccessful_spawn_ids\tisolated_floor_bits\tisolated_adt_ids\tisolated_root_ids\tisolated_group_ids\tisolated_flags\n";
        require(authored_liquid_main(argc, argv) == 0, "reconstruct unchanged base, dry and wet fixtures");

        Geometry high, low, equal, multi, wet, m2;
        addVolumeFloor(high, 0xe1111111u, 6, 0x80002008u);
        addVolumeFloor(low, 0xe2222222u, 2, 0x40002000u);
        addVolumeFloor(equal, 0xe3333333u, 6, 0xa0000008u);
        for (uint32 i = 0; i < 4; ++i)
            addVolumeFloor(multi, 0xe4444440u + i, i < 3 ? float(3 * i) : 6.f,
                           i == 3 ? 0xc0008000u : 0xc0002008u + i);
        multi.groups.emplace_back(0xffffffffu, 0xe4444444u,
                                  AABox(Vector3(-4, -4, -1), Vector3(4, 4, 10)));
        addVolumeFloor(wet, 0xe1111111u, 6, 0x80002008u);
        addLiquid(wet, "wet.wmo", 1, 1, {-2, -2, 123}, 1, {8, 9, 10, 11}, {0x00});
        addVolumeFloor(m2, 0xe5555555u, 7, 0xffffffffu);
        const auto forwardDirectory = root / "scopedarea/forward";
        std::filesystem::create_directories(forwardDirectory);
        finishVolume(high, 0, 0xf1111111u, forwardDirectory / "high.wmo.vmo");
        finishVolume(low, 0, 0xf2222222u, forwardDirectory / "low.wmo.vmo");
        finishVolume(equal, 0, 0xf3333333u, forwardDirectory / "equal.wmo.vmo");
        finishVolume(multi, 0, 0xf4444444u, forwardDirectory / "multi.wmo.vmo");
        finishVolume(wet, 0, 0xf1111111u, forwardDirectory / "wet.wmo.vmo");
        finishVolume(m2, VMAP::MOD_M2, 0xf5555555u, forwardDirectory / "high.m2.vmo");
        std::vector<ModelSpawn> spawns;
        std::vector<WorldModel*> models;
        auto add = [&](Geometry& g, const std::string& name, const Vector3& p,
                       const Vector3& r = Vector3(), float scale = 1.f, bool isM2 = false) {
            spawns.push_back(spawn(g, 900 + uint32(spawns.size()), name, p, r, scale, isM2));
            spawns.back().adtId = uint16(0x8100 + spawns.size());
            models.push_back(&g.model);
        };
        add(high, "high.wmo", {0, 0, 0});
        add(low, "low.wmo", {0, 0, 0});
        add(high, "high.wmo", {30, 0, 0});
        add(equal, "equal.wmo", {30, 0, 0});
        add(multi, "multi.wmo", {60, 0, 0});
        add(multi, "multi.wmo", {100, 20, 4}, {13, 37, -9}, 1.75f);
        add(multi, "multi.wmo", {150, 5, -3}, {}, .5f);
        add(high, "high.wmo", {200, 0, 0});
        add(wet, "wet.wmo", {230, 0, 0});
        // Dry and wet variants differ only by LIQU payload and placement X.
        spawns.back().adtId = spawns[7].adtId;
        add(m2, "high.m2", {260, 0, 0}, {}, 1, true);
        add(high, "high.wmo", {300, 0, 0});
        add(m2, "high.m2", {300, 0, 1}, {}, 1, true);

        size_t dryWetPairs = 0;
        AreaObservation highLow[2], equalFloor[2];
        for (unsigned reverse = 0; reverse < 2; ++reverse) {
            const std::string dataset = reverse ? "scopedarea/reverse" : "scopedarea/forward";
            const auto directory = root / dataset;
            std::filesystem::create_directories(directory);
            if (reverse) for (const auto& entry : std::filesystem::directory_iterator(forwardDirectory))
                if (entry.path().extension() == ".vmo")
                    std::filesystem::copy_file(entry.path(), directory / entry.path().filename());
            VMAP::StaticMapTree tree(0, "");
            populate(tree, spawns, models);
            tree.iTree = AreaLeaf(spawns, bool(reverse));
            serializeTree(directory, tree, spawns);
            for (size_t index : {size_t(0), size_t(2), size_t(4), size_t(5), size_t(6),
                                 size_t(7), size_t(8), size_t(9), size_t(10)})
                for (float y : {-4.f, -2.f, 0.f, 2.f, 4.f})
                    for (float x : {-4.f, -2.f, 0.f, 2.f, 4.f})
                        for (float z : {-1.f, -.101f, -.1f, -.099f, -0.f, 0.f, 2.f, 3.f, 5.f,
                                        5.899f, 5.9f, 5.901f, 6.f, 7.f, 10.f, 11.f}) {
                            const auto name = "model" + std::to_string(index) + "_" +
                                std::to_string(bits(x)) + "_" + std::to_string(bits(y)) + "_" + std::to_string(bits(z));
                            areaQuery(tree, dataset, name, worldPoint(place(spawns[index], {x, y, z})));
                        }
            highLow[reverse] = traced(tree, dataset, "higher_first_lower_last", {0, 0, 7});
            equalFloor[reverse] = traced(tree, dataset, "equal_floor_different_metadata", {30, 0, 7});
            traced(tree, dataset, "multiple_group_equal_floor", {60, 0, 7});
            traced(tree, dataset, "m2_alone_rejected", {260, 0, 8});
            traced(tree, dataset, "m2_above_wmo_ignored", {300, 0, 9});
            traced(tree, dataset, "below_all_floors", {0, 0, -0.f});
            traced(tree, dataset, "outside_root_signed_zero", {-500, -500, -0.f});
            traced(tree, dataset, "inside_root_instance_gap", {15, 0, 7});
            traced(tree, dataset, "above_all_bounds", {0, 0, 11});
            for (float z : {-0.f, 0.f, 7.f})
                areaQuery(tree, dataset, "unloaded_map_" + std::to_string(bits(z)), worldPoint({0, 0, z}), 1);
            for (float z : {5.899f, 5.9f, 5.901f, 6.f, 7.f, 10.f, 11.f}) {
                const auto dry = areaQuery(tree, dataset, "dry_parity_" + std::to_string(bits(z)), worldPoint({200, 0, z}));
                const auto wetResult = areaQuery(tree, dataset, "wet_parity_" + std::to_string(bits(z)), worldPoint({230, 0, z}));
                require(sameArea(dry, wetResult), "raw area ignores liquid payload including liquid presence");
                ++dryWetPairs;
            }
        }
        require(highLow[0].hit && highLow[1].hit && bits(highLow[0].z) == bits(highLow[1].z) &&
                highLow[0].adt == highLow[1].adt && highLow[0].root != highLow[1].root &&
                highLow[0].group != highLow[1].group && highLow[0].flags != highLow[1].flags,
                "reversing map callbacks changes metadata but preserves unique highest floor/ADT");
        require(equalFloor[0].hit && equalFloor[1].hit && bits(equalFloor[0].z) == bits(equalFloor[1].z) &&
                equalFloor[0].adt != equalFloor[1].adt && equalFloor[0].root != equalFloor[1].root &&
                equalFloor[0].group != equalFloor[1].group && equalFloor[0].flags != equalFloor[1].flags,
                "equal-height source floor/ADT is first while metadata is last");
        require(metadataSplits > 0 && missPreserved > 0, "source distinction and preservation cases exercised");
        areaOutput.flush(); traceOutput.flush();
        require(bool(areaOutput) && bool(traceOutput), "write raw area expectations and original traversal traces");
        std::printf("Exact pinned raw area oracle: %zu queries, %zu hits, %zu misses preserve every output, "
                    "%zu metadata/floor provenance splits, %zu dry/wet parity pairs\n",
                    areaCount, areaHits, missPreserved, metadataSplits, dryWetPairs);
        return 0;
    } catch (const std::exception& error) {
        std::fprintf(stderr, "VMap raw area oracle failed: %s\n", error.what());
        return 1;
    }
}
