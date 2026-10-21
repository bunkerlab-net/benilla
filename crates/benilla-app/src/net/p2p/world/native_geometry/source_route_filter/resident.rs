// SPDX-License-Identifier: GPL-3.0-or-later
//! Source-bound point observations for constructor filters, never a dry region.
use super::super::{
    products::{
        navigation,
        source_height::{self, PointNoWaterReply, PointNoWaterRequest, PreparedSourceHeight},
        source_query::SourceMapQuerySnapshot,
    },
    source_pathfinder_query::SourcePathfinderQueryProvider,
};
use super::*;
use std::rc::Rc;

#[path = "chase_los.rs"]
pub(crate) mod chase_los;
#[path = "cold.rs"]
pub(crate) mod cold;
#[path = "dry_near_point.rs"]
pub(crate) mod dry_near_point;
#[path = "joined.rs"]
pub(crate) mod joined;
#[path = "near_point.rs"]
pub(crate) mod near_point;
#[cfg(all(test, not(benilla_headless_geometry)))]
#[path = "retained.rs"]
pub(crate) mod retained;

const POLICY: &[u8] = b"benilla-resident-query-ground-primary-v2";
pub(crate) fn geometry_revision(
    height: source_height::Binding,
    navigation: navigation::SnapshotBinding,
) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(POLICY);
    hash.update([1, 1, 1]); // Fixed collision detection, VMap height and LOS enabled.
    hash.update(10.0f32.to_bits().to_le_bytes()); // Ground Map::GetHeight search.
    hash.update(b"independent-empty-dynamic-participants-v1");
    hash.update(b"cmangos-8ec338a1-map-height-ground-search10-allowed-z-v1");
    // Pinned Unit.cpp, including the Ground UpdateAllowedPositionZ branch.
    hash.update(b"03b67a4b6226f1f463e71be3694d3c950fc9b915a6e921407a98e4ff82fc21d0");
    for policy in [
        benilla_sim::near_point::primary::POLICY,
        benilla_sim::near_point::POLICY,
        benilla_sim::source_trig::SOFT_POLICY,
        benilla_sim::facing::POLICY,
    ] {
        hash.update((policy.len() as u64).to_le_bytes());
        hash.update(policy.as_bytes());
    }
    for source in benilla_sim::near_point::primary::SOURCE_SHA256
        .iter()
        .chain(benilla_sim::near_point::SOURCE_SHA256.iter())
        .chain(benilla_sim::source_trig::SOURCE_SHA256.iter())
        .chain(benilla_sim::facing::SOURCE_SHA256.iter())
    {
        hash.update(source.as_bytes());
    }
    hash.update(height.map.to_le_bytes());
    hash.update(height.revision);
    hash.update(super::super::source_pathfinder_query::geometry_revision(
        navigation,
    ));
    hash.finalize().into()
}
/// Fixed policy selected before Profile/session/dispatch construction. The joined
/// variant can only be derived from genuine consumed-source inventory evidence.
#[derive(Clone, Debug)]
pub(crate) struct ResidentGeometryPolicy {
    installed: Option<InstalledGeometryPolicy>,
    dry_primary: bool,
    first_cold_route: bool,
    #[cfg(all(test, not(benilla_headless_geometry)))]
    one_retained_route: bool,
    #[cfg(all(test, not(benilla_headless_geometry)))]
    two_retained_routes: bool,
    #[cfg(all(test, not(benilla_headless_geometry)))]
    finite_retained_routes: bool,
    #[cfg(all(test, not(benilla_headless_geometry)))]
    finite_straight_routes: bool,
    #[cfg(all(test, not(benilla_headless_geometry)))]
    generated_flat_support: bool,
    #[cfg(all(test, not(benilla_headless_geometry)))]
    planar_nonpenetration: bool,
    #[cfg(all(test, not(benilla_headless_geometry)))]
    endpoint_support: bool,
    #[cfg(all(test, not(benilla_headless_geometry)))]
    source_spline_support: bool,
}
#[derive(Clone, Debug)]
struct InstalledGeometryPolicy {
    height: source_height::Binding,
    epoch: [u8; 32],
    revision: [u8; 32],
    owner: benilla_formats::NativeInventorySource,
}
impl ResidentGeometryPolicy {
    const PRIMARY: Self = Self {
        installed: None,
        dry_primary: false,
        first_cold_route: false,
        #[cfg(all(test, not(benilla_headless_geometry)))]
        one_retained_route: false,
        #[cfg(all(test, not(benilla_headless_geometry)))]
        two_retained_routes: false,
        #[cfg(all(test, not(benilla_headless_geometry)))]
        finite_retained_routes: false,
        #[cfg(all(test, not(benilla_headless_geometry)))]
        finite_straight_routes: false,
        #[cfg(all(test, not(benilla_headless_geometry)))]
        generated_flat_support: false,
        #[cfg(all(test, not(benilla_headless_geometry)))]
        planar_nonpenetration: false,
        #[cfg(all(test, not(benilla_headless_geometry)))]
        endpoint_support: false,
        #[cfg(all(test, not(benilla_headless_geometry)))]
        source_spline_support: false,
    };
    pub(crate) fn installed_corridor(
        inventory: &source_height::inventory::InventoryCompatibility<'_, '_, '_, '_>,
    ) -> Self {
        let (height, epoch) = inventory.source_binding();
        Self {
            installed: Some(InstalledGeometryPolicy {
                height,
                epoch,
                revision: inventory.revision(),
                owner: inventory.owner().clone(),
            }),
            dry_primary: false,
            first_cold_route: false,
            #[cfg(all(test, not(benilla_headless_geometry)))]
            one_retained_route: false,
            #[cfg(all(test, not(benilla_headless_geometry)))]
            two_retained_routes: false,
            #[cfg(all(test, not(benilla_headless_geometry)))]
            finite_retained_routes: false,
            #[cfg(all(test, not(benilla_headless_geometry)))]
            finite_straight_routes: false,
            #[cfg(all(test, not(benilla_headless_geometry)))]
            generated_flat_support: false,
            #[cfg(all(test, not(benilla_headless_geometry)))]
            planar_nonpenetration: false,
            #[cfg(all(test, not(benilla_headless_geometry)))]
            endpoint_support: false,
            #[cfg(all(test, not(benilla_headless_geometry)))]
            source_spline_support: false,
        }
    }
    /// Distinct typed composition; neither Ground nor standalone dry policy is relaxed.
    pub(crate) fn installed_dry_corridor(
        inventory: &source_height::inventory::InventoryCompatibility<'_, '_, '_, '_>,
    ) -> Self {
        let mut policy = Self::installed_corridor(inventory);
        policy.dry_primary = true;
        policy
    }
    /// Separate calculation policy; the observation-only join cannot emit Route facts.
    pub(crate) fn installed_first_cold_route(
        inventory: &source_height::inventory::InventoryCompatibility<'_, '_, '_, '_>,
    ) -> Self {
        let mut policy = Self::installed_dry_corridor(inventory);
        policy.first_cold_route = true;
        policy
    }
    /// Component-only first cold plus exactly one ACK-selected retained calculation.
    #[cfg(all(test, not(benilla_headless_geometry)))]
    pub(crate) fn installed_one_retained_route(
        inventory: &source_height::inventory::InventoryCompatibility<'_, '_, '_, '_>,
    ) -> Self {
        let mut policy = Self::installed_first_cold_route(inventory);
        policy.one_retained_route = true;
        policy
    }
    /// Component-only cold plus two retained calculations, with flat ACK roots.
    #[cfg(all(test, not(benilla_headless_geometry)))]
    pub(crate) fn installed_two_retained_routes(
        inventory: &source_height::inventory::InventoryCompatibility<'_, '_, '_, '_>,
    ) -> Self {
        let mut policy = Self::installed_one_retained_route(inventory);
        policy.two_retained_routes = true;
        policy
    }
    /// Component-only finite calculation loop, distinct from the fixed-three policy.
    #[cfg(all(test, not(benilla_headless_geometry)))]
    pub(crate) fn installed_finite_retained_routes(
        inventory: &source_height::inventory::InventoryCompatibility<'_, '_, '_, '_>,
    ) -> Self {
        let mut policy = Self::installed_one_retained_route(inventory);
        policy.finite_retained_routes = true;
        policy
    }
    /// Separate component-only finite straight policy. Old retained resolvers stay closed.
    #[cfg(all(test, not(benilla_headless_geometry)))]
    pub(crate) fn installed_finite_straight_routes(
        inventory: &source_height::inventory::InventoryCompatibility<'_, '_, '_, '_>,
    ) -> Self {
        let mut policy = Self::installed_first_cold_route(inventory);
        policy.finite_straight_routes = true;
        policy
    }
    /// Distinct component-only one-cell generated Polygon family. Native points stay exact.
    #[cfg(all(test, not(benilla_headless_geometry)))]
    pub(crate) fn installed_generated_flat_straight_routes(
        inventory: &source_height::inventory::InventoryCompatibility<'_, '_, '_, '_>,
    ) -> Self {
        let mut policy = Self::installed_finite_straight_routes(inventory);
        policy.generated_flat_support = true;
        policy
    }
    /// Independent exact planar nonpenetration, never a collision-solver tolerance.
    #[cfg(all(test, not(benilla_headless_geometry)))]
    pub(crate) fn installed_planar_nonpenetration_straight_routes(
        inventory: &source_height::inventory::InventoryCompatibility<'_, '_, '_, '_>,
    ) -> Self {
        let mut policy = Self::installed_generated_flat_straight_routes(inventory);
        policy.planar_nonpenetration = true;
        policy
    }
    /// Separate current-query endpoint probes, never original path height calls.
    #[cfg(all(test, not(benilla_headless_geometry)))]
    pub(crate) fn installed_endpoint_supported_straight_routes(
        inventory: &source_height::inventory::InventoryCompatibility<'_, '_, '_, '_>,
    ) -> Self {
        let mut policy = Self::installed_planar_nonpenetration_straight_routes(inventory);
        policy.endpoint_support = true;
        policy
    }
    /// Separate source-interpolation envelope; never normalized solver-sweep parity.
    #[cfg(all(test, not(benilla_headless_geometry)))]
    pub(crate) fn installed_source_spline_supported_straight_routes(
        inventory: &source_height::inventory::InventoryCompatibility<'_, '_, '_, '_>,
    ) -> Self {
        let mut policy = Self::installed_endpoint_supported_straight_routes(inventory);
        policy.source_spline_support = true;
        policy
    }
    pub(crate) fn revision(
        &self,
        height: source_height::Binding,
        navigation: navigation::SnapshotBinding,
    ) -> Result<[u8; 32], Failure> {
        let primary = geometry_revision(height, navigation);
        let Some(installed) = &self.installed else {
            return Ok(primary);
        };
        if height != installed.height || navigation.source_epoch != installed.epoch {
            return Err(Failure::Binding);
        }
        let mut hash = Sha256::new();
        if self.first_cold_route {
            if !self.dry_primary {
                return Err(Failure::Binding);
            }
            hash.update(b"benilla-resident-installed-first-cold-route-v1".as_slice());
            hash.update(
                b"single-first-calculation-revision1-exact-live-dispatch-historical-ordered-ack-v1",
            );
            hash.update(dry_near_point::geometry_revision(height));
            hash.update(super::super::source_pathfinder_query::geometry_revision(
                navigation,
            ));
        } else if self.dry_primary {
            hash.update(b"benilla-resident-installed-dry-corridor-evidence-v1".as_slice());
            hash.update(dry_near_point::geometry_revision(height));
            hash.update(super::super::source_pathfinder_query::geometry_revision(
                navigation,
            ));
        } else {
            hash.update(b"benilla-resident-installed-corridor-evidence-v1".as_slice());
            hash.update(primary);
        }
        #[cfg(all(test, not(benilla_headless_geometry)))]
        if self.one_retained_route {
            if !self.first_cold_route || !self.dry_primary {
                return Err(Failure::Binding);
            }
            hash.update(b"benilla-resident-installed-one-retained-route-v1");
            hash.update(b"ack-selected-cold-predecessor-exact-native-storage-revision2-v1");
            hash.update(b"fresh-gate-before-selection-update-filter-native-retained-v1");
        }
        #[cfg(all(test, not(benilla_headless_geometry)))]
        if self.two_retained_routes {
            if !self.one_retained_route {
                return Err(Failure::Binding);
            }
            hash.update(b"benilla-resident-installed-two-retained-routes-v1");
            hash.update(b"flat-ack-selected-native-revisions1-2-3-fourth-closed-v1");
            hash.update(b"bounded-full-storage4096-native-trace8050-v1");
        }
        #[cfg(all(test, not(benilla_headless_geometry)))]
        if self.finite_retained_routes {
            if !self.one_retained_route || self.two_retained_routes {
                return Err(Failure::Binding);
            }
            hash.update(b"benilla-resident-installed-finite-retained-routes-max8-v1");
            hash.update(b"flat-ack-selected-first-or-repeated-native-revisions1-through8-v1");
            hash.update(b"checked-source-revision-current-native-full-storage-trace8050-v1");
        }
        #[cfg(all(test, not(benilla_headless_geometry)))]
        if self.finite_straight_routes {
            if !self.first_cold_route
                || self.one_retained_route
                || self.two_retained_routes
                || self.finite_retained_routes
            {
                return Err(Failure::Binding);
            }
            hash.update(b"benilla-resident-installed-finite-straight-routes-max8-v1");
            hash.update(b"flat-ack-selected-cold-or-straight-source1024-full-storage-v1");
            hash.update(b"fresh-source-chase-clear-dry-calculate-force0-straight1-v1");
            hash.update(b"initialized-full-width-optimize0-normalize0-ignore-normalization0-v1");
            hash.update(b"points32-segments31-storage4096-lookup8050-ray4096-native4096-v1");
            hash.update(b"lookup-bound-3A-plus33T-plus2888-storage-growth33-v1");
            hash.update(b"shared-source-segment-budget-v1");
            hash.update(b"exact-success-unclipped-ray-complete-active-prefix-v1");
        }
        #[cfg(all(test, not(benilla_headless_geometry)))]
        if self.generated_flat_support {
            if !self.finite_straight_routes {
                return Err(Failure::Binding);
            }
            hash.update(b"benilla-resident-generated-flat-supported-straight-max8-v1");
            hash.update(b"sealed-current-detail-success-indexed-interior-polygon-vertices-v1");
            hash.update(b"generated-receipt-H-plus-one-cell-Q-unchanged-native-points-in-HQ-v1");
            hash.update(
                b"raw-full-footprint-exact-H-gap-body-prism-actual-varying-Z-cast-head-los-v1",
            );
            hash.update(b"checked-f32-shape-centers-and-full-outward-containment-no-epsilon-v1");
            hash.update(b"stationary-full-segment-square-directed-normalized-cast-enclosure-v1");
            hash.update(
                b"bounded-raw-rectangles93-detail-records30-triangles4096-shared-source-ray-v1",
            );
        }
        #[cfg(all(test, not(benilla_headless_geometry)))]
        if self.planar_nonpenetration {
            if !self.generated_flat_support {
                return Err(Failure::Binding);
            }
            hash.update(b"benilla-resident-generated-flat-planar-nonpenetration-max8-v1");
            hash.update(b"borrowed-full-enclosure-live-raw-collider-proof-actual-shape-v1");
            hash.update(b"exact-directed-normalized-affine-bottom-at-or-above-H-no-epsilon-v1");
            hash.update(b"all-nonterrain-full-enclosure-clear-before-root-hit-adjudication-v1");
            hash.update(b"actual-body-cast-head-los-source-los-final-freshness-preserved-v1");
            hash.update(b"no-solver-parity-toi-threshold-witness-or-normal-permission-v1");
        }
        #[cfg(all(test, not(benilla_headless_geometry)))]
        if self.endpoint_support {
            if !self.planar_nonpenetration {
                return Err(Failure::Binding);
            }
            hash.update(b"benilla-resident-supplemental-same-polygon-endpoints-max8-v1");
            hash.update(
                b"two-current-query-probes-not-original-height-calls-unchanged-native-points-v1",
            );
            hash.update(b"exclusive-indexed-or-endpoint-attribution-shared-triangles4096-v1");
            hash.update(b"same-live-generated-HQ-full-prism-actual-trajectory-and-source-los-v1");
        }
        #[cfg(all(test, not(benilla_headless_geometry)))]
        if self.source_spline_support {
            if !self.endpoint_support {
                return Err(Failure::Binding);
            }
            hash.update(b"benilla-resident-same-polygon-source-spline-envelope-max8-v1");
            hash.update(b"sealed-two-points-current-launch-origin-body-owner-freshness-v1");
            hash.update(b"software-RNE-exact-endpoint-reconstruction-monotone-successful-u01-v1");
            hash.update(b"rounded-source-body-head-extrema-full-independent-clearance-v1");
            hash.update(b"checked-terrain-hit-only-no-normalized-affine-or-solver-parity-v1");
        }
        hash.update(installed.revision);
        hash.update(b"cold-chase-clear-then-calculate-no-normalization-v1");
        hash.update(b"exact-endpoints-live-raw-local-corridor-owner-height-segment-los-v1");
        hash.update([0]); // Source segment LOS always includes M2 geometry.
        Ok(hash.finalize().into())
    }
}
#[derive(Debug)]
pub(crate) enum ResidentFilterError {
    Context(Failure),
    Query(super::super::source_pathfinder_query::ConstructorQueryError),
    Source(source_height::CompositionError),
    Terminal(benilla_native_nav::SourceSamePolygonError),
    #[cfg(all(test, not(benilla_headless_geometry)))]
    StraightTerminal(benilla_native_nav::SourceRetainedStraightError),
    #[cfg(all(test, not(benilla_headless_geometry)))]
    StraightDetail(benilla_native_nav::SourceRetainedDetailError),
}
impl std::fmt::Display for ResidentFilterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Context(error) => write!(f, "resident filter context: {error:?}"),
            Self::Query(error) => write!(f, "resident filter query: {error:?}"),
            Self::Source(error) => write!(f, "resident filter source: {error:?}"),
            Self::Terminal(error) => write!(f, "resident conditional terminal: {error:?}"),
            #[cfg(all(test, not(benilla_headless_geometry)))]
            Self::StraightTerminal(error) => write!(f, "resident straight terminal: {error:?}"),
            #[cfg(all(test, not(benilla_headless_geometry)))]
            Self::StraightDetail(error) => write!(f, "resident straight detail: {error:?}"),
        }
    }
}
impl From<Failure> for ResidentFilterError {
    fn from(e: Failure) -> Self {
        Self::Context(e)
    }
}

