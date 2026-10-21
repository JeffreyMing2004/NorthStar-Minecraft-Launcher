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
        if app.service.installed.is_empty() {
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
    if app.service.installed.is_empty() {
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
        egui::ComboBox::from_id_salt("installed-version")
            .width(selection.available_width())
            .selected_text(&app.service.selected)
            .show_ui(&mut selection, |ui| {
                for version in &app.service.installed {
                    ui.selectable_value(
                        &mut app.service.selected,
                        version.id.clone(),
                        format!("Minecraft {} · Java {}", version.id, version.java_major),
                    );
                }
            });
        app::hint(&mut selection, "原版 · 独立游戏目录 · 离线测试启动");
    }
    launch.add_space(20.0);
    if app.service.installed.is_empty() {
        if app::primary_button(&mut launch, "＋  安装 Minecraft     →") {
            app.page = Page::Downloads;
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
                    app.service.launch(&app.settings, ui.ctx());
                }
            });
            if ui.button("安装其他版本").clicked() {
                app.page = Page::Downloads;
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
            "设置中可选择 Java、游戏目录和下载镜像。"
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
            app::badge_muted(ui, "未登录")
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
        RichText::new("添加你的 Minecraft 账号")
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

pub fn account(ui: &mut Ui) {
    page_card(ui, |ui| {
        app::eyebrow(ui, "ACCOUNT & SKIN");
        ui.add_space(12.0);
        app::heading(ui, "让冒险拥有你的名字");
        ui.add_space(10.0);
        app::body(
            ui,
            "Microsoft 账号登录、皮肤上传与 3D 预览将在后续版本提供。",
        );
        empty_state(
            ui,
            "◎",
            "还没有添加账号",
            "认证服务尚未接入，当前不会收集账号或密码。",
            Some("Microsoft 登录 · 开发中"),
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
    page_card(ui, |ui| {
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
                        crate::minecraft::validate_player_name(&app.settings.player_name)
                    {
                        app.save_status = error;
                        return;
                    }
                    app.settings.memory = memory;
                    app.settings.java_path = app.settings.java_path.trim().to_owned();
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
    task_status(ui, app);
    if app.service.busy() || !app.service.notice.is_empty() {
        ui.add_space(16.0);
    }
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut app.service.search)
                .hint_text("搜索版本，例如 1.20.1")
                .desired_width(240.0),
        );
        ui.checkbox(&mut app.service.snapshots, "显示快照及历史测试版");
    });
    ui.add_space(12.0);
    if app.service.versions.is_empty() {
        if app.service.busy() {
            app::body(ui, "正在获取版本列表……");
        } else {
            app::body(ui, "暂无可用版本，请重试或在设置中更换下载源。");
            ui.add_space(12.0);
            if app::secondary_button(ui, "重新加载", true) {
                app.service.load_versions(&app.settings, ui.ctx());
            }
        }
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
        app::body(ui, "没有匹配的版本，请调整搜索词或启用快照显示。");
        return;
    }
    egui::ScrollArea::vertical()
        .id_salt("vanilla-versions")
        .max_height(350.0)
        .show_rows(ui, 44.0, versions.len(), |ui, range| {
            for index in range {
                let version = &versions[index];
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&version.id).size(15.0).color(theme::HEADING));
                    ui.label(
                        RichText::new(format!(
                            "{} · {}",
                            version.kind,
                            version.release_time.get(..10).unwrap_or("")
                        ))
                        .size(11.0)
                        .color(theme::MUTED),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let installed = app
                            .service
                            .installed
                            .iter()
                            .any(|entry| entry.id == version.id);
                        ui.add_enabled_ui(!app.service.busy(), |ui| {
                            if app::secondary_button(
                                ui,
                                if installed {
                                    "校验 / 修复"
                                } else {
                                    "安装"
                                },
                                true,
                            ) {
                                install = Some(version.clone());
                            }
                        });
                    });
                });
            }
        });
    if let Some(version) = install {
        app.service.install(version, &app.settings, ui.ctx());
    }
}

fn task_status(ui: &mut Ui, app: &mut NsclApp) {
    if app.service.busy() {
        ui.add_space(16.0);
        app::body(ui, &app.service.progress.stage);
        let progress = &app.service.progress;
        if progress.total > 0 {
            ui.add(
                egui::ProgressBar::new(progress.completed as f32 / progress.total as f32)
                    .text(format!(
                        "{} / {} 个文件 · {:.1} MiB",
                        progress.completed,
                        progress.total,
                        progress.bytes_downloaded as f64 / 1048576.0
                    ))
                    .desired_width(ui.available_width()),
            );
        } else {
            ui.spinner();
        }
        if matches!(
            app.service.task,
            crate::service::Task::Install | crate::service::Task::Manifest
        ) && ui.button("取消任务").clicked()
        {
            app.service.cancel();
        }
    } else if !app.service.notice.is_empty() {
        ui.add_space(12.0);
        ui.label(RichText::new(&app.service.notice).color(theme::ACCENT));
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
            "已提供原版版本列表、镜像切换、文件校验与修复、独立实例安装、Java 检测和离线测试启动。账号认证、模组下载及皮肤渲染尚未实现。",
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
