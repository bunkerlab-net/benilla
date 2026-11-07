// SPDX-License-Identifier: GPL-3.0-or-later
// Runs original ModelSpawn/ModelInstance/WorldModel readers over private products.
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <filesystem>
#include <iostream>
#include <map>
#include <memory>
#include <stdexcept>
#include <string>
#include <utility>
#include "ModelInstance.h"
#include "WorldModel.h"
#include "MapTree.h"

static void require(bool value) {
    if (!value) throw std::runtime_error("captured original-source area input refused");
}
static uint32 word(FILE* file) {
    uint32 value;
    require(std::fread(&value, 4, 1, file) == 1);
    return value;
}
static std::string text(FILE* file) {
    const auto size = word(file);
    require(size <= 500);
    std::string value(size, '\0');
    require(std::fread(value.data(), 1, size, file) == size);
    return value;
}
static uint32 bits(float value) {
    uint32 result;
    std::memcpy(&result, &value, 4);
    return result;
}
int main(int argc, char** argv) {
    try {
        require(argc == 5);
        const std::filesystem::path root(argv[1]);
        const G3D::Vector3 world(std::stof(argv[2]), std::stof(argv[3]), std::stof(argv[4]));
        const float mid = 0.5f * 64.0f * 533.33333333f;
        const G3D::Vector3 point(mid - world.x, mid - world.y, world.z);
        std::map<std::string, std::string> names;
        FILE* index = std::fopen((root / "models.index").c_str(), "rb");
        require(index);
        const auto models = word(index);
        for (uint32 i = 0; i < models; ++i) {
            auto name = text(index);
            auto file = text(index);
            require(names.emplace(std::move(name), std::move(file)).second);
        }
        require(std::fgetc(index) == EOF);
        std::fclose(index);
        std::map<uint32, VMAP::ModelSpawn> spawns;
        for (const auto& entry : std::filesystem::directory_iterator(root)) {
            if (entry.path().extension() != ".vmtile") continue;
            FILE* file = std::fopen(entry.path().c_str(), "rb");
            require(file);
            char magic[8];
            require(std::fread(magic, 1, 8, file) == 8 && std::memcmp(magic, "VMAP_7.0", 8) == 0);
            const auto count = word(file);
            for (uint32 i = 0; i < count; ++i) {
                VMAP::ModelSpawn spawn;
                require(VMAP::ModelSpawn::readFromFile(file, spawn));
                const auto slot = word(file);
                spawns.try_emplace(slot, std::move(spawn));
            }
            require(std::fgetc(file) == EOF);
            std::fclose(file);
        }
        VMAP::AreaInfo info;
        uint32 candidates = 0;
        std::map<std::string, std::unique_ptr<VMAP::WorldModel>> loaded;
        for (const auto& entry : spawns) {
            const auto& spawn = entry.second;
            if ((spawn.flags & VMAP::MOD_M2) || !spawn.iBound.contains(point)) continue;
            ++candidates;
            auto& model = loaded[spawn.name];
            if (!model) {
                model = std::make_unique<VMAP::WorldModel>();
                require(model->readFile((root / names.at(spawn.name)).string()));
            }
            VMAP::ModelInstance instance(spawn, model.get());
            instance.intersectPoint(point, info);
        }
        std::cout << "{\"position_bits\":[" << bits(world.x) << ',' << bits(world.y) << ',' << bits(world.z)
                  << "],\"raw_hit\":" << (info.result ? "true" : "false")
                  << ",\"wmo_candidates\":" << candidates << ",\"slots\":" << spawns.size() << "}\n";
        return 0;
    } catch (const std::exception& error) {
        std::cerr << error.what() << '\n';
        return 1;
    }
}