pub(crate) struct ResidentFilterProvider<'a, 'terrain, 'bundle, 'source> {
    identity: Rc<()>,
    geometry_policy: ResidentGeometryPolicy,
    session: Session,
    profile: Profile,
    height: &'a PreparedSourceHeight<'terrain, 'bundle, 'source>,
    query: SourcePathfinderQueryProvider,
}
/// Allocation identity only: it cannot emit facts or construct source evidence.
#[cfg(all(test, not(benilla_headless_geometry)))]
#[derive(Clone)]
pub(crate) struct ResidentProviderIdentity {
    allocation: Rc<()>,
    inventory: [u8; 32],
    source_epoch: [u8; 32],
    one_retained_route: bool,
    two_retained_routes: bool,
    finite_retained_routes: bool,
    finite_straight_routes: bool,
}
#[cfg(all(test, not(benilla_headless_geometry)))]
#[allow(dead_code, reason = "app-only resident controller component fixture")]
impl ResidentProviderIdentity {
    pub(crate) fn matches(&self, provider: &ResidentFilterProvider<'_, '_, '_, '_>) -> bool {
        Rc::ptr_eq(&self.allocation, &provider.identity)
    }
    pub(crate) fn allows_one_retained_route(&self) -> bool {
        self.one_retained_route
    }
    pub(crate) fn allows_two_retained_routes(&self) -> bool {
        self.two_retained_routes
    }
    pub(crate) fn allows_finite_retained_routes(&self) -> bool {
        self.finite_retained_routes
    }
    pub(crate) fn allows_finite_straight_routes(&self) -> bool {
        self.finite_straight_routes
    }
    pub(crate) fn matches_cache(&self, cache: &motion::PathFinderCache) -> bool {
        cache.binding.inventory == self.inventory && cache.binding.source_epoch == self.source_epoch
    }
}
/// Move-only, exact-dispatch point observations. No public fields or Route conversion.
pub(crate) struct ResidentFilterDependency {
    identity: Rc<()>,
    observation_identity: Rc<()>,
    context: AuthorityRequest,
    expires: Instant,
    observations: [PointNoWaterReply; 2],
    filter: benilla_native_nav::SourceOwnerFilter,
}
impl ResidentFilterDependency {
    #[cfg(test)]
    pub(crate) fn expire_for_test(&mut self) {
        self.expires = Instant::now();
    }
    pub(crate) fn observations(&self) -> &[PointNoWaterReply; 2] {
        &self.observations
    }
}
impl<'a, 'terrain, 'bundle, 'source> ResidentFilterProvider<'a, 'terrain, 'bundle, 'source> {
    pub(crate) fn bind(
        session: &Session,
        profile: &Profile,
        height: &'a PreparedSourceHeight<'terrain, 'bundle, 'source>,
        navigation: SourceMapQuerySnapshot,
    ) -> Result<Self, ResidentFilterError> {
        Self::bind_with_policy(
            session,
            profile,
            height,
            navigation,
            ResidentGeometryPolicy::PRIMARY,
        )
    }
    pub(crate) fn bind_with_policy(
        session: &Session,
        profile: &Profile,
        height: &'a PreparedSourceHeight<'terrain, 'bundle, 'source>,
        navigation: SourceMapQuerySnapshot,
        geometry_policy: ResidentGeometryPolicy,
    ) -> Result<Self, ResidentFilterError> {
        if height.binding().map != navigation.binding().map
            || height.generated_query_source_epoch() != navigation.binding().source_epoch
        {
            return Err(Failure::Binding.into());
        }
        require_participants(session)?;
        height
            .verify_unchanged(|| false)
            .map_err(ResidentFilterError::Source)?;
        let query = SourcePathfinderQueryProvider::bind_with_resident_filter(
            session,
            profile,
            navigation,
            height.binding(),
            &geometry_policy,
        )
        .map_err(ResidentFilterError::Query)?;
        Ok(Self {
            identity: Rc::new(()),
            geometry_policy,
            session: session.clone(),
            profile: profile.clone(),
            height,
            query,
        })
    }
    #[cfg(not(benilla_headless_geometry))]
    pub(crate) fn require_first_cold_route(&self) -> Result<(), ResidentFilterError> {
        if !self.geometry_policy.first_cold_route
            || !self.geometry_policy.dry_primary
            || self.geometry_policy.installed.is_none()
        {
            return Err(Failure::UnsupportedQuery.into());
        }
        Ok(())
    }

