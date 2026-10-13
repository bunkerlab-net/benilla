use benilla_ui::script::UiScript;

fn shell() -> UiScript {
    let mut script = UiScript::new().unwrap();
    script.set_screen_size(1024.0, 768.0);
    script.run(r#"
        CreateFrame("Frame", "UIParent"):SetAllPoints()
        CreateFrame("Frame", "Hud", UIParent):SetAllPoints()
        CreateFrame("Frame", "BenillaOptionsFrame", UIParent):Hide()
        for _, name in ipairs({"GameTooltip", "DropDownList1", "DropDownList2",
            "StaticPopup1", "StaticPopup2", "StaticPopup3", "StaticPopup4"}) do
            CreateFrame("Frame", name, UIParent):Hide()
        end
        function BenillaOptionsFrame_UpdateScale() end
        function BenillaOptionsRow_Set(row, value) row.saved = value end
        function BenillaOptionsRow_SetEnabled(row, enabled)
            local check = getglobal(row:GetName() .. "Check")
            if enabled then check:Enable() else check:Disable() end
        end
        for _, name in ipairs({"InterfaceRowShowTutorials", "InterfaceRowShowCloak",
            "InterfaceRowShowHelm", "ActionBarsRowMultiBar1", "ActionBarsRowMultiBar2",
            "ActionBarsRowMultiBar3", "ActionBarsRowMultiBar4", "AudioRowSound"}) do
            local full = "BenillaOptionsFrameContainerBody" .. name
            local row = CreateFrame("Frame", full, BenillaOptionsFrame)
            CreateFrame("CheckButton", full .. "Check", row)
        end
        CreateFrame("CheckButton", "BenillaOptionsFrameContainerBodyKeybindingsCharacterRowCheck", BenillaOptionsFrame)
        BenillaOptionsFrameContainerBodyKeybindingsCharacterRowLabel = BenillaOptionsFrame:CreateFontString()
    "#).unwrap();
    script.run(include_str!("login_options.lua")).unwrap();
    script
}

#[test]
fn login_shell_shows_real_panel_and_scene_but_hides_hud() {
    let s = shell();
    assert!(s.frame_visible("BenillaOptionsFrame"));
    assert!(s.frame_visible("BenillaLoginOptionsScene"));
    assert!(!s.frame_visible("UIParent"));
    assert!(!s.frame_visible("Hud"));
    assert_eq!(
        s.eval::<String>("return BenillaOptionsFrame:GetParent():GetName()")
            .unwrap(),
        "BenillaLoginOptionsRoot"
    );
    assert!(s.errors().is_empty(), "{:?}", s.errors());
}

#[test]
fn login_shell_shared_overlays_remain_visible_outside_hidden_hud() {
    let s = shell();
    for name in [
        "GameTooltip",
        "DropDownList1",
        "DropDownList2",
        "StaticPopup1",
        "StaticPopup4",
    ] {
        s.run(&format!("{name}:Show()")).unwrap();
        assert!(s.frame_visible(name), "{name}");
        s.run(&format!("{name}:Hide()")).unwrap();
    }
}

#[test]
fn login_shell_disables_server_settings_and_defaults_cannot_write_them() {
    let s = shell();
    s.run(
        r#"
        local row = BenillaOptionsFrameContainerBodyInterfaceRowShowTutorials
        BenillaOptionsRow_SetEnabled(row, true)
        BenillaOptionsRow_Set(row, "1")
        BenillaOptionsRow_Set(BenillaOptionsFrameContainerBodyAudioRowSound, "1")
    "#,
    )
    .unwrap();
    assert!(s.eval::<bool>("return BenillaOptionsFrameContainerBodyInterfaceRowShowTutorialsCheck:IsEnabled() == 0").unwrap());
    assert!(s
        .eval::<bool>(
            "return BenillaOptionsFrameContainerBodyInterfaceRowShowTutorials.saved == nil"
        )
        .unwrap());
    assert_eq!(
        s.eval::<String>("return BenillaOptionsFrameContainerBodyAudioRowSound.saved")
            .unwrap(),
        "1"
    );
    assert!(s
        .eval::<bool>(
            "return BenillaOptionsFrameContainerBodyKeybindingsCharacterRowCheck:IsEnabled() == 0"
        )
        .unwrap());
}

#[test]
fn login_shell_follows_scale_and_survives_repeated_close_and_show() {
    let mut s = shell();
    for scale in [0.7, 1.0, 0.9] {
        s.run(&format!("UIParent:SetScale({scale})")).unwrap();
        s.tick(0.02);
        let actual: f64 = s.eval("return BenillaLoginOptionsRoot:GetScale()").unwrap();
        assert!((actual - scale).abs() < 0.001);
        s.run("BenillaOptionsFrame:Hide()").unwrap();
        assert!(!s.frame_visible("BenillaOptionsFrame"));
        s.run("BenillaOptionsFrame:Show()").unwrap();
        assert!(s.frame_visible("BenillaOptionsFrame"));
        assert!(!s.frame_visible("Hud"));
    }
    assert!(s.errors().is_empty(), "{:?}", s.errors());
}
