// SPDX-License-Identifier: GPL-3.0-or-later
//! One-shot cold constructor objects for an isolated component grid operation.
//! CMaNGOS 8ec338a1 Object.cpp WorldObject::WorldObject, Camera.h ViewPoint,
//! Map.cpp AddToGrid(Creature*). Not AddToWorld, lookup, or live membership.
use super::*;
#[cfg(any(test, feature = "synthetic-native-tests"))]
use benilla_sim::resident_map::compute_cell_pair;
use benilla_sim::resident_map::CellPair;

#[derive(Debug)]
struct FreshViewPoint {
    cameras: Vec<u64>,
    grid: Option<CellPair>,
}

/// Minted only by the explicit generation-zero constructor adapter. Neither
/// cloning nor deserializing a pose observation can create this object.
#[derive(Debug)]
pub struct FreshCreatureCellObject {
    generation: CreatureGenerationOwner,
    identity: SourceStaticCreatureIdentity,
    map_id: u32,
    instance_id: u32,
    initial_position_bits: [u32; 2],
    cell: CellPair,
    active: bool,
    viewpoint: FreshViewPoint,
}
impl FreshCreatureCellObject {
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(super) fn new(
        profile: &Profile,
        world: crate::WorldId,
        source: &CreaturePoseCellSource,
        generation: &CreatureGenerationOwner,
        projected: &NpcProfile,
    ) -> Result<Self, NativeError> {
        let npc = generation.npc_id();
        let (source_identity, identity) = source
            .source_identity(npc.spawn)
            .ok_or(NativeError::AuthorityBinding)?;
        let configuration =
            profile
                .pathfinder_configuration()
                .ok_or(NativeError::AuthorityUnavailable(
                    "fresh creature map instance",
                ))?;
        configuration.validate(profile)?;
        if generation.world() != world
            || generation.profile_identity() != profile.digest()
            || generation.source_identity() != source_identity
            || generation.history().len() != 1
            || npc.generation != 0
            || npc.spawn != projected.ai.creature_id
            || generation.result().entry != identity.entry()
        {
            return Err(NativeError::AuthorityBinding);
        }
        let initial_position_bits = [projected.ai.spawn.x.bits(), projected.ai.spawn.y.bits()];
        let cell = compute_cell_pair(initial_position_bits[0], initial_position_bits[1])
            .map_err(|_| NativeError::UnsupportedMechanic("fresh creature cell coordinate"))?;
        Ok(Self {
            generation: generation.clone(),
            identity,
            map_id: profile.map_id,
            instance_id: configuration.selected().source_instance_id,
            initial_position_bits,
            cell,
            active: false,
            viewpoint: FreshViewPoint {
                cameras: Vec::new(),
                grid: None,
            },
        })
    }
    pub fn world(&self) -> crate::WorldId {
        self.generation.world()
    }
    pub fn profile_identity(&self) -> crate::Digest {
        self.generation.profile_identity()
    }
    pub fn source_identity(&self) -> crate::Digest {
        self.generation.source_identity()
    }
    pub fn generation(&self) -> &CreatureGenerationOwner {
        &self.generation
    }
    pub fn npc(&self) -> crate::NpcId {
        self.generation.npc_id()
    }
    pub fn identity(&self) -> SourceStaticCreatureIdentity {
        self.identity
    }
    pub fn map_id(&self) -> u32 {
        self.map_id
    }
    pub fn instance_id(&self) -> u32 {
        self.instance_id
    }
    pub fn initial_position_bits(&self) -> [u32; 2] {
        self.initial_position_bits
    }
    pub fn cell(&self) -> CellPair {
        self.cell
    }
    pub fn is_active(&self) -> bool {
        self.active || !self.viewpoint.cameras.is_empty()
    }
    pub fn viewer_count(&self) -> usize {
        self.viewpoint.cameras.len()
    }
    pub fn viewpoint_grid(&self) -> Option<CellPair> {
        self.viewpoint.grid
    }