    #[cfg(all(test, not(benilla_headless_geometry)))]
    pub(crate) fn require_one_retained_route(&self) -> Result<(), ResidentFilterError> {
        self.require_first_cold_route()?;
        if !self.geometry_policy.one_retained_route {
            return Err(Failure::UnsupportedQuery.into());
        }
        Ok(())
    }

    #[cfg(all(test, not(benilla_headless_geometry)))]
    pub(crate) fn require_two_retained_routes(&self) -> Result<(), ResidentFilterError> {
        self.require_one_retained_route()?;
        if !self.geometry_policy.two_retained_routes {
            return Err(Failure::UnsupportedQuery.into());
        }
        Ok(())
    }

    #[cfg(all(test, not(benilla_headless_geometry)))]
    pub(crate) fn require_finite_retained_routes(&self) -> Result<(), ResidentFilterError> {
        self.require_one_retained_route()?;
        if !self.geometry_policy.finite_retained_routes || self.geometry_policy.two_retained_routes
        {
            return Err(Failure::UnsupportedQuery.into());
        }
        Ok(())
    }

    #[cfg(all(test, not(benilla_headless_geometry)))]
    pub(crate) fn require_finite_straight_routes(&self) -> Result<(), ResidentFilterError> {
        self.require_first_cold_route()?;
        if !self.geometry_policy.finite_straight_routes
            || self.geometry_policy.one_retained_route
            || self.geometry_policy.two_retained_routes
            || self.geometry_policy.finite_retained_routes
        {
            return Err(Failure::UnsupportedQuery.into());
        }
        Ok(())
    }

