use crate::{
    app::{self, NsclApp},
    content::ContentKind,
    pages::{download_button, download_status_height, input_focus, input_frame, task_status},
    project_icons::Logo,
    service::ContentPlan,
    theme,
};
use egui::{Align, FontId, Pos2, Rect, RichText, Sense, Stroke, Ui, UiBuilder, Vec2};

pub fn show(ui: &mut Ui, app: &mut NsclApp) {
    let kind = if app.category == 1 {
        ContentKind::Modpack
    } else {
        ContentKind::Mod
    };
    app.service.content_kind(kind);
    if app.service.content.project.is_some() {
        versions(ui, app);
    } else {
        catalog(ui, app, kind);
    }
    audit(ui.ctx(), app);
    crate::manager_ui::show(ui.ctx(), app);
}

fn catalog(ui: &mut Ui, app: &mut NsclApp, kind: ContentKind) {
    if kind == ContentKind::Mod {
        ui.add_enabled_ui(!app.service.busy(), |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                ui.label("目标实例");
                let before = app.service.content.target.clone();
                let text = app
                    .service
                    .instances
                    .iter()
                    .find(|i| i.id == before)
                    .map(|i| i.name.as_str())
                    .unwrap_or("先安装游戏并选择实例");
                egui::ComboBox::from_id_salt("content-instance")
                    .width(340.0)
                    .selected_text(text)
                    .show_ui(ui, |ui| {
                        for instance in &app.service.instances {
                            ui.selectable_value(
                                &mut app.service.content.target,
                                instance.id.clone(),
                                &instance.name,
                            );
                        }
                    });
                if before != app.service.content.target {
                    app.service.content.results = Default::default();
                    app.service.content.searched = false;
                    app.service.content.audit = None;
                    app.service.content.managed = None;
                    app.service.content.managed_selection.clear();
                    app.service.content.updates = Default::default();
                    app.service.content.updates_checked = false;
                }
                if download_button(ui, "检查兼容性", !app.service.instances.is_empty()) {
                    app.service.content_check(&app.settings, ui.ctx());
                }
                if download_button(ui, "管理已装模组", !app.service.instances.is_empty()) {
                    app.service.content_manage(&app.settings, ui.ctx(), false);
                }
            });
        });
        ui.add_space(8.0);
        let label = app.service.content_target_label().unwrap_or_else(|e| e);
        ui.add(egui::Label::new(RichText::new(label).size(11.0).color(theme::HINT)).truncate());
        ui.add_space(14.0);
    }
    let mut search = false;
    ui.add_enabled_ui(!app.service.busy(), |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 18.0;
            let response = ui.add(
                egui::TextEdit::singleline(&mut app.service.content.query)
                    .font(FontId::proportional(14.0))
                    .frame(input_frame())
                    .margin(Vec2::new(14.0, 10.0))
                    .hint_text(if kind == ContentKind::Mod {
                        "搜索 Modrinth 模组，例如 Sodium"
                    } else {
                        "搜索 Modrinth 整合包"
                    })
                    .desired_width((ui.available_width() - 132.0).max(180.0)),
            );
            input_focus(ui, &response);
            search = download_button(ui, "搜索", true)
                || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
        });
    });
    if search {
        app.service.content_search(0, &app.settings, ui.ctx());
    }
    ui.add_space(14.0);
    let height = (ui.available_height() - download_status_height(app) - 46.0).max(0.0);
    let hits = app.service.content.results.hits.clone();
    list_area(ui, height, |ui| {
        if hits.is_empty() {
            if height >= 50.0 {
                ui.add_space(((height - 50.0) * 0.5).max(0.0));
                ui.vertical_centered(|ui| {
                    app::body(
                        ui,
                        if app.service.busy() {
                            "正在获取资源……"
                        } else if app.service.content.searched {
                            "没有匹配的资源，请调整关键词或目标实例。"
                        } else {
                            "输入关键词后搜索，留空可浏览热门资源。"
                        },
                    );
                    app::hint(
                        ui,
                        "资源来自 Modrinth；安装前会检查版本、加载器与必需依赖。",
                    );
                });
            }
            return;
        }
        ui.spacing_mut().item_spacing.y = 10.0;
        egui::ScrollArea::vertical()
            .id_salt("content-projects")
            .max_height(height)
            .min_scrolled_height(0.0)
            .auto_shrink([false, false])
            .show_rows(ui, 82.0, hits.len(), |ui, range| {
                for index in range {
                    let project = &hits[index];
                    let logo = app.project_icons.get(ui.ctx(), project.icon_url.as_deref());
                    if ui
                        .push_id(&project.project_id, |ui| {
                            row(
                                ui,
                                &project.title,
                                &project.description,
                                &format!(
                                    "{} · {} 次下载 · Modrinth",
                                    project.author, project.downloads
                                ),
                                "查看版本",
                                !app.service.busy(),
                                Some(logo),
                            )
                        })
                        .inner
                    {
                        app.service
                            .content_versions(project.clone(), &app.settings, ui.ctx());
                    }
                }
            });
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        let page = &app.service.content.results;
        let (offset, total) = (page.offset, page.total_hits);
        if download_button(ui, "上一页", offset > 0 && !app.service.busy()) {
            app.service
                .content_search(offset.saturating_sub(20), &app.settings, ui.ctx());
        }
        if download_button(ui, "下一页", offset + 20 < total && !app.service.busy()) {
            app.service
                .content_search(offset + 20, &app.settings, ui.ctx());
        }
        app::hint(ui, &format!("{} 条结果 · 第 {} 页", total, offset / 20 + 1));
    });
    task_status(ui, app);
}

