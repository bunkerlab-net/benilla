use std::cell::RefCell;

use super::*;

struct Host {
    view: RefCell<AdminSnapshot>,
    requests: RefCell<Vec<AdminRequest>>,
    result: RefCell<Option<AdminResult>>,
    reject: RefCell<Option<String>>,
}

fn host() -> Rc<Host> {
    let row = |id, label: &str| AdminCatalogEntry {
        id,
        label: label.into(),
    };
    let mut view = AdminSnapshot::unavailable("Synthetic test authority");
    view.context = Some(AdminContext(
        "test-world/character/principal/session".into(),
    ));
    view.availability = std::array::from_fn(|_| AdminAvailability::Available);
    view.items = vec![row(101, "Test bag"), row(102, "Test item")].into();
    view.mounts = vec![row(201, "Test mount")].into();
    view.cities = vec![row(301, "Test city")].into();
    view.factions = vec![row(401, "Test faction")].into();
    view.level = Some(AdminLevel {
        current: 59,
        maximum: 60,
    });
    view.xp_enabled = Some(true);
    view.location = Some(AdminLocation {
        map_id: 0,
        zone_id: 8,
        revision: 1,
    });
    view.weather = vec![row(0, "Automatic"), row(1, "Test weather")].into();
    view.weather_automatic_id = Some(0);
    Rc::new(Host {
        view: RefCell::new(view),
        requests: RefCell::new(Vec::new()),
        result: RefCell::new(None),
        reject: RefCell::new(None),
    })
}

impl AdminBackend for Host {
    fn snapshot(&self) -> AdminSnapshot {
        self.view.borrow().clone()
    }
    fn submit(&self, request: AdminRequest) -> Result<(), String> {
        if let Some(reason) = self.reject.borrow().as_ref() {
            return Err(reason.clone());
        }
        self.requests.borrow_mut().push(request);
        Ok(())
    }
    fn poll(&self, _id: &AdminRequestId) -> Option<AdminResult> {
        self.result.borrow_mut().take()
    }
}

fn ui(host: Rc<Host>) -> UiScript {
    let s = UiScript::new().unwrap();
    s.set_admin_backend(Some(host)).unwrap();
    s.run("BenillaAdminGetView()").unwrap();
    s
}

#[test]
fn opt_in_interface_defaults_closed_and_never_emits_realm_commands() {
    let mut s = UiScript::new().unwrap();
    assert!(s.eval::<bool>("return BenillaAdminGetView == nil").unwrap());
    s.install_admin_interface().unwrap();
    for action in AdminAction::ALL {
        assert!(!s
            .eval::<bool>(&format!(
                "return BenillaAdminGetView().{}.enabled",
                action.key()
            ))
            .unwrap());
        assert!(!s
            .eval::<bool>(&format!(
                "return BenillaAdminSubmit('{}', '101', '1')",
                action.key()
            ))
            .unwrap());
    }
    assert!(s.take_chat_sends().is_empty());
}

