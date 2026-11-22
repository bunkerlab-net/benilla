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
        .eval::<String>("return BenillaAdminResult1:GetText()")
        .unwrap()
        .contains("102"));
    s.run("BenillaAdminFrameSearch:SetText('101'); BenillaAdminFrameAmount:SetText('2')")
        .unwrap();
    s.tick(0.016);
    s.run("BenillaAdminResult1:Click(); BenillaAdminFrameApply:Click(); BenillaAdminFrameApply:Click()")
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
    assert!(584.0 * scale <= 320.0 && 648.0 * scale <= 240.0);
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

fn catalog(count: u32) -> Rc<[AdminCatalogEntry]> {
    (1..=count)
        .rev()
        .map(|id| AdminCatalogEntry {
            id,
            label: format!("Fictional entry {id:06}"),
        })
        .collect()
}

#[test]
fn empty_query_browses_all_catalogs_in_stable_pages_without_copying_them_to_lua() {
    let h = host();
    {
        let mut view = h.view.borrow_mut();
        view.items = catalog(12);
        view.cities = catalog(12);
        view.mounts = catalog(12);
        view.factions = catalog(12);
        view.weather = catalog(12);
        view.weather_automatic_id = Some(1);
    }
    let s = ui(h.clone());
    for action in ["item", "teleport", "mount", "reputation", "weather"] {
        s.run(&format!("search = BenillaAdminSearch('{action}', '', 1)"))
            .unwrap();
        assert_eq!(s.eval::<u32>("return search.total").unwrap(), 12);
        assert_eq!(s.eval::<u32>("return search.pages").unwrap(), 3);
        assert_eq!(
            s.eval::<u32>("return table.getn(search.entries)").unwrap(),
            5
        );
        assert_eq!(
            s.eval::<String>("return search.entries[1].id").unwrap(),
            "1"
        );
        s.run(&format!("search = BenillaAdminSearch('{action}', '', 2)"))
            .unwrap();
        assert_eq!(
            s.eval::<String>("return search.entries[1].id").unwrap(),
            "6"
        );
        s.run(&format!(
            "search = BenillaAdminSearch('{action}', '', 999999999999)"
        ))
        .unwrap();
        assert_eq!(s.eval::<u32>("return search.page").unwrap(), 3);
        assert_eq!(
            s.eval::<u32>("return table.getn(search.entries)").unwrap(),
            2
        );
        assert_eq!(
            s.eval::<String>("return search.entries[2].id").unwrap(),
            "12"
        );
        s.run(&format!(
            "search = BenillaAdminSearch('{action}', '', -200)"
        ))
        .unwrap();
        assert_eq!(s.eval::<u32>("return search.page").unwrap(), 1);
        assert!(
            s.eval::<usize>(&format!(
                "return table.getn(BenillaAdminGetView().{action}.entries)"
            ))
            .unwrap()
                <= PAGE_SIZE
        );
    }
    assert!(h.requests.borrow().is_empty());
}

#[test]
fn typeahead_ranks_exact_id_then_prefix_then_substrings_without_pattern_matching() {
    let h = host();
    h.view.borrow_mut().items = vec![
        AdminCatalogEntry {
            id: 9,
            label: "Other test [bag]".into(),
        },
        AdminCatalogEntry {
            id: 91,
            label: "Test [bag]".into(),
        },
        AdminCatalogEntry {
            id: 90,
            label: "Test [bag]".into(),
        },
        AdminCatalogEntry {
            id: 2,
            label: "Item 9".into(),
        },
    ]
    .into();
    let s = ui(h);
    s.run("search = BenillaAdminSearch('item', '9', 1)")
        .unwrap();
    assert_eq!(
        s.eval::<String>("return search.entries[1].id").unwrap(),
        "9"
    );
    assert_eq!(
        s.eval::<String>("return search.entries[2].id").unwrap(),
        "90"
    );
    assert_eq!(
        s.eval::<String>("return search.entries[3].id").unwrap(),
        "91"
    );
    assert_eq!(
        s.eval::<String>("return search.entries[4].id").unwrap(),
        "2"
    );
    s.run("search = BenillaAdminSearch('item', '  TEST [BAG]  ', 1)")
        .unwrap();
    assert_eq!(
        s.eval::<String>("return search.entries[1].id").unwrap(),
        "90"
    );
    assert_eq!(
        s.eval::<String>("return search.entries[2].id").unwrap(),
        "91"
    );
    assert_eq!(
        s.eval::<String>("return search.entries[3].id").unwrap(),
        "9"
    );
    s.run("search = BenillaAdminSearch('item', '%', 1)")
        .unwrap();
    assert_eq!(s.eval::<u32>("return search.total").unwrap(), 0);
    assert_eq!(s.eval::<u32>("return search.page").unwrap(), 0);
}

