use egui::{Context, Id};

#[derive(Clone, Default)]
struct ReturnFocus(Option<Id>);

/// A bounded dialog with keyboard focus confined to its contents. Closing is
/// always the first, safe action; Tab moves on to the form or confirmation.
pub fn dialog(ctx: &Context, title: &str, width: f32, content: impl FnOnce(&mut egui::Ui)) -> bool {
    let key = Id::unique(("dialog-return-focus", title));
    let first = ctx.data(|d| d.get_temp::<ReturnFocus>(key).is_none());
    if first {
        let previous = ctx.memory(|m| m.focused());
        ctx.data_mut(|d| d.insert_temp(key, ReturnFocus(previous)));
    }
    let width = width.min((ctx.content_rect().width() - 48.0).max(160.0));
    let mut close = false;
    let id = Id::unique(("accessible-dialog", title));
    let area = egui::Modal::default_area(id)
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 16.0))
        .default_size(egui::vec2(width + 16.0, ctx.content_rect().height() - 32.0))
        .constrain_to(ctx.content_rect());
    let mut close_id = None;
    let response = egui::Modal::new(id).area(area).show(ctx, |ui| {
        ui.set_width(width);
        // Area remembers its previous size. Allow a form to grow again when a
        // choice reveals more fields, up to the current viewport limit.
        ui.set_max_height((ctx.content_rect().height() - 48.0).max(64.0));
        ui.horizontal(|ui| {
            ui.strong(title);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let button = ui.button("关闭");
                close_id = Some(button.id);
                if first {
                    button.request_focus();
                }
                close = button.clicked();
            });
        });
        ui.separator();
        egui::ScrollArea::both()
            .max_width(width)
            .max_height((ctx.content_rect().height() - 112.0).max(64.0))
            .auto_shrink([false, true])
            .show(ui, content);
    });
    // egui clears focus after Tab leaves the final widget. Wrap immediately
    // instead of requiring an extra Tab with no visible focus indicator.
    if response.is_top_modal
        && !egui::Popup::is_any_open(ctx)
        && ctx.memory(|m| m.focused().is_none())
        && let Some(id) = close_id
    {
        ctx.memory_mut(|m| m.request_focus(id));
    }
    // Do not dismiss a form on backdrop clicks: selecting popup options or a
    // layout change must not silently discard edits.
    let escape = response.is_top_modal
        && !response.any_popup_open
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    let open = !close && !escape;
    if !open {
        finish_dialog(ctx, title);
    }
    open
}

pub fn finish_dialog(ctx: &Context, title: &str) {
    let key = Id::unique(("dialog-return-focus", title));
    let previous = ctx.data_mut(|d| d.remove_temp::<ReturnFocus>(key));
    if let Some(ReturnFocus(Some(id))) = previous {
        ctx.memory_mut(|m| m.request_focus(id));
    }
}
