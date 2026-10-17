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
    settings.fields[1].saved_value = "private sample passphrase".into();
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
            >= 8
    );
    assert_eq!(
        app.world_mut()
            .query::<&Action>()
            .iter(app.world())
            .filter(|a| matches!(a, Action::Select(_)))
            .count(),
        7
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
            >= 8
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
            settings.group,
            settings.edit.cursor,
            settings.edit.sel_start,
            settings.edit.sel_end
        ),
        (Some(1), FieldGroup::Connection, 4, 2, 6)
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

#[test]
fn stock_frame_tints_do_not_turn_the_tooltip_mask_into_a_grey_panel() {
    let mut app = App::new();
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
    let mut art = GlueArt::default();
    art.tooltip_bg = Some(Handle::default());
    art.name_border = Some(border);
    let skin = Skin {
        art: &art,
        font: Handle::default(),
        edit_font: Handle::default(),
        s: 1.0,
        border_scale: 1.0,
    };
    let mut queue = bevy::ecs::world::CommandQueue::default();
    Commands::new(&mut queue, app.world())
        .spawn_empty()
        .with_children(|ui| skin.frame(ui, false));
    queue.apply(app.world_mut());
    let images: Vec<_> = app
        .world_mut()
        .query::<&ImageNode>()
        .iter(app.world())
        .collect();
    assert_eq!(images.len(), 9);
    assert_eq!(
        images
            .iter()
            .filter(|image| image.color == BOX_FILL)
            .count(),
        1
    );
    assert_eq!(
        images
            .iter()
            .filter(|image| image.color == BOX_BORDER)
            .count(),
        8
    );
}

#[test]
fn repeated_navigation_and_reopening_keeps_one_panel_and_all_actions() {
    let mut app = panel_app();
    for _ in 0..3 {
        for group in [
            FieldGroup::Connection,
            FieldGroup::Iroh,
            FieldGroup::World,
            FieldGroup::Encounter,
            FieldGroup::Connection,
        ] {
            {
                let mut settings = app.world_mut().resource_mut::<Settings>();
                settings.open = true;
                settings.select_group(group);
                settings.refresh = true;
            }
            app.update();
            app.update();
            root(&mut app);
            let actions: Vec<_> = app
                .world_mut()
                .query::<&Action>()
                .iter(app.world())
                .collect();
            assert_eq!(
                actions.iter().filter(|a| matches!(a, Action::Save)).count(),
                1
            );
            assert_eq!(
                actions
                    .iter()
                    .filter(|a| matches!(a, Action::Cancel))
                    .count(),
                1
            );
            assert_eq!(
                actions
                    .iter()
                    .filter(|a| matches!(a, Action::Group(_)))
                    .count(),
                4
            );
            let settings = app.world().resource::<Settings>();
            assert_eq!(settings.group, group);
            assert_eq!(
                actions
                    .iter()
                    .filter(|a| matches!(a, Action::Select(_)))
                    .count(),
                settings
                    .fields
                    .iter()
                    .filter(|field| FieldGroup::for_field(&field.name) == group)
                    .count()
            );
        }
        app.world_mut().resource_mut::<Settings>().open = false;
        app.update();
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<Root>>()
                .iter(app.world())
                .count(),
            0
        );
        assert_eq!(
            app.world_mut().query::<&Action>().iter(app.world()).count(),
            0
        );
    }
}

#[test]
fn header_uses_stock_small_font_and_panel_blocks_login_text_bleed() {
    let mut app = panel_app();
    app.update();
    let s = layout::scale(800.0, 600.0);
    let title = app
        .world_mut()
        .query::<(&Text, &TextFont)>()
        .iter(app.world())
        .find(|(text, _)| text.0 == "Connection setup")
        .unwrap();
    assert_eq!(title.1.font_size, 12.0 * s);
    assert!(app
        .world_mut()
        .query::<(&BackgroundColor, &Node)>()
        .iter(app.world())
        .any(|(color, node)| color.0 == Color::BLACK
            && node.left == Val::Px(12.0 * s)
            && node.right == Val::Px(12.0 * s)
            && node.top == Val::Px(12.0 * s)
            && node.bottom == Val::Px(12.0 * s)));
}

#[test]
fn every_startup_setting_renders_its_own_brief_explanation() {
    let mut app = panel_app();
    let mut explanations = std::collections::HashSet::new();
    for (index, (name, _, _)) in STARTUP.iter().enumerate() {
        let help = field_help(name).unwrap_or_else(|| panic!("missing help for {name}"));
        assert!(explanations.insert(help), "generic reused help for {name}");
        assert!(help.len() <= 200, "keep {name} help brief");
        {
            let mut settings = app.world_mut().resource_mut::<Settings>();
            settings.select(index);
        }
        app.update();
        assert!(app
            .world_mut()
            .query::<&Text>()
            .iter(app.world())
            .any(|text| text.0 == help));
    }
    assert_eq!(field_help("unknown.setting"), None);
}

