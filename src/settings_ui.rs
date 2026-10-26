use crate::{
    app::{self, NsclApp},
    pages::{download_button, input_focus, input_frame, task_status},
    theme,
};
use egui::{FontId, RichText, Ui, Vec2};

pub fn show(ui: &mut Ui, app: &mut NsclApp) {
    if app.service.busy() {
        theme::card(24).show(ui, |ui| {
            app::hint(ui, "后台任务进行中，完成或取消后可修改设置。");
            task_status(ui, app);
        });
        return;
    }
    ui.horizontal(|ui| {
        let width = (ui.available_width() - 124.0).max(0.0);
        ui.allocate_ui_with_layout(
            Vec2::new(width, 44.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_min_width(width);
                ui.add(
                    egui::Label::new(
                        RichText::new("修改后保存，应用到下一次启动。")
                            .size(13.0)
                            .color(theme::MUTED),
                    )
                    .truncate(),
                );
            },
        );
        if app::primary_button(ui, "保存设置") {
            save(app);
        }
    });
    if !app.save_status.is_empty() {
        ui.add_space(8.0);
        app::body(ui, &app.save_status);
    }
    ui.add_space(18.0);
    egui::ScrollArea::vertical()
        .id_salt("launcher-settings")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(20.0, 0.0);
            if ui.available_width() >= 1040.0 {
                ui.columns(2, |columns| {
                    java(&mut columns[0], app);
                    columns[0].add_space(20.0);
                    directories(&mut columns[0], app);
                    memory(&mut columns[1], app);
                    columns[1].add_space(20.0);
                    network(&mut columns[1], app);
                });
            } else {
                java(ui, app);
                ui.add_space(20.0);
                memory(ui, app);
                ui.add_space(20.0);
                directories(ui, app);
                ui.add_space(20.0);
                network(ui, app);
            }
        });
}

fn section(ui: &mut Ui, title: &str, symbol: theme::Icon, content: impl FnOnce(&mut Ui, f32)) {
    let width = (ui.available_width() - 50.0).max(0.0);
    theme::card(24).show(ui, |ui| {
        ui.set_width(width);
        ui.spacing_mut().item_spacing.y = 0.0;
        theme::section_heading(ui, title, symbol);
        content(ui, width);
    });
}

fn java(ui: &mut Ui, app: &mut NsclApp) {
    section(ui, "Java 运行环境", theme::Icon::Java, |ui, width| {
        ui.checkbox(&mut app.settings.auto_java, "按游戏版本自动选择 Java");
        ui.add_space(10.0);
        if !app.settings.auto_java {
            ui.label("Java 可执行文件路径");
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
        }
        app::hint(
            ui,
            "默认按实例要求自动选择已安装的 Java 主版本和架构；关闭自动选择后可手动指定。",
        );
        ui.add_space(12.0);
        theme::toolbar(ui, |ui| {
            if download_button(ui, "扫描 Java", !app.service.java_busy) {
                app.service.inspect_java(None, ui.ctx());
            }
            if !app.settings.auto_java
                && download_button(
                    ui,
                    "检测当前路径",
                    !app.service.java_busy && !app.settings.java_path.trim().is_empty(),
                )
            {
                app.service
                    .inspect_java(Some(app.settings.java_path.clone()), ui.ctx());
            }
            if !app.settings.auto_java && ui.button("恢复自动匹配").clicked() {
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
                            app.settings.java_path = runtime.path.to_string_lossy().into_owned();
                            app.settings.auto_java = false;
                        }
                    }
                });
        }
        ui.add_space(14.0);
        theme::toolbar(ui, |ui| {
            ui.label("安装");
            egui::ComboBox::from_id_salt("install-java-major")
                .width(88.0)
                .selected_text(format!("Java {}", app.service.java_install_major))
                .show_ui(ui, |ui| {
                    for major in [8, 11, 17, 21, 25] {
                        ui.selectable_value(
                            &mut app.service.java_install_major,
                            major,
                            format!("Java {major}"),
                        );
                    }
                });
            if ui
                .add_enabled(!app.service.java_busy, egui::Button::new("下载并安装"))
                .clicked()
            {
                app.service.install_java(ui.ctx());
            }
            let required = app
                .service
                .instances
                .iter()
                .find(|i| i.id == app.service.selected)
                .and_then(|i| app.service.installed.iter().find(|v| v.id == i.version_id))
                .map(|v| v.java_major);
            if ui
                .add_enabled(required.is_some(), egui::Button::new("按当前实例选择"))
                .clicked()
                && let Some(major) = required
            {
                app.service.java_install_major = major;
            }
        });
        ui.add_space(10.0);
        app::hint(
            ui,
            "下载 Eclipse Temurin JRE，校验后安装到启动器数据目录，启用自动选择后即可匹配。支持 Windows。",
        );
        if !app.service.java_status.is_empty() {
            ui.add_space(10.0);
            app::body(ui, &app.service.java_status);
        }
    });
}

