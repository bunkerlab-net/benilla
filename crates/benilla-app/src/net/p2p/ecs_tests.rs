use super::tests::*;
use super::*;
#[test]
fn peer_plugin_prepares_without_entering_and_never_arms_capture_or_recovery() {
    use super::super::{NetEvents, NetOffline, NetPlugin, NetStatus, SelfGuid};
    use bevy::prelude::App;

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let settings = settings(listener.local_addr().unwrap(), &[]);
    let mut app = App::new();
    app.insert_resource(settings.clone());
    app.add_plugins(NetPlugin { connect: true });
    assert!(app.world().resource::<P2pStart>().can_start());
    assert!(!app.world().resource::<NetStatus>().connected);
    assert!(app.world().resource::<SelfGuid>().0.is_none());
    assert!(matches!(
        app.world().resource::<NetEvents>().0.try_recv(),
        Err(crossbeam_channel::TryRecvError::Empty)
    ));

    for recovery in [false, true] {
        let mut offline = App::new();
        offline.insert_resource(settings.clone());
        if recovery {
            offline.insert_resource(crate::login::settings::ConfigRecovery("invalid".into()));
        }
        offline.add_plugins(NetPlugin { connect: recovery });
        assert!(offline.world().contains_resource::<NetOffline>());
        assert!(!offline.world().contains_resource::<P2pStart>());
    }
}

#[test]
fn realm_plugin_keeps_its_normal_login_channels_without_a_peer_gate() {
    use super::super::{LoginSubmit, NetOffline, NetPlugin};
    use bevy::prelude::App;

    let mut app = App::new();
    app.add_plugins(NetPlugin { connect: true });
    assert!(!app.world().contains_resource::<P2pStart>());
    assert!(!app.world().contains_resource::<NetOffline>());
    assert!(app.world().contains_resource::<LoginSubmit>());
}

#[test]
fn saved_characters_materialize_as_distinct_players_through_the_ecs_bridge() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let (tx, rx) = crossbeam_channel::unbounded();
    app.insert_resource(super::super::NetEvents(rx));
    let me = fixture(1, 9, "Localname");
    let other = fixture(2, 9, "Peername");
    tx.send(SessionEvent::Connected {
        self_guid: character_guid(&me),
        name: me.name.clone(),
        billing_time_rested: 0,
        tutorial_flags: None,
        addon_info: Some(Vec::new()),
    })
    .unwrap();
    tx.send(character_avatar(&me, &AvatarCatalog::fixture()).unwrap())
        .unwrap();
    tx.send(character_name(&other)).unwrap();
    tx.send(character_avatar(&other, &AvatarCatalog::fixture()).unwrap())
        .unwrap();
    super::super::apply_net_updates(app.world_mut());
    let index = app.world().resource::<super::super::GuidIndex>();
    assert!(index.0.contains_key(&character_guid(&me)));
    assert!(index.0.contains_key(&character_guid(&other)));
    assert_ne!(
        index.0[&character_guid(&me)],
        index.0[&character_guid(&other)]
    );
    assert_eq!(
        app.world().resource::<super::super::SelfGuid>().0,
        Some(character_guid(&me))
    );
}