#[test]
fn search_is_bounded_and_unchanged_queries_or_pages_do_not_rescan_large_catalogs() {
    let h = host();
    h.view.borrow_mut().items = catalog(MAX_CATALOG_ENTRIES as u32);
    let s = ui(h.clone());
    s.run("search = BenillaAdminSearch('item', '', 1)").unwrap();
    assert_eq!(
        s.eval::<usize>("return search.total").unwrap(),
        MAX_CATALOG_ENTRIES
    );
    for page in [1, 2, 20_000, 100_000] {
        s.run(&format!(
            "BenillaAdminGetView(); search = BenillaAdminSearch('item', '', {page})"
        ))
        .unwrap();
        assert_eq!(
            s.eval::<usize>("return table.getn(search.entries)")
                .unwrap(),
            PAGE_SIZE
        );
    }
    {
        let state = s.lua.app_data_ref::<AdminUi>().unwrap();
        assert_eq!(
            state.catalogs[AdminAction::Item as usize]
                .as_ref()
                .unwrap()
                .scanned,
            MAX_CATALOG_ENTRIES
        );
    }
    s.run("search = BenillaAdminSearch('item', '100000', 1)")
        .unwrap();
    assert_eq!(
        s.eval::<String>("return search.entries[1].id").unwrap(),
        "100000"
    );
    for query in [
        "x".repeat(81),
        "é".repeat(81),
        "x".repeat(1_000_000),
        "line\nbreak".into(),
    ] {
        s.lua.globals().set("badQuery", query).unwrap();
        s.run("search = BenillaAdminSearch('item', badQuery, 1)")
            .unwrap();
        assert_eq!(
            s.eval::<usize>("return table.getn(search.entries)")
                .unwrap(),
            0
        );
        assert!(!s.eval::<String>("return search.reason").unwrap().is_empty());
    }
    h.view.borrow_mut().items = catalog(MAX_CATALOG_ENTRIES as u32 + 1);
    s.run("search = BenillaAdminSearch('item', '', 1)").unwrap();
    assert_eq!(
        s.eval::<usize>("return table.getn(search.entries)")
            .unwrap(),
        0
    );
    assert!(s
        .eval::<String>("return search.reason")
        .unwrap()
        .contains("browse limit"));
    assert!(!s
        .eval::<bool>("return BenillaAdminSubmit('item', '1', '1')")
        .unwrap());
    assert!(h.requests.borrow().is_empty());
}

#[test]
fn malformed_catalogs_have_explicit_browse_errors_and_no_ambiguous_suggestions() {
    for rows in [
        vec![
            AdminCatalogEntry {
                id: 1,
                label: "A".into(),
            },
            AdminCatalogEntry {
                id: 1,
                label: "B".into(),
            },
        ],
        vec![AdminCatalogEntry {
            id: 0,
            label: "Invalid ID".into(),
        }],
        vec![AdminCatalogEntry {
            id: 1,
            label: " ".into(),
        }],
        vec![AdminCatalogEntry {
            id: 1,
            label: "x".repeat(MAX_LABEL_BYTES + 1),
        }],
        vec![AdminCatalogEntry {
            id: 1,
            label: "Broken\nlabel".into(),
        }],
    ] {
        let h = host();
        h.view.borrow_mut().items = rows.into();
        let s = ui(h);
        s.run("search = BenillaAdminSearch('item', '', 1)").unwrap();
        assert_eq!(
            s.eval::<usize>("return table.getn(search.entries)")
                .unwrap(),
            0
        );
        assert!(s
            .eval::<String>("return search.reason")
            .unwrap()
            .contains("invalid or ambiguous"));
    }
}

