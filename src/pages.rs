use crate::app::{self, NsclApp, Page};
use crate::theme;
use egui::{Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, Ui, UiBuilder, Vec2};

pub fn home(ui: &mut Ui, app: &mut NsclApp) {
    if app.service.busy() {
        task_status(ui, app);
        ui.add_space(10.0);
    }
    let width = ui.available_width();
    let wide = width >= 800.0;
    let height = ui.available_height().clamp(380.0, 560.0);
    let total_height = if wide { height } else { height * 2.0 + 22.0 };
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, total_height), Sense::hover());
    let left_width = if wide { (width - 22.0) * 0.65 } else { width };
    let launch_rect = Rect::from_min_size(rect.min, Vec2::new(left_width, height));
    let aside_rect = if wide {
        Rect::from_min_max(Pos2::new(launch_rect.right() + 22.0, rect.top()), rect.max)
    } else {
        Rect::from_min_size(
            Pos2::new(rect.left(), launch_rect.bottom() + 22.0),
            Vec2::new(width, height),
        )
    };
    draw_card(ui, launch_rect);
    draw_card(ui, aside_rect);
    let banner_height = if height >= 500.0 {
        224.0
    } else if height >= 440.0 {
        186.0
    } else {
        140.0
    };
    let banner = Rect::from_min_size(launch_rect.min, Vec2::new(left_width, banner_height));
    landscape(ui.painter(), banner);
    let content = Rect::from_min_max(
        launch_rect.min + Vec2::new(28.0, banner_height + 24.0),
        launch_rect.max - Vec2::new(28.0, 24.0),
    );
    let mut launch = ui.new_child(UiBuilder::new().id_salt("launch-content").max_rect(content));
    launch.spacing_mut().item_spacing.y = 0.0;
    launch.horizontal(|ui| {
        ui.spacing_mut().interact_size.y = 24.0;
        ui.label(
            RichText::new("游戏实例")
                .size(15.0)
                .family(theme::bold_family()),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(format!("{} 个已安装", app.service.instances.len()))
                    .size(12.0)
                    .color(theme::HINT),
            );
        });
    });
    launch.add_space(14.0);
    let (instance, _) =
        launch.allocate_exact_size(Vec2::new(content.width(), 76.0), Sense::hover());
    let p = launch.painter();
    p.rect_filled(instance, 12.0, theme::INPUT_BG);
    p.rect_stroke(
        instance,
        12.0,
        Stroke::new(1.0, theme::CARD_BORDER),
        egui::StrokeKind::Inside,
    );
    let icon = Rect::from_center_size(
        Pos2::new(instance.left() + 32.0, instance.center().y),
        Vec2::splat(34.0),
    );
    p.rect_filled(icon, 10.0, theme::nav_active_bg());
    theme::icon(p, icon.shrink(8.0), theme::Icon::Folder, theme::ACCENT);
    if app.service.instances.is_empty() {
        p.text(
            instance.min + Vec2::new(64.0, 27.0),
            egui::Align2::LEFT_CENTER,
            "暂无游戏实例",
            FontId::new(14.0, theme::bold_family()),
            theme::HEADING,
        );
        p.text(
            instance.min + Vec2::new(64.0, 51.0),
            egui::Align2::LEFT_CENTER,
            "原版 · Forge · Fabric · NeoForge",
            FontId::proportional(12.0),
            theme::MUTED,
        );
    } else {
        let mut selection = launch.new_child(UiBuilder::new().max_rect(Rect::from_min_max(
            instance.min + Vec2::new(64.0, 18.0),
            instance.max - Vec2::new(16.0, 18.0),
        )));
        let previous = app.service.selected.clone();
        let selected_name = app
            .service
            .instances
            .iter()
            .find(|i| i.id == app.service.selected)
            .map(|i| i.name.as_str())
            .unwrap_or("选择实例");
        egui::ComboBox::from_id_salt("installed-version")
            .width(selection.available_width())
            .wrap_mode(egui::TextWrapMode::Truncate)
            .selected_text(selected_name)
            .show_ui(&mut selection, |ui| {
                for instance in &app.service.instances {
                    ui.selectable_value(
                        &mut app.service.selected,
                        instance.id.clone(),
                        format!("{} · {}", instance.name, instance.version_id),
                    );
                }
            });
        if previous != app.service.selected {
            app.remember_instance();
        }
    }
    let mut actions = launch.new_child(UiBuilder::new().max_rect(Rect::from_min_max(
        Pos2::new(content.left(), content.bottom() - 46.0),
        content.max,
    )));
    if app.service.instances.is_empty() {
        if app::primary_button(
            &mut actions,
            if app.service.installed.is_empty() {
                "安装 Minecraft  →"
            } else {
                "创建游戏实例  →"
            },
        ) {
            if app.service.installed.is_empty() {
                app.page = Page::Downloads;
                app.category = 0;
                app.download = Default::default();
            } else {
                app.page = Page::Instances;
            }
        }
    } else {
        theme::toolbar(&mut actions, |ui| {
            ui.add_enabled_ui(!app.service.busy() && app.service.game.is_none(), |ui| {
                if app::primary_button(
                    ui,
                    if app.service.game.is_some() {
                        "游戏运行中"
                    } else {
                        "启动游戏 →"
                    },
                ) {
                    app.service.launch(&app.settings, &app.accounts, ui.ctx());
                }
            });
            if ui.button("管理实例").clicked() {
                app.page = Page::Instances;
            }
            if app.service.last_log.is_some() && ui.button("查看日志").clicked() {
                app.open_log();
            }
        });
    }
    if app.service.busy() || !app.service.notice.is_empty() {
        launch.add_space(12.0);
        let status = if app.service.busy() {
            &app.service.progress.stage
        } else {
            &app.service.notice
        };
        launch
            .add(egui::Label::new(RichText::new(status).size(12.0).color(theme::HINT)).truncate());
    }

    let content = aside_rect.shrink(24.0);
    let mut aside = ui.new_child(
        UiBuilder::new()
            .id_salt("account-summary")
            .max_rect(content),
    );
    aside.spacing_mut().item_spacing.y = 0.0;
    aside.horizontal(|ui| {
        ui.label(
            RichText::new("我的角色")
                .size(16.0)
                .family(theme::bold_family())
                .color(theme::HEADING),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            app::badge_muted(
                ui,
                if app.settings.online_mode && app.accounts.list.current().is_some() {
                    "正版账号"
                } else if app.settings.online_mode {
                    "未登录"
                } else {
                    if app.accounts.list.current().is_some() {
                        "离线账号"
                    } else {
                        "未选择账号"
                    }
                },
            )
        });
    });
    let preview = Rect::from_min_max(
        content.min + Vec2::new(0.0, 40.0),
        Pos2::new(content.right(), content.bottom() - 140.0),
    );
    let center = preview.center();
    glow(
        aside.painter(),
        preview,
        center,
        Vec2::new(preview.width() * 0.48, preview.height() * 0.48),
        Color32::from_rgb(0x35, 0x59, 0x66),
        0.18,
    );
    app.skin.paint(&mut aside, preview, "home-skin-model");
    let footer = Rect::from_min_max(
        Pos2::new(content.left(), content.bottom() - 115.0),
        content.max,
    );
    let mut account = aside.new_child(UiBuilder::new().max_rect(footer));
    account.spacing_mut().item_spacing.y = 0.0;
    account.label(
        RichText::new(
            app.accounts
                .list
                .current()
                .map(|a| a.name.as_str())
                .unwrap_or("你的冒险身份"),
        )
        .size(16.0)
        .family(theme::bold_family())
        .color(theme::HEADING),
    );
    account.add_space(10.0);
    account.label(
        RichText::new("管理账号，为角色换上新的模样。")
            .size(12.0)
            .color(theme::MUTED),
    );
    account.add_space(20.0);
    account.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        for (label, tab) in [("管理账号", 0), ("更换皮肤", 1)] {
            if app::styled_button(
                ui,
                RichText::new(label).size(13.0),
                &app::ButtonStyle {
                    fill: theme::SECONDARY_BG,
                    stroke: Some(Stroke::new(1.0, theme::SECONDARY_BORDER)),
                    min_size: Vec2::new((footer.width() - 12.0) * 0.5, 42.0),
                    ..Default::default()
                },
            )
            .clicked()
            {
                app.page = Page::Account;
                app.account_tab = tab;
            }
        }
    });
}

