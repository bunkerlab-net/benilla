// SPDX-License-Identifier: GPL-3.0-or-later
//! Distinct random-point endpoint/cache lineage; it never borrows Chase selection.
use super::super::products::{
    ground_height::{MapHeightReply, MapHeightRequest},
    liquid::{LiquidReply, LiquidRequest, LiquidStatus},
};
use super::*;
use benilla_native_nav::{
    SourceWanderBudget, SOURCE_WANDER_MAX_BUILD_LOOKUP_PRIMITIVES,
    SOURCE_WANDER_MAX_ENDPOINT_LOOKUP_PRIMITIVES,
};
use benilla_sim::creature_init::ModelSelectionReceipt;
use benilla_world_runtime::native::{
    CreatureCollisionHeightWitness, CreatureCollisionWidthWitness, CreatureRouteRequest,
};

pub(super) const POLICY: &[u8] = b"cmangos-random-point-dry-same-poly-owned-prefix-tail-v1";

/// Owner model evidence is common; a target receipt exists only for Chase.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in super::super) struct SamePolygonModel {
    pub selection: ModelSelectionReceipt,
    pub width: CreatureCollisionWidthWitness,
    pub owner_height: CreatureCollisionHeightWitness,
    pub(super) chase: Option<ChaseGateModel>,
}
pub(in super::super) fn same_polygon_model(
    profile: &Profile,
    world: [u8; 16],
    player: PlayerId,
    route: &CreatureRouteRequest,
) -> Result<SamePolygonModel, Failure> {
    same_polygon_model_with_initialized(profile, world, player, route, None)
}
pub(in super::super) fn same_polygon_model_with_initialized(
    profile: &Profile,
    world: [u8; 16],
    player: PlayerId,
    route: &CreatureRouteRequest,
    initialized: Option<&super::super::source_route_filter::InitializedOwnerValidation>,
) -> Result<SamePolygonModel, Failure> {
    if route.request.kind == motion::RouteKind::HomeForceDestination {
        if route.request.target.is_some()
            || route.request.wander.is_some()
            || route.request.prior_cache.is_some()
            || route.target_player_height.is_some()
            || route.target_player_water.is_some()
        {
            return Err(Failure::Binding);
        }
        return owner_only_model(profile, route, initialized);
    }
    if !matches!(
        route.request.kind,
        motion::RouteKind::Wander | motion::RouteKind::WanderPrepare
    ) {
        let chase = super::super::source_route_chase_gate::gate_model_with_initialized(
            profile,
            world,
            player,
            route,
            initialized,
        )?;
        return Ok(SamePolygonModel {
            selection: chase.selection.clone(),
            width: chase.width.clone(),
            owner_height: chase.owner_height.clone(),
            chase: Some(chase),
        });
    }
    route
        .request
        .validate_wander()
        .map_err(|_| Failure::Binding)?;
    let motion::DefaultMovement::Wander {
        center: selected_center,
        radius: selected_radius,
    } = profile
        .source_motion
        .as_ref()
        .ok_or(Failure::Binding)?
        .npc(route.owner.npc().spawn)
        .map_err(|_| Failure::Binding)?
        .default_movement
    else {
        return Err(Failure::Binding);
    };
    if route
        .request
        .wander
        .as_deref()
        .is_some_and(|sample| sample.radius != selected_radius)
    {
        return Err(Failure::Binding);
    }
    let center = if let Some(sample) = route.request.wander.as_deref() {
        sample.center
    } else {
        route.request.goal
    };
    if route.request.target.is_some()
        || center != selected_center
        || center
            != profile
                .npc(route.owner.npc().spawn)
                .ok_or(Failure::Binding)?
                .ai
                .spawn
    {
        return Err(Failure::Binding);
    }
    owner_only_model(profile, route, initialized)
}
fn owner_only_model(
    profile: &Profile,
    route: &CreatureRouteRequest,
    initialized: Option<&super::super::source_route_filter::InitializedOwnerValidation>,
) -> Result<SamePolygonModel, Failure> {
    if let Some(initialized) = initialized {
        let validated = initialized.for_route(profile, route)?;
        return Ok(SamePolygonModel {
            selection: validated.selection.clone(),
            width: validated.width.clone(),
            owner_height: validated.height.clone(),
            chase: None,
        });
    }
    let width = route
        .require_collision_width(profile)
        .map_err(|_| Failure::Binding)?;
    let selection = width
        .validated_model_selection(profile, route.owner.npc(), route.owner.initialization())
        .map_err(|_| Failure::Binding)?;
    let owner_height = route
        .require_collision_height(profile)
        .map_err(|_| Failure::Binding)?;
    Ok(SamePolygonModel {
        selection,
        width: width.clone(),
        owner_height: owner_height.clone(),
        chase: None,
    })
}

