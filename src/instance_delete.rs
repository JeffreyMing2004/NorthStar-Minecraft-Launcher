use crate::{
    app::NsclApp,
    instances::{Instance, Isolation},
};
use std::path::{Path, PathBuf};

pub struct Dialog {
    root: PathBuf,
    instance: Instance,
    archive: Option<String>,
    acknowledged: bool,
    started: bool,
}

impl Dialog {
    pub fn new(root: &Path, instance: &Instance, archive: Option<&str>) -> Self {
        Self {
            root: root.to_path_buf(),
            instance: instance.clone(),
            archive: archive.map(str::to_owned),
            acknowledged: false,
            started: false,
        }
    }
}

pub fn show(ctx: &egui::Context, app: &mut NsclApp) {
    let Some(mut dialog) = app.instance_browser.delete.take() else {
        return;
    };
    if dialog.started && !app.service.busy() {
        crate::accessibility::finish_dialog(ctx, "删除实例确认");
        return;
    }
    let root = crate::store::game_directory(&app.settings);
    let editable = !app.service.busy() && app.service.game.is_none() && dialog.root == root;
    let mut delete = false;
    let mut cancel = false;
    let open = crate::accessibility::dialog(ctx, "删除实例确认", 560.0, |ui| {
        ui.strong(format!("确认删除「{}」？", dialog.instance.name));
        ui.label(
            "此操作会永久删除实例目录中的文件，无法通过归档列表恢复。请先备份需要保留的内容。",
        );
        let directory = match &dialog.archive {
            Some(entry) => dialog.root.join("archived-instances").join(entry),
            None => match dialog.instance.directory(&dialog.root) {
                Ok(path) => path,
                Err(error) => {
                    ui.label(error);
                    return;
                }
            },
        };
        ui.add_space(8.0);
        ui.label("将删除的目录：");
        ui.label(directory.display().to_string());
        if dialog.instance.isolation == Isolation::Isolated {
            ui.label("该目录中的存档、模组、配置、资源包和截图会一起删除。");
        } else {
            ui.label("只删除该实例配置及保留的独立目录；共享或原版本游戏目录中的存档、模组和配置会保留。");
        }
        ui.label("共用的 Minecraft 版本、加载器、资源库和下载缓存会保留，其他实例可以继续使用。");
        ui.add_space(8.0);
        if dialog.started {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("正在删除实例文件，请稍候……");
            });
        } else {
            ui.checkbox(&mut dialog.acknowledged, "我已了解，确认永久删除此实例");
            if dialog.root != root {
                ui.label("游戏目录已改变，请关闭后重新选择实例。");
            }
            ui.horizontal(|ui| {
                cancel = ui.button("取消").clicked();
                delete = ui
                    .add_enabled(
                        editable && dialog.acknowledged,
                        egui::Button::new("确认删除"),
                    )
                    .clicked();
            });
        }
    });
    if delete {
        app.service.delete_instance(
            dialog.instance.clone(),
            dialog.archive.clone(),
            &app.settings,
            ctx,
        );
        dialog.started = app.service.busy();
    }
    if open && !cancel {
        app.instance_browser.delete = Some(dialog);
    } else {
        crate::accessibility::finish_dialog(ctx, "删除实例确认");
    }
}
