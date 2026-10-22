use crate::app::{self, NsclApp, Page};
use crate::theme;
use egui::{Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, Ui, UiBuilder, Vec2};

pub fn home(ui: &mut Ui, app: &mut NsclApp) {
    app::hint(
        ui,
        "拖入 .mrpack 或 Modrinth 整合包 ZIP，自动识别并安装到独立实例；可选客户端文件默认不安装。",
    );
    ui.add_space(10.0);
    if app.service.busy() {
        task_status(ui, app);
        ui.add_space(10.0);
    }
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
            &format!(
                "{} · {}",
                if app
                    .service
                    .instances
                    .iter()
                    .find(|i| i.id == app.service.selected)
                    .is_some_and(|i| i.isolation == crate::instances::Isolation::Shared)
                {
                    "共享游戏目录"
                } else {
                    "版本隔离"
                },
                if app.settings.online_mode {
                    "Microsoft 正版身份"
                } else {
                    "离线账号"
                }
            ),
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
                app.category = 0;
                app.download = Default::default();
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
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut app.account_tab, 0, "账号管理");
            ui.selectable_value(&mut app.account_tab, 1, "皮肤与预览");
        });
        ui.add_space(20.0);
        if app.account_tab == 1 {
            crate::skin_ui::controls(ui, app);
            return;
        }
        app::eyebrow(ui, "ACCOUNT & SKIN");
        ui.add_space(12.0);
        app::heading(ui, "让冒险拥有你的名字");
        ui.add_space(10.0);
        app::body(ui, "登录 Microsoft 正版账号，或创建独立的离线账号。");
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
            ui.add_space(16.0);
            let mut create = false;
            ui.horizontal_wrapped(|ui| {
                let name = ui.add(
                    egui::TextEdit::singleline(&mut app.offline_name)
                        .hint_text("离线账号玩家名")
                        .desired_width(260.0),
                );
                create = ui.button("创建离线账号").clicked()
                    || (name.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
            });
            app::hint(
                ui,
                "玩家名为 1–16 位英文字母、数字或下划线。离线账号无法进入要求正版认证的服务器。",
            );
            if create {
                match app
                    .accounts
                    .add_offline(&app.offline_name, &mut app.settings)
                {
                    Ok(()) => app.offline_name.clear(),
                    Err(error) => app.service.error = Some(error),
                }
            }
        });
        ui.add_space(12.0);
        app::hint(
            ui,
            "密码只在 Microsoft 官方网页输入；启动器将刷新令牌保存在系统凭据库。切换到「皮肤与预览」可管理角色皮肤。",
        );
        if app.accounts.pending {
            ui.add_space(20.0);
            if let Some(prompt) = app.accounts.browser_prompt.clone() {
                egui::Frame::NONE
                    .fill(theme::TOPBAR_BG)
                    .corner_radius(10.0)
                    .inner_margin(20)
                    .show(ui, |ui| {
                        app::body(ui, "在 Microsoft 官方网页完成登录与授权，然后返回启动器。");
                        ui.add_space(12.0);
                        ui.horizontal_wrapped(|ui| {
                            if ui
                                .add_enabled(
                                    !app.accounts.browser_opening,
                                    egui::Button::new("重新打开浏览器 ↗"),
                                )
                                .clicked()
                            {
                                app.accounts.open_browser(ui.ctx());
                            }
                            if ui.button("复制登录链接").clicked() {
                                app.accounts.copy_login_link(ui.ctx());
                            }
                        });
                        ui.add_space(10.0);
                        app::hint(
                            ui,
                            "浏览器未弹出时，可复制链接到浏览器地址栏。请保持启动器打开。",
                        );
                        app::hint(
                            ui,
                            "若 Edge 提示现有实例以管理员权限运行，请保存网页后按提示以普通权限重启 Edge。",
                        );
                        ui.add_space(10.0);
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
            if let Some(prompt) = app.accounts.prompt.clone() {
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
                        ui.horizontal_wrapped(|ui| {
                            if ui
                                .add_enabled(
                                    !app.accounts.browser_opening,
                                    egui::Button::new("打开 Microsoft 授权页面 ↗"),
                                )
                                .clicked()
                            {
                                app.accounts.open_browser(ui.ctx());
                            }
                            if ui.button("复制登录链接").clicked() {
                                app.accounts.copy_login_link(ui.ctx());
                            }
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
                        app::badge_muted(
                            ui,
                            if account.is_online() {
                                "Microsoft"
                            } else {
                                "离线账号"
                            },
                        );
                        if app.accounts.list.selected.as_deref() == Some(&account.id) {
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
                    app::hint(
                        ui,
                        &format!(
                            "UUID：{} · {}",
                            account.id,
                            if account.is_online() {
                                "启动时在线验证身份"
                            } else {
                                "使用此玩家名和固定离线 UUID 启动"
                            }
                        ),
                    );
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
        app::hint(
            ui,
            "通过「使用此账号」切换启动身份；正版身份验证失败时不会自动切换离线账号。",
        );
    });
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
        if app.category != 0 && app.service.content.plan.is_some() {
            crate::content_page::plan(ui, app);
            return;
        }
        ui.add_enabled_ui(!app.service.busy(), |ui| {
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
    page_card(ui, |ui| {
        if app.service.busy() {
            app::hint(ui, "后台任务进行中，请等待或取消后再修改设置。");
            task_status(ui, app);
            return;
        }
        app::eyebrow(ui, "PREFERENCES");
        ui.add_space(12.0);
        app::heading(ui, "为你的冒险做好准备");
        ui.add_space(10.0);
        app::body(ui, "配置游戏下载源、独立实例目录及 Java 启动环境。");
        ui.add_space(30.0);
        ui.scope(|ui| {
            let width = ui.available_width().min(580.0);
            ui.set_max_width(width);
            ui.label(RichText::new("Java 可执行文件路径").size(14.0));
            ui.add_space(10.0);
            ui.checkbox(&mut app.settings.auto_java, "按游戏版本自动选择 Java");
            ui.add_space(10.0);
            let edit = egui::TextEdit::singleline(&mut app.settings.java_path)
                .font(FontId::proportional(14.0))
                .hint_text("例如 C:\\Program Files\\Java\\bin\\javaw.exe")
                .frame(input_frame())
                .margin(Vec2::new(14.0, 12.0))
                .desired_width(width);
            let response = ui.add_enabled(!app.settings.auto_java, edit);
            input_focus(ui, &response);
            if response.changed() {
                app.save_status.clear();
            }
            ui.add_space(10.0);
            app::hint(
                ui,
                "默认按实例要求自动选择已安装的 Java 主版本和架构；关闭自动选择后可手动指定。",
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
                    app.settings.auto_java = true;
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
                                app.settings.auto_java = false;
                            }
                        }
                    });
            }
            ui.add_space(14.0);
            ui.horizontal_wrapped(|ui| {
                ui.label("自动安装 Java");
                egui::ComboBox::from_id_salt("install-java-major")
                    .width(110.0)
                    .selected_text(format!("Java {}",app.service.java_install_major))
                    .show_ui(ui,|ui| {for major in [8,11,17,21,25] {ui.selectable_value(&mut app.service.java_install_major,major,format!("Java {major}"));}});
                if ui.add_enabled(!app.service.java_busy,egui::Button::new("下载并安装")).clicked() {app.service.install_java(ui.ctx());}
                let required=app.service.instances.iter().find(|i|i.id==app.service.selected).and_then(|i|app.service.installed.iter().find(|v|v.id==i.version_id)).map(|v|v.java_major);
                if ui.add_enabled(required.is_some(),egui::Button::new("按当前实例选择")).clicked()
                    && let Some(major)=required
                {app.service.java_install_major=major;}
            });
            app::hint(ui,"下载 Eclipse Temurin JRE，校验后安装到启动器数据目录，启用自动选择后即可匹配。支持 Windows。");
            if !app.service.java_status.is_empty() {
                ui.add_space(10.0);
                app::body(ui, &app.service.java_status);
            }
            ui.add_space(24.0);
            ui.label(RichText::new("游戏内存（MiB）").size(14.0));
            ui.add_space(10.0);
            ui.checkbox(&mut app.settings.auto_memory, "自动分配游戏内存");
            ui.add_space(10.0);
            let response = ui.add_enabled(!app.settings.auto_memory,
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
            if app.settings.auto_memory {
                let version = app.service.instances.iter().find(|i| i.id == app.service.selected)
                    .and_then(|i| app.service.installed.iter().find(|v| v.id == i.version_id));
                let (major, modded) = version.map(|v| (v.java_major, v.loader.is_some())).unwrap_or((17, false));
                match nscl::memory::automatic(major, modded) {
                    Ok(memory) => app::hint(ui, &format!("当前实例预计分配 {memory} MiB，启动时按可用物理内存重新计算并为系统保留空间。")),
                    Err(error) => app::hint(ui, &error),
                }
            } else {
                app::hint(ui, "范围 1024–32768 MiB；实例的单独配置优先于此处设置。");
            }
            ui.add_space(24.0);
            ui.label("Minecraft 游戏目录");
            ui.add_space(10.0);
            egui::ComboBox::from_id_salt("game-directories").width(width)
                .selected_text(if app.settings.game_directory.trim().is_empty() { "默认：启动器所在目录" } else { &app.settings.game_directory })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut app.settings.game_directory, String::new(), "默认：启动器所在目录");
                    for path in app.settings.game_directories.clone() {
                        ui.selectable_value(&mut app.settings.game_directory, path.clone(), &path);
                    }
                });
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                ui.add(egui::TextEdit::singleline(&mut app.new_game_directory).hint_text("添加 MC 路径，例如 D:\\Minecraft 或已有 .minecraft 目录").desired_width((width - 100.0).max(180.0)));
                if ui.button("添加路径").clicked() {
                    match crate::store::add_game_directory(&mut app.settings, &app.new_game_directory) {
                        Ok(()) => { app.new_game_directory.clear(); app.save_status = "目录已添加并选中，保存设置后应用。".into(); }
                        Err(error) => app.save_status = error,
                    }
                }
            });
            app::hint(ui, "默认使用启动器所在目录；目录中已有 .minecraft 文件夹时优先使用它。可直接添加 .minecraft 路径，切换不会移动或删除已有文件。");
            app::hint(
                ui,
                &format!(
                    "当前目录：{}",
                    crate::store::game_directory(&app.settings).display()
                ),
            );
            ui.add_space(20.0);
            ui.label("新实例默认隔离模式");
            egui::ComboBox::from_id_salt("default-isolation")
                .selected_text(app.settings.default_isolation.label())
                .show_ui(ui, |ui| {
                    for mode in crate::instances::Isolation::ALL {
                        ui.selectable_value(&mut app.settings.default_isolation, mode, mode.label());
                    }
                });
            app::hint(ui, "仅用于新安装版本与新建实例。隔离模式分别保存存档、模组、配置和截图；共享模式共用当前游戏根目录。已有实例可在「管理实例 → 配置」修改，整合包始终隔离。");
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
                    "例如 https://example.com；需支持 /mc/game、/v1、/maven、/assets，Fabric 还需 /fabric-meta。",
                );
            }
            ui.add_space(10.0);
            ui.checkbox(
                &mut app.settings.source.fallback_to_official,
                "镜像失败或校验不通过时回退到官方源",
            );
            ui.add_space(26.0);
            if app::primary_button(ui, "保存设置") {
                let memory = app.memory_input.trim().parse::<u32>().ok().filter(|m| (1024..=32768).contains(m));
                if app.settings.auto_memory || memory.is_some() {
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
                    if let Some(memory) = memory { app.settings.memory = memory; }
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
                    "可为每个实例选择独立或共享游戏目录；客户端、资源和依赖文件始终复用。",
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
                        app.category = 0;
                        app.download = Default::default();
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
                                            match instance.game_directory(&root).and_then(|path| {
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
                                            "{} · Java：{} · 内存：{}",
                                            instance.isolation.label(),
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
                            match crate::instances::create_with_isolation(
                                &root,
                                &app.new_instance_version,
                                &app.new_instance_name,
                                app.settings.default_isolation,
                            ) {
                                Ok(instance) => {
                                    app.service.refresh(&app.settings);
                                    app.service.selected = instance.id;
                                    app.remember_instance();
                                    app.new_instance_name.clear();
                                    app.instance_status =
                                        "实例已创建，可在配置中查看隔离模式与游戏目录。".into();
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
            .max_height((ctx.content_rect().height() - 120.0).max(220.0))
            .vscroll(true)
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
                    ui.label("版本隔离");
                    ui.add_enabled_ui(!instance.requires_isolation(), |ui| {
                        egui::ComboBox::from_id_salt("instance-isolation")
                            .selected_text(instance.isolation.label())
                            .show_ui(ui, |ui| {
                                for mode in crate::instances::Isolation::ALL {
                                    ui.selectable_value(&mut instance.isolation, mode, mode.label());
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
                ui.label(if instance.isolation == crate::instances::Isolation::Shared {
                    "只归档该实例配置及保留的独立目录；共享游戏目录的存档、模组和配置留在原处，供其他版本继续使用。"
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
        ui.horizontal_wrapped(|ui| {
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
                Pos2::new(rect.right() - 114.0, rect.center().y - 18.0),
                Vec2::new(96.0, 36.0),
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
                min_size: Vec2::new(96.0, 36.0),
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
                crate::service::Task::Install
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
        .fill(Color32::from_rgb(0x0c, 0x16, 0x22))
        .stroke(Stroke::new(1.0, Color32::from_rgb(0x2b, 0x39, 0x47)))
        .corner_radius(7.0)
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
        let (rect, _) = ui.allocate_exact_size(Vec2::new(52.0, 50.0), Sense::hover());
        theme::star4(ui.painter(), rect.center(), 23.0, theme::ACCENT);
        ui.add_space(10.0);
        app::heading(ui, "NorthStar Minecraft Launcher");
        ui.add_space(10.0);
        app::body(ui, &format!("NSCL · v{}", env!("CARGO_PKG_VERSION")));
        ui.add_space(16.0);
        ui.label("启动器更新 · GitHub Releases");
        let before = app.settings.check_updates;
        ui.checkbox(&mut app.settings.check_updates, "启动时检查更新");
        if before != app.settings.check_updates {
            let (mut saved, _) = crate::store::load();
            saved.check_updates = app.settings.check_updates;
            if let Err(error) = crate::store::save(&saved) {
                app.updates.status = error;
            }
        }
        ui.horizontal_wrapped(|ui| {
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
            "支持原版与 Forge / Fabric / NeoForge 安装启动、Microsoft 正版登录、Java 自动安装、多账号与独立实例、Modrinth 模组与整合包、模组更新及卸载，以及皮肤上传和可旋转的 3D 角色预览。",
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
