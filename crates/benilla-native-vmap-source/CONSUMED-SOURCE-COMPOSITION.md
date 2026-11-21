# Compatible consumed-source height observations

The private application `PreparedSourceHeight` view combines a genuine terrain preparation with a genuine assembled collision source map. It offers a historical scalar height calculation using the existing CMaNGOS `TerrainInfo::GetHeightStatic` decision order, with height and VMap calculation enabled as an explicit observation profile. It does not establish the configuration of a running server.

## Compatibility evidence

Terrain and collision consume different sets of archive members, so their source digests normally differ. The constructor requires the same selected map and pinned source revision, and recognizes only the existing terrain dual-resolver policy and VMap numeric-patch resolver policy. These policies remain distinct.

Borrowed receipt accessors expose evidence already retained by actual captures. Receipt fields have no public constructor or deserializer, and the composition accepts the original preparation objects, never caller-supplied receipt arrays. The check requires exact positive shared receipts for Map.dbc, the selected WDT and every selected terrain ADT. It also compares every overlapping history receipt. Presence, absence and dummy outcomes cannot substitute for one another. Positive matches require the same winning archive basename, byte length and SHA-256. Repeated collision lookups must agree. Even equal bytes from different winning archives are rejected by this deliberately strict compatibility profile.

The result means compatible consumed-source snapshots. It does not prove whole-install equality, file-system identity, client authenticity or an atomic capture. No whole-archive content digest is recorded. VMap archive-scoped enumeration receipts remain committed by its original binding; chain lookup receipts provide the cross-consumer comparisons. Inputs unique to one consumer remain bound by that consumer's source identity.

## Calculation and availability

Actual terrain products pass the existing `TerrainTile` decoder. The collision view validates the complete placement index and loads the selected tiles through the ordinary model decoder. Queries share one collision work/deadline budget across all height retries and use the existing source arithmetic. Missing terrain or required collision geometry propagates as an error. Neither missing geometry nor a conservative BIH refusal becomes a height miss.

The binding commits both source identities, actual terrain bytes, selected collision residency and a separate composition policy. A successful query returns a scalar height, including the source's invalid-height sentinel where appropriate. It does not certify body support, traversability, dry terrain, navigation, dynamic-world state or gameplay admission. Existing full-inventory constructors and complete-dry-map requirements remain unchanged.

## Freshness and limits

The constructor checks both sources before and after preparation. The view owns decoded geometry and borrows both source owners. Queries continue to describe that immutable historical snapshot after installation changes. Explicit `verify_unchanged` checks both sources again; any eventual live-use adapter must choose and enforce its freshness boundary. These are sequential observational checks: terrain may change after its check while collision is being checked. They do not lock the filesystem, provide atomic cross-source freshness or prevent hostile changes between reads.

Terrain selection stays within its existing nine-tile preparation limit. Receipt processing is capped at 32,768 entries per consumer, and collision residency retains its existing source/index/resident/query budgets. Terrain source verification is not cancellation-interruptible and has no soft time deadline. It may reread the existing bounded captured set (up to 192 MiB of decoded positive members, at most 64 MiB each), plus bounded resolver/profile checks. Cancellation cannot shorten a blocked underlying filesystem read. Terrain source verification and bounded product decoders are synchronous; cancellation is polled around them and throughout receipt/hash traversal. Producer capture, decoded terrain and resident collision allocations remain separate costs. This does not solve full-map producer memory consumption.
