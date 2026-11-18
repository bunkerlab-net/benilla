// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors.
use super::*;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::{Rc, Weak};

const FOLLOW: u32 = 0x0008_0000; // Unit.h:353.
const FOLLOW_MOVE: u32 = 0x0010_0000; // Unit.h:354.

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct FollowerReferenceId {
    pub unit: ObjectHandle,
    pub ordinal: u64,
}

type FollowerList = RefCell<VecDeque<(FollowerReferenceId, Weak<SourceFollowGenerator>)>>;

/// Unit.h:2615. The list needs storage only once this unit has a follower.
#[derive(Debug, Default)]
pub struct SourceFollowers {
    last_ordinal: u64,
    list: Option<Rc<FollowerList>>,
}
impl SourceFollowers {
    fn allocate(&mut self, unit: ObjectHandle) -> Result<FollowerReferenceId> {
        self.last_ordinal = self.last_ordinal.checked_add(1).ok_or_else(|| {
            refuse("execution.movement.TargetedMovementGenerator_34.follower_ordinal_wrap")
        })?;
        Ok(FollowerReferenceId {
            unit,
            ordinal: self.last_ordinal,
        })
    }

    fn list(&mut self) -> &Rc<FollowerList> {
        self.list.get_or_insert_with(Rc::default)
    }

    pub fn ids(&self) -> Vec<FollowerReferenceId> {
        self.list.as_ref().map_or_else(Vec::new, |list| {
            list.borrow().iter().map(|(id, _)| *id).collect()
        })
    }

    pub(super) fn last_ordinal(&self) -> u64 {
        self.last_ordinal
    }

    /// Unit.cpp:9422, RefManager.h:49-57. Invalidate the first reference until empty.
    pub fn clear_references(&mut self) {
        self.clear(|_| {});
    }

    fn clear(&mut self, mut invalidated: impl FnMut(FollowerReferenceId)) {
        let Some(list) = &self.list else { return };
        loop {
            let Some((id, source)) = list.borrow().front().cloned() else {
                break;
            };
            source.upgrade().unwrap().invalidate_reference();
            // RefManager.h:55 repeats delink, which LinkedList.h:57 makes inert.
            invalidated(id);
        }
    }
}
impl Drop for SourceFollowers {
    fn drop(&mut self) {
        // RefManager.h:35 clears the target's incoming references at destruction.
        self.clear_references();
    }
}

#[derive(Debug)]
struct FollowerTarget {
    unit: ObjectHandle,
    list: Weak<FollowerList>,
}
impl FollowerTarget {
    // Unit.h:2265 deliberately leaves the follower list alone.
    fn remove_follower(&self) {}
}

// Reference.h:47-49 starts both ends empty.
#[derive(Debug, Default)]
struct SourceFollowerReference {
    source: Cell<Option<FollowerReferenceId>>,
    target: RefCell<Option<FollowerTarget>>,
}
impl SourceFollowerReference {
    fn link(
        &self,
        target: ObjectHandle,
        followers: &mut SourceFollowers,
        id: FollowerReferenceId,
        source: &Weak<SourceFollowGenerator>,
    ) {
        // Reference.h:54-66 unlinks a valid old link before writing the two ends.
        if self.target().is_some() {
            self.unlink();
        }
        let list = followers.list();
        self.target.replace(Some(FollowerTarget {
            unit: target,
            list: Rc::downgrade(list),
        }));
        self.source.set(Some(id));
        // FollowerReference.cpp:25, Unit.h:2264, LinkedList.h:112-115 insert first.
        list.borrow_mut().push_front((id, source.clone()));
    }

    fn target(&self) -> Option<ObjectHandle> {
        self.target.borrow().as_ref().map(|target| target.unit)
    }

    fn unlink(&self) {
        // Reference.h:70-76; FollowerReference.cpp:30 notifies the target before delink.
        self.target.borrow().as_ref().unwrap().remove_follower();
        self.delink();
        self.target.replace(None);
        self.source.set(None);
    }

