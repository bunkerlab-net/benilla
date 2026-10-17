# Bounded EventAI text sources

`EventAiCatalog` retains a bounded, immutable, source-shaped dependency selection.
It is not a raw database reader, compiler, profile issuer, gameplay capability or
proof of a runnable Classic scene. `NativeCapability` remains uninhabited.

The executable component domain is AGGRO (4), zero inverse-phase mask/flags/event
parameters, one TEXT action (1) with one to three contiguous positive signed text
IDs, followed by two fully zero NONE actions. Chance is explicitly 1–100; raw
loader narrowing/capping outside that range is not approximated. Every owner
supplies complete entry and negative-database-GUID vectors in loader ID order;
entry events precede GUID events. Missing/unknown closure and all other applicable
shapes reject. Database GUID and stable runtime spawn identity are separate.

Referenced broadcasts and every associated raw locale row are retained. The
initial presentation domain is SAY, language Universal (0) or Common (7), with
zero sound/emote/delay fields. Raw ConditionID, EmotesID, Flags and VerifiedBuild
remain identity-bearing even where the pinned loader does not consume them.
Flags 1 does not imply a script or a newer wire protocol. No fleeing action is
inferred from a text string. NULL and empty raw text remain distinct inputs;
loader string access treats both as empty for presentation.

The selected female vector is overwritten whenever the default male text is
nonempty, matching `DoDisplayText`, rather than `BroadcastText::GetText`.
Per-recipient locale selection and empty-string fallback occur afterward. The
pinned core recognizes enUS/enGB, koKR, frFR, deDE, zhCN, zhTW and esES. Raw esMX,
ptBR and ruRU rows are preserved but ignored by that loader; those session locale
names resolve to enUS. Localized speaker names have independent complete coverage.
Text stays UTF-8 and Common stays language 7; server-side garbling is not added.

## Source anchors

Pinned CMaNGOS Classic commit: `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`.