fn draw_card(ui: &Ui, rect: Rect) {
    ui.painter().rect_filled(rect, 18.0, theme::CARD_BG);
    ui.painter().rect_stroke(
        rect,
        18.0,
        Stroke::new(1.0, theme::CARD_BORDER),
        egui::StrokeKind::Inside,
    );
}

// Vertex colors interpolate in the renderer: no noisy strips or stacked opaque rings.
fn glow(p: &egui::Painter, clip: Rect, center: Pos2, radius: Vec2, color: Color32, opacity: f32) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(center, color.linear_multiply(opacity));
    for i in 0..=64 {
        let angle = i as f32 / 64.0 * std::f32::consts::TAU;
        mesh.colored_vertex(
            center + Vec2::new(angle.cos() * radius.x, angle.sin() * radius.y),
            Color32::TRANSPARENT,
        );
        if i > 0 {
            mesh.add_triangle(0, i, i + 1);
        }
    }
    p.with_clip_rect(clip).add(egui::Shape::mesh(mesh));
}

fn landscape(p: &egui::Painter, rect: Rect) {
    let mut mesh = egui::Mesh::default();
    // Curved upper corners, with a smooth diagonal sky gradient.
    for row in 0..=32 {
        let y = rect.top() + rect.height() * row as f32 / 32.0;
        let dy = (18.0 - (y - rect.top())).max(0.0);
        let inset = if dy > 0.0 {
            18.0 - (324.0 - dy * dy).sqrt()
        } else {
            0.0
        };
        for col in 0..=32 {
            let t = col as f32 / 32.0;
            let k = (t * 0.6 + row as f32 / 32.0 * 0.4).clamp(0.0, 1.0);
            let mix = |a: f32, b: f32| (a + (b - a) * k) as u8;
            mesh.colored_vertex(
                Pos2::new(egui::lerp(rect.left() + inset..=rect.right() - inset, t), y),
                Color32::from_rgb(mix(22.0, 58.0), mix(43.0, 79.0), mix(42.0, 64.0)),
            );
            if row > 0 && col > 0 {
                let i = row * 33 + col;
                mesh.add_triangle(i - 34, i - 1, i);
                mesh.add_triangle(i - 34, i, i - 33);
            }
        }
    }
    p.add(egui::Shape::mesh(mesh));
    glow(
        p,
        rect,
        rect.min + Vec2::new(rect.width() * 0.72, rect.height() * 0.18),
        Vec2::new(rect.width() * 0.45, rect.height() * 0.9),
        Color32::from_rgb(0xb3, 0xd6, 0x96),
        0.16,
    );
    let moon = rect.min + Vec2::new(rect.width() * 0.82, 57.0);
    glow(p, rect, moon, Vec2::splat(68.0), theme::STAR, 0.13);
    p.circle_filled(moon, 18.0, theme::STAR);
    mountain(p, rect, theme::MOUNTAIN_BACK, 1.2, 0.0, 19.0);
    mountain(p, rect, theme::MOUNTAIN_FRONT, 1.3, -0.12, 81.0);
    let art = p.with_clip_rect(rect);
    for (x, height) in [
        (0.61, 44.0),
        (0.68, 62.0),
        (0.76, 38.0),
        (0.89, 76.0),
        (0.96, 54.0),
    ] {
        let base = Pos2::new(rect.left() + rect.width() * x, rect.bottom() + 6.0);
        for (offset, width) in [(0.0, 0.32), (0.25, 0.25), (0.5, 0.18)] {
            let bottom = base.y - height * offset;
            art.add(egui::Shape::convex_polygon(
                vec![
                    Pos2::new(base.x, bottom - height * 0.5),
                    Pos2::new(base.x + height * width, bottom),
                    Pos2::new(base.x - height * width, bottom),
                ],
                Color32::from_rgb(0x16, 0x35, 0x2d),
                Stroke::NONE,
            ));
        }
    }
    for (x, y, radius) in [(0.71, 30.0, 2.0), (0.91, 32.0, 2.5), (0.95, 88.0, 1.5)] {
        theme::star4(
            p,
            rect.min + Vec2::new(rect.width() * x, y),
            radius,
            theme::STAR.gamma_multiply(0.7),
        );
    }
    theme::spaced_text(
        p,
        rect.min + Vec2::new(28.0, 30.0),
        "MINECRAFT JAVA EDITION",
        10.0,
        1.5,
        Color32::from_rgb(0xb4, 0xd2, 0xbf),
    );
    p.text(
        rect.min + Vec2::new(28.0, 59.0),
        egui::Align2::LEFT_TOP,
        "下一站，你的世界。",
        FontId::new(28.0, theme::bold_family()),
        theme::HEADING,
    );
    p.text(
        rect.min + Vec2::new(28.0, 105.0),
        egui::Align2::LEFT_TOP,
        "从这里出发，探索无限可能。",
        FontId::proportional(13.0),
        Color32::from_rgb(0xb8, 0xd2, 0xc3),
    );
}

