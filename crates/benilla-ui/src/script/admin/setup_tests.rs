use super::load_panel;
use crate::script::world_setup::{
    SavedWorld, WorldSetupMailbox, WorldSetupOperation, WorldSetupView,
};
use crate::script::UiScript;

fn view(count: u32) -> WorldSetupView {
    WorldSetupView {
        available: true,
        context: "11".repeat(16),
        owner_id: "22".repeat(16),
        status: "Saved setup only; gameplay and grants unavailable.".into(),
        worlds: (1..=count)
            .map(|id| SavedWorld {
                id: format!("{id:032x}"),
                label: format!("Fictional World {id}"),
                owner_id: "22".repeat(16),
                allowlist: vec!["33".repeat(16)],
            })
            .collect(),
        ..WorldSetupView::default()
    }
}

fn panel(mailbox: &WorldSetupMailbox) -> UiScript {
    let mut s = UiScript::new().unwrap();
    s.install_admin_interface().unwrap();
    s.set_world_setup_mailbox(mailbox.clone()).unwrap();
    load_panel(&mut s);
    s.run("ShowUIPanel(BenillaAdminFrame); BenillaAdminFrameWorldSetup:Click()")
        .unwrap();
    s
}

#[test]
fn saved_setup_create_requires_explicit_reviewed_click_and_never_grants_admin() {
    let mailbox = WorldSetupMailbox::default();
    mailbox.activate(view(0));
    let mut s = panel(&mailbox);
    assert!(!s.frame_visible("BenillaAdminFrame"));
    assert!(s.frame_visible("BenillaWorldSetupFrame"));
    s.run("BenillaWorldSetupFrameNewTab:Click(); BenillaWorldSetupFrameName:SetText(' My Test World '); BenillaWorldSetupFrameAdmins:SetText(string.rep('AB',16))").unwrap();
    s.tick(0.016);
    assert!(s
        .eval::<String>("return BenillaWorldSetupFrameReview:GetText()")
        .unwrap()
        .starts_with("Save and select a new local world"));
    assert!(s
        .eval::<String>("return BenillaWorldSetupFrameReview:GetText()")
        .unwrap()
        .contains("1 other admins"));
    assert!(s
        .eval::<String>("return BenillaWorldSetupFrameReview:GetText()")
        .unwrap()
        .contains(&"22".repeat(16)));
    assert!(s.focus_editbox("BenillaWorldSetupFrameAdmins"));
    s.key_down("ENTER");
    assert!(mailbox.take_request().is_none());
    s.run("BenillaWorldSetupFrameSave:Click(); BenillaWorldSetupFrameSave:Click()")
        .unwrap();
    let request = mailbox.take_request().unwrap();
    assert_eq!(
        request.operation,
        WorldSetupOperation::Create {
            label: "My Test World".into(),
            allowlist: vec!["ab".repeat(16)]
        }
    );
    assert!(mailbox.take_request().is_none());
    assert!(!s
        .eval::<bool>("return BenillaAdminGetView().level.enabled")
        .unwrap());
    assert!(!s
        .eval::<bool>("return BenillaWorldSetupFrameSave:IsEnabled() ~= 0")
        .unwrap());
    assert!(s
        .eval::<String>("return BenillaWorldSetupFrameDisclaimer:GetText()")
        .unwrap()
        .contains("Gameplay and grants remain unavailable"));
    assert!(s.errors().is_empty(), "{:?}", s.errors());
}

