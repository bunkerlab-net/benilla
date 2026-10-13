# Conditional Rust native-profile consumer seam

This preparation is a reproducible input artifact, not a native-profile token.
The next Rust consumer must create an opaque `NativeProfile` from validated local
inputs when its executable capabilities cover those inputs. It must not require
cloud possession of the user's data, a manual trust toggle, a hardcoded spell
ID, an empty diagnostic report or a permanently false native-support flag.
This document specifies the seam; it does not implement or certify that consumer.

## Required local inputs and verification

Use all three inputs: the private source-v2 bundle, the completed local DBC
extraction (including extraction.json), and this preparation. The preparation
alone does not duplicate every source item/quest row or every complete DBC table.
Revalidate source schema, selected rows, closure, consistency and hashes; verify
the completed extraction against whole-file bytes; then recompute the preparation
and its source/DBC bindings. A Rust verifier must reproduce the same contract or
consume a locally reverified result through a comparably strict boundary. A
caller-authored `captured_inputs_complete`, repaired hash or removed blocker is
not evidence. Imported strings remain data, never paths, scripts or SQL.

`source_sha256`, `dbc_manifest`, `extraction_manifest` and
`preparation_sha256` provide deterministic integrity/lineage. They are not
signatures, build authenticity, legal-rights evidence or gameplay certification.
Extraction v3 requires Talent and CreatureDisplayInfo in its exact 20-table set.
Legacy v1/v2 manifests require new local extraction, never a silently repaired
coverage assertion. The diagnostic library may inspect missing tables, but null
coverage never proves empty membership or a neutral display scale.

## Initializer inputs

The following bindings are sufficient to *construct candidate typed inputs*;
all named semantic gaps below must still be resolved before initialization:

- `request` and `player.identity.selection` provide exact race/class/gender.
  `player.identity.start` retains the selected SQL map/zone/decimal pose;
  display/faction IDs, power type and family are actual DBC bindings. Recheck
  CharBaseInfo, playable-race flags and map restrictions. Do not substitute class
  defaults for null fields. The bounded level-one mage initializer must check its
  actual supported identity/power/level domain
- The level-one member of `player.level_inputs` supplies create_health,
  create_mana, strength, agility, stamina, intellect and spirit. These are base
  creation inputs, not effective attributes, full resources or combat values
- `player.skills.sql_candidates` maps in captured source order to
  default_skills `{id: skill, step}`; `player.initial_sql_spells` maps in captured
  order to default_spells `Spell`. No action-button row proves a learned spell
- `player.skills.dependency_closure` enumerates the bounded reachable skill,
  spell and item IDs and every exact DBC reference. Recompute its fixed point.
  `captured_inputs_complete=true` means only that this named dependency scope
  contains its referenced inputs. It does not prove effective learning or final
  stats. Missing source rows require another bounded local recapture
- Use `dbc_bindings` and complete verified DBC tables for typed skills:
  SkillLine words 0/1 are ID/signed category; SkillRaceClassInfo words 0–4 and 6
  are row ID, skill, race mask, class mask, flags and tier. SkillTiers word 0 is
  ID, words 1–16 are values and 17–32 maxima. SkillLineAbility words 0–4, 7, 9,
  10 are row ID, skill, spell, race/class masks, required value, learn-on-get-skill
  and max value. Word 8 is a forward rank reference. Preserve native row-ID order
  and all matching rows, including zero learning mode and unmatched masks.
  Other words remain raw inputs requiring disposition, not implicit zeroes
- `spells[].server_numeric_words` supplies 173 candidate server-overlaid words
  for each captured spell; every reviewed SQL numeric field is overlaid on its
  exact DBC column, preserving unsigned bit patterns/signed interpretation and
  exact binary32 conversion. Effects are words 61–63, BaseDice 67–69, misc
  106–108 and triggers 109–111. `source_row` retains server-side columns beyond
  those words. Check every reported server/client difference and every relevant
  uncaptured server overlay; the array name is not authority for omitted columns