fn mountain(p: &egui::Painter, rect: Rect, color: Color32, scale: f32, shift: f32, sink: f32) {
    let silhouette = [
        (0.0, 0.8),
        (0.18, 0.26),
        (0.28, 0.5),
        (0.48, 0.0),
        (0.65, 0.56),
        (0.78, 0.3),
        (1.0, 0.9),
        (1.0, 1.0),
        (0.0, 1.0),
    ];
    let width = rect.width() * scale;
    let height = 140.0 * scale;
    let left = rect.left() + shift * rect.width() + (rect.width() - width) * 0.5;
    let bottom = rect.bottom() + sink;
    let apex = Pos2::new(left, bottom);
    let points: Vec<_> = silhouette
        .iter()
        .map(|&(x, y)| Pos2::new(left + x * width, bottom - height + y * height))
        .collect();
    let painter = p.with_clip_rect(rect);
    for pair in points.windows(2) {
        painter.add(egui::Shape::convex_polygon(
            vec![apex, pair[0], pair[1]],
            color,
            Stroke::NONE,
        ));
    }
}

fn page_card(ui: &mut Ui, content: impl FnOnce(&mut Ui)) {
    let width = ui.available_width();
    theme::card(32).show(ui, |ui| {
        ui.set_width(width - 66.0);
        ui.set_min_height(450.0 - 64.0);
        ui.spacing_mut().item_spacing.y = 0.0;
        content(ui);
    });
}

pub fn account(ui: &mut Ui, app: &mut NsclApp) {
    crate::account_ui::show(ui, app);
}

pub fn downloads(ui: &mut Ui, app: &mut NsclApp) {
    const CATEGORIES: [&str; 3] = ["Minecraft", "整合包", "Mod"];
    let width = ui.available_width();
    let height = ui.available_height();
    theme::card(24).show(ui, |ui| {
        ui.set_width(width - 50.0);
        ui.set_min_height(height - 50.0);
        ui.spacing_mut().item_spacing.y = 0.0;
        if app.category == 0 && app.download.version.is_some() {
            installation_options(ui, app);
            return;
        }
        if app.category == 1 && app.service.content.download_pack_plan().is_some() {
            crate::content_page::plan(ui, app);
            return;
        }
        ui.add_enabled_ui(!app.service.busy(), |ui| {
            theme::toolbar(ui, |ui| {
                ui.spacing_mut().item_spacing.x = theme::TAB_GAP;
                for (index, item) in CATEGORIES.iter().enumerate() {
                    if app::tab_button(ui, item, app.category == index) {
                        app.category = index;
                    }
                }
            });
        });
        ui.add_space(18.0);
        ui.separator();
        if app.category == 0 {
            ui.add_space(20.0);
            minecraft_downloads(ui, app);
            return;
        }
        ui.add_space(20.0);
        crate::content_page::show(ui, app);
    });
}

pub fn settings(ui: &mut Ui, app: &mut NsclApp) {
    crate::settings_ui::show(ui, app);
}

