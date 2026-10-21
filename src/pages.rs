use crate::app::{self, NsclApp, Page};
use crate::theme;
use egui::{Color32, RichText, Ui, Vec2};

pub fn home(ui: &mut Ui, app: &mut NsclApp) {
    let wide = ui.available_width() >= 980.0;
    let aside = |ui: &mut Ui, app: &mut NsclApp| {
        theme::card(25).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("我的角色")
                        .size(16.0)
                        .strong()
                        .color(theme::HEADING),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    app::badge_muted(ui, "未登录")
                });
            });
            ui.add_space(14.0);
            let (rect, _) = ui
                .allocate_exact_size(Vec2::new(ui.available_width(), 265.0), egui::Sense::hover());
            let painter = ui.painter().with_clip_rect(rect);
            radial_glow(&painter, rect.center(), 135.0, (0x35, 0x59, 0x66), 0.30);
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "◇",
                egui::FontId::proportional(64.0),
                Color32::from_rgb(0x54, 0x7b, 0x88),
            );
            ui.vertical_centered(|ui| {
                ui.add_space(rect.height() * 0.30);
                ui.label(
                    RichText::new("你的角色，即将登场")
                        .size(13.0)
                        .color(Color32::from_rgb(0xa6, 0xbb, 0xc9))
                        .strong(),
                );
                ui.label(
                    RichText::new("3D 皮肤预览尚未接入")
                        .size(11.0)
                        .color(theme::MUTED),
                );
            });
            ui.add_space(18.0);
            ui.label(
                RichText::new("添加你的 Minecraft 账号")
                    .size(16.0)
                    .strong()
                    .color(theme::HEADING),
            );
            ui.label(
                RichText::new("在这里管理身份、切换账号和修改皮肤。")
                    .size(12.0)
                    .color(theme::MUTED),
            );
            ui.add_space(12.0);
            if app::secondary_button(ui, "管理账号 →", true) {
                app.page = Page::Account;
            }
        });
    };

    let launch = |ui: &mut Ui, app: &mut NsclApp| {
        theme::card(0).show(ui, |ui| {
            landscape(ui);
            egui::Frame::default()
                .inner_margin(egui::Margin {
                    left: 30,
                    right: 30,
                    top: 27,
                    bottom: 27,
                })
                .show(ui, |ui| {
                    app::eyebrow(ui, "你的游戏空间");
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("准备好探索新世界了吗？")
                            .size(21.0)
                            .strong()
                            .color(theme::HEADING),
                    );
                    ui.label(
                        RichText::new("安装第一个 Minecraft 实例，让冒险拥有自己的起点。")
                            .size(14.0)
                            .color(theme::MUTED),
                    );
                    ui.add_space(25.0);
                    let (rect, _) = ui.allocate_exact_size(
                        Vec2::new(ui.available_width(), 64.0),
                        egui::Sense::hover(),
                    );
                    let painter = ui.painter().with_clip_rect(rect);
                    painter.rect_filled(rect, egui::CornerRadius::same(9), theme::dashed_bg());
                    dashed_rect(&painter, rect, theme::DASHED_BORDER);
                    let cy = rect.center().y;
                    painter.text(
                        egui::pos2(rect.left() + 18.0, cy),
                        egui::Align2::LEFT_CENTER,
                        "◇",
                        egui::FontId::proportional(28.0),
                        Color32::from_rgb(0x7e, 0xa4, 0xba),
                    );
                    let text_x = rect.left() + 58.0;
                    painter.text(
                        egui::pos2(text_x, cy - 11.0),
                        egui::Align2::LEFT_CENTER,
                        "暂无游戏实例",
                        egui::FontId::proportional(14.0),
                        theme::HEADING,
                    );
                    painter.text(
                        egui::pos2(text_x, cy + 12.0),
                        egui::Align2::LEFT_CENTER,
                        "原版 · Forge · Fabric · NeoForge",
                        egui::FontId::proportional(12.0),
                        theme::MUTED,
                    );
                    ui.add_space(18.0);
                    if app::primary_button(ui, "＋  安装 Minecraft     →") {
                        app.page = Page::Downloads;
                    }
                    ui.add_space(6.0);
                    app::hint(ui, "安装与启动服务正在开发中");
                });
        });
    };

    if wide {
        // 原设计为 1.85fr / 1fr 双栏，右侧约占 33%，栏间距 22px
        let width = (ui.available_width() * 0.33).clamp(300.0, 410.0);
        egui::Panel::right("home-aside")
            .exact_size(width + 22.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::NONE.inner_margin(egui::Margin {
                left: 22,
                right: 0,
                top: 0,
                bottom: 0,
            }))
            .show(ui, |ui| aside(ui, app));
        launch(ui, app);
    } else {
        launch(ui, app);
        ui.add_space(22.0);
        aside(ui, app);
    }
}