fn versions(ui: &mut Ui, app: &mut NsclApp) {
    let Some(project) = app.service.content.project.clone() else {
        return;
    };
    let mut back = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 18.0;
        back = download_button(ui, "返回搜索", !app.service.busy());
        let logo = app.project_icons.get(ui.ctx(), project.icon_url.as_deref());
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(36.0), Sense::hover());
        logo.paint(ui, rect, &project.title);
        ui.add(
            egui::Label::new(
                RichText::new(&project.title)
                    .size(19.0)
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
    ui.add_space(12.0);
    app::hint(
        ui,
        if app.service.content.kind == ContentKind::Mod {
            "仅列出与目标实例的游戏版本和加载器匹配的版本；点击后先查看依赖方案。"
        } else {
            "选择发布版本，检查整合包要求后创建独立实例。"
        },
    );
    ui.add_space(14.0);
    let height = (ui.available_height() - download_status_height(app)).max(0.0);
    let versions = app.service.content.versions.clone();
    list_area(ui, height, |ui| {
        if versions.is_empty() {
            if height >= 30.0 {
                app::body(
                    ui,
                    if app.service.busy() {
                        "正在获取发布版本……"
                    } else {
                        "没有适用于当前选择的可安装版本。"
                    },
                );
            }
            return;
        }
        ui.spacing_mut().item_spacing.y = 10.0;
        egui::ScrollArea::vertical()
            .id_salt("content-versions")
            .max_height(height)
            .min_scrolled_height(0.0)
            .auto_shrink([false, false])
            .show_rows(ui, 82.0, versions.len(), |ui, range| {
                for index in range {
                    let version = &versions[index];
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
                                !app.service.busy(),
                                None,
                            )
                        })
                        .inner
                    {
                        app.service
                            .content_preview(version.clone(), &app.settings, ui.ctx());
                    }
                }
            });
    });
    task_status(ui, app);
}