pub(super) enum SamePolygonTerminal {
    Chase(LoadedSourceSamePolygon),
    Home(LoadedSourceHomeSamePolygon),
    Wander(LoadedSourceWanderTerminal),
}
impl SamePolygonTerminal {
    pub(super) fn chase(&self) -> Option<&LoadedSourceSamePolygon> {
        match self {
            Self::Chase(t) => Some(t),
            Self::Home(_) | Self::Wander(_) => None,
        }
    }
    pub(super) fn wander(&self) -> Option<&LoadedSourceWanderTerminal> {
        match self {
            Self::Wander(t) => Some(t),
            Self::Chase(_) | Self::Home(_) => None,
        }
    }
    pub(super) fn filter(&self) -> SourceOwnerFilter {
        match self {
            Self::Chase(t) => t.compatibility().filter(),
            Self::Home(t) => t.compatibility().filter(),
            Self::Wander(t) => t.filter(),
        }
    }
    pub(super) fn storage(&self) -> &[u64] {
        match self {
            Self::Chase(t) => t.compatibility().storage(),
            Self::Home(t) => t.compatibility().storage(),
            Self::Wander(t) => t.storage(),
        }
    }
    pub(super) fn active_len(&self) -> usize {
        match self {
            Self::Chase(t) => t.compatibility().active_len(),
            Self::Home(t) => t.compatibility().active_len(),
            Self::Wander(t) => t.active_len(),
        }
    }
    pub(super) fn path_type(&self) -> u32 {
        match self {
            Self::Chase(t) => t.path_type(),
            Self::Home(t) => t.path_type(),
            Self::Wander(t) => t.path_type(),
        }
    }
    pub(super) fn points(&self) -> &[[u32; 3]] {
        match self {
            Self::Chase(t) => t.points(),
            Self::Home(t) => t.points(),
            Self::Wander(t) => t.points(),
        }
    }
    pub(super) fn actual_end(&self) -> [u32; 3] {
        match self {
            Self::Chase(t) => t.actual_end(),
            Self::Home(t) => t.actual_end(),
            Self::Wander(t) => t.actual_end(),
        }
    }
    pub(super) fn same_observations(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Chase(a), Self::Chase(b)) => {
                a.compatibility() == b.compatibility() && a.points() == b.points()
            }
            (Self::Home(a), Self::Home(b)) => {
                a.policy() == b.policy()
                    && a.force_destination() == b.force_destination()
                    && a.straight_path() == b.straight_path()
                    && a.compatibility() == b.compatibility()
                    && a.points() == b.points()
            }
            (Self::Wander(a), Self::Wander(b)) => {
                a.before_storage() == b.before_storage()
                    && a.before_active_len() == b.before_active_len()
                    && a.raw_end() == b.raw_end()
                    && a.endpoint_lookup() == b.endpoint_lookup()
                    && a.endpoint_trace() == b.endpoint_trace()
                    && a.height_status() == b.height_status()
                    && a.height_after() == b.height_after()
                    && a.start_lookup() == b.start_lookup()
                    && a.end_lookup() == b.end_lookup()
                    && a.build_trace() == b.build_trace()
                    && a.storage() == b.storage()
                    && a.filter() == b.filter()
                    && a.points() == b.points()
                    && a.path_type() == b.path_type()
                    && a.no_path_reason() == b.no_path_reason()
            }
            _ => false,
        }
    }
}

