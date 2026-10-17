//! Fork-only reload of saved P2P settings. A retiring worker retains its profile lock until
//! its checkpoint completes; no replacement transport exists before that result is known.
use super::*;
use crate::char_select::ClientState;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum ApplyStatus {
    #[default]
    Idle,
    Pending,
    NeedsDisconnect,
    Applied,
    RestartRequired(String),
    Failed(String),
}

#[derive(Default)]
enum Phase {
    #[default]
    Requested,
    Retiring,
    ReturningToLogin,
}

#[derive(Resource, Default)]
pub(crate) struct NetworkReconfigure {
    pub(crate) status: ApplyStatus,
    desired: Option<benilla_config::AppConfig>,
    confirmed: bool,
    needs_teardown: bool,
    phase: Phase,
}
impl NetworkReconfigure {
    /// `config` must be the validated result of ConfigEditor::save, never a draft.
    pub(crate) fn request(&mut self, config: benilla_config::AppConfig, confirm_disconnect: bool) {
        // Never replace the target or revoke consent halfway through retiring a session.
        if !matches!(self.phase, Phase::Requested) {
            return;
        }
        self.desired = Some(config);
        self.confirmed = confirm_disconnect;
        self.status = ApplyStatus::Pending;
    }
    pub(crate) fn confirm_disconnect(&mut self) {
        if self.status == ApplyStatus::NeedsDisconnect {
            self.confirmed = true;
            self.status = ApplyStatus::Pending;
        }
    }
    pub(crate) fn cancel_pending(&mut self) {
        if matches!(self.phase, Phase::Requested) {
            self.desired = None;
            self.status = ApplyStatus::Idle;
            self.confirmed = false;
        }
    }
    fn fail(&mut self, status: ApplyStatus) {
        self.status = status;
        self.desired = None;
        self.phase = Phase::Requested;
        self.confirmed = false;
        self.needs_teardown = false;
    }
}

fn idle(world: &mut World) -> bool {
    let has_avatar = world
        .query_filtered::<Entity, With<SelfPlayer>>()
        .iter(world)
        .next()
        .is_some();
    !has_avatar
        && world
            .get_resource::<State<ClientState>>()
            .is_some_and(|state| *state.get() == ClientState::Login)
        && world
            .get_resource::<SelfGuid>()
            .is_none_or(|guid| guid.0.is_none())
        && world
            .get_resource::<NetStatus>()
            .is_none_or(|status| !status.connected)
        && world
            .get_resource::<P2pStart>()
            .is_some_and(P2pStart::can_start)
}

/// Explicit comparison keeps secrets in memory and includes fields omitted by Debug.
fn same_settings(current: &P2pSettings, desired: &benilla_config::AppConfig) -> bool {
    let Some(b) = desired.p2p.as_ref() else {
        return false;
    };
    let a = &current.0;
    let internet = match (&a.internet, &b.internet) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            a.relay_urls == b.relay_urls
                && a.peer_tickets == b.peer_tickets
                && a.port_mapping == b.port_mapping
                && a.dynamic_candidates == b.dynamic_candidates
        }
        _ => false,
    };
    let encounter = match (&a.encounter, &b.encounter) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            a.local_id == b.local_id
                && a.epoch == b.epoch
                && a.tick_limit == b.tick_limit
                && a.roster.len() == b.roster.len()
                && a.roster
                    .iter()
                    .zip(&b.roster)
                    .all(|(a, b)| a.id == b.id && a.address == b.address)
        }
        _ => false,
    };
    current.1 == desired.ruleset
        && a.psk_bytes() == b.psk_bytes()
        && a.classic_start_captures == b.classic_start_captures
        && a.network == b.network
        && a.timezone == b.timezone
        && a.listen == b.listen
        && a.peers == b.peers
        && a.max_peers == b.max_peers
        && a.timeout_ms == b.timeout_ms
        && internet
        && encounter
}

