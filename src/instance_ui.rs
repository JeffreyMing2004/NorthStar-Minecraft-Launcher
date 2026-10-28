use crate::{
    app::{self, NsclApp, Page},
    instances::{self, Instance},
    pages::{download_button, download_status_height, input_focus, input_frame, task_status},
    theme,
};
use egui::{Align, Layout, Pos2, Rect, RichText, Sense, Stroke, Ui, UiBuilder, Vec2};
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DetailTab {
    #[default]
    Overview,
    Mods,
    ResourcePacks,
    Shaders,
    Files,
}

#[derive(Default)]
pub struct BrowserState {
    pub search: String,
    pub selected: String,
    pub filter: Filter,
    pub tab: DetailTab,
    pub creating: bool,
    pub files: crate::instance_files::Browser,
    pub export: Option<crate::instance_export::ExportDialog>,
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
    ui.horizontal(|ui| {
        if download_button(ui, "返回开始", true) {
            app.page = Page::Home;
        }
        app::heading(ui, "版本管理");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if download_button(ui, "安装新版本", !app.service.busy()) {
                app.page = Page::Downloads;
                app.category = 0;
                app.download = Default::default();
            }
            if download_button(
                ui,
                "新建实例",
                editable && !app.service.installed.is_empty(),
            ) {
                app.instance_browser.creating = true;
            }
            if download_button(ui, "刷新列表", editable) {
                app.service.refresh(&app.settings);
                app.instance_browser.invalidate_archives();
                app.instance_icons.clear();
            }
        });
    });
    ui.add_space(16.0);
    let height = (ui.available_height()
        - download_status_height(app)
        - if app.instance_status.is_empty() {
            0.0
        } else {
            28.0
        })
    .max(180.0);
    let width = ui.available_width();
    let left_width = 252.0_f32.min(width * 0.32);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 16.0;
        ui.allocate_ui_with_layout(
            Vec2::new(left_width, height),
            Layout::top_down(Align::Min),
            |ui| {
                egui::Frame::NONE
                    .fill(theme::TOPBAR_BG)
                    .stroke(Stroke::new(1.0, theme::CARD_BORDER))
                    .corner_radius(8)
                    .inner_margin(12)
                    .show(ui, |ui| {
                        ui.set_width((left_width - 26.0).max(0.0));
                        ui.set_min_height((height - 26.0).max(0.0));
                        ui.spacing_mut().item_spacing.y = 10.0;
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut app.instance_browser.search)
                                .hint_text("搜索版本")
                                .font(egui::FontId::proportional(13.0))
                                .frame(input_frame())
                                .desired_width(ui.available_width()),
                        );
                        input_focus(ui, &response);
                        egui::ComboBox::from_id_salt("instance-filter")
                            .width(ui.available_width())
                            .selected_text(filter_label(app.instance_browser.filter))
                            .show_ui(ui, |ui| {
                                ui.spacing_mut().interact_size.y = 28.0;
                                ui.spacing_mut().button_padding = Vec2::new(8.0, 4.0);
                                ui.spacing_mut().item_spacing.y = 4.0;
                                for filter in [
                                    Filter::All,
                                    Filter::Packs,
                                    Filter::Modded,
                                    Filter::Vanilla,
                                    Filter::Archived,
                                ] {
                                    ui.selectable_value(
                                        &mut app.instance_browser.filter,
                                        filter,
                                        filter_label(filter),
                                    );
                                }
                            });
                        if app.instance_browser.filter == Filter::Archived
                            && app.instance_browser.archived.is_none()
                        {
                            app.instance_browser.archived = Some(instances::archived(&root));
                        }
                        let query = app.instance_browser.search.trim().to_lowercase();
                        let entries: Vec<(Option<String>, Instance)> =
                            if app.instance_browser.filter == Filter::Archived {
                                app.instance_browser
                                    .archived
                                    .as_deref()
                                    .unwrap_or_default()
                                    .iter()
                                    .map(|(entry, instance)| {
                                        (Some(entry.clone()), instance.clone())
                                    })
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
                        egui::ScrollArea::vertical()
                            .id_salt("instance-browser-list")
                            .max_height((height - 128.0).max(40.0))
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.set_width((left_width - 26.0).max(0.0));
                                ui.spacing_mut().item_spacing.y = 2.0;
                                for (archive, instance) in entries {
                                    let icon_kind = icon_kind(app, &instance);
                                    let meta = format!(
                                        "{} · {}",
                                        game_version(app, &instance),
                                        kind(app, &instance)
                                    );
                                    if instance_row(
                                        ui,
                                        &instance,
                                        &meta,
                                        app.instance_browser.selected == instance.id,
                                        app.service.selected == instance.id,
                                        |ui, rect| {
                                            app.instance_icons
                                                .get(
                                                    ui.ctx(),
                                                    &root,
                                                    &instance,
                                                    archive.as_deref(),
                                                    icon_kind,
                                                )
                                                .paint(ui, rect);
                                        },
                                    ) {
                                        app.instance_browser.selected = instance.id;
                                    }
                                }
                                if app.instance_browser.selected.is_empty() {
                                    app::hint(ui, "没有匹配的版本");
                                }
                            });
                    });
            },
        );
        let right_width = (width - left_width - 16.0).max(200.0);
        ui.allocate_ui_with_layout(
            Vec2::new(right_width, height),
            Layout::top_down(Align::Min),
            |ui| {
                theme::card(22).show(ui, |ui| {
                    ui.set_width((right_width - 46.0).max(0.0));
                    ui.set_min_height((height - 46.0).max(0.0));
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let selected = if app.instance_browser.filter == Filter::Archived {
                        app.instance_browser
                            .archived
                            .as_deref()
                            .unwrap_or_default()
                            .iter()
                            .find(|(_, i)| i.id == app.instance_browser.selected)
                            .map(|(entry, i)| (Some(entry.clone()), i.clone()))
                    } else {
                        app.service
                            .instances
                            .iter()
                            .find(|i| i.id == app.instance_browser.selected)
                            .map(|i| (None, i.clone()))
                    };
                    egui::ScrollArea::vertical()
                        .id_salt((
                            "instance-browser-details",
                            &app.instance_browser.selected,
                            app.instance_browser.tab as u8,
                        ))
                        .max_height((height - 106.0).max(60.0))
                        .min_scrolled_height(0.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_width((right_width - 46.0).max(0.0));
                            ui.spacing_mut().item_spacing.y = 8.0;
                            if let Some((archive, instance)) = &selected {
                                details(ui, app, &root, instance, archive.as_deref(), editable);
                            } else {
                                app::hint(ui, "未选择版本");
                            }
                        });
                    ui.add_space(12.0);
                    ui.allocate_ui_with_layout(
                        Vec2::new(ui.available_width(), 48.0),
                        Layout::right_to_left(Align::Center),
                        |ui| {
                            launch_footer(
                                ui,
                                app,
                                &root,
                                selected
                                    .as_ref()
                                    .map(|(entry, instance)| (entry.as_deref(), instance)),
                                editable,
                            );
                        },
                    );
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
    crate::instance_export::show(ui.ctx(), app);
}

fn filter_label(filter: Filter) -> &'static str {
    match filter {
        Filter::All => "全部版本",
        Filter::Packs => "整合包",
        Filter::Modded => "模组版本",
        Filter::Vanilla => "原版",
        Filter::Archived => "已归档",
    }
}

fn game_version<'a>(app: &'a NsclApp, instance: &'a Instance) -> &'a str {
    app.service
        .installed
        .iter()
        .find(|v| v.id == instance.version_id)
        .and_then(|v| v.loader.as_ref())
        .map(|l| l.minecraft.as_str())
        .unwrap_or(&instance.version_id)
}

fn icon_kind(app: &NsclApp, instance: &Instance) -> crate::instance_icons::Kind {
    use crate::{instance_icons::Kind, minecraft::LoaderKind};
    if instance.requires_isolation() {
        return Kind::Pack;
    }
    match app
        .service
        .installed
        .iter()
        .find(|v| v.id == instance.version_id)
        .and_then(|v| v.loader.as_ref())
        .map(|l| l.kind)
    {
        Some(LoaderKind::Fabric) => Kind::Fabric,
        Some(LoaderKind::Forge) => Kind::Forge,
        Some(LoaderKind::NeoForge) => Kind::NeoForge,
        None => Kind::Vanilla,
    }
}

fn instance_row(
    ui: &mut Ui,
    instance: &Instance,
    meta: &str,
    selected: bool,
    current: bool,
    paint_icon: impl FnOnce(&Ui, Rect),
) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 52.0), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &instance.name));
    if !ui.is_rect_visible(rect) {
        return false;
    }
    if selected || response.hovered() {
        ui.painter().rect_filled(
            rect,
            4.0,
            if selected {
                theme::nav_active_bg()
            } else {
                theme::HOVER
            },
        );
    }
    if selected {
        ui.painter().rect_filled(
            Rect::from_min_size(rect.min, Vec2::new(2.0, rect.height())),
            1.0,
            theme::ACCENT,
        );
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect.shrink(1.0),
            4.0,
            Stroke::new(1.0, theme::ACCENT),
            egui::StrokeKind::Inside,
        );
    }
    paint_icon(
        ui,
        Rect::from_min_size(rect.min + Vec2::new(8.0, 10.0), Vec2::splat(32.0)),
    );
    let mut text = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_max(
        rect.min + Vec2::new(48.0, 7.0),
        rect.max - Vec2::new(14.0, 5.0),
    )));
    text.spacing_mut().item_spacing.y = 3.0;
    text.add(
        egui::Label::new(
            RichText::new(&instance.name)
                .size(13.0)
                .color(theme::BODY_TEXT),
        )
        .truncate(),
    );
    text.add(egui::Label::new(RichText::new(meta).size(11.0).color(theme::HINT)).truncate());
    if current {
        ui.painter().circle_filled(
            Pos2::new(rect.right() - 7.0, rect.center().y),
            2.5,
            theme::ACCENT,
        );
    }
    response
        .on_hover_text(&instance.name)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

