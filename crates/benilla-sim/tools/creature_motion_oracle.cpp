// SPDX-License-Identifier: GPL-3.0-or-later
// Source-expression probe for CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f.
// Copyright CMaNGOS Project contributors; GPL-2.0-or-later source, GPL-3.0-or-later probe.
// MoveSpline.cpp:31-59,92-101,210-280,304-308; spline.cpp:117-120,155-159;
// TargetedMovementGenerator.cpp:572-573; Creature.cpp:2988-2990; Unit.h:1331.
// Unsupported spline branches are not emulated. Synthetic coordinates only.
#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <iostream>
#include <vector>
uint32_t bits(float value) { uint32_t out; std::memcpy(&out,&value,4); return out; }
float value(uint32_t bits) { float out; std::memcpy(&out,&bits,4); return out; }
struct P { float x,y,z; };
float length(P a, P b) { float x=a.x-b.x,y=a.y-b.y,z=a.z-b.z; return ::sqrtf(x*x+y*y+z*z); }
P point(P a,P b,float u) { return {a.x+(b.x-a.x)*u,a.y+(b.y-a.y)*u,a.z+(b.z-a.z)*u}; }
int main() {
  const uint32_t healths[]={1,29,30,31,299,300,301,16777215,16777216,16777217,4294967295u};
  const uint32_t maxima[]={100,1000,55924051,4294967295u};
  const uint32_t speeds[]={0x40200000,0x40e00000,0x40e00001,0x40dfffff,0x3f800001};
  for(auto h:healths) for(auto m:maxima) if(h<=m) for(auto b:speeds) for(int walk=0;walk<2;walk++) for(int no=0;no<2;no++) {
    float hp=(h*100.0f)/m;
    float speed=value(b);
    if(!no && hp<30.f && !walk) {
      float slowed=1.f-((30.f-std::min(hp,30.f))*1.67)/100;
      speed*=slowed;
    }
    std::cout<<"S "<<b<<' '<<walk<<' '<<h<<' '<<m<<' '<<no<<' '<<bits(speed)<<'\n';
  }
  const std::vector<std::vector<P>> paths={
    {{0,0,0},{7,0,0}}, {{0,0,0},{3,4,0},{3,4,5},{-1,2,8}},
    {{-22.125f,103.75f,0.25f},{-14.13f,103.753f,0.27f},{-12.1f,105.1f,0.3f}},
    {{0,0,0},{0,0,0},{1,0,0},{1,0,0},{2,0,0}},
    {{1,1,1},{1,1,1}}, {{32000,-32000,30000},{31999.99f,-31999.9f,29999.3f}},
    {{0,0,0},{0.00001f,0,0},{0.00002f,0,0},{0.00003f,0,0}}
  };
  for(size_t k=0;k<paths.size();++k) for(auto b:speeds) {
    auto& path=paths[k];
    std::vector<int32_t> times={0};
    int32_t time=1;
    float velocityInv=1000.f/value(b);
    for(size_t i=0;i+1<path.size();++i) {
      time+=(length(path[i],path[i+1])*velocityInv);
      times.push_back(time);
    }
    for(uint32_t elapsed: {0u,1u,2u,10u,100u,250u,399u,400u,500u,1000u,2000u,10000u,60000u}) {
      uint32_t remaining=elapsed; int32_t passed=0; size_t index=0; bool done=false;
      do {
        uint32_t step=std::min(remaining,uint32_t(times[index+1]-passed));
        passed+=step; remaining-=step;
        if(passed>=times[index+1]) {
          ++index;
          if(index==path.size()-1) { done=true; --index; passed=times.back(); remaining=0; }
        }
      } while(remaining>0);
      float u=1.f;
      int32_t seg_time=times[index+1]-times[index];
      if(seg_time>0) u=(passed-times[index])/(float)seg_time;
      P p=point(path[index],path[index+1],u);
      std::cout<<"L "<<k<<' '<<b<<' '<<elapsed<<' '<<times.back()<<' '<<index<<' '<<done<<' '<<bits(p.x)<<' '<<bits(p.y)<<' '<<bits(p.z)<<'\n';
    }
  }
}
