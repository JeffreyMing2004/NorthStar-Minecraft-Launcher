use crate::{
    app::{self, NsclApp},
    content::ContentKind,
    pages::{download_button, download_status_height, input_focus, input_frame, task_status},
    project_icons::Logo,
    service::ContentPlan,
    theme,
};
use egui::{Align, FontId, Pos2, Rect, RichText, Sense, Stroke, Ui, UiBuilder, Vec2};

/// The download page browses Modrinth on its own: mods are saved as files and packs are
/// installed as new instances, so nothing here depends on a selected instance.
pub fn show(ui: &mut Ui, app: &mut NsclApp) {
    let kind = if app.category == 1 {
        ContentKind::Modpack
    } else {
        ContentKind::Mod
    };
    app.service.content_browse(kind);
    if app.service.content.project.is_some() {
        versions(ui, app, kind);
    } else {
        catalog(ui, app, kind);
    }
}

fn catalog(ui: &mut Ui, app: &mut NsclApp, kind: ContentKind) {
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
                            "没有匹配的资源，请调整关键词。"
                        } else {
                            "输入关键词后搜索，留空可浏览热门资源。"
                        },
                    );
                    app::hint(
                        ui,
                        if kind == ContentKind::Mod {
                            "资源来自 Modrinth；下载后的 JAR 可直接放入任意实例的 mods 目录。"
                        } else {
                            "资源来自 Modrinth；整合包会安装为独立实例。"
                        },
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
    if let Some(path) = app.service.content.downloaded.clone() {
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 18.0;
            ui.add(
                egui::Label::new(
                    RichText::new(format!("已下载：{}", path.display()))
                        .size(11.0)
                        .color(theme::HINT),
                )
                .truncate(),
            )
            .on_hover_text(path.to_string_lossy());
            if download_button(ui, "打开下载目录", true) {
                let directory = path.parent().map(std::path::Path::to_path_buf);
                if let Some(directory) = directory
                    && let Err(error) = open::that_detached(directory).map_err(|e| e.to_string())
                {
                    app.service.error = Some(error);
                }
            }
        });
    }
    task_status(ui, app);
}

fn versions(ui: &mut Ui, app: &mut NsclApp, kind: ContentKind) {
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
        if kind == ContentKind::Mod {
            "列出全部发布版本；下载后把 JAR 放入任意实例的 mods 目录，或在版本管理中按实例安装。"
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
                        "没有可下载的发布版本。"
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
                                if kind == ContentKind::Mod {
                                    "下载"
                                } else {
                                    "查看方案"
                                },
                                !app.service.busy(),
                                None,
                            )
                        })
                        .inner
                    {
                        if kind == ContentKind::Mod {
                            app.service
                                .content_download(version.clone(), &app.settings, ui.ctx());
                        } else {
                            app.service
                                .content_preview(version.clone(), &app.settings, ui.ctx());
                        }
                    }
                }
            });
    });
    task_status(ui, app);
}

pub fn plan(ui: &mut Ui, app: &mut NsclApp) {
    if app.service.content.download_pack_plan().is_none() {
        return;
    }
    let mut back = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 18.0;
        back = download_button(ui, "返回资源", !app.service.busy());
        if let Some(project) = &app.service.content.project {
            let logo = app.project_icons.get(ui.ctx(), project.icon_url.as_deref());
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(36.0), Sense::hover());
            logo.paint(ui, rect, &project.title);
        }
        app::heading(ui, "确认安装方案");
    });
    if back {
        app.service.content.plan = None;
        return;
    }
    ui.add_space(16.0);
    let height = (ui.available_height() - download_status_height(app) - 90.0).max(0.0);
    let busy = app.service.busy();
    let mut include_optional = None;
    list_area(ui, height, |ui| {
        egui::ScrollArea::vertical().id_salt("content-plan").max_height(height).min_scrolled_height(0.0).auto_shrink([false,false]).show(ui,|ui| {
            ui.spacing_mut().item_spacing.y=10.0;
            let Some(ContentPlan::Pack(plan)) = &app.service.content.plan else { return };
            ui.label(RichText::new(plan.label()).color(theme::BODY_TEXT).size(16.0));
            app::body(ui,"安装到新的独立实例，自动准备整合包要求的游戏和加载器。");
            if plan.optional_files>0 {
                let mut choice = plan.include_optional;
                ui.add_enabled_ui(!busy,|ui|{
                    if ui.checkbox(&mut choice,format!("包含 {} 个可选客户端文件",plan.optional_files)).changed() {include_optional=Some(choice);}
                });
            }
            app::body(ui,&format!("{} 个下载文件 · {} 个配置及附带文件",plan.download_count(),plan.override_count()));
            app::hint(ui,"仅安装客户端需要的文件；不会覆盖现有实例。兼容性依据整合包清单，启动时仍可能受到第三方模组组合影响。");
        });
    });
    if let (Some(choice), Some(ContentPlan::Pack(plan))) =
        (include_optional, &mut app.service.content.plan)
    {
        plan.include_optional = choice;
    }
    ui.add_space(16.0);
    ui.separator();
    ui.add_space(12.0);
    if app.service.game.is_some() {
        app::hint(ui, "请先退出当前游戏，再修改资源。");
    }
    ui.add_enabled_ui(!busy && app.service.game.is_none(), |ui| {
        if app::primary_button(ui, "安装整合包") {
            app.service.content_install_pack(&app.settings, ui.ctx());
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
