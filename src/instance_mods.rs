use crate::{
    app::{self, NsclApp},
    instances::Instance,
    pages::{download_button, input_focus, input_frame},
    service::ContentPlan,
    theme,
};
use egui::{FontId, Pos2, Rect, RichText, Sense, Stroke, Ui, UiBuilder, Vec2};

/// Instance-scoped mod management: the installed catalog plus a Modrinth search that
/// filters and installs into this instance only.
pub fn show(ui: &mut Ui, app: &mut NsclApp, instance: &Instance, editable: bool) {
    app.service.content_instance(&instance.id);
    if app.service.content.plan.is_some() {
        plan(ui, app, editable);
    } else if app.service.content.project.is_some() {
        versions(ui, app);
    } else if app.service.content.adding {
        search(ui, app);
    } else {
        catalog(ui, app, instance, editable);
    }
    audit(ui.ctx(), app);
}

fn catalog(ui: &mut Ui, app: &mut NsclApp, instance: &Instance, editable: bool) {
    let busy = app.service.busy();
    // One request per bound instance keeps rendering from starting tasks every frame.
    if app.service.content.managed.is_none() && !app.service.content.managed_requested && !busy {
        app.service.content_manage(&app.settings, ui.ctx(), false);
    }
    let catalog = app.service.content.managed.clone();
    theme::toolbar(ui, |ui| {
        if download_button(ui, "添加模组", !busy) {
            app.service.content.adding = true;
            app.service.content.project = None;
            app.service.content.versions.clear();
            app.service.content.results = Default::default();
            app.service.content.searched = false;
        }
        if download_button(ui, "刷新列表", !busy) {
            app.service.content_manage(&app.settings, ui.ctx(), false);
        }
        if download_button(
            ui,
            "检查更新",
            !busy && catalog.as_ref().is_some_and(|c| !c.entries.is_empty()),
        ) {
            app.service.content_manage(&app.settings, ui.ctx(), true);
        }
        if download_button(ui, "检查兼容性", !busy && editable) {
            app.service.content_check(&app.settings, ui.ctx());
        }
        if download_button(
            ui,
            "卸载所选",
            !busy && editable && !app.service.content.managed_selection.is_empty(),
        ) {
            app.service.content_remove_preview(&app.settings, ui.ctx());
        }
        if download_button(ui, "打开 mods 目录", true) {
            open_mods(app, instance);
        }
    });
    ui.add_space(10.0);
    let Some(catalog) = catalog else {
        app::hint(
            ui,
            if busy {
                "正在读取模组列表……"
            } else {
                "尚未读取此实例的模组列表。"
            },
        );
        return;
    };
    let status = if app.service.content.updates_checked {
        format!(
            "发现 {} 项更新 · 自动检查仅选择适用于当前实例的较新正式版",
            app.service.content.updates.available.len()
        )
    } else {
        "更新前先查看依赖方案；卸载前检查已登记模组的必需依赖。".to_owned()
    };
    app::hint(ui, &status);
    app::hint(
        ui,
        &format!(
            "单独安装 {} 个 · 其他本地 {} 个 · 已选 {} 个",
            catalog.entries.len(),
            catalog.other_files.len(),
            app.service.content.managed_selection.len()
        ),
    );
    ui.add_space(10.0);
    if catalog.entries.is_empty() {
        app::body(
            ui,
            if catalog.other_files.is_empty() {
                "此实例尚无模组文件，点击「添加模组」从 Modrinth 安装。"
            } else {
                "此实例的模组来自整合包或手动添加，文件列在下方。"
            },
        );
    }
    egui::ScrollArea::vertical()
        .id_salt("instance-managed-mods")
        .max_height(220.0)
        .min_scrolled_height(0.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for entry in &catalog.entries {
                let version = &entry.installed.version;
                egui::Frame::NONE
                    .fill(theme::TOPBAR_BG)
                    .corner_radius(8.0)
                    .inner_margin(10)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        let mut selected = app
                            .service
                            .content
                            .managed_selection
                            .contains(&version.project_id);
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(!busy, egui::Checkbox::without_text(&mut selected))
                                .changed()
                            {
                                if selected {
                                    app.service
                                        .content
                                        .managed_selection
                                        .insert(version.project_id.clone());
                                } else {
                                    app.service
                                        .content
                                        .managed_selection
                                        .remove(&version.project_id);
                                }
                            }
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&version.name)
                                        .size(14.0)
                                        .color(theme::HEADING),
                                )
                                .truncate(),
                            );
                        });
                        theme::toolbar(ui, |ui| {
                            ui.label(
                                RichText::new(&version.version_number)
                                    .size(12.0)
                                    .color(theme::MUTED),
                            );
                            if !entry.intact {
                                ui.label(
                                    RichText::new("文件缺失或已修改")
                                        .size(12.0)
                                        .color(theme::ACCENT),
                                );
                            }
                            if let Some(candidate) = app
                                .service
                                .content
                                .updates
                                .available
                                .iter()
                                .find(|u| u.project_id == version.project_id)
                            {
                                ui.label(
                                    RichText::new(format!(
                                        "→ {}",
                                        candidate.available.version_number
                                    ))
                                    .size(12.0)
                                    .color(theme::ACCENT),
                                );
                                if ui
                                    .add_enabled(
                                        !busy && editable,
                                        egui::Button::new("查看更新方案"),
                                    )
                                    .clicked()
                                {
                                    app.service.content_preview(
                                        candidate.available.clone(),
                                        &app.settings,
                                        ui.ctx(),
                                    );
                                }
                            }
                        });
                        ui.add(
                            egui::Label::new(
                                RichText::new(&entry.installed.file.path)
                                    .size(11.0)
                                    .color(theme::HINT),
                            )
                            .truncate(),
                        );
                    });
                ui.add_space(8.0);
            }
        });
    if !catalog.other_files.is_empty() {
        egui::CollapsingHeader::new(format!("其他本地模组（{} 个）", catalog.other_files.len()))
            .default_open(catalog.entries.is_empty())
            .show(ui, |ui| {
                app::hint(
                    ui,
                    "这些 JAR 没有 NSCL 单独安装记录，不影响游戏读取；点击文件名查看位置和大小。",
                );
                for name in &catalog.other_files {
                    if ui
                        .add(
                            egui::Label::new(RichText::new(name).color(theme::ACCENT))
                                .truncate()
                                .sense(Sense::click()),
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text("在文件页查看位置和大小")
                        .clicked()
                    {
                        app.instance_browser.tab = crate::instance_ui::DetailTab::Files;
                        app.instance_browser.files.select(
                            crate::instance_files::Folder::Mods,
                            Some(name.as_str().into()),
                        );
                    }
                }
            });
    }
    for warning in &app.service.content.updates.warnings {
        ui.label(RichText::new(warning).size(12.0).color(theme::MUTED));
    }
}