// 径向光晕：从外到内逐层叠加圆形近似渐变
fn radial_glow(
    painter: &egui::Painter,
    center: egui::Pos2,
    radius: f32,
    rgb: (u8, u8, u8),
    peak: f32,
) {
    let steps = 26;
    for i in 0..steps {
        let t = i as f32 / steps as f32;
        let r = radius * (1.0 - t * 0.94);
        let alpha = (peak * t * t * 255.0).min(255.0) as u8;
        painter.circle_filled(
            center,
            r,
            Color32::from_rgba_unmultiplied(rgb.0, rgb.1, rgb.2, alpha),
        );
    }
}

// 虚线矩形（近似原 CSS dashed 边框）
fn dashed_rect(painter: &egui::Painter, rect: egui::Rect, color: Color32) {
    let (dash, gap) = (7.0, 5.0);
    let stroke = egui::Stroke::new(1.0, color);
    for (y, x0, x1) in [
        (rect.top(), rect.left(), rect.right()),
        (rect.bottom(), rect.left(), rect.right()),
    ] {
        let mut x = x0;
        while x < x1 {
            let end = (x + dash).min(x1);
            painter.line_segment([egui::pos2(x, y), egui::pos2(end, y)], stroke);
            x = end + gap;
        }
    }
    for (x, y0, y1) in [
        (rect.left(), rect.top(), rect.bottom()),
        (rect.right(), rect.top(), rect.bottom()),
    ] {
        let mut y = y0;
        while y < y1 {
            let end = (y + dash).min(y1);
            painter.line_segment([egui::pos2(x, y), egui::pos2(x, end)], stroke);
            y = end + gap;
        }
    }
}

fn landscape(ui: &mut Ui) {
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 190.0), egui::Sense::hover());
    let painter = ui.painter().with_clip_rect(rect);

    // 140° 对角渐变 #142D40 → #214A55，密集竖条 + 每条随机微扰消除条带感
    let from = (0x14u8, 0x2d, 0x40);
    let to = (0x21, 0x4a, 0x55);
    let strips = 192u32;
    let mut noise = 0x2f5c_u32;
    for i in 0..strips {
        noise = noise.wrapping_mul(1664525).wrapping_add(1013904223);
        let jitter = ((noise >> 24) as i32 - 128) as f32 * 0.35;
        let t = i as f32 / strips as f32;
        let x0 = egui::lerp(rect.left()..=rect.right(), t);
        let x1 = egui::lerp(rect.left()..=rect.right(), (i + 1) as f32 / strips as f32);
        let mix = |a: u8, b: u8| (a as f32 * (1.0 - t) + b as f32 * t + jitter) as u8;
        painter.rect_filled(
            egui::Rect::from_min_max(egui::pos2(x0, rect.top()), egui::pos2(x1, rect.bottom())),
            egui::CornerRadius::same(0),
            Color32::from_rgb(mix(from.0, to.0), mix(from.1, to.1), mix(from.2, to.2)),
        );
    }
    // 天空径向光晕：72% 15% 处的椭圆青光
    radial_glow(
        &painter,
        egui::pos2(
            rect.left() + rect.width() * 0.72,
            rect.top() + rect.height() * 0.15,
        ),
        rect.width() * 0.42,
        (0x3c, 0x77, 0x7a),
        0.45,
    );

    // 原设计的折线山体剪影（clip-path 坐标），凸填充用底角扇形三角分解
    mountain(&painter, rect, theme::MOUNTAIN_BACK, 1.26, 0.0, 5.0);
    mountain(&painter, rect, theme::MOUNTAIN_FRONT, 1.3, -0.12, 60.0);

    // 北极星：右侧 24%、顶部 22px，带集中青绿光晕
    let star = egui::pos2(rect.left() + rect.width() * 0.76, rect.top() + 38.0);
    radial_glow(&painter, star, 26.0, (0x77, 0xe6, 0xc8), 0.55);
    painter.text(
        star,
        egui::Align2::CENTER_CENTER,
        "✦",
        egui::FontId::proportional(32.0),
        theme::STAR,
    );

    painter.text(
        egui::pos2(rect.left() + 30.0, rect.bottom() - 24.0),
        egui::Align2::LEFT_CENTER,
        "FIND YOUR NORTH",
        egui::FontId::proportional(11.0),
        Color32::from_rgb(0xb0, 0xdb, 0xdf),
    );
}

