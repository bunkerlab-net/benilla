// SPDX-License-Identifier: GPL-3.0-or-later
//! Genuine generated inputs to existing retained source query ownership.
use super::super::navigation::SnapshotBinding;
use super::*;
use benilla_native_nav::{QueryProfile, SourceMapQuery, SourceMapQuerySlot, TileInput};
use benilla_world_runtime::native::{MMapForcingStatus, Profile};
const POLICY: &[u8] = b"benilla-generated-resident-source-map-query-v1";
const MAX_TILES: usize = 9;
const MAX_BYTES: usize = 16 * 1024 * 1024;

pub(in super::super) fn load_query_set(
    pairs: &[(&PreparedNavigationInput<'_, '_, '_>, &GeneratedNavigation)],
    profile: &Profile,
    deadline: Instant,
    cancelled: impl Fn() -> bool,
) -> Result<(SnapshotBinding, Hash, SourceMapQuery), GenerationError> {
    load_query_set_inner(pairs, profile, deadline, cancelled, || {})
}
fn load_query_set_inner(
    pairs: &[(&PreparedNavigationInput<'_, '_, '_>, &GeneratedNavigation)],
    profile: &Profile,
    deadline: Instant,
    cancelled: impl Fn() -> bool,
    after_load: impl FnOnce(),
) -> Result<(SnapshotBinding, Hash, SourceMapQuery), GenerationError> {
    let poll = || {
        check(&cancelled)?;
        if Instant::now() >= deadline {
            return Err(GenerationError::Deadline);
        }
        Ok(())
    };
    poll()?;
    if pairs.is_empty() {
        return Err(Error::Missing.into());
    }
    if pairs.len() > MAX_TILES {
        return Err(Error::Budget.into());
    }
    profile.validate().map_err(|_| Error::Profile)?;
    let config = profile.pathfinder_configuration().ok_or(Error::Missing)?;
    let (first, output) = pairs[0];
    let map = first.terrain.map_id();
    if profile.map_id != map
        || config.map_id() != map
        || config.raw_instance_type() != 0
        || config.source_is_dungeon()
        || config.selected().source_instance_id != 0
        || !config
            .selected()
            .pathfinding_enabled(map, MMapForcingStatus::Default)
            .map_err(|_| Error::Profile)?
    {
        return Err(Error::Unsupported.into());
    }
    let mut ordered = Vec::new();
    ordered
        .try_reserve_exact(pairs.len())
        .map_err(|_| Error::Budget)?;
    ordered.extend_from_slice(pairs);
    ordered.sort_by_key(|(input, _)| (input.tile.x, input.tile.y));
    let mut seen = BTreeSet::new();
    let mut tiles = Vec::new();
    tiles
        .try_reserve_exact(pairs.len())
        .map_err(|_| Error::Budget)?;
    let mut total = output.response.mmap.len();
    let mut source = Sha256::new();
    source.update(POLICY);
    source.update(first.terrain.source_epoch().as_bytes());
    source.update(first.source.map().source_binding());
    let source_epoch: Hash = source.finalize().into();
    let mut inventory = Sha256::new();
    inventory.update(POLICY);
    inventory.update(source_epoch);
    inventory.update(&output.response.mmap);
    for (input, generated) in &ordered {
        poll()?;
        total = total
            .checked_add(generated.response.mmtile.len())
            .ok_or(Error::Budget)?;
        if total > MAX_BYTES {
            return Err(Error::Budget.into());
        }
        input.verify_unchanged(|| cancelled() || Instant::now() >= deadline)?;
        if input.terrain.map_id() != map
            || input.terrain.source_epoch() != first.terrain.source_epoch()
            || input.source.map().source_binding() != first.source.map().source_binding()
            || input.tile != generated.tile
            || input.revision != generated.input_revision
            || generated.response.mmap != output.response.mmap
        {
            return Err(Error::Identity.into());
        }
        if !seen.insert((input.tile.x, input.tile.y)) {
            return Err(Error::Duplicate.into());
        }
        validate_response(&input.request, &generated.response)?;
        let mut result = Sha256::new();
        result.update(input.revision);
        result.update(&generated.response.mmap);
        for bytes in generated.response.mmtile.chunks(65536) {
            poll()?;
            result.update(bytes);
        }
        if Hash::from(result.finalize()) != generated.revision {
            return Err(Error::Identity.into());
        }
        inventory.update([input.tile.x, input.tile.y]);
        inventory.update(generated.revision);
        // Empty/Exact625 remain absent in Detour; their receipts still bind this set.
        inventory.update([match generated.response.status {
            GenerationStatus::Generated => 0,
            GenerationStatus::Empty => 1,
            GenerationStatus::Exact625 => 2,
        }]);
        if generated.response.status == GenerationStatus::Generated {
            tiles.push(TileInput {
                grid: [input.tile.x, input.tile.y],
                bytes: &generated.response.mmtile,
            });
        }
    }
    if tiles.is_empty() {
        return Err(Error::Missing.into());
    }
    poll()?;
    let slot = SourceMapQuerySlot {
        map_id: map,
        instance_id: config.selected().source_instance_id,
    };
    // Bounded native load is synchronous; deadline checked immediately afterwards.
    let query = SourceMapQuery::load(slot, &output.response.mmap, &tiles)
        .map_err(GenerationError::Native)?;
    after_load();
    poll()?;
    for (input, _) in &ordered {
        input.verify_unchanged(|| cancelled() || Instant::now() >= deadline)?;
        poll()?;
    }
    let inventory: Hash = inventory.finalize().into();
    let mut policy = Sha256::new();
    policy.update(POLICY);
    policy.update(benilla_native_nav::SOURCE_MAP_QUERY_POLICY);
    policy.update(super::super::SOURCE_PIN);
    policy.update(super::super::DETOUR_TREE);
    policy.update(inventory);
    policy.update(profile.content_digest); // Explicit runtime configuration only.
    policy.update(config.binding());
    policy.update(
        QueryProfile::SourceMapInstance1024
            .node_capacity()
            .to_le_bytes(),
    );
    Ok((
        SnapshotBinding {
            map,
            content: profile.content_digest,
            source_epoch,
            inventory,
            policy: policy.finalize().into(),
        },
        config.binding(),
        query,
    ))
}

#[cfg(test)]
pub(crate) fn assert_source_change_after_query_load(
    pairs: &[(&PreparedNavigationInput<'_, '_, '_>, &GeneratedNavigation)],
    profile: &Profile,
    mutate: impl FnOnce(),
) {
    assert!(matches!(
        load_query_set_inner(
            pairs,
            profile,
            Instant::now() + std::time::Duration::from_secs(10),
            || false,
            mutate
        ),
        Err(GenerationError::Composition(_)) | Err(GenerationError::Collision(_))
    ));
}