    #[cfg(all(test, not(benilla_headless_geometry)))]
    pub(crate) fn allows_generated_flat_support(&self) -> bool {
        self.geometry_policy.generated_flat_support
    }

    #[cfg(all(test, not(benilla_headless_geometry)))]
    pub(crate) fn require_generated_flat_support(&self) -> Result<(), ResidentFilterError> {
        self.require_finite_straight_routes()?;
        if !self.geometry_policy.generated_flat_support {
            return Err(Failure::UnsupportedQuery.into());
        }
        Ok(())
    }

    #[cfg(all(test, not(benilla_headless_geometry)))]
    pub(crate) fn allows_planar_nonpenetration(&self) -> bool {
        self.geometry_policy.planar_nonpenetration
    }

    #[cfg(all(test, not(benilla_headless_geometry)))]
    pub(crate) fn require_planar_nonpenetration(&self) -> Result<(), ResidentFilterError> {
        self.require_generated_flat_support()?;
        if !self.geometry_policy.planar_nonpenetration {
            return Err(Failure::UnsupportedQuery.into());
        }
        Ok(())
    }

    #[cfg(all(test, not(benilla_headless_geometry)))]
    pub(crate) fn allows_endpoint_support(&self) -> bool {
        self.geometry_policy.endpoint_support
    }