- `spells[].duration` retains signed base/per-level/maximum milliseconds at the
  actual duration index. Resolve the native duration rule for the initializer's
  level and role. Missing duration and duration-index zero are distinguishable
  via source_row; do not turn every null duration into a zero-duration spell
- `dependency_closure.server_relationships` contains raw selected spell_chain
  and spell_learn_spell rows, or null for legacy unknown coverage. Their exact
  selection evidence is `sql_relationship_coverage`. Convert an empty array to
  `Rows::Complete([])` only after verifying that evidence. It is not yet the
  effective loaded SpellLearnSpell map or the effective previous-rank relation
- `derived_learn_spell_scan` records the native loader's global table-empty
  branch: enabled_by_nonempty_table, skipped_for_empty_table or not_captured.
  The separate 0/1-row global probe establishes only that branch. Reproduce SQL
  loader validation/ordering, DBC-derived Effect36 links, talent/target/passive/
  SkillStep auto-learn flags and rank derivation before constructing
  `SpellRow.learned_spells`, previous_rank and talent. Never map unknown talent
  or unknown external hooks to false
- `player.outfit.grants_in_source_order` preserves positive CharStartOutfit slot
  order followed by custom SQL grants, exact item IDs and bounded counts.
  Resolve every referenced item through the companion source's item_template.
  Pass its actual fields through the initializer's supported item constructor;
  reject unresolved quantities, unknown requirements, unsupported placement,
  bags/offhand/set/random/equip-proc branches or unproven item-creation hooks.
  A quantity or display ID does not establish a created/equipped item instance

Only successful source-ordered initialization establishes effective skills,
abilities, equipment, passive auras, final attributes/resources and the stat
recipe. Unsupported optional active **casting** may remain visible/unavailable,
with actual cast admission rejecting it. Its learning side effects still run or
fail entry; a passive predecessor cannot be scoped away with the higher active
cast. Failure/unknown behavior must not be replaced with neutral modifiers.

## NPC and spell execution inputs

`creatures[].source_row`, spawns, class_level_inputs and model_inputs preserve
selected native inputs. They are not resolved combat stats. Validate complete
required class/level and model rows, selection/RNG semantics and template
multipliers through the exact native stat/AI kernels. `event_ai_capture` proves
only current captured template-entry and selected spawn-GUID applicability;
matching rows block an unimplemented EventAI path. Captured-empty selectors do
not prove absence of pool/event substitution, groups/addons, scripts, assistance,
movement, faction hostility, configuration or lifecycle hooks. Supply and verify
those narrow dependencies where relevant; do not manufacture no-hook defaults.

Spell cast/range/duration/effect/source-row bindings are metadata inputs. Enabled
casts additionally require executable target, effect, scaling, aura/periodic,
resource, interruption/cooldown and projectile semantics for all actual slots
and flags. Unknown initial/passive effects remain entry-critical. Native melee
admission also requires genuine initialized skills/equipment/stats and verified
NPC inputs, supported source callbacks, RNG order and transactional state
application. None of these follow merely from a matching dependency hash.

## Diagnostic fields and admission decisions

`status: blocked`, `runtime_export: null`, `resolved_combat_stats: null`,
`ai_profile: null`, `effective_skills: null`, `effective_learning_proven: false`,
`candidate_paths` with eligibility not_evaluated, `ability_admission`,
`unsupported`, and source/DBC field dispositions are preparation diagnostics.
They neither enable gameplay nor permanently ban inputs a later verified native
implementation can execute. Recompute criticality in the native consumer;
`ability_admission` is conservative guidance, not a learned spellbook.

The future adapter must discharge each relevant input/semantic requirement by
validated data and a concrete capability, then construct a new opaque profile.
It must never delete diagnostic strings to turn this preparation into runnable
JSON. Keep unsupported optional casts unavailable, reject unresolved
entry-critical dependencies, and report exact missing local inputs or unsupported
semantics. Talent/initial-hook/loader gaps identified above are actionable work,
not an unconditional cloud-assets or manual-approval gate.

