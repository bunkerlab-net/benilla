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
#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SourceBroadcastNotice {
    pub line: u32,
    pub object: ObjectHandle,
    pub position: [u32; 2],
}
#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SourceBroadcast {
    OutOfWorld,
    InvalidCoordinates(SourceBroadcastNotice),
    GridUnloaded,
    Visited { loaded_cells: usize },
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
    /// Object.h:1142 returns the live per-object set.
    pub fn client_guids_i_am_at(&self, object: ObjectHandle) -> Result<&BTreeSet<SourceGuid>> {
        Ok(&self.world_visibility(object)?.client_guids)
    }
    /// Unit.cpp:11212-11228. Player lookup uses the live map registry.
    pub fn send_message_to_all_who_see_me_move(
        &self,
        packet: &SplineOutbound,
        mut send_session: impl FnMut(ObjectHandle, &SplineOutbound) -> Result<()>,
    ) -> Result<()> {
        let object = self.get(packet.unit)?;
        if !object.in_world {
            return Ok(());
        }
        let mover_owner = packet.excluded.unwrap_or_default();
        for &guid in self.client_guids_i_am_at(packet.unit)? {
            if mover_owner == guid {
                continue;
            }
            let map = object
                .map
                .ok_or_else(|| refuse("execution.spline.GetMap_owner_11221"))?;
            if let Some(player) = self.lookup(map, LiveLookupKind::Player, guid) {
                send_session(player, packet)?;
            }
        }
        if object.kind == SourceObjectKind::Player {
            let guid = object
                .guid
                .ok_or_else(|| refuse("execution.spline.player_GetObjectGuid_owner_11225"))?;
            if mover_owner != guid {
                send_session(packet.unit, packet)?;
            }
        }
        Ok(())
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
    /// Object.cpp:1916-1921, Map.cpp:528-549. Visit only existing world cells.
    #[cfg(test)]
    pub(crate) fn message_broadcast_without_sessions(
        &self,
        object: ObjectHandle,
        cells: &SourceCells,
        map_distance_bits: u32,
        radius: Option<u32>,
    ) -> Result<SourceBroadcast> {
        let world = self.get(object)?;
        require(
            world.map == Some(cells.map()),
            "execution.spells.Map_541.broadcast_grid_owner",
        )?;
        let [x, y, _, _] = self.pose(object)?.0;
        use benilla_sim::resident_map::Error;
        let undefined = || refuse("execution.spells.GridDefines_154.cell_float_to_int_undefined");
        let standing = match compute_cell_pair(x, y) {
            Ok(pair) => pair,
            Err(Error::OutsideMap) => {
                // The source computes both axes before Map.cpp:532 tests either.
                if compute_cell_pair(0, y) == Err(Error::Numeric) {
                    return Err(undefined());
                }
                return Ok(SourceBroadcast::InvalidCoordinates(SourceBroadcastNotice {
                    line: 534,
                    object,
                    position: [x, y],
                }));
            }
            Err(Error::Numeric) => return Err(undefined()),
        };
        if !cells.object_data_loaded(standing.grid_x(), standing.grid_y()) {
            return Ok(SourceBroadcast::GridUnloaded);
        }
        let override_bits = self.visibility_override_bits(object)?;
        let distance = if f32::from_bits(override_bits) != 0. {
            override_bits
        } else {
            map_distance_bits
        };
        let radius = radius
            .ok_or_else(|| refuse("execution.spells.Unit_1094.UNIT_FIELD_BOUNDINGRADIUS_owner"))?;
        let pairs = source_cell_visit(standing, x, y, distance, radius)
            .map_err(|_| refuse("execution.spells.Cell_80.broadcast_cell_visit"))?;
        let mut loaded_cells = 0;
        for pair in pairs {
            if cells.object_data_loaded(pair.grid_x(), pair.grid_y()) {
                loaded_cells += 1;
                require(
                    cells.first_in_slot(cells.cell(pair)?, 7)?.is_none(),
                    "execution.spells.GridNotifiers_152.camera_session_owner",
                )?;
            }
        }
        Ok(SourceBroadcast::Visited { loaded_cells })
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
    fn broadcast_checks_coordinates_then_grid_then_camera_sessions() {
        let (mut objects, mut cells, object, cell) = fixture();
        let distance = 100f32.to_bits();
        let outside = 20_000f32.to_bits();
        objects
            .set_pose(object, SourcePose([outside, 0, 0, 0]))
            .unwrap();
        assert_eq!(
            objects
                .message_broadcast_without_sessions(object, &cells, distance, None)
                .unwrap(),
            SourceBroadcast::InvalidCoordinates(SourceBroadcastNotice {
                line: 534,
                object,
                position: [outside, 0]
            }),
        );
        for point in [[f32::MAX.to_bits(), 0], [outside, f32::MAX.to_bits()]] {
            objects
                .set_pose(object, SourcePose([point[0], point[1], 0, 0]))
                .unwrap();
            assert_eq!(
                objects
                    .message_broadcast_without_sessions(object, &cells, distance, None)
                    .unwrap_err()
                    .id,
                "execution.spells.GridDefines_154.cell_float_to_int_undefined",
            );
        }
        objects.set_pose(object, SourcePose([0; 4])).unwrap();
        let camera = objects.construct(1, SourceObjectKind::Camera).unwrap();
        let near = cells
            .cell(CellPair::from_indices(514, 512).unwrap())
            .unwrap();
        cells.link(camera, SourceObjectKind::Camera, near).unwrap();
        // A grid can exist and contain a registration before its object data is loaded.
        assert_eq!(
            objects
                .message_broadcast_without_sessions(object, &cells, distance, None)
                .unwrap(),
            SourceBroadcast::GridUnloaded
        );
        cells.set_object_data_loaded(cell.grid).unwrap();
        assert_eq!(
            objects
                .message_broadcast_without_sessions(object, &cells, distance, Some(0))
                .unwrap_err()
                .id,
            "execution.spells.GridNotifiers_152.camera_session_owner",
        );
        objects
            .set_visibility_distance_override(object, SourceVisibilityDistance::Tiny)
            .unwrap();
        assert!(matches!(
            objects
                .message_broadcast_without_sessions(object, &cells, distance, Some(0))
                .unwrap(),
            SourceBroadcast::Visited { loaded_cells } if loaded_cells > 0
        ));
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
    #[test]
    fn authored_move_broadcast_reads_live_set_and_preserves_source_branches() {
        let (mut objects, cells, unit, _) = fixture();
        objects.get_mut(unit).unwrap().kind = SourceObjectKind::Creature;
        let guid = SourceGuid::with_entry(HighGuid::Creature, 1, 1);
        objects.assign_guid(unit, 1, guid).unwrap();
        let mut packet = SplineOutbound {
            unit,
            guid,
            transport: None,
            excluded: None,
            movement: vec![1, 2, 3],
        };
        // Authored players, not stock startup evidence.
        let mut players = Vec::new();
        for low in [7, 8] {
            let player = objects.construct(1, SourceObjectKind::Player).unwrap();
            let guid = SourceGuid::without_entry(HighGuid::Player, low);
            objects.bind_map(player, cells.map(), &cells).unwrap();
            objects.assign_guid(player, 0, guid).unwrap();
            objects.register_player_lookup(player).unwrap();
            objects.link_map_player(player).unwrap();
            objects.add_to_world(player).unwrap();
            players.push((player, guid));
        }
        objects
            .world_visibility_mut(unit)
            .unwrap()
            .client_guids
            .extend([
                SourceGuid::default(),
                players[0].1,
                players[1].1,
                SourceGuid::without_entry(HighGuid::Player, 9),
            ]);
        // Unit.cpp:11214 returns before even reading visibility.
        objects
            .send_message_to_all_who_see_me_move(&packet, |_, _| panic!("out of world"))
            .unwrap();
        objects.add_to_world(unit).unwrap();
        let mut sent = Vec::new();
        objects
            .send_message_to_all_who_see_me_move(&packet, |p, data| {
                assert_eq!(data, &packet);
                sent.push(p);
                Ok(())
            })
            .unwrap();
        assert_eq!(sent, [players[0].0, players[1].0]);
        packet.excluded = Some(players[0].1);
        sent.clear();
        objects
            .send_message_to_all_who_see_me_move(&packet, |p, _| {
                sent.push(p);
                Ok(())
            })
            .unwrap();
        assert_eq!(sent, [players[1].0]);
        objects
            .world_visibility_mut(unit)
            .unwrap()
            .client_guids
            .clear();
        objects
            .send_message_to_all_who_see_me_move(&packet, |_, _| panic!("empty set"))
            .unwrap();
        // Unit.cpp:11225-11226 sends a player mover its own copy unless excluded.
        packet.unit = players[0].0;
        packet.guid = players[0].1;
        objects
            .send_message_to_all_who_see_me_move(&packet, |_, _| panic!("excluded self"))
            .unwrap();
        packet.excluded = None;
        sent.clear();
        objects
            .send_message_to_all_who_see_me_move(&packet, |p, _| {
                sent.push(p);
                Ok(())
            })
            .unwrap();
        assert_eq!(sent, [players[0].0]);
    }

    #[test]
    fn authored_move_broadcast_refuses_unowned_map_and_session_at_the_read() {
        let (mut objects, cells, unit, _) = fixture();
        objects.get_mut(unit).unwrap().kind = SourceObjectKind::Creature;
        let guid = SourceGuid::without_entry(HighGuid::Player, 7);
        objects
            .world_visibility_mut(unit)
            .unwrap()
            .client_guids
            .insert(guid);
        objects.get_mut(unit).unwrap().in_world = true;
        let packet = SplineOutbound {
            unit,
            guid,
            transport: None,
            excluded: None,
            movement: vec![],
        };
        objects.get_mut(unit).unwrap().map = None;
        let skipped = SplineOutbound {
            excluded: Some(guid),
            ..packet.clone()
        };
        objects
            .send_message_to_all_who_see_me_move(&skipped, |_, _| {
                panic!("owner skip precedes map read")
            })
            .unwrap();
        assert_eq!(
            objects
                .send_message_to_all_who_see_me_move(&packet, |_, _| panic!())
                .unwrap_err()
                .id,
            "execution.spline.GetMap_owner_11221"
        );
        objects.get_mut(unit).unwrap().map = Some(cells.map());
        let player = objects.construct(1, SourceObjectKind::Player).unwrap();
        objects.bind_map(player, cells.map(), &cells).unwrap();
        objects.assign_guid(player, 0, guid).unwrap();
        objects.register_player_lookup(player).unwrap();
        objects.add_to_world(player).unwrap();
        struct MissingSession;
        impl MotionExecutionOwner for MissingSession {}
        let mut units = SourceUnits::default();
        let context = MotionContext {
            objects: &objects,
            units: &mut units,
        };
        assert_eq!(
            MissingSession
                .send_spline(&packet, &context)
                .unwrap_err()
                .id,
            "execution.spline.player_GetSession_SendPacket_owner_11222_11226"
        );
        let self_packet = SplineOutbound {
            unit: player,
            ..packet
        };
        assert_eq!(
            MissingSession
                .send_spline(&self_packet, &context)
                .unwrap_err()
                .id,
            "execution.spline.player_GetSession_SendPacket_owner_11222_11226"
        );
    }

    #[test]
    fn authored_out_of_world_move_does_not_read_unowned_visibility() {
        let (mut objects, _, unit, _) = fixture();
        objects.get_mut(unit).unwrap().visibility = None;
        let packet = SplineOutbound {
            unit,
            guid: SourceGuid::with_entry(HighGuid::Creature, 1, 1),
            transport: None,
            excluded: None,
            movement: vec![],
        };
        objects
            .send_message_to_all_who_see_me_move(&packet, |_, _| panic!())
            .unwrap();
        objects.get_mut(unit).unwrap().in_world = true;
        assert_eq!(
            objects
                .send_message_to_all_who_see_me_move(&packet, |_, _| panic!())
                .unwrap_err()
                .id,
            "execution.visibility.world_object_type"
        );
    }
}
