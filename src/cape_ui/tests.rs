use super::*;
use egui::{Context, FullOutput, Pos2, RawInput, Rect};

fn entry(id: &str, active: bool) -> Entry {
    Entry {
        cape: Cape {
            id: id.into(),
            name: format!("Cape {id}"),
            url: "https://textures.minecraft.net/texture/test".into(),
            active,
        },
        pixels: Some(image::RgbaImage::new(64, 32)),
        texture: None,
        error: None,
    }
}

fn frame(
    ctx: &Context,
    view: &mut CapeView,
    editable: bool,
    events: Vec<egui::Event>,
) -> (FullOutput, Option<Command>) {
    let mut command = None;
    let output = ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(700.0, 500.0))),
            events,
            ..Default::default()
        },
        |ui| {
            command = view.rows(ui, editable);
        },
    );
    (output, command)
}

fn click(pos: Pos2) -> Vec<egui::Event> {
    let button = |pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    vec![egui::Event::PointerMoved(pos), button(true), button(false)]
}

fn radios(output: &FullOutput) -> Vec<(Pos2, bool)> {
    let circles: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Circle(circle) => Some(circle),
            _ => None,
        })
        .collect();
    let mut radios: Vec<_> = circles
        .iter()
        .filter(|circle| (5.0..=12.0).contains(&circle.radius))
        .map(|outer| {
            let selected = circles.iter().any(|inner| {
                inner.radius < outer.radius * 0.75
                    && inner.center.distance(outer.center) < 0.1
                    && inner.fill.a() > 0
            });
            (outer.center, selected)
        })
        .collect();
    radios.sort_by(|a, b| a.0.y.total_cmp(&b.0.y));
    radios
}

#[test]
fn cape_rows_mark_the_active_cape_and_only_request_unselected_choices() {
    let (_root, _app, ctx) = crate::service::tests::fixture();
    let mut view = CapeView::new();
    view.loaded = true;
    view.entries = vec![entry("first", true), entry("second", false)];
    let (mut output, command) = frame(&ctx, &mut view, true, vec![]);
    output.textures_delta.clear();
    assert!(command.is_none());
    let choices = radios(&output);
    output.drop_without_applying_deltas();
    assert_eq!(choices.len(), 3);
    assert_eq!(
        choices.iter().map(|c| c.1).collect::<Vec<_>>(),
        vec![false, true, false]
    );
    assert!(choices.iter().all(|c| c.0.x > 640.0 && c.0.x < 700.0));
    assert!(choices.iter().all(|c| (c.0.x - choices[0].0.x).abs() < 1.0));
    let selected = choices[1].0;
    let choose = choices[2].0;
    let (output, command) = frame(&ctx, &mut view, true, click(selected));
    output.drop_without_applying_deltas();
    assert!(command.is_none());
    let (output, command) = frame(&ctx, &mut view, true, click(choose));
    output.drop_without_applying_deltas();
    assert_eq!(command, Some(Command::Select(Some("second".into()))));
    assert!(view.entries[0].cape.active && !view.entries[1].cape.active);
    let (output, command) = frame(&ctx, &mut view, false, click(choose));
    output.drop_without_applying_deltas();
    assert!(command.is_none());
    let (output, _) = frame(&ctx, &mut view, true, vec![]);
    output.drop_without_applying_deltas();
    let (output, command) = frame(&ctx, &mut view, true, click(choices[0].0));
    output.drop_without_applying_deltas();
    assert_eq!(command, Some(Command::Select(None)));
}

#[test]
fn changing_accounts_discards_old_capes_and_late_worker_messages() {
    let mut accounts = AccountService::new();
    accounts.list = Default::default();
    let mut view = CapeView::new();
    view.account = Some("previous".into());
    view.loaded = true;
    view.entries = vec![entry("first", true)];
    view.pending = true;
    let previous_cancel = view.cancel.clone();
    view.sender
        .send((view.generation, Event::Listed(Err("old failure".into()))))
        .unwrap();
    view.sync(&mut accounts, false, true, &Context::default());
    assert!(view.entries.is_empty() && !view.pending && !view.loaded);
    assert!(view.status.is_empty());
    assert!(previous_cancel.load(Ordering::Relaxed));
    assert!(!view.cancel.load(Ordering::Relaxed));
}

#[test]
fn failed_switch_keeps_last_confirmed_selection_and_texture_errors_keep_rows() {
    let mut accounts = AccountService::new();
    accounts.list = Default::default();
    let mut view = CapeView::new();
    view.loaded = true;
    view.pending = true;
    view.entries = vec![entry("first", true), entry("second", false)];
    let generation = view.generation;
    view.sender
        .send((generation, Event::Listed(Err("网络不可用，请刷新".into()))))
        .unwrap();
    view.sender
        .send((
            generation,
            Event::Texture {
                id: "second".into(),
                url: view.entries[1].cape.url.clone(),
                result: Err("预览错误".into()),
            },
        ))
        .unwrap();
    view.sync(&mut accounts, false, false, &Context::default());
    assert!(!view.pending && view.entries.len() == 2);
    assert!(view.entries[0].cape.active && !view.entries[1].cape.active);
    assert_eq!(view.status, "网络不可用，请刷新");
    assert_eq!(view.entries[1].error.as_deref(), Some("预览错误"));
}

#[test]
fn offline_cape_tab_explains_login_without_starting_network_work() {
    let (_root, mut app, ctx) = crate::service::tests::fixture();
    app.account_tab = 2;
    app.settings.online_mode = false;
    app.accounts.list = Default::default();
    let output = ctx.run_ui(RawInput::default(), |ui| {
        crate::pages::account(ui, &mut app)
    });
    crate::service::tests::text_rect(&output, "请先选择 Microsoft 正版账号。");
    output.drop_without_applying_deltas();
    assert!(!app.capes.pending && !app.skin.pending && !app.accounts.pending);
}

#[test]
fn confirmed_server_selection_updates_radios_and_reuses_matching_previews() {
    let (_root, _app, ctx) = crate::service::tests::fixture();
    let mut view = CapeView::new();
    view.entries = vec![entry("first", true), entry("second", false)];
    view.set_capes(
        vec![entry("first", false).cape, entry("second", true).cape],
        "已确认".into(),
    );
    assert!(!view.entries[0].cape.active && view.entries[1].cape.active);
    assert!(view.entries.iter().all(|e| e.pixels.is_some()));
    let (output, _) = frame(&ctx, &mut view, true, vec![]);
    let choices = radios(&output);
    output.drop_without_applying_deltas();
    assert_eq!(
        choices.iter().map(|c| c.1).collect::<Vec<_>>(),
        vec![false, false, true]
    );
    view.set_capes(Vec::new(), "没有披风".into());
    let (output, _) = frame(&ctx, &mut view, true, vec![]);
    let choices = radios(&output);
    crate::service::tests::text_rect(&output, "当前账号没有可用披风。");
    output.drop_without_applying_deltas();
    assert_eq!(choices.len(), 1);
    assert!(choices[0].1);
}