    fn invalidate(&self) {
        // Reference.h:80-85 leaves iRefFrom intact.
        self.delink();
        self.target.replace(None);
    }

    fn delink(&self) {
        // LinkedList.h:55-63 removes only this element, leaving the other links in order.
        if let Some(list) = self
            .target
            .borrow()
            .as_ref()
            .and_then(|target| target.list.upgrade())
        {
            let mut list = list.borrow_mut();
            let id = self.source.get().unwrap();
            if let Some(index) = list.iter().position(|(other, _)| *other == id) {
                list.remove(index);
            }
        }
    }
}
impl Drop for SourceFollowerReference {
    fn drop(&mut self) {
        // LinkedList.h:39 delinks without calling removeFollower or stopFollowing.
        self.delink();
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FollowRequest {
    pub target: ObjectHandle,
    pub offsets: [f32; 2],
    pub main: bool,
    pub boost: bool,
}
impl FollowRequest {
    pub fn linked(target: ObjectHandle, offsets: [f32; 2]) -> Self {
        // CreatureLinkingMgr.cpp:637 uses the defaults in MotionMaster.h.
        Self {
            target,
            offsets,
            main: false,
            boost: false,
        }
    }
}

#[derive(Debug)]
pub struct SourceFollowFields {
    pub recheck_distance: u32,
    pub offsets: [f32; 2],
    pub last_target_position: [f32; 3],
    pub speed_changed: bool,
    pub target_reached: bool,
    pub face_target: bool,
    pub path: Option<Box<SourcePathFinder>>,
    pub main: bool,
    pub possess: bool,
    pub boost: bool,
    pub target_moving: bool,
    pub target_faced: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceFollowNotice {
    DifferentMap {
        unit: ObjectHandle,
        target: ObjectHandle,
    },
}

pub struct SourceFollowGenerator {
    // TargetedMovementGenerator.h:54 deletes the path before the reference base drops.
    pub fields: SourceFollowFields,
    reference: SourceFollowerReference,
    notices: RefCell<Vec<SourceFollowNotice>>,
}
impl SourceFollowGenerator {
    /// Observes the attached base before the medium and Follow fields are written.
    pub fn construct(
        unit: ObjectHandle,
        request: FollowRequest,
        possess: bool,
        context: &mut MotionContext<'_>,
        on_linked: impl FnOnce(FollowerReferenceId, &SourceFollowers),
    ) -> Result<Rc<Self>> {
        let id = context
            .units
            .followers_mut(context.objects, unit)?
            .allocate(unit)?;
        let followers = context
            .units
            .followers_mut(context.objects, request.target)?;
        Ok(Rc::new_cyclic(|source| {
            // MovementGenerator.h:32-68 has no base fields; TargetedMovementGenerator.h:34 links before the medium constructor.
            let reference = SourceFollowerReference::default();
            reference.link(request.target, followers, id, source);
            on_linked(id, followers);
            // TargetedMovementGenerator.h:46-51,81-89; Timer.h:148 and Vector3.h:572.
            let mut fields = SourceFollowFields {
                recheck_distance: 0,
                offsets: request.offsets,
                last_target_position: [0.; 3],
                speed_changed: false,
                target_reached: false,
                face_target: true,
                path: None,
                // TargetedMovementGenerator.h:184-186 initializes the derived fields.
                main: request.main,
                possess,
                boost: request.boost,
                target_moving: false,
                target_faced: false,
            };
            // TargetedMovementGenerator.h:188 runs after both constructors.
            fields.face_target = request.offsets[1] == 0.;
            Self {
                fields,
                reference,
                notices: RefCell::default(),
            }
        }))
    }

    pub fn source(&self) -> FollowerReferenceId {
        self.reference.source.get().unwrap()
    }

    pub fn target(&self) -> Option<ObjectHandle> {
        self.reference.target()
    }

    /// TargetedMovementGenerator.h:36 relinks this generator without changing its identity.
    pub fn set_new_target(
        self: &Rc<Self>,
        target: ObjectHandle,
        context: &mut MotionContext<'_>,
    ) -> Result<()> {
        let followers = context.units.followers_mut(context.objects, target)?;
        self.reference
            .link(target, followers, self.source(), &Rc::downgrade(self));
        Ok(())
    }

    fn invalidate_reference(&self) {
        // Reference.h:83; FollowerReference.cpp:35 notifies the source before delink.
        self.stop_following();
        self.reference.invalidate();
    }

    // TargetedMovementGenerator.h:35 deliberately leaves the generator alone.
    fn stop_following(&self) {}

    pub fn notices(&self) -> std::cell::Ref<'_, [SourceFollowNotice]> {
        std::cell::Ref::map(self.notices.borrow(), Vec::as_slice)
    }
}
impl SourceMovementGenerator for SourceFollowGenerator {
    fn kind(&self) -> u32 {
        7 // MotionMaster.h:61, TargetedMovementGenerator.h:192.
    }

    fn as_follow(&self) -> Option<&Self> {
        Some(self)
    }

    fn initialize(
        &self,
        master: &mut SourceMotionMaster,
        context: &mut MotionContext<'_>,
        _: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        // TargetedMovementGenerator.cpp:813-814 short-circuits before either GetMap.
        let Some(target) = self.target() else {
            return Ok(());
        };
        let target_object = context.objects.get(target)?;
        if !target_object.in_world {
            return Ok(());
        }
        // TargetedMovementGenerator.cpp:816-819; Object.h:1061 asserts a map at GetMap.
        let target_map = target_object.map.ok_or_else(|| {
            refuse("execution.movement.TargetedMovementGenerator_816.target_GetMap_assertion")
        })?;
        let owner_map = context.objects.get(master.unit)?.map.ok_or_else(|| {
            refuse("execution.movement.TargetedMovementGenerator_816.owner_GetMap_assertion")
        })?;
        if target_map != owner_map {
            self.notices
                .borrow_mut()
                .push(SourceFollowNotice::DifferentMap {
                    unit: master.unit,
                    target,
                });
            return Ok(());
        }
        // TargetedMovementGenerator.cpp:822 sets FOLLOW, leaving FOLLOW_MOVE alone.
        context.units.get_mut(context.objects, master.unit)?.state |= FOLLOW;
        Ok(())
    }

    fn finalize(
        &self,
        master: &mut SourceMotionMaster,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        // TargetedMovementGenerator.cpp:827-829 clears both flags before notifying AI.
        let unit = context.units.get_mut(context.objects, master.unit)?;
        unit.state &= !(FOLLOW | FOLLOW_MOVE);
        let target = if unit.has_ai { self.target() } else { None };
        if let Some(target) = target {
            let guid = context.objects.get(target)?.guid.ok_or_else(|| {
                refuse("execution.movement.TargetedMovementGenerator_829.GetObjectGuid_owner")
            })?;
            owner.relinquish_follow(master.unit, guid)?;
        }
        Ok(())
    }

    fn interrupt(
        &self,
        master: &mut SourceMotionMaster,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        // TargetedMovementGenerator.cpp:710,832-836.
        context.units.get_mut(context.objects, master.unit)?.state &= !FOLLOW_MOVE;
        owner.motion_call(master, MotionCall::InterruptMoving, context)
    }

    fn reset(
        &self,
        master: &mut SourceMotionMaster,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        // TargetedMovementGenerator.cpp:838-841 delegates to Initialize.
        self.initialize(master, context, owner)
    }

    fn update(
        &self,
        _: &mut SourceMotionMaster,
        _: u32,
        _: &mut MotionContext<'_>,
        _: &mut dyn MotionExecutionOwner,
    ) -> Result<bool> {
        Err(refuse(
            "execution.movement.TargetedMovementGenerator_47.Update_owner",
        ))
    }

    fn reset_position(&self) -> Result<Option<[f32; 4]>> {
        Err(refuse(
            "execution.movement.TargetedMovementGenerator_845._getLocation_owner",
        ))
    }

    fn removed_on_expire(&self) -> bool {
        !self.fields.main // TargetedMovementGenerator.h:208.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn units<const N: usize>() -> (SourceObjects, SourceUnits, [ObjectHandle; N]) {
        let mut objects = SourceObjects::default();
        let mut units = SourceUnits::default();
        let handles = std::array::from_fn(|_| {
            let unit = objects.construct(7, SourceObjectKind::Creature).unwrap();
            units
                .register(&objects, unit, SourceUnitFields::constructed_creature())
                .unwrap();
            unit
        });
        (objects, units, handles)
    }

    #[test]
    fn fresh_link_relinks_first_and_drop_delinks() {
        let (objects, mut units, [misha, other, rexxar, second]) = units();
        let mut context = MotionContext {
            objects: &objects,
            units: &mut units,
        };
        let initial = context.units.inventory(&objects).unwrap();
        let misha = SourceFollowGenerator::construct(
            misha,
            FollowRequest::linked(rexxar, [3., -0.]),
            false,
            &mut context,
            |id, target| assert_eq!(target.ids(), [id]),
        )
        .unwrap();
        let other = SourceFollowGenerator::construct(
            other,
            FollowRequest::linked(second, [5., 1.]),
            true,
            &mut context,
            |_, _| {},
        )
        .unwrap();
        assert_eq!(misha.target(), Some(rexxar));
        assert_eq!(
            context.units.followers(&objects, rexxar).unwrap().ids(),
            [misha.source()]
        );
        assert!(misha.fields.face_target);
        assert!(!other.fields.face_target);
        assert!(other.fields.possess);
        let linked = context.units.inventory(&objects).unwrap();
        assert_eq!(linked["units"], initial["units"]);
        assert_eq!(
            linked["followers"][0],
            serde_json::json!([misha.source().unit, 1, []])
        );
        assert_eq!(
            linked["followers"][2],
            serde_json::json!([rexxar, 0, [misha.source()]])
        );
        assert_eq!(linked["restore"], "execution.unit.f7_restore_owner");

        misha.set_new_target(second, &mut context).unwrap();
        assert_eq!(misha.target(), Some(second));
        assert!(context
            .units
            .followers(&objects, rexxar)
            .unwrap()
            .ids()
            .is_empty());
        assert_eq!(
            context.units.followers(&objects, second).unwrap().ids(),
            [misha.source(), other.source()]
        );

        drop(misha);
        assert_eq!(
            context.units.followers(&objects, second).unwrap().ids(),
            [other.source()]
        );
        other.reference.unlink();
        assert_eq!(other.reference.source.get(), None);
        assert_eq!(other.target(), None);
        assert!(context
            .units
            .followers(&objects, second)
            .unwrap()
            .ids()
            .is_empty());
    }

    #[test]
    fn clear_invalidates_in_list_order_and_keeps_source() {
        let (objects, mut units, [a, b, target]) = units();
        let mut context = MotionContext {
            objects: &objects,
            units: &mut units,
        };
        let a = SourceFollowGenerator::construct(
            a,
            FollowRequest::linked(target, [0.; 2]),
            false,
            &mut context,
            |_, _| {},
        )
        .unwrap();
        let b = SourceFollowGenerator::construct(
            b,
            FollowRequest::linked(target, [0.; 2]),
            false,
            &mut context,
            |_, _| {},
        )
        .unwrap();
        let expected = [b.source(), a.source()];
        let followers = context.units.followers_mut(&objects, target).unwrap();
        let mut order = Vec::new();
        followers.clear(|id| {
            order.push(id);
            assert_eq!(b.target(), None);
            if id == expected[0] {
                assert_eq!(a.target(), Some(target));
            } else {
                assert_eq!(a.target(), None);
            }
        });
        assert_eq!(order, expected);
        assert!(followers.ids().is_empty());
        assert_eq!([b.source(), a.source()], expected);
        a.set_new_target(target, &mut context).unwrap();
        context
            .units
            .followers_mut(&objects, target)
            .unwrap()
            .clear_references();
        assert_eq!(a.target(), None);
        assert_eq!(a.source(), expected[1]);
    }

    #[derive(Default)]
    struct Unbound;
    impl MotionExecutionOwner for Unbound {}

    #[test]
    fn initialize_checks_residency_then_maps_before_setting_follow() {
        let (mut objects, mut units, [unit, target]) = units();
        let map = MapIdentity {
            map: 1,
            instance: 0,
            creation_generation: 1,
        };
        let other_map = MapIdentity { map: 2, ..map };
        objects
            .assign_guid(target, 1, SourceGuid::with_entry(HighGuid::Creature, 1, 1))
            .unwrap();
        objects
            .bind_map(target, map, &SourceCells::new(map).unwrap())
            .unwrap();
        let mut master = SourceMotionMaster::new(
            unit,
            MotionDefault {
                movement_type: 0,
                default_path: 0,
            },
        );
        let request = FollowRequest::linked(target, [0.; 2]);
        let follow = SourceFollowGenerator::construct(
            unit,
            request,
            false,
            &mut MotionContext {
                objects: &objects,
                units: &mut units,
            },
            |_, _| {},
        )
        .unwrap();

        // TargetedMovementGenerator.cpp:813 skips both GetMap assertions out of world.
        follow
            .initialize(
                &mut master,
                &mut MotionContext {
                    objects: &objects,
                    units: &mut units,
                },
                &mut Unbound,
            )
            .unwrap();
        assert_eq!(units.get(&objects, unit).unwrap().state, 0);
        objects.add_to_world(target).unwrap();
        let error = follow
            .initialize(
                &mut master,
                &mut MotionContext {
                    objects: &objects,
                    units: &mut units,
                },
                &mut Unbound,
            )
            .unwrap_err();
        assert_eq!(
            error.id,
            "execution.movement.TargetedMovementGenerator_816.owner_GetMap_assertion"
        );

        objects
            .bind_map(unit, other_map, &SourceCells::new(other_map).unwrap())
            .unwrap();
        follow
            .initialize(
                &mut master,
                &mut MotionContext {
                    objects: &objects,
                    units: &mut units,
                },
                &mut Unbound,
            )
            .unwrap();
        assert_eq!(
            *follow.notices(),
            [SourceFollowNotice::DifferentMap { unit, target }]
        );
        assert_eq!(units.get(&objects, unit).unwrap().state, 0);

        objects
            .bind_map(unit, map, &SourceCells::new(map).unwrap())
            .unwrap();
        follow
            .initialize(
                &mut master,
                &mut MotionContext {
                    objects: &objects,
                    units: &mut units,
                },
                &mut Unbound,
            )
            .unwrap();
        assert_eq!(units.get(&objects, unit).unwrap().state, FOLLOW);

        units
            .followers_mut(&objects, target)
            .unwrap()
            .clear_references();
        follow
            .finalize(
                &mut master,
                &mut MotionContext {
                    objects: &objects,
                    units: &mut units,
                },
                &mut Unbound,
            )
            .unwrap();
        follow
            .initialize(
                &mut master,
                &mut MotionContext {
                    objects: &objects,
                    units: &mut units,
                },
                &mut Unbound,
            )
            .unwrap();
        assert_eq!(units.get(&objects, unit).unwrap().state, 0);
    }

    #[test]
    fn ordinal_refuses_wrap_after_last_valid_link() {
        let (objects, mut units, [unit, target]) = units();
        units.followers_mut(&objects, unit).unwrap().last_ordinal = u64::MAX - 1;
        let mut context = MotionContext {
            objects: &objects,
            units: &mut units,
        };
        let request = FollowRequest::linked(target, [0.; 2]);
        let last = SourceFollowGenerator::construct(unit, request, false, &mut context, |_, _| {})
            .unwrap();
        assert_eq!(last.source().ordinal, u64::MAX);
        assert!(matches!(
            SourceFollowGenerator::construct(unit, request, false, &mut context, |_, _| panic!("wrapped ordinal linked")),
            Err(err) if err.id == "execution.movement.TargetedMovementGenerator_34.follower_ordinal_wrap"
        ));
        assert_eq!(
            context.units.followers(&objects, target).unwrap().ids(),
            [last.source()]
        );
    }
}
