use crate::{
    app::{self, NsclApp, Page},
    pages::{download_button, task_status},
    theme,
};
use egui::RichText;

pub fn show(ctx: &egui::Context, app: &mut NsclApp) {
    if !app.service.content.manager_open {
        return;
    }
    let mut open = true;
    let mut refresh = None;
    let mut update = None;
    let mut remove = false;
    let mut inspect_file = None;
    let busy = app.service.busy();
    let catalog = app.service.content.managed.clone();
    egui::Window::new("已安装模组")
        .open(&mut open).default_width(700.0).default_height(450.0).max_height(520.0)
        .show(ctx,|ui| {
            if let Some(catalog)=&catalog {app::body(ui,&catalog.target.label());}
            ui.add_space(12.0);
            theme::toolbar(ui, |ui| {
                if download_button(ui,"刷新列表",!busy) {refresh=Some(false);}
                if download_button(ui,"检查更新",!busy && catalog.as_ref().is_some_and(|c| !c.entries.is_empty())) {refresh=Some(true);}
                if download_button(ui,"卸载所选",!busy && app.service.game.is_none() && !app.service.content.managed_selection.is_empty()) {remove=true;}
            });
            ui.add_space(10.0);
            if app.service.content.updates_checked {
                app::hint(ui,&format!("发现 {} 项更新 · 自动检查仅选择适用于当前实例的较新正式版",app.service.content.updates.available.len()));
            } else {app::hint(ui,"更新前先查看依赖方案；卸载前检查已登记模组的必需依赖。");}
            ui.add_space(12.0);
            let reserve=if busy {108.0}else{40.0};
            let height=(ui.available_height()-reserve).clamp(60.0,300.0);
            egui::ScrollArea::vertical().id_salt("managed-mod-list").max_height(height).min_scrolled_height(0.0).auto_shrink([false,false]).show(ui,|ui| {
                if let Some(catalog)=&catalog {
                    if catalog.entries.is_empty() {
                        app::body(ui, if catalog.other_files.is_empty() {"此实例尚无模组文件，可从下载页安装。"} else {"此实例的模组来自整合包或手动添加，文件已在下方列出。"});
                    }
                    for entry in &catalog.entries {
                        let version=&entry.installed.version;
                        egui::Frame::NONE.fill(theme::TOPBAR_BG).corner_radius(8.0).inner_margin(12).show(ui,|ui| {
                            let mut selected=app.service.content.managed_selection.contains(&version.project_id);
                            ui.horizontal(|ui| {
                                if ui.add_enabled(!busy,egui::Checkbox::without_text(&mut selected)).changed() {
                                    if selected {app.service.content.managed_selection.insert(version.project_id.clone());}else{app.service.content.managed_selection.remove(&version.project_id);}
                                }
                                ui.add(egui::Label::new(RichText::new(&version.name).size(15.0).color(theme::HEADING)).truncate());
                            });
                            theme::toolbar(ui, |ui| {
                                ui.label(RichText::new(&version.version_number).size(12.0).color(theme::MUTED));
                                if !entry.intact {ui.label(RichText::new("文件缺失或已修改").size(12.0).color(theme::ACCENT));}
                                if let Some(candidate)=app.service.content.updates.available.iter().find(|u|u.project_id==version.project_id) {
                                    ui.label(RichText::new(format!("→ {}",candidate.available.version_number)).size(12.0).color(theme::ACCENT));
                                    if ui.add_enabled(!busy && app.service.game.is_none(),egui::Button::new("查看更新方案")).clicked() {update=Some(candidate.available.clone());}
                                }
                            });
                            ui.add(egui::Label::new(RichText::new(&entry.installed.file.path).size(11.0).color(theme::HINT)).truncate());
                        });
                        ui.add_space(8.0);
                    }
                    if !catalog.other_files.is_empty() {
                        egui::CollapsingHeader::new(format!("其他本地模组（{} 个）",catalog.other_files.len())).default_open(catalog.entries.is_empty()).show(ui,|ui| {
                            app::hint(ui,"这些 JAR 没有 NSCL 单独安装记录，不影响游戏读取。暂不支持逐个检查更新或卸载；点击文件名查看位置和大小。");
                            for name in &catalog.other_files {
                                if ui.add(egui::Label::new(RichText::new(name).color(theme::ACCENT)).truncate().sense(egui::Sense::click()))
                                    .on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("在文件页查看位置和大小").clicked() {inspect_file=Some(name.clone());}
                            }
                        });
                    }
                } else {app::body(ui,if busy {"正在读取模组列表……"}else{"点击刷新列表读取当前实例。"});}
                for warning in &app.service.content.updates.warnings {ui.label(RichText::new(warning).size(12.0).color(theme::MUTED));}
            });
            ui.add_space(10.0);
            if let Some(catalog)=&catalog {
                theme::toolbar(ui, |ui| {
                    app::hint(ui,&format!("单独安装 {} 个 · 其他本地 {} 个 · 已选 {} 个",catalog.entries.len(),catalog.other_files.len(),app.service.content.managed_selection.len()));
                    if ui.button("打开 mods 目录").clicked() {open_mods(app,catalog);}
                });
            }
            if busy {task_status(ui,app);}
        });
    if !open {
        app.service.content.manager_open = false;
    }
    if let Some(name) = inspect_file
        && let Some(catalog) = &catalog
    {
        app.service.content.manager_open = false;
        app.page = Page::Instances;
        app.instance_browser.selected = catalog.target.instance.id.clone();
        app.instance_browser.filter = crate::instance_ui::Filter::All;
        app.instance_browser.tab = crate::instance_ui::DetailTab::Files;
        app.instance_browser
            .files
            .select(crate::instance_files::Folder::Mods, Some(name.into()));
    }
    if let Some(check) = refresh {
        app.service.content_manage(&app.settings, ctx, check);
    }
    if app.page == Page::Instances && (update.is_some() || remove) {
        app.page = Page::Downloads;
        app.category = 2;
    }
    if let Some(version) = update {
        app.service.content_preview(version, &app.settings, ctx);
    }
    if remove {
        app.service.content_remove_preview(&app.settings, ctx);
    }
}

fn open_mods(app: &mut NsclApp, catalog: &crate::content::ManagedCatalog) {
    let result = catalog
        .target
        .instance
        .game_directory(&crate::store::game_directory(&app.settings))
        .and_then(|directory| {
            let path = directory.join("mods");
            std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
            open::that_detached(path).map_err(|e| e.to_string())
        });
    if let Err(error) = result {
        app.service.error = Some(error);
    }
}
