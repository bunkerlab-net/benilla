# Current-owner corpse appearance preview

`NativeWorld::corpse_appearance_preview` is a read-only observation, not corpse
creation or spirit release. It composes the existing GPL simulation appearance
assignments with actual validated native owners. It does not enable native entry.

The immutable result retains the current checkpoint identity (world, selected
player, profile, tick, state digest and journal tip) and appearance words. Its
fields cannot be constructed or deserialized by callers. `validate_for` compares
them with a fresh observation; the digest is not keyed authentication or proof
of source-server chronology. Capturing a living player is explicitly permitted.
No sequence, state, event, allocation or durable save is changed.

## Inputs and exclusions

- Exact loaded Map classification must be present and non-battleground. Unknown
  or absent evidence is not interpreted as ordinary-map proof.
- Selected fresh-character appearance, native display/collision and the existing
  supported hide-flags owners must all exist. Current display is not substituted
  for native display. All five appearance choices remain source-bound.
- Every occupied **current** combat equipment slot resolves its original registry
  identity and validated prototype. All 19 slots are represented, including
  broken and hidden items. Detached armor is not worn merely because its genesis
  registry entry remains. Missing metadata is an error, not an empty slot.
- Source `Player::Create` initializes guild ID to zero. Source `BGData` initializes
  membership ID to zero. In this existing fresh, script-free supported host there
  is no guild setter, BG join, teleport, imported-character, GM or scripted
  producer that changes either. These are conditional host closure facts, not
  statements about arbitrary CMaNGOS configurations, SQL autojoin or custom
  scripts. No guild rank/timestamp or player guild descriptor is added.
- Map type and player BG membership are distinct: `CreateCorpse` uses
  `Player::InBattleGround()` for the lootable flag, and the map predicate for
  saving. The exact map row alone does **not** prove player membership absence.
  Both the loaded map restriction and the separate fresh-membership closure are
  required here. Existing hide-owner no-rest-entry restrictions remain intact.

## Source and integration boundary

CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`,
`Player.cpp` (`Create` and `CreateCorpse`) and `Player.h` (`BGData`,
`InBattleGround`, `SetBattleGroundId`, `SetInGuild`). New source-derived
composition resides in the existing GPL runtime; no package is relicensed.

The packing primitive preserves the full prototype `DisplayInfoID |
(InventoryType << 24)` operation and the native display, cosmetic and hide
words. This preview does **not** execute the surrounding `CreateCorpse` flow:
no preceding ghost casts, old-corpse retirement, placement, GUID, PvP-death reset,
save, owner registry or Map::Add. It cannot stand in for those operations even
when captured after actual player death. Ghost aura ownership, corpse lifecycle,
reclaim and player release remain separate unconnected requirements. No snapshot
schema or native format change is needed for a transient read-only result.
