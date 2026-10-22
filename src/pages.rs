use crate::app::{self, NsclApp, Page};
use crate::theme;
use egui::{Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, Ui, UiBuilder, Vec2};

pub fn home(ui: &mut Ui, app: &mut NsclApp) {
    let width = ui.available_width();
    let wide = width >= 800.0;
    let height = ui.available_height().clamp(480.0, 554.0);
    let total_height = if wide { height } else { height * 2.0 + 22.0 };
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, total_height), Sense::hover());
    let left_width = if wide {
        (width - 22.0) * 1.85 / 2.85
    } else {
        width
    };
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
    let banner_height = if height >= 530.0 { 190.0 } else { 160.0 };
    let banner = Rect::from_min_size(launch_rect.min, Vec2::new(left_width, banner_height));
    landscape(ui.painter(), banner);
    let content = Rect::from_min_max(
        launch_rect.min + Vec2::new(30.0, banner_height + 27.0),
        launch_rect.max - Vec2::new(30.0, 20.0),
    );
    let mut launch = ui.new_child(UiBuilder::new().id_salt("launch-content").max_rect(content));
    launch.spacing_mut().item_spacing.y = 0.0;
    app::eyebrow(&mut launch, "你的游戏空间");
    launch.add_space(8.0);
    app::heading(&mut launch, "准备好探索新世界了吗？");
    launch.add_space(10.0);
    app::body(
        &mut launch,
        if app.service.instances.is_empty() {
            "安装第一个 Minecraft 实例，让冒险拥有自己的起点。"
        } else {
            "选择已安装版本，开启你的下一段旅程。"
        },
    );
    launch.add_space(20.0);
    let (instance, _) =
        launch.allocate_exact_size(Vec2::new(content.width(), 84.0), Sense::hover());
    let p = launch.painter();
    p.rect_filled(instance, 9.0, theme::dashed_bg());
    dashed_rect(p, instance, theme::DASHED_BORDER);
    diamond(
        p,
        Pos2::new(instance.left() + 27.0, instance.center().y),
        6.0,
        Color32::from_rgb(0x7e, 0xa4, 0xba),
    );
    if app.service.instances.is_empty() {
        p.text(
            instance.min + Vec2::new(50.0, 28.0),
            egui::Align2::LEFT_CENTER,
            "暂无游戏实例",
            FontId::new(14.0, theme::bold_family()),
            theme::HEADING,
        );
        p.text(
            instance.min + Vec2::new(50.0, 55.0),
            egui::Align2::LEFT_CENTER,
            "原版 · Forge · Fabric · NeoForge",
            FontId::proportional(12.0),
            theme::MUTED,
        );
    } else {
        let mut selection =
            launch.new_child(UiBuilder::new().max_rect(instance.shrink2(Vec2::new(50.0, 14.0))));
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
        app::hint(
            &mut selection,
            if app.settings.online_mode {
                "独立游戏目录 · Microsoft 正版身份"
            } else {
                "独立游戏目录 · 离线测试身份"
            },
        );
    }
    launch.add_space(20.0);
    if app.service.instances.is_empty() {
        if app::primary_button(
            &mut launch,
            if app.service.installed.is_empty() {
                "＋  安装 Minecraft     →"
            } else {
                "＋  创建游戏实例     →"
            },
        ) {
            if app.service.installed.is_empty() {
                app.page = Page::Downloads;
            } else {
                app.instance_manager = true;
            }
        }
    } else {
        launch.horizontal(|ui| {
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
                app.instance_manager = true;
            }
            if app.service.last_log.is_some() && ui.button("查看日志").clicked() {
                app.open_log();
            }
        });
    }
    launch.add_space(10.0);
    app::hint(
        &mut launch,
        if app.service.busy() {
            &app.service.progress.stage
        } else if !app.service.notice.is_empty() {
            &app.service.notice
        } else {
            "账号页可登录正版账号，设置中可配置 Java 与下载镜像。"
        },
    );

    let content = aside_rect.shrink(25.0);
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
                    "离线测试"
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
    diamond(
        aside.painter(),
        center - Vec2::new(0.0, 18.0),
        17.0,
        Color32::from_rgb(0x54, 0x7b, 0x88),
    );
    aside.painter().text(
        center + Vec2::new(0.0, 56.0),
        egui::Align2::CENTER_CENTER,
        "你的角色，即将登场",
        FontId::new(13.0, theme::bold_family()),
        Color32::from_rgb(0xa6, 0xbb, 0xc9),
    );
    aside.painter().text(
        center + Vec2::new(0.0, 84.0),
        egui::Align2::CENTER_CENTER,
        "3D 皮肤预览尚未接入",
        FontId::proportional(11.0),
        theme::MUTED,
    );
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
                .unwrap_or("添加你的 Minecraft 账号"),
        )
        .size(16.0)
        .family(theme::bold_family())
        .color(theme::HEADING),
    );
    account.add_space(10.0);
    account.label(
        RichText::new("在这里管理身份、切换账号和修改皮肤。")
            .size(12.0)
            .color(theme::MUTED),
    );
    account.add_space(20.0);
    if app::styled_button(
        &mut account,
        RichText::new("管理账号 →").size(13.0),
        &app::ButtonStyle {
            fill: theme::SECONDARY_BG,
            stroke: Some(Stroke::new(1.0, theme::SECONDARY_BORDER)),
            min_size: Vec2::new(footer.width(), 42.0),
            ..Default::default()
        },
    )
    .clicked()
    {
        app.page = Page::Account;
    }
}