#[test]
fn actual_panel_browses_pages_and_requires_explicit_selection_before_submit() {
    let h = host();
    h.view.borrow_mut().items = catalog(12);
    let mut s = ui(h.clone());
    load_panel(&mut s);
    s.run("ShowUIPanel(BenillaAdminFrame)").unwrap();
    assert!(s
        .eval::<String>("return BenillaAdminFramePageCount:GetText()")
        .unwrap()
        .contains("1-5 of 12"));
    assert!(!s
        .eval::<bool>("return BenillaAdminFramePrevious:IsEnabled() ~= 0")
        .unwrap());
    assert!(!s
        .eval::<bool>("return BenillaAdminFrameApply:IsEnabled() ~= 0")
        .unwrap());
    s.run("BenillaAdminFrameNext:Click()").unwrap();
    assert!(s
        .eval::<String>("return BenillaAdminResult1:GetText()")
        .unwrap()
        .contains("[6]"));
    s.run("BenillaAdminResult2:Click()").unwrap();
    assert!(s
        .eval::<String>("return BenillaAdminFrameChoice:GetText()")
        .unwrap()
        .contains("[7]"));
    assert!(h.requests.borrow().is_empty());
    assert!(s
        .eval::<bool>("return BenillaAdminFrameApply:IsEnabled() ~= 0")
        .unwrap());
    s.run("BenillaAdminFrameNext:Click()").unwrap();
    assert!(!s
        .eval::<bool>("return BenillaAdminFrameApply:IsEnabled() ~= 0")
        .unwrap());
    assert!(!s
        .eval::<bool>("return BenillaAdminFrameNext:IsEnabled() ~= 0")
        .unwrap());
    assert!(!s.frame_visible("BenillaAdminResult3"));
    s.run("BenillaAdminResult2:Click(); BenillaAdminFrameApply:Click()")
        .unwrap();
    assert_eq!(
        h.requests.borrow()[0].operation,
        AdminOperation::GrantItem {
            item_id: 12,
            count: 1
        }
    );
    assert!(s.errors().is_empty(), "{:?}", s.errors());
}

#[test]
fn keyboard_autocomplete_browses_and_selects_but_enter_never_grants() {
    let h = host();
    h.view.borrow_mut().items = catalog(12);
    let mut s = ui(h.clone());
    load_panel(&mut s);
    s.run("ShowUIPanel(BenillaAdminFrame)").unwrap();
    assert!(s.focus_editbox("BenillaAdminFrameSearch"));
    s.key_down("TAB");
    assert!(!s.has_keyboard_focus());
    s.key_down("DOWN");
    s.key_down("ENTER");
    assert!(s
        .eval::<String>("return BenillaAdminFrameChoice:GetText()")
        .unwrap()
        .contains("[2]"));
    s.key_down("ENTER");
    assert!(h.requests.borrow().is_empty());
    s.key_down("TAB");
    assert!(s.has_keyboard_focus());
    s.run("BenillaAdminFrameSearch:SetText('000009')").unwrap();
    s.key_down("ENTER");
    assert!(s
        .eval::<String>("return BenillaAdminFrameChoice:GetText()")
        .unwrap()
        .contains("[9]"));
    assert!(h.requests.borrow().is_empty());
    s.key_down("TAB");
    s.run("BenillaAdminFrameSearch:SetText('')").unwrap();
    s.key_down("TAB");
    s.key_down("RIGHT");
    s.key_down("DOWN");
    s.key_down("ENTER");
    assert!(s
        .eval::<String>("return BenillaAdminFrameChoice:GetText()")
        .unwrap()
        .contains("[7]"));
    s.key_down("ESCAPE");
    assert!(!s.frame_visible("BenillaAdminFrame"));
    assert!(!s.has_keyboard_focus());
    assert_eq!(
        s.eval::<String>("return MainMenuMicroButton:GetButtonState()")
            .unwrap(),
        "NORMAL"
    );
    assert!(h.requests.borrow().is_empty());
    assert!(s.errors().is_empty(), "{:?}", s.errors());
}