## Talent membership input

Extraction v2 includes Talent.dbc (21 words, rank spell IDs at4–8).
`talent_inputs` binds its full-table hash and row count and all matching raw rows
for every selected spell. Missing table differs from complete empty/no-match.
Duplicate memberships are retained, never collapsed into a guessed rank. The
consumer must still apply pinned DBC loader validation and rank-map semantics;
the learning diagnostic now distinguishes a missing table from
`raw_table_captured_effective_loader_unproven`. Raw input alone does not
establish the effective loader map.


## Native spawn dependency evidence

The preparation's `native_spawn_capture` is the validated source coverage object;
`creatures[].native_spawn_dependencies` contains records for that entry's selected
static GUIDs, or null for a legacy source with no coverage. Each record binds the
coverage digest. See the [exact snapshot and selector contract](../classic_export/NATIVE-SPAWN-CONTRACT.md).

Use a table's `status: complete` and actual empty `rows` as complete-empty only
after source validation and preparation recomputation. An absent coverage object,
missing table, missing default row and empty matching rows are distinct states.
`spawn_template.row` is the actual selected override or Entry 0 row; it is never
synthesized. `addon` selects a GUID row before an entry row, even when the GUID
row has only zero values or null auras. Shadowed entry-addon rows remain visible.

`unimplemented_hook_tables` covers only the named event/pool/applicable-addon
inputs. Its empty list does not prove other hooks absent. Every matching hook
row, including zero and negative event rows, remains an execution requirement.
Spawn-template flags, equipment, health/mana, relay and string IDs stay raw;
this preparation does not implement overrides, loader corrections or scripts.
Neither coverage hashes nor successful revalidation create a native admission
profile. Groups/linking, substituted entries and other server context remain
separate unresolved requirements.


## Selected registry and display evidence

`spell_script_capture` carries the validated source proof described in the
[registry contract](../classic_export/SPELL-REGISTRY-CONTRACT.md).
Each `spells[].script_registry` is null for legacy unknown coverage, or a complete
status, exact matching rows and evidence digest. Complete-empty proves only the
pinned fresh-load numeric SpellScript/AuraScript registry absence. Every present
row requires named semantics. Initial-cast area/condition/proc paths still need
their own source-backed admission proof.

`creature_display_inputs` records full-table coverage, SHA-256, row count,
selected IDs and known missing IDs. Missing table has `coverage: not_captured`,
null hash/count/missing IDs; captured table uses `complete_raw_table`, including
an empty table. Each creature has `selected_display_ids`, recursively selected
`model_inputs`, and `display_inputs` with ID, model ID, extended-display ID,
`scale_binary32`, signed `size_class` and `source` (all raw words, decoded strings
and record SHA-256). The same rows appear in `dbc_bindings` with explicit field
dispositions. Whole-file manifest and preparation verification bind exact bytes.

CreatureDisplayInfo has 12 words/48 bytes. ID is word 0; model ID 1,
extended-display ID 3, scale bits 4; strings are exactly 6–8. Word 9 is signed
SizeClass, including -1, not a portrait string despite an obsolete core comment.
The selected roots are all nonzero template DisplayId1..4 and actually applicable
spawn-template ModelId, closed recursively over modelid_other_gender. Missing
SQL gender rows and DBC display rows are explicit requirements. Alternative model
IDs stay unsupported context; no unreviewed alternative relation is followed.
Nonfinite selected scales reject; nonpositive scales are retained and blocked.
Model/extended-display IDs do not establish renderer assets or visual admission.

New P2P worlds may use an explicit named native-consumer world-rule policy. This
preparation does not require importing an existing server's config or assume its
values. Legacy configuration import would need effective parsed values and
provenance. Rates outside that named policy, aggro/lifecycle and executable
callbacks remain separate native requirements.