pub fn instance_windows(ctx: &egui::Context, app: &mut NsclApp) {
    let root = crate::store::game_directory(&app.settings);
    let editable = !app.service.busy() && app.service.game.is_none();
    if let Some(mut instance) = app.instance_edit.take() {
        let mut open = true;
        let mut saved = false;
        egui::Window::new("实例配置")
            .open(&mut open)
            .default_width(530.0)
            .max_height((ctx.content_rect().height() - 120.0).max(220.0))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 12.0;
                ui.add_enabled_ui(editable, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("instance-configuration-form")
                        .max_height((ctx.content_rect().height() - 260.0).max(140.0))
                        .show(ui, |ui| {
                            ui.label(format!(
                                "Minecraft {} · Java {}",
                                instance.version_id,
                                app.service
                                    .installed
                                    .iter()
                                    .find(|v| v.id == instance.version_id)
                                    .map(|v| v.java_major)
                                    .unwrap_or(0)
                            ));
                            ui.label("实例名称");
                            ui.text_edit_singleline(&mut instance.name);
                            ui.label("版本隔离");
                            ui.add_enabled_ui(!instance.requires_isolation(), |ui| {
                                egui::ComboBox::from_id_salt("instance-isolation")
                                    .selected_text(instance.isolation.label())
                                    .show_ui(ui, |ui| {
                                        for mode in crate::instances::Isolation::ALL {
                                            ui.selectable_value(&mut instance.isolation, mode, mode.label());
                                        }
                                        if instance.is_external() {
                                            ui.selectable_value(&mut instance.isolation, crate::instances::Isolation::Version, crate::instances::Isolation::Version.label());
                                        }
                                    });
                            });
                            if instance.requires_isolation() {
                                app::hint(ui, "整合包始终使用独立目录，避免模组与配置混用。");
                            } else {
                                app::hint(ui, "切换后从所选目录读取存档、模组、配置和截图；原目录文件保留，不自动搬移。共享模式下不同版本会使用同一份游戏数据。");
                            }
                            if let Ok(path) = instance.game_directory(&root) {
                                app::hint(ui, &format!("游戏目录：{}", path.display()));
                            }
                            if instance.is_external() { app::hint(ui, "已识别其他启动器的版本配置，使用现有客户端与依赖；缺少文件时会提示具体路径，不会覆盖原配置。"); }
                            if instance.isolation == crate::instances::Isolation::Shared
                                && ui.button("打开保留的独立目录").clicked()
                                && let Err(error) = instance.directory(&root).and_then(|p| open::that_detached(p).map_err(|e| e.to_string()))
                            {
                                app.service.error = Some(error);
                            }
                            let mut override_java = instance.java_path.is_some();
                            if ui
                                .checkbox(&mut override_java, "为此实例单独选择 Java")
                                .changed()
                            {
                                instance.java_path = override_java.then(String::new);
                            }
                            if let Some(path) = &mut instance.java_path {
                                ui.add(
                                    egui::TextEdit::singleline(path)
                                        .hint_text("留空自动匹配 Java；否则填写可执行文件或安装目录"),
                                );
                                egui::ComboBox::from_id_salt("instance-java")
                                    .selected_text("使用已检测的 Java……")
                                    .show_ui(ui, |ui| {
                                        for runtime in &app.service.java_runtimes {
                                            if ui.selectable_label(false, runtime.label()).clicked() {
                                                *path = runtime.path.to_string_lossy().into_owned();
                                            }
                                        }
                                    });
                                if ui.button("扫描 Java").clicked() {
                                    app.service.inspect_java(None, ui.ctx());
                                }
                            }
                            let mut override_memory = instance.memory.is_some();
                            if ui
                                .checkbox(&mut override_memory, "为此实例单独分配内存")
                                .changed()
                            {
                                instance.memory = override_memory.then_some(app.settings.memory);
                            }
                            if instance.memory.is_some() {
                                ui.horizontal(|ui| {
                                    ui.text_edit_singleline(&mut app.instance_memory);
                                    ui.label("MiB（1024–32768）");
                                });
                            }
                        });
                    ui.separator();
                    if ui.button("保存实例配置").clicked() {
                        let result = (|| {
                            if instance.memory.is_some() {
                                instance.memory = Some(
                                    app.instance_memory
                                        .trim()
                                        .parse()
                                        .map_err(|_| "内存必须为整数。")?,
                                );
                            }
                            instance.name = instance.name.trim().to_owned();
                            if let Some(path) = &mut instance.java_path {
                                *path = path.trim().to_owned();
                            }
                            crate::instances::save(&root, &instance)
                        })();
                        match result {
                            Ok(()) => {
                                saved = true;
                                app.service.refresh(&app.settings);
                                app.service.content.plan = None;
                                app.service.content.managed = None;
                                app.service.content.managed_selection.clear();
                                app.service.content.updates = Default::default();
                                app.service.content.updates_checked = false;
                                app.instance_status = "实例配置已保存。".into();
                            }
                            Err(error) => app.service.error = Some(error),
                        }
                    }
                });
            });
        if open && !saved {
            app.instance_edit = Some(instance);
        }
    }
    if let Some(instance) = app.archive_confirm.clone() {
        egui::Window::new("归档实例确认")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(format!("将「{}」移入归档？", instance.name));
                ui.label(if instance.isolation != crate::instances::Isolation::Isolated {
                    "只归档该实例配置及保留的独立目录；实际游戏目录中的存档、模组和配置留在原处。"
                } else {
                    "存档、模组和配置会一起保留，可在管理实例的归档列表中恢复。"
                });
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(editable, egui::Button::new("确认归档"))
                        .clicked()
                    {
                        match crate::instances::archive(&root, &instance) {
                            Ok(_) => {
                                app.service.refresh(&app.settings);
                                app.remember_instance();
                                app.instance_browser.invalidate_archives();
                                app.instance_status = "实例已归档，存档和配置均已保留。".into();
                            }
                            Err(error) => app.service.error = Some(error),
                        }
                        app.archive_confirm = None;
                    }
                    if ui.button("取消").clicked() {
                        app.archive_confirm = None;
                    }
                });
            });
    }
}

