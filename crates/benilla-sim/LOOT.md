# Solo body loot and one-hop references

`src/loot.rs` reconstructs the solo ordinary and quest-only corpse paths from CMaNGOS
Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`, principally
`LootMgr.cpp` 135–195 (loading), 306–367 (row validation), 374–391 (Roll),
930–941 (AddItem), 2397–2477 (group Roll), and 2606–2670 (template processing).
Copyright and complete source-file hashes are in [PROVENANCE.md](PROVENANCE.md).
This is bounded simulation, not general loot-template or gameplay admission.

## Retained capture contract

`Catalog::new_with_references(body, references)` takes two distinct namespaces:
`creature_loot_template` and `reference_loot_template`. The same numeric ID may
exist in each. `Catalog::new(body)` supplies an empty reference namespace.
`captures()` returns body captures; `reference_captures()` returns child captures;
`contains()` and `generate()` address body IDs only.

Every capture retains its source fingerprint, full-table ordered-result
fingerprint/count, selected positions, all selected source row fields in order,
and exact local positive-leaf item lookups. Every selected template must be
complete. Captures within one namespace describe one coherent snapshot: source
fingerprint, ordered-result fingerprint and result count agree, and selected
positions do not overlap. The two namespaces have independent witnesses. The
full table may contain unrelated templates outside the selected projection; they
need not be included. These witnesses are required inputs, not evidence inferred
from row sorting or re-querying. Test-adapter witnesses are authored synthetic
inputs and do not establish real database loader order.

Reference captures are exactly the targets named by body references, including
references with zero repetitions. Missing or extra targets are rejected. Each
capture's item map contains exactly its positive leaves; reference placeholder
item IDs do not imply an item dependency. Repeated leaf IDs in either namespace
must have identical metadata. Policy v3 hashes both namespace labels/counts even
when empty, plus the retained row, item and order inputs. The upstream pin and
complete-capture flags are fixed admission invariants.

## Admitted reference mechanics

Only body rows with negative `min_count_or_reference` and `group == 0` may
reference a child. The checked negation is the child ID. `i32::MIN`, missing
children, nested references and grouped references are rejected. Group IDs are
current-template membership, never a child-group selector. All child rows are
ordinary positive leaves, including the existing explicit/equal-chance groups.
Conditions, quest leaves inside references and unsupported item properties remain
excluded. Source `maxcount` is an unsigned byte for leaves and references; the
loader rejects values above 255. A reference may have zero repetitions and a
positive chance below the leaf-only 0.000001 threshold.

`Rules::referenced_rate_bits` is distinct from the seven leaf quality rates.
Reference and leaf Roll both return immediately for source chance >=100, even
when the relevant rate is zero. Otherwise reference chance is multiplied by the
referenced rate, using binary32, and compared against the supplied chance draw.
One successful parent reference invokes the entire child Process `max_count`
times at that parent's entry position. Each invocation processes ungrouped
entries in retained order, then all child groups in ascending group order. The
parent resumes its entries and subsequently processes its own groups. Equal
groups inspect the shared output, so duplicates can span child invocations,
references and parent rows.

AddItem stops inserting/count-rolling at sixteen output slots. The surrounding
entry chance rolls, reference expansions, group shuffles, group chance draws and
duplicate coins continue. Admission never assumes the output cap stops work.

## Quest-only body leaves

An ungrouped body leaf with negative `ChanceOrQuestChance` retains `quest_only`
on its generated slot. `LootStoreItem` normalizes its chance to the absolute value,
`LootMgr.h:215-217`. Its roll uses the mandatory typed `Rules::quest_drop_rate`,
`LootMgr.cpp:374-389`, rather than its item quality rate. The chance-at-least-100
shortcut still precedes rate multiplication. Smaller chances use strict
draw-less-than-scaled-chance comparison, including at a zero rate.

`generate` requires explicit `OwnerQuestItems`. `Unknown` rejects any quest branch.
`quest_lifecycle::State::needed_items` derives `Known` from the source's logged,
incomplete ReqItem counters below their requirements, `Player.cpp:13777-13807`.
It does not infer need from an item ID or accept unknown quest state as an empty set.

Generation keeps the source roll/count order and cap even for ineligible quest
entries, `LootMgr.cpp:959-1001`. The output retains only the owner's admitted
quest-only slots, with their source slot numbers. A fixed sixteen-entry source
list preserves the cap and equal-group duplicate tests without allocating hidden
items. No fake-empty draw occurs merely because every generated source item is
ineligible. Runtime visibility and collection also require current objective need,
`LootItem::AllowedForPlayer`, `LootMgr.cpp:463-483`. Grouped body quest leaves,
quest references and quest leaves inside reference templates reject.


## Bounds before generation

Before derived map/plan construction or hashing, combined capture count, combined
retained row count, and combined retained item-lookup count are each limited to
4,096. Each template has at most 1,024 rows. Source ordinals are compiled once
into entry/group plans, avoiding a full source-row scan for every group during
each repeated Process.

For every body template, checked multiplication and addition compute three
independent worst-case counters, each limited to `MAX_PROCESS_BUDGET = 4096`:

- Process passes: one per template invocation, including every child repetition
- Work units: one per invocation, one per ungrouped entry, one per source group
  including empty gaps, and two per group candidate for permutation handling and
  candidate traversal. Individual permutation validation is additionally bounded
  by the 1,024-row template cap; duplicate lookup scans at most sixteen outputs
- Recorded draws: an ungrouped chance below 100 costs one; every positive
  ungrouped leaf reserves one count draw. A group with E explicit and Q equal
  candidates reserves E + (E > 0) + 2Q + (E + Q > 0) draws. This covers up to E
  shuffle draws, one explicit chance, up to Q equal shuffle draws, every possible
  equal-candidate duplicate coin and one selected count. The runtime's
  Fisher-Yates adapter records N-1 draws for a shuffle of N candidates, so N is
  conservative. A successful reference adds the complete child counters times
  its repetition count, even when the output would already be full. Finally,
  each body reserves two draws for money and fake-empty-window handling

All potential reference rolls and branches are treated as successful/worst-case;
rate values and output saturation cannot weaken admission. A zero-repetition
reference still resolves and validates its child, and still rolls when its
chance is below 100. Budget rejection occurs while constructing the catalog,
before consuming random outcomes. The draw bound covers this distribution host;
it makes no claim about raw MT engine words or rejection sampling within a host.

## Evidence

`tests/loot.rs` replays the unchanged 1,536 ordinary source vectors and the
separate 1,024 one-hop vectors from `tests/fixtures/loot_reference_cmangos.tsv`,
checking every supplied distribution outcome/permutation and output slot.
Focused synthetic tests additionally reject malformed closure, nested/grouped
references, bad rates, incomplete/incoherent witnesses, metadata divergence,
over-budget work and draws, and oversized retained projections. They also bind
reference rows, placeholders, metadata and order witnesses into catalog identity.
The pinned-source oracles establish distribution-level ordering and results,
not C++ standard-library RNG/shuffle word parity or native-world availability.
