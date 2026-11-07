# Loot, item and quest interaction contract

The original `World` adapter contract below remains separate from the source-bound
`NativeProjection` extension described at the end.

## Metadata gate

`ClientCatalog::item_metadata(entry)` must return a full externally validated
`ItemInfo`, not a sparse `PlainItem` padded into a native-looking template.
`quest_metadata(entry)` returns `QuestMetadata { template, request_text,
offer_text }`; `quest_greeting(creature_entry)` supplies the actual bound greeting.
The catalog manifest must identify the entire immutable client/server metadata
bundle. The adapter checks agreement with world IDs, names, displays, stacks,
objective slots and rewards and rejects unsupported mechanics. It cannot establish
rights or provenance merely from an implementer's assertion; the application must
construct this catalog from the user's validated local content/compiler output.

Only unrestricted, ordinary, non-equipped, non-usable, non-bound, non-unique plain
stacks are represented. Equipment stats, enchantments, damage, spells, durability,
requirements, containers, area restrictions and random properties fail the gate.
Supplied supported cosmetic/vendor tooltip fields are preserved; there is no
vendor transaction capability. Quests currently permit four kill-objective/fixed
reward slots and no choices, item objectives, reputation, source/reward spells,
flags, chains or special conditions. Request/offer/greeting strings are bounded,
control characters are rejected and all displayed strings neutralize UI escape
prefixes. The current workflow intentionally has no quest-panel emotes.

The original fixture remains a **headless** contract with placeholder displays.
Only tests construct clearly labeled original mock full metadata. Nothing here
maps the original abilities/items to unrelated real client records. Native-source
compilation and gameplay remain blocked independently of these display checks.

## Authoritative loot and durable items

The runtime supplies `OpenLoot`, `TakeLootMoney`, `TakeLootItem { slot }` and
`ReleaseLoot`, each bound to the exact spawn generation. It persists the active
window, remaining fixed corpse slots/money, bag slot and monotonic per-player
item-instance counter. Opening/reopening shows existing content without rolling or
granting it. A failed take grants nothing and consumes no corpse reward. Removing
slot zero does not renumber slot one. Empty corpses can be reopened while their
generation exists. Moving out of range, dying or respawning closes the window.
Release is idempotent and a stale release cannot close a newer generation/window.

The stock adapter captures the current window with `capture_loot(world)` when a
slot/money request arrives; the captured NpcHandle stays with the queued request.
It must never resolve that request against a newer window during admission.
Legacy whole-corpse `Command::Loot` is retained only in the headless runtime and
is refused by both adapter envelope creation and remote admission.

Projection emits LootResponse, LootRemoved, LootMoneyNotify/LootClearMoney and
LootReleaseResponse from authoritative events. Bags emit ItemCreate, count values,
backpack GUID slots and ItemPushResult only for actual positive grants. A merge
updates the same item GUID. Initial restore emits descriptors and remaining loot,
without replaying acquisition chat. Item GUID hashing includes world, full owned
player identity and persisted instance counter. Active collisions fail closed.

## Quest panels and confirmation

`quest_status`, `quest_greeting`, `quest_template`, `quest_detail`, `quest_progress`
and `quest_offer` return existing protocol events and cannot mutate the world.
Giver handles must refer to current live generations. Greeting/detail/progress/
offer additionally require a living selected player in the configured integer
interaction range. Status queries are allowed at visible-object distance.
Only represented start/finish relations and objective/level state determine rows;
completed accepted quests remain turn-in eligible after leveling past max_level.
No scripts, faction reactions or unrepresented prerequisites are assumed.

Details shows an available quest. Progress reports accepted objective completion.
Offer requires completion and displays the fixed reward. Neither asking for
progress nor opening/continuing the offer awards anything. The stock Complete
button with choice zero submits `TurnInQuest`, which rechecks the authoritative
conditions and atomically grants XP, money and inventory once. Accept/reward
results close gossip; quest descriptor state and giver status are refreshed.

## GPL workflow provenance and deliberate differences

The interaction separation is based on the GPL-2.0-or-later CMaNGOS source pinned
at `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`:

- [LootHandler.cpp](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Loot/LootHandler.cpp):
  `HandleLootOpcode`, `HandleAutostoreLootItemOpcode`, `HandleLootMoneyOpcode`,
  `HandleLootReleaseOpcode`
- [QuestHandler.cpp](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Quests/QuestHandler.cpp):
  status/hello/query/accept/complete/request-reward/choose-reward handler separation
- Existing `benilla-protocol` GPL descriptor/message definitions supply the
  supported build-5875 field and SessionEvent layouts

This is an original bounded implementation of the reference interaction shape,
not a line-for-line gameplay port. Fixed integer range, deterministic guaranteed
drops, first-damage ownership, static spawns, plain stack insertion, no open-time
cast interruption, no group loot, no scripts, empty-slot retention and original
quest/stat/XP policies remain deliberate deviations. Native numeric kernels in
other crates do not remove these capability gates. Full Classic loot, item,
quest or combat parity is not claimed.


## Native inventory and item rewards

`NativeProjection` uses the audited native inventory/loot catalog independently of
OriginalFixture restrictions. It validates exact item gameplay fields, starter
requirements/stats, and client display presence. Stable carried-instance GUIDs and
starter-equipment GUIDs must not collide. Carried item creates include count, charges,
binding and durability; backpack pointers always describe all sixteen slots.

Source callbacks determine packet order: item credit before storage, stack destruction
before reward creation, loot removal before its push, and quest items before XP.
Explicit release, invalid/consumed slot release and money-last exhaustion unregister
the looter before `SendReleaseForAll`, producing no release response. Item-last
exhaustion uses `SendReleaseForAll` while registered and produces one response.
Solo money emits clear-money, never `LootMoneyNotify`. The player looting flag follows
the durable target. A restored target is reconciled through initial delivery without
replaying item acquisition.

Native `ChooseQuestReward` admits source choice indices 0..5; empty slots are legal.
The original adapter explicitly refuses this native-only intent. Detail/template
responses honor hidden rewards, while offers show ordinary choice/fixed items.
Reward refusal preserves callback-time offer context, with an inventory error only
when the source sends one. Reward pushes use from-NPC
true, created false, chat false and bag 255; the slot is the last destination only
when its post-count equals the full award count. Completion reports fixed template
rows only, even if a later unique-item grant was legitimately skipped. Unsupported
item use, bags, equipment mutation and source/reward spell effects remain excluded.

The actual app controller owns durable publication and published-panel GUID-less
binding; its [contract](../benilla-app/src/net/p2p/world/native_controller/README.md)
and component tests exercise save/retry/reopen boundaries without Bevy linking.