fn installation_options(ui: &mut Ui, app: &mut NsclApp) {
    use crate::minecraft::LoaderKind;
    let Some(version) = app.download.version.clone() else {
        return;
    };
    app.service.ensure_versions_loaded(&app.settings, ui.ctx());
    let mut back = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 18.0;
        back = download_button(ui, "返回版本列表", !app.service.busy());
        app::heading(ui, &format!("Minecraft {}", version.id));
    });
    if back {
        app.download = Default::default();
        return;
    }
    ui.add_space(14.0);
    ui.add_enabled_ui(!app.service.busy(), |ui| {
        theme::toolbar(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(12.0, 10.0);
            for kind in std::iter::once(None).chain(LoaderKind::ALL.map(Some)) {
                let label = kind.map(LoaderKind::label).unwrap_or("不安装加载器");
                if installation_choice(ui, label, app.download.kind == kind) {
                    app.download.choose_kind(kind);
                    app.service.loader_search.clear();
                }
            }
        });
    });
    ui.add_space(10.0);
    if let Some(kind) = app.download.kind {
        app::hint(
            ui,
            "选择一个兼容的加载器版本，然后开始安装；每次只安装一种加载器。",
        );
        ui.add_space(10.0);
        ui.add_enabled_ui(!app.service.busy(), |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 18.0;
                let response = ui.add(
                    egui::TextEdit::singleline(&mut app.service.loader_search)
                        .font(FontId::proportional(14.0))
                        .frame(input_frame())
                        .margin(Vec2::new(14.0, 10.0))
                        .hint_text("搜索加载器版本")
                        .desired_width(280.0),
                );
                input_focus(ui, &response);
                if download_button(ui, "刷新列表", true) {
                    app.download.loader = None;
                    app.service
                        .load_loaders(&version.id, kind, &app.settings, ui.ctx());
                }
            });
        });
        app.service
            .ensure_loaders_loaded(&version.id, kind, &app.settings, ui.ctx());
        // A refreshed or newly requested catalog must not retain an obsolete choice.
        if app
            .download
            .loader
            .as_ref()
            .is_some_and(|loader| !app.service.loaders.contains(loader))
        {
            app.download.loader = None;
        }
    } else {
        app::hint(
            ui,
            "直接安装 Minecraft 原版客户端，之后可在开始页启动游戏。",
        );
    }
    ui.add_space(12.0);
    let querying = app.service.task == crate::service::Task::LoaderVersions;
    let status_height = if querying {
        0.0
    } else {
        download_status_height(app)
    };
    let height = (ui.available_height() - status_height - 88.0).max(0.0);
    ui.allocate_ui_with_layout(
        Vec2::new(ui.available_width(), height),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_min_height(height);
            if let Some(kind) = app.download.kind {
                loader_version_list(ui, app, &version.id, kind, height);
            } else if height >= 46.0 {
                ui.add_space(((height - 46.0) * 0.5).max(0.0));
                ui.vertical_centered(|ui| {
                    app::body(ui, "原版 Minecraft");
                    ui.add_space(8.0);
                    app::hint(ui, "无需选择加载器，点击下方按钮即可安装。");
                });
            }
        },
    );
    installation_footer(ui, app);
    if !querying {
        task_status(ui, app);
    }
}

fn installation_choice(ui: &mut Ui, label: &str, selected: bool) -> bool {
    app::styled_button(
        ui,
        RichText::new(label).size(14.0).color(if selected {
            theme::ON_ACCENT
        } else {
            theme::BODY_TEXT
        }),
        &app::ButtonStyle {
            fill: if selected {
                theme::ACCENT
            } else {
                theme::SECONDARY_BG
            },
            min_size: Vec2::new(108.0, 38.0),
            ..Default::default()
        },
    )
    .clicked()
}

fn loader_version_list(
    ui: &mut Ui,
    app: &mut NsclApp,
    minecraft: &str,
    kind: crate::minecraft::LoaderKind,
    height: f32,
) {
    let query = app.service.loader_search.trim().to_ascii_lowercase();
    let versions: Vec<_> = app
        .service
        .loaders
        .iter()
        .filter(|version| {
            version.minecraft == minecraft
                && version.kind == kind
                && version.version.to_ascii_lowercase().contains(&query)
        })
        .cloned()
        .collect();
    if versions.is_empty() {
        ui.add_space(((height - 84.0) * 0.5).max(0.0));
        ui.vertical_centered(|ui| {
            if app.service.busy() {
                if app.service.task == crate::service::Task::LoaderVersions {
                    ui.spinner();
                    app::body(ui, "正在获取兼容的加载器版本……");
                    ui.add_space(8.0);
                    if download_button(ui, "取消加载", true) {
                        app.service.cancel();
                    }
                }
            } else if !query.is_empty() {
                app::body(ui, "没有匹配的加载器版本，请调整搜索词。");
            } else {
                app::body(
                    ui,
                    "暂无兼容版本，或列表获取失败；可刷新重试或选择不安装加载器。",
                );
            }
        });
        return;
    }
    ui.spacing_mut().item_spacing.y = 10.0;
    egui::ScrollArea::vertical()
        .id_salt(("loader-versions", minecraft, kind.key()))
        .max_height(height)
        .min_scrolled_height(0.0)
        .auto_shrink([false, false])
        .show_rows(ui, 72.0, versions.len(), |ui, range| {
            for index in range {
                let version = &versions[index];
                let installed = version
                    .id()
                    .is_ok_and(|id| app.service.installed.iter().any(|entry| entry.id == id));
                let selected = app.download.loader.as_ref() == Some(version);
                let row = crate::minecraft::VersionEntry {
                    id: format!("{} {}", version.kind.label(), version.version),
                    kind: if version.stable {
                        "稳定版"
                    } else {
                        "预览版"
                    }
                    .into(),
                    release_time: format!("适用于 Minecraft {}", version.minecraft),
                    url: String::new(),
                    sha1: String::new(),
                };
                if ui
                    .push_id(&row.id, |ui| {
                        version_row(
                            ui,
                            &row,
                            installed,
                            selected,
                            !app.service.busy(),
                            if selected { "已选择" } else { "选择" },
                        )
                    })
                    .inner
                {
                    app.download.choose_loader(version.clone());
                }
            }
        });
}

