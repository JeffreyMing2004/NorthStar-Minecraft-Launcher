use crate::{
    app::{self, NsclApp},
    pages::download_button,
    theme,
};
use nscl::{
    accounts::Account,
    multiplayer::{RelayClient, RelaySession},
};
use std::sync::mpsc::{self, Receiver};

#[derive(Default)]
pub struct MultiplayerPage {
    token: String,
    game_port: String,
    room_code: String,
    message: String,
    identity: Option<String>,
    pending: Option<Receiver<Result<RelaySession, String>>>,
    session: Option<RelaySession>,
}

impl MultiplayerPage {
    pub fn poll(&mut self, account: Option<&Account>) {
        let current = account.map(|a| a.id.clone());
        if self.identity != current {
            self.session = None;
            self.pending = None;
            self.token.clear();
            self.message.clear();
            self.identity = current;
        }
        if let Some(receiver) = &self.pending {
            match receiver.try_recv() {
                Ok(result) => {
                    self.pending = None;
                    match result {
                        Ok(session) => {
                            self.message = "联机连接已就绪。".into();
                            self.session = Some(session);
                        }
                        Err(error) => self.message = error,
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending = None;
                    self.message = "联机任务已结束，请重试。".into();
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
    }

    fn start(&mut self, endpoint: String, account: Account, hosting: bool, ctx: &egui::Context) {
        if self.pending.is_some() || self.session.is_some() {
            return;
        }
        let client = match RelayClient::new(&endpoint, self.token.trim()) {
            Ok(client) => client,
            Err(error) => {
                self.message = error;
                return;
            }
        };
        let port = if hosting {
            match self.game_port.trim().parse::<u16>() {
                Ok(port) if port > 0 => port,
                _ => {
                    self.message = "请填写游戏内显示的局域网端口（1–65535）。".into();
                    return;
                }
            }
        } else {
            0
        };
        let (sender, receiver) = mpsc::channel();
        self.pending = Some(receiver);
        self.message = "正在验证联机权限并连接房间……".into();
        let (room, ctx) = (self.room_code.clone(), ctx.clone());
        std::thread::spawn(move || {
            let result = if hosting {
                client.host(&account, port)
            } else {
                client.join(&account, &room)
            };
            let _ = sender.send(result);
            ctx.request_repaint();
        });
    }
}

pub fn show(ui: &mut egui::Ui, app: &mut NsclApp) {
    let account = app.accounts.list.current().cloned();
    app.multiplayer.poll(account.as_ref());
    app::heading(ui, "联机");
    ui.add_space(8.0);
    app::body(ui, "通过 88frp 联机网关，与白名单中的朋友一起游玩。");
    app::hint(
        ui,
        "服务仅向管理员登记的最多 30 个正版或离线账号开放，创建和加入都需要独立联机凭证。",
    );
    ui.add_space(18.0);
    let busy = app.multiplayer.pending.is_some() || app.multiplayer.session.is_some();
    theme::card(18).show(ui, |ui| {
        ui.set_width((ui.available_width() - 4.0).max(0.0));
        ui.spacing_mut().item_spacing.y = 10.0;
        app::body(ui, &account.as_ref().map(|a| format!("当前账号：{}", a.name)).unwrap_or_else(|| "请先在账号页选择正版或离线账号。".into()));
        ui.add_enabled_ui(!busy, |ui| {
            ui.label("联机服务地址");
            ui.add(egui::TextEdit::singleline(&mut app.settings.multiplayer_endpoint)
                .hint_text("管理员提供的 https://域名:端口")
                .desired_width(ui.available_width()));
            ui.label("此账号的联机凭证");
            ui.add(egui::TextEdit::singleline(&mut app.multiplayer.token).password(true)
                .hint_text("由联机服务管理员提供")
                .desired_width(ui.available_width()));
            app::hint(ui, "联机凭证仅保留在本次运行内，不会保存到设置或导出包。");
            theme::toolbar(ui, |ui| {
                if download_button(ui, "保存服务地址", true) {
                    app.multiplayer.message = match crate::store::save(&app.settings) {
                        Ok(()) => "联机服务地址已保存。".into(), Err(error) => error,
                    };
                }
                if download_button(ui, "88frp 帮助", true)
                    && webbrowser::open("https://88frp.com/client/help-support/FRPC%20%E9%85%8D%E7%BD%AE%E6%8C%87%E5%8D%97").is_err()
                {app.multiplayer.message = "无法打开 88frp 帮助页面。".into();}
            });
        });
    });
    ui.add_space(16.0);
    if let Some(session) = &app.multiplayer.session {
        theme::card(18).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 10.0;
            app::body(ui, if session.hosting {"我的房间"} else {"已加入房间"});
            app::body(ui, &format!("房间码：{}", session.room));
            if let Some(address) = session.local_address {
                app::body(ui, &format!("游戏连接地址：{address}"));
                app::hint(ui, "在 Minecraft「多人游戏 → 直接连接」中输入此地址，并保持启动器开启。");
            } else {
                app::hint(ui, "将房间码分享给白名单好友，对方在启动器中加入。房主需保持游戏、局域网世界和启动器开启。");
            }
            app::hint(ui, &format!("当前数据连接：{}", session.connections()));
            let message = session.message();
            if !message.is_empty() {app::hint(ui, &message);}
            if !session.running() {app::hint(ui, "联机已停止，请关闭后重新连接。");}
            theme::toolbar(ui, |ui| {
                if download_button(ui, "复制房间码", true) {ui.ctx().copy_text(session.room.clone());}
                if let Some(address) = session.local_address
                    && download_button(ui, "复制连接地址", true)
                {ui.ctx().copy_text(address.to_string());}
            });
        });
        ui.add_space(12.0);
        if download_button(ui, "断开联机", true) {
            app.multiplayer.session = None;
            app.multiplayer.message = "已断开联机。".into();
        }
    } else {
        theme::card(18).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 10.0;
            ui.add_enabled_ui(!busy && account.is_some(), |ui| {
                app::body(ui, "创建房间");
                app::hint(ui, "先启动游戏并进入世界，选择「对局域网开放」，填入游戏显示的端口。好友需使用兼容的游戏版本和模组。");
                ui.horizontal(|ui| {
                    ui.label("游戏局域网端口");
                    ui.add(egui::TextEdit::singleline(&mut app.multiplayer.game_port).hint_text("例如 51234").desired_width(130.0));
                });
                if app::primary_button(ui, "创建联机房间") {
                    app.multiplayer.start(app.settings.multiplayer_endpoint.clone(), account.clone().unwrap(), true, ui.ctx());
                }
                ui.add_space(14.0);
                app::body(ui, "加入房间");
                ui.add(egui::TextEdit::singleline(&mut app.multiplayer.room_code).hint_text("粘贴朋友分享的房间码").desired_width(ui.available_width()));
                if download_button(ui, "加入联机房间", true) {
                    app.multiplayer.start(app.settings.multiplayer_endpoint.clone(), account.clone().unwrap(), false, ui.ctx());
                }
            });
        });
    }
    if !app.multiplayer.message.is_empty() {
        ui.add_space(12.0);
        app::hint(ui, &app.multiplayer.message);
    }
    if busy {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(250));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Page;

