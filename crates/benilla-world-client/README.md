# World/client boundary

`benilla-world-client` is a tested, engine-free integration seam between the
original deterministic world runtime and existing `benilla_protocol::SessionEvent`
types. It does **not** enable a new game mode, join remote players, replace the
profile store, claim native Classic support, or make the stock client render
original fixture IDs. All source is original GPL-3.0-or-later.

## What is implemented

- `ClientCatalog`: exact-presence checks against validated client build 5875 maps,
  creature/item displays and spells, plus a manifest hash and selected-player
  power type. Full item templates, quest templates/request/offer text, and giver
  greetings must also be supplied and match the supported world definitions.
  Missing metadata is an error; production code never synthesizes it. The manifest
  must bind this entire immutable client/server presentation bundle. The current
  runtime is mana-only, so other/unknown power bindings fail. No ID aliases or fallback models/spells exist; imported text is escaped as plain
  text so stock UI hyperlink/color prefixes cannot activate
- `CommandAdapter::local`: translates typed intents while deriving world ID, tick,
  sequence and owned `PlayerId` from the selected world. No GUI-supplied reward,
  owner, character, tick or sequence is accepted
- `Envelope` and `CommandAdapter::remote`: bounded/strict payload parsing, followed
  by exact world/content/rules/client-manifest/frozen-membership/base-revision/base-state/owner/character/
  tick/sequence checks. The owner comes from an authenticated session, never the
  envelope. Invalid peer inputs can be discarded before a world tick is batched
- `Projection`: initial and incremental NPC create/value/destroy/name events;
  selected player health/max-health/mana/max-mana/level/XP/next-XP/money/quest-log
  descriptors; exact-ID spellbook, pending-cast and cooldown restoration; cast
  starts/finishes/failures, kill XP, kill-objective toasts and quest reward events
- `NpcHandle`: captures a GUI GUID into a spawn+generation before queuing an
  interaction. An old captured command cannot be retargeted after respawn
- Stable item instance GUIDs, ItemCreate/count updates, backpack slot descriptors,
  externally validated ItemTemplate queries and accepted-grant ItemPushResult
- Explicit OpenLoot/TakeLootMoney/TakeLootItem/ReleaseLoot commands and protocol
  loot window/removal/money/release events. Opening never grants a reward; restore
  reopens only the persisted remaining slots. Capture the active NpcHandle at UI
  receipt before queueing slot/money requests, so a later window switch cannot
  retarget them. Legacy atomic headless Loot is rejected by this adapter
- Read-only quest status/greeting/template/detail/progress/offer queries, with
  actual giver generation, relations, eligibility and panel-distance checks. Only
  the guarded TurnInQuest command grants the confirmed reward

Projection consumes `World::last_tick()` directly, not caller-supplied events. It
requires every tick and the exact previous state digest; it fails rather than
silently lose events or project a different world's branch. Construct a fresh projection for an explicitly restored world boundary.
Project first, then acknowledge its saved checkpoint: acknowledgement intentionally
clears the completed journal records.

Native48 initialization receipts remain in the bound state across fresh and
resumed projections. Initial-cast genesis consumes no gameplay cast ID, input
sequence or projected cast packet. The previous-state digest uses the same
native51 boundary as runtime replay; native50 and older digests cannot advance it.

Source-owned creature generations project their selected virtual mainhand display,
packed class/subclass/material/inventory/sheath fields and initial unit sheath/misc
bytes on creation, updates and respawn. Closed offhand/ranged fields are explicit
zero. Retained ordinary spawn/template addons select the projected sheath byte
with whole-row spawn precedence; mounts, stand/emote state and addon auras remain
unsupported. Disarm retains these fields while the generation owns its reduced damage.
Selected raw item metadata must match the client catalog and bind a display; no
item object or inventory ownership is invented. This is a protocol consistency
check, not visual proof from client assets.

The opt-in native adapter projects validated EventAI SAY delivery receipts as
stock `SessionEvent::Chat` monster-say messages. It preserves callback-bound NPC
generation, inline localized name/text, Universal/Common language and player
addressee, in journal order before corpse destruction. Audit-only evaluations,
checkpoint bootstrap and historical replay do not generate chat. Projection
stages all events before committing its cursor; a failed frame returns no partial
speech. This headless boundary does not prove stock UI assets, language fluency,
bubbles, animation, or production native admission.

NPC GUIDs retain the protocol's creature entry bits and use a world/spawn/
generation-derived counter. Any active collision is rejected, never merged. Player
GUIDs use the same owner+character hashing domain as the saved-character app. These
are presentation GUIDs; simulation/network identity remains the complete typed ID.
Item GUIDs use the item namespace and world/owner/character/persisted-instance
hash; stack merges preserve their GUID and slot, and active collisions are rejected.

Classic UI bounds are checked rather than truncated: at most 20 quest templates
in this initial projection slice, each objective count at most 63, 16 backpack
slots, 16-bit initial spell IDs and u32 displayed XP/money. The larger headless
runtime limits do not imply the stock UI can represent the same domain.

## Explicit enabling gates and remaining app work

The untouched original fixture is refused because its creature/item display IDs
are zero and its original ability IDs have no validated client bindings. The
adapter tests use an explicitly authored mock catalog and authored small IDs.
They prove translation, not a real game install or graphical play.

Presence in a DBC table proves only that an ID can be resolved. It does not prove
that imported gameplay semantics match that spell, class, item or creature.
`World` separately refuses native Classic content until real ports exist. Keep
both gates. Do not map Practice Spark to Fireball, insert a fake Classic spell,
choose an arbitrary character model, or lift native-source rejection to make a
screenshot work.

The app integration contract is [APP-CONTRACT.md](APP-CONTRACT.md). The smallest
native starter slice and exact next port entrypoints are described in
[NATIVE-STARTER.md](NATIVE-STARTER.md). Melee autoattack scheduling, combat animation/logs, authoritative movement
feedback and player-death UI transitions are still absent. These must be
implemented and tested before claiming a complete stock-UI interaction loop.
Unsupported ClientCommand variants must get explicit feedback; they cannot be
silently treated as success. [INTERACTIONS.md](INTERACTIONS.md) documents the
ported workflow shape, exact APIs and retained original policies.

Run `cargo test -p benilla-world-client` and
`cargo clippy -p benilla-world-client --all-targets -- -D warnings`.