#[test]
fn validates_all_typed_operations_without_executing_gameplay() {
    let snapshot = host().snapshot();
    let valid = [
        (
            AdminAction::Item,
            "101",
            "200",
            AdminOperation::GrantItem {
                item_id: 101,
                count: 200,
            },
        ),
        (
            AdminAction::Gold,
            "",
            "10000",
            AdminOperation::GrantGold {
                copper: 100_000_000,
            },
        ),
        (
            AdminAction::Mount,
            "201",
            "",
            AdminOperation::GrantMount { mount_id: 201 },
        ),
        (
            AdminAction::Speed,
            "",
            "300",
            AdminOperation::SetMovementSpeed {
                basis_points: 30_000,
            },
        ),
        (
            AdminAction::Teleport,
            "301",
            "",
            AdminOperation::Teleport { city_id: 301 },
        ),
        (AdminAction::Level, "", "", AdminOperation::GainOneLevel),
        (
            AdminAction::Xp,
            "",
            "off",
            AdminOperation::SetXpGain { enabled: false },
        ),
        (
            AdminAction::Reputation,
            "401",
            "-42999",
            AdminOperation::AdjustReputation {
                faction_id: 401,
                delta: -42999,
            },
        ),
        (
            AdminAction::Honor,
            "",
            "1",
            AdminOperation::GrantHonorContribution { points: 1 },
        ),
        (
            AdminAction::Time,
            "",
            "23:59",
            AdminOperation::SetWorldTime {
                minute_of_day: 1439,
            },
        ),
        (
            AdminAction::Time,
            "",
            "auto",
            AdminOperation::RestoreAutomaticWorldTime,
        ),
        (
            AdminAction::Weather,
            "0",
            "",
            AdminOperation::RestoreAutomaticWeather {
                location: snapshot.location.clone().unwrap(),
            },
        ),
        (
            AdminAction::Weather,
            "1",
            "",
            AdminOperation::SetLocationWeather {
                location: snapshot.location.clone().unwrap(),
                option_id: 1,
            },
        ),
    ];
    for (action, selection, amount, expected) in valid {
        assert_eq!(
            validate_operation(&snapshot, action, selection, amount).unwrap(),
            expected
        );
    }
}

#[test]
fn invalid_values_catalog_ids_and_locations_fail_closed() {
    let snapshot = host().snapshot();
    for amount in [
        "",
        "0",
        "201",
        "-1",
        "1.5",
        "1e2",
        " 1",
        "+1",
        "4294967296",
        "NaN",
        "inf",
    ] {
        assert!(
            validate_operation(&snapshot, AdminAction::Item, "101", amount).is_err(),
            "{amount}"
        );
    }
    for (action, selection, amount) in [
        (AdminAction::Item, "999", "1"),
        (AdminAction::Item, "0", "1"),
        (AdminAction::Gold, "", "10001"),
        (AdminAction::Speed, "", "99"),
        (AdminAction::Speed, "", "301"),
        (AdminAction::Reputation, "401", "0"),
        (AdminAction::Reputation, "401", "-43000"),
        (AdminAction::Xp, "", "false"),
        (AdminAction::Time, "", "24:00"),
        (AdminAction::Time, "", "00:60"),
        (AdminAction::Time, "", "1:00"),
        (AdminAction::Time, "", "12:00:00"),
        (AdminAction::Weather, "9", ""),
        (AdminAction::Honor, "", "100001"),
    ] {
        assert!(validate_operation(&snapshot, action, selection, amount).is_err());
    }
    let mut ambiguous = snapshot.clone();
    let mut items = ambiguous.items.to_vec();
    items.push(items[0].clone());
    ambiguous.items = items.into();
    assert!(validate_operation(&ambiguous, AdminAction::Item, "101", "1").is_err());
    ambiguous.location = None;
    assert!(validate_operation(&ambiguous, AdminAction::Weather, "1", "").is_err());
}

#[test]
fn capability_identity_level_and_xp_state_are_required() {
    let h = host();
    let s = ui(h.clone());
    for alteration in 0..5 {
        let mut view = host().snapshot();
        match alteration {
            0 => view.context = None,
            1 => {
                view.availability[AdminAction::Level as usize] =
                    AdminAvailability::Unavailable("Owner permission required".into())
            }
            2 => view.level.as_mut().unwrap().current = 60,
            3 => view.level.as_mut().unwrap().maximum = 61,
            _ => view.level = None,
        }
        *h.view.borrow_mut() = view;
        s.run("BenillaAdminGetView()").unwrap();
        assert!(!s
            .eval::<bool>("return BenillaAdminSubmit('level', '', '')")
            .unwrap());
    }
    let mut view = host().snapshot();
    view.xp_enabled = None;
    *h.view.borrow_mut() = view;
    s.run("BenillaAdminGetView()").unwrap();
    assert!(!s
        .eval::<bool>("return BenillaAdminSubmit('xp', '', 'off')")
        .unwrap());
    assert!(h.requests.borrow().is_empty());
}

