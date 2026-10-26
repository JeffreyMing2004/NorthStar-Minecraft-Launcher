use crate::{
    app::{self, NsclApp, Page},
    content::ContentKind,
    instances::{self, Instance},
    pages::{download_button, download_status_height, input_focus, input_frame, task_status},
    theme,
};
use egui::{Align, Layout, RichText, Ui, Vec2};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Filter {
    #[default]
    All,
    Packs,
    Modded,
    Vanilla,
    Archived,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum DetailTab {
    #[default]
    Overview,
    Files,
}

#[derive(Default)]
pub struct BrowserState {
    pub search: String,
    pub selected: String,
    pub filter: Filter,
    pub tab: DetailTab,
    pub creating: bool,
    root: Option<PathBuf>,
    archived: Option<Vec<(String, Instance)>>,
}

impl BrowserState {
    pub fn invalidate_archives(&mut self) {
        self.archived = None;
    }
}

fn matches_filter(app: &NsclApp, instance: &Instance, filter: Filter) -> bool {
    let installed = app
        .service
        .installed
        .iter()
        .find(|v| v.id == instance.version_id);
    match filter {
        Filter::All | Filter::Archived => true,
        Filter::Packs => instance.requires_isolation(),
        Filter::Modded => {
            !instance.requires_isolation() && installed.is_some_and(|v| v.loader.is_some())
        }
        Filter::Vanilla => {
            !instance.requires_isolation() && installed.is_some_and(|v| v.loader.is_none())
        }
    }
}

fn kind(app: &NsclApp, instance: &Instance) -> &'static str {
    if instance.requires_isolation() {
        "整合包"
    } else {
        app.service
            .installed
            .iter()
            .find(|v| v.id == instance.version_id)
            .map(|v| v.loader.as_ref().map(|l| l.kind.label()).unwrap_or("原版"))
            .unwrap_or("未完成安装")
    }
}

