// SPDX-License-Identifier: GPL-3.0-or-later
// Authored rows/adapters only. Generator inserts hash-verified CMaNGOS bodies.
// CMaNGOS contributors, GPL-2.0-or-later; translation GPL-3.0-or-later; no warranty.
#include <cassert>
#include <cstdint>
#include <iostream>
#include <map>
#include <string>
using uint32 = uint32_t;
using int32 = int32_t;
using RepListID = uint32;
#define MAX_REPUTATION_RANK 8
#define MIN_REPUTATION_RANK REP_HATED
#define MANGOS_ASSERT(x) assert(x);
constexpr int TYPEID_UNIT = 3, TYPEID_PLAYER = 4, UNIT_FIELD_FLAGS = 1;
constexpr int PLAYER_FLAGS = 2, PLAYER_FLAGS_GHOST = 1, PLAYER_FLAGS_CONTESTED_PVP = 2;
constexpr int PLAYER_DUEL_TEAM = 3, PLAYER_DUEL_ARBITER = 4;
namespace CreatureTypeFlags { constexpr int VISIBLE_TO_GHOSTS = 1; }
// PINNED_DECLARATIONS
using FactionStateList = std::map<RepListID, FactionState>;
struct FactionTemplateEntry {
    uint32 faction = 0, factionGroupMask = 0, friendGroupMask = 0, enemyGroupMask = 0;
    uint32 enemyFaction[4] = {}, friendFaction[4] = {};
    bool IsContestedGuardFaction() const { return false; }
};
// PINNED_COMPARATOR
struct FactionStore {
    std::map<uint32, FactionEntry> rows;
    const FactionEntry* LookupEntry(uint32 id) const {
        auto it = rows.find(id);
        return it == rows.end() ? nullptr : &it->second;
    }
    uint32 GetNumRows() const { return rows.empty() ? 0 : rows.rbegin()->first + 1; }
} sFactionStore;
struct Player;
struct ReputationMgr {
    Player* m_player;
    FactionStateList m_factions;
    explicit ReputationMgr(Player* p) : m_player(p) {}
    static const int32 PointsInRank[MAX_REPUTATION_RANK];
    static const int32 Reputation_Cap = 42999;
    static ReputationRank ReputationToRank(int32);
    const FactionState* GetState(const FactionEntry*) const;
    const FactionState* GetState(RepListID) const;
    int32 GetReputation(const FactionEntry*) const;
    int32 GetBaseReputation(const FactionEntry*) const;
    uint32 GetDefaultStateFlags(const FactionEntry*) const;
    ReputationRank GetRank(const FactionEntry*) const;
    bool IsAtWar(const FactionEntry*) const;
    void Initialize();
    const ReputationRank* GetForcedRankIfAny(const FactionTemplateEntry*) const { return nullptr; }
};
struct CombatManager {
    bool evading = false;
    bool IsEvadingHome() const { return evading; }
};
struct Unit {
    int type = TYPEID_UNIT;
    uint32 flags = 0, state = 0, guid = 2;
    bool alive = true, mounted = false, ignore_feign = false, m_canEnterCombat = true;
    FactionTemplateEntry faction;
    CombatManager combat;
    int GetTypeId() const { return type; }
    bool HasFlag(int field, uint32 mask) const { return field == UNIT_FIELD_FLAGS && (flags & mask); }
    bool IsAlive() const { return alive; }
    bool IsMounted() const { return mounted; }
    bool IsPvP() const { return flags & UNIT_FLAG_PVP; }
    bool IsPvPFreeForAll() const { return false; }
    bool IsPlayerControlled() const { return flags & UNIT_FLAG_PLAYER_CONTROLLED; }
    bool IsIgnoringFeignDeath() const { return ignore_feign; }
    bool IsFeigningDeathSuccessfully() const { return state & UNIT_STAT_FEIGN_DEATH; }
    bool hasUnitState(uint32 mask) const { return state & mask; }
    uint32 GetPersuadedGuid() const { return 0; }
    uint32 GetObjectGuid() const { return guid; }
    const CombatManager& GetCombatManager() const { return combat; }
    const Player* GetControllingPlayer() const;
    const FactionTemplateEntry* GetFactionTemplateEntry() const { return &faction; }
    ReputationRank GetReactionTo(const Unit*) const;
    bool IsFriend(const Unit*) const;
    bool IsEnemy(const Unit*) const;
    bool CanAttack(const Unit*) const;
    bool CanAttackNow(const Unit*) const;
    bool CanAttackOnSight(const Unit*) const;
    bool CanAttackServerside(const Unit*, bool, bool) const;
    bool CanAttackInCombat(const Unit*, bool = false, bool = false) const;
    // PINNED_COMBAT_ENTRY
};
struct Player : Unit {
    ReputationMgr rep{this};
    Player() { type = TYPEID_PLAYER; flags = UNIT_FLAG_PLAYER_CONTROLLED; guid = 1; }
    uint32 GetUInt32Value(int) const { return 0; }
    uint32 GetGuidValue(int) const { return 0; }
    uint32 getRaceMask() const { return 1; }
    uint32 getClassMask() const { return 128; }
    bool IsInGroup(const Player*) const { return false; }
    const ReputationMgr& GetReputationMgr() const { return rep; }
};
struct CreatureInfo { bool HasFlag(int) const { return false; } };
struct Creature : Unit {
    bool m_forceAttackingCapability = false, can_aggro = true;
    const CreatureInfo* GetCreatureInfo() const { static CreatureInfo value; return &value; }
    bool IsPet() const { return false; }
    bool CanAggro() const { return can_aggro; }
    bool CanInitiateAttack() const;
};
struct Pet : Creature { bool HasActionsDisabled() const { return false; } };
const Player* Unit::GetControllingPlayer() const {
    return type == TYPEID_PLAYER ? static_cast<const Player*>(this) : nullptr;
}
// PINNED_METHODS
uint32 blocking(unsigned selection) {
    uint32 flags = 0;
    const uint32 bits[] = { UNIT_FLAG_SPAWNING, UNIT_FLAG_NOT_ATTACKABLE_1, UNIT_FLAG_UNTARGETABLE, UNIT_FLAG_TAXI_FLIGHT, UNIT_FLAG_UNINTERACTIBLE };
    for (unsigned i = 0; i != 5; ++i) if (selection & (1u << i)) flags |= bits[i];
    return flags;
}
const int32 bases[] = {-42000, -6001, -6000, -3001, -3000, -1, 0, 2999, 3000, 8999, 9000, 20999, 21000, 41999, 42000, 42999};
void seed(uint32 c) {
    sFactionStore.rows.clear();
    FactionEntry player{}; player.ID = 1; player.reputationListID = -1;
    sFactionStore.rows[1] = player;
    FactionEntry npc{}; npc.ID = 2; npc.reputationListID = c % 2 ? 0 : -1;
    npc.BaseRepRaceMask[0] = c % 3 == 0 ? 0 : 2;
    npc.BaseRepRaceMask[1] = 1; npc.BaseRepClassMask[1] = 128;
    npc.BaseRepValue[0] = bases[(c / 2) % 16];
    npc.BaseRepValue[1] = bases[(c / 2 + 5) % 16];
    npc.ReputationFlags[0] = (c / 32) % 2 ? FACTION_FLAG_AT_WAR : 0;
    npc.ReputationFlags[1] = (c / 64) % 2 ? FACTION_FLAG_AT_WAR : 0;
    sFactionStore.rows[2] = npc;
}
uint32 bit_pattern(uint32 i) { return i == 0 ? 0 : i == 33 ? UINT32_MAX : (uint32(1) << (i - 1)); }
int main(int argc, char** argv) {
    if (argc != 2) return 2;
    const std::string mode(argv[1]);
    if (mode == "relations" || mode == "relations_disarmed") {
        for (uint32 c = 0; c != 32768; ++c) {
            seed(c);
            Player p; Creature n;
            p.rep.Initialize();
            p.faction.faction = 1; n.faction.faction = 2;
            if (c % 3 == 0) p.faction.enemyFaction[0] = 2;
            if (c % 3 == 2) p.faction.friendFaction[0] = 2;
            if ((c / 3) % 3 == 0) n.faction.enemyFaction[0] = 1;
            if ((c / 3) % 3 == 2) n.faction.friendFaction[0] = 1;
            p.flags |= blocking((c / 64) % 32); n.flags |= blocking((c / 1024) % 32);
            if (mode == "relations_disarmed") n.flags |= UNIT_FLAG_DISARMED;
            p.alive = (c / 7) % 2; n.alive = (c / 11) % 2;
            p.mounted = (c / 13) % 2; n.mounted = (c / 17) % 2;
            p.combat.evading = (c / 19) % 2; n.combat.evading = (c / 23) % 2;
            p.state = (c / 29) % 2 ? uint32(UNIT_STAT_FEIGN_DEATH) : 0;
            n.state = (c / 31) % 2 ? uint32(UNIT_STAT_FEIGN_DEATH) : 0;
            p.ignore_feign = (c / 37) % 2; n.ignore_feign = (c / 41) % 2;
            std::cout << c << ' ' << p.GetReactionTo(&n) << ' ' << n.GetReactionTo(&p)
                      << ' ' << p.CanAttack(&n) << ' ' << n.CanAttack(&p)
                      << ' ' << p.CanAttackNow(&n) << ' ' << n.CanAttackNow(&p)
                      << ' ' << p.CanAttackOnSight(&n) << ' ' << n.CanAttackOnSight(&p)
                      << ' ' << p.CanAttackInCombat(&n) << ' ' << n.CanAttackInCombat(&p) << '\n';
        }
    } else if (mode == "lifecycle") {
        for (uint32 s = 0; s != 34; ++s)
        for (uint32 f = 0; f != 34; ++f)
        for (uint32 c = 0; c != 16; ++c) {
            Creature n; n.state = bit_pattern(s); n.flags = bit_pattern(f);
            n.m_forceAttackingCapability = c & 1; n.can_aggro = c & 2;
            n.m_canEnterCombat = c & 4; n.combat.evading = c & 8;
            std::cout << s << ' ' << f << ' ' << c << ' ' << n.CanInitiateAttack() << ' ' << n.CanEnterCombat() << '\n';
        }
    } else return 2;
}
