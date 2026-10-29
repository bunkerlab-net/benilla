// SPDX-License-Identifier: GPL-3.0-or-later
//! Fresh positive single-tile TerrainInfo::Load-stage ownership.
//! Not TerrainInfo construction/GC, NGrid, object loading or map registration.
use super::super::source_query::SourceMapQuerySnapshot;
use super::*;
use benilla_world_runtime::native::Profile;

/// Explicit local effective configuration, not a claim about a remote server.
/// The source load-enabled predicate is LOS || height. This consumer deliberately
/// supports only both enabled because its downstream resident queries use both.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct VmapLoadConfiguration {
    pub line_of_sight_enabled: bool,
    pub height_enabled: bool,
}
impl VmapLoadConfiguration {
    fn require(selected: Option<Self>) -> Result<Self, GenerationError> {
        let selected = selected.ok_or(Error::Missing)?;
        if !selected.line_of_sight_enabled || !selected.height_enabled {
            return Err(Error::Unsupported.into());
        }
        Ok(selected)
    }
}

/// Owns the actual allocations made by this isolated successful load stage.
/// No predecoded geometry, native query, residency flag or restored receipt can
/// be supplied. Extraction/generation inputs are immutable bytes, not this
/// owner's loaded allocations. There is no shared/global manager or unload API.
pub(crate) struct PositiveTileLoad<'terrain, 'bundle, 'source> {
    height: source_height::PreparedSourceHeight<'terrain, 'bundle, 'source>,
    query: SourceMapQuerySnapshot,
    configuration: VmapLoadConfiguration,
    tile: TileId,
    references: u16,
    load_attempts: u32,
}

