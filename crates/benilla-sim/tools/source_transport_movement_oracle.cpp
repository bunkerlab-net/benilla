// SPDX-License-Identifier: GPL-3.0-or-later
// Harness only; inserted Update and CalculateSegmentPos bodies remain unchanged.
#define DETAIL_FILTER_LOG(...) ((void)0)
// INSERT_TRACKER
struct Transport {
    TransportTemplate const& m_transportTemplate;
    GameObjectInfo const* m_goInfo;
    uint32 m_pathProgress=0,m_movementStarted=0,map=0,now=0;
    bool m_isMoving=true,m_pendingStop=false,transferred=false;
    ShortTimeTracker m_positionChangeTimer;
    KeyFrameVec::const_iterator m_currentFrame,m_nextFrame;
    float pose[4]={};
    Transport(TransportTemplate const& t,GameObjectInfo const* go,uint32 initial) : m_transportTemplate(t),m_goInfo(go),m_pathProgress(initial),m_currentFrame(t.keyFrames.begin()),m_nextFrame(t.keyFrames.begin()+1) {
        auto n=t.keyFrames.front().Node; map=n->mapid; pose[0]=n->x;pose[1]=n->y;pose[2]=n->z;pose[3]=t.keyFrames.front().InitialOrientation;
    }
    Transport* GetMap() { return this; }
    uint32 GetCurrentMSTime() { return now; }
    uint32 GetMapId() { return map; }
    uint32 GetPeriod() { return m_transportTemplate.pathTime; }
    KeyFrameVec const& GetKeyFrames() { return m_transportTemplate.keyFrames; }
    bool IsMoving() { return m_isMoving; }
    void SetMoving(bool value) { m_isMoving=value; }
    char const* GetName() { return "authored"; }
    void TeleportTransport(uint32,float,float,float,float) { transferred=true; }
    void UpdatePosition(float x,float y,float z,float o) { pose[0]=x;pose[1]=y;pose[2]=z;pose[3]=o; }
    void MoveToNextWayPoint();
    float CalculateSegmentPos(float);
    void Update(uint32);
};
// INSERT_METHODS
static uint32 word(float f) { uint32 out; std::memcpy(&out,&f,4); return out; }
static float real(uint32 b) { float out; std::memcpy(&out,&b,4); return out; }
static_assert(sizeof(decltype(atan2(0.0f,0.0f)))==sizeof(float));
int main() {
    if (std::fegetround()!=FE_TONEAREST || (_mm_getcsr() & ((1u<<15)|(1u<<6)))) return 2;
    std::string name;
    uint32 entry,path,speed,accel,n,override_period,period,initial,count;
    while (std::cin>>name>>entry>>path>>speed>>accel>>n>>override_period>>period) {
        std::vector<TaxiPathNodeEntry> nodes(n);
        sTaxiPathNodesByPath.assign(path+1,{});
        for (auto& node:nodes) {
            uint32 x,y,z;
            std::cin>>node.index>>node.mapid>>x>>y>>z>>node.actionFlag>>node.delay;
            node.path=path;node.x=real(x);node.y=real(y);node.z=real(z);
            sTaxiPathNodesByPath[path].push_back(&node);
        }
        TransportTemplate t;t.entry=entry;GameObjectInfo go{{path,speed,accel}};TransportMgr manager;
        if (!manager.GenerateWaypoints(&go,t)) return 3;
        if (override_period) { t.pathTime=period;t.keyFrames.back().DepartureTime=period; }
        std::cin>>initial>>count;
        Transport transport(t,&go,initial);
        for (uint32 i=0;i<count;++i) {
            uint32 now,diff;std::cin>>now>>diff;transport.now=now;
            if (!transport.transferred) transport.Update(diff);
            std::cout<<name<<" "<<i<<" "<<transport.m_pathProgress<<" "<<(transport.m_currentFrame-t.keyFrames.begin())<<" "<<(transport.m_nextFrame-t.keyFrames.begin())<<" "<<transport.m_isMoving<<" "<<transport.m_positionChangeTimer.GetExpiry()<<" "<<transport.transferred;
            for (float value:transport.pose) std::cout<<" "<<word(value);
            std::cout<<"\n";
        }
        for (uint32 i=0;i<t.keyFrames.size();++i) {
            auto const& frame=t.keyFrames[i];
            if (!frame.Spline || frame.IsTeleportFrame() || frame.Index>=uint32(frame.Spline->last()) || frame.NextDistFromPrev==0) continue;
            transport.m_currentFrame=t.keyFrames.begin()+i;
            for (uint32 progress : {0u,frame.DepartureTime,frame.DepartureTime+1,frame.DepartureTime+1000,frame.DepartureTime+50000}) {
                float parameter=transport.CalculateSegmentPos(float(progress)*0.001f);
                G3D::Vector3 pos,dir;
                frame.Spline->evaluate_percent(frame.Index,parameter,pos);
                frame.Spline->evaluate_derivative(frame.Index,parameter,dir);
                std::cout<<"S "<<name<<" "<<i<<" "<<progress<<" "<<word(parameter)<<" "<<word(pos.x)<<" "<<word(pos.y)<<" "<<word(pos.z)<<" "<<word(dir.x)<<" "<<word(dir.y)<<" "<<word(dir.z)<<"\n";
            }
        }
    }
}