fn memory(ui: &mut Ui, app: &mut NsclApp) {
    section(ui, "游戏内存", theme::Icon::Memory, |ui, width| {
        ui.checkbox(&mut app.settings.auto_memory, "自动分配游戏内存");
        ui.add_space(10.0);
        if !app.settings.auto_memory {
            ui.label("内存上限（MiB）");
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
        }
        if app.settings.auto_memory {
            let version = app
                .service
                .instances
                .iter()
                .find(|i| i.id == app.service.selected)
                .and_then(|i| app.service.installed.iter().find(|v| v.id == i.version_id));
            let (major, modded) = version
                .map(|v| (v.java_major, v.loader.is_some()))
                .unwrap_or((17, false));
            match nscl::memory::automatic(major, modded) {
                Ok(memory) => {
                    ui.label(
                        RichText::new(format!("{memory} MiB"))
                            .size(26.0)
                            .family(theme::bold_family())
                            .color(theme::ACCENT),
                    );
                    ui.add_space(10.0);
                    app::hint(ui, "启动时按可用物理内存重新计算，并为系统保留空间。");
                }
                Err(error) => app::hint(ui, &error),
            }
        } else {
            app::hint(ui, "范围 1024–32768 MiB；实例的单独配置优先于此处设置。");
        }
    });
}

fn directories(ui: &mut Ui, app: &mut NsclApp) {
    section(ui, "游戏目录", theme::Icon::Folder, |ui, width| {
        ui.label("Minecraft 游戏目录");
        ui.add_space(10.0);
        egui::ComboBox::from_id_salt("game-directories")
            .width(width)
            .wrap_mode(egui::TextWrapMode::Truncate)
            .selected_text(if app.settings.game_directory.trim().is_empty() {
                "默认：启动器所在目录"
            } else {
                &app.settings.game_directory
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut app.settings.game_directory,
                    String::new(),
                    "默认：启动器所在目录",
                );
                for path in app.settings.game_directories.clone() {
                    ui.selectable_value(&mut app.settings.game_directory, path.clone(), &path);
                }
            });
        ui.add_space(10.0);
        theme::toolbar(ui, |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut app.new_game_directory)
                    .hint_text("添加 MC 路径，例如 D:\\Minecraft 或已有 .minecraft 目录")
                    .desired_width((width - 124.0).max(180.0)),
            );
            if ui.button("添加路径").clicked() {
                match crate::store::add_game_directory(&mut app.settings, &app.new_game_directory) {
                    Ok(()) => {
                        app.new_game_directory.clear();
                        app.save_status = "目录已添加并选中，保存设置后应用。".into();
                    }
                    Err(error) => app.save_status = error,
                }
            }
        });
        ui.add_space(10.0);
        app::hint(
            ui,
            "默认使用启动器所在目录；目录中已有 .minecraft 文件夹时优先使用它。可直接添加 .minecraft 路径，切换不会移动或删除已有文件。",
        );
        app::hint(
            ui,
            "保存后扫描 versions 目录，自动识别其他启动器安装的已有版本。",
        );
        app::hint(
            ui,
            &format!(
                "当前目录：{}",
                crate::store::game_directory(&app.settings).display()
            ),
        );
        ui.add_space(20.0);
        ui.label("新实例默认隔离模式");
        ui.add_space(10.0);
        egui::ComboBox::from_id_salt("default-isolation")
            .selected_text(app.settings.default_isolation.label())
            .show_ui(ui, |ui| {
                for mode in crate::instances::Isolation::ALL {
                    ui.selectable_value(&mut app.settings.default_isolation, mode, mode.label());
                }
            });
        ui.add_space(10.0);
        app::hint(
            ui,
            "仅用于新安装版本与新建实例。隔离模式分别保存存档、模组、配置和截图；共享模式共用当前游戏根目录。已有实例可在「管理实例 → 配置」修改，整合包始终隔离。",
        );
    });
}

fn network(ui: &mut Ui, app: &mut NsclApp) {
    section(ui, "下载与网络", theme::Icon::Download, |ui, width| {
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
        if app.settings.source.kind == crate::minecraft::SourceKind::Auto {
            app::hint(
                ui,
                "自动模式依次尝试 BMCLAPI（bangbang93）、minguncle 和官方源；连接失败或校验不通过时切换到下一来源。",
            );
        } else {
            ui.checkbox(
                &mut app.settings.source.fallback_to_official,
                "镜像失败或校验不通过时回退到官方源",
            );
        }
        ui.add_space(16.0);
        ui.label("下载连接方式");
        ui.add_space(10.0);
        egui::ComboBox::from_id_salt("download-proxy-mode")
            .width(width)
            .selected_text(app.settings.source.proxy_mode.label())
            .show_ui(ui, |ui| {
                for mode in crate::minecraft::ProxyMode::ALL {
                    ui.selectable_value(&mut app.settings.source.proxy_mode, mode, mode.label());
                }
            });
        ui.add_space(10.0);
        app::hint(
            ui,
            "直连仅影响游戏下载、加载器与 Modrinth 请求，不修改系统代理设置。",
        );
    });
}

fn save(app: &mut NsclApp) {
    let memory = app
        .memory_input
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|m| (1024..=32768).contains(m));
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
        if let Err(error) =
            crate::accounts::validate_redirect_uri(app.settings.microsoft_redirect_uri.trim())
        {
            app.save_status = error;
            return;
        }
        if let Some(memory) = memory {
            app.settings.memory = memory;
        }
        app.settings.java_path = app.settings.java_path.trim().to_owned();
        app.settings.microsoft_client_id = app.settings.microsoft_client_id.trim().to_owned();
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
