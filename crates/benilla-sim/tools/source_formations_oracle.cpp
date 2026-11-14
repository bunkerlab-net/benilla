// SPDX-License-Identifier: GPL-3.0-or-later
// Authored host for unchanged CMaNGOS 8ec338a1 SpawnGroup.cpp:1541-1911.
#include <cmath>
#include <cstdint>
#include <cstring>
#include <cstdio>
#include <map>
#include <random>
#include <type_traits>
#include <dlfcn.h>
using uint32 = uint32_t;
#define M_PI_F float(M_PI)
static_assert(std::is_same_v<decltype(std::asin(float())), float>);
static_assert(std::is_same_v<decltype(std::sqrt(float())), float>);
enum { SPAWN_GROUP_FORMATION_TYPE_RANDOM, SPAWN_GROUP_FORMATION_TYPE_SINGLE_FILE,
SPAWN_GROUP_FORMATION_TYPE_SIDE_BY_SIDE, SPAWN_GROUP_FORMATION_TYPE_LIKE_GEESE,
SPAWN_GROUP_FORMATION_TYPE_FANNED_OUT_BEHIND, SPAWN_GROUP_FORMATION_TYPE_FANNED_OUT_IN_FRONT,
SPAWN_GROUP_FORMATION_TYPE_CIRCLE_THE_LEADER };
constexpr uint32 SPAWN_GROUP_FORMATION_OPTION_KEEP_CONPACT = 2;
uint32 bits(float f) { uint32 u; std::memcpy(&u,&f,4); return u; }
struct Generator {
    using result_type=uint32;
    std::mt19937 mt{5489};
    static constexpr uint32 min() { return 0; }
    static constexpr uint32 max() { return UINT32_MAX; }
    uint32 operator()() { auto u=mt(); std::printf("W %08x\n",u); return u; }
} randomWords;
float frand(float a, float b) {
    float r=std::uniform_real_distribution<float>(a,b)(randomWords);
    std::printf("R %08x %08x %08x\n",bits(a),bits(b),bits(r)); return r;
}
struct Unit { bool alive=true; float width=1.25f; bool IsAlive() const { return alive; } float GetCollisionWidth() const { return width; } };
struct GroupEntry { bool mirroring=true; bool IsMirroring() const { return mirroring; } };
struct Group { GroupEntry entry; GroupEntry& GetGroupEntry() { return entry; } };
struct FormationData;
struct FormationSlotData {
    Unit* owner=nullptr; FormationData* formation=nullptr; Group* group=nullptr;
    float m_realAngle=0,m_realDistance=1,m_angleVariation=0,m_distanceVariation=0;
    float m_maxAngleVariation=0,m_maxDistanceVariation=0,m_angleVariationDest=0,m_distanceVariationDest=0;
    bool recompute=true;
    Unit* GetOwner() { return owner; }
    FormationData* GetFormationData() { return formation; }
    Group* GetCreatureGroup() { return group; }
    float GetRealAngle() { return m_realAngle; }
    void SetAngle(float x) { m_realAngle=x; m_angleVariation=x; }
    void SetDistance(float x) { m_realDistance=x; m_distanceVariation=x; }
    void SetMaxVariation(float a,float d) { m_maxAngleVariation=a; m_maxDistanceVariation=d; }
    bool& GetRecomputePosition() { return recompute; }
    float GetAngle(); void AddPositionVariation(bool now=false); void Update();
};
struct FormationData {
    uint32 m_currentFormationShape=0,m_currentOptions=0; float m_currentSpread=3.25f;
    bool mirror=false; std::map<uint32,FormationSlotData*> m_slotsMap;
    bool HaveOption(uint32 option) { return (option & m_currentOptions)!=0; }
    Unit* GetMaster() { return m_slotsMap.count(0) ? m_slotsMap.at(0)->owner : nullptr; }
    bool GetMirrorState() { return mirror; }
    void FixSlotsPositions();
};
// INSERT_METHODS
void emit(unsigned id, unsigned phase, FormationData& f) {
    for(auto [index,p]:f.m_slotsMap) {
        std::printf("S %u %u %u",id,phase,index);
        for(float x:{p->m_realAngle,p->m_realDistance,p->m_angleVariation,p->m_distanceVariation,
            p->m_maxAngleVariation,p->m_maxDistanceVariation,p->m_angleVariationDest,p->m_distanceVariationDest,p->GetAngle()}) std::printf(" %08x",bits(x));
        std::printf("\n");
    }
}
int main() {
    for(const char* name:{"asinf","sqrtf"}) {
        void* p=dlsym(RTLD_DEFAULT,name); Dl_info d{}; dladdr(p,&d);
        std::fprintf(stderr,"%s %s offset=%lx symbol=%s\n",name,d.dli_fname,(unsigned long)p-(unsigned long)d.dli_fbase,d.dli_sname?d.dli_sname:"none");
    }
    unsigned id,shape,count,compact,pattern,mirror;
    while(std::scanf("%u %u %u %u %u %u",&id,&shape,&count,&compact,&pattern,&mirror)==6) {
        FormationData f; Group g; Unit units[12]; FormationSlotData slots[12];
        randomWords.mt.seed(5489);
        f.m_currentFormationShape=shape; f.m_currentOptions=compact*2; f.mirror=mirror;
        std::printf("C %u\n",id);
        for(unsigned i=0;i<count;++i) {
            slots[i].formation=&f; slots[i].group=&g;
            units[i].alive=pattern!=2 || i%2==0;
            units[i].width=1.25f+float(i)*0.125f;
            slots[i].owner=(pattern==1 && i%2==1) || pattern==3 ? nullptr : &units[i];
            f.m_slotsMap[i]=&slots[i];
        }
        f.FixSlotsPositions(); emit(id,0,f);
        for(auto [index,p]:f.m_slotsMap) if(p->owner==nullptr || p->owner!=f.GetMaster()) p->AddPositionVariation(false);
        emit(id,1,f);
        for(auto [index,p]:f.m_slotsMap) if(p->owner==nullptr || p->owner!=f.GetMaster()) p->Update();
        emit(id,2,f);
    }
}
