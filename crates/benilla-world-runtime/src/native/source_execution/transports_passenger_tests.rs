// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored executable callbacks test source chronology, not implemented F5/F6 effects.
use super::*;
#[derive(Default)]
struct Owner {
    model: AuthoredOwner,
    effects: Vec<PassengerEffect>,
    fail: bool,
    destination: Option<SourceCells>,
    queue: Option<SourceMessager>,
}
impl MoTransportOwner for Owner {
    fn transport_template(&mut self, e: u32) -> Result<Option<GameObjectTemplate>> {
        self.model.transport_template(e)
    }
    fn transport_allocation(&mut self, h: ObjectHandle, o: &mut SourceObjects) -> Result<()> {
        self.model.transport_allocation(h, o)
    }
    fn transport_model(
        &mut self,
        t: &SourceMoTransport,
        op: GameObjectModelOperation,
    ) -> Result<bool> {
        self.model.transport_model(t, op)
    }
    fn passenger_effect(&mut self, e: PassengerEffect, o: &mut SourceObjects) -> Result<()> {
        self.effects.push(e);
        if self.fail {
            return Err(e.missing_owner());
        }
        match e {
            PassengerEffect::Resurrect { player, health } => {
                assert_eq!(health, 1_f32.to_bits());
                o.get_mut(player)?.alive = true;
            }
            PassengerEffect::PetNearTeleport { pet, pose } => o.set_pose(pet, pose)?,
            PassengerEffect::CreatureRelocate { creature, pose } => o.set_pose(creature, pose)?,
            PassengerEffect::PlayerRelocate { player, pose } => o.set_pose(player, pose)?,
            _ => {}
        }
        Ok(())
    }
    fn transport_destination(&mut self, id: u32, _: ObjectHandle) -> Result<MapIdentity> {
        assert!(self.destination.is_none());
        self.destination = Some(SourceCells::new(map(id))?);
        self.queue = Some(SourceMessager::new(MessageOwner::Map(map(id))));
        Ok(map(id))
    }
    fn transport_message(
        &mut self,
        destination: MapIdentity,
        transfer: TransportTransfer,
    ) -> Result<()> {
        assert_eq!(self.destination.as_ref().unwrap().map(), destination);
        self.queue.as_mut().unwrap().enqueue(MessageEnvelope {
            id: 1,
            cause: 42,
            payload: SourceMessage::AttachTransport { transfer },
        })
    }
}
fn setup() -> (
    SourceTransportTemplates,
    SourceMoTransports,
    SourceCells,
    SourceObjects,
    Owner,
    ObjectHandle,
) {
    let t = templates(&[(42, 0)]);
    let mut m = SourceMoTransports::default();
    let mut c = SourceCells::new(map(0)).unwrap();
    let mut o = SourceObjects::default();
    let mut w = Owner::default();
    initialize(&t, &mut m, &mut c, &mut o, &mut w, 0);
    let h = m.members(map(0)).next().unwrap();
    w.model.operations.clear();
    (t, m, c, o, w, h)
}
fn unit(
    m: &mut SourceMoTransports,
    o: &mut SourceObjects,
    c: &SourceCells,
    kind: SourceObjectKind,
    pointer: u64,
) -> ObjectHandle {
    let h = o.construct(1, kind).unwrap();
    o.bind_source_allocation(h, pointer, 64, 0).unwrap();
    o.bind_map(h, c.map(), c).unwrap();
    o.set_pose(
        h,
        SourcePose([11_f32.to_bits(), 2_f32.to_bits(), 3_f32.to_bits(), 0]),
    )
    .unwrap();
    m.passengers
        .bind_unit(
            h,
            PassengerMovement {
                transport: None,
                guid: 0,
                time: 7,
                offset: SourcePose([1, 2, 3, 4]),
                on_transport: false,
                movement_pose: SourcePose([4_f32.to_bits(), 5_f32.to_bits(), 6_f32.to_bits(), 0]),
                ghost: false,
                pet: None,
                mini_pet: None,
            },
            o,
        )
        .unwrap();
    h
}
#[test]
fn duplicate_boarding_does_not_touch_changed_unit_or_recurse() {
    // Transports.cpp:273-297 skips all mutations on duplicate membership.
    let (_, mut m, c, mut o, mut w, h) = setup();
    let p = unit(&mut m, &mut o, &c, SourceObjectKind::Player, 0x1000);
    m.attach_passenger(h, p, false, &mut o, &mut w).unwrap();
    m.passengers.movement_mut(p).unwrap().time = 999;
    m.attach_passenger(h, p, true, &mut o, &mut w).unwrap();
    assert_eq!(m.passengers.movement(p).unwrap().time, 999);
    assert_eq!(m.passengers.members(h).count(), 1);
    assert!(w.effects.is_empty());
}
#[test]
fn adjust_false_and_unchanged_guid_keep_offset_but_set_attachment_and_progress() {
    // Transports.cpp:277-288 only adjusts coordinates when both predicates hold.
    for same in [false, true] {
        let (_, mut m, c, mut o, mut w, h) = setup();
        let p = unit(&mut m, &mut o, &c, SourceObjectKind::Player, 0x1000);
        let guid = m.passenger_transport(h, &o).unwrap().guid;
        if same {
            m.passengers.movement_mut(p).unwrap().guid = guid;
        }
        m.attach_passenger(h, p, same, &mut o, &mut w).unwrap();
        let s = m.passengers.movement(p).unwrap();
        assert_eq!(s.offset, SourcePose([1, 2, 3, 4]));
        assert_eq!(s.transport, Some(h));
        assert_eq!(s.guid, guid);
        assert_eq!(s.time, 0);
        assert!(s.on_transport);
    }
}
#[test]
fn changed_attachment_uses_world_pose_and_copies_pet_offset_before_near_teleport() {
    // Transports.cpp:279-295,338-343 overwrites pet offsets even after AddPassenger.
    let (_, mut m, c, mut o, mut w, h) = setup();
    let p = unit(&mut m, &mut o, &c, SourceObjectKind::Player, 0x1000);
    let pet = unit(&mut m, &mut o, &c, SourceObjectKind::Pet, 0x2000);
    m.passengers.movement_mut(p).unwrap().pet = Some(pet);
    m.attach_passenger(h, p, true, &mut o, &mut w).unwrap();
    assert_ne!(
        m.passengers.movement(p).unwrap().offset,
        SourcePose([1, 2, 3, 4])
    );
    assert_eq!(
        m.passengers.movement(pet).unwrap().offset,
        m.passengers.movement(p).unwrap().offset
    );
    assert_eq!(
        w.effects,
        [PassengerEffect::PetNearTeleport {
            pet,
            pose: m.passengers.movement(p).unwrap().movement_pose
        }]
    );
    m.detach_passenger(h, p, &mut o, &mut w).unwrap();
    let s = m.passengers.movement(p).unwrap();
    assert_eq!(s.transport, None);
    assert_eq!(s.guid, 0);
    assert_eq!(s.time, 0);
    assert_eq!(s.offset, SourcePose([0; 4]));
    assert!(s.on_transport);
}
#[test]
fn recursive_pet_removal_advances_erased_next_in_pointer_order() {
    // Transports.cpp:214-216,303-311 defines erased-next handling before pet recursion.
    let (_, mut m, c, mut o, mut w, h) = setup();
    let last = unit(&mut m, &mut o, &c, SourceObjectKind::Player, 0x3000);
    let parent = unit(&mut m, &mut o, &c, SourceObjectKind::Creature, 0x1000);
    let pet = unit(&mut m, &mut o, &c, SourceObjectKind::Pet, 0x2000);
    m.passengers.movement_mut(parent).unwrap().pet = Some(pet);
    m.attach_passenger(h, last, false, &mut o, &mut w).unwrap();
    m.attach_passenger(h, parent, true, &mut o, &mut w).unwrap();
    assert_eq!(
        m.passengers.members(h).collect::<Vec<_>>(),
        [parent, pet, last]
    );
    w.effects.clear();
    m.teleport(h, 1, SourcePose([0; 4]), &mut o, &mut w)
        .unwrap();
    assert_eq!(m.passengers.members(h).collect::<Vec<_>>(), [last]);
    assert_eq!(w.effects.len(), 3);
    assert!(matches!(w.effects[0],PassengerEffect::PetNearTeleport { pet:p,.. } if p==pet));
    assert_eq!(
        w.effects[1],
        PassengerEffect::CreatureRemove { creature: parent }
    );
    assert!(matches!(w.effects[2],PassengerEffect::PlayerTeleport { player:p,.. } if p==last));
}
#[test]
fn non_pet_cleanup_and_pet_exclusion_are_cross_map_only() {
    // Transports.cpp:227-234 removes units only on map change and queues only non-pets.
    for destination in [0, 1] {
        let (_, mut m, c, mut o, mut w, h) = setup();
        let creature = unit(&mut m, &mut o, &c, SourceObjectKind::Creature, 0x1000);
        let pet = unit(&mut m, &mut o, &c, SourceObjectKind::Pet, 0x2000);
        for p in [creature, pet] {
            m.attach_passenger(h, p, false, &mut o, &mut w).unwrap();
        }
        m.teleport(h, destination, SourcePose([0; 4]), &mut o, &mut w)
            .unwrap();
        assert_eq!(
            m.passengers.members(h).count(),
            if destination == 0 { 2 } else { 0 }
        );
        assert_eq!(
            w.effects,
            if destination == 0 {
                vec![]
            } else {
                vec![PassengerEffect::CreatureRemove { creature }]
            }
        );
    }
}
#[test]
fn nonghost_dead_player_resurrects_before_teleport_but_ghost_does_not() {
    // Transports.cpp:240-242 resurrects at full health only for nonghost dead players.
    for (alive, ghost) in [(false, false), (false, true), (true, false)] {
        let (_, mut m, c, mut o, mut w, h) = setup();
        let p = unit(&mut m, &mut o, &c, SourceObjectKind::Player, 0x1000);
        o.get_mut(p).unwrap().alive = alive;
        m.passengers.movement_mut(p).unwrap().ghost = ghost;
        m.attach_passenger(h, p, false, &mut o, &mut w).unwrap();
        m.teleport(h, 0, SourcePose([0; 4]), &mut o, &mut w)
            .unwrap();
        assert_eq!(w.effects.len(), if !alive && !ghost { 2 } else { 1 });
        if !alive && !ghost {
            assert!(matches!(w.effects[0], PassengerEffect::Resurrect { .. }));
            assert!(o.get(p).unwrap().alive);
        }
        assert!(matches!(
            w.effects.last().unwrap(),
            PassengerEffect::PlayerTeleport { map: 0, .. }
        ));
    }
}
#[test]
fn same_map_teleport_retains_orientation_and_does_not_update_passenger_positions() {
    // Transports.cpp:209,267-268 retains old o and only updates the model on same-map teleport.
    let (_, mut m, c, mut o, mut w, h) = setup();
    let p = unit(&mut m, &mut o, &c, SourceObjectKind::Creature, 0x1000);
    m.attach_passenger(h, p, false, &mut o, &mut w).unwrap();
    let before = o.pose(p).unwrap();
    let orientation = o.pose(h).unwrap().0[3];
    m.teleport(
        h,
        0,
        SourcePose([1_f32.to_bits(), 2_f32.to_bits(), 3_f32.to_bits(), 0]),
        &mut o,
        &mut w,
    )
    .unwrap();
    assert_eq!(o.pose(h).unwrap().0[3], orientation);
    assert_eq!(o.pose(p).unwrap(), before);
    assert_eq!(o.get(h).unwrap().map, Some(map(0)));
    assert!(m.pending_transfer(h).is_none());
    assert_eq!(
        w.model.operations.iter().map(|r| r.1).collect::<Vec<_>>(),
        [
            GameObjectModelOperation::Contains,
            GameObjectModelOperation::Remove,
            GameObjectModelOperation::Relocate,
            GameObjectModelOperation::Insert
        ]
    );
}
#[test]
fn cross_map_creates_destination_and_enqueues_with_no_current_map() {
    // Transports.cpp:253-265 resets map before creation and attaches only on message delivery.
    let (_, mut m, _, mut o, mut w, h) = setup();
    m.teleport(h, 1, SourcePose([0; 4]), &mut o, &mut w)
        .unwrap();
    assert!(o.get(h).unwrap().map.is_none());
    assert_eq!(m.members(map(0)).count(), 0);
    assert_eq!(m.members(map(1)).count(), 0);
    assert_eq!(m.pending_transfer(h), Some(map(1)));
    assert_eq!(w.queue.as_ref().unwrap().pending(), 1);
    assert!(w.destination.is_some());
    assert_eq!(
        w.model.operations.iter().map(|r| r.1).collect::<Vec<_>>(),
        [
            GameObjectModelOperation::Contains,
            GameObjectModelOperation::Remove
        ]
    );
    assert_eq!(
        m.teleport(h, 1, SourcePose([0; 4]), &mut o, &mut w)
            .unwrap_err()
            .id,
        "execution.transport.current_map_owner"
    );
}
struct NoOther;
impl MessageWork for NoOther {
    fn begin_message(&mut self, _: &MessageEnvelope) -> Result<()> {
        Ok(())
    }
    fn forward_game_event(&mut self, _: u64, _: &[EventMapGuids], _: bool) -> Result<()> {
        panic!("unexpected event")
    }
    fn map_game_event(
        &mut self,
        _: u64,
        _: MapIdentity,
        _: &[u32],
        _: &[u32],
        _: bool,
    ) -> Result<()> {
        panic!("unexpected event")
    }
    fn creature_present(&self, _: MapIdentity, _: u64) -> bool {
        panic!("unexpected event")
    }
    fn update_creature_event(
        &mut self,
        _: u64,
        _: MapIdentity,
        _: EventCreatureUpdate,
    ) -> Result<()> {
        panic!("unexpected event")
    }
}
#[test]
fn destination_message_attaches_before_model_insertion_and_visibility() {
    // Transports.cpp:259-265 uses the actual detached Messager batch and destination map generation.
    let (t, mut m, _, mut o, mut w, h) = setup();
    m.teleport(h, 1, SourcePose([0; 4]), &mut o, &mut w)
        .unwrap();
    let c = w.destination.take().unwrap();
    let mut q = w.queue.take().unwrap();
    w.model.contained = false;
    w.model.operations.clear();
    let p = unit(&mut m, &mut o, &c, SourceObjectKind::Player, 0x1000);
    o.link_map_player(p).unwrap();
    let done = q
        .execute(
            MessageOwner::Map(map(1)),
            2,
            &mut TransportMessageDispatch {
                transports: &mut m,
                cells: &c,
                objects: &mut o,
                owner: &mut w,
                other: &mut NoOther,
            },
        )
        .unwrap();
    assert_eq!(done[0].id, 1);
    assert_eq!(done[0].cause, 42);
    assert_eq!(done[0].outcome, MessageOutcome::Executed);
    assert_eq!(o.get(h).unwrap().map, Some(map(1)));
    assert_eq!(m.members(map(1)).collect::<Vec<_>>(), [h]);
    assert!(m.pending_transfer(h).is_none());
    assert_eq!(
        w.model.operations.iter().map(|r| r.1).collect::<Vec<_>>(),
        [
            GameObjectModelOperation::Contains,
            GameObjectModelOperation::Relocate,
            GameObjectModelOperation::Insert
        ]
    );
    assert_eq!(
        w.effects,
        [PassengerEffect::Visibility {
            transport: h,
            player: p,
            create: true
        }]
    );
    m.initialized.insert(map(1));
    m.update_map(&t, map(1), 0, 999, &mut o, &mut w).unwrap();
    assert_eq!(w.effects.len(), 1);
}
#[test]
fn old_map_visibility_builds_once_even_if_every_player_is_attached() {
    // Transports.cpp:612-617 builds before excluding attached players from sends.
    let (_, mut m, c, mut o, mut w, h) = setup();
    let p = unit(&mut m, &mut o, &c, SourceObjectKind::Player, 0x1000);
    o.link_map_player(p).unwrap();
    m.attach_passenger(h, p, false, &mut o, &mut w).unwrap();
    m.update_for_map(h, map(0), false, &mut o, &mut w).unwrap();
    assert_eq!(
        w.effects,
        [PassengerEffect::BuildOutOfRange { transport: h }]
    );
}
#[test]
fn refusal_occurs_after_prior_attachment_removal_and_transport_relocation() {
    // Transports.cpp:321-326,240-242 retains earlier observable effects at each unavailable F5 call.
    let (_, mut m, c, mut o, mut w, h) = setup();
    let p = unit(&mut m, &mut o, &c, SourceObjectKind::Player, 0x1000);
    let pet = unit(&mut m, &mut o, &c, SourceObjectKind::Pet, 0x2000);
    m.passengers.movement_mut(p).unwrap().pet = Some(pet);
    w.fail = true;
    assert_eq!(
        m.attach_passenger(h, p, true, &mut o, &mut w)
            .unwrap_err()
            .id,
        "execution.transport.f5_pet_near_teleport_owner"
    );
    assert_eq!(m.passengers.members(h).count(), 2);
    assert_eq!(
        m.passengers.movement(pet).unwrap().offset,
        m.passengers.movement(p).unwrap().offset
    );
    assert_eq!(
        m.detach_passenger(h, p, &mut o, &mut w).unwrap_err().id,
        "execution.transport.f5_pet_near_teleport_owner"
    );
    assert_eq!(m.passengers.members(h).count(), 0);
    m.passengers.movement_mut(p).unwrap().pet = None;
    m.attach_passenger(h, p, false, &mut o, &mut w).unwrap();
    o.get_mut(p).unwrap().alive = false;
    let pose = SourcePose([100_f32.to_bits(), 0, 0, 0]);
    assert_eq!(
        m.teleport(h, 1, pose, &mut o, &mut w).unwrap_err().id,
        "execution.transport.f5_player_resurrection_owner"
    );
    assert_eq!(o.pose(h).unwrap().0[0], pose.0[0]);
    assert_eq!(o.get(h).unwrap().map, Some(map(0)));
    assert!(w.destination.is_none());
}
#[test]
fn missing_player_teleport_refuses_before_old_geometry_removal() {
    // Transports.cpp:242 precedes RemoveModelFromMap at :253.
    let (_, mut m, c, mut o, mut w, h) = setup();
    let p = unit(&mut m, &mut o, &c, SourceObjectKind::Player, 0x1000);
    m.attach_passenger(h, p, false, &mut o, &mut w).unwrap();
    w.fail = true;
    assert_eq!(
        m.teleport(h, 1, SourcePose([0; 4]), &mut o, &mut w)
            .unwrap_err()
            .id,
        "execution.transport.f5_player_teleport_owner"
    );
    assert!(w.model.operations.is_empty());
    assert_eq!(m.members(map(0)).count(), 1);
}
#[test]
fn passenger_position_map_guard_and_out_of_world_relocation_preserve_timing() {
    // Transports.cpp:506-549 skips cross-map in-world players, but relocates those not in world.
    let (_, mut m, c, mut o, mut w, h) = setup();
    let p = unit(&mut m, &mut o, &c, SourceObjectKind::Player, 0x1000);
    m.attach_passenger(h, p, true, &mut o, &mut w).unwrap();
    let view = m.passenger_transport(h, &o).unwrap();
    o.get_mut(p).unwrap().map = Some(map(1));
    o.get_mut(p).unwrap().in_world = true;
    let before = o.pose(p).unwrap();
    m.passengers.update_positions(view, &mut o, &mut w).unwrap();
    assert_eq!(o.pose(p).unwrap(), before);
    assert!(w.effects.is_empty());
    o.get_mut(p).unwrap().in_world = false;
    let mut view = view;
    view.pose.0[0] = 100_f32.to_bits();
    view.progress = 91;
    m.passengers.update_positions(view, &mut o, &mut w).unwrap();
    assert_ne!(o.pose(p).unwrap(), before);
    assert_eq!(m.passengers.movement(p).unwrap().time, 91);
    assert_eq!(m.passengers.movement(p).unwrap().guid, view.guid);
}

