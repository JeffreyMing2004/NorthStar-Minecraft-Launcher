use crate::{
    app::{self, NsclApp},
    content::{LocalAction, LocalKind},
    instances::Instance,
    pages::{download_button, input_frame},
    theme,
};
use egui::{RichText, Vec2};

pub fn show(
    ui: &mut egui::Ui,
    app: &mut NsclApp,
    instance: &Instance,
    kind: LocalKind,
    editable: bool,
) {
    app.service
        .local_list(instance, kind, &app.settings, ui.ctx(), false);
    let root = crate::store::game_directory(&app.settings);
    let catalog = app.service.local.catalog.clone().filter(|c| {
        c.root == root
            && c.instance.id == instance.id
            && c.instance.version_id == instance.version_id
            && c.instance.isolation == instance.isolation
            && c.kind == kind
    });
    let busy = app.service.busy();
    let modify = editable && !busy && app.service.game.is_none();
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        ui.add(
            egui::TextEdit::singleline(&mut app.service.local.search)
                .hint_text("搜索本地资源")
                .frame(input_frame())
                .desired_width((ui.available_width() - 316.0).max(150.0)),
        );
        if download_button(ui, "导入文件", modify) {
            app.service
                .local_import(instance, kind, None, &app.settings, ui.ctx());
        }
        if download_button(ui, "刷新", !busy) {
            app.service
                .local_list(instance, kind, &app.settings, ui.ctx(), true);
            if kind == LocalKind::Mods {
                app.service.content.managed_requested = false;
                app.service.content.managed = None;
            }
        }
        if download_button(ui, "打开目录", true) {
            open_directory(app, instance, kind.directory());
        }
    });
    ui.add_space(8.0);
    if app.service.game.is_some() {
        app::hint(ui, "游戏运行中，资源暂时只读。");
    }
    if !app.service.local.notice.is_empty() {
        app::hint(ui, &app.service.local.notice);
    }
    let Some(catalog) = catalog else {
        app::hint(
            ui,
            if busy {
                "正在读取本地资源……"
            } else {
                "点击刷新重新读取资源。"
            },
        );
        return;
    };
    let enabled = catalog
        .entries
        .iter()
        .filter(|e| e.enabled && !e.missing)
        .count();
    let disabled = catalog
        .entries
        .iter()
        .filter(|e| !e.enabled && !e.missing)
        .count();
    let missing = catalog.entries.iter().filter(|e| e.missing).count();
    app::hint(
        ui,
        &format!(
            "共 {} 项 · 已启用 {} 项 · 已禁用 {} 项{}{}",
            catalog.entries.len(),
            enabled,
            disabled,
            if missing > 0 {
                format!(" · 文件缺失 {missing} 项")
            } else {
                String::new()
            },
            if catalog.truncated {
                " · 仅显示前 1000 项"
            } else {
                ""
            }
        ),
    );
    if kind != LocalKind::Mods {
        app::hint(
            ui,
            "启用后可在游戏内选择；资源包顺序和光影设置沿用游戏内配置。",
        );
    }
    let query = app.service.local.search.trim().to_lowercase();
    let entries: Vec<_> = catalog
        .entries
        .iter()
        .filter(|e| {
            query.is_empty()
                || format!("{} {} {}", e.name, e.filename, e.version)
                    .to_lowercase()
                    .contains(&query)
        })
        .collect();
    if entries.is_empty() {
        app::body(
            ui,
            if catalog.entries.is_empty() {
                "尚无资源，可导入文件或拖入此页面。"
            } else {
                "没有匹配的资源"
            },
        );
    }
    let mut action = None;
    let mut update = None;
    let height = (ui.clip_rect().bottom() - ui.cursor().top() - 8.0).clamp(90.0, 300.0);
    egui::ScrollArea::vertical()
        .id_salt(("local-resources", instance.id.as_str(), kind.directory()))
        .max_height(height)
        .min_scrolled_height(0.0)
        .auto_shrink([false, true])
        .show_rows(ui, 70.0, entries.len(), |ui, range| {
            ui.spacing_mut().item_spacing.y = 4.0;
            for index in range {
                let entry = entries[index];
                ui.push_id(&entry.relative, |ui| {
                    egui::Frame::NONE
                        .fill(theme::TOPBAR_BG)
                        .corner_radius(6)
                        .inner_margin(8)
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            let candidate = entry
                                .project
                                .as_ref()
                                .and_then(|id| {
                                    app.service
                                        .content
                                        .updates
                                        .available
                                        .iter()
                                        .find(|v| v.project_id == *id)
                                })
                                .cloned();
                            ui.horizontal_top(|ui| {
                                if let Some(id) = &entry.project {
                                    let mut selected =
                                        app.service.content.managed_selection.contains(id);
                                    if ui
                                        .add_enabled(
                                            modify,
                                            egui::Checkbox::without_text(&mut selected),
                                        )
                                        .changed()
                                    {
                                        if selected {
                                            app.service
                                                .content
                                                .managed_selection
                                                .insert(id.clone());
                                        } else {
                                            app.service.content.managed_selection.remove(id);
                                        }
                                    }
                                }
                                let actions_width = if candidate.is_some() { 240.0 } else { 144.0 };
                                let info_width = (ui.available_width() - actions_width).max(70.0);
                                ui.allocate_ui_with_layout(
                                    Vec2::new(info_width, 50.0),
                                    egui::Layout::top_down(egui::Align::Min),
                                    |ui| {
                                        ui.set_min_width(info_width);
                                        ui.spacing_mut().item_spacing.y = 2.0;
                                        let clickable = !entry.missing
                                            && (kind == LocalKind::Mods || entry.enabled);
                                        if ui
                                            .add(
                                                egui::Label::new(
                                                    RichText::new(&entry.name)
                                                        .size(13.0)
                                                        .color(theme::HEADING),
                                                )
                                                .truncate()
                                                .sense(if clickable {
                                                    egui::Sense::click()
                                                } else {
                                                    egui::Sense::hover()
                                                }),
                                            )
                                            .on_hover_text(format!(
                                                "{}\n{}",
                                                entry.filename,
                                                catalog
                                                    .game_directory
                                                    .join(&entry.relative)
                                                    .display()
                                            ))
                                            .clicked()
                                        {
                                            app.instance_browser.tab =
                                                crate::instance_ui::DetailTab::Files;
                                            let folder = match kind {
                                                LocalKind::Mods => {
                                                    crate::instance_files::Folder::Mods
                                                }
                                                LocalKind::ResourcePacks => {
                                                    crate::instance_files::Folder::Resources
                                                }
                                                LocalKind::Shaders => {
                                                    crate::instance_files::Folder::Shaders
                                                }
                                            };
                                            app.instance_browser.files.select(
                                                folder,
                                                Some(entry.filename.clone().into()),
                                            );
                                        }
                                        let size = if entry.directory {
                                            "文件夹".into()
                                        } else if entry.size < 1024 * 1024 {
                                            format!("{:.1} KiB", entry.size as f64 / 1024.0)
                                        } else {
                                            format!(
                                                "{:.1} MiB",
                                                entry.size as f64 / 1024.0 / 1024.0
                                            )
                                        };
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(format!(
                                                    "{} · {}{}",
                                                    if entry.missing {
                                                        "文件缺失"
                                                    } else if entry.project.as_ref().is_some_and(
                                                        |id| app
                                                            .service
                                                            .content
                                                            .managed
                                                            .as_ref()
                                                            .is_some_and(|c| c.target.instance.id
                                                                == instance.id
                                                                && c.entries.iter().any(|m| m
                                                                    .installed
                                                                    .version
                                                                    .project_id
                                                                    == *id
                                                                    && !m.intact))
                                                    ) {
                                                        "文件已修改"
                                                    } else if entry.enabled {
                                                        "已启用"
                                                    } else {
                                                        "已禁用"
                                                    },
                                                    size,
                                                    if entry.version.is_empty() {
                                                        String::new()
                                                    } else {
                                                        format!(" · {}", entry.version)
                                                    }
                                                ))
                                                .size(11.0)
                                                .color(theme::HINT),
                                            )
                                            .truncate(),
                                        );
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(&entry.filename)
                                                    .size(10.0)
                                                    .color(theme::HINT),
                                            )
                                            .truncate(),
                                        );
                                    },
                                );
                                ui.scope(|ui| {
                                    ui.spacing_mut().interact_size.y = 28.0;
                                    ui.spacing_mut().button_padding = Vec2::new(6.0, 4.0);
                                    ui.spacing_mut().item_spacing.x = 6.0;
                                    if let Some(candidate) = candidate
                                        && ui
                                            .add_enabled(
                                                modify,
                                                egui::Button::new("查看更新方案")
                                                    .min_size(Vec2::new(66.0, 28.0)),
                                            )
                                            .clicked()
                                    {
                                        update = Some(candidate.available);
                                    }
                                    if ui
                                        .add_enabled(
                                            modify && !entry.missing,
                                            egui::Button::new(if entry.enabled {
                                                "禁用"
                                            } else {
                                                "启用"
                                            })
                                            .min_size(Vec2::new(52.0, 28.0)),
                                        )
                                        .clicked()
                                    {
                                        action = Some((
                                            entry.clone(),
                                            if entry.enabled {
                                                LocalAction::Disable
                                            } else {
                                                LocalAction::Enable
                                            },
                                        ));
                                    }
                                    if ui
                                        .add_enabled(
                                            modify,
                                            egui::Button::new("移除")
                                                .min_size(Vec2::new(52.0, 28.0)),
                                        )
                                        .clicked()
                                    {
                                        app.service.local.remove = Some(entry.clone());
                                    }
                                });
                            });
                        });
                });
            }
        });
    if let Some((entry, action)) = action {
        app.service
            .local_change(entry, action, &app.settings, ui.ctx());
    }
    if let Some(version) = update {
        app.service
            .content_preview(version, &app.settings, ui.ctx());
    }
    if let Some(entry) = app.service.local.remove.clone() {
        let mut open = true;
        egui::Window::new("移除资源")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ui.ctx(), |ui| {
                ui.label(format!("移除「{}」？", entry.name));
                ui.label(if entry.missing {
                    "此文件已缺失，将只清理失效的安装记录。"
                } else {
                    "文件会保留到此实例的本地回收目录，可打开目录找回。"
                });
                if kind == LocalKind::Mods && entry.project.is_none() {
                    app::hint(ui, "此文件没有单独的依赖记录，请确认其他模组不需要它。");
                }
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(modify, egui::Button::new("确认移除"))
                        .clicked()
                    {
                        app.service.local.remove = None;
                        app.service.local_change(
                            entry.clone(),
                            LocalAction::Remove,
                            &app.settings,
                            ui.ctx(),
                        );
                    }
                    if ui.button("取消").clicked() {
                        app.service.local.remove = None;
                    }
                });
            });
        if !open {
            app.service.local.remove = None;
        }
    }
    if (catalog.has_trash || app.service.local.notice.contains("回收目录"))
        && ui.button("打开回收目录").clicked()
    {
        open_directory(app, instance, ".nscl/trash");
    }
}

fn open_directory(app: &mut NsclApp, instance: &Instance, relative: &str) {
    let result = crate::content::local_directory(
        &crate::store::game_directory(&app.settings),
        instance,
        relative,
    )
    .and_then(|path| {
        std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
        open::that_detached(path).map_err(|e| e.to_string())
    });
    if let Err(error) = result {
        app.service.error = Some(error);
    }
}