#[test]
fn stale_snapshot_and_forged_lua_capabilities_never_bypass_the_host() {
    let h = host();
    let s = ui(h.clone());
    s.run("local v = BenillaAdminGetView(); v.item.enabled = true; v.item.entries[1].id = '999'")
        .unwrap();
    assert!(!s
        .eval::<bool>("return BenillaAdminSubmit('item', '999', '1')")
        .unwrap());
    h.view.borrow_mut().revision += 1;
    assert!(!s
        .eval::<bool>("return BenillaAdminSubmit('item', '101', '1')")
        .unwrap());
    s.run("BenillaAdminGetView()").unwrap();
    h.view.borrow_mut().location.as_mut().unwrap().revision += 1;
    assert!(!s
        .eval::<bool>("return BenillaAdminSubmit('weather', '1', '')")
        .unwrap());
    assert!(h.requests.borrow().is_empty());
}

#[test]
fn duplicate_clicks_wait_for_a_matching_durable_receipt_and_reject_false_success() {
    let h = host();
    let s = ui(h.clone());
    assert!(s
        .eval::<bool>("return BenillaAdminSubmit('gold', '', '1')")
        .unwrap());
    assert!(!s
        .eval::<bool>("return BenillaAdminSubmit('gold', '', '1')")
        .unwrap());
    assert_eq!(h.requests.borrow().len(), 1);
    let id = h.requests.borrow()[0].id.clone();
    assert!(s
        .eval::<String>("return BenillaAdminGetView().message")
        .unwrap()
        .contains("pending"));
    let mut wrong = id.clone();
    wrong.sequence += 1;
    *h.result.borrow_mut() = Some(AdminResult {
        id: wrong,
        outcome: AdminOutcome::Committed {
            receipt: "test-receipt".into(),
        },
    });
    assert!(s
        .eval::<bool>("return BenillaAdminGetView().pending")
        .unwrap());
    *h.result.borrow_mut() = Some(AdminResult {
        id: id.clone(),
        outcome: AdminOutcome::Committed { receipt: "".into() },
    });
    assert!(s
        .eval::<bool>("return BenillaAdminGetView().pending")
        .unwrap());
    *h.result.borrow_mut() = Some(AdminResult {
        id,
        outcome: AdminOutcome::Committed {
            receipt: "test-only-durable-receipt".into(),
        },
    });
    assert!(s
        .eval::<String>("return BenillaAdminGetView().message")
        .unwrap()
        .contains("test-only-durable-receipt"));
    assert!(!s
        .eval::<bool>("return BenillaAdminGetView().pending")
        .unwrap());
    assert!(s
        .eval::<bool>("return BenillaAdminSubmit('gold', '', '1')")
        .unwrap());
    assert_eq!(h.requests.borrow()[1].id.sequence, 2);
}

#[test]
fn backend_rejection_is_honest_and_detaching_preserves_unknown_outcome() {
    let h = host();
    *h.reject.borrow_mut() = Some("Not an authorized owner".into());
    let s = ui(h.clone());
    assert!(!s
        .eval::<bool>("return BenillaAdminSubmit('gold', '', '1')")
        .unwrap());
    assert!(s
        .eval::<String>("return BenillaAdminGetView().message")
        .unwrap()
        .contains("Not an authorized owner"));
    *h.reject.borrow_mut() = None;
    assert!(s
        .eval::<bool>("return BenillaAdminSubmit('gold', '', '1')")
        .unwrap());
    let id = h.requests.borrow()[0].id.clone();
    *h.result.borrow_mut() = Some(AdminResult {
        id,
        outcome: AdminOutcome::Rejected {
            reason: "Balance limit".into(),
        },
    });
    assert!(s
        .eval::<String>("return BenillaAdminGetView().message")
        .unwrap()
        .contains("Rejected: Balance limit"));
    assert!(s
        .eval::<bool>("return BenillaAdminSubmit('gold', '', '1')")
        .unwrap());
    s.set_admin_backend(None).unwrap();
    assert!(s
        .eval::<bool>("return BenillaAdminGetView().pending")
        .unwrap());
    assert!(s
        .eval::<String>("return BenillaAdminGetView().message")
        .unwrap()
        .contains("unknown"));
}

