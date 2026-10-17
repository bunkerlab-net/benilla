// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS original bodies are inserted from hash-verified pinned source by
// generate_quest_fixture.py. Source grant and attribution: ../PROVENANCE.md.
// This synthetic adapter contains no native world rows or client assets.
#include <cassert>
#include <cfenv>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <iostream>
#include <set>
#include <string>
#include <vector>
using uint32 = uint32_t;
using int32 = int32_t;
using uint16 = uint16_t;
constexpr uint16 QUEST_FLAGS_NO_MONEY_FROM_XP = 0x0100;
constexpr uint32 INVALIDREASON_DONT_HAVE_REQ = 0;
constexpr uint32 INVALIDREASON_QUEST_FAILED_WRONG_RACE = 1;
enum FloatConfig { CONFIG_FLOAT_RATE_DROP_MONEY, CONFIG_FLOAT_RATE_XP_QUEST };
enum IntConfig { CONFIG_UINT32_MAX_PLAYER_LEVEL };
struct World {
    float money = 1, xp = 1;
    uint32 cap = 60;
    float getConfig(FloatConfig c) { return c == CONFIG_FLOAT_RATE_DROP_MONEY ? money : xp; }
    uint32 getConfig(IntConfig) { return cap; }
} sWorld;
struct Quest;
struct Player {
    uint32 level = 1, race = 1, cls = 1, display = 0;
    int64_t given_xp = -1;
    std::vector<int32> changes;
    uint32 GetLevel() const { return level; }
    uint32 getClassMask() const { return uint32(1) << (cls - 1); }
    uint32 getRaceMask() const { return uint32(1) << (race - 1); }
    void SendCanTakeQuestResponse(uint32) const {}
    void GiveXP(uint32 xp, void*) { given_xp = xp; }
    void ModifyMoney(int32 delta) { changes.push_back(delta); }
    bool SatisfyQuestClass(Quest const*, bool) const;
    bool SatisfyQuestRace(Quest const*, bool) const;
    bool SatisfyQuestLevel(Quest const*, bool) const;
    void RewardFragment(Quest const*);
};
struct Quest {
    uint32 QuestLevel = 1, RewMoneyMaxLevel = 0;
    int32 RewOrReqMoney = 0;
    uint16 flags = 0;
    uint32 minimum = 1, maximum = 255, classes = 0, races = 0;
    uint32 GetMinLevel() const { return minimum; }
    uint32 GetMaxLevel() const { return maximum; }
    uint32 GetRequiredClasses() const { return classes; }
    uint32 GetRequiredRaces() const { return races; }
    bool HasQuestFlag(uint16 flag) const { return (flags & flag) != 0; }
    uint32 XPValue(Player*) const;
    int32 GetRewOrReqMoney() const;
    uint32 GetRewMoneyMaxLevel() const;
};
// PINNED_QUEST_METHODS
void Player::RewardFragment(Quest const* pQuest) {
// PINNED_REWARD_FRAGMENT
    display = xp;
}
float from_bits(uint32 bits) {
    float value;
    std::memcpy(&value, &bits, sizeof(value));
    return value;
}
int main(int argc, char** argv) {
    assert(argc == 2);
    assert(std::fesetround(FE_TONEAREST) == 0);
    const std::string mode = argv[1];
    Quest q;
    Player p;
    if (mode == "xp") {
        std::cout << "# quest_level player_level(0=null) raw_max_money xp\n";
        for (uint32 qlevel : {0u,1u,2u,10u,50u,55u,60u,61u,62u,63u,64u,65u,70u,255u}) {
            q.QuestLevel = qlevel;
            std::set<uint32> levels{0,1,30,59,60,61,80,255};
            if (qlevel) levels.insert(qlevel);
            for (uint32 offset = 5; offset <= 10; ++offset)
                if (qlevel + offset <= 255) levels.insert(qlevel + offset);
            for (uint32 plevel : levels) {
                p.level = plevel;
                for (uint32 money : {0u,1u,2u,3u,7u,59u,60u,61u,599u,600u,601u,
                                     16777215u,16777216u,16777217u,999999999u,1000000000u,2000000000u}) {
                    q.RewMoneyMaxLevel = money;
                    std::cout << qlevel << '\t' << plevel << '\t' << money << '\t'
                              << q.XPValue(plevel ? &p : nullptr) << '\n';
                }
            }
        }
    } else if (mode == "money") {
        std::cout << "# raw_money rate_bits reward_or_requirement\n";
        for (int32 money : {INT32_MIN, -INT32_MAX, -100, -1, 0, 1, 2, 3, 7, 59, 60, 61, 600, 1000000}) {
            q.RewOrReqMoney = money;
            for (uint32 rate : {0u,0x00800000u,0x3dcccccdu,0x3f19999au,0x3f7fffffu,
                                0x3f800000u,0x3f800001u,0x3fc00000u,0x40000000u,0x447a0000u}) {
                sWorld.money = from_bits(rate);
                std::cout << money << '\t' << rate << '\t' << q.GetRewOrReqMoney() << '\n';
            }
        }
        for (int32 money : {16777215,16777216,16777217,INT32_MAX - 127}) {
            q.RewOrReqMoney = money;
            sWorld.money = 1;
            std::cout << money << '\t' << 0x3f800000u << '\t' << q.GetRewOrReqMoney() << '\n';
        }
    } else if (mode == "reward") {
        std::cout << "# quest_level player_level cap max_money raw_money flags xp_rate_bits money_rate_bits display_xp given_xp(-1=none) max_money(-1=none) ordinary_money\n";
        const uint32 rates[][2] = {{0x3f800000,0x3f800000},{0x3fc00000,0x3f19999a},
                                  {0,0},{0x3f19999a,0x3fc00000},{0x3f800001,0x3f7fffff}};
        for (uint32 qlevel : {0u,1u,55u,60u,61u,62u,63u,64u,65u})
        for (uint32 plevel : {1u,59u,60u,61u,65u,70u})
        for (uint32 money : {0u,1u,61u,16777217u})
        for (int32 ordinary : {-100,0,1,16777217})
        for (uint16 flags : {uint16(0),uint16(QUEST_FLAGS_NO_MONEY_FROM_XP)})
        for (const auto& rate : rates) {
            q.QuestLevel=qlevel; q.RewMoneyMaxLevel=money; q.RewOrReqMoney=ordinary; q.flags=flags;
            p.level=plevel; p.given_xp=-1; p.changes.clear();
            sWorld.cap=60; sWorld.xp=from_bits(rate[0]); sWorld.money=from_bits(rate[1]);
            p.RewardFragment(&q);
            assert(p.changes.size() == (plevel < sWorld.cap ? 1u : 2u));
            std::cout << qlevel << '\t' << plevel << '\t' << sWorld.cap << '\t' << money << '\t'
                      << ordinary << '\t' << flags << '\t' << rate[0] << '\t' << rate[1] << '\t'
                      << p.display << '\t' << p.given_xp << '\t'
                      << (p.changes.size() == 2 ? int64_t(p.changes[0]) : -1) << '\t'
                      << p.changes.back() << '\n';
        }
    } else if (mode == "gates") {
        std::cout << "# level race class minimum maximum classes races level_ok class_ok race_ok\n";
        for (uint32 level : {1u,5u,6u,59u,60u,61u,255u})
        for (uint32 race : {1u,2u,3u,4u,5u,6u,7u,8u})
        for (uint32 cls : {1u,2u,3u,4u,5u,7u,8u,9u,11u})
        for (uint32 profile = 0; profile < 8; ++profile) {
            p.level=level; p.race=race; p.cls=cls;
            q.minimum=profile & 1 ? 5 : 1;
            q.maximum=profile & 2 ? 60 : 255;
            if (profile == 7) q.maximum=0;
            q.classes=profile & 4 ? 0x85 : 0;
            q.races=profile & 2 ? 0x85 : 0;
            std::cout << level << '\t' << race << '\t' << cls << '\t' << q.minimum << '\t' << q.maximum
                      << '\t' << q.classes << '\t' << q.races << '\t'
                      << p.SatisfyQuestLevel(&q,false) << '\t'
                      << p.SatisfyQuestClass(&q,false) << '\t'
                      << p.SatisfyQuestRace(&q,false) << '\n';
        }
    } else return 2;
}