pub fn show(ui: &mut Ui, app: &mut NsclApp) {
    let root = crate::store::game_directory(&app.settings);
    if app.instance_browser.root.as_ref() != Some(&root) {
        app.instance_browser.root = Some(root.clone());
        app.instance_browser.invalidate_archives();
    }
    let editable = !app.service.busy() && app.service.game.is_none();
    theme::toolbar(ui, |ui| {
        if download_button(ui, "返回开始", true) {
            app.page = Page::Home;
        }
        app::heading(ui, "实例管理");
        if download_button(ui, "刷新列表", editable) {
            app.service.refresh(&app.settings);
            app.instance_browser.invalidate_archives();
        }
        if download_button(
            ui,
            "新建实例",
            editable && !app.service.installed.is_empty(),
        ) {
            app.instance_browser.creating = true;
        }
        if download_button(ui, "安装新版本", !app.service.busy()) {
            app.page = Page::Downloads;
            app.category = 0;
            app.download = Default::default();
        }
    });
    ui.add_space(14.0);
    theme::toolbar(ui, |ui| {
        for (filter, label) in [
            (Filter::All, "全部"),
            (Filter::Packs, "整合包"),
            (Filter::Modded, "模组版本"),
            (Filter::Vanilla, "原版"),
            (Filter::Archived, "已归档"),
        ] {
            if app::tab_button(ui, label, app.instance_browser.filter == filter) {
                app.instance_browser.filter = filter;
            }
        }
    });
    if app.instance_browser.filter == Filter::Archived && app.instance_browser.archived.is_none() {
        // Load only on opening or refreshing this category, never on every frame.
        app.instance_browser.archived = Some(instances::archived(&root));
    }
    ui.add_space(14.0);
    let query = app.instance_browser.search.trim().to_lowercase();
    let entries: Vec<(Option<String>, Instance)> =
        if app.instance_browser.filter == Filter::Archived {
            app.instance_browser
                .archived
                .as_deref()
                .unwrap_or_default()
                .iter()
                .map(|(entry, instance)| (Some(entry.clone()), instance.clone()))
                .collect::<Vec<_>>()
        } else {
            app.service
                .instances
                .iter()
                .filter(|i| matches_filter(app, i, app.instance_browser.filter))
                .map(|i| (None, i.clone()))
                .collect::<Vec<_>>()
        }
        .into_iter()
        .filter(|(_, i)| {
            query.is_empty()
                || format!("{} {}", i.name, i.version_id)
                    .to_lowercase()
                    .contains(&query)
        })
        .collect();
    if !entries
        .iter()
        .any(|(_, i)| i.id == app.instance_browser.selected)
    {
        app.instance_browser.selected = entries
            .iter()
            .find(|(_, i)| i.id == app.service.selected)
            .or_else(|| entries.first())
            .map(|(_, i)| i.id.clone())
            .unwrap_or_default();
    }
    let height = (ui.available_height()
        - download_status_height(app)
        - if app.instance_status.is_empty() {
            0.0
        } else {
            28.0
        })
    .max(160.0);
    let width = ui.available_width();
    let left_width = (width * 0.34).clamp(240.0, 340.0);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 18.0;
        ui.allocate_ui_with_layout(
            Vec2::new(left_width, height),
            Layout::top_down(Align::Min),
            |ui| {
                theme::card(18).show(ui, |ui| {
                    ui.set_width(left_width - 38.0);
                    ui.set_min_height((height - 38.0).max(0.0));
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut app.instance_browser.search)
                            .hint_text("搜索实例或游戏版本")
                            .frame(input_frame())
                            .desired_width(ui.available_width()),
                    );
                    input_focus(ui, &response);
                    ui.add_space(12.0);
                    egui::ScrollArea::vertical()
                        .id_salt("instance-browser-list")
                        .max_height((height - 106.0).max(40.0))
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_width((left_width - 38.0).max(0.0));
                            ui.spacing_mut().item_spacing.y = 8.0;
                            for (_, instance) in &entries {
                                let current = if instance.id == app.service.selected {
                                    " · 当前"
                                } else {
                                    ""
                                };
                                let label = format!(
                                    "{}\n{}{}",
                                    instance.name,
                                    kind(app, instance),
                                    current
                                );
                                if ui
                                    .push_id(&instance.id, |ui| {
                                        ui.add_sized(
                                            Vec2::new(ui.available_width(), 64.0),
                                            egui::Button::new(RichText::new(label).size(13.0))
                                                .selected(
                                                    app.instance_browser.selected == instance.id,
                                                ),
                                        )
                                        .on_hover_text(&instance.name)
                                        .clicked()
                                    })
                                    .inner
                                {
                                    app.instance_browser.selected = instance.id.clone();
                                }
                            }
                            if entries.is_empty() {
                                app::hint(ui, "没有匹配的实例。");
                            }
                        });
                });
            },
        );
        let right_width = (width - left_width - 18.0).max(200.0);
        ui.allocate_ui_with_layout(
            Vec2::new(right_width, height),
            Layout::top_down(Align::Min),
            |ui| {
                theme::card(22).show(ui, |ui| {
                    ui.set_width((right_width - 46.0).max(0.0));
                    ui.set_min_height((height - 46.0).max(0.0));
                    egui::ScrollArea::vertical()
                        .id_salt((
                            "instance-browser-details",
                            &app.instance_browser.selected,
                            app.instance_browser.tab as u8,
                        ))
                        .max_height((height - 46.0).max(60.0))
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_width((right_width - 46.0).max(0.0));
                            if let Some((archive, instance)) = entries
                                .iter()
                                .find(|(_, i)| i.id == app.instance_browser.selected)
                            {
                                details(ui, app, &root, instance, archive.as_deref(), editable);
                            } else {
                                app::heading(ui, "选择一个实例");
                                ui.add_space(12.0);
                                app::body(ui, "从左侧选择实例，或新建实例、安装新的游戏版本。");
                            }
                        });
                });
            },
        );
    });
    if !app.instance_status.is_empty() {
        ui.add_space(8.0);
        app::hint(ui, &app.instance_status);
    }
    task_status(ui, app);
    create_dialog(ui.ctx(), app, &root, editable);
    crate::manager_ui::show(ui.ctx(), app);
}

