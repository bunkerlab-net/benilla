// SPDX-License-Identifier: GPL-3.0-or-later
#include <cfenv>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <dlfcn.h>
#include <xmmintrin.h>
int main() {
    if (std::fegetround() != FE_TONEAREST || (_mm_getcsr() & ((1u << 15) | (1u << 6)))) return 2;
    void* library = dlopen("libm.so.6", RTLD_NOW);
    if (!library) return 3;
    auto function = reinterpret_cast<double (*)(double)>(dlsym(library, "sin"));
    Dl_info info{};
    if (!function || !dladdr(reinterpret_cast<void*>(function), &info)) return 4;
    std::fprintf(stderr, "sin %s %lx\n", info.dli_fname,
        uintptr_t(function) - uintptr_t(info.dli_fbase));
    unsigned long long word;
    while (std::scanf("%llx", &word) == 1) {
        uint64_t input = word, output;
        double value;
        std::memcpy(&value, &input, 8);
        value = function(value);
        std::memcpy(&output, &value, 8);
        std::printf("%016llx %016llx\n", word, static_cast<unsigned long long>(output));
    }
}
