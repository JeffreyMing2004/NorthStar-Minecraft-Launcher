pub fn show(ui: &mut egui::Ui, options: &mut crate::minecraft::LaunchOptions) {
    ui.checkbox(&mut options.custom_resolution, "指定游戏窗口尺寸");
    ui.add_enabled_ui(options.custom_resolution, |ui| {
        ui.horizontal(|ui| {
            ui.label("宽");
            ui.add(egui::DragValue::new(&mut options.width).range(320..=16384));
            ui.label("高");
            ui.add(egui::DragValue::new(&mut options.height).range(240..=16384));
        });
    });
    ui.checkbox(&mut options.fullscreen, "全屏启动");
    for (label, args) in [
        ("附加 JVM 参数", &mut options.jvm_args),
        ("附加游戏参数", &mut options.game_args),
    ] {
        ui.label(label);
        let mut text = args.join("\n");
        if ui
            .add(
                egui::TextEdit::multiline(&mut text)
                    .desired_rows(3)
                    .desired_width(ui.available_width()),
            )
            .changed()
        {
            *args = text
                .lines()
                .filter(|s| !s.trim().is_empty())
                .map(str::to_owned)
                .collect();
        }
    }
    ui.label("每行是一个完整参数，包含空格的参数也放在同一行，无需额外引号。");
    if let Err(e) = options.validate() {
        ui.label(e);
    }
}