#[test]
fn catalog_query_category_world_location_and_revision_changes_invalidate_selection() {
    for change in 0..7 {
        let h = host();
        let mut s = ui(h.clone());
        load_panel(&mut s);
        s.run("ShowUIPanel(BenillaAdminFrame); BenillaAdminResult1:Click()")
            .unwrap();
        match change {
            0 => s.run("BenillaAdminFrameSearch:SetText('Test')").unwrap(),
            1 => s.run("BenillaAdmin_Select(3)").unwrap(),
            2 => h.view.borrow_mut().context = Some(AdminContext("new-world".into())),
            3 => h.view.borrow_mut().revision += 1,
            4 => h.view.borrow_mut().location.as_mut().unwrap().revision += 1,
            5 => {
                h.view.borrow_mut().items = vec![AdminCatalogEntry {
                    id: 101,
                    label: "Changed item".into(),
                }]
                .into()
            }
            _ => s.set_admin_backend(Some(host())).unwrap(),
        }
        s.run("BenillaAdmin_Apply()").unwrap();
        assert!(h.requests.borrow().is_empty(), "change {change}");
        assert!(
            !s.eval::<bool>("return BenillaAdminFrameApply:IsEnabled() ~= 0")
                .unwrap(),
            "change {change}"
        );
        assert!(!s
            .eval::<String>("return BenillaAdminFrameChoice:GetText()")
            .unwrap()
            .starts_with("Selected:"));
    }
}

#[test]
fn stale_suggestion_click_and_stale_generation_cannot_choose_or_submit_a_new_catalog_row() {
    let h = host();
    let mut s = ui(h.clone());
    load_panel(&mut s);
    s.run("ShowUIPanel(BenillaAdminFrame); oldGeneration = BenillaAdminGetView().generation")
        .unwrap();
    h.view.borrow_mut().items = vec![AdminCatalogEntry {
        id: 101,
        label: "New label, same ID".into(),
    }]
    .into();
    s.run("BenillaAdminResult1:Click()").unwrap();
    assert!(!s
        .eval::<bool>("return BenillaAdminFrameApply:IsEnabled() ~= 0")
        .unwrap());
    assert!(!s
        .eval::<bool>("return BenillaAdminSubmit('item', '101', '1', oldGeneration)")
        .unwrap());
    assert!(h.requests.borrow().is_empty());
}

#[test]
fn current_location_weather_catalog_updates_without_inventing_options() {
    let h = host();
    let mut s = ui(h.clone());
    load_panel(&mut s);
    s.run("ShowUIPanel(BenillaAdminFrame); BenillaAdmin_Select(11); BenillaAdminResult2:Click()")
        .unwrap();
    h.view.borrow_mut().location.as_mut().unwrap().revision += 1;
    h.view.borrow_mut().weather = vec![AdminCatalogEntry {
        id: 0,
        label: "Automatic".into(),
    }]
    .into();
    s.run("BenillaAdmin_Apply()").unwrap();
    assert!(h.requests.borrow().is_empty());
    assert!(s
        .eval::<String>("return BenillaAdminResult1:GetText()")
        .unwrap()
        .contains("Automatic [0]"));
    assert!(!s.frame_visible("BenillaAdminResult2"));
    s.run("BenillaAdminResult1:Click(); BenillaAdminFrameApply:Click()")
        .unwrap();
    assert_eq!(
        h.requests.borrow()[0].operation,
        AdminOperation::RestoreAutomaticWeather {
            location: h.view.borrow().location.clone().unwrap()
        }
    );
}

