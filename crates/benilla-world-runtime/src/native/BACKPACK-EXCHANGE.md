# Registered original backpack exchanges

Native format 81, inventory policy v18 and quest reconstruction v17 extend the
existing custody-only relocation owner to occupied backpack slots 23–38. This is
conditional native-component functionality, not production native admission.

## Pinned source behavior

CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`,
`Player.cpp:10715–11000` (`SwapItem`), supplies the source behavior. Missing source
and same-position operations remain ignored; an actual move requires alive but
has no backpack combat/casting restriction. Source attempts a merge before the
real-swap branch. Registered originals are exact pristine count-one, stackable-one
objects, so the admitted occupied transition is a swap, never a merge.

The real branch validates both directions, removes destination then source,
stores source at destination then destination at source, and invokes the existing
`AutoUnequipOffhandIfNeed` closure. The existing reducer checks storage dependencies
and excludes ambiguous two-hand/offhand loadouts. Both positions are backpack:
no equip modifiers, weapon cache recalculation, timer reset, visible equipment
change, or quest item-acquisition/removal hook is invented. StoreItem binding for
an ordinary counterpart remains the real reducer's behavior; it is not forced to
remain byte-identical. Container counterparts are an explicit unsupported host
subset, not an upstream equipment error.

## Exact custody and durable replay

The route checks both endpoints. An ordinary source cannot bypass registry
ownership merely because the original is the destination. A required `registered`
map is independently reconstructed from the complete before-inventory and sealed
genesis registry. Each affected original must have its exact origin, derived
pristine instance facts and current journal placement. Real `Moved` or `Swapped`
receipts are recomputed against the existing inventory catalog. Both registered
placements are updated when necessary; all untouched originals, retired handles,
carried instances, receipt ancestry, equipped counts and allocation frontier remain
validated. No new ID is allocated and no original ID becomes reusable.

This reuses `ItemRelocation`, so existing bounded history count, aggregate bytes,
source replay work and historical stat-prefix validation remain applicable. The
relocation receipt digest is v2. Previous native checkpoints are rejected, not
migrated. Combat recipes/resources/caches are not reconstructed as though an item
were equipped or unequipped. Normal unrelated tick effects and countdown progress
still occur. Source errors and ignored requests do not append a custody mutation.

## Projection and tests

Existing `InventoryMoved` projection represents the final semantic inventory
state: both backpack pointers identify the same original objects at their new
slots. This does not claim byte-identical transient RemoveItem/StoreItem packet
ordering. No item creation/destruction, item-push or equipment descriptor is
synthesized for the exchange itself.

Tests exercise both command directions, two original endpoints, current mainhand
cache preservation, actual casting/combat, actual-death refusal, forged endpoint
ownership, checkpoint and TickRecord replay. The controller test uses real Lua
backpack pickup commands, stock SwapInvItem, save-failure withholding, retry and
reopen. These are parser/controller tests, not rendered-player evidence.

The GPL-covered implementation builds on the existing CMaNGOS-derived reducer;
upstream is GPL-2.0-or-later and the Rust component GPL-3.0-or-later. No private raw
compiler/consumer or production admission path changes.
