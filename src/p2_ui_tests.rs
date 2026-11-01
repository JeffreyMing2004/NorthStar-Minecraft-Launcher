use crate::{
    app::Page,
    store::{Appearance, ThemeChoice},
    theme,
};
use egui::{Event, Key, Modifiers, Pos2, RawInput, Rect, Vec2};

fn contrast(a: egui::Color32, b: egui::Color32) -> f32 {
    fn luminance(c: egui::Color32) -> f32 {
        let channel = |n: u8| {
            let v = n as f32 / 255.0;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b())
    }
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}
#[test]
fn themes_keep_readable_text_and_keyboard_navigation_does_not_launch_tasks() {
    let (_root, mut app, ctx) = crate::service::tests::fixture();
    for choice in [
        ThemeChoice::Dark,
        ThemeChoice::Light,
        ThemeChoice::HighContrast,
    ] {
        theme::apply_appearance(
            &ctx,
            &Appearance {
                theme: choice,
                ..Default::default()
            },
        );
        assert!(contrast(theme::body_text(), theme::card_bg()) >= 4.5);
        assert!(contrast(theme::on_accent(), theme::accent()) >= 4.5);
    }
    app.settings.appearance.scale = 1.25;
    let event = |key| Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::CTRL,
    };
    let mut output = ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0))),
            events: vec![event(Key::Num6), event(Key::Num0)],
            ..Default::default()
        },
        |ui| app.accessibility(ui.ctx()),
    );
    output.textures_delta.clear();
    assert!(app.page == Page::Settings);
    assert_eq!(app.settings.appearance.scale, 1.0);
    assert!(!app.service.busy());
    output.drop_without_applying_deltas();
}
#[test]
fn media_pages_and_external_login_render_in_small_windows_without_starting_mutation() {
    let (root, mut app, ctx) = crate::service::tests::fixture();
    let instance = crate::instances::Instance {
        id: "media-test".into(),
        name: "Media".into(),
        version_id: "test".into(),
        ..Default::default()
    };
    crate::instances::save(root.path(), &instance).unwrap();
    for screenshots in [false, true] {
        let mut output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(640.0, 360.0))),
                ..Default::default()
            },
            |ui| crate::instance_media::show(ui, &mut app, &instance, screenshots),
        );
        output.textures_delta.clear();
        for shape in &output.shapes {
            if let egui::Shape::Text(text) = &shape.shape
                && (text.galley.job.text == "备份所有存档" || text.galley.job.text == "最近 7 天")
            {
                assert!(text.pos.y < 360.0);
            }
        }
        assert!(!app.service.busy());
        output.drop_without_applying_deltas();
    }
    let mut output = ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(876.0, 390.0))),
            ..Default::default()
        },
        |ui| crate::external_auth_ui::show(ui, &mut app),
    );
    output.textures_delta.clear();
    assert!(!app.accounts.pending);
    assert!(!app.service.busy());
    let button = crate::service::tests::text_rect(&output, "读取认证服务器信息");
    assert!(button.max.x < 876.0 && button.max.y < 390.0);
    output.drop_without_applying_deltas();
}

fn key(key: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers,
    }
}

fn shell_frame(
    ctx: &egui::Context,
    app: &mut crate::app::NsclApp,
    size: Vec2,
    dpi: f32,
    events: Vec<Event>,
) -> egui::FullOutput {
    theme::apply_appearance(ctx, &app.settings.appearance);
    let mut input = RawInput {
        screen_rect: Some(Rect::from_min_size(
            Pos2::ZERO,
            size / app.settings.appearance.scale,
        )),
        events,
        ..Default::default()
    };
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .unwrap()
        .native_pixels_per_point = Some(dpi);
    ctx.run_ui(input, |ui| app.render_shell(ui, ctx))
}