#[test]
fn setup_pending_survives_back_close_and_reopen_until_host_confirms_durable_save() {
    let mailbox = WorldSetupMailbox::default();
    mailbox.activate(view(0));
    let mut s = panel(&mailbox);
    s.run("BenillaWorldSetupFrameNewTab:Click(); BenillaWorldSetupFrameName:SetText('Test'); BenillaWorldSetup_Save()").unwrap();
    let request = mailbox.take_request().unwrap();
    s.run("BenillaWorldSetupFrameBack:Click(); BenillaAdminFrameWorldSetup:Click(); BenillaWorldSetupFrameNewTab:Click()").unwrap();
    assert!(!s
        .eval::<bool>("return BenillaWorldSetupFrameSave:IsEnabled() ~= 0")
        .unwrap());
    s.run("BenillaWorldSetupFrameDone:Click(); ShowUIPanel(BenillaWorldSetupFrame)")
        .unwrap();
    assert!(s
        .eval::<bool>("return BenillaWorldSetupGetView().pending")
        .unwrap());
    let mut committed = view(1);
    committed.revision = 1;
    committed.selected_id = Some(format!("{:032x}", 1));
    mailbox.finish(
        &request,
        committed,
        "Saved setup; gameplay and grants unavailable until this world can start.".into(),
        false,
    );
    s.tick(0.3);
    assert!(!s
        .eval::<bool>("return BenillaWorldSetupGetView().pending")
        .unwrap());
    assert!(s
        .eval::<String>("return BenillaWorldSetupFrameResult:GetText()")
        .unwrap()
        .starts_with("Saved setup;"));
    assert!(s
        .eval::<String>("return BenillaWorldSetupRow1:GetText()")
        .unwrap()
        .contains("saved selection"));
    assert!(!s
        .eval::<bool>("return BenillaAdminGetView().item.enabled")
        .unwrap());
    assert!(s.errors().is_empty(), "{:?}", s.errors());
}

#[test]
fn saved_world_pages_and_selection_are_bounded_explicit_and_setup_only() {
    let mailbox = WorldSetupMailbox::default();
    mailbox.activate(view(16));
    let s = panel(&mailbox);
    assert!(s
        .eval::<String>("return BenillaWorldSetupFramePageCount:GetText()")
        .unwrap()
        .contains("1 / 4"));
    assert!(!s
        .eval::<bool>("return BenillaWorldSetupFrameUse:IsEnabled() ~= 0")
        .unwrap());
    s.run("BenillaWorldSetupFrameNext:Click(); BenillaWorldSetupFrameNext:Click(); BenillaWorldSetupFrameNext:Click()").unwrap();
    assert!(!s
        .eval::<bool>("return BenillaWorldSetupFrameNext:IsEnabled() ~= 0")
        .unwrap());
    assert!(!s.frame_visible("BenillaWorldSetupRow2"));
    s.run("BenillaWorldSetupRow1:Click()").unwrap();
    assert!(mailbox.take_request().is_none());
    s.run("BenillaWorldSetupFrameUse:Click(); BenillaWorldSetupFrameUse:Click()")
        .unwrap();
    let request = mailbox.take_request().unwrap();
    assert_eq!(
        request.operation,
        WorldSetupOperation::Select {
            world_id: format!("{:032x}", 16)
        }
    );
    assert!(mailbox.take_request().is_none());
    assert!(s.errors().is_empty(), "{:?}", s.errors());
}

#[test]
fn invalid_setup_forms_and_save_limit_have_reasons_and_never_queue() {
    for text in [
        "not-an-owner-id",
        &"00".repeat(16),
        &"22".repeat(16),
        &format!("{},{}", "33".repeat(16), "33".repeat(16)),
    ] {
        let mailbox = WorldSetupMailbox::default();
        mailbox.activate(view(0));
        let mut s = panel(&mailbox);
        s.lua.globals().set("badAdmins", text).unwrap();
        s.run("BenillaWorldSetupFrameNewTab:Click(); BenillaWorldSetupFrameName:SetText('Test'); BenillaWorldSetupFrameAdmins:SetText(badAdmins)").unwrap();
        s.tick(0.016);
        assert!(!s
            .eval::<bool>("return BenillaWorldSetupFrameSave:IsEnabled() ~= 0")
            .unwrap());
        assert!(!s
            .eval::<String>("return BenillaWorldSetupFrameReason:GetText()")
            .unwrap()
            .is_empty());
        s.run("BenillaWorldSetup_Save()").unwrap();
        assert!(mailbox.take_request().is_none());
    }
    let mailbox = WorldSetupMailbox::default();
    mailbox.activate(view(16));
    let mut s = panel(&mailbox);
    s.run(
        "BenillaWorldSetupFrameNewTab:Click(); BenillaWorldSetupFrameName:SetText('Extra World')",
    )
    .unwrap();
    s.tick(0.016);
    assert!(!s
        .eval::<bool>("return BenillaWorldSetupFrameSave:IsEnabled() ~= 0")
        .unwrap());
    assert!(s
        .eval::<String>("return BenillaWorldSetupFrameReason:GetText()")
        .unwrap()
        .contains("16-world"));
    s.run("BenillaWorldSetup_Save()").unwrap();
    assert!(mailbox.take_request().is_none());
}

