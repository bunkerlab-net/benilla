// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from pinned CMaNGOS Classic Relations.cpp; see ../PROVENANCE.md.
// Copyright: CMaNGOS Project contributors; see ../licenses/AUTHORS.md.
// Upstream grants GPL version 2 or (at your option) any later version.
// Rust translation: 2026 Benilla fork contributors, GPL version 3 or later,
// WITHOUT ANY WARRANTY, including MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
//! Bounded Classic player/NPC relationship checks, not combat, AI, or spell validation.
//!
//! Pinned source and the deliberately rejected domain are in `CLASSIC-TARGETS.md`.
//! These functions reuse the client DBC row types but preserve the *server* comparator.
//! An `Allowed` result only passes this subcheck: range, visibility, hit rolls, spell
//! effects, resources, conditions, immunity and runtime capability gates remain separate.

use benilla_formats::{FactionCatalog, FactionTemplate, Reaction};

/// Version of the deliberately narrow relationship-check contract, not a world profile.
pub const CAPABILITY_ID: &str = "cmangos-classic-solo-player-npc-targets-v1";

/// Native `UnitFlags`, pinned `Entities/Unit.h`. No faction IDs are hardcoded here.
pub mod unit_flags {
    pub const SPAWNING: u32 = 0x0000_0002;
    pub const PLAYER_CONTROLLED: u32 = 0x0000_0008;
    pub const NOT_ATTACKABLE_1: u32 = 0x0000_0080;
    pub const IMMUNE_TO_PLAYER: u32 = 0x0000_0100;
    pub const IMMUNE_TO_NPC: u32 = 0x0000_0200;
    pub const LOOTING: u32 = 0x0000_0400;
    pub const PVP: u32 = 0x0000_1000;
    pub const PERSUADED: u32 = 0x0000_4000;
    pub const SWIMMING: u32 = 0x0000_8000;
    pub const UNTARGETABLE: u32 = 0x0001_0000;
    pub const IN_COMBAT: u32 = 0x0008_0000;
    pub const TAXI_FLIGHT: u32 = 0x0010_0000;
    pub const POSSESSED: u32 = 0x0100_0000;
    pub const UNINTERACTIBLE: u32 = 0x0200_0000;
    pub const SHEATHE: u32 = 0x4000_0000;
    pub const IMMUNE: u32 = 0x8000_0000;
}

/// The unchanged mask from `Unit::CanAttack`/`CanAttackServerside`.
pub const ATTACK_BLOCKING_FLAGS: u32 = unit_flags::SPAWNING
    | unit_flags::NOT_ATTACKABLE_1
    | unit_flags::UNTARGETABLE
    | unit_flags::TAXI_FLIGHT
    | unit_flags::UNINTERACTIBLE;

const SUPPORTED_UNIT_FLAGS: u32 = ATTACK_BLOCKING_FLAGS
    | unit_flags::PLAYER_CONTROLLED
    | unit_flags::SWIMMING
    | unit_flags::IN_COMBAT
    | unit_flags::LOOTING
    | unit_flags::SHEATHE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactionReputation {
    Missing,
    /// Only valid for a template whose faction ID is zero (no Faction.dbc identity).
    NoFaction,
    /// An actual Faction.dbc row was found with a negative reputation list index.
    Untracked,
    Tracked {
        index: i32,
    },
}

/// A borrowed native row plus metadata the client colour comparator does not need.
#[derive(Debug, Clone, Copy)]
pub struct FactionRef<'a> {
    pub template: Option<&'a FactionTemplate>,
    pub template_flags: Option<u32>,
    pub reputation: FactionReputation,
}

impl<'a> FactionRef<'a> {
    /// Resolve user-local rows without treating a missing row as neutral or untracked.
    pub fn from_catalog(catalog: &'a FactionCatalog, template_id: u32) -> Self {
        let template = catalog.template(template_id);
        let reputation = match template {
            None => FactionReputation::Missing,
            Some(row) if row.faction == 0 => FactionReputation::NoFaction,
            Some(row) => match catalog.faction(row.faction) {
                None => FactionReputation::Missing,
                Some(faction) if faction.rep_index < 0 => FactionReputation::Untracked,
                Some(faction) => FactionReputation::Tracked {
                    index: faction.rep_index,
                },
            },
        };
        Self {
            template,
            template_flags: catalog.template_flags(template_id),
            reputation,
        }
    }
}

/// No default: the compiler must classify these states from native/session evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextState {
    /// The caller verified this unsupported state is absent.
    Absent,
    Present,
    Unknown,
}