#[test]
fn navigation_stays_visible_at_large_zoom_and_high_dpi_in_every_theme() {
    for choice in [
        ThemeChoice::Dark,
        ThemeChoice::Light,
        ThemeChoice::HighContrast,
    ] {
        for scale in [1.0, 1.75] {
            for dpi in [1.0, 2.0] {
                let (_root, mut app, ctx) = crate::service::tests::fixture();
                app.accounts.list = Default::default();
                app.settings.appearance = Appearance {
                    theme: choice,
                    scale,
                    reduced_motion: true,
                };
                for size in [Vec2::new(960.0, 640.0), Vec2::new(1200.0, 760.0)] {
                    for page in [
                        Page::Home,
                        Page::Settings,
                        Page::Instances,
                        Page::Downloads,
                        Page::Account,
                    ] {
                        app.page = page;
                        for _ in 0..2 {
                            shell_frame(&ctx, &mut app, size, dpi, vec![])
                                .drop_without_applying_deltas();
                        }
                        let output = shell_frame(&ctx, &mut app, size, dpi, vec![]);
                        let screen = Rect::from_min_size(Pos2::ZERO, size / scale);
                        let mut previous = Rect::NOTHING;
                        for item in Page::ALL {
                            let rect = crate::service::tests::text_rect(&output, item.title());
                            assert!(
                                screen.contains_rect(rect),
                                "{} outside {screen:?} at scale {scale}, dpi {dpi}: {rect:?}",
                                item.title()
                            );
                            assert!(!previous.intersects(rect), "navigation labels overlap");
                            previous = rect;
                        }
                        if page == Page::Settings {
                            let save = crate::service::tests::text_rect(&output, "保存设置");
                            assert!(
                                screen.contains_rect(save),
                                "settings save must remain reachable"
                            );
                        }
                        assert!((ctx.pixels_per_point() - dpi * scale).abs() < 0.01);
                        assert!(!app.service.busy());
                        output.drop_without_applying_deltas();
                    }
                }
            }
        }
    }
}

#[test]
fn tab_shift_tab_enter_and_space_activate_navigation_without_mouse() {
    let (_root, mut app, ctx) = crate::service::tests::fixture();
    app.accounts.list = Default::default();
    app.page = Page::Home;
    let size = Vec2::new(960.0, 640.0);
    for events in [
        vec![],
        vec![key(Key::Tab, Modifiers::NONE)],
        vec![key(Key::Tab, Modifiers::NONE)],
        vec![key(Key::Enter, Modifiers::NONE)],
    ] {
        shell_frame(&ctx, &mut app, size, 1.0, events).drop_without_applying_deltas();
    }
    assert!(app.page == Page::Account);
    shell_frame(
        &ctx,
        &mut app,
        size,
        1.0,
        vec![key(Key::Tab, Modifiers::SHIFT)],
    )
    .drop_without_applying_deltas();
    shell_frame(
        &ctx,
        &mut app,
        size,
        1.0,
        vec![key(Key::Space, Modifiers::NONE)],
    )
    .drop_without_applying_deltas();
    assert!(app.page == Page::Home);
    assert!(!app.service.busy());
}

#[test]
fn modal_focus_is_contained_escape_restores_the_opener_and_shortcuts_do_not_bypass_it() {
    let (_root, mut app, ctx) = crate::service::tests::fixture();
    let mut open = false;
    let mut allowed = Vec::new();
    let mut background = egui::Id::NULL;
    let mut value = String::new();
    {
        let mut frame = |events: Vec<Event>, open: &mut bool| {
            let output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(550.0, 360.0))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    background = ui.button("打开测试弹窗").id;
                    if !*open {
                        ctx.memory_mut(|m| m.request_focus(background));
                    }
                    if *open {
                        allowed.clear();
                        *open = crate::accessibility::dialog(&ctx, "测试焦点", 700.0, |ui| {
                            allowed.push(ui.text_edit_singleline(&mut value).id);
                            allowed.push(ui.button("确认测试").id);
                            allowed.push(ui.button("取消测试").id);
                        });
                    }
                    app.accessibility(&ctx);
                },
            );
            output.drop_without_applying_deltas();
            allowed.clone()
        };
        frame(vec![], &mut open);
        open = true;
        frame(vec![], &mut open);
        let close = ctx.memory(|m| m.focused()).unwrap();
        let mut focusable = frame(vec![], &mut open);
        focusable.push(close);
        for modifiers in [
            Modifiers::NONE,
            Modifiers::NONE,
            Modifiers::NONE,
            Modifiers::NONE,
            Modifiers::NONE,
            Modifiers::SHIFT,
            Modifiers::SHIFT,
        ] {
            frame(vec![key(Key::Tab, modifiers)], &mut open);
            assert!(
                focusable.contains(&ctx.memory(|m| m.focused()).unwrap()),
                "focus escaped the dialog"
            );
        }
        frame(vec![key(Key::Num6, Modifiers::CTRL)], &mut open);
        let outside = egui::pos2(2.0, 2.0);
        frame(
            vec![
                Event::PointerMoved(outside),
                Event::PointerButton {
                    pos: outside,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
                Event::PointerButton {
                    pos: outside,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                },
            ],
            &mut open,
        );
        assert!(open, "backdrop clicks must not discard form edits");
        frame(vec![key(Key::Escape, Modifiers::NONE)], &mut open);
    }
    assert!(app.page != Page::Settings);
    assert!(!open);
    assert_eq!(ctx.memory(|m| m.focused()), Some(background));
}
