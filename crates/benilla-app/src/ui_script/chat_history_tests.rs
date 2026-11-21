//! Wheel scrollback through the shipped layer and real UI engine, without install assets.
use benilla_ui::script::{QuadContent, UiScript};

#[test]
fn chat_wheel_recovers_history_without_capturing_clicks_or_other_panes() {
    let mut s = UiScript::new().unwrap();
    s.set_screen_size(800.0, 600.0);
    s.run(
        r#"
        NUM_CHAT_WINDOWS = 2
        for i = 1, 2 do
            local f = CreateFrame("ScrollingMessageFrame", "ChatFrame" .. i)
            f:SetPoint("BOTTOMLEFT", 20 + (i - 1) * 300, 20)
            f:SetWidth(250) f:SetHeight(100)
            f:EnableMouse(false)
            f:SetFont("synthetic.ttf", 12)
            f:SetMaxLines(100)
            f:SetTimeVisible(0) f:SetFadeDuration(0)
            for j = 1, 40 do f:AddMessage("History " .. j) end
        end
    "#,
    )
    .unwrap();
    let doc =
        benilla_ui::framexml::parse(include_str!("../../assets/ui/FrameXMLFixes.xml")).unwrap();
    let report = benilla_ui::loader::load(&s, &doc, &|_| None);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    s.resolve();
    s.tick(1.0);
    s.resolve();
    assert!(s.eval::<bool>("return ChatFrame1:AtBottom()").unwrap());
    assert_eq!(
        s.hit_test_name(100.0, 50.0),
        None,
        "clicks still reach the world"
    );
    s.mouse_wheel(100.0, 50.0, 1.0);
    assert!(!s.eval::<bool>("return ChatFrame1:AtBottom()").unwrap());
    assert!(s.eval::<bool>("return ChatFrame2:AtBottom()").unwrap());
    s.resolve();
    assert!(
        s.extract().iter().any(|quad| matches!(&quad.content,
            QuadContent::Text { text: Some(text), color: Some(color), .. }
                if text == "History 39" && color[3] == 1.0
        )),
        "scrolling restores an expired historical line to full opacity"
    );
    s.run("ChatFrame1:AddMessage('Incoming')").unwrap();
    assert!(
        !s.eval::<bool>("return ChatFrame1:AtBottom()").unwrap(),
        "incoming lines do not jump the view to the bottom"
    );
    s.mouse_wheel(700.0, 500.0, -1.0);
    assert!(
        !s.eval::<bool>("return ChatFrame1:AtBottom()").unwrap(),
        "wheel outside chat leaves history alone"
    );
    s.mouse_wheel(100.0, 50.0, -1.0);
    s.mouse_wheel(100.0, 50.0, -1.0);
    assert!(s.eval::<bool>("return ChatFrame1:AtBottom()").unwrap());
    assert_eq!(
        s.eval::<i64>("return ChatFrame1:GetNumMessages()").unwrap(),
        41
    );
    assert!(s.errors().is_empty(), "{:?}", s.errors());
}
