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
