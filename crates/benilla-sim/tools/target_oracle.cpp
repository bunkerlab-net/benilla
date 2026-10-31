// SPDX-License-Identifier: GPL-3.0-or-later
// Original bounded harness. The generator injects hash-verified upstream methods.
// Stubs model only the explicitly accepted solo uncharmed player / ordinary NPC domain.
#include <array>
#include <cassert>
#include <cstdint>
#include <iostream>
#include <string>
using uint32 = uint32_t;
#define MANGOS_ASSERT(x) assert(x);
constexpr int TYPEID_UNIT = 3, TYPEID_PLAYER = 4, UNIT_FIELD_FLAGS = 1;
constexpr int PLAYER_FLAGS = 2, PLAYER_FLAGS_GHOST = 1;
constexpr int PLAYER_DUEL_TEAM = 3, PLAYER_DUEL_ARBITER = 4;
enum ReputationRank { REP_HOSTILE = 1, REP_UNFRIENDLY = 2, REP_NEUTRAL = 3, REP_FRIENDLY = 4 };
namespace CreatureTypeFlags { constexpr int VISIBLE_TO_GHOSTS = 1; }
// PINNED_CONSTANTS
struct FactionTemplateEntry {
    uint32 faction = 0, factionGroupMask = 0, friendGroupMask = 0, enemyGroupMask = 0;
    uint32 enemyFaction[4] = {}, friendFaction[4] = {};
};
// PINNED_COMPARATOR
struct FactionEntry { bool HasReputation() const { return false; } };
struct FactionStore { const FactionEntry* LookupEntry(uint32) const { return nullptr; } } sFactionStore;
struct ReputationMgr { bool IsAtWar(const FactionEntry*) const { return false; } };
struct SpellEntry {
    bool HasAttribute(SpellAttributesEx2) const { return false; }
    bool HasAttribute(SpellAttributesEx3) const { return false; }
};
struct Player;
struct Unit {
    int type = TYPEID_UNIT;
    uint32 flags = 0;
    bool alive = true, mounted = false;
    FactionTemplateEntry faction;
    int GetTypeId() const { return type; }
    bool HasFlag(int field, uint32 mask) const { return field == UNIT_FIELD_FLAGS && (flags & mask); }
    bool IsAlive() const { return alive; }
    bool IsMounted() const { return mounted; }
    bool IsPvP() const { return flags & UNIT_FLAG_PVP; }
    bool IsPvPFreeForAll() const { return false; }
    bool IsPlayerControlled() const { return flags & UNIT_FLAG_PLAYER_CONTROLLED; }
    const Unit* GetMaster() const { return nullptr; }
    const Player* GetControllingPlayer() const;
    const FactionTemplateEntry* GetFactionTemplateEntry() const { return &faction; }
    ReputationRank GetReactionTo(const Unit* other) const { return GetFactionReaction(&faction, &other->faction); }
    bool IsFriend(const Unit* other) const { return GetReactionTo(other) > REP_NEUTRAL; }
    bool IsEnemy(const Unit* other) const { return GetReactionTo(other) < REP_UNFRIENDLY; }
    bool CanAttack(const Unit*) const;
    bool CanAttackNow(const Unit*) const;
    bool CanAssist(const Unit*, bool = false) const;
    bool CanAttackServerside(const Unit*, bool, bool) const;
    bool CanAttackInCombat(const Unit*, bool, bool) const;
    bool CanAttackSpell(const Unit*, const SpellEntry*, bool) const;
};
struct Player : Unit {
    Player() { type = TYPEID_PLAYER; flags = UNIT_FLAG_PLAYER_CONTROLLED; }
    uint32 GetUInt32Value(int) const { return 0; }
    uint32 GetGuidValue(int) const { return 0; }
    bool IsInDuelWith(const Player*) const { return false; }
    const ReputationMgr& GetReputationMgr() const { static ReputationMgr value; return value; }
};
struct CreatureInfo { bool HasFlag(int) const { return false; } };
struct Creature : Unit {
    const CreatureInfo* GetCreatureInfo() const { static CreatureInfo value; return &value; }
};
const Player* Unit::GetControllingPlayer() const {
    return type == TYPEID_PLAYER ? static_cast<const Player*>(this) : nullptr;
}
// PINNED_METHODS
std::array<uint32, 4> list(unsigned shape, uint32 match) {
    switch (shape) {
    case 0: return {0, 0, 0, 0};
    case 1: return {match, 0, 0, 0};
    case 2: return {7, 8, 9, match};
    case 3: return {7, 0, match, 0};
    case 4: return {0, match, 0, 0};
    default: return {7, 8, 9, 10};
    }
}
uint32 blocking(unsigned selection) {
    uint32 flags = 0;
    const uint32 bits[] = { UNIT_FLAG_SPAWNING, UNIT_FLAG_NOT_ATTACKABLE_1, UNIT_FLAG_UNTARGETABLE, UNIT_FLAG_TAXI_FLIGHT, UNIT_FLAG_UNINTERACTIBLE };
    for (unsigned i = 0; i != 5; ++i) if (selection & (1u << i)) flags |= bits[i];
    return flags;
}
int main(int argc, char** argv) {
    if (argc != 2) return 2;
    const std::string mode(argv[1]);
    if (mode == "factions") {
        for (unsigned identities = 0; identities != 4; ++identities)
        for (unsigned masks = 0; masks != 8; ++masks)
        for (unsigned enemy = 0; enemy != 6; ++enemy)
        for (unsigned friend_a = 0; friend_a != 6; ++friend_a)
        for (unsigned friend_b = 0; friend_b != 6; ++friend_b) {
            FactionTemplateEntry a, b;
            a.faction = identities & 1 ? 1 : 0; b.faction = identities & 2 ? 2 : 0;
            a.factionGroupMask = 1; b.factionGroupMask = 2;
            a.enemyGroupMask = masks & 1 ? 2 : 0;
            a.friendGroupMask = masks & 2 ? 2 : 0;
            b.friendGroupMask = masks & 4 ? 1 : 0;
            auto e = list(enemy, b.faction), f = list(friend_a, b.faction), r = list(friend_b, a.faction);
            for (unsigned i = 0; i != 4; ++i) { a.enemyFaction[i] = e[i]; a.friendFaction[i] = f[i]; b.friendFaction[i] = r[i]; }
            std::cout << identities << ' ' << masks << ' ' << enemy << ' ' << friend_a << ' ' << friend_b << ' ' << GetFactionReaction(&a, &b) << '\n';
        }
    } else if (mode == "targets") {
        for (unsigned reaction = 0; reaction != 3; ++reaction)
        for (unsigned source = 0; source != 32; ++source)
        for (unsigned target = 0; target != 32; ++target)
        for (unsigned states = 0; states != 8; ++states) {
            Player player; Creature npc;
            player.faction.faction = 1; npc.faction.faction = 2;
            if (reaction == 0) player.faction.enemyFaction[0] = 2;
            if (reaction == 2) player.faction.friendFaction[0] = 2;
            player.flags |= blocking(source); npc.flags = blocking(target);
            player.alive = states & 1; npc.alive = states & 2; player.mounted = states & 4;
            const SpellEntry spell;
            std::cout << reaction << ' ' << source << ' ' << target << ' ' << states << ' '
                      << player.CanAttackNow(&npc) << ' ' << player.CanAttackSpell(&npc, &spell, false) << ' ' << player.CanAssist(&npc) << '\n';
        }
    } else return 2;
}