#[test]
fn profile_or_revision_changes_clear_setup_forms_and_stale_saved_selections() {
    for creating in [true, false] {
        for context_change in [true, false] {
            let mailbox = WorldSetupMailbox::default();
            mailbox.activate(view(1));
            let mut s = panel(&mailbox);
            if creating {
                s.run("BenillaWorldSetupFrameNewTab:Click(); BenillaWorldSetupFrameName:SetText('Old Form')").unwrap();
                s.tick(0.016);
            } else {
                s.run("BenillaWorldSetupRow1:Click()").unwrap();
            }
            let mut changed = view(1);
            if context_change {
                changed.context = "99".repeat(16);
                changed.owner_id = "88".repeat(16);
            } else {
                changed.revision += 1;
            }
            mailbox.activate(changed);
            s.run(if creating {
                "BenillaWorldSetup_Save()"
            } else {
                "BenillaWorldSetup_Use()"
            })
            .unwrap();
            assert!(mailbox.take_request().is_none());
            assert_eq!(
                s.eval::<String>("return BenillaWorldSetupFrameName:GetText()")
                    .unwrap(),
                ""
            );
            assert!(!s
                .eval::<bool>("return BenillaWorldSetupFrameUse:IsEnabled() ~= 0")
                .unwrap());
        }
    }
}

#[test]
fn setup_escape_back_and_close_never_submit_and_reopening_discards_unsaved_form() {
    let mailbox = WorldSetupMailbox::default();
    mailbox.activate(view(0));
    let mut s = panel(&mailbox);
    s.run("BenillaWorldSetupFrameNewTab:Click(); BenillaWorldSetupFrameName:SetText('Unsaved')")
        .unwrap();
    assert!(s.focus_editbox("BenillaWorldSetupFrameName"));
    s.key_down("ESCAPE");
    assert!(!s.frame_visible("BenillaWorldSetupFrame"));
    assert!(!s.has_keyboard_focus());
    assert_eq!(
        s.eval::<String>("return MainMenuMicroButton:GetButtonState()")
            .unwrap(),
        "NORMAL"
    );
    s.run("ShowUIPanel(BenillaWorldSetupFrame); BenillaWorldSetupFrameNewTab:Click()")
        .unwrap();
    assert_eq!(
        s.eval::<String>("return BenillaWorldSetupFrameName:GetText()")
            .unwrap(),
        ""
    );
    s.run("BenillaWorldSetupFrameBack:Click()").unwrap();
    assert!(s.frame_visible("BenillaAdminFrame"));
    assert!(!s.frame_visible("BenillaWorldSetupFrame"));
    assert!(mailbox.take_request().is_none());
    assert!(s.errors().is_empty(), "{:?}", s.errors());
}

#[test]
fn setup_without_a_profile_shows_an_honest_reason_and_disabled_actions() {
    let mailbox = WorldSetupMailbox::default();
    let s = panel(&mailbox);
    for mode in ["saved", "new"] {
        s.run(&format!("BenillaWorldSetup_Mode('{mode}')")).unwrap();
        assert!(!s
            .eval::<String>("return BenillaWorldSetupFrameReason:GetText()")
            .unwrap()
            .is_empty());
        assert!(!s
            .eval::<bool>("return BenillaWorldSetupFrameSave:IsEnabled() ~= 0")
            .unwrap());
        assert!(!s
            .eval::<bool>("return BenillaWorldSetupFrameUse:IsEnabled() ~= 0")
            .unwrap());
    }
    assert!(mailbox.take_request().is_none());
    assert!(s.errors().is_empty(), "{:?}", s.errors());
}

