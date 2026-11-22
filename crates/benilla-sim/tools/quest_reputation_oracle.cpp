// SPDX-License-Identifier: GPL-3.0-or-later
// Original synthetic probes appended to the hash-pinned Player-method harness.
Quest reputation_quest(uint32 id) {
    Quest q;
    q.QuestId = id;
    q.special = 0;
    q.RewOrReqMoney = 0;
    q.ReqCreatureOrGOId[0] = 0;
    q.ReqCreatureOrGOCount[0] = 0;
    return q;
}
void reputation_output(const std::string& name, const Player& p) {
    std::cout << name << " | " << p.money << ' ' << p.xp << ' ' << p.reputation.GetReputation(71) << ' ' << p.reputation.GetReputation(72) << " | ";
    for (uint32 id : p.log) std::cout << id << '/';
    std::cout << " | ";
    for (const auto& kv : p.mQuestStatus) {
        const auto& q = kv.second;
        std::cout << kv.first << ':' << q.m_status << ':' << q.m_rewarded;
        for (uint32 n : q.m_itemcount) std::cout << ':' << n;
        for (uint32 n : q.m_creatureOrGOcount) std::cout << ':' << n;
        std::cout << ';';
    }
    std::cout << " | ";
    for (size_t i = 0; i < events.size(); ++i) {
        if (i) std::cout << ',';
        std::cout << events[i];
    }
    std::cout << '\n';
    events.clear();
}
void add_reputation_quest(Player& p, const Quest& q, Creature& giver) {
    sObjectMgr.rows[q.QuestId] = q;
    p.AddQuest(&q, &giver);
    if (p.CanCompleteQuest(q.QuestId)) p.CompleteQuest(q.QuestId);
}
int main() {
    std::fesetround(FE_TONEAREST);
    Creature giver;
    sFactionStore.rows.emplace(71, FactionEntry{71});
    sFactionStore.rows.emplace(72, FactionEntry{72});
    for (uint32 min : {0u, 71u}) for (uint32 max : {0u, 72u})
    for (int32 lower : {-5, 0, 5}) for (int32 upper : {-5, 0, 5})
    for (int32 first : {-6, -5, 0, 5, 6}) for (int32 second : {-6, -5, 0, 5, 6}) {
        Quest q = reputation_quest(1);
        q.rep_min = min; q.rep_max = max; q.min_value = lower; q.max_value = upper;
        Player p; p.reputation.standing = {{71, first}, {72, second}};
        std::cout << "g " << min << ' ' << max << ' ' << lower << ' ' << upper << ' ' << first << ' ' << second << " | " << p.CanTakeQuest(&q, false) << ' ' << p.CanSeeStartQuest(&q) << '\n';
    }
    for (uint32 faction : {0u, 71u}) for (int32 target : {-10, 0, 10})
    for (int32 standing : {-10, 0, 9, 10, 11}) for (int blocker : {0, 1, 2})
    for (bool satisfied : {false, true}) {
        sObjectMgr.rows.clear(); events.clear();
        Quest q = reputation_quest(1); q.rep_objective = faction; q.objective_value = target;
        Player p; p.trace_views = true; p.reputation.standing = {{71, standing}, {72, 0}};
        if (blocker == 1) {q.special = 8; q.ReqItemId[0] = 200; q.ReqItemCount[0] = 2; p.carried[200] = satisfied ? 2 : 0;}
        if (blocker == 2) {q.RewOrReqMoney = -3; p.money = satisfied ? 3 : 0;}
        add_reputation_quest(p, q, giver);
        reputation_output("a " + std::to_string(faction) + " " + std::to_string(target) + " " + std::to_string(standing) + " " + std::to_string(blocker) + " " + std::to_string(satisfied), p);
    }
    for (int32 before : {0, 4, 5, 9, 10, 15}) for (int32 after : {0, 4, 5, 9, 10, 15})
    for (int blocker : {0, 1, 2}) {
        sObjectMgr.rows.clear(); events.clear();
        Player p; p.trace_views = true; p.reputation.standing = {{71, before}, {72, 0}};
        for (uint32 id : {3u, 1u, 4u, 2u}) {
            Quest q = reputation_quest(id); q.rep_objective = id == 4 ? 72 : 71; q.objective_value = id == 1 ? 10 : 5;
            if (id == 2 && blocker == 1) {q.special = 8; q.ReqItemId[0] = 200; q.ReqItemCount[0] = 2;}
            if (id == 2 && blocker == 2) q.RewOrReqMoney = -3;
            add_reputation_quest(p, q, giver);
        }
        events.clear(); p.reputation.standing[71] = after; p.ReputationChanged(sFactionStore.LookupEntry(71));
        reputation_output("c " + std::to_string(before) + " " + std::to_string(after) + " " + std::to_string(blocker), p);
    }
    for (bool items : {false, true}) for (int32 before : {10, 15}) for (int32 delta : {-10, 0, 10}) for (bool reverse : {false, true}) {
        sObjectMgr.rows.clear(); events.clear();
        Player p; p.trace_views = true; p.carried[200] = 2; p.reputation.standing = {{71, before}, {72, 0}};
        for (uint32 id : {3u, 1u, 4u, 2u}) {
            Quest q = reputation_quest(id); q.rep_objective = id == 4 ? 72 : 71; q.objective_value = id == 3 ? 10 : 15;
            if (id == 3) {
                q.reward_faction[0] = 71; q.reward_value[0] = delta;
                if (reverse) {q.reward_faction[1] = 71; q.reward_value[1] = -delta;}
                if (items) {q.special = 8; q.ReqItemId[0] = 200; q.ReqItemCount[0] = 2;}
            }
            add_reputation_quest(p, q, giver);
        }
        events.clear(); const Quest* q = sObjectMgr.GetQuestTemplate(3); assert(p.CanRewardQuest(q, 0, false)); p.RewardQuest(q, 0, &giver, true);
        reputation_output("r " + std::to_string(items) + " " + std::to_string(before) + " " + std::to_string(delta) + " " + std::to_string(reverse), p);
    }
}
