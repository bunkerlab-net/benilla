// SPDX-License-Identifier: GPL-3.0-or-later
//! One logical F01/F02 scope, not freshness authority or a general view accessor.
use super::super::super::super::source_route_filter::resident::owned::f05::{
    F05ResumePermit, InactiveF05Proof,
};
use super::super::super::Error;
use super::super::{
    inventory::{InventoryCompatibility, OwnedInventoryCompatibility},
    CompositionError, PreparedSourceHeight,
};
use super::{freshness::F05Guard, OwnedSourceProducts};
use std::{rc::Rc, sync::Arc, time::Instant};

pub(in super::super::super::super) struct F05HeightFrame {
    products: Arc<OwnedSourceProducts>,
    custody: Rc<()>,
    invocation: Rc<()>,
    guard: Rc<F05Guard>,
}

pub(in super::super::super::super) struct F05InventoryFrame {
    inventory: Rc<OwnedInventoryCompatibility>,
    products: Arc<OwnedSourceProducts>,
    custody: Rc<()>,
    invocation: Rc<()>,
}

// Private fields: only the checked scoped callback below can mint this bridge.
// Inventory keeps its provenance fields private; no detached metadata crosses it.
pub(in super::super) struct F05ViewAccess<'a> {
    inventory: &'a Rc<OwnedInventoryCompatibility>,
}
impl F05ViewAccess<'_> {
    pub(in super::super) fn matches(&self, inventory: &OwnedInventoryCompatibility) -> bool {
        std::ptr::eq(self.inventory.as_ref(), inventory)
    }
}

impl F05HeightFrame {
    /// Consuming completion discharges F59 even when the saved inner result failed.
    /// Dropping instead is unfinished abandonment, never a completed source trace.
    pub(in super::super::super::super) fn finish<T, E: From<CompositionError>>(
        self,
        inner: Result<T, E>,
    ) -> Result<T, E> {
        self.products.verify_sources(|| {
            self.guard.cancelled() || Instant::now() >= self.guard.callback_deadline()
        })?;
        inner
    }
}

pub(in super::super::super::super) fn begin<T, E: From<CompositionError>>(
    products: Arc<OwnedSourceProducts>,
    inventory: Rc<OwnedInventoryCompatibility>,
    custody: Rc<()>,
    invocation: Rc<()>,
    guard: Rc<F05Guard>,
    read: impl for<'v> FnOnce(
        &'v PreparedSourceHeight<'v, 'v, 'static>,
        &InventoryCompatibility<'v, 'v, 'v, 'static>,
    ) -> Result<T, E>,
) -> Result<(F05HeightFrame, F05InventoryFrame, T), E> {
    if !guard.matches(&products, guard.lease()) {
        return Err(E::from(CompositionError::from(Error::Identity)));
    }
    let height = products.view();
    height.verify_unchanged(|| guard.cancelled() || Instant::now() >= guard.callback_deadline())?;
    let frame = F05HeightFrame {
        products: Arc::clone(&products),
        custody: Rc::clone(&custody),
        invocation: Rc::clone(&invocation),
        guard: Rc::clone(&guard),
    };
    // No ? may leave the logical scope after F01 without paying F59 on error.
    let inner: Result<T, E> = (|| {
        let compatibility =
            inventory.reborrow(&height, guard.callback_deadline(), || guard.cancelled())?;
        let output = read(&height, &compatibility)?;
        Ok(output)
    })();
    match inner {
        Ok(output) => Ok((
            frame,
            F05InventoryFrame {
                inventory,
                products: Arc::clone(&products),
                custody,
                invocation,
            },
            output,
        )),
        Err(error) => frame.finish(Err(error)),
    }
}

fn with_checked_views<T, E: From<CompositionError>>(
    height: &F05HeightFrame,
    inventory: &F05InventoryFrame,
    current_inventory: &Rc<OwnedInventoryCompatibility>,
    permitted: bool,
    read: impl for<'v> FnOnce(
        &'v PreparedSourceHeight<'v, 'v, 'static>,
        &InventoryCompatibility<'v, 'v, 'v, 'static>,
    ) -> Result<T, E>,
) -> Result<T, E> {
    if !permitted
        || !Arc::ptr_eq(&height.products, &inventory.products)
        || !Rc::ptr_eq(&height.custody, &inventory.custody)
        || !Rc::ptr_eq(&height.invocation, &inventory.invocation)
        || !Rc::ptr_eq(current_inventory, &inventory.inventory)
    {
        return Err(E::from(CompositionError::from(Error::Identity)));
    }
    let view = height.products.view();
    let access = F05ViewAccess {
        inventory: &inventory.inventory,
    };
    let compatibility = inventory.inventory.reconstitute_f05(&view, &access)?;
    read(&view, &compatibility)
}

pub(in super::super::super::super) fn with_f05_views<T, E: From<CompositionError>>(
    height: &F05HeightFrame,
    inventory: &F05InventoryFrame,
    current_inventory: &Rc<OwnedInventoryCompatibility>,
    permit: &F05ResumePermit,
    read: impl for<'v> FnOnce(
        &'v PreparedSourceHeight<'v, 'v, 'static>,
        &InventoryCompatibility<'v, 'v, 'v, 'static>,
    ) -> Result<T, E>,
) -> Result<T, E> {
    with_checked_views(
        height,
        inventory,
        current_inventory,
        permit.matches(
            &height.products,
            &height.custody,
            &height.invocation,
            &height.guard,
        ),
        read,
    )
}

// Only the driver's fixed cancelled-root-postcheck routine receives this proof.
// It cannot mint the active resume permit or continue F06.
pub(in super::super::super::super) fn with_inactive_f05_views<T, E: From<CompositionError>>(
    height: &F05HeightFrame,
    inventory: &F05InventoryFrame,
    current_inventory: &Rc<OwnedInventoryCompatibility>,
    proof: &InactiveF05Proof,
    read: impl for<'v> FnOnce(
        &'v PreparedSourceHeight<'v, 'v, 'static>,
        &InventoryCompatibility<'v, 'v, 'v, 'static>,
    ) -> Result<T, E>,
) -> Result<T, E> {
    with_checked_views(
        height,
        inventory,
        current_inventory,
        proof.matches(
            &height.products,
            &height.custody,
            &height.invocation,
            &height.guard,
        ),
        read,
    )
}
