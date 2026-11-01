use crate::{app::NsclApp, diagnostics::Report};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};

pub struct Viewer {
    path: PathBuf,
    report: Option<Report>,
    pending: Option<Receiver<Result<Report, String>>>,
    last_read: Instant,
    query: String,
    level: u8,
    source: usize,
    live: bool,
    follow: bool,
    error: String,
}
impl Viewer {
    pub fn new(path: PathBuf) -> Self {
        let mut v = Self {
            path,
            report: None,
            pending: None,
            last_read: Instant::now(),
            query: String::new(),
            level: 0,
            source: 0,
            live: true,
            follow: true,
            error: String::new(),
        };
        v.refresh();
        v
    }
    fn refresh(&mut self) {
        if self.pending.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let path = self.path.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Report::load(&path).map_err(|e| e.to_string()));
        });
        self.pending = Some(rx);
        self.last_read = Instant::now();
    }
    fn poll(&mut self) {
        if let Some(rx) = &self.pending {
            match rx.try_recv() {
                Ok(result) => {
                    self.pending = None;
                    match result {
                        Ok(r) => {
                            self.report = Some(r);
                            self.error.clear();
                        }
                        Err(e) => self.error = e,
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending = None;
                    self.error = "日志读取线程已结束，请刷新重试。".into();
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.live && self.last_read.elapsed() >= Duration::from_millis(500) {
            self.refresh();
        }
    }
}

fn matches(line: &str, query: &str, level: u8) -> bool {
    let text = line.to_lowercase();
    if !text.contains(query) {
        return false;
    }
    let error = ["error", "fatal", "exception", "caused by"]
        .iter()
        .any(|s| text.contains(s));
    match level {
        1 => error || text.contains("warn"),
        2 => error,
        _ => true,
    }
}

pub fn controls(ui: &mut egui::Ui, app: &mut NsclApp) {
    if app.service.game.is_some() {
        if ui.button("停止游戏").clicked() {
            app.service.stop_game(false);
        }
        if ui.button("强制结束").clicked() {
            app.force_stop_confirm = true;
        }
    }
}

pub fn show(ctx: &egui::Context, app: &mut NsclApp) {
    if app.service.game.is_none() {
        app.force_stop_confirm = false;
    }
    if app.force_stop_confirm {
        let mut open = true;
        egui::Window::new("确认强制结束游戏")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(
                    "强制结束当前由 NSCL 启动的游戏可能丢失尚未保存的进度。建议先尝试停止游戏。",
                );
                ui.horizontal(|ui| {
                    if ui.button("确认强制结束").clicked() {
                        app.service.stop_game(true);
                        app.force_stop_confirm = false;
                    }
                    if ui.button("继续运行").clicked() {
                        app.force_stop_confirm = false;
                    }
                });
            });
        if !open {
            app.force_stop_confirm = false;
        }
    }
    let Some(v) = app.log_view.as_mut() else {
        return;
    };
    v.poll();
    ctx.request_repaint_after(Duration::from_millis(250));
    let mut open = true;
    egui::Window::new("日志与故障诊断")
        .open(&mut open)
        .default_width(840.0)
        .vscroll(true)
        .max_height((ctx.content_rect().height() - 64.0).max(240.0))
        .show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("刷新日志").clicked() {
                    v.refresh();
                }
                ui.checkbox(&mut v.live, "实时刷新");
                ui.checkbox(&mut v.follow, "跟随末尾");
                if let Some(report) = &v.report
                    && ui.button("导出脱敏诊断包").clicked()
                {
                    match crate::file_picker::save_diagnostics() {
                        Ok(Some(path)) => match report.export(&path) {
                            Ok(()) => app.log_export_status = "脱敏诊断包已导出。".into(),
                            Err(e) => app.log_export_status = e,
                        },
                        Ok(None) => {}
                        Err(e) => app.log_export_status = e,
                    }
                }
            });
            ui.label("每份日志最多显示末尾 64 KiB；诊断仅供参考。导出包仅含固定错误特征与建议。");
            if !v.error.is_empty() {
                ui.label(&v.error);
            }
            if !app.log_export_status.is_empty() {
                ui.label(&app.log_export_status);
            }
            let Some(report) = &v.report else {
                ui.spinner();
                return;
            };
            egui::CollapsingHeader::new("故障诊断与证据")
                .default_open(true)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("findings")
                        .max_height(130.0)
                        .show(ui, |ui| {
                            if report.findings.is_empty() {
                                ui.label("未识别出常见错误特征，请继续查看原始日志。");
                            }
                            for f in &report.findings {
                                ui.strong(format!("可能原因：{}", f.reason));
                                ui.label(format!("处理建议：{}", f.advice));
                                ui.label(format!("{}：{}", f.source, f.line));
                            }
                        });
                });
            if report.logs.is_empty() {
                return;
            }
            v.source = v.source.min(report.logs.len() - 1);
            ui.horizontal_wrapped(|ui| {
                egui::ComboBox::from_id_salt("log-source")
                    .selected_text(report.logs[v.source].name)
                    .show_ui(ui, |ui| {
                        for (i, log) in report.logs.iter().enumerate() {
                            ui.selectable_value(&mut v.source, i, log.name);
                        }
                    });
                ui.add(
                    egui::TextEdit::singleline(&mut v.query)
                        .hint_text("搜索日志")
                        .desired_width(200.0),
                );
                egui::ComboBox::from_id_salt("log-level")
                    .selected_text(["全部级别", "警告与错误", "仅错误"][v.level as usize])
                    .show_ui(ui, |ui| {
                        for (i, label) in
                            ["全部级别", "警告与错误", "仅错误"].into_iter().enumerate()
                        {
                            ui.selectable_value(&mut v.level, i as u8, label);
                        }
                    });
            });
            let query = v.query.to_lowercase();
            let lines: Vec<_> = report.logs[v.source]
                .text
                .lines()
                .filter(|l| matches(l, &query, v.level))
                .collect();
            ui.label(format!(
                "{} 行匹配；取消「跟随末尾」可暂停自动滚动。",
                lines.len()
            ));
            egui::ScrollArea::both()
                .id_salt(("live-log", v.source))
                .max_height(360.0)
                .stick_to_bottom(v.follow)
                .show_rows(ui, 17.0, lines.len(), |ui, range| {
                    for i in range {
                        ui.add(
                            egui::Label::new(egui::RichText::new(lines[i]).monospace()).extend(),
                        );
                    }
                });
        });
    if !open {
        app.log_view = None;
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn log_filters_combine_search_and_severity() {
        assert!(super::matches(
            "[ERROR] Missing dependency Fabric",
            "fabric",
            2
        ));
        assert!(!super::matches("[INFO] Fabric loaded", "fabric", 2));
        assert!(super::matches("[WARN] optional missing", "optional", 1));
        assert!(!super::matches("[WARN] optional missing", "other", 0));
    }
}
