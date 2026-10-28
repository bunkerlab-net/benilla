# Ordinary backpack storage and movement

`inventory` is a bounded GPL-3.0-or-later translation of ordinary
`CanStoreNewItem(NULL_BAG, NULL_SLOT)`, `StoreNewItem` and inventory-only
`SwapItem` from CMaNGOS Classic
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`. The upstream grant is
GPL-2.0-or-later; see [PROVENANCE.md](PROVENANCE.md). Its policy is
`cmangos-8ec338a1-carried-items-v9`.
The separately named positive admin policy is `CmangosAddItemV1`.

## Exact source behavior

- Unique `MaxCount` includes source-owned equipped counts and current
  backpack stacks. Slots 23–38 are searched in source order: compatible partial
  stacks first, then empty slots. Capacity and unique errors retain the exact
  partial destinations and remaining count. If both apply, the source returns
  inventory-full until the unique-permitted portion fits.
- A shared preflight destination vector reserves whole positions across calls.
  It does not reserve remaining stack capacity or aggregate unique counts. Every
  unique check sees unchanged actual inventory, including equipment. Each actual
  store recomputes a fresh plan against then-current inventory.
- Any non-OK result refuses the entire store, without mutation or callback.
  Partial plans are diagnostics, never permission for a partial loot grant.
- Creation reserves the original identity first, even for a complete merge.
  Each empty destination before the last allocates a clone; the last destination
  uses the original when empty. Merges preserve existing identities. The returned
  instance is at the last destination, whose post-count can differ from the full
  awarded count.
- The callback corresponding to `ItemAddedQuestCheck` runs after original-item
  creation and before any `_StoreItem` operation. It sees pre-store counts and
  the allocator advanced once. `Stored` includes the full awarded count, exact
  destinations, before/after slots, and last instance/slot/count.
- Creation initializes spell charges and full durability from the template.
  Carried bonding 1/4 binds; bonding 2 does not bind an ordinary backpack item.
  Equipped starter binding is separately owned by its initializer.
- Carried requirement destruction follows ascending backpack slots. The host can
  obtain exact removal counts, then apply each first-stack step at the source
  callback position. Equipment is never mutated by these helpers.

## Admission and transaction boundary

Inputs are loader-resolved templates, without inferred defaults. Stackable must
be 1–255, matching `ObjectMgr::LoadItemPrototypes` normalization and the source's
`ItemPosCount::count` uint8. Unnormalized zero/negative/>255 values reject. Award
count is independently bounded to 1–10,000 and can exceed one stack. The backpack
has sixteen positions; equipment is at most nineteen immutable units. Template
entries, per-instance IDs/counts, total unique counts, metadata and allocator are
checked, including on restore. This profile admits nonnegative `MaxCount` only;
zero means unlimited. Instance IDs are checked u64 continuation identities, not
wire GUIDs. Zero and duplicate IDs, IDs at/after the next allocator value, or
allocation overflow reject. Removing an item never reuses its identity.

The catalog preserves structurally valid loader-resolved rows even when a particular
operation is unavailable. A row's presence is not a grant, equip or use capability.
`ordinary_loot_ready` retains the original restrictive loot/reward closure.
`grant_ready` separately admits carried creation of ordinary items and real empty
Bag objects, including retained five-slot spell records (IDs, triggers, charges,
PPM-rate bits, cooldowns and categories), item sets and StartQuest. None of those metadata fields
executes an equip/use effect merely because an item is stored. Equip and use
readiness remain explicit failures; they are not inferred from grant success.

Storage requires evidence that creation/loot scripts, equipped bags, bank/keyring
items, trade/temporary loot and additional mutations are absent. The last audit
means mutations beyond the policy's explicitly named storage, positive grant,
requirement-removal, backpack-swap and explicit backpack-split paths; it does not admit equip, use,
equipped containers or arbitrary item editing. Unknown or present
evidence blocks the operation without hiding the catalog row. Nonzero ScriptId,
random properties, duration, area/map lifecycle restrictions and unreviewed flags
remain explicit grant blockers. BagFamily is the Classic enum, not a bitmask;
keys (9) require the missing keyring route. Other families reach the backpack in
this no-equipped-bag domain. No input field is zeroed to pass a gate.

## Corpse-carried acquisition

`corpse_carried_ready`, `can_store_corpse_item` and `store_corpse_item` use
`cmangos-8ec338a1-corpse-carried-items-v1`. They reuse the same all-or-nothing
planner, original/clone allocation, pre-store quest callback and storage path.
They never call the admin grant handler or unbind destinations. The ordinary
`can_store`/`store_new`, starter and quest-reward readiness remain unchanged.

This separate domain retains unused SpellId0 metadata, including -1 cooldowns
and nonzero charges. Item::Create copies all five charges regardless of SpellId.
Real trigger0 on-use spells are retained without execution: the pickup/store path
does not call CastItemUseSpell or equipment modifiers. Other real spell triggers
remain blocked. All fields, including inactive cooldown/category/PPM values, stay
unchanged in catalog identity. No use, eating/drinking, equipment or class-based
equip capability follows from acquiring an item.

Ordinary empty bags require class1, InventoryType18, BagFamily0, Stackable1 and
1–36 ContainerSlots. Their instance retains the source slot count and durability,
but has zero charges even if template charges are nonzero. They consume existing
backpack positions without extending capacity or admitting contents. Quivers,
malformed bag shapes, nonzero bag families, StartQuest, sets and locks remain
blocked, together with existing random-property/script/duration/location/flag
and audit guards. The source loader must still establish resolved input rows.

The existing original-source oracle additionally runs 320 authored corpse cases
through unchanged CanStoreNewItem/StoreNewItem, comparing complete metadata,
pre-state, capacity/unique refusals, callback position, destinations, allocator,
binding and final instances. No admin operation or partial grant supplies the
expected results. Native collection stages the real quest callback and corpse
mutation in its existing atomic frame; native35 identity separates this expanded
accepted domain from native34 without migration.

## Positive admin grants and empty containers

`State::grant_item` implements the positive branch of
`ChatHandler::HandleAddItemCommand` from pinned `Chat/Level3.cpp`. It recomputes
trusted destinations and subtracts the source no-space count. The durable result
records requested, granted and ungranted counts, the original storage result,
ordered destinations, allocator boundaries and complete before/after instances.
A zero grant retains its complete pre-state capacity witness but has no creation,
callback, allocation or notification. Unique limits
include immutable equipment; there is no negative deletion, overflow mail or
unique bypass. Ordinary `store_new` still rejects the entire non-OK plan.

Original allocation precedes `ItemAddedQuestCheck`; its count is the actual
partial award and it sees pre-store inventory. Creation hooks, duration hooks
and enchantment hooks run in the source oracle; the supported Rust domain proves
their dependent routes absent. A newly placed destination gets its source binding;
merges preserve metadata and source IDs. After all storage, self-target `.additem`
unbinds every destination, including an old merged stack. Other-target source
semantics preserve binding. The current native admin host only admits its selected
self character; other-player routing is not implemented by this component slice.

`NewItemOrBag` dispatches only on InventoryType 18. Bag::Create accepts at most 36
slots and initializes empty container slots, ownership and durability, but unlike
Item::Create does not initialize template spell charges or duration. The
`EmptyBag { slots }` instance kind is an explicit all-empty container invariant;
serialization cannot smuggle contents into it. Its protocol object is a Container,
with the exact capacity and 36 zero slot GUIDs. A carried empty bag occupies one
backpack position and does not add usable slots. Equipping bags and storing their
contents are the next distinct implementation, not an implication of this grant.

`State::validate` checks current values but cannot authenticate immutable genesis
or prove a legal history. The runtime must compare equipment to its genesis and
validate its own item receipts, authority, quest state and checkpoint ancestry.
Catalog hashes use a policy-separated explicit little-endian encoding of sorted
entries and all template/audit fields, independent of Debug formatting.

`store_new` preflights IDs before invoking the callback and stages all inventory
changes. Failure leaves this `State` unchanged. A callback must mutate only the
caller's isolated host, never publish external effects: the caller atomically
commits/discards inventory and quest changes together. Internal failures differ
from legitimate source storage refusals. There is no lifetime receipt-count cap
in the storage primitive.

## Independent source differential oracle

Run:

    python3 crates/benilla-sim/tools/generate_inventory_fixture.py UPSTREAM --check

UPSTREAM is the pinned repository root. The generator verifies complete-file
SHA-256 identities before extracting unchanged `_Spell`, `InventoryResult`, `ItemPosCount`,
`isContainedIn`, `CanBeMergedPartlyWith`, `_CanTakeMoreSimilarItems`,
`_CanStoreItem_InInventorySlots`, `_CanStoreItem_InBag`, complete `_CanStoreItem`,
`Item::Create`, `CreateItem`, `CloneItem`, `StoreNewItem`, `StoreItem`, and
`_StoreItem`. It compiles an authored adapter with warnings as errors and UBSan.
Unsupported specific-slot, equipped-bag and other routes abort rather than return
invented answers. The adapter supplies only the declared no-script/no-modifier
object, allocator and inventory context.

The fixtures contain 1,577 planning and 515 ordinary creation outcomes, plus
1,024 positive admin grants and 216 direct Item/Bag creation outcomes from synthetic
rows. The original two fixture files remain byte-identical after adding grants.
The admin fixture compares complete input/output state, partial/zero counts,
self/other binding, clone identities, quest callback position and bag initialization.
The generator also requires random-property, script, duration and keyring routes
to abort when their unsupported dependency is deliberately introduced.
They cover partial capacity/unique refusals, immutable equipment, shared reserved
positions, uint8 stack boundaries, original/clone IDs, full merges, last returned
instance, pre-store callback count and allocator, bonding and durability. Rust
compares every result/destination/remaining count and complete stored state.
Separate regressions cover exact 255+1 splitting, two independent unique reward
preflights, malformed/restored inputs, excluded dependencies, allocator exhaustion,
callback rollback and ordered requirement removal. No native database rows,
client assets, networking or playable-world claims are included.

## Backpack move, swap and merge

`State::swap_backpack` translates the bounded `Player::SwapItem` branches for
source and destination slots 23–38 in bag 255, including the same-position guard
from `HandleSwapInvItemOpcode` and `HandleSwapItem`. The separately typed policy
is `MovementPolicy::CmangosBackpackSwapV1`. Unsupported positions and invalid
state fail before any mutation; they are not invented source inventory errors.
All existing catalog, lifecycle and mutation-audit gates remain in force even
for an empty inventory or a request that would otherwise be ignored.

- Same-position input is ignored before calling `SwapItem`. An empty source is
  ignored before the alive check. An existing source on a dead player returns
  source `EQUIP_ERR_YOU_ARE_DEAD` (38), with both original item pointers available
  for the error packet. These outcomes leave all inventory unchanged.
- Moving into an empty slot preserves the original instance and count. Occupied
  different-item slots swap both instances. Matching ordinary items first attempt
  merge only when the destination is not full. A full destination takes the real
  swap branch, even if entry and stack counts match.
- A complete merge destroys the source instance identity and preserves the
  destination identity. A partial fill retains both identities: destination fills
  to Stackable and the remainder stays at the original source position. It never
  distributes the remainder into other positions, even though `CanStoreItem`
  preflights a complete placement. The original source slot supplies enough
  capacity because planning ignores the moving source object.
- `MaxCount` checks ignore the moving source while including the remaining
  backpack and unchanged source-owned equipped counts. No acquisition or removal quest callback
  runs: total item counts are unchanged. Movement never advances the allocator,
  so even the maximum continuation identity can still move existing items.
- `_StoreItem` binds bonding 1/4 on empty-slot moves, both halves of real swaps,
  and complete merges. This can rebind previously self-granted admin items.
  Partial fill only changes counts and preserves both binding values. Binding 2
  does not bind in backpack slots. All other per-instance metadata is preserved.
- Real empty carried Bag objects participate in moves and swaps, never merges.
  Their contents remain empty and they provide no usable slots. Equipped bags,
  storing bag contents, equipment, bank, keyring and use remain unsupported.
  Explicit ordinary backpack splits are separately covered in `INVENTORY-SPLIT.md`.

The source real-swap branch ends in `AutoUnequipOffhandIfNeed`. Its active branch
requires both a mainhand InventoryType 17 item and a nonempty offhand. The
entry-count-only `equipped` map cannot locate actual slots, so the primitive
conservatively rejects an equipped type17 together with any offhand-compatible
type13/14/22/23, and rejects a total type17 count above one. This gate follows
`Player::IsTwoHandUsed` and `ViableEquipSlots` at the pinned revision. It returns
`Unsupported("backpack_movement_offhand_dependency")` atomically before source
execution, not an invented source error code. It may reject a harmless real
loadout whose slot evidence is unavailable. Equipment counts must still come
from a separately validated immutable loadout; this primitive does not authenticate
equipment genesis. The native initializer additionally excludes offhand slot16.
The extracted oracle routine runs on real swaps and its offhand/two-handed
dependent route has a dedicated abort probe.

Every `MovementReceipt` records the exact request/alive input, source result,
typed outcome and complete before/after `State`, including ignored and rejected
input. `validate` recomputes the entire transition. This proves local semantics,
not external authority: the host must bind the pre-state and alive value to its
authenticated character, phase and journal/checkpoint ancestry. The host executes
FIFO positional requests against the state at command execution time.

The independent movement oracle runs separately:

    python3 crates/benilla-sim/tools/generate_inventory_movement_fixture.py UPSTREAM --check

The generator verifies complete-file identities before extracting the entire
`SwapItem`, packet handler, specific-slot planning, `GetItemCount`, `RemoveItem`,
`AutoUnequipOffhandIfNeed`, existing store/merge methods and exact position guards.
Its 2,880 synthetic fixture cases compare complete input/output instances,
allocator, source result codes and error source/destination GUIDs. C++ adapters
also assert zero acquisition callbacks/notifications and exact slot, owner,
contained and root-GUID consistency. Compilation uses warnings as errors and
UBSan. Clone allocation, scripts, duration, equipment, bank, equipped-bag and offhand
probes must abort, rather than return an invented successful result.
Focused Rust tests also cover all sixteen-by-sixteen position
pairs, immutable equipment, allocation exhaustion without allocation, filled
backpacks, metadata/receipt tampering, excluded topology and strict audit failure.

Selected ordinary-backpack destruction is specified in
[INVENTORY-DESTROY.md](INVENTORY-DESTROY.md): the exact selected instance is removed
only after source quest callbacks, with typed no-user-destroy protection and
strict checkpoint/projection ownership. Other item topologies remain excluded.

The v8 bounded `autostore_backpack` handler retains the source instance, searches
other backpack stacks then empty positions, and emits no acquisition/removal
quest hook. See `../benilla-world-runtime/src/native/INVENTORY-AUTOSTORE.md`.

## Separately owned original armor membership (v9)

The ordinary inventory reducers still do not implement equip. Native quest
progression may compose one separately validated operation: an original pristine
nonweapon armor instance moves between its genesis slot and an empty backpack
slot. That owner requires the immutable registry, full raw source authority,
exactly-one custody, unchanged allocator, real modifier replay and checksum
receipts. Without that owner, equipped counts remain the immutable initial counts.
Generic equip/use/bag contents and damaged/BoE carried objects remain unsupported;
ordinary instance validators are unchanged. This is a versioned host-composition
exception, not a claim that `additional_item_mutations: Absent` grants equipment
permission. See `benilla-world-runtime/ARMOR-MEMBERSHIP.md` for the narrower scope.
