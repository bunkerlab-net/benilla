use super::*;
fn view() -> WorldSetupView {
    WorldSetupView {
        available: true,
        context: "11".repeat(16),
        owner_id: "22".repeat(16),
        status: "Saved setup only; gameplay unavailable.".into(),
        ..WorldSetupView::default()
    }
}
fn ui(mailbox: &WorldSetupMailbox) -> UiScript {
    let ui = UiScript::new().unwrap();
    ui.install_admin_interface().unwrap();
    ui.set_world_setup_mailbox(mailbox.clone()).unwrap();
    ui
}
fn create_call(ui: &UiScript) -> bool {
    ui.eval("return BenillaWorldSetupCreate('Test World', '', string.rep('11',16), '0')")
        .unwrap()
}
#[test]
fn default_unavailable_and_setup_never_grants_admin() {
    let mailbox = WorldSetupMailbox::default();
    let ui = ui(&mailbox);
    assert!(!create_call(&ui));
    assert!(mailbox.take_request().is_none());
    mailbox.activate(view());
    assert!(create_call(&ui));
    assert!(!ui
        .eval::<bool>("return BenillaAdminGetView().level.enabled")
        .unwrap());
}
#[test]
fn pending_survives_reload_and_host_receipt_controls_success() {
    let mailbox = WorldSetupMailbox::default();
    mailbox.activate(view());
    let first = ui(&mailbox);
    assert!(create_call(&first));
    assert!(!create_call(&first));
    let request = mailbox.take_request().unwrap();
    assert_eq!(request.sequence, 1);
    assert!(mailbox.take_request().is_none());
    let second = ui(&mailbox);
    assert!(second
        .eval::<bool>("return BenillaWorldSetupGetView().pending")
        .unwrap());
    assert!(!create_call(&second));
    let mut next = view();
    next.revision = 1;
    mailbox.finish(&request, next, "Saved setup only.".into(), false);
    assert!(!second
        .eval::<bool>("return BenillaWorldSetupGetView().pending")
        .unwrap());
    assert!(!create_call(&second));
    assert!(second
        .eval::<bool>("return BenillaWorldSetupCreate('Next', '', string.rep('11',16), '1')")
        .unwrap());
    assert_eq!(mailbox.take_request().unwrap().sequence, 2);
}
#[test]
fn changed_profile_and_stale_revision_reject_typed_forms() {
    let mailbox = WorldSetupMailbox::default();
    mailbox.activate(view());
    let ui = ui(&mailbox);
    let mut next = view();
    next.context = "33".repeat(16);
    mailbox.activate(next);
    assert!(!create_call(&ui));
    assert!(mailbox.take_request().is_none());
    assert!(!ui
        .eval::<bool>("return BenillaWorldSetupCreate('World', '', string.rep('33',16), '01')")
        .unwrap());
}
#[test]
fn uncertain_save_latches_and_late_result_cannot_cross_context() {
    let mailbox = WorldSetupMailbox::default();
    mailbox.activate(view());
    let ui = ui(&mailbox);
    assert!(create_call(&ui));
    let request = mailbox.take_request().unwrap();
    let mut blocked = view();
    blocked.available = false;
    mailbox.finish(
        &request,
        blocked,
        "Durability unknown; reopen and inspect.".into(),
        true,
    );
    assert!(ui
        .eval::<bool>("return BenillaWorldSetupGetView().pending")
        .unwrap());
    assert!(!create_call(&ui));
    let mut next = view();
    next.context = "33".repeat(16);
    mailbox.activate(next.clone());
    mailbox.finish(&request, view(), "Stale success".into(), false);
    assert_eq!(
        ui.eval::<String>("return BenillaWorldSetupGetView().context")
            .unwrap(),
        next.context
    );
}
#[test]
fn names_allowlists_and_foreign_selection_are_bounded() {
    let mailbox = WorldSetupMailbox::default();
    mailbox.activate(view());
    let ui = ui(&mailbox);
    for call in [
        "BenillaWorldSetupCreate('', '', string.rep('11',16), '0')",
        "BenillaWorldSetupCreate(string.rep('a',41), '', string.rep('11',16), '0')",
        "BenillaWorldSetupCreate('World', string.rep('22',16), string.rep('11',16), '0')",
        "BenillaWorldSetupCreate('World', string.rep('00',16), string.rep('11',16), '0')",
        "BenillaWorldSetupCreate('World', string.rep('aa',16)..','..string.rep('AA',16), string.rep('11',16), '0')",
        "BenillaWorldSetupSelect(string.rep('44',16), string.rep('11',16), '0')",
    ] { assert!(!ui.eval::<bool>(&format!("return {call}")).unwrap(), "{call}"); }
    assert!(mailbox.take_request().is_none());
    assert!(ui.eval::<bool>("return BenillaWorldSetupCreate(' World ', string.rep('AA',16), string.rep('11',16), '0')").unwrap());
    assert_eq!(
        mailbox.take_request().unwrap().operation,
        WorldSetupOperation::Create {
            label: "World".into(),
            allowlist: vec!["aa".repeat(16)]
        }
    );
}
#[test]
fn close_retains_uncertain_request_and_new_session_explains_review() {
    let mailbox = WorldSetupMailbox::default();
    mailbox.activate(view());
    let ui = ui(&mailbox);
    assert!(create_call(&ui));
    mailbox.close(&view().context);
    assert!(!create_call(&ui));
    assert!(ui
        .eval::<bool>("return BenillaWorldSetupGetView().pending")
        .unwrap());
    mailbox.activate(view());
    assert!(ui
        .eval::<String>("return BenillaWorldSetupGetView().message")
        .unwrap()
        .contains("previous"));
}