fn installation_footer(ui: &mut Ui, app: &mut NsclApp) {
    use crate::download_flow::InstallTarget;
    let target = app.download.target();
    let installed = target
        .as_ref()
        .and_then(InstallTarget::id)
        .is_some_and(|id| app.service.installed.iter().any(|version| version.id == id));
    let summary = match &target {
        Some(InstallTarget::Vanilla(version)) => format!("Minecraft {} · 原版", version.id),
        Some(InstallTarget::Modded(loader)) => loader.label(),
        None => "请选择一个加载器版本".into(),
    };
    ui.add_space(18.0);
    ui.separator();
    ui.add_space(12.0);
    let mut install = false;
    ui.horizontal(|ui| {
        let summary_width = (ui.available_width() - 220.0).max(0.0);
        ui.add_sized(
            Vec2::new(summary_width, 46.0),
            egui::Label::new(RichText::new(summary).size(14.0).color(theme::BODY_TEXT)).truncate(),
        );
        ui.add_enabled_ui(
            target.is_some() && !app.service.busy() && app.service.game.is_none(),
            |ui| {
                install = app::primary_button(
                    ui,
                    if installed {
                        "校验 / 修复"
                    } else {
                        "开始安装"
                    },
                );
            },
        );
    });
    if install && let Some(target) = target {
        match target {
            InstallTarget::Vanilla(version) => {
                app.service.install(version, &app.settings, ui.ctx())
            }
            InstallTarget::Modded(loader) => {
                app.service.install_modded(loader, &app.settings, ui.ctx())
            }
        }
    }
}

pub(crate) fn download_status_height(app: &NsclApp) -> f32 {
    if app.service.busy() {
        88.0
    } else if !app.service.notice.is_empty() {
        52.0
    } else {
        0.0
    }
}

fn minecraft_downloads(ui: &mut Ui, app: &mut NsclApp) {
    app.service.ensure_versions_loaded(&app.settings, ui.ctx());
    let search_width = (ui.available_width() * 0.42).clamp(240.0, 400.0);
    let refresh = ui
        .horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 20.0;
            let response = ui.add(
                egui::TextEdit::singleline(&mut app.service.search)
                    .font(FontId::proportional(14.0))
                    .frame(input_frame())
                    .margin(Vec2::new(14.0, 10.0))
                    .hint_text("搜索版本，例如 1.20.1")
                    .desired_width(search_width),
            );
            input_focus(ui, &response);
            ui.checkbox(&mut app.service.snapshots, "显示快照与历史版本");
            download_button(ui, "刷新列表", !app.service.busy())
        })
        .inner;
    if refresh {
        app.service.load_versions(&app.settings, ui.ctx());
    }
    ui.add_space(20.0);
    let status_height = download_status_height(app);
    let list_height = (ui.available_height() - status_height).max(80.0);
    ui.allocate_ui_with_layout(
        Vec2::new(ui.available_width(), list_height),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_min_height(list_height);
            minecraft_version_list(ui, app, list_height);
        },
    );
    task_status(ui, app);
}

fn minecraft_version_list(ui: &mut Ui, app: &mut NsclApp, height: f32) {
    if app.service.versions.is_empty() {
        ui.add_space(((height - 84.0) * 0.5).max(0.0));
        ui.vertical_centered(|ui| {
            if app.service.busy() {
                app::body(ui, "正在获取版本列表……");
            } else {
                app::body(ui, "暂无可用版本，请重试或在设置中更换下载源。");
                ui.add_space(16.0);
                if download_button(ui, "重新加载", true) {
                    app.service.load_versions(&app.settings, ui.ctx());
                }
            }
        });
        return;
    }
    let query = app.service.search.trim().to_ascii_lowercase();
    let versions: Vec<_> = app
        .service
        .versions
        .iter()
        .filter(|version| {
            (app.service.snapshots || version.kind == "release")
                && version.id.to_ascii_lowercase().contains(&query)
        })
        .cloned()
        .collect();
    let mut selection = None;
    if versions.is_empty() {
        ui.add_space(((height - 24.0) * 0.5).max(0.0));
        ui.vertical_centered(|ui| {
            app::body(ui, "没有匹配的版本，请调整搜索词或启用快照显示。");
        });
        return;
    }
    ui.spacing_mut().item_spacing.y = 10.0;
    egui::ScrollArea::vertical()
        .id_salt("vanilla-versions")
        .max_height(height)
        .min_scrolled_height(0.0)
        .auto_shrink([false, false])
        .show_rows(ui, 72.0, versions.len(), |ui, range| {
            for index in range {
                let version = &versions[index];
                let installed = app
                    .service
                    .installed
                    .iter()
                    .any(|entry| entry.id == version.id);
                let clicked = ui.push_id(&version.id, |ui| {
                    version_row(
                        ui,
                        version,
                        installed,
                        false,
                        !app.service.busy(),
                        "选择版本",
                    )
                });
                if clicked.inner {
                    selection = Some(version.clone());
                }
            }
        });
    if let Some(version) = selection {
        app.download.choose_version(version);
        app.service.loader_search.clear();
    }
}

