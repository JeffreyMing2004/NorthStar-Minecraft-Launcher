use crate::app::{self, NsclApp, Page};
use crate::theme;
use egui::{Color32, RichText, Ui, Vec2};

pub fn home(ui: &mut Ui, app: &mut NsclApp) {
    let wide = ui.available_width() >= 980.0;
    let aside = |ui: &mut Ui, app: &mut NsclApp| {
        theme::card(24).show(ui, |ui| {
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
            ui.add_space(16.0);
            let (rect, _) = ui
                .allocate_exact_size(Vec2::new(ui.available_width(), 250.0), egui::Sense::hover());
            let painter = ui.painter().with_clip_rect(rect);
            painter.circle_filled(
                rect.center(),
                92.0,
                Color32::from_rgba_unmultiplied(0x35, 0x59, 0x66, 0x30),
            );
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "◇",
                egui::FontId::proportional(66.0),
                Color32::from_rgb(0x54, 0x7b, 0x88),
            );
            ui.vertical_centered(|ui| {
                ui.add_space(rect.height() as i32 as f32 * 0.28);
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
            ui.add_space(16.0);
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
                    left: 28,
                    right: 28,
                    top: 24,
                    bottom: 26,
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
                            .size(13.0)
                            .color(theme::MUTED),
                    );
                    ui.add_space(20.0);
                    egui::Frame::default()
                        .stroke(egui::Stroke::new(1.0, theme::DASHED_BORDER))
                        .fill(theme::dashed_bg())
                        .corner_radius(9.0)
                        .inner_margin(egui::Margin::symmetric(18, 15))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("◇")
                                        .size(24.0)
                                        .color(Color32::from_rgb(0x7e, 0xa4, 0xba)),
                                );
                                ui.add_space(8.0);
                                ui.vertical(|ui| {
                                    ui.label(
                                        RichText::new("暂无游戏实例")
                                            .size(14.0)
                                            .strong()
                                            .color(theme::HEADING),
                                    );
                                    ui.label(
                                        RichText::new("原版 · Forge · Fabric · NeoForge")
                                            .size(12.0)
                                            .color(theme::MUTED),
                                    );
                                });
                            });
                        });
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
        let width = (ui.available_width() * 0.26).clamp(300.0, 380.0);
        egui::Panel::right("home-aside")
            .exact_size(width)
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::NONE)
            .show(ui, |ui| aside(ui, app));
        launch(ui, app);
    } else {
        launch(ui, app);
        ui.add_space(16.0);
        aside(ui, app);
    }
}

fn landscape(ui: &mut Ui) {
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 180.0), egui::Sense::hover());
    let painter = ui.painter().with_clip_rect(rect);
    painter.rect_filled(rect, egui::CornerRadius::same(0), theme::LANDSCAPE_BASE);
    let glow = egui::pos2(rect.right() - rect.width() * 0.22, rect.top() + 40.0);
    painter.circle_filled(
        glow,
        90.0,
        Color32::from_rgba_unmultiplied(0x3c, 0x77, 0x7a, 0x45),
    );
    painter.circle_filled(
        glow,
        32.0,
        Color32::from_rgba_unmultiplied(0x77, 0xe6, 0xc8, 0x38),
    );
    // 填充仅支持凸多边形，山体剪影用两组三角形叠加表现
    let peak = |x: f32, y: f32, half: f32| {
        vec![
            egui::pos2(rect.left() + rect.width() * (x - half), rect.bottom() + 2.0),
            egui::pos2(
                rect.left() + rect.width() * x,
                rect.top() + rect.height() * y,
            ),
            egui::pos2(rect.left() + rect.width() * (x + half), rect.bottom() + 2.0),
        ]
    };
    for (x, y, half) in [
        (0.16, 0.62, 0.20),
        (0.44, 0.16, 0.24),
        (0.72, 0.50, 0.18),
        (0.97, 0.32, 0.22),
    ] {
        painter.add(egui::Shape::convex_polygon(
            peak(x, y, half),
            theme::MOUNTAIN_BACK,
            egui::Stroke::NONE,
        ));
    }
    for (x, y, half) in [
        (0.06, 0.58, 0.22),
        (0.36, 0.46, 0.24),
        (0.64, 0.72, 0.20),
        (0.90, 0.60, 0.24),
    ] {
        painter.add(egui::Shape::convex_polygon(
            peak(x, y, half),
            theme::MOUNTAIN_FRONT,
            egui::Stroke::NONE,
        ));
    }
    painter.text(
        egui::pos2(rect.left() + 28.0, rect.bottom() - 24.0),
        egui::Align2::LEFT_CENTER,
        "FIND YOUR NORTH",
        egui::FontId::proportional(11.0),
        Color32::from_rgb(0xb0, 0xdb, 0xdf),
    );
    painter.text(
        egui::pos2(rect.left() + rect.width() * 0.40, rect.top() + 34.0),
        egui::Align2::RIGHT_TOP,
        "✦",
        egui::FontId::proportional(32.0),
        theme::STAR,
    );
}

pub fn account(ui: &mut Ui) {
    theme::card(30).show(ui, |ui| {
        ui.set_min_height(440.0);
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
    theme::card(30).show(ui, |ui| {
        ui.set_min_height(440.0);
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
        ui.add_space(14.0);
        ui.separator();
        ui.add_space(16.0);
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
            ui.label(RichText::new("✦").size(44.0).color(theme::ACCENT));
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
        ui.label(RichText::new(description).size(12.0).color(theme::MUTED));
        if let Some((label, enabled)) = action {
            ui.add_space(10.0);
            app::secondary_button(ui, label, enabled);
        }
        ui.add_space(18.0);
    });
}
