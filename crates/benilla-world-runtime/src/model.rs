// SPDX-License-Identifier: GPL-3.0-or-later
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub type OwnerId = [u8; 16];
pub type CharacterId = [u8; 16];
pub type WorldId = [u8; 16];
pub type Digest = [u8; 32];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerId {
    pub owner: OwnerId,
    pub character: CharacterId,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Position {
    pub map: u32,
    /// Thousandths of a world unit. These are not metres or millimetres.
    pub xyz: [i32; 3],
}
impl Position {
    pub(crate) fn distance_squared(self, other: Self) -> Option<u64> {
        if self.map != other.map {
            return None;
        }
        Some(
            self.xyz
                .into_iter()
                .zip(other.xyz)
                .map(|(a, b)| {
                    let d = i64::from(a) - i64::from(b);
                    (d * d) as u64
                })
                .sum(),
        )
    }
    pub(crate) fn in_range(self, other: Self, min: u32, max: u32) -> bool {
        self.distance_squared(other)
            .is_some_and(|d| d >= u64::from(min).pow(2) && d <= u64::from(max).pow(2))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcId {
    pub spawn: u64,
    /// Distinguishes a corpse from the next creature at the same spawn.
    pub generation: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Target {
    Npc { id: NpcId },
    SelfPlayer,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Move {
        delta: [i32; 3],
    },
    Melee {
        target: NpcId,
    },
    Cast {
        ability: u32,
        target: Target,
    },
    AcceptQuest {
        quest: u32,
        giver: NpcId,
    },
    TurnInQuest {
        quest: u32,
        giver: NpcId,
    },
    OpenLoot {
        target: NpcId,
    },
    TakeLootMoney {
        target: NpcId,
    },
    TakeLootItem {
        target: NpcId,
        /// Stable, zero-based index in this corpse generation's loot_items.
        slot: u8,
    },
    ReleaseLoot {
        target: NpcId,
    },
    /// Legacy original headless API: atomically take all remaining corpse rewards.
    /// Stock interaction adapters must use the explicit open/take/release commands.
    Loot {
        target: NpcId,
    },
    /// Original slice policy: revive at the configured start after a fixed delay.
    Revive,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub world: WorldId,
    pub tick: u64,
    pub player: PlayerId,
    pub sequence: u64,
    pub command: Command,
}
/// The caller supplies the owner established by its authenticated transport or local
/// profile. A self-asserted owner from untrusted JSON is not authentication.
#[derive(Clone, Debug)]
pub struct OwnedInput {
    pub authenticated_owner: OwnerId,
    pub input: Input,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestProgress {
    pub counts: Vec<u16>,
    pub turned_in: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CastState {
    pub ability: u32,
    pub target: Target,
    pub completes_at: u64,
}
/// Original plain bag stack identity; no equipment stats or native item semantics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InventoryItem {
    /// Monotonically allocated per player, never reused, and preserved on merges.
    pub instance: u64,
    /// Stable, zero-based bag position. Inventory is serialized in ascending slot order.
    pub slot: u16,
    pub item_entry: u32,
    pub count: u16,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Player {
    pub id: PlayerId,
    pub position: Position,
    pub health: u32,
    pub mana: u32,
    pub xp: u64,
    pub level: u8,
    pub money: u64,
    pub next_sequence: u64,
    pub melee_ready_at: u64,
    pub global_ready_at: u64,
    pub abilities: BTreeSet<u32>,
    pub cooldowns: BTreeMap<u32, u64>,
    pub casting: Option<CastState>,
    pub quests: BTreeMap<u32, QuestProgress>,
    pub inventory: Vec<InventoryItem>,
    /// The next unused instance ID; u64::MAX means no new stack can be allocated.
    pub next_item_instance: u64,
    pub active_loot: Option<NpcId>,
    pub revive_at: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Npc {
    pub id: NpcId,
    pub health: u32,
    pub tagged_by: Option<PlayerId>,
    pub melee_ready_at: u64,
    pub respawn_at: Option<u64>,
    /// Derived: dead and all fixed rewards have been taken (or none were generated).
    pub looted: bool,
    pub loot_money: u32,
    /// Death initializes these fixed slots once; a take replaces a slot with None.
    pub loot_items: Vec<Option<benilla_world_data::ItemStack>>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub world: WorldId,
    /// Incremented once for every committed fixed tick.
    pub revision: u64,
    pub tick: u64,
    /// Reserved, persisted world identity input; the original slice has no randomized mechanics.
    pub seed: u64,
    pub players: Vec<Player>,
    pub npcs: Vec<Npc>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rejection {
    Dead,
    Alive,
    NotReady,
    BusyCasting,
    OutOfRange,
    InvalidMove,
    UnknownTarget,
    StaleTarget,
    TargetDead,
    PassiveTarget,
    UnknownAbility,
    InvalidTarget,
    InsufficientMana,
    UnknownQuest,
    WrongQuestGiver,
    QuestLevel,
    AlreadyAccepted,
    QuestNotAccepted,
    QuestIncomplete,
    AlreadyTurnedIn,
    LootNotOwned,
    AlreadyLooted,
    LootNotOpen,
    LootMoneyEmpty,
    LootSlotEmpty,
    InvalidLootSlot,
    InventoryFull,
    QuestLogFull,
    ProgressionLimit,
    UnsupportedMechanic,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Outcome {
    pub player: PlayerId,
    pub sequence: u64,
    pub rejection: Option<Rejection>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Event {
    DamageNpc {
        id: NpcId,
        player: PlayerId,
        amount: u32,
    },
    DamagePlayer {
        player: PlayerId,
        amount: u32,
    },
    Healed {
        player: PlayerId,
        amount: u32,
    },
    NpcDied {
        id: NpcId,
        owner: PlayerId,
        xp: u64,
    },
    PlayerDied {
        player: PlayerId,
    },
    Respawned {
        id: NpcId,
    },
    CastStarted {
        player: PlayerId,
        ability: u32,
        target: Target,
        completes_at: u64,
    },
    CastFinished {
        player: PlayerId,
        ability: u32,
        target: Target,
    },
    CastFizzled {
        player: PlayerId,
        ability: u32,
        reason: Rejection,
    },
    QuestRewarded {
        player: PlayerId,
        quest: u32,
    },
    LootOpened {
        player: PlayerId,
        id: NpcId,
    },
    LootMoneyTaken {
        player: PlayerId,
        id: NpcId,
        amount: u32,
    },
    LootItemTaken {
        player: PlayerId,
        id: NpcId,
        slot: u8,
    },
    LootReleased {
        player: PlayerId,
        id: NpcId,
    },
    /// Legacy headless atomic-take event.
    Looted {
        player: PlayerId,
        id: NpcId,
    },
    Revived {
        player: PlayerId,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TickResult {
    pub outcomes: Vec<Outcome>,
    pub events: Vec<Event>,
}
/// A future join protocol may authenticate and transfer this exact boundary.
/// This is not membership, consensus, host election, or divergent-world merging.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JoinBoundary {
    pub world: WorldId,
    pub revision: u64,
    pub tick: u64,
    pub state_digest: Digest,
    pub content_digest: Digest,
    pub rules_digest: Digest,
}