fn version_row(
    ui: &mut Ui,
    version: &crate::minecraft::VersionEntry,
    installed: bool,
    selected: bool,
    enabled: bool,
    action_label: &str,
) -> bool {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 72.0), Sense::hover());
    ui.painter().rect_filled(
        rect,
        10.0,
        if ui.rect_contains_pointer(rect) {
            theme::SECONDARY_BG
        } else {
            theme::TOPBAR_BG
        },
    );
    ui.painter().rect_stroke(
        rect,
        10.0,
        Stroke::new(
            1.0,
            if selected {
                theme::ACCENT
            } else {
                theme::CARD_BORDER
            },
        ),
        egui::StrokeKind::Inside,
    );
    let info_rect = Rect::from_min_max(
        rect.min + Vec2::new(18.0, 12.0),
        Pos2::new(rect.right() - 132.0, rect.bottom() - 12.0),
    );
    let mut info = ui.new_child(UiBuilder::new().max_rect(info_rect));
    info.spacing_mut().item_spacing = Vec2::new(14.0, 5.0);
    info.spacing_mut().interact_size.y = 24.0;
    info.horizontal(|ui| {
        ui.add(
            egui::Label::new(
                RichText::new(&version.id)
                    .size(16.0)
                    .family(theme::bold_family())
                    .color(theme::HEADING),
            )
            .truncate(),
        );
        let kind = match version.kind.as_str() {
            "release" => "正式版",
            "snapshot" => "快照",
            "old_beta" => "历史 Beta",
            "old_alpha" => "历史 Alpha",
            other => other,
        };
        ui.label(RichText::new(kind).size(11.0).color(theme::MUTED));
        if installed {
            ui.label(RichText::new("已安装").size(11.0).color(theme::ACCENT));
        }
    });
    info.label(
        RichText::new(if version.kind == "稳定版" || version.kind == "预览版" {
            &version.release_time
        } else {
            version.release_time.get(..10).unwrap_or("")
        })
        .size(12.0)
        .color(theme::MUTED),
    );
    let mut action = ui.new_child(
        UiBuilder::new()
            .max_rect(Rect::from_min_size(
                Pos2::new(
                    rect.right() - 114.0,
                    rect.center().y - theme::CONTROL_HEIGHT / 2.0,
                ),
                Vec2::new(96.0, theme::CONTROL_HEIGHT),
            ))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    download_button(&mut action, action_label, enabled)
}

pub(crate) fn download_button(ui: &mut Ui, label: &str, enabled: bool) -> bool {
    ui.add_enabled_ui(enabled, |ui| {
        app::styled_button(
            ui,
            RichText::new(label).size(13.0).color(theme::BODY_TEXT),
            &app::ButtonStyle {
                fill: theme::SECONDARY_BG,
                hover: theme::HOVER,
                stroke: Some(Stroke::new(1.0, theme::SECONDARY_BORDER)),
                min_size: Vec2::new(96.0, theme::CONTROL_HEIGHT),
                ..Default::default()
            },
        )
    })
    .inner
    .clicked()
}

pub(crate) fn task_status(ui: &mut Ui, app: &mut NsclApp) {
    if app.service.busy() {
        ui.add_space(16.0);
        ui.separator();
        ui.add_space(12.0);
        let progress = &app.service.progress;
        let detail = if let Some(total) = progress.total_bytes {
            format!(
                "{:.1} / {:.1} MiB",
                progress.bytes_downloaded as f64 / 1048576.0,
                total as f64 / 1048576.0
            )
        } else if progress.total > 0 {
            format!(
                "{} / {} 个文件 · {:.1} MiB",
                progress.completed,
                progress.total,
                progress.bytes_downloaded as f64 / 1048576.0
            )
        } else {
            "正在处理中……".into()
        };
        ui.horizontal(|ui| {
            let info_width = (ui.available_width() - 116.0).max(0.0);
            ui.allocate_ui_with_layout(
                Vec2::new(info_width, theme::CONTROL_HEIGHT),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(info_width);
                    ui.add(
                        egui::Label::new(
                            RichText::new(&progress.stage)
                                .size(13.0)
                                .color(theme::BODY_TEXT),
                        )
                        .truncate(),
                    );
                    ui.add_space(4.0);
                    app::hint(ui, &detail);
                },
            );
            if matches!(
                app.service.task,
                crate::service::Task::Install
                    | crate::service::Task::Launch
                    | crate::service::Task::Manifest
                    | crate::service::Task::LoaderVersions
                    | crate::service::Task::Content
            ) && download_button(ui, "取消任务", true)
            {
                app.service.cancel();
            }
        });
        ui.add_space(8.0);
        if progress.total > 0 || progress.total_bytes.is_some() {
            let fraction = progress
                .total_bytes
                .map(|total| progress.bytes_downloaded as f32 / total.max(1) as f32)
                .unwrap_or(progress.completed as f32 / progress.total.max(1) as f32);
            ui.add(
                egui::ProgressBar::new(fraction.clamp(0.0, 1.0))
                    .desired_height(4.0)
                    .desired_width(ui.available_width()),
            );
        } else {
            ui.add(
                egui::ProgressBar::new(0.0)
                    .animate(true)
                    .desired_height(4.0),
            );
        }
    } else if !app.service.notice.is_empty() {
        ui.add_space(16.0);
        ui.separator();
        ui.add_space(12.0);
        ui.add(
            egui::Label::new(
                RichText::new(&app.service.notice)
                    .size(13.0)
                    .color(theme::ACCENT),
            )
            .truncate(),
        );
    }
}