    #[cfg(all(test, not(benilla_headless_geometry)))]
    pub(crate) fn require_endpoint_support(&self) -> Result<(), ResidentFilterError> {
        self.require_planar_nonpenetration()?;
        if !self.allows_endpoint_support() {
            return Err(Failure::UnsupportedQuery.into());
        }
        Ok(())
    }

    #[cfg(all(test, not(benilla_headless_geometry)))]
    pub(crate) fn allows_source_spline_support(&self) -> bool {
        self.geometry_policy.source_spline_support
    }

    #[cfg(all(test, not(benilla_headless_geometry)))]
    pub(crate) fn require_source_spline_support(&self) -> Result<(), ResidentFilterError> {
        self.require_endpoint_support()?;
        if !self.allows_source_spline_support() {
            return Err(Failure::UnsupportedQuery.into());
        }
        Ok(())
    }

    #[cfg(all(test, not(benilla_headless_geometry)))]
    #[allow(dead_code, reason = "app-only resident controller component fixture")]
    pub(crate) fn ledger_identity(
        &self,
        session: &Session,
        profile: &Profile,
    ) -> Result<ResidentProviderIdentity, ResidentFilterError> {
        self.require_first_cold_route()?;
        if self.profile != *profile || self.session != *session {
            return Err(Failure::Binding.into());
        }
        Ok(ResidentProviderIdentity {
            allocation: Rc::clone(&self.identity),
            inventory: self.query.binding().inventory,
            source_epoch: self.query.binding().source_epoch,
            one_retained_route: self.geometry_policy.one_retained_route,
            two_retained_routes: self.geometry_policy.two_retained_routes,
            finite_retained_routes: self.geometry_policy.finite_retained_routes,
            finite_straight_routes: self.geometry_policy.finite_straight_routes,
        })
    }

