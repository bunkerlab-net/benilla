// SPDX-License-Identifier: GPL-3.0-or-later
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <dlfcn.h>
#include <inttypes.h>
#include <xmmintrin.h>

int main(int argc, char** argv) {
    if ((_mm_getcsr() & 0xe040) != 0) return 2;
    auto handle = dlopen("/evidence/libport.so", RTLD_NOW);
    if (!handle) { std::fprintf(stderr, "%s\n", dlerror()); return 3; }
    auto port = reinterpret_cast<uint64_t(*)(uint32_t)>(dlsym(handle, "port"));
    auto sine = reinterpret_cast<float(*)(float)>(dlsym(RTLD_DEFAULT, "sinf"));
    auto cosine = reinterpret_cast<float(*)(float)>(dlsym(RTLD_DEFAULT, "cosf"));
    if (!port || !sine || !cosine) return 4;
    Dl_info info;
    dladdr(reinterpret_cast<void*>(sine), &info);
    uintptr_t so = reinterpret_cast<uintptr_t>(sine) - reinterpret_cast<uintptr_t>(info.dli_fbase);
    dladdr(reinterpret_cast<void*>(cosine), &info);
    uintptr_t co = reinterpret_cast<uintptr_t>(cosine) - reinterpret_cast<uintptr_t>(info.dli_fbase);
    uint64_t count = 0, ds = 0, dc = 0;
    const bool exhaustive = argc == 2 && std::strcmp(argv[1], "--exhaustive") == 0;
    uint32_t buffer[262144];
    size_t used = 0;
    uint32_t input;
    while (exhaustive ? count < UINT64_C(4294967296) : std::scanf("%" SCNx32, &input) == 1) {
        uint32_t bits = exhaustive ? static_cast<uint32_t>(count) : input;
        float x, s, c;
        std::memcpy(&x, &bits, 4);
        s = sine(x); c = cosine(x);
        uint32_t sb, cb;
        std::memcpy(&sb, &s, 4); std::memcpy(&cb, &c, 4);
        uint64_t p = port(bits);
        uint32_t ps = static_cast<uint32_t>(p), pc = static_cast<uint32_t>(p >> 32);
        if (sb != ps) {
            if (ds < 16) std::fprintf(stderr, "sinf %08x %08x %08x\n", bits, sb, ps);
            ++ds;
        }
        if (cb != pc) {
            if (dc < 16) std::fprintf(stderr, "cosf %08x %08x %08x\n", bits, cb, pc);
            ++dc;
        }
        if (exhaustive) {
            buffer[used++] = sb; buffer[used++] = cb;
            buffer[used++] = ps; buffer[used++] = pc;
            if (used == 262144) {
                if (std::fwrite(buffer, 4, used, stdout) != used) return 5;
                used = 0;
            }
        } else {
            std::printf("%08x %08x %08x\n", bits, sb, cb);
        }
        ++count;
    }
    if (used && std::fwrite(buffer, 4, used, stdout) != used) return 5;
    std::fprintf(stderr, "{\"count\":%" PRIu64 ",\"sin_differences\":%" PRIu64 ",\"cos_differences\":%" PRIu64 ",\"sinf_offset\":\"0x%lx\",\"cosf_offset\":\"0x%lx\"}\n", count, ds, dc, so, co);
    return std::ferror(stdout) ? 6 : 0;
}