/// Explicit exclusions from the ordinary, uncharmed, solo player/NPC domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelationContext {
    /// Ownership, charm, possession, pets, guardians, totems, or a second player.
    pub control: ContextState,
    /// PvP flags, FFA, duel, contested PvP, battleground and sanctuary overrides.
    pub pvp: ContextState,
    /// Forced ranks, at-war overrides and persuasion (not just visible standings).
    pub reputation_overrides: ContextState,
    /// Aura, school, mechanic or scripted immunities, beyond native UNIT_FIELD_FLAGS.
    pub immunity: ContextState,
    /// Ghost, feign death, evade, temporary faction, scripted/conditional target rules.
    pub special_state: ContextState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitTargetState {
    pub unit_flags: u32,
    pub alive: bool,
    pub mounted: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct PlayerNpcInput<'a> {
    pub player_faction: FactionRef<'a>,
    pub npc_faction: FactionRef<'a>,
    pub player: UnitTargetState,
    pub npc: UnitTargetState,
    pub context: RelationContext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Player,
    Npc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetUnsupported {
    MissingTemplate(Side),
    MissingTemplateFlags(Side),
    MissingFaction(Side),
    InconsistentFactionMetadata(Side),
    Reputation(Side),
    TemplateFlags {
        side: Side,
        flags: u32,
    },
    ControlContext,
    PvpContext,
    ReputationOverrides,
    Immunity,
    SpecialState,
    UnitFlags {
        side: Side,
        flags: u32,
    },
    AreaSpell,
    DeadTargetSpell,
    IgnoreRestrictionsSpell,
    SpellAttributes {
        attributes_ex2: u32,
        attributes_ex3: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetDenial {
    PlayerDead,
    PlayerMounted,
    NpcDead,
    TargetFlags {
        flags: u32,
    },
    Friendly,
    NotFriendly,
    /// Classic CanAssist permits player assistance of ordinary NPCs only with PvP UI.
    NpcNotPvp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetDecision {
    Allowed,
    Denied(TargetDenial),
    Unsupported(TargetUnsupported),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerNpcReactions {
    pub player_to_npc: Reaction,
    pub npc_to_player: Reaction,
}

/// `Relations.cpp::GetFactionReaction(FactionTemplateEntry*, FactionTemplateEntry*)`.
/// Unlike the client's current comparator, slot zero enables a scan of *all* four
/// entries. A zero gap does not terminate that scan. Hostile masks precede lists.
pub fn faction_reaction(this: &FactionTemplate, other: &FactionTemplate) -> Reaction {
    if other.group_mask & this.enemy_group_mask != 0
        || (this.enemies[0] != 0 && other.faction != 0 && this.enemies.contains(&other.faction))
    {
        return Reaction::Hostile;
    }
    if other.group_mask & this.friend_group_mask != 0
        || (this.friends[0] != 0 && other.faction != 0 && this.friends.contains(&other.faction))
        || this.group_mask & other.friend_group_mask != 0
        || (other.friends[0] != 0 && this.faction != 0 && other.friends.contains(&this.faction))
    {
        return Reaction::Friendly;
    }
    Reaction::Neutral
}

fn validate_faction<'a>(
    row: FactionRef<'a>,
    side: Side,
) -> Result<&'a FactionTemplate, TargetUnsupported> {
    let template = row
        .template
        .ok_or(TargetUnsupported::MissingTemplate(side))?;
    let flags = row
        .template_flags
        .ok_or(TargetUnsupported::MissingTemplateFlags(side))?;
    if flags != 0 {
        return Err(TargetUnsupported::TemplateFlags { side, flags });
    }
    match row.reputation {
        FactionReputation::Missing => return Err(TargetUnsupported::MissingFaction(side)),
        FactionReputation::NoFaction if template.faction == 0 => {}
        FactionReputation::Untracked if template.faction != 0 => {}
        FactionReputation::Tracked { index } if index >= 0 && template.faction != 0 => {
            return Err(TargetUnsupported::Reputation(side));
        }
        _ => return Err(TargetUnsupported::InconsistentFactionMetadata(side)),
    }
    Ok(template)
}

/// Supported branch of `Unit::GetReactionTo`, evaluated in both directions.
/// Rejections take precedence over any relationship result, including friendly.
pub fn player_npc_reactions(
    input: &PlayerNpcInput<'_>,
) -> Result<PlayerNpcReactions, TargetUnsupported> {
    let context = input.context;
    for (state, reason) in [
        (context.control, TargetUnsupported::ControlContext),
        (context.pvp, TargetUnsupported::PvpContext),
        (
            context.reputation_overrides,
            TargetUnsupported::ReputationOverrides,
        ),
        (context.immunity, TargetUnsupported::Immunity),
        (context.special_state, TargetUnsupported::SpecialState),
    ] {
        if state != ContextState::Absent {
            return Err(reason);
        }
    }
    if input.player.unit_flags & unit_flags::PLAYER_CONTROLLED == 0
        || input.npc.unit_flags & unit_flags::PLAYER_CONTROLLED != 0
    {
        return Err(TargetUnsupported::ControlContext);
    }
    for (side, state) in [(Side::Player, input.player), (Side::Npc, input.npc)] {
        let flags = state.unit_flags;
        if flags & unit_flags::POSSESSED != 0 {
            return Err(TargetUnsupported::ControlContext);
        }
        if flags & unit_flags::PVP != 0 {
            return Err(TargetUnsupported::PvpContext);
        }
        if flags & unit_flags::PERSUADED != 0 {
            return Err(TargetUnsupported::ReputationOverrides);
        }
        if flags & (unit_flags::IMMUNE_TO_PLAYER | unit_flags::IMMUNE_TO_NPC | unit_flags::IMMUNE)
            != 0
        {
            return Err(TargetUnsupported::Immunity);
        }
        let unsupported = flags & !SUPPORTED_UNIT_FLAGS;
        if unsupported != 0 {
            return Err(TargetUnsupported::UnitFlags {
                side,
                flags: unsupported,
            });
        }
    }
    let player = validate_faction(input.player_faction, Side::Player)?;
    let npc = validate_faction(input.npc_faction, Side::Npc)?;
    Ok(PlayerNpcReactions {
        player_to_npc: faction_reaction(player, npc),
        npc_to_player: faction_reaction(npc, player),
    })
}

fn attack_relationship(flags: u32, reaction: Reaction) -> TargetDecision {
    let blocking = flags & ATTACK_BLOCKING_FLAGS;
    if blocking != 0 {
        TargetDecision::Denied(TargetDenial::TargetFlags { flags: blocking })
    } else if reaction == Reaction::Friendly {
        TargetDecision::Denied(TargetDenial::Friendly)
    } else {
        // With exactly one player-controlled side, CanAttack tests !IsFriend,
        // not IsEnemy. A neutral ordinary NPC can be attacked deliberately.
        TargetDecision::Allowed
    }
}

/// `Unit::CanAttackNow`'s bounded player -> ordinary NPC branch.
/// Does not authorize a swing; weapon, range, facing, timing and hit rules are absent.
pub fn check_player_melee_target(input: &PlayerNpcInput<'_>) -> TargetDecision {
    let reactions = match player_npc_reactions(input) {
        Ok(reactions) => reactions,
        Err(reason) => return TargetDecision::Unsupported(reason),
    };
    if !input.player.alive {
        return TargetDecision::Denied(TargetDenial::PlayerDead);
    }
    if input.player.mounted {
        return TargetDecision::Denied(TargetDenial::PlayerMounted);
    }
    if !input.npc.alive {
        return TargetDecision::Denied(TargetDenial::NpcDead);
    }
    attack_relationship(input.npc.unit_flags, reactions.player_to_npc)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HarmfulSpellTargetInput {
    /// Native Spell.dbc AttributesEx2; only the ordinary zero value is supported here.
    pub attributes_ex2: u32,
    /// Native Spell.dbc AttributesEx3; only the ordinary zero value is supported here.
    pub attributes_ex3: u32,
    pub is_aoe: bool,
}

/// `CanAttackSpell -> CanAttackInCombat -> CanAttackServerside` relationship subcheck
/// for a direct harmful unit target, as called by Spell::CheckCast's TARGET_HARMFUL.
/// Intentionally does not check caster liveness/mount, just as CanAttackSpell does
/// not: CheckCast performs those independently. It is never a full cast validator.
pub fn check_player_harmful_spell_relationship(
    input: &PlayerNpcInput<'_>,
    spell: &HarmfulSpellTargetInput,
) -> TargetDecision {
    let reactions = match player_npc_reactions(input) {
        Ok(reactions) => reactions,
        Err(reason) => return TargetDecision::Unsupported(reason),
    };
    let unsupported = if spell.is_aoe {
        Some(TargetUnsupported::AreaSpell)
    } else if spell.attributes_ex2 & 0x0000_0001 != 0 {
        Some(TargetUnsupported::DeadTargetSpell)
    } else if spell.attributes_ex3 & 0x1000_0000 != 0 {
        Some(TargetUnsupported::IgnoreRestrictionsSpell)
    } else if spell.attributes_ex2 != 0 || spell.attributes_ex3 != 0 {
        Some(TargetUnsupported::SpellAttributes {
            attributes_ex2: spell.attributes_ex2,
            attributes_ex3: spell.attributes_ex3,
        })
    } else {
        None
    };
    if let Some(reason) = unsupported {
        return TargetDecision::Unsupported(reason);
    }
    if !input.npc.alive {
        return TargetDecision::Denied(TargetDenial::NpcDead);
    }
    attack_relationship(input.npc.unit_flags, reactions.player_to_npc)
}

/// `Unit::CanAssist`/`CanAssistSpell` player -> ordinary NPC branch.
/// Friendly non-PvP NPCs are *not* healable by players in this pinned Classic code.
/// PvP NPCs are outside the profile, so they return Unsupported rather than true.
pub fn check_player_assist_relationship(input: &PlayerNpcInput<'_>) -> TargetDecision {
    let reactions = match player_npc_reactions(input) {
        Ok(reactions) => reactions,
        Err(reason) => return TargetDecision::Unsupported(reason),
    };
    if input.npc.unit_flags & unit_flags::UNINTERACTIBLE != 0 {
        return TargetDecision::Denied(TargetDenial::TargetFlags {
            flags: unit_flags::UNINTERACTIBLE,
        });
    }
    if reactions.player_to_npc != Reaction::Friendly {
        return TargetDecision::Denied(TargetDenial::NotFriendly);
    }
    TargetDecision::Denied(TargetDenial::NpcNotPvp)
}