    /// The dry adapter borrows this exact installed policy and native query owner.
    /// Compatibility is rechecked, never supplied as an arbitrary revision hash.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn resolve_dry_primary_near_point(
        &self,
        session: &Session,
        request: &AuthorityRequest,
        player: benilla_world_runtime::PlayerId,
        compatibility: &source_height::inventory::InventoryCompatibility<'_, '_, '_, '_>,
        configuration: Option<source_height::ResidentInitializationConfiguration>,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<benilla_world_runtime::native::Facts, ResidentFilterError> {
        if !self.geometry_policy.dry_primary {
            return Err(Failure::Binding.into());
        }
        self.verify_installed_owner(compatibility, compatibility.owner(), deadline, &cancelled)?;
        let dry = dry_near_point::ResidentDryNearPointProvider::bind_installed(
            self,
            player,
            configuration,
            deadline,
            &cancelled,
        )?;
        dry.resolve(session, request, deadline, cancelled)
    }

    /// Re-enter the genuine registry on a live callback, including later use.
    /// The checked original allocation is independent of joined's early refusal.
    #[cfg(test)]
    pub(crate) fn retained_constructor_for_test(
        &mut self,
        context: &AuthorityRequest,
    ) -> Result<(usize, benilla_native_nav::SourceMapQuerySlot, usize), ResidentFilterError> {
        let slot = self
            .query
            .retain_constructor(context)
            .map_err(ResidentFilterError::Query)?
            .slot();
        let dispatch = context
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?;
        let (identity, _, _) = self
            .query
            .registered_constructor_for(
                context.binding.npc.ok_or(Failure::Binding)?,
                dispatch.construction_binding(),
            )
            .ok_or(Failure::Binding)?;
        Ok((identity, slot, self.query.constructor_count()))
    }
    pub(crate) fn observe(
        &mut self,
        context: &AuthorityRequest,
        first_ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<ResidentFilterDependency, ResidentFilterError> {
        if !context
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?
            .first_construction()
        {
            return Err(Failure::Binding.into());
        }
        self.observe_inner(context, first_ticket, deadline, cancelled, None)
    }
    pub(crate) fn observe_update(
        &mut self,
        prior: &ResidentFilterDependency,
        context: &AuthorityRequest,
        first_ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<ResidentFilterDependency, ResidentFilterError> {
        // Expired point evidence cannot answer a query, but its private retained
        // filter provenance still permits fresh source observations on this owner.
        if !Rc::ptr_eq(&self.identity, &prior.identity) {
            return Err(Failure::Binding.into());
        }
        self.check_context(&prior.context, deadline, &cancelled)?;
        let retained = prior.filter;
        self.check_context(context, deadline, &cancelled)?;
        let saved = prior
            .context
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?;
        let current = context
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?;
        let Query::SourceMotion(MotionQuery::Route(old_route)) = &prior.context.query else {
            return Err(Failure::Binding.into());
        };
        let Query::SourceMotion(MotionQuery::Route(new_route)) = &context.query else {
            return Err(Failure::Binding.into());
        };
        if saved.construction_binding() != current.construction_binding()
            || old_route.owner != new_route.owner
        {
            return Err(Failure::Binding.into());
        }
        if prior.observations[1].request().ticket >= first_ticket {
            return Err(Failure::Binding.into());
        }
        self.observe_inner(context, first_ticket, deadline, cancelled, Some(retained))
    }
    fn observe_inner(
        &mut self,
        context: &AuthorityRequest,
        first_ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        retained: Option<benilla_native_nav::SourceOwnerFilter>,
    ) -> Result<ResidentFilterDependency, ResidentFilterError> {
        self.check_context(context, deadline, &cancelled)?;
        if first_ticket == 0 || first_ticket.checked_add(1).is_none() {
            return Err(Failure::Invalid.into());
        }
        // Consumes the real live constructor event, retaining its original query.
        self.query
            .retain_constructor(context)
            .map_err(ResidentFilterError::Query)?;
        let (observations, filter) = point_observations(
            self.height,
            context,
            first_ticket,
            deadline,
            &cancelled,
            retained,
            &mut |_| {},
        )?;
        self.check_context(context, deadline, &cancelled)?;
        self.height
            .verify_unchanged(&cancelled)
            .map_err(ResidentFilterError::Source)?;
        self.check_context(context, deadline, &cancelled)?;
        Ok(ResidentFilterDependency {
            identity: Rc::clone(&self.identity),
            observation_identity: Rc::new(()),
            context: context.clone(),
            expires: deadline,
            observations,
            filter,
        })
    }
    /// Reuse is limited to the identical dispatch before expiry. Movement, another
    /// dispatch or a changed capability witness requires a fresh observation.
    pub(crate) fn filter_for(
        &self,
        dependency: &ResidentFilterDependency,
        context: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<benilla_native_nav::SourceOwnerFilter, ResidentFilterError> {
        if !Rc::ptr_eq(&self.identity, &dependency.identity) || dependency.context != *context {
            return Err(Failure::Binding.into());
        }
        let saved = dependency
            .context
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?;
        let current = context
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?;
        if !saved.same_live_dispatch(current) {
            return Err(Failure::Binding.into());
        }
        let deadline = deadline.min(dependency.expires);
        self.check_context(context, deadline, &cancelled)?;
        self.height
            .verify_unchanged(&cancelled)
            .map_err(ResidentFilterError::Source)?;
        self.check_context(context, deadline, &cancelled)?;
        Ok(dependency.filter)
    }
    fn check_context(
        &self,
        context: &AuthorityRequest,
        deadline: Instant,
        cancelled: &impl Fn() -> bool,
    ) -> Result<(), ResidentFilterError> {
        if cancelled() {
            return Err(ResidentFilterError::Source(
                source_height::CompositionError::Cancelled,
            ));
        }
        if Instant::now() >= deadline {
            return Err(Failure::Deadline.into());
        }
        validate_route_context(&self.session, &self.profile, context)?;
        Ok(())
    }
}

fn point_observations(
    height: &PreparedSourceHeight<'_, '_, '_>,
    context: &AuthorityRequest,
    first_ticket: u64,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
    retained: Option<benilla_native_nav::SourceOwnerFilter>,
    observe: &mut impl FnMut(u8),
) -> Result<
    (
        [PointNoWaterReply; 2],
        benilla_native_nav::SourceOwnerFilter,
    ),
    ResidentFilterError,
> {
    if first_ticket == 0 || first_ticket.checked_add(1).is_none() {
        return Err(Failure::Invalid.into());
    }
    let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
        return Err(Failure::UnsupportedQuery.into());
    };
    let capabilities = route.owner.capabilities().map_err(|_| Failure::Binding)?;
    let position = [
        route.owner_position.x.bits(),
        route.owner_position.y.bits(),
        route.owner_position.z.bits(),
    ];
    let mut replies = [None, None];
    let mut next = 0usize;
    let mut liquid_query = |mask| {
        let required = match next {
            0 => route_filter::LiquidMask::AllLiquids,
            1 => route_filter::LiquidMask::WaterOrOcean,
            _ => return Err(Failure::Binding.into()),
        };
        if mask != required {
            return Err(Failure::Binding.into());
        }
        observe(next as u8);
        let reply = height
            .point_no_water(
                PointNoWaterRequest {
                    binding: height.binding(),
                    ticket: first_ticket + next as u64,
                    position,
                    mask: if next == 0 { 15 } else { 10 },
                },
                deadline,
                cancelled,
            )
            .map_err(ResidentFilterError::Source)?;
        replies[next] = Some(reply);
        next += 1;
        route_filter::NoWater::try_from_status(0).map_err(|_| Failure::Water.into())
    };
    let filter = if let Some(retained) = retained {
        liquid_query(route_filter::LiquidMask::AllLiquids)?;
        liquid_query(route_filter::LiquidMask::WaterOrOcean)?;
        // Both predicates are false, so updateFilter performs no read/write
        // of include flags and never calls getNavTerrain.
        retained
    } else {
        let created = route_filter::create_dry_filter(capabilities, liquid_query).map_err(
            |error| match error {
                route_filter::Error::UnsupportedCapabilities => {
                    ResidentFilterError::Context(Failure::UnsupportedQuery)
                }
                route_filter::Error::Liquid(error) => error,
            },
        )?;
        benilla_native_nav::SourceOwnerFilter {
            include_flags: created.include_flags(),
            exclude_flags: created.exclude_flags(),
        }
    };

    Ok((
        [
            replies[0].ok_or(Failure::Binding)?,
            replies[1].ok_or(Failure::Binding)?,
        ],
        filter,
    ))
}