    /// Consume the fresh object and assign constructor CurrentCell. The caller
    /// owns insertion into its actual cell store. This arithmetic/binding check
    /// alone proves neither registration nor even storage in a grid.
    pub fn attach_initial_cell(
        self,
        expected_map: u32,
        expected_instance: u32,
        expected_cell: CellPair,
    ) -> Result<AttachedInitialCreatureCell, NativeError> {
        if self.map_id != expected_map
            || self.instance_id != expected_instance
            || !expected_cell.validate()
            || self.cell != expected_cell
        {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(AttachedInitialCreatureCell {
            object: self,
            current_cell: expected_cell,
        })
    }
}

/// Retains the original constructor object. No serialization, cloning, restore,
/// post-step membership, or registration semantics are supplied.
#[derive(Debug)]
pub struct AttachedInitialCreatureCell {
    object: FreshCreatureCellObject,
    current_cell: CellPair,
}
impl AttachedInitialCreatureCell {
    pub fn object(&self) -> &FreshCreatureCellObject {
        &self.object
    }
    pub fn current_cell(&self) -> CellPair {
        self.current_cell
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn profile() -> Profile {
        let mut p = synthetic_source_spline_fixture::surviving_profile();
        let maps = synthetic_map_classification_fixture::catalog(p.map_id, 0);
        p.initialize_map_classification(&maps).unwrap();
        p.initialize_pathfinder_configuration(
            SelectedMMapConfiguration {
                enabled: true,
                effective_disabled_maps: EffectiveDisabledMMapSet::Complete(Default::default()),
                source_instance_id: 0,
            },
            &maps,
        )
        .unwrap();
        p
    }
    fn construct(
        profile: Profile,
        hook: &mut dyn FnMut(FreshCreatureCellObject) -> Result<(), NativeError>,
    ) -> Result<NativeWorld, NativeError> {
        NativeWorld::synthetic_component_test_with_initialization_and_fresh_creatures(
            profile,
            [1; 16],
            synthetic_fixture::player(),
            42,
            None,
            &mut synthetic_source_spline_fixture::CompleteAuthority::default(),
            hook,
        )
    }
    #[test]
    fn constructor_objects_are_ordered_owned_and_do_not_change_checkpoint() {
        let p = profile();
        let mut attached = Vec::new();
        let w = construct(p.clone(), &mut |object| {
            assert_eq!(object.world(), [1; 16]);
            assert_eq!(object.profile_identity(), p.digest());
            assert_eq!(object.npc().generation, 0);
            assert_eq!(object.generation().history().len(), 1);
            assert!(!object.is_active());
            assert_eq!(object.viewer_count(), 0);
            assert_eq!(object.viewpoint_grid(), None);
            let cell = object.cell();
            attached.push(object.attach_initial_cell(p.map_id, 0, cell)?);
            Ok(())
        })
        .unwrap();
        assert_eq!(
            attached
                .iter()
                .map(|a| a.object().npc().spawn)
                .collect::<Vec<_>>(),
            p.creature_generations.as_ref().unwrap().cold_entry_order()
        );
        for a in &attached {
            assert_eq!(a.current_cell(), a.object().cell());
            assert_eq!(a.object().viewpoint_grid(), None);
        }
        let unchanged = NativeWorld::synthetic_component_test(
            p.clone(),
            [1; 16],
            synthetic_fixture::player(),
            42,
        )
        .unwrap();
        assert_eq!(w.checkpoint().unwrap(), unchanged.checkpoint().unwrap());
        let restored =
            NativeWorld::restore_synthetic_component_test(p, w.checkpoint().unwrap()).unwrap();
        assert_eq!(restored.checkpoint().unwrap(), w.checkpoint().unwrap());
    }
    #[test]
    fn wrong_cell_slot_and_callback_failure_abort_constructor() {
        for mismatch in 0..3 {
            let p = profile();
            let mut seen = 0;
            let error = construct(p, &mut |object| {
                seen += 1;
                let map = object.map_id() + u32::from(mismatch == 0);
                let instance = object.instance_id() + u32::from(mismatch == 1);
                let cell = if mismatch == 2 {
                    compute_cell_pair(16000_f32.to_bits(), 16000_f32.to_bits()).unwrap()
                } else {
                    object.cell()
                };
                object.attach_initial_cell(map, instance, cell)?;
                Ok(())
            })
            .err()
            .unwrap();
            assert_eq!(error, NativeError::AuthorityBinding);
            assert_eq!(seen, 1);
        }
        let mut seen = 0;
        assert_eq!(
            construct(profile(), &mut |_| {
                seen += 1;
                Err(NativeError::AuthorityUnavailable("test callback"))
            })
            .err(),
            Some(NativeError::AuthorityUnavailable("test callback"))
        );
        assert_eq!(seen, 1);
    }
    #[test]
    fn callback_failure_after_first_retained_object_requires_transaction_discard() {
        let p = profile();
        assert!(p.npcs.len() > 1);
        let mut retained = Vec::new();
        let mut seen = 0;
        let outcome = construct(p.clone(), &mut |object| {
            seen += 1;
            if seen == 2 {
                return Err(NativeError::AuthorityUnavailable("later cold entry"));
            }
            let cell = object.cell();
            retained.push(object.attach_initial_cell(p.map_id, 0, cell)?);
            Ok(())
        });
        assert_eq!(
            outcome.err(),
            Some(NativeError::AuthorityUnavailable("later cold entry"))
        );
        assert_eq!(seen, 2);
        assert_eq!(retained.len(), 1);
        // The low-level hook intentionally cannot roll back caller storage;
        // the app wrapper must own and discard this entire transaction.
        drop(retained);
        assert!(construct(p, &mut |_| Ok(())).is_ok());
    }
    #[test]
    fn missing_qualified_map_fails_before_any_callback() {
        let p = synthetic_source_spline_fixture::surviving_profile();
        let mut seen = 0;
        assert!(construct(p, &mut |_| {
            seen += 1;
            Ok(())
        })
        .is_err());
        assert_eq!(seen, 0);
    }
}