fn draw_card(ui: &Ui, rect: Rect) {
    ui.painter().rect_filled(rect, 16.0, theme::CARD_BG);
    ui.painter().rect_stroke(
        rect,
        16.0,
        Stroke::new(1.0, theme::CARD_BORDER),
        egui::StrokeKind::Inside,
    );
}

fn diamond(p: &egui::Painter, center: Pos2, radius: f32, color: Color32) {
    p.add(egui::Shape::closed_line(
        vec![
            center + Vec2::new(0.0, -radius),
            center + Vec2::new(radius, 0.0),
            center + Vec2::new(0.0, radius),
            center + Vec2::new(-radius, 0.0),
        ],
        Stroke::new(if radius > 10.0 { 3.0 } else { 1.0 }, color),
    ));
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

fn dashed_rect(p: &egui::Painter, rect: Rect, color: Color32) {
    let rect = rect.shrink(1.0);
    for y in [rect.top(), rect.bottom()] {
        let mut x = rect.left() + 9.0;
        while x < rect.right() - 9.0 {
            let end = (x + 5.0).min(rect.right() - 9.0);
            p.line_segment(
                [Pos2::new(x, y), Pos2::new(end, y)],
                Stroke::new(1.0, color),
            );
            x = end + 4.0;
        }
    }
    for x in [rect.left(), rect.right()] {
        let mut y = rect.top() + 9.0;
        while y < rect.bottom() - 9.0 {
            let end = (y + 5.0).min(rect.bottom() - 9.0);
            p.line_segment(
                [Pos2::new(x, y), Pos2::new(x, end)],
                Stroke::new(1.0, color),
            );
            y = end + 4.0;
        }
    }
}

fn landscape(p: &egui::Painter, rect: Rect) {
    let mut mesh = egui::Mesh::default();
    // Curved upper corners, with a smooth diagonal sky gradient.
    for row in 0..=32 {
        let y = rect.top() + rect.height() * row as f32 / 32.0;
        let dy = (16.0 - (y - rect.top())).max(0.0);
        let inset = if dy > 0.0 {
            16.0 - (256.0 - dy * dy).sqrt()
        } else {
            0.0
        };
        for col in 0..=32 {
            let t = col as f32 / 32.0;
            let k = (t * 0.6 + row as f32 / 32.0 * 0.4).clamp(0.0, 1.0);
            let mix = |a: f32, b: f32| (a + (b - a) * k) as u8;
            mesh.colored_vertex(
                Pos2::new(egui::lerp(rect.left() + inset..=rect.right() - inset, t), y),
                Color32::from_rgb(mix(20.0, 33.0), mix(45.0, 74.0), mix(64.0, 85.0)),
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
        Color32::from_rgb(0x3c, 0x77, 0x7a),
        0.4,
    );
    mountain(p, rect, theme::MOUNTAIN_BACK, 1.2, 0.0, 19.0);
    mountain(p, rect, theme::MOUNTAIN_FRONT, 1.3, -0.12, 81.0);
    let star = rect.min + Vec2::new(rect.width() * 0.745, 44.0);
    theme::star4(p, star, 12.0, theme::STAR);
    theme::spaced_text(
        p,
        rect.min + Vec2::new(30.0, rect.height() - 38.0),
        "FIND YOUR NORTH",
        11.0,
        5.0,
        Color32::from_rgb(0xb0, 0xdb, 0xdf),
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
    page_card(ui, |ui| {
        app::eyebrow(ui, "ACCOUNT & SKIN");
        ui.add_space(12.0);
        app::heading(ui, "让冒险拥有你的名字");
        ui.add_space(10.0);
        app::body(
            ui,
            "使用 Microsoft 账号授权，验证 Minecraft Java Edition 身份。",
        );
        ui.add_space(24.0);
        let editable = !app.accounts.pending && !app.service.busy() && app.service.game.is_none();
        ui.add_enabled_ui(editable, |ui| {
            if app::primary_button(ui, "＋  登录 Microsoft 账号") {
                app.accounts.login(
                    &app.settings.microsoft_client_id,
                    &app.settings.microsoft_redirect_uri,
                    ui.ctx(),
                );
            }
            egui::CollapsingHeader::new("其他登录方式").show(ui, |ui| {
                if ui.button("设备代码登录（需应用启用对应流程）").clicked() {
                    app.accounts
                        .login_device(&app.settings.microsoft_client_id, ui.ctx());
                }
            });
        });
        ui.add_space(12.0);
        app::hint(
            ui,
            "密码只在 Microsoft 官方网页输入；启动器将刷新令牌保存在系统凭据库。皮肤管理与 3D 预览仍在开发中。",
        );
        if app.accounts.pending {
            ui.add_space(20.0);
            if let Some(prompt) = &app.accounts.browser_prompt {
                egui::Frame::NONE
                    .fill(theme::TOPBAR_BG)
                    .corner_radius(10.0)
                    .inner_margin(20)
                    .show(ui, |ui| {
                        app::body(ui, "在 Microsoft 官方网页完成登录与授权，然后返回启动器。");
                        ui.add_space(12.0);
                        ui.hyperlink_to("打开 Microsoft 登录页面 ↗", &prompt.url);
                        ui.add_space(10.0);
                        app::hint(ui, &format!("当前本地回调：{}", prompt.redirect_uri));
                        let remaining = prompt
                            .expires
                            .saturating_duration_since(std::time::Instant::now())
                            .as_secs();
                        app::hint(
                            ui,
                            &format!("剩余等待时间 {} 分 {} 秒", remaining / 60, remaining % 60),
                        );
                        ui.ctx()
                            .request_repaint_after(std::time::Duration::from_secs(1));
                    });
            }
            if let Some(prompt) = &app.accounts.prompt {
                egui::Frame::NONE
                    .fill(theme::TOPBAR_BG)
                    .corner_radius(10.0)
                    .inner_margin(20)
                    .show(ui, |ui| {
                        app::body(ui, "在 Microsoft 页面输入以下代码：");
                        ui.add_space(10.0);
                        ui.label(
                            RichText::new(&prompt.code)
                                .size(28.0)
                                .monospace()
                                .color(theme::ACCENT),
                        );
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            ui.hyperlink_to("打开 Microsoft 授权页面 ↗", &prompt.url);
                            if ui.button("复制代码").clicked() {
                                ui.ctx().copy_text(prompt.code.clone());
                            }
                        });
                        let remaining = prompt
                            .expires
                            .saturating_duration_since(std::time::Instant::now())
                            .as_secs();
                        app::hint(
                            ui,
                            &format!("代码剩余有效期 {} 分 {} 秒", remaining / 60, remaining % 60),
                        );
                        ui.ctx()
                            .request_repaint_after(std::time::Duration::from_secs(1));
                    });
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.spinner();
                app::body(ui, &app.accounts.status);
            });
            ui.add_space(10.0);
            if ui.button("取消登录").clicked() {
                app.accounts.cancel();
            }
        } else if !app.accounts.status.is_empty() {
            ui.add_space(16.0);
            app::body(ui, &app.accounts.status);
        }
        ui.add_space(28.0);
        let mut select = None;
        let mut remove = None;
        for account in app.accounts.list.accounts.clone() {
            egui::Frame::NONE
                .fill(theme::TOPBAR_BG)
                .corner_radius(10.0)
                .inner_margin(18)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(&account.name)
                                .size(17.0)
                                .family(theme::bold_family()),
                        );
                        if app.settings.online_mode
                            && app.accounts.list.selected.as_deref() == Some(&account.id)
                        {
                            app::badge_muted(ui, "当前账号");
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if download_button(ui, "移除", editable) {
                                remove = Some(account.id.clone());
                            }
                            if download_button(ui, "使用此账号", editable) {
                                select = Some(account.id.clone());
                            }
                        });
                    });
                    ui.add_space(10.0);
                    app::hint(ui, &format!("UUID：{} · 启动时在线验证身份", account.id));
                });
            ui.add_space(12.0);
        }
        if let Some(id) = select
            && let Err(error) = app.accounts.select(&id, &mut app.settings)
        {
            app.service.error = Some(error);
        }
        if let Some(id) = remove
            && let Err(error) = app.accounts.remove(&id, &mut app.settings)
        {
            app.service.error = Some(error);
        }
        ui.add_space(16.0);
        ui.add_enabled_ui(editable, |ui| {
            let mut offline = !app.settings.online_mode;
            if ui.checkbox(&mut offline, "使用离线测试身份").changed() {
                if !offline && app.accounts.list.current().is_none() {
                    app.service.error = Some("请先登录 Microsoft 账号。".into());
                } else {
                    app.settings.online_mode = !offline;
                    if let Err(error) = app.accounts.save_mode(&app.settings) {
                        app.service.error = Some(error);
                    }
                }
            }
        });
        app::hint(
            ui,
            "离线测试玩家名和试玩选项可在设置中配置；正版启动验证失败时不会自动切换离线身份。",
        );
    });
}