#[test]
fn current_saved_selection_cannot_queue_another_selection_receipt() {
    let mailbox = WorldSetupMailbox::default();
    let mut selected = view(2);
    selected.selected_id = Some(format!("{:032x}", 1));
    mailbox.activate(selected);
    let s = panel(&mailbox);
    s.run("BenillaWorldSetupRow1:Click(); BenillaWorldSetup_Use()")
        .unwrap();
    assert!(!s
        .eval::<bool>("return BenillaWorldSetupFrameUse:IsEnabled() ~= 0")
        .unwrap());
    assert!(mailbox.take_request().is_none());
}

#[test]
fn setup_controls_fit_both_modes_at_small_screen_sizes() {
    for (width, height) in [(320.0, 240.0), (800.0, 600.0)] {
        let mailbox = WorldSetupMailbox::default();
        mailbox.activate(view(16));
        let mut s = panel(&mailbox);
        s.run("HideUIPanel(BenillaWorldSetupFrame)").unwrap();
        s.set_screen_size(width, height);
        s.run(&format!("UIParent:SetPoint('TOPLEFT', nil, 'TOPLEFT', 0, 0); UIParent:SetWidth({width}); UIParent:SetHeight({height}); ShowUIPanel(BenillaWorldSetupFrame)")).unwrap();
        for mode in ["saved", "new"] {
            s.run(&format!("BenillaWorldSetup_Mode('{mode}')")).unwrap();
            s.resolve();
            let model = s.model_ref();
            let rect = |name| {
                *model
                    .resolved
                    .get(&model.arena.lookup(name).unwrap())
                    .unwrap()
            };
            let dialog = rect("BenillaWorldSetupFrame");
            assert!(
                dialog.left >= 0.0
                    && dialog.right <= width + 0.01
                    && dialog.bottom >= 0.0
                    && dialog.top <= height + 0.01,
                "{dialog:?}"
            );
            let mut names = vec![
                "BenillaWorldSetupFrameBack",
                "BenillaWorldSetupFrameDone",
                "BenillaWorldSetupFrameSavedTab",
                "BenillaWorldSetupFrameNewTab",
            ];
            if mode == "saved" {
                names.extend([
                    "BenillaWorldSetupRow1",
                    "BenillaWorldSetupRow5",
                    "BenillaWorldSetupFramePrevious",
                    "BenillaWorldSetupFrameNext",
                    "BenillaWorldSetupFrameUse",
                ]);
            } else {
                names.extend([
                    "BenillaWorldSetupFrameName",
                    "BenillaWorldSetupFrameAdmins",
                    "BenillaWorldSetupFrameSave",
                ]);
            }
            for name in names {
                let r = rect(name);
                assert!(
                    r.left >= dialog.left
                        && r.right <= dialog.right + 0.01
                        && r.bottom >= dialog.bottom
                        && r.top <= dialog.top + 0.01,
                    "{name}: {r:?} outside {dialog:?}"
                );
            }
        }
    }
}

#[test]
fn setup_entry_without_an_installed_host_is_read_only_and_explains_the_blocker() {
    let mut s = UiScript::new().unwrap();
    s.install_admin_interface().unwrap();
    load_panel(&mut s);
    s.run("ShowUIPanel(BenillaAdminFrame); BenillaAdminFrameWorldSetup:Click(); BenillaWorldSetupFrameNewTab:Click()").unwrap();
    assert!(s
        .eval::<bool>("return BenillaWorldSetupGetView == nil")
        .unwrap());
    assert!(!s
        .eval::<bool>("return BenillaWorldSetupFrameSave:IsEnabled() ~= 0")
        .unwrap());
    assert!(s
        .eval::<String>("return BenillaWorldSetupFrameReason:GetText()")
        .unwrap()
        .contains("unavailable"));
    assert!(s.errors().is_empty(), "{:?}", s.errors());
}