/// Synthetic engine-only dependencies verify our own layer's behavior without distributing
/// Blizzard FrameXML. The application tests separately require the real player-supplied files.
fn load_panel(s: &mut UiScript) {
    s.set_screen_size(1024.0, 768.0);
    let dependencies = r#"<Ui>
        <Font name="GameFontNormal" font="test.ttf" virtual="true"><FontHeight><AbsValue val="12"/></FontHeight></Font>
        <Font name="GameFontNormalSmall" inherits="GameFontNormal" virtual="true"/>
        <Font name="GameFontNormalLarge" inherits="GameFontNormal" virtual="true"/>
        <Font name="GameFontHighlight" inherits="GameFontNormal" virtual="true"/>
        <Font name="GameFontHighlightSmall" inherits="GameFontNormal" virtual="true"/>
        <Button name="GameMenuButtonTemplate" virtual="true"><Size><AbsDimension x="144" y="21"/></Size></Button>
        <Button name="UIPanelButtonTemplate" inherits="GameMenuButtonTemplate" virtual="true"/>
        <Button name="UIPanelCloseButton" inherits="GameMenuButtonTemplate" virtual="true"/>
        <EditBox name="InputBoxTemplate" virtual="true"><FontString inherits="GameFontNormal"/></EditBox>
        <Frame name="UIParent"><Size><AbsDimension x="1024" y="768"/></Size></Frame>
        <Frame name="GameMenuFrame" parent="UIParent" hidden="true"/>
    </Ui>"#;
    let doc = crate::framexml::parse(dependencies).unwrap();
    assert!(crate::loader::load(s, &doc, &|_| None).errors.is_empty());
    s.run(r#"
        UIPanelWindows = {}
        MAIN_MENU = "Main Menu"
        CLOSE = "Close"
        function ShowUIPanel(frame) frame:Show() end
        function HideUIPanel(frame) frame:Hide() end
        MainMenuMicroButton = CreateFrame("Button", "MainMenuMicroButton", UIParent)
        function UpdateMicroButtons() MainMenuMicroButton:SetButtonState("NORMAL") end
        function IsOptionFrameOpen() return nil end
        function StaticPopup_EscapePressed() return nil end
        function ToggleGameMenu(clicked)
            if GameMenuFrame:IsShown() then GameMenuFrame:Hide() else GameMenuFrame:Show() end
        end
        GameMenuFrame:SetScript("OnShow", function() end)
        for _, suffix in ipairs({ "Options", "SoundOptions", "UIOptions", "Keybindings", "Macros", "Logout", "Quit", "Continue" }) do
            CreateFrame("Button", "GameMenuButton" .. suffix, GameMenuFrame, "GameMenuButtonTemplate")
        end
    "#).unwrap();
    let doc = crate::framexml::parse(include_str!(
        "../../../../benilla-app/assets/ui/GameMenuAdapters.xml"
    ))
    .unwrap();
    let report = crate::loader::load(s, &doc, &|_| None);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert!(s.errors().is_empty(), "{:?}", s.errors());
}

#[test]
fn panel_menu_close_reopen_search_and_pending_flow_use_the_actual_layer() {
    let h = host();
    let mut s = ui(h.clone());
    load_panel(&mut s);
    s.run("ToggleGameMenu(); BenillaGameMenuButtonAdmin:Click()")
        .unwrap();
    assert!(s.frame_visible("BenillaAdminFrame"));
    assert_eq!(
        s.eval::<String>("return MainMenuMicroButton:GetButtonState()")
            .unwrap(),
        "PUSHED"
    );
    assert!(!s.frame_visible("GameMenuFrame"));
    s.run("BenillaAdminFrameSearch:SetText('TEST ITEM')")
        .unwrap();
    s.tick(0.016);
    assert!(s
        .eval::<String>("return BenillaAdminFrameChoice:GetText()")
        .unwrap()
        .contains("102"));
    s.run("BenillaAdminFrameSearch:SetText('101'); BenillaAdminFrameAmount:SetText('2')")
        .unwrap();
    s.tick(0.016);
    s.run("BenillaAdminFrameApply:Click(); BenillaAdminFrameApply:Click()")
        .unwrap();
    assert_eq!(h.requests.borrow().len(), 1);
    assert_eq!(
        h.requests.borrow()[0].operation,
        AdminOperation::GrantItem {
            item_id: 101,
            count: 2
        }
    );
    s.run("ToggleGameMenu()").unwrap();
    assert!(!s.frame_visible("BenillaAdminFrame"));
    assert_eq!(
        s.eval::<String>("return MainMenuMicroButton:GetButtonState()")
            .unwrap(),
        "NORMAL"
    );
    assert!(!s.frame_visible("GameMenuFrame"));
    s.run("ToggleGameMenu(); BenillaGameMenuButtonAdmin:Click()")
        .unwrap();
    assert!(!s
        .eval::<bool>("return BenillaAdminFrameApply:IsEnabled() ~= 0")
        .unwrap());
    s.run("BenillaAdminFrameCloseButton:Click(); ToggleGameMenu(); BenillaGameMenuButtonAdmin:Click(); ToggleGameMenu(1)").unwrap();
    assert!(!s.frame_visible("BenillaAdminFrame"));
    assert!(s.frame_visible("GameMenuFrame"));
    assert!(s.errors().is_empty(), "{:?}", s.errors());
}

#[test]
fn every_panel_action_has_an_honest_unavailable_reason_without_a_host() {
    let mut s = UiScript::new().unwrap();
    s.install_admin_interface().unwrap();
    load_panel(&mut s);
    s.run("ShowUIPanel(BenillaAdminFrame)").unwrap();
    for i in 1..=11 {
        s.run(&format!("BenillaAdmin_Select({i})")).unwrap();
        assert!(!s
            .eval::<bool>("return BenillaAdminFrameApply:IsEnabled() ~= 0")
            .unwrap());
        assert!(!s
            .eval::<String>("return BenillaAdminFrameReason:GetText()")
            .unwrap()
            .is_empty());
    }
    s.run("BenillaAdminFrameDone:Click()").unwrap();
    assert!(!s.frame_visible("BenillaAdminFrame"));
    assert!(s.take_chat_sends().is_empty());
    assert!(s.errors().is_empty(), "{:?}", s.errors());
}

#[test]
fn changed_world_clears_old_success_and_empty_disabled_reasons_stay_honest() {
    let h = host();
    let s = ui(h.clone());
    assert!(s
        .eval::<bool>("return BenillaAdminSubmit('gold', '', '1')")
        .unwrap());
    let id = h.requests.borrow()[0].id.clone();
    *h.result.borrow_mut() = Some(AdminResult {
        id,
        outcome: AdminOutcome::Committed {
            receipt: "world-a-receipt".into(),
        },
    });
    assert!(s
        .eval::<String>("return BenillaAdminGetView().message")
        .unwrap()
        .contains("world-a-receipt"));
    h.view.borrow_mut().context = Some(AdminContext("world-b".into()));
    assert_eq!(
        s.eval::<String>("return BenillaAdminGetView().message")
            .unwrap(),
        ""
    );
    h.view.borrow_mut().availability[AdminAction::Gold as usize] =
        AdminAvailability::Unavailable(String::new());
    assert!(!s
        .eval::<String>("return BenillaAdminGetView().gold.reason")
        .unwrap()
        .is_empty());
    for automatic in [None, Some(99)] {
        h.view.borrow_mut().weather_automatic_id = automatic;
        assert!(!s
            .eval::<bool>("return BenillaAdminGetView().weather.enabled")
            .unwrap());
    }
}

#[test]
fn focused_escape_popup_priority_and_small_screen_fit() {
    let mut s = UiScript::new().unwrap();
    s.install_admin_interface().unwrap();
    load_panel(&mut s);
    s.set_screen_size(320.0, 240.0);
    s.run("ShowUIPanel(BenillaAdminFrame)").unwrap();
    let scale = s
        .eval::<f64>("return BenillaAdminFrame:GetScale()")
        .unwrap();
    assert!(584.0 * scale <= 320.0 && 528.0 * scale <= 240.0);
    s.run("function StaticPopup_EscapePressed() return 1 end; ToggleGameMenu()")
        .unwrap();
    assert!(s.frame_visible("BenillaAdminFrame"));
    s.run("function StaticPopup_EscapePressed() return nil end")
        .unwrap();
    for field in ["BenillaAdminFrameSearch", "BenillaAdminFrameAmount"] {
        s.run("ShowUIPanel(BenillaAdminFrame)").unwrap();
        assert!(s.focus_editbox(field));
        assert!(s.has_keyboard_focus());
        s.key_down("ESCAPE");
        assert!(!s.frame_visible("BenillaAdminFrame"));
        assert!(!s.has_keyboard_focus());
        assert!(!s.frame_visible("GameMenuFrame"));
    }
    assert!(s.errors().is_empty(), "{:?}", s.errors());
}

#[test]
fn backend_detach_replace_clear_completed_results_but_keep_fresh_context_errors() {
    for detach in [true, false] {
        let h = host();
        let s = ui(h.clone());
        assert!(s
            .eval::<bool>("return BenillaAdminSubmit('gold', '', '1')")
            .unwrap());
        let id = h.requests.borrow()[0].id.clone();
        *h.result.borrow_mut() = Some(AdminResult {
            id,
            outcome: AdminOutcome::Committed {
                receipt: "old-world-receipt".into(),
            },
        });
        s.run("BenillaAdminGetView()").unwrap();
        s.set_admin_backend(if detach { None } else { Some(host()) })
            .unwrap();
        assert_eq!(
            s.eval::<String>("return BenillaAdminGetView().message")
                .unwrap(),
            ""
        );
    }
    let h = host();
    let s = ui(h.clone());
    h.view.borrow_mut().context = Some(AdminContext("changed-world".into()));
    assert!(!s
        .eval::<bool>("return BenillaAdminSubmit('gold', '', '1')")
        .unwrap());
    assert!(s
        .eval::<String>("return BenillaAdminGetView().message")
        .unwrap()
        .contains("context or catalog changed"));
    h.view.borrow_mut().weather = vec![AdminCatalogEntry {
        id: 0,
        label: " ".into(),
    }]
    .into();
    assert!(!s
        .eval::<bool>("return BenillaAdminGetView().weather.enabled")
        .unwrap());
}

#[test]
fn pending_request_reconciles_its_receipt_after_switching_away_and_back() {
    let h = host();
    let s = ui(h.clone());
    assert!(s
        .eval::<bool>("return BenillaAdminSubmit('gold', '', '1')")
        .unwrap());
    let id = h.requests.borrow()[0].id.clone();
    h.view.borrow_mut().context = Some(AdminContext("another-world".into()));
    s.run("BenillaAdminGetView()").unwrap();
    assert!(!s
        .eval::<bool>("return BenillaAdminSubmit('gold', '', '1')")
        .unwrap());
    h.view.borrow_mut().context = Some(id.context.clone());
    *h.result.borrow_mut() = Some(AdminResult {
        id,
        outcome: AdminOutcome::Committed {
            receipt: "reconciled-receipt".into(),
        },
    });
    for _ in 0..2 {
        assert!(s
            .eval::<String>("return BenillaAdminGetView().message")
            .unwrap()
            .contains("reconciled-receipt"));
    }
    assert_eq!(h.requests.borrow().len(), 1);
}