pub fn downloads(ui: &mut Ui, app: &mut NsclApp) {
    const CATEGORIES: [&str; 4] = ["原版", "加载器", "整合包", "Mod"];
    const DESCRIPTIONS: [&str; 3] = [
        "为你的实例安装 Forge、Fabric 或 NeoForge。",
        "发现组合好的模组与全新玩法。",
        "为现有实例添加更多可能。",
    ];
    let width = ui.available_width();
    let height = ui.available_height();
    theme::card(24).show(ui, |ui| {
        ui.set_width(width - 50.0);
        ui.set_min_height(height - 50.0);
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            for (index, item) in CATEGORIES.iter().enumerate() {
                let selected = app.category == index;
                if app::styled_button(
                    ui,
                    RichText::new(*item).size(14.0).color(if selected {
                        theme::ON_ACCENT
                    } else {
                        theme::MUTED
                    }),
                    &app::ButtonStyle {
                        fill: if selected {
                            theme::ACCENT
                        } else {
                            Color32::TRANSPARENT
                        },
                        hover: if selected {
                            theme::ACCENT_HOVER
                        } else {
                            theme::HOVER
                        },
                        active: if selected {
                            theme::ACCENT_HOVER
                        } else {
                            theme::HOVER
                        },
                        min_size: Vec2::new(68.0, 38.0),
                        ..Default::default()
                    },
                )
                .clicked()
                {
                    app.category = index;
                }
            }
        });
        ui.add_space(18.0);
        ui.separator();
        if app.category == 0 {
            ui.add_space(20.0);
            vanilla_downloads(ui, app);
            return;
        }
        egui::ScrollArea::vertical()
            .id_salt("download-placeholder")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(28.0);
                app::heading(ui, CATEGORIES[app.category]);
                ui.add_space(10.0);
                app::body(ui, DESCRIPTIONS[app.category - 1]);
                if app.category == 1 {
                    ui.add_space(20.0);
                    ui.horizontal(|ui| {
                        for loader in ["Forge", "Fabric", "NeoForge"] {
                            egui::Frame::NONE
                                .stroke(Stroke::new(1.0, theme::CARD_BORDER))
                                .corner_radius(7.0)
                                .inner_margin(egui::Margin::symmetric(18, 9))
                                .show(ui, |ui| {
                                    ui.label(RichText::new(loader).size(13.0));
                                });
                        }
                    });
                }
                empty_state(
                    ui,
                    "↓",
                    &format!("{}下载服务尚未接入", CATEGORIES[app.category]),
                    "接入资源服务后，这里将显示可用版本和安装进度。",
                    None,
                );
                ui.vertical_centered(|ui| app::badge_muted(ui, "暂无下载任务"));
            });
    });
}

