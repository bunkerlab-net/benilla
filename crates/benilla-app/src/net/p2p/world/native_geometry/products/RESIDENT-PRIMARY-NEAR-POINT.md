# Resident primary Ground NearPoint

The bound resident context can answer an actual initial NearPoint query using the
same consumed terrain/collision and generated navigation identity as its filter,
Chase and constructor queries. Its geometry domain is versioned to
benilla-resident-query-ground-primary-v2. It intrinsically binds actual product
identities, fixed enabled collision/height/LOS configuration, ground search10,
empty-dynamic semantics and pinned source policies/hashes. An old v1 or arbitrary
caller-supplied expected revision cannot select this domain.

The exact Session must select the existing PrimaryNearPointPolicy for its Profile;
empty-selector fallback is not selected. Source-owned searcher/generation/body,
pose, clock and target checks are unchanged. The supported branch is Ground.
Swimming and Flying remain unavailable. Independent NonRenderParticipants evidence
establishes the empty dynamic tree; the source's empty selector is not that proof.

The provider reuses source angle and initial candidate arithmetic, starts Z at the
target's actual Z, evaluates Map::GetHeight with static search10 and the original
dynamic start/range arithmetic, then applies the Ground allowed-Z clamp. Search50
belongs to liquid observations and is not reused here. A single bounded query
budget spans actual height and LOS work. Invalid height remains the source scalar
sentinel and does not become physical support.

After the source vertical predicate, the exact ray runs from target eye to the
candidate's adjusted feet with ignore_m2=false. A clear initial candidate yields
an exact NearPoint fact for the existing runtime reducer. If vertical or LOS
acceptance fails, UnsupportedNearPointFallback is returned; required unloaded
geometry remains Missing. No guessed fallback point is published. Each retry
requeries the geometry and checks consumed-source freshness before success.

The context's environment factory uses its already-validated v2 Profile/session
and actual borrowed source. The independently usable environment provider still
has its narrower source-only scope; it does not certify navigation or acquire
new authority merely because its metadata matches.

Facts are internally consistent historical observations. The runtime must reject
stale facts against its current request; equality does not establish a live
frame. This does not add normal-session installation, real model/starter capture,
full fallback selection, body support/corridor certification, route/cache commit
or populated-world admission. Authored catalog/model initialization and remaining
motion authority inputs in the integration fixtures are identified separately.

No serialized field or network message layout changes. The geometry policy digest
intentionally changes: old v1 profiles/receipts cannot be mixed with this provider.
They require new preparation under the selected v2 policy rather than a metadata
rewrite or implicit compatibility fallback.

The composed runtime tests cover clear primary acceptance, blocked LOS, vertical
fallback, and missing terrain. Missing collision-slot fallback remains covered
by the independent consumed-source height primitive; an accepted loaded resident
navigation composition for that negative case is not established here. Neither
an un-emitted generator result nor output refused by the mandatory production
navigation reader is substituted to construct that test.