fn search(ui: &mut Ui, app: &mut NsclApp) {
    let busy = app.service.busy();
    let mut back = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        back = download_button(ui, "返回已装模组", !busy);
        ui.add(
            egui::Label::new(
                RichText::new("从 Modrinth 添加")
                    .size(16.0)
                    .color(theme::HEADING),
            )
            .truncate(),
        );
    });
    if back {
        app.service.content.adding = false;
        app.service.content.project = None;
        app.service.content.versions.clear();
        return;
    }
    ui.add_space(10.0);
    let mut search = false;
    ui.add_enabled_ui(!busy, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            let response = ui.add(
                egui::TextEdit::singleline(&mut app.service.content.query)
                    .font(FontId::proportional(13.0))
                    .frame(input_frame())
                    .margin(Vec2::new(12.0, 8.0))
                    .hint_text("搜索模组，例如 Sodium")
                    .desired_width((ui.available_width() - 108.0).max(150.0)),
            );
            input_focus(ui, &response);
            search = download_button(ui, "搜索", true)
                || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
        });
    });
    if search {
        app.service.content_search(0, &app.settings, ui.ctx());
    }
    ui.add_space(8.0);
    app::hint(
        ui,
        "仅列出与此实例的游戏版本和加载器兼容的模组；安装前会先给出依赖方案。",
    );
    ui.add_space(10.0);
    let hits = app.service.content.results.hits.clone();
    if hits.is_empty() {
        app::body(
            ui,
            if busy {
                "正在获取资源……"
            } else if app.service.content.searched {
                "没有匹配的模组，请调整关键词。"
            } else {
                "输入关键词后搜索，留空可浏览热门模组。"
            },
        );
        return;
    }
    egui::ScrollArea::vertical()
        .id_salt("instance-mod-search")
        .max_height(260.0)
        .min_scrolled_height(0.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for project in &hits {
                if ui
                    .push_id(&project.project_id, |ui| {
                        row(
                            ui,
                            &project.title,
                            &project.description,
                            &format!("{} · {} 次下载", project.author, project.downloads),
                            "查看版本",
                            !busy,
                        )
                    })
                    .inner
                {
                    app.service
                        .content_versions(project.clone(), &app.settings, ui.ctx());
                }
            }
        });
}