pub fn settings(ui: &mut Ui, app: &mut NsclApp) {
    page_card(ui, |ui| {
        if app.service.busy() {
            app::hint(ui, "后台任务进行中，请等待或取消后再修改设置。");
            ui.disable();
        }
        app::eyebrow(ui, "PREFERENCES");
        ui.add_space(12.0);
        app::heading(ui, "为你的冒险做好准备");
        ui.add_space(10.0);
        app::body(ui, "配置原版游戏下载、独立实例目录及 Java 启动环境。");
        ui.add_space(30.0);
        ui.scope(|ui| {
            let width = ui.available_width().min(580.0);
            ui.set_max_width(width);
            ui.label(RichText::new("Java 可执行文件路径").size(14.0));
            ui.add_space(10.0);
            let edit = egui::TextEdit::singleline(&mut app.settings.java_path)
                .font(FontId::proportional(14.0))
                .hint_text("例如 C:\\Program Files\\Java\\bin\\javaw.exe")
                .frame(input_frame())
                .margin(Vec2::new(14.0, 12.0))
                .desired_width(width);
            let response = ui.add(edit);
            input_focus(ui, &response);
            if response.changed() {
                app.save_status.clear();
            }
            ui.add_space(10.0);
            app::hint(
                ui,
                "留空自动检测 JAVA_HOME、PATH 及常见安装目录，选择版本要求的 Java。",
            );
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if download_button(ui, "扫描 Java", !app.service.java_busy) {
                    app.service.inspect_java(None, ui.ctx());
                }
                if download_button(
                    ui,
                    "检测当前路径",
                    !app.service.java_busy && !app.settings.java_path.trim().is_empty(),
                ) {
                    app.service
                        .inspect_java(Some(app.settings.java_path.clone()), ui.ctx());
                }
                if ui.button("恢复自动匹配").clicked() {
                    app.settings.java_path.clear();
                }
            });
            if !app.service.java_runtimes.is_empty() {
                ui.add_space(12.0);
                egui::ComboBox::from_id_salt("detected-java")
                    .width(width)
                    .selected_text("从检测到的 Java 中选择……")
                    .show_ui(ui, |ui| {
                        for runtime in &app.service.java_runtimes {
                            if ui
                                .selectable_label(
                                    false,
                                    format!("{}\n{}", runtime.label(), runtime.path.display()),
                                )
                                .clicked()
                            {
                                app.settings.java_path =
                                    runtime.path.to_string_lossy().into_owned();
                            }
                        }
                    });
            }
            if !app.service.java_status.is_empty() {
                ui.add_space(10.0);
                app::body(ui, &app.service.java_status);
            }
            ui.add_space(24.0);
            ui.label(RichText::new("游戏内存（MiB）").size(14.0));
            ui.add_space(10.0);
            let response = ui.add(
                egui::TextEdit::singleline(&mut app.memory_input)
                    .font(FontId::proportional(14.0))
                    .frame(input_frame())
                    .margin(Vec2::new(14.0, 12.0))
                    .desired_width(width),
            );
            input_focus(ui, &response);
            if response.changed() {
                app.save_status.clear();
            }
            ui.add_space(10.0);
            app::hint(ui, "范围 1024–32768 MiB，请根据设备可用内存配置。");
            ui.add_space(24.0);
            settings_input(ui, "游戏文件目录", &mut app.settings.game_directory, width);
            app::hint(
                ui,
                &format!(
                    "当前目录：{}",
                    crate::store::game_directory(&app.settings).display()
                ),
            );
            ui.add_space(24.0);
            ui.label("下载源");
            ui.add_space(10.0);
            egui::ComboBox::from_id_salt("download-source")
                .width(width)
                .selected_text(app.settings.source.kind.label())
                .show_ui(ui, |ui| {
                    for kind in crate::minecraft::SourceKind::ALL {
                        ui.selectable_value(&mut app.settings.source.kind, kind, kind.label());
                    }
                });
            if app.settings.source.kind == crate::minecraft::SourceKind::Custom {
                ui.add_space(12.0);
                settings_input(
                    ui,
                    "自定义镜像根地址（BMCLAPI 兼容）",
                    &mut app.settings.source.custom_base,
                    width,
                );
                app::hint(
                    ui,
                    "例如 https://example.com；需支持 /mc/game、/v1、/maven 和 /assets 路径。",
                );
            }
            ui.add_space(10.0);
            ui.checkbox(
                &mut app.settings.source.fallback_to_official,
                "镜像失败或校验不通过时回退到官方源",
            );
            ui.add_space(24.0);
            settings_input(
                ui,
                "Microsoft 应用 Client ID",
                &mut app.settings.microsoft_client_id,
                width,
            );
            app::hint(
                ui,
                "NSCL 的公共应用标识；应用需支持个人账号，回调平台选择移动和桌面应用程序。",
            );
            ui.add_space(24.0);
            settings_input(ui, "Microsoft 登录回调 URI", &mut app.settings.microsoft_redirect_uri, width);
            app::hint(ui, "与应用注册一致；默认 http://localhost:29111/auth-response，端口占用时尝试已登记的 29112–29115。浏览器登录使用 PKCE，无需 Client Secret。");
            ui.add_space(24.0);
            settings_input(ui, "离线测试玩家名", &mut app.settings.player_name, width);
            app::hint(
                ui,
                "1–16 位字母、数字或下划线；离线身份不能用于需要正版认证的服务器。",
            );
            ui.add_space(10.0);
            ui.checkbox(&mut app.settings.demo_mode, "以 Minecraft 试玩模式启动");
            ui.add_space(26.0);
            if app::primary_button(ui, "保存设置") {
                if let Ok(memory) = app.memory_input.trim().parse::<u32>()
                    && (1024..=32768).contains(&memory)
                {
                    if let Err(error) = app.settings.source.base() {
                        app.save_status = error;
                        return;
                    }
                    if let Err(error) =
                        crate::accounts::validate_client_id(app.settings.microsoft_client_id.trim())
                    {
                        app.save_status = error;
                        return;
                    }
                    if let Err(error) = crate::accounts::validate_redirect_uri(app.settings.microsoft_redirect_uri.trim()) { app.save_status = error; return; }
                    if let Err(error) =
                        crate::minecraft::validate_player_name(&app.settings.player_name)
                    {
                        app.save_status = error;
                        return;
                    }
                    app.settings.memory = memory;
                    app.settings.java_path = app.settings.java_path.trim().to_owned();
                    app.settings.microsoft_client_id =
                        app.settings.microsoft_client_id.trim().to_owned();
                    app.settings.microsoft_redirect_uri = app.settings.microsoft_redirect_uri.trim().to_owned();
                    app.save_status = match crate::store::save(&app.settings) {
                        Ok(()) => {
                            app.service.refresh(&app.settings);
                            app.service.invalidate_versions();
                            app.service.notice.clear();
                            "设置已保存，新任务将使用当前下载源和游戏目录。".to_owned()
                        }
                        Err(e) => e,
                    };
                } else {
                    app.save_status = "内存必须为 1024–32768 MiB 之间的整数。".to_owned();
                }
            }
            if !app.save_status.is_empty() {
                ui.add_space(12.0);
                app::body(ui, &app.save_status);
            }
        });
    });
}