pub fn plan(ui: &mut Ui, app: &mut NsclApp) {
    let mut back = false;
    let removal = matches!(app.service.content.plan, Some(ContentPlan::Remove(_)));
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 18.0;
        back = download_button(ui, "返回资源", !app.service.busy());
        if let Some(project) = &app.service.content.project {
            let logo = app.project_icons.get(ui.ctx(), project.icon_url.as_deref());
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(36.0), Sense::hover());
            logo.paint(ui, rect, &project.title);
        }
        app::heading(
            ui,
            if removal {
                "确认卸载方案"
            } else {
                "确认安装方案"
            },
        );
    });
    if back {
        app.service.content.plan = None;
        app.service.content.manager_open = removal;
        return;
    }
    ui.add_space(16.0);
    let height = (ui.available_height() - download_status_height(app) - 90.0).max(0.0);
    let busy = app.service.busy();
    list_area(ui, height, |ui| {
        egui::ScrollArea::vertical().id_salt("content-plan").max_height(height).min_scrolled_height(0.0).auto_shrink([false,false]).show(ui,|ui| {
            ui.spacing_mut().item_spacing.y=10.0;
            match &mut app.service.content.plan {
                Some(ContentPlan::Mod(plan))=>{
                    ui.add(egui::Label::new(RichText::new(plan.target.label()).color(theme::BODY_TEXT)).wrap());
                    app::body(ui,&format!("将安装 {} 个模组及必需依赖，游戏版本与加载器匹配。",plan.versions.len()));
                    for warning in &plan.warnings {ui.label(RichText::new(warning).color(theme::MUTED).size(13.0));}
                    ui.separator();
                    for version in &plan.versions {
                        ui.label(RichText::new(format!("{} · {}",version.name,version.version_number)).color(theme::BODY_TEXT).size(14.0));
                    }
                    app::hint(ui,"更新已登记模组会替换旧版本；存档和手动修改的同名文件会保留。");
                }
                Some(ContentPlan::Pack(plan))=>{
                    ui.label(RichText::new(plan.label()).color(theme::BODY_TEXT).size(16.0));
                    app::body(ui,"安装到新的独立实例，自动准备整合包要求的游戏和加载器。");
                    if plan.optional_files>0 {
                        ui.add_enabled_ui(!busy,|ui|{ui.checkbox(&mut plan.include_optional,format!("包含 {} 个可选客户端文件",plan.optional_files));});
                    }
                    app::body(ui,&format!("{} 个下载文件 · {} 个配置及附带文件",plan.download_count(),plan.override_count()));
                    app::hint(ui,"仅安装客户端需要的文件；不会覆盖现有实例。兼容性依据整合包清单，启动时仍可能受到第三方模组组合影响。");
                }
                Some(ContentPlan::Remove(plan))=>{
                    app::body(ui,&plan.target.label());
                    app::body(ui,&format!("将卸载 {} 个已登记模组：",plan.entries.len()));
                    for entry in &plan.entries {ui.label(RichText::new(format!("{} · {}",entry.version.name,entry.version.version_number)).color(theme::HEADING));app::hint(ui,&entry.file.path);}
                    app::hint(ui,"仅移除以上 JAR 和对应安装记录；存档、配置与未选择的依赖保留。手动修改的文件会阻止卸载。");
                    if plan.unmanaged_count>0 {app::hint(ui,&format!("另有 {} 个未独立登记的 JAR，无法核验其依赖，请确认它们不依赖所选模组。",plan.unmanaged_count));}
                }
                None=>{}
            }
        });
    });
    ui.add_space(16.0);
    ui.separator();
    ui.add_space(12.0);
    if app.service.game.is_some() {
        app::hint(ui, "请先退出当前游戏，再修改资源。");
    }
    let pack = matches!(app.service.content.plan, Some(ContentPlan::Pack(_)));
    ui.add_enabled_ui(!busy && app.service.game.is_none(), |ui| {
        if app::primary_button(
            ui,
            if removal {
                "确认卸载所选模组"
            } else if pack {
                "安装整合包"
            } else {
                "安装模组及依赖"
            },
        ) {
            app.service.content_install(&app.settings, ui.ctx());
        }
    });
    task_status(ui, app);
}

fn list_area(ui: &mut Ui, height: f32, add: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(
        Vec2::new(ui.available_width(), height),
        egui::Layout::top_down(Align::Min),
        |ui| {
            ui.set_min_height(height);
            add(ui);
        },
    );
}

fn row(
    ui: &mut Ui,
    title: &str,
    description: &str,
    meta: &str,
    action: &str,
    enabled: bool,
    logo: Option<Logo>,
) -> bool {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 82.0), Sense::hover());
    ui.painter().rect_filled(rect, 10.0, theme::TOPBAR_BG);
    ui.painter().rect_stroke(
        rect,
        10.0,
        Stroke::new(1.0, theme::CARD_BORDER),
        egui::StrokeKind::Inside,
    );
    let text_offset = if let Some(logo) = logo {
        logo.paint(
            ui,
            Rect::from_min_size(rect.min + Vec2::new(16.0, 13.0), Vec2::splat(56.0)),
            title,
        );
        88.0
    } else {
        16.0
    };
    let mut info = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_max(
        rect.min + Vec2::new(text_offset, 10.0),
        Pos2::new(rect.right() - 132.0, rect.bottom() - 8.0),
    )));
    info.spacing_mut().item_spacing.y = 4.0;
    info.add(
        egui::Label::new(
            RichText::new(title)
                .size(15.0)
                .color(theme::HEADING)
                .family(theme::bold_family()),
        )
        .truncate(),
    );
    info.add(
        egui::Label::new(RichText::new(description).size(12.0).color(theme::MUTED)).truncate(),
    );
    info.add(egui::Label::new(RichText::new(meta).size(11.0).color(theme::HINT)).truncate());
    let mut button = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(
        Pos2::new(
            rect.right() - 114.0,
            rect.center().y - theme::CONTROL_HEIGHT / 2.0,
        ),
        Vec2::new(96.0, theme::CONTROL_HEIGHT),
    )));
    download_button(&mut button, action, enabled)
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