#[test]
fn field_text_stack_is_centered_and_inset_for_selected_and_unselected_rows() {
    let mut app = panel_app();
    // The stock texture stays in use; handles establish geometry, not pixel rendering.
    app.world_mut().resource_mut::<GlueArt>().title_highlight = Some(Handle::default());
    app.update();
    let s = layout::scale(800.0, 600.0);
    let world = app.world_mut();
    let mut rows = world.query::<(&Action, &Children, &Node)>();
    let rows: Vec<_> = rows
        .iter(world)
        .filter_map(|(action, children, node)| {
            if let Action::Select(index) = action {
                Some((*index, children.iter().collect::<Vec<_>>(), node.clone()))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(rows.len(), 7);
    for (index, children, row) in rows {
        assert_eq!(row.height, Val::Px(layout::FIELD_HEIGHT * s));
        let texts: Vec<_> = children
            .iter()
            .filter_map(|id| {
                world.get::<Text>(*id)?;
                world.get::<Node>(*id)
            })
            .collect();
        assert_eq!(texts.len(), 2);
        for node in &texts {
            assert_eq!(node.left, Val::Px(layout::FIELD_TEXT_LEFT * s));
            assert_eq!(node.width, Val::Px(layout::FIELD_TEXT_WIDTH * s));
        }
        assert_eq!(texts[0].top, Val::Px(layout::FIELD_TEXT_TOP * s));
        assert_eq!(
            texts[1].top,
            Val::Px((layout::FIELD_TEXT_TOP + layout::FIELD_LABEL_HEIGHT) * s)
        );
        assert_eq!(texts[0].height, Val::Px(layout::FIELD_LABEL_HEIGHT * s));
        assert_eq!(texts[1].height, Val::Px(layout::FIELD_VALUE_HEIGHT * s));
        assert_eq!(
            children
                .iter()
                .filter(|id| world.get::<ImageNode>(**id).is_some())
                .count(),
            usize::from(index == 1)
        );
    }
    assert_eq!(
        layout::FIELD_TEXT_TOP * 2.0 + layout::FIELD_LABEL_HEIGHT + layout::FIELD_VALUE_HEIGHT,
        layout::FIELD_HEIGHT
    );
}

#[test]
fn booleans_use_checkboxes_instead_of_text_inputs_and_groups_have_no_paging() {
    let mut app = panel_app();
    for name in [
        "p2p.enabled",
        "p2p.iroh.enabled",
        "p2p.iroh.port_mapping",
        "p2p.encounter.enabled",
    ] {
        let index = app
            .world()
            .resource::<Settings>()
            .fields
            .iter()
            .position(|field| field.name == name)
            .unwrap();
        app.world_mut().resource_mut::<Settings>().select(index);
        app.update();
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<Edit>>()
                .iter(app.world())
                .count(),
            0
        );
        assert_eq!(
            app.world_mut()
                .query::<&Action>()
                .iter(app.world())
                .filter(|action| matches!(action, Action::ToggleBoolean(i) if *i == index))
                .count(),
            1
        );
        assert_eq!(
            app.world_mut()
                .query::<&Action>()
                .iter(app.world())
                .filter(|action| matches!(action, Action::Group(_)))
                .count(),
            4
        );
        assert!(app
            .world_mut()
            .query::<&Text>()
            .iter(app.world())
            .all(|text| text.0 != "Next" && text.0 != "Previous" && !text.0.starts_with("Page ")));
    }
}

#[test]
fn live_apply_requires_explicit_disconnect_controls() {
    let mut app = panel_app();
    app.world_mut().resource_mut::<Settings>().apply_status =
        Some(crate::net::reconfigure::ApplyStatus::NeedsDisconnect);
    app.update();
    for confirm in [true, false] {
        assert_eq!(
            app.world_mut()
                .query::<&Action>()
                .iter(app.world())
                .filter(|action| if confirm {
                    matches!(action, Action::ConfirmApply)
                } else {
                    matches!(action, Action::KeepConnection)
                })
                .count(),
            1
        );
    }
}

#[test]
fn unsaved_indicator_detects_uncommitted_typing_while_apply_is_pending() {
    let mut app = panel_app();
    app.world_mut().resource_mut::<Settings>().apply_status =
        Some(crate::net::reconfigure::ApplyStatus::Pending);
    app.update();
    assert!(app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .any(|text| text.0 == "No unsaved changes"));
    let before = root(&mut app);
    app.world_mut()
        .resource_mut::<Settings>()
        .edit
        .set_text("new unsaved draft");
    app.update();
    assert_ne!(root(&mut app), before);
    assert!(app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .any(|text| text.0 == "Unsaved draft changes"));
    assert!(
        !app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .any(|text| text.0.contains("new unsaved draft")),
        "secret remains masked while dirty state refreshes"
    );
}
