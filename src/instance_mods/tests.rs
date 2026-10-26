use super::*;
use crate::service::tests::text_rect;
use egui::{Context, Event, FullOutput, RawInput};

fn frame(
    ctx: &Context,
    offset: usize,
    total: usize,
    busy: bool,
    events: Vec<Event>,
) -> (FullOutput, Option<usize>) {
    let mut requested = None;
    let mut output = ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(600.0, 120.0))),
            events,
            ..Default::default()
        },
        |ui| {
            requested = pagination(ui, offset, total, busy);
        },
    );
    output.textures_delta.clear();
    (output, requested)
}

fn click(ctx: &Context, offset: usize, total: usize, busy: bool, label: &str) -> Option<usize> {
    frame(ctx, offset, total, busy, vec![])
        .0
        .drop_without_applying_deltas();
    let output = frame(ctx, offset, total, busy, vec![]).0;
    let pos = text_rect(&output, label).center();
    output.drop_without_applying_deltas();
    let (output, requested) = frame(
        ctx,
        offset,
        total,
        busy,
        vec![
            Event::PointerMoved(pos),
            Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
            Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            },
        ],
    );
    output.drop_without_applying_deltas();
    requested
}

#[test]
fn paging_clicks_request_other_results_and_respect_boundaries_and_busy_state() {
    let ctx = Context::default();
    crate::theme::apply(&ctx);
    assert_eq!(click(&ctx, 0, 45, false, "下一页"), Some(20));
    assert_eq!(click(&ctx, 20, 45, false, "下一页"), Some(40));
    assert_eq!(click(&ctx, 40, 45, false, "上一页"), Some(20));
    assert_eq!(click(&ctx, 0, 45, false, "上一页"), None);
    assert_eq!(click(&ctx, 40, 45, false, "下一页"), None);
    assert_eq!(click(&ctx, 0, 0, false, "下一页"), None);
    assert_eq!(click(&ctx, 20, 45, true, "上一页"), None);
    assert_eq!(click(&ctx, 20, 45, true, "下一页"), None);
}
