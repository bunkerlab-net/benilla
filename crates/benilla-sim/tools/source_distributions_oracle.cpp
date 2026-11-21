// SPDX-License-Identifier: GPL-3.0-or-later
// Authored oracle against retained GCC 12.2 headers; CMaNGOS Util.h/cpp wrappers
// below follow pin 8ec338a1, GPL-2.0-or-later. See PROVENANCE.md.
#include <algorithm>
#include <array>
#include <bit>
#include <cmath>
#include <cstdint>
#include <iostream>
#include <limits>
#include <numeric>
#include <openssl/sha.h>
#include <random>
#include <sstream>
#include <string>
#include <vector>

std::string sha(const std::string& text) {
    unsigned char digest[SHA256_DIGEST_LENGTH];
    SHA256(reinterpret_cast<const unsigned char*>(text.data()), text.size(), digest);
    const char* hex = "0123456789abcdef";
    std::string out;
    for (auto c : digest) { out += hex[c >> 4]; out += hex[c & 15]; }
    return out;
}
struct MT {
    using result_type = std::mt19937::result_type;
    std::mt19937 inner;
    uint64_t count = 0;
    MT() { std::seed_seq seq{1u, 2u}; inner.seed(seq); }
    static constexpr result_type min() { return 0; }
    static constexpr result_type max() { return 0xffffffffu; }
    result_type operator()() { ++count; return inner(); }
    std::string state() const { std::ostringstream s; s << inner; return s.str(); }
};
constexpr uint64_t WORDS64[] = {
    0, 1, UINT64_MAX, UINT64_MAX - 1, 0x8000000000000000ull,
    0x123456789abcdef0ull, 0x1111111111111111ull, 0xeeeeeeeeeeeeeeeeull,
};
struct Script64 {
    using result_type = uint64_t;
    uint64_t count = 0;
    static constexpr result_type min() { return 0; }
    static constexpr result_type max() { return UINT64_MAX; }
    result_type operator()() { return WORDS64[count++ % std::size(WORDS64)]; }
    std::string state() const { return "script64 " + std::to_string(count); }
};
struct Narrow {
    using result_type = uint64_t;
    uint64_t count = 0;
    static constexpr result_type min() { return 10; }
    static constexpr result_type max() { return 109; }
    result_type operator()() {
        constexpr uint64_t words[] = {109, 108, 10, 11, 59, 88, 45, 12};
        return words[count++ % std::size(words)];
    }
    std::string state() const { return "narrow " + std::to_string(count); }
};
template<class E, class F>
void row(const char* family, E& engine, const char* op, uint64_t a, uint64_t b, F fn) {
    auto begin = engine.count;
    uint64_t out = fn();
    std::cout << "{\"family\":\"" << family << "\",\"op\":\"" << op
              << "\",\"a\":" << a << ",\"b\":" << b << ",\"out\":" << out
              << ",\"begin\":" << begin << ",\"end\":" << engine.count
              << ",\"state\":\"" << sha(engine.state()) << "\"}\n";
}
uint32_t chance_u(float chance) {
    return uint32_t(::roundf(std::max(0.0f, chance) * 100));
}
template<class E> uint32_t urand(E& g, uint32_t a, uint32_t b) {
    return std::uniform_int_distribution<uint32_t>(a, b)(g);
}
template<class E> double rand_chance(E& g) {
    return std::uniform_real_distribution<double>(0, 100.0)(g);
}
template<class E> void shuffle_row(const char* family, E& g, size_t n, bool duplicates) {
    std::vector<uint64_t> values(n);
    for (size_t i = 0; i < n; ++i) values[i] = duplicates ? i % 3 : i;
    auto begin = g.count;
    std::shuffle(values.begin(), values.end(), g);
    std::string bytes;
    for (auto v : values) for (unsigned i = 0; i < 8; ++i) bytes += char(v >> (8 * i));
    std::cout << "{\"family\":\"" << family << "\",\"op\":\"shuffle\",\"a\":"
              << n << ",\"b\":" << duplicates << ",\"out_sha\":\"" << sha(bytes)
              << "\",\"begin\":" << begin << ",\"end\":" << g.count
              << ",\"state\":\"" << sha(g.state()) << "\"}\n";
}
template<class E> void integer_rows(const char* family, E& g) {
    row(family,g,"u32",0,UINT32_MAX,[&]{ return std::uniform_int_distribution<uint32_t>(0,UINT32_MAX)(g); });
    row(family,g,"u32",0,0x80000000u,[&]{ return std::uniform_int_distribution<uint32_t>(0,0x80000000u)(g); });
    row(family,g,"u32",7,7,[&]{ return std::uniform_int_distribution<uint32_t>(7,7)(g); });
    row(family,g,"i32",uint64_t(INT32_MIN),INT32_MAX,[&]{ return std::uniform_int_distribution<int32_t>(INT32_MIN,INT32_MAX)(g); });
    row(family,g,"i32",uint64_t(INT32_MIN+1),INT32_MAX,[&]{ return std::uniform_int_distribution<int32_t>(INT32_MIN+1,INT32_MAX)(g); });
    row(family,g,"u64",0,UINT64_MAX,[&]{ return std::uniform_int_distribution<uint64_t>(0,UINT64_MAX)(g); });
    row(family,g,"u64",0,0x8000000000000000ull,[&]{ return std::uniform_int_distribution<uint64_t>(0,0x8000000000000000ull)(g); });
    row(family,g,"u64",0x100000000ull,0xffffffffffull,[&]{ return std::uniform_int_distribution<uint64_t>(0x100000000ull,0xffffffffffull)(g); });
    row(family,g,"i64",uint64_t(INT64_MIN),INT64_MAX,[&]{ return std::uniform_int_distribution<int64_t>(INT64_MIN,INT64_MAX)(g); });
    row(family,g,"i64",uint64_t(INT64_MIN+7),uint64_t(INT64_MAX-9),[&]{ return std::uniform_int_distribution<int64_t>(INT64_MIN+7,INT64_MAX-9)(g); });
    row(family,g,"i64",uint64_t(-77),uint64_t(-77),[&]{ return std::uniform_int_distribution<int64_t>(-77,-77)(g); });
}
template<class E> void real_rows(const char* family, E& g) {
    row(family,g,"f32",std::bit_cast<uint32_t>(-1.f),std::bit_cast<uint32_t>(1.f),[&]{ return std::bit_cast<uint32_t>(std::uniform_real_distribution<float>(-1,1)(g)); });
    row(family,g,"f64",std::bit_cast<uint64_t>(-1.),std::bit_cast<uint64_t>(100.),[&]{ return std::bit_cast<uint64_t>(std::uniform_real_distribution<double>(-1,100)(g)); });
}
int main() {
    MT g;
    for (unsigned round = 0; round < 100; ++round) {
        integer_rows("mt",g);
        real_rows("mt",g);
        row("mt",g,"irand_default",0,0,[&]{ return std::uniform_int_distribution<int32_t>()(g); });
        row("mt",g,"urand_default",0,0,[&]{ return std::uniform_int_distribution<uint32_t>()(g); });
        row("mt",g,"rand_norm",0,0,[&]{ return std::bit_cast<uint64_t>(std::uniform_real_distribution<double>(0,1)(g)); });
        row("mt",g,"rand_norm_f",0,0,[&]{ return std::bit_cast<uint32_t>(std::uniform_real_distribution<float>(0,1)(g)); });
        row("mt",g,"rand_chance",0,0,[&]{ return std::bit_cast<uint64_t>(rand_chance(g)); });
        row("mt",g,"rand_chance_f",0,0,[&]{ return std::bit_cast<uint32_t>(float(rand_chance(g))); });
        const float chances[] = {0.f, 50.f, 100.f, 100.01f, std::numeric_limits<float>::quiet_NaN(), -0.f};
        float chance = chances[round % std::size(chances)];
        row("mt",g,"roll_chance_f",std::bit_cast<uint32_t>(chance),0,[&]{ return chance > rand_chance(g); });
        int chance_i = int(round % 103) - 1;
        row("mt",g,"roll_chance_i",uint64_t(chance_i),0,[&]{ return chance_i > std::uniform_int_distribution<int32_t>(0,99)(g); });
        const float combat[] = {0.f, 0.0049f, 0.005f, 50.f, 100.f};
        chance = combat[round % std::size(combat)];
        row("mt",g,"combat",std::bit_cast<uint32_t>(chance),0,[&]{ auto u=chance_u(chance); return u && u > urand(g,1,10000); });
        uint32_t count = round % 11 + 1;
        row("mt",g,"pick",count,0,[&]{ return urand(g,0,count-1); });
        shuffle_row("mt",g,round%13,round%2);
    }
    for (auto n : {size_t(65535),size_t(65536),size_t(65537)}) shuffle_row("mt",g,n,false);
    Script64 wide;
    for (unsigned i = 0; i < 10; ++i) { integer_rows("wide",wide); real_rows("wide",wide); shuffle_row("wide",wide,i,false); }
    Narrow narrow;
    for (unsigned i = 0; i < 20; ++i) {
        row("narrow",narrow,"u64",0,70,[&]{ return std::uniform_int_distribution<uint64_t>(0,70)(narrow); });
        row("narrow",narrow,"u64",0,150,[&]{ return std::uniform_int_distribution<uint64_t>(0,150)(narrow); });
        shuffle_row("narrow",narrow,i%8,false);
    }
    for (uint64_t bits : {0ull,0x8000000000000000ull,0x3ff0000010000000ull,0x3ff0000030000000ull,
                          0x3690000000000000ull,0x36a0000000000000ull,0x7ff0000000000000ull,
                          0xfff0000000000000ull,0x7ff8123456789abcull}) {
        auto out = std::bit_cast<uint32_t>(float(std::bit_cast<double>(bits)));
        std::cout << "{\"op\":\"cast\",\"a\":" << bits << ",\"out\":" << out << "}\n";
    }
    for (float chance : {-1.f,-0.f,0.f,0.0049f,0.005f,0.015f,50.f,100.f,
                          30000000.f,std::numeric_limits<float>::quiet_NaN(),-std::numeric_limits<float>::infinity()}) {
        std::cout << "{\"op\":\"chance_u\",\"a\":" << std::bit_cast<uint32_t>(chance)
                  << ",\"out\":" << chance_u(chance) << "}\n";
    }
}