pub fn instance_windows(ctx: &egui::Context, app: &mut NsclApp) {
    let root = crate::store::game_directory(&app.settings);
    let editable = !app.service.busy() && app.service.game.is_none();
    if app.instance_manager {
        let mut open = true;
        egui::Window::new("管理游戏实例")
            .open(&mut open)
            .default_width(760.0)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 12.0;
                app::body(
                    ui,
                    "每个实例拥有独立存档与配置，同一版本共享客户端、资源和依赖文件。",
                );
                if !editable {
                    app::hint(ui, "游戏或任务运行中，实例配置暂时锁定。");
                }
                ui.horizontal(|ui| {
                    if ui.button("刷新列表").clicked() {
                        app.service.refresh(&app.settings);
                    }
                    if ui.button("安装其他版本").clicked() {
                        app.page = Page::Downloads;
                        app.instance_manager = false;
                    }
                });
                egui::ScrollArea::vertical()
                    .id_salt("instance-manager")
                    .max_height(330.0)
                    .show(ui, |ui| {
                        for instance in app.service.instances.clone() {
                            egui::Frame::NONE
                                .fill(theme::TOPBAR_BG)
                                .corner_radius(8.0)
                                .inner_margin(14)
                                .show(ui, |ui| {
                                    ui.set_min_width(660.0_f32.min(ui.available_width()));
                                    ui.horizontal_wrapped(|ui| {
                                        if ui
                                            .selectable_label(
                                                app.service.selected == instance.id,
                                                &instance.name,
                                            )
                                            .clicked()
                                            && editable
                                        {
                                            app.service.selected = instance.id.clone();
                                            app.remember_instance();
                                        }
                                        ui.label(
                                            RichText::new(format!("MC {}", instance.version_id))
                                                .color(theme::MUTED),
                                        );
                                        ui.add_enabled_ui(editable, |ui| {
                                            if ui.button("配置").clicked() {
                                                app.instance_memory = instance
                                                    .memory
                                                    .map(|m| m.to_string())
                                                    .unwrap_or_else(|| {
                                                        app.settings.memory.to_string()
                                                    });
                                                app.instance_edit = Some(instance.clone());
                                            }
                                            if ui.button("归档").clicked() {
                                                app.archive_confirm = Some(instance.clone());
                                            }
                                        });
                                        if ui.button("打开目录").clicked() {
                                            match instance.directory(&root).and_then(|path| {
                                                std::fs::create_dir_all(&path)
                                                    .map_err(|e| e.to_string())?;
                                                open::that_detached(path).map_err(|e| e.to_string())
                                            }) {
                                                Ok(()) => {}
                                                Err(error) => app.service.error = Some(error),
                                            }
                                        }
                                    });
                                    app::hint(
                                        ui,
                                        &format!(
                                            "Java：{} · 内存：{}",
                                            instance
                                                .java_path
                                                .as_deref()
                                                .map(|p| if p.is_empty() {
                                                    "自动匹配"
                                                } else {
                                                    p
                                                })
                                                .unwrap_or("跟随全局"),
                                            instance
                                                .memory
                                                .map(|m| format!("{m} MiB"))
                                                .unwrap_or_else(|| "跟随全局".into())
                                        ),
                                    );
                                });
                        }
                        if app.service.instances.is_empty() {
                            app::body(ui, "暂无实例，请先安装客户端，或使用已安装版本创建实例。");
                        }
                        egui::CollapsingHeader::new("已归档实例").show(ui, |ui| {
                            for (entry, instance) in crate::instances::archived(&root) {
                                ui.horizontal(|ui| {
                                    ui.label(format!(
                                        "{} · {}",
                                        instance.name, instance.version_id
                                    ));
                                    if ui
                                        .add_enabled(editable, egui::Button::new("恢复"))
                                        .clicked()
                                    {
                                        match crate::instances::restore(&root, &entry) {
                                            Ok(instance) => {
                                                app.service.refresh(&app.settings);
                                                app.service.selected = instance.id;
                                                app.remember_instance();
                                                app.instance_status = "实例已恢复。".into();
                                            }
                                            Err(error) => app.service.error = Some(error),
                                        }
                                    }
                                });
                            }
                        });
                    });
                ui.separator();
                ui.add_enabled_ui(editable && !app.service.installed.is_empty(), |ui| {
                    app::heading(ui, "创建空白实例");
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut app.new_instance_name)
                                .hint_text("实例名称，例如 生存世界")
                                .desired_width(240.0),
                        );
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
                        if ui.button("创建").clicked() {
                            match crate::instances::create(
                                &root,
                                &app.new_instance_version,
                                &app.new_instance_name,
                            ) {
                                Ok(instance) => {
                                    app.service.refresh(&app.settings);
                                    app.service.selected = instance.id;
                                    app.remember_instance();
                                    app.new_instance_name.clear();
                                    app.instance_status = "已创建独立实例，可在开始页启动。".into();
                                }
                                Err(error) => app.service.error = Some(error),
                            }
                        }
                    });
                });
                if !app.instance_status.is_empty() {
                    app::body(ui, &app.instance_status);
                }
            });
        app.instance_manager &= open;
    }
    if let Some(mut instance) = app.instance_edit.take() {
        let mut open = true;
        let mut saved = false;
        egui::Window::new("实例配置")
            .open(&mut open)
            .default_width(530.0)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 12.0;
                ui.add_enabled_ui(editable, |ui| {
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
                ui.label("存档、模组和配置会一起保留，可在管理实例的归档列表中恢复。");
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(editable, egui::Button::new("确认归档"))
                        .clicked()
                    {
                        match crate::instances::archive(&root, &instance) {
                            Ok(_) => {
                                app.service.refresh(&app.settings);
                                app.remember_instance();
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

fn settings_input(ui: &mut Ui, label: &str, value: &mut String, width: f32) {
    ui.label(label);
    ui.add_space(10.0);
    let response = ui.add(
        egui::TextEdit::singleline(value)
            .font(FontId::proportional(14.0))
            .frame(input_frame())
            .desired_width(width),
    );
    input_focus(ui, &response);
    ui.add_space(10.0);
}

fn vanilla_downloads(ui: &mut Ui, app: &mut NsclApp) {
    app.service.ensure_versions_loaded(&app.settings, ui.ctx());
    let search_width = (ui.available_width() * 0.42).clamp(240.0, 400.0);
    ui.horizontal(|ui| {
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
    });
    ui.add_space(20.0);
    let status_height = if app.service.busy() {
        88.0
    } else if !app.service.notice.is_empty() {
        52.0
    } else {
        0.0
    };
    let list_height = (ui.available_height() - status_height).max(80.0);
    ui.allocate_ui_with_layout(
        Vec2::new(ui.available_width(), list_height),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_min_height(list_height);
            vanilla_version_list(ui, app, list_height);
        },
    );
    task_status(ui, app);
}

fn vanilla_version_list(ui: &mut Ui, app: &mut NsclApp, height: f32) {
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
    let mut install = None;
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
                    version_row(ui, version, installed, !app.service.busy())
                });
                if clicked.inner {
                    install = Some(version.clone());
                }
            }
        });
    if let Some(version) = install {
        app.service.install(version, &app.settings, ui.ctx());
    }
}

