# Ordinary backpack storage

`inventory` is a bounded GPL-3.0-or-later translation of ordinary
`CanStoreNewItem(NULL_BAG, NULL_SLOT)` and `StoreNewItem` from CMaNGOS Classic
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`. The upstream grant is
GPL-2.0-or-later; see [PROVENANCE.md](PROVENANCE.md). Its policy is
`cmangos-8ec338a1-ordinary-backpack-v2`.

## Exact source behavior

- Unique `MaxCount` includes immutable equipped starter counts and current
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

Every catalog explicitly attests absence of creation/loot scripts, equipped bags,
bank/keyring items, trade/temporary loot and additional item mutations. Unknown
or present evidence rejects. Bags/quivers, keyring families, random properties,
item-started quests, duration, item sets, area/map/lock restrictions, nonzero item
spells/triggers/charges, extra flags and unsupported flags reject. Only the
single-owner-inert MULTI_DROP flag is accepted. Catalog admission does not permit
item use, equipment changes, selling, trading or arbitrary grants. Carried
instances have exactly their creation metadata because other mutations are
excluded. Actual native-source admission remains the caller's separate burden.

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
SHA-256 identities before extracting unchanged `InventoryResult`, `ItemPosCount`,
`isContainedIn`, `CanBeMergedPartlyWith`, `_CanTakeMoreSimilarItems`,
`_CanStoreItem_InInventorySlots`, `_CanStoreItem_InBag`, complete `_CanStoreItem`,
`Item::Create`, `CreateItem`, `CloneItem`, `StoreNewItem`, `StoreItem`, and
`_StoreItem`. It compiles an authored adapter with warnings as errors and UBSan.
Unsupported specific-slot, equipped-bag and other routes abort rather than return
invented answers. The adapter supplies only the declared no-script/no-modifier
object, allocator and inventory context.

The fixtures contain 1,577 planning and 515 creation outcomes from synthetic rows.
They cover partial capacity/unique refusals, immutable equipment, shared reserved
positions, uint8 stack boundaries, original/clone IDs, full merges, last returned
instance, pre-store callback count and allocator, bonding and durability. Rust
compares every result/destination/remaining count and complete stored state.
Separate regressions cover exact 255+1 splitting, two independent unique reward
preflights, malformed/restored inputs, excluded dependencies, allocator exhaustion,
callback rollback and ordered requirement removal. No native database rows,
client assets, networking or playable-world claims are included.
