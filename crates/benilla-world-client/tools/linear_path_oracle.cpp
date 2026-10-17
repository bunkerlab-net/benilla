// SPDX-License-Identifier: GPL-3.0-or-later
// Source-expression probe for CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f.
// Copyright CMaNGOS contributors; GPL-2.0-or-later source, GPL-3.0-or-later probe.
// packet_builder.cpp::WriteLinearPath, ByteBuffer.h::appendPackXYZ,
// G3D/Vector3.h::squaredMagnitude; original synthetic coordinates only.
// Regenerate from the repository root:
// c++ -std=c++17 -O2 -ffp-contract=off -fno-fast-math crates/benilla-world-client/tools/linear_path_oracle.cpp -o /tmp/benilla-linear-path-oracle
// /tmp/benilla-linear-path-oracle > crates/benilla-world-client/tools/native_linear_path_cmangos.tsv
#include <array>
#include <cstdint>
#include <cstring>
#include <iostream>
#include <vector>
using P = std::array<float,3>;
uint32_t bits(float value) { uint32_t out; std::memcpy(&out,&value,4); return out; }
float value(uint32_t bits) { float out; std::memcpy(&out,&bits,4); return out; }
uint32_t pack(P offset) {
    // Unsigned shifts retain the source's masked word without signed-shift UB.
    return (uint32_t(int(offset[0]/0.25f)) & 0x7ff)
        | ((uint32_t(int(offset[1]/0.25f)) & 0x7ff) << 11)
        | ((uint32_t(int(offset[2]/0.25f)) & 0x3ff) << 22);
}
void print_point(P p) { for(float v:p) std::cout << ' ' << bits(v); }
void emit(const std::vector<P>& path) {
    P end=path.back(); std::vector<uint32_t> words; std::vector<P> decoded={path.front()};
    for(size_t i=1;i+1<path.size();++i) {
        P offset={end[0]-path[i][0],end[1]-path[i][1],end[2]-path[i][2]};
        if(offset[0]*offset[0]+offset[1]*offset[1]+offset[2]*offset[2] < 0.5f) continue;
        uint32_t word=pack(offset); words.push_back(word);
        int32_t x=int32_t(word<<21)>>21, y=int32_t(word<<10)>>21, z=int32_t(word)>>22;
        decoded.push_back({end[0]-float(x)*0.25f,end[1]-float(y)*0.25f,end[2]-float(z)*0.25f});
    }
    decoded.push_back(end);
    std::cout << "P " << path.size(); for(P p:path) print_point(p);
    std::cout << ' ' << words.size(); for(uint32_t w:words) std::cout << ' ' << w;
    std::cout << ' ' << decoded.size(); for(P p:decoded) print_point(p);
    std::cout << '\n';
}
int main() {
    emit({{10,20,30},{-1,-2,-3}});
    emit({{8,8,8},{0,0,0},{0.5f,0,0},{0.5f,0.5f,0},{0,0,0.75f},{0,0,0}});
    emit({{8,8,8},{value(0x3effffff),0.5f,0},{0.5f,0.5f,0},{value(0x3f000001),0.5f,0},{0,0,0}});
    emit({{8,8,8},{value(0x3f3504f2),0,0},{value(0x3f3504f3),0,0},{value(0x3f3504f4),0,0},{0,0,0}});
    emit({{10,20,30},{-0.9f,1.1f,-0.9f},{0.125f,-0.125f,1.0f}});
    emit({{-32760,-32760,-32760},{-32759.875f,32759.875f,-32000.125f},{32760,-32760,32760}});
    emit({{32760,32760,32760},{32759.875f,-32759.875f,32000.125f},{-32760,32760,-32760}});
    emit({{8,8,8},{value(0x00800000),-value(0x00800000),0},{0,0,0}});
    for(float sign:{-1.0f,1.0f}) for(int axis=0;axis<3;++axis)
        for(float magnitude:{0.74f,0.75f,0.76f,0.99f,1.01f,1.99f,127.75f,128.0f,128.25f,255.75f,256.0f,256.25f,512.25f,32760.0f}) {
            P mid={0,0,0}; mid[axis]=sign*magnitude;
            emit({{10,20,30},mid,{0,0,0}});
        }
}