fn versions(ui: &mut Ui, app: &mut NsclApp) {
    let Some(project) = app.service.content.project.clone() else {
        return;
    };
    let busy = app.service.busy();
    let mut back = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        back = download_button(ui, "返回列表", !busy);
        ui.add(
            egui::Label::new(
                RichText::new(&project.title)
                    .size(16.0)
                    .color(theme::HEADING),
            )
            .truncate(),
        );
    });
    if back {
        app.service.content.project = None;
        app.service.content.versions.clear();
        return;
    }
    ui.add_space(10.0);
    app::hint(ui, "仅列出与此实例匹配的版本；点击后先查看依赖方案。");
    ui.add_space(10.0);
    let versions = app.service.content.versions.clone();
    if versions.is_empty() {
        app::body(
            ui,
            if busy {
                "正在获取发布版本……"
            } else {
                "没有适用于当前实例的可安装版本。"
            },
        );
        return;
    }
    egui::ScrollArea::vertical()
        .id_salt("instance-mod-versions")
        .max_height(260.0)
        .min_scrolled_height(0.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for version in &versions {
                let release = match version.version_type.as_str() {
                    "release" => "正式版",
                    "beta" => "Beta",
                    _ => "Alpha",
                };
                if ui
                    .push_id(&version.id, |ui| {
                        row(
                            ui,
                            &version.name,
                            &format!(
                                "Minecraft {} · {}",
                                version.game_versions.join(", "),
                                version.loaders.join(" / ")
                            ),
                            &format!(
                                "{} · {} · {}",
                                version.version_number,
                                release,
                                version.date_published.get(..10).unwrap_or("")
                            ),
                            "查看方案",
                            !busy,
                        )
                    })
                    .inner
                {
                    app.service
                        .content_preview(version.clone(), &app.settings, ui.ctx());
                }
            }
        });
}

fn plan(ui: &mut Ui, app: &mut NsclApp, editable: bool) {
    let Some(plan) = app.service.content.plan.clone() else {
        return;
    };
    let busy = app.service.busy();
    let removal = matches!(plan, ContentPlan::Remove(_));
    let mut back = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        back = download_button(ui, "返回", !busy);
        ui.add(
            egui::Label::new(
                RichText::new(if removal {
                    "确认卸载方案"
                } else {
                    "确认安装方案"
                })
                .size(16.0)
                .color(theme::HEADING),
            )
            .truncate(),
        );
    });
    if back {
        app.service.content.plan = None;
        return;
    }
    ui.add_space(12.0);
    match plan {
        ContentPlan::Mod(plan) => {
            ui.add(
                egui::Label::new(RichText::new(plan.target.label()).color(theme::BODY_TEXT)).wrap(),
            );
            app::body(
                ui,
                &format!(
                    "将安装 {} 个模组及必需依赖，游戏版本与加载器匹配。",
                    plan.versions.len()
                ),
            );
            for warning in &plan.warnings {
                ui.label(RichText::new(warning).color(theme::MUTED).size(13.0));
            }
            ui.separator();
            for version in &plan.versions {
                ui.label(
                    RichText::new(format!("{} · {}", version.name, version.version_number))
                        .color(theme::BODY_TEXT)
                        .size(14.0),
                );
            }
            app::hint(
                ui,
                "更新已登记模组会替换旧版本；存档和手动修改的同名文件会保留。",
            );
        }
        ContentPlan::Remove(plan) => {
            app::body(ui, &plan.target.label());
            app::body(ui, &format!("将卸载 {} 个已登记模组：", plan.entries.len()));
            for entry in &plan.entries {
                ui.label(
                    RichText::new(format!(
                        "{} · {}",
                        entry.version.name, entry.version.version_number
                    ))
                    .color(theme::HEADING),
                );
                app::hint(ui, &entry.file.path);
            }
            app::hint(
                ui,
                "仅移除以上 JAR 和对应安装记录；存档、配置与未选择的依赖保留。手动修改的文件会阻止卸载。",
            );
            if plan.unmanaged_count > 0 {
                app::hint(
                    ui,
                    &format!(
                        "另有 {} 个未独立登记的 JAR，无法核验其依赖，请确认它们不依赖所选模组。",
                        plan.unmanaged_count
                    ),
                );
            }
        }
        ContentPlan::Pack(_) => {}
    }
    ui.add_space(16.0);
    ui.separator();
    ui.add_space(12.0);
    if app.service.game.is_some() {
        app::hint(ui, "请先退出当前游戏，再修改资源。");
    }
    ui.add_enabled_ui(!busy && editable, |ui| {
        if app::primary_button(
            ui,
            if removal {
                "确认卸载所选模组"
            } else {
                "安装模组及依赖"
            },
        ) {
            app.service.content_install(&app.settings, ui.ctx());
        }
    });
}

