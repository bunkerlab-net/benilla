use super::*;
use crate::glue::widgets::{FallbackFace, GlueBtn};

fn panel_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<GlueArt>()
        .init_resource::<Settings>()
        .init_resource::<crate::login::LoginForm>()
        .add_message::<MouseWheel>()
        .add_systems(
            Update,
            (render, crate::login::screen::refresh_boxes).chain(),
        );
    app.world_mut().spawn((
        Window {
            resolution: (800, 600).into(),
            ..default()
        },
        bevy::window::PrimaryWindow,
    ));
    let mut settings = app.world_mut().resource_mut::<Settings>();
    settings.open = true;
    settings.refresh_fields(&Cvars::default());
    settings.fields[1].value = "private sample passphrase".into();
    settings.fields[1].locked = false;
    settings.select(1);
    settings.edit.cursor = 3;
    settings.edit.sel_start = 2;
    settings.edit.sel_end = 5;
    app
}
fn root(app: &mut App) -> Entity {
    app.world_mut()
        .query_filtered::<Entity, With<Root>>()
        .single(app.world())
        .unwrap()
}

#[test]
fn artless_recovery_has_readable_fonts_actions_and_masked_input() {
    let mut app = panel_app();
    app.update();
    let first = root(&mut app);
    app.update();
    assert_eq!(
        first,
        root(&mut app),
        "an unchanged fallback tree should be reused"
    );
    assert!(app
        .world_mut()
        .query::<&TextFont>()
        .iter(app.world())
        .all(|f| f.font == Handle::<Font>::default()));
    assert!(app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .all(|t| !t.0.contains("private sample passphrase")));
    assert!(
        app.world_mut()
            .query_filtered::<Entity, (With<GlueBtn>, With<FallbackFace>)>()
            .iter(app.world())
            .count()
            >= 10
    );
    assert_eq!(
        app.world_mut()
            .query::<&Action>()
            .iter(app.world())
            .filter(|a| matches!(a, Action::Select(_)))
            .count(),
        PAGE_SIZE
    );
    let settings = app.world().resource::<Settings>();
    assert!(settings.config.is_none() && settings.changes.is_empty());
    assert_eq!(
        (
            settings.selected,
            settings.edit.cursor,
            settings.edit.sel_start,
            settings.edit.sel_end
        ),
        (Some(1), 3, 2, 5)
    );
}

#[test]
fn late_art_and_resize_rebuild_without_resetting_draft_focus_or_reveal() {
    let mut app = panel_app();
    app.update();
    let before = root(&mut app);
    {
        let mut settings = app.world_mut().resource_mut::<Settings>();
        settings.edit.password = false;
        settings.edit.set_text("unsaved secret draft");
        settings.edit.cursor = 4;
        settings.edit.sel_start = 2;
        settings.edit.sel_end = 6;
    }
    // Synthetic handles establish tree shape, never asset-backed visual evidence.
    {
        let mut art = app.world_mut().resource_mut::<GlueArt>();
        art.button_up = Some((Handle::default(), Vec2::new(256.0, 64.0)));
        let border = crate::glue::backdrop::BackdropEdges {
            left: Handle::default(),
            right: Handle::default(),
            top: Handle::default(),
            bottom: Handle::default(),
            tl: Handle::default(),
            tr: Handle::default(),
            bl: Handle::default(),
            br: Handle::default(),
        };
        art.dialog_bg = Some(Handle::default());
        art.dialog_border = Some(border.clone());
        art.tooltip_bg = Some(Handle::default());
        art.name_border = Some(border);
        art.tab_active = Some((Handle::default(), Vec2::new(128.0, 32.0)));
    }
    app.update();
    assert_ne!(before, root(&mut app));
    assert!(
        app.world_mut()
            .query_filtered::<&ImageNode, With<GlueBtn>>()
            .iter(app.world())
            .count()
            >= 10
    );
    assert_eq!(
        app.world_mut()
            .query_filtered::<Entity, With<crate::glue::backdrop::BackdropRig>>()
            .iter(app.world())
            .count(),
        5,
        "dialog, list, editor, input and status use shared backdrop rigs"
    );
    let art_root = root(&mut app);
    app.world_mut()
        .query::<&mut Window>()
        .single_mut(app.world_mut())
        .unwrap()
        .resolution
        .set(640.0, 700.0);
    app.update();
    assert_ne!(art_root, root(&mut app));
    let settings = app.world().resource::<Settings>();
    assert_eq!(settings.edit.text, "unsaved secret draft");
    assert_eq!(
        (
            settings.selected,
            settings.offset,
            settings.edit.cursor,
            settings.edit.sel_start,
            settings.edit.sel_end
        ),
        (Some(1), 0, 4, 2, 6)
    );
    assert!(!settings.edit.password);
    assert!(settings.config.is_none() && settings.changes.is_empty());
}

#[test]
fn long_diagnostics_keep_full_text_and_cannot_move_save_close_footer() {
    let mut app = panel_app();
    let message = "A detailed diagnostic line.\n".repeat(200);
    app.world_mut().resource_mut::<Settings>().status = message.clone();
    app.update();
    assert!(app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .any(|t| t.0 == message));
    let footer = app
        .world_mut()
        .query_filtered::<&Node, With<Footer>>()
        .single(app.world())
        .unwrap();
    let s = layout::scale(800.0, 600.0);
    assert_eq!(footer.top, Val::Px(layout::FOOTER[1] * s));
    assert_eq!(footer.height, Val::Px(layout::FOOTER[3] * s));
    let area = app
        .world_mut()
        .query_filtered::<Entity, With<ScrollArea>>()
        .single(app.world())
        .unwrap();
    app.world_mut().entity_mut(area).insert((
        RelativeCursorPosition {
            cursor_over: true,
            normalized: Some(Vec2::ZERO),
        },
        ComputedNode {
            size: Vec2::new(600.0, 60.0),
            content_size: Vec2::new(600.0, 1000.0),
            inverse_scale_factor: 1.0,
            ..default()
        },
    ));
    app.world_mut().write_message(MouseWheel {
        unit: MouseScrollUnit::Line,
        x: 0.0,
        y: -2.0,
        window: Entity::PLACEHOLDER,
    });
    app.update();
    assert_eq!(app.world().get::<ScrollPosition>(area).unwrap().0.y, 48.0);
    assert_eq!(
        app.world_mut()
            .query_filtered::<&Node, With<Footer>>()
            .single(app.world())
            .unwrap()
            .top,
        Val::Px(layout::FOOTER[1] * s)
    );
}