- [EventAI loader and order](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/AI/EventAI/CreatureEventAIMgr.cpp#L210-L269)
- [Text dependency validation](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/AI/EventAI/CreatureEventAIMgr.cpp#L594-L614)
- [Holder initialization](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/AI/EventAI/CreatureEventAI.cpp#L102-L163)
- [Chance and text selection](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/AI/EventAI/CreatureEventAI.cpp#L598-L730)
- [Broadcast loader](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Globals/ObjectMgr.cpp#L7782-L7880)
- [Gender precedence](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Globals/ObjectMgr.cpp#L9978-L10066)
- [Pinned locale names](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Globals/Locales.cpp#L1-L36)
- [Localized SAY](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Object.cpp#L1798-L1847)

The concrete motivating source is ClassicDB's Vermin event 601: AGGRO, chance 10,
TEXT references 1868/1864, no other action. Its two broadcasts have zero server
sound/emote and languages 0/7. Committed tests use authored IDs and text. Literal
source inspection is not installed-database/loader evidence, and component tests
are not graphical-client evidence.

## Runtime selection and synchronous execution

`SourceVisitorCatalog::validate_sources_with_event_ai` retains the immutable
runtime catalog under the existing profile identity. The old constructor remains
complete-empty only. Runtime selection requires exact complete owner, entry and
negative-GUID vectors, no civilian/guard callback, and a complete empty effective
spell list after the zero-template-ID `entry * 100` fallback. Nonempty vectors
require explicit AIName EventAI in this bounded policy; pinned source also has
an empty-name Permit fallback, which this selection does not infer. Owners with
empty vectors stay represented and execute no EventAI action/evaluation.

All admitted runtime broadcasts have nonempty default male text. This proves
DoDisplayText's gender invariance without inventing a selected-model gender.
Spell target admission accepts only matching retained event IDs/AIName and
preserves unrelated min=max level, script, addon, immunity and subtype guards.
The NPC slow-caster source domain remains separate and empty-event only.

The actual callback runs after source combat-position capture and initial notify
cancellation, before player combat and Motion. Both selected source sight and
hostile-spell receipt paths reach it. Late legacy/update-finish CombatStarted is
rejected under this selection. The callback binds world/tick/moment, exact NPC
generation, direct player invoker, poses, catalog and source cause. Only genuine
entry evaluates holders; attacks while already Combat do not. Chance/action use
the existing shared full-u32 counter stream (one word on failure, two on pass).
Eligible periodic-aura value rolls precede hostile-spell entry; target health and
aura installation still follow it.

## Complete camera audience and protocol

A fresh dedicated query asks only for complete loaded world-container camera
membership. The selected session is explicitly one player using its own body as
camera. Exactly one current owner/body is classified as visited or outside;
missing, duplicate, stale, farsight and unrepresented-camera evidence rejects.
Zero visited is valid only with complete outside evidence. Authority does not
choose chance, text or delivery.

The configured SAY range doubles at speaker visibility >=200 and quadruples at
>=400. Source Cell traversal adds speaker bounding radius then caps at the
constant MAX_VISIBILITY_DISTANCE (533.33333f); it is not capped by speaker
visibility and is not a geometric disk. Only visited cameras get the strict
binary32 3D squared-distance test with both combat reaches. There is no LOS/alive
filter. No listener still consumes both source-required words. The numeric-only
[oracle](../../../../scripts/native_event_ai_camera_oracle/README.md) generates
unchanged pinned C++ source outputs; 44 edge vectors test the actual Rust range,
distance and audience validator. Its authored membership proves no loaded map.

Typed speech projects through existing SessionEvent::Chat/ChatMessage monster
SAY (0x0B), language 0/7, localized inline name/text, player target GUID and tag0.
Source-motion projection binds the callback generation in event order, including
speech before same-tick lethal damage/destruction. No synthetic sound, emote or
notification is sent; existing client chat/language/bubble handling is reused.
Graphical assets, font/language tables and client talk gestures remain unverified.

## Transaction, continuation and incompatible format

Native format 38 (`benilla-native-creature-generation-v38`) binds profile, state,
record, checkpoint, melee and callback domains together. Native37 and earlier are rejected,
even after outer checksums are recomputed. The periodic-roll reorder also changes
callback witnesses and is an independent reason old journals cannot be replayed.
Required option fields never silently default missing EventAI continuation or
NPC callback anchors to an empty state.

Per-generation ready holders and bounded semantic history live inside step_batch.
Each genuine callback independently advances a count/chained semantic anchor in
its NPC Unit event context. Restore recomputes RNG words from world/seed/counter,
selection, locale and audience, and reproduces those anchors; changed/dropped
rows, adjusted occurrence counts, stale generations and tampered holders reject.
Callback queue snapshots are structurally validated, including distinct player
insertion/event identities after SpellUpdate re-adds. Respawn rebuilds ready
holders for the replacement generation and preserves the source Unit anchor.
History shares existing byte/replay-work limits; it is never silently truncated.
These integrity/semantic checks do not authenticate a wholly fabricated history.

RNG, holder, history, receipt and speech state are fully staged. Late authority,
size, checkpoint and projection failures cannot partially publish a frame.
Selected EventAI controller ticks join the durable-before-publish path. Store
failure/retry tests withhold staged chat; historical restore/replay/bootstrap
stays silent, and a new-generation entry emits only its own line. This proves the
session/store publication boundary, not crash-proof exactly-once external display.

## Independent closed boundaries

Authored component fixtures use IDs91001/71001/71002 and chance100 for deterministic
coverage; they are not a captured Vermin profile. Actual ClassicDB601 (chance10,
TEXT1868/1864, language0/7, zero effects) matches this retained catalog/reducer
shape. Actual level1–2 selection, complete installed source capture and scene
ownership, runtime producer, client/terrain dependencies, Fireball/loot closure
and graphical evidence remain separate gates. NativeCapability/startup and loaded
native-cache saved-session reopening remain closed; no compatibility exception
is added here.
