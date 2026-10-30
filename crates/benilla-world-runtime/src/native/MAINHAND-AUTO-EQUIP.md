# Original mainhand AutoEquip

Native format 81, quest reconstruction v14 and carried-item host v15 extend the
existing separately identified AutoEquip operation to the same source-initialized
pristine original mainhand, only inventory types 17 (two-hand) and 21 (mainhand).
The original slot 15 must be empty. Type 13, replacement weapons, occupied
slots, imported objects, offhand/ranged and all other new equipment remain outside
this handler. Production native admission and held raw consumers are unchanged.

## Source and owner composition

Pinned CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`:
`ItemHandler.cpp::HandleAutoEquipItemOpcode`, `Player.cpp::CanEquipItem`,
`ViableEquipSlots`, `FindEquipSlot`, `EquipItem`, and `AutoUnequipOffhandIfNeed`.
The source initializer and retained registry identify the original object; actual
current custody supplies the carried source and exact empty destination. Both
initial and current equipment prove the absent offhand. The existing raw spell
and item callback closure is reused, not expanded or fabricated.

An empty carried source is silently ignored before eligibility checks. Unlike
SwapItem, AutoEquip calls CanEquipItem before CanUseItem: a blocking generic cast
returns source error 39 before dead error 38. Weapons skip the armor combat
refusal entirely. After real source refusals, an otherwise eligible combat switch
is rejected as UnsupportedMechanic because combat weapon-reset/GCD callbacks are
not implemented. Traveling projectiles do not block equipping. No new alive,
combat, or casting rule is attributed to upstream.

A successful operation invokes the same source-ordered mainhand membership
transition documented in [MAINHAND-MEMBERSHIP.md](MAINHAND-MEMBERSHIP.md): exact
original identity, ordered item modifiers, distinct equipment versus cached weapon
damage views, preserved out-of-combat countdowns, no resource refill. The required
AutoEquip discriminator and source singleton selection are replayed against exact
historical custody. No new item is allocated; no generic weapon consumption permission follows.
Separately versioned [carried custody](MAINHAND-CUSTODY.md) owns only bounded
original-mainhand relocation/AutoStore, not another equipment transition. Checkpoint and event publication remain atomic.

This is a connected conditional native command/projection path, not an assertion
that production entry is available or that a rendered player build was verified.
Old native snapshots are rejected; no migration is supplied.