impl<'terrain, 'bundle, 'source> PositiveTileLoad<'terrain, 'bundle, 'source> {
    pub(crate) fn from_generated(
        input: &PreparedNavigationInput<'terrain, 'bundle, 'source>,
        generated: &GeneratedNavigation,
        profile: &Profile,
        configuration: Option<VmapLoadConfiguration>,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<Self, GenerationError> {
        Self::from_generated_observed(
            input,
            generated,
            profile,
            configuration,
            deadline,
            cancelled,
            |_| {},
        )
    }

    pub(crate) fn from_generated_observed(
        input: &PreparedNavigationInput<'terrain, 'bundle, 'source>,
        generated: &GeneratedNavigation,
        profile: &Profile,
        configuration: Option<VmapLoadConfiguration>,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        mut observed: impl FnMut(LoadStage),
    ) -> Result<Self, GenerationError> {
        let poll = || poll(deadline, &cancelled);
        poll()?;
        let configuration = VmapLoadConfiguration::require(configuration)?;
        input.tile.validate()?;
        // Inspect the actual retained source inventory, not a caller tile label.
        let mut terrain_tiles = input.terrain.tiles();
        let (tile, _) = terrain_tiles.next().ok_or(Error::Missing)?;
        if terrain_tiles.next().is_some()
            || tile.x != input.tile.x
            || tile.y != input.tile.y
            || generated.tile != input.tile
            || generated.response.status != GenerationStatus::Generated
            || generated.input_revision != input.revision
            || profile.map_id != input.terrain.map_id()
            || input.source.map().map_id() != input.terrain.map_id()
        {
            return Err(Error::Identity.into());
        }
        input.verify_unchanged(|| cancelled() || Instant::now() >= deadline)?;
        poll()?;

        // Pinned GridMap.cpp::Load refs first; the fresh slot has no GridMap,
        // VMap or MMap allocations. These are staged host effects until the
        // complete positive branch succeeds, never exposed partial source state.
        let references = 1;
        observed(LoadStage::Referenced);
        poll()?;
        // The existing constructor performs terrain decode before collision
        // decode. Neither immutable extraction nor generation is counted here.
        let height =
            source_height::PreparedSourceHeight::from_source(input.terrain, input.source, || {
                cancelled() || Instant::now() >= deadline
            })
            .map_err(GenerationError::Composition)?;
        if height.single_resident_tile() != Some(input.tile) {
            return Err(Error::Identity.into());
        }
        observed(LoadStage::TerrainAndVmapLoaded);
        poll()?;
        let query = SourceMapQuerySnapshot::from_generated(
            &[(input, generated)],
            profile,
            deadline,
            &cancelled,
        )?;
        observed(LoadStage::MmapLoaded);
        poll()?;
        height
            .verify_unchanged(|| cancelled() || Instant::now() >= deadline)
            .map_err(GenerationError::Composition)?;
        input.verify_unchanged(|| cancelled() || Instant::now() >= deadline)?;
        poll()?;
        // Source marks FullyLoaded in the helper, then Load marks attempted.
        // Both hold only after the actual positive allocations above completed.
        observed(LoadStage::FullyLoaded);
        observed(LoadStage::Attempted);
        height
            .verify_unchanged(|| cancelled() || Instant::now() >= deadline)
            .map_err(GenerationError::Composition)?;
        poll()?;
        Ok(Self {
            height,
            query,
            configuration,
            tile: input.tile,
            references,
            load_attempts: 1,
        })
    }

    /// Source Load(existing GridMap) refs and returns without retrying VMap or
    /// MMap. This is not GetGrid, which has different partial-load retry rules.
    /// Freshness/configuration failures are host refusals before that operation.
    pub(crate) fn acquire(
        &mut self,
        configuration: Option<VmapLoadConfiguration>,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<(), GenerationError> {
        poll(deadline, &cancelled)?;
        if VmapLoadConfiguration::require(configuration)? != self.configuration {
            return Err(Error::Identity.into());
        }
        let next_references = next_reference_count(self.references)?;
        self.height
            .verify_unchanged(|| cancelled() || Instant::now() >= deadline)
            .map_err(GenerationError::Composition)?;
        poll(deadline, &cancelled)?;
        self.references = next_references;
        Ok(())
    }
    pub(crate) fn references(&self) -> u16 {
        self.references
    }
    pub(crate) fn load_attempts(&self) -> u32 {
        self.load_attempts
    }
    pub(crate) fn configuration(&self) -> VmapLoadConfiguration {
        self.configuration
    }
    pub(crate) fn tile(&self) -> TileId {
        self.tile
    }
    pub(crate) fn height(
        &self,
    ) -> &source_height::PreparedSourceHeight<'terrain, 'bundle, 'source> {
        &self.height
    }
    pub(crate) fn query(&self) -> &SourceMapQuerySnapshot {
        &self.query
    }
    /// Consumes the lifecycle owner and transfers its exact loaded products.
    /// This does not unload, reload, clone or grant map registration authority.
    pub(crate) fn into_parts(
        self,
    ) -> (
        source_height::PreparedSourceHeight<'terrain, 'bundle, 'source>,
        SourceMapQuerySnapshot,
    ) {
        (self.height, self.query)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LoadStage {
    Referenced,
    TerrainAndVmapLoaded,
    MmapLoaded,
    FullyLoaded,
    Attempted,
}
fn next_reference_count(current: u16) -> Result<u16, GenerationError> {
    if current >= i16::MAX as u16 {
        return Err(Error::Budget.into());
    }
    Ok(current + 1)
}
fn poll(deadline: Instant, cancelled: &impl Fn() -> bool) -> Result<(), GenerationError> {
    check(cancelled)?;
    if Instant::now() >= deadline {
        return Err(GenerationError::Deadline);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn positive_load_reference_count_stays_inside_source_signed_domain() {
        assert_eq!(next_reference_count(0).unwrap(), 1);
        assert_eq!(next_reference_count(1).unwrap(), 2);
        assert_eq!(next_reference_count(32766).unwrap(), 32767);
        assert!(matches!(
            next_reference_count(32767),
            Err(GenerationError::Validation(Error::Budget))
        ));
        assert!(next_reference_count(u16::MAX).is_err());
    }
}
