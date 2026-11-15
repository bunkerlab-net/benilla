// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! Camera.h:89-115; Map.cpp:501-504,1335-1350; GridNotifiers.cpp:32-38.
use super::*;
use benilla_sim::resident_map::{compute_cell_pair, source_cell_visit};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Default)]
pub(super) struct WorldVisibility {
    viewpoint_cell: Option<CellIdentity>,
    cameras: Vec<ObjectHandle>,
    override_bits: u32,
    client_guids: BTreeSet<SourceGuid>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceVisibilityDistance {
    Normal,
    Tiny,
    Small,
    Large,
    Gigantic,
    Infinite,
}
impl SourceVisibilityDistance {
    pub fn bits(self) -> u32 {
        // ObjectVisibility.cpp:25-33; ObjectDefines.h:32-38.
        match self {
            Self::Normal => 100f32.to_bits(),
            Self::Tiny => 25f32.to_bits(),
            Self::Small => 50f32.to_bits(),
            Self::Large => 200f32.to_bits(),
            Self::Gigantic => 400f32.to_bits(),
            Self::Infinite => 0x4405_5555,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct SourceVisibilityUpdate {
    pub object: ObjectHandle,
    pub map_distance_bits: u32,
    pub bounding_radius_bits: u32,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceVisibilityCensus {
    pub considered_cells: usize,
    pub loaded_cells: usize,
    pub cameras: usize,
    pub unvisited_guids: usize,
    pub residual_players: usize,
}
impl SourceObjects {
    fn world_visibility(&self, object: ObjectHandle) -> Result<&WorldVisibility> {
        self.get(object)?
            .visibility
            .as_ref()
            .ok_or_else(|| refuse("execution.visibility.world_object_type"))
    }
    fn world_visibility_mut(&mut self, object: ObjectHandle) -> Result<&mut WorldVisibility> {
        self.get_mut(object)?
            .visibility
            .as_mut()
            .ok_or_else(|| refuse("execution.visibility.world_object_type"))
    }
    pub fn visibility_override_bits(&self, object: ObjectHandle) -> Result<u32> {
        Ok(self.world_visibility(object)?.override_bits)
    }
    pub fn set_visibility_distance_override(
        &mut self,
        object: ObjectHandle,
        distance: SourceVisibilityDistance,
    ) -> Result<()> {
        // ObjectVisibility.cpp:44-50 ignores player overrides, not existing non-player values.
        if self.get(object)?.kind != SourceObjectKind::Player {
            self.world_visibility_mut(object)?.override_bits = distance.bits();
        }
        Ok(())
    }
    pub fn viewpoint_cell(&self, object: ObjectHandle) -> Result<Option<CellIdentity>> {
        Ok(self.world_visibility(object)?.viewpoint_cell)
    }
    /// ViewPoint::Attach appends; duplicates are not deduplicated, Camera.h:89.
    pub fn attach_viewpoint_camera(
        &mut self,
        object: ObjectHandle,
        camera: ObjectHandle,
    ) -> Result<()> {
        require(
            self.get(camera)?.kind == SourceObjectKind::Camera,
            "execution.visibility.camera_type",
        )?;
        self.world_visibility_mut(object)?.cameras.push(camera);
        Ok(())
    }
    pub fn viewpoint_added_to_world(
        &mut self,
        object: ObjectHandle,
        cell: CellIdentity,
        cells: &mut SourceCells,
        mut camera_added: impl FnMut(&mut Self, &mut SourceCells, ObjectHandle) -> Result<()>,
    ) -> Result<()> {
        cells.validate_cell(cell)?;
        self.world_visibility_mut(object)?.viewpoint_cell = Some(cell);
        let mut next = (!self.world_visibility(object)?.cameras.is_empty()).then_some(0);
        while let Some(index) = next {
            let cameras = &self.world_visibility(object)?.cameras;
            let camera = cameras[index];
            // Camera.h:98 saves the following iterator before invoking the handler.
            // Appending after that saved iterator reaches end must not extend this call.
            next = (index + 1 < cameras.len()).then_some(index + 1);
            camera_added(self, cells, camera)?;
        }
        Ok(())
    }
    pub fn update_object_visibility(
        &mut self,
        request: SourceVisibilityUpdate,
        cells: &mut SourceCells,
        census: &mut SourceVisibilityCensus,
        mut camera_update: impl FnMut(
            &mut Self,
            &mut SourceCells,
            ObjectHandle,
            ObjectHandle,
        ) -> Result<SourceGuid>,
        mut residual_update: impl FnMut(
            &mut Self,
            &mut SourceCells,
            ObjectHandle,
            ObjectHandle,
        ) -> Result<()>,
    ) -> Result<()> {
        require(
            self.get(request.object)?.map == Some(cells.map()),
            "execution.visibility.map_binding",
        )?;
        let visibility = self.world_visibility(request.object)?;
        // VisibleChangesNotifier copies this set before traversing, GridNotifiers.h:52.
        let mut unvisited = visibility.client_guids.clone();
        let pose = self.pose(request.object)?.0;
        let standing = compute_cell_pair(pose[0], pose[1])
            .map_err(|_| refuse("execution.cell_coordinates"))?;
        let distance = if f32::from_bits(visibility.override_bits) != 0.0 {
            visibility.override_bits
        } else {
            request.map_distance_bits
        };
        let pairs = source_cell_visit(
            standing,
            pose[0],
            pose[1],
            distance,
            request.bounding_radius_bits,
        )
        .map_err(|_| refuse("execution.cell_visit_numeric"))?;
        for pair in pairs {
            census.considered_cells += 1;
            // Map.cpp:1337 and :660-663 never create or load a grid here.
            if !cells.object_data_loaded(pair.grid_x(), pair.grid_y()) {
                continue;
            }
            let cell = cells.cell(pair)?;
            census.loaded_cells += 1;
            // GridNotifiers.h:53 does not iterate Player/Pet/Corpse lists at all.
            let mut next = cells.first_in_slot(cell, 7)?;
            while let Some(camera) = next {
                census.cameras += 1;
                let player = camera_update(self, cells, camera, request.object)?;
                unvisited.remove(&player);
                // The source range-for increments after UpdateVisibilityOf, GridNotifiers.cpp:34-37.
                // Reuse the live iterator guard if a callback invalidated the current reference.
                next = cells.next_in_slot(camera, cell, 7)?;
            }
        }
        census.unvisited_guids = unvisited.len();
        for guid in unvisited {
            if let Some(player) = self.lookup(cells.map(), LiveLookupKind::Player, guid) {
                census.residual_players += 1;
                residual_update(self, cells, player, request.object)?;
            }
        }
        Ok(())
    }
    pub fn map_add_visibility(
        &mut self,
        request: SourceVisibilityUpdate,
        cells: &mut SourceCells,
        census: &mut SourceVisibilityCensus,
        camera_update: impl FnMut(
            &mut Self,
            &mut SourceCells,
            ObjectHandle,
            ObjectHandle,
        ) -> Result<SourceGuid>,
        residual_update: impl FnMut(
            &mut Self,
            &mut SourceCells,
            ObjectHandle,
            ObjectHandle,
        ) -> Result<()>,
    ) -> Result<()> {
        self.get_mut(request.object)?.is_new_object = true;
        self.update_object_visibility(request, cells, census, camera_update, residual_update)?;
        self.get_mut(request.object)?.is_new_object = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_sim::resident_map::CellPair;
    fn fixture() -> (SourceObjects, SourceCells, ObjectHandle, CellIdentity) {
        let map = MapIdentity {
            map: 0,
            instance: 0,
            creation_generation: 1,
        };
        let mut objects = SourceObjects::default();
        let mut cells = SourceCells::new(map).unwrap();
        let pair = CellPair::from_indices(512, 512).unwrap();
        cells.configure_grids(300000, true).unwrap();
        cells.grids.create(pair.grid_x(), pair.grid_y()).unwrap();
        let cell = cells.cell(pair).unwrap();
        let go = objects.construct(1, SourceObjectKind::GameObject).unwrap();
        objects.bind_map(go, map, &cells).unwrap();
        objects.set_pose(go, SourcePose([0; 4])).unwrap();
        (objects, cells, go, cell)
    }
    fn request(object: ObjectHandle) -> SourceVisibilityUpdate {
        SourceVisibilityUpdate {
            object,
            map_distance_bits: 100f32.to_bits(),
            bounding_radius_bits: 0x3ec7_2b02,
        }
    }
    #[test]
    fn viewpoint_registration_precedes_dispatch_and_keeps_the_preincremented_append_boundary() {
        let (mut objects, mut cells, go, cell) = fixture();
        let camera = objects.construct(1, SourceObjectKind::Camera).unwrap();
        let extra = objects.construct(1, SourceObjectKind::Camera).unwrap();
        assert_eq!(objects.viewpoint_cell(go).unwrap(), None);
        objects.attach_viewpoint_camera(go, camera).unwrap();
        objects.attach_viewpoint_camera(go, camera).unwrap();
        let mut visits = Vec::new();
        objects
            .viewpoint_added_to_world(go, cell, &mut cells, |objects, _, c| {
                visits.push(c);
                if visits.len() == 1 || visits.len() == 3 {
                    objects.attach_viewpoint_camera(go, extra)?;
                }
                Ok(())
            })
            .unwrap();
        assert_eq!(visits, [camera, camera, extra]);
        assert_eq!(objects.viewpoint_cell(go).unwrap(), Some(cell));
        let failure = objects
            .viewpoint_added_to_world(go, cell, &mut cells, |_, _, _| {
                Err(refuse("authored.camera_added"))
            })
            .unwrap_err();
        assert_eq!(failure.id, "authored.camera_added");
        assert_eq!(objects.viewpoint_cell(go).unwrap(), Some(cell));
    }
    #[test]
    fn visibility_no_create_traverses_real_cells_but_not_non_camera_type_lists() {
        let (mut objects, mut cells, go, cell) = fixture();
        let mut census = SourceVisibilityCensus::default();
        objects
            .map_add_visibility(
                request(go),
                &mut cells,
                &mut census,
                |_, _, _, _| panic!("no cameras"),
                |_, _, _, _| panic!("no stale recipients"),
            )
            .unwrap();
        assert!(census.considered_cells > 1);
        assert_eq!(census.loaded_cells, 0);
        assert!(!objects.get(go).unwrap().is_new_object);
        cells.set_object_data_loaded(cell.grid).unwrap();
        let player = objects.construct(1, SourceObjectKind::Player).unwrap();
        objects.bind_map(player, cells.map(), &cells).unwrap();
        objects.link_cell(player, cell, &mut cells).unwrap();
        let mut census = SourceVisibilityCensus::default();
        objects
            .map_add_visibility(
                request(go),
                &mut cells,
                &mut census,
                |_, _, _, _| panic!("players are not CameraMapType"),
                |_, _, _, _| panic!("empty recipient set"),
            )
            .unwrap();
        assert!(census.loaded_cells > 0);
        assert_eq!(census.cameras, 0);
    }
    #[test]
    fn visibility_camera_update_precedes_erase_and_residuals_visit_live_map_players() {
        let (mut objects, mut cells, go, cell) = fixture();
        cells.set_object_data_loaded(cell.grid).unwrap();
        let camera = objects.construct(1, SourceObjectKind::Camera).unwrap();
        cells.link(camera, SourceObjectKind::Camera, cell).unwrap();
        let guid = SourceGuid::without_entry(HighGuid::Player, 7);
        let stale = SourceGuid::without_entry(HighGuid::Player, 8);
        objects
            .world_visibility_mut(go)
            .unwrap()
            .client_guids
            .extend([guid, stale]);
        let mut census = SourceVisibilityCensus::default();
        objects
            .map_add_visibility(
                request(go),
                &mut cells,
                &mut census,
                |_, _, c, o| {
                    assert_eq!((c, o), (camera, go));
                    Ok(guid)
                },
                |_, _, _, _| panic!("stale player not live"),
            )
            .unwrap();
        assert_eq!(
            (
                census.cameras,
                census.unvisited_guids,
                census.residual_players
            ),
            (1, 1, 0)
        );
        assert_eq!(objects.world_visibility(go).unwrap().client_guids.len(), 2);
        let player = objects.construct(1, SourceObjectKind::Player).unwrap();
        objects.bind_map(player, cells.map(), &cells).unwrap();
        objects.assign_guid(player, 0, stale).unwrap();
        objects.register_player_lookup(player).unwrap();
        objects.link_map_player(player).unwrap();
        objects.add_to_world(player).unwrap();
        let mut census = SourceVisibilityCensus::default();
        let mut residuals = Vec::new();
        objects
            .map_add_visibility(
                request(go),
                &mut cells,
                &mut census,
                |_, _, _, _| Ok(guid),
                |_, _, p, o| {
                    residuals.push((p, o));
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(residuals, [(player, go)]);
        assert_eq!(census.residual_players, 1);
        let failure = objects
            .map_add_visibility(
                request(go),
                &mut cells,
                &mut SourceVisibilityCensus::default(),
                |_, _, _, _| {
                    Err(refuse(
                        "execution.visibility.GridNotifiers_36.camera_update_owner",
                    ))
                },
                |_, _, _, _| panic!("camera refused first"),
            )
            .unwrap_err();
        assert_eq!(
            failure.id,
            "execution.visibility.GridNotifiers_36.camera_update_owner"
        );
        assert!(objects.get(go).unwrap().is_new_object);
    }
    #[test]
    fn visibility_camera_reference_is_read_after_callback_and_refuses_invalidation() {
        let (mut objects, mut cells, go, cell) = fixture();
        cells.set_object_data_loaded(cell.grid).unwrap();
        let camera = objects.construct(1, SourceObjectKind::Camera).unwrap();
        cells.link(camera, SourceObjectKind::Camera, cell).unwrap();
        let guid = SourceGuid::without_entry(HighGuid::Player, 7);
        objects
            .map_add_visibility(
                request(go),
                &mut cells,
                &mut SourceVisibilityCensus::default(),
                |_, _, _, _| Ok(guid),
                |_, _, _, _| Ok(()),
            )
            .unwrap();
        let failure = objects
            .map_add_visibility(
                request(go),
                &mut cells,
                &mut SourceVisibilityCensus::default(),
                |_, cells, c, _| {
                    cells.unlink(c);
                    Ok(guid)
                },
                |_, _, _, _| Ok(()),
            )
            .unwrap_err();
        assert_eq!(failure.id, "execution.residency.invalidated_cell_iterator");
        assert!(objects.get(go).unwrap().is_new_object);
    }
    #[test]
    fn visibility_override_uses_last_write_and_players_ignore_it() {
        let (mut objects, _, go, _) = fixture();
        assert_eq!(objects.visibility_override_bits(go).unwrap(), 0);
        for distance in [
            SourceVisibilityDistance::Large,
            SourceVisibilityDistance::Gigantic,
            SourceVisibilityDistance::Infinite,
        ] {
            objects
                .set_visibility_distance_override(go, distance)
                .unwrap();
            assert_eq!(
                objects.visibility_override_bits(go).unwrap(),
                distance.bits()
            );
        }
        let player = objects.construct(1, SourceObjectKind::Player).unwrap();
        objects
            .set_visibility_distance_override(player, SourceVisibilityDistance::Infinite)
            .unwrap();
        assert_eq!(objects.visibility_override_bits(player).unwrap(), 0);
    }
}
