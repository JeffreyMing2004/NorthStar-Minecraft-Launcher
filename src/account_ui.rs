use crate::{
    app::{self, NsclApp},
    pages::download_button,
    theme,
};
use egui::{RichText, Ui, Vec2};

pub fn show(ui: &mut Ui, app: &mut NsclApp) {
    theme::toolbar(ui, |ui| {
        ui.spacing_mut().item_spacing.x = theme::TAB_GAP;
        for (index, label) in ["账号管理", "皮肤与预览"].iter().enumerate() {
            if app::tab_button(ui, label, app.account_tab == index as u8) {
                app.account_tab = index as u8;
            }
        }
    });
    ui.add_space(20.0);
    if app.account_tab == 1 {
        panel(ui, |ui| crate::skin_ui::controls(ui, app));
        return;
    }
    if ui.available_width() >= 1000.0 {
        ui.spacing_mut().item_spacing.x = 20.0;
        ui.columns(2, |columns| {
            panel(&mut columns[0], |ui| login(ui, app));
            panel(&mut columns[1], |ui| saved_accounts(ui, app));
        });
    } else {
        panel(ui, |ui| login(ui, app));
        ui.add_space(20.0);
        panel(ui, |ui| saved_accounts(ui, app));
    }
}

fn panel(ui: &mut Ui, content: impl FnOnce(&mut Ui)) {
    let width = (ui.available_width() - 50.0).max(0.0);
    theme::card(24).show(ui, |ui| {
        ui.set_width(width);
        ui.spacing_mut().item_spacing.y = 0.0;
        content(ui);
    });
}

fn saved_accounts(ui: &mut Ui, app: &mut NsclApp) {
    theme::section_heading(ui, "已保存的账号", theme::Icon::User);
    let editable = !app.accounts.pending && !app.service.busy() && app.service.game.is_none();
    if app.accounts.list.accounts.is_empty() {
        ui.add_space(28.0);
        ui.vertical_centered(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(64.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 20.0, theme::SECONDARY_BG);
            theme::icon(
                ui.painter(),
                rect.shrink(17.0),
                theme::Icon::User,
                theme::MUTED,
            );
            ui.add_space(18.0);
            ui.label(RichText::new("还没有账号").size(16.0).color(theme::HEADING));
            ui.add_space(8.0);
            app::hint(ui, "添加账号后，在这里切换你的游戏身份。");
        });
        ui.add_space(46.0);
        return;
    }
    let mut select = None;
    let mut remove = None;
    for account in app.accounts.list.accounts.clone() {
        egui::Frame::NONE
            .fill(theme::INPUT_BG)
            .corner_radius(12.0)
            .inner_margin(18)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(&account.name)
                                .size(17.0)
                                .family(theme::bold_family()),
                        )
                        .truncate(),
                    );
                    if app.accounts.list.selected.as_deref() == Some(&account.id) {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            app::badge_muted(ui, "当前账号")
                        });
                    }
                });
                ui.add_space(6.0);
                app::hint(
                    ui,
                    if account.is_online() {
                        "Microsoft 正版账号"
                    } else {
                        "离线账号"
                    },
                );
                ui.add_space(16.0);
                theme::toolbar(ui, |ui| {
                    if download_button(ui, "使用此账号", editable) {
                        select = Some(account.id.clone());
                    }
                    if download_button(ui, "移除", editable) {
                        remove = Some(account.id.clone());
                    }
                });
                ui.add_space(10.0);
                egui::CollapsingHeader::new("账号信息")
                    .id_salt(&account.id)
                    .show(ui, |ui| {
                        app::hint(ui, &format!("UUID：{}", account.id));
                        app::hint(
                            ui,
                            if account.is_online() {
                                "启动时在线验证身份。"
                            } else {
                                "使用此玩家名和固定离线 UUID 启动。"
                            },
                        );
                    });
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
    ui.add_space(8.0);
    app::hint(ui, "正版验证失败时不会自动切换为离线身份。");
}

fn login(ui: &mut Ui, app: &mut NsclApp) {
    theme::section_heading(ui, "添加账号", theme::Icon::User);
    app::body(ui, "登录 Microsoft 正版账号，或创建独立的离线账号。");
    ui.add_space(24.0);
    let editable = !app.accounts.pending && !app.service.busy() && app.service.game.is_none();
    ui.add_enabled_ui(editable, |ui| {
        if app::primary_button(ui, "登录 Microsoft 账号  ↗") {
            app.accounts.login(
                &app.settings.microsoft_client_id,
                &app.settings.microsoft_redirect_uri,
                ui.ctx(),
            );
        }
        ui.add_space(theme::CONTROL_GAP);
        egui::CollapsingHeader::new("其他登录方式").show(ui, |ui| {
            if ui.button("设备代码登录（需应用启用对应流程）").clicked() {
                app.accounts
                    .login_device(&app.settings.microsoft_client_id, ui.ctx());
            }
        });
        ui.add_space(16.0);
        let mut create = false;
        theme::toolbar(ui, |ui| {
            let name = ui.add(
                egui::TextEdit::singleline(&mut app.offline_name)
                    .hint_text("离线账号玩家名")
                    .desired_width(260.0),
            );
            create = ui.button("创建离线账号").clicked()
                || (name.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
        });
        ui.add_space(8.0);
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
                    theme::toolbar(ui, |ui| {
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
                    theme::toolbar(ui, |ui| {
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
                    ui.add_space(10.0);
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
}