fn details(
    ui: &mut Ui,
    app: &mut NsclApp,
    root: &Path,
    instance: &Instance,
    archive: Option<&str>,
    editable: bool,
) {
    let installed = app
        .service
        .installed
        .iter()
        .find(|v| v.id == instance.version_id)
        .cloned();
    ui.add(
        egui::Label::new(
            RichText::new(&instance.name)
                .size(21.0)
                .family(theme::bold_family())
                .color(theme::HEADING),
        )
        .truncate(),
    );
    ui.add_space(8.0);
    let minecraft = installed
        .as_ref()
        .and_then(|v| v.loader.as_ref())
        .map(|l| l.minecraft.as_str())
        .unwrap_or(&instance.version_id);
    let origin = if instance.is_external() {
        " · 已有版本"
    } else {
        ""
    };
    app::body(
        ui,
        &format!("Minecraft {minecraft} · {}{origin}", kind(app, instance)),
    );
    ui.add_space(16.0);
    if let Some(entry) = archive {
        app::hint(ui, "此实例已归档，恢复后可继续配置和启动。");
        ui.add_space(14.0);
        if download_button(ui, "恢复实例", editable) {
            match instances::restore(root, entry) {
                Ok(restored) => {
                    app.service.refresh(&app.settings);
                    app.service.selected = restored.id.clone();
                    app.instance_browser.selected = restored.id;
                    app.instance_browser.filter = Filter::All;
                    app.instance_browser.tab = DetailTab::Overview;
                    app.instance_browser.invalidate_archives();
                    app.remember_instance();
                    app.instance_status = "实例已恢复。".into();
                }
                Err(error) => app.service.error = Some(error),
            }
        }
        return;
    }
    theme::toolbar(ui, |ui| {
        for (tab, label) in [(DetailTab::Overview, "概览"), (DetailTab::Files, "文件")] {
            if app::tab_button(ui, label, app.instance_browser.tab == tab) {
                app.instance_browser.tab = tab;
            }
        }
    });
    ui.add_space(18.0);
    if app.instance_browser.tab == DetailTab::Files {
        app::body(ui, "打开此实例对应的游戏文件夹。");
        ui.add_space(14.0);
        let folders = [
            ("游戏目录", ""),
            ("存档", "saves"),
            ("模组", "mods"),
            ("资源包", "resourcepacks"),
            ("光影", "shaderpacks"),
            ("截图", "screenshots"),
            ("日志", "logs"),
        ];
        let columns = ((ui.available_width() + theme::CONTROL_GAP) / (96.0 + theme::CONTROL_GAP))
            .floor()
            .max(1.0) as usize;
        for row in folders.chunks(columns) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme::CONTROL_GAP;
                for (label, directory) in row {
                    if download_button(ui, label, true) {
                        let result = instance.game_directory(root).and_then(|path| {
                            let path = path.join(directory);
                            std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
                            open::that_detached(path).map_err(|e| e.to_string())
                        });
                        if let Err(error) = result {
                            app.service.error = Some(error);
                        }
                    }
                }
            });
            ui.add_space(theme::CONTROL_GAP);
        }
        return;
    }
    let loader = installed
        .as_ref()
        .and_then(|v| v.loader.as_ref())
        .map(|l| format!("{} {}", l.kind.label(), l.version))
        .unwrap_or_else(|| "原版".into());
    for (label, value) in [
        ("加载器", loader),
        ("游戏目录", instance.isolation.label().to_owned()),
        (
            "Java",
            instance
                .java_path
                .as_ref()
                .map(|p| {
                    if p.is_empty() {
                        "自动匹配"
                    } else {
                        "单独指定"
                    }
                })
                .unwrap_or("跟随全局")
                .into(),
        ),
        (
            "内存",
            instance
                .memory
                .map(|m| format!("{m} MiB"))
                .unwrap_or_else(|| "跟随全局".into()),
        ),
    ] {
        ui.horizontal(|ui| {
            ui.add_sized(
                Vec2::new(80.0, 24.0),
                egui::Label::new(RichText::new(label).color(theme::HINT)),
            );
            ui.add(egui::Label::new(value).truncate());
        });
    }
    ui.add_space(18.0);
    theme::toolbar(ui, |ui| {
        ui.add_enabled_ui(editable && installed.is_some(), |ui| {
            if app::primary_button(ui, "启动游戏") {
                app.service.selected = instance.id.clone();
                app.remember_instance();
                app.service.launch(&app.settings, &app.accounts, ui.ctx());
            }
        });
        if download_button(
            ui,
            "设为当前实例",
            editable && app.service.selected != instance.id,
        ) {
            app.service.selected = instance.id.clone();
            app.remember_instance();
        }
    });
    ui.add_space(12.0);
    theme::toolbar(ui, |ui| {
        if download_button(ui, "实例设置", editable) {
            app.instance_memory = instance.memory.unwrap_or(app.settings.memory).to_string();
            app.instance_edit = Some(instance.clone());
        }
        if download_button(
            ui,
            "管理模组",
            editable && installed.as_ref().is_some_and(|v| v.loader.is_some()),
        ) {
            app.service.content_kind(ContentKind::Mod);
            app.service.content.target = instance.id.clone();
            app.service.content.plan = None;
            app.service.content.managed = None;
            app.service.content.managed_selection.clear();
            app.service.content.updates = Default::default();
            app.service.content.updates_checked = false;
            app.service.content_manage(&app.settings, ui.ctx(), false);
        }
        if download_button(ui, "归档实例", editable) {
            app.archive_confirm = Some(instance.clone());
        }
    });
    if !editable {
        ui.add_space(12.0);
        app::hint(ui, "游戏或任务运行中，实例配置暂时锁定。");
    }
    if installed.is_none() {
        ui.add_space(12.0);
        app::hint(ui, "游戏组件尚未安装完整，可先配置实例或安装游戏版本。");
    }
}

