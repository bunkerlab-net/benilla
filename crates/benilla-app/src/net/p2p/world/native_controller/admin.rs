// SPDX-License-Identifier: GPL-3.0-or-later
//! Nonblocking UI mailbox for the explicitly selected native controller. No default host.
use super::*;
use benilla_ui::script::admin as ui;
use sha2::{Digest as _, Sha256};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

fn digest(domain: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(domain);
    for part in parts {
        hash.update(part);
    }
    hash.finalize().into()
}
const MAX_MAILBOX_REQUESTS: usize = n::MAX_ADMIN_RECEIPTS;
#[derive(Clone, Debug, PartialEq, Eq)]
struct View {
    snapshot: Result<n::AdminSnapshot, n::AdminRejection>,
    unavailable: Option<String>,
    items: Vec<ui::AdminCatalogEntry>,
}
struct Request {
    ui: ui::AdminRequest,
    native: n::AdminRequest,
    admitted: bool,
    result: Option<ui::AdminResult>,
}
struct Shared {
    world: runtime::WorldId,
    player: runtime::PlayerId,
    profile: runtime::Digest,
    context: ui::AdminContext,
    namespace: [u8; 16],
    revision: u64,
    view: View,
    requests: BTreeMap<(String, u64), Request>,
    arrival: VecDeque<(String, u64)>,
}
#[derive(Clone)]
pub(super) struct ControllerBridge(Arc<Mutex<Shared>>);
#[cfg(test)]
impl ControllerBridge {
    pub(super) fn with_locked_for_test(&self, check: impl FnOnce()) {
        let _guard = self.0.lock().unwrap();
        check();
    }
}
struct CachedCatalog {
    revision: u64,
    entries: std::rc::Rc<[ui::AdminCatalogEntry]>,
}
pub(crate) struct Backend {
    catalog: RefCell<Option<CachedCatalog>>,
    accepted: RefCell<BTreeMap<u64, ui::AdminRequest>>,
    shared: Arc<Mutex<Shared>>,
    context: ui::AdminContext,
}

