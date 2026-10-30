# Original mainhand membership

Native81 (quest reconstruction v13, recipe v4, ancillary v2 and inventory host v14) adds a bounded `EquipmentMove` path for the **same source-initialized
mainhand object**, between slot15 and an empty ordinary backpack slot. It does
not enable native admission, acquire a new weapon, replace an occupied slot,
repair equipment or supply missing source data. The existing raw spell consumer
and private compiler remain unchanged.

## Source and supported domain

Pinned CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`,
`Player::SwapItem`, `RemoveItem`, `EquipItem`, `_ApplyItemMods`,
`_ApplyItemBonuses`, and `StatSystem.cpp` govern the operation. The existing
fresh mage initializer, original item registry, complete prototype, retained
weapon/unarmed skills, neutral full-raw spell authority, equipment dependencies
and derived-stat owners are required. Only pristine original mainhand weapons
with a positive Delay and one physical damage line are supported. Offhand,
ranged, item sets, enchantments, equip spells and unknown source effects remain
excluded by their existing owners.

Source weapons can change while in combat. This host deliberately returns
`UnsupportedMechanic` for that case because the weapon-switch timer reset/GCD
callback is not composed here; it does **not** invent the armor-only source
error60. Source death and equip-casting guards remain, while removing the
weapon does not acquire an equip-casting guard. The later separately versioned
[mainhand AutoEquip handler](MAINHAND-AUTO-EQUIP.md) admits only original types
17/21 into empty slot15. Registered backpack relocation, AutoStore and destruction
remain armor-only. Generic requirement
consumption remains blocked for registered originals.

## Separate equipment and cached weapon observations

Removing the item invokes its stat callbacks while the old item remains in the
source equipment array. The weapon tail then clears cached base damage to0,
sets the attack-time field to2000 and refreshes physical damage. Inserting it
occurs before its stat callbacks: critical-skill observation sees the inserted
item, but attack-power callbacks still read the preceding cached damage fields
until the weapon tail writes the item damage and Delay. The ancillary callback
adapter represents these observations separately and preserves callback order.

An inactive original is **not broken**, and it is not a never-equipped player.
Never-equipped source damage starts at1..2; a removed original leaves cleared0
rows, including on later attack-power updates. Actual attack skill is unarmed162
while detached. `UpdateWeaponDependantStats` updates hit chance, not an invented
full critical/damage refresh. Out of combat, the attack-time field changes but
existing BASE/OFF countdowns are not reset.

## Persistence and presentation

The existing required placement receipt and source-replayed recipe prefix bind
exact custody, original identity, item facts, resource clamps and derived
callbacks. No object is cloned and no allocator position is consumed. Historical
melee/cast contexts use their own validated prefix rather than the current
loadout. Snapshot witnesses establish owner-bound reachable state; complete
TickRecord replay establishes the recorded command trajectory. Previous native
schemas reject; there is no save migration.

The stock inventory command route and existing semantic item/slot/visible-entry
projection carry the same object GUID. Checkpoint publication still withholds
all staged effects until durable save succeeds. Headless controller/parser tests
are not a rendered player-build or user verification claim.