fn launch_footer(
    ui: &mut Ui,
    app: &mut NsclApp,
    root: &Path,
    selected: Option<(Option<&str>, &Instance)>,
    editable: bool,
) {
    let Some((archive, instance)) = selected else {
        ui.add_enabled_ui(false, |ui| {
            app::primary_button(ui, "启动游戏");
        });
        return;
    };
    if let Some(entry) = archive {
        if ui
            .add_enabled_ui(editable, |ui| app::primary_button(ui, "恢复实例"))
            .inner
        {
            match instances::restore(root, entry) {
                Ok(restored) => {
                    app.service.refresh(&app.settings);
                    app.service.selected = restored.id.clone();
                    app.instance_browser.selected = restored.id;
                    app.instance_browser.filter = Filter::All;
                    app.instance_browser.tab = DetailTab::Overview;
                    app.instance_browser.invalidate_archives();
                    app.instance_icons.clear();
                    app.remember_instance();
                    app.instance_status = "实例已恢复".into();
                }
                Err(error) => app.service.error = Some(error),
            }
        }
        return;
    }
    let installed = app
        .service
        .installed
        .iter()
        .any(|v| v.id == instance.version_id);
    if ui
        .add_enabled_ui(editable && installed, |ui| {
            app::primary_button(ui, "启动游戏")
        })
        .inner
    {
        app.service.selected = instance.id.clone();
        app.remember_instance();
        app.service.launch(&app.settings, &app.accounts, ui.ctx());
    }
    if download_button(
        ui,
        "设为当前实例",
        editable && app.service.selected != instance.id,
    ) {
        app.service.selected = instance.id.clone();
        app.remember_instance();
    }
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
    let icon_kind = icon_kind(app, instance);
    let icon = app
        .instance_icons
        .get(ui.ctx(), root, instance, archive, icon_kind);
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
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(44.0), Sense::hover());
        icon.paint(ui, rect);
        ui.vertical(|ui| {
            ui.add(
                egui::Label::new(
                    RichText::new(&instance.name)
                        .size(20.0)
                        .family(theme::bold_family())
                        .color(theme::HEADING),
                )
                .truncate(),
            );
            ui.add(
                egui::Label::new(
                    RichText::new(format!(
                        "Minecraft {minecraft} · {}{origin}",
                        kind(app, instance)
                    ))
                    .size(12.0)
                    .color(theme::HINT),
                )
                .truncate(),
            );
        });
    });
    ui.add_space(16.0);
    if archive.is_some() {
        app::hint(ui, "已归档");
        return;
    }
    theme::toolbar(ui, |ui| {
        for (tab, label) in [
            (DetailTab::Overview, "概览"),
            (DetailTab::Mods, "模组"),
            (DetailTab::ResourcePacks, "资源包"),
            (DetailTab::Shaders, "光影"),
            (DetailTab::Files, "文件"),
        ] {
            if app::tab_button(ui, label, app.instance_browser.tab == tab) {
                app.instance_browser.tab = tab;
            }
        }
    });
    ui.add_space(18.0);
    if app.instance_browser.tab == DetailTab::Files {
        crate::instance_files::show(ui, root, instance, &mut app.instance_browser.files);
        return;
    }
    if matches!(
        app.instance_browser.tab,
        DetailTab::ResourcePacks | DetailTab::Shaders
    ) {
        let kind = if app.instance_browser.tab == DetailTab::ResourcePacks {
            crate::content::LocalKind::ResourcePacks
        } else {
            crate::content::LocalKind::Shaders
        };
        crate::instance_resources::show(ui, app, instance, kind, editable);
        return;
    }
    if app.instance_browser.tab == DetailTab::Mods {
        let modded = installed.as_ref().is_some_and(|v| v.loader.is_some());
        if modded {
            crate::instance_mods::show(ui, app, instance, editable);
        } else {
            app::body(ui, "此实例未安装 Forge、Fabric 或 NeoForge，无法加载模组。");
            app::hint(
                ui,
                "先在 Minecraft 下载页安装带加载器的版本，它会创建可安装模组的实例。",
            );
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
            app.instance_browser.tab = DetailTab::Mods;
        }
        if download_button(ui, "归档实例", editable) {
            app.archive_confirm = Some(instance.clone());
        }
        if download_button(ui, "导出整合包", editable && installed.is_some()) {
            app.instance_browser.export = Some(crate::instance_export::ExportDialog::new(instance));
        }
    });
    if let Some(path) = &app.service.exported_pack {
        ui.add_space(8.0);
        app::hint(ui, &format!("已导出：{}", path.display()));
        if download_button(ui, "打开导出目录", true)
            && let Some(parent) = path.parent()
            && let Err(error) = open::that(parent)
        {
            app.service.error = Some(format!("打开导出目录失败：{error}"));
        }
    }
    if !editable {
        ui.add_space(12.0);
        app::hint(ui, "游戏或任务运行中，实例配置暂时锁定。");
    }
    if installed.is_none() {
        ui.add_space(12.0);
        app::hint(ui, "版本未安装完整");
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