fn rejected(id: ui::AdminRequestId, reason: impl Into<String>) -> ui::AdminResult {
    ui::AdminResult {
        id,
        outcome: ui::AdminOutcome::Rejected {
            reason: reason.into(),
        },
    }
}
fn committed(
    id: ui::AdminRequestId,
    receipt: benilla_world_store::native::DurableAdminReceipt,
) -> ui::AdminResult {
    ui::AdminResult {
        id,
        outcome: ui::AdminOutcome::Committed {
            receipt: format!(
                "{}native-admin:{:02x?}:store-{}:{:02x?}",
                match &receipt.receipt().effect {
                    n::AdminEffect::ItemGrant(g) => format!(
                        "Item {}: granted {} of {}; {} ungranted ({:?}). ",
                        g.item, g.granted, g.requested, g.ungranted, g.storage_result
                    ),
                    _ => String::new(),
                },
                receipt.receipt().digest(),
                receipt.generation(),
                receipt.envelope_digest()
            ),
        },
    }
}
impl ui::AdminBackend for Backend {
    fn snapshot(&self) -> ui::AdminSnapshot {
        let Ok(shared) = self.shared.try_lock() else {
            let mut view =
                ui::AdminSnapshot::unavailable("World admin is busy; try again shortly.");
            view.context = Some(self.context.clone());
            return view;
        };
        let Ok(snapshot) = &shared.view.snapshot else {
            return ui::AdminSnapshot::unavailable(
                "This selected owner has no admitted world-admin permission.",
            );
        };
        let mut view =
            ui::AdminSnapshot::unavailable("This operation has no admitted native backend.");
        view.context = Some(self.context.clone());
        view.revision = shared.revision;
        view.identity = "Selected native-world administrator".into();
        view.status = format!(
            "Supported levels 1–{}; configured cap {}, Classic ceiling {}",
            snapshot.supported_maximum, snapshot.configured_maximum, snapshot.classic_ceiling
        );
        if snapshot.supported_maximum < snapshot.configured_maximum {
            view.status
                .push_str(". XP needing unavailable levels pauses simulation.");
        }
        view.level = Some(ui::AdminLevel {
            current: snapshot.level as u8,
            maximum: snapshot.supported_maximum as u8,
        });
        view.xp_enabled = Some(snapshot.xp_enabled);
        // Preserve Rc identity for unchanged views: the UI's cached search must not
        // clone and compare the full item catalog on each quarter-second refresh.
        let mut catalog = self.catalog.borrow_mut();
        if catalog
            .as_ref()
            .is_none_or(|cached| cached.revision != shared.revision)
        {
            *catalog = Some(CachedCatalog {
                revision: shared.revision,
                entries: shared.view.items.clone().into(),
            });
        }
        view.items = catalog
            .as_ref()
            .expect("catalog initialized")
            .entries
            .clone();
        if !snapshot.items.is_empty() {
            view.status
                .push_str(". Carried grants only; bag equipment and item use are unavailable.");
        }
        let unavailable = (shared.context != self.context).then(|| "This UI attachment has ended; only prior results may be queried.".to_owned())
            .or_else(|| shared.requests.values().any(|r| r.ui.id.context != shared.context && r.result.is_none()).then(|| "A prior UI request is still pending; reconcile it before submitting another operation.".to_owned()))
            .or_else(|| shared.view.unavailable.clone()).or_else(|| {
            (snapshot.receipts_remaining == 0 || shared.requests.len() >= MAX_MAILBOX_REQUESTS)
                .then(|| {
                    "Admin receipt capacity is full; ordinary gameplay remains available.".into()
                })
        });
        let availability = unavailable.map_or(
            ui::AdminAvailability::Available,
            ui::AdminAvailability::Unavailable,
        );
        view.availability[ui::AdminAction::Level as usize] = availability.clone();
        view.availability[ui::AdminAction::Xp as usize] = availability.clone();
        if !snapshot.items.is_empty() {
            view.availability[ui::AdminAction::Item as usize] = availability;
        }
        if snapshot.level >= snapshot.supported_maximum {
            view.availability[ui::AdminAction::Level as usize] = ui::AdminAvailability::Unavailable(
                "Already at this world's supported maximum level.".into(),
            );
        }
        view
    }
    fn submit(&self, request: ui::AdminRequest) -> Result<(), String> {
        if request.id.context != self.context {
            return Err("The world-admin context is stale.".into());
        }
        if let Some(previous) = self.accepted.borrow().get(&request.id.sequence) {
            return if previous == &request {
                Ok(())
            } else {
                Err("The request ID was already used for another operation.".into())
            };
        }
        let mut shared = self
            .shared
            .try_lock()
            .map_err(|_| "World admin is busy; request was not admitted.".to_owned())?;
        if request.id.context != self.context {
            return Err("The world-admin context is stale.".into());
        }
        let correlation = (request.id.context.0.clone(), request.id.sequence);
        if let Some(previous) = shared.requests.get(&correlation) {
            return if previous.ui == request {
                Ok(())
            } else {
                Err("The request ID was already used for another operation.".into())
            };
        }
        if request.id.context != shared.context {
            return Err("This UI attachment has ended.".into());
        }
        if shared
            .requests
            .values()
            .any(|r| r.ui.id.context != shared.context && r.result.is_none())
        {
            return Err("A prior UI request is still pending; reconcile it first.".into());
        }
        if request.catalog_revision != shared.revision {
            return Err("The world-admin view has changed; refresh before submitting.".into());
        }
        if let Some(reason) = &shared.view.unavailable {
            return Err(reason.clone());
        }
        let snapshot = shared
            .view
            .snapshot
            .as_ref()
            .map_err(|e| format!("World admin unavailable: {e:?}"))?;
        if snapshot.receipts_remaining == 0 || shared.requests.len() >= MAX_MAILBOX_REQUESTS {
            return Err("Admin receipt capacity is full; request was not admitted.".into());
        }
        let pending = shared
            .requests
            .values()
            .filter(|r| !r.admitted && r.result.is_none())
            .count();
        if pending >= MAX_ACTIONS {
            return Err("Admin queue is full; request was not admitted.".into());
        }
        let operation = match request.operation {
            ui::AdminOperation::GrantItem { item_id, count } => {
                if count == 0 || count > ui::MAX_ITEM_GRANT {
                    return Err("Invalid item grant count.".into());
                }
                let item = snapshot
                    .items
                    .iter()
                    .find(|i| i.item == item_id)
                    .ok_or("Item is absent from the selected catalog.")?;
                item.grant.clone()?;
                n::AdminOperation::GrantItem {
                    item: item_id,
                    count,
                }
            }
            ui::AdminOperation::GainOneLevel => n::AdminOperation::GainOneLevel,
            ui::AdminOperation::SetXpGain { enabled } => n::AdminOperation::SetXpGain { enabled },
            _ => return Err("This operation has no admitted native backend.".into()),
        };
        // Public random incarnation + UI correlation yields a stable per-session operation ID.
        // This carries no permission; the controller's authenticated selection is the actor.
        let digest = digest(
            b"benilla-admin-ui-request-v1\0",
            &[&shared.namespace, &request.id.sequence.to_le_bytes()],
        );
        let key = digest[..16].try_into().expect("16-byte nonce");
        let native = n::AdminRequest {
            key,
            world: snapshot.world,
            player: snapshot.player,
            policy_revision: snapshot.policy_revision,
            operation,
        };
        let accepted = request.clone();
        shared.arrival.push_back(correlation.clone());
        shared.requests.insert(
            correlation,
            Request {
                ui: request,
                native,
                admitted: false,
                result: None,
            },
        );
        self.accepted
            .borrow_mut()
            .insert(accepted.id.sequence, accepted);
        Ok(())
    }
    fn poll(&self, id: &ui::AdminRequestId) -> Option<ui::AdminResult> {
        let shared = self.shared.try_lock().ok()?;
        if id.context != self.context {
            return None;
        }
        shared
            .requests
            .get(&(id.context.0.clone(), id.sequence))?
            .result
            .clone()
    }
}
impl NativeController {
    fn admin_view(&self) -> View {
        let snapshot = self
            .store
            .committed_admin_snapshot(self.selected.player.owner);
        let items = snapshot.as_ref().map_or_else(
            |_| Vec::new(),
            |s| {
                s.items
                    .iter()
                    .filter_map(|i| {
                        let name = self.projection.admin_item_label(i.item)?;
                        let label = match &i.grant {
                            Ok(()) => name,
                            Err(reason) => format!("{name} (grant unavailable: {reason})"),
                        };
                        Some(ui::AdminCatalogEntry { id: i.item, label })
                    })
                    .collect()
            },
        );
        View {
            items,
            snapshot,
            unavailable: if self.closed || self.closing {
                Some("The world session has closed.".into())
            } else if self.pause.is_some() || self.outbox.is_some() {
                Some("World persistence or simulation is paused; resolve it before a new admin request.".into())
            } else {
                None
            },
        }
    }
    /// Called only by an admitted native entry producer. There is deliberately no such
    /// production producer yet; component tests install this same bridge explicitly.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "NativeCapability has no production constructor")
    )]
    pub(crate) fn admin_backend(&mut self) -> Backend {
        let namespace: [u8; 16] = rand::random();
        let context = digest(
            b"benilla-native-admin-context-v1\0",
            &[
                &namespace,
                &self.world.state().world,
                &self.selected.player.owner,
                &self.selected.player.character,
                &self.world.profile().digest(),
            ],
        );
        let context = ui::AdminContext(format!("native-admin:{context:02x?}"));
        if let Some(existing) = &self.admin_bridge {
            let mut shared = existing
                .0
                .lock()
                .expect("world-admin mailbox is not poisoned");
            shared.context = context.clone();
            shared.namespace = namespace;
            shared.revision = shared.revision.saturating_add(1);
            return Backend {
                catalog: RefCell::new(None),
                accepted: RefCell::new(BTreeMap::new()),
                shared: existing.0.clone(),
                context,
            };
        }
        let shared = Arc::new(Mutex::new(Shared {
            world: self.world.state().world,
            player: self.selected.player,
            profile: self.world.profile().digest(),
            context: context.clone(),
            namespace,
            revision: 1,
            view: self.admin_view(),
            requests: BTreeMap::new(),
            arrival: VecDeque::new(),
        }));
        self.admin_bridge = Some(ControllerBridge(shared.clone()));
        Backend {
            catalog: RefCell::new(None),
            accepted: RefCell::new(BTreeMap::new()),
            shared,
            context,
        }
    }
    /// A new exclusively opened controller can reconcile an old bridge's retained IDs.
    /// It does not resubmit anything and cannot turn an old context into new permission.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "NativeCapability has no production constructor")
    )]
    pub(crate) fn reconcile_admin_backend(&self, old: &Backend) -> Result<(), String> {
        if self.admin_bridge.is_some()
            || !self.actions.is_empty()
            || self.pending.is_some()
            || self.outbox.is_some()
            || self.world.state().tick != self.saved_tick
        {
            return Err("Reconciliation requires a freshly opened durable world.".into());
        }
        let mut shared = old
            .shared
            .try_lock()
            .map_err(|_| "The previous bridge is busy.".to_owned())?;
        if shared.world != self.world.state().world
            || shared.player != self.selected.player
            || shared.profile != self.world.profile().digest()
            || shared.requests.values().any(|r| {
                r.native.world != self.world.state().world
                    || r.native.player != self.selected.player
            })
        {
            return Err("The prior requests belong to another selected world or character.".into());
        }
        shared.view = self.admin_view();
        shared.view.unavailable =
            Some("This session has ended; prior requests have been reconciled.".into());
        shared.revision = shared.revision.saturating_add(1);
        for request in shared.requests.values_mut().filter(|r| r.result.is_none()) {
            request.result = Some(match self.store.committed_admin_receipt(&request.native) {
                Some(receipt) => committed(request.ui.id.clone(), receipt),
                None => rejected(
                    request.ui.id.clone(),
                    "The reopened durable world contains no commit for this request; it was not resubmitted.",
                ),
            });
        }
        Ok(())
    }
    pub(super) fn publish_admin_rejections(
        &mut self,
        rejected_requests: &[(n::AdminRequest, n::AdminRejection)],
    ) {
        let Some(bridge) = &self.admin_bridge else {
            return;
        };
        // The controller is the sole mutating owner. Keep lock scope to this bounded map update.
        let Ok(mut shared) = bridge.0.lock() else {
            return;
        };
        for (native, reason) in rejected_requests {
            if let Some(request) = shared.requests.values_mut().find(|r| &r.native == native) {
                request.result = Some(rejected(
                    request.ui.id.clone(),
                    format!("Admin request rejected: {reason:?}"),
                ));
            }
        }
    }
    fn submit_admin(&mut self, request: n::AdminRequest) -> Result<(), String> {
        // An exact durable retry remains queryable while paused and cannot enqueue a grant.
        if self.store.committed_admin_receipt(&request).is_some() {
            return Ok(());
        }
        self.world
            .validate_admin_request(self.selected.player.owner, &request)
            .map_err(|e| format!("Admin request rejected: {e:?}"))?;
        if self.world.admin_receipt(&request.key).is_some() {
            return Ok(());
        }
        // An exact queued request is also idempotent before its first world tick.
        let queued = self
            .actions
            .iter()
            .filter_map(|i| match i {
                Intent::Command {
                    command: n::Command::Admin(r),
                    ..
                } => Some(r),
                _ => None,
            })
            .chain(
                self.pending
                    .iter()
                    .flat_map(|p| p.inputs.iter())
                    .filter_map(|i| match &i.input.command {
                        n::Command::Admin(r) => Some(r),
                        _ => None,
                    }),
            );
        for previous in queued {
            if previous.key == request.key {
                return if previous == &request {
                    Ok(())
                } else {
                    Err("Admin request ID conflict.".into())
                };
            }
        }
        self.ensure_input().map_err(|e| e.to_string())?;
        self.queue(n::Command::Admin(request))
            .map_err(|e| e.to_string())
    }
    pub(super) fn admit_admin_mailbox(&mut self) {
        let Some(bridge) = self.admin_bridge.clone() else {
            return;
        };
        let Ok(mut shared) = bridge.0.try_lock() else {
            return;
        };
        // Correlation sequence is not an ordering authority. Preserve actual arrival FIFO.
        for _ in 0..MAX_ACTIONS {
            let Some(sequence) = shared.arrival.pop_front() else {
                break;
            };
            let request = shared
                .requests
                .get_mut(&sequence)
                .expect("queued admin request");
            if request.admitted || request.result.is_some() {
                continue;
            }
            match self.submit_admin(request.native.clone()) {
                Ok(()) => request.admitted = true,
                Err(reason) => request.result = Some(rejected(request.ui.id.clone(), reason)),
            }
        }
    }
    pub(super) fn refresh_admin_mailbox(&mut self) {
        let Some(bridge) = self.admin_bridge.clone() else {
            return;
        };
        let Ok(mut shared) = bridge.0.try_lock() else {
            return;
        };
        let view = self.admin_view();
        if view != shared.view {
            shared.revision = shared.revision.saturating_add(1);
            shared.view = view;
        }
        for request in shared.requests.values_mut().filter(|r| r.result.is_none()) {
            if let Some(receipt) = self.store.committed_admin_receipt(&request.native) {
                request.result = Some(committed(request.ui.id.clone(), receipt));
            } else if self.closed {
                request.result = Some(rejected(
                    request.ui.id.clone(),
                    "The world closed before this request was committed.",
                ));
            }
        }
    }
}

impl Drop for NativeController {
    fn drop(&mut self) {
        if let Some(bridge) = &self.admin_bridge {
            if let Ok(mut shared) = bridge.0.lock() {
                // Losing the controller does not tell us whether an uncertain save committed.
                // Keep pending IDs for reopen reconciliation, but admit no new stale work.
                shared.view.unavailable = Some(
                    "The world session ended; reconcile pending requests before retrying.".into(),
                );
                shared.revision = shared.revision.saturating_add(1);
            }
        }
    }
}