fn create_dialog(ctx: &egui::Context, app: &mut NsclApp, root: &Path, editable: bool) {
    if !app.instance_browser.creating {
        return;
    }
    let (mut open, mut created) = (true, false);
    egui::Window::new("新建实例")
        .open(&mut open)
        .default_width(440.0)
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 12.0;
            ui.add_enabled_ui(editable, |ui| {
                ui.label("实例名称");
                ui.add(
                    egui::TextEdit::singleline(&mut app.new_instance_name)
                        .hint_text("例如 生存世界")
                        .desired_width(ui.available_width()),
                );
                ui.label("已安装的游戏版本");
                if !app
                    .service
                    .installed
                    .iter()
                    .any(|v| v.id == app.new_instance_version)
                {
                    app.new_instance_version = app
                        .service
                        .installed
                        .first()
                        .map(|v| v.id.clone())
                        .unwrap_or_default();
                }
                egui::ComboBox::from_id_salt("new-instance-version")
                    .width(360.0)
                    .selected_text(&app.new_instance_version)
                    .show_ui(ui, |ui| {
                        for version in &app.service.installed {
                            ui.selectable_value(
                                &mut app.new_instance_version,
                                version.id.clone(),
                                &version.id,
                            );
                        }
                    });
                app::hint(
                    ui,
                    &format!("游戏目录：{}", app.settings.default_isolation.label()),
                );
                if download_button(
                    ui,
                    "创建实例",
                    !app.new_instance_name.trim().is_empty()
                        && !app.new_instance_version.is_empty(),
                ) {
                    match instances::create_with_isolation(
                        root,
                        &app.new_instance_version,
                        &app.new_instance_name,
                        app.settings.default_isolation,
                    ) {
                        Ok(instance) => {
                            app.service.refresh(&app.settings);
                            app.service.selected = instance.id.clone();
                            app.instance_browser.selected = instance.id;
                            app.instance_browser.filter = Filter::All;
                            app.instance_browser.tab = DetailTab::Overview;
                            app.instance_browser.search.clear();
                            app.remember_instance();
                            app.new_instance_name.clear();
                            app.instance_status = "实例已创建。".into();
                            created = true;
                        }
                        Err(error) => app.service.error = Some(error),
                    }
                }
            });
        });
    app.instance_browser.creating = open && !created;
}

#[cfg(test)]
mod tests;
