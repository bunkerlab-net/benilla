# Source action-bar visibility byte

The existing stock `SetActionBarToggles` command now reaches the native owner for
profiles with the existing source-backed packed-player-field closure. It retains
all **256 byte values**, projects byte 2 of `PLAYER_FIELD_BYTES` (**field1222**), and
survives checkpoint/reopen. This is display configuration, **not action-button
contents**: no starter bar layout, spell placement, macro, or item action is made
up. Tests are component/controller tests, not rendered/user-verified gameplay or
production native admission.

## Source

CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`:

- `Entities/MiscHandler.cpp:896`, `HandleSetActionBarTogglesOpcode`, reads uint8 and
  calls `SetByteValue(PLAYER_FIELD_BYTES,2,value)`. Its pre-login null-player guard
  is already satisfied by a live native controller. There is no life/combat guard.
- `Entities/Object.cpp`, `_InitValues`, zero-initializes descriptors. `SetByteValue`
  masks only the selected byte and does not dirty an unchanged value.
- `Entities/Player.cpp`: load14151 and save15756 retain the byte; `KillPlayer`4426
  modifies only low-byte `PLAYER_FIELD_BYTE_RELEASE_TIMER`.
- `Entities/UpdateFields.h`: `0x40a + UNIT_END(188) = 1222`.

## Explicit packed-field ownership

A single shared eligibility helper is used by live commands, saved-state restore,
and projection. It requires the immutable source `CombatGenesis` fresh mage and
`QuestProgressionGenesis`, not caller-supplied absence flags. Incomplete profiles
reject the command, cannot restore a nonempty toggle ledger, and do not gain a new
alive field1222 projection. Their previous corpse projection is preserved.

The other packed bytes are not casually assumed zero:

| Bits | Existing source/producer closure |
| --- | --- |
| byte0 tracking0x02 | Fresh initialization; `starter_init::apply_aura_modifier` exhaustively accepts numeric/stat/rep holders and rejects aura151. Training reuses that dispatcher. Current runtime player spells are bounded direct/periodic damage; the incoming player aura owner is typed movement slow, not tracking. |
| byte0 release0x08 | Existing retained death owner, exactly **Corpse** phase and its captured `map_instanceable` configuration. |
| byte0 no-release0x10 | No admitted producer; existing death domain excludes battleground/repop/ghost paths. |
| byte1 combo | Fresh mage initialization (`class8`, fresh level1, no persisted character input); supported runtime effects/commands do not produce combo points. |
| byte2 | This owner, raw uint8, source-zero initial value. |
| byte3 highest honor | Fresh source initializer plus the validated kill catalog covering every profile NPC entry and rejecting civilian/racial-leader/group honor dependencies. No PvP-player target or honor setter is admitted. |

Relevant checks are in `starter_init/mod.rs`, native `combat.rs`, and
`quest_progression/kills.rs` / `player_progression/kill_xp.rs`.
**DeathSupport alone does not prove combo/honor/tracking absence.** New producers
must extend this composition, not silently overwrite their bits.

Projection emits **one** field1222 word: toggle byte2 plus the existing source
release flag. Changing the UI byte cannot erase that flag. Existing consolidated
player-field delivery may repeat unchanged words; it is not a claim of exact
upstream dirty-mask packet timing. The UI getter waits for the descriptor echo;
no separate acknowledgment is invented.

## Persistence and limits

Changed writes retain actual command sequences, replay from zero, and must be
strictly increasing below `next_sequence`; final value must match replay. Invalid
or omitted required fields, divergent/re-signed values, and malformed receipts
reject. Authenticated tick inputs remain sender provenance; this internal ledger
is not cryptographic proof against rewriting an entire history.

The explicit host limit is 2048 **changed** writes. Same-value requests still
succeed at capacity without adding a receipt. Overflow fails atomically and is a
host resource limit, not an upstream rejection. Controller checkpoint failures
withhold the descriptor; retry publishes the retained batch without executing
the command twice.

Native format `NATIVE_FORMAT_VERSION` rejects earlier context. No migration, ghost/release support,
private capture/compiler work, or native admission is added.