fn row(
    ui: &mut Ui,
    title: &str,
    description: &str,
    meta: &str,
    action: &str,
    enabled: bool,
) -> bool {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 64.0), Sense::hover());
    ui.painter().rect_filled(rect, 8.0, theme::TOPBAR_BG);
    ui.painter().rect_stroke(
        rect,
        8.0,
        Stroke::new(1.0, theme::CARD_BORDER),
        egui::StrokeKind::Inside,
    );
    let mut info = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_max(
        rect.min + Vec2::new(14.0, 8.0),
        Pos2::new(rect.right() - 120.0, rect.bottom() - 6.0),
    )));
    info.spacing_mut().item_spacing.y = 3.0;
    info.add(
        egui::Label::new(
            RichText::new(title)
                .size(14.0)
                .color(theme::HEADING)
                .family(theme::bold_family()),
        )
        .truncate(),
    );
    info.add(
        egui::Label::new(RichText::new(description).size(11.0).color(theme::MUTED)).truncate(),
    );
    info.add(egui::Label::new(RichText::new(meta).size(10.0).color(theme::HINT)).truncate());
    let mut button = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(
        Pos2::new(
            rect.right() - 110.0,
            rect.center().y - theme::CONTROL_HEIGHT / 2.0,
        ),
        Vec2::new(96.0, theme::CONTROL_HEIGHT),
    )));
    let clicked = download_button(&mut button, action, enabled);
    ui.add_space(4.0);
    clicked
}

fn open_mods(app: &mut NsclApp, instance: &Instance) {
    let result = instance
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

fn audit(ctx: &egui::Context, app: &mut NsclApp) {
    let Some(report) = app.service.content.audit.clone() else {
        return;
    };
    let mut open = true;
    egui::Window::new("实例兼容性检查")
        .open(&mut open)
        .default_width(650.0)
        .show(ctx, |ui| {
            app::body(
                ui,
                &format!(
                    "已检查 {} 个已登记文件 · {} 项问题 · {} 项提示",
                    report.checked_files,
                    report.errors.len(),
                    report.warnings.len()
                ),
            );
            egui::ScrollArea::vertical()
                .max_height(360.0)
                .show(ui, |ui| {
                    if report.errors.is_empty() {
                        ui.label(
                            RichText::new(if report.checked_files == 0 {
                                "此实例没有已登记的模组或整合包文件。"
                            } else {
                                "已登记文件的版本、依赖与完整性检查通过。"
                            })
                            .color(theme::ACCENT),
                        );
                    }
                    for error in &report.errors {
                        ui.label(error);
                    }
                    for warning in &report.warnings {
                        ui.label(RichText::new(warning).color(theme::MUTED));
                    }
                });
        });
    if !open {
        app.service.content.audit = None;
    }
}