#[test]
fn unsupported_item_reason_remains_complete_in_tooltip_without_inferred_readiness() {
    let h = host();
    let label = "Fictional elaborate bag (grant unavailable: fictional nested container rules have not been admitted by this world's backend)";
    h.view.borrow_mut().items = vec![AdminCatalogEntry {
        id: 500,
        label: label.into(),
    }]
    .into();
    *h.reject.borrow_mut() = Some("Fictional nested container rules are unavailable".into());
    let mut s = ui(h.clone());
    load_panel(&mut s);
    s.run("GameTooltip = { SetOwner = function() end, SetText = function(self, text) tooltipText = text end, AddLine = function() end, Show = function() end, Hide = function() end }; ShowUIPanel(BenillaAdminFrame); BenillaAdmin_Tooltip(1)").unwrap();
    assert_eq!(s.eval::<String>("return tooltipText").unwrap(), label);
    s.run("BenillaAdminResult1:Click()").unwrap();
    assert!(h.requests.borrow().is_empty());
    s.run("BenillaAdminFrameApply:Click()").unwrap();
    assert!(h.requests.borrow().is_empty());
    assert!(s
        .eval::<String>("return BenillaAdminFrameResult:GetText()")
        .unwrap()
        .contains("container rules"));
}

#[test]
fn all_catalog_rows_and_page_controls_fit_the_scaled_dialog() {
    for (width, height) in [(320.0, 240.0), (800.0, 600.0), (1024.0, 768.0)] {
        let h = host();
        h.view.borrow_mut().items = catalog(12);
        let mut s = ui(h);
        load_panel(&mut s);
        s.set_screen_size(width, height);
        s.run(&format!("UIParent:SetPoint('TOPLEFT', nil, 'TOPLEFT', 0, 0); UIParent:SetWidth({width}); UIParent:SetHeight({height}); ShowUIPanel(BenillaAdminFrame)"))
            .unwrap();
        s.resolve();
        let quads = s.extract();
        let frame_rect = |name: &str| {
            quads
                .iter()
                .find_map(|quad| {
                    if matches!(quad.target, crate::order::ZTarget::Frame(_))
                        && s.quad_owner_name(quad.target).as_deref() == Some(name)
                    {
                        quad.rect
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| panic!("Missing layout for {name}"))
        };
        let dialog = frame_rect("BenillaAdminFrame");
        assert!(
            dialog.left >= 0.0 && dialog.right <= width + 0.01,
            "{dialog:?}"
        );
        assert!(
            dialog.bottom >= 0.0 && dialog.top <= height + 0.01,
            "{dialog:?}"
        );
        let mut names = vec![
            "BenillaAdminFrameSearch".to_string(),
            "BenillaAdminFramePrevious".to_string(),
            "BenillaAdminFrameNext".to_string(),
            "BenillaAdminFrameApply".to_string(),
            "BenillaAdminFrameAmount".to_string(),
        ];
        names.extend((1..=5).map(|i| format!("BenillaAdminResult{i}")));
        for name in names {
            let rect = frame_rect(&name);
            assert!(
                rect.left >= dialog.left
                    && rect.right <= dialog.right + 0.01
                    && rect.bottom >= dialog.bottom
                    && rect.top <= dialog.top + 0.01,
                "{name}: {rect:?} outside {dialog:?}"
            );
        }
        assert!(frame_rect("BenillaAdminResult5").bottom > frame_rect("BenillaAdminFrameNext").top);
        assert!(
            frame_rect("BenillaAdminFrameSearch").bottom > frame_rect("BenillaAdminResult1").top
        );
    }
}

#[path = "setup_tests.rs"]
mod setup_tests;