fn mountain(
    painter: &egui::Painter,
    rect: egui::Rect,
    color: Color32,
    scale: f32,
    x_shift: f32,
    sink: f32,
) {
    // 原剪影归一化坐标：polygon(0 80%, 18% 26%, 28% 50%, 48% 0, 65% 56%, 78% 30%, 100% 90%)
    let silhouette = [
        (0.0f32, 0.80f32),
        (0.18, 0.26),
        (0.28, 0.50),
        (0.48, 0.00),
        (0.65, 0.56),
        (0.78, 0.30),
        (1.00, 0.90),
        (1.00, 1.00),
    ];
    let height = 140.0 * scale;
    let width = rect.width() * scale;
    let bottom = rect.bottom() + sink;
    let top = bottom - height;
    let left = rect.left() + x_shift * rect.width() + (rect.width() - width) * 0.5;
    let point = |p: (f32, f32)| egui::pos2(left + p.0 * width, top + p.1 * height);
    let apex = egui::pos2(left, bottom);
    let points: Vec<egui::Pos2> = silhouette.iter().map(|&p| point(p)).collect();
    for pair in points.windows(2) {
        painter.add(egui::Shape::convex_polygon(
            vec![apex, pair[0], pair[1]],
            color,
            egui::Stroke::NONE,
        ));
    }
}

pub fn account(ui: &mut Ui) {
    theme::card(32).show(ui, |ui| {
        ui.set_min_height(450.0);
        ui.add_space(24.0);
        app::eyebrow(ui, "ACCOUNT & SKIN");
        ui.add_space(8.0);
        app::heading(ui, "让冒险拥有你的名字");
        app::body(
            ui,
            "Microsoft 账号登录、皮肤上传与 3D 预览将在后续版本提供。",
        );
        ui.add_space(26.0);
        empty_state(
            ui,
            "◎",
            "还没有添加账号",
            "认证服务尚未接入，当前不会收集账号或密码。",
            Some(("Microsoft 登录 · 开发中", false)),
        );
    });
}

pub fn downloads(ui: &mut Ui, app: &mut NsclApp) {
    const CATEGORIES: [&str; 4] = ["原版", "加载器", "整合包", "Mod"];
    const DESCRIPTIONS: [&str; 4] = [
        "选择 Minecraft 版本，开启新的冒险。",
        "为你的实例安装 Forge、Fabric 或 NeoForge。",
        "发现组合好的模组与全新玩法。",
        "为现有实例添加更多可能。",
    ];
    theme::card(32).show(ui, |ui| {
        ui.set_min_height(450.0);
        ui.add_space(18.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);
            for (index, item) in CATEGORIES.iter().enumerate() {
                let selected = app.category == index;
                let fill = if selected {
                    theme::ACCENT
                } else {
                    Color32::TRANSPARENT
                };
                let text = RichText::new(*item).size(13.0).color(if selected {
                    theme::ON_ACCENT
                } else {
                    Color32::from_rgb(0xa7, 0xb5, 0xc6)
                });
                let response = app::styled_button(
                    ui,
                    text,
                    &app::ButtonStyle {
                        fill,
                        radius: 7.0,
                        min_size: Vec2::new(84.0, 36.0),
                        ..Default::default()
                    },
                );
                if response.clicked() {
                    app.category = index;
                }
            }
        });
        ui.add_space(18.0);
        let sep = ui.available_rect_before_wrap();
        ui.painter().line_segment(
            [sep.left_top(), egui::pos2(sep.right(), sep.top())],
            egui::Stroke::new(1.0, theme::CARD_BORDER),
        );
        ui.add_space(28.0);
        let category = CATEGORIES[app.category];
        app::heading(
            ui,
            if app.category == 0 {
                "Minecraft Java Edition"
            } else {
                category
            },
        );
        app::body(ui, DESCRIPTIONS[app.category]);
        if app.category == 1 {
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                for loader in ["Forge", "Fabric", "NeoForge"] {
                    egui::Frame::default()
                        .stroke(egui::Stroke::new(1.0, Color32::from_rgb(0x2c, 0x3d, 0x4d)))
                        .corner_radius(7.0)
                        .inner_margin(egui::Margin::symmetric(18, 9))
                        .show(ui, |ui| {
                            ui.label(RichText::new(loader).size(13.0).color(theme::BODY_TEXT))
                        });
                }
            });
        }
        empty_state(
            ui,
            "↓",
            &format!("{category}下载服务尚未接入"),
            "接入资源服务后，这里将显示可用版本和安装进度。",
            None,
        );
        ui.vertical_centered(|ui| app::badge_muted(ui, "暂无下载任务"));
    });
}