fn version_row(
    ui: &mut Ui,
    version: &crate::minecraft::VersionEntry,
    installed: bool,
    enabled: bool,
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
        Stroke::new(1.0, theme::CARD_BORDER),
        egui::StrokeKind::Inside,
    );
    let info_rect = Rect::from_min_max(
        rect.min + Vec2::new(18.0, 12.0),
        Pos2::new(rect.right() - 132.0, rect.bottom() - 12.0),
    );
    let mut info = ui.new_child(UiBuilder::new().max_rect(info_rect));
    info.spacing_mut().item_spacing = Vec2::new(14.0, 5.0);
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
        RichText::new(version.release_time.get(..10).unwrap_or(""))
            .size(12.0)
            .color(theme::MUTED),
    );
    let mut action = ui.new_child(
        UiBuilder::new()
            .max_rect(Rect::from_min_size(
                Pos2::new(rect.right() - 114.0, rect.center().y - 18.0),
                Vec2::new(96.0, 36.0),
            ))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    download_button(
        &mut action,
        if installed {
            "校验 / 修复"
        } else {
            "安装"
        },
        enabled,
    )
}

fn download_button(ui: &mut Ui, label: &str, enabled: bool) -> bool {
    ui.add_enabled_ui(enabled, |ui| {
        app::styled_button(
            ui,
            RichText::new(label).size(13.0).color(theme::BODY_TEXT),
            &app::ButtonStyle {
                fill: theme::SECONDARY_BG,
                hover: theme::HOVER,
                stroke: Some(Stroke::new(1.0, theme::SECONDARY_BORDER)),
                min_size: Vec2::new(96.0, 36.0),
                ..Default::default()
            },
        )
    })
    .inner
    .clicked()
}

