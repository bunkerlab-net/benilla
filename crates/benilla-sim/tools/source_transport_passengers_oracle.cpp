// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS 8ec338a1 Transports.cpp:567-589, MapManager.h:121-132.
#include <cmath>
#include <cstdint>
#include <cstring>
#include <cstdio>
#include <dlfcn.h>
#include <type_traits>
static_assert(std::is_same_v<decltype(std::tan(float())), float>);
static float f(uint32_t u) { float x; std::memcpy(&x,&u,4); return x; }
static uint32_t u(float x) { uint32_t v; std::memcpy(&v,&x,4); return v; }
static float norm(float o) { if(o<0) return -std::fmod(-o,2.0f*float(M_PI))+2.0f*float(M_PI); return std::fmod(o,2.0f*float(M_PI)); }
int main(int argc, char** argv) {
    if(argc==3) { if(!std::freopen(argv[1],"r",stdin) || !std::freopen(argv[2],"w",stdout)) return 2; }
    void* p=dlsym(RTLD_DEFAULT,"tanf"); Dl_info d{}; dladdr(p,&d);
    std::fprintf(stderr,"tanf %s offset=%lx symbol=%s\n",d.dli_fname,(unsigned long)p-(unsigned long)d.dli_fbase,d.dli_sname?d.dli_sname:"none");
    uint32_t angle;
    while(std::scanf("%x",&angle)==1) {
        float o=f(angle), x=7.25f,y=-3.75f,z=1.125f, a=norm(-0.25f-o);
        float px=100.5f+x*std::cos(o)-y*std::sin(o);
        float py=-200.25f+y*std::cos(o)+x*std::sin(o);
        float ox=x,oy=y;
        y=(oy-ox*std::tan(o))/(std::cos(o)+std::sin(o)*std::tan(o));
        x=(ox+oy*std::tan(o))/(std::cos(o)+std::sin(o)*std::tan(o));
        std::printf("%08x %08x %08x %08x %08x %08x %08x %08x\n",angle,u(std::tan(o)),u(px),u(py),u(z+10.0f),u(x),u(y),u(a));
    }
}
