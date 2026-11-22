// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS 8ec338a1 ObjectMgr.cpp:7413-7474,7555-7664.
use super::*;
use rustc_apfloat::Float;

/// Raw loaded float words and uint32 operands, before ObjectMgr validation.
#[derive(Clone, Copy, Debug)]
pub struct RawSpillover {
    pub faction: u32,
    pub rate: SpellFloat,
    pub maximum_rank: u32,
}

/// One explicit lookup per Faction row, including unlisted initial factions.
#[derive(Clone, Debug)]
pub struct FactionPolicyLookup {
    pub faction: u32,
    pub reward_rates: Option<RewardRates>,
    pub spillover: Option<[RawSpillover; 4]>,
}

/// Validates the complete lookup domain and ports the source loader, not rewards.
/// This output never establishes the actual initialized aura list or its audit.
pub fn compile_faction_policies(
    factions: &[Faction],
    lookups: &[FactionPolicyLookup],
) -> Result<Vec<FactionRewardPolicy>> {
    if factions.len() > MAX_INITIAL_FACTION_ROWS || lookups.len() > MAX_INITIAL_FACTION_ROWS {
        return Err(Error::Invalid("reputation_policy_row_limit"));
    }
    let factions: BTreeMap<_, _> = factions.iter().map(|f| (f.id, f)).collect();
    let mut covered = BTreeSet::new();
    let mut policies = Vec::with_capacity(lookups.len());
    for lookup in lookups {
        let faction = factions
            .get(&lookup.faction)
            .ok_or(Error::Invalid("reputation_policy_extra_lookup"))?;
        if !covered.insert(lookup.faction) {
            return Err(Error::Invalid("reputation_policy_duplicate_lookup"));
        }
        let reward_rates = match lookup.reward_rates {
            Some(rates) => {
                // Negative rates skip the whole row. Zero is accepted, not absence.
                let mut negative = false;
                for rate in [rates.quest, rates.creature, rates.spell] {
                    let value = numeric::finite(rate)?;
                    negative |= value < rustc_apfloat::ieee::Single::ZERO;
                }
                if negative {
                    Resolved::Absent
                } else {
                    Resolved::Present(rates)
                }
            }
            None => Resolved::Absent,
        };
        let spillover = match lookup.spillover {
            // Inner-loop unlisted/rank errors do not discard a row. Only the
            // final existence checks (7635-7657) and source team check do so.
            Some(raw)
                if faction.team != 0
                    && raw
                        .iter()
                        .all(|s| s.faction == 0 || factions.contains_key(&s.faction)) =>
            {
                let mut slots = [Spillover {
                    faction: 0,
                    rate: SpellFloat::ZERO,
                    maximum_rank: Rank::Hated,
                }; 4];
                for (slot, raw) in slots.iter_mut().zip(raw) {
                    let _ = numeric::finite(raw.rate)?;
                    let rank = match raw.maximum_rank {
                        0 => Rank::Hated,
                        1 => Rank::Hostile,
                        2 => Rank::Unfriendly,
                        3 => Rank::Neutral,
                        4 => Rank::Friendly,
                        5 => Rank::Honored,
                        6 => Rank::Revered,
                        7 => Rank::Exalted,
                        // The rank operand is unreachable for a zero target.
                        _ if raw.faction == 0 => Rank::Hated,
                        _ => return Err(Error::Unsupported("reputation_spillover_rank")),
                    };
                    *slot = Spillover {
                        faction: raw.faction,
                        rate: raw.rate,
                        maximum_rank: rank,
                    };
                }
                Resolved::Present(slots)
            }
            _ => Resolved::Absent,
        };
        policies.push(FactionRewardPolicy {
            faction: lookup.faction,
            reward_rates,
            spillover,
        });
    }
    if covered.len() != factions.len() {
        return Err(Error::Unsupported("reputation_policy_lookup_missing"));
    }
    policies.sort_by_key(|p| p.faction);
    Ok(policies)
}
