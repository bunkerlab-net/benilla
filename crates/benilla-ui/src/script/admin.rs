//! Fork-only Admin panel boundary, installed explicitly by the application layer. UI validation
//! is convenience, never authorization: the host must authenticate the signed world owner or an
//! explicit admin delegation independently of PSK membership, revalidate catalogs and state,
//! and durably deduplicate/commit each operation before returning a receipt.

use std::rc::Rc;

use mlua::{Lua, Table};

use super::UiScript;

mod catalog;

#[cfg(test)]
mod tests;

use catalog::{CatalogSearch, MAX_CATALOG_ENTRIES, MAX_LABEL_BYTES, PAGE_SIZE};

// Conservative per-request UI ceilings, not claims about Classic account/ruleset maxima.
// The authoritative backend may impose tighter source-validated limits.
pub const MAX_ITEM_GRANT: u32 = 200;
pub const MAX_GOLD_GRANT: u32 = 10_000;
pub const MIN_SPEED_PERCENT: u32 = 100;
pub const MAX_SPEED_PERCENT: u32 = 300;
pub const MAX_REPUTATION_DELTA: u32 = 42_999;
pub const MAX_HONOR_CONTRIBUTION: u32 = 100_000;

/// Catalog keys are supplied by the admitted world backend, never guessed IDs/coordinates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminCatalogEntry {
    pub id: u32,
    pub label: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdminAction {
    Item,
    Gold,
    Mount,
    Speed,
    Teleport,
    Level,
    Xp,
    Reputation,
    Honor,
    Time,
    Weather,
}

impl AdminAction {
    pub const ALL: [Self; 11] = [
        Self::Item,
        Self::Gold,
        Self::Mount,
        Self::Speed,
        Self::Teleport,
        Self::Level,
        Self::Xp,
        Self::Reputation,
        Self::Honor,
        Self::Time,
        Self::Weather,
    ];

    fn key(self) -> &'static str {
        match self {
            Self::Item => "item",
            Self::Gold => "gold",
            Self::Mount => "mount",
            Self::Speed => "speed",
            Self::Teleport => "teleport",
            Self::Level => "level",
            Self::Xp => "xp",
            Self::Reputation => "reputation",
            Self::Honor => "honor",
            Self::Time => "time",
            Self::Weather => "weather",
        }
    }

    fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.key() == key)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdminAvailability {
    Available,
    Unavailable(String),
}

