// SPDX-License-Identifier: GPL-3.0-or-later
// Harness only. The generator inserts unchanged pinned source implementations.
#include <algorithm>
static_assert(sizeof(decltype(atan2(0.0f,0.0f)))==sizeof(float), "source global atan2 result width");
static_assert(sizeof(decltype(sqrt(0.0f)))==sizeof(float), "source global sqrt result width");
static uint32 word(float f) { uint32 out; std::memcpy(&out,&f,4); return out; }
static uint64_t word(double f) { uint64_t out; std::memcpy(&out,&f,8); return out; }
static float real(uint32 b) { float out; std::memcpy(&out,&b,4); return out; }
int main() {
    if (std::fegetround()!=FE_TONEAREST || (_mm_getcsr() & ((1u<<15)|(1u<<6)))) return 2;
    std::string name;
    uint32 entry,path,speed,accel,n,override_period,period;
    while (std::cin>>name>>entry>>path>>speed>>accel>>n>>override_period>>period) {
        std::vector<TaxiPathNodeEntry> nodes(n);
        sTaxiPathNodesByPath.assign(path+1,{});
        for (auto& node:nodes) {
            uint32 x,y,z;
            std::cin>>node.index>>node.mapid>>x>>y>>z>>node.actionFlag>>node.delay;
            node.path=path; node.x=real(x); node.y=real(y); node.z=real(z);
            sTaxiPathNodesByPath[path].push_back(&node);
        }
        TransportTemplate t; t.entry=entry;
        GameObjectInfo go{{path,speed,accel}};
        TransportMgr manager;
        if (!manager.GenerateWaypoints(&go,t)) return 3;
        if (override_period) {
            // Transports.cpp:89-90. No other template field is recomputed.
            t.pathTime=period;
            t.keyFrames.back().DepartureTime=period;
        }
        std::cout<<name<<" "<<t.entry<<" "<<t.inInstance<<" "<<t.pathTime<<" "<<word(t.accelTime)<<" "<<word(t.accelDist)<<" "<<t.mapsUsed.size();
        for (auto map:t.mapsUsed) std::cout<<" "<<map;
        std::cout<<" "<<t.keyFrames.size();
        std::vector<TransportSpline const*> splines;
        for (auto const& f:t.keyFrames) {
            auto it=std::find(splines.begin(),splines.end(),f.Spline);
            if (f.Spline && it==splines.end()) { splines.push_back(f.Spline); it=splines.end()-1; }
            int64_t spline_id=f.Spline ? it-splines.begin() : -1;
            std::cout<<" "<<f.Node->index<<" "<<f.Index<<" "<<word(f.InitialOrientation)
                     <<" "<<word(f.DistSinceStop)<<" "<<word(f.DistUntilStop)<<" "<<word(f.DistFromPrev)
                     <<" "<<word(f.TimeFrom)<<" "<<word(f.TimeTo)<<" "<<f.Teleport<<" "<<f.Update
                     <<" "<<f.ArriveTime<<" "<<f.DepartureTime<<" "<<word(f.NextDistFromPrev)<<" "<<f.NextArriveTime<<" "<<spline_id;
        }
        std::cout<<" "<<splines.size();
        for (auto spline:splines) {
            auto const& points=spline->getPoints();
            std::cout<<" "<<points.size()<<" "<<spline->first()<<" "<<spline->last();
            for (auto p:points) std::cout<<" "<<word(p.x)<<" "<<word(p.y)<<" "<<word(p.z);
            for (int i=0;i<=spline->last();++i) std::cout<<" "<<word(spline->length(i));
        }
        std::cout<<"\n";
    }
}