pub(crate) fn input_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(theme::INPUT_BG)
        .stroke(Stroke::new(1.0, theme::CARD_BORDER))
        .corner_radius(10.0)
        .inner_margin(egui::Margin::symmetric(14, 12))
}

pub(crate) fn input_focus(ui: &Ui, response: &egui::Response) {
    if response.has_focus() {
        ui.painter().rect_stroke(
            response.rect.expand(2.0),
            8.0,
            Stroke::new(2.0, theme::ACCENT),
            egui::StrokeKind::Outside,
        );
    }
}

pub fn about(ui: &mut Ui, app: &mut NsclApp) {
    page_card(ui, |ui| {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(56.0), Sense::hover());
            ui.painter().rect_filled(rect, 16.0, theme::nav_active_bg());
            theme::star4(ui.painter(), rect.center(), 20.0, theme::ACCENT);
            ui.add_space(8.0);
            ui.vertical(|ui| {
                app::heading(ui, "NorthStar Minecraft Launcher");
                ui.add_space(8.0);
                app::body(
                    ui,
                    &format!(
                        "NSCL v{} · Minecraft Java Edition",
                        env!("CARGO_PKG_VERSION")
                    ),
                );
            });
        });
        ui.add_space(28.0);
        ui.separator();
        ui.add_space(22.0);
        ui.label(
            RichText::new("启动器更新")
                .size(16.0)
                .family(theme::bold_family()),
        );
        ui.add_space(10.0);
        let before = app.settings.check_updates;
        ui.checkbox(&mut app.settings.check_updates, "启动时检查更新");
        if before != app.settings.check_updates {
            let (mut saved, _) = crate::store::load();
            saved.check_updates = app.settings.check_updates;
            if let Err(error) = crate::store::save(&saved) {
                app.updates.status = error;
            }
        }
        ui.add_space(theme::CONTROL_GAP);
        theme::toolbar(ui, |ui| {
            if ui
                .add_enabled(!app.updates.busy, egui::Button::new("检查更新"))
                .clicked()
            {
                app.updates.check(ui.ctx());
            }
            if ui.button("打开 GitHub Releases ↗").clicked()
                && let Err(error) = nscl::browser::open(nscl::updater::RELEASES_URL)
            {
                app.updates.status = error;
            }
            if ui
                .add_enabled(
                    !app.updates.busy && app.updates.available() && app.updates.ready.is_none(),
                    egui::Button::new("下载更新"),
                )
                .clicked()
            {
                app.updates.download(ui.ctx());
            }
            if app.updates.downloading && ui.button("取消更新下载").clicked() {
                app.updates.cancel();
            }
            if let Some(payload) = &app.updates.ready {
                let can_restart = !app.updates.busy
                    && !app.service.busy()
                    && app.service.game.is_none()
                    && !app.accounts.pending
                    && !app.skin.pending;
                if ui
                    .add_enabled(can_restart, egui::Button::new("重启并更新"))
                    .clicked()
                {
                    match nscl::updater::restart_with_update(payload) {
                        Ok(()) => {
                            app.updates.busy = true;
                            app.updates.status = "正在重启并更新……".into();
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        Err(error) => app.updates.status = error,
                    }
                }
            }
        });
        ui.add_space(theme::CONTROL_GAP);
        app::hint(ui, &app.updates.status);
        if let Some(release) = &app.updates.release {
            egui::CollapsingHeader::new(format!("{} 更新说明", release.tag_name)).show(ui, |ui| {
                ui.label(
                    release
                        .body
                        .as_deref()
                        .unwrap_or("此版本未填写更新说明。")
                        .chars()
                        .take(8000)
                        .collect::<String>(),
                );
            });
        }
        ui.add_space(20.0);
        egui::CollapsingHeader::new("功能概览").show(ui, |ui| {
            app::body(ui, "原版与 Forge / Fabric / NeoForge · Microsoft 正版与离线账号 · Java 与内存自动配置 · 独立实例 · Modrinth 模组与整合包 · 皮肤上传与 3D 预览。");
        });
        ui.add_space(18.0);
        app::heading(ui, "设计参考与致谢");
        ui.add_space(10.0);
        app::body(
            ui,
            "感谢以下启动器在安装流程、资源分类与实例管理方面提供的设计参考。",
        );
        ui.add_space(10.0);
        theme::toolbar(ui, |ui| {
            for (name, url) in [
                (
                    "HMCL · Hello Minecraft! Launcher",
                    "https://github.com/HMCL-dev/HMCL",
                ),
                (
                    "PCL · Plain Craft Launcher",
                    "https://github.com/Meloong-Git/PCL",
                ),
                (
                    "Axolotl Launcher",
                    "https://github.com/Axolotl-Launcher/Axolotl",
                ),
            ] {
                if ui.link(name).clicked()
                    && let Err(error) = nscl::browser::open(url)
                {
                    app.service.error = Some(error);
                }
            }
        });
        ui.add_space(18.0);
        app::hint(
            ui,
            "本项目与 Mojang Studios 或 Microsoft 无隶属或背书关系。",
        );
    });
}