    #[test]
    fn multiplayer_page_is_available_and_account_changes_clear_credentials() {
        assert!(Page::ALL.contains(&Page::Multiplayer));
        let alice = Account::offline("Alice").unwrap();
        let bob = Account::offline("Bob").unwrap();
        let mut page = MultiplayerPage::default();
        page.poll(Some(&alice));
        page.token = "a".repeat(64);
        page.message = "old account".into();
        page.poll(Some(&alice));
        assert_eq!(page.token.len(), 64);
        page.poll(Some(&bob));
        assert!(page.token.is_empty());
        assert!(page.message.is_empty());
        page.token = "b".repeat(64);
        page.poll(None);
        assert!(page.token.is_empty());
    }

    #[test]
    fn multiplayer_page_renders_in_the_minimum_window_and_does_not_persist_credentials() {
        let (_root, mut app, ctx) = crate::service::tests::fixture();
        app.page = Page::Multiplayer;
        let alice = Account::offline("Alice").unwrap();
        app.accounts.list.accounts = vec![alice.clone()];
        app.accounts.list.selected = Some(alice.id);
        app.multiplayer.poll(app.accounts.list.current());
        app.multiplayer.token = "a".repeat(64);
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(960.0, 640.0),
                )),
                ..Default::default()
            },
            |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| show(ui, &mut app));
            },
        );
        for label in [
            "联机",
            "当前账号：Alice",
            "联机服务地址",
            "此账号的联机凭证",
            "创建联机房间",
            "加入联机房间",
        ] {
            crate::service::tests::text_rect(&output, label);
        }
        assert!(
            !serde_json::to_string(&app.settings)
                .unwrap()
                .contains(&"a".repeat(64))
        );
        output.drop_without_applying_deltas();
    }
}
