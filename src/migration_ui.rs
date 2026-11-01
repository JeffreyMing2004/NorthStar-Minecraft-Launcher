use crate::{
    app::NsclApp,
    instances::{self, Instance},
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TryRecvError},
    },
};
enum Reply {
    Instances(PathBuf, Vec<Instance>),
    Preview(Box<instances::migration::Plan>),
}
#[derive(Default)]
pub struct Dialog {
    pub open: bool,
    path: String,
    source: Option<PathBuf>,
    instances: Vec<Instance>,
    selected: String,
    plan: Option<instances::migration::Plan>,
    pending: Option<Receiver<Result<Reply, String>>>,
    cancel: Arc<AtomicBool>,
    notice: String,
}
impl Dialog {
    fn start(&mut self, target: PathBuf, preview: bool, ctx: &egui::Context) {
        if self.pending.is_some() {
            return;
        }
        let settings = crate::store::Settings {
            game_directory: self.path.trim().trim_matches('"').into(),
            ..Default::default()
        };
        if settings.game_directory.is_empty() {
            self.notice = "请填写来源目录。".into();
            return;
        }
        let source = crate::store::game_directory(&settings);
        let instance = self
            .instances
            .iter()
            .find(|i| i.id == self.selected)
            .cloned();
        self.plan = None;
        self.notice.clear();
        self.cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        self.pending = Some(rx);
        let cancel = self.cancel.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let d = crate::minecraft::Downloader::new(
                    Default::default(),
                    cancel,
                    Arc::new(|_| {}),
                )?;
                if preview {
                    instances::migration::inspect(
                        &source,
                        &target,
                        &instance.ok_or("请先选择来源实例。")?,
                        &d,
                    )
                    .map(|p| Reply::Preview(Box::new(p)))
                } else {
                    let list = instances::list(&source);
                    if list.is_empty() {
                        Err("来源目录中没有可识别的已安装版本或实例。".into())
                    } else {
                        Ok(Reply::Instances(source, list))
                    }
                }
            })();
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }
}
pub fn show(ctx: &egui::Context, app: &mut NsclApp) {
    if !app.migration.open {
        return;
    }
    let mut dialog = std::mem::take(&mut app.migration);
    if let Some(rx) = &dialog.pending {
        match rx.try_recv() {
            Ok(result) => {
                dialog.pending = None;
                match result {
                    Ok(Reply::Instances(root, items)) => {
                        dialog.source = Some(root);
                        dialog.selected = items[0].id.clone();
                        dialog.instances = items;
                    }
                    Ok(Reply::Preview(plan)) => dialog.plan = Some(*plan),
                    Err(e) => dialog.notice = e,
                }
            }
            Err(TryRecvError::Disconnected) => {
                dialog.pending = None;
                dialog.notice = "读取任务结束，请重试。".into();
            }
            Err(TryRecvError::Empty) => {}
        }
    }
    if dialog
        .plan
        .as_ref()
        .is_some_and(|p| app.service.instances.iter().any(|i| i.id == p.instance.id))
    {
        dialog.plan = None;
        dialog.notice = "已完成复制迁入，来源保持不变。".into();
    }
    let mut open = dialog.open;
    let mut scan = false;
    let mut preview = false;
    let mut reuse = false;
    let mut apply = false;
    egui::Window::new("迁入已有实例").open(&mut open).default_width(660.0).vscroll(true).max_height((ctx.content_rect().height()-64.0).max(250.0)).show(ctx,|ui| {
        ui.label("先退出原启动器与游戏。复用会继续使用原目录；复制会在当前 NSCL 游戏目录建立独立实例。");
        let editable=dialog.pending.is_none() && !app.service.busy() && app.service.game.is_none();
        ui.add_enabled_ui(editable,|ui| {
            ui.label("来源 .minecraft 或启动器游戏目录");
            if ui.text_edit_singleline(&mut dialog.path).changed(){dialog.source=None;dialog.instances.clear();dialog.plan=None;}
            if ui.button("扫描来源目录").clicked(){scan=true;}
            egui::ComboBox::from_id_salt("migration-instance").selected_text(dialog.instances.iter().find(|i| i.id==dialog.selected).map(|i| i.name.as_str()).unwrap_or("选择来源实例")).show_ui(ui,|ui|{
                for i in &dialog.instances { if ui.selectable_value(&mut dialog.selected,i.id.clone(),&i.name).changed(){dialog.plan=None;} }
            });
            if let Some(source)=&dialog.source {ui.label(format!("实际来源：{}",source.display()));}
            ui.label(format!("复制目标：{}",crate::store::game_directory(&app.settings).display()));
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(dialog.source.is_some(),egui::Button::new("复用原目录（不复制）")).clicked(){reuse=true;}
                if ui.add_enabled(!dialog.instances.is_empty(),egui::Button::new("预览复制到 NSCL")).clicked(){preview=true;}
            });
            if let Some(plan)=&mut dialog.plan {
                ui.separator();ui.label(format!("运行文件 {} 项，实例内容 {} 项，共 {:.1} MiB",plan.runtime_files,plan.content_files,plan.bytes as f64/1048576.0));
                ui.label("范围：此版本及继承版本的运行文件、所需资源与库，选中实例的模组/配置/存档/截图等内容；不复制账号、凭据、日志与下载缓存。");
                ui.label("新实例名称");ui.text_edit_singleline(&mut plan.instance.name);
                if ui.button("确认复制迁入").clicked(){apply=true;}
            }
        });
        if dialog.pending.is_some(){ui.spinner();if ui.button("取消预览").clicked(){dialog.cancel.store(true,Ordering::Relaxed);}}
        if app.service.busy(){ui.label(&app.service.progress.stage);if ui.button("取消迁入").clicked(){app.service.cancel();}}
        if !dialog.notice.is_empty(){ui.label(&dialog.notice);}
        if let Some(e)=&app.service.error {ui.label(e);}
    });
    if scan {
        dialog.start(crate::store::game_directory(&app.settings), false, ctx);
    }
    if preview {
        dialog.start(crate::store::game_directory(&app.settings), true, ctx);
    }
    if reuse && let Some(root) = &dialog.source {
        let result = crate::store::add_game_directory(&mut app.settings, &root.to_string_lossy())
            .and_then(|_| crate::store::save(&app.settings));
        match result {
            Ok(()) => {
                app.service.refresh(&app.settings);
                app.service.selected = dialog.selected.clone();
                open = false;
            }
            Err(e) => dialog.notice = e,
        }
    }
    if apply && let Some(plan) = &dialog.plan {
        app.service.copy_migration(plan.clone(), &app.settings, ctx);
    }
    if !open {
        dialog.cancel.store(true, Ordering::Relaxed);
    }
    dialog.open = open;
    app.migration = dialog;
}
