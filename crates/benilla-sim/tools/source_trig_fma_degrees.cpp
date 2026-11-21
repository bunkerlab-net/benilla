// SPDX-License-Identifier: GPL-3.0-or-later
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <dlfcn.h>
#include <inttypes.h>
#include <initializer_list>
#include <xmmintrin.h>

int main(int argc, char**) {
    if ((_mm_getcsr() & 0xe040) != 0) return 2;
    auto sine = reinterpret_cast<float(*)(float)>(dlsym(RTLD_DEFAULT, "sinf"));
    auto cosine = reinterpret_cast<float(*)(float)>(dlsym(RTLD_DEFAULT, "cosf"));
    if (!sine || !cosine) return 3;
    Dl_info info;
    dladdr(reinterpret_cast<void*>(sine), &info);
    auto so = reinterpret_cast<uintptr_t>(sine) - reinterpret_cast<uintptr_t>(info.dli_fbase);
    dladdr(reinterpret_cast<void*>(cosine), &info);
    auto co = reinterpret_cast<uintptr_t>(cosine) - reinterpret_cast<uintptr_t>(info.dli_fbase);
    const bool grid = argc == 2;
    uint64_t count = 0;
    uint32_t buffer[196608];
    size_t used = 0;
    auto emit = [&](uint32_t bits) {
        float degrees;
        std::memcpy(&degrees, &bits, 4);
        // ModelInstance.cpp:31, including G3D's nonstandard binary64 PI.
        float radians = static_cast<float>(0x1.921fb54442d28p1 * static_cast<double>(degrees) / 180.0);
        float s = sine(radians), c = cosine(radians);
        uint32_t rb, sb, cb;
        std::memcpy(&rb, &radians, 4);
        std::memcpy(&sb, &s, 4);
        std::memcpy(&cb, &c, 4);
        if (grid) {
            buffer[used++] = bits; buffer[used++] = sb; buffer[used++] = cb;
            if (used == 196608) {
                if (std::fwrite(buffer, 4, used, stdout) != used) return false;
                used = 0;
            }
        } else {
            std::printf("%08x\t%08x\t%08x\t%08x\n", bits, rb, sb, cb);
        }
        ++count;
        return true;
    };
    if (grid) {
        for (uint32_t bits = 0; bits <= 0x44e10000; bits += 4093)
            for (uint32_t sign : {0u, 0x80000000u}) if (!emit(bits | sign)) return 4;
        for (uint32_t step = 0; step <= 1800 * 1024; ++step) {
            float degrees = static_cast<float>(step) / 1024.0f;
            uint32_t bits;
            std::memcpy(&bits, &degrees, 4);
            for (uint32_t sign : {0u, 0x80000000u}) if (!emit(bits | sign)) return 4;
        }
    } else {
        uint32_t bits;
        while (std::scanf("%" SCNx32, &bits) == 1) if (!emit(bits)) return 4;
    }
    if (used && std::fwrite(buffer, 4, used, stdout) != used) return 4;
    std::fprintf(stderr, "{\"count\":%" PRIu64 ",\"sinf_offset\":\"0x%lx\",\"cosf_offset\":\"0x%lx\"}\n", count, so, co);
    return std::ferror(stdout) ? 5 : 0;
}