/// A host-owned, opaque binding to the current world, character, verified principal and session.
/// This value is a correlation token, not an authentication credential. The backend owns its
/// meaning and must invalidate it on any identity/world/session change, including reconnect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminContext(pub String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminLevel {
    pub current: u8,
    pub maximum: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminLocation {
    pub map_id: u32,
    pub zone_id: u32,
    /// Host-issued location revision; changes even when leaving and re-entering the same zone.
    pub revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminSnapshot {
    pub context: Option<AdminContext>,
    /// Changes whenever capabilities, catalog or authority changes, within this context.
    pub revision: u64,
    pub identity: String,
    pub status: String,
    /// In `AdminAction::ALL` order. Availability alone never authorizes a mutation.
    pub availability: [AdminAvailability; 11],
    pub items: Rc<[AdminCatalogEntry]>,
    pub mounts: Rc<[AdminCatalogEntry]>,
    pub cities: Rc<[AdminCatalogEntry]>,
    pub factions: Rc<[AdminCatalogEntry]>,
    pub level: Option<AdminLevel>,
    pub xp_enabled: Option<bool>,
    pub location: Option<AdminLocation>,
    /// Current-location-valid weather choices, including an explicit automatic option.
    /// Keys are opaque host catalog IDs, not guessed Classic weather types.
    pub weather: Rc<[AdminCatalogEntry]>,
    pub weather_automatic_id: Option<u32>,
}

impl AdminSnapshot {
    pub fn unavailable(reason: &str) -> Self {
        Self {
            context: None,
            revision: 0,
            identity: "No verified world admin".into(),
            status: reason.into(),
            availability: std::array::from_fn(|_| AdminAvailability::Unavailable(reason.into())),
            items: Rc::from([]),
            mounts: Rc::from([]),
            cities: Rc::from([]),
            factions: Rc::from([]),
            level: None,
            xp_enabled: None,
            location: None,
            weather: Rc::from([]),
            weather_automatic_id: None,
        }
    }

    fn catalog(&self, action: AdminAction) -> &[AdminCatalogEntry] {
        match action {
            AdminAction::Item => &self.items,
            AdminAction::Mount => &self.mounts,
            AdminAction::Teleport => &self.cities,
            AdminAction::Reputation => &self.factions,
            AdminAction::Weather => &self.weather,
            _ => &[],
        }
    }

    fn reason(&self, action: AdminAction) -> Option<&str> {
        if self.catalog(action).len() > MAX_CATALOG_ENTRIES {
            return Some("Catalog exceeds the supported browse limit. Ask the world host to supply a bounded catalog.");
        }
        if self.context.as_ref().is_none_or(|c| c.0.is_empty()) {
            return Some("This session has no verified world admin authority.");
        }
        if let AdminAvailability::Unavailable(reason) = &self.availability[action as usize] {
            return Some(if reason.trim().is_empty() {
                "This admin operation is unavailable."
            } else {
                reason
            });
        }
        if matches!(
            action,
            AdminAction::Item
                | AdminAction::Mount
                | AdminAction::Teleport
                | AdminAction::Reputation
                | AdminAction::Weather
        ) && self.catalog(action).is_empty()
        {
            return Some("No supported entries in this world's validated catalog.");
        }
        if action == AdminAction::Level {
            match &self.level {
                Some(level)
                    if (1..=60).contains(&level.maximum)
                        && level.current > 0
                        && level.current < level.maximum => {}
                Some(level) if level.current == level.maximum && level.maximum > 0 => {
                    return Some("Already at the supported maximum level.")
                }
                _ => return Some("Supported Classic level rules are unavailable."),
            }
        }
        if action == AdminAction::Xp && self.xp_enabled.is_none() {
            return Some("Persistent XP-gain control is unavailable.");
        }
        if action == AdminAction::Weather {
            if self.location.is_none() {
                return Some("Current-location weather authority is unavailable.");
            }
            if self.weather_automatic_id.is_none_or(|id| {
                let mut matches = self.weather.iter().filter(|entry| entry.id == id);
                matches.next().is_none_or(|entry| {
                    entry.label.len() > MAX_LABEL_BYTES || entry.label.trim().is_empty()
                }) || matches.next().is_some()
            }) {
                return Some("A validated Automatic weather option is unavailable.");
            }
        }
        None
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdminOperation {
    GrantItem {
        item_id: u32,
        count: u32,
    },
    GrantGold {
        copper: u32,
    },
    /// A supported mount grant, not an arbitrary display/model substitution.
    GrantMount {
        mount_id: u32,
    },
    /// Must enter the authoritative movement/ACK path, never a client movement multiplier.
    SetMovementSpeed {
        basis_points: u32,
    },
    /// A city key whose validated map/position remains backend-owned.
    Teleport {
        city_id: u32,
    },
    /// Exactly one level; the backend must recheck its ruleset cap and level-up side effects.
    GainOneLevel,
    /// Preserve current XP; durably gate every award source, including quests and kills.
    SetXpGain {
        enabled: bool,
    },
    AdjustReputation {
        faction_id: u32,
        delta: i32,
    },
    /// Classic honor contribution accounting, not a spendable modern currency.
    GrantHonorContribution {
        points: u32,
    },
    /// Shared civil/daylight projection only: never rewinds simulation, cooldown or reset clocks.
    SetWorldTime {
        minute_of_day: u16,
    },
    RestoreAutomaticWorldTime,
    /// The catalog includes return-to-automatic; exact type/intensity remain authority-owned.
    SetLocationWeather {
        location: AdminLocation,
        option_id: u32,
    },
    RestoreAutomaticWeather {
        location: AdminLocation,
    },
}

/// UI correlation only. The host must bind it to its durable per-world replay ledger; VM-local
/// sequences reset on UI reload and are not a substitute for signed operation identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminRequestId {
    pub context: AdminContext,
    pub sequence: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminRequest {
    pub id: AdminRequestId,
    pub catalog_revision: u64,
    pub operation: AdminOperation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdminOutcome {
    /// Only after durable atomic commit, including the operation's replay record.
    Committed {
        receipt: String,
    },
    Rejected {
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminResult {
    pub id: AdminRequestId,
    pub outcome: AdminOutcome,
}

/// Explicit host attachment. None is installed in the current exploration/realm application.
/// `submit` only acknowledges receipt, never success; `poll` provides the authoritative result.
/// An Err is definitive non-admission: no mutation/queued operation may have occurred. Ambiguous
/// transport outcomes must return Ok and remain pending until reconciled through poll; they
/// must never allow a new request ID to repeat a possibly committed grant.
/// Implementations must not perform filesystem/network work on the UI thread. They must reject
/// stale contexts, unauthorized identities, unsupported data, overflow and invalid positions,
/// and atomically persist inventory/money/mount/world changes with the replay ledger. Teleport
/// and speed results require the matching world/session and authoritative movement handshake.
/// Multiplayer peers must validate the same signed world-scoped operation in their agreed
/// ordering; this interface does not designate a central simulation server or new network path.
/// A PSK, a UI capability, or the presence of this panel never grants admin authority.
pub trait AdminBackend {
    fn snapshot(&self) -> AdminSnapshot;
    fn submit(&self, request: AdminRequest) -> Result<(), String>;
    fn poll(&self, id: &AdminRequestId) -> Option<AdminResult>;
}

#[derive(Default)]
struct AdminUi {
    backend: Option<Rc<dyn AdminBackend>>,
    shown: Option<AdminSnapshot>,
    pending: Option<AdminRequestId>,
    sequence: u64,
    message: String,
    message_context: Option<AdminContext>,
    generation: u64,
    catalogs: [Option<CatalogSearch>; 11],
    reasons: [Option<String>; 11],
}

impl AdminUi {
    fn snapshot(&self) -> AdminSnapshot {
        self.backend.as_ref().map_or_else(
            || AdminSnapshot::unavailable("Admin gameplay backend unavailable in this session."),
            |backend| backend.snapshot(),
        )
    }

    fn refresh(&mut self) -> (AdminSnapshot, bool) {
        let snapshot = self.snapshot();
        if self.pending.is_none() && self.message_context != snapshot.context {
            self.message.clear();
        }
        if let Some(id) = &self.pending {
            if snapshot.context.as_ref() != Some(&id.context) {
                self.message =
                    "Session changed. Previous result is unknown; verify it before retrying."
                        .into();
                // Retain the latch: an unknown result is not permission to repeat a grant.
            } else if let Some(result) = self.backend.as_ref().and_then(|b| b.poll(id)) {
                if result.id != *id {
                    self.message =
                        "Admin result did not match the pending request. No success confirmed."
                            .into();
                } else {
                    self.message_context = Some(result.id.context.clone());
                    self.message = match result.outcome {
                        AdminOutcome::Committed { receipt } if !receipt.trim().is_empty() => {
                            self.pending = None;
                            format!("Committed. Receipt: {receipt}")
                        }
                        AdminOutcome::Rejected { reason } => {
                            self.pending = None;
                            format!("Rejected: {reason}")
                        }
                        AdminOutcome::Committed { .. } => {
                            "Missing durable receipt. No success confirmed; verify before retrying."
                                .into()
                        }
                    };
                }
            }
        }
        let changed = self
            .shown
            .as_ref()
            .is_none_or(|shown| !same_snapshot(shown, &snapshot));
        if changed {
            self.generation = self
                .generation
                .checked_add(1)
                .expect("admin view generation exhausted");
            self.catalogs = std::array::from_fn(|_| None);
            self.reasons =
                AdminAction::ALL.map(|action| snapshot.reason(action).map(str::to_owned));
        }
        self.shown = Some(snapshot.clone());
        (snapshot, changed)
    }

    fn submit(
        &mut self,
        action: &str,
        selection: &str,
        amount: &str,
        generation: Option<&str>,
    ) -> Result<(), String> {
        if self.pending.is_some() {
            return Err("An admin request is pending. Wait for its authoritative result.".into());
        }
        let snapshot = self.snapshot();
        let shown = self
            .shown
            .as_ref()
            .ok_or("Reopen Admin to refresh this session.")?;
        if !same_snapshot(shown, &snapshot)
            || generation.is_some_and(|value| value != self.generation.to_string())
        {
            return Err("Admin context or catalog changed. Refresh and select again.".into());
        }
        let action = AdminAction::parse(action).ok_or("Unsupported admin operation.")?;
        if let Some(reason) = snapshot.reason(action) {
            return Err(reason.into());
        }
        let operation = validate_operation(&snapshot, action, selection, amount)?;
        let context = snapshot
            .context
            .ok_or("No verified world admin authority.")?;
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or("Admin request sequence exhausted.")?;
        let id = AdminRequestId {
            context,
            sequence: self.sequence,
        };
        self.backend
            .as_ref()
            .ok_or("Admin backend unavailable.")?
            .submit(AdminRequest {
                id: id.clone(),
                catalog_revision: snapshot.revision,
                operation,
            })?;
        self.message_context = Some(id.context.clone());
        self.pending = Some(id);
        self.message = "Pending authoritative result. No change confirmed yet.".into();
        Ok(())
    }
}

// Reject oversized/malformed replacement catalogs without unbounded content comparisons.
// A stable Rc is immutable and needs no scan; changed content is checked even if the host
// forgot to advance its revision. The host remains responsible for bounded snapshot creation.
fn same_snapshot(a: &AdminSnapshot, b: &AdminSnapshot) -> bool {
    let same_catalog = |a: &Rc<[AdminCatalogEntry]>, b: &Rc<[AdminCatalogEntry]>| {
        Rc::ptr_eq(a, b)
            || (a.len() == b.len()
                && a.len() <= MAX_CATALOG_ENTRIES
                && a.iter().zip(b.iter()).all(|(a, b)| {
                    a.id == b.id
                        && a.label.len() <= MAX_LABEL_BYTES
                        && b.label.len() <= MAX_LABEL_BYTES
                        && a.label == b.label
                }))
    };
    a.context == b.context
        && a.revision == b.revision
        && a.identity == b.identity
        && a.status == b.status
        && a.availability == b.availability
        && a.level == b.level
        && a.xp_enabled == b.xp_enabled
        && a.location == b.location
        && a.weather_automatic_id == b.weather_automatic_id
        && same_catalog(&a.items, &b.items)
        && same_catalog(&a.mounts, &b.mounts)
        && same_catalog(&a.cities, &b.cities)
        && same_catalog(&a.factions, &b.factions)
        && same_catalog(&a.weather, &b.weather)
}

fn number(text: &str, min: u32, max: u32) -> Result<u32, String> {
    if text.is_empty() || text.len() > 10 || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("Enter a whole number from {min} to {max}."));
    }
    text.parse::<u32>()
        .ok()
        .filter(|n| (min..=max).contains(n))
        .ok_or_else(|| format!("Enter a whole number from {min} to {max}."))
}

fn validate_operation(
    snapshot: &AdminSnapshot,
    action: AdminAction,
    selection: &str,
    amount: &str,
) -> Result<AdminOperation, String> {
    let entry = || -> Result<u32, String> {
        let id = number(
            selection,
            if action == AdminAction::Weather { 0 } else { 1 },
            u32::MAX,
        )?;
        let mut matches = snapshot.catalog(action).iter().filter(|row| row.id == id);
        let first = matches
            .next()
            .ok_or("Choose an entry from this world's supported catalog.")?;
        if matches.next().is_some()
            || first.label.len() > MAX_LABEL_BYTES
            || first.label.trim().is_empty()
        {
            return Err("The supported catalog entry is invalid or ambiguous.".into());
        }
        Ok(id)
    };
    Ok(match action {
        AdminAction::Item => AdminOperation::GrantItem {
            item_id: entry()?,
            count: number(amount, 1, MAX_ITEM_GRANT)?,
        },
        AdminAction::Gold => AdminOperation::GrantGold {
            copper: number(amount, 1, MAX_GOLD_GRANT)? * 10_000,
        },
        AdminAction::Mount => AdminOperation::GrantMount { mount_id: entry()? },
        AdminAction::Speed => AdminOperation::SetMovementSpeed {
            basis_points: number(amount, MIN_SPEED_PERCENT, MAX_SPEED_PERCENT)? * 100,
        },
        AdminAction::Teleport => AdminOperation::Teleport { city_id: entry()? },
        AdminAction::Level => AdminOperation::GainOneLevel,
        AdminAction::Xp => AdminOperation::SetXpGain {
            enabled: match amount {
                "on" => true,
                "off" => false,
                _ => return Err("Choose XP gain On or Off.".into()),
            },
        },
        AdminAction::Reputation => {
            let (sign, digits) = if let Some(digits) = amount.strip_prefix('-') {
                (-1, digits)
            } else {
                (1, amount.strip_prefix('+').unwrap_or(amount))
            };
            AdminOperation::AdjustReputation {
                faction_id: entry()?,
                delta: sign * number(digits, 1, MAX_REPUTATION_DELTA)? as i32,
            }
        }
        AdminAction::Honor => AdminOperation::GrantHonorContribution {
            points: number(amount, 1, MAX_HONOR_CONTRIBUTION)?,
        },
        AdminAction::Time => {
            if amount == "auto" {
                AdminOperation::RestoreAutomaticWorldTime
            } else {
                let (hour, minute) = amount
                    .split_once(':')
                    .ok_or("Enter HH:MM (00:00-23:59), or auto.")?;
                if hour.len() != 2 || minute.len() != 2 {
                    return Err("Enter HH:MM (00:00-23:59), or auto.".into());
                }
                AdminOperation::SetWorldTime {
                    minute_of_day: (number(hour, 0, 23)? * 60 + number(minute, 0, 59)?) as u16,
                }
            }
        }
        AdminAction::Weather => {
            let location = snapshot
                .location
                .clone()
                .ok_or("No current location authority.")?;
            let option_id = entry()?;
            if snapshot.weather_automatic_id == Some(option_id) {
                AdminOperation::RestoreAutomaticWeather { location }
            } else {
                AdminOperation::SetLocationWeather {
                    location,
                    option_id,
                }
            }
        }
    })
}

impl UiScript {
    /// Opt-in fork bindings: no extra globals in the stock engine or Glue VM.
    pub fn install_admin_interface(&self) -> mlua::Result<()> {
        if self.lua.app_data_ref::<AdminUi>().is_some() {
            return Ok(());
        }
        self.lua.set_app_data(AdminUi::default());
        self.lua.globals().set(
            "BenillaAdminGetView",
            self.lua.create_function(|lua, ()| {
                let mut ui = lua
                    .app_data_mut::<AdminUi>()
                    .expect("admin interface installed");
                let (snapshot, _) = ui.refresh();
                let view = lua.create_table()?;
                view.set("identity", snapshot.identity.as_str())?;
                view.set("status", snapshot.status.as_str())?;
                view.set("message", ui.message.as_str())?;
                view.set("pending", ui.pending.is_some())?;
                view.set("generation", ui.generation.to_string())?;
                view.set("xp_enabled", snapshot.xp_enabled)?;
                if let Some(level) = &snapshot.level {
                    view.set("current_level", level.current)?;
                    view.set("max_level", level.maximum)?;
                }
                for action in AdminAction::ALL {
                    let row = lua.create_table()?;
                    let reason = ui.reasons[action as usize].as_deref();
                    row.set("enabled", reason.is_none() && ui.pending.is_none())?;
                    row.set("reason", reason.unwrap_or(""))?;
                    // Compatibility preview only. The menu uses the bounded search binding;
                    // never copy the complete item catalog into Lua on a periodic refresh.
                    row.set(
                        "entries",
                        catalog_table(
                            lua,
                            snapshot
                                .catalog(action)
                                .iter()
                                .take(PAGE_SIZE)
                                .filter(|entry| entry.label.len() <= MAX_LABEL_BYTES),
                        )?,
                    )?;
                    row.set("catalog_count", snapshot.catalog(action).len())?;
                    view.set(action.key(), row)?;
                }
                Ok(view)
            })?,
        )?;
        self.lua.globals().set(
            "BenillaAdminSearch",
            self.lua.create_function(
                |lua, (action, query, requested): (String, mlua::String, i64)| {
                    let mut ui = lua
                        .app_data_mut::<AdminUi>()
                        .expect("admin interface installed");
                    let (snapshot, _) = ui.refresh();
                    let result = lua.create_table()?;
                    result.set("generation", ui.generation.to_string())?;
                    result.set("page_size", PAGE_SIZE)?;
                    let action = AdminAction::parse(&action);
                    let Some(action) = action.filter(|action| {
                        matches!(
                            action,
                            AdminAction::Item
                                | AdminAction::Mount
                                | AdminAction::Teleport
                                | AdminAction::Reputation
                                | AdminAction::Weather
                        )
                    }) else {
                        result.set("entries", lua.create_table()?)?;
                        result.set("reason", "This operation has no browsable catalog.")?;
                        result.set("total", 0)?;
                        result.set("pages", 0)?;
                        result.set("page", 0)?;
                        return Ok(result);
                    };
                    let search = ui.catalogs[action as usize].get_or_insert_with(|| {
                        CatalogSearch::new(action, snapshot.catalog(action))
                    });
                    if let Err(reason) = search.filter(&query.to_str()?) {
                        result.set("entries", lua.create_table()?)?;
                        result.set("reason", reason)?;
                        result.set("total", 0)?;
                        result.set("pages", 0)?;
                        result.set("page", 0)?;
                    } else {
                        let page = search.page(requested);
                        result.set(
                            "entries",
                            catalog_table(
                                lua,
                                search
                                    .page_indices(page)
                                    .map(|index| &snapshot.catalog(action)[index]),
                            )?,
                        )?;
                        result.set("reason", "")?;
                        result.set("total", search.total())?;
                        result.set("pages", search.pages())?;
                        result.set("page", if search.total() == 0 { 0 } else { page })?;
                    }
                    Ok(result)
                },
            )?,
        )?;
        self.lua.globals().set(
            "BenillaAdminSubmit",
            self.lua.create_function(
                |lua,
                 (action, selection, amount, generation): (
                    String,
                    String,
                    String,
                    Option<String>,
                )| {
                    let mut ui = lua
                        .app_data_mut::<AdminUi>()
                        .expect("admin interface installed");
                    let result = ui.submit(&action, &selection, &amount, generation.as_deref());
                    if let Err(reason) = &result {
                        ui.message = reason.clone();
                        ui.message_context = ui.snapshot().context;
                    }
                    Ok(result.is_ok())
                },
            )?,
        )?;
        Ok(())
    }

    /// Attachment is host code only, not a Lua setting. Never call this based on PSK possession.
    /// Detaching/replacing preserves a pending request latch: its unknown result is not success.
    pub fn set_admin_backend(&self, backend: Option<Rc<dyn AdminBackend>>) -> mlua::Result<()> {
        self.install_admin_interface()?;
        let mut ui = self
            .lua
            .app_data_mut::<AdminUi>()
            .expect("admin interface installed");
        ui.backend = backend;
        ui.shown = None;
        if ui.pending.is_none() {
            ui.message.clear();
            ui.message_context = None;
        }
        Ok(())
    }
}

fn catalog_table<'a>(
    lua: &Lua,
    entries: impl Iterator<Item = &'a AdminCatalogEntry>,
) -> mlua::Result<Table> {
    let rows = lua.create_table()?;
    for (index, entry) in entries.enumerate() {
        let row = lua.create_table()?;
        row.set("id", entry.id.to_string())?;
        row.set("label", entry.label.as_str())?;
        let mut display: String = entry.label.chars().take(44).collect();
        if entry.label.chars().nth(44).is_some() {
            display.push_str("...");
        }
        row.set("display_label", display)?;
        rows.set(index + 1, row)?;
    }
    Ok(rows)
}
