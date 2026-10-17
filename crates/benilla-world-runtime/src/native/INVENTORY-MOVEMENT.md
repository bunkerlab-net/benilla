# Backpack movement component boundary

The native runtime accepts owner-authenticated, sequenced `InventoryMove` commands
for the sixteen root backpack positions, 23 through 38. Each input resolves the
current slots at its own phase, matching the positional `CMSG_SWAP_INV_ITEM`
request. It does not capture a client-guessed item identity: an earlier queued
loot, reward or movement can legitimately change the slot before this input.

The simulation's named source policy and independent oracle are documented in
[ordinary inventory](../../../benilla-sim/INVENTORY.md). The runtime passes its
actual current health to the source alive gate. Same-slot and missing-source
requests remain source no-ops; an occupied-source request from a dead player
retains the source inventory error. Unsupported positions are not equipment
operations and are rejected without expanding inventory admission. The source
real-swap tail can auto-unequip an offhand item; the primitive conservatively
rejects ambiguous two-hand/offhand equipment counts rather than omitting that
callback. Current starter initialization supplies no offhand slot.

A movement receipt contains the exact phase-owned before/after inventory,
operation, result and source policy. Changed inventory updates per-instance
receipt ancestry within the enclosing atomic frame. Movement allocates no new
identity and emits no item acquisition or quest objective callback. Source
binding changes on storage are still applied; moving an admin-unbound item is
not permission to retain an otherwise-invalid unbound state.

The projection validates the receipt against its current inventory frame and
uses stable instance-derived GUIDs for item values, removals and player slot
references. It emits the source error once with the source-time item GUIDs.
There is no `ItemPushResult` for relocation. The existing controller checkpoint
and outbox transaction withholds these packets until the save is durable.
Checkpoint/replay recomputes movements rather than trusting supplied receipts.
Native format 49 rejects older experimental native saves; exploration character
profiles are unchanged.

This is a component integration, not live exploration inventory. Production
native entry remains closed. Bag equipping or contents, equipment, splitting,
item use, banking, trade and other item operations remain separate work.
Authored component and protocol tests do not establish asset-backed rendering
or a populated playable Classic world.