fn task_status(ui: &mut Ui, app: &mut NsclApp) {
    if app.service.busy() {
        ui.add_space(16.0);
        ui.separator();
        ui.add_space(12.0);
        let progress = &app.service.progress;
        let detail = if progress.total > 0 {
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
                Vec2::new(info_width, 36.0),
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
                crate::service::Task::Install | crate::service::Task::Manifest
            ) && download_button(ui, "取消任务", true)
            {
                app.service.cancel();
            }
        });
        ui.add_space(8.0);
        if progress.total > 0 {
            ui.add(
                egui::ProgressBar::new(progress.completed as f32 / progress.total as f32)
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

fn input_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(Color32::from_rgb(0x0c, 0x16, 0x22))
        .stroke(Stroke::new(1.0, Color32::from_rgb(0x2b, 0x39, 0x47)))
        .corner_radius(7.0)
        .inner_margin(egui::Margin::symmetric(14, 12))
}

fn input_focus(ui: &Ui, response: &egui::Response) {
    if response.has_focus() {
        ui.painter().rect_stroke(
            response.rect.expand(2.0),
            8.0,
            Stroke::new(2.0, theme::ACCENT),
            egui::StrokeKind::Outside,
        );
    }
}

pub fn about(ui: &mut Ui) {
    page_card(ui, |ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(52.0, 50.0), Sense::hover());
        theme::star4(ui.painter(), rect.center(), 23.0, theme::ACCENT);
        ui.add_space(10.0);
        app::heading(ui, "NorthStar Minecraft Launcher");
        ui.add_space(10.0);
        app::body(
            ui,
            &format!("NSCL · v{} · 开发预览", env!("CARGO_PKG_VERSION")),
        );
        ui.add_space(10.0);
        app::body(ui, "一个以 Rust 为核心的 Minecraft Java Edition 启动器。");
        ui.add_space(30.0);
        ui.set_max_width(690.0);
        ui.label(
            RichText::new("本版本")
                .size(16.0)
                .family(theme::bold_family()),
        );
        ui.add_space(10.0);
        app::body(
            ui,
            "已提供原版安装启动、Microsoft 正版登录与续期、多账号切换、Java 检测和实例独立配置及归档恢复。模组下载、皮肤修改与渲染尚未实现。",
        );
        ui.add_space(22.0);
        ui.label(
            RichText::new("项目声明")
                .size(16.0)
                .family(theme::bold_family()),
        );
        ui.add_space(10.0);
        app::body(
            ui,
            "本项目与 Mojang Studios 或 Microsoft 无隶属或背书关系。",
        );
    });
}

fn empty_state(ui: &mut Ui, glyph: &str, title: &str, description: &str, action: Option<&str>) {
    ui.add_space(40.0);
    ui.vertical_centered(|ui| {
        ui.label(
            RichText::new(glyph)
                .size(42.0)
                .color(Color32::from_rgb(0x71, 0xb9, 0xbc)),
        );
        ui.add_space(18.0);
        ui.label(RichText::new(title).size(16.0).family(theme::bold_family()));
        ui.add_space(10.0);
        app::body(ui, description);
        if let Some(label) = action {
            ui.add_space(20.0);
            app::secondary_button(ui, label, false);
        }
    });
    ui.add_space(18.0);
}