pub(super) fn drive(world: &mut World) {
    world.resource_scope(|world, mut reload: Mut<NetworkReconfigure>| {
        if reload.desired.is_none() {
            return;
        }
        match reload.phase {
            Phase::Requested => {
                if world.contains_resource::<NetOffline>()
                    || world.contains_resource::<crate::run_mode::CaptureMode>()
                    || world.contains_resource::<crate::login::settings::ConfigRecovery>()
                    || crate::run_mode::unattended()
                {
                    reload.fail(ApplyStatus::RestartRequired(
                        "This offline, recovery, or unattended run cannot reload networking. Restart to use the saved settings.".into(),
                    ));
                    return;
                }
                if !world.contains_resource::<P2pStart>()
                    || reload.desired.as_ref().is_none_or(|config| config.p2p.is_none())
                {
                    reload.fail(ApplyStatus::RestartRequired(
                        "Switching between realm and P2P backends requires restarting the game.".into(),
                    ));
                    return;
                }
                if let Some(error) = world.resource::<P2pStart>().retirement_failure() {
                    reload.fail(ApplyStatus::Failed(error));
                    return;
                }
                if world.get_resource::<P2pSettings>().is_some_and(|current|
                    same_settings(current, reload.desired.as_ref().expect("saved configuration"))) {
                    reload.fail(ApplyStatus::Applied);
                    return;
                }
                let active = !idle(world);
                if active && !reload.confirmed {
                    reload.status = ApplyStatus::NeedsDisconnect;
                    return;
                }
                reload.needs_teardown = active;
                // Last runs after this frame's final movement production. Entering Login at
                // the next state transition prevents further InWorld input while checkpointing.
                if let Some(mut next) = world.get_resource_mut::<NextState<ClientState>>() {
                    next.set(ClientState::Login);
                }
                world.resource_mut::<P2pStart>().begin_retirement();
                reload.phase = Phase::Retiring;
                reload.status = ApplyStatus::Pending;
            }
            Phase::Retiring => {
                // Old in-flight world-entry messages may request InWorld; hold Login until
                // retirement and teardown finish so those messages cannot re-enable input.
                if let Some(mut next) = world.get_resource_mut::<NextState<ClientState>>() {
                    next.set(ClientState::Login);
                }
                let Some(result) = world.resource_mut::<P2pStart>().finish_retirement() else {
                    return;
                };
                world.resource::<P2pEndpointTicket>().clear();
                // Queued old roster/world events must never populate the replacement session.
                world.resource::<NetEvents>().0.try_iter().for_each(drop);
                if let Some(mut intent) = world.get_resource_mut::<crate::login::LoginIntent>() {
                    intent.clear();
                }
                // Use the established teardown handlers, including all session-scoped caches.
                if reload.needs_teardown
                    || world.get_resource::<NetStatus>().is_some_and(|status| status.connected)
                    || world.get_resource::<SelfGuid>().is_some_and(|guid| guid.0.is_some())
                    || world.get_resource::<State<ClientState>>().is_some_and(|state| *state.get() != ClientState::Login)
                {
                    handlers::dispatch(world, vec![
                        SessionEvent::Disconnected {
                            reason: "P2P connection closed for saved network settings".into(),
                            end: benilla_protocol::SessionEnd::Lost,
                        },
                    ]);
                }
                if let Some(mut next) = world.get_resource_mut::<NextState<ClientState>>() {
                    next.set(ClientState::Login);
                }
                if let Err(error) = result {
                    reload.fail(ApplyStatus::Failed(error));
                    return;
                }
                reload.phase = Phase::ReturningToLogin;
            }
            Phase::ReturningToLogin => {
                if world.get_resource::<State<ClientState>>().is_none_or(|state| *state.get() != ClientState::Login)
                    || world.get_resource::<SelfGuid>().is_some_and(|guid| guid.0.is_some())
                {
                    return;
                }
                let config = reload.desired.take().expect("pending saved configuration");
                let settings = P2pSettings(config.p2p.expect("P2P target checked before retirement"), config.ruleset);
                let ticket = world.resource::<P2pEndpointTicket>().clone();
                let (handles, start) = p2p::prepare(settings.clone(), ticket);
                world.insert_resource(NetEvents(handles.events));
                world.insert_resource(NetCommands(handles.commands));
                world.insert_resource(CharPick(handles.pick));
                world.insert_resource(RealmChoice(handles.realm));
                world.insert_resource(RosterPolicy(handles.screen_asks_roster));
                world.insert_resource(LoginSubmit(handles.login));
                world.insert_resource(LoginAbandon(handles.login_abandon));
                world.insert_resource(PingShared(handles.ping));
                world.insert_resource(settings);
                world.insert_resource(start);
                reload.fail(ApplyStatus::Applied);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> benilla_config::AppConfig {
        benilla_config::AppConfig::parse(
            "[p2p]\npsk = \"public reload test phrase only\"\nlisten = \"127.0.0.1:7474\"\n",
            std::path::Path::new("/tmp"),
        )
        .unwrap()
    }

    #[test]
    fn equality_ignores_asset_path_but_detects_network_and_secret_changes() {
        let mut desired = fixture();
        let current = P2pSettings(desired.p2p.clone().unwrap(), desired.ruleset);
        desired.wow.data = Some("/different/assets".into());
        assert!(same_settings(&current, &desired));
        desired.p2p.as_mut().unwrap().network.push_str("-different");
        assert!(!same_settings(&current, &desired));
        let desired = benilla_config::AppConfig::parse(
            "[p2p]\npsk = \"another public test phrase\"\nlisten = \"127.0.0.1:7474\"\n",
            std::path::Path::new("/tmp"),
        )
        .unwrap();
        assert!(!same_settings(&current, &desired));
    }

    #[test]
    fn idle_apply_replaces_channels_with_an_inert_start() {
        let mut world = World::new();
        let config = fixture();
        let settings = P2pSettings(config.p2p.clone().unwrap(), config.ruleset);
        let ticket = P2pEndpointTicket::default();
        let (handles, start) = p2p::prepare(settings.clone(), ticket.clone());
        world.insert_resource(settings);
        world.insert_resource(start);
        world.insert_resource(ticket);
        world.insert_resource(NetEvents(handles.events));
        world.insert_resource(NetStatus::default());
        world.insert_resource(SelfGuid::default());
        world.insert_resource(State::new(ClientState::Login));
        let mut reload = NetworkReconfigure::default();
        let mut changed = config;
        changed.p2p.as_mut().unwrap().timeout_ms += 1;
        reload.request(changed, false);
        world.insert_resource(reload);
        drive(&mut world);
        drive(&mut world);
        drive(&mut world);
        assert_eq!(
            world.resource::<NetworkReconfigure>().status,
            ApplyStatus::Applied
        );
        assert!(world.resource::<P2pStart>().can_start());
        assert!(handles.commands.send(ClientCommand::QueryTime).is_err());
        assert!(world.resource::<P2pEndpointTicket>().get().is_none());
    }

    #[test]
    fn changed_active_settings_require_consent_and_same_settings_do_not() {
        let mut world = World::new();
        let config = fixture();
        let settings = P2pSettings(config.p2p.clone().unwrap(), config.ruleset);
        let (handles, start) = p2p::prepare(settings.clone(), P2pEndpointTicket::default());
        world.insert_resource(settings);
        world.insert_resource(start);
        world.insert_resource(NetStatus {
            connected: true,
            last_reason: None,
        });
        world.insert_resource(State::new(ClientState::InWorld));
        let mut reload = NetworkReconfigure::default();
        reload.request(config.clone(), false);
        world.insert_resource(reload);
        drive(&mut world);
        assert_eq!(
            world.resource::<NetworkReconfigure>().status,
            ApplyStatus::Applied
        );
        assert!(world.resource::<P2pStart>().can_start());
        let mut changed = config;
        changed.p2p.as_mut().unwrap().timeout_ms += 1;
        world
            .resource_mut::<NetworkReconfigure>()
            .request(changed, false);
        drive(&mut world);
        assert_eq!(
            world.resource::<NetworkReconfigure>().status,
            ApplyStatus::NeedsDisconnect
        );
        assert!(world.resource::<P2pStart>().can_start());
        assert!(handles.commands.send(ClientCommand::QueryTime).is_ok());
        world.resource_mut::<NetworkReconfigure>().cancel_pending();
        drive(&mut world);
        assert_eq!(
            world.resource::<NetworkReconfigure>().status,
            ApplyStatus::Idle
        );
        assert!(world.resource::<P2pStart>().can_start());
    }

    #[test]
    fn offline_process_cannot_be_promoted_to_networking() {
        let mut world = World::new();
        world.insert_resource(NetOffline);
        let mut reload = NetworkReconfigure::default();
        reload.request(fixture(), true);
        world.insert_resource(reload);
        drive(&mut world);
        assert!(matches!(
            world.resource::<NetworkReconfigure>().status,
            ApplyStatus::RestartRequired(_)
        ));
        assert!(!world.contains_resource::<P2pStart>());
    }

    #[test]
    fn confirmation_and_cancel_do_not_mutate_saved_configuration() {
        let mut reload = NetworkReconfigure::default();
        reload.request(benilla_config::AppConfig::default(), false);
        reload.status = ApplyStatus::NeedsDisconnect;
        reload.confirm_disconnect();
        assert!(reload.confirmed);
        assert_eq!(reload.status, ApplyStatus::Pending);
        reload.cancel_pending();
        assert!(reload.desired.is_none());
        assert_eq!(reload.status, ApplyStatus::Idle);
    }

    #[test]
    fn retiring_target_cannot_be_replaced_or_cancelled() {
        let mut reload = NetworkReconfigure::default();
        reload.request(benilla_config::AppConfig::default(), true);
        reload.phase = Phase::Retiring;
        reload.cancel_pending();
        reload.request(benilla_config::AppConfig::default(), false);
        assert!(reload.desired.is_some());
        assert!(reload.confirmed);
        assert!(matches!(reload.phase, Phase::Retiring));
    }
}
