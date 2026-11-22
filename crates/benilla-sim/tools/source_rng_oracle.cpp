// SPDX-License-Identifier: GPL-3.0-or-later
// Authored oracle using the retained GCC 12.2 libstdc++ headers, not Rust code.
#include <cstdint>
#include <iostream>
#include <random>
#include <vector>

int main() {
    static_assert(sizeof(std::mt19937::result_type) == 8);
    static_assert(sizeof(size_t) == 8 && sizeof(unsigned int) == 4);
    const std::vector<std::vector<uint32_t>> seeds = {
        {}, {1}, {1, 2, 3, 4}, {0xffffffffu, 0, 0x80000000u, 42}
    };
    const size_t lengths[] = {0, 1, 2, 6, 7, 38, 39, 67, 68, 622, 623, 624};
    for (size_t s = 0; s < seeds.size(); ++s) {
        std::seed_seq seq(seeds[s].begin(), seeds[s].end());
        for (size_t n : lengths) {
            std::vector<uint32_t> out(n);
            seq.generate(out.begin(), out.end());
            std::cout << "seq " << s << ' ' << n;
            for (auto word : out) std::cout << ' ' << word;
            std::cout << '\n';
        }
    }
    for (uint32_t seed : {5489u, 0u, 1u, 0xffffffffu}) {
        std::mt19937 engine(seed);
        std::cout << "initial " << seed << ' ' << engine << '\n';
        for (size_t i = 0; i < 10000; ++i) {
            auto word = engine();
            if (i < 5 || (i >= 620 && i <= 627) || i == 9999)
                std::cout << "mt " << seed << ' ' << i << ' ' << word << '\n';
        }
    }
    for (size_t s = 0; s < seeds.size(); ++s) {
        std::seed_seq seq(seeds[s].begin(), seeds[s].end());
        std::mt19937 engine(seq);
        std::cout << "seeded " << s << ' ' << engine << '\n';
        for (size_t i = 0; i < 1250; ++i) {
            auto word = engine();
            if (i < 5 || (i >= 620 && i <= 627) || i >= 1247)
                std::cout << "seqmt " << s << ' ' << i << ' ' << word << '\n';
        }
    }
    for (uint32_t arg1 : {1u, 2u, 100u, 0x80000001u, 0xffffffffu}) {
        for (uint32_t arg2 : {0u, 1u, 5489u, 0xffffffffu}) {
            std::mt19937 engine(arg2 ? arg2 : 1);
            int32_t result = arg1 == 1 ? 1 : engine() % arg1 + 1;
            std::cout << "wse " << arg1 << ' ' << arg2 << ' ' << result << '\n';
        }
    }
    for (auto bounds : {std::pair<uint32_t, uint32_t>{0, 100}, {100, 100},
                        {0, 0x40000000u}, {0x80000000u, 0xffffffffu},
                        {0xffffffffu, 0xffffffffu}}) {
        std::mt19937 engine(1);
        std::uniform_int_distribution<int32_t> dist(
            std::min(bounds.first, bounds.second), std::max(bounds.first, bounds.second));
        for (size_t i = 0; i < 10; ++i)
            std::cout << "random " << bounds.first << ' ' << bounds.second
                      << ' ' << i << ' ' << dist(engine) << '\n';
        std::cout << "random_state " << bounds.first << ' ' << bounds.second
                  << ' ' << engine << '\n';
    }
}
