// SPDX-License-Identifier: GPL-3.0-or-later
//! Production retains NativeStore. Recipe capture is a component-only durability path.
#[cfg(not(test))]
pub(super) type Store = benilla_world_store::native::NativeStore;

#[cfg(test)]
use benilla_world_store::native::{reconstruction::RecipeStore, Error, NativeStore, Receipt};

#[cfg(test)]
pub(super) enum Store {
    Native(Box<NativeStore>),
    Recipe(Box<RecipeStore>),
}
#[cfg(test)]
impl From<NativeStore> for Store {
    fn from(store: NativeStore) -> Self {
        Self::Native(Box::new(store))
    }
}
#[cfg(test)]
impl Store {
    pub(super) fn committed_admin_snapshot(
        &self,
        authenticated_owner: benilla_world_runtime::OwnerId,
    ) -> Result<
        benilla_world_runtime::native::AdminSnapshot,
        benilla_world_runtime::native::AdminRejection,
    > {
        match self {
            Self::Native(store) => store.committed_admin_snapshot(authenticated_owner),
            // Recipe capture has no admin receipt implementation or mailbox admission.
            Self::Recipe(_) => Err(benilla_world_runtime::native::AdminRejection::Disabled),
        }
    }
    pub(super) fn committed_admin_receipt(
        &self,
        request: &benilla_world_runtime::native::AdminRequest,
    ) -> Option<benilla_world_store::native::DurableAdminReceipt> {
        match self {
            Self::Native(store) => store.committed_admin_receipt(request),
            Self::Recipe(_) => None,
        }
    }
    pub(super) fn checkpoint(
        &mut self,
        world: &mut benilla_world_runtime::native::NativeWorld,
    ) -> Result<Receipt, Error> {
        match self {
            Self::Native(store) => store.checkpoint(world),
            Self::Recipe(store) => store.checkpoint(world),
        }
    }
    pub(super) fn requires_reopen(&self) -> bool {
        match self {
            Self::Native(store) => store.requires_reopen(),
            Self::Recipe(store) => store.requires_reopen(),
        }
    }
    pub(super) fn generation(&self) -> u64 {
        match self {
            Self::Native(store) => store.generation(),
            Self::Recipe(store) => store.generation(),
        }
    }
}
