use super::*;
use benilla_ui::script::UiScript;
use std::fs;
struct Fixture(std::path::PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn profile() -> (Fixture, ProfileStore) {
    let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
        "benilla-setup-host-fixture-{}",
        hex(saved::new_request_session().unwrap())
    ));
    fs::create_dir(&path).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let store = ProfileStore::create(path.join("profile")).unwrap();
    (Fixture(path), store)
}
fn script(mailbox: &ui::WorldSetupMailbox) -> UiScript {
    let script = UiScript::new().unwrap();
    script.install_admin_interface().unwrap();
    script.set_world_setup_mailbox(mailbox.clone()).unwrap();
    script
}
fn create(script: &UiScript) -> bool {
    script.eval("local v=BenillaWorldSetupGetView(); return BenillaWorldSetupCreate('Fixture world',string.rep('42',16),v.context,v.revision)").unwrap()
}
#[test]
fn actual_lua_mailbox_profile_save_and_reopen_preserve_exploration() {
    let (fixture, mut profile) = profile();
    let before = fs::read(fixture.0.join("profile/profile.json")).unwrap();
    let mailbox = ui::WorldSetupMailbox::default();
    let script = script(&mailbox);
    let mut service = Service::open(&mut profile, mailbox.clone());
    assert!(create(&script));
    assert!(profile.world_setups().unwrap().worlds.is_empty());
    service.poll(&mut profile);
    let world = profile.world_setups().unwrap().worlds[0].clone();
    assert_eq!(world.policy.world_owner, profile.owner_id());
    assert_eq!(world.policy.allowlist, [[0x42; 16]].into());
    assert!(!script
        .eval::<bool>("return BenillaWorldSetupGetView().pending")
        .unwrap());
    assert!(script
        .eval::<String>("return BenillaWorldSetupGetView().message")
        .unwrap()
        .starts_with("Saved and selected"));
    assert!(!script
        .eval::<bool>("return BenillaAdminGetView().level.enabled")
        .unwrap());
    assert_eq!(
        fs::read(fixture.0.join("profile/profile.json")).unwrap(),
        before
    );
    let context = script
        .eval::<String>("return BenillaWorldSetupGetView().context")
        .unwrap();
    drop(service);
    drop(profile);
    let mut reopened = ProfileStore::open(fixture.0.join("profile")).unwrap();
    let _next = Service::open(&mut reopened, mailbox);
    assert_ne!(
        context,
        script
            .eval::<String>("return BenillaWorldSetupGetView().context")
            .unwrap()
    );
    assert_eq!(
        script
            .eval::<String>("return BenillaWorldSetupGetView().selected_id")
            .unwrap(),
        hex(world.policy.world)
    );
}
#[test]
fn close_before_processing_and_stale_form_cannot_create_another_world() {
    let (_fixture, mut profile) = profile();
    let mailbox = ui::WorldSetupMailbox::default();
    let script = script(&mailbox);
    let service = Service::open(&mut profile, mailbox.clone());
    script.run("old=BenillaWorldSetupGetView()").unwrap();
    assert!(create(&script));
    drop(service);
    let mut next = Service::open(&mut profile, mailbox);
    next.poll(&mut profile);
    assert!(profile.world_setups().unwrap().worlds.is_empty());
    assert!(!script
        .eval::<bool>("return BenillaWorldSetupCreate('Lost form','',old.context,old.revision)")
        .unwrap());
    assert!(create(&script));
    next.poll(&mut profile);
    assert_eq!(profile.world_setups().unwrap().worlds.len(), 1);
}
#[test]
fn corrupt_sidecar_disables_setup_without_replacing_profile_or_enabling_grants() {
    let (fixture, mut profile) = profile();
    let file = fixture.0.join("profile/local-worlds.json");
    fs::write(&file, b"corrupt").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let mailbox = ui::WorldSetupMailbox::default();
    let script = script(&mailbox);
    let mut service = Service::open(&mut profile, mailbox);
    assert!(!script
        .eval::<bool>("return BenillaWorldSetupGetView().available")
        .unwrap());
    assert!(!create(&script));
    service.poll(&mut profile);
    assert_eq!(fs::read(file).unwrap(), b"corrupt");
    assert!(!script
        .eval::<bool>("return BenillaAdminGetView().level.enabled")
        .unwrap());
}
#[test]
fn production_entry_hooks_are_exploration_only_and_do_not_admit_native_worlds() {
    let p2p = include_str!("../../p2p.rs");
    let exploration = p2p.split("fn run_exploration_character(").nth(1).unwrap();
    assert!(exploration.contains("world_setup::Service::open(store, ticket.world_setup())"));
    assert!(exploration.contains("setup.poll(store)"));
    let source = include_str!("../world_setup.rs");
    assert!(!source.contains("NativeCapability"));
    assert!(!source.contains("set_admin_backend"));
}