pub fn settings(ui: &mut Ui, app: &mut NsclApp) {
    theme::card(30).show(ui, |ui| {
        ui.add_space(16.0);
        app::eyebrow(ui, "PREFERENCES");
        ui.add_space(8.0);
        app::heading(ui, "为你的冒险做好准备");
        app::body(ui, "设置保存在本机，当前不会执行 Java 或启动游戏。");
        ui.add_space(22.0);
        egui::Frame::NONE.show(ui, |ui| {
            ui.set_max_width(560.0);
            ui.label(
                RichText::new("Java 可执行文件路径")
                    .size(13.0)
                    .color(theme::BODY_TEXT),
            );
            // 参照设计的输入框更高（约 44px）
            ui.spacing_mut().interact_size.y = 22.0;
            ui.add(
                egui::TextEdit::singleline(&mut app.settings.java_path)
                    .hint_text("例如 C:\\Program Files\\Java\\bin\\javaw.exe")
                    .desired_width(560.0),
            );
            app::hint(ui, "留空表示自动检测（检测功能待接入）。");
            ui.add_space(14.0);
            ui.label(
                RichText::new(format!("游戏内存：{} MiB", app.settings.memory))
                    .size(13.0)
                    .color(theme::BODY_TEXT),
            );
            let slider = egui::Slider::new(&mut app.settings.memory, 1024..=32768)
                .step_by(64.0)
                .show_value(false);
            if ui.add(slider).changed() {
                app.save_status.clear();
            }
            app::hint(ui, "范围 1024–32768 MiB，请根据设备可用内存配置。");
            ui.add_space(18.0);
            if app::primary_button(ui, "保存设置") {
                if !(1024..=32768).contains(&app.settings.memory) {
                    app.save_status = "内存必须为 1024–32768 MiB 之间的整数。".into();
                } else {
                    app.settings.java_path = app.settings.java_path.trim().to_string();
                    app.save_status = match crate::store::save(&app.settings) {
                        Ok(()) => "设置已保存在本机；游戏启动功能接入后生效。".into(),
                        Err(e) => e,
                    };
                }
            }
            ui.add_space(4.0);
            if !app.save_status.is_empty() {
                ui.label(
                    RichText::new(&app.save_status)
                        .size(12.0)
                        .color(theme::MUTED),
                );
            }
        });
    });
}

pub fn about(ui: &mut Ui) {
    theme::card(30).show(ui, |ui| {
        ui.add_space(26.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("✦").size(50.0).color(theme::ACCENT));
            ui.add_space(8.0);
            ui.label(RichText::new("NorthStar Minecraft Launcher").size(21.0).strong().color(theme::HEADING));
            ui.label(RichText::new(format!("NSCL · v{} · 开发预览", env!("CARGO_PKG_VERSION"))).size(12.0).color(theme::MUTED));
            ui.label(RichText::new("一个以 Rust 为核心的 Minecraft Java Edition 启动器。").size(13.0).color(theme::MUTED));
        });
        ui.add_space(24.0);
        ui.separator();
        ui.add_space(16.0);
        ui.label(RichText::new("本版本").size(14.0).strong().color(theme::HEADING));
        ui.label(RichText::new("已提供导航、页面框架和本地设置保存。游戏安装与启动、账号认证、模组下载及皮肤渲染尚未实现。").size(13.0).color(theme::MUTED));
        ui.add_space(12.0);
        ui.label(RichText::new("项目声明").size(14.0).strong().color(theme::HEADING));
        ui.label(RichText::new("本项目与 Mojang Studios 或 Microsoft 无隶属或背书关系。").size(13.0).color(theme::MUTED));
    });
}

fn empty_state(
    ui: &mut Ui,
    glyph: &str,
    title: &str,
    description: &str,
    action: Option<(&str, bool)>,
) {
    ui.vertical_centered(|ui| {
        ui.add_space(22.0);
        ui.label(
            RichText::new(glyph)
                .size(36.0)
                .color(Color32::from_rgb(0x71, 0xb9, 0xbc)),
        );
        ui.add_space(8.0);
        ui.label(
            RichText::new(title)
                .size(17.0)
                .strong()
                .color(theme::HEADING),
        );
        ui.label(RichText::new(description).size(14.0).color(theme::MUTED));
        if let Some((label, enabled)) = action {
            ui.add_space(10.0);
            app::secondary_button(ui, label, enabled);
        }
        ui.add_space(18.0);
    });
}