/// PathFinder.cpp:1420-1429 runs both dry predicates after BuildPolyPath success.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WanderTail {
    pub water: LiquidReply,
    pub ground: MapHeightReply,
    pub above_ground: bool,
}
pub(crate) struct WanderSourceReceipt<'a> {
    pub preparation: &'a AuthorityRequest,
    pub constructor_filter: Option<&'a RouteFilterDependency>,
    pub update_filter: &'a RouteFilterUpdateDependency,
    pub clear_before_active: usize,
    pub clear_after_active: usize,
    pub selected: bool,
    pub tail: Option<&'a WanderTail>,
}

pub(super) struct WanderPreparation {
    pub(super) prepared: PreparedCalculation,
    pub(super) constructor_filter: Option<RouteFilterDependency>,
    pub(super) filter: RouteFilterUpdateDependency,
    pub(super) native: benilla_native_nav::LoadedSourceWanderPrefix,
}
const MAX_WANDER_PREFIXES: usize = MAX_GENERATORS * MAX_CANDIDATES_PER_GENERATOR;

impl SourceSamePolygonRouteProvider {
    /// The only pre-draw reply is emitted after genuine clear/update/select.
    pub(in super::super) fn prepare_wander(
        &mut self,
        session: &Session,
        context: &AuthorityRequest,
        first_ticket: u64,
        deadline: Instant,
    ) -> Result<Facts, SamePolygonError> {
        self.prepare_wander_observed(session, context, first_ticket, deadline, |_| {})
    }
    pub(super) fn prepare_wander_observed(
        &mut self,
        session: &Session,
        context: &AuthorityRequest,
        first_ticket: u64,
        deadline: Instant,
        mut observe: impl FnMut(Observation),
    ) -> Result<Facts, SamePolygonError> {
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::Binding.into());
        };
        if route.request.kind != motion::RouteKind::WanderPrepare || self.wander_height.is_none() {
            return Err(Failure::UnsupportedQuery.into());
        }
        let validation = self.validate_calculation(session, context, first_ticket, deadline)?;
        let duplicate = self.wander_prefixes.iter().position(|prefix| {
            prefix.prepared.validation.context == *context
                && prefix
                    .prepared
                    .validation
                    .dispatch
                    .same_live_dispatch(&validation.dispatch)
        });
        if let Some(index) = duplicate {
            let prefix = Rc::clone(&self.wander_prefixes[index]);
            self.check_prepared(&prefix.prepared)?;
        } else {
            if self.wander_prefixes.len() >= MAX_WANDER_PREFIXES {
                return Err(SamePolygonError::CandidateBudget);
            }
            self.wander_prefixes
                .try_reserve(1)
                .map_err(|_| SamePolygonError::CandidateBudget)?;
            let dependencies =
                self.prepare_constructor(&validation, None, deadline, &mut observe)?;
            let prepared = PreparedCalculation {
                validation,
                dependencies,
            };
            let gate = self.calculate_gate(&prepared, deadline, &mut observe)?;
            let (filter, native) =
                self.calculate_wander_prefix(&prepared, &gate, deadline, &mut observe)?;
            self.retain_wander_prefix(prepared, gate, filter, native);
        }
        let facts = Facts::SourceMotion(benilla_world_runtime::native::MotionFacts::Spatial(
            Box::new(motion::Reply {
                request: motion::Query::Route(route.request.clone()),
                fact: motion::Fact::WanderPrepared,
            }),
        ));
        super::super::core::validate_facts(session, context, &facts)?;
        Ok(facts)
    }
    pub(super) fn cached_wander_preparation(
        &mut self,
        validation: &ValidatedCalculation,
    ) -> Result<Option<Rc<WanderPreparation>>, SamePolygonError> {
        if self.wander_height.is_none() {
            return Err(Failure::UnsupportedQuery.into());
        }
        let prefix = self
            .wander_prefixes
            .iter()
            .find(|prefix| {
                prefix.prepared.validation.context == validation.context
                    && prefix
                        .prepared
                        .validation
                        .dispatch
                        .same_live_dispatch(&validation.dispatch)
            })
            .cloned();
        if let Some(prefix) = &prefix {
            self.check_prepared(&prefix.prepared)?;
        }
        Ok(prefix)
    }
    pub(super) fn calculate_wander_prefix(
        &mut self,
        prepared: &PreparedCalculation,
        gate: &GateDependencies,
        deadline: Instant,
        mut observe: impl FnMut(Observation),
    ) -> Result<
        (
            RouteFilterUpdateDependency,
            benilla_native_nav::LoadedSourceWanderPrefix,
        ),
        SamePolygonError,
    > {
        self.check_prepared(prepared)?;
        if self.wander_height.is_none() {
            return Err(Failure::UnsupportedQuery.into());
        }
        if self.wander_prefixes.len() >= MAX_WANDER_PREFIXES {
            return Err(SamePolygonError::CandidateBudget);
        }
        self.wander_prefixes
            .try_reserve(1)
            .map_err(|_| SamePolygonError::CandidateBudget)?;
        let prior = prepared
            .dependencies
            .prior
            .as_ref()
            .map(|prior| prior.terminal.wander().ok_or(Failure::Binding))
            .transpose()?;
        let retained_filter = if let Some(prior) = prior {
            prior.filter()
        } else {
            let created = gate.constructor_filter.as_ref().ok_or(Failure::Binding)?;
            SourceOwnerFilter {
                include_flags: created.filter().include_flags(),
                exclude_flags: created.filter().exclude_flags(),
            }
        };
        let mut constructor = self.queries.reborrow(&prepared.dependencies.constructor)?;
        let mut native = constructor.prepare_wander(
            prior,
            retained_filter,
            self.binding.configuration.normalization,
            SourceWanderBudget {
                endpoint_lookup_primitives: SOURCE_WANDER_MAX_ENDPOINT_LOOKUP_PRIMITIVES,
                build_lookup_primitives: SOURCE_WANDER_MAX_BUILD_LOOKUP_PRIMITIVES,
                terminal_primitives: 1,
            },
            deadline,
        )?;
        observe(Observation::WanderClear);
        let filter = self
            .filter
            .reborrow(&prepared.validation.filter_context)?
            .query_retained_update(
                RouteFilterRequest {
                    snapshot: self.binding.liquid,
                    context: prepared.validation.context.clone(),
                    first_ticket: prepared.validation.first_ticket + 2,
                },
                retained_filter,
                None,
                deadline,
            )?;
        observe(Observation::CalculateFilter);
        constructor.select_wander_prefix(&mut native, deadline)?;
        observe(Observation::SelectedQuery);
        if native.cleared_active_len() != 0
            || !native.current_selected()
            || native.filter() != filter.filter()
        {
            return Err(Failure::Binding.into());
        }
        require_time(deadline)?;

        Ok((filter, native))
    }
    pub(super) fn retain_wander_prefix(
        &mut self,
        prepared: PreparedCalculation,
        gate: GateDependencies,
        filter: RouteFilterUpdateDependency,
        native: benilla_native_nav::LoadedSourceWanderPrefix,
    ) -> Rc<WanderPreparation> {
        // Equal live jobs share the completed allocation; neither may evict the
        // other's still-pending prefix after both passed the constructor phase.
        if let Some(prefix) = self.wander_prefixes.iter().find(|prefix| {
            prefix.prepared.validation.context == prepared.validation.context
                && prefix
                    .prepared
                    .validation
                    .dispatch
                    .same_live_dispatch(&prepared.validation.dispatch)
        }) {
            return Rc::clone(prefix);
        }
        // Replace only the same rolled-back context, never a different pending prefix.
        self.wander_prefixes
            .retain(|prefix| prefix.prepared.validation.context != prepared.validation.context);
        let prefix = Rc::new(WanderPreparation {
            prepared,
            constructor_filter: gate.constructor_filter,
            filter,
            native,
        });
        self.wander_prefixes.push(Rc::clone(&prefix));
        prefix
    }
    pub(super) fn wander_prefix(
        &self,
        validation: &ValidatedCalculation,
    ) -> Result<Rc<WanderPreparation>, SamePolygonError> {
        let context = &validation.context;
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::Binding.into());
        };
        let sample = route.request.wander.as_deref().ok_or(Failure::Binding)?;
        let prefix = self
            .wander_prefixes
            .iter()
            .find(|prefix| {
                prefix.prepared.validation.dispatch.dispatch_binding() == sample.prepared
                    && prefix.prepared.validation.dispatch.construction_binding()
                        == validation.dispatch.construction_binding()
                    && prefix.prepared.validation.context.binding == context.binding
            })
            .ok_or(Failure::Binding)?;
        let Query::SourceMotion(MotionQuery::Route(before)) =
            &prefix.prepared.validation.context.query
        else {
            return Err(Failure::Binding.into());
        };
        if before.request.goal != sample.center
            || before.request.origin != route.request.origin
            || before.request.prior_cache != route.request.prior_cache
            || before.owner != route.owner
            || before.request.owner_collision_height != route.request.owner_collision_height
            || prefix.prepared.validation.model != validation.model
            || !prefix.native.current_selected()
        {
            return Err(Failure::Binding.into());
        }
        Ok(Rc::clone(prefix))
    }
    pub(super) fn prepare_wander_continuation(
        &mut self,
        validation: &ValidatedCalculation,
        job: Option<&super::super::core::RouteJob<'_>>,
    ) -> Result<ConstructorDependencies, SamePolygonError> {
        let prefix = self.wander_prefix(validation)?;
        self.check_prepared(&prefix.prepared)?;
        let constructor = match job {
            Some(job) => self.queries.continue_wander_constructor_for_job(
                &prefix.prepared.dependencies.constructor,
                job,
            )?,
            None => self.queries.continue_wander_constructor(
                &prefix.prepared.dependencies.constructor,
                &validation.context,
            )?,
        };
        Ok(ConstructorDependencies {
            prior: prefix.prepared.dependencies.prior.clone(),
            constructor,
            reserved: true,
        })
    }
    pub(super) fn calculate_wander_terminal(
        &mut self,
        prepared: &PreparedCalculation,
        dependencies: &GateDependencies,
        deadline: Instant,
        mut observe: impl FnMut(Observation),
    ) -> Result<TerminalDependencies, SamePolygonError> {
        let context = &prepared.validation.context;
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::Binding.into());
        };
        if dependencies.gate.is_some() || prepared.validation.model.chase.is_some() {
            return Err(Failure::Binding.into());
        }
        let prefix = self.wander_prefix(&prepared.validation)?;
        let filter = prefix.filter.clone();
        let constructor = self.queries.reborrow(&prepared.dependencies.constructor)?;
        let selected = constructor.continue_wander_prefix(&prefix.native)?;
        // Samples and the source near-zero test precede owner-ignore and HaveTile.
        let squared = route
            .request
            .origin
            .distance_squared(route.request.goal)
            .map_err(|_| Failure::Invalid)?;
        if squared.as_f32() < f32::from_bits(0x3c23_d70a) {
            let terminal = selected.loaded_prepared_wander_no_path(&prefix.native, deadline)?;
            terminal
                .check_query(&selected)
                .map_err(|_| Failure::Binding)?;
            observe(Observation::Terminal);
            if terminal.path_type() != 8
                || terminal.active_len() != 0
                || !terminal.points().is_empty()
            {
                return Err(Failure::Binding.into());
            }
            return self.retain_terminal(
                prepared,
                SamePolygonTerminal::Wander(terminal),
                None,
                None,
                None,
                filter,
                deadline,
            );
        }
        let owner_ignore = match &prepared.validation.owner_ignore {
            Some(receipt) => receipt.clone(),
            None => route
                .require_owner_ignore_pathfinding(&self.profile)
                .map_err(|_| Failure::Binding)?
                .clone(),
        };
        observe(Observation::OwnerIgnore);
        if owner_ignore.ignore_pathfinding() {
            return Err(Failure::UnsupportedQuery.into());
        }
        let tiles = selected
            .tile_presence(deadline)
            .map_err(|e| SamePolygonError::Constructor(ConstructorQueryError::Native(e)))?;
        selected
            .check_tile_presence(&tiles)
            .map_err(|e| SamePolygonError::Constructor(ConstructorQueryError::Native(e)))?;
        observe(Observation::HaveTile);
        if !tiles.both_present() {
            return Err(Failure::MissingGeometry.into());
        }
        let terminal = selected.loaded_prepared_wander_same_polygon(&prefix.native, deadline)?;
        terminal
            .check_query(&selected)
            .map_err(|_| Failure::Binding)?;
        observe(Observation::Terminal);
        let valid_shape = match terminal.path_type() {
            1 => {
                terminal.active_len() == 1
                    && terminal.points() == [bits(route.request.origin), terminal.actual_end()]
            }
            8 => {
                terminal.active_len() == 0
                    && terminal.points().is_empty()
                    && matches!(
                        terminal.no_path_reason(),
                        Some(
                            benilla_native_nav::SourceWanderNoPathReason::EndpointMissing
                                | benilla_native_nav::SourceWanderNoPathReason::EndpointHeight
                        )
                    )
            }
            _ => false,
        };
        if terminal.raw_end() != bits(route.request.goal)
            || terminal.filter() != filter.filter()
            || !valid_shape
        {
            return Err(Failure::Binding.into());
        }
        let height = self
            .wander_height
            .as_ref()
            .ok_or(Failure::MissingGeometry)?;
        let water_request = LiquidRequest {
            snapshot: self.binding.liquid,
            ticket: prepared.validation.first_ticket + 4,
            position: bits(route.request.origin),
        };
        let water = self
            .filter
            .dry_liquid()
            .query(water_request, deadline)
            .map_err(Failure::product)?;
        if water.request != water_request || water.status != LiquidStatus::NoWater {
            return Err(Failure::Binding.into());
        }
        observe(Observation::WanderWater);
        let ground_request = MapHeightRequest {
            snapshot: height.binding(),
            ticket: prepared.validation.first_ticket + 5,
            position: bits(route.request.origin),
            swim: false,
        };
        let ground = height
            .query(ground_request, deadline)
            .map_err(Failure::product)?;
        if ground.request != ground_request {
            return Err(Failure::Binding.into());
        }
        let floor = f32::from_bits(ground.height);
        if !floor.is_finite() {
            return Err(Failure::Invalid.into());
        }
        let above_ground = (route.request.origin.z.as_f32() - floor).abs() > 0.5;
        observe(Observation::WanderGround);
        // Levitation/hover shortcut behavior is outside this grounded successful branch.
        if above_ground {
            return Err(Failure::UnsupportedQuery.into());
        }
        self.retain_terminal(
            prepared,
            SamePolygonTerminal::Wander(terminal),
            Some(WanderTail {
                water,
                ground,
                above_ground,
            }),
            Some(owner_ignore),
            Some(tiles),
            filter,
            deadline,
        )
    }
}