#[test]
fn retained_unsealed_passenger_transfer_projection_or_clean_absence() {
    for (variable, expected) in [
        (
            "BENILLA_F1_SEAL8_FRAMING",
            "097b87217694b01c5a42c114358a7f30b2faf2062095640c0e50a4028e739cfa",
        ),
        (
            "BENILLA_F1_TRANSPORT_FRAMING",
            "cafc1fdd139046836d248f7799e72bcb7708d0fe645d451960945a62fe4214a7",
        ),
    ] {
        let Some(root) = std::env::var_os(variable) else {
            println!("SKIP unsealed passenger/transfer projection: {variable} absent");
            continue;
        };
        let (held, raw) = metadata(
            &std::path::PathBuf::from(root).join("source-sites.json"),
            1_048_576,
        )
        .unwrap();
        assert_eq!(Sha256Id::of(&raw).as_str(), expected);
        let sites: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        let direct = sites
            .as_object()
            .unwrap()
            .values()
            .filter(|s| {
                let f = s["callsite"]["function"].as_str().unwrap_or("");
                [
                    "GenericTransport::AddPassenger(",
                    "GenericTransport::RemovePassenger(",
                    "Transport::TeleportTransport(",
                    "Transport::UpdateForMap(",
                    "GenericTransport::UpdatePassengerPosition(",
                ]
                .iter()
                .any(|needle| f.contains(needle))
            })
            .count();
        held.check().unwrap();
        assert_eq!(direct, 0);
        println!("SKIP unsealed MT passenger/transfer comparison: {variable}, direct implementation sites={direct}, independently matched transfer/passenger records=0; retained MO constructors are not passenger or transfer observations");
    }
}
